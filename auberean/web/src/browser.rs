//! The browser bindings: what the worker's script calls, and what it answers.
//!
//! The worker owns everything asynchronous (the player's files, the WebSocket, the timer); this
//! side is synchronous and keeps the client's state between calls. A worker has one thread, so the
//! state lives in thread-locals. The world stays open for the worker's next play.

use std::cell::RefCell;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use auberean_client_sdk::dat::locate::DatName;
use auberean_client_sdk::dat::{DatFile, DatStorage};
use auberean_client_sdk::script::local::LocalTime;
use auberean_client_sdk::ClientData;
use auberean_interface::data::{open_data, ui_assets, Install};
use auberean_present::Presenter;
use auberean_scene::WorldData;
use auberean_shell::input::{Event, Key};
use auberean_shell::interface::{CursorImage, Interface};
use auberean_shell::Shell;
use wasm_bindgen::prelude::*;

use crate::host::{self, Browser, LocalClock, PageHost, OUTPUT_RATE, SETTINGS_FILE};
use crate::{keys, size, store};

#[wasm_bindgen]
extern "C" {
    /// Fill `buf` from `offset` of file `file`; `false` when the file is missing or too short.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethDatRead)]
    fn dat_read(file: u32, offset: f64, buf: &mut [u8]) -> bool;

    /// Whether the worker has file `file` at all.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethDatPresent)]
    fn dat_present(file: u32) -> bool;

    /// The client's files as the worker last kept them; empty for none.
    #[wasm_bindgen(js_namespace = globalThis, js_name = aubereanSettingsLoad)]
    fn settings_load() -> Vec<u8>;

    /// Keep the client's files: the whole store's bytes, written before the call returns.
    #[wasm_bindgen(js_namespace = globalThis, js_name = aubereanSettingsSave)]
    fn settings_save(bytes: &[u8]);

    #[wasm_bindgen(js_namespace = crypto, js_name = getRandomValues)]
    fn random_values(buf: &mut [u8]);

    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;

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

/// Report panics on the console rather than as an unexplained trap, and send the client's log
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

/// This build's version: Auberean's.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// The data files a play reads, by the numbers [`dat_read`] reads them by: the portal file, the
/// first region's cells and the English strings, which every play needs, and the high-resolution
/// pictures when the player has them.
fn data_files() -> [DatName; 4] {
    [
        DatName::Portal,
        DatName::Cell(1),
        DatName::Local("English".to_owned()),
        DatName::HighRes,
    ]
}

/// The names of the data files a play reads, in the order the worker numbers them.
#[wasm_bindgen(js_name = dataFiles)]
#[must_use]
pub fn data_file_names() -> Vec<String> {
    data_files().iter().map(DatName::file_name).collect()
}

/// A data file the worker reads, by its number.
#[derive(Debug, Clone, Copy)]
struct PageFile(u32);

impl DatStorage for PageFile {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        // A data file is under 4 GiB, so the offset is exact as a JavaScript number.
        #[allow(clippy::cast_precision_loss)]
        let at = offset as f64;
        if dat_read(self.0, at, buf) {
            Ok(())
        } else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }
}

/// The world, the client's data and the interface's records, opened over the files the worker
/// holds: its reads go through the worker, so they outlive the files' handles.
#[derive(Clone)]
struct Opened {
    world: &'static WorldData,
    data: ClientData,
    assets: std::sync::Arc<auberean_ui::DatAssets>,
}

thread_local! {
    static OPENED: RefCell<Option<Opened>> = const { RefCell::new(None) };
}

/// The player's data files, opened through the worker. Answers one line per file for the log. The
/// world stays for as long as the worker does.
///
/// # Errors
/// A file a play cannot do without is missing, or is not what its name says.
#[wasm_bindgen(js_name = openDats)]
pub fn open_dats() -> Result<String, JsError> {
    store::install(&settings_load(), Some(settings_save));
    let names = data_files();
    let number = |name: &DatName| {
        names
            .iter()
            .position(|n| n == name)
            .and_then(|i| u32::try_from(i).ok())
    };
    let found: Vec<DatName> = names
        .iter()
        .filter(|n| number(n).is_some_and(dat_present))
        .cloned()
        .collect();
    let missing: Vec<String> = names[..3]
        .iter()
        .filter(|n| !found.contains(n))
        .map(DatName::file_name)
        .collect();
    if !missing.is_empty() {
        return Err(JsError::new(&format!("{} missing", missing.join(", "))));
    }
    let open = |name: &DatName| {
        let file = number(name)?;
        DatFile::from_storage(PathBuf::from(name.file_name()), Box::new(PageFile(file))).ok()
    };
    let install = Install {
        found: found.clone(),
        open: &open,
    };
    let preferences = store::read(SETTINGS_FILE)
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default();
    let (world, data, language) =
        open_data(&install, None, &preferences).map_err(|e| JsError::new(&e))?;
    let assets = ui_assets(&install, &language).map_err(|e| JsError::new(&e))?;
    let report: Vec<String> = found.iter().map(DatName::file_name).collect();
    OPENED.with(|o| {
        *o.borrow_mut() = Some(Opened {
            world: Box::leak(Box::new(world)),
            data,
            assets,
        });
    });
    Ok(format!("{}, in {language}", report.join(", ")))
}

/// The page's clock, as the host asks it.
#[derive(Debug, Clone, Copy)]
struct PageClock;

impl Browser for PageClock {
    fn now(&self) -> f64 {
        performance_now() / 1000.0
    }

    fn unix_time(&self) -> u32 {
        u32::try_from(whole(js_sys::Date::now() / 1000.0)).unwrap_or(0)
    }

    fn random(&self, out: &mut [u8]) {
        random_values(out);
    }
}

/// A whole, non-negative JavaScript number as an integer; 0 for anything else.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole(x: f64) -> u64 {
    if x.is_finite() && x > 0.0 {
        x as u64
    } else {
        0
    }
}

const CLOCK: LocalClock = LocalClock {
    local: || {
        let d = js_sys::Date::new_0();
        Some(LocalTime {
            year: u16::try_from(d.get_full_year()).ok()?,
            month: u8::try_from(d.get_month() + 1).ok()?,
            day: u8::try_from(d.get_date()).ok()?,
            weekday: u8::try_from(d.get_day()).ok()?,
            hour: u8::try_from(d.get_hours()).ok()?,
            minute: u8::try_from(d.get_minutes()).ok()?,
            second: u8::try_from(d.get_seconds()).ok()?,
            millisecond: u16::try_from(d.get_milliseconds()).ok()?,
        })
    },
    real: || i64::try_from(whole(js_sys::Date::now() / 1000.0)).ok(),
};

/// The server as the client numbers it: the WebSocket's far end alone knows where it is, and maps
/// the frame's ports onto the server's own.
const fn nominal_server() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9000)
}

/// What the device came up on, and the device: WebGPU where the browser has it, WebGL 2 where it
/// does not or where `webgl_only` asks for it.
async fn device(
    canvas: &web_sys::OffscreenCanvas,
    drawn: (u32, u32),
    webgl_only: bool,
) -> Result<Presenter, JsError> {
    canvas.set_width(drawn.0);
    canvas.set_height(drawn.1);
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = if webgl_only {
        wgpu::Backends::GL
    } else {
        wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
    };
    let instance = wgpu::util::new_instance_with_webgpu_detection(desc).await;
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
        .map_err(|e| JsError::new(&format!("no surface: {e}")))?;
    let renderer = auberean_gpu::Renderer::when_ready(&instance, Some(&surface))
        .await
        .map_err(|e| JsError::new(&e.to_string()))?;
    // What the device refuses is the console's to show: drawing goes on without it.
    renderer
        .device()
        .on_uncaptured_error(std::sync::Arc::new(|e: wgpu::Error| {
            tracing::error!("the device: {e}");
        }));
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .map_err(|e| JsError::new(&format!("no adapter: {e}")))?;
    Presenter::on(instance, surface, &adapter, renderer, drawn).map_err(|e| JsError::new(&e))
}

/// The playable client: the shell and the game's own interface, drawing into the worker's canvas.
#[wasm_bindgen]
pub struct WebPlay {
    shell: Shell<'static, PageHost<PageClock>>,
    world: &'static WorldData,
    present: Presenter,
    canvas: web_sys::OffscreenCanvas,
    /// The page in the device's own pixels, and the share of it drawn, in percent.
    page: (u32, u32),
    scale: u32,
    drawn: (u32, u32),
    /// The interface's cursor as it last drew it, and the one the page shows, by its picture.
    cursor: Option<std::sync::Arc<CursorImage>>,
    cursor_shown: Option<u32>,
    /// The pointer as the page last locked it.
    locked: bool,
    /// The keys held, so a browser's repeat reads as a repeat.
    down: std::collections::BTreeSet<Key>,
    backend: String,
}

impl std::fmt::Debug for WebPlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebPlay")
            .field("drawn", &self.drawn)
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

#[wasm_bindgen]
impl WebPlay {
    /// Bring the client up in `canvas` over the files [`open_dats`] opened, on a page of `page`
    /// (`[width, height, scale]`: the page in the device's own pixels and the share of them to
    /// draw), and start logging in as `account`, on WebGL 2 when `webgl_only`, with the friends
    /// service when `friends`. The server is
    /// whatever the worker's WebSocket reaches.
    ///
    /// # Errors
    /// The data files are not open, or no device comes up.
    pub async fn create(
        canvas: web_sys::OffscreenCanvas,
        page: Vec<u32>,
        account: String,
        password: String,
        webgl_only: bool,
        friends: bool,
    ) -> Result<WebPlay, JsError> {
        let at = |i: usize, or: u32| page.get(i).copied().unwrap_or(or);
        let (page, scale) = ((at(0, 800), at(1, 600)), at(2, 100));
        let drawn = size::drawn(page, scale);
        let present = device(&canvas, drawn, webgl_only).await?;
        let Opened {
            world,
            data,
            assets,
        } = OPENED
            .with(|o| o.borrow().clone())
            .ok_or_else(|| JsError::new("the data files are not open"))?;
        store::install(&settings_load(), Some(settings_save));
        let server = nominal_server();
        let login = auberean_interface::Login {
            server,
            account,
            password,
        };
        let interface: Box<dyn Interface> = Box::new(auberean_interface::Retail::new(
            assets,
            (1024, 768),
            Some(login),
        ));
        let host = PageHost::new(PageClock, CLOCK, server);
        let host = if friends { host.with_friends() } else { host };
        let mut shell = Shell::new(host, world, data, interface);
        shell.event(&Event::Resized {
            width: drawn.0,
            height: drawn.1,
        });
        let backend = present.renderer.adapter.trim().to_owned();
        tracing::info!(%backend, "drawing");
        Ok(Self {
            shell,
            world,
            present,
            canvas,
            page,
            scale,
            drawn,
            cursor: None,
            cursor_shown: None,
            locked: false,
            down: std::collections::BTreeSet::new(),
            backend,
        })
    }

    /// What the device came up on.
    #[must_use]
    pub fn backend(&self) -> String {
        self.backend.clone()
    }

    /// A message the WebSocket brought.
    pub fn receive(&mut self, message: Vec<u8>) {
        self.shell.host.received(message);
    }

    /// One frame. Answers whether the client goes on.
    pub fn frame(&mut self) -> bool {
        let world = self.world;
        // A page is all the screen this client has, so the brightness always shows.
        let brightness = self.shell.settings.screen_brightness();
        let out = self.shell.frame();
        let quit = out.quit;
        self.cursor.clone_from(&out.overlay.cursor);
        let mut load = |id: u32| auberean_draw::picture::load(&world.records, id);
        self.present.set_brightness(Some(brightness));
        self.present.present(
            out.world,
            out.portal.map(|f| (f, out.portal_alpha)),
            out.veil,
            out.overlay,
            &out.views,
            &mut load,
            None,
        );
        !quit
    }

    /// The canvas's size, which the page scales to fit.
    #[wasm_bindgen(js_name = clientSize)]
    #[must_use]
    pub fn client_size(&self) -> Vec<u32> {
        vec![self.drawn.0, self.drawn.1]
    }

    /// The messages for the WebSocket since the last call, each one datagram.
    #[wasm_bindgen(js_name = takeMessages)]
    pub fn take_messages(&mut self) -> Vec<js_sys::Uint8Array> {
        self.shell
            .host
            .take_messages()
            .iter()
            .map(|m| js_sys::Uint8Array::from(m.as_slice()))
            .collect()
    }

    /// The session's will, framed for the WebSocket: the goodbye's datagrams, for its far end to
    /// send if the page goes away before it can log off. The same until the connections change.
    #[must_use]
    pub fn will(&self) -> Vec<u8> {
        let goodbye: Vec<(u16, Vec<u8>)> = self
            .shell
            .client
            .as_ref()
            .map(|c| c.goodbye())
            .unwrap_or_default()
            .into_iter()
            .map(|(to, bytes)| (to.port(), bytes))
            .collect();
        dereth_transport::web_frame::encode_will(&goodbye)
    }

    /// The pointer moved to `x`, `y` of the canvas, by `dx`, `dy`.
    #[wasm_bindgen(js_name = pointerMove)]
    #[allow(clippy::cast_possible_truncation)]
    pub fn pointer_move(&mut self, x: f64, y: f64, dx: f64, dy: f64, _buttons: i32) {
        // While the pointer is held the page's lock leaves it where it was.
        self.shell.event(&Event::Pointer {
            x: x as f32,
            y: y as f32,
        });
        if dx != 0.0 || dy != 0.0 {
            self.shell.event(&Event::Motion {
                dx: dx as f32,
                dy: dy as f32,
            });
        }
    }

    /// Whether the page should lock the pointer to the canvas, when that changed since the last
    /// call.
    #[wasm_bindgen(js_name = pointerLock)]
    #[must_use]
    pub fn pointer_lock(&mut self) -> Option<bool> {
        let held = self.shell.host.pointer_held();
        (held != self.locked).then(|| {
            self.locked = held;
            held
        })
    }

    /// A button went down or up.
    #[wasm_bindgen(js_name = pointerButton)]
    pub fn pointer_button(&mut self, button: u16, pressed: bool) {
        if let Some(button) = keys::button(button) {
            self.shell.event(&Event::Button {
                button,
                down: pressed,
            });
        }
    }

    /// The wheel turned, in notches.
    pub fn wheel(&mut self, notches: f32) {
        self.shell.event(&Event::Wheel { notches });
    }

    /// A key went down or up, and the text it types. Answers whether the client knows it.
    pub fn key(&mut self, code: &str, pressed: bool, text: Option<String>) -> bool {
        let key = keys::key(code);
        if let Some(k) = key {
            let repeat = pressed && !self.down.insert(k);
            if !pressed {
                self.down.remove(&k);
            }
            self.shell.event(&Event::Key {
                key: k,
                down: pressed,
                repeat,
            });
        }
        if let Some(text) = text {
            self.shell.event(&Event::Text(text));
        }
        key.is_some()
    }

    /// The Alt key's state: Alt goes to the client as any key does, and the page is its window, so
    /// nothing here takes Alt+Enter.
    pub fn alt(&mut self, _held: bool) {}

    /// The canvas gained or lost the keyboard.
    pub fn focus(&mut self, gained: bool) {
        if !gained {
            // What was held when the keyboard went is let go of.
            for key in std::mem::take(&mut self.down) {
                self.shell.event(&Event::Key {
                    key,
                    down: false,
                    repeat: false,
                });
            }
        }
        self.shell.event(&Event::Focus(gained));
    }

    /// The page changed, in the device's own pixels: the picture follows it.
    #[wasm_bindgen(js_name = setPage)]
    pub fn set_page(&mut self, width: u32, height: u32) {
        self.page = (width, height);
        let drawn = size::drawn(self.page, self.scale);
        if drawn != self.drawn {
            self.drawn = drawn;
            self.canvas.set_width(drawn.0);
            self.canvas.set_height(drawn.1);
            self.present.resize(drawn.0, drawn.1);
            self.shell.event(&Event::Resized {
                width: drawn.0,
                height: drawn.1,
            });
        }
    }

    /// The share of the page's pixels drawn, in percent.
    #[wasm_bindgen(js_name = renderScale)]
    #[must_use]
    pub fn render_scale(&self) -> u32 {
        self.scale
    }

    /// The interface's cursor for the page, when it changed since the last call: `[width, height,
    /// hot_x, hot_y]` as little-endian words, then RGBA rows.
    #[wasm_bindgen(js_name = cursorChange)]
    pub fn cursor_change(&mut self) -> Option<Vec<u8>> {
        let image = self.cursor.as_ref()?;
        if self.cursor_shown == Some(image.picture) {
            return None;
        }
        self.cursor_shown = Some(image.picture);
        let mut out = Vec::with_capacity(16 + image.rgba.len());
        for word in [image.width, image.height, image.hot_x, image.hot_y] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend_from_slice(&image.rgba);
        Some(out)
    }

    /// The page's clipboard text pasted: typed, as the client has no clipboard of its own.
    pub fn paste(&mut self, text: String) {
        self.shell.event(&Event::Text(text));
    }

    /// The text the client copied since the last call: the client copies nothing.
    #[wasm_bindgen(js_name = takeCopied)]
    #[must_use]
    pub fn take_copied(&mut self) -> Vec<String> {
        Vec::new()
    }

    /// The addresses the client asked to open since the last call: it asks for none.
    #[wasm_bindgen(js_name = takeOpenedUrls)]
    #[must_use]
    pub fn take_opened_urls(&mut self) -> Vec<String> {
        Vec::new()
    }

    /// The page is going away, or the client ends: a goodbye on every connection, the settings
    /// kept. The goodbye is among [`Self::take_messages`].
    #[wasm_bindgen(js_name = logOff)]
    pub fn log_off(&mut self) {
        self.shell.event(&Event::Close);
        let _ = self.shell.frame();
    }

    /// The client is done: everything is kept by now, and the device goes before the worker does.
    /// A browser that ends a worker holding a live device may not survive it.
    #[wasm_bindgen(js_name = shutDown)]
    pub fn shut_down(&mut self) {
        self.present.renderer.device().destroy();
    }

    /// The next `frames` sample frames of the sound, interleaved stereo.
    #[must_use]
    pub fn audio(&mut self, frames: u32) -> Vec<f32> {
        host::pull_sound(usize::try_from(frames).unwrap_or(0))
    }

    /// The rate the page's audio context is opened at.
    #[wasm_bindgen(js_name = audioRate)]
    #[must_use]
    pub fn audio_rate() -> u32 {
        OUTPUT_RATE
    }
}
