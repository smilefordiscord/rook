//! Terrain's Clouds on the GPU (`shaders/sky/clouds.wgsl`;
//! docs/behavior/lighting/clouds.md), in the passes Studio's cloud shaders
//! show (`shaders_glsl3.pack`): the layer's shadow from the sun, the march
//! at a quarter of the frame's pixels, the march blended with last frames'
//! at where they were (temporal reprojection), and that drawn over the sky
//! in the main pass. Nothing is made or drawn while no Clouds are: the
//! pipelines and textures are made the first time clouds are drawn, the
//! targets follow the frame's size, and each frame costs the same however
//! big the place is.

mod noise;
mod pipelines;
mod targets;

use std::sync::Arc;

use glam::Mat4;

use super::*;
use pipelines::CloudPipelines;
use targets::Targets;

/// How far the cloud layer's units are from studs: Studio's shaders take
/// the camera's position times this, in kilometers (1 stud is 0.28 m).
const KM_PER_STUD: f64 = 0.00028;

/// The field repeats every this many kilometers across, and the detail
/// and flow a whole number of times within it, so the drift wraps here.
const FIELD_TILE_KM: f64 = 32.0;

/// The detail's and flow's scroll repeats after this much change of shape
/// (the shaders scroll them 39.0625 and 17.5 per unit; each a whole number
/// of their tiles in 6.4).
const MORPH_PERIOD: f64 = 6.4;

/// How much the clouds change shape per kilometer the wind carries them
/// (Approximation: Studio's clock for it is fed by its CPU side; this
/// scrolls the detail about half as fast as the wind).
const MORPH_PER_KM: f64 = 0.0128;

/// Samples along each ray through the layer (Approximation: Studio's count
/// at Automatic isn't in the pack; at 9 or fewer its shaders read coarser
/// texture levels).
const STEPS: f32 = 20.0;

/// The detail noise's strength and how fine it is (Approximation: Studio's
/// CPU side feeds both).
const DETAIL: [f32; 2] = [0.12, 1.0];

/// Above this height (studs) the clouds thin out, gone 1,000 studs higher
/// (Studio's shaders).
const THIN_ABOVE: f32 = 6000.0;

/// Cloud textures: a field low where there's cloud, the detail noise that
/// adds to it, and the flow both are pushed along. The install's
/// (`sky/clouds.dds`, `cloudDetail3D.dds`, `cloudAdvection.dds`), or Rook's
/// own.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudNoise {
    /// Across (x), down (z) and up through the layer, repeating across: one
    /// tile is 32 km.
    pub field: Volume,
    /// Repeating on every side.
    pub detail: Volume,
    /// Two channels, X then Z, 0.5 for none; one deep.
    pub flow: Volume,
}

/// A texture of bytes, one or two channels a texel, in levels.
#[derive(Debug, Clone, PartialEq)]
pub struct Volume {
    /// Texels across, down and deep (1 deep for a flat one).
    pub size: [u32; 3],
    /// Bytes a texel: 1 or 2.
    pub channels: u32,
    /// Level 0 first; level `i` is each side halved `i` times (at least
    /// 1), layer by layer, row by row.
    pub levels: Vec<Vec<u8>>,
}

impl Volume {
    /// A volume from its first level, with each level after a box filter
    /// of the one before, down to one texel.
    pub fn with_levels(size: [u32; 3], channels: u32, level0: Vec<u8>) -> Self {
        let mut levels = vec![level0];
        let mut at = size;
        while at.iter().any(|&s| s > 1) {
            let next = at.map(|s| (s / 2).max(1));
            let last = levels.last().expect("level 0");
            let mut half = Vec::with_capacity((next[0] * next[1] * next[2] * channels) as usize);
            for z in 0..next[2] {
                for y in 0..next[1] {
                    for x in 0..next[0] {
                        for c in 0..channels {
                            let (mut sum, mut n) = (0u32, 0u32);
                            for dz in 0..(at[2] / next[2]) {
                                for dy in 0..(at[1] / next[1]) {
                                    for dx in 0..(at[0] / next[0]) {
                                        let (sx, sy, sz) = (
                                            x * at[0] / next[0] + dx,
                                            y * at[1] / next[1] + dy,
                                            z * at[2] / next[2] + dz,
                                        );
                                        let i = ((sz * at[1] + sy) * at[0] + sx) * channels + c;
                                        sum += u32::from(last[i as usize]);
                                        n += 1;
                                    }
                                }
                            }
                            half.push(((sum + n / 2) / n) as u8);
                        }
                    }
                }
            }
            levels.push(half);
            at = next;
        }
        Self {
            size,
            channels,
            levels,
        }
    }

    /// Whether its levels hold what its size and channels say.
    fn well_formed(&self) -> bool {
        (1..=2).contains(&self.channels)
            && self.size.iter().all(|&s| s > 0)
            && !self.levels.is_empty()
            && self.levels.iter().enumerate().all(|(i, level)| {
                let side = self.size.map(|s| (s >> i).max(1));
                level.len() == (side[0] * side[1] * side[2] * self.channels) as usize
            })
    }
}

impl CloudNoise {
    /// Whether the renderer can draw with these: a field more than one
    /// texel deep, a flat flow of two channels, one channel elsewhere.
    pub(crate) fn usable(&self) -> bool {
        [&self.field, &self.detail, &self.flow]
            .iter()
            .all(|v| v.well_formed())
            && self.field.channels == 1
            && self.detail.channels == 1
            && self.field.size[2] > 1
            && self.detail.size[2] > 1
            && self.flow.channels == 2
            && self.flow.size[2] == 1
    }
}

/// What the cloud shaders read (`Clouds` in `shaders/sky/clouds.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    /// Cover, density (thinned by the camera's height), steps, the
    /// detail's strength.
    shape: [f32; 4],
    /// Color in light units; how fine the detail is.
    color: [f32; 4],
    /// How far the wind has carried the field (km), and the change of
    /// shape.
    drift: [f32; 4],
    /// The march's size in pixels, then the frame's.
    sizes: [f32; 4],
    /// World to last frame's clip space.
    last_view_proj: [[f32; 4]; 4],
    /// x: 1 when last frame's clouds can't be blended in.
    last: [f32; 4],
}

/// What last frame's clouds were drawn with.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LastFrame {
    view_proj: Mat4,
    clouds: Clouds,
}

/// The cloud passes' GPU objects, made the first time clouds are drawn.
#[derive(Debug)]
struct Drawing {
    pipelines: CloudPipelines,
    params: wgpu::Buffer,
    /// The textures' bind group, and the shadow map's (`pipelines.rs`).
    shapes: wgpu::BindGroup,
    shadow: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    targets: Option<Targets>,
    last: Option<LastFrame>,
}

/// Terrain's Clouds on the GPU, and how far the wind has carried them.
#[derive(Debug, Default)]
pub(crate) struct GpuClouds {
    /// The install's textures, when given; Rook's own otherwise.
    given: Option<Arc<CloudNoise>>,
    drawing: Option<Box<Drawing>>,
    /// Kilometers the wind has carried the field, wrapped at its tile.
    drift: [f64; 3],
    /// How far the clouds have changed shape, wrapped.
    morph: f64,
}

impl GpuClouds {
    /// Move the clouds on by `dt` seconds of `wind` (`Workspace.GlobalWind`,
    /// studs a second): the field drifts with the wind (Inference: Studio's
    /// shaders offset where they read the field by a vector its CPU feeds;
    /// taken here as the wind's own distance) and changes shape with how
    /// far it went.
    pub(crate) fn advance(&mut self, dt: f32, wind: Vec3) {
        let dt = f64::from(dt);
        for (axis, w) in [wind.x, wind.y, wind.z].into_iter().enumerate() {
            // The field is read at where a point is plus the drift, so it
            // moves the other way.
            self.drift[axis] =
                (self.drift[axis] - f64::from(w) * KM_PER_STUD * dt).rem_euclid(FIELD_TILE_KM);
        }
        let carried = f64::from(wind.length()) * KM_PER_STUD * dt;
        self.morph = (self.morph + carried * MORPH_PER_KM).rem_euclid(MORPH_PERIOD);
    }

    /// Draw with `noise` from now on. False, keeping what it had, if the
    /// textures aren't laid out as the shaders read them.
    pub(crate) fn set_noise(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        noise: &CloudNoise,
    ) -> bool {
        if !noise.usable() {
            return false;
        }
        let noise = Arc::new(noise.clone());
        if let Some(drawing) = &mut self.drawing {
            drawing.shapes = drawing
                .pipelines
                .shapes_group(device, queue, &drawing.params, &noise);
            drawing.last = None;
        }
        self.given = Some(noise);
        true
    }

    /// Ready this frame's clouds, `clouds` as `camera` sees them in a
    /// `width` × `height` frame: the GPU objects made if they aren't yet,
    /// the targets sized and what the shaders read written. Returns
    /// whether there are clouds to draw (`Cover` above 0).
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        (layouts, frame_format): ([&wgpu::BindGroupLayout; 2], wgpu::TextureFormat),
        (clouds, camera, (width, height)): (Option<Clouds>, &Camera, (u32, u32)),
    ) -> bool {
        // Cover 0 draws none (Inference: the field is a distance, never
        // below 0, and only what's below Cover is cloud).
        let Some(clouds) = clouds.filter(|c| c.cover > 0.0) else {
            if let Some(drawing) = &mut self.drawing {
                drawing.last = None;
            }
            return false;
        };
        let given = self.given.clone();
        let drawing = self.drawing.get_or_insert_with(|| {
            let pipelines = CloudPipelines::new(device, layouts, frame_format);
            let params = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rook clouds"),
                size: std::mem::size_of::<Params>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let noise = given.as_deref().unwrap_or_else(|| noise::rooks_own());
            let shapes = pipelines.shapes_group(device, queue, &params, noise);
            let (shadow_view, shadow) = pipelines.shadow_map(device);
            Box::new(Drawing {
                pipelines,
                params,
                shapes,
                shadow,
                shadow_view,
                targets: None,
                last: None,
            })
        });
        let march = [width.div_ceil(2), height.div_ceil(2)];
        if drawing.targets.as_ref().is_none_or(|t| t.size != march) {
            let targets = Targets::new(device, &drawing.pipelines, &drawing.params, march);
            drawing.targets = Some(targets);
            drawing.last = None;
        }
        let aspect = width as f32 / height.max(1) as f32;
        let view_proj = camera.view_projection(aspect);
        let last = drawing.last.filter(|l| l.clouds == clouds);
        let thin = (1.0 + 0.001 * (THIN_ABOVE - camera.position.y)).clamp(0.0, 1.0);
        let [r, g, b] = clouds.color;
        let params = Params {
            shape: [
                clouds.cover.clamp(0.0, 1.0),
                clouds.density.clamp(0.0, 1.0) * thin,
                STEPS,
                DETAIL[0],
            ],
            color: [r, g, b, DETAIL[1]],
            drift: [
                self.drift[0] as f32,
                self.drift[1] as f32,
                self.drift[2] as f32,
                self.morph as f32,
            ],
            sizes: [
                march[0] as f32,
                march[1] as f32,
                width as f32,
                height as f32,
            ],
            last_view_proj: last.map_or(view_proj, |l| l.view_proj).to_cols_array_2d(),
            last: [if last.is_some() { 0.0 } else { 1.0 }, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&drawing.params, 0, bytemuck::bytes_of(&params));
        drawing.last = Some(LastFrame { view_proj, clouds });
        true
    }

    /// The passes before the main pass: the shadow map, the march and
    /// the blend with last frames', each with the globals and the main
    /// group (the reflection cube the haze reads) bound as for drawing.
    pub(crate) fn record(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        groups: [&wgpu::BindGroup; 2],
    ) {
        let Some(drawing) = &mut self.drawing else {
            return;
        };
        let Some(targets) = &mut drawing.targets else {
            return;
        };
        let p = &drawing.pipelines;
        let pass = |encoder: &mut wgpu::CommandEncoder,
                    label,
                    views: &[&wgpu::TextureView],
                    pipeline: &wgpu::RenderPipeline,
                    bound: &[&wgpu::BindGroup]| {
            let attachments: Vec<_> = views
                .iter()
                .map(|view| {
                    Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })
                })
                .collect();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            for (i, group) in groups.iter().chain(bound).enumerate() {
                pass.set_bind_group(i as u32, *group, &[]);
            }
            pass.draw(0..3, 0..1);
        };
        pass(
            encoder,
            "rook cloud shadows",
            &[&drawing.shadow_view],
            &p.shadow,
            &[&drawing.shapes],
        );
        pass(
            encoder,
            "rook clouds march",
            &[&targets.march_color, &targets.march_dist],
            &p.march,
            &[&drawing.shapes, &drawing.shadow],
        );
        let newest = 1 - targets.newest;
        let [color, dist] = &targets.history[newest];
        pass(
            encoder,
            "rook clouds blend",
            &[color, dist],
            &p.resolve,
            &[&targets.resolve[targets.newest]],
        );
        targets.newest = newest;
    }

    /// Draw the clouds over the sky, in the main pass with the globals and
    /// the main group bound: before the parts, which cover them. Leaves
    /// its own group bound in group 2.
    pub(crate) fn draw_over(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(drawing) = &self.drawing else {
            return;
        };
        let Some(targets) = &drawing.targets else {
            return;
        };
        pass.set_pipeline(&drawing.pipelines.over);
        pass.set_bind_group(2, &targets.over[targets.newest], &[]);
        pass.draw(0..3, 0..1);
    }
}
