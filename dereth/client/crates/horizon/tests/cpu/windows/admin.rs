//! The Admin window: who has it, what its buttons send, and what it does with the replies.
//!
//! Behaviour: none (this client's own window, pinned as `CD-admin-window`)

use dereth_client_contract::actions::dereth::TOGGLE_ADMIN_WINDOW;
use dereth_client_contract::UiRequest;
use dereth_client_model::cmd::admin::admin_command;
use dereth_horizon::ui::game::{ChatLine, GameState, KeyBindingsView, KeyRow, Relation, Target};
use dereth_horizon::ui::panels::admin::{field, offer_admin_key, TABS};
use dereth_horizon::ui::panels::WindowId;
use dereth_primitives::ObjectId;

use crate::Harness;

fn player(is_admin: bool) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        is_admin,
        ..GameState::default()
    }
}

fn with_target(mut state: GameState) -> GameState {
    state.target = Some(Target {
        id: ObjectId(0x8000_1A2C),
        name: "Drudge Slinker".into(),
        relation: Relation::Hostile,
        ..Target::default()
    });
    state
}

/// The interface with the Admin window open on `tab`, drawn once.
fn opened(state: &GameState, tab: usize) -> Harness {
    let mut h = Harness::with_built_in_art();
    h.frame(state);
    h.ui.windows.open(WindowId::Admin, 0.0);
    h.ui.windows.admin.tab = tab;
    h.frame(state);
    h
}

/// A press and release of the button that sends `line`.
fn press(h: &mut Harness, state: &GameState, line: &str) -> Vec<UiRequest> {
    h.frame(state);
    let offered = &h.ui.windows.admin.offered;
    let r = offered
        .iter()
        .find(|(l, _)| l == line)
        .unwrap_or_else(|| panic!("no button sends {line:?}: {offered:?}"))
        .1;
    h.move_to(r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.press();
    let mut out = h.frame(state).requests;
    h.release();
    out.extend(h.frame(state).requests);
    out
}

fn said(requests: &[UiRequest]) -> Vec<&str> {
    requests
        .iter()
        .filter_map(|r| match r {
            UiRequest::ChatLineOnBehalf { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn line(text: &str) -> ChatLine {
    ChatLine {
        chat_type: 0,
        text: text.into(),
        window: 0,
    }
}

/// Behaviour: presentation.admin-window.only-an-administrator-has-it
#[test]
fn a_player_can_never_have_the_window() {
    let state = player(false);
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    assert!(!h.ui.windows.offers(WindowId::Admin));

    h.input.actions.push(TOGGLE_ADMIN_WINDOW.0);
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Admin), "by its key");

    h.ui.windows.open(WindowId::Admin, 0.0);
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Admin), "opened in any way");

    h.ui.windows.open_by_name("admin");
    h.frame(&state);
    assert!(!h.ui.windows.is_open(WindowId::Admin), "by name");
    assert!(h.ui.windows.admin.offered.is_empty(), "nothing was drawn");
}

#[test]
fn an_administrator_has_it_by_its_key_and_loses_it_with_the_flag() {
    let admin = player(true);
    let mut h = Harness::new(Default::default());
    h.frame(&admin);
    assert!(h.ui.windows.offers(WindowId::Admin));
    h.input.actions.push(TOGGLE_ADMIN_WINDOW.0);
    h.frame(&admin);
    assert!(h.ui.windows.is_open(WindowId::Admin));

    h.frame(&player(false));
    assert!(!h.ui.windows.is_open(WindowId::Admin));
    assert!(!h.ui.windows.offers(WindowId::Admin));
}

#[test]
fn asked_for_by_name_before_the_flag_comes_it_opens_when_the_flag_does() {
    let mut h = Harness::new(Default::default());
    h.ui.windows.open_by_name("admin");
    h.frame(&player(false));
    assert!(!h.ui.windows.is_open(WindowId::Admin));
    h.frame(&player(true));
    assert!(h.ui.windows.is_open(WindowId::Admin));
}

#[test]
fn the_key_page_shows_its_key_only_to_an_administrator() {
    let view = || KeyBindingsView {
        rows: vec![
            KeyRow {
                map: 0,
                action: TOGGLE_ADMIN_WINDOW.0,
                group: 0,
                caption: "Admin Window".into(),
                keys: vec![],
                changed: false,
            },
            KeyRow {
                map: 0,
                action: 1,
                group: 0,
                caption: "Other".into(),
                keys: vec![],
                changed: false,
            },
        ],
        capture: None,
        refused: None,
    };
    let mut keys = view();
    offer_admin_key(&mut keys, true);
    assert_eq!(keys.rows.len(), 2);
    let mut keys = view();
    offer_admin_key(&mut keys, false);
    assert_eq!(keys.rows.len(), 1);
    assert_eq!(keys.rows[0].action, 1);
}

#[test]
fn every_button_on_every_tab_sends_a_command_the_table_knows() {
    let state = with_target(player(true));
    let filled = [
        field::NAME,
        field::AMOUNT,
        field::BOOT_REASON,
        field::ACCOUNT,
        field::DAYS,
        field::HOURS,
        field::MINUTES,
        field::BAN_REASON,
        field::NEW_NAME,
        field::COORDS,
        field::POI,
        field::DUNGEON,
        field::LOCATION,
        field::PROPERTY,
        field::PROPERTY_VALUE,
        field::ADJUST_NAME,
        field::ADJUST_BY,
        field::FACTION_RANK,
        field::LEVEL,
        field::WEENIE,
        field::SPAWN_AMOUNT,
        field::PALETTE,
        field::SHADE,
        field::CUSTOM_NAME,
        field::TIER,
        field::LOOT_ITEMS,
        field::MATERIAL,
        field::STRUCTURE,
        field::WORKMANSHIP,
        field::MESSAGE,
        field::SHUTDOWN_MESSAGE,
        field::ENVIRONMENT,
        field::EVENT,
        field::SERVER_PROPERTY,
        field::SERVER_VALUE,
    ];
    for (tab, name) in TABS.iter().enumerate().take(TABS.len() - 1) {
        let mut h = opened(&state, tab);
        for id in filled {
            h.ui.windows.admin.set_field(id, "1");
        }
        h.frame(&state);
        let offered = &h.ui.windows.admin.offered;
        assert!(offered.len() > 3, "{} offers {offered:?}", name);
        for (lines, r) in offered {
            for sent in lines.lines() {
                let verb = sent
                    .strip_prefix('@')
                    .and_then(|l| l.split(' ').next())
                    .expect("a command");
                // The client answers `@loc` itself.
                assert!(
                    verb == "loc" || admin_command(verb).is_some(),
                    "{sent} is not in the table"
                );
            }
            // Inside the window and clear of the results.
            let ((w, height), (x, y)) = WindowId::Admin.geometry();
            assert!(r.x >= x && r.right() <= x + w, "{lines} runs off: {r:?}");
            assert!(
                r.y >= y && r.bottom() <= y + height - 100.0,
                "{lines} on {} runs into the results: {r:?}",
                name
            );
        }
    }
}

#[test]
fn nothing_on_a_tab_is_drawn_over_anything_else_at_any_size() {
    let state = with_target(player(true));
    for screen in [(1920.0, 1080.0), (800.0, 600.0)] {
        for (tab, name) in TABS.iter().enumerate() {
            let mut h = Harness::with_built_in_art();
            h.screen = screen;
            h.frame(&state);
            h.ui.windows.open(WindowId::Admin, 0.0);
            h.ui.windows.admin.tab = tab;
            h.frame(&state);
            let laid_out = &h.ui.windows.admin.laid_out;
            assert!(!laid_out.is_empty(), "{name}");
            for (n, a) in laid_out.iter().enumerate() {
                for b in &laid_out[n + 1..] {
                    let apart = a.right() <= b.x
                        || b.right() <= a.x
                        || a.bottom() <= b.y
                        || b.bottom() <= a.y;
                    assert!(apart, "{name} at {screen:?}: {a:?} and {b:?} overlap");
                }
            }
        }
    }
}

#[test]
fn a_button_sends_its_line_on_the_player_s_behalf() {
    let state = player(true);
    let mut h = opened(&state, 3);
    let out = press(&mut h, &state, "@cloak on");
    assert_eq!(said(&out), ["@cloak on"]);
    assert!(!out.iter().any(|r| matches!(r, UiRequest::Appraise(_))));
}

#[test]
fn a_form_builds_what_the_server_reads() {
    let state = player(true);
    let mut h = opened(&state, 0);
    let admin = |h: &mut Harness, id, text: &str| h.ui.windows.admin.set_field(id, text);
    admin(&mut h, field::NAME, "Foo Bar");
    admin(&mut h, field::AMOUNT, "1000");
    h.frame(&state);
    for want in [
        "@teleto Foo Bar",
        "@buff Foo Bar 8",
        "@grantxp Foo Bar 1000",
        "@boot char Foo Bar",
        "@set-characteraccess \"Foo Bar\" Sentinel",
    ] {
        assert!(
            h.ui.windows.admin.offered.iter().any(|(l, _)| l == want),
            "{want}: {:?}",
            h.ui.windows.admin.offered
        );
    }
    admin(&mut h, field::BOOT_REASON, "spam");
    admin(&mut h, field::ACCOUNT, "foo acct");
    admin(&mut h, field::HOURS, "2");
    admin(&mut h, field::BAN_REASON, "too much spam");
    admin(&mut h, field::NEW_NAME, "New Name");
    h.frame(&state);
    let offered: Vec<&str> =
        h.ui.windows
            .admin
            .offered
            .iter()
            .map(|(l, _)| l.as_str())
            .collect();
    for want in [
        "@boot char Foo Bar, spam",
        "@ban \"foo acct\" 0 2 0 too much spam",
        "@unban \"foo acct\"",
        "@rename Foo Bar, New Name",
    ] {
        assert!(offered.contains(&want), "{want}: {offered:?}");
    }
}

#[test]
fn a_ban_with_no_length_is_not_offered() {
    let state = player(true);
    let mut h = opened(&state, 0);
    h.ui.windows.admin.set_field(field::ACCOUNT, "acct");
    h.frame(&state);
    let banning = |h: &Harness| {
        h.ui.windows
            .admin
            .offered
            .iter()
            .any(|(l, _)| l.starts_with("@ban "))
    };
    assert!(!banning(&h));
    h.ui.windows.admin.set_field(field::DAYS, "x");
    h.frame(&state);
    assert!(!banning(&h));
    h.ui.windows.admin.set_field(field::DAYS, "1");
    h.frame(&state);
    assert!(banning(&h));
}

#[test]
fn a_dangerous_button_takes_a_second_press_and_forgets_the_first() {
    let state = player(true);
    let mut h = opened(&state, 5);
    let close = "@world close";
    assert!(said(&press(&mut h, &state, close)).is_empty(), "armed");
    assert_eq!(said(&press(&mut h, &state, close)), [close]);

    // Five seconds on, the first press has lapsed.
    assert!(said(&press(&mut h, &state, close)).is_empty(), "armed");
    for _ in 0..300 {
        h.frame(&state);
    }
    assert!(said(&press(&mut h, &state, close)).is_empty(), "armed anew");
}

#[test]
fn a_shutdown_always_sets_its_delay_first() {
    let state = player(true);
    let mut h = opened(&state, 5);
    h.ui.windows
        .admin
        .set_field(field::SHUTDOWN_MESSAGE, "Back soon");
    let both = "@set-shutdown-interval 300\n@shutdown Back soon";
    assert!(said(&press(&mut h, &state, both)).is_empty(), "armed");
    assert_eq!(
        said(&press(&mut h, &state, both)),
        ["@set-shutdown-interval 300", "@shutdown Back soon"]
    );
}

#[test]
fn a_command_on_the_selection_has_it_appraised_afresh_first() {
    let state = with_target(player(true));
    let mut h = opened(&state, 2);
    press(&mut h, &state, "@delete");
    let out = press(&mut h, &state, "@delete");
    let at = |want: &UiRequest| out.iter().position(|r| r == want).expect("sent");
    let cancel = at(&UiRequest::CancelAppraisal);
    let appraise = at(&UiRequest::Appraise(ObjectId(0x8000_1A2C)));
    let delete = at(&UiRequest::ChatLineOnBehalf {
        text: "@delete".into(),
        window: 0,
    });
    assert!(cancel < appraise && appraise < delete, "{out:?}");
}

#[test]
fn with_nothing_selected_the_target_tab_offers_nothing_that_needs_it() {
    let state = player(true);
    let h = opened(&state, 2);
    assert!(
        h.ui.windows.admin.offered.is_empty(),
        "{:?}",
        h.ui.windows.admin.offered
    );
}

#[test]
fn every_command_can_be_found_and_one_that_needs_a_selection_waits_for_it() {
    let state = player(true);
    let mut h = opened(&state, 6);
    h.ui.windows.admin.set_field(field::SEARCH, "@delete");
    h.frame(&state);
    h.ui.windows
        .admin
        .set_field(field::SEARCH, "the selected object");
    h.frame(&state);
    // The first row of the list, under the search and clear of its scroll bar.
    let ((_, _), (x, y)) = WindowId::Admin.geometry();
    h.move_to(x + 100.0, y + 120.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    assert!(
        h.ui.windows.admin.offered.is_empty(),
        "no selection, no bare command: {:?}",
        h.ui.windows.admin.offered
    );
    let with = with_target(state);
    h.frame(&with);
    assert!(
        !h.ui.windows.admin.offered.is_empty(),
        "with a selection it can run"
    );
}

#[test]
fn the_results_show_a_reply_but_not_a_line_long_after() {
    let mut state = player(true);
    let mut h = opened(&state, 3);
    press(&mut h, &state, "@god");
    state.new_chat = vec![line("You are now a god.")];
    h.frame(&state);
    state.new_chat = vec![];
    assert_eq!(h.ui.windows.admin.replies(), ["You are now a god."]);
    for _ in 0..240 {
        h.frame(&state);
    }
    state.new_chat = vec![line("Much later.")];
    h.frame(&state);
    assert_eq!(h.ui.windows.admin.replies(), ["You are now a god."]);
}

#[test]
fn someone_talking_is_not_a_reply() {
    let mut state = player(true);
    let mut h = opened(&state, 3);
    press(&mut h, &state, "@god");
    state.new_chat = vec![ChatLine {
        chat_type: 2,
        text: "Bob says, \"hello\"".into(),
        window: 0,
    }];
    h.frame(&state);
    assert!(h.ui.windows.admin.replies().is_empty());
}

#[test]
fn the_online_list_comes_from_the_server_s_answer() {
    let mut state = player(true);
    let mut h = opened(&state, 0);
    h.frame(&state);
    state.new_chat = vec![line(
        "Foo Bar : 12\n+Admin : 3\nTotal connected Players: 2\n",
    )];
    h.frame(&state);
    assert_eq!(h.ui.windows.admin.online(), ["Foo Bar", "+Admin"]);
}

#[test]
fn asking_for_where_the_player_is_fills_the_location_box() {
    let mut state = player(true);
    let mut h = opened(&state, 1);
    assert_eq!(said(&press(&mut h, &state, "@loc")), ["@loc"]);
    state.new_chat = vec![line(
        "Your location is: 0x7F0401AD [12.3 -28.4 0.0] 1 0 0 0",
    )];
    h.frame(&state);
    state.new_chat = vec![];
    assert_eq!(
        h.ui.windows.admin.field(field::LOCATION),
        "0x7F0401AD [12.3 -28.4 0.0] 1 0 0 0"
    );
    let out = press(
        &mut h,
        &state,
        "@teleloc 0x7F0401AD [12.3 -28.4 0.0] 1 0 0 0",
    );
    assert_eq!(said(&out).len(), 1);
}

/// The map window open, drawn once, and the point in the middle of its picture.
fn map_open(state: &GameState) -> (Harness, (f32, f32)) {
    let mut h = Harness::with_built_in_art();
    h.frame(state);
    h.ui.windows.open(WindowId::Map, 0.0);
    h.frame(state);
    let r = h.ui.windows.map_picture.expect("the map's picture");
    (h, (r.x + r.w / 2.0, r.y + r.h / 2.0))
}

fn click_at(h: &mut Harness, state: &GameState, at: (f32, f32)) -> Vec<UiRequest> {
    h.move_to(at.0, at.1);
    h.frame(state);
    h.press();
    let mut out = h.frame(state).requests;
    h.release();
    out.extend(h.frame(state).requests);
    out
}

#[test]
fn a_player_s_map_has_no_teleport_and_a_press_on_it_sends_nothing() {
    let state = player(false);
    let (mut h, middle) = map_open(&state);
    assert_eq!(h.ui.windows.admin.map_button, None);
    assert!(said(&click_at(&mut h, &state, middle)).is_empty());
}

#[test]
fn an_administrator_s_map_teleports_once_after_its_button_and_not_before() {
    let state = player(true);
    let (mut h, middle) = map_open(&state);
    assert!(
        said(&click_at(&mut h, &state, middle)).is_empty(),
        "not armed"
    );

    let b = h.ui.windows.admin.map_button.expect("the button");
    click_at(&mut h, &state, (b.x + b.w / 2.0, b.y + b.h / 2.0));
    assert!(h.ui.windows.admin.map_armed);

    let sent = click_at(&mut h, &state, middle);
    let lines = said(&sent);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("@tele "), "{lines:?}");
    assert!(!h.ui.windows.admin.map_armed, "one press, one teleport");
    assert!(said(&click_at(&mut h, &state, middle)).is_empty());
}

#[test]
fn closing_the_map_lets_go_of_its_teleport() {
    let state = player(true);
    let (mut h, _) = map_open(&state);
    let b = h.ui.windows.admin.map_button.expect("the button");
    click_at(&mut h, &state, (b.x + b.w / 2.0, b.y + b.h / 2.0));
    assert!(h.ui.windows.admin.map_armed);
    h.ui.windows.close(WindowId::Map);
    h.frame(&state);
    assert!(!h.ui.windows.admin.map_armed);
}
