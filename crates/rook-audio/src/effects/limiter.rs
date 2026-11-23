//! AudioLimiter: whenever the sound goes over `MaxLevel` dB (-12 to 0) it
//! is turned down at once, coming back up over `Release` seconds (0.001 to
//! 1) once it isn't (creator-docs).
//!
//! A feed-forward peak limiter with an instant attack and no look-ahead:
//! it follows the larger side's absolute sample, falling back with time
//! constant `Release`, and scales both sides by `MaxLevel` over that level
//! when it is above it, so no sample comes out over `MaxLevel`
//! (Inference: creator-docs says only that loud streams are reduced "for a
//! moment"; whether Studio looks ahead, and so delays the sound, isn't
//! measured).

use super::{Frame, db};

#[derive(Debug, Clone)]
pub(super) struct Limiter {
    rate: f32,
    /// Linear.
    max: f32,
    /// How much of the level followed is left after a frame.
    fall: f32,
    level: f32,
}

impl Limiter {
    pub(super) fn new(rate: f32, max_level: f32, release: f32) -> Self {
        let mut l = Self {
            rate,
            max: 1.0,
            fall: 0.0,
            level: 0.0,
        };
        l.set(max_level, release);
        l
    }

    pub(super) fn set(&mut self, max_level: f32, release: f32) {
        self.max = db(max_level.clamp(-12.0, 0.0));
        self.fall = (-1.0 / (release.clamp(0.001, 1.0) * self.rate)).exp();
    }

    pub(super) fn process(&mut self, frames: &mut [Frame]) {
        for frame in frames {
            let peak = frame[0].abs().max(frame[1].abs());
            self.level = peak.max(self.level * self.fall);
            if self.level > self.max {
                let gain = self.max / self.level;
                frame[0] *= gain;
                frame[1] *= gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn tone(amplitude: f32, frames: usize) -> Vec<Frame> {
        (0..frames)
            .map(|i| [amplitude * (std::f32::consts::TAU * 440.0 * i as f32 / RATE).sin(); 2])
            .collect()
    }

    fn peak(frames: &[Frame]) -> f32 {
        frames.iter().flatten().map(|s| s.abs()).fold(0.0, f32::max)
    }

    /// Nothing comes out over `MaxLevel`, from the first sample on.
    #[test]
    fn caps_the_peak() {
        let mut l = Limiter::new(RATE, -6.0, 0.01);
        let mut frames = tone(2.0, 4800);
        l.process(&mut frames);
        assert!(peak(&frames) <= db(-6.0) + 1e-6, "{}", peak(&frames));
        assert!(peak(&frames[2400..]) > db(-6.0) * 0.95);
    }

    #[test]
    fn leaves_quiet_sound_alone() {
        let mut l = Limiter::new(RATE, 0.0, 0.01);
        let mut frames = tone(0.9, 4800);
        let plain = frames.clone();
        l.process(&mut frames);
        assert_eq!(frames, plain);
    }

    /// After a loud burst, quiet sound comes back over about `Release`.
    #[test]
    fn recovers_over_its_release() {
        let mut l = Limiter::new(RATE, -12.0, 0.1);
        let mut burst = vec![[1.0, 1.0]; 10];
        l.process(&mut burst);
        let mut quiet = vec![[0.2, 0.2]; 48_000];
        l.process(&mut quiet);
        // One time constant on, the level followed has fallen to 1/e:
        // still over -12 dB (0.25), so still turned down.
        assert!(quiet[4800][0] < 0.2 * 0.7);
        assert!((quiet[47_999][0] - 0.2).abs() < 1e-6);
    }
}
