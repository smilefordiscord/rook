//! BlurEffect: a separable Gaussian over the frame, as Studio's `Blur1FS`
//! to `Blur7FS` draw it (`shaders_glsl3.pack`): the frame's encoded
//! values, a center texel and pairs of neighbors each read by one
//! filtered tap between them. A wide blur runs on a copy of the frame
//! shrunk by a power of two, so a pass never reads more than 17 taps
//! (docs/behavior/lighting/post-processing.md#blureffect).

use super::*;

/// The Gaussian's sigma in texels of the copy it runs on, at most: a
/// wider one runs on a smaller copy.
const SIGMA_TEXELS: f32 = 4.0;

/// The most tap pairs a pass reads on each side.
const PAIRS: f32 = 8.0;

/// One blur pass's parameters (`shaders/post/blur.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct BlurPass {
    /// xy: one source texel along the pass's axis, in texture
    /// coordinates; z: sigma in texels; w: tap pairs.
    step: [f32; 4],
    /// x: frame texels per texel of the copy, each way.
    down: [f32; 4],
}

/// How a Blur of some `Size` runs at some resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Plan {
    /// Frame pixels per texel of the copy it blurs, each way.
    factor: u32,
    /// The copy's size.
    size: (u32, u32),
    sigma: f32,
    pairs: f32,
}

impl Plan {
    /// `blur_size` is `BlurEffect.Size`, in pixels at the screen's
    /// resolution: the Gaussian's sigma is half of it (Approximation: the
    /// creator docs call Size the blur's radius; Studio's width for a
    /// Size isn't measured).
    fn new(blur_size: f32, (width, height): (u32, u32)) -> Self {
        let sigma = blur_size / 2.0;
        let steps = (sigma / SIGMA_TEXELS).log2().ceil().clamp(0.0, 6.0) as u32;
        let factor = 1 << steps;
        let sigma = sigma / factor as f32;
        Self {
            factor,
            size: (width.div_ceil(factor), height.div_ceil(factor)),
            sigma,
            pairs: (1.5 * sigma).ceil().clamp(1.0, PAIRS),
        }
    }
}

/// Its textures at one size: the copy and the pass between, and the
/// groups reading the frame, the copy and the pass between.
#[derive(Debug)]
struct Textures {
    frame_size: (u32, u32),
    /// The frame `from_frame` reads: a resize makes a new one, which can
    /// come back to this size between two frames that draw the Blur.
    frame: wgpu::TextureView,
    factor: u32,
    copy: Target,
    across: Target,
    from_frame: wgpu::BindGroup,
    from_copy: wgpu::BindGroup,
    from_across: wgpu::BindGroup,
}

/// The most passes the Blur runs in a frame: a copy, then across and
/// down.
pub(super) const SLOTS: u64 = 3;

impl Textures {
    /// `slot`'s textures, made again unless they fit `frame` and `plan`.
    fn at<'a>(
        slot: &'a mut Option<Self>,
        device: &wgpu::Device,
        shared: &Shared,
        (frame, frame_size): (&wgpu::TextureView, (u32, u32)),
        plan: &Plan,
    ) -> &'a Self {
        let fresh =
            |t: &Self| t.frame_size == frame_size && t.frame == *frame && t.factor == plan.factor;
        if !slot.as_ref().is_some_and(fresh) {
            let copy = Shared::target(device, "rook blur", plan.size);
            let across = Shared::target(device, "rook blur across", plan.size);
            *slot = Some(Self {
                frame_size,
                frame: frame.clone(),
                factor: plan.factor,
                from_frame: shared.group(device, frame, None),
                from_copy: shared.group(device, &copy.view, None),
                from_across: shared.group(device, &across.view, None),
                copy,
                across,
            });
        }
        slot.as_ref().expect("made above")
    }
}

#[derive(Debug)]
pub(crate) struct Blur {
    down: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    textures: Option<Textures>,
}

impl Blur {
    pub(super) fn new(device: &wgpu::Device, shared: &Shared, module: &wgpu::ShaderModule) -> Self {
        Self {
            down: shared.pipeline(device, module, "fs_down", None),
            blur: shared.pipeline(device, module, "fs_blur", None),
            textures: None,
        }
    }

    /// The frame (`size` pixels) blurred by a `blur_size` Blur, when it's
    /// above 0: a texture in the frame's encoding, maybe smaller than it.
    pub(super) fn record(
        &mut self,
        device: &wgpu::Device,
        shared: &mut Shared,
        encoder: &mut wgpu::CommandEncoder,
        frame: &wgpu::TextureView,
        size: (u32, u32),
        blur_size: f32,
    ) -> Option<wgpu::TextureView> {
        if blur_size <= 0.0 {
            return None;
        }
        let plan = Plan::new(blur_size, size);
        let t = Textures::at(&mut self.textures, device, shared, (frame, size), &plan);
        let (w, h) = (plan.size.0 as f32, plan.size.1 as f32);
        let pass = |step: [f32; 2]| BlurPass {
            step: [step[0], step[1], plan.sigma, plan.pairs],
            down: [plan.factor as f32, 0.0, 0.0, 0.0],
        };
        let (down, blur) = (&self.down, &self.blur);
        let source = if plan.factor > 1 {
            let slot = shared.slots.push(&pass([0.0; 2]));
            Shared::run(
                encoder,
                "rook blur copy",
                &t.copy.view,
                down,
                &t.from_frame,
                slot,
                false,
            );
            &t.from_copy
        } else {
            &t.from_frame
        };
        let slot = shared.slots.push(&pass([1.0 / w, 0.0]));
        Shared::run(
            encoder,
            "rook blur across",
            &t.across.view,
            blur,
            source,
            slot,
            false,
        );
        let slot = shared.slots.push(&pass([0.0, 1.0 / h]));
        Shared::run(
            encoder,
            "rook blur down",
            &t.copy.view,
            blur,
            &t.from_across,
            slot,
            false,
        );
        Some(t.copy.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A narrow blur runs on the frame itself; a wide one on a copy small
    /// enough that its sigma stays within the taps a pass reads.
    #[test]
    fn wide_blurs_run_on_smaller_copies() {
        let narrow = Plan::new(4.0, (1920, 1080));
        assert_eq!((narrow.factor, narrow.size), (1, (1920, 1080)));
        assert_eq!((narrow.sigma, narrow.pairs), (2.0, 3.0));
        let wide = Plan::new(32.0, (1920, 1080));
        assert_eq!((wide.factor, wide.size), (4, (480, 270)));
        assert!(wide.sigma <= SIGMA_TEXELS && wide.pairs <= PAIRS);
    }
}
