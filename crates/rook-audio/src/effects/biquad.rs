//! A second-order filter section, with the coefficients of Robert
//! Bristow-Johnson's Audio EQ Cookbook (the bilinear transform of each
//! analog prototype, with the frequency pre-warped), plus the first-order
//! low and high passes as degenerate sections. The equalizer's crossovers
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

    /// A first-order low (`high` false) or high pass, -3 dB at
    /// `frequency` and 6 dB per octave beyond it (the bilinear transform
    /// of `1 / (1 + s)`, pre-warped).
    pub(super) fn first_order(high: bool, frequency: f32, rate: f32) -> Self {
        let k = (omega(frequency, rate) / 2.0).tan();
        let b = if high { [1.0, -1.0, 0.0] } else { [k, k, 0.0] };
        Self::from(b, [1.0 + k, k - 1.0, 0.0])
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

    /// The magnitude of the response at `frequency`, linear.
    pub(super) fn gain_at(&self, frequency: f32, rate: f32) -> f32 {
        let w = TAU * frequency / rate;
        // H(e^jw) = (b0 + b1 e^-jw + b2 e^-2jw) / (1 + a1 e^-jw + a2 e^-2jw).
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let [b0, b1, b2] = self.b;
        let [a1, a2] = self.a;
        let num = (b0 + b1 * c1 + b2 * c2).hypot(b1 * s1 + b2 * s2);
        let den = (1.0 + a1 * c1 + a2 * c2).hypot(a1 * s1 + a2 * s2);
        num / den.max(1e-12)
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
        assert!((db(&Biquad::first_order(false, 1000.0, RATE), 1000.0) + 3.01).abs() < 0.05);
        assert!((db(&Biquad::first_order(true, 1000.0, RATE), 1000.0) + 3.01).abs() < 0.05);
        assert!((Biquad::all_pass(1000.0, q, RATE).gain_at(3000.0, RATE) - 1.0).abs() < 1e-4);
    }
}
