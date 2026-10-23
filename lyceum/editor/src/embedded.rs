//! A playtest inside the editor: a connected client drawn in the world view, sharing the editor's
//! device and window, with the editor's panels around it.
//!
//! **Drawing.** Each frame the editor lays out its panels, then the game runs its frame on the
//! editor's device with the world view's rectangle set as its draw region: the game draws its
//! world and its interface there as on a screen of its own, at least 800 by 600 (scaled down into
//! a smaller view), and the panels are drawn over the rest of the window after it.
//!
//! **Input.** A click in the view gives the game the keyboard and the mouse; Shift+F1, or a click
//! on the panels, gives them back to the editor while the game plays on. The pointer's image is
//! the game's while it has them and is over the view, and the editor's arrow everywhere else.
//!
//! **Stopping** shuts the game down as the client does on exit: it logs off and disconnects, and
//! its world, interface, sound, cursors and pointer hold go with it. Playing again starts afresh.
use std::cell::RefCell;

use dereth_client_runtime::app::{Platform, StartupError};
use dereth_client_runtime::config::Config;
use dereth_client_runtime::platform::window::{NullWindow, WindowHost};
use dereth_client_runtime::present::NullPresentation;
use dereth_client_shell::cursor::{CursorImages, CursorInstall, CursorKey};
use dereth_client_shell::gpu::Renderer;
use dereth_client_shell::platform::host::Host;
use dereth_client_shell::platform::window::lifecycle;
use dereth_client_shell::pointer::HostPointer;
use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;
use dereth_primitives::{Inset, Viewport};

use super::EditorApp;

/// The desktop host the game would have on its own.
type Desktop = dereth_desktop::Desktop<crate::Lyceum>;

/// The game, on the desktop host with its pointer and cursors shared with the editor.
type Game = dereth_client_shell::app::App<PlayHost>;

/// The smallest screen the game lays its interface out on; a smaller view shows a screen this
/// size, or wider or taller to keep the view's shape, scaled down into it.
pub const SMALLEST_SCREEN: (u32, u32) = (800, 600);

/// The key that gives the keyboard and the mouse back to the editor, held with Shift: F1.
const RELEASE_KEY: usize = 0x70;

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

/// What the game and the editor share of the window: the game's cursors and its hold on the
/// pointer, which reach the window only while the game has the pointer.
#[derive(Default)]
struct Shared {
    /// The desktop's cursor images, which the game's cursors are built and put on the window by.
    cursors: Option<Box<dyn CursorImages>>,
    /// The cursor the game last asked for, put back on the window when the game has the pointer.
    asked: Option<CursorKey>,
    /// Whether the game has the pointer: its cursor shows, and it may hold the pointer.
    game: bool,
    /// The desktop's hold on the pointer, and whether the game holds it now.
    pointer: Option<Box<dyn HostPointer>>,
    holding: bool,
    /// Where the game is drawn in the window, for the points its pointer hold names.
    region: Option<Inset>,
    /// The cursor on the window now: the editor's arrow (`None`) or the game's.
    shown: Option<CursorKey>,
}

std::thread_local! {
    /// The playtest's share of the window, on the thread the window and the game live on.
    static SHARED: RefCell<Shared> = RefCell::new(Shared::default());
}

/// The game's cursor images: built by the desktop's, and put on the window only while the game
/// has the pointer.
struct PlayCursors;

impl CursorImages for PlayCursors {
    fn build(&mut self, key: CursorKey, bits: &dereth_render::cursor::IconBits) -> bool {
        SHARED.with(|s| {
            s.borrow_mut()
                .cursors
                .as_mut()
                .is_some_and(|c| c.build(key, bits))
        })
    }

    fn install(&mut self, key: CursorKey) -> Option<CursorInstall> {
        SHARED.with(|s| {
            let mut s = s.borrow_mut();
            s.asked = Some(key);
            if !s.game {
                return Some(CursorInstall {
                    window: true,
                    icon: true,
                    took: false,
                });
            }
            s.shown = Some(key);
            s.cursors.as_mut()?.install(key)
        })
    }
}

/// The game's hold on the pointer for a camera drag: the desktop's, at the window's points for
/// the game's, and only while the game has the pointer.
struct PlayPointer;

impl HostPointer for PlayPointer {
    fn capture(&mut self, at: (f64, f64)) -> bool {
        SHARED.with(|s| {
            let mut s = s.borrow_mut();
            if !s.game {
                return false;
            }
            let at = s.region.map_or(at, |r| r.point_to(at.0, at.1));
            let held = s.pointer.as_mut().is_some_and(|p| p.capture(at));
            s.holding |= held;
            held
        })
    }

    fn release(&mut self, at: (f64, f64)) {
        SHARED.with(|s| {
            let mut s = s.borrow_mut();
            let at = s.region.map_or(at, |r| r.point_to(at.0, at.1));
            if let Some(p) = s.pointer.as_mut() {
                p.release(at);
            }
            s.holding = false;
        });
    }
}

/// The desktop host, but for the window's cursor and pointer hold, which the editor shares.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlayHost;

impl Host for PlayHost {
    const BUILD_ID: &'static str = <Desktop as Host>::BUILD_ID;
    const BUILD_COMMIT: &'static str = <Desktop as Host>::BUILD_COMMIT;

    type Clipboard = <Desktop as Host>::Clipboard;

    fn open_platform(
        cfg: &Config,
        events: dereth_client_shell::platform::window::WindowEvents,
    ) -> Result<Platform, StartupError> {
        Desktop::open_platform(cfg, events)
    }

    fn local_utc_offset_secs(unix_secs: i64) -> i32 {
        Desktop::local_utc_offset_secs(unix_secs)
    }

    fn launch_uri(url: &str) -> i32 {
        Desktop::launch_uri(url)
    }

    fn install_default_output() {
        Desktop::install_default_output();
    }

    fn clipboard() -> Self::Clipboard {
        Desktop::clipboard()
    }

    fn cursor_images(window: Option<isize>) -> Box<dyn CursorImages> {
        let images = Desktop::cursor_images(window);
        SHARED.with(|s| s.borrow_mut().cursors = Some(images));
        Box::new(PlayCursors)
    }

    fn classic_fonts() -> Option<std::sync::Arc<dyn dereth_client_shell::platform::host::FontSource>>
    {
        Desktop::classic_fonts()
    }

    fn caret_blink_secs() -> f64 {
        Desktop::caret_blink_secs()
    }

    fn classic_help_book() -> Result<Option<Vec<u8>>, String> {
        Desktop::classic_help_book()
    }

    fn classic_welcome() -> String {
        Desktop::classic_welcome()
    }

    fn pointer(window: Option<isize>) -> Box<dyn HostPointer> {
        let pointer = Desktop::pointer(window);
        SHARED.with(|s| s.borrow_mut().pointer = Some(pointer));
        Box::new(PlayPointer)
    }

    fn gamepad() -> Box<dyn dereth_client_shell::gamepad::HostGamepad> {
        Desktop::gamepad()
    }

    fn movie_bytes(path: &std::path::Path) -> Option<Vec<u8>> {
        Desktop::movie_bytes(path)
    }
}

/// A playtest running in the editor.
pub(super) struct Embedded {
    pub game: Game,
    scene: Option<dereth_scene::world_scene::SceneDraw>,
    /// Whether the game has the keyboard and the mouse.
    pub focused: bool,
    pub name: String,
    interface: super::PlayInterface,
    pub destination: Option<(u32, [f32; 3])>,
    /// In the world (its view and interface shown), and at the editor's camera (entered).
    in_world: bool,
    entered: bool,
    revision: u64,
    /// Where the game is drawn in the window, and the screen it lays itself out on, as last drawn.
    region: Option<Inset>,
    /// The mouse buttons pressed in the view and not yet let go, whose release the game is owed;
    /// whether Shift is held; where the pointer is in the window.
    buttons: Vec<MouseButton>,
    shift: bool,
    pointer: Option<(f64, f64)>,
    /// The editor's controls in the view's corner, in window pixels: the editor's, not the game's.
    corner: Option<Viewport>,
}

impl std::fmt::Debug for Embedded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Embedded")
            .field("focused", &self.focused)
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}

impl Embedded {
    /// Whether the pointer is over the game's view (and not the editor's controls in its corner).
    fn pointer_over(&self) -> bool {
        self.pointer.is_some_and(|(x, y)| {
            let in_corner = self.corner.is_some_and(|c| {
                x >= f64::from(c.x)
                    && y >= f64::from(c.y)
                    && x < f64::from(c.x + c.width)
                    && y < f64::from(c.y + c.height)
            });
            !in_corner && self.region.is_some_and(|r| r.point_inside(x, y).is_some())
        })
    }
}

/// What a playtest leaves running, counted: each is zero once it has stopped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayCensus {
    /// The game itself.
    pub sessions: usize,
    /// Its connection to the editor's world.
    pub connections: usize,
    /// Its sound.
    pub sound: usize,
    /// Its cursor on the window, and its hold on the pointer.
    pub cursors: usize,
    pub pointer_holds: usize,
    /// Its draw region on the editor's device.
    pub regions: usize,
}

fn select_interface(interface: super::PlayInterface) {
    use dereth_client_contract::options::{interface as choice, store};
    let value = match interface {
        super::PlayInterface::Horizon => choice::Interface::Horizon,
        super::PlayInterface::Modern => choice::Interface::Modern,
        super::PlayInterface::Classic => choice::Interface::Classic,
    };
    let _ = store::set_value(
        choice::INTERFACE,
        dereth_client_contract::PrefValue::Int(value.value()),
    );
}

/// The editor's renderer under its presentation.
fn renderer(present: &mut dyn dereth_client_shell::present::ClientPresentation) -> &mut Renderer {
    present
        .as_any_mut()
        .downcast_mut::<Renderer>()
        .expect("editor renderer")
}

impl EditorApp {
    /// Open a client session inside the editor's world view using the normal client arguments.
    ///
    /// # Errors
    /// Invalid arguments, unavailable data, or failed client startup.
    pub fn start_embedded(
        &mut self,
        args: &[String],
        settings: &std::path::Path,
    ) -> Result<(), String> {
        self.stop_embedded();
        std::fs::create_dir_all(settings).map_err(|e| e.to_string())?;
        let interface = super::play::read_play_options(settings).interface;
        let mut cfg =
            Config::from_args_and_prefs_named_at(args, &settings.join("UserPreferences.ini"), true)
                .map_err(|e| e.to_string())?;
        let view = self
            .play_view()
            .map_or_else(|| self.core.present.size(), |v| (v.width, v.height));
        let (w, h) = screen_for(view);
        cfg.hosted_entry = true;
        cfg.width = w;
        cfg.height = h;
        cfg.display.full_screen = false;
        let mut platform = Platform::headless(w, h);
        platform.window = Box::new(HostedWindow {
            window: NullWindow::new(w, h),
            handle: self.core.window.raw_handle(),
        });
        platform.clock = Box::new(dereth_client_runtime::platform::clock::SystemClock::new());
        SHARED.with(|s| *s.borrow_mut() = Shared::default());
        let mut game = Game::with_platform(cfg, Box::new(NullPresentation::new(w, h)), platform)
            .map_err(|e| e.to_string())?;
        if let Err(e) = game.start_shell() {
            game.shutdown();
            SHARED.with(|s| *s.borrow_mut() = Shared::default());
            return Err(e.to_string());
        }
        // The hidden entry screens create characters on every supported game interface.
        select_interface(super::PlayInterface::Horizon);
        let scene = game.cfg.scene_config();
        game.defer_static_scene(scene);
        game.queue_window_event(HostEvent::Focused(true));
        self.playing = Some(Embedded {
            game,
            scene: None,
            focused: true,
            name: self.characters.name.clone(),
            interface,
            destination: None,
            in_world: false,
            entered: false,
            revision: self.live.as_ref().map_or(0, |l| l.revision),
            region: None,
            buttons: Vec::new(),
            shift: false,
            pointer: None,
            corner: None,
        });
        self.held = super::Held::default();
        self.look = None;
        Ok(())
    }

    pub(super) fn play_resident_blocks(&self) -> std::collections::BTreeSet<u16> {
        let mut blocks = std::collections::BTreeSet::new();
        if let Some(world) = self.playing.as_ref().and_then(|p| p.game.world.as_ref()) {
            let window = &world.streamer.window;
            for x in 0..window.mid_width() {
                for y in 0..window.mid_width() {
                    if let Some(slot) = window.slot(x, y) {
                        if let Ok(block) = u16::try_from((slot.block_x << 8) | slot.block_y) {
                            blocks.insert(block);
                        }
                    }
                }
            }
            if let Some(body) = world.character.as_ref() {
                blocks.insert((body.position().cell.0 >> 16) as u16);
            }
        }
        blocks
    }

    /// Adopt a published overlay without replacing the connected client or its character.
    ///
    /// # Errors
    /// An older revision, or an overlay that will not open over the playtest's data.
    pub fn adopt_play_revision(&mut self, revision: &crate::live::Revision) -> Result<(), String> {
        let Some(play) = self.playing.as_mut() else {
            return Ok(());
        };
        if revision.number <= play.revision {
            return Err("The playtest already has this revision or a newer one.".into());
        }
        let overlay =
            dereth_dat::overlay::OverlayDir::new(&revision.overlay).map_err(|e| e.to_string())?;
        if overlay.world_key().as_deref() != Some(self.session.world().key.as_str()) {
            return Err("The live overlay is missing or belongs to another world.".into());
        }
        let previous = play
            .game
            .cfg
            .overlay_dat_dir
            .replace(revision.overlay.clone());
        let fresh = match play.game.reopen_store() {
            Ok(store) => std::sync::Arc::new(store),
            Err(error) => {
                play.game.cfg.overlay_dat_dir = previous;
                return Err(error.cause);
            }
        };
        if !fresh.has_overlay() {
            play.game.cfg.overlay_dat_dir = previous;
            return Err("The live overlay could not be opened over the playtest data.".into());
        }
        play.game.adopt_store(std::sync::Arc::clone(&fresh));
        let ids: Vec<_> = revision
            .blocks
            .iter()
            .map(|lb| dereth_primitives::DataId((u32::from(*lb) << 16) | 0xFFFF))
            .collect();
        if let Some(body) = play.game.world.as_ref().and_then(|w| w.character.as_ref()) {
            body.land().resupply(fresh, &ids);
        }
        let renderer = renderer(self.core.present.as_mut());
        renderer.swap_world(&mut play.scene);
        if let (Some(draw), Some(world)) = (renderer.world_mut(), play.game.world.as_mut()) {
            let blocks: Vec<_> = revision
                .blocks
                .iter()
                .map(|b| (i32::from(b >> 8), i32::from(b & 255)))
                .collect();
            draw.reread_blocks(world, &blocks);
        }
        renderer.swap_world(&mut play.scene);
        play.revision = revision.number;
        Ok(())
    }

    #[cfg(feature = "live-server-tests")]
    pub fn play_store(&self) -> Option<&dereth_dat::RetailDatStore> {
        self.playing.as_ref().map(|p| p.game.store.as_ref())
    }

    /// Where the playtest character stands: its cell and its frame (that cell's landblock
    /// meters), once it is in the world.
    #[must_use]
    pub fn play_body(&self) -> Option<dereth_primitives::Position> {
        let world = self.playing.as_ref()?.game.world.as_ref()?;
        Some(world.character.as_ref()?.position())
    }

    /// The physics state the playtest's world holds for object `guid` (a door's opens it to
    /// pass through, `0x4`), once it knows the object.
    #[must_use]
    pub fn play_object_state(&self, guid: u32) -> Option<u32> {
        let play = self.playing.as_ref()?;
        play.game
            .objects
            .physics_state(dereth_primitives::ObjectId(guid))
    }

    /// The connected player's identity and the last live overlay it adopted.
    pub fn play_identity(&self) -> Option<(u32, u64)> {
        let play = self.playing.as_ref()?;
        Some((play.game.world.as_ref()?.player_object?.0, play.revision))
    }

    /// Give the keyboard and the mouse back to the editor while the playtest plays on.
    pub fn return_to_editor(&mut self) {
        self.focus_game(false);
    }

    /// Whether a playtest is running in the world view (or starting there).
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    /// Whether the game has the keyboard and the mouse.
    #[must_use]
    pub fn game_has_focus(&self) -> bool {
        self.playing.as_ref().is_some_and(|p| p.focused)
    }

    /// Whether the server has delivered the playtest character at the editor's camera.
    #[must_use]
    pub fn play_ready(&self) -> bool {
        self.playing.as_ref().is_some_and(|p| p.entered)
    }

    /// Whether the playtest is in the world, its view and interface shown.
    #[must_use]
    pub fn play_in_world(&self) -> bool {
        self.playing.as_ref().is_some_and(|p| p.in_world)
    }

    /// Whether the playtest's character is between places, in portal space.
    #[must_use]
    pub fn play_teleporting(&self) -> bool {
        self.playing.as_ref().is_some_and(|p| {
            let t = p.game.teleport();
            t.world_hidden() || t.teleport_in_progress() || t.anim.teleport_in_progress
        })
    }

    /// Where the game is drawn in the window and the screen it lays itself out on, as last
    /// drawn.
    #[must_use]
    pub fn play_region(&self) -> Option<Inset> {
        self.playing.as_ref().and_then(|p| p.region)
    }

    /// What a playtest has running now.
    #[must_use]
    pub fn play_census(&self) -> PlayCensus {
        let (cursors, pointer_holds) = SHARED.with(|s| {
            let s = s.borrow();
            (usize::from(s.shown.is_some()), usize::from(s.holding))
        });
        let regions = usize::from(self.core.present.inset().is_some());
        let Some(play) = self.playing.as_ref() else {
            return PlayCensus {
                cursors,
                pointer_holds,
                regions,
                ..PlayCensus::default()
            };
        };
        PlayCensus {
            sessions: 1,
            connections: usize::from(play.game.link.is_some()),
            sound: usize::from(play.game.audio.is_some()),
            cursors,
            pointer_holds,
            regions,
        }
    }

    /// The world view's rectangle in window pixels, as the panels last laid it out.
    pub(super) fn play_view(&self) -> Option<Viewport> {
        let r = self.world_rect?;
        let scale = self.gui.scale();
        let (w, h) = self.core.present.size();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a window's pixels
        let px = |v: f32, most: u32| ((v * scale).round().max(0.0) as u32).min(most);
        let (x0, y0) = (px(r.min.x, w), px(r.min.y, h));
        let (x1, y1) = (px(r.max.x, w), px(r.max.y, h));
        (x1 > x0 && y1 > y0).then_some(Viewport {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }

    /// Give the game the keyboard and the mouse, or take them back for the editor.
    pub(super) fn focus_game(&mut self, focused: bool) {
        let Some(play) = self.playing.as_mut() else {
            return;
        };
        if play.focused == focused {
            return;
        }
        play.focused = focused;
        if !focused {
            // The game lets go of what it holds, keys and buttons, as when its window loses focus.
            play.buttons.clear();
        }
        play.game.queue_window_event(HostEvent::Focused(focused));
        let over = play.pointer_over();
        self.give_pointer(focused && over);
    }

    /// Show the game's cursor on the window and let it hold the pointer, or show the editor's
    /// arrow and let a held pointer go.
    fn give_pointer(&mut self, game: bool) {
        let window = self
            .core
            .window
            .raw_handle()
            .and_then(dereth_desktop::platform::window::cursor_window)
            .and_then(|w| w.upgrade());
        SHARED.with(|s| {
            let mut s = s.borrow_mut();
            if s.game == game {
                return;
            }
            s.game = game;
            if game {
                if let Some(key) = s.asked {
                    s.shown = Some(key);
                    if let Some(c) = s.cursors.as_mut() {
                        c.install(key);
                    }
                }
            } else {
                s.shown = None;
                s.holding = false;
                if let Some(w) = &window {
                    w.let_pointer_go(None);
                    w.set_default_cursor();
                }
            }
        });
    }

    /// One frame with a playtest running: the window's events shared between the editor and the
    /// game, the panels laid out, the game's frame drawn in the world view and the panels over it.
    /// False when the window closed.
    pub(super) fn play_frame(&mut self) -> bool {
        let (w, h) = self.core.present.size();
        if self.core.window.pump_events((w, h), false).exited {
            return false;
        }
        let mut events: Vec<HostEvent> = std::mem::take(&mut *self.shell.events.borrow_mut());
        events.extend(std::mem::take(&mut self.shell.inbox));
        if let Some(batch) = self.later.pop_front() {
            events.extend(batch);
        }
        let time = self
            .playing
            .as_ref()
            .map_or(self.core.timer.local_time, |p| p.game.timer.local_time);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // milliseconds
        let time_ms = (time * 1000.0) as u32;
        let mut device = Vec::new();
        for e in events {
            use dereth_client_runtime::platform::window::HostEvent as Lifecycle;
            match lifecycle(&e) {
                Some(Lifecycle::CloseRequested) => return false,
                Some(l) => {
                    if matches!(l, Lifecycle::Focused(false)) {
                        self.focus_game(false);
                    }
                    self.core.handle_window_event(&mut self.shell, &l, time_ms);
                }
                None => device.push(e),
            }
        }
        let editor = self.route_play_input(device);
        let items = self.play_panels(editor, time);
        if self.playing.is_none() {
            // The panels stopped it.
            return true;
        }
        self.draw_play(items)
    }

    /// Share the window's device events between the game and the editor: the game's go to it,
    /// in its own screen's points; the editor's are answered.
    fn route_play_input(&mut self, device: Vec<HostEvent>) -> Vec<HostEvent> {
        let holding = SHARED.with(|s| s.borrow().holding);
        let Some(play) = self.playing.as_mut() else {
            return device;
        };
        let mut editor = Vec::new();
        let mut focused = play.focused;
        for e in device {
            match &e {
                HostEvent::KeyboardInput { key, pressed, .. } => {
                    if matches!(key.virtual_key, 0x10 | 0xA0 | 0xA1) {
                        play.shift = *pressed;
                    }
                    if focused && *pressed && play.shift && key.virtual_key == RELEASE_KEY {
                        focused = false;
                    } else if focused {
                        play.game.queue_window_event(e);
                    } else {
                        editor.push(e);
                    }
                }
                HostEvent::CursorMoved { x, y } => {
                    play.pointer = Some((*x, *y));
                    let over = play.pointer_over();
                    if focused && (over || holding || !play.buttons.is_empty()) {
                        if let Some(r) = play.region {
                            let (x, y) = r.point_from(*x, *y);
                            play.game
                                .queue_window_event(HostEvent::CursorMoved { x, y });
                        }
                    }
                    if !holding {
                        editor.push(e);
                    }
                }
                HostEvent::PointerMotion { .. } => {
                    if focused {
                        play.game.queue_window_event(e);
                    }
                }
                HostEvent::MouseInput { button, pressed } => {
                    if *pressed {
                        if play.pointer_over() || holding {
                            focused = true;
                            play.buttons.push(*button);
                            play.game.queue_window_event(e);
                        } else {
                            focused = false;
                            editor.push(e);
                        }
                    } else if let Some(at) = play.buttons.iter().position(|b| b == button) {
                        play.buttons.remove(at);
                        play.game.queue_window_event(e);
                    } else {
                        editor.push(e);
                    }
                }
                HostEvent::MouseWheel { .. } => {
                    if focused && play.pointer_over() {
                        play.game.queue_window_event(e);
                    } else {
                        editor.push(e);
                    }
                }
                HostEvent::CursorLeft => {
                    if focused {
                        play.game.queue_window_event(e.clone());
                    }
                    play.pointer = None;
                    editor.push(e);
                }
                HostEvent::ModifiersChanged { .. } => {
                    play.game.queue_window_event(e.clone());
                    editor.push(e);
                }
                _ => editor.push(e),
            }
        }
        self.focus_game(focused);
        // The game's cursor while it has the mouse and the pointer is over its view.
        if let Some(play) = self.playing.as_ref() {
            let game = play.focused && (play.pointer_over() || holding || !play.buttons.is_empty());
            self.give_pointer(game);
        }
        editor
    }

    /// The game's frame on the editor's device, drawn into the world view, with the panels'
    /// `items` over the window after it. False when the window closed.
    fn draw_play(&mut self, items: Vec<dereth_client_contract::overlay::OverlayItem>) -> bool {
        let Some(view) = self.play_view() else {
            // No world view laid out yet: the game waits for one.
            return true;
        };
        let Some(mut play) = self.playing.take() else {
            return true;
        };
        let wanted = screen_for((view.width, view.height));
        let in_world = play.game.objects.world.player_initialized && play.game.world.is_some();
        if in_world && !play.in_world {
            // In the world, the game's view and interface are shown as the player chose them.
            play.in_world = true;
            select_interface(play.interface);
        }
        let at_destination = play.destination.is_none_or(|(cell, at)| {
            play.game
                .world
                .as_ref()
                .and_then(|w| w.character.as_ref())
                .is_some_and(|body| {
                    let position = body.position();
                    let p = position.frame.origin;
                    same_destination_cell(position.cell.0, cell)
                        && (p.x - at[0]).abs() < 2.0
                        && (p.y - at[1]).abs() < 2.0
                })
        });
        let ready = self.character_work.is_none()
            && (play.entered
                || (in_world
                    && !play.game.teleport().world_hidden()
                    && !play.game.teleport().teleport_in_progress()
                    && !play.game.teleport().anim.teleport_in_progress
                    && at_destination));
        if ready && !play.entered {
            tracing::info!("playtest entered at the editor camera");
            play.entered = true;
        }
        for (texture, data) in std::mem::take(&mut self.gui.uploads) {
            if let Err(e) = self.core.present.overlay_upload(texture, &data) {
                self.panels.message = e.to_string();
            }
        }
        play.game
            .set_host_overlay(items, !play.in_world || self.character_work.is_some());
        let screen = play.game.present.size();
        renderer(self.core.present.as_mut()).swap_world(&mut play.scene);
        std::mem::swap(&mut self.core.present, &mut play.game.present);
        let region = Inset {
            rect: view,
            extent: screen,
        };
        play.game.present.set_inset(Some(region));
        SHARED.with(|s| s.borrow_mut().region = Some(region));
        if screen != wanted {
            play.game.force_display_resolution(true, wanted.0, wanted.1);
        }
        let running = play.game.frame_with_drawing(true);
        if let Some(path) = play.game.cfg.capture.take() {
            if play.entered {
                if let Err(e) = play.game.present.capture_png(&path) {
                    self.panels.message = e.to_string();
                }
            } else {
                play.game.cfg.capture = Some(path);
            }
        }
        let drawn = play.game.present.inset().unwrap_or(region);
        play.game.present.set_inset(None);
        std::mem::swap(&mut self.core.present, &mut play.game.present);
        renderer(self.core.present.as_mut()).swap_world(&mut play.scene);
        if play.game.present.size() != drawn.extent {
            let _ = play.game.present.resize(drawn.extent.0, drawn.extent.1);
        }
        play.region = Some(drawn);
        SHARED.with(|s| s.borrow_mut().region = Some(drawn));
        self.playing = Some(play);
        if !running {
            if !self.play_ready() && self.character_work.is_none() {
                self.panels.message = self.say("play.entry_failed");
            }
            self.stop_embedded();
        }
        true
    }

    /// The panels laid out over a playtest, from the editor's share of this frame's `events`;
    /// their actions answered, and the editor's keys (undo, redo, save, the panels' scale) while
    /// the game does not have the keyboard. Answers the panels' drawing.
    fn play_panels(
        &mut self,
        events: Vec<HostEvent>,
        time: f64,
    ) -> Vec<dereth_client_contract::overlay::OverlayItem> {
        self.text.refresh();
        if let Err(e) = self.take_catalog(false) {
            self.panels.message = e;
        }
        let (w, h) = self.core.present.size();
        let raw = self.gui.input(&events, (w, h), time);
        let ctx = self.gui.ctx.clone();
        let mut pass = super::panels::Pass::default();
        let out = ctx.run_ui(raw, |ui| pass = self.panels_ui(ui));
        self.world_rect = pass.world;
        let typing = ctx.egui_wants_keyboard_input();
        let items = self.gui.paint(out, (w, h));
        let mut actions = Vec::new();
        for e in &events {
            let HostEvent::KeyboardInput { key, pressed, .. } = e else {
                continue;
            };
            let vk = u32::try_from(key.virtual_key).unwrap_or(0);
            if matches!(vk, 0x11 | 0xA2 | 0xA3) {
                self.ctrl = *pressed;
            }
            if !*pressed || typing {
                continue;
            }
            if let Some(a) = self.scale_key(vk, self.ctrl) {
                actions.push(a);
                continue;
            }
            match vk {
                0x5A if self.ctrl => actions.push(super::UiAction::Undo),
                0x59 if self.ctrl => actions.push(super::UiAction::Redo),
                0x53 if self.ctrl => actions.push(super::UiAction::Save),
                _ => {}
            }
        }
        actions.append(&mut pass.actions);
        for a in actions {
            self.ui_action(a);
        }
        items
    }

    /// What the world view shows over a playtest: while the game is not in the world yet, what it
    /// is doing; and in its corner, Stop with the key that gives the editor back the keyboard and
    /// the mouse (or, while the editor has them, that a click gives them to the game).
    pub(super) fn play_marks(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        rect: egui::Rect,
        out: &mut Vec<super::UiAction>,
    ) {
        let Some(play) = self.playing.as_ref() else {
            return;
        };
        let t = self.text.table();
        if !play.in_world || self.character_work.is_some() {
            let words = if self.character_work.is_some() {
                self.characters.message.as_str()
            } else {
                t.get("play.loading")
            };
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                words,
                egui::FontId::proportional(18.0),
                egui::Color32::WHITE,
            );
        }
        let hint = t.get(if play.focused {
            "hints.play"
        } else {
            "hints.play_paused"
        });
        let mut stop = None;
        let corner = egui::Area::new(egui::Id::new("playtest-corner"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.left_top() + egui::vec2(8.0, 8.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let b = ui.button(
                            egui::RichText::new(t.get("top.stop"))
                                .color(super::panels::mark::FAULT),
                        );
                        if b.clicked() {
                            out.push(super::UiAction::StopPlay);
                        }
                        stop = Some(b.rect);
                        if self.panels.show_hints {
                            ui.label(hint);
                        }
                    });
                });
            });
        self.play_button = stop;
        let scale = self.gui.scale();
        let r = corner.response.rect;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a window's pixels
        let px = |v: f32| (v * scale).round().max(0.0) as u32;
        if let Some(play) = self.playing.as_mut() {
            play.corner = Some(Viewport {
                x: px(r.min.x),
                y: px(r.min.y),
                width: px(r.max.x).saturating_sub(px(r.min.x)),
                height: px(r.max.y).saturating_sub(px(r.min.y)),
            });
        }
    }

    /// Stop the playtest: the game logs off and disconnects as the client does on exit, and
    /// everything it had goes with it: its world, interface, sound, cursors and pointer hold.
    pub(super) fn stop_embedded(&mut self) {
        self.give_pointer(false);
        if let Some(mut play) = self.playing.take() {
            renderer(self.core.present.as_mut()).swap_world(&mut play.scene);
            std::mem::swap(&mut self.core.present, &mut play.game.present);
            play.game.present.set_inset(None);
            select_interface(play.interface);
            let (present, _) = play.game.shutdown_retaining_presentation();
            self.core.present = present;
            renderer(self.core.present.as_mut()).swap_world(&mut play.scene);
            // The game's world goes with it here, its workers told to stop.
            drop(play.scene.take());
        }
        self.core.present.set_inset(None);
        SHARED.with(|s| *s.borrow_mut() = Shared::default());
        self.held = super::Held::default();
        self.look = None;
    }
}

impl Drop for EditorApp {
    fn drop(&mut self) {
        self.stop_embedded();
    }
}

// Outdoor boundaries can belong to either neighboring cell after collision settling.
fn same_destination_cell(actual: u32, expected: u32) -> bool {
    use dereth_primitives::CellId;
    actual == expected
        || (CellId(actual).is_outdoor()
            && CellId(expected).is_outdoor()
            && actual >> 16 == expected >> 16)
}

// Input uses the editor's window; its lifetime and geometry remain editor-owned. The game's
// screen is the part of the window the editor gives it.
struct HostedWindow {
    window: NullWindow,
    handle: Option<isize>,
}

impl WindowHost for HostedWindow {
    fn raw_handle(&self) -> Option<isize> {
        self.handle
    }
    fn client_size(&self) -> (u32, u32) {
        SHARED
            .with(|s| s.borrow().region.map(|r| r.extent))
            .unwrap_or_else(|| self.window.client_size())
    }
    fn screen_metrics(
        &self,
        frame: (i32, i32, i32),
    ) -> dereth_client_contract::window::ScreenMetrics {
        self.window.screen_metrics(frame)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the playtest inside the editor's world view)
    use super::*;

    #[test]
    fn entry_accepts_adjacent_outdoor_cells_but_not_other_rooms_or_blocks() {
        assert!(same_destination_cell(0xA9B3001E, 0xA9B30026));
        assert!(same_destination_cell(0xA9B30100, 0xA9B30100));
        assert!(!same_destination_cell(0xA9B30100, 0xA9B30101));
        assert!(!same_destination_cell(0xA9B3001E, 0xA9B4001E));
        assert!(!same_destination_cell(0xA9B3001E, 0xA9B30100));
    }

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
}
