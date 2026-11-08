//! The desktop client: a window in which Asheron's Call 2's world is drawn and played. The shell's
//! host on the desktop: the window and its events, the device, the network, the clock, the
//! settings folder and the log.
//!
//! **Depends on** the shell (`auberean-shell`), the game's interface (`auberean-interface`,
//! `auberean-ui`) and the world it shows (`auberean-scene`), the
//! world's device (`auberean-gpu`, `auberean-present`) and its frames (`auberean-draw`), the data files
//! (`auberean-dat`, `auberean-assets`, `auberean-world`, `auberean-client-sdk`), the shared math
//! (`dereth-primitives`), the console (`dereth-console`), the window system (`winit`), the device (`wgpu`) and the sound device
//! (`dereth-audio-out`, through the sound engine's output seam in `dereth-audio`). **Used by** nothing:
//! it is the `auberean` program.
//!
//! ```text
//! auberean --dats <install folder> [--overlay-dat-dir DIR] [--server 127.0.0.1:9000]
//!          [--account ACCOUNT[:PASSWORD]]
//!          [--glsticketdirect SECRET]
//!          [--gunserver HOST [--gunport PORT] [--gunticket] [--gunticketoverride TICKET]]
//!          [--size 1280x960] [--dev-login ACCOUNT:PASSWORD [--dev-character NAME]]
//!          [--dev-overlay] [--dev-script FILE] [--shots DIR] [--no-window SECONDS] [--no-sound]
//!          [--verbose]
//! ```
//!
//! The install folder is the September 2013 client's: its data files are the world and the
//! interface's records, and answer the server's question of which data the client has.
//! `--overlay-dat-dir` names a world's overlay to read them through; the files are never written,
//! and an overlay made against other files stops the client before it starts. The
//! interface is the game's own (`auberean-interface`): with `--account` it logs in as the launcher
//! would have it log in, and the character screens take it from there. `--dev-login` instead logs
//! in and enters the world by itself (making the character, a human, when the account has none of
//! that name) while the screens follow; `--dev-overlay` writes the player's place and the
//! session's state over the world; `--dev-script` plays a script of input and screenshots
//! (`script.rs`). Screenshots (the screenshot key's, and a script's) go to `--shots`, by default
//! the settings folder's `screenshots`. `--no-window` runs everything but the window and the device
//! for that many seconds (the world is still framed, with no picture made), printing the status
//! lines.
//! The game's sound plays through the default output device; `--no-sound` runs silent.
//!
//! `--glsticketdirect` gives the login its secret directly, as the September 2013 client's own option
//! does (it stands for the password after `--account`).
//!
//! The command line is read as the September 2013 client reads its own, and takes its options too:
//! `-h`/`--host` and `-p`/`--port` name the server (instead of `--server`), `-a`/`--account` the
//! account (in small letters), `-z` is `--glsticketdirect`, `--width` and `--height` the window,
//! `--prefs` the preferences file (in the settings folder unless a path says otherwise), `-u`/`--user`
//! the character to go into the world with as soon as the account's are listed, and `-r`/`--create`
//! the one to go in with when `--user` names none. An option
//! may be written `-name`, `--name` or `/name`. Named with a host or an account, the line also
//! needs the other, as the client's own does. A line that does not read (`-?` included) lists
//! every option.
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
//! The files the game's scripts write and read (a chat log, an options export) are kept there too,
//! by bare names only, where the September 2013 client keeps them beside itself.
//!
//! **Must never** hold the game: what a frame shows and what the player's input does are the
//! shell's (`auberean-shell`); this program only brings the platform under it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod friends;
mod icon;
mod keys;
mod local;
mod perf;
mod script;

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use auberean_client_sdk::dat::locate::{install_dats, DatName};
use auberean_client_sdk::dat::overlay::WorldOverlay;
use auberean_client_sdk::dat::DatFile;
use auberean_interface::data::{open_data, ui_assets, Install};
use auberean_scene::WorldData;
use auberean_shell::command_line::{parse_args, set_error_text, Arg, ArgList, ClientLine, Reader};
use auberean_shell::input::{Event, Key};
use auberean_shell::interface::Interface;
use auberean_shell::settings::{held_to_picture, in_picture_pixels, on_picture, on_window};
use auberean_shell::{Host, Presentation, Shell};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

/// The variable naming the settings folder.
const SETTINGS_DIR: &str = "AUBEREAN_SETTINGS_DIR";
/// The settings file in it.
const SETTINGS_FILE: &str = "Preferences.ini";

#[derive(Debug, Default)]
struct Options {
    dats: PathBuf,
    world_overlay: Option<PathBuf>,
    server: Option<SocketAddr>,
    size: Option<(u32, u32)>,
    login: Option<(String, String)>,
    account: Option<(String, String)>,
    character: Option<String>,
    overlay: bool,
    script: Option<PathBuf>,
    /// Where each frame's times and device work are written.
    perf: Option<PathBuf>,
    shots: Option<PathBuf>,
    verbose: bool,
    no_window: Option<f64>,
    no_sound: bool,
    gun_server: Option<String>,
    gun_port: Option<u16>,
    gun_ticket: bool,
    gun_ticket_override: Option<String>,
    /// What the command line sets over the settings read.
    line: ClientLine,
    /// The preferences file named (else the settings folder's own).
    prefs: Option<String>,
    /// The language named.
    language: Option<String>,
    /// The top gore level and blood color forced (0: the language's).
    forced_gore: (u32, u32),
    /// The creation screen offers the administrator's character.
    admin_client: bool,
    /// The character named to play.
    user: Option<String>,
    /// The character named to make and play.
    create: Option<String>,
}

/// This program's own options, beside the September 2013 client's: each one's form (what it takes
/// and keeps), long name, help and a switch's value.
const OWN_OPTIONS: [(u32, &str, &str, i32); 14] = [
    (
        0x11,
        "adminclient",
        "Show the administrator's button on the creation screen.",
        0,
    ),
    (
        0x32,
        "dats",
        "<folder>: The September 2013 client's install folder.",
        0,
    ),
    (
        0x32,
        "overlay-dat-dir",
        "<folder>: A world's overlay to read the data files through.",
        0,
    ),
    (0x32, "server", "<IP:port>: The server to log in to.", 0),
    (0x32, "size", "<WxH>: The window's size.", 0),
    (
        0x32,
        "dev-login",
        "<account:password>: Log in and enter the world by itself.",
        0,
    ),
    (
        0x32,
        "dev-character",
        "<name>: The character the developer's login enters as.",
        0,
    ),
    (
        0x11,
        "dev-overlay",
        "Show the session's state over the world.",
        1,
    ),
    (
        0x32,
        "dev-script",
        "<file>: Play a script of input and screenshots.",
        0,
    ),
    (
        0x32,
        "dev-perf",
        "<file>: Write each frame's times and device work there.",
        0,
    ),
    (0x32, "shots", "<folder>: Where screenshots go.", 0),
    (0x11, "verbose", "Write more to the log.", 1),
    (0x11, "no-sound", "Play no sound.", 1),
    (
        0x32,
        "no-window",
        "<seconds>: Run with no window for that long.",
        0,
    ),
];

/// The command line: the September 2013 client's options, read as it reads them, and this
/// program's own.
#[derive(Debug, Default)]
struct Line {
    client: ClientLine,
    o: Options,
    /// The account as given, which may carry the password after a colon.
    account: Option<String>,
    /// The options the line named.
    given: Vec<String>,
}

impl Line {
    fn fail(&mut self, text: &str) -> bool {
        set_error_text(self, text);
        false
    }
}

impl Reader for Line {
    fn build(&mut self, list: &mut ArgList) {
        self.client.build(list);
        for (form, name, help, value) in OWN_OPTIONS {
            list.add_cmd(form, 0, name, help, value);
        }
    }

    fn evaluate(&mut self, arg: &Arg, value: &str) -> bool {
        self.given.push(arg.name.clone());
        let pair = |v: &str| {
            let (a, p) = v.split_once(':').unwrap_or((v, ""));
            (a.to_owned(), p.to_owned())
        };
        match arg.name.as_str() {
            "adminclient" => self.o.admin_client = true,
            "dats" => self.o.dats = PathBuf::from(value),
            "overlay-dat-dir" => self.o.world_overlay = Some(PathBuf::from(value)),
            "server" => match value.parse() {
                Ok(a) => self.o.server = Some(a),
                Err(e) => return self.fail(&format!("--server: {e} (want IP:PORT)")),
            },
            "size" => {
                let size = value
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
                match size {
                    Some(s) => self.o.size = Some(s),
                    None => return self.fail("--size: want WxH"),
                }
            }
            "dev-login" => self.o.login = Some(pair(value)),
            "dev-character" => self.o.character = Some(value.to_owned()),
            "dev-overlay" => self.o.overlay = true,
            "dev-script" => self.o.script = Some(PathBuf::from(value)),
            "dev-perf" => self.o.perf = Some(PathBuf::from(value)),
            "shots" => self.o.shots = Some(PathBuf::from(value)),
            "verbose" => self.o.verbose = true,
            "no-sound" => self.o.no_sound = true,
            "no-window" => match value.parse() {
                Ok(v) => self.o.no_window = Some(v),
                Err(e) => return self.fail(&format!("--no-window: {e} (want seconds)")),
            },
            _ => {
                if arg.name == "account" {
                    self.account = Some(value.to_owned());
                }
                return self.client.evaluate(arg, value);
            }
        }
        true
    }

    /// The install folder is needed. Launched the way the launcher launches the client (a host or
    /// an account named), the client's own check; else the friends service's ticket still needs
    /// the service.
    fn done(&mut self) -> bool {
        if self.o.dats.as_os_str().is_empty() {
            return self.fail("--dats <install folder> is needed");
        }
        if !self.client.host.is_empty() || !self.client.account.is_empty() {
            return self.client.done();
        }
        if self.client.gun_ticket != 0 && self.client.gun_server.is_none() {
            return self.fail("--gunticket needs --gunserver");
        }
        true
    }

    fn error_text(&mut self) -> &mut String {
        self.client.error_text()
    }

    fn output(&mut self, text: &str) {
        eprint!("{text}");
    }
}

// D2-00096
fn options() -> Result<Options, String> {
    let words: Vec<String> = std::env::args().collect();
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let mut line = Line::default();
    if !parse_args(&mut line, &words, true) {
        return Err("the command line does not read".into());
    }
    let c = &line.client;
    let mut o = line.o;
    // The account as the client keeps it (in small letters), with this program's password after a
    // colon, which the ticket given directly stands for.
    if let Some(raw) = &line.account {
        let (_, password) = raw.split_once(':').unwrap_or((raw, ""));
        let name = c
            .account
            .split_once(':')
            .map_or(c.account.as_str(), |(a, _)| a);
        o.account = Some((name.to_owned(), password.to_owned()));
    }
    if let Some(secret) = c.ticket.clone() {
        let Some((_, password)) = o.account.as_mut() else {
            return Err("--glsticketdirect needs --account".into());
        };
        *password = secret;
    }
    if !c.host.is_empty() {
        use std::net::ToSocketAddrs;
        let port = u16::try_from(c.port).unwrap_or(0);
        o.server = (c.host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| format!("--host {}: {e}", c.host))?
            .next();
    }
    let named = |n: &str| line.given.iter().any(|g| g == n);
    if named("width") || named("height") {
        if let (Ok(w), Ok(h)) = (u32::try_from(c.width), u32::try_from(c.height)) {
            if w > 0 && h > 0 {
                o.size = Some((w, h));
            }
        }
    }
    o.language.clone_from(&c.language);
    o.forced_gore = (c.top_gore.unwrap_or(0), u32::from(c.green_blood));
    o.gun_server.clone_from(&c.gun_server);
    o.gun_port = (c.gun_port != 0).then_some(c.gun_port);
    o.gun_ticket = c.gun_ticket != 0;
    o.gun_ticket_override.clone_from(&c.gun_ticket_override);
    o.line = c.clone();
    o.prefs.clone_from(&c.prefs);
    o.user.clone_from(&c.user);
    o.create.clone_from(&c.create);
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

/// The desktop as the shell's host.
struct Desktop {
    start: Instant,
    socket: Option<UdpSocket>,
    window: Option<Arc<Window>>,
    /// How the game is to be shown, asked for and not yet carried out.
    presentation: Option<Presentation>,
    /// The window's drawing area and the size of the picture filling it.
    picture_in_window: ([f32; 4], (u32, u32)),
    settings: PathBuf,
    /// The preferences file, in the settings folder unless named elsewhere.
    settings_file: PathBuf,
    /// The install's folder: the game's own files beside its data files.
    install: PathBuf,
    held: bool,
    /// A script is playing: the window hears it alone, and the real pointer is left alone.
    scripted: bool,
    seed: u64,
    buf: Vec<u8>,
    friends: friends::FriendsSocket,
    /// The sound device (none with `--no-sound`).
    sound: Option<dereth_audio_out::CpalOutput>,
}

impl Host for Desktop {
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn set_presentation(&mut self, presentation: &Presentation) {
        self.presentation = Some(*presentation);
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

    fn open_page(&mut self, url: &str) -> Result<(), i32> {
        open::that(url).map_err(|e| e.raw_os_error().unwrap_or(0))
    }

    fn error_box(&mut self, title: &str, text: &str) {
        // The box needs the pointer: one held for mouse look is let go of first.
        self.hold_pointer(false);
        dereth_console::error_box(title, text, 0);
    }

    fn hold_pointer(&mut self, held: bool) {
        if held == self.held {
            return;
        }
        self.held = held;
        if self.scripted {
            return;
        }
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

    fn set_input_method(&mut self, area: Option<auberean_shell::host::InputArea>) {
        let Some(w) = &self.window else {
            return;
        };
        w.set_ime_allowed(area.is_some());
        if let Some(a) = area {
            let (shown, drawn) = self.picture_in_window;
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = on_window((a.x as f32, a.y as f32), shown, drawn);
            #[allow(clippy::cast_precision_loss)]
            let (width, height) = on_window(
                (a.width as f32, a.height as f32),
                [0.0, 0.0, shown[2], shown[3]],
                drawn,
            );
            w.set_ime_cursor_area(
                winit::dpi::PhysicalPosition::new(x, y),
                winit::dpi::PhysicalSize::new(width, height),
            );
        }
    }

    fn load_settings(&self) -> Option<String> {
        std::fs::read_to_string(&self.settings_file).ok()
    }

    fn save_settings(&mut self, text: &str) {
        let _ = std::fs::create_dir_all(&self.settings);
        if let Err(e) = std::fs::write(&self.settings_file, text) {
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

    fn local_system(
        &self,
    ) -> Option<Box<dyn auberean_client_sdk::script::local::LocalSystem + Send>> {
        Some(Box::new(local::SettingsFolder {
            dir: self.settings.clone(),
        }))
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

    fn sound_output(&mut self) -> Option<&mut dyn dereth_audio::output::AudioOutput> {
        self.sound
            .as_mut()
            .map(|s| s as &mut dyn dereth_audio::output::AudioOutput)
    }

    fn friends_news(&mut self) -> Option<auberean_shell::FriendsNews> {
        self.friends.news()
    }
}

/// The device for `window`.
fn presenter(
    window: &Arc<Window>,
    presentation: &Presentation,
) -> Result<auberean_present::Presenter, String> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let size = window.inner_size();
    let surface = instance
        .create_surface(Arc::clone(window))
        .map_err(|e| format!("no surface for the window: {e}"))?;
    let mut p = auberean_present::Presenter::new(
        instance,
        surface,
        (size.width, size.height),
        presentation.antialiasing,
    )?;
    p.set_presentation(
        presentation.full_screen,
        presentation.triple_buffering,
        presentation.sync_to_display,
    );
    Ok(p)
}

/// The sizes the display's row offers: those of a monitor's modes of full color.
/// Whether a window's event comes from the keyboard, the mouse or the window being the active one:
/// what a script playing stands in for, so it plays the same in a window in the background.
const fn from_the_devices(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::Focused(_)
            | WindowEvent::KeyboardInput { .. }
            | WindowEvent::ModifiersChanged(_)
            | WindowEvent::Ime(_)
            | WindowEvent::MouseInput { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::MouseWheel { .. }
    )
}

fn monitor_sizes(monitor: Option<winit::monitor::MonitorHandle>) -> Vec<(u32, u32)> {
    let Some(m) = monitor else {
        return Vec::new();
    };
    let mut sizes: Vec<(u32, u32)> = m
        .video_modes()
        .filter(|v| v.bit_depth() >= 32)
        .map(|v| (v.size().width, v.size().height))
        .collect();
    sizes.sort_unstable();
    sizes.dedup();
    sizes
}

/// The window to ask for to show a picture of this size. On macOS a window goes by points, of which
/// a Retina screen has two pixels to one; elsewhere by the screen's pixels.
fn window_size_for(size: (u32, u32)) -> winit::dpi::Size {
    if cfg!(target_os = "macos") {
        winit::dpi::LogicalSize::new(size.0, size.1).into()
    } else {
        winit::dpi::PhysicalSize::new(size.0, size.1).into()
    }
}

/// How many of the screen's pixels a window gives each pixel of its picture (see
/// [`window_size_for`]).
fn window_scale(window: &Window) -> f64 {
    if cfg!(target_os = "macos") {
        window.scale_factor()
    } else {
        1.0
    }
}

struct App<'w> {
    shell: Shell<'w, Desktop>,
    world: &'w WorldData,
    present: Option<auberean_present::Presenter>,
    /// Each frame's times and device work written out, when asked for.
    perf: Option<perf::PerfLog>,
    /// How the game is shown now.
    shown: Presentation,
    /// The pointer is on a bar, off the picture.
    pointer_on_bar: bool,
    /// The buttons pressed on a bar and not yet let go.
    pressed_on_bar: Vec<auberean_shell::input::Button>,
    /// The window's drawing area now.
    script: Option<script::Script>,
    marks: script::Marks,
    shots: PathBuf,
    shot_next: Option<PathBuf>,
    shot_count: u32,
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
                self.shell.toggle_full_screen();
                return;
            }
            _ => {}
        }
        self.shell.event(e);
    }

    /// The window's drawing area is `size`: the device fills it, and the game is told the size of
    /// the picture it draws.
    fn window_sized(&mut self, size: (u32, u32)) {
        let scale = self.shell.host.window.as_deref().map_or(1.0, window_scale);
        let drawn = self.shown.drawn_in(in_picture_pixels(size, scale));
        if let Some(p) = &mut self.present {
            p.resize(size.0, size.1);
            p.draw_at(drawn.0, drawn.1);
            p.keep_shape(self.shown.full_screen);
        }
        self.shell.host.picture_in_window = (self.shown.shown_in(drawn, size), drawn);
        self.send(&Event::Resized {
            width: drawn.0,
            height: drawn.1,
        });
    }

    /// Another device for window `w`, as the game is now shown. The game ends without one.
    fn make_device(&mut self, el: &ActiveEventLoop, w: &Arc<Window>) {
        // The window's surface is let go before another is made on it.
        self.present = None;
        match presenter(w, &self.shown) {
            Ok(made) => self.present = Some(made),
            Err(e) => {
                self.failed = Some(e);
                el.exit();
            }
        }
    }

    /// The game shown as the rendering preferences now ask: full screen fills the monitor as it
    /// is, which never changes its display mode, or the window at its size;
    /// the device made again for the edges smoothed or not; the way pictures reach the screen.
    // D2-08343, D2-09722
    fn show(&mut self, el: &ActiveEventLoop, p: Presentation) {
        let Some(w) = self.shell.host.window.clone() else {
            return;
        };
        let was = std::mem::replace(&mut self.shown, p);
        if p.full_screen != was.full_screen {
            w.set_fullscreen(
                p.full_screen
                    .then(|| Fullscreen::Borderless(w.current_monitor())),
            );
        }
        if !p.full_screen && (was.full_screen || p.size != was.size) {
            let _ = w.request_inner_size(window_size_for(p.size));
        }
        if p.antialiasing != was.antialiasing {
            self.make_device(el, &w);
        } else if let Some(present) = &mut self.present {
            present.set_presentation(p.full_screen, p.triple_buffering, p.sync_to_display);
        }
        // In full screen the window stays as it is while the picture's size changes; a new device
        // starts at the window's own size; a window leaving full screen fills again at once.
        if p.full_screen || was.full_screen || p.antialiasing != was.antialiasing {
            let size = w.inner_size();
            self.window_sized((size.width, size.height));
        }
    }

    // D2-00029
    fn redraw(&mut self, el: &ActiveEventLoop) {
        if let Some(p) = self.shell.host.presentation.take() {
            self.show(el, p);
        }
        if let Some(s) = &mut self.script {
            let asked = s.step(
                self.shell.host.now(),
                self.shell.phase(),
                &self.shell.scene.status,
            );
            let playing = s.playing();
            for e in &asked.events {
                self.send(e);
            }
            if asked.shot.is_some() {
                self.shot_next = asked.shot;
            }
            if let Some(m) = &asked.mark {
                self.marks.print(m, self.shell.client.as_ref());
            }
            if asked.quit {
                self.shell.event(&Event::Close);
            }
            // The script over, the window hears the devices again, starting with its own state.
            if self.shell.host.scripted && !playing {
                self.shell.host.scripted = false;
                if let Some(active) = self.shell.host.window.as_ref().map(|w| w.has_focus()) {
                    self.send(&Event::Focus(active));
                }
            }
        }
        let mut shot = self.shot_next.take();
        let world = self.world;
        let settings = &self.shell.settings;
        let brightness = settings.full_screen.then(|| settings.screen_brightness());
        let began = Instant::now();
        let out = self.shell.frame();
        let shell_took = began.elapsed();
        if out.screenshot {
            let (n, path) = next_free_shot(&self.shots, self.shot_count);
            self.shot_count = n;
            shot = Some(path);
        }
        let quit = out.quit;
        let cursor = out.overlay.cursor.clone();
        if let Some(p) = &mut self.present {
            p.set_brightness(brightness);
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
            let work = p.work();
            if let Some(log) = &mut self.perf {
                log.frame(began, shell_took, self.shell.costs, work);
            }
            if p.hidden() {
                std::thread::sleep(hidden_rest(began.elapsed()));
            }
        }
        if let Some(why) = self.present.as_ref().and_then(|p| p.renderer.device_lost()) {
            tracing::error!(%why, "the graphics device was lost: making another");
            if let Some(w) = self.shell.host.window.clone() {
                self.make_device(el, &w);
                let size = w.inner_size();
                self.window_sized((size.width, size.height));
            }
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
        self.shell
            .set_display_modes(&monitor_sizes(el.primary_monitor()));
        let (w, h) = self.shell.size;
        let attrs = icon::with_icon(Window::default_attributes())
            .with_title("Auberean")
            .with_inner_size(window_size_for((w, h)))
            // D2-08325
            .with_fullscreen(
                self.shell
                    .settings
                    .full_screen
                    .then(|| Fullscreen::Borderless(el.primary_monitor())),
            );
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.failed = Some(format!("no window: {e}"));
                el.exit();
                return;
            }
        };
        self.shown = self.shell.settings.presentation();
        match presenter(&window, &self.shown) {
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
        self.window_sized((size.width, size.height));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.shell.host.scripted && from_the_devices(&event) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => self.send(&Event::Close),
            WindowEvent::Resized(s) => self.window_sized((s.width, s.height)),
            WindowEvent::Focused(f) => self.send(&Event::Focus(f)),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
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
                    let down = state == ElementState::Pressed;
                    // A press on a bar is off the picture, and so is its release.
                    if down && self.pointer_on_bar {
                        self.pressed_on_bar.push(button);
                    } else if !down && self.pressed_on_bar.contains(&button) {
                        self.pressed_on_bar.retain(|b| *b != button);
                    } else {
                        self.send(&Event::Button { button, down });
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                #[allow(clippy::cast_possible_truncation)]
                let (x, y) = on_picture(
                    (position.x as f32, position.y as f32),
                    self.shell.host.picture_in_window.0,
                    self.shell.size,
                );
                let ((x, y), on) = held_to_picture((x, y), self.shell.size);
                self.pointer_on_bar = !on;
                self.send(&Event::Pointer { x, y });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                #[allow(clippy::cast_possible_truncation)]
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 120.0) as f32,
                };
                self.send(&Event::Wheel { notches });
            }
            // What the input method wrote is typed as the keys' own text is.
            WindowEvent::Ime(winit::event::Ime::Commit(t)) => self.send(&Event::Text(t)),
            WindowEvent::Ime(winit::event::Ime::Preedit(t, _)) => {
                self.send(&Event::Composing(t));
            }
            WindowEvent::RedrawRequested => self.redraw(el),
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, e: DeviceEvent) {
        if self.shell.host.scripted {
            return;
        }
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

/// The world's overlay named on the command line, checked against every file it would lie over: a
/// world's overlay the client can't take is not played with the installed files instead.
fn world_overlay(
    dir: Option<&Path>,
    found: &[(DatName, PathBuf)],
) -> Result<Option<WorldOverlay>, String> {
    let Some(dir) = dir else {
        return Ok(None);
    };
    let overlay = WorldOverlay::open(dir).map_err(|e| e.to_string())?;
    for (_, path) in found {
        let file = DatFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        overlay.over(file).map_err(|e| e.to_string())?;
    }
    tracing::info!(
        world = overlay.world(),
        "reading the data files through a world's overlay"
    );
    Ok(Some(overlay))
}

// D2-00130, D2-01703
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
    let settings = settings_dir();
    // D2-16047, D2-16017
    let settings_file = settings.join(o.prefs.as_deref().unwrap_or(SETTINGS_FILE));
    let preferences = std::fs::read_to_string(&settings_file).unwrap_or_default();
    let found = install_dats(&o.dats).map_err(|e| format!("{}: {e}", o.dats.display()))?;
    let overlay = world_overlay(o.world_overlay.as_deref(), &found)?;
    let install = Install {
        found: found.iter().map(|(n, _)| n.clone()).collect(),
        open: &|name| {
            let (_, path) = found.iter().find(|(n, _)| n == name)?;
            let file = DatFile::open(path).ok()?;
            match &overlay {
                Some(w) => w.over(file).ok(),
                None => Some(file),
            }
        },
    };
    let (world, data, language) = open_data(&install, o.language.as_deref(), &preferences)?;
    tracing::info!(settings = %settings.display(), "settings");
    let shots = o
        .shots
        .clone()
        .unwrap_or_else(|| settings.join("screenshots"));
    let server = o
        .server
        .unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9000));
    let assets = ui_assets(&install, &language)?;
    let login = o
        .account
        .as_ref()
        .map(|(account, password)| auberean_interface::Login {
            server,
            account: account.clone(),
            password: password.clone(),
        });
    let mut retail = auberean_interface::Retail::new(assets, o.size.unwrap_or((1024, 768)), login)
        .with_forced_gore(o.forced_gore.0, o.forced_gore.1)
        .with_start_character(o.user.as_deref(), o.create.as_deref())
        .with_admin_client(o.admin_client);
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
        presentation: None,
        picture_in_window: ([0.0, 0.0, 1.0, 1.0], (1, 1)),
        settings_file,
        settings,
        install: o.dats.clone(),
        held: false,
        scripted: o.script.is_some(),
        seed: u64::from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos()),
        ),
        buf: vec![0; 65536],
        friends: friends::FriendsSocket::default(),
        sound: (!o.no_sound).then(dereth_audio_out::CpalOutput::default),
    };
    let mut shell = Shell::new(host, &world, data, interface);
    shell.apply_line(&o.line);
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
    let shown = shell.settings.presentation();
    let perf = match &o.perf {
        Some(f) => Some(perf::PerfLog::create(f).map_err(|e| format!("{}: {e}", f.display()))?),
        None => None,
    };
    let mut app = App {
        shell,
        perf,
        world: &world,
        present: None,
        shown,
        pointer_on_bar: false,
        pressed_on_bar: Vec::new(),
        script,
        marks: script::Marks::default(),
        shots,
        shot_next: None,
        shot_count: 0,
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
    let mut marks = script::Marks::default();
    loop {
        let now = shell.host.now();
        if let Some(s) = &mut script {
            let asked = s.step(now, shell.phase(), &shell.scene.status);
            for e in &asked.events {
                shell.event(e);
            }
            if let Some(m) = &asked.mark {
                marks.print(m, shell.client.as_ref());
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

/// How long a frame that took `took` rests in a window that can't be seen. Nothing there waits for
/// the display, so without this it would draw as fast as it can.
fn hidden_rest(took: std::time::Duration) -> std::time::Duration {
    std::time::Duration::from_micros(16_667).saturating_sub(took)
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

/// Where the next screenshot goes: the first number after `after` that no file in `dir` has, so a
/// new session leaves the old ones alone.
// D2-08355
fn next_free_shot(dir: &std::path::Path, after: u32) -> (u32, PathBuf) {
    let mut n = after + 1;
    loop {
        let path = dir.join(format!("shot-{n:03}.png"));
        if !path.exists() {
            return (n, path);
        }
        n += 1;
    }
}

#[cfg(test)]
mod shot_tests {
    //! Behaviour: none (the host's own file names: the numbering is retail's, the names are not)
    use super::next_free_shot;

    /// A screenshot takes the first number no file has, and one asked for before the last is
    /// written takes the number after it.
    #[test]
    fn a_screenshot_keeps_the_ones_before_it() {
        let dir = std::env::temp_dir().join(format!("auberean-shots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in [1, 2, 4] {
            std::fs::write(dir.join(format!("shot-{n:03}.png")), b"kept").unwrap();
        }
        assert_eq!(next_free_shot(&dir, 0), (3, dir.join("shot-003.png")));
        assert_eq!(next_free_shot(&dir, 3), (5, dir.join("shot-005.png")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod script_tests {
    //! Behaviour: none (tooling: a developer's script of input)
    use super::{from_the_devices, WindowEvent};

    #[test]
    fn a_script_stands_in_for_the_devices_and_not_for_the_window() {
        assert!(from_the_devices(&WindowEvent::Focused(false)));
        assert!(!from_the_devices(&WindowEvent::CloseRequested));
        assert!(!from_the_devices(&WindowEvent::RedrawRequested));
    }
}

#[cfg(test)]
mod hidden_tests {
    //! Behaviour: none (a window nobody can see, which retail's own loop isn't measured against)
    use super::hidden_rest;
    use std::time::Duration;

    #[test]
    fn a_hidden_window_rests_out_a_sixtieth_of_a_second_and_a_slow_frame_not_at_all() {
        assert_eq!(hidden_rest(Duration::ZERO), Duration::from_micros(16_667));
        assert_eq!(
            hidden_rest(Duration::from_millis(10)),
            Duration::from_micros(6_667)
        );
        assert_eq!(hidden_rest(Duration::from_millis(40)), Duration::ZERO);
    }
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
