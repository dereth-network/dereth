//! Every command id carries its index in its low 16 bits; every command the dat names resolves to
//! and packs back to the dat id; the atlatl wire index is a stance; every human motion table key
//! resolves.
//! Fixture: the shipped retail DAT records and recorded inputs.

use super::common;

use dereth_animation::data::AnimAssets;
use dereth_animation::MotionCommand;
use dereth_dat::DbType;
use dereth_primitives::DataId;

/// The Aluvian male motion table, which every human player uses.
const HUMAN_MOTION_TABLE: DataId = DataId(0x0900_0001);

/// Every full 32-bit command id the shipped motion tables carry, with how many tables carry it.
fn ids_named_by_the_shipped_dat() -> std::collections::BTreeMap<u32, usize> {
    let a = common::open();
    let mut out: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for id in a.ids_of(DbType::MTable) {
        let Some(mt) = a.motion_table(id) else {
            continue;
        };
        let mut note = |v: u32| *out.entry(v).or_default() += 1;
        note(mt.default_style.0);
        for k in mt.style_defaults.keys() {
            note(*k);
        }
        for v in mt.style_defaults.values() {
            note(v.0);
        }
        for inner in mt.links.values() {
            for k in inner.keys() {
                note(*k);
            }
        }
    }
    out
}

/// **The identity that makes the wire index readable at all**, from the table itself.
#[test]
fn every_command_id_carries_its_own_index_in_its_low_sixteen_bits() {
    for i in 0u16..412 {
        let c = MotionCommand::from_index(i).expect("the table has 412 entries");
        assert_eq!(
            c.0 & 0xFFFF,
            u32::from(i),
            "command_ids[{i}] = {:#010X} does not carry its index in its low half",
            c.0
        );
    }
    assert!(
        MotionCommand::from_index(412).is_none(),
        "and 412 is past the end"
    );
}

/// Behaviour: movement.command-table.every-wire-index-resolves-to-the-command-the-dat-names
/// **Every command the shipped dat names resolves to the id the dat gives it**, with no
/// correction list: `from_index` is what the movement unpack does with the wire's 16-bit word.
#[test]
fn every_command_the_shipped_dat_names_resolves_to_the_id_the_dat_gives_it() {
    let named = ids_named_by_the_shipped_dat();
    assert!(
        named.len() > 200,
        "the oracle must be broad: {} ids named",
        named.len()
    );

    let mut wrong: Vec<String> = Vec::new();
    for &id in named.keys() {
        #[allow(clippy::cast_possible_truncation)]
        let index = (id & 0xFFFF) as u16;
        match MotionCommand::from_index(index) {
            Some(got) if got.0 == id => {}
            got => wrong.push(format!(
                "index {index:#06X}: dat says {id:#010X}, table says {got:?}"
            )),
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} commands named by the shipped dat resolve to a different id:\n  {}",
        wrong.len(),
        named.len(),
        wrong.join("\n  ")
    );
}

/// The wire index for the atlatl stance resolves to a stance.
#[test]
fn the_wire_index_for_the_atlatl_stance_resolves_to_a_stance() {
    const ATLATL_WIRE_INDEX: u16 = 0x013B;
    let c = MotionCommand::from_index(ATLATL_WIRE_INDEX).expect("in range");
    assert!(
        c.is_style(),
        "index {ATLATL_WIRE_INDEX:#06X} must be a stance, and it is {c:?}"
    );
    assert_eq!(c, MotionCommand::ATLATL_COMBAT);
    assert_eq!(c.0, 0x8000_013B);

    // And it is a stance the shipped art can actually play: `style_defaults` is keyed by the raw
    // stance id, so a table that has the key has an idle cycle for the stance.
    let named = ids_named_by_the_shipped_dat();
    assert!(
        named.get(&0x8000_013B).copied().unwrap_or(0) >= 20,
        "the shipped motion tables must carry AtlatlCombat as a stance: {:?}",
        named.get(&0x8000_013B)
    );
    assert_eq!(
        named.get(&0x8000_0138),
        None,
        "and 0x80000138 -- the 2013 table's AtlatlCombat -- is a stance in no shipped table at all"
    );
}

/// The round trip the outbound half needs: an id the dat names packs back to the index it
/// arrived as.
#[test]
fn every_dat_command_packs_back_to_its_own_index() {
    for &id in ids_named_by_the_shipped_dat().keys() {
        #[allow(clippy::cast_possible_truncation)]
        let index = (id & 0xFFFF) as u16;
        assert_eq!(
            MotionCommand(id).to_index(),
            Some(index),
            "{id:#010X} must pack back to {index:#06X}"
        );
    }
}

/// Every command the human motion table uses resolves in the table.
#[test]
fn every_command_the_human_motion_table_uses_resolves_in_the_table() {
    let a = common::open();
    let mt = a
        .motion_table(HUMAN_MOTION_TABLE)
        .expect("the human motion table decodes");

    let mut full: Vec<u32> = vec![mt.default_style.0];
    full.extend(mt.style_defaults.keys().copied());
    full.extend(mt.style_defaults.values().map(|c| c.0));
    for inner in mt.links.values() {
        full.extend(inner.keys().copied());
    }
    for id in &full {
        #[allow(clippy::cast_possible_truncation)]
        let index = (id & 0xFFFF) as u16;
        assert_eq!(
            MotionCommand::from_index(index).map(|c| c.0),
            Some(*id),
            "the human motion table names {id:#010X}, which the command table does not hold"
        );
    }

    let packed = mt
        .cycles
        .keys()
        .chain(mt.modifiers.keys())
        .chain(mt.links.keys());
    let mut n = 0;
    for key in packed {
        #[allow(clippy::cast_possible_truncation)]
        let (style, motion) = ((key >> 16) as u16, (key & 0xFFFF) as u16);
        if style != 0 {
            let s = MotionCommand::from_index(style).unwrap_or_else(|| {
                panic!("key {key:#010X}: style index {style:#06X} is not a row")
            });
            assert!(s.is_style(), "key {key:#010X}: {s:?} is not a stance");
        }
        assert!(
            MotionCommand::from_index(motion).is_some(),
            "key {key:#010X}: motion index {motion:#06X} is not a row"
        );
        n += 1;
    }
    assert!(n > 0, "the human table contains packed keys");
    assert_eq!(n, mt.cycles.len() + mt.modifiers.len() + mt.links.len());

    // The stance emotes the human table loops in NonCombat, and the ones the command table has.
    let non_combat = MotionCommand::NON_COMBAT.0 & 0xFFFF;
    let cycled: Vec<MotionCommand> = dereth_animation::command::all()
        .map(|(_, c, _)| c)
        .filter(|c| c.0 >> 24 == 0x43)
        .filter(|c| mt.cycles.contains_key(&((non_combat << 16) | c.ordinal())))
        .collect();
    for c in [
        MotionCommand::SNOW_ANGEL_STATE,
        MotionCommand::CURTSEY_STATE,
        MotionCommand::AFK_STATE,
        MotionCommand::MEDITATE_STATE,
        MotionCommand::SIT_STATE,
        MotionCommand::SIT_CROSSLEGGED_STATE,
        MotionCommand::SIT_BACK_STATE,
        MotionCommand::AT_EASE_STATE,
    ] {
        assert!(
            cycled.contains(&c),
            "the human table plays no NonCombat cycle for {c:?}"
        );
    }
    // The 2013 numbering of the same emote is not a cycle the table has.
    assert!(
        !mt.cycles.contains_key(&((non_combat << 16) | 0x115)),
        "0x43000115, the 2013 SnowAngelState, must not be animated -- the defect's own witness"
    );
}
