//! Source spell-family selection and magical-item properties.
// Rules ported from ac1999 (Adam Plumb), AGPL-3.0: Server/Source/TreasureFactory.cpp.

use empyrean_common::dotnet::CsCast;

use super::{
    int,
    rng::{self, Favor, LootRng},
};
use crate::{entity::spell::Spell, world_objects::world_object::WorldObject, World};
use empyrean_content::loot::{
    self, Category, Entry, SourceLootPolicy, SpellCategory, SpellFamily, Tier,
};
use empyrean_entity::enums::{
    EquipMask, ItemType, MagicSchool, PropertyDataId as D, PropertyFloat as F, PropertyInt as I,
    PropertyString as S, Skill, Usable,
};

#[derive(Debug, Default)]
struct Magic {
    count: u32,
    levels: u32,
    mana: i32,
    power: i32,
    highest: i32,
    buffs: Vec<u32>,
    gem: Option<u32>,
}
impl Magic {
    fn add(&mut self, p: &SourceLootPolicy, level: usize) {
        self.count += 1;
        self.levels += u32::try_from(level).expect("validated spell level") + 1;
        self.mana += p.profile.spell_properties.spell_mana[level];
        let power = p.profile.spell_properties.spell_power[level];
        self.power += power;
        self.highest = self.highest.max(power);
    }
}
fn merge<'a>(out: &mut Vec<&'a SpellFamily>, more: &'a [SpellFamily]) {
    for f in more {
        if !out.iter().any(|x| x.spell_name == f.spell_name) {
            out.push(f);
        }
    }
}
fn families(p: &SourceLootPolicy, ty: ItemType, coverage: u32) -> Vec<&SpellFamily> {
    let base = match ty {
        ItemType::MeleeWeapon => "melee",
        ItemType::MissileWeapon => "missile",
        ItemType::Caster => "caster",
        ItemType::Clothing => "clothing-item",
        ItemType::Jewelry => "jewelry",
        ItemType::Gem => "gem",
        ItemType::Armor => "armor-item",
        _ => return Vec::new(),
    };
    let mut out = p.spells[base].iter().collect();
    let covers = |mask: EquipMask| coverage & mask.0 != 0;
    if ty == ItemType::Clothing {
        let extra = if covers(EquipMask::HeadWear) {
            Some("clothing-head")
        } else if covers(EquipMask::HandWear) {
            Some("clothing-hands")
        } else if covers(EquipMask::FootWear) {
            Some("clothing-feet")
        } else {
            None
        };
        if let Some(name) = extra {
            merge(&mut out, &p.spells[name]);
        }
    } else if ty == ItemType::Armor {
        for (mask, name) in [
            (EquipMask::HeadWear, "armor-head"),
            (EquipMask::ChestArmor, "armor-upper"),
            (EquipMask::UpperArmArmor, "armor-upper"),
            (EquipMask::LowerArmArmor, "armor-upper"),
        ] {
            if covers(mask) {
                merge(&mut out, &p.spells[name]);
            }
        }
        if covers(EquipMask::AbdomenArmor)
            && !covers(EquipMask::UpperLegArmor)
            && !covers(EquipMask::LowerLegArmor)
        {
            merge(&mut out, &p.spells["armor-upper"]);
        }
        for (mask, name) in [
            (EquipMask::HandWear, "armor-hands"),
            (EquipMask::UpperLegArmor, "armor-lower"),
            (EquipMask::LowerLegArmor, "armor-lower"),
            (EquipMask::FootWear, "armor-feet"),
            (EquipMask::Shield, "shield"),
        ] {
            if covers(mask) {
                merge(&mut out, &p.spells[name]);
            }
        }
    }
    out
}
fn level(t: &Tier, f: &SpellFamily, q: f64, r: &mut impl LootRng) -> usize {
    rng::favored(
        r,
        t.min_spell_level as i32 - 1,
        i32::try_from((t.max_spell_level as usize).min(f.spells.len()))
            .expect("validated spell level")
            - 1,
        Favor::Specific(f64::from(t.preferred_spell_level) - 1.0),
        t.preferred_spell_level_strength,
        q,
    ) as usize
}

#[allow(
    clippy::too_many_arguments,
    reason = "The selected source records and injected random stream share one pipeline."
)]
pub(super) fn assign(
    w: &World,
    o: &mut WorldObject,
    p: &SourceLootPolicy,
    t: &Tier,
    c: &Category,
    e: &Entry,
    q: f64,
    r: &mut impl LootRng,
) -> u32 {
    let ty = ItemType(int(o, I::ItemType, 0) as u32);
    let mut magic = Magic::default();
    if ty == ItemType::Caster {
        cast_spell(w, o, p, t, e, q, r, &mut magic);
    }
    let mut pool = families(p, ty, int(o, I::ValidLocations, 0) as u32);
    if !pool.is_empty() && t.max_amount_of_spells > 0 && rng::chance(r, t.chance_of_spells, q) {
        let mut count = rng::favored(r, 1, t.max_amount_of_spells as i32, Favor::Low, 4.5, q);
        if c.category == "gemstones" || ty == ItemType::Jewelry || ty == ItemType::Clothing {
            count = 1;
        }
        for attempt in 0..count {
            if pool.is_empty() {
                break;
            }
            let index = if attempt == 0 && ty == ItemType::Armor {
                0
            } else {
                rng::index(r, pool.len())
            };
            let family = pool[index];
            let level = level(t, family, q, r);
            let id = family.spells[level];
            if id == 0 {
                continue;
            }
            pool.remove(index);
            if family.spell_category == SpellCategory::WeaponSkillMastery {
                pool.retain(|f| f.spell_category != SpellCategory::WeaponSkillMastery);
            }
            let spell = Spell::new(w, id, true);
            if spell.not_found() {
                log::error!("source loot spell {id} is absent from local content/assets");
                continue;
            }
            if c.category == "gemstones" {
                o.set_property(D::Spell, id);
                o.set_property(I::ItemUseable, Usable::Contained.0 as i32);
                magic.gem = Some(id);
            }
            o.biota
                .properties_spell_book
                .get_or_insert_with(Default::default)
                .insert(id as i32, 2.0);
            magic.add(p, level);
            if matches!(
                spell.school(),
                MagicSchool::LifeMagic | MagicSchool::CreatureEnchantment
            ) {
                magic
                    .buffs
                    .push(u32::try_from(level).expect("validated spell level") + 1);
            }
        }
    }
    if magic.count > 0 {
        finish(w, o, ty, e, &magic, r);
    }
    magic.levels
}
#[allow(
    clippy::too_many_arguments,
    reason = "Cast spells update the same item and magic accumulator as passive spells."
)]
fn cast_spell(
    w: &World,
    o: &mut WorldObject,
    p: &SourceLootPolicy,
    t: &Tier,
    e: &Entry,
    q: f64,
    r: &mut impl LootRng,
    m: &mut Magic,
) {
    if r.unit() >= 0.1 {
        return;
    }
    let support = e.cast_spell_pool == "support";
    let pool = &p.spells[if support {
        "caster-support"
    } else {
        "caster-attack"
    }];
    let family = &pool[rng::index(r, pool.len())];
    let level = level(t, family, q, r);
    let id = family.spells[level];
    let spell = Spell::new(w, id, true);
    if spell.not_found() {
        log::error!("source cast spell {id} is absent from local content/assets");
        return;
    }
    o.set_property(D::Spell, id);
    o.set_property(
        I::ItemUseable,
        Usable::SourceWieldedTargetRemoteNeverWalk.0 as i32,
    );
    o.set_property(
        I::ItemManaCost,
        (f64::from(spell.base_mana()) * if support { 5.0 } else { 2.5 }).cs_cast(),
    );
    m.add(p, level);
}
fn finish(
    w: &World,
    o: &mut WorldObject,
    ty: ItemType,
    e: &Entry,
    m: &Magic,
    r: &mut impl LootRng,
) {
    o.set_property(I::UiEffects, 1);
    let mut craft: i32 = rng::real(
        r,
        f64::from(m.highest) * 0.92,
        f64::from(m.highest) * 1.10,
        Some(Favor::Mid),
        2.0,
        0.0,
    )
    .round()
    .cs_cast();
    if ty == ItemType::Caster {
        craft = craft.max(25);
    }
    let mut max_mana: i32 = (f64::from(m.mana) * e.mana_capacity_mult)
        .round()
        .max(1.0)
        .cs_cast();
    let mut current = rng::favored(r, max_mana / 2, max_mana, Favor::Mid, 2.0, 0.0);
    if let Some(id) = m.gem {
        max_mana = (Spell::new(w, id, true).base_mana() as i32).max(1);
        current = max_mana;
        o.set_property(I::ItemManaCost, max_mana);
    }
    o.set_property(I::ItemSpellcraft, craft);
    o.set_property(I::ItemMaxMana, max_mana);
    o.set_property(I::ItemCurMana, current);
    o.set_property(F::ManaRate, (-f64::from(m.power) / 10000.0).min(-0.0166));
    let mut difficulty = f64::from(craft);
    let skill = if matches!(ty, ItemType::MeleeWeapon | ItemType::MissileWeapon) {
        let own = int(o, I::WeaponSkill, 0);
        if own != 0 && int(o, I::WeaponType, 0) != 10 && r.unit() < 0.25 {
            own
        } else {
            0
        }
    } else if ty == ItemType::Armor && r.unit() < 0.20 {
        if r.integer(0, 1) != 0 {
            Skill::MeleeDefense.0
        } else {
            Skill::MissileDefense.0
        }
    } else {
        0
    };
    if skill != 0 {
        let limit = if skill == Skill::MissileDefense.0 {
            (f64::from(craft + 20) * 0.7).cs_cast()
        } else {
            craft + 20
        };
        o.set_property(D::ItemSkillLimit, skill as u32);
        o.set_property(I::ItemSkillLevelLimit, limit);
        difficulty -= f64::from(limit) / 2.0;
    }
    if ty != ItemType::Gem && r.unit() < 0.25 {
        let heritage = e
            .heritage
            .as_deref()
            .and_then(loot::heritage_id)
            .unwrap_or_else(|| r.integer(1, 3));
        o.set_property(I::HeritageGroup, heritage);
        o.set_property(
            S::ItemHeritageGroupRestriction,
            ["Aluvian", "Gharu'ndim", "Sho"][(heritage - 1) as usize].to_string(),
        );
        difficulty -= f64::from(craft) / 4.0;
    }
    let chance = if e.allegiance_rank_chance >= 0.0 {
        e.allegiance_rank_chance
    } else {
        0.04
    };
    if ty != ItemType::Gem && r.unit() < chance {
        let rank = r.integer(2, 3);
        o.set_property(I::ItemAllegianceRankLimit, rank);
        difficulty -= f64::from(rank * craft) / 8.0;
    }
    difficulty = difficulty.max(0.0);
    let mut buffs = m.buffs.clone();
    buffs.sort_unstable();
    buffs.pop();
    for level in buffs {
        difficulty += f64::from(level) * 5.0 * (0.5 + r.unit());
    }
    if ty == ItemType::Gem {
        difficulty = 0.0;
    }
    o.set_property(I::ItemDifficulty, difficulty.round().max(0.0).cs_cast());
}
