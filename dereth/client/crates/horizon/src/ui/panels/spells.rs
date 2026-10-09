//! The Spellbook window: the spells of each school, a search over their names, the chosen spell's
//! details as the game's spell examination words them (with its components' icons), and what can
//! be done with it: put it on the current spell bar (a double-click or a drag to a slot of the
//! bar does the same, as the game's spellbook does), or forget it after a question.
//!
//! The spellbook's level filter is the game's, kept by the server: a level switched off hides its
//! spells. The tabs follow the world's era: Void magic only where the world has it, and a Create
//! Spell tab where it has spell research.

use dereth_client_contract::UiRequest;
use dereth_ui_screens::panels::spell_examine as words;

use super::Windows;
use crate::art::Family;
use crate::draw::{Rect, WHITE};
use crate::ui::game::GameState;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The schools, as the tabs name them, with the game's school numbers.
const SCHOOLS: [(&str, u32); 5] = [
    ("War", 1),
    ("Life", 2),
    ("Item", 3),
    ("Creature", 4),
    ("Void", 5),
];

/// What a tab of the Spellbook shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    School(usize),
    Components,
    Research,
}

/// The tabs this world offers, in order.
fn tabs(state: &GameState) -> Vec<(&'static str, Tab)> {
    let w = &state.world_services;
    let void = !state.in_world || w.era_ui.void_magic;
    let mut out: Vec<(&'static str, Tab)> = SCHOOLS
        .iter()
        .enumerate()
        .filter(|(_, (_, school))| *school != 5 || void)
        .map(|(i, (name, _))| (*name, Tab::School(i)))
        .collect();
    out.push(("Components", Tab::Components));
    if w.features.spell_research && state.in_world {
        out.push((dereth_ui_screens::panels::research::TAB_TEXT, Tab::Research));
    }
    out
}

/// The level filters, as the spellbook names them.
const LEVELS: [&str; 8] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII"];

/// The text box the spell search is typed into.
const SEARCH_FIELD: u32 = 60;

/// The share of the window's width the spell list takes; the chosen spell's details have the
/// rest.
const LIST_SHARE: f32 = 0.47;

/// The search box's height, in layout units: room for its text above and below.
const SEARCH_H: f32 = 34.0;

/// How far the spell list starts below the top of the window's body, in layout units: under the
/// tabs and the search box.
const LIST_TOP: f32 = 74.0;

/// The level a search word names, written as the spellbook writes levels (`viii`) or as a
/// number (`8`).
fn level_word(word: &str) -> Option<u32> {
    let w = word.to_ascii_lowercase();
    LEVELS
        .iter()
        .position(|l| l.eq_ignore_ascii_case(&w))
        .or_else(|| {
            w.parse::<usize>()
                .ok()
                .filter(|n| (1..=LEVELS.len()).contains(n))
                .map(|n| n - 1)
        })
        .and_then(|i| u32::try_from(i + 1).ok())
}

/// Whether a spell matches the search `needle`: every word of the needle must be a level the
/// spell is (`viii`, `8`), or begin a word of its name or its school, ignoring case. An empty
/// needle matches every spell.
#[must_use]
pub fn search_matches(name: &str, level: u32, school: &str, needle: &str) -> bool {
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .chain(school.split_whitespace())
        .filter(|w| !w.is_empty() && level_word(w).is_none())
        .map(str::to_lowercase)
        .collect();
    needle
        .split_whitespace()
        .all(|word| match level_word(word) {
            Some(l) => l == level,
            None => {
                let word = word.to_lowercase();
                words.iter().any(|w| w.starts_with(&word))
            }
        })
}

/// At most this many spells arriving at once are spells learned and shown; more is the spellbook
/// being sent whole.
const LEARNED_AT_ONCE: usize = 3;

impl Windows {
    /// A spell learned (from a scroll or by spell research) opens the Spellbook on its school's
    /// tab with it chosen, its search cleared so it shows.
    pub(super) fn follow_learned_spells(&mut self, ctx: &Ctx<'_>, state: &GameState) {
        if !state.in_world || state.spells.is_empty() {
            if !state.in_world {
                self.known_spells = None;
            }
            return;
        }
        let ids: std::collections::BTreeSet<u32> = state.spells.iter().map(|s| s.id).collect();
        if let Some(known) = &self.known_spells {
            let learned: Vec<&crate::ui::game::Spell> = state
                .spells
                .iter()
                .filter(|s| !known.contains(&s.id))
                .collect();
            if let Some(spell) = learned.last().filter(|_| learned.len() <= LEARNED_AT_ONCE) {
                let school = SCHOOLS.iter().position(|(_, n)| *n == spell.school);
                if let Some(tab) =
                    school.and_then(|i| tabs(state).iter().position(|(_, t)| *t == Tab::School(i)))
                {
                    self.actions_tab = tab;
                    self.actions_selected = Some(spell.id);
                    self.spell_search.clear();
                    if !self.is_open(super::WindowId::Actions) {
                        self.open(super::WindowId::Actions, ctx.time);
                    }
                }
            }
        }
        self.known_spells = Some(ids);
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn actions(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let offered = tabs(state);
        let labels: Vec<&str> = offered.iter().map(|(n, _)| *n).collect();
        self.actions_tab = self.actions_tab.min(offered.len() - 1);
        kit::tabs(
            p,
            ctx,
            Rect::new(body.x, body.y, body.w, 26.0 * k),
            &labels,
            &mut self.actions_tab,
        );
        let tab = offered[self.actions_tab.min(offered.len() - 1)].1;
        if tab != Tab::Components {
            self.leave_components(state, out);
        }
        let school_tab = match tab {
            Tab::Components => {
                self.components(p, ctx, state, body, out);
                return;
            }
            Tab::Research => {
                self.research(p, ctx, state, body, out);
                return;
            }
            Tab::School(i) => i,
        };
        let (school_name, school) = SCHOOLS[school_tab];
        let mask = state.world_services.spell_filters;
        // The search box under the tabs.
        let search = Rect::new(
            body.x,
            body.y + 32.0 * k,
            body.w * LIST_SHARE - 8.0 * k,
            SEARCH_H * k,
        );
        kit::text_box(
            p,
            ctx,
            search,
            &mut self.spell_search,
            &mut self.typing_field,
            SEARCH_FIELD,
        );
        if self.spell_search.is_empty() && self.typing_field != SEARCH_FIELD {
            let hint =
                TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
            p.text_in(&hint, search.offset(6.0 * k, 0.0), Align::Left, "Search");
        }
        let needle = self.spell_search.clone();
        let mut spells: Vec<_> = state
            .spells
            .iter()
            .filter(|s| s.school == school)
            .filter(|s| dereth_client_contract::spellbook::accepts(mask, s.school, s.level))
            .filter(|s| search_matches(&s.name, s.level, school_name, &needle))
            .collect();
        spells.sort_by(|a, b| a.level.cmp(&b.level).then(a.name.cmp(&b.name)));
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let list_w = body.w * LIST_SHARE;
        let area = Rect::new(
            body.x,
            body.y + LIST_TOP * k,
            list_w,
            body.h - (LIST_TOP + 46.0) * k,
        );
        self.spell_filters(
            p,
            ctx,
            state,
            Rect::new(body.x, area.bottom() + 6.0 * k, list_w, 40.0 * k),
            out,
        );
        // Beside the level filters, under the details: the fighting stance's controls.
        stance_controls(
            p,
            ctx,
            state,
            Rect::new(
                area.right() + 12.0 * k,
                area.bottom() + 4.0 * k,
                body.right() - area.right() - 12.0 * k,
                40.0 * k,
            ),
            out,
            &mut self.tip,
        );
        let row_h = 40.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * spells.len() as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.actions_scroll);
        // While the list scrolls, the rows stop short of its bar.
        let bar = if content > area.h {
            kit::scrollbar_width(p).map_or(8.0 * k, |w| (w + 4.0) * k)
        } else {
            8.0 * k
        };
        p.list.push_clip(area);
        if spells.is_empty() {
            p.text(
                &dim,
                area.x + 10.0 * k,
                area.y + 4.0 * k,
                "No spells shown.",
            );
        }
        // The pad's focus starts on the first spell in sight, not the search box above.
        let mut homed = false;
        for (i, s) in spells.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let row = Rect::new(
                area.x,
                area.y + i as f32 * row_h - offset,
                area.w - bar,
                row_h - 2.0 * k,
            );
            if row.bottom() < area.y || row.y > area.bottom() {
                continue;
            }
            let visible = Rect::new(
                row.x,
                row.y.max(area.y),
                row.w,
                row.bottom().min(area.bottom()) - row.y.max(area.y),
            );
            if !homed && row.y >= area.y {
                ctx.input.nav.home(visible);
                homed = true;
            }
            let on = self.actions_selected == Some(s.id);
            if on || ctx.input.hover(&visible) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            let icon_r = Rect::new(row.x + 4.0 * k, row.y + 2.0 * k, 34.0 * k, 34.0 * k);
            if let Some(icon) = p
                .art
                .ac_spell(s.ac_icon, s.icon_power, s.bitfield)
                .or_else(|| p.art.ac_icon(s.ac_icon))
            {
                p.sprite(&icon, icon_r, WHITE);
            }
            // The name and the level as one block, centred on the icon.
            // Cut short to stop before the row's end.
            let room = row.right() - icon_r.right() - 14.0 * k;
            let shown = p.fit(&name, &s.name, room);
            p.text(&name, icon_r.right() + 10.0 * k, row.y + 0.5 * k, &shown);
            p.text(
                &dim,
                icon_r.right() + 10.0 * k,
                row.y + 17.5 * k,
                &format!("Level {}", s.level),
            );
            if ctx.input.double_clicked(&visible) {
                *ctx.drag = None;
                self.actions_selected = Some(s.id);
                out.requests.extend(self.add_to_bar(state, s.id));
            } else if ctx.input.right_clicked(&visible) {
                // A right-click identifies the spell, in a window of its own.
                self.ask(super::info::Ask::Spell(s.id), ctx.time);
            } else if ctx.over(&visible) && ctx.input.clicked(&visible) {
                self.actions_selected = Some(s.id);
                // With the pad, choosing a spell takes the focus on to Add to Bar and Forget.
                self.focus_spell_details = ctx.input.pad.mode.is_some();
                // Pressed, it can be dragged to a spell bar slot (with the mouse; the pad adds
                // it to a bar from its details).
                *ctx.drag = ctx.input.pad.mode.is_none().then_some(crate::ui::Drag {
                    item: dereth_primitives::ObjectId(0),
                    look: None,
                    spell_look: Some((s.icon_power, s.bitfield)),
                    from_shortcut: None,
                    icon: Some(s.ac_icon),
                    origin: ctx.input.mouse,
                    active: false,
                    on_click: None,
                    spell: Some(s.id),
                    from_spell_slot: None,
                    component: false,
                    stance: None,
                });
            }
        }
        p.list.pop_clip();
        // The chosen spell's details.
        out.examine_spell = self.actions_selected;
        let pane = Rect::new(
            area.right() + 12.0 * k,
            area.y,
            body.right() - area.right() - 12.0 * k,
            area.h,
        );
        kit::panel(p, pane, 0.5);
        let Some(id) = self.actions_selected else {
            p.text_in(&dim, pane, Align::Centre, "Choose a spell.");
            return;
        };
        let Some((_, d)) = state.spell_detail.as_ref().filter(|(sid, _)| *sid == id) else {
            return;
        };
        // The details keep clear of the pane's frame.
        let (left, top, right, bottom) = kit::panel_inset(p);
        let x = pane.x + left;
        let w = pane.w - left - right;
        let buttons_y = pane.bottom() - bottom - 28.0 * k;
        self.spell_details(
            p,
            ctx,
            state,
            (id, d),
            Rect::new(x, pane.y + top, w, buttons_y - pane.y - top),
            &[],
        );
        let bw = (w - 8.0 * k) / 2.0;
        let add = Rect::new(x, buttons_y, bw, 28.0 * k);
        if std::mem::take(&mut self.focus_spell_details) {
            ctx.input.nav.want_focus(add);
        }
        if kit::button(p, ctx, add, "Add to Bar", true) {
            // In gamepad mode the spell goes to a cross hotbar, through the bind-to-hotbar notice.
            if ctx.input.pad.mode.is_some() {
                out.bind_to_hotbar = Some(crate::ui::hud::cross::CrossBind::Spell(id));
            } else {
                out.requests.extend(self.add_to_bar(state, id));
            }
        }
        if kit::button(
            p,
            ctx,
            Rect::new(x + bw + 8.0 * k, buttons_y, bw, 28.0 * k),
            "Forget",
            true,
        ) {
            self.forget_asked = Some(id);
        }
    }

    /// A spell's details as the game's spell examination words them, in `r`: its icon and name,
    /// its school, mana, duration and range, then `extra` lines, its description, and the
    /// components it burns in a row of their icons at the foot of `r`.
    pub(super) fn spell_details(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &Ctx<'_>,
        state: &GameState,
        (id, d): (u32, &dereth_client_contract::view::SpellExamineView),
        r: Rect,
        extra: &[String],
    ) {
        let k = p.scale;
        let head = TextStyle::new(Family::Body, 15.0, 0xFFFF_FFFF).edge(ctx.colours.edge());
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let (x, w) = (r.x, r.w);
        let mut y = r.y;
        let known = state.spells.iter().find(|s| s.id == id);
        let icon = d.icon.and_then(|i| {
            known
                .and_then(|s| p.art.ac_spell(i.0, s.icon_power, s.bitfield))
                .or_else(|| p.art.ac_icon(i.0))
        });
        if let Some(icon) = icon {
            p.sprite(&icon, Rect::new(x, y, 40.0 * k, 40.0 * k), WHITE);
        }
        for (i, l) in p
            .wrap(&head, &d.name, w - 50.0 * k)
            .iter()
            .take(2)
            .enumerate()
        {
            #[allow(clippy::cast_precision_loss)]
            p.text(&head, x + 50.0 * k, y + i as f32 * 18.0 * k, l);
        }
        y += 48.0 * k;
        let mut lines = vec![
            words::school_name(d.school).to_owned(),
            words::mana_text(d.base_mana, d.mana_mod),
        ];
        lines.extend(words::duration_text(d.duration));
        lines.extend(words::range_text(d.range));
        lines.extend(extra.iter().cloned());
        for l in &lines {
            p.text(&dim, x, y, l);
            y += 17.0 * k;
        }
        y += 8.0 * k;
        let lh = p.line_height(&name);
        let foot = r.bottom();
        for l in p.wrap(&name, &d.description, w) {
            if y + lh > foot - 60.0 * k {
                break;
            }
            p.text(&name, x, y, &l);
            y += lh;
        }
        // The components the spell burns, as a row of their icons centred at the foot; a
        // component's name is its icon's tip. A component with no icon is left out, as the game's
        // own examination leaves it out.
        let size = 32.0 * k;
        let gap = 4.0 * k;
        let comps_y = foot - size - 8.0 * k;
        let icons: Vec<_> = d
            .components
            .iter()
            .flatten()
            .filter_map(|c| Some((c, c.icon.and_then(|i| p.art.ac_component(i.0))?)))
            .collect();
        let fits = ((w + gap) / (size + gap)).floor().max(0.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a count of icons that fit across the pane, small and not negative.
        let shown = icons.len().min(fits as usize);
        #[allow(clippy::cast_precision_loss)]
        let row_w = (shown as f32 * (size + gap) - gap).max(0.0);
        let mut cx = x + (w - row_w) / 2.0;
        for (c, icon) in icons.into_iter().take(shown) {
            let r = Rect::new(cx, comps_y, size, size);
            p.sprite(&icon, r, WHITE);
            if ctx.input.hover(&r) {
                self.tip = Some((c.name.clone(), vec!["Component".into()]));
            }
            cx += size + gap;
        }
    }

    /// The spellbook's level filter: each switch, box and label alike, flips its level's bit of
    /// the game's filter, which the server keeps and the list follows. A level the world does not
    /// have is not offered.
    fn spell_filters(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        r: Rect,
        out: &mut Outcome,
    ) {
        use dereth_client_contract::spellbook::level_mask;
        let k = p.scale;
        let w = &state.world_services;
        let mask = w.spell_filters;
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let per = (r.w / 4.0).max(1.0);
        for (i, label) in LEVELS.iter().enumerate() {
            let bit = level_mask(u32::try_from(i + 1).unwrap_or(1));
            if bit == 0 || (state.in_world && !w.era_ui.spell_filter(bit)) {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let row = Rect::new(
                r.x + (i % 4) as f32 * per,
                r.y + (i / 4) as f32 * 20.0 * k,
                per,
                20.0 * k,
            );
            let on = mask & bit != 0;
            let boxed = kit::checkbox(p, ctx, Rect::new(row.x, row.y, 20.0 * k, row.h), on);
            let label_r = Rect::new(row.x + 22.0 * k, row.y, row.w - 22.0 * k, row.h);
            // The level alone when the word will not fit beside it.
            let full = format!("Level {label}");
            let shown = if p.measure(&dim, &full) <= label_r.w - 4.0 * k {
                full
            } else {
                (*label).to_string()
            };
            p.text(&dim, label_r.x + 2.0 * k, row.y + 2.0 * k, &shown);
            // The label answers the pointer, but its box is the one stop for the pad.
            let labelled = ctx.over_quiet(&label_r) && ctx.input.clicked(&label_r);
            if boxed.is_some() || labelled {
                let mask = if on { mask & !bit } else { mask | bit };
                out.requests.push(UiRequest::SetSpellbookFilter { mask });
            }
        }
    }

    /// Adds a spell to the end of the open tab; one the tab already holds moves there.
    fn add_to_bar(&self, state: &GameState, spell_id: u32) -> Vec<UiRequest> {
        let tab = self.spell_tab;
        let rows = state.spell_tabs.get(tab).map_or(&[][..], Vec::as_slice);
        crate::ui::place_spell(rows, spell_id, rows.len(), tab)
    }

    /// Whether a window's question is up (forgetting a spell, abandoning a contract): it takes
    /// every press and key until it is answered, as the game's own questions do.
    #[must_use]
    pub fn questions_open(&self) -> bool {
        self.forget_asked.is_some() || self.abandon_asked.is_some()
    }

    /// The windows' questions, asked over everything else once the keys held back for them are
    /// theirs again.
    pub fn questions(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        self.forget_question(p, ctx, state, out);
        self.abandon_question(p, ctx, out);
    }

    /// The question before a spell is forgotten.
    pub(super) fn forget_question(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let Some(id) = self.forget_asked else {
            return;
        };
        let name = state
            .spells
            .iter()
            .find(|s| s.id == id)
            .map_or("this spell", |s| s.name.as_str());
        let text = format!("Forget {name}? You will have to learn it again to cast it.");
        if let Some(b) =
            crate::ui::pregame::dialog_box(p, ctx, "Forget Spell", &text, &["Yes", "No"], true, 0)
        {
            self.forget_asked = None;
            if b == 0 {
                out.requests.push(UiRequest::RemoveSpell { spell_id: id });
                self.actions_selected = None;
            }
        }
    }
}

/// The fighting stance's five controls in a row over `r`, each to be put on a cross hotbar slot:
/// confirming one starts the bind-to-hotbar notice, and the mouse drags it onto a slot. Each
/// names itself (for the weapon in hand) while the pointer is over it.
fn stance_controls(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &GameState,
    r: Rect,
    out: &mut Outcome,
    tip: &mut Option<(String, Vec<String>)>,
) {
    use crate::ui::hud::cross::{self, CrossBind, PowerAct};
    let k = p.scale;
    let missile = state.combat_mode == dereth_client_contract::combat_mode::MISSILE;
    let side = 36.0 * k;
    let gap = 8.0 * k;
    for (i, act) in PowerAct::ALL.into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let tile = Rect::new(
            r.x + i as f32 * (side + gap),
            r.y + (r.h - side) / 2.0,
            side,
            side,
        );
        if tile.right() > r.right() {
            break;
        }
        let bind = CrossBind::Power(act);
        let pressed = cross::draw_slot(p, ctx, tile, None, Some(bind), missile, false, false);
        if ctx.input.hover(&tile) {
            *tip = Some((act.name(missile).to_owned(), Vec::new()));
        }
        if !pressed {
            continue;
        }
        if ctx.input.pad.mode.is_some() {
            out.bind_to_hotbar = Some(bind);
        } else {
            // Pressed, it can be dragged onto a cross hotbar slot.
            *ctx.drag = Some(crate::ui::Drag {
                item: dereth_primitives::ObjectId(0),
                look: None,
                spell_look: None,
                from_shortcut: None,
                icon: None,
                origin: ctx.input.mouse,
                active: false,
                on_click: None,
                spell: None,
                from_spell_slot: None,
                component: false,
                stance: Some(act),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    fn world(research: bool, void: bool) -> GameState {
        let mut state = GameState {
            in_world: true,
            ..GameState::default()
        };
        state.world_services.features = dereth_primitives::EraFeatures::ALL;
        state.world_services.features.spell_research = research;
        state.world_services.era_ui.void_magic = void;
        state
    }

    #[test]
    fn the_search_takes_a_level_as_a_level_and_other_words_as_the_starts_of_words() {
        let m = |name: &str, level: u32, needle: &str| search_matches(name, level, "Life", needle);
        assert!(m("Strength Self VIII", 8, ""));
        assert!(m("Strength Self VIII", 8, "self viii"));
        assert!(m("Strength Self VIII", 8, "SELF 8"));
        assert!(
            !m("Strength Self I", 1, "self viii"),
            "a level-I spell is not level VIII"
        );
        assert!(!m("Strength Self VI", 6, "self vii"), "VI is not VII");
        assert!(m("Flame Bolt VI", 6, "fla bo"));
        assert!(m("Flame Bolt VI", 6, "life"), "its school");
        assert!(!m("Flame Bolt VI", 6, "frost"));
        assert!(
            !m("Flame Bolt VI", 6, "lame"),
            "words match from their starts"
        );
    }

    #[test]
    fn create_spell_is_offered_only_where_the_world_has_research_and_void_only_with_void_magic() {
        let names = |s: &GameState| tabs(s).into_iter().map(|(n, _)| n).collect::<Vec<_>>();
        let eor = names(&world(false, true));
        assert!(eor.contains(&"Void"));
        assert!(!eor.contains(&"Create Spell"));
        let early = names(&world(true, false));
        assert!(!early.contains(&"Void"));
        assert_eq!(
            early[early.len() - 2..],
            ["Components", "Create Spell"],
            "Create Spell after the schools and Components; no page of the stance's controls"
        );
    }
}
