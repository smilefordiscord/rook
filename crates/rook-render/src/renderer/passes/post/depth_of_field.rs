//! DepthOfFieldEffect: what lies outside the focus blurred by its
//! distance, as Studio's DOF shaders split it (`shaders_glsl3.pack`:
//! `DOF*`, `Bokeh*CoCFS`): a far field and a near field, each gathered
//! from a disk of taps at half the frame's size, a tap counting where its
//! own circle of confusion reaches the pixel. A first pass halves the
//! frame into light units with each texel's far circle; the far gather
//! spreads it by the pixel's own circle; the near gather spreads near
//! things outwards over what's behind them, so their blur covers the
//! focus (docs/behavior/lighting/post-processing.md#depthoffieldeffect).

use super::*;

/// A circle of confusion of 1 at intensity 1 blurs this share of the
/// screen's height across its radius (Approximation: Studio's bokeh
/// shader scales its disk by 0.014 of the height times settings it's
/// given, unmeasured).
const RADIUS: f32 = 0.02;

/// One depth of field pass's parameters
/// (`shaders/post/depth_of_field.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct DofPass {
    /// x: `FocusDistance`; y: `InFocusRadius`, both in studs; z: the far
    /// field's radius and w: the near field's, at a circle of 1, as
    /// shares of the screen's height.
    focus: [f32; 4],
    /// xy: one texel of the texture drawn; z: the screen's height over
    /// its width.
    texel: [f32; 4],
}

/// Its textures at one frame size, and the groups reading them.
#[derive(Debug)]
struct Textures {
    frame_size: (u32, u32),
    size: (u32, u32),
    /// The frame halved, in light units, with each texel's far circle.
    field: Target,
    far: Target,
    near: Target,
    from_frame: wgpu::BindGroup,
    from_field: wgpu::BindGroup,
    from_field_and_depth: wgpu::BindGroup,
}

impl Textures {
    fn at<'a>(
        slot: &'a mut Option<Self>,
        device: &wgpu::Device,
        shared: &Shared,
        view: &PostView,
    ) -> &'a Self {
        if !slot.as_ref().is_some_and(|t| t.frame_size == view.size) {
            let size = (view.size.0.div_ceil(2), view.size.1.div_ceil(2));
            let field = Shared::target(device, "rook depth of field", size);
            *slot = Some(Self {
                frame_size: view.size,
                size,
                from_frame: shared.depth_group(device, view.frame, view.depth),
                from_field: shared.group(device, &field.view, None),
                from_field_and_depth: shared.depth_group(device, &field.view, view.depth),
                far: Shared::target(device, "rook depth of field far", size),
                near: Shared::target(device, "rook depth of field near", size),
                field,
            });
        }
        slot.as_ref().expect("made above")
    }
}

/// The far and near fields, blurred, for the tone pass.
#[derive(Debug)]
pub(super) struct Fields {
    /// rgb: the far field's light times a: the texel's far circle.
    pub(super) far: wgpu::TextureView,
    /// rgb: the near field's light; a: how much of it covers the pixel.
    pub(super) near: wgpu::TextureView,
}

#[derive(Debug)]
pub(crate) struct DepthOfFieldPasses {
    field: wgpu::RenderPipeline,
    far: wgpu::RenderPipeline,
    near: wgpu::RenderPipeline,
    textures: Option<Textures>,
}

impl DepthOfFieldPasses {
    pub(super) fn new(device: &wgpu::Device, shared: &Shared, module: &wgpu::ShaderModule) -> Self {
        Self {
            field: shared.depth_pipeline(device, module, "fs_field"),
            far: shared.pipeline(device, module, "fs_far", None),
            near: shared.depth_pipeline(device, module, "fs_near"),
            textures: None,
        }
    }

    pub(super) fn record(
        &mut self,
        device: &wgpu::Device,
        shared: &mut Shared,
        encoder: &mut wgpu::CommandEncoder,
        view: &PostView,
        dof: Option<DepthOfField>,
    ) -> Option<Fields> {
        let dof = dof?;
        let t = Textures::at(&mut self.textures, device, shared, view);
        let pass = DofPass {
            focus: [
                dof.focus_distance.max(0.0),
                dof.in_focus_radius.max(0.0),
                dof.far_intensity.max(0.0) * RADIUS,
                dof.near_intensity.max(0.0) * RADIUS,
            ],
            texel: [
                1.0 / t.size.0 as f32,
                1.0 / t.size.1 as f32,
                view.size.1 as f32 / view.size.0.max(1) as f32,
                0.0,
            ],
        };
        let slot = shared.slots.push(&pass);
        let field = (&t.field.view, &self.field);
        Shared::run(
            encoder,
            "rook depth of field",
            field.0,
            field.1,
            &t.from_frame,
            slot,
            false,
        );
        let far = (&t.far.view, &self.far);
        Shared::run(
            encoder,
            "rook depth of field far",
            far.0,
            far.1,
            &t.from_field,
            slot,
            false,
        );
        let near = (&t.near.view, &self.near);
        let group = &t.from_field_and_depth;
        Shared::run(
            encoder,
            "rook depth of field near",
            near.0,
            near.1,
            group,
            slot,
            false,
        );
        Some(Fields {
            far: t.far.view.clone(),
            near: t.near.view.clone(),
        })
    }
}
