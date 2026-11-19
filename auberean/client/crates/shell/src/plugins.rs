//! The plugins' seat in the shell: the plugin host the product hands it, installed as the
//! client's frame hook for each session and kept between sessions; the lines the player types,
//! offered to it before they are said; what it prints, for the interface's log; and its windows
//! and HUDs, handed to the interface shown as plain trees ([`PluginWindow`]), which draws them and asks for
//! the player's presses and openings ([`crate::interface::Request::PluginClick`],
//! [`crate::interface::Request::PluginWindow`]).

use std::any::Any;

use auberean_client_sdk::plugin_api::UiNode;
use auberean_client_sdk::plugins::PluginHost;
use auberean_client_sdk::{Driver, FrameHook, HookFrame};

/// The host as a session's frame hook, taken out again as the session ends.
#[derive(Debug)]
struct Hosted(Option<PluginHost>);

impl FrameHook for Hosted {
    fn step(&mut self, frame: &mut HookFrame<'_>) {
        if let Some(h) = &mut self.0 {
            h.step(frame);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// One of a plugin's declared windows or HUDs, as the interface draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginWindow {
    /// The plugin's id, and the window's.
    pub plugin: String,
    pub window: String,
    /// The window's tree: its root a window or a HUD.
    pub tree: UiNode,
    /// Whether the player sees it.
    pub open: bool,
    /// Raised by the host on every change of the window.
    pub revision: u64,
}

/// The plugin host while there is no session, and the windows as last handed to the interface.
#[derive(Debug, Default)]
pub struct Seat {
    /// The host, between sessions; in a session it is the client's frame hook.
    pub idle: Option<PluginHost>,
    /// The plugins' windows and HUDs, as the host last had them.
    pub windows: Vec<PluginWindow>,
}

impl Seat {
    /// The host installed as `driver`'s frame hook, for its session at `world` as `account`.
    pub fn install(&mut self, driver: &mut Driver, world: String, account: String) {
        if let Some(mut host) = self.idle.take() {
            host.set_session(Some(world), Some(account));
            driver.install_hook(Box::new(Hosted(Some(host))));
        }
    }

    /// The host taken back from a session's `driver` as the session ends, its stores written.
    pub fn take_back(&mut self, driver: &mut Driver) {
        let host = driver
            .hook_mut()
            .and_then(|h| h.as_any_mut().downcast_mut::<Hosted>())
            .and_then(|h| h.0.take());
        if let Some(mut host) = host {
            driver.remove_hook();
            host.flush();
            self.idle = Some(host);
        }
    }

    /// The host: the session's frame hook while there is one, else the one kept between
    /// sessions.
    pub fn host<'a>(&'a mut self, driver: Option<&'a mut Driver>) -> Option<&'a mut PluginHost> {
        let hosted = driver
            .and_then(|d| d.hook_mut())
            .and_then(|h| h.as_any_mut().downcast_mut::<Hosted>())
            .and_then(|h| h.0.as_mut());
        hosted.or(self.idle.as_mut())
    }

    /// A frame's turn: the windows followed from the host (the session's, else the one kept),
    /// and the lines it printed since the last, for the player.
    pub fn frame(&mut self, driver: Option<&mut Driver>) -> Vec<String> {
        let hosted = driver
            .and_then(|d| d.hook_mut())
            .and_then(|h| h.as_any_mut().downcast_mut::<Hosted>())
            .and_then(|h| h.0.as_mut());
        let Some(host) = hosted.or(self.idle.as_mut()) else {
            self.windows.clear();
            return Vec::new();
        };
        let printed = host.take_printed().iter().map(told).collect();
        follow(&mut self.windows, host);
        printed
    }
}

/// `windows` followed from `host`: each kept as it was while its revision is unchanged.
fn follow(windows: &mut Vec<PluginWindow>, host: &PluginHost) {
    {
        let mut next = Vec::new();
        for (plugin, window, w) in host.windows() {
            let kept = windows
                .iter()
                .position(|o| o.plugin == plugin && o.window == window && o.revision == w.revision);
            next.push(match kept {
                Some(i) => windows.swap_remove(i),
                None => PluginWindow {
                    plugin,
                    window,
                    tree: w.tree.clone(),
                    open: w.open,
                    revision: w.revision,
                },
            });
        }
        *windows = next;
    }
}

/// A line a plugin printed, as the player is told it: the plugin's name before it.
#[must_use]
pub fn told(line: &auberean_client_sdk::plugins::Line) -> String {
    match &line.from {
        Some(name) => format!("{name}: {}", line.text),
        None => line.text.clone(),
    }
}
