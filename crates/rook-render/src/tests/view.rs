use super::*;

fn scene() -> Scene {
    Scene {
        parts: vec![part(PartShape::Block, 0.0, [1.0, 0.0, 0.0], 0.0)],
        ..Scene::default()
    }
}

/// The 3D view in part of the target is the picture a target of the
/// view's size would show, projected for its own shape and centered on
/// it, with black around it: an editor's view between its panels.
#[test]
fn a_view_rectangle_draws_the_whole_view_there() {
    let Ok(mut r) = Renderer::headless() else {
        return;
    };
    r.set_scene(&scene());
    r.finish_uploads();
    let camera = Camera::look_at(Vec3::new(1.0, 1.0, 6.0), Vec3::ZERO);
    let alone = r.render_to_image(32, 32, &camera).unwrap();
    r.set_view(Some(ViewRect {
        x: 24,
        y: 8,
        width: 32,
        height: 32,
    }));
    let placed = r.render_to_image(64, 48, &camera).unwrap();
    for y in 0..48usize {
        for x in 0..64usize {
            let px = &placed[(y * 64 + x) * 4..][..3];
            if (24..56).contains(&x) && (8..40).contains(&y) {
                let own = &alone[((y - 8) * 32 + x - 24) * 4..][..3];
                assert_eq!(px, own, "at {x}, {y}");
            } else {
                assert_eq!(px, [0, 0, 0], "at {x}, {y}");
            }
        }
    }
    // Back to the whole target.
    r.set_view(None);
    assert_eq!(r.render_to_image(32, 32, &camera).unwrap(), alone);
}

/// A view reaching past the target is cut to it, and one of no size is
/// a pixel, so a window resized under it never draws out of bounds.
#[test]
fn a_view_rectangle_is_cut_to_the_target() {
    let cut = ViewRect {
        x: 50,
        y: 10,
        width: 40,
        height: 0,
    }
    .within(64, 48);
    assert_eq!(
        cut,
        ViewRect {
            x: 50,
            y: 10,
            width: 14,
            height: 1,
        }
    );
}
