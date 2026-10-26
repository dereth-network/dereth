//! Driving a real [`App`] from a parsed script.
//!
//! The client this module builds is the runtime's application with nothing plugged into it:
//! [`HeadlessShell`] for the front end (no UI, only the game's questions and pop-ups kept for the
//! script), `NullPresentation` for the device, `NullWindow` for the window, `FixedStepClock` for
//! the clock. There is no window, no graphics device and no UI; a socket is opened only to the
//! server [`Options::connect`] names, and no data file is read beyond the retail dats
//! [`Options::dat_dir`] names and the plugins' folders.
//!
//! Everything this module prints goes through one `Write`, one line per fact, so the whole run is
//! a text file a test can compare.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use dereth_client_sdk::actions::{Action, ActionId};
use dereth_client_sdk::contract::{GameView, UiRequest};
use dereth_client_sdk::net::client_session::SessionState;
use dereth_client_sdk::primitives::{LocalTime, ObjectId};
use dereth_client_sdk::runtime::app::{App, Platform, HEADLESS_STEP};
use dereth_client_sdk::runtime::config::Config;
use dereth_client_sdk::runtime::frame::FrameStep;
use dereth_client_sdk::runtime::frame_events::{FrameEvent, FrameEvents};
use dereth_client_sdk::runtime::net::ClientNetwork;
use dereth_client_sdk::runtime::present::{NullPresentation, Presentation};
use dereth_client_sdk::runtime::scene::SceneConfig;
use dereth_client_sdk::runtime::sim_present::SimPresentation;

/// What the client names itself when the game asks (`@version`): this program and Dereth's
/// version, which it carries.
pub const BUILD_ID: &str = concat!("dereth-headless ", env!("CARGO_PKG_VERSION"));

// `peer(pair)` is the address scheme every capture-replaying harness uses; it lives with the reader.
use crate::capture::{self, peer, CaptureError, Datagram};
use crate::dialogs::HeadlessShell;
use crate::script::{action_name, Command, Direction, Dump, PluginCommand, UiCommand};
use dereth_client_sdk::plugins::{MemoryStorage, PluginHook, PluginHost, PluginStorage, UiInput};

/// Where the run reads from and how big its imaginary back buffer is.
#[derive(Debug, Clone)]
pub struct Options {
    /// The retail dats, and nothing else this run reads: `--dat-dir`, else the first of the
    /// working directory and the executable's directory that holds them.
    pub dat_dir: PathBuf,
    /// `--classic-dat-dir`: where the files from before Throne of Destiny are when they are not
    /// beside [`Self::dat_dir`]'s.
    pub classic_dat_dir: Option<PathBuf>,
    /// `--era`: the era the world plays, which by default chooses the set that draws it.
    pub era: Option<dereth_client_sdk::primitives::EraId>,
    /// `--world-base`: the set that draws the world, over the era's.
    pub world_base: Option<dereth_client_sdk::primitives::ContainerEra>,
    /// `fixtures/packet-captures/`, the directory `login <session>` resolves a slug in.
    pub captures_dir: PathBuf,
    /// The null presentation's extent, which is what the UI shell lays out against.
    pub width: u32,
    pub height: u32,
    /// Where `Config::preferences_file` points. **Never the user's own**: the default is a path under
    /// the temp directory that this run does not create; a settings folder named for the run
    /// ([`settings_preferences`]) holds it instead.
    pub preferences_file: PathBuf,
    /// The account name the replay endpoint is built with. Only the recorded server's own bytes
    /// drive the login FSM, so this names the endpoint and nothing else.
    pub account: String,
    /// The most frames one `login` may run before it gives up, so a recording that never reaches
    /// character select ends with an error rather than a hang.
    pub max_login_frames: u64,
    /// `--world`: build and drive the world with no device (the runtime's `SimPresentation`), so
    /// entering it over a recording, or `world <landblock>`, stands a body there that actions
    /// move. Off, the presentation is the null one and there is no world, as before.
    pub world: bool,
    /// `--plugins <dir>`: plugin packages loaded at the first frame (each a package folder or a
    /// folder of them). With none, the plugin host is installed by the first `plugin` command.
    pub plugins: Vec<PathBuf>,
    /// `--plugin-store <dir>`: where plugin stores are kept; in memory when absent.
    pub plugin_store: Option<PathBuf>,
    /// `--plugin-data <dir>`: the plugins' data folders; none when absent.
    pub plugin_data: Option<PathBuf>,
    /// `--plugin-policy <file>`: the world's plugin policy document, as its world entry publishes
    /// it; absent, the policy of a world that publishes none.
    pub plugin_policy: Option<String>,
    /// `--peers <dir>`: the bus between the player's clients reaches the other headless clients
    /// that name the same folder, by loopback datagrams; absent, a bus with no one else on it.
    pub peers: Option<PathBuf>,
    /// `--connect <host:port>`: log on to a server instead of replaying, as `--account` with
    /// `--password`; the script's `enter` then takes a character into the world.
    pub connect: Option<String>,
    pub password: String,
    /// `--nav-published <file>`: the server's published portal table, read at start and taken
    /// into the client's journeys as the server's knowledge.
    pub nav_published: Option<PathBuf>,
    /// The time a plugin's entry may take before it is stopped, in milliseconds; `None` for the
    /// host's own. A test that runs a large plugin on a busy machine raises it, so that the
    /// plugin's work, not the machine's speed, decides what happens.
    pub plugin_call_budget_ms: Option<f64>,
    /// Set (by Ctrl+C) to stop the client: `run forever` and the daemon end at the next frame,
    /// and the client logs out as it shuts down.
    pub stop: Arc<AtomicBool>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            dat_dir: dereth_client_sdk::runtime::config::default_dat_dir(),
            classic_dat_dir: None,
            era: None,
            world_base: None,
            captures_dir: default_captures_dir(),
            width: 800,
            height: 600,
            preferences_file: std::env::temp_dir()
                .join("dereth-headless-not-created")
                .join("prefs.ini"),
            account: "dereth-headless".to_owned(),
            max_login_frames: 4_000,
            world: false,
            plugins: Vec::new(),
            plugin_store: None,
            plugin_data: None,
            plugin_policy: None,
            peers: None,
            connect: None,
            password: String::new(),
            nav_published: None,
            plugin_call_budget_ms: None,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// `fixtures/packet-captures/` of **this** checkout, found from the crate's own directory so that a run in a
/// git worktree reads that worktree's corpus and not another tree's.
/// Each recording is `fixtures/packet-captures/<slug>.jsonl`.
#[must_use]
pub fn default_captures_dir() -> PathBuf {
    // The workspace root is two levels above this crate.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/packet-captures")
}

/// Anything that stops a run.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("the client would not start: {0}")]
    Startup(String),
    #[error(transparent)]
    Capture(#[from] CaptureError),
    #[error("{0}")]
    Net(String),
    #[error("login {session}: the replay ran {frames} frames and reached {state:?}, not {goal:?}")]
    NotReached {
        session: String,
        frames: u64,
        goal: SessionState,
        state: SessionState,
    },
    #[error("login {session}: the character set names no {character:?}; it names {names:?}")]
    NoSuchCharacter {
        session: String,
        character: String,
        names: Vec<String>,
    },
    #[error("{0} needs a login first")]
    NoSession(&'static str),
    #[error("the client shut itself down at frame {0}")]
    Stopped(u64),
    /// The client shut itself down because its session ended, the server saying why.
    #[error("{0}")]
    Ended(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Plugin(String),
    #[error("the connection was lost: {0}")]
    Disconnected(String),
}

/// The variable that names a settings folder, the desktop client's own.
pub const SETTINGS_DIR_ENV: &str = "DERETH_SETTINGS_DIR";

/// The preferences file a run keeps in the settings folder `dir` (from [`SETTINGS_DIR_ENV`]), as
/// the desktop client keeps its own: beside it the client keeps what its journeys learn for each
/// world (portals used, portals that refused it), so the next session starts knowing them. No
/// folder named, none: the run writes nothing.
#[must_use]
pub fn settings_preferences(dir: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let dir = dir.filter(|d| !d.is_empty())?;
    Some(PathBuf::from(dir).join(dereth_client_sdk::runtime::config::PREFERENCES_FILE_NAME))
}

/// Why a session ended, in words. A logon refused because the account is still in the world (the
/// server holds a session a client left without logging off) names that: the server logs that
/// session off as it refuses, and the next logon gets in.
fn ended_why(why: dereth_client_sdk::net::client_session::DisconnectReason) -> String {
    use dereth_client_sdk::net::client_session::DisconnectReason;
    match why {
        DisconnectReason::CharacterError(1) => concat!(
            "the server refused the logon: the account was still in the world ",
            "(character error 1), and the server logs that session off"
        )
        .to_owned(),
        DisconnectReason::CharacterError(code) => format!("character error {code}"),
        DisconnectReason::ServerDied => "the server went silent".to_owned(),
        DisconnectReason::AccountBooted => "the account was booted".to_owned(),
        DisconnectReason::AccountBanned => "the account is banned".to_owned(),
        DisconnectReason::LoggedOff => "logged off".to_owned(),
        DisconnectReason::Transport(code) => format!("transport error {code}"),
    }
}

/// What a finished run did, for a caller that wants numbers rather than the text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// Frames run, over the whole script.
    pub frames: u64,
    /// Commands executed, `quit` included.
    pub commands: usize,
    /// The login FSM's state when the run ended, if a `login` ever ran.
    pub state: Option<SessionState>,
    /// The characters the replayed login's character set named.
    pub characters: Vec<String>,
    /// How many login-complete notifications (`0x00A1`) the client sent: one when the log-in's
    /// portal-space animation ends, and one after each teleport.
    pub login_completes_sent: u64,
    /// Whether the client built its travel's planner, which it does the first time a journey or
    /// a plan is asked for.
    pub travel_planner_built: bool,
}

/// Build a headless client, run the script against it, and shut it down.
///
/// # Errors
/// [`RunError`] -- and the client is shut down on the way out either way.
pub fn run(commands: &[Command], opts: &Options, out: &mut dyn Write) -> Result<Summary, RunError> {
    let mut session = Run::new(opts, out)?;
    let result = session.script(commands);
    let summary = session.finish();
    result?;
    Ok(summary)
}

pub(crate) struct Run<'a> {
    app: App<HeadlessShell>,
    shell: HeadlessShell,
    opts: Options,
    out: &'a mut dyn Write,
    frames: u64,
    commands: usize,
    state: Option<SessionState>,
    characters: Vec<String>,
    /// How many of the chat windows' queued lines `chat` has printed.
    chat_seen: usize,
    /// The inbound datagrams `login` left unfed, with the recorded time and the client time the
    /// last one was fed at, for `replay`.
    rest: std::collections::VecDeque<Datagram>,
    paced_from: Option<(f64, f64)>,
    /// The frames now are at the wall clock's pace.
    paced: bool,
    /// A `quit` ended the script.
    pub(crate) quit: bool,
    /// An `enter` took a character into the world.
    pub(crate) entered: bool,
}

impl<'a> Run<'a> {
    pub(crate) fn new(opts: &Options, out: &'a mut dyn Write) -> Result<Self, RunError> {
        let mut cfg = Config {
            headless: true,
            frames: None,
            // Without `--connect` no socket is ever opened: the only endpoint is the socket-free
            // one `App::attach_replay_network` installs.
            connect: false,
            sound: false,
            dat_dir: opts.dat_dir.clone(),
            classic_dat_dir: opts.classic_dat_dir.clone(),
            era: opts.era,
            world_base: opts.world_base,
            preferences_file: opts.preferences_file.clone(),
            width: opts.width,
            height: opts.height,
            ..Config::default()
        };
        if let Some(server) = &opts.connect {
            let (host, port) = server
                .rsplit_once(':')
                .and_then(|(h, p)| Some((h.to_owned(), p.parse::<u32>().ok()?)))
                .ok_or_else(|| {
                    RunError::Startup(format!("--connect {server}: want <host:port>"))
                })?;
            cfg.connect = true;
            cfg.host = host;
            cfg.port = port;
            cfg.account = opts.account.to_lowercase();
            cfg.account_as_typed.clone_from(&opts.account);
            cfg.vg_password.clone_from(&opts.password);
        }
        let (width, height) = (opts.width, opts.height);
        let world = opts.world;
        let mut app = App::<HeadlessShell>::bring_up(
            cfg,
            |_| Ok(Platform::headless(width, height)),
            |_, _, _, _| {
                Ok(if world {
                    Box::new(SimPresentation::new(width, height)) as Box<dyn Presentation>
                } else {
                    Box::new(NullPresentation::new(width, height))
                })
            },
        )
        .map_err(|e| RunError::Startup(e.to_string()))?;
        app.interaction.client_build_id = BUILD_ID;
        if world && opts.connect.is_some() {
            // Built on world entry, at the character's own place, as for a recorded login.
            app.defer_static_scene(SceneConfig {
                character: true,
                ..SceneConfig::default()
            });
        }
        if let Some(path) = &opts.nav_published {
            let text = std::fs::read_to_string(path).map_err(|e| {
                RunError::Startup(format!("--nav-published {}: {e}", path.display()))
            })?;
            if let Ok(mut slot) = app.published_portals.lock() {
                *slot = Some(text);
            }
        }
        let mut shell = HeadlessShell::default();
        // Initialization step 10 is `App::bring_up`; starting input and the sound manager is
        // `App::start_shell`. `HeadlessShell` has no device input to start (this client acts by
        // injecting actions) and no UI but the game's questions, and `Config::sound` is false
        // above, so no audio endpoint is taken.
        app.start_shell(&mut shell)
            .map_err(|e| RunError::Startup(e.to_string()))?;
        let mut run = Self {
            app,
            shell,
            opts: opts.clone(),
            out,
            frames: 0,
            commands: 0,
            state: None,
            characters: Vec::new(),
            chat_seen: 0,
            rest: std::collections::VecDeque::new(),
            paced_from: None,
            paced: false,
            quit: false,
            entered: false,
        };
        if !run.opts.plugins.is_empty() {
            run.plugin_host();
        }
        let (w, h) = run.app.presentation().size();
        run.line(format!("app {w}x{h} headless step={HEADLESS_STEP:.6}"))?;
        Ok(run)
    }

    fn line(&mut self, s: impl AsRef<str>) -> Result<(), RunError> {
        writeln!(self.out, "{}", s.as_ref())?;
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Summary {
        if let Some(host) = self.existing_host() {
            host.flush();
        }
        let summary = Summary {
            frames: self.frames,
            commands: self.commands,
            state: self.state,
            characters: self.characters.clone(),
            login_completes_sent: self.app.teleport.login_completes_sent,
            travel_planner_built: self.app.travel.planner_built(),
        };
        self.app.shutdown(&mut self.shell);
        summary
    }

    pub(crate) fn script(&mut self, commands: &[Command]) -> Result<(), RunError> {
        for c in commands {
            self.commands += 1;
            match c {
                Command::Login { session, character } => self.login(session, character.as_deref()),
                Command::Tick(n) => self.tick(*n),
                Command::Walk { direction, secs } => self.walk(*direction, *secs),
                Command::Hold { action, secs } => self.hold(*action, *secs),
                Command::Press(action) => self.press(*action),
                Command::Begin(action) => self.edge(*action, true),
                Command::End(action) => self.edge(*action, false),
                Command::Use(id) => self.use_object(*id),
                Command::Say(text) => self.say(text),
                Command::Dump(what) => self.dump(*what),
                Command::Snapshot => self.snapshot(),
                Command::World(block) => self.world(*block),
                Command::Position => self.position(),
                Command::Plugin(c) => self.plugin(c),
                Command::Ui(c) => self.ui(c),
                Command::Chat => self.chat(),
                Command::Replay(secs) => self.replay(*secs),
                Command::Run(secs) => self.run_for(*secs),
                Command::RunForever => self.run_forever(),
                Command::Enter(name, mage) => self.enter(name, *mage),
                Command::Answer(yes) => self.answer(*yes),
                Command::Quit => {
                    self.quit = true;
                    let frames = self.frames;
                    self.line(format!("quit frames={frames}"))?;
                    return Ok(());
                }
            }?;
        }
        Ok(())
    }

    /// One whole frame. `App::frame` answering `false` means the frame loop returned false, which
    /// is the client shutting itself down; a script cannot carry on past it.
    fn frame(&mut self) -> Result<(), RunError> {
        // The portal tunnel is one of the interface's elements, ticked by a front end's UI step;
        // this client has none, so over a server connection it ticks it here. It is what tells
        // the server the character has arrived (`Character_LoginCompleteNotification`), without
        // which the server holds the character hidden and still. A replay needs no answer.
        if self.opts.connect.is_some() {
            self.app.teleport_use_time(&self.shell);
        }
        // Frames stepped faster than the clock wait for the travel's plans, as the clock would
        // have; frames at the clock's pace never do.
        self.app.travel.wait_for_plans = !self.paced;
        if !self.app.frame(&mut self.shell) {
            // A client whose session ended shuts itself down with it: the reason is the server's.
            if let Some(SessionState::Disconnected(why)) =
                self.app.network_mut().map(|n| n.session_state())
            {
                return Err(RunError::Ended(ended_why(why)));
            }
            return Err(RunError::Stopped(self.frames));
        }
        self.frames += 1;
        Ok(())
    }

    // -------------------------------------------------------------------------------------
    // login
    // -------------------------------------------------------------------------------------

    fn login(&mut self, session: &str, character: Option<&str>) -> Result<(), RunError> {
        // `login <name>` takes a recording's slug, which is its file name. A name
        // the corpus does not carry is "no such capture" with the name the caller typed in it.
        if self.opts.world && character.is_some() {
            // Armed before the login and built on world entry, at the player's own landblock and
            // position, exactly as the windowed client does when it connects.
            self.app.defer_static_scene(SceneConfig {
                character: true,
                ..SceneConfig::default()
            });
        }
        let path = self.opts.captures_dir.join(format!("{session}.jsonl"));
        let records = capture::load(&path)?;
        let sequence = capture::connection_sequence_number(&records).unwrap_or(0);
        let net = ClientNetwork::new(
            &peer(0).to_string(),
            7304,
            &self.opts.account,
            "unused",
            sequence,
        )
        .map_err(|e| RunError::Net(e.to_string()))?;
        self.app
            .attach_replay_network(net)
            .map_err(|_| RunError::Net("this client already has a link".to_owned()))?;
        let inbound = records.iter().filter(|r| !r.c2s).count();
        self.line(format!(
            "login {session} datagrams={} inbound={inbound} sequence={sequence}",
            records.len()
        ))?;

        let mut fed = 0_u64;
        let mut selected = character.is_none();
        let started = self.frames;
        let goal = if character.is_none() {
            SessionState::CharacterSelect
        } else {
            SessionState::Playable
        };
        let inbound: Vec<&Datagram> = records.iter().filter(|r| !r.c2s).collect();
        let mut next = inbound.len();
        for (k, r) in inbound.iter().enumerate() {
            next = k + 1;
            self.feed(r)?;
            self.paced_from = Some((r.t, self.app.clock().local_time));
            fed += 1;
            self.frame()?;
            let state = self.session_state()?;
            self.state = Some(state);
            if state == SessionState::CharacterSelect && !selected {
                self.characters = self.character_names();
                let name = character.unwrap_or_default();
                self.enter_world(session, name)?;
                selected = true;
            }
            // Without a character the goal is the character-select screen; with one it is being in
            // the world, which is `0x0013 Login_PlayerDescription` and therefore `Playable`.
            // Either way the replay stops at its goal rather than running the recording's log-off,
            // which would put the session back where it started.
            if state == goal {
                break;
            }
            if self.frames - started >= self.opts.max_login_frames {
                break;
            }
        }
        self.rest = inbound[next..].iter().map(|d| (*d).clone()).collect();
        // A login that stopped before the character set arrived never gave the script anything to
        // select, and a run that carried on would be asserting against an empty client.
        let state = self.session_state()?;
        self.state = Some(state);
        if self.characters.is_empty() {
            self.characters = self.character_names();
        }
        if state != goal {
            return Err(RunError::NotReached {
                session: session.to_owned(),
                frames: self.frames - started,
                goal,
                state,
            });
        }
        for n in self.characters.clone() {
            self.line(format!("character {n}"))?;
        }
        let frames = self.frames - started;
        self.line(format!(
            "login {session} fed={fed} frames={frames} state={state:?} characters={}",
            self.characters.len()
        ))
    }

    fn feed(&mut self, r: &Datagram) -> Result<(), RunError> {
        // **The recording's own timestamps are not this client's clock.** Local time comes from
        // the `FixedStepClock`'s, and the login FSM's give-up timer (20 x 2 s) is
        // measured against whatever time a datagram arrives at, so a datagram fed at its recorded
        // `t` would arrive from the client's own future and the FSM would stop. Feeding at
        // the application clock keeps every datagram in the client's present.
        let now = LocalTime(self.app.clock().local_time);
        let from = peer(r.pair);
        let net = self
            .app
            .replay_network_mut()
            .ok_or_else(|| RunError::Net("the replay endpoint is gone".to_owned()))?;
        net.feed(&r.raw, from, now);
        Ok(())
    }

    /// The login FSM's own answer. There is no sensible stand-in for "no endpoint", so it is an
    /// error: every caller has just attached one.
    fn session_state(&mut self) -> Result<SessionState, RunError> {
        self.app
            .replay_network_mut()
            .map(|net| net.session_state())
            .ok_or_else(|| RunError::Net("the replay endpoint is gone".to_owned()))
    }

    fn character_names(&mut self) -> Vec<String> {
        self.app.replay_network_mut().map_or_else(Vec::new, |net| {
            net.characters()
                .characters
                .iter()
                .map(|c| c.name.clone())
                .collect()
        })
    }

    fn enter_world(&mut self, session: &str, name: &str) -> Result<(), RunError> {
        let Some(net) = self.app.replay_network_mut() else {
            return Err(RunError::Net("the replay endpoint is gone".to_owned()));
        };
        let set = net.characters();
        let account = set.account.clone();
        let Some(gid) = set
            .characters
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.gid)
        else {
            let names = set.characters.iter().map(|c| c.name.clone()).collect();
            return Err(RunError::NoSuchCharacter {
                session: session.to_owned(),
                character: name.to_owned(),
                names,
            });
        };
        net.enter_world(gid, &account);
        self.line(format!("enter-world {name} id=0x{:08X}", gid.0))
    }

    // -------------------------------------------------------------------------------------
    // the rest of the verbs
    // -------------------------------------------------------------------------------------

    fn tick(&mut self, n: u64) -> Result<(), RunError> {
        for _ in 0..n {
            self.frame()?;
        }
        let (frames, time) = (self.frames, self.app.clock().cur_time);
        self.line(format!("tick {n} frames={frames} time={time:.6}"))
    }

    /// Hold one movement action for `secs` of *simulated* time: [`Self::hold`] on the direction's
    /// action, printed as the walk it is.
    fn walk(&mut self, direction: Direction, secs: f64) -> Result<(), RunError> {
        let frames = self.hold_frames(direction.action(), secs)?;
        self.line(format!(
            "walk {} secs={secs} frames={frames}",
            direction.name()
        ))
    }

    /// Hold an action for `secs` of *simulated* time.
    ///
    /// The begin is injected into the runtime's action queue (`App::inject_action`), the frames
    /// run, and the end stops it. That is the same pair of edges a held key produces, and this
    /// client has no keyboard to produce them.
    fn hold(&mut self, action: ActionId, secs: f64) -> Result<(), RunError> {
        let frames = self.hold_frames(action, secs)?;
        self.line(format!(
            "hold {} secs={secs} frames={frames}",
            action_name(action)
        ))
    }

    /// Begin, `secs` of frames, end, one more frame; answers how many frames ran.
    fn hold_frames(&mut self, action: ActionId, secs: f64) -> Result<u64, RunError> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = (secs / HEADLESS_STEP).ceil().max(0.0) as u64;
        self.app.inject_action(Action::begin(action));
        for _ in 0..frames {
            self.frame()?;
        }
        self.app.inject_action(Action::end(action));
        self.frame()?;
        Ok(frames + 1)
    }

    /// `secs` of frames at the wall clock's pace, as a connection to a server needs: each frame
    /// waits for its moment.
    fn run_for(&mut self, secs: f64) -> Result<(), RunError> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = (secs / HEADLESS_STEP).ceil().max(0.0) as u64;
        let start = std::time::Instant::now();
        self.paced = true;
        for i in 0..frames {
            #[allow(clippy::cast_precision_loss)]
            let due = std::time::Duration::from_secs_f64(i as f64 * HEADLESS_STEP);
            if let Some(wait) = due.checked_sub(start.elapsed()) {
                std::thread::sleep(wait);
            }
            if let Err(e) = self.frame() {
                self.paced = false;
                return Err(e);
            }
        }
        self.paced = false;
        let state = self.app.network_mut().map(|n| n.session_state());
        self.state = state;
        self.line(format!("run secs={secs} frames={frames} state={state:?}"))
    }

    /// `run forever`: frames at the wall clock's pace until [`Options::stop`] is set, or the
    /// connection is lost (an error, so a daemon connects again). Each chat line the client
    /// takes is logged (target `chat`) and then let go, as a chat window would take it, so a
    /// client that runs for days does not keep every line.
    pub(crate) fn run_forever(&mut self) -> Result<(), RunError> {
        let mut last = std::time::Instant::now();
        let started = self.frames;
        self.chat_seen = 0;
        loop {
            if self.opts.stop.load(Ordering::Relaxed) {
                break;
            }
            self.paced_frame(&mut last)?;
            for (_, m) in std::mem::take(&mut self.app.hud.pending_chat) {
                let line = format!("{}{}", m.prefix.clone().unwrap_or_default(), m.body);
                tracing::info!(target: "chat", "{}", line.trim_end().replace('\n', " / "));
            }
            if let Some(dereth_client_sdk::net::client_session::SessionState::Disconnected(why)) =
                self.app.network_mut().map(|n| n.session_state())
            {
                self.state = Some(SessionState::Disconnected(why));
                return Err(RunError::Disconnected(ended_why(why)));
            }
        }
        let frames = self.frames - started;
        self.line(format!("run forever stopped frames={frames}"))
    }

    /// One frame at the wall clock's pace: it waits until `HEADLESS_STEP` after the last.
    fn paced_frame(&mut self, last: &mut std::time::Instant) -> Result<(), RunError> {
        let due = *last + std::time::Duration::from_secs_f64(HEADLESS_STEP);
        if let Some(wait) = due.checked_duration_since(std::time::Instant::now()) {
            std::thread::sleep(wait);
        }
        *last = std::time::Instant::now();
        self.paced = true;
        let r = self.frame();
        self.paced = false;
        r
    }

    /// Paced frames until `done` holds, for at most `secs`; answers whether it held.
    fn paced_until(
        &mut self,
        secs: f64,
        mut done: impl FnMut(&mut Self) -> bool,
    ) -> Result<bool, RunError> {
        let start = std::time::Instant::now();
        let mut last = start;
        while start.elapsed().as_secs_f64() < secs {
            if done(self) {
                return Ok(true);
            }
            self.paced_frame(&mut last)?;
        }
        Ok(done(self))
    }

    /// `enter <name>` over a server connection: once the character list is up, enter the world as
    /// `name`, creating the character first (an Aluvian soldier in Holtburg) when the account has
    /// none of that name; then frames until the character stands in the world.
    fn enter(&mut self, name: &str, mage: bool) -> Result<(), RunError> {
        use dereth_client_sdk::contract::pregame::CharGenAction;
        let listed = |run: &mut Self| run.app.host_state.received_set;
        if !self.paced_until(60.0, listed)? {
            return Err(RunError::Net("no character list within 60 s".to_owned()));
        }
        let find = |run: &mut Self| {
            run.app.network_mut().and_then(|n| {
                n.characters()
                    .characters
                    .iter()
                    // An administrator's characters are listed with a leading `+`.
                    .find(|c| c.name.trim_start_matches('+').eq_ignore_ascii_case(name))
                    .map(|c| c.gid)
            })
        };
        let mut created = false;
        if find(self).is_none() {
            let before = self.app.host_state.chargen_response_notices;
            self.app
                .run_chargen_actions(vec![CharGenAction::SendCharGenResult(Box::new(if mage {
                    caster(name)
                } else {
                    soldier(name)
                }))]);
            let answered = |run: &mut Self| run.app.host_state.chargen_response_notices != before;
            if !self.paced_until(30.0, answered)? {
                return Err(RunError::Net(
                    "no answer to the character's creation".to_owned(),
                ));
            }
            created = true;
        }
        let Some(gid) = find(self) else {
            let code = self.app.host_state.chargen_response;
            return Err(RunError::Net(format!(
                "the character {name} was not created (answer {code:?})"
            )));
        };
        self.app
            .run_chargen_actions(vec![CharGenAction::LogOn(gid)]);
        let in_world = |run: &mut Self| {
            run.app
                .network_mut()
                .is_some_and(|n| n.session_state() == SessionState::Playable)
                && run.app.objects.player().is_some()
        };
        let entered = self.paced_until(90.0, in_world)?;
        self.entered |= entered;
        let state = self.app.network_mut().map(|n| n.session_state());
        self.state = state;
        self.line(format!(
            "enter {name} created={created} entered={entered} state={state:?}"
        ))
    }

    /// A key tapped: the begin, a frame, the end, a frame.
    fn press(&mut self, action: ActionId) -> Result<(), RunError> {
        self.app.inject_action(Action::begin(action));
        self.frame()?;
        self.app.inject_action(Action::end(action));
        self.frame()?;
        self.line(format!("press {} frames=2", action_name(action)))
    }

    /// One edge of an action alone, and the frame it lands in.
    fn edge(&mut self, action: ActionId, begin: bool) -> Result<(), RunError> {
        self.app.inject_action(if begin {
            Action::begin(action)
        } else {
            Action::end(action)
        });
        self.frame()?;
        let verb = if begin { "begin" } else { "end" };
        self.line(format!("{verb} {}", action_name(action)))
    }

    fn use_object(&mut self, id: ObjectId) -> Result<(), RunError> {
        self.app.submit_requests(vec![UiRequest::Use(id)]);
        self.frame()?;
        self.line(format!("use 0x{:08X}", id.0))
    }

    /// `say <text>`: the line, then each line the chat took in that frame (the client's own
    /// answers among them), as `chat <line>`.
    fn say(&mut self, text: &str) -> Result<(), RunError> {
        self.app.submit_requests(vec![UiRequest::ChatLine {
            text: text.to_owned(),
            window: 0,
        }]);
        self.frame()?;
        self.line(format!("say {text}"))?;
        let answers: Vec<String> = self
            .app
            .objects()
            .world
            .scroll
            .pending()
            .iter()
            .flat_map(|l| l.body.lines().map(str::to_owned).collect::<Vec<_>>())
            .collect();
        for answer in answers {
            self.line(format!("chat {answer}"))?;
        }
        Ok(())
    }

    fn dump(&mut self, what: Dump) -> Result<(), RunError> {
        let events: Vec<FrameEvent> = match what {
            Dump::Events => self.app.frame_events().last_frame().to_vec(),
            Dump::Steps => self
                .app
                .last_frame_steps()
                .iter()
                .copied()
                .map(FrameEvent::Step)
                .collect(),
        };
        for line in FrameEvents::text(&events).lines() {
            self.line(line)?;
        }
        Ok(())
    }

    fn snapshot(&mut self) -> Result<(), RunError> {
        let snap = self.app.hud().snapshot(self.app.objects());
        let id = |o: Option<ObjectId>| o.map_or("none".to_owned(), |o| format!("0x{:08X}", o.0));
        let rows = [
            format!("player={}", id(snap.player())),
            format!("character={}", snap.character_name().unwrap_or("none")),
            format!("selection={}", id(snap.selection())),
            format!("combat-mode={}", snap.combat_mode()),
            format!("outside={}", snap.player_outside()),
            format!("heading={:.3}", snap.player_heading()),
            format!(
                "coords={}",
                snap.player_coords()
                    .map_or("none".to_owned(), |(x, y)| format!("{x:.3},{y:.3}"))
            ),
            format!("radar={}", snap.radar_objects().len()),
            format!("skills={}", snap.skills().len()),
            format!("now={:.6}", snap.now()),
        ];
        for r in rows {
            self.line(format!("snapshot {r}"))?;
        }
        Ok(())
    }

    /// `world <landblock>`: a world built offline with a body at the middle of the landblock.
    fn world(&mut self, block: u16) -> Result<(), RunError> {
        if !self.opts.world {
            return Err(RunError::Net(
                "world needs --world: with no world presentation there is nothing to load"
                    .to_owned(),
            ));
        }
        self.app
            .load_static_scene(SceneConfig {
                landblock: block,
                character: true,
                ..SceneConfig::default()
            })
            .map_err(|e| RunError::Startup(e.to_string()))?;
        self.line(format!("world {block:#06X}"))
    }

    /// `position`: the body's cell and block-local origin, whether it is hidden, and whether it
    /// stands on something (what the client reports to the server as its contact).
    fn position(&mut self) -> Result<(), RunError> {
        let body = self.app.world.as_ref().and_then(|w| {
            w.character
                .as_ref()
                .map(|c| (c.position(), w.character_state.is_hidden(), c.in_contact()))
        });
        match body {
            Some((p, hidden, contact)) => {
                let o = p.frame.origin;
                self.line(format!(
                    "position cell={:#010X} origin={:.3},{:.3},{:.3} hidden={hidden} contact={contact}",
                    p.cell.0, o.x, o.y, o.z
                ))
            }
            None => self.line("position none"),
        }
    }
}

impl Run<'_> {
    // -------------------------------------------------------------------------------------
    // plugins
    // -------------------------------------------------------------------------------------

    /// The installed plugin host, if any.
    fn existing_host(&mut self) -> Option<&mut PluginHost> {
        self.app
            .frame_hook_mut()
            .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
            .map(|h| &mut h.host)
    }

    /// The plugin host, installed into the runtime on first use with the packages `--plugins`
    /// names queued for the next frame.
    fn plugin_host(&mut self) -> &mut PluginHost {
        if self.existing_host().is_none() {
            let storage: Box<dyn PluginStorage> = match &self.opts.plugin_store {
                Some(root) => Box::new(crate::plugins::DirStorage { root: root.clone() }),
                None => Box::new(MemoryStorage::default()),
            };
            let mut config = crate::plugins::host_config(self.opts.world);
            if let Some(ms) = self.opts.plugin_call_budget_ms {
                config.limits.call_budget_ms = ms;
            }
            let mut host = PluginHost::new(
                config,
                Box::new(dereth_client_sdk::plugin_lua::LuauEngine),
                Box::new(crate::plugins::DirSource {
                    roots: self.opts.plugins.clone(),
                }),
                storage,
            )
            .with_engine(Box::new(
                dereth_client_sdk::plugin_wasm::WasmEngine::interpreted(),
            ))
            // The other headless clients that name the same folder, or a bus with no one else on
            // it.
            .with_peers(
                match self
                    .opts
                    .peers
                    .as_deref()
                    .map(crate::plugins::LoopbackPeers::open)
                {
                    Some(Ok(bus)) => Box::new(bus),
                    Some(Err(e)) => {
                        tracing::warn!("--peers: {e}; no other clients are heard");
                        Box::new(dereth_client_sdk::plugins::LocalBus::new().endpoint())
                    }
                    None => Box::new(dereth_client_sdk::plugins::LocalBus::new().endpoint()),
                },
            );
            if let Some(root) = &self.opts.plugin_data {
                host = host.with_files(Box::new(crate::plugins::DirFiles { root: root.clone() }));
            }
            if let Some(text) = &self.opts.plugin_policy {
                match dereth_client_sdk::plugins::policy_doc::parse(text) {
                    Ok(p) => host.set_world_policy(p),
                    Err(e) => tracing::warn!("--plugin-policy: {e}; the defaults apply"),
                }
            }
            if self.opts.world {
                host = host.with_game_data(Box::new(dereth_client_sdk::DatGameData(
                    self.app.store.clone(),
                )));
            }
            host.load_all();
            self.app.install_frame_hook(Box::new(PluginHook { host }));
        }
        self.existing_host().expect("installed above")
    }

    fn plugin_list(&mut self) -> Result<(), RunError> {
        let list = self.plugin_host().plugins();
        if list.is_empty() {
            self.line("plugin none")?;
        }
        for p in list {
            let why = p.reason.map(|r| format!(": {r}")).unwrap_or_default();
            self.line(format!("plugin {} {} {}{why}", p.id, p.version, p.status))?;
        }
        Ok(())
    }

    fn plugin(&mut self, c: &PluginCommand) -> Result<(), RunError> {
        match c {
            PluginCommand::Load(dir) => {
                let path = std::path::Path::new(dir);
                let package = if path.is_file() {
                    crate::plugins::read_zip_package(path)
                } else {
                    crate::plugins::read_package(path)
                }
                .map_err(RunError::Plugin)?;
                self.plugin_host().request_load(package);
                self.frame()?;
                self.plugin_list()
            }
            PluginCommand::Unload(id) => {
                self.plugin_host().request_unload(id);
                self.frame()?;
                self.plugin_list()
            }
            PluginCommand::Reload(id) => {
                self.plugin_host().request_reload(id);
                self.frame()?;
                self.plugin_list()
            }
            PluginCommand::List => self.plugin_list(),
            PluginCommand::Console(id) => {
                let lines = self
                    .plugin_host()
                    .console(id)
                    .ok_or_else(|| RunError::Plugin(format!("no plugin {id} is loaded")))?;
                for l in lines {
                    self.line(format!("console {id}: {l}"))?;
                }
                Ok(())
            }
        }
    }

    fn ui(&mut self, c: &UiCommand) -> Result<(), RunError> {
        let (plugin, window, input) = match c {
            UiCommand::Dump => {
                let dumps: Vec<(String, String, bool, String)> = self
                    .plugin_host()
                    .windows()
                    .into_iter()
                    .map(|(p, w, win)| (p, w, win.open, win.tree.dump()))
                    .collect();
                for (p, w, open, tree) in dumps {
                    self.line(format!("window {p} {w} open={open}"))?;
                    for l in tree.lines() {
                        self.line(format!("  {l}"))?;
                    }
                }
                return Ok(());
            }
            UiCommand::Layout => {
                let laid: Vec<(String, String, String)> = self
                    .plugin_host()
                    .windows()
                    .into_iter()
                    .filter(|(_, _, win)| win.open)
                    .map(|(p, w, win)| {
                        let text = dereth_client_sdk::plugin_api::layout::reference_dump(
                            &win.tree, win.size,
                        );
                        (p, w, text)
                    })
                    .collect();
                for (p, w, text) in laid {
                    self.line(format!("window {p} {w}"))?;
                    for l in text.lines() {
                        self.line(format!("  {l}"))?;
                    }
                }
                return Ok(());
            }
            UiCommand::Click {
                plugin,
                window,
                node,
            } => (plugin, window, UiInput::Click { node: node.clone() }),
            UiCommand::Set {
                plugin,
                window,
                node,
                value,
            } => {
                let value = dereth_client_sdk::plugin_api::Value::from_json(value)
                    .map_err(|e| RunError::Plugin(format!("ui set: {value:?} is not JSON: {e}")))?;
                (
                    plugin,
                    window,
                    UiInput::Change {
                        node: node.clone(),
                        value,
                    },
                )
            }
            UiCommand::Close { plugin, window } => (plugin, window, UiInput::Close),
            UiCommand::Resize {
                plugin,
                window,
                width,
                height,
            } => (
                plugin,
                window,
                UiInput::Resize {
                    width: *width,
                    height: *height,
                },
            ),
            UiCommand::Hotkey { plugin, node } => {
                self.plugin_host()
                    .press_hotkey(plugin, node)
                    .map_err(RunError::Plugin)?;
                self.frame()?;
                return self.line(format!("hotkey {plugin} {node} ok"));
            }
            UiCommand::Bar { plugin } => {
                self.plugin_host()
                    .toggle_window(plugin)
                    .map_err(RunError::Plugin)?;
                self.frame()?;
                return self.line(format!("bar {plugin} ok"));
            }
        };
        self.plugin_host()
            .ui_input(plugin, window, input)
            .map_err(RunError::Plugin)?;
        self.frame()?;
        self.line(format!("ui {plugin} {window} ok"))
    }

    /// Carry on with the recording at its recorded pace: before each datagram, as many frames run
    /// as fit in the recording's own gap since the last one fed (one per [`HEADLESS_STEP`]).
    fn replay(&mut self, secs: Option<f64>) -> Result<(), RunError> {
        let Some((t0, _)) = self.paced_from else {
            return Err(RunError::NoSession("replay"));
        };
        let until = secs.map(|s| t0 + s);
        let started = self.frames;
        let mut fed = 0_u64;
        while let Some(r) = self.rest.front() {
            if until.is_some_and(|u| r.t > u) {
                break;
            }
            let r = self.rest.pop_front().expect("front exists");
            let (last_t, _) = self.paced_from.expect("set by login");
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let gap = ((r.t - last_t) / HEADLESS_STEP).floor().max(0.0) as u64;
            for _ in 1..gap {
                self.frame()?;
            }
            self.feed(&r)?;
            self.frame()?;
            self.paced_from = Some((r.t, 0.0));
            fed += 1;
        }
        let state = self.session_state()?;
        self.state = Some(state);
        let frames = self.frames - started;
        self.line(format!(
            "replay fed={fed} frames={frames} left={} state={state:?}",
            self.rest.len()
        ))
    }

    /// The lines queued for the chat windows since the last `chat`: this client has no chat
    /// window, so the queue is the record of what one would have shown.
    fn chat(&mut self) -> Result<(), RunError> {
        let lines: Vec<(u8, String)> = {
            let queued = &self.app.hud.pending_chat;
            if queued.len() < self.chat_seen {
                self.chat_seen = 0;
            }
            let new = queued[self.chat_seen..]
                .iter()
                .map(|(_, m)| {
                    (
                        m.ty,
                        format!("{}{}", m.prefix.clone().unwrap_or_default(), m.body),
                    )
                })
                .collect();
            self.chat_seen = queued.len();
            new
        };
        for (ty, text) in lines {
            self.line(format!("chat {ty} {text}"))?;
        }
        for shown in self.shell.questions.take_shown() {
            self.line(format!("dialog {shown}"))?;
        }
        Ok(())
    }

    /// `answer yes|no`: the player's answer to the oldest question on screen, taken at the next
    /// frame.
    fn answer(&mut self, yes: bool) -> Result<(), RunError> {
        let word = if yes { "yes" } else { "no" };
        match self.shell.questions.answer(yes) {
            Some(q) => self.line(format!("answer {word}: {}", q.replace('\n', " "))),
            None => self.line(format!("answer {word}: no question on screen")),
        }
    }
}

/// The fourteen frame steps, as `dump steps` prints them. Exposed so that a test
/// can compare a run's output against the order rather than against a copy of it.
#[must_use]
pub fn step_lines() -> Vec<String> {
    FrameStep::ORDER
        .iter()
        .copied()
        .map(|s| FrameEvent::Step(s).to_string())
        .collect()
}

/// A new character for `enter <name> mage`: the soldier's look and start, with War Magic,
/// Creature Enchantment, Life Magic and Mana Conversion trained and Focus and Self high.
fn caster(name: &str) -> dereth_client_sdk::contract::pregame::CharGenResultData {
    let mut c = soldier(name);
    let mut skills = vec![0; 55];
    for skill in [34usize, 31, 33, 16] {
        skills[skill] = 2;
    }
    c.skill_advancement_classes = skills;
    c.strength = 10;
    c.endurance = 60;
    c.coordination = 50;
    c.quickness = 10;
    c.focus = 100;
    c.self_ = 100;
    c
}

/// A new character for `enter`: an Aluvian male soldier (template 6) with Heavy Weapons, Melee
/// Defense, Healing, Jump and Run trained, starting in Holtburg.
fn soldier(name: &str) -> dereth_client_sdk::contract::pregame::CharGenResultData {
    let mut skills = vec![0; 55];
    for skill in [44usize, 6, 21, 22, 24] {
        skills[skill] = 2;
    }
    dereth_client_sdk::contract::pregame::CharGenResultData {
        heritage_group: 1,
        gender: 1,
        eyes_strip: 0,
        nose_strip: 0,
        mouth_strip: 0,
        hair_color: 0,
        eye_color: 0,
        hair_style: 0,
        headgear_style: 0,
        headgear_color: 3,
        shirt_style: 0,
        shirt_color: 4,
        trousers_style: 0,
        trousers_color: 5,
        footwear_style: 0,
        footwear_color: 6,
        skin_shade: 1.0,
        hair_shade: 0.5,
        headgear_shade: 0.25,
        shirt_shade: 0.125,
        trousers_shade: 0.375,
        footwear_shade: 0.625,
        template_num: 6,
        strength: 100,
        endurance: 60,
        coordination: 100,
        quickness: 50,
        focus: 10,
        self_: 10,
        slot: 0,
        class_id: 1,
        skill_advancement_classes: skills,
        name: name.to_owned(),
        start_area: 0,
        is_admin: 0,
        is_envoy: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{ended_why, settings_preferences};
    use dereth_client_sdk::net::client_session::DisconnectReason;

    #[test]
    fn a_settings_folder_named_holds_the_preferences_and_none_named_writes_nothing() {
        let dir = std::ffi::OsString::from("runs/bot");
        assert_eq!(
            settings_preferences(Some(&dir)),
            Some(std::path::Path::new("runs/bot").join("UserPreferences.ini"))
        );
        assert_eq!(settings_preferences(Some(std::ffi::OsStr::new(""))), None);
        assert_eq!(settings_preferences(None), None);
    }

    #[test]
    fn a_logon_refused_while_the_account_is_still_in_the_world_says_so() {
        let why = ended_why(DisconnectReason::CharacterError(1));
        assert!(why.contains("the account was still in the world"), "{why}");
        assert_eq!(
            ended_why(DisconnectReason::CharacterError(13)),
            "character error 13"
        );
        assert_eq!(
            ended_why(DisconnectReason::ServerDied),
            "the server went silent"
        );
    }
}
