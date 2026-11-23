//! Reading the post effects the frame applies from Lighting's and the
//! current Camera's children (docs/behavior/lighting/post-processing.md).
//! Read again only when the journal marks them
//! (`RenderChanges::post_effects`), so a read costs those children.

use super::*;
use rook_render::{ColorCorrection, PostEffects, Tonemapper};

/// The enabled PostEffects directly under Lighting, then under the current
/// Camera, in child order (Confirmed: creator-docs PostEffect, effects
/// work parented to either).
pub(crate) fn post_effects(dom: &Dom) -> PostEffects {
    let mut post = PostEffects::default();
    let mut grading = None;
    let holders = rook_engine::post_effect_holders(dom);
    let children = holders.into_iter().flatten().flat_map(|h| dom.children(h));
    for &id in children {
        if !dom.bool_property(id, "Enabled").unwrap_or(true) {
            continue;
        }
        match dom.class_name(id) {
            // The first one's curve (Inference: one curve draws; which of
            // several isn't measured).
            "ColorGradingEffect" if grading.is_none() => grading = Some(id),
            "ColorCorrectionEffect" => post.color.push(ColorCorrection {
                brightness: dom.float_property(id, "Brightness").unwrap_or(0.0),
                contrast: dom.float_property(id, "Contrast").unwrap_or(0.0),
                saturation: dom.float_property(id, "Saturation").unwrap_or(0.0),
                tint: (dom.color3_property(id, "TintColor")).map_or([1.0; 3], Color3::to_array),
            }),
            _ => {}
        }
    }
    post.tonemapper = match grading.and_then(|c| dom.enum_item_property(c, "TonemapperPreset")) {
        Some("Retro") => Tonemapper::Retro,
        _ => Tonemapper::Default,
    };
    post
}

#[cfg(test)]
mod tests {
    use super::*;
    use rook_engine::{DataModel, Options};
    use std::sync::Arc;

    /// A cache of an empty place, and a way to run a script in it and
    /// apply what it changed.
    fn place() -> (DataModel, SceneCache) {
        let mut dm = DataModel::new(&Options::default()).unwrap();
        dm.start();
        dm.take_render_changes();
        let content = Arc::new(rook_content::Resolver::new(
            rook_engine::settings::content_config(false),
        ));
        let cache = SceneCache::new(
            &dm.runtime().world().dom,
            crate::Floors::default(),
            content,
            Vec3::ZERO,
        );
        (dm, cache)
    }

    fn run(dm: &mut DataModel, cache: &mut SceneCache, source: &str) -> Option<PostEffects> {
        dm.run_extra_script(source).unwrap();
        let changes = dm.take_render_changes();
        cache.apply(&dm.runtime().world().dom, &changes);
        cache.take_post_change()
    }

    fn saturations(post: &PostEffects) -> Vec<f32> {
        post.color.iter().map(|c| c.saturation).collect()
    }

    /// Effects under Lighting and then the current Camera are read in that
    /// order, and every change that matters reaches the scene: a
    /// property, `Enabled`, leaving for somewhere they don't work, and
    /// the camera they're under no longer being current.
    #[test]
    fn effects_follow_their_changes() {
        let (mut dm, mut cache) = place();
        assert_eq!(cache.scene.post, PostEffects::default());
        let post = run(
            &mut dm,
            &mut cache,
            "local c = Instance.new('ColorCorrectionEffect')
             c.Saturation = -1
             c.Parent = workspace.CurrentCamera
             local l = Instance.new('ColorCorrectionEffect')
             l.Name = 'Lit'
             l.TintColor = Color3.new(1, 0, 0)
             l.Parent = game.Lighting",
        )
        .unwrap();
        assert_eq!(saturations(&post), [0.0, -1.0]);
        assert_eq!(post.color[0].tint, [1.0, 0.0, 0.0]);
        let post = run(&mut dm, &mut cache, "game.Lighting.Lit.Saturation = 0.5").unwrap();
        assert_eq!(saturations(&post), [0.5, -1.0]);
        let post = run(&mut dm, &mut cache, "game.Lighting.Lit.Enabled = false").unwrap();
        assert_eq!(saturations(&post), [-1.0]);
        let post = run(
            &mut dm,
            &mut cache,
            "workspace.CurrentCamera.ColorCorrection.Parent = game.ReplicatedStorage",
        )
        .unwrap();
        assert!(post.color.is_empty());
        let post = run(
            &mut dm,
            &mut cache,
            "game.ReplicatedStorage.ColorCorrection.Parent = workspace.CurrentCamera",
        )
        .unwrap();
        assert_eq!(saturations(&post), [-1.0]);
        let post = run(
            &mut dm,
            &mut cache,
            "workspace.CurrentCamera = Instance.new('Camera', workspace)",
        )
        .unwrap();
        assert!(post.color.is_empty());
        // Nothing that matters changed: nothing to hand over.
        assert_eq!(run(&mut dm, &mut cache, "workspace.Gravity = 10"), None);
    }

    /// A ColorGradingEffect's curve applies from the Camera too, and the
    /// first enabled one in Lighting wins.
    #[test]
    fn the_first_grading_picks_the_curve() {
        let (mut dm, mut cache) = place();
        let post = run(
            &mut dm,
            &mut cache,
            "local g = Instance.new('ColorGradingEffect')
             g.TonemapperPreset = Enum.TonemapperPreset.Retro
             g.Parent = workspace.CurrentCamera",
        )
        .unwrap();
        assert_eq!(post.tonemapper, Tonemapper::Retro);
        let post = run(
            &mut dm,
            &mut cache,
            "Instance.new('ColorGradingEffect').Parent = game.Lighting",
        )
        .unwrap();
        assert_eq!(post.tonemapper, Tonemapper::Default);
    }
}
