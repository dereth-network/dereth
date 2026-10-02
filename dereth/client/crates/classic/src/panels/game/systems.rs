//! The classic interface's pages for the systems the game gained after its 2005 redesign: the
//! character's titles and the contract tracker. Each is drawn in the classic side pages' style
//! (their background, title bar and rows) and shows only what the world's era has: a world without
//! titles or contracts shows a page that says so.
use super::super::*;
use super::common::*;
use crate::int::i32_from;

/// The page's list of rows: 32 pixels each, under the title bar.
const ROW: i32 = 32;
const TOP: i32 = 25;

/// A side page's frame: background, title bar, title and close button.
fn page(title: &str) -> (PanelFrame, i32) {
    let height = crate::panels::side_height();
    let mut f = PanelFrame::new(300, height);
    let h = height as i32;
    image(&mut f, 0x06001398, rect(0, 0, 300, h), None, true, false);
    image(&mut f, 0x0600127b, rect(0, 0, 276, 25), None, true, false);
    close(&mut f, 0x06001393, 0x06001394);
    text(
        &mut f,
        rect(2, 2, 272, 21),
        title,
        "16-7",
        CREAM,
        1,
        false,
        None,
    );
    (f, h)
}

/// One list row: the classic row background, highlighted when selected, and its text.
fn list_row(f: &mut PanelFrame, y: i32, name: &str, detail: &str, selected: bool, clip: [i32; 4]) {
    image(
        f,
        if selected { 0x06001397 } else { 0x06001396 },
        rect(0, y, 300, ROW),
        Some(clip),
        false,
        false,
    );
    text(
        f,
        rect(8, y, 200, ROW),
        name,
        "16-7",
        CREAM,
        0,
        true,
        Some(clip),
    );
    text(
        f,
        rect(204, y, 74, ROW),
        detail,
        "15-6",
        CREAM,
        2,
        false,
        Some(clip),
    );
}

/// The character's titles: the earned ones in name order, the displayed one marked, and a button
/// that makes the selected one the title shown under the character's name.
#[derive(Debug, Default)]
pub struct Titles {
    selected: Option<u32>,
    scroll: i32,
}

fn sorted_titles(game: &dyn GameView) -> (u32, Vec<(u32, String)>) {
    let t = game.character_titles();
    let mut titles = t.titles;
    titles.sort_by_key(|t| t.1.to_lowercase());
    (t.display, titles)
}

fn era_has(game: &dyn GameView, which: fn(&dereth_primitives::era::EraFeatures) -> bool) -> bool {
    game.era().is_none_or(|e| which(&e.features()))
}

impl Panel for Titles {
    fn id(&self) -> &'static str {
        "titles"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let (mut f, h) = page("Titles");
        if !era_has(c.game, |e| e.titles) {
            text(
                &mut f,
                rect(10, TOP + 10, 280, 40),
                "This world has no titles.",
                "16-7",
                CREAM,
                0,
                true,
                None,
            );
            return f;
        }
        let (display, titles) = sorted_titles(c.game);
        let clip = [0, TOP, 280, h - 44];
        let max = (i32_from(titles.len()) * ROW - (h - 44 - TOP)).max(0);
        let scroll = self.scroll.clamp(0, max);
        for (i, (id, name)) in titles.iter().enumerate() {
            let y = TOP + i32_from(i) * ROW - scroll;
            if y + ROW <= TOP || y >= h - 44 {
                continue;
            }
            let shown = if *id == display { "shown" } else { "" };
            list_row(&mut f, y, name, shown, self.selected == Some(*id), clip);
        }
        f.control(
            "rows",
            rect(0, TOP, 280, h - 44 - TOP),
            ControlKind::HitList {
                row_count: titles.len(),
                row_height: ROW,
                selected: titles.iter().position(|(id, _)| Some(*id) == self.selected),
                offset: scroll,
            },
            true,
        );
        f.control(
            "scroll",
            rect(280, TOP, 20, h - 44 - TOP),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: scroll,
                page: h - 44 - TOP,
                step: ROW,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        let can_set = self.selected.is_some_and(|s| s != display);
        f.button("set", rect(80, h - 40, 140, 36), "Set as Title", can_set);
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Activate(id) if id == "close" => return vec![PanelAction::Close],
            ControlEvent::Select { id, index } if id == "rows" => {
                let (_, titles) = sorted_titles(c.game);
                if let Some((title, _)) = titles.get(index) {
                    self.selected = Some(*title);
                }
            }
            ControlEvent::Scroll { id, value } if id == "scroll" => self.scroll = value.max(0),
            ControlEvent::Activate(id) if id == "set" => {
                if let Some(title_id) = self.selected {
                    return vec![PanelAction::Game(UiRequest::SetDisplayCharacterTitle {
                        title_id,
                    })];
                }
            }
            _ => {}
        }
        vec![]
    }
}

/// The contract tracker: the character's contracts with their progress, the selected one's notes
/// and contact, and a button that abandons it.
#[derive(Debug, Default)]
pub struct Contracts {
    selected: Option<u32>,
    scroll: i32,
}

impl Panel for Contracts {
    fn id(&self) -> &'static str {
        "contracts"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let (mut f, h) = page("Contracts");
        if !era_has(c.game, |e| e.contracts) {
            text(
                &mut f,
                rect(10, TOP + 10, 280, 40),
                "This world has no contracts.",
                "16-7",
                CREAM,
                0,
                true,
                None,
            );
            return f;
        }
        let contracts = c.game.contracts();
        // The list takes the top half; the selected contract's notes the rest.
        let list_bottom = TOP + (h - TOP - 44) / 2;
        let clip = [0, TOP, 280, list_bottom];
        let max = (i32_from(contracts.len()) * ROW - (list_bottom - TOP)).max(0);
        let scroll = self.scroll.clamp(0, max);
        for (i, contract) in contracts.iter().enumerate() {
            let y = TOP + i32_from(i) * ROW - scroll;
            if y + ROW <= TOP || y >= list_bottom {
                continue;
            }
            list_row(
                &mut f,
                y,
                &contract.name,
                &contract.status,
                self.selected == Some(contract.contract_id),
                clip,
            );
        }
        f.control(
            "rows",
            rect(0, TOP, 280, list_bottom - TOP),
            ControlKind::HitList {
                row_count: contracts.len(),
                row_height: ROW,
                selected: contracts
                    .iter()
                    .position(|k| Some(k.contract_id) == self.selected),
                offset: scroll,
            },
            true,
        );
        f.control(
            "scroll",
            rect(280, TOP, 20, list_bottom - TOP),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: scroll,
                page: list_bottom - TOP,
                step: ROW,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        if let Some(contract) = contracts
            .iter()
            .find(|k| Some(k.contract_id) == self.selected)
        {
            let notes = if contract.contact.is_empty() {
                contract.description.clone()
            } else {
                format!("{}\n\nContact: {}", contract.description, contract.contact)
            };
            text(
                &mut f,
                rect(10, list_bottom + 6, 280, h - 44 - list_bottom - 6),
                notes,
                "16-7",
                CREAM,
                0,
                true,
                Some([0, list_bottom, 300, h - 44]),
            );
        }
        f.button(
            "abandon",
            rect(80, h - 40, 140, 36),
            "Abandon",
            self.selected.is_some(),
        );
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Activate(id) if id == "close" => return vec![PanelAction::Close],
            ControlEvent::Select { id, index } if id == "rows" => {
                if let Some(k) = c.game.contracts().get(index) {
                    self.selected = Some(k.contract_id);
                }
            }
            ControlEvent::Scroll { id, value } if id == "scroll" => self.scroll = value.max(0),
            ControlEvent::Activate(id) if id == "abandon" => {
                if let Some(contract_id) = self.selected.take() {
                    return vec![PanelAction::Game(UiRequest::AbandonContract {
                        contract_id,
                    })];
                }
            }
            _ => {}
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic panel adapter; the requests' behaviour is the runtime's).
    use super::*;
    use dereth_client_contract::view::{CharacterTitles, ContractEntry};

    #[derive(Debug, Default)]
    struct Game {
        era: Option<dereth_client_contract::EraView>,
    }
    impl GameView for Game {
        fn era(&self) -> Option<&dereth_client_contract::EraView> {
            self.era.as_ref()
        }
        fn character_titles(&self) -> CharacterTitles {
            CharacterTitles {
                display: 2,
                titles: vec![(2, "Wanderer".into()), (5, "Apprentice".into())],
            }
        }
        fn contracts(&self) -> Vec<ContractEntry> {
            vec![ContractEntry {
                contract_id: 9,
                name: "Rats".into(),
                status: "1/5".into(),
                ..Default::default()
            }]
        }
    }
    fn with(game: &Game, run: impl FnOnce(&Context<'_>)) {
        let (state, pregame, keyboard, settings) = Default::default();
        run(&Context {
            game,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &state,
        });
    }

    #[test]
    fn a_title_picked_from_the_list_is_set_as_the_shown_title() {
        let mut titles = Titles::default();
        with(&Game::default(), |c| {
            // The rows are in name order: Apprentice first.
            titles.event(
                ControlEvent::Select {
                    id: "rows".into(),
                    index: 0,
                },
                c,
            );
            assert_eq!(
                titles.event(ControlEvent::Activate("set".into()), c),
                [PanelAction::Game(UiRequest::SetDisplayCharacterTitle {
                    title_id: 5
                })]
            );
        });
    }

    #[test]
    fn a_picked_contract_is_abandoned_and_a_world_without_contracts_says_so() {
        let mut contracts = Contracts::default();
        with(&Game::default(), |c| {
            contracts.event(
                ControlEvent::Select {
                    id: "rows".into(),
                    index: 0,
                },
                c,
            );
            assert_eq!(
                contracts.event(ControlEvent::Activate("abandon".into()), c),
                [PanelAction::Game(UiRequest::AbandonContract {
                    contract_id: 9
                })]
            );
        });
        let mut era = dereth_client_contract::EraView::default();
        era.era = dereth_primitives::era::EraId::Infiltration;
        era.era_announced = true;
        let plain = Game { era: Some(era) };
        with(&plain, |c| {
            let f = contracts.frame(c);
            assert!(!f.controls.iter().any(|k| k.id == "abandon"));
        });
    }
}
