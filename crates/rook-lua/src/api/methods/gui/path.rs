//! Path2D's control points (creator-docs Path2D). The points live in the
//! GUI ([`rook_gui::Gui::path_points`]), which draws them; the cap and the
//! errors were measured in Studio.

use mlua::{AnyUserData, Lua, Table, Value as LuaValue};
use rook_dom::InstanceId;
use rook_gui::PathPoint;

use super::RawMethod;
use crate::runtime::ctx::args::{Args, borrow_clone};
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::types::LPath2DControlPoint;

/// Measured: `GetMaxControlPoints()` is 100.
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

fn store(lua: &Lua, this: InstanceId, points: Vec<PathPoint>) {
    with_world(lua, |w| w.gui.set_path_points(this, points));
}

/// Measured: `Path2D:GetControlPoint() at index 5 access out of bounds`.
fn out_of_bounds(member: &str, index: f64) -> ScriptError {
    ScriptError(format!(
        "Path2D:{member}() at index {} access out of bounds",
        crate::runtime::ctx::args::format_number(index)
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

/// Measured: past the cap the first 100 are kept and it raises; anything
/// but a point in the array is `Variant cast failed`.
fn set_points(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let LuaValue::Table(list) = a.get(0) else {
        return Err(a.wrong(0, "Array"));
    };
    let list: &Table = list;
    let mut out = Vec::new();
    for v in list.clone().sequence_values::<LuaValue>() {
        let point = match v? {
            LuaValue::UserData(ud) => cast(&ud),
            _ => None,
        }
        .ok_or_else(|| ScriptError("Variant cast failed".into()))?;
        out.push(to_point(&point));
    }
    let over = out.len() > MAX_POINTS;
    out.truncate(MAX_POINTS);
    store(lua, this, out);
    if over {
        return Err(ScriptError(format!(
            "Path2D:SetControlPoints() only supports {MAX_POINTS} control points."
        )));
    }
    Outcome::ret(lua, ())
}

fn cast(ud: &AnyUserData) -> Option<LPath2DControlPoint> {
    borrow_clone::<LPath2DControlPoint>(ud)
}

fn get_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let list = points(lua, this);
    let i = index(a, "GetControlPoint", list.len())?;
    Outcome::ret(lua, from_point(&list[i]))
}

/// Approximation: Studio refuses this on a path with no parent ("Attempting
/// to use Path2D with invalid parent"); Rook inserts wherever it is.
fn insert_point(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let mut list = points(lua, this);
    let i = index(a, "InsertControlPoint", list.len() + 1)?;
    let point = a.userdata::<LPath2DControlPoint>(1, "Path2DControlPoint")?;
    if list.len() >= MAX_POINTS {
        return Err(ScriptError(format!(
            "Path2D:InsertControlPoint() only supports {MAX_POINTS} control points."
        )));
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
