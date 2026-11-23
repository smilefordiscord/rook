//! What each pad's motors run at, from what HapticService set
//! (docs/behavior/input/gamepads.md#motors). The window sends it to the
//! pads.

use rook_lua::NetValue;
use rook_lua::gamepads::GAMEPAD_SLOTS;
use rook_types::{UserInputType, VibrationMotor};

use crate::DataModel;

/// Each slot's motors, by [`VibrationMotor::index`], 0 (still) to 1.
pub type MotorSpeeds = [[f32; VibrationMotor::COUNT]; GAMEPAD_SLOTS];

impl DataModel {
    /// The one place a motor's speed is decided: the first value
    /// `SetMotor` set, clamped to 0..1, times the player's
    /// `HapticStrength` (creator docs: output is scaled by the player's
    /// haptic setting; Inference that it's a plain product), and still for
    /// a value that isn't a number (Inference: Studio keeps any value, and
    /// only a number can say how hard), a motor the slot's pad lacks or a
    /// slot without a pad.
    pub fn motor_speeds(&self) -> MotorSpeeds {
        let w = self.runtime.world();
        let strength = (w.user_settings)
            .and_then(|root| rook_lua::user_settings::game_settings(&w.dom, root))
            .and_then(|game| w.dom.float_property(game, "HapticStrength"))
            .filter(|s| !s.is_nan())
            .map_or(1.0, |s| s.clamp(0.0, 1.0));
        let mut speeds = [[0.0; VibrationMotor::COUNT]; GAMEPAD_SLOTS];
        let slots = w.input.gamepads.iter().zip(UserInputType::GAMEPADS);
        for ((slot, input_type), out) in slots.zip(&mut speeds) {
            if !slot.connected {
                continue;
            }
            for &motor in &slot.motors {
                let set = w.input.motor_values.get(&(input_type.value(), motor));
                if let Some(NetValue::Number(value)) = set.and_then(|v| v.first()) {
                    // `clamp` passes NaN through; a NaN speed runs nothing.
                    let value = if value.is_nan() { 0.0 } else { *value };
                    out[motor.index()] = value.clamp(0.0, 1.0) as f32 * strength;
                }
            }
        }
        speeds
    }
}
