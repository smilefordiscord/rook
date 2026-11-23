use rook_content::decode::image::{Encoding, Image};
use rook_content::decode::mesh::Mesh;
use rook_content::{ContentRef, Resolver};
use rook_engine::console;
use rook_lua::MessageType;
use rook_render::{CloudNoise, Renderer, Volume};

/// What the renderer takes from the user's Roblox install, read from disk
/// at startup: the Head MeshType's mesh, the surface patterns, water's
/// normal frames, the BRDF table reflections go by and the cloud textures.
/// Without an install (or the files), Rook draws its own (and flat water).
#[derive(Debug)]
pub struct Installed {
    pub head: Option<Mesh>,
    /// `textures/studs.dds`: the studs, lattice, inlet and universal tiles.
    pub surfaces: Option<Image>,
    /// `textures/water/normal_01.dds` to `normal_25.dds`, when all load.
    pub water: Option<Vec<Image>>,
    /// `textures/brdfLUT.dds`: its side and its pairs of half floats.
    pub brdf: Option<(u32, Vec<u8>)>,
    /// `sky/clouds.dds`, `sky/cloudDetail3D.dds` and
    /// `sky/cloudAdvection.dds`, when all three load.
    pub clouds: Option<CloudNoise>,
}

/// How many water normal frames the install has (measured: Studio plays 25).
const WATER_FRAMES: usize = 25;

impl Installed {
    pub fn find(content: &Resolver) -> Self {
        let read = |path: &str| {
            let fetched = content.resolve_local(&ContentRef::File(path.into()))?;
            Some((fetched.source.location, fetched.bytes))
        };
        let warn = |location: &str, e: &dyn std::fmt::Display, what: &str| {
            console::print_message(
                MessageType::Warning,
                &format!("{location}: {e}; drawing Rook's own {what}"),
            );
        };
        let head = read("avatar/heads/head.mesh")
            .and_then(|(at, bytes)| Mesh::decode(&bytes).map_err(|e| warn(&at, &e, "head")).ok());
        let surfaces = read("textures/studs.dds").and_then(|(at, bytes)| {
            Image::decode(&bytes)
                .map_err(|e| warn(&at, &e, "surfaces"))
                .ok()
        });
        let water = (1..=WATER_FRAMES)
            .map(|i| {
                let (at, bytes) = read(&format!("textures/water/normal_{i:02}.dds"))?;
                Image::decode(&bytes)
                    .map_err(|e| warn(&at, &e, "flat water"))
                    .ok()
            })
            .collect();
        let brdf = read("textures/brdfLUT.dds").and_then(|(at, bytes)| {
            brdf_table(&bytes)
                .ok_or_else(|| warn(&at, &"not a square R16G16_FLOAT DDS", "BRDF table"))
                .ok()
        });
        let volume = |path: &str| {
            let (at, bytes) = read(path)?;
            r8_volume(&bytes)
                .ok_or_else(|| warn(&at, &"not a 3D R8_UNORM DDS", "clouds"))
                .ok()
        };
        let flow = read("sky/cloudAdvection.dds").and_then(|(at, bytes)| {
            Image::decode(&bytes)
                .map(|image| two_channels(&image))
                .map_err(|e| warn(&at, &e, "clouds"))
                .ok()
                .flatten()
        });
        let clouds = (volume("sky/clouds.dds"))
            .zip(volume("sky/cloudDetail3D.dds"))
            .zip(flow)
            .map(|((field, detail), flow)| CloudNoise {
                field,
                detail,
                flow,
            });
        Self {
            head,
            surfaces,
            water,
            brdf,
            clouds,
        }
    }

    pub fn apply(&self, renderer: &mut Renderer) {
        if let Some(head) = &self.head {
            renderer.set_head_mesh(head);
        }
        if let Some(atlas) = &self.surfaces
            && !renderer.set_surface_atlas(atlas)
        {
            console::print_message(
                MessageType::Warning,
                "textures/studs.dds isn't laid out as expected; drawing Rook's own surfaces",
            );
        }
        if let Some((size, texels)) = &self.brdf
            && !renderer.set_brdf_table(*size, texels)
        {
            console::print_message(
                MessageType::Warning,
                "textures/brdfLUT.dds isn't laid out as expected; drawing Rook's own BRDF table",
            );
        }
        if let Some(noise) = &self.clouds
            && !renderer.set_cloud_noise(noise)
        {
            console::print_message(
                MessageType::Warning,
                "sky/clouds.dds, cloudDetail3D.dds and cloudAdvection.dds aren't laid out as expected; drawing Rook's own clouds",
            );
        }
        if let Some(frames) = &self.water
            && !renderer.set_water_normals(frames)
        {
            console::print_message(
                MessageType::Warning,
                "textures/water/normal_*.dds aren't all one square size; drawing flat water",
            );
        }
    }
}

/// The texels of `brdfLUT.dds`: a square DX10 DDS of R16G16_FLOAT
/// (DXGI format 34) with one level, as the install's is. Returns its side
/// and the texels.
fn brdf_table(bytes: &[u8]) -> Option<(u32, Vec<u8>)> {
    /// The magic, the header and its DX10 extension.
    const HEADER: usize = 4 + 124 + 20;
    const R16G16_FLOAT: u32 = 34;
    let u32_at = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let (height, width) = (u32_at(12)?, u32_at(16)?);
    let four_cc = bytes.get(84..88)?;
    if !bytes.starts_with(b"DDS ")
        || four_cc != b"DX10"
        || u32_at(128)? != R16G16_FLOAT
        || width != height
    {
        return None;
    }
    let texels = bytes.get(HEADER..HEADER + (width * height * 4) as usize)?;
    Some((width, texels.to_vec()))
}

/// The texels of a DX10 DDS of R8_UNORM (DXGI format 61), as the install's
/// cloud field and detail are: its width, height and depth, and each of
/// its levels.
fn r8_volume(bytes: &[u8]) -> Option<Volume> {
    /// The magic, the header and its DX10 extension.
    const HEADER: usize = 4 + 124 + 20;
    const R8_UNORM: u32 = 61;
    let u32_at = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let (height, width, depth) = (u32_at(12)?, u32_at(16)?, u32_at(24)?.max(1));
    let levels = u32_at(28)?.max(1);
    if !bytes.starts_with(b"DDS ") || bytes.get(84..88)? != b"DX10" || u32_at(128)? != R8_UNORM {
        return None;
    }
    let mut at = HEADER;
    let mut out = Vec::new();
    for i in 0..levels {
        let side = [width, height, depth].map(|s| (s >> i).max(1));
        let len = (side[0] * side[1] * side[2]) as usize;
        out.push(bytes.get(at..at + len)?.to_vec());
        at += len;
    }
    Some(Volume {
        size: [width, height, depth],
        channels: 1,
        levels: out,
    })
}

/// An RGBA image's red and green, level by level, as a flat volume.
fn two_channels(image: &Image) -> Option<Volume> {
    if image.encoding != Encoding::Rgba8 {
        return None;
    }
    let levels = (image.mips.iter())
        .map(|level| {
            level
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1]])
                .collect()
        })
        .collect();
    Some(Volume {
        size: [image.width, image.height, 1],
        channels: 2,
        levels,
    })
}
