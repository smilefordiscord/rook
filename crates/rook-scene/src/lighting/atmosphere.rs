//! Lighting's Atmosphere: its six properties as the parameters of Studio's
//! atmosphere shaders (docs/behavior/lighting/atmosphere.md). The shaders'
//! form is Studio's (`shaders_glsl3.pack`); what each property becomes in
//! it is fed by Studio's CPU side, which the pack doesn't show, so each
//! mapping here is an Approximation until Studio captures settle it.

use rook_render::Atmosphere;

use super::*;

/// How fast surfaces fade at Density 1: they keep half of themselves per
/// this many studs, at Density `d` per this over `d`³ (Approximation: in
/// a Studio 0.748 screenshot at Density 0.45, parts 150 to 900 studs off
/// take only a slight haze and near ones none; the default 0.395 halves
/// one at about 4,000).
const HALF_STUDS: f32 = 250.0;

/// The haze's numerator per unit of `Haze`: at Haze 2 the sky keeps 69%
/// of itself a degree up and half of itself half a degree up, so only a thin pale
/// band shows at the horizon (Approximation: a Studio 0.748 screenshot at
/// Haze 2, Glare 2 shows no colored band or glare there).
const HAZE_PER_UNIT: f32 = -0.005;

/// The glare's falloff per channel is this over that channel of `Decay`:
/// channels Decay keeps fall off slowly away from the sun, the others fast
/// (Approximation).
const GLARE_SHARPNESS: f32 = 4.0;

/// The first Atmosphere in Lighting, if any, as the renderer draws it. With
/// one, Lighting's fog isn't drawn (Inference: creator docs say Studio
/// hides the fog properties then, and its surface shaders' Atmosphere form
/// has no FogStart or FogColor).
pub(super) fn atmosphere(dom: &Dom, lighting: InstanceId) -> Option<Atmosphere> {
    let id =
        (dom.children(lighting).iter().copied()).find(|&c| dom.class_name(c) == "Atmosphere")?;
    let float = |name, default: f32| dom.float_property(id, name).unwrap_or(default).max(0.0);
    let color = |name, default: [f32; 3]| {
        let c = dom
            .color3_property(id, name)
            .map_or(default, Color3::to_array);
        // A script's NaN reads as 0: `clamp` would keep it.
        c.map(|v| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) })
    };
    let density = float("Density", 0.395).min(1.0);
    // Defaults: a new Atmosphere's (Confirmed: conformance case
    // `lighting/sky_clamps`).
    let decay = color("Decay", [92.0 / 255.0, 60.0 / 255.0, 14.0 / 255.0]);
    Some(Atmosphere {
        falloff: density.powi(3) / HALF_STUDS,
        blur: float("Offset", 0.0).min(1.0),
        haze: HAZE_PER_UNIT * float("Haze", 0.0).min(10.0),
        // The haze takes the color as Studio's shaders take a color: its
        // sRGB value squared.
        color: color("Color", [200.0 / 255.0, 170.0 / 255.0, 108.0 / 255.0]).map(|c| c * c),
        glare: float("Glare", 0.0).min(10.0),
        glare_falloff: decay.map(|c| GLARE_SHARPNESS / c.max(0.05)),
    })
}

#[cfg(test)]
mod tests {
    use rook_types::Value;

    use super::*;

    /// An Atmosphere in Lighting reaches the scene and replaces the fog;
    /// a script's change to it reaches the scene through the journal, and
    /// removing it brings Lighting's fog back.
    #[test]
    fn an_atmosphere_is_read_and_follows_changes() {
        let mut dom = Dom::new();
        let lighting = dom.get_service("Lighting").unwrap();
        let content = Arc::new(Resolver::new(rook_content::Config::empty()));
        let mut cache = SceneCache::new(&dom, Floors::default(), content, Vec3::ZERO);
        assert!(cache.scene.lighting.atmosphere.is_none());
        let air = dom.create_by_name("Atmosphere").unwrap();
        dom.set_parent(air, Some(lighting)).unwrap();
        dom.set_property_by_name(air, "Density", Value::Float32(1.0))
            .unwrap();
        dom.set_property_by_name(air, "Haze", Value::Float32(2.0))
            .unwrap();
        let changes = RenderChanges {
            lighting: true,
            ..RenderChanges::default()
        };
        cache.apply(&dom, &changes);
        let a = cache.scene.lighting.atmosphere.expect("an Atmosphere");
        assert!((a.falloff - 1.0 / HALF_STUDS).abs() < 1e-6);
        assert!((a.haze - 2.0 * HAZE_PER_UNIT).abs() < 1e-6);
        dom.set_property_by_name(air, "Density", Value::Float32(0.5))
            .unwrap();
        cache.apply(&dom, &changes);
        let a = cache.scene.lighting.atmosphere.unwrap();
        assert!((a.falloff - 0.125 / HALF_STUDS).abs() < 1e-6);
        dom.set_parent(air, None).unwrap();
        cache.apply(&dom, &changes);
        assert!(cache.scene.lighting.atmosphere.is_none());
    }

    /// A color a script made NaN reaches the shaders as 0, not NaN.
    #[test]
    fn colors_that_are_not_numbers_read_as_zero() {
        let mut dom = Dom::new();
        let lighting = dom.get_service("Lighting").unwrap();
        let air = dom.create_by_name("Atmosphere").unwrap();
        dom.set_parent(air, Some(lighting)).unwrap();
        let nan = Value::Color3(Color3::new(f32::NAN, 1.0, 0.5));
        dom.set_property_by_name(air, "Color", nan.clone()).unwrap();
        dom.set_property_by_name(air, "Decay", nan).unwrap();
        let a = atmosphere(&dom, lighting).unwrap();
        assert_eq!(a.color, [0.0, 1.0, 0.25]);
        assert_eq!(
            a.glare_falloff,
            [GLARE_SHARPNESS / 0.05, GLARE_SHARPNESS, 8.0]
        );
    }
}
