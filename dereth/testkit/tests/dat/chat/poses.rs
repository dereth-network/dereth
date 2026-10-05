use super::*;
// ---------------------------------------------------------------------------------------------
// Poses: the run between stars, and the keys bound to one
// ---------------------------------------------------------------------------------------------
//
// **No datagram leaves this process.** The client has no link; where the bytes are the claim the
// request is handed to the production sender over a mock transport.

/// The two sections of the shipped keymap a pose can be bound in.
const MOVEMENT_COMMANDS: u32 = 4;
const EMOTES_MAP: u32 = 0x1000_0006;
/// Waving, and the two motions this section is about.
const WAVE_ACTION: u32 = 0x1000_00E5;
const MOTION_WAVE: u32 = 0x1300_0087;
const MOTION_SLEEPING: u32 = 0x4100_0014;

/// The key the shipped keymap binds an action to, answered as one of the keys a hand could press.
fn key_bound_to(c: &mut HeadlessClient, action: u32, map: u32) -> winit::keyboard::KeyCode {
    use winit::keyboard::KeyCode;
    const CANDIDATES: [KeyCode; 8] = [
        KeyCode::KeyJ,
        KeyCode::KeyB,
        KeyCode::KeyO,
        KeyCode::KeyU,
        KeyCode::KeyI,
        KeyCode::KeyK,
        KeyCode::KeyG,
        KeyCode::KeyV,
    ];
    let binding = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell is part of the shell")
        .keys_for_action(
            dereth_input::ActionId(action),
            dereth_input::InputMapId(map),
        )
        .into_iter()
        .find(|b| b.meta_mode == 0)
        .unwrap_or_else(|| panic!("nothing unmodified is bound to {action:#010X} in {map:#010X}"));
    CANDIDATES
        .into_iter()
        .find(|k| {
            dereth_desktop::pump::scan_code_from_key_code(*k)
                .is_some_and(|s| s & 0x7F == binding.control.offset() & 0x7F)
        })
        .unwrap_or_else(|| panic!("no candidate key carries that scan code"))
}

/// A client in the world, with the gameplay screen up, to press keys at.
fn a_client_to_pose_with() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay(4))
}

/// A key bound to lying down really lies the body down, and one the shipped keymap binds to no
/// pose at all is bound to none.
pub fn a_key_bound_to_a_pose_moves_the_body() {
    use winit::keyboard::KeyCode;

    let mut c = a_client_to_pose_with();
    let mut hand = Hand::new();

    let lie_down = key_bound_to(
        &mut c,
        dereth_client_runtime::actions::movement::action::LAY_DOWN.0,
        MOVEMENT_COMMANDS,
    );
    let bound_to_b = lie_down == KeyCode::KeyB;
    let before = c
        .view()
        .expect_app()
        .probe()
        .movement()
        .transient_motions_issued;
    hand.tap(&mut c, key_of(lie_down));
    c.tick(2);
    let moved = c
        .view()
        .expect_app()
        .probe()
        .movement()
        .transient_motions_issued
        == before + 1
        && c.view()
            .expect_app()
            .probe()
            .movement()
            .last_transient_motion
            == Some((MOTION_SLEEPING, true));

    // The other half. The lookup must be able to find a binding at all, so it is pointed at a key
    // that has one first: an empty answer from a lookup that never resolves anything is not
    // evidence of anything.
    let resolves = key_bound_to(&mut c, WAVE_ACTION, EMOTES_MAP) == KeyCode::KeyJ;
    let v_scan = dereth_desktop::pump::scan_code_from_key_code(KeyCode::KeyV)
        .expect("that key has a scan code");
    let mut bound_to_v: Vec<u32> = Vec::new();
    for map in [MOVEMENT_COMMANDS, EMOTES_MAP] {
        let shell = c.app_mut().input_manager_mut().expect("the input shell");
        if let Some(section) = shell.manager.keymap.section(dereth_input::InputMapId(map)) {
            for (control, act) in section.bindings() {
                if control.control.offset() & 0x7F == v_scan & 0x7F {
                    bound_to_v.push(act.0);
                }
            }
        }
    }
    let unbound = bound_to_v.is_empty();

    c.assert_behaviour(
        "chat.pose.a-key-bound-to-a-pose-moves-the-body-and-one-that-is-not-is-bound-to-nothing",
        move |_| bound_to_b && moved && resolves && unbound,
    );
    c.shutdown();
}

/// The bytes one request goes out in, composed here rather than by the writer being asserted over.
fn game_action(stamp: u32, opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut v = vec![0xb1, 0xf7, 0x00, 0x00];
    v.extend_from_slice(&stamp.to_le_bytes());
    v.extend_from_slice(&opcode.to_le_bytes());
    v.extend_from_slice(body);
    v
}

/// A length-prefixed string padded out to four.
fn pstr(s: &str) -> Vec<u8> {
    let mut v = u16::try_from(s.len())
        .expect("short")
        .to_le_bytes()
        .to_vec();
    v.extend_from_slice(s.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

/// A run between stars is performed, the others are told one word and the player reads another,
/// and the rest of the line is spoken.
pub fn a_run_between_stars_is_performed_and_the_rest_spoken() {
    let mut c = a_client_to_pose_with();
    let mut hand = Hand::new();

    let motions = c
        .view()
        .expect_app()
        .probe()
        .movement()
        .transient_motions_issued;
    let poses = c.view().expect_app().interaction().stats.poses_resolved;
    hand.say(&mut c, "hello *wave* there");

    let found = c.view().expect_app().interaction().stats.poses_resolved == poses + 1;
    // Two messages, in the client's own order: what the others are told, then what he said.
    let sent = c.view().expect_app().interaction().last_sent.to_vec();
    let both = sent
        == vec![
            dereth_client_model::Request::SoulEmote(
                dereth_protocol::comms::CommunicationSoulEmote {
                    message: "waves.".to_owned(),
                },
            ),
            dereth_client_model::Request::Talk(dereth_protocol::comms::CommunicationTalk {
                message: "hello  there".to_owned(),
            }),
        ];

    // The bytes.
    let mut session = Session::new(MockTransport::new());
    let mut bytes = true;
    for (i, r) in sent.iter().enumerate() {
        bytes &= dereth_client_runtime::requests::send_request(&mut session, r);
        let packet = session
            .transport
            .sent
            .last()
            .expect("one datagram per message");
        let stamp = u32::try_from(i + 1).expect("small");
        let want = match r {
            dereth_client_model::Request::SoulEmote(_) => {
                game_action(stamp, 0x01E1, &pstr("waves."))
            }
            dereth_client_model::Request::Talk(_) => {
                game_action(stamp, 0x0015, &pstr("hello  there"))
            }
            other => panic!("unexpected {other:?}"),
        };
        bytes &=
            (packet.queue, packet.ordered) == (NetQueue::Weenie, true) && packet.payload == want;
    }

    // The body moved, with the same command the key bound to waving produces.
    c.tick(2);
    let performed = c
        .view()
        .expect_app()
        .probe()
        .movement()
        .transient_motions_issued
        == motions + 1
        && c.view()
            .expect_app()
            .probe()
            .movement()
            .last_transient_motion
            == Some((MOTION_WAVE, true));

    // And what the player himself reads is the **other** word of the pair.
    c.tick(3);
    let log = log_text(&mut c);
    let echoed = log.contains("You wave.")
        && !log.contains("waves.")
        && c.view()
            .expect_app()
            .interaction()
            .stats
            .pose_echoes_printed
            == 1;

    c.assert_behaviour(
        "chat.pose.a-run-between-stars-is-performed-and-the-rest-of-the-line-is-spoken",
        move |_| found && both && bytes && performed && echoed,
    );
    c.shutdown();
}

/// A run on its own says nothing at all, and a run the client cannot place is spoken whole --
/// stars and typing and all -- rather than being eaten.
pub fn a_run_alone_says_nothing_and_an_unknown_one_is_spoken() {
    let mut c = a_client_to_pose_with();
    let mut hand = Hand::new();

    hand.say(&mut c, "*wave*");
    let alone = c.view().expect_app().interaction().last_sent.as_slice()
        == [dereth_client_model::Request::SoulEmote(
            dereth_protocol::comms::CommunicationSoulEmote {
                message: "waves.".to_owned(),
            },
        )];

    let poses = c.view().expect_app().interaction().stats.poses_resolved;
    hand.say(&mut c, "hello *xyzzy* there");
    let unknown = c.view().expect_app().interaction().stats.poses_resolved == poses
        && c.view().expect_app().interaction().last_sent.as_slice()
            == [dereth_client_model::Request::Talk(
                dereth_protocol::comms::CommunicationTalk {
                    message: "hello *xyzzy* there".to_owned(),
                },
            )];

    c.assert_behaviour(
        "chat.pose.a-run-alone-says-nothing-and-one-the-client-cannot-place-is-spoken-whole",
        move |_| alone && unknown,
    );
    c.shutdown();
}

/// Saying something with the command that says it goes through the same extraction as saying it
/// with no command in front at all.
pub fn the_say_command_goes_through_the_same_extraction() {
    let mut c = a_client_to_pose_with();
    let mut hand = Hand::new();
    hand.say(&mut c, "@say *wave* hi");
    let same = c.view().expect_app().interaction().last_sent.as_slice()
        == [
            dereth_client_model::Request::SoulEmote(
                dereth_protocol::comms::CommunicationSoulEmote {
                    message: "waves.".to_owned(),
                },
            ),
            dereth_client_model::Request::Talk(dereth_protocol::comms::CommunicationTalk {
                message: "hi".to_owned(),
            }),
        ];

    c.assert_behaviour(
        "chat.pose.the-say-command-goes-through-the-same-extraction",
        move |_| same,
    );
    c.shutdown();
}

/// Asking for the list of poses prints the shipped list, whole and in one line, and sends nothing.
pub fn the_emotes_command_prints_the_shipped_list() {
    let mut c = a_client_to_pose_with();
    let mut hand = Hand::new();

    let unimplemented = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .chat_commands_unimplemented;
    hand.say(&mut c, "@emotes");
    let wired = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .chat_commands_unimplemented
        == unimplemented
        && c.view().expect_app().interaction().last_sent.is_empty();
    c.tick(3);
    let log = log_text(&mut c);
    let want = dereth_client_model::emotes::emote_list_text();
    // Two of the names, so a list that was cut short cannot pass.
    let printed =
        log.contains(want.trim_end()) && want.contains("ShakeFist") && want.contains("Shake Head");

    c.assert_behaviour(
        "chat.pose.the-list-command-prints-the-shipped-list-and-sends-nothing",
        move |_| wired && printed,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The shipped table of poses, and what it decides
// ---------------------------------------------------------------------------------------------
//
// The two words a pose has are the **shipped** table's and not this file's: a pose has one form
// the player reads about himself and another the people around him read, and which is which is
// what these are about.

/// The shipped table of poses, out of the retail data.
fn shipped_pose_table() -> dereth_assets::tables::ChatPoseTable {
    use dereth_assets::Decode as _;
    let store =
        dereth_dat::testing::open_store().expect("the retail data is this scenario's oracle");
    let id = store
        .ids_of(dereth_dat::divine::DbType::ChatPoseTable)
        .into_iter()
        .next()
        .expect("the retail data carries a table of poses");
    let bytes = store.read_portal(id).expect("the table reads");
    dereth_assets::tables::ChatPoseTable::decode(&mut dereth_dat::Cursor::new(&bytes))
        .expect("the table decodes")
}

/// The motion a pose's name resolves to, which the client asks the animation side for.
fn motion_of(name: &str) -> Option<u32> {
    dereth_animation::command::MotionCommand::from_name(name).map(|m| m.0)
}

/// What a pose sends is a different message from what the emote command sends: the same framing
/// and the same place in the order, a different message and different words.
pub fn a_pose_sends_a_different_message_from_the_emote_command() {
    let table = shipped_pose_table();
    let out = dereth_client_model::emotes::public_chat("hello *wave* there", |name| {
        dereth_client_model::emotes::pose(&table, name, 1, motion_of)
    });
    let extracted = out.poses.len() == 1;
    let p = &out.poses[0];
    let resolved = p.motion_name == "Wave"
        && p.motion_command.is_some()
        // The two words: one for the people around, one for the player himself.
        && p.soul_emote.as_deref() == Some("waves.")
        && p.my_emote.as_deref() == Some("wave.")
        && out.speech.as_deref() == Some("hello  there");

    let (queue, pose_bytes) = on_the_wire(&dereth_protocol::comms::CommunicationSoulEmote {
        message: p.soul_emote.clone().expect("the other word is not empty"),
    });
    let framed = queue == NetQueue::Weenie
        && pose_bytes[0..4] == 0xF7B1_u32.to_le_bytes()
        && pose_bytes[8..12] == [0xE1, 0x01, 0x00, 0x00]
        && pose_bytes[12..14] == 6_u16.to_le_bytes()
        && &pose_bytes[14..20] == b"waves.";

    // And beside it the message the emote command sends, with the same header and the same first
    // place in the order -- but a different message and a different body.
    let (_, command_bytes) = on_the_wire(&dereth_protocol::comms::CommunicationEmote {
        message: "waves".to_owned(),
    });
    let differ = pose_bytes[8..12] != command_bytes[8..12]
        && pose_bytes[0..8] == command_bytes[0..8]
        && pose_bytes[12..] != command_bytes[12..];
    // The emote command's own bytes, whole: a length-prefixed word padded out to four.
    let commands_own = command_bytes[8..12] == [0xDF, 0x01, 0x00, 0x00]
        && command_bytes[4..8] == 1_u32.to_le_bytes()
        && command_bytes[12..14] == 5_u16.to_le_bytes()
        && &command_bytes[14..19] == b"waves"
        && command_bytes.len() == 20;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.pose.what-a-pose-sends-is-a-different-message-from-what-the-command-sends",
        move |_| extracted && resolved && framed && differ && commands_own,
    );
}

/// The possessive word in a pose is chosen by the player's sex before the message leaves, so no
/// placeholder ever reaches anybody's window.
pub fn the_possessive_word_is_chosen_by_sex() {
    let table = shipped_pose_table();
    let male =
        dereth_client_model::emotes::pose(&table, "Scratch Head", 1, motion_of).expect("a pose");
    let female =
        dereth_client_model::emotes::pose(&table, "Scratch Head", 2, motion_of).expect("a pose");
    let chosen = male.soul_emote.as_deref() == Some("scratches his head.")
        && female.soul_emote.as_deref() == Some("scratches her head.")
        // The form the player reads about himself has no possessive in it at all.
        && male.my_emote.as_deref() == Some("scratch your head.")
        && !male.soul_emote.as_deref().unwrap_or_default().contains("%p");
    // And the name is looked up without minding how it was capitalised.
    let insensitive = dereth_client_model::emotes::pose(&table, "scratch head", 1, motion_of)
        .expect("found either way")
        .motion_name
        == "ScratchHead";

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.pose.the-possessive-word-is-chosen-by-sex-before-the-message-leaves",
        move |_| chosen && insensitive,
    );
}

/// What the player reads about his own pose is never turned into a noise, whatever people he is.
pub fn the_players_own_echo_is_never_turned_into_a_noise() {
    let table = shipped_pose_table();
    let p = dereth_client_model::emotes::pose(&table, "Wave", 1, motion_of).expect("a pose");
    let my_emote = p.my_emote.clone().expect("the player's own form");

    let mut every = true;
    for heritage in [0_i32, 12] {
        let mut c = HeadlessClient::model();
        {
            let w = c.world_mut();
            let mut q = dereth_client_model::qualities::Qualities::new();
            q.ints = Some(
                [(
                    dereth_ui_screens::panels::inventory::HERITAGE_GROUP_PROPERTY,
                    heritage,
                )]
                .into_iter()
                .collect(),
            );
            w.seed_player_desc(ObjectId(0x5000_0001), q);
        }
        c.hud_mut().player_desc_received = true;
        let is_olthoi = c.view().hud().is_olthoi(c.view().world());
        assert_eq!(
            is_olthoi,
            heritage == 12,
            "the premise: {heritage} decides it"
        );

        c.when(Inbound::message(
            &dereth_protocol::comms::CommunicationHearSoulEmote {
                sender: ObjectId(0),
                sender_name: dereth_client_model::emotes::LOCAL_ECHO_NAME.to_owned(),
                text: my_emote.clone(),
            },
        ));
        let lines: Vec<String> = c
            .view()
            .chat_lines()
            .iter()
            .map(|m| m.body.clone())
            .collect();
        let stats = &c.view().hud().stats;
        let holds = lines == vec!["You wave.".to_owned()]
            && c.view().chat_lines()[0].ty == 0x0C
            && stats.emote_lines_composed == 1
            // The echo cannot be turned into a noise, whatever people the player is.
            && stats.emote_lines_untranslated == 0
            // And it is not swallowed as the player's own words coming back to him.
            && stats.soul_emote_self_echoes_discarded == 0;
        assert!(holds, "heritage {heritage}: {lines:?}");
        every &= holds;
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.pose.the-players-own-echo-is-never-turned-into-a-noise",
        move |_| every,
    );
}

/// Every pose the list advertises is one the shipped table can really perform.
///
/// The list and the table are two different things and can disagree; a name the list advertises
/// that the table cannot place is a command that silently does nothing at all.
pub fn every_pose_the_list_advertises_can_be_performed() {
    let table = shipped_pose_table();
    let text = dereth_client_model::emotes::emote_list_text();
    let shape = text.starts_with("Standard Emotes:\n") && text.ends_with("Shake Head\n\n");

    let mut missing: Vec<&str> = Vec::new();
    let mut motionless: Vec<&str> = Vec::new();
    for name in dereth_client_model::emotes::STANDARD_EMOTES {
        match dereth_client_model::emotes::pose(&table, name, 1, motion_of) {
            None => missing.push(name),
            Some(p) if p.motion_command.is_none() => motionless.push(name),
            Some(_) => {}
        }
    }
    assert!(
        missing.is_empty(),
        "advertised and not in the table: {missing:?}"
    );
    assert!(
        motionless.is_empty(),
        "advertised, in the table, and with no motion: {motionless:?}"
    );
    let all_perform = missing.is_empty() && motionless.is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "chat.pose.every-pose-the-list-advertises-can-really-be-performed",
        move |_| shape && all_perform,
    );
}
