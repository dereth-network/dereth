//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The game tables whose layouts changed over the life of the game read each layout from what the
//! record holds: the three ways a spell record ends, a spell table without a set table, the older
//! quality filter's seven lists, the quest table, and the ids only the older files type.
//! Fixture: records built byte by byte in the test.

use dereth_assets::{Decode, QualityFilter, QuestTable, SpellTable};
use dereth_dat::{divine_type, divine_type_in, ContainerEra, DbType};
use dereth_primitives::DataId;

fn words(ws: &[u32]) -> Vec<u8> {
    ws.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// A padded `u16`-length string, its bytes nibble-swapped when `swap`.
fn packed(s: &[u8], swap: bool) -> Vec<u8> {
    let mut b = u16::try_from(s.len()).unwrap().to_le_bytes().to_vec();
    b.extend(s.iter().map(|c| if swap { c.rotate_left(4) } else { *c }));
    while b.len() % 4 != 0 {
        b.push(0);
    }
    b
}

/// One spell keyed `key`: a name and description, the eleven scalars, a projectile meta-spell,
/// an empty formula, the effects and recovery fields, then `tail` (the words its layout ends with).
fn spell(key: u32, name: &[u8], tail: &[u32]) -> Vec<u8> {
    let mut b = words(&[key]);
    b.extend(packed(name, true));
    b.extend(packed(b"A spell.", true));
    b.extend(words(&[2, 0x0600_1000, 7, 0, 10, 0, 0, 1, 0, 1, 0]));
    b.extend(words(&[2, key]));
    b.extend(words(&[0; 8]));
    b.extend(words(&[0, 6, 0, 0, 0, 0]));
    b.extend(words(tail));
    b
}

/// A spell table of the older files holding `spells`, under the `u16` count and bucket header.
fn table(spells: &[Vec<u8>]) -> Vec<u8> {
    let mut b = words(&[0x0E00_000E]);
    b.extend(u16::try_from(spells.len()).unwrap().to_le_bytes());
    b.extend(0x0800u16.to_le_bytes());
    for s in spells {
        b.extend(s);
    }
    b
}

/// The oldest spell record ends after its recovery fields, the next adds the display order and
/// the later one the target type and per-target mana; each table reads in the ending it holds,
/// and a name whose length counts its terminating NUL reads without it.
#[test]
fn an_older_spell_table_reads_in_whichever_ending_its_spells_have() {
    let id = DataId(0x0E00_000E);
    let read = |b: &[u8]| SpellTable::decode_payload_in(ContainerEra::PreTod, id, b).unwrap();

    let t = read(&table(&[spell(1, b"Bolt\0", &[]), spell(2, b"Arc\0", &[])]));
    assert_eq!(t.spells[&1].name, "Bolt");
    assert_eq!(t.spells[&2].display_order, 0);

    let t = read(&table(&[
        spell(1, b"Bolt\0", &[40]),
        spell(2, b"Arc\0", &[41]),
    ]));
    assert_eq!(
        (t.spells[&1].display_order, t.spells[&2].display_order),
        (40, 41)
    );
    assert_eq!(t.spells[&2].non_component_target_type, 0);

    let t = read(&table(&[spell(1, b"Bolt", &[40, 0x10, 5])]));
    let s = &t.spells[&1];
    assert_eq!(
        (s.display_order, s.non_component_target_type, s.mana_mod),
        (40, 0x10, 5)
    );
    assert!(t.spellsets.is_empty());

    // A table whose spells end in none of the three ways is refused.
    assert!(SpellTable::decode_payload_in(
        ContainerEra::PreTod,
        id,
        &table(&[spell(1, b"Bolt", &[40, 0x10])])
    )
    .is_err());
}

/// The first spell tables of the later files end after the spells; the set table is read only
/// when one follows.
#[test]
fn a_later_spell_table_with_no_set_table_reads_with_no_sets() {
    let id = DataId(0x0E00_000E);
    let spells = table(&[spell(1, b"Bolt", &[40, 0x10, 5])]);
    let t = SpellTable::decode_payload(id, &spells).unwrap();
    assert!(t.spellsets.is_empty());
    assert_eq!(t.spells[&1].name, "Bolt");

    // One set (key 9) of one tier: two pieces give spells 1 and 2.
    let mut with_sets = spells;
    with_sets.extend(words(&[0x0100_0001, 9, 1, 2, 2, 1, 2]));
    let t = SpellTable::decode_payload(id, &with_sets).unwrap();
    assert_eq!(t.spellsets[&9].tiers[&2], [1, 2]);
    assert_eq!(t.spellset_bucket_index, 1);
}

/// Before Throne of Destiny there are no 64-bit integer properties: the filter has seven lists
/// and the int64 list reads empty.
#[test]
fn an_older_quality_filter_has_no_int64_list() {
    let id = DataId(0x0E00_0017);
    let mut b = words(&[id.raw(), 1, 2, 0, 0, 0, 0, 0]);
    b.extend(words(&[12, 3, 20]));
    b.extend(words(&[0, 3, 0, 2, 4, 6]));
    let f = QualityFilter::decode_payload_in(ContainerEra::PreTod, id, &b).unwrap();
    assert_eq!(f.property_lists[0], [12]);
    assert!(f.property_lists[1].is_empty());
    assert_eq!(f.property_lists[2], [3, 20]);
    assert_eq!(f.attribute_lists[1], [2, 4, 6]);
    // Read as the later layout, the eighth count swallows the first property.
    assert!(QualityFilter::decode_payload(id, &b).is_err());
}

/// A quest: its key, minimum interval, solve limit and nibble-swapped display name.
#[test]
fn the_quest_table_reads_each_quest_and_its_display_name() {
    let id = DataId(0x0E00_001B);
    let mut b = words(&[id.raw()]);
    b.extend(1u16.to_le_bytes());
    b.extend(32u16.to_le_bytes());
    b.extend(packed(b"BestowerFletching1\0", false));
    b.extend(words(&[0, 1]));
    b.extend(packed(b"Fletcher", true));
    let t = QuestTable::decode_payload_in(ContainerEra::PreTod, id, &b).unwrap();
    assert_eq!(t.buckets, 32);
    assert_eq!(t.quests[0].key, "BestowerFletching1");
    assert_eq!((t.quests[0].min_delta, t.quests[0].max_solves), (0, 1));
    assert_eq!(t.quests[0].full_name, "Fletcher");
}

/// The older files' quality filters and second region have ids the later type ranges leave
/// empty; only a record of the older files is typed by them.
#[test]
fn only_the_older_files_type_their_quality_filters_and_second_region() {
    for (id, kind) in [
        (0x0E00_0010, DbType::QualityFilter),
        (0x0E00_0017, DbType::QualityFilter),
        (0x130F_0000, DbType::Region),
    ] {
        assert_eq!(divine_type(DataId(id)), None, "{id:#010X}");
        assert_eq!(
            divine_type_in(ContainerEra::Tod, DataId(id)),
            None,
            "{id:#010X}"
        );
        assert_eq!(
            divine_type_in(ContainerEra::PreTod, DataId(id)),
            Some(kind),
            "{id:#010X}"
        );
    }
    assert_eq!(
        divine_type_in(ContainerEra::PreTod, DataId(0x0E00_001B)),
        Some(DbType::QuestDefDb)
    );
}
