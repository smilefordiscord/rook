//! Path2D's methods (docs/behavior/gui/modern.md#path2d): its control
//! points, which live in the GUI ([`rook_gui::Gui::path_points`]), and
//! where its curve runs as laid out now ([`rook_gui::PathCurve`]). The cap
//! is creator-docs'; the setters' errors are Inference.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_gui::{PathCurve, PathPoint};
use rook_types::Vector2;

use super::RawMethod;
use crate::runtime::ctx::args::{Args, borrow_clone, format_number};
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
    with_world(lua, |w| w.gui.path_points(this).to_vec())
}

/// Keep `points` as the path's and fire `ControlPointChanged`
/// (Confirmed: creator-docs, for each of the four setters).
fn store(lua: &Lua, this: InstanceId, points: Vec<PathPoint>) {
    with_world(lua, |w| w.gui.set_path_points(this, points));
    fire(lua, &SignalKey::event(this, "ControlPointChanged"), &[]);
}

/// Inference: `Path2D:GetControlPoint() at index 5 access out of bounds`.
fn out_of_bounds(member: &str, index: f64) -> ScriptError {
    ScriptError(format!(
        "Path2D:{member}() at index {} access out of bounds",
        format_number(index)
    ))
}

fn too_many(member: &str) -> ScriptError {
    ScriptError(format!(
        "Path2D:{member}() only supports {MAX_POINTS} control points."
    ))
}

/// A 1-based index within `1..=last`, or the member's error.
fn index(a: &Args, member: &str, last: usize) -> ApiResult<usize> {
    let i = a.number(0)?;
    if i.fract() != 0.0 || i < 1.0 || i > last as f64 {
        return Err(out_of_bounds(member, i));
    }
    Ok(i as usize - 1)
}

fn get_points(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let list = points(lua, this);
    let t = lua.create_table_with_capacity(list.len(), 0)?;
    for (i, p) in list.iter().enumerate() {
        t.raw_set(i + 1, from_point(p))?;
    }
    Outcome::ret(lua, t)
}

/// Inference: past the cap the first 100 are kept and it raises; anything
/// but a point in the array is `Variant cast failed`.
fn set_points(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let LuaValue::Table(list) = a.get(0) else {
        return Err(a.wrong(0, "Array"));
    };
    let mut out = Vec::new();
    for v in list.clone().sequence_values::<LuaValue>() {
        let point = match v? {
            LuaValue::UserData(ud) => borrow_clone::<LPath2DControlPoint>(&ud),
            _ => None,
        }
        .ok_or_else(|| ScriptError("Variant cast failed".into()))?;
        out.push(to_point(&point));
    }
    let over = out.len() > MAX_POINTS;
    out.truncate(MAX_POINTS);
    store(lua, this, out);
    if over {
        return Err(too_many("SetControlPoints"));
    }
    Outcome::ret(lua, ())
}

fn get_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let list = points(lua, this);
    let i = index(a, "GetControlPoint", list.len())?;
    Outcome::ret(lua, from_point(&list[i]))
}

/// Rook inserts wherever the path is. Inference: Studio refuses this on a
/// path with no parent ("Attempting to use Path2D with invalid parent").
fn insert_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let mut list = points(lua, this);
    let i = index(a, "InsertControlPoint", list.len() + 1)?;
    let point = a.userdata::<LPath2DControlPoint>(1, "Path2DControlPoint")?;
    if list.len() >= MAX_POINTS {
        return Err(too_many("InsertControlPoint"));
    }
    list.insert(i, to_point(&point));
    store(lua, this, list);
    Outcome::ret(lua, ())
}

fn remove_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let mut list = points(lua, this);
    let i = index(a, "RemoveControlPoint", list.len())?;
    list.remove(i);
    store(lua, this, list);
    Outcome::ret(lua, ())
}

fn update_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let mut list = points(lua, this);
    let i = index(a, "UpdateControlPoint", list.len())?;
    let point = a.userdata::<LPath2DControlPoint>(1, "Path2DControlPoint")?;
    list[i] = to_point(&point);
    store(lua, this, list);
    Outcome::ret(lua, ())
}

fn curve(lua: &Lua, this: InstanceId) -> PathCurve {
    with_world(lua, |w| w.gui.path_curve(&mut w.dom, this))
}

/// Confirmed (creator-docs): the curve queries raise with fewer than two
/// points. Inference: the message.
fn too_few(member: &str) -> ScriptError {
    ScriptError(format!(
        "Path2D:{member}() requires at least 2 control points"
    ))
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
    let t = a.number(0)? as f32;
    let at = at(&curve(lua, this), t).ok_or_else(|| too_few(member))?;
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
    let t = a.number(0)? as f32;
    let v = at(&curve(lua, this), t).ok_or_else(|| too_few(member))?;
    Outcome::ret(lua, LVector2(Vector2::new(v.x, v.y)))
}

/// In pixels as drawn. Inference: 0 with fewer than two points.
fn get_length(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, f64::from(curve(lua, this).length()))
}

/// Confirmed (creator-docs): the box round the curve, grown by its
/// thickness. Inference: half of it each way (the stroke's edge), in the
/// pixels `AbsolutePosition` is in, and an empty Rect with fewer than two
/// points.
fn get_bounding_rect(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let thickness = with_world(lua, |w| w.dom.float_property(this, "Thickness"));
    let grow = thickness.unwrap_or(1.0).clamp(0.0, 100.0) * 0.5;
    let (lo, hi) = (curve(lua, this).bounds(grow)).unwrap_or_default();
    let v = |p: glam::Vec2| rbx_types::Vector2::new(p.x, p.y);
    Outcome::ret(lua, LRect(rbx_types::Rect::new(v(lo), v(hi))))
}
