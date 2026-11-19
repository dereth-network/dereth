//! A playtest inside the editor: our client run in the editor's own process, logged in to the
//! editor's Yalain as the playtest character, and drawn in the editor's world view on the editor's
//! device.
//!
//! **Drawing.** Each frame the game runs its frame, and its picture (the world, the game's
//! interface and the overlay over it, composed as our client composes its window's) is laid into
//! the world view: at least 800 by 600, scaled down into a smaller view ([`screen_for`]). The
//! panels are drawn over the rest of the window after it.
//!
//! **Input.** A click in the view gives the game the keyboard and the mouse; Shift+F1, or a click
//! on the panels, gives them back to the editor while the game plays on. While the game has them,
//! its pointer's picture shows over the view, and it may hold the pointer (mouse look); the
//! editor's arrow shows everywhere else.
//!
//! **Stopping** shuts the game down as our client does on exit: it says goodbye to the server and
//! keeps its settings in the playtest's own folder, and its world, interface and sound go with it.
//! Playing again starts afresh.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::overlay::WorldOverlay;
use auberean_dat::DatFile;
use auberean_gpu::Renderer;
use auberean_interface::data::{open_data, ui_assets, Install};
use auberean_present::Composer;
use auberean_scene::WorldData;
use auberean_shell::dev::DevLogin;
use auberean_shell::input::{Button, Event, Key};
use auberean_shell::interface::{CursorImage, Phase};
use auberean_shell::{Host, Shell};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::live::Launch;

/// The smallest screen the game lays its interface out on; a smaller view shows a screen this
/// size, or wider or taller to keep the view's shape, scaled down into it.
pub const SMALLEST_SCREEN: (u32, u32) = (800, 600);

/// The key that gives the keyboard and the mouse back to the editor, held with Shift.
pub const RELEASE_KEY: KeyCode = KeyCode::F1;

/// The game's settings file, in the playtest's settings folder.
const SETTINGS_FILE: &str = "Preferences.ini";

/// The size of the screen the game lays itself out on when it is drawn into a view of `view`'s
/// size: the view's own, or the smallest screen widened or heightened to the view's shape.
#[must_use]
pub fn screen_for(view: (u32, u32)) -> (u32, u32) {
    let (w, h) = (view.0.max(1), view.1.max(1));
    if w >= SMALLEST_SCREEN.0 && h >= SMALLEST_SCREEN.1 {
        return (w, h);
    }
    let k = (f64::from(SMALLEST_SCREEN.0) / f64::from(w))
        .max(f64::from(SMALLEST_SCREEN.1) / f64::from(h));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a screen's size
    let scaled = |n: u32, floor: u32| ((f64::from(n) * k).round() as u32).max(floor);
    (scaled(w, SMALLEST_SCREEN.0), scaled(h, SMALLEST_SCREEN.1))
}

/// Where in the window the game is drawn: its left, top, width and height, in pixels.
pub type ViewRect = [u32; 4];

/// The point of the game's `screen` that window pixel `at` shows, when the game is drawn into
/// `view`.
#[must_use]
pub fn screen_point(view: ViewRect, screen: (u32, u32), at: (f64, f64)) -> (f32, f32) {
    let [x, y, w, h] = view.map(f64::from);
    let sx = (at.0 - x) * f64::from(screen.0) / w.max(1.0);
    let sy = (at.1 - y) * f64::from(screen.1) / h.max(1.0);
    #[allow(clippy::cast_possible_truncation)] // a point of the screen
    (sx as f32, sy as f32)
}

/// Whether window pixel `at` is in `view`.
#[must_use]
pub fn inside(view: ViewRect, at: (f64, f64)) -> bool {
    let [x, y, w, h] = view.map(f64::from);
    at.0 >= x && at.1 >= y && at.0 < x + w && at.1 < y + h
}

/// The playtest's platform under the game: the editor's clock, a socket on this machine's
/// loopback address, the playtest's settings folder and the sound device. The window is the
/// editor's: the game's wish to hold the pointer is kept for the editor to carry out.
struct PlayHost {
    start: Instant,
    socket: Option<UdpSocket>,
    buf: Vec<u8>,
    /// The playtest's settings folder and the game's settings file in it.
    settings: PathBuf,
    /// The install's folder: the game's own files beside its data files.
    install: PathBuf,
    seed: u64,
    /// The game asks to hold the pointer (mouse look).
    held: bool,
    sound: Option<dereth_audio_out::CpalOutput>,
    /// The friends service's stream, at the login's own address as the editor's Yalain serves it.
    friends: auberean_client::friends::FriendsSocket,
}

impl Host for PlayHost {
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
        // The editor's own server is on this machine, reached from a loopback address, which no
        // firewall asks about.
        let local = if server.ip().is_loopback() {
            Ipv4Addr::LOCALHOST
        } else {
            Ipv4Addr::UNSPECIFIED
        };
        let s = UdpSocket::bind((local, 0)).map_err(|e| e.to_string())?;
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
        self.held = held;
    }

    fn set_title(&mut self, _title: &str) {}

    fn load_settings(&self) -> Option<String> {
        std::fs::read_to_string(self.settings.join(SETTINGS_FILE)).ok()
    }

    fn save_settings(&mut self, text: &str) {
        let _ = std::fs::create_dir_all(&self.settings);
        if let Err(e) = std::fs::write(self.settings.join(SETTINGS_FILE), text) {
            tracing::warn!("saving the playtest's settings: {e}");
        }
    }

    /// The player's file in the playtest's settings folder, else the install's own.
    fn load_file(&self, name: &str) -> Option<Vec<u8>> {
        let plain = std::path::Path::new(name).file_name()?;
        std::fs::read(self.settings.join(plain))
            .or_else(|_| std::fs::read(self.install.join(plain)))
            .ok()
    }

    fn save_file(&mut self, name: &str, bytes: &[u8]) -> bool {
        let Some(plain) = std::path::Path::new(name).file_name() else {
            return false;
        };
        let _ = std::fs::create_dir_all(&self.settings);
        std::fs::write(self.settings.join(plain), bytes).is_ok()
    }

    fn sound_output(&mut self) -> Option<&mut dyn dereth_audio::output::AudioOutput> {
        self.sound
            .as_mut()
            .map(|s| s as &mut dyn dereth_audio::output::AudioOutput)
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

/// The game over the playtest's platform, showing the world it owns.
type PlayShell<'w> = Shell<'w, PlayHost>;

self_cell::self_cell!(
    /// The world the game reads and the game itself, which reads it: kept together, the game
    /// going first.
    struct Game {
        owner: WorldData,
        #[not_covariant]
        dependent: PlayShell,
    }
);

/// The game's world and data, read from `launch`'s install through the world's overlay (checked
/// against every file it lies over, as our client checks it), and the game over them, logging in
/// by itself as `launch` says.
fn open(launch: &Launch, sound: bool) -> Result<Game, String> {
    let found =
        install_dats(&launch.dats).map_err(|e| format!("{}: {e}", launch.dats.display()))?;
    let overlay = match &launch.overlay {
        Some(dir) => {
            let o = WorldOverlay::open(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            for (_, path) in &found {
                let file = DatFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
                o.over(file).map_err(|e| e.to_string())?;
            }
            Some(o)
        }
        None => None,
    };
    let open = |name: &DatName| {
        let (_, path) = found.iter().find(|(n, _)| n == name)?;
        let file = DatFile::open(path).ok()?;
        match &overlay {
            Some(o) => o.over(file).ok(),
            None => Some(file),
        }
    };
    let install = Install {
        found: found.iter().map(|(n, _)| n.clone()).collect(),
        open: &open,
    };
    let preferences =
        std::fs::read_to_string(launch.settings.join(SETTINGS_FILE)).unwrap_or_default();
    let (world, data, language) = open_data(&install, None, &preferences)?;
    let assets = ui_assets(&install, &language)?;
    let records: auberean_horizon::art::Records = assets.clone();
    let auto = DevLogin::new(
        launch.server,
        launch.account.clone(),
        launch.password.clone(),
        Some(launch.character.clone()),
    );
    let start = auberean_horizon::Start {
        auto: Some(auto.clone()),
        ..auberean_horizon::Start::default()
    };
    let interface = auberean_interface::Retail::new(assets, SMALLEST_SCREEN, None).with_auto(auto);
    let host = PlayHost {
        start: Instant::now(),
        socket: None,
        buf: vec![0; 65536],
        settings: launch.settings.clone(),
        install: launch.dats.clone(),
        seed: u64::from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos()),
        ),
        held: false,
        sound: sound.then(dereth_audio_out::CpalOutput::default),
        friends: auberean_client::friends::FriendsSocket::default(),
    };
    Ok(Game::new(world, |world| {
        let maker = auberean_horizon::maker(interface, move || {
            auberean_horizon::Horizon::built_in(Some(std::sync::Arc::clone(&records)))
                .map(|h| h.with_start(start.clone()))
                .map(|h| Box::new(h) as Box<dyn auberean_shell::interface::Interface + '_>)
        });
        let mut shell = Shell::with_interfaces(host, world, data, maker);
        // The editor's world is Yalain's, which takes the client's word on its overlay.
        shell.world_takes_overlays = true;
        // Its friends service is at the login's address, with the ticket the login makes.
        shell.friends = auberean_shell::FriendsService::named(
            Some(&launch.server.ip().to_string()),
            Some(launch.server.port()),
            true,
            None,
        );
        shell.size = SMALLEST_SCREEN;
        shell
    }))
}

/// The game playing in the editor.
pub struct Embedded {
    game: Game,
    /// The character it plays.
    pub name: String,
    /// The game has the keyboard and the mouse.
    focused: bool,
    /// The screen the game lays itself out on, and where in the window it is drawn.
    screen: (u32, u32),
    view: Option<ViewRect>,
    /// Where the pointer is in the window; the mouse buttons pressed in the view and not yet let
    /// go, whose release the game is owed; whether Shift is held, and Command on a Mac.
    pointer: Option<(f64, f64)>,
    buttons: Vec<Button>,
    shift: bool,
    command: bool,
    composer: Option<Composer>,
    /// The game's pointer picture, as its last frame asked.
    cursor: Option<Arc<CursorImage>>,
    /// The game asked to close.
    quit: bool,
    /// The game has been in the world.
    entered: bool,
}

impl std::fmt::Debug for Embedded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Embedded")
            .field("name", &self.name)
            .field("focused", &self.focused)
            .field("screen", &self.screen)
            .finish_non_exhaustive()
    }
}

/// The game's button for a mouse button, if it has one.
fn button(b: winit::event::MouseButton) -> Option<Button> {
    auberean_client::keys::button(b)
}

impl Embedded {
    /// Start the game as `launch` says, with the sound device when `sound`: it logs in by itself
    /// and takes the keyboard and the mouse.
    ///
    /// # Errors
    /// The install's data files do not read, or the world's overlay does not lie over them.
    pub fn start(launch: &Launch, sound: bool) -> Result<Self, String> {
        let game = open(launch, sound)?;
        let mut e = Self {
            game,
            name: launch.character.clone(),
            focused: false,
            screen: SMALLEST_SCREEN,
            view: None,
            pointer: None,
            buttons: Vec::new(),
            shift: false,
            command: false,
            composer: None,
            cursor: None,
            quit: false,
            entered: false,
        };
        e.event(&Event::Resized {
            width: SMALLEST_SCREEN.0,
            height: SMALLEST_SCREEN.1,
        });
        e.focus(true);
        Ok(e)
    }

    /// One event straight to the game.
    pub fn event(&mut self, e: &Event) {
        self.game.with_dependent_mut(|_, shell| shell.event(e));
    }

    /// Give the game the keyboard and the mouse, or take them back for the editor: the game lets
    /// go of what it holds, keys and buttons, as when its window loses focus.
    pub fn focus(&mut self, focused: bool) {
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        if !focused {
            self.buttons.clear();
        }
        self.event(&Event::Focus(focused));
    }

    /// Whether the game has the keyboard and the mouse.
    #[must_use]
    pub const fn focused(&self) -> bool {
        self.focused
    }

    /// Where the game is drawn in the window from now on: its screen follows the view's size.
    pub fn set_view(&mut self, view: Option<ViewRect>) {
        self.view = view;
        let Some([_, _, w, h]) = view else {
            return;
        };
        let screen = screen_for((w, h));
        if screen != self.screen {
            self.screen = screen;
            self.event(&Event::Resized {
                width: screen.0,
                height: screen.1,
            });
        }
    }

    /// Where the game is drawn in the window, and the screen it lays itself out on.
    #[must_use]
    pub const fn view(&self) -> (Option<ViewRect>, (u32, u32)) {
        (self.view, self.screen)
    }

    /// Whether the pointer is over the game's view.
    #[must_use]
    pub fn pointer_over(&self) -> bool {
        matches!((self.view, self.pointer), (Some(v), Some(p)) if inside(v, p))
    }

    /// Whether the game holds the pointer (mouse look) while it has the mouse.
    #[must_use]
    pub fn holds_pointer(&self) -> bool {
        self.focused && self.game.with_dependent(|_, shell| shell.host.held)
    }

    /// The game's pointer picture, while the game has the mouse and the pointer is over its view
    /// (or held, or a button pressed there is still down); none for the editor's arrow.
    #[must_use]
    pub fn cursor(&self) -> Option<&CursorImage> {
        let shown = self.focused
            && (self.pointer_over() || self.holds_pointer() || !self.buttons.is_empty());
        self.cursor.as_deref().filter(|_| shown)
    }

    /// Whether the game asked to close.
    #[must_use]
    pub const fn quit(&self) -> bool {
        self.quit
    }

    /// Where the session is.
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.game.with_dependent(|_, shell| shell.phase())
    }

    /// The character's cell and its place in that cell's landblock, once it is in the world.
    #[must_use]
    pub fn place(&self) -> Option<(u32, [f32; 3])> {
        self.game.with_dependent(|_, shell| {
            // The world's frame says where the body stands from the first frame it framed.
            let s = &shell.scene.status;
            (shell.phase() == Phase::World && s.cell != 0)
                .then_some((s.cell, [s.origin.x, s.origin.y, s.origin.z]))
        })
    }

    /// One of the window's events, given whether the pointer is over the editor's panels or a
    /// window of its over the view (`covered`): answers whether the game took it, in which case
    /// the editor does not. Keys go to the game while it has the keyboard (Shift+F1 gives them
    /// back); a press in the view gives it the mouse, a press elsewhere takes it back; the pointer
    /// goes to the game in its screen's points.
    pub fn window_event(&mut self, e: &WindowEvent, covered: bool) -> bool {
        match e {
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                let PhysicalKey::Code(code) = event.physical_key else {
                    return self.focused;
                };
                if matches!(code, KeyCode::ShiftLeft | KeyCode::ShiftRight) {
                    self.shift = down;
                }
                if !self.focused {
                    return false;
                }
                if down && self.shift && code == RELEASE_KEY {
                    self.focus(false);
                    return true;
                }
                if let Some(key) = auberean_client::keys::key(code) {
                    self.event(&Event::Key {
                        key,
                        down,
                        repeat: event.repeat,
                    });
                }
                if down {
                    let pressed = auberean_client::keys::Pressed::of(event);
                    if let Some(t) = auberean_client::keys::typed(&pressed, self.command) {
                        self.event(&Event::Text(t));
                    }
                }
                true
            }
            WindowEvent::ModifiersChanged(m) => {
                self.command = cfg!(target_os = "macos") && m.state().super_key();
                false
            }
            WindowEvent::CursorMoved { position, .. } => {
                let at = (position.x, position.y);
                self.pointer = Some(at);
                let ours = self.focused
                    && (self.pointer_over() || self.holds_pointer() || !self.buttons.is_empty());
                if let (true, Some(view)) = (ours, self.view) {
                    let (x, y) = screen_point(view, self.screen, at);
                    self.event(&Event::Pointer { x, y });
                }
                self.holds_pointer()
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = None;
                false
            }
            WindowEvent::MouseInput {
                state, button: b, ..
            } => {
                let Some(b) = button(*b) else {
                    return false;
                };
                if *state == ElementState::Pressed {
                    if (self.pointer_over() && !covered) || self.holds_pointer() {
                        self.focus(true);
                        self.buttons.push(b);
                        self.event(&Event::Button {
                            button: b,
                            down: true,
                        });
                        true
                    } else {
                        self.focus(false);
                        false
                    }
                } else if let Some(at) = self.buttons.iter().position(|x| *x == b) {
                    self.buttons.remove(at);
                    self.event(&Event::Button {
                        button: b,
                        down: false,
                    });
                    true
                } else {
                    false
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if !(self.focused && self.pointer_over() && !covered) {
                    return false;
                }
                #[allow(clippy::cast_possible_truncation)]
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 120.0) as f32,
                };
                self.event(&Event::Wheel { notches });
                true
            }
            WindowEvent::Focused(false) => {
                self.focus(false);
                false
            }
            _ => false,
        }
    }

    /// The mouse moved, in its own counts: the game's while it has the mouse.
    pub fn motion(&mut self, dx: f32, dy: f32) {
        if self.focused {
            self.event(&Event::Motion { dx, dy });
        }
    }

    /// A key of the game's by its name (`W`, `Space`, `Numpad5`) pressed or let go, as a script
    /// presses it: straight to the game, whatever has the keyboard.
    ///
    /// # Errors
    /// A name that is no key of the game's.
    pub fn key(&mut self, name: &str, down: bool) -> Result<(), String> {
        let key: Key = auberean_client::keys::named(name).ok_or(format!("{name}: no such key"))?;
        self.event(&Event::Key {
            key,
            down,
            repeat: false,
        });
        Ok(())
    }

    /// The game's frame with no picture made: its session, its world and its interface move on.
    pub fn tick(&mut self) {
        let (quit, cursor) = self.game.with_dependent_mut(|_, shell| {
            let out = shell.frame();
            (out.quit, out.overlay.cursor.clone())
        });
        self.framed(quit, cursor);
    }

    /// The game's frame, its picture composed on `renderer`'s device at the game's screen size.
    pub fn draw(&mut self, renderer: &mut Renderer) -> wgpu::Texture {
        let screen = self.screen;
        let composer = self.composer.get_or_insert_with(|| Composer::new(renderer));
        let (picture, quit, cursor) = self.game.with_dependent_mut(|world, shell| {
            let out = shell.frame();
            let mut encoder =
                renderer
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("playtest"),
                    });
            let mut load = |id: u32| auberean_draw::picture::load(&world.records, id);
            let picture = composer.compose(
                renderer,
                screen,
                out.world,
                out.portal.map(|f| (f, out.portal_alpha)),
                out.veil,
                out.overlay,
                &out.views,
                &mut load,
                &mut encoder,
            );
            renderer.queue().submit([encoder.finish()]);
            (picture, out.quit, out.overlay.cursor.clone())
        });
        self.framed(quit, cursor);
        picture
    }

    /// After a frame: whether the game asked to close, its pointer picture, and its entering the
    /// world written to the log.
    fn framed(&mut self, quit: bool, cursor: Option<Arc<CursorImage>>) {
        self.quit |= quit;
        self.cursor = cursor;
        if !self.entered {
            if let Some((cell, at)) = self.place() {
                self.entered = true;
                tracing::info!(
                    character = %self.name,
                    "playing in the editor: entered the world at {cell:08X} {at:?}"
                );
            }
        }
    }

    /// Stop the game as our client stops on exit: a goodbye to the server and the settings kept.
    pub fn stop(mut self) {
        if let Some((cell, at)) = self.place() {
            tracing::info!(
                character = %self.name,
                "playing in the editor: stopped at {cell:08X} {at:?}"
            );
        }
        self.focus(false);
        self.event(&Event::Close);
        self.tick();
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest inside the editor's world view)
    use super::*;

    #[test]
    fn a_view_at_least_the_smallest_screen_is_the_games_screen_and_a_smaller_one_keeps_its_shape() {
        assert_eq!(screen_for((1200, 800)), (1200, 800));
        assert_eq!(screen_for((800, 600)), (800, 600));
        // Half the smallest screen: the smallest screen, scaled down into it.
        assert_eq!(screen_for((400, 300)), (800, 600));
        // Narrow and tall: as wide as the smallest screen, and taller in the view's shape.
        assert_eq!(screen_for((400, 600)), (800, 1200));
        // Wide and short: as tall as the smallest screen, and wider.
        assert_eq!(screen_for((1600, 300)), (3200, 600));
        let (w, h) = screen_for((700, 500));
        assert!(w >= 800 && h >= 600);
        assert!((f64::from(w) / f64::from(h) - 700.0 / 500.0).abs() < 0.01);
    }

    #[test]
    fn a_window_pixel_in_the_view_is_the_screens_point_it_shows() {
        let view = [300, 100, 400, 300];
        let screen = screen_for((400, 300));
        assert_eq!(screen_point(view, screen, (300.0, 100.0)), (0.0, 0.0));
        assert_eq!(screen_point(view, screen, (500.0, 250.0)), (400.0, 300.0));
        assert_eq!(screen_point(view, screen, (700.0, 400.0)), (800.0, 600.0));
        assert!(inside(view, (300.0, 100.0)));
        assert!(inside(view, (699.0, 399.0)));
        assert!(!inside(view, (700.0, 250.0)));
        assert!(!inside(view, (299.0, 250.0)));
    }
}
