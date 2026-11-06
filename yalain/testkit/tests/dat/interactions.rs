//! The items' interaction tables as the server reads them from the September 2013 data: what a
//! hoard's key and a bell may be used on, from how far, and to what end.

use yalain_testkit::authored::Portal;

const IRON_KEY: u32 = 0x4700_12BA;
const IRON_STOCKPILE: u32 = 0x4700_12B5;
const BELL: u32 = 0x4700_1A3C;

/// The iron hoard's key may be used on its stockpile from 7 m, at once, used up, with its own text;
/// the bell on three dillos from 8 m, kept, with none.
/// Behaviour: server.places.item-used-on-an-object-at-once
#[test]
fn the_interaction_tables_read_as_the_data_defines_them() {
    let p = Portal::open();
    let t = p.interactions(&[IRON_KEY, BELL], &[]);
    let key = p.template(IRON_KEY);
    let on = t
        .between(&key.props, IRON_STOCKPILE)
        .expect("the stockpile");
    assert_eq!(on.targets, [IRON_STOCKPILE]);
    assert!((on.reach - 7.0).abs() < 1e-6);
    assert_eq!(on.behavior, None);
    assert!(on.uses_up);
    assert_eq!(on.message, Some((0x7F6B_F22A, 0x2500_1446)));
    let bell = p.template(BELL);
    let list = &t.tables[&bell
        .props
        .word("Usage_ItemInteractionTable")
        .expect("a table")];
    assert_eq!(list.len(), 3);
    assert!(list
        .iter()
        .all(|i| !i.uses_up && i.message.is_none() && (i.reach - 8.0).abs() < 1e-6));
    assert!(t.between(&bell.props, IRON_STOCKPILE).is_none());
}

const CAN: u32 = 0x4700_145F;
const REQUIESCAT: u32 = 0x4700_1C8D;
const DYE_PLANT: u32 = 0x4700_341D;
const AXE: u32 = 0x4700_0745;

/// The watering can's table names the dye plants by their class: a dye plant counts, a stockpile
/// does not; it names no behavior of its own (the plant's use behavior plays). The Requiescat's
/// names the weapons' class, an axe among them, and it is used up.
/// Behaviour: server.places.item-used-on-an-object-plays-a-behavior
#[test]
fn a_table_naming_a_class_counts_the_templates_of_that_class() {
    let p = Portal::open();
    let targets: Vec<(u32, u32)> = [DYE_PLANT, AXE, IRON_STOCKPILE]
        .iter()
        .map(|t| (*t, p.template(*t).description.package))
        .collect();
    let t = p.interactions(&[CAN, REQUIESCAT], &targets);
    let can = p.template(CAN);
    let on = t.between(&can.props, DYE_PLANT).expect("the dye plant");
    assert_eq!(on.behavior, None);
    assert!(!on.uses_up);
    assert!((on.reach - 4.0).abs() < 1e-6);
    assert!(t.between(&can.props, IRON_STOCKPILE).is_none());
    assert_eq!(
        p.template(DYE_PLANT)
            .props
            .word(yalain_world::usable::interactions::USER_BEHAVIOR),
        Some(yalain_world::usable::interactions::PLANT_BEHAVIOR)
    );
    let requiescat = p.template(REQUIESCAT);
    let on = t.between(&requiescat.props, AXE).expect("the axe");
    assert!(on.uses_up);
    assert!(t.between(&requiescat.props, DYE_PLANT).is_none());
    assert_eq!(
        requiescat
            .props
            .word(yalain_world::usable::interactions::USER_BEHAVIOR),
        Some(yalain_world::usable::interactions::REQUIESCAT_BEHAVIOR)
    );
}

/// The kinds of placed objects that answer a use as a person does are the script library's person
/// kind: an immobile person (the Diving Board), a kingdom stone, the Cauldron of Tanacha, a person
/// (the Orb of Insight); the faction shrines are a kind of their own, and portals and lifestones are
/// neither.
/// Behaviour: server.places.person-kinds-read
#[test]
fn person_kind_objects_and_faction_shrines_are_told_by_their_script_kinds() {
    use yalain_world::places::PERSONS;
    use yalain_world::usable::shrines::FACTION_SHRINES;
    let p = Portal::open();
    for t in [0x4700_33ED, 0x4700_3363, 0x4700_2F43, 0x4700_3266] {
        assert!(p.derives(t, &PERSONS), "{t:08X} a person");
        assert!(!p.derives(t, &FACTION_SHRINES), "{t:08X} no shrine");
    }
    for t in [0x4700_100D, 0x4700_100F, 0x4700_1010] {
        assert!(p.derives(t, &FACTION_SHRINES), "{t:08X} a shrine");
        assert!(!p.derives(t, &PERSONS), "{t:08X} no person");
    }
    for t in [0x4700_114D, 0x4700_113A] {
        assert!(
            !p.derives(t, &PERSONS) && !p.derives(t, &FACTION_SHRINES),
            "{t:08X}"
        );
    }
}

/// Each faction shrine joins the kingdom its script kind names: the placed shrines are the
/// Dominion's, the neutral kingdom's and the Order's, each of one kind alone.
/// Behaviour: server.kingdoms.shrine-kinds-read
#[test]
fn each_faction_shrine_joins_the_kingdom_its_script_kind_names() {
    use yalain_world::kingdoms::{kingdom, SHRINES};
    let p = Portal::open();
    let of = |t: u32| -> Vec<u32> {
        SHRINES
            .iter()
            .filter(|(kind, _)| p.derives(t, &[*kind]))
            .map(|(_, k)| *k)
            .collect()
    };
    assert_eq!(of(0x4700_100D), [kingdom::DOMINION]);
    assert_eq!(of(0x4700_100F), [kingdom::NEUTRAL]);
    assert_eq!(of(0x4700_1010), [kingdom::ORDER]);
    assert!(of(0x4700_33ED).is_empty(), "a person joins none");
}

/// The kinds of placed objects that cannot be used are the script library's forges and road
/// signs (the town signs and the gateway signs among them); persons, portals and lifestones are
/// none of them.
/// Behaviour: server.places.cannot-be-used-read
#[test]
fn forges_and_road_signs_are_told_by_their_script_kinds() {
    use yalain_world::places::{CANNOT_BE_USED, PERSONS};
    let p = Portal::open();
    // Forges, the Refining Workshops, a gateway sign, a road sign, a town sign.
    for t in [
        0x4700_0CD1,
        0x4700_0CD2,
        0x4700_0CD5,
        0x4700_0CD6,
        0x4700_0CD7,
        0x4700_1BE1,
        0x4700_1BC0,
        0x4700_1BEF,
    ] {
        assert!(p.derives(t, &CANNOT_BE_USED), "{t:08X} cannot be used");
        assert!(!p.derives(t, &PERSONS), "{t:08X} no person");
    }
    for t in [0x4700_33ED, 0x4700_114D, 0x4700_113A] {
        assert!(!p.derives(t, &CANNOT_BE_USED), "{t:08X}");
    }
}
