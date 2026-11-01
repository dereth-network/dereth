//! What the player's requests carry: an examination request's packed form, a selection's packed
//! form, a typed line's command and words.

use auberean_client_runtime::examination::{Request, OBJECT, TARGET};
use auberean_client_runtime::selection::{Selection, WITH_NUMBERS};
use auberean_client_runtime::shell::{command_line, rest_of, words};

fn words_of(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// An examination request packs its kind and its admin word, then by kind: the object and the
/// tabs for an object; the object, the mode, the skill level and the tabs for a target; the
/// enumerated datum or the data id for the kinds that carry one; nothing more for a request about
/// nothing, which is what a new one is (asking for the tabs 0x203). A packed request reads back
/// as itself.
///
/// Behaviour: script.natives.examination-request-packs-by-kind
#[test]
fn an_examination_request_packs_what_its_kind_carries() {
    let new = Request::default();
    assert_eq!(new.current, 0x203);
    assert_eq!(words_of(&new.pack()), [0, 0]);
    let object = Request {
        kind: OBJECT,
        object: 0x8130_0000_1000_0002,
        current: 0x3FF,
        ..Request::default()
    };
    assert_eq!(
        words_of(&object.pack()),
        [OBJECT, 0, 0x1000_0002, 0x8130_0000, 0x3FF]
    );
    let target = Request {
        kind: TARGET,
        object: 0x8130_0000_1000_0002,
        mode: 0x4000_0001,
        skill_level: 2,
        current: 0x3FF,
        ..Request::default()
    };
    assert_eq!(
        words_of(&target.pack()),
        [TARGET, 0, 0x1000_0002, 0x8130_0000, 0x4000_0001, 2, 0x3FF]
    );
    let effect = Request {
        kind: 0x4000_0001,
        data_enum: 7,
        data_id: 9,
        ..Request::default()
    };
    assert_eq!(words_of(&effect.pack()), [0x4000_0001, 0, 7]);
    let recipe = Request {
        kind: 0x4000_0007,
        data_enum: 7,
        data_id: 9,
        ..Request::default()
    };
    assert_eq!(words_of(&recipe.pack()), [0x4000_0007, 0, 9]);
    for r in [object, target] {
        assert_eq!(Request::unpack(&r.pack()), r);
    }
}

/// A selection packs its type, then its six numbers only when it is of the type that carries
/// them; a new selection is all zeros, and a packed selection reads back as itself.
///
/// Behaviour: script.natives.selection-packs-its-numbers-by-type
#[test]
fn a_selection_packs_its_numbers_only_for_the_type_that_carries_them() {
    let new = Selection::default();
    assert_eq!(words_of(&new.pack()), [0]);
    let numbers = Selection {
        kind: WITH_NUMBERS,
        cur_health: 1,
        pk_damage: 2,
        max_health: 3,
        cur_vigor: 4,
        pk_vigor_loss: 5,
        max_vigor: 6,
    };
    assert_eq!(words_of(&numbers.pack()), [WITH_NUMBERS, 1, 2, 3, 4, 5, 6]);
    assert_eq!(Selection::unpack(&numbers.pack()), numbers);
    let other = Selection {
        kind: 0x4000_0002,
        ..numbers
    };
    assert_eq!(words_of(&other.pack()), [0x4000_0002]);
    assert_eq!(
        Selection::unpack(&other.pack()),
        Selection {
            kind: 0x4000_0002,
            ..Selection::default()
        }
    );
}

/// A typed line is a command's line: a line that is no command gets the say command put first,
/// a command's marks are cut; its words are split at spaces, a quoted run one word without its
/// quotes (a quote not closed refuses the line), the first naming the command; the say command is
/// handed its words and the rest of the line after the command word.
///
/// Behaviour: session.interface.say-hands-the-line-and-its-words
#[test]
fn a_typed_line_splits_into_words_with_quoted_runs_kept_whole() {
    assert_eq!(command_line("hello", "say").as_deref(), Ok("say hello"));
    assert_eq!(
        command_line("/say message", "say").as_deref(),
        Ok("say message")
    );
    let line = "say Well met.";
    let w = words(line).expect("the words");
    assert_eq!(w, ["say", "Well", "met."]);
    assert_eq!(rest_of(line, &w[0]), "Well met.");
    assert_eq!(
        words("say  two   spaces ").expect("the words"),
        ["say", "two", "spaces"]
    );
    assert_eq!(
        words("tell \"Elara Brightwind\" hello").expect("the words"),
        ["tell", "Elara Brightwind", "hello"]
    );
    assert_eq!(words("tell \"Elara"), None, "a quote not closed");
}
