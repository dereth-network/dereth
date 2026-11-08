//! What a world's start asks the install to describe: every kind of pet, plant and map node the
//! server's rules make, and what the world's rows make. Fixture: tables written here.

use std::collections::BTreeMap;

use yalain_content::quests::{Give, QuestTable};
use yalain_content::{CraftStart, DeathSpawn, StoreTable};
use yalain_world::makes::{by_a_template, by_the_rows, by_the_server, Rows};
use yalain_world::{pets, plants, usable};

/// The server's own list names every kind of pet, whatever makes it, every dye plant with its
/// seed, pollen and harvest, and every node a map conjures.
/// Behaviour: server.makes.the-server-names-what-it-makes
#[test]
fn the_server_names_every_pet_plant_and_map_node_it_makes() {
    let named = by_the_server(&yalain_testkit::test_start());
    for k in pets::fallback_kinds() {
        assert!(named.contains(&k.template), "{:08X}", k.template);
    }
    for p in plants::SPECIES {
        for t in [p.plant, p.seed, p.harvest].into_iter().chain(p.pollen) {
            assert!(named.contains(&t), "{t:08X}");
        }
    }
    assert_eq!(usable::maps::nodes().count(), 15);
    for n in usable::maps::nodes() {
        assert!(named.contains(&n), "{n:08X}");
    }
}

/// A map's effect only names nodes the list holds.
/// Behaviour: server.makes.the-server-names-what-it-makes
#[test]
fn every_node_a_maps_effect_names_is_among_the_nodes() {
    for region in ["Linvak", "Omishan", "Osteth"] {
        for material in ["Crystal", "Iron", "Silver", "Stone", "Wood"] {
            let node = usable::maps::summoned_by(&format!("Eff_SummonMine_{material}_{region}"));
            assert!(
                usable::maps::nodes().any(|n| Some(n) == node),
                "{material} {region}"
            );
        }
    }
}

/// A give row's gift and the item it takes, and what a death or a use makes by a row, are asked for
/// with the rows' other makers.
/// Behaviour: server.makes.the-rows-name-what-they-make
#[test]
fn a_give_rows_gift_and_a_death_spawn_are_named_with_the_other_rows() {
    const PET: u32 = 0x4700_0862;
    const GIFT: u32 = 0x4700_10DA;
    const TAKEN: u32 = 0x4700_1775;
    const DUMMY: u32 = 0x4700_2DAE;
    let quests = QuestTable {
        gives: vec![Give {
            to: 0x4700_3009,
            item: Some(TAKEN),
            wants: None,
            gives: vec![GIFT],
            counts: Vec::new(),
            effects: Vec::new(),
            text: None,
            fx: None,
            requires: Vec::new(),
            placed: "authored for a test".into(),
        }],
        ..QuestTable::default()
    };
    let mut death_spawns = BTreeMap::new();
    death_spawns.insert(
        "Eff_Summons".to_owned(),
        DeathSpawn {
            template: DUMMY,
            count: 1,
            chance: 1.0,
            offset: [0.0, 0.0],
            lasts: None,
        },
    );
    let named = by_the_rows(Rows {
        objects: &[],
        stores: &StoreTable::default(),
        quests: &quests,
        crafting: &CraftStart::default(),
        death_spawns: &death_spawns,
        pets: &std::collections::BTreeMap::from([(
            0x7000_092A,
            yalain_world::pets::PetKind::new(0x7000_092A, PET, 1096, &[]),
        )]),
    });
    for t in [GIFT, TAKEN, DUMMY, PET] {
        assert!(named.contains(&t), "{t:08X}");
    }
}

/// A node described brings what it gives with it.
/// Behaviour: server.makes.the-rows-name-what-they-make
#[test]
fn a_node_described_asks_for_what_it_gives() {
    let mut props = yalain_content::Props::default();
    props.set("Mine_Object", yalain_content::Prop::Word(0x4700_0B31));
    assert_eq!(by_a_template(&props), [0x4700_0B31]);
}
