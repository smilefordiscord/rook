//! Scripts changing how the sky and lighting look reach the renderer.

use crate::tests::*;

/// Lighting's properties and its children's (a Sky arriving, changing or
/// leaving) mark the lighting as changed; instances elsewhere don't.
#[test]
fn sky_changes_mark_the_lighting() {
    let mut s = Server::new(&Options::default()).unwrap();
    s.start();
    s.take_render_changes();
    let lighting = |s: &mut Server, source: &str| {
        s.run_extra_script(source).unwrap();
        let changes = s.take_render_changes();
        assert!(!changes.full, "{source}");
        changes.lighting
    };
    for source in [
        "game.Lighting.FogEnd = 500",
        "Instance.new('Sky').Parent = game.Lighting",
        "game.Lighting.Sky.SkyboxUp = 'rbxassetid://1'",
        "game.Lighting.Sky.Parent = nil",
    ] {
        assert!(lighting(&mut s, source), "{source}");
    }
    assert!(!lighting(
        &mut s,
        "Instance.new('Sky').Parent = game.ReplicatedStorage"
    ));
}

/// Clouds in the Terrain arriving, changing or leaving mark the lighting
/// (the scene reads them with it); Clouds elsewhere don't.
#[test]
fn cloud_changes_mark_the_lighting() {
    let mut s = Server::new(&Options::default()).unwrap();
    s.start();
    s.take_render_changes();
    let lighting = |s: &mut Server, source: &str| {
        s.run_extra_script(source).unwrap();
        let changes = s.take_render_changes();
        assert!(!changes.full, "{source}");
        changes.lighting
    };
    for source in [
        "Instance.new('Clouds').Parent = workspace.Terrain",
        "workspace.Terrain.Clouds.Cover = 0.9",
        "workspace.Terrain.Clouds.Color = Color3.new(1, 0, 0)",
        "workspace.Terrain.Clouds.Enabled = false",
        "workspace.Terrain.Clouds:Destroy()",
    ] {
        assert!(lighting(&mut s, source), "{source}");
    }
    for source in [
        "Instance.new('Clouds').Parent = workspace",
        "workspace.Clouds.Cover = 0.9",
    ] {
        assert!(!lighting(&mut s, source), "{source}");
    }
}

/// A PostEffect under Lighting or the current Camera marks the post
/// effects when it changes, comes or goes, and so does a new current
/// Camera; one anywhere else doesn't, and none marks the lighting.
#[test]
fn post_effect_changes_mark_the_post_effects() {
    let mut s = Server::new(&Options::default()).unwrap();
    s.start();
    s.take_render_changes();
    let marks = |s: &mut Server, source: &str| {
        s.run_extra_script(source).unwrap();
        let changes = s.take_render_changes();
        assert!(!changes.full && !changes.lighting, "{source}");
        changes.post_effects
    };
    for source in [
        "Instance.new('BlurEffect').Parent = game.Lighting",
        "game.Lighting.Blur.Size = 4",
        "game.Lighting.Blur.Enabled = false",
        "game.Lighting.Blur.Parent = workspace.CurrentCamera",
        "workspace.CurrentCamera.Blur.Size = 8",
        "workspace.CurrentCamera = Instance.new('Camera', workspace)",
    ] {
        assert!(marks(&mut s, source), "{source}");
    }
    assert!(!marks(
        &mut s,
        "Instance.new('BloomEffect').Parent = game.ReplicatedStorage"
    ));
    assert!(!marks(&mut s, "game.ReplicatedStorage.Bloom.Size = 3"));
}
