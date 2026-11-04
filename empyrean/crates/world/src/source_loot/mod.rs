//! Source-pinned six-group loot. This reconstruction is not a claim about launch probabilities.
// Rules ported from ac1999 (Adam Plumb), AGPL-3.0: Server/Source/TreasureFactory.cpp.

mod mutation;
pub mod rng;
mod spells;

use crate::factories::world_object_factory;
use crate::world_objects::world_object::WorldObject;
use crate::World;
use empyrean_content::loot::{Category, Entry, GroupedTreasureDeath, SourceLootPolicy, Tier};
use empyrean_entity::enums::{ItemType, PropertyInt};
use rng::{Favor, LootRng};

const KINDS: [&[&str]; 6] = [
    &["melee-weapons", "missile-weapons"],
    &["armor", "clothing"],
    &["jewelry", "gems"],
    &["casters", "scrolls", "manaStones"],
    &["healingKits", "potions", "food"],
    &["coins", "lockpicks", "parchment", "components"],
];

/// Verify the local halves of the source policy's template and spell dependencies at startup.
/// Mana and school come from these installed assets; no retail table is bundled with the policy.
pub fn validate_local_inputs(w: &World, p: &SourceLootPolicy) -> Result<(), String> {
    p.validate()?;
    for cats in p.pools.values() {
        for c in cats {
            for e in &c.entries {
                for id in std::iter::once(&e.wcid).chain(&e.elemental_variants) {
                    if w.content.get_cached_weenie(*id).is_none() {
                        return Err(format!("source loot template {id} is missing"));
                    }
                }
            }
        }
    }
    for pool in p.spells.values() {
        for family in pool {
            for id in &family.spells {
                if *id != 0 && crate::entity::spell::Spell::new(w, *id, true).not_found() {
                    return Err(format!(
                        "source loot spell {id} is missing from local content/assets"
                    ));
                }
            }
        }
    }
    Ok(())
}
fn misc(kind: &str) -> bool {
    !empyrean_content::loot::POOLS.contains(&kind)
}
fn total(categories: &[Category]) -> i32 {
    categories.iter().map(|c| c.weight.max(0)).sum()
}

/// Returns uninserted objects, as the ACE loot factory does.
pub fn generate(
    w: &mut World,
    row: &GroupedTreasureDeath,
    p: &SourceLootPolicy,
) -> Vec<WorldObject> {
    generate_with(w, row, p, &mut rng::ThreadRng)
}
pub fn generate_with(
    w: &mut World,
    row: &GroupedTreasureDeath,
    p: &SourceLootPolicy,
    r: &mut impl LootRng,
) -> Vec<WorldObject> {
    let Some(tier) = p.tiers.get(&row.tier) else {
        log::error!("missing source loot tier {}", row.tier);
        return Vec::new();
    };
    let mut result = Vec::new();
    for (group, kinds) in row.groups.iter().zip(KINDS) {
        let picks: Vec<_> = kinds
            .iter()
            .copied()
            .filter(|k| {
                !misc(k)
                    || p.pools["misc"]
                        .iter()
                        .filter(|c| c.category == *k)
                        .flat_map(|c| &c.entries)
                        .any(|e| e.base_tier <= row.tier)
            })
            .collect();
        if group.max == 0 || group.chance == 0 || picks.is_empty() {
            continue;
        }
        let count = r.integer(group.min.min(group.max) as i32, group.max as i32);
        for _ in 0..count {
            if r.integer(0, 99) >= group.chance as i32 {
                continue;
            }
            let weights: Vec<_> = picks
                .iter()
                .map(|k| {
                    if matches!(*k, "melee-weapons" | "missile-weapons") {
                        total(&p.pools[*k])
                    } else {
                        1
                    }
                })
                .collect();
            let kind = picks[rng::weighted(r, &weights)];
            if let Some(item) = generate_kind(w, p, tier, kind, row.quality * 0.25, r) {
                result.push(item);
            }
        }
    }
    result
}

/// Nearest populated base tier, with lower tiers tried before equally distant higher tiers.
fn rung_order(weights: &[i32], r: &mut impl LootRng) -> Vec<u32> {
    if weights.iter().sum::<i32>() <= 0 {
        return Vec::new();
    }
    let wanted = i32::try_from(rng::weighted(r, weights)).expect("validated tier count") + 1;
    let mut result = vec![wanted as u32];
    for d in 1..i32::try_from(weights.len()).expect("validated tier count") {
        for v in [wanted - d, wanted + d] {
            if v >= 1 && v <= i32::try_from(weights.len()).expect("validated tier count") {
                result.push(v as u32);
            }
        }
    }
    result
}
fn narrow<'a>(
    candidates: Vec<&'a Entry>,
    weights: &[i32],
    tier: u32,
    r: &mut impl LootRng,
) -> Vec<&'a Entry> {
    let order = rung_order(weights, r);
    if order.is_empty() {
        return candidates;
    }
    for rung in order.into_iter().filter(|v| *v <= tier) {
        let entries: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|e| e.base_tier == 0 || e.base_tier == rung)
            .collect();
        if !entries.is_empty() {
            return entries;
        }
    }
    Vec::new()
}

pub fn generate_kind(
    w: &mut World,
    p: &SourceLootPolicy,
    t: &Tier,
    kind: &str,
    q: f64,
    r: &mut impl LootRng,
) -> Option<WorldObject> {
    if misc(kind) {
        let entries = p.pools["misc"]
            .iter()
            .filter(|c| c.category == kind)
            .flat_map(|c| &c.entries)
            .collect();
        let entries = narrow(entries, &t.mundane_base_tier_weights, t.tier_id, r);
        if entries.is_empty() {
            return None;
        }
        let entry = entries[rng::index(r, entries.len())];
        let mut item =
            world_object_factory::create_new_world_object_by_wcid_in_world(w, entry.wcid)?;
        if entry.max_amount > 1 {
            let amount = rng::favored(r, 1, entry.max_amount as i32, Favor::Mid, 2.0, q).min(int(
                &item,
                PropertyInt::MaxStackSize,
                1,
            ));
            item.set_stack_size(Some(amount));
        }
        return Some(item);
    }
    let cats = p.pools.get(kind)?;
    if kind == "scrolls" || kind == "components" {
        let names = if kind == "scrolls" {
            &t.scroll_categories
        } else {
            &t.component_categories
        };
        let name = &names[rng::index(r, names.len())];
        let c = cats.iter().find(|c| c.category == *name)?;
        return world_object_factory::create_new_world_object_by_wcid_in_world(
            w,
            c.entries[rng::index(r, c.entries.len())].wcid,
        );
    }
    let c = if kind == "armor" {
        let order = rung_order(&t.armor_base_tier_weights, r);
        if order.is_empty() {
            &cats[rng::index(r, cats.len())]
        } else {
            let mut chosen = None;
            for rung in order.into_iter().filter(|v| *v <= t.tier_id) {
                let options: Vec<_> = cats
                    .iter()
                    .filter(|c| c.base_tier == rung && !c.entries.is_empty())
                    .collect();
                if !options.is_empty() {
                    chosen = Some(options[rng::index(r, options.len())]);
                    break;
                }
            }
            chosen?
        }
    } else if matches!(kind, "melee-weapons" | "missile-weapons") {
        &cats[rng::weighted(r, &cats.iter().map(|c| c.weight).collect::<Vec<_>>())]
    } else if kind == "gems" {
        &cats[0]
    } else {
        &cats[rng::index(r, cats.len())]
    };
    let mut entries: Vec<_> = c.entries.iter().collect();
    if kind == "gems" {
        let allowed: Vec<_> = entries
            .iter()
            .copied()
            .filter(|e| {
                t.materials_gem
                    .iter()
                    .any(|m| empyrean_content::loot::material_id(m) == Some(e.material))
            })
            .collect();
        if !allowed.is_empty() {
            entries = allowed;
        } else {
            log::warn!("source gem pool has no tier material match");
        }
    } else if matches!(kind, "melee-weapons" | "missile-weapons") {
        entries = narrow(
            entries,
            if kind == "melee-weapons" {
                &t.melee_weapon_base_tier_weights
            } else {
                &t.missile_weapon_base_tier_weights
            },
            t.tier_id,
            r,
        );
    }
    if entries.is_empty() {
        return None;
    }
    for _ in 0..10 {
        let weights: Vec<_> = entries
            .iter()
            .map(|e| {
                if e.tier_weights.is_empty() {
                    1
                } else {
                    e.tier_weights[(t.tier_id as usize).min(e.tier_weights.len()) - 1]
                }
            })
            .collect();
        let e = entries[rng::weighted(r, &weights)];
        let Some(mut item) =
            world_object_factory::create_new_world_object_by_wcid_in_world(w, e.wcid)
        else {
            continue;
        };
        let ty = ItemType(int(&item, PropertyInt::ItemType, 0) as u32);
        if !matches!(
            ty,
            ItemType::MeleeWeapon
                | ItemType::MissileWeapon
                | ItemType::Caster
                | ItemType::Armor
                | ItemType::Clothing
                | ItemType::Jewelry
                | ItemType::Gem
        ) {
            log::error!("unsupported source loot item type for {}", e.wcid);
            crate::managers::guid_manager::recycle_dynamic_guid(w, item.guid);
            continue;
        }
        mutation::mutate(w, &mut item, p, t, c, e, q, r);
        return Some(item);
    }
    None
}
pub(super) fn int(o: &WorldObject, p: PropertyInt, default: i32) -> i32 {
    o.get_property(p).unwrap_or(default)
}
pub(super) fn float(
    o: &WorldObject,
    p: empyrean_entity::enums::PropertyFloat,
    default: f64,
) -> f64 {
    o.get_property(p).unwrap_or(default)
}
