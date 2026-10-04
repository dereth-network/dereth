//! The command line and the preferences file.
//!
//! The parser preserves the option arities, lookup rules and switch table, along with the
//! preferences-file location and profile format. Startup flags and `DisplayPrefs` defaults enter
//! through the same configuration boundary.
//!
//! Two developer affordances, neither of which changes what any retail spelling does:
//!
//! 1. The **rebuild-only switches** (`--headless`, `--frames`, `--capture`, `--dat-dir`) are spelled
//!    with two leading command characters. Exactly one command character is stripped before the
//!    lookup, so a doubled spelling can never name a retail option, and every retail spelling
//!    parses exactly as before.
//! 2. The "you must specify an account name / a host name" check is
//!    `Config::require_account_and_host`, run by the login path rather than by the parser. Given
//!    one of the account and the host without the other, start-up still stops with the launcher
//!    message; given neither, the client runs with no server, which is a development affordance
//!    (a tooling row of the behaviour registry), not a published divergence.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::corestrings::{display_string, ID_LAUNCHER_ONLY};

/// The parse failed. Every one of these surfaces to the user as corestrings 205, because
/// finalizing error output discards the accumulated text and shows that one string;
/// the `detail` is what the retail client would have put in its full output text and thrown away.
#[derive(Debug, thiserror::Error)]
#[error("{}", display_string(ID_LAUNCHER_ONLY, &[]))]
pub struct ConfigError {
    /// What was given. Never shown by the retail client.
    pub detail: String,
}

impl ConfigError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

/// The size every screen but gameplay runs at.
///
/// It is `0x320 x 0x258` at all three call sites — the
/// client's own start-up, the gameplay screen's constructor
/// (which passes it unforced) and its
/// destructor (forced unless gameplay remains active). Leaving gameplay returns to
/// **800x600**, and the number appears in retail three times.
pub const FORCED_LOGIN_SIZE: (u32, u32) = (800, 600);

/// The display preferences use the device globals' compiled-in initial values, except that the
/// client starts in a window: retail's initial value for full screen was on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayPrefs {
    /// Packs `width << 16 | height`; the compiled-in default `0x04000300` is 1024x768.
    pub resolution: u32,
    pub full_screen: bool,
    pub refresh_rate: u32,
    /// In `DisplayPrefs` and honoured by display-preference loading, but never registered as a
    /// user preference in this build, so it stays at its compiled-in `false`.
    pub triple_buffering: bool,
    pub sync_to_refresh: bool,
    /// As `triple_buffering`.
    pub antialiasing: bool,
}

impl Default for DisplayPrefs {
    fn default() -> Self {
        Self {
            resolution: 0x0400_0300,
            full_screen: false,
            refresh_rate: 0,
            triple_buffering: false,
            sync_to_refresh: false,
            antialiasing: false,
        }
    }
}

/// Display-preference values preserved by the compatibility projection.
/// The active renderer reads its preferences independently of this record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationFlags {
    pub fs_refresh_rate: u32,
    pub fs_bits_per_pixel: u32,
    pub fs_triple_buffering: bool,
    pub fs_sync_to_display_refresh: bool,
    pub antialiasing: bool,
}

impl Default for PresentationFlags {
    fn default() -> Self {
        Self {
            fs_refresh_rate: 0,
            fs_bits_per_pixel: 32,
            fs_triple_buffering: false,
            fs_sync_to_display_refresh: false,
            antialiasing: false,
        }
    }
}

/// Presentation defaults, overwritten when the device loads display preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
    pub width: u32,
    pub height: u32,
    pub full_screen: bool,

    /// Compatibility projection retained independently of the effective window size.
    pub compatibility: PresentationFlags,
}

impl Default for Presentation {
    /// The render-device presentation constructor uses this value.
    fn default() -> Self {
        Self {
            compatibility: PresentationFlags::default(),
            width: 800,
            height: 600,
            full_screen: false,
        }
    }
}

/// Command-line compatibility values retained with their parsed defaults and values.
/// Startup services do not consume these flags; their switch syntax remains accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetailFlags {
    /// `-language`.
    pub language: String,
    /// `-rodat`. **Bare `-rodat` turns read-only dats on; `-rodat <anything>` turns them off.**
    /// Default 1 (read-only).
    pub read_only_dat_files: bool,
    /// `-usemem`.
    pub use_memory_manager: bool,
    /// `-z` / `-zoneticket`. The accepted ticket value.
    pub zone_ticket: String,
    /// Records whether the `-glsticket` request flag was supplied.
    pub use_gls: bool,
    /// `-migrationurl`.
    pub migration_url: String,
    /// `-debug`, `strtoul(value, 0, 0)`. The mask *replaces* the whole flag word.
    pub debug_flags: u32,
}

impl Default for RetailFlags {
    fn default() -> Self {
        Self {
            language: String::new(),
            read_only_dat_files: true,
            use_memory_manager: false,
            zone_ticket: String::new(),
            use_gls: false,
            migration_url: String::from(
                "http://acbm.turbinegames.com/IISAcBillingMigration/IISAcBillingMigration.dll?ac1",
            ),
            debug_flags: 0xFFFF_F1F7,
        }
    }
}

/// Everything the application needs to start.
#[derive(Debug, Clone)]
pub struct Config {
    /// Parsed compatibility values that do not drive this client's startup services.
    pub retail: RetailFlags,

    // ---- the retail switch set ----
    /// `-a` / `-account`. Lower-cased in place with `_strlwr` by the handler.
    pub account: String,
    /// The same value **as it was typed**, which retail does not keep: `_strlwr` is in place.
    ///
    /// For the title bar and nothing else -- [`Config::window_title`] is its only reader. Every
    /// other consumer, the login handshake above all, wants [`Config::account`], because the
    /// lower-cased spelling is the one the server is given.
    pub account_as_typed: String,
    /// `-h` / `-host`.
    pub host: String,
    /// `-p` / `-port`. Client initialization defaults it to 7304 (0x1C88).
    pub port: u32,
    /// `-q` / `-outport`. Range-checked 1..=65535 by the handler; 0 means "not given".
    pub client_port: u32,
    /// `-prefs` (or `--prefs`), or the default preferences path selected at startup. Empty for a
    /// `--headless` run that named no file: see [`Self::preferences_named`].
    pub preferences_file: PathBuf,
    /// Whether the command line named the preferences file (`-prefs <file>` or `--prefs <file>`).
    ///
    /// **A `--headless` run that names none uses no settings folder at all**: it neither reads nor
    /// writes the player's preferences, keymap or screen layouts, because every one of those
    /// hangs off [`Self::preferences_file`] and an empty path is the client's own "no file" in
    /// each of them. A headless run is a capture or a regression run, and its output must depend
    /// on nothing in the player's folder, nor change anything there. Naming a file asks for it.
    pub preferences_named: bool,
    /// Which graphics backend to bring the device up on:
    /// `--renderer vulkan|d3d12`, else the `Renderer=` preference, else `None`, which leaves the
    /// choice to the default (Vulkan). A rebuild-only
    /// selector: retail had one renderer and no such switch.
    pub renderer: Option<dereth_client_contract::RendererChoice>,
    /// `-u` / `-user`. Stored and never read; vestigial in this build.
    pub start_char: String,
    /// `-r` / `-create`. As `start_char`.
    pub create_char: String,
    /// `-glsticketdirect`, or the value `-glsticket` read out of the registry.
    pub gls_ticket: String,
    /// `-v` / `-vgpassword`. Also the password the `-v` login path sends.
    pub vg_password: String,

    // ---- the device ----
    /// The windowed-mode request, whose constructed default is 1. There is no switch for it in
    /// this build; device initialization receives it as its windowed-request argument.
    pub windowed: bool,
    /// Device initialization receives `(800, 600)` from UI initialization.
    pub width: u32,
    pub height: u32,
    /// The device stores these display preferences after user preferences load.
    pub display: DisplayPrefs,
    /// The three `Camera.*` preferences after loading; see
    /// [`crate::camera::CameraPreferences`] for where each reaches retail's camera.
    pub camera: crate::camera::CameraPreferences,
    /// The four `Input.*` mouse-look preferences registered by input startup.
    /// `Input.UseMouseTurning` is what decides
    /// whether the mouse turns the *body* or orbits the camera.
    pub mouse_look: crate::actions::camera::MouseLookPreferences,
    /// The three render-degradation statics after preference loading; see
    /// [`crate::render_prefs::RenderPreferences`]
    /// for each name's consumer in retail.
    pub render: crate::render_prefs::RenderPreferences,

    // ---- rebuild-only ----
    /// `--headless`: no window, an offscreen render target, and the WARP adapter so the output does
    /// not depend on the machine's GPU.
    pub headless: bool,
    /// `--frames <n>`: stop after n frames. `None` runs until the loop ends on its own.
    pub frames: Option<u64>,
    /// `--capture <path>`: write the last frame as a PNG.
    pub capture: Option<PathBuf>,
    /// `--capture-at <frame>:<path>`, any number of times: once that many frames have been drawn,
    /// write the last of them as a PNG, so one run can record a picture before and after a change.
    /// Like `--capture` it reads the offscreen target and needs `--headless`.
    pub capture_at: Vec<(u64, PathBuf)>,
    /// `--set-at <frame>:<Section.Name>=<value>`, any number of times: as that frame starts, set
    /// the preference as an options page does (the value store, then the change request), so a
    /// run can change an option part way through. The value is spelled as the preferences file
    /// spells it.
    pub set_at: Vec<(u64, String)>,
    /// `--action-at <frame>:<ActionName>`, any number of times: as that frame starts, the action
    /// (a key-map name, such as `ToggleAllegiancePanel`) happens as if its key had been pressed, so
    /// a run can open a window or work a toggle part way through.
    pub action_at: Vec<(u64, u32)>,
    /// `--no-console`: do not borrow the console of the terminal the client was started from, so
    /// the run is silent even there. (A console is never created, with or without it.)
    ///
    /// **The parser is not what acts on this.** The binary is linked for the windows subsystem, so
    /// the console is something `main` asks `dereth_console::attach` for before anything is
    /// printed -- which is before this parse runs, and before a parse *failure* could be reported.
    /// `main` therefore reads the switch straight out of `argv` with [`no_console_in_argv`], and
    /// this field only records the same answer for anything downstream that wants it.
    pub console: bool,
    /// `--dat-dir <dir>`: the data files. The folder holds the later set (`client_portal.dat`,
    /// `client_cell_1.dat`, `client_local_English.dat`, optional `client_highres.dat`) and may hold
    /// the set from before Throne of Destiny (`portal.dat`, `cell.dat`) beside it. [`Self::era`]
    /// chooses which set draws the world ([`crate::assets::open_world_files`]).
    pub dat_dir: PathBuf,
    /// `--classic-dat-dir <dir>`: where to look for the set from before Throne of Destiny when it
    /// is not beside the later one. It wins over a set in [`Self::dat_dir`]. Only where the older
    /// files are read from follows from it: the classic interface, the older looks and an older
    /// world are available whenever an older set is found, wherever it is. `None`: look in
    /// [`Self::dat_dir`] alone.
    pub classic_dat_dir: Option<PathBuf>,
    /// `--overlay-dat-dir <dir>`: the folder this world's overlay is kept in for the run (the
    /// records the world adds, replaces and deletes over the locked data files, which a patch from
    /// the server writes and nothing else does). Only where the overlay is follows from it.
    /// `None`: a folder per server in the per-user cache ([`crate::world_overlay::overlay_dir`]).
    pub overlay_dat_dir: Option<PathBuf>,
    /// `--object-visuals <world|legacy|modern>`: the era whose look the world's objects draw
    /// with, over `[Render] Objects` (which it also sets, so the options page shows it). `None`:
    /// the switch was not given and the preference stands; `Some(None)`: the world's own.
    pub object_visuals: Option<Option<crate::render_prefs::RegionStyle>>,
    /// `--era <name>`: the era the server says its world plays (`eor`, `infiltration`), as the
    /// launcher reads it from the world's status. It wins over the era read from the data files,
    /// and chooses which of [`Self::dat_dir`]'s sets draws the world; `None`: the end of retail's
    /// set when the folder has it, and the data files decide.
    pub era: Option<dereth_primitives::EraId>,
    /// `--era-features <name=true,...>`: the systems the server says its world has, as the
    /// launcher reads them from the world's status. Each one named wins over the era's table; a
    /// name this client does not know is skipped.
    pub era_features: dereth_primitives::EraFeatureOverrides,

    // ---- the static scene ----
    /// `--landblock <hex>`: which landblock the camera starts over. Holtburg by default, the
    /// usual worked example.
    pub landblock: u16,
    /// `--start-cell <hex>`: which interior cell the offline body stands in, so that
    /// "stand in the training dungeon" is something a person can type. `None` leaves the body
    /// where the offline character is placed, on the ground in the middle of `landblock`.
    pub start_cell: Option<u32>,
    /// `--no-cell-statics`: leave the interior cells' baked objects undrawn and unregistered.
    /// The client has no such switch; this exists so that a person can look at the same room with
    /// and without its furniture, to see exactly what the cell statics contribute.
    pub cell_statics: bool,
    /// `--no-mesh-collision`: collide against setup spheres alone. The client chooses from
    /// the part array's physics-BSP presence; this switch lets a person compare both collision
    /// paths at the same door.
    pub mesh_collision: bool,
    /// `[Render] TerrainBlending` (`cpu`, `gpu` or `splat`), read by [`Self::apply_preferences`].
    /// See [`crate::render_prefs::TerrainBlending`]; the preferences file is its only control.
    pub terrain_blending: crate::render_prefs::TerrainBlending,
    /// `--land-radius <n>`: the landscape's middle radius, so `2n+1` blocks a side are meshed.
    ///
    /// It is set by `Render.LandscapeDrawDistance` (see
    /// [`crate::render_prefs::landscape_draw_distance`]), applied by [`Self::apply_preferences`]
    /// before the command line, matching the renderer's preference update that applies it
    /// before anything asks the landscape to generate. The default below is that preference's
    /// registered default, **8**; the switch stays because a bench wants a cheap window.
    pub land_radius: u32,
    /// `--scenery-radius <n>`: how many rings of blocks grow scenery, buildings and static objects.
    pub scenery_radius: u32,
    /// `--stream-budget-ms <n>`: milliseconds per frame the landscape may spend building
    /// landblocks, nearest first, before the rest waits for later frames. `0` builds everything
    /// queued at once. Unset, a connected run uses a small budget (so the portal tunnel keeps
    /// animating through a load) and an offline run or capture builds everything at once.
    pub stream_budget_ms: Option<u32>,
    /// `--object-identity-ms <n>`: milliseconds per frame the client gives, from start-up, to
    /// working out which of the other era's records stand for the world's, so the other era's
    /// object look is ready before it is asked for. `0` works them out at once the first time
    /// that look is drawn. Unset, a connected run uses a small budget and an offline run or
    /// capture works them out at once.
    pub object_identity_ms: Option<u32>,
    /// `--no-world`: fall back to the full-screen quad, which is what the quad regression
    /// wants and what a machine too slow for the world can still run.
    pub world: bool,

    // ---- the embodied character ----
    /// `--no-character`: leave the body out and fly the free camera over the static scene.
    /// Kept reachable so the static-scene regression does not disappear.
    pub character: bool,

    // ---- the adaptive degrade ----
    /// Run the degradation governor, so its multiplier follows the measured frame rate and every
    /// LOD threshold slides with it.
    ///
    /// **On by default, as in retail** (the automatic-degradation flag starts set, and
    /// `Render.AutomaticDegrades` is registered true). The player's `Render.AutomaticDegrades`
    /// preference turns it off; `--auto-degrades` and `--no-auto-degrades` override both. A
    /// [`crate::scene::SceneConfig`] built directly, as the tests build theirs, stays pinned; see
    /// `crate::scene::SceneConfig::auto_degrades`.
    pub auto_degrades: bool,

    // ---- the sky ----
    /// `--time-of-day <0..1>`: where in the in-game day the session starts, 0 = midnight,
    /// 0.5 = noon. `None` runs the clock alone.
    ///
    /// This is not a new mechanism. The client's own knob is the global-registry variable
    /// `GameTime.TimeZeroDelta` ("Number of seconds to adjust Timer time to compute GameTime time.
    /// GameTime effects the state of the sky"), written by sky time adjustment;
    /// the switch names the fraction it is easier to think in and
    /// `crate::sky::GameClock::set_time_of_day` does the arithmetic.
    pub time_of_day: Option<f32>,

    // ---- the connected slice ----
    /// Run against `-h`, with `-a` and `-v`. **On by default**; `--no-connect` is the opt-out
    /// and `--connect` is still accepted.
    ///
    /// The client run method connects unconditionally, once, before the main loop, and
    /// command-line validation refuses to start at all without an
    /// account and a host — so in the retail client "connect" is not a choice. It is one here for
    /// exactly one reason: the offline runs (quad, static scene, character, sky) and the headless capture
    /// gate must keep running with no server. That is why the connect is skipped when there is no
    /// `-a`/`-h` rather than being refused: `App::new` tests
    /// `Config::require_account_and_host` and connects only when it passes, which makes
    /// `dereth-client -a ac01 -v pass -h 127.0.0.1:19000` the ordinary invocation and a bare
    /// `dereth-client` the offline one.
    pub connect: bool,
    /// `--enter-world`: once `0xF658` has arrived, enter the world with `-u <name>` or, failing
    /// that, the first character in the list, then log off once `0x0013` makes the session
    /// playable and exit.
    ///
    /// The retail client waits for the player to click. A scripted run is what makes the
    /// connected acceptance run reproducible; injected keystrokes do not prove that the client reached the
    /// playable state on its own.
    pub enter_world: bool,
    /// `--linger <seconds>`: how long `--enter-world` stays in the world before logging off.
    ///
    /// A populated-world demo needs it: objects appear the instant the world burst arrives, but
    /// they only **move** over the following seconds, so a run that logs off as soon as `0x0013`
    /// lands sees a still photograph. Zero (log off at once) stays the default. Like `--frames`, this
    /// is this rebuild's switch and not one the retail client has.
    pub linger: f64,
    /// `--cast <spell id>`: once `--enter-world` is in the world, open the backpack and cast this
    /// spell as the spell bar's Cast button does, through the client's own component check and
    /// request. It proves a login can play, not just arrive; like `--linger` it is this rebuild's
    /// switch and not one the retail client has.
    pub cast: Option<u32>,
    /// `--say <text>`, any number of times: once `--enter-world` is in the world, submit each
    /// line in turn as the chat window's Send does (so `@` and `/` commands are commands). Like
    /// `--cast` it is this rebuild's switch and not one the retail client has.
    pub say: Vec<String>,
    /// `--use <object id | closest | logout>`, any number of times: after any `--say` lines, use
    /// each in turn, six seconds apart, as a double-click uses it (a door's id twice opens and
    /// closes it); `closest` is the closest compass item, selected by its key's action, and
    /// `logout` logs the character out to character select as the confirmed logout button does,
    /// departure and all. This rebuild's switch.
    pub use_targets: Vec<String>,

    // ---- the shell ----
    /// Bring up the UI element tree and flow controller, run the mode machine, and draw the current screen
    /// over the world. **On by default**; `--no-ui` is the opt-out.
    ///
    /// **The retail client always has a UI**, so the default is on. The two capture gates
    /// (`tests/gpu/presentation/headless_capture_determinism.rs` and `tests/gpu/rendering/static_scene.rs`) therefore capture the world plus
    /// the interface over it, which is what the client shows; both assert reproducibility rather
    /// than a golden image.
    pub ui: bool,
    /// `--no-sound`: do not open an audio device.
    ///
    /// Sound initialization tolerates having no device and so does this — and the
    /// tolerance is observable, because `srand((unsigned)time(NULL))` runs *only if DirectSound
    /// initialised*, so a silent client keeps the CRT generator's default seed of 1. The switch
    /// exists because a headless regression must not take the machine's audio endpoint, and
    /// `--headless` therefore implies it.
    pub sound: bool,
    /// `--ui-mode <hex>`: queue one of the eight UI-flow modes once the shell is up, so a screen
    /// that the flow would only reach through the network can be looked at by hand.
    ///
    /// A rebuild-only switch and a debugging one: an id the build does not register is refused and
    /// counted (`UiStats::unregistered_mode_requests`) rather than retried for ever, which is what
    /// the client would do.
    pub ui_mode: Option<u32>,

    // ---- the log ----
    /// `--log <filter>`, else the `Log.Level` preference, else `None` for the default (`info`):
    /// which events the client's log shows. A level (`error`, `warn`, `info`, `debug`, `trace`,
    /// `off`) or a comma-separated list of `target=level` directives after an optional default
    /// level, such as `info,dereth_client_runtime::frame=trace`. Read by the binary when it
    /// installs the log; an unreadable filter is reported there and the default stands.
    ///
    /// The live diagnostic traces are targets of their own at `debug`, off at the default level:
    /// `dereth::trace::net` (every received datagram's header), `dereth::trace::camera` (the
    /// camera's fields every 30th frame), `dereth::trace::raise` (the `+10` raise inputs) and
    /// `dereth::trace::notice` (every text-bearing notice and where its chat line went).
    /// `--log info,dereth::trace=debug` turns on all four ([`crate::trace`]).
    ///
    /// The preference takes a level only: the profile format drops every `=` in a value, so a
    /// directive list cannot survive the file.
    pub log_filter: Option<String>,
    /// `--log-file`, or `Log.File=True`: also append the log, with timestamps, to
    /// `dereth-client.log` beside the preferences file.
    pub log_file: bool,
    /// `--log-spans`: add a line as each enabled span closes -- a frame, a frame step, a world load,
    /// a landblock build -- with how long it ran. Which of them are enabled is the filter's
    /// business: frames are `debug`, frame steps `trace`.
    pub log_spans: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            retail: RetailFlags::default(),
            account: String::new(),
            account_as_typed: String::new(),
            host: String::new(),
            port: 7304,
            client_port: 0,
            preferences_file: PathBuf::new(),
            preferences_named: false,
            renderer: None,
            start_char: String::new(),
            create_char: String::new(),
            gls_ticket: String::new(),
            vg_password: String::new(),
            landblock: crate::landblock::DEFAULT_LANDBLOCK,
            start_cell: None,
            cell_statics: true,
            mesh_collision: true,
            terrain_blending: crate::render_prefs::TerrainBlending::default(),
            // `Render.LandscapeDrawDistance`'s registered default, `Medium`, which is what a fresh
            // retail profile gets -- not the cheapest window (3, `VeryLow`).
            land_radius: crate::render_prefs::LANDSCAPE_DRAW_DISTANCE_DEFAULT,
            scenery_radius: 1,
            stream_budget_ms: None,
            object_identity_ms: None,
            world: true,
            character: true,
            auto_degrades: true,
            time_of_day: None,
            connect: true,
            enter_world: false,
            linger: 0.0,
            cast: None,
            say: Vec::new(),
            use_targets: Vec::new(),
            ui: true,
            sound: true,
            ui_mode: None,
            log_filter: None,
            log_file: false,
            log_spans: false,
            windowed: true,
            width: 800,
            height: 600,
            display: DisplayPrefs::default(),
            camera: crate::camera::CameraPreferences::default(),
            mouse_look: crate::actions::camera::MouseLookPreferences::default(),
            render: crate::render_prefs::RenderPreferences::default(),
            headless: false,
            frames: None,
            capture: None,
            capture_at: Vec::new(),
            set_at: Vec::new(),
            action_at: Vec::new(),
            console: true,
            dat_dir: default_dat_dir(),
            object_visuals: None,
            classic_dat_dir: None,
            overlay_dat_dir: None,
            era: None,
            era_features: dereth_primitives::EraFeatureOverrides::default(),
        }
    }
}

/// Command characters selected during client initialization: `-host` and `/host` are accepted.
const CMD_CHARS: &[char] = &['-', '/'];

/// The arity of a switch, i.e. the low nibble of its argument-type word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arity {
    /// `1` — takes no value; evaluated immediately with an empty string.
    None,
    /// `2` — requires a value; the next token is consumed **even if it starts with a command
    /// character**. An empty token errors with `%S requires a value`.
    Required,
    /// `3` and `0xA` — the next token is taken only if it is not a switch.
    Optional,
}

/// One row built by the two client command-line passes,
/// in the order those two functions register them.
struct Switch {
    long: &'static str,
    short: Option<char>,
    arity: Arity,
}

/// The complete switch set accepted by the client command-line parser.
const SWITCHES: &[Switch] = &[
    Switch {
        long: "account",
        short: Some('a'),
        arity: Arity::Required,
    },
    Switch {
        long: "debug",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "host",
        short: Some('h'),
        arity: Arity::Required,
    },
    Switch {
        long: "language",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "outport",
        short: Some('q'),
        arity: Arity::Required,
    },
    Switch {
        long: "port",
        short: Some('p'),
        arity: Arity::Required,
    },
    Switch {
        long: "prefs",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "rodat",
        short: None,
        arity: Arity::Optional,
    },
    Switch {
        long: "usemem",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "user",
        short: Some('u'),
        arity: Arity::Required,
    },
    Switch {
        long: "create",
        short: Some('r'),
        arity: Arity::Required,
    },
    Switch {
        long: "zoneticket",
        short: Some('z'),
        arity: Arity::Required,
    },
    Switch {
        long: "glsticketdirect",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "glsticket",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "migrationurl",
        short: None,
        arity: Arity::Optional,
    },
    Switch {
        long: "vgpassword",
        short: Some('v'),
        arity: Arity::Required,
    },
];

/// The rebuild-only switches, spelled with two command characters so no retail spelling changes
/// meaning. See the module documentation.
const REBUILD_SWITCHES: &[Switch] = &[
    Switch {
        long: "headless",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "frames",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "capture",
        short: None,
        arity: Arity::Required,
    },
    // Silence even for a run started from a terminal, whose console the client otherwise borrows.
    Switch {
        long: "no-console",
        short: None,
        arity: Arity::None,
    },
    // Not in any help text: a deliberate panic, so the crash log's panic hook can be shown to
    // record a backtrace.
    Switch {
        long: "crash-test",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "dat-dir",
        short: None,
        arity: Arity::Required,
    },
    // Where the files from before Throne of Destiny are, when they are not beside the later ones.
    Switch {
        long: "classic-dat-dir",
        short: None,
        arity: Arity::Required,
    },
    // Where the world's overlay is kept.
    Switch {
        long: "overlay-dat-dir",
        short: None,
        arity: Arity::Required,
    },
    // The era whose look the world's objects draw with.
    Switch {
        long: "object-visuals",
        short: None,
        arity: Arity::Required,
    },
    // A preference set part way through a run, and a picture taken part way through.
    Switch {
        long: "set-at",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "capture-at",
        short: None,
        arity: Arity::Required,
    },
    // An action part way through a run, as if its key had been pressed.
    Switch {
        long: "action-at",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "era",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "era-features",
        short: None,
        arity: Arity::Required,
    },
    // The retail `-prefs`, doubled.
    Switch {
        long: "prefs",
        short: None,
        arity: Arity::Required,
    },
    // The static scene.
    Switch {
        long: "landblock",
        short: None,
        arity: Arity::Required,
    },
    // Stand the offline body in an interior cell instead of on the grass.
    Switch {
        long: "start-cell",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "no-cell-statics",
        short: None,
        arity: Arity::None,
    },
    // The control half of the mesh-collision differential.
    Switch {
        long: "no-mesh-collision",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "land-radius",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "scenery-radius",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "stream-budget-ms",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "object-identity-ms",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "no-world",
        short: None,
        arity: Arity::None,
    },
    // The embodied character.
    Switch {
        long: "no-character",
        short: None,
        arity: Arity::None,
    },
    // The adaptive degrade.
    Switch {
        long: "auto-degrades",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "no-auto-degrades",
        short: None,
        arity: Arity::None,
    },
    // The sky.
    Switch {
        long: "time-of-day",
        short: None,
        arity: Arity::Required,
    },
    // The connected slice.
    Switch {
        long: "connect",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "enter-world",
        short: None,
        arity: Arity::None,
    },
    // The populated slice.
    Switch {
        long: "linger",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "cast",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "say",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "use",
        short: None,
        arity: Arity::Required,
    },
    // The shell; `--ui` is kept as an accepted no-op now that the UI is the default.
    Switch {
        long: "ui",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "no-ui",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "no-connect",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "no-sound",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "ui-mode",
        short: None,
        arity: Arity::Required,
    },
    // Which graphics backend the device comes up on.
    Switch {
        long: "renderer",
        short: None,
        arity: Arity::Required,
    },
    // The log: its filter, a copy in a file, and the span timings.
    Switch {
        long: "log",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "log-file",
        short: None,
        arity: Arity::None,
    },
    Switch {
        long: "log-spans",
        short: None,
        arity: Arity::None,
    },
];

/// a token is a switch when its first character
/// is in [`CMD_CHARS`].
fn is_switch(tok: &str) -> bool {
    tok.chars().next().is_some_and(|c| CMD_CHARS.contains(&c))
}

/// Strip **exactly one** leading command character, which is why `--rodat` is looked up as the long
/// name `-rodat` and fails.
fn strip_one(tok: &str) -> &str {
    let mut it = tok.chars();
    if it.next().is_some_and(|c| CMD_CHARS.contains(&c)) {
        it.as_str()
    } else {
        tok
    }
}

/// Short-name lookup (exact, case sensitive) for a one-character name, otherwise
/// long-name lookup (case **insensitive**). If the short lookup misses, the token is
/// retried as a long name.
fn find<'a>(table: &'a [Switch], name: &str) -> Option<&'a Switch> {
    if name.chars().count() == 1 {
        let c = name.chars().next().unwrap_or('\0');
        if let Some(s) = table.iter().find(|s| s.short == Some(c)) {
            return Some(s);
        }
    }
    table.iter().find(|s| s.long.eq_ignore_ascii_case(name))
}

/// **`--no-console`, answered from `argv` alone.**
///
/// `main` needs this before [`Config::from_args_and_prefs_at`] runs, because the console is what the
/// answer is *about*: the binary is linked for the windows subsystem, nothing is printed until
/// `dereth_console::attach` is called, and a parse that fails reports itself on stderr. Deciding
/// the console after the parse would mean the one run that most needs a console -- the one whose
/// command line is wrong -- is the run that has none.
///
/// It is not a second parser. The resolution is the real one: [`is_switch`] for the command
/// character, two of them for a rebuild-only switch, and [`find`] over [`REBUILD_SWITCHES`], which
/// is case-insensitive on long names. So `/no-console`, `-/NO-CONSOLE` and `--no-console` all mean
/// it, and a single `-no-console` does not -- exactly as the module documentation says of every
/// rebuild-only switch.
///
/// A value is never consumed, so this cannot be confused by the token *after* it: `--no-console`
/// has [`Arity::None`], and the only tokens inspected are ones that resolve to it by name.
#[must_use]
pub fn no_console_in_argv<S: AsRef<str>>(argv: &[S]) -> bool {
    rebuild_flag_in_argv(argv, "no-console")
}

/// `argv` as it may be written to a log: the value after each switch that carries a login -- the
/// account (`-a`), the password (`-v`) and the two tickets (`-z`, `-glsticketdirect`) -- replaced
/// by `***`, so a log kept or shared never carries one.
///
/// A token is resolved as the parser resolves it ([`find`] over the retail switches, after one or
/// two command characters), and an upper-case short name is hidden too: a spelling the parser
/// refuses still ends the run with its log kept, and that log must not carry the value either.
/// `-glsticket` takes no value (the ticket is read elsewhere), so the token after it is left alone.
#[must_use]
pub fn argv_for_log<S: AsRef<str>>(argv: &[S]) -> Vec<String> {
    const SECRET: &[&str] = &["account", "vgpassword", "zoneticket", "glsticketdirect"];
    let mut out = Vec::with_capacity(argv.len());
    let mut hide_next = false;
    for tok in argv {
        let tok = tok.as_ref();
        if std::mem::take(&mut hide_next) {
            out.push("***".to_owned());
            continue;
        }
        if is_switch(tok) {
            let name = strip_one(strip_one(tok));
            hide_next = [name.to_owned(), name.to_ascii_lowercase()]
                .iter()
                .filter_map(|n| find(SWITCHES, n))
                .any(|s| SECRET.contains(&s.long));
        }
        out.push(tok.to_owned());
    }
    out
}

/// **`--crash-test`, answered from `argv` alone**, for the same reason as [`no_console_in_argv`]:
/// the crash log's hook is installed before the parse, and the deliberate panic this asks for
/// proves that hook. The switch is in no help text.
#[must_use]
pub fn crash_test_in_argv<S: AsRef<str>>(argv: &[S]) -> bool {
    rebuild_flag_in_argv(argv, "crash-test")
}

/// Whether `argv` holds the arity-`None` rebuild-only switch `long`, resolved as the parser
/// resolves it.
fn rebuild_flag_in_argv<S: AsRef<str>>(argv: &[S], long: &str) -> bool {
    argv.iter().any(|tok| {
        let tok = tok.as_ref();
        is_switch(tok)
            && tok.chars().nth(1).is_some_and(|c| CMD_CHARS.contains(&c))
            && find(REBUILD_SWITCHES, strip_one(strip_one(tok))).is_some_and(|s| s.long == long)
    })
}

impl Config {
    /// Build the scene policy from the loaded profile and connection mode.
    /// A connected client streams in four-millisecond slices unless explicitly overridden.
    #[must_use]
    pub fn scene_config(&self) -> crate::scene::SceneConfig {
        crate::scene::SceneConfig {
            landblock: self.landblock,
            // `--start-cell` names an interior cell of `--landblock` to stand in.
            start_cell: self.start_cell.map(dereth_primitives::CellId),
            cell_statics: self.cell_statics,
            mesh_collision: self.mesh_collision,
            land_radius: self.land_radius,
            scenery_radius: self.scenery_radius,
            character: self.character,
            auto_degrades: self.auto_degrades,
            time_of_day: self.time_of_day,
            // The profile's `Camera.*` values, for the body's camera.
            camera: self.camera,
            mouse_look: self.mouse_look,
            // The profile's `Render.*` values -- the field of view, the aspect ratio, the
            // texture-detail levels, multi-pass alpha and the two degrade knobs.
            // The governor switch the command line settled on is the one the options page shows.
            render: crate::render_prefs::RenderPreferences {
                automatic_degrades: self.auto_degrades,
                ..self.render
            },
            // A connected client streams the landscape over several frames, so the portal tunnel
            // keeps animating through a teleport; an offline run or a capture builds it all at once.
            // Measured on a live Extreme-distance login, a budget this small still finishes the whole
            // window inside the tunnel's own minimum duration, and leaves the tunnel animating smoothly.
            stream_budget: match self.stream_budget_ms {
                Some(0) => None,
                Some(ms) => Some(std::time::Duration::from_millis(u64::from(ms))),
                None => self
                    .will_connect()
                    .then_some(std::time::Duration::from_millis(4)),
            },
            // `[Render] TerrainBlending`, the only control there is for it.
            gpu_terrain_merge: self.terrain_blending != crate::render_prefs::TerrainBlending::Cpu,
            terrain_splat: self.terrain_blending == crate::render_prefs::TerrainBlending::Splat,
            // The client keeps the object-identity verdicts in its per-user cache folder. A
            // connected client works them out in the background from start-up, so no frame
            // waits on them; an offline run or a capture works them out when first drawn.
            object_identity_cache: true,
            object_identity_budget: match self.object_identity_ms {
                Some(0) => None,
                Some(ms) => Some(std::time::Duration::from_millis(u64::from(ms))),
                None => self
                    .will_connect()
                    .then_some(crate::app::IDENTITY_BACKGROUND_BUDGET),
            },
            ..crate::scene::SceneConfig::default()
        }
    }

    /// The window title: `Dereth | <account>`, or `Dereth` when there is no account.
    ///
    /// **A deliberate divergence from retail** (client divergence CD-006). Window creation is called
    /// with the constant `"Asheron's Call"` and the retail client never changes it. The account is
    /// here because more than one client is routinely up at once on this machine -- the char-gen
    /// sweeps run several, and trade and housing need two -- and the task bar button is the only
    /// thing that tells them apart.
    ///
    /// The spelling is [`Config::account_as_typed`], not [`Config::account`]: `-a Tester` reads
    /// `Dereth | Tester` rather than `Dereth | tester`. What goes to the server is unaffected.
    ///
    /// An offline run (`--headless`, or any of the offline slices) has no account and gets the
    /// bare name, which is also what a run whose account is whitespace gets.
    #[must_use]
    pub fn window_title(&self) -> String {
        self.window_title_for("Dereth")
    }

    /// [`Self::window_title`] for a product named `name`: the name, and the account after it.
    #[must_use]
    pub fn window_title_for(&self, name: &str) -> String {
        let account = self.account_as_typed.trim();
        if account.is_empty() {
            name.to_string()
        } else {
            format!("{name} | {account}")
        }
    }

    /// The command line `argv` (without the program name) and the preferences file selected by
    /// it, `default_preferences_file` being the one used when no `-prefs` is given.
    ///
    /// This is the actual startup algorithm; the host supplies the command line and the
    /// machine-specific default path, so an offline test can exercise `-prefs` without reading or
    /// writing the user's profile. An empty default means no file: nothing is read, and the
    /// empty path is what every writer takes as "no file".
    ///
    /// # Errors
    /// [`ConfigError`] for any parse failure.
    pub fn from_args_and_prefs_at(
        argv: &[String],
        default_preferences_file: &Path,
    ) -> Result<Self, ConfigError> {
        Self::from_args_and_prefs_named_at(argv, default_preferences_file, false)
    }

    /// [`Self::from_args_and_prefs_at`], with `named` saying the default file was asked for
    /// outright (its folder named in the environment), so that even a `--headless` run reads it
    /// and writes it back, as it does a file given with `-prefs`.
    ///
    /// # Errors
    /// [`ConfigError`] for any parse failure.
    pub fn from_args_and_prefs_named_at(
        argv: &[String],
        default_preferences_file: &Path,
        named: bool,
    ) -> Result<Self, ConfigError> {
        // Native constructs the preferences-file path, parses the arguments (which may replace it),
        // and only then initializes and loads user preferences. Parse once through the real parser
        // to resolve and validate that selection; do not maintain a second switch scanner whose
        // case, duplicate, command-character, or required-value rules drift.
        let mut cfg = Self {
            preferences_file: default_preferences_file.to_path_buf(),
            preferences_named: named,
            ..Self::default()
        };
        cfg.parse_args(argv)?;
        // A headless run that named no file keeps out of the player's settings folder entirely,
        // reading nothing from it and, through the empty path, writing nothing to it.
        if cfg.headless && !cfg.preferences_named {
            cfg.preferences_file = PathBuf::new();
        }
        let prefs = Preferences::load(&cfg.preferences_file);
        cfg.apply_preferences(&prefs);

        // Re-apply the command line because rebuild-only switches such as `--land-radius` share
        // fields with profile preferences and have always been the final override in this client.
        // The parser's stores are absolute/idempotent, so this preserves that policy while the
        // selected-file load itself follows native's order.
        cfg.parse_args(argv)?;
        Ok(cfg)
    }

    /// The same, with the argument vector and the loaded preferences supplied. This is the form the
    /// tests use; nothing here reads the environment.
    ///
    /// # Errors
    /// [`ConfigError`] for any parse failure.
    pub fn from_args_and_prefs_with(
        argv: &[String],
        prefs: &Preferences,
    ) -> Result<Self, ConfigError> {
        let mut cfg = Self::default();
        cfg.apply_preferences(prefs);
        cfg.parse_args(argv)?;
        Ok(cfg)
    }

    /// The registered display-preference names and defaults, plus the one
    /// `Render.*` preference whose owner is a `SceneConfig` field.
    pub fn apply_preferences(&mut self, prefs: &Preferences) {
        // `Render.LandscapeDrawDistance` sets the smart-box mid radius, then
        // the landscape mid radius represented by [`Self::land_radius`]. Without it the
        // landscape would sit at 3 -- `VeryLow`, the *lowest* of the six choices -- and the
        // window would reach 672 m where a fresh retail install reaches 1 632 m: no distant
        // mountains, and nothing beyond 672 m able to occlude anything.
        if let Some(v) = crate::render_prefs::landscape_draw_distance(prefs) {
            self.land_radius = v;
        }
        // The remaining render preferences, consumed by the per-frame
        // preference update. `land_radius` above stays
        // as set there because it is a `SceneConfig` field of its own; `self.render` carries the
        // same value so that the live options-page path and the start-up path agree.
        self.render = crate::render_prefs::RenderPreferences::from_preferences(prefs);
        // `Render.AutomaticDegrades` is the player's switch for the governor, registered true.
        self.auto_degrades = self.render.automatic_degrades;
        self.terrain_blending = crate::render_prefs::terrain_blending(prefs);
        // `Renderer=vulkan|d3d12`, in `[Render]` or with no section at all.
        // A name this build does not understand leaves the choice where it was, exactly as an
        // unparsable numeric preference leaves its registered default standing.
        if let Some(b) = prefs
            .get("Render.Renderer")
            .or_else(|| prefs.get("Default.Renderer"))
            .and_then(dereth_client_contract::RendererChoice::parse)
        {
            self.renderer = Some(b);
        }
        // `[Log] Level=` and `[Log] File=`, rebuild-only like `Renderer=`: the log's level and
        // whether it is also written to a file. The command line overrides both.
        if let Some(v) = prefs
            .get("Log.Level")
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            self.log_filter = Some(v.to_string());
        }
        if let Some(v) = prefs.bool("Log.File") {
            self.log_file = v;
        }
        if let Some(v) = display_resolution(prefs) {
            self.display.resolution = v;
            // Device initialization step 7 is
            // loading the display preferences into the presentation and then starting the graphics
            // engine, so the presentation the graphics engine comes up with is the display
            // preferences' resolution's width and height — not the 800x600 used to create the window with.
            // [`Self::load_display_preferences`] is that call; without it a saved
            // `Display.Resolution` has no effect on anything at start-up.
            //
            // **Only when the key is present.** The registered default is 1024x768 and
            // the initial window is 800x600; taking the preference unconditionally would move
            // every default run, and every golden capture with it, to 1024x768. A file that names
            // a resolution gets it; a file that does not keeps the initial window size.
            //
            // Below 800x600 display-preference loading answers `false` and
            // the caller fails; here the size simply does not move, which is the same refusal
            // without taking the client down.
            if let Some(p) = self.load_display_preferences(None, true) {
                self.width = p.width;
                self.height = p.height;
            }
        }
        if let Some(v) = prefs.bool("Display.FullScreen") {
            self.display.full_screen = v;
        }
        if let Some(v) = prefs.u32("Display.RefreshRate") {
            self.display.refresh_rate = v;
        }
        if let Some(v) = prefs.bool("Display.SyncToRefresh") {
            self.display.sync_to_refresh = v;
        }
        // The camera manager's three `Camera.*` registrations,
        // whose preloaded shadow the variable registration applies to the
        // manager's fields, so a saved value reaches the camera and not only the options page.
        if let Some(v) = prefs.bool(crate::camera::CameraPreferences::ALIGN_TO_SLOPE) {
            self.camera.align_to_slope = v;
        }
        if let Some(v) = prefs.f32(crate::camera::CameraPreferences::STIFFNESS) {
            self.camera.stiffness = v;
        }
        if let Some(v) = prefs.f32(crate::camera::CameraPreferences::ADJUSTMENT_SPEED) {
            self.camera.adjustment_speed = v;
        }
        // The input manager's four mouse-look preferences, with their
        // constructor defaults. Camera rotation reads `Input.UseMouseTurning` to decide
        // whether a mouse turn reaches the body at all.
        type Mlp = crate::actions::camera::MouseLookPreferences;
        if let Some(v) = prefs.f32(Mlp::SENSITIVITY) {
            self.mouse_look.sensitivity = v;
        }
        if let Some(v) = prefs.f32(Mlp::SMOOTHING) {
            self.mouse_look.smoothing = v;
        }
        if let Some(v) = prefs.bool(Mlp::INVERT_Y) {
            self.mouse_look.invert_y = v;
        }
        if let Some(v) = prefs.bool(Mlp::USE_MOUSE_TURNING) {
            self.mouse_look.use_mouse_turning = v;
        }
    }

    /// Parse arguments using the original switch table plus
    /// the four rebuild-only switches.
    ///
    /// Errors set the error text and do **not** stop the loop, so the first failure is reported but
    /// every remaining token is still classified; that is why the loop below records `first_error`
    /// rather than returning on the spot.
    fn parse_args(&mut self, argv: &[String]) -> Result<(), ConfigError> {
        let mut first_error: Option<ConfigError> = None;
        // The one switch that collects rather than stores: started afresh, so a second pass over
        // the same command line leaves it as the first did.
        self.say.clear();
        self.set_at.clear();
        self.action_at.clear();
        self.capture_at.clear();
        self.use_targets.clear();
        let mut i = 0usize;
        let fail = |e: ConfigError, slot: &mut Option<ConfigError>| {
            if slot.is_none() {
                *slot = Some(e);
            }
        };

        while i < argv.len() {
            let tok = argv[i].as_str();
            i += 1;

            if !is_switch(tok) {
                // "an *empty* bare token is skipped silently"; the client registers no positional
                // argument (the empty-long-name entry), so anything else is an
                // error.
                if !tok.is_empty() {
                    fail(
                        ConfigError::new(format!("Unrecognized command line argument: {tok}")),
                        &mut first_error,
                    );
                }
                continue;
            }

            // A rebuild-only switch is spelled with two command characters and is resolved before
            // the retail table sees the token.
            let rebuild = tok
                .chars()
                .nth(1)
                .is_some_and(|c| CMD_CHARS.contains(&c))
                .then(|| find(REBUILD_SWITCHES, strip_one(strip_one(tok))))
                .flatten();

            let (table_is_rebuild, sw) = match rebuild {
                Some(s) => (true, Some(s)),
                None => (false, find(SWITCHES, strip_one(tok))),
            };

            let Some(sw) = sw else {
                fail(
                    ConfigError::new(format!("Unrecognized command line argument: {tok}")),
                    &mut first_error,
                );
                continue;
            };

            let value: Option<&str> = match sw.arity {
                Arity::None => None,
                Arity::Required => {
                    // The next token is consumed even if it starts with a command character.
                    match argv.get(i) {
                        Some(v) if !v.is_empty() => {
                            i += 1;
                            Some(v.as_str())
                        }
                        _ => {
                            fail(
                                ConfigError::new(format!("{tok} requires a value")),
                                &mut first_error,
                            );
                            continue;
                        }
                    }
                }
                Arity::Optional => match argv.get(i) {
                    Some(v) if !is_switch(v) => {
                        i += 1;
                        Some(v.as_str())
                    }
                    _ => None,
                },
            };

            let outcome = if table_is_rebuild {
                self.evaluate_rebuild(sw.long, value)
            } else {
                self.evaluate(sw.long, value)
            };
            if let Err(e) = outcome {
                fail(e, &mut first_error);
            }
        }

        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// The two client command-line evaluators,
    /// plus the plain `0x22`/`0x32` conversions the base class performs.
    fn evaluate(&mut self, long: &str, value: Option<&str>) -> Result<(), ConfigError> {
        let v = value.unwrap_or("");
        match long {
            // "the value is copied into ... and then lower-cased in place with
            // _strlwr. Account names are therefore always sent lower case."
            "account" => {
                self.account = v.to_ascii_lowercase();
                self.account_as_typed = v.to_string();
            }
            "debug" => {
                // strtoul(value, 0, 0): 0x... hex and leading-zero octal are accepted.
                self.retail.debug_flags = strtoul_base0(v)
                    .ok_or_else(|| ConfigError::new(format!("bad -debug value {v:?}")))?;
            }
            "host" => self.host = v.to_string(),
            "language" => self.retail.language = v.to_string(),
            "outport" => {
                self.client_port = strtol(v);
                // "the handler range-checks 1 <= client_port <= 65535 and fails parsing with
                // `Client port must be between 1 and 65535\n` otherwise."
                if !(1..=65535).contains(&self.client_port) {
                    return Err(ConfigError::new(
                        "Client port must be between 1 and 65535\n",
                    ));
                }
            }
            // "-port is not range-checked despite its description."
            "port" => self.port = strtol(v),
            "prefs" => {
                self.preferences_file = PathBuf::from(v);
                self.preferences_named = true;
            }
            // "the handler is read_only_dat_files = (value is the empty string). Bare -rodat turns
            // read-only dats on; -rodat <anything> turns them off. The text of the value is never
            // examined, so `-rodat on` also disables read-only mode."
            "rodat" => self.retail.read_only_dat_files = v.is_empty(),
            "usemem" => self.retail.use_memory_manager = true,
            "user" => self.start_char = v.to_string(),
            "create" => self.create_char = v.to_string(),
            "zoneticket" => self.retail.zone_ticket = v.to_string(),
            "glsticketdirect" => self.gls_ticket = v.to_string(),
            // The registry read (`HKCU\Software\Turbine\ac1\GLSTicket`, read then deleted) belongs
            // to the login path; the switch itself only records that GLS authentication was asked
            // for.
            "glsticket" => self.retail.use_gls = true,
            "migrationurl" => {
                if !v.is_empty() {
                    self.retail.migration_url = v.to_string();
                }
            }
            "vgpassword" => self.vg_password = v.to_string(),
            other => return Err(ConfigError::new(format!("unhandled switch {other}"))),
        }
        Ok(())
    }

    fn evaluate_rebuild(&mut self, long: &str, value: Option<&str>) -> Result<(), ConfigError> {
        let v = value.unwrap_or("");
        match long {
            "headless" => self.headless = true,
            // `--prefs <file>`, the doubled spelling of the retail `-prefs`, so a rebuild-only
            // command line can name its settings file in the same style as its other switches.
            "prefs" => return self.evaluate("prefs", value),
            // `--renderer vulkan|d3d12|wgpu`. Parsed and validated here; whether
            // the named backend is in this build is decided at device creation, which logs a line
            // and falls back rather than failing (`App::device_presentation`).
            "renderer" => {
                self.renderer = Some(dereth_client_contract::RendererChoice::parse(v).ok_or_else(
                    || {
                        ConfigError::new(format!(
                            "--renderer wants vulkan, d3d12 or wgpu, not {v:?}"
                        ))
                    },
                )?);
            }
            "frames" => {
                self.frames = Some(
                    v.parse::<u64>()
                        .map_err(|_| ConfigError::new(format!("bad --frames value {v:?}")))?,
                );
            }
            "capture" => self.capture = Some(PathBuf::from(v)),
            // Recorded only; `main` has already acted on it. See [`Config::console`].
            "no-console" => self.console = false,
            // Acted on by `main` before the parse (see [`crash_test_in_argv`]); nothing to record.
            "crash-test" => {}
            // Recorded only; the binary reads these when it installs the log.
            "log" => self.log_filter = Some(v.to_string()),
            "log-file" => self.log_file = true,
            "log-spans" => self.log_spans = true,
            "dat-dir" => self.dat_dir = PathBuf::from(v),
            "classic-dat-dir" => self.classic_dat_dir = Some(PathBuf::from(v)),
            "overlay-dat-dir" => self.overlay_dat_dir = Some(PathBuf::from(v)),
            "object-visuals" => {
                let style = dereth_client_contract::options::landscape::parse(v)
                    .ok_or_else(|| {
                        ConfigError::new(format!(
                            "--object-visuals takes world, legacy or modern, not {v:?}"
                        ))
                    })?
                    .map(crate::render_prefs::objects_style);
                self.render.objects = style;
                self.object_visuals = Some(style);
            }
            "set-at" => {
                let (frame, setting) = v
                    .split_once(':')
                    .and_then(|(f, rest)| Some((f.trim().parse::<u64>().ok()?, rest)))
                    .filter(|(_, rest)| rest.contains('='))
                    .ok_or_else(|| {
                        ConfigError::new(format!(
                            "bad --set-at value {v:?}: expected <frame>:<Section.Name>=<value>"
                        ))
                    })?;
                self.set_at.push((frame, setting.to_string()));
            }
            "action-at" => {
                let (frame, action) = v
                    .split_once(':')
                    .and_then(|(f, rest)| {
                        let id = dereth_client_contract::actions::names::action_for_enum_name(
                            rest.trim(),
                        )?;
                        Some((f.trim().parse::<u64>().ok()?, id.0))
                    })
                    .ok_or_else(|| {
                        ConfigError::new(format!(
                            "bad --action-at value {v:?}: expected <frame>:<ActionName>"
                        ))
                    })?;
                self.action_at.push((frame, action));
            }
            "capture-at" => {
                let (frame, path) = v
                    .split_once(':')
                    .and_then(|(f, rest)| Some((f.trim().parse::<u64>().ok()?, rest)))
                    .filter(|(_, rest)| !rest.is_empty())
                    .ok_or_else(|| {
                        ConfigError::new(format!(
                            "bad --capture-at value {v:?}: expected <frame>:<path>"
                        ))
                    })?;
                self.capture_at.push((frame, PathBuf::from(path)));
            }
            "era" => {
                self.era = Some(
                    dereth_primitives::EraId::parse(v)
                        .ok_or_else(|| ConfigError::new(format!("unknown --era {v:?}")))?,
                );
            }
            "era-features" => {
                let (features, unknown) = dereth_primitives::EraFeatureOverrides::parse(v)
                    .map_err(|e| ConfigError::new(format!("bad --era-features: {e}")))?;
                if !unknown.is_empty() {
                    tracing::warn!(
                        "--era-features names systems this client does not know: {unknown:?}"
                    );
                }
                self.era_features = features;
            }
            "landblock" => {
                self.landblock = u16::from_str_radix(v.trim_start_matches("0x"), 16)
                    .map_err(|_| ConfigError::new(format!("bad --landblock value {v:?}")))?;
            }
            // `--landblock` alone puts the offline body on the terrain, which for a
            // dungeon block is twelve metres of rock above the rooms; this names an interior cell
            // of it instead. Sets `--landblock` too, because a cell id carries its own block.
            "start-cell" => {
                let id = u32::from_str_radix(v.trim_start_matches("0x"), 16)
                    .map_err(|_| ConfigError::new(format!("bad --start-cell value {v:?}")))?;
                if id & 0xFFFF < 0x0100 {
                    return Err(ConfigError::new(format!(
                        "--start-cell wants an interior cell (0xXXXX01xx and up), not {v:?}"
                    )));
                }
                self.start_cell = Some(id);
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: the landblock is the top 16 bits of a cell id. Not a float conversion.
                {
                    self.landblock = (id >> 16) as u16;
                }
            }
            "land-radius" => {
                self.land_radius = v
                    .parse::<u32>()
                    .map_err(|_| ConfigError::new(format!("bad --land-radius value {v:?}")))?
                    .min(15);
            }
            "stream-budget-ms" => {
                self.stream_budget_ms = Some(v.parse::<u32>().map_err(|_| {
                    ConfigError::new(format!("bad --stream-budget-ms value {v:?}"))
                })?);
            }
            "object-identity-ms" => {
                self.object_identity_ms = Some(v.parse::<u32>().map_err(|_| {
                    ConfigError::new(format!("bad --object-identity-ms value {v:?}"))
                })?);
            }
            "scenery-radius" => {
                self.scenery_radius = v
                    .parse::<u32>()
                    .map_err(|_| ConfigError::new(format!("bad --scenery-radius value {v:?}")))?
                    .min(15);
            }
            "linger" => {
                self.linger = v
                    .parse::<f64>()
                    .map_err(|_| ConfigError::new(format!("bad --linger value {v:?}")))?
                    .clamp(0.0, 3600.0);
            }
            "cast" => {
                self.cast = Some(
                    v.parse::<u32>()
                        .map_err(|_| ConfigError::new(format!("bad --cast value {v:?}")))?,
                );
            }
            "say" => self.say.push(v.to_owned()),
            "use" => {
                let id = v.strip_prefix("0x").map(|h| u32::from_str_radix(h, 16));
                if v != "closest" && v != "logout" && !matches!(id, Some(Ok(_))) {
                    return Err(ConfigError::new(format!(
                        "--use takes an object id (0x...), closest or logout, not {v:?}"
                    )));
                }
                self.use_targets.push(v.to_owned());
            }
            "time-of-day" => {
                let f = v
                    .parse::<f32>()
                    .map_err(|_| ConfigError::new(format!("bad --time-of-day value {v:?}")))?;
                if !(0.0..1.0).contains(&f) {
                    return Err(ConfigError::new(format!(
                        "--time-of-day is a fraction of the day in [0, 1), not {f}"
                    )));
                }
                self.time_of_day = Some(f);
            }
            // The UI is on by default, as the retail client's is. `--ui` stays
            // accepted so older scripts that pass it keep working.
            "ui" => self.ui = true,
            "no-ui" => self.ui = false,
            "no-connect" => self.connect = false,
            "no-sound" => self.sound = false,
            "ui-mode" => {
                self.ui = true;
                self.ui_mode = Some(
                    u32::from_str_radix(v.trim_start_matches("0x"), 16)
                        .map_err(|_| ConfigError::new(format!("bad --ui-mode value {v:?}")))?,
                );
            }
            "no-world" => self.world = false,
            "no-character" => self.character = false,
            // Leave the interior cells' baked objects out, so the same station can be
            // looked at with and without them. Always on in the client.
            "no-cell-statics" => self.cell_statics = false,
            // The control half of the mesh-collision differential.
            "no-mesh-collision" => self.mesh_collision = false,
            "auto-degrades" => self.auto_degrades = true,
            "no-auto-degrades" => self.auto_degrades = false,
            // As `--ui`: accepted, and now the default. See [`Config::connect`].
            "connect" => self.connect = true,
            // Entering the world without connecting is not a state that exists.
            "enter-world" => {
                self.enter_world = true;
                self.connect = true;
            }
            other => return Err(ConfigError::new(format!("unhandled switch {other}"))),
        }
        Ok(())
    }

    /// Whether this run will open a socket at all.
    ///
    /// Connecting is the default, as it is the retail client's only behaviour; what
    /// makes a run offline is having no credentials to connect *with*. A bare `dereth-client` draws
    /// the world with no server; `dereth-client -a ac01 -v pass -h 127.0.0.1:19000` is the ordinary
    /// invocation. Giving one of `-a`/`-h` without the other is still
    /// `Config::require_account_and_host`'s documented error rather than a silent offline start,
    /// which is why this tests for *both* being absent.
    ///
    /// Running offline at all is a developer affordance and not a published divergence: the retail
    /// client refuses to start without an account and a host, so it always connects.
    #[must_use]
    pub fn will_connect(&self) -> bool {
        self.connect && !(self.account.is_empty() && self.host.is_empty())
    }

    /// This is not called by the parser: see the module
    /// documentation. The login path is what must call it.
    ///
    /// # Errors
    /// [`ConfigError`] when either the account or the host is empty.
    pub fn require_account_and_host(&self) -> Result<(), ConfigError> {
        if self.account.is_empty() {
            return Err(ConfigError::new("You must specify an account name"));
        }
        if self.host.is_empty() {
            return Err(ConfigError::new("You must specify a host name"));
        }
        Ok(())
    }

    /// Load display preferences into the presentation record, including
    /// its "returns false and the caller then fails" rejection of anything below 800x600.
    ///
    /// `use_forced_resolution` selects the forced-resolution branch, and `allow_full_screen` controls
    /// whether full screen is allowed. Both values live in the device rather than in the config.
    #[must_use]
    pub fn load_display_preferences(
        &self,
        forced: Option<(u32, u32)>,
        allow_full_screen: bool,
    ) -> Option<Presentation> {
        let w = self.display.resolution >> 16;
        let h = self.display.resolution & 0xFFFF;
        if w < 800 || h < 600 {
            return None;
        }
        let mut p = Presentation {
            width: w,
            height: h,
            full_screen: self.display.full_screen,
            compatibility: PresentationFlags {
                fs_refresh_rate: self.display.refresh_rate,
                fs_triple_buffering: self.display.triple_buffering,
                fs_sync_to_display_refresh: self.display.sync_to_refresh,
                antialiasing: self.display.antialiasing,
                ..PresentationFlags::default()
            },
        };
        if let Some((fw, fh)) = forced {
            p.width = fw;
            p.height = fh;
        }
        if !allow_full_screen {
            p.full_screen = false;
        }
        Some(p)
    }
}

/// String conversion for `Display.Resolution`, which is the one
/// registered preference in this file whose saved form is **not** a number.
///
/// Saving user preferences calls the string conversion
/// ([`dereth_client_contract::options::store::convert_to_string`]), whose first arm is
/// *"if the preference has a choice list then write the matching label"* — and
/// `Display.Resolution`'s labels are the `"%ix%i"` strings built during display-preference
/// initialization, so a retail profile holds `Resolution=1280x1024`. [`Preferences::u32`] is
/// `strtoul`, which answers `None` for that, so on its own it would silently drop the saved size
/// and leave the registered default standing.
///
/// The decode is [`dereth_client_contract::options::store::display_choice`] — the same function the
/// option store uses for its display labels — with `strtoul` kept
/// behind it as the string-to-value conversion's own no-label-matched fall-back, which is what a hand-edited
/// `Resolution=83886800` takes.
fn display_resolution(prefs: &Preferences) -> Option<u32> {
    let raw = prefs.get("Display.Resolution")?;
    if let Some(v) =
        dereth_client_contract::options::store::display_choice("Display.Resolution", raw)
    {
        return Some(u32::from_ne_bytes(v.to_ne_bytes()));
    }
    strtoul_base0(raw)
}

/// `strtol(value, 0, 0)` as the `0x22` conversion applies it, clamped into the `u32` the two port
/// fields are. A token that does not parse gives 0, which is what `strtol` returns.
fn strtol(v: &str) -> u32 {
    strtoul_base0(v).unwrap_or(0)
}

/// `strtoul(value, 0, 0)`: base 16 for a `0x` prefix, base 8 for a leading zero, else base 10.
fn strtoul_base0(v: &str) -> Option<u32> {
    let t = v.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let parsed = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(h, 16).ok()?
    } else if t.len() > 1 && t.starts_with('0') {
        u32::from_str_radix(&t[1..], 8).ok()?
    } else {
        t.parse::<u32>().ok()?
    };
    Some(if neg { parsed.wrapping_neg() } else { parsed })
}

/// The name of the preferences file itself, in the client's settings directory. The desktop host
/// decides where that directory is; every other file the client saves is placed beside this one.
///
/// Keep the basename in one constant because
/// the cwd probe and the settings-directory branch must agree on it or a portable install would
/// write one file and read another.
pub const PREFERENCES_FILE_NAME: &str = "UserPreferences.ini";

/// The directories the retail dats are looked for in when `--dat-dir` is not given: the working
/// directory, then the directory holding the executable.
///
/// Cache initialization locates `client_portal.dat` by searching the working directory first;
/// the executable's own directory is where an install that is started from elsewhere keeps them.
#[must_use]
pub fn dat_dir_candidates() -> Vec<PathBuf> {
    let mut dirs = vec![std::env::current_dir().unwrap_or_default()];
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        dirs.push(exe_dir);
    }
    dirs
}

/// Where the four retail dats are when `--dat-dir` is not given: the first of
/// [`dat_dir_candidates`] that holds them, or the working directory when none does (so the error
/// the open reports names it).
#[must_use]
pub fn default_dat_dir() -> PathBuf {
    let candidates = dat_dir_candidates();
    match dereth_dat::locate_retail_dats(&candidates) {
        Ok(dir) => dir.into_path_buf(),
        Err(_) => candidates.into_iter().next().unwrap_or_default(),
    }
}

/// `UserPreferences.ini`, a plain Win32 profile file.
///
/// Preference loading reads it with `GetPrivateProfileSectionNamesA` /
/// `GetPrivateProfileSectionA` and pushes every `"<Section>.<Key>"` through
/// the registry setter. Names are stored lower-cased, so lookups are
/// case-insensitive; that is reproduced by lower-casing the key on insertion and on lookup.
#[derive(Debug, Default, Clone)]
pub struct Preferences {
    // ORDER-OK: a map keyed by the preference name and only ever looked up by name. Loading walks it
    // in file order and saving in registry order, neither of which is observable here.
    values: BTreeMap<String, String>,
    /// The user-preference load-success flag.
    pub loaded_ok: bool,
}

impl Preferences {
    /// Load one file. **A missing file is not an error**: step 2 of loading says "a zero return ends the
    /// load", and client initialization ignores the result entirely (step 8 of
    /// client initialization: "Return value **ignored**").
    #[must_use]
    pub fn load(path: &Path) -> Self {
        match crate::platform::files::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(_) => Self::default(),
        }
    }

    /// The profile parse, including the documented `=`-in-a-value corruption.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut values = BTreeMap::new();
        let mut section = String::from("Default");
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name) = rest.strip_suffix(']') {
                    section = name.to_string();
                }
                continue;
            }
            // Each line is split on '=': 1 part -> value "";
            // 2 parts -> key/value; more than 2 parts -> the value is the remaining parts
            // concatenated **without** the `=` separators, so a value containing `=` is corrupted
            // on load.
            let mut parts = line.split('=');
            let Some(key) = parts.next() else { continue };
            let value: String = parts.collect();
            values.insert(format!("{section}.{key}").to_ascii_lowercase(), value);
        }
        Self {
            values,
            loaded_ok: true,
        }
    }

    /// The raw string a name was loaded with.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// A preference registered with the unsigned 32-bit data type.
    #[must_use]
    pub fn u32(&self, name: &str) -> Option<u32> {
        self.get(name).and_then(strtoul_base0)
    }

    /// A preference registered with the 32-bit float data type. Its string-to-value conversion
    /// float arm is `atof`, so a value with trailing text keeps its numeric prefix and an
    /// unparsable one reads as 0 -- here `None`, so the caller keeps its registered default,
    /// which is what a fresh profile with the key missing gets in retail too.
    #[must_use]
    pub fn f32(&self, name: &str) -> Option<f32> {
        let s = self.get(name)?.trim();
        let end = s
            .char_indices()
            .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0)))
            .map_or(s.len(), |(i, _)| i);
        s[..end].parse::<f32>().ok()
    }

    /// A preference registered with the boolean data type.
    /// Accepts case-insensitive `True`/`False` or numeric `1`/`0` after trimming whitespace;
    /// other values return `None`.
    #[must_use]
    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.get(name)?.trim() {
            v if v.eq_ignore_ascii_case("true") || v == "1" => Some(true),
            v if v.eq_ignore_ascii_case("false") || v == "0" => Some(false),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: none (the object identity budget distinguishes connected and offline startup).
    #[test]
    fn connected_clients_work_out_the_object_identity_in_the_background_unless_told_not_to() {
        let mut cfg = Config::default();
        assert_eq!(cfg.scene_config().object_identity_budget, None);
        assert!(cfg.scene_config().object_identity_cache);
        cfg.account = "account".into();
        cfg.host = "127.0.0.1".into();
        assert_eq!(
            cfg.scene_config().object_identity_budget,
            Some(crate::app::IDENTITY_BACKGROUND_BUDGET)
        );
        cfg.object_identity_ms = Some(0);
        assert_eq!(cfg.scene_config().object_identity_budget, None);
        cfg.object_identity_ms = Some(5);
        cfg.connect = false;
        assert_eq!(
            cfg.scene_config().object_identity_budget,
            Some(std::time::Duration::from_millis(5))
        );
        let mut parsed = Config::default();
        parsed
            .parse_args(&["--object-identity-ms".to_string(), "2".to_string()])
            .expect("the switch parses");
        assert_eq!(parsed.object_identity_ms, Some(2));
    }

    /// Behaviour: none (scene streaming budgets distinguish connected and offline startup).
    #[test]
    fn connected_scenes_stream_unless_the_budget_is_explicitly_disabled() {
        let mut cfg = Config::default();
        assert_eq!(cfg.scene_config().stream_budget, None);
        cfg.account = "account".into();
        cfg.host = "127.0.0.1".into();
        assert_eq!(
            cfg.scene_config().stream_budget,
            Some(std::time::Duration::from_millis(4))
        );
        cfg.stream_budget_ms = Some(0);
        assert_eq!(cfg.scene_config().stream_budget, None);
        cfg.stream_budget_ms = Some(9);
        assert_eq!(
            cfg.scene_config().stream_budget,
            Some(std::time::Duration::from_millis(9))
        );
        cfg.connect = false;
        assert_eq!(
            cfg.scene_config().stream_budget,
            Some(std::time::Duration::from_millis(9))
        );
        cfg.stream_budget_ms = None;
        assert_eq!(cfg.scene_config().stream_budget, None);
    }

    /// Behaviour: none (scene startup carries profile values and the selected terrain policy).
    #[test]
    fn scene_startup_keeps_profile_values_and_each_terrain_mode() {
        let mut cfg = Config {
            landblock: 0xA9B4,
            start_cell: Some(0xA9B4_0100),
            land_radius: 3,
            scenery_radius: 2,
            time_of_day: Some(0.25),
            ..Config::default()
        };
        cfg.render.field_of_view = 75.0;
        for (mode, merge, splat) in [
            (crate::render_prefs::TerrainBlending::Cpu, false, false),
            (crate::render_prefs::TerrainBlending::Gpu, true, false),
            (crate::render_prefs::TerrainBlending::Splat, true, true),
        ] {
            cfg.terrain_blending = mode;
            let scene = cfg.scene_config();
            assert_eq!(scene.landblock, 0xA9B4);
            assert_eq!(
                scene.start_cell,
                Some(dereth_primitives::CellId(0xA9B4_0100))
            );
            assert_eq!((scene.land_radius, scene.scenery_radius), (3, 2));
            assert_eq!(scene.time_of_day, Some(0.25));
            assert_eq!(scene.render, cfg.render);
            assert_eq!(scene.camera, cfg.camera);
            assert_eq!(scene.mouse_look, cfg.mouse_look);
            assert_eq!(
                (scene.gpu_terrain_merge, scene.terrain_splat),
                (merge, splat)
            );
        }
    }

    fn parse(args: &[&str]) -> Result<Config, ConfigError> {
        let argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        Config::from_args_and_prefs_with(&argv, &Preferences::default())
    }

    /// `[Render] Ground` and `[Render] Sky` choose a style each, read in any of their spellings
    /// (the older `software`, `hardware` and `later` included); absent, or anything else, is the
    /// world's own.
    #[test]
    fn the_ground_and_sky_preferences_choose_a_style_and_default_to_the_worlds_own() {
        use crate::render_prefs::RegionStyle;
        let styles = |text: &str| {
            let s = Config::from_args_and_prefs_with(&[], &Preferences::parse(text))
                .expect("parses")
                .scene_config();
            (s.render.ground, s.render.sky)
        };
        assert_eq!(
            styles("[Render]\nGround=PaletteShift\nSky=Legacy Hardware\n"),
            (
                Some(RegionStyle::LegacySoftware),
                Some(RegionStyle::LegacyHardware)
            )
        );
        assert_eq!(
            styles("[Render]\nGround=LegacyBlend\nSky=modern\n"),
            (Some(RegionStyle::LegacyHardware), Some(RegionStyle::Modern))
        );
        assert_eq!(
            styles("[Render]\nGround=LATER\n"),
            (Some(RegionStyle::Modern), None)
        );
        assert_eq!(
            styles("[Render]\nGround=software\n"),
            (Some(RegionStyle::LegacySoftware), None)
        );
        assert_eq!(styles("[Render]\nGround=hardware\n"), (None, None));
        assert_eq!(styles("[Render]\nGround=tod\nSky=World\n"), (None, None));
        assert_eq!(styles(""), (None, None));
    }

    /// `[Render] Objects` chooses the objects' look, and `--object-visuals` wins over it; either
    /// older style is the one older look.
    #[test]
    fn the_object_visuals_come_from_the_switch_before_the_preference() {
        use crate::render_prefs::RegionStyle;
        let objects = |args: &[&str], text: &str| {
            let argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
            let c =
                Config::from_args_and_prefs_with(&argv, &Preferences::parse(text)).expect("parses");
            (c.scene_config().render.objects, c.object_visuals)
        };
        assert_eq!(objects(&[], ""), (None, None));
        assert_eq!(
            objects(&[], "[Render]\nObjects=Legacy\n"),
            (Some(RegionStyle::LegacyHardware), None)
        );
        assert_eq!(
            objects(&[], "[Render]\nObjects=PaletteShift\n"),
            (Some(RegionStyle::LegacyHardware), None)
        );
        assert_eq!(
            objects(
                &["--object-visuals", "modern"],
                "[Render]\nObjects=Legacy\n"
            ),
            (Some(RegionStyle::Modern), Some(Some(RegionStyle::Modern)))
        );
        assert_eq!(
            objects(&["--object-visuals", "world"], "[Render]\nObjects=Legacy\n"),
            (None, Some(None))
        );
        let argv = vec!["--object-visuals".to_string(), "sideways".to_string()];
        assert!(Config::from_args_and_prefs_with(&argv, &Preferences::parse("")).is_err());
    }

    /// Behaviour: none (tooling: the switch only says where the older data files are)
    #[test]
    fn the_classic_dat_dir_switch_names_where_the_older_files_are_and_is_optional() {
        let c = parse(&["--dat-dir", "eor", "--classic-dat-dir", "feb2005"]).expect("parses");
        assert_eq!(c.dat_dir, PathBuf::from("eor"));
        assert_eq!(c.classic_dat_dir, Some(PathBuf::from("feb2005")));
        assert_eq!(parse(&[]).expect("parses").classic_dat_dir, None);
        let c = parse(&["--overlay-dat-dir", "worlds/one"]).expect("parses");
        assert_eq!(c.overlay_dat_dir, Some(PathBuf::from("worlds/one")));
        for retired in ["--world-dat-dir", "--legacy-dat-dir"] {
            assert!(parse(&[retired, "x"]).is_err(), "{retired}");
        }
    }

    /// `--set-at` and `--capture-at` collect a frame and what to do there, in order.
    #[test]
    fn a_run_can_set_a_preference_and_take_a_picture_part_way_through() {
        let c = parse(&[
            "--capture-at",
            "4:before.png",
            "--set-at",
            "5:Render.Ground=PaletteShift",
            "--capture-at",
            "9:after.png",
        ])
        .expect("parses");
        assert_eq!(c.set_at, [(5, "Render.Ground=PaletteShift".to_string())]);
        assert_eq!(
            c.capture_at,
            [
                (4, PathBuf::from("before.png")),
                (9, PathBuf::from("after.png"))
            ]
        );
        assert!(parse(&["--set-at", "five:Render.Ground=PaletteShift"]).is_err());
        assert!(parse(&["--set-at", "5:Render.Ground"]).is_err());
        assert!(parse(&["--capture-at", "5:"]).is_err());
    }

    /// `--action-at` collects a frame and an action named as a key map names it; a name no action
    /// has is refused.
    #[test]
    fn a_run_can_press_an_action_part_way_through() {
        let c = parse(&["--action-at", "7:ToggleAllegiancePanel"]).expect("parses");
        assert_eq!(c.action_at, [(7, 0x1000_000E)]);
        assert!(parse(&["--action-at", "7:NoSuchAction"]).is_err());
        assert!(parse(&["--action-at", "seven:ToggleAllegiancePanel"]).is_err());
    }

    /// `--era` names the era the server's world plays; an unknown name is refused.
    #[test]
    fn the_era_switch_names_the_servers_era() {
        let c = parse(&["--era", "Infiltration"]).expect("parses");
        assert_eq!(c.era, Some(dereth_primitives::EraId::Infiltration));
        assert_eq!(parse(&[]).expect("parses").era, None);
        assert!(parse(&["--era", "tod"]).is_err());
    }

    /// `--era-features` names the systems the server's world has; a system this client does not
    /// know is skipped, and a malformed list is refused.
    #[test]
    fn the_era_features_switch_names_the_servers_systems() {
        let c = parse(&[
            "--era-features",
            "trade=false,aetheria=true,spell_credits=true",
        ])
        .expect("parses");
        assert_eq!(c.era_features.get("trade"), Some(false));
        assert_eq!(c.era_features.get("aetheria"), Some(true));
        assert_eq!(c.era_features.get("chess"), None);
        assert!(parse(&[]).expect("parses").era_features.is_empty());
        assert!(parse(&["--era-features", "trade"]).is_err());
    }

    /// `--cast` names the spell a scripted world entry casts; it takes a spell id.
    #[test]
    fn the_cast_switch_names_the_spell_a_scripted_entry_casts() {
        assert_eq!(parse(&["--cast", "35"]).expect("parses").cast, Some(35));
        assert_eq!(parse(&[]).expect("parses").cast, None);
        assert!(parse(&["--cast", "blood"]).is_err());
    }

    /// `--say` lines and `--use` targets are kept in the order given.
    #[test]
    fn the_say_switch_keeps_each_line_in_order() {
        let c = parse(&["--say", "@level 2", "--say", "hello there"]).expect("parses");
        assert_eq!(c.say, ["@level 2", "hello there"]);
        assert!(parse(&[]).expect("parses").say.is_empty());
        let argv: Vec<String> = ["--headless", "--say", "@level 2"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let at = Config::from_args_and_prefs_at(&argv, Path::new(""))
            .expect("parses")
            .say;
        assert_eq!(
            at,
            ["@level 2"],
            "the command line is read twice, the line kept once"
        );
        let c =
            parse(&["--use", "0x7A9B0000", "--use", "closest", "--use", "logout"]).expect("parses");
        assert_eq!(c.use_targets, ["0x7A9B0000", "closest", "logout"]);
        assert!(parse(&["--use", "door"]).is_err());
    }

    // Oracle: the complete supported switch table and its per-switch behavior.
    #[test]
    fn the_documented_switches_parse_to_the_documented_values() {
        let c = parse(&[
            "-a",
            "Ac01",
            "-h",
            "127.0.0.1",
            "-p",
            "19000",
            "-q",
            "9000",
            "-language",
            "English",
            "-usemem",
            "-u",
            "Aren",
            "-r",
            "Aldis",
            "-z",
            "tkt",
            "-vgpassword",
            "pw",
        ])
        .expect("parses");
        // "-a / -account -- the value is ... lower-cased in place with _strlwr."
        assert_eq!(c.account, "ac01");
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 19000);
        assert_eq!(c.client_port, 9000);
        assert_eq!(c.retail.language, "English");
        assert!(c.retail.use_memory_manager);
        assert_eq!(c.start_char, "Aren");
        assert_eq!(c.create_char, "Aldis");
        assert_eq!(c.retail.zone_ticket, "tkt");
        assert_eq!(c.vg_password, "pw");
    }

    #[test]
    fn the_defaults_are_the_constructed_ones() {
        let c = parse(&[]).expect("an empty command line is legal at this layer");
        assert_eq!(c.port, 7304, "the client default port is 0x1C88");
        assert_eq!(c.client_port, 0);
        assert!(c.retail.read_only_dat_files, "read-only dats default on");
        assert!(c.windowed, "windowed defaults on");
        assert_eq!(
            (c.width, c.height),
            (800, 600),
            "device initialization defaults to 800 by 600"
        );
        assert_eq!(
            c.display.resolution, 0x0400_0300,
            "the compiled-in default is 1024x768"
        );
        assert!(!c.display.full_screen, "the client starts in a window");
        // The login path owns the account/host requirement; the parser only records values.
        assert!(c.require_account_and_host().is_err());
    }

    // "-rodat -- the handler is read_only_dat_files = (value is the empty string). Bare -rodat
    // turns read-only dats **on**; -rodat <anything> (the launcher passes `off`) turns them
    // **off**. The text of the value is never examined, so `-rodat on` also *disables* read-only
    // mode."
    #[test]
    fn rodat_is_backwards_and_stays_backwards() {
        assert!(parse(&["-rodat"]).unwrap().retail.read_only_dat_files);
        assert!(
            !parse(&["-rodat", "off"])
                .unwrap()
                .retail
                .read_only_dat_files
        );
        assert!(!parse(&["-rodat", "on"]).unwrap().retail.read_only_dat_files);
        // Value optional: a following switch is re-processed rather than consumed.
        let c = parse(&["-rodat", "-usemem"]).unwrap();
        assert!(c.retail.read_only_dat_files);
        assert!(c.retail.use_memory_manager);
    }

    /// The renderer is selected by the switch over the preference over the default.
    #[test]
    fn the_renderer_is_selected_by_the_switch_over_the_preference_over_the_default() {
        use dereth_client_contract::RendererChoice as Backend;

        // Neither: the choice is left open, which `App::device_presentation` reads as
        // the default.
        assert_eq!(parse(&[]).unwrap().renderer, None);

        // The switch, in both spellings the parser accepts, and case-insensitively.
        assert_eq!(
            parse(&["--renderer", "d3d12"]).unwrap().renderer,
            Some(Backend::D3d12)
        );
        assert_eq!(
            parse(&["--renderer", "vulkan"]).unwrap().renderer,
            Some(Backend::Vulkan)
        );
        assert_eq!(
            parse(&["--RENDERER", "D3D12"]).unwrap().renderer,
            Some(Backend::D3d12)
        );

        // A name that is not a backend is a parse error, not a silent default.
        let e = parse(&["--renderer", "opengl"]).expect_err("opengl is not a backend");
        assert!(
            e.detail.contains("--renderer wants vulkan, d3d12 or wgpu"),
            "{}",
            e.detail
        );

        // The preference, in the `[Render]` section and with no section at all.
        let p = Preferences::parse(
            "[Render]
Renderer=d3d12
",
        );
        let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
        assert_eq!(c.renderer, Some(Backend::D3d12));
        let p = Preferences::parse(
            "Renderer=d3d12
",
        );
        let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
        assert_eq!(c.renderer, Some(Backend::D3d12));

        // The switch wins over the preference: `from_args_and_prefs_with` applies the profile and
        // then the command line, which is the order the startup path uses.
        let p = Preferences::parse(
            "[Render]
Renderer=d3d12
",
        );
        let argv = vec!["--renderer".to_string(), "vulkan".to_string()];
        let c = Config::from_args_and_prefs_with(&argv, &p).unwrap();
        assert_eq!(c.renderer, Some(Backend::Vulkan));

        // A preference naming nothing this client knows leaves the choice open rather than
        // failing the start-up, as an unparsable numeric preference leaves its default standing.
        let p = Preferences::parse(
            "[Render]
Renderer=glide
",
        );
        assert_eq!(
            Config::from_args_and_prefs_with(&[], &p).unwrap().renderer,
            None
        );
    }

    /// The log's three switches and its two preferences, with the command line winning.
    #[test]
    fn the_log_is_configured_by_switch_and_by_preference() {
        let c = parse(&[]).unwrap();
        assert_eq!(
            (c.log_filter.as_deref(), c.log_file, c.log_spans),
            (None, false, false)
        );

        let c = parse(&[
            "--log",
            "info,dereth_client_runtime::frame=trace",
            "--log-file",
            "--log-spans",
        ])
        .unwrap();
        assert_eq!(
            c.log_filter.as_deref(),
            Some("info,dereth_client_runtime::frame=trace")
        );
        assert!(c.log_file && c.log_spans);
        // `--log` takes a value; with none it is a parse error like any other required value.
        assert!(parse(&["--log"]).is_err());

        let p = Preferences::parse("[Log]\nLevel=debug\nFile=True\n");
        let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
        assert_eq!((c.log_filter.as_deref(), c.log_file), (Some("debug"), true));
        let argv = vec!["--log".to_string(), "warn".to_string()];
        let c = Config::from_args_and_prefs_with(&argv, &p).unwrap();
        assert_eq!(c.log_filter.as_deref(), Some("warn"));
        // An empty preference is no preference.
        let p = Preferences::parse("[Log]\nLevel=\n");
        let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
        assert_eq!(c.log_filter, None);
    }

    /// Behaviour: none (tooling: what the crash log may carry of the command line).
    #[test]
    fn a_logged_command_line_hides_the_account_password_and_tickets() {
        let argv = [
            "dereth-client",
            "-a",
            "acct",
            "-v",
            "secret",
            "-h",
            "host:9000",
            "--vgpassword",
            "again",
            "/z",
            "ticket",
            "-glsticketdirect",
            "gls",
            "-ACCOUNT",
            "loud",
            "-A",
            "upper",
            "-glsticket",
            "--headless",
            "-u",
            "-v",
        ];
        assert_eq!(
            argv_for_log(&argv),
            [
                "dereth-client",
                "-a",
                "***",
                "-v",
                "***",
                "-h",
                "host:9000",
                "--vgpassword",
                "***",
                "/z",
                "***",
                "-glsticketdirect",
                "***",
                "-ACCOUNT",
                "***",
                "-A",
                "***",
                "-glsticket",
                "--headless",
                "-u",
                "-v",
            ]
        );
    }

    // Client initialization sets the command characters to "-/", so both prefixes are accepted.
    #[test]
    fn both_command_characters_are_accepted() {
        assert_eq!(parse(&["/host", "example"]).unwrap().host, "example");
        assert_eq!(parse(&["-host", "example"]).unwrap().host, "example");
    }

    // "Exactly one leading command character is stripped, so --rodat is looked up as the long name
    // -rodat and fails."
    #[test]
    fn exactly_one_command_character_is_stripped() {
        let e = parse(&["--rodat"]).expect_err("--rodat is not a name");
        assert!(
            e.detail.contains("Unrecognized command line argument"),
            "{}",
            e.detail
        );
        // ... and every command-line error surfaces as corestrings 205.
        assert_eq!(
            e.to_string(),
            "You can only run the game from the launcher program."
        );
    }

    // "otherwise long-name lookup (case **insensitive**)"; short names are case sensitive.
    #[test]
    fn long_names_are_case_insensitive_and_short_names_are_not() {
        assert_eq!(parse(&["-HOST", "h"]).unwrap().host, "h");
        assert_eq!(parse(&["-a", "X"]).unwrap().account, "x");
        // 'A' is not a registered short name, and "A" is not a long name either.
        assert!(parse(&["-A", "X"]).is_err());
    }

    // "2 | **requires** a value; the next token is consumed as the value even if it starts with a
    // command character. An empty token errors with `%S requires a value`"
    #[test]
    fn a_required_value_swallows_the_next_token_whatever_it_is() {
        let c = parse(&["-h", "-usemem"]).unwrap();
        assert_eq!(c.host, "-usemem");
        assert!(
            !c.retail.use_memory_manager,
            "the switch was eaten as -host's value"
        );
        let e = parse(&["-h"]).expect_err("no value at all");
        assert!(e.detail.contains("requires a value"), "{}", e.detail);
    }

    // "-outport -- after assignment the handler range-checks 1 <= client_port <= 65535 ...
    // -port is **not** range-checked despite its description."
    #[test]
    fn outport_is_range_checked_and_port_is_not() {
        assert!(parse(&["-q", "70000"]).is_err());
        assert!(parse(&["-q", "0"]).is_err());
        assert_eq!(parse(&["-p", "70000"]).unwrap().port, 70000);
    }

    // "-debug -- strtoul(value, 0, 0) (so 0x... hex and leading-zero octal are accepted)"
    #[test]
    fn debug_takes_a_strtoul_base_zero_mask() {
        assert_eq!(parse(&["-debug", "0x1F"]).unwrap().retail.debug_flags, 0x1F);
        assert_eq!(parse(&["-debug", "31"]).unwrap().retail.debug_flags, 31);
        assert_eq!(parse(&["-debug", "037"]).unwrap().retail.debug_flags, 0o37);
    }

    #[test]
    fn a_stray_bare_token_is_an_error_but_an_empty_one_is_skipped() {
        assert!(parse(&["stray"]).is_err());
        assert!(parse(&[""]).is_ok());
    }

    // The rebuild-only spelling, which the retail parser could never have resolved.
    #[test]
    fn the_rebuild_switches_use_two_command_characters() {
        let c = parse(&["--headless", "--frames", "3", "--capture", "out.png"]).unwrap();
        assert!(c.headless);
        assert_eq!(c.frames, Some(3));
        assert_eq!(c.capture, Some(PathBuf::from("out.png")));
        // The single-dash spelling is not a retail name, so it is rejected like any unknown switch.
        assert!(parse(&["-headless"]).is_err());
    }

    /// The title the owner asked for, and the one case that is not `Dereth | <account>`.
    ///
    /// Client divergence CD-006.
    ///
    /// Behaviour: presentation.window.the-title-names-the-account-as-it-was-typed
    #[test]
    fn the_window_title_carries_the_account_as_typed() {
        // `-a` is lower-cased for the wire and kept as typed for the title. Both, from one token.
        let c = parse(&["-a", "Tester"]).unwrap();
        assert_eq!(c.account, "tester", "the wire spelling is still _strlwr's");
        assert_eq!(c.window_title(), "Dereth | Tester");
        // No account: the offline slices, which run with no login at all.
        assert_eq!(parse(&[]).unwrap().window_title(), "Dereth");
        // An account of blanks is not an account. `-a` takes the next token whatever it is.
        assert_eq!(parse(&["-a", "   "]).unwrap().window_title(), "Dereth");
    }

    /// `--no-console` parses like any other arity-`None` rebuild switch, and records itself.
    #[test]
    fn no_console_is_a_rebuild_switch() {
        assert!(parse(&[]).unwrap().console, "a console by default");
        assert!(!parse(&["--no-console"]).unwrap().console);
        // It takes no value, so the token after it is still a switch in its own right.
        let c = parse(&["--no-console", "--frames", "2"]).unwrap();
        assert!(!c.console);
        assert_eq!(c.frames, Some(2));
        // One command character makes it the long name `-no-console`, which no table has.
        assert!(parse(&["-no-console"]).is_err());
    }

    /// The pre-parse peek and the parser agree, which is the property that matters: `main` acts on
    /// the first and everything downstream reads the second.
    #[test]
    fn the_console_peek_matches_the_parser() {
        for argv in [
            vec!["--no-console"],
            vec!["--NO-CONSOLE"],
            vec!["//no-console"],
            vec!["-/no-console"],
            vec!["--frames", "2", "--no-console", "--headless"],
        ] {
            let owned: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
            assert!(no_console_in_argv(&owned), "peek missed {argv:?}");
            assert!(!parse(&argv).unwrap().console, "parser missed {argv:?}");
        }
        for argv in [
            vec![],
            vec!["--headless"],
            // A *value* that happens to spell the switch belongs to `--capture`, and the peek does
            // not consume values -- but it also never sees this token as a switch, because a
            // capture path is not one. The parser is the check that the two agree.
            vec!["--capture", "no-console"],
        ] {
            let owned: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
            assert!(
                !no_console_in_argv(&owned),
                "peek false-positive on {argv:?}"
            );
            assert!(
                parse(&argv).unwrap().console,
                "parser disagrees on {argv:?}"
            );
        }
    }

    // Oracle: the preference parser's multi-separator rule:
    // "more than 2 parts -> the value is the remaining parts concatenated **without** the `=`
    // separators, so a value containing `=` is corrupted on load."
    #[test]
    fn a_value_containing_an_equals_sign_is_corrupted_on_load() {
        let p = Preferences::parse("[Net]\nBindInterface=a=b=c\nComputeUniquePort=True\nBare\n");
        assert_eq!(p.get("Net.BindInterface"), Some("abc"));
        assert_eq!(p.bool("Net.ComputeUniquePort"), Some(true));
        // "1 part -> value \"\""
        assert_eq!(p.get("Net.Bare"), Some(""));
        // Names are stored lower-cased, so lookups are case-insensitive.
        assert_eq!(p.get("net.bindinterface"), Some("abc"));
    }

    #[test]
    fn a_missing_preference_file_still_starts() {
        let p = Preferences::load(Path::new("no-such-file-anywhere.ini"));
        assert!(!p.loaded_ok);
        let c = Config::from_args_and_prefs_with(&[], &p).expect("starts anyway");
        assert_eq!(
            c.display.resolution, 0x0400_0300,
            "the compiled-in default survives"
        );
    }

    // Oracle: display-preference initialization and loading.
    // Resolution's stored value is the display-mode word (w<<16)|h.
    #[test]
    fn the_display_preferences_reach_the_presentation() {
        let p = Preferences::parse(
            "[Display]\nResolution=83887104\nFullScreen=False\nSyncToRefresh=True\nRefreshRate=60\n",
        );
        let c = Config::from_args_and_prefs_with(&[], &p).unwrap();
        assert_eq!(c.display.resolution, 1280 << 16 | 1024);
        assert!(!c.display.full_screen);
        assert!(c.display.sync_to_refresh);
        assert_eq!(c.display.refresh_rate, 60);

        let pres = c
            .load_display_preferences(None, true)
            .expect("1280x1024 is >= 800x600");
        assert_eq!((pres.width, pres.height), (1280, 1024));
        assert!(!pres.full_screen);
        assert!(pres.compatibility.fs_sync_to_display_refresh);
        assert_eq!(
            pres.compatibility.fs_bits_per_pixel, 32,
            "the default-constructed presentation's value"
        );

        // "if (w < 800 || h < 600) return false // caller then fails"
        let mut small = c.clone();
        small.display.resolution = 640 << 16 | 480;
        assert!(small.load_display_preferences(None, true).is_none());

        // A forced resolution overrides the width and height, and a disallowed full-screen mode
        // forces windowed.
        let mut fs = c.clone();
        fs.display.full_screen = true;
        let pres = fs
            .load_display_preferences(Some((800, 600)), false)
            .unwrap();
        assert_eq!((pres.width, pres.height), (800, 600));
        assert!(!pres.full_screen);
    }

    #[test]
    fn the_prefs_switch_replaces_the_path() {
        let c = parse(&["-prefs", "x/y.ini"]).unwrap();
        assert_eq!(c.preferences_file, PathBuf::from("x/y.ini"));
        assert!(c.preferences_named);
    }

    /// Behaviour: presentation.settings.a-headless-run-leaves-the-players-settings-alone-unless-given-a-file
    ///
    /// The doubled spelling names the same file as the retail one, and neither is needed for a
    /// windowed run to use the default.
    #[test]
    fn either_spelling_of_the_prefs_switch_names_the_settings_file() {
        let c = parse(&["--prefs", "x/y.ini"]).unwrap();
        assert_eq!(c.preferences_file, PathBuf::from("x/y.ini"));
        assert!(c.preferences_named);
        assert!(!parse(&[]).unwrap().preferences_named);
        assert!(
            parse(&["--prefs"]).is_err(),
            "the doubled spelling wants a value too"
        );
    }

    /// Behaviour: presentation.settings.a-headless-run-leaves-the-players-settings-alone-unless-given-a-file
    ///
    /// A headless run that names no file is given no settings file at all -- nothing is read from
    /// the default, and the empty path is what every writer takes as "no file" -- while a
    /// windowed run keeps the default, and a headless run that names one keeps and reads it.
    #[test]
    fn a_headless_run_that_names_no_settings_file_reads_and_keeps_none() {
        let dir = std::env::temp_dir().join(format!(
            "dereth-headless-prefs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let default = dir.join("UserPreferences.ini");
        std::fs::write(&default, "[Render]\r\nRenderer=d3d12\r\n").unwrap();
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();

        let headless = Config::from_args_and_prefs_at(&args(&["--headless"]), &default).unwrap();
        assert_eq!(headless.preferences_file, PathBuf::new());
        assert_eq!(headless.renderer, None, "the default file was not read");

        let windowed = Config::from_args_and_prefs_at(&[], &default).unwrap();
        assert_eq!(windowed.preferences_file, default);
        assert_eq!(
            windowed.renderer,
            Some(dereth_client_contract::RendererChoice::D3d12)
        );

        let named = dir.join("named.ini");
        std::fs::write(&named, "[Render]\r\nRenderer=wgpu\r\n").unwrap();
        let path = named.to_str().unwrap();
        let asked =
            Config::from_args_and_prefs_at(&args(&["--headless", "--prefs", path]), &default)
                .unwrap();
        assert_eq!(asked.preferences_file, named);
        assert_eq!(
            asked.renderer,
            Some(dereth_client_contract::RendererChoice::Wgpu)
        );
        // A settings folder named outright (in the environment) is asked for too: the headless
        // run reads its file and keeps it to write back.
        let folder =
            Config::from_args_and_prefs_named_at(&args(&["--headless"]), &default, true).unwrap();
        assert_eq!(folder.preferences_file, default);
        assert_eq!(
            folder.renderer,
            Some(dereth_client_contract::RendererChoice::D3d12)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The files that hang off the settings directory all take it from the *parent of the
    /// preferences file*, which is the settings directory. (The keymap file is the front end's;
    /// its own test asserts the same join.)
    /// This asserts the join, so that moving the default moves all of them together and `-prefs`
    /// still moves all of them together.
    #[test]
    fn everything_the_client_saves_hangs_off_the_preferences_files_directory() {
        let prefs = PathBuf::from("root/dir").join(PREFERENCES_FILE_NAME);
        let dir = prefs.parent().expect("a parent");
        assert_eq!(dir, std::path::Path::new("root/dir"));

        // The journal, with no prefix on it.
        let journal = dereth_client_contract::journal::JournalIdentity {
            directory: dir.to_path_buf(),
            world: "Frostfell".into(),
            character: "Tester".into(),
        };
        assert_eq!(journal.client_path().parent(), Some(dir));
        assert_eq!(
            journal.client_path().file_name().and_then(|n| n.to_str()),
            Some("Journal-Frostfell-Tester.txt")
        );

        // The gameplay screen-layout path.
        let layout =
            dereth_client_contract::persist::ScreenLayout::default_path(&dir.to_string_lossy());
        assert!(layout.starts_with("root/dir"), "{layout}");
    }
}
