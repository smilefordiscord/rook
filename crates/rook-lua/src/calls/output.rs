//! `LogService.MessageOut`: each line of output, announced once a frame
//! after it's printed (Confirmed: creator-docs LogService.MessageOut
//! fires for prints, warnings and errors; Measured: deferred, never during
//! the print; Approximation: at the frame's end rather than the printing
//! thread's next yield) (docs/design/data-and-logging.md).

use crate::Runtime;
use crate::runtime::world::MessageType;

/// `Enum.MessageType`'s value for an entry's kind.
fn message_type(kind: MessageType) -> u32 {
    match kind {
        MessageType::Output => 0,
        MessageType::Info => 1,
        MessageType::Warning => 2,
        MessageType::Error => 3,
    }
}

/// An entry as `MessageOut` announces it: an error's stack, which Rook
/// keeps in the error's entry, is lines of its own (Measured: `Stack
/// Begin`, each frame and `Stack End`, as `MessageInfo`, after the error).
fn lines(message: String, kind: MessageType) -> Vec<(String, MessageType)> {
    let stack = match kind {
        MessageType::Error => message.find(
            "
Stack Begin",
        ),
        _ => None,
    };
    let Some(at) = stack else {
        return vec![(message, kind)];
    };
    let mut out = vec![(message[..at].to_owned(), kind)];
    out.extend(
        message[at + 1..]
            .lines()
            .map(|l| (l.to_owned(), MessageType::Info)),
    );
    out
}

impl Runtime {
    /// Fire `LogService.MessageOut` for the output since the last call.
    /// Output it prints while firing waits for the next call.
    pub fn announce_output(&self) {
        let (pending, service) = {
            let mut w = self.world_mut();
            if w.message_out.is_empty() {
                return;
            }
            let pending = std::mem::take(&mut w.message_out);
            (pending, w.dom.find_service("LogService"))
        };
        let Some(service) = service else {
            return;
        };
        for (message, kind) in pending.into_iter().flat_map(|(m, k)| lines(m, k)) {
            let Ok(item) = self.push_enum_item("MessageType", message_type(kind)) else {
                continue;
            };
            // Measured: a third argument, the API dump's `context`, an
            // empty table for prints, warnings and errors alike.
            let Ok(context) = self.lua.create_table() else {
                continue;
            };
            self.fire_event(service, "MessageOut", (message, item, context));
        }
    }

    /// Add a line printed elsewhere (a play test) to this DataModel's
    /// output, for `MessageOut` to announce, without printing it here.
    pub fn add_output(&self, message: String, kind: MessageType) {
        self.world_mut().message_out.push((message, kind));
    }
}
