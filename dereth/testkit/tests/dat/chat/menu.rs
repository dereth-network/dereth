use super::*;
// -------------------------------------------------------------------------------------------
// chat.talk-focus.picking-a-target-redraws-every-row-from-the-settings
// -------------------------------------------------------------------------------------------

/// The rows of the shipped talk-to menu, by the number the chat system knows each one as.
const SAY_ROW: u32 = 1;
const SELECTED_ROW: u32 = 2;
const FELLOWSHIP_ROW: u32 = 3;
pub(super) const GENERAL_ROW: u32 = 8;
const OLTHOI_ROW: u32 = 13;
/// What a row's element carries when it may be picked, and when it may not.
const PICKABLE: dereth_ui::StateId = dereth_ui::StateId(1);
const GREYED: dereth_ui::StateId = dereth_ui::StateId(0xD);

/// Run `f` against the shipped gameplay screen and the shell it was laid out in.
///
/// The chat state is lifted out of the world for the call and put straight back, because the
/// client's own delivery takes the screen, the shell and the chat state at once and a scenario
/// reaching the live client can only borrow one of them at a time. Nothing else sees it move.
fn on_the_shipped_menu<T>(
    c: &mut HeadlessClient,
    f: impl FnOnce(
        &mut dereth_ui::UiSystem,
        &mut GamePlayScreen,
        &mut dereth_client_model::chat::ChatState,
    ) -> T,
) -> T {
    let mut chat = std::mem::take(&mut c.world_mut().chat);
    let out = {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell is up");
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        f(&mut shell.ui, gameplay, &mut chat)
    };
    c.world_mut().chat = chat;
    out
}

/// The menu's rows follow what the chat system said, and picking a target redraws them all from
/// the player's own settings.
pub fn the_talk_to_menu_is_redrawn_from_the_settings() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // Whatever bringing the screen up said about the rows is spent; what is measured below is
    // what this scenario says about them.
    let _ = c.world_mut().chat.take_talk_focus_notices();

    // An Olthoi character, with the two rows that are not channels switched on.
    {
        let w = c.world_mut();
        w.chat.using_turbine_chat = true;
        w.enable_chat_talk_focuses(true);
        w.chat.set_talk_focus_enabled(TalkFocus::Selected, true);
        w.chat.set_talk_focus_enabled(TalkFocus::Fellowship, true);
    }
    let notices = c.world_mut().chat.take_talk_focus_notices();
    let drawn = on_the_shipped_menu(&mut c, |ui, screen, chat| {
        dereth_client_runtime::ui_context::offer_talk_focus_notices(
            chat,
            notices,
            &mut |focus, notice| {
                dereth_client_shell::hud_drive::talk_focus_notice(ui, screen, focus, notice)
            },
        );
        let state = |row: u32| {
            screen
                .main_chat
                .menu_item(ui, row)
                .map(|h| ui.node(h).expect("the row").state)
        };
        (
            state(SELECTED_ROW),
            state(FELLOWSHIP_ROW),
            state(GENERAL_ROW),
            state(OLTHOI_ROW),
            // The menu's own readback of the setting behind the row it just greyed.
            screen.main_chat.is_talk_focus_enabled(FELLOWSHIP_ROW),
        )
    });
    let rows_follow_the_answers = drawn
        == (
            Some(PICKABLE),
            Some(GREYED),
            Some(GREYED),
            Some(PICKABLE),
            true,
        );

    // **Picking a target.** Every row is redrawn from the player's stored settings, so the row the
    // answers above had opened closes again -- while the setting behind it is untouched.
    let after_picking = on_the_shipped_menu(&mut c, |ui, screen, _| {
        let say = screen
            .main_chat
            .menu_item(ui, SAY_ROW)
            .expect("the say row");
        screen.chat_target_menu_item(ui, say);
        screen
            .main_chat
            .menu_item(ui, SELECTED_ROW)
            .map(|h| ui.node(h).expect("the row").state)
    });
    let setting_untouched = c.view().world().chat.enabled_focuses()[SELECTED_ROW as usize];

    // Removing a projected row does not remove the shared channel fallback.
    let removed = on_the_shipped_menu(&mut c, |ui, screen, _| {
        let general = screen
            .main_chat
            .menu_item(ui, GENERAL_ROW)
            .expect("the general row");
        ui.remove_and_delete_root(general);
        screen.main_chat.menu_item(ui, GENERAL_ROW).is_none()
    });
    {
        let w = c.world_mut();
        w.chat.set_talk_focus(TalkFocus::General);
        w.enable_chat_talk_focuses(false);
        w.chat.set_talk_focus_enabled(TalkFocus::General, false);
    }
    let notices = c.world_mut().chat.take_talk_focus_notices();
    let rebuilt = on_the_shipped_menu(&mut c, |ui, screen, chat| {
        dereth_client_runtime::ui_context::offer_talk_focus_notices(
            chat,
            notices,
            &mut |focus, notice| {
                dereth_client_shell::hud_drive::talk_focus_notice(ui, screen, focus, notice)
            },
        );
        let kept = chat.talk_focus == TalkFocus::All;
        // The menu builds itself again and lands on the row it does have.
        let landed = screen.main_chat.init_talk_focus_menu(ui);
        let say = screen
            .main_chat
            .menu_item(ui, SAY_ROW)
            .expect("the say row");
        screen.chat_target_menu_item(ui, say);
        let general = screen
            .main_chat
            .menu_item(ui, GENERAL_ROW)
            .map(|h| ui.node(h).expect("the row").state);
        (kept, landed, general)
    });

    c.assert_behaviour(
        "chat.talk-focus.picking-a-target-redraws-every-row-from-the-settings",
        move |_| {
            rows_follow_the_answers
            && after_picking == Some(GREYED)
            && setting_untouched
            && removed
            && rebuilt.1 == OLTHOI_ROW as usize
            // Rebuilt from the setting the chat system holds, which that last batch set false.
            && rebuilt.2 == Some(GREYED)
        },
    );
    c.assert_behaviour(
        "chat.talk-focus.channel-fallback-does-not-require-a-window",
        move |_| rebuilt.0,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The talk-to menu: picking a channel, losing it, and the popup itself
// ---------------------------------------------------------------------------------------------

/// The say row of the talk-to menu, and the row for a channel this character may never use.
const SAY_MENU_ROW: u32 = 1;
const OLTHOI_MENU_ROW: u32 = 13;
/// The chat window itself, its entry, and the selection area beside it.
const CHAT_WINDOW: ElementId = ElementId(0x1000_0601);
const SELECTION_FIELD: ElementId = ElementId(0x1000_019E);
const SELECTION_NAME: ElementId = ElementId(0x1000_019F);

/// What the talk-to row of the chat window says the player is talking to.
fn menu_talk_focus(c: &HeadlessClient) -> u32 {
    let app = c.view().expect_app();
    let any: &dyn std::any::Any = app
        .ui()
        .expect("the shell")
        .flow
        .current()
        .expect("a screen");
    any.downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .main_chat
        .talk_focus
}

/// The shipped popup the talk-to button opens.
fn talk_to_popup(c: &HeadlessClient) -> ElemHandle {
    let menu = element(c, TALK_TO_BUTTON);
    dereth_ui::widgets::menu::popup_handle(
        &c.view().expect_app().ui().expect("the shell is up").ui,
        menu,
    )
    .expect("the talk-to button carries a popup")
}

/// The drawn text of one element.
fn element_text(c: &mut HeadlessClient, h: ElemHandle) -> String {
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(true))
}

/// A client with the chat-room service on and the player described, in gameplay.
fn a_client_on_the_general_channel() -> HeadlessClient {
    let mut c = a_bare_client(true);
    to_gameplay(&mut c);
    c.world_mut().chat.using_turbine_chat = true;
    describe(&mut c, true, false);
    c.tick(1);
    c
}

/// A channel the player was talking on, switched off and straight back on again, drops him back to
/// saying it aloud -- and the line he sends next goes where the window now says it does.
pub fn a_channel_switched_off_and_on_drops_the_player_back_to_saying_it() {
    use dereth_client_model::chat::TalkFocus;

    let mut c = a_client_on_the_general_channel();
    let mut hand = Hand::new();
    pick_general(&mut c, &mut hand);

    // A line typed on the channel is not misrouted into an ordinary spoken one just because the
    // room is not there to take it.
    let lines = c.view().expect_app().interaction().stats.chat_lines_sent;
    hand.say(&mut c, "general intent");
    let routed = c.view().expect_app().interaction().stats.chat_lines_sent == lines + 1
        && c.view().world().chat.talk_focus == TalkFocus::General
        && !c.view().expect_app().interaction().last_sent.iter().any(
            |r| matches!(r, dereth_client_model::Request::Talk(m) if m.message == "general intent"),
        );

    // Switching the channel off and on again is two changes, not none: the player is put back on
    // saying it aloud by the first and left there by the second, and the line he was typing goes
    // out that way.
    hand.click_element(&mut c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    hand.type_text(&mut c, "after the fallback");
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, false);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    hand.press_return(&mut c);
    let fell_back = menu_talk_focus(&c) == SAY_MENU_ROW
        && c.view().world().chat.talk_focus == TalkFocus::All
        && c.view().expect_app().interaction().last_sent.iter().any(
            |r| matches!(r, dereth_client_model::Request::Talk(m) if m.message == "after the fallback"),
        );
    // The row is offered again, not picked again.
    let offered = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen");
        let row = screen
            .main_chat
            .menu_item(&shell.ui, GENERAL_ROW)
            .expect("the general row");
        shell.ui.node(row).expect("the row is alive").state == StateId(1)
    };

    // The same edge from the option the player himself sets, with a line between the two halves:
    // the line has to see the fallback that has already happened rather than the final answer.
    pick_general(&mut c, &mut hand);
    for request in [
        dereth_client_contract::UiRequest::SetPlayerOption(
            dereth_client_contract::PlayerOption::HearGeneralChat,
            false,
        ),
        dereth_client_contract::UiRequest::ChatLine {
            text: "between the options".to_owned(),
            window: 1,
        },
        dereth_client_contract::UiRequest::SetPlayerOption(
            dereth_client_contract::PlayerOption::HearGeneralChat,
            true,
        ),
    ] {
        c.ui_outbox().emit(request);
    }
    c.tick(1);
    let in_between = c.view().world().chat.talk_focus == TalkFocus::All
        && menu_talk_focus(&c) == SAY_MENU_ROW
        && c.view().expect_app().interaction().last_sent.iter().any(
            |r| matches!(r, dereth_client_model::Request::Talk(m) if m.message == "between the options"),
        )
        && c.view().world().chat.enabled_focuses()[GENERAL_ROW as usize];

    // And the same edge when it comes from the shard's own description of the player.
    pick_general(&mut c, &mut hand);
    describe(&mut c, false, false);
    describe(&mut c, true, false);
    c.tick(1);
    let from_the_shard =
        c.view().world().chat.talk_focus == TalkFocus::All && menu_talk_focus(&c) == SAY_MENU_ROW;

    // And when it comes from the service itself going away.
    pick_general(&mut c, &mut hand);
    c.world_mut().chat.using_turbine_chat = false;
    c.world_mut().enable_chat_talk_focuses(false);
    c.world_mut().chat.using_turbine_chat = true;
    c.world_mut().enable_chat_talk_focuses(false);
    c.tick(1);
    let from_the_service = menu_talk_focus(&c) == SAY_MENU_ROW
        && c.view().world().chat.talk_focus == TalkFocus::All
        && c.world_mut().chat.take_talk_focus_notices().is_empty();

    c.assert_behaviour(
        "chat.talk-focus.a-channel-switched-off-and-on-again-drops-the-player-back-to-saying-it",
        move |_| routed && fell_back && offered && in_between && from_the_shard && from_the_service,
    );
    c.shutdown();
}

/// A chat window built after the settings changed reads the settings as they are now, and never
/// replays what it was not there to hear.
pub fn a_new_chat_window_reads_the_settings_now() {
    use dereth_client_model::chat::TalkFocus;

    let mut c = a_bare_client(true);
    let mut hand = Hand::new();
    // The player is described before the window is built, which is the ordinary order.
    c.world_mut().chat.using_turbine_chat = true;
    describe(&mut c, false, false);
    describe(&mut c, true, false);
    to_gameplay(&mut c);

    // The offline server stub answers the opening allegiance request after the UI boundary;
    // its newly shared availability notices belong to the next frame.
    c.tick(1);
    // **Everything a frame says about the channels is delivered in that frame.** A notice left
    // over would be heard by the next window, which is the replay this claim is about; the message
    // names whatever is left so that the next producer to forget is identified rather than guessed.
    let left_over = c.world_mut().chat.take_talk_focus_notices();
    assert!(
        left_over.is_empty(),
        "a change to a channel is delivered in the frame that made it; left over: {left_over:?}"
    );

    pick_general(&mut c, &mut hand);
    let old_menu = element(&c, TALK_TO_BUTTON);
    let old_popup = talk_to_popup(&c);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, false);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    to_gameplay(&mut c);
    let rebuilt = {
        let ui = &c.view().expect_app().ui().expect("the shell").ui;
        !ui.is_alive(old_menu) && !ui.is_alive(old_popup)
    } && c.view().world().chat.talk_focus == TalkFocus::All;
    pick_general(&mut c, &mut hand);
    c.tick(1);
    let not_replayed = c.view().world().chat.talk_focus == TalkFocus::General;

    // A closed window does not suspend shared fallback; notices still are not replayed.
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::CHARACTER_MANAGEMENT);
    c.tick(1);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, false);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    c.tick(1);
    let discarded = c.world_mut().chat.take_talk_focus_notices().is_empty()
        && c.view().world().chat.talk_focus == TalkFocus::All;
    // A returning window projects that current shared choice.
    to_gameplay(&mut c);
    let chose_say = c.view().world().chat.talk_focus == TalkFocus::All;
    pick_general(&mut c, &mut hand);
    c.tick(1);
    let picked_again = c.view().world().chat.talk_focus == TalkFocus::General;

    // A line the player typed to somebody in particular, queued before the window was replaced: the
    // new window may choose a channel for the next line, but it must not turn this one into a
    // shout.
    let selected = ObjectId(0x5000_0017);
    c.world_mut().selected = Some(selected);
    // The chat window has taken him up as its chat target, which is who a tell goes to.
    c.world_mut().chat.last_speakable_target = Some(selected);
    c.world_mut().chat.set_talk_focus(TalkFocus::Selected);
    c.ui_outbox()
        .emit(dereth_client_contract::UiRequest::ChatLine {
            text: "before the window was replaced".to_owned(),
            window: 1,
        });
    to_gameplay(&mut c);
    let stayed_a_tell = c.view().expect_app().interaction().last_sent.iter().any(|r| {
        matches!(r, dereth_client_model::Request::TalkDirect(m)
            if m.target == selected && m.message == "before the window was replaced")
    }) && !c.view().expect_app().interaction().last_sent.iter().any(|r| {
        matches!(r, dereth_client_model::Request::Talk(m) if m.message == "before the window was replaced")
    });

    c.assert_behaviour(
        "chat.talk-focus.a-new-window-reads-the-settings-now-rather-than-replaying-what-it-missed",
        move |_| rebuilt && not_replayed && picked_again && stayed_a_tell,
    );
    c.assert_behaviour(
        "chat.talk-focus.channel-fallback-does-not-require-a-window",
        move |_| discarded && chose_say,
    );
    c.shutdown();
}

/// Shared channel fallback runs without a window, and notices are not kept for a future one.
pub fn channel_fallback_does_not_require_a_window() {
    use dereth_client_model::chat::TalkFocus;

    let mut c = a_bare_client(false);
    let had_none = c.view().expect_app().ui().is_none();
    c.world_mut().chat.using_turbine_chat = true;
    describe(&mut c, true, false);
    c.world_mut().chat.set_talk_focus(TalkFocus::General);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, false);
    c.world_mut()
        .chat
        .set_talk_focus_enabled(TalkFocus::General, true);
    c.tick(1);
    let kept = c.view().world().chat.talk_focus == TalkFocus::All
        && c.world_mut().chat.take_talk_focus_notices().is_empty();
    c.tick(1);
    let still = c.view().world().chat.talk_focus == TalkFocus::All;

    c.assert_behaviour(
        "chat.talk-focus.channel-fallback-does-not-require-a-window",
        move |_| had_none && kept && still,
    );
    c.shutdown();
}

/// Whether `popup` draws above every part of the chat window, and every one of its rows is drawn
/// whole rather than clipped into the window's log.
fn popup_is_in_front(c: &HeadlessClient, popup: ElemHandle, chat: ElemHandle) -> bool {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the shell is up").ui;
    let cmds = app.ui_draw_list();
    let last_chat = cmds
        .iter()
        .rposition(|cmd| cmd.who == chat || ui.is_ancestor_of(chat, cmd.who))
        .expect("the chat window is drawn");
    let first_popup = cmds
        .iter()
        .position(|cmd| cmd.who == popup || ui.is_ancestor_of(popup, cmd.who))
        .expect("the popup is drawn");
    if first_popup <= last_chat {
        return false;
    }
    let any: &dyn std::any::Any = app
        .ui()
        .expect("the shell")
        .flow
        .current()
        .expect("a screen");
    let screen = any
        .downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen");
    for row in std::iter::once(screen.main_chat.squelch_toggle.expect("the squelch row"))
        .chain(screen.main_chat.talk_focus_buttons.iter().copied())
    {
        let Some(cmd) = cmds.iter().find(|cmd| cmd.who == row) else {
            return false;
        };
        if cmd.glyphs.is_empty() || cmd.screen.intersect(&cmd.clip) != cmd.screen {
            return false;
        }
    }
    true
}

/// The talk-to popup draws over the chat window, a row the player may not use refuses him, a row
/// he may use closes the popup and takes the channel, and the keyboard goes back to the entry.
pub fn the_talk_to_popup_draws_over_the_window_and_gives_the_keyboard_back() {
    let mut c = a_bare_client(true);
    to_gameplay(&mut c);
    c.world_mut().chat.using_turbine_chat = true;
    c.when(Inbound::event(SessionEvent::PlayerDescription(
        Box::default(),
    )));
    c.tick(1);

    let chat = element(&c, CHAT_WINDOW);
    let popup = talk_to_popup(&c);
    let parented = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .parent(popup)
        == Some(c.view().expect_app().ui().expect("the shell").ui.root());

    let mut hand = Hand::new();
    hand.click_element(&mut c, TALK_TO_BUTTON);
    let in_front = popup_is_in_front(&c, popup, chat)
        && c.view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .active_element()
            == Some(popup);

    // The general channel is offered because the player's own description said so; the one this
    // character may never use is not.
    let (general_offered, dark_row) = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen");
        let general = screen
            .main_chat
            .menu_item(&shell.ui, GENERAL_ROW)
            .expect("the general row");
        let dark = screen
            .main_chat
            .menu_item(&shell.ui, OLTHOI_MENU_ROW)
            .expect("the dark row");
        (
            c.view().world().chat.enabled_focuses()[GENERAL_ROW as usize]
                && shell.ui.node(general).expect("alive").state != StateId(0xD),
            dark,
        )
    };
    let dark_state = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .node(dark_row)
        .expect("alive")
        .state
        == StateId(0xD);
    hand.click_handle(&mut c, dark_row);
    let refused = menu_talk_focus(&c) == SAY_MENU_ROW;

    // Re-open it if that click closed it, then pick a row the player may use.
    if !c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .node(popup)
        .expect("alive")
        .region
        .flags
        .visible
    {
        hand.click_element(&mut c, TALK_TO_BUTTON);
    }
    let still_in_front = popup_is_in_front(&c, popup, chat);
    let row = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen");
        screen
            .main_chat
            .menu_item(&shell.ui, SAY_MENU_ROW)
            .expect("the say row")
    };
    let hit = {
        let ui = &c.view().expect_app().ui().expect("the shell").ui;
        let b = ui.screen_box(row);
        ui.hit_test(ui.root(), (b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(row)
    };
    hand.click_handle(&mut c, row);
    let closed = menu_talk_focus(&c) == SAY_MENU_ROW
        && !c
            .view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .node(popup)
            .expect("alive")
            .region
            .flags
            .visible
        && !c
            .view()
            .expect_app()
            .ui_draw_list()
            .iter()
            .any(|cmd| cmd.who == row)
        && !c
            .view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .activatable_elements()
            .contains(&popup);

    // And the keyboard is the entry's again.
    let entry = element(&c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    hand.click_element(&mut c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    hand.type_text(&mut c, "next");
    let typed = element_text(&mut c, entry) == "next";
    // Opening the popup and then clicking away closes it without eating the next keystroke.
    hand.click_element(&mut c, TALK_TO_BUTTON);
    let reopened = popup_is_in_front(&c, popup, chat);
    hand.click_element(&mut c, dereth_testkit::adapters_chat::CHAT_ENTRY);
    let clicked_away = !c
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
    hand.type_text(&mut c, " operation");
    let carried_on = element_text(&mut c, entry) == "next operation";

    c.assert_behaviour(
        "chat.talk-to-menu.the-popup-draws-over-the-window-and-gives-the-keyboard-back",
        move |_| {
            parented
                && in_front
                && general_offered
                && dark_state
                && refused
                && still_in_front
                && hit
                && closed
                && typed
                && reopened
                && clicked_away
                && carried_on
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The selection area beside the chat window
// ---------------------------------------------------------------------------------------------
//
// **This row belongs in another subject.** What the selection field draws is the shell's claim and
// not chat's; it is kept here, beside the chat window it borders.

/// The image the field draws, and the state it is in.
fn selection_field(c: &HeadlessClient, want_state: u32) -> bool {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the shell is up").ui;
    let field = {
        let any: &dyn std::any::Any = app
            .ui()
            .expect("the shell")
            .flow
            .current()
            .expect("a screen");
        let root = any
            .downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen")
            .root()
            .expect("a root");
        ui.get_child_recursive(root, SELECTION_FIELD)
            .expect("the selection field")
    };
    let Some(n) = ui.node(field) else {
        return false;
    };
    // The field is authored visible even when nothing is picked: what changes is its state, not
    // whether it is there.
    if n.state != StateId(want_state) || !n.region.flags.visible {
        return false;
    }
    let last_image = |media: &[dereth_ui::desc::MediaDesc]| {
        media.iter().rev().find_map(|m| match m.fields {
            dereth_ui::desc::MediaFields::Image { file, .. } => Some(file),
            _ => None,
        })
    };
    let image = n
        .desc
        .access_state(StateId(want_state))
        .and_then(|d| last_image(&d.media))
        .or_else(|| last_image(&n.desc.base.media))
        .expect("the shipped field has an authored image");
    let Some(cmd) = app.ui_draw_list().iter().find(|cmd| cmd.who == field) else {
        return false;
    };
    cmd.image == Some(image) && image.0 != 0
}

/// Picking an object fills the field beside the chat window, a stack of more than one changes it
/// again, and picking nothing puts it back to the background it was authored with.
pub fn the_selection_field_follows_what_is_picked_and_clears() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(1));
    let name = element(&c, SELECTION_NAME);
    let empty_to_start = selection_field(&c, 0);

    let id = ObjectId(0x7100_0021);
    {
        let mut w = dereth_client_model::Weenie::new(id);
        w.pwd.name = "a picked thing".to_owned();
        w.pwd.obj_type = 1;
        c.world_mut().tables.weenies.insert(id, w);
    }
    let mut choose = |c: &mut HeadlessClient, id: Option<ObjectId>| {
        c.world_mut().set_selected_object(
            id,
            false,
            &mut dereth_client_model::RecordingSink::default(),
        );
        c.tick(1);
    };
    choose(&mut c, Some(id));
    let picked = selection_field(&c, 0x1000_000B) && element_text(&mut c, name) == "a picked thing";

    // A stack of more than one is a different state again, and dropping back to one puts it back.
    c.world_mut()
        .tables
        .weenies
        .get_mut(id)
        .expect("the thing")
        .pwd
        .stack_size = Some(3);
    c.tick(1);
    let stacked = selection_field(&c, 0x1000_000C);
    c.world_mut()
        .tables
        .weenies
        .get_mut(id)
        .expect("the thing")
        .pwd
        .stack_size = Some(1);
    c.tick(1);
    let unstacked = selection_field(&c, 0x1000_000B);

    choose(&mut c, None);
    let cleared = selection_field(&c, 0) && element_text(&mut c, name).is_empty();

    // And the same when the thing itself has gone rather than merely been unpicked.
    choose(&mut c, Some(id));
    let picked_again = selection_field(&c, 0x1000_000B);
    c.world_mut().tables.weenies.remove(id);
    choose(&mut c, None);
    let gone = selection_field(&c, 0) && element_text(&mut c, name).is_empty();

    // An empty field does not block anything else: the talk-to popup still opens.
    let mut hand = Hand::new();
    hand.click_element(&mut c, TALK_TO_BUTTON);
    let popup = talk_to_popup(&c);
    let unblocked = c
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

    c.assert_behaviour(
        "chat.selection.the-field-beside-the-window-follows-what-is-picked-and-clears",
        move |_| {
            empty_to_start
                && picked
                && stacked
                && unstacked
                && cleared
                && picked_again
                && gone
                && unblocked
        },
    );
    c.shutdown();
}
