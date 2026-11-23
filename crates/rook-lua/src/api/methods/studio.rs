//! What Studio's plugins call: the Plugin and what it makes (toolbars, dock
//! widgets, its mouse and settings), the Selection, ChangeHistoryService,
//! and the modes RunService reports (docs/behavior/studio/plugins.md,
//! docs/behavior/studio/history.md). The state they share is
//! [`crate::studio::Studio`].

use crate::api::methods::{RawMethod, first};

mod history;
mod modes;
mod plugin;
mod selection;
mod widgets;

pub(crate) use history::{redo as studio_redo, undo as studio_undo};
pub(crate) use selection::{
    current as studio_selection, drop_moved as studio_drop_moved, select as studio_select,
};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(
        &[
            plugin::native,
            widgets::native,
            selection::native,
            history::native,
            modes::native,
        ],
        owner,
        name,
    )
}
