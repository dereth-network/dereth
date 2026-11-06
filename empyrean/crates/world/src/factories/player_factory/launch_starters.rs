//! Not ACE: Launch's starter items, spells and account-formula supplies (V467).
// Grant rules adapted from ac1999, AGPL-3.0: starter provisioning.

use std::collections::BTreeMap;

use super::{create_new_world_object_by_wcid, with_player_in_world, CreatedPlayer};
use crate::World;
use empyrean_entity::enums::{PropertyDataId, Skill, SkillAdvancementClass};

#[allow(clippy::too_many_lines)] // The skill and heritage grant table is kept together.
pub(super) fn grant(w: &mut World, p: &mut CreatedPlayer, account_id: u32) -> Result<(), String> {
    let heritage = p.player.heritage_group().0;
    let index = match heritage {
        1 => 0,
        2 => 1,
        3 => 2,
        _ => return Err("unsupported starter heritage".into()),
    };
    let advancement = |skill| {
        p.player
            .biota
            .get_skill(skill)
            .map(|s| s.sac)
            .unwrap_or(SkillAdvancementClass::Inactive)
    };
    let trained = |skill| {
        matches!(
            advancement(skill),
            SkillAdvancementClass::Trained | SkillAdvancementClass::Specialized
        )
    };
    let specialized = |skill| advancement(skill) == SkillAdvancementClass::Specialized;
    let mut items = BTreeMap::from([
        (273, 100u32),
        (1077, 1),
        (166, 1),
        (5084, 1),
        ([259, 261, 258][index], 1),
    ]);
    for (skill, weapons) in [
        (Skill::Axe, [517, 536, 530]),
        (Skill::Bow, [518, 537, 531]),
        (Skill::Crossbow, [521; 3]),
        (Skill::ThrownWeapon, [522, 522, 532]),
        (Skill::Dagger, [527, 523, 527]),
        (Skill::Mace, [520, 526, 524]),
        (Skill::Spear, [534, 519, 539]),
        (Skill::Staff, [529, 528, 525]),
        (Skill::Sword, [535, 533, 538]),
    ] {
        if trained(skill) {
            items.insert(
                weapons[index],
                if skill == Skill::ThrownWeapon { 30 } else { 1 },
            );
        }
    }
    for (skill, wcid, count) in [
        (Skill::Bow, 4184, 30),
        (Skill::Crossbow, 4189, 30),
        (Skill::Alchemy, 4751, 1),
        (Skill::Cooking, 4761, 6),
        (Skill::Cooking, 4746, 6),
        (Skill::Fletching, 4586, 30),
        (Skill::Fletching, 5339, 30),
        (Skill::Fletching, 4585, 30),
        (Skill::Healing, 628, 1),
        (Skill::Lockpick, 511, 1),
    ] {
        if trained(skill) {
            items.insert(wcid, count);
        }
    }
    if specialized(Skill::Healing) {
        items.insert(628, 2);
    }
    if specialized(Skill::Alchemy) {
        items.insert(2414, 3);
    }
    let mut spells = Vec::new();
    for (skill, first, extra) in [
        (Skill::CreatureEnchantment, [1, 18], 678),
        (Skill::ItemEnchantment, [35, 51], 49),
        (Skill::LifeMagic, [23, 6], 53),
        (Skill::WarMagic, [27, 86], 75),
    ] {
        if trained(skill) {
            spells.extend(first);
        }
        if specialized(skill) {
            spells.push(extra);
        }
    }
    if !spells.is_empty() {
        items.insert([4914, 4916, 4915][index], 1);
        let account = w
            .auth
            .lock()
            .get_account_by_id(account_id)
            .ok_or("starter account is missing")?
            .account_name;
        let mut components = BTreeMap::new();
        // Component items recognized by the original component catalog, in WCID order.
        for wcid in (625..=627)
            .chain(686..=691)
            .chain(740..=792)
            .chain(1643..=1654)
        {
            if let Some(id) = w
                .content
                .get_cached_weenie(wcid)
                .and_then(|item| item.get_property(PropertyDataId::SpellComponent))
            {
                if id != 0 {
                    components.entry(id).or_insert(wcid);
                }
            }
        }
        let mut supplies = BTreeMap::<u32, u32>::new();
        for &spell in &spells {
            if w.content.get_cached_spell(spell).is_none() {
                return Err(format!("missing starter spell content {spell}"));
            }
            let formula = empyrean_dat::file_types::spell_table::get_spell_formula(
                w.dats.portal_dat().spell_table(),
                spell,
                &account,
            )
            .map_err(|e| format!("starter spell {spell}: {e}"))?;
            if formula.is_empty() {
                return Err(format!("starter spell {spell} has no formula"));
            }
            let mut per_spell = BTreeMap::<u32, u32>::new();
            for id in formula.into_iter().filter(|id| *id != 0) {
                let wcid = components
                    .get(&id)
                    .ok_or_else(|| format!("missing starter component {id}"))?;
                *per_spell.entry(*wcid).or_default() += 1;
            }
            for (wcid, count) in per_spell {
                let entry = supplies.entry(wcid).or_default();
                *entry = (*entry).max(count);
            }
        }
        // Shared components use the largest requirement of any one granted spell, not a sum.
        items.extend(supplies);
    }
    // Resolve every item before granting anything; creation fails without saving a partial kit.
    for &wcid in items.keys() {
        if w.content.get_cached_weenie(wcid).is_none() {
            return Err(format!("missing starter item {wcid}"));
        }
    }
    for (wcid, mut count) in items {
        while count > 0 {
            let mut item = create_new_world_object_by_wcid(w, wcid)
                .ok_or_else(|| format!("cannot create starter item {wcid}"))?;
            let quantity = if item.is_stackable() && item.stack_size().is_some() {
                let max = item.max_stack_size().map(u32::from).unwrap_or(1).max(1);
                let quantity = count.min(max);
                item.set_stack_size(Some(i32::try_from(quantity).expect("bounded stack")));
                quantity
            } else {
                1
            };
            let guid = item.guid;
            if !with_player_in_world(w, p, Some(item), |w, player| {
                crate::world_objects::container::try_add_to_inventory(
                    w, player, guid, 0, true, false,
                )
            }) {
                return Err(format!("starter item {wcid} does not fit"));
            }
            count -= quantity;
        }
    }
    with_player_in_world(w, p, None, |w, player| {
        crate::world_objects::player_commerce::update_coin_value(w, player, false);
    });
    for spell in spells {
        p.player
            .biota
            .get_or_add_known_spell(i32::try_from(spell).expect("starter spell"), 2.0);
    }
    Ok(())
}
