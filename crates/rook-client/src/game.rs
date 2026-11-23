//! The game the player shows: a play session with one local player, whose
//! server runs on its own thread, or the server alone (Studio's Run mode),
//! which the window draws itself. A teleport starts the destination's
//! server on a new thread while the old client keeps running, then the
//! window switches to the new client (docs/behavior/scripting/teleports.md).

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;

#[cfg(feature = "repl")]
use rook_engine::console::repl::Side;
use rook_engine::session::host::{Carried, ClientHost, ServerHost};
use rook_engine::session::server_thread::{
    Joining, Placement, ServerThread, Started, Starting, Teleported,
};
use rook_engine::settings::user_settings::PlayerSettings;
use rook_engine::universe::Universe;
use rook_engine::universe::routing::Target;
use rook_engine::{DataModel, InputEvent, Options, console};
#[cfg(feature = "repl")]
use rook_lua::MessageType;
use rook_lua::teleport::JoinData;
use rook_render::Camera;

use crate::stats::Times;

/// Simulation step: 60 Hz, like Roblox's frame-based scheduler.
pub(crate) const STEP: f64 = 1.0 / 60.0;

#[derive(Debug)]
enum Mode {
    /// Play: the server on its thread, the local player's client here.
    Play {
        server: ServerThread,
        client: Box<ClientHost>,
    },
    /// Run: the server alone, here, since the window draws it.
    Run(Box<ServerHost>),
}

#[derive(Debug)]
pub struct Game {
    mode: Mode,
    pub(crate) saved_camera: Camera,
    /// Server and client step times, for `--bench` and the readout.
    pub(crate) server_steps: Times,
    pub(crate) client_steps: Times,
    /// Time spent applying server frames since the last client step.
    delivering: std::time::Duration,
    /// Run mode: real time the server hasn't stepped yet, in seconds.
    behind: f64,
    /// When loading the place started (`--bench` times content from it).
    pub(crate) load_start: std::time::Instant,
    /// The client runs Roblox's CoreScripts, whose menu Escape opens.
    core_menu: bool,
    pub(crate) name: String,
    /// What new clients and servers are made with.
    options: Options,
    universe: Arc<Universe>,
    /// The player's teleport, while its destination's server starts.
    teleport: Option<Teleporting>,
    /// The server steps by itself ([`Game::free_run`]), as a teleport's
    /// next one will.
    free_running: Cell<bool>,
}

/// A teleport on its way: the destination's server starting, and what the
/// old client carries to the new one.
#[derive(Debug)]
struct Teleporting {
    starting: Starting,
    carried: Carried,
}

/// A place on its way to being a [`Game`], for a window that keeps
/// drawing while it loads: [`Loading::poll`] it once a frame.
#[derive(Debug)]
pub struct Loading {
    stage: Stage,
    finish: Finish,
}

/// A place's file name, as the window and messages call it.
pub fn place_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[derive(Debug)]
enum Stage {
    /// Play: the server is loading the place on its thread.
    Server(Starting),
    /// Run: the place loads here on the next poll.
    Run(Vec<u8>),
}

/// What's needed once the server is there.
#[derive(Debug)]
struct Finish {
    options: Options,
    placement: Placement,
    script: Option<String>,
    client_script: Option<String>,
    load_start: std::time::Instant,
    name: String,
}

/// How every place the player opens is run, whichever place it is.
#[derive(Debug, Clone)]
pub struct Setup {
    /// A local player joins and the window shows that player's client, as
    /// in Studio's Play mode; otherwise the server alone (Run mode).
    pub play: bool,
    /// Where content requests resolve.
    pub content: Arc<rook_content::Resolver>,
    /// The `--script` file's source, run as a server script once the
    /// place starts.
    pub script: Option<String>,
    /// The `--client-script` file's source, run as a LocalScript once the
    /// player has joined (Play mode only).
    pub client_script: Option<String>,
    /// Sounds load in the background while the game runs, as in Roblox,
    /// unless this holds each frame until they have.
    pub wait_for_sounds: bool,
    pub core_scripts: Option<Arc<rook_engine::core_scripts::CorePatch>>,
    /// The player's settings file; `None` for runs that aren't played,
    /// whose changes aren't saved.
    pub user_settings: Option<PathBuf>,
    /// The local player's `LaunchData` (`Player:GetJoinData`), as a share
    /// link gives it.
    pub launch_data: Option<String>,
}

impl Loading {
    /// Start loading a place (see [`Game::load`]).
    pub fn begin(path: &Path, setup: &Setup) -> anyhow::Result<Self> {
        let load_start = std::time::Instant::now();
        let bytes = rook_platform::get().files.read(path)?;
        let options = Options {
            content: Some(setup.content.clone()),
            wait_for_sounds: setup.wait_for_sounds,
            core_scripts: setup.core_scripts.clone(),
            user_settings: setup.user_settings.clone(),
            ..Options::default()
        };
        let script = setup.script.clone();
        let placement = Placement::open(path)?;
        let (stage, script) = if setup.play {
            let player = Joining {
                name: "Player1".into(),
                user_id: 1,
                join: JoinData {
                    launch_data: setup.launch_data.clone(),
                    ..JoinData::default()
                },
            };
            let starting = ServerThread::spawn(
                bytes.into(),
                options.clone(),
                script,
                placement.clone(),
                Some(player),
            )?;
            (Stage::Server(starting), None)
        } else {
            (Stage::Run(bytes), script)
        };
        Ok(Self {
            stage,
            finish: Finish {
                options,
                placement,
                script,
                client_script: setup.client_script.clone(),
                load_start,
                name: place_name(path),
            },
        })
    }

    pub fn name(&self) -> &str {
        &self.finish.name
    }

    /// The game, once the place has loaded. Joining the client (Play) or
    /// loading the place (Run) happens in the poll that returns it, and
    /// `present` gives the input devices there as the client starts.
    pub fn poll(
        &mut self,
        present: impl FnOnce() -> Vec<InputEvent>,
    ) -> Option<anyhow::Result<Game>> {
        match &mut self.stage {
            Stage::Server(starting) => {
                let started = starting.poll()?;
                Some(
                    started
                        .map_err(Into::into)
                        .and_then(|s| self.finish.joined(s, &present())),
                )
            }
            Stage::Run(bytes) => Some(self.finish.run(bytes)),
        }
    }
}

impl Finish {
    fn joined(
        &self,
        (server, started): (ServerThread, Started),
        present: &[InputEvent],
    ) -> anyhow::Result<Game> {
        let joined = started.joined.context("the player didn't join")?;
        let client = ClientHost::new(&joined, &self.options, present)?;
        if let Some(source) = &self.client_script {
            client
                .client()
                .run_extra_script(source)
                .map_err(anyhow::Error::msg)?;
        }
        Ok(self.game(
            Mode::Play {
                server,
                client: Box::new(client),
            },
            rook_scene::initial_camera(started.camera),
        ))
    }

    fn run(&self, bytes: &[u8]) -> anyhow::Result<Game> {
        let mut server = DataModel::load_place(bytes, &self.options)?;
        self.placement.identity.apply(&server);
        server.start();
        let host = ServerHost::new(server);
        if let Some(source) = &self.script {
            host.server()
                .run_extra_script(source)
                .map_err(anyhow::Error::msg)?;
        }
        let camera = rook_scene::initial_camera(rook_engine::SavedCamera::read(host.server()));
        Ok(self.game(Mode::Run(Box::new(host)), camera))
    }

    fn game(&self, mode: Mode, saved_camera: Camera) -> Game {
        let core_menu = matches!(mode, Mode::Play { .. }) && self.options.core_scripts.is_some();
        let game = Game {
            core_menu,
            mode,
            saved_camera,
            server_steps: Times::default(),
            client_steps: Times::default(),
            delivering: std::time::Duration::ZERO,
            behind: 0.0,
            load_start: self.load_start,
            name: self.name.clone(),
            options: self.options.clone(),
            universe: self.placement.universe.clone(),
            teleport: None,
            free_running: Cell::new(false),
        };
        game.print_logs();
        game
    }
}

impl Game {
    /// Load a place and start it, waiting until it has.
    pub fn load(path: &Path, setup: &Setup) -> anyhow::Result<Self> {
        let loading = Loading::begin(path, setup)?;
        match loading.stage {
            Stage::Server(starting) => loading.finish.joined(starting.wait()?, &[]),
            Stage::Run(bytes) => loading.finish.run(&bytes),
        }
    }
    /// The DataModel the window shows.
    pub(crate) fn view(&self) -> &DataModel {
        match &self.mode {
            Mode::Play { client, .. } => client.client(),
            Mode::Run(host) => host.server(),
        }
    }

    pub(crate) fn view_mut(&mut self) -> &mut DataModel {
        match &mut self.mode {
            Mode::Play { client, .. } => client.client_mut(),
            Mode::Run(host) => host.server_mut(),
        }
    }

    /// Whether the window's DataModel has anything a mixer could play
    /// ([`DataModel::hears_anything`]).
    pub(crate) fn hears_anything(&self) -> bool {
        self.view().hears_anything()
    }

    /// What the window's DataModel is playing, for the mixer: its sounds,
    /// their SoundGroups, `SoundService.AmbientReverb` and the audio API's
    /// graph.
    pub(crate) fn sounds(&self) -> std::cell::Ref<'_, rook_audio::Mix<rook_dom::InstanceId>> {
        self.view().mix()
    }

    /// Send window input to the local player's client (Run mode has no player).
    pub(crate) fn input(&mut self, event: InputEvent) {
        if let Mode::Play { server, client } = &mut self.mode {
            for msg in client.input(event) {
                server.deliver(msg);
            }
        }
    }

    pub(crate) fn set_viewport(&mut self, width: u32, height: u32) {
        self.view_mut().set_viewport(width, height);
    }

    /// One frame of the whole game in lockstep: the server steps, its
    /// changes reach the client, then the client steps. Results are the
    /// same every run.
    pub(crate) fn step(&mut self) {
        self.arrive(true);
        let mut teleports = Vec::new();
        match &mut self.mode {
            Mode::Play { server, client } => {
                let frame = server.step();
                self.server_steps.push(frame.took);
                let start = std::time::Instant::now();
                for msg in &frame.messages {
                    client.deliver(msg);
                }
                teleports = frame.teleports;
                client.step();
                for msg in client.outgoing() {
                    server.deliver(msg);
                }
                self.client_steps.push(start.elapsed());
            }
            Mode::Run(host) => {
                let start = std::time::Instant::now();
                host.step();
                host.teleports_alone();
                host.updates();
                self.server_steps.push(start.elapsed());
            }
        }
        self.print_logs();
        for teleported in teleports {
            self.depart(teleported);
        }
    }

    /// Let the server keep its own 60 Hz pace (Play mode), for live play.
    pub(crate) fn free_run(&self) {
        if let Mode::Play { server, .. } = &self.mode {
            server.run(true);
            self.free_running.set(true);
        }
    }

    /// Live play, `dt` seconds after the last call: apply what the running
    /// server sent, then step the client once, `dt` long, as Roblox runs a
    /// client frame per drawn frame. In Run mode the server steps here at
    /// 60 Hz, catching up at most four steps.
    pub(crate) fn advance(&mut self, dt: f64) {
        self.arrive(false);
        let mut teleports = Vec::new();
        match &mut self.mode {
            Mode::Play { server, client } => {
                let start = std::time::Instant::now();
                for frame in server.frames() {
                    self.server_steps.push(frame.took);
                    for msg in &frame.messages {
                        client.deliver(msg);
                    }
                    teleports.extend(frame.teleports);
                }
                // Applying the server's changes counts towards the client's
                // next step, as it does in lockstep.
                self.delivering += start.elapsed();
                let start = std::time::Instant::now();
                client.step_by(dt);
                for msg in client.outgoing() {
                    server.deliver(msg);
                }
                let took = start.elapsed() + std::mem::take(&mut self.delivering);
                self.client_steps.push(took);
                self.print_logs();
                for teleported in teleports {
                    self.depart(teleported);
                }
            }
            Mode::Run(_) => {
                self.behind += dt;
                let mut steps = 0;
                while self.behind >= STEP && steps < 4 {
                    self.behind -= STEP;
                    steps += 1;
                    self.step();
                }
                if steps == 4 {
                    self.behind = 0.0;
                }
            }
        }
    }

    /// The local player left its server by teleport: its client hears so
    /// and packs what it carries, then joins the server it rejoined, or
    /// keeps running while the destination's server starts.
    fn depart(&mut self, teleported: Teleported) {
        let Mode::Play { client, .. } = &mut self.mode else {
            return;
        };
        let departure = teleported.departure;
        for msg in &departure.messages {
            client.deliver(msg);
        }
        let carried = client.carry(departure.teleport_data.clone());
        let started = match (teleported.joined, departure.target) {
            (Some(joined), _) => {
                match ClientHost::arriving(&joined, &self.options, &[], Some(carried)) {
                    Ok(arrived) => {
                        for entry in client.client().take_log() {
                            console::print_entry("Client", &entry);
                        }
                        **client = arrived;
                        return;
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
            (None, Target::New(identity)) => {
                let place = self.universe.place_bytes(identity.place_id);
                let placement = Placement {
                    universe: self.universe.clone(),
                    identity,
                };
                let player = Joining {
                    name: departure.name,
                    user_id: departure.user_id,
                    join: departure.join,
                };
                (place.map_err(|e| e.to_string()))
                    .and_then(|place| {
                        let options = self.options.clone();
                        ServerThread::spawn(place, options, None, placement, Some(player))
                            .map_err(|e| e.to_string())
                    })
                    .map(|starting| self.teleport = Some(Teleporting { starting, carried }))
            }
            (None, Target::Running(_)) => Err("the server isn't running".to_owned()),
        };
        if let Err(e) = started {
            self.teleport_failed(&e);
        }
    }

    /// The teleport's destination has started (waiting for it with
    /// `wait`): the window switches to the new client, and the old server
    /// closes.
    fn arrive(&mut self, wait: bool) {
        let Some(teleport) = &mut self.teleport else {
            return;
        };
        let started = if wait {
            let Some(teleport) = self.teleport.take() else {
                return;
            };
            Some((teleport.starting.wait(), teleport.carried))
        } else {
            match teleport.starting.poll() {
                Some(started) => self.teleport.take().map(|t| (started, t.carried)),
                None => None,
            }
        };
        let Some((started, carried)) = started else {
            return;
        };
        let arrived = (started.map_err(|e| e.to_string())).and_then(|(server, started)| {
            let joined = (started.joined).ok_or_else(|| "the player didn't join".to_owned())?;
            let client = ClientHost::arriving(&joined, &self.options, &[], Some(carried))
                .map_err(|e| e.to_string())?;
            Ok((server, client))
        });
        match arrived {
            Ok((server, client)) => {
                self.print_logs();
                if self.free_running.get() {
                    server.run(true);
                }
                self.mode = Mode::Play {
                    server,
                    client: Box::new(client),
                };
            }
            Err(e) => self.teleport_failed(&e),
        }
    }

    /// The teleport couldn't finish: the player has left its old server,
    /// so its client ends there with a prompt saying why.
    fn teleport_failed(&mut self, why: &str) {
        tracing::warn!("teleport failed: {why}");
        self.view_mut()
            .end_session(rook_engine::session::ended::teleport_failed_prompt(why));
    }

    pub(crate) fn run_for(&mut self, seconds: f64) {
        let target = self.view().now() + seconds;
        while self.view().now() + 1e-9 < target {
            self.step();
        }
    }

    /// Run a command-bar entry on the server or the client.
    #[cfg(feature = "repl")]
    pub(crate) fn repl(&self, side: Side, line: &str) {
        let dm = match (&self.mode, side) {
            (Mode::Play { server, .. }, Side::Server) => {
                server.repl(line);
                return;
            }
            (Mode::Play { client, .. }, Side::Client) => client.client(),
            (Mode::Run(host), _) => host.server(),
        };
        if let Err(e) = rook_engine::console::repl::run_line(dm, line) {
            console::print_message(MessageType::Error, &e);
        }
    }

    /// Whether GUI took the last input event, so the window shouldn't use
    /// it too.
    pub fn input_processed(&self) -> bool {
        match &self.mode {
            Mode::Play { client, .. } => client.client().input_processed(),
            Mode::Run(_) => false,
        }
    }

    /// Whether the client runs Roblox's CoreScripts, whose menu Escape
    /// opens.
    pub fn core_menu(&self) -> bool {
        self.core_menu
    }

    /// Whether keys are typing into a TextBox.
    pub fn typing(&self) -> bool {
        match &self.mode {
            Mode::Play { client, .. } => client.client().focused_text_box().is_some(),
            Mode::Run(_) => false,
        }
    }

    /// The player's settings as the local player's client has them (Run
    /// mode has no player to change them).
    pub fn player_settings(&self) -> Option<&PlayerSettings> {
        match &self.mode {
            Mode::Play { client, .. } => client.client().player_settings(),
            Mode::Run(_) => None,
        }
    }

    /// Save the player's settings if they changed, before exiting.
    pub fn save_user_settings(&mut self) {
        if let Mode::Play { client, .. } = &mut self.mode {
            client.client_mut().save_user_settings();
        }
    }

    /// Whether the player picked Leave on the prompt shown when the
    /// server ended the connection (a kick): the window closes.
    pub(crate) fn left(&self) -> bool {
        self.view().left()
    }

    pub(crate) fn has_client(&self) -> bool {
        matches!(self.mode, Mode::Play { .. })
    }

    /// Print new output from this thread's DataModel (the server thread
    /// prints its own).
    pub(crate) fn print_logs(&self) {
        let side = if self.has_client() {
            "Client"
        } else {
            "Server"
        };
        for entry in self.view().take_log() {
            console::print_entry(side, &entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A place file whose ServerScriptService runs `source`, with
    /// characters off.
    fn place(source: &str) -> String {
        format!(
            r#"<roblox version="4">
<Item class="Players" referent="P"><Properties><string name="Name">Players</string><bool name="CharacterAutoLoads">false</bool></Properties></Item>
<Item class="ServerScriptService" referent="S"><Properties><string name="Name">ServerScriptService</string></Properties>
<Item class="Script" referent="A"><Properties><string name="Name">Main</string><ProtectedString name="Source"><![CDATA[{source}]]></ProtectedString></Properties></Item>
</Item>
</roblox>"#
        )
    }

    /// The window's game follows its player to the place a teleport sends
    /// it to, on a new server thread.
    #[test]
    fn the_game_follows_its_player_to_another_place() {
        let dir = std::env::temp_dir()
            .join("rook-client-tests")
            .join(format!("teleport-{}", std::process::id()));
        let files = &rook_platform::get().files;
        files.create_dir_all(&dir).unwrap();
        let lobby = place(
            r#"game.Players.PlayerAdded:Connect(function(p)
                task.wait(0.1)
                local options = Instance.new("TeleportOptions")
                options:SetTeleportData({ x = 1 })
                game:GetService("TeleportService"):TeleportAsync(1002, { p }, options)
            end)"#,
        );
        let manifest = "game_id = 9\n[places]\n1001 = \"Lobby.rbxlx\"\n1002 = \"Arena.rbxlx\"\n";
        let written = [
            ("Lobby.rbxlx", lobby),
            ("Arena.rbxlx", place("")),
            ("universe.toml", manifest.to_owned()),
        ];
        for (name, text) in &written {
            files.write(&dir.join(name), text.as_bytes()).unwrap();
        }
        let setup = Setup {
            play: true,
            content: Arc::new(rook_content::Resolver::new(
                rook_engine::settings::content_config(false),
            )),
            script: None,
            client_script: None,
            wait_for_sounds: true,
            core_scripts: None,
            user_settings: None,
            launch_data: Some("hello".into()),
        };
        let mut game = Game::load(&dir.join("Lobby.rbxlx"), &setup).unwrap();
        let place_id = |game: &Game| {
            let w = game.view().runtime().world();
            w.dom.number_property(w.dom.root(), "PlaceId")
        };
        assert_eq!(place_id(&game), Some(1001.0));
        game.run_for(0.3);
        game.step();
        assert_eq!(place_id(&game), Some(1002.0));
        let data = game.view().run_extra_script(
            "print(game:GetService('TeleportService'):GetLocalPlayerTeleportData().x)",
        );
        assert!(data.is_ok());
        let out: Vec<String> = game
            .view()
            .take_log()
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert_eq!(out, ["1"]);
        for (name, _) in &written {
            // Clean-up only: a file left in the temp folder is harmless.
            let _ = files.remove_file(&dir.join(name));
        }
    }
}
