use super::*;

/// layout
/// `0x21000005` (enum `0x10000006`) is the only shipped layout whose root
/// element is `0x10000495`, which pins the otherwise omitted enum.
#[test]
fn the_screen_names_the_shipped_layout_enum_and_root() {
    assert_eq!(LAYOUT, LayoutEnum(0x1000_0006));
    assert_eq!(ROOT, ElementId(0x1000_0495));
    assert_eq!(CAMERA_SCALE, 1.1);
}

/// the id test that decides which visibility changes are
/// reported to the server.
#[test]
fn only_the_four_floaty_chat_windows_report_their_visibility() {
    assert!(reports_panel_visibility(ElementId(0x1000_0505)));
    for id in 0x1000_050E..=0x1000_0510 {
        assert!(reports_panel_visibility(ElementId(id)), "{id:#X}");
    }
    assert!(!reports_panel_visibility(ElementId(0x1000_050D)));
    assert!(!reports_panel_visibility(ElementId(0x1000_0511)));
    assert!(
        !reports_panel_visibility(ElementId(0x1000_0601)),
        "the main chat window does not"
    );
}

/// Oracle: the key press handling's four-row table.
#[test]
fn the_four_hot_keys_set_the_documented_flags() {
    let mut ui = UiSystem::new((800, 600));
    let mut s = GamePlayScreen {
        shown: true,
        ..Default::default()
    };

    s.handle_key_press(&mut ui, action::LOGOUT_TO_SELECT);
    assert!(!s.should_quit_on_logout && s.logout_confirmed && s.do_end_session);
    let _ = ui.requests.take();
    assert_eq!(
        s.update(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            LocalTime(0.0)
        ),
        None,
        "the non-quit branch queues no mode"
    );
    assert!(
        ui.requests
            .take()
            .contains(&UiRequest::EndCharacterSession { ask: false }),
        "it asks the host to log the character off instead"
    );
    assert!(!s.do_end_session, "and takes the request once");

    let mut s = GamePlayScreen {
        shown: true,
        ..Default::default()
    };
    s.handle_key_press(&mut ui, action::LOGOUT_AND_QUIT);
    assert!(s.should_quit_on_logout);
    assert_eq!(
        s.update(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            LocalTime(0.0)
        ),
        Some(dereth_ui::framework::mode::EPILOGUE)
    );
    assert_eq!(
        s.update(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            LocalTime(0.0)
        ),
        None,
        "the exit is queued once"
    );

    // Toggling UI visibility flips a flag, and an unknown action does nothing.
    let mut s = GamePlayScreen {
        shown: true,
        ..Default::default()
    };
    s.handle_key_press(&mut ui, action::TOGGLE_UI);
    assert!(!s.shown);
    s.handle_key_press(&mut ui, action::TOGGLE_UI);
    assert!(s.shown);
    s.handle_key_press(&mut ui, 0xDEAD);
    assert!(s.shown && !s.do_end_session);
}

/// A press off the ground is refused out loud once and thrown away.
#[test]
fn a_press_off_the_ground_is_refused_out_loud_once_and_thrown_away() {
    let mut ui = UiSystem::new((800, 600));
    let mut s = GamePlayScreen {
        shown: true,
        player_airborne: true,
        ..Default::default()
    };
    s.handle_key_press(&mut ui, action::LOGOUT_TO_SELECT);
    assert!(s.do_end_session);

    let _ = ui.requests.take();
    assert_eq!(
        s.update(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            LocalTime(0.0)
        ),
        None,
        "no mode is ever queued here"
    );
    assert_eq!(s.logoff_refusals, 1);
    assert!(
        !s.do_end_session,
        "the refusal path consumes the pending press"
    );
    assert!(
        !s.ending_session,
        "the refusal path unlatches the guard for a later press"
    );
    let said = ui.requests.take();
    assert!(
        said.contains(&UiRequest::DisplayChatText {
            feedback: dereth_client_contract::feedback::Feedback::LOCAL,
            channel: 0x1A,
            text: "Cannot log off while in mid-air.".to_owned(),
        }),
        "the player is told why: {said:?}"
    );
    assert!(
        !said
            .iter()
            .any(|r| matches!(r, UiRequest::EndCharacterSession { .. })),
        "and nothing was asked of the host: {said:?}"
    );

    // Another look, still airborne, with no fresh press: `do_end_session` is clear, so
    // `UseTime` returns and there is no second line.
    assert_eq!(
        s.update(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            LocalTime(0.0)
        ),
        None
    );
    assert_eq!(s.logoff_refusals, 1, "one press, one refusal");
    assert!(ui.requests.take().is_empty(), "and no second line");

    // He lands. Nothing was deferred, so nothing happens until he presses again.
    s.player_airborne = false;
    assert_eq!(
        s.update(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            LocalTime(0.0)
        ),
        None
    );
    assert!(
        ui.requests.take().is_empty(),
        "landing alone logs nobody off"
    );

    // The second press, on the ground, goes through.
    s.handle_key_press(&mut ui, action::LOGOUT_TO_SELECT);
    assert_eq!(
        s.update(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            LocalTime(0.0)
        ),
        None
    );
    assert!(
        ui.requests
            .take()
            .contains(&UiRequest::EndCharacterSession { ask: false }),
        "the guard was un-latched, so the next press is heard"
    );
    assert_eq!(s.logoff_refusals, 1);
}

/// Exit game is not gated by the ground and says nothing.
#[test]
fn exit_game_is_not_gated_by_the_ground_and_says_nothing() {
    let mut ui = UiSystem::new((800, 600));
    let mut s = GamePlayScreen {
        shown: true,
        player_airborne: true,
        ..Default::default()
    };
    s.handle_key_press(&mut ui, action::LOGOUT_AND_QUIT);
    let _ = ui.requests.take();
    assert_eq!(
        s.update(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            LocalTime(0.0)
        ),
        Some(dereth_ui::framework::mode::EPILOGUE),
        "exit mode is reached even while the body is in mid-air"
    );
    assert_eq!(s.logoff_refusals, 0);
    assert!(ui.requests.take().is_empty(), "and no refusal line");
}

/// a floaty chat window's visibility change becomes a
/// set-panel-visibility notice carrying the *element id* as the panel id.
#[test]
fn a_floaty_chat_visibility_change_is_reported_with_its_element_id() {
    let mut ui = UiSystem::new((800, 600));
    let mut s = GamePlayScreen::default();
    let msg = |id: u32, visible: u32| ElementMessage {
        source_id: ElementId(id),
        source: ElemHandle::for_test(1),
        id: dereth_ui::msg::element::id::VISIBILITY_CHANGED,
        p1: visible,
        p2: 0,
        point: dereth_ui::msg::MessagePoint::default(),
        serial: 1,
    };
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &msg(0x1000_050F, 1),
    );
    s.on_element_message(
        &mut dereth_ui::framework::ScreenCx::new(&mut ui),
        &msg(0x1000_0601, 1),
    );
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::SetPanelVisibility {
            panel: 0x1000_050F,
            visible: true
        }]
    );
}

/// Every start visibility row names the mechanism that puts it there.
#[test]
fn every_start_visibility_row_names_the_mechanism_that_puts_it_there() {
    let rows = GamePlayScreen::HUD_START_VISIBILITY;
    assert_eq!(rows.len(), 17);

    // No element appears twice: two rows disagreeing would make the order load-bearing.
    let mut ids: Vec<u32> = rows.iter().map(|r| r.element.0).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), before, "an element is listed twice");

    let from_the_layout = [
        window::MAIN_CHAT,
        ElementId(0x1000_0505),
        ElementId(0x1000_050E),
        ElementId(0x1000_050F),
        ElementId(0x1000_0510),
        window::KEYBOARD,
        window::ADMIN,
        window::ENV_PANEL,
    ];
    for r in rows {
        assert!(!r.why.is_empty());
        assert!(
            !r.why.contains("#223"),
            "{:#010X}: {:?}",
            r.element.0,
            r.why
        );
        assert!(
            !r.why.contains("UNVERIFIED"),
            "{:#010X}: {:?}",
            r.element.0,
            r.why
        );
        assert_eq!(
            r.why.starts_with("layout:"),
            from_the_layout.contains(&r.element),
            "{:#010X}: {:?}",
            r.element.0,
            r.why
        );
    }

    // The six windows the retail session shows are the six not switched off here: the table
    // turns eleven things off and two on, and the four it never mentions — `<SBOX>`,
    // `<TBAR>`, `<INDI>`, `<RADA>` — are the ones the layout already has up.
    let off: Vec<u32> = rows
        .iter()
        .filter(|r| !r.visible)
        .map(|r| r.element.0)
        .collect();
    assert_eq!(off.len(), 15);
    for id in [
        window::SMART_BOX,
        window::TOOLBAR,
        window::RADAR,
        ElementId(0x1000_0611),
    ] {
        assert!(!off.contains(&id.0), "{:#010X} must stay visible", id.0);
    }
    assert!(rows
        .iter()
        .any(|r| r.element == window::MAIN_CHAT && r.visible));
    assert!(rows
        .iter()
        .any(|r| r.element == window::STACKED_VITALS && r.visible));
}

/// Setting up a page group hides every page and the stack with them.
#[test]
fn setting_up_a_page_group_hides_every_page_and_the_stack_with_them() {
    let mut ui = UiSystem::new((800, 600));
    let stack = ui.create_hollow(None);
    let pages: Vec<ElemHandle> = crate::panels::catalogue::PANEL_PAGES
        .iter()
        .map(|_| ui.create_hollow(Some(stack)))
        .collect();
    for h in &pages {
        assert!(
            ui.node(*h).unwrap().region.flags.visible,
            "a fresh element starts visible"
        );
    }

    // The synthetic pages carry none of the documented ids, so bind them by hand and then run
    // only the part under test: the hide loop and the stack's own visibility.
    let mut s = PanelStack {
        pages: pages
            .iter()
            .enumerate()
            .map(|(i, h)| crate::panels::panel_stack::PageInfo {
                element: ElementId(crate::panels::catalogue::PANEL_PAGES[i]),
                handle: *h,
                panel_id: u32::try_from(i).unwrap() + 1,
                transient: false,
            })
            .collect(),
        ..PanelStack::default()
    };
    for p in &s.pages {
        ui.set_visible(p.handle, false);
    }
    s.root = Some(stack);
    ui.set_visible(stack, s.current.is_some());

    assert!(
        !ui.node(stack).unwrap().region.flags.visible,
        "no page current, no stack"
    );
    for h in &pages {
        assert!(!ui.node(*h).unwrap().region.flags.visible);
    }

    // Opening one page shows that page and the stack, and nothing else.
    s.recv_set_panel_visibility(&mut ui, 3, true);
    assert!(ui.node(stack).unwrap().region.flags.visible);
    for (i, h) in pages.iter().enumerate() {
        assert_eq!(
            ui.node(*h).unwrap().region.flags.visible,
            i == 2,
            "page {i}"
        );
    }
    // Closing it puts both back.
    s.recv_set_panel_visibility(&mut ui, 3, false);
    assert!(!ui.node(stack).unwrap().region.flags.visible);
    assert!(pages
        .iter()
        .all(|h| !ui.node(*h).unwrap().region.flags.visible));
}

/// Oracle: the floaty vitals panel's update from the player module (`SetVisible(!sideBySide)`)
/// and the floaty side vitals panel's update from the player module (`SetVisible(sideBySide)`),
/// and the HUD behavior for the placement blob: **visibility is read unconditionally,
/// position and size only when the layout did not come from file**.
#[test]
fn the_player_module_decides_visibility_always_and_position_only_without_a_layout_file() {
    use crate::hud::floaty::WindowPlacement;

    // Two windows: one that reads `0x1000008A` (the toolbar) and one that does not (the
    // examination panel), so the class test is exercised in both directions.
    let mut placements = WindowPlacements::default();
    placements.set(
        10,
        WindowPlacement {
            x: Some(11),
            y: Some(22),
            w: Some(120),
            h: Some(60),
            visible: Some(false),
            title: None,
        },
    );
    placements.set(
        7,
        WindowPlacement {
            visible: Some(false),
            ..WindowPlacement::default()
        },
    );

    let mut ui = UiSystem::new((800, 600));
    let mut s = GamePlayScreen::default();
    let root = ui.create_hollow(None);
    s.roots.push(root);
    let tbar = ui.create_hollow(Some(root));
    let exam = ui.create_hollow(Some(root));
    // The recursive child lookup matches on the element id, so the synthetic children have to
    // carry the real ones.
    ui.node_mut(tbar).unwrap().desc.element_id = window::TOOLBAR;
    ui.node_mut(exam).unwrap().desc.element_id = window::EXAMINATION;
    s.window_ids = vec![(window::TOOLBAR, 10), (window::EXAMINATION, 7)];

    let pm = PlayerSettingsView {
        placements,
        side_by_side_vitals: false,
        lock_ui: false,
        ..PlayerSettingsView::default()
    };
    assert_eq!(
        s.update_from_player_module(&mut ui, &pm),
        1,
        "only the toolbar reads 0x1000008A"
    );
    assert!(!ui.node(tbar).unwrap().region.flags.visible);
    assert!(
        ui.node(exam).unwrap().region.flags.visible,
        "the examination window never reads placement visibility"
    );
    // With no layout file the rectangle is applied, resize before move.
    let b = ui.node(tbar).unwrap().region.box_;
    assert_eq!((b.x0, b.y0, b.width(), b.height()), (11, 22, 120, 60));

    // With one, the server's rectangle is ignored and the visibility is not.
    let mut s2 = GamePlayScreen {
        layout_from_file: true,
        ..GamePlayScreen::default()
    };
    s2.roots.push(root);
    s2.window_ids = s.window_ids.clone();
    ui.move_to(tbar, 500, 500);
    ui.resize_to(tbar, 40, 40);
    ui.set_visible(tbar, true);
    assert_eq!(s2.update_from_player_module(&mut ui, &pm), 1);
    assert!(
        !ui.node(tbar).unwrap().region.flags.visible,
        "visibility still applies"
    );
    let b = ui.node(tbar).unwrap().region.box_;
    assert_eq!(
        (b.x0, b.y0),
        (500, 500),
        "a local layout file wins over the server rectangle"
    );
}

/// Oracle: the HUD behavior's table and `hud::floaty::READS_PLACEMENT_VISIBILITY`,
/// which was read from all ten floating-window player-state update bodies: exactly five
/// mention `0x1000008A`.
#[test]
fn exactly_five_window_classes_take_their_visibility_from_the_server() {
    let five = crate::hud::floaty::READS_PLACEMENT_VISIBILITY;
    assert_eq!(five.len(), 5);
    let named: Vec<&str> = GAMEPLAY_WINDOWS
        .iter()
        .filter(|w| five.contains(&w.class))
        .map(|w| w.class)
        .collect();
    // `<CHAT>` plus the four `<FCHn>` plus `<INDI>`, `<PBAR>` and `<TBAR>` — eight windows
    // across five classes.
    assert_eq!(named.len(), 8);
    for c in five {
        assert!(
            GAMEPLAY_WINDOWS.iter().any(|w| w.class == c),
            "{c} names no window"
        );
    }
    // And the three that call `SetVisible` from somewhere else are not in it.
    for c in [
        "FloatingExamination",
        "FloatingEnvironmentStack",
        "FloatingCombatStack",
    ] {
        assert!(
            !five.contains(&c),
            "{c} does not read Option_Placement_Visibility"
        );
    }
}

/// "`windowId != 0` targets one window;
/// `windowId == 0` broadcasts subject to the per-window 64-bit filter", and the description's default
/// filters, which are what make the main window reject the over-head-bubble type `0x1A`.
#[test]
fn a_broadcast_line_reaches_the_windows_whose_filter_accepts_its_type() {
    use crate::chat::interface::window as w;
    let mut ui = UiSystem::new((800, 600));
    let mut s = GamePlayScreen {
        chat: vec![
            ChatInterface::new(w::MAIN),
            ChatInterface::new(w::FLOATY_1),
            ChatInterface::new(w::FLOATY_2),
            ChatInterface::new(w::FLOATY_3),
            ChatInterface::new(w::FLOATY_4),
        ],
        chat_windows: vec![crate::chat::window::ChatWindow::default(); 5],
        ..GamePlayScreen::default()
    };

    // A system line broadcasts and the main window takes it.
    let sys = ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        // `LogTextType` 5 = System (the chat and social behavior).
        ty: 5,
        body: "Welcome to Asheron's Call".into(),
        prefix: None,
        window: 0,
    };
    let took = s.recv_display_final_string_info(&mut ui, &sys);
    assert!(took.contains(&w::MAIN), "the main window takes System");
    assert_eq!(s.chat[0].log_text(), "Welcome to Asheron's Call");

    // Type 0x1A is the over-head bubble channel and the main window's default filter masks it
    // out (`0xFBFFFFFF`).
    let bubble = ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        ty: 0x1A,
        body: "hi".into(),
        prefix: None,
        window: 0,
    };
    assert!(!s
        .recv_display_final_string_info(&mut ui, &bubble)
        .contains(&w::MAIN));

    // A line addressed to floaty 2 goes only there, filter or no filter.
    let direct = ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        ty: 0x1A,
        body: "tell".into(),
        prefix: None,
        window: w::FLOATY_2,
    };
    assert_eq!(
        s.recv_display_final_string_info(&mut ui, &direct),
        vec![w::FLOATY_2]
    );
}
