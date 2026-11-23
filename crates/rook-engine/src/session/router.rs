//! The one place teleports are routed and players sent off
//! (docs/behavior/scripting/teleports.md). Every host asks its [`Router`]
//! after a server's step: a [`Session`](super::Session) alone, the
//! universe's [`Servers`](super::servers::Servers), a
//! [`ServerThread`](super::server_thread::ServerThread) and, through it,
//! the player, and Run mode. The router takes the server's requests,
//! routes them against the servers its host runs ([`routing::route`]),
//! answers lookups, fails what can't go, and sends each player who goes
//! off its server as a [`Move`]. A host only carries moves out: it gives
//! the departing client its messages ([`ClientHost::send_off`]), then has
//! the player join where the move says, its server started if it must be;
//! a move that can't finish ends the client with a prompt
//! ([`ClientHost::teleport_failed`]), whichever host it's in.

use std::sync::Arc;

use super::host::{ClientHost, Departure, ServerHost};
use super::server_thread::Placement;
use crate::universe::routing::{RunningServer, Target};
use crate::universe::{ServerIdentity, Universe};

/// Routes the teleports of the servers of one universe.
#[derive(Debug, Clone, Default)]
pub struct Router {
    universe: Arc<Universe>,
}

/// A player sent off its server, and where it goes.
#[derive(Debug)]
pub struct Move {
    pub departure: Departure,
    pub to: To,
}

/// Where a [`Move`] goes.
#[derive(Debug, Clone)]
pub enum To {
    /// The server it left: it joins that server again.
    Same,
    /// Another server the host runs, by `JobId`.
    Running(String),
    /// A server the host starts, unless an earlier move of the same frame
    /// started it (a group going together).
    Start(Placement),
}

impl Router {
    pub fn new(universe: Arc<Universe>) -> Self {
        Self { universe }
    }

    /// The router of a server outside any universe: every teleport fails
    /// as a game not found, and lookups find only its own players.
    pub fn alone() -> Self {
        Self::default()
    }

    pub fn universe(&self) -> &Arc<Universe> {
        &self.universe
    }

    /// Route what `source`'s scripts asked for against `running`, every
    /// server the host runs (`source` among them, as
    /// [`ServerHost::running`] gives them); servers and players the moves
    /// add are added to it, so a later source's teleports see them. Each
    /// player sent off has left `source` when this returns.
    pub fn route(&self, source: &mut ServerHost, running: &mut Vec<RunningServer>) -> Vec<Move> {
        if !source.server().runtime().wants_teleports() {
            return Vec::new();
        }
        let own = ServerIdentity::read(source.server()).job_id;
        let mut moves = Vec::new();
        for plan in source.teleports(&self.universe, running) {
            let to = match &plan.route.target {
                Target::Running(job) if *job == own => To::Same,
                Target::Running(job) => To::Running(job.clone()),
                Target::New(identity) => To::Start(Placement {
                    universe: self.universe.clone(),
                    identity: identity.clone(),
                }),
            };
            for departure in source.depart(plan) {
                let to = to.clone();
                moves.push(Move { departure, to });
            }
        }
        moves
    }

    /// [`Router::route`] for a host that runs `source` alone.
    pub fn route_alone(&self, source: &mut ServerHost) -> Vec<Move> {
        if !source.server().runtime().wants_teleports() {
            return Vec::new();
        }
        let mut running = vec![source.running()];
        self.route(source, &mut running)
    }
}

impl ClientHost {
    /// Its player left by teleport: it hears how the teleport went so far,
    /// and packs what it takes to its next client.
    pub fn send_off(&mut self, departure: &Departure) -> super::host::Carried {
        for msg in &departure.messages {
            self.deliver(msg);
        }
        self.carry(departure.teleport_data.clone())
    }

    /// The teleport couldn't finish after its player left the server: the
    /// client ends with a prompt saying why.
    pub fn teleport_failed(&mut self, why: &str) {
        tracing::warn!("teleport failed: {why}");
        (self.client_mut()).end_session(super::ended::teleport_failed_prompt(why));
    }
}
