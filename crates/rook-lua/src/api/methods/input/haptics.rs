//! HapticService: the motors of each `GamepadN` slot's pad
//! (docs/behavior/input/gamepads.md#motors). Scripts set what a motor
//! should run at; the engine owns what it does run at
//! (`DataModel::motor_speeds`), and the window sends that to the pad.

use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::VibrationMotor;

use super::gamepads::pad_arg;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, with_world};

fn motor_arg(lua: &Lua, a: &Args, i: usize) -> ApiResult<VibrationMotor> {
    let value = a.enum_value(lua, i, "VibrationMotor")?;
    VibrationMotor::from_u32(value).ok_or_else(|| a.wrong(i, "VibrationMotor"))
}

/// A connected pad with a motor (Inference: false with no pad in the
/// slot, and for every input type that isn't a pad).
pub(super) fn vibration_supported(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let pad = pad_arg(lua, a, 0)?;
    let yes = pad.is_some_and(|p| {
        with_world(lua, |w| {
            let slot = &w.input.gamepads[p];
            slot.connected && !slot.motors.is_empty()
        })
    });
    Outcome::ret(lua, yes)
}

pub(super) fn motor_supported(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (pad, motor) = (pad_arg(lua, a, 0)?, motor_arg(lua, a, 1)?);
    let yes = pad.is_some_and(|p| {
        with_world(lua, |w| {
            let slot = &w.input.gamepads[p];
            slot.connected && slot.motors.contains(&motor)
        })
    });
    Outcome::ret(lua, yes)
}

/// Only the tuple's first value counts, a number (creator docs). It's
/// kept as given, for `GetMotor`, whether or not a pad is there to run
/// it (Inference); other input types keep nothing.
pub(super) fn set_motor(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (pad, motor) = (pad_arg(lua, a, 0)?, motor_arg(lua, a, 1)?);
    let value = a.number(2)?;
    if let Some(pad) = pad {
        with_world(lua, |w| {
            w.input.gamepads[pad].motor_values[motor.index()] = Some(value);
        });
    }
    Outcome::ret(lua, ())
}

/// Nothing until `SetMotor` set the motor (creator docs).
pub(super) fn get_motor(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (pad, motor) = (pad_arg(lua, a, 0)?, motor_arg(lua, a, 1)?);
    let value =
        pad.and_then(|p| with_world(lua, |w| w.input.gamepads[p].motor_values[motor.index()]));
    match value {
        Some(v) => Outcome::ret(lua, v),
        None => Outcome::ret(lua, ()),
    }
}
