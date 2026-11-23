//! AudioGate: a noise gate. It starts closed (silent); when the sound
//! rises above the high end of `Threshold` (a NumberRange in dB, -80 to
//! 30) it opens over `Attack` seconds, and when it falls below the low end
//! it closes over `Release` seconds (both 0.001 to 5). The gap between the
//! two is hysteresis, so it doesn't flutter (creator-docs).
//!
//! The level compared is a peak follower: the larger side's absolute
//! sample, falling back with a 50 ms time constant so a tone's level
//! doesn't drop between its peaks (Approximation: creator-docs doesn't say
//! how Studio measures the level; 50 ms holds a 20 Hz tone within 5 dB).
//! The gate's gain moves linearly between 0 and 1, all the way in `Attack`
//! or `Release` (Inference from "over the course of").

use super::{Frame, db};

/// The peak follower's fall, seconds.
const FOLLOW_SECONDS: f32 = 0.050;

/// AudioGate's settings: thresholds in dB, times in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Settings {
    pub(super) low: f32,
    pub(super) high: f32,
    pub(super) attack: f32,
    pub(super) release: f32,
}

#[derive(Debug, Clone)]
pub(super) struct Gate {
    rate: f32,
    /// Linear.
    low: f32,
    high: f32,
    /// The gain's step a frame while opening and closing.
    opening: f32,
    closing: f32,
    follow: f32,
    level: f32,
    open: bool,
    gain: f32,
}

impl Gate {
    pub(super) fn new(rate: f32, settings: Settings) -> Self {
        let mut g = Self {
            rate,
            low: 0.0,
            high: 0.0,
            opening: 1.0,
            closing: 1.0,
            follow: (-1.0 / (FOLLOW_SECONDS * rate)).exp(),
            level: 0.0,
            open: false,
            gain: 0.0,
        };
        g.set(settings);
        g
    }

    pub(super) fn set(&mut self, s: Settings) {
        let low = s.low.clamp(-80.0, 30.0);
        let high = s.high.clamp(-80.0, 30.0);
        self.low = db(low.min(high));
        self.high = db(high.max(low));
        self.opening = 1.0 / (s.attack.clamp(0.001, 5.0) * self.rate);
        self.closing = 1.0 / (s.release.clamp(0.001, 5.0) * self.rate);
    }

    pub(super) fn process(&mut self, frames: &mut [Frame]) {
        for frame in frames {
            let peak = frame[0].abs().max(frame[1].abs());
            self.level = peak.max(self.level * self.follow);
            if self.open {
                self.open = self.level >= self.low;
            } else {
                self.open = self.level > self.high;
            }
            self.gain = if self.open {
                (self.gain + self.opening).min(1.0)
            } else {
                (self.gain - self.closing).max(0.0)
            };
            frame[0] *= self.gain;
            frame[1] *= self.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;
    const DEFAULTS: Settings = Settings {
        low: -36.0,
        high: -24.0,
        attack: 0.01,
        release: 0.1,
    };

    fn tone(amplitude: f32, frames: usize) -> Vec<Frame> {
        (0..frames)
            .map(|i| [amplitude * (std::f32::consts::TAU * 220.0 * i as f32 / RATE).sin(); 2])
            .collect()
    }

    fn peak(frames: &[Frame]) -> f32 {
        frames.iter().flatten().map(|s| s.abs()).fold(0.0, f32::max)
    }

    #[test]
    fn silences_sound_below_the_threshold() {
        let mut g = Gate::new(RATE, DEFAULTS);
        // -30 dB: between the two thresholds, and the gate starts closed.
        let mut frames = tone(db(-30.0), 48_000);
        g.process(&mut frames);
        assert_eq!(peak(&frames), 0.0);
    }

    /// Above the high threshold it opens over `Attack`; then it stays open
    /// down to the low one (hysteresis), and closes over `Release` below
    /// it.
    #[test]
    fn opens_and_closes_with_hysteresis() {
        let mut g = Gate::new(RATE, DEFAULTS);
        let mut loud = tone(db(-12.0), 4800);
        g.process(&mut loud);
        assert!(peak(&loud[..240]) < db(-12.0) * 0.6, "opening over 10 ms");
        assert!((peak(&loud[2400..]) - db(-12.0)).abs() < 1e-3);
        let mut between = tone(db(-30.0), 24_000);
        g.process(&mut between);
        assert!((peak(&between[12_000..]) - db(-30.0)).abs() < 1e-4);
        let mut quiet = tone(db(-50.0), 24_000);
        g.process(&mut quiet);
        assert!(peak(&quiet[..2400]) > 0.0, "closing over 100 ms");
        assert_eq!(peak(&quiet[12_000..]), 0.0);
    }
}
