//! Behaviour: none (checks the place store's learning, its text form and the portal-use watch).

use super::*;
use dereth_primitives::{CellId, Frame, Quat, Vec3};

fn at(cell: u32, x: f32, y: f32) -> Position {
    Position::new(
        CellId(cell),
        Frame::new(Vec3::new(x, y, 20.0), Quat::IDENTITY),
    )
}

fn seen(kind: PlaceKind, name: &str, x: f32, now: u64) -> KnownPlace {
    KnownPlace {
        kind,
        source: EdgeSource::Learned,
        wcid: Some(509),
        name: name.to_owned(),
        at: at(0xDA55_001D, x, 103.0),
        first_seen: now,
        last_seen: now,
        owner: None,
        to: None,
    }
}

#[test]
fn a_place_is_told_by_its_description_and_players_items_and_monsters_are_not_places() {
    assert_eq!(
        PlaceKind::of(0x0000_4014, 0x1000_0000),
        Some(PlaceKind::Lifestone)
    );
    assert_eq!(
        PlaceKind::of(0x0004_0014, 0x0001_0000),
        Some(PlaceKind::Portal)
    );
    assert_eq!(PlaceKind::of(0x0000_0204, 0x10), Some(PlaceKind::Vendor));
    assert_eq!(PlaceKind::of(0x0000_0004, 0x10), Some(PlaceKind::Npc));
    assert_eq!(PlaceKind::of(0x0000_0014, 0x10), None, "a monster");
    assert_eq!(PlaceKind::of(0x0000_0018, 0x10), None, "a player");
    assert_eq!(
        PlaceKind::of(0x0000_0094, 0x80),
        None,
        "hidden from the interface"
    );
    assert_eq!(PlaceKind::of(0x0000_0015, 0x200), None, "a hook");
    for k in PlaceKind::ALL {
        assert_eq!(PlaceKind::parse(k.word()), Some(k));
    }
}

#[test]
fn a_place_seen_again_is_the_same_place_and_a_new_name_or_owner_is_a_change() {
    let mut s = PlaceStore::new("Test@127.0.0.1:9000");
    assert!(s.see(seen(PlaceKind::Lifestone, "Life Stone", 88.8, 100)));
    assert!(!s.see(seen(PlaceKind::Lifestone, "Life Stone", 89.5, 200)));
    assert_eq!(s.places().len(), 1, "seen 0.7 m apart");
    assert_eq!(
        (s.places()[0].first_seen, s.places()[0].last_seen),
        (100, 200)
    );
    let r = s.revision();
    let mut house = seen(PlaceKind::House, "Villa", 10.0, 300);
    assert!(s.see(house.clone()));
    house.owner = Some(String::from("Someone"));
    assert!(s.see(house));
    assert_eq!(s.revision(), r + 2);
    assert_eq!(s.places()[1].owner.as_deref(), Some("Someone"));
    assert!(
        s.see(seen(PlaceKind::Vendor, "Life Stone", 88.8, 400)),
        "another kind"
    );
}

#[test]
fn a_published_place_never_replaces_one_the_character_saw() {
    let mut s = PlaceStore::new("w");
    s.see(seen(PlaceKind::Npc, "San Ming", 73.5, 1));
    let mut told = seen(PlaceKind::Npc, "Sun Ming", 73.6, 2);
    told.source = EdgeSource::Published;
    s.merge([told]);
    assert_eq!(s.places()[0].name, "San Ming");
    assert_eq!(s.places()[0].source, EdgeSource::Learned);
    assert_eq!(s.without(EdgeSource::Learned).places().len(), 0);
}

#[test]
fn a_published_place_told_again_changes_nothing_and_seen_becomes_the_characters_own() {
    let mut s = PlaceStore::new("w");
    let mut told = seen(PlaceKind::Lifestone, "Life Stone", 88.8, 0);
    told.source = EdgeSource::Published;
    assert!(s.see(told.clone()));
    let r = s.revision();
    assert!(!s.see(told), "told again: nothing a reader shows changed");
    assert_eq!(s.revision(), r);
    assert!(
        s.see(seen(PlaceKind::Lifestone, "Life Stone", 89.0, 500)),
        "seen: it is the character's now"
    );
    let p = &s.places()[0];
    assert_eq!(p.source, EdgeSource::Learned);
    assert_eq!(
        (p.first_seen, p.last_seen),
        (500, 500),
        "first seen when it was seen, not when it was told of"
    );
}

#[test]
fn a_place_across_a_landblock_edge_is_the_same_place_wherever_it_was_last_seen() {
    let mut s = PlaceStore::new("w");
    let mut p = seen(PlaceKind::Npc, "Guard", 0.0, 1);
    p.at = at(0xA9B4_0008, 10.0, 191.5);
    assert!(s.see(p.clone()));
    // A metre away, in the landblock to the north.
    p.at = at(0xA9B5_0001, 10.0, 0.5);
    p.last_seen = 2;
    assert!(!s.see(p.clone()));
    assert_eq!(s.places().len(), 1);
    assert_eq!(
        s.places()[0].at.cell.0 >> 16,
        0xA9B5,
        "where it was last seen"
    );
    // It walks on north, two metres at a time, into the landblock past that one: found where it
    // now stands, not where it was first kept.
    for step in 1..=96u8 {
        p.at = at(0xA9B5_0001, 10.0, 0.5 + 2.0 * f32::from(step));
        assert!(!s.see(p.clone()));
    }
    p.at = at(0xA9B6_0001, 10.0, 0.5);
    assert!(!s.see(p.clone()));
    assert_eq!(s.places().len(), 1);
    // Read back, it is found as before.
    let mut back = PlaceStore::from_text(&s.to_text()).expect("reads");
    assert!(!back.see(p));
    assert_eq!(back.places().len(), 1);
    assert_eq!(back.without(EdgeSource::Hint).places().len(), 1);
}

#[test]
fn thousands_of_published_places_are_taken_in_once_each() {
    let mut s = PlaceStore::new("w");
    let grid: Vec<KnownPlace> = (0..4000u32)
        .map(|n| {
            let mut p = seen(PlaceKind::Npc, "Person", 0.0, 0);
            p.source = EdgeSource::Published;
            p.wcid = Some(n);
            #[allow(clippy::cast_precision_loss)] // LINT-OK: small whole numbers.
            let (x, y) = ((n % 40) as f32 * 4.0, (n / 40 % 40) as f32 * 4.0);
            p.at = at(0xA9B4_0001 + ((n / 1600) << 16), x, y);
            p
        })
        .collect();
    s.merge(grid.clone());
    assert_eq!(s.places().len(), 4000);
    let r = s.revision();
    s.merge(grid);
    assert_eq!(s.places().len(), 4000);
    assert_eq!(s.revision(), r, "told again: nothing changed");
}

#[test]
fn where_a_portal_leads_is_never_taken_from_a_less_trusted_source() {
    let shoushi = at(0xDA55_0014, 66.0, 92.0);
    let elsewhere = at(0x01AC_0118, 60.0, -50.0);
    let mut s = PlaceStore::new("w");
    let mut used = seen(PlaceKind::Portal, "Exit to Shoushi", 158.6, 5);
    used.to = Some(shoushi);
    s.see(used);
    let mut told = seen(PlaceKind::Portal, "Exit to Shoushi", 158.6, 0);
    told.source = EdgeSource::Published;
    told.to = Some(elsewhere);
    assert!(!s.see(told.clone()), "the character's own use wins");
    assert_eq!(s.places()[0].to, Some(shoushi));
    // A portal seen standing but never used takes where the server says it leads.
    let mut t = PlaceStore::new("w");
    t.see(seen(PlaceKind::Portal, "Exit to Shoushi", 158.6, 5));
    assert!(t.see(told));
    assert_eq!(t.places()[0].to, Some(elsewhere));
    assert_eq!(t.places()[0].source, EdgeSource::Learned);
}

#[test]
fn the_store_reads_back_as_written_and_names_another_world() {
    let mut s = PlaceStore::new("Tab\tWorld@host:1");
    s.see(seen(PlaceKind::Portal, "Exit\tto Shoushi", 158.6, 5));
    let mut h = seen(PlaceKind::House, "Villa", 1.0, 6);
    h.wcid = None;
    h.owner = Some(String::from("A \\ B"));
    s.see(h);
    let mut used = seen(PlaceKind::Portal, "Exit\tto Shoushi", 158.6, 7);
    used.to = Some(at(0xDA55_0014, 66.0, 92.0));
    assert!(s.see(used), "learned where it leads");
    let text = s.to_text();
    assert!(text.starts_with("dereth-places-store\t1\n"));
    let back = PlaceStore::from_text(&text).expect("reads");
    assert_eq!(back.world, s.world);
    assert_eq!(back.places(), s.places());
    assert_eq!(
        PlaceStore::from_text("dereth-places-store\t2\n"),
        Err(StoreError::Version(2))
    );
    assert_eq!(
        PlaceStore::from_text("dereth-navigation-store\t1\n"),
        Err(StoreError::NotAStore)
    );
    let later = "dereth-places-store\t1\nworld\tw\nplace\tshrine\tlearned\t-\tX\t0x00000001 0 0 0\t0\t0\t-\t-\n";
    assert_eq!(
        PlaceStore::from_text(later).map(|s| s.places().len()),
        Ok(0)
    );
}

fn portal() -> PortalSighting {
    PortalSighting {
        wcid: Some(29337),
        name: String::from("Exit to Shoushi"),
        position: at(0x7F03_0169, 158.6, -149.5),
        shape: None,
    }
}

#[test]
fn a_portal_used_is_learned_where_its_teleport_lands() {
    let mut w = PortalUseWatch::default();
    let by_it = at(0x7F03_0169, 157.0, -149.0);
    let landed = at(0xDA55_0014, 66.0, 92.0);
    w.used(portal(), 10.0);
    assert_eq!(w.frame(&by_it, false, 11.0), None, "walked to it");
    assert_eq!(w.frame(&by_it, true, 12.0), None, "portal space");
    assert_eq!(w.frame(&landed, true, 13.0), None, "still in portal space");
    assert_eq!(w.frame(&landed, false, 14.0), Some((portal(), landed)));
    assert_eq!(w.frame(&landed, true, 15.0), None, "once");
}

#[test]
fn a_teleport_no_use_began_or_far_from_the_portal_or_too_late_teaches_nothing() {
    let by_it = at(0x7F03_0169, 157.0, -149.0);
    let far = at(0x7F03_0134, 120.0, -160.0);
    let landed = at(0xDA55_0014, 66.0, 92.0);
    // A recall: no use before it.
    let mut w = PortalUseWatch::default();
    assert_eq!(w.frame(&by_it, true, 1.0), None);
    assert_eq!(w.frame(&landed, false, 2.0), None);
    // A recall cast after walking away from a portal that was used.
    w.used(portal(), 3.0);
    assert_eq!(w.frame(&far, true, 4.0), None);
    assert_eq!(w.frame(&landed, false, 5.0), None);
    // A use that no teleport follows in time.
    w.used(portal(), 10.0);
    assert_eq!(
        w.frame(&by_it, false, 10.0 + PortalUseWatch::WAIT + 1.0),
        None
    );
    assert_eq!(w.frame(&by_it, true, 32.0), None);
    assert_eq!(w.frame(&landed, false, 33.0), None);
    // A teleport that leaves the body where it stood.
    w.used(portal(), 40.0);
    assert_eq!(w.frame(&by_it, true, 41.0), None);
    assert_eq!(w.frame(&by_it, false, 42.0), None);
}
