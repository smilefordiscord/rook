//! Path2D's methods (docs/behavior/gui/modern.md#path2d): its control
//! points, kept in its `PropertiesSerialize` ([`rook_gui::path_points`])
//! and its spline (`rook-gui`'s `path/spline.rs`), and where its curve
//! runs as laid out now ([`rook_gui::PathCurve`]). Arguments are cast as
//! Roblox's reflection casts them; the order of the checks after that is
//! Studio 0.748's.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_gui::{PathCurve, PathPoint};
use rook_types::Vector2;

use super::RawMethod;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::runtime::signal::SignalKey;
use crate::runtime::threads::fire;
use crate::types::{LPath2DControlPoint, LRect, LUDim2, LVector2};

/// Confirmed: creator-docs `GetMaxControlPoints`, 100.
const MAX_POINTS: usize = 100;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Path2D", "GetMaxControlPoints") => |l, _, _| Outcome::ret(l, MAX_POINTS as f64),
        ("Path2D", "GetControlPoints") => get_points,
        ("Path2D", "SetControlPoints") => set_points,
        ("Path2D", "GetControlPoint") => get_point,
        ("Path2D", "InsertControlPoint") => insert_point,
        ("Path2D", "RemoveControlPoint") => remove_point,
        ("Path2D", "UpdateControlPoint") => update_point,
        ("Path2D", "GetLength") => get_length,
        ("Path2D", "GetBoundingRect") => get_bounding_rect,
        ("Path2D", "GetPositionOnCurve") => {
            |l, this, a| position(l, this, a, "GetPositionOnCurve", PathCurve::position)
        }
        ("Path2D", "GetPositionOnCurveArcLength") => |l, this, a| {
            let by_length = PathCurve::position_by_length;
            position(l, this, a, "GetPositionOnCurveArcLength", by_length)
        },
        ("Path2D", "GetTangentOnCurve") => {
            |l, this, a| tangent(l, this, a, "GetTangentOnCurve", PathCurve::tangent)
        }
        ("Path2D", "GetTangentOnCurveArcLength") => |l, this, a| {
            let by_length = PathCurve::tangent_by_length;
            tangent(l, this, a, "GetTangentOnCurveArcLength", by_length)
        },
        _ => return None,
    })
}

fn to_point(p: &LPath2DControlPoint) -> PathPoint {
    PathPoint {
        position: p.position,
        left: p.left,
        right: p.right,
    }
}

fn from_point(p: &PathPoint) -> LPath2DControlPoint {
    LPath2DControlPoint {
        position: p.position,
        left: p.left,
        right: p.right,
    }
}

fn points(lua: &Lua, this: InstanceId) -> Vec<PathPoint> {
    with_world(lua, |w| rook_gui::path_points(&w.dom, this))
}

/// Keep `points` as the path's.
fn store(lua: &Lua, this: InstanceId, points: &[PathPoint]) -> ApiResult<()> {
    with_world(lua, |w| rook_gui::set_path_points(&mut w.dom, this, points))?;
    Ok(())
}

/// Confirmed (creator-docs; Studio 0.748): each setter fires
/// `ControlPointChanged`, with no arguments, once it has finished; one
/// that raises fires nothing, whatever it changed first.
fn changed(lua: &Lua, this: InstanceId) -> ApiResult<Outcome> {
    fire(lua, &SignalKey::event(this, "ControlPointChanged"), &[]);
    Outcome::ret(lua, ())
}

/// Confirmed (Studio 0.748): `Path2D:GetControlPoint() at index 5 access
/// out of bounds`, the index as cast.
fn out_of_bounds(member: &str, index: i32) -> ScriptError {
    ScriptError(format!(
        "Path2D:{member}() at index {index} access out of bounds"
    ))
}

/// Confirmed (Studio 0.748).
fn too_many(member: &str) -> ScriptError {
    ScriptError(format!(
        "Path2D:{member}() only supports {MAX_POINTS} control points."
    ))
}

/// Confirmed (Studio 0.748).
fn invalid_parent() -> ScriptError {
    ScriptError("Attempting to use Path2D with invalid parent".into())
}

/// Confirmed (Studio 0.748): the spline's own message, its index from 0.
fn spline_out_of_bounds(doing: &str, index: usize) -> ScriptError {
    ScriptError(format!(
        "Spline {doing} control point at index {index} access out of bounds"
    ))
}

/// Argument 1, an `int`, as a 0-based index within `1..=last`, or the
/// member's error.
fn index(a: &Args, member: &str, last: usize) -> ApiResult<usize> {
    let i = a.int(0)?;
    match usize::try_from(i) {
        Ok(n) if (1..=last).contains(&n) => Ok(n - 1),
        _ => Err(out_of_bounds(member, i)),
    }
}

fn placed(lua: &Lua, this: InstanceId) -> bool {
    with_world(lua, |w| rook_gui::path_is_placed(&w.dom, this))
}

fn get_points(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let list = points(lua, this);
    let t = lua.create_table_with_capacity(list.len(), 0)?;
    for (i, p) in list.iter().enumerate() {
        t.raw_set(i + 1, from_point(p))?;
    }
    Outcome::ret(lua, t)
}

/// Confirmed (Studio 0.748): the points are emptied, then each item added
/// in turn, so an item that isn't a point (`Variant cast failed`) or the
/// 101st raises with those before it kept. No parent is needed.
fn set_points(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let items = a.array(0)?;
    with_world(lua, |w| w.gui.empty_path_spline(this));
    let mut out = Vec::new();
    let mut failed = None;
    for item in &items {
        let point = match item {
            LuaValue::UserData(ud) => ud.borrow::<LPath2DControlPoint>().ok().map(|p| *p),
            _ => None,
        };
        let Some(point) = point else {
            failed = Some(ScriptError("Variant cast failed".into()));
            break;
        };
        if out.len() >= MAX_POINTS {
            failed = Some(too_many("SetControlPoints"));
            break;
        }
        out.push(to_point(&point));
    }
    store(lua, this, &out)?;
    match failed {
        Some(e) => Err(e),
        None => changed(lua, this),
    }
}

fn get_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let list = points(lua, this);
    let i = index(a, "GetControlPoint", list.len())?;
    Outcome::ret(lua, from_point(&list[i]))
}

/// Confirmed (Studio 0.748): with no parent to place the point in it
/// raises and changes nothing; with one, the point is inserted, then the
/// spline told.
fn insert_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    a.int(0)?;
    let point = a.datatype::<LPath2DControlPoint>(1, "Path2DControlPoint")?;
    let mut list = points(lua, this);
    if list.len() >= MAX_POINTS {
        return Err(too_many("InsertControlPoint"));
    }
    let i = index(a, "InsertControlPoint", list.len() + 1)?;
    if !placed(lua, this) {
        return Err(invalid_parent());
    }
    list.insert(i, to_point(&point));
    store(lua, this, &list)?;
    if !with_world(lua, |w| w.gui.insert_into_path_spline(this, i)) {
        return Err(spline_out_of_bounds("inserting", i));
    }
    changed(lua, this)
}

/// Confirmed (Studio 0.748): no parent is needed; the point is removed,
/// then the spline told.
fn remove_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let mut list = points(lua, this);
    let i = index(a, "RemoveControlPoint", list.len())?;
    list.remove(i);
    store(lua, this, &list)?;
    if !with_world(lua, |w| w.gui.remove_from_path_spline(this, i)) {
        return Err(spline_out_of_bounds("removing", i));
    }
    changed(lua, this)
}

/// Confirmed (Studio 0.748): with no parent it raises and changes nothing.
/// Inference: the spline takes the new point wherever it is.
fn update_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    a.int(0)?;
    let point = a.datatype::<LPath2DControlPoint>(1, "Path2DControlPoint")?;
    let mut list = points(lua, this);
    let i = index(a, "UpdateControlPoint", list.len())?;
    if !placed(lua, this) {
        return Err(invalid_parent());
    }
    list[i] = to_point(&point);
    store(lua, this, &list)?;
    changed(lua, this)
}

/// The path's curve, or the invalid parent error.
fn curve(lua: &Lua, this: InstanceId) -> ApiResult<PathCurve> {
    with_world(lua, |w| w.gui.path_curve(&mut w.dom, this)).ok_or_else(invalid_parent)
}

/// Confirmed (Studio 0.748): what the curve queries raise with fewer than
/// two points, checked before the parent.
fn no_segments(member: &str) -> ScriptError {
    ScriptError(format!(
        "Path2D:{member}() attempting to query with zero segments"
    ))
}

/// The curve to query with argument 1, `t` (a `float`): `no_segments`
/// with none. Inference: the parent is checked as `GetLength` checks it.
fn queried(lua: &Lua, this: InstanceId, a: &Args, member: &str) -> ApiResult<(PathCurve, f32)> {
    let t = a.float(0)?;
    if with_world(lua, |w| rook_gui::path_segments(&w.dom, this)) == 0 {
        return Err(no_segments(member));
    }
    Ok((curve(lua, this)?, t))
}

/// `GetPositionOnCurve` and its arc-length twin: a UDim2 in the path's
/// parent, `t` from 0 to 1 along the path.
fn position(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
    member: &str,
    at: fn(&PathCurve, f32) -> Option<rook_types::UDim2>,
) -> ApiResult<Outcome> {
    let (curve, t) = queried(lua, this, a, member)?;
    let at = at(&curve, t).ok_or_else(|| no_segments(member))?;
    Outcome::ret(lua, LUDim2(at))
}

/// `GetTangentOnCurve` and its arc-length twin.
fn tangent(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
    member: &str,
    at: fn(&PathCurve, f32) -> Option<glam::Vec2>,
) -> ApiResult<Outcome> {
    let (curve, t) = queried(lua, this, a, member)?;
    let v = at(&curve, t).ok_or_else(|| no_segments(member))?;
    Outcome::ret(lua, LVector2(Vector2::new(v.x, v.y)))
}

/// In the parent's pixels. Inference: 0 with fewer than two points.
fn get_length(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, f64::from(curve(lua, this)?.length()))
}

/// In the pixels `AbsolutePosition` is in. Inference: an empty Rect with
/// no points.
fn get_bounding_rect(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let curve = curve(lua, this)?;
    let thickness = with_world(lua, |w| w.dom.float_property(this, "Thickness"));
    let (lo, hi) = (curve.bounding_rect(thickness.unwrap_or(1.0))).unwrap_or_default();
    let v = |p: glam::Vec2| rbx_types::Vector2::new(p.x, p.y);
    Outcome::ret(lua, LRect(rbx_types::Rect::new(v(lo), v(hi))))
}
