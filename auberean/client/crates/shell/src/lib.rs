//! The application over a host, with no platform: the device's events routed to the game's
//! interface and then to the world's controls, the client's session carried over the host's
//! network, the world moved and framed, the settings read and kept, and what each frame shows.
//!
//! **Depends on** the client (`auberean-client-sdk`), the world a window shows (`auberean-scene`),
//! drawing's frame (`auberean-draw`), the world (`auberean-world`, `auberean-physics`), the key
//! maps' records and files (`auberean-assets`, `auberean-dat`), the game's sound
//! (`auberean-audio`) and the sound device's seam (`dereth-audio`), `auberean-primitives` and
//! `dereth-primitives`. **Used by** the desktop client (`auberean-client`), which is its host, and
//! by the game's interface, which plugs in through [`interface::Interface`].
//!
//! A [`Shell`] is generic over its [`Host`] (the platform: clock, network, pointer, title,
//! settings, the player's files and the sound device) and holds one [`interface::Interface`]. Each event goes to the
//! interface first and, if the interface does not take it, to the world's controls
//! ([`controls`]), which turn it into actions through the key maps ([`keys`]): the player's,
//! read from their files at startup and written when the key map screen applies new ones. Each frame
//! ([`Shell::frame`]):
//!
//! 1. the datagrams that arrived go to the client;
//! 2. the interface sees the game and asks for things: connecting, the client's actions,
//!    disconnecting, quitting ([`interface::Request`]);
//! 3. in the world, the controls' intent moves the body and the camera ([`auberean_scene`]), and
//!    selecting and using objects become the server calls the client makes ([`calls`]);
//! 4. the client runs its frame, and what it sends goes out through the host;
//! 5. the sounds the interface asked for and the world's effects played are played, with the
//!    listener at the camera ([`sound`]);
//! 6. the frame to draw is the scene's, with the interface's overlay (and the developer's status
//!    overlay when asked for, [`dev`]) over it.
//!
//! **Must never** name a platform (a window system, the operating system, a sound device): the
//! host brings those (`cargo xtask seams`).

pub mod calls;
pub mod command_line;
pub mod controls;
pub mod dev;
pub mod host;
pub mod input;
pub mod input_manager;
pub mod interface;
pub mod keys;
pub mod preferences;
pub mod profile;
pub mod settings;
pub mod sound;

use std::collections::VecDeque;

use core::net::SocketAddr;

use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::runtime::SessionEnd;
use auberean_client_sdk::{Client, ClientData, ClientSettings};
use auberean_draw::scene::Frame;
use auberean_scene::{Scene, WorldData};

pub use auberean_scene::view::ViewFrame;
pub use host::{FriendsNews, Host};
use input::Event;
use interface::{CameraRequest, Game, Heard, Interface, Notice, Overlay, Phase, Request, Shown};
pub use settings::{Presentation, Settings};

/// The game as the interface sees it, from the shell's fields one by one (so the interface itself
/// may be borrowed beside it).
macro_rules! game {
    ($s:ident, $phase:expr) => {
        Game {
            phase: $phase,
            client: $s.client.as_ref(),
            world: $s.world,
            size: $s.size,
            now: $s.host.now(),
            status: &$s.scene.status,
            selected: $s.selected,
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
        }
    };
}

/// How many lines heard the status overlay keeps.
const HEARD: usize = 6;

/// What a frame shows.
#[derive(Debug)]
pub struct Output<'a> {
    /// The world, once the player is in it and portal space leaves it in sight.
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
    /// The program should close.
    pub quit: bool,
}

/// The application.
pub struct Shell<'w, H: Host> {
    pub host: H,
    world: &'w WorldData,
    data: ClientData,
    /// The session, while there is one.
    pub client: Option<Client>,
    pub scene: Scene<'w>,
    interface: Box<dyn Interface + 'w>,
    pub controls: controls::Controls,
    /// What decides which events reach the interface and the world's controls.
    input: input_manager::InputManager,
    /// The key maps and what their actions do.
    pub keys: keys::Keys,
    pub settings: Settings,
    /// The window's drawing area, in pixels.
    pub size: (u32, u32),
    /// The object selected (0 for none).
    pub selected: u64,
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
    last: f64,
    frames: VecDeque<f64>,
    heard: VecDeque<String>,
    /// What was heard since the last frame.
    fresh: Vec<Heard>,
    /// The lines the scripts wrote for the chat windows since the last frame.
    shown: Vec<Shown>,
    /// The notices the scripts sent the panels since the last frame.
    notices: Vec<Notice>,
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
    /// The game's sound.
    pub sound: sound::Sound,
}

/// Where a session finds the friends service.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FriendsService {
    /// At the login's own host, on the login's port number, with a ticket made from the account
    /// and the password the player logs in with.
    #[default]
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
    /// None.
    Off,
}

/// The original friends service's port, which a service named without one is on.
const FRIENDS_PORT: u16 = 2310;

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
    /// over the world. Reads the settings the host kept; the sound starts, then the world and the
    /// interface, and the key maps and the action interpreter are ready.
    // D2-00141, D2-00027
    pub fn new(
        mut host: H,
        world: &'w WorldData,
        data: ClientData,
        mut interface: Box<dyn Interface + 'w>,
    ) -> Self {
        let settings = host
            .load_settings()
            .map(|t| Settings::read(&t))
            .unwrap_or_default();
        let random = auberean_primitives::random::Random::new();
        // The graphics preferences kept, else a fresh install's: the interface's options start
        // from them and the world is drawn by them from its first frame.
        let graphics = settings.starting_graphics();
        let mut scene = Scene::new(world, &graphics);
        scene.set_random(&random);
        interface.set_random(&random);
        scene.camera = auberean_scene::CameraControl::new(settings.camera);
        interface.set_preferences(&settings, &world.records);
        interface.set_graphics(&graphics);
        // The key maps: the base map, then the player's from their files (the data's Defaults where
        // a file does not load).
        let keys = keys::Keys::start(
            keys::KeyRecords::read(&world.records),
            settings.key_files.clone(),
            &mut crate::host::HostFiles(&mut host),
        );
        let last = host.now();
        let sound = sound::Sound::start(
            &mut host,
            data.files.files.clone(),
            auberean_scene::BLOCK_LENGTH,
        );
        Self {
            size: settings.window,
            host,
            world,
            data,
            client: None,
            scene,
            interface,
            controls: controls::Controls::default(),
            input: input_manager::InputManager::default(),
            keys,
            settings,
            selected: 0,
            dev_overlay: false,
            entering: false,
            refusals: 0,
            stopped: None,
            ended: None,
            costs: [0.0; 9],
            quit: false,
            last,
            frames: VecDeque::new(),
            heard: VecDeque::new(),
            fresh: Vec::new(),
            shown: Vec::new(),
            notices: Vec::new(),
            overlay: Overlay::default(),
            title_at: f64::NEG_INFINITY,
            account: String::new(),
            world_pick: None,
            random,
            friends: FriendsService::default(),
            sound,
        }
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

    /// One event from the host: the interface first, then the world's controls.
    pub fn event(&mut self, e: &Event) {
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
            _ => {}
        }
        let phase = self.phase();
        let game = game!(self, phase);
        if !self.interface.event(e, &game) {
            self.controls.event(e, &self.keys);
        }
        if let Some((x, y)) = self.input.take_missed() {
            self.event(&Event::Pointer { x, y });
        }
    }

    /// The sizes the host's display can take, in pixels, from a host that lets the player choose
    /// how the game is shown: the options window's rendering section lists them. A host that
    /// does not say leaves that section empty.
    pub fn set_display_modes(&mut self, modes: &[(u32, u32)]) {
        self.interface.set_presentation(modes, &self.settings);
    }

    /// Full screen, or back to the window.
    // D2-08327
    pub fn toggle_full_screen(&mut self) {
        let mut requests = Vec::new();
        self.interface.toggle_full_screen(&mut requests);
        for r in requests {
            self.request(r);
        }
    }

    fn request(&mut self, r: Request) {
        match r {
            Request::Connect {
                server,
                account,
                password,
            } => self.connect(server, &account, &password),
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
            Request::ShowWorld(on) => self.scene.show_world(on),
            Request::PortalScenePlaying(on) => self.scene.portal_scene_playing = on,
            Request::Camera(c) => self.camera(c),
            Request::PickWorld { x, y } => {
                self.world_pick = Some(self.scene.pick_world((x, y), self.size));
            }
            Request::Graphics(g) => {
                self.scene.init_preferences(&g);
                self.settings.graphics = Some(g);
            }
            Request::FxChannels(c) => self.scene.set_fx_channels(c),
            Request::Presentation(p) => {
                self.settings.set_presentation(&p);
                self.host.set_presentation(&p);
            }
            Request::KeepPreferences(writes) => {
                for (section, key, value) in &writes {
                    self.settings.keep(section, key, value);
                }
                self.host.save_settings(&self.settings.write());
            }
            Request::Move { action, extent } => {
                self.controls.action_handler(
                    &self.keys,
                    action,
                    extent,
                    controls::INTERFACE,
                    action,
                );
            }
            Request::Sound(r) => self.sound.request(r),
            Request::InputMethod(area) => self.host.set_input_method(area),
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
            CameraRequest::Options(o) => {
                cam.options = o;
                cam.apply_options();
                self.settings.camera = o;
            }
        }
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
        if self.client.take().is_some_and(|c| c.friends().is_some()) {
            self.host.friends_close();
        }
        self.entering = false;
    }

    /// The session ended under the player (the login failed, or the server is gone): the
    /// session ends where it is, and the interface is told how.
    fn end(&mut self, end: SessionEnd) {
        tracing::info!(?end, "the session ended");
        self.ended = Some(end);
        if self.client.take().is_some_and(|c| c.friends().is_some()) {
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
        client.set_random(&self.random);
        let local = self.host.local_system();
        client.with_host(|h| h.set_local(local));
        client.observe.extend(HEARD_CALLS);
        if self.client.as_ref().is_some_and(|c| c.friends().is_some()) {
            self.host.friends_close();
        }
        if let Some(options) = self.friends_options(server, account, password) {
            let link = auberean_client_sdk::net::friends::FriendsLink::new(FRIENDS_SOFTWARE, None);
            client.use_friends(auberean_client_sdk::runtime::friends::Friends::new(
                Box::new(link),
                &options,
            ));
        }
        self.client = Some(client);
        self.entering = false;
    }

    /// The player leaves the server: a goodbye on every connection, and the session goes.
    // D2-00099
    fn disconnect(&mut self) {
        if let Some(c) = &mut self.client {
            c.disconnect();
            while let Some((to, bytes)) = c.poll_transmit() {
                self.host.send(to, &bytes);
            }
        }
        if self.client.take().is_some_and(|c| c.friends().is_some()) {
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

    fn command(&mut self, cmd: controls::Command) {
        let Some(c) = &mut self.client else { return };
        // With the player's scripts, the interface's own way: the scripts make the calls; without
        // them, the calls themselves.
        let scripted = c.scripts.is_some();
        let select = |id: u64| {
            if scripted {
                auberean_client_sdk::Action::Select { target: id }
            } else {
                calls::select(id)
            }
        };
        match cmd {
            controls::Command::SelectNone => {
                if self.selected != 0 {
                    self.selected = 0;
                    c.act(select(0));
                }
            }
            controls::Command::SelectNext => {
                let at = |o: &auberean_client_sdk::model::Object| {
                    o.place.map(|p| {
                        #[allow(clippy::cast_precision_loss)]
                        let g = auberean_primitives::Vec3::new(
                            (p.cell >> 24) as f32 * 160.0 + p.origin.x,
                            ((p.cell >> 16) & 0xFF) as f32 * 160.0 + p.origin.y,
                            p.origin.z,
                        );
                        g
                    })
                };
                let me = c.model.player_object().and_then(at);
                let Some(me) = me else { return };
                let mut near: Vec<(f32, u64)> = c
                    .model
                    .objects()
                    .filter(|o| Some(o.id) != c.model.player && o.in_world)
                    .filter_map(|o| {
                        let p = at(o)?;
                        let d =
                            ((p.x - me.x).powi(2) + (p.y - me.y).powi(2) + (p.z - me.z).powi(2))
                                .sqrt();
                        (d < 60.0).then_some((d, o.id))
                    })
                    .collect();
                near.sort_by(|a, b| a.0.total_cmp(&b.0));
                let next = near
                    .iter()
                    .position(|(_, id)| *id == self.selected)
                    .map_or(0, |i| i + 1);
                if let Some((_, id)) = near.get(next).or_else(|| near.first()) {
                    self.selected = *id;
                    c.act(select(*id));
                }
            }
            controls::Command::Use => {
                if self.selected != 0 {
                    c.act(if scripted {
                        auberean_client_sdk::Action::UseSelected
                    } else {
                        calls::use_object(self.selected)
                    });
                }
            }
        }
    }

    fn hear(&mut self) {
        self.fresh.clear();
        self.shown.clear();
        self.notices.clear();
        let Some(c) = &mut self.client else { return };
        self.notices
            .extend(std::mem::take(&mut c.notices).into_iter().map(|n| Notice {
                native: n.native,
                about: n.about,
                words: n.words,
                objects: n.objects,
                requests: n.requests,
                effect_lines: n.effect_lines,
                listed: n.listed,
            }));
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
            if let Some(t) = calls::text_of(&values) {
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

    /// One frame: the network, the interface, the world, the client; the world's frame, what is
    /// placed over it; then what to show.
    pub fn frame(&mut self) -> Output<'_> {
        let now = self.host.now();
        #[allow(clippy::cast_possible_truncation)]
        let dt = (now - self.last).clamp(0.0, 0.25) as f32;
        self.last = now;
        self.frames.push_back(now);
        while self.frames.front().is_some_and(|t| now - t > 1.0) {
            self.frames.pop_front();
        }
        let local = LocalTime(1.0 + now);
        let stream_now = core::time::Duration::from_secs_f64(local.0);
        let began = self.host.now();
        if let Some(c) = &mut self.client {
            while let Some((from, bytes)) = self.host.receive() {
                c.handle_datagram(from, &bytes, local);
            }
        }
        self.carry_friends(stream_now);
        self.hear();
        if let Some(c) = &mut self.client {
            self.interface.prepare(c);
        }
        self.costs[3] = self.host.now() - began;
        let began = self.host.now();
        // The interface's requests.
        let mut requests = Vec::new();
        let phase = self.phase();
        let game = game!(self, phase);
        self.interface.update(&game, &mut requests);
        self.world_pick = None;
        for r in requests {
            self.request(r);
        }
        self.costs[4] = self.host.now() - began;
        // The world's controls.
        let phase = self.phase();
        self.controls
            .set_text_mode(&self.keys, self.interface.text_mode());
        self.controls.steers = self.scene.camera.options.mouse_look_steers;
        let intent = self.controls.intent(&self.keys);
        let commands = self.controls.take_commands();
        if phase == Phase::World {
            self.entering = false;
            for cmd in commands {
                self.command(cmd);
            }
        }
        if let Some(c) = &mut self.client {
            let t0 = self.host.now();
            self.scene.entering = self.entering;
            self.scene.update(c, &intent, now, dt);
            let t1 = self.host.now();
            c.frame(local);
            self.costs[0] = t1 - t0;
            self.costs[1] = self.host.now() - t1;
            while let Some((to, bytes)) = c.poll_transmit() {
                self.host.send(to, &bytes);
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
        if let Some(end) = self.client.as_ref().and_then(Client::session_end) {
            self.end(end);
        }
        self.carry_friends(stream_now);
        let phase_now = self.phase();
        let free = self.interface.wants_pointer(&game!(self, phase_now));
        let pointer = self.scene.camera.mouse_look && !free;
        self.controls.mouse_look = self.scene.camera.mouse_look;
        if let Some(at) = self.input.set_mouse_look_mode(pointer, self.size) {
            self.host.hold_pointer(pointer);
            self.controls.mouse_look_switched(&self.keys);
            if let Some((x, y)) = at {
                // Past the gate: the pointer is held now, and it is the shell that moved it.
                self.scene.pointer = Some(((x, y), self.size));
                let game = game!(self, phase_now);
                self.interface.event(&Event::Pointer { x, y }, &game);
            }
        }
        if now - self.title_at > 0.5 {
            self.title_at = now;
            let t = self.title();
            self.host.set_title(&t);
        }
        // The world's frame, then what is placed over the world from its camera.
        let t = self.host.now();
        self.scene.build_frames(now, phase == Phase::World);
        self.costs[2] = self.host.now() - t;
        let t = self.host.now();
        let mut requests = Vec::new();
        let phase_now = self.phase();
        let game = game!(self, phase_now);
        self.interface.after_world(&game, &mut requests);
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
        overlay.scenes.clear();
        let phase_now = self.phase();
        let game = game!(self, phase_now);
        self.interface.draw(&game, &mut overlay);
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
            if let Some(c) = &mut self.client {
                c.disconnect();
                c.end_friends();
                while let Some((to, bytes)) = c.poll_transmit() {
                    self.host.send(to, &bytes);
                }
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
            quit,
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
            if self.selected != 0 {
                l.push(format!("SELECTED {:016X}", self.selected));
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
