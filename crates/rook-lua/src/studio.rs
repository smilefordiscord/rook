//! Studio's side of a DataModel: whether it's being edited or play tested
//! (what RunService says), the selection, the change history, and what
//! plugins made that the host shows (toolbar buttons, dock widgets) or
//! keeps (settings) (docs/behavior/studio/plugins.md,
//! docs/behavior/studio/history.md).
//!
//! The host (an editor) sets the mode and reads the rest; scripts reach it
//! through the Plugin, Selection and ChangeHistoryService members in
//! `api/methods/studio/`.

use rook_dom::{InstanceId, Undo};
use rustc_hash::FxHashMap;

/// Which of Studio's modes a DataModel is in (creator-docs RunService:
/// the table of what `IsStudio`, `IsEdit`, `IsRunMode`, `IsServer` and
/// `IsClient` return in each).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioMode {
    /// Editing: no scripts but plugins run, and edits can be undone.
    Edit,
    /// Run: the server alone, simulating.
    Run,
    /// Play: a server and a local player's client.
    Play,
}

/// Where a dock widget first goes (`Enum.InitialDockState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockState {
    Top,
    Bottom,
    Left,
    Right,
    Float,
}

impl DockState {
    pub(crate) fn from_name(name: &str) -> Self {
        match name {
            "Top" => Self::Top,
            "Bottom" => Self::Bottom,
            "Left" => Self::Left,
            "Float" => Self::Float,
            _ => Self::Right,
        }
    }
}

/// A dock widget a plugin made, as `DockWidgetPluginGuiInfo` described it.
#[derive(Debug, Clone, PartialEq)]
pub struct Widget {
    pub plugin: InstanceId,
    pub dock: DockState,
    /// The floating size, and the smallest it may be, in pixels.
    pub float_size: (i32, i32),
    pub min_size: (i32, i32),
}

/// A toolbar button a plugin made: what the host shows for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Button {
    pub toolbar: InstanceId,
    pub text: String,
    pub tooltip: String,
    pub icon: String,
    /// `SetActive`: shown pressed.
    pub active: bool,
}

/// One undoable step: what puts the place back, and its waypoint's name.
#[derive(Debug, Clone)]
pub struct Step {
    pub name: String,
    pub changes: Vec<Undo>,
}

/// A recording `TryBeginRecording` started and `FinishRecording` ends.
#[derive(Debug, Clone)]
pub struct Recording {
    /// `recording_` and a number (Measured).
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
}

/// The change history: undoable steps, newest last, and what undoing
/// them put aside to redo.
#[derive(Debug, Default)]
pub struct History {
    pub undo: Vec<Step>,
    pub redo: Vec<Step>,
    /// `SetEnabled(false)`: waypoints are ignored.
    pub disabled: bool,
    /// The recording `TryBeginRecording` started.
    pub recording: Option<Recording>,
    pub(crate) next_recording: u64,
}

#[derive(Debug, Default)]
pub struct Studio {
    /// `None`: a live game, not Studio.
    pub mode: Option<StudioMode>,
    /// The Selection service's instances, in the order selected.
    pub selection: Vec<InstanceId>,
    pub history: History,
    /// Plugins' settings, by plugin name then key, as JSON
    /// (`Plugin:GetSetting`), which the host loads and saves.
    pub settings: FxHashMap<String, serde_json::Map<String, serde_json::Value>>,
    /// A setting changed since the host last saved them.
    pub settings_changed: bool,
    /// The activated plugin, and whether with the exclusive mouse.
    pub active: Option<(InstanceId, bool)>,
    /// Each plugin's PluginMouse (`Plugin:GetMouse`).
    pub mice: FxHashMap<InstanceId, InstanceId>,
    pub buttons: FxHashMap<InstanceId, Button>,
    pub widgets: FxHashMap<InstanceId, Widget>,
    /// Scripts plugins asked to open (`Plugin:OpenScript`), with the line.
    pub open_requests: Vec<(InstanceId, Option<u32>)>,
}

impl Studio {
    pub fn is_edit(&self) -> bool {
        self.mode == Some(StudioMode::Edit)
    }

    /// The activated plugin's PluginMouse, which hears the viewport's
    /// mouse.
    pub fn active_mouse(&self) -> Option<InstanceId> {
        self.active.and_then(|(p, _)| self.mice.get(&p).copied())
    }
}
