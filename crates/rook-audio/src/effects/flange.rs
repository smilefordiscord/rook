//! FlangeSoundEffect and AudioFlanger: the sound plus one copy of it whose
//! delay sweeps from nothing up to a few milliseconds and back at `Rate`
//! Hz, `Mix` being the copy's share (creator-docs). The two classes give
//! `Depth` different spans: FlangeSoundEffect's is a share of 40 ms,
//! AudioFlanger's of 10 ms (creator-docs), so the reader turns it into
//! the longest delay.
//!
//! A flanger without feedback: one delay line read at a point swung by a
//! raised cosine, `longest (1 - cos 2πφ) / 2`, the same on both sides,
//! mixed as `(1 - Mix) dry + Mix copy` (Inference: FlangeSoundEffect's
//! controls, ranges and 40 ms span are FMOD's flange's, which has no
//! feedback; the sweep's shape isn't published, and Studio's isn't
//! measured).

use super::Frame;
use super::line::Line;

/// FlangeSoundEffect's deepest delay, at `Depth` 1, seconds
/// (creator-docs).
pub const FLANGE_SPAN: f32 = 0.040;
/// AudioFlanger's deepest delay, at `Depth` 1, seconds (creator-docs).
pub const FLANGER_SPAN: f32 = 0.010;
/// The longest delay either class reaches, seconds.
const LONGEST: f32 = FLANGE_SPAN;

#[derive(Debug, Clone)]
pub(super) struct Flanger {
    rate: f32,
    /// The deepest delay, frames.
    delay: f32,
    mix: f32,
    speed: f32,
    phase: f32,
    line: Line,
}

impl Flanger {
    pub(super) fn new(rate: f32, delay: f32, mix: f32, speed: f32) -> Self {
        let mut f = Self {
            rate,
            delay: 0.0,
            mix: 0.0,
            speed: 0.0,
            phase: 0.0,
            line: Line::new((rate * LONGEST) as usize + 2),
        };
        f.set(delay, mix, speed);
        f
    }

    /// `delay` is the deepest delay in seconds.
    pub(super) fn set(&mut self, delay: f32, mix: f32, speed: f32) {
        self.delay = delay.clamp(0.0, LONGEST) * self.rate;
        self.mix = mix.clamp(0.0, 1.0);
        self.speed = speed.clamp(0.0, 20.0);
    }

    pub(super) fn latency(&self) -> usize {
        self.line.len()
    }

    pub(super) fn process(&mut self, frames: &mut [Frame]) {
        let step = self.speed / self.rate;
        for frame in frames {
            self.line.push(*frame);
            let sweep = (1.0 - (std::f32::consts::TAU * self.phase).cos()) / 2.0;
            let delay = 1.0 + self.delay * sweep;
            for (c, s) in frame.iter_mut().enumerate() {
                *s = *s * (1.0 - self.mix) + self.line.read(delay, c) * self.mix;
            }
            self.phase = (self.phase + step).rem_euclid(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The copy's delay sweeps from none to the deepest and back once a
    /// cycle: an impulse at the sweep's top comes back that much later.
    #[test]
    fn delay_sweeps_to_its_depth() {
        let rate = 1000.0;
        // 1 Hz: the delay is deepest half a second in.
        let mut f = Flanger::new(rate, 0.010, 1.0, 1.0);
        let mut frames = vec![[0.0; 2]; 520];
        frames[500] = [1.0, 1.0];
        f.process(&mut frames);
        let at = frames.iter().position(|x| x[0] > 0.5).unwrap();
        assert_eq!(at, 510);
        // At the start of the cycle the copy isn't delayed at all.
        let mut f = Flanger::new(rate, 0.010, 0.5, 1.0);
        let mut frames = vec![[1.0; 2]; 1];
        f.process(&mut frames);
        assert_eq!(frames[0], [1.0, 1.0]);
    }

    /// Mixed with itself a little later, a tone cancels where the delay
    /// is half its period: the comb a flanger sweeps.
    #[test]
    fn combs_against_the_dry_sound() {
        let rate = 48_000.0;
        // Held at its deepest, 1 ms: 500 Hz cancels, 1 kHz adds up.
        let level = |hz: f32| {
            let mut f = Flanger::new(rate, 0.001, 0.5, 0.0);
            f.phase = 0.5;
            let mut frames: Vec<Frame> = (0..4800)
                .map(|i| [(std::f32::consts::TAU * hz * i as f32 / rate).sin(); 2])
                .collect();
            f.process(&mut frames);
            frames[2400..]
                .iter()
                .map(|x| x[0].abs())
                .fold(0.0, f32::max)
        };
        assert!(level(500.0) < 0.02, "{}", level(500.0));
        assert!((level(1000.0) - 1.0).abs() < 0.02);
    }
}
