//! A library of another world's rows: the items a list names, and everything they reach, taken
//! from a whole world into a world of their own.
//!
//! A list names one item a line (`#` starts a comment):
//!
//! ```text
//! weenie 509              a weenie, by class id
//! weenie lifestone        or by class name
//! spell 1                 a spell; `spell all` every spell
//! death-treasure 1        a death treasure type; `death-treasure all` every one
//! wielded-treasure 3      a wielded treasure type, its whole list; `wielded-treasure all`
//! quest PathwardenComplete  a quest, by name (without case); `quest all`
//! landblock 01AC          a landblock's placements, with the guids they link
//! ```
//!
//! What an item reaches is taken with it, and what that reaches, until nothing new is reached:
//! - a weenie's create list (a vendor's stock among it), its generator table, the classes its
//!   emotes give or create, and the class it makes when used (`UseCreateItem`,
//!   `AugmentationCreateItem`);
//! - the spells it knows, casts from its emotes, or carries (`Spell`, `ProcSpell`, `DeathSpell`,
//!   the surge spells);
//! - its death, inventory, shop and Olthoi treasure types (`death-treasure`) and its wielded
//!   treasure type, with the classes that list names;
//! - the quests its emotes name, where the world has them;
//! - a placement's class and the placements it links.
//!
//! The treasure tables every treasure type draws on (gem counts and materials) come whole with
//! the first treasure type taken.

use std::collections::{BTreeMap, BTreeSet};

use empyrean_content::WorldContent;
use empyrean_entity::enums::PropertyDataId;

use crate::value::R;

/// What a list names: each kind's items, or all of the kind.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub weenies: BTreeSet<u32>,
    /// Weenies named by class name, resolved against the world.
    pub weenie_names: BTreeSet<String>,
    pub spells: Pick<u32>,
    pub death_treasure: Pick<u32>,
    pub wielded_treasure: Pick<u32>,
    /// Quests by name, lowercased.
    pub quests: Pick<String>,
    pub landblocks: BTreeSet<u16>,
}

/// Some items of a kind, or all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick<T: Ord> {
    All,
    These(BTreeSet<T>),
}

impl<T: Ord> Default for Pick<T> {
    fn default() -> Self {
        Self::These(BTreeSet::new())
    }
}

impl<T: Ord + Clone> Pick<T> {
    fn add(&mut self, v: T) {
        if let Self::These(s) = self {
            s.insert(v);
        }
    }

    fn has(&self, v: &T) -> bool {
        match self {
            Self::All => true,
            Self::These(s) => s.contains(v),
        }
    }
}

/// The list `text` names.
///
/// # Errors
/// A line that names no item this list knows, with its line number.
pub fn parse(text: &str) -> R<Wanted> {
    let mut w = Wanted::default();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let bad = || format!("line {}: `{line}` names no item", n + 1);
        let (head, rest) = line.split_once(char::is_whitespace).ok_or_else(bad)?;
        let rest = rest.trim();
        let all = rest.eq_ignore_ascii_case("all");
        let num = || crate::layers::word_num(rest).ok_or_else(bad);
        match head {
            "weenie" => match crate::layers::word_num(rest) {
                Some(id) => {
                    w.weenies.insert(id);
                }
                None => {
                    w.weenie_names.insert(rest.to_owned());
                }
            },
            "spell" if all => w.spells = Pick::All,
            "spell" => w.spells.add(num()?),
            "death-treasure" if all => w.death_treasure = Pick::All,
            "death-treasure" => w.death_treasure.add(num()?),
            "wielded-treasure" if all => w.wielded_treasure = Pick::All,
            "wielded-treasure" => w.wielded_treasure.add(num()?),
            "quest" if all => w.quests = Pick::All,
            "quest" if !rest.is_empty() => w.quests.add(rest.to_lowercase()),
            "landblock" => {
                let lb =
                    u16::from_str_radix(rest.trim_start_matches("0x"), 16).map_err(|_| bad())?;
                w.landblocks.insert(lb);
            }
            _ => return Err(bad()),
        }
    }
    Ok(w)
}

/// What [`select`] took, by kind, and the names of the list it found nothing for.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Taken {
    pub weenies: usize,
    /// Of them, the ones the list named itself.
    pub named: usize,
    pub spells: usize,
    pub death_treasure: usize,
    pub wielded_treasure: usize,
    pub quests: usize,
    pub placements: usize,
    /// What the list or a reach names that the world doesn't have.
    pub missing: Vec<String>,
}

/// The items `wanted` names in `world`, and everything they reach, as a world of their own in
/// `world`'s order and era.
#[must_use]
#[allow(clippy::too_many_lines)] // one reach per kind of reference
pub fn select(world: &WorldContent, wanted: &Wanted) -> (WorldContent, Taken) {
    let mut taken = Taken::default();
    let by_class: BTreeMap<u32, usize> = world
        .weenies
        .iter()
        .enumerate()
        .map(|(i, w)| (w.class_id, i))
        .collect();
    let by_name: BTreeMap<String, u32> = world
        .weenies
        .iter()
        .map(|w| (w.class_name.to_lowercase(), w.class_id))
        .collect();
    let spells_have: BTreeSet<u32> = world.spells.iter().map(|s| s.id).collect();
    let quests_have: BTreeMap<String, &str> = world
        .quests
        .iter()
        .map(|q| (q.name.to_lowercase(), q.name.as_str()))
        .collect();

    let mut classes: BTreeSet<u32> = BTreeSet::new();
    let mut todo: Vec<u32> = Vec::new();
    let mut spells = wanted.spells.clone();
    let mut death = wanted.death_treasure.clone();
    let mut wielded = wanted.wielded_treasure.clone();
    let mut quests = wanted.quests.clone();
    let mut missing: BTreeSet<String> = BTreeSet::new();

    for id in &wanted.weenies {
        todo.push(*id);
    }
    for name in &wanted.weenie_names {
        match by_name.get(&name.to_lowercase()) {
            Some(id) => todo.push(*id),
            None => {
                missing.insert(format!("weenie {name}"));
            }
        }
    }
    let placements: Vec<usize> = {
        let picked: BTreeSet<u32> = world
            .landblock_instances
            .iter()
            .filter(|i| wanted.landblocks.contains(&((i.obj_cell_id >> 16) as u16)))
            .flat_map(|i| {
                std::iter::once(i.guid)
                    .chain(i.landblock_instance_link.iter().map(|l| l.child_guid))
            })
            .collect();
        world
            .landblock_instances
            .iter()
            .enumerate()
            .filter(|(_, i)| picked.contains(&i.guid))
            .map(|(n, i)| {
                todo.push(i.weenie_class_id);
                n
            })
            .collect()
    };
    taken.named = todo.iter().collect::<BTreeSet<_>>().len();

    // Wielded treasure lists name classes, so their reach is part of the weenies'.
    let mut wielded_done: BTreeSet<u32> = BTreeSet::new();
    loop {
        if let Pick::All = wielded {
            todo.extend(world.treasure_wielded.iter().map(|t| t.weenie_class_id));
            wielded_done.extend(world.treasure_wielded.iter().map(|t| t.treasure_type));
        }
        while let Some(id) = todo.pop() {
            if id == 0 || !classes.insert(id) {
                continue;
            }
            let Some(&i) = by_class.get(&id) else {
                missing.insert(format!("weenie {id}"));
                continue;
            };
            let w = &world.weenies[i];
            todo.extend(
                w.weenie_properties_create_list
                    .iter()
                    .map(|r| r.weenie_class_id),
            );
            todo.extend(
                w.weenie_properties_generator
                    .iter()
                    .map(|r| r.weenie_class_id),
            );
            for e in &w.weenie_properties_emote {
                todo.extend(e.weenie_class_id);
                for a in &e.weenie_properties_emote_action {
                    todo.extend(a.weenie_class_id);
                    if let Some(s) = a.spell_id.and_then(|s| u32::try_from(s).ok()) {
                        if s != 0 {
                            spells.add(s);
                        }
                    }
                    if let Some(name) = a.message.as_deref() {
                        if quests_have.contains_key(&name.to_lowercase()) {
                            quests.add(name.to_lowercase());
                        }
                    }
                }
            }
            for s in &w.weenie_properties_spell_book {
                if let Ok(s) = u32::try_from(s.spell) {
                    spells.add(s);
                }
            }
            for d in &w.weenie_properties_did {
                let t = PropertyDataId(d.r#type);
                if d.value == 0 {
                    continue;
                }
                if [
                    PropertyDataId::Spell,
                    PropertyDataId::ProcSpell,
                    PropertyDataId::DeathSpell,
                    PropertyDataId::BlueSurgeSpell,
                    PropertyDataId::YellowSurgeSpell,
                    PropertyDataId::RedSurgeSpell,
                ]
                .contains(&t)
                {
                    spells.add(d.value);
                } else if [
                    PropertyDataId::DeathTreasureType,
                    PropertyDataId::InventoryTreasureType,
                    PropertyDataId::ShopTreasureType,
                    PropertyDataId::OlthoiDeathTreasureType,
                ]
                .contains(&t)
                {
                    death.add(d.value);
                } else if t == PropertyDataId::WieldedTreasureType {
                    wielded.add(d.value);
                } else if [
                    PropertyDataId::UseCreateItem,
                    PropertyDataId::AugmentationCreateItem,
                ]
                .contains(&t)
                {
                    todo.push(d.value);
                }
            }
        }
        // Every wielded type now wanted, not yet taken, adds its classes.
        let mut grew = false;
        for t in &world.treasure_wielded {
            if wielded.has(&t.treasure_type) && !wielded_done.contains(&t.treasure_type) {
                todo.push(t.weenie_class_id);
                grew = true;
            }
        }
        wielded_done.extend(
            world
                .treasure_wielded
                .iter()
                .filter(|t| wielded.has(&t.treasure_type))
                .map(|t| t.treasure_type),
        );
        if !grew && todo.is_empty() {
            break;
        }
    }

    let mut out = WorldContent {
        era: world.era,
        ..WorldContent::default()
    };
    out.weenies = world
        .weenies
        .iter()
        .filter(|w| classes.contains(&w.class_id))
        .cloned()
        .collect();
    out.spells = world
        .spells
        .iter()
        .filter(|s| spells.has(&s.id))
        .cloned()
        .collect();
    if let Pick::These(s) = &spells {
        missing.extend(
            s.iter()
                .filter(|id| !spells_have.contains(id))
                .map(|id| format!("spell {id}")),
        );
    }
    out.treasure_death = world
        .treasure_death
        .iter()
        .filter(|t| death.has(&t.treasure_type))
        .cloned()
        .collect();
    out.treasure_wielded = world
        .treasure_wielded
        .iter()
        .filter(|t| wielded.has(&t.treasure_type))
        .cloned()
        .collect();
    if !out.treasure_death.is_empty() || !out.treasure_wielded.is_empty() {
        out.treasure_gem_count.clone_from(&world.treasure_gem_count);
        out.treasure_material_base
            .clone_from(&world.treasure_material_base);
        out.treasure_material_color
            .clone_from(&world.treasure_material_color);
        out.treasure_material_groups
            .clone_from(&world.treasure_material_groups);
    }
    out.quests = world
        .quests
        .iter()
        .filter(|q| quests.has(&q.name.to_lowercase()))
        .cloned()
        .collect();
    if let Pick::These(q) = &wanted.quests {
        missing.extend(
            q.iter()
                .filter(|n| !quests_have.contains_key(*n))
                .map(|n| format!("quest {n}")),
        );
    }
    out.landblock_instances = placements
        .iter()
        .map(|&n| world.landblock_instances[n].clone())
        .collect();

    taken.weenies = out.weenies.len();
    taken.spells = out.spells.len();
    taken.death_treasure = out
        .treasure_death
        .iter()
        .map(|t| t.treasure_type)
        .collect::<BTreeSet<_>>()
        .len();
    taken.wielded_treasure = out
        .treasure_wielded
        .iter()
        .map(|t| t.treasure_type)
        .collect::<BTreeSet<_>>()
        .len();
    taken.quests = out.quests.len();
    taken.placements = out.landblock_instances.len();
    taken.missing = missing.into_iter().collect();
    (out, taken)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a library of another world's rows, the items a list names and
    //! what they reach)

    use empyrean_content::models::world::{
        Quest, Spell, TreasureDeath, TreasureGemCount, TreasureWielded, Weenie,
        WeeniePropertiesCreateList, WeeniePropertiesDID, WeeniePropertiesEmote,
        WeeniePropertiesEmoteAction, WeeniePropertiesGenerator,
    };
    use empyrean_content::WorldContent;
    use empyrean_entity::enums::PropertyDataId;

    use super::{parse, select, Pick};

    fn weenie(class_id: u32, name: &str) -> Weenie {
        Weenie {
            class_id,
            class_name: name.into(),
            ..Weenie::default()
        }
    }

    fn did(t: PropertyDataId, value: u32) -> WeeniePropertiesDID {
        WeeniePropertiesDID {
            r#type: t.0,
            value,
            ..WeeniePropertiesDID::default()
        }
    }

    /// A vendor selling a sword, a generator of a creature that wields from a treasure list and
    /// gives a quest's token, and things nothing names.
    fn world() -> WorldContent {
        let mut vendor = weenie(10, "vendor");
        vendor
            .weenie_properties_create_list
            .push(WeeniePropertiesCreateList {
                destination_type: 4,
                weenie_class_id: 11,
                ..WeeniePropertiesCreateList::default()
            });
        let mut generator = weenie(20, "camp");
        generator
            .weenie_properties_generator
            .push(WeeniePropertiesGenerator {
                weenie_class_id: 21,
                ..WeeniePropertiesGenerator::default()
            });
        let mut creature = weenie(21, "drudge");
        creature.weenie_properties_did = vec![
            did(PropertyDataId::DeathTreasureType, 7),
            did(PropertyDataId::WieldedTreasureType, 8),
        ];
        creature
            .weenie_properties_emote
            .push(WeeniePropertiesEmote {
                weenie_properties_emote_action: vec![WeeniePropertiesEmoteAction {
                    weenie_class_id: Some(23),
                    spell_id: Some(5),
                    message: Some("TokenGiven".into()),
                    ..WeeniePropertiesEmoteAction::default()
                }],
                ..WeeniePropertiesEmote::default()
            });
        WorldContent {
            weenies: vec![
                vendor,
                weenie(11, "sword"),
                weenie(12, "unnamed"),
                generator,
                creature,
                weenie(22, "club"),
                weenie(23, "token"),
            ],
            spells: vec![
                Spell {
                    id: 5,
                    ..Spell::default()
                },
                Spell {
                    id: 6,
                    ..Spell::default()
                },
            ],
            quests: vec![
                Quest {
                    name: "TokenGiven".into(),
                    ..Quest::default()
                },
                Quest {
                    name: "Other".into(),
                    ..Quest::default()
                },
            ],
            treasure_death: vec![
                TreasureDeath {
                    treasure_type: 7,
                    ..TreasureDeath::default()
                },
                TreasureDeath {
                    treasure_type: 9,
                    ..TreasureDeath::default()
                },
            ],
            treasure_wielded: vec![TreasureWielded {
                treasure_type: 8,
                weenie_class_id: 22,
                ..TreasureWielded::default()
            }],
            treasure_gem_count: vec![TreasureGemCount::default()],
            ..WorldContent::default()
        }
    }

    fn classes(c: &WorldContent) -> Vec<u32> {
        c.weenies.iter().map(|w| w.class_id).collect()
    }

    #[test]
    fn a_named_weenie_brings_what_it_sells_spawns_wields_drops_casts_and_gives() {
        let wanted = parse("# the town\nweenie vendor\nweenie 20\n").expect("reads");
        let (out, taken) = select(&world(), &wanted);
        assert_eq!(classes(&out), [10, 11, 20, 21, 22, 23]);
        assert_eq!(out.spells.iter().map(|s| s.id).collect::<Vec<_>>(), [5]);
        assert_eq!(out.quests.len(), 1);
        assert_eq!(out.treasure_death.len(), 1);
        assert_eq!(out.treasure_death[0].treasure_type, 7);
        assert_eq!(out.treasure_wielded.len(), 1);
        // The treasure tables come whole with the first treasure type.
        assert_eq!(out.treasure_gem_count.len(), 1);
        assert_eq!(taken.named, 2);
        assert!(taken.missing.is_empty());
    }

    #[test]
    fn all_of_a_kind_is_taken_whole_and_what_the_world_lacks_is_listed() {
        let wanted =
            parse("spell all\ndeath-treasure all\nweenie 99\nweenie nothing\nquest gone\n")
                .expect("reads");
        assert_eq!(wanted.spells, Pick::All);
        let (out, taken) = select(&world(), &wanted);
        assert_eq!(out.spells.len(), 2);
        assert_eq!(out.treasure_death.len(), 2);
        assert!(out.weenies.is_empty());
        assert_eq!(taken.missing, ["quest gone", "weenie 99", "weenie nothing"]);
    }

    #[test]
    fn a_line_that_names_no_item_is_refused_with_its_number() {
        let e = parse("weenie 1\nsomething 2\n").unwrap_err();
        assert!(e.starts_with("line 2:"), "{e}");
        assert!(parse("spell x").is_err());
    }
}
