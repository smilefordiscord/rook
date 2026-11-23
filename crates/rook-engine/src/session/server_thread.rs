//! The server on its own thread, so a window waits only on its client and
//! the renderer, and a dedicated server keeps its 60 Hz pace
//! (docs/design/architecture/threads-and-frames.md#threading-and-data-flow). The server's DataModel is
//! made on the thread and never leaves it; the rest of the app talks to it
//! in messages.
//!
//! The thread steps when told to ([`ServerThread::step`], which keeps a
//! client in lockstep with it, so `--seconds`, `--bench` and screenshots
//! give the same results every run), or by itself at 60 Hz of real time
//! ([`ServerThread::run`]), as a server does for players.
//!
//! The thread is one server of a universe, whose teleports the shared
//! [`Router`] routes against itself: a player sent elsewhere leaves in a
//! [`Teleported`] the app carries out by starting that server's thread,
//! which reads and loads its place (docs/behavior/scripting/teleports.md).

use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};

use rook_lua::teleport::JoinData;

#[cfg(feature = "repl")]
use rook_lua::MessageType;

use crate::session::host::{Joined, ServerHost, ToClient, ToServer};
use crate::session::router::{Move, Router, To};
use crate::universe::{PlaceSource, ServerIdentity, Universe};
use crate::{DataModel, EngineError, Options, SavedCamera, console};

#[derive(Debug)]
pub struct Frame {
    pub messages: Vec<ToClient>,
    /// How long the server took to step.
    pub took: Duration,
    /// The player teleported this frame, after `messages`.
    pub teleports: Vec<Teleported>,
}

/// The player left its server by teleport.
#[derive(Debug)]
pub struct Teleported {
    pub sent: Move,
    /// For a teleport back into this same server ([`To::Same`]), how the
    /// player joined it again; otherwise the app carries the move out.
    pub joined: Option<Joined>,
}

/// Which server a thread runs, and the universe it's in.
#[derive(Debug, Clone)]
pub struct Placement {
    pub universe: Arc<Universe>,
    pub identity: ServerIdentity,
}

impl Placement {
    /// A public server of the place file at `place`, in the universe its
    /// folder's manifest describes.
    pub fn open(place: &std::path::Path) -> Result<Self, crate::universe::ManifestError> {
        let (universe, place_id) = Universe::open(place)?;
        let universe = Arc::new(universe);
        let identity = ServerIdentity::new_server(&universe, place_id, String::new());
        Ok(Self { universe, identity })
    }
}

/// A player joining as the thread's server starts.
#[derive(Debug, Clone)]
pub struct Joining {
    pub name: String,
    pub user_id: i64,
    pub join: JoinData,
}

#[derive(Debug)]
enum Command {
    Step,
    /// Step at 60 Hz by itself, or stop doing so.
    Run(bool),
    Deliver(ToServer),
    #[cfg(feature = "repl")]
    Repl(String),
}

/// What the server sends back once it's running.
#[derive(Debug)]
pub struct Started {
    pub joined: Option<Joined>,
    /// The camera the place was saved with.
    pub camera: Option<SavedCamera>,
}

#[derive(Debug)]
pub struct ServerThread {
    commands: Sender<Command>,
    frames: Receiver<Frame>,
    /// Closed once the thread has ended, its DataModel dropped.
    ended: Receiver<()>,
}

/// How far behind real time a running server may fall before it stops
/// catching up and drops the lost time (as the window's client does).
const MAX_LAG: Duration = Duration::from_millis(250);

/// A server thread still loading its place: [`Starting::poll`] it each
/// frame, or [`Starting::wait`].
#[derive(Debug)]
pub struct Starting {
    server: Option<ServerThread>,
    started: Receiver<Result<Started, EngineError>>,
}

impl Starting {
    /// The server, once it has loaded and its player has joined.
    pub fn poll(&mut self) -> Option<Result<(ServerThread, Started), EngineError>> {
        let started = match self.started.try_recv() {
            Ok(started) => started,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(EngineError::ServerStopped),
        };
        Some(started.map(|s| (self.server.take().expect("polled after it started"), s)))
    }

    pub fn wait(mut self) -> Result<(ServerThread, Started), EngineError> {
        let started = self
            .started
            .recv()
            .map_err(|_| EngineError::ServerStopped)??;
        Ok((self.server.take().expect("waited once"), started))
    }
}

impl ServerThread {
    /// Read and load `place` on a new thread as the server `placement`
    /// names, start its scripts, run `script` (the `--script` file) and let
    /// `player` join, if given.
    pub fn spawn(
        place: PlaceSource,
        options: Options,
        script: Option<String>,
        placement: Placement,
        player: Option<Joining>,
    ) -> Result<Starting, EngineError> {
        let (commands, command_rx) = channel();
        let (frame_tx, frames) = channel();
        let (started_tx, started_rx) = channel();
        let (ended_tx, ended) = channel::<()>();
        let body = move || {
            // Dropped as the thread ends, however it ends.
            let _ended = ended_tx;
            let host = (|| -> Result<Hosted, EngineError> {
                let place_id = placement.identity.place_id;
                let bytes =
                    (place.read()).map_err(|source| EngineError::PlaceFile { place_id, source })?;
                let mut server = DataModel::load_place(&bytes, &options)?;
                placement.identity.apply(&server);
                server.start();
                let mut host = ServerHost::new(server);
                let camera = SavedCamera::read(host.server());
                let (client, joined) = match player {
                    Some(p) => {
                        let (index, joined) = host.join_with(&p.name, p.user_id, p.join)?;
                        (index, Some(joined))
                    }
                    None => (0, None),
                };
                if let Some(source) = &script {
                    host.server()
                        .run_extra_script(source)
                        .map_err(EngineError::Script)?;
                }
                print_logs(host.server());
                // The receiver is gone only if the game closed while
                // starting.
                let _ = started_tx.send(Ok(Started { joined, camera }));
                Ok(Hosted {
                    host,
                    router: Router::new(placement.universe),
                    client,
                })
            })();
            match host {
                Ok(host) => serve(host, &command_rx, &frame_tx),
                Err(e) => {
                    let _ = started_tx.send(Err(e));
                }
            }
        };
        rook_platform::get()
            .threads
            .spawn("server", Box::new(body))?;
        Ok(Starting {
            server: Some(Self {
                commands,
                frames,
                ended,
            }),
            started: started_rx,
        })
    }

    /// Send the server thread a command. It is gone only if it failed,
    /// which the game has reported, or the game is closing.
    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Step the server once and wait for its frame.
    pub fn step(&self) -> Frame {
        self.send(Command::Step);
        self.frames.recv().unwrap_or(Frame {
            messages: Vec::new(),
            took: Duration::ZERO,
            teleports: Vec::new(),
        })
    }

    /// Let the server step by itself at 60 Hz (`true`), or only when told.
    pub fn run(&self, on: bool) {
        self.send(Command::Run(on));
    }

    /// Frames the running server has finished since the last call.
    pub fn frames(&self) -> impl Iterator<Item = Frame> + '_ {
        self.frames.try_iter()
    }

    pub fn deliver(&self, msg: ToServer) {
        self.send(Command::Deliver(msg));
    }

    #[cfg(feature = "repl")]
    pub fn repl(&self, line: &str) {
        self.send(Command::Repl(line.to_owned()));
    }

    /// Let the thread end without waiting for it: [`ServerThread::ended`]
    /// says when it has, and dropping it then doesn't wait.
    pub fn close(&mut self) {
        let (dead, _) = channel();
        self.commands = dead;
    }

    /// Whether the thread has ended, its DataModel dropped.
    pub fn ended(&self) -> bool {
        matches!(self.ended.try_recv(), Err(TryRecvError::Disconnected))
    }
}

impl Drop for ServerThread {
    fn drop(&mut self) {
        // Closing the command channel ends the thread; wait until it has,
        // so its DataModel is gone (and its last output printed) when this
        // returns. A panic was reported where it happened.
        self.close();
        let _ = self.ended.recv();
    }
}

fn print_logs(server: &DataModel) {
    for entry in server.take_log() {
        console::print_entry("Server", &entry);
    }
}

/// The thread's server, its universe's router, and the index of the
/// app's client.
struct Hosted {
    host: ServerHost,
    router: Router,
    client: usize,
}

/// The server thread's loop, until the app lets it go.
fn serve(mut host: Hosted, commands: &Receiver<Command>, frames: &Sender<Frame>) {
    let step = Duration::from_secs_f64(1.0 / 60.0);
    let mut running = false;
    let mut next = Instant::now();
    loop {
        // Everything the client sent goes in before the next step, even
        // when the server is behind and stepping back to back.
        loop {
            match commands.try_recv() {
                Ok(command) => {
                    if !obey(&mut host, command, frames, &mut running, &mut next) {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let command = if running {
            let now = Instant::now();
            if now >= next {
                if frames.send(frame(&mut host)).is_err() {
                    return;
                }
                next += step;
                if now.duration_since(next) > MAX_LAG {
                    next = now;
                }
                continue;
            }
            match commands.recv_timeout(next - now) {
                Ok(c) => c,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match commands.recv() {
                Ok(c) => c,
                Err(_) => return,
            }
        };
        if !obey(&mut host, command, frames, &mut running, &mut next) {
            return;
        }
    }
}

/// Carry out one command. Returns false once the app is gone.
fn obey(
    host: &mut Hosted,
    command: Command,
    frames: &Sender<Frame>,
    running: &mut bool,
    next: &mut Instant,
) -> bool {
    match command {
        Command::Step => return frames.send(frame(host)).is_ok(),
        Command::Run(on) => {
            *running = on;
            *next = Instant::now();
        }
        Command::Deliver(msg) => host.host.deliver(host.client, msg),
        #[cfg(feature = "repl")]
        Command::Repl(line) => {
            if let Err(e) = crate::console::repl::run_line(host.host.server(), &line) {
                console::print_message(MessageType::Error, &e);
            }
            print_logs(host.host.server());
        }
    }
    true
}

/// One server frame, with what the client should receive.
fn frame(hosted: &mut Hosted) -> Frame {
    let start = Instant::now();
    hosted.host.step();
    let teleports = teleport(hosted);
    let host = &mut hosted.host;
    let mut messages: Vec<ToClient> = host.outgoing().into_iter().map(|(_, m)| m).collect();
    messages.extend(host.updates().into_iter().map(|(_, m)| m));
    print_logs(host.server());
    Frame {
        messages,
        took: start.elapsed(),
        teleports,
    }
}

/// The moves the router sends off this server, which routes against itself
/// alone: a player sent into it again joins it again here; any other
/// leaves for the app to carry out.
fn teleport(hosted: &mut Hosted) -> Vec<Teleported> {
    let host = &mut hosted.host;
    let mut out = Vec::new();
    for m in hosted.router.route_alone(host) {
        let joined = match m.to {
            To::Same => {
                let d = &m.departure;
                match host.join_with(&d.name, d.user_id, d.join.clone()) {
                    Ok((index, joined)) => {
                        hosted.client = index;
                        Some(joined)
                    }
                    Err(e) => {
                        tracing::warn!("{} couldn't join again: {e}", d.name);
                        None
                    }
                }
            }
            To::Running(_) | To::Start(_) => None,
        };
        out.push(Teleported { sent: m, joined });
    }
    out
}
