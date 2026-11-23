//! Thread scheduling and frame timing.
//!
//! The scheduler owns the DataModel's clock, its queues of pending work
//! (timers for legacy `wait`/`delay`/`spawn` and `task.wait`/`task.delay`,
//! the deferred queue for `task.defer` and Deferred signal handlers) and
//! the execution phase: serial, or parallel for one Actor's work. It
//! doesn't run anything itself. The engine drives the frame and asks for
//! the work due at each resumption point. Jobs are an opaque `T` (for the
//! Lua layer, a thread to resume or a function to call). Work scheduled
//! while parallel comes back parallel, and parallel work is handed out by
//! Actor (docs/behavior/scripting/parallel-luau.md).
//!
//! Frame order and resumption points: `docs/design/architecture/scripting.md#signals-and-scheduler-rook-luas-signal-and-scheduler`.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::time::Instant;

use rook_dom::InstanceId;

/// Which resumption point a timer belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Queue {
    /// `wait`, `delay`, `spawn`: resumed by the legacy step, before simulation.
    Legacy,
    /// `task.wait`, `task.delay`: resumed after Heartbeat.
    Task,
}

/// Shortest legacy `wait` and the default when no time is given. The
/// creator-docs signature gives `wait(seconds: number = 0.03)`; whether
/// smaller values round up to it is Inference.
pub const LEGACY_MIN_WAIT: f64 = 0.03;

/// Timers due within this many seconds of the current time count as due,
/// so fixed-step float rounding never delays one by a whole frame.
const EPSILON: f64 = 1e-9;

/// Which kind of code runs now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Phase {
    /// Synchronized code: anything may run, one thing at a time.
    #[default]
    Serial,
    /// Desynchronized code of this Actor, which may only do what is safe in
    /// parallel.
    Parallel(InstanceId),
}

/// Where time comes from.
#[derive(Debug, Clone, Copy)]
pub enum Clock {
    /// Each frame advances by exactly `step` seconds (headless runs, tests,
    /// conformance), so results are reproducible.
    Fixed {
        step: f64,
    },
    Real {
        start: Instant,
    },
}

#[derive(Debug)]
struct Timer<T> {
    at: f64,
    seq: u64,
    queue: Queue,
    /// The Actor whose parallel phase it resumes in, if scheduled there.
    parallel: Option<InstanceId>,
    job: T,
}

impl<T> PartialEq for Timer<T> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl<T> Eq for Timer<T> {}
impl<T> PartialOrd for Timer<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Timer<T> {
    /// Reversed so `BinaryHeap` pops the earliest timer first.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .at
            .total_cmp(&self.at)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

#[derive(Debug)]
pub struct Scheduler<T> {
    clock: Clock,
    now: f64,
    delta: f64,
    frame: u64,
    seq: u64,
    legacy: BinaryHeap<Timer<T>>,
    task: BinaryHeap<Timer<T>>,
    deferred: VecDeque<T>,
    phase: Phase,
    /// Work for the next parallel phase, by Actor, in the order it came.
    parallel: VecDeque<(InstanceId, T)>,
    /// Desynchronized work that called `task.synchronize`, for the serial
    /// phase after the parallel one.
    synchronized: VecDeque<T>,
}

impl<T> Scheduler<T> {
    pub fn new(clock: Clock) -> Self {
        Self {
            clock,
            now: 0.0,
            delta: 0.0,
            frame: 0,
            seq: 0,
            legacy: BinaryHeap::new(),
            task: BinaryHeap::new(),
            deferred: VecDeque::new(),
            phase: Phase::Serial,
            parallel: VecDeque::new(),
            synchronized: VecDeque::new(),
        }
    }

    pub fn fixed(hz: f64) -> Self {
        Self::new(Clock::Fixed { step: 1.0 / hz })
    }

    pub fn real() -> Self {
        Self::new(Clock::Real {
            start: Instant::now(),
        })
    }

    /// Seconds since the DataModel started (`time()`, `workspace.DistributedGameTime`).
    pub fn now(&self) -> f64 {
        self.now
    }

    pub fn delta(&self) -> f64 {
        self.delta
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Start a frame `dt` seconds after the last, whatever the clock says
    /// (a client stepped once per drawn frame). Returns `dt`.
    pub fn begin_frame_by(&mut self, dt: f64) -> f64 {
        self.delta = dt;
        self.now += dt;
        self.frame += 1;
        dt
    }

    /// Start a new frame, advancing the clock. Returns the frame's delta.
    pub fn begin_frame(&mut self) -> f64 {
        let next = match self.clock {
            Clock::Fixed { step } => self.now + step,
            Clock::Real { start } => start.elapsed().as_secs_f64(),
        };
        self.delta = next - self.now;
        self.now = next;
        self.frame += 1;
        self.delta
    }

    /// Fixed clocks only: jump forward whole frames so that the next frame
    /// is the first one at or after the earliest timer. Lets headless runs
    /// skip idle stretches. Returns whether anything is pending.
    pub fn skip_idle(&mut self) -> bool {
        let Clock::Fixed { step } = self.clock else {
            return self.has_work();
        };
        if !self.deferred.is_empty() || !self.parallel.is_empty() || !self.synchronized.is_empty() {
            return true;
        }
        let Some(next) = self.next_timer() else {
            return false;
        };
        let frames = ((next - self.now) / step - EPSILON).ceil() - 1.0;
        if frames > 0.0 {
            self.now += frames * step;
        }
        true
    }

    fn next_timer(&self) -> Option<f64> {
        let a = self.legacy.peek().map(|t| t.at);
        let b = self.task.peek().map(|t| t.at);
        match (a, b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    pub fn has_work(&self) -> bool {
        !self.deferred.is_empty()
            || !self.legacy.is_empty()
            || !self.task.is_empty()
            || !self.parallel.is_empty()
            || !self.synchronized.is_empty()
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The Actor whose desynchronized code runs now, if any.
    pub fn parallel_actor(&self) -> Option<InstanceId> {
        match self.phase {
            Phase::Serial => None,
            Phase::Parallel(actor) => Some(actor),
        }
    }

    /// Enter `phase`: the engine's parallel phase sets each Actor's in
    /// turn, and serial after.
    pub fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
    }

    /// Run `job` once `delay` seconds have passed, at `queue`'s resumption
    /// point; in the parallel phase after it if scheduled while parallel.
    pub fn schedule(&mut self, queue: Queue, delay: f64, job: T) {
        let at = self.now + delay.max(0.0);
        self.seq += 1;
        let timer = Timer {
            at,
            seq: self.seq,
            queue,
            parallel: self.parallel_actor(),
            job,
        };
        match queue {
            Queue::Legacy => self.legacy.push(timer),
            Queue::Task => self.task.push(timer),
        }
    }

    /// Jobs on `queue` whose time has come, earliest first; ties keep
    /// scheduling order. Desynchronized ones go to the next parallel
    /// phase instead.
    pub fn take_due(&mut self, queue: Queue) -> Vec<T> {
        let heap = match queue {
            Queue::Legacy => &mut self.legacy,
            Queue::Task => &mut self.task,
        };
        let mut out = Vec::new();
        while heap.peek().is_some_and(|t| t.at <= self.now + EPSILON) {
            let t = heap.pop().expect("peeked");
            debug_assert_eq!(t.queue, queue);
            match t.parallel {
                Some(actor) => self.parallel.push_back((actor, t.job)),
                None => out.push(t.job),
            }
        }
        out
    }

    /// `task.defer` and Deferred signal handlers. Deferred while parallel,
    /// it runs later in the same parallel phase.
    pub fn defer(&mut self, job: T) {
        match self.phase {
            Phase::Serial => self.deferred.push_back(job),
            Phase::Parallel(actor) => self.parallel.push_back((actor, job)),
        }
    }

    /// Run `job` at the next serial resumption point, in whatever phase it
    /// was made: a signal's serial handlers, which desynchronized code
    /// firing it mustn't run as its own.
    pub fn defer_serial(&mut self, job: T) {
        self.deferred.push_back(job);
    }

    /// Run `job` in `actor`'s next parallel phase (`task.desynchronize`,
    /// `ConnectParallel` and `BindToMessageParallel` handlers).
    pub fn desynchronize(&mut self, actor: InstanceId, job: T) {
        self.parallel.push_back((actor, job));
    }

    /// Run `job` in the serial phase after this parallel one
    /// (`task.synchronize`).
    pub fn synchronize(&mut self, job: T) {
        self.synchronized.push_back(job);
    }

    /// The work for a parallel phase, grouped by Actor in the order each
    /// Actor's first job came. One Actor's work always runs in order, on
    /// one worker; different Actors' may run at the same time.
    pub fn take_parallel(&mut self) -> Vec<(InstanceId, Vec<T>)> {
        let mut groups: Vec<(InstanceId, Vec<T>)> = Vec::new();
        for (actor, job) in self.parallel.drain(..) {
            match groups.iter_mut().find(|(a, _)| *a == actor) {
                Some((_, jobs)) => jobs.push(job),
                None => groups.push((actor, vec![job])),
            }
        }
        groups
    }

    /// Whether a parallel phase has anything to run or resume.
    pub fn has_parallel_work(&self) -> bool {
        !self.parallel.is_empty() || !self.synchronized.is_empty()
    }

    /// Next job that synchronized during the last parallel phase.
    pub fn pop_synchronized(&mut self) -> Option<T> {
        self.synchronized.pop_front()
    }

    /// Next deferred job. Jobs deferred while draining run in the same
    /// drain, after the ones already queued.
    pub fn pop_deferred(&mut self) -> Option<T> {
        self.deferred.pop_front()
    }

    /// Remove deferred jobs matching `pred`, leaving the timers unlooked
    /// at: for jobs only ever deferred (a Deferred signal's invocations,
    /// dropped when its connection goes), whose cost is then the frame's
    /// deferred queue, not every waiting thread. Returns how many were
    /// removed.
    pub fn cancel_deferred(&mut self, mut pred: impl FnMut(&T) -> bool) -> usize {
        let before = self.deferred.len() + self.parallel.len();
        self.deferred.retain(|j| !pred(j));
        self.parallel.retain(|(_, j)| !pred(j));
        before - (self.deferred.len() + self.parallel.len())
    }

    /// Remove pending jobs matching `pred` (for `task.cancel`). Returns how
    /// many were removed.
    pub fn cancel(&mut self, mut pred: impl FnMut(&T) -> bool) -> usize {
        let pending = |s: &Self| {
            s.legacy.len()
                + s.task.len()
                + s.deferred.len()
                + s.parallel.len()
                + s.synchronized.len()
        };
        let before = pending(self);
        self.legacy.retain(|t| !pred(&t.job));
        self.task.retain(|t| !pred(&t.job));
        self.deferred.retain(|j| !pred(j));
        self.parallel.retain(|(_, j)| !pred(j));
        self.synchronized.retain(|j| !pred(j));
        before - pending(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timers_fire_in_time_order_then_schedule_order() {
        let mut s = Scheduler::fixed(60.0);
        s.schedule(Queue::Task, 0.04, "late");
        s.schedule(Queue::Task, 0.0, "a");
        s.schedule(Queue::Task, 0.0, "b");
        s.schedule(Queue::Legacy, 0.0, "legacy");
        s.begin_frame();
        assert_eq!(s.take_due(Queue::Task), ["a", "b"]);
        assert_eq!(s.take_due(Queue::Legacy), ["legacy"]);
        s.begin_frame();
        assert!(s.take_due(Queue::Task).is_empty());
        s.begin_frame();
        assert_eq!(s.take_due(Queue::Task), ["late"]);
    }

    #[test]
    fn skip_idle_lands_just_before_the_next_timer() {
        let mut s = Scheduler::fixed(60.0);
        s.schedule(Queue::Legacy, 10.0, ());
        assert!(s.skip_idle());
        s.begin_frame();
        assert_eq!(s.take_due(Queue::Legacy).len(), 1);
        assert!((s.now() - 10.0).abs() < 1e-6, "{}", s.now());
        assert!(!s.skip_idle());
    }

    #[test]
    fn deferred_is_fifo_and_cancellable() {
        let mut s = Scheduler::fixed(60.0);
        s.defer(1);
        s.defer(2);
        s.schedule(Queue::Task, 1.0, 3);
        assert_eq!(s.cancel(|j| *j == 2 || *j == 3), 2);
        assert_eq!(s.pop_deferred(), Some(1));
        assert_eq!(s.pop_deferred(), None);
    }

    #[test]
    fn work_made_in_parallel_comes_back_in_parallel() {
        let mut dom = rook_dom::Dom::new();
        let (a, b) = (
            dom.create_by_name("Actor").unwrap(),
            dom.create_by_name("Actor").unwrap(),
        );
        let mut s = Scheduler::fixed(60.0);
        s.desynchronize(a, "a1");
        s.desynchronize(b, "b1");
        s.desynchronize(a, "a2");
        let groups = s.take_parallel();
        assert_eq!(groups, [(a, vec!["a1", "a2"]), (b, vec!["b1"])]);
        s.set_phase(Phase::Parallel(a));
        s.defer("a deferred");
        s.schedule(Queue::Task, 0.0, "a waited");
        s.synchronize("a synchronized");
        s.set_phase(Phase::Serial);
        assert_eq!(s.pop_deferred(), None);
        assert_eq!(s.pop_synchronized(), Some("a synchronized"));
        assert_eq!(s.take_parallel(), [(a, vec!["a deferred"])]);
        s.begin_frame();
        assert!(s.take_due(Queue::Task).is_empty());
        assert_eq!(s.take_parallel(), [(a, vec!["a waited"])]);
    }

    #[test]
    fn cancel_deferred_leaves_the_timers() {
        let mut s = Scheduler::fixed(60.0);
        s.defer(1);
        s.defer(2);
        s.schedule(Queue::Task, 0.0, 1);
        assert_eq!(s.cancel_deferred(|j| *j == 1), 1);
        assert_eq!(s.pop_deferred(), Some(2));
        s.begin_frame();
        assert_eq!(s.take_due(Queue::Task), [1]);
    }
}
