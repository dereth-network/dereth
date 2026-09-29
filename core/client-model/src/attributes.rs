//! Attributes, vitals and their derivations.
//!
//! The client has **no vital regeneration tick of its own.** Current health, stamina and mana change
//! only when the server sends `Qualities_UpdateAttribute2nd`. The bar interpolates its *fill*; the
//! number is never predicted.
//!
//! The pure rules of this module live in [`dereth_rules::attributes`]; they are re-exported
//! here, so every `dereth_client_model::attributes::*` path resolves. So do the inquiries
//! (`inq_attribute`, `inq_attribute_2nd` and the rest), written against
//! `dereth_rules::quality::QualityRead`.

use crate::enchant::EnchantmentRegistry;
use crate::qualities::Qualities;
use dereth_assets::tables::{Attribute2ndTable, QualityFilter};

pub use dereth_rules::attributes::*;

/// True for the three *current* vital ids (2, 4, 6), the only ones a received value is clamped on.
#[must_use]
pub const fn is_current_vital(id: u32) -> bool {
    matches!(id, vital::HEALTH | vital::STAMINA | vital::MANA)
}

/// Behavior: the bounds check a received current-only vital update goes through before it is
/// stored. Any other id passes unchanged. A value that is negative when read as a signed 32-bit
/// number becomes 0 without consulting the maximum. Otherwise the value is capped at the
/// enchanted maximum (`inq_attribute_2nd(id - 1)`, enchanted as `filter` allows), computed from
/// the qualities as they stand *before* the store; `None` means the maximum could not be
/// computed, and the update is then refused rather than stored.
#[must_use]
pub fn bounds_check(
    q: &Qualities,
    table: &Attribute2ndTable,
    filter: Option<&QualityFilter>,
    id: u32,
    value: u32,
) -> Option<u32> {
    if !is_current_vital(id) {
        return Some(value);
    }
    #[allow(clippy::cast_possible_wrap)] // the check reads the value as signed
    if (value as i32) < 0 {
        return Some(0);
    }
    let max = inq_attribute_2nd(q, table, id - 1, false, filter)?;
    Some(value.min(max))
}

/// Behavior: the clamp a received whole secondary-attribute record goes through before it is
/// stored. Only a current id (2, 4, 6) is touched: its `current_level` is capped at the enchanted
/// maximum computed before the store (no signed-zero step on this path). `None` refuses the update,
/// as in [`bounds_check`]. Servers send the whole record under the *maximum* id (1, 3, 5), so in
/// practice this path stores the record as received.
#[must_use]
pub fn bounds_check_record(
    q: &Qualities,
    table: &Attribute2ndTable,
    filter: Option<&QualityFilter>,
    id: u32,
    mut v: dereth_protocol::types::qualities::SecondaryAttribute,
) -> Option<dereth_protocol::types::qualities::SecondaryAttribute> {
    if is_current_vital(id) {
        let max = inq_attribute_2nd(q, table, id - 1, false, filter)?;
        v.current_level = v.current_level.min(max);
    }
    Some(v)
}

/// The enchanted-attribute clamps, hoisted so a caller can reason about them without a registry.
///
/// The attribute enchant: floor 1 when the raw value is below 10, else floor 10.
/// The secondary-attribute enchant: floor 1 below 5, else floor 5, **after vitae**.
#[must_use]
pub fn enchant_attribute(reg: &EnchantmentRegistry, id: u32, raw: i32) -> i32 {
    reg.enchant_attribute(id, raw)
}

// ---------------------------------------------------------------------------------------------
// The body-part table.
// ---------------------------------------------------------------------------------------------

/// `ArmorCache` (0x28) — nine `int32`s in wire order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArmorCache {
    pub base_armor: i32,
    pub armor_vs_slash: i32,
    pub armor_vs_pierce: i32,
    pub armor_vs_bludgeon: i32,
    pub armor_vs_cold: i32,
    pub armor_vs_fire: i32,
    pub armor_vs_acid: i32,
    pub armor_vs_electric: i32,
    pub armor_vs_nether: i32,
}

/// The body-part selection data (0x34) — twelve floats, the probability of hitting this part from
/// each of the twelve (height × side × facing) buckets, in wire order.
pub const BODY_PART_SELECTION_ORDER: [&str; 12] = [
    "HLF", "MLF", "LLF", "HRF", "MRF", "LRF", "HLB", "MLB", "LLB", "HRB", "MRB", "LRB",
];

/// `BodyPart` (0x40).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BodyPart {
    /// `DAMAGE_TYPE` this part deals.
    pub dtype: u32,
    pub dval: i32,
    pub dvar: f32,
    pub acache: ArmorCache,
    /// `BODY_HEIGHT`: `HIGH` = 1, `MEDIUM` = 2, `LOW` = 3.
    pub bh: u32,
    pub bpsd: Option<[f32; 12]>,
}

/// `BODY_HEIGHT`.
pub mod body_height {
    pub const HIGH: u32 = 1;
    pub const MEDIUM: u32 = 2;
    pub const LOW: u32 = 3;
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::tables::SkillFormula;
    use dereth_protocol::types::qualities::{Attribute, AttributeCache, SecondaryAttribute};

    /// A quality filter shaped like the shipped one for the secondary attributes: the three maxima
    /// may be enchanted, the current values may not.
    fn maxima() -> QualityFilter {
        QualityFilter {
            id: dereth_primitives::DataId(0x0E01_0001),
            property_lists: Default::default(),
            attribute_lists: [
                Vec::new(),
                vec![vital::MAX_HEALTH, vital::MAX_STAMINA, vital::MAX_MANA],
                Vec::new(),
            ],
        }
    }

    fn formula(w: u32, x: u32, y: u32, z: u32, a1: u32, a2: u32) -> SkillFormula {
        SkillFormula {
            w,
            x,
            y,
            z,
            attr1: a1,
            attr2: a2,
        }
    }

    fn qualities_with(str_: u32, end: u32, health_cp: u32, current: u32) -> Qualities {
        let mut q = Qualities::new();
        q.attributes = Some(AttributeCache {
            flags: 0x1FF,
            strength: Some(Attribute {
                init_level: str_,
                level_from_cp: 0,
                cp_spent: 0,
            }),
            endurance: Some(Attribute {
                init_level: end,
                level_from_cp: 0,
                cp_spent: 0,
            }),
            health: Some(SecondaryAttribute {
                attribute: Attribute {
                    init_level: health_cp,
                    level_from_cp: 0,
                    cp_spent: 0,
                },
                current_level: current,
            }),
            ..AttributeCache::default()
        });
        q
    }

    /// Oracle: §3's attribute lookup — the base value is `_init_level + _level_from_cp`.
    #[test]
    fn attribute_base_is_init_plus_level_from_cp() {
        let mut q = qualities_with(100, 120, 0, 0);
        assert_eq!(inq_attribute_base(&q, attribute::STRENGTH), Some(100));
        if let Some(c) = q.attributes.as_mut() {
            if let Some(a) = c.strength.as_mut() {
                a.level_from_cp = 15;
            }
        }
        assert_eq!(inq_attribute_base(&q, attribute::STRENGTH), Some(115));
        assert_eq!(
            inq_attribute_base(&q, 99),
            None,
            "an id outside 1..=6 has no attribute"
        );
    }

    /// Oracle: `12-skills-and-advancement.md` §4 — the vital is `formula + GearMaxHealth + stored`,
    /// and `GearMaxHealth` applies to `MaxHealth` **only**.
    #[test]
    fn max_health_picks_up_gear_max_health_and_nothing_else_does() {
        use crate::qualities::{StatKey, StatType, StatValue};
        let table = Attribute2ndTable {
            id: dereth_primitives::DataId(0x0E00_0003),
            // MaxHealth = Endurance / 2
            health: formula(0, 1, 0, 2, attribute::ENDURANCE, 0),
            stamina: formula(0, 1, 0, 1, attribute::ENDURANCE, 0),
            mana: formula(0, 1, 0, 1, attribute::SELF, 0),
        };
        let mut q = qualities_with(100, 200, 30, 55);
        // 200/2 = 100 base, + 30 stored ranks.
        assert_eq!(
            inq_attribute_2nd(&q, &table, vital::MAX_HEALTH, true, Some(&maxima())),
            Some(130)
        );
        q.set(StatKey::new(StatType::Int, 379), StatValue::Int(25));
        assert_eq!(
            inq_attribute_2nd(&q, &table, vital::MAX_HEALTH, true, Some(&maxima())),
            Some(155)
        );
        // MaxStamina has no stored SecondaryAttribute here, so it is the bare formula.
        assert_eq!(
            inq_attribute_2nd(&q, &table, vital::MAX_STAMINA, true, Some(&maxima())),
            Some(200)
        );
        // The current value is the stored `_current_level`; no formula base is added.
        assert_eq!(
            inq_attribute_2nd(&q, &table, vital::HEALTH, true, Some(&maxima())),
            Some(55)
        );
    }

    /// Enlightenment (property 390): absent, nothing changes; above zero, twice its count lands on
    /// maximum health only, before the multiplier, which scales it.
    ///
    /// Behaviour: attributes.enlightenment.each-level-adds-two-to-maximum-health-and-nothing-else
    #[test]
    fn enlightenment_adds_twice_its_count_to_max_health_only() {
        use crate::qualities::{StatKey, StatType, StatValue};
        let table = Attribute2ndTable {
            id: dereth_primitives::DataId(0x0E00_0003),
            health: formula(0, 1, 0, 2, attribute::ENDURANCE, 0),
            stamina: formula(0, 1, 0, 1, attribute::ENDURANCE, 0),
            mana: formula(0, 1, 0, 1, attribute::SELF, 0),
        };
        let mut q = qualities_with(100, 200, 30, 55);
        let vitals = |q: &Qualities| {
            [
                vital::MAX_HEALTH,
                vital::MAX_STAMINA,
                vital::MAX_MANA,
                vital::HEALTH,
            ]
            .map(|id| inq_attribute_2nd(q, &table, id, false, Some(&maxima())))
        };
        assert_eq!(vitals(&q), [Some(130), Some(200), None, Some(55)]);
        q.set(
            StatKey::new(StatType::Int, crate::skills::aug::ENLIGHTENMENT),
            StatValue::Int(3),
        );
        assert_eq!(vitals(&q), [Some(136), Some(200), None, Some(55)]);
        assert_eq!(
            inq_attribute_2nd(&q, &table, vital::MAX_HEALTH, true, Some(&maxima())),
            Some(136)
        );

        // x1.5 on MaxHealth: (100 + 30 + 6) * 1.5 = 204.
        q.enchantments
            .mult_list
            .push(crate::skills::tests::multiplier(
                crate::enchant::ench_type::SECOND_ATT,
                vital::MAX_HEALTH,
                1.5,
            ));
        assert_eq!(
            inq_attribute_2nd(&q, &table, vital::MAX_HEALTH, false, Some(&maxima())),
            Some(204)
        );
        assert_eq!(
            inq_attribute_2nd(&q, &table, vital::MAX_HEALTH, true, Some(&maxima())),
            Some(136)
        );
    }

    /// Gear max health (property 379) is added as stored: a negative value lowers maximum health.
    #[test]
    fn a_negative_gear_max_health_lowers_maximum_health() {
        use crate::qualities::{StatKey, StatType, StatValue};
        let table = Attribute2ndTable {
            id: dereth_primitives::DataId(0x0E00_0003),
            health: formula(0, 1, 0, 2, attribute::ENDURANCE, 0),
            stamina: formula(0, 1, 0, 1, attribute::ENDURANCE, 0),
            mana: formula(0, 1, 0, 1, attribute::SELF, 0),
        };
        let mut q = qualities_with(100, 200, 30, 55);
        assert_eq!(
            inq_attribute_2nd(&q, &table, vital::MAX_HEALTH, true, Some(&maxima())),
            Some(130)
        );
        q.set(StatKey::new(StatType::Int, 379), StatValue::Int(-10));
        assert_eq!(
            inq_attribute_2nd(&q, &table, vital::MAX_HEALTH, true, Some(&maxima())),
            Some(120)
        );
        q.set(StatKey::new(StatType::Int, 379), StatValue::Int(15));
        assert_eq!(
            inq_attribute_2nd(&q, &table, vital::MAX_HEALTH, true, Some(&maxima())),
            Some(145)
        );
    }

    /// A current vital read without the formula table is the unraw full read: the stored current
    /// level, enchanted only when the filter lists the id; a maximum has no such read.
    #[test]
    fn a_current_vital_is_enchanted_only_where_the_filter_lists_it() {
        let table = Attribute2ndTable {
            id: dereth_primitives::DataId(0x0E00_0003),
            health: formula(0, 1, 0, 2, attribute::ENDURANCE, 0),
            stamina: formula(0, 1, 0, 1, attribute::ENDURANCE, 0),
            mana: formula(0, 1, 0, 1, attribute::SELF, 0),
        };
        let mut q = qualities_with(100, 200, 30, 55);
        q.enchantments
            .mult_list
            .push(crate::skills::tests::multiplier(
                crate::enchant::ench_type::SECOND_ATT,
                vital::HEALTH,
                2.0,
            ));
        let mut current_too = maxima();
        current_too.attribute_lists[1].push(vital::HEALTH);
        for filter in [None, Some(maxima()), Some(current_too.clone())] {
            assert_eq!(
                inq_current_vital(&q, vital::HEALTH, filter.as_ref()),
                inq_attribute_2nd(&q, &table, vital::HEALTH, false, filter.as_ref()),
                "the table-free read agrees with the full read under {filter:?}"
            );
        }
        assert_eq!(inq_current_vital(&q, vital::HEALTH, None), Some(55));
        assert_eq!(
            inq_current_vital(&q, vital::HEALTH, Some(&maxima())),
            Some(55),
            "the shipped shape leaves the current value as stored"
        );
        assert_eq!(
            inq_current_vital(&q, vital::HEALTH, Some(&current_too)),
            Some(110),
            "a filter that lists the current value lets the multiplier reach it"
        );
        assert_eq!(inq_current_vital(&q, vital::MAX_HEALTH, None), None);
        assert_eq!(inq_current_vital(&q, vital::STAMINA, None), None);
    }

    /// Oracle: §3's — only the three *current* ids are clamped.
    #[test]
    fn bounds_check_clamps_only_the_current_vitals() {
        let table = Attribute2ndTable {
            id: dereth_primitives::DataId(0x0E00_0003),
            health: formula(0, 1, 0, 2, attribute::ENDURANCE, 0),
            stamina: formula(0, 1, 0, 1, attribute::ENDURANCE, 0),
            mana: formula(0, 1, 0, 1, attribute::SELF, 0),
        };
        let mut q = qualities_with(100, 200, 30, 55);
        assert_eq!(
            bounds_check(&q, &table, Some(&maxima()), vital::HEALTH, 999),
            Some(130)
        );
        assert_eq!(
            bounds_check(&q, &table, Some(&maxima()), vital::HEALTH, 12),
            Some(12)
        );
        assert_eq!(
            bounds_check(&q, &table, Some(&maxima()), vital::MAX_HEALTH, 999),
            Some(999),
            "maxima pass"
        );
        assert_eq!(
            bounds_check(&q, &table, Some(&maxima()), 8, 999),
            Some(999),
            "only 2, 4 and 6 are current vitals"
        );
        // Read as signed, a negative value is zeroed without consulting the maximum.
        assert_eq!(
            bounds_check(&q, &table, Some(&maxima()), vital::HEALTH, u32::MAX),
            Some(0)
        );
        assert_eq!(
            bounds_check(&q, &table, Some(&maxima()), vital::MANA, 0x8000_0000),
            Some(0)
        );
        // Stamina: no stored record, so the maximum is the bare formula (Endurance = 200).
        assert_eq!(
            bounds_check(&q, &table, Some(&maxima()), vital::STAMINA, 250),
            Some(200)
        );
        // Mana: no record and a formula that evaluates to 0 (no Self) -- the maximum cannot be
        // computed, and the check refuses rather than clamping to 0.
        assert_eq!(
            bounds_check(&q, &table, Some(&maxima()), vital::MANA, 5),
            None
        );
        // The maximum is the enchanted one: x0.5 on MaxHealth caps a received 100 at 65.
        q.enchantments
            .mult_list
            .push(crate::skills::tests::multiplier(
                crate::enchant::ench_type::SECOND_ATT,
                vital::MAX_HEALTH,
                0.5,
            ));
        assert_eq!(
            bounds_check(&q, &table, Some(&maxima()), vital::HEALTH, 100),
            Some(65)
        );
    }

    /// The whole-record form: only a current id is capped, there is no signed-zero step, and a
    /// maximum that cannot be computed refuses the record.
    #[test]
    fn a_whole_record_is_capped_only_under_a_current_id() {
        let table = Attribute2ndTable {
            id: dereth_primitives::DataId(0x0E00_0003),
            health: formula(0, 1, 0, 2, attribute::ENDURANCE, 0),
            stamina: formula(0, 1, 0, 1, attribute::ENDURANCE, 0),
            mana: formula(0, 1, 0, 1, attribute::SELF, 0),
        };
        let q = qualities_with(100, 200, 30, 55);
        let rec = |current| SecondaryAttribute {
            attribute: Attribute {
                init_level: 30,
                level_from_cp: 0,
                cp_spent: 0,
            },
            current_level: current,
        };
        assert_eq!(
            bounds_check_record(&q, &table, Some(&maxima()), vital::MAX_HEALTH, rec(999)),
            Some(rec(999))
        );
        assert_eq!(
            bounds_check_record(&q, &table, Some(&maxima()), vital::HEALTH, rec(999)),
            Some(rec(130))
        );
        assert_eq!(
            bounds_check_record(&q, &table, Some(&maxima()), vital::HEALTH, rec(u32::MAX)),
            Some(rec(130))
        );
        assert_eq!(
            bounds_check_record(&q, &table, Some(&maxima()), vital::MANA, rec(5)),
            None
        );
    }
}
