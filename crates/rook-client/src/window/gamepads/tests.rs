use super::*;

fn events(sent: &mut PadState, now: PadState, focused: bool) -> Vec<InputEvent> {
    let mut out = Vec::new();
    diff(0, sent, now, focused, &mut |e| out.push(e));
    out
}

/// A new state sends only what changed, in order; unfocused, only a
/// button let go comes, and the rest waits for focus.
#[test]
fn states_become_events_for_what_changed() {
    let mut sent = PadState::default();
    let mut now = PadState::default();
    now.set(GamepadKey::ButtonA, true);
    now.sticks[0].1 = 0.5;
    let e = events(&mut sent, now, true);
    assert_eq!(e.len(), 2);
    assert!(matches!(
        e[0],
        InputEvent::GamepadButton {
            key: GamepadKey::ButtonA,
            down: true,
            ..
        }
    ));
    assert!(matches!(
        e[1],
        InputEvent::GamepadAxis {
            axis: GamepadAxis::StickY(0),
            ..
        }
    ));
    assert!(events(&mut sent, now, true).is_empty());

    let mut later = now;
    later.set(GamepadKey::ButtonA, false);
    later.set(GamepadKey::ButtonB, true);
    later.sticks[0].1 = 0.0;
    let e = events(&mut sent, later, false);
    assert_eq!(e.len(), 1);
    assert!(matches!(
        e[0],
        InputEvent::GamepadButton {
            key: GamepadKey::ButtonA,
            down: false,
            ..
        }
    ));
    assert_eq!(events(&mut sent, later, true).len(), 2);
}

#[test]
fn studio_lists_each_key_a_pad_has() {
    for &(_, key) in GamepadKey::ITEMS {
        let listed = STUDIO_KEYS.iter().filter(|&&k| k == key).count();
        let expected = match key {
            k if k.is_thumbstick() => 2,
            GamepadKey::ButtonCenter
            | GamepadKey::ButtonBack
            | GamepadKey::ButtonUp
            | GamepadKey::ButtonDown
            | GamepadKey::ButtonLeft
            | GamepadKey::ButtonRight => 0,
            _ => 1,
        };
        assert_eq!(listed, expected, "{key:?}");
    }
    assert_eq!(kind(SONY, 0x0ce6), GamepadKind::PlayStation5);
}

/// A pad is sent its speeds only when they change, and slots without a
/// pad are skipped.
#[test]
fn rumble_is_sent_only_when_it_changes() {
    let mut slots: [Option<Slot>; 8] = Default::default();
    slots[1] = Some(Slot {
        id: PadId::Sony(7),
        sent: PadState::default(),
        rumble: [0.0; 2],
    });
    let mut speeds: MotorSpeeds = Default::default();
    let mut sent = Vec::new();
    rumble_changes(&mut slots, &speeds, |id, r| sent.push((id, r)));
    assert!(sent.is_empty());
    speeds[1][VibrationMotor::Small.index()] = 0.5;
    speeds[1][VibrationMotor::LeftTrigger.index()] = 1.0;
    speeds[0][VibrationMotor::Large.index()] = 1.0;
    rumble_changes(&mut slots, &speeds, |id, r| sent.push((id, r)));
    rumble_changes(&mut slots, &speeds, |id, r| sent.push((id, r)));
    assert_eq!(sent, [(PadId::Sony(7), [0.0, 0.5])]);
}
