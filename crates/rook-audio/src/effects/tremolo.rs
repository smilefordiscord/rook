//! TremoloSoundEffect and AudioTremolo: the volume swung down and back up
//! `Frequency` times a second, by up to `Depth` (0 leaves it alone, 1 goes
//! to silence), creator-docs. AudioTremolo also shapes the swing: `Shape`
//! morphs it from a triangle (0) to a sine (1), `Skew` moves its turns
//! later (+1) or earlier (-1) in the cycle, `Square` flattens it toward a
//! square wave, and `Duty` is the share of each cycle on its upper half.
//! TremoloSoundEffect has only `Depth`, `Duty` and `Frequency`, and plays
//! with the rest at AudioTremolo's defaults.
//!
//! These are FMOD's tremolo's controls and defaults. Its curve isn't
//! published, so Rook builds one from their descriptions: an oscillator
//! from -1 to 1 (`Duty` and `Skew` warp its phase, `Shape` mixes a
//! triangle and a sine, `Square` amplifies and clips it) and a gain of
//! `1 - Depth (1 - lfo) / 2`, starting at full volume (Inference; none of
//! it measured in Studio).

use std::f32::consts::TAU;

use super::Frame;

/// The swing's settings, unclamped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tremolo {
    pub depth: f32,
    pub duty: f32,
    /// Hz.
    pub frequency: f32,
    pub shape: f32,
    pub skew: f32,
    pub square: f32,
}

impl Default for Tremolo {
    /// Studio's defaults for both classes.
    fn default() -> Self {
        Self {
            depth: 1.0,
            duty: 0.5,
            frequency: 5.0,
            shape: 0.0,
            skew: 0.0,
            square: 0.0,
        }
    }
}

impl Tremolo {
    /// The share of the cycle the upper half takes, and where in each half
    /// the wave turns (0.5 unskewed). Approximation: both are kept a
    /// hundredth from 0 and 1, where creator-docs allows `Duty` its ends,
    /// as the warp divides by each half's share.
    fn warp(&self) -> (f32, f32) {
        let duty = self.duty.clamp(0.01, 0.99);
        let turn = ((1.0 + self.skew.clamp(-1.0, 1.0)) / 2.0).clamp(0.01, 0.99);
        (duty, turn)
    }

    /// The phase the oscillator peaks at.
    fn peak(&self) -> f32 {
        let (duty, turn) = self.warp();
        duty * turn
    }

    /// The oscillator at `phase` (0 to 1), -1 to 1.
    fn lfo(&self, phase: f32) -> f32 {
        let (duty, turn) = self.warp();
        let (half, t) = if phase < duty {
            (0.0, phase / duty)
        } else {
            (0.5, (phase - duty) / (1.0 - duty))
        };
        let t = if t < turn {
            0.5 * t / turn
        } else {
            0.5 + 0.5 * (t - turn) / (1.0 - turn)
        };
        // The unwarped phase: the upper half is 0 to 0.5, peaking at 0.25.
        let q = half + t / 2.0;
        let triangle = 1.0 - 4.0 * (q - 0.25).abs().min((q - 1.25).abs());
        let sine = (TAU * q).sin();
        let wave = triangle + (sine - triangle) * self.shape.clamp(0.0, 1.0);
        let square = self.square.clamp(0.0, 1.0);
        (wave / (1.0 - square * 0.999)).clamp(-1.0, 1.0)
    }

    fn gain(&self, phase: f32) -> f32 {
        1.0 - self.depth.clamp(0.0, 1.0) * (1.0 - self.lfo(phase)) / 2.0
    }
}

#[derive(Debug, Clone)]
pub(super) struct State {
    rate: f32,
    settings: Tremolo,
    phase: f32,
}

impl State {
    pub(super) fn new(rate: f32, settings: Tremolo) -> Self {
        Self {
            rate,
            settings,
            phase: settings.peak(),
        }
    }

    pub(super) fn set(&mut self, settings: Tremolo) {
        self.settings = settings;
    }

    pub(super) fn process(&mut self, frames: &mut [Frame]) {
        let step = self.settings.frequency.clamp(0.0, 20.0) / self.rate;
        for frame in frames {
            let g = self.settings.gain(self.phase);
            frame[0] *= g;
            frame[1] *= g;
            self.phase = (self.phase + step).rem_euclid(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gains(settings: Tremolo, frames: usize) -> Vec<f32> {
        let mut state = State::new(1000.0, settings);
        let mut block = vec![[1.0; 2]; frames];
        state.process(&mut block);
        block.iter().map(|f| f[0]).collect()
    }

    fn with(f: impl FnOnce(&mut Tremolo)) -> Tremolo {
        let mut t = Tremolo::default();
        f(&mut t);
        t
    }

    /// At `Frequency` 5 the volume dips five times a second, to silence at
    /// `Depth` 1, starting at full volume.
    #[test]
    fn swings_at_its_frequency() {
        let g = gains(Tremolo::default(), 1000);
        assert!((g[0] - 1.0).abs() < 1e-3);
        let dips = (1..g.len() - 1)
            .filter(|&i| g[i] < g[i - 1] && g[i] <= g[i + 1])
            .count();
        assert_eq!(dips, 5);
        let low = g.iter().copied().fold(1.0, f32::min);
        assert!(low < 0.01, "{low}");
        // A triangle by default: a quarter of a cycle on, it is halfway.
        assert!((g[50] - 0.5).abs() < 0.01, "{}", g[50]);
    }

    #[test]
    fn depth_sets_how_far_it_falls() {
        let g = gains(with(|t| t.depth = 0.4), 1000);
        let low = g.iter().copied().fold(1.0, f32::min);
        assert!((low - 0.6).abs() < 0.01, "{low}");
        assert!(
            gains(with(|t| t.depth = 0.0), 100)
                .iter()
                .all(|&g| g == 1.0)
        );
    }

    /// `Duty` is the share of a cycle above half volume; `Square`
    /// flattens the swing toward full and silent.
    #[test]
    fn duty_and_square_shape_the_cycle() {
        let upper = |t: Tremolo| gains(t, 1000).iter().filter(|&&g| g > 0.5).count();
        assert!((upper(Tremolo::default()) as i32 - 500).abs() <= 10);
        assert!((upper(with(|t| t.duty = 0.8)) as i32 - 800).abs() <= 10);
        let square = gains(with(|t| t.square = 1.0), 1000);
        let edges = square.iter().filter(|&&g| g > 0.01 && g < 0.99).count();
        assert!(edges < 20, "{edges}");
    }

    #[test]
    fn shape_morphs_triangle_to_sine() {
        // An eighth of a cycle after the top, a sine is at cos 45°.
        let g = gains(with(|t| t.shape = 1.0), 1000);
        let expected = 1.0 - (1.0 - std::f32::consts::FRAC_PI_4.cos()) / 2.0;
        assert!((g[25] - expected).abs() < 0.01, "{}", g[25]);
    }

    /// `Skew` 0.5 turns each half three quarters of the way through it:
    /// the fall from the top to halfway takes an eighth of a cycle, not a
    /// quarter.
    #[test]
    fn skew_moves_the_turns() {
        let late = with(|t| t.skew = 0.5);
        assert!((late.peak() - 0.375).abs() < 1e-6);
        let g = gains(late, 200);
        assert!((g[0] - 1.0).abs() < 1e-3);
        assert!((g[25] - 0.5).abs() < 0.01, "{}", g[25]);
    }
}
