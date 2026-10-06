//! The browser bindings: what the worker's script calls, and the two calls it answers.
//!
//! The worker owns everything asynchronous (the player's files, the WebSocket, the timer); this
//! side is synchronous and keeps the client's state between calls. A worker has one thread, so the
//! state lives in thread-locals.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;

use dereth_client_sdk::dat::RetailDatStore;
use wasm_bindgen::prelude::*;

use crate::dats::{self, BrowserFile};
use crate::front::Front;

#[wasm_bindgen]
extern "C" {
    /// Fill `buf` from `offset` of file `file`; `false` when the file is missing or too short.
    /// The worker's script defines it before it calls into the module.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethDatRead)]
    pub(crate) fn dat_read(file: u32, offset: f64, buf: &mut [u8]) -> bool;

    /// Whether the worker has file `file` at all.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethDatPresent)]
    fn dat_present(file: u32) -> bool;

    /// Fill `buf` from `offset` of the file `file` the worker is reading a data set's files from
    /// (to say what each reports); `false` when it is too short.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethProbeRead)]
    fn probe_read(file: u32, offset: f64, buf: &mut [u8]) -> bool;

    /// The client's files as the page last saved them; empty for none. See [`crate::settings`].
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethSettingsLoad)]
    fn settings_load() -> Vec<u8>;

    /// Keep the client's files: the whole store's bytes, written before the call returns.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethSettingsSave)]
    fn settings_save(bytes: &[u8]);

    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);

    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log(message: &str);
}

/// A writer for the log subscriber: each event, whole, as one console line.
#[derive(Debug, Default)]
struct ConsoleLine(Vec<u8>);

impl std::io::Write for ConsoleLine {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for ConsoleLine {
    fn drop(&mut self) {
        let text = String::from_utf8_lossy(&self.0);
        let text = text.trim_end();
        if !text.is_empty() {
            console_log(text);
        }
    }
}

thread_local! {
    static STORE: RefCell<Option<Arc<RetailDatStore>>> = const { RefCell::new(None) };
    static FRONT: RefCell<Front> = RefCell::new(Front::default());
}

/// A file of a data set the worker is reading, by its place in the worker's list.
#[derive(Debug, Clone, Copy)]
struct ProbeFile(u32);

impl dereth_dat::DatStorage for ProbeFile {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        // A dat file is under 4 GiB, so the offset is exact as a JavaScript number.
        #[allow(clippy::cast_precision_loss)]
        let at = offset as f64;
        if probe_read(self.0, at, buf) {
            Ok(())
        } else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }
}

/// A whole, non-negative JavaScript number (a size, a time in seconds) as an integer; 0 for
/// anything else.
fn whole(x: f64) -> u64 {
    u64::try_from(dereth_primitives::num::to_i64_f64(x)).unwrap_or(0)
}

fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// The overlay folder a page names (empty for none), as the client's configuration names it.
fn overlay_path(overlay: &str) -> Option<PathBuf> {
    (!overlay.is_empty()).then(|| PathBuf::from(overlay))
}

fn store() -> Result<Arc<RetailDatStore>, JsError> {
    STORE
        .with(|s| s.borrow().clone())
        .ok_or_else(|| JsError::new("the data files are not open"))
}

/// Report panics on the console rather than as an unexplained trap, and send the runtime's log
/// events at `filter` (`info`, `debug`, ...) there too.
#[wasm_bindgen]
pub fn start(filter: &str) {
    std::panic::set_hook(Box::new(|info| console_error(&info.to_string())));
    let level = filter
        .parse::<tracing_subscriber::filter::LevelFilter>()
        .unwrap_or(tracing_subscriber::filter::LevelFilter::INFO);
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_ansi(false)
        .without_time()
        .with_writer(ConsoleLine::default)
        .try_init();
}

/// Keep the client's own files (preferences, keymaps, the overlay blocklist) in the page's
/// settings file from now on: the worker opened it before it asks anything of them.
#[wasm_bindgen(js_name = installSettings)]
pub fn install_settings() {
    crate::settings::install(&settings_load(), Some(settings_save));
}

/// The set a world is drawn from, by the desktop client's rule (its overlay's base, else
/// `--world-base`, else its era's): `"modern"`, `"classic"`, or empty when nothing names one.
/// `args` are what the launcher says about the world, and `overlay` the folder the worker holds
/// its overlay in (empty for none).
///
/// # Errors
/// Words about the world the parser refuses.
#[wasm_bindgen(js_name = worldSet)]
pub fn world_set(args: Vec<String>, overlay: &str) -> Result<String, JsError> {
    crate::overlay::hold(overlay);
    let cfg = crate::play::world_config(&args, overlay_path(overlay)).map_err(js_err)?;
    Ok(match dereth_client_runtime::assets::world_set(&cfg) {
        Some(dereth_primitives::ContainerEra::Modern) => "modern",
        Some(dereth_primitives::ContainerEra::Classic) => "classic",
        None => "",
    }
    .to_owned())
}

/// Open the player's data files through the worker for the world `args` describe (what the
/// launcher says about it: its era, the set its world is drawn from, ...), with its overlay kept in
/// the folder `overlay` the worker holds (empty for none), and describe each. Which set is the
/// world's follows the desktop client's rule ([`world_set`], [`dats::plan`]).
///
/// # Errors
/// Words about the world the parser refuses, or the first required file that is missing or will
/// not open.
#[wasm_bindgen(js_name = openDats)]
pub fn open_dats(args: Vec<String>, overlay: &str) -> Result<String, JsError> {
    crate::overlay::hold(overlay);
    let cfg = crate::play::world_config(&args, overlay_path(overlay)).map_err(js_err)?;
    let world_set = dereth_client_runtime::assets::world_set(&cfg);
    let (store, reports) = dats::open_store(
        |index| {
            let index = u32::try_from(index).ok()?;
            dat_present(index).then_some(BrowserFile(index))
        },
        world_set,
    )
    .map_err(|e| JsError::new(&e.to_string()))?;
    STORE.with(|s| *s.borrow_mut() = Some(Arc::new(store)));
    Ok(dats::describe(&reports))
}

/// The playable client: the client shell's application and modern interface, drawing into the
/// worker's canvas.
#[wasm_bindgen]
#[derive(Debug)]
pub struct WebPlay {
    /// `None` once the client has shut down.
    play: Option<crate::play::Play>,
    backend: String,
}

#[wasm_bindgen]
impl WebPlay {
    /// Bring the client up in `canvas` at `width` by `height` and start logging in as `account`,
    /// on WebGL 2 when `webgl_only`, told about the world what the launcher tells the desktop
    /// client on its command line (`args`: its era and systems, the set its world is drawn from,
    /// its logon version and rules), with the world's overlay kept in the folder `overlay` the
    /// worker holds (empty for none). The data files must be open. The server is whatever the
    /// worker's WebSocket reaches; the client addresses it at its nominal ports
    /// ([`crate::server_url::NOMINAL_SERVER`]).
    ///
    /// # Errors
    /// The data files are not open, no GPU backend comes up, the words about the world do not
    /// read, or bring-up or the connection refuses.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        canvas: web_sys::OffscreenCanvas,
        width: u32,
        height: u32,
        account: String,
        password: String,
        sequence: u32,
        webgl_only: bool,
        args: Vec<String>,
        overlay: String,
    ) -> Result<WebPlay, JsError> {
        let store = store()?;
        let prepared = dereth_render::wgpu::prepare_canvas(canvas, width, height, webgl_only)
            .await
            .map_err(|e| JsError::new(&e))?;
        dereth_render::wgpu::install(prepared);
        install_settings();
        tracing::info!("settings kept: {:?}", crate::settings::paths());
        crate::overlay::hold(&overlay);
        let play = crate::play::Play::new(
            store,
            crate::server_url::NOMINAL_SERVER,
            &account,
            &password,
            sequence,
            width,
            height,
            &args,
            overlay_path(&overlay),
        )
        .map_err(|e| JsError::new(&e))?;
        let backend = play.app().renderer().adapter_name();
        Ok(Self {
            play: Some(play),
            backend,
        })
    }

    /// Which GPU backend came up, and on what.
    #[must_use]
    pub fn backend(&self) -> String {
        self.backend.clone()
    }

    /// One message from the server's WebSocket.
    pub fn receive(&mut self, message: &[u8]) {
        if let Some(p) = self.play.as_mut() {
            p.receive(message);
        }
    }

    /// One frame, drawn; `false` once the client has shut itself down.
    pub fn frame(&mut self) -> bool {
        self.play.as_mut().is_some_and(crate::play::Play::frame)
    }

    /// The size the client draws at, `[width, height]`: see [`crate::play::Play::client_size`].
    #[wasm_bindgen(js_name = clientSize)]
    #[must_use]
    pub fn client_size(&self) -> Vec<u32> {
        self.play.as_ref().map_or_else(Vec::new, |p| {
            let (w, h) = p.client_size();
            vec![w, h]
        })
    }

    /// The session's will, for the server's WebSocket end: see [`crate::play::Play::will`].
    #[must_use]
    pub fn will(&mut self) -> Vec<u8> {
        self.play
            .as_mut()
            .map(crate::play::Play::will)
            .unwrap_or_default()
    }

    /// Everything the connection wants sent, framed for the WebSocket.
    #[wasm_bindgen(js_name = takeMessages)]
    pub fn take_messages(&mut self) -> Vec<js_sys::Uint8Array> {
        self.play
            .as_mut()
            .map(crate::play::Play::take_messages)
            .unwrap_or_default()
            .iter()
            .map(|m| js_sys::Uint8Array::from(m.as_slice()))
            .collect()
    }

    #[wasm_bindgen(js_name = pointerMove)]
    pub fn pointer_move(&mut self, x: f64, y: f64) {
        if let Some(p) = self.play.as_mut() {
            p.pointer_move(x, y);
        }
    }

    #[wasm_bindgen(js_name = pointerButton)]
    pub fn pointer_button(&mut self, button: u16, pressed: bool) {
        if let Some(p) = self.play.as_mut() {
            p.pointer_button(button, pressed);
        }
    }

    pub fn wheel(&mut self, notches: f32) {
        if let Some(p) = self.play.as_mut() {
            p.wheel(notches);
        }
    }

    /// A key by the page's `KeyboardEvent.code`, typing `text`; `false` for a key the client does
    /// not know.
    pub fn key(&mut self, code: &str, pressed: bool, text: Option<String>) -> bool {
        self.play
            .as_mut()
            .is_some_and(|p| p.key(code, pressed, text))
    }

    pub fn alt(&mut self, held: bool) {
        if let Some(p) = self.play.as_mut() {
            p.alt(held);
        }
    }

    pub fn focus(&mut self, gained: bool) {
        if let Some(p) = self.play.as_mut() {
            p.focus(gained);
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some(p) = self.play.as_mut() {
            p.resize(width, height);
        }
    }

    /// The cursor the interface set since the last call, as `[width, height, hot_x, hot_y]` and
    /// then its RGBA rows; `None` when it has not changed.
    #[wasm_bindgen(js_name = cursorChange)]
    pub fn cursor_change(&mut self) -> Option<Vec<u8>> {
        let c = self.play.as_mut()?.cursor_change()?;
        let mut out = Vec::with_capacity(16 + c.rgba.len());
        for v in [c.width, c.height, c.hot_x, c.hot_y] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&c.rgba);
        Some(out)
    }

    /// Text the page pasted, for the client's clipboard.
    pub fn paste(&mut self, text: &str) {
        crate::host::clipboard::paste_from_page(text);
    }

    /// Text the client copied, for the page to put on the system clipboard.
    #[wasm_bindgen(js_name = takeCopied)]
    #[must_use]
    pub fn take_copied(&mut self) -> Vec<String> {
        crate::host::clipboard::take_copied()
    }

    /// URLs the client asked to open.
    #[wasm_bindgen(js_name = takeOpenedUrls)]
    #[must_use]
    pub fn take_opened_urls(&mut self) -> Vec<String> {
        crate::host::take_opened_urls()
    }

    #[wasm_bindgen(js_name = logOff)]
    pub fn log_off(&mut self) {
        if let Some(p) = self.play.as_mut() {
            p.log_off();
        }
    }

    /// Run the client's cleanup: its keymap and preferences are saved to browser storage. Log
    /// off first ([`Self::log_off`]) and send what that queues: the cleanup's own goodbye would go
    /// out on the connection it drops. After it, every other call does nothing.
    #[wasm_bindgen(js_name = shutDown)]
    pub fn shut_down(&mut self) {
        if let Some(p) = self.play.take() {
            let log = p.shut_down();
            tracing::info!("shut down: {log:?}");
        }
    }

    /// The next `frames` sample frames of sound, interleaved stereo at
    /// [`Self::audio_rate`]; empty while the client has no sound.
    pub fn audio(&mut self, frames: u32) -> Vec<f32> {
        crate::host::audio::pull(frames as usize)
    }

    /// The rate [`Self::audio`] is at, which the page opens its audio context at.
    #[wasm_bindgen(js_name = audioRate)]
    #[must_use]
    pub fn audio_rate() -> u32 {
        crate::host::audio::OUTPUT_RATE
    }
}

/// Why `url` cannot be the server the worker connects to, or nothing when it can: `wss://` to any
/// host, and `ws://` only to this machine (see [`crate::server_url`]).
#[wasm_bindgen(js_name = serverUrlProblem)]
#[must_use]
pub fn server_url_problem(url: &str) -> Option<String> {
    crate::server_url::problem(url)
}

// ---- the front page's launcher ---------------------------------------------------------------

/// Where the community list is read from: the desktop launcher's address for it.
#[wasm_bindgen(js_name = frontListUrl)]
#[must_use]
pub fn front_list_url() -> String {
    dereth_launch::serverlist::COMMUNITY_LIST_URL.to_owned()
}

/// Read the community list (`Servers.xml`, as fetched); answers how many worlds it lists.
///
/// # Errors
/// The list does not read; the worlds read before stand.
#[wasm_bindgen(js_name = frontList)]
pub fn front_list(xml: &str) -> Result<u32, JsError> {
    let n = FRONT
        .with(|f| f.borrow_mut().set_list(xml))
        .map_err(js_err)?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Whether a copy of the list fetched at `fetched_at` (seconds since the epoch) is the day's at
/// `now`.
#[wasm_bindgen(js_name = frontListFresh)]
#[must_use]
pub fn front_list_fresh(fetched_at: f64, now: f64) -> bool {
    let secs = whole;
    crate::front::list_is_fresh(secs(fetched_at), secs(now))
}

/// Take the launcher's state record as the page kept it (empty for none).
///
/// # Errors
/// A record that does not read.
#[wasm_bindgen(js_name = frontSetState)]
pub fn front_set_state(json: &str) -> Result<(), JsError> {
    FRONT
        .with(|f| f.borrow_mut().set_state(json))
        .map_err(js_err)
}

/// The launcher's state record, for the page to keep.
#[wasm_bindgen(js_name = frontState)]
#[must_use]
pub fn front_state() -> String {
    FRONT.with(|f| f.borrow().state_json())
}

/// A world's status document, as fetched; answers whether it was one.
#[wasm_bindgen(js_name = frontStatus)]
#[must_use]
pub fn front_status(slug: &str, body: &[u8]) -> bool {
    FRONT.with(|f| f.borrow_mut().set_status(slug, body))
}

/// A world's status document did not answer.
#[wasm_bindgen(js_name = frontStatusFailed)]
pub fn front_status_failed(slug: &str) {
    FRONT.with(|f| f.borrow_mut().status_failed(slug));
}

/// Every world, with the eras and systems on offer, as JSON.
#[wasm_bindgen(js_name = frontWorlds)]
#[must_use]
pub fn front_worlds() -> String {
    FRONT.with(|f| f.borrow().worlds().to_string())
}

/// One world's page as JSON: [`Front::world`]. `None` for a world not shown.
#[wasm_bindgen(js_name = frontWorld)]
#[must_use]
pub fn front_world(slug: &str) -> Option<String> {
    FRONT.with(|f| f.borrow().world(slug).map(|v| v.to_string()))
}

/// Add a server by host and port; answers its slug.
///
/// # Errors
/// What is wrong with the host or port, for the player.
#[wasm_bindgen(js_name = frontAddWorld)]
pub fn front_add_world(
    name: &str,
    host: &str,
    port: &str,
    ruleset: &str,
    emulator: &str,
) -> Result<String, JsError> {
    FRONT
        .with(|f| {
            f.borrow_mut()
                .add_world(name, host, port, ruleset, emulator)
        })
        .map_err(js_err)
}

/// Take a server the player added off the list.
#[wasm_bindgen(js_name = frontRemoveWorld)]
pub fn front_remove_world(slug: &str) {
    FRONT.with(|f| f.borrow_mut().remove_world(slug));
}

/// The era the player chose for a world that does not say its own (empty for none).
#[wasm_bindgen(js_name = frontSetEra)]
pub fn front_set_era(slug: &str, era: &str) {
    FRONT.with(|f| f.borrow_mut().set_era(slug, era));
}

/// One of a world's systems on or off; answers whether `name` is a system's.
#[wasm_bindgen(js_name = frontSetFeature)]
#[must_use]
pub fn front_set_feature(slug: &str, name: &str, on: bool) -> bool {
    FRONT.with(|f| f.borrow_mut().set_feature(slug, name, on))
}

/// Remember the player's choices on a world (JSON: `account`, `dat_set_id`, `classic_set_id`).
///
/// # Errors
/// Choices that do not read.
#[wasm_bindgen(js_name = frontRemember)]
pub fn front_remember(slug: &str, prefs: &str) -> Result<(), JsError> {
    FRONT
        .with(|f| f.borrow_mut().remember(slug, prefs))
        .map_err(js_err)
}

/// Read the files the worker holds open for a data set kept in the browser's folder `folder`:
/// `names[i]` (of `sizes[i]` bytes) is read through the worker's file `i`. Answers the ids of the
/// folder's sets.
#[wasm_bindgen(js_name = frontAddSets)]
#[must_use]
pub fn front_add_sets(folder: &str, names: Vec<String>, sizes: Vec<f64>) -> Vec<String> {
    let files = names
        .into_iter()
        .zip(sizes)
        .enumerate()
        .map(|(i, (name, size))| {
            let storage: Box<dyn dereth_dat::DatStorage> =
                Box::new(ProbeFile(u32::try_from(i).unwrap_or(u32::MAX)));
            (name, whole(size), storage)
        })
        .collect();
    FRONT.with(|f| f.borrow_mut().add_sets(folder, files))
}

/// Forget the sets kept in the browser's folder `folder`.
#[wasm_bindgen(js_name = frontRemoveSets)]
pub fn front_remove_sets(folder: &str) {
    FRONT.with(|f| f.borrow_mut().remove_sets(folder));
}

/// The folder a world's overlay is kept in, from its address (`host:port`).
#[wasm_bindgen(js_name = overlayFolder)]
#[must_use]
pub fn overlay_folder(address: &str) -> String {
    format!("overlays/{}", crate::front::overlay_folder(address))
}

// ---- the world's overlay -----------------------------------------------------------------------

/// The worker now holds the overlay folder `folder` open (empty for none).
#[wasm_bindgen(js_name = holdOverlay)]
pub fn hold_overlay(folder: &str) {
    crate::overlay::hold(folder);
}

/// What the folder the worker holds open keeps, as JSON: the world its overlay belongs to, the set
/// it was made against and its containers; `null` when the worker holds none.
#[wasm_bindgen(js_name = overlayInfo)]
#[must_use]
pub fn overlay_info() -> String {
    let Some(folder) = crate::overlay::BrowserFolder::held() else {
        return "null".into();
    };
    match dereth_dat::overlay::OverlayDir::in_folder(Arc::new(folder)) {
        Ok(dir) => serde_json::json!({
            "folder": dir.path().display().to_string(),
            "world_key": dir.world_key(),
            "base": dir.base_era().map(|e| match e {
                dereth_primitives::ContainerEra::Modern => "modern",
                dereth_primitives::ContainerEra::Classic => "classic",
            }),
            "containers": dir
                .containers()
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect::<Vec<_>>(),
        })
        .to_string(),
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

/// The worlds whose overlays the player refuses: the client's overlay blocklist, kept with its
/// settings.
#[wasm_bindgen(js_name = overlayBlocklist)]
#[must_use]
pub fn overlay_blocklist() -> Vec<String> {
    crate::settings::blocklist().into_iter().collect()
}

/// Refuse the overlay of the world `key` (as the world names itself), or take it off the list.
///
/// # Errors
/// The settings file will not take it.
#[wasm_bindgen(js_name = setOverlayBlocked)]
pub fn set_overlay_blocked(key: &str, blocked: bool) -> Result<(), JsError> {
    crate::settings::set_blocked(key, blocked).map_err(js_err)
}
