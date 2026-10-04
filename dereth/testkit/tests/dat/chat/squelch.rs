use super::*;
// ---------------------------------------------------------------------------------------------
// The squelch tab
// ---------------------------------------------------------------------------------------------
//
// **No datagram leaves this process**: the shard's answer is built with the production writer and
// handed to the reader, and the two sends are read out of the client's own outbox as the bytes
// that *would* go out. Squelching somebody is real state on a live shard and nothing here does it.

/// The social page, and the recording whose login puts a real character behind the tab.
const SOCIAL_PAGE_ID: ElementId = ElementId(0x1000_018F);
const SOCIAL_BUTTON_ID: u32 = 0x0C;
const LOGIN_SESSION: &str = "long-solo-play";

/// Replay the recorded login up to the moment the shard creates the player, so that the tab is
/// driven with a live character the way it is in the running client.
///
/// The blob to stop at is **found** rather than pinned: it is the first one that creates a player.
fn corpus_login(c: &mut HeadlessClient) {
    let corpus = Corpus::load(LOGIN_SESSION)
        .expect("the corpus decodes")
        .expect("the corpus holds the recording");
    let end = corpus
        .blobs
        .iter()
        .find(|b| b.dir == Direction::ServerToClient && b.opcode == 0xF746)
        .map(|b| b.idx)
        .expect("the recorded login creates a player");
    c.when(Inbound::from_corpus(LOGIN_SESSION, 0..end + 1));
    assert!(
        c.view().world().player.is_some(),
        "the recorded login reaches the player's creation"
    );
    c.tick(2);
}

/// Run `f` against the shipped gameplay screen's root and the shell it was laid out in.
fn on_the_shipped_tree<T>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut dereth_ui::UiSystem, ElemHandle) -> T,
) -> T {
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the UI shell is up");
    let root = {
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen")
            .root()
            .expect("the gameplay screen's root")
    };
    f(&mut shell.ui, root)
}

/// The squelch tab as the client holds it.
fn squelch_panel(c: &HeadlessClient) -> &dereth_ui_screens::panels::squelch::SquelchPanel {
    &c.view().expect_app().hud().panels.squelch
}

/// `(squelch this character, squelch this account, remove)` as a player sees them -- read off the
/// live elements and not off the tab's own mirror, because a tab that worked out the right answer
/// and wrote it nowhere is exactly what this is about.
fn squelch_buttons(c: &HeadlessClient) -> (bool, bool, bool) {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the shell is up").ui;
    let p = &app.hud().panels.squelch;
    let read = |h: Option<ElemHandle>| {
        h.is_some_and(|h| dereth_ui_screens::panels::squelch::button_enabled(ui, h))
    };
    (
        read(p.squelch_character_button),
        read(p.squelch_account_button),
        read(p.remove_button),
    )
}

fn squelch_name_box(c: &mut HeadlessClient) -> String {
    let entry = squelch_panel(c).name_entry.expect("the name box is bound");
    c.app_mut()
        .ui_mut()
        .expect("the shell is up")
        .ui
        .text_element_mut(entry)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// Open the social page and then the squelch tab, by real presses. The tab is **discovered** off
/// the live tab table -- the page whose subtree carries the squelch list -- rather than named, so a
/// layout change fails loudly instead of quietly binding the wrong caption.
fn open_the_squelch_tab(c: &mut HeadlessClient, hand: &mut Hand) {
    let button = {
        let app = c.view().expect_app();
        let any: &dyn std::any::Any = app
            .ui()
            .expect("the shell")
            .flow
            .current()
            .expect("a screen");
        any.downcast_ref::<GamePlayScreen>()
            .expect("the gameplay screen")
            .toolbar
            .buttons
            .iter()
            .find(|b| b.panel_id == SOCIAL_BUTTON_ID)
            .expect("one of the shipped toolbar's buttons opens the social page")
            .handle
    };
    hand.click_handle(c, button);
    let tab = on_the_shipped_tree(c, |ui, root| {
        let page = ui
            .get_child_recursive(root, SOCIAL_PAGE_ID)
            .expect("the social page");
        let pairs: Vec<(ElementId, ElementId)> = ui
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .expect("the social page is a tabbed container")
            .page_to_tab
            .iter()
            .map(|(p, t)| (*p, *t))
            .collect();
        for (page_id, tab_id) in pairs {
            let Some(pe) = ui.get_child_recursive(page, page_id) else {
                continue;
            };
            if ui
                .get_child_recursive(pe, dereth_ui_screens::panels::squelch::SQUELCH_LIST)
                .is_some()
            {
                return ui
                    .get_child_recursive(page, tab_id)
                    .expect("the tab caption");
            }
        }
        panic!("no page of the social panel carries the squelch list");
    });
    hand.click_handle(c, tab);
    let h = squelch_panel(c)
        .panel
        .expect("the squelch tab is in the shipped tree");
    assert!(
        c.view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .is_visible(h),
        "clicking the squelch caption must bring the tab up"
    );
}

/// Put the caret in the name box with a real press, then type. The caret first: a keystroke only
/// reaches an element that has it, and a press is the only thing in the client that moves it.
fn type_into_the_squelch_box(c: &mut HeadlessClient, hand: &mut Hand, text: &str) {
    let entry = squelch_panel(c).name_entry.expect("the name box is bound");
    if c.view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .focus_element()
        != Some(entry)
    {
        hand.click_handle(c, entry);
        assert_eq!(
            c.view()
                .expect_app()
                .ui()
                .expect("the shell")
                .ui
                .focus_element(),
            Some(entry),
            "a press on the name box must take the caret"
        );
    }
    {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        for ch in text.encode_utf16() {
            ui.character(ch);
        }
    }
    c.tick(2);
}

/// One backspace, through the key the player presses. It is **not** the programmatic clear, and
/// the two paths differ.
fn backspace(c: &mut HeadlessClient, times: usize) {
    for _ in 0..times {
        {
            let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
            ui.key_press(&dereth_ui::focus::InputEvent {
                action: dereth_ui::focus::action::BACKSPACE,
                start: true,
                x: 0,
                y: 0,
            });
        }
        c.tick(1);
    }
}

/// One squelched character, as the shard names him.
fn squelched(name: &str, account: bool) -> dereth_protocol::comms::SquelchInfo {
    dereth_protocol::comms::SquelchInfo {
        squelch_msgs: dereth_protocol::comms::VLong(vec![
            0xFFFF_FFFF,
            0xFFFF_FFFF,
            0xFFFF_FFFF,
            0xFFFF_FFFF,
        ]),
        name: name.to_owned(),
        is_zone_squelch: i32::from(account),
    }
}

/// The whole squelch list the shard sends after every change it accepts.
fn squelch_list(
    characters: Vec<(u32, dereth_protocol::comms::SquelchInfo)>,
) -> dereth_protocol::comms::CommunicationSetSquelchDb {
    dereth_protocol::comms::CommunicationSetSquelchDb(dereth_protocol::comms::SquelchDb {
        // The shard sends this half always empty and folds an account squelch into the other one.
        account_hash: dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries: Vec::new(),
        },
        character_hash: dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries: characters,
        },
        global_squelch_info: dereth_protocol::comms::SquelchInfo {
            squelch_msgs: dereth_protocol::comms::VLong(vec![0, 0, 0, 0]),
            name: String::new(),
            is_zone_squelch: 0,
        },
    })
}

/// The squelch messages the client put in its outbox after `from`, as the bytes that would go out.
fn squelch_sent_since(c: &HeadlessClient, from: usize) -> Vec<Vec<u8>> {
    use dereth_protocol::Opcode;
    c.view().outbound()[from..]
        .iter()
        .filter_map(|r| {
            let (op, body) = match r {
                dereth_client_model::Request::ModifyCharacterSquelch(m) => (
                    Opcode::COMMUNICATION_MODIFY_CHARACTER_SQUELCH,
                    dereth_protocol::write_body(m),
                ),
                dereth_client_model::Request::ModifyAccountSquelch(m) => (
                    Opcode::COMMUNICATION_MODIFY_ACCOUNT_SQUELCH,
                    dereth_protocol::write_body(m),
                ),
                _ => return None,
            };
            let mut blob = op.0.to_le_bytes().to_vec();
            blob.extend(body.expect("the body encodes"));
            Some(blob)
        })
        .collect()
}

/// A client logged in from the recording, with the squelch tab open.
fn a_client_on_the_squelch_tab(hand: &mut Hand) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));
    corpus_login(&mut c);
    open_the_squelch_tab(&mut c, hand);
    c
}

/// Deliver the shard's whole squelch list and let the tab redraw.
fn deliver_squelch_list(
    c: &mut HeadlessClient,
    characters: Vec<(u32, dereth_protocol::comms::SquelchInfo)>,
) {
    c.when(Inbound::message(&squelch_list(characters)));
    c.tick(2);
}

/// The tab opens with both squelch buttons lit over an empty box, and remove dark.
pub fn the_squelch_tab_opens_with_both_buttons_lit() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    let empty = squelch_name_box(&mut c).is_empty();
    // Both squelch buttons are lit whatever the box says; remove is dark because nothing is
    // picked. This is what says the darkness below is not simply always right.
    let lit = squelch_buttons(&c) == (true, true, false);

    c.assert_behaviour(
        "chat.squelch-panel.the-tab-opens-with-both-buttons-lit-over-an-empty-box",
        move |_| empty && lit,
    );
    c.shutdown();
}

/// Typing in the name box arms both buttons and emptying it again darkens both -- which is the one
/// rule in the whole tab that reads the box, and it reads it only when a key was pressed.
pub fn emptying_the_squelch_name_box_dims_both_buttons() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);

    type_into_the_squelch_box(&mut c, &mut hand, "A");
    let typed = squelch_name_box(&mut c) == "A"
        && (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, true);

    backspace(&mut c, 1);
    let emptied = squelch_name_box(&mut c).is_empty()
        && (squelch_buttons(&c).0, squelch_buttons(&c).1) == (false, false);

    type_into_the_squelch_box(&mut c, &mut hand, "B");
    let re_armed = (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, true);

    c.assert_behaviour(
        "chat.squelch-panel.emptying-the-name-box-darkens-both-buttons-and-typing-arms-them",
        move |_| typed && emptied && re_armed,
    );
    c.shutdown();
}

/// Pressing one of the two squelch buttons empties the box, sends that button's own message, and
/// darkens that button and no other.
pub fn squelching_clears_the_box_and_dims_that_button_alone() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    type_into_the_squelch_box(&mut c, &mut hand, "Bob");
    let armed = squelch_buttons(&c).0;

    let b = squelch_panel(&c).squelch_character_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, b);
    let sent = squelch_sent_since(&c, before);
    let character = squelch_name_box(&mut c).is_empty()
        && sent.len() == 1
        && sent[0][0..4] == [0x58, 0, 0, 0]
        // The pressed button darkens; the other is still lit over an empty box, which is the
        // client's own asymmetry and not the same thing the box's own rule does.
        && (squelch_buttons(&c).0, squelch_buttons(&c).1) == (false, true);

    // The other button, the same way round.
    type_into_the_squelch_box(&mut c, &mut hand, "Dave");
    let b = squelch_panel(&c).squelch_account_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, b);
    let sent = squelch_sent_since(&c, before);
    let account = sent.len() == 1
        && sent[0][0..4] == [0x59, 0, 0, 0]
        && (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, false);

    c.assert_behaviour(
        "chat.squelch-panel.squelching-clears-the-box-and-darkens-that-button-alone",
        move |_| armed && character && account,
    );
    c.shutdown();
}

/// The shard's answer draws the new row and lights both buttons again -- so the darkness a player
/// sees after pressing one lasts exactly as long as the round trip.
pub fn the_shards_answer_lights_both_buttons_and_adds_the_row() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    type_into_the_squelch_box(&mut c, &mut hand, "Bob");
    let b = squelch_panel(&c).squelch_character_button.expect("bound");
    hand.click_handle(&mut c, b);
    let dark_in_flight = !squelch_buttons(&c).0;

    deliver_squelch_list(&mut c, vec![(0x5000_001E, squelched("Bob", false))]);
    let drawn = squelch_panel(&c).shown() == vec!["Bob".to_owned()];
    let lit_again = (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, true);

    c.assert_behaviour(
        "chat.squelch-panel.the-shards-answer-lights-both-buttons-again-and-adds-the-row",
        move |_| dark_in_flight && drawn && lit_again,
    );
    c.shutdown();
}

/// Picking a row in the list lights both squelch buttons again, over an empty box.
///
/// **This is the client's own behaviour and the scenario says so out loud.** Picking a row does
/// not look at the name box at all, so a darkness the box's own rule caused is undone by it. If
/// the buttons are ever made to follow the box whatever happened, this is what
/// will go red -- which is what makes that a decision somebody takes rather than a drift somebody
/// notices.
pub fn picking_a_squelch_row_lights_both_buttons_again() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    deliver_squelch_list(
        &mut c,
        vec![
            (0x5000_001E, squelched("Bob", false)),
            (0x5000_001F, squelched("Carol", false)),
        ],
    );
    let two_rows = squelch_panel(&c).shown() == vec!["Bob".to_owned(), "Carol".to_owned()];

    // Darken one button the only way a player can: type a name and press it.
    type_into_the_squelch_box(&mut c, &mut hand, "Dave");
    let b = squelch_panel(&c).squelch_character_button.expect("bound");
    hand.click_handle(&mut c, b);
    let darkened = squelch_name_box(&mut c).is_empty()
        && (squelch_buttons(&c).0, squelch_buttons(&c).1) == (false, true);

    // Now the gesture.
    let (list, row) = squelch_panel(&c)
        .list
        .as_ref()
        .and_then(|l| Some((l.handle, l.items.first().copied()?)))
        .expect("the list drew its rows");
    hand.click_row(&mut c, list, row);
    let picked = squelch_panel(&c).selected == Some(0);
    let box_still_empty = squelch_name_box(&mut c).is_empty();
    let lit_again = (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, true);

    c.assert_behaviour(
        "chat.squelch-panel.picking-a-row-lights-both-buttons-again-over-an-empty-box",
        move |_| two_rows && darkened && picked && box_still_empty && lit_again,
    );
    c.shutdown();
}

/// Remove follows what is picked, and the two squelch buttons are on another rule entirely.
///
/// It is worth asserting on its own: if a press on a row reached nothing at all, both squelch
/// buttons would still read lit -- they already were -- and the scenario above would pass while
/// measuring silence.
pub fn the_remove_button_follows_the_selection() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    let dark_to_start = !squelch_buttons(&c).2;

    deliver_squelch_list(&mut c, vec![(0x5000_001E, squelched("Bob", false))]);
    // A redraw picks nothing, so remove is still dark.
    let still_dark = !squelch_buttons(&c).2;

    let (list, row) = squelch_panel(&c)
        .list
        .as_ref()
        .and_then(|l| Some((l.handle, l.items.first().copied()?)))
        .expect("the list drew its row");
    hand.click_row(&mut c, list, row);
    let picked = squelch_panel(&c).selected == Some(0);
    let lit = squelch_buttons(&c).2;
    let others = (squelch_buttons(&c).0, squelch_buttons(&c).1) == (true, true);

    c.assert_behaviour(
        "chat.squelch-panel.remove-follows-what-is-picked",
        move |_| dark_to_start && still_dark && picked && lit && others,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// What the squelch tab lists, and what its buttons send
// ---------------------------------------------------------------------------------------------

/// The names the shipped squelch list holds, in the order a player sees them.
fn squelch_names(c: &mut HeadlessClient) -> Vec<String> {
    squelch_panel(c).shown()
}

/// What each row says about the kind of squelch it is, in list order.
fn squelch_row_states(c: &HeadlessClient) -> Vec<dereth_ui::StateId> {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the shell is up").ui;
    app.hud()
        .panels
        .squelch
        .rows()
        .iter()
        .filter_map(|r| {
            ui.get_child_recursive(r.element, dereth_ui_screens::panels::squelch::ROW_NAME)
        })
        .filter_map(|t| ui.node(t).map(|n| n.state))
        .collect()
}

/// The bytes one squelch-a-character message goes out in: whether it is being added or taken off,
/// a place for an id that is never used, the name, and the whole set of kinds of line. Composed
/// here rather than by the writer being asserted over.
fn character_squelch_bytes(add: u32, name: &str) -> Vec<u8> {
    let mut v = vec![0x58, 0x00, 0x00, 0x00];
    v.extend(add.to_le_bytes());
    v.extend(0_u32.to_le_bytes());
    v.extend(u16::try_from(name.len()).expect("short").to_le_bytes());
    v.extend(name.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v.extend(1_u32.to_le_bytes());
    v
}

/// The bytes one squelch-a-whole-account message goes out in: no id and no kind, only the name.
fn account_squelch_bytes(add: u32, name: &str) -> Vec<u8> {
    let mut v = vec![0x59, 0x00, 0x00, 0x00];
    v.extend(add.to_le_bytes());
    v.extend(u16::try_from(name.len()).expect("short").to_le_bytes());
    v.extend(name.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

/// The tab lists who the shard says is squelched, in one block sorted by name, with each row
/// saying whether it is a character or a whole account.
pub fn the_squelch_tab_lists_who_the_shard_says_is_squelched() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    // Before: bound, driven, and nothing on it -- which is a different reading from never run.
    let before = squelch_panel(&c).bound()
        && squelch_panel(&c).rebuilds >= 1
        && c.view().expect_app().hud().stats.squelch_db_applied == 0
        && squelch_names(&mut c).is_empty();

    // The empty list every recorded session's login carries: a real arrival that draws nothing.
    deliver_squelch_list(&mut c, Vec::new());
    let empty_arrived = c.view().expect_app().hud().stats.squelch_db_applied == 1
        && squelch_names(&mut c).is_empty();

    // A populated one, out of order, and with one whole account among the characters.
    deliver_squelch_list(
        &mut c,
        vec![
            (0x5000_0031, squelched("Zed", false)),
            (0x5000_001E, squelched("Ash", false)),
            (0x5000_001F, squelched("Bex", true)),
        ],
    );
    let held = c.view().world().chat.squelch.characters.len() == 3;
    // **One** block, sorted by name: unlike the friends list there is no second block for the
    // accounts, and they are mixed in with the characters.
    let sorted =
        squelch_names(&mut c) == vec!["Ash".to_owned(), "Bex".to_owned(), "Zed".to_owned()];
    let kinds = squelch_row_states(&c)
        == vec![
            dereth_ui_screens::panels::squelch::ROW_STATE_CHARACTER,
            dereth_ui_screens::panels::squelch::ROW_STATE_ACCOUNT,
            dereth_ui_screens::panels::squelch::ROW_STATE_CHARACTER,
        ];

    // Idempotent, and it flushes rather than appending.
    let rebuilds = squelch_panel(&c).rebuilds;
    c.tick(1);
    let unchanged = squelch_panel(&c).rebuilds == rebuilds && squelch_names(&mut c).len() == 3;

    c.assert_behaviour(
        "chat.squelch-panel.the-tab-lists-who-the-shard-says-is-squelched-in-one-sorted-block",
        move |_| before && empty_arrived && held && sorted && kinds && unchanged,
    );
    c.shutdown();
}

/// The two squelch buttons send two different messages about the same typed name, and the one for
/// a whole account is shorter by exactly the two things it has no room for.
pub fn the_two_squelch_buttons_send_different_messages() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);

    type_into_the_squelch_box(&mut c, &mut hand, "Bob");
    let b = squelch_panel(&c).squelch_character_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, b);
    let character = squelch_sent_since(&c, before) == vec![character_squelch_bytes(1, "Bob")]
        && squelch_panel(&c).character_squelch_requests == 1;

    type_into_the_squelch_box(&mut c, &mut hand, "Bob");
    let b = squelch_panel(&c).squelch_account_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, b);
    let sent = squelch_sent_since(&c, before);
    let account = sent == vec![account_squelch_bytes(1, "Bob")]
        && squelch_panel(&c).account_squelch_requests == 1;
    // With the same name, the account one is eight bytes shorter -- exactly the unused place for
    // an id and the set of kinds it does not carry.
    let shorter = sent[0].len() + 8 == character_squelch_bytes(1, "Bob").len();

    c.assert_behaviour(
        "chat.squelch-panel.the-two-buttons-send-different-messages-about-the-same-name",
        move |_| character && account && shorter,
    );
    c.shutdown();
}

/// Taking somebody off the list sends the kind of un-squelch the row itself names, so a whole
/// account and a single character are undone the way each was done.
pub fn removing_sends_the_kind_the_row_itself_names() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    deliver_squelch_list(
        &mut c,
        vec![
            (0x5000_001E, squelched("Ash", false)),
            (0x5000_001F, squelched("Bex", true)),
        ],
    );
    let two = squelch_names(&mut c) == vec!["Ash".to_owned(), "Bex".to_owned()];

    let list = squelch_panel(&c)
        .list
        .as_ref()
        .expect("the list is bound")
        .handle;
    let row = squelch_panel(&c).rows()[0].element;
    hand.click_row(&mut c, list, row);
    let picked = squelch_panel(&c).selected == Some(0);
    // The picked row is drawn in the state the list gives a picked row, so a player can see which
    // one it is; the other is not.
    let visibly_picked = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the shell").ui;
        let other = app.hud().panels.squelch.rows()[1].element;
        ui.node(row).map(|n| n.state.0) == Some(ui.get_attribute_enum(list, 0x5E).1)
            && ui.node(other).map(|n| n.state.0) != ui.node(row).map(|n| n.state.0)
    };

    let h = squelch_panel(&c).remove_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, h);
    let a_character = squelch_sent_since(&c, before) == vec![character_squelch_bytes(0, "Ash")]
        && squelch_panel(&c).remove_requests == 1;

    // The other row is the other arm of the same rule.
    let row = squelch_panel(&c).rows()[1].element;
    hand.click_row(&mut c, list, row);
    let picked_second = squelch_panel(&c).selected == Some(1);
    let h = squelch_panel(&c).remove_button.expect("bound");
    let before = mark(&c);
    hand.click_handle(&mut c, h);
    let an_account = squelch_sent_since(&c, before) == vec![account_squelch_bytes(0, "Bex")]
        && squelch_panel(&c).remove_requests == 2;

    c.assert_behaviour(
        "chat.squelch-panel.removing-sends-the-kind-the-row-itself-names",
        move |_| two && picked && visibly_picked && a_character && picked_second && an_account,
    );
    c.shutdown();
}

/// The name box takes the caret from a press, and once it has it there is a caret to draw.
pub fn the_squelch_name_box_takes_the_caret_from_a_press() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    let entry = squelch_panel(&c).name_entry.expect("the name box is bound");
    let not_yet = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .focus_element()
        != Some(entry);

    hand.click_handle(&mut c, entry);
    let took_it = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .focus_element()
        == Some(entry);

    let inside = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let (ox, oy) = ui.screen_origin(entry);
        let r = ui.node(entry).expect("alive").region.box_;
        let screen = dereth_ui::Box2D {
            x0: ox,
            y0: oy,
            x1: ox + r.width(),
            y1: oy + r.height(),
        };
        let caret = ui
            .text_element_mut(entry)
            .expect("the name box is a text element")
            .caret_box(screen)
            .expect("a box that can be typed in and has the caret has a caret to draw");
        caret.x0 >= screen.x0 && caret.x1 <= screen.x1 && caret.height() > 0
    };

    c.assert_behaviour(
        "chat.squelch-panel.the-name-box-takes-the-caret-from-a-press",
        move |_| not_yet && took_it && inside,
    );
    c.shutdown();
}

/// The label beside the name box is wider than the box it was drawn in, so it wraps on to two
/// lines -- **and that is what the shipped layout does**, not a defect in the drawing.
pub fn the_name_label_is_wider_than_its_box_and_wraps() {
    let mut hand = Hand::new();
    let mut c = a_client_on_the_squelch_tab(&mut hand);
    let label = on_the_shipped_tree(&mut c, |ui, root| {
        ui.get_child_recursive(root, ElementId(0x1000_053F))
            .expect("the label is in the tree")
    });

    let (box_, one_line_attr) = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the shell").ui;
        let n = ui.node(label).expect("alive");
        (
            n.region.box_,
            n.merged_properties()
                .0
                .get(&dereth_ui::props::attr::TEXT_ONE_LINE)
                .is_some(),
        )
    };
    // The shipped layout carries no instruction to keep it on one line; its neighbours do.
    let measured = (box_.x0, box_.x1) == (0, 99) && box_.height() == 18 && !one_line_attr;

    let (text, one_line, margins, glyphs) = {
        let ui = &mut c.app_mut().ui_mut().expect("the shell is up").ui;
        let t = ui.text_element_mut(label).expect("a text element");
        (
            t.glyphs.inq_text(false),
            t.glyphs.one_line,
            t.margins,
            t.glyphs.glyphs.clone(),
        )
    };
    let label_reads = text == "Character Name:" && !one_line && margins == (0, 0, 0, 6);
    let wider = glyphs.iter().map(|g| g.width).sum::<i32>() == 103;

    // The width it is laid out in, and what it does with it: it breaks at the space, and the space
    // stays on the first line while not counting towards its width.
    let width = box_.x1 - box_.x0 - margins.2 - margins.3 + 1;
    let lines = dereth_ui::text::glyph::wrap(&glyphs, width, false);
    let wraps = width == 94
        && lines.len() == 2
        // Nor does it fit in the whole box, so the margins are not what decides it.
        && dereth_ui::text::glyph::wrap(&glyphs, box_.width(), false).len() == 2
        && lines.iter().map(|l| l.width).collect::<Vec<_>>() == vec![60, 39];
    let first = lines[0];
    let space_stays = glyphs[first.start..first.end]
        .iter()
        .map(|g| g.width)
        .sum::<i32>()
        == 64
        && glyphs[first.end - 1].is_white_space()
        && 64 - 60 == glyphs[first.end - 1].width
        && lines[1].end == glyphs.len();

    c.assert_behaviour(
        "chat.squelch-panel.the-name-label-is-wider-than-its-box-and-wraps-in-retail-too",
        move |_| measured && label_reads && wider && wraps && space_stays,
    );
    c.shutdown();
}
