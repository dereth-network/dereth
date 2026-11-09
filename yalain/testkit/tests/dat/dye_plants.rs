//! The dye plants' species as the September 2013 data defines them: each seed's planting and its
//! plant, the plant item a harvest gives, the pollen's table, and the ripe look read from the plant
//! item's look. Fixture: the test world's plant rows.

use std::collections::BTreeMap;

use yalain_testkit::authored::Portal;
use yalain_world::plants;
use yalain_world::usable::interactions;

/// The install's dye plants, each with the seed whose planting summons it (its effect and the
/// plant's table name the same color) and the plant item of its name.
/// Behaviour: server.plants.plant-and-seed-read-from-the-install
#[test]
fn every_dye_plant_of_the_install_pairs_with_the_rows_seed_and_plant_item() {
    let p = Portal::open();
    let color = |t: u32, prop: &str, prefix: &str| {
        p.template(t)
            .props
            .text(prop)
            .and_then(|v| v.strip_prefix(prefix).map(str::to_string))
    };
    let seeds: BTreeMap<String, u32> = p
        .templates_of_class("MapTemplate")
        .into_iter()
        .filter_map(|t| Some((color(t, "Usage_Effect1", "Eff_SummonPetDyePlant")?, t)))
        .collect();
    let items: Vec<u32> = p.templates_of_class("DyePlantTemplate");
    let read: BTreeMap<u32, (u32, u32)> = p
        .templates_of_class("PetDyePlantTemplate")
        .into_iter()
        .map(|plant| {
            let c = color(plant, "AI_NPCTable", "NPCTable_PetDyePlant").expect("a plant's color");
            let seed = seeds
                .get(&c)
                .copied()
                .unwrap_or_else(|| panic!("{c}'s seed"));
            let name = p.template(plant).facts.name;
            assert!(name.is_some(), "{plant:08X} named");
            let named: Vec<u32> = items
                .iter()
                .copied()
                .filter(|i| p.template(*i).facts.name == name)
                .collect();
            assert_eq!(named.len(), 1, "{plant:08X}'s plant item");
            (plant, (seed, named[0]))
        })
        .collect();
    let rows: BTreeMap<u32, (u32, u32)> = yalain_testkit::test_plants()
        .species
        .values()
        .map(|s| (s.plant, (s.seed, s.harvest)))
        .collect();
    assert_eq!(read.len(), 13);
    assert_eq!(read, rows);
}

/// Each species' seed plays the planting and summons the dye plant of its color (its effect and
/// the plant's table name the same color); the plant plays the plants' behavior when used; the
/// plant item a harvest gives carries the plant's own name; each pollen may be used on its
/// species' plant, used up; the ripe look is the plant item's look, its second icon layer the
/// item's icon.
/// Behaviour: server.plants.species-from-the-install
#[test]
fn each_species_seed_plant_plant_item_pollen_and_ripe_look_agree_with_the_install() {
    let p = Portal::open();
    for sp in yalain_testkit::test_plants().species.into_values() {
        let seed = p.template(sp.seed);
        let plant = p.template(sp.plant);
        let color = seed
            .props
            .text("Usage_Effect1")
            .and_then(|e| e.strip_prefix("Eff_SummonPetDyePlant"))
            .expect("a seed summons a dye plant")
            .to_string();
        assert_eq!(
            plant.props.text("AI_NPCTable"),
            Some(format!("NPCTable_PetDyePlant{color}").as_str()),
            "{:08X}",
            sp.seed
        );
        assert_eq!(
            seed.props.word(interactions::USER_BEHAVIOR),
            Some(plants::PLANTING_BEHAVIOR)
        );
        assert_eq!(
            plant.props.word(interactions::USER_BEHAVIOR),
            Some(interactions::PLANT_BEHAVIOR)
        );
        let item = p.template(sp.harvest);
        assert!(plant.facts.name.is_some(), "{:08X} named", sp.plant);
        assert_eq!(
            item.facts.name, plant.facts.name,
            "{:08X}'s harvest",
            sp.plant
        );
        if let Some(pollen) = sp.pollen {
            let t = p.interactions(&[pollen], &[(sp.plant, plant.description.package)]);
            let on = t
                .between(&p.template(pollen).props, sp.plant)
                .expect("the pollen on its plant");
            assert!(on.uses_up);
        }
        let v: auberean_assets::visual::VisualDesc = auberean_assets::decode(
            item.description.look,
            &p.dat.read(item.description.look).expect("the item's look"),
            &p.ctx,
        )
        .expect("the look reads");
        let look = plants::look_of(&v);
        assert_eq!(
            look.bits, 0x380,
            "icon, the whole object's changes, its part groups"
        );
        assert_eq!(
            look.icon.get(1).map(|l| l.1),
            item.props.word("Icon"),
            "the item's icon"
        );
        assert_eq!(look.part_groups.entries.len(), 1);
    }
}
