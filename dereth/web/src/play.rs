//! The playable client: the client shell's application and front end -- the retail UI, the scene
//! drawing, the input pipeline -- on the browser's host ([`WebHost`]). The data files come in
//! already open, the graphics device is the one the page prepared (`dereth_render::wgpu`), the
//! window is the canvas, and the connection is the runtime's socket-free endpoint fed by the page's
//! WebSocket (a server's own endpoint, or `dereth-web-relay` on the player's machine).
//!
//! The page queues its events here in the shell's terms ([`HostEvent`]), calls
//! [`Play::frame`] once an animation frame, and carries [`Play::receive`] and
//! [`Play::take_messages`] to and from the WebSocket. Everything between is the client shell's.

use std::cell::Cell;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use dereth_client_runtime::app::Platform;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::net::ClientNetwork;
use dereth_dat::RetailDatStore;
use dereth_primitives::LocalTime;

use dereth_client_shell::platform::keys::MouseButton;

use crate::frame;
use crate::host::clock::{FramePaced, SystemClock};
use crate::host::dialog::SystemDialog;
use crate::host::window::{
    key_code_from_dom, key_from_key_code, CanvasSize, HostEvent, WebWindow, WindowEvents,
};
use crate::host::WebHost;

/// The client shell's application, on the browser's host.
pub type App = dereth_client_shell::app::App<WebHost>;

/// The server as the client addresses it. The WebSocket's far end alone knows where the server is
/// and sends only there, so the client needs its ports and not its address: a host name, which a
/// browser cannot look up, is given the loopback address with its port kept.
fn nominal_host(host: &str) -> String {
    let spec = dereth_client_net::socket::parse_host_spec(host, 9000);
    if spec.host.parse::<std::net::Ipv4Addr>().is_ok() {
        host.to_string()
    } else {
        format!("127.0.0.1:{}", spec.port)
    }
}

/// A cursor image for the page: RGBA rows and the point within it that is the pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub hot_x: u32,
    pub hot_y: u32,
}

/// The playable client, connected over the page's WebSocket.
pub struct Play {
    app: App,
    events: WindowEvents,
    size: CanvasSize,
    server: IpAddr,
    frames: u64,
    /// The cursor [`Play::cursor_change`] last returned.
    cursor_shown: Option<(dereth_primitives::DataId, i32, i32)>,
}

impl std::fmt::Debug for Play {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Play")
            .field("server", &self.server)
            .field("frames", &self.frames)
            .finish_non_exhaustive()
    }
}

impl Play {
    /// Bring the client up over `store` in a `width` by `height` canvas, on the device
    /// `dereth_render::wgpu::install` prepared, and start logging in to `host` as `account`.
    ///
    /// # Errors
    /// A startup step the client treats as fatal, no prepared device, or a host the connection
    /// refuses.
    pub fn new(
        store: Arc<RetailDatStore>,
        host: &str,
        account: &str,
        password: &str,
        sequence: u32,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let preferences_file = crate::settings::preferences_file();
        let mut cfg = Config {
            account: account.to_lowercase(),
            account_as_typed: account.to_string(),
            host: host.to_string(),
            // Never opened: the store is already open.
            dat_dir: PathBuf::from("dats-held-by-the-page"),
            // The WebSocket is the connection, attached below; bring-up must not open a socket.
            connect: false,
            headless: false,
            frames: None,
            width,
            height,
            preferences_file: preferences_file.clone(),
            ..Config::default()
        };
        // The saved profile, read where the host keeps the client's files, as the desktop client
        // reads it before anything else starts.
        cfg.apply_preferences(&dereth_client_runtime::config::Preferences::load(
            &preferences_file,
        ));
        // The world is built where the server puts the player, so it waits for world entry, as
        // the desktop client's does when it connects. The profile's scene policy, with the
        // browser's one difference: the WebSocket is not the runtime's socket connection, so the
        // connected client's streaming budget is asked for explicitly (it keeps the portal tunnel
        // animating). A device that cannot compose landscape surfaces or splat them (WebGL 2)
        // says so, and the scene composes them on the CPU.
        let mut scene = cfg.scene_config();
        scene.stream_budget = Some(std::time::Duration::from_millis(4));
        // The host's answers the runtime cannot compute itself, as the desktop bring-up installs
        // them.
        dereth_client_shell::hud::install_platform::<WebHost>();
        <WebHost as dereth_client_shell::platform::host::Host>::install_default_output();
        let size: CanvasSize = Rc::new(Cell::new((width, height)));
        let window_size = Rc::clone(&size);
        let events = WindowEvents::default();
        let mut app = App::bring_up_with_store(
            cfg,
            Some(store),
            Rc::clone(&events),
            move |_| {
                Ok(Platform {
                    window: Box::new(WebWindow::new(window_size)),
                    clock: Box::new(SystemClock::new()),
                    pacer: Box::new(FramePaced),
                    dialog: Box::new(SystemDialog),
                })
            },
            |_, client_w, client_h, cfg| {
                let mut renderer = dereth_client_shell::gpu::Renderer::new(
                    None, client_w, client_h,
                )
                .map_err(|e| dereth_client_runtime::app::StartupError::Device {
                    cause: format!("graphics engine: {e}"),
                })?;
                renderer.set_gamma(cfg.render.screen_brightness);
                renderer.set_presentation_sync(false, cfg.display.sync_to_refresh);
                tracing::info!("graphics backend on {}", renderer.adapter_name());
                Ok(Box::new(renderer))
            },
        )
        .map_err(|e| e.to_string())?;
        app.start_shell().map_err(|e| e.to_string())?;
        app.defer_static_scene(scene);
        let net = ClientNetwork::new(&nominal_host(host), 9000, account, password, sequence)
            .map_err(|e| e.to_string())?;
        let server = net.logon_addr().ip();
        app.attach_relay_network(net)
            .map_err(|_| "the client already has a connection".to_string())?;
        // The page has the focus when the client starts; a window that has not been told so is
        // minimised to the runtime, which paces it at ten frames a second.
        events.borrow_mut().push(HostEvent::Focused(true));
        Ok(Self {
            app,
            events,
            size,
            server,
            frames: 0,
            cursor_shown: None,
        })
    }

    fn net(&mut self) -> Option<&mut ClientNetwork> {
        self.app.replay_network_mut()
    }

    /// One message from the WebSocket, delivered at the application's current time.
    pub fn receive(&mut self, message: &[u8]) {
        let now = LocalTime(self.app.clock().local_time);
        let server = self.server;
        if let (Some((port, datagram)), Some(net)) = (frame::decode(message), self.net()) {
            net.feed(datagram, SocketAddr::new(server, port), now);
        }
    }

    /// One whole frame of the client, drawn. `false` when the client has shut itself down.
    pub fn frame(&mut self) -> bool {
        self.frames += 1;
        self.app.frame()
    }

    /// Everything the connection wants sent, framed for the WebSocket.
    pub fn take_messages(&mut self) -> Vec<Vec<u8>> {
        self.net().map_or_else(Vec::new, |net| {
            net.take_outgoing()
                .into_iter()
                .map(|(bytes, to)| frame::encode(to.port(), &bytes))
                .collect()
        })
    }

    /// The client's cleanup, in its order: the keymap and the preferences saved, the connection
    /// closed and the interface taken down. Log off first ([`Self::log_off`]) and send what that
    /// queues: the cleanup's own goodbye goes out with the connection it closes.
    pub fn shut_down(self) -> dereth_client_runtime::shutdown::CleanupLog {
        self.app.shutdown()
    }

    /// The session's will, framed for the WebSocket: the log-off's datagrams, for its far end to
    /// send if the page goes away before it can log off. The same until the connections change.
    pub fn will(&mut self) -> Vec<u8> {
        let goodbye = self.net().map(|n| n.goodbye()).unwrap_or_default();
        let datagrams: Vec<(u16, Vec<u8>)> = goodbye
            .into_iter()
            .map(|(bytes, to)| (to.port(), bytes))
            .collect();
        frame::encode_will(&datagrams)
    }

    /// Log off; the goodbye is in the next [`Self::take_messages`].
    pub fn log_off(&mut self) {
        if let Some(net) = self.net() {
            net.log_off_server();
        }
    }

    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The size the client draws at: the canvas's drawing buffer, which the client changes itself
    /// (to the display preference on entering the game, and back to 800 by 600 on leaving it).
    /// The page scales the canvas to fit and maps the pointer by it.
    #[must_use]
    pub fn client_size(&self) -> (u32, u32) {
        self.app.present.size()
    }

    /// Queue one host event for the next frame.
    pub fn event(&mut self, event: HostEvent) {
        if let HostEvent::Resized { width, height } = event {
            self.size.set((width, height));
        }
        self.events.borrow_mut().push(event);
    }

    /// The pointer moved to `(x, y)` in canvas pixels.
    pub fn pointer_move(&mut self, x: f64, y: f64) {
        self.event(HostEvent::CursorMoved { x, y });
    }

    /// Pointer button `button` (the page's numbering: 0 left, 1 middle, 2 right, 3 back,
    /// 4 forward) went down or up.
    pub fn pointer_button(&mut self, button: u16, pressed: bool) {
        let button = match button {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            3 => MouseButton::Back,
            4 => MouseButton::Forward,
            n => MouseButton::Other(n),
        };
        self.event(HostEvent::MouseInput { button, pressed });
    }

    /// The wheel turned `notches` detents, away from the player positive.
    pub fn wheel(&mut self, notches: f32) {
        self.event(HostEvent::MouseWheel { notches });
    }

    /// The key the page's `KeyboardEvent.code` names went down or up, typing `text`. `false` for a
    /// key the client's tables do not know, which the page may then keep for itself.
    pub fn key(&mut self, code: &str, pressed: bool, text: Option<String>) -> bool {
        let Some(key) = key_code_from_dom(code).and_then(key_from_key_code) else {
            return false;
        };
        self.event(HostEvent::KeyboardInput { key, pressed, text });
        true
    }

    /// Alt went down or up.
    pub fn alt(&mut self, held: bool) {
        self.event(HostEvent::ModifiersChanged { alt: held });
    }

    /// The canvas gained or lost the focus.
    pub fn focus(&mut self, gained: bool) {
        self.event(HostEvent::Focused(gained));
    }

    /// The canvas's drawing buffer is now `width` by `height`.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.event(HostEvent::Resized { width, height });
    }

    /// The cursor the interface last set, when it differs from the last one this returned: its
    /// image as RGBA rows and its hotspot, for the page to show over the canvas.
    pub fn cursor_change(&mut self) -> Option<CursorImage> {
        let (did, hot_x, hot_y) = self.app.ui()?.ui.last_cursor?;
        if self.cursor_shown == Some((did, hot_x, hot_y)) {
            return None;
        }
        self.cursor_shown = Some((did, hot_x, hot_y));
        let image = dereth_scene::textures::TextureStore::new(&self.app.store)
            .bgra8(did)
            .ok()?;
        Some(CursorImage {
            width: image.width,
            height: image.height,
            rgba: image
                .pixels
                .iter()
                .flat_map(|[b, g, r, a]| [*r, *g, *b, *a])
                .collect(),
            hot_x: u32::try_from(hot_x).unwrap_or(0),
            hot_y: u32::try_from(hot_y).unwrap_or(0),
        })
    }

    /// The application, for a caller that reads its state.
    #[must_use]
    pub fn app(&self) -> &App {
        &self.app
    }

    /// The application, mutably.
    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How long the live tests let the character screen open before they click on it. Its opening
    /// runs on the clock, not on frames, so a count of frames is too short on a fast machine.
    const SPLASH: std::time::Duration = std::time::Duration::from_secs(15);

    /// Frames, `step` each, for [`SPLASH`] of wall-clock time.
    fn splash(step: &mut dyn FnMut()) {
        let until = std::time::Instant::now() + SPLASH;
        while std::time::Instant::now() < until {
            step();
        }
    }

    /// An address is kept as given; a name keeps its port and is addressed at loopback, and a
    /// name without a port is at the default one.
    #[test]
    fn a_named_server_keeps_its_port_and_not_its_name() {
        assert_eq!(nominal_host("10.0.0.5:9010"), "10.0.0.5:9010");
        assert_eq!(nominal_host("play.example.org:9010"), "127.0.0.1:9010");
        assert_eq!(nominal_host("play.example.org"), "127.0.0.1:9000");
    }

    /// The client, brought up over the retail data on an off-screen device with no server to
    /// answer, draws the retail interface's first screen, the connecting screen: its logo and
    /// progress bars cover a tenth of the frame or more, the rest being the black it is cleared
    /// to.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats and needs a GPU: --features retail-dats"
    )]
    fn the_retail_interface_draws_on_the_web_device() {
        let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let (width, height) = (800, 600);
        match pollster::block_on(dereth_render::wgpu::prepare_offscreen(width, height, false)) {
            Ok(p) => dereth_render::wgpu::install(p),
            Err(e) => {
                eprintln!("skipped: {e}");
                return;
            }
        }
        let mut play = Play::new(store, "127.0.0.1", "account", "password", 1, width, height)
            .expect("bring-up");
        for _ in 0..10 {
            assert!(play.frame(), "the client stopped");
        }
        let renderer = play.app_mut().renderer_mut();
        if let Ok(path) = std::env::var("DERETH_WEB_DUMP") {
            renderer
                .capture_png(std::path::Path::new(&path))
                .expect("a capture written");
        }
        let (w, h, bgra) = renderer.capture_bgra().expect("a capture");
        let drawn = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[..3] != [0, 0, 0])
            .count();
        assert!(
            drawn > (w * h / 10) as usize,
            "{drawn} of {} pixels drawn",
            w * h
        );
    }

    /// Against a live server on this machine (`DERETH_TEST_LIVE_SERVER=127.0.0.1:9000`, account
    /// `DERETH_TEST_LIVE_ACCOUNT`, its password `DERETH_TEST_LIVE_PASSWORD` or else the account
    /// name): the client logs in through the WebSocket's framing and its interface reaches the
    /// character screen, whose cursor comes out as an image once. With `DERETH_WEB_DUMP_DIR` set,
    /// each screen is written there.
    #[test]
    #[ignore = "needs a live server: DERETH_TEST_LIVE_SERVER and DERETH_TEST_LIVE_ACCOUNT"]
    fn a_live_login_reaches_the_character_screen() {
        use dereth_client_net::client_session::SessionState;

        let host = std::env::var("DERETH_TEST_LIVE_SERVER").expect("DERETH_TEST_LIVE_SERVER");
        let account = std::env::var("DERETH_TEST_LIVE_ACCOUNT").expect("DERETH_TEST_LIVE_ACCOUNT");
        let password =
            std::env::var("DERETH_TEST_LIVE_PASSWORD").unwrap_or_else(|_| account.clone());
        let dump = std::env::var("DERETH_WEB_DUMP_DIR").ok();
        let store = Arc::new(dereth_dat::testing::open_store().expect("the dats"));
        let (width, height) = (800, 600);
        match pollster::block_on(dereth_render::wgpu::prepare_offscreen(width, height, false)) {
            Ok(p) => dereth_render::wgpu::install(p),
            Err(e) => {
                eprintln!("skipped: {e}");
                return;
            }
        }
        let server: SocketAddr = host.parse().expect("an address");
        let mut play =
            Play::new(store, &host, &account, &password, 1, width, height).expect("bring-up");
        let udp = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind");
        udp.set_nonblocking(true).expect("non-blocking");
        let mut buf = vec![0u8; 65_536];
        let mut step = |play: &mut Play| {
            while let Ok((n, from)) = udp.recv_from(&mut buf) {
                play.receive(&frame::encode(from.port(), &buf[..n]));
            }
            assert!(play.frame(), "the client stopped");
            for m in play.take_messages() {
                let (port, d) = frame::decode(&m).expect("framed");
                udp.send_to(d, SocketAddr::new(server.ip(), port))
                    .expect("send");
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        };
        let capture = |play: &mut Play, name: &str| {
            if let Some(dir) = &dump {
                let path = std::path::Path::new(dir).join(format!("{name}.png"));
                play.app_mut()
                    .renderer_mut()
                    .capture_png(&path)
                    .expect("a capture");
            }
        };
        step(&mut play);
        capture(&mut play, "connecting");
        let state = |play: &mut Play| play.net().map(|n| n.session_state());
        let mut frames = 0;
        while state(&mut play) != Some(SessionState::CharacterSelect) && frames < 600 {
            step(&mut play);
            frames += 1;
        }
        assert_eq!(state(&mut play), Some(SessionState::CharacterSelect));
        for i in 0..8 {
            for _ in 0..60 {
                step(&mut play);
            }
            capture(&mut play, &format!("characters-{i}"));
        }
        let cursor = play.cursor_change().expect("the interface set a cursor");
        assert!(cursor.width > 0 && cursor.height > 0);
        assert_eq!(
            cursor.rgba.len(),
            (cursor.width * cursor.height * 4) as usize
        );
        assert_eq!(play.cursor_change(), None, "reported once until it changes");
        let modes = play.app().ui().map(|u| u.transitions().to_vec());
        play.log_off();
        step(&mut play);
        eprintln!("character screen after {frames} frames; interface modes {modes:?}");
    }

    /// Against a live server on this machine, with an account that has a character (the
    /// variables of [`a_live_login_reaches_the_character_screen`]): the character is picked and
    /// entered with the pointer, the body walks with the keymap's forward key, and a line typed
    /// into chat is sent with Enter. With `DERETH_WEB_DUMP_DIR` set, each step's frame is
    /// written there.
    #[test]
    #[ignore = "needs a live server: DERETH_TEST_LIVE_SERVER and DERETH_TEST_LIVE_ACCOUNT"]
    fn a_live_session_enters_walks_and_talks_through_the_interface() {
        use dereth_client_net::client_session::SessionState;

        let host = std::env::var("DERETH_TEST_LIVE_SERVER").expect("DERETH_TEST_LIVE_SERVER");
        let account = std::env::var("DERETH_TEST_LIVE_ACCOUNT").expect("DERETH_TEST_LIVE_ACCOUNT");
        let password =
            std::env::var("DERETH_TEST_LIVE_PASSWORD").unwrap_or_else(|_| account.clone());
        let dump = std::env::var("DERETH_WEB_DUMP_DIR").ok();
        let store = Arc::new(dereth_dat::testing::open_store().expect("the dats"));
        let (width, height) = (800, 600);
        match pollster::block_on(dereth_render::wgpu::prepare_offscreen(width, height, false)) {
            Ok(p) => dereth_render::wgpu::install(p),
            Err(e) => {
                eprintln!("skipped: {e}");
                return;
            }
        }
        let server: SocketAddr = host.parse().expect("an address");
        let mut play =
            Play::new(store, &host, &account, &password, 1, width, height).expect("bring-up");
        let udp = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind");
        udp.set_nonblocking(true).expect("non-blocking");
        let mut buf = vec![0u8; 65_536];
        let mut run = |play: &mut Play, frames: u32| {
            for _ in 0..frames {
                while let Ok((n, from)) = udp.recv_from(&mut buf) {
                    play.receive(&frame::encode(from.port(), &buf[..n]));
                }
                assert!(play.frame(), "the client stopped");
                for m in play.take_messages() {
                    let (port, d) = frame::decode(&m).expect("framed");
                    udp.send_to(d, SocketAddr::new(server.ip(), port))
                        .expect("send");
                }
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        };
        let capture = |play: &mut Play, name: &str| {
            if let Some(dir) = &dump {
                let path = std::path::Path::new(dir).join(format!("{name}.png"));
                play.app_mut()
                    .renderer_mut()
                    .capture_png(&path)
                    .expect("a capture");
            }
        };
        let click = |play: &mut Play, x: f64, y: f64| {
            play.pointer_move(x, y);
            play.pointer_button(0, true);
            play.pointer_button(0, false);
        };
        let state = |play: &mut Play| play.net().map(|n| n.session_state());
        let mut frames = 0;
        while state(&mut play) != Some(SessionState::CharacterSelect) && frames < 600 {
            run(&mut play, 1);
            frames += 1;
        }
        splash(&mut || run(&mut play, 1));
        capture(&mut play, "select");
        click(&mut play, 120.0, 222.0);
        run(&mut play, 10);
        click(&mut play, 345.0, 392.0);
        frames = 0;
        while state(&mut play) != Some(SessionState::Playable) && frames < 900 {
            run(&mut play, 1);
            frames += 1;
        }
        assert_eq!(state(&mut play), Some(SessionState::Playable));
        run(&mut play, 360);
        capture(&mut play, "arrived");
        let before = play
            .app()
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(|c| c.position());
        assert!(play.key("KeyW", true, Some("w".into())));
        run(&mut play, 120);
        play.key("KeyW", false, None);
        run(&mut play, 30);
        capture(&mut play, "walked");
        let after = play
            .app()
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(|c| c.position());
        // Each key's release comes a frame after its press, as a page's events can.
        let press =
            |play: &mut Play, run: &mut dyn FnMut(&mut Play, u32), code: &str, text: &str| {
                play.key(code, true, Some(text.to_string()));
                run(play, 1);
                play.key(code, false, None);
            };
        press(&mut play, &mut run, "Enter", "\r");
        run(&mut play, 10);
        for ch in "hello".chars() {
            let code = format!("Key{}", ch.to_ascii_uppercase());
            press(&mut play, &mut run, &code, &ch.to_string());
            run(&mut play, 2);
        }
        run(&mut play, 10);
        capture(&mut play, "typing");
        press(&mut play, &mut run, "Enter", "\r");
        run(&mut play, 60);
        capture(&mut play, "said");
        play.log_off();
        run(&mut play, 2);
        eprintln!("walked from {before:?} to {after:?}");
        assert_ne!(before, after, "the body walked");
    }

    /// Against a live server on this machine (the variables of
    /// [`a_live_login_reaches_the_character_screen`]): the character screen's Exit, confirmed,
    /// ends the client -- the epilogue screen logs off and a frame reports the end -- so a page
    /// can offer to play again.
    #[test]
    #[ignore = "needs a live server: DERETH_TEST_LIVE_SERVER and DERETH_TEST_LIVE_ACCOUNT"]
    fn a_live_exit_from_the_character_screen_ends_the_client() {
        use dereth_client_net::client_session::SessionState;

        let host = std::env::var("DERETH_TEST_LIVE_SERVER").expect("DERETH_TEST_LIVE_SERVER");
        let account = std::env::var("DERETH_TEST_LIVE_ACCOUNT").expect("DERETH_TEST_LIVE_ACCOUNT");
        let password =
            std::env::var("DERETH_TEST_LIVE_PASSWORD").unwrap_or_else(|_| account.clone());
        let store = Arc::new(dereth_dat::testing::open_store().expect("the dats"));
        match pollster::block_on(dereth_render::wgpu::prepare_offscreen(800, 600, false)) {
            Ok(p) => dereth_render::wgpu::install(p),
            Err(e) => {
                eprintln!("skipped: {e}");
                return;
            }
        }
        let server: SocketAddr = host.parse().expect("an address");
        let mut play = Play::new(store, &host, &account, &password, 1, 800, 600).expect("bring-up");
        let udp = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind");
        udp.set_nonblocking(true).expect("non-blocking");
        let mut buf = vec![0u8; 65_536];
        let mut step = |play: &mut Play| -> bool {
            while let Ok((n, from)) = udp.recv_from(&mut buf) {
                play.receive(&frame::encode(from.port(), &buf[..n]));
            }
            let alive = play.frame();
            for m in play.take_messages() {
                let (port, d) = frame::decode(&m).expect("framed");
                udp.send_to(d, SocketAddr::new(server.ip(), port))
                    .expect("send");
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
            alive
        };
        let mut frames = 0;
        while play.net().map(|n| n.session_state()) != Some(SessionState::CharacterSelect)
            && frames < 600
        {
            step(&mut play);
            frames += 1;
        }
        splash(&mut || {
            step(&mut play);
        });
        let mut click = |play: &mut Play, x: f64, y: f64| {
            play.pointer_move(x, y);
            play.pointer_button(0, true);
            play.pointer_button(0, false);
            for _ in 0..30 {
                step(play);
            }
        };
        // Exit, then Yes to "Are you sure you want to leave?".
        click(&mut play, 700.0, 572.0);
        click(&mut play, 320.0, 326.0);
        let mut ended_after = None;
        for n in 0..1200 {
            if !step(&mut play) {
                ended_after = Some(n);
                break;
            }
        }
        if let Ok(dir) = std::env::var("DERETH_WEB_DUMP_DIR") {
            let path = std::path::Path::new(&dir).join("after-exit.png");
            let _ = play.app_mut().renderer_mut().capture_png(&path);
        }
        // The log-off a page sends before its cleanup, so the server lets the account go.
        play.log_off();
        for m in play.take_messages() {
            let (port, d) = frame::decode(&m).expect("framed");
            udp.send_to(d, SocketAddr::new(server.ip(), port))
                .expect("send");
        }
        eprintln!("ended after {ended_after:?} frames");
        assert!(ended_after.is_some(), "the client ended");
    }
}
