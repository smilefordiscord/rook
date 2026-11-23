//! DockWidgetPluginGuiInfo: where a plugin's dock widget starts and how
//! big it may be, made by `DockWidgetPluginGuiInfo.new` (creator-docs
//! DockWidgetPluginGuiInfo; docs/behavior/studio/plugins.md#dock-widgets).
//! Scripts can only print and compare it: Studio has no member to read
//! (Measured).

use mlua::{Lua, MetaMethod, MultiValue, UserData, UserDataFields, UserDataMethods, Value};

use crate::types::enums::{LEnumItem, has_value};
use crate::types::err;
use crate::types::members::{Wording, add_unknown, not_a_member};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LDockWidgetInfo {
    /// The `InitialDockState` item's name.
    pub(crate) dock: &'static str,
    pub(crate) initial_enabled: bool,
    pub(crate) override_restore: bool,
    /// Pixels; whole numbers, as Studio keeps them.
    pub(crate) float_size: (i32, i32),
    pub(crate) min_size: (i32, i32),
}

impl Default for LDockWidgetInfo {
    /// What `DockWidgetPluginGuiInfo.new()` gives (Measured).
    fn default() -> Self {
        Self {
            dock: "Right",
            initial_enabled: false,
            override_restore: false,
            float_size: (0, 0),
            min_size: (0, 0),
        }
    }
}

impl UserData for LDockWidgetInfo {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "DockWidgetPluginGuiInfo");
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        // Measured: reading any key is `'key' is not a valid member of
        // DockWidgetPluginGuiInfo`, writing one is the same unquoted.
        add_unknown(m, "DockWidgetPluginGuiInfo", Wording::Quoted);
        m.add_meta_method(MetaMethod::NewIndex, |_, _, (key, _): (Value, Value)| {
            Err::<(), _>(not_a_member(
                &key,
                "DockWidgetPluginGuiInfo",
                Wording::Plain,
            ))
        });
        // Measured: infos made from the same arguments are equal.
        m.add_meta_method(MetaMethod::Eq, |_, t, other: mlua::AnyUserData| {
            Ok(other.borrow::<Self>().is_ok_and(|o| *t == *o))
        });
        // Measured: the booleans print as 0 and 1.
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!(
                "InitialDockState:{} InitialEnabled:{} InitialEnabledShouldOverrideRestore:{} \
                 FloatingXSize:{} FloatingYSize:{} MinWidth:{} MinHeight:{}",
                t.dock,
                u8::from(t.initial_enabled),
                u8::from(t.override_restore),
                t.float_size.0,
                t.float_size.1,
                t.min_size.0,
                t.min_size.1
            ))
        });
    }
}

/// The `InitialDockState` item an argument names: the item, or its value
/// as a number (truncated). Not its name (Measured).
fn dock(v: &Value) -> Option<&'static str> {
    let e = rook_reflection::database().enum_by_name("InitialDockState")?;
    let value = match v {
        Value::UserData(u) => {
            let item = u.borrow::<LEnumItem>().ok()?;
            (item.enum_id == e.id).then_some(item.value)?
        }
        Value::Integer(n) => u32::try_from(*n).ok()?,
        Value::Number(n) if *n >= 0.0 && *n < f64::from(u32::MAX) => *n as u32,
        _ => return None,
    };
    has_value(e.id, value).then(|| e.item_name(value)).flatten()
}

/// A size argument as Studio keeps it: truncated toward zero, and
/// `i32::MIN` for anything out of an `i32`'s range, NaN included
/// (Measured: 12.7 is 12, -2.7 is -2, 2^31, -1e12 and inf are
/// -2147483648). Not a numeric string (Measured).
fn int(v: &Value) -> Option<i32> {
    let n = match v {
        Value::Integer(n) => *n as f64,
        Value::Number(n) => *n,
        _ => return None,
    };
    let in_range = n > f64::from(i32::MIN) - 1.0 && n < f64::from(i32::MAX) + 1.0;
    Some(if in_range { n as i32 } else { i32::MIN })
}

/// `DockWidgetPluginGuiInfo.new(dock, enabled, overrideRestore, floatX,
/// floatY, minWidth, minHeight)`. Arguments not passed keep their
/// defaults; each one passed, `nil` included, must have its type, with
/// Studio's message for each (Measured).
fn new(_: &Lua, args: MultiValue) -> mlua::Result<LDockWidgetInfo> {
    let wrong = |message: &str| err(format!("DockWidgetPluginGuiInfo.new {message}"));
    let flag = |v: &Value, message: &str| match v {
        Value::Boolean(b) => Ok(*b),
        _ => Err(wrong(message)),
    };
    let size = |v: &Value, ordinal: &str, name: &str| {
        int(v).ok_or_else(|| wrong(&format!("{ordinal} argument expects an int for {name}.")))
    };
    let mut info = LDockWidgetInfo::default();
    for (i, v) in args.into_iter().enumerate() {
        match i {
            0 => {
                info.dock = dock(&v)
                    .ok_or_else(|| wrong("first argument expects InitialDockState input"))?;
            }
            1 => {
                info.initial_enabled =
                    flag(&v, "second arg should be a boolean for InitialEnabled.")?;
            }
            2 => {
                info.override_restore = flag(
                    &v,
                    "third arg should be a boolean for InitialEnabledShouldOverrideRestore.",
                )?;
            }
            3 => info.float_size.0 = size(&v, "fourth", "FloatingXSize")?,
            4 => info.float_size.1 = size(&v, "fifth", "FloatingYSize")?,
            5 => info.min_size.0 = size(&v, "sixth", "MinWidth")?,
            6 => info.min_size.1 = size(&v, "seventh", "MinHeight")?,
            _ => break,
        }
    }
    Ok(info)
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let t = lua.create_table()?;
    t.set("new", lua.create_function(new)?)?;
    lua.globals().set("DockWidgetPluginGuiInfo", t)
}
