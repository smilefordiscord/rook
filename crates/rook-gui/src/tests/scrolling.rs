//! ScrollingFrames, checked against values read in Studio 0.743
//! (docs/behavior/gui/scrolling.md): a 200 × 100 frame at (10, 10) in the
//! 400 × 300 box, with no border unless a case gives one.

use super::*;

/// A ScrollingFrame in the box with `props` set, and a child at (10, 20)
/// half the canvas's size.
fn scroller(props: &[(&str, Value)]) -> (Dom, InstanceId, InstanceId) {
    let (mut dom, frame) = boxed();
    let mut all = vec![
        ("Size", udim2(0.0, 200, 0.0, 100)),
        ("Position", udim2(0.0, 10, 0.0, 10)),
        ("BorderSizePixel", Value::Int32(0)),
    ];
    all.extend(props.iter().cloned());
    let f = child(&mut dom, "ScrollingFrame", frame, &all);
    let c = child(
        &mut dom,
        "Frame",
        f,
        &[
            ("Position", udim2(0.0, 10, 0.0, 20)),
            ("Size", udim2(0.5, 0, 0.5, 0)),
        ],
    );
    (dom, f, c)
}

fn canvas(x: i32, y: i32) -> (&'static str, Value) {
    ("CanvasSize", udim2(0.0, x, 0.0, y))
}

fn sizes(dom: &Dom, f: InstanceId) -> (Vec2, Vec2) {
    (
        vec2_of(dom, f, "AbsoluteWindowSize"),
        vec2_of(dom, f, "AbsoluteCanvasSize"),
    )
}

#[track_caller]
fn check(props: &[(&str, Value)], window: [f32; 2], canvas: [f32; 2]) {
    let (mut dom, f, _) = scroller(props);
    laid_out(&mut dom);
    near(sizes(&dom, f), (window, canvas));
}

#[test]
fn bars_take_their_thickness_from_the_window() {
    check(&[canvas(300, 400)], [188.0, 88.0], [300.0, 400.0]);
    check(&[canvas(100, 400)], [188.0, 100.0], [200.0, 400.0]);
    check(&[canvas(300, 50)], [200.0, 88.0], [300.0, 100.0]);
    check(&[canvas(50, 50)], [200.0, 100.0], [200.0, 100.0]);
    // The vertical bar's room makes the horizontal one show.
    check(&[canvas(195, 400)], [188.0, 88.0], [200.0, 400.0]);
    check(
        &[canvas(300, 400), ("ScrollBarThickness", Value::Int32(30))],
        [170.0, 70.0],
        [300.0, 400.0],
    );
}

#[test]
fn insets_and_directions() {
    let (mut dom, f, c) = scroller(&[canvas(50, 50)]);
    set_enum(&mut dom, f, "VerticalScrollBarInset", "Always");
    set_enum(&mut dom, f, "HorizontalScrollBarInset", "Always");
    laid_out(&mut dom);
    near(sizes(&dom, f), ([200.0, 100.0], [188.0, 88.0]));
    near(rect(&dom, c, f), ([10.0, 20.0], [94.0, 44.0]));

    let (mut dom, f, _) = scroller(&[canvas(100, 400)]);
    set_enum(&mut dom, f, "VerticalScrollBarInset", "ScrollBar");
    laid_out(&mut dom);
    near(sizes(&dom, f), ([188.0, 100.0], [188.0, 400.0]));

    let (mut dom, f, _) = scroller(&[canvas(300, 400)]);
    set_enum(&mut dom, f, "ScrollingDirection", "X");
    laid_out(&mut dom);
    near(sizes(&dom, f), ([200.0, 88.0], [300.0, 400.0]));
}

#[test]
fn canvas_size_scales_with_the_parent() {
    // The default {0, 0}, {2, 0}: twice the 300-high box.
    check(&[], [188.0, 100.0], [200.0, 600.0]);
}

#[test]
fn borders_part_the_bars_from_the_window() {
    check(
        &[canvas(400, 600), ("BorderSizePixel", Value::Int32(3))],
        [185.0, 85.0],
        [400.0, 600.0],
    );
    let (mut dom, f, c) = scroller(&[canvas(400, 600), ("BorderSizePixel", Value::Int32(4))]);
    set_enum(&mut dom, f, "BorderMode", "Inset");
    laid_out(&mut dom);
    near(sizes(&dom, f), ([184.0, 84.0], [400.0, 600.0]));
    near(rect(&dom, c, f), ([14.0, 24.0], [196.0, 296.0]));
}

#[test]
fn canvas_position_is_kept_within_the_canvas() {
    let at = |x, y| ("CanvasPosition", v2(x, y));
    let (mut dom, f, c) = scroller(&[canvas(300, 400), at(1000.0, 1000.0)]);
    laid_out(&mut dom);
    assert_eq!(vec2_of(&dom, f, "CanvasPosition"), Vec2::new(112.0, 312.0));
    near(rect(&dom, c, f), ([-102.0, -292.0], [150.0, 200.0]));
    // Children land on whole pixels.
    let (mut dom, f, c) = scroller(&[canvas(300, 400), at(10.6, 20.25)]);
    laid_out(&mut dom);
    near(rect(&dom, c, f), ([-1.0, 0.0], [150.0, 200.0]));
    // Scripts' writes are kept within the canvas at once.
    let (mut dom, f, _) = scroller(&[canvas(300, 400)]);
    let mut gui = laid_out(&mut dom);
    gui.set_canvas_position(&mut dom, f, Vec2::new(-50.0, 500.0))
        .unwrap();
    assert_eq!(vec2_of(&dom, f, "CanvasPosition"), Vec2::new(0.0, 312.0));
}

#[test]
fn a_left_bar_moves_an_inset_canvas() {
    let (mut dom, f, c) = scroller(&[canvas(100, 400)]);
    set_enum(&mut dom, f, "VerticalScrollBarPosition", "Left");
    set_enum(&mut dom, f, "VerticalScrollBarInset", "ScrollBar");
    laid_out(&mut dom);
    near(rect(&dom, c, f), ([22.0, 20.0], [94.0, 200.0]));
}

#[test]
fn automatic_canvas_size_holds_what_is_inside() {
    let (mut dom, f, c) = scroller(&[canvas(0, 0)]);
    set_enum(&mut dom, f, "AutomaticCanvasSize", "Y");
    set(&mut dom, c, "Size", udim2(0.0, 50, 0.0, 50));
    set(&mut dom, c, "Position", udim2(0.0, 0, 0.0, 300));
    let deep = frame(&mut dom, c, udim2(0.0, 50, 0.0, 50));
    set(&mut dom, deep, "Position", udim2(0.0, 0, 0.0, 400));
    let padding = child(
        &mut dom,
        "UIPadding",
        f,
        &[
            ("PaddingTop", udim(0.0, 10)),
            ("PaddingBottom", udim(0.0, 20)),
        ],
    );
    laid_out(&mut dom);
    // A grandchild counts, with the padding: 10 + 300 + 400 + 50 + 20.
    near(sizes(&dom, f), ([188.0, 100.0], [200.0, 780.0]));
    dom.destroy(padding).unwrap();
    dom.destroy(deep).unwrap();
    set(&mut dom, f, "CanvasSize", udim2(0.0, 0, 0.0, 500));
    laid_out(&mut dom);
    near(sizes(&dom, f), ([188.0, 100.0], [200.0, 500.0]));
}

#[test]
fn automatic_canvas_size_with_a_list() {
    let (mut dom, f, c) = scroller(&[canvas(0, 0)]);
    dom.destroy(c).unwrap();
    set_enum(&mut dom, f, "AutomaticCanvasSize", "Y");
    child(&mut dom, "UIListLayout", f, &[("Padding", udim(0.0, 5))]);
    for _ in 0..5 {
        frame(&mut dom, f, udim2(0.0, 150, 0.0, 60));
    }
    laid_out(&mut dom);
    near(sizes(&dom, f), ([188.0, 100.0], [200.0, 320.0]));
}

/// Every GUI object's `AbsolutePosition` under `top`.
fn positions(dom: &Dom, top: InstanceId) -> Vec<(InstanceId, Vec2)> {
    (dom.descendants(top).into_iter())
        .filter(|&d| dom.is_a(d, "GuiObject"))
        .map(|d| (d, vec2_of(dom, d, "AbsolutePosition")))
        .collect()
}

/// A moved canvas moves what it holds as it was laid out: everything ends
/// where laying it all out again puts it, drawn there too, inside turned
/// and scaled frames, a UI layout and a ScrollingFrame within.
#[test]
fn a_moved_canvas_lands_where_a_fresh_layout_puts_it() {
    let (mut dom, held) = boxed();
    set(&mut dom, held, "Rotation", Value::Float32(15.0));
    child(&mut dom, "UIScale", held, &[("Scale", Value::Float32(1.5))]);
    let f = child(
        &mut dom,
        "ScrollingFrame",
        held,
        &[
            ("Size", udim2(0.0, 200, 0.0, 100)),
            ("Position", udim2(0.0, 10, 0.0, 10)),
            canvas(300, 600),
        ],
    );
    child(&mut dom, "UIListLayout", f, &[]);
    for _ in 0..5 {
        let row = frame(&mut dom, f, udim2(1.0, 0, 0.0, 50));
        child(
            &mut dom,
            "TextLabel",
            row,
            &[("Size", udim2(0.5, 0, 1.0, 0))],
        );
    }
    let inner = child(
        &mut dom,
        "ScrollingFrame",
        f,
        &[("Size", udim2(1.0, 0, 0.0, 80)), canvas(0, 400)],
    );
    let deep = frame(&mut dom, inner, udim2(0.0, 40, 0.0, 40));
    set(&mut dom, deep, "Position", udim2(0.0, 5, 0.0, 300));
    let mut gui = laid_out(&mut dom);
    // Whole pixels, a fraction, past the end, the inner one alone, and
    // both at once.
    let moves: &[&[(InstanceId, [f32; 2])]] = &[
        &[(f, [0.0, 120.0])],
        &[(f, [30.5, 47.4])],
        &[(f, [9000.0, 9000.0])],
        &[(inner, [0.0, 250.0])],
        &[(f, [12.0, 3.0]), (inner, [0.0, 10.0])],
    ];
    for &step in moves {
        for &(id, [x, y]) in step {
            set(&mut dom, id, "CanvasPosition", v2(x, y));
        }
        gui.sync(&mut dom);
        let moved = (positions(&dom, held), drawn(&mut gui, &dom));
        let mut fresh = laid_out(&mut dom);
        let again = (positions(&dom, held), drawn(&mut fresh, &dom));
        for ((id, got), (_, want)) in moved.0.iter().zip(&again.0) {
            assert!(
                got.abs_diff_eq(*want, 1e-3),
                "{step:?}: {id:?} at {got}, fresh {want}"
            );
        }
        assert_eq!(moved.1.len(), again.1.len(), "{step:?}");
        for (got, want) in moved.1.iter().zip(&again.1) {
            // Moved rather than placed under a turn: within rounding.
            assert!(got.center.abs_diff_eq(want.center, 1e-3), "{step:?}");
            let got = rook_types::GuiQuad {
                center: want.center,
                ..got.clone()
            };
            assert_eq!(&got, want, "{step:?}");
        }
    }
    // Kept within the canvas, as a script's write is (a 187 × 87 window:
    // its 1-pixel border parts the bars from it).
    set(&mut dom, f, "CanvasPosition", v2(9000.0, 9000.0));
    gui.sync(&mut dom);
    near(
        (vec2_of(&dom, f, "CanvasPosition"), Vec2::ZERO),
        ([113.0, 513.0], [0.0, 0.0]),
    );
}

/// What a canvas holds that moves or restacks (a list that recycles its
/// rows) is placed again alone and drawn in its new order: where laying
/// it all out again puts it, in a scrolled canvas.
#[test]
fn a_canvas_child_moved_or_restacked_lands_where_a_fresh_layout_puts_it() {
    let (mut dom, held) = boxed();
    let f = child(
        &mut dom,
        "ScrollingFrame",
        held,
        &[
            ("Size", udim2(0.0, 200, 0.0, 100)),
            ("Position", udim2(0.0, 10, 0.0, 10)),
            canvas(0, 600),
            ("CanvasPosition", v2(0.0, 140.0)),
        ],
    );
    let tiles: Vec<InstanceId> = (0..4)
        .map(|i| {
            let tile = frame(&mut dom, f, udim2(0.5, 0, 0.0, 60));
            set(&mut dom, tile, "Position", udim2(0.0, 0, 0.0, 70 * i));
            frame(&mut dom, tile, udim2(0.5, 0, 0.5, 0));
            tile
        })
        .collect();
    let mut gui = laid_out(&mut dom);
    // The top tile recycled to the bottom; another drawn over the rest.
    set(&mut dom, tiles[0], "Position", udim2(0.0, 0, 0.0, 280));
    set(&mut dom, tiles[1], "ZIndex", Value::Int32(5));
    set(&mut dom, tiles[2], "Size", udim2(1.0, 0, 0.0, 30));
    gui.sync(&mut dom);
    let moved = (positions(&dom, held), drawn(&mut gui, &dom));
    let mut fresh = laid_out(&mut dom);
    let again = (positions(&dom, held), drawn(&mut fresh, &dom));
    assert_eq!(moved.0, again.0);
    assert_eq!(moved.1, again.1);
    near(rect(&dom, tiles[0], f), ([0.0, 140.0], [100.0, 60.0]));
    // A ZIndex alone stacks again, nothing laid out.
    set(&mut dom, tiles[3], "ZIndex", Value::Int32(7));
    gui.sync(&mut dom);
    let restacked = drawn(&mut gui, &dom);
    assert_ne!(restacked, moved.1);
    assert_eq!(restacked, drawn(&mut laid_out(&mut dom), &dom));
}
