//! What a playtest character holds beyond its build: its augmentations, luminance, auras and
//! summoning mastery on the character itself, and its titles and quests on its character record.
//!
//! Each augmentation is held as using its gem leaves it (its count, its family's count, a side
//! pack more for the pack augmentation), and its experience was spent from the build's. Its
//! quests are its recipe's and nothing else, solved long enough ago that no timer runs. Its title
//! book is the title creation gave it and its recipe's, the first of those shown.

use dereth_progression::augmentations::{table, Costs, Family};
use dereth_progression::{Build, Extras};
use empyrean_entity::enums::{CharacterTitle, HeritageGroup, PropertyInt, PropertyInt64};
use empyrean_entity::Biota;
use empyrean_store::models::shard::{
    Character, CharacterPropertiesQuestRegistry, CharacterPropertiesTitleBook,
};
use empyrean_world::World;
use std::collections::BTreeMap;

/// The quest the server's experience check charges an augmentation for, and what it charges.
const LESSER_BENEDICTION: (&str, i64) = ("LesserBenedictionAug", 2_000_000_000);

/// What each augmentation's gem costs in this world, as the server's experience check reads it.
#[must_use]
pub fn costs(w: &World) -> Costs {
    Costs::of(|gem, _| {
        w.content
            .get_cached_weenie_by_class_name(gem)
            .and_then(|g| {
                g.properties_int64
                    .as_ref()?
                    .get(&PropertyInt64::AugmentationCost)
                    .copied()
            })
            .and_then(|c| u64::try_from(c).ok())
    })
}

/// Every augmentation and aura property `player` holds, and how many.
#[must_use]
pub fn held(player: &Biota) -> BTreeMap<u32, i32> {
    let t = table();
    t.augmentation
        .iter()
        .filter_map(|a| a.property)
        .chain(t.aura.iter().map(|a| a.property))
        .filter_map(|p| {
            let n = player.get_property(PropertyInt(u16::try_from(p).ok()?))?;
            (n != 0).then_some((p, n))
        })
        .collect()
}

/// What `record`'s quests add to a character's numbers as the server's checks count them: the
/// skill credits its quests granted, and the experience its quest-held augmentation cost.
#[must_use]
pub fn from_record(record: &Character) -> (i32, i64) {
    let solved: BTreeMap<String, u32> = record
        .character_properties_quest_registry
        .iter()
        .map(|q| {
            (
                q.quest_name.clone(),
                u32::try_from(q.num_times_completed).unwrap_or(0),
            )
        })
        .collect();
    let benediction = record
        .character_properties_quest_registry
        .iter()
        .any(|q| q.quest_name.eq_ignore_ascii_case(LESSER_BENEDICTION.0));
    (
        dereth_progression::extras::credits_of(&solved),
        if benediction { LESSER_BENEDICTION.1 } else { 0 },
    )
}

/// Make `player` and its `record` hold what `extras` and its build `b` say, for a character of
/// `level`.
///
/// # Errors
/// A title the server does not have, titles in a world whose era has none, a quest solved more
/// often than the world allows, or quests or titles for a character with no record.
pub fn write(
    w: &World,
    player: &mut Biota,
    record: Option<&mut Character>,
    level: u32,
    b: &Build,
    extras: &Extras,
) -> Result<(), String> {
    let t = table();
    let int = |p: u32| {
        u16::try_from(p)
            .map(PropertyInt)
            .map_err(|_| "no such property")
    };
    // The augmentations and auras: what the build holds, nothing else.
    for p in t
        .augmentation
        .iter()
        .filter_map(|a| a.property)
        .chain(t.aura.iter().map(|a| a.property))
    {
        player.try_remove_property(int(p)?);
    }
    let mut families: BTreeMap<Family, i32> = BTreeMap::new();
    for (p, n) in &b.held {
        player.set_property(int(*p)?, *n);
        if let Some(f) = t.by_property(*p).and_then(|a| a.family) {
            *families.entry(f).or_default() += n;
        }
    }
    for (family, property) in [
        (Family::Innate, PropertyInt::AugmentationInnateFamily),
        (
            Family::Resistance,
            PropertyInt::AugmentationResistanceFamily,
        ),
    ] {
        match families.get(&family) {
            Some(n) if *n > 0 => {
                player.set_property(property, *n);
            }
            _ => {
                player.try_remove_property(property);
            }
        }
    }
    // Seven side packs and one more for the pack augmentation, as a player's are counted each time
    // it is loaded.
    let packs = 7;
    let more = b
        .held
        .get(&u32::from(PropertyInt::AugmentationExtraPackSlot.0))
        .copied()
        .unwrap_or(0);
    player.set_property(PropertyInt::ContainersCapacity, packs + more);
    // Luminance: all it can hold, all spent.
    if extras.luminous(level) {
        player.set_property(PropertyInt64::MaximumLuminance, t.luminance.maximum);
        player.set_property(PropertyInt64::AvailableLuminance, 0);
    } else {
        player.try_remove_property(PropertyInt64::MaximumLuminance);
        player.try_remove_property(PropertyInt64::AvailableLuminance);
    }
    // The summoning mastery, as a statue in Arwic sets it; none for none.
    match extras.summoning_mastery {
        Some(m) => {
            player.set_property(PropertyInt::SummoningMastery, m.number());
        }
        None => {
            player.try_remove_property(PropertyInt::SummoningMastery);
        }
    }

    let Some(record) = record else {
        if extras.titles.is_empty() && extras.solved(level).is_empty() {
            return Ok(());
        }
        return Err("the character has no character record".into());
    };
    // The quests, solved long ago.
    let mut quests = Vec::new();
    for (name, n) in extras.solved(level) {
        if let Some(q) = w.content.get_cached_quest(&name) {
            if q.max_solves > 0 && i64::from(n) > i64::from(q.max_solves) {
                return Err(format!(
                    "quest {name} can be solved at most {} times",
                    q.max_solves
                ));
            }
        }
        quests.push(CharacterPropertiesQuestRegistry {
            character_id: record.id,
            quest_name: name,
            last_time_completed: 0,
            num_times_completed: i32::try_from(n).map_err(|_| "too many solves")?,
        });
    }
    record.character_properties_quest_registry = quests;

    // The titles: creation's and the recipe's, the recipe's first shown.
    if !w.era.features.titles {
        if !extras.titles.is_empty() {
            return Err("this world's era has no titles".into());
        }
        return Ok(());
    }
    if let Some(bad) = extras
        .titles
        .iter()
        .find(|id| !CharacterTitle(**id).is_defined())
    {
        return Err(format!("there is no title {bad}"));
    }
    let heritage = player.get_property(PropertyInt::HeritageGroup).unwrap_or(0);
    let created = creation_title(w, HeritageGroup(heritage));
    let mut book: Vec<u32> = Vec::new();
    for id in created.into_iter().chain(extras.titles.iter().copied()) {
        if !book.contains(&id) {
            book.push(id);
        }
    }
    record.character_properties_title_book = book
        .iter()
        .map(|id| CharacterPropertiesTitleBook {
            character_id: record.id,
            title_id: *id,
        })
        .collect();
    player.set_property(
        PropertyInt::NumCharacterTitles,
        i32::try_from(book.len()).map_err(|_| "too many titles")?,
    );
    match extras.titles.first().copied().or(created) {
        Some(shown) => {
            player.set_property(
                PropertyInt::CharacterTitleId,
                i32::try_from(shown).map_err(|_| "no such title")?,
            );
        }
        None => {
            player.try_remove_property(PropertyInt::CharacterTitleId);
        }
    }
    Ok(())
}

/// The title character creation gives a character of `heritage`: its first template's.
fn creation_title(w: &World, heritage: HeritageGroup) -> Option<u32> {
    let id = u32::try_from(heritage.0).ok()?;
    super::with_tables(w, |t| {
        t.chargen
            .heritage_groups
            .get(&id)?
            .templates
            .first()
            .map(|x| x.title)
            .filter(|title| *title != 0)
    })
}
