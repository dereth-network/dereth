//! The plugins' surfaces for an interface with no windows of its own to seat plugin windows in
//! (the modern and the classic interface): drawn by the kit in a layer over the interface, each
//! window into a draw list of its own so the interface can stand each among its own windows.
//!
//! Bottom to top the layer is: the world's labels; the HUDs, the marks on the radar and the map,
//! the arrow and the plugin bar; each window, back to front, a press bringing one to the front;
//! over them all the open choice's list and the tooltip; and over everything the host's own tier
//! ([`crate::host`]), its windows and its question. The interface draws each list where it belongs
//! among its own, and hands the layer only the presses that land on it ([`Drawn::hits`]).

use dereth_ui_kit::art::{Art, Family};
use dereth_ui_kit::draw::{DrawList, Rect};
use dereth_ui_kit::paint::{Painter, TextStyle};
use dereth_ui_kit::widgets::Ctx;

use dereth_client_contract::host_question::{HostQuestion, HostReply};

use crate::bar::{Bar, BarLook, BarPlace};
use crate::host::HostTier;
use crate::window::PluginUi;
use crate::{BarItem, Glyph, Tier, WindowView};

/// What the layer draws this frame, as the host hands it over.
#[derive(Debug, Clone, Copy)]
pub struct Scene<'a> {
    /// Every window and HUD the host keeps, open or not.
    pub views: &'a [WindowView],
    /// The plugin bar's buttons, and where the player left it.
    pub bar: &'a [BarItem],
    pub bar_at: BarPlace,
    /// The marks over the interface: radar blips, map points and lines, the arrow.
    pub marks: &'a [Glyph],
    /// The labels standing in the world, under every window and panel.
    pub labels: &'a [Glyph],
    /// The host's question to the player, over everything.
    pub question: Option<&'a HostQuestion>,
    /// The interface's own controls under the layer this frame: a HUD's controls never take the
    /// pointer over one of them (a click there is the interface's), though the HUD is drawn over.
    pub game_controls: &'a [Rect],
    /// How the plugin bar is drawn: plainly, or in the interface's own art.
    pub bar_look: BarLook,
    /// The player has unlocked the HUDs: a press on one moves it.
    pub huds_unlocked: bool,
}

/// What the layer drew this frame: each part's draw list, for the interface to draw where it
/// belongs, and where a press lands on the layer.
#[derive(Debug, Default)]
pub struct Drawn {
    /// The labels in the world.
    pub labels: DrawList,
    /// The HUDs, the marks and the bar.
    pub under: DrawList,
    /// Each window, back to front: its plugin and window, where it is, and its draw list.
    pub windows: Vec<((String, String), Rect, DrawList)>,
    /// Over everything: the open choice's list, the tooltip and the host's own tier.
    pub top: DrawList,
    /// Where a press is the layer's: each window, the bar, the HUDs' controls, an open choice's
    /// list, the host's own windows and its question.
    pub hits: Vec<Rect>,
    /// Where the parts over everything are: an open choice's list, the host's own windows and its
    /// question.
    pub top_hits: Vec<Rect>,
    /// The window a press landed on this frame, brought to the front.
    pub raised: Option<(String, String)>,
}

/// The layer's state from one frame to the next.
#[derive(Debug, Default)]
pub struct Layer {
    /// The windows' presentation, shared with an interface that seats them itself.
    pub ui: PluginUi,
    /// The open windows, back to front.
    order: Vec<(String, String)>,
    bar: Bar,
    drawn: Drawn,
    /// The host's own tier over the plugins.
    pub host: HostTier,
    /// The host's question was up last frame.
    asking: bool,
}

impl Layer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Where the bar stands and the width the player gave it, for the caller to remember, once
    /// it has been drawn.
    #[must_use]
    pub fn bar_place(&self) -> Option<BarPlace> {
        self.bar.place()
    }

    /// Where the bar was drawn last, in screen pixels: its handle, a button for each plugin and
    /// its grip.
    #[must_use]
    pub fn bar_rect(&self) -> Option<Rect> {
        self.bar.rect()
    }

    /// The open windows, back to front.
    #[must_use]
    pub fn stacking(&self) -> &[(String, String)] {
        &self.order
    }

    /// Bring window `(plugin, window)` to the front, as a press on it does.
    pub fn raise(&mut self, plugin: &str, window: &str) {
        let key = (plugin.to_owned(), window.to_owned());
        if self.order.contains(&key) {
            self.order.retain(|k| *k != key);
            self.order.push(key);
        }
    }

    /// What was drawn last frame.
    #[must_use]
    pub fn drawn(&self) -> &Drawn {
        &self.drawn
    }

    /// Whether a press at `(x, y)` lands on the layer, by last frame's drawing.
    #[must_use]
    pub fn hits(&self, x: f32, y: f32) -> bool {
        self.drawn.hits.iter().any(|r| r.contains(x, y))
    }

    /// Whether `(x, y)` is on a part of the layer drawn over everything, by last frame's
    /// drawing: an open choice's list, the host's own windows and its question.
    #[must_use]
    pub fn over_top(&self, x: f32, y: f32) -> bool {
        self.drawn.top_hits.iter().any(|r| r.contains(x, y))
    }

    /// The player's answer to the host's question since the last call.
    pub fn take_reply(&mut self) -> Option<HostReply> {
        self.host.take_reply()
    }

    /// What the player did on the plugins' surfaces and the host's own windows since the last
    /// call.
    pub fn take_inputs(&mut self) -> Vec<crate::Input> {
        let mut out = self.ui.take_inputs();
        out.extend(self.host.ui.take_inputs());
        out
    }

    /// Draw this frame's layer over a screen of `screen` pixels at `scale`, in the skin of
    /// `art`, with `ctx`'s input.
    pub fn frame(
        &mut self,
        art: &Art,
        scale: f32,
        screen: (f32, f32),
        ctx: &mut Ctx<'_>,
        scene: Scene<'_>,
    ) -> &Drawn {
        let mut out = Drawn::default();
        self.ui.retain(art, scene.views);
        self.asking = scene.question.is_some();
        // The host's own windows and its question stand over every plugin window: where they
        // were last frame no plugin surface takes the pointer.
        let mut host_rects = self.host.rects(scene.views);
        if scene.question.is_some() && self.host.asking.id().is_some() {
            host_rects.push(self.host.asking.drawn.frame);
        }
        // The windows open now, back to front: one newly open comes to the front.
        let open: Vec<&WindowView> = scene
            .views
            .iter()
            .filter(|v| {
                v.open
                    && v.tier == Tier::Plugin
                    && v.tree.kind == dereth_plugin_api::NodeKind::Window
            })
            .collect();
        self.order
            .retain(|k| open.iter().any(|v| v.plugin == k.0 && v.window == k.1));
        for v in &open {
            let key = (v.plugin.clone(), v.window.clone());
            if !self.order.contains(&key) {
                self.order.push(key);
            }
        }
        // A press on a window brings it to the front before anything sees the press.
        let rects: Vec<((String, String), Rect)> = self
            .order
            .iter()
            .filter_map(|k| self.ui.rect_of(&k.0, &k.1).map(|r| (k.clone(), r)))
            .collect();
        if ctx.input.pressed[0] || ctx.input.pressed[1] {
            let (mx, my) = ctx.input.mouse;
            let on_host = host_rects.iter().any(|r| r.contains(mx, my));
            let hit = rects.iter().rev().find(|(_, r)| r.contains(mx, my));
            if let (false, Some((k, _))) = (on_host, hit) {
                // Brought to the front of the layer, and of the interface's own windows too.
                out.raised = Some(k.clone());
                self.raise(&k.0, &k.1);
            }
        }
        let rects: Vec<((String, String), Rect)> = self
            .order
            .iter()
            .filter_map(|k| self.ui.rect_of(&k.0, &k.1).map(|r| (k.clone(), r)))
            .collect();
        let interface = std::mem::take(&mut ctx.input.occluders);
        let mut base = interface.clone();
        base.extend(host_rects.iter().copied());
        ctx.input.occluders = base.clone();
        self.ui
            .begin(&painter(&mut out.top, art, scale, screen), ctx);
        // The labels, under everything; they take no pointer.
        glyphs(
            &mut painter(&mut out.labels, art, scale, screen),
            scene.labels,
        );
        // The HUDs, the marks and the bar, under every window.
        ctx.input.occluders = base.clone();
        ctx.input.occluders.extend(rects.iter().map(|(_, r)| *r));
        let under_windows = ctx.input.occluders.clone();
        // A HUD's controls never take the pointer over one of the interface's own.
        ctx.input
            .occluders
            .extend(scene.game_controls.iter().copied());
        let noted = ctx.input.nav.targets.len();
        let mode = crate::HudMode::Placed {
            unlocked: scene.huds_unlocked,
        };
        let mut huds = Vec::new();
        for v in scene.views.iter().filter(|v| {
            v.open && v.tier == Tier::Plugin && v.tree.kind == dereth_plugin_api::NodeKind::Hud
        }) {
            let mut p = painter(&mut out.under, art, scale, screen);
            huds.push(self.ui.hud(&mut p, ctx, v, mode));
        }
        ctx.input.occluders = under_windows;
        out.hits
            .extend(ctx.input.nav.targets[noted..].iter().map(|t| t.rect));
        // Unlocked, a whole HUD is the layer's: a press anywhere on it moves it.
        if scene.huds_unlocked {
            out.hits.extend(huds);
        }
        glyphs(
            &mut painter(&mut out.under, art, scale, screen),
            scene.marks,
        );
        if !scene.bar.is_empty() {
            let mut p = painter(&mut out.under, art, scale, screen);
            let (r, clicked) = self
                .bar
                .draw(&mut p, ctx, scene.bar, scene.bar_at, scene.bar_look);
            if let Some(plugin) = clicked {
                self.ui.push_input(crate::Input::Bar { plugin });
            }
            out.hits.push(r);
        }
        // Each window, back to front, the ones above it taking the pointer where they cover it.
        for (i, key) in self.order.clone().iter().enumerate() {
            let Some(view) = open.iter().find(|v| v.plugin == key.0 && v.window == key.1) else {
                continue;
            };
            ctx.input.occluders = base.clone();
            ctx.input
                .occluders
                .extend(rects.iter().skip(i + 1).map(|(_, r)| *r));
            let mut list = DrawList::default();
            let mut p = painter(&mut list, art, scale, screen);
            if let Some(r) = self.ui.window(&mut p, ctx, view) {
                out.hits.push(r);
                out.windows.push((key.clone(), r, list));
            }
        }
        ctx.input.occluders = base;
        let mut p = painter(&mut out.top, art, scale, screen);
        if let Some(r) = self.ui.choice_rect(&p) {
            out.hits.push(r);
            out.top_hits.push(r);
        }
        self.ui.top(&mut p, ctx);
        // The host's own tier over everything: its windows, then its question.
        ctx.input.occluders = interface;
        let host = self.host.windows(&mut p, ctx, scene.views);
        out.hits.extend(host.iter().copied());
        out.top_hits.extend(host);
        if let Some(r) = self.host.ask(&mut p, ctx, scene.question) {
            out.hits.push(r);
            out.top_hits.push(r);
        }
        self.drawn = out;
        &self.drawn
    }

    /// Whether the host's question is up: Escape and the pad's cancel are its own.
    #[must_use]
    pub fn asking(&self) -> bool {
        self.asking
    }

    /// The interface the layer is drawn over is put away: nothing it drew stands or takes a press
    /// until it is drawn again, and the keyboard and any open choice are let go.
    pub fn put_away(&mut self) {
        self.ui.let_go();
        self.host.ui.let_go();
        self.host.asking.forget();
        self.asking = false;
        self.drawn = Drawn::default();
    }
}

/// A painter over `list` in `art` at `scale` on a screen of `screen` pixels.
fn painter<'a>(
    list: &'a mut DrawList,
    art: &'a Art,
    scale: f32,
    screen: (f32, f32),
) -> Painter<'a> {
    Painter {
        list,
        art,
        scale,
        screen,
        fade: 1.0,
    }
}

/// Draw `glyphs`, in screen pixels: a dot as a square of its colour, a line of text centred on
/// its place.
pub fn glyphs(p: &mut Painter<'_>, glyphs: &[Glyph]) {
    let style = TextStyle::new(Family::Body, 13.0, 0xFFFF_FFFF).edge(p.art.palette().edge);
    for g in glyphs {
        match g {
            Glyph::Dot { x, y, size, color } => {
                #[allow(clippy::cast_precision_loss)]
                let (x, y, s) = (*x as f32, *y as f32, *size as f32);
                p.fill(
                    Rect::new(x - (s / 2.0).floor(), y - (s / 2.0).floor(), s, s),
                    color | 0xFF00_0000,
                );
            }
            Glyph::Text { x, y, text, color } => {
                let w = p.measure(&style, text);
                #[allow(clippy::cast_precision_loss)]
                p.text(
                    &style.colour(color | 0xFF00_0000),
                    (*x as f32 - w / 2.0).round(),
                    *y as f32,
                    text,
                );
            }
            Glyph::Label { x, y, text, color } => {
                let w = p.measure(&style, text);
                let edge = dereth_ui_kit::draw::with_alpha(
                    p.art.palette().edge,
                    f32::from(u8::try_from(color >> 24).unwrap_or(255)) / 255.0,
                );
                #[allow(clippy::cast_precision_loss)]
                p.text(
                    &style.colour(*color).edge(edge),
                    (*x as f32 - w / 2.0).round(),
                    *y as f32,
                    text,
                );
            }
        }
    }
}
