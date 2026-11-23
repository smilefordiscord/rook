//! A Path2D's curve where its parent is laid out: a cubic Bézier for each
//! pair of neighboring points, kept both as given (UDim2s, which the
//! position queries answer in) and in GUI pixels as drawn (which lengths,
//! tangents, bounds and the drawn stroke come from). The curve math is
//! kurbo's: Legendre-Gauss arc lengths with an error bound, exact extrema
//! for bounds, and Levien's flattening to a polyline within a tolerance.

use glam::Vec2;
use kurbo::{
    CubicBez, ParamCurve, ParamCurveArclen, ParamCurveDeriv, ParamCurveExtrema, PathEl, Point,
};
use rook_types::{UDim, UDim2};

use super::PathPoint;
use crate::layout::Placed;

/// How close, in pixels, an arc length is worked out.
const ACCURACY: f64 = 1e-6;

/// A Path2D's segments placed in its parent's frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PathCurve {
    /// Each segment's control points as given: its start, the start's
    /// handle, the end's handle and its end.
    given: Vec<[UDim2; 4]>,
    /// The same segments in GUI pixels (`AbsolutePosition`'s space), sized,
    /// scaled and turned with the parent.
    drawn: Vec<CubicBez>,
    /// Whether every drawn point is a number: a script can place a point at
    /// `math.huge` or NaN, which kurbo's arc lengths and flattening can't
    /// take (they subdivide without end, or ask for an endless buffer).
    finite: bool,
}

impl PathCurve {
    /// `points`' curve in `frame`, the box of the path's parent. Confirmed
    /// (creator-docs 2D paths): each segment runs from a point's position,
    /// through its position plus its right tangent and the next point's
    /// position plus its left tangent, to the next point's position.
    /// Confirmed (creator-docs `Closed`): a closed path of 3 or more points
    /// also joins the last back to the first.
    pub(crate) fn new(points: &[PathPoint], closed: bool, frame: Placed) -> Self {
        let n = points.len();
        let count = match n {
            0 | 1 => 0,
            _ if closed && n >= 3 => n,
            _ => n - 1,
        };
        let given: Vec<[UDim2; 4]> = (0..count)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                [
                    a.position,
                    a.position + a.right,
                    b.position + b.left,
                    b.position,
                ]
            })
            .collect();
        let at = |u: UDim2| {
            let local = Vec2::new(u.x.resolve(frame.local.x), u.y.resolve(frame.local.y));
            let p = frame.screen_point(local);
            Point::new(f64::from(p.x), f64::from(p.y))
        };
        let drawn: Vec<CubicBez> = (given.iter())
            .map(|c| CubicBez::new(at(c[0]), at(c[1]), at(c[2]), at(c[3])))
            .collect();
        let finite = (drawn.iter()).all(|c| [c.p0, c.p1, c.p2, c.p3].iter().all(|p| p.is_finite()));
        Self {
            given,
            drawn,
            finite,
        }
    }

    /// Whether the curve is placed anywhere: false when a point is at an
    /// infinite or NaN place. Inference: such a path draws nothing.
    pub(crate) fn is_finite(&self) -> bool {
        self.finite
    }

    pub fn segments(&self) -> usize {
        self.drawn.len()
    }

    /// The segment `t` (0 to 1 along the whole path, held at its ends)
    /// falls in, and how far along it. Inference: each segment takes an
    /// equal share of `t`, as Bézier splines are parameterized.
    fn split(&self, t: f32) -> (usize, f64) {
        let n = self.drawn.len();
        let s = f64::from(t.clamp(0.0, 1.0)) * n as f64;
        let i = (s.floor() as usize).min(n - 1);
        (i, s - i as f64)
    }

    /// Where `(segment, u)` from [`PathCurve::split`] is, as a UDim2 in the
    /// parent. Inference: each part of the UDim2s on the curve's
    /// polynomial, offsets rounded to whole pixels as `UDim2:Lerp` rounds
    /// them, so the point follows the parent as its control points do.
    fn given_at(&self, (i, u): (usize, f64)) -> UDim2 {
        let [p0, p1, p2, p3] = self.given[i];
        let v = 1.0 - u;
        let w = [v * v * v, 3.0 * v * v * u, 3.0 * v * u * u, u * u * u].map(|w| w as f32);
        let mix = |f: fn(UDim2) -> UDim| {
            let ps = [p0, p1, p2, p3].map(f);
            let scale = (0..4).map(|k| w[k] * ps[k].scale).sum::<f32>();
            let offset = (0..4).map(|k| w[k] * ps[k].offset as f32).sum::<f32>();
            UDim::new(scale, offset.round() as i32)
        };
        UDim2 {
            x: mix(|p| p.x),
            y: mix(|p| p.y),
        }
    }

    /// The curve's direction at `(segment, u)` on the screen, one long.
    /// Inference: a unit vector. Where the curve slows to a stop (an end
    /// whose handle sits on it), the way it leaves or arrives: towards the
    /// first control point apart from that end.
    fn tangent_at(&self, (i, u): (usize, f64)) -> Vec2 {
        let c = &self.drawn[i];
        // Slower than a millionth of a pixel per whole segment is stopped.
        let unit = |v: kurbo::Vec2| {
            (v.hypot() > 1e-6).then(|| Vec2::new(v.x as f32, v.y as f32).normalize())
        };
        let leaving = [c.p1 - c.p0, c.p2 - c.p0, c.p3 - c.p0];
        let arriving = [c.p3 - c.p2, c.p3 - c.p1, c.p3 - c.p0];
        let stopped = || {
            if u <= 0.0 {
                leaving.into_iter().find_map(unit)
            } else if u >= 1.0 {
                arriving.into_iter().find_map(unit)
            } else {
                unit(c.eval((u + 1e-3).min(1.0)) - c.eval((u - 1e-3).max(0.0)))
            }
        };
        (unit(c.deriv().eval(u).to_vec2()))
            .or_else(stopped)
            .unwrap_or(Vec2::ZERO)
    }

    /// The segment and place in it `t` (0 to 1) of the way along the
    /// path's length. Inference: a curve with no finite length is split by
    /// its parameter, as [`PathCurve::split`].
    fn split_by_length(&self, t: f32) -> (usize, f64) {
        if !self.finite {
            return self.split(t);
        }
        let lengths: Vec<f64> = self.drawn.iter().map(|c| c.arclen(ACCURACY)).collect();
        let mut left = lengths.iter().sum::<f64>() * f64::from(t.clamp(0.0, 1.0));
        for (i, &l) in lengths.iter().enumerate() {
            if left <= l || i + 1 == lengths.len() {
                let u = if l > 0.0 {
                    self.drawn[i].inv_arclen(left.min(l), ACCURACY)
                } else {
                    0.0
                };
                return (i, u);
            }
            left -= l;
        }
        (0, 0.0)
    }

    /// `GetPositionOnCurve`: `None` with fewer than two points.
    pub fn position(&self, t: f32) -> Option<UDim2> {
        (self.segments() > 0).then(|| self.given_at(self.split(t)))
    }

    /// `GetPositionOnCurveArcLength`.
    pub fn position_by_length(&self, t: f32) -> Option<UDim2> {
        (self.segments() > 0).then(|| self.given_at(self.split_by_length(t)))
    }

    /// `GetTangentOnCurve`.
    pub fn tangent(&self, t: f32) -> Option<Vec2> {
        (self.segments() > 0).then(|| self.tangent_at(self.split(t)))
    }

    /// `GetTangentOnCurveArcLength`.
    pub fn tangent_by_length(&self, t: f32) -> Option<Vec2> {
        (self.segments() > 0).then(|| self.tangent_at(self.split_by_length(t)))
    }

    /// `GetLength`, in pixels as drawn. Inference: infinite for a curve
    /// through a point at no finite place.
    pub fn length(&self) -> f32 {
        if !self.finite {
            return f32::INFINITY;
        }
        (self.drawn.iter()).fold(0.0, |sum, c| sum + c.arclen(ACCURACY)) as f32
    }

    /// The smallest box holding the curve, grown by `grow` pixels each
    /// way: its top left and bottom right. `None` with fewer than two
    /// points.
    pub fn bounds(&self, grow: f32) -> Option<(Vec2, Vec2)> {
        let r = (self.drawn.iter())
            .map(ParamCurveExtrema::bounding_box)
            .reduce(|a, b| a.union(b))?;
        let g = Vec2::splat(grow);
        Some((
            Vec2::new(r.x0 as f32, r.y0 as f32) - g,
            Vec2::new(r.x1 as f32, r.y1 as f32) + g,
        ))
    }

    /// The curve as a line through points no further than `tolerance`
    /// pixels from it, closed paths ending where they start. Points on a
    /// straight run are left out (kurbo cuts a straight cubic whose
    /// handles sit on its ends into several pieces), so each line drawn is
    /// as long as it can be.
    pub(crate) fn flatten(&self, tolerance: f32) -> Vec<Vec2> {
        let Some(first) = self.drawn.first().filter(|_| self.finite) else {
            return Vec::new();
        };
        let path = std::iter::once(PathEl::MoveTo(first.p0))
            .chain((self.drawn.iter()).map(|c| PathEl::CurveTo(c.p1, c.p2, c.p3)));
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

/// Whether `b` is on the straight line from `a` on to `c`, within a
/// thousandth of a pixel.
fn on_line(a: Vec2, b: Vec2, c: Vec2) -> bool {
    let (ab, ac) = (b - a, c - a);
    let len = ac.length();
    len > 0.0 && ab.dot(ac) > 0.0 && ac.dot(c - b) > 0.0 && ac.perp_dot(ab).abs() / len < 1e-3
}
