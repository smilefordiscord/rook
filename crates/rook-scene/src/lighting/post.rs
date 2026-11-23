//! Reading the post effects the frame applies from Lighting's and the
//! current Camera's children (docs/behavior/lighting/post-processing.md).
//! Read again only when the journal marks them
//! (`RenderChanges::post_effects`), so a read costs those children.

use super::*;
use rook_render::{Bloom, ColorCorrection, DepthOfField, PostEffects, SunRays, Tonemapper};

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
                brightness: setting(dom, id, "Brightness", 0.0),
                contrast: setting(dom, id, "Contrast", 0.0),
                saturation: setting(dom, id, "Saturation", 0.0),
                tint: (dom.color3_property(id, "TintColor"))
                    .map(Color3::to_array)
                    .filter(|c| c.iter().all(|v| v.is_finite()))
                    .unwrap_or([1.0; 3]),
            }),
            // Each adds its own (Confirmed: creator-docs BloomEffect, they
            // compose).
            "BloomEffect" => post.bloom.push(Bloom {
                intensity: setting(dom, id, "Intensity", 0.4),
                size: setting(dom, id, "Size", 24.0),
                threshold: setting(dom, id, "Threshold", 0.95),
            }),
            // The brightest draws (Inference: Studio's shader takes one
            // set of settings).
            "SunRaysEffect" => {
                let rays = SunRays {
                    intensity: setting(dom, id, "Intensity", 0.25),
                    spread: setting(dom, id, "Spread", 1.0),
                };
                if post.sun_rays.is_none_or(|r| rays.intensity > r.intensity) {
                    post.sun_rays = Some(rays);
                }
            }
            // The first one draws (Inference: Studio's shaders take one set
            // of settings).
            "DepthOfFieldEffect" if post.depth_of_field.is_none() => {
                post.depth_of_field = Some(DepthOfField {
                    far_intensity: setting(dom, id, "FarIntensity", 0.75),
                    focus_distance: setting(dom, id, "FocusDistance", 0.05),
                    in_focus_radius: setting(dom, id, "InFocusRadius", 10.0),
                    near_intensity: setting(dom, id, "NearIntensity", 0.75),
                });
            }
            // The greatest Size wins (Confirmed: creator-docs BlurEffect).
            "BlurEffect" => {
                let size = setting(dom, id, "Size", 24.0);
                post.blur = post.blur.max(size);
            }
            _ => {}
        }
    }
    post.tonemapper = match grading.and_then(|c| dom.enum_item_property(c, "TonemapperPreset")) {
        Some("Retro") => Tonemapper::Retro,
        _ => Tonemapper::Default,
    };
    post
}

/// `id`'s `name`, or `default` when it isn't a finite number
/// (Inference: what Studio draws for one isn't measured; Rook's shaders
/// would carry a NaN or infinity into every pixel).
fn setting(dom: &Dom, id: InstanceId, name: &str, default: f32) -> f32 {
    (dom.float_property(id, name))
        .filter(|v| v.is_finite())
        .unwrap_or(default)
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

    /// Of several Blurs the greatest Size draws, wherever it is.
    #[test]
    fn the_greatest_blur_wins() {
        let (mut dm, mut cache) = place();
        let post = run(
            &mut dm,
            &mut cache,
            "local a = Instance.new('BlurEffect')
             a.Size = 4
             a.Parent = game.Lighting
             local b = Instance.new('BlurEffect')
             b.Size = 10
             b.Parent = workspace.CurrentCamera",
        )
        .unwrap();
        assert_eq!(post.blur, 10.0);
        let post = run(
            &mut dm,
            &mut cache,
            "workspace.CurrentCamera.Blur.Enabled = false",
        );
        assert_eq!(post.unwrap().blur, 4.0);
    }

    /// Every Bloom counts, with its own settings, and the defaults are
    /// Studio's.
    #[test]
    fn every_bloom_adds() {
        let (mut dm, mut cache) = place();
        let post = run(
            &mut dm,
            &mut cache,
            "Instance.new('BloomEffect').Parent = game.Lighting
             local b = Instance.new('BloomEffect')
             b.Size = 0
             b.Parent = workspace.CurrentCamera",
        )
        .unwrap();
        let sizes: Vec<f32> = post.bloom.iter().map(|b| b.size).collect();
        assert_eq!(sizes, [24.0, 0.0]);
        assert_eq!(
            (post.bloom[0].intensity, post.bloom[0].threshold),
            (0.4, 0.95)
        );
    }

    /// Of several SunRays the brightest draws.
    #[test]
    fn the_brightest_sun_rays_win() {
        let (mut dm, mut cache) = place();
        let post = run(
            &mut dm,
            &mut cache,
            "local a = Instance.new('SunRaysEffect')
             a.Parent = game.Lighting
             local b = Instance.new('SunRaysEffect')
             b.Intensity = 0.5
             b.Spread = 0.3
             b.Parent = game.Lighting",
        )
        .unwrap();
        let rays = post.sun_rays.unwrap();
        assert_eq!((rays.intensity, rays.spread), (0.5, 0.3));
    }

    /// Of several DepthOfFields the first enabled one draws, with Studio's
    /// defaults.
    #[test]
    fn the_first_depth_of_field_wins() {
        let (mut dm, mut cache) = place();
        let post = run(
            &mut dm,
            &mut cache,
            "Instance.new('DepthOfFieldEffect').Parent = game.Lighting
             local b = Instance.new('DepthOfFieldEffect')
             b.FocusDistance = 30
             b.Parent = game.Lighting",
        )
        .unwrap();
        let dof = post.depth_of_field.unwrap();
        assert_eq!((dof.focus_distance, dof.in_focus_radius), (0.05, 10.0));
        assert_eq!((dof.far_intensity, dof.near_intensity), (0.75, 0.75));
        let post = run(
            &mut dm,
            &mut cache,
            "game.Lighting:FindFirstChildOfClass('DepthOfFieldEffect').Enabled = false",
        );
        assert_eq!(post.unwrap().depth_of_field.unwrap().focus_distance, 30.0);
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

    /// A setting a script made NaN or infinite reads as its default, so
    /// no NaN reaches the frame's pixels.
    #[test]
    fn settings_that_are_not_numbers_read_as_defaults() {
        let (mut dm, mut cache) = place();
        let post = run(
            &mut dm,
            &mut cache,
            "local b = Instance.new('BloomEffect')
             b.Intensity = 0/0
             b.Size = math.huge
             b.Parent = game.Lighting
             local c = Instance.new('ColorCorrectionEffect')
             c.Contrast = -math.huge
             c.TintColor = Color3.new(0/0, 1, 1)
             c.Parent = game.Lighting",
        )
        .unwrap();
        assert_eq!((post.bloom[0].intensity, post.bloom[0].size), (0.4, 24.0));
        assert_eq!(
            (post.color[0].contrast, post.color[0].tint),
            (0.0, [1.0; 3])
        );
    }
}
