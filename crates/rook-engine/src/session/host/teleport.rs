//! Teleports at the session's two ends (docs/behavior/scripting/teleports.md):
//! the server routes its scripts' requests into [`Plan`]s, and a plan
//! sends the players away as [`Departure`]s, both for the
//! [`Router`](crate::session::router::Router), the one caller; a client
//! hears how its player's teleport goes, and [`Carried`] is what it takes
//! to the next server's client.

use std::collections::BTreeMap;

use rook_lua::teleport::{Arrival, Destination, Request};
use rustc_hash::FxHashSet;

use super::*;
use crate::universe::Universe;
use crate::universe::routing::{self, Refusal, Route, RunningServer, Target};

/// A routed teleport, which [`ServerHost::depart`] carries out
/// ([`crate::session::router`]).
#[derive(Debug)]
pub(crate) struct Plan {
    request: TeleportRequest,
    /// The clients of the players going.
    clients: Vec<usize>,
    pub(crate) route: Route,
}

/// A player who left for another server.
#[derive(Debug)]
pub struct Departure {
    /// The client's index on the server it left.
    pub client: usize,
    pub name: String,
    pub user_id: i64,
    pub place_id: i64,
    /// What the destination's server holds for the player.
    pub join: JoinData,
    /// The teleport data the arriving client gets.
    pub teleport_data: Option<NetValue>,
    /// What the client hears before it goes.
    pub messages: Vec<ToClient>,
}

/// What a client takes to the next server's client.
#[derive(Debug, Default)]
pub struct Carried {
    pub teleport_data: Option<NetValue>,
    /// The loading GUI `SetTeleportGui` set, copied out with its
    /// descendants; references outside it are dropped (creator-docs
    /// SetTeleportGui).
    pub gui: Option<rbx_dom_weak::WeakDom>,
    pub settings: BTreeMap<String, NetValue>,
}

impl ServerHost {
    /// This server, as routing sees it.
    pub fn running(&self) -> RunningServer {
        let w = self.server.runtime().world();
        let dom = &w.dom;
        let users: Vec<i64> = self
            .links()
            .map(|(_, c)| dom.number_property(c.player, "UserId").unwrap_or(0.0) as i64)
            .collect();
        let max_players = (dom.find_service("Players"))
            .and_then(|p| dom.number_property(p, "MaxPlayers"))
            .map_or(usize::MAX, |n| n.max(0.0) as usize);
        drop(w);
        RunningServer {
            identity: crate::universe::ServerIdentity::read(&self.server),
            players: users.len(),
            max_players,
            users,
        }
    }

    /// Route the teleports this server's scripts asked for since the last
    /// call, and answer their lookups. `running` is every server of the
    /// universe; the plans' new servers and joining players are added to
    /// it, so later requests see them. Refused teleports fail here.
    pub(crate) fn teleports(
        &mut self,
        universe: &Universe,
        running: &mut Vec<RunningServer>,
    ) -> Vec<Plan> {
        let requests = self.server.runtime().take_teleport_requests();
        if requests.is_empty() {
            return Vec::new();
        }
        let me = crate::universe::ServerIdentity::read(&self.server);
        let mut going = FxHashSet::default();
        let mut plans = Vec::new();
        for request in requests {
            let request = match request {
                Request::Lookup { call, user_id } => {
                    let found = routing::find_user(running, user_id, &me.job_id);
                    self.server.runtime().answer_lookup(call, found);
                    continue;
                }
                Request::Teleport(request) => request,
            };
            let mut clients = Vec::new();
            for &player in &request.players {
                let client = self.links().find(|(_, c)| c.player == player);
                match client {
                    Some((index, _)) if going.insert(player) => clients.push(index),
                    Some((index, _)) => {
                        let refusal = (
                            rook_lua::teleport::TeleportResult::IsTeleporting,
                            "The player is already teleporting".to_owned(),
                        );
                        self.fail(&request, &[(Some(index), player)], refusal);
                    }
                    // Inference: a Player with no client fails as an
                    // unknown failure.
                    None => {
                        let refusal = (
                            rook_lua::teleport::TeleportResult::Failure,
                            "The player is not connected".to_owned(),
                        );
                        self.fail(&request, &[(None, player)], refusal);
                    }
                }
            }
            if clients.is_empty() {
                self.finish(&request, None);
                continue;
            }
            let routed = routing::route(
                universe,
                running,
                request.place_id,
                &request.routing,
                clients.len(),
            );
            match routed {
                Ok(route) => {
                    match &route.target {
                        Target::New(identity) => running.push(RunningServer {
                            identity: identity.clone(),
                            players: clients.len(),
                            max_players: usize::MAX,
                            users: Vec::new(),
                        }),
                        Target::Running(job) => {
                            if let Some(s) = running.iter_mut().find(|s| s.identity.job_id == *job)
                            {
                                s.players += clients.len();
                            }
                        }
                    }
                    plans.push(Plan {
                        request,
                        clients,
                        route,
                    });
                }
                Err(refusal) => self.refuse_clients(&request, &clients, refusal),
            }
        }
        plans
    }

    fn refuse_clients(&mut self, request: &TeleportRequest, clients: &[usize], refusal: Refusal) {
        let players: Vec<(Option<usize>, InstanceId)> = (clients.iter())
            .filter_map(|&i| Some((Some(i), self.player(i)?)))
            .collect();
        self.fail(request, &players, refusal);
        self.finish(request, None);
    }

    /// `TeleportInitFailed` fires for each player, here and on its client,
    /// whose `OnTeleport` reports it failed; the players stay.
    fn fail(
        &mut self,
        request: &TeleportRequest,
        players: &[(Option<usize>, InstanceId)],
        (result, message): Refusal,
    ) {
        let failure = rook_lua::teleport::Failure {
            result,
            message,
            place_id: request.place_id,
            routing: request.routing.clone(),
            data: request.data.clone(),
        };
        for &(client, player) in players {
            self.server.runtime().teleport_init_failed(player, &failure);
            if let Some(index) = client {
                self.pending
                    .push((index, ToClient::TeleportFailed(failure.clone())));
            }
        }
    }

    /// The call waiting on `request` returns.
    fn finish(&self, request: &TeleportRequest, went: Option<&Destination>) {
        if let Some(call) = request.call {
            self.server.runtime().finish_teleport(call, went);
        }
    }

    /// Carry out a plan whose server is there: the waiting call returns,
    /// then each player leaves this server for it (Inference: in that
    /// order). A server's teleport gives the destination `SourcePlaceId`,
    /// `SourceGameId`, `TeleportData` and, for a group, `Members`
    /// (creator-docs Player:GetJoinData); a client's own legacy teleport
    /// carries its data to the client only.
    pub(crate) fn depart(&mut self, plan: Plan) -> Vec<Departure> {
        let Plan {
            request,
            clients,
            route,
        } = plan;
        self.finish(&request, Some(&route.destination));
        let me = crate::universe::ServerIdentity::read(&self.server);
        let who = |host: &Self, index: usize| {
            let player = host.player(index)?;
            let w = host.server.runtime().world();
            let user = w.dom.number_property(player, "UserId").unwrap_or(0.0) as i64;
            Some((w.dom.name(player).to_owned(), user))
        };
        let people: Vec<(usize, String, i64)> = (clients.iter())
            .filter_map(|&i| who(self, i).map(|(name, user)| (i, name, user)))
            .collect();
        let members: Vec<i64> = if people.len() > 1 {
            people.iter().map(|(_, _, user)| *user).collect()
        } else {
            Vec::new()
        };
        let mut departures = Vec::new();
        for (index, name, user_id) in people {
            let states = [
                request
                    .from_server
                    .then_some(TeleportState::RequestedFromServer),
                Some(TeleportState::Started),
                Some(TeleportState::WaitingForServer),
                Some(TeleportState::InProgress),
            ];
            let messages = (states.into_iter().flatten())
                .map(|state| ToClient::TeleportState {
                    state,
                    place_id: request.place_id,
                    spawn_name: request.spawn_name.clone(),
                })
                .collect();
            let join = if request.from_server {
                JoinData {
                    source_place_id: Some(me.place_id),
                    source_game_id: Some(me.game_id),
                    teleport_data: request.data.clone(),
                    members: members.clone(),
                    launch_data: None,
                    spawn_name: request.spawn_name.clone(),
                }
            } else {
                JoinData {
                    spawn_name: request.spawn_name.clone(),
                    ..JoinData::default()
                }
            };
            // Inference: leaving for a teleport is an Unknown exit, as
            // `Enum.PlayerExitReason` names no teleport.
            self.leave(index, rook_lua::ExitReason::Unknown);
            departures.push(Departure {
                client: index,
                name,
                user_id,
                place_id: request.place_id,
                join,
                teleport_data: request.data.clone(),
                messages,
            });
        }
        departures
    }
}

impl ClientHost {
    /// What the client takes to its next server: the teleport's data, its
    /// teleport GUI and its teleport settings.
    pub(crate) fn carry(&self, teleport_data: Option<NetValue>) -> Carried {
        let rt = self.client.runtime();
        let gui = rt.teleport_gui().map(|gui| {
            let w = rt.world();
            rook_formats::from_dom(&w.dom, &[gui])
        });
        Carried {
            teleport_data,
            gui,
            settings: rt.teleport_settings(),
        }
    }

    /// Put what a teleport carried into the new client, before its
    /// scripts start: the GUI goes in CoreGui (creator-docs
    /// SetTeleportGui).
    pub(super) fn receive(client: &DataModel, carried: Carried) {
        let rt = client.runtime();
        let gui = carried.gui.and_then(|weak| {
            let mut w = rt.world_mut();
            let core_gui = w.dom.get_service("CoreGui").ok()?;
            rook_formats::into_dom(weak, &mut w.dom, core_gui)
                .top
                .first()
                .copied()
        });
        rt.set_arrival(Arrival {
            teleport_data: carried.teleport_data,
            gui,
            settings: carried.settings,
        });
    }

    /// A teleport message from the server, for the local player.
    pub(super) fn teleport_message(&self, msg: &ToClient) {
        let rt = self.client.runtime();
        let Some(local) = rt.world().dom.local_player() else {
            return;
        };
        match msg {
            ToClient::TeleportState {
                state,
                place_id,
                spawn_name,
            } => rt.on_teleport(local, *state, *place_id, spawn_name),
            ToClient::TeleportFailed(failure) => {
                rt.on_teleport(local, TeleportState::Failed, failure.place_id, "");
                rt.teleport_init_failed(local, failure);
            }
            _ => {}
        }
    }

    /// The client's own legacy teleports, for the server, by its ids.
    pub(super) fn teleport_requests(&self) -> Vec<ToServer> {
        let requests = self.client.runtime().take_teleport_requests();
        (requests.into_iter())
            .filter_map(|r| match r {
                Request::Teleport(r) => Some(r),
                Request::Lookup { .. } => None,
            })
            .filter_map(|r| {
                let players = (r.players.iter())
                    .map(|&p| self.sink.server_id(p))
                    .collect::<Option<Vec<_>>>()?;
                Some(ToServer::Teleport(TeleportRequest { players, ..r }))
            })
            .collect()
    }
}
