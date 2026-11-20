//! The HUD layout: where the player has moved each HUD element and how large they have made it,
//! and the editor that changes it (HUD layout, in the settings window).
//!
//! Every element draws at its default place. The layout then moves and scales what it drew, about
//! the element's own top-left corner, and lends the element the pointer in its own frame while it
//! draws, so its buttons still answer where they are shown. Over a window the element sees no
//! pointer at all, so a press there is the window's. An element's outline in the editor is the box
//! around what it drew. Whatever its place and size, an element stays on the screen.
//!
//! The editor outlines every element and names it, one that shows nothing now too (the effects
//! with none, the target bar with nothing selected, the party list out of a fellowship): its
//! outline is then the place it would fill. A drag moves one, the wheel over the one selected
//! resizes it. A name stands whole on the screen outside its element's outline, so it
//! never covers the element's own text: above it, else below, left or right, the first of those
//! clear of every other outline too ([`label_place`]). What it changes shows at once; Save keeps it for this character, and
//! closing the editor with changes unsaved puts back the layout last kept. A layout is kept per
//! world and character in one of the player's files ([`file_name`]): a line per element moved,
//! `name x y scale edges`. Save keeps each element by the screen's edges it stands nearest, across
//! and down (`l`, `c` or `r`, then `t`, `m` or `b`: left, middle or right, top, middle or bottom),
//! and `x y` is how far it stands from them in layout units, so a layout holds on a screen of any
//! shape: an element by the right edge stays by it, one in the middle stays in the middle. A line
//! with no edges, `name dx dy scale`, is an offset from the element's default place, as layouts
//! were first kept. A character with no file of its own starts from the layout Horizon ships
//! ([`SHIPPED`], written the same way), and Reset puts that one back.

use std::collections::BTreeMap;

use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::Rect;

/// The HUD's elements, by their names in the layout file, in drawing order.
pub mod element {
    pub const PARAMETER: &str = "parameter";
    pub const FOCUS: &str = "focus";
    pub const TARGET: &str = "target";
    pub const LINK: &str = "link";
    pub const STATUS: &str = "status";
    pub const LAMP: &str = "lamp";
    pub const EXPERIENCE: &str = "experience";
    pub const PARTY: &str = "party";
    pub const MENU: &str = "menu";
    pub const CHAT: &str = "chat";
    pub const HOTBAR: &str = "hotbar";
    pub const CROSS: &str = "cross";
    pub const MINIMAP: &str = "minimap";
    pub const PETS: &str = "pets";
    pub const DISMOUNT: &str = "dismount";

    /// Every element, in drawing order.
    pub const ALL: [&str; 15] = [
        PARAMETER, FOCUS, TARGET, LINK, STATUS, LAMP, EXPERIENCE, PARTY, MENU, CHAT, HOTBAR, CROSS,
        MINIMAP, PETS, DISMOUNT,
    ];
}

/// The smallest and largest an element may be made.
pub const SCALE_RANGE: (f32, f32) = (0.6, 2.0);

/// How much a notch of the wheel resizes the element selected.
pub const SCALE_STEP: f32 = 0.1;

/// The editor's window body, in layout units.
const BODY: (f32, f32) = (360.0, 96.0);

/// How far a span from `at`, `len` long, must move to lie within `0..screen`; a span longer than
/// the screen keeps its start on it.
fn on_screen(at: f32, len: f32, screen: f32) -> f32 {
    if at < 0.0 || len >= screen {
        -at
    } else if at + len > screen {
        screen - (at + len)
    } else {
        0.0
    }
}

/// A screen's edge, or its middle, that an element keeps to along one way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// The left, or the top.
    Start,
    Middle,
    /// The right, or the bottom.
    End,
}

/// The edges an element keeps to: across, and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edges {
    pub across: Edge,
    pub down: Edge,
}

impl Edges {
    /// As a layout file writes them: `l`, `c` or `r`, then `t`, `m` or `b`.
    #[must_use]
    pub fn letters(self) -> String {
        let across = match self.across {
            Edge::Start => 'l',
            Edge::Middle => 'c',
            Edge::End => 'r',
        };
        let down = match self.down {
            Edge::Start => 't',
            Edge::Middle => 'm',
            Edge::End => 'b',
        };
        format!("{across}{down}")
    }

    /// Read from a layout file's letters.
    #[must_use]
    pub fn read(letters: &str) -> Option<Self> {
        let mut c = letters.chars();
        let across = match c.next()? {
            'l' => Edge::Start,
            'c' => Edge::Middle,
            'r' => Edge::End,
            _ => return None,
        };
        let down = match c.next()? {
            't' => Edge::Start,
            'm' => Edge::Middle,
            'b' => Edge::End,
            _ => return None,
        };
        c.next().is_none().then_some(Self { across, down })
    }
}

/// Where one element is, and how large. With no `edges`, moved by `(dx, dy)` layout units from its
/// default place; with them, standing `dx` from the edge it keeps to across and `dy` from the one
/// down, in layout units: in from the left or the right, down from the top or up from the bottom,
/// and from the middle rightward or downward. Either way scaled about its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    pub edges: Option<Edges>,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            dx: 0.0,
            dy: 0.0,
            scale: 1.0,
            edges: None,
        }
    }
}

/// Where a span `len` long starts on a screen `screen` long, standing `gap` layout units from
/// `edge`, at `k` pixels a layout unit.
fn start_from(edge: Edge, gap: f32, len: f32, screen: f32, k: f32) -> f32 {
    match edge {
        Edge::Start => gap * k,
        Edge::Middle => screen / 2.0 + gap * k - len / 2.0,
        Edge::End => screen - gap * k - len,
    }
}

/// The edge, or the middle, a span from `at`, `len` long, stands nearest on a screen `screen`
/// long, and how far from it it stands in layout units at `k` pixels a layout unit.
fn nearest(at: f32, len: f32, screen: f32, k: f32) -> (Edge, f32) {
    [
        (Edge::Start, at / k),
        (Edge::Middle, (at + len / 2.0 - screen / 2.0) / k),
        (Edge::End, (screen - at - len) / k),
    ]
    .into_iter()
    .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
    .unwrap_or((Edge::Start, at / k))
}

impl Placement {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// An element being drawn: where its quads begin in the list, and the pointer it was lent.
#[derive(Debug)]
pub struct Open {
    name: &'static str,
    first_quad: usize,
    mouse: (f32, f32),
}

/// Where the editor's controls are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorControls {
    pub save: Rect,
    pub reset: Rect,
    pub close: Rect,
}

/// What the editor's buttons were pressed for, for what else the editor places (the log window's
/// tabs popped out) to follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pressed {
    Save,
    Reset,
}

/// The layout Horizon ships, as a layout file's text: what a character with no file of its own
/// starts from, and what Reset puts back.
pub const SHIPPED: &str = include_str!("default-hud.txt");

/// The layout a character starts from: the shipped one, unless another is given.
#[derive(Debug, Clone, PartialEq)]
struct StartingLayout(BTreeMap<String, Placement>);

impl Default for StartingLayout {
    fn default() -> Self {
        Self(parse(SHIPPED))
    }
}

/// The layout and its editor.
#[derive(Debug, Default)]
pub struct HudLayout {
    pub placements: BTreeMap<String, Placement>,
    /// The layout a character with no file of its own starts from, and Reset puts back.
    default: StartingLayout,
    /// Each element's outline this frame, after its placement.
    pub outlines: Vec<(&'static str, Rect)>,
    /// Where the editor wrote each element's name this frame, while it is open.
    pub labels: Vec<(&'static str, Rect)>,
    /// The element the editor has selected, and where in it the drag took hold.
    pub selected: Option<&'static str>,
    grab: Option<(f32, f32)>,
    /// Each element's top-left corner as it drew itself, before its placement: what it is moved
    /// and scaled about.
    pivots: BTreeMap<&'static str, (f32, f32)>,
    /// How far each element was moved, after its placement, to keep it on the screen.
    nudges: BTreeMap<&'static str, (f32, f32)>,
    /// How far each element was moved from its default place this frame, in layout units: its
    /// placement's offset, or where its edges put it.
    offsets: BTreeMap<&'static str, (f32, f32)>,
    /// The screen the elements were last drawn on, and its pixels a layout unit.
    screen: (f32, f32),
    k: f32,
    /// The placements as last read or saved.
    saved: BTreeMap<String, Placement>,
    /// The file the layout is kept in: this character's, once known.
    pub file: Option<String>,
    /// The file's text to write, once Save is pressed.
    to_write: Option<String>,
    /// The editor is open.
    editing: bool,
    /// What else the editor places has changes not saved: Save is offered for them too.
    pub others_unsaved: bool,
    /// The editor's Save or Reset, pressed this frame.
    pressed: Option<Pressed>,
    window: WindowState,
    /// Where the editor's window was last drawn, while it is open.
    drawn: Option<(Rect, Rect)>,
}

/// The file, among the player's, a character's layout is kept in: by its world and its name, as
/// two worlds may each have a character of one name. Anything but an ASCII letter or digit is
/// written as an underscore.
#[must_use]
pub fn file_name(world: &str, name: &str) -> String {
    format!("auberean-horizon-hud-{}-{}.txt", safe(world), safe(name))
}

fn safe(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

impl HudLayout {
    /// A layout whose default, for a character with no file of its own, is the layout file text
    /// `text` instead of the shipped one.
    #[must_use]
    pub fn with_default(text: &str) -> Self {
        Self {
            default: StartingLayout(parse(text)),
            ..Self::default()
        }
    }

    /// The layout a character with no file of its own starts from.
    #[must_use]
    pub fn default_placements(&self) -> &BTreeMap<String, Placement> {
        &self.default.0
    }

    /// The layout of `file` begins: the default layout until the file's text comes.
    pub fn start(&mut self, file: String) {
        self.file = Some(file);
        self.placements = self.default.0.clone();
        self.saved = self.placements.clone();
        self.selected = None;
        self.grab = None;
    }

    /// The file's text came (none: there is no such file, and the default layout is the one):
    /// its layout is the one shown and kept.
    pub fn read(&mut self, text: Option<&str>) {
        self.placements = text.map_or_else(|| self.default.0.clone(), parse);
        self.saved = self.placements.clone();
    }

    /// Whether the placements differ from those last read or saved: an element put back where it
    /// started is no change.
    #[must_use]
    pub fn unsaved(&self) -> bool {
        let moved = |m: &BTreeMap<String, Placement>| {
            m.iter()
                .filter(|(_, p)| !p.is_default())
                .map(|(n, p)| (n.clone(), *p))
                .collect::<Vec<_>>()
        };
        moved(&self.placements) != moved(&self.saved)
    }

    /// Keep the placements, each element moved by the edges it stands nearest as last drawn: their
    /// text waits to be written to the file ([`Self::take_write`]).
    pub fn save(&mut self) {
        let k = self.k;
        for (name, pl) in &mut self.placements {
            if pl.is_default() || k <= 0.0 {
                continue;
            }
            let Some((_, r)) = self.outlines.iter().find(|(n, _)| n == name) else {
                continue;
            };
            let (across, dx) = nearest(r.x, r.w, self.screen.0, k);
            let (down, dy) = nearest(r.y, r.h, self.screen.1, k);
            *pl = Placement {
                dx,
                dy,
                scale: pl.scale,
                edges: Some(Edges { across, down }),
            };
        }
        self.saved = self.placements.clone();
        if self.file.is_some() {
            self.to_write = Some(render(&self.placements));
        }
    }

    /// The file and the text to write to it, once saved.
    pub fn take_write(&mut self) -> Option<(String, String)> {
        let text = self.to_write.take()?;
        Some((self.file.clone()?, text))
    }

    /// Put every element back where the default layout has it (kept only once saved).
    pub fn reset(&mut self) {
        self.placements = self.default.0.clone();
    }

    /// Open the editor.
    pub fn open_editor(&mut self) {
        self.editing = true;
        self.window.open = true;
    }

    /// Close the editor: what was not saved is put back.
    pub fn close_editor(&mut self) {
        if self.unsaved() {
            self.placements = self.saved.clone();
        }
        self.editing = false;
        self.window.open = false;
        self.drawn = None;
        self.selected = None;
        self.grab = None;
    }

    /// The editor's Save or Reset, if one was pressed since this was last asked.
    pub fn take_pressed(&mut self) -> Option<Pressed> {
        self.pressed.take()
    }

    /// Whether the editor is open.
    #[must_use]
    pub const fn editing(&self) -> bool {
        self.editing
    }

    /// Where the editor's window was last drawn, while it is open.
    #[must_use]
    pub fn rect(&self) -> Option<Rect> {
        self.drawn.map(|(r, _)| r)
    }

    /// Where the editor's window's body was last drawn, while it is open.
    #[must_use]
    pub fn body(&self) -> Option<Rect> {
        self.drawn.map(|(_, b)| b)
    }

    /// Where the editor's controls are in a window drawn with `body` as its body, at `k` pixels
    /// a layout unit.
    #[must_use]
    pub fn controls(body: Rect, k: f32) -> EditorControls {
        let button = |x: f32| Rect::new(body.x + x * k, body.y + 54.0 * k, 104.0 * k, 30.0 * k);
        EditorControls {
            save: button(12.0),
            reset: button(128.0),
            close: button(244.0),
        }
    }

    /// The placement of element `name`.
    #[must_use]
    pub fn placement(&self, name: &str) -> Placement {
        self.placements.get(name).copied().unwrap_or_default()
    }

    /// A frame begins: the outlines are drawn anew.
    pub fn begin_frame(&mut self) {
        self.outlines.clear();
        self.labels.clear();
    }

    /// Start drawing element `name`: lend it the pointer in its own frame. While the editor is
    /// open, or the pointer is over a window drawn over the HUD, the element sees no pointer.
    pub fn begin(&self, name: &'static str, p: &Painter<'_>, ctx: &mut Ctx<'_>) -> Open {
        let open = Open {
            name,
            first_quad: p.list.quads.len(),
            mouse: ctx.input.mouse,
        };
        let (mx, my) = ctx.input.mouse;
        if self.editing || ctx.input.occluders.iter().any(|r| r.contains(mx, my)) {
            ctx.input.mouse = (-10_000.0, -10_000.0);
        } else {
            ctx.input.mouse = self.to_element(name, (mx, my), p.scale);
        }
        open
    }

    /// Finish drawing an element: move and scale what it drew, note its outline, and give the
    /// pointer back. An element that drew nothing has no outline.
    pub fn end(&mut self, open: Open, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) {
        self.finish(open, p, ctx, None);
    }

    /// Finish drawing an element that fills `home` when it shows, at its default place: as
    /// [`Self::end`], but it is moved and scaled about `home`'s top-left corner, and its outline
    /// takes in `home`; in the editor, whether it drew anything or not, so it can be placed while
    /// it shows nothing.
    pub fn end_in(&mut self, open: Open, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, home: Rect) {
        self.finish(open, p, ctx, Some(home));
    }

    fn finish(&mut self, open: Open, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, home: Option<Rect>) {
        ctx.input.mouse = open.mouse;
        let quads = &mut p.list.quads[open.first_quad..];
        let drew = !quads.is_empty();
        let Some(mut bounds) = home.or_else(|| quads.first().map(|q| q.dst)) else {
            return;
        };
        for q in quads.iter() {
            bounds = span(bounds, q.dst);
        }
        let k = p.scale;
        let pivot = home.map_or((bounds.x, bounds.y), |h| (h.x, h.y));
        let pl = self.placement(open.name);
        // Where the edges put it, as an offset from its default place.
        let (dx, dy) = pl.edges.map_or((pl.dx, pl.dy), |e| {
            let x = pivot.0 + (bounds.x - pivot.0) * pl.scale;
            let y = pivot.1 + (bounds.y - pivot.1) * pl.scale;
            let (w, h) = (bounds.w * pl.scale, bounds.h * pl.scale);
            (
                (start_from(e.across, pl.dx, w, p.screen.0, k) - x) / k,
                (start_from(e.down, pl.dy, h, p.screen.1, k) - y) / k,
            )
        });
        let map = |r: Rect| {
            Rect::new(
                pivot.0 + (r.x - pivot.0) * pl.scale + dx * k,
                pivot.1 + (r.y - pivot.1) * pl.scale + dy * k,
                r.w * pl.scale,
                r.h * pl.scale,
            )
        };
        let placed = map(bounds);
        let nudge = (
            on_screen(placed.x, placed.w, p.screen.0),
            on_screen(placed.y, placed.h, p.screen.1),
        );
        let moved = |r: Rect| map(r).offset(nudge.0, nudge.1);
        if !pl.is_default() || nudge != (0.0, 0.0) {
            for q in quads.iter_mut() {
                q.dst = moved(q.dst);
                q.clip = q.clip.map(moved);
            }
        }
        self.pivots.insert(open.name, pivot);
        self.nudges.insert(open.name, nudge);
        self.offsets.insert(open.name, (dx, dy));
        self.screen = p.screen;
        self.k = k;
        // Outside the editor, an element that drew nothing has no outline.
        if drew || self.editing {
            self.outlines.push((open.name, moved(bounds)));
        }
    }

    /// Where a rectangle element `name` drew is shown, after its placement.
    #[must_use]
    pub fn map_rect(&self, name: &str, r: Rect, k: f32) -> Rect {
        let Some(&pivot) = self.pivots.get(name) else {
            return r;
        };
        let pl = self.placement(name);
        let (dx, dy) = self.offset(name);
        let (nx, ny) = self.nudges.get(name).copied().unwrap_or((0.0, 0.0));
        Rect::new(
            pivot.0 + (r.x - pivot.0) * pl.scale + dx * k + nx,
            pivot.1 + (r.y - pivot.1) * pl.scale + dy * k + ny,
            r.w * pl.scale,
            r.h * pl.scale,
        )
    }

    /// The point of element `name`'s own frame that is shown at `(x, y)` on the screen.
    #[must_use]
    pub fn to_element(&self, name: &str, (x, y): (f32, f32), k: f32) -> (f32, f32) {
        let Some(&pivot) = self.pivots.get(name) else {
            return (x, y);
        };
        let pl = self.placement(name);
        let (dx, dy) = self.offset(name);
        let (nx, ny) = self.nudges.get(name).copied().unwrap_or((0.0, 0.0));
        (
            pivot.0 + (x - nx - pivot.0 - dx * k) / pl.scale,
            pivot.1 + (y - ny - pivot.1 - dy * k) / pl.scale,
        )
    }

    /// How far element `name` was moved from its default place as last drawn, in layout units.
    fn offset(&self, name: &str) -> (f32, f32) {
        self.offsets.get(name).copied().unwrap_or_else(|| {
            let pl = self.placement(name);
            (pl.dx, pl.dy)
        })
    }

    /// The editor, over the HUD: every element outlined and named, the selected one gold; a drag
    /// moves one, the wheel over the one selected resizes it; and its window, with Save, Reset
    /// and Close. Where it is, while it is open.
    pub fn edit(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) -> Option<Rect> {
        if !self.editing {
            return None;
        }
        let k = p.scale;
        let size = kit::window_size_for(p, BODY);
        let at = (
            ((p.screen.0 - size.0 * k) / 2.0).round(),
            (160.0 * k).round(),
        );
        // The window first, so a press on it is never an element's.
        let w = kit::window(p, ctx, &mut self.window, "HUD Layout", size, at);
        if w.closed {
            self.close_editor();
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        p.text(
            &body,
            w.body.x + 12.0 * k,
            w.body.y + 12.0 * k,
            "Drag to move; the wheel changes the size.",
        );
        let c = Self::controls(w.body, k);
        if kit::button(
            p,
            ctx,
            c.save,
            "Save",
            self.unsaved() || self.others_unsaved,
        ) {
            self.save();
            self.pressed = Some(Pressed::Save);
        }
        if kit::button(p, ctx, c.reset, "Reset", true) {
            self.reset();
            self.pressed = Some(Pressed::Reset);
        }
        if kit::button(p, ctx, c.close, "Close", true) {
            self.close_editor();
            return Some(w.rect);
        }
        let over_window = w.rect.contains(ctx.input.mouse.0, ctx.input.mouse.1);
        self.outline(p, ctx, over_window);
        Some(w.rect)
    }

    /// The outlines, and the selected element moved by a drag or resized by the wheel.
    fn outline(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, over_window: bool) {
        let k = p.scale;
        let label = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let outlines = std::mem::take(&mut self.outlines);
        if let (Some(name), Some((gx, gy))) = (self.selected, self.grab) {
            if ctx.input.down[0] {
                if let Some((_, r)) = outlines.iter().find(|(n, _)| *n == name) {
                    // Moved from where it stands: from its default place, until Save keeps it
                    // by its edges again.
                    let (dx, dy) = self.offset(name);
                    let pl = self.placements.entry(name.to_owned()).or_default();
                    *pl = Placement {
                        dx: dx + (ctx.input.mouse.0 - gx - r.x) / k,
                        dy: dy + (ctx.input.mouse.1 - gy - r.y) / k,
                        scale: pl.scale,
                        edges: None,
                    };
                }
                ctx.input.captured = true;
            } else {
                self.grab = None;
            }
        }
        for (name, r) in &outlines {
            let selected = self.selected == Some(*name);
            let over = !over_window && r.contains(ctx.input.mouse.0, ctx.input.mouse.1);
            let fill = if selected {
                0x40FF_C040
            } else if over {
                0x30FF_FFFF
            } else {
                0x20FF_FFFF
            };
            p.fill(*r, fill);
            let edge = if selected { 0xFFFF_C040 } else { 0xC0FF_FFFF };
            for side in [
                Rect::new(r.x, r.y, r.w, 1.5),
                Rect::new(r.x, r.bottom() - 1.5, r.w, 1.5),
                Rect::new(r.x, r.y, 1.5, r.h),
                Rect::new(r.right() - 1.5, r.y, 1.5, r.h),
            ] {
                p.fill(side, edge);
            }
            let title = element_title(name);
            let size = (
                p.measure(&label, title) + 6.0 * k,
                p.line_height(&label) + 2.0 * k,
            );
            let others: Vec<Rect> = outlines
                .iter()
                .filter(|(n, _)| n != name)
                .map(|(_, o)| *o)
                .chain(self.labels.iter().map(|(_, l)| *l))
                .collect();
            let at = label_place(*r, size, p.screen, 2.0 * k, &others);
            p.fill(at, 0xA000_0000);
            p.text(&label, at.x + 3.0 * k, at.y + 1.0 * k, title);
            self.labels.push((name, at));
            // An element the editor moves by dragging it shows the pointer of one moved so.
            if over || selected && self.grab.is_some() {
                ctx.input.pointer_over = Some(dereth_ui_kit::input::PointerOver::Move);
            }
            if over && ctx.input.pressed[0] && !ctx.input.captured {
                self.selected = Some(name);
                self.grab = Some((ctx.input.mouse.0 - r.x, ctx.input.mouse.1 - r.y));
                ctx.input.pressed[0] = false;
                ctx.input.captured = true;
            }
            if selected && over && ctx.input.wheel != 0.0 {
                let pl = self.placements.entry((*name).to_owned()).or_default();
                let step = if ctx.input.wheel > 0.0 {
                    SCALE_STEP
                } else {
                    -SCALE_STEP
                };
                pl.scale = (pl.scale + step).clamp(SCALE_RANGE.0, SCALE_RANGE.1);
                ctx.input.wheel = 0.0;
            }
        }
        self.outlines = outlines;
    }
}

/// The smallest rectangle holding both `a` and `b`.
fn span(a: Rect, b: Rect) -> Rect {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    Rect::new(
        x0,
        y0,
        a.right().max(b.right()) - x0,
        a.bottom().max(b.bottom()) - y0,
    )
}

/// Where a label `size` big names an element outlined at `outline` on a screen of `screen` pixels:
/// `gap` outside the outline, above it, else below, left or right of it, at either end of that
/// side and slid along it to lie wholly on the screen. The first of those that lies on the screen clear of the outline and
/// of every rectangle of `others` (the other elements and the names placed before); else the
/// first on the screen clear of the outline; else inside the outline's top-left corner.
#[must_use]
pub fn label_place(
    outline: Rect,
    size: (f32, f32),
    screen: (f32, f32),
    gap: f32,
    others: &[Rect],
) -> Rect {
    let (w, h) = size;
    let along_x = |x: f32| x.clamp(0.0, (screen.0 - w).max(0.0));
    let along_y = |y: f32| y.clamp(0.0, (screen.1 - h).max(0.0));
    let (left, right) = (along_x(outline.x), along_x(outline.right() - w));
    let (top, bottom) = (along_y(outline.y), along_y(outline.bottom() - h));
    let (above, below) = (outline.y - gap - h, outline.bottom() + gap);
    let (before, after) = (outline.x - gap - w, outline.right() + gap);
    let candidates = [
        Rect::new(left, above, w, h),
        Rect::new(right, above, w, h),
        Rect::new(left, below, w, h),
        Rect::new(right, below, w, h),
        Rect::new(before, top, w, h),
        Rect::new(before, bottom, w, h),
        Rect::new(after, top, w, h),
        Rect::new(after, bottom, w, h),
    ];
    let on_screen =
        |r: &Rect| r.x >= 0.0 && r.y >= 0.0 && r.right() <= screen.0 && r.bottom() <= screen.1;
    let meets = |a: &Rect, b: &Rect| {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    };
    let fair: Vec<Rect> = candidates
        .into_iter()
        .filter(|r| on_screen(r) && !meets(r, &outline))
        .collect();
    fair.iter()
        .find(|r| !others.iter().any(|o| meets(r, o)))
        .or_else(|| fair.first())
        .copied()
        .unwrap_or_else(|| {
            Rect::new(
                along_x(outline.x),
                along_y(outline.y),
                w.min(screen.0),
                h.min(screen.1),
            )
        })
}

/// An element's name as the editor labels it.
#[must_use]
pub fn element_title(name: &str) -> &str {
    use element as e;
    match name {
        e::PARAMETER => "Parameter Bar",
        e::FOCUS => "Focus Gauge",
        e::TARGET => "Target Info",
        e::STATUS => "Status Info",
        e::LAMP => "Lamps",
        e::PARTY => "Party List",
        e::MENU => "Main Menu",
        e::CHAT => "Log Window",
        e::HOTBAR => "Hotbars",
        e::CROSS => "Cross Hotbars",
        e::LINK => "Server Info",
        e::MINIMAP => "Minimap",
        e::PETS => "Pets",
        e::DISMOUNT => "Dismount Button",
        e::EXPERIENCE => "Experience Bar",
        other => other,
    }
}

/// A layout file's text: a line per element moved or resized.
#[must_use]
pub fn render(placements: &BTreeMap<String, Placement>) -> String {
    let mut out = String::new();
    for (name, p) in placements.iter().filter(|(_, p)| !p.is_default()) {
        out.push_str(&format!("{name} {:.1} {:.1} {:.2}", p.dx, p.dy, p.scale));
        if let Some(e) = p.edges {
            out.push(' ');
            out.push_str(&e.letters());
        }
        out.push('\n');
    }
    out
}

/// A layout file's text read: a line with its edges, or one without, an offset from the default
/// place. A line that does not read is skipped, and a scale out of range is brought into it.
#[must_use]
pub fn parse(text: &str) -> BTreeMap<String, Placement> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let (name, dx, dy, scale, edges) = match parts[..] {
            [name, dx, dy, scale] => (name, dx, dy, scale, None),
            [name, dx, dy, scale, letters] => match Edges::read(letters) {
                Some(e) => (name, dx, dy, scale, Some(e)),
                None => continue,
            },
            _ => continue,
        };
        if let (Ok(dx), Ok(dy), Ok(scale)) =
            (dx.parse::<f32>(), dy.parse::<f32>(), scale.parse::<f32>())
        {
            if dx.is_finite() && dy.is_finite() && scale.is_finite() {
                out.insert(
                    name.to_owned(),
                    Placement {
                        dx,
                        dy,
                        scale: scale.clamp(SCALE_RANGE.0, SCALE_RANGE.1),
                        edges,
                    },
                );
            }
        }
    }
    out
}
