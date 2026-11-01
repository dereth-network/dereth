//! The desktop client: a window in which Asheron's Call 2's world is drawn and played. The shell's
//! host on the desktop: the window and its events, the device, the network, the clock, the
//! settings folder and the log.
//!
//! **Depends on** the shell (`auberean-shell`), the game's interface (`auberean-interface`,
//! `auberean-ui`) and the world it shows (`auberean-scene`), the
//! world's device (`auberean-gpu`) and its frames (`auberean-draw`), the data files
//! (`auberean-dat`, `auberean-assets`, `auberean-world`, `auberean-client-sdk`), the shared math
//! (`dereth-primitives`), and the window system (`winit`) and device (`wgpu`). **Used by** nothing:
//! it is the `auberean` program.
//!
//! ```text
//! auberean --dats <install folder> [--server 127.0.0.1:9000] [--account ACCOUNT[:PASSWORD]]
//!          [--glsticketdirect SECRET]
//!          [--gunserver HOST [--gunport PORT] [--gunticket] [--gunticketoverride TICKET]]
//!          [--size 1280x960] [--dev-login ACCOUNT:PASSWORD [--dev-character NAME]]
//!          [--dev-overlay] [--dev-script FILE] [--shots DIR] [--no-window SECONDS] [--verbose]
//! ```
//!
//! The install folder is the September 2013 client's: its data files are the world and the
//! interface's records, and answer the server's question of which data the client has. The
//! interface is the game's own (`auberean-interface`): with `--account` it logs in as the launcher
//! would have it log in, and the character screens take it from there. `--dev-login` instead logs
//! in and enters the world by itself (making the character, a human, when the account has none of
//! that name) while the screens follow; `--dev-overlay` writes the player's place and the
//! session's state over the world; `--dev-script` plays a script of input and screenshots
//! (`script.rs`). Screenshots (Ctrl+P, and a script's) go to `--shots`, by default the settings
//! folder's `screenshots`. `--no-window` runs everything but the window and the device for that
//! many seconds (the world is still framed, with no picture made), printing the status lines.
//!
//! `--glsticketdirect` gives the login its secret directly, as the September 2013 client's own option
//! does (it stands for the password after `--account`).
//!
//! The friends service (friends, the ignore list, the away state, tells to `Name@World`) is reached
//! at the login's host and port by default, with a ticket made from the login's account and
//! password. The September 2013 client's own options name it elsewhere: `--gunserver` its host,
//! `--gunport` its port (2310 when left out, as the original service's), `--gunticket` to present
//! the ticket and hear the service's events, `--gunticketoverride` the ticket; `--gunclientdll`,
//! the retail client's library, is taken and ignored. A service that cannot be reached leaves the
//! player without friends when the login has its secret; a service named for a login without one
//! ends the login, as the September 2013 client does.
//!
//! `auberean --version` prints the version, and the commit and target a release build was made
//! from. A release build is a windowed program on Windows, with no console of its own.
//!
//! The settings (`Preferences.ini`) are kept in the folder `AUBEREAN_SETTINGS_DIR` names, else in
//! the user's application data (`Auberean`). Tests and trial runs set the variable to a folder of
//! their own. The key maps' files are saved there too; one not saved there is read from the
//! install folder, where the September 2013 client keeps its own.
//!
//! **Must never** hold the game: what a frame shows and what the player's input does are the
//! shell's (`auberean-shell`); this program only brings the platform under it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod friends;
mod icon;
mod keys;
mod present;
mod script;

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::runtime::data::{DataFiles, ENGLISH};
use auberean_client_sdk::ClientData;
use auberean_scene::WorldData;
use auberean_shell::input::{Event, Key};
use auberean_shell::interface::Interface;
use auberean_shell::{Host, Shell};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{CursorGrabMode, Window, WindowId};

/// The variable naming the settings folder.
const SETTINGS_DIR: &str = "AUBEREAN_SETTINGS_DIR";
/// The settings file in it.
const SETTINGS_FILE: &str = "Preferences.ini";

#[derive(Debug, Default)]
struct Options {
    dats: PathBuf,
    server: Option<SocketAddr>,
    size: Option<(u32, u32)>,
    login: Option<(String, String)>,
    account: Option<(String, String)>,
    character: Option<String>,
    overlay: bool,
    script: Option<PathBuf>,
    shots: Option<PathBuf>,
    verbose: bool,
    no_window: Option<f64>,
    gun_server: Option<String>,
    gun_port: Option<u16>,
    gun_ticket: bool,
    gun_ticket_override: Option<String>,
    secret: Option<String>,
}

fn options() -> Result<Options, String> {
    let mut o = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--dats" => o.dats = PathBuf::from(value()?),
            "--server" => {
                o.server = Some(
                    value()?
                        .parse()
                        .map_err(|e| format!("--server: {e} (want IP:PORT)"))?,
                );
            }
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or("--size: want WxH")?;
                o.size = Some((
                    w.parse().map_err(|e| format!("--size: {e}"))?,
                    h.parse().map_err(|e| format!("--size: {e}"))?,
                ));
            }
            "--dev-login" => {
                let v = value()?;
                let (a, p) = v.split_once(':').unwrap_or((v.as_str(), ""));
                o.login = Some((a.to_owned(), p.to_owned()));
            }
            "--account" => {
                let v = value()?;
                let (a, p) = v.split_once(':').unwrap_or((v.as_str(), ""));
                o.account = Some((a.to_owned(), p.to_owned()));
            }
            "--dev-character" => o.character = Some(value()?),
            "--dev-overlay" => o.overlay = true,
            "--dev-script" => o.script = Some(PathBuf::from(value()?)),
            "--shots" => o.shots = Some(PathBuf::from(value()?)),
            "--verbose" => o.verbose = true,
            "--no-window" => {
                o.no_window = Some(
                    value()?
                        .parse()
                        .map_err(|e| format!("--no-window: {e} (want seconds)"))?,
                );
            }
            "--glsticketdirect" => o.secret = Some(value()?),
            "--gunserver" => o.gun_server = Some(value()?),
            "--gunport" => {
                o.gun_port = Some(
                    value()?
                        .parse()
                        .map_err(|e| format!("--gunport: {e} (want a port)"))?,
                );
            }
            "--gunticket" => o.gun_ticket = true,
            "--gunticketoverride" => o.gun_ticket_override = Some(value()?),
            "--gunclientdll" => {
                value()?;
            }
            "-h" | "--help" => return Err("see the usage at the top of src/main.rs".into()),
            other => return Err(format!("unknown option {other}")),
        }
    }
    if o.dats.as_os_str().is_empty() {
        return Err("--dats <install folder> is needed".into());
    }
    if o.gun_ticket && o.gun_server.is_none() {
        return Err("--gunticket needs --gunserver".into());
    }
    if let Some(secret) = o.secret.take() {
        let Some((_, password)) = o.account.as_mut() else {
            return Err("--glsticketdirect needs --account".into());
        };
        *password = secret;
    }
    Ok(o)
}

/// Where the friends service is: where the options name it, else at the login.
fn friends_service(o: &Options) -> auberean_shell::FriendsService {
    match &o.gun_server {
        Some(host) => auberean_shell::FriendsService::At {
            host: host.clone(),
            port: o.gun_port,
            use_ticket: o.gun_ticket,
            ticket: o.gun_ticket_override.clone(),
        },
        None => auberean_shell::FriendsService::AtTheLogin,
    }
}

/// The settings folder: the variable's, else the user's application data.
fn settings_dir() -> PathBuf {
    if let Some(d) = std::env::var_os(SETTINGS_DIR) {
        return PathBuf::from(d);
    }
    if let Some(d) = std::env::var_os("APPDATA") {
        return PathBuf::from(d).join("Auberean");
    }
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(d).join("auberean");
    }
    std::env::var_os("HOME").map_or_else(
        || PathBuf::from("."),
        |h| PathBuf::from(h).join(".config").join("auberean"),
    )
}

/// The data files: the world's (portal, cell, high resolution) and the client's (those and the
/// English strings, in the order the client opens them, with the script library linked).
fn open(dir: &Path) -> Result<(WorldData, ClientData), String> {
    let found = install_dats(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
        let (_, p) = found.iter().find(|(n, _)| want(n))?;
        DatFile::open(p).ok()
    };
    let portal = open(&|n| *n == DatName::Portal).ok_or("no portal file")?;
    let cell = open(&|n| *n == DatName::Cell(1)).ok_or("no cell file")?;
    let ctx = auberean_assets::Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
    let records = auberean_world::records::WorldRecords::new(
        portal,
        cell,
        open(&|n| *n == DatName::HighRes),
        ctx,
    );
    let world = WorldData::new(records)?;
    let portal = Arc::new(open(&|n| *n == DatName::Portal).ok_or("no portal file")?);
    let mut files = vec![Arc::clone(&portal)];
    files.extend(open(&|n| *n == DatName::Local("English".into())).map(Arc::new));
    files.extend(open(&|n| *n == DatName::Cell(1)).map(Arc::new));
    files.extend(open(&|n| *n == DatName::HighRes).map(Arc::new));
    let program = auberean_client_sdk::runtime::link_scripts(&portal)?;
    let data = ClientData {
        files: DataFiles {
            files,
            language: ENGLISH,
        },
        scripts: Some((portal, Arc::new(program))),
    };
    Ok((world, data))
}

/// The interface's records: the English strings first, then the portal and high-resolution files.
fn ui_assets(dir: &Path) -> Result<Arc<auberean_ui::DatAssets>, String> {
    let found = install_dats(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
        let (_, p) = found.iter().find(|(n, _)| want(n))?;
        DatFile::open(p).ok()
    };
    let portal = open(&|n| *n == DatName::Portal).ok_or("no portal file")?;
    let ctx = auberean_assets::Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
    let mut dats = Vec::new();
    dats.extend(open(&|n| *n == DatName::Local("English".into())));
    dats.push(portal);
    dats.extend(open(&|n| *n == DatName::HighRes));
    Ok(Arc::new(auberean_ui::DatAssets::new(dats, ctx)))
}

/// The desktop as the shell's host.
struct Desktop {
    start: Instant,
    socket: Option<UdpSocket>,
    window: Option<Arc<Window>>,
    settings: PathBuf,
    /// The install's folder: the game's own files beside its data files.
    install: PathBuf,
    held: bool,
    seed: u64,
    buf: Vec<u8>,
    friends: friends::FriendsSocket,
}

impl Host for Desktop {
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn unix_time(&self) -> u32 {
        u32::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        )
        .unwrap_or(0)
    }

    fn random(&mut self, out: &mut [u8]) {
        use std::hash::{BuildHasher, Hasher};
        for chunk in out.chunks_mut(8) {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(self.seed);
            self.seed = h.finish();
            for (b, v) in chunk.iter_mut().zip(self.seed.to_le_bytes()) {
                *b = v;
            }
        }
    }

    fn connect(&mut self, server: SocketAddr) -> Result<(), String> {
        // A server on this machine is reached from a loopback address, which no firewall asks
        // about.
        let local = if server.ip().is_loopback() {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        };
        let s = UdpSocket::bind(SocketAddr::new(local, 0)).map_err(|e| e.to_string())?;
        s.set_nonblocking(true).map_err(|e| e.to_string())?;
        self.socket = Some(s);
        Ok(())
    }

    fn send(&mut self, to: SocketAddr, bytes: &[u8]) {
        if let Some(s) = &self.socket {
            let _ = s.send_to(bytes, to);
        }
    }

    fn receive(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        let s = self.socket.as_ref()?;
        match s.recv_from(&mut self.buf) {
            Ok((n, from)) => Some((from, self.buf[..n].to_vec())),
            Err(_) => None,
        }
    }

    fn hold_pointer(&mut self, held: bool) {
        if held == self.held {
            return;
        }
        self.held = held;
        if let Some(w) = &self.window {
            if held {
                if w.set_cursor_grab(CursorGrabMode::Locked).is_err() {
                    let _ = w.set_cursor_grab(CursorGrabMode::Confined);
                }
            } else {
                let _ = w.set_cursor_grab(CursorGrabMode::None);
            }
            w.set_cursor_visible(!held);
        }
    }

    fn set_title(&mut self, title: &str) {
        if let Some(w) = &self.window {
            w.set_title(title);
        }
    }

    fn load_settings(&self) -> Option<String> {
        std::fs::read_to_string(self.settings.join(SETTINGS_FILE)).ok()
    }

    fn save_settings(&mut self, text: &str) {
        let _ = std::fs::create_dir_all(&self.settings);
        if let Err(e) = std::fs::write(self.settings.join(SETTINGS_FILE), text) {
            tracing::warn!("saving the settings: {e}");
        }
    }

    /// The player's file in the settings folder, else the install's own: the key maps a player
    /// saved with the September 2013 client are read from beside its data files, and saved here.
    fn load_file(&self, name: &str) -> Option<Vec<u8>> {
        let plain = Path::new(name).file_name()?;
        std::fs::read(self.settings.join(plain))
            .or_else(|_| std::fs::read(self.install.join(plain)))
            .ok()
    }

    fn save_file(&mut self, name: &str, bytes: &[u8]) -> bool {
        let Some(plain) = Path::new(name).file_name() else {
            return false;
        };
        let _ = std::fs::create_dir_all(&self.settings);
        match std::fs::write(self.settings.join(plain), bytes) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("saving {name}: {e}");
                false
            }
        }
    }

    fn carries_friends(&self) -> bool {
        true
    }

    fn friends_dial(&mut self, host: &str, port: u16) {
        self.friends.dial(host, port);
    }

    fn friends_send(&mut self, bytes: &[u8]) {
        self.friends.send(bytes);
    }

    fn friends_close(&mut self) {
        self.friends.close();
    }

    fn friends_news(&mut self) -> Option<auberean_shell::FriendsNews> {
        self.friends.news()
    }
}

struct App<'w> {
    shell: Shell<'w, Desktop>,
    world: &'w WorldData,
    present: Option<present::Presenter>,
    script: Option<script::Script>,
    shots: PathBuf,
    shot_next: Option<PathBuf>,
    shot_count: u32,
    ctrl: bool,
    alt: bool,
    failed: Option<String>,
    /// The interface's cursor shown over the window (by its picture), and the cursors made.
    cursor: Option<u32>,
    cursors: std::collections::HashMap<u32, winit::window::CustomCursor>,
}

impl App<'_> {
    /// Show the interface's cursor over the window: the window system's cursor made from its
    /// picture once, then reused; no picture leaves the window system's own.
    fn set_cursor(
        &mut self,
        el: &ActiveEventLoop,
        c: Option<&auberean_shell::interface::CursorImage>,
    ) {
        let want = c.map(|c| c.picture);
        if want == self.cursor {
            return;
        }
        let Some(w) = self.shell.host.window.clone() else {
            return;
        };
        let Some(c) = c else {
            w.set_cursor(winit::window::Cursor::Icon(
                winit::window::CursorIcon::Default,
            ));
            self.cursor = None;
            return;
        };
        let made = match self.cursors.entry(c.picture) {
            std::collections::hash_map::Entry::Occupied(e) => e.get().clone(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let (Ok(width), Ok(height)) = (u16::try_from(c.width), u16::try_from(c.height))
                else {
                    return;
                };
                let hot = |v: u32, n: u16| u16::try_from(v).unwrap_or(0).min(n.saturating_sub(1));
                match winit::window::CustomCursor::from_rgba(
                    c.rgba.clone(),
                    width,
                    height,
                    hot(c.hot_x, width),
                    hot(c.hot_y, height),
                ) {
                    Ok(source) => e.insert(el.create_custom_cursor(source)).clone(),
                    Err(err) => {
                        tracing::warn!("cursor {:#010x}: {err}", c.picture);
                        self.cursor = want;
                        return;
                    }
                }
            }
        };
        w.set_cursor(made);
        self.cursor = want;
    }

    fn send(&mut self, e: &Event) {
        match e {
            Event::Resized { width, height } => {
                if let Some(p) = &mut self.present {
                    p.resize(*width, *height);
                }
            }
            Event::Key {
                key: Key::ControlLeft | Key::ControlRight,
                down,
                ..
            } => self.ctrl = *down,
            Event::Key {
                key: Key::AltLeft | Key::AltRight,
                down,
                ..
            } => self.alt = *down,
            // Alt+Enter: full screen, or back to the window.
            // D2-08334, D2-08327
            Event::Key {
                key: Key::Enter,
                down: true,
                repeat: false,
            } if self.alt => {
                let full = !self.shell.settings.full_screen;
                self.shell.settings.full_screen = full;
                if let Some(w) = &self.shell.host.window {
                    w.set_fullscreen(full.then_some(winit::window::Fullscreen::Borderless(None)));
                }
                return;
            }
            _ => {}
        }
        self.shell.event(e);
    }

    fn redraw(&mut self, el: &ActiveEventLoop) {
        if let Some(s) = &mut self.script {
            let asked = s.step(
                self.shell.host.now(),
                self.shell.phase(),
                &self.shell.scene.status,
            );
            for e in &asked.events {
                self.shell.event(e);
            }
            if asked.shot.is_some() {
                self.shot_next = asked.shot;
            }
            if asked.quit {
                self.shell.event(&Event::Close);
            }
        }
        let shot = self.shot_next.take();
        let world = self.world;
        let out = self.shell.frame();
        let quit = out.quit;
        let cursor = out.overlay.cursor.clone();
        if let Some(p) = &mut self.present {
            let mut load = |id: u32| auberean_draw::picture::load(&world.records, id);
            p.present(
                out.world,
                out.portal.map(|f| (f, out.portal_alpha)),
                out.veil,
                out.overlay,
                &out.views,
                &mut load,
                shot.as_deref(),
            );
        }
        self.set_cursor(el, cursor.as_deref());
        if quit {
            el.exit();
        }
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.present.is_some() {
            return;
        }
        let (w, h) = self.shell.size;
        let attrs = icon::with_icon(Window::default_attributes())
            .with_title("Auberean")
            .with_inner_size(winit::dpi::PhysicalSize::new(w, h))
            // D2-08325
            .with_fullscreen(
                self.shell
                    .settings
                    .full_screen
                    .then_some(winit::window::Fullscreen::Borderless(None)),
            );
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.failed = Some(format!("no window: {e}"));
                el.exit();
                return;
            }
        };
        match present::Presenter::new(Arc::clone(&window)) {
            Ok(p) => {
                tracing::info!(adapter = %p.renderer.adapter, "drawing");
                self.present = Some(p);
            }
            Err(e) => {
                self.failed = Some(e);
                el.exit();
                return;
            }
        }
        let size = window.inner_size();
        self.shell.host.window = Some(window);
        self.send(&Event::Resized {
            width: size.width,
            height: size.height,
        });
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.send(&Event::Close),
            WindowEvent::Resized(s) => self.send(&Event::Resized {
                width: s.width,
                height: s.height,
            }),
            WindowEvent::Focused(f) => self.send(&Event::Focus(f)),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    // Ctrl+P saves a screenshot, as the game's own key does.
                    if code == winit::keyboard::KeyCode::KeyP
                        && event.state == ElementState::Pressed
                        && self.ctrl
                    {
                        self.shot_count += 1;
                        self.shot_next =
                            Some(self.shots.join(format!("shot-{:03}.png", self.shot_count)));
                    }
                    if let Some(key) = keys::key(code) {
                        self.send(&Event::Key {
                            key,
                            down: event.state == ElementState::Pressed,
                            repeat: event.repeat,
                        });
                    }
                }
                if let Some(t) = event.text.filter(|_| event.state == ElementState::Pressed) {
                    self.send(&Event::Text(t.to_string()));
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(button) = keys::button(button) {
                    self.send(&Event::Button {
                        button,
                        down: state == ElementState::Pressed,
                    });
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                #[allow(clippy::cast_possible_truncation)]
                self.send(&Event::Pointer {
                    x: position.x as f32,
                    y: position.y as f32,
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                #[allow(clippy::cast_possible_truncation)]
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 120.0) as f32,
                };
                self.send(&Event::Wheel { notches });
            }
            WindowEvent::RedrawRequested => self.redraw(el),
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, e: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = e {
            #[allow(clippy::cast_possible_truncation)]
            self.send(&Event::Motion {
                dx: delta.0 as f32,
                dy: delta.1 as f32,
            });
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(w) = &self.shell.host.window {
            w.request_redraw();
        }
    }
}

fn run() -> Result<(), String> {
    let o = options()?;
    tracing_subscriber::fmt()
        .with_max_level(if o.verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .with_writer(std::io::stderr)
        .init();
    let (world, data) = open(&o.dats)?;
    let settings = settings_dir();
    tracing::info!(settings = %settings.display(), "settings");
    let shots = o
        .shots
        .clone()
        .unwrap_or_else(|| settings.join("screenshots"));
    let server = o
        .server
        .unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9000));
    let assets = ui_assets(&o.dats)?;
    let login = o
        .account
        .as_ref()
        .map(|(account, password)| auberean_interface::Login {
            server,
            account: account.clone(),
            password: password.clone(),
        });
    let mut retail = auberean_interface::Retail::new(assets, o.size.unwrap_or((1024, 768)), login);
    if let Some((account, password)) = &o.login {
        retail = retail.with_auto(auberean_shell::dev::DevLogin::new(
            server,
            account.clone(),
            password.clone(),
            o.character.clone(),
        ));
    } else if o.account.is_none() {
        tracing::warn!("no account: log in with --account ACCOUNT:PASSWORD");
    }
    let interface: Box<dyn Interface> = Box::new(retail);
    let host = Desktop {
        start: Instant::now(),
        socket: None,
        window: None,
        settings,
        install: o.dats.clone(),
        held: false,
        seed: u64::from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos()),
        ),
        buf: vec![0; 65536],
        friends: friends::FriendsSocket::default(),
    };
    let mut shell = Shell::new(host, &world, data, interface);
    shell.dev_overlay = o.overlay;
    shell.friends = friends_service(&o);
    if let Some(s) = o.size {
        shell.size = s;
    }
    let script = match &o.script {
        Some(f) => Some(script::Script::load(f, shots.clone())?),
        None => None,
    };
    if let Some(seconds) = o.no_window {
        return no_window(shell, script, seconds);
    }
    let mut app = App {
        shell,
        world: &world,
        present: None,
        script,
        shots,
        shot_next: None,
        shot_count: 0,
        ctrl: false,
        alt: false,
        failed: None,
        cursor: None,
        cursors: std::collections::HashMap::new(),
    };
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    el.set_control_flow(ControlFlow::Poll);
    el.run_app(&mut app).map_err(|e| e.to_string())?;
    if let Some(e) = app.failed {
        return Err(e);
    }
    Ok(())
}

/// Run with no window for `seconds`: the session, the world and its frames, no picture.
fn no_window(
    mut shell: Shell<'_, Desktop>,
    mut script: Option<script::Script>,
    seconds: f64,
) -> Result<(), String> {
    let mut printed = 0.0;
    loop {
        let now = shell.host.now();
        if let Some(s) = &mut script {
            let asked = s.step(now, shell.phase(), &shell.scene.status);
            for e in &asked.events {
                shell.event(e);
            }
            if asked.quit {
                shell.event(&Event::Close);
            }
        }
        let started = Instant::now();
        let out = shell.frame();
        let quit = out.quit;
        let batches = out.world.map_or(0, |f| f.objects.len() + f.terrain.len());
        let took = started.elapsed().as_secs_f64();
        if took > 0.1 {
            tracing::info!(seconds = took, batches, "a slow frame");
        }
        if now - printed >= 2.0 || quit || now > seconds {
            printed = now;
            for l in shell.status_lines() {
                println!("{now:7.2} {l}");
            }
        }
        if quit || now > seconds {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn main() -> ExitCode {
    // `--version` alone: what this build is, and nothing opened.
    if std::env::args().skip(1).eq(["--version"]) {
        print!("{}", version_text());
        return ExitCode::SUCCESS;
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("auberean: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What `auberean --version` prints: the version, and the commit and target a release build
/// was made from (`unknown` in a build that did not say).
fn version_text() -> String {
    format!(
        "auberean {}\ncommit: {}\ntarget: {}\n",
        env!("CARGO_PKG_VERSION"),
        option_env!("AUBEREAN_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("AUBEREAN_BUILD_TARGET").unwrap_or("unknown"),
    )
}

#[cfg(test)]
mod version_tests {
    //! Behaviour: none (tooling: what `--version` prints, which the release workflow checks)
    use super::version_text;

    #[test]
    fn version_names_the_program_its_version_its_commit_and_its_target() {
        let text = version_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert_eq!(lines[0], concat!("auberean ", env!("CARGO_PKG_VERSION")));
        assert!(lines[1].starts_with("commit: "), "{text}");
        assert!(lines[2].starts_with("target: "), "{text}");
    }
}
