//! The spell research page, as clients up to January 2002 had it: up to eight carried components
//! laid into a formula, and the formula tested on the selected target in magic mode. A formula
//! that makes a spell casts it, and teaches it if it is new; one that makes none is refused like any
//! cast. The page itself is told nothing of the outcome: a spell learned opens the spellbook on it.
//! The formula and the test's rules are the game's ([`dereth_client_contract::research`]); this
//! is the page.
use super::super::*;
use super::common::*;
use crate::int::{i32_from, u32_from};
use dereth_client_contract::view::ComponentRow;

use dereth_client_contract::research::{Formula, FORMULA_SLOTS};
/// The components grid: eight columns of 32-pixel slots, six rows showing.
const COLUMNS: usize = 8;
const ROWS_SHOWN: i32 = 6;
/// The page's top, under the magic window's tabs.
const TOP: i32 = 25;

#[derive(Debug, Default)]
pub struct SpellResearch {
    /// The formula: component class ids, in the order they were laid.
    formula: Formula,
    /// How far the components grid is scrolled, in rows.
    scroll: i32,
}

/// The carried components, in the component tracker's order: one slot per kind of component.
fn carried(game: &dyn GameView) -> Vec<ComponentRow> {
    game.spell_components()
        .into_iter()
        .flat_map(|c| c.rows)
        .filter(|r| r.owned > 0)
        .collect()
}

/// A component's slot: the carried object when there is one, else its icon alone.
fn slot(game: &dyn GameView, row: Option<&ComponentRow>) -> ItemEntry {
    match row {
        Some(r) => match r.object {
            Some(object) => super::items::entry(game, object),
            None => ItemEntry {
                icon: r.icon,
                ..ItemEntry::empty()
            },
        },
        None => ItemEntry::empty(),
    }
}

impl SpellResearch {
    fn add(&mut self, wcid: u32) {
        self.formula.add(wcid);
    }
}

impl Panel for SpellResearch {
    fn id(&self) -> &'static str {
        "spell-research"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let height = crate::panels::side_height();
        let mut f = PanelFrame::new(300, height);
        // The magic window's third tab. The page under the tabs has its 300x51 art tiled over it.
        image(
            &mut f,
            0x06001399,
            rect(0, TOP, 300, height as i32 - TOP),
            None,
            true,
            false,
        );
        super::magic::tabs(&mut f, 2, true);
        text(
            &mut f,
            rect(0, TOP + 5, 300, 20),
            "FORMULA",
            "16-7",
            CREAM,
            1,
            false,
            None,
        );
        let carried = carried(c.game);
        let laid: Vec<ItemEntry> = (0..FORMULA_SLOTS)
            .map(|i| {
                let wcid = self.formula.components().get(i).copied();
                let row = wcid.and_then(|w| carried.iter().find(|r| r.wcid == w));
                match (wcid, row) {
                    (Some(_), Some(row)) => slot(c.game, Some(row)),
                    // A component no longer carried keeps its place, drawn as an empty slot.
                    _ => ItemEntry::empty(),
                }
            })
            .collect();
        f.control(
            "formula",
            rect(22, TOP + 25, 256, 32),
            ControlKind::Items {
                entries: laid,
                columns: u32_from(FORMULA_SLOTS),
                slot_size: 32,
                selected: None,
            },
            true,
        );
        text(
            &mut f,
            rect(0, TOP + 65, 300, 20),
            "COMPONENTS",
            "16-7",
            CREAM,
            1,
            false,
            None,
        );
        let rows = i32_from(carried.len().div_ceil(COLUMNS));
        let max = (rows - ROWS_SHOWN).max(0);
        let first = self.scroll.clamp(0, max) as usize * COLUMNS;
        let shown: Vec<ItemEntry> = (first..first + COLUMNS * ROWS_SHOWN as usize)
            .map(|i| slot(c.game, carried.get(i)))
            .collect();
        f.control(
            "carried",
            rect(12, TOP + 85, 256, 192),
            ControlKind::Items {
                entries: shown,
                columns: u32_from(COLUMNS),
                slot_size: 32,
                selected: c.game.selected_object(),
            },
            true,
        );
        f.control(
            "carried-scroll",
            rect(268, TOP + 85, 20, 192),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: self.scroll.clamp(0, max),
                page: ROWS_SHOWN,
                step: 1,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        // Test and Clear wait for a formula. The layout gives their place and width; their
        // height is the theme button art's.
        let any = !self.formula.is_empty();
        f.button("test", rect(20, TOP + 290, 110, 36), "Test", any);
        f.button("clear", rect(170, TOP + 290, 110, 36), "Clear", any);
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        // A world without spell research has no research page: it gives way to the spellbook.
        if !super::magic::research_on(c.game) && matches!(e, ControlEvent::Tick) {
            return vec![PanelAction::Open("spellbook".into())];
        }
        let carried = carried(c.game);
        let at = |index: usize| {
            carried
                .get(self.scroll.max(0) as usize * COLUMNS + index)
                .cloned()
        };
        match e {
            ControlEvent::Activate(id) if id == "close" => return vec![PanelAction::Close],
            ControlEvent::Activate(id) if id == "spellbook" || id == "components" => {
                return vec![PanelAction::Open(id)];
            }
            ControlEvent::DragStart { id, index } if id == "carried" => {
                if let Some(object) = at(index).and_then(|r| r.object) {
                    return vec![PanelAction::BeginDrag(DragPayload::Object(object))];
                }
            }
            ControlEvent::Select { id, index } if id == "carried" => {
                if let Some(object) = at(index).and_then(|r| r.object) {
                    return vec![PanelAction::Game(UiRequest::Select(object))];
                }
            }
            // A component dragged onto the formula, or double-clicked, is laid at its end.
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(object),
                ..
            } if id == "formula" => {
                if let Some(wcid) = c.game.object_is_owned_component(object) {
                    self.add(wcid);
                }
            }
            ControlEvent::DoubleClick { id, index } if id == "carried" => {
                if let Some(row) = at(index) {
                    self.add(row.wcid);
                }
            }
            // A double click on a laid component takes it out again.
            ControlEvent::DoubleClick { id, index } if id == "formula" => {
                self.formula.remove(index);
            }
            ControlEvent::Scroll { id, value } if id == "carried-scroll" => {
                self.scroll = value.max(0);
            }
            ControlEvent::Activate(id) if id == "clear" => self.formula.clear(),
            ControlEvent::Activate(id) if id == "test" && !self.formula.is_empty() => {
                return vec![PanelAction::Host(HostAction::TestSpellFormula {
                    components: self.formula.components().to_vec(),
                })];
            }
            _ => {}
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    use dereth_client_contract::view::ComponentCategory;

    #[derive(Debug, Default)]
    struct Game {
        mode: u32,
        selected: Option<ObjectId>,
        era: Option<dereth_client_contract::EraView>,
    }
    impl GameView for Game {
        fn era(&self) -> Option<&dereth_client_contract::EraView> {
            self.era.as_ref()
        }
        fn combat_mode(&self) -> u32 {
            self.mode
        }
        fn selected_object(&self) -> Option<ObjectId> {
            self.selected
        }
        fn spell_components(&self) -> Vec<ComponentCategory> {
            vec![ComponentCategory {
                category: 0,
                rows: [(0x2b1, 10), (0x2b2, 0), (0x2b3, 4)]
                    .into_iter()
                    .map(|(wcid, owned)| ComponentRow {
                        wcid,
                        owned,
                        object: Some(ObjectId(wcid)),
                        ..Default::default()
                    })
                    .collect(),
            }]
        }
        fn object_is_owned_component(&self, obj: ObjectId) -> Option<u32> {
            Some(obj.0)
        }
    }
    fn with(game: &Game, run: impl FnOnce(&Context<'_>)) {
        let (state, pregame, keyboard, settings) = Default::default();
        run(&Context {
            now: dereth_primitives::LocalTime(0.0),
            game,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &state,
        });
    }

    #[test]
    fn components_laid_into_the_formula_are_sent_as_the_test() {
        let mut page = SpellResearch::default();
        let game = Game {
            mode: 8,
            selected: Some(ObjectId(0x5000_0001)),
            era: None,
        };
        with(&game, |c| {
            // Only carried components show: the second (none carried) is left out.
            page.event(
                ControlEvent::DoubleClick {
                    id: "carried".into(),
                    index: 1,
                },
                c,
            );
            page.event(
                ControlEvent::Drop {
                    id: "formula".into(),
                    payload: DragPayload::Object(ObjectId(0x2b1)),
                    slot: 0,
                },
                c,
            );
            assert_eq!(page.formula.components(), [0x2b3, 0x2b1]);
            let sent = page.event(ControlEvent::Activate("test".into()), c);
            assert_eq!(
                sent,
                [PanelAction::Host(HostAction::TestSpellFormula {
                    components: vec![0x2b3, 0x2b1],
                })]
            );
            // A double click on a laid component takes it out; Clear empties the formula.
            page.event(
                ControlEvent::DoubleClick {
                    id: "formula".into(),
                    index: 0,
                },
                c,
            );
            assert_eq!(page.formula.components(), [0x2b1]);
            page.event(ControlEvent::Activate("clear".into()), c);
            assert!(page.formula.is_empty());
        });
    }

    #[test]
    fn a_formula_holds_eight_components() {
        let mut page = SpellResearch::default();
        for _ in 0..10 {
            page.add(0x2b1);
        }
        assert_eq!(page.formula.components().len(), FORMULA_SLOTS);
    }

    /// A world announcing spell research has the research page and its tab; one without gives
    /// the page up for the spellbook, and the spellbook draws its two tabs.
    #[test]
    fn the_research_page_and_its_tab_follow_the_worlds_spell_research() {
        let mut era = dereth_client_contract::EraView::default();
        era.announced_features.set("spell_research", true);
        let researching = Game {
            era: Some(era),
            ..Game::default()
        };
        let plain = Game::default();
        let mut page = SpellResearch::default();
        with(&researching, |c| {
            assert!(super::super::magic::research_on(c.game));
            assert!(page.event(ControlEvent::Tick, c).is_empty());
            let frame = page.frame(c);
            assert!(frame.controls.iter().any(|b| b.id == "create-spell"));
        });
        with(&plain, |c| {
            assert!(!super::super::magic::research_on(c.game));
            assert_eq!(
                page.event(ControlEvent::Tick, c),
                [PanelAction::Open("spellbook".into())]
            );
            let book = super::super::magic::Spellbook::new(false).frame(c);
            assert!(!book.controls.iter().any(|b| b.id == "create-spell"));
        });
    }
}
