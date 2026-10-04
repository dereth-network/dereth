//! Driving a real [`App`] from a parsed script.
//!
//! The client this module builds is the runtime's application with nothing plugged into it:
//! [`NullShell`] for the front end (no UI), `NullPresentation` for the device, `NullWindow` for the
//! window, `FixedStepClock` for the clock. There is no window, no graphics device, no UI, no
//! socket, and no data file beyond the retail dats [`Options::dat_dir`] names.
//!
//! Everything this module prints goes through one `Write`, one line per fact, so the whole run is
//! a text file a test can compare.

use std::io::Write;
use std::path::PathBuf;

use dereth_client_sdk::actions::{Action, ActionId};
use dereth_client_sdk::contract::{GameView, UiRequest};
use dereth_client_sdk::net::client_session::SessionState;
use dereth_client_sdk::primitives::{LocalTime, ObjectId};
use dereth_client_sdk::runtime::app::{App, NullShell, Platform, HEADLESS_STEP};
use dereth_client_sdk::runtime::config::Config;
use dereth_client_sdk::runtime::frame::FrameStep;
use dereth_client_sdk::runtime::frame_events::{FrameEvent, FrameEvents};
use dereth_client_sdk::runtime::net::ClientNetwork;
use dereth_client_sdk::runtime::present::{NullPresentation, Presentation};
use dereth_client_sdk::runtime::scene::SceneConfig;
use dereth_client_sdk::runtime::sim_present::SimPresentation;

// `peer(pair)` is the address scheme every capture-replaying harness uses; it lives with the reader.
use crate::capture::{self, peer, CaptureError, Datagram};
use crate::script::{action_name, Command, Direction, Dump};

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
    /// the temp directory that this run does not create.
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
    #[error(transparent)]
    Io(#[from] std::io::Error),
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

struct Run<'a> {
    app: App<NullShell>,
    shell: NullShell,
    opts: Options,
    out: &'a mut dyn Write,
    frames: u64,
    commands: usize,
    state: Option<SessionState>,
    characters: Vec<String>,
}

impl<'a> Run<'a> {
    fn new(opts: &Options, out: &'a mut dyn Write) -> Result<Self, RunError> {
        let cfg = Config {
            headless: true,
            frames: None,
            // No socket is ever opened: the only endpoint this binary has is the socket-free one
            // `App::attach_replay_network` installs.
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
        let (width, height) = (opts.width, opts.height);
        let world = opts.world;
        let mut app = App::<NullShell>::bring_up(
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
        let mut shell = NullShell;
        // Initialization step 10 is `App::bring_up`; starting input and the sound manager is
        // `App::start_shell`. `NullShell` has no device input to start (this client acts by
        // injecting actions) and no UI, and `Config::sound` is false above, so no audio endpoint is
        // taken.
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
        };
        let (w, h) = run.app.presentation().size();
        run.line(format!("app {w}x{h} headless step={HEADLESS_STEP:.6}"))?;
        Ok(run)
    }

    fn line(&mut self, s: impl AsRef<str>) -> Result<(), RunError> {
        writeln!(self.out, "{}", s.as_ref())?;
        Ok(())
    }

    fn finish(mut self) -> Summary {
        let summary = Summary {
            frames: self.frames,
            commands: self.commands,
            state: self.state,
            characters: self.characters.clone(),
            login_completes_sent: self.app.teleport.login_completes_sent,
        };
        self.app.shutdown(&mut self.shell);
        summary
    }

    fn script(&mut self, commands: &[Command]) -> Result<(), RunError> {
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
                Command::Quit => {
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
        if !self.app.frame(&mut self.shell) {
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
        for r in records.iter().filter(|r| !r.c2s) {
            self.feed(r)?;
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

    fn say(&mut self, text: &str) -> Result<(), RunError> {
        self.app.submit_requests(vec![UiRequest::ChatLine {
            text: text.to_owned(),
            window: 0,
        }]);
        self.frame()?;
        self.line(format!("say {text}"))
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

    /// `position`: the body's cell and block-local origin.
    fn position(&mut self) -> Result<(), RunError> {
        let body = self.app.world.as_ref().and_then(|w| {
            w.character
                .as_ref()
                .map(|c| (c.position(), w.character_state.is_hidden()))
        });
        match body {
            Some((p, hidden)) => {
                let o = p.frame.origin;
                self.line(format!(
                    "position cell={:#010X} origin={:.3},{:.3},{:.3} hidden={hidden}",
                    p.cell.0, o.x, o.y, o.z
                ))
            }
            None => self.line("position none"),
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
