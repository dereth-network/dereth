//! [`HeadlessClient`] -- a whole client a scenario can drive, and the read-only views it answers
//! with.
//!
//! # Three backends, one vocabulary
//!
//! A scenario says `given`, `when`, `tick`, and then asks the client what happened. What is
//! underneath depends on one field of [`ClientSpec`]:
//!
//! | [`Assets`] | what it builds | what it can assert |
//! |---|---|---|
//! | [`Assets::None`] | the object stream, the HUD and the interaction layer, with an optional \
//!   replay endpoint under them -- no `App`, no dats, no screens | the model: objects, chat, \
//!   requests, the snapshot |
//! | [`Assets::Shell`] | the same model, plus a bare `dereth_client_shell::ui::UiShell` over the retail \
//!   dats driven against a **synthetic** `HostState` -- no `App` | the UI tree and the shipped \
//!   layouts, at a host state the scenario writes |
//! | [`Assets::Retail`] | `App::with_platform(cfg, NullPresentation, Platform::headless(w, h))` \
//!   -- the whole client, on a fixed-step clock, with no device and no window | all of the above \
//!   plus the frame log, the UI tree and the shipped layouts |
//!
//! # Why the shell backend exists
//!
//! For the login-flow claims and "the UI is configured and the shell is not started". Those flows
//! are about what the *shell* does when the
//! host tells it something -- the shard sent a second, identical character set; the patch
//! finished; the player is in the world -- and an `App`'s host state is its own, computed from
//! its own link every frame. A scenario cannot say "and now the shard says this" to an `App`
//! without a shard, and half of what those flows are about never reaches one. [`ClientSpec::shell`]
//! hands the scenario the `HostState` itself.
//!
//! It is **not** a whole client: there is no renderer, no scene, no input manager and no frame
//! log, and [`crate::ScenarioView::app`] answers `None`. A claim about the game model under a
//! panel wants [`Assets::Retail`]; a claim about the flow between screens wants this.
//!
//! The split is not a convenience. The `cpu` tier opens no retail data file, and an `App` cannot
//! be built without one, so a scenario whose claim is about the model must be able to make the
//! model without the client's shell. That is the same line `dereth-client`'s own two test binaries
//! already draw; this crate makes it a parameter instead of a different harness.
//!
//! **The recorded captures are not dats.** They are committed to the repository, so an
//! [`Assets::None`] scenario may still replay one, and many do.
//!
//! # Setup failures are panics
//!
//! `given`, `when` and `tick` panic rather than return a `Result`. A scenario that cannot set
//! itself up has not proved anything: a test that skips its setup can appear to pass without
//! exercising the behavior it claims to cover. The panic message names the step and the recording.

use std::sync::Arc;

use dereth_client_contract::{GameSnapshot, UiRequest};
use dereth_client_model::{Notice, RecordingSink, Request, World};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::frame_events::FrameEvents;
use dereth_client_runtime::interaction::Interaction;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_client_runtime::present::NullPresentation;
use dereth_client_shell::hud::Hud;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui_screens::chat::interface::ChatMessage;
use {
    dereth_client::app::App, dereth_client_runtime::app::Platform,
    dereth_client_runtime::platform::clock::HEADLESS_STEP,
};

use crate::login;
use crate::view::ScenarioView;

/// What a scenario's client is built out of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assets {
    /// No retail data file at all. The model only: no `App`, no screens, no layouts.
    None,
    /// The retail dats under `$DERETH_TEST_DAT_DIR`, a bare [`dereth_client_shell::ui::UiShell`] over them, and
    /// the host state the scenario writes. No `App`. See the module docs.
    Shell,
    /// The retail dats under `$DERETH_TEST_DAT_DIR`, and a whole `App` over them.
    Retail,
}

/// How to build the client.
#[derive(Debug, Clone)]
pub struct ClientSpec {
    /// Which backend. See [`Assets`].
    pub assets: Assets,
    /// The null presentation's extent, which is what the UI shell lays out against.
    pub width: u32,
    pub height: u32,
    /// Bring the UI shell up (`Assets::Retail` only). A scenario whose claim is about a panel
    /// needs it; one whose claim is about the frame does not, and it is the slower half of
    /// startup.
    pub shell: bool,
    /// Put a screen up after the shell.
    pub ui_mode: Option<dereth_ui::UiMode>,
    /// Frames to settle the screen for, after the mode is queued.
    pub settle_frames: u64,
    /// Load the default static scene before the screens (`Assets::Retail` only). The notice-strip
    /// scenarios need one; nothing else does.
    pub static_scene: bool,
    /// Raise this registered page of the gameplay panel bar once the screen has settled
    /// (`Assets::Retail` only). See [`Self::with_open_page`].
    pub open_page: Option<dereth_ui::ElementId>,
    /// Where the client writes its preferences and everything beside them. See
    /// [`Self::with_settings_dir`]; `None` is a temporary path that is never created, which is
    /// what a scenario that writes nothing wants.
    pub settings_dir: Option<std::path::PathBuf>,
    /// The synthetic host state an [`Assets::Shell`] client is driven against. See
    /// [`Self::shell`].
    pub host: Option<Box<dereth_client_contract::pregame::PregameView>>,
}

impl ClientSpec {
    /// The model only: no dats, no shell.
    #[must_use]
    pub fn model() -> Self {
        Self {
            assets: Assets::None,
            width: 800,
            height: 600,
            shell: false,
            ui_mode: None,
            settle_frames: 0,
            static_scene: false,
            open_page: None,
            settings_dir: None,
            host: None,
        }
    }

    /// A whole `App` over the retail dats, with no shell.
    #[must_use]
    pub fn retail() -> Self {
        Self {
            assets: Assets::Retail,
            ..Self::model()
        }
    }

    /// A whole `App` with its UI shell up and the gameplay screen settled.
    #[must_use]
    pub fn gameplay(frames: u64) -> Self {
        Self {
            shell: true,
            ui_mode: Some(dereth_ui::framework::mode::GAME_PLAY),
            settle_frames: frames,
            ..Self::retail()
        }
    }

    /// [`Self::gameplay`] with the default static scene under it, which the notice strip needs.
    #[must_use]
    pub fn gameplay_in_world(frames: u64) -> Self {
        Self {
            static_scene: true,
            ..Self::gameplay(frames)
        }
    }

    /// A whole `App` with its UI shell up and `mode` settled.
    #[must_use]
    pub fn screen(mode: dereth_ui::UiMode, frames: u64) -> Self {
        Self {
            shell: true,
            ui_mode: Some(mode),
            settle_frames: frames,
            ..Self::retail()
        }
    }

    /// A bare [`dereth_client_shell::ui::UiShell`] over the retail dats, driven against `host`. No `App`.
    ///
    /// The shell's mode is whatever the flow decides from `host` -- which is the
    /// point: `UiShell::new` comes up with the data-patch mode pending, and the screens the login
    /// flow walks through are reached by *what the host says*, not by queueing a mode. A scenario
    /// that wants to stand on one mode regardless says so with [`Self::on_mode`].
    #[must_use]
    pub fn shell(host: dereth_client_contract::pregame::PregameView) -> Self {
        Self {
            assets: Assets::Shell,
            host: Some(Box::new(host)),
            ..Self::model()
        }
    }

    /// Queue `mode` and settle for `frames` frames, whichever backend this is. On
    /// [`Assets::Shell`] it is `UiShell::queue`, which is the shell's own entry point.
    #[must_use]
    pub fn on_mode(self, mode: dereth_ui::UiMode, frames: u64) -> Self {
        Self {
            ui_mode: Some(mode),
            settle_frames: frames,
            ..self
        }
    }

    /// Raise `page` -- one of the gameplay screen's registered panel pages -- once the screen has
    /// settled, through the screen's own `recv_set_panel_visibility`.
    ///
    /// Every inventory scenario opens the pack, and many panel scenarios open a page too, so this
    /// is the one way to do it. It is deliberately the
    /// *page element id* the shipped layout carries and not a panel id, because that is what a
    /// scenario knows; the panel id is looked up in the screen's own page table, so a page the
    /// layout does not register panics here rather than opening nothing.
    #[must_use]
    pub fn with_open_page(self, page: dereth_ui::ElementId) -> Self {
        Self {
            open_page: Some(page),
            ..self
        }
    }

    /// Put the client's preferences file, and everything the client writes beside it, under `dir`.
    ///
    /// The journal is a file beside the preferences file, so a scenario about it needs a
    /// preferences directory of its own rather than the user's real settings folder. The directory is created when the client is
    /// built and **removed when the client is dropped** -- see [`HeadlessClient::scratch_settings`]
    /// -- so a scenario gets a fresh one every time and leaves nothing behind.
    ///
    /// `Self::scratch_settings` is the one that names a unique directory for you.
    #[must_use]
    pub fn with_settings_dir(self, dir: std::path::PathBuf) -> Self {
        Self {
            settings_dir: Some(dir),
            ..self
        }
    }

    /// [`Self::with_settings_dir`] with a unique directory under the system temporary directory,
    /// named after `tag`, the process and a counter -- so two scenarios in one binary, and two
    /// binaries at once, never share one.
    #[must_use]
    pub fn with_scratch_settings(self, tag: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("dere-scenario-{tag}-{}-{n}", std::process::id()));
        self.with_settings_dir(dir)
    }
}

/// How a scenario begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Given {
    /// Replay a recorded session's login.
    ///
    /// Without a character the replay stops when the shard's character set has arrived; with one
    /// it selects that character by name and keeps replaying until the player is in the world.
    /// **No socket is opened**: the endpoint is the replay one, and only the recording's
    /// server-to-client datagrams are fed.
    EnteredWorld {
        session: &'static str,
        character: Option<&'static str>,
    },
    /// A player character in an otherwise empty world, and nothing else. `Assets::None` only:
    /// the `App` builds its own world.
    APlayer(ObjectId),
}

/// One thing a scenario does to the client. Implemented by [`crate::Inbound`] and
/// [`crate::Player`].
pub trait Step {
    /// Apply this step. Panics on a step the client cannot take; see the module docs.
    fn apply(self, client: &mut HeadlessClient);
}

/// The client under a scenario.
pub struct HeadlessClient {
    backend: Backend,
    /// Everything the scenario has seen the client do, accumulated across steps.
    pub(crate) log: Log,
    /// What the client really framed into a datagram. See [`crate::wire`].
    wire: crate::wire::Wire,
    spec: ClientSpec,
    /// The disposable preferences directory, when the spec named one. Dropping it removes the
    /// directory. See [`ClientSpec::with_settings_dir`].
    settings: Option<ScratchSettings>,
}

/// A directory the client was told to write its preferences into, removed when the scenario ends.
///
/// It is a guard rather than a `cleanup()` a scenario has to remember, because the scenario that
/// most wants one is the scenario that panics: a failed assertion must not leave a settings
/// directory behind.
#[derive(Debug)]
pub struct ScratchSettings {
    dir: std::path::PathBuf,
}

impl ScratchSettings {
    /// The directory itself, for a scenario that wants to read what the client wrote.
    #[must_use]
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }
}

impl Drop for ScratchSettings {
    fn drop(&mut self) {
        // A scenario's own directory under the system temporary directory, created by this
        // process. A failure to remove it is not a failure of the scenario -- a file the client
        // still holds open on Windows is the usual reason -- and panicking in a `Drop` during an
        // unwind would replace the assertion message with this one.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl std::fmt::Debug for HeadlessClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeadlessClient")
            .field("assets", &self.spec.assets)
            .field("frames", &self.log.frames)
            .field("asserted", &self.log.asserted)
            .finish_non_exhaustive()
    }
}

pub(crate) enum Backend {
    Model(Box<Model>),
    App(Box<App>),
}

/// The `Assets::None` client: `App::frame`'s model half, with the renderer, the screens and the
/// platform left out.
///
/// **An [`Assets::Shell`] client is this with [`Self::shell`] filled in**, and not a third
/// `Backend` variant, on purpose: a variant would make every `match` on [`Backend`] in this crate
/// non-exhaustive for no gain. The shell is an
/// addition to the model backend rather than an alternative to it, which is also what it is at
/// runtime -- `UiShell` reads `HostState` and nothing else, so the object stream underneath it is
/// the same empty model either way.
pub(crate) struct Model {
    pub(crate) objects: ObjectStream,
    pub(crate) hud: Hud,
    pub(crate) interaction: Interaction,
    pub(crate) net: Option<ClientNetwork>,
    /// The bare shell, on an [`Assets::Shell`] client.
    pub(crate) shell: Option<Box<ShellOnly>>,
    /// The dat store the shell was built over, on an [`Assets::Shell`] client.
    pub(crate) store: Option<Arc<RetailDatStore>>,
    /// Empty, and it stays empty: the frame log is `App`'s and this backend runs no frame steps.
    /// It exists so that `frame_events()` answers the same type on both backends rather than an
    /// `Option` every caller would unwrap.
    pub(crate) events: FrameEvents,
    /// The clock this backend feeds and stamps at. The recorded `t` drives it during a login
    /// replay.
    pub(crate) now: f64,
}

/// The bare shell of an [`Assets::Shell`] client: the element manager and the mode flow, the
/// synthetic host state they read, and nothing else.
///
/// This is everything a login-flow scenario needs, and the `host` field is the thing it cannot
/// say to an `App`.
pub(crate) struct ShellOnly {
    pub(crate) ui: dereth_client_shell::ui::UiShell,
    /// What the application would be telling the shell this frame. A scenario writes it through
    /// [`HeadlessClient::host_mut`], which is the step "and now the shard says so".
    pub(crate) host: dereth_client_contract::pregame::PregameView,
    /// What the shell asked the host for, accumulated. `UiShell::frame` answers these and the
    /// application acts on them; here the scenario reads them.
    pub(crate) requests: Vec<UiRequest>,
}

/// What the scenario has watched the client do.
#[derive(Debug, Default)]
pub(crate) struct Log {
    pub(crate) frames: u64,
    pub(crate) chat: Vec<ChatMessage>,
    pub(crate) notices: Vec<Notice>,
    pub(crate) ui_requests: Vec<UiRequest>,
    pub(crate) requests: Vec<Request>,
    pub(crate) asserted: Vec<&'static str>,
    /// The `App`'s frame counter when this scenario started, so that `events_text` is this
    /// scenario's frames and not the ring's.
    pub(crate) first_frame: u64,
    /// The client's own frame index at the last drain of its outbox, so that two steps between
    /// two frames do not count one frame's outbox twice. See `drain`.
    pub(crate) last_drained_frame: Option<u64>,
    /// The millisecond clock a pointer gesture is stamped at. It starts well away from zero
    /// because the input manager's double-click window is measured against it.
    pub(crate) pointer_time: u32,
    /// The account the replayed login's character set named.
    pub(crate) account: String,
    /// The characters it offered, in the recorded order.
    pub(crate) characters: Vec<String>,
}

impl HeadlessClient {
    /// Build one.
    ///
    /// # Panics
    /// Panics when an `Assets::Retail` client cannot be built -- which means the retail dats under
    /// `$DERETH_TEST_DAT_DIR` are absent, and a scenario that carried on would be asserting over an
    /// empty client.
    #[must_use]
    pub fn new(spec: ClientSpec) -> Self {
        // The desktop's platform answers, the local time zone among them, before any surface
        // formats a date: a client built with no application installs them nowhere else.
        dereth_client::hud::install_platform();
        let settings = spec.settings_dir.as_ref().map(|dir| {
            std::fs::create_dir_all(dir).unwrap_or_else(|e| {
                panic!(
                    "this scenario asked for a disposable settings directory at {}, and it could \
                     not be made: {e}",
                    dir.display()
                )
            });
            ScratchSettings { dir: dir.clone() }
        });
        let model = || Model {
            objects: ObjectStream::new(),
            hud: Hud::new(),
            interaction: Interaction::default(),
            net: None,
            shell: None,
            store: None,
            events: FrameEvents::new(),
            now: 0.0,
        };
        let backend = match spec.assets {
            Assets::None => Backend::Model(Box::new(model())),
            Assets::Shell => {
                let (store, shell) = build_shell(&spec);
                Backend::Model(Box::new(Model {
                    shell: Some(Box::new(shell)),
                    store: Some(store),
                    ..model()
                }))
            }
            Assets::Retail => Backend::App(Box::new(build_app(&spec))),
        };
        let first_frame = match &backend {
            Backend::App(app) => app.frame_events().frame_index(),
            Backend::Model(m) => m.events.frame_index(),
        };
        Self {
            backend,
            log: Log {
                first_frame,
                pointer_time: 100_000,
                ..Log::default()
            },
            wire: crate::wire::Wire::new(),
            spec,
            settings,
        }
    }

    /// The disposable settings directory this client was built with, when its spec named one.
    ///
    /// A scenario about what the client *wrote* -- the journal, the options file --
    /// reads it back from here. See [`ClientSpec::with_settings_dir`].
    #[must_use]
    pub fn scratch_settings(&self) -> Option<&ScratchSettings> {
        self.settings.as_ref()
    }

    /// The model-only client, for the `cpu` tier.
    #[must_use]
    pub fn model() -> Self {
        Self::new(ClientSpec::model())
    }

    /// Set the scenario up.
    ///
    /// # Panics
    /// Panics when the recording is absent or the login does not reach its goal.
    pub fn given(&mut self, g: Given) -> &mut Self {
        match g {
            Given::EnteredWorld { session, character } => {
                let set = match &mut self.backend {
                    Backend::Model(m) => login::model_login(m, session, character),
                    Backend::App(app) => login::app_login(app, session, character),
                };
                self.log.account = set.account;
                self.log.characters = set.characters;
            }
            Given::APlayer(id) => {
                let Backend::Model(m) = &mut self.backend else {
                    panic!(
                        "Given::APlayer is an Assets::None step: an App builds its own world, so \
                         seeding one behind its back would be asserting over a world the client \
                         does not use"
                    )
                };
                seed_player(&mut m.objects.world, id);
            }
        }
        self.drain();
        self
    }

    /// Do something to the client.
    pub fn when<S: Step>(&mut self, step: S) -> &mut Self {
        step.apply(self);
        self.drain();
        self
    }

    /// Run `n` whole frames.
    ///
    /// On the `App` backend that is `n` calls to the client's own frame; on the model backend it
    /// is `n` passes of the pump, the HUD and the interaction layer, which is what a frame's model
    /// half does.
    ///
    /// # Panics
    /// Panics when the client shuts itself down, which a scenario cannot continue past.
    pub fn tick(&mut self, n: u64) -> &mut Self {
        for _ in 0..n {
            match &mut self.backend {
                Backend::App(app) => {
                    assert!(
                        app.frame(),
                        "the client shut itself down at frame {}",
                        self.log.frames
                    );
                }
                Backend::Model(m) => {
                    let (chat, notices, requests) = model_frame(m);
                    self.log.chat.extend(chat);
                    self.log.notices.extend(notices);
                    self.log.requests.extend(requests);
                }
            }
            self.log.frames += 1;
            self.drain();
        }
        self
    }

    // -------------------------------------------------------------------------------------
    // The views
    // -------------------------------------------------------------------------------------

    /// A read-only handle on everything below.
    #[must_use]
    pub fn view(&self) -> ScenarioView<'_> {
        ScenarioView::new(self)
    }

    /// The frame as an owned value, through the client's own projection.
    #[must_use]
    pub fn snapshot(&self) -> GameSnapshot {
        match &self.backend {
            Backend::App(app) => app.hud().snapshot(app.objects()),
            Backend::Model(m) => m.hud.snapshot(&m.objects),
        }
    }

    /// The frame **and the shipped element tree** as one owned value. See
    /// [`crate::ui_snapshot`].
    ///
    /// # Panics
    /// Panics on the model backend, which has no shell, and on an `App` whose shell is not up --
    /// both of which mean the scenario asked for the wrong [`ClientSpec`], and both of which the
    /// message names.
    pub fn ui_snapshot(&mut self) -> crate::ui_snapshot::UiSnapshot {
        let game = self.snapshot();
        // **The shell backend answers this too.** It has a tree and a draw list and no `App`,
        // which is exactly the case the login flows are.
        if let Backend::Model(m) = &mut self.backend {
            let shell = m.shell.as_mut().unwrap_or_else(|| {
                panic!(
                    "this assertion reads the shipped element tree, so the scenario must be built \
                     with ClientSpec::gameplay(..), ClientSpec::screen(..) or ClientSpec::shell(..)"
                )
            });
            let draw = shell.ui.draw_list();
            let roots: Vec<dereth_ui::ElemHandle> = shell
                .ui
                .flow
                .current()
                .expect("a screen is current")
                .roots()
                .to_vec();
            assert!(
                !roots.is_empty(),
                "the current screen has no root element, so there is no tree to read"
            );
            return crate::ui_snapshot::UiSnapshot::capture(game, draw, &mut shell.ui.ui, &roots);
        }
        let Backend::App(app) = &mut self.backend else {
            unreachable!("the model backend is handled above")
        };
        let draw = app.ui_draw_list().to_vec();
        let shell = app.ui_mut().expect(
            "this assertion reads the shipped element tree, so the scenario's ClientSpec must \
             bring the UI shell up",
        );
        let roots: Vec<dereth_ui::ElemHandle> = shell
            .flow
            .current()
            .expect("a screen is current")
            .roots()
            .to_vec();
        assert!(
            !roots.is_empty(),
            "the current screen has no root element, so there is no tree to read"
        );
        crate::ui_snapshot::UiSnapshot::capture(game, draw, &mut shell.ui, &roots)
    }

    /// The opcode of every message the client has sent during this scenario, in order.
    ///
    /// It is derived by handing each request the client produced to the production sender over a
    /// mock transport and reading the opcode dword back off the blob -- so a request whose sender
    /// is missing contributes nothing, and no second table of opcodes exists here to drift from
    /// the sender's own.
    ///
    /// An ordered envelope reports **its sub-type** rather than the envelope opcode every ordered
    /// game action shares; see [`crate::outbound::opcodes`].
    #[must_use]
    pub fn outbound_opcodes(&self) -> Vec<u32> {
        crate::outbound::opcodes(&self.log.requests)
    }

    /// The ordered sub-type of every message the client really **framed into a datagram**, in
    /// order, from the first frame of this scenario.
    ///
    /// **It is not a duplicate of [`Self::outbound`]**: see [`crate::wire`] for the
    /// one message the frame's own outbox cannot see. It needs an attached endpoint and answers an
    /// empty slice without one, because a client with no link has written nothing anywhere.
    pub fn outbound_wire(&mut self) -> &[u32] {
        self.observe_wire();
        self.wire.sub_types()
    }

    /// How many of `sub_type` the client has framed into a datagram since the last take.
    ///
    /// The question a scenario about one message asks, and a take rather than a count so that
    /// "one swing, fifty frames ago" and "one swing just now" are different observations.
    pub fn take_wire_count(&mut self, sub_type: u32) -> usize {
        self.observe_wire();
        self.wire.take_count(sub_type)
    }

    /// The clock the wire reader reassembles at: this scenario's own frame count, in the client's
    /// fixed step. Reassembly needs a monotonic clock and nothing more.
    #[must_use]
    pub fn wire_clock(&self) -> LocalTime {
        #[allow(clippy::cast_precision_loss)]
        LocalTime(self.log.frames as f64 * HEADLESS_STEP)
    }

    /// Take what the endpoint has written and fold it into this client's wire reader. A client
    /// with no endpoint has written nothing, which is not a failure.
    fn observe_wire(&mut self) {
        let now = self.wire_clock();
        let datagrams = match &mut self.backend {
            Backend::App(app) => app.replay_network_mut().map(ClientNetwork::take_outgoing),
            Backend::Model(m) => m.net.as_mut().map(ClientNetwork::take_outgoing),
        };
        if let Some(datagrams) = datagrams {
            self.wire.ingest(&datagrams, now);
        }
    }

    /// Every request the client produced, in order, before it became bytes.
    ///
    /// **This is the frame's outbox, not the wire**, and the two differ in both directions:
    ///
    /// * a message the client puts straight on the flow queue never reaches the outbox at all --
    ///   the cancel that breaks an automatic attack is one, which is why [`Self::outbound_wire`]
    ///   exists beside this;
    /// * on the **model backend** nothing is ever framed into a datagram, because that backend
    ///   runs no sender, so this is the only reader there is and [`Self::outbound_wire`] is
    ///   honestly empty. (The layer's own `last_sent` is filled by the frame's send pass, and the
    ///   model backend takes its pending queue instead.)
    ///
    /// # Read this, and not `interaction().last_sent`
    ///
    /// On the `App` backend `Interaction::last_sent` is a **one-frame
    /// window**: the frame's send pass *replaces* it every time, so a scenario that does a gesture,
    /// runs the four frames the gesture needs, and then reads `last_sent` finds whatever the last
    /// of those frames put there -- which is usually nothing, so the scenario looks green while
    /// asserting over nothing.
    ///
    /// This reader accumulates instead, across every step and every frame of the scenario, and it
    /// drains the `App`'s outbox once per *client* frame rather than once per scenario step, so a
    /// `when` followed by a `tick` does not count one outbox twice (see `drain`).
    #[must_use]
    pub fn outbound(&self) -> &[Request] {
        &self.log.requests
    }

    /// Every notice the scenario's own drives raised.
    ///
    /// **This is empty on the `App` backend**, an adapter gap: the `App` owns its notice sink and
    /// exposes no accessor. A
    /// scenario over an `App` reads the same lines through [`Self::chat_lines`], which is where
    /// the notices that reach the screen end up.
    #[must_use]
    pub fn notices(&self) -> &[Notice] {
        &self.log.notices
    }

    /// Every UI request the scenario put into the client.
    ///
    /// The requests the shipped panels raise for themselves are not here: `Interaction` keeps them
    /// in a private queue with no accessor. Another adapter gap.
    #[must_use]
    pub fn ui_requests(&self) -> &[UiRequest] {
        &self.log.ui_requests
    }

    /// The frame log. Empty on the model backend, which runs no frame steps.
    #[must_use]
    pub fn frame_events(&self) -> &FrameEvents {
        match &self.backend {
            Backend::App(app) => app.frame_events(),
            Backend::Model(m) => &m.events,
        }
    }

    /// Every chat line the client composed from an event this scenario delivered.
    #[must_use]
    pub fn chat_lines(&self) -> &[ChatMessage] {
        &self.log.chat
    }

    /// The frame log of this scenario's frames, one event per line, in the frame log's own text
    /// form. This is what a golden file holds; see [`crate::golden`].
    ///
    /// # Panics
    /// Panics when the scenario has run more frames than the log's ring retains. The ring is
    /// bounded on purpose -- a station that runs ten thousand frames must not grow a buffer per
    /// frame -- and a golden written from a truncated log would be a golden of the tail with
    /// nothing saying so.
    #[must_use]
    pub fn events_text(&self) -> String {
        let log = self.frame_events();
        assert!(
            log.first_retained_frame() <= self.log.first_frame,
            "this scenario has run {} frames and the frame log retains from frame {} onwards, so \
             its first {} frames are already gone; a golden of the rest would be a golden of the \
             tail",
            self.log.frames,
            log.first_retained_frame(),
            log.first_retained_frame() - self.log.first_frame,
        );
        let events = log.since(self.log.first_frame);
        FrameEvents::text(&events)
    }

    /// Assert a documented behaviour, and record the id as asserted.
    ///
    /// The recording is the scenario's, not the process's: `crate::behaviours::run_scenario`
    /// opens one around the scenario and fails it unless the ids noted here are exactly the ones
    /// its `ALL` entry declares. Asserting an id a scenario does not declare is a red.
    ///
    /// # Panics
    /// Panics when `id` is not in the registry, or when `f` answers false. The message carries the
    /// behaviour's own sentence, so a failure reads as the claim that broke rather than as a
    /// comparison.
    pub fn assert_behaviour(
        &mut self,
        id: &'static str,
        f: impl FnOnce(&ScenarioView<'_>) -> bool,
    ) -> &mut Self {
        let b = crate::behaviours::lookup(id)
            .unwrap_or_else(|| panic!("no behaviour is documented with the id {id:?}"));
        let held = f(&self.view());
        assert!(
            held,
            "behaviour {id} does not hold: {}\n  evidence: {:?}",
            b.says, b.evidence
        );
        crate::behaviours::note_asserted(id);
        self.log.asserted.push(id);
        self
    }

    /// The ids this scenario asserted, in order.
    #[must_use]
    pub fn asserted(&self) -> &[&'static str] {
        &self.log.asserted
    }

    /// How many frames this scenario has run.
    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.log.frames
    }

    /// Shut the client down. Optional: dropping the scenario is enough for the model backend, and
    /// an `App` that is dropped still releases what it holds.
    pub fn shutdown(self) {
        if let Backend::App(app) = self.backend {
            app.shutdown();
        }
    }

    // -------------------------------------------------------------------------------------
    // What the steps reach
    // -------------------------------------------------------------------------------------

    pub(crate) const fn backend(&self) -> &Backend {
        &self.backend
    }

    pub(crate) fn backend_mut(&mut self) -> &mut Backend {
        &mut self.backend
    }

    /// The account the replayed login's character set named, empty when no login ran.
    #[must_use]
    pub fn account(&self) -> &str {
        &self.log.account
    }

    /// The characters the replayed login was offered, in the recorded order.
    #[must_use]
    pub fn characters(&self) -> &[String] {
        &self.log.characters
    }

    /// The game model, to set a scenario up with.
    ///
    /// A `given` and a `when` are the readable way to reach the client; this is for the state a
    /// scenario must stand up that no step expresses -- the objects a claim is about, and the
    /// options it depends on. It is deliberately on the client and not on the view, so that a
    /// mutation is visible in the scenario body rather than inside an assertion.
    pub fn world_mut(&mut self) -> &mut World {
        match &mut self.backend {
            Backend::App(app) => &mut app.probe_mut().objects_mut().world,
            Backend::Model(m) => &mut m.objects.world,
        }
    }

    /// The object stream, to set a scenario up with. See [`Self::world_mut`].
    pub fn objects_mut(&mut self) -> &mut ObjectStream {
        match &mut self.backend {
            Backend::App(app) => app.probe_mut().objects_mut(),
            Backend::Model(m) => &mut m.objects,
        }
    }

    /// The HUD, to set a scenario up with. See [`Self::world_mut`].
    pub fn hud_mut(&mut self) -> &mut Hud {
        match &mut self.backend {
            Backend::App(app) => app.probe_mut().hud_mut(),
            Backend::Model(m) => &mut m.hud,
        }
    }

    /// The interaction layer, to set a scenario up with. See [`Self::world_mut`].
    ///
    /// The layer owns the command table (`@a` and its siblings are registered into it at login,
    /// and a scenario about a chat command has to register them the way the client does), and it
    /// owns the refusal line and the per-subsystem counters. The [`HeadlessClient::ui_requests`]
    /// gap is the same seam from the other side.
    pub fn interaction_mut(&mut self) -> &mut Interaction {
        match &mut self.backend {
            Backend::App(app) => app.probe_mut().interaction_mut(),
            Backend::Model(m) => &mut m.interaction,
        }
    }

    /// The interaction layer and the game model **at the same time**, whichever backend this is.
    ///
    /// Several of the production entry points a scenario
    /// drives take both at once -- `Interaction::on_world_object_found(found, &mut
    /// world, now)` is one -- and [`Self::interaction_mut`] and
    /// [`Self::world_mut`] are two separate borrows of the client, so a scenario with only those
    /// two ends up standing a model host of its own up beside the one it is testing.
    pub fn interaction_and_world_mut(&mut self) -> (&mut Interaction, &mut World) {
        match &mut self.backend {
            Backend::App(app) => app.probe_mut().interaction_and_world_mut(),
            Backend::Model(m) => (&mut m.interaction, &mut m.objects.world),
        }
    }

    /// The whole client, when the scenario was built with one.
    ///
    /// # Panics
    /// Panics on the model backend.
    pub fn app_mut(&mut self) -> &mut App {
        match &mut self.backend {
            Backend::App(app) => app,
            Backend::Model(_) => panic!(
                "this scenario drives the client's shell, so it must be built with \
                 ClientSpec::retail() or ClientSpec::gameplay(..)"
            ),
        }
    }

    /// The request queue of this client's UI: the `App`'s shell's on a retail client, the bare
    /// shell's on an [`Assets::Shell`] one. What a scenario emits here is what a panel of that UI
    /// would have emitted, and what the frame drains.
    ///
    /// # Panics
    /// Panics on a client with no UI.
    pub fn ui_outbox(&mut self) -> &mut dereth_client_contract::requests::Outbox {
        &mut self.ui_system().requests
    }

    /// The inbound-notice queue of this client's UI. See [`Self::ui_outbox`].
    ///
    /// # Panics
    /// Panics on a client with no UI.
    pub fn ui_notice_inbox(&mut self) -> &mut dereth_client_contract::notices::NoticeInbox {
        &mut self.ui_system().notice_inbox
    }

    fn ui_system(&mut self) -> &mut dereth_ui::UiSystem {
        match &mut self.backend {
            Backend::App(app) => &mut app.ui_mut().expect("the App's UI shell is up").ui,
            Backend::Model(m) => {
                &mut m
                    .shell
                    .as_mut()
                    .expect("this client has no UI; build it with a shell")
                    .ui
                    .ui
            }
        }
    }

    /// Tell the HUD where the player is looking from, which is what the frame's own sync does.
    ///
    /// A claim about earshot or about what is on the radar needs a viewer; one about the chat
    /// model does not, and a HUD that has never synced deliberately has no body -- which is itself
    /// a behaviour two of the seeded scenarios assert.
    pub fn sync_viewer(
        &mut self,
        viewer: Option<dereth_client_runtime::hud::ViewerFrame>,
    ) -> &mut Self {
        match &mut self.backend {
            Backend::Model(m) => m.hud.sync(&m.objects, viewer),
            Backend::App(_) => panic!(
                "an App syncs its own HUD every frame; a scenario that pushed a viewer in would \
                 be asserting over a frame the client did not build"
            ),
        }
        self
    }

    /// Attach a socket-free endpoint, whichever backend this is.
    ///
    /// The two backends keep their link in two different places -- an `App` owns its own and the
    /// model backend holds one beside the object stream -- so the match is written here once.
    /// [`crate::replay`] builds the endpoint; this is where
    /// it goes.
    ///
    /// # Panics
    /// Panics when the client already has a link. A scenario gets one shard, and a second endpoint
    /// would leave the first one's queued datagrams unread.
    pub fn attach_replay(&mut self, net: dereth_client_runtime::net::ClientNetwork) {
        match &mut self.backend {
            Backend::App(app) => app.attach_replay_network(net).unwrap_or_else(|_| {
                panic!("this client already has a link; a scenario logs in once")
            }),
            Backend::Model(m) => {
                assert!(
                    m.net.is_none(),
                    "this client already has a link; a scenario logs in once"
                );
                m.net = Some(net);
            }
        }
    }

    /// The attached endpoint. Mutable and not also immutable, because `App` publishes only the
    /// mutable accessor; everything the harness asks
    /// an endpoint for -- feeding a datagram in, taking the written ones out -- needs it anyway.
    ///
    /// See [`Self::attach_replay`].
    pub fn replay_net_mut(&mut self) -> Option<&mut dereth_client_runtime::net::ClientNetwork> {
        match &mut self.backend {
            Backend::App(app) => app.replay_network_mut(),
            Backend::Model(m) => m.net.as_mut(),
        }
    }

    /// The dat store the client was built over, when it has one. An [`Assets::Shell`] client has
    /// one -- no layout decodes without it -- and an [`Assets::None`] client does not.
    #[must_use]
    pub fn dat_store(&self) -> Option<&Arc<RetailDatStore>> {
        match &self.backend {
            Backend::App(app) => Some(app.probe().dat_store()),
            Backend::Model(m) => m.store.as_ref(),
        }
    }

    // -------------------------------------------------------------------------------------
    // The bare shell (`Assets::Shell`).
    // -------------------------------------------------------------------------------------

    /// The bare shell, on an [`Assets::Shell`] client, and `None` on any other.
    ///
    /// A scenario reaches the current screen through it -- `shell.flow.current_mut()` and a
    /// downcast -- which is what every login-flow scenario does. The element tree is
    /// better read through [`Self::ui_snapshot`].
    pub fn shell_mut(&mut self) -> Option<&mut dereth_client_shell::ui::UiShell> {
        match &mut self.backend {
            Backend::Model(m) => m.shell.as_mut().map(|s| &mut s.ui),
            Backend::App(_) => None,
        }
    }

    /// The bare shell, or a panic naming the spec the scenario should have asked for.
    ///
    /// # Panics
    /// Panics on any backend but [`Assets::Shell`].
    pub fn expect_shell(&mut self) -> &mut dereth_client_shell::ui::UiShell {
        assert!(
            matches!(self.spec.assets, Assets::Shell),
            "this step drives the bare UI shell, so the scenario must be built with \
             ClientSpec::shell(host)"
        );
        self.shell_mut()
            .expect("an Assets::Shell client has a shell")
    }

    /// What the application is telling the shell, to change between frames.
    ///
    /// This is the step the login-flow scenarios are written in: *"and now the shard sends a
    /// second, identical character set"* is `host_mut().character_set_notices += 1`, and an `App`
    /// has no way to be told it because an `App` computes its host state from its own link.
    ///
    /// # Panics
    /// Panics on any backend but [`Assets::Shell`].
    pub fn host_mut(&mut self) -> &mut dereth_client_contract::pregame::PregameView {
        assert!(
            matches!(self.spec.assets, Assets::Shell),
            "the host state is the Assets::Shell backend's; an App computes its own every frame, \
             so a scenario that wrote one here would be asserting over a frame the client did not \
             build"
        );
        match &mut self.backend {
            Backend::Model(m) => {
                &mut m
                    .shell
                    .as_mut()
                    .expect("an Assets::Shell client has a shell")
                    .host
            }
            Backend::App(_) => unreachable!("checked above"),
        }
    }

    /// Everything the bare shell has asked the host to do, in request order, including shutdown,
    /// login, and character requests. Empty on every other backend, where the `App` answers them itself.
    #[must_use]
    pub fn shell_requests(&self) -> &[UiRequest] {
        match &self.backend {
            Backend::Model(m) => m.shell.as_ref().map_or(&[][..], |s| &s.requests),
            Backend::App(_) => &[],
        }
    }

    /// Deliver `events` the way the frame does -- the objects, then the HUD, then the interaction
    /// layer **with the frame's own panels callback** -- and record everything that came back.
    ///
    /// **The panels callback must be the real one.** `app.probe_mut().apply_interaction_events` passes
    /// `&mut |_, _, _| {}` as its `panels` argument, while `App::frame` delivers through
    /// `apply_events_at_boundary` with the real one. Delivering through the former would leave
    /// every shipped panel that is a cached join of the notice stream -- the enchantment pane, the
    /// vitae line, the paper doll -- **blind to [`crate::Inbound`]**, and a scenario asserting that
    /// one of them had not changed would pass for the wrong reason. So this goes through
    /// `App::apply_interaction_events_to_panels`, which is `App::frame`'s own two lines.
    ///
    /// On the model backend the callback stays a no-op, and honestly: that backend has no shell,
    /// so there is no panel to notify.
    pub(crate) fn deliver(&mut self, events: &[SessionEvent], now: LocalTime) {
        match &mut self.backend {
            Backend::App(app) => {
                for e in events {
                    app.probe_mut().objects_mut().apply_event(e, now);
                }
                let chat = app.apply_hud_events(events);
                app.apply_interaction_events_to_panels(events);
                self.log.chat.extend(chat);
            }
            Backend::Model(m) => {
                for e in events {
                    m.objects.apply_event(e, now);
                }
                let chat = m.hud.apply_events_with_combat_mode_handler(
                    events,
                    &mut m.objects.world,
                    m.shell.as_mut().map(|s| &mut s.ui.ui.requests),
                    &mut |_, _| {},
                );
                dereth_client_runtime::interaction::apply_events(
                    &mut m.interaction,
                    events,
                    &mut m.objects.world,
                );
                self.log.chat.extend(chat);
            }
        }
        self.drain();
    }

    /// Feed one recorded datagram through the client's **real transport** and run the frame that
    /// consumes it, at the clock this backend must use (see [`crate::replay`]), entering the world
    /// when the shard's character set arrives and `entered` is still false.
    ///
    /// It is `pub(crate)` because a scenario reaches it through
    /// [`crate::Inbound::from_raw_capture`], which is the step; this is the pass that step is
    /// written in terms of.
    pub(crate) fn replay_datagram(
        &mut self,
        raw: Option<(&[u8], std::net::SocketAddr)>,
        recorded_t: f64,
        entered: &mut bool,
    ) {
        match &mut self.backend {
            Backend::App(app) => {
                // A datagram the recorded *client* sent is not delivered to this one; it is only
                // a point on the recording's clock, and this backend runs on its own.
                let Some((raw, from)) = raw else { return };
                // The recorded timestamps are not this client's clock; see `crate::replay`.
                let now = LocalTime(app.clock().local_time);
                app.replay_network_mut()
                    .expect(
                        "a raw-capture replay needs an endpoint; give the scenario a recorded \
                         login or attach one before the step",
                    )
                    .feed(raw, from, now);
                assert!(app.frame(), "the client shut itself down during the replay");
                let net = app.replay_network_mut().expect("the endpoint is attached");
                if !*entered
                    && net.session_state()
                        == dereth_client_net::client_session::SessionState::CharacterSelect
                {
                    let set = net.characters().clone();
                    if let Some(c) = set.characters.first() {
                        net.enter_world(c.gid, &set.account);
                        *entered = true;
                    }
                }
            }
            Backend::Model(m) => {
                let now = LocalTime(recorded_t);
                m.now = recorded_t;
                let net = m.net.as_mut().expect(
                    "a raw-capture replay needs an endpoint; give the scenario a recorded login \
                     or attach one before the step",
                );
                // **A client-to-server datagram is still a point on the clock.** It is not fed --
                // this client is not the one that sent it -- but the sweep that gives up on a
                // silent shard measures against the time a *pass* happens at, so a loop that
                // skipped these entirely would never reach the moment the shard's silence is long
                // enough. That is the whole subject of the error-condition recordings.
                if let Some((raw, from)) = raw {
                    net.feed(raw, from, now);
                }
                net.tick(now);
                let events = m.objects.pump(net, now);
                if !*entered {
                    for e in &events {
                        if let SessionEvent::CharacterSet(set) = e {
                            if let Some(c) = set.characters.first() {
                                let account = set.account.clone();
                                let gid = c.gid;
                                m.net
                                    .as_mut()
                                    .expect("the endpoint is attached")
                                    .enter_world(gid, &account);
                                *entered = true;
                            }
                        }
                    }
                }
                let chat = m.hud.apply_events_with_combat_mode_handler(
                    &events,
                    &mut m.objects.world,
                    m.shell.as_mut().map(|s| &mut s.ui.ui.requests),
                    &mut |_, _| {},
                );
                dereth_client_runtime::interaction::apply_events(
                    &mut m.interaction,
                    &events,
                    &mut m.objects.world,
                );
                self.log.chat.extend(chat);
            }
        }
        self.log.frames += 1;
        self.drain();
    }

    /// The millisecond stamp the next pointer gesture is delivered at, two seconds on from the
    /// last -- well clear of the double-click window, so one press is never read as a double click
    /// of itself. See [`crate::player`].
    pub(crate) fn next_pointer_time(&mut self) -> u32 {
        self.log.pointer_time += 2_000;
        self.log.pointer_time
    }

    /// The scenario's own notice sink, for a step that drives the game model directly.
    pub(crate) fn absorb_sink(&mut self, sink: RecordingSink) {
        self.log.notices.extend(sink.0);
    }

    pub(crate) fn note_ui_requests(&mut self, r: &[UiRequest]) {
        self.log.ui_requests.extend(r.iter().cloned());
    }

    pub(crate) fn note_requests(&mut self, r: Vec<Request>) {
        self.log.requests.extend(r);
    }

    /// Take everything the client has queued up since the last look.
    fn drain(&mut self) {
        let taken = match &mut self.backend {
            // **The App's outbox is not a queue: the frame replaces it every pass.** So it is
            // read once per frame and not once per `drain` -- a `when` followed by a `tick`
            // drains twice between two frames, and reading per drain would append the same
            // outbox twice. The frame index is the client's own, so this counts
            // frames the client ran rather than steps a scenario wrote.
            Backend::App(app) => {
                let at = app.frame_events().frame_index();
                if self.log.last_drained_frame == Some(at) {
                    Vec::new()
                } else {
                    self.log.last_drained_frame = Some(at);
                    app.interaction().last_sent.clone()
                }
            }
            // The model backend's is a queue and taking it empties it, so a second look in the
            // same frame honestly answers nothing.
            Backend::Model(m) => m.interaction.take_pending_requests(),
        };
        self.log.requests.extend(taken);
    }
}

/// One pass of the model half of a frame.
///
/// With an endpoint attached this is the pump, the HUD and the interaction layer, which is what the
/// frame's model half is. **With no endpoint it also runs the game model's own maintenance tick**,
/// because that is what the pump would have run: the twenty-second sweep over dangling references
/// lives inside it, and a model scenario with no network would otherwise have no way to reach it.
fn model_frame(m: &mut Model) -> (Vec<ChatMessage>, Vec<Notice>, Vec<Request>) {
    m.now += HEADLESS_STEP;
    let now = LocalTime(m.now);
    let mut notices = RecordingSink::default();
    let mut requests = dereth_client_model::RecordingRequests::default();
    let events = if let Some(net) = m.net.as_mut() {
        net.tick(now);
        // **The written datagrams are not dropped here**: `HeadlessClient::outbound_wire` reads
        // that queue, and emptying it would make the reader blind on this backend. Nothing sends them: the endpoint has no socket, so they
        // stay queued until the wire reader takes them.
        m.objects.pump(net, now)
    } else {
        m.objects.world.use_time(
            dereth_primitives::ServerTime(m.now),
            &mut notices,
            &mut requests,
        );
        Vec::new()
    };
    let chat = m.hud.apply_events_with_combat_mode_handler(
        &events,
        &mut m.objects.world,
        m.shell.as_mut().map(|s| &mut s.ui.ui.requests),
        &mut |_, _| {},
    );
    dereth_client_runtime::interaction::apply_events(
        &mut m.interaction,
        &events,
        &mut m.objects.world,
    );
    shell_frame(m);
    (chat, notices.0, requests.0)
}

/// One pass of the bare shell, on an [`Assets::Shell`] client. Nothing on any other backend.
///
/// `UiShell::frame` is the whole of what the login-flow scenarios drive, and the requests it
/// answers with are what the application would act on; they are accumulated rather than dropped so
/// that [`HeadlessClient::shell_requests`] can answer "and the shell asked the host to do this".
fn shell_frame(m: &mut Model) {
    let Some(shell) = m.shell.as_mut() else {
        return;
    };
    let mut pump = dereth_ui::NullInputPump;
    let out = shell.ui.frame(LocalTime(m.now), &shell.host, &mut pump);
    shell.requests.extend(out);
}

/// A player character with a pack, and nothing else -- the world every synthesised model scenario
/// starts from.
fn seed_player(w: &mut World, id: ObjectId) {
    use dereth_protocol::types::PublicWeenieDesc;
    use {dereth_rules::weenie::bitfield, dereth_rules::weenie::item_type};

    let mut it = dereth_client_model::Weenie::new(id);
    it.pwd = PublicWeenieDesc {
        bitfield: bitfield::PLAYER,
        obj_type: item_type::CREATURE,
        items_capacity: Some(102),
        containers_capacity: Some(7),
        ..PublicWeenieDesc::default()
    };
    it.valid = true;
    w.tables.weenies.insert(id, it);
    w.player = Some(id);
    w.tables
        .inventories
        .insert(id, dereth_client_model::objects::ObjectInventory::new(id));
}

/// A bare [`dereth_client_shell::ui::UiShell`] over the retail dats, settled to the spec's mode if it
/// named one.
///
/// There is no `App` here and nothing that could open a socket: the shell reads a `HostState` and
/// the dats, and writes an element tree.
fn build_shell(spec: &ClientSpec) -> (Arc<RetailDatStore>, ShellOnly) {
    let dir = dereth_dat::testing::dat_dir();
    let store = Arc::new(RetailDatStore::open_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "a shell scenario needs the retail dats under $DERETH_TEST_DAT_DIR ({}): {e}",
            dir.display()
        )
    }));
    #[allow(clippy::cast_possible_wrap)]
    let display = (spec.width as i32, spec.height as i32);
    let ui = dereth_client_shell::ui::UiShell::new(&store, display)
        .unwrap_or_else(|e| panic!("the UI shell comes up over the retail dats: {e}"));
    let host = spec.host.as_deref().cloned().unwrap_or_default();
    let mut shell = ShellOnly {
        ui,
        host,
        requests: Vec::new(),
    };
    if let Some(mode) = spec.ui_mode {
        shell.ui.queue(mode);
    }
    // The flow is settled here rather than by the caller's first `tick` so that a scenario's own
    // frame count starts at the screen it asked for.
    let mut pump = dereth_ui::NullInputPump;
    for f in 0..spec.settle_frames {
        #[allow(clippy::cast_precision_loss)]
        let now = LocalTime(f as f64 * HEADLESS_STEP);
        let out = shell.ui.frame(now, &shell.host, &mut pump);
        shell.requests.extend(out);
    }
    if let Some(mode) = spec.ui_mode {
        assert_eq!(
            shell.ui.flow.current_mode(),
            Some(mode),
            "the flow did not reach the mode this scenario asked for in {} frames",
            spec.settle_frames
        );
    }
    (store, shell)
}

/// `App::with_platform(cfg, NullPresentation, Platform::headless(w, h))`, plus whatever of the
/// shell the spec asked for.
fn build_app(spec: &ClientSpec) -> App {
    let cfg = Config {
        headless: true,
        frames: None,
        // No socket is ever opened: the only endpoint a scenario has is the socket-free one
        // `App::attach_replay_network` installs.
        connect: false,
        sound: false,
        ui: spec.shell,
        width: spec.width,
        height: spec.height,
        dat_dir: dereth_dat::testing::dat_dir(),
        // **A scenario may name a disposable directory.** The default is a temporary path that
        // is never created -- a client that writes nothing must not touch the user's settings
        // folder -- and `ClientSpec::with_settings_dir` is how a scenario whose
        // claim *is* what the client wrote gets a directory it may have.
        preferences_file: spec
            .settings_dir
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("dere-scenario-not-created"))
            .join("prefs.ini"),
        ..Config::default()
    };
    let mut app = App::with_platform(
        cfg,
        Box::new(NullPresentation::new(spec.width, spec.height)),
        Platform::headless(spec.width, spec.height),
    )
    .unwrap_or_else(|e| {
        panic!("a headless client needs the retail dats under $DERETH_TEST_DAT_DIR and nothing else: {e}")
    });
    if spec.shell {
        app.start_shell().expect("the UI shell comes up");
    }
    if spec.static_scene {
        let s = dereth_client_runtime::scene::SceneConfig {
            landblock: app.config().landblock,
            land_radius: app.config().land_radius,
            scenery_radius: app.config().scenery_radius,
            ..dereth_client_runtime::scene::SceneConfig::default()
        };
        app.load_static_scene(s).expect("the static scene loads");
    }
    if let Some(mode) = spec.ui_mode {
        app.queue_ui_mode(mode);
        for _ in 0..spec.settle_frames {
            assert!(
                app.frame(),
                "the client shut itself down while settling {mode:?}"
            );
        }
    }
    if let Some(page) = spec.open_page {
        open_page(&mut app, page);
    }
    app
}

/// Raise one of the gameplay screen's registered panel pages, through the screen's own
/// `recv_set_panel_visibility` -- which is what the toolbar button ends in.
///
/// See [`ClientSpec::with_open_page`]. Three frames, enough for the page to settle.
fn open_page(app: &mut App, page: dereth_ui::ElementId) {
    {
        let shell = app.ui_mut().unwrap_or_else(|| {
            panic!(
                "ClientSpec::with_open_page raises a page of the gameplay panel bar, so the \
                 scenario's spec must bring the UI shell up"
            )
        });
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a screen is current");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .unwrap_or_else(|| {
                panic!(
                    "ClientSpec::with_open_page raises a page of the gameplay panel bar, so the \
                     gameplay screen must be the current one"
                )
            });
        let panel_id = gameplay
            .panels
            .pages
            .iter()
            .find(|p| p.element == page)
            .map(|p| p.panel_id)
            .unwrap_or_else(|| panic!("{page:?} is one of the shipped registered pages"));
        gameplay.recv_set_panel_visibility(ui, panel_id, true);
    }
    for _ in 0..3 {
        assert!(
            app.frame(),
            "the client shut itself down while raising a panel page"
        );
    }
}
