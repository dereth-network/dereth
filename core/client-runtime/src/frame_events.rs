//! The frame's event log, and the one store behind `App`'s test-only counters.
//!
//! [`crate::app::App`] has ~30 `pub fn`s whose only callers are under `tests/`:
//! `actions_routed`, `frames_drawn`, `world_resets`, `stream_failures` and the rest. As separate
//! `u64` fields each would say *how many* without saying *which*, *when*, or *in what order*.
//!
//! So they are one append-only log. Every counted site pushes a `FrameEvent` at the place and
//! under the condition it counts, and each accessor is a one-line read over the log. What the log
//! adds over plain counters is the ordering and a stable one-line text form per event
//! (`FrameEvent`'s [`std::fmt::Display`]) that a scenario can diff against a committed golden
//! file.
//!
//! Plain data only. No handle, address, pointer or borrowed object appears in a variant, which is
//! what lets the whole log be `Copy`-cheap, comparable, and printable without a device.
//!
//! # What is kept, and for how long
//!
//! Three stores, because the three questions have different lifetimes:
//!
//! * **totals** — one `u64` per `FrameEventKind`, for the whole process. This is what the
//!   accessors read, so it can never be trimmed.
//! * **amounts** — one `u64` per kind, summing `FrameEvent::amount`. Four counts advance by
//!   more than one (`world_textures_released` adds the slots released), and this is where that
//!   quantity lives.
//! * **the ring** — the events themselves, for the last `ring_frames` frames plus the frame in
//!   progress. Text, order and `FrameEvents::since` read this. It is bounded because a station
//!   that runs ten thousand frames must not grow a buffer per frame, and because nothing needs an
//!   event's *payload* from an hour ago — the totals already carry the count.
//!
//! `FrameEvents::last` keeps the most recent event of each kind for ever, which is how
//! `App::last_render_pref_work` survives the trim.

use std::collections::VecDeque;

use crate::frame::FrameStep;
/// A landscape style refused for want of its files: the files it needed and the style kept.
pub type LandscapeRefusal = (
    dereth_client_contract::options::landscape::RequiredFiles,
    Option<dereth_client_contract::options::landscape::RegionStyle>,
);

/// What one `WorldScene::update_from_preferences` poll actually did.
///
/// Every field is the state of one preference poll's own local
/// flags, and they exist so that "the option changed and the renderer did nothing" and "the
/// option did not change" cannot print alike.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderPrefWork {
    /// Either texture-detail level moved, so every landblock was released and rebuilt.
    pub flushed: bool,
    /// `Render.LandscapeDrawDistance` moved, so the smart-box middle radius changed.
    pub mid_radius_changed: bool,
    /// The compare — `Render.BuildingDetailTextures` moved. **Counted and not
    /// acted on**: this build has no detail-texture pass. See
    /// `WorldScene::update_from_preferences` for the retail chain it would reach.
    pub detail_texturing_changed: bool,
    /// Window slots the rebuild queued.
    pub blocks_queued: usize,
    /// Blocks resident once the rebuild had streamed.
    pub blocks_rebuilt: usize,
    /// How many of the four detail classes hold a surface after the apply —
    /// `2` (building and environment) with the preference on, `0` with it off. It is the
    /// difference between "the poll noticed" ([`Self::detail_texturing_changed`]) and "the
    /// subsystem did something", which the other fields cannot distinguish.
    pub detail_surfaces: usize,
    /// `[Render] Ground` moved and the ground was rebuilt with the new land surface.
    pub ground_changed: bool,
    /// `[Render] Sky` moved and the sky, its light and its fog were rebuilt.
    pub sky_changed: bool,
    /// `[Render] Ground` named a style whose files are not present: the files it needed, and the
    /// style the ground kept (`None` is the world's own). The preference went back to that style.
    pub ground_refused: Option<LandscapeRefusal>,
    /// The same for `[Render] Sky`.
    pub sky_refused: Option<LandscapeRefusal>,
    /// `[Render] Objects` moved and every object, the body and the landscape's objects were
    /// rebuilt with the new look.
    pub objects_changed: bool,
    /// The same as [`Self::ground_refused`] for `[Render] Objects`.
    pub objects_refused: Option<LandscapeRefusal>,
    /// `[Render] Objects` asks for another era's look whose verdicts the application is still
    /// working out: the objects keep the look they have until they arrive.
    pub objects_waiting: bool,
}

/// Which of `App::apply_input_actions`'s five consumers took an input action.
///
/// Action dispatch in the order the `continue`s are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionRoute {
    /// `try_finish_jump_from_escape` — Escape landed a jump that was in the air.
    EscapeFinishedJump,
    /// `apply_jump_action` — `CommenceJump` / `DoJump`.
    Jump,
    /// `MovementCommands::on_action`.
    Movement,
    /// `crate::actions::camera::on_action` — the commands that need no world.
    Camera,
    /// `apply_world_camera_action` — the eight camera commands that do.
    WorldCamera,
}

/// Which half of `App::stream_world` failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamStage {
    /// The per-frame render-preference poll.
    RenderPreferences,
    /// The landblock window's own build.
    Landblocks,
}

/// Why a server teleport found no body to move. See `App::player_teleport_use_time`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoBodyReason {
    /// No local body at all: the whole pending journal was discarded rather than replayed late.
    JournalDiscarded,
    /// No scene, so no motion/teleport callback exists to replay on.
    NoScene,
    /// A scene with no character in it.
    NoCharacter,
}

/// Which call handed control of the player to the server. Both are the same hand-over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlLossSite {
    /// `command_interpreter_control_transfer`'s own accepted call, once per frame.
    Transfer,
    /// The movement setter's accepted-player return, inside an admitted movement dispatch.
    MovementDispatch,
}

/// One thing a frame did.
///
/// Every variant is either a [`FrameStep`] the frame performed or a thing one of `App`'s
/// counters counts. Each accessor projects its total from the corresponding variant in this log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameEvent {
    /// One operation in the client tick, pushed immediately before it is performed.
    /// [`crate::frame::FrameRecorder`] reads these.
    Step(FrameStep),
    /// Frame completion returned and the frame was counted in `App::frames_drawn`.
    FrameDrawn,
    /// Input actions routed into the body or the camera this session.
    ///
    /// `App::actions_routed` is this kind's total; the payload says which of the five consumers
    /// took it, which the counter could not.
    ActionRouted(ActionRoute),
    /// Motion commands issued through
    /// the interpreter this session -- a typed `*wave*` reaching the same
    /// keyboard-command handler the `J` key does.
    PoseMotionIssued,
    /// Input-dispatch passes containing the jump action owner. `App::jump_use_times`.
    JumpUseTime,
    /// Pending-charge releases which attempted the actual body's jump.
    ///
    /// `App::jump_counts` is this kind's total and its `FrameEvent::amount`: jump releases the
    /// body refused, so no `0xF61B` was produced, are the ones with a non-zero `status`.
    JumpRequested {
        /// `crate::jump::Release::status`; non-zero is a refusal.
        status: u32,
    },
    /// Automatic-attack aborts triggered by a new forward movement this session, and the
    /// `Combat_CancelAttack` (`0x01B7`) requests those calls produced.
    ///
    /// Two events, because the abort is **state-gated**: the original client sends nothing
    /// unless an attack is actually in flight, so *the edge was consumed* and *a cancel went out*
    /// are different findings and a single number cannot tell them apart. That gate is the whole
    /// reason ordinary walking does not spray `0x01B7` at the shard.
    NewForwardAttackAborted,
    /// One `0x01B7` the abort produced — `App::new_forward_attack_aborts`'s second half.
    NewForwardAttackCancelSent,
    /// How many player-options saves put a `0x01A1` on
    /// the wire from `App::log_off_character` — the flush retail does on the way out. Not the
    /// number of logouts: a session that deferred nothing sends nothing.
    PlayerModuleSavedAtLogout,
    /// Body turns taken off `CameraEffects` and given to the command
    /// interpreter this session. See `App::apply_camera_turn`. `App::camera_turns_applied`.
    WorldCameraTurnApplied,
    /// How many full smart-box resets have been performed.
    ///
    /// The instrument for the teardown: a reset that runs and a reset that is skipped are
    /// otherwise indistinguishable from outside, and "the world is `None`" is a fact about a
    /// function rather than about a second login. `App::world_resets` is this kind's total.
    WorldReset {
        /// Texture slots `App::reset_world_view` has handed back. Summed over the
        /// run by `FrameEvent::amount`, which is `App::world_textures_released`.
        ///
        /// Observable texture release during land teardown. It is counted rather than assumed
        /// because a teardown that drops the scene without calling
        /// `WorldScene::release_textures` is invisible until the descriptor heap runs out several
        /// logins later.
        textures_released: u32,
    },
    /// How many times a streamed landblock failed to build. Asserted on by the tests: a decode or
    /// a device failure that is merely tolerated is a gap that hides inside a green suite.
    /// `App::stream_failures` is this kind's total; the payload says which half failed.
    StreamFailed(StreamStage),
    /// Render-preference updating returned, whether or not anything changed.
    ///
    /// What that poll last did, for the tests and the report line: the payload of
    /// the most recent one of these is `App::last_render_pref_work`.
    RenderPreferencesPolled(RenderPrefWork),
    /// Frames on which the render-preference poll found a
    /// scene-owned `Render.*` preference had moved and did something about it. Zero for a session
    /// in which nobody touched the options page, which is what makes it a usable instrument.
    /// `App::render_pref_applies`.
    RenderPreferencesApplied,
    /// How many times `App::position_use_time` has run, counted **before** its
    /// early return. `App::position_use_times`.
    ///
    /// This is the wiring assertion, and it exists because a producer nothing calls is an easy
    /// defect to miss: with no body and no link the reporter's own counters stay zero,
    /// so they cannot tell "the frame reached it" from "the frame never called it". A headless
    /// `App` has neither, and this event is the only thing that can see the call site.
    PositionUseTime,
    /// How many frames have reached `App::player_teleport_use_time`.
    /// `App::player_teleport_use_times`.
    PlayerTeleportUseTime,
    /// Server teleports that moved this client's body.
    /// `App::player_teleports_applied`.
    PlayerTeleportApplied,
    /// Server teleports that arrived with no body to move.
    /// `App::player_teleports_before_a_body` is this kind's `FrameEvent::amount`, because one
    /// discarded journal can cover several of them.
    PlayerTeleportBeforeABody {
        /// How many dispatches this one drop covered.
        count: u64,
        /// What was missing.
        reason: NoBodyReason,
    },
    /// Frames that reached `App::command_interpreter_control_transfer`.
    /// `App::control_transfer_counts`, first.
    ControlTransfer,
    /// Loss of local control — one per
    /// non-autonomous movement buffer the server addressed to this player.
    /// `App::control_transfer_counts`, second.
    ServerControlLost(ControlLossSite),
    /// The command interpreter retaking local control. Fewer than
    /// [`FrameEvent::ServerControlLost`] by design: an auto-runner's lock is cancelled by
    /// turning auto-run off inside the loss, so nothing is ever handed back.
    /// `App::control_transfer_counts`, third.
    ServerControlRetaken,
    /// UI visibility-toggle calls made on behalf of
    /// `EscapeKey`'s "nothing is selected" leg. Counted **here** and not on `Interaction`, because
    /// `Interaction`'s counter says the arm asked and this one says the UI was there to answer —
    /// which is exactly the difference between a behaviour that is transcribed and one that is
    /// wired.
    /// `App::action_arm_host_stats`, first and second.
    EscapeOptionsToggle {
        /// Of those, how many **answered `true`** — the visibility-toggle action's own
        /// return, i.e. the action had a listener bucket. `0x1000001B` does have one
        /// in the shipped `classic_gameplay` tree (measured); `0x1000001E ToggleRadarPanel` does
        /// **not**. Summed by `FrameEvent::amount`.
        answered: bool,
    },
    /// Complete-stop calls made on behalf of `EscapeKey`'s stop leg,
    /// for the same reason. `App::action_arm_host_stats`, third.
    EscapeStopPerformed,
    /// Screenshot writes that answered **true**, i.e. a PNG on disk.
    /// `App::action_arm_host_stats`, fourth.
    ScreenshotSaved,
    /// The ones that answered **false**. Retail's arm returns `TRUE` either way and prints
    /// nothing, so this event is the only thing that separates the two here.
    /// `App::action_arm_host_stats`, fifth.
    ScreenshotFailed,
}

/// The discriminant of a `FrameEvent`, with no payload: the key the totals are kept under.
///
/// It is a separate enum rather than `std::mem::discriminant` because the totals are an array and
/// need an index, and because [`FrameEventKind::name`] is the stable text of every log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(usize)]
pub enum FrameEventKind {
    /// [`FrameEvent::Step`].
    Step,
    /// [`FrameEvent::FrameDrawn`].
    FrameDrawn,
    /// [`FrameEvent::ActionRouted`].
    ActionRouted,
    /// [`FrameEvent::PoseMotionIssued`].
    PoseMotionIssued,
    /// [`FrameEvent::JumpUseTime`].
    JumpUseTime,
    /// [`FrameEvent::JumpRequested`].
    JumpRequested,
    /// [`FrameEvent::NewForwardAttackAborted`].
    NewForwardAttackAborted,
    /// [`FrameEvent::NewForwardAttackCancelSent`].
    NewForwardAttackCancelSent,
    /// [`FrameEvent::PlayerModuleSavedAtLogout`].
    PlayerModuleSavedAtLogout,
    /// [`FrameEvent::WorldCameraTurnApplied`].
    WorldCameraTurnApplied,
    /// [`FrameEvent::WorldReset`].
    WorldReset,
    /// [`FrameEvent::StreamFailed`].
    StreamFailed,
    /// [`FrameEvent::RenderPreferencesPolled`].
    RenderPreferencesPolled,
    /// [`FrameEvent::RenderPreferencesApplied`].
    RenderPreferencesApplied,
    /// [`FrameEvent::PositionUseTime`].
    PositionUseTime,
    /// [`FrameEvent::PlayerTeleportUseTime`].
    PlayerTeleportUseTime,
    /// [`FrameEvent::PlayerTeleportApplied`].
    PlayerTeleportApplied,
    /// [`FrameEvent::PlayerTeleportBeforeABody`].
    PlayerTeleportBeforeABody,
    /// [`FrameEvent::ControlTransfer`].
    ControlTransfer,
    /// [`FrameEvent::ServerControlLost`].
    ServerControlLost,
    /// [`FrameEvent::ServerControlRetaken`].
    ServerControlRetaken,
    /// [`FrameEvent::EscapeOptionsToggle`].
    EscapeOptionsToggle,
    /// [`FrameEvent::EscapeStopPerformed`].
    EscapeStopPerformed,
    /// [`FrameEvent::ScreenshotSaved`].
    ScreenshotSaved,
    /// [`FrameEvent::ScreenshotFailed`].
    ScreenshotFailed,
}

/// How many `FrameEventKind`s there are — the length of the totals arrays.
///
/// `FrameEventKind::ALL` is checked against it by a unit test, so adding a variant and
/// forgetting this constant is a red, not a silent out-of-bounds.
pub const KIND_COUNT: usize = 25;

impl FrameEventKind {
    /// Every kind, in declaration order. The totals arrays are this long.
    pub const ALL: &'static [FrameEventKind] = &[
        FrameEventKind::Step,
        FrameEventKind::FrameDrawn,
        FrameEventKind::ActionRouted,
        FrameEventKind::PoseMotionIssued,
        FrameEventKind::JumpUseTime,
        FrameEventKind::JumpRequested,
        FrameEventKind::NewForwardAttackAborted,
        FrameEventKind::NewForwardAttackCancelSent,
        FrameEventKind::PlayerModuleSavedAtLogout,
        FrameEventKind::WorldCameraTurnApplied,
        FrameEventKind::WorldReset,
        FrameEventKind::StreamFailed,
        FrameEventKind::RenderPreferencesPolled,
        FrameEventKind::RenderPreferencesApplied,
        FrameEventKind::PositionUseTime,
        FrameEventKind::PlayerTeleportUseTime,
        FrameEventKind::PlayerTeleportApplied,
        FrameEventKind::PlayerTeleportBeforeABody,
        FrameEventKind::ControlTransfer,
        FrameEventKind::ServerControlLost,
        FrameEventKind::ServerControlRetaken,
        FrameEventKind::EscapeOptionsToggle,
        FrameEventKind::EscapeStopPerformed,
        FrameEventKind::ScreenshotSaved,
        FrameEventKind::ScreenshotFailed,
    ];

    /// This kind's index into the totals.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The first word of the event's text line. Stable: a golden file names these.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Step => "step",
            Self::FrameDrawn => "frame-drawn",
            Self::ActionRouted => "action-routed",
            Self::PoseMotionIssued => "pose-motion-issued",
            Self::JumpUseTime => "jump-use-time",
            Self::JumpRequested => "jump-requested",
            Self::NewForwardAttackAborted => "new-forward-attack-aborted",
            Self::NewForwardAttackCancelSent => "new-forward-attack-cancel-sent",
            Self::PlayerModuleSavedAtLogout => "player-module-saved-at-logout",
            Self::WorldCameraTurnApplied => "world-camera-turn-applied",
            Self::WorldReset => "world-reset",
            Self::StreamFailed => "stream-failed",
            Self::RenderPreferencesPolled => "render-preferences-polled",
            Self::RenderPreferencesApplied => "render-preferences-applied",
            Self::PositionUseTime => "position-use-time",
            Self::PlayerTeleportUseTime => "player-teleport-use-time",
            Self::PlayerTeleportApplied => "player-teleport-applied",
            Self::PlayerTeleportBeforeABody => "player-teleport-before-a-body",
            Self::ControlTransfer => "control-transfer",
            Self::ServerControlLost => "server-control-lost",
            Self::ServerControlRetaken => "server-control-retaken",
            Self::EscapeOptionsToggle => "escape-options-toggle",
            Self::EscapeStopPerformed => "escape-stop-performed",
            Self::ScreenshotSaved => "screenshot-saved",
            Self::ScreenshotFailed => "screenshot-failed",
        }
    }
}

impl std::fmt::Display for FrameEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl FrameEvent {
    /// Which counter this event advances.
    #[must_use]
    pub const fn kind(self) -> FrameEventKind {
        match self {
            Self::Step(_) => FrameEventKind::Step,
            Self::FrameDrawn => FrameEventKind::FrameDrawn,
            Self::ActionRouted(_) => FrameEventKind::ActionRouted,
            Self::PoseMotionIssued => FrameEventKind::PoseMotionIssued,
            Self::JumpUseTime => FrameEventKind::JumpUseTime,
            Self::JumpRequested { .. } => FrameEventKind::JumpRequested,
            Self::NewForwardAttackAborted => FrameEventKind::NewForwardAttackAborted,
            Self::NewForwardAttackCancelSent => FrameEventKind::NewForwardAttackCancelSent,
            Self::PlayerModuleSavedAtLogout => FrameEventKind::PlayerModuleSavedAtLogout,
            Self::WorldCameraTurnApplied => FrameEventKind::WorldCameraTurnApplied,
            Self::WorldReset { .. } => FrameEventKind::WorldReset,
            Self::StreamFailed(_) => FrameEventKind::StreamFailed,
            Self::RenderPreferencesPolled(_) => FrameEventKind::RenderPreferencesPolled,
            Self::RenderPreferencesApplied => FrameEventKind::RenderPreferencesApplied,
            Self::PositionUseTime => FrameEventKind::PositionUseTime,
            Self::PlayerTeleportUseTime => FrameEventKind::PlayerTeleportUseTime,
            Self::PlayerTeleportApplied => FrameEventKind::PlayerTeleportApplied,
            Self::PlayerTeleportBeforeABody { .. } => FrameEventKind::PlayerTeleportBeforeABody,
            Self::ControlTransfer => FrameEventKind::ControlTransfer,
            Self::ServerControlLost(_) => FrameEventKind::ServerControlLost,
            Self::ServerControlRetaken => FrameEventKind::ServerControlRetaken,
            Self::EscapeOptionsToggle { .. } => FrameEventKind::EscapeOptionsToggle,
            Self::EscapeStopPerformed => FrameEventKind::EscapeStopPerformed,
            Self::ScreenshotSaved => FrameEventKind::ScreenshotSaved,
            Self::ScreenshotFailed => FrameEventKind::ScreenshotFailed,
        }
    }

    /// How much this event contributes to the quantity its kind carries, for the four counts
    /// that do not advance by one. Everything else answers 1, so its amount is its count.
    ///
    /// * [`Self::WorldReset`] — texture slots released.
    /// * [`Self::PlayerTeleportBeforeABody`] — dispatches dropped.
    /// * [`Self::JumpRequested`] — refusals.
    /// * [`Self::EscapeOptionsToggle`] — toggles that answered TRUE.
    #[must_use]
    pub const fn amount(self) -> u64 {
        match self {
            Self::WorldReset { textures_released } => textures_released as u64,
            Self::PlayerTeleportBeforeABody { count, .. } => count,
            Self::JumpRequested { status } => (status != 0) as u64,
            Self::EscapeOptionsToggle { answered } => answered as u64,
            _ => 1,
        }
    }
}

impl std::fmt::Display for FrameEvent {
    /// One line, stable, no addresses and no pointers: the kind's name, then its detail.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.kind().name())?;
        match self {
            Self::Step(s) => write!(f, " {s:?}"),
            Self::ActionRouted(r) => write!(f, " {}", route_name(*r)),
            Self::JumpRequested { status } => write!(f, " status={status}"),
            Self::WorldReset { textures_released } => write!(f, " textures={textures_released}"),
            Self::StreamFailed(s) => write!(f, " {}", stage_name(*s)),
            Self::RenderPreferencesPolled(w) => write!(
                f,
                " flushed={} mid-radius={} detail-texturing={} queued={} rebuilt={} surfaces={}",
                w.flushed,
                w.mid_radius_changed,
                w.detail_texturing_changed,
                w.blocks_queued,
                w.blocks_rebuilt,
                w.detail_surfaces,
            ),
            Self::PlayerTeleportBeforeABody { count, reason } => {
                write!(f, " count={count} {}", no_body_name(*reason))
            }
            Self::ServerControlLost(s) => write!(f, " {}", loss_site_name(*s)),
            Self::EscapeOptionsToggle { answered } => write!(f, " answered={answered}"),
            _ => Ok(()),
        }
    }
}

const fn route_name(r: ActionRoute) -> &'static str {
    match r {
        ActionRoute::EscapeFinishedJump => "escape-finished-jump",
        ActionRoute::Jump => "jump",
        ActionRoute::Movement => "movement",
        ActionRoute::Camera => "camera",
        ActionRoute::WorldCamera => "world-camera",
    }
}

const fn stage_name(s: StreamStage) -> &'static str {
    match s {
        StreamStage::RenderPreferences => "render-preferences",
        StreamStage::Landblocks => "landblocks",
    }
}

const fn no_body_name(r: NoBodyReason) -> &'static str {
    match r {
        NoBodyReason::JournalDiscarded => "journal-discarded",
        NoBodyReason::NoScene => "no-scene",
        NoBodyReason::NoCharacter => "no-character",
    }
}

const fn loss_site_name(s: ControlLossSite) -> &'static str {
    match s {
        ControlLossSite::Transfer => "transfer",
        ControlLossSite::MovementDispatch => "movement-dispatch",
    }
}

/// How many completed frames of events the ring keeps by default.
pub const DEFAULT_RING_FRAMES: usize = 64;

/// The log. One per `App`.
#[derive(Debug, Clone)]
pub struct FrameEvents {
    /// The frame in progress.
    frame: Vec<FrameEvent>,
    /// The [`FrameStep`]s of the frame in progress, in order — the projection
    /// [`crate::frame::FrameRecorder`] reads, maintained by the
    /// same [`FrameEvents::push`] call. It exists because `App::last_frame_steps` hands out a
    /// `&[FrameStep]`, which no mixed-variant vector can lend.
    steps: Vec<FrameStep>,
    /// Completed frames, oldest first, at most `ring_frames` of them.
    ring: VecDeque<Vec<FrameEvent>>,
    ring_frames: usize,
    /// The index of the frame in progress. The first frame is 0.
    frame_index: u64,
    /// Pushes per kind, for the whole process. Each entry is a counter's value.
    ///
    /// An array rather than a map because [`FrameEvents::total`] has to be a `const fn`: half the
    /// accessors that read it are `pub const fn` and none of them may lose that.
    totals: [u64; KIND_COUNT],
    /// `FrameEvent::amount` summed per kind, for the whole process.
    amounts: [u64; KIND_COUNT],
    /// The most recent event of each kind, kept past the ring's trim.
    last: [Option<FrameEvent>; KIND_COUNT],
}

impl Default for FrameEvents {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameEvents {
    /// An empty log with the default ring depth.
    #[must_use]
    pub fn new() -> Self {
        Self::with_ring_frames(DEFAULT_RING_FRAMES)
    }

    /// An empty log that keeps `frames` completed frames of events.
    #[must_use]
    pub fn with_ring_frames(frames: usize) -> Self {
        Self {
            frame: Vec::new(),
            steps: Vec::new(),
            ring: VecDeque::new(),
            ring_frames: frames,
            frame_index: 0,
            totals: [0; KIND_COUNT],
            amounts: [0; KIND_COUNT],
            last: [None; KIND_COUNT],
        }
    }

    /// Record one event. Totals and amounts advance here and never go back.
    pub fn push(&mut self, event: FrameEvent) {
        let i = event.kind().index();
        self.totals[i] += 1;
        self.amounts[i] += event.amount();
        self.last[i] = Some(event);
        if let FrameEvent::Step(s) = event {
            self.steps.push(s);
        }
        self.frame.push(event);
    }

    /// Close the frame in progress and open the next one. `App::frame` calls this first.
    ///
    /// Returns the closed frame's events, which are also the ring's newest entry.
    pub fn drain_frame(&mut self) -> Vec<FrameEvent> {
        let done = std::mem::take(&mut self.frame);
        self.steps.clear();
        if self.ring_frames > 0 {
            self.ring.push_back(done.clone());
            while self.ring.len() > self.ring_frames {
                self.ring.pop_front();
            }
        }
        self.frame_index += 1;
        done
    }

    /// The events of the frame in progress, in order.
    ///
    /// Once `App::frame` has returned, the
    /// frame in progress *is* the frame that just ran, because nothing has opened the next one.
    #[must_use]
    pub fn last_frame(&self) -> &[FrameEvent] {
        &self.frame
    }

    /// The [`FrameStep`]s of the frame in progress, in call order.
    #[must_use]
    pub fn frame_steps(&self) -> &[FrameStep] {
        &self.steps
    }

    /// The index of the frame in progress; equivalently, how many frames have been closed.
    #[must_use]
    pub const fn frame_index(&self) -> u64 {
        self.frame_index
    }

    /// How many events of this kind have ever been pushed. **This is a counter's value.**
    #[must_use]
    pub const fn total(&self, kind: FrameEventKind) -> u64 {
        self.totals[kind.index()]
    }

    /// `FrameEvent::amount` summed over every event of this kind ever pushed. For the four
    /// kinds that carry a quantity this is the counter's value; for the rest it equals
    /// [`Self::total`].
    #[must_use]
    pub const fn amount(&self, kind: FrameEventKind) -> u64 {
        self.amounts[kind.index()]
    }

    /// The most recent event of this kind, whatever frame it was in.
    #[must_use]
    pub const fn last(&self, kind: FrameEventKind) -> Option<FrameEvent> {
        self.last[kind.index()]
    }

    /// How many **retained** events satisfy `pred` — the ring plus the frame in progress.
    ///
    /// For a whole-run count use [`Self::total`]: the ring is bounded and this is not a total.
    pub fn count(&self, pred: impl Fn(&FrameEvent) -> bool) -> usize {
        self.retained().filter(|e| pred(e)).count()
    }

    /// Every retained event, oldest first.
    pub fn retained(&self) -> impl Iterator<Item = &FrameEvent> {
        self.ring.iter().flatten().chain(self.frame.iter())
    }

    /// The retained events of frames `frame..`, oldest first. Frames older than the ring are
    /// gone; [`Self::first_retained_frame`] says where the answer actually starts.
    #[must_use]
    pub fn since(&self, frame: u64) -> Vec<FrameEvent> {
        let first = self.first_retained_frame();
        let skip = usize::try_from(frame.saturating_sub(first)).unwrap_or(usize::MAX);
        self.ring
            .iter()
            .skip(skip)
            .flatten()
            .chain(self.frame.iter())
            .copied()
            .collect()
    }

    /// The oldest frame index the ring still holds.
    #[must_use]
    pub fn first_retained_frame(&self) -> u64 {
        self.frame_index - self.ring.len() as u64
    }

    /// The frame in progress as text, one event per line — the golden-file form.
    #[must_use]
    pub fn last_frame_text(&self) -> String {
        Self::text(&self.frame)
    }

    /// Any slice of events as text, one per line.
    #[must_use]
    pub fn text(events: &[FrameEvent]) -> String {
        events
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_indexes_itself_and_has_its_own_name() {
        assert_eq!(
            FrameEventKind::ALL.len(),
            KIND_COUNT,
            "KIND_COUNT is out of date"
        );
        let mut names: Vec<&str> = Vec::new();
        for (i, k) in FrameEventKind::ALL.iter().enumerate() {
            assert_eq!(k.index(), i, "{k:?} is out of declaration order");
            names.push(k.name());
        }
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "two kinds share a name");
    }

    #[test]
    fn a_total_is_the_counter_and_an_amount_is_the_quantity() {
        let mut log = FrameEvents::new();
        log.push(FrameEvent::WorldReset {
            textures_released: 3,
        });
        log.push(FrameEvent::WorldReset {
            textures_released: 0,
        });
        // `world_resets` and `world_textures_released` were two fields and are one event.
        assert_eq!(log.total(FrameEventKind::WorldReset), 2);
        assert_eq!(log.amount(FrameEventKind::WorldReset), 3);

        log.push(FrameEvent::JumpRequested { status: 0 });
        log.push(FrameEvent::JumpRequested { status: 7 });
        assert_eq!(log.total(FrameEventKind::JumpRequested), 2);
        assert_eq!(log.amount(FrameEventKind::JumpRequested), 1, "one refusal");
    }

    #[test]
    fn the_ring_trims_the_events_and_never_the_totals() {
        let mut log = FrameEvents::with_ring_frames(2);
        for _ in 0..10 {
            log.push(FrameEvent::FrameDrawn);
            log.drain_frame();
        }
        assert_eq!(
            log.total(FrameEventKind::FrameDrawn),
            10,
            "the counter is whole"
        );
        assert_eq!(
            log.count(|e| *e == FrameEvent::FrameDrawn),
            2,
            "the ring is two deep"
        );
        assert_eq!(log.frame_index(), 10);
        assert_eq!(log.first_retained_frame(), 8);
        assert_eq!(log.since(8).len(), 2);
        assert_eq!(log.since(9).len(), 1);
    }

    #[test]
    fn the_step_projection_is_the_recorders_old_store() {
        let mut log = FrameEvents::new();
        log.push(FrameEvent::Step(FrameStep::ClockSample));
        log.push(FrameEvent::FrameDrawn);
        log.push(FrameEvent::Step(FrameStep::ProcessWindowEvents));
        assert_eq!(
            log.frame_steps(),
            &[FrameStep::ClockSample, FrameStep::ProcessWindowEvents]
        );
        log.drain_frame();
        assert!(log.frame_steps().is_empty());
    }

    #[test]
    fn last_survives_the_trim_which_is_what_last_render_pref_work_needs() {
        let mut log = FrameEvents::with_ring_frames(1);
        let w = RenderPrefWork {
            flushed: true,
            blocks_queued: 4,
            ..RenderPrefWork::default()
        };
        log.push(FrameEvent::RenderPreferencesPolled(w));
        for _ in 0..5 {
            log.drain_frame();
        }
        assert_eq!(
            log.count(|e| matches!(e, FrameEvent::RenderPreferencesPolled(_))),
            0
        );
        assert_eq!(
            log.last(FrameEventKind::RenderPreferencesPolled),
            Some(FrameEvent::RenderPreferencesPolled(w))
        );
    }

    // The serialisation a golden file names. No address, no pointer, one line each.
    #[test]
    fn the_text_form_is_one_stable_line_per_event() {
        let events = [
            FrameEvent::Step(FrameStep::DrawWorld),
            FrameEvent::ActionRouted(ActionRoute::Movement),
            FrameEvent::JumpRequested { status: 0 },
            FrameEvent::WorldReset {
                textures_released: 12,
            },
            FrameEvent::StreamFailed(StreamStage::Landblocks),
            FrameEvent::PlayerTeleportBeforeABody {
                count: 2,
                reason: NoBodyReason::NoCharacter,
            },
            FrameEvent::ServerControlLost(ControlLossSite::Transfer),
            FrameEvent::EscapeOptionsToggle { answered: true },
            FrameEvent::RenderPreferencesPolled(RenderPrefWork::default()),
            FrameEvent::FrameDrawn,
        ];
        assert_eq!(
            FrameEvents::text(&events),
            "step DrawWorld\n\
             action-routed movement\n\
             jump-requested status=0\n\
             world-reset textures=12\n\
             stream-failed landblocks\n\
             player-teleport-before-a-body count=2 no-character\n\
             server-control-lost transfer\n\
             escape-options-toggle answered=true\n\
             render-preferences-polled flushed=false mid-radius=false detail-texturing=false \
             queued=0 rebuilt=0 surfaces=0\n\
             frame-drawn"
        );
        for e in events {
            let line = e.to_string();
            assert!(!line.contains('\n'), "{line} is more than one line");
            assert!(!line.contains("0x"), "{line} carries an address");
        }
    }
}
