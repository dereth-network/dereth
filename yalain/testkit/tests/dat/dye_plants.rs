//! The dye plants' species as the September 2013 data defines them: each seed's planting and its
//! plant, the plant item a harvest gives, the pollen's table, and the ripe look read from the plant
//! item's look.

use yalain_testkit::authored::Portal;
use yalain_world::plants::{self, SPECIES};
use yalain_world::usable::interactions;

/// Each species' seed plays the planting and summons the dye plant of its color (its effect and
/// the plant's table name the same color); the plant plays the plants' behavior when used; the
/// plant item a harvest gives carries the plant's own name; each pollen may be used on its
/// species' plant, used up; the ripe look is the plant item's look, its second icon layer the
/// item's icon.
/// Behaviour: server.plants.species-from-the-install
#[test]
fn each_species_seed_plant_plant_item_pollen_and_ripe_look_agree_with_the_install() {
    let p = Portal::open();
    for sp in SPECIES {
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
        assert_eq!(item.description.name, plant.description.name);
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
