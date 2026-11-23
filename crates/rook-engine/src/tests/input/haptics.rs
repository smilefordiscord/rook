use crate::tests::*;
use crate::{GamepadKind, MotorSpeeds};
use rook_types::{GamepadKey, VibrationMotor};

const LARGE: usize = VibrationMotor::Large as usize;

/// A client whose LocalScript runs `source` with a pad that has a Large
/// and a Small motor in Gamepad1 from its first line.
fn client_with_pad(source: &str) -> (Session, usize) {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(&server, screen, "LocalScript", "Log", source);
    server.start();
    let mut session = Session::new(server, Options::default());
    let present = [InputEvent::GamepadConnected {
        pad: 0,
        kind: GamepadKind::Xbox,
        supported: GamepadKey::ITEMS.iter().map(|&(_, k)| k).collect(),
        motors: vec![VibrationMotor::Large, VibrationMotor::Small],
    }];
    let index = session.join_with("Player", 1, &present).unwrap();
    session.run_for(0.1);
    (session, index)
}

/// Support follows the pad in the slot; `GetMotor` returns nothing until
/// `SetMotor`, then the values as set, for any input type; the speed a
/// motor runs at is the first value clamped to 0..1, and nothing for a
/// value that isn't a number, a motor the pad lacks or a slot without a
/// pad.
#[test]
fn set_motors_run_the_pads_motors() {
    let (session, index) = client_with_pad(
        r##"
        local haptics = game:GetService("HapticService")
        local pad, empty = Enum.UserInputType.Gamepad1, Enum.UserInputType.Gamepad2
        local motor = Enum.VibrationMotor
        print(haptics:IsVibrationSupported(pad), haptics:IsVibrationSupported(empty),
            haptics:IsVibrationSupported(Enum.UserInputType.Keyboard))
        print(haptics:IsMotorSupported(pad, motor.Large), haptics:IsMotorSupported(pad, motor.Small),
            haptics:IsMotorSupported(pad, motor.LeftTrigger), haptics:IsMotorSupported(empty, motor.Large))
        print(select("#", haptics:GetMotor(pad, motor.Large)))
        haptics:SetMotor(pad, motor.Large, 0.75)
        haptics:SetMotor(pad, motor.Small, 2, 0)
        haptics:SetMotor(pad, motor.LeftTrigger, 1)
        haptics:SetMotor(empty, motor.Large, 1)
        haptics:SetMotor(Enum.UserInputType.Keyboard, motor.Large, 1)
        print(haptics:GetMotor(pad, motor.Large), haptics:GetMotor(pad, motor.Small),
            haptics:GetMotor(empty, motor.Large),
            select("#", haptics:GetMotor(Enum.UserInputType.Keyboard, motor.Large)))
        print(pcall(haptics.SetMotor, haptics, pad, motor.LeftTrigger))
        print(pcall(haptics.IsMotorSupported, haptics, pad, Enum.Material.Plastic))
        haptics:SetMotor(pad, motor.Small, "1")
        "##,
    );
    let client = session.client(index).unwrap();
    assert_eq!(
        lines(client),
        [
            "true false false",
            "true true false false",
            "0",
            "0.75 2 1 1",
            "true",
            "false Unable to cast Material to VibrationMotor",
        ]
    );
    let mut expected = MotorSpeeds::default();
    expected[0][LARGE] = 0.75;
    assert_eq!(client.motor_speeds(), expected);
}

/// The player's haptic strength scales every motor (creator docs); a pad
/// leaving stills its slot's motors, and one connecting to it runs them
/// again at what was set.
#[test]
fn strength_and_pads_coming_and_going_change_the_speeds() {
    let (mut session, index) = client_with_pad(
        r#"
        UserSettings()
        game:GetService("HapticService"):SetMotor(Enum.UserInputType.Gamepad1,
            Enum.VibrationMotor.Large, 0.5)
        "#,
    );
    let client = session.client_mut(index).unwrap();
    let game = {
        let w = client.runtime().world();
        rook_lua::user_settings::game_settings(&w.dom, w.user_settings.unwrap()).unwrap()
    };
    client.set_property(game, "HapticStrength", Value::Float32(0.5));
    assert_eq!(client.motor_speeds()[0][LARGE], 0.25);

    session.input(index, InputEvent::GamepadDisconnected { pad: 0 });
    let client = session.client(index).unwrap();
    assert_eq!(client.motor_speeds(), MotorSpeeds::default());
    session.input(
        index,
        InputEvent::GamepadConnected {
            pad: 0,
            kind: GamepadKind::PlayStation5,
            supported: Vec::new(),
            motors: vec![VibrationMotor::Large],
        },
    );
    assert_eq!(
        session.client(index).unwrap().motor_speeds()[0][LARGE],
        0.25
    );
}
