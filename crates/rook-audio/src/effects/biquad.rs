//! A second-order filter section, with the coefficients of Robert
//! Bristow-Johnson's Audio EQ Cookbook (the bilinear transform of each
//! analog prototype, with the frequency pre-warped), plus a one-pole low
//! pass as a degenerate section. The equalizer's crossovers
//! and AudioFilter are built from these.

use std::f32::consts::TAU;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    /// Per channel: transposed direct form II state.
    z: [[f32; 2]; 2],
}

/// The cookbook's `w0` (radians per sample), kept below Nyquist so a
/// frequency past it can't make the section unstable.
fn omega(frequency: f32, rate: f32) -> f32 {
    TAU * (frequency / rate).clamp(1e-6, 0.49)
}

impl Biquad {
    /// From unnormalized coefficients `b0..b2` and `a0..a2`.
    fn from(b: [f32; 3], a: [f32; 3]) -> Self {
        Self {
            b: b.map(|v| v / a[0]),
            a: [a[1] / a[0], a[2] / a[0]],
            z: [[0.0; 2]; 2],
        }
    }

    pub(super) fn low_pass(frequency: f32, q: f32, rate: f32) -> Self {
        let (sin, cos) = omega(frequency, rate).sin_cos();
        let alpha = sin / (2.0 * q);
        let b = (1.0 - cos) / 2.0;
        Self::from([b, 1.0 - cos, b], [1.0 + alpha, -2.0 * cos, 1.0 - alpha])
    }

    pub(super) fn high_pass(frequency: f32, q: f32, rate: f32) -> Self {
        let (sin, cos) = omega(frequency, rate).sin_cos();
        let alpha = sin / (2.0 * q);
        let b = (1.0 + cos) / 2.0;
        Self::from([b, -(1.0 + cos), b], [1.0 + alpha, -2.0 * cos, 1.0 - alpha])
    }

    pub(super) fn all_pass(frequency: f32, q: f32, rate: f32) -> Self {
        let (sin, cos) = omega(frequency, rate).sin_cos();
        let alpha = sin / (2.0 * q);
        let a = [1.0 + alpha, -2.0 * cos, 1.0 - alpha];
        Self::from([a[2], a[1], a[0]], a)
    }

    /// The band pass with 0 dB at its peak.
    pub(super) fn band_pass(frequency: f32, q: f32, rate: f32) -> Self {
        let (sin, cos) = omega(frequency, rate).sin_cos();
        let alpha = sin / (2.0 * q);
        Self::from([alpha, 0.0, -alpha], [1.0 + alpha, -2.0 * cos, 1.0 - alpha])
    }

    pub(super) fn notch(frequency: f32, q: f32, rate: f32) -> Self {
        let (sin, cos) = omega(frequency, rate).sin_cos();
        let alpha = sin / (2.0 * q);
        Self::from(
            [1.0, -2.0 * cos, 1.0],
            [1.0 + alpha, -2.0 * cos, 1.0 - alpha],
        )
    }

    /// `gain` dB at `frequency`, falling back to 0 dB away from it.
    pub(super) fn peak(frequency: f32, q: f32, gain: f32, rate: f32) -> Self {
        let (sin, cos) = omega(frequency, rate).sin_cos();
        let alpha = sin / (2.0 * q);
        let a = 10f32.powf(gain / 40.0);
        Self::from(
            [1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a],
            [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a],
        )
    }

    /// `gain` dB below `frequency` (`high` false) or above it, half that at
    /// `frequency`, with the cookbook's steepest shelf that doesn't
    /// overshoot (slope 1).
    pub(super) fn shelf(high: bool, frequency: f32, gain: f32, rate: f32) -> Self {
        let (sin, cos) = omega(frequency, rate).sin_cos();
        let a = 10f32.powf(gain / 40.0);
        let alpha = sin / std::f32::consts::SQRT_2;
        let root = 2.0 * a.sqrt() * alpha;
        // The high shelf is the low one with cos negated, and b1 and a1
        // flipped in sign.
        let (cos, s) = if high { (-cos, -1.0) } else { (cos, 1.0) };
        Self::from(
            [
                a * ((a + 1.0) - (a - 1.0) * cos + root),
                s * 2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - root),
            ],
            [
                (a + 1.0) + (a - 1.0) * cos + root,
                s * -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - root,
            ],
        )
    }

    /// A one-pole low pass, `y += a (x - y)` with `a = 1 - e^(-2π f / rate)`:
    /// -3 dB near `frequency` and 6 dB per octave beyond it, flattening
    /// toward Nyquist as it has no zero there.
    pub(super) fn one_pole(frequency: f32, rate: f32) -> Self {
        let a = 1.0 - (-omega(frequency, rate)).exp();
        Self::from([a, 0.0, 0.0], [1.0, a - 1.0, 0.0])
    }

    /// Take `other`'s coefficients, keeping this section's state so a
    /// change of setting doesn't click.
    pub(super) fn retune(&mut self, other: Self) {
        self.b = other.b;
        self.a = other.a;
    }

    pub(super) fn run(&mut self, channel: usize, x: f32) -> f32 {
        let z = &mut self.z[channel];
        let y = self.b[0] * x + z[0];
        z[0] = self.b[1] * x - self.a[0] * y + z[1];
        z[1] = self.b[2] * x - self.a[1] * y;
        y
    }

    /// The magnitude of the response at `frequency`, linear: `|H(z)|` at
    /// `z = e^-jw`, each polynomial in `z` taken by Horner's rule in single
    /// precision. Studio's `GetGainAt` reads deep stop bands and a notch's
    /// center to the same rounding (Confirmed to 0.01 dB down to -320 dB in
    /// conformance case `content/sound_effects`; evaluating more exactly
    /// reads those differently).
    pub(super) fn gain_at(&self, frequency: f32, rate: f32) -> f32 {
        let (sin, cos) = (TAU * frequency / rate).sin_cos();
        let z = (cos, -sin);
        // c0 + z (c1 + z c2).
        let horner = |c0: f32, c1: f32, c2: f32| {
            let t = (c1 + z.0 * c2, z.1 * c2);
            (c0 + (z.0 * t.0 - z.1 * t.1), z.0 * t.1 + z.1 * t.0)
        };
        let [b0, b1, b2] = self.b;
        let [a1, a2] = self.a;
        let num = horner(b0, b1, b2);
        let den = horner(1.0, a1, a2);
        num.0.hypot(num.1) / den.0.hypot(den.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn db(b: &Biquad, f: f32) -> f32 {
        20.0 * b.gain_at(f, RATE).log10()
    }

    #[test]
    fn cookbook_responses() {
        let q = std::f32::consts::FRAC_1_SQRT_2;
        assert!((db(&Biquad::low_pass(1000.0, q, RATE), 1000.0) + 3.01).abs() < 0.05);
        assert!((db(&Biquad::high_pass(1000.0, q, RATE), 1000.0) + 3.01).abs() < 0.05);
        assert!((db(&Biquad::peak(1000.0, 1.0, 12.0, RATE), 1000.0) - 12.0).abs() < 0.01);
        assert!(db(&Biquad::band_pass(1000.0, 1.0, RATE), 1000.0).abs() < 0.01);
        assert!(db(&Biquad::notch(1000.0, 1.0, RATE), 1000.0) < -60.0);
        assert!((db(&Biquad::shelf(false, 1000.0, -12.0, RATE), 20.0) + 12.0).abs() < 0.1);
        assert!((db(&Biquad::shelf(false, 1000.0, -12.0, RATE), 1000.0) + 6.0).abs() < 0.05);
        assert!((db(&Biquad::shelf(true, 1000.0, 9.0, RATE), 20_000.0) - 9.0).abs() < 0.1);
        assert!(db(&Biquad::shelf(true, 1000.0, 9.0, RATE), 20.0).abs() < 0.05);
        assert!((db(&Biquad::one_pole(1000.0, RATE), 1000.0) + 3.0).abs() < 0.05);
        assert!((Biquad::all_pass(1000.0, q, RATE).gain_at(3000.0, RATE) - 1.0).abs() < 1e-4);
    }
}
