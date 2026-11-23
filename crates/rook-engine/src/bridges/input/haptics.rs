//! What each pad's motors run at, from what HapticService set
//! (docs/behavior/input/gamepads.md#motors). The window sends it to the
//! pads.

use rook_lua::gamepads::GAMEPAD_SLOTS;
use rook_types::VibrationMotor;

use crate::DataModel;

/// Each slot's motors, by [`VibrationMotor::index`], 0 (still) to 1.
pub type MotorSpeeds = [[f32; VibrationMotor::COUNT]; GAMEPAD_SLOTS];

impl DataModel {
    /// The one place a motor's speed is decided: what `SetMotor` set,
    /// clamped to 0..1, times the player's `HapticStrength` (creator docs:
    /// output is scaled by the player's haptic setting; Inference that
    /// it's a plain product), and still for a motor the slot's pad lacks
    /// or a slot without a pad.
    pub fn motor_speeds(&self) -> MotorSpeeds {
        let w = self.runtime.world();
        let strength = (w.user_settings)
            .and_then(|root| rook_lua::user_settings::game_settings(&w.dom, root))
            .and_then(|game| w.dom.float_property(game, "HapticStrength"))
            .filter(|s| !s.is_nan())
            .map_or(1.0, |s| s.clamp(0.0, 1.0));
        let mut speeds = [[0.0; VibrationMotor::COUNT]; GAMEPAD_SLOTS];
        for (slot, out) in w.input.gamepads.iter().zip(&mut speeds) {
            if !slot.connected {
                continue;
            }
            for &motor in &slot.motors {
                if let Some(value) = slot.motor_values[motor.index()] {
                    // `clamp` passes NaN through; a NaN speed runs nothing.
                    let value = if value.is_nan() { 0.0 } else { value };
                    out[motor.index()] = value.clamp(0.0, 1.0) as f32 * strength;
                }
            }
        }
        speeds
    }
}
