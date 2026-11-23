//! Path2D (docs/behavior/gui/modern.md#path2d): its curve placed in its
//! parent, drawn as round-capped lines among its siblings, and the
//! queries scripts make.

use std::sync::Arc;

use rook_types::{Color3, GuiQuad, Paint};

use super::*;
use crate::PathPoint;

fn point(xs: f32, xo: i32, ys: f32, yo: i32) -> PathPoint {
    PathPoint {
        position: UDim2::new(xs, xo, ys, yo),
        left: UDim2::default(),
        right: UDim2::default(),
    }
}

/// A red Path2D in `parent`, `thickness` wide.
fn path(dom: &mut Dom, parent: InstanceId, thickness: f32) -> InstanceId {
    child(
        dom,
        "Path2D",
        parent,
        &[
            ("Thickness", Value::Float32(thickness)),
            ("Color3", Value::Color3(Color3::new(1.0, 0.0, 0.0))),
        ],
    )
}

/// The quads drawn as a path's lines: cut to a capsule.
fn lines(quads: &[GuiQuad]) -> Vec<&GuiQuad> {
    (quads.iter())
        .filter(|q| q.shape.is_some_and(|s| s.radii[0] * 2.0 == s.size.y))
        .collect()
}

/// A line's ends, from its capsule.
fn ends(q: &GuiQuad) -> (Vec2, Vec2) {
    let s = q.shape.unwrap();
    let half = (s.size.x - s.size.y) * 0.5;
    let along = Vec2::from_angle(q.rotation) * half;
    (s.center - along, s.center + along)
}

/// A straight path is one line from point to point in its parent's box,
/// as wide as `Thickness` with round ends, in `Color3`, below the top bar.
#[test]
fn a_straight_path_is_one_round_capped_line() {
    let (mut dom, frame) = boxed();
    let p = path(&mut dom, frame, 4.0);
    let mut gui = laid_out(&mut dom);
    gui.set_path_points(p, vec![point(0.0, 0, 0.5, 0), point(0.25, 0, 0.5, 0)]);
    gui.sync(&mut dom);
    let quads = drawn(&mut gui, &dom);
    let lines = lines(&quads);
    assert_eq!(lines.len(), 1);
    let q = lines[0];
    let (a, b) = ends(q);
    // The frame is at (10, 20), 400 × 300; the top bar is 58 high.
    assert!(a.abs_diff_eq(Vec2::new(10.0, 228.0), 1e-3), "{a}");
    assert!(b.abs_diff_eq(Vec2::new(110.0, 228.0), 1e-3), "{b}");
    let s = q.shape.unwrap();
    assert_eq!((s.size.y, s.radii), (4.0, [2.0; 4]));
    assert_eq!(q.color, [1.0, 0.0, 0.0, 1.0]);
    assert!(q.size.cmpgt(s.size).all(), "room for the smoothed edge");
}

/// A curved segment is flattened to lines that join end to end and stay
/// within a fraction of a pixel of the curve.
#[test]
fn a_curve_is_flattened_close_to_it() {
    let (mut dom, frame) = boxed();
    let p = path(&mut dom, frame, 2.0);
    let mut gui = laid_out(&mut dom);
    let start = PathPoint {
        right: UDim2::from_offset(100, -150),
        ..point(0.0, 0, 0.0, 200)
    };
    let end = PathPoint {
        left: UDim2::from_offset(-100, -150),
        ..point(0.0, 300, 0.0, 200)
    };
    gui.set_path_points(p, vec![start, end]);
    gui.sync(&mut dom);
    let quads = drawn(&mut gui, &dom);
    let lines: Vec<(Vec2, Vec2)> = lines(&quads).into_iter().map(ends).collect();
    assert!(lines.len() > 8, "{}", lines.len());
    for pair in lines.windows(2) {
        assert!(pair[0].1.abs_diff_eq(pair[1].0, 1e-3));
    }
    let origin = Vec2::new(10.0, 20.0 + GUI_INSET);
    assert!(lines[0].0.abs_diff_eq(origin + Vec2::new(0.0, 200.0), 1e-3));
    let last = lines.last().unwrap().1;
    assert!(last.abs_diff_eq(origin + Vec2::new(300.0, 200.0), 1e-3));
    // The curve's middle: (150, 200 - 0.75 * 150).
    let middle = origin + Vec2::new(150.0, 87.5);
    let near = (lines.iter())
        .map(|&(a, b)| {
            let t = ((middle - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            middle.distance(a + (b - a) * t)
        })
        .fold(f32::INFINITY, f32::min);
    assert!(near < 0.25, "{near}");
}

/// Confirmed (creator-docs ZIndex): a path stacks among its parent's
/// children by `ZIndex`, over the parent; hidden, it draws nothing; it
/// takes no input.
#[test]
fn a_path_stacks_among_its_siblings() {
    let (mut dom, frame) = boxed();
    let tag = |dom: &mut Dom, z: i32, red: f32| {
        child(
            dom,
            "Frame",
            frame,
            &[
                ("Size", udim2(1.0, 0, 1.0, 0)),
                ("ZIndex", Value::Int32(z)),
                ("BorderSizePixel", Value::Int32(0)),
                (
                    "BackgroundColor3",
                    Value::Color3(Color3::new(red, 0.0, 1.0)),
                ),
            ],
        )
    };
    tag(&mut dom, 1, 0.25);
    tag(&mut dom, 3, 0.75);
    let p = path(&mut dom, frame, 3.0);
    set(&mut dom, p, "ZIndex", Value::Int32(2));
    let mut gui = laid_out(&mut dom);
    gui.set_path_points(p, vec![point(0.0, 0, 0.5, 0), point(1.0, 0, 0.5, 0)]);
    gui.sync(&mut dom);
    // The tags and the path, which have no green; the frame's background
    // and border have.
    let reds = |gui: &mut Gui, dom: &Dom| -> Vec<f32> {
        (drawn(gui, dom).iter())
            .filter(|q| q.color[1] == 0.0)
            .map(|q| q.color[0])
            .collect()
    };
    assert_eq!(reds(&mut gui, &dom), [0.25, 1.0, 0.75]);
    set(&mut dom, p, "ZIndex", Value::Int32(4));
    gui.sync(&mut dom);
    assert_eq!(reds(&mut gui, &dom), [0.25, 0.75, 1.0]);
    set(&mut dom, p, "Visible", Value::Bool(false));
    gui.sync(&mut dom);
    assert_eq!(reds(&mut gui, &dom), [0.25, 0.75]);
    set(&mut dom, p, "Visible", Value::Bool(true));
    gui.sync(&mut dom);
    assert_eq!(reds(&mut gui, &dom), [0.25, 0.75, 1.0]);
    let on_path = Vec2::new(200.0, 170.0);
    let hit = gui.objects_at(&dom, on_path);
    assert!(!hit.contains(&p) && hit.len() == 3, "{hit:?}");
}

/// Its points, its look and its parent's place each draw the path's piece
/// again and no other; the result is what a fresh drawing draws.
#[test]
fn changes_draw_again_only_the_paths_piece() {
    let (mut dom, frame) = boxed();
    let p = path(&mut dom, frame, 2.0);
    let points = vec![point(0.0, 0, 0.0, 0), point(1.0, 0, 1.0, 0)];
    let mut gui = laid_out(&mut dom);
    gui.set_path_points(p, points.clone());
    gui.sync(&mut dom);
    let mut before = gui.draw(&dom);
    let mut step = |dom: &mut Dom, change: &dyn Fn(&mut Dom, &mut Gui)| {
        change(dom, &mut gui);
        gui.sync(dom);
        let now = gui.draw(dom);
        let kept: Vec<bool> = (now.iter())
            .map(|p| before.iter().any(|b| Arc::ptr_eq(b, p)))
            .collect();
        before = now;
        let mut fresh = laid_out(dom);
        fresh.set_path_points(p, gui.path_points(p).to_vec());
        fresh.sync(dom);
        assert_eq!(drawn(&mut gui, dom), drawn(&mut fresh, dom));
        kept
    };
    let bent = vec![points[0], point(0.5, 0, 0.0, 0), points[1]];
    let moved = step(&mut dom, &|_, gui| gui.set_path_points(p, bent.clone()));
    assert_eq!(moved, [true, false]);
    let blue = Value::Color3(Color3::new(0.0, 0.0, 1.0));
    let recolored = step(&mut dom, &|dom, _| set(dom, p, "Color3", blue.clone()));
    assert_eq!(recolored, [true, false]);
    let thick = step(&mut dom, &|dom, _| {
        set(dom, p, "Thickness", Value::Float32(6.0))
    });
    assert_eq!(thick, [true, false]);
    let size = udim2(0.0, 200, 0.0, 100);
    let resized = step(&mut dom, &|dom, _| set(dom, frame, "Size", size.clone()));
    assert_eq!(resized, [false, false]);
    let closed = step(&mut dom, &|dom, _| set(dom, p, "Closed", Value::Bool(true)));
    assert_eq!(closed, [true, false]);
}

/// A translucent path is drawn whole into a texture of its own and laid
/// over faded, so its lines' overlaps don't show.
#[test]
fn a_translucent_path_is_drawn_as_one() {
    let (mut dom, frame) = boxed();
    let p = path(&mut dom, frame, 2.0);
    set(&mut dom, p, "Transparency", Value::Float32(0.5));
    let mut gui = laid_out(&mut dom);
    let points = vec![
        point(0.0, 0, 0.0, 0),
        point(0.5, 0, 0.5, 0),
        point(1.0, 0, 0.0, 0),
    ];
    gui.set_path_points(p, points);
    gui.sync(&mut dom);
    let quads = drawn(&mut gui, &dom);
    let group = quads.last().unwrap();
    let Paint::Group(g) = &group.paint else {
        panic!("{:?}", group.paint);
    };
    assert_eq!(group.color[3], 0.5);
    assert_eq!(lines(&g.quads).len(), 2);
    assert!(g.quads.iter().all(|q| q.color[3] == 1.0));
}

/// A closed path of three points or more joins its last point to its
/// first; with two it stays open (Confirmed: creator-docs `Closed`).
#[test]
fn closing_joins_the_ends() {
    let (mut dom, frame) = boxed();
    let p = path(&mut dom, frame, 2.0);
    set(&mut dom, p, "Closed", Value::Bool(true));
    let mut gui = laid_out(&mut dom);
    let two = vec![point(0.0, 0, 0.0, 0), point(1.0, 0, 0.0, 0)];
    gui.set_path_points(p, two.clone());
    gui.sync(&mut dom);
    assert_eq!(lines(&drawn(&mut gui, &dom)).len(), 1);
    let three = [two, vec![point(0.5, 0, 1.0, 0)]].concat();
    gui.set_path_points(p, three);
    gui.sync(&mut dom);
    assert_eq!(lines(&drawn(&mut gui, &dom)).len(), 3);
}

/// Positions come back as UDim2s in the parent, by parameter or by
/// length; tangents as unit vectors; the length and bounds in pixels.
#[test]
fn queries_read_the_curve_where_it_is_laid_out() {
    let (mut dom, frame) = boxed();
    let p = path(&mut dom, frame, 4.0);
    let mut gui = laid_out(&mut dom);
    // A short leg then a long one: 100 pixels across, then 300 down.
    let points = vec![
        point(0.0, 0, 0.0, 0),
        point(0.25, 0, 0.0, 0),
        point(0.25, 0, 1.0, 0),
    ];
    gui.set_path_points(p, points);
    let curve = gui.path_curve(&mut dom, p);
    assert_eq!(curve.segments(), 2);
    assert!((curve.length() - 400.0).abs() < 1e-2);
    assert_eq!(curve.position(0.5), Some(UDim2::new(0.25, 0, 0.0, 0)));
    assert_eq!(curve.position(0.75), Some(UDim2::new(0.25, 0, 0.5, 0)));
    let by_length = curve.position_by_length(0.5).unwrap();
    assert!((by_length.x.scale - 0.25).abs() < 1e-4, "{by_length:?}");
    assert!(
        (by_length.y.scale - 1.0 / 3.0).abs() < 1e-4,
        "{by_length:?}"
    );
    assert!(curve.tangent(0.25).unwrap().abs_diff_eq(Vec2::X, 1e-5));
    assert!(
        curve
            .tangent_by_length(0.5)
            .unwrap()
            .abs_diff_eq(Vec2::Y, 1e-5)
    );
    // In GUI pixels, like AbsolutePosition, grown by half the thickness.
    let (lo, hi) = curve.bounds(2.0).unwrap();
    assert!(lo.abs_diff_eq(Vec2::new(8.0, 18.0), 1e-3), "{lo}");
    assert!(hi.abs_diff_eq(Vec2::new(112.0, 322.0), 1e-3), "{hi}");
    gui.set_path_points(p, vec![point(0.0, 0, 0.0, 0)]);
    let single = gui.path_curve(&mut dom, p);
    assert_eq!((single.position(0.5), single.length()), (None, 0.0));
}
