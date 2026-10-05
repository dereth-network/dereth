//! A character's load is read from its enchanted strength, not only its allocated ranks.
//! Fixture: none; a synthetic qualities record with one additive strength enchantment.

use dereth_client_model::qualities::Qualities;
use dereth_primitives::ObjectId;

/// Behaviour: inventory.burden.load-uses-enchanted-strength
#[test]
fn load_inquiry_uses_active_enchanted_strength_not_only_allocated_ranks() {
    use dereth_protocol::types::qualities::{Attribute, AttributeCache, StatMod};
    use {dereth_rules::enchant::ench_type, dereth_rules::enchant::Enchantment};
    let mut q = Qualities::new();
    q.attributes = Some(AttributeCache {
        strength: Some(Attribute {
            init_level: 100,
            ..Default::default()
        }),
        ..Default::default()
    });
    q.ints = Some([(5, 30_000)].into_iter().collect());
    assert_eq!(dereth_rules::burden::inq_load(&q), 2.0);
    assert!(q.enchantments.update_enchantment(Enchantment {
        id: 1,
        spell_category: 1,
        power_level: 100,
        start_time: 0.0,
        duration: 60.0,
        caster: ObjectId(1),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        spell_set_id: None,
        smod: StatMod {
            kind: ench_type::ATTRIBUTE | ench_type::SINGLE_STAT | ench_type::ADDITIVE,
            key: 1,
            value: 100.0
        },
    }));
    assert_eq!(
        dereth_rules::attributes::inq_attribute(&q, 1, false),
        Some(200)
    );
    assert_eq!(
        dereth_rules::burden::inq_load(&q),
        1.0,
        "attribute inquiry(Strength, raw=false):30000/(200*150), not raw100 capacity"
    );
}
