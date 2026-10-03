//! The screens leave out what the world's era lacks: on an Infiltration world the quest page has
//! no Contracts tab and the character page no Titles tab (a page left open on one moves to the
//! next tab), the paper doll no cloak or trinket slot, the character sheet no luminance section,
//! and the toolbar no journal button (the quest page never opens); on an end-of-retail world all
//! are there. A world the server announces without trade, tinkering, housing or chess never opens
//! the secure-trade, salvage, house purchase or chess window, and has no House tab on the map page.
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
    assert_eq!(
        open_tab(&ui, tab),
        None,
        "neither quest system is available"
    );
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

    sheet.era_features = EraId::Infiltration.features();
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

    // The strip's tabs, left to right, as the layout has them.
    let strip = |ui: &UiSystem| -> Vec<(ElemHandle, i32, i32, bool)> {
        let p = ui
            .node(page)
            .and_then(|n| n.behaviour.as_ref())
            .and_then(|b| b.as_any())
            .and_then(<dyn std::any::Any>::downcast_ref::<dereth_ui::widgets::panel::Panel>)
            .expect("tabbed");
        let mut tabs: Vec<_> = p
            .tab_to_page
            .keys()
            .filter_map(|&id| ui.get_child_recursive(page, id))
            .map(|h| {
                let n = ui.node(h).expect("alive");
                let b = n.region.box_;
                (h, b.x0, b.x1 + 1, n.region.flags.visible)
            })
            .collect();
        tabs.sort_by_key(|t| t.1);
        tabs
    };
    let own = strip(&ui);
    assert_eq!(own.len(), 3, "attributes, skills and titles");
    let (start, end) = (own[0].1, own[2].2);

    let mut era = EraPanels::default();
    era.post_init(&ui, root);
    assert!(era.update(&mut ui, &world(EraId::Infiltration)));
    pump(&mut ui, &mut s);
    assert!(
        !ui.node(tab).expect("alive").region.flags.visible,
        "the tab is hidden"
    );
    // Attributes and Skills grow over the strip, half each, their end caps at their ends.
    let left: Vec<(i32, i32)> = strip(&ui)
        .iter()
        .filter(|t| t.3)
        .map(|t| (t.1, t.2))
        .collect();
    let half = start + (end - start) / 2;
    assert_eq!(
        left,
        [(start, half), (half, end)],
        "the two tabs fill the strip"
    );
    let first = strip(&ui)[0].0;
    let caps: Vec<(i32, i32)> = ui
        .children(first)
        .iter()
        .map(|&c| {
            let b = ui.node(c).expect("alive").region.box_;
            (b.x0, b.x1 + 1)
        })
        .collect();
    assert!(
        caps.iter().any(|&(_, e)| e == half - start),
        "a part of the tab reaches its right end: {caps:?}"
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
    let back: Vec<(i32, i32)> = strip(&ui).iter().map(|t| (t.1, t.2)).collect();
    let own: Vec<(i32, i32)> = own.iter().map(|t| (t.1, t.2)).collect();
    assert_eq!(
        back, own,
        "with the Titles tab back each tab is in its own place"
    );
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

/// Behaviour: presentation.era.the-panel-buttons-close-up-over-a-system-the-world-lacks
#[test]
fn on_an_infiltration_world_the_panel_buttons_close_up_over_the_missing_journal_button() {
    let (mut ui, mut s) = screen();
    // The row of small panel buttons, as (handle, x) in the layout's own places, left to right.
    let row = |ui: &UiSystem, s: &GamePlayScreen| -> Vec<(ElemHandle, i32, bool)> {
        let first = s.toolbar.buttons[0].slot;
        let mut r: Vec<_> = s
            .toolbar
            .buttons
            .iter()
            .filter(|b| b.slot.y0 == first.y0 && b.slot.height() == first.height())
            .map(|b| {
                let n = ui.node(b.handle).expect("alive");
                (b.handle, n.region.box_.x0, n.region.flags.visible)
            })
            .collect();
        r.sort_by_key(|(_, x, _)| *x);
        r
    };
    let places: Vec<i32> = row(&ui, &s).iter().map(|(_, x, _)| *x).collect();
    assert_eq!(places.len(), 6, "six small panel buttons in one row");
    assert!(row(&ui, &s).iter().all(|(_, _, shown)| *shown));

    let width = |ui: &UiSystem, h: ElemHandle| ui.node(h).expect("alive").region.box_.width();
    let all = row(&ui, &s);
    let end = all[5].1 + width(&ui, all[5].0);
    let back = ui
        .get_child_recursive(s.roots()[0], dereth_ui_screens::toolbar::BAR_BACK)
        .expect("the bar's strip");
    let own_back = ui.node(back).expect("alive").region.box_;

    assert!(s.apply_era(&mut ui, EraId::Infiltration.features()));
    let shown: Vec<(ElemHandle, i32)> = row(&ui, &s)
        .iter()
        .filter(|(_, _, shown)| *shown)
        .map(|(h, x, _)| (*h, *x))
        .collect();
    assert_eq!(shown.len(), 5, "the journal button is hidden");
    assert_eq!(shown[0].1, places[0], "the first starts where the row does");
    assert_eq!(
        shown[4].1 + width(&ui, shown[4].0),
        end,
        "the last ends where the row does"
    );
    let gaps: Vec<i32> = shown.windows(2).map(|w| w[1].1 - w[0].1).collect();
    let (least, most) = (
        gaps.iter().min().copied().unwrap_or(0),
        gaps.iter().max().copied().unwrap_or(0),
    );
    assert!(most - least <= 1, "evenly spread: {gaps:?}");
    // The bar's black and gold strip runs behind the whole row, so there is no hole.
    let b = ui.node(back).expect("alive").region.box_;
    assert_eq!(
        (b.x0, b.x1 + 1),
        (places[0], end),
        "the strip spans the row"
    );

    assert!(s.apply_era(&mut ui, EraId::Eor.features()));
    let back: Vec<i32> = s
        .toolbar
        .buttons
        .iter()
        .map(|b| ui.node(b.handle).expect("alive").region.box_.x0)
        .collect();
    let own: Vec<i32> = s.toolbar.buttons.iter().map(|b| b.slot.x0).collect();
    assert_eq!(back, own, "every button is back in its own place");
    assert_eq!(
        ui.node(
            ui.get_child_recursive(s.roots()[0], dereth_ui_screens::toolbar::BAR_BACK)
                .expect("the strip")
        )
        .expect("alive")
        .region
        .box_,
        own_back,
        "and the strip its own size"
    );
}

/// Behaviour: presentation.era.the-screens-leave-out-what-the-worlds-era-lacks
#[test]
fn a_world_without_trade_tinkering_housing_or_chess_never_opens_their_windows() {
    use dereth_ui_screens::panels::{minigame, salvage, slumlord, trade};
    let (mut ui, mut s) = screen();
    let page = |s: &GamePlayScreen, element| {
        s.panels
            .pages
            .iter()
            .chain(s.env_panel.pages.iter())
            .find(|p| p.element == element)
            .copied()
            .expect("a page of one of the stacks")
    };
    let shown = |ui: &UiSystem, h| ui.node(h).expect("alive").region.flags.visible;
    let windows = [
        ("trade", trade::WINDOW),
        ("tinkering", salvage::PANEL),
        ("housing", slumlord::PANEL),
        ("chess", minigame::PANEL),
    ];
    // Announced by the server over an end-of-retail world.
    let mut view = EraView {
        era: EraId::Eor,
        era_announced: true,
        ..EraView::default()
    };
    view.announced_features = dereth_primitives::EraFeatureOverrides::parse(
        "trade=false,tinkering=false,housing=false,chess=false",
    )
    .expect("parses")
    .0;
    let lacking = view.features();
    assert!(!lacking.apartments, "housing off takes apartments with it");
    for (system, element) in windows {
        let p = page(&s, element);
        assert_ne!(p.panel_id, 0, "{system}");
        s.recv_set_panel_visibility(&mut ui, p.panel_id, true);
        assert!(shown(&ui, p.handle), "{system}: opens before any era");
        s.apply_era(&mut ui, lacking);
        assert!(!shown(&ui, p.handle), "{system}: the open window closes");
        s.recv_set_panel_visibility(&mut ui, p.panel_id, true);
        assert!(!shown(&ui, p.handle), "{system}: nothing opens it");
        assert!(s.apply_era(&mut ui, EraId::Eor.features()));
        s.recv_set_panel_visibility(&mut ui, p.panel_id, true);
        assert!(shown(&ui, p.handle), "{system}: opens at the end of retail");
        s.recv_set_panel_visibility(&mut ui, p.panel_id, false);
    }
    // The chess window's toolbar lamp goes with it.
    let chess = page(&s, minigame::PANEL);
    let lamps: Vec<ElemHandle> = s
        .toolbar
        .buttons
        .iter()
        .filter(|b| b.panel_id == chess.panel_id)
        .map(|b| b.handle)
        .collect();
    s.apply_era(&mut ui, lacking);
    assert!(lamps.iter().all(|&h| !shown(&ui, h)));
    // Infiltration had every one of them.
    let infiltration = EraId::Infiltration.features();
    assert!(
        infiltration.trade && infiltration.tinkering && infiltration.housing && infiltration.chess
    );
}

/// Behaviour: presentation.era.the-screens-leave-out-what-the-worlds-era-lacks
#[test]
fn a_world_without_housing_has_no_house_tab_on_the_map_page() {
    let (mut ui, mut s) = screen();
    let root = s.roots()[0];
    let house = ui
        .get_child_recursive(root, dereth_ui_screens::panels::house::TAB)
        .expect("the House tab");
    ui.broadcast_element_message(house, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    pump(&mut ui, &mut s);
    assert_eq!(
        open_tab(&ui, house),
        Some(dereth_ui_screens::panels::house::TAB)
    );
    let mut view = world(EraId::Infiltration);
    view.0.announced_features = dereth_primitives::EraFeatureOverrides::parse("housing=false")
        .expect("parses")
        .0;
    let mut era = EraPanels::default();
    era.post_init(&ui, root);
    assert!(era.update(&mut ui, &view));
    pump(&mut ui, &mut s);
    assert!(!ui.node(house).expect("alive").region.flags.visible);
    assert_ne!(
        open_tab(&ui, house),
        Some(dereth_ui_screens::panels::house::TAB),
        "the map page left the House tab"
    );
    // February 2005 had houses.
    assert!(era.update(&mut ui, &world(EraId::Infiltration)));
    assert!(ui.node(house).expect("alive").region.flags.visible);
}

#[derive(Debug)]
struct Facts {
    profile: EraView,
    bits: i32,
    magic: Option<dereth_client_contract::era::EraUiFacts>,
}
impl GameView for Facts {
    fn era(&self) -> Option<&EraView> {
        Some(&self.profile)
    }
    fn player(&self) -> Option<dereth_primitives::ObjectId> {
        Some(dereth_primitives::ObjectId(1))
    }
    fn int_stat(&self, _: dereth_primitives::ObjectId, prop: u32) -> Option<i32> {
        (prop == 0x142).then_some(self.bits)
    }
    fn era_ui(&self) -> dereth_client_contract::era::EraUiFacts {
        self.magic.unwrap_or_else(|| {
            dereth_client_contract::era::EraUiFacts::for_profile(self.profile.era)
        })
    }
    fn spell_tab(&self, tab: usize) -> &[u32] {
        if tab == 7 {
            &[777]
        } else {
            &[]
        }
    }
    fn character_info(&self) -> Option<CharacterInfo> {
        let mut info = CharacterInfo::default();
        info.aug_ints.insert(0xda, 2);
        Some(info)
    }
}
fn facts() -> Facts {
    Facts {
        profile: EraView::default(),
        bits: 0,
        magic: None,
    }
}
fn available(ui: &UiSystem, root: ElemHandle, id: dereth_ui::ElementId) -> (bool, bool) {
    let n = ui
        .node(ui.get_child_recursive(root, id).expect("authored element"))
        .unwrap();
    (n.region.flags.visible, n.should_be_mouse_visible)
}

/// Behaviour: presentation.era.aetheria-slots-follow-character-unlocks
#[test]
fn aetheria_quality_changes_refresh_visible_and_mouse_slots_without_an_era_change() {
    use dereth_ui_screens::panels::inventory::SIGIL_SLOTS;
    let (mut ui, s) = screen();
    let root = s.roots()[0];
    let mut era = EraPanels::default();
    era.post_init(&ui, root);
    let mut w = facts();
    for bits in [0, 1, 2, 4, 7, 0x100] {
        w.bits = bits;
        era.update(&mut ui, &w);
        for (i, id) in SIGIL_SLOTS.into_iter().enumerate() {
            let has = bits & (1 << i) != 0;
            assert_eq!(
                available(&ui, root, id),
                (has, has),
                "bits={bits:#x}, slot={i}"
            );
        }
        assert!(!era.update(&mut ui, &w), "unchanged facts need no writes");
    }
    w.bits = 7;
    w.profile.era = EraId::Infiltration;
    era.update(&mut ui, &w);
    for id in SIGIL_SLOTS {
        assert_eq!(available(&ui, root, id), (false, false));
    }
}

/// Behaviour: presentation.era.shared-facts-follow-the-world-profile
#[test]
fn quest_system_combinations_keep_valid_tabs_and_remove_unavailable_click_targets() {
    use dereth_ui_screens::panels::{contracts, journal, pagelist};
    let (mut ui, mut s) = screen();
    let root = s.roots()[0];
    let mut era = EraPanels::default();
    era.post_init(&ui, root);
    let quest = s
        .panels
        .pages
        .iter()
        .find(|p| p.element == journal::PAGE)
        .copied()
        .unwrap();
    let button = s
        .toolbar
        .buttons
        .iter()
        .find(|b| b.panel_id == quest.panel_id)
        .unwrap()
        .handle;
    let initial = ui.get_child_recursive(root, journal::TAB).unwrap();
    ui.broadcast_element_message(initial, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    pump(&mut ui, &mut s);
    let mut w = facts();
    for (journal_on, contracts_on) in [
        (true, true),
        (false, true),
        (true, false),
        (false, false),
        (true, true),
    ] {
        w.profile.announced_features.set("journal", journal_on);
        w.profile.announced_features.set("contracts", contracts_on);
        let features = w.era_features();
        s.apply_era(&mut ui, features);
        era.update(&mut ui, &w);
        pump(&mut ui, &mut s);
        for (id, has) in [
            (journal::TAB, journal_on),
            (pagelist::TAB, journal_on),
            (contracts::TAB, contracts_on),
        ] {
            assert_eq!(available(&ui, root, id), (has, has));
        }
        assert_eq!(
            ui.node(button).unwrap().region.flags.visible,
            journal_on || contracts_on
        );
        s.recv_set_panel_visibility(&mut ui, quest.panel_id, true);
        assert_eq!(
            ui.node(quest.handle).unwrap().region.flags.visible,
            journal_on || contracts_on
        );
        let tab = ui.get_child_recursive(root, journal::TAB).unwrap();
        let expected = if journal_on {
            Some(journal::TAB)
        } else if contracts_on {
            Some(contracts::TAB)
        } else {
            None
        };
        assert_eq!(open_tab(&ui, tab), expected);
    }
    let contract_tab = ui.get_child_recursive(root, contracts::TAB).unwrap();
    ui.broadcast_element_message(contract_tab, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    pump(&mut ui, &mut s);
    w.bits = 1;
    era.update(&mut ui, &w);
    pump(&mut ui, &mut s);
    assert_eq!(open_tab(&ui, contract_tab), Some(contracts::TAB));
    let pages = ui.get_child_recursive(root, pagelist::TAB).unwrap();
    ui.broadcast_element_message(pages, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
    pump(&mut ui, &mut s);
    w.bits = 2;
    era.update(&mut ui, &w);
    pump(&mut ui, &mut s);
    assert_eq!(
        open_tab(&ui, pages),
        Some(pagelist::TAB),
        "valid journal subpage stays selected"
    );
}

/// Behaviour: presentation.era.shared-facts-follow-the-world-profile
#[test]
fn magic_controls_and_favorite_navigation_follow_independent_profile_facts() {
    use dereth_ui_screens::panels::{spellbook, spellcasting};
    let (mut ui, mut s) = screen();
    let root = s.roots()[0];
    let mut book = spellbook::SpellbookPanel::default();
    book.post_init(&mut ui, root);
    let mut bar = spellcasting::SpellcastingPanel::default();
    bar.post_init(&mut ui, root);
    let mut w = facts();
    w.magic = Some(dereth_client_contract::era::EraUiFacts {
        void_magic: false,
        spell_level_eight: true,
        spell_favorite_tabs: 7,
    });
    book.update(&mut ui, &w);
    bar.update(&mut ui, &w);
    pump(&mut ui, &mut s);
    for (id, bit) in spellbook::FILTER_BUTTONS {
        assert_eq!(
            available(&ui, root, dereth_ui::ElementId(id)),
            (bit != 0x2000, bit != 0x2000)
        );
    }
    ui.requests.clear();
    let void_button = ui.get_element(spellbook::School::Void.button()).unwrap();
    ui.set_state(void_button, dereth_ui::StateId(1));
    book.update_filter(&mut ui, &w, spellbook::School::Void.button(), 0x2000);
    assert!(
        ui.requests.take().is_empty(),
        "hidden filter cannot write the mask"
    );
    let eighth = dereth_ui::ElementId(spellcasting::SUB_MENU_TAB_BUTTONS[7]);
    assert_eq!(available(&ui, root, eighth), (false, false));
    assert!(!bar.add_favorite(&mut ui, &w, 7, 777, -1, true));
    assert_eq!(w.spell_tab(7), &[777]);
    assert_eq!(
        bar.favorites_pruned, 0,
        "the hidden bank is retained even if unknown"
    );
    for which in [spellcasting::TabJump::Next, spellcasting::TabJump::Prev] {
        let panel = ui
            .node_mut(bar.panel.unwrap())
            .unwrap()
            .behaviour
            .as_mut()
            .unwrap()
            .as_any_mut()
            .unwrap()
            .downcast_mut::<dereth_ui::widgets::panel::Panel>()
            .unwrap();
        panel.open_tab = Some(dereth_ui::ElementId(0xdead));
        assert_eq!(
            bar.open_spell_tab(&mut ui, which).unwrap().0,
            spellcasting::SUB_MENU_TAB_BUTTONS[0]
        );
        pump(&mut ui, &mut s);
    }
    let last = bar
        .open_spell_tab(&mut ui, spellcasting::TabJump::Last)
        .unwrap();
    pump(&mut ui, &mut s);
    assert_eq!(last.0, spellcasting::SUB_MENU_TAB_BUTTONS[6]);
    assert_eq!(
        bar.open_spell_tab(&mut ui, spellcasting::TabJump::Next)
            .unwrap()
            .0,
        spellcasting::SUB_MENU_TAB_BUTTONS[0]
    );
    pump(&mut ui, &mut s);
    w.magic = None;
    book.update(&mut ui, &w);
    bar.update(&mut ui, &w);
    pump(&mut ui, &mut s);
    assert_eq!(available(&ui, root, eighth), (true, true));
    bar.open_spell_tab(&mut ui, spellcasting::TabJump::Last);
    pump(&mut ui, &mut s);
    w.profile.era = EraId::Infiltration;
    bar.update(&mut ui, &w);
    pump(&mut ui, &mut s);
    assert_eq!(
        bar.open_sub_menu_index(&ui),
        0,
        "an unavailable current tab falls back immediately"
    );
}

/// Behaviour: presentation.era.shared-facts-follow-the-world-profile
#[test]
fn fellowship_caption_uses_the_complete_dat_variant_only_for_its_option() {
    use dereth_client_contract::PlayerOption;
    use dereth_ui_screens::panels::era::option_caption;
    let (ui, _) = screen();
    assert_eq!(
        option_caption(&ui, PlayerOption::FellowshipShareXP, EraId::Eor.features()).as_deref(),
        Some("Share Fellowship Experience and Luminance")
    );
    assert_eq!(
        option_caption(
            &ui,
            PlayerOption::FellowshipShareXP,
            EraId::Infiltration.features()
        )
        .as_deref(),
        Some("Share Fellowship Experience")
    );
    assert_eq!(
        option_caption(
            &ui,
            PlayerOption::FellowshipShareLoot,
            EraId::Infiltration.features()
        ),
        None
    );
}

/// Behaviour: presentation.era.shared-facts-follow-the-world-profile
#[test]
fn character_section_refreshes_when_only_augmentation_availability_changes() {
    let (mut ui, s) = screen();
    let root = s.roots()[0];
    let mut sheet = CharacterInfoPanel::default();
    sheet.post_init(&mut ui, root);
    ui.set_visible(sheet.panel.unwrap(), true);
    let mut w = facts();
    w.profile.announced_features.set("luminance", false);
    assert!(sheet.update(&mut ui, &w));
    let with = sheet.text.clone();
    w.profile
        .announced_features
        .set("innate_augmentations", false);
    assert!(sheet.update(&mut ui, &w));
    assert_ne!(sheet.text, with);
    assert!(sheet.sections[4].is_empty());
    w.profile
        .announced_features
        .set("innate_augmentations", true);
    assert!(sheet.update(&mut ui, &w));
    assert_eq!(sheet.text, with);
}

/// Behaviour: presentation.era.shared-facts-follow-the-world-profile
#[test]
fn character_options_restore_controls_and_refresh_complete_captions_on_feature_changes() {
    use dereth_client_contract::PlayerOption;
    let (mut ui, mut s) = screen();
    let mut w = facts();
    let original: Vec<_> = s.character_options.rows.iter().map(|r| r.option).collect();
    let boxed = s.character_options.row_count();
    let mut remaining = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    remaining.post_init(&mut ui, s.roots()[0]);
    for era in [
        EraId::Infiltration,
        EraId::Eor,
        EraId::Infiltration,
        EraId::Eor,
    ] {
        w.profile.era = era;
        s.apply_era(&mut ui, w.era_features());
        s.character_options.on_player_option_changed(&mut ui, &w);
        remaining.update(&mut ui, &w);
        let expected = Some(if era == EraId::Eor {
            "Share Fellowship Experience and Luminance"
        } else {
            "Share Fellowship Experience"
        });
        let share = remaining
            .fellowship_options
            .boxes
            .iter()
            .find(|b| b.option == PlayerOption::FellowshipShareXP)
            .unwrap();
        assert_eq!(share.label.as_deref(), expected);
        let p = &s.character_options;
        assert_eq!(
            p.row_of(PlayerOption::ShowCloak).is_some(),
            era == EraId::Eor
        );
        let i = p.row_of(PlayerOption::FellowshipShareXP).unwrap();
        assert_eq!(
            p.rows[i].label.as_deref(),
            Some(if era == EraId::Eor {
                "Share Fellowship Experience and Luminance"
            } else {
                "Share Fellowship Experience"
            })
        );
        if era == EraId::Eor {
            assert_eq!(
                p.rows.iter().map(|r| r.option).collect::<Vec<_>>(),
                original
            );
            assert_eq!(p.row_count(), boxed);
        }
    }
}
