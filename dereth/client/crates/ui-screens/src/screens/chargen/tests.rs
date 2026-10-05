use super::*;

/// Oracle: the screen catalogue's four tables, and the layout-enum map (enum
/// `0x10000039` → `0x21000038` `chargen_master`, root `0x100003CC`).
#[test]
fn the_wizard_names_the_documented_layout_chrome_pages_and_buttons() {
    assert_eq!(LAYOUT, LayoutEnum(0x1000_0039));
    assert_eq!(ROOT, ElementId(0x1000_03CC));
    assert_eq!(CHROME.len(), 9);
    // The six pages: contiguous ids 0x100003D1..0x100003D6 against types 0x10000039..0x1000003E.
    for (i, p) in EcgProgress::PAGES.iter().enumerate() {
        let (id, ty) = p.page().unwrap();
        let i = u32::try_from(i).unwrap();
        assert_eq!(id, ElementId(0x1000_03D1 + i), "{p:?} element id");
        assert_eq!(ty, ElementType(0x1000_0039 + i), "{p:?} element type");
        assert_eq!(p.select_button(), Some(ElementId(0x1000_03EF + i)));
    }
    assert_eq!(EcgProgress::Invalid.page(), None);
    assert_eq!(EcgProgress::Invalid.select_button(), None);
}

/// the values, including the heritage typo the document
/// flags with *(sic)*.
#[test]
fn the_progress_enum_has_the_documented_values() {
    assert_eq!(EcgProgress::Invalid as i32, 0);
    assert_eq!(EcgProgress::Hertage as i32, 1);
    assert_eq!(EcgProgress::Profession as i32, 2);
    assert_eq!(EcgProgress::Skills as i32, 3);
    assert_eq!(EcgProgress::Appearance as i32, 4);
    assert_eq!(EcgProgress::Town as i32, 5);
    assert_eq!(EcgProgress::Summary as i32, 6);
}

/// Oracle: the progress-state write — one page visible, the arrows enabled by position, and the
/// constructor ending on the heritage page.
#[test]
fn walking_all_six_pages_shows_one_at_a_time_and_bounds_the_arrows() {
    let mut ui = UiSystem::new((800, 600));
    let mut s = CharGenScreen::default();
    s.set_progress_state(&mut ui, EcgProgress::Hertage);
    assert_eq!(s.visible_page, Some(ElementId(0x1000_03D1)));
    assert!(!s.left_enabled, "nothing before the first page");
    assert!(s.right_enabled);

    for expected in &EcgProgress::PAGES[1..] {
        s.next_page(&mut ui);
        assert_eq!(s.progress, *expected);
        assert_eq!(s.visible_page, expected.page().map(|(i, _)| i));
    }
    assert_eq!(s.progress, EcgProgress::Summary);
    assert!(s.left_enabled);
    assert!(!s.right_enabled, "nothing after the last page");
    s.next_page(&mut ui);
    assert_eq!(
        s.progress,
        EcgProgress::Summary,
        "the right arrow stops at the summary"
    );

    for expected in EcgProgress::PAGES.iter().rev().skip(1) {
        s.previous_page(&mut ui);
        assert_eq!(s.progress, *expected);
    }
    s.previous_page(&mut ui);
    assert_eq!(
        s.progress,
        EcgProgress::Hertage,
        "the left arrow stops at heritage"
    );
}

/// A stand-in for the two dat tables, with one heritage and two templates.
///
/// The *shape* is the retail one — `attribute_credits` 330 and `skill_credits` 52 are what
/// every non-Olthoi heritage carries — but the numbers here are this test's, so nothing below
/// asserts fidelity against them. The fidelity assertions are in
/// [`dereth_chargen`], against retail's behaviour.
fn tables() -> Rc<CharGenTables> {
    use dereth_assets::tables::{HeritageGroup, SexCg};
    use std::collections::BTreeMap;
    let mut sexes = BTreeMap::new();
    sexes.insert(
        1u32,
        SexCg {
            naming_help: None,
            name: "Female".into(),
            scale: 100,
            setup: dereth_primitives::DataId(0),
            sound_table: dereth_primitives::DataId(0),
            icon: 0,
            base_palette: dereth_primitives::DataId(0),
            skin_palset: dereth_primitives::DataId(0),
            physics_table: dereth_primitives::DataId(0),
            motion_table: dereth_primitives::DataId(0),
            combat_table: dereth_primitives::DataId(0),
            base_objdesc: dereth_assets::tables::ObjDesc {
                version: 11,
                palette: None,
                subpalettes: Vec::new(),
                texture_changes: Vec::new(),
                anim_part_changes: Vec::new(),
            },
            hair_colors: Vec::new(),
            hair_styles: Vec::new(),
            eye_colors: Vec::new(),
            eye_strips: Vec::new(),
            nose_strips: Vec::new(),
            mouth_strips: Vec::new(),
            headgear: Vec::new(),
            shirts: Vec::new(),
            pants: Vec::new(),
            footwear: Vec::new(),
            clothing_colors: Vec::new(),
        },
    );
    let mut heritage_groups = BTreeMap::new();
    for id in [1u32, TOD_HERITAGE, HERITAGE_OLTHOI] {
        heritage_groups.insert(
            id,
            HeritageGroup {
                description: None,
                name: format!("H{id}"),
                icon: 0,
                setup: dereth_primitives::DataId(0),
                environment_setup: dereth_primitives::DataId(0),
                attribute_credits: 330,
                skill_credits: 52,
                primary_start_areas: Vec::new(),
                secondary_start_areas: Vec::new(),
                skills: Vec::new(),
                templates: Vec::new(),
                sex_table_marker: 0,
                sex_order: sexes.keys().copied().collect(),
                template_presentations: BTreeMap::new(),
                sexes: sexes.clone(),
            },
        );
    }
    Rc::new(CharGenTables {
        texts: dereth_chargen::CreationTexts::default(),
        world: Rc::new(dereth_chargen::CreationTables {
            chargen: CharGen {
                help_strings: vec![],
                id: dereth_primitives::DataId(0x0E00_0002),
                second_data_id: dereth_primitives::DataId(0),
                starter_areas: TOWN_NAMES
                    .iter()
                    .map(|name| dereth_assets::tables::StarterArea {
                        name: (*name).to_string(),
                        locations: Vec::new(),
                    })
                    .collect(),
                hg_table_marker: 0,
                heritage_order: heritage_groups.keys().copied().collect(),
                heritage_groups,
            },
            skills: SkillTable {
                id: dereth_primitives::DataId(0x0E00_0004),
                buckets: 0,
                skills: BTreeMap::new(),
            },
            clothing: Rc::new(BTreeMap::new()),
        }),
        // No dat behind this fixture, so the wheel has no art and no colours -- which is
        // exactly the Get returned null case the client also draws as nothing.
        color_wheel_art: ColorWheelArt::default(),
        colors: None,
    })
}

fn wizard() -> CharGenScreen {
    CharGenScreen {
        tables: Some(tables()),
        account_has_tod: true,
        ..CharGenScreen::default()
    }
}

/// Oracle: the client's switch, read case by case. The ids are neither contiguous nor ordered
/// by heritage number, so this is a table copied case by case and not a pattern.
#[test]
fn the_thirteen_heritage_buttons_are_the_switch_cases() {
    let want: [(u32, u32); 13] = [
        (0x1000_03BF, 1),
        (0x1000_03C1, 2),
        (0x1000_03C2, 3),
        (0x1000_03C3, 4),
        (0x1000_0590, 5),
        (0x1000_05A9, 6),
        (0x1000_05E8, 7),
        (0x1000_05F1, 8),
        (0x1000_05C4, 9),
        (0x1000_0591, 10),
        (0x1000_05BF, 11),
        (0x1000_05C7, 12),
        (0x1000_05C8, 13),
    ];
    let got: Vec<(u32, u32)> = HERITAGE_BUTTONS.iter().map(|(e, h)| (e.0, *h)).collect();
    assert_eq!(got, want.to_vec());
    // Every heritage 1..=13 exactly once, which is what makes the table checkable.
    let mut hs: Vec<u32> = HERITAGE_BUTTONS.iter().map(|(_, h)| *h).collect();
    hs.sort_unstable();
    assert_eq!(hs, (1..=13).collect::<Vec<u32>>());
}

/// Oracle: the town write (`start area = town - Holtburg`, and the four town cases highlight
/// Holt, Shoushi, Yaraq, Sanamar in that order) and the switch's literal start area 3 for
/// Sanamar.
#[test]
fn the_four_town_buttons_map_to_the_documented_start_areas() {
    let got: Vec<(u32, u32)> = TOWN_BUTTONS.iter().map(|(e, a)| (e.0, *a)).collect();
    assert_eq!(
        got,
        vec![
            (0x1000_040D, 0),
            (0x1000_040F, 1),
            (0x1000_040E, 2),
            (0x1000_040B, 3)
        ]
    );
    assert_eq!(TOD_START_AREA, 3, "Sanamar is the Throne of Destiny town");
}

/// Oracle: the two expansion-entitlement guards — the heritage page's, on Viamontian
/// only, and the town page's, on Sanamar only. Both raise a warning
/// and leave state unchanged.
#[test]
fn the_expansion_gates_refuse_and_change_nothing() {
    let mut s = CharGenScreen {
        tables: Some(tables()),
        ..CharGenScreen::default()
    };
    assert!(!s.account_has_tod);
    s.choose_heritage(TOD_HERITAGE);
    assert_eq!(s.open_dialog, Some(CharGenDialog::ToDRequired));
    assert_eq!(s.state.heritage_group, 0, "the heritage was not taken");
    s.open_dialog = None;
    s.choose_town(TOD_START_AREA);
    assert_eq!(s.open_dialog, Some(CharGenDialog::ToDRequired));
    assert_eq!(s.state.start_area, 0);
    // The other twelve heritages and the other three towns fall straight through.
    s.open_dialog = None;
    s.choose_heritage(1);
    assert_eq!(s.state.heritage_group, 1);
    assert_eq!(s.state.gender, 1);
    assert_eq!(s.state.total_atrb_credits, 330);
    assert_eq!(s.state.total_skill_credits, 52);
    s.choose_town(2);
    assert_eq!(s.state.start_area, 2);
    assert_eq!(s.open_dialog, None);
    // With the expansion, Viamontian is allowed.
    s.account_has_tod = true;
    s.choose_heritage(TOD_HERITAGE);
    assert_eq!(s.state.heritage_group, TOD_HERITAGE);
}

/// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
#[test]
fn heritage_buttons_refuse_every_key_missing_from_the_world() {
    let mut t = tables();
    let world = Rc::get_mut(&mut Rc::get_mut(&mut t).unwrap().world).unwrap();
    let human = world.chargen.heritage_groups[&1].clone();
    world.chargen.heritage_groups.clear();
    for key in 1..=3 {
        world.chargen.heritage_groups.insert(key, human.clone());
    }
    world.chargen.heritage_order = vec![1, 2, 3];
    let mut s = CharGenScreen {
        tables: Some(t),
        account_has_tod: true,
        ..CharGenScreen::default()
    };
    for available in 1..=3 {
        s.open_dialog = None;
        s.choose_heritage(available);
        assert_eq!(s.state.heritage_group, available);
        assert_eq!(s.open_dialog, None);
        for unavailable in 4..=13 {
            s.choose_heritage(unavailable);
            assert_eq!(s.state.heritage_group, available);
            assert_eq!(s.state.gender, 1);
            assert_eq!(s.open_dialog, Some(CharGenDialog::ToDRequired));
            s.open_dialog = None;
        }
    }
}

/// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
#[test]
fn town_buttons_map_geographic_names_to_the_world_area_indices() {
    let mut t = tables();
    let world = Rc::get_mut(&mut Rc::get_mut(&mut t).unwrap().world).unwrap();
    let names = [
        "Holtburg South",
        "Holtburg West",
        "Shoushi Southeast",
        "Shoushi West",
        "Yaraq North",
        "Yaraq East",
    ];
    world.chargen.starter_areas = names
        .iter()
        .map(|name| dereth_assets::tables::StarterArea {
            name: (*name).to_string(),
            locations: Vec::new(),
        })
        .collect();
    let mut s = CharGenScreen {
        tables: Some(t),
        ..CharGenScreen::default()
    };
    assert!(s.uses_world_town_list());
    for (map, area) in [(0, 0), (1, 2), (2, 4)] {
        s.choose_map_town(map);
        assert_eq!(s.state.start_area, area);
        assert_eq!(s.open_dialog, None);
    }
    for area in 0..6 {
        s.choose_town(area);
        assert_eq!(s.state.start_area, area);
        assert_eq!(
            s.open_dialog, None,
            "Shoushi West does not require expansion access"
        );
    }
    s.choose_map_town(3);
    s.choose_town(6);
    assert_eq!(
        s.state.start_area, 5,
        "absent towns never replace a valid selection"
    );
    assert!(!wizard().uses_world_town_list());
}

/// Oracle: the finish step, line for line — the no-name refusal, the credit warning and the
/// already-sent double-send guard.
#[test]
fn finishing_refuses_without_a_name_warns_about_credits_and_only_sends_once() {
    let mut s = wizard();
    s.choose_heritage(1);
    s.choose_town(0);

    // No name: the dialog, and nothing sent.
    assert!(!s.do_finish(true));
    assert_eq!(s.error_string_id, Some("ID_CharGen_NoNameWarning"));
    assert!(s.take_actions().is_empty());

    // A name, but the default template leaves 30 of the 330 credits unspent.
    s.name_entered = true;
    assert!(s.state.set_name("Kupo"));
    assert!(s.state.remaining_atrb_credits > 0);
    assert!(!s.do_finish(true));
    assert_eq!(s.open_dialog, Some(CharGenDialog::CreditWarning));
    assert!(
        s.take_actions().is_empty(),
        "the warning is asked before anything is sent"
    );

    // Answering "yes" re-enters the finish with the credit check off, which sends.
    assert_eq!(s.close_dialog(true), None);
    let sent = s.take_actions();
    assert_eq!(sent.len(), 1);
    let CharGenAction::SendCharGenResult(r) = &sent[0] else {
        panic!("not a create")
    };
    assert_eq!(r.name, "Kupo");
    assert_eq!(r.heritage_group, 1);
    assert_eq!(r.start_area, 0);
    assert_eq!(r.skill_advancement_classes.len(), 55, "the wire contract");
    assert_eq!(s.state.verification, CgVerification::Pending);

    // Pressing Finish again while the answer is outstanding sends nothing.
    assert!(!s.do_finish(false));
    assert!(s.take_actions().is_empty());
}

/// Oracle: the char-gen verification-response notice's table, and the client's
/// awaiting-character-set branch — "a successful creation logs the new character in without a
/// visit to the character-select screen".
#[test]
fn a_successful_creation_logs_the_new_character_straight_in() {
    use dereth_ui::persist::CharacterIdentity;
    let mut s = wizard();
    s.choose_heritage(1);
    s.name_entered = true;
    s.state.set_name("Kupo");
    assert!(s.do_finish(false));
    s.take_actions();

    // A failure keeps the wizard on the summary page and re-arms Finish.
    s.on_chargen_verification_response(3);
    assert_eq!(s.open_dialog, Some(CharGenDialog::ErrorMessage));
    assert_eq!(s.error_string_id, Some("ID_Character_Err_NameReserved"));
    assert!(!s.awaiting_char_set_for_login);
    assert_eq!(
        s.state.verification,
        CgVerification::Undef,
        "Finish can be pressed again"
    );

    // Success arms the log-in, and the next character set carries it out.
    assert!(s.do_finish(false));
    s.take_actions();
    s.on_chargen_verification_response(1);
    assert!(s.awaiting_char_set_for_login);
    assert_eq!(s.open_dialog, None, "the please-wait dialog is closed");

    let set = CharacterSet {
        set: vec![
            CharacterIdentity {
                id: dereth_primitives::ObjectId(0x5000_0001),
                name: "Someone".into(),
                seconds_grace_period: 0,
            },
            CharacterIdentity {
                id: dereth_primitives::ObjectId(0x5000_0002),
                // ACE plusses every character on a privileged account, so the name that comes
                // back is not the name that was asked for.
                name: "+Kupo".into(),
                seconds_grace_period: 0,
            },
        ],
        num_allowed_characters: 11,
        ..CharacterSet::default()
    };
    assert_eq!(
        s.character_set_arrived(&set),
        None,
        "no mode change: it logs in instead"
    );
    assert_eq!(
        s.take_actions(),
        vec![CharGenAction::LogOn(dereth_primitives::ObjectId(
            0x5000_0002
        ))]
    );

    // And a set that does *not* contain the new name falls back to character select.
    s.awaiting_char_set_for_login = true;
    let empty = CharacterSet {
        set: Vec::new(),
        ..CharacterSet::default()
    };
    assert_eq!(
        s.character_set_arrived(&empty),
        Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT)
    );
    assert!(s.take_actions().is_empty());
}

/// The chrome buttons open the documented dialogs and the strip jumps pages.
#[test]
fn the_chrome_buttons_open_the_documented_dialogs_and_the_strip_jumps_pages() {
    let mut ui = UiSystem::new((800, 600));
    let mut s = CharGenScreen::default();
    s.set_progress_state(&mut ui, EcgProgress::Hertage);
    let msg = |id: u32| ElementMessage {
        source_id: ElementId(id),
        source: ElemHandle::for_test(1),
        id: MessageId(1),
        p1: 0,
        p2: 0,
        point: dereth_ui::msg::MessagePoint::default(),
        serial: 1,
    };
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &msg(0x1000_03CA),
    );
    assert_eq!(s.open_dialog, Some(CharGenDialog::Exit));
    s.open_dialog = None;
    // Off the summary page, Random rerolls the current page without raising a dialog.
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &msg(0x1000_03CB),
    );
    assert_eq!(
        s.open_dialog, None,
        "Random does not warn on the heritage page"
    );
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &msg(0x1000_03F4),
    );
    assert_eq!(
        s.progress,
        EcgProgress::Summary,
        "the strip jumps straight to a page"
    );
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &msg(0x1000_03CB),
    );
    assert_eq!(
        s.open_dialog,
        Some(CharGenDialog::RandomizeWarning),
        "on the summary page Random warns first"
    );
}
