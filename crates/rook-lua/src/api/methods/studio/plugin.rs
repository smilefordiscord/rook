//! A Plugin's own members: its toolbars and their buttons, activation and
//! its mouse, its settings, and asking the editor to open a script
//! (creator-docs Plugin, PluginToolbar, PluginToolbarButton;
//! docs/behavior/studio/plugins.md).

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;

use crate::api::methods::web::http::{from_json, to_json};
use crate::api::methods::{RawMethod, mutate, ret_instance};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, with_world};
use crate::runtime::signal::SignalKey;
use crate::runtime::threads::fire;
use crate::studio::Button;

/// A new instance of `class` named `name` in `parent` (or nowhere).
pub(super) fn make(
    lua: &Lua,
    class: &str,
    name: &str,
    parent: Option<InstanceId>,
) -> ApiResult<InstanceId> {
    mutate(lua, |w| {
        let id = w.dom.create_by_name(class)?;
        w.dom.set_name(id, name.to_owned())?;
        w.dom.set_parent(id, parent)?;
        Ok(id)
    })
}

/// A toolbar in the plugin (Inference: where Studio keeps it isn't seen
/// by scripts; in the plugin, so it goes with it).
fn create_toolbar(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let toolbar = make(lua, "PluginToolbar", &name, Some(this))?;
    ret_instance(lua, Some(toolbar))
}

/// A button in the toolbar, named by its id; the host shows its text
/// (its id when the text is empty), tooltip and icon.
fn create_button(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let id = a.string(0)?;
    let button = Button {
        toolbar: this,
        tooltip: a.string(1)?,
        icon: a.string(2)?,
        text: a.opt_string(3)?.unwrap_or_default(),
        active: false,
    };
    let made = make(lua, "PluginToolbarButton", &id, Some(this))?;
    with_world(lua, |w| w.studio.buttons.insert(made, button));
    ret_instance(lua, Some(made))
}

fn set_active(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let on = a.truthy(0);
    with_world(lua, |w| {
        if let Some(b) = w.studio.buttons.get_mut(&this) {
            b.active = on;
        }
    });
    Outcome::ret(lua, ())
}

/// One plugin is activated at a time: activating another deactivates the
/// last, whose `Deactivation` fires (creator-docs Plugin:Activate).
fn activate(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let exclusive = a.truthy(0);
    let before = with_world(lua, |w| w.studio.active.replace((this, exclusive)));
    if let Some((other, _)) = before
        && other != this
    {
        fire(lua, &SignalKey::event(other, "Deactivation"), &[]);
    }
    Outcome::ret(lua, ())
}

fn deactivate(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let was = with_world(lua, |w| {
        let was = w.studio.active.is_some_and(|(p, _)| p == this);
        if was {
            w.studio.active = None;
        }
        was
    });
    if was {
        fire(lua, &SignalKey::event(this, "Deactivation"), &[]);
    }
    Outcome::ret(lua, ())
}

fn is_activated(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let on = with_world(lua, |w| w.studio.active.is_some_and(|(p, _)| p == this));
    Outcome::ret(lua, on)
}

fn is_activated_exclusive(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let on = with_world(lua, |w| w.studio.active == Some((this, true)));
    Outcome::ret(lua, on)
}

/// The plugin's PluginMouse, the same one each call; it hears the
/// viewport while the plugin is activated.
fn get_mouse(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    if let Some(m) = with_world(lua, |w| w.studio.mice.get(&this).copied()) {
        return ret_instance(lua, Some(m));
    }
    let mouse = make(lua, "PluginMouse", "PluginMouse", None)?;
    with_world(lua, |w| w.studio.mice.insert(this, mouse));
    ret_instance(lua, Some(mouse))
}

/// Settings are kept per plugin, by its name, as JSON (creator-docs
/// Plugin:SetSetting: values that JSON can hold); `nil` once unset.
fn get_setting(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    let value = with_world(lua, |w| {
        let name = w.dom.name(this).to_owned();
        w.studio
            .settings
            .get(&name)
            .and_then(|s| s.get(&key))
            .cloned()
    });
    match value {
        Some(v) => Outcome::ret(lua, from_json(lua, &v)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

fn set_setting(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    let value = to_json(a.get(1), 0)?;
    with_world(lua, |w| {
        let name = w.dom.name(this).to_owned();
        let settings = w.studio.settings.entry(name).or_default();
        if value.is_null() {
            settings.remove(&key);
        } else {
            settings.insert(key, value);
        }
        w.studio.settings_changed = true;
    });
    Outcome::ret(lua, ())
}

/// Ask the editor to show a script, at a line if given.
fn open_script(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let script = a.instance(0)?;
    let line = a.opt_number(1)?.map(|l| l.max(1.0) as u32);
    with_world(lua, |w| w.studio.open_requests.push((script, line)));
    Outcome::ret(lua, ())
}

/// Nobody is signed in to Roblox (Inference: Studio says 0 then).
fn studio_user_id(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, 0)
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Plugin", "CreateToolbar") => create_toolbar,
        ("PluginToolbar", "CreateButton") => create_button,
        ("PluginToolbarButton", "SetActive") => set_active,
        ("Plugin", "Activate") => activate,
        ("Plugin", "Deactivate") => deactivate,
        ("Plugin", "IsActivated") => is_activated,
        ("Plugin", "IsActivatedWithExclusiveMouse") => is_activated_exclusive,
        ("Plugin", "GetMouse") => get_mouse,
        ("Plugin", "GetSetting") => get_setting,
        ("Plugin", "SetSetting") => set_setting,
        ("Plugin", "OpenScript") => open_script,
        ("Plugin", "GetStudioUserId") => studio_user_id,
        _ => return None,
    })
}
