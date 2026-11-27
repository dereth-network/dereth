//! Behaviour: none (each test names its own row)

use super::*;
use crate::travel::StoreFiles;
use dereth_primitives::{CellId, Frame, ObjectId, Quat, Vec3};

fn at(cell: u32, x: f32, y: f32) -> Position {
    Position::new(
        CellId(cell),
        Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
    )
}

/// A client with a store of its own on disk, in `world`.
fn travel_in(dir: &std::path::Path) -> Travel {
    let mut t = Travel::new(StoreFiles {
        dir: Some(dir.to_path_buf()),
        tiles: None,
        hints: None,
        prefix: String::from("navigation-"),
    });
    t.note_world("Recalls@127.0.0.1:9000");
    t
}

fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "dereth-recalls-{name}-{}-{}",
        std::process::id(),
        unix_now()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    dir
}

fn portal_seen(x: f32) -> PlaceSeen {
    PlaceSeen {
        id: ObjectId(7),
        bitfield: 0x0004_0000,
        item_type: 0x0001_0000,
        wcid: 1234,
        name: String::from("Portal to Somewhere"),
        at: Some(at(0xA9B4_0011, x, 10.0)),
        useability: Some(0x0002_0000),
        physics_state: 0,
    }
}

fn lifestone_seen(x: f32) -> PlaceSeen {
    PlaceSeen {
        id: ObjectId(8),
        bitfield: 0x4014,
        item_type: 0x1000_0000,
        wcid: 509,
        name: String::from("Life Stone"),
        at: Some(at(0xA9B4_0011, x, 10.0)),
        useability: None,
        physics_state: 0,
    }
}

/// A recall the player sets off by hand (the lifestone command, here) is learned where it lands
/// the character, once a teleport has taken the body more than a few metres; one that never
/// leaves, or whose time runs out, teaches nothing. What is learned is kept for the character
/// across a restart, and only for that character.
///
/// Behaviour: plugins.nav.a-characters-recalls-are-learned-where-they-land
#[test]
fn a_recall_used_by_hand_is_learned_where_it_lands_and_kept_for_the_character() {
    let dir = folder("by-hand");
    let mut t = travel_in(&dir);
    let home = at(0xA9B4_0011, 20.0, 20.0);
    let far = at(0x7D64_0021, 50.0, 60.0);
    t.recall_used(Recall::Lifestone, 1.0);
    t.body_at(&home, false, 2.0, "Tester");
    t.body_at(&home, true, 5.0, "Tester");
    t.body_at(&far, false, 9.0, "Tester");
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Lifestone),
        Some(far)
    );
    // The house command, never leaving: nothing learned.
    t.recall_used(Recall::House, 20.0);
    t.body_at(&far, true, 21.0, "Tester");
    t.body_at(&at(0x7D64_0021, 52.0, 60.0), false, 24.0, "Tester");
    assert_eq!(t.portals.recall_destination("Tester", Recall::House), None);
    // Set off too long before a teleport: not its landing.
    t.recall_used(Recall::Marketplace, 30.0);
    t.body_at(&far, true, 30.0 + RecallWatch::WAIT + 1.0, "Tester");
    t.body_at(&home, false, 70.0, "Tester");
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Marketplace),
        None
    );
    // A restart: kept, for the character alone.
    let again = travel_in(&dir);
    assert_eq!(
        again
            .portals
            .recall_destination("Tester", Recall::Lifestone),
        Some(far)
    );
    assert_eq!(
        again.portals.recall_destination("Other", Recall::Lifestone),
        None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A tie's recall is learned the first time the character uses it, where it lands, like any
/// other recall. Using a lifestone sets the lifestone command's landing to where the character
/// stands when the lifestone speaks to it; a word from it too late, or with the character no
/// longer beside it, sets nothing.
///
/// Behaviour: plugins.nav.a-characters-recalls-are-learned-where-they-land
#[test]
fn a_ties_recall_is_learned_by_use_and_a_lifestone_sets_where_its_recall_lands() {
    let dir = folder("ties");
    let mut t = travel_in(&dir);
    let home = at(0xA9B4_0011, 20.0, 20.0);
    let tied = at(0x8588_002C, 120.0, 95.0);
    // Primary Portal Recall, cast for the first time.
    t.recall_used(Recall::Spell(48), 1.0);
    t.body_at(&home, true, 4.0, "Tester");
    t.body_at(&tied, false, 8.0, "Tester");
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Spell(48)),
        Some(tied)
    );
    // Using a lifestone: the lifestone command lands where the character stood as it spoke.
    let stone = at(0xA9B4_0011, 40.0, 10.0);
    let beside = at(0xA9B4_0011, 41.5, 11.0);
    let away = at(0xA9B4_0011, 70.0, 11.0);
    // Its word late: nothing.
    t.lifestone_used(stone, 20.0);
    t.server_lines(
        &[7],
        Some(&beside),
        20.0 + RecallWatch::ANSWER + 1.0,
        "Tester",
    );
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Lifestone),
        None
    );
    // The character walked away before it spoke: nothing.
    t.lifestone_used(stone, 40.0);
    t.server_lines(&[7], Some(&away), 41.0, "Tester");
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Lifestone),
        None
    );
    // A line of another kind does not answer it; the lifestone's own word does.
    t.lifestone_used(stone, 60.0);
    t.server_lines(&[0], Some(&beside), 61.0, "Tester");
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Lifestone),
        None
    );
    t.server_lines(&[7], Some(&beside), 63.0, "Tester");
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Lifestone),
        Some(beside)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Tying to a portal whose destination the store knows sets the tie's recall to land there once
/// the server's code says the tie took; tying to a lifestone sets Lifestone Recall to land at it,
/// on the lifestone's own code; a tie answered by another code, or too late, sets nothing.
///
/// Behaviour: plugins.nav.a-characters-recalls-are-learned-where-they-land
#[test]
fn a_tie_sets_its_recalls_landing_once_the_server_says_it_took() {
    let dir = folder("tied");
    let mut t = travel_in(&dir);
    let portal = portal_seen(30.0);
    let beyond = at(0x8588_002C, 120.0, 95.0);
    t.portals.learn_use(
        &dereth_navigation::PortalSighting {
            wcid: Some(portal.wcid),
            name: portal.name.clone(),
            position: portal.at.expect("placed"),
            shape: None,
        },
        &beyond,
        1,
    );
    // Primary Portal Tie at it: the lifestone's code does not answer it, the portal's does.
    t.spell_cast_at(47, &portal, 10.0);
    t.server_errors(&[TOOK_LIFESTONE], 11.0, "Tester");
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Spell(48)),
        None
    );
    t.server_errors(&[TOOK_PORTAL], 12.0, "Tester");
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Spell(48)),
        Some(beyond)
    );
    // Secondary Portal Tie at it, answered too late: nothing.
    t.spell_cast_at(2646, &portal, 20.0);
    t.server_errors(&[TOOK_PORTAL], 20.0 + RecallWatch::ANSWER + 1.0, "Tester");
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Spell(2647)),
        None
    );
    // Lifestone Tie at a lifestone.
    let stone = lifestone_seen(40.0);
    t.spell_cast_at(2644, &stone, 50.0);
    t.server_errors(&[TOOK_LIFESTONE], 52.0, "Tester");
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Spell(1635)),
        stone.at
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The Marketplace recall lands every character at the Marketplace: where this world's places
/// know the Marketplace, a character who has never used the recall is planned to land there.
///
/// Behaviour: plugins.nav.a-characters-recalls-are-learned-where-they-land
#[test]
fn the_marketplace_recall_lands_at_the_marketplace_this_worlds_places_know() {
    let dir = folder("marketplace");
    let mut t = travel_in(&dir);
    t.character.name = String::from("Tester");
    t.marketplace_from_places();
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Marketplace),
        None
    );
    let market = at(0x016C_01BC, 49.206, -31.935);
    // The era's places, as the launcher hands them over, name it.
    t.take_hints(concat!(
        "dereth-places-store\t1\n",
        "world\tDereth\n",
        "place\tlandmark\tpublished\t-\tMarketplace\t0x016C01BC 49.206 -31.935 0.000\t0\t0\t-\t-\n",
    ))
    .expect("a place package");
    t.marketplace_from_places();
    assert_eq!(
        t.portals.recall_destination("Tester", Recall::Marketplace),
        Some(market)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A recall gem whose spell sends to a fixed place is planned with before the character has used
/// it, landing where the era's places say the spell sends; the server's places say it over the
/// era's, and what the character learned says it over both.
///
/// Behaviour: plugins.nav.a-characters-recalls-are-learned-where-they-land
#[test]
fn a_recall_gem_never_used_is_planned_to_land_where_the_places_say_its_spell_sends() {
    let dir = folder("sending");
    let mut t = travel_in(&dir);
    t.character.name = String::from("Tester");
    t.character.recall_items = vec![(
        ObjectId(0x8000_0001),
        5175,
        String::from("Facility Hub Portal Gem"),
    )];
    let gem = |t: &Travel| {
        t.teleports(
            crate::travel::TravelAllow::default(),
            &std::collections::BTreeSet::new(),
        )
        .iter()
        .find(|p| p.recall == Recall::Spell(5175))
        .map(|p| (p.item, p.to.cell.0))
    };
    assert_eq!(gem(&t), None, "nothing says where it lands");
    // The era's places, as the launcher hands them over.
    t.take_hints(concat!(
        "dereth-places-store\t1\n",
        "world\tDereth\n",
        "sending\tpublished\t5175\tFacility Hub Recall\t0x8A020212 58.639 -89.923 6.005\n",
    ))
    .expect("a place package");
    assert_eq!(gem(&t), Some((Some(ObjectId(0x8000_0001)), 0x8A02_0212)));
    // The server's own places say otherwise: theirs is taken.
    t.set_published_places(concat!(
        "dereth-places-store\t1\n",
        "world\tDereth\n",
        "sending\tpublished\t5175\tFacility Hub Recall\t0x8A020100 10 -10 6\n",
    ))
    .expect("the server's places");
    assert_eq!(gem(&t).map(|g| g.1), Some(0x8A02_0100));
    // Used, it lands where it was seen to.
    t.portals.learn_recall(
        "Tester",
        Recall::Spell(5175),
        &at(0x8A02_0213, 60.0, -88.0),
        1,
    );
    assert_eq!(gem(&t).map(|g| g.1), Some(0x8A02_0213));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A house, mansion or hometown recall the character has been seen to use is one it has: a
/// journey plans with it from then on, and not before.
///
/// Behaviour: plugins.nav.a-characters-recalls-are-learned-where-they-land
#[test]
fn a_house_recall_once_learned_is_planned_with() {
    let dir = folder("house");
    let mut t = travel_in(&dir);
    t.character.name = String::from("Tester");
    let planned = |t: &Travel| {
        t.teleports(
            crate::travel::TravelAllow::default(),
            &std::collections::BTreeSet::new(),
        )
        .iter()
        .map(|p| p.recall)
        .collect::<Vec<_>>()
    };
    assert!(!planned(&t).contains(&Recall::House));
    let villa = at(0x9390_0021, 120.0, 80.0);
    t.portals.learn_recall("Tester", Recall::House, &villa, 1);
    assert!(planned(&t).contains(&Recall::House));
    assert!(!planned(&t).contains(&Recall::Mansion));
    let _ = std::fs::remove_dir_all(&dir);
}

/// What sets a recall off, by hand: the five recall commands, a recall spell cast on oneself, and
/// an item from the pack that casts one; any other spell, item or request does not.
///
/// Behaviour: plugins.nav.a-characters-recalls-are-learned-where-they-land
#[test]
fn the_commands_spells_and_items_that_set_a_recall_off() {
    use dereth_client_model::Request as R;
    use dereth_protocol::combat as c;
    let recall_spell = |s: u32| matches!(s, 48 | 2645);
    let item_spell = |id: ObjectId| match id.0 {
        1 => Some(2645),
        2 => Some(1),
        _ => None,
    };
    let of = |r: R| recall_set_off(&r, &recall_spell, &item_spell);
    assert_eq!(
        of(R::TeleToLifestone(c::CharacterTeleToLifestone)),
        Some(Recall::Lifestone)
    );
    assert_eq!(
        of(R::TeleToMarketplace(c::CharacterTeleToMarketplace)),
        Some(Recall::Marketplace)
    );
    assert_eq!(
        of(R::CastUntargetedSpell(c::MagicCastUntargetedSpell {
            spell_id: 2645
        })),
        Some(Recall::Spell(2645))
    );
    assert_eq!(
        of(R::CastUntargetedSpell(c::MagicCastUntargetedSpell {
            spell_id: 7
        })),
        None,
        "not a recall"
    );
    let used = |id: u32| {
        R::UseEvent(dereth_protocol::items::InventoryUseEvent {
            object: ObjectId(id),
        })
    };
    assert_eq!(of(used(1)), Some(Recall::Spell(2645)), "a recall gem");
    assert_eq!(of(used(2)), None, "an item that casts something else");
    assert_eq!(of(used(3)), None, "an item that casts nothing");
}

/// Where the server keeps the character's lifestone, its tied lifestone and its last portal
/// among its qualities, those are the lifestone command's, Lifestone Recall's and Portal
/// Recall's landings, learned without a use; any other position teaches nothing.
///
/// Behaviour: plugins.nav.a-characters-recalls-are-learned-where-they-land
#[test]
fn the_positions_a_server_keeps_for_the_character_are_its_recalls_landings() {
    let dir = folder("positions");
    let mut t = travel_in(&dir);
    let (stone, tied, gate, death) = (
        at(0xA9B4_0011, 30.0, 30.0),
        at(0x7D64_0021, 40.0, 40.0),
        at(0x8588_002C, 120.0, 95.0),
        at(0x0001_0101, 5.0, 5.0),
    );
    t.recall_positions("Tester", &[(4, stone), (15, tied), (9, gate), (14, death)]);
    let to = |r: Recall| t.portals.recall_destination("Tester", r);
    assert_eq!(to(Recall::Lifestone), Some(stone));
    assert_eq!(to(Recall::Spell(1635)), Some(tied));
    assert_eq!(to(Recall::Spell(2645)), Some(gate));
    assert_eq!(
        t.portals.recalls().len(),
        3,
        "the last outside death is no recall's"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
