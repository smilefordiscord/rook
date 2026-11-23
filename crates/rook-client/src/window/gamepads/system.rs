//! Pads the platform's own API reads, through gilrs. Its filters are off:
//! Studio passes a stick's raw values, jitter included, with no deadzone.
//! A pad's motors run through one looping force-feedback effect, made
//! again at each change of speed: gilrs mutes an effect's gain below 0.05,
//! so the speeds go in its magnitudes.

use gilrs::ff::{BaseEffect, BaseEffectType, Effect, EffectBuilder, Repeat};
use gilrs::{Axis, Button, EventType, Gamepad, GamepadId, Gilrs, GilrsBuilder};
use rook_types::GamepadKey;
use rustc_hash::FxHashMap;

use super::{PadChange, PadId, PadState, Rumble, STUDIO_KEYS, kind};

pub(super) struct System {
    /// None where the platform has no gamepad API.
    gilrs: Option<Gilrs>,
    states: FxHashMap<GamepadId, PadState>,
    /// Pads gilrs had when it started: it announces only later ones.
    found: Vec<GamepadId>,
    /// Each rumbling pad's effect; dropping it stops the motors.
    rumbling: FxHashMap<GamepadId, Effect>,
}

impl std::fmt::Debug for System {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("System")
            .field("pads", &self.states.len())
            .finish()
    }
}

impl System {
    pub(super) fn new() -> Self {
        let gilrs = GilrsBuilder::new()
            .with_default_filters(false)
            .build()
            .inspect_err(|e| tracing::warn!("gamepads won't be read: {e}"))
            .ok();
        let found = gilrs
            .as_ref()
            .map(|g| g.gamepads().map(|(id, _)| id).collect())
            .unwrap_or_default();
        Self {
            gilrs,
            states: FxHashMap::default(),
            found,
            rumbling: FxHashMap::default(),
        }
    }

    /// Run `id`'s strong (Large) and weak (Small) motors at these speeds.
    /// The new effect is made before the old one goes, so gilrs's
    /// thread doesn't still the pad between them.
    pub(super) fn rumble(&mut self, id: GamepadId, [large, small]: Rumble) {
        let Some(gilrs) = &mut self.gilrs else {
            return;
        };
        if large <= 0.0 && small <= 0.0 {
            self.rumbling.remove(&id);
            return;
        }
        let magnitude = |speed: f32| (speed.clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as u16;
        let motor = |kind| BaseEffect {
            kind,
            ..Default::default()
        };
        // A tick long, repeated: on until dropped.
        let effect = EffectBuilder::new()
            .add_effect(motor(BaseEffectType::Strong {
                magnitude: magnitude(large),
            }))
            .add_effect(motor(BaseEffectType::Weak {
                magnitude: magnitude(small),
            }))
            .gamepads(&[id])
            .repeat(Repeat::Infinitely)
            .finish(gilrs)
            .and_then(|effect| effect.play().map(|()| effect));
        match effect {
            Ok(effect) => {
                self.rumbling.insert(id, effect);
            }
            Err(e) => {
                self.rumbling.remove(&id);
                tracing::debug!("a gamepad's motors won't run: {e}");
            }
        }
    }

    pub(super) fn poll(&mut self, out: &mut Vec<PadChange>) {
        let Some(gilrs) = &mut self.gilrs else {
            return;
        };
        for id in std::mem::take(&mut self.found) {
            connect(gilrs, &mut self.states, id, out);
        }
        while let Some(event) = gilrs.next_event() {
            let id = event.id;
            if event.event == EventType::Connected {
                connect(gilrs, &mut self.states, id, out);
                continue;
            }
            let Some(state) = self.states.get_mut(&id) else {
                continue;
            };
            let changed = match event.event {
                EventType::Disconnected => {
                    self.states.remove(&id);
                    self.rumbling.remove(&id);
                    out.push(PadChange::Disconnected(PadId::System(id)));
                    continue;
                }
                EventType::ButtonPressed(b, _) => press(state, b, true),
                EventType::ButtonReleased(b, _) => press(state, b, false),
                EventType::ButtonChanged(b, value, _) => match b {
                    Button::LeftTrigger2 => set(&mut state.triggers[0], value),
                    Button::RightTrigger2 => set(&mut state.triggers[1], value),
                    _ => false,
                },
                EventType::AxisChanged(a, value, _) => match a {
                    Axis::LeftStickX => set(&mut state.sticks[0].0, value),
                    Axis::LeftStickY => set(&mut state.sticks[0].1, value),
                    Axis::RightStickX => set(&mut state.sticks[1].0, value),
                    Axis::RightStickY => set(&mut state.sticks[1].1, value),
                    Axis::DPadX => dpad(state, value, GamepadKey::DPadLeft, GamepadKey::DPadRight),
                    Axis::DPadY => dpad(state, value, GamepadKey::DPadDown, GamepadKey::DPadUp),
                    _ => false,
                },
                _ => false,
            };
            if changed {
                out.push(PadChange::State(PadId::System(id), *state));
            }
        }
    }
}

fn connect(
    gilrs: &Gilrs,
    states: &mut FxHashMap<GamepadId, PadState>,
    id: GamepadId,
    out: &mut Vec<PadChange>,
) {
    let Some(g) = gilrs.connected_gamepad(id) else {
        return;
    };
    let (vendor, product) = (g.vendor_id().unwrap_or(0), g.product_id().unwrap_or(0));
    // Sony's pads are `sony`'s on Windows, so none is heard twice.
    if cfg!(windows) && vendor == super::SONY {
        return;
    }
    if states.insert(id, PadState::default()).is_some() {
        return;
    }
    let supported = STUDIO_KEYS
        .iter()
        .copied()
        .filter(|&k| has(&g, k))
        .collect();
    out.push(PadChange::Connected {
        id: PadId::System(id),
        kind: kind(vendor, product),
        supported,
        rumbles: g.is_ff_supported(),
    });
}

fn has(g: &Gamepad<'_>, key: GamepadKey) -> bool {
    match key {
        GamepadKey::Thumbstick1 => g.axis_code(Axis::LeftStickX).is_some(),
        GamepadKey::Thumbstick2 => g.axis_code(Axis::RightStickX).is_some(),
        _ => {
            button_of(key).is_some_and(|b| g.button_code(b).is_some())
                || dpad_axis(key).is_some_and(|a| g.axis_code(a).is_some())
        }
    }
}

/// The gilrs button for a Roblox key. Triggers come as values, and the
/// D-pad may come as two axes.
fn button_of(key: GamepadKey) -> Option<Button> {
    Some(match key {
        GamepadKey::ButtonA => Button::South,
        GamepadKey::ButtonB => Button::East,
        GamepadKey::ButtonX => Button::West,
        GamepadKey::ButtonY => Button::North,
        GamepadKey::ButtonL1 => Button::LeftTrigger,
        GamepadKey::ButtonR1 => Button::RightTrigger,
        GamepadKey::ButtonL2 => Button::LeftTrigger2,
        GamepadKey::ButtonR2 => Button::RightTrigger2,
        GamepadKey::ButtonL3 => Button::LeftThumb,
        GamepadKey::ButtonR3 => Button::RightThumb,
        GamepadKey::ButtonStart => Button::Start,
        GamepadKey::ButtonSelect => Button::Select,
        GamepadKey::DPadUp => Button::DPadUp,
        GamepadKey::DPadDown => Button::DPadDown,
        GamepadKey::DPadLeft => Button::DPadLeft,
        GamepadKey::DPadRight => Button::DPadRight,
        _ => return None,
    })
}

fn dpad_axis(key: GamepadKey) -> Option<Axis> {
    match key {
        GamepadKey::DPadLeft | GamepadKey::DPadRight => Some(Axis::DPadX),
        GamepadKey::DPadUp | GamepadKey::DPadDown => Some(Axis::DPadY),
        _ => None,
    }
}

/// A button down or up; the pad's own center button (Mode) is no key
/// (measured: it fires nothing).
fn press(state: &mut PadState, b: Button, down: bool) -> bool {
    let key = GamepadKey::ITEMS
        .iter()
        .map(|&(_, k)| k)
        .find(|&k| !k.is_trigger() && button_of(k) == Some(b));
    match key {
        Some(key) if state.is_down(key) != down => {
            state.set(key, down);
            true
        }
        _ => false,
    }
}

fn set(slot: &mut f32, value: f32) -> bool {
    let changed = *slot != value;
    *slot = value;
    changed
}

fn dpad(state: &mut PadState, value: f32, less: GamepadKey, more: GamepadKey) -> bool {
    let before = state.buttons;
    state.set(less, value < -0.5);
    state.set(more, value > 0.5);
    state.buttons != before
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key Studio lists, triggers aside, maps to a gilrs button
    /// and back; the center button maps to none.
    #[test]
    fn keys_map_both_ways() {
        for key in STUDIO_KEYS {
            if key.is_thumbstick() || key.is_trigger() {
                continue;
            }
            let b = button_of(key).unwrap();
            let mut state = PadState::default();
            assert!(press(&mut state, b, true), "{key:?}");
            assert!(state.is_down(key));
        }
        assert!(!press(&mut PadState::default(), Button::Mode, true));
    }
}
