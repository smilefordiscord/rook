//! The post effects' pixel math, on small frames
//! (docs/behavior/lighting/post-processing.md).

use super::*;

/// An orange wall filling the view, lit by the default lighting.
fn wall() -> (Scene, Camera) {
    let mut wall = part(PartShape::Block, 0.0, [0.9, 0.45, 0.1], 0.0);
    wall.size = Vec3::new(100.0, 100.0, 1.0);
    let camera = Camera::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO);
    (
        Scene {
            parts: vec![wall],
            ..Scene::default()
        },
        camera,
    )
}

fn correction() -> ColorCorrection {
    ColorCorrection {
        brightness: 0.0,
        contrast: 0.0,
        saturation: 0.0,
        tint: [1.0; 3],
    }
}

/// ColorCorrection acts on the finished pixels: its defaults change
/// nothing, Saturation -1 leaves gray, a red tint leaves only red, and
/// Brightness -1 leaves black.
#[test]
fn color_correction_grades_the_pixels() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let (mut scene, camera) = wall();
    let plain = middle(&mut r, &scene, &camera);
    assert!(plain[0] > plain[1] && plain[1] > plain[2], "{plain:?}");
    let with = |r: &mut Renderer, scene: &mut Scene, c: ColorCorrection| {
        scene.post.color = vec![c];
        middle(r, scene, &camera)
    };
    assert_eq!(with(&mut r, &mut scene, correction()), plain);
    let gray = with(
        &mut r,
        &mut scene,
        ColorCorrection {
            saturation: -1.0,
            ..correction()
        },
    );
    assert!(
        gray.iter().all(|&c| c.abs_diff(gray[0]) <= 1) && gray[0] > 0,
        "{gray:?}"
    );
    let red = with(
        &mut r,
        &mut scene,
        ColorCorrection {
            tint: [1.0, 0.0, 0.0],
            ..correction()
        },
    );
    assert_eq!(red, [plain[0], 0, 0]);
    let black = with(
        &mut r,
        &mut scene,
        ColorCorrection {
            brightness: -1.0,
            ..correction()
        },
    );
    assert_eq!(black, [0, 0, 0]);
}

/// A white Neon block, a few pixels across in a 32 × 32 frame, in the
/// dark: no sky, sun or ambient light.
fn glow() -> (Scene, Camera) {
    let mut block = part(PartShape::Block, 0.0, [1.0; 3], 0.0);
    block.material = Material::Neon;
    block.size = Vec3::splat(0.5);
    let camera = Camera::look_at(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO);
    let dark = Lighting {
        ambient: [0.0; 3],
        outdoor_ambient: [0.0; 3],
        sun_color: [0.0; 3],
        sky_tint: [0.0; 3],
        ..Lighting::default()
    };
    (
        Scene {
            parts: vec![block],
            lighting: dark,
            ..Scene::default()
        },
        camera,
    )
}

/// The red channel of each pixel of a 32 × 32 render, by row.
fn reds(r: &mut Renderer, scene: &Scene, camera: &Camera) -> Vec<u8> {
    r.set_scene(scene);
    let pixels = r.render_to_image(32, 32, camera).unwrap();
    pixels.chunks(4).map(|p| p[0]).collect()
}

/// A Blur spreads a bright spot over its neighbors and dims its middle,
/// keeping the dark far away dark.
#[test]
fn blur_spreads_a_point() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let (mut scene, camera) = glow();
    let sharp = reds(&mut r, &scene, &camera);
    let at = |image: &[u8], x: usize, y: usize| image[y * 32 + x];
    assert!(at(&sharp, 16, 16) > 200, "{}", at(&sharp, 16, 16));
    assert_eq!(at(&sharp, 20, 16), 0);
    scene.post.blur = 8.0;
    let soft = reds(&mut r, &scene, &camera);
    assert!(at(&soft, 16, 16) < at(&sharp, 16, 16));
    assert!(at(&soft, 20, 16) > 0, "{}", at(&soft, 20, 16));
    assert!(at(&soft, 16, 20) > 0, "{}", at(&soft, 16, 20));
    assert_eq!(at(&soft, 1, 1), 0);
    // A wide one runs on a shrunken copy and spreads a wider block as
    // far.
    scene.parts[0].size = Vec3::splat(3.0);
    scene.post.blur = 40.0;
    let wide = reds(&mut r, &scene, &camera);
    assert!(at(&wide, 28, 16) > 0 && at(&wide, 16, 16) > 0);
}

/// A Bloom brightens the dark around a light over its threshold, and
/// adds nothing around one under it; with Size 0 it brightens the light
/// itself and nothing around it.
#[test]
fn bloom_brightens_around_a_bright_light() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    let (mut scene, camera) = glow();
    // A few pixels across: a lone pixel averages away below any threshold
    // in the chain's first copy, as in Studio's.
    scene.parts[0].size = Vec3::splat(1.5);
    let plain = reds(&mut r, &scene, &camera);
    let at = |image: &[u8], x: usize, y: usize| image[y * 32 + x];
    let bloom = Bloom {
        intensity: 1.0,
        size: 8.0,
        threshold: 0.5,
    };
    scene.post.bloom = vec![bloom];
    let bloomed = reds(&mut r, &scene, &camera);
    assert_eq!(at(&plain, 21, 16), 0);
    assert!(at(&bloomed, 21, 16) > 0, "{}", at(&bloomed, 21, 16));
    assert_eq!(at(&bloomed, 1, 1), 0);
    // Gray Neon under the threshold blooms nowhere.
    scene.parts[0].color = [0.2; 3];
    let dim = reds(&mut r, &scene, &camera);
    assert_eq!(at(&dim, 21, 16), 0);
    // Without bleed, a mid-bright wall gets brighter, its edges no wider.
    let (mut wall, wall_camera) = wall();
    let before = middle(&mut r, &wall, &wall_camera);
    wall.post.bloom = vec![Bloom {
        size: 0.0,
        threshold: 0.0,
        ..bloom
    }];
    let after = middle(&mut r, &wall, &wall_camera);
    assert!(after[0] > before[0], "{after:?} {before:?}");
}
