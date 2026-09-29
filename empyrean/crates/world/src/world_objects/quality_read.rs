//! A world object as the shared quality inquiries read it (`dereth_rules::quality::QualityRead`), so
//! the client's retail formulas (`inq_attribute`, `inq_attribute_2nd`, `inq_skill`, `inq_load`, the
//! raise costs) can be asked of the server's own objects.
//!
//! Not an ACE port: ACE has no such seam, and nothing in the port calls these inquiries; the server
//! computes its stats with ACE's own formulas (`CreatureAttribute`, `CreatureVital`,
//! `CreatureSkill`). This is what the comparisons of ACE's stats with the client's run on, and where the server
//! would call a shared retail rule if one should replace ACE's.
//!
//! What it reads, all stored values: int and float properties through
//! [`WorldObject::get_property`] (so an ephemeral override applies, as it would to anything the
//! client is sent), the biota's attribute, vital and skill records, and its enchantment registry
//! rows, sorted into the four lists as ACE's registry sorts them
//! (`empyrean_entity::adapter::quality_bridge`).

use std::borrow::Cow;

use dereth_primitives::LocalTime;
use dereth_protocol::types::qualities::{
    Attribute, EnchantmentRegistry as WireRegistry, SecondaryAttribute, Skill as WireSkill,
};
use dereth_rules::enchant::EnchantmentRegistry;
use dereth_rules::quality::QualityRead;
use empyrean_entity::adapter::quality_bridge::{
    enchantment_row_to_wire, ENCHANTMENT_MASK_ADDITIVE, ENCHANTMENT_MASK_COOLDOWN,
    ENCHANTMENT_MASK_MULTIPLICATIVE,
};
use empyrean_entity::enums::{
    PropertyAttribute, PropertyAttribute2nd, PropertyFloat, PropertyInt, Skill,
};

use crate::world_objects::world_object::WorldObject;

impl QualityRead for WorldObject {
    fn int(&self, id: u32) -> Option<i32> {
        self.get_property(PropertyInt(u16::try_from(id).ok()?))
    }

    fn float(&self, id: u32) -> Option<f64> {
        self.get_property(PropertyFloat(u16::try_from(id).ok()?))
    }

    fn attribute(&self, id: u32) -> Option<Attribute> {
        let r = self
            .biota
            .properties_attribute
            .as_ref()?
            .get(&PropertyAttribute(u16::try_from(id).ok()?))?;
        Some(Attribute {
            level_from_cp: r.level_from_cp,
            init_level: r.init_level,
            cp_spent: r.cp_spent,
        })
    }

    fn attribute_2nd(&self, id: u32) -> Option<SecondaryAttribute> {
        // the maximum's id names the record; the current value's id is the one after it
        let max = match id {
            1 | 2 => 1,
            3 | 4 => 3,
            5 | 6 => 5,
            _ => return None,
        };
        let r = self
            .biota
            .properties_attribute_2nd
            .as_ref()?
            .get(&PropertyAttribute2nd(max))?;
        Some(SecondaryAttribute {
            attribute: Attribute {
                level_from_cp: r.level_from_cp,
                init_level: r.init_level,
                cp_spent: r.cp_spent,
            },
            current_level: r.current_level,
        })
    }

    fn skill(&self, id: u32) -> Option<WireSkill> {
        let s = self
            .biota
            .properties_skill
            .as_ref()?
            .get(&Skill(i32::try_from(id).ok()?))?;
        Some(WireSkill {
            level_from_pp: s.level_from_pp,
            format_version: 1,
            sac: s.sac.0,
            pp: s.pp,
            init_level: s.init_level,
            resistance_of_last_check: s.resistance_at_last_check.cast_signed(),
            last_used_time: s.last_used_time,
        })
    }

    fn enchantments(&self) -> Cow<'_, EnchantmentRegistry> {
        let Some(rows) = self.biota.properties_enchantment_registry.as_ref() else {
            return Cow::Owned(EnchantmentRegistry::default());
        };
        let mut wire = WireRegistry::default();
        for row in rows {
            let (mask, e) = enchantment_row_to_wire(row);
            let list = match mask {
                ENCHANTMENT_MASK_MULTIPLICATIVE => &mut wire.multiplicative,
                ENCHANTMENT_MASK_ADDITIVE => &mut wire.additive,
                ENCHANTMENT_MASK_COOLDOWN => &mut wire.cooldowns,
                // one vitae: the first, as ACE's registry sends it
                _ => {
                    if wire.vitae.is_none() {
                        wire.vitae = Some(e);
                    }
                    continue;
                }
            };
            list.get_or_insert_with(Vec::new).push(e);
        }
        // received at time zero: the rows keep their own times (no inquiry reads them)
        Cow::Owned(EnchantmentRegistry::from_wire(&wire, LocalTime(0.0)))
    }
}
