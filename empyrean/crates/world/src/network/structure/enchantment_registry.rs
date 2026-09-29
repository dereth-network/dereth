// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/EnchantmentRegistry.cs
//! Port of `Source/ACE.Server/Network/Structure/EnchantmentRegistry.cs`.

use empyrean_common::extensions::float_extensions::epsilon_equals;
use empyrean_entity::enums::{EnchantmentMask, EnchantmentTypeFlags, SpellId};
use empyrean_entity::models::PropertiesEnchantmentRegistry;
use empyrean_entity::ObjectGuid;

use super::enchantment::{self, enchantment_from_registry, Enchantment, SPELL_CATEGORY_COOLDOWN};
use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims;
use crate::World;

/// ACE's `Dictionary<EnchantmentMask, List<Enchantment>>`, which always holds exactly these four
/// keys, added in this order (so its enumeration order is this field order).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnchantmentCategories {
    pub multiplicative: Vec<Enchantment>,
    pub additive: Vec<Enchantment>,
    pub vitae: Vec<Enchantment>,
    pub cooldown: Vec<Enchantment>,
}

impl EnchantmentCategories {
    /// `(key, list)` in the dictionary's enumeration order.
    fn entries(&self) -> [(EnchantmentMask, &Vec<Enchantment>); 4] {
        [
            (EnchantmentMask::Multiplicative, &self.multiplicative),
            (EnchantmentMask::Additive, &self.additive),
            (EnchantmentMask::Vitae, &self.vitae),
            (EnchantmentMask::Cooldown, &self.cooldown),
        ]
    }
}

// ACE: EnchantmentRegistry
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnchantmentRegistry {
    // ACE: EnchantmentRegistry.EnchantmentMask
    pub enchantment_mask: EnchantmentMask,
    // ACE: EnchantmentRegistry.Enchantments
    pub enchantments: EnchantmentCategories,
}

// ACE: EnchantmentRegistry.EnchantmentRegistry
/// `new EnchantmentRegistry(Player player)`: the player's registry by category. A vitae at or
/// above 1.0 is dispelled from the player (`EnchantmentManager.Dispel`) and not sent.
pub fn enchantment_registry_new(w: &mut World, player: ObjectGuid) -> EnchantmentRegistry {
    let enchantments: Vec<PropertiesEnchantmentRegistry> = w
        .objects
        .get(player)
        .expect("ACE: player is null")
        .biota
        .properties_enchantment_registry
        .clone()
        .unwrap_or_default();

    let mut r = EnchantmentRegistry {
        enchantment_mask: EnchantmentMask::default(),
        enchantments: build_categories(w, player, &enchantments),
    };

    let vitae = r.enchantments.vitae.first();

    if let Some(vitae) = vitae {
        if epsilon_equals(vitae.stat_mod_value, 1.0) || vitae.stat_mod_value > 1.0 {
            let entry = enchantments
                .iter()
                .find(|e| e.spell_id.cast_unsigned() == SpellId::Vitae.0)
                .cloned();
            shims::enchantment_manager_dispel(w, player, entry);

            r.enchantments.vitae.clear();
        }
    }
    r.set_enchant_mask();
    r
}

// ACE: EnchantmentRegistry.BuildCategories
fn build_categories(
    w: &World,
    player: ObjectGuid,
    registry: &[PropertiesEnchantmentRegistry],
) -> EnchantmentCategories {
    let mut categories = EnchantmentCategories::default();

    for entry in registry {
        let enchantment = enchantment_from_registry(w, player, entry);

        if u32::from(enchantment.spell_id) == SpellId::Vitae.0 {
            categories.vitae.push(enchantment);
        } else if enchantment.spell_id > SPELL_CATEGORY_COOLDOWN {
            categories.cooldown.push(enchantment);
        } else if (enchantment.stat_mod_type & EnchantmentTypeFlags::Multiplicative).0 != 0 {
            categories.multiplicative.push(enchantment);
        } else {
            if (enchantment.stat_mod_type & EnchantmentTypeFlags::Additive).0 == 0 {
                log::info!(
                    "EnchantmentRegistry.BuildCategories(): unknown enchantment {} StatModType {}",
                    enchantment.spell_id,
                    enchantment.stat_mod_type
                );
            }

            categories.additive.push(enchantment);
        }
    }
    categories
}

impl EnchantmentRegistry {
    // ACE: EnchantmentRegistry.SetEnchantMask
    fn set_enchant_mask(&mut self) {
        self.enchantment_mask = EnchantmentMask(0);
        let mut mask = EnchantmentMask(0);
        for (key, value) in self.enchantments.entries() {
            if !value.is_empty() {
                mask |= key;
            }
        }
        self.enchantment_mask = mask;
    }
}

// ACE: EnchantmentRegistryExtensions.Write
/// `writer.Write(EnchantmentRegistry registry)`: the mask, then each non-empty list (the vitae as
/// a single enchantment).
pub fn write(writer: &mut Vec<u8>, registry: &EnchantmentRegistry) {
    write_record(writer, &[], |w| record(registry).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field. Each list the mask names, the vitae as a single enchantment.
#[must_use]
pub fn record(registry: &EnchantmentRegistry) -> dereth_protocol::types::EnchantmentRegistry {
    let enchantment_mask = registry.enchantment_mask;
    let list = |bit: EnchantmentMask, list: &[Enchantment]| {
        enchantment_mask
            .contains(bit)
            .then(|| list.iter().map(enchantment::record).collect())
    };
    dereth_protocol::types::EnchantmentRegistry {
        flags: enchantment_mask.0.cast_unsigned(),
        multiplicative: list(
            EnchantmentMask::Multiplicative,
            &registry.enchantments.multiplicative,
        ),
        additive: list(EnchantmentMask::Additive, &registry.enchantments.additive),
        cooldowns: list(EnchantmentMask::Cooldown, &registry.enchantments.cooldown),
        // `FirstOrDefault()`: the mask bit is only set for a non-empty list.
        vitae: enchantment_mask.contains(EnchantmentMask::Vitae).then(|| {
            enchantment::record(
                registry
                    .enchantments
                    .vitae
                    .first()
                    .expect("ACE: null vitae (NullReferenceException)"),
            )
        }),
    }
}
