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

/// The era a server's status names (`"infiltration"`, `"eor"`, ...), or `None` for an empty or
/// unknown name, which leaves the client to read the era from the data files.
#[must_use]
pub fn announced_era(name: &str) -> Option<dereth_primitives::EraId> {
    let era = dereth_primitives::EraId::parse(name.trim());
    if era.is_none() && !name.trim().is_empty() {
        tracing::warn!("the server names an era this client does not know: {name:?}");
    }
    era
}

/// The systems a server's status lists (`name=true,...`, as `--era-features` reads them); empty,
/// malformed or naming no system this client knows, the era's own table stands.
#[must_use]
pub fn announced_features(text: &str) -> dereth_primitives::EraFeatureOverrides {
    match dereth_primitives::EraFeatureOverrides::parse(text) {
        Ok((features, unknown)) => {
            if !unknown.is_empty() {
                tracing::warn!("the server names systems this client does not know: {unknown:?}");
            }
            features
        }
        Err(e) => {
            tracing::warn!("the server's systems do not read: {e}");
            dereth_primitives::EraFeatureOverrides::default()
        }
    }
}

impl Play {
    /// Bring the client up over `store` in a `width` by `height` canvas, on the device
    /// `dereth_render::wgpu::install` prepared, and start logging in to `host` as `account`.
    /// `era` is the era the server's status announces, as the launcher passes the desktop client
    /// `--era`; without one the client reads it from the data files. `era_features` are the
    /// systems the status lists, as the launcher passes `--era-features`.
    ///
    /// # Errors
    /// A startup step the client treats as fatal, no prepared device, or a host the connection
    /// refuses.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        store: Arc<RetailDatStore>,
        host: &str,
        account: &str,
        password: &str,
        sequence: u32,
        width: u32,
        height: u32,
        era: Option<dereth_primitives::EraId>,
        era_features: dereth_primitives::EraFeatureOverrides,
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
            era,
            era_features,
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

    /// The era a server's status names is the one the client plays; an empty or unknown name
    /// leaves it to the data files.
    #[test]
    fn the_era_a_servers_status_names_is_the_one_the_client_plays() {
        use dereth_primitives::EraId;
        assert_eq!(announced_era("infiltration"), Some(EraId::Infiltration));
        assert_eq!(announced_era("Infiltration"), Some(EraId::Infiltration));
        let f = announced_features("trade=false,later_system=true");
        assert_eq!((f.get("trade"), f.get("chess")), (Some(false), None));
        assert!(
            announced_features("trade").is_empty(),
            "malformed: the era's table"
        );
        assert_eq!(announced_era(""), None);
        assert_eq!(announced_era("tod"), None);
    }

    /// An address is kept as given; a name keeps its port and is addressed at loopback, and a
    /// name without a port is at the default one.
    #[test]
    fn a_named_server_keeps_its_port_and_not_its_name() {
        assert_eq!(nominal_host("10.0.0.5:9010"), "10.0.0.5:9010");
        assert_eq!(nominal_host("play.example.org:9010"), "127.0.0.1:9010");
        assert_eq!(nominal_host("play.example.org"), "127.0.0.1:9000");
    }
}
