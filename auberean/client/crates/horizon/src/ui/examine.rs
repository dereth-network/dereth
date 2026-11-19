//! The examine window: what the player examines (the selection, a skill, an item on the hotbar),
//! its profile's lines, and the tabs they show on: General, Details, Effects and Usage.
//!
//! Opened, the window asks the game to examine with its tabs; closed, to stop examining. The
//! classic window (the player's options choose it, as the game's own) has no tabs and asks for
//! every tab's lines, one page in the profile's own order; switched while open, it asks again. In
//! the tabbed one a tab the profile has no line for is shown disabled; pressing another asks to
//! examine with it, as do the keys for the next and the previous tab. Before a profile comes the
//! window says it waits for one, or that nothing is examined. The lines flow left to right and
//! wrap, pictures and icons in among the words; a break starts a new line and an indent moves on,
//! as does a line break in a text; a countdown reads the time left. The wheel scrolls a long
//! profile, its lines wrapped clear of the scrollbar.

use auberean_client_sdk::contract::notice::examination::tab;
use auberean_client_sdk::contract::Action;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::{Rect, WHITE};
use crate::examined::Piece;
use crate::state::GameState;

/// The window's body, in layout units.
const BODY: (f32, f32) = (320.0, 440.0);

/// The tabs, as the window lists them, and the lines each shows.
pub const TABS: [(&str, u32); 4] = [
    ("General", tab::GENERAL),
    ("Details", tab::DETAILS),
    ("Effects", tab::EFFECTS),
    ("Usage", tab::USAGE),
];

/// An indent's width, and a picture's side, in layout units.
pub const INDENT: f32 = 16.0;
pub const PICTURE: f32 = 32.0;

/// How a tab is shown: asked for, there to be asked for, or with no line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabState {
    Active,
    Inactive,
    Disabled,
}

/// How a tab of `bits` is shown when `asked` were asked for and `made` have lines.
#[must_use]
pub const fn tab_state(bits: u32, asked: u32, made: u32) -> TabState {
    if bits & asked != 0 {
        TabState::Active
    } else if bits & made == 0 {
        TabState::Disabled
    } else {
        TabState::Inactive
    }
}

/// Where the classic window's lines are, in a window drawn with `body` as its body: the whole
/// body, a margin in.
#[must_use]
pub fn classic_lines(body: Rect, k: f32) -> Rect {
    Rect::new(
        body.x + 10.0 * k,
        body.y + 10.0 * k,
        body.w - 20.0 * k,
        body.h - 20.0 * k,
    )
}

/// Where the tabbed window's parts are, in a window drawn with `body` as its body: the tabs,
/// sharing the width, and the lines.
#[must_use]
pub fn parts(body: Rect, k: f32) -> ([Rect; 4], Rect) {
    let w = (body.w - 14.0 * k) / 4.0;
    #[allow(clippy::cast_precision_loss)]
    let tab = |i: usize| {
        Rect::new(
            body.x + 4.0 * k + i as f32 * (w + 2.0 * k),
            body.y + 6.0 * k,
            w,
            26.0 * k,
        )
    };
    (
        [tab(0), tab(1), tab(2), tab(3)],
        Rect::new(
            body.x + 10.0 * k,
            body.y + 42.0 * k,
            body.w - 20.0 * k,
            body.h - 52.0 * k,
        ),
    )
}

/// What is left until a countdown's end, `seconds` of it: months and days above a month, days and
/// hours above a day, hours and minutes above an hour, else minutes and seconds.
#[must_use]
pub fn countdown(seconds: f64) -> String {
    let s = dereth_primitives::num::to_i32_f64(seconds.max(0.0).ceil());
    let (day, month) = (86_400, 30 * 86_400);
    if s >= month {
        format!("({}mo {}d)", s / month, s % month / day)
    } else if s >= day {
        format!("({}d {}h)", s / day, s % day / 3600)
    } else if s >= 3600 {
        format!("({}h {}m)", s / 3600, s % 3600 / 60)
    } else {
        format!("({}m {}s)", s / 60, s % 60)
    }
}

/// The examine window.
#[derive(Debug)]
pub struct ExamineWindow {
    window: WindowState,
    drawn: Option<(Rect, Rect)>,
    /// The tabs the tabbed window asks for.
    pub tabs: u32,
    /// It is the classic window.
    pub classic: bool,
    scroll: f32,
}

impl Default for ExamineWindow {
    fn default() -> Self {
        Self {
            window: WindowState::default(),
            drawn: None,
            tabs: tab::GENERAL,
            classic: true,
            scroll: 0.0,
        }
    }
}

impl ExamineWindow {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.window.open
    }

    /// The tabs it asks for: every tab in the classic window, else those chosen.
    #[must_use]
    pub const fn asked(&self) -> u32 {
        if self.classic {
            tab::ALL
        } else {
            self.tabs
        }
    }

    /// The classic window or the tabbed one, as `classic`; switched while open, it asks the game
    /// to examine again with its tabs (the tabbed one with the first).
    pub fn set_classic(&mut self, classic: bool, actions: &mut Vec<Action>) {
        if classic == self.classic {
            return;
        }
        self.classic = classic;
        self.tabs = tab::GENERAL;
        self.scroll = 0.0;
        if self.window.open {
            actions.push(Action::Examination {
                tabs: Some(self.asked()),
            });
        }
    }

    /// Open it, asking the game to examine with its tabs (again, when it is open).
    pub fn open(&mut self, actions: &mut Vec<Action>) {
        self.window.open = true;
        actions.push(Action::Examination {
            tabs: Some(self.asked()),
        });
    }

    /// Close it, asking the game to stop examining; nothing when it is closed.
    pub fn close(&mut self, actions: &mut Vec<Action>) {
        if !self.window.open {
            return;
        }
        self.window.open = false;
        self.drawn = None;
        actions.push(Action::Examination { tabs: None });
    }

    pub fn toggle(&mut self, actions: &mut Vec<Action>) {
        if self.window.open {
            self.close(actions);
        } else {
            self.open(actions);
        }
    }

    /// Ask for one tab's lines.
    pub fn choose(&mut self, bits: u32, actions: &mut Vec<Action>) {
        self.tabs = bits;
        self.scroll = 0.0;
        actions.push(Action::Examination { tabs: Some(bits) });
    }

    /// The tab after the one shown (`next`) or before it that has lines, round past either end.
    pub fn step(&mut self, next: bool, made: u32, actions: &mut Vec<Action>) {
        if !self.window.open || self.classic {
            return;
        }
        let at = TABS
            .iter()
            .rposition(|(_, b)| b & self.tabs != 0)
            .unwrap_or(0);
        let n = TABS.len();
        let found = (1..n)
            .map(|i| if next { (at + i) % n } else { (at + n - i) % n })
            .map(|i| TABS[i].1)
            .find(|b| b & made != 0);
        if let Some(b) = found {
            self.choose(b, actions);
        }
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

    /// Draw it while it is open; where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        actions: &mut Vec<Action>,
    ) -> Option<Rect> {
        if !self.window.open {
            return None;
        }
        let k = p.scale;
        let size = kit::window_size_for(p, BODY);
        let at = (
            (p.screen.0 * 0.78 - size.0 * k / 2.0).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let at = crate::ui::place::clear(p, state, size, at);
        let w = kit::window(p, ctx, &mut self.window, "Examine", size, at);
        if w.closed {
            self.window.open = true;
            self.close(actions);
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        let e = &state.examined;
        let (tabs, lines) = parts(w.body, k);
        let lines = if self.classic {
            classic_lines(w.body, k)
        } else {
            for (r, (label, bits)) in tabs.iter().zip(TABS) {
                let s = tab_state(bits, e.asked, e.made);
                if kit::button_lit(
                    p,
                    ctx,
                    *r,
                    label,
                    s != TabState::Disabled,
                    s == TabState::Active,
                ) && s == TabState::Inactive
                {
                    self.choose(bits, actions);
                }
            }
            lines
        };
        let body = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let nothing = e.request.is_none_or(|r| r.kind == 0);
        if nothing || e.profile.is_none() {
            let said = if nothing {
                "Nothing is selected."
            } else {
                "Examining..."
            };
            p.text_in(
                &body,
                Rect::new(lines.x, lines.y, lines.w, 20.0 * k),
                Align::Left,
                said,
            );
            return Some(w.rect);
        }
        // The lines wrap short of the scrollbar a long profile brings.
        let room = kit::scrollbar_width(p).map_or(12.0, |w| w + 6.0) * k;
        let text = Rect::new(lines.x, lines.y, lines.w - room, lines.h);
        let height = self.lay_out(p, &body, text, &e.pieces, state.game_time, true);
        let offset = kit::scroll(p, ctx, lines, height, &mut self.scroll);
        self.scroll = offset;
        p.list.push_clip(lines);
        self.lay_out(p, &body, text, &e.pieces, state.game_time, false);
        p.list.pop_clip();
        Some(w.rect)
    }

    /// The pieces laid out in `area` from the top, scrolled; drawn unless `measure`; how tall
    /// they are.
    fn lay_out(
        &self,
        p: &mut Painter<'_>,
        style: &TextStyle,
        area: Rect,
        pieces: &[Piece],
        now: f64,
        measure: bool,
    ) -> f32 {
        let k = p.scale;
        let line_h = p.line_height(style).max(16.0 * k);
        let (mut x, mut y, mut bottom, mut indent) = (0.0_f32, 0.0_f32, line_h, 0.0_f32);
        let top = area.y - self.scroll;
        for piece in pieces {
            let text = match piece {
                Piece::Text(Some(t)) => t.clone(),
                Piece::Text(None) => continue,
                Piece::Countdown(end) => countdown(end - now),
                Piece::Picture(_) | Piece::Icon(_) => {
                    let side = PICTURE * k;
                    if x + side > area.w && x > indent {
                        y = bottom;
                        bottom = y + line_h;
                        x = indent;
                    }
                    bottom = bottom.max(y + side);
                    let at = Rect::new(area.x + x, top + y, side.min(area.w - x), side);
                    if !measure && at.bottom() >= area.y && at.y <= area.bottom() {
                        let ids: Vec<u32> = match piece {
                            Piece::Picture(id) => vec![*id],
                            Piece::Icon(layers) => layers.iter().map(|(id, _)| *id).collect(),
                            _ => Vec::new(),
                        };
                        for id in ids {
                            if let Some(i) = p.art.icon(id) {
                                p.sprite(&i, at, WHITE);
                            }
                        }
                    }
                    x += side;
                    continue;
                }
                Piece::Break => {
                    y = bottom;
                    bottom = y + line_h;
                    x = 0.0;
                    indent = 0.0;
                    continue;
                }
                Piece::Indent(n) => {
                    #[allow(clippy::cast_precision_loss)]
                    let by = *n as f32 * INDENT * k;
                    x += by;
                    indent += by;
                    continue;
                }
            };
            // Word by word: a word that does not fit goes on a new line, keeping the indent; one
            // wider than a whole line is broken where the line ends. A line break in the text
            // starts a new line at the indent (two, a blank line between).
            for (i, part) in text.split('\n').enumerate() {
                if i > 0 {
                    y = bottom;
                    bottom = y + line_h;
                    x = indent;
                }
                let part = part.strip_suffix('\r').unwrap_or(part);
                let mut words: Vec<String> = part.split_inclusive(' ').map(str::to_owned).collect();
                words.reverse();
                while let Some(word) = words.pop() {
                    let inked = p.measure(style, word.trim_end());
                    if x + inked > area.w && x > indent {
                        y = bottom;
                        bottom = y + line_h;
                        x = indent;
                    }
                    let word = if x + inked > area.w {
                        let (head, rest) = split_to_fit(p, style, &word, area.w - x);
                        if !rest.is_empty() {
                            words.push(rest);
                        }
                        head
                    } else {
                        word
                    };
                    let w = p.measure(style, &word);
                    bottom = bottom.max(y + line_h);
                    if !measure {
                        let at = Rect::new(area.x + x, top + y, w.min(area.w - x), line_h);
                        if at.bottom() >= area.y && at.y <= area.bottom() {
                            p.text_in(style, at, Align::Left, &word);
                        }
                    }
                    x += w;
                    if !words.is_empty() && x >= area.w {
                        y = bottom;
                        bottom = y + line_h;
                        x = indent;
                    }
                }
            }
        }
        bottom
    }
}

/// `word` split where `room` ends: the most of its letters that fit (at least one), and the rest.
fn split_to_fit(p: &Painter<'_>, style: &TextStyle, word: &str, room: f32) -> (String, String) {
    let mut end = word.chars().next().map_or(0, char::len_utf8);
    for (i, c) in word.char_indices().skip(1) {
        if p.measure(style, &word[..i + c.len_utf8()]) > room {
            break;
        }
        end = i + c.len_utf8();
    }
    (word[..end].to_owned(), word[end..].to_owned())
}
