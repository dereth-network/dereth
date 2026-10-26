//! Gear for a playtest character: loot of a tier, chosen so the character can wear and wield it.
//!
//! Every rolled item is one the loot of that tier could have dropped: a base item rolled as the
//! loot tables roll it and mutated as a drop of the tier. What is ours is the choice of items,
//! slot by slot, each slot rolled on its own seed so that rolling one again leaves the rest:
//! - **the weapon** is rolled from the character's main skill's own weapons (a caster for a
//!   mage or a summoner), again and again until its wield requirement is one the character meets
//!   with its base values; if none does, the best one rolled a tier lower;
//! - **the off hand**, beside a one-handed weapon: a second weapon of its skill when the
//!   character trains Dual Wield, else a shield when it trains Shield (or when the era's files
//!   have no Shield skill, when anyone could carry one);
//! - **the suit** is one armor type, chosen as the tier's armor type chances choose it, and each
//!   armor slot's piece rolled from that type's own pieces until one covers it and nothing
//!   already covered, every piece one the character can wear;
//! - **better gear** keeps the best of three rolls of each slot (the one the loot values most),
//!   and adds jewelry, a trinket and, where the era's loot has them, a cloak;
//! - **a slot filled with a chosen item** takes it as it is, and is refused when the item cannot
//!   go there or the character cannot wear or wield it.

use dereth_progression::gear::{self as rule, Requirement, Slot};
use dereth_progression::order::{Gear, Order};
use dereth_progression::{Build, Tables};
use empyrean_common::era::{EraId, LootRules};
use empyrean_common::random::with_seed;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::{PropertyDataId, PropertyInt, PropertyString, WeenieType};
use empyrean_entity::Biota;
use empyrean_tables::enums::{
    TreasureArmorType, TreasureItemCategory, TreasureItemType, TreasureWeaponType, WeenieClassName,
};
use empyrean_world::factories::loot_generation_factory as loot;
use empyrean_world::factories::loot_generation_factory::tables_logic::wcids::{
    armor_wcids, weapon_wcids,
};
use empyrean_world::factories::world_object_factory::create_new_world_object_by_wcid_in_world;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// The slots a suit covers: head, hands, feet, and the six armor slots of the body.
pub const ARMOR_SLOTS: u32 = rule::ARMOR_LOCATIONS;

/// How many weapons are rolled at a tier before one a tier lower is taken.
const WEAPON_TRIES: u32 = 60;

/// How many pieces of the suit's type are rolled for a slot before it is left empty.
const PIECE_TRIES: u32 = 80;

/// How many shields, jewels or cloaks are rolled for a slot before it is left empty.
const OTHER_TRIES: u32 = 200;

/// How many rolls better gear keeps the best of.
const BETTER_ROLLS: usize = 3;

/// The Shield and Dual Wield skills.
const SHIELD: u32 = 48;
const DUAL_WIELD: u32 = 49;

/// What the character wielding an item has.
#[derive(Debug, Clone, Copy)]
pub struct Wielder<'a> {
    pub build: &'a Build,
    pub tables: Tables<'a>,
}

impl Wielder<'_> {
    /// Whether the character meets every wield requirement `item` has, with its base values.
    #[must_use]
    pub fn meets(&self, item: &Biota) -> bool {
        requirements(item)
            .iter()
            .all(|r| rule::meets(self.build, &self.tables, r))
    }

    /// Whether the character trains skill `id`.
    fn trains(&self, id: u32) -> bool {
        self.build
            .skills
            .iter()
            .any(|s| s.id == id && s.sac >= dereth_progression::Sac::Trained)
    }
}

/// Each wield requirement `item` has.
#[must_use]
pub fn requirements(item: &Biota) -> Vec<Requirement> {
    [
        (
            PropertyInt::WieldRequirements,
            PropertyInt::WieldSkillType,
            PropertyInt::WieldDifficulty,
        ),
        (
            PropertyInt::WieldRequirements2,
            PropertyInt::WieldSkillType2,
            PropertyInt::WieldDifficulty2,
        ),
        (
            PropertyInt::WieldRequirements3,
            PropertyInt::WieldSkillType3,
            PropertyInt::WieldDifficulty3,
        ),
        (
            PropertyInt::WieldRequirements4,
            PropertyInt::WieldSkillType4,
            PropertyInt::WieldDifficulty4,
        ),
    ]
    .into_iter()
    .filter_map(|(r, s, d)| {
        let kind = item.get_property(r).unwrap_or(0);
        (kind != 0).then(|| Requirement {
            kind,
            of: item.get_property(s).unwrap_or(0),
            at: item.get_property(d).unwrap_or(0),
        })
    })
    .collect()
}

/// What wielding `item` takes, as a player reads it ("Heavy Weapons 250", "level 180").
#[must_use]
pub fn wield_text(t: &Tables<'_>, item: &Biota) -> String {
    requirements(item)
        .iter()
        .map(|r| rule::requirement_text(t, r))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The tier a character of `level` hunts at: the highest tier whose creatures' median level is
/// at or below it, from the world database's creatures and their death treasures (worked out
/// once for each era's world).
#[must_use]
pub fn tier_for_level(w: &World, level: u32) -> u32 {
    static MEDIANS: Mutex<BTreeMap<EraId, [u32; 8]>> = Mutex::new(BTreeMap::new());
    let mut known = MEDIANS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let medians = known.entry(w.era.id).or_insert_with(|| {
        let deaths = w.content.get_all_treasure_death();
        let mut creatures = Vec::new();
        for weenie in w.content.get_all_weenies() {
            if weenie.r#type != WeenieType::Creature.0.cast_signed() {
                continue;
            }
            let Some(level) = weenie
                .weenie_properties_int
                .iter()
                .find(|p| p.r#type == PropertyInt::Level.0)
                .and_then(|p| u32::try_from(p.value).ok())
            else {
                continue;
            };
            for d in weenie
                .weenie_properties_did
                .iter()
                .filter(|d| d.r#type == PropertyDataId::DeathTreasureType.0)
            {
                if let Some(tier) = deaths
                    .get(&d.value)
                    .and_then(|t| u32::try_from(t.tier).ok())
                {
                    creatures.push((level, tier));
                }
            }
        }
        rule::level_medians(creatures)
    });
    rule::tier_for_level(medians, level)
}

/// A drop profile of `tier`, as the loot command makes one.
fn profile(tier: u32) -> TreasureDeath {
    TreasureDeath {
        tier: i32::try_from(tier).unwrap_or(1),
        loot_quality_mod: 0.0,
        ..Default::default()
    }
}

/// One base item of class `wcid`, mutated as loot of the profile's tier.
fn mutated(w: &mut World, wcid: WeenieClassName, death: &TreasureDeath) -> Option<WorldObject> {
    let class = u32::try_from(wcid.0).ok().filter(|c| *c != 0)?;
    let mut item = create_new_world_object_by_wcid_in_world(w, class)?;
    loot::mutate_item(w, &mut item, death, true);
    Some(item)
}

/// Where class `wcid` may be worn or wielded, as its weenie says: read before an item is made, so
/// one that cannot go in a slot is never made.
fn locations_of(w: &World, wcid: WeenieClassName) -> u32 {
    u32::try_from(wcid.0)
        .ok()
        .and_then(|c| w.content.get_cached_weenie(c))
        .and_then(|g| g.get_property(PropertyInt::ValidLocations))
        .map_or(0, i32::cast_unsigned)
}

/// Where `item` may be worn or wielded.
#[must_use]
pub fn valid_locations(item: &Biota) -> u32 {
    item.get_property(PropertyInt::ValidLocations)
        .map_or(0, i32::cast_unsigned)
}

/// What the loot values `item` at: how better gear tells the best of its rolls.
fn worth(item: &WorldObject) -> i32 {
    item.biota.get_property(PropertyInt::Value).unwrap_or(0)
}

/// One slot's roll, on its own seed: `tries` attempts, keeping the first that passes, or of
/// better gear the best of three that pass.
fn roll(
    w: &mut World,
    seed: u64,
    tries: u32,
    keep: usize,
    mut attempt: impl FnMut(&mut World) -> Option<WorldObject>,
) -> Option<WorldObject> {
    with_seed(seed, || {
        let mut found: Vec<WorldObject> = Vec::new();
        for _ in 0..tries {
            if let Some(item) = attempt(w) {
                found.push(item);
                if found.len() >= keep {
                    break;
                }
            }
        }
        let best = found.iter().map(worth).max()?;
        found.into_iter().find(|i| worth(i) == best)
    })
}

/// A weapon base item for main skill `skill`, as the loot tables roll that skill's weapons;
/// `None` for a skill they do not roll on its own.
fn weapon_class(w: &World, skill: u32, death: &TreasureDeath) -> Option<WeenieClassName> {
    use empyrean_tables::logic::weapons::{
        finesse_weapon_wcids, heavy_weapon_wcids, light_weapon_wcids, two_handed_weapon_wcids,
    };
    if w.era.loot_rules != LootRules::EndOfRetail {
        return None;
    }
    let mut kind = TreasureWeaponType::Undef;
    Some(match skill {
        44 => heavy_weapon_wcids::roll(&mut kind),
        45 => light_weapon_wcids::roll(&mut kind),
        46 => finesse_weapon_wcids::roll(&mut kind),
        41 => two_handed_weapon_wcids::roll(&mut kind),
        47 => weapon_wcids::roll_bow_wcid(death),
        34 | 43 | 54 => weapon_wcids::roll_caster(death),
        _ => return None,
    })
}

/// Whether `item` is a weapon of main skill `skill`: its weapon skill, or a caster for magic and
/// summoning.
fn of_skill(item: &Biota, skill: u32) -> bool {
    if matches!(skill, 31..=34 | 43 | 54) {
        return item.weenie_type == WeenieType::Caster;
    }
    item.get_property(PropertyInt::WeaponSkill)
        .and_then(|s| u32::try_from(s).ok())
        == Some(skill)
}

/// One weapon rolled at `tier` for main skill `skill`, kept when the character can wield it (and,
/// for the off hand, when it is wielded in one hand).
fn weapon_try(
    w: &mut World,
    who: &Wielder<'_>,
    skill: u32,
    tier: u32,
    one_hand: bool,
) -> Option<WorldObject> {
    let death = profile(tier);
    let item = match weapon_class(w, skill, &death) {
        Some(wcid) => {
            if one_hand && weapon_slot(locations_of(w, wcid)) != rule::ONE_HANDED {
                return None;
            }
            mutated(w, wcid, &death)
        }
        // An era whose loot rolls weapons by its own tables: a drop of the tier, kept when it is
        // of the skill.
        None => loot::create_random_loot_objects_of_category(
            w,
            &death,
            TreasureItemCategory::MagicItem,
            TreasureItemType::Weapon,
        )
        .filter(|i| of_skill(&i.biota, skill)),
    };
    item.filter(|i| {
        who.meets(&i.biota)
            && (!one_hand || weapon_slot(valid_locations(&i.biota)) == rule::ONE_HANDED)
    })
}

/// The weapon for main skill `skill` rolled on `seed`: one rolled at `tier` that the character
/// can wield, else at each tier lower in turn.
#[must_use]
pub fn weapon(
    w: &mut World,
    who: &Wielder<'_>,
    skill: u32,
    tier: u32,
    seed: u64,
    keep: usize,
    one_hand: bool,
) -> Option<WorldObject> {
    (1..=tier.max(1)).rev().find_map(|t| {
        roll(w, seed, WEAPON_TRIES, keep, |w| {
            weapon_try(w, who, skill, t, one_hand)
        })
    })
}

/// Where a weapon that may be wielded where `valid` says is wielded: the melee, missile or held
/// slot it may take, else all it may.
#[must_use]
pub fn weapon_slot(valid: u32) -> u32 {
    Slot::Weapon.location(valid).unwrap_or(valid)
}

/// The armor type a suit of `tier` is, chosen on the suit's own seed as the tier's armor type
/// chances choose it; only a type whose pieces the loot tables list.
fn suit_type(w: &World, tier: u32, seed: u64) -> Option<TreasureArmorType> {
    use empyrean_tables::logic::era::infiltration;
    use empyrean_tables::tables::wcids::armor_wcids::STATIC_CTOR_BUILD_COMBINED_ARGS as TABLES;
    let death = profile(tier);
    let end_of_retail = w.era.loot_rules == LootRules::EndOfRetail;
    with_seed(seed, || {
        (0..40).find_map(|_| {
            let kind = if end_of_retail {
                let mut kind = empyrean_tables::logic::tables::armor_type_chance::roll(death.tier);
                let _ = armor_wcids::roll(&death, &mut kind);
                kind
            } else {
                let kind = infiltration::roll_armor_type(death.tier);
                infiltration::roll_armor(kind, death.unknown_chances)?.1
            };
            TABLES.iter().any(|(_, k)| *k == kind).then_some(kind)
        })
    })
}

/// A piece of armor type `kind` for `slot`, at `tier`: rolled from the type's pieces, kept when
/// it covers the slot, nothing in `covered`, and the character can wear it.
fn piece_try(
    w: &mut World,
    who: &Wielder<'_>,
    kind: TreasureArmorType,
    slot: Slot,
    tier: u32,
    covered: u32,
) -> Option<WorldObject> {
    use empyrean_tables::tables::wcids::armor_wcids::STATIC_CTOR_BUILD_COMBINED_ARGS as TABLES;
    let table = TABLES.iter().find(|(_, k)| *k == kind)?.0;
    let wcid = table.roll(0.0);
    let at = slot.location(locations_of(w, wcid))?;
    if at & covered != 0 {
        return None;
    }
    mutated(w, wcid, &profile(tier)).filter(|i| who.meets(&i.biota))
}

/// A shield of `tier`: an armor piece rolled as a drop of the tier rolls one, kept when it is a
/// shield the character can carry.
fn shield_try(w: &mut World, who: &Wielder<'_>, tier: u32) -> Option<WorldObject> {
    use empyrean_tables::logic::era::infiltration;
    let death = profile(tier);
    let wcid = if w.era.loot_rules == LootRules::EndOfRetail {
        let mut kind = empyrean_tables::logic::tables::armor_type_chance::roll(death.tier);
        armor_wcids::roll(&death, &mut kind)
    } else {
        let kind = infiltration::roll_armor_type(death.tier);
        infiltration::roll_armor(kind, death.unknown_chances)?.0
    };
    Slot::Shield.location(locations_of(w, wcid))?;
    mutated(w, wcid, &death).filter(|i| who.meets(&i.biota))
}

/// A piece of jewelry, a trinket or a cloak for `slot`, a drop of `tier`, kept when it goes there
/// and the character can wear it.
fn jewel_try(w: &mut World, who: &Wielder<'_>, slot: Slot, tier: u32) -> Option<WorldObject> {
    let kind = if slot == Slot::Cloak {
        TreasureItemType::Cloak
    } else {
        TreasureItemType::Jewelry
    };
    loot::create_random_loot_objects_of_category(
        w,
        &profile(tier),
        TreasureItemCategory::MagicItem,
        kind,
    )
    .filter(|i| slot.location(valid_locations(&i.biota)).is_some() && who.meets(&i.biota))
}

/// An item worn or wielded, where, and the slot it fills.
#[derive(Debug)]
pub struct Worn {
    pub item: WorldObject,
    /// Where it is worn: everything an armor piece covers.
    pub at: u32,
    pub slot: Slot,
}

/// The chosen class `class` for `slot`, as it is.
///
/// # Errors
/// A class the world lacks, one that cannot go in the slot or where something already is, or one
/// the character cannot wear or wield.
fn chosen(
    w: &mut World,
    who: &Wielder<'_>,
    slot: Slot,
    class: u32,
    covered: u32,
) -> Result<Worn, String> {
    let item = create_new_world_object_by_wcid_in_world(w, class)
        .ok_or_else(|| format!("the world has no class {class}"))?;
    let name = name(&item.biota);
    let at = slot
        .location(valid_locations(&item.biota))
        .ok_or_else(|| format!("{name} cannot be worn as the {}", slot.word()))?;
    if at & covered != 0 {
        return Err(format!("{name} takes a place another chosen item takes"));
    }
    if !who.meets(&item.biota) {
        return Err(format!(
            "{name} needs {}, more than the character has",
            wield_text(&who.tables, &item.biota)
        ));
    }
    Ok(Worn { item, at, slot })
}

/// What a character built as `who` wears and wields, from `order` at `tier`: its chosen items
/// first, then loot for every slot they leave (none at tier 0).
///
/// # Errors
/// A chosen item that cannot be worn where it is asked for.
pub fn outfit(
    w: &mut World,
    who: &Wielder<'_>,
    order: &Order,
    tier: u32,
) -> Result<Vec<Worn>, String> {
    let mut worn = Vec::new();
    if order.gear == Gear::None {
        return Ok(worn);
    }
    let mut covered = 0;
    for (slot, s) in &order.slots {
        if let Some(class) = s.class {
            let c = chosen(w, who, *slot, class, covered)?;
            covered |= c.at;
            worn.push(c);
        }
    }
    if tier == 0 {
        return Ok(worn);
    }
    let better = order.gear == Gear::Better;
    let keep = if better { BETTER_ROLLS } else { 1 };
    let seed = |slot: Slot| slot.seed(order.seed, order.slots.get(&slot).map_or(0, |s| s.roll));
    let free = |covered: u32, slot: Slot| covered & slot.mask() == 0;
    let put = |worn: &mut Vec<Worn>, covered: &mut u32, item: WorldObject, slot: Slot| {
        let at = slot
            .location(valid_locations(&item.biota))
            .unwrap_or(slot.mask());
        *covered |= at;
        worn.push(Worn { item, at, slot });
    };

    // The weapon, and beside a one-handed one the off hand.
    let main = order
        .plan
        .specialized
        .first()
        .and_then(|s| who.tables.resolve_skill(s));
    if let Some(skill) = main.filter(|_| free(covered, Slot::Weapon)) {
        if let Some(item) = weapon(w, who, skill, tier, seed(Slot::Weapon), keep, false) {
            put(&mut worn, &mut covered, item, Slot::Weapon);
        }
    }
    let one_handed = worn.iter().any(|x| x.at == rule::ONE_HANDED);
    if one_handed && free(covered, Slot::Shield) {
        let off = if who.trains(DUAL_WIELD) {
            main.and_then(|skill| weapon(w, who, skill, tier, seed(Slot::Shield), keep, true))
        } else if who.trains(SHIELD) || !who.tables.has_skill(SHIELD) {
            roll(w, seed(Slot::Shield), OTHER_TRIES, keep, |w| {
                shield_try(w, who, tier)
            })
        } else {
            None
        };
        if let Some(item) = off {
            covered |= Slot::Shield.mask();
            worn.push(Worn {
                item,
                at: Slot::Shield.mask(),
                slot: Slot::Shield,
            });
        }
    }

    // The suit: one armor type, each slot's piece on its own seed.
    if let Some(kind) = suit_type(w, tier, rule::suit_seed(order.seed)) {
        for slot in Slot::ARMOR {
            if !free(covered, slot) {
                continue;
            }
            let have = covered;
            if let Some(item) = roll(w, seed(slot), PIECE_TRIES, keep, |w| {
                piece_try(w, who, kind, slot, tier, have)
            }) {
                put(&mut worn, &mut covered, item, slot);
            }
        }
    }

    // Better gear: jewelry, a trinket, and a cloak where the era's loot has them.
    if better {
        let cloaks = w.era.loot_rules == LootRules::EndOfRetail;
        for slot in Slot::JEWELRY
            .into_iter()
            .chain(cloaks.then_some(Slot::Cloak))
        {
            if !free(covered, slot) {
                continue;
            }
            if let Some(item) = roll(w, seed(slot), OTHER_TRIES, keep, |w| {
                jewel_try(w, who, slot, tier)
            }) {
                put(&mut worn, &mut covered, item, slot);
            }
        }
    }
    Ok(worn)
}

/// An item's name.
#[must_use]
pub fn name(item: &Biota) -> String {
    item.get_property(PropertyString::Name).unwrap_or_default()
}
