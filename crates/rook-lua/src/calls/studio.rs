//! What an editor asks of a DataModel: its Studio mode, its plugins,
//! their toolbar buttons and widgets, and undo
//! (docs/behavior/studio/plugins.md).

use rook_dom::InstanceId;
use rook_types::Value;

use crate::Runtime;
use crate::studio::StudioMode;

impl Runtime {
    /// Put this DataModel in one of Studio's modes (`None`: a live game).
    /// Edit mode records edits for ChangeHistoryService, leaving out the
    /// GUI Studio and its plugins draw (CoreGui, PluginGuiService).
    pub fn set_studio_mode(&self, mode: Option<StudioMode>) {
        let mut w = self.world_mut();
        w.studio.mode = mode;
        if mode == Some(StudioMode::Edit) {
            for service in ["CoreGui", "PluginGuiService"] {
                if let Ok(id) = w.dom.get_service(service) {
                    w.dom.leave_unrecorded(id);
                }
            }
            w.dom.start_recording();
        } else {
            w.dom.stop_recording();
        }
    }

    /// Load a plugin: a Plugin named `name`, outside the DataModel, holding
    /// `contents` (instances made unparented), then start its enabled
    /// Scripts in tree order, each with `plugin` set.
    pub fn load_plugin(&self, name: &str, contents: &[InstanceId]) -> Option<InstanceId> {
        let plugin = {
            let mut w = self.world_mut();
            let plugin = w.dom.create_by_name("Plugin").ok()?;
            rook_dom::ignore_stale(w.dom.set_name(plugin, name.to_owned()));
            for &c in contents {
                if let Err(e) = w.dom.set_parent(c, Some(plugin)) {
                    tracing::warn!("putting {} in plugin {name}: {e}", w.dom.name(c));
                }
            }
            plugin
        };
        let scripts: Vec<InstanceId> = {
            let w = self.world();
            std::iter::once(plugin)
                .chain(w.dom.descendants(plugin))
                .filter(|&s| {
                    w.dom.class_name(s) == "Script"
                        && w.dom.bool_property(s, "Disabled") != Some(true)
                })
                .collect()
        };
        for script in scripts {
            self.run_script(script);
        }
        Some(plugin)
    }

    /// Unload a plugin: its `Unloading` fires, its scripts stop, and its
    /// widgets go.
    pub fn unload_plugin(&self, plugin: InstanceId) {
        self.fire_event(plugin, "Unloading", ());
        let (scripts, widgets) = {
            let mut w = self.world_mut();
            let scripts: Vec<InstanceId> = w
                .dom
                .descendants(plugin)
                .into_iter()
                .filter(|&s| w.started.contains_key(&s))
                .collect();
            let widgets: Vec<InstanceId> = (w.studio.widgets.iter())
                .filter(|(_, info)| info.plugin == plugin)
                .map(|(&id, _)| id)
                .collect();
            for id in &widgets {
                w.studio.widgets.remove(id);
            }
            if w.studio.active.is_some_and(|(p, _)| p == plugin) {
                w.studio.active = None;
            }
            (scripts, widgets)
        };
        for s in scripts {
            self.stop_script(s);
        }
        for widget in widgets.into_iter().chain([plugin]) {
            self.destroy(widget);
        }
    }

    /// A toolbar button was pressed: its `Click` fires.
    pub fn press_button(&self, button: InstanceId) {
        self.fire_event(button, "Click", ());
    }

    /// A widget's close button was pressed: the function its plugin bound
    /// runs, or else the widget hides.
    pub fn close_widget(&self, widget: InstanceId) {
        let key = format!("rook.close.{}", widget.to_ref().0);
        let bound = self
            .lua
            .named_registry_value::<Option<mlua::Function>>(&key)
            .ok()
            .flatten();
        match bound {
            Some(f) => self.guarded(|| {
                if let Err(e) = crate::runtime::threads::spawn(&self.lua, f, ()) {
                    tracing::warn!("a widget's close function failed to start: {e}");
                }
                crate::runtime::threads::drain_deferred(&self.lua);
            }),
            None => {
                let start = self.world().dom.journal().end();
                let result = self.world_mut().dom.unrecorded(|dom| {
                    dom.set_property_by_name(widget, "Enabled", Value::Bool(false))
                });
                rook_dom::ignore_stale(result);
                self.dispatch_since(start);
            }
        }
    }

    /// ChangeHistoryService:Undo, as the editor's own shortcut.
    pub fn undo(&self) {
        self.history_call(crate::api::methods::studio_undo);
    }

    /// ChangeHistoryService:Redo.
    pub fn redo(&self) {
        self.history_call(crate::api::methods::studio_redo);
    }

    fn history_call(&self, f: fn(&mlua::Lua, InstanceId) -> crate::runtime::ctx::ApiResult<()>) {
        let service = self.world_mut().dom.get_service("ChangeHistoryService");
        let Ok(service) = service else {
            return;
        };
        self.guarded(|| {
            if let Err(e) = f(&self.lua, service) {
                tracing::warn!("undo failed: {}", e.0);
            }
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }

    /// Select `ids` as `Selection:Set` does, with its rule for firing
    /// `SelectionChanged`.
    pub fn select(&self, ids: Vec<InstanceId>) {
        let service = self.world_mut().dom.get_service("Selection");
        if let Ok(service) = service {
            self.guarded(|| {
                crate::api::methods::studio_select(&self.lua, service, ids);
                crate::runtime::threads::drain_deferred(&self.lua);
            });
        }
    }

    /// The selection, without what left the DataModel.
    pub fn selection(&self) -> Vec<InstanceId> {
        crate::api::methods::studio_selection(&self.world())
    }
}
