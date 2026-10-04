use super::*;
// ---------------------------------------------------------------------------------------------
// The chat surface: the return key, the keys that move the log, the menu, and the small windows
// ---------------------------------------------------------------------------------------------

/// The keys the chat window answers, by the number it knows each by.
const ACTION_ENTER: u32 = 0x25;
const ACTION_HOME: u32 = 0x1C;
const ACTION_END: u32 = 0x1D;
pub(super) const ACTION_HISTORY_BACK: u32 = 0x1E;
const ACTION_PAGE_UP: u32 = 0x20;
const ACTION_ESCAPE: u32 = 0x27;
/// The key that opens the entry, and the three that reply to somebody.
pub(super) const ACTION_OPEN_ENTRY: u32 = 0x1000_0023;
const ACTION_REPLY_MONARCH: u32 = 0x1000_0020;
const ACTION_REPLY_PATRON: u32 = 0x1000_0021;
const ACTION_REPLY_LAST: u32 = 0x1000_0022;

/// One key the chat window is asked about.
fn an_action(action: u32) -> dereth_ui::focus::InputEvent {
    dereth_ui::focus::InputEvent {
        action,
        start: true,
        x: 0,
        y: 0,
    }
}

/// The line the return key sends is taken out of the box, remembered, and raised for the window it
/// was typed in -- and the key that opens the entry gives it the caret back.
pub fn the_line_the_return_key_sends_is_remembered() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    hand.click_handle(&mut c, entry);
    let focused = focus_of(&c) == Some(entry);
    hand.type_text(&mut c, "hail well met");
    let typed = element_text_of(&mut c, entry) == "hail well met";

    c.ui_outbox().clear();
    hand.press_return(&mut c);
    let emptied = element_text_of(&mut c, entry).is_empty();
    let remembered = chat_history(&mut c) == ["hail well met"]
        && with_screen(&mut c, |_, s| {
            !s.chat
                .iter()
                .find(|w| w.window_id == 8)
                .unwrap()
                .chat_entry_active
        });

    // The key that opens the entry gives it the caret back, which is the other way in.
    let opened = with_screen(&mut c, |ui, s| s.chat_on_action(ui, ACTION_OPEN_ENTRY));
    // One frame, because whether the keyboard goes to the box is read when the character arrives
    // and the shell mirrors the new caret into it on the next.
    c.tick(1);
    hand.type_text(&mut c, "second line");
    let back = focus_of(&c) == Some(entry);
    // The frame drains what the window asked for before a scenario could look at it, so the ask
    // is read by running the same arm the key runs, with the queue in hand.
    c.ui_outbox().clear();
    let consumed = with_screen(&mut c, |ui, s| {
        s.chat_on_child_action(ui, entry, &an_action(ACTION_ENTER))
    });
    let raised = c.ui_outbox().take()
        == vec![dereth_client_contract::UiRequest::ChatLine {
            text: "second line".to_owned(),
            window: 8,
        }];

    c.assert_behaviour(
        "chat.entry.the-line-the-return-key-sends-is-remembered-and-raised-for-its-own-window",
        move |_| focused && typed && emptied && remembered && opened && back && consumed && raised,
    );
    c.shutdown();
}

/// The keys that move the log move the log, and the keys that walk the history fill the entry --
/// two different places, and telling them apart is the point.
pub fn the_scroll_keys_move_the_log_and_the_history_keys_fill_the_entry() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut hand = Hand::new();
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    let log = element(&c, dereth_ui_screens::chat::window::LOG);
    hand.click_handle(&mut c, entry);

    // Enough in the window that the log has somewhere to go.
    for i in 0..12 {
        let m = dereth_ui_screens::chat::interface::ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty: 0,
            body: format!("line number {i} of the backlog"),
            prefix: None,
            window: 0,
        };
        with_screen(&mut c, |ui, s| s.recv_display_final_string_info(ui, &m));
    }
    c.tick(1);

    let scroll_y = |c: &mut HeadlessClient| {
        with_screen(c, |ui, _| {
            ui.text_element_mut(log).map_or(0, |t| t.scroll.y)
        })
    };
    let at_end = scroll_y(&mut c);
    let has_travel = at_end > 0;

    let home = with_screen(&mut c, |ui, s| {
        s.chat_on_child_action(ui, entry, &an_action(ACTION_HOME))
    });
    let to_the_top = home && scroll_y(&mut c) == 0;
    let end = with_screen(&mut c, |ui, s| {
        s.chat_on_child_action(ui, entry, &an_action(ACTION_END))
    });
    let to_the_bottom = end && scroll_y(&mut c) == at_end;
    let page = with_screen(&mut c, |ui, s| {
        s.chat_on_child_action(ui, entry, &an_action(ACTION_PAGE_UP))
    });
    let paged = page && scroll_y(&mut c) < at_end;

    // The history keys write into the **entry**, not the log.
    for text in ["first", "second"] {
        c.world_mut().chat.entries.entry(8).or_default().submit(
            text,
            dereth_client_contract::options::interface::Interface::Retail,
        );
    }
    let back = with_screen(&mut c, |ui, s| {
        s.chat_on_child_action(ui, entry, &an_action(ACTION_HISTORY_BACK))
    });
    c.tick(1);
    let filled = back && element_text_of(&mut c, entry) == "second";

    // And the key that gives up: the caret goes, and the text stays.
    let escaped = with_screen(&mut c, |ui, s| {
        let took = s.chat_on_child_action(ui, entry, &an_action(ACTION_ESCAPE));
        (took, ui.focus_element())
    }) == (true, None);

    c.assert_behaviour(
        "chat.window.the-keys-that-move-the-log-and-the-keys-that-walk-the-history-are-different",
        move |_| has_travel && to_the_top && to_the_bottom && paged && filled && escaped,
    );
    c.shutdown();
}

/// Run one typed line through the client's own frame with the talk focus set, and answer with what
/// it produced.
fn line_with_focus(focus: u32, text: &str) -> Vec<dereth_client_model::Request> {
    let store = dereth_dat::testing::open_store().expect("the retail data is the client's own");
    let mut inter = dereth_client::interaction::Interaction::new();
    let mut objects = dereth_client::objects::ObjectStream::new();
    inter.queue(
        Vec::new(),
        vec![
            dereth_client_contract::UiRequest::SetTalkFocus { focus },
            dereth_client_contract::UiRequest::ChatLine {
                text: text.to_owned(),
                window: 8,
            },
        ],
    );
    let (unowned, _) = dereth_client::interaction::use_time(
        &mut inter,
        &store,
        None,
        &mut objects,
        None,
        Vec::new(),
        false,
        (800, 600),
        dereth_primitives::LocalTime(1.0),
    );
    assert!(
        unowned.is_empty(),
        "focus {focus}: nobody owned {unowned:?}"
    );
    inter.last_sent.clone()
}

/// What a typed line becomes follows the talk-to menu: an ordinary spoken line, or a line on one
/// of four channels -- and the row that talks to whoever is selected sends nothing at all when
/// nobody is.
pub fn what_a_typed_line_becomes_follows_the_menu() {
    use dereth_client_model::Request;
    let talk = |m: &str| {
        Request::Talk(dereth_protocol::comms::CommunicationTalk {
            message: m.to_owned(),
        })
    };
    let channel = |ch: u32, m: &str| {
        Request::ChannelBroadcast(dereth_protocol::comms::CommunicationChannelBroadcast {
            channel: ch,
            message: m.to_owned(),
        })
    };
    // **The channels do not follow the menu's own naming**, which is why each is named here: the
    // fellowship row carries one number and the vassals row another, and getting them the wrong
    // way round sends a line where everybody but the player can read it.
    let cases: [(u32, Option<Request>); 6] = [
        (1, Some(talk("well met"))),
        // The row that talks to whoever is selected, with nobody selected: dropped rather than
        // said out loud, which is what stops a private line becoming a public one.
        (2, None),
        (3, Some(channel(0x800, "well met"))),
        (4, Some(channel(0x2000, "well met"))),
        (5, Some(channel(0x4000, "well met"))),
        (6, Some(channel(0x1000, "well met"))),
    ];
    let mut every = true;
    for (focus, want) in cases {
        let got = line_with_focus(focus, "well met");
        let holds = match &want {
            Some(w) => got == vec![w.clone()],
            None => got.is_empty(),
        };
        assert!(holds, "focus {focus}: got {got:?}");
        every &= holds;
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.talk-focus.what-a-typed-line-becomes-follows-the-menu",
        move |_| every,
    );
}

/// A line the client handles itself is never spoken to the shard, whichever channel the menu is
/// on -- saying a command out loud on a channel everybody reads would be both wrong and rude.
pub fn a_command_the_client_handles_is_never_spoken() {
    let mut every = true;
    for focus in [1_u32, 3, 4, 5, 6] {
        let got = line_with_focus(focus, "@help");
        assert!(got.is_empty(), "focus {focus}: got {got:?}");
        every &= got.is_empty();
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.commands.a-command-the-client-handles-is-never-spoken-to-the-shard",
        move |_| every,
    );
}

/// The caption on the talk-to button follows the menu, a row that is shut refuses to be picked,
/// and a row that is not there at all does nothing.
pub fn the_button_caption_follows_the_menu() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let caption_el = element(
        &c,
        dereth_ui_screens::chat::mainchat::CHAT_TARGET_BUTTON_TEXT,
    );
    let caption = |c: &mut HeadlessClient| element_text_of(c, caption_el);
    let default = caption(&mut c);
    let started_on_one = with_screen(&mut c, |_, s| (s.main_chat.talk_focus, s.main_chat.caption))
        == (1, "ID_Chat_ChatTargetMenu")
        && !default.is_empty();

    // A channel row starts shut, and picking it is refused.
    let refused = with_screen(&mut c, |ui, s| s.main_chat.handle_selection(ui, 8)).is_none()
        && caption(&mut c) == default;

    // Opened, the same pick takes.
    let taken = with_screen(&mut c, |ui, s| {
        s.main_chat.set_talk_focus_enabled(8, true);
        // Turning the channel on is the chat system's own flag and touches no row; putting the
        // row's own state back is a separate act, and it is what the pick is refused on.
        s.main_chat.reset_all_talk_focus_menu_buttons(ui);
        s.main_chat.handle_selection(ui, 8)
    })
    .is_some_and(|change| {
        change.focus == 8
            && change.caption == "ID_Chat_ChatTargetMenuGeneral"
            && !change.wants_alleg_chat
    });
    let general = caption(&mut c);
    let followed = general != default;

    // One row does something else as well: it asks to be in the allegiance's chat.
    let allegiance = with_screen(&mut c, |ui, s| {
        s.main_chat.set_talk_focus_enabled(7, true);
        s.main_chat.reset_all_talk_focus_menu_buttons(ui);
        let change = s
            .main_chat
            .handle_selection(ui, 7)
            .expect("the allegiance row");
        change.wants_alleg_chat
    }) && caption(&mut c) != general;

    // A row the menu does not have at all: nothing happens.
    let absent = with_screen(&mut c, |ui, s| s.main_chat.handle_selection(ui, 99)).is_none();

    c.assert_behaviour(
        "chat.talk-to-menu.the-button-caption-follows-the-menu-and-a-row-that-is-shut-refuses",
        move |_| started_on_one && refused && taken && followed && allegiance && absent,
    );
    c.shutdown();
}

/// Which channel a row of the talk-to menu is, is carried by the row itself rather than by where
/// it sits in the list -- the list is not in the channels' own order, and the first row is not a
/// channel at all.
pub fn a_rows_place_in_the_list_is_not_the_order_of_the_channels() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.ui_outbox().clear();

    // The first row is the one that silences somebody and is not a channel.
    let first_is_not_a_channel =
        with_screen(&mut c, |ui, s| s.chat_target_menu_selection(ui, 0)).is_none();

    let picked = with_screen(&mut c, |ui, s| {
        s.main_chat.set_talk_focus_enabled(5, true);
        s.main_chat.reset_all_talk_focus_menu_buttons(ui);
        s.chat_target_menu_selection(ui, 1)
    })
    .is_some_and(|change| change.focus == 5 && change.caption == "ID_Chat_ChatTargetMenuMonarch");
    let asked =
        c.ui_outbox().take() == vec![dereth_client_contract::UiRequest::SetTalkFocus { focus: 5 }];

    c.assert_behaviour(
        "chat.talk-to-menu.which-channel-a-row-is-comes-from-the-row-and-not-its-place",
        move |_| first_is_not_a_channel && picked && asked,
    );
    c.shutdown();
}

/// The three reply keys address three different people, and each addresses his own -- getting them
/// the wrong way round would send a private reply to the wrong person.
pub fn the_reply_keys_address_three_different_people() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let entry = element(&c, dereth_ui_screens::chat::window::ENTRY);
    c.world_mut().chat.last_teller_name = "Ipsum".into();
    c.world_mut().chat.last_monarch_sender = "Amaranthea".into();
    c.world_mut().chat.last_patron_sender = "Borelean".into();
    let template = dereth_ui_screens::chat::window::ASSISTED_TELL_FALLBACK;

    let mut every = true;
    for (action, who) in [
        (ACTION_REPLY_LAST, "Ipsum"),
        (ACTION_REPLY_MONARCH, "Amaranthea"),
        (ACTION_REPLY_PATRON, "Borelean"),
    ] {
        let took = with_screen(&mut c, |ui, s| s.chat_on_action(ui, action));
        c.tick(1);
        let took = took && focus_of(&c) == Some(entry);
        let addressed = element_text_of(&mut c, entry) == format!("{template}{who}, ");
        assert!(took && addressed, "{action:#X} should address {who}");
        every &= took && addressed;
    }

    // With nobody to reply to, the key is still taken and the box is left alone.
    c.world_mut().chat.last_teller_name.clear();
    c.world_mut().chat.last_monarch_sender.clear();
    c.world_mut().chat.last_patron_sender.clear();
    let before = element_text_of(&mut c, entry);
    let still_taken = with_screen(&mut c, |ui, s| s.chat_on_action(ui, ACTION_REPLY_LAST));
    c.tick(1);
    let left_alone = still_taken && element_text_of(&mut c, entry) == before;

    c.assert_behaviour(
        "chat.tell.the-three-reply-keys-address-three-different-people",
        move |_| every && left_alone,
    );
    c.shutdown();
}

/// A small chat window writes back where it was put, how big it is, whether it is open and what it
/// is called -- so a player's arrangement survives him logging out.
pub fn a_floaty_window_writes_its_place_and_title_back() {
    use dereth_ui_screens::chat::floaty::placement as p;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.ui_outbox().clear();
    let ids: Vec<u32> = with_screen(&mut c, |_, s| {
        assert_eq!(s.floaty_chat.len(), 4, "the four small chat windows");
        s.floaty_chat.iter().map(|w| w.window_id).collect()
    });
    let four = ids == vec![2, 3, 4, 5];

    with_screen(&mut c, |ui, s| {
        let w = s.floaty_chat[0].clone();
        w.move_to(ui, 120, 340);
        w.resize_to(ui, 300, 150);
        w.set_visible(ui, false);
    });
    let written = c.ui_outbox().take()
        == vec![
            dereth_client_contract::UiRequest::SetChatWindowOption {
                window: 2,
                property: p::X,
                value: 120,
            },
            dereth_client_contract::UiRequest::SetChatWindowOption {
                window: 2,
                property: p::Y,
                value: 340,
            },
            dereth_client_contract::UiRequest::SetChatWindowOption {
                window: 2,
                property: p::WIDTH,
                value: 300,
            },
            dereth_client_contract::UiRequest::SetChatWindowOption {
                window: 2,
                property: p::HEIGHT,
                value: 150,
            },
            dereth_client_contract::UiRequest::SetChatWindowOption {
                window: 2,
                property: p::VISIBILITY,
                value: 0,
            },
        ];

    // A new title is broadcast and taken by the one window it names, and that window alone.
    let title = dereth_ui_screens::view::ChatWindowTitle::Table {
        string_id: dereth_ui::persist::preferences::token_of("ID_Chat_Chat3_DefaultTitle"),
        table_id: dereth_ui_screens::chat::mainchat::CAPTION_STRING_TABLE.0,
    };
    let titled = with_screen(&mut c, |ui, s| {
        let took = s.on_set_chat_window_title(ui, 4, title.clone());
        (
            took,
            s.floaty_chat[0].title.clone(),
            s.floaty_chat[2].title.clone(),
        )
    }) == (true, None, Some(title));
    let sent = c.ui_outbox().take();
    let title_written = sent.len() == 1
        && matches!(
            sent[0],
            dereth_client_contract::UiRequest::SetChatWindowTitle { window: 4, .. }
        );

    c.assert_behaviour(
        "chat.window.a-small-window-writes-its-place-size-openness-and-title-back",
        move |_| four && written && titled && title_written,
    );
    c.shutdown();
}

/// The close button shuts exactly its own window and writes that down, so a window a player closed
/// stays closed when he comes back.
pub fn the_close_button_hides_its_own_window() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.ui_outbox().clear();
    let (root, close) = with_screen(&mut c, |_, s| {
        let w = &s.floaty_chat[1];
        (
            w.root.expect("the window's root"),
            w.close_button.expect("the close button"),
        )
    });
    let closed = with_screen(&mut c, |ui, s| {
        let which = s.chat_on_close_button(ui, close);
        (which, ui.node(root).is_some_and(|n| n.region.flags.visible))
    }) == (Some(3), false);
    let written = c.ui_outbox().take()
        == vec![dereth_client_contract::UiRequest::SetChatWindowOption {
            window: 3,
            property: dereth_ui_screens::chat::floaty::placement::VISIBILITY,
            value: 0,
        }];

    c.assert_behaviour(
        "chat.window.the-close-button-shuts-its-own-window-and-writes-that-down",
        move |_| closed && written,
    );
    c.shutdown();
}
