//! Plugins' dock widgets: DockWidgetPluginGuis in PluginGuiService
//! (creator-docs PluginGuiService: it holds them), which the host places in
//! its window and Rook's GUI lays out and draws there
//! (docs/behavior/studio/plugins.md#dock-widgets).

use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::Value;

use super::plugin::make;
use crate::api::methods::{RawMethod, mutate, ret_instance};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::studio::{DockState, Widget};
use crate::types::dock_widget_info::LDockWidgetInfo;

/// A widget named by its id, enabled as the info says, with no title
/// until the plugin gives one (Inference).
fn create(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let id = a.string(0)?;
    let info: LDockWidgetInfo = a.userdata(1, "DockWidgetPluginGuiInfo")?;
    let service = mutate(lua, |w| w.dom.get_service("PluginGuiService"))?;
    let taken = with_world(lua, |w| {
        w.dom
            .children(service)
            .iter()
            .any(|&c| w.dom.name(c) == id && w.studio.widgets.contains_key(&c))
    });
    if taken {
        return Err(ScriptError(format!(
            "A plugin gui with the id {id} already exists"
        )));
    }
    let widget = make(lua, "DockWidgetPluginGui", &id, None)?;
    mutate(lua, |w| {
        w.dom
            .set_property_by_name(widget, "Enabled", Value::Bool(info.initial_enabled))?;
        w.studio.widgets.insert(
            widget,
            Widget {
                plugin: this,
                dock: DockState::from_name(info.dock),
                float_size: info.float_size,
                min_size: info.min_size,
            },
        );
        w.dom.set_parent(widget, Some(service))
    })?;
    ret_instance(lua, Some(widget))
}

/// The mouse from the widget's top left, in pixels.
fn relative_mouse(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let (mouse, origin) = with_world(lua, |w| {
        let origin = w.gui.widget_rect(this).map_or((0.0, 0.0), |r| (r.0, r.1));
        (w.input.mouse, origin)
    });
    let at = rook_types::Vector2::new(mouse.0 - origin.0, mouse.1 - origin.1);
    Outcome::ret(lua, crate::types::LVector2(at))
}

/// What the host calls instead of hiding the widget when its close
/// button is pressed (creator-docs PluginGui:BindToClose); `nil` unbinds.
fn bind_to_close(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = format!("rook.close.{}", this.to_ref().0);
    match a.get(0) {
        mlua::Value::Function(f) => lua.set_named_registry_value(&key, f.clone())?,
        _ => lua.unset_named_registry_value(&key)?,
    }
    Outcome::ret(lua, ())
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Plugin", "CreateDockWidgetPluginGui" | "CreateDockWidgetPluginGuiAsync") => create,
        ("PluginGui", "GetRelativeMousePosition") => relative_mouse,
        ("PluginGui", "BindToClose") => bind_to_close,
        _ => return None,
    })
}
