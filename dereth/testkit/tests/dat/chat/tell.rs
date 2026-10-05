use super::*;
// ---------------------------------------------------------------------------------------------
// Starting a private message to somebody
// ---------------------------------------------------------------------------------------------
//
// **No datagram leaves this process**: starting a tell is the client putting words in its own box,
// and every scenario here asserts that nothing is sent.

/// The action that starts a tell to whoever is selected, the map it is in, and a key that the
/// shipped keymaps deliberately leave free to bind it to.
const TELL_SELECTED: dereth_input::ActionId = dereth_input::ActionId(0x1000_0119);
const CHAT_COMMANDS: dereth_input::InputMapId = dereth_input::InputMapId(0x1000_000A);
const FREE_KEY_SCAN: u16 = 0x41;

/// Bind the tell-to-selected action to a key the shipped keymaps leave free, so that it can be
/// pressed at all: it is an action the client has and gives no key to.
fn bind_the_tell_key(c: &mut HeadlessClient) {
    use dereth_input::spec::{activation, ControlCode, SubControlIndex};
    let input = c.app_mut().input_manager_mut().expect("the input shell");
    assert!(
        input
            .keys_for_action(TELL_SELECTED, CHAT_COMMANDS)
            .is_empty(),
        "the client has this action and gives it no key, which is why one is bound here"
    );
    let key = dereth_input::ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, FREE_KEY_SCAN),
        0,
        activation::CLICK,
    );
    assert!(input.set_binding(CHAT_COMMANDS, TELL_SELECTED, None, key));
}

/// Put a named body in the world and select it, and let the screen see the selection.
fn select_a_named_player(c: &mut HeadlessClient, id: ObjectId, name: &str) {
    {
        let w = c.world_mut();
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.pwd.name = name.to_owned();
        wn.pwd.obj_type = dereth_rules::weenie::item_type::CREATURE;
        w.tables.weenies.insert(id, wn);
        w.selected = Some(id);
    }
    c.tick(1);
}

/// Put one line with a clickable name in it on the main log, and answer with the log and a point
/// over the first letter of that name.
fn a_clickable_name_on_the_log(c: &mut HeadlessClient, name: &str) -> (ElemHandle, (i32, i32)) {
    let line = dereth_ui_screens::chat::interface::ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        ty: 17,
        body: format!("<Tell:IIDString:0:{name}>{name}<\\Tell> says, \"hello\""),
        prefix: None,
        window: 0,
    };
    let reached = with_screen(c, |ui, s| s.recv_display_final_string_info(ui, &line));
    assert!(
        reached.contains(&dereth_ui_screens::chat::interface::window::MAIN),
        "the line reaches the main window"
    );
    let log = element(c, dereth_ui_screens::chat::window::LOG);
    let screen_box = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .screen_box(log);
    let (placed, glyph) = with_screen(c, |ui, _| {
        let t = ui.text_element_mut(log).expect("the log");
        let at = t
            .glyphs
            .glyphs
            .iter()
            .position(|g| {
                g.tag
                    .as_ref()
                    .is_some_and(|tag| tag.payload() == Some(name))
                    && g.data == name.encode_utf16().next().expect("a name has a letter")
            })
            .expect("the drawn name keeps what makes it clickable");
        (t.compose(screen_box)[at], t.glyphs.glyphs[at].clone())
    });
    (
        log,
        (
            placed.x + (glyph.width / 4).max(1),
            placed.y + (glyph.height / 2).max(1),
        ),
    )
}

/// Clicking a name in the chat log starts a private message to him -- and it does that only while
/// the entry does not already have the caret, so a half-typed line is not thrown away.
pub fn clicking_a_name_in_the_log_starts_a_tell_to_him() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let mut hand = Hand::new();
    let entry = main_chat_entry(&c);
    let (log, point) = a_clickable_name_on_the_log(&mut c, "Alba");
    c.app_mut().probe_mut().interaction_mut().last_sent.clear();

    let ready = focus_of(&c) != Some(entry)
        && element_text_of(&mut c, entry).is_empty()
        && c.view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .hit_test_screen(point.0, point.1)
            == Some(log);

    hand.press_at(&mut c, point.0, point.1);
    let started = element_text_of(&mut c, entry) == "@tell Alba, "
        && focus_of(&c) == Some(entry)
        && c.view().expect_app().interaction().last_sent.is_empty();

    // The other side of the guard. A press on the log takes the caret by itself, so merely giving
    // the entry the caret beforehand cannot reach it: the caret has to come back between the press
    // and the release.
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .text_element_mut(entry)
        .expect("the entry")
        .set_text("draft");
    let (_, other) = a_clickable_name_on_the_log(&mut c, "Focused");
    hand.mouse_down_at(&mut c, other.0, other.1);
    let took_it = focus_of(&c) != Some(entry);
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .take_focus(entry);
    hand.mouse_up(&mut c);
    let left_alone = element_text_of(&mut c, entry) == "draft"
        && c.view().expect_app().interaction().last_sent.is_empty();

    c.assert_behaviour(
        "chat.tell.clicking-a-name-in-the-log-starts-a-private-message-to-him",
        move |_| ready && started && took_it && left_alone,
    );
    c.shutdown();
}

/// The key that starts a private message to whoever is selected names him, gives the entry the
/// caret with the caret at the end and nothing picked out, and sends nothing.
pub fn the_tell_key_names_the_selected_player() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let mut hand = Hand::new();
    bind_the_tell_key(&mut c);
    select_a_named_player(&mut c, ObjectId(0x5000_1234), "Target Player");
    let entry = main_chat_entry(&c);
    let before = c.view().expect_app().interaction().last_sent.len();

    let key = key_of(winit::keyboard::KeyCode::F7);
    hand.key(&mut c, key, true);
    c.tick(1);

    let filled =
        element_text_of(&mut c, entry) == "@tell Target Player, " && focus_of(&c) == Some(entry);
    let caret = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let t = ui.text_element_mut(entry).expect("the entry");
        t.cursor == t.glyphs.len() && !t.bits.selecting()
    };
    let sent_nothing = c.view().expect_app().interaction().last_sent.len() == before;
    hand.key(&mut c, key, false);
    c.tick(1);

    c.assert_behaviour(
        "chat.tell.the-key-for-a-private-message-to-the-selected-player-names-him",
        move |_| filled && caret && sent_nothing,
    );
    c.shutdown();
}

/// The two rows of the talk-to menu that change with the selection carry the selected player's own
/// name, drawn into the rows rather than left as a placeholder.
pub fn the_selected_players_name_fills_both_changing_rows() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let mut hand = Hand::new();
    let selected = ObjectId(0x5000_1234);
    select_a_named_player(&mut c, selected, "Target Player");
    c.world_mut()
        .weenie_mut(selected)
        .expect("the selected fixture exists")
        .pwd
        .bitfield |= dereth_rules::weenie::bitfield::PLAYER;
    c.tick(1);

    // The rows are filled by the client's own once-a-second sweep rather than by the selection
    // being written, so the sweep is run -- with the one thing a nearby body would have given it.
    let mut facts = with_screen(&mut c, |_ui, s| s.chat_auto_target_world.clone());
    facts.in_range_of_player.push(selected.0);
    let adopted = {
        let (interaction, world) = c.app_mut().probe_mut().interaction_and_world_mut();
        world
            .chat
            .set_talk_focus_enabled(TalkFocus::Selected, false);
        interaction.update_chat_target(world, 100.0, &facts);
        world.chat.last_speakable_target == Some(selected)
    };
    c.tick(1);

    let (menu, row, squelch) = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the shell").ui;
        let any: &dyn std::any::Any = app
            .ui()
            .expect("the shell")
            .flow
            .current()
            .expect("a screen");
        let main = &any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen")
            .main_chat;
        (
            main.menu.expect("the talk-to menu"),
            main.menu_item(ui, 2).expect("the tell-to-selected row"),
            main.squelch_toggle.expect("the squelch row"),
        )
    };
    let popup = dereth_ui::widgets::menu::popup_handle(
        &c.view().expect_app().ui().expect("the shell").ui,
        menu,
    )
    .expect("the menu has a popup");

    hand.click_handle(&mut c, menu);
    let opened = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .node(popup)
        .expect("alive")
        .region
        .flags
        .visible;
    let reachable = {
        let ui = &c.view().expect_app().ui().expect("the shell").ui;
        let b = ui.screen_box(row);
        ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(row)
    };
    let named = element_text_of(&mut c, row) == "Tell to Target Player"
        && element_text_of(&mut c, squelch) == "Squelch (ignore) Target Player";

    c.assert_behaviour(
        "chat.talk-to-menu.the-selected-players-name-fills-both-of-the-changing-rows",
        move |_| adopted && opened && reachable && named,
    );
    c.shutdown();
}

/// The key does nothing when there is nobody selected, or when what is selected is not a player --
/// and it reads the selection as it is now rather than as the last frame left it.
pub fn the_tell_key_does_nothing_without_a_player_selected() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let mut hand = Hand::new();
    bind_the_tell_key(&mut c);
    select_a_named_player(&mut c, ObjectId(0x5000_2345), "Old Selection");
    let entry = main_chat_entry(&c);
    let key = key_of(winit::keyboard::KeyCode::F7);

    // Cleared after the screen had already been told about it, so a client reading last frame's
    // answer would address the wrong person.
    c.world_mut().selected = None;
    let stale = with_screen(&mut c, |_, s| {
        s.chat_auto_target_world.selected_name.clone()
    }) == "Old Selection";
    let fired_before = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired;
    hand.key(&mut c, key, true);
    c.tick(1);
    let pressed = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .stats
        .actions_fired
        > fired_before;
    let nothing = element_text_of(&mut c, entry).is_empty() && focus_of(&c) != Some(entry);
    hand.key(&mut c, key, false);
    c.tick(1);

    // And the two ends of the range of ids a player can have, neither of which is one.
    let mut boundaries = true;
    for boundary in [0x5000_0000_u32, 0x7000_0000] {
        select_a_named_player(&mut c, ObjectId(boundary), "Not A Player");
        hand.key(&mut c, key, true);
        c.tick(1);
        let holds = element_text_of(&mut c, entry).is_empty() && focus_of(&c) != Some(entry);
        assert!(holds, "{boundary:#010X}");
        boundaries &= holds;
        hand.key(&mut c, key, false);
        c.tick(1);
    }

    c.assert_behaviour(
        "chat.tell.the-key-does-nothing-when-nobody-a-player-could-talk-to-is-selected",
        move |_| stale && pressed && nothing && boundaries,
    );
    c.shutdown();
}

/// A client with the friends tab open and two friends on it.
fn a_client_with_two_friends(hand: &mut Hand) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    crate::social::open_the_friends_tab(&mut c);
    let _ = &hand;
    c.when(Inbound::message(&crate::social::friends_update(
        vec![
            crate::social::a_friend(crate::social::FRIEND_A, "Ash", true),
            crate::social::a_friend(crate::social::FRIEND_B, "Bex", true),
        ],
        0,
    )));
    c.tick(1);
    assert_eq!(
        c.view().expect_app().hud().panels.friends.rows().len(),
        2,
        "the list reached the tab"
    );
    c
}

/// Picking a friend and pressing the tell button fills the main entry with the beginning of a
/// private message to him, gives it the caret, and replaces whatever was half-typed there.
pub fn picking_a_friend_and_pressing_tell_fills_the_entry() {
    let mut hand = Hand::new();
    let mut c = a_client_with_two_friends(&mut hand);
    let entry = main_chat_entry(&c);

    // Before: nothing picked, the button dark, the entry empty and without the caret.
    let before = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .selected
        .is_none()
        && !crate::social::friends_button_lit(
            &c,
            c.view().expect_app().hud().panels.friends.tell_button,
        )
        && element_text_of(&mut c, entry).is_empty()
        && focus_of(&c) != Some(entry);

    let (list, row) = {
        let p = &c.view().expect_app().hud().panels.friends;
        (
            p.list.as_ref().expect("the list is bound").handle,
            p.rows()[0].element,
        )
    };
    let first_is_one = c.view().expect_app().hud().panels.friends.rows()[0].name == "Ash";
    hand.click_row(&mut c, list, row);
    let picked = c.view().expect_app().hud().panels.friends.selected == Some(0)
        && crate::social::friends_button_lit(
            &c,
            c.view().expect_app().hud().panels.friends.tell_button,
        );

    let tell = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .tell_button
        .expect("bound");
    hand.click_handle(&mut c, tell);
    let filled = element_text_of(&mut c, entry) == "@tell Ash, " && focus_of(&c) == Some(entry);
    let caret = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let t = ui.text_element_mut(entry).expect("the entry");
        t.cursor == t.glyphs.len() && !t.bits.selecting()
    };
    // And the window's own idea of what is being typed agrees with the element.
    let agrees = with_screen(&mut c, |_, s| {
        (s.chat[0].entry.clone(), s.chat[0].chat_entry_active)
    }) == ("@tell Ash, ".to_owned(), true);

    // Half-typed, then the other friend: replaced, not added to.
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .text_element_mut(entry)
        .expect("the entry")
        .set_text("hello there");
    let half_typed = element_text_of(&mut c, entry) == "hello there";
    let row = c.view().expect_app().hud().panels.friends.rows()[1].element;
    hand.click_row(&mut c, list, row);
    let tell = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .tell_button
        .expect("bound");
    hand.click_handle(&mut c, tell);
    let replaced = element_text_of(&mut c, entry) == "@tell Bex, " && focus_of(&c) == Some(entry);

    c.assert_behaviour(
        "chat.tell.picking-a-friend-and-pressing-tell-fills-the-entry-and-replaces-what-was-there",
        move |_| {
            before && first_is_one && picked && filled && caret && agrees && half_typed && replaced
        },
    );
    c.shutdown();
}

/// Only the main chat window takes the tell: the smaller windows beside it are left alone.
pub fn only_the_main_window_takes_the_tell() {
    let mut hand = Hand::new();
    let mut c = a_client_with_two_friends(&mut hand);
    let (list, row) = {
        let p = &c.view().expect_app().hud().panels.friends;
        (p.list.as_ref().expect("bound").handle, p.rows()[0].element)
    };
    hand.click_row(&mut c, list, row);
    let tell = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .tell_button
        .expect("bound");
    hand.click_handle(&mut c, tell);

    let main = main_chat_entry(&c);
    let filled = element_text_of(&mut c, main) == "@tell Ash, ";
    let others: Vec<ElemHandle> = with_screen(&mut c, |_, s| {
        s.chat_windows
            .iter()
            .skip(1)
            .filter_map(|w| w.entry)
            .collect()
    });
    let there_are_others = !others.is_empty();
    let mut untouched = true;
    for h in others {
        untouched &= element_text_of(&mut c, h).is_empty();
    }

    c.assert_behaviour("chat.tell.only-the-main-window-takes-the-tell", move |_| {
        filled && there_are_others && untouched
    });
    c.shutdown();
}
