//! What a playtest character carries: the items its recipe gives, starter gear as character
//! creation gives it, and where each goes in its packs; and whether it can carry it all.
//!
//! Side packs among the items go in the main pack's side-pack places. The rest fill the main
//! pack, then each side pack in turn. Arrows (or bolts, or darts) the character's missile weapon
//! shoots are worn ready to shoot, not packed.

use dereth_progression::gear::AMMUNITION;
use dereth_progression::load::{self, Carried, Load};
use dereth_progression::order::Order;
use dereth_progression::{Build, Sac};
use empyrean_entity::enums::{PropertyInt, Skill, WeenieType};
use empyrean_entity::Biota;
use empyrean_world::factories::starter_gear_factory;
use empyrean_world::factories::world_object_factory::create_new_world_object_by_wcid_in_world;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;

use super::gear::Worn;

/// Where a missile weapon is wielded.
const MISSILE_WEAPON: u32 = 0x0040_0000;

/// One of class `class` made, `count` of it in stacks where it stacks: each stack an object.
fn made(w: &mut World, class: u32, count: u32) -> Result<Vec<WorldObject>, String> {
    let weenie = w
        .content
        .get_cached_weenie(class)
        .ok_or_else(|| format!("the world has no class {class}"))?;
    let stack = weenie
        .get_property(PropertyInt::MaxStackSize)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 1);
    let mut out = Vec::new();
    let mut left = count.clamp(1, 10_000);
    while left > 0 {
        let mut object = create_new_world_object_by_wcid_in_world(w, class)
            .ok_or_else(|| format!("class {class} cannot be made"))?;
        let n = stack.map_or(1, |s| left.min(s));
        if stack.is_some() {
            // The stack's burden and value follow its size.
            object.set_stack_size(Some(i32::try_from(n).map_err(|_| "too many")?));
        }
        out.push(object);
        left -= n;
    }
    Ok(out)
}

/// The items of `order` given as they are: each class made, a stackable one in stacks.
///
/// # Errors
/// A class the world lacks.
pub fn given(w: &mut World, order: &Order) -> Result<Vec<WorldObject>, String> {
    let mut out = Vec::new();
    for item in &order.items {
        out.extend(made(w, item.class, item.count)?);
    }
    Ok(out)
}

/// What a new character with `built`'s skills and heritage is given, as character creation gives
/// it: the starter items of each skill it trains or specializes, and that skill's heritage
/// items, a class given twice making one stack; a melee weapon twice for one who trains Dual
/// Wield. A class the world lacks is left out.
pub fn starter(w: &mut World, built: &Build) -> Vec<WorldObject> {
    let config = starter_gear_factory::get_starter_gear_configuration_for(w.era.starter_gear);
    let trains = |id: u32| {
        built
            .skills
            .iter()
            .any(|s| s.id == id && s.sac >= Sac::Trained)
    };
    let dual = trains(u32::try_from(Skill::DualWield.0).unwrap_or(0));
    let mut out: Vec<WorldObject> = Vec::new();
    for skill in config.skills {
        if !trains(u32::from(skill.skill_id)) {
            continue;
        }
        let heritage = skill
            .heritage
            .iter()
            .filter(|h| u32::from(h.heritage_id) == built.heritage)
            .flat_map(|h| h.gear.iter());
        for item in skill.gear.iter().chain(heritage) {
            if let Some(have) = out
                .iter_mut()
                .find(|o| o.biota.weenie_class_id == item.weenie_id)
            {
                if have.max_stack_size().map_or(1, i32::from) > 1 {
                    let n = have
                        .stack_size()
                        .unwrap_or(1)
                        .saturating_add(i32::from(item.stack_size));
                    let most = have.max_stack_size().map_or(n, i32::from);
                    have.set_stack_size(Some(n.min(most)));
                }
                continue;
            }
            let Ok(mut objects) = made(w, item.weenie_id, 1) else {
                continue;
            };
            let Some(mut object) = objects.pop() else {
                continue;
            };
            if object.max_stack_size().map_or(1, i32::from) > 1 {
                let most = object.max_stack_size().map_or(1, i32::from);
                object.set_stack_size(Some(i32::from(item.stack_size).min(most)));
            }
            let melee = object.biota.weenie_type == WeenieType::MeleeWeapon;
            out.push(object);
            if dual && melee {
                if let Ok(second) = made(w, item.weenie_id, 1) {
                    out.extend(second);
                }
            }
        }
    }
    out
}

/// The ammunition among `carried` that the missile weapon among `worn` shoots, taken out to be
/// worn ready: the first stack of it.
pub fn ammunition(worn: &[Worn], carried: &mut Vec<WorldObject>) -> Option<WorldObject> {
    let shoots = worn
        .iter()
        .find(|x| x.at == MISSILE_WEAPON)?
        .item
        .biota
        .get_property(PropertyInt::AmmoType)?;
    let at = carried.iter().position(|o| {
        super::gear::valid_locations(&o.biota) & AMMUNITION != 0
            && o.biota.get_property(PropertyInt::AmmoType) == Some(shoots)
    })?;
    Some(carried.remove(at))
}

/// Whether `item` is a side pack: a container that holds things.
fn side_pack(item: &Biota) -> Option<u32> {
    (item.weenie_type == WeenieType::Container)
        .then(|| item.get_property(PropertyInt::ItemsCapacity))
        .flatten()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n > 0)
}

/// What one carried object is, as the load reads it: its whole stack's burden, one thing.
fn carried_as(item: &Biota) -> Carried {
    Carried {
        name: super::gear::name(item),
        burden: item
            .get_property(PropertyInt::EncumbranceVal)
            .and_then(|b| u32::try_from(b).ok())
            .unwrap_or(0),
        count: 1,
        stack: 1,
        holds: side_pack(item).unwrap_or(0),
    }
}

/// The load of a character built as `built` (its stored augmentations in `player`) wearing
/// `worn`, keeping `kept` (worn where it was, or in its packs) and carrying `carried`.
#[must_use]
pub fn load_of(
    player: &Biota,
    built: &Build,
    worn: &[&Biota],
    kept: &[Biota],
    carried: &[WorldObject],
) -> Load {
    let burden = |b: &Biota| {
        b.get_property(PropertyInt::EncumbranceVal)
            .and_then(|v| u64::try_from(v).ok())
            .unwrap_or(0)
    };
    let worn_kept = |b: &Biota| {
        b.get_property(PropertyInt::CurrentWieldedLocation)
            .is_some()
    };
    let on: u64 = worn.iter().map(|b| burden(b)).sum::<u64>()
        + kept
            .iter()
            .filter(|b| worn_kept(b))
            .map(burden)
            .sum::<u64>();
    let things: Vec<Carried> = kept
        .iter()
        .filter(|b| !worn_kept(b))
        .chain(carried.iter().map(|o| &o.biota))
        .map(carried_as)
        .collect();
    let augmentations = player
        .get_property(PropertyInt::AugmentationIncreasedCarryingCapacity)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0);
    let mut out = load::load(built.attribute(1), augmentations, on, &things);
    out.extra_packs = player
        .get_property(PropertyInt::AugmentationExtraPackSlot)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0);
    out
}

/// Where each of `carried` goes in the packs of character `owner`, after the `kept` things
/// already there: `(object, the container it is in, its place there)`. Side packs go in the main
/// pack's side-pack places; the rest fill the main pack, then each side pack in turn.
#[must_use]
pub fn pack(owner: u32, kept: u32, carried: Vec<WorldObject>) -> Vec<(Biota, u32, usize)> {
    let mut packs: Vec<(u32, u32)> = Vec::new();
    let mut out = Vec::new();
    let mut things = Vec::new();
    for o in carried {
        match side_pack(&o.biota) {
            Some(holds) => {
                packs.push((o.biota.id, holds));
                let at = packs.len() - 1;
                out.push((o.biota, owner, at));
            }
            None => things.push(o.biota),
        }
    }
    let mut room = vec![(
        owner,
        load::MAIN_PACK.saturating_sub(kept),
        usize::try_from(kept).unwrap_or(0),
    )];
    room.extend(packs.iter().map(|(id, holds)| (*id, *holds, 0)));
    let mut into = room.iter_mut();
    let mut at = into.next();
    for item in things {
        while at.as_ref().is_some_and(|(_, left, _)| *left == 0) {
            at = into.next();
        }
        // More than the packs hold is refused before packing; the rest go in the main pack.
        match at.as_mut() {
            Some((id, left, index)) => {
                out.push((item, *id, *index));
                *left -= 1;
                *index += 1;
            }
            None => out.push((item, owner, 0)),
        }
    }
    out
}
