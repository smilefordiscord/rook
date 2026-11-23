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
