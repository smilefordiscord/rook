//! AudioFilter: one of `AudioFilterType`'s curves at `Frequency`, shaped
//! by `Gain` (dB, the peak and shelves) and `Q` (the peak, passes, band
//! pass and notch), as creator-docs describes them; `GetGainAt` reads the
//! same curve (`docs/behavior/audio/audio-api.md`).
//!
//! Each curve is built from the Audio EQ Cookbook's sections
//! ([`Biquad`]). The steeper passes are cascades of identical sections:
//! 24 dB per octave is two and 48 dB four, each with `Q` to the power of
//! one over their count, so the whole is `Q` at the cutoff (-3 dB at the
//! default 0.707) and bulges a little before it (Confirmed in Studio
//! 0.748: `GetGainAt` matches it to 0.01 dB at every measured frequency,
//! conformance case `content/sound_effects`). The shelves don't take `Q`
//! (creator-docs) and use the cookbook's slope 1. Lowpass6dB is a
//! one-pole smoother, without `Q` (Confirmed the same way).

use super::Frame;
use super::biquad::Biquad;

/// `AudioFilterType`, by its item names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    Peak,
    LowShelf,
    HighShelf,
    Lowpass6dB,
    Lowpass12dB,
    Lowpass24dB,
    Lowpass48dB,
    Highpass12dB,
    Highpass24dB,
    Highpass48dB,
    Bandpass,
    Notch,
}

impl FilterType {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Peak" => Self::Peak,
            "LowShelf" => Self::LowShelf,
            "HighShelf" => Self::HighShelf,
            "Lowpass6dB" => Self::Lowpass6dB,
            "Lowpass12dB" => Self::Lowpass12dB,
            "Lowpass24dB" => Self::Lowpass24dB,
            "Lowpass48dB" => Self::Lowpass48dB,
            "Highpass12dB" => Self::Highpass12dB,
            "Highpass24dB" => Self::Highpass24dB,
            "Highpass48dB" => Self::Highpass48dB,
            "Bandpass" => Self::Bandpass,
            "Notch" => Self::Notch,
            _ => return None,
        })
    }
}

/// An AudioFilter's settings: `Frequency` in Hz (20 to 22,000), `Gain` in
/// dB (-30 to 30) and `Q` (0.1 to 10), each kept to its range when it
/// runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Filter {
    pub kind: FilterType,
    pub frequency: f32,
    pub gain: f32,
    pub q: f32,
}

impl Default for Filter {
    /// Studio's defaults.
    fn default() -> Self {
        Self {
            kind: FilterType::Peak,
            frequency: 2000.0,
            gain: 0.0,
            q: 0.707,
        }
    }
}

impl Filter {
    /// The curve's gain in dB at `frequency` Hz, as `GetGainAt` gives it,
    /// at the mixer's rate. Frequencies below 0 read as 0 Hz (Studio reads
    /// 0 dB at both for a peak; Inference for the other curves).
    pub fn gain_at(&self, frequency: f32) -> f32 {
        let rate = crate::SAMPLE_RATE as f32;
        let gain: f32 = self
            .sections(rate)
            .iter()
            .map(|s| s.gain_at(frequency.max(0.0), rate))
            .product();
        20.0 * gain.log10()
    }

    fn sections(&self, rate: f32) -> Vec<Biquad> {
        let f = self.frequency.clamp(20.0, 22_000.0);
        let gain = self.gain.clamp(-30.0, 30.0);
        let q = self.q.clamp(0.1, 10.0);
        let cascade = |count: u16, high: bool| -> Vec<Biquad> {
            let q = q.powf(1.0 / f32::from(count));
            let section = if high {
                Biquad::high_pass(f, q, rate)
            } else {
                Biquad::low_pass(f, q, rate)
            };
            vec![section; usize::from(count)]
        };
        match self.kind {
            FilterType::Peak => vec![Biquad::peak(f, q, gain, rate)],
            FilterType::LowShelf => vec![Biquad::shelf(false, f, gain, rate)],
            FilterType::HighShelf => vec![Biquad::shelf(true, f, gain, rate)],
            FilterType::Lowpass6dB => vec![Biquad::one_pole(f, rate)],
            FilterType::Lowpass12dB => cascade(1, false),
            FilterType::Lowpass24dB => cascade(2, false),
            FilterType::Lowpass48dB => cascade(4, false),
            FilterType::Highpass12dB => cascade(1, true),
            FilterType::Highpass24dB => cascade(2, true),
            FilterType::Highpass48dB => cascade(4, true),
            FilterType::Bandpass => vec![Biquad::band_pass(f, q, rate)],
            FilterType::Notch => vec![Biquad::notch(f, q, rate)],
        }
    }
}

/// A running AudioFilter.
#[derive(Debug, Clone)]
pub(super) struct State {
    rate: f32,
    sections: Vec<Biquad>,
}

impl State {
    pub(super) fn new(rate: f32, filter: Filter) -> Self {
        Self {
            rate,
            sections: filter.sections(rate),
        }
    }

    /// Take new settings. The sections keep their state while there are
    /// as many of them; a type with more or fewer starts afresh.
    pub(super) fn set(&mut self, filter: Filter) {
        let new = filter.sections(self.rate);
        if new.len() == self.sections.len() {
            for (s, n) in self.sections.iter_mut().zip(new) {
                s.retune(n);
            }
        } else {
            self.sections = new;
        }
    }

    pub(super) fn process(&mut self, frames: &mut [Frame]) {
        for frame in frames {
            for (c, s) in frame.iter_mut().enumerate() {
                *s = self.sections.iter_mut().fold(*s, |x, b| b.run(c, x));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = crate::SAMPLE_RATE as f32;

    /// A settled sine's level through `filter`, in dB.
    fn level(filter: Filter, frequency: f32) -> f32 {
        let mut state = State::new(RATE, filter);
        let mut frames: Vec<Frame> = (0..crate::SAMPLE_RATE)
            // In f64: an f32 phase this far in is rough enough to add
            // noise louder than a steep filter's stop band.
            .map(|i| {
                let t = f64::from(frequency) * f64::from(i) / f64::from(RATE);
                [(std::f64::consts::TAU * t).sin() as f32; 2]
            })
            .collect();
        state.process(&mut frames);
        let tail = &frames[24_000..];
        let rms = (tail.iter().map(|f| f[0] * f[0]).sum::<f32>() / tail.len() as f32).sqrt();
        20.0 * (rms * std::f32::consts::SQRT_2).log10()
    }

    fn filter(kind: FilterType) -> Filter {
        Filter {
            kind,
            frequency: 1000.0,
            ..Filter::default()
        }
    }

    /// An octave above the cutoff, each slope takes off what its name
    /// says, and the default Q is -3 dB at the cutoff.
    #[test]
    fn low_passes_cut_a_high_tone_by_their_slope() {
        for (kind, slope) in [
            (FilterType::Lowpass6dB, 6.0),
            (FilterType::Lowpass12dB, 12.0),
            (FilterType::Lowpass24dB, 24.0),
            (FilterType::Lowpass48dB, 48.0),
        ] {
            let f = filter(kind);
            assert!(
                (level(f, 1000.0) + 3.0).abs() < 0.2,
                "{kind:?} at the cutoff"
            );
            assert!(level(f, 100.0).abs() < 0.2, "{kind:?} passes the low tone");
            // The octave from 2 to 4 kHz, past the knee.
            let drop = level(f, 2000.0) - level(f, 4000.0);
            assert!(
                (drop - slope).abs() < slope * 0.15 + 0.5,
                "{kind:?}: {drop} dB"
            );
        }
        assert!(level(filter(FilterType::Lowpass24dB), 8000.0) < -70.0);
    }

    #[test]
    fn high_passes_cut_a_low_tone() {
        assert!((level(filter(FilterType::Highpass12dB), 1000.0) + 3.0).abs() < 0.2);
        assert!(level(filter(FilterType::Highpass24dB), 10_000.0).abs() < 0.2);
        assert!(level(filter(FilterType::Highpass48dB), 500.0) < -45.0);
    }

    #[test]
    fn peak_shelves_band_pass_and_notch() {
        let with_gain = |kind, gain| Filter {
            gain,
            ..filter(kind)
        };
        assert!((level(with_gain(FilterType::Peak, 12.0), 1000.0) - 12.0).abs() < 0.1);
        assert!(level(with_gain(FilterType::Peak, 12.0), 50.0).abs() < 0.2);
        assert!((level(with_gain(FilterType::LowShelf, -12.0), 50.0) + 12.0).abs() < 0.3);
        assert!(level(with_gain(FilterType::LowShelf, -12.0), 15_000.0).abs() < 0.2);
        assert!((level(with_gain(FilterType::HighShelf, 6.0), 15_000.0) - 6.0).abs() < 0.3);
        assert!(level(filter(FilterType::Bandpass), 1000.0).abs() < 0.1);
        assert!(level(filter(FilterType::Bandpass), 100.0) < -15.0);
        assert!(level(filter(FilterType::Notch), 1000.0) < -40.0);
        assert!(level(filter(FilterType::Notch), 100.0).abs() < 0.2);
        // Gain applies only to the peak and shelves.
        assert_eq!(
            level(with_gain(FilterType::Notch, 30.0), 100.0),
            level(filter(FilterType::Notch), 100.0)
        );
    }

    #[test]
    fn gain_at_reads_the_curve_that_plays() {
        for kind in [
            FilterType::Lowpass24dB,
            FilterType::Peak,
            FilterType::HighShelf,
        ] {
            let f = Filter {
                gain: 9.0,
                q: 2.0,
                ..filter(kind)
            };
            for hz in [200.0, 1000.0, 3000.0] {
                assert!((f.gain_at(hz) - level(f, hz)).abs() < 0.1, "{kind:?} {hz}");
            }
        }
    }

    /// A new frequency keeps the sections' state: the output carries on
    /// instead of starting from silence, so it doesn't click.
    #[test]
    fn settings_change_in_place() {
        let mut state = State::new(RATE, filter(FilterType::Lowpass24dB));
        let mut frames = vec![[1.0, 1.0]; 4800];
        state.process(&mut frames);
        state.set(Filter {
            frequency: 1100.0,
            ..filter(FilterType::Lowpass24dB)
        });
        let mut next = [[1.0, 1.0]];
        state.process(&mut next);
        assert!((next[0][0] - 1.0).abs() < 0.01, "{:?}", next[0]);
        state.set(filter(FilterType::Peak));
        assert_eq!(state.sections.len(), 1);
    }
}
