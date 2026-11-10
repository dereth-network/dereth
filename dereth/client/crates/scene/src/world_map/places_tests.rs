//! Behaviour: none (this client's own world map, not a behaviour of the retail client)

use super::*;

/// A building of `model` at `(ns, ew)`.
fn at(model: u32, ns: f64, ew: f64) -> Building {
    Building { model, ns, ew }
}

/// The place named `name`, if there is one.
fn named<'a>(places: &'a [Place], name: &str) -> Option<&'a Place> {
    places.iter().find(|p| p.name == name)
}

/// Where the game's map puts the note named `name`.
fn note(name: &str) -> (f64, f64) {
    let note = dereth_client_contract::panels::map::notes(EraId::Eor)
        .find(|n| n.name == name)
        .expect("a note of that name");
    note_coords(&note)
}

#[test]
fn a_house_is_marked_by_its_building_and_a_mansion_once() {
    let buildings = [
        at(COTTAGES[0], 10.0, 10.0),
        at(VILLAS[1], 11.0, 10.0),
        at(MANSIONS[0], 12.0, 10.0),
        at(MANSIONS[1], 12.05, 10.0),
        at(MANSIONS[2], 12.0, 10.05),
        at(MANSIONS[3], 12.05, 10.05),
        at(0x0100_1124, 13.0, 10.0),
    ];
    let kinds: Vec<(PlaceKind, f64)> = places(&buildings).iter().map(|p| (p.kind, p.ns)).collect();
    assert_eq!(
        kinds,
        [
            (PlaceKind::Cottage, 10.0),
            (PlaceKind::Villa, 11.0),
            (PlaceKind::Mansion, 12.0)
        ]
    );
}

#[test]
fn the_game_s_map_puts_holtburg_where_holtburg_is() {
    let (ns, ew) = note("Holtburg");
    assert!(
        (ns - 42.7).abs() < 0.1 && (ew - 35.1).abs() < 0.1,
        "{ns} {ew}"
    );
}

#[test]
fn a_town_is_named_on_the_buildings_near_its_note_and_not_without_them() {
    let (ns, ew) = note("Holtburg");
    // A town of several kinds of building a unit and a half south-west of the note, and a block of
    // one kind further off.
    let mut buildings: Vec<Building> = (0..6)
        .map(|i| at(0x0100_1100 + i, ns - 1.5 + 0.1 * f64::from(i), ew - 1.5))
        .collect();
    buildings.extend((0..20).map(|_| at(0x0100_4000, ns + 3.0, ew)));
    let places = named_places(&buildings);
    let holtburg = named(&places, "Holtburg").expect("Holtburg");
    assert_eq!(holtburg.kind, PlaceKind::Town);
    assert!(
        (holtburg.ns - (ns - 1.25)).abs() < 1e-9 && (holtburg.ew - (ew - 1.5)).abs() < 1e-9,
        "on the town: {holtburg:?}"
    );
    assert_eq!(places.len(), 1, "no other note has buildings: {places:?}");

    // Houses alone are no town, and one building is too few.
    let houses: Vec<Building> = (0..6).map(|_| at(COTTAGES[0], ns, ew)).collect();
    assert!(named_places(&houses).is_empty());
    assert!(named_places(&[at(0x0100_1100, ns, ew)]).is_empty());
}

#[test]
fn a_small_note_is_a_landmark() {
    let (ns, ew) = note("Bandit Castle");
    let buildings = [at(0x0100_1100, ns, ew), at(0x0100_1101, ns, ew + 0.1)];
    let places = named_places(&buildings);
    assert_eq!(
        named(&places, "Bandit Castle").map(|p| p.kind),
        Some(PlaceKind::Landmark)
    );
}
