//! What the sky draws besides its skybox: the sun and moon, the stars and
//! the night's gradient (docs/behavior/lighting/sky.md).

use super::*;

/// The sun or the moon, drawn as a quad facing the camera at infinity
/// over the skybox, as Studio's `AdvSun` shader draws it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Disc {
    /// Unit vector towards it.
    pub direction: Vec3,
    /// How far its image spans, in radians (`SunAngularSize`,
    /// `MoonAngularSize`).
    pub size: f32,
    pub image: DiscImage,
    /// The light units its image's white draws at, per channel, before
    /// the exposure.
    pub light: [f32; 3],
}

/// What a [`Disc`] draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscImage {
    /// Its texture (`SunTextureId`, `MoonTextureId`), once loaded, drawn
    /// as Studio's shaders take a texture: its sRGB color squared.
    Image(ImageHash),
    /// Rook's own soft white disc, for a texture that won't load (no
    /// Roblox install).
    Own,
}

/// The stars: `count` points two pixels across, as Studio's `AdvStars`
/// draws them, each at a direction and brightness made from its index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stars {
    /// `Sky.StarCount`.
    pub count: u32,
    /// How far the sky has turned about the Z axis with the time of day,
    /// in radians, as the sun turns.
    pub turn: f32,
    /// The light units the brightest star draws at, the night's share
    /// included.
    pub light: f32,
}

/// The night sky's gradient, under the skybox's tinted images: sRGB
/// colors overhead and along the horizon (fitted looks, through the tone
/// curve's inverse as the plain sky is), and how much of it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NightSky {
    pub top: [f32; 3],
    pub horizon: [f32; 3],
    /// 0 by day to 1 at night.
    pub shown: f32,
}
