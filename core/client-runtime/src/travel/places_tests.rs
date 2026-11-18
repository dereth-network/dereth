//! Behaviour: none (the client's own record of a world's places, learned from recorded sessions).

use super::*;
use crate::travel::StoreFiles;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_primitives::{CellId, Frame, Quat, Vec3};
use dereth_protocol::Reader;
use std::collections::HashMap;
use std::path::PathBuf;

/// A folder of this test's own, empty.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dereth-places-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    dir
}

fn travel_in(dir: &std::path::Path) -> Travel {
    Travel::new(StoreFiles {
        dir: Some(dir.to_path_buf()),
        tiles: None,
        hints: None,
        prefix: String::from("navigation-"),
    })
}

fn position(cell: u32, o: Vec3) -> Position {
    Position::new(CellId(cell), Frame::new(o, Quat::IDENTITY))
}

fn dword(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|d| u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
}

/// Play recording `slug` into `t` as the client would see it, in the order it arrived: each
/// object the server creates, each use the player sends, the player's own position reports and
/// the server's moves of the player, the teleports, and the house profiles. The clock is the
/// recording's own.
fn play(slug: &str, t: &mut Travel) {
    let corpus = Corpus::shared(slug);
    let mut objects: HashMap<ObjectId, PlaceSeen> = HashMap::new();
    let mut player = None;
    let mut at: Option<Position> = None;
    let mut portal_space = false;
    for b in &corpus.blobs {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: an hour of microseconds.
        let now = b.t_rel_micros as f64 / 1e6;
        let p = &b.payload;
        match (b.dir, dword(p, 0)) {
            (Direction::ServerToClient, Some(0xF746)) => player = dword(p, 4).map(ObjectId),
            (Direction::ServerToClient, Some(0xF745)) => {
                let Ok(m) = dereth_protocol::read_body::<dereth_protocol::objects::ItemCreateObject>(
                    &p[4..],
                ) else {
                    continue;
                };
                let w = &m.0.wdesc;
                let o = PlaceSeen {
                    id: m.0.id,
                    bitfield: w.bitfield,
                    item_type: w.obj_type,
                    wcid: w.wcid,
                    name: w.name.clone(),
                    at: m
                        .0
                        .physicsdesc
                        .position
                        .map(|q| position(q.objcell_id, q.frame.origin.into())),
                };
                if Some(o.id) != player {
                    t.see_object(&o);
                }
                objects.insert(o.id, o);
            }
            (Direction::ClientToServer, Some(0xF7B1)) => match dword(p, 8) {
                Some(0x0036) => {
                    let Some(o) = dword(p, 12).and_then(|id| objects.get(&ObjectId(id))) else {
                        continue;
                    };
                    if let (Some(PlaceKind::Portal), Some(position)) =
                        (PlaceKind::of(o.bitfield, o.item_type), o.at)
                    {
                        let sighting = PortalSighting {
                            wcid: Some(o.wcid),
                            name: o.name.clone(),
                            position,
                        };
                        t.portal_used(sighting, now);
                    }
                }
                Some(0xF753) => {
                    let mut r = Reader::body(&p[12..]);
                    if let Ok(a) = dereth_protocol::movement::AutonomousPosition::read(&mut r) {
                        at = Some(position(
                            a.position.objcell_id,
                            a.position.frame.origin.into(),
                        ));
                    }
                }
                _ => {}
            },
            (Direction::ServerToClient, Some(0xF751)) => portal_space = true,
            (Direction::ServerToClient, Some(0xF748)) => {
                let Ok(m) = dereth_protocol::read_body::<
                    dereth_protocol::movement::MovementPositionEvent,
                >(&p[4..]) else {
                    continue;
                };
                if Some(m.id) == player {
                    let o = m.position.origin;
                    at = Some(position(o.objcell_id, o.origin.into()));
                    portal_space = false;
                }
            }
            (Direction::ServerToClient, Some(0xF7B0)) if dword(p, 12) == Some(0x021D) => {
                let mut r = Reader::body(&p[16..]);
                let read =
                    <dereth_protocol::trade::HouseProfileMessage as dereth_protocol::Message>::read(
                        &mut r,
                    );
                if let Ok(m) = read {
                    if let Some(o) = objects.get(&m.covenant_crystal) {
                        let owner = (m.profile.owner.0 != 0).then_some(m.profile.name.as_str());
                        t.see_house(o, m.profile.house_type, owner);
                    }
                }
            }
            _ => continue,
        }
        if let Some(a) = &at {
            t.body_at(a, portal_space, now, "Recorded");
        }
    }
    t.save_places();
}

fn named<'a>(t: &'a Travel, kind: PlaceKind, name: &str) -> Vec<&'a KnownPlace> {
    t.places()
        .places()
        .iter()
        .filter(|p| p.kind == kind && p.name == name)
        .collect()
}

#[test]
fn a_recorded_session_teaches_its_lifestones_portals_vendors_people_and_where_its_exit_leads() {
    let dir = folder("solo");
    let mut t = travel_in(&dir);
    t.note_world("Recorded@127.0.0.1:9000");
    play("long-solo-play", &mut t);
    // The lifestones of the Academy and of Shoushi, where it sends the character.
    let stones: Vec<u32> = named(&t, PlaceKind::Lifestone, "Life Stone")
        .iter()
        .map(|p| p.at.cell.0)
        .collect();
    assert_eq!(stones, [0x7F03_0134, 0xDA55_001D], "{stones:08X?}");
    // The portals that stand where it went, and the vendors and people of the town.
    for (kind, name) in [
        (PlaceKind::Portal, "Exit to Shoushi"),
        (PlaceKind::Portal, "Mannikin Foundry Portal"),
        (PlaceKind::Portal, "Portal to Town Network"),
        (PlaceKind::Vendor, "Hau Ji Tan the Grocer"),
        (PlaceKind::Vendor, "Academy Shopkeep"),
        (PlaceKind::Npc, "Jonathan"),
        (PlaceKind::Npc, "San Ming"),
    ] {
        assert_eq!(named(&t, kind, name).len(), 1, "{name}");
    }
    let creature = t
        .places()
        .places()
        .iter()
        .filter(|p| p.name.contains("Drudge") || p.name == "Recorded")
        .count();
    assert_eq!(creature, 0, "monsters and the player are not places");
    // The exit the player walked through: learned where it leads, in Shoushi's landblock.
    let exit = t
        .store()
        .portals()
        .iter()
        .find(|e| e.name == "Exit to Shoushi")
        .expect("the exit's use learned");
    assert_eq!(exit.source, EdgeSource::Learned);
    assert_eq!(exit.from.cell.0, 0x7F03_0169);
    let to = exit.to.expect("where it leads");
    assert_eq!(to.cell.0 >> 16, 0xDA55, "Shoushi: {:08X}", to.cell.0);
    assert_eq!(
        named(&t, PlaceKind::Portal, "Exit to Shoushi")[0].to,
        Some(to),
        "the map's place knows where it leads too"
    );
    assert!(
        t.store()
            .recall_destination("Recorded", Recall::Spell(PORTAL_RECALL))
            .is_some(),
        "portal recall goes where the last portal sent the character"
    );
    // And it is kept for the world, in its own file.
    let text = std::fs::read_to_string(dir.join("navigation-places-Recorded_127_0_0_1_9000.tsv"))
        .expect("the world's places are written");
    let back = PlaceStore::from_text(&text).expect("they read back");
    let key = |p: &KnownPlace| (p.kind, p.name.clone(), p.at.cell, p.wcid);
    assert_eq!(
        back.places().iter().map(key).collect::<Vec<_>>(),
        t.places().places().iter().map(key).collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_house_is_learned_with_its_owner_from_what_the_server_says_of_it() {
    let dir = folder("house");
    let mut t = travel_in(&dir);
    t.note_world("Recorded@127.0.0.1:9000");
    play("house-purchase-and-trade", &mut t);
    let houses = named(&t, PlaceKind::House, "Villa");
    assert_eq!(houses.len(), 1, "one villa, told of three times");
    assert_eq!(houses[0].at.cell.0, 0x9DAF_002A, "where its crystal stands");
    assert!(houses[0].owner.is_some(), "bought in the recording");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_place_learned_in_one_world_never_shows_in_another() {
    let dir = folder("worlds");
    let mut t = travel_in(&dir);
    t.note_world("First@10.0.0.1:9000");
    play("long-solo-play", &mut t);
    let first = t.places().places().len();
    assert!(first > 30, "{first} places learned in the first world");
    // Another world, even one of the same name on another server, starts with nothing.
    t.note_world("First@10.0.0.2:9000");
    assert_eq!(t.places().places().len(), 0);
    assert_eq!(t.places().world, "First@10.0.0.2:9000");
    t.see_object(&PlaceSeen {
        id: ObjectId(1),
        bitfield: 0x4014,
        item_type: 0x1000_0000,
        wcid: 509,
        name: String::from("Life Stone"),
        at: Some(position(0xA9B4_0011, Vec3::new(10.0, 10.0, 0.0))),
    });
    t.save_places();
    // A new client, as after a restart: each world reads back only its own.
    let mut again = travel_in(&dir);
    again.note_world("First@10.0.0.1:9000");
    assert_eq!(again.places().places().len(), first);
    assert!(again
        .places()
        .places()
        .iter()
        .all(|p| p.at.cell.0 != 0xA9B4_0011));
    again.note_world("First@10.0.0.2:9000");
    assert_eq!(again.places().places().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// What a server publishes of its places, as Empyrean writes them: a lifestone, a person and a
/// portal with where it leads.
const PUBLISHED: &str = "dereth-places-store\t1\nworld\tServer\n\
place\tlifestone\tpublished\t509\tLife Stone\t0xA9B40011 10.000 10.000 0.000\t0\t0\t-\t-\n\
place\tnpc\tpublished\t1101\tSun Ming\t0xA9B40011 20.000 10.000 0.000\t0\t0\t-\t-\n\
place\tportal\tpublished\t1000\tPortal\t0xA9B40011 30.000 10.000 0.000\t0\t0\t-\t0x01AC0118 60.000 -50.000 0.500\n";

#[test]
fn the_places_a_server_publishes_load_under_those_seen_and_are_never_written() {
    let dir = folder("published");
    let mut t = travel_in(&dir);
    // Published before the world is known, as a host fetches it at start.
    assert_eq!(t.take_published(PUBLISHED), Ok(3));
    t.note_world("Server@10.0.0.1:9000");
    assert_eq!(t.places().places().len(), 3);
    assert!(t
        .places()
        .places()
        .iter()
        .all(|p| p.source == EdgeSource::Published));
    // The character sees the lifestone and the person, by another name, and uses the portal.
    let seen = |id: u32, bitfield: u32, item_type: u32, wcid: u32, name: &str, x: f32| PlaceSeen {
        id: ObjectId(id),
        bitfield,
        item_type,
        wcid,
        name: name.to_owned(),
        at: Some(position(0xA9B4_0011, Vec3::new(x, 10.0, 0.0))),
    };
    t.see_object(&seen(1, 0x4014, 0x1000_0000, 509, "Life Stone", 10.5));
    t.see_object(&seen(2, 0x0004, 0x10, 1101, "San Ming", 20.5));
    let stone = &named(&t, PlaceKind::Lifestone, "Life Stone")[0];
    assert_eq!(
        stone.source,
        EdgeSource::Learned,
        "seen, it is the character's"
    );
    assert!(
        stone.first_seen > 0,
        "first seen now, not when it was told of"
    );
    assert_eq!(
        named(&t, PlaceKind::Npc, "San Ming").len(),
        1,
        "the name seen wins"
    );
    assert!(named(&t, PlaceKind::Npc, "Sun Ming").is_empty());
    // Told again, the server's names never replace what the character saw.
    assert_eq!(t.take_published(PUBLISHED), Ok(3));
    assert_eq!(named(&t, PlaceKind::Npc, "San Ming").len(), 1);
    assert_eq!(t.places().places().len(), 3);
    t.save_places();
    // Only what the character saw is written: the portal it never saw is the server's alone.
    let text = std::fs::read_to_string(dir.join("navigation-places-Server_10_0_0_1_9000.tsv"))
        .expect("the world's places are written");
    let back = PlaceStore::from_text(&text).expect("they read back");
    assert_eq!(back.places().len(), 2);
    assert!(back
        .places()
        .iter()
        .all(|p| p.source == EdgeSource::Learned));
    // A new client reads them back, and the server's again under them.
    let mut again = travel_in(&dir);
    again.note_world("Server@10.0.0.1:9000");
    assert_eq!(again.places().places().len(), 2);
    assert_eq!(again.take_published(PUBLISHED), Ok(3));
    assert_eq!(again.places().places().len(), 3);
    let portal = &named(&again, PlaceKind::Portal, "Portal")[0];
    assert_eq!(portal.source, EdgeSource::Published);
    assert_eq!(portal.to.map(|p| p.cell.0), Some(0x01AC_0118));
    assert!(again.take_published("not a store").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The place package of a world's era: its portals and places are taken in as hints, under what
/// the character learned there, and never written into the world's own files.
#[test]
fn an_eras_place_package_is_taken_in_under_what_play_taught_and_never_written() {
    let dir = folder("package");
    let package = dir.join("places-eor");
    std::fs::create_dir_all(&package).expect("the package's folder");
    // As `dereth-places` writes it, its knowledge said to be the server's: taken in as hints all
    // the same.
    std::fs::write(
        package.join("portals.tsv"),
        "dereth-navigation-store\t2\nworld\tDereth\n\
         portal\tpublished\t2068\tDrudge Hideout\t0xA9B30002 5.4 29.5 94.0\t0x019E0114 10.0 -40.0 0.0\t0\t0\t0\t-\t-\t-\t\n",
    )
    .expect("written");
    std::fs::write(
        package.join("places.tsv"),
        "dereth-places-store\t1\nworld\tDereth\n\
         place\ttown\thint\t-\tHoltburg\t0xA9B40019 84.0 7.1 94.0\t0\t0\t-\t-\n\
         place\tlifestone\thint\t509\tPackage's name\t0xA9B40021 30.0 30.0 94.0\t0\t0\t-\t-\n",
    )
    .expect("written");
    // What the character saw of the same lifestone, kept for the world before.
    let world = "Coldeve@127.0.0.1:9000";
    std::fs::write(
        dir.join(format!("navigation-places-{}.tsv", file_name(world))),
        "dereth-places-store\t1\nworld\tColdeve\n\
         place\tlifestone\tlearned\t509\tLife Stone\t0xA9B40021 30.0 30.0 94.0\t10\t20\t-\t-\n",
    )
    .expect("written");
    let mut t = Travel::new(StoreFiles {
        dir: Some(dir.clone()),
        tiles: None,
        hints: Some(package),
        prefix: String::from("navigation-"),
    });
    t.note_world(world);
    let portals = t.store().portals();
    assert_eq!(portals.len(), 1, "{portals:?}");
    assert_eq!(portals[0].source, EdgeSource::Hint);
    let town = named(&t, PlaceKind::Town, "Holtburg");
    assert_eq!(town.len(), 1, "the package's town");
    assert_eq!(town[0].source, EdgeSource::Hint);
    let stone = named(&t, PlaceKind::Lifestone, "Life Stone");
    assert_eq!(stone.len(), 1, "the lifestone as the character saw it");
    assert_eq!(stone[0].source, EdgeSource::Learned);
    assert!(named(&t, PlaceKind::Lifestone, "Package's name").is_empty());
    // Written back: what was learned, and nothing of the package's.
    t.places.changed = true;
    t.save_places();
    t.save_store();
    let kept =
        std::fs::read_to_string(dir.join(format!("navigation-places-{}.tsv", file_name(world))))
            .expect("the world's places");
    assert!(!kept.contains("Holtburg"), "{kept}");
    let portals_kept =
        std::fs::read_to_string(dir.join(format!("navigation-{}.tsv", file_name(world))))
            .unwrap_or_default();
    assert!(!portals_kept.contains("Drudge Hideout"), "{portals_kept}");
    let _ = std::fs::remove_dir_all(&dir);
}
