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
//! * a front end, `Shell` -- the retail UI, the cursor, the clipboard and whatever else draws
//!   over the world. `NullShell` is the front end with no UI at all, which is what a headless
//!   run or a client started without its UI has.
//!
//! The frame's order is decided here and nowhere else: every step the front end takes part in is
//! a `Shell` call made at the point in the frame where the retail client makes it, so a front
//! end supplies behaviour and never sequence.

use dereth_client_contract::window_proc::style;

use dereth_primitives::{AssetSource, DataId};

pub use crate::shell::{NullShell, Shell, UiNotices};
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
/// [`record_shell_calls`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCall {
    /// The URL launch. `result` is the value returned by the host or the armed recorder.
    Open { url: String, result: i32 },
    /// The error dialog requested after an unsuccessful launch.
    ErrorBox { title: String, text: String },
}

thread_local! {
    /// `Some` once a test has called [`record_shell_calls`]: the two host calls are then recorded
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
pub fn record_shell_calls() {
    record_shell_calls_answering(33);
}

/// [`record_shell_calls`] with a chosen launch result, so a test can drive the failure leg
/// (`result <= 32`) and inspect the requested error dialog.
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
/// # Two declared deviations, both forced by `#![forbid(unsafe_code)]`
///
/// `ShellExecuteA` and `MessageBoxA` are `unsafe fn`s in the `windows` crate and this crate
/// forbids `unsafe_code`, exactly as `GetTimeZoneInformation` is out of reach in
/// [`crate::platform`]. The answer is the same as there: the **safe** WinRT binding in the
/// same already-locked crate.
///
/// 1. The launch is `Windows.System.Launcher.LaunchUriAsync`, which is the shell's
///    `"open"`-verb protocol dispatch for a URL and reaches the same registered handler
///    `ShellExecuteA(NULL, "open", "http://...")` reaches. It is **not awaited**: retail's
///    `ShellExecuteA` answers as soon as the handler has been started, so the result here is
///    `33` (the smallest value the signed comparison accepts as success) once the launch has been
///    handed off, and `0` when the uri will not parse or the launcher refuses the call --
///    the value `SE_ERR_NOASSOC` would land in the same `<= 32` leg.
/// 2. The failure leg's `MessageBoxA` has no safe equivalent (WinRT's `MessageDialog` wants a
///    `CoreWindow`, which a Win32 client does not have), so the [`ShellCall::ErrorBox`] is
///    handed back and `App` puts its text in the chat scroll instead, the way
///    `App::take_action_screenshot` puts its own host-side line there. **The text itself is
///    retail's, byte for byte** -- see [`shell_error_text`] -- so the player is still told the
///    error code and the address to type in by hand. A real modal box is the one remaining gap.
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

/// `0xF658 Login_LoginCharacterSet` → the persistent-data object's character set.
///
/// The session decodes the message and the pre-game view owns the destination; the conversion
/// between them is the application's. It is a field-for-field copy and nothing more.
#[must_use]
pub fn character_set_from_login(
    set: &dereth_protocol::login::LoginCharacterSet,
) -> dereth_client_contract::persist::persistent_data::CharacterSet {
    let one = |c: &dereth_protocol::login::CharacterIdentity| {
        dereth_client_contract::persist::persistent_data::CharacterIdentity {
            id: c.gid,
            name: c.name.clone(),
            seconds_grace_period: c.seconds_greyed_out,
        }
    };
    dereth_client_contract::persist::persistent_data::CharacterSet {
        set: set.characters.iter().map(one).collect(),
        del_set: set.deleted.iter().map(one).collect(),
        status: set.status,
        // The client's own default is 5 and the server overwrites it; a negative count is not a
        // state the message can legally carry, and clamping is what the client's unsigned
        // allowed-characters field does with one.
        num_allowed_characters: u32::try_from(set.num_allowed_characters).unwrap_or(0),
        account: set.account.clone(),
        // `0xF658` carries `has_throne_of_destiny` and nothing about Dark Majesty or a pre-order;
        // the other two flags are set from elsewhere in the client and are left at their defaults
        // rather than guessed at.
        is_dark_majesty: false,
        is_throne_of_destiny: set.has_throne_of_destiny != 0,
        pre_ordered_throne_of_destiny: false,
    }
}

/// Answer server interrogation with one iteration list per open DAT file.
///
/// ```text
/// iteration lists = []
/// ok = true
/// for each open data file:
///     if it is initialized:
///         append its type, then its file id
///         ok = load its mostly-consecutive integer set and ok
/// if (ok) adopt and deliver the interrogation response;                // 0xF7E6 on queue 5
/// ```
///
/// The **type dword comes first**, then the id: type 0 portal, 1 cell and language, `"HiFi"` as an
/// int for the high-res dat; id 1 portal and highres, 2 cell, 3 language. The id ordering was
/// cross-checked against the server's `switch (entry.DatFileId)`.
///
/// **The set is re-emitted from the dat's own bytes rather than re-encoded.** In the original client,
/// every initialized file contributes its load result to the final success condition and the response
/// copies that loaded payload. This Rust function instead skips an unreadable set while still
/// avoiding re-encoding ambiguity. The dat reader in `dereth_dat::iteration` expands a negative `-k`
/// by reading a **following `first` dword**; `dereth_protocol`'s (and ACE's) reader treats it as a run of `k - 1` with no operand.
/// Both consume the retail portal list (`count 2072`, then `-2072, 1`) to exactly the same end, and
/// which is right in general is a question this function does not need to answer to send the
/// right bytes.
///
/// **Known limitation:** the two serializers for the
/// mostly-consecutive integer set disagree for any list that is not one run, and no shipped dat exercises the
/// difference.
/// A product-id bit test loads the high-resolution dat when mask `0x4` is set.
/// `dereth_protocol::admin::DddInterrogation::PRODUCT_HIGHRES` is the same bit on the wire side.
pub const PRODUCT_HIGHRES: u32 = 4;

#[must_use]
pub fn ddd_interrogation_response(
    store: &dereth_dat::RetailDatStore,
    product_id: u32,
) -> dereth_protocol::admin::DddInterrogationResponse {
    use dereth_protocol::admin::{
        DddInterrogationResponse, MostlyConsecutiveIntSet, TaggedIterationList,
    };

    /// `"HiFi"` as a little-endian int, which is `DDDManager.HiFi_String_As_Int`.
    const HIFI: u32 = u32::from_le_bytes(*b"HiFi");

    /// The file's `0xFFFF0001` payload as `(count, the run-length dwords that follow)`.
    fn set_of(f: &dereth_dat::DatFile) -> Option<MostlyConsecutiveIntSet> {
        let raw = f.read(dereth_dat::divine::ITERATION_LIST).ok()?;
        let (head, rest) = raw.split_at_checked(4)?;
        let iterations = i32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let ints = rest
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(i32::from_le_bytes)
            .collect();
        Some(MostlyConsecutiveIntSet { iterations, ints })
    }

    let mut iters_with_keys = Vec::new();
    let mut push = |ty: u32, id: u32, f: &dereth_dat::DatFile| {
        if let Some(iterations) = set_of(f) {
            iters_with_keys.push(TaggedIterationList {
                dat_file_type: ty,
                dat_file_id: id,
                iterations,
            });
        }
    };
    push(0, 1, store.portal());
    push(1, 2, store.cell());
    push(1, 3, store.local());
    if product_id & PRODUCT_HIGHRES != 0 {
        if let Some(hi) = store.highres() {
            push(HIFI, 1, hi);
        }
    }
    DddInterrogationResponse {
        // Read the local language. 1 is English, which is the only `client_local_*.dat` this
        // install has; `-language` does not reach this yet.
        client_language: 1,
        iters_with_keys,
        // "a second `CAllIterationList`, always empty in practice" -- and ACE does not read it.
        iters_without_keys: Vec::new(),
        flags: 0,
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
/// does not print it. `main.rs`'s own summary prints it and `tests/gpu/world/interiors.rs` reads it, so its
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
    /// notice bus that reaches a UI element — the same declared deviation
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
    /// Whether this frame's teleport step ([`Self::teleport_use_time`]) has run. A front end runs
    /// it inside its own UI step, where the teleport overlay ticks; the frame runs it itself when
    /// no front end did, so a client with no UI, or one that never calls it, still tells the
    /// server it has finished loading. Cleared at the top of every frame.
    teleport_ticked: bool,

    /// The `AnimAssets` a preview space builds its objects through. One per process, because it
    /// memoises every setup record and animation record it decodes.
    pub anim_assets: std::sync::Arc<crate::anim_assets::DatAnimAssets>,
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

impl<S: Shell> App<S> {
    /// Run the twelve startup steps.
    ///
    /// The platform and the presentation are the caller's, and they are asked for at the steps
    /// the client builds them: `platform` at step 12, once the data files are open, and
    /// `present` straight after it, handed the window it just produced -- a device needs the
    /// window's handles. A headless caller hands back what it already has.
    ///
    /// # Errors
    /// [`StartupError`] for any of the failures the client treats as fatal.
    pub fn bring_up(
        cfg: Config,
        platform: impl FnOnce(&Config) -> Result<Platform, StartupError>,
        present: impl FnOnce(
            &dyn WindowHost,
            u32,
            u32,
            &Config,
        ) -> Result<Box<S::Present>, StartupError>,
    ) -> Result<Self, StartupError> {
        Self::bring_up_with_store(cfg, None, platform, present)
    }

    /// [`Self::bring_up`] with the data files already open: step 10 takes `store` instead of
    /// opening [`Config::dat_dir`]. A platform with no file the client can open by path (a
    /// browser, which reads the player's files from its own storage) opens them itself. A later
    /// data patch still reopens [`Config::dat_dir`], which such a platform cannot, so the old view
    /// stands there.
    ///
    /// # Errors
    /// As [`Self::bring_up`].
    pub fn bring_up_with_store(
        cfg: Config,
        store: Option<std::sync::Arc<dereth_dat::RetailDatStore>>,
        platform: impl FnOnce(&Config) -> Result<Platform, StartupError>,
        present: impl FnOnce(
            &dyn WindowHost,
            u32,
            u32,
            &Config,
        ) -> Result<Box<S::Present>, StartupError>,
    ) -> Result<Self, StartupError> {
        // The client's initialization begins by forcing an 800x600 display size.
        //
        // Display preferences are loaded later in startup, after the force flag is already
        // set — so **the retail client opens its window at 800x600 whatever the profile names**,
        // and the saved resolution only reaches the presentation when gameplay-screen construction
        // lifts the force. Startup does not use the preference directly; it uses the preference
        // *through* that override.
        //
        // The one gate is this build's own. `Config::width`/`height` is also the headless render
        // extent, set directly by test files and capture runs, and a directly-set extent was never
        // display-preference load's answer. So the force only claims a presentation the preference
        // actually produced; see [`App::unforced_resolution`].
        let mut cfg = cfg;
        let unforced_resolution = (cfg.width, cfg.height);
        let forced_resolution = crate::config::FORCED_LOGIN_SIZE;
        let from_preference = cfg
            .load_display_preferences(None, true)
            .is_some_and(|p| (p.width, p.height) == unforced_resolution);
        if from_preference {
            cfg.width = forced_resolution.0;
            cfg.height = forced_resolution.1;
        }
        let forced_resolution = if from_preference {
            forced_resolution
        } else {
            unforced_resolution
        };
        let cfg = cfg;

        // Step 9: initialize networking, plus the connection portion of step 11.
        //
        // Network initialization allocates the client network object, the packet controller, and
        // five receive queues before the database opens because the dat cache uses queue 5. The
        // socket is bound later, when the run path connects before entering its frame loop.
        // Both are done here, because `App::new` is where this crate's fallible startup lives and
        // `App::run` returns a frame count rather than a `Result`. The observable difference is
        // only *when* a bad `-h` is reported, and it is still reported before any frame.
        //
        // A failure is fatal: startup returns the error and the process displays it before exit.
        //
        // **This is the normal path.** The shipped client connects unconditionally and
        // refuses to start without `-a` and `-h`, so it has no offline mode. This build does,
        // for the offline slices and the capture gate, and the way it keeps both is to make the
        // *credentials* the switch: given an account and a host, the client connects, exactly as
        // the launcher's child process does; given neither, it draws the world with no server.
        // `--no-connect` is the explicit opt-out, and giving one of the two without the other is
        // still the documented error rather than a silent offline start.
        let link = if cfg.will_connect() {
            cfg.require_account_and_host().map_err(|e| {
                // The dialog shows corestrings 205 whatever went wrong and discards accumulated
                // connection text, so the detail goes to the log, which
                // is where this rebuild's equivalent of retail's full output text lives.
                tracing::error!("{}", e.detail);
                StartupError::CommandLine(e.to_string())
            })?;
            // The connection sequence number is the low 32 bits of the current real-time
            // millisecond count. ACE reads it as `Timestamp` and otherwise ignores it.
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: the deliberate 32-bit wrap of a millisecond counter, not a float conversion.
            let seq = (crate::platform::clock::system_unix_time().map_or(0, |d| d.as_millis())
                & u128::from(u32::MAX)) as u32;
            let client_port = u16::try_from(cfg.client_port).unwrap_or(0);
            Some(NetLink::connect(
                &cfg.host,
                u16::try_from(cfg.port).unwrap_or(0),
                client_port,
                &cfg.account,
                &cfg.vg_password,
                seq,
            )?)
        } else {
            None
        };
        // A headless client with no server answers for the server: see `crate::server_stub`.
        let server_stub =
            (link.is_none() && cfg.headless).then(crate::server_stub::ServerStub::default);

        // Step 10: open the data files.
        let store = match store {
            Some(store) => store,
            None => std::sync::Arc::new(crate::assets::open_data_files_with(
                &cfg.dat_dir,
                cfg.world_dat_dir.as_deref(),
            )?),
        };

        // Step 12: UI initialization -> (windowed, title, 800, 600, visible, "").
        // The window and its metrics; `Platform` is supplied by a headless caller
        // and built here otherwise. Window creation lives in
        // [`crate::platform::window::open_window`].
        // The three platform seams, taken here because this is step 12 --
        // and the clock's epoch begins here rather than at an earlier startup step.
        // `Platform::open` opens the window first and takes the clock after it.
        let Platform {
            window,
            clock,
            pacer,
            dialog,
        } = platform(&cfg)?;
        let (client_w, client_h) = window.client_size();
        // A headless run has no window, so the rectangle it would have had is
        // seeded from the normal creation placement over the headless screen. That is
        // the *retail* placement — the screen centre — and it is deliberately not the divergence:
        // the divergence is about not moving a window that already exists, and a window being
        // created has no position to keep.
        // `false`, not `cfg.display.full_screen`: the window is created windowed whatever the
        // preference says, and full screen is applied on entering the game. See
        // [`Self::follow_gameplay_full_screen`] and `platform::window::open_window`.
        let frame_metrics = window.frame_metrics().unwrap_or((0, 0, 0));
        let window_rect = if window.has_window() {
            window.window_rect()
        } else {
            let seed = dereth_client_contract::window_proc::placement(
                false,
                true,
                i32::try_from(cfg.width).unwrap_or(i32::MAX),
                i32::try_from(cfg.height).unwrap_or(i32::MAX),
                &window.screen_metrics(frame_metrics),
            );
            Some(dereth_client_contract::window_proc::Rect {
                left: seed.x,
                top: seed.y,
                right: seed.x + seed.cx,
                bottom: seed.y + seed.cy,
            })
        };

        // Step 11 of graphics-engine startup, after which the client is ready.
        //
        // Only when the caller did not bring its own presentation. The device is
        // built here rather than by [`App::bring_up`] because it needs the window handles the block
        // above has just produced.
        let present = present(&*window, client_w, client_h, &cfg)?;

        let timer = Clock::init();

        let mut pump = Pump::new();
        pump.state.is_ready = true;
        // The full-screen preference, which is what Alt+Enter flips
        // (the event loop's epilogue) and what
        // render-preference polling compares against its shadow every frame.
        //
        // **It starts `false` regardless of the preference.** The preference
        // lives in `cfg.display.full_screen` and is what the options page writes;
        // it is applied on entering gameplay. Seeding the
        // shadow from the preference here would make the first frame full screen and then make
        // [`Self::follow_gameplay_full_screen`] undo it, which is a visible flash rather than a
        // no-op. See that function for the whole rule.
        //
        // `allow_full_screen_mode` starts `false` for the same reason: there is no gameplay
        // screen yet. [`Self::do_event_loop`] maintains it from the second line of every frame.
        pump.state.allow_full_screen_mode = false;
        // A headless run has no window to lose focus, so it is always the foreground application.
        // That matters: the frame pacer would otherwise cap every frame at 99 ms.
        pump.state.is_active_app = cfg.headless;
        pump.state.is_minimized = !cfg.headless;

        // Read before the struct literal because `timer` is moved into it.
        let clock_start = timer.cur_time;
        // For the same reason: `cfg` is moved into it. `false` for the reason the
        // pump shadow above is.
        let started_full_screen = false;
        let started_sync_to_refresh = cfg.display.sync_to_refresh;

        // Built before the struct literal because `store` is moved into it.
        let anim_assets = std::sync::Arc::new(crate::anim_assets::DatAnimAssets::new(
            std::sync::Arc::clone(&store),
        ));

        let objects = crate::objects::ObjectStream::with_store(std::sync::Arc::clone(&store));
        // Before the struct literal, for the same reason `anim_assets` is: `cfg`
        // is moved into it.
        let ddd = crate::ddd::DddPatcher::new(cfg.dat_dir.clone());
        Ok(Self {
            cfg,
            state: AppState::Startup,
            pump,
            timer,
            clock,
            pacer,
            events: FrameEvents::new(),
            present,
            world: None,
            store,
            taboo_table: None,
            window,
            input: CameraInput::default(),
            char_input: crate::character::CharacterInput::default(),
            movement: crate::character::MovementCommands::default(),
            mouse_look: false,
            last_cursor: None,
            last_time: 0.0,
            last_target_tracking: None,
            last_option_environment: None,
            environment_override: crate::environment::EnvironmentOverrideState::default(),
            link,
            server_stub,
            script: EnterWorldScript::default(),
            last_link_status: None,
            last_rejected: 0,
            last_net_error: None,
            dialog,
            connect_failure: None,
            objects,
            pending_scene: None,
            scene_config: None,
            last_viewer_block: None,
            last_viewer_cell: None,
            last_object_report: None,
            viewer_block_gate: crate::report_gate::ReportGate::default(),
            object_report_gate: crate::report_gate::ReportGate::default(),
            unowned_gate: crate::report_gate::ReportGate::default(),
            unowned_suppressed: 0,
            playable_at: None,
            // The reporter seeds `last_sent_position_time` with the clock start, not zero.
            position: dereth_client_net::client_session::PositionReporter::new(clock_start),
            last_jump_request: None,
            interaction: crate::interaction::Interaction::new(),
            actions: crate::actions::ActionQueue::default(),
            audio: None,
            host_state: HostState::default(),
            pending_ddd: Vec::new(),
            ddd,
            ddd_invalidation: None,
            pending_auto_layout: false,
            applied_full_screen: started_full_screen,
            applied_resolution: (client_w, client_h),
            applied_sync_to_refresh: started_sync_to_refresh,
            // True before a single preference has been read.
            use_forced_resolution: true,
            forced_resolution,
            unforced_resolution,
            frame_metrics,
            window_rect,
            windowed_rect_before_full_screen: None,
            startup_sound_played: false,
            ui_sound_table: None,
            hud: S::Hud::default(),
            teleport: crate::teleport::Teleport::new(),
            teleport_ticked: false,
            anim_assets,
        })
    }

    /// The asset seam, which is all any other consumer ever needs.
    #[must_use]
    pub fn assets(&self) -> &dyn dereth_primitives::AssetSource {
        &*self.store
    }

    /// Load the static scene — one landblock's terrain and scenery, plus the LOD window
    /// around it — and point the free camera at it.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the region, the landblock or a device resource is unavailable.
    pub fn load_static_scene(
        &mut self,
        cfg: crate::scene::SceneConfig,
    ) -> Result<(), StartupError> {
        // The switches this scene was built from are kept so the world can be
        // built a *second* time; see [`App::scene_config`].
        self.scene_config = Some(cfg);
        self.present
            .load_world(&self.store, cfg, &mut self.world)
            .map_err(|e| StartupError::Device {
                cause: format!("{e}"),
            })?;
        if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
            world.set_environment_override_state(self.environment_override.clone());
        }
        Ok(())
    }

    /// Start input, sound, and the UI shell in the client's order.
    ///
    /// Split out of [`App::bring_up`] because only the first is fatal and because the two offline
    /// slices and the headless capture gate must keep running with none of them.
    /// Startup order is *data files* → *network* → *keymap* → *UI*, and sound comes up after
    /// the dat cache; the keymap is loaded before the UI
    /// because a text element registers input maps as soon as a screen builds one.
    ///
    /// # Errors
    /// [`StartupError::Device`] when the UI's own two dat objects are missing; without them no
    /// screen can be reached.
    pub fn start_shell(&mut self, shell: &mut S) -> Result<(), StartupError> {
        // The front end's device input first: a client with no key bindings is useless but can
        // still draw, so a front end that cannot load its bindings says so and carries on.
        shell.start_input(self);

        // A headless run never takes the machine's audio endpoint.
        let want_device = self.cfg.sound && !self.cfg.headless;
        // Seed the random stream from `time(NULL)`. Called once immediately after timer
        // initialization and unconditionally, i.e.
        // **not** gated on DirectSound the way the CRT `srand` is.
        //
        // **A headless run seeds it with 1 instead**, for the same reason `Clock::fixed_step`
        // exists and by the same rule: `--headless` is a regression harness and its output must be
        // reproducible. The seed decides which row of a multi-row sound table
        // entry plays and *when* each ambient fires, so
        // with the wall clock in it two runs of the same frame report a different number of live
        // voices — which is what the headless acceptance gate compares. The windowed path reads the
        // clock, exactly as the client does; nothing but the headless harness is affected, and the
        // draw order within a seed is unchanged.
        let seed = if self.cfg.headless {
            1
        } else {
            crate::audio::ran2_seed(&*self.clock)
        };
        // The eight `Sound.*` preferences the `UserPreferences.ini` carries, rather than
        // `Prefs::default()`. The shipped client loads user preferences before the sound
        // manager registers anything, so file values are already present when sound starts.
        let sound_prefs = crate::audio::prefs_from_file(&self.cfg.preferences_file);
        let mut audio = crate::audio::Audio::new(sound_prefs, seed, want_device);
        // **The wizard's opening roll comes out of these two seeds.**
        // Character randomization draws the heritage and gender from `ran2`, the same stream
        // sound uses, and everything else from CRT `rand()`. The CRT stream
        // is seeded here or not at all -- sound initialization calls `srand`
        // only when DirectSound came up. A headless, silent run is therefore (1, 1) and its roll
        // is reproducible.
        self.host_state.chargen_seeds = Some((seed, audio.crt_seed()));
        if audio.has_device() {
            tracing::info!("audio device at {} Hz", audio.device_rate());
        }
        // The one sound that proves the path: the UI button-press sound (0x72) out of the UI sound
        // table resolves, through entry 7
        // (play a sound-table row from the centre).
        if let Some(table) = crate::assets::enum_did(
            &*self.store,
            crate::audio::UI_SOUND_TABLE_GROUP,
            crate::audio::UI_SOUND_TABLE_ENUM,
        ) {
            if audio.load_sound_table(&self.store, table) {
                // Loading a table creates every non-zero sound referenced by its recursively
                // unpacked nodes. The UI table is not merely the
                // button-click row: the same resident object supplies portal and Admin_Environs
                // sounds. Loading only the click row would make those other producers reach a
                // valid table selection and then mix silence because their waves do not exist.
                audio.create_table_waves(&self.store, table);
                self.ui_sound_table = Some(table);
            }
        }
        self.audio = Some(audio);

        // Initialize the UI after input and sound.
        if self.cfg.ui {
            shell.start_ui(self)?;
        }
        // `Attribute2ndTable 0x0E000003`, which the vitals bar needs
        // for every maximum it shows.
        // The server's era, when the launcher passed it, wins over the one the data files suggest.
        if let Some(era) = self.cfg.era {
            self.hud.era.era = era;
            self.hud.era.era_announced = true;
        }
        self.hud.load_tables(&self.store, &self.objects.world);
        // The same table computes the maximum a received current vital is clamped to, which the
        // world's quality-update paths apply before storing.
        if let Some(t) = self.hud.vitals_table {
            self.objects.world.install_vital_formulas(t);
        }
        // The maximum is enchanted only as far as the quality filter allows.
        if let Some(f) = self.hud.quality_filter.clone() {
            self.objects.world.install_quality_filter(f);
        }
        // Inventory refusal text is built by `World`, but its material names are
        // process-owned DAT content loaded here with the rest of the HUD's enum mappers. Resolve
        // them through the one material-type-to-string transcription and give the model strings,
        // not a second table decoder or material-name formatter.
        let material_names = self.hud.material_names.as_ref().map_or_else(
            std::collections::BTreeMap::new,
            |names| {
                names
                    .enum_to_name
                    .iter()
                    .filter_map(|(id, _)| {
                        crate::hud::material_name_of(Some(names), *id).map(|name| (*id, name))
                    })
                    .collect()
            },
        );
        self.objects.world.install_material_names(material_names);
        // The enum lookup `(0x11, 2, 0x14)` resolves to the shipped TabooTable `0x0E00001E`.
        // Load it once beside the other UI-owned tables; the option itself is synchronized at the
        // head of every HUD event batch because a world reset replaces `Scroll`.
        let taboo_id = DataId(0x0E00_001E);
        self.taboo_table = self
            .store
            .read(taboo_id)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                <dereth_assets::TabooTable as dereth_assets::Decode>::decode_payload(
                    taboo_id, &bytes,
                )
                .map_err(|e| e.to_string())
            })
            .map(std::sync::Arc::new)
            .map_err(|e| tracing::warn!("TabooTable: {e}"))
            .ok();
        // The enum lookup `(7, 2, 0x11)` selects the `ChatPoseTable`, registered by
        // the client and fetched as chat-pose lookup's first act. Loaded once, beside the
        // HUD's own tables, because `Interaction` has no dat store: it is the table that turns
        // `*wave*` into a motion, a `0x01E1` and a local echo, and the pose lookup's
        // `if (!table) return false` is exactly what a miss here reproduces.
        if self.interaction.chat_pose_table.is_none() {
            match self
                .store
                .ids_of(dereth_dat::DbType::ChatPoseTable)
                .into_iter()
                .next()
                .ok_or_else(|| "client_portal.dat carries no ChatPoseTable".to_owned())
                .and_then(|id| {
                    self.store
                        .read(id)
                        .map_err(|e| e.to_string())
                        .and_then(|b| {
                            <dereth_assets::tables::ChatPoseTable as dereth_assets::Decode>::decode_payload(id, &b)
                                .map_err(|e| e.to_string())
                        })
                }) {
                Ok(t) => self.interaction.chat_pose_table = Some(std::sync::Arc::new(t)),
                Err(e) => tracing::warn!("ChatPoseTable: {e}"),
            }
        }
        Ok(())
    }

    /// Play the one sound the bring-up proves the device with, once.
    ///
    /// The UI button-press sound (`0x72`) through entry 7, which is the path the UI
    /// `MediaPlayback`'s sound media descriptor takes for every button in the game.
    pub fn play_startup_sound(&mut self) {
        if self.startup_sound_played {
            return;
        }
        let Some(table) = self.ui_sound_table else {
            return;
        };
        let Some(audio) = self.audio.as_mut() else {
            return;
        };
        audio.play_ui_sound(dereth_audio::UiSoundRef::Table {
            table,
            stype: crate::audio::SOUND_UI_BUTTON_PRESS,
        });
        self.startup_sound_played = true;
    }

    /// Apply the environment-control message's local visual and sound effects.
    ///
    /// `true` means the event belonged to the client's visual `0..=6` / `9999` cases or the
    /// `101..=124` sound range, including the three sound values which deliberately fall through
    /// its switch. Malformed and unknown options remain available to the generic diagnostics.
    fn apply_admin_environs(
        &mut self,
        shell: &S,
        event: &dereth_client_net::client_session::SessionEvent,
    ) -> bool {
        let dereth_client_net::client_session::SessionEvent::UiEvent { opcode, blob } = event
        else {
            return false;
        };
        if *opcode != dereth_protocol::Opcode::ADMIN_ENVIRONS {
            return false;
        }
        let Ok(message) = dereth_protocol::read_body_padded::<dereth_protocol::admin::AdminEnvirons>(
            blob.get(4..).unwrap_or_default(),
        ) else {
            return false;
        };
        if let Some(blank) = self
            .environment_override
            .apply_option(message.environ_option)
        {
            self.hud.set_admin_radar_blank(blank);
            if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
                world.sync_environment_override_flags();
            }
            return true;
        }
        if !dereth_protocol::admin::AdminEnvirons::is_sound_cue(message.environ_option) {
            return false;
        }

        // The outer range encloses three absent switch cases: they are handled no-ops, not an
        // unknown Admin_Environs operation. All remaining exits preserve the client's order:
        // player physics object, UI system/table, and only then centered table playback.
        let Ok(option) = u32::try_from(message.environ_option) else {
            return true;
        };
        let Some(stype) = dereth_audio::trigger::environ_sound_type(option) else {
            return true;
        };
        let Some(player) = self.objects.player() else {
            return true;
        };
        if self.objects.world.physics(player).is_none() {
            return true;
        }
        // The live UI-system lookup is a separate client guard from the cached table. The
        // latter can outlive a screen transition in this host; it must not stand in for a UI
        // system that was never created.
        if !shell.has_ui() {
            return true;
        }
        if let (Some(table), Some(audio)) = (self.ui_sound_table, self.audio.as_mut()) {
            audio.play_ui_sound(dereth_audio::UiSoundRef::Table { table, stype });
        }
        true
    }

    /// The teleport animation's per-frame step, inside frame step 7.
    ///
    /// Three things reach the rest of the client from here and nothing else does: the world stops
    /// being drawn, the projection's field-of-view distance is overridden, and
    /// the two portal sounds are played. Everything is a function of the teleport state; nothing
    /// accumulates per frame.
    pub fn teleport_use_time(&mut self, shell: &S) {
        self.teleport_ticked = true;
        let now = self.timer.cur_time;
        let before = (self.teleport.tunnels_played, self.teleport.anim.state);
        let was_teleporting = self.teleport.anim.teleport_in_progress;
        self.teleport.anim_use_time(now, self.game_view_distance());
        // The player's teleport-in-progress flag holds the busy cursor up from the moment a
        // portal (or the log-in, or a log-off's fade) starts until the world has faded back in.
        let teleporting = self.teleport.anim.teleport_in_progress;
        if teleporting != was_teleporting {
            let busy = &mut self.objects.world.magic.busy_count;
            *busy = if teleporting {
                busy.saturating_add(1)
            } else {
                busy.saturating_sub(1)
            };
        }
        // A report line per state change: an animation that is *absent* on a live run must be
        // visible in the log, and a run that plays it must be able to say so.
        let after = self.teleport.anim.state;
        if after != before.1 {
            tracing::debug!(
                "portal-tunnel animation {:?} -> {after:?} at t={now:.2} \
                 (world {}, vdist {:?}, tunnel #{})",
                before.1,
                if self.teleport.world_hidden() {
                    "hidden"
                } else {
                    "drawn"
                },
                self.teleport.view_distance(),
                self.teleport.tunnels_played,
            );
        }
        let _ = before.0;
        // The rotation block's
        // notice send -- one fixed
        // built-in literal ( `L"In Portal Space - Please Wait..."`) on the tunnel's
        // entry frame and on every re-aim — lands on the client scroll handler,
        // which adds `(text, 0x1A, true, 0)`, drained by the
        // HUD at the top of the next frame into the spew panel (type `0x1A`'s only default
        // destination). Without this the animation raises the effect and nothing takes it.
        for (channel, text) in self.teleport.take_notices() {
            self.objects
                .world
                .scroll
                .on_display_string_info(channel, &text);
        }
        // World drawing reads `hidden`; the normal world render reads the override.
        // Credits has no backdrop element. Retail's black is the frame-begin clear:
        // the character-management screen builds the rotating preview, while the credits screen
        // only creates its two authored roots. Keep this first slice mode-specific so
        // the character-management presentation remains independent; entering Credits hides the
        // retained world immediately, and its existing action back to character management shows
        // that presentation again on the next frame.
        let credits = shell.hides_world();
        self.present.set_world_view_state(
            self.teleport.world_hidden() || credits,
            self.teleport.view_distance(),
        );
        // The client sends the character login-complete notification, which is `0x00A1` on the Weenie queue.
        //
        // ACE uses it to run `Player.HandleLoginComplete`: measured on the live shard, a client
        // that never sends it is answered with `0xF659 Character_CharacterError(1)
        // ID_CHAR_ERROR_LOGON` a few seconds after entering the world. It belongs here because the
        // *animation* is what calls it -- once at the end of the world fade-in, and once in
        // the idle teleport-animation state for a teleport that played no animation.
        //
        // The client's own gate requires the player object to exist **and** every id in the two
        // content-profile lists to have a world object.
        // Only the first half is checked here; the contained-objects half needs a reachable answer
        // from the object tables and is not checked. Until it passes, the client's own
        // pending-login-complete retry is reproduced: the flag stays set and is tried again
        // next frame.
        if self.teleport.login_complete_owed() && self.objects.player().is_some() {
            if let Some(link) = self.link.as_mut() {
                match link
                    .net
                    .session
                    .send_action(&dereth_protocol::login::CharacterLoginCompleteNotification)
                {
                    Ok(stamp) => {
                        tracing::debug!("0x00A1 login complete (stamp {stamp})");
                        self.teleport.login_completes_sent += 1;
                        self.teleport.login_complete_sent();
                    }
                    Err(e) => tracing::warn!("0x00A1 would not encode: {e}"),
                }
            } else {
                // No server to tell; do not spin on it every frame.
                self.teleport.login_complete_sent();
            }
        }
        let sounds = self.teleport.take_sounds();
        if !sounds.is_empty() {
            if let (Some(table), Some(audio)) = (self.ui_sound_table, self.audio.as_mut()) {
                for stype in sounds {
                    audio.play_ui_sound(dereth_audio::UiSoundRef::Table { table, stype });
                }
            }
        }
    }

    /// The smart-box field-of-view distance with its override off: the game view distance.
    ///
    /// The aspect is built the same way the drawn world's view parameters build it, so the
    /// distance the animation ramps away from is the one the world is actually drawn at.
    fn game_view_distance(&self) -> f32 {
        use dereth_client_contract::camera as cam;
        let (w, h) = self.present.size();
        #[allow(clippy::cast_precision_loss)]
        // LINT-OK: a back-buffer extent, at most a few thousand.
        let (fw, fh) = (w as f32, h as f32);
        // `Render.AspectRatio` and `Render.FieldOfView`, from the live scene when
        // there is one; the `Config` copy is the answer before a world is loaded, matching the
        // process render preferences at that point.
        let prefs = self
            .present
            .scene(self.world.as_ref())
            .map_or(self.cfg.render, |s| s.render_preferences());
        let display = prefs.aspect().display_aspect_ratio(fw, fh);
        let aspect = cam::compute_aspect_for_viewport(fw, fh, display, false);
        let fov = cam::fov_y_from_preference(prefs.game_fov_rad(), aspect);
        cam::view_distance_from_fov(fov)
    }

    /// The teleport state, for the report line and the tests.
    #[must_use]
    pub const fn teleport(&self) -> &crate::teleport::Teleport {
        &self.teleport
    }

    /// The teleport state, mutably, so a test can feed it the session events a live server would
    /// and then let the application's own frame do the rest.
    pub const fn teleport_mut(&mut self) -> &mut crate::teleport::Teleport {
        &mut self.teleport
    }

    /// The HUD state, for the report line and the tests.
    #[must_use]
    pub fn hud(&self) -> &S::Hud {
        &self.hud
    }

    /// The HUD state, mutably, so a test can feed it a recorded session's events.
    pub fn hud_mut(&mut self) -> &mut S::Hud {
        &mut self.hud
    }

    /// [`crate::hud::Hud::apply_events`] against this application's own object tables.
    ///
    /// `apply_events` takes the tables because `0x0013`'s two inventory lists
    /// go into them (`dereth_client_model`'s `update_object_inventory`), and the two
    /// fields cannot be borrowed through `App` from outside. This is the same call the frame makes.
    pub fn apply_hud_events(
        &mut self,
        shell: &mut S,
        events: &[dereth_client_net::client_session::SessionEvent],
    ) -> Vec<dereth_client_contract::chat::interface::ChatMessage> {
        // Text-scroll composition reads this filter bit for every line.
        // Keep the decoded table on the world-side scroll so both the actual Scroll producer and
        // HUD's still-direct incoming handlers use one matcher. This runs before Hud drains or
        // composes anything, and after any WorldObjects reset has installed a fresh world.
        self.objects.world.scroll.filter_language = self
            .objects
            .world
            .player_system
            .options
            .get(dereth_client_model::player::options::option::FILTER_LANGUAGE);
        self.objects
            .world
            .scroll
            .taboo_table
            .clone_from(&self.taboo_table);
        // A Turbine callback is a synchronous notice to the CURRENT chat subscribers.
        // Preserve that generation through the deferred Hud delivery, not across a rebuild.
        let chat_generation = shell.chat_generation();
        self.hud.set_turbine_chat_generation(chat_generation);
        // Most frames have no combat-quality update. Avoid copying every object's selection
        // geometry unless an integer update can reach the callback below.
        if !events.iter().any(|e| {
            matches!(e, dereth_client_net::client_session::SessionEvent::UiEvent { opcode, .. }
                if *opcode == dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_INT
                    || *opcode == dereth_protocol::Opcode::QUALITIES_UPDATE_INT)
        }) {
            return self.hud.apply_events_with_combat_mode_handler(
                events,
                &mut self.objects.world,
                shell.ui_requests(),
                &mut |_, _| {},
            );
        }
        let origin = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(crate::character::Character::position);
        let phys =
            crate::selection_geometry::SceneSelectionPhysics::new(origin.as_ref(), &self.objects);
        let radius = dereth_client_contract::radar::radar_range(
            origin
                .as_ref()
                .is_some_and(|p| dereth_physics::landdefs::is_outdoors(p.cell)),
        );
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        let interaction = &mut self.interaction;
        let mut combat_modes = Vec::new();
        let applied = self.hud.apply_events_with_combat_mode_handler(
            events,
            &mut self.objects.world,
            shell.ui_requests(),
            &mut |world, mode| {
                interaction.on_combat_mode_quality_changed(world, mode, &phys, radius, now);
                combat_modes.push(world.combat.combat_mode.raw());
            },
        );
        // The device input follows each combat-mode change, in the order they landed. Nothing
        // reads a control between the change and this line, so telling it after the batch is
        // telling it at the change.
        for mode in combat_modes {
            shell.control_notice(crate::shell::ControlNotice::CombatMode(mode));
        }
        applied
    }

    /// The character controls the input seam has latched, for the tests.
    ///
    /// The only writer is [`Self::apply_input_actions`], which runs on what
    /// `fire::walk_input_maps` produced -- so reading `forward` here is reading "did a control
    /// survive the walk", which is precisely what the typing barrier decides.
    #[must_use]
    pub const fn char_input(&self) -> crate::character::CharacterInput {
        self.char_input
    }

    /// The camera/flycam controls, for the tests.
    #[must_use]
    pub const fn camera_input(&self) -> CameraInput {
        self.input
    }

    /// The command interpreter's three command lists and the counters
    /// on them, for the tests.
    ///
    /// The one that matters is
    /// [`crate::character::MovementCommands::transient_motions_issued`], which is the denominator
    /// for `move_player`'s do-motion/stop-motion tail: *no emote key was pressed* and
    /// *the drain never ran* are the same reading of an empty queue without it.
    #[must_use]
    pub const fn movement(&self) -> &crate::character::MovementCommands {
        &self.movement
    }

    /// The movement command interpreter's three command lists, for the tests.
    ///
    /// "Hold `A`, press `D`, release `D`, still turning left" is a statement about the **turn
    /// list**, not about a bool, so the test has to be able to read the list.
    #[must_use]
    pub const fn movement_commands(&self) -> &crate::character::MovementCommands {
        &self.movement
    }

    /// The body's real camera controller, for the tests.
    ///
    /// The eight commands of [`Self::apply_world_camera_action`] are arithmetic and ordering —
    /// a viewer offset, a stiffness, two mode flags — so they are asserted against the client's
    /// own arms rather than by eye.
    #[must_use]
    pub fn camera_control(&self) -> Option<&crate::camera::CameraControl> {
        self.world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(|c| &c.camera)
    }

    /// Return whether anything has asked the main loop to stop.
    ///
    /// A test reads it to assert that a plain `Escape` does not end the process. Ending the loop
    /// on `Escape` straight from the raw event drain would let one `Escape` with the chat entry
    /// focused terminate a live client.
    #[must_use]
    pub const fn device_done(&self) -> bool {
        self.pump.state.is_done
    }

    /// Everything this session's frames did, in order, with the totals every
    /// counter accessor above reads. See [`crate::frame_events`].
    #[must_use]
    pub const fn frame_events(&self) -> &FrameEvents {
        &self.events
    }

    /// The sound subsystem, for the tests and for the report line.
    pub fn audio_mut(&mut self) -> Option<&mut crate::audio::Audio> {
        self.audio.as_mut()
    }

    /// What the screens read out of globals this frame.
    #[must_use]
    pub fn host_state(&self) -> &HostState {
        &self.host_state
    }

    /// The same, mutably, for a test that has no server to hear it from.
    ///
    /// [`Self::build_host_state`] rewrites the five fields it reads off the link every frame and
    /// leaves the rest alone, so the character set, the world name and the char-gen response
    /// survive being written here -- which is what lets the character screens be driven offline.
    pub fn host_state_mut(&mut self) -> &mut HostState {
        &mut self.host_state
    }

    /// Assemble [`HostState`] from the session, once per frame.
    ///
    /// Every value is read from somewhere that already exists: the character set is `0xF658`,
    /// `in_world` is `SessionState::Playable`, and the error is whatever ended the session.
    fn build_host_state(&mut self) {
        let has_net = self.link.is_some();
        let (connected, in_world, error) = match self.link.as_ref() {
            Some(link) => (
                link.net.status() == crate::net::LinkStatus::Connected,
                link.net.session.state()
                    == dereth_client_net::client_session::SessionState::Playable,
                // The transport's own error if it has one, and otherwise **whatever was already
                // latched**. Overwriting the field with `None` every frame would erase a character
                // error raised by [`Self::recv_disconnect_notice`] *in the same frame it was
                // raised*, before the front end could see the edge, and a disconnected client
                // would never reach the disconnected screen with a server on the line.
                // Latching is the same statement the `None` arm below already makes, and for the
                // same reason: the notice is raised once and there is no reconnect path.
                link.net
                    .error()
                    .map(|c| c.id_string().to_string())
                    .or_else(|| self.host_state.error.clone()),
            ),
            // With no `--connect` there is no shared network to ask and no packet controller to wait
            // for. On this no-controller branch the set is not required, so readiness is immediate.
            // The connect meter is reported
            // satisfied because there is no connection to be waiting on -- a statement about *this
            // build having no network*, not a claim about the retail client, which always connects
            // before and therefore always has one.
            //
            // The error is **latched**, not cleared: the character-error event is a notice raised
            // once, and with no shared network there is nothing to ask
            // whether it is still true. Overwriting it with `None` every frame would erase a
            // locally-raised character error — the subscription-expired error is exactly that —
            // before the shell's own edge test could see it.
            None => (true, false, self.host_state.error.clone()),
        };
        self.host_state.connected = connected;
        self.host_state.has_packet_controller = has_net;
        self.host_state.in_world = in_world;
        self.host_state.error = error;
        // With a live connection the DDD stream is real and `patch_finished` is what the
        // patch-time-end message says rather than an assumption. Without one there is no shared
        // asset cache to interrogate anything, so the phase is over before it starts.
        if !has_net {
            self.host_state.patch_finished = true;
        }
        // Drained into the screen once per frame; `HostState` is cloned into `UiShell::frame`, so
        // the queue must be emptied here or every event would be delivered again next frame.
        self.host_state.ddd = std::mem::take(&mut self.pending_ddd);
    }

    /// Step 7's UI update and mode switch. Player-description arrival triggers
    /// loading the automatic screen layout.
    ///
    /// Kept as a narrow test seam for the path-and-file stations. The production arm is
    /// separately covered by a socket-free transport station, which delivers a wire-framed
    /// `0x0013` through the client network, `Session` and the App event drain.
    pub fn note_player_description(&mut self) {
        self.pending_auto_layout = true;
    }

    /// Whether [`Self::note_player_description`]'s load is still outstanding.
    #[must_use]
    pub fn auto_layout_pending(&self) -> bool {
        self.pending_auto_layout
    }

    /// Frame step 7: the UI update, the mode switch, and everything the front end does inside it.
    ///
    /// What is here runs whether or not there is a UI: the notices the UI would have taken are
    /// drained all the same, because a notice has no replay history and a UI that comes up later
    /// must not see an old one. The front end's own step is [`Shell::ui_frame`]; with no UI the
    /// input manager still gets its per-frame tick.
    fn ui_use_time(&mut self, shell: &mut S, now: dereth_primitives::LocalTime) {
        // The previous frame's unclaimed actions expire here, before this frame's are produced.
        self.actions.begin_frame();
        shell.service_dialogs(&mut self.interaction, &mut self.objects.world, now);
        // Communication notices affect command routing, so the existing subscriber receives
        // them before shell input, not in the late display-only power-bar batch below. If a mode
        // is queued, this is still the outgoing live subscriber; the new `post_init` reads globals.
        let chat_focus_notices = self.objects.world.chat.take_talk_focus_notices();
        // The player's `transient_state & 1`, the one thing
        // the log-off confirmation checks before refusing an airborne log-off. Read here, before
        // `UiShell::frame_with_dispatch` runs the screen's per-frame step, so the frame that
        // answers *Yes* is judged on **this** frame's contact state. A frame with no body yet
        // reports "on the ground": that is the client's null-player arm reduced to the one answer a
        // player who is standing on a screen can actually be in.
        let player_airborne = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .is_some_and(|c| !c.in_contact());
        shell.before_ui_input(
            &mut self.objects.world.chat,
            player_airborne,
            chat_focus_notices,
        );
        // Drain even with no UI/gameplay subscriber. Retail notices have no replay history;
        // keeping them while headless or on another screen would grow without bound and deliver
        // old combat into a later UI. The source ordering survives multiple edges in one frame.
        let power_bar_notices = self.objects.world.combat.take_power_bar_notices();
        // Notice subscribers have no replay history. Drain before every no-UI/other-screen
        // return, and do not deliver an old field's notices into a replacement generation.
        let external_container_notices = self.interaction.take_external_container_notices();
        self.hud.pending_external_container.clear();
        // Slumlord range exit is a UI notice, not a second proximity
        // poll. Drain it with the other one-generation panel notices below.
        let slumlord_range_exits = self.interaction.take_slumlord_range_exits();
        // Book range exit has the same one-generation UI-notice shape,
        // using the current book id as its stale-registration guard.
        let book_range_exits = self.interaction.take_book_range_exits();
        // The salvage window's notices, drained and delivered on exactly the same
        // terms for exactly the same reason: a panel generation that did not exist when the notice
        // was emitted keeps its `post_init` state, and an undelivered batch must not survive a
        // screen change and open a window for a tool used two screens ago.
        let salvage_notices = self.interaction.take_salvage_notices();
        self.hud.clear_salvage_notices();
        // Accepted secure-trade item ids, drained on the same terms and for the
        // same reason as the salvage batch
        // above: an undelivered batch must not survive a screen change and put an item on a trade
        // table that a different generation of the window owns.
        let trade_for_dummies = self.interaction.take_trade_for_dummies();
        self.build_host_state();
        shell.drive_pregame_screens(self);
        // Before the pointer events are dispatched: a click reaches smart-box object
        // search from inside `dispatch` below, and it must be measured against
        // the rectangle `<SBOX>` occupies *this* frame.
        self.note_game_viewport(shell);
        if !shell.has_ui() {
            // An absent local UI/module consumer has no replay history for placement calls, and
            // with no UI there is no request queue for any to be waiting in.
            // Destroying the external-container panel unregisters its range watches.
            // on_view_contents still registers on the model seam for non-App consumers; no
            // absent UI subscriber may retain those watches in a running application.
            self.objects
                .world
                .object_range_checks
                .unregister_all(dereth_client_model::range::RangeHandler::ExternalContainer);
            // No UI, but the device input still gets its per-frame tick: the input manager's
            // per-frame step is step 6 of the frame, and the repeat sweep is what makes a held key
            // repeat at all.
            shell.input_use_time(self, now);
            // No UI, so nothing else runs the teleport step: without it the login-complete
            // notification never goes out and the server never finishes the player's log-in.
            self.teleport_use_time(shell);
            shell.hand_on_actions(&mut self.actions);
            return;
        }
        shell.ui_frame(
            self,
            now,
            UiNotices {
                power_bar: power_bar_notices,
                external_container: external_container_notices,
                slumlord_range_exits,
                book_range_exits,
                salvage: salvage_notices,
                trade_for_dummies,
            },
        );
        // The teleport step belongs to the frame, not to a front end: a UI that did not run it
        // inside its own step has it run here, once, so the login-complete notification and the
        // world's hide and reveal never depend on a front end remembering to call it.
        if !self.teleport_ticked {
            self.teleport_use_time(shell);
        }
        shell.hand_on_actions(&mut self.actions);
    }

    /// Queue an action for the next frame, as a device would produce it. A script, a bot or a
    /// test acts through this; it reaches the same handlers a key does.
    pub fn inject_action(&mut self, action: crate::actions::Action) {
        self.actions.inject(action);
    }

    /// Queue requests for the interaction step of the next frame: a chat line, a use of an
    /// object, anything the UI would have asked for.
    pub fn submit_requests(&mut self, requests: Vec<dereth_client_contract::UiRequest>) {
        self.interaction.queue(Vec::new(), requests);
    }

    /// Push the render device's viewport rectangle into [`crate::interaction`].
    ///
    /// The smart-box object-found receiver reads the render viewport and performs unsigned bounds
    /// comparisons against its width and height. `Interaction` has no renderer or UI tree, so the
    /// value crosses the same way [`Shell::examine_panel_open`] does: read here once a
    /// frame, immediately before the step that consumes it.
    ///
    /// It is deliberately [`Shell::game_viewport`] and not the renderer's stored viewport: the two
    /// are the same rectangle by construction — `App::frame` installs the first as the second —
    /// and asking the renderer instead would make the pick agree with a stored field rather than
    /// with the box the player is looking at.
    fn note_game_viewport(&mut self, shell: &S) {
        let v = shell.game_viewport();
        self.interaction.note_game_viewport(v);
        let over = shell.pointer_over_game_view(self.interaction.cursor());
        self.interaction.note_pointer_over_game_view(over);
    }

    /// Capture a screenshot using the original filename rule, verified against retail.
    ///
    /// So: **the directory of the preferences file** — the default preferences file is the same
    /// global the device input takes the `.keymap` directory from, which is why
    /// the screenshot lands beside `acclient.keymap` and not beside the DATs — then `ScreenShot` +
    /// a **five-digit zero-padded** index, the **lowest index whose file does not already exist**,
    /// capped at `99999`: the loop stops at `99999` when every name is taken, and saves over that
    /// one. An empty default preferences file gives an empty directory and therefore a path
    /// relative to the process working directory, which is retail's behaviour and is reproduced
    /// rather than special-cased.
    ///
    /// **The one declared deviation: the extension.** Retail writes a JPEG. This workspace has
    /// **no JPEG encoder** — `zune-jpeg`
    /// in `dereth-render` is a decoder, and adding one would change `Cargo.lock`, which `--locked`
    /// forbids — so the file is a PNG through the existing `Gpu::capture_png` read-back and is
    /// named `.png`. Naming a PNG `.jpg` would be worse than the honest extension; the numbering,
    /// the padding and the directory are retail's, and swapping the encoder later changes one
    /// literal here and one in `capture_png`'s caller.
    fn screenshot_path(&self) -> std::path::PathBuf {
        // Resolve the screenshots directory. `Path::parent` of an
        // empty path is `Some("")`, which is the empty-`%s` case above.
        let dir = self
            .cfg
            .preferences_file
            .parent()
            .map_or_else(std::path::PathBuf::new, std::path::Path::to_path_buf);
        let name = |n: u32| dir.join(format!("ScreenShot{n:05}.png"));
        // Retail checks `_access(name, 0)` — "does it exist". First miss wins.
        (0..100_000_u32)
            .map(name)
            .find(|p| !p.exists())
            .unwrap_or_else(|| name(99_999))
    }

    /// Capture the screenshot through the presentation, the half that requires the device.
    ///
    /// On success the client formats the chosen path into its confirmation line, so the player
    /// sees the saved filename. Channel `0x1A` is
    /// `dereth_client_model::scroll::LOCAL_ERROR_TYPE`, which is the channel every
    /// combat and client-UI scroll-text calls use.
    ///
    /// The path is the client's directory and numbering, not the process temp directory and a
    /// count of screenshots already taken; see [`Self::screenshot_path`] for the rule and the one
    /// deviation that is left.
    ///
    /// **The remaining deviation on this side:** retail captures from inside the action while this
    /// reads back the buffer the device holds when the drain runs, which is the previously
    /// presented frame.
    fn take_action_screenshot(&mut self) {
        let path = self.screenshot_path();
        match self.present.capture_png(&path) {
            Ok(()) => {
                self.events.push(FrameEvent::ScreenshotSaved);
                let text = format!("Screenshot saved to file '{}'", path.display());
                self.objects.world.scroll.add_text_to_scroll(
                    &text,
                    dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                    true,
                    0,
                );
            }
            Err(e) => {
                // The arm still returns TRUE and still prints nothing, so the only place this can
                // be seen is here.
                self.events.push(FrameEvent::ScreenshotFailed);
                tracing::warn!("screenshot refused: {e}");
            }
        }
    }

    /// What the three host-side action arms did:
    /// `(visibility-toggle dispatches, of those that answered TRUE, stop-completely calls,
    /// screenshots saved, screenshots failed)`.
    #[must_use]
    pub const fn action_arm_host_stats(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.events.total(FrameEventKind::EscapeOptionsToggle),
            self.events.amount(FrameEventKind::EscapeOptionsToggle),
            self.events.total(FrameEventKind::EscapeStopPerformed),
            self.events.total(FrameEventKind::ScreenshotSaved),
            self.events.total(FrameEventKind::ScreenshotFailed),
        )
    }

    fn interaction_use_time(&mut self, shell: &mut S) {
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        let viewport = self.present.size();
        // The render device's viewport rectangle, before
        // `draw_use_time_with_chat_focus`'s step 3 reads it back out.
        self.note_game_viewport(shell);
        // The smart-box global-loop listener does not run here. Its broadcast belongs inside the
        // UI manager's per-frame step, i.e. step 7 -- so it runs in the UI step
        // ([`Shell::ui_frame`]), **before** the input drain (the order is the tooltip check, the
        // global-message-3 broadcast, the input manager's per-frame step). Run here, four frame
        // phases late, it could never precede a click in the same frame, and retail's always does.
        // The actions the UI declined this frame — the action dispatch gives the winning map's
        // callback first refusal and the rest reach the other input handlers.
        let actions = self.actions.take();
        // The action handler's `0x1000002B` arm asks whether element
        // `0x100005F7` (`<EXAM>`) is visible before examining anything, and shuts it instead when
        // it is. The client
        // has a manager singleton in reach; `interaction.rs` has no UI at all, so the question is
        // asked here — this frame, after `ui_use_time` has already run — and pushed in. The
        // matching hide is performed below, the statement after `use_time` returns.
        let examine_panel_open = shell.examine_panel_open();
        self.interaction.note_examine_panel_open(examine_panel_open);
        // The spell-cast path's only command-interpreter test, the
        // command interpreter's `controlled_by_server` flag. The free-hands-and-cast path
        // reaches the interpreter through the world objects; `interaction.rs` has none, so
        // the field is pushed in here, in the same way and for the same reason as
        // `note_examine_panel_open` above it. `MovementCommands::lists` is where
        // `lose_control_to_server` sets it and the per-frame step clears it again.
        self.interaction
            .note_controlled_by_server(self.movement.lists.controlled_by_server);
        let (mut unowned, left) = crate::interaction::draw_use_time_with_chat_focus(
            &mut self.interaction,
            &self.store,
            self.present.scene(self.world.as_ref()).as_deref(),
            &mut self.objects,
            self.link.as_mut().map(|l| &mut l.net),
            actions,
            // The send re-reads the spell-component list out of
            // the interface system's player description immediately before it sends, which is the copy the
            // HUD keeps and the same one the footer's cost came from — not the object table's,
            // which `0x0013` reaches before the player's own `0xF745` has even created it.
            self.hud.player_desc_received,
            viewport,
            now,
            &mut |chat| {
                // An option's notice is synchronous relative to the next queued ChatLine.
                // Drain even with no subscriber; no hidden UI may replay this batch later.
                let notices = chat.take_talk_focus_notices();
                shell.deliver_chat_focus_notices(chat, notices);
            },
        );
        // The magic notices this frame's input raised go to the UI's inbox now, in the order they
        // were raised, ahead of the panels that drain them later in this frame. With no UI they
        // stay where they are.
        if shell.has_ui() {
            shell.emit_magic_notices(self.interaction.magic_notices.take());
        }
        // The render-option command owns the parser and ordered command feedback above; the
        // existing `UiRequest::SetPreference` chain below owns live application and persistence.
        // Update the registry at the options-page boundary before the renderer consumes each
        // request, retaining the command order recorded by Interaction.
        for (name, value) in self.interaction.take_render_preferences() {
            let _ = dereth_client_contract::options::store::set_value(name, value.clone());
            unowned.push(dereth_client_contract::UiRequest::SetPreference(
                name, value,
            ));
        }
        // The component-fill operation raises its notice synchronously; the vendor-panel
        // receiver opens buying tab `0x100000BA`. The command/model half above
        // owns the basket but not this live tree. Take the edge every frame even without a shell,
        // and repeat the open guard so a stale edge cannot affect a subsequently opened vendor.
        if let Some(vendor) = self.interaction.take_vendor_buying_tab_request() {
            if self.objects.world.shop.vendor_id == Some(vendor) {
                shell.open_vendor_buying(&mut self.hud);
            }
        }
        // The four layout commands raise a UI notice; the live `UiShell` is
        // the gameplay-screen receiver. Resolve the file beside the configured preferences file
        // with the current character/world identity, then invoke the existing retail-format
        // persistence seam. Retail prints no success, missing-file or I/O-failure line here.
        let layout_commands = self.interaction.take_ui_layout_commands();
        if !layout_commands.is_empty() {
            let prefs = self.cfg.preferences_file.clone();
            let character = self
                .host_state
                .entered_character
                .clone()
                .unwrap_or_default();
            let world = self.host_state.world_name.clone().unwrap_or_default();
            shell.run_ui_layout_commands(&prefs, &character, &world, layout_commands);
        }
        // Complete a geometric target's dialog at this world-draw boundary, also draining
        // notices without UI. No extra frame tick or broadcast pass is introduced.
        shell.service_dialogs(&mut self.interaction, &mut self.objects.world, now);
        // The arm calls the visibility setter with `false` on `<EXAM>` itself.
        // `ExaminationPanel::hide` is that
        // call and is the one place `closed` is counted, so this leg and the close button are the
        // same statement in the same function, as they are in the client.
        if self.interaction.take_examine_panel_close() {
            shell.close_examine_panel();
        }
        // The toolbar's case 2 raises this notice synchronously, but
        // `Interaction` cannot see the toolbar panel. Drain it in the same host-effect slot as the
        // neighbouring examine/visibility calls, against the live selection and object view.
        if let Some(selected) = self.interaction.take_split_stack_notice() {
            let view = self.hud.view(&self.objects);
            shell.split_stack(&view, selected);
        }
        // ---- The three effects whose action arms reach through a
        //      singleton that `interaction.rs` cannot see, drained in the same slot and for the
        //      same reason as `0x1000002B`'s hide above. ---------------------------------------
        //
        // `EscapeKey` with nothing selected requests input action `0x1000001B`.
        // `dereth_ui::UiSystem::dispatch_input_action` **is** retail's visibility-toggle dispatch,
        // element message `0x31` to every listener registered for the action; the `else` below
        // reproduces the successful no-UI return.
        if let Some(a) = self.interaction.take_visibility_toggle() {
            // The visibility-toggle dispatch's return means *"the action had a
            // bucket"*, not *"somebody consumed it"* — an empty bucket still answers TRUE.
            // Kept because it is the only thing the call itself hands back, and because a
            // counter beside the call cannot tell a call that happened from one that did not.
            // `None` is no UI to dispatch into.
            if let Some(answered) = shell.dispatch_input_action(a) {
                self.events
                    .push(FrameEvent::EscapeOptionsToggle { answered });
            }
        }
        // The command-interpreter stop call ends by stopping the player completely. The `else`
        // covers an absent player or physics object.
        if self.interaction.take_stop_completely() {
            if let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) {
                c.stop_completely_from_action();
                self.events.push(FrameEvent::EscapeStopPerformed);
            }
        }
        // The screenshot action initializes a path and, **only on success**, formats
        // `L"Screenshot saved to file '%hs'"` into channel `0x1A`.
        //
        // Two declared deviations follow from this build lacking retail's device-level screenshot
        // operation: retail's device picks the filename, while this build uses the system preferences
        // directory; retail captures inside the action, while this build captures the buffer already
        // on the device when the request is drained — the frame before this one.
        if self.interaction.take_screenshot_request() {
            self.take_action_screenshot();
        }
        self.actions.put_back(left);
        // The device input follows the combat mode (exactly one of the three combat-mode control
        // sets is live at a time) and the target mode, after the step that may have changed them.
        shell.control_notice(crate::shell::ControlNotice::CombatMode(
            self.objects.world.combat.combat_mode.raw(),
        ));
        shell.control_notice(crate::shell::ControlNotice::TargetMode(
            self.interaction.target_mode() != crate::interaction::TargetMode::None,
        ));
        // `UiRequest::SetPreference`'s consumer: the eight `Sound.*` names reach
        // `AudioSystem::set_prefs`. Everything else falls through to the line below, which is the
        // point of that line.
        let unowned = crate::audio::apply_preference_requests(self.audio.as_mut(), unowned);
        // `Display.FullScreen`. It sits before the render names because it is a `Display.*` and not
        // a `Render.*`, and neither of the two below claims it.
        let unowned = self.apply_display_preference_requests(shell, unowned);
        // The client drains UI first, prepares the graphics device, and only then
        // starts the frame.
        // Apply a display request at that same boundary; the event-loop poll remains for
        // Alt+Enter, startup and live window events that do not originate in this drain.
        self.apply_changed_display_presentation(shell);
        let unowned = self.present.apply_render_preference_requests(unowned);
        // The three `Camera.*` names reach the body's camera controller, with
        // a live stiffness update for `Camera.Stiffness`.
        let unowned = crate::camera::apply_preference_requests(
            self.world.as_mut().and_then(|w| w.character.as_mut()),
            unowned,
        );
        // `UiRequest::OpenUrl`, from the Options *Game / Support* page's two support-ticket
        // buttons. Without this filter it would fall past every filter above and only be printed
        // by the line below. See
        // [`apply_open_url_requests`] for the complete consume-and-launch behavior.
        let (unowned, shell) = apply_open_url_requests(unowned);
        for c in shell {
            // Deviation 2 in [`apply_open_url_requests`]: retail's `MessageBoxA` is out
            // of reach of a `forbid(unsafe_code)` crate, so its **own text** goes to the scroll.
            if let ShellCall::ErrorBox { text, .. } = c {
                self.objects.world.scroll.add_text_to_scroll(
                    &text,
                    dereth_client_model::scroll::LOCAL_ERROR_TYPE,
                    true,
                    0,
                );
            }
        }
        for r in unowned {
            if self.unowned_gate.ready() {
                let more = std::mem::take(&mut self.unowned_suppressed);
                if more > 0 {
                    tracing::debug!(
                        "UI request with no owner yet: {r:?} (and {more} more since the last line)"
                    );
                } else {
                    tracing::debug!("UI request with no owner yet: {r:?}");
                }
            } else {
                self.unowned_suppressed += 1;
            }
        }
    }

    /// Tell the server where the player is.
    ///
    /// The client first asks `should_send_position_event`; a non-zero answer calls
    /// `send_position_event` and emits `0xF753`.
    /// The `0xF61C` half is `send_movement_event`, which retail calls from its six
    /// command handlers; [`dereth_client_net::client_session::PositionReporter`] carries the policy and this function
    /// is only the state hand-off.
    ///
    /// **Why it matters.** ACE writes `Player.Location` from
    /// `GameActionMoveToState`, `GameActionAutonomousPosition` and `GameActionJump` and from
    /// nothing else, so a client that sends none of the three leaves the server's copy of the
    /// player at the login position for the whole session — every range check, every approach and
    /// every reachability decision made against a stale point.
    ///
    /// **Where each field comes from, and the one gap.**
    ///
    /// * `position`, `contact`, `contact_plane` — the body's own physics object: `position`,
    ///   `transient_state & (CONTACT_TS | ON_WALKABLE_TS)` (which is `Character::on_ground`, and
    ///   is exactly the pair position reporting gates on), and `contact_plane`.
    /// * `position_valid` — an inbound-valid cell id (already transcribed in
    ///   `dereth_physics::landdefs`, not copied here) and seven finite frame components.
    /// * `raw_motion_state`, `longjump_mode` — the body's raw motion state and
    ///   standing-long-jump flag from the motion interpolation that its
    ///   motion driver owns.
    /// * `timestamps` — `update_times[8]`, `[5]`, `[4]`, `[6]`. `Presence` carries all four, so
    ///   the teleport and force-position timestamps are the values the client holds rather than a
    ///   literal 0. Measured over the 1,827 recorded position bodies, `teleport` is non-zero in
    ///   **1,481** of them (running 0..=4) and `force_position` in **none**, so a literal 0 would
    ///   be wrong four times in five for the first — and the second is right *because nothing
    ///   advances it*, which is a different fact from being hard-wired.
    fn position_use_time(&mut self) {
        // Before the early return: this counts the *call*, not the send.
        self.events.push(FrameEvent::PositionUseTime);
        let now = self.timer.cur_time;
        // `enabled != 0 && player != NULL`. There is
        // no point reporting a position before the server has told us which object we are, and
        // none at all with no link to report it on.
        let player = self.objects.player();
        let has_link = self.link.is_some();
        let Some(motion) = self.player_motion() else {
            self.position.active = false;
            return;
        };
        self.position.active = has_link && player.is_some() && self.movement.is_enabled();
        let before = self.position.stats;
        if let Some(link) = self.link.as_mut() {
            self.position.use_time(now, &motion, &mut link.net.session);
        }
        let after = self.position.stats;
        if after.encode_failures != before.encode_failures {
            tracing::warn!("a position event would not encode");
        }
        // One line the first time each half goes out, so a driven run can say the producer fired
        // rather than leaving it to be inferred. Replay anchors prove encoding but cannot prove
        // that the producer emitted anything.
        if before.position_events == 0 && after.position_events == 1 {
            tracing::debug!(
                "0xF753 Movement_AutonomousPosition, cell {:#010X} \
                 (every {} s, plus a cell or contact-plane change)",
                motion.position.objcell_id,
                dereth_client_net::client_session::TIME_BETWEEN_POSITION_EVENTS
            );
        }
        if before.movement_events == 0 && after.movement_events == 1 {
            tracing::debug!(
                "0xF61C Movement_MoveToState, cell {:#010X}",
                motion.position.objcell_id
            );
        }
    }

    /// The two calls that give the control transfer its callers.
    ///
    /// Losing control, the per-frame retake and taking control from the server live on the
    /// command lists; this is where they are called. A player movement dispatch that is
    /// non-autonomous loses control. Each later use-time tick retakes it only when the interpreter
    /// is enabled, control is still held by the server, the player has no pending motion or move-to
    /// operation, and at least one command list or auto-run requests movement.
    ///
    /// The movement decoder guards its unpack with `(autonomous == 0 || not-the-player)` and sets
    /// its dispatch flag only for the player, so a true result means **non-autonomous and the
    /// player**, which is exactly the arm
    /// `world.rs::apply_player_movement` runs. That is why the stop edge is latched there and
    /// drained here rather than being decided in this file.
    ///
    /// **Where it sits in the frame, and the one deviation.** It is immediately after
    /// [`Self::sync_objects`], because that is where this build unpacks the player's movement
    /// buffer, preserving the retail order within one frame: dispatch, then control transfer. It
    /// is **not** beside
    /// [`Self::position_use_time`], which is the same retail function's *other* half at frame step
    /// 8.9: putting it there would consume the latch a frame after the dispatch that set it, and
    /// this build already runs `apply_player_movement` after step 8.9. The visible cost is that a
    /// `set_auto_run(0)` notice queued here is drained by the **next** frame's
    /// [`Self::apply_input_actions`], one frame (33 ms) later than the stop it describes.
    ///
    /// The three counters exist for the reason [`Self::player_teleport_use_time`] gives: a step
    /// nothing drives is invisible to every anchor in this project, and a mutation that deletes a
    /// call from `App::frame` survives everything until a counter can see it. They are separate —
    /// *ran*, *lost*, *retook* — so that "no dispatch arrived" and "the step never ran" cannot be
    /// confused: an instrument with no third state cannot tell them apart.
    ///
    /// The work itself is the free [`command_interpreter_control_transfer`], for the reason
    /// [`apply_player_teleport`] is free: a counter can prove the *call site* exists and can prove
    /// nothing at all about what the step does, so the step has to be reachable by a test holding
    /// a real body and no device.
    fn command_interpreter_control_transfer(&mut self, shell: &mut S) {
        // Counted before the call: this is the **call**, not the transfer.
        self.events.push(FrameEvent::ControlTransfer);
        let (lost, retook) = command_interpreter_control_transfer_with_finish(
            self.present.scene_mut(self.world.as_mut()).as_deref_mut(),
            &mut self.movement,
            &mut self.char_input,
            |character| crate::jump::finish(&mut self.objects.world.combat, character),
        );
        if lost {
            self.events
                .push(FrameEvent::ServerControlLost(ControlLossSite::Transfer));
        }
        if retook {
            self.events.push(FrameEvent::ServerControlRetaken);
        }
        if lost {
            self.deliver_jump_power_bar_notices(shell);
        }
    }

    /// How many frames reached [`Self::command_interpreter_control_transfer`], how
    /// many accepted calls ran `lose_control_to_server`, and how many took control back. Ordered
    /// player dispatch can lose control more than once in a frame; each accepted call is counted.
    ///
    /// Three numbers rather than one: a run where the server never sent a non-autonomous buffer
    /// and a run where the step was never called read `(n, 0, 0)` and `(0, 0, 0)`, and only the
    /// first is a client that is working.
    #[must_use]
    pub const fn control_transfer_counts(&self) -> (u64, u64, u64) {
        (
            self.events.total(FrameEventKind::ControlTransfer),
            self.events.total(FrameEventKind::ServerControlLost),
            self.events.total(FrameEventKind::ServerControlRetaken),
        )
    }

    /// Move the player's own body to where the server has just put him.
    ///
    /// [`complete_player_teleport`] includes the accepted-edge relocation, teleport_hook and
    /// player-teleported command tail. The free seam lets tests drive the same path over a real
    /// body and inspect outgoing movement without a live connection.
    fn player_teleport_use_time(&mut self, shell: &mut S) {
        // Before every early return: this counts the **call**, not the teleport, which is the only
        // thing that can tell a frame that skipped this step from a frame with nothing to do. The
        // pattern is the same as `position_use_times`: a step nothing drives is invisible to every
        // anchor in this project, and a mutation that deletes this call from `App::frame` survives
        // everything but this counter.
        //
        // Accepted local movement and its control/teleport callbacks complete before another
        // queued message is admitted, so this also runs at each delivery boundary inside
        // `deliver_session_events` — the analogue of the smart box's event dispatch handling
        // a received position once per admitted blob. The per-frame call is
        // `step_incoming_world_objects`'s, and it is unconditional; see the note there.
        self.events.push(FrameEvent::PlayerTeleportUseTime);
        if self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .is_none()
        {
            // No local body can execute the tail. Preserve the latest viewer/initial-placement
            // snapshots, but never retain a journal that a later body would replay out of time.
            let discarded = self.objects.discard_bodyless_player_dispatches();
            if discarded > 0 {
                self.events.push(FrameEvent::PlayerTeleportBeforeABody {
                    count: discarded,
                    reason: NoBodyReason::JournalDiscarded,
                });
            }
            return;
        }
        if self.objects.has_player_motion_dispatches() {
            // The packet stage has already accepted this batch's creates. Materialize their
            // existing shared scene prefix before calling the movement manager: a later setup
            // rebuild would otherwise erase the command, and a newly created target would be
            // mistaken for the missing-object MoveToPosition fallback. This is the pre-existing
            // create-before-movement batch contract, not a journal of all WorldObjects object edges.
            let store = std::sync::Arc::clone(&self.store);
            if let Err(e) =
                self.present
                    .prepare_object_dispatch(&store, &mut self.objects, self.world.as_mut())
            {
                tracing::warn!("player dispatch preparation failed: {e}");
                return;
            }
            if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
                if let Some(c) = world.world_mut().character.as_mut() {
                    // As in ordinary object sync, claim the local id before adding remote
                    // bodies. No physics clock advances here. A target's DAT dimensions must
                    // exist before MoveToObject captures them, not after its first request.
                    c.adopt_server_id(self.objects.player());
                    self.objects.sync_physics_at(
                        &store,
                        &mut c.world,
                        dereth_primitives::LocalTime(self.timer.cur_time),
                    );
                }
                world.world_mut().refresh_object_geometry(&self.objects);
            }
        }
        // Movement-event and received-position handlers
        // execute these calls in message order. Independent pending snapshots would invert a
        // `0xF74C` followed by a teleport: the approach would be installed after teleport_hook
        // canceled it.
        for event in self.objects.take_player_motion_dispatches() {
            use crate::objects::PlayerMotionDispatch;
            let Some(mut world) = self.present.scene_mut(self.world.as_mut()) else {
                // The destination remains on Presence for initial scene placement. No body's
                // motion/teleport callback exists yet, so neither can be replayed on a new body.
                if matches!(event, PlayerMotionDispatch::Teleport { .. }) {
                    self.events.push(FrameEvent::PlayerTeleportBeforeABody {
                        count: 1,
                        reason: NoBodyReason::NoScene,
                    });
                }
                continue;
            };
            match event {
                PlayerMotionDispatch::Movement(buf) => {
                    if world.character().is_some() {
                        world
                            .world_mut()
                            .dispatch_player_movement(&buf, &self.objects);
                        if world.world_mut().take_player_movement_applied() {
                            // set_object_movement's accepted-player return is acted on immediately,
                            // not after a later player-teleported reapplication in this batch.
                            self.movement.lose_control_to_server_with_finish(
                                &mut self.char_input,
                                || {
                                    crate::jump::finish(
                                        &mut self.objects.world.combat,
                                        world.character(),
                                    )
                                },
                            );
                            self.events.push(FrameEvent::ServerControlLost(
                                ControlLossSite::MovementDispatch,
                            ));
                        }
                    }
                }
                PlayerMotionDispatch::Teleport {
                    position: pos,
                    timestamps,
                } => {
                    let Some(character) = world.world_mut().character.as_mut() else {
                        self.events.push(FrameEvent::PlayerTeleportBeforeABody {
                            count: 1,
                            reason: NoBodyReason::NoCharacter,
                        });
                        continue;
                    };
                    let previous_cell = character.position().cell;
                    let had_previous_cell = character
                        .world
                        .get(character.handle)
                        .and_then(|body| body.cell)
                        .is_some();
                    complete_player_teleport_at(
                        pos,
                        character,
                        &mut self.movement,
                        &mut self.char_input,
                        |character| {
                            if let Some(link) = self.link.as_mut() {
                                self.position.send_movement_event(
                                    self.timer.cur_time,
                                    &body_motion(character, timestamps),
                                    &mut link.net.session,
                                );
                            }
                        },
                    );
                    self.events.push(FrameEvent::PlayerTeleportApplied);
                    if !had_previous_cell || pos.cell != previous_cell {
                        // Cell-release processing sees the body after the simple position set has
                        // committed the destination. Publish that live membership before the
                        // queued landscape release asks ObjectStream which cell the player left;
                        // the ordinary post-physics publication is later than this edge.
                        self.objects.publish_physics_cells(&character.world);
                        world.release_landscape_for_teleport(pos.cell.landblock());
                    }
                    world.world_mut().follow_character_now();
                    tracing::info!(
                        "the server teleported the player to cell {:#010X} at ({:.2}, {:.2}, {:.2})",
                        pos.cell.0, pos.frame.origin.x, pos.frame.origin.y, pos.frame.origin.z,
                    );
                }
            }
            // The view holds the presentation and the world state; hand both back first.
            drop(world);
            self.deliver_jump_power_bar_notices(shell);
        }
        for text in self.movement.take_notices() {
            self.objects
                .world
                .scroll
                .on_display_string_info(dereth_client_model::scroll::LOCAL_ERROR_TYPE, text);
        }
    }

    /// How many frames have reached [`Self::player_teleport_use_time`].
    ///
    /// One per drawn frame, plus one per admitted session event on a frame that had traffic — the
    /// per-blob smart-box event-dispatch boundary. A test that asserts this equals
    /// [`Self::frames_drawn`] therefore drives an `App` with no link, and is asserting the
    /// **call site**, which no counter inside the object stream can do — a mutation deleting the
    /// call from `App::frame` is otherwise unobservable.
    #[must_use]
    pub const fn player_teleport_use_times(&self) -> u64 {
        self.events.total(FrameEventKind::PlayerTeleportUseTime)
    }

    /// How many server teleports actually moved this client's body.
    ///
    /// Exposed for the same reason [`Self::position_reporter_stats`] is: replay anchors prove
    /// *encoding*, while a producer or consumer that
    /// never fires is invisible to them.
    #[must_use]
    pub const fn player_teleports_applied(&self) -> u64 {
        self.events.total(FrameEventKind::PlayerTeleportApplied)
    }

    /// Teleports that arrived before there was a body to move. See
    /// [`Self::player_teleport_use_time`] for why they are dropped rather than queued.
    #[must_use]
    pub const fn player_teleports_before_a_body(&self) -> u64 {
        self.events
            .amount(FrameEventKind::PlayerTeleportBeforeABody)
    }

    /// Press starts `CommenceJump`; release calls `DoJump(true)`.
    /// Commands preceding this event are completed without advancing the world clock.
    fn apply_jump_action(
        &mut self,
        shell: &mut S,
        command: crate::actions::movement::MovementAction,
    ) -> bool {
        use crate::actions::movement::MovementAction as M;
        if !matches!(command, M::CommenceJump | M::DoJump) {
            return false;
        }
        // The command-interpreter action handler checks that it is active before the jump arms
        // too. It consumes the action while disabled, but neither starts nor finishes a jump.
        if !self.movement.is_enabled() {
            return true;
        }
        let retake = self.movement.take_control_retake_pending();
        let Some(character) = self.world.as_mut().and_then(|w| w.character.as_mut()) else {
            return true;
        };
        if retake {
            character.take_control_from_server();
        }
        crate::jump::refresh_qualities(
            character,
            self.hud.player_desc(&self.objects.world),
            self.hud.skill_table.as_ref(),
            self.hud.quality_filter.as_ref(),
        );
        character.flush_command_input(self.char_input);
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        match command {
            M::CommenceJump => {
                let started =
                    crate::jump::commence(character, &mut self.objects.world, now, || {
                        if let Some(link) = self.link.as_mut() {
                            let _ = link
                                .net
                                .session
                                .send_action(&dereth_protocol::combat::CombatCancelAttack);
                        }
                    });
                if started {
                    // The commence-jump step's final call: the charge flag is already set, no
                    // physics tick has occurred, and this is independent of motion-change polling.
                    if let Some(motion) = self.player_motion() {
                        if let Some(link) = self.link.as_mut() {
                            self.position.send_movement_event(
                                now.0,
                                &motion,
                                &mut link.net.session,
                            );
                        }
                    }
                }
            }
            M::DoJump => {
                if let Some(result) = crate::jump::release(character, &mut self.objects.world, now)
                {
                    // One event for both halves of `jump_counts`: the refusals are the
                    // releases whose `status` is non-zero, which is the test the `else` was.
                    self.events.push(FrameEvent::JumpRequested {
                        status: result.status,
                    });
                    if result.status == 0 {
                        if let Some(motion) = self.player_motion() {
                            let pack =
                                dereth_client_net::client_session::PositionReporter::jump_pack(
                                    result.extent,
                                    result.local_velocity,
                                    &motion,
                                );
                            self.last_jump_request =
                                Some(dereth_protocol::movement::MovementJump(pack));
                            if let Some(link) = self.link.as_mut() {
                                self.position.send_jump_pack(pack, &mut link.net.session);
                            }
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
        self.deliver_jump_power_bar_notices(shell);
        true
    }

    /// The jump's notice sends are synchronous with their input/control-loss owner. Finish the
    /// existing subscriber before the next action, not next frame. No UI tick or mode switch is
    /// performed here, and an absent/outgoing subscriber leaves no retained notice history.
    fn deliver_jump_power_bar_notices(&mut self, shell: &mut S) {
        let notices = self.objects.world.combat.take_power_bar_notices();
        shell.deliver_power_bar_notices(&mut self.hud, notices);
    }

    /// Pending releases attempted, and actual nonzero motion-interpreter results.
    #[must_use]
    pub const fn jump_counts(&self) -> (u64, u64) {
        (
            self.events.total(FrameEventKind::JumpRequested),
            self.events.amount(FrameEventKind::JumpRequested),
        )
    }

    /// Input-dispatch passes, not autonomous `DoJump` calls per frame, despite the name.
    #[must_use]
    pub const fn jump_use_times(&self) -> u64 {
        self.events.total(FrameEventKind::JumpUseTime)
    }

    /// The last actual successful jump request; offline observers see the same pack transport
    /// receives, not a substitute acceptance answer or a later post-physics reconstruction.
    #[must_use]
    pub fn last_jump_request(&self) -> Option<&dereth_protocol::movement::MovementJump> {
        self.last_jump_request.as_ref()
    }

    /// Everything [`dereth_client_net::client_session::PositionReporter`] reads off the body, in one place.
    ///
    /// `None` when there is no body yet, which is every frame before `load_pending_scene` has
    /// stood one up.
    fn player_motion(&self) -> Option<dereth_client_net::client_session::PlayerMotion> {
        let character = self.world.as_ref()?.character.as_ref()?;
        let presence = self
            .objects
            .player()
            .and_then(|id| self.objects.presence(id));
        // All four sequences are the client's own, and the mapping is a public
        // free function so that the test which owns the echo can call the same code this does.
        Some(body_motion(character, player_timestamps(presence)))
    }

    /// How many `0xF753` and `0xF61C` blobs this client has produced.
    ///
    /// Exposed so a test can assert **emission**, which no replay anchor can see.
    #[must_use]
    pub const fn position_reporter_stats(
        &self,
    ) -> dereth_client_net::client_session::PositionReporterStats {
        self.position.stats
    }

    /// How many frames have reached [`Self::position_use_time`].
    ///
    /// One per drawn frame. A test that asserts this equals [`Self::frames_drawn`] is asserting
    /// the **call site**, which no counter inside the reporter can do.
    #[must_use]
    pub const fn position_use_times(&self) -> u64 {
        self.events.total(FrameEventKind::PositionUseTime)
    }

    /// Request character logoff through `dereth_client_net::client_session::Session::log_off`.
    ///
    /// **The client's function takes a `bool` and this one does not, which is a declared
    /// simplification and not a transcription.** The direct character-logoff arm used during
    /// epilogue-screen construction saves character state and executes logoff **directly**: no `0xF653`, no three
    /// second deadline, no portal space, because the process is about to exit.
    /// The ordinary character-logoff arm — also used by the 1,200-second
    /// idle kick — saves character state, then (unless an item request is outstanding, in which case
    /// logoff is deferred to the next opportunity) requests departure,
    /// which is the departure. This build runs the `false` arm for both, and the difference is
    /// invisible on the quit path only because the epilogue screen requests device shutdown in the same
    /// frame. The `ask` flag on `UiRequest::EndCharacterSession` already carries the distinction;
    /// nothing reads it yet.
    pub fn log_off_character(&mut self) {
        // The logoff request is sent and the log-off-requested flag is armed; that flag fades
        // the world out three seconds later,
        // plus twenty when the local player's `is_player_killer` predicate answers true — so
        // the fade is armed even when there is no link to tell.
        let is_player_killer = self
            .objects
            .player()
            .and_then(|id| self.objects.world.weenie(id))
            .is_some_and(dereth_client_model::Weenie::is_player_killer);
        // **Saving the player module, which is the *first* thing both
        // character-logoff arms do.**
        //
        // ```text
        // direct epilogue arm  : save character state ; execute logoff
        // ordinary logoff arm : save character state ; request logoff
        //
        // save(force): if the module is dirty or forced, send the whole player module
        //              then clear the dirty flag
        // ```
        //
        // Without it the `0x01A1` that carries the **whole** player module never goes out on the
        // way out. That is not a cosmetic gap: individual changes send a single-option
        // `0x0005` only for the twenty-one auto-save options, while
        // changes to every other gameplay option, including the chat
        // windows' text filters (`0x1000007F`) and all of their placement — sends **nothing** and
        // only stamps the module dirty. So the other 31 character options, every chat filter and
        // every window position would reach the shard only by two accidents: the 480-second
        // player-module flush, and reopening an options page (whose show edge
        // raises `UiRequest::SavePlayerOptions`). Change one and log out inside eight minutes and
        // the next login would come back to the old value.
        //
        // `force` is **false**, which is the arm both callers use: a session that deferred nothing
        // still sends nothing, because the save gate is `dirty || force`. The order
        // is the client's too — the save precedes departure request `0xF653`, so the
        // module is on the wire before the departure is.
        if self.link.is_some() {
            let mut req = dereth_client_model::RecordingRequests::default();
            if self
                .objects
                .world
                .player_system
                .save_to_server(&mut req, false)
            {
                self.events.push(FrameEvent::PlayerModuleSavedAtLogout);
            }
            for r in req.0 {
                if let Some(link) = self.link.as_mut() {
                    let _ = crate::interaction::send_request(&mut link.net.session, &r);
                }
            }
        }

        self.teleport
            .request_log_off(self.timer.cur_time, is_player_killer);
        if let Some(link) = self.link.as_mut() {
            tracing::info!("the epilogue UI requested character logoff");
            // The departure request sends before its local teardown tail.
            link.net.session.log_off();
        }

        // Local teardown disables movement. The returned guard is
        // `autonomy_level && player && !controlled_by_server`, split at the body boundary here.
        let apply_and_send = self.movement.disable(&mut self.char_input);
        if apply_and_send {
            if let Some(character) = self.world.as_mut().and_then(|w| w.character.as_mut()) {
                character.flush_command_input(self.char_input);
            }
            if let Some(motion) = self.player_motion() {
                if let Some(link) = self.link.as_mut() {
                    let _ = self.position.send_movement_event(
                        self.timer.cur_time,
                        &motion,
                        &mut link.net.session,
                    );
                }
            }
        }
    }

    /// The player system's three character calls, made on the session instead of on a singleton.
    ///
    /// Log-on is the two-step `0xF7C8` / `0xF657` exchange (`Session::enter_world` is that state
    /// machine), delete is `0xF655` addressed by **slot**, and restore is `0xF7D9` on net queue 2.
    pub fn run_character_actions(
        &mut self,
        actions: Vec<dereth_client_contract::pregame::CharacterAction>,
    ) {
        use dereth_client_contract::pregame::CharacterAction;
        if actions.is_empty() {
            return;
        }
        let Some(link) = self.link.as_mut() else {
            for a in actions {
                tracing::warn!("{a:?} with no server to ask");
            }
            return;
        };
        let account = link.net.characters().account.clone();
        for a in actions {
            match a {
                CharacterAction::LogOn(gid) => {
                    let name = link
                        .net
                        .characters()
                        .characters
                        .iter()
                        .find(|c| c.gid == gid)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    tracing::info!("entering the world as {name:?} ({:#010X})", gid.0);
                    // Building the screen-layout path asks
                    // the player object's singular name for half of the automatic layout file's
                    // name. This build has
                    // no `WorldObjects` singleton, so the name is recorded on the edge that knows it.
                    self.host_state.entered_character = Some(name.clone());
                    link.net.enter_world(gid, &account);
                    self.script = EnterWorldScript::Entering;
                }
                CharacterAction::Delete(gid) => {
                    tracing::info!("deleting {:#010X}", gid.0);
                    link.net.session.delete_character(gid);
                }
                CharacterAction::Restore(gid) => {
                    // Restore-character requests use net queue **2**, with two
                    // empty packed strings after the id.
                    tracing::info!("restoring {:#010X}", gid.0);
                    link.net.session.restore_character(gid);
                }
            }
        }
    }

    /// Send the character-generation result and the log-on that follows a successful creation.
    ///
    /// The conversion from the UI's `CharGenResultData` to the protocol's
    /// `dereth_protocol::login::CharGenResult` is field for field and belongs to neither side, so
    /// it lives here — the same seam, in the same direction, as `ui::character_set_from_login`. The
    /// **checksum** is computed rather than carried: sums
    /// fields 2-10, 12, 14, 16 and 24-31 as it writes them, and `dereth_protocol` already knows how.
    pub fn run_chargen_actions(
        &mut self,
        actions: Vec<dereth_client_contract::pregame::CharGenAction>,
    ) {
        use dereth_client_contract::pregame::CharGenAction;
        if actions.is_empty() {
            return;
        }
        let Some(link) = self.link.as_mut() else {
            for a in actions {
                tracing::warn!("{a:?} with no server to ask");
            }
            return;
        };
        for a in actions {
            match a {
                CharGenAction::SendCharGenResult(r) => {
                    let mut msg = chargen_result_to_wire(&r);
                    msg.checksum_value = msg.checksum();
                    tracing::info!(
                        "0xF656 creating {:?} -- heritage {}, gender {}, town {}, \
                         {} skill entries, slot {}",
                        msg.name,
                        msg.heritage_group,
                        msg.gender,
                        msg.start_area,
                        msg.skill_advancement_classes.len(),
                        msg.slot
                    );
                    link.net.session.create_character(msg);
                }
                CharGenAction::LogOn(gid) => {
                    let account = link.net.characters().account.clone();
                    tracing::info!("entering the world as the new {:#010X}", gid.0);
                    // See the other two sites. The name is not in the character
                    // set yet on this path (the server sends the new one back with `0xF658`), so
                    // it is looked up and left `None` when it is not there rather than guessed:
                    // an automatic layout file for a character with no name would be `UI--<world>`
                    // and would collide with every other unnamed one.
                    self.host_state.entered_character = link
                        .net
                        .characters()
                        .characters
                        .iter()
                        .find(|c| c.gid == gid)
                        .map(|c| c.name.clone());
                    link.net.enter_world(gid, &account);
                    self.script = EnterWorldScript::Entering;
                }
            }
        }
    }

    /// Run the main loop: run the per-frame step until it returns false.
    ///
    /// Returns the number of frames drawn.
    pub fn run(&mut self, shell: &mut S) -> u64 {
        self.state = AppState::Running;
        while self.frame(shell) {}
        self.state = AppState::ShuttingDown;
        self.frames_drawn()
    }

    /// One iteration of the main loop, in the documented order. Returns false when
    /// the loop should end.
    #[allow(clippy::too_many_lines)]
    pub fn frame(&mut self, shell: &mut S) -> bool {
        // The frame's spans: the frame and, as each step starts, that step. See
        // `crate::frame::FrameSpans`.
        let mut spans = crate::frame::FrameSpans::begin(self.frames_drawn() + 1);
        self.events.drain_frame();
        self.teleport_ticked = false;

        // Explicit component adapter: ObjectStream::apply_event may have already accepted
        // local calls before App began. Complete that old journal before simulation or a
        // snapshot sync can consume it. Real network entries remain raw until their owning
        // WorldObjects phase and therefore cannot enter this path.
        if self.objects.has_player_motion_dispatches() {
            self.player_teleport_use_time(shell);
        }

        // Client use time prepends the UI queue-manager drain before chaining the base step.
        spans.step(FrameStep::UiQueueStep);
        self.events.push(FrameEvent::Step(FrameStep::UiQueueStep));
        self.ui_queue_use_time(shell);

        spans.step(FrameStep::ClockSample);
        self.events.push(FrameEvent::Step(FrameStep::ClockSample));
        self.timer.update_time(&*self.clock);

        spans.step(FrameStep::ProcessWindowEvents);
        self.events
            .push(FrameEvent::Step(FrameStep::ProcessWindowEvents));
        if self.do_event_loop(shell) {
            // "If true, the per-frame step returns false immediately -- the network is not pumped
            // and no frame is drawn."
            self.state = AppState::ShuttingDown;
            return false;
        }

        // Receive first, run queue-4 callbacks next, then process outbound packets.
        // Reception retains raw UI/WorldObjects entries; it does not admit them against old objects.
        spans.step(FrameStep::NetworkStep);
        self.events.push(FrameEvent::Step(FrameStep::NetworkStep));
        let net_time = dereth_primitives::LocalTime(self.timer.local_time);
        // Link-status timestamps use server-adjusted `cur_time`, while connection and snapshot
        // cadences use local elapsed time. The network layer therefore publishes edges and this frame loop
        // performs the stamp: first on the login-connected edge, then on each two-second
        // connection heartbeat.
        let heartbeat_time = self.timer.cur_time;
        let time_sync = if let Some(link) = self.link.as_mut() {
            // The time-sync speed-check tail reads the
            // global, not `local_time`, and reads it before receive handling below. Publishing
            // the pre-sync value here matches retail's order: it occurs at the top of the network
            // step, before this frame's datagrams are walked.
            link.set_cur_time(heartbeat_time);
            link.receive_use_time(net_time);
            if link.net.take_connected() {
                crate::net::link_status_holder::on_connected(heartbeat_time);
            }
            // A link heartbeat updates the holder's *second* store,
            // the average packet loss. The same callback also refreshes
            // the last-heard-from-server time, so both stores happen on this one edge.
            if let Some(loss) = link.net.take_link_heartbeat() {
                crate::net::link_status_holder::on_heartbeat(heartbeat_time);
                crate::net::link_status_holder::on_packet_loss(loss);
            }
            // Time synchronization is accepted during packet handling, exactly here: after this
            // frame's datagrams have been parsed and before subsequent frame consumers read the
            // clock. Draining it
            // a step later would put the first `cur_time` reader of the frame on the stale clock.
            link.net.session.transport.take_time_sync()
        } else {
            None
        };
        // **Retail has only one such clock-setting call.** The time-sync handler reads
        // the `double` payload at offset `0x18` in the time-sync header, copies it to a
        // local, and passes that value by const reference
        // to the clock's time setter.
        //
        // This is what makes the world clock the shard's rather than this process's: with no sync
        // the external time offset stays 0 and `cur_time` is just local elapsed seconds, which is a
        // process-local epoch -- the wrong date on the map panel and the wrong day/night cycle.
        if let Some(server_time) = time_sync {
            self.timer.set_time(&*self.clock, server_time);
        }
        // The first connection is made before any screen exists, so a login that ends before the
        // link was ever up is not a screen: it is one modal error box, and when the player closes
        // it the process exits. Nothing else runs this frame and no frame is drawn.
        if self.connect_failure_use_time() {
            self.pump.done();
            self.state = AppState::ShuttingDown;
            return false;
        }
        self.deliver_session_events(shell, dereth_primitives::LocalTime(self.timer.cur_time));
        spans.step(FrameStep::LoginEvents);
        self.events.push(FrameEvent::Step(FrameStep::LoginEvents));
        while self
            .link
            .as_mut()
            .is_some_and(|link| link.net.session.process_next_logon())
        {
            self.deliver_session_events(shell, dereth_primitives::LocalTime(self.timer.cur_time));
        }
        // Per-event delivery carries lines from scroll-text calls to the
        // HUD's spew box and chat interface. Every frame also refreshes the three once-a-frame
        // `display_time_stamps`/clock inputs at the head of `Hud::apply_events` and drains pending
        // notices even without new events; that is a frame phase in its own right.
        // The per-event `apply_hud_events` call lives in `deliver_session_events`; without this
        // empty application, a frame with no link or new
        // packet would not drain a notice that a UI request raised in step 7 of the previous frame.
        let _ = self.apply_hud_events(shell, &[]);
        // The login-event queue's per-frame half. Everything before
        // its event loop -- the link-status and rejected-datagram lines, the net-error report
        // and its no-UI exit, and the `--linger` deadline whose own comment says it is
        // "checked before the batch so a run with no traffic still ends" -- is once-a-frame
        // work, not per-event work. Running the whole call per session event would silently make
        // all four conditional on this frame having had traffic.
        self.process_logon_event_queue(shell, Vec::new());
        spans.step(FrameStep::PacketStep);
        self.events.push(FrameEvent::Step(FrameStep::PacketStep));
        if let Some(link) = self.link.as_mut() {
            link.packet_controller_use_time(net_time);
        }
        spans.step(FrameStep::AssetCacheStep);
        self.events
            .push(FrameEvent::Step(FrameStep::AssetCacheStep)); // Shared asset-cache loads.
        while self
            .link
            .as_mut()
            .is_some_and(|link| link.net.session.process_next_cache())
        {
            self.deliver_session_events(shell, dereth_primitives::LocalTime(self.timer.cur_time));
        }
        // The other half of the shared asset cache: a get whose object is
        // neither in memory nor on disk sends the object-missing request `0xF7E3`.
        // This runs in the same frame phase as the queue-5 drain above, because in retail
        // both operations belong to the same cache.
        self.request_missing_cell_records();
        // Step 7 is **where the phase state machine
        // advances**, after global message 3 and with the queued mode switch running last.
        // The input-manager update is step 6 inside it.
        spans.step(FrameStep::UiStep);
        self.events.push(FrameEvent::Step(FrameStep::UiStep));
        self.ui_use_time(shell, dereth_primitives::LocalTime(self.timer.cur_time));
        // Input action dispatch's
        // input-handler leg for the movement interpreter and camera controller: the actions the UI declined
        // become motion commands and camera rotations *here*, downstream of `walk_input_maps` and
        // therefore of the typing barrier. It is before `WorldScene::update` reads `char_input`,
        // and after step 7 because the UI gets first refusal.
        self.apply_input_actions(shell);
        spans.step(FrameStep::WorldViewStep);
        self.events.push(FrameEvent::Step(FrameStep::WorldViewStep));
        // The landscape cannot be built until the server says where the player is, and the objects
        // cannot be drawn until it exists. Both happen here, outside the frame bracket.
        self.load_pending_scene();
        // Object maintenance runs only while the cell manager is not blocking,
        // before physics. No connection/no player body is not itself a cell-load blocker.
        if self.pending_scene.is_none() {
            let (geometry, radius) = self.current_selection_geometry();
            let session = self.link.as_mut().map(|link| &mut link.net.session);
            let inter = &mut self.interaction;
            let hud = &mut self.hud;
            let now = dereth_primitives::LocalTime(self.timer.cur_time);
            inter.last_use_time = now;
            self.objects.use_time_with_dispatch(
                dereth_primitives::ServerTime(self.timer.cur_time),
                session,
                &mut |world, notice| {
                    inter.dispatch_object_notice(world, notice.clone(), &geometry, radius, now);
                    shell.object_panel_notice(hud, inter, world, &notice);
                },
            );
            self.deliver_object_notices();
        }
        self.sync_objects();
        // The teleport's second half. Run the smart-box teleport arm at its own frame step and
        // **after** the UI animation ran inside step 7; see
        // `crate::teleport::Teleport::step_world_view` for why that order is load-bearing. It
        // is after `load_pending_scene` because "the cell manager is no longer blocking" is what
        // this build reads as "the body is standing in a loaded scene".
        let blocking = self.pending_scene.is_some()
            || self
                .world
                .as_ref()
                .and_then(|w| w.character.as_ref())
                .is_none()
            || self
                .present
                .scene(self.world.as_ref())
                .is_some_and(|s| s.loading_near_viewer());
        self.teleport.step_world_view(blocking);
        // The sound world step: sets the listener, fires the animation sound hooks and runs
        // the ambient sounds.
        if self.pending_scene.is_none() {
            crate::audio::world_use_time(
                self.audio.as_mut(),
                self.present.scene_mut(self.world.as_mut()).as_deref_mut(),
                &mut self.objects,
                self.anim_assets.as_ref(),
                &self.store,
                dereth_primitives::LocalTime(self.timer.cur_time),
            );
        }
        // The world simulation's slot. With no world the free camera is here, and this is
        // where a viewpoint moves -- once per frame, off the one sample.
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a frame delta in seconds, narrowed for the camera's own f32 arithmetic. It is
        // not engine arithmetic: the free camera is a debug flycam, not the camera controller.
        let dt = (self.timer.cur_time - self.last_time).clamp(0.0, 0.25) as f32;
        self.last_time = self.timer.cur_time;
        let input = self.input;
        let char_input = self.char_input;
        // The rising-edge controls are consumed here, once, so a key press cannot be issued twice.
        self.char_input.jump = false;
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        // Motion interpolation asks the weenie for the run rate *live*, on every
        // `apply_run_to_command`, `get_max_speed`, `get_adjusted_max_speed` and
        // `get_state_velocity`; here the answer is a copied fact in `MotionEnv`, so it is refreshed
        // once per frame, before `world.update` -> `Character::apply_input` issues the motion whose
        // speed it scales. Without this every body runs at the `my_run_rate` default of 1.0 and
        // the Run skill does not reach the ground at all.
        if let Some(character) = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .filter(|_| self.pending_scene.is_none())
        {
            crate::character::refresh_run_rate(
                character,
                self.hud.player_desc(&self.objects.world),
                self.hud.skill_table.as_ref(),
                self.hud.quality_filter.as_ref(),
            );
        }
        if let Some(mut world) = self
            .present
            .scene_mut(self.world.as_mut())
            .filter(|_| self.pending_scene.is_none())
        {
            world.update(input, char_input, now, dt);
            if let Some(c) = world.character() {
                self.objects.publish_physics_cells(&c.world);
            }
            // The real camera, whose swept
            // sphere replaces the debug chase camera `world.update` just placed. It runs here
            // rather than inside `world.update` because the sweep starts from the pivot the body
            // has just moved to and must be expressed in the block the re-centre has just chosen.
            //
            // **What that costs is measured, and the line stays where it is.** The client
            // sweeps and re-centres inside one (`update_viewer`
            // then the normal world render); here the re-centre inside `world.update` reads the
            // viewpoint this line left **last** frame. Measured over five walking stations, that
            // lag is one frame of camera travel: 0.099 m walking,
            // 0.152 m running, 0.192 m off-axis, and a 0.971 m worst case that is a camera being
            // released from a wall rather than one following a body -- 0.41% to 4.05% of a 24 m
            // cell. Across 5,400 frames, 33 take a different cell decision and 3 a different block
            // decision, and no single frame ever moves the cell index by more than 1: each is a
            // boundary reached one frame late, never one skipped.
            //
            // The block the sweep is expressed in is what binds, and it binds on the half of this
            // call that is not the sweep. `CameraControl::update_viewer` is block-independent --
            // it works in `Position` and physics space -- but this function's tail is
            // `scene.camera = FreeCamera::from_frame(&c.camera_render_frame()?)`, and
            // `Character::render_frame_of` subtracts the **current** viewer block. Above the
            // re-centre that leaves the render camera in the block the frame is leaving: a 192 m
            // error once per crossing, bought with a 0.97 m worst-case lag. A faithful move is a
            // split -- sweep above, projection below -- and the measurement does not pay for it.
            // See `WorldScene::viewpoint` for the table and the rest of the reasoning.
            crate::camera::update_viewer(&mut *world, input, now, f64::from(dt));
        }
        // Camera rotation turns the *body*
        // in two of its three arms — in first person, and whenever
        // `Input.UseMouseTurning` is on — and the calls it makes to do it are the command
        // interpreter's, not the camera's. It runs here, immediately after `update_viewer`,
        // because `update_viewer` is where held-key camera repeat reaches
        // `Rotate` at all; see [`Self::apply_camera_turn`].
        self.apply_camera_turn(now);
        // Target tracking: the camera update routine at the three edges retail raises it on — see
        // [`Self::last_target_tracking`] for why an edge detector is the faithful shape here and a
        // per-frame re-run is not.
        //
        // It runs **after** `update_viewer` rather than before for the same reason
        // `update_viewer` is where it is: it ends in `set_target_for_offset`, which
        // only writes, and `update_camera` reads that mask at the top
        // of the *next* frame's sweep. Putting it before would have it decide on a selection the
        // frame's own physics had not yet moved the body away from.
        self.update_target_tracking();
        // Apply the three player-option environment arms (persistent day,
        // weather, and fog) plus their initialization copies. This sits beside
        // `update_target_tracking` because it
        // is the fourth arm of the same option-change switch and the same kind of edge; after it
        // for no reason except that retail's player-module initialization runs them in that order.
        self.apply_player_option_effects();
        // The callback queue is outside the cell-blocked simulation branch. Released
        // arrivals and control-loss hooks finish before the next message and command tail.
        self.step_incoming_world_objects(shell);
        self.command_interpreter_control_transfer(shell);
        self.position_use_time();
        // The landblock window was re-centred inside `world.update`;
        // this is the fetch it asked for, and it is here rather than a line later because
        // `Gpu::upload_texture` runs a command list of its own and the frame begin is next.
        self.stream_world();

        // Smart-box drawing builds the selection ray from
        // *this* frame's camera and raises the smart-box object-found notice before the UI overlay
        // draws, so the pick runs after the camera update and before the frame bracket.
        self.interaction_use_time(shell);
        // This frame's requests have gone out; a headless client with no server has its stand-in
        // answer them, and the answers are read from the next frame on.
        if let Some(stub) = self.server_stub.as_mut() {
            stub.answer(&self.interaction.last_sent, &mut self.objects.world);
        }

        // Apply the object-found notice's tooltip flag
        // and the UI manager's `clear_tooltip` on the wrapper element, one statement after the
        // world draw that raised the notice, for the same reason `update_cursor_state` below is:
        // this is where `click_object_id` becomes known, and both are calls the notice makes
        // through a singleton `interaction.rs` cannot see. Nothing observes the flag until the
        // next UI frame, whose `check_tooltip` runs *before*
        // it broadcasts global message 3 to the global-loop listeners — so the client's own
        // ordering is that the pick answer of frame N is read by the tooltip check of frame N+1.
        shell.world_tooltip(&mut self.interaction);

        shell.draw_world_target(self);

        // Cursor-state update chooses one of the dat's 41 cursors
        // and pushes it as the manager's **default**. Retail calls it from eight notice
        // handlers rather than from the frame loop — busy-count up and down, target-mode change,
        // use, examine object, examine spell, use-done and the object-found notice — and the
        // union of those is "whenever any of its five inputs moved". Calling it once a frame is
        // the same answer and cannot miss an edge, because the current and last cursor ids
        // between them make a redundant call free.
        //
        // It sits **here**, immediately after `interaction_use_time`, because that is where
        // smart-box drawing completes the pick that writes `click_object_id` — the `h` of
        // the state machine — and before the frame bracket, where a `SetCursor` would be
        // competing with the swap chain.
        shell.update_cursor(self);

        // Copy the completed frame into the UI image, and the
        // mirror `Paste` reads back. Here, beside the cursor drain, because both are
        // the same shape: `dereth-ui` records what only a host with a window can perform. Both
        // directions are cheap -- the send happens only on a Copy, the read only when
        // `GetClipboardSequenceNumber` moved.
        shell.sync_clipboard();

        shell.compose_ui(self);

        spans.step(FrameStep::PrepareDevice);
        self.events.push(FrameEvent::Step(FrameStep::PrepareDevice));
        self.present.prepare_graphics_device();

        spans.step(FrameStep::BeginFrame);
        self.events.push(FrameEvent::Step(FrameStep::BeginFrame));
        if let Err(e) = self.present.start_frame() {
            tracing::error!("starting a frame failed: {e}");
            self.state = AppState::ShuttingDown;
            return false;
        }

        // **The 3D viewport follows the smart box.**
        //
        // Moving or resizing a docked object updates the smart-box region
        // and broadcasts the resulting change. The
        // handler that decides the final rectangle **ignores the notice's four parameters** and
        // re-reads its own region.
        //
        // So the rectangle is `<SBOX>`'s screen box, and reading it here -- once per frame, from
        // the live tree -- is the same answer the notice would carry with none of the machinery,
        // because a notice this build has no subscriber list for would be transcribed and unwired
        // rather than a fix. What is *not* optional is that something consume it: without this,
        // `hud::world_view::client_rect` and `dereth_render::camera::compute_game_viewport` are
        // called by nobody and the viewport never moves.
        spans.step(FrameStep::DrawWorld);
        self.events.push(FrameEvent::Step(FrameStep::DrawWorld));
        self.present.set_game_viewport(shell.game_viewport());
        if let Err(e) = self.present.draw_scene(self.world.as_ref()) {
            tracing::warn!("draw failed: {e}");
        }

        // Ending the frame with `true` is three things in one call: **the 2D UI
        // overlay**, `EndScene` and `Present`. The overlay is composited over the finished 3D
        // frame with the depth test off, which is why it is inside this step and not a step of its
        // own -- the documented frame order has fourteen calls and this is one of them.
        spans.step(FrameStep::PresentFrame);
        self.events.push(FrameEvent::Step(FrameStep::PresentFrame));
        if let Err(e) = shell.draw_ui(&mut self.present) {
            tracing::warn!("the UI overlay failed: {e}");
        }
        if let Err(e) = self.present.end_frame() {
            tracing::error!("finishing a frame failed: {e}");
            self.state = AppState::ShuttingDown;
            return false;
        }
        self.events.push(FrameEvent::FrameDrawn);

        spans.step(FrameStep::PaceFrame);
        self.events.push(FrameEvent::Step(FrameStep::PaceFrame));
        self.pacer.frame_sleep(self.pump.state.is_active_app);

        // `--enter-world` ends the loop when its script has run: `0x0013` made the session
        // playable, the log-off went out and the server answered. The retail equivalent is
        // Shift+Escape -> EXIT -> Yes, so the account is not left logged in until the server's own timeout.
        if self.script == EnterWorldScript::Done {
            self.pump.done();
            self.state = AppState::ShuttingDown;
            return false;
        }

        // `--frames n` is this rebuild's test limit; the ordinary client runs until shutdown.
        if self.cfg.frames.is_some_and(|n| self.frames_drawn() >= n) {
            self.pump.done();
            self.state = AppState::ShuttingDown;
            return false;
        }
        true
    }

    /// Show the first connection's failure, if the login has just ended before the link was up.
    ///
    /// Returns whether it did, which ends the frame loop. The box is shown once, through the
    /// platform's [`crate::platform::dialog::ErrorDialogHost`], and this returns only after the
    /// player has closed it; the same text goes to the log first so a run with no desktop still
    /// says why it stopped.
    fn connect_failure_use_time(&mut self) -> bool {
        if self.connect_failure.is_some() {
            return true;
        }
        let Some(code) = self.link.as_ref().and_then(|link| link.net.login_refusal()) else {
            return false;
        };
        let failure = crate::connect_failure::connect_failure(code, &*self.store);
        tracing::warn!("net error {}", code.id_string());
        tracing::warn!("{}: {}", failure.popup.caption, failure.popup.text);
        self.last_net_error = Some(code);
        self.dialog.show_modal(&failure.popup);
        self.connect_failure = Some(failure);
        true
    }

    /// The first connection's failure and the box it was shown in, once the login has ended
    /// before the link was up. `None` while the login is running, after it has succeeded, and for
    /// a link that was up and then went down.
    #[must_use]
    pub fn connect_failure(&self) -> Option<&crate::connect_failure::ConnectFailure> {
        self.connect_failure.as_ref()
    }

    /// Act on what the session decoded this frame.
    ///
    /// The retail function drives the player system and the login UI; with no UI, this is
    /// the same decisions taken by `--enter-world`'s script, plus a log line per event so a
    /// headless run says what it reached. Everything it reads is a [`dereth_client_net::client_session::SessionEvent`];
    /// it decodes nothing.
    pub fn process_logon_event_queue(
        &mut self,
        shell: &mut S,
        events: Vec<dereth_client_net::client_session::SessionEvent>,
    ) {
        use dereth_client_net::client_session::{SessionEvent, SessionState};

        // **The two "any -> Disconnected" edges, taken first.**
        //
        // Character-error and server-died receivers
        // are *notices*: the player system raises them, they queue a UI mode, and they read
        // nothing from the shared network state. So they are taken here, **before** the link guard below,
        // which makes them arrive with or without a packet controller -- and it is the one call
        // site, so there is nothing for a second path to drift away from.
        for e in &events {
            self.recv_disconnect_notice(shell, e);
            // **The phase-two smart-box reset, taken here for the same reason the two notices
            // above it are: before the link guard.**
            //
            // Character log-on phase 2 does not read shared network state to
            // decide to wipe the world — it wipes it and *then* asks the UI protocol to send. A
            // teardown that only ran while the link was healthy would skip exactly the endings
            // that need it (a transport drop, the 110 s timeout). See [`App::reset_world_view`].
            if matches!(e, SessionEvent::WorldReset) {
                self.reset_world_view();
                // **The subsystem sweep, on the same edge and for the same reason.**
                //
                // End-character-session handling is retail's *ending*, and in
                // retail it is reached by the same broadcast that reaches the
                // client object manager. It is taken here on the **entry** edge instead, which
                // is the same shape as `ObjectStream::reset` and `Teleport::reset`:
                // an ending this client never sees leaves the state set, and the entry edge is
                // reached by every ending there is. Nothing of the new session — its `0xF746`, its
                // objects, its `0x0013` — has arrived yet, so it needs no object-lifetime guard.
                //
                // See [`crate::interaction::Interaction::on_end_character_session`] for the field
                // list, which is read off rather than chosen here.
                self.interaction.on_end_character_session();
                // The same sweep empties the busy count: nothing the last character asked for is
                // still owed an answer.
                self.objects.world.magic.busy_count = 0;
                self.command_interpreter_disable();
                self.log_on_character_communication_clears();
            }
            if matches!(e, SessionEvent::PlayerCreated(_)) {
                // The player-description handler enables the command interpreter after accepting
                // the player id. Like Reset above,
                // this is local work owed by the already-decoded event, not by a surviving
                // packet controller.
                self.movement.enable(&mut self.char_input);
            }
            if let SessionEvent::CharacterSet(set) = e {
                // **The player system's account-name field.**
                //
                // Character-set delivery copies the unpacked account value into the field
                // **before** it looks
                // at the Turbine-chat flag and without consulting the link. That field is the one
                // `get_appropriate_spell_formula` hashes for a spell's tapers, so it is
                // taken here for the same reason the startup call below is: ahead of the link
                // guard, which a logon-queue delivery with no packet controller never passes.
                // `Hud`'s own `CharacterSet` arm sets the same field in stream order for the
                // batch path; both are plain assignments of the same string and are idempotent.
                self.objects
                    .world
                    .player_system
                    .account
                    .clone_from(&set.account);
                if set.use_turbine_chat != 0 {
                    // Startup performs local communication-provider registration before any player
                    // or gameplay screen exists. `Hud` already observes the stream-ordered model
                    // half; this idempotent call also serves direct logon-queue delivery.
                    self.objects.world.chat.startup_turbine_chat();
                    self.interaction.startup_turbine_chat_commands();
                }
            }
        }
        let Some(link) = self.link.as_mut() else {
            return;
        };
        let status = link.net.status();
        if self.last_link_status != Some(status) {
            self.last_link_status = Some(status);
            tracing::info!("link {status:?}");
        }
        let rejected = link.net.rejected();
        if rejected != self.last_rejected {
            self.last_rejected = rejected;
            tracing::warn!("{rejected} datagram(s) rejected");
        }
        if let Some(code) = link.net.error() {
            // Keyed on the code, not on a bool: the line prints
            // `code.id_string()`, so a latch prints the first error and swallows every distinct
            // one after it. See [`App::last_net_error`].
            if should_report_net_error(self.last_net_error, code) {
                self.last_net_error = Some(code);
                tracing::warn!("net error {}", code.id_string());
                // With a UI up the error is a *screen*, not an exit:
                // `build_host_state` already puts `id_string()` into `HostState::error` and
                // the front end queues the disconnected screen from it, and the screen's
                // OK button is the documented way out (the quit path goes gameplay ->
                // epilogue, never straight to `exit`). Only a run with no UI at
                // all -- `--no-ui`, the scripted slices -- still ends here, because in that build
                // there is nothing that could show the reason or take the player's answer.
                if !shell.has_ui() {
                    self.script = EnterWorldScript::Done;
                }
            }
            return;
        }
        let scripted = self.cfg.enter_world;
        let no_ui = !shell.has_ui();
        let wanted = self.cfg.start_char.clone();

        // The `--linger` deadline, checked before the batch so a run with no traffic still ends.
        if self.script == EnterWorldScript::Entering && self.cfg.linger > 0.0 {
            if let Some(at) = self.playable_at {
                if self.timer.local_time - at >= self.cfg.linger {
                    tracing::info!("logging off");
                    link.net.session.log_off();
                    self.script = EnterWorldScript::LoggingOff;
                }
            }
        }

        for e in events {
            match e {
                SessionEvent::CharacterSet(set) => {
                    // The persistent-data object's character-set notice is the hinge between
                    // the network and the flow. The session decodes `0xF658` and the pre-game
                    // view owns the destination; the copy between them is
                    // `character_set_from_login`. Setting it here rather than in `build_host_state`
                    // is deliberate: the client's is a **notice**, and a notice arrives once.
                    self.host_state.character_set = Some(character_set_from_login(&set));
                    self.host_state.received_set = true;
                    // **A notice is a count, not a value.** ACE re-sends the *same* list six
                    // seconds after a log-off (all five recorded sessions: `0xF653` and `0xF658`
                    // in the same instant). An identical set is no value edge at all, so a shell
                    // that applied this on `host.character_set != last_host.character_set` would
                    // drop the second notice — and it is the second one that carries the player
                    // out of the world in retail. See
                    // `UiShell::apply_host_notices`.
                    self.host_state.character_set_notices =
                        self.host_state.character_set_notices.wrapping_add(1);
                    // The account's Throne of Destiny flag gates one heritage and one start area
                    // in the character-generation wizard.
                    self.host_state.account_has_tod = set.has_throne_of_destiny != 0;
                    self.hud.era.account_has_throne_of_destiny = self.host_state.account_has_tod;
                    tracing::info!(
                        "account {:?}, {} character(s): {}",
                        set.account,
                        set.characters.len(),
                        set.characters
                            .iter()
                            .map(|c| c.name.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    // With the UI up, the front end's pre-game drive
                    // ([`Shell::drive_pregame_screens`]) presses the screen's own
                    // buttons and does the rest.
                    // This branch is the no-UI fallback, which is what `--no-ui` runs.
                    if scripted && no_ui && self.script == EnterWorldScript::AwaitingCharacterSet {
                        // `-u`/`-user` names the character in the retail switch set and this build
                        // is the first thing to read it; without one, the first slot.
                        let pick = set
                            .characters
                            .iter()
                            .find(|c| !wanted.is_empty() && c.name.eq_ignore_ascii_case(&wanted))
                            .or_else(|| set.characters.first());
                        if let Some(c) = pick {
                            tracing::info!("entering the world as {:?}", c.name);
                            let account = set.account.clone();
                            // See `CharacterAction::LogOn`.
                            self.host_state.entered_character = Some(c.name.clone());
                            link.net.enter_world(c.gid, &account);
                            self.script = EnterWorldScript::Entering;
                        } else {
                            tracing::info!("the account has no characters");
                            self.script = EnterWorldScript::Done;
                        }
                    }
                }
                SessionEvent::WorldInfo {
                    name,
                    connections,
                    max_connections,
                } => {
                    tracing::info!("world {name:?}, {connections}/{max_connections} connections");
                    // The restore response flows through the character-generation response notice
                    // and updates the character row that the UI then reads back.
                    self.host_state.world_name = Some(name);
                }
                SessionEvent::EnterWorldReady => tracing::info!("0xF7DF server ready"),
                SessionEvent::CharGenResponse(r) => {
                    // The character-create response handler ends every arm by sending the
                    // character-generation verification notice.
                    //
                    // The successful case is **not** followed by a fresh `0xF658`. ACE sends
                    // `GameMessageCharacterList` on authenticate, on delete and on log-off only
                    // (`AuthenticationHandler.cs:258`, `CharacterHandler.cs:322`,
                    // `Session.cs:273`); a creation is answered by
                    // `GameMessageCharacterCreateResponse` and a restore by
                    // `GameMessageCharacterRestore`, both `0xF643` and both alone. The client
                    // rebuilds the row out of *this* message — see the `0xF643` arm in
                    // `dereth_client_net::client_session` — and the notice below is what closes the please-wait modal.
                    tracing::info!(
                        "0xF643 char-gen response {} for {:?} ({:#010X})",
                        r.response_type,
                        r.identity.name,
                        r.identity.gid.0
                    );
                    self.host_state.chargen_response = Some(r.response_type);
                    // A notice is a count: two refusals in a row carry the same code.
                    self.host_state.chargen_response_notices =
                        self.host_state.chargen_response_notices.wrapping_add(1);
                }
                SessionEvent::CharacterDeleted => {
                    tracing::info!("0xF655 the slot was deleted");
                }
                SessionEvent::Ddd(dereth_client_net::client_session::DddEvent::Interrogation(
                    interrogation,
                )) => {
                    // The interrogation handler builds a list from every open data-file
                    // controller and answers with `0xF7E6` on queue 5. **Without
                    // this the phase never ends**: ACE's `DDDHandler` answers the *response*, not
                    // the interrogation, so a client that stays quiet is left on the data-patch
                    // screen for ever and `DDD_EndDDD` never arrives.
                    // Interrogation handling first tests product-id mask `0x4` and loads the
                    // high-resolution dat only when it is set. Opening it whenever it exists would
                    // make every two-level surface texture draw the high-res art retail never
                    // shows on ACE (which sends `0x1` unless `allow_highres_dat`).
                    if interrogation.product_id & PRODUCT_HIGHRES != 0 {
                        match self.store.grant_highres() {
                            Ok(opened) => tracing::debug!(
                                "0xF7E5 product id {:#x} grants client_highres.dat: {}",
                                interrogation.product_id,
                                if opened { "opened" } else { "no such file" }
                            ),
                            Err(e) => tracing::warn!("client_highres.dat did not open: {e}"),
                        }
                    }
                    let response =
                        ddd_interrogation_response(&self.store, interrogation.product_id);
                    tracing::debug!(
                        "0xF7E6 answering with {} iteration list(s): {:?}",
                        response.iters_with_keys.len(),
                        response
                            .iters_with_keys
                            .iter()
                            .map(|l| (l.dat_file_type, l.dat_file_id, l.iterations.iterations))
                            .collect::<Vec<_>>()
                    );
                    link.net.session.answer_ddd_interrogation(&response);
                    // The DDD state becomes interrogation-received: records
                    // that arrive from here until `0xF7E7` are early saves, which the begin handler
                    // subtracts from the byte count it shows the player.
                    self.ddd.on_interrogation();
                    self.pending_ddd
                        .push(dereth_client_contract::pregame::DddEvent::PatchtimeInterrogation);
                    tracing::debug!("DDD Interrogation({interrogation:?})");
                }
                SessionEvent::Ddd(d) => {
                    // The DDD notifier fans the event out to its plugins, including
                    // the data-patch screen while it is up. The translation is the whole of
                    // it: the protocol's `DddEvent` is the wire message, the pre-game view's is
                    // what the screen shows, and the client's `DDDEvent` enum is the same seven
                    // values.
                    use dereth_client_contract::pregame::DddEvent as UiDdd;
                    // **The patch is applied here, before the screen is told.**
                    // The `0xF7E2` arm saves the downloaded record asynchronously and only *then*
                    // notifies the UI of its compressed size, so the byte count
                    // the player watches is the count of bytes that reached the dat.
                    let ui_event = match &d {
                        // The interrogation is handled above, because answering it is what makes
                        // the phase finish.
                        dereth_client_net::client_session::DddEvent::Interrogation(_) => None,
                        // "The begin handler with a non-zero remaining byte count". The subtraction
                        // is expected bytes minus early-save bytes, which the patcher owns because
                        // it is the thing that counted the early saves.
                        dereth_client_net::client_session::DddEvent::Begin(b) => {
                            let (expected, action) = self.ddd.on_begin(b);
                            if action.send_end {
                                // Begin-request completion: nothing to download, so
                                // the DDD state becomes end-sent and `0xF7EA` goes out now.
                                link.net.session.send_ddd_end();
                            }
                            (expected != 0).then_some(UiDdd::PatchtimeBegin { expected })
                        }
                        dereth_client_net::client_session::DddEvent::Data(m) => {
                            let (outcome, action) = self.ddd.on_data(m);
                            if action.send_end {
                                // Removing the last pending download sends the end request.
                                link.net.session.send_ddd_end();
                            }
                            tracing::debug!("DDD applied {outcome:?}");
                            // The reported compressed size is the payload size minus 4, the wire
                            // payload length whether or not the body was compressed.
                            Some(UiDdd::DataDownloaded {
                                bytes: m.data.len() as u64,
                            })
                        }
                        dereth_client_net::client_session::DddEvent::End => {
                            // End-of-DDD closes the books; the application's cache flush runs
                            // below, after the `link`
                            // borrow ends.
                            self.ddd_invalidation = Some(self.ddd.on_end());
                            Some(UiDdd::PatchtimeEnd)
                        }
                        dereth_client_net::client_session::DddEvent::PatchtimePending => {
                            Some(UiDdd::PatchtimePending { total: 0 })
                        }
                        dereth_client_net::client_session::DddEvent::Error(e) => {
                            // The per-frame step's `0xF7E4` arm only marks the asynchronous get
                            // failed: the pending download stays pending, and no byte is written.
                            self.ddd.on_error(e);
                            None
                        }
                    };
                    if let Some(e) = ui_event {
                        // `DDD_PatchtimeEnd` also triggers the application's cache reload; what the
                        // *flow* needs from it is that the patch phase is over.
                        if e == UiDdd::PatchtimeEnd {
                            self.host_state.patch_finished = true;
                        }
                        self.pending_ddd.push(e);
                    }
                    tracing::debug!("DDD {d:?}");
                }
                SessionEvent::Dropped {
                    queue,
                    opcode,
                    reason,
                } => {
                    tracing::debug!("dropped {opcode:?} on {queue:?}: {reason:?}");
                }
                SessionEvent::PlayerCreated(id) => {
                    tracing::debug!("0xF746 player is {:#010X}", id.0);
                }
                SessionEvent::PlayerDescription(_) => {
                    tracing::info!("0x0013 player description -- in world");
                    // Player-description delivery raises the notice answered with
                    // loading the `"#auto"` screen layout. The field is used rather than
                    // [`Self::note_player_description`] because `link` is borrowed here; the
                    // method exists so the same decision is reachable from a test.
                    self.pending_auto_layout = true;
                }
                SessionEvent::StateChanged(SessionState::Playable) if scripted => {
                    // `--linger` holds the session in the world so the objects have time to move.
                    // Without it a scripted run logs off in the same frame `0x0013` arrives, which
                    // is the default.
                    self.playable_at = Some(self.timer.local_time);
                    if self.cfg.linger <= 0.0 {
                        tracing::info!("logging off");
                        link.net.session.log_off();
                        self.script = EnterWorldScript::LoggingOff;
                    } else {
                        tracing::info!("in world; staying {} s", self.cfg.linger);
                    }
                }
                SessionEvent::StateChanged(s) => tracing::info!("session {s:?}"),
                SessionEvent::LoggedOff => {
                    tracing::info!("logged off");
                    // **The loop does not end on every `0xF653`.**
                    //
                    // `EnterWorldScript::Done` is `App::frame`'s exit condition. Set with no guard,
                    // the *server's* `0xF653` — which in every one of the five recorded sessions
                    // arrives **six seconds** after the client asks — would end the main loop of a
                    // client that had just returned to character select, and the player could not
                    // enter the world again because there would be no process to do it with.
                    //
                    // A log-off to character select never ends the retail client:
                    // character-session teardown clears the player and leaves the
                    // loop running, and the only thing that stops is
                    // the device-done edge from the epilogue screen. So the loop ends here **only** when
                    // there is no screen to go back to (`--no-ui`) or when this run's own script
                    // asked for the log-off (`--enter-world --linger`), which are the two builds
                    // that have nothing to return to. This is exactly the shape of the
                    // `CharacterError` arm below, and for the same reason: ending the loop instead
                    // of showing a screen is indistinguishable from a crash from the player's seat.
                    if no_ui || self.script == EnterWorldScript::LoggingOff {
                        self.script = EnterWorldScript::Done;
                    }
                }
                SessionEvent::CharacterError(code) => {
                    tracing::warn!("character error {code}");
                    // Character-error delivery reaches the UI flow, which maps the code
                    // to a `StringInfo` in table `0x10000002` and
                    // queues disconnected mode `0x10000002` with that text. A code with no token is the
                    // switch's `default:` arm and **never queues a mode**.
                    //
                    // The notice itself is raised in [`Self::recv_disconnect_notice`], above and
                    // outside this loop. What is left here is the *script*, and it does not end the
                    // process when there is a UI: the client's own answer to a character error is
                    // a screen with the reason on it and a button, and ending the loop instead is
                    // indistinguishable from a crash from the player's seat. `--no-ui` ends the
                    // loop, because in that build there is no screen to show it on and nothing to
                    // take the answer.
                    if no_ui {
                        self.script = EnterWorldScript::Done;
                    }
                }
                _ => {}
            }
        }
        // The `DDD_PatchtimeEnd` cache-invalidation arm is taken here
        // rather than in the loop because it needs `&mut self` and the loop holds `link`.
        self.invalidate_after_ddd();
    }

    /// Handle `DDD_PatchtimeEnd` by making caches forget what
    /// the patch changed.
    ///
    /// The client shuts down the language interface, releases the master property list, flushes
    /// each object cache, restarts the language interface, and refreshes the active region, in that
    /// order.
    ///
    /// Retail's free-object flush drops only objects nothing still holds, so **the client does not
    /// invalidate a referenced object either** — it drops the two singletons it knows are
    /// referenced (the language interface and the master property list) by hand and re-creates
    /// them.
    ///
    /// This build's equivalent is to reopen the store. Every `DatFile` caches its whole B-tree at
    /// open, so a reader that was up across the patch holds entries naming chains that are now on
    /// the free list; [`dereth_dat::DatFile::reload`] is the fix and this is its one production
    /// caller. What that reaches, and what it does not, is the list in [`crate::ddd`]'s module
    /// documentation — the short version being that every heavy consumer
    /// (`WorldScene`, `LandSource`, `DatAnimAssets`, `ObjectStream`, the renderer's world) takes
    /// its `Arc<RetailDatStore>` at **world entry**, which is after DDD, so it picks up the
    /// replacement; the UI shell takes its copy at `start_shell`, which is before, and does not.
    fn invalidate_after_ddd(&mut self) {
        let Some(summary) = self.ddd_invalidation.take() else {
            return;
        };
        tracing::info!(
            "DDD finished -- {} applied, {} refused, {} stale, {} still pending, \
             {} file(s) changed",
            summary.applied,
            summary.refused,
            summary.stale,
            summary.still_pending,
            summary.changed.len()
        );
        if !summary.changed_anything() {
            // The overwhelmingly common case: the server said "you are up to date". Reopening
            // 1.4 GB of container for nothing is exactly the kind of cost a patch path should not
            // impose on a session that had no patch.
            return;
        }
        match crate::assets::open_data_files_with(
            &self.cfg.dat_dir,
            self.cfg.world_dat_dir.as_deref(),
        ) {
            Ok(fresh) => {
                self.store = std::sync::Arc::new(fresh);
                tracing::info!(
                    "DDD reopened the dat files; {} changed",
                    summary
                        .changed
                        .iter()
                        .map(|t| t.file_name())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            // Reopening failed, so the old store stands. Stale is better than absent: the client
            // has a complete, self-consistent view of the files as they were.
            Err(e) => tracing::warn!(
                "DDD could not reopen the dat files ({}); the old view stands and a \
                 restart is needed to see the patch",
                e.cause
            ),
        }
    }

    /// The asynchronous cache-miss path's production caller.
    ///
    /// [`crate::land_source::DatLandSource::build`] is the run-time cache miss: a landblock record
    /// the store does not carry, discovered while the streaming ring was building a block. This
    /// turns each one into the `0xF7E3` the client sends, under the client's own two conditions --
    /// a connection has to exist and one `QualifiedDataID` gets one
    /// outstanding request per `QualifiedDataID`.
    ///
    /// The return leg is the other half: a `0xF7E2` answering one of those gets goes through the
    /// ordinary patcher (the client's arm has no state gate either), and the record then has to
    /// reach the *land source's* handle on the store, which is an `Arc` it took at world entry.
    /// That is what [`crate::land_source::DatLandSource::resupply`] is for.
    fn request_missing_cell_records(&mut self) {
        let land = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(|c| std::sync::Arc::clone(c.land()));
        let Some(land) = land else { return };
        let resupplied = self.ddd.take_resupplied();
        if !resupplied.is_empty() {
            // The patch is on disk; the land source is still reading through the handle it took
            // at world entry. `invalidate_after_ddd` replaced `App::store` for the *patch* phase;
            // a run-time answer arrives with no `0xF7EA` behind it, so the reopen happens here.
            match crate::assets::open_data_files_with(&self.cfg.dat_dir, self.cfg.world_dat_dir.as_deref()) {
                Ok(fresh) => {
                    let fresh = std::sync::Arc::new(fresh);
                    self.store = std::sync::Arc::clone(&fresh);
                    land.resupply(fresh, &resupplied);
                    tracing::info!(
                        "0xF7E2 answered {} run-time get(s); the land source was reseeded",
                        resupplied.len()
                    );
                }
                Err(e) => tracing::warn!(
                    "a run-time DDD answer landed but the dat files would not reopen ({}); the old view stands",
                    e.cause
                ),
            }
        }
        // `0xF7E4` failed a get. The asynchronous-get failure path has already taken
        // it out of the pending-gets list; forgetting the cached `None` is what lets the streaming
        // ring ask for the block again rather than answering from the miss it remembers.
        let failed = self.ddd.take_failed_gets();
        if !failed.is_empty() {
            land.forget_blocks(&failed);
        }
        // A network cache lookup returns false without
        // a packet controller, so with no link the get simply fails and nothing is
        // queued for later. Draining the misses anyway would lose them; leaving them is what lets
        // the next connected frame ask.
        let Some(link) = self.link.as_mut() else {
            return;
        };
        let sent = crate::ddd::drain_cache_misses(&land, &mut self.ddd, &mut link.net.session);
        if sent > 0 {
            tracing::info!("0xF7E3 requested {sent} missing cell record(s) from the server");
        }
    }

    /// The `DddPatcher` this client patches through, for a test that wants to see what a session
    /// did with a patch.
    #[must_use]
    pub fn ddd(&self) -> &crate::ddd::DddPatcher {
        &self.ddd
    }

    /// Character-error and server-died notices, the two edges that reach the disconnected screen
    /// from anywhere; and account-booted and account-banned responses, the two that reach it
    /// without a notice.
    ///
    /// Both are notices rather than reads: the player system raises them and the flow queues
    /// disconnected mode `0x10000002` with the string info. What this function produces is its
    /// **symbolic id**; the front end's UI does the queueing (its host-state step carries
    /// the server-died notice's "unless the current mode is already `0x10000002`" guard) and
    /// the screen resolves the id against table enum `0x10000002`.
    ///
    /// `character_error_string_id` returning `None` is the switch's `default:` arm, which "leaves
    /// the `StringInfo` untouched and **never queues a mode**" — so a code with no token shows
    /// nothing at all, rather than an empty screen.
    ///
    /// The latch is first-notice-wins because there is no reconnect path: once the client is on
    /// the disconnected screen the only way off it is the OK button.
    ///
    /// **`0xF7DC` and `0xF7C1` are the two edges that carry their own sentence.**
    /// Account-booted and account-banned handling raises no character error and looks no token up:
    /// each formats a message from a built-in literal, wraps it with
    /// a literal string value, and queues disconnected mode `0x10000002`
    /// itself. So what those two arms put in `HostState::error` is the **text**, not a token, and
    /// the shell's resolve-or-pass-through is what shows it — which is why the shell's string
    /// resolver must not find a row for it.
    ///
    /// The literals are the front end's: see [`Shell::disconnect_message`].
    fn recv_disconnect_notice(
        &mut self,
        shell: &S,
        e: &dereth_client_net::client_session::SessionEvent,
    ) {
        let id = shell.disconnect_message(e, &*self.clock);
        let Some(id) = id else { return };
        if self.host_state.error.is_none() {
            tracing::info!("the disconnected screen, showing {id}");
            self.host_state.error = Some(id);
        }
    }

    /// Complete one admitted message and its synchronous object-arrival callbacks before the
    /// owner asks Session for another raw entry. No second instance or UI timestamp admission.
    fn deliver_session_events(&mut self, shell: &mut S, now: dereth_primitives::LocalTime) {
        let events = self
            .link
            .as_mut()
            .map(|link| link.net.drain_events())
            .unwrap_or_default();
        // The use-position-from-server test, which answers `autonomy_level != 2`.
        // The vector-update path asks it before applying a `0xF74E` **about the player**, and `ObjectStream`
        // has no command interpreter — in the client `WorldObjects` holds a pointer to one.
        // Pushed in here, before the drain that consumes it, so the answer a message is
        // measured against is the one the interpreter held when it arrived.
        self.objects
            .note_use_position_from_server(self.movement.lists.autonomy_level != 2);
        for event in events {
            let body = self.world.as_ref().and_then(|w| w.character.as_ref());
            self.interaction.prepare_ui_dispatch(
                body,
                &mut self.objects.world,
                self.present.size(),
            );
            let (geometry, radius) = self.current_selection_geometry();
            self.interaction.last_use_time = now;
            let inter = &mut self.interaction;
            let hud = &mut self.hud;
            let arrived =
                self.objects
                    .apply_event_with_dispatch(&event, now, &mut |world, notice| {
                        inter.dispatch_object_notice(world, notice.clone(), &geometry, radius, now);
                        shell.object_panel_notice(hud, inter, world, &notice);
                    });
            // A public description update forces a synchronous object-description Control enqueue. Preserve that
            // boundary: it reaches Session before queued notice delivery, released arrivals, or
            // the next admitted object message. Packet serialization retains the packet controller's
            // frame slot and may therefore happen on the next frame.
            self.objects
                .drain_pending_requests(self.link.as_mut().map(|link| &mut link.net.session));
            // Old lifetime queues are retired here, before publishing the new instance and
            // before any callback can admit a subsequent object-scoped packet.
            self.objects
                .retire_session_instances(self.link.as_mut().map(|l| &mut l.net.session));
            self.deliver_object_notices();
            // `Admin_Environs` is a player-system local handler, not a Hud or interaction
            // notice. Consume only its accepted visual and sound cases here; unknown values remain
            // visible to the generic unreceived-opcode diagnostics.
            let admin_environs = self.apply_admin_environs(shell, &event);
            let events = if admin_environs {
                &[][..]
            } else {
                std::slice::from_ref(&event)
            };
            // `Hud::now` is stamped here, not only once a frame by `Hud::drive` — which runs on
            // the **gameplay screen**, so a message arriving during the login transition would be
            // measured against `0.0`.
            // `0x0013 Login_PlayerDescription` is exactly such a message and it carries the
            // enchantment registry, whose `_start_time` is relative to receipt. Stamped here, from
            // the same clock `ObjectStream::apply_event` and `Interaction::last_use_time` get, so
            // that every handler in the batch reads the instant the packet was processed — which
            // is what reads in retail.
            self.hud.now = now;
            let _ = self.apply_hud_events(shell, events);
            self.interaction.last_use_time = now;
            let (geometry, radius) = self.current_selection_geometry();
            let hud = &mut self.hud;
            crate::interaction::apply_events_at_boundary(
                &mut self.interaction,
                events,
                &mut self.objects.world,
                Some((&geometry, radius)),
                &mut |inter, world, notice| shell.object_panel_notice(hud, inter, world, notice),
            );
            // Defender handlers stamp/AutoTarget first; their synchronously raised selection
            // notices then reenter Combat before another admitted UI packet can change state.
            let body = self.world.as_ref().and_then(|w| w.character.as_ref());
            self.interaction
                .dispatch_ui_selection_notices(body, &mut self.objects, now);
            self.teleport.apply_events(events);
            // Accepted local movement/control-transfer tails precede the next accepted message.
            self.player_teleport_use_time(shell);
            if matches!(
                event,
                dereth_client_net::client_session::SessionEvent::WorldObject { .. }
                    | dereth_client_net::client_session::SessionEvent::PlayerCreated(_)
            ) {
                // `sync_objects` performs no physics or animation clock advance. Drain remote
                // movement and physical/state projections now, so two accepted commands cannot
                // overwrite Presence.pending_movement before the first callback runs. The local
                // journal above has already removed its matching snapshot to avoid double apply.
                self.sync_objects();
            }
            self.process_logon_event_queue(shell, vec![event]);
            for (id, instance) in arrived {
                let Some(link) = self.link.as_mut() else {
                    continue;
                };
                if self.objects.world.weenie(id).is_some() {
                    link.net.session.begin_weenie_arrival(id);
                }
                // Weenie creation and the physical null-object setup have independent success. A
                // surviving Weenie remains known to subsequent UI input, without publishing a body.
                if self.objects.world.physics(id).is_none() {
                    continue;
                }
                link.net.session.begin_object_arrival(id, instance);
                // Object-net-blob processing updates the object first. Callback
                // deletion can remove this ordering window, so ask it for only one entry.
                while self
                    .link
                    .as_mut()
                    .is_some_and(|link| link.net.session.process_next_object_ui(id, now))
                {
                    self.deliver_session_events(shell, now);
                }
                // Snapshot only after UI callbacks finish; each physical blob is admitted
                // against the then-current object table, and re-parks if still not ready.
                let blobs = self
                    .link
                    .as_mut()
                    .map(|link| link.net.session.take_object_message(id))
                    .unwrap_or_default();
                for blob in blobs {
                    if let Some(link) = self.link.as_mut() {
                        link.net.session.process_object_message(&blob, now);
                    }
                    self.deliver_session_events(shell, now);
                }
            }
        }
    }

    fn current_selection_geometry(
        &self,
    ) -> (crate::selection_geometry::SceneSelectionPhysics, f32) {
        let origin = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(crate::character::Character::position);
        let geometry =
            crate::selection_geometry::SceneSelectionPhysics::new(origin.as_ref(), &self.objects);
        let radius = dereth_client_contract::radar::radar_range(
            origin
                .as_ref()
                .is_some_and(|p| dereth_physics::landdefs::is_outdoors(p.cell)),
        );
        (geometry, radius)
    }

    fn ui_queue_use_time(&mut self, shell: &mut S) {
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        self.deliver_session_events(shell, now);
        while self
            .link
            .as_mut()
            .is_some_and(|link| link.net.session.process_next_ui(now))
        {
            self.deliver_session_events(shell, now);
        }
        crate::interaction::registered_systems_use_time(
            &mut self.interaction,
            self.present.scene(self.world.as_ref()).as_deref(),
            &mut self.objects,
            self.link.as_mut().map(|link| &mut link.net),
            now,
        );
        shell.control_notice(crate::shell::ControlNotice::TargetMode(
            self.interaction.target_mode() != crate::interaction::TargetMode::None,
        ));
    }

    fn step_incoming_world_objects(&mut self, shell: &mut S) {
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        while self
            .link
            .as_mut()
            .is_some_and(|link| link.net.session.process_next_world_object(now))
        {
            self.deliver_session_events(shell, now);
        }
        // The existing socket-free ObjectStream component API can have accepted a journal
        // directly. Consume it here, never before physics and never twice in the phased path.
        //
        // **Unconditional, and that is the point.** Smart-box use time reaches its input-queue
        // drain every frame after the two cell-manager arms rejoin. The guard tests the persistent
        // list object, not whether it contains an entry, so an empty list still enters the loop,
        // observes a null head, and breaks. Its tail is likewise unconditional. Retail has no
        // "is there anything queued" test in front of this
        // phase, and neither may this build: `player_teleport_use_time`'s counter is incremented
        // before each of its own early returns precisely so that "the step was skipped" and "the
        // step ran with nothing to do" stay distinguishable, and hoisting the function's own
        // `has_player_motion_dispatches` predicate into an outer `if` collapses those two states
        // back together. The teleport and stance-stop tests both pin one call per frame here,
        // before `position_use_time`.
        self.player_teleport_use_time(shell);
        // Newly created/retired scene projections settle before the world draw, not before physics.
        self.sync_objects();
    }

    /// Build the landscape around the player once the server has said where the player is.
    ///
    /// This is the application's stand-in for telling the landscape where to
    /// load. Landblock *streaming* — re-scrolling the window as the player walks — is not done
    /// here: the window is built once, around the block the player
    /// entered in, and a player who walks out of it walks off the drawn world.
    fn load_pending_scene(&mut self) {
        let Some(cfg) = self.pending_scene else {
            return;
        };
        let Some(player) = self.objects.player() else {
            return;
        };
        let Some(pos) = self.objects.presence(player).and_then(|p| p.position) else {
            return;
        };
        let block = pos.cell.landblock();
        let landblock = (u16::from(block.x()) << 8) | u16::from(block.y());
        let cfg = crate::scene::SceneConfig { landblock, ..cfg };
        tracing::info!(
            "entering the world at landblock 0x{landblock:04X}, cell {:#010X}",
            pos.cell.0
        );
        self.pending_scene = None;
        let _load = tracing::debug_span!("load_world", landblock, cell = pos.cell.0).entered();
        if let Err(e) = self.present.load_world(&self.store, cfg, &mut self.world) {
            tracing::error!("the landscape would not load: {e}");
            return;
        }
        if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
            world.set_environment_override_state(self.environment_override.clone());
        }
        // Stand the body where the server says the player is, so the chase camera looks at the
        // part of the world the server is talking about.
        if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
            if let Some(c) = world.world_mut().character.as_mut() {
                c.teleport(pos);
            }
            world.world_mut().follow_character_now();
        }
        self.report_scene(landblock);
    }

    /// Build the landblocks the window scrolled onto this frame.
    ///
    /// A failure here is not fatal: the block stays undrawn and the next scroll will ask for it
    /// again. It is counted and logged rather than swallowed, because a device that cannot make a
    /// texture will not recover on its own and a silent hole in the world is the worst kind of
    /// gap.
    fn stream_world(&mut self) {
        let _stream = tracing::trace_span!("stream_world").entered();
        let store = std::sync::Arc::clone(&self.store);
        // The client updates rendering preferences every frame. It sits **here**, at the top
        // of the frame's streaming step, because the work a changed preference asks for is the
        // same work a landblock scroll asks for -- release, queue, build -- and that work needs
        // the device and must stay outside the frame bracket.
        match self
            .present
            .update_render_preferences(&store, self.world.as_mut())
        {
            Ok(w) => {
                // The poll itself, whether or not it moved anything: this event's payload is
                // what `last_render_pref_work` reads.
                self.events.push(FrameEvent::RenderPreferencesPolled(w));
                if w.flushed || w.mid_radius_changed || w.detail_texturing_changed {
                    self.events.push(FrameEvent::RenderPreferencesApplied);
                    tracing::info!(
                        "render preferences changed -- flush {}, mid_radius {}, \
                         detail textures {} (no consumer in this build), {} block(s) queued, \
                         {} resident",
                        w.flushed,
                        w.mid_radius_changed,
                        w.detail_texturing_changed,
                        w.blocks_queued,
                        w.blocks_rebuilt,
                    );
                }
            }
            Err(e) => {
                self.events
                    .push(FrameEvent::StreamFailed(StreamStage::RenderPreferences));
                tracing::warn!("applying a render preference failed: {e}");
            }
        }
        if let Err(e) = self.present.stream_world(&store, self.world.as_mut()) {
            self.events
                .push(FrameEvent::StreamFailed(StreamStage::Landblocks));
            tracing::warn!("landblock streaming failed: {e}");
        }
        let Some(world) = self.present.scene(self.world.as_ref()) else {
            return;
        };
        // Crossing a building's threshold changes the viewer's cell from a land cell to
        // an env cell, which is what switches the draw and the collision. Logged because it is the
        // one state change a person walking around cannot otherwise see.
        //
        // Keyed on everything the line prints, not on the cell alone. `inside`
        // streams and is asked of the *camera's* cell rather than the body's — see
        // [`ViewerCellReport`].
        let report = world
            .viewer_cell_id()
            .map(|c| ViewerCellReport::new(c, world.interior_batches()));
        if report != self.last_viewer_cell {
            self.last_viewer_cell = report;
            if let Some(r) = report {
                tracing::debug!(
                    "viewer cell {:#010X} ({}), {} interior batch(es)",
                    r.cell.0,
                    if r.outdoors { "outdoors" } else { "indoors" },
                    r.inside
                );
            }
        }
        let Some(block) = world.viewer_block() else {
            return;
        };
        // A window streaming in over several frames changes the census on every one of them;
        // report the block once it has settled rather than once per frame.
        if world.blocks_pending() > 0 {
            return;
        }
        // Keyed on everything the line prints: the six counters below stream in
        // asynchronously under a block that is not moving. See [`ViewerBlockReport`].
        let report = ViewerBlockReport::new(block, &world.census());
        if self.last_viewer_block != Some(report) && self.viewer_block_gate.ready() {
            self.last_viewer_block = Some(report);
            tracing::debug!(
                "landblock 0x{:02X}{:02X}, {} blocks resident, {} terrain surfaces, \
                 {} scenery + {} buildings + {} statics, {} triangles",
                report.block.0 & 0xFF,
                report.block.1 & 0xFF,
                report.blocks_meshed,
                report.terrain_surfaces,
                report.scenery_objects,
                report.buildings,
                report.static_objects,
                report.object_triangles
            );
        }
    }

    /// How many times [`Self::stream_world`] failed, for the tests. A green run has zero: the
    /// counter exists so that a tolerated failure cannot hide behind a passing suite.
    #[must_use]
    pub const fn stream_failures(&self) -> u64 {
        self.events.total(FrameEventKind::StreamFailed)
    }

    /// How many frames applied a changed scene-owned render preference, and what the last of
    /// them did. The preference update is a poll, so both stay at zero on every frame on which
    /// nothing moved.
    #[must_use]
    pub const fn render_pref_applies(&self) -> u64 {
        self.events.total(FrameEventKind::RenderPreferencesApplied)
    }

    /// [`Self::render_pref_applies`]'s detail: the last poll's own flags.
    #[must_use]
    pub const fn last_render_pref_work(&self) -> crate::frame_events::RenderPrefWork {
        match self.events.last(FrameEventKind::RenderPreferencesPolled) {
            Some(FrameEvent::RenderPreferencesPolled(w)) => w,
            // No poll yet, which is `RenderPrefWork::default()` written out: `Default::default`
            // is not a `const fn` and this accessor has always been one.
            _ => crate::frame_events::RenderPrefWork {
                flushed: false,
                mid_radius_changed: false,
                detail_texturing_changed: false,
                blocks_queued: 0,
                blocks_rebuilt: 0,
                detail_surfaces: 0,
            },
        }
    }

    /// Give every new object geometry and every moved object its new frame.
    fn sync_objects(&mut self) {
        let store = std::sync::Arc::clone(&self.store);
        if let Err(e) = self
            .present
            .sync_objects(&store, &mut self.objects, self.world.as_mut())
        {
            tracing::warn!("object sync failed: {e}");
        }
        // Step 4 both draws the object and
        // puts it in the world: the line above is the first half and this is the second. The
        // physics world is `Character`'s, so an object is solid exactly when there is a body for it
        // to be solid to. See [`crate::object_physics`].
        if let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) {
            // This must come first: the local body takes the id `0xF746` gave before
            // any of the server's objects are registered, so the physics world can
            // never be asked to hold two bodies under one id.
            c.adopt_server_id(self.objects.player());
            self.objects.sync_physics_at(
                &store,
                &mut c.world,
                dereth_primitives::LocalTime(self.timer.cur_time),
            );
        }
        // The gate is everything the line prints, with the two message-rate
        // counters entering by magnitude so the census cannot go quiet *or* flood; the raw values
        // are what is printed. See [`ObjectReport`].
        let n = self
            .present
            .scene(self.world.as_ref())
            .map_or(0, |w| w.server_object_count());
        let w = self
            .present
            .scene(self.world.as_ref())
            .map_or_else(Default::default, |w| w.census());
        let report = ObjectReport::new(n, &w, &self.objects.stats);
        let key = report.key();
        if self.last_object_report != Some(key) && self.object_report_gate.ready() {
            self.last_object_report = Some(key);
            tracing::debug!(
                "{} object(s) drawn ({} animated, {} held, {} setups, \
                 {} triangles) -- {} created, {} merged, {} recreated, {} removed, {} moved, \
                 {} motions, {} parent events, {} out of a container",
                report.drawn,
                report.animated,
                report.held,
                report.setups,
                report.triangles,
                report.creates,
                report.merges,
                report.recreates,
                report.removes,
                report.position_updates,
                report.movement_updates,
                report.parent_events,
                report.container_exits_offered
            );
        }
    }

    /// Build the landscape now rather than at startup. `--connect` uses this.
    pub fn defer_static_scene(&mut self, scene: crate::scene::SceneConfig) {
        // Remembered as well as armed, so [`App::reset_world_view`] can arm it
        // again on the next world entry.
        self.scene_config = Some(scene);
        self.pending_scene = Some(scene);
    }

    /// The phase-two smart-box reset — the world teardown.
    ///
    /// Raised by [`dereth_client_net::client_session::SessionEvent::WorldReset`], which
    /// login **phase 2** issues immediately before the enter-world request.
    /// So it runs on the *enter-world* edge, strictly before the new
    /// session's `0xF746`, its objects or its `0x0013` can arrive — which is what lets it be
    /// unconditional. Keyed on object lifetime instead, this reset would eat the session it is
    /// supposed to be preparing.
    ///
    /// The retail cascade and where each half of it lands here:
    ///
    /// | retail | here |
    /// |---|---|
    /// | release landscape blocks and flush cells | `Renderer::release_world` |
    /// | destroy objects, leaving the world for each | `ObjectStream::reset`, on the same event |
    /// | clear the physics player and smart-box player | the body is inside the scene, so it goes with it |
    /// | destroy queued network blobs | `Session`'s parked lists, already cleared |
    ///
    /// **Object destruction walks the player too** — its loop takes every
    /// entry of the object hash and calls `exit_world`, `leave_world`, `unset_parent`,
    /// `unparent_children` and the virtual destructor with no test for the player at all. This
    /// build splits the player's body (`WorldScene::character`) from the object tables
    /// (`ObjectStream`), and **retail has no such split**; releasing the scene is what retires the
    /// body here, and that is the only reason the two halves stay in step.
    fn reset_world_view(&mut self) {
        let released = self.present.release_world(&mut self.world);
        self.events.push(FrameEvent::WorldReset {
            textures_released: released,
        });
        // The landscape is armed again rather than rebuilt now: the block to build it around is
        // the one the *server* puts the player in, and nothing knows that until the new session's
        // `0xF745` arrives. `load_pending_scene`'s own guards hold it until then, and it is that
        // function that stands the body where the server says, so it must run again.
        self.pending_scene = self.scene_config;
        self.last_viewer_block = None;
        self.last_viewer_cell = None;
        self.last_object_report = None;
        tracing::debug!(
            "(1) -- world torn down, {released} texture slot(s) \
             released, landscape re-armed: {}",
            self.pending_scene.is_some()
        );
    }

    /// Login phase 2's communication clears. They bracket the smart-box reset and
    /// enter-world request: clear the squelch database first, then set talk focus back to `All`
    /// after requesting entry. The following combat-mode reset is discussed below.
    ///
    /// All three run before anything of the new session can arrive — the enter-world send is the
    /// `0xF657` that *asks* for it — so they are taken on the one event raised at `phase_two`
    /// rather than at three separate positions in it. Nothing can interleave between them, which is
    /// what makes that collapse an ordering that cannot be observed rather than one that is
    /// guessed.
    ///
    /// # Why this is needed at all, given `ObjectStream::reset` already replaces the `World`
    ///
    /// It is needed for the **hole**, not for the common case, and the common case is already
    /// right: `ObjectStream::reset` ends `self.world = ()`, and the squelch DB, the talk
    /// focus and `CombatState::combat_mode` all live on that `World` — so every *ending* this
    /// client sees already clears them.
    ///
    /// The hole is that function's first two lines:
    ///
    /// ```text
    /// if self.presences.is_empty() && self.world.player.is_none() { return; }
    /// ```
    ///
    /// After an ending has run, both are true — so the `WorldReset` arm is a **no-op
    /// on the entry edge**, and anything written into the `World` between the ending and the next
    /// entry survives into the next session. `0x01F4 Communication_SetSquelchDB` is a *UI-queue*
    /// message and `Hud::ui_event`'s arm for it has no player gate at all, so it is exactly such a
    /// write. Retail has no hole to close because its squelch-database clear is unconditional.
    ///
    /// `set_combat_mode(NONCOMBAT_COMBAT_MODE, true)` is **deliberately not called
    /// here**, and it is not an omission:
    ///
    /// * the combat-mode setter's first statement is `if mode == combat_mode { return
    ///   Ok(()) }` — the client's early return — and `CombatState::combat_mode` is already
    ///   `CombatMode::NonCombat` from world construction, so the call would do nothing;
    /// * it cannot be reached through the hole above the way the squelch can. The only writer that
    ///   is not a `World` replacement is `Hud::apply_quality_update`, and that goes through
    ///   the player lookup, which returns `None` when there is no player — so
    ///   between an ending and the next entry there is nothing that can set it;
    /// * and with `send_to_server == true` it is the one of the three that would put a `0x0053` on
    ///   the wire. A reset that sends a datagram has to be earning it.
    fn log_on_character_communication_clears(&mut self) {
        // See [`dereth_client_model::chat::ChatState::clear_squelch_db`].
        self.objects.world.chat.clear_squelch_db();
        // `TalkFocus::All` is the Say channel: a player who left the last session talking on
        // Allegiance must not still be
        // talking on Allegiance when a different character logs in.
        self.objects
            .world
            .chat
            .set_talk_focus(dereth_client_model::chat::TalkFocus::All);
    }

    /// Clear movement commands on the next session edge.
    ///
    /// Without this the three command lists are never emptied on a session edge, so the run lock
    /// survives a logout: auto-run, log out, log in, and the new character is **already
    /// running**, because `apply_current_movement` opens with
    /// `if (auto_run) move_player(0x45000005 /* WalkForward */)` and stays there for as long as the
    /// flag is set. The body walks off the placement the shard just gave it and
    /// [`App::position_use_time`] reports that it did, so item drops land in the wrong place on a
    /// second login.
    ///
    /// The client's disable step clears all command lists, releases hold-run and hold-sidestep,
    /// conditionally applies and sends the resulting movement while the player remains autonomous,
    /// and finally clears the interpreter's enabled flag. This entry-edge helper performs the
    /// command and hold cleanup that prevents the next character inheriting the old run state.
    ///
    /// The broader next-entry cleanup protects an unclean ending too, but it deliberately does
    /// not change `enabled`: the client has no reset-to-disable edge. Disable happens on the
    /// retail-proven `request_log_off` edge and enable on the accepted `0xF746` edge. The client's
    /// disable itself does **not** clear `auto_run`, so this entry-edge cleanup is not
    /// attributed to the scalar store above either.
    fn command_interpreter_disable(&mut self) {
        // `clear_all_commands`, `set_auto_run(false)` and the projection, all three of which
        // `MovementCommands::clear_all_commands` already is.
        self.movement.clear_all_commands(&mut self.char_input);
        // Release the physical hold-run input and recompute its effective value against the UI
        // toggle, just like every other `set_hold_run` caller.
        self.char_input.run = self
            .movement
            .lists
            .set_hold_run(false, self.movement.ui_toggles_run);
        self.movement.lists.hold_sidestep = false;
        // The entry-edge cleanup intentionally also clears the run lock through
        // `clear_all_commands`; unlike `request_log_off`, this edge does not disable the interpreter.
    }

    /// How many times [`App::reset_world_view`] has run.
    #[must_use]
    pub const fn world_resets(&self) -> u64 {
        self.events.total(FrameEventKind::WorldReset)
    }

    /// Texture slots the teardowns have handed back, cumulatively.
    #[must_use]
    pub const fn world_textures_released(&self) -> u64 {
        self.events.amount(FrameEventKind::WorldReset)
    }

    /// The switches the landscape is (re)built from, for the tests.
    #[must_use]
    pub fn scene_config(&self) -> Option<crate::scene::SceneConfig> {
        self.scene_config
    }

    /// The startup line for a scene, shared by the immediate and the deferred path.
    fn report_scene(&mut self, landblock: u16) {
        let Some(world) = self.present.scene(self.world.as_ref()) else {
            return;
        };
        let s = world.census();
        tracing::info!(
            "landblock 0x{landblock:04X}, {} blocks, {} terrain surfaces, \
             {} scenery + {} buildings + {} statics",
            s.blocks_meshed,
            s.terrain_surfaces,
            s.scenery_objects,
            s.buildings,
            s.static_objects
        );
    }

    /// The object tables and the render facts, for the tests and the log lines.
    #[must_use]
    pub fn objects(&self) -> &crate::objects::ObjectStream {
        &self.objects
    }

    /// The same tables, mutable. A test seeds a player and an inventory the
    /// way a `0x0013 Login_PlayerDescription` would, so that a combat-mode toggle has something to
    /// decide against without a server.
    pub fn objects_mut(&mut self) -> &mut crate::objects::ObjectStream {
        &mut self.objects
    }

    /// The interaction layer and the game model **at the same time**.
    ///
    /// Several of the production entry points a harness has to call take both --
    /// `Interaction::on_world_object_found(found, &mut world, now)` is the one that named it -- and
    /// [`Self::interaction_mut`] and [`Self::objects_mut`] are two separate borrows of `self`, so a
    /// caller with only those two has to build a model host of its own per scenario. They are
    /// disjoint fields; this hands out both.
    pub fn interaction_and_world_mut(
        &mut self,
    ) -> (
        &mut crate::interaction::Interaction,
        &mut dereth_client_model::World,
    ) {
        (&mut self.interaction, &mut self.objects.world)
    }

    /// Install a socket-free replay endpoint on a headless App that has no live link. The real
    /// frame consumes its raw transport queues; no decoded event/model replacement is injected.
    /// Returns the unconsumed endpoint on refusal, preserving an existing live/replay session.
    #[allow(clippy::result_large_err)] // the refused network is handed back whole, once
    pub fn attach_replay_network(
        &mut self,
        net: crate::net::ClientNetwork,
    ) -> Result<(), crate::net::ClientNetwork> {
        if !self.cfg.headless || self.link.is_some() {
            return Err(net);
        }
        self.link = Some(NetLink::replay(net));
        Ok(())
    }

    /// Install a socket-free endpoint whose datagrams the host carries (a web page's relay), on
    /// an App of any kind that has no link yet. It is the replay endpoint's arrangement for a
    /// windowed client: the frame consumes the transport queues the host fills and drains, and
    /// nothing else about the run changes.
    ///
    /// # Errors
    /// The endpoint back, unconsumed, when the App already has a link.
    #[allow(clippy::result_large_err)] // the refused network is handed back whole, once
    pub fn attach_relay_network(
        &mut self,
        net: crate::net::ClientNetwork,
    ) -> Result<(), crate::net::ClientNetwork> {
        if self.link.is_some() {
            return Err(net);
        }
        self.link = Some(NetLink::replay(net));
        // A real server is behind the relay, and it answers for itself.
        self.server_stub = None;
        Ok(())
    }

    /// Feed/inspect only an explicitly socket-free endpoint, never an owner's live connection.
    /// The dat store this client reads, shared with its world. A probe for the tests: whether
    /// the DDD interrogation's product id granted `client_highres.dat`.
    #[must_use]
    pub fn dat_store(&self) -> &std::sync::Arc<dereth_dat::RetailDatStore> {
        &self.store
    }

    pub fn replay_network_mut(&mut self) -> Option<&mut crate::net::ClientNetwork> {
        self.link
            .as_mut()
            .filter(|link| link.local_addr().is_none())
            .map(|link| &mut link.net)
    }

    /// The interaction state: `WorldObjects`'s pick, the `SearchReason` machine and the request
    /// counters.
    #[must_use]
    pub fn interaction(&self) -> &crate::interaction::Interaction {
        &self.interaction
    }

    /// The same state, writable — the sibling of [`Self::objects_mut`] and
    /// `Self::renderer_mut`, and for the same reason: a harness that drives a real `App` must be
    /// able to hand it the pointer events would have
    /// dispatched, which is [`crate::interaction::Interaction::queue`]. Without it
    /// the only way to exercise a click against a *resized* smart box is to build an `Interaction`
    /// by hand, and an `Interaction` built by hand has never had `App::frame` push
    /// `<SBOX>`'s rectangle into it, which is precisely the step under test.
    pub fn interaction_mut(&mut self) -> &mut crate::interaction::Interaction {
        &mut self.interaction
    }

    /// The production UI-queue interaction boundary, also usable without a socket by replay
    /// harnesses. `App::frame` delivers the already ordered session events here.
    ///
    /// **It runs with no panels callback and no selection geometry**, which is not what
    /// [`Self::frame`] does — see [`Self::apply_interaction_events_to_panels`] for the boundary
    /// the frame really uses. This one is kept as it is because eighty-odd stations under
    /// `tests/` are written against it.
    pub fn apply_interaction_events(
        &mut self,
        events: &[dereth_client_net::client_session::SessionEvent],
    ) {
        self.interaction.last_use_time = dereth_primitives::LocalTime(self.timer.cur_time);
        crate::interaction::apply_events(&mut self.interaction, events, &mut self.objects.world);
    }

    /// The same boundary **as `App::frame` runs it**: the current selection geometry, and the
    /// frame's own panels callback, so that a panel which is a cached join of the notice stream
    /// sees the message.
    ///
    /// [`Self::apply_interaction_events`] passes `None` geometry and a no-op
    /// `panels` argument, so a harness that delivers through it has every panel blind to the
    /// message it delivers -- silently, which makes a wrong assertion look right rather than
    /// red. The two lines below are `frame`'s own, lifted so a harness can reach them; `frame`
    /// itself is unchanged and still calls `apply_events_at_boundary` inline.
    pub fn apply_interaction_events_to_panels(
        &mut self,
        shell: &mut S,
        events: &[dereth_client_net::client_session::SessionEvent],
    ) {
        self.interaction.last_use_time = dereth_primitives::LocalTime(self.timer.cur_time);
        let (geometry, radius) = self.current_selection_geometry();
        let hud = &mut self.hud;
        crate::interaction::apply_events_at_boundary(
            &mut self.interaction,
            events,
            &mut self.objects.world,
            Some((&geometry, radius)),
            &mut |inter, world, notice| shell.object_panel_notice(hud, inter, world, notice),
        );
    }

    fn deliver_object_notices(&mut self) {
        self.interaction.last_use_time = dereth_primitives::LocalTime(self.timer.cur_time);
        let notices = self.objects.take_notices();
        self.interaction
            .apply_object_notices(&mut self.objects.world, notices);
    }

    /// Drain the window queue completely, then process the Alt+Enter latch and return the done
    /// state.
    ///
    /// "the pump drains the queue completely each frame — it is not the classic `PeekMessage`-or-
    /// render alternation", which is what a zero timeout on `pump_events` gives.
    fn do_event_loop(&mut self, shell: &mut S) -> bool {
        let time_ms = self.clock.tick_ms();
        // The full-screen-allowed flag. `finish_event_loop` reads it --
        // `s.full_screen = s.allow_full_screen_mode && !s.full_screen` is the event loop's own
        // epilogue -- and left at its `true` default it would make Alt+Enter live in every mode.
        //
        // It is the exact gate the full-screen rule needs: full screen belongs to
        // gameplay, so Alt+Enter is refused outside it rather than honoured and then quietly
        // undone by [`Self::follow_gameplay_full_screen`] at the next mode edge. Inside gameplay
        // the toggle is untouched and still overrides the preference until the player leaves.
        //
        // Set every frame rather than on the mode edge because it has to be right at the instant
        // the drain below consumes the latch, which is not an edge frame.
        self.pump.state.allow_full_screen_mode = shell.in_gameplay();
        let drained = self
            .window
            .pump_events((self.cfg.width, self.cfg.height), self.applied_full_screen);

        for event in drained.events {
            self.handle_window_event(shell, &event, time_ms);
        }
        shell.window_input(self, time_ms);
        if drained.exited {
            self.pump.done();
        }

        // The renderer exists whenever the graphics engine is up, which it is here.
        let done = self.pump.finish_event_drain(true);
        // The `Display.FullScreen` shadow comparison runs once a
        // frame: a value that differs from what the presentation is wearing triggers a change.
        // Both producers land here — Alt+Enter, which
        // `finish_event_drain` has just applied to the full-screen display preference, and
        // startup/live window state. Display requests use this same helper immediately after their
        // UI drain, matching retail's later-in-frame device preparation.
        //
        // The second half of the same flag compares the resolution display preference against
        // its own shadow *before* the full-screen one and raises the identical flag, so a
        // resolution change reaches `change_presentation` by exactly this route. The comparison
        // here is on the decoded `(width, height)` rather than on the packed mode descriptor,
        // because this build's presentation is the client rectangle and that is what the shadow has
        // to be able to answer for.
        self.apply_changed_display_presentation(shell);
        done
    }

    /// Apply the event loop's display-shadow transition.
    ///
    /// The shared check keeps the event-loop producer and the UI preference producer on the same
    /// transition: once [`Self::change_presentation`] moves the shadows, the next poll is a no-op.
    fn apply_changed_display_presentation(&mut self, shell: &mut S) {
        if self.pump.state.full_screen != self.applied_full_screen
            || (self.cfg.width, self.cfg.height) != self.applied_resolution
            || self.cfg.display.sync_to_refresh != self.applied_sync_to_refresh
        {
            self.change_presentation(shell);
        }
    }

    /// The App consumer for one host event, also used by socket-free synthetic window-event
    /// tests. DPI size negotiation belongs earlier, inside the host's live callback.
    pub fn handle_window_event(&mut self, shell: &mut S, event: &HostEvent, time_ms: u32) {
        let refresh_frame = matches!(event, HostEvent::ScaleFactorChanged)
            || matches!(event, HostEvent::Resized { width, height } if *width > 0 && *height > 0);
        if refresh_frame && !self.applied_full_screen {
            // The caption changes DPI even when our override keeps the client pixel extent
            // unchanged (which need not generate a Resized event). Refresh on either edge.
            if let Some(metrics) = self.window.frame_metrics() {
                self.frame_metrics = metrics;
            }
        }
        if let HostEvent::Resized { width, height } = event {
            if *width > 0 && *height > 0 {
                // A monitor/DPI change can resize the host without a preference change. Keep
                // the backbuffer and UI/hit-test extent in the host's same physical coordinates.
                // No reposition or preference write here; a minimized zero extent is ignored.
                self.restart_rendering_system(shell, *width, *height);
            }
        }
        self.note_flycam_input(event);
        // The window procedure runs its message table over the lifecycle messages the event maps
        // to. A front end whose input layer listens to the same messages (focus loss releases
        // every held control) hands them on itself.
        self.pump.on_window_event(event, time_ms);
        //  / `Deactivate` also reach the sound system:
        // `Focus` is what the "play only when the window is active" preference reads.
        if let HostEvent::Focused(active) = event {
            if let Some(audio) = self.audio.as_ref() {
                audio.set_focus(*active);
            }
        }
    }

    /// Build the two display-preference choice lists from the
    /// modes this machine reports.
    ///
    /// Returns how many resolution rows the drop-down will hold. The fall-back to
    /// [`STANDARD_DISPLAY_MODES`] is the declared divergence documented on that constant; the
    /// line it prints says which source was used, because "the list is short" and "the list came
    /// from the wrong place" are the same symptom otherwise.
    pub fn register_display_modes(&mut self) -> usize {
        use dereth_client_contract::options::store::{self, DisplayMode};

        let mut modes = self.window.display_modes();
        let enumerated = modes.len();
        if modes.is_empty() {
            modes = STANDARD_DISPLAY_MODES
                .iter()
                .map(|(width, height)| DisplayMode {
                    width: *width,
                    height: *height,
                    refresh_rate: 0,
                    bits_per_pixel: 32,
                })
                .collect();
        }
        store::initialize_display_preferences(&modes);
        let n = store::display_choices(store::DISPLAY_RESOLUTION).len();
        tracing::debug!(
            "-- {enumerated} adapter mode(s), \
             {n} resolution(s), {} refresh rate(s){}",
            store::display_choices(store::DISPLAY_REFRESH_RATE).len(),
            if enumerated == 0 {
                " (standard table: no adapter modes reported)"
            } else {
                ""
            }
        );
        n
    }

    /// Change presentation, minus the D3D9 device reset this build does not have.
    ///
    /// The client changes the window style, applies a frame-changed `SetWindowPos`, resets the
    /// presentation, moves the window to its final rectangle and Z-order, then notifies the UI of
    /// the display change.
    ///
    /// The order is the client's, including the hazard it accepts: the style and the Z-order
    /// change **before** the device is reset and the final move happens **after**. The reset here
    /// is the presentation's resize, the swap-chain half, and the UI half is
    /// [`Shell::set_display`], and this is the caller that makes it run after start-up.
    ///
    /// The presentation stays **windowed** in the D3D sense at all times: this build uses
    /// borderless windowed rather than exclusive mode, so there is no mode switch to make and
    /// [`dereth_client_contract::window_proc::placement`] hands back the monitor's rectangle instead of the
    /// requested resolution. See that function for the divergence in full.
    ///
    /// **The placement function is the second declared divergence in this path.** The final
    /// `SetWindowPos`'s rectangle is
    /// [`dereth_client_contract::window_proc::change_presentation_placement`], whose arithmetic
    /// keeps the window at its existing top-left rather than returning it to the creation-time
    /// centre: a window size change does not snap the window to the centre of the screen, and
    /// leaving full screen puts the window back at the top-left it had before it went full screen.
    /// Read that function for retail's numbers and the off-screen rule.
    fn change_presentation(&mut self, shell: &mut S) {
        let full = self.pump.state.full_screen;
        // The client reads the *previous* presentation's `FullScreen` byte — the copy taken
        // before the display-preference load overwrote it — and branches on it,
        // so the old value has to be captured before the shadow moves.
        let was_full = self.applied_full_screen;
        self.applied_full_screen = full;
        self.applied_resolution = (self.cfg.width, self.cfg.height);
        self.applied_sync_to_refresh = self.cfg.display.sync_to_refresh;
        self.present
            .set_presentation_sync(full, self.cfg.display.sync_to_refresh);
        // **This function is split in two, and the split is retail's own.**
        // `change_presentation`'s window calls (`SetWindowLongA`, the two `SetWindowPos`) act on
        // the window handle; resetting the device and
        // broadcasting global message `(0x0E, 0)` do not touch a window at all. A
        // headless run has no window handle and every other step still applies to it — the
        // offscreen target is resized by the same device reset and the element manager is re-laid
        // out by the same broadcast. Returning at the `host` test would leave a headless `App`
        // unable to apply a presentation change and nothing able to measure one.
        let requested = (self.cfg.width, self.cfg.height);
        // The placement is computed for both paths, and *before* the `host` test, for the same
        // reason the rendering restart is in front of it: the arithmetic touches no window.
        // `GetWindowRect` is the live window when there is one and [`Self::window_rect`]'s
        // maintained rectangle when there is not, so the "do not re-centre" rule is observable
        // headlessly.
        let screen = self.window.screen_metrics(self.frame_metrics);
        let current = if self.window.has_window() {
            self.window.window_rect()
        } else {
            self.window_rect
        };
        // The rectangle whose top-left a windowed result keeps (client divergence CD-001): the
        // window's own for a windowed change; on leaving full screen, the one it had before it
        // went full screen, since its current one is the monitor's. Going full screen remembers
        // it.
        let keep = match (was_full, full) {
            (false, true) => {
                self.windowed_rect_before_full_screen = current;
                None
            }
            (true, false) => self.windowed_rect_before_full_screen.take(),
            (true, true) => None,
            (false, false) => current,
        };
        // **`change_presentation_placement`, NOT `placement`.** Creation placement centres on
        // the screen because it is creating a window; a presentation change has a window already,
        // and this build keeps its **top-left**. Calling `placement` here would snap the window
        // to the middle of the monitor on every resolution pick and on both edges of the
        // forced-resolution path.
        let place = dereth_client_contract::window_proc::change_presentation_placement(
            full,
            true,
            i32::try_from(self.cfg.width).unwrap_or(i32::MAX),
            i32::try_from(self.cfg.height).unwrap_or(i32::MAX),
            keep,
            &screen,
        );
        self.window_rect = Some(dereth_client_contract::window_proc::Rect {
            left: place.x,
            top: place.y,
            right: place.x + place.cx,
            bottom: place.y + place.cy,
        });
        if !self.window.has_window() {
            self.restart_rendering_system(shell, requested.0, requested.1);
            return;
        }
        // **Full screen is the window system's own request; windowed is retail's arithmetic.**
        //
        // The windowed branch is `SetWindowLongA` + `SetWindowPos` as the client makes them:
        // `place.style`'s `WS_POPUP` bit, the Z-order, then the outer rectangle
        // [`dereth_client_contract::window_proc::change_presentation_placement`] computed.
        //
        // The full-screen branch does not imitate a mode switch with those same three calls.
        // Restyle-plus-move-plus-resize is only a fullscreen *request* on Windows; see
        // [`crate::platform::window::WindowHost::set_borderless_fullscreen`] for why it produces
        // a chromeless non-fullscreen window on Wayland and on macOS. The rectangle is still
        // computed above, because the offscreen path below resizes to it and the placement
        // assertions read it; a window, when there is one, is asked instead of arranged.
        let applied = if full {
            self.window.set_borderless_fullscreen(true);
            self.window.set_topmost(place.topmost);
            // The window system answers with the extent it granted, which on every backend is
            // the monitor's and need not be the rectangle computed above -- a scaled display or
            // a compositor with a reserved edge will differ, and the back buffer must follow the
            // window rather than the arithmetic.
            //
            // On Wayland the request is asynchronous and this reads the *old* extent, because the
            // compositor has not answered yet. That is correct and not a race: the answer arrives
            // as a `Resized`, and [`Self::handle_window_event`] puts it straight through
            // `restart_rendering_system`, which is the same call this line ends in. The first
            // frame after the switch is drawn at the old size; every one after it is not.
            self.window.client_size()
        } else {
            self.window.set_borderless_fullscreen(false);
            // The style, exactly as the client's `SetWindowLongA` is.
            self.window
                .set_decorations(place.style & dereth_client_contract::window_proc::WS_POPUP == 0);
            self.window.set_topmost(place.topmost);
            // `cx, cy` are the **outer** rectangle; framed, the host adds the frame back, so the
            // client area asked for is the requested resolution -- which is what
            // the original `cx = Width + 2 * dialog-frame width` calculation specifies.
            let applied = self
                .window
                .request_inner_size(self.cfg.width, self.cfg.height);
            self.window.set_outer_position(place.x, place.y);
            applied
        };
        self.restart_rendering_system(shell, applied.0, applied.1);
    }

    /// Reset the device, then broadcast global message `0x0E` — `change_presentation`'s tail,
    /// with no window in it, lifted out of [`Self::change_presentation`].
    ///
    /// The element manager's `display` must agree with the back buffer or every screen box is
    /// wrong, which is why the broadcast reads the size back off the device rather than trusting
    /// the requested one: `ResizeBuffers` is allowed to answer with a different extent, and a
    /// failed resize must leave the UI on the size the device still has.
    fn restart_rendering_system(&mut self, shell: &mut S, width: u32, height: u32) {
        if (width, height) != self.present.size() && width > 0 && height > 0 {
            if let Err(e) = self.present.resize(width, height) {
                tracing::warn!("presentation change: {e}");
                return;
            }
        }
        let (w, h) = self.present.size();
        // Broadcast the global refresh message, which re-lays the UI out at the new extent and
        // pushes UI global message `0x0E` at every registered listener.
        shell.set_display((
            i32::try_from(w).unwrap_or(i32::MAX),
            i32::try_from(h).unwrap_or(i32::MAX),
        ));
    }

    /// Apply display-preference requests to the window state.
    ///
    /// `Display.FullScreen` is a registered preference with an options-page row
    /// (`dereth_ui_screens::options::config`'s `GRAPHICS` group).
    /// [`crate::config::Config::apply_preferences`] parses it into
    /// [`crate::config::DisplayPrefs::full_screen`], and this is its reader. The write lands on
    /// the full-screen preference, which is [`dereth_client_contract::window_proc::DeviceState`]'s
    /// `full_screen` — the same field Alt+Enter flips — and the change is applied at the end of
    /// the producing frame after the UI request drain, which is where retail's per-frame poll
    /// applies it too.
    fn apply_display_preference_requests(
        &mut self,
        shell: &S,
        requests: Vec<dereth_client_contract::UiRequest>,
    ) -> Vec<dereth_client_contract::UiRequest> {
        use dereth_client_contract::{PrefValue, UiRequest};
        let in_gameplay = shell.in_gameplay();
        let mut resolution: Option<i32> = None;
        let rest: Vec<UiRequest> = requests
            .into_iter()
            .filter(|r| {
                match r {
                    UiRequest::SetPreference(name, PrefValue::Bool(v)) => {
                        if name.eq_ignore_ascii_case("Display.FullScreen") {
                            self.cfg.display.full_screen = *v;
                            // **The one rule, in the one place that is not a mode edge.** The
                            // preference is stored whatever mode the player is in -- the options
                            // page is reachable from character select, and a box they tick there
                            // has to survive to the next save. The *shadow* only ever carries it
                            // in gameplay, because that is where full screen lives. Ticking the box
                            // outside gameplay therefore changes nothing on screen until they enter
                            // the world, which is what [`Self::follow_gameplay_full_screen`] then
                            // does.
                            self.pump.state.full_screen = *v && in_gameplay;
                            return false;
                        }
                        if name.eq_ignore_ascii_case("Display.SyncToRefresh") {
                            self.cfg.display.sync_to_refresh = *v;
                            return false;
                        }
                    }
                    // `Display.Resolution` — the resolution drop-down. The drop-down's selection
                    // callback stores the packed mode descriptor straight into the resolution
                    // preference, and the rendering preference poll picks the change up later in
                    // the producing frame. Without this arm the value falls past every consumer and
                    // is printed as *"UI request with no owner yet"*.
                    UiRequest::SetPreference(name, PrefValue::Int(v))
                        if name.eq_ignore_ascii_case(
                            dereth_client_contract::options::store::DISPLAY_RESOLUTION,
                        ) =>
                    {
                        resolution = Some(*v);
                        return false;
                    }
                    _ => {}
                }
                true
            })
            .collect();
        if let Some(desc) = resolution {
            self.set_display_resolution(desc);
        }
        rest
    }

    /// Store the resolution display preference and decode its width and height.
    ///
    /// The width is the high 16 bits and the height the low 16; below 800x600 the load fails.
    ///
    /// The refusal below 800x600 is the client's and is kept: the display-preference load
    /// answering `false` makes `change_presentation` return `false` without touching anything,
    /// which is what leaving the fields alone reproduces. The packed value is still stored,
    /// because the drop-down's store happens before `change_presentation` runs and it is what the
    /// next preference save writes.
    ///
    /// Returns whether the presentation size moved.
    fn set_display_resolution(&mut self, desc: i32) -> bool {
        let packed = u32::from_ne_bytes(desc.to_ne_bytes());
        self.cfg.display.resolution = packed;
        // The mode decode owns the floor as well; `self.forced_pair()` is
        // the forced-resolution override, which sits *after* the decode and before the caller sees the
        // presentation. So a size chosen while the force is up is stored and not applied — which
        // is retail, and is why the drop-down is only reachable from the gameplay screen.
        let Some(p) = self.cfg.load_display_preferences(None, true) else {
            tracing::warn!(
                "Display.Resolution {packed:#010x} is below \
                 the display-preference load's 800x600 floor; the presentation is unchanged"
            );
            return false;
        };
        // The preference was authored, so this is the size to come back to when the force lifts.
        self.unforced_resolution = (p.width, p.height);
        let p = self
            .cfg
            .load_display_preferences(self.forced_pair(), true)
            .unwrap_or(p);
        if (p.width, p.height) == (self.cfg.width, self.cfg.height) {
            return false;
        }
        self.cfg.width = p.width;
        self.cfg.height = p.height;
        true
    }

    /// The forced width and height when the override is enabled.
    ///
    /// This is the argument [`crate::config::Config::load_display_preferences`] carries for the
    /// display-size override — the mechanism by which retail runs the login and character-select
    /// screens at a fixed size.
    fn forced_pair(&self) -> Option<(u32, u32)> {
        self.use_forced_resolution.then_some(self.forced_resolution)
    }

    /// Whether the display-size override is enabled.
    #[must_use]
    pub fn uses_forced_resolution(&self) -> bool {
        self.use_forced_resolution
    }

    /// The outer window rectangle, used to position the resized window.
    ///
    /// `pub` for the same reason [`Self::force_display_resolution`] is: it is the observable the
    /// "do not re-centre on a size change" rule is asserted on, and the window-position tests
    /// have to be able to read it.
    #[must_use]
    pub fn window_rect(&self) -> Option<dereth_client_contract::window_proc::Rect> {
        self.window_rect
    }

    /// Force display resolution with `(bool force, ulong width, ulong height)`.
    ///
    /// Width, height, and the force flag are always stored first. With no active device, or before
    /// presentation readiness, nothing else happens. Enabling the force changes presentation only
    /// when the forced size differs from the current size. Disabling it changes presentation only
    /// when the old flag was set and the saved preference differs from the current size; an invalid
    /// saved preference still falls through to the forced fallback dimensions.
    ///
    /// The client calls this while entering the login screen (`true`), during construction
    /// (`false`), and when gameplay-screen lifetime changes (`true` unless gameplay is active). The
    /// console command is a fourth entry point.
    /// [`Self::follow_screen_forced_resolution`] is the pair of screen-lifetime calls.
    ///
    /// **`pub` because it is `static` in retail** and because the resolution tests have to lift
    /// the force to reach the drop-down, exactly as opening
    /// Client Options from the gameplay screen does.
    pub fn force_display_resolution(
        &mut self,
        shell: &mut S,
        force: bool,
        width: u32,
        height: u32,
    ) {
        let was_forced = self.use_forced_resolution;
        self.forced_resolution = (width, height);
        self.use_forced_resolution = force;
        // With no renderer or an uninitialized device, the flag is recorded and
        // nothing else happens, which is exactly the state of the startup call.
        if !self.device_is_initialized() {
            return;
        }
        let live = self.present.size();
        let want = if force {
            // /`Height` against the forced pair.
            if live == (width, height) {
                return;
            }
            (width, height)
        } else {
            // un-forcing something that was never forced does nothing at all.
            if !was_forced {
                return;
            }
            // the restore arm, which is the display-preference load with the override
            // now off. See [`Self::unforced_resolution`] for why the target is latched here.
            let target = self.unforced_resolution;
            if live == target {
                return;
            }
            target
        };
        // Below the 800x600 floor the display-preference load answers false, `change_presentation`
        // returns false and nothing moves — the corresponding below-range branch.
        if want.0 < 800 || want.1 < 600 {
            return;
        }
        self.cfg.width = want.0;
        self.cfg.height = want.1;
        // Apply the new dimensions immediately through a tail call, rather than waiting for a poll.
        self.change_presentation(shell);
    }

    /// Presentation readiness and renderer availability are the two gates guarding every arm of
    /// [`Self::force_display_resolution`].
    ///
    /// The renderer in this build is constructed with the `App`, so the observable that tells a
    /// live device from a not-yet-initialised one is its extent.
    fn device_is_initialized(&self) -> bool {
        let (w, h) = self.present.size();
        w > 0 && h > 0
    }

    /// Follow gameplay-screen construction and destruction, the only two screen-lifetime callers of
    /// [`Self::force_display_resolution`]. Construction disables the forced
    /// 800x600 login size. Destruction restores it unless the application is already quitting.
    ///
    /// The gameplay object exists exactly while the current mode is gameplay, so observing the
    /// mode-switch edge reproduces its constructor and destructor. `Pump::is_done` supplies the
    /// quitting predicate; a closing window is never resized.
    /// **`Display.FullScreen` applies on entering the game, and only there.** This is a
    /// declared divergence (client divergence CD-005) with no retail counterpart, because retail's full screen is a D3D9
    /// device mode that exists for the life of the process.
    ///
    /// Two separate things are wrong with applying it at start-up. Creating the window
    /// borderless and moving it to the monitor rectangle **is not a fullscreen request** and is
    /// not portable: a window cannot position itself on Wayland, and on macOS the rectangle does
    /// not cover the menu bar or the Dock, so both platforms would draw a chromeless window that
    /// does not fill the screen. And on Windows, where the rectangle does work, coming into
    /// gameplay *from* a full-screen character select goes through
    /// [`Self::follow_screen_forced_resolution`]'s forced display resolution on the same edge,
    /// which fights it.
    ///
    /// So: the pre-game flow is windowed, the world is whatever the preference says, and the
    /// transition is the mode edge. This runs on the same `screen_changed` edge as the forced
    /// resolution and for the same reason -- the flow has just swapped the screen, so
    /// this *is* the gameplay-screen construction/destruction edge. It writes only the shadow;
    /// [`Self::apply_changed_display_presentation`] notices and
    /// [`Self::change_presentation`] does the work, exactly as it does for Alt+Enter and for the
    /// options page.
    ///
    /// Alt+Enter outside gameplay is refused rather than fought: this function would undo it on
    /// the next mode edge, and a toggle that silently reverts is worse than one that does
    /// nothing. The event loop (`do_event_loop`) is where that refusal lives.
    pub fn follow_gameplay_full_screen(&mut self, shell: &mut S) {
        if self.pump.state.is_done {
            return;
        }
        self.pump.state.full_screen = shell.in_gameplay() && self.cfg.display.full_screen;
    }

    pub fn follow_screen_forced_resolution(&mut self, shell: &mut S) {
        if self.pump.state.is_done {
            return;
        }
        let gameplay = shell.in_gameplay();
        // `self.forced_resolution` rather than the literal `800, 600` retail pushes: see
        // [`Self::unforced_resolution`]. For every client whose presentation came from the
        // preference the two are the same number, which is the only case the force acts in.
        let (w, h) = self.forced_resolution;
        self.force_display_resolution(shell, !gameplay, w, h);
    }

    /// What a window's focus loss does to the controls the runtime itself latches.
    ///
    /// The camera's held look controls are cleared, the keyboard's movement commands are dropped
    /// the way the client's keyboard-focus loss drops them, and the mouse-look hold is released.
    /// The per-control release -- every held control firing its end -- is the device input's, and
    /// a front end does it when it sees the same focus loss.
    ///
    /// **It is `pub`** because a focus loss is an event a test can construct, and the arm below is
    /// a step whose absence is only visible from outside the event loop.
    pub fn note_flycam_input(&mut self, event: &HostEvent) {
        if let HostEvent::Focused(false) = event {
            self.input = CameraInput::default();
            // The lists have to go with it. Retail's pressed-key release fires
            // a synthetic release for every held control on focus loss; clearing the slots
            // without clearing the lists would let the next command re-assert a key nobody is
            // holding, because `apply_current_movement` reads the list and not the bool.
            //
            // The per-control half of the pressed-key release is **not** here and must not be:
            // `Pump::map_window_event` turns this same `Focused(false)` into `WM_KILLFOCUS`,
            // and the device input's handler for it calls `release_pressed_keys`, which walks
            // the active-controls table and fires a real zero-extent release per held control
            // down the ordinary `fire_action_event` path. That is the focus-loss release loop,
            // and it is what makes an alt-tab with Jump held run the jump-release handler
            // rather than stranding the charge. The focus-release tests measure it end to end.
            //
            // This line calls `lose_keyboard_focus`, which the input manager invokes through the
            // command interpreter after its active-control hash is empty.
            //
            // It is not `self.char_input = CharacterInput::default()` followed by
            // `MovementCommands::clear_all_commands`: neither statement matches the client's
            // focus-loss behavior, which clears only the *keyboard* commands, never calls
            // `set_auto_run`, and refuses to re-apply movement while the server owns the body.
            // There is no wholesale wipe — `lose_keyboard_focus` reaches
            // the six motion slots the way every other caller does, through
            // `apply_current_movement`'s projection, and `char_input.jump` is cleared every
            // frame by `App::frame` regardless. See
            // [`crate::character::MovementCommands::lose_keyboard_focus`] for the bytes.
            //
            // The returned `bool` controls the movement event, which is not sent —
            // named as a gap on that function rather than half-wired here.
            let _movement_changed = self.movement.lose_keyboard_focus(&mut self.char_input);
            // **The third thing the pressed-key release fires.**
            // Its last act, when mouse-look mode is on and a device exists, is a
            // synthetic button input event (activation `0x80`) on the
            // virtual device's control 1 — the mouse-look hold, released so the mode cannot
            // survive the focus loss. The device input's `WM_KILLFOCUS` arm already does the
            // input-manager half (`leave_mouse_look`); this is the half that lives here,
            // because [`Self::mouse_look`] is a second, app-level latch that only
            // `WindowEvent::MouseInput` otherwise writes. Without this line an alt-tab with the
            // right button held comes back with the latch still set, and the next cursor
            // motion swings the camera with no button down until the user presses and
            // releases it again. Synthesised through the same seam a real button-up uses, so
            // there is one code path and not two.
            self.mouse_look_button(false);
        }
    }

    /// The app-level mouse-button projection: the right button is the mouse-look hold, and
    /// the mouse-look flag is what
    /// [`Self::cursor_moved`] reads.
    ///
    /// **`pub`** because a front end calls it for its mouse-look button.
    /// The window system's own mouse-button event carries a device id that cannot be constructed
    /// without `unsafe` — `crate::pump`'s own module documentation says so and splits its half out
    /// for the same reason — so code inline in that arm would be code no test in this workspace
    /// could reach. The pumped events are plain data, so that arm is reachable; this split stays
    /// because the two halves are the two things the arm does.
    pub fn mouse_look_button(&mut self, down: bool) {
        self.mouse_look = down;
        // A fresh hold starts from the cursor's next sample, never from where it was left.
        self.last_cursor = None;
    }

    /// The `CursorMoved` arm, split out with [`Self::mouse_look_button`] and for the same reason.
    pub fn cursor_moved(&mut self, x: f64, y: f64) {
        let now = (x, y);
        if self.mouse_look {
            if let Some(prev) = self.last_cursor {
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: a cursor delta in pixels for a debug flycam, narrowed for its
                // own f32 arithmetic. Not engine arithmetic.
                let (dx, dy) = ((now.0 - prev.0) as f32, (now.1 - prev.1) as f32);
                if let Some(mut world) = self.present.scene_mut(self.world.as_mut()) {
                    // With a body this is camera-set mouse look through
                    // `crate::actions::camera`; without one it is the flycam look.
                    crate::camera::mouse_look(
                        &mut *world,
                        dx,
                        dy,
                        dereth_primitives::LocalTime(self.timer.cur_time),
                    );
                }
            }
        }
        self.last_cursor = Some(now);
    }

    /// The residual flycam's "up", held or released: the one control with no action behind it.
    ///
    /// The shipped keymap has no action that raises or lowers a viewpoint, because retail has no
    /// viewpoint to raise -- the camera is a swept sphere pivoting on a body. So the front end
    /// calls this directly for its flycam key, gated on the one condition only it can read: that
    /// nothing has claimed the keyboard. This side gates on the other: there is no body, which is
    /// the only configuration in which the free camera runs at all.
    ///
    /// **What it deliberately does not do is as important as what it does**: it does not touch
    /// [`Self::char_input`], so it can never walk a body.
    pub fn flycam_rise(&mut self, held: bool) {
        if self.has_body() {
            return;
        }
        self.input.up = held;
    }

    /// The residual flycam's "down", held or released. See [`Self::flycam_rise`].
    pub fn flycam_sink(&mut self, held: bool) {
        if self.has_body() {
            return;
        }
        self.input.down = held;
    }

    /// Whether the scene has a body. `WorldScene::character.is_some()`.
    fn has_body(&self) -> bool {
        self.world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .is_some()
    }

    /// The do-motion/stop-motion tail for the commands that are on no command list: the four
    /// stances and the 87 emotes. The emote hash is
    /// [`crate::actions::emote::INPUT_ACTION_COMMANDS`], and
    /// [`crate::character::MovementCommands::take_transient_motions`] is drained here into the
    /// body's motion driver. That is what the movement-command tail does with a command on no list.
    ///
    /// The queue is taken **unconditionally** so that the counter behind it moves even when no
    /// body exists to play the motion: *the key produced nothing* and *the key produced a motion
    /// nobody could play* are different findings, and a `take` inside the `if let` could not tell
    /// them apart.
    ///
    /// It is a method rather than a block at the foot of [`Self::apply_input_actions`], because
    /// that foot is below two early returns and a typed `*wave*` arrives on a frame with no input
    /// events at all.
    fn play_transient_motions(&mut self) {
        let transient = self.movement.take_transient_motions();
        if transient.is_empty() {
            return;
        }
        if let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) {
            for (cmd, start) in transient {
                c.command_motion(start, dereth_animation::MotionCommand(cmd));
            }
        }
    }

    /// The input seam for movement and for the camera's look controls.
    ///
    /// Input dispatch gives the winning map's callback first refusal and then walks the
    /// input-handler list; the movement interpreter and camera controller are two entries in
    /// that list. This is their dispatch point. Everything that reaches it has already been
    /// through step 6 — the device input's map walk, which is where the `MAP_BLOCK_KEYBOARD`
    /// barrier `break`s — so **the barrier is load-bearing for movement**.
    ///
    /// The two decoders are [`crate::actions::movement::on_action`] and
    /// [`crate::actions::camera::on_action`]; this is their caller.
    ///
    /// It runs immediately after UI input dispatch (step 7), where the UI has
    /// already taken what it consumed and handed the rest back, and **before** `WorldScene::update`
    /// reads `char_input`, so a key pressed this frame moves the body this frame.
    fn apply_input_actions(&mut self, shell: &mut S) {
        self.events.push(FrameEvent::JumpUseTime);
        // **This must run before the two early returns below.**
        //
        // A typed pose plays through the command interpreter's motion entry point, the same one
        // the emote input-action hash's 91
        // keys reach through. So a typed `*wave*` and the `J` key put the
        // identical command into the identical list bookkeeping, and the `take_transient_motions`
        // drain at the foot of this function plays both.
        //
        // Placed above `let Some(shell) = ...` and above the `events.is_empty()` return because a
        // pose arrives from the **chat** dispatch, not from an input event: a frame where the
        // player typed and pressed nothing has no events at all, and that is the frame a pose
        // most often lands in -- and both of those returns are above the drain at the foot, so a
        // pose fed here and left for that drain would sit in the queue for ever. It is therefore
        // fed **and played** in one block, which is also the order retail has: the pose issues the
        // command and its motion tail runs inside that same call.
        let poses = self.interaction.take_pose_motions();
        if !poses.is_empty() {
            for cmd in poses {
                // Movement parameters are built by the movement command; the interpreter receives
                // the command and its start flag. `extent` is `None` because
                // keyboard-command handling reads it only for `AutoRun`, which no pose can be.
                let c = crate::actions::movement::CmdStruct {
                    command: cmd,
                    start: Some(true),
                    extent: None,
                };
                let m = crate::actions::movement::MovementAction::SetMotion(c);
                if self.movement.on_action(m, &mut self.char_input) {
                    self.events.push(FrameEvent::PoseMotionIssued);
                }
                // A pose reaches `add_command` through the identical bookkeeping, so it
                // can raise the identical edge. The 87 emotes are `0x13……` and carry neither
                // `0x40000000` nor a list, so in practice this drains nothing; the four stances
                // the emote input-action hash also carries are `0x41……` and do raise it, and
                // retail's stance change does abort an automatic attack. Drained per command so a
                // frame with no input events cannot leave one queued for the next one.
                self.abort_automatic_attack_on_new_forward_movement();
            }
            self.play_transient_motions();
        }
        let events = self.actions.take();
        if events.is_empty() {
            return;
        }
        // **The second operand of `set_hold_run`'s XOR.**
        //
        // `set_hold_run` reads the toggle-run option on **every** call and XORs the physical key
        // against it. `ToggleRun` is default-on, so on a shipped character
        // `effective_run = !hold_run`: run is the default and holding the key walks you.
        //
        // [`crate::character::MovementCommands::ui_toggles_run`] carries that operand; left
        // unassigned it is `false` and the polarity is inverted in a running client. This is the
        // assignment. It sits here, once per frame ahead of the dispatch loop, because
        // every path that can reach `set_hold_run` — the `HoldRun` keyboard command and the
        // `SetHoldRun` action — goes through that loop, so a per-frame refresh and the client's
        // live option read cannot disagree within a frame.
        self.movement.ui_toggles_run = self.objects.world.player_system.options.toggle_run();
        let mut declined = Vec::with_capacity(events.len());
        for e in events {
            // The client UI system's complete first Escape leg returns before a following input
            // event. Leaving it in the declined end-of-batch queue would let an old Space-up
            // launch before cancellation. Non-jump Escape remains with its existing owner.
            if e.id.0 == crate::interaction::action::ESCAPE_KEY
                && self.interaction.try_finish_jump_from_escape(
                    &mut self.objects.world,
                    dereth_primitives::LocalTime(self.timer.cur_time),
                    self.world.as_ref().and_then(|w| w.character.as_ref()),
                )
            {
                self.events
                    .push(FrameEvent::ActionRouted(ActionRoute::EscapeFinishedJump));
                self.deliver_jump_power_bar_notices(shell);
                continue;
            }
            // The emote action-to-command table — see [`crate::actions::emote`] for why `|_| None`
            // here would close the four stance keys and all 87 emotes. The hash is the
            // interpreter's member and this is the interpreter's dispatch, so the table is passed
            // rather than duplicated.
            let m =
                crate::actions::movement::on_action(&e, crate::actions::emote::command_for_action);
            if self.apply_jump_action(shell, m) {
                self.events
                    .push(FrameEvent::ActionRouted(ActionRoute::Jump));
                continue;
            }
            if self.movement.on_action(m, &mut self.char_input) {
                self.events
                    .push(FrameEvent::ActionRouted(ActionRoute::Movement));
                // Accepting a new movement substate or listless motion invokes the movement
                // override, which performs two calls: `set_auto_run(0)`, already handled by
                // the movement handler, and the automatic-attack cancellation that needs the combat
                // system. This is that second call, and it is here -- inside the loop, on the
                // event that raised the edge -- because retail's is synchronous with the key: the
                // attack cancellation leaves before `move_player` has finished moving the body, in
                // the same `handle_keyboard_command`. Drained through `MovementCommands` for the
                // same one-borrow-at-a-time reason the retake and the notices below are.
                self.abort_automatic_attack_on_new_forward_movement();
                continue;
            }
            let cmd = crate::actions::camera::on_action(&e);
            if Self::apply_camera_action(&mut self.input, cmd) {
                self.events
                    .push(FrameEvent::ActionRouted(ActionRoute::Camera));
                continue;
            }
            // The other eight camera commands, which need the world.
            if self.apply_world_camera_action(shell, cmd) {
                self.events
                    .push(FrameEvent::ActionRouted(ActionRoute::WorldCamera));
                continue;
            }
            declined.push(e);
        }
        self.actions.put_back(declined);
        // `handle_keyboard_command` unconditionally requests `take_control_from_server` after
        // accepting a command. The interpreter half runs inside the dispatch loop above; this is
        // the body operation it cannot reach, and it is the
        // same call [`Self::command_interpreter_control_transfer`] makes when the per-frame step
        // retakes instead. Drained here, after the loop, for the reason the notices below are: one
        // borrow at a time, and a press cannot be answered twice.
        if self.movement.take_control_retake_pending() {
            if let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) {
                c.take_control_from_server();
            }
        }
        // The movement-command interpreter's do-motion/stop-motion tail for the
        // commands that are on no command list — the four stances and the 87 emotes. The queue is
        // taken unconditionally so that the counter behind it moves even when no body exists to
        // play the motion: *the key produced nothing* and *the key produced a motion nobody could
        // play* are different findings, and a `take` inside the `if let` could not tell them
        // apart.
        self.play_transient_motions();
        // Toggling auto-run selects `L"AutoRun ON"` or `L"AutoRun OFF"` and sends it as chat type
        // `0x1A`. That line is the only feedback a player gets that
        // `DIK_Q` did anything, and `MovementCommands::on_action` had nowhere to put it.
        //
        // This is the scroll-text surface and not a second road to the chat window: the channel is
        // the chat **type**, and `0x1A` is the one type the main window's default filter
        // `0xFBFFFFFF` drops (bit 26 clear). It draws in the spew panel's strip, which accepts
        // `0x1A` and nothing else. `Hud::drain_scroll` fans it out to both, each with its own test,
        // exactly as the final-string-info notice does.
        for text in self.movement.take_notices() {
            self.objects
                .world
                .scroll
                .on_display_string_info(dereth_client_model::scroll::LOCAL_ERROR_TYPE, text);
        }
    }

    /// The movement handler's combat half.
    ///
    /// The base call is the run lock, which [`crate::actions::movement::CommandLists::add_command`]
    /// performs; this function supplies the automatic-attack abort. It is **state-gated** — *"is an attack
    /// pending, in progress, being requested, or repeating?"* — so an ordinary walk with no attack
    /// running sends nothing, and this function faithfully asks the same question by calling the
    /// gate rather than reimplementing it. The world's automatic-attack abort also has
    /// the `Escape` and selection-change callers; without this caller on this edge, a player
    /// could not break a sticky repeated melee by walking backwards.
    ///
    /// The request goes out through [`crate::interaction::send_request`], the one place a
    /// [`dereth_client_model::Request`] becomes bytes, so the `0x01B7` carries an `OrderedActionHeader` stamp from the
    /// same global counter as every other game action and leaves in this input frame — ahead of
    /// `interaction_use_time`'s own flush, exactly as retail's leaves inside
    /// `handle_keyboard_command`. With no link there is nothing to send to and the state change
    /// still happens, which is what the client does while disconnected.
    fn abort_automatic_attack_on_new_forward_movement(&mut self) {
        if !self.movement.take_new_forward_movement() {
            return;
        }
        self.events.push(FrameEvent::NewForwardAttackAborted);
        let mut req = dereth_client_model::RecordingRequests::default();
        self.objects.world.abort_automatic_attack(&mut req);
        for r in req.0 {
            self.events.push(FrameEvent::NewForwardAttackCancelSent);
            if let Some(link) = self.link.as_mut() {
                let _ = crate::interaction::send_request(&mut link.net.session, &r);
            }
        }
    }

    /// `(edges consumed, `0x01B7`s the state gate let through)`.
    #[must_use]
    pub const fn new_forward_attack_aborts(&self) -> (u64, u64) {
        (
            self.events.total(FrameEventKind::NewForwardAttackAborted),
            self.events
                .total(FrameEventKind::NewForwardAttackCancelSent),
        )
    }

    /// How many `0x01A1`s [`Self::log_off_character`]'s player-module save has
    /// sent. The unforced `save_to_server`'s gate is
    /// dirty-or-forced, so this counts *dirty* logouts, not logouts.
    #[must_use]
    pub const fn player_modules_saved_at_logout(&self) -> u64 {
        self.events.total(FrameEventKind::PlayerModuleSavedAtLogout)
    }

    /// Target tracking: the camera update routine, raised on the three edges retail raises it
    /// on.
    ///
    /// The gates themselves are [`crate::camera::update_target_tracking`]; this is the edge
    /// detection, and the triple is exactly `(tracking_target, combat_mode, selected_id)` — the
    /// three words retail's three call sites each change one of. See
    /// [`Self::last_target_tracking`].
    ///
    /// `ViewCombatTarget` is option ordinal 7, and `dereth_client_model::player`'s
    /// `OptionSideEffect::TrackTarget` is the producer `Interaction` counts as
    /// *"decided and not applied"* in `option_side_effects_unapplied`. That counter keeps counting:
    /// the option's bit is read here, from the model, rather than the side effect being consumed,
    /// because the bit is the state and the side effect is only the edge — and reading the state
    /// also gets the value right after a whole-module `0x01A1` load, which raises no side effect at
    /// all.
    fn update_target_tracking(&mut self) {
        let tracking = self
            .objects
            .world
            .player_system
            .options
            .get(dereth_client_model::player::option::VIEW_COMBAT_TARGET);
        let now = (
            tracking,
            self.objects.world.combat.combat_mode,
            self.objects.world.selected,
        );
        if self.last_target_tracking == Some(now) {
            return;
        }
        let Some(mut world) = self
            .present
            .scene_mut(self.world.as_mut())
            .filter(|_| self.pending_scene.is_none())
        else {
            // No scene means no camera set, so retail returns
            // having touched nothing. **The latch is deliberately not advanced**, so the first
            // frame that does have a body applies the state the edge was about — otherwise an
            // option set or a mode entered while a scene was still loading would be lost for the
            // rest of the session.
            return;
        };
        crate::camera::update_target_tracking(&mut *world, &self.objects.world, tracking);
        self.last_target_tracking = Some(now);
    }

    /// Apply player-module initialization and the three **environment** arms of its option-change
    /// switch.
    ///
    /// The option-change switch, indexed by `option - 2`, has five live arms; two of
    /// them are the fellowship mutual exclusion that `dereth_client_model::player` already owns, one is
    /// `ViewCombatTarget` which [`Self::update_target_tracking`] owns, and the remaining **three**
    /// are these:
    ///
    /// `PersistentAtDay` applies the option directly to the landscape; `DisableMostWeatherEffects`
    /// passes its inverse to weather enablement; `DisableDistanceFog` stores its inverse in the
    /// landscape fog-enabled field. The order is significant and retained.
    ///
    /// Player-module initialization runs the identical three (plus `ViewCombatTarget`) in the same
    /// order when the `PlayerModule` first arrives, which is the login edge.
    ///
    /// # Why this is a latch and not a side-effect consumer
    ///
    /// `dereth_client_model::player::OptionSideEffect` already names all four calls, and
    /// `interaction.rs`'s `UiRequest::SetPlayerOption` arm **counts** them into
    /// `option_side_effects_unapplied` without applying any, because the arm holds no scene; so
    /// the apply is here, in the frame, as an edge detector over the three bits.
    ///
    /// Reading the **state** rather than consuming the **edge** is deliberate and is the same
    /// choice [`Self::update_target_tracking`] documents: a whole-module load (`0x0013` at login,
    /// or a `0x01A1` round trip) raises no option-change call at all, and retail covers that case
    /// with its initialization pass. One latch over the three bits reproduces both producers
    /// exactly, and nothing else in the process writes the three scene fields.
    ///
    /// `option_side_effects_unapplied` keeps counting, for the reason target tracking gives: the
    /// counter is
    /// about the *edge* the interaction arm declined, and this is the *state*.
    fn apply_player_option_effects(&mut self) {
        let o = &self.objects.world.player_system.options;
        let now = (
            // `PersistentAtDay` -- the second option word's bit 0.
            o.get(dereth_client_model::player::option::PERSISTENT_AT_DAY),
            // `DisableMostWeatherEffects` -- the first option word's bit 16.
            o.get(dereth_client_model::player::option::DISABLE_MOST_WEATHER_EFFECTS),
            // `DisableDistanceFog` -- the second option word's bit 21.
            o.get(dereth_client_model::player::option::DISABLE_DISTANCE_FOG),
        );
        if self.last_option_environment == Some(now) {
            return;
        }
        let Some(mut world) = self
            .present
            .scene_mut(self.world.as_mut())
            .filter(|_| self.pending_scene.is_none())
        else {
            // No scene, no landscape. The latch is deliberately **not** advanced, for exactly the
            // reason `update_target_tracking`'s is not: an option set while a scene was still
            // loading would otherwise be lost for the rest of the session.
            return;
        };
        let (day, no_weather, no_fog) = now;
        world.set_always_daylight(day);
        world.set_weather_enabled(!no_weather);
        world.set_world_fog(!no_fog);
        self.last_option_environment = Some(now);
    }

    /// Apply the camera's turn decision to the body.
    ///
    /// `CameraEffects`' turn-player and stop-turn fields are written by `Rotate`; this is their
    /// reader. Without it **turning in first person, and turning with `Input.UseMouseTurning` on,
    /// reaches no body at all**. Retail dispatches three different command-interpreter actions:
    /// move the player, stop drift, and turn to a heading. Each behavior is transcribed on
    /// [`crate::character::Character`]; this is the wire between them.
    ///
    /// Retail makes the call from inside `Rotate`, i.e. from inside
    /// the gameplay UI's camera update, which is *before* physics in retail.
    /// Here the camera's frame runs after `WorldScene::update`
    /// (its placement is measured by the sweep-lag tests), so a turn a held key repeats lands
    /// on the body in the same frame — `do_motion` applies to motion interpolation immediately — and
    /// is stepped by the *next* physics tick rather than this one. That is the same measured
    /// one-tick offset the camera placement already has, not a new one.
    ///
    /// Sending a movement event has the mouse-turning arm's own 0.5 s throttle, which is
    /// **not** [`dereth_client_net::client_session::PositionReporter`]'s change detector: retail runs both, and this
    /// does too.
    fn apply_camera_turn(&mut self, now: dereth_primitives::LocalTime) {
        let Some(mut world) = self
            .present
            .scene_mut(self.world.as_mut())
            .filter(|_| self.pending_scene.is_none())
        else {
            return;
        };
        let Some(c) = world.world_mut().character.as_mut() else {
            return;
        };
        let (turn, send) = c.camera.take_turn();
        if turn.is_none() && !send {
            return;
        }
        match turn {
            Some(crate::camera::CameraTurn::TurnToHeading { heading }) => {
                c.turn_to_heading(heading);
            }
            Some(crate::camera::CameraTurn::MovePlayer { command, extent }) => {
                c.camera_turn_motion(dereth_animation::MotionCommand(command), extent);
            }
            Some(crate::camera::CameraTurn::StopDrift) => c.stop_drift(),
            None => {}
        }
        drop(world);
        if turn.is_some() {
            self.events.push(FrameEvent::WorldCameraTurnApplied);
        }
        if send {
            if let Some(motion) = self.player_motion() {
                if let Some(link) = self.link.as_mut() {
                    self.position
                        .send_movement_event(now.0, &motion, &mut link.net.session);
                }
            }
        }
    }

    /// How many body turns [`Self::apply_camera_turn`] took off the camera and gave
    /// to the command interpreter this session.
    #[must_use]
    pub const fn camera_turns_applied(&self) -> u64 {
        self.events.total(FrameEventKind::WorldCameraTurnApplied)
    }

    /// The eight `CameraCommand`s [`Self::apply_camera_action`] declines.
    ///
    /// `crate::camera::CameraControl::on_action` implements every one of them — the two zooms,
    /// the three view modes, `SetDefaultOffsets` and the two mouse-look toggles. Its other caller,
    /// [`crate::camera::CameraControl::apply_input`], can construct exactly six of its twelve
    /// commands: `Rotate`, `StopRotating`, `Raise`, `StopRaising`, `Lower` and `StopLowering`.
    /// Reaching the other eight needs the world, which `apply_input_actions` does not take; this
    /// is that plumbing.
    ///
    /// The four *rotation* commands are deliberately **not** here: they stay on
    /// [`Self::apply_camera_action`]'s held-key flags, which the camera update repeats every frame
    /// and the body-less flycam also reads. Routing them
    /// through both would apply each press twice.
    ///
    /// Action `0x3E` is the one with a second half: it registers input map **6** at the
    /// unfocused-UI input priority while it is held and unregisters it on the release, *then* falls
    /// through to `0x3D`'s `ToggleMouseLook`. That registration is why it is here rather than in the
    /// world-free arm — and it is done even when there is no body, because the map stack is the
    /// input manager's and exists in every build.
    fn apply_world_camera_action(
        &mut self,
        shell: &mut S,
        cmd: crate::actions::camera::CameraCommand,
    ) -> bool {
        use crate::actions::camera::CameraCommand as C;
        match cmd {
            C::Closer { .. }
            | C::StopCloser
            | C::Farther { .. }
            | C::StopFarther
            | C::SetDefaultOffsets
            | C::SetInHead
            | C::ToggleLookDown
            | C::ToggleMapMode
            | C::ToggleMouseLook(_) => {}
            C::AlternateMode { on } => {
                shell.control_notice(crate::shell::ControlNotice::AlternateCamera(on));
            }
            // The four rotations, and `NotHandled`.
            _ => return false,
        }
        let now = self.timer.cur_time;
        self.with_camera(|camera, player| camera.on_action(cmd, player, now))
    }

    /// Run `f` against the body's [`crate::camera::CameraControl`] and its object id, answering
    /// whether there was a body to run it against.
    ///
    /// Without a body there is no `CameraState` at all — the free camera is a debug flycam with no
    /// zoom, no first person and no map mode — so the eight commands are consumed and do nothing,
    /// exactly as they would with a null body and the camera handler's own active guard failing.
    fn with_camera(
        &mut self,
        f: impl FnOnce(&mut crate::camera::CameraControl, dereth_primitives::ObjectId),
    ) -> bool {
        let Some(c) = self.world.as_mut().and_then(|w| w.character.as_mut()) else {
            return true;
        };
        let id = c.object_id();
        f(&mut c.camera, id);
        true
    }

    /// The four look controls of the camera action set, applied to the held-key
    /// flags both cameras read.
    ///
    /// `CameraState` repeats a held rotation every frame from `update_camera` rather than acting once
    /// per event, which is what these four bools are: with a body they are
    /// [`crate::camera::CameraControl::apply_input`]'s input and become `Rotate`/`Raise`/`Lower`
    /// again on the far side; without one they are the flycam's yaw and pitch.
    ///
    /// The other **eight** commands the decoder produces — the two zooms, the three view modes,
    /// `SetDefaultOffsets` and the two mouse-look toggles — need the world, and are routed
    /// through [`Self::apply_world_camera_action`] instead.
    fn apply_camera_action(
        input: &mut CameraInput,
        cmd: crate::actions::camera::CameraCommand,
    ) -> bool {
        use crate::actions::camera::CameraCommand as C;
        match cmd {
            C::Rotate { left: true, .. } => input.look_left = true,
            C::StopRotating { left: true } => input.look_left = false,
            C::Rotate { left: false, .. } => input.look_right = true,
            C::StopRotating { left: false } => input.look_right = false,
            C::Raise { .. } => input.look_up = true,
            C::StopRaising => input.look_up = false,
            C::Lower { .. } => input.look_down = true,
            C::StopLowering => input.look_down = false,
            _ => return false,
        }
        true
    }

    /// How many input actions [`Self::apply_input_actions`] routed into the body or
    /// the camera this session.
    ///
    /// A denominator, not decoration: "typing did not walk the character" is the same observation
    /// as "no action arrived at all", and a test that cannot tell them apart passes on a build that
    /// blocks movement for ever.
    #[must_use]
    pub const fn actions_routed(&self) -> u64 {
        self.events.total(FrameEventKind::ActionRouted)
    }

    /// Device completion is the single quit switch. Every documented exit path routes
    /// through it: `WM_CLOSE`/`WM_DESTROY`, `SC_CLOSE`, the `Exit`/`Quit` console commands, and the
    /// epilogue UI's confirmation.
    pub fn done(&mut self) {
        self.pump.done();
    }

    /// What the last frame did, in call order. The frame-order test reads this.
    #[must_use]
    pub fn last_frame_steps(&self) -> &[FrameStep] {
        FrameRecorder::new(&self.events).steps()
    }

    #[must_use]
    pub fn state(&self) -> AppState {
        self.state
    }

    #[must_use]
    pub const fn frames_drawn(&self) -> u64 {
        self.events.total(FrameEventKind::FrameDrawn)
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// The device state, read-only — including the full-screen display preference, which is
    /// what Alt+Enter and the `Display.FullScreen` option both write and what
    /// [`Self::change_presentation`] applies.
    #[must_use]
    pub fn device_state(&self) -> &dereth_client_contract::window_proc::DeviceState {
        &self.pump.state
    }

    /// The clock's local/server times and the server offset behind them, read-only.
    ///
    /// Exists so a test can tell a clock that *rejected* a stale `TimeSync` from a map panel that
    /// merely did not redraw inside its five-second throttle. Those are two different builds and
    /// the rendered date alone cannot separate them.
    #[must_use]
    pub fn clock(&self) -> &Clock {
        &self.timer
    }

    /// The presentation this `App` was built with.
    ///
    /// A headless harness reads its [`crate::present::NullPresentation`] counters back through
    /// this.
    #[must_use]
    pub fn presentation(&self) -> &S::Present {
        &self.present
    }

    /// The world state this `App` owns: the body, the server objects' simulation,
    /// the residency window, the camera and the clock. `None` before a world is loaded, and always
    /// with a presentation that has no device.
    #[must_use]
    pub fn world_state(&self) -> Option<&crate::world_state::WorldState> {
        self.world.as_ref()
    }

    /// …and writable.
    pub fn world_state_mut(&mut self) -> Option<&mut crate::world_state::WorldState> {
        self.world.as_mut()
    }

    /// Have the presentation load a world, straight away and outside the frame:
    /// what the device tests did through the renderer, with the world state landing here.
    ///
    /// # Errors
    /// As [`crate::present::Presentation::load_world`].
    pub fn load_world(
        &mut self,
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        cfg: crate::scene::SceneConfig,
    ) -> Result<(), crate::landblock::WorldError> {
        self.present.load_world(store, cfg, &mut self.world)
    }

    /// The presentation's world draw on its own, from this `App`'s world state:
    /// what the device tests did through the renderer.
    ///
    /// # Errors
    /// As [`crate::present::Presentation::draw_scene`].
    pub fn draw_world_scene(&mut self) -> Result<(), crate::present::PresentError> {
        self.present.draw_scene(self.world.as_ref())
    }

    /// The window title bar, for the hand check of the windowed path.
    pub fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }

    /// Run the normal cleanup path.
    ///
    /// The order is [`crate::shutdown::Step::ORDER`]; the recording it returns is what
    /// `tests/gpu/presentation/shutdown.rs` asserts on, because an ordering nothing checks is a comment.
    pub fn shutdown(mut self, shell: &mut S) -> crate::shutdown::CleanupLog {
        use crate::shutdown::{Outcome, Step};
        self.state = AppState::ShuttingDown;
        let mut log = crate::shutdown::CleanupLog::default();

        // What the position reporter produced this session, with its denominator.
        // A driven run has to be able to say how many position reports actually left the client,
        // because replay anchors cannot see emission at all and
        // "it looked like it worked" is not a count.
        let p = self.position.stats;
        tracing::debug!(
            "position reporter -- {} x 0xF753, {} x 0xF61C, {} gated, \
             {} encode failures, over {} frame(s)",
            p.position_events,
            p.movement_events,
            p.position_events_gated,
            p.encode_failures,
            self.position_use_times()
        );

        // Detach UI preferences before chaining to the base cleanup.
        log.push(Step::DetachUiPreferences, Outcome::NotInThisBuild);
        // Save the key map when it was loaded and its configured filename
        // is non-empty. It is what makes a rebound key survive the session:
        // keymap serialization writes the **full merged map**, defaults
        // included, and the key-map merge reads it back first on the next run so it wins.
        //
        // `Outcome::Nothing` is the client's own skip when that filename is empty — a `Config`
        // with no preferences file, which is every test that does not ask for one.
        log.push(Step::SaveKeyMap, shell.save_bindings());

        // 1. Disconnect the active session and release its network client.
        //
        // **This step is both halves, not `Session::log_off` alone.** `Session::log_off` sends
        // `0xF653`, a *message* about a *character*, while transport log-off sends
        // a *packet* about the *connection*. The retail client does both, in that order
        // (epilogue-screen construction logs off a selected character before the
        // process exit disconnects from the server either way). Without the transport half an
        // EXIT from **character select**, where `Flow::log_off` emits nothing because no character
        // is selected, leaves without a word and ACE falls back on its 60-second timeout.
        //
        // The count is reported rather than assumed: `Outcome::Ran` means a datagram the OS
        // accepted, and a goodbye that was built and refused reads as `Nothing` with a line saying
        // so. The client-net disconnect tests pin the bytes.
        log.push(
            Step::Disconnect,
            match self.link.as_mut() {
                Some(link) => {
                    link.net.session.log_off();
                    let (sent, queued) = link.log_off_server();
                    if sent == 0 && queued > 0 {
                        tracing::warn!(
                            "the Disconnect was built for {queued} connection(s) and \
                             none of them was written to the socket"
                        );
                        Outcome::Nothing
                    } else {
                        tracing::info!("0x8000 Disconnect sent to {sent} connection(s)");
                        Outcome::Ran
                    }
                }
                None => Outcome::Nothing,
            },
        );
        // 2. Communication cleanup — chat and Turbine Chat, neither of which exists.
        log.push(Step::CommunicationSystem, Outcome::NotInThisBuild);
        // 3. Network cleanup: the packet controller, the client network object, and the twelve
        //    queues.
        log.push(
            Step::CleanupNet,
            if self.link.take().is_some() {
                Outcome::Ran
            } else {
                Outcome::Nothing
            },
        );
        // 4. Language-information shutdown.
        log.push(Step::LanguageInfo, Outcome::NotInThisBuild);
        // 5. UI cleanup: the flow and every root element it holds, then the element manager.
        //    **Before the database**, because UI elements hold dat objects.
        shell.cleanup_ui(&mut self);
        log.push(Step::CleanupUi, Outcome::Ran);
        // 6. Preference cleanup, whose last two operations are
        //    save preferences, then run the remaining preference cleanup.
        //
        //    Writing the profile back is retail, not a liberty with a retail install's
        //    `UserPreferences.ini`. Both client layers perform the same preference cleanup,
        //    whose whole body unregisters three preferences, performs
        //    IME preference cleanup, **saves preferences**,
        //    and remaining preference cleanup — so retail writes the file on every normal exit, and
        //    a build that did not would lose every option the player changed during the session.
        //    (Contrast saving the screen layout, which retail really does *not* call at shutdown;
        //    that one would be a deviation.)
        //
        //    `store::save_into` is a **merge**, because the save's one write call is
        //    a Win32 profile write: the file is read first and every key this build does not
        //    register — `Net.*`, `Input.KeymapFile`, anything a later client added — survives
        //    untouched.
        log.push(Step::CleanupPreferences, {
            let path = self.cfg.preferences_file.clone();
            if path.as_os_str().is_empty() || dereth_client_contract::options::store::len() == 0 {
                // The preferences-file path empty, or a registry preference load never filled —
                // which is every test that does not ask for a profile. The client's own skip.
                Outcome::Nothing
            } else {
                let mut ini = crate::platform::files::read_to_string(&path)
                    .ok()
                    .and_then(|t| {
                        dereth_client_contract::persist::preferences::UserPreferences::parse(&t)
                            .ok()
                    })
                    .unwrap_or(
                        dereth_client_contract::persist::preferences::UserPreferences {
                            // `fopen(path, "w")` is text mode on Windows: a file the client wrote has
                            // CRLF, and a first run has to start that way too.
                            crlf: true,
                            ..Default::default()
                        },
                    );
                let n = dereth_client_contract::options::store::save_into(&mut ini);
                match crate::platform::files::write(&path, ini.to_text()) {
                    Ok(()) => {
                        tracing::info!("saved {n} preferences to {}", path.display());
                        Outcome::Ran
                    }
                    Err(e) => {
                        tracing::warn!("saving user preferences failed: {e}");
                        Outcome::Nothing
                    }
                }
            }
        });
        // 7-9. The quality registrar, the global event handler and the interface system.
        log.push(Step::DeleteGlobals, Outcome::NotInThisBuild);
        // 10. Clean up the database, closing every data-file controller. The store is
        //     an `Arc` and other holders are gone by now; dropping the app's reference is the close.
        log.push(Step::CleanupDatabase, Outcome::Ran);
        // 11. Clean up the sound manager.
        let audio = self.audio.take();
        log.push(
            Step::SoundManager,
            if audio.is_some() {
                Outcome::Ran
            } else {
                Outcome::Nothing
            },
        );
        drop(audio);
        // Releasing the client destroys the game client and then the client. The single-instance semaphore has no
        // counterpart: this build takes no single-instance lock, which is why two of them can run (client
        // divergence CD-008).
        log.push(Step::ReleaseClient, Outcome::NotInThisBuild);
        // Last, and not the client's: every release above must be observed before the device goes.
        log.push(
            Step::WaitIdle,
            match self.present.wait_idle() {
                Ok(()) => Outcome::Ran,
                Err(e) => {
                    tracing::warn!("wait_idle failed: {e}");
                    Outcome::Nothing
                }
            },
        );
        log
    }
}

/// The four sequences every outbound movement pack echoes back, off the object
/// table.
///
/// `send_movement_event` and `send_position_event` both pass
/// `update_times[8], [5], [4], [6]` — the instance, server-controlled-move, teleport and
/// force-position timestamps — in that argument order. They are the *server's* numbers and live on
/// [`crate::objects::Presence`], not on the body, which is why [`body_motion`] takes them rather
/// than reaching for them.
///
/// Free and `pub` for the same reason `body_motion` is: this is the only route from the object
/// table to the wire, so a test that asserts the echo has to be able to call exactly what
/// `App::player_motion` calls. Wiring it to the wrong slot would otherwise be invisible to
/// everything except a driven session.
#[must_use]
pub fn player_timestamps(
    presence: Option<&crate::objects::Presence>,
) -> dereth_protocol::movement::MoveTimestamps {
    dereth_protocol::movement::MoveTimestamps {
        instance: presence.map_or(0, |p| p.instance),
        server_control: presence.map_or(0, |p| p.server_control_ts),
        teleport: presence.map_or(0, |p| p.teleport_ts),
        force_position: presence.map_or(0, |p| p.force_position_ts),
    }
}

/// Legacy isolated/debug ground-edge observer, retained for the jump body tests. Actual App input
/// uses the direct result and immediate physical velocity in crate::jump;
/// it does not infer acceptance from a later physics edge.
#[must_use]
pub fn jump_edge_sample(character: &crate::character::Character) -> u64 {
    character.ground_edges().left
}

#[must_use]
pub fn body_jump(
    character: &crate::character::Character,
    left_before: u64,
) -> (bool, dereth_protocol::types::Vec3) {
    let accepted = character.ground_edges().left > left_before;
    let rotation = character.position().frame.rotation;
    let global = character
        .world
        .get(character.handle)
        .map_or(dereth_primitives::Vec3::ZERO, |o| o.velocity_vector);
    let local = dereth_physics::math::globaltolocalvec(dereth_physics::math::l2g(rotation), global);
    (
        accepted,
        dereth_protocol::types::Vec3 {
            x: local.x,
            y: local.y,
            z: local.z,
        },
    )
}

/// The control transfer between the server and
/// [`crate::character::MovementCommands`], both halves, once per frame.
///
/// Lose control to the server when the position update returns 1, then
/// apply the command interpreter's control-retake decision and, on it,
/// `take_control_from_server`'s body half. Returns `(lost, retook)`.
///
/// **Free and `pub` for the same reason [`apply_player_teleport`] and [`body_motion`] are**: this
/// is the application's own path, so a test that drives a real body over a real capture must be
/// able to call exactly what [`App::command_interpreter_control_transfer`] calls, without a
/// device, a window or a link. Without that the only observable is the call counter, and a
/// mutation that empties the body of the step survives it (see
/// [`App::player_teleport_use_time`]).
///
/// The order inside is retail's and is load-bearing: the loss must precede the retake, because
/// the per-frame step's first term is `controlled_by_server` and `lose_control_to_server` is what
/// sets it. With no body there is no retake at all: the per-frame step returns before sampling
/// any other gate.
pub fn command_interpreter_control_transfer(
    scene: Option<&mut (dyn crate::present::SceneMut + '_)>,
    movement: &mut crate::character::MovementCommands,
    input: &mut crate::character::CharacterInput,
) -> (bool, bool) {
    command_interpreter_control_transfer_with_finish(scene, movement, input, |character| {
        if let Some(character) = character {
            character.finish_jump();
        }
    })
}

/// App-owned finish_jump at the accepted loss boundary; compatibility callers above have no
/// combat model but still clear the actual body before the retake gate is sampled.
fn command_interpreter_control_transfer_with_finish(
    scene: Option<&mut (dyn crate::present::SceneMut + '_)>,
    movement: &mut crate::character::MovementCommands,
    input: &mut crate::character::CharacterInput,
    finish_jump: impl FnOnce(Option<&crate::character::Character>),
) -> (bool, bool) {
    let Some(scene) = scene else {
        return (false, false);
    };
    // The smart box's event dispatch loses control to the server when setting the object's
    // movement returns non-zero, taken
    // off the latch `world.rs::apply_player_movement` set on this frame's dispatch.
    let lost = scene.world_mut().take_player_movement_applied();
    if lost {
        movement.lose_control_to_server_with_finish(input, || finish_jump(scene.character()));
    }
    // Sample `motions_pending` and `is_moving_to` from the local body. They are copied into a pair
    // so the scene borrow is released before the retake, which
    // takes the same body mutably.
    let Some(gates) = scene.character().map(|c| {
        let d = c.driver();
        (d.movement.motions_pending(), d.movement.is_moving_to())
    }) else {
        return (lost, false);
    };
    if !movement.use_time(gates.0, gates.1, input) {
        return (lost, false);
    }
    if let Some(c) = scene.world_mut().character.as_mut() {
        c.take_control_from_server();
    }
    (lost, true)
}

/// Apply a pending server teleport to the player's own body. With no player the
/// operation is a no-op; otherwise it sets the position and runs the player-position-updated tail.
///
/// [`crate::character::Character::teleport`] **is** the client's simple position set with its
/// teleport flag — it normalises an outdoor cell, re-enters the cell, restarts the object's clock
/// and resets the camera smoother, all of which that position set's `set_frame`/`enter_cell` path
/// does. This is the **call**.
///
/// The implemented player-position-updated (teleport) tail now performs teleport_hook's movement,
/// sticky and target cleanup. The command-interpreter tail is composed by
/// [`complete_player_teleport`]. Interpolation, constraints, voyeur notifications and complete
/// collision-end bookkeeping remain explicit Character-side limitations, not audio-only work.
///
/// Free and `pub` for the same reason [`body_motion`] and [`player_timestamps`] are: this is the
/// application's own path from the object stream to the body, so a test that asserts the body
/// moved has to be able to call exactly what [`App::player_teleport_use_time`] calls. Returns the
/// destination when a teleport was pending, `None` otherwise.
pub fn apply_player_teleport(
    objects: &mut crate::objects::ObjectStream,
    character: &mut crate::character::Character,
) -> Option<dereth_primitives::Position> {
    let pos = objects.take_player_teleport()?;
    apply_player_teleport_at(pos, character);
    Some(pos)
}

fn apply_player_teleport_at(
    pos: dereth_primitives::Position,
    character: &mut crate::character::Character,
) {
    // **This build's prerequisite, not retail's.** The landscape holds every loaded block and the
    // client's position setting finds the destination cell already there; this build's land
    // source is prefetched per block, and `App::load_pending_scene` prefetches the window it builds
    // (`character.land().load_block_cells(..)`). A teleport moves the body into a block
    // the window has never held, so the same prefetch has to happen here or the body enters a cell
    // with no interior geometry under it. Cheap and idempotent: the source caches per block.
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a cell id's top 16 bits are its landblock. Not a float conversion.
    let block = dereth_primitives::LandblockId((pos.cell.0 >> 16) as u16);
    character.land().load_block_cells(block);
    character.teleport(pos);
    // The player-position-updated (teleport) path calls teleport_hook after the position set. Keep
    // this on the accepted TELEPORT_TS edge, not Character::teleport's initial-placement callers.
    character.player_teleport_hook();
}

/// The accepted teleport chain: teleport the player, update its position, report it teleported.
/// The send tail runs after body cleanup and set_auto_run(0, 1)'s actual raw-motion reapplication,
/// even when autorun was already off. The callback lets App supply its existing session sender
/// and lets headless component tests inspect its real outgoing payload without a live link.
pub fn complete_player_teleport(
    objects: &mut crate::objects::ObjectStream,
    character: &mut crate::character::Character,
    movement: &mut crate::character::MovementCommands,
    input: &mut crate::character::CharacterInput,
    send_movement: impl FnOnce(&crate::character::Character),
) -> Option<dereth_primitives::Position> {
    let pos = objects.take_player_teleport()?;
    complete_player_teleport_at(pos, character, movement, input, send_movement);
    Some(pos)
}

/// One already-accepted teleport, also used by the ordered App dispatcher. The caller supplies
/// the edge's own position and captures its timestamp snapshot in the sender closure.
pub fn complete_player_teleport_at(
    pos: dereth_primitives::Position,
    character: &mut crate::character::Character,
    movement: &mut crate::character::MovementCommands,
    input: &mut crate::character::CharacterInput,
    send_movement: impl FnOnce(&crate::character::Character),
) {
    apply_player_teleport_at(pos, character);
    if movement.player_teleported(input) {
        character.reapply_teleport_input(*input);
    }
    send_movement(character);
}

/// Everything the two senders read off the physics object, as one value.
///
/// Free rather than a method so a test can build a real [`crate::character::Character`] over the
/// retail landblocks and assert this translation without a device, a link or an `App` — the
/// `App` method is one call to it. The four timestamps are the caller's because they are the
/// *server's* sequence numbers and live on the object table, not on the body.
#[must_use]
pub fn body_motion(
    character: &crate::character::Character,
    timestamps: dereth_protocol::movement::MoveTimestamps,
) -> dereth_client_net::client_session::PlayerMotion {
    let pos = character.position();
    let position = dereth_protocol::types::PositionWire {
        objcell_id: pos.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: pos.frame.origin.x,
                y: pos.frame.origin.y,
                z: pos.frame.origin.z,
            },
            orientation: dereth_protocol::types::Quat {
                w: pos.frame.rotation.w,
                x: pos.frame.rotation.x,
                y: pos.frame.rotation.y,
                z: pos.frame.rotation.z,
            },
        },
    };
    // Position validity requires an inbound-valid cell id (transcribed in
    // `dereth_physics::landdefs`, not copied) and seven finite frame components.
    let position_valid = dereth_physics::landdefs::inbound_valid_cellid(pos.cell)
        && ![
            pos.frame.origin.x,
            pos.frame.origin.y,
            pos.frame.origin.z,
            pos.frame.rotation.w,
            pos.frame.rotation.x,
            pos.frame.rotation.y,
            pos.frame.rotation.z,
        ]
        .iter()
        .any(|f| f.is_nan());
    let plane = character
        .world
        .get(character.handle)
        .map(|o| o.contact_plane)
        .unwrap_or_default();
    let (raw_motion_state, longjump_mode) = {
        let driver = character.driver();
        (
            raw_motion_state_to_wire(&driver.movement.interp.raw_state),
            driver.movement.interp.standing_longjump,
        )
    };
    dereth_client_net::client_session::PlayerMotion {
        position,
        position_valid,
        timestamps,
        // `send_position_event` requires both contact and walkable-surface state, exactly the
        // condition represented by `Character::on_ground`.
        contact: character.on_ground(),
        longjump_mode,
        raw_motion_state,
        contact_plane: dereth_client_net::client_session::ContactPlane {
            normal: dereth_protocol::types::Vec3 {
                x: plane.normal.x,
                y: plane.normal.y,
                z: plane.normal.z,
            },
            d: plane.d,
        },
    }
}

/// Pack the player's raw motion state into its wire form.
///
/// Each field's presence bit is decided by comparing it with a default:
///
/// | bit | field | present when |
/// |---:|---|---|
/// | 0 | `current_holdkey` | `!=` the no-hold-key value (1) |
/// | 1 | `current_style` | `!= 0x8000003D` (`NonCombat`) |
/// | 2 | `forward_command` | `!= 0x41000003` (`Ready`) |
/// | 3 | `forward_holdkey` | `!=` the invalid hold key (0) |
/// | 4 | `forward_speed` | `!= 1.0` |
/// | 5 | `sidestep_command` | `!= 0` |
/// | 6 | `sidestep_holdkey` | `!=` the invalid hold key |
/// | 7 | `sidestep_speed` | `!= 1.0` |
/// | 8 | `turn_command` | `!= 0` |
/// | 9 | `turn_holdkey` | `!=` the invalid hold key |
/// | 10 | `turn_speed` | `!= 1.0` |
/// | 11–15 | the action count |
///
/// Those are exactly the animation interpreter's pending movement fields, which transcribe the same
/// constructor defaults, so this is `Some(x) if x != default` field by field and nothing else.
/// \[verified\]
///
/// **What the corpus exercises.** Across the 1,187 recorded `0xF61C` bodies the flags word takes
/// nine distinct values, all of them below `0x800`: `current_holdkey` appears in 1,182 (always
/// the run hold key, 2), `current_style` in 175, `forward_command` in 673 and `turn_command` in
/// 524.
/// **No recorded body carries a queued action, 0 of 1,187**, and none carries a per-axis hold key
/// or a speed other than 1.0 — so the action arm and five of the eleven presence bits below are
/// transcribed but unexercised by the corpus, which is stated here rather than implied by a
/// passing test.
pub fn raw_motion_state_to_wire(
    s: &dereth_animation::motion::RawMotionState,
) -> dereth_protocol::movement::RawMotionState {
    use dereth_animation::command::MotionCommand;
    use dereth_animation::motion::HoldKey;

    let opt_key = |k: HoldKey, default: HoldKey| (k != default).then_some(k as u32);
    let opt_cmd = |c: MotionCommand, default: u32| (c.0 != default).then_some(c.0);
    let opt_speed = |v: f32| (v != 1.0).then_some(v);

    dereth_protocol::movement::RawMotionState {
        current_holdkey: opt_key(s.current_holdkey, HoldKey::None),
        current_style: opt_cmd(s.current_style, MotionCommand::NON_COMBAT.0),
        forward_command: opt_cmd(s.forward_command, MotionCommand::READY.0),
        forward_holdkey: opt_key(s.forward_holdkey, HoldKey::Invalid),
        forward_speed: opt_speed(s.forward_speed),
        sidestep_command: opt_cmd(s.sidestep_command, 0),
        sidestep_holdkey: opt_key(s.sidestep_holdkey, HoldKey::Invalid),
        sidestep_speed: opt_speed(s.sidestep_speed),
        turn_command: opt_cmd(s.turn_command, 0),
        turn_holdkey: opt_key(s.turn_holdkey, HoldKey::Invalid),
        turn_speed: opt_speed(s.turn_speed),
        // `command_index` is the index into the client's 412-entry `command_ids` table, never the
        // id itself; `MotionCommand::to_index` is that table. A command the table does not name
        // cannot be expressed on the wire, so it is dropped rather than sent as a wrong index.
        actions: s
            .actions
            .iter()
            .filter_map(|a| {
                a.action
                    .to_index()
                    .map(|command_index| dereth_protocol::movement::MotionAction {
                        command_index,
                        // `(stamp & 0x7FFF) | (autonomous ? 0x8000 : 0)`; the mask makes the
                        // narrowing total, so the fallback is unreachable rather than a guess.
                        stamp_and_autonomy: u16::try_from(a.stamp & 0x7FFF).unwrap_or(0)
                            | if a.autonomous { 0x8000 } else { 0 },
                        speed: a.speed,
                    })
            })
            .take(31)
            .collect(),
    }
}

/// The window class name, exported so a driving harness can find the window the way it finds the
/// retail one, by class. The window system names the class itself, so this is the value to match
/// rather than the value in use.
pub const RETAIL_WINDOW_CLASS: &str = style::CLASS_NAME;

/// `CharGenResultData` → `dereth_protocol::login::CharGenResult`, field for field.
///
/// The UI crate may not depend on `dereth-protocol` and the protocol may not depend on the UI; the
/// copy between them is the application's, exactly as [`character_set_from_login`] is in the other
/// direction. `version` is the client's hard-coded 1 and the checksum is filled in by the caller.
#[must_use]
pub fn chargen_result_to_wire(
    r: &dereth_client_contract::pregame::CharGenResultData,
) -> dereth_protocol::login::CharGenResult {
    dereth_protocol::login::CharGenResult {
        // "Hard-coded 1 by the client"; ACE's `CharacterCreateInfo::Unpack` skips it as
        // "Unknown constant (1)".
        version: 1,
        heritage_group: r.heritage_group,
        gender: r.gender,
        eyes_strip: r.eyes_strip,
        nose_strip: r.nose_strip,
        mouth_strip: r.mouth_strip,
        hair_color: r.hair_color,
        eye_color: r.eye_color,
        hair_style: r.hair_style,
        headgear_style: r.headgear_style,
        headgear_color: r.headgear_color,
        shirt_style: r.shirt_style,
        shirt_color: r.shirt_color,
        trousers_style: r.trousers_style,
        trousers_color: r.trousers_color,
        footwear_style: r.footwear_style,
        footwear_color: r.footwear_color,
        skin_shade: r.skin_shade,
        hair_shade: r.hair_shade,
        headgear_shade: r.headgear_shade,
        shirt_shade: r.shirt_shade,
        trousers_shade: r.trousers_shade,
        footwear_shade: r.footwear_shade,
        template_num: r.template_num,
        strength: r.strength,
        endurance: r.endurance,
        coordination: r.coordination,
        quickness: r.quickness,
        focus: r.focus,
        self_: r.self_,
        slot: r.slot,
        class_id: r.class_id,
        skill_advancement_classes: r.skill_advancement_classes.clone(),
        name: r.name.clone(),
        start_area: r.start_area,
        is_admin: r.is_admin,
        is_envoy: r.is_envoy,
        checksum_value: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The landblock line prints when any number in it moves under a held block.
    #[test]
    fn the_landblock_line_prints_when_any_number_in_it_moves_under_a_held_block() {
        use crate::present::SceneCensus;
        let block = (0xA9, 0xB4);
        let base = SceneCensus::default();
        let quiet = ViewerBlockReport::new(block, &base);

        // Every one of the six printed counters, on its own, with the block never moving. Each
        // closure is a landblock finishing its mesh under a player who has not walked anywhere.
        type BumpScene = fn(&mut SceneCensus);
        let held: [(&str, BumpScene); 6] = [
            ("blocks_meshed", |s| s.blocks_meshed += 1),
            ("terrain_surfaces", |s| s.terrain_surfaces += 1),
            ("scenery_objects", |s| s.scenery_objects += 1),
            ("buildings", |s| s.buildings += 1),
            ("static_objects", |s| s.static_objects += 1),
            ("object_triangles", |s| s.object_triangles += 1),
        ];
        for (name, bump) in held {
            let mut s = base;
            bump(&mut s);
            assert_ne!(
                quiet,
                ViewerBlockReport::new(block, &s),
                "{name} climbed and the landblock line stayed silent"
            );
        }
        // And the term it has always carried.
        assert_ne!(
            quiet,
            ViewerBlockReport::new((0xA9, 0xB5), &base),
            "the crossing itself"
        );
    }

    /// **`last_viewer_cell`.** The key was the cell; the line also prints `env_cell_counts().1`,
    /// which streams and is asked of the *camera's* cell rather than the body's.
    ///
    /// Falsified by: keying on the cell alone.
    #[test]
    fn the_viewer_cell_line_prints_when_the_interior_batch_count_moves_under_a_held_cell() {
        // An **interior** cell: `is_outdoors` is `(id & 0xFFFF) < 0x100`, so 0x0129 is indoors.
        let cell = dereth_primitives::CellId(0xA9B4_0129);
        let quiet = ViewerCellReport::new(cell, 0);
        assert!(
            !quiet.outdoors,
            "the fixture cell is indoors, which is what the line prints"
        );

        // The body has not moved a millimetre; the block carrying its cell finished meshing.
        assert_ne!(
            quiet,
            ViewerCellReport::new(cell, 7),
            "the interior batch count arrived and the line stayed silent"
        );
        // The two terms the line has always carried, through the same constructor the frame uses.
        let outdoor = dereth_primitives::CellId(0xA9B4_0001);
        assert_ne!(quiet, ViewerCellReport::new(outdoor, 0));
        assert!(
            ViewerCellReport::new(outdoor, 0).outdoors,
            "is_outdoors is not being consulted, so the line's own word is a constant"
        );
    }

    /// **`last_object_count`.** The key was the drawn count; eleven other printed numbers can move
    /// while it holds still, and the first arm below is the one that actually happens — a sync in
    /// which as many objects left as arrived.
    ///
    /// Falsified by: keying on `drawn` alone (every arm fails), or by dropping the magnitude on the
    /// two message-rate counters (the last arm fails and the line floods).
    #[test]
    fn the_object_census_prints_when_creates_and_removes_move_together() {
        use crate::objects::ObjectStats;
        use crate::present::SceneCensus;
        let w = SceneCensus::default();
        let s = ObjectStats::default();
        let quiet = ObjectReport::new(4, &w, &s).key();

        // One object created and one removed in the same sync: the count is unchanged and the
        // line used to stay silent over it.
        let churn = ObjectStats {
            creates: 1,
            removes: 1,
            ..s
        };
        assert_ne!(
            quiet,
            ObjectReport::new(4, &w, &churn).key(),
            "a create and a remove cancelled in the count and the census stayed silent"
        );

        // Each of the other printed numbers that leaves the count alone, **on its own**.
        //
        // `creates` and `removes` are in this list as well as in the churn arm above, and that is
        // not redundancy: a mutation blanking `creates` in the constructor SURVIVED the churn arm,
        // because that arm moves both counters at once and `removes` alone still moved the key.
        // The station confounded two variables; this is it rebuilt on one. `drawn` is a
        // parameter here, so a create with no matching remove can be driven with the count held.
        type BumpBoth = fn(&mut SceneCensus, &mut ObjectStats);
        let held: [(&str, BumpBoth); 10] = [
            ("creates", |_, s| s.creates += 1),
            ("removes", |_, s| s.removes += 1),
            ("merges", |_, s| s.merges += 1),
            ("recreates", |_, s| s.recreates += 1),
            ("parent_events", |_, s| s.parent_events += 1),
            ("container_exits_offered", |_, s| {
                s.container_exits_offered += 1
            }),
            ("server_objects_animated", |w, _| {
                w.server_objects_animated += 1
            }),
            ("server_objects_held", |w, _| w.server_objects_held += 1),
            ("server_object_setups", |w, _| w.server_object_setups += 1),
            ("server_object_triangles", |w, _| {
                w.server_object_triangles += 1
            }),
        ];
        for (name, bump) in held {
            let (mut w2, mut s2) = (w, s);
            bump(&mut w2, &mut s2);
            assert_ne!(
                quiet,
                ObjectReport::new(4, &w2, &s2).key(),
                "{name} climbed and the object census stayed silent"
            );
        }
        // And the term it has always carried.
        assert_ne!(
            quiet,
            ObjectReport::new(5, &w, &s).key(),
            "the drawn count itself"
        );

        // The two message-rate counters: announced on the first one and at every power of two,
        // silent in between. Both halves matter -- never silent, and never a line per frame.
        let mut printed = 0u32;
        let mut last = Some(quiet);
        for n in 0..=1024u64 {
            let r = ObjectReport::new(
                4,
                &w,
                &ObjectStats {
                    position_updates: n,
                    ..s
                },
            );
            if last != Some(r.key()) {
                last = Some(r.key());
                printed += 1;
                assert_eq!(
                    r.position_updates, n,
                    "the print carries the raw value, not the key's"
                );
            }
        }
        assert_eq!(
            printed, 11,
            "1,025 position updates must announce at 1, 2, 4 .. 1024 and nowhere else"
        );
        assert!(
            ObjectReport::new(
                4,
                &w,
                &ObjectStats {
                    movement_updates: 1,
                    ..s
                }
            )
            .key()
                != quiet,
            "the first movement update was never announced at all"
        );
    }

    /// **`net_error_reported`.** It was a `bool` over a printed *code*, so the second distinct
    /// error was silent -- and the second one is the interesting one, because it is what the
    /// reconnection did.
    ///
    /// Falsified by: `last.is_none()` (the old latch: the third arm fails), or by returning `true`
    /// unconditionally (the second arm fails and the line prints every frame the error is live).
    #[test]
    fn the_net_error_line_prints_the_second_distinct_error_and_not_a_repeat() {
        use dereth_transport::conn::NetErrorCode;
        // Two real connection-error codes, in the order a dropped session
        // then a refused re-login produces them.
        let first = NetErrorCode::ClientTimedOutServer;
        let second = NetErrorCode::PlayerAlreadyLoggedOn;
        assert_ne!(
            first, second,
            "the two arms below are the same code; this asserts nothing"
        );

        let mut last: Option<NetErrorCode> = None;
        let mut printed: Vec<NetErrorCode> = Vec::new();
        for code in [first, first, first, second, second, first] {
            if should_report_net_error(last, code) {
                last = Some(code);
                printed.push(code);
            }
        }
        assert_eq!(
            printed,
            vec![first, second, first],
            "each distinct error prints once and a repeat is silent"
        );
    }

    // Oracle: the original fixed window class and non-resizable window style.
    #[test]
    fn the_documented_window_style_has_no_resize_and_no_maximise() {
        assert_eq!(style::WINDOWED, 0x12CA_0000);
        const WS_THICKFRAME: u32 = 0x0004_0000;
        const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
        assert_eq!(style::WINDOWED & WS_THICKFRAME, 0);
        assert_eq!(style::WINDOWED & WS_MAXIMIZEBOX, 0);
        assert_eq!(style::EX_STYLE, 0);
        assert_eq!(RETAIL_WINDOW_CLASS, "Turbine Device Class");
    }

    /// A store the platform opened itself is the one the application reads: bring-up never looks
    /// at `dat_dir`, which here names nothing.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_store_opened_by_the_platform_is_the_one_the_app_reads() {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let cfg = Config {
            headless: true,
            frames: None,
            connect: false,
            sound: false,
            dat_dir: std::path::PathBuf::from("a directory that does not exist"),
            ..Config::default()
        };
        let app = App::<NullShell>::bring_up_with_store(
            cfg,
            Some(std::sync::Arc::clone(&store)),
            |_| Ok(Platform::headless(64, 64)),
            |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(64, 64))),
        )
        .expect("bring-up takes the store it is given");
        assert!(std::sync::Arc::ptr_eq(app.dat_store(), &store));
    }

    /// Behaviour: none (a host may attach one socket-free transport endpoint).
    /// A windowed App takes a relay-fed endpoint, which the replay attachment refuses it, and a
    /// second endpoint is handed back while the first is attached.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_windowed_app_takes_one_relay_endpoint() {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let cfg = Config {
            headless: false,
            frames: None,
            connect: false,
            sound: false,
            dat_dir: std::path::PathBuf::from("a directory that does not exist"),
            ..Config::default()
        };
        let mut app = App::<NullShell>::bring_up_with_store(
            cfg,
            Some(store),
            |_| Ok(Platform::headless(64, 64)),
            |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(64, 64))),
        )
        .expect("bring-up");
        let endpoint = || {
            crate::net::ClientNetwork::new("127.0.0.1", 9000, "account", "password", 1)
                .expect("an endpoint")
        };
        assert!(
            app.attach_replay_network(endpoint()).is_err(),
            "the replay attachment is for a headless run"
        );
        assert!(app.attach_relay_network(endpoint()).is_ok());
        assert!(
            app.replay_network_mut().is_some(),
            "the endpoint is socket-free"
        );
        assert!(
            app.attach_relay_network(endpoint()).is_err(),
            "one link at a time"
        );
    }
}
