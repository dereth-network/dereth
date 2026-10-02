//! The browser bindings: what the worker's script calls, and the two calls it answers.
//!
//! The worker owns everything asynchronous (the player's files, the WebSocket, the timer); this
//! side is synchronous and keeps the client's state between calls. A worker has one thread, so the
//! state lives in thread-locals.

use std::cell::RefCell;
use std::sync::Arc;

use dereth_client_sdk::dat::RetailDatStore;
use wasm_bindgen::prelude::*;

use crate::dats::{self, BrowserFile};

#[wasm_bindgen]
extern "C" {
    /// Fill `buf` from `offset` of file `file`; `false` when the file is missing or too short.
    /// The worker's script defines it before it calls into the module.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethDatRead)]
    pub(crate) fn dat_read(file: u32, offset: f64, buf: &mut [u8]) -> bool;

    /// Whether the worker has file `file` at all.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethDatPresent)]
    fn dat_present(file: u32) -> bool;

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

/// Open the player's data files through the worker and describe each.
///
/// # Errors
/// The first required file that is missing or will not open.
#[wasm_bindgen(js_name = openDats)]
pub fn open_dats() -> Result<String, JsError> {
    let (store, reports) = dats::open_store(|index| {
        let index = u32::try_from(index).ok()?;
        dat_present(index).then_some(BrowserFile(index))
    })
    .map_err(|e| JsError::new(&e.to_string()))?;
    STORE.with(|s| *s.borrow_mut() = Some(Arc::new(store)));
    Ok(dats::describe(&reports))
}

/// The playable client: the client shell's application and retail interface, drawing into the
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
    /// on WebGL 2 when `webgl_only`, in the `era` the server's status names (empty when it names
    /// none) with the systems its status lists in `features` (`name=true,...`; empty for the era's
    /// own table). The data files must be open. The server is whatever the
    /// worker's WebSocket reaches; the client addresses it at its nominal ports
    /// ([`crate::server_url::NOMINAL_SERVER`]).
    ///
    /// # Errors
    /// The data files are not open, no GPU backend comes up, or bring-up or the connection
    /// refuses.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        canvas: web_sys::OffscreenCanvas,
        width: u32,
        height: u32,
        account: String,
        password: String,
        sequence: u32,
        webgl_only: bool,
        era: String,
        features: String,
    ) -> Result<WebPlay, JsError> {
        let store = store()?;
        let prepared = dereth_render::wgpu::prepare_canvas(canvas, width, height, webgl_only)
            .await
            .map_err(|e| JsError::new(&e))?;
        dereth_render::wgpu::install(prepared);
        crate::settings::install(&settings_load(), Some(settings_save));
        tracing::info!("settings kept: {:?}", crate::settings::paths());
        let play = crate::play::Play::new(
            store,
            crate::server_url::NOMINAL_SERVER,
            &account,
            &password,
            sequence,
            width,
            height,
            crate::play::announced_era(&era),
            crate::play::announced_features(&features),
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
