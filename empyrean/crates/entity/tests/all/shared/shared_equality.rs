//! Vectors: ACE opcode tables and the shared protocol, movement and animation constants
//! Helpers for shared_sets plus checks: ACE opcode enums within the master table but for known
//! differences, movement params/options defaults, shared subsets are ACE members, motion command
//! names/values.
//! Fixture: enum values and synthetic entity records.

use std::collections::BTreeSet;

use empyrean_entity::enums::*;

use super::ace_server_opcodes::{GAME_ACTION_TYPE, GAME_EVENT_TYPE, GAME_MESSAGE_OPCODE};
use super::shared_sets::KNOWN_OPCODES_ACE_ONLY;

/// A genuine difference: the value, ACE's or retail's name for it, and why it differs.
pub(crate) type Known = (u64, &'static str, &'static str);

pub(crate) fn set(values: impl IntoIterator<Item = u64>) -> BTreeSet<u64> {
    values.into_iter().collect()
}

fn ace_set<E: AceEnum>() -> BTreeSet<u64> {
    set(E::MEMBERS.iter().map(|e| e.key()))
}

/// A shared constant as a set key: sign-extended to 64 bits, the way an ACE enum's `key()` is
/// (.NET's `ToUInt64`), so a signed shared constant meets a signed ACE member.
#[allow(clippy::cast_sign_loss)]
pub(crate) fn k<T: Into<i64>>(v: T) -> u64 {
    v.into() as u64
}

/// Asserts that `ace` and `shared` differ by exactly the listed values.
pub(crate) fn assert_sets(
    what: &str,
    ace: &BTreeSet<u64>,
    shared: &BTreeSet<u64>,
    ace_only: &[Known],
    shared_only: &[Known],
) {
    let got_ace_only: Vec<u64> = ace.difference(shared).copied().collect();
    let got_shared_only: Vec<u64> = shared.difference(ace).copied().collect();
    let want_ace_only: Vec<u64> = set(ace_only.iter().map(|k| k.0)).into_iter().collect();
    let want_shared_only: Vec<u64> = set(shared_only.iter().map(|k| k.0)).into_iter().collect();
    let hex = |v: &[u64]| {
        v.iter()
            .map(|x| format!("{x:#x}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    assert_eq!(
        hex(&got_ace_only),
        hex(&want_ace_only),
        "{what}: values only ACE has (left: found, right: KNOWN_DIFFERENCES)"
    );
    assert_eq!(
        hex(&got_shared_only),
        hex(&want_shared_only),
        "{what}: values only the shared crate has (left: found, right: KNOWN_DIFFERENCES)"
    );
}

pub(crate) fn assert_enum<E: AceEnum>(shared: &[u64], ace_only: &[Known], shared_only: &[Known]) {
    assert_sets(
        E::TYPE_NAME,
        &ace_set::<E>(),
        &set(shared.iter().copied()),
        ace_only,
        shared_only,
    );
}

// ---------------------------------------------------------------------------------------------
// Opcodes (ACE.Server's three opcode enums against dereth-protocol's master table)
// ---------------------------------------------------------------------------------------------

/// dereth-protocol's master opcode table.
pub(crate) fn opcode_table() -> BTreeSet<u64> {
    set(dereth_protocol::OPCODES
        .iter()
        .map(|i| u64::from(i.opcode.0)))
}

fn fixture(rows: &[(&str, u32)]) -> BTreeSet<u64> {
    set(rows.iter().map(|r| u64::from(r.1)))
}

/// ACE.Server's three opcode enums together.
pub(crate) fn ace_opcodes() -> BTreeSet<u64> {
    let mut ace = fixture(GAME_MESSAGE_OPCODE);
    ace.extend(fixture(GAME_ACTION_TYPE));
    ace.extend(fixture(GAME_EVENT_TYPE));
    ace
}

#[test]
fn each_ace_opcode_enum_is_within_the_table_but_for_its_known_differences() {
    let table = opcode_table();
    let known = set(KNOWN_OPCODES_ACE_ONLY.iter().map(|k| k.0));
    for (name, rows) in [
        ("GameMessageOpcode", GAME_MESSAGE_OPCODE),
        ("GameActionType", GAME_ACTION_TYPE),
        ("GameEventType", GAME_EVENT_TYPE),
    ] {
        for &(member, v) in rows {
            assert!(
                table.contains(&u64::from(v)) || known.contains(&u64::from(v)),
                "{name}.{member} = {v:#06x} is not in the opcode table"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Checks beyond the value sets
// ---------------------------------------------------------------------------------------------

/// Known difference: ACE's `MovementParamsExtensions.Default` (0xEE0F) lacks `StopCompletely`,
/// which the client's default (0x1EE0F) has. Nothing in ACE reads that field.
#[test]
fn movement_params_default_lacks_only_stop_completely() {
    let ace_default = ext::movement_params_extensions::DEFAULT;
    assert_eq!(ace_default.0, 0xEE0F);
    assert_eq!(
        (ace_default | MovementParams::StopCompletely).0,
        dereth_animation::motion::DEFAULT_FLAGS
    );
    assert_eq!(
        dereth_animation::motion::DEFAULT_FLAGS,
        dereth_protocol::movement::MovementParameters::DEFAULT_BITFIELD
    );
}

#[test]
fn character_options2_default_is_the_player_modules() {
    assert_eq!(
        CharacterOptions2::Default.0,
        dereth_protocol::login::PlayerModule::DEFAULT_OPTIONS2
    );
}

/// The shared crates name only a subset of these ACE sets; every shared value must be ACE's.
#[test]
fn shared_subsets_are_ace_members() {
    use dereth_protocol::types::qualities::enchantment_type as et;
    for v in [et::STAT_TYPES, et::VITAE, et::COOLDOWN, et::BENEFICIAL] {
        assert!(
            EnchantmentTypeFlags(v as i32).is_defined(),
            "EnchantmentTypeFlags {v:#x}"
        );
    }
    use dereth_animation::motion::interp as mi;
    for v in [
        mi::NO_PHYSICS_OBJECT,
        mi::NO_MOTION_INTERPRETER,
        mi::YOU_CANT_JUMP_WHILE_IN_THE_AIR,
        mi::ACTION_CANCELLED,
        mi::OBJECT_GONE,
        mi::NO_OBJECT,
        mi::YOU_CHARGED_TOO_FAR,
        mi::CANT_CROUCH_IN_COMBAT,
        mi::CANT_SIT_IN_COMBAT,
        mi::CANT_LIE_DOWN_IN_COMBAT,
        mi::CANT_CHAT_EMOTE_IN_COMBAT,
        mi::TOO_MANY_ACTIONS,
        mi::GENERAL_MOVEMENT_FAILURE,
        mi::YOU_CANT_JUMP_FROM_THIS_POSITION,
        mi::CANT_JUMP_LOADED_DOWN,
    ] {
        assert!(WeenieError(v as i32).is_defined(), "WeenieError {v:#x}");
    }
}

/// `MotionCommand` against the client's command table by name as well as by value (V331, V331):
/// every command both sides name has the same id (only `Invalid` differs, 0 against 0x80000000),
/// and the UI target-selection block, which ACE numbers the 2013 way, is the client's, index for
/// index: the server's command for each raw index 0x10F..0x117 is the client's command there.
#[test]
fn motion_command_names_and_values_match_the_client_table() {
    let mut named_by_both = 0;
    for (_, c, name) in dereth_animation::command::all() {
        let Some(m) = MotionCommand::from_name(name) else {
            continue;
        };
        named_by_both += 1;
        if name == "Invalid" {
            continue;
        }
        assert_eq!(
            m.0, c.0,
            "{name}: server {:#010x}, client {:#010x}",
            m.0, c.0
        );
    }
    assert!(
        named_by_both > 400,
        "the tables share their names ({named_by_both})"
    );
    let block = [
        "SkillHealOther",
        "CombatEat",
        "CombatDrink",
        "NextMonster",
        "PreviousMonster",
        "ClosestMonster",
        "NextPlayer",
        "PreviousPlayer",
        "ClosestPlayer",
    ];
    for (i, want) in (0x10F_u16..=0x117).zip(block) {
        let client =
            dereth_animation::command::MotionCommand::from_index(i).expect("a client command");
        assert_eq!(client.name(), Some(want), "client index {i:#x}");
        let server: Vec<_> = MotionCommand::ALL
            .iter()
            .filter(|m| m.0 & 0xFFFF == u32::from(i))
            .collect();
        assert_eq!(server.len(), 1, "one server command at index {i:#x}");
        assert_eq!(server[0].0, client.0, "index {i:#x}");
        assert_eq!(server[0].name(), Some(want), "index {i:#x}");
    }
}
