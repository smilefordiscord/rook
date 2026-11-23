//! Terrain's Clouds: the properties Studio's cloud shaders take
//! (docs/behavior/lighting/clouds.md). Read with the Lighting service, as
//! a change to them marks the lighting changed.

use rook_render::Clouds;

use super::*;
use crate::terrain::workspace_terrain;

/// The first enabled Clouds in Workspace's Terrain, if any. Clouds
/// anywhere else draw nothing (Confirmed: creator docs, "clouds only
/// render if you parent the object under Terrain"); with several, the
/// first enabled one draws (Inference: the docs suggest toggling between
/// Clouds with `Enabled`).
pub(super) fn clouds(dom: &Dom) -> Option<Clouds> {
    let terrain = workspace_terrain(dom)?;
    let id = (dom.children(terrain).iter().copied()).find(|&c| {
        dom.class_name(c) == "Clouds" && dom.bool_property(c, "Enabled").unwrap_or(true)
    })?;
    // A script's NaN stays NaN in Studio (Confirmed: conformance case
    // `lighting/clouds_clamps`); it reaches the shaders as 0 (Inference:
    // how Studio draws one isn't measured), where `clamp` would keep it.
    let unit = |v: f32| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
    let float = |name, default| unit(dom.float_property(id, name).unwrap_or(default));
    let color = dom
        .color3_property(id, "Color")
        .map_or([1.0; 3], Color3::to_array);
    Some(Clouds {
        cover: float("Cover", 0.5),
        density: float("Density", 0.7),
        // As Studio's shaders take a color: its sRGB value squared.
        color: color.map(|c| unit(c).powi(2)),
    })
}

#[cfg(test)]
mod tests {
    use rook_types::Value;

    use super::*;

    fn cache(dom: &Dom) -> SceneCache {
        let content = Arc::new(Resolver::new(rook_content::Config::empty()));
        SceneCache::new(dom, Floors::default(), content, Vec3::ZERO)
    }

    fn lighting() -> RenderChanges {
        RenderChanges {
            lighting: true,
            ..RenderChanges::default()
        }
    }

    /// Clouds in Workspace's Terrain reach the scene with their
    /// properties, and a script's changes follow through the journal's
    /// lighting mark: Cover, Density, Color, Enabled, and leaving.
    #[test]
    fn clouds_are_read_and_follow_changes() {
        let mut dom = Dom::new();
        let workspace = dom.get_service("Workspace").unwrap();
        let terrain = dom.create_by_name("Terrain").unwrap();
        dom.set_parent(terrain, Some(workspace)).unwrap();
        let mut cache = cache(&dom);
        assert!(cache.scene.lighting.clouds.is_none());
        let clouds = dom.create_by_name("Clouds").unwrap();
        dom.set_parent(clouds, Some(terrain)).unwrap();
        cache.apply(&dom, &lighting());
        let c = cache.scene.lighting.clouds.expect("Clouds");
        assert!(
            (c.cover - 0.5).abs() < 1e-6 && (c.density - 0.7).abs() < 1e-6,
            "{c:?}"
        );
        assert_eq!(c.color, [1.0; 3]);
        dom.set_property_by_name(clouds, "Cover", Value::Float32(0.9))
            .unwrap();
        dom.set_property_by_name(clouds, "Density", Value::Float32(0.25))
            .unwrap();
        dom.set_property_by_name(clouds, "Color", Value::Color3(Color3::new(0.5, 1.0, 0.0)))
            .unwrap();
        cache.apply(&dom, &lighting());
        let c = cache.scene.lighting.clouds.unwrap();
        assert!((c.cover - 0.9).abs() < 1e-6 && (c.density - 0.25).abs() < 1e-6);
        assert_eq!(c.color, [0.25, 1.0, 0.0]);
        dom.set_property_by_name(clouds, "Enabled", Value::Bool(false))
            .unwrap();
        cache.apply(&dom, &lighting());
        assert!(cache.scene.lighting.clouds.is_none());
        dom.set_property_by_name(clouds, "Enabled", Value::Bool(true))
            .unwrap();
        dom.set_parent(clouds, None).unwrap();
        cache.apply(&dom, &lighting());
        assert!(cache.scene.lighting.clouds.is_none());
    }

    /// Numbers a script made NaN reach the shaders as 0, not NaN.
    #[test]
    fn numbers_that_are_not_numbers_read_as_zero() {
        let mut dom = Dom::new();
        let workspace = dom.get_service("Workspace").unwrap();
        let terrain = dom.create_by_name("Terrain").unwrap();
        dom.set_parent(terrain, Some(workspace)).unwrap();
        let id = dom.create_by_name("Clouds").unwrap();
        dom.set_parent(id, Some(terrain)).unwrap();
        for name in ["Cover", "Density"] {
            dom.set_property_by_name(id, name, Value::Float32(f32::NAN))
                .unwrap();
        }
        let nan = Color3::new(f32::NAN, 1.0, 0.5);
        dom.set_property_by_name(id, "Color", Value::Color3(nan))
            .unwrap();
        let c = clouds(&dom).expect("Clouds");
        assert_eq!((c.cover, c.density), (0.0, 0.0));
        assert_eq!(c.color, [0.0, 1.0, 0.25]);
    }

    /// Clouds outside Terrain draw nothing, as on Roblox: in Workspace, in
    /// Lighting, or in a Terrain that isn't Workspace's.
    #[test]
    fn clouds_outside_terrain_are_ignored() {
        let mut dom = Dom::new();
        let workspace = dom.get_service("Workspace").unwrap();
        let lighting = dom.get_service("Lighting").unwrap();
        let stray = dom.create_by_name("Terrain").unwrap();
        dom.set_parent(stray, Some(lighting)).unwrap();
        for parent in [workspace, lighting, stray] {
            let clouds = dom.create_by_name("Clouds").unwrap();
            dom.set_parent(clouds, Some(parent)).unwrap();
        }
        let cache = cache(&dom);
        assert!(cache.scene.lighting.clouds.is_none());
    }
}
