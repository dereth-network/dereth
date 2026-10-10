//! The application state machine and the frame loop.
//!
//! The application follows the startup routine's twelve steps and fatal-failure rules, the main
//! loop's exit behavior, and the window class, style, and size used by the device.
//!
//! The main loop is 58 bytes: connect once, then run the per-frame step until it returns false.
//! `App::run` is that loop and `App::frame` is that per-frame step.
//!
//! **What plugs in.** `App` owns everything the frame needs that is not drawing, a device, a UI
//! or an OS window: the session, the object tables, the HUD model, the interaction layer, the
//! body and the camera, the sound mixer and the frame's own event log. Three things are handed
//! to it:
//!
//! * a `Platform` -- the window, the clock, the frame pacer and the error box;
//! * a presentation, [`crate::present::Presentation`] -- the device, or
//!   [`crate::present::NullPresentation`], which draws nothing and counts what it was asked;
//! * a front end, `Shell` -- the modern UI, the cursor, the clipboard and whatever else draws
//!   over the world. `NullShell` is the front end with no UI at all, which is what a headless
//!   run or a client started without its UI has.
//!
//! The frame's order is decided here and nowhere else: every step the front end takes part in is
//! a `Shell` call made at the point in the frame where the retail client makes it, so a front
//! end supplies behaviour and never sequence.

#[cfg(any(test, feature = "test-support"))]
mod probe;
#[cfg(any(test, feature = "test-support"))]
pub use probe::{AppProbe, AppProbeMut};

mod access;
mod display;
mod frame;
mod hud;
mod input;
mod interaction;
mod motion;
mod movement;
mod scripted;
mod session;
mod shutdown;
mod startup;
mod teleport;
mod ui;
mod world;

use motion::command_interpreter_control_transfer_with_finish;
pub use motion::{
    body_motion_in, complete_player_teleport_at, player_timestamps, raw_motion_state_to_wire_in,
};
#[cfg(any(test, feature = "test-support"))]
mod testing;
pub use session::{
    character_set_from_login, chargen_result_to_wire, ddd_interrogation_response, PRODUCT_HIGHRES,
};
#[cfg(any(test, feature = "test-support"))]
pub use testing::{
    apply_player_teleport, body_jump, body_motion, command_interpreter_control_transfer,
    complete_player_teleport, jump_edge_sample, raw_motion_state_to_wire,
};

use dereth_client_contract::window_proc::style;

use dereth_primitives::{AssetSource, DataId};

pub use crate::shell::{NullShell, Shell, UiNotices};
use crate::ui_context::UiContext;
use crate::{
    camera::CameraInput,
    config::Config,
    frame::{FrameRecorder, FrameStep},
    frame_events::{
        ActionRoute, ControlLossSite, FrameEvent, FrameEventKind, FrameEvents, NoBodyReason,
        StreamStage,
    },
    net::NetLink,
    platform::window::{HostEvent, NullWindow, WindowHost},
    present::Presentation,
    pump::Pump,
};

// -------------------------------------------------------------------------------------------
// The shell seam, and the two host calls the Options page's support buttons make
// -------------------------------------------------------------------------------------------

/// One host call made while consuming an open-URL request, as recorded by
/// `record_shell_calls`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCall {
    /// The URL launch. `result` is the value returned by the host or the armed recorder.
    Open { url: String, result: i32 },
    /// The error dialog requested after an unsuccessful launch.
    ErrorBox { title: String, text: String },
}

thread_local! {
    /// `Some` once a test has called `record_shell_calls`: the two host calls are then recorded
    /// instead of performed, so a station can assert that the request reached the shell without a
    /// browser window or a modal dialog appearing on the desktop. The `i32` is what the recorder
    /// answers `ShellExecuteA` with.
    static SHELL_LOG: std::cell::RefCell<Option<(i32, ())>> =
        const { std::cell::RefCell::new(None) };
}

/// Record shell calls on this thread instead of making them, answering the URL launch with `33`,
/// the smallest value that the signed `result > 32` test accepts as success.
///
/// **A test that drives the support buttons must call this first**, or it opens a browser.
#[cfg(any(test, feature = "test-support"))]
pub fn record_shell_calls() {
    record_shell_calls_answering(33);
}

/// `record_shell_calls` with a chosen launch result, so a test can drive the failure leg
/// (`result <= 32`) and inspect the requested error dialog.
#[cfg(any(test, feature = "test-support"))]
pub fn record_shell_calls_answering(result: i32) {
    SHELL_LOG.with(|l| *l.borrow_mut() = Some((result, ())));
}

/// The armed `ShellExecuteA` answer, or `None` when recording is not armed and the real host call
/// must be made.
fn armed_shell_answer() -> Option<i32> {
    SHELL_LOG.with(|l| l.borrow().as_ref().map(|(result, ())| *result))
}

/// Build the failure dialog text from the launch result and URL.
#[must_use]
pub fn shell_error_text(result: i32, url: &str) -> String {
    format!(
        "An error occurred while trying to launch your web browser. (Error code {result})\n\
         The web site to submit an urgent assistance request is listed below. Please go there to \
         complete your request.\n{url}\n"
    )
}

/// One `--set-at` setting, `Section.Name=value`, as the preference it names and the value the
/// preferences file's spelling stands for. The landscape options read every spelling they read
/// from the file; any other registered option is read through the option store's own load, as the
/// file would be. `None` for a name no option has, or a value it cannot take.
fn scripted_preference(setting: &str) -> Option<(&'static str, dereth_client_contract::PrefValue)> {
    use dereth_client_contract::options::{
        classic, interface, landscape, performance, preferences, store,
    };
    let (key, value) = setting.split_once('=')?;
    let key = key.trim();
    // The optional high-fidelity presentation's options take a number, which may ask an effect
    // for a quality level past the boxes' on.
    #[cfg(feature = "hifi")]
    if let Some(name) = dereth_client_contract::options::names::fidelity::NAMES
        .iter()
        .find(|n| n.eq_ignore_ascii_case(key))
    {
        let v = value.trim().parse::<i32>().ok()?;
        return Some((name, dereth_client_contract::PrefValue::Int(v)));
    }
    let name: &'static str = preferences::UI_PREFERENCES
        .iter()
        .map(|p| p.name)
        .chain([
            landscape::GROUND,
            landscape::SKY,
            landscape::OBJECTS,
            interface::INTERFACE,
            performance::PERFORMANCE_PANEL,
        ])
        .chain(classic::NAMES)
        .find(|n| n.eq_ignore_ascii_case(key))?;
    if let Some(v) =
        landscape::parse_value(name, value).or_else(|| interface::parse_value(name, value))
    {
        return Some((name, dereth_client_contract::PrefValue::Int(v)));
    }
    let (section, bare) = name.rsplit_once('.')?;
    let ini = dereth_client_contract::persist::preferences::UserPreferences::parse(&format!(
        "[{section}]\n{bare}={}\n",
        value.trim()
    ))
    .ok()?;
    let (applied, _) = store::load(&ini);
    (applied == 1)
        .then(|| store::inq_value(name))
        .flatten()
        .map(|v| (name, v))
}

/// `cfg` with the drawing, camera and mouse-look options the option store holds: every option a
/// world or a body is built from, where the store has a value for it.
#[must_use]
pub fn with_stored_options(mut cfg: crate::scene::SceneConfig) -> crate::scene::SceneConfig {
    use crate::actions::camera::MouseLookPreferences as Look;
    use crate::camera::CameraPreferences as Camera;
    use dereth_client_contract::options::{landscape, preferences, store};
    use dereth_client_contract::PrefValue;
    let names = preferences::UI_PREFERENCES.iter().map(|p| p.name).chain([
        landscape::GROUND,
        landscape::SKY,
        landscape::OBJECTS,
        store::LANDSCAPE_DETAIL_TEXTURES,
    ]);
    for name in names {
        let Some(v) = store::inq_value(name) else {
            continue;
        };
        cfg.render.set_named(name, &v);
        match (name, v) {
            (Camera::ALIGN_TO_SLOPE, PrefValue::Bool(b)) => cfg.camera.align_to_slope = b,
            (Camera::STIFFNESS, PrefValue::Float(f)) => cfg.camera.stiffness = f,
            (Camera::ADJUSTMENT_SPEED, PrefValue::Float(f)) => cfg.camera.adjustment_speed = f,
            (Look::SENSITIVITY, PrefValue::Float(f)) => cfg.mouse_look.sensitivity = f,
            (Look::SMOOTHING, PrefValue::Float(f)) => cfg.mouse_look.smoothing = f,
            (Look::INVERT_Y, PrefValue::Bool(b)) => cfg.mouse_look.invert_y = b,
            (Look::USE_MOUSE_TURNING, PrefValue::Bool(b)) => cfg.mouse_look.use_mouse_turning = b,
            _ => {}
        }
    }
    cfg
}

/// Whether a preference set while playing is also the next world's, recorded in the scene that
/// will be loaded: the landscape options, and the optional high-fidelity presentation's, which the
/// option store does not hold and [`with_stored_options`] therefore cannot read back.
#[cfg(feature = "hifi")]
pub(crate) fn kept_for_the_next_world(name: &str) -> bool {
    dereth_client_contract::options::landscape::Landscape::of(name).is_some()
        || dereth_client_contract::options::names::fidelity::NAMES.contains(&name)
}

/// Whether a preference set while playing is also the next world's, recorded in the scene that
/// will be loaded: the landscape options, which the option store does not hold and
/// [`with_stored_options`] therefore cannot read back.
#[cfg(not(feature = "hifi"))]
#[inline]
pub(crate) fn kept_for_the_next_world(name: &str) -> bool {
    dereth_client_contract::options::landscape::Landscape::of(name).is_some()
}

/// What the player is told when a box of the optional high-fidelity presentation is ticked and
/// the client draws with a renderer other than `wgpu`, the only one it draws on, and no restart
/// onto `wgpu` is chosen. Otherwise the player is told what the device and the renderer chosen
/// for the next start say ([`dereth_client_contract::options::fidelity::Availability::refusal`]):
/// the presentation draws only on a device asked for it at start-up, when the client starts in the
/// Horizon interface on `wgpu` with a box ticked, so a box first ticked takes effect at that start.
pub const FIDELITY_REFUSED: &str = dereth_client_contract::options::fidelity::REFUSED_RENDERER;

/// Consume `UiRequest::OpenUrl` requests from the two support-ticket buttons.
///
/// Each URL is passed to the desktop's registered handler. A signed result greater than `32`
/// succeeds; every other result, including zero for a missing association, requests an error
/// dialog. The dialog repeats the URL so the player can type the support address by hand.
///
/// Every other request is handed back, the way [`crate::audio::apply_preference_requests`] and
/// the three filters beside it do. The second half of the answer is **every host call this made**,
/// in order, so the caller can see what happened and a station can assert it without a browser
/// window or a modal dialog appearing on the desktop.
///
/// The launcher is installed through [`crate::platform::shell::install_uri_launcher`]. The runtime
/// preserves its signed result and records each [`ShellCall`] in order. The caller handles
/// the returned error-box request; this layer opens no browser or dialog itself. Tests can
/// install a recording launcher and inspect the same result path.
pub fn apply_open_url_requests(
    requests: Vec<dereth_client_contract::UiRequest>,
) -> (Vec<dereth_client_contract::UiRequest>, Vec<ShellCall>) {
    let mut rest = Vec::with_capacity(requests.len());
    let mut calls = Vec::new();
    for r in requests {
        match r {
            dereth_client_contract::UiRequest::OpenUrl(url) => open_url(url, &mut calls),
            other => rest.push(other),
        }
    }
    (rest, calls)
}

/// Apply the open-URL behavior to one URL.
fn open_url(url: &str, calls: &mut Vec<ShellCall>) {
    let result = shell_execute_open(url);
    calls.push(ShellCall::Open {
        url: url.to_owned(),
        result,
    });
    // The comparison is signed, so every error value and zero take the dialog leg.
    if result > 32 {
        return;
    }
    calls.push(ShellCall::ErrorBox {
        title: dereth_client_contract::options::SHELL_EXECUTE_ERROR_TITLE.to_owned(),
        text: shell_error_text(result, url),
    });
}

/// `ShellExecuteA(NULL, "open", url, NULL, NULL, SW_SHOWNORMAL)`'s answer, through the seam.
fn shell_execute_open(url: &str) -> i32 {
    if let Some(answer) = armed_shell_answer() {
        return answer;
    }
    crate::platform::shell::launch_uri(url)
}

/// The application phases, deliberately smaller than the client's UI flow to begin with.
///
/// The UI flow has eight modes, which live in the front end; this is the outer shell they live
/// inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Startup is still running.
    Startup,
    /// Initialised, no frame drawn yet.
    Idle,
    /// Inside the main frame loop.
    Running,
    /// Shutdown has been requested; the next frame exits.
    ShuttingDown,
}

/// Anything that stops the client before `Run`.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// Opening or validating the required data files failed; displayed with corestrings 201.
    #[error(transparent)]
    DataFiles(#[from] crate::assets::DataFilesError),
    /// Creating the presentation or window failed; displayed with corestrings 127.
    #[error("{}", crate::corestrings::display_string(crate::corestrings::ID_FATAL_WINDOWS_API, &[]))]
    Device {
        /// What actually failed, for a log line rather than for the dialog.
        cause: String,
    },
    /// Network initialization failed.
    ///
    /// Network startup failure is fatal: startup returns the error and the process displays it
    /// before exiting.
    #[error(transparent)]
    Net(#[from] crate::net::ClientNetworkError),
    /// `--connect` with no `-a` or no `-h`.
    #[error("{0}")]
    CommandLine(String),
}

impl StartupError {
    /// The [`StartupError::Device`] `cause`, which the dialog sentence deliberately does not name.
    ///
    /// Corestrings 127 says only "a fatal Windows API issue"; the string carried beside it is what
    /// actually failed. `main` logs it as an error before the sentence, which is the "log line
    /// rather than … the dialog" the variant's field is for.
    #[must_use]
    pub fn cause(&self) -> Option<&str> {
        match self {
            Self::Device { cause } => Some(cause.as_str()),
            _ => None,
        }
    }
}

/// `Timer`, the clock and its frame pacer, and the headless step, reachable from here because
/// that is where the suite and the rest of the crate look for them: `Clock` is the retail `Timer`
/// and is what [`App::clock`] hands back.
pub use crate::platform::clock::{
    Clock as ClockSource, FixedStepClock, Pacer, SystemClock, Timer as Clock,
    EXTERNAL_TIME_EPSILON, HEADLESS_STEP,
};
pub use crate::platform::window::STANDARD_DISPLAY_MODES;

/// What the screens read out of globals, rebuilt each frame from the session: the pre-game
/// view the character screens and the data-patch screen are handed.
pub use dereth_client_contract::pregame::PregameView as HostState;

// ---------------------------------------------------------------------------------------------
// The diagnostic log throttles.
// ---------------------------------------------------------------------------------------------
//
// Four of `App`'s log lines print once per *change* rather than once per frame, which is right:
// these are the instruments this project debugs a running client with, and a line per frame
// destroys a log as thoroughly as silence does. Each of the four was gated on a **projection** of
// the line it prints — one term out of seven, or a bool over a value — so the numbers beside it
// could move and the line never go out.
//
// **A diagnostic that can go quiet is worse than none, because its silence reads as absence of
// the condition.** `ObjectPhysics::report_key` follows the same rule. A key can also be a
// superset of its line until the thing it summarises begins changing underneath it, which is
// what `last_viewer_block` guards against.
//
// The repair is the same each time and it is structural rather than an audit: a small `Copy`
// struct that holds **exactly the values the line prints**, the line formatted out of that struct
// and nothing else, and the struct itself as the key. The line and the key cannot then drift
// apart, because there is only one of them — which is the property an enumerated key does not
// have. Where a printed value climbs at message rate, it enters the key by
// [`magnitude`] and the *raw* value is still what gets printed.

/// `64 - leading_zeros`: 0 -> 0, 1 -> 1, 2..3 -> 2, 4..7 -> 3, … (the same idiom as
/// `ObjectPhysics::report_key`).
///
/// A counter that climbs on a retry or at message rate cannot be in a log key raw — the line would
/// print every frame. Announced at its first occurrence and again at 2, 4, 8, 16 …, it is bounded
/// at 64 lines apiece for the life of a session, never silent and never a flood.
const fn magnitude(n: u64) -> u64 {
    (u64::BITS - n.leading_zeros()) as u64
}

/// The net-error line's gate.
///
/// # What this key is a superset OF
///
/// The line prints one thing: `NetErrorCode::id_string()`. The key is the code itself, so it is
/// exactly a superset — a second **distinct** error prints, a repeat of the same one does not, and
/// a code cannot move underneath a held key.
///
/// A `bool` latch would be the smallest projection there is: the first error printed and every
/// distinct error after it silent, so a session that timed out
/// (`ID_ConnectionError_ClientTimedOutServer`) and then failed to log back in
/// (`ID_ConnectionError_PlayerAlreadyLoggedOn`) would report only the timeout — and the second one
/// is the interesting one, because it is what the *reconnection* did.
/// The network error code is a *value*, so the gate has to be one too.
///
/// It is a free function rather than an inline compare so that the decision has a station: the one
/// production call site holds a live [`crate::net::NetLink`], which a test cannot build, and a gate
/// no test can drive can become inert unnoticed. This is the whole decision, and the test drives it.
fn should_report_net_error(
    last: Option<dereth_transport::conn::NetErrorCode>,
    code: dereth_transport::conn::NetErrorCode,
) -> bool {
    last != Some(code)
}

/// Everything the landblock-crossing line prints, and the key it is gated on.
///
/// # What this key is a superset OF
///
/// The line reads the viewer's block and **six** of the drawn world's scene statistics' counters:
/// `blocks_meshed`, `terrain_surfaces`, `scenery_objects`, `buildings`, `static_objects` and
/// `object_triangles`. All seven values are in here and the line is formatted from this struct
/// alone, so it is a superset **by construction** and cannot become a projection by an edit to the
/// line: a value that is not in this struct cannot be printed.
///
/// Keying on the **block alone** would be cell-manager-shaped reasoning that does not hold here:
/// only in a build where the landblocks around you are already there can the six counters change
/// only when the block changes. `WorldScene::stream` meshes them asynchronously, so a player who
/// logs in and stands still watches `blocks_meshed`, `terrain_surfaces` and `object_triangles`
/// climb for several seconds with the block held still — and a block-only key would never print
/// the line that says so.
///
/// # What is deliberately excluded, and why it is not the same defect again
///
/// `SceneStats` carries **82** fields and this key holds seven of them. That is not an
/// oversight and a whole-struct key here would be wrong, not merely wider: `held_without_holding_
/// location` is *"counted once per frame per object"*, `remote_sticks_pulled` counts frames, and
/// `character_batches` is a per-frame draw cost. Keyed on those, this line would print on every
/// frame of any session in which one of them is non-zero. They are also not in the line, so their
/// movement is not a silence — they have their own readers on `SceneStats`. The rule this key
/// follows is *superset of what the line prints*, and the line is printed from this struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewerBlockReport {
    /// `WorldScene::viewer_block()`.
    block: (i32, i32),
    blocks_meshed: usize,
    terrain_surfaces: usize,
    scenery_objects: usize,
    buildings: usize,
    static_objects: usize,
    object_triangles: usize,
}

impl ViewerBlockReport {
    fn new(block: (i32, i32), s: &crate::present::SceneCensus) -> Self {
        Self {
            block,
            blocks_meshed: s.blocks_meshed,
            terrain_surfaces: s.terrain_surfaces,
            scenery_objects: s.scenery_objects,
            buildings: s.buildings,
            static_objects: s.static_objects,
            object_triangles: s.object_triangles,
        }
    }
}

/// Everything the viewer-cell line prints, and the key it is gated on.
///
/// # What this key is a superset OF
///
/// The line reads the cell id, `is_outdoors(cell)` and the **second** half of
/// `WorldScene::env_cell_counts()` — how many interior batches the viewer's own cell carries. All
/// three are here and the line is formatted from this struct alone.
///
/// The **cell alone** would not do, because `inside` is not a function of the cell in this build
/// for two independent reasons. It streams: `env_cell_counts` walks `blocks`, so an interior cell
/// whose block is still meshing answers 0 and answers its real count a second later, with the cell
/// held still. And it is asked of a *different* cell from the one in the key — `env_cell_counts`
/// uses `viewer_cell()`, the **camera's** cell, while the key's `cell` is `viewer_cell_id()`, the
/// **body's**, so a camera pushed through a doorway behind a standing body moves one and not the
/// other.
///
/// `resident` — `env_cell_counts()`'s first half — is deliberately **not** here, because the line
/// does not print it. `main.rs`'s own summary prints it and `dereth/client/tests/gpu/world/interiors.rs` reads it, so its
/// movement is not a silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewerCellReport {
    cell: dereth_primitives::CellId,
    /// `dereth_physics::landdefs::is_outdoors(cell)` — the word the line prints.
    outdoors: bool,
    /// `env_cell_counts().1`.
    inside: usize,
}

impl ViewerCellReport {
    fn new(cell: dereth_primitives::CellId, inside: usize) -> Self {
        Self {
            cell,
            outdoors: dereth_physics::landdefs::is_outdoors(cell),
            inside,
        }
    }
}

/// Everything the object-census line prints, and the key it is gated on.
///
/// # What this key is a superset OF
///
/// The line reads the drawn-object count, **four** of the drawn world's scene statistics' counters and
/// **eight** of `ObjectStream`'s. All thirteen are here and the line is formatted from this struct
/// alone.
///
/// The eighth, `container_exits_offered`, belongs on this line: when a dropped item does not
/// appear in the world, the number that separates *"the drop never reached the
/// client"* from *"the drop reached the client and was not drawn"* is how many objects have left a
/// container. It is an **event** counter, not a rate one -- a drop is a deliberate act -- so it
/// enters the key raw, like `creates` and `parent_events` and unlike the two below.
///
/// `server_object_count()` **alone** would be a projection of eleven other numbers:
/// `creates` and `removes` moving together in one sync hold the count still, `merges` and
/// `recreates` do not change it at all, and `parent_events` changes what is *drawn on whom* rather
/// than how many there are. So the count could move underneath the line and the line stay silent —
/// and this is the line somebody debugging *"objects are arriving and nothing appears"* reads.
///
/// # The two counters that enter by magnitude, and why that is not a projection
///
/// `position_updates` and `movement_updates` climb at **message rate**: a `0xF748` for anything
/// that walks lands several times a second, so keyed raw this line would print on most frames of
/// any session with a mobile creature in it. They therefore enter the key through [`magnitude`],
/// so the condition is announced at 1, 2, 4, 8 … and is bounded at 64 lines apiece for a session.
/// **Every print carries the raw value**, so the number a reader sees is never the rounded one.
/// `ObjectPhysics::report_key` treats `moved` and `unplaced` the same way, and it is the reason a
/// whole-struct key is not the right answer here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ObjectReport {
    drawn: usize,
    animated: usize,
    held: usize,
    setups: usize,
    triangles: usize,
    creates: u64,
    merges: u64,
    recreates: u64,
    removes: u64,
    position_updates: u64,
    movement_updates: u64,
    parent_events: u64,
    container_exits_offered: u64,
}

impl ObjectReport {
    fn new(drawn: usize, w: &crate::present::SceneCensus, s: &crate::objects::ObjectStats) -> Self {
        Self {
            drawn,
            animated: w.server_objects_animated,
            held: w.server_objects_held,
            setups: w.server_object_setups,
            triangles: w.server_object_triangles,
            creates: s.creates,
            merges: s.merges,
            recreates: s.recreates,
            removes: s.removes,
            position_updates: s.position_updates,
            movement_updates: s.movement_updates,
            parent_events: s.parent_events,
            container_exits_offered: s.container_exits_offered,
        }
    }

    /// The gate. Identical to `self` except for the two message-rate counters; see this type's note.
    fn key(self) -> Self {
        Self {
            position_updates: magnitude(self.position_updates),
            movement_updates: magnitude(self.movement_updates),
            ..self
        }
    }
}

/// The three platform seams an `App` is built on: the window, the time source and the frame
/// pacer.
///
/// [`Platform::headless`] is *the* headless configuration; the windowed one is opened by the
/// executable, which owns the window system. `App::with_platform` takes one, which is how a test builds a real
/// `App` with no window system and no graphics device at all.
pub struct Platform {
    /// The application window and the event-loop state owned by it.
    pub window: Box<dyn WindowHost>,
    /// What [`Clock::update_time`] samples once a frame.
    pub clock: Box<dyn ClockSource>,
    /// The frame pacer used after each completed frame.
    pub pacer: Box<dyn Pacer>,
    /// The modal error box a failed first connection is shown in: the OS message box on the
    /// windowed path, nothing at all under `--headless`.
    pub dialog: Box<dyn crate::platform::dialog::ErrorDialogHost>,
}

impl std::fmt::Debug for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("has_window", &self.window.has_window())
            .field("fixed_step", &self.clock.fixed_step())
            .finish_non_exhaustive()
    }
}

impl Platform {
    /// No window, and a simulated clock.
    ///
    /// `--headless` is a regression harness, not a session: it steps the simulated clock so the
    /// capture is reproducible. See [`FixedStepClock`].
    ///
    /// The **pacer** is still the system one, because the frame pacer's active arm is `Sleep(0)` —
    /// a bare yield — and a headless run is always the foreground application, so there is
    /// nothing to simulate.
    #[must_use]
    pub fn headless(width: u32, height: u32) -> Self {
        Self {
            window: Box::new(NullWindow::new(width, height)),
            clock: Box::new(FixedStepClock::new(HEADLESS_STEP)),
            pacer: Box::new(SystemClock::new()),
            dialog: Box::new(crate::platform::dialog::NoDialog),
        }
    }
}

/// The application: everything the frame needs that is not drawing, a device, a UI or an OS
/// window, and the frame loop over it.
///
/// `S` is the front end that plugs in (`Shell`); its presentation is `S::Present` and its HUD
/// `S::Hud`. The fields are public so that a front end's `Shell` calls can act on the state
/// they are handed, exactly as the frame's own steps do.
pub struct App<S: Shell> {
    pub cfg: Config,
    pub state: AppState,
    pub pump: Pump,
    /// The timer's state — the retail clock's arithmetic, which is all it is, because
    /// the time source is [`Self::clock`].
    pub timer: Clock,
    /// Where the time comes from: [`crate::platform::clock::SystemClock`] on the windowed path,
    /// [`crate::platform::clock::FixedStepClock`] under `--headless`.
    pub clock: Box<dyn ClockSource>,
    /// The sole frame limiter.
    pub pacer: Box<dyn Pacer>,
    /// Everything the frame did, in order, and the frame totals. Every counter accessor below is a
    /// one-line read over it and [`Self::last_frame_steps`] is [`FrameRecorder`]'s view of its step
    /// order.
    pub events: FrameEvents,
    /// The presentation. `App` owns a seam, not a device, which is what lets this
    /// struct -- and every test that builds one -- exist without one. See [`crate::present`].
    pub present: Box<S::Present>,
    /// The world's simulation and residency -- the body, every server object's
    /// simulation record, the landblock window, the camera, the clock -- which the application
    /// owns. The presentation builds it when it loads the world (and a presentation with no device
    /// leaves it `None`, as a headless run always has had no scene), draws from it, and is handed
    /// it by every world method.
    pub world: Option<crate::world_state::WorldState>,
    /// The dat store.
    ///
    /// The seam every asset consumer takes is [`AssetSource`], and [`App::assets`] hands it over as
    /// one. It is held **concretely** because the world path needs `read_typed`: a cell-dat id
    /// carries no type of its own: the top 16 bits are the landblock, the low 16 are the cell index,
    /// and the container decides the type. `DivineType` can and does collide with the portal ranges, so `AssetSource::read`
    /// cannot address a landblock unambiguously.
    pub store: std::sync::Arc<dereth_dat::RetailDatStore>,
    /// The taboo table selected by type `0x11`, enum `2`, and id `0x14`.
    /// Retained by the app because session reset replaces the game world and its scroll.
    pub taboo_table: Option<std::sync::Arc<dereth_assets::TabooTable>>,
    /// The window, its event loop and its display: a
    /// [`crate::platform::window::WindowHost`], so a headless `App` holds a
    /// [`crate::platform::window::NullWindow`] rather than a `None` every caller had to test.
    pub window: Box<dyn WindowHost>,
    /// The free camera controls. Written by [`App::apply_input_actions`] from the camera
    /// action set, plus the two residual flycam keys of [`App::note_flycam_input`].
    pub input: CameraInput,
    /// The character controls. **Written only by [`App::apply_input_actions`]**, which is
    /// downstream of `walk_input_maps` and therefore of the typing barrier.
    ///
    /// **It is a projection of [`Self::movement`]'s three command lists**, recomputed
    /// after every command rather than latched per key.
    pub char_input: crate::character::CharacterInput,
    /// The command interpreter's three command
    /// lists, the run lock and the two hold keys. The only writer of [`Self::char_input`]'s six
    /// direction slots.
    pub movement: crate::character::MovementCommands,
    /// Right button held: the cursor drives the look direction.
    pub mouse_look: bool,
    /// The orbit camera an interface asked for in place of the game's own, with its movement
    /// scheme and pointer directions; `None` is the game's camera.
    pub orbit: Option<crate::orbit::OrbitSettings>,
    /// The movement keys and mouse buttons as the orbit camera's movement reads them.
    pub orbit_keys: crate::orbit::MovementKeys,
    /// What the orbit camera's mouse buttons asked of the game since the last dispatch.
    pub orbit_pending: Vec<dereth_client_contract::actions::Action>,
    /// Whether the interface shown is Horizon, the only one the optional high-fidelity
    /// presentation draws under: every world this client builds takes it, and the drawn one
    /// follows it as it changes ([`crate::ui_context::UiContext::set_hifi_interface`]).
    #[cfg(feature = "hifi")]
    pub hifi_interface: bool,
    /// What the player has been told this session of a box of the presentation that this device
    /// cannot draw: each line is said once.
    #[cfg(feature = "hifi")]
    fidelity_told: Vec<&'static str>,
    /// A look stick's push, each axis from -1 to 1, which turns the orbit camera every frame.
    pub orbit_look: (f32, f32),
    /// Every animated body drawn between its animation's keyframes, as an interface asked
    /// ([`crate::ui_context::UiContext::set_smooth_animation`]); the drawn world takes it each
    /// frame (`crate::world_state::WorldState::smooth_animation`).
    pub smooth_animation: bool,
    /// Every object physics or the server moves drawn moving between its physics ticks, as an
    /// interface asked ([`crate::ui_context::UiContext::set_smooth_movement`]); the drawn world
    /// takes it each frame (`crate::world_state::WorldState::smooth_movement`).
    pub smooth_movement: bool,
    /// A press of an attack key or an attack height button is the whole attack, which repeats
    /// until it is interrupted, and the advanced combat interface is never used, as an interface
    /// asked
    /// ([`crate::ui_context::UiContext::set_press_attacks`]); the combat system takes it each
    /// frame (`crate::interaction::Interaction::note_press_attacks`).
    pub press_attacks: bool,
    /// When the cursor last moved under mouse look, for the input poll's 0.2 s idle tick.
    last_mouse_move: f64,
    pub last_cursor: Option<(f64, f64)>,
    /// Time at the previous frame, used to compute the camera timestep.
    pub last_time: f64,
    /// The three inputs target tracking reads,
    /// as they were the last time it ran: `(tracking_target, combat_mode, selected_id)`.
    ///
    /// Retail re-runs target tracking from exactly three places, and each is a change in one
    /// of these three: the `ViewCombatTarget` option (acting only when the new value differs
    /// from the current tracking flag), the selection-changed notice and the combat-mode
    /// change. This build has no
    /// notice bus that reaches a UI element — the same deferred delivery
    /// `dereth_client_model::combat`'s `set_combat_mode` header records for the combat-mode notice — so
    /// the three edges are detected here instead of being delivered. An edge detector over exactly
    /// these three words reproduces exactly those three call sites and no fourth: it is deliberately
    /// **not** a per-frame re-run, because that would also re-track when a target stopped being
    /// attackable without the selection changing, which retail does not do until the next edge.
    pub last_target_tracking: Option<(
        bool,
        dereth_client_model::combat::CombatMode,
        Option<dereth_primitives::ObjectId>,
    )>,
    /// `(PersistentAtDay, DisableMostWeatherEffects, DisableDistanceFog)` as
    /// [`Self::apply_player_option_effects`] last applied them.
    ///
    /// The same shape, and for the same reason, as [`Self::last_target_tracking`]: this build has
    /// no notice bus that reaches the landscape, so changes to the three landscape override values
    /// and their initialization copies are detected as one edge
    /// over the three bits instead of being delivered. `None` until the first scene exists.
    pub last_option_environment: Option<(bool, bool, bool)>,
    /// The landscape environment-override globals written by visual `Admin_Environs`. The scene owns
    /// a clone of this handle so landscape replacement preserves an in-flight transition.
    pub environment_override: crate::environment::EnvironmentOverrideState,
    /// The network link, present only with `--connect`.
    pub link: Option<NetLink>,
    /// The server's stand-in, which answers what a server always answers. Present on a headless
    /// client built with no connection, and taken away when a relay links it to a real server.
    /// A run that is about waiting for an answer turns it, or one of its answers, off. See
    /// [`crate::server_stub`].
    pub server_stub: Option<crate::server_stub::ServerStub>,
    /// `--enter-world`'s script: which step it is on.
    pub script: EnterWorldScript,
    /// The last reported, so a transition is logged once.
    pub last_link_status: Option<crate::net::LinkStatus>,
    /// The bad-packets-received counter, reported when it moves.
    pub last_rejected: u32,
    /// Which has already been logged.
    ///
    /// With a UI up the script does not end on a net error (the disconnected screen does), so the
    /// latch is its own field rather than `script == Done`.
    ///
    /// **The line prints a *code*, so the latch holds one.** A `bool` latch over a printed value is
    /// the smallest possible projection: the first error would print and every subsequent
    /// **distinct** error would be silent, so a session that timed out and then failed
    /// authentication would report only the timeout. Keyed on the code, a second distinct error
    /// prints and a repeat of the same one does not.
    pub last_net_error: Option<dereth_transport::conn::NetErrorCode>,
    /// Where a failed first connection's error box is shown. See [`Platform::dialog`].
    pub dialog: Box<dyn crate::platform::dialog::ErrorDialogHost>,
    /// The first connection's failure, once the login has ended before the link was up: the box
    /// that was shown, after which the loop ends and the process exits.
    pub connect_failure: Option<crate::connect_failure::ConnectFailure>,
    /// The object tables and the render facts that go with them.
    pub objects: crate::objects::ObjectStream,
    /// A scene waiting for the player's position.
    ///
    /// With `--connect` the landscape is not built at startup, because the block to build it
    /// around is the one the *server* puts the player in and nothing knows that until `0xF745` for
    /// the player arrives. The retail client is the same shape: the landscape has no landblocks until
    /// the object stream supplies it with the player's position.
    pub pending_scene: Option<crate::scene::SceneConfig>,
    /// The switches [`App::pending_scene`] was *armed* from, kept so it can be armed again.
    ///
    /// `pending_scene` is consumed by [`App::load_pending_scene`], and `App::defer_static_scene`
    /// has exactly one caller, in `main`, before the loop. Without this field nothing would refill
    /// it, so the landscape, the resident blocks, the interior cells, the viewer cell and the body
    /// would be built **once per process**, and a second login would inherit the first one's —
    /// including the `Character::teleport` to the server's position, which lives inside
    /// `load_pending_scene`. A second login would then have incorrect positions and misplaced item
    /// drops: the rendered body would stay at session 1's frame, the movement command reporter
    /// would report that frame, and the shard would place the drop against it.
    ///
    /// `None` is a build with no landscape at all (`--no-world`, the char-gen and intro slices);
    /// there is nothing to rebuild and the teardown is a no-op.
    pub scene_config: Option<crate::scene::SceneConfig>,
    /// The world drawn behind the screens before the player is in it, while one is: the block it
    /// was built around and the camera it was built with. See [`App::show_backdrop`].
    backdrop: Option<(u16, crate::camera::FreeCamera)>,
    /// What the landblock line last printed, so a crossing is logged once rather than
    /// every frame. Not the block alone; see [`ViewerBlockReport`].
    last_viewer_block: Option<ViewerBlockReport>,
    /// What the viewer-cell line last printed. Not the cell alone; see [`ViewerCellReport`].
    last_viewer_cell: Option<ViewerCellReport>,
    /// What the object-census line last printed, keyed. Not the drawn count alone, which eleven
    /// other printed numbers can move underneath; see [`ObjectReport`].
    last_object_report: Option<ObjectReport>,
    /// The landblock, object and unowned-request lines each go out at most once per
    /// [`crate::report_gate::REPORT_INTERVAL`], with the latest state.
    pub viewer_block_gate: crate::report_gate::ReportGate,
    pub object_report_gate: crate::report_gate::ReportGate,
    pub unowned_gate: crate::report_gate::ReportGate,
    /// Unowned requests not printed because the gate was shut, reported with the next one.
    pub unowned_suppressed: u64,
    /// Preferences `--set-at` set this frame, handed to their owners with the options page's.
    scripted_preferences: Vec<(&'static str, dereth_client_contract::PrefValue)>,
    /// Actions `--action-at` presses this frame, for the front end to press as its keys would.
    pub(crate) scripted_actions: Vec<dereth_client_contract::actions::ActionId>,
    /// How many frames [`Self::frame`] has begun, which is what `--set-at` and `--capture-at`
    /// count.
    frames_begun: u64,
    /// The last frames' times, for the performance panel.
    pub perf: crate::perf::FramePerf,
    /// How long each step of the last whole frame took.
    perf_steps: [f32; crate::frame::STEPS],
    /// Whether the performance panel's font is on the device.
    perf_font: bool,
    /// Time when `0x0013` made the session playable, used by `--linger`.
    pub playable_at: Option<f64>,
    /// Movement-command position reporting: `0xF753` on the client's own 1.0 s schedule and
    /// `0xF61C` on a movement-state change. Without it **nothing tells the server where the player
    /// is**, and ACE's copy of `Player.Location` stays at the login position for the whole
    /// session. See [`dereth_client_net::client_session::position`] and
    /// [`App::position_use_time`].
    pub position: dereth_client_net::client_session::PositionReporter,
    /// Last successful release's actual request, before transport; never used to drive motion.
    pub last_jump_request: Option<dereth_protocol::movement::MovementJump>,
    /// The world-object pick, the world-view wrapper's search-reason machine and the
    /// one place a `dereth_client_model::Request` becomes bytes. See [`crate::interaction`].
    pub interaction: crate::interaction::Interaction,
    /// The questions the game has asked the player and not had answered. See [`crate::dialogs`].
    pub dialogs: crate::dialogs::DialogService,
    /// The screen-size choice, independent of the active interface.
    pub resolution: crate::resolution::ResolutionTransaction,
    /// This frame's actions, from the front end's device input or injected by a script, on
    /// their way through the frame's handler stages. See [`crate::actions`].
    pub actions: crate::actions::ActionQueue,
    /// The sound manager's globals and the output device.
    pub audio: Option<crate::audio::Audio>,
    /// What the screens read out of globals, rebuilt each frame from the session.
    pub host_state: HostState,
    /// DDD events seen since the last `build_host_state`, waiting for the screen.
    pub pending_ddd: Vec<dereth_client_contract::pregame::DddEvent>,
    /// The shared asset cache's DDD patch state, and the writers it patches through.
    ///
    /// Aimed at [`crate::config::Config::dat_dir`], which in a normal run is the retail
    /// client directory — and `DatWriter::open` refuses to write there. So the
    /// production posture is *"a server cannot patch the installed dats"*, and every refusal is in
    /// [`crate::ddd::DddPatcher::notices`] rather than swallowed. See [`crate::ddd`].
    pub ddd: crate::ddd::DddPatcher,
    /// Set by the `0xF7EA` arm; drained by [`Self::invalidate_after_ddd`] once the `link` borrow
    /// the event loop holds has ended.
    pub ddd_invalidation: Option<crate::ddd::DddSummary>,
    /// How many times the data files have been reopened since start-up ([`Self::adopt_store`]):
    /// a front end that keeps anything read from them compares it with the count it last saw and
    /// reads again when it has moved.
    store_generation: u64,
    /// The data files as the platform opened them, when it opened them itself
    /// ([`Self::bring_up_with_store`]): the base a reopen lays the world's overlay over again, in
    /// place of opening [`crate::config::Config::dat_dir`].
    base_store: Option<std::sync::Arc<dereth_dat::RetailDatStore>>,
    /// `0x0013` has arrived and the automatic screen layout has not been applied
    /// yet. The front end's UI step ([`Shell::ui_frame`]) is where the notice's own arm runs.
    pub pending_auto_layout: bool,
    /// The presentation the window is *currently* wearing, against which
    /// full-screen preference (`self.pump.state.full_screen`) is compared once a frame. A
    /// difference triggers presentation replacement.
    pub applied_full_screen: bool,
    /// The resolution shadow used by presentation-change polling.
    ///
    /// The client polls resolution, full-screen mode, refresh rate, vertical sync, and
    /// antialiasing, raising one change flag if any differs from its applied shadow.
    /// Triple buffering is the one display-preference field that this poll does not watch. This
    /// build represents presentation size as the client rectangle, so this field stores the
    /// resolution currently applied to the swap chain.
    pub applied_resolution: (u32, u32),
    /// The `Display.SyncToRefresh` value the presentation currently wears.
    ///
    /// Presentation polling compares this with the saved vertical-sync preference alongside the
    /// resolution and full-screen shadows. Logical full-screen mode still gates the effective
    /// swap-chain interval.
    pub applied_sync_to_refresh: bool,
    /// Whether presentation loading substitutes [`Self::forced_resolution`].
    ///
    /// The flag is read after decoding the saved resolution. When set, it replaces the decoded
    /// width and height, so every presentation consumer observes the forced dimensions.
    ///
    /// It starts **true**, because client initialization first installs
    /// the forced `800x600` display resolution, before presentation setup has
    /// loaded a single preference. See [`App::force_display_resolution`].
    pub use_forced_resolution: bool,
    /// The forced width and height.
    pub forced_resolution: (u32, u32),
    /// The presentation to return to when the force is lifted.
    ///
    /// In retail this is simply the display preferences' resolution, and the restore arm reads it
    /// back through the display-preference load. Here it is latched instead, and the divergence is
    /// declared: `Config::width`/`height` is also this build's **headless render extent**, set
    /// directly by forty test files and by `--capture` runs, so restoring the *registered default*
    /// (`0x04000300` = 1024x768) would drag every such run to a size nobody asked for.
    /// [`App::bring_up`] latches the size the client was configured to run at and
    /// [`App::set_display_resolution`] moves it when the drop-down writes a legal one, which is
    /// the same "only when it was authored" rule the start-up read follows.
    pub unforced_resolution: (u32, u32),
    /// `GetSystemMetrics(SM_CXDLGFRAME, SM_CYCAPTION, SM_CYDLGFRAME)`, measured off the decorated
    /// window at start-up; see [`App::bring_up`]. `(0, 0, 0)` headless or when the window came up
    /// borderless, in which case the windowed placement is recomputed from the frame the host gives
    /// it back.
    pub frame_metrics: (i32, i32, i32),
    /// The **outer** rectangle the window last occupied, in the process's own coordinate space —
    /// Read from the host window whenever presentation changes.
    ///
    /// This is the input to the declared divergence in
    /// [`dereth_client_contract::window_proc::change_presentation_placement`]: a size change keeps this
    /// rectangle's top-left instead of re-centring. With a window it is read back off the window
    /// on every presentation change and is therefore the truth; headless it is *maintained* —
    /// seeded by [`App::bring_up`]'s own creation placement over
    /// `headless_screen_metrics` and moved by each [`App::change_presentation`] — so the rule is
    /// observable without a monitor. `None` is `GetWindowRect` failing, which falls
    /// back to the screen centre.
    pub window_rect: Option<dereth_client_contract::window_proc::Rect>,
    /// The windowed rectangle the window had when it last went full screen, remembered so that
    /// leaving full screen puts the window back where it was rather than centring it (client
    /// divergence CD-001). Taken, and so cleared, when full screen is left. `None` when the window
    /// is windowed, or when it went full screen with no rectangle to remember.
    pub windowed_rect_before_full_screen: Option<dereth_client_contract::window_proc::Rect>,
    /// The UI button-press sound (`0x72`) proof, played once. See `App::start_shell`.
    pub startup_sound_played: bool,
    /// UI sound-table id, resolved and cached once per session.
    pub ui_sound_table: Option<dereth_primitives::DataId>,

    /// The player system's player module, the player's description and the chat stream, applied to
    /// the gameplay screen's windows once per frame. See [`crate::hud`].
    pub hud: S::Hud,

    /// The world-object system's three teleport flags and the world-controller UI's animation. Driven at the
    /// `WorldViewStep` step and read by the teleport overlay. See [`crate::teleport`].
    pub teleport: crate::teleport::Teleport,

    /// The `AnimAssets` a preview space builds its objects through. One per process, because it
    /// memoises every setup record and animation record it decodes.
    pub anim_assets: std::sync::Arc<dereth_world_data::anim_assets::DatAnimAssets>,

    /// The object identity verdicts, worked out a few milliseconds a frame from start-up when
    /// another era's files are beside the world and the scene waits for them
    /// ([`crate::scene::SceneConfig::object_identity_budget`]). `None` otherwise.
    pub object_identity: Option<ObjectIdentityPrep>,

    /// Which of the game's own per-frame duties have run this frame. See [`FrameDuties`].
    duties: FrameDuties,
}

/// How long a frame of a connected client gives the object identity verdicts while nothing waits
/// on them ([`crate::scene::SceneConfig::object_identity_budget`]).
pub const IDENTITY_BACKGROUND_BUDGET: std::time::Duration = std::time::Duration::from_millis(3);

/// How long a frame gives them, at least, while the objects wait to take a look the player asked
/// for, and while no world is drawn.
pub const IDENTITY_WAITING_BUDGET: std::time::Duration = std::time::Duration::from_millis(8);

/// [`App::object_identity`]: the verdicts being worked out for the store, and what the frames
/// have spent on them.
#[derive(Debug)]
pub struct ObjectIdentityPrep {
    /// The work.
    pub build: crate::object_identity::IdentityBuild,
    /// How long a frame gives it while nothing waits on it.
    budget: std::time::Duration,
    /// The files it is for: a store reopened after a patch starts it again.
    store: std::sync::Arc<dereth_dat::RetailDatStore>,
    /// When it started.
    started: web_time::Instant,
    /// The longest a frame spent on it.
    pub longest_step: std::time::Duration,
    /// Whether the presentation has been handed the verdicts.
    pub offered: bool,
    /// Whether the drawn world waited on it at the last preference poll.
    awaited: bool,
    /// Whether the player has been told the look is being prepared.
    noticed: bool,
}

impl ObjectIdentityPrep {
    /// Start working out the verdicts for `store`, `budget` a frame, kept in the host's cache
    /// folder when `cache`. `None` when no other era's files are beside the world.
    #[must_use]
    pub fn start(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        budget: std::time::Duration,
        cache: bool,
    ) -> Option<Self> {
        let dir = if cache {
            crate::object_identity::default_cache_dir()
        } else {
            None
        };
        let build = crate::object_identity::IdentityBuild::for_store(store, dir)?;
        Some(Self {
            build,
            budget,
            store: std::sync::Arc::clone(store),
            started: web_time::Instant::now(),
            longest_step: std::time::Duration::ZERO,
            offered: false,
            awaited: false,
            noticed: false,
        })
    }
}

/// The game's own per-frame duties inside the UI step, and which of them have run.
///
/// The teleport tick (and the portal space it drives), the HUD model's sync and following the game
/// screen's size are the game's,
/// not the UI's: skip the teleport tick and the server is never told the character arrived, and
/// it keeps the player in portal space. Each runs once a frame. A front end whose timing matters
/// runs one at its own point in its UI step ([`App::teleport_use_time`], [`App::sync_hud`],
/// [`App::follow_screen_forced_resolution`]); whatever it did not run, the runtime runs at the
/// foot of the UI step, so a front end that never heard of them still has a working game.
#[derive(Debug, Default)]
struct FrameDuties {
    /// [`App::teleport_use_time`] ran this frame.
    teleport_ticked: bool,
    /// [`App::sync_hud`] ran this frame.
    hud_synced: bool,
    /// The game-screen answer the screen size last followed; `None` before the first.
    gameplay_followed: Option<bool>,
    /// Whether the interface the screen size last followed keeps the login size before the
    /// world; `None` before the first.
    login_size_followed: Option<bool>,
    /// Whether full screen was allowed where the full-screen state last followed the screen
    /// ([`App::follow_gameplay_full_screen`]); `None` before the first.
    full_screen_followed: Option<bool>,
    /// [`App::start_preferences`] has run.
    preferences_started: bool,
    /// [`App::portal_space_use_time`] ran this frame.
    portal_driven: bool,
    /// The time of the portal space's last step, which it advances by.
    portal_last_time: f64,
    /// The UI has the character-creation wizard up (`UiRequest::CharacterCreation`).
    creating_character: bool,
    /// The character has asked to leave the world and is still in it.
    leaving_world: bool,
    /// A quit has logged the character off and stops the loop once the log-off has gone out.
    quit_owed: bool,
}

/// An object notice offered to the front end's panels, and the requests they raised on the way
/// answered in order: the toolbar's queries here, at once, and the rest back to the front end.
fn object_panel_notice<S: Shell>(
    shell: &mut S,
    hud: &mut S::Hud,
    inter: &mut crate::interaction::Interaction,
    world: &mut dereth_client_model::World,
    notice: &dereth_client_model::Notice,
) {
    for request in shell.object_panel_notice(hud, world, notice) {
        if !inter.dispatch_toolbar_query(world, &request) {
            if let Some(outbox) = shell.ui_requests() {
                outbox.emit(request);
            }
        }
    }
}

/// The scripted enter-world run `--enter-world` drives, in place of the player's clicks.
///
/// Injected keystrokes do not prove application behavior, so the sequence runs through the
/// application's own update path and reports what it reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnterWorldScript {
    /// Waiting for `0xF658 Login_LoginCharacterSet`.
    #[default]
    AwaitingCharacterSet,
    /// `0xF7C8` has gone out; waiting for `0xF7DF` and then `0x0013`.
    Entering,
    /// `0x0013` arrived: the client is in world. A log-off request has been
    /// sent, because killing the process leaves the account logged in on the server until its own
    /// timeout, which makes the next run fail to enter the world.
    LoggingOff,
    /// `0xF653 Login_ExecuteLogOff` came back. The loop may end.
    Done,
}

impl<S: Shell> std::fmt::Debug for App<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("state", &self.state)
            .field("frames_drawn", &self.frames_drawn())
            .field("headless", &self.cfg.headless)
            .field("device_state", &self.pump.state)
            .finish_non_exhaustive()
    }
}

/// How long the mouse stays still under mouse look before the input poll's idle tick, in
/// seconds.
const MOUSE_LOOK_IDLE: f64 = 0.2;

/// The window class name, exported so a driving harness can find the window the way it finds the
/// retail one, by class. The window system names the class itself, so this is the value to match
/// rather than the value in use.
pub const RETAIL_WINDOW_CLASS: &str = style::CLASS_NAME;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "resolution_app_tests.rs"]
mod resolution_app_tests;
