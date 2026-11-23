//! What goes to the GPU: the frame's globals, each instance's record, meshes and images, and the main pass's bind groups.

pub(crate) mod cache;
mod images;
mod instance;
mod main_group;
pub(crate) mod pool;

pub(crate) use images::*;
pub(crate) use instance::*;
pub(crate) use main_group::*;

use super::*;
use crate::effects::particles::Rng;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Globals {
    pub(crate) view_proj: [[f32; 4]; 4],
    pub(crate) camera_pos: [f32; 4],
    pub(crate) sun_dir: [f32; 4],
    /// `Lighting::sun_color`, in light units.
    pub(crate) sun_color: [f32; 4],
    pub(crate) ambient: [f32; 4],
    pub(crate) outdoor_ambient: [f32; 4],
    pub(crate) fog_color: [f32; 4],
    pub(crate) fog_range: [f32; 4],
    pub(crate) shadow_view_proj: [[f32; 4]; 4],
    /// A texel in uv, the normal offset, the depth bias, and how far
    /// shadows reach (0 for none).
    pub(crate) shadow: [f32; 4],
    /// Each face's image right and up (`NormalId::image_axes`), in
    /// `NormalId` order; w holds the studs per surface pattern tile.
    pub(crate) faces: [[f32; 4]; 12],
    /// From clip space to directions from the camera, for the skybox.
    pub(crate) sky_from_clip: [[f32; 4]; 4],
    /// Each skybox image's direction, right and up ([`SKYBOX_FACES`]).
    pub(crate) sky_faces: [[f32; 4]; 18],
    /// Seconds on the renderer's clock, the waves' size and speed, and how
    /// far grass is drawn ([`TerrainLook`]).
    pub(crate) terrain: [f32; 4],
    /// `TerrainLook::wind`; w is water's phase (`Renderer::water_phase`).
    pub(crate) wind: [f32; 4],
    /// The sky's color overhead, at the horizon and below
    /// (`Lighting::sky_top`, `sky_horizon`, `sky_bottom`, sRGB): water
    /// reflects the first two, and all three light surfaces. `sky_top`'s w
    /// is `Lighting::environment_diffuse`, `sky_horizon`'s
    /// `Lighting::environment_specular`.
    pub(crate) sky_top: [f32; 4],
    pub(crate) sky_horizon: [f32; 4],
    pub(crate) sky_bottom: [f32; 4],
    pub(crate) sky_tint: [f32; 4],
    /// Each skybox image's slot in the image table, when there is one,
    /// then the sun's and the moon's images'.
    pub(crate) sky_images: [[u32; 4]; 2],
    /// From world directions to the skybox's own (the inverse of
    /// `Lighting::skybox_turn`), for reading the reflection cube.
    pub(crate) sky_turn: [[f32; 4]; 3],
    /// The sun's and the moon's quads ([`Disc`]), two rows each: the
    /// direction and the tangent of half its size; its light and what it
    /// draws (0 nothing, 1 its image, 2 Rook's own disc).
    pub(crate) discs: [[f32; 4]; 4],
    /// `Lighting::night_sky`: the gradient's top (w how much shows) and
    /// horizon, sRGB.
    pub(crate) night_top: [f32; 4],
    pub(crate) night_horizon: [f32; 4],
    /// `Lighting::stars`: the count, turn and light.
    pub(crate) stars: [f32; 4],
}

impl Upload for Mesh {
    type Gpu = GpuMesh;
    type Context<'a> = MeshPool;

    fn upload_size(&self) -> usize {
        self.positions.len() * std::mem::size_of::<mesh::Vertex>() + self.indices.len() * 4
    }

    fn upload(&self, pool: &mut MeshPool) -> GpuMesh {
        profiling::scope!("upload mesh");
        pool.add(&mesh::files::from_file(self))
    }
}
