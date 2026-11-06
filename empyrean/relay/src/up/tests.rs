//! Behaviour: none (what the relay lets through to the server, over made-up messages).

use dereth_primitives::ObjectId;
use dereth_protocol::login::{CharGenResult, CharacterSendCharGenResult, LoginSendEnterWorld};
use dereth_protocol::read_checked;

use super::*;

fn action(state: &mut UpState, opcode: u32, body: &[u8]) -> Up {
    let mut bytes = 0xF7B1u32.to_le_bytes().to_vec();
    bytes.extend(99u32.to_le_bytes());
    bytes.extend(opcode.to_le_bytes());
    bytes.extend_from_slice(body);
    state.message(&bytes).unwrap()
}

#[test]
fn the_allowlist_is_the_1999_clients_eighty_two_actions_and_nine_raw_messages() {
    let ordered: u32 = ORDERED_ACTIONS.iter().map(|(a, b)| b - a + 1).sum();
    assert_eq!(ordered, 82);
    assert_eq!(RAW_MESSAGES.len(), 9);
    assert!(ORDERED_ACTIONS.windows(2).all(|w| w[0].1 < w[1].0));
}

#[test]
fn every_action_outside_the_set_stops_here_with_a_message_or_in_silence() {
    let mut state = UpState::default();
    for trade in [0x01F6, 0x0204, 0x021C, 0x027D, 0x002C, 0x0290, 0x0317] {
        assert_eq!(
            action(&mut state, trade, &[]),
            Up::Refuse(NOT_IN_1999),
            "{trade:#06X}"
        );
    }
    for silent in DROPPED_SILENTLY {
        assert_eq!(action(&mut state, *silent, &[]), Up::Drop, "{silent:#06X}");
    }
}

#[test]
fn no_action_dropped_in_silence_is_one_the_1999_client_sends() {
    for silent in DROPPED_SILENTLY {
        assert!(!is_ordered_action(*silent), "{silent:#06X}");
    }
}

#[test]
fn every_action_in_the_set_goes_up_unless_the_later_client_means_something_else_by_it() {
    let mut state = UpState::default();
    for (first, last) in ORDERED_ACTIONS {
        for number in *first..=*last {
            let sent = action(&mut state, number, &[0; 64]);
            assert_eq!(
                matches!(sent, Up::Send { .. }),
                !LATER_MEANING.contains(&number),
                "{number:#06X}"
            );
        }
    }
}

#[test]
fn a_forwarded_action_takes_the_next_order_number_and_keeps_its_body() {
    let mut state = UpState::default();
    let body = [1, 2, 3, 4];
    for order in 1u32..=3 {
        let Up::Send { queue, bytes } = action(&mut state, 0xF61C, &body) else {
            panic!("the movement report goes through");
        };
        assert_eq!(queue, QUEUE_WEENIE);
        assert_eq!(bytes[..4], 0xF7B1u32.to_le_bytes());
        assert_eq!(bytes[4..8], order.to_le_bytes());
        assert_eq!(bytes[8..12], 0xF61Cu32.to_le_bytes());
        assert_eq!(bytes[12..], body);
    }
}

#[test]
fn a_refused_action_takes_no_order_number() {
    let mut state = UpState::default();
    action(&mut state, 0x01F6, &[]);
    let Up::Send { bytes, .. } = action(&mut state, 0xF61C, &[]) else {
        panic!("the movement report goes through");
    };
    assert_eq!(bytes[4..8], 1u32.to_le_bytes());
}

#[test]
fn talk_is_sent_in_the_1999_strings_form() {
    let mut w = Writer::new();
    w.pstring("hello").unwrap();
    let mut state = UpState::default();
    let Up::Send { bytes, .. } = action(&mut state, 0x0015, &w.into_inner()) else {
        panic!("talk goes through");
    };
    // The count holds the terminator, and the string pads to four from the message's start.
    assert_eq!(bytes[12..14], 6u16.to_le_bytes());
    assert_eq!(&bytes[14..20], b"hello\0");
    assert_eq!(bytes.len() % 4, 0);
}

#[test]
fn a_combat_mode_goes_back_to_the_1999_numbers() {
    for (later, old) in [(1u32, 1u32), (2, 2), (4, 3), (8, 4)] {
        let mut state = UpState::default();
        let Up::Send { bytes, .. } = action(&mut state, 0x0053, &later.to_le_bytes()) else {
            panic!("the mode goes through");
        };
        assert_eq!(bytes[12..16], old.to_le_bytes());
    }
}

#[test]
fn a_later_character_becomes_the_1999_lists_places() {
    let mut c = CharGenResult {
        heritage_group: 1,
        gender: 1,
        name: "Tester".into(),
        skill_advancement_classes: vec![0, 3, 2],
        strength: 60,
        start_area: 2,
        ..CharGenResult::default()
    };
    let g = character_gen(&c);
    assert_eq!((g.heritage, g.gender), (0, 1), "Aluvian, Male");
    assert_eq!(g.skills.len(), 35);
    assert_eq!(
        g.skills[..4],
        [0, 3, 2, 1],
        "a skill not named is untrained"
    );
    assert_eq!(g.attributes[0], 60);
    assert_eq!(g.start_area, 2);
    c.heritage_group = 3;
    c.gender = 2;
    let g = character_gen(&c);
    assert_eq!((g.heritage, g.gender), (2, 0), "Sho, Female");
}

#[test]
fn creating_and_entering_name_the_account_the_list_carried() {
    let mut state = UpState {
        account: "from the list".into(),
        ..UpState::default()
    };
    let create = write_blob(&CharacterSendCharGenResult {
        account: "what Dereth typed".into(),
        result: CharGenResult {
            name: "Tester".into(),
            ..CharGenResult::default()
        },
    })
    .unwrap();
    let Up::Send { queue, bytes } = state.message(&create).unwrap() else {
        panic!("a creation goes through");
    };
    assert_eq!(queue, QUEUE_LOGIN);
    let sent = read_checked::<CreateCharacter>(&bytes).unwrap();
    assert_eq!(sent.account, "from the list");
    assert_eq!(sent.character.name, "Tester");
    let enter = write_blob(&LoginSendEnterWorld {
        character: ObjectId(7),
        account: "what Dereth typed".into(),
    })
    .unwrap();
    let Up::Send { bytes, .. } = state.message(&enter).unwrap() else {
        panic!("an entry goes through");
    };
    let sent = read_checked::<SelectCharacter>(&bytes).unwrap();
    assert_eq!(
        (sent.character, sent.account.as_str()),
        (ObjectId(7), "from the list")
    );
}

#[test]
fn a_short_message_is_an_error_not_a_panic() {
    let mut state = UpState::default();
    assert!(state.message(&[1, 2]).is_err());
    assert!(state.message(&0xF7B1u32.to_le_bytes()).is_err());
}

#[test]
fn a_favorite_spell_goes_up_without_the_later_clients_bar_number() {
    let mut state = UpState::default();
    let add = [1234u32, 3, 0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    let Up::Send { bytes, .. } = action(&mut state, 0x01E3, &add) else {
        panic!("the favorite goes through");
    };
    assert_eq!(bytes[12..], add[..8]);
    let remove = [1234u32, 0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    let Up::Send { bytes, .. } = action(&mut state, 0x01E4, &remove) else {
        panic!("the removal goes through");
    };
    assert_eq!(bytes[12..], remove[..4]);
}

#[test]
fn casting_goes_up_unchanged() {
    let mut state = UpState::default();
    for (action_id, body) in [
        (0x004Au32, vec![1234u32]),
        (0x0048, vec![0x5000_0001, 1234]),
    ] {
        let body = body
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let Up::Send { bytes, .. } = action(&mut state, action_id, &body) else {
            panic!("the cast goes through");
        };
        assert_eq!(bytes[8..12], action_id.to_le_bytes());
        assert_eq!(bytes[12..], body);
    }
}

fn body_of(up: Up) -> (u32, Vec<u8>) {
    let Up::Send { bytes, .. } = up else {
        panic!("it goes through");
    };
    (
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        bytes[12..].to_vec(),
    )
}

fn string_1999(text: &str) -> Vec<u8> {
    let mut w = Writer::new();
    codec::write_string(&mut w, text, codec::MAX_NEWS).unwrap();
    w.into_inner()
}

#[test]
fn a_later_squelch_by_name_or_id_becomes_the_1999_action_for_it() {
    let mut state = UpState::default();
    let squelch = |add: i32, id: u32, name: &str| {
        let mut w = Writer::new();
        w.i32(add);
        w.u32(id);
        w.pstring(name).unwrap();
        w.u32(0);
        w.into_inner()
    };
    let (op, body) = body_of(action(&mut state, 0x0058, &squelch(1, 0, "Pest")));
    assert_eq!((op, body), (0x0058, string_1999("Pest")));
    let (op, body) = body_of(action(&mut state, 0x0058, &squelch(0, 0, "Pest")));
    assert_eq!((op, body), (0x0059, string_1999("Pest")));
    let (op, body) = body_of(action(&mut state, 0x0058, &squelch(1, 7, "")));
    assert_eq!((op, body), (0x005A, 7u32.to_le_bytes().to_vec()));
    let (op, body) = body_of(action(&mut state, 0x0058, &squelch(0, 7, "")));
    assert_eq!((op, body), (0x005B, 7u32.to_le_bytes().to_vec()));
}

#[test]
fn account_and_global_squelch_do_not_reach_the_server_as_the_1999_actions_of_those_numbers() {
    let mut state = UpState::default();
    assert_eq!(
        action(&mut state, 0x0059, &[1, 0, 0, 0]),
        Up::Refuse(NOT_IN_1999)
    );
    assert_eq!(
        action(&mut state, 0x005B, &[1, 0, 0, 0]),
        Up::Refuse(NOT_IN_1999)
    );
}

#[test]
fn text_bearing_actions_go_up_in_1999_strings() {
    let mut state = UpState::default();
    let mut w = Writer::new();
    w.u32(0x800);
    w.pstring("to the fellows").unwrap();
    let (op, body) = body_of(action(&mut state, 0x0147, &w.into_inner()));
    assert_eq!(op, 0x0147);
    assert_eq!(body[..4], 0x800u32.to_le_bytes());
    assert_eq!(body[4..6], 15u16.to_le_bytes());
    let mut w = Writer::new();
    w.pstring("Band").unwrap();
    w.u32(1);
    let (op, body) = body_of(action(&mut state, 0x00A2, &w.into_inner()));
    assert_eq!((op, body), (0x00A2, string_1999("Band")), "no share word");
    let mut w = Writer::new();
    w.u32(9);
    w.u32(2);
    w.pstring("Page").unwrap();
    let (op, body) = body_of(action(&mut state, 0x00AB, &w.into_inner()));
    assert_eq!(op, 0x00AB);
    assert_eq!(body[8..], string_1999("Page")[..]);
}

#[test]
fn a_buy_drops_the_later_currency_word_and_a_sell_passes() {
    let mut state = UpState::default();
    let buy = [3u32, 1, 2, 77, 0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    assert_eq!(body_of(action(&mut state, 0x005F, &buy)).1, buy[..16]);
    let sell = [3u32, 1, 2, 77]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    assert_eq!(body_of(action(&mut state, 0x0060, &sell)).1, sell);
}

#[test]
fn an_option_change_goes_up_as_the_whole_1999_word() {
    let mut state = UpState {
        options: 0x400,
        ..UpState::default()
    };
    let change = |option: u32, value: u32| {
        [option, value]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>()
    };
    let (op, body) = body_of(action(&mut state, 0x0005, &change(0x0, 1)));
    assert_eq!((op, body), (0x01A1, 0x402u32.to_le_bytes().to_vec()));
    let (_, body) = body_of(action(&mut state, 0x0005, &change(0xA, 0)));
    assert_eq!(body, 0x2u32.to_le_bytes());
    assert_eq!(
        action(&mut state, 0x0005, &change(0x14, 1)),
        Up::Drop,
        "no 1999 bit"
    );
}

#[test]
fn an_answer_goes_up_only_to_a_question_the_server_asked() {
    let mut state = UpState::default();
    assert_eq!(action(&mut state, 0x0275, &[]), Up::Refuse(NOT_IN_1999));
    state.questions = 1;
    assert_eq!(body_of(action(&mut state, 0x0275, &[1, 2, 3, 4])).0, 0x0275);
    assert_eq!(action(&mut state, 0x0275, &[]), Up::Refuse(NOT_IN_1999));
}

#[test]
fn deleting_names_the_lists_account_and_the_slot() {
    let mut state = UpState {
        account: "from the list".into(),
        ..UpState::default()
    };
    let request = write_blob(&CharacterDeleteRequest {
        account: "typed".into(),
        slot_index: 2,
    })
    .unwrap();
    let Up::Send { queue, bytes } = state.message(&request).unwrap() else {
        panic!("a deletion goes through");
    };
    assert_eq!(queue, QUEUE_LOGIN);
    let sent = read_checked::<DeleteCharacter>(&bytes).unwrap();
    assert_eq!((sent.account.as_str(), sent.slot), ("from the list", 2));
}

#[test]
fn friends_go_up_as_the_1999_command() {
    let mut state = UpState::default();
    let mut w = Writer::new();
    w.pstring("Pal").unwrap();
    let Up::Send { queue, bytes } = action(&mut state, 0x0018, &w.into_inner()) else {
        panic!("it goes through");
    };
    assert_eq!(queue, QUEUE_CONTROL);
    assert_eq!(bytes[..8], [0xCD, 0xF7, 0, 0, 1, 0, 0, 0]);
    assert_eq!(bytes[8..], string_1999("Pal")[..]);
}

#[test]
fn the_1999_clients_save_command_goes_up_as_its_action_and_not_as_speech() {
    let mut state = UpState::default();
    for typed in ["@save", "/SAVE", " @Save "] {
        let mut body = Writer::new();
        body.pstring(typed).unwrap();
        let (op, sent) = body_of(action(&mut state, 0x0015, &body.into_inner()));
        assert_eq!((op, sent), (0x01A2, Vec::new()), "{typed}");
    }
    let mut body = Writer::new();
    body.pstring("@saveui").unwrap();
    let (op, _) = body_of(action(&mut state, 0x0015, &body.into_inner()));
    assert_eq!(op, 0x0015);
}

#[test]
fn the_squelch_upload_is_a_count_and_the_ids() {
    let mut state = UpState::default();
    let (op, body) = body_of(state.squelch_upload(&[7, 9]));
    assert_eq!(op, 0x005C);
    assert_eq!(body, [2, 0, 0, 0, 7, 0, 0, 0, 9, 0, 0, 0]);
}
