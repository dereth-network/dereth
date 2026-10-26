//! The plugins under every interface. Their windows, HUDs, bar, hotkeys, the marks they draw in
//! the world and on the radar and the map, the host's questions to the player and its own windows
//! reach the player whichever interface is shown. The Horizon interface seats the plugin windows
//! among its own, with their HUDs, bar and marks, and asks the host's questions in its own top
//! tier ([`ClientShell::seat_plugins_in_horizon`]); under the modern and the classic interface they
//! are a layer over the interface ([`super::plugin_layer`]), which asks the host's questions over
//! everything. The marks every interface draws are worked out here ([`plugin_marks`]).

use dereth_client_contract::options::interface::Interface;
use dereth_plugin_host::{PluginHook, PluginHost};

use super::{ClientShell, Cx};
use crate::platform::host::Host;

/// What the shell keeps for the plugins from one frame to the next.
#[derive(Debug, Default)]
pub(crate) struct PluginSurface {
    /// Which of Ctrl, Shift and Alt are held, for the plugins' hotkeys.
    modifiers: (bool, bool, bool),
    /// The plugin host's windows as an interface that seats them itself keeps them.
    views: ViewCache,
    /// Which of the plugins' world labels the world hides, from the depth at their anchors.
    pub(crate) occlusion: dereth_plugin_ui::LabelOcclusion,
    /// The plugins' layer over the modern and the classic interface.
    pub(crate) layer: super::plugin_layer::LayerHost,
    /// The client's own question this frame, for the layer to ask (Horizon asks it itself).
    question: Option<dereth_client_runtime::dialogs::HostQuestion>,
}

/// One window event, offered to the plugins' layer before the modern or the classic interface
/// sees it: whether the layer keeps it. A press is the layer's where it lands on one of the
/// layer's surfaces and nothing of the interface's stands over it there (the classic interface's
/// modal box stands over the layer, and its panels over the plugins' HUDs; the modern interface's
/// windows stand over it as they are stacked, and its panels and controls over the HUDs). The
/// client's question stands over everything: while it is up every press is the layer's.
pub(super) fn route_to_layer<H: Host>(
    _cx: &mut Cx<'_, H>,
    shell: &mut ClientShell<H>,
    event: &dereth_input::host::HostEvent,
    time_ms: u32,
) -> bool {
    let layer = &mut shell.plugins.layer;
    if let Some(ui) = shell.classic.active() {
        let free = classic_free(ui, layer);
        return layer.route(event, time_ms, &free);
    }
    if shell.horizon.active {
        return false;
    }
    let Some(ui) = shell.modern.ui.as_ref() else {
        return false;
    };
    let screen = ui.flow.current().and_then(|s| s.roots().first().copied());
    // A press lands where the pointer is now.
    let (x, y) = layer.input.mouse;
    // What the layer draws over everything (an open choice's list, the client's own windows, the
    // client's question) is over the interface's windows too.
    let free = layer.layer.over_top(x, y) || layer.modern_free(&ui.ui, screen, x, y);
    layer.route(event, time_ms, &|_, _| free)
}

/// One frame of the pad on the plugins' layer under the modern or the classic interface, which
/// take no pad themselves: a press lands where nothing of the interface stands over the layer
/// (the classic modal box stands over it; the modern interface's windows stand over it as they
/// are stacked).
pub(super) fn pad_to_layer<H: Host>(
    shell: &mut ClientShell<H>,
    pad: Option<crate::gamepad::PadState>,
    time_ms: u32,
) {
    if let Some(ui) = shell.classic.active() {
        let free = classic_free(ui, &shell.plugins.layer);
        shell.plugins.layer.pad(pad, time_ms, &free);
        return;
    }
    let Some(ui) = shell.modern.ui.as_ref() else {
        return;
    };
    let screen = ui.flow.current().and_then(|s| s.roots().first().copied());
    let (tops, seats, bar) = shell.plugins.layer.free_places();
    let free = |x: f32, y: f32| {
        tops.iter().any(|r| r.contains(x, y))
            || super::plugin_layer::modern_free_at(&ui.ui, screen, &seats, &bar, x, y)
    };
    shell.plugins.layer.pad(pad, time_ms, &free);
}

/// Whether nothing of the classic interface's stands over a place on screen: the place is on what
/// the layer draws over everything (the host's own windows and question stand over the modal box
/// too), or no modal box is up and the place is on a plugin window, or off every panel and control
/// of the game's (a plugin's HUD over one of them takes no press there).
pub(super) fn classic_free<'a>(
    ui: &'a dereth_classic_ui::runtime::ClassicUi,
    layer: &super::plugin_layer::LayerHost,
) -> impl Fn(f32, f32) -> bool + use<'a> {
    let modal = ui.modal_up();
    let top = layer.free_places().0;
    let over = layer.over_classic();
    let on_panel = |x: f32, y: f32| {
        ui.game_control_at(
            dereth_primitives::num::to_i32(x),
            dereth_primitives::num::to_i32(y),
        )
    };
    move |x, y| {
        top.iter().any(|r| r.contains(x, y))
            || (!modal && (over.iter().any(|r| r.contains(x, y)) || !on_panel(x, y)))
    }
}

/// A press of `button` under the modern interface that the plugins' layer did not keep: one on the
/// map page's picture of the world, with nothing of the interface's over it there, is told to the
/// plugins with the place under the pointer and the modifiers held. The press goes on to the
/// interface as before.
pub(super) fn modern_map_press<H: Host>(
    cx: &mut Cx<'_, H>,
    shell: &ClientShell<H>,
    button: dereth_input::keys::MouseButton,
) {
    use dereth_client_contract::panels::map::{MapButton, MapClick};
    use dereth_input::keys::MouseButton;
    let button = match button {
        MouseButton::Left => MapButton::Left,
        MouseButton::Right => MapButton::Right,
        MouseButton::Middle => MapButton::Middle,
        _ => return,
    };
    let Some(ui) = shell.modern.ui.as_ref() else {
        return;
    };
    let (x, y) = shell.plugins.layer.input.mouse;
    let Some((ns, ew)) = modern_map_at(&ui.ui, x, y) else {
        return;
    };
    let (ctrl, shift, alt) = shell.plugins.modifiers;
    if let Some(host) = host_mut(cx) {
        host.map_click(&MapClick {
            ns,
            ew,
            button,
            ctrl,
            shift,
            alt,
        });
    }
}

/// The map coordinates under `(x, y)` on the modern interface's map page, read at the pixel's
/// middle by the projection its marks are placed with: `None` unless the page shows and the point
/// is on its picture of the world, with nothing that takes the pointer over the picture there (a
/// place's note on the map and the page itself are the map's own).
fn modern_map_at(ui: &dereth_ui::UiSystem, x: f32, y: f32) -> Option<(f64, f64)> {
    use dereth_ui_screens::mapradar::map::{child, map_coords_at, SHIPPED_NOTE_BINDING};
    let root = ui.root();
    let image = ui
        .get_child_recursive(root, child::MAP_IMAGE)
        .filter(|h| ui.is_visible(*h))?;
    let page = ui.get_child_recursive(root, dereth_ui_screens::mapradar::map::MAP_PAGE)?;
    let (px, py) = (
        dereth_primitives::num::to_i32(x),
        dereth_primitives::num::to_i32(y),
    );
    // Whether `inner` is `outer` or lies within it.
    let within = |inner: dereth_ui::ElemHandle, outer: dereth_ui::ElemHandle| {
        let mut at = Some(inner);
        while let Some(h) = at {
            if h == outer {
                return true;
            }
            at = ui.parent(h);
        }
        false
    };
    if let Some(hit) = ui.hit_test_screen(px, py) {
        if !(within(hit, page) || within(image, hit)) {
            return None;
        }
    }
    let b = ui.screen_box(image);
    map_coords_at(
        SHIPPED_NOTE_BINDING.2,
        f64::from(px - b.x0) + 0.5,
        f64::from(py - b.y0) + 0.5,
    )
}

/// What the player did on a plugin surface or on the game's map, handed to the plugin host: it
/// reaches the plugins at the host's next step.
pub(super) fn deliver(host: &mut PluginHost, input: dereth_plugin_ui::Input) {
    use dereth_plugin_host::UiInput;
    use dereth_plugin_ui::Input;
    let (plugin, window, input) = match input {
        Input::Map(click) => {
            host.map_click(&click);
            return;
        }
        Input::Bar { plugin } => {
            if let Err(e) = host.toggle_window(&plugin) {
                tracing::debug!("a plugin bar click went nowhere: {e}");
            }
            return;
        }
        Input::Command { plugin, verb } => {
            if let Err(e) = host.run_command(&plugin, &verb) {
                tracing::debug!("a plugin command from a slot went nowhere: {e}");
            }
            return;
        }
        Input::Resize {
            plugin,
            window,
            width,
            height,
        } => (plugin, window, UiInput::Resize { width, height }),
        Input::Click {
            plugin,
            window,
            node,
        } => (plugin, window, UiInput::Click { node }),
        Input::Toggle {
            plugin,
            window,
            node,
            value,
        } => (
            plugin,
            window,
            UiInput::Change {
                node,
                value: dereth_plugin_api::Value::Bool(value),
            },
        ),
        Input::Select {
            plugin,
            window,
            node,
            row,
        } => (
            plugin,
            window,
            UiInput::Select {
                node,
                rows: vec![row],
            },
        ),
        Input::Close { plugin, window } => (plugin, window, UiInput::Close),
        Input::Set {
            plugin,
            window,
            node,
            value,
        } => (plugin, window, UiInput::Change { node, value }),
    };
    if let Err(e) = host.ui_input(&plugin, &window, input) {
        tracing::debug!("a plugin window input went nowhere: {e}");
    }
}

/// The plugin bar's buttons: the plugins active here that have a window.
pub(super) fn bar_items(host: &PluginHost) -> Vec<dereth_plugin_ui::BarItem> {
    host.bar()
        .into_iter()
        .filter(|e| e.window.is_some())
        .map(|e| dereth_plugin_ui::BarItem {
            plugin: e.id,
            name: e.name,
            state: e.state,
            open: e.open,
        })
        .collect()
}

/// Where the plugin bar opens and how wide, as remembered for the character: the top left of
/// the game area at first, as wide as its buttons.
pub(super) fn bar_at(host: &PluginHost) -> dereth_plugin_ui::BarPlace {
    let v = host.layout("bar");
    #[allow(clippy::cast_precision_loss)]
    let at = v
        .and_then(|v| Some((v.get("x").as_i64()?, v.get("y").as_i64()?)))
        .map_or((8.0, 8.0), |(x, y)| (x as f32, y as f32));
    #[allow(clippy::cast_precision_loss)]
    let width = v
        .and_then(|v| v.get("w").as_i64())
        .map(|w| w.clamp(1, 100_000) as f32);
    dereth_plugin_ui::BarPlace { at, width }
}

/// Where plugin window `(plugin, window)` was left, and the size the player gave it, as
/// remembered for the character.
#[allow(clippy::type_complexity)] // a place and a size, read in two places
fn kept(host: &PluginHost, plugin: &str, window: &str) -> Option<((f32, f32), Option<(f32, f32)>)> {
    let v = host.layout(&format!("at/{plugin}/{window}"))?;
    #[allow(clippy::cast_precision_loss)]
    let at = (v.get("x").as_i64()? as f32, v.get("y").as_i64()? as f32);
    #[allow(clippy::cast_precision_loss)]
    let size = v
        .get("w")
        .as_i64()
        .zip(v.get("h").as_i64())
        .map(|(w, h)| (w.clamp(1, 100_000) as f32, h.clamp(1, 100_000) as f32));
    Some((at, size))
}

/// Where each plugin window was left, as remembered for the character.
pub(super) fn window_places(host: &PluginHost) -> dereth_plugin_ui::Places {
    host.windows()
        .into_iter()
        .filter_map(|(plugin, window, _)| {
            let (at, _) = kept(host, &plugin, &window)?;
            Some(((plugin, window), at))
        })
        .collect()
}

/// The size the player left each plugin window at, frame and all, as remembered for the
/// character (layout units).
pub(super) fn window_sizes(host: &PluginHost) -> dereth_plugin_ui::Sizes {
    host.windows()
        .into_iter()
        .filter_map(|(plugin, window, _)| {
            let (_, size) = kept(host, &plugin, &window)?;
            Some(((plugin, window), size?))
        })
        .collect()
}

/// Remember where the player left the plugin windows (and the size they gave each) and the bar
/// (and the width they gave it), with the character.
pub(super) fn remember_places(
    host: &mut PluginHost,
    places: &[dereth_plugin_ui::Place],
    sizes: &[dereth_plugin_ui::Size],
    bar: Option<dereth_plugin_ui::BarPlace>,
) {
    let whole = |v: f32| i64::from(dereth_primitives::num::to_i32(v.round()));
    let v = |(x, y): (f32, f32)| {
        dereth_plugin_api::Value::map()
            .with("x", whole(x))
            .with("y", whole(y))
    };
    for (key, at) in places {
        let mut place = v(*at);
        if let Some((_, (w, h))) = sizes.iter().find(|(k, _)| k == key) {
            place = place.with("w", whole(*w)).with("h", whole(*h));
        }
        host.remember(&format!("at/{}/{}", key.0, key.1), place);
    }
    if let Some(bar) = bar {
        let mut place = v(bar.at);
        if let Some(w) = bar.width {
            place = place.with("w", whole(w));
        }
        host.remember("bar", place);
    }
}

/// Hand the layer's windows where the player left them and the size they gave them.
fn hint_layer(ui: &mut dereth_plugin_ui::PluginUi, host: &PluginHost) {
    for ((plugin, window), (x, y)) in window_places(host) {
        ui.place_hint(&plugin, &window, x, y);
    }
    for ((plugin, window), (w, h)) in window_sizes(host) {
        ui.size_hint(&plugin, &window, w, h);
    }
}

/// How often a window's tree is shown afresh at most, a second: a plugin changing it faster has
/// its changes shown together, the latest each time.
pub(crate) const SHOWN_A_SECOND: f64 = 30.0;

/// The plugin host's windows as the presentation keeps them: each window's copy made again only
/// when the window changes, and its tree at most [`SHOWN_A_SECOND`] times a second, so a frame
/// hands the presentation the trees and pictures it already has.
#[derive(Debug, Default)]
pub(crate) struct ViewCache {
    views: Vec<dereth_plugin_ui::WindowView>,
    /// When each window's copy was last made, by `(plugin, window)`, on the frame clock.
    made: std::collections::HashMap<(String, String), f64>,
}

impl ViewCache {
    /// Every window the host keeps now, in the host's order, at `now` on the frame clock.
    pub(crate) fn refresh(
        &mut self,
        host: &PluginHost,
        now: f64,
    ) -> Vec<dereth_plugin_ui::WindowView> {
        let mut kept = std::mem::take(&mut self.views);
        let was = std::mem::take(&mut self.made);
        let made = &mut self.made;
        self.views = host
            .windows()
            .into_iter()
            .map(|(plugin, window, w)| {
                let key = (plugin.clone(), window.clone());
                let was_made = was.get(&key).copied();
                // A new tree waits while the last was made too lately; a window opened, closed,
                // or sized is shown at once.
                let old = kept
                    .iter()
                    .position(|v| v.plugin == plugin && v.window == window)
                    .map(|i| kept.swap_remove(i))
                    .filter(|v| {
                        v.open == w.open
                            && v.size == w.size
                            && v.by_player == w.by_player
                            && (v.revision == w.revision
                                || was_made.is_some_and(|t| now - t < 1.0 / SHOWN_A_SECOND))
                    });
                made.insert(
                    key,
                    if old.is_some() {
                        was_made.unwrap_or(now)
                    } else {
                        now
                    },
                );
                old.unwrap_or_else(|| dereth_plugin_ui::WindowView {
                    tier: if PluginHost::is_host_window(&plugin, &window) {
                        dereth_plugin_ui::Tier::Host
                    } else {
                        dereth_plugin_ui::Tier::Plugin
                    },
                    images: dereth_plugin_ui::image_sources(&w.tree)
                        .into_iter()
                        .filter_map(|src| {
                            let (bytes, hash) = host.image(&plugin, &src)?;
                            Some(dereth_plugin_ui::Picture {
                                src,
                                bytes: std::sync::Arc::from(bytes),
                                hash,
                            })
                        })
                        .collect(),
                    tree: std::sync::Arc::new(w.tree.clone()),
                    open: w.open,
                    revision: w.revision,
                    size: w.size,
                    by_player: w.by_player,
                    plugin,
                    window,
                })
            })
            .collect();
        self.views.clone()
    }
}

/// The installed plugin host, if any.
pub(super) fn host_mut<'a, H: Host>(cx: &'a mut Cx<'_, H>) -> Option<&'a mut PluginHost> {
    cx.frame_hook_mut()
        .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
        .map(|h| &mut h.host)
}

impl<H: Host> ClientShell<H> {
    /// Each frame, once the interface shown is settled: whether it draws the plugin windows, the
    /// player's answer to the host's dialog for a file, then the dialog to show now (Horizon
    /// shows it in its own top tier, every other interface in the plugins' layer); and for an interface
    /// that seats the plugin windows itself, what the player did on them since the last frame,
    /// and the windows, bar and marks to draw now.
    pub(super) fn follow_plugins(&mut self, cx: &mut Cx<'_, H>) {
        if self.horizon.active {
            self.seat_plugins_in_horizon(cx);
        }
        let modern = self.modern.ui.is_some() && self.shown_interface() == Interface::Modern;
        let horizon = self.horizon.active;
        let windows = modern || horizon || self.classic.active;
        let layer_reply = self.plugins.layer.layer.take_reply();
        let (front, _) = self.front_and_services();
        let Some(host) = host_mut(cx) else {
            front.host_question(None);
            self.plugins.question = None;
            return;
        };
        host.set_windows_shown(windows);
        for reply in front.take_host_reply().into_iter().chain(layer_reply) {
            if let Err(e) = host.reply(&reply) {
                tracing::debug!("an answer to the plugin host's question went nowhere: {e}");
            }
        }
        let question = host.question();
        front.host_question(if horizon { question.as_ref() } else { None });
        self.plugins.question = if horizon { None } else { question };
    }

    /// The modern interface's share of the plugins, once a frame before its draw list is made:
    /// what the player did on the layer last frame goes to the host, the game's panels a plugin
    /// hides or moves are set, the layer is drawn (its marks placed on the interface's radar,
    /// map and screen), and its stand-ins stacked among the interface's windows.
    pub(super) fn compose_plugins_modern(&mut self, cx: &mut Cx<'_, H>) {
        let marks = plugin_marks(cx, &mut self.plugins.occlusion);
        let Some(shell) = self.modern.ui.as_mut() else {
            return;
        };
        let layer = &mut self.plugins.layer;
        let now = cx.now();
        let huds_unlocked = host_mut(cx).is_some_and(|h| h.huds_unlocked());
        let (views, bar, at) = match host_mut(cx) {
            Some(host) => {
                super::apply_retail_ui(shell, &mut self.modern.retail_ui_before, &host.retail_ui());
                for input in layer.layer.take_inputs() {
                    deliver(host, input);
                }
                remember_places(
                    host,
                    &layer.layer.ui.places(),
                    &layer.layer.ui.sizes(),
                    layer.layer.bar_place(),
                );
                hint_layer(&mut layer.layer.ui, host);
                (
                    layer.views.refresh(host, now),
                    bar_items(host),
                    bar_at(host),
                )
            }
            None => (
                Vec::new(),
                Vec::new(),
                dereth_plugin_ui::BarPlace::default(),
            ),
        };
        let glyphs = modern_mark_glyphs(&shell.ui, &marks);
        let raised = layer.frame(
            cx,
            dereth_plugin_ui::Scene {
                views: &views,
                bar: &bar,
                bar_at: at,
                marks: &glyphs,
                labels: &marks.labels,
                question: self.plugins.question.as_ref(),
                bar_look: dereth_plugin_ui::BarLook::Plain,
                // The interface's own panels and controls are kept from the HUDs as each press
                // is routed.
                game_controls: &[],
                huds_unlocked,
            },
            &super::plugin_layer::Skin::Retail,
        );
        let screen = shell
            .flow
            .current()
            .and_then(|s| s.roots().first().copied());
        let view = shell.ui.get_child_recursive(
            shell.ui.root(),
            dereth_ui_screens::hud::world_view::SMART_BOX,
        );
        layer.seat_in_modern(&mut shell.ui, screen, view, raised.as_ref());
    }

    /// The classic interface's share of the plugins, once a frame before it composes: as the
    /// modern interface's, the game's panels a plugin hides hidden (its panels are docked, so a
    /// move has no place to go), and the marks placed on its radar and its map.
    pub(super) fn compose_plugins_classic(&mut self, cx: &mut Cx<'_, H>) {
        let marks = plugin_marks(cx, &mut self.plugins.occlusion);
        let layer = &mut self.plugins.layer;
        let now = cx.now();
        let huds_unlocked = host_mut(cx).is_some_and(|h| h.huds_unlocked());
        let (views, bar, at) = match host_mut(cx) {
            Some(host) => {
                let asked = host.retail_ui();
                let hidden: Vec<&str> = asked
                    .iter()
                    .filter(|(_, _, hidden, _)| *hidden == Some(true))
                    .map(|(panel, ..)| panel.as_str())
                    .collect();
                if let Some(ui) = self.classic.active_mut() {
                    ui.set_retail_hidden(&hidden);
                    for click in ui.take_map_clicks() {
                        host.map_click(&click);
                    }
                }
                for input in layer.layer.take_inputs() {
                    deliver(host, input);
                }
                remember_places(
                    host,
                    &layer.layer.ui.places(),
                    &layer.layer.ui.sizes(),
                    layer.layer.bar_place(),
                );
                hint_layer(&mut layer.layer.ui, host);
                (
                    layer.views.refresh(host, now),
                    bar_items(host),
                    bar_at(host),
                )
            }
            None => {
                if let Some(ui) = self.classic.active_mut() {
                    ui.set_retail_hidden(&[]);
                    let _ = ui.take_map_clicks();
                }
                (
                    Vec::new(),
                    Vec::new(),
                    dereth_plugin_ui::BarPlace::default(),
                )
            }
        };
        let mut glyphs = Vec::new();
        if let Some(ui) = self.classic.active() {
            if let Some((centre, radius)) = ui.radar_area() {
                glyphs.extend(dereth_plugin_ui::marks::radar_glyphs(
                    &marks.radar,
                    centre,
                    radius,
                    marks.radar_range,
                    0.0,
                ));
            }
            let place = |ns: f64, ew: f64| ui.map_point(ns, ew);
            glyphs.extend(dereth_plugin_ui::marks::map_glyphs(&marks.map, &place));
            if let Some(arrow) = &marks.arrow {
                let size = cx.present().size();
                glyphs.extend(dereth_plugin_ui::marks::arrow_mark_glyphs(
                    arrow,
                    i32::try_from(size.0 / 2).unwrap_or(400),
                    64,
                ));
            }
        }
        // The classic interface's own art, where it has been read.
        let skin = self
            .classic
            .active()
            .and_then(|ui| ui.resources.art.clone())
            .map_or(super::plugin_layer::Skin::Retail, |art| {
                super::plugin_layer::Skin::Classic(art)
            });
        layer.frame(
            cx,
            dereth_plugin_ui::Scene {
                views: &views,
                bar: &bar,
                bar_at: at,
                marks: &glyphs,
                labels: &marks.labels,
                question: self.plugins.question.as_ref(),
                bar_look: dereth_plugin_ui::BarLook::Plain,
                game_controls: &[],
                huds_unlocked,
            },
            &skin,
        );
    }

    /// The Horizon interface's share of the plugins: what the player did on their windows, HUDs
    /// and bar last frame goes to the host, where the player left them is remembered, and the
    /// windows, bar and marks to draw this frame are handed over.
    fn seat_plugins_in_horizon(&mut self, cx: &mut Cx<'_, H>) {
        let marks = plugin_marks(cx, &mut self.plugins.occlusion);
        let now = cx.now();
        let Some(ui) = self.horizon.active_mut() else {
            return;
        };
        let Some(host) = host_mut(cx) else {
            let _ = ui.take_plugin_inputs();
            ui.set_plugins(dereth_horizon::ui::plugins::PluginFrame::default());
            ui.set_retail_ui(&[]);
            return;
        };
        for input in ui.take_plugin_inputs() {
            deliver(host, input);
        }
        // The game's panels a plugin hides or moves, as Horizon has them.
        #[allow(clippy::cast_precision_loss)]
        let asked: Vec<dereth_horizon::retail_ui::Ask> = host
            .retail_ui()
            .into_iter()
            .map(|(panel, _, hidden, at)| dereth_horizon::retail_ui::Ask {
                panel,
                hidden,
                at: at.map(|(x, y)| (x as f32, y as f32)),
            })
            .collect();
        ui.set_retail_ui(&asked);
        let (places, sizes, bar) = ui.plugin_places();
        remember_places(host, &places, &sizes, bar);
        for ((plugin, window), (x, y)) in window_places(host) {
            ui.plugin_place_hint(&plugin, &window, x, y);
        }
        for ((plugin, window), (w, h)) in window_sizes(host) {
            ui.plugin_size_hint(&plugin, &window, w, h);
        }
        let views = self.plugins.views.refresh(host, now);
        let frame = dereth_horizon::ui::plugins::PluginFrame {
            views,
            bar: bar_items(host),
            bar_at: bar_at(host),
            marks,
            commands: host
                .commands()
                .into_iter()
                .map(|c| dereth_horizon::ui::plugins::PluginCommandEntry {
                    plugin: c.plugin,
                    verb: c.verb,
                    help: c.help,
                })
                .collect(),
            huds_unlocked: host.huds_unlocked(),
        };
        ui.set_plugins(frame);
    }

    /// The plugins' marks in the world: a world marker's dot and a shape's outline, drawn by the
    /// world against its depth under whichever interface is shown.
    pub(super) fn plugin_world_marks(cx: &mut Cx<'_, H>) {
        use dereth_plugin_host::overlay::{Kind, ShapeKind};
        let mut marks = Vec::new();
        let host = cx
            .frame_hook()
            .and_then(|h| h.as_any().downcast_ref::<PluginHook>())
            .map(|h| &h.host);
        if let Some(host) = host {
            let core = &*cx;
            let mark = |points: &[dereth_primitives::Position], color: u32, width: i32, on_top| {
                let points: Vec<[f32; 3]> =
                    points.iter().filter_map(|p| core.render_point(p)).collect();
                #[allow(clippy::cast_precision_loss)] // 1 to 6 pixels
                (!points.is_empty()).then_some(crate::present::WorldMark {
                    points,
                    color: 0xFF00_0000 | (color & 0x00FF_FFFF),
                    width: width as f32,
                    on_top,
                })
            };
            for m in &host.overlays().markers {
                match m.kind {
                    Kind::World => {
                        let Some(mut at) = m.at else { continue };
                        at.frame.origin.z += 2.0;
                        marks.extend(mark(&[at], m.color, 6, m.on_top));
                    }
                    Kind::Shape if m.shape != Some(ShapeKind::Text) => {
                        marks.extend(mark(&m.path, m.color, m.width, m.on_top));
                    }
                    _ => {}
                }
            }
        }
        cx.present_mut().set_world_marks(marks);
    }
}

/// The plugins' marks this frame, for whichever interface is shown: their labels, floating text
/// and nametags where their places fall in the game view, each tested at its anchor against the
/// depth the world left there and decluttered nearest first; a blip for the radar (heading up, in
/// metres, within the radar's range); points and lines for the map; and the arrow toward its
/// target. The depths asked for come back a frame or two later, to `occlusion`.
pub(super) fn plugin_marks<H: Host>(
    cx: &mut Cx<'_, H>,
    occlusion: &mut dereth_plugin_ui::LabelOcclusion,
) -> dereth_plugin_ui::Marks {
    use dereth_plugin_host::overlay::{Kind, ShapeKind};
    use dereth_plugin_host::places;
    use dereth_plugin_ui::{declutter, ArrowMark, Glyph, LabelSpot, MapMark, Marks, RadarMark};
    if let Some((tag, depths)) = cx.present_mut().take_depth_probes() {
        occlusion.answer(tag, &depths);
    }
    let probing = cx.present().depth_probes_supported();
    // The anchors to probe this frame: the pixel, the label's key and its anchor's depth.
    let mut probes: Vec<((u32, u32), String, f32)> = Vec::new();
    let mut marks = Marks::default();
    // Labels that stand at places in the world: the spot, its colour and how strongly it is
    // drawn (a nametag fades at the end of its range).
    let mut spots: Vec<(LabelSpot, u32, f64)> = Vec::new();
    {
        let core = &*cx;
        let host = core
            .frame_hook()
            .and_then(|h| h.as_any().downcast_ref::<PluginHook>())
            .map(|h| &h.host);
        if let Some(host) = host {
            let ov = host.overlays();
            marks.radar_range = ov.player.map_or(0.0, |me| {
                dereth_client_contract::radar::radar_range(places::outdoors(me.cell))
            });
            // Each label is tested once, at its anchor, against the depth the world left there, and
            // is shown or hidden whole. `key` names it from one frame to the next; `toward` is how
            // far nearer the camera than its place it is tested, so a nametag clears the object it
            // names; `rank` puts a plugin's own text before nametags.
            let mut label = |m: &dereth_plugin_host::OverlayView,
                             at: &dereth_primitives::Position,
                             (rank, toward): (u8, f32),
                             dy: i32| {
                if m.label.is_empty() {
                    return;
                }
                let Some((x, y, depth)) = core.screen_point_depth(at, toward) else {
                    return;
                };
                let key = format!("{}#{}#{}", m.plugin, m.id, m.object.map_or(0, |o| o.0));
                if probing && !m.on_top {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // LINT-OK: a pixel inside the game view.
                    probes.push(((x.max(0.0) as u32, y.max(0.0) as u32), key.clone(), depth));
                    if !occlusion.visible(&key) {
                        return;
                    }
                }
                let distance = ov.player.map_or(0.0, |me| places::distance(&me, at));
                let strength = m.reach.map_or(1.0, |r| {
                    dereth_plugin_host::overlay::nametag_strength(distance, r)
                });
                spots.push((
                    LabelSpot {
                        x: dereth_primitives::num::to_i32(x),
                        y: dereth_primitives::num::to_i32(y) + dy,
                        text: m.label.clone(),
                        distance,
                        rank,
                    },
                    m.color,
                    strength,
                ));
            };
            for m in &ov.markers {
                match m.kind {
                    Kind::World => {
                        let Some(mut at) = m.at else { continue };
                        at.frame.origin.z += 2.0;
                        label(m, &at, (2, 0.5), -20);
                    }
                    Kind::Radar => {
                        let (Some(me), Some(at)) = (ov.player, m.at) else {
                            continue;
                        };
                        let range = f64::from(dereth_client_contract::radar::radar_range(
                            places::outdoors(me.cell),
                        ));
                        let d = places::ground_distance(&me, &at);
                        if d >= range {
                            continue;
                        }
                        let turn = places::turn(places::heading(&me), places::bearing(&me, &at))
                            .to_radians();
                        #[allow(clippy::cast_possible_truncation)]
                        // LINT-OK: metres within a radar's range.
                        marks.radar.push(RadarMark {
                            right: (dereth_primitives::num::math::sin(turn) * d) as f32,
                            ahead: (dereth_primitives::num::math::cos(turn) * d) as f32,
                            color: m.color,
                        });
                    }
                    Kind::Map => marks.map.push(MapMark {
                        points: m.points.iter().filter_map(places::coords).collect(),
                        line: m.line,
                        label: m.label.clone(),
                        color: m.color,
                    }),
                    Kind::Shape if m.shape == Some(ShapeKind::Text) => {
                        let Some(at) = m.at else { continue };
                        label(m, &at, (2, 0.5), 0);
                    }
                    Kind::Shape => {
                        if let Some(first) = m.path.first() {
                            label(m, first, (2, 0.5), -16);
                        }
                    }
                    Kind::Nametag => {
                        let Some(at) = m.at else { continue };
                        label(m, &at, (1, 3.0), 0);
                    }
                    Kind::Nametags => {}
                    Kind::Arrow => {
                        if let Some((turn, distance)) = ov.arrow_of(m) {
                            marks.arrow = Some(ArrowMark {
                                turn,
                                distance,
                                label: m.label.clone(),
                                color: m.color,
                            });
                        }
                    }
                }
            }
        }
    }
    let placed: Vec<LabelSpot> = spots.iter().map(|(s, _, _)| s.clone()).collect();
    marks.labels = declutter(&placed)
        .into_iter()
        .filter_map(|(i, alpha)| {
            let (s, color, strength) = &spots[i];
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: an alpha from 0 to 255.
            let alpha = (f64::from(alpha) * strength).round() as u32;
            (alpha > 0).then(|| Glyph::Label {
                x: s.x,
                y: s.y,
                text: s.text.clone(),
                color: (alpha << 24) | (color & 0x00FF_FFFF),
            })
        })
        .collect();
    if !probes.is_empty() {
        let (pixels, labels): (Vec<_>, Vec<_>) = probes
            .into_iter()
            .map(|(pixel, key, depth)| (pixel, (key, depth)))
            .unzip();
        let tag = occlusion.ask(labels);
        cx.present_mut().request_depth_probes(tag, pixels);
    }
    marks
}

/// The plugins' marks as the modern interface places them: a blip on its radar (heading up, at
/// the radar's range), points and lines on its map when the map is open, and the arrow at the top
/// of the screen.
fn modern_mark_glyphs(
    ui: &dereth_ui::UiSystem,
    marks: &dereth_plugin_ui::Marks,
) -> Vec<dereth_plugin_ui::Glyph> {
    let root = ui.root();
    let shown = |id| {
        ui.get_child_recursive(root, id)
            .filter(|h| ui.is_visible(*h))
            .map(|h| ui.screen_box(h))
    };
    let mut glyphs = Vec::new();
    if let Some(b) = shown(dereth_ui_screens::screens::gameplay::window::RADAR) {
        #[allow(clippy::cast_precision_loss)]
        let r = ((b.x1 - b.x0).min(b.y1 - b.y0) / 2 - 6) as f32;
        #[allow(clippy::cast_precision_loss)]
        let centre = ((b.x0 + b.x1) as f32 / 2.0, (b.y0 + b.y1) as f32 / 2.0);
        glyphs.extend(dereth_plugin_ui::marks::radar_glyphs(
            &marks.radar,
            centre,
            r,
            marks.radar_range,
            0.0,
        ));
    }
    if let Some(b) = shown(dereth_ui_screens::mapradar::map::child::MAP_IMAGE) {
        let area = dereth_ui_screens::mapradar::map::SHIPPED_NOTE_BINDING.2;
        #[allow(clippy::cast_possible_truncation)]
        let place = |ns: f64, ew: f64| {
            let (x, y) = dereth_ui_screens::mapradar::map::place_marker_on_map(
                area,
                ew as f32,
                ns as f32,
                (0, 0),
            );
            Some((b.x0 + x, b.y0 + y))
        };
        glyphs.extend(dereth_plugin_ui::marks::map_glyphs(&marks.map, &place));
    }
    if let Some(arrow) = &marks.arrow {
        let top = ui.screen_box(root);
        glyphs.extend(dereth_plugin_ui::marks::arrow_mark_glyphs(
            arrow,
            (top.x0 + top.x1) / 2,
            top.y0 + 64,
        ));
    }
    glyphs
}

/// A key a plugin's hotkey asks for, pressed with its Ctrl, Shift and Alt, under any interface:
/// its command runs, and the key goes no further. Ctrl, Shift and Alt are followed here; a key
/// pressed while the player types in a text box is the text box's, and while the client's own
/// question is up no plugin's hotkey runs.
pub(super) fn plugin_hotkey<H: Host>(
    cx: &mut Cx<'_, H>,
    shell: &mut ClientShell<H>,
    key: dereth_input::keys::Key,
    pressed: bool,
) -> bool {
    let vk = key.virtual_key;
    let m = &mut shell.plugins.modifiers;
    match vk {
        0x11 | 0xA2 | 0xA3 => m.0 = pressed,
        0x10 | 0xA0 | 0xA1 => m.1 = pressed,
        0x12 | 0xA4 | 0xA5 => m.2 = pressed,
        _ => {}
    }
    if !pressed {
        return false;
    }
    // The key's name as a hotkey spells it.
    let name = match vk {
        0x30..=0x39 | 0x41..=0x5A => {
            char::from_u32(u32::try_from(vk).unwrap_or(0)).map(|c| c.to_string())
        }
        0x70..=0x87 => Some(format!("F{}", vk - 0x6F)),
        _ => None,
    };
    let Some(name) = name else {
        return false;
    };
    let (ctrl, shift, alt) = *m;
    if !(ctrl || alt) {
        return false;
    }
    let layer = &shell.plugins.layer;
    let (layer_typing, layer_asking) = (layer.typing(), layer.asking());
    let (front, _) = shell.front_and_services();
    if front.typing() || front.asking() || layer_typing || layer_asking {
        return false;
    }
    let Some(host) = host_mut(cx) else {
        return false;
    };
    let hit = host.hotkeys().into_iter().find(|h| {
        h.key
            .as_deref()
            .and_then(dereth_plugin_api::ui::chord)
            .is_some_and(|c| c == (ctrl, shift, alt, name.clone()))
    });
    match hit {
        Some(h) => host.press_hotkey(&h.plugin, &h.node).is_ok(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use dereth_plugin_api::HostCapability;
    use dereth_plugin_host::{HostConfig, MemoryStorage, PluginHost, StaticSource};

    /// A host with no plugin, its memory of the interface's places empty.
    fn host() -> PluginHost {
        PluginHost::new(
            HostConfig::new("dereth-test", vec![HostCapability::Core]),
            Box::new(dereth_plugin_lua::LuauEngine),
            Box::new(StaticSource {
                packages: Vec::new(),
            }),
            Box::new(MemoryStorage::default()),
        )
    }

    /// Where the player left a plugin window and the size they gave it, and where they left the
    /// plugin bar and the width they gave it, are remembered with the character and given back
    /// as they were left.
    /// Behaviour: plugins.ui.a-window-and-the-bar-come-back-where-the-player-left-them-on-the-screen
    #[test]
    fn a_windows_place_and_size_and_the_bars_place_and_width_are_given_back_as_left() {
        let mut host = host();
        let key = ("test.kept".to_owned(), "main".to_owned());
        super::remember_places(
            &mut host,
            &[(key.clone(), (120.0, 80.0))],
            &[(key.clone(), (420.0, 260.0))],
            Some(dereth_plugin_ui::BarPlace {
                at: (16.0, 640.0),
                width: Some(300.0),
            }),
        );
        assert_eq!(
            super::kept(&host, &key.0, &key.1),
            Some(((120.0, 80.0), Some((420.0, 260.0))))
        );
        assert_eq!(
            super::bar_at(&host),
            dereth_plugin_ui::BarPlace {
                at: (16.0, 640.0),
                width: Some(300.0),
            }
        );
    }
}
