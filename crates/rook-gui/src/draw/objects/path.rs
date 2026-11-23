//! Path2D: its curve stroked `Thickness` pixels wide in `Color3`, among
//! its parent's children by its `ZIndex` (docs/behavior/gui/modern.md#path2d).
//!
//! The curve is flattened to a polyline within a fraction of a pixel, as
//! vector renderers do before stroking (Skia, Lyon, NanoVG), and each line
//! of it is a quad cut by its shape to a capsule, a rounded rectangle whose
//! radius is half the width: neighbors overlap in a disk at each joint,
//! which makes round joins and caps, as GPU line renderers that extrude
//! one instance per segment get them (Mapbox GL, deck.gl's PathLayer).
//! The overlaps would show through a translucent stroke, so one is drawn
//! opaque into a texture of its own first and laid over faded, as Skia's
//! `saveLayer` does for group opacity. A path draws again only when its
//! piece does: its points or looks changed, or its parent moved.

use std::sync::Arc;

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{GuiGradient, GuiGroup, GuiQuad, GuiShape, Paint};

use crate::draw::modifiers::gradient;
use crate::layout::Placed;
use crate::path::{PathCurve, PathPoint};

/// How far, in pixels as drawn, the drawn line may stray from the curve:
/// Skia's tessellation tolerance on the screen; finer on a world canvas,
/// which can be seen magnified.
const TOLERANCE: f32 = 0.25;
const WORLD_TOLERANCE: f32 = 1.0 / 16.0;

/// The path `id` with `points` in `frame` (its parent's box), clipped to
/// `clip`.
pub(in crate::draw) fn emit(
    dom: &Dom,
    id: InstanceId,
    (points, frame): (&[PathPoint], Placed),
    clip: [f32; 4],
    world: bool,
    out: &mut Vec<GuiQuad>,
) {
    // Confirmed: creator-docs, 0 to 100 pixels. Inference: UIScales widen
    // it, as they do a UIStroke's.
    let width = dom
        .float_property(id, "Thickness")
        .unwrap_or(1.0)
        .clamp(0.0, 100.0)
        * frame.scale;
    // Inference: files save a `Transparency`, which scripts can't set.
    let alpha = 1.0 - dom.float_property(id, "Transparency").unwrap_or(0.0);
    if width <= 0.0 || alpha <= 0.0 || points.len() < 2 {
        return;
    }
    let closed = dom.bool_property(id, "Closed") == Some(true);
    let curve = PathCurve::new(points, closed, frame);
    let line = curve.flatten(if world { WORLD_TOLERANCE } else { TOLERANCE });
    let Some((lo, hi)) = curve.bounds(width * 0.5) else {
        return;
    };
    let box_at = Placed {
        center: (lo + hi) * 0.5,
        local: hi - lo,
        scale: 1.0,
        rotation: 0.0,
    };
    // Confirmed: creator-docs Color3, a UIGradient under the path colors it
    // in place of `Color3`. Inference: over the stroke's bounding box.
    let gradient = gradient::of(dom, id, box_at, box_at.local);
    let color = match gradient {
        Some(_) => [1.0; 3],
        None => dom
            .color3_property(id, "Color3")
            .map_or([0.0; 3], |c| [c.r, c.g, c.b]),
    };
    // The lines, moved by `-shift`.
    let stroke = |clip: [f32; 4], shift: Vec2| -> Vec<GuiQuad> {
        let rgba = [color[0], color[1], color[2], 1.0];
        let mut quads: Vec<GuiQuad> = (line.windows(2))
            .map(|pair| capsule(pair[0] - shift, pair[1] - shift, width, rgba, clip))
            .collect();
        if let Some(g) = &gradient {
            let g = GuiGradient {
                origin: g.origin - shift,
                ..g.clone()
            };
            for q in &mut quads {
                q.gradient = Some(g.clone());
            }
        }
        quads
    };
    if alpha >= 1.0 {
        out.extend(stroke(clip, Vec2::ZERO));
        return;
    }
    // A pixel of room round the stroke for its smoothed edge.
    let (lo, hi) = (lo.floor() - Vec2::ONE, hi.ceil() + Vec2::ONE);
    let size = hi - lo;
    out.push(GuiQuad {
        center: (lo + hi) * 0.5,
        size,
        rotation: 0.0,
        color: [1.0, 1.0, 1.0, alpha],
        clip,
        paint: Paint::Group(Arc::new(GuiGroup {
            key: id.to_ref().0,
            size: [size.x as u32, size.y as u32],
            quads: stroke(GuiQuad::NO_CLIP, lo),
        })),
        shape: None,
        gradient: None,
    });
}

/// The line from `a` to `b`, `width` wide with round ends: a quad turned
/// along it, a pixel bigger each way than the capsule it's cut to so the
/// capsule's smoothed edge isn't cut off.
fn capsule(a: Vec2, b: Vec2, width: f32, color: [f32; 4], clip: [f32; 4]) -> GuiQuad {
    let d = b - a;
    let center = (a + b) * 0.5;
    let shape = Vec2::new(d.length() + width, width);
    GuiQuad {
        center,
        size: shape + Vec2::splat(2.0),
        rotation: d.y.atan2(d.x),
        color,
        clip,
        paint: Paint::Solid,
        shape: Some(GuiShape {
            center,
            size: shape,
            radii: [width * 0.5; 4],
            ring: 0.0,
            bevel: false,
        }),
        gradient: None,
    }
}
