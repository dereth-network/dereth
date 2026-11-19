//! The skill window: one skill panel at a time as a tree, the player's points, and what may be
//! done with the skill selected.
//!
//! - **The kinds of skill** (melee, missile, magic, the rest) are tabs across the top, one for
//!   each the player's race has, each with the game's own short name for its kind; a press shows
//!   that kind's first panel (its own tab, pressed on a later panel, goes back to it). The window
//!   opens on the first kind's first panel, whatever panel it showed before. Under them the
//!   panel's name, Back (the panel shown before this one) and the player's points, the credits
//!   over the experience. A press on the window leaves the selection in the world as it is.
//! - **The tree** grows up from its first row at the bottom, as the game's own does: each node at
//!   its place, hung from its parents by a line. A tree taller than its room scrolls, starting at
//!   the bottom. A skill trained shows its level; one that may be trained or raised is marked
//!   green; one not trained is dim. A node that opens another panel is marked with an arrow.
//!   Pressing a node has the game examine it: a skill is selected, a panel is shown.
//! - **The foot**: the skill selected, its level, and by its state the points it takes; Train,
//!   Raise, Untrain, or Stop untraining, each lit when it may be done. Training a skill whose
//!   training cannot be undone asks first; untraining asks, and so does stopping an untraining;
//!   the question takes the points' line, Yes and No the buttons' places.
//! - **A trained skill that is not passive** can be dragged onto the hotbar.

use auberean_client_sdk::contract::notice::examination::kind as examine;
use auberean_client_sdk::contract::view::skill_node;
use auberean_client_sdk::contract::{Action, SkillAction};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::abilities::{skill_shortcut, Node};
use crate::draw::{Rect, WHITE};
use crate::state::{GameState, Skills};
use crate::ui::hotbar::Drag;
use crate::ui::Tip;

/// The window's body, in layout units.
const BODY: (f32, f32) = (336.0, 480.0);

/// The kinds of skill, as the buttons read when the game's strings do not name them.
pub const KINDS: [&str; 4] = ["Melee", "Missile", "Magic", "Special"];

/// The foot's height, and the room the tree keeps for its scroll bar, in layout units.
pub const FOOT: f32 = 88.0;
pub const SCROLL_ROOM: f32 = 18.0;

/// A node's cell in the tree, and its icon's side, in layout units: six columns, as wide as the
/// game's panels are, fill the tree's room.
pub const CELL: f32 = 50.0;
pub const NODE: f32 = 42.0;

/// The marks: may be trained or raised; the line a node hangs by; the rim of the one selected.
pub const CAN: u32 = 0xFF6C_D85A;
pub const LINE: u32 = 0xFF8A_7A5A;
pub const RIM: u32 = 0xFFFF_D860;

/// A question the window asks before doing something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Question {
    /// Train a skill whose training cannot be undone; untrain one; stop the untraining.
    Train(u32),
    Untrain(u32),
    StopUntraining,
}

impl Question {
    /// What it asks, `name` the skill's.
    #[must_use]
    pub fn text(self, name: &str, perk: bool) -> String {
        match self {
            Self::Train(_) => format!("Training {name} cannot be undone. Train it?"),
            Self::Untrain(_) if perk => format!("Untrain the perk {name}?"),
            Self::Untrain(_) => format!("Untrain {name}?"),
            Self::StopUntraining => "Stop untraining?".into(),
        }
    }

    /// What answering yes asks of the game.
    #[must_use]
    pub const fn action(self) -> Action {
        Action::Skill(match self {
            Self::Train(s) => SkillAction::Train(s),
            Self::Untrain(s) => SkillAction::Untrain(s),
            Self::StopUntraining => SkillAction::CancelUntrain,
        })
    }
}

/// Where the window's parts are, in a window drawn with `body` as its body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    /// The kinds' tabs, the panel's name and Back, the points, the tree, the foot, and the
    /// foot's four buttons (train, raise, untrain, stop untraining) in order; and the question's
    /// yes and no.
    pub tabs: Rect,
    pub title: Rect,
    pub back: Rect,
    pub points: Rect,
    /// The tree's room, which scrolls, and where its nodes stand in it (clear of its scroll bar).
    pub tree: Rect,
    pub nodes: Rect,
    pub foot: Rect,
    pub buttons: [Rect; 4],
    pub yes: Rect,
    pub no: Rect,
}

/// The skill window.
#[derive(Debug, Default)]
pub struct SkillsWindow {
    window: WindowState,
    drawn: Option<(Rect, Rect)>,
    /// The question up, if one is.
    pub question: Option<Question>,
    /// The kind whose tab is lit while a panel that is no kind's first is shown.
    kind: Option<usize>,
    /// The kinds' tabs as last drawn: each kind and its tab.
    kind_tabs: Vec<(usize, Rect)>,
    /// How far the tree is scrolled, from the top of all its rows; it starts at the bottom.
    scroll: f32,
    /// The panel the tree last showed.
    shown: Option<u32>,
    /// Opened, and the first kind's first panel not yet asked for.
    opening: bool,
}

impl SkillsWindow {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.window.open
    }

    pub fn toggle(&mut self) {
        if self.window.open {
            self.close();
        } else {
            self.window.open = true;
            self.opening = true;
        }
    }

    pub fn close(&mut self) {
        self.window.open = false;
        self.drawn = None;
        self.question = None;
    }

    /// Where it was last drawn, while it is open.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Where its body was last drawn, while it is open.
    #[must_use]
    pub fn body(&self) -> Option<Rect> {
        self.drawn.map(|(_, b)| b)
    }

    /// The kinds' tabs as last drawn, each with its kind (0 melee, 1 missile, 2 magic, 3 the
    /// rest); only the kinds the player's race has.
    #[must_use]
    pub fn kind_tabs(&self) -> &[(usize, Rect)] {
        &self.kind_tabs
    }

    /// How far the tree is scrolled up from its bottom, in screen pixels, as last drawn.
    #[must_use]
    pub fn lifted(&self, parts: &Parts, k: f32, nodes: &[Node]) -> f32 {
        (Self::rows_height(parts.nodes, k, nodes) - parts.tree.h).max(0.0) - self.scroll
    }

    /// Where the parts are in a window drawn with `body` as its body, at `k` pixels a layout
    /// unit.
    #[must_use]
    pub fn parts(body: Rect, k: f32) -> Parts {
        let tabs = Rect::new(
            body.x + 8.0 * k,
            body.y + 6.0 * k,
            body.w - 16.0 * k,
            26.0 * k,
        );
        let back = Rect::new(
            body.right() - 98.0 * k,
            tabs.bottom() + 8.0 * k,
            90.0 * k,
            24.0 * k,
        );
        let title = Rect::new(tabs.x, back.y, back.x - tabs.x - 8.0 * k, back.h);
        let points = Rect::new(tabs.x, back.bottom() + 4.0 * k, tabs.w, 32.0 * k);
        let foot = Rect::new(body.x, body.bottom() - FOOT * k, body.w, FOOT * k);
        #[allow(clippy::cast_precision_loss)]
        let button = |i: usize| {
            Rect::new(
                foot.x + (12.0 + i as f32 * 156.0) * k,
                foot.bottom() - 36.0 * k,
                148.0 * k,
                28.0 * k,
            )
        };
        let tree_top = points.bottom() + 6.0 * k;
        let tree = Rect::new(
            body.x + 8.0 * k,
            tree_top,
            body.w - 16.0 * k,
            foot.y - tree_top - 6.0 * k,
        );
        Parts {
            tabs,
            title,
            back,
            points,
            tree,
            nodes: Rect::new(tree.x, tree.y, tree.w - SCROLL_ROOM * k, tree.h),
            foot,
            buttons: [button(0), button(0), button(1), button(0)],
            yes: button(0),
            no: button(1),
        }
    }

    /// A tree's cell over `tree` for the panel's nodes `nodes`: as wide as the tree's room
    /// allows its columns, at most the full cell.
    fn cell(tree: Rect, k: f32, nodes: &[Node]) -> (f32, f32, f32) {
        let (cols, rows) = nodes
            .iter()
            .filter(|n| n.x >= 0 && n.y >= 0)
            .fold((1, 1), |(c, r), n| (c.max(n.x + 1), r.max(n.y + 1)));
        #[allow(clippy::cast_precision_loss)]
        let (cols, rows) = (cols as f32, rows as f32);
        ((CELL * k).min(tree.w / cols), cols, rows)
    }

    /// How tall all the tree's rows are, in screen pixels.
    #[must_use]
    pub fn rows_height(tree: Rect, k: f32, nodes: &[Node]) -> f32 {
        let (cell, _, rows) = Self::cell(tree, k, nodes);
        rows * cell
    }

    /// Where `node` is drawn in a tree over `tree`, the panel's nodes `nodes`, scrolled to its
    /// bottom: the tree as wide as its nodes reach, set in the middle (its cells shrunk to fit
    /// across), its first row at the bottom.
    #[must_use]
    pub fn node_rect(tree: Rect, k: f32, nodes: &[Node], node: &Node) -> Rect {
        Self::node_rect_lifted(tree, k, nodes, node, 0.0)
    }

    /// Where `node` is drawn in a tree over `tree` scrolled up from its bottom by `lifted` screen
    /// pixels.
    #[must_use]
    pub fn node_rect_lifted(tree: Rect, k: f32, nodes: &[Node], node: &Node, lifted: f32) -> Rect {
        let (cell, cols, _) = Self::cell(tree, k, nodes);
        let side = cell * NODE / CELL;
        let x0 = tree.x + (tree.w - cols * cell) / 2.0;
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (node.x as f32, node.y as f32);
        let top = tree.bottom() + lifted - (y + 1.0) * cell;
        Rect::new(
            (x0 + x * cell + (cell - side) / 2.0).round(),
            (top + (cell - side) / 2.0).round(),
            side.round(),
            side.round(),
        )
    }

    /// Draw it while it is open; where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        drag: &mut Option<Drag>,
        tip: &mut Option<Tip>,
        actions: &mut Vec<Action>,
    ) -> Option<Rect> {
        if !self.window.open {
            return None;
        }
        let k = p.scale;
        let size = kit::window_size_for(p, BODY);
        let at = (
            (p.screen.0 * 0.22 - size.0 * k / 2.0).round().max(0.0),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let at = crate::ui::place::clear(p, state, size, at);
        let w = kit::window(p, ctx, &mut self.window, "Skills", size, at);
        if w.closed {
            self.close();
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        let parts = Self::parts(w.body, k);
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let Some(s) = &state.skills else {
            p.text_in(&body, w.body, Align::Centre, "No skills yet.");
            return Some(w.rect);
        };
        let selection = state.target.as_ref().map_or(0, |t| t.id);
        if self.opening {
            self.open_first(s, selection, actions);
        }
        self.kinds(p, ctx, s, &parts, selection, actions);
        self.tree(p, ctx, s, &parts, selection, drag, tip, actions);
        self.foot(p, ctx, s, &parts, actions);
        Some(w.rect)
    }

    /// Just opened: the first kind's first panel shown, at its bottom, the game asked for it
    /// unless it shows already. `selection` the world's selection, kept.
    fn open_first(&mut self, s: &Skills, selection: u64, actions: &mut Vec<Action>) {
        let Some((i, root)) = s
            .roots
            .iter()
            .enumerate()
            .find_map(|(i, r)| r.as_ref().map(|(id, _, _)| (i, *id)))
        else {
            return;
        };
        self.opening = false;
        self.kind = Some(i);
        self.shown = None;
        if s.panel != root {
            self.question = None;
            actions.push(show_panel(root, selection));
        }
    }

    /// The kinds' buttons, Back and the points; `selection` the world's selection, kept.
    fn kinds(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        s: &Skills,
        parts: &Parts,
        selection: u64,
        actions: &mut Vec<Action>,
    ) {
        let shown: Vec<usize> = (0..KINDS.len()).filter(|i| s.roots[*i].is_some()).collect();
        let root = |i: usize| s.roots[i].as_ref().map(|(id, _, _)| *id);
        if let Some(&i) = shown.iter().find(|&&i| root(i) == Some(s.panel)) {
            self.kind = Some(i);
        }
        let mut lit = self
            .kind
            .and_then(|k| shown.iter().position(|&i| i == k))
            .unwrap_or(0);
        let labels: Vec<&str> = shown.iter().map(|&i| kind_label(s, i)).collect();
        let rects = kit::tab_rects_for(p, parts.tabs, &labels, lit);
        self.kind_tabs = shown.iter().copied().zip(rects.iter().copied()).collect();
        // The lit tab pressed on a later panel goes back to its kind's first.
        let again = rects
            .get(lit)
            .is_some_and(|r| ctx.over_quiet(r) && ctx.input.clicked(r));
        let before = lit;
        if kit::tabs(p, ctx, parts.tabs, &labels, &mut lit) || again {
            if let Some(id) = shown.get(lit).and_then(|&i| root(i)) {
                if lit != before || id != s.panel {
                    self.question = None;
                    self.kind = Some(shown[lit]);
                    actions.push(show_panel(id, selection));
                }
            }
        }
        let title =
            TextStyle::new(Family::Heading, 15.0, ctx.colours.heading()).edge(ctx.colours.edge());
        let name = p.fit(&title, &s.panel_name, parts.title.w);
        p.text_in(&title, parts.title, Align::Left, &name);
        if let Some(&before) = s.history.first() {
            if kit::button(p, ctx, parts.back, "Back", true) {
                self.question = None;
                actions.push(show_panel(before, selection));
            }
        }
        let points = TextStyle::new(Family::Body, 12.0, 0xFFE8_E0D0).edge(0xFF10_0C08);
        // The credits, and the experience under them.
        let r = parts.points;
        let half = r.h / 2.0;
        for (i, line) in Self::points_lines(s).iter().enumerate() {
            let line = p.fit(&points, line, r.w);
            #[allow(clippy::cast_precision_loss)]
            let at = Rect::new(r.x, r.y + i as f32 * half, r.w, half);
            p.text_in(&points, at, Align::Left, &line);
        }
    }

    /// The player's points as the window writes them, a line each: the credits, then the
    /// experience.
    #[must_use]
    pub fn points_lines(s: &Skills) -> [String; 2] {
        [
            format!(
                "Skill Credits {}    Hero Skill Credits {}",
                s.credits, s.hero_credits
            ),
            format!("Available XP {}", grouped(s.experience)),
        ]
    }

    /// The tree: the lines, then the nodes; `selection` the world's selection, kept.
    #[allow(clippy::too_many_arguments)]
    fn tree(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        s: &Skills,
        parts: &Parts,
        selection: u64,
        drag: &mut Option<Drag>,
        tip: &mut Option<Tip>,
        actions: &mut Vec<Action>,
    ) {
        let k = p.scale;
        if self.shown != Some(s.panel) {
            self.shown = Some(s.panel);
            self.scroll = f32::MAX;
        }
        let (tree, room) = (parts.tree, parts.nodes);
        let height = Self::rows_height(room, k, &s.nodes);
        let offset = kit::scroll(p, ctx, tree, height, &mut self.scroll);
        self.scroll = offset;
        let lifted = (height - tree.h).max(0.0) - offset;
        let placed: Vec<&Node> = s
            .nodes
            .iter()
            .filter(|n| n.x >= 0 && n.y >= 0)
            .filter(|n| {
                let r = Self::node_rect_lifted(room, k, &s.nodes, n, lifted);
                r.bottom() > tree.y && r.y < tree.bottom()
            })
            .collect();
        let at = |n: &Node| Self::node_rect_lifted(room, k, &s.nodes, n, lifted);
        // A press above or below the tree's room is not on a node scrolled out of it.
        let inside = tree.contains(ctx.input.mouse.0, ctx.input.mouse.1);
        p.list.push_clip(tree);
        let t = (2.0 * k).max(1.0);
        let all: Vec<&Node> = s.nodes.iter().filter(|n| n.x >= 0 && n.y >= 0).collect();
        for n in &all {
            let r = at(n);
            for parent in n.parents.iter().filter_map(|id| s.node(*id)) {
                if parent.x < 0 || parent.y < 0 {
                    continue;
                }
                let q = at(parent);
                // Up from the parent, across, and up to the child above it.
                let (px, cx) = (q.x + q.w / 2.0, r.x + r.w / 2.0);
                let mid = (r.bottom() + q.y) / 2.0;
                p.fill(Rect::new(px - t / 2.0, mid, t, (q.y - mid).max(0.0)), LINE);
                p.fill(
                    Rect::new(px.min(cx) - t / 2.0, mid - t / 2.0, (px - cx).abs() + t, t),
                    LINE,
                );
                p.fill(
                    Rect::new(cx - t / 2.0, r.bottom(), t, (mid - r.bottom()).max(0.0)),
                    LINE,
                );
            }
        }
        let level = TextStyle::new(Family::Numerals, 12.0, WHITE).edge(0xFF00_0000);
        for n in placed {
            let r = at(n);
            let state = s.states.get(&n.skill).copied().unwrap_or_default();
            let icon = p.art.icon(n.icon);
            let dim = n.is_skill() && !state.trained && state.can_train != 0;
            let pressed = kit::icon_slot(
                p,
                ctx,
                r,
                icon.as_ref(),
                if dim { 0x80FF_FFFF } else { WHITE },
            ) && inside;
            if icon.is_none() {
                p.fill(r.inset(4.0 * k), 0x6030_3840);
            }
            if n.is_skill() {
                let can = if state.trained {
                    state.can_raise == 0
                } else {
                    state.can_train == 0
                };
                if can {
                    p.fill(
                        Rect::new(r.right() - 9.0 * k, r.y + 2.0 * k, 7.0 * k, 7.0 * k),
                        CAN,
                    );
                }
                if state.trained {
                    p.text_in(
                        &level,
                        Rect::new(r.x, r.bottom() - 14.0 * k, r.w - 3.0 * k, 13.0 * k),
                        Align::Right,
                        &state.level.to_string(),
                    );
                }
            } else {
                p.text_in(&level, r, Align::Centre, ">");
            }
            if n.skill == s.selected && n.is_skill() {
                p.outline(r, 2.0 * k, RIM);
            }
            if inside && ctx.over_quiet(&r) && drag.is_none() {
                let mut lines = Vec::new();
                if n.is_skill() && state.trained {
                    lines.push(format!("Level {}", state.level));
                }
                *tip = Some(Tip {
                    title: n.name.clone(),
                    lines,
                });
            }
            if pressed {
                self.question = None;
                actions.push(examine_node(n, s, selection));
                if n.is_skill() && state.trained && !n.passive && drag.is_none() {
                    *drag = Some(Drag {
                        shortcut: skill_shortcut(n.skill),
                        from: None,
                        icon: n.icon,
                        origin: ctx.input.mouse,
                        active: false,
                    });
                }
            }
        }
        p.list.pop_clip();
    }

    /// The foot: the skill selected and what may be done with it, or the question up.
    fn foot(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        s: &Skills,
        parts: &Parts,
        actions: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let f = parts.foot;
        p.fill(
            Rect::new(f.x + 8.0 * k, f.y, f.w - 16.0 * k, 1.0_f32.max(k)),
            LINE,
        );
        let name = TextStyle::new(Family::Heading, 15.0, 0xFFF4_E8C8).edge(0xFF1A_1008);
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let Some(node) = s.node(s.selected).filter(|n| n.is_skill()) else {
            return;
        };
        let state = s.states.get(&node.skill).copied().unwrap_or_default();
        let icon = Rect::new(f.x + 12.0 * k, f.y + 8.0 * k, 36.0 * k, 36.0 * k);
        let text_x = icon.right() + 10.0 * k;
        let room = f.right() - 12.0 * k - text_x;
        if let Some(i) = p.art.icon(node.icon) {
            p.sprite(&i, icon, WHITE);
        }
        let title = if state.trained {
            format!("{}  ·  Level {}", node.name, state.level)
        } else {
            node.name.clone()
        };
        let title = p.fit(&name, &title, room);
        p.text(&name, text_x, f.y + 8.0 * k, &title);
        let (credits, credit_name) = if state.hero {
            (s.hero_credits, "Hero Skill Credits")
        } else {
            (s.credits, "Skill Credits")
        };
        let line = if state.being_untrained {
            format!(
                "Available XP {}    In This Skill {}",
                grouped(s.experience),
                grouped(state.experience)
            )
        } else if state.trained {
            format!(
                "Available XP {}    Cost to Raise {}",
                grouped(s.experience),
                grouped(state.experience_to_raise)
            )
        } else {
            format!(
                "{credit_name} {credits}    Cost to Train {}",
                state.cost_to_train
            )
        };
        // A question up takes the points' line.
        let line = self
            .question
            .map_or(line, |q| q.text(&node.name, node.perk));
        let line = p.fit(&body, &line, room);
        p.text(&body, text_x, f.y + 28.0 * k, &line);
        if let Some(q) = self.question {
            if kit::button(p, ctx, parts.yes, "Yes", true) {
                actions.push(q.action());
                self.question = None;
            } else if kit::button(p, ctx, parts.no, "No", true) {
                self.question = None;
            }
            return;
        }
        let skill = node.skill;
        let [train, raise, untrain, stop] = parts.buttons;
        if !state.trained && kit::button(p, ctx, train, "Train", state.can_train == 0) {
            if node.untrainable {
                actions.push(Action::Skill(SkillAction::Train(skill)));
            } else {
                self.question = Some(Question::Train(skill));
            }
        }
        if state.trained && !state.being_untrained {
            if kit::button(p, ctx, raise, "Raise", state.can_raise == 0) {
                actions.push(Action::Skill(SkillAction::Raise(skill)));
            }
            if kit::button(p, ctx, untrain, "Untrain", state.can_untrain == 0) {
                self.question = Some(Question::Untrain(skill));
            }
        }
        if state.being_untrained && kit::button(p, ctx, stop, "Stop Untraining", true) {
            self.question = Some(Question::StopUntraining);
        }
    }
}

/// What the button of the `i`th kind of skill reads: the game's own short name for the kind, or
/// [`KINDS`]' while the game's strings do not name it.
#[must_use]
pub fn kind_label(s: &Skills, i: usize) -> &str {
    s.labels
        .get(i)
        .map(String::as_str)
        .filter(|l| !l.is_empty())
        .unwrap_or(KINDS[i])
}

/// Have the game show a skill panel, `selection` (the world's selection) kept.
#[must_use]
pub const fn show_panel(panel: u32, selection: u64) -> Action {
    Action::Examine {
        kind: examine::PANEL,
        data_enum: panel,
        item: 0,
        data_id: 0,
        selection,
    }
}

/// Have the game examine a node: a skill, or the panel it opens (the node going back, the panel
/// shown before); `selection` (the world's selection) kept.
#[must_use]
pub fn examine_node(n: &Node, s: &Skills, selection: u64) -> Action {
    if n.is_skill() {
        return Action::Examine {
            kind: examine::SKILL,
            data_enum: n.skill,
            item: 0,
            data_id: 0,
            selection,
        };
    }
    let panel = if n.skill == skill_node::BACK {
        s.history.first().copied().unwrap_or(s.panel)
    } else {
        n.skill
    };
    show_panel(panel, selection)
}

/// A number with its thousands set apart by commas.
#[must_use]
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
