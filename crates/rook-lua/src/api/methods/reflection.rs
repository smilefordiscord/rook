//! ReflectionService: the class database as scripts read it, what
//! Studio's Properties window and plugins build their views from
//! (docs/behavior/studio/plugins.md#reflectionservice). `Permits` holds
//! the capability needed as a string, where Roblox gives a
//! SecurityCapabilities (Approximation: Rook has no SecurityCapabilities
//! datatype; docs/guide/stubs.md).

use mlua::{Lua, Table, Value as LuaValue};
use rbx_types::VariantType;
use rook_dom::InstanceId;
use rook_reflection::Class;
use rook_reflection::members::Security;
use rook_reflection::{
    Property, PropertyKind, PropertyType, Scriptability, Serialization, database, members,
};

use crate::api::methods::RawMethod;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome};

/// What the API dump and Studio call a value type: the engine's name and
/// Luau's.
fn type_names(ty: VariantType) -> (&'static str, &'static str) {
    use VariantType as V;
    match ty {
        V::Bool => ("bool", "boolean"),
        V::Float32 => ("float", "number"),
        V::Float64 => ("double", "number"),
        V::Int32 => ("int", "number"),
        V::Int64 => ("int64", "number"),
        V::String => ("string", "string"),
        V::BinaryString => ("BinaryString", "string"),
        V::Vector2 => ("Vector2", "Vector2"),
        V::Vector3 => ("Vector3", "Vector3"),
        V::CFrame => ("CoordinateFrame", "CFrame"),
        V::OptionalCFrame => ("OptionalCoordinateFrame", "CFrame"),
        V::Color3 | V::Color3uint8 => ("Color3", "Color3"),
        V::BrickColor => ("BrickColor", "BrickColor"),
        V::UDim => ("UDim", "UDim"),
        V::UDim2 => ("UDim2", "UDim2"),
        V::Ref => ("Object", "Instance"),
        V::Content => ("Content", "Content"),
        V::ContentId => ("ContentId", "ContentId"),
        V::NumberSequence => ("NumberSequence", "NumberSequence"),
        V::ColorSequence => ("ColorSequence", "ColorSequence"),
        V::NumberRange => ("NumberRange", "NumberRange"),
        V::Rect => ("Rect", "Rect"),
        V::Font => ("Font", "Font"),
        V::PhysicalProperties => ("PhysicalProperties", "PhysicalProperties"),
        V::Faces => ("Faces", "Faces"),
        V::Axes => ("Axes", "Axes"),
        V::Ray => ("Ray", "Ray"),
        _ => ("Variant", "any"),
    }
}

fn property_table(lua: &Lua, class: &Class, p: &Property) -> mlua::Result<Table> {
    let db = database();
    let owner = db.class(p.owner).name;
    let t = lua.create_table()?;
    t.set("Name", p.name)?;
    t.set("Owner", owner)?;
    let display = lua.create_table()?;
    let category = (members().classes.get(owner))
        .and_then(|m| m.property_category.get(p.name))
        .map_or("Data", String::as_str);
    display.set("Category", category)?;
    if p.flags.deprecated {
        display.set("DeprecationMessage", "")?;
    }
    t.set("Display", display)?;
    let security = (members().classes.get(owner))
        .and_then(|m| m.property_security.get(p.name))
        .copied()
        .unwrap_or_default();
    let permits = lua.create_table()?;
    let canonical = db.find_canonical_property(class.id, p.name).unwrap_or(p);
    let (read, write) = match canonical.scriptability {
        Scriptability::None | Scriptability::Custom => (false, false),
        Scriptability::Read => (true, false),
        Scriptability::Write => (false, true),
        Scriptability::ReadWrite => (true, !p.flags.read_only),
    };
    let locked = |s: Security| {
        matches!(
            s,
            Security::RobloxSecurity | Security::NotAccessibleSecurity
        )
    };
    let capability = |s: Security| match s {
        Security::None => "",
        other => other.capability(),
    };
    if read && !locked(security[0]) {
        permits.set("Read", capability(security[0]))?;
    }
    if write && !locked(security[1]) {
        permits.set("Write", capability(security[1]))?;
    }
    t.set("Permits", permits)?;
    let serialized = matches!(
        p.kind,
        PropertyKind::Canonical(Serialization::Serializes | Serialization::SerializesAs(_))
    );
    t.set("Serialized", serialized)?;
    let ty = lua.create_table()?;
    match canonical.ty {
        PropertyType::Enum(e) => {
            ty.set("EngineType", "Enum")?;
            ty.set("ScriptType", "EnumItem")?;
            ty.set("EnumType", db.enum_info(e).name)?;
        }
        PropertyType::Value(v) => {
            let (engine, script) = type_names(v);
            ty.set("EngineType", engine)?;
            ty.set("ScriptType", script)?;
        }
    }
    t.set("Type", ty)?;
    Ok(t)
}

/// `GetPropertiesOfClass(className, filter)`: the class's own properties,
/// then each superclass's, nearest first; `filter.ExcludeInherited` keeps
/// only its own. An unknown class gives `nil` (Measured).
fn properties_of_class(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let exclude_inherited = match a.get(1) {
        LuaValue::Table(f) => f.get::<bool>("ExcludeInherited").unwrap_or(false),
        _ => false,
    };
    let db = database();
    let Some(class) = db.class_by_name(&name) else {
        return Outcome::ret(lua, LuaValue::Nil);
    };
    let out = lua.create_table()?;
    let chain =
        std::iter::once(class).chain(db.superclasses(class.id).filter(|c| c.id != class.id));
    for owner in chain {
        // Measured: write-only properties (`Part.shap`) aren't listed;
        // hidden and unscriptable ones are.
        for &pid in &owner.own_properties {
            let p = db.property(pid);
            if !p.flags.write_only {
                out.raw_push(property_table(lua, class, p)?)?;
            }
        }
        if exclude_inherited {
            break;
        }
    }
    Outcome::ret(lua, out)
}

fn class_table(lua: &Lua, class: &Class) -> mlua::Result<Table> {
    let db = database();
    let t = lua.create_table()?;
    t.set("Name", class.name)?;
    if let Some(sup) = class.superclass {
        t.set("Superclass", db.class(sup).name)?;
    }
    let subclasses = lua.create_table()?;
    for c in db.classes() {
        if c.superclass == Some(class.id) {
            subclasses.raw_push(c.name)?;
        }
    }
    t.set("Subclasses", subclasses)?;
    Ok(t)
}

fn get_class(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    match database().class_by_name(&name) {
        Some(c) => Outcome::ret(lua, class_table(lua, c)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

fn get_classes(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let out = lua.create_table()?;
    for c in database().classes() {
        out.raw_push(class_table(lua, c)?)?;
    }
    Outcome::ret(lua, out)
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    if owner != "ReflectionService" {
        return None;
    }
    Some(match name {
        "GetPropertiesOfClass" => properties_of_class,
        "GetClass" => get_class,
        "GetClasses" => get_classes,
        _ => return None,
    })
}
