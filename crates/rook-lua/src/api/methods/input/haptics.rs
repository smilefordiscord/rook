//! HapticService: the motors of each `GamepadN` slot's pad
//! (docs/behavior/input/gamepads.md#motors). Scripts set what a motor
//! should run at; the engine owns what it does run at
//! (`DataModel::motor_speeds`), and the window sends that to the pad.
//!
//! Arguments are cast as Studio's reflected methods cast them
//! (`Args::enum_value`): `Unable to cast KeyCode to UserInputType`,
//! `Argument 1 missing or nil` (Confirmed, conformance case
//! `input/haptics`).

use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::VibrationMotor;

use super::gamepads::pad_of;
use crate::net::{NetValue, from_net, to_net};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, with_world};

fn input_type_arg(lua: &Lua, a: &Args, i: usize) -> ApiResult<u32> {
    a.enum_value(lua, i, "UserInputType")
}

fn motor_arg(lua: &Lua, a: &Args, i: usize) -> ApiResult<VibrationMotor> {
    let value = a.enum_value(lua, i, "VibrationMotor")?;
    VibrationMotor::from_u32(value).ok_or_else(|| a.wrong(i, "VibrationMotor"))
}

/// A connected pad with a motor (Confirmed false with no pad, for every
/// input type).
pub(super) fn vibration_supported(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let pad = pad_of(input_type_arg(lua, a, 0)?);
    let yes = pad.is_some_and(|p| {
        with_world(lua, |w| {
            let slot = &w.input.gamepads[p];
            slot.connected && !slot.motors.is_empty()
        })
    });
    Outcome::ret(lua, yes)
}

pub(super) fn motor_supported(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let pad = pad_of(input_type_arg(lua, a, 0)?);
    let motor = motor_arg(lua, a, 1)?;
    let yes = pad.is_some_and(|p| {
        with_world(lua, |w| {
            let slot = &w.input.gamepads[p];
            slot.connected && slot.motors.contains(&motor)
        })
    });
    Outcome::ret(lua, yes)
}

/// The tuple is kept for `GetMotor` whatever it holds, for any input
/// type, pad or not (Confirmed): numbers as single-precision floats,
/// booleans as 1 and 0 (Confirmed for true), anything else as given (a
/// table copied, as a remote's argument is), and nothing as one nil.
pub(super) fn set_motor(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let input_type = input_type_arg(lua, a, 0)?;
    let motor = motor_arg(lua, a, 1)?;
    let mut values: Vec<NetValue> = (a.rest(2).iter())
        .map(|v| match to_net(v, 0) {
            NetValue::Bool(b) => NetValue::Number(f64::from(u8::from(b))),
            NetValue::Number(n) => NetValue::Number(f64::from(n as f32)),
            other => other,
        })
        .collect();
    if values.is_empty() {
        values.push(NetValue::Nil);
    }
    with_world(lua, |w| {
        w.input.motor_values.insert((input_type, motor), values);
    });
    Outcome::ret(lua, ())
}

/// Nothing until `SetMotor` set the motor (Confirmed), then its tuple.
pub(super) fn get_motor(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let input_type = input_type_arg(lua, a, 0)?;
    let motor = motor_arg(lua, a, 1)?;
    let values = with_world(lua, |w| {
        w.input.motor_values.get(&(input_type, motor)).cloned()
    })
    .unwrap_or_default();
    let values = (values.iter())
        .map(|v| from_net(lua, v))
        .collect::<mlua::Result<Vec<_>>>()?;
    Outcome::ret(lua, mlua::MultiValue::from_vec(values))
}
