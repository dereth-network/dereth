//! The source mutation pipeline, without ACE's later mutation scripts.
// Rules ported from ac1999 (Adam Plumb), AGPL-3.0: Server/Source/TreasureFactory.cpp.

use empyrean_common::dotnet::CsCast;

use super::{
    float, int,
    rng::{self, Favor, LootRng},
};
use crate::{world_objects::world_object::WorldObject, World};
use empyrean_content::loot::{self, Band, Category, Entry, SourceLootPolicy, Tier};
use empyrean_entity::enums::{
    DamageType, EquipMask, ItemType, PropertyDataId as D, PropertyFloat as F, PropertyInt as I,
    PropertyString as S, UiEffects,
};

#[allow(
    clippy::too_many_arguments,
    reason = "The pipeline passes the selected tier, category and entry without cloning them."
)]
pub(super) fn mutate(
    w: &World,
    o: &mut WorldObject,
    p: &SourceLootPolicy,
    t: &Tier,
    c: &Category,
    e: &Entry,
    q: f64,
    r: &mut impl LootRng,
) {
    let ty = ItemType(int(o, I::ItemType, 0) as u32);
    let base_al = int(o, I::ArmorLevel, 0);
    let bands = if ty == ItemType::Armor && c.mutation_bands.is_empty() {
        p.mutations["armor"].as_slice()
    } else {
        &c.mutation_bands
    };
    if !bands.is_empty() {
        let band = &bands[(t.tier_id as usize).min(bands.len()) - 1];
        if ty == ItemType::MeleeWeapon || ty == ItemType::MissileWeapon {
            weapon(w, o, band, e, q, r);
        } else if ty == ItemType::Armor {
            armor(o, band, q, r);
        }
    }
    material(o, p, t, c, e, r);
    if let Some(book) = &mut o.biota.properties_spell_book {
        book.clear();
    }
    let levels = super::spells::assign(w, o, p, t, c, e, q, r);
    let base_value = f64::from(int(o, I::Value, 0));
    let value = if ty == ItemType::Gem {
        base_value
            * rng::real(
                r,
                t.gem_value_factor_min,
                t.gem_value_factor_max,
                Some(Favor::Mid),
                2.0,
                q,
            )
            * (1.0 + 0.1 * f64::from(levels))
    } else if ty == ItemType::Armor {
        let ratio = if base_al > 0 {
            f64::from(int(o, I::ArmorLevel, 0)) / f64::from(base_al)
        } else {
            1.0
        };
        base_value * 1.37 * dereth_primitives::num::math::pow(ratio, 2.2)
            + 200.0 * f64::from(levels)
    } else {
        let mat = int(o, I::MaterialType, 0) as u32;
        let factor = p
            .material_values
            .iter()
            .find(|(name, _)| loot::material_id(name) == Some(mat))
            .map_or(1.0, |(_, v)| *v);
        base_value
            * rng::real(
                r,
                t.value_factor_min,
                t.value_factor_max,
                Some(Favor::Mid),
                2.0,
                q,
            )
            * factor
            + 200.0 * f64::from(levels)
    };
    o.set_property(
        I::Value,
        rng::real(r, value * 0.9, value * 1.1, None, 0.0, 0.0)
            .round()
            .cs_cast(),
    );
    o.set_property(
        I::ResistItemAppraisal,
        [0, 25, 75, 125, 175, 225][t.tier_id as usize],
    );
    if e.stack_size > 1 {
        let stack = (e.stack_size as i32).min(int(o, I::MaxStackSize, 1));
        if stack > 1 {
            // Empyrean's split, resize and reload paths price from the unit property.
            // Keep the generated unit price so those operations preserve this stack's value.
            o.set_property(I::StackUnitValue, int(o, I::Value, 0));
            o.set_property(I::StackSize, stack);
            o.set_property(
                I::EncumbranceVal,
                stack * int(o, I::StackUnitEncumbrance, 0),
            );
            o.set_property(I::Value, stack * int(o, I::Value, 0));
            o.set_property(I::MaxStackSize, stack);
        }
    }
}

fn modifier(
    o: &mut WorldObject,
    property: F,
    lo: f64,
    hi: f64,
    chance: f64,
    q: f64,
    r: &mut impl LootRng,
) {
    if hi <= 0.0 || !rng::chance(r, chance, q) {
        return;
    }
    let v = rng::modifier_round(rng::real(
        r,
        1.0 + lo / 100.0,
        1.0 + hi / 100.0,
        Some(Favor::Mid),
        2.0,
        q,
    ));
    if v - 1.0 >= 0.005 - 1e-9 {
        o.set_property(property, v);
    }
}
fn weapon(w: &World, o: &mut WorldObject, b: &Band, e: &Entry, q: f64, r: &mut impl LootRng) {
    if let Some(skill) = e
        .weapon_skill
        .as_deref()
        .and_then(loot::skill_id)
        .filter(|v| *v != 0)
    {
        o.set_property(I::WeaponSkill, skill);
    }
    modifier(
        o,
        F::WeaponDefense,
        b.min_melee_defense_bonus,
        b.max_melee_defense_bonus,
        b.melee_defense_bonus_chance,
        q,
        r,
    );
    elemental(w, o, b, e, r);
    let direct = ItemType(int(o, I::ItemType, 0) as u32) == ItemType::MeleeWeapon
        || int(o, I::DefaultCombatStyle, 0) == 0x80;
    if direct {
        o.set_property(
            I::Damage,
            int(o, I::Damage, 0)
                + rng::favored(
                    r,
                    b.min_damage_bonus,
                    b.max_damage_bonus,
                    Favor::Mid,
                    2.0,
                    q,
                ),
        );
    } else {
        let bonus = rng::real(
            r,
            b.min_damage_mod_bonus,
            b.max_damage_mod_bonus,
            Some(Favor::Mid),
            2.0,
            q,
        );
        o.set_property(
            F::DamageMod,
            rng::modifier_round(float(o, F::DamageMod, 1.0) * (1.0 + bonus)),
        );
    }
    if b.fast_speed_mod > 0.0 && b.slow_speed_mod > 0.0 {
        let speed = rng::real(
            r,
            b.fast_speed_mod,
            b.slow_speed_mod,
            Some(Favor::High),
            2.0,
            q,
        );
        o.set_property(
            I::WeaponTime,
            (f64::from(int(o, I::WeaponTime, 50)) * speed)
                .round()
                .cs_cast(),
        );
    }
    if direct {
        modifier(
            o,
            F::WeaponOffense,
            b.min_attack_skill_bonus,
            b.max_attack_skill_bonus,
            b.attack_skill_bonus_chance,
            q,
            r,
        );
    }
}
fn elemental(w: &World, o: &mut WorldObject, b: &Band, e: &Entry, r: &mut impl LootRng) {
    if e.elemental_variants.is_empty() || !rng::chance(r, b.elemental_chance, 0.0) {
        return;
    }
    let id = e.elemental_variants[rng::index(r, e.elemental_variants.len())];
    let Some(template) = w.content.get_cached_weenie(id) else {
        return;
    };
    if let Some(setup) = template
        .properties_did
        .as_ref()
        .and_then(|p| p.get(&D::Setup))
        .copied()
    {
        o.set_property(D::Setup, setup);
    }
    if id == 3750 {
        o.set_property(D::Setup, 33555690u32);
    }
    let damage = template
        .properties_int
        .as_ref()
        .and_then(|p| p.get(&I::DamageType))
        .copied()
        .unwrap_or(0);
    let (prefix, effect) = match DamageType(damage) {
        DamageType::Acid => ("Acid", UiEffects::Acid),
        DamageType::Cold => ("Frost", UiEffects::Frost),
        DamageType::Fire => ("Flaming", UiEffects::Fire),
        DamageType::Electric => ("Lightning", UiEffects::Lightning),
        _ => return,
    };
    o.set_property(I::DamageType, damage);
    o.set_property(I::UiEffects, effect.0 as i32);
    let name: String = o.get_property(S::Name).unwrap_or_default();
    o.set_property(S::Name, format!("{prefix} {name}"));
}
fn armor(o: &mut WorldObject, b: &Band, q: f64, r: &mut impl LootRng) {
    let burden = int(o, I::EncumbranceVal, 0);
    if burden != 0 {
        o.set_property(
            I::EncumbranceVal,
            (f64::from(burden)
                * rng::real(
                    r,
                    b.min_burden_mod,
                    b.max_burden_mod,
                    Some(Favor::Mid),
                    1.4,
                    -q,
                ))
            .round()
            .cs_cast(),
        );
    }
    let props = [
        F::ArmorModVsAcid,
        F::ArmorModVsElectric,
        F::ArmorModVsFire,
        F::ArmorModVsCold,
        F::ArmorModVsSlash,
        F::ArmorModVsPierce,
        F::ArmorModVsBludgeon,
    ];
    let candidates: Vec<_> = props
        .iter()
        .map(|p| {
            // Source integer division produces zero; retaining the draws also preserves ordering.
            let increase = rng::favored(r, 1, 2, Favor::Low, 1.4, q) / 10;
            (float(o, *p, 0.0) + f64::from(increase)).min(2.0)
        })
        .collect();
    for (property, value) in props.into_iter().zip(candidates) {
        if rng::chance(r, 0.25, q) {
            o.set_property(property, value);
        }
    }
    let shield = (int(o, I::ValidLocations, 0) as u32) & EquipMask::Shield.0 != 0;
    let (lo, hi) = if shield {
        (b.min_shield_armor_mult, b.max_shield_armor_mult)
    } else {
        (b.min_armor_mult, b.max_armor_mult)
    };
    let mult = rng::real(r, lo, hi, Some(Favor::Mid), 1.4, q);
    o.set_property(
        I::ArmorLevel,
        (f64::from(int(o, I::ArmorLevel, 0)) * mult)
            .round()
            .cs_cast(),
    );
}
fn material(
    o: &mut WorldObject,
    p: &SourceLootPolicy,
    t: &Tier,
    c: &Category,
    e: &Entry,
    r: &mut impl LootRng,
) {
    let mutation = int(o, I::TsysMutationData, 0);
    if mutation == 0 || ItemType(int(o, I::ItemType, 0) as u32) == ItemType::Gem {
        return;
    }
    let buckets = if e.possible_materials.is_empty() {
        &c.possible_materials
    } else {
        &e.possible_materials
    };
    let materials: Vec<_> = buckets
        .iter()
        .flat_map(|b| t.materials(b))
        .filter_map(|m| loot::material_id(m))
        .collect();
    if materials.is_empty() {
        return;
    }
    let material = materials[rng::index(r, materials.len())];
    o.set_property(I::MaterialType, material as i32);
    let code = ((mutation as u32) >> 16) & 255;
    let entries = p
        .material_colors
        .iter()
        .find(|m| m.key == material)
        .and_then(|m| m.value.as_ref())
        .and_then(|v| v.iter().find(|c| c.key == code))
        .and_then(|c| c.value.as_ref());
    let palette = if let Some(entries) = entries {
        let mut draw: f32 = rng::real(r, 0.0, 1.0, None, 0.0, 0.0).cs_cast();
        let mut picked = 0;
        for e in entries {
            let chance: f32 = e.chance.cs_cast();
            if draw <= chance {
                picked = e.value;
                break;
            }
            draw -= chance;
        }
        picked
    } else if (4..=8).contains(&material) {
        r.integer(1, 18) as u32
    } else {
        0
    };
    if palette > 0 {
        o.set_property(I::PaletteTemplate, palette as i32);
        o.set_property(F::Shade, rng::real(r, 0.0, 1.0, None, 0.0, 0.0));
    }
}
