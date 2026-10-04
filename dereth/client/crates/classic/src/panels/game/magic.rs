//! The spellbook and components panel, the active effects panel and the vitae panel.
use super::super::*;
use super::common::*;
use crate::int::{i32_from, u32_from};
use dereth_client_contract::spellbook::{level_mask, school_mask};
use dereth_client_contract::view::{ComponentRow, SpellEntry};

const FILTERS: [(&str, u32, i32, i32); 13] = [
    ("Creature", school_mask(4), 10, 273),
    ("Item", school_mask(3), 84, 273),
    ("Life", school_mask(2), 138, 273),
    ("War", school_mask(1), 190, 273),
    ("Void", school_mask(5), 242, 273),
    ("I", level_mask(1), 10, 309),
    ("II", level_mask(2), 80, 309),
    ("III", level_mask(3), 150, 309),
    ("IV", level_mask(4), 10, 326),
    ("V", level_mask(5), 80, 326),
    ("VI", level_mask(6), 150, 326),
    ("VII", level_mask(7), 10, 343),
    ("VIII", level_mask(8), 80, 343),
];

pub fn visible_spell(s: &SpellEntry, mask: u32) -> bool {
    dereth_client_contract::spellbook::accepts(mask, s.school, s.level)
}
fn spells(game: &dyn GameView) -> Vec<&SpellEntry> {
    let mut v: Vec<_> = game
        .spellbook()
        .iter()
        .filter(|s| visible_spell(s, game.spell_filters()))
        .collect();
    v.sort_by_key(|s| s.display_order);
    v
}
/// Whether the world has spell research: the Create Spell page is shown only where it does.
pub fn research_on(game: &dyn GameView) -> bool {
    game.era_features().spell_research
}

/// The magic window's tabs and the close button. `page` is the shown one: 0 the spellbook, 1 the
/// components, 2 the research page. A world with spell research has the three 92-pixel tabs the
/// clients with the research page had; one without has the two 138-pixel tabs of the early-2005
/// window.
pub(super) fn tabs(f: &mut PanelFrame, page: usize, research: bool) {
    let all = [
        (0, "spellbook", "Spellbook"),
        (92, "components", "Components"),
        (184, "create-spell", "Create Spell"),
    ];
    let width = if research { 92 } else { 138 };
    for (i, (x, id, label)) in all.into_iter().enumerate() {
        if i == 2 && !research {
            continue;
        }
        let x = if research {
            x
        } else {
            i32::try_from(i).unwrap_or(0) * 138
        };
        let selected = i == page;
        art(
            f.button(id, rect(x, 0, width, 25), label, true),
            if selected { 0x06000f76 } else { 0x06000f77 },
            0x06000f76,
            0x06000f77,
        );
    }
    close(f, 0x06001393, 0x06001394);
}
fn row(
    f: &mut PanelFrame,
    y: i32,
    name: &str,
    icon: Option<DataId>,
    selected: bool,
    clip: [i32; 4],
) {
    image(
        f,
        if selected { 0x06001397 } else { 0x06001396 },
        rect(0, y, 300, 32),
        Some(clip),
        false,
        false,
    );
    if let Some(icon) = icon {
        component_icon(f, icon.0, rect(0, y, 32, 32), Some(clip));
    }
    text(
        f,
        rect(42, y, 188, 32),
        name,
        "16-7",
        CREAM,
        0,
        true,
        Some(clip),
    );
}
#[derive(Debug)]
pub struct Spellbook {
    height: Option<u32>,
    edits: std::collections::BTreeMap<u32, String>,
    components: bool,
    selected: Option<u32>,
    scroll: i32,
    /// The selected spell is to be scrolled into view on the next tick.
    reveal: bool,
}
impl Spellbook {
    pub fn new(components: bool) -> Self {
        Self {
            height: None,
            components,
            edits: Default::default(),
            selected: None,
            scroll: 0,
            reveal: false,
        }
    }
}
fn component_rows(game: &dyn GameView) -> Vec<(Option<&'static str>, Option<ComponentRow>)> {
    let categories = game.spell_components();
    let mut result = vec![];
    for (index, title) in [
        "SCARABS",
        "HERBS",
        "POWDERED GEMS",
        "ALCHEMICAL SUBSTANCES",
        "TALISMANS",
        "TAPERS",
        "PEAS",
    ]
    .iter()
    .enumerate()
    {
        if let Some(c) = categories
            .iter()
            .find(|c| c.category == u32_from(index))
            .filter(|c| !c.rows.is_empty())
        {
            result.push((Some(*title), None));
            result.extend(c.rows.iter().cloned().map(|r| (None, Some(r))));
        }
    }
    result
}
impl Panel for Spellbook {
    fn resize(&mut self, _width: u32, height: u32) {
        self.height = Some(height.max(362));
    }
    fn id(&self) -> &'static str {
        if self.components {
            "components"
        } else {
            "spellbook"
        }
    }
    fn frame(&self, ctx: &Context<'_>) -> PanelFrame {
        let height = self.height.unwrap_or_else(crate::panels::side_height);
        let mut f = PanelFrame::new(300, height);
        // Stretched, the lists grow and the spellbook's filters keep to the bottom.
        let dy = height as i32 - 362;
        image(
            &mut f,
            0x06001398,
            rect(0, 25, 300, height as i32 - 25),
            None,
            true,
            false,
        );
        tabs(&mut f, usize::from(self.components), research_on(ctx.game));
        if self.components {
            let rows = component_rows(ctx.game);
            let offset = self
                .scroll
                .clamp(0, (i32_from(rows.len()) * 32 - (337 + dy)).max(0));
            let clip = [0, 25, 280, 362 + dy];
            for (i, (heading, entry)) in rows.iter().enumerate() {
                let y = 25 + i32_from(i) * 32 - offset;
                if y + 32 <= 25 || y >= 362 + dy {
                    continue;
                }
                if let Some(h) = heading {
                    image(
                        &mut f,
                        0x06001392,
                        rect(0, y, 300, 32),
                        Some(clip),
                        false,
                        false,
                    );
                    text(
                        &mut f,
                        rect(5, y + 9, 275, 23),
                        *h,
                        "16-7",
                        CREAM,
                        0,
                        false,
                        Some(clip),
                    );
                }
                if let Some(r) = entry {
                    row(
                        &mut f,
                        y,
                        &r.name,
                        r.icon,
                        r.object.is_some() && r.object == ctx.game.selected_object(),
                        clip,
                    );
                    text(
                        &mut f,
                        rect(237, y, 41, 16),
                        number(r.owned),
                        "15-6",
                        CREAM,
                        2,
                        false,
                        Some(clip),
                    );
                    if y >= 25 && y + 32 <= 362 + dy {
                        let c = f.edit(
                            format!("desired:{}", r.wcid),
                            rect(235, y + 16, 45, 16),
                            self.edits
                                .get(&r.wcid)
                                .cloned()
                                .unwrap_or_else(|| r.desired.to_string()),
                            4,
                            false,
                            true,
                        );
                        c.font = "15-6".into();
                    }
                }
            }
            scrollbar(
                &mut f,
                "rows-scroll",
                rect(280, 25, 20, 337 + dy),
                i32_from(rows.len()) * 32,
                337 + dy,
                offset,
                32,
                true,
            );
            list_hits(
                &mut f,
                "rows",
                rect(0, 25, 280, 337 + dy),
                rows.len(),
                32,
                None,
                offset,
            );
        } else {
            let rows = spells(ctx.game);
            let facts = ctx.game.era_ui();
            let offset = self
                .scroll
                .clamp(0, (i32_from(rows.len()) * 32 - (224 + dy)).max(0));
            let clip = [0, 25, 280, 249 + dy];
            for (i, s) in rows.iter().enumerate() {
                let y = 25 + i32_from(i) * 32 - offset;
                if y + 32 > 25 && y < 249 + dy {
                    row(&mut f, y, &s.name, None, self.selected == Some(s.id), clip);
                    spell_icon(
                        &mut f,
                        s.icon,
                        s.icon_power,
                        s.bitfield,
                        rect(0, y, 32, 32),
                        Some(clip),
                    );
                }
            }
            scrollbar(
                &mut f,
                "rows-scroll",
                rect(280, 25, 20, 224 + dy),
                i32_from(rows.len()) * 32,
                224 + dy,
                offset,
                32,
                true,
            );
            list_hits(
                &mut f,
                "rows",
                rect(0, 25, 280, 224 + dy),
                rows.len(),
                32,
                rows.iter().position(|s| Some(s.id) == self.selected),
                offset,
            );
            image(
                &mut f,
                0x06002722,
                rect(0, 249 + dy, 300, 113),
                None,
                true,
                false,
            );
            text(
                &mut f,
                rect(10, 257 + dy, 90, 20),
                "Schools",
                "16-7",
                CREAM,
                0,
                false,
                None,
            );
            text(
                &mut f,
                rect(10, 292 + dy, 90, 17),
                "Levels",
                "16-7",
                CREAM,
                0,
                false,
                None,
            );
            f.button(
                "delete",
                rect(202, 309 + dy, 88, 30),
                "Delete",
                self.selected.is_some_and(|id| ctx.game.is_spell_known(id)),
            );
            for (label, bit, x, y) in FILTERS.iter().filter(|r| facts.spell_filter(r.1)) {
                f.check(
                    format!("filter:{bit}"),
                    rect(*x, y + dy, if *bit == school_mask(4) { 74 } else { 52 }, 17),
                    *label,
                    ctx.game.spell_filters() & bit != 0,
                    true,
                );
            }
        }
        f
    }
    /// A spell just learned: the book shows it, selected and scrolled into view.
    fn set_spell(&mut self, spell: u32) {
        self.components = false;
        self.selected = Some(spell);
        self.reveal = true;
    }
    fn event(&mut self, e: ControlEvent, ctx: &Context<'_>) -> Vec<PanelAction> {
        if self.reveal && matches!(e, ControlEvent::Tick) {
            self.reveal = false;
            if let Some(i) = spells(ctx.game)
                .iter()
                .position(|s| Some(s.id) == self.selected)
            {
                self.scroll = i32_from(i) * 32;
            }
        }
        match e {
            ControlEvent::Activate(id) if id == "close" => return vec![PanelAction::Close],
            ControlEvent::Activate(id) if id == "components" || id == "spellbook" => {
                self.components = id == "components";
                self.scroll = 0;
            }
            ControlEvent::Activate(id) if id == "create-spell" && research_on(ctx.game) => {
                return vec![PanelAction::Open("spell-research".into())];
            }
            ControlEvent::DragStart { id, index } if id == "rows" && !self.components => {
                if let Some(s) = spells(ctx.game).get(index) {
                    return vec![PanelAction::BeginDrag(DragPayload::Spell(s.id))];
                }
            }
            ControlEvent::Scroll { id, value } if id == "rows" || id == "rows-scroll" => {
                self.scroll = value.max(0)
            }
            ControlEvent::Check { id, checked } if id.starts_with("filter:") => {
                if let Ok(bit) = id[7..].parse::<u32>() {
                    if ctx.game.era_ui().spell_filter(bit) && FILTERS.iter().any(|r| r.1 == bit) {
                        let mask = if checked {
                            ctx.game.spell_filters() | bit
                        } else {
                            ctx.game.spell_filters() & !bit
                        };
                        return vec![PanelAction::Game(UiRequest::SetSpellbookFilter { mask })];
                    }
                }
            }
            ControlEvent::Edit { id, text } if id.starts_with("desired:") => {
                if let Ok(wcid) = id[8..].parse::<u32>() {
                    self.edits.insert(wcid, text);
                }
            }
            ControlEvent::Commit { id } if id.starts_with("desired:") => {
                if let Ok(wcid) = id[8..].parse::<u32>() {
                    if let Some(level) = self
                        .edits
                        .remove(&wcid)
                        .and_then(|s| s.parse::<i32>().ok())
                        .filter(|v| (0..5001).contains(v))
                    {
                        if ctx
                            .game
                            .spell_components()
                            .iter()
                            .any(|c| c.rows.iter().any(|r| r.wcid == wcid))
                        {
                            return vec![PanelAction::Game(UiRequest::SetDesiredComponentLevel {
                                wcid,
                                level,
                            })];
                        }
                    }
                }
            }
            ControlEvent::Select { id, index } if id == "rows" => {
                if self.components {
                    if let Some((_, r)) = component_rows(ctx.game).get(index) {
                        return vec![PanelAction::Game(UiRequest::Select(
                            r.as_ref().and_then(|r| r.object).unwrap_or(ObjectId(0)),
                        ))];
                    }
                } else if let Some(s) = spells(ctx.game).get(index) {
                    // A click selects the spell; the examination is the right button's.
                    self.selected = Some(s.id);
                }
            }
            ControlEvent::RightClick { id, index } if id == "rows" && !self.components => {
                if let Some(s) = spells(ctx.game).get(index) {
                    self.selected = Some(s.id);
                    return vec![PanelAction::OpenSpell {
                        id: "examine-spell".into(),
                        spell: s.id,
                    }];
                }
            }
            ControlEvent::Activate(id) if id == "delete" && !self.components => {
                if let Some(s) = self.selected.and_then(|id| ctx.game.spell(id)) {
                    return vec![PanelAction::Confirm {
                        id: "delete-spell".into(),
                        text: format!("\n\n\nAre you sure you want to delete {}?", s.name),
                        accept: vec![PanelAction::Game(UiRequest::RemoveSpell { spell_id: s.id })],
                    }];
                }
            }
            _ => {}
        }
        vec![]
    }
}
#[derive(Debug)]
pub struct Effects {
    description_scroll: i32,
    beneficial: bool,
    selected: Option<u32>,
    scroll: i32,
}
impl Effects {
    pub fn new(beneficial: bool) -> Self {
        Self {
            description_scroll: 0,
            beneficial,
            selected: None,
            scroll: 0,
        }
    }
}
impl Panel for Effects {
    fn id(&self) -> &'static str {
        if self.beneficial {
            "beneficial-effects"
        } else {
            "harmful-effects"
        }
    }
    fn frame(&self, ctx: &Context<'_>) -> PanelFrame {
        let height = crate::panels::side_height();
        let mut f = PanelFrame::new(300, height);
        // Stretched, the list grows and the description keeps to the bottom.
        let dy = height as i32 - 362;
        image(
            &mut f,
            0x06001398,
            rect(0, 0, 300, height as i32),
            None,
            true,
            false,
        );
        image(&mut f, 0x0600127b, rect(0, 0, 276, 25), None, true, false);
        close(&mut f, 0x06001393, 0x06001394);
        text(
            &mut f,
            rect(2, 2, 272, 21),
            if self.beneficial {
                "Beneficial Spells in Effect"
            } else {
                "Harmful Spells in Effect"
            },
            "16-7",
            CREAM,
            1,
            false,
            None,
        );
        let mut rows: Vec<_> = ctx
            .game
            .active_effects()
            .into_iter()
            .filter(|e| e.beneficial == self.beneficial)
            .collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        let offset = self
            .scroll
            .clamp(0, (i32_from(rows.len()) * 32 - (256 + dy)).max(0));
        let clip = [0, 25, 280, 281 + dy];
        for (i, e) in rows.iter().enumerate() {
            let y = 25 + i32_from(i) * 32 - offset;
            if y + 32 > 25 && y < 281 + dy {
                row(
                    &mut f,
                    y,
                    &e.name,
                    None,
                    self.selected == Some(e.spell),
                    clip,
                );
                if let Some(s) = ctx.game.spell(e.spell) {
                    spell_icon(
                        &mut f,
                        e.icon,
                        s.icon_power,
                        s.bitfield,
                        rect(0, y, 32, 32),
                        Some(clip),
                    );
                }
                // The time left is shown only with the "show spell durations" character option.
                let timer = if ctx.classic.option_words[0] & 0x80_0000 != 0 {
                    effect_timer(e.remaining, e.permanent)
                } else {
                    String::new()
                };
                text(
                    &mut f,
                    rect(232, y + 6, 46, 20),
                    timer,
                    "16-7",
                    CREAM,
                    2,
                    false,
                    Some(clip),
                );
            }
        }
        scrollbar(
            &mut f,
            "rows-scroll",
            rect(280, 25, 20, 256 + dy),
            i32_from(rows.len()) * 32,
            256 + dy,
            offset,
            32,
            true,
        );
        list_hits(
            &mut f,
            "rows",
            rect(0, 25, 280, 256 + dy),
            rows.len(),
            32,
            rows.iter().position(|e| Some(e.spell) == self.selected),
            offset,
        );
        image(
            &mut f,
            0x06001395,
            rect(0, 281 + dy, 300, 81),
            None,
            false,
            false,
        );
        if let Some(e) = rows.iter().find(|e| Some(e.spell) == self.selected) {
            rich_scroll(
                &mut f,
                rect(5, 291 + dy, 275, 71),
                vec![crate::TextRun {
                    text: format!("{}\n{}", e.name, e.description),
                    color: CREAM,
                }],
                "16-6",
                self.description_scroll,
            );
        }
        f
    }
    fn event(&mut self, e: ControlEvent, ctx: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Activate(id) if id == "close" => return vec![PanelAction::Close],
            ControlEvent::Scroll { id, value } if id == "rows" || id == "rows-scroll" => {
                self.scroll = value.max(0)
            }
            ControlEvent::Scroll { id, value } if id == "description" => {
                self.description_scroll = value.max(0)
            }
            ControlEvent::Select { id, index } if id == "rows" => {
                let mut rows: Vec<_> = ctx
                    .game
                    .active_effects()
                    .into_iter()
                    .filter(|e| e.beneficial == self.beneficial)
                    .collect();
                rows.sort_by(|a, b| a.name.cmp(&b.name));
                self.selected = rows.get(index).map(|e| e.spell);
                self.description_scroll = 0;
            }
            _ => {}
        }
        vec![]
    }
}
#[derive(Debug, Default)]
pub struct Vitae;
impl Panel for Vitae {
    fn id(&self) -> &'static str {
        "vitae"
    }
    fn frame(&self, ctx: &Context<'_>) -> PanelFrame {
        let height = crate::panels::side_height();
        let mut f = PanelFrame::new(300, height);
        image(
            &mut f,
            0x06001398,
            rect(0, 0, 300, height as i32),
            None,
            true,
            false,
        );
        image(&mut f, 0x0600127b, rect(0, 0, 276, 25), None, true, false);
        close(&mut f, 0x06001393, 0x06001394);
        text(
            &mut f,
            rect(2, 2, 272, 21),
            "Vitae",
            "16-7",
            CREAM,
            1,
            false,
            None,
        );
        if let Some(v) = ctx
            .game
            .vitae_display()
            .map(|v| {
                dereth_presentation::stats::vitae_content(
                    v,
                    dereth_presentation::DisplayVariant::Classic,
                )
            })
            .filter(|v| v.penalty > 0)
        {
            text(
                &mut f,
                rect(15, 35, 275, 327),
                v.classic_text(),
                "16-7",
                CREAM,
                0,
                true,
                None,
            );
        }

        f
    }
    fn event(&mut self, e: ControlEvent, ctx: &Context<'_>) -> Vec<PanelAction> {
        if matches!(e,ControlEvent::Activate(ref id) if id=="close")
            || matches!(e, ControlEvent::Tick)
                && ctx.game.vitae_display().is_none_or(|v| {
                    dereth_presentation::stats::vitae_content(
                        v,
                        dereth_presentation::DisplayVariant::Classic,
                    )
                    .penalty
                        <= 0
                })
        {
            vec![PanelAction::Close]
        } else {
            vec![]
        }
    }
}

// An effect's time left, quantized to hundredths of a minute before the remainder becomes seconds.
pub(super) fn effect_timer(remaining: f64, permanent: bool) -> String {
    if permanent || remaining < 0. {
        return String::new();
    }
    let centiminutes = dereth_primitives::num::to_i64_f64(remaining * (1. / 60.) * 100.);
    let seconds = dereth_primitives::num::to_i64_f64((centiminutes % 100) as f64 * 0.01 * 60.);
    format!("{}:{:02}", centiminutes / 100, seconds)
}

#[cfg(test)]
mod filter_layout_tests {
    //! Behaviour: none (spellbook control placement; shared era facts own availability).
    use super::*;
    #[derive(Debug)]
    struct World(dereth_client_contract::EraView);
    impl GameView for World {
        fn era(&self) -> Option<&dereth_client_contract::EraView> {
            Some(&self.0)
        }
    }
    /// Behaviour: classic.spellbook.filters-fit-supported-world
    #[test]
    fn schools_share_one_row_and_all_available_levels_fit_below_without_overlap() {
        for old in [false, true] {
            let mut era = dereth_client_contract::EraView::default();
            if old {
                era.era = dereth_primitives::era::EraId::Infiltration;
            }
            era.era_announced = true;
            let game = World(era);
            let ctx = Context {
                game: &game,
                pregame: &Default::default(),
                keyboard: &Default::default(),
                settings: &Default::default(),
                classic: &Default::default(),
                map_teleport_allowed: false,
            };
            for height in [362, 618] {
                let mut panel = Spellbook::new(false);
                panel.resize(300, height);
                let frame = panel.frame(&ctx);
                let filters: Vec<_> = frame
                    .controls
                    .iter()
                    .filter(|c| c.id.starts_with("filter:"))
                    .collect();
                assert_eq!(filters.len(), if old { 11 } else { 13 });
                for (i, control) in filters.iter().enumerate() {
                    assert!(control.rect.x >= 0 && control.rect.x + control.rect.w <= 300);
                    assert!(control.rect.y + control.rect.h <= height as i32);
                    for other in filters.iter().skip(i + 1) {
                        let (a, b) = (control.rect, other.rect);
                        assert!(
                            a.x + a.w <= b.x
                                || b.x + b.w <= a.x
                                || a.y + a.h <= b.y
                                || b.y + b.h <= a.y
                        );
                    }
                }
                let school_y: Vec<_> = filters
                    .iter()
                    .filter(|c| c.id[7..].parse::<u32>().unwrap() & 0x200f != 0)
                    .map(|c| c.rect.y)
                    .collect();
                assert!(school_y.iter().all(|y| *y == school_y[0]));
                let delete = frame.controls.iter().find(|c| c.id == "delete").unwrap();
                assert!(filters.iter().all(|c| c.rect.x + c.rect.w <= delete.rect.x
                    || c.rect.y + c.rect.h <= delete.rect.y));
            }
        }
    }
}
