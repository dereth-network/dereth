// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/Enchantment.cs
//! Port of `Source/ACE.Server/Network/Structure/Enchantment.cs`.
//!
//! The spell-based constructors read `ACE.Server.Entity.Spell` (the spell table and the world
//! database's spell rows), which is not ported: they go through
//! [`shims::spell_new`], a
//! `not_ported!` pointer.

use empyrean_entity::enums::{EnchantmentMask, EnchantmentTypeFlags, SpellId};
use empyrean_entity::models::PropertiesEnchantmentRegistry;
use empyrean_entity::ObjectGuid;

use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims::{self, SpellView};
use crate::World;

/// `EnchantmentManager.SpellCategory_Cooldown` (a `static ushort`, never reassigned).
pub const SPELL_CATEGORY_COOLDOWN: u16 = 0x8000;

// ACE: Enchantment
#[derive(Debug, Clone, PartialEq)]
pub struct Enchantment {
    // ACE: Enchantment.SpellID
    pub spell_id: u16,
    // ACE: Enchantment.Layer
    pub layer: u16,
    // ACE: Enchantment.HasSpellSetID
    /// default true?
    pub has_spell_set_id: u16,
    // ACE: Enchantment.SpellCategory
    pub spell_category: u16,
    // ACE: Enchantment.PowerLevel
    pub power_level: u32,
    // ACE: Enchantment.StartTime
    pub start_time: f64,
    // ACE: Enchantment.Duration
    pub duration: f64,
    // ACE: Enchantment.CasterGuid
    /// can be from items
    pub caster_guid: u32,
    // ACE: Enchantment.DegradeModifier
    pub degrade_modifier: f32,
    // ACE: Enchantment.DegradeLimit
    pub degrade_limit: f32,
    // ACE: Enchantment.LastTimeDegraded
    pub last_time_degraded: f64,
    // ACE: Enchantment.StatModType
    pub stat_mod_type: EnchantmentTypeFlags,
    // ACE: Enchantment.StatModKey
    pub stat_mod_key: u32,
    // ACE: Enchantment.StatModValue
    pub stat_mod_value: f32,
    // ACE: Enchantment.SpellSetID
    /// only sent if HasSpellSetID = true
    pub spell_set_id: u32,

    // not sent in network structure
    // ACE: Enchantment.Target
    pub target: ObjectGuid,
    // ACE: Enchantment.Spell
    pub spell: Option<SpellView>,
    // ACE: Enchantment.EnchantmentMask
    pub enchantment_mask: EnchantmentMask,
}

impl Default for Enchantment {
    /// The C# field initializers: `HasSpellSetID = 1`, everything else zero.
    fn default() -> Self {
        Enchantment {
            spell_id: 0,
            layer: 0,
            has_spell_set_id: 1,
            spell_category: 0,
            power_level: 0,
            start_time: 0.0,
            duration: 0.0,
            caster_guid: 0,
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: 0.0,
            stat_mod_type: EnchantmentTypeFlags::default(),
            stat_mod_key: 0,
            stat_mod_value: 0.0,
            spell_set_id: 0,
            target: ObjectGuid::default(),
            spell: None,
            enchantment_mask: EnchantmentMask::default(),
        }
    }
}

// ACE: Enchantment.Enchantment
/// `new Enchantment(WorldObject target, uint casterGuid, uint spellId, ushort layer,
/// EnchantmentMask enchantmentMask, float? statModVal = null)`.
pub fn enchantment_new(
    w: &World,
    target: ObjectGuid,
    caster_guid: u32,
    spell_id: u32,
    layer: u16,
    enchantment_mask: EnchantmentMask,
    stat_mod_val: Option<f32>,
) -> Enchantment {
    let mut e = Enchantment::default();
    // 2 references left, can this use BiotaPropertiesEnchantment?
    e.init(shims::spell_new(w, spell_id));

    e.layer = layer;
    e.caster_guid = caster_guid;
    e.stat_mod_value =
        stat_mod_val.unwrap_or_else(|| e.spell.as_ref().map_or(0.0, |s| s.stat_mod_val));

    e.target = target;
    e.enchantment_mask = enchantment_mask;
    e
}

// ACE: Enchantment.Enchantment
/// `new Enchantment(WorldObject target, SpellBase spellBase, ushort layer, EnchantmentMask
/// enchantmentMask, float? statModVal = null)`: the spell is the base's `MetaSpellId`.
pub fn enchantment_from_spell_base(
    w: &World,
    target: ObjectGuid,
    meta_spell_id: u32,
    layer: u16,
    enchantment_mask: EnchantmentMask,
    stat_mod_val: Option<f32>,
) -> Enchantment {
    let mut e = Enchantment::default();
    // should be able to replace this with cooldown constructor
    e.init(shims::spell_new(w, meta_spell_id));

    e.layer = layer;
    e.caster_guid = target.full();
    e.stat_mod_value = stat_mod_val.unwrap_or(35.0);

    e.target = target;
    e.enchantment_mask = enchantment_mask;
    e
}

// ACE: Enchantment.Enchantment
/// `new Enchantment(WorldObject target, PropertiesEnchantmentRegistry entry)`.
#[allow(clippy::cast_possible_truncation)] // C# `(ushort)`/`(uint)` narrowing
pub fn enchantment_from_registry(
    w: &World,
    target: ObjectGuid,
    entry: &PropertiesEnchantmentRegistry,
) -> Enchantment {
    let mut e = Enchantment::default();
    if entry.spell_category.0 == u32::from(SPELL_CATEGORY_COOLDOWN) {
        e.init_cooldown(target, entry);
        return e;
    }

    e.init(shims::spell_new(w, entry.spell_id.cast_unsigned()));

    e.layer = entry.layer_id;
    e.start_time = entry.start_time;
    e.duration = entry.duration; // item spells can have -1, overriding the spell duration
    e.caster_guid = entry.caster_object_id;
    e.stat_mod_value = entry.stat_mod_value;
    e.spell_set_id = entry.spell_set_id.0.cast_unsigned();

    e.target = target;
    e.enchantment_mask = EnchantmentMask(entry.enchantment_category.cast_signed());
    e
}

impl Enchantment {
    // ACE: Enchantment.Init
    #[allow(clippy::cast_possible_truncation)] // C# `(ushort)` narrowing
    pub fn init(&mut self, spell: SpellView) {
        self.spell_id = spell.id as u16;
        self.spell_category = spell.category.0 as u16;
        self.power_level = spell.power;
        self.duration = spell.duration;
        self.degrade_modifier = spell.degrade_modifier;
        self.degrade_limit = spell.degrade_limit;

        if spell.has_spell_base {
            self.stat_mod_type = spell.stat_mod_type;
            self.stat_mod_key = spell.stat_mod_key;

            if spell.is_beneficial {
                // should "server" data be fixed or is this the better way to do this?
                self.stat_mod_type |= EnchantmentTypeFlags::Beneficial;
            }
        }
        self.spell = Some(spell);
    }

    // ACE: Enchantment.InitCooldown
    #[allow(clippy::cast_possible_truncation)] // C# `(ushort)` narrowing
    pub fn init_cooldown(&mut self, _target: ObjectGuid, entry: &PropertiesEnchantmentRegistry) {
        self.spell_id = entry.spell_id as u16;
        self.layer = entry.layer_id;
        self.spell_category = entry.spell_category.0 as u16;
        self.start_time = entry.start_time;
        self.duration = entry.duration;
        self.caster_guid = entry.caster_object_id;
        self.degrade_modifier = entry.degrade_modifier;
        self.degrade_limit = entry.degrade_limit;
        self.last_time_degraded = entry.last_time_degraded;
        self.stat_mod_type = entry.stat_mod_type;
        self.stat_mod_key = entry.stat_mod_key;
        self.stat_mod_value = entry.stat_mod_value;
    }

    // ACE: Enchantment.GetInfo
    /// A debug dump; the spell and target names come from their owners (pointers).
    #[must_use]
    pub fn get_info(&self, w: &World) -> String {
        let spell = shims::spell_new(w, u32::from(self.spell_id));
        let target_name = shims::name(w, self.target).unwrap_or_default();

        let mut info = format!("Spell: {} ({})\n", spell.name, self.spell_id);
        info += &format!("Target: {target_name}\n");
        info += &format!("Layer: {}\n", self.layer);
        info += &format!(
            "SpellCategory: {}\n",
            empyrean_entity::enums::SpellCategory(u32::from(self.spell_category))
        );
        info += &format!("Power: {}\n", self.power_level);
        info += &format!(
            "StartTime: {}\n",
            empyrean_common::dotnet::to_string(self.start_time)
        );
        info += &format!(
            "Duration: {}\n",
            empyrean_common::dotnet::to_string(self.duration)
        );
        info += &format!("CasterGuid: {:08X}\n", self.caster_guid);
        info += &format!("StatModType: {}\n", self.stat_mod_type);
        info += &format!("StatModKey: {}\n", self.stat_mod_key);
        info += &format!(
            "StatModValue: {}\n",
            empyrean_common::dotnet::to_string(self.stat_mod_value)
        );
        info += "---------";

        info
    }
}

// ACE: EnchantmentExtensions.Write
/// `writer.Write(List<Enchantment> enchantments)`.
pub fn write_list(writer: &mut Vec<u8>, enchantments: &[Enchantment]) {
    let list: Vec<_> = enchantments.iter().map(record).collect();
    write_record(writer, &[], |w| w.packed_list(&list, |w, e| e.write(w)));
}

// ACE: EnchantmentExtensions.Write
/// `writer.Write(Enchantment enchantment)`.
pub fn write(writer: &mut Vec<u8>, enchantment: &Enchantment) {
    write_record(writer, &[], |w| record(enchantment).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(enchantment: &Enchantment) -> dereth_protocol::types::Enchantment {
    // this line is to force vitae to be layer 0 to match retail pcaps. We save it as layer 1 to make EF Core happy.
    #[allow(clippy::cast_possible_truncation)] // `(ushort)SpellId.Vitae`
    let layer = if enchantment.spell_id == SpellId::Vitae.0 as u16 {
        0
    } else {
        enchantment.layer
    };
    dereth_protocol::types::Enchantment {
        id: u32::from(enchantment.spell_id) | (u32::from(layer) << 16),
        category_word: u32::from(enchantment.spell_category)
            | (u32::from(enchantment.has_spell_set_id) << 16),
        power_level: enchantment.power_level.cast_signed(),
        start_time: enchantment.start_time,
        duration: enchantment.duration,
        caster: dereth_primitives::ObjectId(enchantment.caster_guid),
        degrade_modifier: enchantment.degrade_modifier,
        degrade_limit: enchantment.degrade_limit,
        last_time_degraded: enchantment.last_time_degraded, // always 0 / spell economy?
        smod: dereth_protocol::types::StatMod {
            kind: enchantment.stat_mod_type.0.cast_unsigned(),
            key: enchantment.stat_mod_key,
            value: enchantment.stat_mod_value,
        },
        spell_set_id: (enchantment.has_spell_set_id != 0).then_some(enchantment.spell_set_id),
    }
}
