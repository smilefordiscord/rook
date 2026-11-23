//! The post effects under Lighting and the current Camera, as the frame's
//! last passes take them (docs/behavior/lighting/post-processing.md).

/// Every enabled PostEffect the frame applies, read from Lighting's
/// children and then the current Camera's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostEffects {
    /// The first enabled ColorGradingEffect's `TonemapperPreset`.
    pub tonemapper: Tonemapper,
    /// The ColorCorrectionEffects, applied one after another in this order.
    pub color: Vec<ColorCorrection>,
    /// The greatest enabled BlurEffect's `Size`, in pixels at the screen's
    /// resolution; 0 blurs nothing.
    pub blur: f32,
}

/// `ColorGradingEffect.TonemapperPreset`: how light becomes the screen's
/// pixels (docs/behavior/lighting/tone-mapping.md).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tonemapper {
    /// Studio's curve, which darkens the darks and rolls off the brights;
    /// places without a ColorGradingEffect get it too.
    #[default]
    Default,
    /// No curve: the light's square root is the pixel.
    Retro,
}

/// A ColorCorrectionEffect's properties.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorCorrection {
    /// Added to each channel: -1 turns every pixel black, 1 white.
    pub brightness: f32,
    /// 0 keeps the pixels; above it spreads them from mid gray, below
    /// draws them in.
    pub contrast: f32,
    /// 0 keeps the colors; -1 is gray, above 0 more vivid.
    pub saturation: f32,
    /// Multiplies each channel (sRGB, 0 to 1).
    pub tint: [f32; 3],
}

/// An affine map of sRGB pixel values: each row's first three numbers
/// times the pixel, plus its fourth. Studio's last pass multiplies the
/// pixels by one such matrix (`shaders_glsl3.pack`).
pub type ColorMatrix = [[f32; 4]; 3];

pub(crate) const IDENTITY: ColorMatrix = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];

/// Rec. 709's luma weights, which saturation mixes towards (Inference:
/// Studio's shader takes a whole matrix, built where it can't be read).
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

impl ColorCorrection {
    /// What it does to a pixel: brightness and contrast about mid gray,
    /// then saturation about the pixel's luma, then the tint (Inference on
    /// the order and the pivot; each alone does what the creator docs
    /// say).
    pub fn matrix(&self) -> ColorMatrix {
        let c = 1.0 + self.contrast;
        let s = 1.0 + self.saturation;
        let offset = 0.5 - 0.5 * c + self.brightness;
        let mut m = [[0.0; 4]; 3];
        for (i, row) in m.iter_mut().enumerate() {
            // Saturation: s × the channel + (1 - s) × luma, of the pixel
            // after contrast and brightness.
            for (j, v) in row.iter_mut().take(3).enumerate() {
                let own = if i == j { s } else { 0.0 };
                *v = (own + (1.0 - s) * LUMA[j]) * c * self.tint[i];
            }
            // The offset goes through saturation unchanged, as the luma
            // weights sum to 1.
            row[3] = offset * self.tint[i];
        }
        m
    }
}

/// `second` applied after `first`.
fn then(first: &ColorMatrix, second: &ColorMatrix) -> ColorMatrix {
    let mut m = [[0.0; 4]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| second[i][k] * first[k][j]).sum::<f32>();
        }
        row[3] += second[i][3];
    }
    m
}

impl PostEffects {
    /// The ColorCorrections as one matrix, or `None` without any.
    pub(crate) fn color_matrix(&self) -> Option<ColorMatrix> {
        let mut effects = self.color.iter();
        let first = effects.next()?.matrix();
        Some(effects.fold(first, |m, c| then(&m, &c.matrix())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: ColorCorrection = ColorCorrection {
        brightness: 0.0,
        contrast: 0.0,
        saturation: 0.0,
        tint: [1.0; 3],
    };

    fn apply(m: &ColorMatrix, p: [f32; 3]) -> [f32; 3] {
        m.map(|r| r[0] * p[0] + r[1] * p[1] + r[2] * p[2] + r[3])
    }

    fn near(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn defaults_change_nothing() {
        assert!(near(
            apply(&PLAIN.matrix(), [0.2, 0.5, 0.9]),
            [0.2, 0.5, 0.9]
        ));
    }

    #[test]
    fn each_property_does_what_the_docs_say() {
        let with = |f: fn(&mut ColorCorrection)| {
            let mut c = PLAIN;
            f(&mut c);
            c.matrix()
        };
        let p = [0.8, 0.4, 0.1];
        // Brightness -1 is black and 1 white, once clamped.
        let dark = apply(&with(|c| c.brightness = -1.0), p);
        assert!(dark.iter().all(|&v| v <= 0.0), "{dark:?}");
        let light = apply(&with(|c| c.brightness = 1.0), p);
        assert!(light.iter().all(|&v| v >= 1.0), "{light:?}");
        // Saturation -1 is gray.
        let gray = apply(&with(|c| c.saturation = -1.0), p);
        assert!((gray[0] - gray[1]).abs() < 1e-6 && (gray[1] - gray[2]).abs() < 1e-6);
        // Contrast spreads values from mid gray.
        let spread = apply(&with(|c| c.contrast = 1.0), p);
        assert!(near(spread, [1.1, 0.3, -0.3]), "{spread:?}");
        // A red tint takes green and blue away.
        let red = apply(&with(|c| c.tint = [1.0, 0.0, 0.0]), p);
        assert!(near(red, [0.8, 0.0, 0.0]));
    }

    /// Two effects apply one after the other: a tint after a brightness
    /// lift tints the lifted color.
    #[test]
    fn several_compose_in_order() {
        let lift = ColorCorrection {
            brightness: 0.2,
            ..PLAIN
        };
        let half = ColorCorrection {
            tint: [0.5; 3],
            ..PLAIN
        };
        let post = PostEffects {
            color: vec![lift, half],
            ..PostEffects::default()
        };
        let m = post.color_matrix().unwrap();
        assert!(near(apply(&m, [0.4; 3]), [0.3; 3]));
        assert_eq!(PostEffects::default().color_matrix(), None);
    }
}
