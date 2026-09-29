//! The Character Options page is 6 headers, 6 separators, 50 toggles off the shipped tree,
//! captioned from their own tokens; a tick emits one SetPlayerOption; showing re-reads rows and
//! hiding reverts; page id ambiguity; first show swallows its own visibility message.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use crate::common::layout::Strings;
use std::cell::RefCell;
use std::rc::Rc;

use dereth_ui::framework::Screen;
use dereth_ui::msg::Delivery;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use dereth_ui_screens::options::character::{
    self, help_token, label_token, option_name, CHARACTER_PAGE_ELEMENT, HELP_SUFFIX, OPTION_BOX,
    STRING_TABLE_ENUM, TOKEN_PREFIX,
};
use dereth_ui_screens::options::pages::CHARACTER_SETTINGS_PAGE;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::{GameView, PlayerOption, UiRequest};

// ---------------------------------------------------------------------------------------------
// The literals, taken and from the built tree
// ---------------------------------------------------------------------------------------------

/// The Character Settings panel, and its type.
///
/// **Literals, not symbols** (the stated testability rule: a test that reads a constant through the same
/// symbol it writes it through cannot detect a wrong constant). Both were measured off the built
/// shipped tree; the id collides with the Keyboard panel's fifth tab page and the type is what
/// separates them.
const PAGE_ELEMENT: u32 = 0x1000_0211;
const PAGE_TYPE: u32 = 0x1000_0027;
/// The option box the character page's post-init binds — **not** the Client Options panel's `0x10000200`.
const OPTION_BOX_ID: u32 = 0x1000_01FA;
/// The child a toggle option looks up on its row, and the attribute a check-box option
/// refreshes.
const CHECKBOX_CHILD: u32 = 0x1000_0219;
const ATTR_CHECKED: u32 = 0x0E;
/// The seven option types' `0x10000035` checkbox option.
const UIOPTION_CHECKBOX: u32 = 0x1000_0035;

/// The **50 string literals** the page hashes, in page order, with
/// the six header tokens interleaved where the client puts them.
///
/// This is the whole oracle for the caption half, transcribed from retail's own list rather
/// than assembled from [`PlayerOption`]'s `Debug`. A token built by concatenation and checked by
/// concatenation cannot catch a wrong spelling — `ID_PlayerOption_FellowshipShareXP`'s capital
/// `XP` is the one that is not obvious, and `AutoRepeatAttack` / `AcceptLootPermits` are the two
/// whose enum name and token differ in nothing at all, which is exactly why they are here.
const TOKENS_IN_ORDER: [&str; 56] = [
    "ID_CharacterOption_UIBehavior_Section",
    "ID_PlayerOption_ViewCombatTarget",
    "ID_PlayerOption_SalvageMultiple",
    "ID_PlayerOption_MainPackPreferred",
    "ID_CharacterOption_UIDisplay_Section",
    "ID_PlayerOption_VividTargetingIndicator",
    "ID_PlayerOption_ShowTooltips",
    "ID_PlayerOption_CoordinatesOnRadar",
    "ID_PlayerOption_SideBySideVitals",
    "ID_PlayerOption_SpellDuration",
    "ID_PlayerOption_DisableMostWeatherEffects",
    "ID_PlayerOption_DisableDistanceFog",
    "ID_PlayerOption_PersistentAtDay",
    "ID_PlayerOption_DisableHouseRestrictionEffects",
    "ID_PlayerOption_UseCraftSuccessDialog",
    "ID_PlayerOption_ConfirmVolatileRareUse",
    "ID_PlayerOption_DisplayTimeStamps",
    "ID_PlayerOption_FilterLanguage",
    "ID_PlayerOption_ShowHelm",
    "ID_PlayerOption_ShowCloak",
    "ID_CharacterOption_Grouping_Section",
    "ID_PlayerOption_IgnoreAllegianceRequests",
    "ID_PlayerOption_IgnoreFellowshipRequests",
    "ID_PlayerOption_DisplayAllegianceLogonNotifications",
    "ID_PlayerOption_FellowshipShareXP",
    "ID_PlayerOption_FellowshipShareLoot",
    "ID_PlayerOption_FellowshipAutoAcceptRequests",
    "ID_CharacterOption_OtherPlayers_Section",
    "ID_PlayerOption_AcceptLootPermits",
    "ID_PlayerOption_UseDeception",
    "ID_PlayerOption_AllowGive",
    "ID_PlayerOption_IgnoreTradeRequests",
    "ID_PlayerOption_DragItemOnPlayerOpensSecureTrade",
    "ID_PlayerOption_DisplayDateOfBirth",
    "ID_PlayerOption_DisplayAge",
    "ID_PlayerOption_DisplayChessRank",
    "ID_PlayerOption_DisplayFishingSkill",
    "ID_PlayerOption_DisplayNumberDeaths",
    "ID_PlayerOption_DisplayNumberCharacterTitles",
    "ID_CharacterOption_CharacterBehavior_Section",
    "ID_PlayerOption_ToggleRun",
    "ID_PlayerOption_AdvancedCombatUI",
    "ID_PlayerOption_AutoTarget",
    "ID_PlayerOption_AutoRepeatAttack",
    "ID_PlayerOption_UseChargeAttack",
    "ID_PlayerOption_LeadMissileTargets",
    "ID_PlayerOption_UseFastMissiles",
    "ID_CharacterOption_Chat_Section",
    "ID_PlayerOption_StayInChatMode",
    "ID_PlayerOption_HearAllegianceChat",
    "ID_PlayerOption_HearGeneralChat",
    "ID_PlayerOption_HearTradeChat",
    "ID_PlayerOption_HearLFGChat",
    "ID_PlayerOption_HearRoleplayChat",
    "ID_PlayerOption_HearSocietyChat",
    "ID_PlayerOption_HearPKDeaths",
];

// ---------------------------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------------------------

/// `with_strings = false` is the **calibration** environment: everything else identical, no
/// resolver, so a caption count that cannot fall to zero is visible as such.
fn env(with_strings: bool) -> UiSystem {
    let (mut ui, _flow, store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    if with_strings {
        ui.strings = Some(Rc::new(Strings(Rc::clone(&store))));
    }
    ui
}

/// The shipped gameplay screen, built the way the client builds it.
fn screen(ui: &mut UiSystem) -> GamePlayScreen {
    ui.requests.clear();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(ui))
        .expect("the gameplay screen builds");
    s
}

fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) -> usize {
    let mut n = 0;
    for d in ui.drain_outbox() {
        if let Delivery::Element { msg, .. } = d {
            s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            n += 1;
        }
    }
    n
}

#[derive(Debug, Default)]
struct Module(Rc<RefCell<Vec<PlayerOption>>>);

impl GameView for Module {
    fn player_option(&self, o: PlayerOption) -> bool {
        self.0.borrow().contains(&o)
    }
}

// ---------------------------------------------------------------------------------------------
// The rows
// ---------------------------------------------------------------------------------------------

/// Oracle: the character-settings page's own option-building sequence in retail — **six**
/// headers, **six** separators (the last one trailing, after the Chat section, with no section
/// following it) and **50** toggle options.
#[test]
fn the_page_is_six_headers_six_separators_and_fifty_toggles_off_the_shipped_tree() {
    let mut ui = env(true);
    let s = screen(&mut ui);
    let p = &s.character_options;

    assert_eq!(p.rows.len(), 50, "toggle options added");
    assert_eq!(p.headers, 6, "headers added");
    assert_eq!(p.separators, 6, "separators added — the sixth is trailing");
    assert_eq!(p.row_count(), 62, "6 + 6 + 50 rows in the option box");
    assert_eq!(p.failures, 0, "every template-list add produced a row");

    // The section shape, in page-building order.
    let counts: Vec<usize> = CHARACTER_SETTINGS_PAGE
        .iter()
        .map(|c| c.options.len())
        .collect();
    assert_eq!(counts, vec![3, 15, 6, 11, 7, 8]);
    let order: Vec<PlayerOption> = p.rows.iter().map(|r| r.option).collect();
    let expect: Vec<PlayerOption> = CHARACTER_SETTINGS_PAGE
        .iter()
        .flat_map(|c| c.options.iter().copied())
        .collect();
    assert_eq!(order, expect, "the rows are in option initialization order");

    // Every row's control really is a checkbox option under `0x10000219` — the runtime type check
    // `add_toggle_option` performs, asserted against the literal type rather than the symbol.
    for r in &p.rows {
        let n = ui.node(r.element).expect("the check box is in the tree");
        assert_eq!(n.ty().0, UIOPTION_CHECKBOX, "{:?}", r.option);
        assert_eq!(n.element_id().0, CHECKBOX_CHILD, "{:?}", r.option);
    }

    // The post-init's one child, by literal id, and it is not the Client Options panel's.
    assert_eq!(OPTION_BOX.0, OPTION_BOX_ID);
    assert_ne!(OPTION_BOX_ID, 0x1000_0200);
    let page = s.character_options.page.expect("the page was bound");
    assert!(ui
        .get_child_recursive(page, ElementId(OPTION_BOX_ID))
        .is_some());
}

/// Oracle: the 50 + 6 string literals of `option initialization`, transcribed above, against the tokens the
/// page actually builds.
#[test]
fn the_page_names_every_row_the_way_init_options_spells_it() {
    let mut built: Vec<String> = Vec::new();
    for c in CHARACTER_SETTINGS_PAGE {
        built.push(c.header.to_string());
        for o in c.options {
            built.push(label_token(*o));
        }
    }
    assert_eq!(
        built, TOKENS_IN_ORDER,
        "the 56 tokens option initialization hashes, in order"
    );
    assert_eq!(TOKEN_PREFIX, "ID_PlayerOption_");
    assert_eq!(HELP_SUFFIX, "_Help");
    assert_eq!(
        help_token(PlayerOption::FellowshipShareXP),
        "ID_PlayerOption_FellowshipShareXP_Help"
    );
    assert_eq!(
        option_name(PlayerOption::AutoRepeatAttack),
        "AutoRepeatAttack"
    );
    // Setting a toggle's label puts both ids in table enum `0x10000003`, by literal.
    assert_eq!(STRING_TABLE_ENUM, 0x1000_0003);
}

/// Behaviour: options.character-page.the-page-is-forty-nine-toggles-captioned-from-their-own-tokens
/// Every row captions itself from its own token and none from the preference registry.
#[test]
fn every_row_captions_itself_from_its_own_token_and_none_from_the_preference_registry() {
    let mut ui = env(true);
    let s = screen(&mut ui);
    let p = &s.character_options;

    assert_eq!(p.row_captions, 50, "rows captioned of {}", p.rows.len());
    assert_eq!(p.header_captions, 6, "section headers captioned");
    let resolved = p
        .rows
        .iter()
        .filter(|r| r.label.as_deref().is_some_and(|s| !s.is_empty()));
    assert_eq!(resolved.count(), 50, "captions that resolved to real text");

    // Three of them by literal text, out of the shipped `client_local_English.dat`, so that a
    // caption count that is right for the wrong strings cannot pass.
    let text = |o: PlayerOption| {
        p.rows
            .iter()
            .find(|r| r.option == o)
            .and_then(|r| r.label.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        text(PlayerOption::ViewCombatTarget),
        "Keep Combat Targets in View"
    );
    assert_eq!(text(PlayerOption::ShowTooltips), "Display 3D Tooltips");
    assert_eq!(text(PlayerOption::SideBySideVitals), "Side By Side Vitals");

    // The other direction: the registry answers for none of them.
    let from_registry = CHARACTER_SETTINGS_PAGE
        .iter()
        .flat_map(|c| c.options.iter().copied())
        .filter(|o| {
            dereth_ui_screens::options::preferences::inq_preference(option_name(*o)).is_some()
                || dereth_ui_screens::options::preferences::inq_preference(&label_token(*o))
                    .is_some()
        })
        .count();
    assert_eq!(
        from_registry, 0,
        "rows whose caption comes from inq_preference, of 50"
    );
}

/// The calibration for the count above follows the live-run validation policy: the *same* page, built the
/// same way, with no `StringResolver` installed reads **0 of 50**. Without this, "50 of 50" and
/// "my counter cannot go down" are the same output.
#[test]
fn with_no_string_resolver_no_row_is_captioned() {
    let mut ui = env(false);
    let s = screen(&mut ui);
    let p = &s.character_options;
    assert_eq!(p.rows.len(), 50, "the rows are still built");
    let resolved = p
        .rows
        .iter()
        .filter(|r| r.label.as_deref().is_some_and(|s| !s.is_empty()))
        .count();
    assert_eq!(resolved, 0, "captions resolved with no resolver, of 50");
    assert_eq!(
        p.header_captions, 0,
        "section headers captioned with no resolver"
    );
}

// ---------------------------------------------------------------------------------------------
// The wire
// ---------------------------------------------------------------------------------------------

/// Behaviour: options.character-page.a-tick-raises-one-set-player-option
/// A check-box option's message-1 arm, end to end: a click on
/// the check box in the **shipped tree**, delivered through `GamePlayScreen::on_element_message`,
/// produces exactly one `UiRequest::SetPlayerOption`.
///
/// This is the wire the row was filed for. Before it, `CharacterSettingsPage` was not even a
/// module of this crate: `options/mod.rs` did not declare it, so 545 lines compiled to nothing.
#[test]
fn clicking_a_check_box_emits_one_set_player_option_and_nothing_else() {
    let mut ui = env(true);
    let mut s = screen(&mut ui);
    let i = s
        .character_options
        .row_of(PlayerOption::ShowTooltips)
        .expect("the row is on the page");
    let element = s.character_options.rows[i].element;
    assert!(
        !s.character_options.rows[i].current,
        "the module answers false with no producer"
    );

    // What the check box does before it raises message 1: button behavior writes `0x0E`.
    ui.set_attribute_bool(element, ATTR_CHECKED, true);
    ui.requests.clear();
    ui.drain_outbox();
    let msg = dereth_ui::ElementMessage {
        source_id: ElementId(CHECKBOX_CHILD),
        source: element,
        id: dereth_ui::msg::element::id::BUTTON_CLICKED,
        p1: 0,
        p2: 0,
        point: dereth_ui::msg::MessagePoint::default(),
        serial: 1,
    };
    s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(&mut ui), &msg);
    pump(&mut ui, &mut s);

    let got = ui.requests.take();
    assert_eq!(
        got,
        vec![UiRequest::SetPlayerOption(PlayerOption::ShowTooltips, true)],
        "one request, naming one option"
    );
    assert!(s.character_options.rows[i].current);
    assert!(
        s.character_options.changed(),
        "the page reports itself changed"
    );

    // Nothing else on the page moved — the click is one option, not a page-wide recompute.
    let moved: Vec<PlayerOption> = s
        .character_options
        .rows
        .iter()
        .filter(|r| r.current)
        .map(|r| r.option)
        .collect();
    assert_eq!(moved, vec![PlayerOption::ShowTooltips]);
}

/// The page never composes an option word from its own check boxes.
#[test]
fn the_page_never_composes_an_option_word_from_its_own_check_boxes() {
    let mut ui = env(true);
    let mut s = screen(&mut ui);
    ui.requests.clear();
    for i in 0..s.character_options.rows.len() {
        let element = s.character_options.rows[i].element;
        ui.set_attribute_bool(element, ATTR_CHECKED, true);
        let msg = dereth_ui::ElementMessage {
            source_id: ElementId(CHECKBOX_CHILD),
            source: element,
            id: dereth_ui::msg::element::id::BUTTON_CLICKED,
            p1: 0,
            p2: 0,
            point: dereth_ui::msg::MessagePoint::default(),
            serial: 1,
        };
        s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(&mut ui), &msg);
    }
    let got = ui.requests.take();
    assert_eq!(got.len(), 50, "one request per row, of 50 rows");
    let named: Vec<PlayerOption> = got
        .iter()
        .map(|r| match r {
            UiRequest::SetPlayerOption(o, v) => {
                assert!(*v);
                *o
            }
            other => panic!("the page emitted something other than SetPlayerOption: {other:?}"),
        })
        .collect();
    let expect: Vec<PlayerOption> = CHARACTER_SETTINGS_PAGE
        .iter()
        .flat_map(|c| c.options.iter().copied())
        .collect();
    assert_eq!(named, expect, "50 distinct options, in page order");
}

/// Behaviour: options.character-page.each-row-shows-the-bit-the-shard-sent-for-it
/// The page's visibility hook — `save_current_values` on show,
/// `restore_saved_values` on hide, driven through the screen's own queue.
///
/// The show arm's return is the **denominator**: a `save_current_values` that silently did nothing
/// and one that re-read 50 identical values are otherwise the same answer.
#[test]
fn showing_the_page_re_reads_every_row_from_the_module_and_hiding_it_reverts() {
    let mut ui = env(true);
    let mut s = screen(&mut ui);
    let page = s.character_options.page.expect("bound");
    let module = Module::default();
    module.0.borrow_mut().push(PlayerOption::ShowHelm);
    module.0.borrow_mut().push(PlayerOption::ToggleRun);

    // Show / hide / show, because the first show's `0x18` is swallowed — see the pinned defect
    // below. The two later edges are the ones the client would deliver on every visit.
    for v in [true, false, true] {
        ui.set_visible(page, v);
        pump(&mut ui, &mut s);
    }
    let moved = s.drive_character_options(&mut ui, &module);
    assert!(
        !moved.is_empty(),
        "no visibility edge reached the page at all"
    );
    assert_eq!(
        moved.last().copied(),
        Some(2),
        "the show arm re-read 2 of 50 rows as newly set"
    );
    assert!(
        s.character_options.rows[s.character_options.row_of(PlayerOption::ShowHelm).unwrap()]
            .current
    );
    assert!(
        !s.character_options.rows[s
            .character_options
            .row_of(PlayerOption::ShowTooltips)
            .unwrap()]
        .current
    );
    // The value is on the element too — writes `0x0E`.
    let helm = s.character_options.rows
        [s.character_options.row_of(PlayerOption::ShowHelm).unwrap()]
    .element;
    assert_eq!(
        ui.node(helm)
            .map(|n| format!("{:?}", n.merged_properties().get(ATTR_CHECKED))),
        Some("Some(Bool(true))".to_string()),
        "Refresh wrote attribute 0x0E on the check box"
    );

    // A change made on the page and then a hide: `restore_saved_values` writes the
    // snapshot back and re-applies only the rows that moved.
    let i = s
        .character_options
        .row_of(PlayerOption::ShowTooltips)
        .unwrap();
    let element = s.character_options.rows[i].element;
    ui.set_attribute_bool(element, ATTR_CHECKED, true);
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &dereth_ui::ElementMessage {
            source_id: ElementId(CHECKBOX_CHILD),
            source: element,
            id: dereth_ui::msg::element::id::BUTTON_CLICKED,
            p1: 0,
            p2: 0,
            point: dereth_ui::msg::MessagePoint::default(),
            serial: 1,
        },
    );
    assert!(s.character_options.rows[i].current);
    ui.requests.clear();
    ui.set_visible(page, false);
    pump(&mut ui, &mut s);
    let reverted = s.drive_character_options(&mut ui, &module);
    assert_eq!(reverted, vec![1], "one row rolled back on hide");
    assert!(!s.character_options.rows[i].current);
    assert_eq!(
        ui.requests.take(),
        vec![
            UiRequest::SetPanelVisibility {
                panel: 10,
                visible: false
            },
            UiRequest::SetPlayerOption(PlayerOption::ShowTooltips, false),
        ],
        "the container follows the page down, then the rollback is re-applied through the \
         module, one option"
    );
}

/// **The shipped tree carries element id `0x10000211` twice**, and a depth-first recursive child
/// search from the screen root reaches the wrong one first: the Keyboard panel's
/// fifth tab page (engine type 3) sits above the Character Settings panel (type `0x10000027`) in the walk.
///
/// Binding by id alone finds a page with no `0x100001FA` under it and leaves the option box
/// silently empty — indistinguishable from "the layout changed". `character::find_page` checks the
/// type, which is the type-checked cast the client performs on an element the element manager
/// handed it by construction.
#[test]
fn the_page_element_id_is_ambiguous_and_a_bare_recursive_lookup_finds_the_wrong_one() {
    let mut ui = env(true);
    let s = screen(&mut ui);
    let root = s.root().expect("a root");

    let mut all = Vec::new();
    fn walk(ui: &UiSystem, h: ElemHandle, out: &mut Vec<ElemHandle>) {
        out.push(h);
        for c in ui.children(h) {
            walk(ui, c, out);
        }
    }
    walk(&ui, root, &mut all);
    let types: Vec<u32> = all
        .iter()
        .filter_map(|h| ui.node(*h))
        .filter(|n| n.element_id().0 == PAGE_ELEMENT)
        .map(|n| n.ty().0)
        .collect();
    assert_eq!(
        types,
        vec![3, PAGE_TYPE],
        "two elements share id {PAGE_ELEMENT:#010X}"
    );

    let naive = ui
        .get_child_recursive(root, ElementId(PAGE_ELEMENT))
        .expect("finds one of them");
    assert_eq!(
        ui.node(naive).map(|n| n.ty().0),
        Some(3),
        "get_child_recursive finds key-binding panel's tab page"
    );
    let right = character::find_page(&ui, root).expect("find_page finds the option page");
    assert_ne!(right, naive);
    assert_eq!(ui.node(right).map(|n| n.ty().0), Some(PAGE_TYPE));
    assert_eq!(right, s.character_options.page.expect("bound"));
    assert_eq!(CHARACTER_PAGE_ELEMENT.0, PAGE_ELEMENT);
}

/// The first show of the character page swallows its own visibility message.
#[test]
fn the_first_show_of_the_character_page_swallows_its_own_visibility_message() {
    let mut ui = env(true);
    let mut s = screen(&mut ui);
    let page = s.character_options.page.expect("bound");
    ui.drain_outbox();

    let edges = |ui: &mut UiSystem, s: &mut GamePlayScreen| -> Vec<bool> {
        pump(ui, s);
        s.take_character_option_visibility()
    };

    ui.set_visible(page, true);
    let first = edges(&mut ui, &mut s);
    assert_eq!(
        first,
        Vec::<bool>::new(),
        "The initial show does not dispatch message 0x18"
    );

    ui.set_visible(page, false);
    assert_eq!(edges(&mut ui, &mut s), vec![false], "the hide is delivered");
    ui.set_visible(page, true);
    assert_eq!(
        edges(&mut ui, &mut s),
        vec![true],
        "every later show is delivered"
    );
}

/// The page opens unticked when the view implements nothing.
#[test]
fn the_page_opens_unticked_when_the_view_implements_nothing() {
    let mut ui = env(true);
    let s = screen(&mut ui);
    assert_eq!(
        s.character_options.values_seen, 50,
        "every row asked the view"
    );
    let ticked = s
        .character_options
        .rows
        .iter()
        .filter(|r| r.current)
        .count();
    assert_eq!(
        ticked, 0,
        "ticked rows at build time, of 50, with a view that implements nothing"
    );
}

mod defaults {
    //! Apply/Cancel/Restore Defaults on the Character Options page are under that page and reach its
    //! three handlers; Restore Defaults writes every row's default; with no default producer it writes
    //! nothing.
    //! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

    use crate::common::layout::RegistrationOrder;
    use std::cell::RefCell;
    use std::rc::Rc;

    use dereth_ui::framework::Screen;
    use dereth_ui::msg::Delivery;
    use dereth_ui::{ElemHandle, ElementId, UiSystem};

    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::{GameView, PlayerOption, UiRequest};

    // -------------------------------------------------------------------------------------------
    // Literals, not symbols
    // -------------------------------------------------------------------------------------------

    /// The option page's three buttons, shared by all three option pages; the
    /// handler compares the clicked id against `0x100001FC`, `0x100001FD` and `0x100001FE`.
    const APPLY: u32 = 0x1000_01FC;
    const CANCEL: u32 = 0x1000_01FD;
    const DEFAULTS: u32 = 0x1000_01FE;

    /// The Character Options page, and the id it shares with the keyboard page's fifth tab.
    const CHARACTER_PAGE: u32 = 0x1000_0211;

    // -------------------------------------------------------------------------------------------
    // The harness
    // -------------------------------------------------------------------------------------------

    fn env() -> UiSystem {
        let (ui, _flow, _store) =
            crate::common::layout::load((800, 600), RegistrationOrder::AfterResolver);
        ui
    }

    fn screen(ui: &mut UiSystem) -> GamePlayScreen {
        let mut s = GamePlayScreen::default();
        s.create(&mut dereth_ui::framework::ScreenCx::new(ui))
            .expect("the gameplay screen builds");
        s
    }

    fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
        for d in ui.drain_outbox() {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }

    #[derive(Debug, Default)]
    struct Module {
        set: Rc<RefCell<Vec<PlayerOption>>>,
        defaults: Option<Rc<RefCell<Vec<PlayerOption>>>>,
    }

    impl GameView for Module {
        fn player_option(&self, o: PlayerOption) -> bool {
            self.set.borrow().contains(&o)
        }
        fn player_option_default(&self, o: PlayerOption) -> Option<bool> {
            self.defaults.as_ref().map(|d| d.borrow().contains(&o))
        }
    }

    /// The three button elements under the Character Options page, found the way the click is
    /// attributed: by walking **down from the bound page handle**, because the id `0x10000211` is in
    /// the shipped tree twice.
    fn buttons(ui: &UiSystem, s: &GamePlayScreen) -> Vec<(ElemHandle, ElementId)> {
        let page = s.character_options.page.expect("the page is bound");
        [APPLY, CANCEL, DEFAULTS]
            .into_iter()
            .map(|id| {
                let h = ui
                    .get_child_recursive(page, ElementId(id))
                    .unwrap_or_else(|| panic!("{id:#x} is not under the character page"));
                (h, ElementId(id))
            })
            .collect()
    }

    /// Press one button the way the manager does: broadcast message 1 from the element itself, then
    /// let the screen's handler and the frame drain run.
    fn press(
        ui: &mut UiSystem,
        s: &mut GamePlayScreen,
        view: &dyn GameView,
        h: ElemHandle,
    ) -> Vec<usize> {
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
        pump(ui, s);
        s.drive_character_options(ui, view)
    }

    // -------------------------------------------------------------------------------------------
    // 1. The three arms
    // -------------------------------------------------------------------------------------------

    /// The three buttons are under the character page and reach its three vcalls.
    #[test]
    fn the_three_buttons_are_under_the_character_page_and_reach_its_three_vcalls() {
        let mut ui = env();
        let mut s = screen(&mut ui);
        assert_eq!(s.character_options.rows.len(), 50);
        let page = s.character_options.page.expect("bound");
        assert_eq!(ui.node(page).expect("live").element_id().0, CHARACTER_PAGE);

        let b = buttons(&ui, &s);
        assert_eq!(b.len(), 3, "0x100001FC/FD/FE are all under this page");

        // The module says two options are on; the page was built before that, so a *show* or an
        // *Apply* is what makes it agree.
        let module = Module {
            set: Rc::new(RefCell::new(vec![
                PlayerOption::ShowHelm,
                PlayerOption::AutoTarget,
            ])),
            defaults: None,
        };
        let _ = ui.drain_outbox();

        // --- Apply:, a re-read and a re-snapshot ------
        let moved = press(&mut ui, &mut s, &module, b[0].0);
        assert_eq!(
            moved,
            vec![2],
            "Apply re-read the module and two rows moved"
        );
        let i = s
            .character_options
            .row_of(PlayerOption::ShowHelm)
            .expect("a row");
        assert!(
            s.character_options.rows[i].current,
            "and the box is ticked now"
        );
        assert!(
            s.character_options.rows[i].saved,
            "…and `saved` moved with it, so Cancel is a no-op"
        );
        assert!(!s.character_options.changed());

        // --- Cancel:, only the rows that changed ------------------
        // Untick one row the way a click does, then cancel.
        ui.requests.clear();
        s.character_options.rows[i].current = false;
        assert!(s.character_options.changed());
        let reverted = press(&mut ui, &mut s, &module, b[1].0);
        assert_eq!(
            reverted,
            vec![1],
            "one row was changed, so one row is written back"
        );
        assert!(s.character_options.rows[i].current, "…to `saved`");
        assert_eq!(
            ui.requests.take(),
            vec![UiRequest::SetPlayerOption(PlayerOption::ShowHelm, true)],
            "restoring the saved value applies, and only for the row that moved"
        );

        // --- Defaults with no producer: 0, and nothing written ------------------------------------
        ui.requests.clear();
        let n = press(&mut ui, &mut s, &module, b[2].0);
        assert_eq!(n, vec![0], "no a default-option producer on this host");
        assert!(ui.requests.take().is_empty(), "and not one option written");

        // A button id this page does not carry is not one of the three.
        assert_ne!(APPLY, CANCEL);
        assert_eq!(dereth_ui_screens::options::config::button::APPLY.0, APPLY);
        assert_eq!(dereth_ui_screens::options::config::button::CANCEL.0, CANCEL);
        assert_eq!(
            dereth_ui_screens::options::config::button::DEFAULTS.0,
            DEFAULTS
        );
    }

    // -------------------------------------------------------------------------------------------
    // 2. Restore Defaults, with a producer
    // -------------------------------------------------------------------------------------------

    /// Behaviour: options.character-page.restore-defaults-writes-every-row-from-its-default
    /// The page's restore-defaults is **unconditional**: every row, changed or not,
    /// has its current value set to its default, is refreshed, and is applied.
    ///
    /// So 50 rows means 50 `SetPlayerOption` requests, each carrying that option's own default and not
    /// a blanket `false`. Three of the defaults are asserted as literals against
    /// the client's own default-option true-list — the same list
    /// `dereth_client_model::player::options::DEFAULT_TRUE_ORDINALS` holds, reached here through the test
    /// double so that this file states the *page's* behaviour rather than the table's.
    #[test]
    fn restore_defaults_writes_every_row_from_get_default_option_value() {
        let mut ui = env();
        let mut s = screen(&mut ui);

        // `ToggleRun` and `ShowHelm` default **on** (`DEFAULT_OPTIONS & 0x0400` and the true-list);
        // `SalvageMultiple` defaults **off**. Everything else off, so the count can be read directly.
        let defaults = Rc::new(RefCell::new(vec![
            PlayerOption::ToggleRun,
            PlayerOption::ShowHelm,
        ]));
        let module = Module {
            set: Rc::new(RefCell::new(Vec::new())),
            defaults: Some(Rc::clone(&defaults)),
        };
        // Re-bind so reads a default-option producer at bind time, which is
        // the only moment it does.
        let page = s.character_options.page.expect("bound");
        s.character_options = dereth_ui_screens::options::character::character_settings_post_init(
            &mut ui, page, &module,
        )
        .expect("the page rebuilds");
        assert_eq!(s.character_options.rows.len(), 50);
        assert_eq!(
            s.character_options.defaults_seen, 50,
            "every row got a default"
        );

        let b = buttons(&ui, &s);
        let _ = ui.drain_outbox();
        ui.requests.clear();
        let n = press(&mut ui, &mut s, &module, b[2].0);
        assert_eq!(n, vec![50], "unconditional: every row is written");

        let reqs = ui.requests.take();
        assert_eq!(
            reqs.len(),
            50,
            "one `SetPlayerOption` per row, never a composed word"
        );
        assert!(reqs.contains(&UiRequest::SetPlayerOption(PlayerOption::ToggleRun, true)));
        assert!(reqs.contains(&UiRequest::SetPlayerOption(PlayerOption::ShowHelm, true)));
        assert!(reqs.contains(&UiRequest::SetPlayerOption(
            PlayerOption::SalvageMultiple,
            false
        )));
        let on = reqs
            .iter()
            .filter(|r| matches!(r, UiRequest::SetPlayerOption(_, true)))
            .count();
        assert_eq!(
            on, 2,
            "exactly the two the true-list names, not 0 and not 50"
        );

        // …and the elements show it: attribute `0x0E` on the check box, which is.
        let i = s
            .character_options
            .row_of(PlayerOption::ToggleRun)
            .expect("a row");
        let h = s.character_options.rows[i].element;
        assert_eq!(dereth_ui_screens::bind::attr_bool(&ui, h, 0x0E), Some(true));
        let j = s
            .character_options
            .row_of(PlayerOption::SalvageMultiple)
            .expect("a row");
        let g = s.character_options.rows[j].element;
        assert_eq!(
            dereth_ui_screens::bind::attr_bool(&ui, g, 0x0E),
            Some(false)
        );
    }

    // -------------------------------------------------------------------------------------------
    // 3. The calibration
    // -------------------------------------------------------------------------------------------

    /// "Defaults is not available on this host" and "Defaults set every option off" must be different
    /// numbers, because the second is what a `bool` seam would silently produce and it looks like a
    /// working button.
    #[test]
    fn with_no_default_producer_restore_defaults_writes_nothing() {
        let mut ui = env();
        let mut s = screen(&mut ui);
        // The trait's own default, which is what a host with no player-options source answers.
        let module = Module {
            set: Rc::new(RefCell::new(Vec::new())),
            defaults: None,
        };
        let page = s.character_options.page.expect("bound");
        s.character_options = dereth_ui_screens::options::character::character_settings_post_init(
            &mut ui, page, &module,
        )
        .expect("the page rebuilds");

        assert_eq!(
            s.character_options.rows.len(),
            50,
            "the rows are still built"
        );
        assert_eq!(
            s.character_options.defaults_seen, 0,
            "and not one of them has a default"
        );
        assert!(s.character_options.rows.iter().all(|r| r.default.is_none()));

        ui.requests.clear();
        assert_eq!(s.character_options.restore_default_values(&mut ui), 0);
        assert!(
            ui.requests.take().is_empty(),
            "a Defaults button with nothing to restore must write nothing, not `false` x 50"
        );
    }
}
