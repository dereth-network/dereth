//! The ordinary int and float quality reads enchant exactly the properties the shipped quality
//! filter lists, and every property the client reads through the plain stored read is one the
//! filter does not list. The vital read enchants the three maxima the filter lists and leaves the
//! current values as stored.
//! Fixture: the shipped quality filter (`0x0E010001`) and vital formulas (`0x0E000003`) from the
//! retail DATs; constructed qualities and enchantments.

use dereth_assets::tables::Attribute2ndTable;
use dereth_assets::tables::QualityFilter;
use dereth_assets::Decode;
use dereth_primitives::{DataId, ObjectId};
use dereth_protocol::types::qualities::{Attribute, SecondaryAttribute, StatMod};
use {
    dereth_client_model::qualities::Qualities, dereth_client_model::qualities::StatKey,
    dereth_client_model::qualities::StatType, dereth_client_model::qualities::StatValue,
    dereth_rules::quality::QualityRead,
};
use {dereth_rules::attributes::inq_attribute_2nd, dereth_rules::attributes::vital};
use {dereth_rules::enchant::ench_type, dereth_rules::enchant::Enchantment};

/// The filter the int and float enchantment steps consult: the "Enchantable" one.
const ENCHANTABLE: DataId = DataId(0x0E01_0001);

fn shipped_filter() -> QualityFilter {
    let s = dereth_dat::testing::open_store_or_fail();
    let bytes = s
        .read_portal(ENCHANTABLE)
        .expect("the filter is in the portal dat");
    QualityFilter::decode_payload(ENCHANTABLE, &bytes).expect("the filter decodes")
}

/// A single-stat enchantment of `family` on property `key`, additive or multiplicative.
fn on(family: u32, key: u32, additive: bool, value: f32) -> Enchantment {
    let mode = if additive {
        ench_type::ADDITIVE
    } else {
        ench_type::MULTIPLICATIVE
    };
    Enchantment {
        id: key,
        // one category per property, so no two of them duel
        spell_category: u16::try_from(key).expect("a small key"),
        power_level: 1,
        start_time: 0.0,
        duration: -1.0,
        caster: ObjectId(1),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: StatMod {
            kind: family | ench_type::SINGLE_STAT | mode,
            key,
            value,
        },
        spell_set_id: None,
    }
}

const ALLEGIANCE_RANK: u32 = 30;
const ENCUMBRANCE: u32 = 5;
const HEALTH_RATE: u32 = 3;
const ITEM_ARMOR_LEVEL: u32 = 28;

/// Behaviour: enchantments.qualities.a-listed-whole-number-or-decimal-property-reads-enchanted-and-an-unlisted-one-stored
#[test]
fn a_listed_whole_number_or_decimal_property_reads_enchanted_and_an_unlisted_one_reads_stored() {
    let filter = shipped_filter();
    let mut q = Qualities::new();
    q.set(
        StatKey::new(StatType::Int, ALLEGIANCE_RANK),
        StatValue::Int(5),
    );
    q.set(
        StatKey::new(StatType::Int, ENCUMBRANCE),
        StatValue::Int(300),
    );
    q.set(
        StatKey::new(StatType::Float, HEALTH_RATE),
        StatValue::Float(0.5),
    );
    q.enchantments
        .add_list
        .push(on(ench_type::INT, ALLEGIANCE_RANK, true, 1.0));
    q.enchantments
        .add_list
        .push(on(ench_type::INT, ENCUMBRANCE, true, 50.0));
    q.enchantments
        .mult_list
        .push(on(ench_type::FLOAT, HEALTH_RATE, false, 1.5));

    // The listed whole-number property reads enchanted, the plain read stays stored.
    assert_eq!(
        q.inq_int_enchanted(ALLEGIANCE_RANK, false, Some(&filter)),
        Some(6),
        "the allegiance rank is listed: stored 5, +1"
    );
    assert_eq!(
        q.inq_int(ALLEGIANCE_RANK),
        5,
        "the raw read is the stored rank"
    );

    // The listed decimal property reads enchanted, the plain read stays stored.
    assert_eq!(
        q.inq_float_enchanted(HEALTH_RATE, Some(&filter)),
        Some(0.75),
        "the health rate is listed: stored 0.5, x1.5"
    );
    assert_eq!(
        q.inq_float(HEALTH_RATE),
        0.5,
        "the raw read is the stored rate"
    );

    // An unlisted property keeps its stored value whatever enchantment names it.
    assert_eq!(
        q.inq_int_enchanted(ENCUMBRANCE, false, Some(&filter)),
        Some(300),
        "the burden is not listed, so +50 on it is never applied"
    );

    // An absent property is not enchanted into existence, and a missing filter enchants nothing.
    assert_eq!(
        q.inq_int_enchanted(ITEM_ARMOR_LEVEL, false, Some(&filter)),
        None
    );
    assert_eq!(q.inq_int_enchanted(ALLEGIANCE_RANK, false, None), Some(5));
    assert_eq!(q.inq_float_enchanted(HEALTH_RATE, None), Some(0.5));
}

/// Every int property the client reads through the plain stored read, and the one float.
///
/// These are the character pane (burden, deaths, birth and age, fishing and chess ranks, masteries,
/// every augmentation), the vitae lamp, the stat footers and header, the paper doll, the barber,
/// the vendor and toolbar coin total, the house purchase line, the examine faction bits, the spell
/// formula's infused augmentations, the vital, skill, load, run-rate and jump inquiries, and the jump
/// stamina cost's player-killer timestamp.
const PLAIN_INT_READS: &[u32] = &[
    5, 20, 24, 25, 43, 47, 98, 113, 125, 129, 134, 139, 181, 188, 192, 199, 218, 219, 220, 221,
    222, 223, 224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 240, 241,
    242, 243, 244, 245, 246, 281, 293, 294, 295, 296, 297, 298, 299, 300, 301, 302, 309, 310, 322,
    326, 327, 328, 333, 334, 335, 336, 338, 339, 340, 342, 343, 344, 354, 355, 362, 365, 379, 390,
];
const PLAIN_FLOAT_READS: &[u32] = &[145];

/// Behaviour: enchantments.qualities.every-property-the-client-reads-plainly-is-outside-the-shipped-filter
#[test]
fn every_property_the_client_reads_plainly_is_outside_the_shipped_filter() {
    let filter = shipped_filter();
    let listed_ints: Vec<u32> = PLAIN_INT_READS
        .iter()
        .copied()
        .filter(|id| filter.allows_int(*id))
        .collect();
    assert_eq!(
        listed_ints,
        Vec::<u32>::new(),
        "no plainly-read int is listed"
    );
    let listed_floats: Vec<u32> = PLAIN_FLOAT_READS
        .iter()
        .copied()
        .filter(|id| filter.allows_float(*id))
        .collect();
    assert_eq!(
        listed_floats,
        Vec::<u32>::new(),
        "no plainly-read float is listed"
    );
    // The premise: the filter does list the rank, which the allegiance tab reads enchanted.
    assert!(filter.allows_int(ALLEGIANCE_RANK));
    assert!(filter.allows_float(HEALTH_RATE));
}

/// The shipped vital formulas: maximum health is half Endurance, maximum stamina is Endurance and
/// maximum mana is Self.
const VITAL_FORMULAS: DataId = DataId(0x0E00_0003);

fn shipped_vital_formulas() -> Attribute2ndTable {
    let s = dereth_dat::testing::open_store_or_fail();
    let bytes = s
        .read_portal(VITAL_FORMULAS)
        .expect("the vital formulas are in the portal dat");
    Attribute2ndTable::decode_payload(VITAL_FORMULAS, &bytes).expect("the formulas decode")
}

/// Every attribute at 100 and no vital ranks bought: maxima of 50 health, 100 stamina and 100 mana,
/// with the current values below them at 40, 90 and 80.
fn a_character_below_full_vitals() -> Qualities {
    let mut q = Qualities::new();
    for id in 1..=6 {
        q.set_attribute(
            id,
            Attribute {
                init_level: 100,
                level_from_cp: 0,
                cp_spent: 0,
            },
        );
    }
    for (max, current) in [(1, 40), (3, 90), (5, 80)] {
        q.set_attribute_2nd(
            max,
            SecondaryAttribute {
                attribute: Attribute::default(),
                current_level: current,
            },
        );
    }
    q
}

/// The vitals as the panel reads them, in the order maximum health, health, maximum stamina,
/// stamina, maximum mana, mana.
fn vitals(q: &Qualities, t: &Attribute2ndTable, filter: &QualityFilter, raw: bool) -> [u32; 6] {
    [
        vital::MAX_HEALTH,
        vital::HEALTH,
        vital::MAX_STAMINA,
        vital::STAMINA,
        vital::MAX_MANA,
        vital::MANA,
    ]
    .map(|id| inq_attribute_2nd(q, t, id, raw, Some(filter)).expect("every vital reads"))
}

/// Behaviour: enchantments.vitals.an-all-vitals-multiplier-scales-the-maxima-and-not-the-current-values
#[test]
fn an_all_vitals_multiplier_scales_the_maxima_and_leaves_the_current_values_as_stored() {
    let filter = shipped_filter();
    let t = shipped_vital_formulas();
    let mut q = a_character_below_full_vitals();
    assert_eq!(
        vitals(&q, &t, &filter, false),
        [50, 40, 100, 90, 100, 80],
        "unenchanted, the reads are the formulas and the stored current values"
    );
    // One spell multiplying every vital by 0.6, the shape of the swamp blights and the culinary
    // debuff: the whole secondary-attribute family, whatever the key.
    q.enchantments.mult_list.push(Enchantment {
        id: 1,
        spell_category: 1,
        power_level: 1,
        start_time: 0.0,
        duration: -1.0,
        caster: ObjectId(1),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: StatMod {
            kind: ench_type::SECOND_ATT | ench_type::MULTIPLE_STAT | ench_type::MULTIPLICATIVE,
            key: 0,
            value: 0.6,
        },
        spell_set_id: None,
    });
    assert_eq!(
        vitals(&q, &t, &filter, false),
        [30, 40, 60, 90, 60, 80],
        "the maxima are scaled by 0.6, the current values are not"
    );
    assert_eq!(
        vitals(&q, &t, &filter, true),
        [50, 40, 100, 90, 100, 80],
        "the raw read ignores the spell entirely"
    );
}

/// Behaviour: enchantments.vitals.a-buff-on-a-maximum-still-raises-it
#[test]
fn a_buff_on_a_vital_maximum_still_raises_it_while_one_naming_the_current_value_does_not() {
    let filter = shipped_filter();
    let t = shipped_vital_formulas();
    let mut q = a_character_below_full_vitals();
    q.enchantments
        .add_list
        .push(on(ench_type::SECOND_ATT, vital::MAX_HEALTH, true, 20.0));
    q.enchantments
        .add_list
        .push(on(ench_type::SECOND_ATT, vital::HEALTH, true, 5.0));
    let [max_health, health, ..] = vitals(&q, &t, &filter, false);
    assert_eq!(max_health, 70, "maximum health 50, +20");
    assert_eq!(
        health, 40,
        "the current health stays as stored under a spell naming it"
    );
}
