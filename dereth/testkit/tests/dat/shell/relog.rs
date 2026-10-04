//! Shell fixtures and scenarios for relog.

use super::*;
// =============================================================================================
// What survives a relog
//
// This is not a shell-only scenario at all: it is a whole client with a shard on the other end of a
// socket-free endpoint, and the shard is the point. [`RelogShard`] is a reference server's
// persistence and nothing more -- it holds the settings it would store for this character and
// applies the four handlers a reference server has for the messages this client sends -- and it
// learns **only what crossed the wire**, because [`relog_drain_wire`] reads the client's own
// outgoing datagrams and parses them.
//
// That is the whole argument: a scenario that re-read the client's own memory after a log-off
// would be green on a client that never told anybody anything.
// =============================================================================================

/// The character this scenario logs on as.
const RELOG_LARK: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0001);

/// The item the shortcut gesture drops. The toolbar refuses a zero id and nothing else, so an id
/// with no object behind it still makes the shortcut -- which is the point: this is about what the
/// shortcut list does across a relog and not about what is in the pack.
const RELOG_ITEM: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x8000_0111);

/// The setting whose ordinal is not one of the named ones, taken from the table by index with the
/// table's own name asserted beside it -- a bare number with nothing checking it is exactly the
/// kind that goes stale.
const HEAR_GENERAL_CHAT: usize = 35;

/// The main chat window, which is the one the filter and placement gestures address.
const RELOG_WINDOW: u32 = 1;

/// A setting the client keeps to itself until the way out...
const RELOG_DEFERRED: PlayerOption = PlayerOption::ShowTooltips;
/// ...and one it saves the moment it is ticked, so both directions are asserted.
const RELOG_AUTO_SAVED: PlayerOption = PlayerOption::HearGeneralChat;

/// A reference server's persistence for this character, and the four handlers it has for the
/// messages this client sends. Nothing here reads the client's memory.
struct RelogShard {
    module: dereth_protocol::login::PlayerModule,
    /// Every message the client put on the wire, for the record.
    heard: Vec<(dereth_protocol::Opcode, Vec<u8>)>,
}

impl RelogShard {
    /// A fresh character: the settings a reference server builds for somebody who has never
    /// changed anything.
    fn new() -> Self {
        Self {
            module: dereth_protocol::login::PlayerModule {
                spell_bars: vec![Vec::new()],
                spell_filters: dereth_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
                options: dereth_client_model::player::options::DEFAULT_OPTIONS,
                options2: dereth_client_model::player::options::DEFAULT_OPTIONS2,
                ..dereth_protocol::login::PlayerModule::default()
            },
            heard: Vec::new(),
        }
    }

    fn opcodes(&self) -> Vec<u32> {
        self.heard.iter().map(|(o, _)| o.0).collect()
    }

    /// One inbound message. Anything the shard has no handler for is recorded and ignored, which
    /// is what a shard with no handler for it does.
    fn apply(&mut self, opcode: dereth_protocol::Opcode, body: &[u8]) {
        use dereth_protocol::Message as _;
        self.heard.push((opcode, body.to_vec()));
        let mut r = dereth_protocol::archive::Reader::new(body);
        match opcode.0 {
            // One setting, persisted on its own.
            0x0005 => {
                if let Ok(m) =
                    dereth_protocol::login::CharacterPlayerOptionChangedEvent::read(&mut r)
                {
                    let ordinal = usize::try_from(m.option).unwrap_or(usize::MAX);
                    let mut o = dereth_client_model::player::options::Options {
                        options: self.module.options,
                        options2: self.module.options2,
                    };
                    o.set(ordinal, m.value != 0);
                    self.module.options = o.options;
                    self.module.options2 = o.options2;
                }
            }
            // A shortcut added, and one removed.
            0x019C => {
                if let Ok(m) = dereth_protocol::login::CharacterAddShortCut::read(&mut r) {
                    let list = self.module.shortcuts.get_or_insert_with(Vec::new);
                    list.retain(|s| s.index != m.shortcut.index);
                    list.push(m.shortcut);
                    list.sort_by_key(|s| s.index);
                    self.module.option_flags |=
                        dereth_protocol::login::player_module_flags::SHORTCUT;
                }
            }
            0x019D => {
                if let Ok(m) = dereth_protocol::login::CharacterRemoveShortCut::read(&mut r) {
                    if let Some(list) = self.module.shortcuts.as_mut() {
                        list.retain(|s| s.index != i32::try_from(m.index).unwrap_or(-1));
                    }
                }
            }
            // The whole settings block, replacing the stored one. This is the only route the
            // deferred half of the state has.
            0x01A1 => {
                if let Ok(m) = dereth_protocol::login::CharacterCharacterOptionsEvent::read(&mut r)
                {
                    self.module = m.module;
                }
            }
            _ => {}
        }
    }

    /// The description a second login receives, built out of nothing but what the shard was told.
    fn player_description(&self) -> dereth_protocol::login::LoginPlayerDescription {
        use dereth_protocol::login::player_module_flags as f;
        let mut m = self.module.clone();
        // The header and the fields have to agree or the block does not encode; the shard rebuilds
        // the header from what it holds, as its own writer does.
        let mut flags = f::SPELLBOOK_FILTERS | f::CHARACTER_OPTIONS_2;
        if m.shortcuts.as_ref().is_some_and(|s| !s.is_empty()) {
            flags |= f::SHORTCUT;
        } else {
            m.shortcuts = None;
        }
        if m.desired_comps.is_some() {
            flags |= f::DESIRED_COMPS;
        }
        if m.generic_qualities.is_some() {
            flags |= f::GENERIC_QUALITIES_DATA;
        }
        if m.gameplay_options
            .as_ref()
            .is_some_and(|o| !o.properties.entries.is_empty())
        {
            flags |= f::GAMEPLAY_OPTIONS;
        } else {
            m.gameplay_options = None;
        }
        if m.spell_bars.len() >= 8 {
            m.spell_bars.resize(8, Vec::new());
            flags |= f::SPELL_LISTS_8;
        } else {
            m.spell_bars.resize(1, Vec::new());
        }
        m.timestamp_format = None;
        m.option_flags = flags;
        dereth_protocol::login::LoginPlayerDescription {
            player_module: m,
            ..dereth_protocol::login::LoginPlayerDescription::default()
        }
    }
}

/// Every message the client has put on the wire since the last drain, read off its **own outgoing
/// datagrams** and handed to the shard.
///
/// A payload that begins with the ordered-action envelope is unpacked into its sub-type and body;
/// anything else is a control message whose first word is its own.
fn relog_drain_wire(c: &mut HeadlessClient, shard: &mut RelogShard) {
    let out = c
        .replay_net_mut()
        .expect("the endpoint is attached")
        .take_outgoing();
    for (bytes, _) in out {
        let Ok(packet) = dereth_transport::wire::ParsedPacket::parse(&bytes) else {
            continue;
        };
        for frag in &packet.fragments {
            let payload = &frag.payload;
            let Some(first) = payload
                .get(..4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            else {
                continue;
            };
            if first == 0xF7B1 {
                if let Ok(a) = dereth_protocol::actions::unpack_action(payload) {
                    let consumed = payload.len() - a.body.remaining();
                    shard.apply(a.sub_type, &payload[consumed..]);
                }
            } else {
                shard.apply(dereth_protocol::Opcode(first), &payload[4..]);
            }
        }
    }
}

/// Bring a client up on a socket-free link, land it in the world, and give it the settings block
/// the shard holds. The login is driven **through the transport** rather than by handing the
/// client decoded events, because the log-off below is state-gated: a session that never reached
/// the world sends no departure at all and the scenario would be measuring a log-off that did not
/// happen.
fn a_relog_client(shard: &RelogShard) -> (HeadlessClient, dereth_testkit::replay::Peer) {
    let mut c =
        HeadlessClient::new(ClientSpec::gameplay_in_world(4).with_scratch_settings("relog"));
    let mut peer = dereth_testkit::replay::Peer::attach(&mut c, RELOG_LARK);

    // The shard names itself, then offers the character list. The world name is load-bearing for
    // the notebook: the client composes its per-character file path out of the settings directory,
    // the world name and the character.
    peer.send(
        &mut c,
        9,
        dereth_protocol::write_blob(&dereth_protocol::login::LoginWorldInfo {
            connections: 1,
            max_connections: 100,
            world_name: "relog-world".into(),
        })
        .expect("the world name encodes"),
    );
    peer.send(
        &mut c,
        9,
        dereth_protocol::write_blob(&relog_character_set()).expect("the list encodes"),
    );
    c.tick(1);

    // The player picks the character, which is the two-step exchange the session layer owns.
    c.replay_net_mut()
        .expect("the endpoint")
        .session
        .enter_world(RELOG_LARK, "relog");
    c.tick(1);
    peer.send(
        &mut c,
        9,
        dereth_protocol::write_blob(&dereth_protocol::login::LoginEnterGameServerReady)
            .expect("the ready message encodes"),
    );
    c.tick(1);

    // The description, which is the last step and what puts the client in the world.
    peer.send(
        &mut c,
        9,
        dereth_protocol::write_blob(&shard.player_description()).expect("the description encodes"),
    );
    c.tick(1);

    // Two passes: the description is reassembled on the first and applied on the second.
    c.tick(2);
    assert_eq!(
        c.replay_net_mut().expect("the endpoint").session_state(),
        dereth_client_net::client_session::SessionState::Playable,
        "the description is what puts the client in the world, and the log-off depends on it"
    );
    assert!(
        c.view().world().player_system.module.is_some(),
        "the retained settings are the shard's, applied through the description"
    );
    (c, peer)
}

fn relog_character_set() -> dereth_protocol::login::LoginCharacterSet {
    dereth_protocol::login::LoginCharacterSet {
        status: 0,
        characters: vec![dereth_protocol::login::CharacterIdentity {
            gid: RELOG_LARK,
            name: "Lark".into(),
            seconds_greyed_out: 0,
        }],
        deleted: vec![],
        num_allowed_characters: 5,
        account: "relog".into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 0,
    }
}

/// Run on until the login's own traffic has stopped, so that what a gesture puts on the wire is
/// that gesture's and not the tail of the two questions every entry asks. The shard is told about
/// them and then its record is cleared: the denominator below is *what this gesture sent*, and a
/// login's own messages in it would read as one.
fn relog_settle(c: &mut HeadlessClient, shard: &mut RelogShard) {
    c.tick(6);
    relog_drain_wire(c, shard);
    shard.heard.clear();
}

/// Raise one request the way a panel does -- the queue every production callback pushes into --
/// and let the frame drain it through the real interaction arm.
///
/// Three passes and not one: the first drains the request and hands it to the session, and the
/// **next** one's send phase is what builds the datagram. A scenario that read the wire after one
/// pass would read a message that had been queued and not yet sent.
fn relog_gesture(c: &mut HeadlessClient, r: UiRequest) {
    c.ui_outbox().emit(r);
    c.tick(3);
}

/// What the client is holding for each of the states this is about, read off the one place each of
/// them lives, and comparable across two sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RelogHeld {
    deferred_option: bool,
    auto_saved_option: bool,
    shortcut_slot_3: Option<dereth_primitives::ObjectId>,
    chat_filter: Option<u64>,
    window_x: Option<i32>,
}

fn relog_held(c: &HeadlessClient) -> RelogHeld {
    let ps = &c.view().world().player_system;
    RelogHeld {
        deferred_option: ps.options.get(relog_option::SHOW_TOOLTIPS),
        auto_saved_option: ps.options.get(HEAR_GENERAL_CHAT),
        shortcut_slot_3: ps.shortcut_at(3),
        chat_filter: relog_chat_filter(c, RELOG_WINDOW),
        window_x: relog_window_x(c, RELOG_WINDOW),
    }
}

/// The chat window's own filter, out of the retained settings.
fn relog_chat_filter(c: &HeadlessClient, window: u32) -> Option<u64> {
    let module = c.view().world().player_system.module.as_ref()?;
    dereth_client::hud::decode_chat_filters(module)
        .into_iter()
        .find_map(|(w, m)| (w == window).then_some(m))
}

/// Where the chat window sits, out of the retained settings.
fn relog_window_x(c: &HeadlessClient, window: u32) -> Option<i32> {
    let module = c.view().world().player_system.module.as_ref()?;
    dereth_client::hud::decode_placements(module)
        .get(window)
        .and_then(|p| p.x)
}

// ---------------------------------------------------------------------------------------------
// relog.state.only-the-shortcut-and-the-auto-saved-option-reach-the-shard-at-the-gesture
// ---------------------------------------------------------------------------------------------

/// The ownership map, asserted rather than described. It is the denominator for everything below:
/// three of the four kinds of change put nothing on the wire at the moment they are made, so what
/// happens at the log-off is the whole question.
pub(super) fn only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture() {
    use dereth_client_model::player::options::{is_auto_save_option, PLAYER_OPTIONS};

    // The two ordinals, named out of the table rather than trusted as numbers, with the membership
    // that is the whole distinction between them.
    let named = PLAYER_OPTIONS[HEAR_GENERAL_CHAT].0 == "HearGeneralChat"
        && PLAYER_OPTIONS[relog_option::SHOW_TOOLTIPS].0 == "ShowTooltips"
        && is_auto_save_option(HEAR_GENERAL_CHAT)
        && !is_auto_save_option(relog_option::SHOW_TOOLTIPS);

    let mut shard = RelogShard::new();
    let (mut c, _peer) = a_relog_client(&shard);
    relog_settle(&mut c, &mut shard);

    relog_gesture(
        &mut c,
        UiRequest::DragDrop {
            item: RELOG_ITEM,
            target: DropTarget::ShortcutAlias { slot: 3, from: -1 },
        },
    );
    relog_drain_wire(&mut c, &mut shard);
    let the_drop_is_told_at_once = shard.opcodes() == vec![0x019C];
    shard.heard.clear();

    // Both settings are toggled **off their current value**, never written to a fixed one: writing
    // the value a setting already holds returns before anything is raised, and both of these ship
    // on, so writing `true` would be a no-op that reads exactly like a send that did not happen.
    let want_auto = !c
        .view()
        .world()
        .player_system
        .options
        .get(HEAR_GENERAL_CHAT);
    relog_gesture(
        &mut c,
        UiRequest::SetPlayerOption(RELOG_AUTO_SAVED, want_auto),
    );
    relog_drain_wire(&mut c, &mut shard);
    let the_auto_saved_one_goes_out = shard.opcodes() == vec![0x0005];
    shard.heard.clear();

    let want_deferred = !c
        .view()
        .world()
        .player_system
        .options
        .get(relog_option::SHOW_TOOLTIPS);
    relog_gesture(
        &mut c,
        UiRequest::SetPlayerOption(RELOG_DEFERRED, want_deferred),
    );
    relog_drain_wire(&mut c, &mut shard);
    let the_deferred_one_does_not = shard.opcodes().is_empty();

    relog_gesture(
        &mut c,
        UiRequest::SetChatWindowFilter {
            window: RELOG_WINDOW,
            mask: 0x0000_00FF,
        },
    );
    relog_gesture(
        &mut c,
        UiRequest::SetChatWindowOption {
            window: RELOG_WINDOW,
            property: 0x1000_0086,
            value: 137,
        },
    );
    relog_drain_wire(&mut c, &mut shard);
    let the_window_sends_nothing_either = shard.opcodes().is_empty();

    // ...and the client is holding all four, so the gestures did land somewhere.
    let unsaved = c.view().world().player_system.is_dirty();
    let all_four_landed = relog_held(&c)
        == RelogHeld {
            deferred_option: want_deferred,
            auto_saved_option: want_auto,
            shortcut_slot_3: Some(RELOG_ITEM),
            chat_filter: Some(0x0000_00FF),
            window_x: Some(137),
        };

    c.assert_behaviour(
        "relog.state.only-the-shortcut-and-the-auto-saved-option-reach-the-shard-at-the-gesture",
        move |_| {
            named
                && the_drop_is_told_at_once
                && the_auto_saved_one_goes_out
                && the_deferred_one_does_not
                && the_window_sends_nothing_either
                && unsaved
                && all_four_landed
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// relog.state.every-deferred-change-is-saved-at-the-log-off-and-found-by-the-next-session
// ---------------------------------------------------------------------------------------------

/// A player's own trip: one client, four changes, a clean log-off, and a second login on the same
/// character whose description is built out of nothing but what the shard was told.
///
/// Every state is read as a **value** at both ends rather than as a difference, because a scenario
/// that compared the second session with the first through the same field could not see a value
/// that was never stored at all.
pub(super) fn every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session() {
    let mut shard = RelogShard::new();

    let (mut c, _peer) = a_relog_client(&shard);
    relog_settle(&mut c, &mut shard);

    let default_tooltips = c
        .view()
        .world()
        .player_system
        .options
        .get(relog_option::SHOW_TOOLTIPS);
    let default_hear = c
        .view()
        .world()
        .player_system
        .options
        .get(HEAR_GENERAL_CHAT);
    let default_filter = relog_chat_filter(&c, RELOG_WINDOW);

    relog_gesture(
        &mut c,
        UiRequest::DragDrop {
            item: RELOG_ITEM,
            target: DropTarget::ShortcutAlias { slot: 3, from: -1 },
        },
    );
    relog_gesture(
        &mut c,
        UiRequest::SetPlayerOption(RELOG_AUTO_SAVED, !default_hear),
    );
    relog_gesture(
        &mut c,
        UiRequest::SetPlayerOption(RELOG_DEFERRED, !default_tooltips),
    );
    relog_gesture(
        &mut c,
        UiRequest::SetChatWindowFilter {
            window: RELOG_WINDOW,
            mask: 0x0000_00FF,
        },
    );
    relog_gesture(
        &mut c,
        UiRequest::SetChatWindowOption {
            window: RELOG_WINDOW,
            property: 0x1000_0086,
            value: 137,
        },
    );

    let left = relog_held(&c);
    let first_session_left =
        left == RelogHeld {
            deferred_option: !default_tooltips,
            auto_saved_option: !default_hear,
            shortcut_slot_3: Some(RELOG_ITEM),
            chat_filter: Some(0x0000_00FF),
            window_x: Some(137),
        } && left.chat_filter != default_filter;

    // The clean log-off, through the screen's own answer.
    relog_gesture(&mut c, UiRequest::EndCharacterSession { ask: false });
    relog_drain_wire(&mut c, &mut shard);
    let heard = shard.opcodes();
    let saved_once = heard.contains(&0x01A1)
        && c.view()
            .expect_app()
            .probe()
            .player_modules_saved_at_logout()
            == 1;
    // ...and in the client's own order: the settings go out ahead of the departure.
    let in_order = match (
        heard.iter().position(|o| *o == 0x01A1),
        heard.iter().position(|o| *o == 0xF653),
    ) {
        (Some(a), Some(b)) => a < b,
        _ => false,
    };

    // The notebook is the one thing on this list that is not the shard's, and it is not on the
    // wire: the settings the shard now stores carry no text of any kind.
    let nothing_of_the_notebook = shard
        .player_description()
        .player_module
        .timestamp_format
        .is_none()
        && shard.heard.iter().all(|(o, _)| o.0 != 0x0295);

    // The shard answers as it does six seconds later, and the client keeps running.
    c.app_mut().process_logon_event_queue(vec![
        dereth_client_net::client_session::SessionEvent::LoggedOff,
        dereth_client_net::client_session::SessionEvent::CharacterSet(Box::new(
            relog_character_set(),
        )),
    ]);
    let mut still_running = true;
    for _ in 0..4 {
        still_running &= c.app_mut().frame();
    }
    relog_drain_wire(&mut c, &mut shard);
    c.shutdown();

    // The second session, on a description the shard built out of the wire alone.
    let (mut c, _peer) = a_relog_client(&shard);
    let found = relog_held(&c);

    c.assert_behaviour(
        "relog.state.every-deferred-change-is-saved-at-the-log-off-and-found-by-the-next-session",
        move |_| {
            first_session_left
                && saved_once
                && in_order
                && nothing_of_the_notebook
                && still_running
                && found == left
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// relog.state.a-session-that-changed-nothing-deferred-saves-nothing-at-the-log-off
// ---------------------------------------------------------------------------------------------

/// The other direction, and the one that keeps the saving from being a blanket send: a session
/// whose only change was a setting that had already gone out saves nothing at all on the way out,
/// and the departure itself still goes.
pub(super) fn a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off() {
    let mut shard = RelogShard::new();
    let (mut c, _peer) = a_relog_client(&shard);
    relog_settle(&mut c, &mut shard);

    let want = !c
        .view()
        .world()
        .player_system
        .options
        .get(HEAR_GENERAL_CHAT);
    relog_gesture(&mut c, UiRequest::SetPlayerOption(RELOG_AUTO_SAVED, want));
    relog_drain_wire(&mut c, &mut shard);
    let it_went_out_on_its_own = shard.heard.iter().any(|(o, _)| o.0 == 0x0005)
        && !c.view().world().player_system.is_dirty();
    shard.heard.clear();

    relog_gesture(&mut c, UiRequest::EndCharacterSession { ask: false });
    relog_drain_wire(&mut c, &mut shard);
    let heard = shard.opcodes();
    let nothing_was_saved = !heard.contains(&0x01A1);
    let and_it_still_left = heard.contains(&0xF653);

    c.assert_behaviour(
        "relog.state.a-session-that-changed-nothing-deferred-saves-nothing-at-the-log-off",
        move |_| it_went_out_on_its_own && nothing_was_saved && and_it_still_left,
    );
    c.shutdown();
}
