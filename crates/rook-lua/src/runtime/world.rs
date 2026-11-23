use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;

use crate::runtime::scheduler::{Clock, Scheduler};
use crate::runtime::signal::{ConnectionId, Signals};
use mlua::{Function, Table, Thread, Value as LuaValue};
use rook_dom::{Dom, InstanceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalBehavior {
    /// Handlers run when the signal fires (what `Default` means today).
    Immediate,
    /// Handlers run at the next resumption point.
    Deferred,
}

/// What a connection does when its signal fires.
#[derive(Debug, Clone)]
pub enum Handler {
    /// `Connect`/`Once`: call the function on a new thread.
    Call(Function),
    /// `Wait`: resume the waiting thread with the arguments.
    Resume(Thread),
    /// `ConnectParallel`/`BindToMessageParallel`: call the function on a
    /// new thread in the Actor's next parallel phase.
    Parallel { func: Function, actor: InstanceId },
}

/// How a resumed `wait` reports its result.
#[derive(Debug, Clone, Copy)]
pub enum WaitKind {
    /// `wait()`: returns elapsed time and `time()`.
    Legacy { since: f64 },
    /// `task.wait()`: returns elapsed time.
    Task { since: f64 },
    /// `delay`/`spawn`/`task.delay`: the job is a call, nothing to report.
    None,
}

#[derive(Debug, Clone)]
pub enum Job {
    Resume {
        thread: Thread,
        args: Vec<LuaValue>,
        wait: WaitKind,
    },
    /// Call a function on a new thread (`spawn`, `delay`, `task.delay` with a function).
    Call {
        func: Function,
        args: Vec<LuaValue>,
        /// `delay`/`spawn` pass elapsed time and `time()` to the function.
        legacy_since: Option<f64>,
    },
    /// Destroy an instance (`Debris:AddItem`).
    Destroy(InstanceId),
    /// A Deferred-mode signal handler.
    Fire {
        connection: ConnectionId,
        handler: Handler,
        args: Vec<LuaValue>,
    },
}

/// A script's code as it came compiled from Roblox's packages.
#[derive(Debug, Clone)]
pub struct Compiled {
    pub code: std::sync::Arc<[u8]>,
    /// Runs as Roblox's own code (the CoreScripts), reaching members game
    /// scripts can't. The PlayerModule runs as a game script, as on
    /// Roblox, where developers may replace it.
    pub core: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Output,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub kind: MessageType,
    pub message: String,
    /// `time()` when it was logged.
    pub time: f64,
    /// Wall-clock time when it was logged.
    pub wall: std::time::SystemTime,
    /// Where it came from, as Studio shows it: `Script:3` (the script's
    /// name and line).
    pub source: Option<String>,
}

/// `Workspace.Model.Script` and line 3 → `Script:3`.
pub(crate) fn short_source(chunk: &str, line: usize) -> String {
    let name = chunk.rsplit('.').next().unwrap_or(chunk);
    format!("{name}:{line}")
}

/// Input devices as the client sees them (UserInputService state).
#[derive(Debug, Default, Clone)]
pub struct InputState {
    /// Held keys, as `Enum.KeyCode` values.
    pub keys: FxHashSet<u32>,
    /// Mouse buttons 1-3 held.
    pub buttons: [bool; 3],
    /// Cursor position in pixels from the viewport's top left.
    pub mouse: (f32, f32),
    /// Cursor movement so far this frame: every motion event added up,
    /// back to zero when a client frame ends (`GetMouseDelta`).
    pub mouse_delta: (f32, f32),
    /// The local player's Mouse (`Player:GetMouse()`).
    pub mouse_object: Option<InstanceId>,
    /// The Mice `Equipped` passed for Tools the local character holds, by
    /// Tool: they act as the PlayerMouse does, and fire before it.
    pub tool_mice: Vec<(InstanceId, InstanceId)>,
    /// Gamepad1 to Gamepad8.
    pub gamepads: [crate::gamepads::Gamepad; crate::gamepads::GAMEPAD_SLOTS],
    /// What `HapticService:SetMotor` last set, by `UserInputType` value
    /// and motor, for `GetMotor` and the pads' motors: any input type
    /// keeps its values (Confirmed), and a slot keeps them while pads come
    /// and go (Inference).
    pub motor_values: FxHashMap<(u32, rook_types::VibrationMotor), Vec<crate::net::NetValue>>,
    /// What `GetLastInputType` answers; None until the first input, when
    /// it's Focus, as Studio's client starts.
    pub last_input_type: Option<rook_types::UserInputType>,
    /// The kind of pad `GetStringForKeyCode` and `GetImageForKeyCode`
    /// name keys after: the last one connected, kept after it goes.
    pub key_names: crate::gamepads::GamepadKind,
}

impl InputState {
    /// Drop what is kept about `id`, which is being freed.
    pub fn forget(&mut self, id: InstanceId) {
        for pad in &mut self.gamepads {
            pad.forget(id);
        }
        for values in self.motor_values.values_mut() {
            for v in values {
                v.map_instances(&|kept| (kept != id).then_some(kept));
            }
        }
    }

    /// Whether any gamepad is connected (`GamepadEnabled`).
    pub fn any_gamepad(&self) -> bool {
        self.gamepads.iter().any(|p| p.connected)
    }
}

/// A visual effect's or adornment's method a script called: what the
/// renderer applies besides properties.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectCall {
    Emit(u32),
    /// `ParticleEmitter:Clear()`, `Trail:Clear()`,
    /// `WireframeHandleAdornment:Clear()`.
    Clear,
    TextureOffset(f32),
    /// A line a WireframeHandleAdornment's `AddLine`, `AddLines` or
    /// `AddPath` added, in its CFrame's axes.
    Line(glam::Vec3, glam::Vec3),
}

/// What scripts did to a Sound that isn't a property (see
/// `api/methods/audio/sound.rs`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SoundControl {
    /// Where `Play` starts: the last `TimePosition` a script set, or 0
    /// after `Stop`.
    pub start: f64,
    /// Loops since the last `Play` (`DidLoop`'s count).
    pub loops: u32,
}

/// The audio API's script state (`api/methods/audio/audio_api.rs`).
#[derive(Debug, Default)]
pub struct AudioState {
    /// The graph's instances. Interior mutability, as `Dom`'s defaults
    /// cache: property reads, which borrow the world to read, bring it
    /// up to date.
    pub graph: std::cell::RefCell<crate::audio_api::GraphNodes>,
    /// AudioAnalyzers' last spectra, written by the engine on clients.
    pub spectra: FxHashMap<InstanceId, Vec<f32>>,
    /// `AudioPlayer:Play(atTime)` and `Stop(atTime)` calls waiting for
    /// their time.
    pub scheduled: Vec<Scheduled>,
    /// The last id a scheduled call returned.
    pub next_action: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scheduled {
    pub id: i64,
    pub player: InstanceId,
    pub play: bool,
    /// On the DataModel's clock (`SoundService:GetMixerTime`).
    pub at: f64,
}

/// Where 3D sounds are heard from (`SoundService:SetListener`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Listener {
    /// `Workspace.CurrentCamera`.
    #[default]
    Camera,
    CFrame(rook_types::CFrame),
    /// The part's position, facing as the camera does (Inference).
    ObjectPosition(InstanceId),
    ObjectCFrame(InstanceId),
}

/// A ModuleScript as one context loads it: each Actor's scripts get a copy
/// of their own (`runtime::actors`).
pub(crate) type ModuleKey = (crate::runtime::actors::Context, InstanceId);

/// ModuleScripts' `require` state.
#[derive(Debug, Default)]
pub(crate) struct ModuleState {
    /// `require` results.
    pub(crate) results: FxHashMap<ModuleKey, mlua::RegistryKey>,
    /// Modules currently loading, with the thread running each body: a
    /// require from that thread is a cycle, one from another thread waits.
    pub(crate) loading: FxHashMap<ModuleKey, mlua::Thread>,
    /// Threads waiting for a module another thread is loading.
    pub(crate) waiting: FxHashMap<ModuleKey, Vec<mlua::Thread>>,
}

impl ModuleState {
    /// Drop what is kept about `id`, a module or an Actor.
    fn forget(&mut self, id: InstanceId) {
        let other = |&(context, module): &ModuleKey| module != id && context != Some(id);
        self.results.retain(|k, _| other(k));
        self.loading.retain(|k, _| other(k));
        self.waiting.retain(|k, _| other(k));
    }
}

/// Everything a DataModel's scripts can reach.
#[derive(Debug)]
pub struct World {
    pub dom: Dom,
    pub signals: Signals<Handler>,
    pub scheduler: Scheduler<Job>,
    pub signal_behavior: SignalBehavior,
    /// Whether this DataModel is a client (RunService:IsClient).
    pub is_client: bool,
    /// Keyboard and mouse state (clients only).
    pub input: InputState,
    /// Remote calls waiting for the session to deliver them.
    pub outbox: Vec<crate::net::RemoteMessage>,
    /// Messages sent to Actors, for the next resumption point.
    pub messages: Vec<crate::runtime::actors::Message>,
    /// `SharedTableRegistry`'s tables, by name.
    pub(crate) shared_tables: FxHashMap<String, crate::types::shared_table::LSharedTable>,
    /// Tools a client's scripts moved between its player's Backpack and
    /// character, or dropped, for the server to repeat.
    pub(crate) tool_moves: Vec<crate::api::methods::tools::ToolMove>,
    /// Tool activations that fired here, for the other side to fire.
    pub(crate) tool_activations: Vec<crate::api::methods::tools::Activation>,
    /// ProximityPrompts shown and held here (clients only).
    pub(crate) prompts: crate::api::methods::prompts::Prompts,
    /// ContentProvider requests waiting for the engine to resolve them.
    pub content: crate::api::methods::content::preload::ContentRequests,
    pub assets: crate::api::methods::content::assets::AssetRequests,
    /// Web requests waiting for the engine to send them.
    pub web: crate::api::methods::web::requests::WebRequests,
    /// DataStoreService's stores, budgets and objects.
    pub(crate) data_stores: crate::data_stores::DataStores,
    /// What each TextFilterResult holds.
    pub(crate) filter_results: FxHashMap<InstanceId, crate::api::methods::web::filtering::Filtered>,
    /// Badges awarded in this session, as (user, badge), when no badges
    /// endpoint keeps them.
    pub(crate) badges_awarded: FxHashSet<(i64, i64)>,
    /// MemoryStoreService's sorted maps, queues and hash maps.
    pub(crate) memory_stores: crate::memory_stores::MemoryStores,
    /// What each Pages object holds.
    pub(crate) pages: FxHashMap<InstanceId, crate::api::methods::web::pages::Pages>,
    /// The physics world. Scripts query it (raycasts, regions), which
    /// brings it up to date with the DOM first, so it lives here with the
    /// DOM. Each side steps what it owns (`characters::ownership`).
    pub physics: rook_physics::Physics,
    /// GUI layout. Scripts read it (`AbsoluteSize` and the like), which
    /// lays out again first when GUI changed, so it lives here too.
    pub gui: rook_gui::Gui,
    pub(crate) text_bounds_waiting: crate::api::methods::gui::text::BoundsWaiting,
    /// Threads waiting for an invoke's reply, by call id.
    pub(crate) pending_calls: FxHashMap<u64, Thread>,
    pub(crate) next_call: u64,
    pub log: Vec<LogEntry>,
    /// Calls to Roblox members Rook has no implementation of, by `Class.Member`.
    pub unimplemented: BTreeMap<String, u64>,
    /// Calls to inert members (accepted and ignored), by `Class.Member`.
    pub inert: BTreeMap<String, u64>,
    /// Threads that ended with an error.
    pub errors: u64,
    pub(crate) modules: ModuleState,
    /// Luau bytecode for scripts that came without source (Roblox's own
    /// packages: see `rook_formats::compiled_scripts`), run in its place.
    pub bytecode: FxHashMap<InstanceId, Compiled>,
    /// The folder of CoreScripts `ScriptContext:AddCoreScriptLocal`
    /// starts from, outside the DataModel (clients running Roblox's
    /// CoreScripts).
    pub core_scripts: Option<InstanceId>,
    /// `UserSettings()`, outside the DataModel: the engine fills it from
    /// the player's saved settings (`crate::user_settings`).
    pub user_settings: Option<InstanceId>,
    /// Rook's own `RookOutputDevice` setting changed: it has no property
    /// for the journal to note (`crate::user_settings::set_output_device`).
    pub output_device_chosen: bool,
    /// A client's script called `game:Shutdown()` (the menu's Leave): its
    /// host closes it.
    pub shutdown: bool,
    /// The machine's output devices, as the window listed them when the
    /// game started; empty where nothing plays sound.
    pub output_devices: rook_audio::OutputDevices,
    /// Fast flags by full name (`FFlagX`, `DFIntY`), as Studio last
    /// downloaded them, for CoreScripts' `DefineFastFlag` and the like.
    pub fast_flags: std::sync::Arc<FxHashMap<String, String>>,
    /// Flags scripts defined that Studio has no value for, with their
    /// defaults.
    pub(crate) defined_flags: FxHashMap<String, String>,
    /// LocalizationTables' entries, parsed from `Contents` when first used.
    pub(crate) localization:
        FxHashMap<InstanceId, std::rc::Rc<Vec<crate::api::methods::localization::Entry>>>,
    /// Each Translator's table (`None`: the game's tables, for one from
    /// LocalizationService).
    pub(crate) translators: FxHashMap<InstanceId, Option<InstanceId>>,
    /// `Enum.CoreGuiType`s scripts turned off with `SetCoreGuiEnabled`.
    pub(crate) core_gui_disabled: FxHashSet<&'static str>,
    /// Movement modes registered with each PlayerScripts, by kind
    /// (`player_scripts::KINDS`), as enum values.
    pub(crate) movement_modes: FxHashMap<(InstanceId, usize), Vec<u32>>,
    pub(crate) styles: crate::styles::Styles,
    /// Running scripts and the environment each runs in, so they don't
    /// start twice and stopping one can find its threads.
    pub(crate) started: FxHashMap<InstanceId, Table>,
    /// Whether any script has been stopped, so threads are only checked for
    /// a stopped owner once there can be one.
    pub(crate) any_stopped: bool,
    /// Sounds' and AudioPlayers' script state, for those a script has
    /// played or seeked.
    pub sounds: FxHashMap<InstanceId, SoundControl>,
    pub listener: Listener,
    /// Sounds given to `SoundService:PlayLocalSound`, for the engine to
    /// play once.
    pub local_sounds: Vec<InstanceId>,
    pub audio: AudioState,
    /// Effect methods called since the renderer last took them, in order.
    pub effect_calls: Vec<(InstanceId, EffectCall)>,
    /// Players that have had a character, whose next spawns reset their
    /// PlayerGui.
    pub(crate) spawned: FxHashSet<InstanceId>,
    /// Builds R15 bodies from Roblox's rig file; the engine sets it on
    /// servers. Without it, characters are R6, which Rook builds itself.
    pub rigs: Option<crate::api::methods::characters::RigSource>,
    /// Players' looks from the avatar API; the rest have the default look.
    pub(crate) avatars: FxHashMap<InstanceId, crate::api::methods::characters::Avatar>,
    /// Players whose look is still on its way, whose first spawn waits.
    pub avatars_pending: FxHashSet<InstanceId>,
    /// Players whose clients simulate their characters (a server's).
    pub client_players: FxHashSet<InstanceId>,
    /// `Player:Kick` calls since the session last took them: each player
    /// and the message it's to be shown.
    pub(crate) kicks: Vec<(InstanceId, String)>,
    /// Teleports asked for and what players arrived with.
    pub(crate) teleports: crate::teleport::Teleports,
    /// What this client's server made it the owner of (a client's).
    pub owned_here: crate::OwnedHere,
    /// Picks spawns and spots on them.
    pub(crate) spawn_random: crate::types::random::LRandom,
    /// PlayerGuis' `SetTopbarTransparency`, for those a script set.
    pub(crate) topbar_transparency: FxHashMap<InstanceId, f32>,
    /// Humanoids' walks and disabled states.
    pub(crate) movement: crate::api::methods::characters::movement::Movement,
    /// Cameras moving by `Camera:Interpolate`.
    pub(crate) interpolations:
        FxHashMap<InstanceId, crate::api::methods::camera::moves::Interpolation>,
    /// Scriptable cameras' `SetRoll`, in radians, for those a script rolled.
    pub(crate) camera_rolls: FxHashMap<InstanceId, f32>,
}

/// SpawnLocation, which the DOM indexes so a respawn needn't walk
/// Workspace.
pub(crate) fn spawn_class() -> rook_reflection::ClassId {
    static CLASS: std::sync::LazyLock<rook_reflection::ClassId> = std::sync::LazyLock::new(|| {
        (rook_reflection::database().class_by_name("SpawnLocation"))
            .expect("SpawnLocation")
            .id
    });
    *CLASS
}

impl World {
    pub fn new(mut dom: Dom, clock: Clock) -> Self {
        dom.track_class(spawn_class());
        Self {
            dom,
            signals: Signals::default(),
            scheduler: Scheduler::new(clock),
            signal_behavior: SignalBehavior::Immediate,
            is_client: false,
            input: InputState::default(),
            outbox: Vec::new(),
            messages: Vec::new(),
            shared_tables: FxHashMap::default(),
            tool_moves: Vec::new(),
            tool_activations: Vec::new(),
            prompts: Default::default(),
            content: Default::default(),
            assets: Default::default(),
            web: Default::default(),
            data_stores: Default::default(),
            filter_results: FxHashMap::default(),
            badges_awarded: FxHashSet::default(),
            memory_stores: Default::default(),
            pages: FxHashMap::default(),
            physics: rook_physics::Physics::new(),
            gui: rook_gui::Gui::default(),
            text_bounds_waiting: Vec::new(),
            pending_calls: FxHashMap::default(),
            next_call: 0,
            log: Vec::new(),
            unimplemented: BTreeMap::new(),
            inert: BTreeMap::new(),
            errors: 0,
            modules: ModuleState::default(),
            bytecode: FxHashMap::default(),
            core_scripts: None,
            user_settings: None,
            output_device_chosen: false,
            shutdown: false,
            output_devices: rook_audio::OutputDevices::default(),
            fast_flags: Default::default(),
            defined_flags: FxHashMap::default(),
            localization: FxHashMap::default(),
            translators: FxHashMap::default(),
            core_gui_disabled: FxHashSet::default(),
            movement_modes: FxHashMap::default(),
            styles: Default::default(),
            started: FxHashMap::default(),
            any_stopped: false,
            sounds: FxHashMap::default(),
            listener: Listener::Camera,
            local_sounds: Vec::new(),
            audio: AudioState::default(),
            effect_calls: Vec::new(),
            spawned: FxHashSet::default(),
            rigs: None,
            avatars: FxHashMap::default(),
            avatars_pending: FxHashSet::default(),
            client_players: FxHashSet::default(),
            kicks: Vec::new(),
            teleports: Default::default(),
            owned_here: crate::OwnedHere::default(),
            // A fixed seed: runs repeat exactly.
            spawn_random: crate::types::random::LRandom::new(1),
            topbar_transparency: FxHashMap::default(),
            movement: Default::default(),
            interpolations: FxHashMap::default(),
            camera_rolls: FxHashMap::default(),
        }
    }

    pub fn push_log(&mut self, kind: MessageType, message: String) {
        self.push_log_from(kind, message, None);
    }

    pub fn push_log_from(&mut self, kind: MessageType, message: String, source: Option<String>) {
        let time = self.scheduler.now();
        self.log.push(LogEntry {
            kind,
            message,
            time,
            wall: std::time::SystemTime::now(),
            source,
        });
    }

    /// The instances the runtime holds outside the tree, as a script's
    /// handle does: what it still uses later (see `collect.rs`).
    pub(crate) fn held(&self) -> FxHashSet<InstanceId> {
        let listened = match self.listener {
            Listener::ObjectPosition(id) | Listener::ObjectCFrame(id) => Some(id),
            Listener::Camera | Listener::CFrame(_) => None,
        };
        self.translators
            .values()
            .flatten()
            .copied()
            .chain(listened)
            .chain(self.input.mouse_object)
            .chain(self.audio.scheduled.iter().map(|s| s.player))
            .chain(self.local_sounds.iter().copied())
            .chain(self.data_stores.objects.held())
            .chain(self.effect_calls.iter().map(|(id, _)| *id))
            .chain(self.input.tool_mice.iter().map(|&(_, mouse)| mouse))
            .chain(
                (self.tool_moves.iter())
                    .flat_map(|m| [Some(m.tool), Some(m.parent), m.grip].into_iter().flatten()),
            )
            .chain(self.tool_activations.iter().map(|a| a.tool))
            .chain(self.prompts.held())
            .chain(self.movement.held())
            .chain(self.teleports.held())
            .collect()
    }

    /// Drop what the runtime keeps about `id`, which is being freed.
    pub(crate) fn forget(&mut self, id: InstanceId) {
        self.filter_results.remove(&id);
        self.memory_stores.objects.remove(&id);
        self.pages.remove(&id);
        self.data_stores.objects.forget(id);
        self.prompts.forget(id);
        self.modules.forget(id);
        self.messages.retain(|m| m.actor != id);
        self.bytecode.remove(&id);
        self.localization.remove(&id);
        self.translators.remove(&id);
        self.movement_modes.retain(|(m, _), _| *m != id);
        self.started.remove(&id);
        self.sounds.remove(&id);
        self.audio.spectra.remove(&id);
        self.spawned.remove(&id);
        self.avatars.remove(&id);
        self.avatars_pending.remove(&id);
        self.client_players.remove(&id);
        self.teleports.forget(id);
        self.physics.forget_network_owner(id);
        self.topbar_transparency.remove(&id);
        self.interpolations.remove(&id);
        self.camera_rolls.remove(&id);
        self.movement.forget(id);
        self.styles.forget(id);
        self.input.forget(id);
    }

    /// The roll `camera` is drawn with (`Camera:SetRoll`), in radians.
    pub fn camera_roll(&self, camera: InstanceId) -> f32 {
        self.camera_rolls.get(&camera).copied().unwrap_or(0.0)
    }

    /// Look at the journal for what StyleSheets must style again, before
    /// it's trimmed.
    pub fn note_style_changes(&mut self) {
        self.styles.note_changes(&self.dom);
    }

    /// Bring the audio graph's instances up to date, before the journal
    /// is trimmed.
    pub fn note_audio_graph(&mut self) {
        self.audio.graph.get_mut().note_changes(&self.dom);
    }

    pub(crate) fn count_unimplemented(&mut self, what: String) {
        *self.unimplemented.entry(what).or_default() += 1;
    }

    pub fn started_count(&self) -> usize {
        self.started.len()
    }

    pub(crate) fn count_inert(&mut self, what: String) {
        *self.inert.entry(what).or_default() += 1;
    }
}
