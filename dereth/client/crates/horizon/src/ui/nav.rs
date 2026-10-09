//! A focus moved over the interface by a pad's d-pad, the way a controller moves the cursor of a
//! console game's menus.
//!
//! Every widget that answers the pointer notes where it is as it is drawn ([`NavFrame::note`]);
//! each window, box and the chat log notes itself as a panel ([`NavFrame::layer`]), with the
//! control that cancels it (its close button, a box's No or OK) and, for a window of packs, the
//! packs the shoulder buttons step through. Between frames that becomes a [`Snapshot`]: what can
//! be reached, with what lies behind a modal box or under a window left out, a list's rows
//! scrolled out of sight kept apart (they are what a list scrolls to), and any target that only
//! holds others (a window's whole body, a list's area) dropped in favour of what it holds.
//!
//! A [`Cursor`] keeps the focus on one target from frame to frame, inside its panel: the d-pad
//! moves it to the nearest target that way in the same panel. A panel that opens takes the focus;
//! when it closes, the focus goes back to the control that had it when the panel opened.
//!
//! A scrolling list ([`NavFrame::list`]) has its rows as stops: the d-pad moves one row at a
//! time, the list scrolling by exactly a row to keep the row the focus moves to in sight, and
//! past its last row (or before its first) the focus goes on to what lies beyond the list. Two
//! kinds of list are different:
//! - a list that is what its window is for ([`NavFrame::list_inside`], a pack's contents): the
//!   window opens inside it, the d-pad keeps to it, and a step back goes to what it belongs to;
//! - a list beside other content that is one stop until confirmed ([`NavFrame::list_whole`], the
//!   chat log).
//!
//! Stepping back ([`Cursor::back`]) returns from a control the focus was sent to (a choice moving
//! it on to what acts on the choice) to where it came from, and from a list as its kind says.

use crate::draw::Rect;

/// A direction of the d-pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// What a target is, where that changes what the pad does on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// Pressed and released, as a click.
    Plain,
    /// A slider: left and right move its knob, from `x0` to `x1` along it, now at `t` (0 to 1).
    Slider { x0: f32, x1: f32, t: f32 },
    /// A line of text to type into.
    Text,
    /// A scrolling list as a whole, not entered: confirming enters it.
    List,
}

/// A scrolling list: its area (the scrollbar's included) in a panel, and whether it can scroll
/// up and down from where it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct List {
    pub area: Rect,
    pub layer: usize,
    pub up: bool,
    pub down: bool,
    /// The list is what its window is for: the window opens with it entered, at its first row,
    /// and the d-pad keeps to it.
    pub inside: bool,
    /// Where a step back out of a list the window is for goes: the control it belongs to (a
    /// pack's icon for its contents).
    pub owner: Option<Rect>,
    /// The list is one stop until confirmed (a log beside other content).
    pub whole: bool,
    /// A step back from a row goes to where the focus last was in the window outside its lists
    /// (a character's paper doll beside its pages of rows); otherwise the window is cancelled.
    pub back_outside: bool,
}

/// Something the focus can rest on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub rect: Rect,
    pub kind: Kind,
    /// The panel it was drawn in: its place among the layers, counted from 1; `0` for the screen
    /// under every panel.
    pub layer: usize,
    /// The clip it was drawn in: the list it is a row of.
    pub clip: Option<Rect>,
}

impl Target {
    /// Its middle.
    #[must_use]
    pub fn centre(&self) -> (f32, f32) {
        centre(&self.rect)
    }
}

fn centre(r: &Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// What a panel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    /// A window: the focus goes into it as it opens, and leaves it when it closes.
    Window,
    /// A box asking or telling something.
    Box,
    /// The chat log: always there, reached by cycling the panels.
    Chat,
}

/// A panel: a window, a box, the chat log.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub rect: Rect,
    /// What tells it from another frame to frame: its title.
    pub name: String,
    pub kind: PanelKind,
    /// The control that cancels it: a window's close button, a box's last button.
    pub cancel: Option<Rect>,
    /// The packs (or pages) it steps through with the shoulder buttons, and the one shown.
    pub steps: Option<(Vec<Rect>, usize)>,
}

/// What the interface noted of itself this frame, for the focus to move over next frame.
#[derive(Debug, Clone, Default)]
pub struct NavFrame {
    /// The targets noted so far, in drawing order.
    pub targets: Vec<Target>,
    /// Targets drawn outside their clip: a list's rows scrolled out of sight.
    pub hidden: Vec<Target>,
    /// The panels drawn so far, the last on top.
    pub layers: Vec<Layer>,
    /// The panel what is noted now goes in: `0` for the screen.
    current: usize,
    /// The rows of tabs drawn so far, each with its layer.
    pub tab_bars: Vec<(Rect, usize)>,
    /// Where each screen or panel would have the focus start: its main button, its first item.
    pub homes: Vec<Rect>,
    /// A control that asks for the focus now (a list's choice moving it to what acts on it).
    pub want: Option<Rect>,
    /// The control that cancels the screen itself (leaving character select).
    pub screen_cancel: Option<Rect>,
    /// The scrolling lists drawn so far.
    pub lists: Vec<List>,
    /// The first target a modal box took: those before it are behind the box.
    modal_from: Option<usize>,
    /// Last frame's, as the focus moves over it.
    pub last: Snapshot,
}

/// What the focus can reach, as last frame drew it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub targets: Vec<Target>,
    pub hidden: Vec<Target>,
    pub layers: Vec<Layer>,
    pub tab_bars: Vec<(Rect, usize)>,
    pub homes: Vec<Rect>,
    pub want: Option<Rect>,
    pub screen_cancel: Option<Rect>,
    pub lists: Vec<List>,
}

impl Snapshot {
    /// What the focus can reach with the list at `entered` entered (or none):
    /// - a list that is one stop until confirmed is one target over its area in place of its
    ///   rows, until it is entered, when its panel has only its rows;
    /// - a list its window is for has its rows reached only once it is entered, when its panel
    ///   has only its rows;
    /// - any other list's rows are reached as they are.
    #[must_use]
    pub fn seen(&self, entered: Option<Rect>) -> Snapshot {
        let mut out = self.clone();
        for l in self.lists.iter().filter(|l| l.whole || l.inside) {
            let inside = |t: &Target| {
                let (x, y) = t.centre();
                t.layer == l.layer && t.rect != l.area && l.area.contains(x, y)
            };
            let whole = Target {
                rect: l.area,
                kind: Kind::List,
                layer: l.layer,
                clip: None,
            };
            if entered == Some(l.area) {
                out.targets.retain(|t| t.layer != l.layer || inside(t));
                if !out.targets.iter().any(inside) {
                    out.targets.push(whole);
                }
                continue;
            }
            out.targets.retain(|t| !inside(t));
            out.hidden.retain(|t| !inside(t));
            if l.whole {
                out.targets.push(whole);
            }
        }
        out
    }

    /// The list whose area is `area`.
    #[must_use]
    pub fn list(&self, area: Rect) -> Option<List> {
        self.lists.iter().find(|l| l.area == area).copied()
    }

    /// The list `t` is a row of, if any.
    #[must_use]
    pub fn list_of(&self, t: &Target) -> Option<List> {
        let (x, y) = t.centre();
        self.lists
            .iter()
            .find(|l| l.layer == t.layer && l.area != t.rect && l.area.contains(x, y))
            .copied()
    }

    /// The row of tabs in `layer`, the last drawn there.
    #[must_use]
    pub fn tabs_in(&self, layer: usize) -> Option<Rect> {
        self.tab_bars
            .iter()
            .rev()
            .find(|(_, l)| *l == layer)
            .map(|(r, _)| *r)
    }

    /// Where the focus starts in panel `layer` (or on the screen, for `0`), if it says.
    #[must_use]
    pub fn home_in(&self, layer: usize) -> Option<Target> {
        self.targets
            .iter()
            .find(|t| t.layer == layer && self.homes.contains(&t.rect))
            .copied()
    }

    /// Panel `layer` (counted from 1).
    #[must_use]
    pub fn layer(&self, layer: usize) -> Option<&Layer> {
        layer.checked_sub(1).and_then(|i| self.layers.get(i))
    }

    /// The control that cancels panel `layer`, or the screen's for `0`.
    #[must_use]
    pub fn cancel_of(&self, layer: usize) -> Option<Rect> {
        if layer == 0 {
            self.screen_cancel
        } else {
            self.layer(layer).and_then(|l| l.cancel)
        }
    }

    /// Whether panel `layer` has anything to rest on.
    #[must_use]
    pub fn has_targets(&self, layer: usize) -> bool {
        self.targets.iter().any(|t| t.layer == layer)
    }

    /// The panels the Back button cycles through, in order: those with something to rest on.
    #[must_use]
    pub fn panels(&self) -> Vec<usize> {
        (1..=self.layers.len())
            .filter(|l| self.has_targets(*l))
            .collect()
    }
}

impl NavFrame {
    /// Note `r`, drawn now and answering the pointer, unless it is out of sight: its middle
    /// covered by one of `occluders` (a window drawn over it); outside the clip it is drawn in,
    /// it is kept apart as a row scrolled out of sight.
    pub fn note(&mut self, r: Rect, kind: Kind, occluders: &[Rect]) {
        if r.w < 2.0 || r.h < 2.0 {
            return;
        }
        let (cx, cy) = centre(&r);
        if occluders.iter().any(|o| o.contains(cx, cy)) {
            return;
        }
        let clip = crate::draw::current_clip();
        let target = Target {
            rect: r,
            kind,
            layer: self.current,
            clip,
        };
        if clip.is_some_and(|c| !c.contains(cx, cy)) {
            self.hidden.push(target);
            return;
        }
        if let Some(t) = self
            .targets
            .iter_mut()
            .rev()
            .take(8)
            .find(|t| t.rect == r && t.layer == target.layer)
        {
            // Noted twice (hovered, then clicked): the more telling kind stays.
            if t.kind == Kind::Plain {
                t.kind = kind;
            }
            return;
        }
        self.targets.push(target);
    }

    /// A scrolling list over `area` (its scrollbar's included), which can scroll `up` and `down`
    /// from where it is: its rows are stops. Not where a window drawn over it covers its middle.
    pub fn list(&mut self, area: Rect, up: bool, down: bool, occluders: &[Rect]) {
        let (cx, cy) = centre(&area);
        if occluders.iter().any(|o| o.contains(cx, cy)) {
            return;
        }
        if let Some(l) = self
            .lists
            .iter_mut()
            .find(|l| l.area == area && l.layer == self.current)
        {
            l.up = up;
            l.down = down;
            return;
        }
        self.lists.push(List {
            area,
            layer: self.current,
            up,
            down,
            inside: false,
            owner: None,
            whole: false,
            back_outside: false,
        });
    }

    /// The list over `area` (noted as one, or noted now if it does not scroll) is what its window
    /// is for: the window opens inside it, the d-pad keeps to it, and a step back out goes to
    /// `owner`.
    pub fn list_inside(&mut self, area: Rect, owner: Option<Rect>, occluders: &[Rect]) {
        let l = self.list_here(area, occluders);
        if let Some(l) = l {
            l.inside = true;
            l.owner = owner;
        }
    }

    /// The list over `area` (noted as one, or noted now if it does not scroll) is one stop until
    /// confirmed: a log beside other content.
    pub fn list_whole(&mut self, area: Rect, up: bool, down: bool, occluders: &[Rect]) {
        self.list(area, up, down, occluders);
        if let Some(l) = self
            .lists
            .iter_mut()
            .find(|l| l.area == area && l.layer == self.current)
        {
            l.whole = true;
        }
    }

    /// The list over `area` (noted as one, or noted now if it does not scroll) is a page of rows
    /// beside other content of its window: a step back from a row goes to where the focus last was
    /// outside the window's lists.
    pub fn list_of_rows(&mut self, area: Rect, occluders: &[Rect]) {
        if let Some(l) = self.list_here(area, occluders) {
            l.back_outside = true;
        }
    }

    fn list_here(&mut self, area: Rect, occluders: &[Rect]) -> Option<&mut List> {
        let layer = self.current;
        if !self
            .lists
            .iter()
            .any(|l| l.area == area && l.layer == layer)
        {
            self.list(area, false, false, occluders);
        }
        self.lists
            .iter_mut()
            .find(|l| l.area == area && l.layer == layer)
    }

    /// What was noted at `r` in the panel being drawn is not a stop after all: a picture that
    /// does nothing on confirming (the mouse may still drag it).
    pub fn forget(&mut self, r: Rect) {
        let layer = self.current;
        self.targets.retain(|t| t.rect != r || t.layer != layer);
    }

    /// A row of a list at `r`, scrolled out of sight and not drawn: kept apart, as what the list
    /// scrolls to.
    pub fn note_hidden(&mut self, r: Rect) {
        self.hidden.push(Target {
            rect: r,
            kind: Kind::Plain,
            layer: self.current,
            clip: crate::draw::current_clip(),
        });
    }

    /// A row of tabs drawn at `r`, unless a window drawn over it covers its middle.
    pub fn tabs(&mut self, r: Rect, occluders: &[Rect]) {
        let (cx, cy) = centre(&r);
        if !occluders.iter().any(|o| o.contains(cx, cy)) {
            self.tab_bars.push((r, self.current));
        }
    }

    /// A panel begins at `r`, named `name`: what is noted from now on is in it.
    pub fn layer(&mut self, r: Rect, name: &str, kind: PanelKind) {
        self.layers.push(Layer {
            rect: r,
            name: name.to_owned(),
            kind,
            cancel: None,
            steps: None,
        });
        self.current = self.layers.len();
    }

    /// The panel being drawn is known as `name`, frame to frame, whatever its title.
    pub fn rename(&mut self, name: &str) {
        if let Some(l) = self
            .current
            .checked_sub(1)
            .and_then(|i| self.layers.get_mut(i))
        {
            name.clone_into(&mut l.name);
        }
    }

    /// The panel being drawn is a box asking or telling something, not a window.
    pub fn mark_box(&mut self) {
        if let Some(l) = self
            .current
            .checked_sub(1)
            .and_then(|i| self.layers.get_mut(i))
        {
            l.kind = PanelKind::Box;
        }
    }

    /// The panel begun last is over: what is noted from now on is on the screen again.
    pub fn end_layer(&mut self) {
        self.current = 0;
    }

    /// The control at `r` cancels the panel being drawn (or the screen, outside any).
    pub fn cancel(&mut self, r: Rect) {
        match self
            .current
            .checked_sub(1)
            .and_then(|i| self.layers.get_mut(i))
        {
            Some(l) => l.cancel = Some(r),
            None => self.screen_cancel = Some(r),
        }
    }

    /// The panel being drawn steps through `rects` with the shoulder buttons, `at` shown.
    pub fn steps(&mut self, rects: Vec<Rect>, at: usize) {
        if let Some(l) = self
            .current
            .checked_sub(1)
            .and_then(|i| self.layers.get_mut(i))
        {
            l.steps = Some((rects, at));
        }
    }

    /// The screen's main button is at `r`: where the focus starts on it.
    pub fn home(&mut self, r: Rect) {
        self.homes.push(r);
    }

    /// The control at `r` asks for the focus now.
    pub fn want_focus(&mut self, r: Rect) {
        self.want = Some(r);
    }

    /// A modal box begins: nothing noted before it can be reached.
    pub fn modal(&mut self) {
        self.modal_from.get_or_insert(self.targets.len());
    }

    /// The frame is over: what it noted becomes what the focus moves over next.
    pub fn finish(&mut self) {
        let mut targets = std::mem::take(&mut self.targets);
        let modal = self.modal_from.take();
        if let Some(from) = modal {
            targets.drain(..from.min(targets.len()));
        }
        self.current = 0;
        let homes = std::mem::take(&mut self.homes);
        let mut targets = without_holders(targets, &homes);
        // A window with nothing in it to rest on (a page of text) is a place to rest itself, so
        // the focus can be in it and its cancel close it; not while a modal box keeps the focus.
        for (i, l) in self.layers.iter().enumerate().filter(|_| modal.is_none()) {
            if l.kind == PanelKind::Window && !targets.iter().any(|t| t.layer == i + 1) {
                targets.push(Target {
                    rect: l.rect,
                    kind: Kind::Plain,
                    layer: i + 1,
                    clip: None,
                });
            }
        }
        // A scrolling list with nothing in it to rest on (a page of text) is one stop, which the
        // d-pad's up and down scroll.
        for l in self.lists.iter().filter(|l| !l.whole && !l.inside) {
            let holds_any = targets.iter().any(|t| {
                let (x, y) = t.centre();
                t.layer == l.layer && t.rect != l.area && l.area.contains(x, y)
            });
            if !holds_any {
                targets.push(Target {
                    rect: l.area,
                    kind: Kind::List,
                    layer: l.layer,
                    clip: None,
                });
            }
        }
        self.last = Snapshot {
            targets,
            hidden: std::mem::take(&mut self.hidden),
            layers: std::mem::take(&mut self.layers),
            tab_bars: std::mem::take(&mut self.tab_bars),
            homes,
            want: self.want.take(),
            screen_cancel: self.screen_cancel.take(),
            lists: std::mem::take(&mut self.lists),
        };
    }
}

/// The targets, less any that holds the middle of another of the same layer: a window's body or a
/// list's area, which the pointer is over whenever it is over what they hold.
fn without_holders(targets: Vec<Target>, homes: &[Rect]) -> Vec<Target> {
    let holds = |a: &Target, b: &Target| {
        let (x, y) = b.centre();
        a.layer == b.layer
            && a.rect != b.rect
            && a.rect.w * a.rect.h > b.rect.w * b.rect.h
            && a.rect.contains(x, y)
    };
    targets
        .iter()
        .filter(|a| homes.contains(&a.rect) || !targets.iter().any(|b| holds(a, b)))
        .copied()
        .collect()
}

/// How far along `dir` `t` lies from `f`, how far off to the side, and whether the two share the
/// row (or column).
fn bearing(f: &Rect, t: &Rect, dir: Dir) -> (f32, f32, bool) {
    let (fx, fy) = centre(f);
    let (tx, ty) = centre(t);
    match dir {
        Dir::Right => (tx - fx, (ty - fy).abs(), spans(f.y, f.h, t.y, t.h)),
        Dir::Left => (fx - tx, (ty - fy).abs(), spans(f.y, f.h, t.y, t.h)),
        Dir::Down => (ty - fy, (tx - fx).abs(), spans(f.x, f.w, t.x, t.w)),
        Dir::Up => (fy - ty, (tx - fx).abs(), spans(f.x, f.w, t.x, t.w)),
    }
}

/// How a target that lies `dir` of `from` scores: lower nearer; `None` when it is not that way.
/// What shares the row (or column) scores by how far along it is; off the row, what lies off to
/// the side counts three times as far and the way along twice, with a little more besides.
fn score(from: &Rect, t: &Rect, dir: Dir) -> Option<f32> {
    let (along, side, overlap) = bearing(from, t, dir);
    if along <= 2.0 || t == from {
        return None;
    }
    // What shares the row (taller or shorter, a little lower or higher) is not above or below
    // it, nor what shares the column beside it.
    let level = match dir {
        Dir::Up | Dir::Down => spans(from.y, from.h, t.y, t.h),
        Dir::Left | Dir::Right => spans(from.x, from.w, t.x, t.w),
    };
    if level {
        return None;
    }
    // Far off to the side (more than twice as far as along) is not that way at all.
    if !overlap && side > along * 2.0 + 4.0 {
        return None;
    }
    Some(if overlap {
        along
    } else {
        2.0 * along + 3.0 * side + 100.0
    })
}

/// Whether two spans along one axis overlap by half the smaller.
fn spans(a: f32, aw: f32, b: f32, bw: f32) -> bool {
    let overlap = (a + aw).min(b + bw) - a.max(b);
    overlap > aw.min(bw) * 0.5
}

/// Where the d-pad's step from `from` lands among `targets`: the best scored target that way in
/// `from`'s panel, or `None` when nothing in it lies that way.
#[must_use]
pub fn step(targets: &[Target], from: &Target, dir: Dir) -> Option<Target> {
    let same = || targets.iter().filter(|t| t.layer == from.layer);
    same()
        .filter_map(|t| score(&from.rect, &t.rect, dir).map(|s| (s, *t)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, t)| t)
        // Nothing within the way's reach: the nearest that lies that way at all (a tab bar's
        // tabs at a window's left, its controls at the right).
        .or_else(|| {
            same()
                .filter_map(|t| {
                    let (along, side, _) = bearing(&from.rect, &t.rect, dir);
                    (along > 2.0 && t.rect != from.rect).then_some((along + side, *t))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, t)| t)
        })
}

/// What a step of the d-pad did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stepped {
    /// The focus moved.
    Moved,
    /// The focus is at its list's end with rows beyond it out of sight: scroll the list that way.
    Scroll(Dir),
    /// Scroll the list over `area` by `by` pixels (down positive): a row's height to bring the
    /// next row into the place of the one the focus stays on, or what brings the row the focus
    /// moved to wholly into sight, or (very far) to an end of the list.
    ScrollBy { area: Rect, by: f32 },
    /// Nothing lies that way.
    Stayed,
}

/// Whether the list `from` is a row of has rows out of sight `dir` of it.
fn rows_beyond(snap: &Snapshot, from: &Target, dir: Dir) -> bool {
    let Some(clip) = from.clip else {
        return false;
    };
    snap.hidden
        .iter()
        .filter(|t| t.layer == from.layer && t.clip == Some(clip))
        .any(|t| bearing(&from.rect, &t.rect, dir).0 > 2.0)
}

/// How far a page of text scrolls for a press of the d-pad, in pixels.
const TEXT_STEP: f32 = 40.0;

/// How far a list is scrolled to reach one of its ends at once, in pixels.
const TO_THE_END: f32 = 1.0e6;

/// The height of one row of `list`: the least step down between its rows in sight, else
/// `fallback`'s height.
fn row_pitch(snap: &Snapshot, list: &List, fallback: &Target) -> f32 {
    let mut ys: Vec<f32> = snap
        .targets
        .iter()
        .chain(snap.hidden.iter())
        .filter(|t| {
            t.layer == list.layer && t.rect != list.area && {
                let (x, y) = t.centre();
                list.area.contains(x, y) || t.clip == Some(list.area)
            }
        })
        .map(|t| t.rect.y)
        .collect();
    ys.sort_by(f32::total_cmp);
    ys.windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| *d > 2.0)
        .reduce(f32::min)
        .unwrap_or(fallback.rect.h)
}

/// The focus, from frame to frame.
#[derive(Debug, Clone, Default)]
pub struct Cursor {
    /// The target the focus is on.
    pub focus: Option<Target>,
    /// The panels last frame drew, by name, to tell when one opens.
    names: Vec<String>,
    /// The screen's main button as last seen, to tell when a screen comes up with one.
    home: Option<Rect>,
    /// For each panel the focus went into as it opened: its name, and the control that had the
    /// focus before, which gets it back when the panel closes.
    openers: Vec<(String, Option<Rect>)>,
    /// The list entered (one its window is for, or one that is a stop until confirmed), by its
    /// area.
    pub entered: Option<Rect>,
    /// Where the focus last was in each list left, to go back to on entering it again.
    rows: Vec<(Rect, Rect)>,
    /// Where the focus came from each time a control sent it on, with the list entered then: a
    /// step back returns there.
    sources: Vec<(Target, Option<Rect>)>,
    /// The panel the list entered is in.
    entered_layer: usize,
    /// For each panel, by name, where the focus last was outside its lists.
    outside: Vec<(String, Rect)>,
    /// A list just scrolled under the focus: its area, and where the row the focus moves to will
    /// stand once the list has moved. Then the focus goes to the row there.
    edge: Option<(Rect, Rect)>,
}

impl Cursor {
    /// Keep the focus on its target in `snap`, as the interface changes under it:
    /// - a control asking for the focus takes it;
    /// - a window or box that has opened since the last frame takes it (inside the list it is for,
    ///   else its main button, else its first target below the title bar);
    /// - a screen's main button that has come up takes it, unless a panel holds it;
    /// - kept on the same rectangle while that is there;
    /// - its panel closed, back to the control that had it when the panel opened;
    /// - otherwise the nearest in its panel, or nothing.
    ///
    /// The chat log never takes the focus by opening: the Back button brings the focus to it.
    pub fn follow(&mut self, raw: &Snapshot) {
        self.follow_inner(raw);
        self.note_outside(raw);
        if let Some(l) = self.entered.and_then(|a| raw.list(a)) {
            self.entered_layer = l.layer;
        }
    }

    fn follow_inner(&mut self, raw: &Snapshot) {
        // A list scrolled under the focus: the focus to the row now where it was to stand.
        if let Some((area, there)) = self.edge.take() {
            let (cx, cy) = centre(&there);
            if let Some(t) = raw
                .seen(self.entered)
                .targets
                .iter()
                .filter(|t| raw.list_of(t).is_some_and(|l| l.area == area))
                .min_by(|a, b| {
                    let d = |t: &Target| {
                        let (x, y) = t.centre();
                        (x - cx).abs() + 4.0 * (y - cy).abs()
                    };
                    d(a).total_cmp(&d(b))
                })
            {
                self.focus = Some(*t);
            }
        }
        // A list gone with its window is no longer entered; a source whose panel is gone is
        // forgotten.
        // (A list hidden for a frame by something over it stays entered while its window is
        // open.)
        let window_open = self
            .names
            .get(self.entered_layer.wrapping_sub(1))
            .is_some_and(|n| raw.layers.iter().any(|l| &l.name == n));
        if self.entered.is_some_and(|a| raw.list(a).is_none()) && !window_open {
            self.entered = None;
        }
        self.sources
            .retain(|(t, _)| t.layer == 0 || raw.layer(t.layer).is_some());
        let before = self.entered;
        // A control asking for the focus has it: in a list its window is for, that list is
        // entered; elsewhere, the list entered is left.
        if let Some(wanted) = raw
            .want
            .and_then(|w| raw.targets.iter().find(|t| t.rect == w))
        {
            self.entered = raw
                .list_of(wanted)
                .filter(|l| l.inside || l.whole)
                .map(|l| l.area);
        }
        let seen = raw.seen(self.entered);
        let snap = &seen;
        let names: Vec<String> = snap.layers.iter().map(|l| l.name.clone()).collect();
        let opened = snap
            .layers
            .iter()
            .enumerate()
            .rev()
            .find(|(_, l)| l.kind != PanelKind::Chat && !self.names.contains(&l.name))
            .map(|(i, l)| (i + 1, l.name.clone()));
        self.names = names;
        if let Some(t) = snap
            .want
            .and_then(|r| snap.targets.iter().find(|t| t.rect == r))
        {
            if let Some(from) = self.focus.filter(|f| f.rect != t.rect) {
                self.sources.push((from, before));
            }
            self.focus = Some(*t);
            return;
        }
        let main = snap.home_in(0);
        let new_home = main.is_some_and(|m| Some(m.rect) != self.home);
        self.home = main.map(|m| m.rect);
        if let Some((layer, name)) = opened {
            let was = self.focus.map(|f| f.rect);
            if self.enter_inside(raw, layer) {
                self.openers.push((name, was));
                return;
            }
            let into = snap.home_in(layer).or_else(|| first_in(snap, layer));
            if let Some(t) = into {
                self.openers.push((name, was));
                self.focus = Some(t);
                return;
            }
        }
        if new_home && self.focus.is_none_or(|f| f.layer == 0) {
            self.focus = main;
            return;
        }
        let Some(was) = self.focus else {
            return;
        };
        if let Some(t) = snap.targets.iter().find(|t| t.rect == was.rect) {
            self.focus = Some(*t);
            return;
        }
        // Panels gone: back to what had the focus when the latest of them opened.
        let mut back = None;
        while let Some((name, opener)) = self.openers.last() {
            if self.names.contains(name) {
                break;
            }
            back = Some(*opener);
            self.openers.pop();
        }
        if let Some(opener) = back {
            // Back in the list it was in, when it was a row of one.
            let in_raw = opener.and_then(|r| raw.targets.iter().find(|t| t.rect == r).copied());
            if let Some(t) = in_raw {
                if let Some(l) = raw.list_of(&t).filter(|l| l.inside || l.whole) {
                    self.entered = Some(l.area);
                    self.focus = Some(t);
                    return;
                }
            }
            self.focus = opener
                .and_then(|r| snap.targets.iter().find(|t| t.rect == r))
                .copied()
                .or_else(|| main.filter(|_| snap.layers.is_empty()))
                .or_else(|| {
                    snap.panels()
                        .into_iter()
                        .rev()
                        .find(|l| snap.layer(*l).is_some_and(|l| l.kind != PanelKind::Chat))
                        .and_then(|l| first_in(snap, l))
                });
            return;
        }
        // The screen changed under it: the new screen's main button.
        if was.layer == 0 {
            if let Some(m) = main {
                self.focus = Some(m);
                return;
            }
        }
        let layer_there = was.layer == 0 || snap.layer(was.layer).is_some();
        self.focus = if layer_there {
            nearest(&snap.targets, was.centre(), Some(was.layer))
        } else {
            None
        };
    }

    /// Remember where the focus is in its panel when it is outside the panel's lists.
    fn note_outside(&mut self, raw: &Snapshot) {
        let Some(f) = self.focus else {
            return;
        };
        if raw.list_of(&f).is_some() || raw.list(f.rect).is_some() {
            return;
        }
        let Some(name) = raw.layer(f.layer).map(|l| l.name.clone()) else {
            return;
        };
        self.outside.retain(|(n, _)| *n != name);
        self.outside.push((name, f.rect));
    }

    /// Move the focus a step `dir` among `snap`'s targets, within its panel. In a list, up and down
    /// move exactly one row: at the last row in sight with more below (or the first, with more
    /// above), the list scrolls by a row and the focus stays where it is, now on the next row; a
    /// row only partly in sight is scrolled wholly into it. Past the list's true end the focus
    /// goes on to what lies beyond it.
    pub fn step(&mut self, raw: &Snapshot, dir: Dir) -> Stepped {
        let Some(from) = self.focus else {
            return Stepped::Stayed;
        };
        let seen = raw.seen(self.entered);
        let snap = &seen;
        let next = step(&snap.targets, &from, dir);
        let along = matches!(dir, Dir::Up | Dir::Down);
        // A page of text scrolls a line's worth at a time, and past its end the focus moves on.
        if let Some(l) = raw
            .list(from.rect)
            .filter(|l| from.kind == Kind::List && !l.whole && along)
        {
            let more = if dir == Dir::Up { l.up } else { l.down };
            if more {
                return Stepped::ScrollBy {
                    area: l.area,
                    by: if dir == Dir::Up {
                        -TEXT_STEP
                    } else {
                        TEXT_STEP
                    },
                };
            }
        }
        if let Some(l) = raw.list_of(&from).filter(|l| !l.whole && along) {
            // The list's rows, those in sight and those scrolled out of it, and the one the step
            // goes to: the best placed that way.
            let rows: Vec<Target> = snap
                .targets
                .iter()
                .chain(raw.hidden.iter())
                .filter(|t| {
                    t.layer == l.layer
                        && t.rect != l.area
                        && (t.clip == Some(l.area)
                            || raw.list_of(t).is_some_and(|m| m.area == l.area))
                })
                .copied()
                .collect();
            if let Some(n) = step(&rows, &from, dir) {
                let over = if dir == Dir::Down {
                    (n.rect.bottom() - l.area.bottom()).max(0.0)
                } else {
                    (n.rect.y - l.area.y).min(0.0)
                };
                if over.abs() <= 1.0 {
                    self.focus = Some(n);
                    self.note_outside(raw);
                    return Stepped::Moved;
                }
                // Not wholly in sight: the list moves just far enough to show it, and once it
                // has, the focus is on that row at the list's end.
                let mut there = n.rect;
                there.y -= over;
                self.edge = Some((l.area, there));
                return Stepped::ScrollBy {
                    area: l.area,
                    by: over,
                };
            }
            let more = if dir == Dir::Up { l.up } else { l.down };
            if more {
                let sign = if dir == Dir::Up { -1.0 } else { 1.0 };
                self.edge = Some((l.area, from.rect));
                return Stepped::ScrollBy {
                    area: l.area,
                    by: sign * row_pitch(raw, &l, &from),
                };
            }
        }
        // A list not known as one (its rows clipped): at its end with rows out of sight, it
        // scrolls instead.
        let leaves_list = from.clip.is_some() && next.is_none_or(|n| n.clip != from.clip);
        if leaves_list && raw.list_of(&from).is_none() && rows_beyond(snap, &from, dir) {
            return Stepped::Scroll(dir);
        }
        match next {
            Some(t) => {
                self.focus = Some(t);
                self.note_outside(raw);
                Stepped::Moved
            }
            None => Stepped::Stayed,
        }
    }

    /// Round to the other end of the focus's panel, at its end `dir`: the first target going
    /// down past the last, the last going up past the first; where that is in a list, the list is
    /// scrolled to that end too.
    pub fn wrap(&mut self, raw: &Snapshot, dir: Dir) -> Stepped {
        let Some(from) = self.focus else {
            return Stepped::Stayed;
        };
        let seen = raw.seen(self.entered);
        let snap = &seen;
        let same = snap.targets.iter().filter(|t| t.layer == from.layer);
        let key = |t: &&Target| ((t.rect.y / 8.0).floor(), t.rect.x);
        let to = match dir {
            Dir::Down => same.min_by(|a, b| {
                key(a)
                    .partial_cmp(&key(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            Dir::Up => same.max_by(|a, b| {
                ((a.rect.y / 8.0).floor(), -a.rect.x)
                    .partial_cmp(&((b.rect.y / 8.0).floor(), -b.rect.x))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            _ => None,
        };
        let Some(t) = to.copied().filter(|t| t.rect != from.rect) else {
            return Stepped::Stayed;
        };
        self.focus = Some(t);
        self.note_outside(raw);
        if let Some(l) = raw.list_of(&t).filter(|l| !l.whole) {
            if dir == Dir::Down && l.up {
                return Stepped::ScrollBy {
                    area: l.area,
                    by: -TO_THE_END,
                };
            }
            if dir == Dir::Up && l.down {
                return Stepped::ScrollBy {
                    area: l.area,
                    by: TO_THE_END,
                };
            }
        }
        Stepped::Moved
    }

    /// Bring the focus into panel `layer`: inside the list it is for, else where it says the
    /// focus starts, else its first target.
    pub fn enter(&mut self, raw: &Snapshot, layer: usize) {
        if self
            .entered
            .and_then(|a| raw.list(a))
            .is_some_and(|l| l.layer == layer)
        {
            self.entered = None;
        }
        if self.enter_inside(raw, layer) {
            return;
        }
        let seen = raw.seen(self.entered);
        self.focus = seen.home_in(layer).or_else(|| first_in(&seen, layer));
        self.note_outside(raw);
    }

    /// Enter the list panel `layer` is for, if it has one: at the place it says the focus
    /// starts, else at its first row in sight. Whether it had one.
    pub fn enter_inside(&mut self, raw: &Snapshot, layer: usize) -> bool {
        let Some(list) = raw
            .lists
            .iter()
            .find(|l| l.layer == layer && l.inside)
            .copied()
        else {
            return false;
        };
        self.entered = Some(list.area);
        let seen = raw.seen(self.entered);
        let rows: Vec<Target> = seen
            .targets
            .iter()
            .filter(|t| t.layer == layer && t.rect != list.area)
            .copied()
            .collect();
        let home = rows.iter().find(|t| raw.homes.contains(&t.rect)).copied();
        self.focus = home.or_else(|| first_row(&rows)).or(self.focus);
        true
    }

    /// Enter the list the focus rests on as a whole: the focus goes to the row it was on when the
    /// list was last left, else the first row in sight. Whether there was a list to enter.
    pub fn enter_list(&mut self, raw: &Snapshot) -> bool {
        let Some(list) = self
            .focus
            .filter(|f| f.kind == Kind::List)
            .and_then(|f| raw.list(f.rect))
            .filter(|l| l.whole)
        else {
            return false;
        };
        self.entered = Some(list.area);
        let seen = raw.seen(self.entered);
        let rows: Vec<Target> = seen
            .targets
            .iter()
            .filter(|t| t.layer == list.layer && t.rect != list.area)
            .copied()
            .collect();
        let was = self
            .rows
            .iter()
            .find(|(a, _)| *a == list.area)
            .and_then(|(_, r)| rows.iter().find(|t| t.rect == *r))
            .copied();
        if let Some(t) = was.or_else(|| first_row(&rows)) {
            self.focus = Some(t);
        }
        true
    }

    /// The focus to the first row of the list it is in (or the list entered) nearest `x` across:
    /// the column kept when a page of rows gives way to another.
    pub fn keep_column(&mut self, raw: &Snapshot, x: f32) {
        let Some(list) = self
            .entered
            .and_then(|a| raw.list(a))
            .or_else(|| self.focus.and_then(|f| raw.list_of(&f)))
        else {
            return;
        };
        let seen = raw.seen(self.entered);
        let rows: Vec<Target> = seen
            .targets
            .iter()
            .filter(|t| raw.list_of(t).is_some_and(|l| l.area == list.area))
            .copied()
            .collect();
        let Some(top) = rows
            .iter()
            .map(|t| (t.rect.y / 8.0).floor())
            .reduce(f32::min)
        else {
            return;
        };
        if let Some(t) = rows
            .into_iter()
            .filter(|t| (t.rect.y / 8.0).floor() == top)
            .min_by(|a, b| {
                (a.centre().0 - x)
                    .abs()
                    .total_cmp(&(b.centre().0 - x).abs())
            })
        {
            self.focus = Some(t);
        }
    }

    /// A step back:
    /// - to where a control sent the focus from;
    /// - out of a list its window is for, to what the list belongs to;
    /// - out of a list that is a stop until confirmed, to the list as a whole;
    /// - from a page of rows beside other content, to where the focus last was outside the
    ///   window's lists (else the first thing outside them).
    ///
    /// Whether it went anywhere; when not, the panel itself is what to cancel.
    pub fn back(&mut self, raw: &Snapshot) -> bool {
        while let Some((from, entered)) = self.sources.pop() {
            if self.focus.is_some_and(|f| f.layer != from.layer) {
                continue;
            }
            let seen = raw.seen(entered);
            if let Some(t) = seen.targets.iter().find(|t| t.rect == from.rect) {
                self.entered = entered;
                self.focus = Some(*t);
                return true;
            }
        }
        // The list entered, while the focus is in it (not in a window or menu over it).
        if let Some(list) = self
            .entered
            .and_then(|a| raw.list(a))
            .filter(|l| self.focus.is_some_and(|f| f.layer == l.layer))
        {
            if let Some(f) = self.focus {
                self.rows.retain(|(a, _)| *a != list.area);
                self.rows.push((list.area, f.rect));
            }
            self.entered = None;
            let outside = raw.seen(None);
            let owner = list
                .owner
                .and_then(|o| outside.targets.iter().find(|t| t.rect == o).copied());
            self.focus = Some(owner.unwrap_or(Target {
                rect: list.area,
                kind: Kind::List,
                layer: list.layer,
                clip: None,
            }));
            return true;
        }
        let Some(f) = self.focus else {
            return false;
        };
        let Some(list) = raw.list_of(&f).filter(|l| l.back_outside) else {
            return false;
        };
        let seen = raw.seen(None);
        let outside: Vec<Target> = seen
            .targets
            .iter()
            .filter(|t| t.layer == list.layer && raw.list_of(t).is_none())
            .copied()
            .collect();
        let name = raw.layer(list.layer).map(|l| l.name.clone());
        let last = self
            .outside
            .iter()
            .find(|(n, _)| Some(n) == name.as_ref())
            .and_then(|(_, r)| outside.iter().find(|t| t.rect == *r))
            .copied();
        match last.or_else(|| first_row(&outside)) {
            Some(t) => {
                self.focus = Some(t);
                true
            }
            None => false,
        }
    }

    /// Let the focus go: back to the world.
    pub fn resign(&mut self) {
        self.focus = None;
        self.entered = None;
        self.sources.clear();
    }
}

/// The first of `rows`, reading row by row from the top left.
fn first_row(rows: &[Target]) -> Option<Target> {
    rows.iter().copied().min_by(|a, b| {
        ((a.rect.y / 8.0).floor(), a.rect.x)
            .partial_cmp(&((b.rect.y / 8.0).floor(), b.rect.x))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// How far down a window its title bar reaches, in pixels at the usual scale: what is there (its
/// close button) is not where the focus starts.
const TITLE_BAR: f32 = 36.0;

/// The first target in panel `layer`: its top left below the title bar, or in it when there is
/// nothing below.
#[must_use]
pub fn first_in(snap: &Snapshot, layer: usize) -> Option<Target> {
    let below = snap.layer(layer).map_or(f32::MIN, |l| l.rect.y + TITLE_BAR);
    let all: Vec<Target> = snap
        .targets
        .iter()
        .filter(|t| t.layer == layer)
        .copied()
        .collect();
    let inside: Vec<Target> = all.iter().filter(|t| t.rect.y >= below).copied().collect();
    let from = if inside.is_empty() { all } else { inside };
    // A panel's main content first: its scrolling list, else anything but a line of text.
    let lists: Vec<Target> = from
        .iter()
        .filter(|t| t.kind == Kind::List)
        .copied()
        .collect();
    let plain: Vec<Target> = from
        .iter()
        .filter(|t| t.kind != Kind::Text)
        .copied()
        .collect();
    let from = if !lists.is_empty() {
        lists
    } else if plain.is_empty() {
        from
    } else {
        plain
    };
    from.into_iter().min_by(|a, b| {
        // Row by row, a few pixels counting as one row.
        ((a.rect.y / 8.0).floor(), a.rect.x)
            .partial_cmp(&((b.rect.y / 8.0).floor(), b.rect.x))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// The target nearest `at`, in `layer` when given.
fn nearest(targets: &[Target], at: (f32, f32), layer: Option<usize>) -> Option<Target> {
    targets
        .iter()
        .filter(|t| layer.is_none_or(|l| t.layer == l))
        .min_by(|a, b| {
            let d = |t: &Target| {
                let (x, y) = t.centre();
                (x - at.0).powi(2) + (y - at.1).powi(2)
            };
            d(a).total_cmp(&d(b))
        })
        .copied()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own pad support)
    use super::*;

    fn t(x: f32, y: f32, layer: usize) -> Target {
        Target {
            rect: Rect::new(x, y, 40.0, 20.0),
            kind: Kind::Plain,
            layer,
            clip: None,
        }
    }

    fn panel(name: &str, rect: Rect) -> Layer {
        Layer {
            rect,
            name: name.to_owned(),
            kind: PanelKind::Window,
            cancel: None,
            steps: None,
        }
    }

    /// A grid of three rows of three buttons, 100 apart across and 50 down.
    fn grid() -> Vec<Target> {
        let mut v = Vec::new();
        for row in 0..3 {
            for col in 0..3 {
                #[allow(clippy::cast_precision_loss)]
                v.push(t(100.0 * col as f32, 50.0 * row as f32, 0));
            }
        }
        v
    }

    #[test]
    fn the_d_pad_steps_to_the_neighbour_that_way_and_nowhere_past_the_edge() {
        let g = grid();
        let middle = g[4];
        assert_eq!(step(&g, &middle, Dir::Right), Some(g[5]));
        assert_eq!(step(&g, &middle, Dir::Left), Some(g[3]));
        assert_eq!(step(&g, &middle, Dir::Up), Some(g[1]));
        assert_eq!(step(&g, &middle, Dir::Down), Some(g[7]));
        assert_eq!(step(&g, &g[2], Dir::Right), None);
        assert_eq!(step(&g, &g[0], Dir::Up), None);
    }

    #[test]
    fn a_step_keeps_to_its_row_before_a_nearer_target_off_to_the_side() {
        let from = t(0.0, 0.0, 0);
        let along = t(300.0, 0.0, 0);
        let aside = t(60.0, 60.0, 0);
        assert_eq!(step(&[from, along, aside], &from, Dir::Right), Some(along));
    }

    #[test]
    fn a_step_never_leaves_its_panel() {
        let from = t(0.0, 0.0, 1);
        let hud = t(50.0, 0.0, 0);
        let other = t(0.0, 100.0, 2);
        let same = t(200.0, 0.0, 1);
        assert_eq!(step(&[from, hud, same], &from, Dir::Right), Some(same));
        assert_eq!(step(&[from, hud, other], &from, Dir::Right), None);
        assert_eq!(step(&[from, hud, other], &from, Dir::Down), None);
    }

    #[test]
    fn what_only_holds_others_is_dropped_and_a_modal_box_hides_what_is_behind_it() {
        let mut f = NavFrame::default();
        f.note(Rect::new(0.0, 0.0, 500.0, 500.0), Kind::Plain, &[]);
        f.note(Rect::new(10.0, 10.0, 40.0, 20.0), Kind::Plain, &[]);
        f.note(
            Rect::new(600.0, 0.0, 40.0, 20.0),
            Kind::Plain,
            &[Rect::new(590.0, 0.0, 100.0, 100.0)],
        );
        f.finish();
        assert_eq!(f.last.targets.len(), 1);
        assert_eq!(f.last.targets[0].rect, Rect::new(10.0, 10.0, 40.0, 20.0));
        f.note(Rect::new(10.0, 10.0, 40.0, 20.0), Kind::Plain, &[]);
        f.modal();
        f.layer(Rect::new(200.0, 200.0, 300.0, 200.0), "Box", PanelKind::Box);
        f.note(Rect::new(220.0, 350.0, 80.0, 30.0), Kind::Plain, &[]);
        f.cancel(Rect::new(220.0, 350.0, 80.0, 30.0));
        f.end_layer();
        f.finish();
        assert_eq!(f.last.targets.len(), 1);
        assert_eq!(f.last.targets[0].layer, 1);
        assert_eq!(
            f.last.cancel_of(1),
            Some(Rect::new(220.0, 350.0, 80.0, 30.0))
        );
    }

    #[test]
    fn a_panel_that_opens_takes_the_focus_and_gives_it_back_to_its_opener_when_it_closes() {
        let mut c = Cursor::default();
        let mut screen = Snapshot {
            targets: grid(),
            ..Snapshot::default()
        };
        screen.homes = vec![screen.targets[4].rect];
        c.follow(&screen);
        assert_eq!(c.focus, Some(screen.targets[4]), "the screen's main button");
        assert_eq!(c.step(&screen, Dir::Right), Stepped::Moved);
        c.follow(&screen);
        let opener = screen.targets[5];
        assert_eq!(c.focus, Some(opener), "kept where it was moved");
        let mut opened = screen.clone();
        opened
            .layers
            .push(panel("Log In", Rect::new(500.0, 0.0, 300.0, 300.0)));
        let a = t(700.0, 100.0, 1);
        let b = t(520.0, 100.0, 1);
        opened.targets.extend([a, b]);
        c.follow(&opened);
        assert_eq!(c.focus, Some(b), "the box's first target");
        c.follow(&opened);
        assert_eq!(c.focus, Some(b));
        c.follow(&screen);
        assert_eq!(c.focus, Some(opener), "back on the control that opened it");
    }

    #[test]
    fn the_chat_log_does_not_take_the_focus_by_being_there() {
        let mut c = Cursor::default();
        let mut snap = Snapshot {
            targets: vec![t(10.0, 900.0, 1)],
            ..Snapshot::default()
        };
        let mut chat = panel("Chat", Rect::new(0.0, 850.0, 500.0, 200.0));
        chat.kind = PanelKind::Chat;
        snap.layers.push(chat);
        c.follow(&snap);
        assert_eq!(c.focus, None);
        c.enter(&snap, 1);
        assert_eq!(c.focus.map(|f| f.layer), Some(1));
    }

    #[test]
    fn at_a_list_s_end_with_rows_out_of_sight_the_list_scrolls_instead_of_leaving() {
        let clip = Rect::new(0.0, 0.0, 200.0, 100.0);
        let row = |y: f32| Target {
            clip: Some(clip),
            ..t(0.0, y, 1)
        };
        let below_list = t(0.0, 150.0, 1);
        let mut snap = Snapshot {
            targets: vec![row(10.0), row(60.0), below_list],
            hidden: vec![row(110.0)],
            ..Snapshot::default()
        };
        snap.layers
            .push(panel("Inventory", Rect::new(0.0, 0.0, 300.0, 300.0)));
        let mut c = Cursor {
            focus: Some(row(60.0)),
            ..Cursor::default()
        };
        assert_eq!(c.step(&snap, Dir::Down), Stepped::Scroll(Dir::Down));
        assert_eq!(c.focus, Some(row(60.0)));
        assert_eq!(
            c.step(&snap, Dir::Up),
            Stepped::Moved,
            "up stays in the list"
        );
        snap.hidden.clear();
        let mut c = Cursor {
            focus: Some(row(60.0)),
            ..Cursor::default()
        };
        assert_eq!(
            c.step(&snap, Dir::Down),
            Stepped::Moved,
            "at its true end it leaves"
        );
        assert_eq!(c.focus, Some(below_list));
    }

    #[test]
    fn a_target_out_of_its_clip_is_kept_apart_as_a_row_out_of_sight() {
        let mut list = crate::draw::DrawList::default();
        let mut f = NavFrame::default();
        list.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0));
        f.note(Rect::new(10.0, 10.0, 40.0, 20.0), Kind::Plain, &[]);
        f.note(Rect::new(10.0, 150.0, 40.0, 20.0), Kind::Plain, &[]);
        list.pop_clip();
        f.finish();
        assert_eq!(f.last.targets.len(), 1);
        assert_eq!(f.last.hidden.len(), 1);
        list.clear();
    }

    #[test]
    fn past_a_panel_s_last_row_the_focus_goes_round_to_its_first_and_back() {
        let mut snap = Snapshot {
            targets: grid()
                .into_iter()
                .map(|t| Target { layer: 1, ..t })
                .collect(),
            ..Snapshot::default()
        };
        snap.layers
            .push(panel("Menu", Rect::new(0.0, 0.0, 300.0, 200.0)));
        let mut c = Cursor {
            focus: Some(snap.targets[7]),
            ..Cursor::default()
        };
        assert_eq!(c.step(&snap, Dir::Down), Stepped::Stayed);
        assert_eq!(c.wrap(&snap, Dir::Down), Stepped::Moved);
        assert_eq!(c.focus, Some(snap.targets[0]), "the first row");
        assert_eq!(c.wrap(&snap, Dir::Up), Stepped::Moved);
        assert_eq!(c.focus, Some(snap.targets[6]), "the last row");
    }

    #[test]
    fn a_window_with_nothing_to_rest_on_is_itself_a_place_for_the_focus_but_not_behind_a_modal() {
        let page = Rect::new(100.0, 100.0, 400.0, 300.0);
        let mut f = NavFrame::default();
        f.layer(page, "Letter", PanelKind::Window);
        f.end_layer();
        f.finish();
        assert_eq!(f.last.targets.len(), 1);
        assert_eq!(f.last.targets[0].rect, page);
        assert_eq!(f.last.targets[0].layer, 1);
        f.layer(page, "Letter", PanelKind::Window);
        f.end_layer();
        f.modal();
        f.layer(Rect::new(200.0, 200.0, 200.0, 100.0), "Box", PanelKind::Box);
        f.note(Rect::new(220.0, 250.0, 80.0, 30.0), Kind::Plain, &[]);
        f.end_layer();
        f.finish();
        assert_eq!(f.last.targets.len(), 1, "only the box's button");
        assert_eq!(f.last.targets[0].layer, 2);
    }

    #[test]
    fn an_opened_panel_takes_the_focus_where_it_says_it_starts() {
        let mut f = NavFrame::default();
        f.layer(
            Rect::new(0.0, 0.0, 300.0, 300.0),
            "Dropdown",
            PanelKind::Box,
        );
        f.note(Rect::new(10.0, 10.0, 200.0, 20.0), Kind::Plain, &[]);
        f.note(Rect::new(10.0, 40.0, 200.0, 20.0), Kind::Plain, &[]);
        f.home(Rect::new(10.0, 40.0, 200.0, 20.0));
        f.end_layer();
        f.finish();
        let mut c = Cursor::default();
        c.follow(&f.last);
        assert_eq!(
            c.focus.map(|t| t.rect),
            Some(Rect::new(10.0, 40.0, 200.0, 20.0)),
            "the row chosen before, not the first"
        );
    }

    /// A window with a list of five rows of 40 (three in sight, two below), each a row stop with a
    /// button at its end, and a Defaults button under the list.
    fn listed() -> (Snapshot, Rect, Target) {
        let area = Rect::new(0.0, 40.0, 300.0, 120.0);
        let row = |y: f32| Target {
            rect: Rect::new(0.0, y, 200.0, 30.0),
            kind: Kind::Plain,
            layer: 1,
            clip: Some(area),
        };
        let defaults = t(10.0, 200.0, 1);
        let mut snap = Snapshot {
            targets: vec![row(45.0), row(85.0), row(125.0), defaults],
            hidden: vec![row(165.0), row(205.0)],
            lists: vec![List {
                area,
                layer: 1,
                up: false,
                down: true,
                inside: false,
                owner: None,
                whole: false,
                back_outside: false,
            }],
            ..Snapshot::default()
        };
        snap.layers
            .push(panel("Settings", Rect::new(0.0, 0.0, 300.0, 260.0)));
        (snap, area, defaults)
    }

    #[test]
    fn a_list_s_rows_are_stops_and_a_step_past_those_in_sight_scrolls_by_exactly_one_row() {
        let (mut snap, area, defaults) = listed();
        let mut c = Cursor::default();
        c.follow(&snap);
        assert_eq!(
            c.focus.map(|f| f.rect.y),
            Some(45.0),
            "the window opens on its first row"
        );
        assert_eq!(c.step(&snap, Dir::Down), Stepped::Moved);
        assert_eq!(c.step(&snap, Dir::Down), Stepped::Moved);
        assert_eq!(
            c.step(&snap, Dir::Down),
            Stepped::ScrollBy { area, by: 35.0 },
            "the list moves just enough to show the next row whole"
        );
        // Drawn moved: the rows 35 higher, the next one now in sight at the bottom.
        let moved = |y: f32| Target {
            rect: Rect::new(0.0, y, 200.0, 30.0),
            kind: Kind::Plain,
            layer: 1,
            clip: Some(area),
        };
        let scrolled = Snapshot {
            targets: vec![moved(50.0), moved(90.0), moved(130.0), defaults],
            hidden: vec![moved(10.0), moved(170.0)],
            ..snap.clone()
        };
        c.follow(&scrolled);
        assert_eq!(
            c.focus.map(|f| f.rect.y),
            Some(130.0),
            "once moved, the focus on the next row, at the list's foot"
        );
        // At the list's true end, down leaves it for what lies below.
        snap = scrolled;
        snap.hidden.retain(|t| t.rect.y < 100.0);
        snap.lists[0].down = false;
        snap.lists[0].up = true;
        assert_eq!(c.step(&snap, Dir::Down), Stepped::Moved);
        assert_eq!(c.focus, Some(defaults));
        // Down from the last control: round to the first row, the list back at its top.
        assert_eq!(c.step(&snap, Dir::Down), Stepped::Stayed);
        assert_eq!(
            c.wrap(&snap, Dir::Down),
            Stepped::ScrollBy {
                area,
                by: -TO_THE_END
            }
        );
        assert_eq!(c.focus.map(|f| f.rect.y), Some(50.0));
        assert!(
            !c.back(&snap),
            "B from a row: the window is what is cancelled"
        );
    }

    #[test]
    fn a_row_only_partly_in_sight_is_scrolled_wholly_into_it() {
        let (mut snap, area, _) = listed();
        // A fourth row whose top two thirds are in sight.
        snap.hidden.clear();
        snap.targets.insert(
            3,
            Target {
                rect: Rect::new(0.0, 140.0, 200.0, 30.0),
                kind: Kind::Plain,
                layer: 1,
                clip: Some(area),
            },
        );
        snap.targets[0].rect.y = 20.0;
        snap.targets[1].rect.y = 60.0;
        snap.targets[2].rect.y = 100.0;
        let mut c = Cursor::default();
        c.follow(&snap);
        c.focus = Some(snap.targets[2]);
        assert_eq!(
            c.step(&snap, Dir::Down),
            Stepped::ScrollBy { area, by: 10.0 }
        );
        for t in &mut snap.targets {
            t.rect.y -= 10.0;
        }
        c.follow(&snap);
        assert_eq!(c.focus.map(|f| f.rect.y), Some(130.0), "on it, once moved");
    }

    #[test]
    fn a_step_back_returns_the_focus_to_where_a_control_sent_it_from() {
        let (mut snap, _, defaults) = listed();
        let mut c = Cursor::default();
        c.follow(&snap);
        c.step(&snap, Dir::Down);
        let row = c.focus.unwrap();
        snap.want = Some(defaults.rect);
        c.follow(&snap);
        assert_eq!(c.focus, Some(defaults));
        snap.want = None;
        c.follow(&snap);
        assert!(c.back(&snap));
        assert_eq!(c.focus, Some(row), "back on the row");
    }

    #[test]
    fn a_list_its_window_is_for_opens_entered_keeps_the_d_pad_and_steps_back_to_its_owner() {
        let (mut snap, area, defaults) = listed();
        snap.lists[0].inside = true;
        snap.lists[0].owner = Some(defaults.rect);
        let mut c = Cursor::default();
        c.follow(&snap);
        assert_eq!(c.entered, Some(area), "opened inside its list");
        assert_eq!(c.focus.map(|f| f.rect.y), Some(45.0), "at its first row");
        assert!(c.back(&snap));
        assert_eq!(c.focus, Some(defaults), "out to what the list belongs to");
        assert_eq!(
            c.step(&snap, Dir::Up),
            Stepped::Stayed,
            "and not back in by the d-pad: its rows are not stops until confirmed"
        );
        assert!(c.enter_inside(&snap, 1));
        assert_eq!(c.focus.map(|f| f.rect.y), Some(45.0));
    }

    #[test]
    fn a_page_of_rows_steps_back_to_where_the_focus_last_was_outside_it() {
        let (mut snap, area, _) = listed();
        snap.lists[0].back_outside = true;
        let slot = t(320.0, 60.0, 1);
        snap.targets.push(slot);
        let mut c = Cursor::default();
        c.follow(&snap);
        c.focus = Some(slot);
        c.follow(&snap);
        assert_eq!(c.step(&snap, Dir::Left), Stepped::Moved);
        assert!(snap
            .list_of(&c.focus.unwrap())
            .is_some_and(|l| l.area == area));
        c.step(&snap, Dir::Down);
        assert!(c.back(&snap));
        assert_eq!(c.focus, Some(slot), "back to the slot it came from");
    }

    #[test]
    fn a_window_with_nothing_in_it_opened_first_thing_takes_the_focus() {
        let page = Rect::new(100.0, 100.0, 400.0, 300.0);
        let mut f = NavFrame::default();
        f.layer(page, "Letter From Home", PanelKind::Window);
        f.end_layer();
        f.finish();
        let mut c = Cursor::default();
        c.follow(&f.last);
        assert_eq!(c.focus.map(|t| (t.rect, t.layer)), Some((page, 1)));
    }

    #[test]
    fn a_menu_opened_from_a_row_of_a_list_its_window_is_for_closes_back_onto_that_row() {
        let (mut snap, area, owner) = listed();
        snap.lists[0].inside = true;
        snap.lists[0].owner = Some(owner.rect);
        let mut c = Cursor::default();
        c.follow(&snap);
        c.step(&snap, Dir::Down);
        let row = c.focus.unwrap();
        // The menu opens over the list, which is out of sight under it for the while.
        let mut menu = snap.clone();
        menu.lists.clear();
        menu.layers
            .push(panel("Item Menu", Rect::new(0.0, 60.0, 200.0, 100.0)));
        menu.targets.push(t(10.0, 70.0, 2));
        c.follow(&menu);
        assert_eq!(c.focus.map(|f| f.layer), Some(2), "the menu has the focus");
        c.follow(&snap);
        assert_eq!(c.focus, Some(row), "back on the row it was opened from");
        assert_eq!(c.entered, Some(area), "still inside the list");
    }

    #[test]
    fn b_in_a_window_opened_over_a_list_entered_is_that_window_s_not_the_list_s() {
        let (mut snap, _, owner) = listed();
        snap.lists[0].inside = true;
        snap.lists[0].owner = Some(owner.rect);
        let mut c = Cursor::default();
        c.follow(&snap);
        assert!(c.entered.is_some());
        // A book opened from a row, over the window the list is in.
        let mut book = snap.clone();
        book.layers
            .push(panel("Book", Rect::new(400.0, 0.0, 300.0, 300.0)));
        book.targets.push(t(420.0, 100.0, 2));
        c.follow(&book);
        assert_eq!(c.focus.map(|f| f.layer), Some(2));
        assert!(
            !c.back(&book),
            "nothing to step back to inside the book: the book is what B closes"
        );
        assert_eq!(c.focus.map(|f| f.layer), Some(2));
    }
}
