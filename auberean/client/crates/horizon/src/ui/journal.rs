//! The journal window: the quests under way, those finished, and the story's acts.
//!
//! - **Underway** and **Finished**: the quests, newest given first, each a row with its name and
//!   its time (the time left before it runs out; for one finished, before it may be taken again,
//!   or that it may be taken no more). The quest chosen shows its description, its journal entry,
//!   and how hard it is or how it ended; one under way that may be given up has Abandon, which
//!   asks first. A description longer than the pane scrolls, its lines wrapped clear of the
//!   scrollbar. A quest the game changes is marked until its row is pressed or the window
//!   closes, and becomes the one chosen, its page shown.
//! - **Story**: the story's acts, each open to list its scenes; a scene the player has seen has
//!   Play, which plays it again. An act the game changes is marked until its row is pressed.

use std::collections::BTreeSet;

use auberean_client_sdk::Action;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::Rect;
use crate::journal::{time_left, Quest};
use crate::state::GameState;

/// The window's body, in layout units.
const BODY: (f32, f32) = (640.0, 460.0);

/// The pages, as their tabs read.
pub const TABS: [&str; 3] = ["Underway", "Finished", "Story"];

/// A row's height, in layout units: a quest's, an act's, a scene's.
pub const ROW: f32 = 40.0;
pub const SCENE_ROW: f32 = 28.0;

/// The mark of a quest or an act that changed.
pub const CHANGED: u32 = 0xFFFF_D860;

/// The journal's pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    UnderWay,
    Finished,
    Story,
}

impl Page {
    const ALL: [Self; 3] = [Self::UnderWay, Self::Finished, Self::Story];

    fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }
}

/// Where the window's parts are, in a window drawn with `body` as its body, at `k` pixels a
/// layout unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parts {
    pub tabs: Rect,
    /// The list of quests or acts; the chosen quest's pane, which stops above its foot.
    pub list: Rect,
    pub pane: Rect,
    /// The pane's text, which scrolls; its scrollbar stands at its right.
    pub text: Rect,
    /// Under the pane: give up, and its question's yes and no.
    pub give_up: Rect,
    pub yes: Rect,
    pub no: Rect,
}

/// The journal window.
#[derive(Debug, Default)]
pub struct JournalWindow {
    window: WindowState,
    drawn: Option<(Rect, Rect)>,
    pub page: Page,
    /// The quest chosen; the quest asked about giving up.
    pub chosen: Option<u32>,
    pub giving_up: Option<u32>,
    /// The quests and acts marked changed.
    pub marked: BTreeSet<u32>,
    pub acts_marked: BTreeSet<u32>,
    /// The acts open.
    pub open_acts: BTreeSet<u32>,
    scroll: f32,
    /// How far the chosen quest's pane is scrolled, and the quest it is scrolled for.
    pane_scroll: f32,
    pane_of: Option<u32>,
    /// The pane's lines as last drawn: each line's words and where they stand, scrolled.
    pane_lines: Vec<(String, Rect)>,
}

impl JournalWindow {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.window.open
    }

    pub fn toggle(&mut self) {
        if self.window.open {
            self.close();
        } else {
            self.window.open = true;
        }
    }

    /// Close it: the marks of what changed go, and the question.
    pub fn close(&mut self) {
        self.window.open = false;
        self.drawn = None;
        self.giving_up = None;
        self.marked.clear();
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

    /// The chosen quest's lines as last drawn: each line's words and where they stand, scrolled
    /// (a line scrolled out of the pane is not drawn, but still listed).
    #[must_use]
    pub fn pane_lines(&self) -> &[(String, Rect)] {
        &self.pane_lines
    }

    /// What the game changed this frame: each quest changed marked and chosen, its page shown;
    /// each act changed marked. Heard whether the window is open or not.
    pub fn hear(&mut self, state: &GameState) {
        let j = &state.journal;
        for id in &j.changed_now {
            self.marked.insert(*id);
            self.chosen = Some(*id);
            if self.page != Page::Story {
                if let Some(q) = j.quests.iter().find(|q| q.id == *id) {
                    self.page = if q.under_way() {
                        Page::UnderWay
                    } else {
                        Page::Finished
                    };
                }
            }
        }
        self.acts_marked.extend(j.acts_changed_now.iter().copied());
        if j.quests.is_empty() && state.journal.acts.is_none() && !state.in_world {
            self.marked.clear();
            self.acts_marked.clear();
            self.chosen = None;
        }
    }

    /// The quests on a page, in order.
    #[must_use]
    pub fn listed(state: &GameState, page: Page) -> Vec<&Quest> {
        state
            .journal
            .quests
            .iter()
            .filter(|q| match page {
                Page::UnderWay => q.under_way(),
                Page::Finished => !q.under_way(),
                Page::Story => false,
            })
            .collect()
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
        let top = tabs.bottom() + 8.0 * k;
        let h = body.bottom() - top - 8.0 * k;
        let list = Rect::new(body.x + 8.0 * k, top, body.w * 0.42, h);
        let foot = list.bottom() - 28.0 * k;
        let pane = Rect::new(
            list.right() + 10.0 * k,
            top,
            body.right() - list.right() - 18.0 * k,
            foot - 8.0 * k - top,
        );
        let no = Rect::new(pane.right() - 90.0 * k, foot, 90.0 * k, 28.0 * k);
        Parts {
            tabs,
            list,
            pane,
            text: Rect::new(
                pane.x + 12.0 * k,
                pane.y + 8.0 * k,
                pane.w - 16.0 * k,
                pane.h - 16.0 * k,
            ),
            give_up: Rect::new(pane.right() - 120.0 * k, foot, 120.0 * k, 28.0 * k),
            yes: Rect::new(no.x - 98.0 * k, foot, 90.0 * k, 28.0 * k),
            no,
        }
    }

    /// The `i`th quest row of a list at `list`, at `k` pixels a layout unit, scrolled by `scroll`.
    #[must_use]
    pub fn row(list: Rect, i: usize, k: f32, scroll: f32) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        let y = list.y + 4.0 * k + i as f32 * (ROW + 2.0) * k - scroll;
        Rect::new(list.x, y, list.w - 10.0 * k, ROW * k)
    }

    /// A quest's time as its row shows it, the game's time `now`.
    #[must_use]
    pub fn time_of(q: &Quest, now: f64) -> String {
        if q.maxed_out {
            return "No more".to_owned();
        }
        match q.until {
            Some(t) if t > now && q.under_way() => format!("{} left", time_left(t - now)),
            Some(t) if t > now => format!("Again in {}", time_left(t - now)),
            _ => String::new(),
        }
    }

    /// Draw it while it is open, asking for the actions in `acts`; where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        acts: &mut Vec<Action>,
    ) -> Option<Rect> {
        if !self.window.open {
            return None;
        }
        let k = p.scale;
        let size = kit::window_size_for(p, BODY);
        let at = (
            (p.screen.0 * 0.3 - size.0 * k * 0.5).max(0.0).round(),
            ((p.screen.1 - size.1 * k) / 2.0).round(),
        );
        let at = crate::ui::place::clear(p, state, size, at);
        let w = kit::window(p, ctx, &mut self.window, "Journal", size, at);
        if w.closed {
            self.close();
            return Some(w.rect);
        }
        self.drawn = Some((w.rect, w.body));
        let parts = Self::parts(w.body, k);
        let mut tab = self.page.index();
        if kit::tabs(p, ctx, parts.tabs, &TABS, &mut tab) {
            self.page = Page::ALL[tab];
            self.scroll = 0.0;
            self.giving_up = None;
        }
        if self.page == Page::Story {
            self.story(p, ctx, state, parts, acts);
        } else {
            self.quests(p, ctx, state, parts, acts);
        }
        Some(w.rect)
    }

    fn quests(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        parts: Parts,
        acts: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let now = state.game_time;
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let lit = TextStyle::new(Family::Body, 13.0, CHANGED).edge(ctx.colours.edge());
        let list = parts.list;
        p.fill(list, 0x6010_0C08);
        let quests = Self::listed(state, self.page);
        if quests.is_empty() {
            let none = if self.page == Page::UnderWay {
                "No quests underway."
            } else {
                "No quests finished."
            };
            p.text_in(&dim, list, Align::Centre, none);
        }
        #[allow(clippy::cast_precision_loss)]
        let content = quests.len() as f32 * (ROW + 2.0) * k + 8.0 * k;
        let scroll = kit::scroll(p, ctx, list, content, &mut self.scroll);
        p.list.push_clip(list);
        for (i, q) in quests.iter().enumerate() {
            let row = Self::row(list, i, k, scroll);
            if row.bottom() < list.y || row.y > list.bottom() {
                continue;
            }
            let on = self.chosen == Some(q.id);
            if on || ctx.input.hover(&row) {
                kit::list_highlight(p, row, on);
            }
            let style = if self.marked.contains(&q.id) {
                &lit
            } else {
                &name
            };
            let shown = p.fit(style, &q.name, row.w - 16.0 * k);
            p.text(style, row.x + 8.0 * k, row.y + 3.0 * k, &shown);
            p.text(
                &dim,
                row.x + 8.0 * k,
                row.y + 21.0 * k,
                &Self::time_of(q, now),
            );
            if ctx.over(&row) && ctx.input.clicked(&row) {
                self.marked.remove(&q.id);
                self.chosen = if on { None } else { Some(q.id) };
                self.giving_up = None;
            }
        }
        p.list.pop_clip();
        let pane = parts.pane;
        kit::panel(p, pane, 0.5);
        self.pane_lines.clear();
        let Some(q) = self
            .chosen
            .and_then(|id| quests.iter().find(|q| q.id == id).copied())
        else {
            p.text_in(&dim, pane, Align::Centre, "Choose a quest.");
            return;
        };
        if self.pane_of != Some(q.id) {
            self.pane_of = Some(q.id);
            self.pane_scroll = 0.0;
        }
        let head =
            TextStyle::new(Family::Heading, 16.0, ctx.colours.heading()).edge(ctx.colours.edge());
        // The lines wrap short of the scrollbar a long description brings.
        let text = parts.text;
        let width = text.w - kit::scrollbar_width(p).map_or(12.0, |w| w + 6.0) * k;
        let standing = match q.ending() {
            Some(e) => e.to_owned(),
            None => match q.challenge {
                Some(level) => format!("Intended Level: {level}"),
                None => "Intended Level: Any".to_owned(),
            },
        };
        let time = Self::time_of(q, now);
        let mut page: Vec<(&TextStyle, &str, f32)> = vec![
            (&head, q.name.as_str(), 0.0),
            (&dim, standing.as_str(), 4.0),
        ];
        if !time.is_empty() {
            page.push((&dim, time.as_str(), 0.0));
        }
        if q.under_way() {
            page.push((&name, q.description.as_str(), 8.0));
            if !q.journal.is_empty() {
                page.push((&name, q.journal.as_str(), 8.0));
            }
        }
        // Each line with its face and how far down the page it stands.
        let mut lines: Vec<(&TextStyle, String, f32)> = Vec::new();
        let mut y = 0.0;
        for (style, words, gap) in page {
            y += gap * k;
            let lh = p.line_height(style);
            for l in p.wrap(style, words, width) {
                lines.push((style, l, y));
                y += lh;
            }
        }
        let offset = kit::scroll(p, ctx, text, y + 4.0 * k, &mut self.pane_scroll);
        p.list.push_clip(text);
        for (style, l, at) in lines {
            let lh = p.line_height(style);
            let at = Rect::new(text.x, text.y + at - offset, p.measure(style, &l), lh);
            if at.bottom() >= text.y && at.y <= text.bottom() {
                p.text(style, at.x, at.y, &l);
            }
            self.pane_lines.push((l, at));
        }
        p.list.pop_clip();
        if !q.may_give_up() {
            return;
        }
        if self.giving_up == Some(q.id) {
            p.text_in(
                &name,
                Rect::new(
                    pane.x,
                    parts.yes.y,
                    parts.yes.x - pane.x - 8.0 * k,
                    parts.yes.h,
                ),
                Align::Right,
                "Abandon this quest?",
            );
            if kit::button(p, ctx, parts.yes, "Yes", true) {
                acts.push(Action::CancelQuest { quest: q.id });
                self.giving_up = None;
            }
            if kit::button(p, ctx, parts.no, "No", true) {
                self.giving_up = None;
            }
        } else if kit::button(p, ctx, parts.give_up, "Abandon", true) {
            self.giving_up = Some(q.id);
        }
    }

    fn story(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        parts: Parts,
        acts: &mut Vec<Action>,
    ) {
        let k = p.scale;
        let name = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let lit = TextStyle::new(Family::Body, 14.0, CHANGED).edge(ctx.colours.edge());
        let left = parts.list;
        let area = Rect::new(left.x, left.y, parts.pane.right() - left.x, left.h);
        p.fill(area, 0x6010_0C08);
        let Some(story) = state.journal.acts.as_ref().filter(|a| !a.is_empty()) else {
            p.text_in(&dim, area, Align::Centre, "No story yet.");
            return;
        };
        let rows = story
            .iter()
            .map(|a| {
                ROW + if self.open_acts.contains(&a.id) {
                    #[allow(clippy::cast_precision_loss)]
                    let n = a.scenes.len() as f32;
                    n * SCENE_ROW
                } else {
                    0.0
                } + 2.0
            })
            .sum::<f32>();
        let scroll = kit::scroll(p, ctx, area, rows * k + 8.0 * k, &mut self.scroll);
        p.list.push_clip(area);
        let mut y = area.y + 4.0 * k - scroll;
        let width = area.w - 10.0 * k;
        for act in story {
            let row = Rect::new(area.x, y, width, ROW * k);
            let open = self.open_acts.contains(&act.id);
            if open || ctx.input.hover(&row) {
                kit::list_highlight(p, row, open);
            }
            let style = if self.acts_marked.contains(&act.id) {
                &lit
            } else {
                &name
            };
            let mark = if open { "-" } else { "+" };
            p.text_in(
                style,
                row.offset(8.0 * k, 0.0),
                Align::Left,
                &format!("{mark}  {}", act.name),
            );
            let seen = act.scenes.iter().filter(|s| s.playable).count();
            p.text_in(
                &dim,
                Rect::new(row.x, row.y, row.w - 12.0 * k, row.h),
                Align::Right,
                &format!("{seen} of {} seen", act.scenes.len()),
            );
            if ctx.over(&row) && ctx.input.clicked(&row) {
                self.acts_marked.remove(&act.id);
                if open {
                    self.open_acts.remove(&act.id);
                } else {
                    self.open_acts.insert(act.id);
                }
            }
            y += (ROW + 2.0) * k;
            if !open {
                continue;
            }
            for scene in &act.scenes {
                let r = Rect::new(area.x + 28.0 * k, y, width - 28.0 * k, SCENE_ROW * k);
                let style = if scene.playable { &name } else { &dim };
                p.text_in(style, r, Align::Left, &scene.name);
                let play = Rect::new(r.right() - 70.0 * k, r.y + 2.0 * k, 64.0 * k, r.h - 4.0 * k);
                if scene.playable && kit::button(p, ctx, play, "Play", true) {
                    acts.push(Action::PlayStoryScene {
                        act: act.id,
                        scene: scene.id,
                    });
                }
                y += SCENE_ROW * k;
            }
        }
        p.list.pop_clip();
    }
}
