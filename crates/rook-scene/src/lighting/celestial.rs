//! The Sky's sun and moon: where they are, how big, what they show
//! (docs/behavior/lighting/sky.md#the-sun-and-moon).

use rook_render::{Disc, DiscImage};

use super::sky::SkyImage;
use super::sun::{moon_direction, moon_fade, sun_color};
use super::*;

/// What a Sky's celestial bodies are without a Sky, or before its
/// properties were saved: a new Sky's defaults (creator-docs Sky). That a
/// place without a Sky shows them is an Inference.
const SUN_TEXTURE: &str = "rbxasset://sky/sun.jpg";
const MOON_TEXTURE: &str = "rbxasset://sky/moon.jpg";
const SUN_SIZE: f32 = 21.0;
const MOON_SIZE: f32 = 11.0;

/// The light units the sun's image draws at, by day, at its white
/// (Approximation: Studio's `AdvSun` scales the image's squared color by a
/// constant the pack doesn't hold; the disc in Studio is white at its
/// middle, which this reaches at the frame's brightest).
const SUN_LIGHT: f32 = 4.0;

/// The moon's, at its white (Approximation, as for the sun: a gray disc).
const MOON_LIGHT: f32 = 1.0;

impl SceneCache {
    /// The sun and moon over the sky of the Sky `sky` (a new Sky's without
    /// one), for the sun at `sun`, into `l`. None with
    /// `CelestialBodiesShown` off, at an angular size of 0 (both Confirmed:
    /// creator-docs Sky), or while its image loads.
    pub(super) fn celestial(
        &mut self,
        dom: &Dom,
        sky: Option<InstanceId>,
        sun: Vec3,
        l: &mut Lighting,
    ) {
        let bool_of = |name, default| {
            sky.and_then(|s| dom.bool_property(s, name))
                .unwrap_or(default)
        };
        if !bool_of("CelestialBodiesShown", true) {
            return;
        }
        // The sun takes its light's hue: white by day, yellow in the last
        // degrees above the horizon, gone below it (Inference).
        let hue = sun_color(sun.y).map(|c| c * 2.0 * SUN_LIGHT);
        if sun.y > 0.0 {
            l.sun_disc = self.disc(
                dom,
                sky,
                ("SunTextureContent", "SunTextureId", SUN_TEXTURE),
                ("SunAngularSize", SUN_SIZE),
                sun,
                hue,
            );
        }
        let moon = moon_direction(sun);
        if moon.y > 0.0 {
            let light = [MOON_LIGHT * moon_fade(moon.y); 3];
            l.moon_disc = self.disc(
                dom,
                sky,
                ("MoonTextureContent", "MoonTextureId", MOON_TEXTURE),
                ("MoonAngularSize", MOON_SIZE),
                moon,
                light,
            );
        }
    }

    /// One body: its image by its `Content` property, its legacy name or
    /// `default`; its angular size in degrees, clamped to 0 to 60
    /// (Confirmed: creator-docs Sky).
    fn disc(
        &mut self,
        dom: &Dom,
        sky: Option<InstanceId>,
        (content, legacy, default): (&str, &str, &str),
        (size_name, default_size): (&str, f32),
        direction: Vec3,
        light: [f32; 3],
    ) -> Option<Disc> {
        let degrees = sky
            .and_then(|s| dom.float_property(s, size_name))
            .unwrap_or(default_size);
        let size = degrees.clamp(0.0, 60.0).to_radians();
        if size <= 0.0 {
            return None;
        }
        let url = match sky {
            Some(s) => content_url(dom, s, &[content, legacy]),
            None => default.to_owned(),
        };
        // An empty image draws nothing, as an empty Decal (Inference).
        if url.is_empty() {
            return None;
        }
        let image = match self.sky_image(&url) {
            SkyImage::Ready(hash, _) => DiscImage::Image(hash),
            SkyImage::Loading => return None,
            // No Roblox install to load it from: Rook's own.
            SkyImage::Failed => DiscImage::Own,
        };
        Some(Disc {
            direction,
            size,
            image,
            light,
        })
    }
}

#[cfg(test)]
mod tests {
    use rook_types::Value;

    use super::*;

    /// A place with a Sky, at `time` (`TimeOfDay`), its images resolved
    /// through no content at all.
    fn place(time: &str) -> (Dom, InstanceId, SceneCache) {
        let mut dom = Dom::new();
        let lighting = dom.get_service("Lighting").unwrap();
        dom.set_property_by_name(lighting, "TimeOfDay", Value::String(time.into()))
            .unwrap();
        let sky = dom.create_by_name("Sky").unwrap();
        dom.set_parent(sky, Some(lighting)).unwrap();
        let content = Arc::new(Resolver::new(rook_content::Config::empty()));
        let mut cache = SceneCache::new(&dom, Floors::default(), content, Vec3::ZERO);
        cache.poll_files(&dom, true);
        (dom, sky, cache)
    }

    /// Applies a script's change to Lighting or its children, as the
    /// journal reports it.
    fn changed(dom: &Dom, cache: &mut SceneCache) {
        let changes = RenderChanges {
            lighting: true,
            ..RenderChanges::default()
        };
        cache.apply(dom, &changes);
        cache.poll_files(dom, true);
    }

    #[test]
    fn the_sun_shows_by_day_and_the_moon_by_night() {
        let (_, _, cache) = place("14:00:00");
        let l = cache.scene.lighting;
        let sun = l.sun_disc.expect("the sun by day");
        assert!(l.moon_disc.is_none());
        assert!((sun.size - 21f32.to_radians()).abs() < 1e-5);
        assert_eq!(sun.direction, l.sun_direction);
        // No Roblox install here: Rook's own disc.
        assert_eq!(sun.image, DiscImage::Own);
        let (_, _, cache) = place("00:00:00");
        let l = cache.scene.lighting;
        assert!(l.sun_disc.is_none());
        let moon = l.moon_disc.expect("the moon at night");
        assert!((moon.size - 11f32.to_radians()).abs() < 1e-5);
    }

    /// Scripts' changes to the Sky reach the scene through the journal.
    #[test]
    fn sky_changes_reach_the_scene() {
        let (mut dom, sky, mut cache) = place("14:00:00");
        dom.set_property_by_name(sky, "SunAngularSize", Value::Float32(90.0))
            .unwrap();
        let turned = rook_types::Vector3::new(0.0, 90.0, 0.0);
        dom.set_property_by_name(sky, "SkyboxOrientation", Value::Vector3(turned))
            .unwrap();
        changed(&dom, &mut cache);
        let l = cache.scene.lighting;
        // Clamped to 60 degrees.
        assert!((l.sun_disc.unwrap().size - 60f32.to_radians()).abs() < 1e-5);
        assert!((l.skybox_turn * Vec3::Z).distance(Vec3::X) < 1e-5);
        dom.set_property_by_name(sky, "SunAngularSize", Value::Float32(0.0))
            .unwrap();
        changed(&dom, &mut cache);
        assert!(cache.scene.lighting.sun_disc.is_none());
        dom.set_property_by_name(sky, "SunAngularSize", Value::Float32(21.0))
            .unwrap();
        dom.set_property_by_name(sky, "CelestialBodiesShown", Value::Bool(false))
            .unwrap();
        changed(&dom, &mut cache);
        assert!(cache.scene.lighting.sun_disc.is_none());
    }
}
