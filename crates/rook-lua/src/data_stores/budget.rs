//! Request budgets: how many data store requests of each kind a game
//! server may make each minute, by its player count (creator-docs "Data
//! store error codes and limits", server limits), and the queues requests
//! wait in once a budget is spent.
//!
//! Each budget is a token bucket refilled from the game's clock at its
//! per-minute rate. Approximation: Roblox gives a server a one-time
//! startup burst and caps what builds up, and documents neither size, so
//! a bucket starts with and holds at most one minute's worth.

use std::collections::VecDeque;

use super::StoreError;

/// The budgets of the docs' server limits table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pool {
    StandardRead,
    StandardWrite,
    StandardList,
    StandardRemove,
    RemoveVersion,
    OrderedRead,
    OrderedWrite,
    OrderedList,
    OrderedRemove,
    /// `OnUpdate`'s, which today's docs no longer list. Inference: the
    /// older docs' 30 a minute plus 5 per player.
    OnUpdate,
}

const POOLS: usize = 10;

/// Each pool's default limit per minute: a base and an amount per player.
const DEFAULT_LIMITS: [(u32, u32); POOLS] = [
    (60, 40),
    (60, 40),
    (5, 2),
    (60, 40),
    (5, 2),
    (60, 40),
    (30, 5),
    (5, 2),
    (30, 5),
    (30, 5),
];

/// The queues throttled requests wait in (creator-docs: set, ordered set,
/// get and ordered get).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Queue {
    Get,
    Set,
    OrderedGet,
    OrderedSet,
}

/// Requests one queue holds; more are dropped (creator-docs).
const QUEUE_LIMIT: usize = 30;

/// How long past the time it was told to come back a waiting request may
/// stay at the front of its queue before it counts as abandoned (its
/// thread stopped), so it can't hold up the requests behind it.
const ABANDONED_AFTER: f64 = 1.0;

/// What one request spends, and where it waits if it can't yet.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cost {
    pub(crate) pools: Vec<(Pool, f64)>,
    pub(crate) queue: Queue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Admission {
    /// Spent: the request goes ahead.
    Now,
    /// Queued: come back with the ticket in `wait` seconds.
    Queued { ticket: u64, wait: f64 },
}

#[derive(Debug)]
struct Waiting {
    ticket: u64,
    pools: Vec<(Pool, f64)>,
    /// When it was told to come back.
    due: f64,
}

#[derive(Debug)]
pub(crate) struct Budgets {
    limits: [(u32, u32); POOLS],
    tokens: [f64; POOLS],
    /// Game time of the last refill; `None` before the first request.
    refilled: Option<f64>,
    queues: [VecDeque<Waiting>; 4],
    next_ticket: u64,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            limits: DEFAULT_LIMITS,
            tokens: [0.0; POOLS],
            refilled: None,
            queues: Default::default(),
            next_ticket: 0,
        }
    }
}

impl Budgets {
    fn per_minute(&self, pool: Pool, players: usize) -> f64 {
        let (base, per_player) = self.limits[pool as usize];
        f64::from(base) + f64::from(per_player) * players as f64
    }

    fn refill(&mut self, now: f64, players: usize) {
        let elapsed = self.refilled.map(|then| (now - then).max(0.0));
        self.refilled = Some(now);
        for (i, pool) in ALL.into_iter().enumerate() {
            let cap = self.per_minute(pool, players);
            self.tokens[i] = match elapsed {
                Some(dt) => (self.tokens[i] + cap / 60.0 * dt).min(cap),
                None => cap,
            };
        }
    }

    fn affords(&self, pools: &[(Pool, f64)]) -> bool {
        pools.iter().all(|&(p, n)| self.tokens[p as usize] >= n)
    }

    fn spend(&mut self, pools: &[(Pool, f64)]) {
        for &(p, n) in pools {
            self.tokens[p as usize] -= n;
        }
    }

    /// Seconds until `pools` can be afforded `ahead + 1` times over; a
    /// second when a budget never refills (its limit is 0), to look again
    /// in case the limit or the player count changes.
    fn wait_for(&self, pools: &[(Pool, f64)], ahead: usize, players: usize) -> f64 {
        let mut wait: f64 = 0.0;
        for &(p, n) in pools {
            let rate = self.per_minute(p, players) / 60.0;
            let short = n * (ahead + 1) as f64 - self.tokens[p as usize];
            if short > 0.0 {
                wait = wait.max(if rate > 0.0 { short / rate } else { 1.0 });
            }
        }
        wait
    }

    /// Spend a request's budget, or queue it, or `None` when its queue is
    /// full and it's dropped.
    pub(crate) fn admit(&mut self, cost: &Cost, now: f64, players: usize) -> Option<Admission> {
        self.refill(now, players);
        let queue = cost.queue as usize;
        self.drop_abandoned(queue, now);
        if self.queues[queue].is_empty() && self.affords(&cost.pools) {
            self.spend(&cost.pools);
            return Some(Admission::Now);
        }
        if self.queues[queue].len() >= QUEUE_LIMIT {
            return None;
        }
        let ahead = self.queues[queue].len();
        let wait = self.wait_for(&cost.pools, ahead, players);
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        self.queues[queue].push_back(Waiting {
            ticket,
            pools: cost.pools.clone(),
            due: now + wait,
        });
        Some(Admission::Queued { ticket, wait })
    }

    /// A queued request coming back: `None` once it has spent its budget
    /// and goes ahead, else the seconds to wait again. Requests go in the
    /// order they were queued. One dropped as abandoned that comes back
    /// after all is no longer queued, and goes ahead.
    pub(crate) fn poll(&mut self, ticket: u64, now: f64, players: usize) -> Option<f64> {
        self.refill(now, players);
        let queue = self
            .queues
            .iter()
            .position(|q| q.iter().any(|w| w.ticket == ticket))?;
        self.drop_abandoned(queue, now);
        let q = &self.queues[queue];
        let at = q.iter().position(|w| w.ticket == ticket)?;
        if at == 0 && self.affords(&q[0].pools) {
            let pools = q[0].pools.clone();
            self.spend(&pools);
            self.queues[queue].pop_front();
            return None;
        }
        let wait = self.wait_for(&q[at].pools, at, players);
        self.queues[queue][at].due = now + wait;
        Some(wait)
    }

    /// Drop the front request while its thread hasn't come back long after
    /// it was due.
    fn drop_abandoned(&mut self, queue: usize, now: f64) {
        let q = &mut self.queues[queue];
        while q.front().is_some_and(|w| now > w.due + ABANDONED_AFTER) {
            q.pop_front();
        }
    }

    /// The whole requests a pool can make now
    /// (`GetRequestBudgetForRequestType`).
    pub(crate) fn remaining(&mut self, pool: Pool, now: f64, players: usize) -> u32 {
        self.refill(now, players);
        self.tokens[pool as usize].max(0.0) as u32
    }

    /// `SetRateLimitForRequestType`: a new limit, from now on.
    pub(crate) fn set_limit(&mut self, pool: Pool, base: u32, per_player: u32) {
        self.limits[pool as usize] = (base, per_player);
    }
}

const ALL: [Pool; POOLS] = [
    Pool::StandardRead,
    Pool::StandardWrite,
    Pool::StandardList,
    Pool::StandardRemove,
    Pool::RemoveVersion,
    Pool::OrderedRead,
    Pool::OrderedWrite,
    Pool::OrderedList,
    Pool::OrderedRemove,
    Pool::OnUpdate,
];

/// What `GetRequestBudgetForRequestType` reports for a
/// `DataStoreRequestType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reported {
    Pool(Pool),
    /// `UpdateAsync`, which spends both: the smaller.
    Both(Pool, Pool),
    /// Creator-docs DataStoreRequestType: `OrderedList` reports 0.
    Zero,
}

/// Each `DataStoreRequestType` value's budget. The modern values name
/// their pools; the legacy ones (creator-docs: "should not be used")
/// report the pool that now covers the functions each names (Inference).
pub(crate) fn reported(request_type: u32) -> Option<Reported> {
    use Pool::*;
    Some(match request_type {
        0 | 7 | 9 => Reported::Pool(StandardRead),
        1 | 10 => Reported::Pool(StandardWrite),
        2 => Reported::Both(StandardRead, StandardWrite),
        3 => Reported::Pool(OrderedList),
        4 | 14 => Reported::Pool(OrderedWrite),
        5 => Reported::Pool(OnUpdate),
        6 | 11 => Reported::Pool(StandardList),
        8 => Reported::Pool(RemoveVersion),
        12 => Reported::Pool(StandardRemove),
        13 => Reported::Pool(OrderedRead),
        15 => Reported::Zero,
        16 => Reported::Pool(OrderedRemove),
        _ => return None,
    })
}

/// The pool `SetRateLimitForRequestType` sets for a request type, and the
/// most its base and per-player limits may be (creator-docs
/// DataStoreService:SetRateLimitForRequestType's constraints table).
pub(crate) fn configurable(
    request_type: u32,
    name: &'static str,
) -> Result<(Pool, u32, u32), StoreError> {
    use Pool::*;
    const MODERN: (u32, u32) = (10_000, 200);
    let (pool, (base, per_player)) = match request_type {
        0 => (StandardRead, (60, 40)),
        1 => (StandardWrite, (60, 40)),
        3 => (OrderedList, (5, 2)),
        4 => (OrderedWrite, (30, 5)),
        6 => (StandardList, (5, 2)),
        7 => (StandardRead, (5, 2)),
        8 => (RemoveVersion, (5, 2)),
        9 => (StandardRead, MODERN),
        10 => (StandardWrite, MODERN),
        11 => (StandardList, MODERN),
        12 => (StandardRemove, MODERN),
        13 => (OrderedRead, MODERN),
        14 => (OrderedWrite, MODERN),
        15 => (OrderedList, MODERN),
        16 => (OrderedRemove, MODERN),
        _ => return Err(StoreError::NotConfigurable(name)),
    };
    Ok((pool, base, per_player))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cost(pool: Pool) -> Cost {
        Cost {
            pools: vec![(pool, 1.0)],
            queue: Queue::Get,
        }
    }

    #[test]
    fn budgets_start_full_and_refill_by_the_minute() {
        let mut b = Budgets::default();
        assert_eq!(b.remaining(Pool::StandardList, 0.0, 0), 5);
        for _ in 0..5 {
            assert_eq!(
                b.admit(&cost(Pool::StandardList), 0.0, 0),
                Some(Admission::Now)
            );
        }
        assert_eq!(b.remaining(Pool::StandardList, 0.0, 0), 0);
        // 5 a minute: one every 12 seconds.
        assert_eq!(b.remaining(Pool::StandardList, 12.0, 0), 1);
        assert_eq!(b.remaining(Pool::StandardList, 600.0, 0), 5);
        assert_eq!(b.remaining(Pool::StandardList, 600.0, 3), 5);
        assert_eq!(b.remaining(Pool::StandardList, 1200.0, 3), 11);
    }

    #[test]
    fn spent_budgets_queue_in_order_and_drop_past_thirty() {
        let mut b = Budgets::default();
        b.set_limit(Pool::StandardList, 0, 0);
        let mut tickets = Vec::new();
        for _ in 0..30 {
            match b.admit(&cost(Pool::StandardList), 0.0, 0) {
                Some(Admission::Queued { ticket, .. }) => tickets.push(ticket),
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(b.admit(&cost(Pool::StandardList), 0.0, 0), None);
        b.set_limit(Pool::StandardList, 60, 0);
        // One a second: the second in line waits for the first.
        assert_eq!(b.poll(tickets[1], 0.5, 0), Some(1.5));
        assert_eq!(b.poll(tickets[0], 1.0, 0), None);
        assert_eq!(b.poll(tickets[1], 2.0, 0), None);
    }
}
