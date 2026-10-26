//! The plugins' windows, HUDs, bar and marks, seated in the interface: each plugin window is one
//! of the interface's own windows ([`crate::ui::panels::WindowId::Plugin`]), framed, stacked,
//! raised and closed like the others, so the pad's focus, its cancel, the shoulders' tabs and the
//! on-screen keyboard work on it as on any window; its HUDs stand among the HUD's elements; its
//! marks are drawn on the minimap and the map. The drawing of each window's nodes is the plugin
//! presentation's ([`dereth_plugin_ui::PluginUi`]), in the interface's own art.

use dereth_plugin_ui::bar::{Bar, BarLook, BarPlace};
use dereth_plugin_ui::{BarItem, Input, Marks, PluginUi, Tier, WindowView};

use crate::draw::Rect;
use crate::ui::kit::Ctx;
use crate::ui::paint::Painter;

/// What the host hands the interface of the plugins each frame.
#[derive(Debug, Clone, Default)]
pub struct PluginFrame {
    /// Every window and HUD the host keeps, open or not.
    pub views: Vec<WindowView>,
    /// The plugin bar's buttons, and where the player left it.
    pub bar: Vec<BarItem>,
    pub bar_at: BarPlace,
    /// The marks: labels in the world, blips, map points and lines, the arrow.
    pub marks: Marks,
    /// The commands the plugins claimed, which a cross hotbar slot can hold.
    pub commands: Vec<PluginCommandEntry>,
    /// The player has unlocked the HUDs: a press on one moves it.
    pub huds_unlocked: bool,
}

/// One command a plugin claimed: its plugin, its verb and what it says it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommandEntry {
    pub plugin: String,
    pub verb: String,
    pub help: String,
}

/// The plugins' share of the interface.
#[derive(Debug, Default)]
pub struct PluginSeat {
    /// The plugin windows' presentation state.
    pub ui: PluginUi,
    /// This frame's windows, HUDs, bar and marks.
    pub frame: PluginFrame,
    /// Each plugin window's place in the window order, by `(plugin, window)`: the number its
    /// window id carries.
    keys: Vec<(String, String)>,
    /// What the player did on them that the presentation does not see (the pad's menu).
    inputs: Vec<Input>,
    /// The plugin bar.
    bar: Bar,
    /// Where the HUDs' controls and the bar were last frame.
    pub hits: Vec<Rect>,
    /// The HUD the player is moving while the HUDs are unlocked, by its name in the layout, and
    /// where on it the press took hold.
    moving: Option<(String, (f32, f32))>,
    /// Where the plugin windows over the screens before the world were last frame: over the
    /// screen's own controls.
    pregame_over: Vec<Rect>,
    /// The plugin windows over the screens before the world, where the interface has no windows
    /// of its own.
    pub layer: dereth_plugin_ui::Layer,
    /// The plugin host's own tier over everything: its windows (the plugin panel) and its
    /// question.
    pub host: dereth_plugin_ui::host::HostTier,
}

impl PluginSeat {
    /// The number of window `(plugin, window)` in the window order, given one the first time.
    pub fn number_of(&mut self, plugin: &str, window: &str) -> u32 {
        let at = self
            .keys
            .iter()
            .position(|k| k.0 == plugin && k.1 == window)
            .unwrap_or_else(|| {
                self.keys.push((plugin.to_owned(), window.to_owned()));
                self.keys.len() - 1
            });
        u32::try_from(at).unwrap_or(u32::MAX)
    }

    /// The window numbered `n`: its view this frame, if it is open.
    #[must_use]
    pub fn view(&self, n: u32) -> Option<&WindowView> {
        let (plugin, window) = self.keys.get(n as usize)?;
        self.frame
            .views
            .iter()
            .find(|v| v.open && &v.plugin == plugin && &v.window == window)
    }

    /// The plugin windows open this frame, by their numbers.
    pub fn open_windows(&mut self) -> Vec<u32> {
        let open: Vec<(String, String)> = self
            .frame
            .views
            .iter()
            .filter(|v| {
                v.open
                    && v.tier == Tier::Plugin
                    && v.tree.kind == dereth_plugin_api::NodeKind::Window
            })
            .map(|v| (v.plugin.clone(), v.window.clone()))
            .collect();
        open.iter().map(|(p, w)| self.number_of(p, w)).collect()
    }

    /// Note something the player did on a plugin surface.
    pub fn push_input(&mut self, input: Input) {
        self.inputs.push(input);
    }

    /// What the player did on the plugins' surfaces since the last call.
    pub fn take_inputs(&mut self) -> Vec<Input> {
        let mut out = std::mem::take(&mut self.inputs);
        out.extend(self.ui.take_inputs());
        out.extend(self.host.ui.take_inputs());
        out
    }

    /// Where the bar stands and the width the player gave it, once drawn.
    #[must_use]
    pub fn bar_place(&self) -> Option<BarPlace> {
        self.bar.place()
    }

    /// The labels in the world: under everything the interface draws.
    pub fn labels(&self, p: &mut Painter<'_>) {
        dereth_plugin_ui::layer::glyphs(p, &self.frame.marks.labels);
    }

    /// Where the plugin windows over the screens before the world stood last frame (and an open
    /// choice's list): the screen's own controls take no pointer there.
    #[must_use]
    pub fn pregame_over(&self) -> &[Rect] {
        &self.pregame_over
    }

    /// The plugins over the screens before the world: their windows, HUDs and bar drawn as a
    /// layer over the screen, in the interface's own art. A HUD's controls take no pointer over
    /// the screen's own `game` controls.
    pub fn pregame(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, game: &[Rect]) {
        self.pregame_over.clear();
        if self.frame.views.iter().all(|v| !v.open) && self.frame.bar.is_empty() {
            return;
        }
        // The host's own windows are its own tier's, drawn over everything at the end.
        let views: Vec<WindowView> = self
            .frame
            .views
            .iter()
            .filter(|v| v.tier == Tier::Plugin)
            .cloned()
            .collect();
        let drawn = self.layer.frame(
            p.art,
            p.scale,
            p.screen,
            &mut ctx.kit,
            dereth_plugin_ui::Scene {
                views: &views,
                bar: &self.frame.bar,
                bar_at: self.frame.bar_at,
                marks: &[],
                labels: &[],
                question: None,
                bar_look: BarLook::Kit,
                game_controls: game,
                huds_unlocked: self.frame.huds_unlocked,
            },
        );
        self.pregame_over = drawn
            .windows
            .iter()
            .map(|w| w.1)
            .chain(drawn.top_hits.iter().copied())
            .collect();
        for list in std::iter::once(&drawn.labels)
            .chain(std::iter::once(&drawn.under))
            .chain(drawn.windows.iter().map(|w| &w.2))
            .chain(std::iter::once(&drawn.top))
        {
            p.list.quads.extend(list.quads.iter().copied());
        }
        if drawn
            .hits
            .iter()
            .any(|r| r.contains(ctx.input.mouse.0, ctx.input.mouse.1))
        {
            ctx.hot = true;
        }
        let inputs = self.layer.ui.take_inputs();
        self.inputs.extend(inputs);
    }

    /// The HUDs, the bar and the arrow, among the HUD's elements and under every window. Each
    /// HUD is an element of the HUD's `layout`: moved and scaled where the player put it, and
    /// outlined for the layout's editor (while `editing`, it takes no pointer). Where the pointer
    /// is over a window or over one of the HUD's own `game` elements and controls, no HUD takes
    /// it: a click there is the window's or the game's, a plugin's HUD drawn over it or not.
    /// While the player has the HUDs unlocked, a press on one moves it (its place kept in the
    /// layout, at once), it is outlined, and its controls take no pointer.
    pub fn huds(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        layout: &mut crate::ui::layout::HudLayout,
        editing: bool,
        game: &[Rect],
    ) {
        // Weighed where the pointer is on the screen, before a HUD's placement lends it the
        // pointer in its own frame.
        let (mx, my) = ctx.input.mouse;
        let covered = ctx
            .input
            .occluders
            .iter()
            .chain(game)
            .any(|r| r.contains(mx, my));
        let unlocked = self.frame.huds_unlocked;
        let mut rects = Vec::new();
        let mut placed = Vec::new();
        let noted = ctx.input.nav.targets.len();
        let views: Vec<WindowView> = self
            .frame
            .views
            .iter()
            .filter(|v| {
                v.open && v.tier == Tier::Plugin && v.tree.kind == dereth_plugin_api::NodeKind::Hud
            })
            .cloned()
            .collect();
        self.ui.retain(p.art, &self.frame.views);
        for v in &views {
            let name = crate::ui::layout::plugin_hud(&v.plugin, &v.window);
            let first = ctx.input.nav.targets.len();
            let open = layout.begin(&name, p, ctx, editing || covered || unlocked);
            let r = self
                .ui
                .hud(p, &mut ctx.kit, v, dereth_plugin_ui::HudMode::Anchored);
            layout.end(open, p, ctx);
            // Its controls are where the layout put them, for the pad's focus and the pointer.
            let k = p.scale;
            for t in ctx.input.nav.targets.iter_mut().skip(first) {
                t.rect = layout.map_rect(&name, t.rect, k);
            }
            let shown = layout.map_rect(&name, r, k);
            rects.push(shown);
            placed.push((name, shown));
        }
        if unlocked && !editing {
            self.move_huds(p, ctx, layout, &placed, covered);
        } else {
            self.moving = None;
        }
        if let Some(arrow) = &self.frame.marks.arrow {
            let cx = dereth_primitives::num::to_i32(p.screen.0 / 2.0);
            let cy = dereth_primitives::num::to_i32(96.0 * p.scale);
            dereth_plugin_ui::layer::glyphs(
                p,
                &dereth_plugin_ui::marks::arrow_mark_glyphs(arrow, cx, cy),
            );
        }
        // The HUDs' controls and the bar take the pointer over the HUD's own elements; unlocked,
        // the whole of each HUD does.
        self.hits = ctx.input.nav.targets[noted.min(ctx.input.nav.targets.len())..]
            .iter()
            .map(|t| t.rect)
            .filter(|t| rects.iter().any(|r| r.contains(t.x + 1.0, t.y + 1.0)))
            .collect();
        if unlocked {
            self.hits.extend(rects.iter().copied());
        }
        if !self.frame.bar.is_empty() {
            let (bar, clicked) = self.bar.draw(
                p,
                &mut ctx.kit,
                &self.frame.bar,
                self.frame.bar_at,
                BarLook::Kit,
            );
            if let Some(plugin) = clicked {
                self.inputs.push(Input::Bar { plugin });
            }
            self.hits.push(bar);
        }
    }

    /// The HUDs unlocked: each outlined where it is shown (`placed`, by its name in the layout),
    /// a press on one taking hold of it and a drag moving it; let go, the layout is kept, so it
    /// stands there again for this character. The layout keeps it on the screen.
    fn move_huds(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        layout: &mut crate::ui::layout::HudLayout,
        placed: &[(String, Rect)],
        covered: bool,
    ) {
        let k = p.scale.max(0.01);
        let (mx, my) = ctx.input.mouse;
        let colour = p.art.palette().label_lit | 0xFF00_0000;
        let line = 1.0_f32.max(k.floor());
        for (name, r) in placed {
            for side in [
                Rect::new(r.x, r.y, r.w, line),
                Rect::new(r.x, r.bottom() - line, r.w, line),
                Rect::new(r.x, r.y, line, r.h),
                Rect::new(r.right() - line, r.y, line, r.h),
            ] {
                p.fill(side, colour);
            }
            if self.moving.is_none()
                && !covered
                && ctx.input.pressed[0]
                && !ctx.input.captured
                && r.contains(mx, my)
            {
                self.moving = Some((name.clone(), (mx - r.x, my - r.y)));
                ctx.input.pressed[0] = false;
                ctx.input.captured = true;
            }
        }
        let Some((name, (gx, gy))) = self.moving.clone() else {
            return;
        };
        if ctx.input.down[0] {
            if let Some((_, r)) = placed.iter().find(|(n, _)| *n == name) {
                let pl = layout.placements.entry(name).or_default();
                pl.dx += (mx - gx - r.x) / k;
                pl.dy += (my - gy - r.y) / k;
            }
            ctx.input.captured = true;
        } else {
            self.moving = None;
            if let Err(e) = layout.save() {
                tracing::warn!("the HUD's place was not kept: {e}");
            }
        }
    }
}
