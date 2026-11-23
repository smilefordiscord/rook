//! Argument checking with Roblox-style messages.
//!
//! Message texts follow what Roblox prints for engine functions
//! (`Argument 1 missing or nil`, `invalid argument #1 to 'new' (number
//! expected, got string)`). They are Inference until the conformance suite
//! pins them.
//!
//! A reflected method casts its arguments from the last to the first, so
//! its native casts them in that order. Measured on MarketplaceService:
//! `PromptPurchase()` is `Argument 2 missing or nil`, and
//! `PromptPurchase(player, "abc", true, "Nope")` is `Unable to cast string
//! to CurrencyType`.

use mlua::{AnyUserData, Function, Lua, MultiValue, UserData, Value as LuaValue};
use rook_dom::InstanceId;
use rook_reflection::members::{Param, TypeKind};

use crate::api::instance::LuaInstance;
use crate::runtime::ctx::{ApiResult, ScriptError};

pub(crate) struct Args {
    vals: Vec<LuaValue>,
    /// Name used in messages, for example `new` or `FindFirstChild`.
    name: &'static str,
    /// How many of the call's arguments come before `vals[0]`, so messages
    /// number arguments as the script passed them.
    offset: usize,
}

/// The type name scripts would see from `typeof`.
pub(crate) fn type_name(v: &LuaValue) -> String {
    match v {
        LuaValue::Nil => "nil".into(),
        LuaValue::Boolean(_) => "boolean".into(),
        LuaValue::Integer(_) | LuaValue::Number(_) => "number".into(),
        LuaValue::String(_) => "string".into(),
        LuaValue::Table(_) => "table".into(),
        LuaValue::Function(_) => "function".into(),
        LuaValue::Thread(_) => "thread".into(),
        LuaValue::Vector(_) => "Vector3".into(),
        LuaValue::Buffer(_) => "buffer".into(),
        LuaValue::UserData(ud) => ud
            .metatable()
            .and_then(|mt| mt.get::<String>("__type"))
            .unwrap_or_else(|_| "userdata".into()),
        _ => "userdata".into(),
    }
}

/// What Roblox's reflection calls a value it couldn't cast, in `Unable to
/// cast X to Y`: numbers are `double`, booleans `bool`, a table with keys
/// but no first item `Dictionary` and any other `Array`, an EnumItem its
/// enum, anything else its type name. Measured: `{}` is `Unable to cast
/// Array to Vector2`, another enum's item `Unable to cast Material to
/// ThumbnailType`.
pub(crate) fn cast_name(v: &LuaValue) -> String {
    match v {
        LuaValue::Integer(_) | LuaValue::Number(_) => "double".into(),
        LuaValue::Boolean(_) => "bool".into(),
        LuaValue::Table(t) if is_dictionary(t) => "Dictionary".into(),
        LuaValue::Table(_) => "Array".into(),
        LuaValue::UserData(u) => u
            .borrow::<crate::types::enums::LEnumItem>()
            .map_or_else(|_| type_name(v), |item| item.enum_name().to_owned()),
        v => type_name(v),
    }
}

/// Whether Roblox's reflection reads `t` as a Dictionary rather than an
/// Array: it has keys but no first item.
fn is_dictionary(t: &mlua::Table) -> bool {
    t.raw_get::<LuaValue>(1).is_ok_and(|v| v.is_nil()) && !t.is_empty()
}

/// `v` didn't cast to `target`: `Unable to cast string to int64`.
pub(crate) fn cast_error(v: &LuaValue, target: &str) -> ScriptError {
    ScriptError(format!("Unable to cast {} to {target}", cast_name(v)))
}

pub(crate) fn number_of(v: &LuaValue) -> Option<f64> {
    match v {
        LuaValue::Integer(i) => Some(*i as f64),
        LuaValue::Number(n) => Some(*n),
        // Lua's string-to-number coercion, hex included.
        LuaValue::String(s) => {
            let s = s.to_str().ok()?;
            let s = s.trim();
            match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                Some(hex) => u64::from_str_radix(hex, 16).ok().map(|n| n as f64),
                None => s.parse().ok(),
            }
        }
        _ => None,
    }
}

/// A number argument of a datatype constructor, read as Roblox reads
/// them: a number, or a string that reads as one; anything else, nil or
/// missing included, is 0. Measured: `Color3.new('New Yeller')` is black
/// and `Vector3.new(true, 2, 3)` is (0, 2, 3), where methods such as
/// `CFrame.Angles` or `Color3:Lerp` error.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Num(pub f64);

impl Num {
    pub(crate) fn f32(self) -> f32 {
        self.0 as f32
    }
}

impl mlua::FromLua for Num {
    fn from_lua(value: LuaValue, _: &Lua) -> mlua::Result<Self> {
        Ok(Self(number_of(&value).unwrap_or(0.0)))
    }
}

impl Args {
    pub(crate) fn new(args: MultiValue, name: &'static str) -> Self {
        Self {
            vals: args.into_vec(),
            name,
            offset: 0,
        }
    }

    /// The arguments from `from` on, still numbered as in the whole call
    /// (`CollectionService:AddTag(nil)` is `Argument 2 missing or nil`,
    /// measured).
    pub(crate) fn shifted(&self, from: usize) -> Self {
        Self {
            vals: self.rest(from),
            name: self.name,
            offset: self.offset + from,
        }
    }

    /// For methods: take `self` as an instance.
    pub(crate) fn method(args: MultiValue, name: &'static str) -> ApiResult<(InstanceId, Self)> {
        let mut vals = args.into_vec();
        let this = match vals.first() {
            Some(LuaValue::UserData(ud)) => ud.borrow::<LuaInstance>().ok().map(|i| i.0),
            _ => None,
        };
        let Some(this) = this else {
            return Err(ScriptError(format!(
                "Expected ':' not '.' calling member function {name}"
            )));
        };
        vals.remove(0);
        Ok((
            this,
            Self {
                vals,
                name,
                offset: 0,
            },
        ))
    }

    pub(crate) fn get(&self, i: usize) -> &LuaValue {
        self.vals.get(i).unwrap_or(&LuaValue::Nil)
    }

    /// How many arguments were passed, explicit nils included.
    pub(crate) fn len(&self) -> usize {
        self.vals.len()
    }

    pub(crate) fn rest(&self, from: usize) -> Vec<LuaValue> {
        self.vals.get(from..).map(<[_]>::to_vec).unwrap_or_default()
    }

    pub(crate) fn missing(&self, i: usize) -> ScriptError {
        ScriptError(format!("Argument {} missing or nil", self.offset + i + 1))
    }

    pub(crate) fn wrong(&self, i: usize, expected: &str) -> ScriptError {
        ScriptError(format!(
            "invalid argument #{} to '{}' ({expected} expected, got {})",
            self.offset + i + 1,
            self.name,
            type_name(self.get(i))
        ))
    }

    /// Roblox's reflection's message for argument `i`, which isn't a `to`:
    /// `Argument 1 missing or nil` for nil, else `Unable to cast string to
    /// int`.
    pub(crate) fn cast_error(&self, i: usize, to: &str) -> ScriptError {
        match self.get(i) {
            LuaValue::Nil => self.missing(i),
            v => cast_error(v, to),
        }
    }

    /// A datatype argument of a reflected method, named `name` in its
    /// cast error (`Unable to cast UDim2 to Path2DControlPoint`).
    pub(crate) fn datatype<T: UserData + Clone + 'static>(
        &self,
        i: usize,
        name: &str,
    ) -> ApiResult<T> {
        match self.get(i) {
            LuaValue::UserData(ud) => borrow_clone::<T>(ud),
            _ => None,
        }
        .ok_or_else(|| self.cast_error(i, name))
    }

    /// An `int` argument of a reflected method: a number, or a string that
    /// reads as one, rounded to the nearest whole one. Measured on
    /// `Path2D:GetControlPoint`: 1.5 is 2 and 3.9 is 4, NaN and anything
    /// past an `int32` -2147483648, `"a"` `Unable to cast string to int`.
    pub(crate) fn int(&self, i: usize) -> ApiResult<i32> {
        let n = number_of(self.get(i)).ok_or_else(|| self.cast_error(i, "int"))?;
        let n = n.round();
        Ok(
            if (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&n) {
                n as i32
            } else {
                i32::MIN
            },
        )
    }

    /// A `float` argument of a reflected method: a number, or a string
    /// that reads as one. Measured on `AudioFilter:GetGainAt("x")`:
    /// `Unable to cast string to float`.
    pub(crate) fn float(&self, i: usize) -> ApiResult<f32> {
        (number_of(self.get(i)).map(|n| n as f32)).ok_or_else(|| self.cast_error(i, "float"))
    }

    /// An `Array` argument's items from 1 to its length, nils in holes
    /// included, cast as [`Args::array`] casts it. Measured on
    /// `Path2D:SetControlPoints`: a table with no first item but other keys
    /// is `Unable to cast Dictionary to Array`, a string `Unable to cast
    /// string to Array`.
    pub(crate) fn array_items(&self, i: usize) -> ApiResult<Vec<LuaValue>> {
        let t = self.array(i)?;
        (1..=t.raw_len())
            .map(|k| Ok(t.raw_get::<LuaValue>(k)?))
            .collect()
    }

    pub(crate) fn number(&self, i: usize) -> ApiResult<f64> {
        match self.get(i) {
            LuaValue::Nil => Err(self.missing(i)),
            v => number_of(v).ok_or_else(|| self.wrong(i, "number")),
        }
    }

    pub(crate) fn opt_number(&self, i: usize) -> ApiResult<Option<f64>> {
        match self.get(i) {
            LuaValue::Nil => Ok(None),
            v => number_of(v)
                .map(Some)
                .ok_or_else(|| self.wrong(i, "number")),
        }
    }

    pub(crate) fn number_or(&self, i: usize, default: f64) -> ApiResult<f64> {
        Ok(self.opt_number(i)?.unwrap_or(default))
    }

    /// An item of the enum called `enum_name`, as an EnumItem, its number
    /// or its name, giving its value. Measured on
    /// `Players:GetUserThumbnailAsync`: a number that is no item is
    /// `Unable to cast double to ThumbnailSize`, another enum's item
    /// `Unable to cast Material to ThumbnailType`.
    pub(crate) fn enum_value(&self, lua: &Lua, i: usize, enum_name: &str) -> ApiResult<u32> {
        crate::types::enums::item_value(lua, self.get(i), enum_name)
            .ok_or_else(|| self.cast_error(i, enum_name))
    }

    /// An `int64` parameter (an asset, badge or group id): a number, or a
    /// string that reads as one, truncated toward zero. Measured on
    /// MarketplaceService, BadgeService and GroupService: `Unable to cast
    /// string to int64`, `Unable to cast Array to int64`;
    /// [`Args::int64_rounded`] is for the members that round.
    pub(crate) fn int64(&self, i: usize) -> ApiResult<i64> {
        self.int64_truncated(i, "int64")
    }

    /// A `User` parameter, a user id, cast as an `int64` is (measured:
    /// `Unable to cast string to User`, `Unable to cast Instance to User`).
    pub(crate) fn user(&self, i: usize) -> ApiResult<i64> {
        self.int64_truncated(i, "User")
    }

    fn int64_truncated(&self, i: usize, target: &str) -> ApiResult<i64> {
        match self.get(i) {
            LuaValue::Nil => Err(self.missing(i)),
            v => number_of(v)
                .map(|n| n as i64)
                .ok_or_else(|| cast_error(v, target)),
        }
    }

    /// An optional `bool` parameter: nil is its default; anything but a
    /// boolean doesn't cast (measured: a table is `Unable to cast Array to
    /// bool`; Inference for numbers and strings).
    pub(crate) fn opt_bool(&self, i: usize) -> ApiResult<Option<bool>> {
        match self.get(i) {
            LuaValue::Nil => Ok(None),
            LuaValue::Boolean(b) => Ok(Some(*b)),
            v => Err(cast_error(v, "bool")),
        }
    }

    /// An `int64` parameter (`User` too, a user id, under its own name in
    /// messages): a number, or a string that reads as one, rounded to the
    /// nearest whole number as `InsertService:LoadAsset`'s id was measured
    /// to be. TeleportService casts its ids with it; [`Args::int64`]
    /// truncates instead.
    pub(crate) fn int64_rounded(&self, i: usize, target: &str) -> ApiResult<i64> {
        number_of(self.get(i))
            .map(|n| n.round() as i64)
            .ok_or_else(|| self.cast_error(i, target))
    }

    /// A `Variant` parameter, which takes any value but nil (measured:
    /// `TeleportOptions:SetTeleportData(nil)` is `Argument 1 missing or
    /// nil`).
    pub(crate) fn variant(&self, i: usize) -> ApiResult<&LuaValue> {
        match self.get(i) {
            LuaValue::Nil => Err(self.missing(i)),
            v => Ok(v),
        }
    }

    /// An `Objects` parameter: an array of instances, empty included.
    /// Measured on `TeleportService:TeleportAsync`: missing or nil is
    /// `Argument 2 missing or nil`; anything but an array (a Player not in a
    /// table, a dictionary) `Unable to cast value to Objects`; an item that
    /// isn't an Instance `Unable to cast value to Object`.
    pub(crate) fn objects(&self, i: usize) -> ApiResult<Vec<InstanceId>> {
        let LuaValue::Table(t) = self.get(i) else {
            return Err(match self.get(i) {
                LuaValue::Nil => self.missing(i),
                _ => ScriptError("Unable to cast value to Objects".into()),
            });
        };
        let length = t.raw_len();
        let not_array = || ScriptError("Unable to cast value to Objects".into());
        let mut entries = 0;
        for pair in t.clone().pairs::<LuaValue, LuaValue>() {
            let (key, _) = pair.map_err(|_| not_array())?;
            let index = match key {
                LuaValue::Integer(n) => n as f64,
                LuaValue::Number(n) => n,
                _ => return Err(not_array()),
            };
            if index.fract() != 0.0 || index < 1.0 || index > length as f64 {
                return Err(not_array());
            }
            entries += 1;
        }
        if entries != length {
            return Err(not_array());
        }
        (1..=length)
            .map(|n| match t.raw_get::<LuaValue>(n) {
                Ok(LuaValue::UserData(ud)) => ud
                    .borrow::<LuaInstance>()
                    .map(|inst| inst.0)
                    .map_err(|_| cast_to_object()),
                _ => Err(cast_to_object()),
            })
            .collect()
    }

    /// A `Dictionary` parameter: a table without a first item, an empty
    /// one included. Measured on `HttpService:RequestAsync`: nil is
    /// `Argument 1 missing or nil`, a string `Unable to cast string to
    /// Dictionary`, a list `Unable to cast Array to Dictionary`.
    pub(crate) fn dictionary(&self, i: usize) -> ApiResult<mlua::Table> {
        match self.get(i) {
            LuaValue::Nil => Err(self.missing(i)),
            LuaValue::Table(t) if t.raw_get::<LuaValue>(1)?.is_nil() => Ok(t.clone()),
            LuaValue::Table(_) => Err(ScriptError("Unable to cast Array to Dictionary".into())),
            v => Err(ScriptError(format!(
                "Unable to cast {} to Dictionary",
                cast_name(v)
            ))),
        }
    }

    pub(crate) fn string(&self, i: usize) -> ApiResult<String> {
        self.opt_string(i)?.ok_or_else(|| self.missing(i))
    }

    pub(crate) fn opt_string(&self, i: usize) -> ApiResult<Option<String>> {
        match self.get(i) {
            LuaValue::Nil => Ok(None),
            LuaValue::String(s) => Ok(Some(s.to_string_lossy())),
            LuaValue::Integer(n) => Ok(Some(n.to_string())),
            LuaValue::Number(n) => Ok(Some(format_number(*n))),
            _ => Err(self.wrong(i, "string")),
        }
    }

    /// Lua truthiness, as Roblox uses for optional boolean arguments.
    pub(crate) fn truthy(&self, i: usize) -> bool {
        !matches!(self.get(i), LuaValue::Nil | LuaValue::Boolean(false))
    }

    /// A `bool` parameter as Roblox casts it: `default` when nil or left
    /// out, a boolean as itself, anything else `Unable to cast string to
    /// bool` (measured on `HttpService:GenerateGUID("x")`).
    pub(crate) fn bool_or(&self, i: usize, default: bool) -> ApiResult<bool> {
        match self.get(i) {
            LuaValue::Nil => Ok(default),
            LuaValue::Boolean(b) => Ok(*b),
            v => Err(ScriptError(format!(
                "Unable to cast {} to bool",
                cast_name(v)
            ))),
        }
    }

    /// An `Object` parameter as Roblox casts it: a missing argument errors,
    /// an explicit `nil` is the null object (`None`), anything but an
    /// Instance doesn't cast. Measured: `Debris:AddItem(nil)` and
    /// `workspace:IsDescendantOf(nil)` run, `Debris:AddItem()` is
    /// `Argument 1 missing or nil`. What null means is each method's own.
    pub(crate) fn object(&self, i: usize) -> ApiResult<Option<InstanceId>> {
        if i >= self.len() {
            return Err(self.missing(i));
        }
        self.opt_instance(i)
    }

    /// An `Object` parameter the method then checks is a `class`: null and
    /// other classes are both `None`, as Roblox gives them the same
    /// message (`Parameter 1 must be BasePart in SetPartCollisionGroup.`).
    pub(crate) fn object_of(
        &self,
        lua: &Lua,
        i: usize,
        class: &str,
    ) -> ApiResult<Option<InstanceId>> {
        Ok(self
            .object(i)?
            .filter(|&id| crate::runtime::ctx::with_world(lua, |w| w.dom.is_a(id, class))))
    }

    /// An instance the method requires, for methods whose own check turns
    /// null away as missing (`Humanoid:ApplyDescription(nil)` is
    /// `Argument 1 missing or nil`, measured).
    pub(crate) fn instance(&self, i: usize) -> ApiResult<InstanceId> {
        self.opt_instance(i)?.ok_or_else(|| self.missing(i))
    }

    /// An optional instance: missing and `nil` are both `None`.
    pub(crate) fn opt_instance(&self, i: usize) -> ApiResult<Option<InstanceId>> {
        match self.get(i) {
            LuaValue::Nil => Ok(None),
            LuaValue::UserData(ud) => match ud.borrow::<LuaInstance>() {
                Ok(inst) => Ok(Some(inst.0)),
                Err(_) => Err(cast_to_object()),
            },
            _ => Err(cast_to_object()),
        }
    }

    /// An `Array` parameter. Measured on `CheckUserBadgesAsync`: `Unable to
    /// cast string to Array`, `Unable to cast Dictionary to Array` for a
    /// table with keys but no first item; an empty table is an Array.
    pub(crate) fn array(&self, i: usize) -> ApiResult<mlua::Table> {
        match self.get(i) {
            LuaValue::Nil => Err(self.missing(i)),
            v @ LuaValue::Table(t) => match cast_name(v).as_str() {
                "Array" => Ok(t.clone()),
                _ => Err(cast_error(v, "Array")),
            },
            v => Err(cast_error(v, "Array")),
        }
    }

    /// Cast the arguments as the API dump's `params` take them, the last
    /// first, raising Roblox's error for one that's missing or doesn't
    /// cast: for a member Rook has no native for, these come before
    /// anything the member does. Only the casts measured are checked
    /// (`int64`, `User`, `bool`, `Array`, enums, instances, and a missing
    /// `string`); other types are taken as they come.
    pub(crate) fn cast(&self, lua: &Lua, params: &[Param]) -> ApiResult<()> {
        for (i, p) in params.iter().enumerate().rev() {
            let optional = p.ty.nullable || p.default.is_some();
            match (p.ty.kind, p.ty.name.as_str()) {
                (TypeKind::Class, _) if optional => {
                    self.opt_instance(i)?;
                }
                (TypeKind::Class, _) => {
                    self.object(i)?;
                }
                _ if optional && self.get(i).is_nil() => {}
                (TypeKind::Primitive, "int64") => {
                    self.int64(i)?;
                }
                (TypeKind::DataType, "User") => {
                    self.user(i)?;
                }
                (TypeKind::Primitive, "bool") => {
                    self.opt_bool(i)?;
                }
                (TypeKind::Group, "Array") => {
                    self.array(i)?;
                }
                (TypeKind::Enum, name) => {
                    self.enum_value(lua, i, name)?;
                }
                (TypeKind::Primitive, "string") if self.get(i).is_nil() => {
                    return Err(self.missing(i));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// An array of instances (`Instances` in the API dump).
    pub(crate) fn instances(&self, i: usize) -> ApiResult<Vec<InstanceId>> {
        let LuaValue::Table(t) = self.get(i) else {
            return Err(self.wrong(i, "table"));
        };
        t.sequence_values::<LuaValue>()
            .map(|v| match v? {
                LuaValue::UserData(ud) => ud
                    .borrow::<LuaInstance>()
                    .map(|inst| inst.0)
                    .map_err(|_| cast_to_object()),
                _ => Err(cast_to_object()),
            })
            .collect()
    }

    pub(crate) fn function(&self, i: usize) -> ApiResult<Function> {
        match self.get(i) {
            LuaValue::Function(f) => Ok(f.clone()),
            LuaValue::Nil => Err(self.missing(i)),
            _ => Err(self.wrong(i, "function")),
        }
    }

    pub(crate) fn userdata<T: UserData + Clone + 'static>(
        &self,
        i: usize,
        expected: &str,
    ) -> ApiResult<T> {
        self.opt_userdata(i, expected)?
            .ok_or_else(|| self.missing(i))
    }

    pub(crate) fn opt_userdata<T: UserData + Clone + 'static>(
        &self,
        i: usize,
        expected: &str,
    ) -> ApiResult<Option<T>> {
        match self.get(i) {
            LuaValue::Nil => Ok(None),
            LuaValue::UserData(ud) => borrow_clone::<T>(ud)
                .map(Some)
                .ok_or_else(|| self.wrong(i, expected)),
            _ => Err(self.wrong(i, expected)),
        }
    }

    pub(crate) fn vector3(&self, i: usize) -> ApiResult<rook_types::Vector3> {
        match self.get(i) {
            LuaValue::Vector(v) => Ok(crate::runtime::ctx::convert::from_lua_vector(*v)),
            LuaValue::Nil => Err(self.missing(i)),
            _ => Err(self.wrong(i, "Vector3")),
        }
    }
}

fn cast_to_object() -> ScriptError {
    ScriptError("Unable to cast value to Object".into())
}

pub(crate) fn borrow_clone<T: UserData + Clone + 'static>(ud: &AnyUserData) -> Option<T> {
    ud.borrow::<T>().ok().map(|v| v.clone())
}

/// Format a number the way Luau's `tostring` does.
pub(crate) fn format_number(n: f64) -> String {
    if n.is_nan() {
        return "nan".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if n == n.trunc() && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    // Luau prints the shortest representation that round-trips.
    let s = format!("{n}");
    if s.len() > 20 { format!("{n:e}") } else { s }
}

/// Convert anything to a string the way Luau's `tostring` would, for
/// values that don't need metamethods.
pub(crate) fn display_value(lua: &Lua, v: &LuaValue) -> String {
    let tostring: mlua::Result<Function> = lua.globals().get("tostring");
    match tostring.and_then(|f| f.call::<mlua::LuaString>(v.clone())) {
        Ok(s) => s.to_string_lossy(),
        Err(_) => type_name(v),
    }
}
