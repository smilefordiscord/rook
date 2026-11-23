//! BloomEffect: the frame's brightest light, spread through a chain of
//! ever smaller copies and added back before the tone curve, as Studio's
//! bloom shaders do (`shaders_glsl3.pack`: `DownSampleGjoel*`,
//! `UpSampleTent`, `ImageProcessBloom*`). The first copy, at half the
//! frame's size, keeps each texel's light above the threshold; each
//! smaller one averages the one before; then from the smallest up each
//! adds a tent-filtered copy of itself to the next larger, and the
//! largest goes into a sum every Bloom adds to. A Bloom with `Size` 0
//! spreads nothing: the tone pass adds its light in place
//! (docs/behavior/lighting/post-processing.md#bloomeffect).

use super::*;

/// The most copies a chain makes: its reach is about 2 to the power of
/// one more than this, in pixels.
const MAX_LEVELS: u32 = 8;

/// How many Blooms with `Size` 0 the tone pass adds in place.
const IN_PLACE: usize = 4;

/// One bloom pass's parameters (`shaders/post/bloom.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct BloomPass {
    /// xy: one texel of the texture read, zw: one texel of the texture
    /// drawn, in texture coordinates.
    texels: [f32; 4],
    /// x: the threshold in light units; y: the intensity; z: what the
    /// tent of the first texture is scaled by; w: what the second texture
    /// is scaled by.
    weights: [f32; 4],
}

/// The threshold in light units for `BloomEffect.Threshold`, which the
/// creator docs put at 1 for only pure white and 0 for every color: the
/// frame's encoded value (the light's square root over the frame's 4
/// units) at which light blooms (Inference: Studio's in-place Neon glow
/// thresholds the encoded value; its bloom chain the light).
fn threshold_units(threshold: f32) -> f32 {
    FRAME_UNITS * threshold.max(0.0).powi(2)
}

/// The light the frame holds at its brightest (`shaders/tone.wgsl`).
const FRAME_UNITS: f32 = 4.0;

/// How many copies a Bloom of `size` (`BloomEffect.Size`, pixels) makes,
/// so its glow reaches about `size` pixels (Approximation: how Studio's
/// chain follows Size isn't measured).
fn levels(size: f32) -> u32 {
    let reach = size.max(1.0).log2().ceil() as i64 - 1;
    reach.clamp(1, i64::from(MAX_LEVELS)) as u32
}

/// What a frame's Blooms drew for the tone pass.
#[derive(Debug, Default)]
pub(super) struct Drawn {
    /// The sum of the chained Blooms' light, at half the frame's size.
    pub(super) texture: Option<wgpu::TextureView>,
    /// The Blooms with `Size` 0: thresholds in light units, then
    /// intensities (0 for none).
    pub(super) in_place: [[f32; 4]; 2],
}

/// The chain's copies at one frame size, the sum, and the groups reading
/// each.
#[derive(Debug)]
struct Textures {
    frame_size: (u32, u32),
    /// Each copy and its size, the half-size one first.
    levels: Vec<(Target, (u32, u32))>,
    sum: Target,
    /// Reads the frame.
    from_frame: wgpu::BindGroup,
    /// Reads each copy alone, by its level.
    from_level: Vec<wgpu::BindGroup>,
    /// Reads the second copy (or nothing) and the first.
    last: wgpu::BindGroup,
}

impl Textures {
    /// `slot`'s textures, made again unless they fit `frame` and hold
    /// `levels` copies.
    fn at<'a>(
        slot: &'a mut Option<Self>,
        device: &wgpu::Device,
        shared: &Shared,
        (frame, frame_size): (&wgpu::TextureView, (u32, u32)),
        levels: u32,
    ) -> &'a Self {
        let fresh = |t: &Self| t.frame_size == frame_size && t.levels.len() >= levels as usize;
        if !slot.as_ref().is_some_and(fresh) {
            let mut size = frame_size;
            let made: Vec<_> = (0..levels)
                .map(|_| {
                    size = (size.0.div_ceil(2), size.1.div_ceil(2));
                    (Shared::target(device, "rook bloom", size), size)
                })
                .collect();
            let sum = Shared::target(device, "rook bloom sum", made[0].1);
            let second = made.get(1).map(|(t, _)| &t.view);
            *slot = Some(Self {
                frame_size,
                from_frame: shared.group(device, frame, None),
                from_level: (made.iter())
                    .map(|(t, _)| shared.group(device, &t.view, None))
                    .collect(),
                last: shared.group(
                    device,
                    second.unwrap_or(&shared.black),
                    Some(&made[0].0.view),
                ),
                levels: made,
                sum,
            });
        }
        slot.as_ref().expect("made above")
    }
}

#[derive(Debug)]
pub(crate) struct BloomPasses {
    extract: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    textures: Option<Textures>,
}

/// Adds what a pass draws to what's there.
const ADD: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

impl BloomPasses {
    pub(super) fn new(device: &wgpu::Device, shared: &Shared, module: &wgpu::ShaderModule) -> Self {
        Self {
            extract: shared.pipeline(device, module, "fs_extract", None),
            down: shared.pipeline(device, module, "fs_down", None),
            up: shared.pipeline(device, module, "fs_up", Some(ADD)),
            textures: None,
        }
    }

    /// Draw `blooms` (each Bloom's settings) from the frame, `size`
    /// pixels.
    pub(super) fn record(
        &mut self,
        device: &wgpu::Device,
        shared: &mut Shared,
        encoder: &mut wgpu::CommandEncoder,
        (frame, size): (&wgpu::TextureView, (u32, u32)),
        blooms: &[Bloom],
    ) -> Drawn {
        let mut drawn = Drawn::default();
        let (flat, chained): (Vec<&Bloom>, Vec<&Bloom>) =
            blooms.iter().partition(|b| b.size <= 0.0);
        for (i, b) in flat.iter().take(IN_PLACE).enumerate() {
            drawn.in_place[0][i] = threshold_units(b.threshold);
            drawn.in_place[1][i] = b.intensity;
        }
        let Some(most) = chained.iter().map(|b| levels(b.size)).max() else {
            return drawn;
        };
        let t = Textures::at(&mut self.textures, device, shared, (frame, size), most);
        let texel = |(w, h): (u32, u32)| [1.0 / w as f32, 1.0 / h as f32];
        let pass = |from, to, weights| {
            let ([a, b], [c, d]) = (texel(from), texel(to));
            BloomPass {
                texels: [a, b, c, d],
                weights,
            }
        };
        for (i, b) in chained.iter().enumerate() {
            let n = levels(b.size) as usize;
            let (first, first_size) = &t.levels[0];
            let light = [threshold_units(b.threshold), b.intensity, 0.0, 0.0];
            let slot = shared.slots.push(&pass(size, *first_size, light));
            let extract = &self.extract;
            Shared::run(
                encoder,
                "rook bloom",
                &first.view,
                extract,
                &t.from_frame,
                slot,
                false,
            );
            for k in 1..n {
                let (to, to_size) = &t.levels[k];
                let slot = shared
                    .slots
                    .push(&pass(t.levels[k - 1].1, *to_size, [0.0; 4]));
                let group = &t.from_level[k - 1];
                Shared::run(
                    encoder,
                    "rook bloom down",
                    &to.view,
                    &self.down,
                    group,
                    slot,
                    false,
                );
            }
            for k in (2..n).rev() {
                let (to, to_size) = &t.levels[k - 1];
                let slot =
                    (shared.slots).push(&pass(t.levels[k].1, *to_size, [0.0, 0.0, 1.0, 0.0]));
                let group = &t.from_level[k];
                Shared::run(
                    encoder,
                    "rook bloom up",
                    &to.view,
                    &self.up,
                    group,
                    slot,
                    true,
                );
            }
            // Each copy's light counts once in the sum (Approximation:
            // how Studio weighs them isn't measured).
            let share = 1.0 / n as f32;
            let tent = if n > 1 { share } else { 0.0 };
            let from = t.levels.get(1).map_or(*first_size, |l| l.1);
            let slot = shared
                .slots
                .push(&pass(from, *first_size, [0.0, 0.0, tent, share]));
            let up = &self.up;
            Shared::run(
                encoder,
                "rook bloom sum",
                &t.sum.view,
                up,
                &t.last,
                slot,
                i > 0,
            );
        }
        drawn.texture = Some(t.sum.view.clone());
        drawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Threshold 1 keeps only light the frame holds at its brightest, 0
    /// keeps all of it.
    #[test]
    fn thresholds_follow_the_frames_values() {
        assert_eq!(threshold_units(1.0), FRAME_UNITS);
        assert_eq!(threshold_units(0.0), 0.0);
        assert_eq!(threshold_units(0.5), 1.0);
    }

    /// The default Size (24 pixels) makes four copies, reaching about 32
    /// pixels; tiny and huge Sizes stay within the chain.
    #[test]
    fn size_picks_how_many_copies() {
        assert_eq!(levels(24.0), 4);
        assert_eq!(levels(0.5), 1);
        assert_eq!(levels(1.0e6), MAX_LEVELS);
    }
}
