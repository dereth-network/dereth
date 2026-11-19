//! The application over a host, with no platform: the device's events routed to the game's
//! interface and then to the world's controls, the client's session carried over the host's
//! network, the world moved and framed, the settings read and kept, and what each frame shows.
//!
//! **Depends on** the client (`auberean-client-sdk`), the world a window shows (`auberean-scene`),
//! drawing's frame (`auberean-draw`), the client's own preferences' values and bank
//! (`auberean-options`), the world (`auberean-world`, `auberean-physics`), the key
//! maps' records and files (`auberean-assets`, `auberean-dat`), the game's sound
//! (`auberean-audio`) and the sound device's seam (`dereth-audio`), the overlay's textured
//! batches, which name no game (`dereth-ui-overlay`), `auberean-primitives` and
//! `dereth-primitives`. **Used by** the desktop client (`auberean-client`), which is its host, and
//! by the game's interface, which plugs in through [`interface::Interface`].
//!
//! A [`Shell`] is generic over its [`Host`] (the platform: clock, network, pointer, title,
//! settings, the player's files, the sound device and the pad) and shows one
//! [`interface::Interface`] at a time, of those the host's [`interface::InterfaceMaker`] builds,
//! as the client's own
//! preferences ([`client_preferences`]) choose. It holds those preferences for whichever
//! interface is shown, and carries out what they apply. Each event goes to the
//! interface first and, if the interface does not take it, to the world's controls
//! ([`controls`]), which turn it into the game's controls through the key maps ([`keys`]): the
//! player's, read from their files at startup and written when the key map screen applies new
//! ones. The controls go to the client as actions, which carries them in force as a script's or a
//! plugin's would be. Each frame ([`Shell::frame`]):
//!
//! 1. the datagrams that arrived go to the client (the SDK's driver runs the client's frame, the
//!    host's network its port);
//! 2. the game's view is refreshed (what the interface asks it to answer first), and the
//!    interface sees the game through it, with the chat lines it has not had yet
//!    ([`chat_history`]: an interface shown after a switch is handed what was written before), and
//!    asks for things: connecting, the client's actions, disconnecting, quitting
//!    ([`interface::Request`]);
//! 3. in the world, the client's controls in force (with the mouse's motion) move the body and
//!    the camera ([`auberean_scene`]);
//! 4. the client runs its frame, and what it sends goes out through the host;
//! 5. the sounds the interface asked for and the world's effects played are played, with the
//!    listener at the camera ([`sound`]);
//! 6. the frame to draw is the scene's, with the interface's overlay (and the developer's status
//!    overlay when asked for, [`dev`]) over it.
//!
//! The plugins the product hands it ([`Shell::set_plugins`], [`plugins`]) run as each session's
//! frame hook: a typed line goes to them before it is said, and the interface shows what they
//! print and draws their windows.
//!
//! In explore mode ([`explore`]) there is no interface and no session: each event goes to the free
//! camera's keys, and each frame is the world drawn from that camera with a line in the corner.
//!
//! **Must never** name a platform (a window system, the operating system, a sound device): the
//! host brings those (`cargo xtask seams`).

pub mod chat_history;
pub mod client_preferences;
pub mod command_line;
pub mod controls;
pub mod dev;
pub mod explore;
pub mod host;
pub mod input;
pub mod input_manager;
pub mod interface;
pub mod interface_keys;
pub mod keys;
pub mod net_binding;
pub mod pad;
pub mod plugins;
pub mod preferences;
pub mod profile;
pub mod settings;
pub mod sound;

use std::collections::VecDeque;

use core::net::SocketAddr;

use auberean_client_sdk::contract::control::ControlFrame;
use auberean_client_sdk::driver::CLIENT_CLOCK_START;
use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::runtime::SessionEnd;
use auberean_client_sdk::script::local::bare_name;
use auberean_client_sdk::{Client, ClientData, ClientSettings, Driver, Port};
use auberean_draw::scene::Frame;
use auberean_scene::{Scene, WorldData};

pub use auberean_scene::view::ViewFrame;
pub use host::{FriendsNews, Host};
use input::Event;
use interface::{
    CameraRequest, Choice, Game, Heard, Interface, InterfaceMaker, Overlay, Phase, Request, Shown,
};
pub use settings::{Presentation, Settings};

/// The game as the interface sees it, from the shell's fields one by one (so the interface itself
/// may be borrowed beside it).
macro_rules! game {
    ($s:ident, $phase:expr) => {
        Game {
            phase: $phase,
            view: $s.client.as_ref().map(|c| c.view()),
            strings: $s.client.as_ref().map(|c| {
                c.data_download().strings() as &dyn auberean_client_sdk::text::CachedStrings
            }),
            looks: &$s.looks,
            world: $s.world,
            size: $s.size,
            now: $s.host.now(),
            status: &$s.scene.status,
            heard: &$s.fresh,
            shown: &$s.shown,
            picked: &$s.scene.views.picked,
            behaving: &$s.scene.views.behaving,
            notices: &$s.notices,
            seen: &$s.scene.seen,
            world_pick: $s.world_pick,
            arrivals: $s.scene.arrivals(),
            keys: &$s.keys,
            screens_sound: $s
                .sound
                .has_device()
                .then(|| !$s.sound.finished(interface::SoundSlot::Screens)),
            stopped: $s.stopped,
            ended: $s.ended,
            interface: $s.interfaces.shown,
            preferences: &$s.preferences,
            preferences_changed: &$s.preferences_changed,
            told: &$s.told,
            plugin_lines: &$s.plugin_lines,
            events: &$s.events,
            keyed: &$s.keyed,
            files_read: &$s.files_read,
            pad: $s.pad,
            plugin_windows: &$s.plugins.windows,
        }
    };
}

/// The interfaces the shell can show: the one shown, those built and not shown, what builds the
/// others, and the one chosen to be shown from the next frame.
struct Interfaces<'w> {
    shown: Choice,
    current: Box<dyn Interface + 'w>,
    idle: Vec<(Choice, Box<dyn Interface + 'w>)>,
    maker: Box<dyn InterfaceMaker<'w> + 'w>,
    chosen: Option<Choice>,
}

/// The maker of a shell given one interface: it shows as the retail one, and no other can be.
struct OnlyOne<'w>(Option<Box<dyn Interface + 'w>>);

impl<'w> InterfaceMaker<'w> for OnlyOne<'w> {
    fn make(&mut self, choice: Choice) -> Result<Box<dyn Interface + 'w>, String> {
        match choice {
            Choice::Retail => self.0.take().ok_or_else(String::new),
            Choice::Horizon => Err(interface::NOT_AVAILABLE_YET.to_owned()),
        }
    }
}

/// The host's network as the driver's port.
struct HostPort<'a, H: Host>(&'a mut H);

impl<H: Host> Port for HostPort<'_, H> {
    fn receive(&mut self, _now: LocalTime) -> Option<(SocketAddr, Vec<u8>)> {
        self.0.receive()
    }

    fn send(&mut self, to: SocketAddr, bytes: &[u8], _now: LocalTime) {
        self.0.send(to, bytes);
    }
}

/// How many lines heard the status overlay keeps.
const HEARD: usize = 6;

/// What a frame shows.
#[derive(Debug)]
pub struct Output<'a> {
    /// The world, once the player is in it and portal space leaves it in sight; before the world,
    /// the view the interface asked to show behind its screens.
    pub world: Option<&'a Frame>,
    /// Portal space, while it is on: laid over the world `portal_alpha` of the way.
    pub portal: Option<&'a Frame>,
    pub portal_alpha: f32,
    /// A color laid over the world and portal space (under the interface's front), with its alpha.
    pub veil: Option<[f32; 4]>,
    /// Drawn over the world, in order.
    pub overlay: &'a Overlay,
    /// The interface's viewport scenes, each to be drawn in its rectangle.
    pub views: Vec<auberean_scene::view::ViewFrame>,
    /// The landblocks whose objects fade in, each with the opacity its objects are drawn at
    /// (see [`auberean_scene::Frames::fade`]).
    pub fade: Vec<((u8, u8), f32)>,
    /// Pictures for the device to ready this frame, ahead of the frame that draws them (see
    /// [`auberean_scene::Frames::warm`]).
    pub warm: &'a auberean_draw::warm::Share,
    /// The program should close.
    pub quit: bool,
    /// The player asked for a screenshot of this frame.
    pub screenshot: bool,
}

/// The application.
pub struct Shell<'w, H: Host> {
    pub host: H,
    world: &'w WorldData,
    data: ClientData,
    /// The session, while there is one: the client, its frames run over the host's network.
    pub client: Option<Driver>,
    pub scene: Scene<'w>,
    interfaces: Interfaces<'w>,
    /// The client's own preferences, one set whichever interface is shown.
    pub preferences: client_preferences::ClientPreferences,
    /// The preferences the client changed itself since the interface last saw them.
    preferences_changed: Vec<auberean_options::page::Change>,
    /// What the client tells the player, until the interface has seen it.
    told: Vec<String>,
    /// What the plugins printed, until the interface has seen it.
    plugin_lines: Vec<String>,
    pub controls: controls::Controls,
    /// The interface's actions the keys began and ended for this frame's update.
    keyed: Vec<(u32, bool)>,
    /// What decides which events reach the interface and the world's controls.
    input: input_manager::InputManager,
    /// The key maps and what their actions do.
    pub keys: keys::Keys,
    pub settings: Settings,
    /// The window's drawing area, in pixels.
    pub size: (u32, u32),
    /// Draw the developer's status overlay.
    pub dev_overlay: bool,
    entering: bool,
    /// How many entry refusals the shell has seen (a refusal ends the entering).
    refusals: u32,
    /// The notice that stopped the last session (the data download could not store a record).
    stopped: Option<u32>,
    /// How the last session ended under the player, when the server ended it.
    ended: Option<SessionEnd>,
    /// Seconds the last frame spent moving the world, in the client, and framing the world.
    pub costs: [f64; 9],
    quit: bool,
    screenshot: bool,
    /// The frame was drawn at full detail for a screenshot.
    shooting: bool,
    last: f64,
    frames: VecDeque<f64>,
    heard: VecDeque<String>,
    /// What was heard since the last frame.
    fresh: Vec<Heard>,
    /// The lines the scripts wrote for the chat windows since the last frame, after those the
    /// interface shown has not had yet.
    shown: Vec<Shown>,
    /// The lines the scripts wrote, kept for an interface shown later.
    chat_history: chat_history::ChatHistory,
    /// The notices the scripts sent the panels since the last frame.
    notices: Vec<auberean_client_sdk::contract::notice::Notice>,
    /// What the client told since the last frame.
    events: Vec<auberean_client_sdk::contract::event::GameEvent>,
    /// The player's files the interface asked to read last frame: each name, and what it holds.
    files_read: Vec<(String, Option<Vec<u8>>)>,
    /// The looks of the figures the interface's viewports show.
    looks: interface::Looks,
    overlay: Overlay,
    title_at: f64,
    /// The account the player logged in with, as typed, which the window's title names.
    account: String,
    /// The answer to the interface's last pick in the world, for its next frame.
    world_pick: Option<Option<u64>>,
    /// The client's random numbers: one generator for the program's life, never seeded, which
    /// the world, the interface and every session's scripts draw from.
    random: auberean_primitives::random::Random,
    /// Where the friends service is for the next session.
    pub friends: FriendsService,
    /// The world takes this client's own word on keeping a world's overlay over its files, as the
    /// launch said: only then is it sent. Off unless said.
    pub world_takes_overlays: bool,
    /// The game's sound.
    pub sound: sound::Sound,
    /// Explore mode, while it runs ([`explore`]): no interface, no session, a free camera.
    pub explore: Option<explore::Explore>,
    /// The world the interface asked to be drawn behind its screens before the world, this frame
    /// ([`interface::Request::Vista`]): the camera and the game clock.
    vista: Option<(auberean_scene::explore::FreeCamera, f64)>,
    /// The marks the interface asked laid on the ground this frame
    /// ([`interface::Request::GroundMarks`]).
    ground_marks: Vec<interface::GroundMarker<u64>>,
    /// The orbit camera's settings the shown interface asked for, or none for the game's own
    /// camera.
    scheme: Option<interface::OrbitSettings>,
    /// The host's pad as read this frame, for the interface.
    pad: Option<pad::PadState>,
    /// The plugins' seat: their host and their windows.
    plugins: plugins::Seat,
    /// The server the session is with, which the plugins' stores are kept under.
    server: Option<SocketAddr>,
}

/// Where a session finds the friends service.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FriendsService {
    /// At the login's own host, on the login's port number, with a ticket made from the account
    /// and the password the player logs in with (a host whose one connection carries it, as a web
    /// page's does).
    AtTheLogin,
    /// Where the options named it: its host and port (the original service's 2310 when none was
    /// named), whether to present the ticket, and the ticket (made from the login's account and
    /// password when none was named).
    At {
        host: String,
        port: Option<u16>,
        use_ticket: bool,
        ticket: Option<String>,
    },
    /// None: what a session has unless the options name one, as the September 2013 client had.
    #[default]
    Off,
}

impl FriendsService {
    /// The friends service the options name: at `host` (on `port`, else the original service's
    /// port), presenting the ticket when `use_ticket` (`ticket`, else one made from the login's
    /// account and password); with no host named, none at all, as the September 2013 client
    /// connects to none.
    #[must_use]
    pub fn named(
        host: Option<&str>,
        port: Option<u16>,
        use_ticket: bool,
        ticket: Option<&str>,
    ) -> Self {
        match host {
            Some(host) => Self::At {
                host: host.to_owned(),
                port,
                use_ticket,
                ticket: ticket.map(str::to_owned),
            },
            None => Self::Off,
        }
    }
}

/// The original friends service's port, which a service named without one is on.
const FRIENDS_PORT: u16 = 2310;

/// The character Ctrl+V types: a text entry pastes for it.
const PASTE: char = '\u{16}';

/// How the client names itself to the friends service.
const FRIENDS_SOFTWARE: &str = concat!("auberean/", env!("CARGO_PKG_VERSION"));

/// The web page where players ask for help.
pub const ASSISTANCE_PAGE: &str = "https://github.com/dereth-network/dereth/issues";

impl<H: Host> std::fmt::Debug for Shell<'_, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shell")
            .field("phase", &self.phase())
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// The client calls whose words the status overlay shows: a saying, a tell, a text for the chat
/// window.
pub const HEARD_CALLS: [u32; 3] = [0x8205_0016, 0x8205_0017, 0x8205_0011];

impl<'w, H: Host> Shell<'w, H> {
    /// The application over `host`, showing `world`, with the client's `data`, and `interface`
    /// over the world, the one interface it can show. Reads the settings the host kept; the sound
    /// starts, then the world and the interface, and the key maps and the action interpreter are
    /// ready.
    pub fn new(
        host: H,
        world: &'w WorldData,
        data: ClientData,
        interface: Box<dyn Interface + 'w>,
    ) -> Self {
        Self::with_interfaces(host, world, data, Box::new(OnlyOne(Some(interface))))
    }

    /// The application over `host`, showing `world`, with the client's `data`, and over the world
    /// the interfaces `maker` builds: first the one the client's preferences name, else the retail
    /// one (the player told why). Reads the settings the host kept; the sound starts, then the
    /// world and the interface, and the key maps and the action interpreter are ready.
    // D2-00141, D2-00027
    pub fn with_interfaces(
        mut host: H,
        world: &'w WorldData,
        data: ClientData,
        mut maker: Box<dyn InterfaceMaker<'w> + 'w>,
    ) -> Self {
        let settings = host
            .load_settings()
            .map(|t| Settings::read(&t))
            .unwrap_or_default();
        let random = auberean_primitives::random::Random::new();
        // The graphics preferences kept, else a fresh install's: the client's preferences start
        // from them and the world is drawn by them from its first frame.
        let graphics = settings.starting_graphics();
        let mut scene = Scene::new(world, &graphics);
        scene.set_terrain_blending(settings.terrain_blending);
        scene.set_random(&random);
        scene.camera = auberean_scene::CameraControl::new(settings.camera);
        let mut preferences = client_preferences::ClientPreferences::default();
        preferences.start(&settings, &world.records);
        preferences.set(&graphics);
        // The interface the preferences name, else the retail one.
        let mut told = Vec::new();
        let wanted = preferences.interface();
        let (shown, mut current) = match maker.make(wanted) {
            Ok(i) => (wanted, i),
            Err(why) => {
                tracing::warn!(interface = wanted.name(), %why, "the interface chosen can't be shown");
                if !why.is_empty() {
                    told.push(why);
                }
                let retail = if wanted == Choice::Retail {
                    None
                } else {
                    maker.make(Choice::Retail).ok()
                };
                (
                    Choice::Retail,
                    retail.unwrap_or_else(|| Box::new(interface::NoInterface)),
                )
            }
        };
        preferences.show_interface(shown);
        current.set_random(&random);
        // The key maps: the base map, then the player's from their files (the data's Defaults where
        // a file does not load).
        let mut keys = keys::Keys::start(
            keys::KeyRecords::read(&world.records),
            settings.key_files.clone(),
            &mut crate::host::HostFiles(&mut host),
        );
        // The shown interface's own key maps, in front of the player's.
        let own_keys =
            interface_keys::layers(shown, &keys.records, &crate::host::HostFiles(&mut host));
        keys.stack.set_interface(own_keys);
        let last = host.now();
        let sound = sound::Sound::start(
            &mut host,
            data.files.files.clone(),
            auberean_scene::BLOCK_LENGTH,
        );
        let mut shell = Self {
            size: settings.window,
            host,
            world,
            data,
            client: None,
            scene,
            interfaces: Interfaces {
                shown,
                current,
                idle: Vec::new(),
                maker,
                chosen: None,
            },
            preferences,
            preferences_changed: Vec::new(),
            told,
            plugin_lines: Vec::new(),
            controls: controls::Controls::default(),
            keyed: Vec::new(),
            input: input_manager::InputManager::default(),
            keys,
            settings,
            dev_overlay: false,
            entering: false,
            refusals: 0,
            stopped: None,
            ended: None,
            costs: [0.0; 9],
            quit: false,
            screenshot: false,
            shooting: false,
            last,
            frames: VecDeque::new(),
            heard: VecDeque::new(),
            fresh: Vec::new(),
            shown: Vec::new(),
            chat_history: chat_history::ChatHistory::default(),
            notices: Vec::new(),
            events: Vec::new(),
            files_read: Vec::new(),
            looks: interface::Looks::default(),
            overlay: Overlay::default(),
            title_at: f64::NEG_INFINITY,
            account: String::new(),
            world_pick: None,
            random,
            friends: FriendsService::default(),
            world_takes_overlays: false,
            sound,
            explore: None,
            vista: None,
            ground_marks: Vec::new(),
            scheme: None,
            pad: None,
            plugins: plugins::Seat::default(),
            server: None,
        };
        let phase = shell.phase();
        let game = game!(shell, phase);
        shell.interfaces.current.enter(&game);
        shell
    }

    /// Load plugins: `host` is installed as the client's frame hook for each session from the next
    /// (and for the session there is, if one is), and kept between sessions. A line the player
    /// types goes to it before it is said; what it prints is told the player; its windows reach
    /// the interface ([`Game::plugin_windows`]).
    pub fn set_plugins(&mut self, host: auberean_client_sdk::plugins::PluginHost) {
        self.plugins.idle = Some(host);
        if let (Some(d), Some(server)) = (&mut self.client, self.server) {
            self.plugins
                .install(d, server.to_string(), self.account.clone());
        }
    }

    /// The plugin host, when plugins are loaded: the session's frame hook while there is one.
    pub fn plugin_host(&mut self) -> Option<&mut auberean_client_sdk::plugins::PluginHost> {
        self.plugins.host(self.client.as_mut())
    }

    /// The session's client let go of, the plugin host taken back from it first.
    fn drop_client(&mut self) -> Option<Driver> {
        let mut d = self.client.take()?;
        self.plugins.take_back(&mut d);
        Some(d)
    }

    /// The top gore level and blood color the command line forces over the language's (0: its
    /// own): the client's preferences read again with them.
    // D2-14179, D2-14180, D2-14185, D2-14186
    pub fn force_gore(&mut self, gore: u32, blood: u32) {
        self.preferences.force_tops(gore, blood);
        self.preferences.start(&self.settings, &self.world.records);
        self.preferences.set(&self.settings.starting_graphics());
        self.preferences.show_interface(self.interfaces.shown);
    }

    /// The interface shown.
    #[must_use]
    pub const fn interface(&self) -> Choice {
        self.interfaces.shown
    }

    /// Where the session is.
    #[must_use]
    pub fn phase(&self) -> Phase {
        let Some(c) = &self.client else {
            return Phase::Disconnected;
        };
        if c.model.player_object().is_some() && c.body.placed() {
            return Phase::World;
        }
        if self.entering || c.model.player.is_some() {
            return Phase::Entering;
        }
        if c.model.characters.is_some() {
            Phase::Characters
        } else {
            Phase::Connecting
        }
    }

    /// One event from the host: the interface first, then the world's controls; in explore mode,
    /// the camera's alone.
    pub fn event(&mut self, e: &Event) {
        if self.explore.is_some() {
            self.explore_event(e);
            return;
        }
        self.input.hear_the_pointer_in_background(
            self.interfaces.current.hears_the_pointer_in_background(),
        );
        if !self.input.admit(e) {
            return;
        }
        if let Event::Pointer { x, y } = e {
            self.scene.pointer = Some(((*x, *y), self.size));
        }
        match e {
            Event::Resized { width, height } => {
                self.size = ((*width).max(1), (*height).max(1));
                // The size kept is the window's: full screen fills the display.
                if !self.settings.full_screen {
                    self.settings.window = self.size;
                }
                return;
            }
            Event::Close => {
                self.quit = true;
                return;
            }
            Event::Key { .. }
            | Event::Text(_)
            | Event::Composing(_)
            | Event::Button { .. }
            | Event::Pointer { .. }
            | Event::Motion { .. }
            | Event::Wheel { .. }
            | Event::Focus(_) => {}
        }
        // Ctrl+V's character pastes what the system's clipboard holds as it is typed.
        // D2-08342
        if matches!(e, Event::Text(t) if t.contains(PASTE)) {
            self.interfaces
                .current
                .set_clipboard(self.host.clipboard_text());
        }
        let phase = self.phase();
        let game = game!(self, phase);
        if !self.interfaces.current.event(e, &game) {
            self.controls.event(e, &self.keys);
            self.send_controls();
        } else if let Event::Button {
            button,
            down: false,
        } = e
        {
            // A button let go over the interface is no longer held for the orbit camera.
            self.controls.button_released(*button);
        }
        self.hand_on_copied();
        if let Some((x, y)) = self.input.take_missed() {
            self.event(&Event::Pointer { x, y });
        }
    }

    /// What the interface cut or copied, put on the system's clipboard.
    // D2-08338
    fn hand_on_copied(&mut self) {
        if let Some(text) = self.interfaces.current.take_copied() {
            self.host.set_clipboard_text(&text);
        }
    }

    /// The settings the command line sets over those read.
    pub fn apply_line(&mut self, line: &command_line::ClientLine) {
        line.apply_to(&mut self.settings);
        self.scene
            .set_terrain_blending(self.settings.terrain_blending);
    }

    /// The sizes the host's display can take, in pixels, from a host that lets the player choose
    /// how the game is shown: the options window's rendering section lists them. A host that
    /// does not say leaves that section empty.
    pub fn set_display_modes(&mut self, modes: &[(u32, u32)]) {
        self.preferences.set_presentation(modes, &self.settings);
    }

    /// Whether the host can draw the world with the remaster: the options window's rendering
    /// section offers a box for it only while it can. A host that does not say offers none.
    pub fn offer_remaster(&mut self, offered: bool) {
        self.preferences.offer_remaster(offered);
    }

    /// Full screen, or back to the window.
    // D2-08327
    pub fn toggle_full_screen(&mut self) {
        self.preferences.toggle_full_screen();
        self.carry_out_preferences();
    }

    /// What the client's preferences applied, carried out: the world drawn by the graphics, the
    /// camera's options, how the game is shown, the effects' and the sound's preferences, what is
    /// kept in the file, the interface chosen; and the preferences the client changed itself
    /// gathered for the interface.
    fn carry_out_preferences(&mut self) {
        if let Some(g) = self.preferences.take_applied() {
            self.scene.init_preferences(&g);
            self.settings.graphics = Some(g);
        }
        if let Some(o) = self.preferences.take_camera() {
            self.scene.camera.set_options(o);
            self.settings.camera = o;
        }
        if let Some(p) = self.preferences.take_presentation() {
            self.settings.set_presentation(&p);
            self.host.set_presentation(&self.settings.presentation());
        }
        if let Some(c) = self.preferences.take_channels() {
            self.scene.set_fx_channels(c);
        }
        for (key, value, applied) in self.preferences.take_sound() {
            self.sound.request(interface::SoundOrder::Preference {
                key,
                value,
                applied,
            });
        }
        if let Some(writes) = self.preferences.take_kept() {
            for (section, key, value) in &writes {
                self.settings.keep(section, key, value);
            }
            self.host.save_settings(&self.settings.write());
        }
        if let Some(c) = self.preferences.take_interface() {
            self.choose_interface(c);
        }
        if self.preferences.take_orbit() {
            self.follow_scheme();
        }
        self.preferences_changed
            .extend(self.preferences.take_changed());
    }

    /// An interface chosen: built the first time (or refused, the player told why and the
    /// preference put back to the interface shown), then shown from the next frame.
    fn choose_interface(&mut self, choice: Choice) {
        let i = &mut self.interfaces;
        let built = choice == i.shown || i.idle.iter().any(|(c, _)| *c == choice);
        if !built {
            match i.maker.make(choice) {
                Ok(mut made) => {
                    made.set_random(&self.random);
                    i.idle.push((choice, made));
                }
                Err(why) => {
                    tracing::info!(interface = choice.name(), %why, "interface refused");
                    self.told.push(why);
                    self.preferences.show_interface(i.shown);
                    self.preferences_changed
                        .extend(self.preferences.take_changed());
                    return;
                }
            }
        }
        i.chosen = (choice != i.shown).then_some(choice);
        self.preferences.show_interface(choice);
    }

    /// The interface chosen since the last frame shown from this one: the one shown left (what it
    /// asks carried out), the world's controls letting go of what is held and mouse look ended,
    /// then the one chosen entered.
    fn follow_interface(&mut self) {
        let Some(choice) = self.interfaces.chosen.take() else {
            return;
        };
        let Some(at) = self.interfaces.idle.iter().position(|(c, _)| *c == choice) else {
            return;
        };
        let mut requests = Vec::new();
        let phase = self.phase();
        let game = game!(self, phase);
        self.interfaces.current.leave(&game, &mut requests);
        self.interfaces.current.release_textures(&mut self.overlay);
        for r in requests {
            self.request(r);
        }
        let (_, next) = self.interfaces.idle.swap_remove(at);
        let was = std::mem::replace(&mut self.interfaces.current, next);
        let left = std::mem::replace(&mut self.interfaces.shown, choice);
        self.interfaces.idle.push((left, was));
        self.controls.let_go();
        self.send_controls();
        let heading = self.scene.status.heading;
        self.scene.camera.set_mouse_look(false, heading);
        // Every interface starts with the game's own camera, and the keys through its own maps.
        self.scheme = None;
        self.scene.set_scheme(None);
        let own_keys = interface_keys::layers(
            choice,
            &self.keys.records,
            &crate::host::HostFiles(&mut self.host),
        );
        self.keys.stack.set_interface(own_keys);
        let phase = self.phase();
        let game = game!(self, phase);
        self.interfaces.current.enter(&game);
        tracing::info!(interface = choice.name(), "interface shown");
    }

    fn request(&mut self, r: Request) {
        match r {
            Request::Connect {
                server,
                account,
                password,
            } => self.connect(server, &account, &password),
            Request::Act(auberean_client_sdk::Action::Say { text })
                if self
                    .plugins
                    .host(self.client.as_mut())
                    .is_some_and(|h| h.command(&text)) =>
            {
                // A line a plugin claims is the plugin's, never said.
            }
            Request::Act(a) => {
                if let Some(c) = &mut self.client {
                    if matches!(a, auberean_client_sdk::Action::PlayCharacter { .. }) {
                        self.entering = true;
                    }
                    c.act(a);
                }
            }
            Request::Disconnect => self.disconnect(),
            Request::OpenAssistancePage => {
                if let Err(code) = self.host.open_page(ASSISTANCE_PAGE) {
                    let text = format!(
                        "An error occurred while trying to launch your web browser. (Error code \
                         {code})\nThe web site to submit an urgent assistance request is listed \
                         below. Please go there to complete your request.\n{ASSISTANCE_PAGE}\n"
                    );
                    self.host.error_box("Auberean Error", &text);
                }
            }
            Request::Quit => self.quit = true,
            Request::Screenshot => self.screenshot = true,
            Request::ShowWorld(on) => self.scene.show_world(on),
            Request::PortalScenePlaying(on) => self.scene.portal_scene_playing = on,
            Request::PortalSpaceLingers(seconds) => self.scene.portal_linger = seconds,
            Request::Vista { camera, time } => self.vista = Some((camera, time)),
            Request::GroundMarks(marks) => self.ground_marks = marks,
            Request::Camera(c) => self.camera(c),
            Request::PickWorld { x, y } => {
                self.world_pick = Some(self.scene.pick_world((x, y), self.size));
            }
            Request::Preferences(r) => {
                self.preferences.handle(&r);
                self.carry_out_preferences();
            }
            Request::ChooseInterface(c) => self.choose_interface(c),
            Request::WritePlayerFile { name, bytes } => {
                let written = bare_name(&name)
                    && self
                        .host
                        .local_system()
                        .is_some_and(|mut l| l.write_file(&name, &bytes));
                if !written {
                    tracing::warn!(name, "the player's file could not be written");
                }
            }
            Request::ReadPlayerFile { name } => {
                let bytes = if bare_name(&name) {
                    self.host
                        .local_system()
                        .and_then(|mut l| l.read_file(&name))
                } else {
                    None
                };
                self.files_read.push((name, bytes));
            }
            Request::Sound(r) => self.sound.request(r),
            Request::InputMethod(area) => self.host.set_input_method(area),
            Request::BindKeyMaps(maps) => {
                let [n, s, c] = &maps;
                self.keys.bind([n, s, c]);
            }
            Request::PluginClick {
                plugin,
                window,
                node,
            } => {
                if let Some(Err(why)) = self
                    .plugins
                    .host(self.client.as_mut())
                    .map(|h| h.click(&plugin, &window, &node))
                {
                    tracing::warn!(%why, "a press on a plugin's window");
                }
            }
            Request::PluginWindow {
                plugin,
                window,
                open,
            } => {
                let done = self.plugins.host(self.client.as_mut()).map(|h| {
                    if open {
                        h.open_window(&plugin, &window)
                    } else {
                        h.close_window(&plugin, &window)
                    }
                });
                if let Some(Err(why)) = done {
                    tracing::warn!(%why, "a plugin's window opened or closed");
                }
            }
            Request::InterfaceKeyMaps(maps) => {
                let [n, s, c] = &maps;
                let shown = self.interfaces.shown;
                let mut files = crate::host::HostFiles(&mut self.host);
                if !interface_keys::write(shown, [n, s, c], &self.keys.records, &mut files) {
                    tracing::warn!("the interface's key maps' files could not be written");
                }
                let own = interface_keys::layers(shown, &self.keys.records, &files);
                self.keys.stack.set_interface(own);
            }
            Request::KeyMaps(maps) => {
                let [n, s, c] = &maps;
                if !self
                    .keys
                    .apply([n, s, c], &mut crate::host::HostFiles(&mut self.host))
                {
                    tracing::warn!("the key maps' files could not be written");
                }
            }
        }
    }

    /// The interface sets the camera: mouse look and first person to the state asked (each
    /// switched over only when it differs), a zoom step.
    fn camera(&mut self, c: CameraRequest) {
        use auberean_scene::camera::Press;
        // The camera's direction is kept where it points, turned by the player's heading.
        let heading = self.scene.status.heading;
        let cam = &mut self.scene.camera;
        match c {
            CameraRequest::MouseLook(on) => cam.set_mouse_look(on, heading),
            CameraRequest::FirstPerson(on) => {
                if cam.first_person() != on {
                    cam.press(Press::FirstPerson);
                }
            }
            CameraRequest::Zoom { inward } => cam.press(Press::Wheel(if inward { 1 } else { -1 })),
            CameraRequest::Scheme(settings) => {
                self.scheme = settings;
                self.follow_scheme();
            }
        }
    }

    /// The camera the world is shown with, as the interface asked: the orbit camera with its
    /// settings and the player's orbit preferences laid over them (mouse look ending as it
    /// begins), or the game's own.
    fn follow_scheme(&mut self) {
        let settings = self.scheme.map(|s| self.preferences.orbit_over(s));
        if settings.is_some() && self.scene.orbit.is_none() {
            let heading = self.scene.status.heading;
            self.scene.camera.set_mouse_look(false, heading);
        }
        self.scene.set_scheme(settings);
    }

    /// The client stops for notice `notice`: the session ends where it is (no goodbye is said),
    /// and the interface is told why.
    // D2-09653, D2-17492
    fn stop(&mut self, notice: u32) {
        tracing::error!(
            notice = format_args!("{notice:#x}"),
            line = auberean_client_sdk::runtime::download::notice_line(notice),
            "the client stops"
        );
        self.stopped = Some(notice);
        if self.drop_client().is_some_and(|c| c.friends().is_some()) {
            self.host.friends_close();
        }
        self.entering = false;
    }

    /// The session ended under the player (the login failed, or the server is gone): the
    /// session ends where it is, and the interface is told how.
    fn end(&mut self, end: SessionEnd) {
        tracing::info!(?end, "the session ended");
        self.ended = Some(end);
        if self.drop_client().is_some_and(|c| c.friends().is_some()) {
            self.host.friends_close();
        }
        self.entering = false;
    }

    // D2-00098
    fn connect(&mut self, server: SocketAddr, account: &str, password: &str) {
        self.stopped = None;
        self.ended = None;
        if let Err(e) = self.host.connect(server) {
            tracing::warn!(%server, "cannot reach the server: {e}");
            return;
        }
        account.trim().clone_into(&mut self.account);
        let mut key = [0u8; 64];
        self.host.random(&mut key);
        let settings = ClientSettings {
            login_server: server,
            account: account.as_bytes().to_vec(),
            ticket: password.as_bytes().to_vec(),
            timestamp: self.host.unix_time(),
            session_key: key,
            script_sends: true,
        };
        let mut client = Client::new(settings, self.data.clone());
        client.data_download_mut().world_takes_overlays = self.world_takes_overlays;
        client.set_random(&self.random);
        let local = self.host.local_system();
        client.with_host(|h| h.set_local(local));
        client.observe.extend(HEARD_CALLS);
        if self.client.as_ref().is_some_and(|c| c.friends().is_some()) {
            self.host.friends_close();
        }
        if let Some(mut old) = self.client.take() {
            self.plugins.take_back(&mut old);
        }
        if let Some(options) = self.friends_options(server, account, password) {
            let link = auberean_client_sdk::net::friends::FriendsLink::new(FRIENDS_SOFTWARE, None);
            client.use_friends(auberean_client_sdk::runtime::friends::Friends::new(
                Box::new(link),
                &options,
            ));
        }
        // Asked for during a frame, whose time the session's first frame takes.
        let mut driver = Driver::during_frame(client, self.last);
        self.plugins
            .install(&mut driver, server.to_string(), self.account.clone());
        self.server = Some(server);
        self.client = Some(driver);
        self.entering = false;
    }

    /// The player leaves the server: a goodbye on every connection, and the session goes.
    // D2-00099
    fn disconnect(&mut self) {
        if let Some(d) = &mut self.client {
            d.disconnect(&mut HostPort(&mut self.host));
        }
        if self.drop_client().is_some_and(|c| c.friends().is_some()) {
            self.host.friends_close();
        }
        self.entering = false;
    }

    /// The friends service a session at `server` as `account` uses, when the host carries its
    /// stream and the player did not turn it off.
    fn friends_options(
        &self,
        server: SocketAddr,
        account: &str,
        password: &str,
    ) -> Option<auberean_client_sdk::runtime::friends::FriendsOptions> {
        use auberean_client_sdk::runtime::friends::FriendsOptions;
        if !self.host.carries_friends() {
            return None;
        }
        let own = || {
            auberean_client_sdk::net::friends::Ticket::new(account, password)
                .encode()
                .ok()
        };
        match &self.friends {
            FriendsService::Off => None,
            FriendsService::AtTheLogin => Some(FriendsOptions {
                server: server.ip().to_string(),
                port: server.port(),
                use_ticket: true,
                ticket: own(),
                named: false,
            }),
            FriendsService::At {
                host,
                port,
                use_ticket,
                ticket,
            } => Some(FriendsOptions {
                server: host.clone(),
                port: port.unwrap_or(FRIENDS_PORT),
                use_ticket: *use_ticket,
                ticket: ticket.clone().or_else(own),
                named: true,
            }),
        }
    }

    /// The friends service's stream carried both ways: what the host's connection did, to the
    /// client's link; what the link asks, to the host.
    fn carry_friends(&mut self, now: core::time::Duration) {
        use auberean_client_sdk::net::friends::Io;
        let Some(link) = self
            .client
            .as_mut()
            .and_then(|c| c.friends_mut())
            .and_then(|f| f.connection().io())
        else {
            return;
        };
        while let Some(news) = self.host.friends_news() {
            match news {
                FriendsNews::Opened => link.opened(now),
                FriendsNews::Failed => link.failed(now),
                FriendsNews::Closed => link.closed(now),
                FriendsNews::Bytes(b) => link.on_bytes(now, &b),
            }
        }
        while let Some(io) = link.poll_io(now) {
            match io {
                Io::Dial { host, port } => self.host.friends_dial(&host, port),
                Io::Write(bytes) => self.host.friends_send(&bytes),
                Io::Close => self.host.friends_close(),
            }
        }
    }

    /// The controls the device's keys and buttons pressed or let go, carried to the client; with
    /// no client they go nowhere.
    fn send_controls(&mut self) {
        let actions = self.controls.take_actions();
        if let Some(c) = &mut self.client {
            for a in actions {
                c.act(a);
            }
        }
    }

    fn hear(&mut self) {
        self.fresh.clear();
        self.shown.clear();
        self.notices.clear();
        self.events.clear();
        // What the plugins printed goes to the interface's log; so do their windows.
        let printed = self.plugins.frame(self.client.as_mut());
        self.plugin_lines.extend(printed);
        // A camera a plugin chose stands until the interface asks for another.
        let chosen = self
            .plugins
            .host(self.client.as_mut())
            .and_then(auberean_client_sdk::plugins::PluginHost::take_camera);
        if let Some(scheme) = chosen {
            self.camera(CameraRequest::Scheme(scheme));
        }
        let Some(c) = &mut self.client else { return };
        self.events = c.take_events();
        self.notices = c.take_notices();
        for s in std::mem::take(&mut c.shown) {
            if let Some(text) = c.write_text(&s.text) {
                self.shown.push(Shown {
                    kind: s.kind,
                    about: s.about,
                    text,
                    info: Some(auberean_client_sdk::runtime::text::from_script(&s.text)),
                    notices_before: s.notices_before,
                });
            }
        }
        let observed = std::mem::take(&mut c.observed);
        let Some((_, program)) = &self.data.scripts else {
            return;
        };
        for (_, function, args, from) in observed {
            let Some(a) = args else { continue };
            let Ok(values) = auberean_client_sdk::script::args::decode(&a.0, &program.library)
            else {
                continue;
            };
            if let Some(t) = auberean_client_sdk::runtime::calls::text_of(&values) {
                tracing::info!("heard: {t}");
                if self.heard.len() == HEARD {
                    self.heard.pop_front();
                }
                self.heard.push_back(t.clone());
                self.fresh.push(Heard {
                    function,
                    from,
                    text: t,
                });
            }
        }
    }

    /// The looks the interface's viewports show, before it draws: the player's in the world, the
    /// account's characters' on the screens before it.
    fn follow_looks(&mut self, phase: Phase) {
        let records = &self.world.records;
        let c = self.client.as_ref();
        self.looks.player = c
            .filter(|_| phase == Phase::World)
            .and_then(|c| c.model.player.and_then(|p| c.model.object(p)))
            .map(|o| (o.id, auberean_scene::standing_desc(records, &o.visual)));
        self.looks.characters.clear();
        if phase != Phase::World {
            if let Some(list) = c.and_then(|c| c.model.characters.as_ref()) {
                self.looks.characters.extend(
                    list.characters
                        .iter()
                        .map(|ch| auberean_scene::desc_of(records, &ch.visual)),
                );
            }
        }
    }

    /// One frame: the network, the interface, the world, the client; the world's frame, what is
    /// placed over it; then what to show.
    pub fn frame(&mut self) -> Output<'_> {
        if self.explore.is_some() {
            return self.explore_frame();
        }
        self.input.frame_begins();
        self.follow_interface();
        let now = self.host.now();
        #[allow(clippy::cast_possible_truncation)]
        let dt = (now - self.last).clamp(0.0, 0.25) as f32;
        self.last = now;
        self.frames.push_back(now);
        while self.frames.front().is_some_and(|t| now - t > 1.0) {
            self.frames.pop_front();
        }
        let stream_now = core::time::Duration::from_secs_f64(CLIENT_CLOCK_START + now);
        let began = self.host.now();
        if let Some(d) = &mut self.client {
            d.receive(now, &mut HostPort(&mut self.host));
        }
        self.carry_friends(stream_now);
        // What the interface asks the view's refresh to answer; then the view, before the
        // frame's notices are taken (it reads what they change at the next refresh).
        let mut asked = Vec::new();
        self.interfaces.current.before_view(&mut asked);
        for r in asked {
            self.request(r);
        }
        let records = &self.world.records;
        if let Some(c) = &mut self.client {
            c.refresh_view(&|v| auberean_scene::icon_layers(records, v));
        }
        self.hear();
        let fresh = std::mem::take(&mut self.shown);
        self.shown = self.chat_history.hand(self.interfaces.shown, fresh);
        self.costs[3] = self.host.now() - began;
        let began = self.host.now();
        // The client's preferences: the display's size follows the window's; what they applied
        // (and, first, what was read) takes effect.
        if self.size.0 > 0 && self.size.1 > 0 {
            self.preferences.follow_window(self.size);
        }
        self.carry_out_preferences();
        // The interface's requests, the host's pad read for it.
        self.pad = self.host.gamepad();
        let mut requests = Vec::new();
        let phase = self.phase();
        self.keyed = self.controls.take_keyed();
        // Asked again each frame it is wanted.
        let had_vista = self.vista.take().is_some();
        self.ground_marks.clear();
        let game = game!(self, phase);
        self.interfaces.current.update(&game, &mut requests);
        self.keyed.clear();
        self.preferences_changed.clear();
        self.told.clear();
        self.plugin_lines.clear();
        self.hand_on_copied();
        self.world_pick = None;
        self.files_read.clear();
        for r in requests {
            self.request(r);
        }
        self.costs[4] = self.host.now() - began;
        // The world's controls.
        let phase = self.phase();
        self.controls
            .set_text_mode(&self.keys, self.interfaces.current.text_mode());
        self.send_controls();
        let frame = match &mut self.client {
            Some(c) => {
                // Under the orbit camera the mouse never steers as mouse look's does: the orbit
                // camera steers the body itself.
                c.controls.steers =
                    self.scene.camera.options.mouse_look_steers && self.scene.orbit.is_none();
                c.take_control_frame()
            }
            None => ControlFrame::default(),
        };
        let intent = self.controls.intent(frame);
        if phase == Phase::World {
            self.entering = false;
        }
        // The frame a screenshot is taken of is drawn at full detail, and the next as before.
        // D2-10548, D2-10549
        if std::mem::take(&mut self.shooting) {
            self.scene.set_full_detail(false);
        }
        let screenshot = std::mem::take(&mut self.screenshot);
        if screenshot {
            self.shooting = true;
            self.scene.set_full_detail(true);
        }
        if let Some(c) = &mut self.client {
            let t0 = self.host.now();
            self.scene.entering = self.entering;
            self.scene.update(c, &intent, now, dt);
            // D2-09649, D2-10665
            if self.scene.player_moving_to() || self.scene.player_teleported() {
                c.controls.turn_off_run_lock();
            }
            let t1 = self.host.now();
            c.finish(&mut HostPort(&mut self.host));
            self.costs[0] = t1 - t0;
            self.costs[1] = self.host.now() - t1;
            if let Some(players) = c.take_world_players() {
                self.host.world_players(players);
            }
            self.scene.game_time = c.world_time();
            // An entry refused: the client is back at the character list.
            if let Some((_, n)) = c.model.refused {
                if n != self.refusals {
                    self.refusals = n;
                    self.entering = false;
                }
            }
        }
        // A record the data download could not store stops the client: the session ends there,
        // and the interface says why.
        if let Some(notice) = self.client.as_ref().and_then(|c| c.data_download().fatal) {
            self.stop(notice);
        }
        if let Some(end) = self.client.as_deref().and_then(Client::session_end) {
            self.end(end);
        }
        self.carry_friends(stream_now);
        let phase_now = self.phase();
        let free = self
            .interfaces
            .current
            .wants_pointer(&game!(self, phase_now));
        let pointer = self.scene.camera.mouse_look && !free;
        if let Some(c) = &mut self.client {
            c.controls.mouse_look = self.scene.camera.mouse_look;
        }
        if let Some(at) = self.input.set_mouse_look_mode(pointer, self.size) {
            self.host.hold_pointer(pointer);
            self.controls.mouse_look_switched();
            self.send_controls();
            if let Some((x, y)) = at {
                // Past the gate: the pointer is held now, and it is the shell that moved it.
                self.scene.pointer = Some(((x, y), self.size));
                let game = game!(self, phase_now);
                self.interfaces
                    .current
                    .event(&Event::Pointer { x, y }, &game);
            }
        }
        if now - self.title_at > 0.5 {
            self.title_at = now;
            let t = self.title();
            self.host.set_title(&t);
        }
        // The world's frame, then what is placed over the world from its camera; before the world,
        // the world the interface asked to show behind its screens.
        let t = self.host.now();
        let before = !matches!(phase, Phase::Entering | Phase::World);
        match self.vista.filter(|_| before) {
            Some((camera, time)) => {
                self.scene.game_time = time;
                self.scene.build_explore(now, &camera);
            }
            None => {
                if had_vista {
                    self.scene.forget_explore();
                }
                let textures = &self.overlay.textures;
                self.scene
                    .set_ground_marks(std::mem::take(&mut self.ground_marks), &|t| {
                        textures.get(t).cloned()
                    });
                self.scene.build_frames(now, phase == Phase::World);
            }
        }
        self.costs[2] = self.host.now() - t;
        let t = self.host.now();
        let mut requests = Vec::new();
        let phase_now = self.phase();
        let game = game!(self, phase_now);
        self.interfaces.current.after_world(&game, &mut requests);
        for r in requests {
            self.request(r);
        }
        self.costs[5] = self.host.now() - t;
        let t = self.host.now();
        self.sound_frame(now, phase_now == Phase::World);
        self.costs[6] = self.host.now() - t;
        let t = self.host.now();
        // What to draw over the world.
        let mut overlay = std::mem::take(&mut self.overlay);
        overlay.quads.clear();
        overlay.items.clear();
        overlay.scenes.clear();
        let phase_now = self.phase();
        self.follow_looks(phase_now);
        let game = game!(self, phase_now);
        self.interfaces.current.draw(&game, &mut overlay);
        if self.dev_overlay {
            let lines = self.status_lines();
            dev::overlay(&mut overlay, &lines);
        }
        self.overlay = overlay;
        self.costs[7] = self.host.now() - t;
        let quit = self.quit;
        if quit {
            // Closing, in the world or out of it: a goodbye on every connection, and the program
            // ends (the server takes a character left in the world out itself).
            // D2-08334, D2-00085
            if let Some(h) = self.plugin_host() {
                h.flush();
            }
            if let Some(d) = &mut self.client {
                d.client_mut().disconnect();
                d.end_friends();
                d.send(&mut HostPort(&mut self.host));
            }
            self.carry_friends(stream_now);
            self.host.save_settings(&self.settings.write());
        }
        let t = self.host.now();
        let views = self.scene.view_frames(&self.overlay.scenes, dt);
        self.costs[8] = self.host.now() - t;
        let frames = self.scene.frames(now, phase == Phase::World);
        Output {
            world: frames.world,
            portal: frames.portal,
            portal_alpha: frames.portal_alpha,
            veil: frames.over,
            overlay: &self.overlay,
            views,
            fade: frames.fade,
            warm: frames.warm,
            quit,
            screenshot,
        }
    }

    /// The sound's turn: the sounds the world's effects played or stopped, the sounds of objects
    /// gone stopped and of objects out of the world silent, then the ambient sound's turn and the
    /// manager's with the listener at the camera.
    fn sound_frame(&mut self, now: f64, in_world: bool) {
        for order in self.scene.take_portal_sounds() {
            let records = &self.world.records;
            self.sound.portal(order, &|value| {
                records.data_id_by_enum(value, auberean_draw::portal::SOUND_MAPPING)
            });
        }
        for (object, s) in self.scene.take_sounds() {
            self.sound.effect(object, s);
        }
        let mut muted = std::collections::BTreeSet::new();
        for e in self.sound.manager.emitters() {
            // The ambient sound's objects are no objects of the world.
            if auberean_audio::ambient::is_emitter(e) {
                continue;
            }
            let object = self.client.as_ref().and_then(|c| c.model.object(e));
            match sound::heard(object.map(|o| o.in_world), self.scene.should_mute_sound(e)) {
                sound::Heard::Gone => self.sound.object_gone(e),
                sound::Heard::Silent => {
                    muted.insert(e);
                }
                sound::Heard::Heard => {}
            }
        }
        self.sound.manager.set_muted(muted);
        let rain = if in_world {
            self.scene.rain_heard()
        } else {
            0.0
        };
        let records = &self.world.records;
        self.sound.rain(rain, now, &self.random, &|value| {
            records.data_id_by_enum(value, auberean_draw::rain::MAPPING)
        });
        self.sound
            .frame(now, &self.scene.seen, in_world, self.world, &self.random);
    }

    /// The window's title: the product's name, and the account the player logged in with after
    /// it (`Auberean | kara`), as Dereth's client names its window; the name alone before a login.
    /// The task bar's button is what tells several clients on one machine apart.
    #[must_use]
    pub fn title(&self) -> String {
        if self.account.is_empty() {
            "Auberean".to_owned()
        } else {
            format!("Auberean | {}", self.account)
        }
    }

    /// The player's health and vigor with their greatest, where the model has them.
    #[must_use]
    pub fn vitals(&self) -> Option<[f32; 4]> {
        let c = self.client.as_ref()?;
        let q = &c.model.player_object()?.qualities;
        let v = |id: u32| {
            #[allow(clippy::cast_precision_loss)]
            q.ints
                .get(&id)
                .map(|v| *v as f32)
                .or_else(|| q.floats.get(&id).copied())
        };
        Some([v(258)?, v(263)?, v(259)?, v(264)?])
    }

    /// The developer's status lines.
    #[must_use]
    pub fn status_lines(&self) -> Vec<String> {
        let s = &self.scene.status;
        if let Some(x) = &self.explore {
            let mut l = vec![format!(
                "EXPLORE  {} FPS  {} BATCHES  {}K+{}K VERTICES  MS {:.1}",
                self.frames.len(),
                s.batches,
                s.vertices.0 / 1000,
                s.vertices.1 / 1000,
                self.costs[2] * 1000.0
            )];
            l.extend(x.lines(self.host.now()).into_iter().take(1));
            return l;
        }
        let mut l = vec![format!(
            "{:?}  {} FPS  {} BATCHES  {}K+{}K VERTICES  MS {:.1} {:.1} {:.1}",
            self.phase(),
            self.frames.len(),
            s.batches,
            s.vertices.0 / 1000,
            s.vertices.1 / 1000,
            self.costs[0] * 1000.0,
            self.costs[1] * 1000.0,
            self.costs[2] * 1000.0
        )];
        if self.phase() == Phase::World {
            l.push(format!(
                "CELL {:08X}  ({:.1}, {:.1}, {:.1})  HEADING {:.0}",
                s.cell, s.origin.x, s.origin.y, s.origin.z, s.heading
            ));
            // Where the camera stands, kept off the world.
            let eye = self
                .scene
                .camera
                .last
                .map_or(dereth_primitives::Vec3::ZERO, |v| v.origin);
            let (look, tilt) = self
                .scene
                .seen
                .camera
                .map_or((0.0, 0.0), |c| (c.heading, c.pitch));
            l.push(format!(
                "SPEED {:.1}  {}  {} BODIES  CAMERA {:.1} M AT ({:.2}, {:.2}, {:.2}) LOOKING {:.1} {:.1}{}",
                s.speed,
                if s.contact { "GROUND" } else { "AIR" },
                s.others,
                self.scene.camera.distance,
                eye.x,
                eye.y,
                eye.z,
                look,
                tilt,
                if self.scene.camera.mouse_look {
                    "  MOUSE LOOK"
                } else {
                    ""
                }
            ));
            if let Some([h, hm, v, vm]) = self.vitals() {
                l.push(format!("HEALTH {h:.0}/{hm:.0}  VIGOR {v:.0}/{vm:.0}"));
            }
            let selected = self
                .client
                .as_ref()
                .and_then(|c| c.view().world.as_ref())
                .map_or(0, |w| w.selection.current);
            if selected != 0 {
                l.push(format!("SELECTED {selected:016X}"));
            }
            if !s.nearest.is_empty() {
                let near: Vec<String> = s
                    .nearest
                    .iter()
                    .map(|(id, d, b)| format!("{:X} {d:.0} M AT {b:.0}", id & 0xFFFF_FFFF))
                    .collect();
                l.push(format!("NEAR {}", near.join("  ")));
            }
        }
        if self.sound.has_device() {
            let (peak, blocks, silent) = self.sound.take_meter();
            let playing: Vec<String> = self
                .sound
                .manager
                .playing()
                .iter()
                .map(|(_, id, life)| format!("{id:08X} {life:?}"))
                .collect();
            l.push(format!(
                "SOUND PEAK {peak:.3}  BLOCKS {blocks} ({silent} SILENT)  PLAYING {}",
                playing.join(", ")
            ));
        }
        l.extend(self.heard.iter().map(|t| format!("> {t}")));
        l
    }
}
