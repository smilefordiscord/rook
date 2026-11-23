//! A Path2D's curve where its parent is laid out: a segment for each pair
//! of neighboring points, in the parent's own pixels (from its top left,
//! before UIScales and turning), which the queries answer in, and placed
//! on the screen for drawing. The curve math is kurbo's: Legendre-Gauss
//! arc lengths with an error bound, exact extrema for bounds, and
//! Levien's flattening to a polyline within a tolerance.

use glam::Vec2;
use kurbo::{
    Affine, CubicBez, Line, ParamCurve, ParamCurveArclen, ParamCurveDeriv, ParamCurveExtrema,
    PathEl, PathSeg, Point, QuadBez,
};
use rook_types::UDim2;

use super::PathPoint;
use crate::layout::Placed;

/// How close, in pixels, an arc length is worked out.
const ACCURACY: f64 = 1e-6;

/// A Path2D's segments placed in its parent's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PathCurve {
    /// The segments in the parent's pixels.
    segments: Vec<PathSeg>,
    /// Where each point is in the parent's pixels.
    positions: Vec<Point>,
    /// The parent's box, which `segments` are placed on the screen by.
    frame: Placed,
    /// Whether every control point is a number: a script can place a point
    /// at `math.huge` or NaN, which kurbo's arc lengths and flattening
    /// can't take (they subdivide without end, or ask for an endless
    /// buffer).
    finite: bool,
}

impl PathCurve {
    /// `points`' curve in `frame`, the box of the path's parent.
    /// Confirmed (Studio 0.748): a segment runs from a point's position to
    /// the next's, a straight line when both tangents between them are
    /// zero, a quadratic Bézier through the one handle when one is (its
    /// tangent at the zero end leads to that handle, at twice its
    /// distance), and otherwise a cubic through the point's position plus
    /// its right tangent and the next's position plus its left.
    pub(crate) fn new(points: &[PathPoint], closed: bool, frame: Placed) -> Self {
        let n = points.len();
        let count = segment_count(n, closed);
        let at = |u: UDim2| {
            Point::new(
                f64::from(u.x.resolve(frame.local.x)),
                f64::from(u.y.resolve(frame.local.y)),
            )
        };
        let segments: Vec<PathSeg> = (0..count)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                let (start, end) = (at(a.position), at(b.position));
                let zero = UDim2::default();
                match (a.right == zero, b.left == zero) {
                    (true, true) => PathSeg::Line(Line::new(start, end)),
                    (false, true) => {
                        PathSeg::Quad(QuadBez::new(start, at(a.position + a.right), end))
                    }
                    (true, false) => {
                        PathSeg::Quad(QuadBez::new(start, at(b.position + b.left), end))
                    }
                    (false, false) => PathSeg::Cubic(CubicBez::new(
                        start,
                        at(a.position + a.right),
                        at(b.position + b.left),
                        end,
                    )),
                }
            })
            .collect();
        let positions: Vec<Point> = points.iter().map(|p| at(p.position)).collect();
        let finite = positions.iter().all(|p| p.is_finite())
            && segments
                .iter()
                .all(|s| controls(s).iter().all(|p| p.is_finite()));
        Self {
            segments,
            positions,
            frame,
            finite,
        }
    }

    /// Whether the curve is placed anywhere: false when a point is at an
    /// infinite or NaN place. Inference: such a path draws nothing.
    pub(crate) fn is_finite(&self) -> bool {
        self.finite
    }

    pub fn segments(&self) -> usize {
        self.segments.len()
    }

    /// The segment `t` (0 to 1 along the whole path, held at its ends)
    /// falls in, and how far along it. Confirmed (Studio 0.748): each
    /// segment takes an equal share of `t`, a segment's start belonging to
    /// it rather than to the one before, and NaN is the start.
    fn split(&self, t: f32) -> (usize, f64) {
        let n = self.segments.len();
        let s = held(t) * n as f64;
        let i = (s.floor() as usize).min(n - 1);
        (i, s - i as f64)
    }

    /// The segment and place in it `t` (0 to 1) of the way along the
    /// path's length. Confirmed (Studio 0.748): by the segments' true arc
    /// lengths, a place where two meet belonging to the earlier one.
    fn split_by_length(&self, t: f32) -> (usize, f64) {
        let lengths: Vec<f64> = self.segments.iter().map(length_of).collect();
        let mut left = lengths.iter().sum::<f64>() * held(t);
        let last = lengths.len() - 1;
        for (i, (&l, seg)) in lengths.iter().zip(&self.segments).enumerate() {
            if left <= l || i == last {
                return (i, along(seg, l, left));
            }
            left -= l;
        }
        (last, 1.0)
    }

    /// Where `(segment, u)` is, as a UDim2 of the parent's size.
    /// Confirmed (Studio 0.748): in scale alone, offsets 0.
    fn position_at(&self, (i, u): (usize, f64)) -> UDim2 {
        let p = self.segments[i].eval(u);
        let size = self.frame.local;
        UDim2::from_scale(p.x as f32 / size.x, p.y as f32 / size.y)
    }

    /// The curve's derivative at `(segment, u)` in the parent's pixels.
    /// Confirmed (Studio 0.748): by the segment's own parameter, not
    /// made one long, and zero where it stops.
    fn tangent_at(&self, (i, u): (usize, f64)) -> Vec2 {
        let d = match self.segments[i] {
            PathSeg::Line(l) => l.p1 - l.p0,
            PathSeg::Quad(q) => q.deriv().eval(u).to_vec2(),
            PathSeg::Cubic(c) => c.deriv().eval(u).to_vec2(),
        };
        Vec2::new(d.x as f32, d.y as f32)
    }

    /// `GetPositionOnCurve`: `None` with no segments.
    pub fn position(&self, t: f32) -> Option<UDim2> {
        (self.segments() > 0).then(|| self.position_at(self.split(t)))
    }

    /// `GetPositionOnCurveArcLength`.
    pub fn position_by_length(&self, t: f32) -> Option<UDim2> {
        (self.segments() > 0).then(|| self.position_at(self.split_by_length(t)))
    }

    /// `GetTangentOnCurve`.
    pub fn tangent(&self, t: f32) -> Option<Vec2> {
        (self.segments() > 0).then(|| self.tangent_at(self.split(t)))
    }

    /// `GetTangentOnCurveArcLength`.
    pub fn tangent_by_length(&self, t: f32) -> Option<Vec2> {
        (self.segments() > 0).then(|| self.tangent_at(self.split_by_length(t)))
    }

    /// `GetLength`, in the parent's pixels. Confirmed (Studio 0.748):
    /// infinite through a point at an infinite place, NaN through one at
    /// NaN.
    pub fn length(&self) -> f32 {
        // Folded from 0: an empty sum is -0.
        (self.segments.iter()).fold(0.0, |sum, s| sum + length_of(s)) as f32
    }

    /// `GetBoundingRect`: the box round the points' positions (not the
    /// curve between them) on the screen, its top left and bottom right.
    /// Confirmed (Studio 0.748): grown by half of `thickness` and half a
    /// pixel each way, and not at all at thickness 0. `None` with no
    /// points.
    pub fn bounding_rect(&self, thickness: f32) -> Option<(Vec2, Vec2)> {
        let grow = if thickness > 0.0 {
            thickness * 0.5 + 0.5
        } else {
            0.0
        };
        let screen = |p: &Point| self.frame.screen_point(Vec2::new(p.x as f32, p.y as f32));
        let (lo, hi) =
            (self.positions.iter().map(screen)).fold(None, |acc: Option<(Vec2, Vec2)>, p| {
                Some(acc.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))))
            })?;
        Some((lo - Vec2::splat(grow), hi + Vec2::splat(grow)))
    }

    /// The segments on the screen.
    fn drawn(&self) -> impl Iterator<Item = PathSeg> + '_ {
        let f = self.frame;
        let to_screen = Affine::translate((f64::from(f.center.x), f64::from(f.center.y)))
            * Affine::rotate(f64::from(f.rotation.to_radians()))
            * Affine::scale(f64::from(f.scale))
            * Affine::translate((-f64::from(f.local.x) * 0.5, -f64::from(f.local.y) * 0.5));
        self.segments.iter().map(move |&s| to_screen * s)
    }

    /// The smallest box holding the curve as drawn, grown by `grow` pixels
    /// each way: its top left and bottom right. `None` with no segments.
    pub(crate) fn drawn_bounds(&self, grow: f32) -> Option<(Vec2, Vec2)> {
        let r = (self.drawn())
            .map(|s| s.bounding_box())
            .reduce(|a, b| a.union(b))?;
        let g = Vec2::splat(grow);
        Some((
            Vec2::new(r.x0 as f32, r.y0 as f32) - g,
            Vec2::new(r.x1 as f32, r.y1 as f32) + g,
        ))
    }

    /// The curve as drawn, as a line through points no further than
    /// `tolerance` pixels from it, closed paths ending where they start.
    /// Points on a straight run are left out (kurbo cuts a straight curve
    /// into several pieces), so each line drawn is as long as it can be.
    pub(crate) fn flatten(&self, tolerance: f32) -> Vec<Vec2> {
        if !self.finite {
            return Vec::new();
        }
        let mut drawn = self.drawn().peekable();
        let Some(first) = drawn.peek().map(|s| controls(s)[0]) else {
            return Vec::new();
        };
        let path = std::iter::once(PathEl::MoveTo(first)).chain(drawn.map(|s| s.as_path_el()));
        let mut out: Vec<Vec2> = Vec::new();
        kurbo::flatten(path, f64::from(tolerance), |el| {
            let (PathEl::MoveTo(p) | PathEl::LineTo(p)) = el else {
                return;
            };
            let p = Vec2::new(p.x as f32, p.y as f32);
            match out[..] {
                [.., last] if last.distance_squared(p) < 1e-8 => {}
                [.., a, b] if on_line(a, b, p) => *out.last_mut().expect("two") = p,
                _ => out.push(p),
            }
        });
        out
    }
}

/// How many segments `points` points make. Confirmed (creator-docs
/// `Closed`): a closed path of 3 or more points also joins the last back
/// to the first.
pub(super) fn segment_count(points: usize, closed: bool) -> usize {
    match points {
        0 | 1 => 0,
        n if closed && n >= 3 => n,
        n => n - 1,
    }
}

/// `t` held within 0 to 1, NaN as 0.
fn held(t: f32) -> f64 {
    if t.is_nan() {
        0.0
    } else {
        f64::from(t.clamp(0.0, 1.0))
    }
}

/// A segment's control points, its start first.
fn controls(s: &PathSeg) -> Vec<Point> {
    match *s {
        PathSeg::Line(l) => vec![l.p0, l.p1],
        PathSeg::Quad(q) => vec![q.p0, q.p1, q.p2],
        PathSeg::Cubic(c) => vec![c.p0, c.p1, c.p2, c.p3],
    }
}

/// A segment's arc length. One through a point at no finite place is the
/// length round its control points, which is infinite or NaN as they are
/// (exact for a line).
fn length_of(s: &PathSeg) -> f64 {
    let points = controls(s);
    if points.iter().all(|p| p.is_finite()) {
        return s.arclen(ACCURACY);
    }
    points.windows(2).map(|w| (w[1] - w[0]).hypot()).sum()
}

/// The parameter `distance` along `s`, which is `length` long. A line's,
/// or one at no finite place, is in proportion (NaN where the distance
/// is); a segment with no length is at its start.
fn along(s: &PathSeg, length: f64, distance: f64) -> f64 {
    if length == 0.0 {
        return 0.0;
    }
    match s {
        PathSeg::Line(_) => distance / length,
        _ if !length.is_finite() || !distance.is_finite() => distance / length,
        _ => s.inv_arclen(distance.clamp(0.0, length), ACCURACY),
    }
}

/// Whether `b` is on the straight line from `a` on to `c`, within a
/// thousandth of a pixel.
fn on_line(a: Vec2, b: Vec2, c: Vec2) -> bool {
    let (ab, ac) = (b - a, c - a);
    let len = ac.length();
    len > 0.0 && ab.dot(ac) > 0.0 && ac.dot(c - b) > 0.0 && ac.perp_dot(ab).abs() / len < 1e-3
}
