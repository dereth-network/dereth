//! The places the world map names, read from the same data as its picture.
//!
//! Two kinds of place come from the data alone, with no server:
//!
//! - **The houses**, by the building each one is: every cottage, villa and mansion the landblocks
//!   stand, by its model ([`house_kind`]). Whether a world offers them, and who owns them, is the
//!   server's to say.
//! - **The towns and landmarks** the game's own map names (its rollover notes), each placed on the
//!   buildings that stand near where that map puts it, and left out where the data has none: data
//!   from before a town was built does not name it.
//!
//! Portals, lifestones, vendors and anything else the server places are not in the data. The
//! client learns them per world as the player plays, and [`learned`] turns what it learned into
//! places the map draws over these.

pub use dereth_client_contract::panels::map::{MapPlace as Place, MapPlaceKind as PlaceKind};

pub use dereth_client_runtime::map_places::{
    house_kind, named_places, note_coords, places, Building, COTTAGES, MANSIONS, VILLAS,
};

/// Map coordinates `(ns, ew)` of `at`, as its landblock and the place in it give them: a building's
/// rooms share the landblock's ground, a dungeon's do not.
#[must_use]
pub fn coords(at: &dereth_primitives::Position) -> (f64, f64) {
    let unit = |block: u32, m: f32| (f64::from(block) * 192.0 + f64::from(m)) / 240.0 - 101.95;
    let (bx, by) = (at.cell.0 >> 24, (at.cell.0 >> 16) & 0xFF);
    (unit(by, at.frame.origin.y), unit(bx, at.frame.origin.x))
}

/// Whether `at` is a land cell, out of doors.
const fn outdoors(at: &dereth_primitives::Position) -> bool {
    at.cell.0 & 0xFFFF < 0x100
}

/// How near a place the data names a learned place indoors must stand, in map units, to be in a
/// building of a town rather than a dungeon.
const IN_TOWN: f64 = 1.0;

/// A portal's name as its dungeon's: "Portal to Drudge Hideout" and "Drudge Hideout Portal" both
/// name "Drudge Hideout".
#[must_use]
pub fn dungeon_name(portal: &str) -> &str {
    portal
        .strip_prefix("Portal to ")
        .or_else(|| portal.strip_suffix(" Portal"))
        .unwrap_or(portal)
}

/// The places the client learned of this world, as the map draws them over `named`, the places
/// the data names:
///
/// - each lifestone, portal, vendor and person standing outdoors, or indoors in a building that
///   stands among the data's places (a shop in a town); a place in a dungeon is not drawn;
/// - a portal the player used into a dungeon in another landblock, as that dungeon's entrance;
/// - a house whose crystal the player used, with its owner, or for sale.
#[must_use]
pub fn learned(places: &[dereth_navigation::KnownPlace], named: &[Place]) -> Vec<Place> {
    use dereth_navigation::PlaceKind as Learned;
    // The data's places by the square of the map they stand in, a town's width across, so a
    // place indoors is looked for among those near it alone: a server may publish thousands.
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: map coordinates are about ±100.
    let square = |ns: f64, ew: f64| ((ns / IN_TOWN).floor() as i64, (ew / IN_TOWN).floor() as i64);
    let mut near: std::collections::HashMap<(i64, i64), Vec<(f64, f64)>> =
        std::collections::HashMap::new();
    for n in named {
        near.entry(square(n.ns, n.ew))
            .or_default()
            .push((n.ns, n.ew));
    }
    let in_town = |ns: f64, ew: f64| {
        let (a, b) = square(ns, ew);
        (a - 1..=a + 1).any(|i| {
            (b - 1..=b + 1).any(|j| {
                near.get(&(i, j)).is_some_and(|v| {
                    v.iter()
                        .any(|(n, e)| (n - ns).powi(2) + (e - ew).powi(2) <= IN_TOWN * IN_TOWN)
                })
            })
        })
    };
    let mut out = Vec::new();
    for p in places {
        let (ns, ew) = coords(&p.at);
        if !outdoors(&p.at) && !in_town(ns, ew) {
            continue;
        }
        let (kind, name) = match p.kind {
            // The map names its towns and landmarks from its own notes.
            Learned::Town | Learned::Landmark => continue,
            Learned::Lifestone => (PlaceKind::Lifestone, p.name.clone()),
            Learned::Vendor => (PlaceKind::Vendor, p.name.clone()),
            Learned::Npc => (PlaceKind::Npc, p.name.clone()),
            Learned::Portal => match &p.to {
                Some(to) if !outdoors(to) && to.cell.0 >> 16 != p.at.cell.0 >> 16 => {
                    (PlaceKind::Dungeon, dungeon_name(&p.name).to_owned())
                }
                _ => (PlaceKind::Portal, p.name.clone()),
            },
            Learned::House => {
                let kind = match p.name.as_str() {
                    "Villa" => PlaceKind::Villa,
                    "Mansion" => PlaceKind::Mansion,
                    _ => PlaceKind::Cottage,
                };
                let name = match &p.owner {
                    Some(owner) => format!("{}, owned by {owner}", p.name),
                    None => format!("{}, for sale", p.name),
                };
                (kind, name)
            }
        };
        out.push(Place { kind, name, ns, ew });
    }
    out
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
