//! Shell fixtures and scenarios for login.

use super::*;
use dereth_ui_screens::screens::chargen;
// =============================================================================================
// The shell with no client under it
//
// `ClientSpec::shell(host)` is a backend and not a subject: the UI shell over the retail dats,
// driven against a `HostState` the scenario writes, with no `App` at all. Its one row keeps the
// `shell-only` id.
//
// The three tests at the end are **harness self-proofs and not census rows**: they make no claim
// about the client, they prove three pieces of the harness do what they say. `goldens.rs` is the
// precedent for a `dat` test that is not a behaviour.
// =============================================================================================

/// Three characters in an order that is neither alphabetical nor the one the scenario picks, so
/// that each of the list's three fallbacks would choose a **different** row: the char-gen slot is
/// unset, the remembered pick is the third name, and the first live row in the shard's own order
/// is the first. If the fallback and the answer were the same row this could not fail.
fn a_character_set() -> dereth_ui::persist::CharacterSet {
    let named = |gid: u32, name: &str| dereth_protocol::login::CharacterIdentity {
        gid: dereth_primitives::ObjectId(gid),
        name: name.into(),
        seconds_greyed_out: 0,
    };
    dereth_client::ui::character_set_from_login(&dereth_protocol::login::LoginCharacterSet {
        status: 0,
        characters: vec![
            named(0x5000_0001, "Zoranth"),
            named(0x5000_0002, "Ailinn"),
            named(0x5000_0003, "Borumar"),
        ],
        deleted: vec![],
        num_allowed_characters: 5,
        account: "acct0001".into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 0,
    })
}

/// What the application would be telling the shell: connected, patched, and here is the list.
fn a_host_at_character_select() -> HostState {
    HostState {
        has_packet_controller: true,
        connected: true,
        patch_finished: true,
        received_set: true,
        character_set: Some(a_character_set()),
        character_set_notices: 1,
        ..HostState::default()
    }
}

fn with_charmgmt<T>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut CharacterManagementScreen) -> T,
) -> T {
    let shell = c.expect_shell();
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    f(any
        .downcast_mut::<CharacterManagementScreen>()
        .expect("the character-management screen"))
}

/// The name of the row the list is currently highlighting.
fn selected_name(c: &mut HeadlessClient) -> Option<String> {
    with_charmgmt(c, |s| s.selected_row().map(|r| r.name.clone()))
}

/// Click the row carrying `name` -- through the row **element**, which is what a player's pointer
/// lands on, and not through the screen's own `select_character`.
fn click_the_row(c: &mut HeadlessClient, name: &str) {
    let row = with_charmgmt(c, |s| {
        s.rows
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("{name} is in the rebuilt list"))
            .element
            .unwrap_or_else(|| panic!("{name}'s row has an element"))
    });
    c.expect_shell().ui.broadcast_element_message(
        row,
        dereth_ui::msg::element::id::BUTTON_CLICKED,
        0,
        0,
    );
    c.tick(2);
}

// -------------------------------------------------------------------------------------------
// shell-only.character-select.the-returning-list-selects-the-character-just-played
// -------------------------------------------------------------------------------------------

/// The whole trip, as one scenario: the list comes up on its own fallback, the player picks
/// somebody else, the world comes up and throws the whole framework away, and the log-off brings
/// the list back.
pub(super) fn the_returning_character_list_selects_the_character_that_was_just_played() {
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::CHARACTER_MANAGEMENT, 30),
    );
    c.tick(2);

    // The premise, asserted so the ending is a measurement and not a coincidence: with nothing
    // remembered the list falls back to the first live row in the shard's own order.
    assert_eq!(
        selected_name(&mut c).as_deref(),
        Some("Zoranth"),
        "the list's last fallback is the first live row in the order the shard sent"
    );

    click_the_row(&mut c, "Borumar");
    assert_eq!(
        selected_name(&mut c).as_deref(),
        Some("Borumar"),
        "the click selected a row"
    );

    // Into the world. The mode switch destroys the current screen, so anything the *screen*
    // remembered is gone from here on.
    c.host_mut().in_world = true;
    c.tick(2);
    assert_eq!(
        c.expect_shell().flow.current_mode(),
        Some(mode::GAME_PLAY),
        "in the world"
    );

    // The log-off, which the shard answers with a second, identical character set. **This is the
    // step that needs this backend**: the set has not changed, so only the notice count says
    // anything happened, and an `App` has no way to be told either.
    c.host_mut().in_world = false;
    c.host_mut().character_set_notices = 2;
    c.tick(4);
    assert_eq!(
        c.expect_shell().flow.current_mode(),
        Some(mode::CHARACTER_MANAGEMENT),
        "back at character select"
    );

    let selected = selected_name(&mut c);
    c.assert_behaviour(
        "shell-only.character-select.the-returning-list-selects-the-character-just-played",
        move |_| selected.as_deref() == Some("Borumar"),
    );
}

// =============================================================================================
// The login flow, end to end
//
// Seven scenarios. Five of them drive a whole `App` with a shard on the other end of a socket-free
// endpoint: the character list arrives as a real message through the transport, and what the client
// sends back is read off its own outgoing datagrams rather than out of its memory. Two of them need
// the bare shell -- `ClientSpec::shell(HostState)` -- for the one thing an `App` cannot be told:
// that a second, *identical*, character list has arrived.
//
// Nothing here binds a socket. [`Peer`] is the harness's own shard and every datagram it writes
// is handed straight to the client's transport.
// =============================================================================================

/// The queue the shard's login messages ride on, which is what the client listens for a character
/// list on.
const LOGIN_QUEUE: u16 = 9;

/// The character this family deletes. It is the **first** the shard lists and sorts **last** by
/// name, so a client that confused a place in the sorted list with a place in the shard's own
/// could never pass here.
const WIRE_DOOMED: ObjectId = ObjectId(0x5000_0003);

/// The character this family restores. It is the **last** the shard lists and sorts into the
/// **middle**, so "back in its own place" is a claim that can fail: while it is waiting to be
/// deleted it is drawn at the bottom instead.
const WIRE_LAPSED: ObjectId = ObjectId(0x5000_0002);

/// The account the shard's list carries, which is the string a delete must carry back.
const WIRE_ACCOUNT: &str = "acct0001";

/// The shard's own list of three, with at most one of them waiting to be deleted.
fn wire_character_set(
    pending: Option<(ObjectId, u32)>,
) -> dereth_protocol::login::LoginCharacterSet {
    let named = |gid: ObjectId, name: &str| dereth_protocol::login::CharacterIdentity {
        gid,
        name: name.into(),
        seconds_greyed_out: match pending {
            Some((p, s)) if p == gid => s,
            _ => 0,
        },
    };
    dereth_protocol::login::LoginCharacterSet {
        status: 0,
        characters: vec![
            named(WIRE_DOOMED, "Zeddish"),
            named(ObjectId(0x5000_0001), "Larktest"),
            named(WIRE_LAPSED, "Tarinell"),
        ],
        deleted: vec![],
        num_allowed_characters: 5,
        account: WIRE_ACCOUNT.into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 0,
    }
}

/// The shard sends its list, and the screen rebuilds out of it.
fn the_shard_lists_the_characters(
    c: &mut HeadlessClient,
    peer: &mut Peer,
    pending: Option<(ObjectId, u32)>,
) {
    let blob = dereth_protocol::write_blob(&wire_character_set(pending)).expect("the list encodes");
    peer.send(c, LOGIN_QUEUE, blob);
    c.tick(3);
    assert_eq!(
        with_charmgmt_screen(c, |s| s.rows.len()),
        3,
        "the list on screen was rebuilt out of the one the shard really sent"
    );
}

/// A character screen whose list came **off the wire**, and the shard that sent it.
///
/// The endpoint goes on after the flow has reached the screen: the data-patch screen waits on a
/// connect meter only the real login exchange moves, and this shard deliberately does not run that
/// exchange.
fn a_character_list_from_the_wire(pending: Option<(ObjectId, u32)>) -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::screen(mode::CHARACTER_MANAGEMENT, 4));
    c.app_mut().probe_mut().host_state_mut().world_name = Some("ACEmulator".into());
    let mut peer = Peer::attach(&mut c, WIRE_DOOMED);
    the_shard_lists_the_characters(&mut c, &mut peer, pending);
    (c, peer)
}

/// Every fragment the client has really put on a datagram since the last look, as
/// `(queue, payload)`.
///
/// `HeadlessClient::outbound_wire` is the wrong reader for this family: it reports the sub-types
/// of the **ordered game actions** a client framed, and every message here -- the delete, the
/// restore, the creation -- is a bare control message with no ordered envelope at all.
fn fragments_sent(c: &mut HeadlessClient) -> Vec<(u16, Vec<u8>)> {
    let mut out = Vec::new();
    let taken = c
        .replay_net_mut()
        .expect("this scenario reads the wire, so its client has a shard attached")
        .take_outgoing();
    for (bytes, _) in taken {
        let Ok(packet) = dereth_transport::wire::ParsedPacket::parse(&bytes) else {
            continue;
        };
        for f in packet.fragments {
            out.push((f.header.queue_id, f.payload.clone()));
        }
    }
    out
}

/// The one fragment whose leading word is `op`, if the client sent one.
fn one_message(sent: &[(u16, Vec<u8>)], op: dereth_protocol::Opcode) -> Option<(u16, Vec<u8>)> {
    sent.iter()
        .find(|(_, p)| {
            p.get(..4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                == Some(op.0)
        })
        .cloned()
}

/// The colour a row is really drawn in -- the text element's own current font colour.
fn row_colour(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> u32 {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .map_or(0, |t| t.font_color)
}

/// Pick `who`, press delete, type `typed` into the box and accept it.
fn delete_the_character(c: &mut HeadlessClient, hands: &mut Hands, who: ObjectId, typed: &str) {
    let row = charmgmt_row(c, who);
    hands.click_handle(c, row);
    let delete = element(c, DELETE_BUTTON_ID);
    hands.click_handle(c, delete);
    let h = charmgmt_dialog(c, DialogContext::DeleteCharacter).expect("the question is raised");
    let field = charmgmt_child(c, h, charmgmt::TEXT_INPUT_FIELD);
    hands.click_handle(c, field);
    hands.type_text(c, typed);
    let accept = charmgmt_child(c, h, charmgmt::TEXT_INPUT_ACCEPT);
    hands.click_handle(c, accept);
    // The action leaves the screen in the press's own frame; the pass that puts it on a datagram
    // is the next one.
    c.tick(1);
}

/// Pick `who` and press restore, leaving the waiting box up. Answers whether the screen really
/// offered the button and really raised the box.
fn restore_the_character(c: &mut HeadlessClient, hands: &mut Hands, who: ObjectId) -> bool {
    let row = charmgmt_row(c, who);
    hands.click_handle(c, row);
    let offered = with_charmgmt_screen(c, |s| s.update_buttons().restore);
    let restore = element(c, RESTORE_BUTTON_ID);
    hands.click_handle(c, restore);
    let waiting = charmgmt_dialog(c, DialogContext::PleaseWait).is_some();
    c.tick(1);
    offered && waiting
}

/// The shard's answer to a delete: the bare acknowledgement, which carries no list at all.
fn the_shard_acknowledges_the_delete(c: &mut HeadlessClient, peer: &mut Peer) {
    let blob = dereth_protocol::write_blob(&dereth_protocol::login::CharacterDeleteAck::default())
        .expect("the acknowledgement encodes");
    peer.send(c, LOGIN_QUEUE, blob);
    c.tick(2);
}

/// The shard's answer to a restore: the identity that place in the list now holds.
fn the_shard_restores(c: &mut HeadlessClient, peer: &mut Peer, gid: ObjectId, name: &str) {
    let r = dereth_protocol::login::CharGenVerificationResponse {
        response_type: 1,
        identity: dereth_protocol::login::CharacterIdentity {
            gid,
            name: name.into(),
            seconds_greyed_out: 0,
        },
    };
    let blob = dereth_protocol::write_blob(&r).expect("the answer encodes");
    peer.send(c, LOGIN_QUEUE, blob);
    c.tick(2);
}

/// The shard's refusal of a restore or a creation: the reason and nothing after it, built by hand
/// so that this crate's own writer cannot define the oracle.
fn the_shard_refuses(c: &mut HeadlessClient, peer: &mut Peer, code: u32) {
    let mut blob = Vec::new();
    blob.extend_from_slice(
        &dereth_protocol::Opcode::CHARACTER_CHAR_GEN_VERIFICATION_RESPONSE
            .0
            .to_le_bytes(),
    );
    blob.extend_from_slice(&code.to_le_bytes());
    assert_eq!(
        blob.len(),
        8,
        "a refusal is the opcode and the reason, and nothing else"
    );
    peer.send(c, LOGIN_QUEUE, blob);
    c.tick(2);
}

/// The reason the shard gives when somebody else has taken the name.
const NAME_IN_USE: u32 = 3;

// ---------------------------------------------------------------------------------------------
// shell-only.log-off.the-shards-answer-does-not-end-a-client-that-has-a-screen-to-go-back-to
// ---------------------------------------------------------------------------------------------

/// The defect that made re-entry impossible in the most literal way there is: the process was
/// gone. Both directions, because a client that always keeps running and one that never does read
/// alike from one of them.
pub(super) fn the_shards_log_off_answer_does_not_end_a_client_with_screens() {
    // With a screen to go back to: thirty frames of still running.
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let _shard = Peer::attach(&mut c, ObjectId(0x5000_0001));
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::LoggedOff]);
    let mut still_running = true;
    for _ in 0..30 {
        still_running &= c.app_mut().frame();
    }
    let not_shutting_down =
        c.view().expect_app().state() != dereth_client::app::AppState::ShuttingDown;
    c.shutdown();

    // With none: the same answer ends the run, because there is nothing to return to and nothing
    // to take the player's next press.
    let mut c = HeadlessClient::new(ClientSpec::retail());
    let no_screens = c.view().expect_app().ui().is_none();
    let _shard = Peer::attach(&mut c, ObjectId(0x5000_0001));
    let running_before = c.app_mut().frame();
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::LoggedOff]);
    let ended = !c.app_mut().frame();

    c.assert_behaviour(
        "shell-only.log-off.the-shards-answer-does-not-end-a-client-that-has-a-screen-to-go-back-to",
        move |_| still_running && not_shutting_down && no_screens && running_before && ended,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shell-only.character-set.every-arrival-is-a-notice-even-when-the-list-has-not-changed
// ---------------------------------------------------------------------------------------------

/// A notice is a count, not a value: the shard re-sends the *same* list after a log-off, and a
/// client comparing the two would see nothing happen at the one moment something did.
pub(super) fn every_character_set_the_session_decodes_is_an_arrival() {
    let mut c = HeadlessClient::new(ClientSpec::screen(mode::CHARACTER_MANAGEMENT, 4));
    let _shard = Peer::attach(&mut c, ObjectId(0x5000_0001));
    let nothing_yet = c.view().expect_app().host_state().character_set_notices == 0;

    let set = wire_character_set(None);
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::CharacterSet(Box::new(set.clone()))]);
    let first = c.view().expect_app().host_state().character_set_notices == 1;

    // The second carries the **same** characters, which is what the shard sends after a log-off.
    let before = c.view().expect_app().host_state().character_set.clone();
    c.app_mut()
        .process_logon_event_queue(vec![SessionEvent::CharacterSet(Box::new(set))]);
    let unchanged = c.view().expect_app().host_state().character_set == before;
    let second = c.view().expect_app().host_state().character_set_notices == 2;

    c.assert_behaviour(
        "shell-only.character-set.every-arrival-is-a-notice-even-when-the-list-has-not-changed",
        move |_| nothing_yet && first && unchanged && second,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shell-only.character-set.a-list-arriving-in-the-world-sends-the-player-back-to-the-character-screen
// ---------------------------------------------------------------------------------------------

/// Both halves, because either alone is a different defect -- and the negative beside them,
/// because an arm that fired every frame would pass the first two.
pub(super) fn a_list_arriving_in_the_world_sends_the_player_back_and_at_the_list_does_not() {
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::CHARACTER_MANAGEMENT, 30),
    );
    c.tick(2);
    let received = c.expect_shell().flow.data.received_set;

    // At the character screen, a second list rebuilds the rows and moves the flow nowhere.
    c.host_mut().character_set_notices = 2;
    c.tick(4);
    let stays_at_the_list =
        c.expect_shell().flow.current_mode() == Some(mode::CHARACTER_MANAGEMENT);

    // Into the world, and then the same list again -- which no value comparison could see.
    c.expect_shell().queue(mode::GAME_PLAY);
    c.tick(1);
    let in_the_world = c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);
    c.host_mut().character_set_notices = 3;
    c.tick(1);
    let carried_back = c.expect_shell().flow.current_mode() == Some(mode::CHARACTER_MANAGEMENT);
    c.shutdown();

    // And with no arrival at all the player stays in the world, however long the client runs.
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::GAME_PLAY, 30),
    );
    let mut stays_in_the_world = c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);
    for _ in 0..30 {
        c.tick(1);
        stays_in_the_world &= c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);
    }

    c.assert_behaviour(
        "shell-only.character-set.a-list-arriving-in-the-world-sends-the-player-back-to-the-character-screen",
        move |_| {
            received && stays_at_the_list && in_the_world && carried_back && stays_in_the_world
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shell-only.enter-world.the-way-into-the-world-opens-again-after-a-log-off
// ---------------------------------------------------------------------------------------------

/// In, out, in. The middle leg is the whole scenario: the step into the world is taken on the
/// **rising** edge of being in it, so a log-off that never lowered that edge would leave the player
/// on the character screen with a world running behind it for ever.
pub(super) fn the_way_into_the_world_opens_again_after_a_log_off() {
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::CHARACTER_MANAGEMENT, 30),
    );
    c.tick(2);

    c.host_mut().in_world = true;
    c.tick(1);
    let first_entry = c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);

    c.host_mut().in_world = false;
    c.host_mut().character_set_notices = 2;
    c.tick(1);
    let back_at_the_list = c.expect_shell().flow.current_mode() == Some(mode::CHARACTER_MANAGEMENT);

    c.host_mut().in_world = true;
    c.tick(1);
    let second_entry = c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);

    c.assert_behaviour(
        "shell-only.enter-world.the-way-into-the-world-opens-again-after-a-log-off",
        move |_| first_entry && back_at_the_list && second_entry,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shell-only.log-off.answering-yes-asks-to-log-off-and-moves-no-screen-of-its-own
// ---------------------------------------------------------------------------------------------

/// Press an element of the bare shell by id, through the arena rather than by walking a root: a
/// dialog's buttons are children of the dialog the screen raised and not of the screen itself.
fn shell_click(c: &mut HeadlessClient, id: ElementId) {
    let shell = c.expect_shell();
    let h = shell
        .ui
        .get_element(id)
        .unwrap_or_else(|| panic!("{:#X} is in the shipped layout", id.0));
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
}

/// *Exit to Character Selection*, then *Yes*: the host is asked to log off and nothing on screen
/// moves in that frame, which is the window the leaving is played out in.
pub(super) fn answering_yes_asks_to_log_off_and_moves_no_screen_of_its_own() {
    use dereth_ui_screens::screens::gameplay::{logout, GamePlayScreen};

    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::GAME_PLAY, 30),
    );
    // Anything the shell raised on the way here is taken now, so every later read is about this
    // scenario's own gesture.
    let _ = c.expect_shell().take_log_off();

    shell_click(&mut c, logout::EXIT_TO_CHARACTER_SELECTION);
    c.tick(1);
    let asked_first = {
        let shell = c.expect_shell();
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen is up")
            .logout_dialog()
            .is_some()
    };

    shell_click(&mut c, logout::BUTTON_YES);
    c.tick(1);
    let no_screen_moved = c.expect_shell().flow.current_mode() == Some(mode::GAME_PLAY);
    let the_host_was_asked = c.expect_shell().take_log_off();

    c.assert_behaviour(
        "shell-only.log-off.answering-yes-asks-to-log-off-and-moves-no-screen-of-its-own",
        move |_| asked_first && no_screen_moved && the_host_was_asked,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// shell-only.character-select.a-pick-the-returning-list-cannot-honour-falls-back-to-the-shards-own-order
// ---------------------------------------------------------------------------------------------

/// The other direction of the remembered pick, without which a client that simply repeats the
/// *last* thing it was told reads the same as one that remembers a choice.
pub(super) fn a_pick_the_returning_list_cannot_honour_falls_back_to_the_shards_own_order() {
    // Nothing was ever picked: out and back, and the fallback still decides.
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::CHARACTER_MANAGEMENT, 30),
    );
    c.tick(2);
    let opens_on_the_fallback = selected_name(&mut c).as_deref() == Some("Zoranth");
    c.host_mut().in_world = true;
    c.tick(2);
    c.host_mut().in_world = false;
    c.host_mut().character_set_notices = 2;
    c.tick(4);
    let back_at_the_list = c.expect_shell().flow.current_mode() == Some(mode::CHARACTER_MANAGEMENT);
    let nothing_remembered = selected_name(&mut c).as_deref() == Some("Zoranth");
    c.shutdown();

    // And a pick the next list has lost: driven as the deletion it would be.
    let mut c = HeadlessClient::new(
        ClientSpec::shell(a_host_at_character_select()).on_mode(mode::CHARACTER_MANAGEMENT, 30),
    );
    c.tick(2);
    click_the_row(&mut c, "Borumar");
    let picked = selected_name(&mut c).as_deref() == Some("Borumar");
    let mut shorter = a_character_set();
    shorter.set.retain(|ch| ch.name != "Borumar");
    c.host_mut().character_set = Some(shorter);
    c.host_mut().character_set_notices = 2;
    c.tick(3);
    let fell_back = selected_name(&mut c).as_deref() == Some("Zoranth");

    c.assert_behaviour(
        "shell-only.character-select.a-pick-the-returning-list-cannot-honour-falls-back-to-the-shards-own-order",
        move |_| {
            opens_on_the_fallback && back_at_the_list && nothing_remembered && picked && fell_back
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.delete.the-request-that-leaves-the-client-carries-the-account-and-the-shards-own-place
// ---------------------------------------------------------------------------------------------

/// The whole message, byte for byte, and the one thing a sorted list box can get wrong: **which
/// place**. `WIRE_DOOMED` is the shard's first character and the screen's third row, so a client
/// that sent the row would be asking the shard to destroy somebody else.
pub(super) fn the_delete_that_leaves_the_client_carries_the_account_and_the_shards_own_place() {
    let (mut c, _peer) = a_character_list_from_the_wire(None);
    let mut hands = Hands::new();
    let phrase = charmgmt_word(&c, charmgmt::DELETE_RESPONSE_STRING);

    // The premise, asserted rather than assumed: the two numbers really are different here.
    let row_index =
        with_charmgmt_screen(&mut c, |s| s.rows.iter().position(|r| r.id == WIRE_DOOMED));
    let set_index = with_charmgmt_screen(&mut c, |s| {
        s.char_set.set.iter().position(|ch| ch.id == WIRE_DOOMED)
    });
    let they_differ = row_index == Some(2) && set_index == Some(0);

    let _ = fragments_sent(&mut c);
    delete_the_character(&mut c, &mut hands, WIRE_DOOMED, &phrase);

    let out = fragments_sent(&mut c);
    let sent = one_message(&out, dereth_protocol::Opcode::CHARACTER_CHARACTER_DELETE);
    let mut want = dereth_protocol::Opcode::CHARACTER_CHARACTER_DELETE
        .0
        .to_le_bytes()
        .to_vec();
    want.extend(
        dereth_protocol::write_body(&dereth_protocol::login::CharacterDeleteRequest {
            account: WIRE_ACCOUNT.to_owned(),
            slot_index: 0,
        })
        .expect("the request encodes"),
    );
    let on_the_log_on_queue = sent.as_ref().map(|(q, _)| *q) == Some(4);
    let byte_for_byte = sent.as_ref().map(|(_, p)| p.clone()) == Some(want);

    // Read back the way the shard reads it, which is the only reading that matters.
    let read_back = sent.as_ref().and_then(|(_, p)| {
        dereth_protocol::read_body::<dereth_protocol::login::CharacterDeleteRequest>(&p[4..]).ok()
    });
    let names_the_account_and_the_place = read_back
        .as_ref()
        .is_some_and(|r| r.account == WIRE_ACCOUNT && r.slot_index == 0);

    // ...and the waiting box is up by the time the request exists.
    let waiting = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_some();

    c.assert_behaviour(
        "character-select.delete.the-request-that-leaves-the-client-carries-the-account-and-the-shards-own-place",
        move |_| {
            they_differ
                && on_the_log_on_queue
                && byte_for_byte
                && names_the_account_and_the_place
                && waiting
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.delete.a-pending-deletion-is-drawn-red-and-last-and-offers-restore
// ---------------------------------------------------------------------------------------------

/// What the player sees once the shard has answered: the acknowledgement alone changes nothing,
/// the fresh list is what takes the waiting box down, and the row is then red, last, and offers
/// to be restored rather than played.
///
/// It is also where the premise *"a pending row says how long it has left"* is falsified: the row
/// says the name and nothing else.
pub(super) fn a_pending_deletion_is_drawn_red_and_last_and_offers_restore() {
    let (mut c, mut peer) = a_character_list_from_the_wire(None);
    let mut hands = Hands::new();
    let phrase = charmgmt_word(&c, charmgmt::DELETE_RESPONSE_STRING);
    delete_the_character(&mut c, &mut hands, WIRE_DOOMED, &phrase);

    // The acknowledgement carries no list, so the box the player cannot dismiss is still up.
    the_shard_acknowledges_the_delete(&mut c, &mut peer);
    let still_waiting = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_some();

    // The fresh list is what takes it down.
    the_shard_lists_the_characters(&mut c, &mut peer, Some((WIRE_DOOMED, 3_600)));
    let taken_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();

    let rows = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let kept_its_place_in_the_list = rows.len() == 3;
    let last_and_alone = rows[2].id == WIRE_DOOMED
        && rows[2].greyed_out
        && !rows[0].greyed_out
        && !rows[1].greyed_out;

    let doomed_h = rows[2].element.expect("the row has an element");
    let live_h = rows[0].element.expect("the row has an element");
    let drawn_red = row_colour(&mut c, doomed_h) == charmgmt::GREYED_OUT_COLOR
        && row_colour(&mut c, live_h) != charmgmt::GREYED_OUT_COLOR;
    let says_only_the_name = wizard_text(&mut c, doomed_h) == "Zeddish";

    let row = charmgmt_row(&mut c, WIRE_DOOMED);
    hands.click_handle(&mut c, row);
    let buttons = with_charmgmt_screen(&mut c, |s| s.update_buttons());
    let offers_restore =
        buttons.restore && !buttons.delete && !buttons.enter_game && buttons.create;

    c.assert_behaviour(
        "character-select.delete.a-pending-deletion-is-drawn-red-and-last-and-offers-restore",
        move |_| {
            still_waiting
                && taken_down
                && kept_its_place_in_the_list
                && last_and_alone
                && drawn_red
                && says_only_the_name
                && offers_restore
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.restore.the-request-that-leaves-the-client-is-the-characters-own-id-on-the-control-queue
// ---------------------------------------------------------------------------------------------

/// The restore's whole message: the id and two empty names, on the control queue. A place in the
/// list on this wire would find the shard nothing at all, and silently.
pub(super) fn the_restore_that_leaves_the_client_is_the_characters_own_id_on_the_control_queue() {
    let (mut c, _peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
    let mut hands = Hands::new();

    let _ = fragments_sent(&mut c);
    let asked = restore_the_character(&mut c, &mut hands, WIRE_LAPSED);

    let out = fragments_sent(&mut c);
    let sent = one_message(
        &out,
        dereth_protocol::Opcode::ADMIN_SEND_ADMIN_RESTORE_CHARACTER,
    );
    let mut want = dereth_protocol::Opcode::ADMIN_SEND_ADMIN_RESTORE_CHARACTER
        .0
        .to_le_bytes()
        .to_vec();
    want.extend(
        dereth_protocol::write_body(&dereth_protocol::admin::AdminSendAdminRestoreCharacter {
            iid: WIRE_LAPSED,
            restored_char_name: String::new(),
            account_to_restore_to: String::new(),
        })
        .expect("the request encodes"),
    );
    let on_the_control_queue = sent.as_ref().map(|(q, _)| *q) == Some(2);
    let byte_for_byte = sent.as_ref().map(|(_, p)| p.clone()) == Some(want);

    let read_back = sent.as_ref().and_then(|(_, p)| {
        dereth_protocol::read_body::<dereth_protocol::admin::AdminSendAdminRestoreCharacter>(
            &p[4..],
        )
        .ok()
    });
    let names_the_character = read_back.as_ref().is_some_and(|r| {
        r.iid == WIRE_LAPSED
            && r.restored_char_name.is_empty()
            && r.account_to_restore_to.is_empty()
    });
    // ...and it is the id and not the place: the place is 2 here, and the id is not.
    let not_a_place = read_back.as_ref().is_some_and(|r| r.iid.0 != 2);

    c.assert_behaviour(
        "character-select.restore.the-request-that-leaves-the-client-is-the-characters-own-id-on-the-control-queue",
        move |_| asked && on_the_control_queue && byte_for_byte && names_the_character && not_a_place,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.restore.a-restored-character-comes-back-in-its-own-place-and-in-the-ordinary-colour
// ---------------------------------------------------------------------------------------------

/// The check a player makes, drawn: the waiting box goes, the list is no longer, the row is back in
/// the middle where its name sorts, in the colour every other row is drawn in, and offers to be
/// deleted or played.
pub(super) fn a_restored_character_comes_back_in_its_own_place_and_in_the_ordinary_colour() {
    let (mut c, mut peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
    let mut hands = Hands::new();

    // Pending: last, and red. Asserted, so that the ending is a measurement.
    let before = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let pending_h = before[2].element.expect("the row has an element");
    let started_last_and_red =
        before[2].id == WIRE_LAPSED && row_colour(&mut c, pending_h) == charmgmt::GREYED_OUT_COLOR;

    restore_the_character(&mut c, &mut hands, WIRE_LAPSED);
    the_shard_restores(&mut c, &mut peer, WIRE_LAPSED, "Tarinell");

    let taken_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none()
        && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_none();
    let after = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let replaced_rather_than_added = after.len() == 3;
    let back_in_its_own_place = after.iter().map(|r| r.name.clone()).collect::<Vec<_>>()
        == ["Larktest", "Tarinell", "Zeddish"]
        && after.iter().all(|r| !r.greyed_out);

    let restored_h = after[1].element.expect("the row has an element");
    let live_h = after[0].element.expect("the row has an element");
    let drawn_like_the_others = row_colour(&mut c, restored_h) != charmgmt::GREYED_OUT_COLOR
        && row_colour(&mut c, restored_h) == row_colour(&mut c, live_h);

    let row = charmgmt_row(&mut c, WIRE_LAPSED);
    hands.click_handle(&mut c, row);
    let buttons = with_charmgmt_screen(&mut c, |s| s.update_buttons());
    let playable_again = buttons.delete && !buttons.restore && buttons.enter_game;

    c.assert_behaviour(
        "character-select.restore.a-restored-character-comes-back-in-its-own-place-and-in-the-ordinary-colour",
        move |_| {
            started_last_and_red
                && taken_down
                && replaced_rather_than_added
                && back_in_its_own_place
                && drawn_like_the_others
                && playable_again
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.restore.the-answer-overwrites-the-place-it-was-asked-about-and-never-appends
// ---------------------------------------------------------------------------------------------

/// The answer is addressed by the **place** the client asked about, and whatever identity it
/// carries is written over that place -- name included. That is not a quirk to design around: it
/// is how a character brought back under another name reaches the list at all.
pub(super) fn the_restore_answer_overwrites_the_place_it_was_asked_about_and_never_appends() {
    let (mut c, mut peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
    let mut hands = Hands::new();

    restore_the_character(&mut c, &mut hands, WIRE_LAPSED);
    // A different name *and* a different id, which is the sharpest form of the same rule.
    the_shard_restores(&mut c, &mut peer, ObjectId(0x5000_00FF), "Somebodyelse");

    let rows = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let nothing_appended = rows.len() == 3;
    let the_place_was_overwritten = rows.iter().all(|r| r.id != WIRE_LAPSED);
    let carries_what_the_answer_said = rows
        .iter()
        .find(|r| r.name == "Somebodyelse")
        .is_some_and(|r| !r.greyed_out);
    let sorted_like_any_other_row = rows.iter().map(|r| r.name.clone()).collect::<Vec<_>>()
        == ["Larktest", "Somebodyelse", "Zeddish"];

    c.assert_behaviour(
        "character-select.restore.the-answer-overwrites-the-place-it-was-asked-about-and-never-appends",
        move |_| {
            nothing_appended
                && the_place_was_overwritten
                && carries_what_the_answer_said
                && sorted_like_any_other_row
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.restore.a-refusal-takes-the-waiting-box-down-and-says-why-in-the-shipped-words
// ---------------------------------------------------------------------------------------------

/// The half that leaves a player stuck when it is missing: the shard answers a restore with a
/// refusal and **no list at all**, so the box the player cannot dismiss has only this one way
/// down. A refusal shows why; a success shows nothing; and a second refusal carrying the same
/// reason as the first still takes the second box down.
pub(super) fn a_refused_restore_takes_the_waiting_box_down_and_says_why() {
    let want = {
        let (c, _peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
        let s = charmgmt_word(&c, charmgmt::CHARGEN_VERIFICATION_STRINGS[0]);
        c.shutdown();
        s
    };

    let (mut c, mut peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
    let mut hands = Hands::new();

    restore_the_character(&mut c, &mut hands, WIRE_LAPSED);
    the_shard_refuses(&mut c, &mut peer, NAME_IN_USE);

    let taken_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();
    let told_why = charmgmt_dialog(&mut c, DialogContext::ErrorMessage).is_some()
        && with_charmgmt_screen(&mut c, |s| s.error_text.clone()).as_deref() == Some(want.as_str());
    // A refusal writes nothing: the character is still waiting to be deleted.
    let still_pending = with_charmgmt_screen(&mut c, |s| {
        s.rows
            .iter()
            .find(|r| r.id == WIRE_LAPSED)
            .is_some_and(|r| r.greyed_out)
    });

    // The same reason a second time still takes the second box down -- a notice is a count.
    let h = charmgmt_dialog(&mut c, DialogContext::ErrorMessage).expect("the message box");
    let button = charmgmt_child(&c, h, charmgmt::MESSAGE_BUTTON);
    hands.click_handle(&mut c, button);
    c.tick(1);
    restore_the_character(&mut c, &mut hands, WIRE_LAPSED);
    the_shard_refuses(&mut c, &mut peer, NAME_IN_USE);
    let second_taken_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();
    c.shutdown();

    // And a restore that works shows no message at all, on a client of its own.
    let (mut c, mut peer) = a_character_list_from_the_wire(Some((WIRE_LAPSED, 3_600)));
    let mut hands = Hands::new();
    restore_the_character(&mut c, &mut hands, WIRE_LAPSED);
    the_shard_restores(&mut c, &mut peer, WIRE_LAPSED, "Tarinell");
    let success_says_nothing = charmgmt_dialog(&mut c, DialogContext::ErrorMessage).is_none()
        && with_charmgmt_screen(&mut c, |s| s.open_dialog).is_none();

    c.assert_behaviour(
        "character-select.restore.a-refusal-takes-the-waiting-box-down-and-says-why-in-the-shipped-words",
        move |_| {
            taken_down && told_why && still_pending && second_taken_down && success_says_nothing
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// character-select.round-trip.deleting-and-restoring-over-the-wire-leaves-the-list-where-it-started
// ---------------------------------------------------------------------------------------------

/// Delete, then restore, both over real datagrams. The asymmetry between the two answers is the
/// whole story: a delete is followed by a fresh list and a restore is followed by nothing else,
/// so one of the two waiting boxes has to be taken down by the answer itself.
pub(super) fn deleting_and_restoring_over_the_wire_leaves_the_list_where_it_started() {
    let (mut c, mut peer) = a_character_list_from_the_wire(None);
    let mut hands = Hands::new();
    let phrase = charmgmt_word(&c, charmgmt::DELETE_RESPONSE_STRING);

    let started_live = with_charmgmt_screen(&mut c, |s| s.rows.iter().all(|r| !r.greyed_out));

    let _ = fragments_sent(&mut c);
    delete_the_character(&mut c, &mut hands, WIRE_DOOMED, &phrase);
    let asked_to_delete = one_message(
        &fragments_sent(&mut c),
        dereth_protocol::Opcode::CHARACTER_CHARACTER_DELETE,
    )
    .is_some();
    let waiting_on_the_delete = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_some();

    the_shard_acknowledges_the_delete(&mut c, &mut peer);
    the_shard_lists_the_characters(&mut c, &mut peer, Some((WIRE_DOOMED, 3_600)));
    let the_list_took_that_box_down = charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none()
        && with_charmgmt_screen(&mut c, |s| {
            s.rows
                .iter()
                .find(|r| r.id == WIRE_DOOMED)
                .is_some_and(|r| r.greyed_out)
        });

    let _ = fragments_sent(&mut c);
    restore_the_character(&mut c, &mut hands, WIRE_DOOMED);
    let asked_to_restore = one_message(
        &fragments_sent(&mut c),
        dereth_protocol::Opcode::ADMIN_SEND_ADMIN_RESTORE_CHARACTER,
    )
    .is_some();

    // No list follows a restore, so this answer is the only thing that can take its box down.
    the_shard_restores(&mut c, &mut peer, WIRE_DOOMED, "Zeddish");
    let the_answer_took_this_one_down =
        charmgmt_dialog(&mut c, DialogContext::PleaseWait).is_none();

    let rows = with_charmgmt_screen(&mut c, |s| s.rows.clone());
    let row = charmgmt_row(&mut c, WIRE_DOOMED);
    hands.click_handle(&mut c, row);
    let where_it_started = rows.len() == 3
        && rows.iter().all(|r| !r.greyed_out)
        && with_charmgmt_screen(&mut c, |s| s.update_buttons().delete);

    c.assert_behaviour(
        "character-select.round-trip.deleting-and-restoring-over-the-wire-leaves-the-list-where-it-started",
        move |_| {
            started_live
                && asked_to_delete
                && waiting_on_the_delete
                && the_list_took_that_box_down
                && asked_to_restore
                && the_answer_took_this_one_down
                && where_it_started
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.finish.a-refused-creation-stops-the-wizard-waiting-and-says-why
// ---------------------------------------------------------------------------------------------

/// The check a player makes: Finish with a name somebody already has. The request goes out, the
/// wizard waits, the shard's refusal comes back and the wizard says which refusal it was, with a
/// button that closes it -- and the same is true of the reason that shares the client's general
/// arm, which must not leave nothing on the screen at all.
pub(super) fn a_refused_creation_stops_the_wizard_waiting_and_says_why() {
    use dereth_chargen::CgVerification;
    use dereth_ui_screens::screens::chargen::{CharGenDialog, MESSAGE_BUTTON};

    // The name already taken.
    let mut c = a_client_on_the_wizard();
    let mut peer = Peer::attach(&mut c, ObjectId(0x5000_0001));
    walk_the_wizard(&mut c);
    let _ = fragments_sent(&mut c);
    click_wizard(&mut c, chargen::FINISH_BUTTON);
    c.tick(2);

    let asked = one_message(
        &fragments_sent(&mut c),
        dereth_protocol::Opcode::CHARACTER_SEND_CHAR_GEN_RESULT,
    )
    .is_some_and(|(q, _)| q == 4);
    let waiting = with_wizard(&mut c, |_, w| w.state.verification) == CgVerification::Pending;

    the_shard_refuses(&mut c, &mut peer, NAME_IN_USE);

    let h = wizard_dialog(&mut c, CharGenDialog::ErrorMessage).expect("the player is told why");
    let want = shipped_word(&c, "ID_Character_Err_NameReserved");
    let other = shipped_word(
        &c,
        "ID_CharacterManagement_CG_VERIFICATION_RESPONSE_NAME_IN_USE",
    );
    let body = dialog_child(&mut c, h, dereth_ui::dialog::base::child::TEXT);
    let drawn = wizard_text(&mut c, body);
    let says_the_wizards_own_sentence = drawn == want && drawn != other;
    // ...and the latch clears, so the player can fix the name and finish again.
    let ready_again = with_wizard(&mut c, |_, w| w.state.verification) == CgVerification::Undef;
    c.shutdown();

    // The reason that takes the client's own general arm, which must not answer with nothing.
    let mut c = a_client_on_the_wizard();
    let mut peer = Peer::attach(&mut c, ObjectId(0x5000_0001));
    walk_the_wizard(&mut c);
    click_wizard(&mut c, chargen::FINISH_BUTTON);
    c.tick(2);
    the_shard_refuses(&mut c, &mut peer, 2);

    let h = wizard_dialog(&mut c, CharGenDialog::ErrorMessage)
        .expect("the general arm builds a box like every other failure");
    let want = shipped_word(&c, "ID_Character_Err_NameDBDown");
    let body = dialog_child(&mut c, h, dereth_ui::dialog::base::child::TEXT);
    let the_general_arm_says_so = wizard_text(&mut c, body) == want;

    // And the one button closes it, leaving the wizard usable.
    click_in_dialog(&mut c, h, MESSAGE_BUTTON);
    c.tick(2);
    let closed = wizard_dialog(&mut c, CharGenDialog::ErrorMessage).is_none()
        && with_wizard(&mut c, |_, w| w.open_dialog).is_none();

    c.assert_behaviour(
        "chargen.finish.a-refused-creation-stops-the-wizard-waiting-and-says-why",
        move |_| {
            asked
                && waiting
                && says_the_wizards_own_sentence
                && ready_again
                && the_general_arm_says_so
                && closed
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.finish.every-refusal-the-shard-can-send-draws-its-own-sentence
// ---------------------------------------------------------------------------------------------

/// Every reason the shard can answer a creation with, and the sentence the wizard's own arm for it
/// names -- including two that could be answered with a token nobody had written and two that could
/// be answered with silence.
pub(super) fn every_refusal_the_shard_can_send_draws_its_own_sentence() {
    use dereth_chargen::CgVerification;

    let table: [(u32, &str); 7] = [
        (0, "ID_Character_Err_NameDBDown"),
        (2, "ID_Character_Err_NameDBDown"),
        (3, "ID_Character_Err_NameReserved"),
        (4, "ID_Character_Err_NameBanned"),
        (5, "ID_Character_Err_NameDBDown"),
        (6, "ID_Character_Err_NameDBDown"),
        (7, "ID_Character_Err_NameAdminDenied"),
    ];
    let every_arm_names_its_own = table
        .iter()
        .all(|(code, token)| CgVerification::from_code(*code).error_string_id() == Some(*token));
    // The one answer that is not a refusal names no sentence at all.
    let a_yes_says_nothing = CgVerification::Ok.error_string_id().is_none();

    // And the four are real: four different sentences the shipped table can draw, none of them a
    // bare token.
    let mut c = a_client_on_the_wizard();
    let mut seen: Vec<String> = Vec::new();
    let mut four_different_sentences = true;
    for token in [
        "ID_Character_Err_NameDBDown",
        "ID_Character_Err_NameAdminDenied",
        "ID_Character_Err_NameBanned",
        "ID_Character_Err_NameReserved",
    ] {
        let s = shipped_word(&c, token);
        four_different_sentences &= !s.is_empty() && s != token && !seen.contains(&s);
        seen.push(s);
    }

    c.assert_behaviour(
        "chargen.finish.every-refusal-the-shard-can-send-draws-its-own-sentence",
        move |_| every_arm_names_its_own && a_yes_says_nothing && four_different_sentences,
    );
    c.shutdown();
}
