//! Teleports: the requests a DataModel's scripts queued, and the session's
//! answers (docs/behavior/scripting/teleports.md).

use mlua::Value as LuaValue;
use rook_dom::InstanceId;

use crate::api::instance::push_instance;
use crate::net::from_net;
use crate::runtime::signal::SignalKey;
use crate::runtime::threads::{drain_deferred, fire, resume};
use crate::teleport::{
    Arrival, Destination, Failure, JoinData, PlaceInstance, Request, Returns, Routing,
    TeleportState,
};
use crate::types::enums::named_item;
use crate::{MessageType, Runtime};

impl Runtime {
    /// Whether scripts have asked for a teleport or lookup since the
    /// session last took them.
    pub fn wants_teleports(&self) -> bool {
        !self.world().teleports.requests.is_empty()
    }

    /// Teleports and lookups asked for since the last call.
    pub fn take_teleport_requests(&self) -> Vec<Request> {
        std::mem::take(&mut self.world_mut().teleports.requests)
    }

    /// Queue a teleport for this DataModel's session, as a script's call
    /// would.
    pub fn request_teleport(&self, request: crate::teleport::TeleportRequest) {
        self.world_mut()
            .teleports
            .requests
            .push(Request::Teleport(request));
    }

    /// The call waiting on a teleport returns: where the players went, or
    /// nothing when the teleport failed (its failure is fired apart).
    pub fn finish_teleport(&self, call: u64, went: Option<&Destination>) {
        let Some((thread, returns)) = self.world_mut().teleports.waiting.remove(&call) else {
            return;
        };
        let lua = &self.lua;
        let value = (|| -> mlua::Result<LuaValue> {
            Ok(match (returns, went) {
                (Returns::Result, Some(d)) => {
                    let result = self.teleport_result(d)?;
                    LuaValue::UserData(push_instance(lua, result)?)
                }
                (Returns::JobId, Some(d)) => LuaValue::String(lua.create_string(&d.job_id)?),
                _ => LuaValue::Nil,
            })
        })();
        let value = value.unwrap_or_else(|e| {
            self.world_mut()
                .push_log(MessageType::Error, format!("teleport result: {e}"));
            LuaValue::Nil
        });
        self.guarded(|| {
            resume(lua, &thread, value);
            drain_deferred(lua);
        });
    }

    /// A new `TeleportAsyncResult` describing `d` (creator-docs).
    fn teleport_result(&self, d: &Destination) -> mlua::Result<InstanceId> {
        let mut w = self.world_mut();
        let made = (|| {
            let result = w.dom.create_by_name("TeleportAsyncResult")?;
            for (name, value) in [
                ("PrivateServerId", &d.private_server_id),
                ("ReservedServerAccessCode", &d.access_code),
            ] {
                w.dom.set_property_by_name(
                    result,
                    name,
                    rook_types::Value::String(value.clone()),
                )?;
            }
            Ok::<_, rook_dom::DomError>(result)
        })();
        made.map_err(mlua::Error::external)
    }

    /// Answer a `GetPlayerPlaceInstanceAsync`: where the user plays, or
    /// the error it raises.
    pub fn answer_lookup(&self, call: u64, found: Result<PlaceInstance, String>) {
        let Some((thread, _)) = self.world_mut().teleports.waiting.remove(&call) else {
            return;
        };
        let lua = &self.lua;
        self.guarded(|| {
            match found {
                Ok(p) => resume(lua, &thread, (true, p.current, "", p.place_id, p.job_id)),
                Err(message) => resume(lua, &thread, (false, message)),
            }
            drain_deferred(lua);
        });
    }

    /// Fire `TeleportService.TeleportInitFailed` for `player`, with new
    /// TeleportOptions that retry the teleport (creator-docs).
    pub fn teleport_init_failed(&self, player: InstanceId, failure: &Failure) {
        let lua = &self.lua;
        let fired = (|| -> mlua::Result<()> {
            let (service, options) = {
                let mut w = self.world_mut();
                let service = w.dom.get_service("TeleportService");
                let options = w.dom.create_by_name("TeleportOptions");
                let (Ok(service), Ok(options)) = (service, options) else {
                    return Ok(());
                };
                retry_options(&mut w.dom, options, &failure.routing)
                    .map_err(mlua::Error::external)?;
                if let Some(data) = &failure.data {
                    w.teleports.options_data.insert(options, data.clone());
                }
                (service, options)
            };
            let args = [
                LuaValue::UserData(push_instance(lua, player)?),
                named_item(lua, "TeleportResult", failure.result.item())?,
                LuaValue::String(lua.create_string(&failure.message)?),
                LuaValue::Number(failure.place_id as f64),
                LuaValue::UserData(push_instance(lua, options)?),
            ];
            self.guarded(|| {
                fire(lua, &SignalKey::event(service, "TeleportInitFailed"), &args);
                drain_deferred(lua);
            });
            Ok(())
        })();
        if let Err(e) = fired {
            self.world_mut()
                .push_log(MessageType::Error, format!("TeleportInitFailed: {e}"));
        }
    }

    /// Fire `Player.OnTeleport` for the local player.
    pub fn on_teleport(
        &self,
        player: InstanceId,
        state: TeleportState,
        place_id: i64,
        spawn: &str,
    ) {
        let lua = &self.lua;
        let args = (|| -> mlua::Result<_> {
            Ok([
                named_item(lua, "TeleportState", state.item())?,
                LuaValue::Number(place_id as f64),
                LuaValue::String(lua.create_string(spawn)?),
            ])
        })();
        match args {
            Ok(args) => self.guarded(|| {
                fire(lua, &SignalKey::event(player, "OnTeleport"), &args);
                drain_deferred(lua);
            }),
            Err(e) => self
                .world_mut()
                .push_log(MessageType::Error, format!("OnTeleport: {e}")),
        }
    }

    /// How `player` joined, for `GetJoinData`; set before `PlayerAdded`.
    pub fn set_join_data(&self, player: InstanceId, data: JoinData) {
        self.world_mut().teleports.join_data.insert(player, data);
    }

    /// The GUI `SetTeleportGui` (or a legacy method's loading screen) set
    /// here, if it still exists.
    pub fn teleport_gui(&self) -> Option<InstanceId> {
        let w = self.world();
        w.teleports.gui.filter(|&g| w.dom.get(g).is_some())
    }

    /// What this client keeps across teleports: `SetTeleportSetting`'s
    /// values.
    pub fn teleport_settings(&self) -> std::collections::BTreeMap<String, crate::NetValue> {
        self.world().teleports.arrival.settings.clone()
    }

    /// What the local player arrived with; set before scripts start.
    pub fn set_arrival(&self, arrival: Arrival) {
        self.world_mut().teleports.arrival = arrival;
    }

    /// The local player has arrived from a teleport:
    /// `LocalPlayerArrivedFromTeleport` fires with its loading GUI and data,
    /// then the GUI leaves CoreGui unless a handler moved it (creator-docs
    /// SetTeleportGui: parented to nil once the place has loaded).
    pub fn arrived_from_teleport(&self) {
        let lua = &self.lua;
        let (gui, data, service, core_gui) = {
            let mut w = self.world_mut();
            let a = &w.teleports.arrival;
            let (gui, data) = (a.gui, a.teleport_data.clone());
            let service = w.dom.get_service("TeleportService").ok();
            (gui, data, service, w.dom.find_service("CoreGui"))
        };
        let Some(service) = service else {
            return;
        };
        let args = (|| -> mlua::Result<_> {
            Ok([
                match gui {
                    Some(g) => LuaValue::UserData(push_instance(lua, g)?),
                    None => LuaValue::Nil,
                },
                match &data {
                    Some(d) => from_net(lua, d)?,
                    None => LuaValue::Nil,
                },
            ])
        })();
        match args {
            Ok(args) => self.guarded(|| {
                fire(
                    lua,
                    &SignalKey::event(service, "LocalPlayerArrivedFromTeleport"),
                    &args,
                );
                drain_deferred(lua);
            }),
            Err(e) => self.world_mut().push_log(
                MessageType::Error,
                format!("LocalPlayerArrivedFromTeleport: {e}"),
            ),
        }
        let Some(gui) = gui else {
            return;
        };
        let start = self.world().dom.journal().end();
        {
            let mut w = self.world_mut();
            if core_gui.is_some()
                && w.dom.parent(gui) == core_gui
                && let Err(e) = w.dom.set_parent(gui, None)
            {
                tracing::warn!("the arriving teleport GUI stays in CoreGui: {e}");
            }
        }
        self.dispatch_since(start);
    }
}

/// Set the routing field a retry of `routing` needs on new options
/// (creator-docs TeleportInitFailed's table).
fn retry_options(
    dom: &mut rook_dom::Dom,
    options: InstanceId,
    routing: &Routing,
) -> Result<(), rook_dom::DomError> {
    use rook_types::Value;
    let (name, value) = match routing {
        Routing::Public => return Ok(()),
        Routing::Instance(job) => ("ServerInstanceId", Value::String(job.clone())),
        Routing::Reserved(id) => ("ReservedServerId", Value::String(id.clone())),
        Routing::AccessCode(code) => ("ReservedServerAccessCode", Value::String(code.clone())),
        Routing::NewReserved => ("ShouldReserveServer", Value::Bool(true)),
        Routing::Vip(id) => ("VipServerId", Value::String(id.clone())),
    };
    dom.set_property_by_name(options, name, value)
}
