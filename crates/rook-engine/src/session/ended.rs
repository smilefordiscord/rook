//! A client whose connection to its server ended (it was kicked): the
//! place stops running there, frozen as it was, and Roblox's disconnect
//! prompt shows over it until the player picks Leave
//! (docs/behavior/characters/leaving.md). The prompt is Rook's own
//! drawing (`rook_gui::Prompt`), so it shows with or without Roblox's
//! CoreScripts.

use rook_types::{Value, Vector2};

use crate::{DataModel, InputEvent};

/// Whether a client's connection to its server is still open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Connection {
    Open,
    /// It ended; the prompt shows. `pressed`: the first mouse button went
    /// down on the prompt's button and hasn't come up.
    Ended {
        pressed: bool,
    },
    /// The player picked Leave on the prompt.
    Left,
}

/// Roblox's English text for a kick, from its CoreScripts' translations
/// (`InGame.ConnectionError.DisconnectLuaKick` and `...WithMessage`), with
/// the error code its prompt adds (`Enum.ConnectionError.DisconnectLuaKick`
/// is 267). A script's message is cut to its first 200 bytes, as the
/// CoreScript does by default (`MaxKickMessageLength`).
pub fn kick_prompt(message: &str) -> rook_gui::Prompt {
    const KICKED: &str = "You have been kicked by this experience or its moderators.";
    const MAX_MESSAGE: usize = 200;
    let mut end = message.len().min(MAX_MESSAGE);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    let text = match &message[..end] {
        "" => KICKED.to_owned(),
        shown => format!("{KICKED} Moderation message:  {shown}"),
    };
    rook_gui::Prompt {
        title: "Disconnected".into(),
        message: format!("{text}\n(Error Code: 267)"),
        button: "Leave".into(),
    }
}

/// What a client shows when its teleport couldn't finish after its player
/// had left the server: Rook's own words, as Roblox's client shows its
/// own error there.
pub fn teleport_failed_prompt(why: &str) -> rook_gui::Prompt {
    rook_gui::Prompt {
        title: "Teleport Failed".into(),
        message: format!("The place couldn't be joined: {why}"),
        button: "Leave".into(),
    }
}

impl DataModel {
    /// The connection to the server ended: from now on the place doesn't
    /// run here (steps do nothing), and `prompt` shows over it, taking the
    /// window's input and freeing the mouse, as Roblox's prompt does.
    pub fn end_session(&mut self, prompt: rook_gui::Prompt) {
        if self.connection != Connection::Open {
            return;
        }
        self.connection = Connection::Ended { pressed: false };
        self.runtime.world_mut().gui.show_prompt(prompt);
    }

    pub fn session_ended(&self) -> bool {
        self.connection != Connection::Open
    }

    /// Whether the player picked Leave on the prompt: the window closes.
    pub fn left(&self) -> bool {
        self.connection == Connection::Left
    }

    /// Input while the prompt shows: the mouse moves, and pressing the
    /// first button on the prompt's button and releasing it there leaves
    /// (Inference: as a GuiButton's `Activated`), so a button held when
    /// the kick came doesn't. Nothing reaches the place's scripts.
    pub(crate) fn prompt_input(&mut self, event: &InputEvent) {
        let mut w = self.runtime.world_mut();
        let Connection::Ended { pressed } = self.connection else {
            return;
        };
        match *event {
            InputEvent::MouseMove { x, y } => w.input.mouse = (x, y),
            InputEvent::MouseButton { button: 1, down } => {
                let (x, y) = w.input.mouse;
                let over = w.gui.prompt_button_at(glam::Vec2::new(x, y));
                self.connection = match (down, over && (down || pressed)) {
                    (false, true) => Connection::Left,
                    (_, pressed) => Connection::Ended { pressed },
                };
            }
            _ => {}
        }
    }

    /// The window's size changed after the connection ended: the camera
    /// takes it without anything firing, as scripts stop with the
    /// connection, and the prompt is centered again.
    pub(crate) fn resize_ended(&self) {
        let (width, height) = self.input.viewport;
        let camera = self.current_camera();
        let mut w = self.runtime.world_mut();
        if let Some(camera) = camera {
            let size = Value::Vector2(Vector2::new(width as f32, height as f32));
            if let Err(e) = w.dom.set_property_by_name(camera, "ViewportSize", size) {
                tracing::warn!("sizing the camera after the connection ended: {e}");
            }
        }
        w.gui.resize_prompt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kick_text_cuts_long_messages_at_a_character() {
        let plain = kick_prompt("");
        assert_eq!(
            plain.message,
            "You have been kicked by this experience or its moderators.\n(Error Code: 267)"
        );
        let long = format!("{}é", "a".repeat(199));
        let cut = kick_prompt(&long);
        assert!(
            cut.message
                .contains(&format!("message:  {}\n", "a".repeat(199)))
        );
    }
}
