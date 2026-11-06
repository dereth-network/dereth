//! What the server's own rules make, as the September 2013 data holds it: a template the install
//! describes as what it's made for.

use yalain_testkit::authored::Portal;
use yalain_world::makes::by_the_server;
use yalain_world::{pets, plants, usable};

/// Every kind of pet is a creature in the install, at the level its summoning gives it.
/// Behaviour: server.makes.the-install-holds-what-the-server-makes
#[test]
fn every_kind_of_pet_is_a_creature_the_install_describes() {
    let p = Portal::open();
    for k in pets::fallback_kinds() {
        let mut props = p.template(k.template).props;
        props.set(
            yalain_content::props::creature_property::LEVEL,
            yalain_content::Prop::Int(20),
        );
        assert!(
            yalain_content::CreatureStats::from_props(&props).is_some(),
            "{:08X}",
            k.template
        );
    }
}

/// Every plant's seed sows it, every map node gives a material, and everything the server names
/// reads from the install.
/// Behaviour: server.makes.the-install-holds-what-the-server-makes
#[test]
fn every_seed_and_node_the_server_names_reads_from_the_install() {
    let p = Portal::open();
    for s in plants::SPECIES {
        assert!(plants::is_seed(s.seed));
        assert!(
            p.template(s.seed).props.flag("Item_IsStackable"),
            "{:08X}",
            s.seed
        );
    }
    for n in usable::maps::nodes() {
        assert!(p.template(n).props.word("Mine_Object").is_some(), "{n:08X}");
    }
    for t in by_the_server(&p.start_rules()) {
        assert_eq!(p.template(t).id, t);
    }
}
