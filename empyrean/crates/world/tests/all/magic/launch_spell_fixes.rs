//! The numbers a Launch world plays over the 1999 spell table. Spells carry their 1999 values, and
//! every era but Launch reads them as they are. Fixture: a synthetic spell table.

use std::collections::BTreeMap;

use dereth_assets::tables::SpellBase;
use dereth_assets::SpellTable;
use dereth_primitives::DataId;
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::era::{EraExt, EraId};
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::SpellFlags;
use empyrean_world::entity::spell::Spell;
use empyrean_world::World;

const LIFE: u32 = 2;
const ITEM: u32 = 3;
const CREATURE: u32 = 4;
const WAR: u32 = 1;
const ENCHANTMENT: u32 = 1;

const STRENGTH_SELF_I: u32 = 2;
const STRENGTH_SELF_VI: u32 = 3;
const IMPERIL_OTHER_I: u32 = 25;
const IMPERIL_OTHER_VI: u32 = 1327;
const BLOOD_DRINKER_VI: u32 = 40;
const INEPTITUDE_III: u32 = 41;
const VITAE: u32 = 666;
const HARM_OTHER_I: u32 = 7;
const ENFEEBLE_OTHER_I: u32 = 1195;
const DRAIN_MANA_OTHER_VI: u32 = 1265;
const HARM_SELF_I: u32 = 8;
const PORTAL_TIE: u32 = 47;
const SUMMON_PORTAL_II: u32 = 158;
const REJUVENATION_SELF_VI: u32 = 193;
const REJUVENATION_SELF_V: u32 = 192;
const WHIRLING_BLADE_VI: u32 = 42;

fn spell(name: &str, school: u32, meta: u32) -> SpellBase {
    SpellBase {
        name: name.to_owned(),
        description: String::new(),
        school,
        icon: 0,
        category: 0,
        bitfield: 0x12,
        base_mana: 0,
        base_range_constant: 5.0,
        base_range_mod: 1.0,
        power: 10,
        spell_economy_mod: 0.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: meta,
        meta_spell_id: 0,
        duration: None,
        portal_lifetime: None,
        raw_comps: [0; 8],
        comp_key: 0,
        comps: Vec::new(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0,
        mana_mod: 0,
    }
}

fn enchantment(name: &str, school: u32, seconds: f64) -> SpellBase {
    SpellBase {
        duration: Some((seconds, 0.0, 0.0)),
        ..spell(name, school, ENCHANTMENT)
    }
}

fn world(era: EraId) -> World {
    let spells: BTreeMap<u32, SpellBase> = [
        (
            STRENGTH_SELF_I,
            enchantment("Strength Self I", CREATURE, 150.0),
        ),
        (
            STRENGTH_SELF_VI,
            enchantment("Strength Self VI", CREATURE, 150.0),
        ),
        (IMPERIL_OTHER_I, enchantment("Imperil Other I", LIFE, 60.0)),
        (
            IMPERIL_OTHER_VI,
            enchantment("Imperil Other VI", LIFE, 300.0),
        ),
        (
            BLOOD_DRINKER_VI,
            enchantment("Blood Drinker VI", ITEM, 300.0),
        ),
        (
            INEPTITUDE_III,
            enchantment("Item Enchantment Ineptitude Other III", CREATURE, 500.0),
        ),
        (VITAE, enchantment("Vitae", LIFE, -1.0)),
        (HARM_OTHER_I, spell("Harm Other I", LIFE, 3)),
        (ENFEEBLE_OTHER_I, spell("Enfeeble Other I", LIFE, 3)),
        (DRAIN_MANA_OTHER_VI, spell("Drain Mana Other VI", LIFE, 4)),
        (HARM_SELF_I, spell("Harm Self I", LIFE, 3)),
        (PORTAL_TIE, spell("Portal Tie", ITEM, 5)),
        (SUMMON_PORTAL_II, spell("Summon Portal II", ITEM, 7)),
        (REJUVENATION_SELF_VI, spell("Rejuvenation Self VI", LIFE, 3)),
        (REJUVENATION_SELF_V, spell("Rejuvenation Self V", LIFE, 3)),
        (
            WHIRLING_BLADE_VI,
            enchantment("Whirling Blade VI", WAR, 300.0),
        ),
    ]
    .into_iter()
    .collect();
    let table = SpellTable {
        id: DataId(file_id::SPELL_TABLE),
        spell_buckets: 64,
        spells,
        spellset_bucket_index: 1,
        spellsets: BTreeMap::new(),
    };
    let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_spell_table(table)
        .build()
        .expect("fake dats");
    let clock = VirtualClock::default();
    let mut w = World::new(ClockSnapshot::take(&clock, 0.0), dats);
    w.era = era.rules();
    w
}

fn seconds(w: &World, id: u32) -> f64 {
    Spell::new(w, id, false).duration()
}

fn range(w: &World, id: u32) -> (f32, f32) {
    let s = Spell::new(w, id, false);
    (s.base_range_constant(), s.base_range_mod())
}

/// Divergence: V512
/// A Launch enchantment lasts by the level in its name, from 5 minutes at I to 17 at VI, and half
/// that for the attribute families; the 1999 table's outliers join the ramp, and Vitae, which has
/// no level, stays as it is.
#[test]
fn a_launch_enchantment_lasts_by_its_level() {
    let w = world(EraId::Launch);
    for (id, want) in [
        (STRENGTH_SELF_I, 150.0),
        (STRENGTH_SELF_VI, 510.0),
        (IMPERIL_OTHER_I, 300.0),
        (IMPERIL_OTHER_VI, 1020.0),
        (BLOOD_DRINKER_VI, 1020.0),
        (INEPTITUDE_III, 480.0),
        (VITAE, -1.0),
    ] {
        assert_eq!(seconds(&w, id), want, "spell {id}");
    }
}

/// Divergence: V512
/// Only the three schools' enchantments have the ramp: a war spell's duration, if it has one, is
/// the table's.
#[test]
fn a_launch_war_spell_keeps_the_tables_duration() {
    let w = world(EraId::Launch);
    assert_eq!(seconds(&w, WHIRLING_BLADE_VI), 300.0);
}

/// Divergence: V513
/// A Launch harm or drain on another reaches less far with skill: Harm Other I at skill 200 reaches
/// 30 m, not 75.
#[test]
fn a_launch_harm_or_drain_on_another_reaches_less_far() {
    let w = world(EraId::Launch);
    assert_eq!(range(&w, HARM_OTHER_I), (10.0, 0.10));
    assert_eq!(range(&w, ENFEEBLE_OTHER_I), (10.0, 0.10));
    assert_eq!(range(&w, DRAIN_MANA_OTHER_VI), (5.0, 0.05));
    let (constant, per_skill) = range(&w, HARM_OTHER_I);
    assert_eq!(constant + 200.0 * per_skill, 30.0);
    assert_eq!(
        range(&w, HARM_SELF_I),
        (5.0, 1.0),
        "a self spell is not one"
    );
}

/// Divergence: V513
/// Launch's portal spells are harder to cast, and Rejuvenation Self VI's power of 10 is 250; the
/// level below it keeps its own.
#[test]
fn launch_portal_spells_and_rejuvenation_vi_have_more_power() {
    let w = world(EraId::Launch);
    for (id, want) in [
        (PORTAL_TIE, 150),
        (SUMMON_PORTAL_II, 225),
        (REJUVENATION_SELF_VI, 250),
        (REJUVENATION_SELF_V, 10),
    ] {
        assert_eq!(Spell::new(&w, id, false).power(), want, "spell {id}");
    }
}

/// Divergence: V514
/// On Launch a target may resist every level of Imperil Other with Magic Defense; the rest of the
/// spell's flags stay.
#[test]
fn launch_imperil_other_is_resistable_at_every_level() {
    let w = world(EraId::Launch);
    for id in [IMPERIL_OTHER_I, IMPERIL_OTHER_VI] {
        let s = Spell::new(&w, id, false);
        assert!(s.is_resistable(), "spell {id}");
        assert_eq!(s.flags().0, 0x12 | SpellFlags::Resistable.0);
    }
    assert!(!Spell::new(&w, HARM_OTHER_I, false).is_resistable());
}

/// Divergence: V512, V513, V514
/// Every other era plays the table as it stands.
#[test]
fn only_launch_changes_the_tables_numbers() {
    for era in [EraId::Eor, EraId::Infiltration] {
        let w = world(era);
        assert_eq!(seconds(&w, STRENGTH_SELF_VI), 150.0, "{era:?}");
        assert_eq!(seconds(&w, IMPERIL_OTHER_I), 60.0, "{era:?}");
        assert_eq!(range(&w, HARM_OTHER_I), (5.0, 1.0), "{era:?}");
        assert_eq!(Spell::new(&w, PORTAL_TIE, false).power(), 10, "{era:?}");
        assert!(!Spell::new(&w, IMPERIL_OTHER_I, false).is_resistable());
    }
}

/// Divergence: V512, V513, V514
/// The dat table itself stays as it is, only what the rules read changes, so what a world serves
/// its clients is still the plain file.
#[test]
fn a_launch_world_leaves_the_table_it_read_alone() {
    let w = world(EraId::Launch);
    let held = &w.dats.portal_dat().spell_table().spells[&IMPERIL_OTHER_I];
    assert_eq!(held.duration, Some((60.0, 0.0, 0.0)));
    assert_eq!(held.bitfield, 0x12);
}
