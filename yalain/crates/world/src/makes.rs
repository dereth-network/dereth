//! The templates a world makes things of. A template is described at start only when something asks
//! for it, and what isn't described can't be made: a summoning, a seed, a map or a gift of it
//! comes to nothing. So everything that makes things by template is named here, the server's own
//! rules and the world's rows, and a start describes all of it.
//!
//! A row or a rule that makes a thing of a template goes in this list too. Tests build their own
//! catalogs, so they don't show one left out; the world's real start is what they ask of.
//!
//! What could still fail to be described (a world database or a dat overlay of someone's own) takes
//! nothing from the player: a use, a give, a step or a recipe that would make what can't be made is
//! refused and uses up nothing. [`can_make`] is what they ask.

use std::collections::BTreeMap;

use yalain_content::quests::Trigger;
use yalain_content::{ContentObject, CraftStart, DeathSpawn, Props, QuestTable, StoreTable};

use yalain_store::Store;

use crate::{death, inventory, mounts, pets, plants, usable, Server};

/// Can this world make a thing of this template?
#[must_use]
pub fn can_make<S: Store>(s: &Server<S>, template: u32) -> bool {
    s.catalog.get(template).is_some()
}

/// What the server's own rules make, whatever the world holds: the corpse, the races' starting
/// packs and the book a new character is given, the mounts, a summoned portal, every kind of pet,
/// the dye plants and what they give, and the nodes maps conjure.
#[must_use]
pub fn by_the_server(start: &crate::StartRules) -> Vec<u32> {
    let starting = start.items.values().flatten().copied();
    let plants = plants::SPECIES
        .iter()
        .flat_map(|p| [p.plant, p.seed, p.harvest].into_iter().chain(p.pollen));
    starting
        .chain([death::CORPSE_TEMPLATE, inventory::GUIDE_BOOK])
        .chain(mounts::ITEMS)
        .chain([usable::portals::TEMPLATE])
        .chain(pets::fallback_kinds().iter().map(|k| k.template))
        .chain(plants)
        .chain(usable::maps::nodes())
        .collect()
}

/// The property naming what a node gives.
const NODE_GIVES: &str = "Mine_Object";

/// A world's rows, as far as they make things.
#[derive(Debug, Clone, Copy)]
pub struct Rows<'a> {
    pub objects: &'a [ContentObject],
    pub stores: &'a StoreTable,
    pub quests: &'a QuestTable,
    pub crafting: &'a CraftStart,
    pub death_spawns: &'a BTreeMap<String, DeathSpawn>,
    pub pets: &'a BTreeMap<u32, yalain_content::PetKind>,
}

/// What a world's rows make or hand on: placed objects (a chest's lock and contents, a door's), what
/// stores sell, what quest steps and people give, what is taken up for a step and what's
/// handed in for a gift, what butchery yields and what a death makes. Camps and loot are the
/// catalog's own, and the recipes' products and materials [`crate::craft::Crafting::templates`].
#[must_use]
pub fn by_the_rows(rows: Rows<'_>) -> Vec<u32> {
    let steps = rows.quests.steps.iter();
    let gifts = rows.quests.gives.iter();
    rows.objects
        .iter()
        .map(|o| o.template)
        .chain(rows.pets.values().map(|k| k.template))
        .chain(rows.stores.sales.iter().map(|s| s.product))
        .chain(steps.clone().flat_map(|s| s.gives.iter().copied()))
        .chain(steps.filter_map(|s| match s.trigger {
            Trigger::Take(t) => Some(t),
            _ => None,
        }))
        .chain(gifts.flat_map(|g| g.gives.iter().copied().chain(g.item)))
        .chain(
            rows.crafting
                .butchery
                .iter()
                .flat_map(|b| b.items.iter().copied()),
        )
        .chain(rows.death_spawns.values().map(|d| d.template))
        .collect()
}

/// What a described template names in its turn: what a creature's body yields, what a node gives.
#[must_use]
pub fn by_a_template(props: &Props) -> Vec<u32> {
    let trophies = yalain_content::CreatureStats::from_props(props)
        .map_or_else(Vec::new, |s| s.trophies.iter().map(|(t, _)| *t).collect());
    trophies.into_iter().chain(props.word(NODE_GIVES)).collect()
}
