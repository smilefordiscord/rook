//! A play session in one thread: a server and the clients connected to it,
//! each message delivered as soon as it's sent (docs/design/architecture/replication.md#run-modes-and-replication-rook-net).
//! The ends are the same [`ServerHost`] and [`ClientHost`] a threaded host
//! runs apart. A session alone is a place outside any universe, whose
//! teleports find no place; [`servers::Servers`] holds the sessions of a
//! universe and moves players between them.

pub mod ended;
pub mod host;
pub mod server_thread;
pub mod servers;
pub mod threads;

use rook_dom::InstanceId;

use crate::session::host::{Carried, ClientHost, ServerHost};
use crate::{DataModel, EngineError, Options};

#[derive(Debug)]
pub struct Session {
    server: ServerHost,
    /// By index; `None` once that client has teleported away.
    clients: Vec<Option<ClientHost>>,
    options: Options,
    /// Held by a [`servers::Servers`], which routes its teleports.
    hosted: bool,
}

impl Session {
    /// Start a session around a server (its scripts already started).
    pub fn new(server: DataModel, options: Options) -> Self {
        Self {
            server: ServerHost::new(server),
            clients: Vec::new(),
            options,
            hosted: false,
        }
    }

    pub fn server(&self) -> &DataModel {
        self.server.server()
    }

    pub fn server_mut(&mut self) -> &mut DataModel {
        self.server.server_mut()
    }

    pub fn clients(&self) -> impl Iterator<Item = &DataModel> {
        self.clients.iter().flatten().map(ClientHost::client)
    }

    pub fn client(&self, index: usize) -> Option<&DataModel> {
        self.clients.get(index)?.as_ref().map(ClientHost::client)
    }

    pub fn client_mut(&mut self, index: usize) -> Option<&mut DataModel> {
        self.clients
            .get_mut(index)?
            .as_mut()
            .map(ClientHost::client_mut)
    }

    /// A player joins (see [`ServerHost::join`]); a new client DataModel
    /// receives the replicated game, its PlayerScripts (from
    /// StarterPlayerScripts), and starts its LocalScripts. Returns the
    /// client's index.
    pub fn join(&mut self, name: &str, user_id: i64) -> Result<usize, EngineError> {
        self.join_with(name, user_id, &[])
    }

    /// [`Session::join`], with the input devices `present` when the client
    /// starts (see [`ClientHost::new`]).
    pub fn join_with(
        &mut self,
        name: &str,
        user_id: i64,
        present: &[crate::InputEvent],
    ) -> Result<usize, EngineError> {
        let (index, joined) = self.server.join(name, user_id)?;
        self.clients
            .push(Some(ClientHost::new(&joined, &self.options, present)?));
        Ok(index)
    }

    /// A player arrives from a teleport: it joins with its join data, and
    /// its new client gets what the last one carried.
    pub(crate) fn arrive(
        &mut self,
        name: &str,
        user_id: i64,
        join: rook_lua::teleport::JoinData,
        carried: Carried,
    ) -> Result<usize, EngineError> {
        let (index, joined) = self.server.join_with(name, user_id, join)?;
        let client = ClientHost::arriving(&joined, &self.options, &[], Some(carried))?;
        self.clients.push(Some(client));
        Ok(index)
    }

    /// Client `index` leaves this session (its player has teleported).
    pub(crate) fn take_client(&mut self, index: usize) -> Option<ClientHost> {
        self.clients.get_mut(index)?.take()
    }

    pub(crate) fn host_mut(&mut self) -> &mut ServerHost {
        &mut self.server
    }

    pub(crate) fn host(&self) -> &ServerHost {
        &self.server
    }

    /// One frame: the server steps, its changes replicate to every client,
    /// then each client steps.
    pub fn step(&mut self) {
        profiling::scope!("Session::step");
        self.step_server();
        self.step_clients();
    }

    /// Each client's frame, then what they sent.
    pub(crate) fn step_clients(&mut self) {
        for client in self.clients.iter_mut().flatten() {
            client.step();
        }
        self.route();
    }

    /// One server frame, its changes delivered to the clients without
    /// stepping them: with [`Session::step_clients_by`], clients drawn at
    /// their own rate, as `rook-player` runs them.
    pub fn step_server(&mut self) {
        self.server.step();
        self.after_server_step();
    }

    /// What follows the server's frame: a session alone routes its
    /// teleports (finding no place, see [`Session`]), then messages and
    /// changes go to the clients.
    pub(crate) fn after_server_step(&mut self) {
        if !self.hosted {
            self.server.teleports_alone();
        }
        self.route();
        for (index, updates) in self.server.updates() {
            self.deliver(index, &updates);
        }
    }

    /// A message to client `index`, if it's still here.
    pub(crate) fn deliver(&mut self, index: usize, msg: &host::ToClient) {
        if let Some(Some(client)) = self.clients.get_mut(index) {
            client.deliver(msg);
        }
    }

    /// One frame of every client, `dt` seconds long.
    pub fn step_clients_by(&mut self, dt: f64) {
        for client in self.clients.iter_mut().flatten() {
            client.step_by(dt);
        }
        self.route();
    }

    /// Deliver queued remote calls in both directions. Messages arrive in
    /// the order they were sent; each crosses within the same session frame
    /// (Approximation: Roblox delivers on the next network tick).
    fn route(&mut self) {
        profiling::scope!("Session::route");
        for (index, msg) in self.server.outgoing() {
            self.deliver(index, &msg);
        }
        for index in 0..self.clients.len() {
            let Some(Some(client)) = self.clients.get_mut(index) else {
                continue;
            };
            for msg in client.outgoing() {
                self.server.deliver(index, msg);
            }
        }
    }

    /// Run frames until `seconds` of server time have passed.
    pub fn run_for(&mut self, seconds: f64) {
        let target = self.server().now() + seconds;
        while self.server().now() + 1e-9 < target {
            self.step();
        }
    }

    pub fn input(&mut self, index: usize, event: crate::InputEvent) {
        let Some(Some(client)) = self.clients.get_mut(index) else {
            return;
        };
        for msg in client.input(event) {
            self.server.deliver(index, msg);
        }
    }

    pub fn player(&self, index: usize) -> Option<InstanceId> {
        self.server.player(index)
    }
}
