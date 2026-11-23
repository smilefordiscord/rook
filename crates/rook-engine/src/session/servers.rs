//! Every running server of a universe in one thread, and the players
//! teleporting between them (docs/behavior/scripting/teleports.md): each
//! server is a hosted [`Session`] with its clients. After every server
//! has stepped, the [`Router`] routes the teleports their scripts asked
//! for against all of them, and each player it sends off is carried to its
//! destination's session, started from the universe's files if it must
//! be, as the player carries one to a new server thread.

use std::sync::Arc;

use crate::session::Session;
use crate::session::router::{Move, Router, To};
use crate::universe::routing::RunningServer;
use crate::universe::{ServerIdentity, Universe};
use crate::{DataModel, EngineError, Options};

#[derive(Debug)]
pub struct Servers {
    router: Router,
    options: Options,
    /// In the order they started.
    sessions: Vec<Session>,
}

impl Servers {
    pub fn new(universe: Arc<Universe>, options: Options) -> Self {
        Self {
            router: Router::new(universe),
            options,
            sessions: Vec::new(),
        }
    }

    pub fn universe(&self) -> &Universe {
        self.router.universe()
    }

    /// Start a public server of `place_id`, as a player joining the game
    /// there would. Returns its index.
    pub fn start(&mut self, place_id: i64) -> Result<usize, EngineError> {
        let identity = ServerIdentity::new_server(self.universe(), place_id, String::new());
        self.start_server(&identity)
    }

    fn start_server(&mut self, identity: &ServerIdentity) -> Result<usize, EngineError> {
        let bytes = (self.universe().place_bytes(identity.place_id)).map_err(|source| {
            EngineError::PlaceFile {
                place_id: identity.place_id,
                source,
            }
        })?;
        let mut server = DataModel::load_place(&bytes, &self.options)?;
        identity.apply(&server);
        server.start();
        let mut session = Session::new(server, self.options.clone());
        session.hosted = true;
        self.sessions.push(session);
        Ok(self.sessions.len() - 1)
    }

    /// A player joins server `server`; returns its client's index there.
    pub fn join(&mut self, server: usize, name: &str, user_id: i64) -> Result<usize, EngineError> {
        self.sessions
            .get_mut(server)
            .ok_or(EngineError::ServerStopped)?
            .join(name, user_id)
    }

    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    pub fn session(&self, server: usize) -> Option<&Session> {
        self.sessions.get(server)
    }

    pub fn session_mut(&mut self, server: usize) -> Option<&mut Session> {
        self.sessions.get_mut(server)
    }

    /// The server with this `JobId`.
    pub fn find(&self, job_id: &str) -> Option<usize> {
        (self.sessions.iter()).position(|s| ServerIdentity::read(s.server()).job_id == job_id)
    }

    /// Where the user's client is: its server and index there.
    pub fn client_of(&self, user_id: i64) -> Option<(usize, usize)> {
        self.sessions.iter().enumerate().find_map(|(server, s)| {
            let w = s.server().runtime().world();
            (0..s.clients.len()).find_map(|index| {
                let player = s.player(index)?;
                let user = w.dom.number_property(player, "UserId")? as i64;
                (user == user_id && s.client(index).is_some()).then_some((server, index))
            })
        })
    }

    /// One frame of every server: each steps, their teleports go, then
    /// changes reach the clients and the clients step.
    pub fn step(&mut self) {
        profiling::scope!("Servers::step");
        for session in &mut self.sessions {
            session.host_mut().step();
        }
        self.teleport();
        for session in &mut self.sessions {
            session.after_server_step();
            session.step_clients();
        }
    }

    /// Run frames until `seconds` have passed.
    pub fn run_for(&mut self, seconds: f64) {
        let frames = (seconds * self.options.hz).round() as u64;
        for _ in 0..frames {
            self.step();
        }
    }

    /// Route the teleports every server asked for and carry them out. A
    /// server whose last player left keeps running (Approximation: Roblox
    /// closes it once its `BindToClose` callbacks have run).
    fn teleport(&mut self) {
        let asked = (self.sessions.iter()).any(|s| s.server().runtime().wants_teleports());
        if !asked {
            return;
        }
        let mut running: Vec<RunningServer> =
            self.sessions.iter().map(|s| s.host().running()).collect();
        for from in 0..self.sessions.len() {
            let router = self.router.clone();
            for m in router.route(self.sessions[from].host_mut(), &mut running) {
                self.carry(from, &m);
            }
        }
    }

    /// Carry `m`, a move off server `from`, to its destination's session.
    fn carry(&mut self, from: usize, m: &Move) {
        let Some(carried) = self.sessions[from].send_off(&m.departure) else {
            return;
        };
        let d = &m.departure;
        let to = match &m.to {
            To::Same => Ok(from),
            To::Running(job) => self
                .find(job)
                .ok_or_else(|| "the server isn't running".to_owned()),
            To::Start(placement) => match self.find(&placement.identity.job_id) {
                Some(to) => Ok(to),
                None => (self.start_server(&placement.identity)).map_err(|e| e.to_string()),
            },
        };
        match to {
            Ok(to) if to == from => self.sessions[from].rejoin(d, carried),
            Ok(to) => {
                self.sessions[from].take_client(d.client);
                let arrived =
                    (self.sessions[to]).arrive(&d.name, d.user_id, d.join.clone(), carried);
                if let Err(e) = arrived {
                    tracing::warn!("{} couldn't join place {}: {e}", d.name, d.place_id);
                }
            }
            Err(e) => self.sessions[from].fail_move(d.client, &e),
        }
    }
}
