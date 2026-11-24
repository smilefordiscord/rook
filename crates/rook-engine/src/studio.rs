//! Editing a place as Studio does: a DataModel in Edit mode, where nothing
//! runs but plugins and edits can be undone; loading plugins; the editor's
//! camera; and saving what was edited (docs/behavior/studio/plugins.md).
//! An editor is the host: it draws the DataModel, places the plugins' dock
//! widgets and shows their toolbar buttons, through the calls here and
//! [`rook_lua::studio`].

use std::cell::Ref;

pub use rook_lua::studio::{Button, DockState, Studio, StudioMode, Widget};

use super::*;

/// Children of the DataModel a place file doesn't keep: what Studio and
/// its plugins draw, Studio's own state, and the session's network side
/// (Inference: places Studio saves have none of them).
const UNSAVED: &[&str] = &[
    "CoreGui",
    "PluginGuiService",
    "Selection",
    "ChangeHistoryService",
    "NetworkServer",
    "NetworkClient",
];

impl DataModel {
    /// A place opened for editing: loaded as for playing, but in Edit mode,
    /// so no script starts, nothing simulates, StarterGui shows and edits
    /// are recorded for undo.
    pub fn edit_place(bytes: &[u8], options: &Options) -> Result<Self, EngineError> {
        // Inference: Studio makes a legacy place's surface joints when it
        // runs, so saving an edited place doesn't add them to the file.
        let dm = Self::load_place_joining(bytes, options, false)?;
        dm.runtime.set_studio_mode(Some(StudioMode::Edit));
        dm.runtime.world_mut().gui.set_editing(true);
        Ok(dm)
    }

    /// Put a DataModel of a play test in its Studio mode, so RunService
    /// says where it runs as in Studio.
    pub fn set_studio_mode(&self, mode: Option<StudioMode>) {
        self.runtime.set_studio_mode(mode);
    }

    pub fn is_editing(&self) -> bool {
        self.runtime.world().studio.is_edit()
    }

    /// What plugins made and chose: the selection, history, toolbar
    /// buttons, dock widgets, settings.
    pub fn studio(&self) -> Ref<'_, Studio> {
        Ref::map(self.runtime.world(), |w| &w.studio)
    }

    /// Load a plugin from a model file (`.rbxm` or `.rbxmx`): what the
    /// file holds goes in a Plugin named `name`, and its Scripts start.
    pub fn load_plugin(&mut self, name: &str, bytes: &[u8]) -> Result<InstanceId, EngineError> {
        let loaded = rook_formats::load(bytes)?;
        let contents = {
            let mut w = self.runtime.world_mut();
            let holder = w
                .dom
                .create_by_name("Folder")
                .map_err(|e| EngineError::Lua(e.to_string()))?;
            rook_formats::into_dom(loaded.dom, &mut w.dom, holder);
            let contents = w.dom.children(holder).to_vec();
            for &c in &contents {
                rook_dom::ignore_stale(w.dom.set_parent(c, None));
            }
            contents
        };
        let plugin = self.runtime.load_plugin(name, &contents);
        self.runtime.drain_deferred();
        plugin.ok_or_else(|| EngineError::Lua(format!("plugin {name} didn't load")))
    }

    /// Load a plugin that is one Script of Luau source.
    pub fn load_plugin_source(&mut self, name: &str, source: &str) -> Option<InstanceId> {
        let script = {
            let mut w = self.runtime.world_mut();
            let script = w.dom.create_by_name("Script").ok()?;
            rook_dom::ignore_stale(w.dom.set_name(script, name.to_owned()));
            rook_dom::ignore_stale(w.dom.set_property_by_name(
                script,
                "Source",
                Value::String(source.to_owned()),
            ));
            script
        };
        let plugin = self.runtime.load_plugin(name, &[script]);
        self.runtime.drain_deferred();
        plugin
    }

    /// The place as a file: the DataModel's children but those a place
    /// doesn't keep ([`UNSAVED`]) and those not `Archivable`.
    pub fn save_place(&self, format: rook_formats::Format) -> Result<Vec<u8>, EngineError> {
        let w = self.runtime.world();
        let dom = &w.dom;
        let roots: Vec<InstanceId> = (dom.children(dom.root()).iter().copied())
            .filter(|&c| {
                !UNSAVED.contains(&dom.class_name(c))
                    && dom.bool_property(c, "Archivable") != Some(false)
            })
            .collect();
        let weak = rook_formats::from_dom(dom, &roots);
        Ok(rook_formats::save(&weak, format)?)
    }

    /// Where the editor looks from: the current camera's `CFrame`,
    /// `Focus` and `FieldOfView`, which plugins read and the mouse looks
    /// through. Not an edit, so not recorded.
    pub fn set_edit_camera(&self, cframe: rook_types::CFrame, focus: rook_types::CFrame, fov: f32) {
        let Some(camera) = self.current_camera() else {
            return;
        };
        let start = self.runtime.world().dom.journal().end();
        {
            let mut w = self.runtime.world_mut();
            w.dom.unrecorded(|dom| {
                for (name, value) in [
                    ("CFrame", Value::CFrame(cframe)),
                    ("Focus", Value::CFrame(focus)),
                    ("FieldOfView", Value::Float32(fov)),
                ] {
                    rook_dom::ignore_stale(dom.set_property_by_name(camera, name, value));
                }
            });
        }
        self.runtime.dispatch_since(start);
    }

    /// Where the current camera looks from now: its `CFrame`, `Focus` and
    /// `FieldOfView`. A plugin may have moved it since
    /// [`DataModel::set_edit_camera`], and the editor's view follows, as
    /// Studio's does (measured in Studio 0.748: a script setting the
    /// current camera's `CFrame` moves the view, which stays there, and
    /// makes no undo step).
    pub fn edit_camera(&self) -> Option<(rook_types::CFrame, rook_types::CFrame, f32)> {
        let camera = self.current_camera()?;
        let w = self.runtime.world();
        let cframe = w.dom.cframe_property(camera, "CFrame")?;
        let focus = w.dom.cframe_property(camera, "Focus")?;
        let fov = w.dom.float_property(camera, "FieldOfView")?;
        Some((cframe, focus, fov))
    }

    /// Show dock widget `widget` at `rect` (left, top, width, height in
    /// window pixels), or nowhere.
    pub fn set_widget_rect(&self, widget: InstanceId, rect: Option<[f32; 4]>) {
        self.runtime.world_mut().gui.set_widget_rect(widget, rect);
    }

    /// A toolbar button was pressed.
    pub fn press_button(&self, button: InstanceId) {
        self.runtime.press_button(button);
    }

    /// A dock widget's close button was pressed.
    pub fn close_widget(&self, widget: InstanceId) {
        self.runtime.close_widget(widget);
    }

    pub fn undo(&self) {
        self.runtime.undo();
    }

    pub fn redo(&self) {
        self.runtime.redo();
    }

    pub fn select(&self, ids: Vec<InstanceId>) {
        self.runtime.select(ids);
    }

    pub fn selection(&self) -> Vec<InstanceId> {
        self.runtime.selection()
    }

    /// Scripts plugins asked the editor to open since the last call.
    pub fn take_open_requests(&self) -> Vec<(InstanceId, Option<u32>)> {
        std::mem::take(&mut self.runtime.world_mut().studio.open_requests)
    }
}
