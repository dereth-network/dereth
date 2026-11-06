//! The account's characters read by slot.

use auberean_client_model::characters::{
    greyed_out_for, id, identity, includes, invalid, name, set_greyed_out_for, which_slot,
};
use auberean_client_model::{Effect, Item, Model};
use auberean_protocol::login::{CharacterCreated, CharacterDeleted, CharacterList};
use auberean_protocol::types::CharacterIdentity;
use auberean_protocol::ServerMessage;

fn character(id: u64, name: &str, greyed: u32) -> CharacterIdentity {
    CharacterIdentity {
        id,
        name: name.into(),
        seconds_greyed_out: greyed,
        ..CharacterIdentity::default()
    }
}

/// A slot holds the character at that place in the list, unless it has no id; past the end, or
/// empty, it has no name and the invalid id; a character's slot is its place, -1 when absent;
/// how long a slot stays greyed out also reads 0 past the characters the account may have.
///
/// Behaviour: model.characters.a-slot-holds-the-character-at-its-place
#[test]
fn a_slot_holds_the_character_at_its_place_or_nothing() {
    let mut list = CharacterList {
        characters: vec![
            character(0x5000_0001, "Aerin", 0),
            character(0, "", 0),
            character(0x5000_0003, "Bryn", 3600),
        ],
        slots: 2,
        ..CharacterList::default()
    };
    assert!(
        invalid(&CharacterIdentity::default()),
        "a new identity names none"
    );
    assert_eq!(which_slot(&list, 0x5000_0003), 2);
    assert_eq!(which_slot(&list, 0x5000_0009), -1);
    assert!(includes(&list, 0x5000_0001));
    assert!(!includes(&list, 0x5000_0009));
    assert_eq!(name(&list, 0).map(|n| n.0.len()), Some(5));
    assert!(identity(&list, 1).is_none(), "no id");
    assert!(identity(&list, 3).is_none(), "past the end");
    assert_eq!(id(&list, 2), 0x5000_0003);
    assert_eq!(id(&list, 1), 0);
    assert_eq!(id(&list, 7), 0);
    assert_eq!(
        greyed_out_for(&list, 2),
        0,
        "past the two the account may have"
    );
    list.slots = 0;
    assert_eq!(greyed_out_for(&list, 2), 3600, "no number of slots: any");
    assert_eq!(greyed_out_for(&list, -1), 0);
    set_greyed_out_for(&mut list, 0, 60);
    set_greyed_out_for(&mut list, 1, 60);
    assert_eq!(greyed_out_for(&list, 0), 60);
    assert_eq!(
        list.characters[1].seconds_greyed_out, 0,
        "an empty slot is left"
    );
    // A copy of the list is its own.
    let copy = list.clone();
    list.characters.clear();
    assert_eq!(copy.characters.len(), 3);
}

/// The character list, and every answer to a character asked for, tell the character screens.
///
/// Behaviour: model.characters.the-list-and-answers-tell-the-screens
#[test]
fn the_list_and_every_answer_to_a_character_tell_the_screens() {
    let mut m = Model::new();
    let at = |message| Item {
        blob_id: 0,
        message,
    };
    assert_eq!(
        m.apply(&at(ServerMessage::CharacterList(CharacterList::default()))),
        vec![Effect::Characters]
    );
    for response in [CharacterCreated::OK, CharacterCreated::NAME_IN_USE] {
        let made = CharacterCreated {
            response,
            identity: character(0x5000_0004, "Cael", 0),
            result: 0,
        };
        assert_eq!(
            m.apply(&at(ServerMessage::CharacterCreated(made))),
            vec![Effect::Characters]
        );
    }
    assert_eq!(m.characters.map(|l| l.characters.len()), Some(1));
}

/// A character deleted is told on, and the list is left as it is: the account's list the server
/// sends after it shows the character gone.
///
/// Behaviour: model.characters.a-deletion-is-told-and-the-list-waits-for-the-server
#[test]
fn a_deletion_is_told_and_the_list_waits_for_the_servers() {
    let mut m = Model::new();
    let at = |message| Item {
        blob_id: 0,
        message,
    };
    let list = CharacterList {
        characters: vec![character(0x5000_0004, "Cael", 0)],
        ..CharacterList::default()
    };
    m.apply(&at(ServerMessage::CharacterList(list)));
    assert_eq!(
        m.apply(&at(ServerMessage::CharacterDeleted(CharacterDeleted {
            character: 0x5000_0004,
        }))),
        vec![Effect::CharacterDeleted(0x5000_0004)]
    );
    assert_eq!(
        m.characters.as_ref().map(|l| l.characters.len()),
        Some(1),
        "the list as it was"
    );
}

/// A character list made before the server sends one holds no characters and allows five.
///
/// Behaviour: model.characters.a-new-list-allows-five-characters
#[test]
fn a_new_list_allows_five_characters() {
    let list = CharacterList::default();
    assert!(list.characters.is_empty() && list.deleted.is_empty());
    assert_eq!(list.slots, 5);
    assert_eq!(list.status, 0);
}

/// A deleted character brought back: the answer that it is made again takes its grey off in the
/// list, where it stays one row.
///
/// Behaviour: model.characters.a-character-brought-back-loses-its-grey
#[test]
fn a_character_brought_back_loses_its_grey() {
    let mut m = Model::new();
    let at = |message| Item {
        blob_id: 0,
        message,
    };
    let greyed = CharacterIdentity {
        id: 7,
        seconds_greyed_out: 3600,
        ..CharacterIdentity::default()
    };
    let list = CharacterList {
        slots: 5,
        characters: vec![greyed.clone()],
        ..CharacterList::default()
    };
    m.apply(&at(ServerMessage::CharacterList(list)));
    m.apply(&at(ServerMessage::CharacterCreated(CharacterCreated {
        response: CharacterCreated::OK,
        identity: CharacterIdentity {
            seconds_greyed_out: 0,
            ..greyed
        },
        result: 0,
    })));
    let list = m.characters.as_ref().expect("the list");
    assert_eq!(list.characters.len(), 1);
    assert_eq!(list.characters[0].seconds_greyed_out, 0);
}

/// A character refused keeps the error the server gave, for the creation screen to say why.
///
/// Behaviour: model.characters.a-refused-character-keeps-the-servers-error
#[test]
fn a_refused_character_keeps_the_servers_error() {
    let mut m = Model::new();
    let refused = CharacterCreated {
        response: 5,
        identity: CharacterIdentity::default(),
        result: 0xC74A,
    };
    m.apply(&Item {
        blob_id: 0,
        message: ServerMessage::CharacterCreated(refused),
    });
    assert_eq!(m.creation, Some((5, 0, 1, 0xC74A)));
}
