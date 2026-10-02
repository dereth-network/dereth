//! The screens leave out what the world's era lacks: on an Infiltration world the quest page has
//! no Contracts tab and the character page no Titles tab (a page left open on one moves to the
//! next tab), the paper doll no cloak or trinket slot, the character sheet no luminance section,
//! and the toolbar no journal button (the quest page never opens); on an end-of-retail world all
//! are there.
//! Fixture: shipped layouts and strings loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::EraId;
use dereth_ui::{Delivery, ElemHandle, Screen, UiSystem};
use dereth_ui_screens::panels::characterinfo::CharacterInfoPanel;
use dereth_ui_screens::panels::era::EraPanels;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{CharacterInfo, EraView, GameView};

#[derive(Debug)]
struct World(EraView);

impl GameView for World {
    fn era(&self) -> Option<&EraView> {
        Some(&self.0)
    }
}

fn world(era: EraId) -> World {
    World(EraView {
        era,
        era_announced: true,
        ..EraView::default()
    })
}

fn screen() -> (UiSystem, GamePlayScreen) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    pump(&mut ui, &mut s);
    (ui, s)
}

fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            break;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
}

fn open_tab(ui: &UiSystem, tab: ElemHandle) -> Option<dereth_ui::ElementId> {
    let page = ui.node(tab)?.region.parent?;
    ui.node(page)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<dereth_ui::widgets::panel::Panel>()?
        .open_tab
}

/// Behaviour: presentation.era.the-screens-leave-out-what-the-worlds-era-lacks
#[test]
fn an_infiltration_world_has_no_contracts_tab_and_the_quest_page_leaves_it() {
    let (mut ui, mut s) = screen();
    let root = s.roots()[0];
    let tab = ui
        .get_child_recursive(root, dereth_ui_screens::panels::contracts::TAB)
        .expect("the Contracts tab");
    // The quest page open on its Contracts tab, as a click on the tab leaves it.
    ui.broadcast_element_message(tab, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    pump(&mut ui, &mut s);
    assert_eq!(
        open_tab(&ui, tab),
        Some(dereth_ui_screens::panels::contracts::TAB)
    );

    let mut era = EraPanels::default();
    era.post_init(&ui, root);
    assert!(era.update(&mut ui, &world(EraId::Infiltration)));
    pump(&mut ui, &mut s);
    let n = ui.node(tab).expect("alive");
    assert!(!n.region.flags.visible, "the tab is hidden");
    let now = open_tab(&ui, tab).expect("a tab is open");
    assert_ne!(now, dereth_ui_screens::panels::contracts::TAB);
    // Applied once: the same era again changes nothing.
    assert!(!era.update(&mut ui, &world(EraId::Infiltration)));

    assert!(era.update(&mut ui, &world(EraId::Eor)));
    assert!(ui.node(tab).expect("alive").region.flags.visible);
}

/// Behaviour: presentation.era.the-screens-leave-out-what-the-worlds-era-lacks
#[test]
fn an_infiltration_character_sheet_has_no_luminance_section() {
    let (mut ui, s) = screen();
    let root = s.roots()[0];
    let header = dereth_ui_screens::panels::characterinfo::compose(
        &ui,
        "ID_CharacterInfo_Luminance_Header",
        &[],
    );
    assert!(!header.is_empty(), "the header's string is shipped");
    let info = CharacterInfo::default();

    let mut sheet = CharacterInfoPanel::default();
    sheet.post_init(&mut ui, root);
    sheet.write(&mut ui, Some(&info));
    assert!(sheet.text.contains(&header), "{}", sheet.text);

    sheet.era_lacks_luminance = true;
    sheet.write(&mut ui, Some(&info));
    assert!(!sheet.text.contains(&header), "{}", sheet.text);
}

/// Behaviour: presentation.era.the-screens-leave-out-what-the-worlds-era-lacks
#[test]
fn an_infiltration_world_has_no_titles_tab_and_no_cloak_or_trinket_slot() {
    let (mut ui, mut s) = screen();
    let root = s.roots()[0];
    let titles = ui
        .get_child_recursive(root, dereth_ui_screens::panels::titles::PANEL)
        .expect("the Titles panel");
    let page = ui
        .node(titles)
        .and_then(|n| n.region.parent)
        .expect("its page");
    let tab_id = *ui
        .node(page)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| b.as_any())
        .and_then(<dyn std::any::Any>::downcast_ref::<dereth_ui::widgets::panel::Panel>)
        .expect("the character page is tabbed")
        .page_to_tab
        .get(&dereth_ui_screens::panels::titles::PANEL)
        .expect("a tab opens the Titles panel");
    let tab = ui
        .get_child_recursive(page, tab_id)
        .expect("the Titles tab");
    // The character page open on its Titles tab.
    ui.broadcast_element_message(tab, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    pump(&mut ui, &mut s);
    assert_eq!(open_tab(&ui, tab), Some(tab_id));
    let slot = |ui: &UiSystem, id| {
        let h = ui
            .get_child_recursive(root, id)
            .expect("the paper doll slot");
        ui.node(h).expect("alive").region.flags.visible
    };
    let (cloak, trinket) = (
        dereth_ui_screens::panels::era::CLOAK_SLOT,
        dereth_ui_screens::panels::era::TRINKET_SLOT,
    );
    assert!(
        slot(&ui, cloak) && slot(&ui, trinket),
        "shown before any era"
    );

    let mut era = EraPanels::default();
    era.post_init(&ui, root);
    assert!(era.update(&mut ui, &world(EraId::Infiltration)));
    pump(&mut ui, &mut s);
    assert!(
        !ui.node(tab).expect("alive").region.flags.visible,
        "the tab is hidden"
    );
    assert_ne!(
        open_tab(&ui, tab),
        Some(tab_id),
        "the page left the Titles tab"
    );
    assert!(!slot(&ui, cloak) && !slot(&ui, trinket));

    assert!(era.update(&mut ui, &world(EraId::Eor)));
    assert!(ui.node(tab).expect("alive").region.flags.visible);
    assert!(slot(&ui, cloak) && slot(&ui, trinket));
}

/// Behaviour: presentation.era.the-screens-leave-out-what-the-worlds-era-lacks
#[test]
fn an_infiltration_world_has_no_journal_button_and_never_opens_the_quest_page() {
    let (mut ui, mut s) = screen();
    let quest = s
        .panels
        .pages
        .iter()
        .find(|p| p.element == dereth_ui_screens::panels::journal::PAGE)
        .copied()
        .expect("the quest page is a page of the stack");
    let button = s
        .toolbar
        .buttons
        .iter()
        .find(|b| b.panel_id == quest.panel_id)
        .map(|b| b.handle)
        .expect("a toolbar button opens the quest page");
    let shown = |ui: &UiSystem, h| ui.node(h).expect("alive").region.flags.visible;
    s.recv_set_panel_visibility(&mut ui, quest.panel_id, true);
    assert!(shown(&ui, quest.handle), "the journal opens before any era");

    let infiltration = EraId::Infiltration.features();
    assert!(s.apply_era(&mut ui, infiltration));
    assert!(!s.apply_era(&mut ui, infiltration), "applied once");
    assert!(!shown(&ui, quest.handle), "the open quest page closes");
    assert!(!shown(&ui, button), "the button is hidden");
    s.recv_set_panel_visibility(&mut ui, quest.panel_id, true);
    assert!(!shown(&ui, quest.handle), "a key cannot open it");

    assert!(s.apply_era(&mut ui, EraId::Eor.features()));
    assert!(shown(&ui, button));
    s.recv_set_panel_visibility(&mut ui, quest.panel_id, true);
    assert!(shown(&ui, quest.handle));
}
