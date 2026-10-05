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
pub struct DisplayModeFlags {
    pub fs_refresh_rate: u32,
    pub fs_bits_per_pixel: u32,
    pub fs_triple_buffering: bool,
    pub fs_sync_to_display_refresh: bool,
    pub antialiasing: bool,
}

impl Default for DisplayModeFlags {
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

/// Display mode defaults, overwritten when the device loads display preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub full_screen: bool,

    /// Compatibility projection retained independently of the effective window size.
    pub compatibility: DisplayModeFlags,
}

impl Default for DisplayMode {
    /// The render-device presentation constructor uses this value.
    fn default() -> Self {
        Self {
            compatibility: DisplayModeFlags::default(),
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
    /// `--world-base <modern|classic>`: which dat set the world is drawn from when the world's
    /// overlay does not say (its containers name their base files, and win over this): the later
    /// files (`modern`) or the files from before Throne of Destiny (`classic`). `None`: the era's.
    pub world_base: Option<dereth_primitives::ContainerEra>,
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
    /// `--era-features <version>:<bits>` (or `<name=true,...>`): the systems the server says its
    /// world has, as the launcher reads them from the world's status, in the shared bitfield
    /// (`dereth_primitives::EraFeatureBits`) or by name. Each system given wins over the era's
    /// table; one this client's table and the bitfield's do not both have, or a name it does not
    /// know, is left to the table.
    pub era_features: dereth_primitives::EraFeatureOverrides,
    /// `--logon-version <string>`: the logon version string the login request carries, for a
    /// world whose server wants one other than the end of retail's `"1802"`, as the launcher
    /// reads it from its table of such worlds.
    pub logon_version: String,
    /// `--world-profile <name>`: the named rules the world's own client played by
    /// (`dereth_rules::world::PROFILES`), over the end of retail's. Default: none, the end of
    /// retail's rules.
    pub world_rules: dereth_primitives::WorldRules,

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
    /// (`dereth/client/tests/gpu/presentation/headless_capture_determinism.rs` and `dereth/client/tests/gpu/rendering/static_scene.rs`) therefore capture the world plus
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
            landblock: dereth_world_data::landblock::DEFAULT_LANDBLOCK,
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
            world_base: None,
            era: None,
            era_features: dereth_primitives::EraFeatureOverrides::default(),
            logon_version: dereth_transport::conn::CLIENT_VERSION.to_owned(),
            world_rules: dereth_primitives::WorldRules::default(),
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
    // Which dat set the world is drawn from, when its overlay does not say.
    Switch {
        long: "world-base",
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
    // The logon version string a world's server wants, and the named rules its client played by.
    Switch {
        long: "logon-version",
        short: None,
        arity: Arity::Required,
    },
    Switch {
        long: "world-profile",
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
        if let Some(v) = prefs.bool(dereth_client_contract::options::names::DISPLAY_FULL_SCREEN) {
            self.display.full_screen = v;
        }
        if let Some(v) = prefs.u32(dereth_client_contract::options::names::DISPLAY_REFRESH_RATE) {
            self.display.refresh_rate = v;
        }
        if let Some(v) = prefs.bool(dereth_client_contract::options::names::DISPLAY_SYNC_TO_REFRESH)
        {
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
            "world-base" => {
                self.world_base = Some(match v.trim().to_ascii_lowercase().as_str() {
                    "modern" => dereth_primitives::ContainerEra::Modern,
                    "classic" => dereth_primitives::ContainerEra::Classic,
                    _ => {
                        return Err(ConfigError::new(format!(
                            "unknown --world-base {v:?} (modern or classic)"
                        )))
                    }
                });
            }
            "object-visuals" => {
                let style = dereth_client_contract::options::landscape::parse(v)
                    .ok_or_else(|| {
                        ConfigError::new(format!(
                            "--object-visuals takes world, classic or modern, not {v:?}"
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
                // The bitfield (`<table version>:<hex>`), as the launcher passes it, or the
                // systems by name (`name=true,...`), as a person types them.
                if !v.contains('=') && v.contains(':') {
                    let bits = dereth_primitives::EraFeatureBits::parse(v)
                        .map_err(|e| ConfigError::new(format!("bad --era-features: {e}")))?;
                    if bits.table_version > dereth_primitives::EraFeatures::TABLE_VERSION {
                        tracing::warn!(
                            "--era-features is from a newer table (version {}); the systems this client does not know are skipped",
                            bits.table_version
                        );
                    }
                    self.era_features = bits.overrides();
                } else {
                    let (features, unknown) = dereth_primitives::EraFeatureOverrides::parse(v)
                        .map_err(|e| ConfigError::new(format!("bad --era-features: {e}")))?;
                    if !unknown.is_empty() {
                        tracing::warn!(
                            "--era-features names systems this client does not know: {unknown:?}"
                        );
                    }
                    self.era_features = features;
                }
            }
            "logon-version" => {
                // A packed string the server compares byte for byte; Windows-1252 has no room for
                // anything but plain characters here, and an empty one would be no version at all.
                let v = v.trim();
                if v.is_empty() || !v.chars().all(|c| c.is_ascii_graphic()) || v.len() > 64 {
                    return Err(ConfigError::new(format!("bad --logon-version {v:?}")));
                }
                v.clone_into(&mut self.logon_version);
            }
            "world-profile" => {
                self.world_rules = dereth_rules::world::profile_rules(v)
                    .map_err(|e| ConfigError::new(format!("bad --world-profile: {e}")))?;
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
    ) -> Option<DisplayMode> {
        let w = self.display.resolution >> 16;
        let h = self.display.resolution & 0xFFFF;
        if w < 800 || h < 600 {
            return None;
        }
        let mut p = DisplayMode {
            width: w,
            height: h,
            full_screen: self.display.full_screen,
            compatibility: DisplayModeFlags {
                fs_refresh_rate: self.display.refresh_rate,
                fs_triple_buffering: self.display.triple_buffering,
                fs_sync_to_display_refresh: self.display.sync_to_refresh,
                antialiasing: self.display.antialiasing,
                ..DisplayModeFlags::default()
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
    let raw = prefs.get(dereth_client_contract::options::names::DISPLAY_RESOLUTION)?;
    if let Some(v) = dereth_client_contract::options::store::display_choice(
        dereth_client_contract::options::names::DISPLAY_RESOLUTION,
        raw,
    ) {
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
    match dereth_dat::locate_modern_dats(&candidates) {
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
mod tests;
