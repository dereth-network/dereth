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
        .between(&key.props, IRON_STOCKPILE, 0)
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
    assert!(t.between(&bell.props, IRON_STOCKPILE, 0).is_none());
}

const SIGNIFICATOR: u32 = 0x4700_13D7;
const GATEWAY_APPRENTICE: u32 = 0x4700_13A9;
const TWIN_MOONS: u32 = 0x4700_13B5;
const ACE_OF_SWORDS: u32 = 0x4700_1375;

/// A significator may be used on each of the eighteen melds, from 3 m, kept, its outcome taking
/// the meld, with "Your significator passes through the gateway!"; a meld on a lesser trump is
/// kept and takes the trump.
/// Behaviour: server.interactions.an-outcome-that-destroys-its-target-takes-a-held-one
#[test]
fn a_significators_outcome_takes_the_meld_and_a_melds_the_lesser_trump() {
    let p = Portal::open();
    let t = p.interactions(&[SIGNIFICATOR, TWIN_MOONS], &[]);
    let significator = p.template(SIGNIFICATOR);
    let list = &t.tables[&significator
        .props
        .word("Usage_ItemInteractionTable")
        .expect("a table")];
    assert_eq!(list.len(), 18, "the melds");
    let on = t
        .between(&significator.props, GATEWAY_APPRENTICE, 0)
        .expect("a gateway");
    assert!(!on.uses_up && on.uses_up_target);
    assert!((on.reach - 3.0).abs() < 1e-6);
    assert_eq!(on.message, Some((0xBA0F_3786, 0x2500_1F24)));
    assert!(list.iter().all(|i| !i.uses_up && i.uses_up_target));
    let meld = p.template(TWIN_MOONS);
    let on = t
        .between(&meld.props, ACE_OF_SWORDS, 0)
        .expect("a lesser trump");
    assert!(!on.uses_up && on.uses_up_target);
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
    let on = t.between(&can.props, DYE_PLANT, 0).expect("the dye plant");
    assert_eq!(on.behavior, None);
    assert!(!on.uses_up);
    assert!((on.reach - 4.0).abs() < 1e-6);
    assert!(t.between(&can.props, IRON_STOCKPILE, 0).is_none());
    assert_eq!(
        p.template(DYE_PLANT)
            .props
            .word(yalain_world::usable::interactions::USER_BEHAVIOR),
        Some(yalain_world::usable::interactions::PLANT_BEHAVIOR)
    );
    let requiescat = p.template(REQUIESCAT);
    let on = t.between(&requiescat.props, AXE, 0).expect("the axe");
    assert!(on.uses_up);
    assert!(t.between(&requiescat.props, DYE_PLANT, 0).is_none());
    assert_eq!(
        requiescat
            .props
            .word(yalain_world::usable::interactions::USER_BEHAVIOR),
        Some(yalain_world::usable::interactions::REQUIESCAT_BEHAVIOR)
    );
}

const BLANK_EMBLEM: u32 = 0x4700_15BC;
const AXES: u32 = 4184;
const BOWS: u32 = 4205;

/// The blank emblem's table names the weapons' classes one by one: an axe counts in the axes' class
/// and not the bows', and the emblem used on it is used up with the axes' text.
/// Behaviour: server.interactions.an-item-used-on-another-makes-what-its-row-gives
#[test]
fn a_tables_classes_count_their_own_templates_alone() {
    let p = Portal::open();
    let targets = [(AXE, p.template(AXE).description.package)];
    let t = p.interactions(&[BLANK_EMBLEM], &targets);
    assert!(t.counts_in(AXES, AXE));
    assert!(!t.counts_in(BOWS, AXE));
    let on = t
        .between(&p.template(BLANK_EMBLEM).props, AXE, 0)
        .expect("the axe");
    assert!(on.uses_up);
    assert_eq!(on.message, Some((0x401B_5816, 0x2500_1FE8)));
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

/// The templates of the install's lifestone class are lifestones, the two placed ones that only
/// their class says so of among them; a real row's object binds as any does, and no other kind is
/// one.
/// Behaviour: server.places.lifestones-by-their-class
#[test]
fn a_placed_lifestone_of_the_lifestone_class_binds_its_user() {
    use yalain_testkit::play::{server_call, use_of};
    use yalain_world::places::{kind, LIFESTONE};
    use yalain_world::requests::function;
    let p = Portal::open();
    for t in [
        0x4700_1138,
        0x4700_113A,
        0x4700_113B,
        0x4700_113D,
        0x4700_113E,
        0x4700_1140,
        0x4700_1143,
        0x4700_1144,
    ] {
        assert!(p.derives(t, &[LIFESTONE]), "{t:08X} a lifestone");
    }
    for t in [0x4700_114D, 0x4700_33ED, 0x4700_100D] {
        assert!(!p.derives(t, &[LIFESTONE]), "{t:08X} no lifestone");
    }
    // As the world's rows describe them, standing beside the player.
    for t in [0x4700_1138, 0x4700_1140] {
        let template = p.template(t);
        const STONE: u64 = 0x404D_A200_0000_0003;
        let mut h = yalain_testkit::Harness::new();
        h.server
            .lifestone_packages
            .insert(template.description.package);
        h.server.content = yalain_content::ContentPack {
            objects: vec![yalain_content::ContentObject {
                object: Some(STONE),
                template: t,
                description: template.description.clone(),
                cell: 0x0299_0109,
                origin: [60.0, 241.0, -44.6],
                rotation: [1.0, 0.0, 0.0, 0.0],
                kind: yalain_content::Kind::Static,
                label: String::new(),
                placed: "as a world row places it".into(),
                destination: None,
                facts: template.facts,
                walks: None,
                gate: None,
            }],
            ..yalain_content::ContentPack::default()
        };
        let (a, me) = h.player_in_world("one", "Elara");
        h.run(10);
        h.bots[a].send(
            true,
            auberean_protocol::queue::WEENIE,
            &server_call(function::USE, &[use_of(STONE, me)]),
        );
        h.run(40);
        let bound = h
            .server
            .players
            .values()
            .next()
            .expect("in the world")
            .places
            .iter()
            .any(|(k, _)| *k == kind::LIFESTONE);
        assert!(bound, "{t:08X} binds");
    }
}

/// The six card melds, Deck of Fortune to Twin Moons.
const CARD_MELDS: [u32; 6] = [
    0x4700_13B0,
    0x4700_13B1,
    0x4700_13B2,
    0x4700_13B3,
    0x4700_13B4,
    0x4700_13B5,
];
/// The four suits' Aces (each suit's fourteen cards follow its Ace), in their tables' order, and
/// their classes: swords, scepters, skulls, drums.
const ACES: [(u32, u32); 4] = [
    (0x4700_1375, 10262),
    (0x4700_1359, 10260),
    (0x4700_1367, 10261),
    (0x4700_134B, 10259),
];
/// "You transform the lesser trump into an optimal fate card!" and "... into a fate card!"
const OPTIMAL: u32 = 0x3EBB_8F3B;
const PLAIN: u32 = 0xAF1E_F93B;

/// Each card meld's table names each of the 56 lesser trumps for the characters whose number leaves
/// its rank (Ace 0 to King 13) divided by 14, with the optimal text, then each suit's class for
/// anyone, with the plain one, every interaction keeping the meld and taking the trump: a
/// character of each of the fourteen numbers meets the optimal interaction on the trumps of its own
/// rank alone.
/// Behaviour: server.interactions.an-interaction-for-some-characters-only
#[test]
fn the_card_melds_tables_give_each_rank_of_lesser_trump_to_one_character_number_in_fourteen() {
    use yalain_world::usable::interactions::Gate;
    let p = Portal::open();
    let mut targets = Vec::new();
    for (ace, _) in ACES {
        for r in 0..14 {
            let t = ace + r;
            targets.push((t, p.template(t).description.package));
        }
    }
    let t = p.interactions(&CARD_MELDS, &targets);
    for meld in CARD_MELDS {
        let props = &p.template(meld).props;
        let list = &t.tables[&props.word("Usage_ItemInteractionTable").expect("a table")];
        assert_eq!(list.len(), 60, "{meld:08X}");
        for (i, (ace, class)) in ACES.iter().enumerate() {
            for r in 0..14u32 {
                let entry = &list[i * 14 + r as usize];
                assert_eq!(entry.targets, [ace + r], "{meld:08X}");
                assert_eq!(
                    entry.gate,
                    Gate::of(14, i32::try_from(r).expect("a rank")),
                    "{meld:08X}"
                );
                assert_eq!(entry.message.map(|m| m.0), Some(OPTIMAL));
            }
            assert!(t.counts_in(*class, *ace), "{meld:08X}");
        }
        for anyone in &list[56..] {
            assert_eq!(anyone.gate, None, "{meld:08X}");
            assert_eq!(anyone.message.map(|m| m.0), Some(PLAIN));
        }
        assert!(list.iter().all(|e| !e.uses_up && e.uses_up_target));
        for number in 0..14u64 {
            let character = yalain_world::login::PLAYER_ID_BASE | (1_000 + number);
            for (ace, _) in ACES {
                for r in 0..14u32 {
                    let met = t
                        .between(props, ace + r, character)
                        .expect("an interaction");
                    let own = (1_000 + number) % 14 == u64::from(r);
                    assert_eq!(
                        met.message.map(|m| m.0),
                        Some(if own { OPTIMAL } else { PLAIN }),
                        "{meld:08X} number {number} rank {r}"
                    );
                    assert_eq!(met.gate.is_some(), own);
                }
            }
        }
    }
}
