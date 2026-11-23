//! SunRaysEffect: light from the sun streaming past what stands in front
//! of it, as Studio's `SunRays*FS` and `RadialBlur*FS` draw it
//! (`shaders_glsl3.pack`). At half the frame's size, each pixel marches
//! half the way towards the sun's place on screen (stopping at the
//! screen's edge) and counts the steps that reach the sky, which a depth
//! of 499.5 studs or more counts as; that share, times a lobe around the
//! sun, is its light. A radial blur towards the sun then streaks it
//! (docs/behavior/lighting/post-processing.md#sunrayseffect).

use super::*;

/// The steps each pixel marches (Studio's shaders come with 6, 9 and 12).
const STEPS: f32 = 12.0;

/// One sun rays pass's parameters (`shaders/post/sun_rays.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct RaysPass {
    /// From clip space to directions from the camera.
    from_clip: [[f32; 4]; 4],
    /// xy: the sun's place in texture coordinates; zw: one texel of the
    /// texture drawn.
    sun: [f32; 4],
    /// xyz: towards the sun; w: the lobe's anisotropy.
    direction: [f32; 4],
    /// rgb: the light at the sun, in light units; w: steps.
    light: [f32; 4],
}

/// The lobe's anisotropy for `SunRaysEffect.Spread` (0 to 1): a
/// Henyey–Greenstein lobe, as Studio's shader weighs the rays
/// (Approximation: how Spread becomes it isn't in the shader; this gives
/// a halo about 15 degrees wide at Spread 1 and under a degree at 0).
fn anisotropy(spread: f32) -> f32 {
    (1.0 - 0.2 * spread.clamp(0.0, 1.0)).min(0.99)
}

/// The sun's place on screen, in texture coordinates (y down), when it's
/// in front of the camera.
fn sun_on_screen(camera: &Camera, aspect: f32, towards: Vec3) -> Option<[f32; 2]> {
    let clip = camera.view_projection(aspect) * towards.extend(0.0);
    (clip.w > 0.0).then(|| {
        let ndc = clip.truncate().truncate() / clip.w;
        [ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5]
    })
}

/// Its textures at one frame size, and the groups reading the depth
/// buffer and the marched rays.
#[derive(Debug)]
struct Textures {
    frame_size: (u32, u32),
    /// The depth buffer `from_depth` reads: a resize makes a new one,
    /// which can come back to this size between two frames with rays.
    depth: wgpu::TextureView,
    size: (u32, u32),
    marched: Target,
    streaked: Target,
    from_depth: wgpu::BindGroup,
    from_marched: wgpu::BindGroup,
}

/// The passes the rays run in a frame that take parameters: the streak
/// reads the march's slot.
pub(super) const SLOTS: u64 = 1;

impl Textures {
    fn at<'a>(
        slot: &'a mut Option<Self>,
        device: &wgpu::Device,
        shared: &Shared,
        view: &PostView,
    ) -> &'a Self {
        if !(slot.as_ref()).is_some_and(|t| t.frame_size == view.size && t.depth == *view.depth) {
            let size = (view.size.0.div_ceil(2), view.size.1.div_ceil(2));
            let marched = Shared::target(device, "rook sun rays", size);
            let streaked = Shared::target(device, "rook sun rays streaked", size);
            *slot = Some(Self {
                frame_size: view.size,
                depth: view.depth.clone(),
                size,
                from_depth: shared.depth_group(device, &shared.black, view.depth),
                from_marched: shared.group(device, &marched.view, None),
                marched,
                streaked,
            });
        }
        slot.as_ref().expect("made above")
    }
}

#[derive(Debug)]
pub(crate) struct SunRaysPasses {
    march: wgpu::RenderPipeline,
    streak: wgpu::RenderPipeline,
    textures: Option<Textures>,
}

impl SunRaysPasses {
    pub(super) fn new(device: &wgpu::Device, shared: &Shared, module: &wgpu::ShaderModule) -> Self {
        Self {
            march: shared.depth_pipeline(device, module, "fs_march"),
            streak: shared.pipeline(device, module, "fs_streak", None),
            textures: None,
        }
    }

    /// The rays' light, in light units at half the frame's size, when
    /// `rays` is on and the sun is in front of the camera.
    pub(super) fn record(
        &mut self,
        device: &wgpu::Device,
        shared: &mut Shared,
        encoder: &mut wgpu::CommandEncoder,
        view: &PostView,
        rays: Option<SunRays>,
    ) -> Option<wgpu::TextureView> {
        let rays = rays?;
        let aspect = view.size.0 as f32 / view.size.1.max(1) as f32;
        let towards = view.lighting.sun_direction.normalize_or(Vec3::Y);
        let sun = sun_on_screen(view.camera, aspect, towards)?;
        let t = Textures::at(&mut self.textures, device, shared, view);
        let c = view.lighting.sun_color.map(|c| c * rays.intensity);
        let pass = RaysPass {
            from_clip: view.camera.sky_from_clip(aspect).to_cols_array_2d(),
            sun: [sun[0], sun[1], 1.0 / t.size.0 as f32, 1.0 / t.size.1 as f32],
            direction: towards.extend(anisotropy(rays.spread)).to_array(),
            light: [c[0], c[1], c[2], STEPS],
        };
        let slot = shared.slots.push(&pass);
        let march = &self.march;
        Shared::run(
            encoder,
            "rook sun rays",
            &t.marched.view,
            march,
            &t.from_depth,
            slot,
            false,
        );
        let streak = &self.streak;
        let marched = &t.from_marched;
        Shared::run(
            encoder,
            "rook sun rays streak",
            &t.streaked.view,
            streak,
            marched,
            slot,
            false,
        );
        Some(t.streaked.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sun straight ahead is in the middle of the screen; behind the
    /// camera it's nowhere.
    #[test]
    fn the_sun_has_a_place_only_in_front() {
        let camera = Camera::look_at(Vec3::ZERO, Vec3::NEG_Z);
        let ahead = sun_on_screen(&camera, 1.0, Vec3::NEG_Z).unwrap();
        assert!((ahead[0] - 0.5).abs() < 1e-5 && (ahead[1] - 0.5).abs() < 1e-5);
        let up = sun_on_screen(&camera, 1.0, Vec3::new(0.0, 0.2, -1.0)).unwrap();
        assert!(up[1] < 0.5);
        assert_eq!(sun_on_screen(&camera, 1.0, Vec3::Z), None);
    }

    #[test]
    fn spread_widens_the_halo() {
        assert!(anisotropy(1.0) < anisotropy(0.0));
        assert!(anisotropy(0.0) < 1.0);
    }
}
