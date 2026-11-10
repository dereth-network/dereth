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
//! Portals, lifestones, vendors and anything else the server places are not here: the data does
//! not hold them.

use dereth_client_contract::panels::map::MapNote;
use dereth_primitives::EraId;

/// What a place on the map is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceKind {
    /// A town the game's map marks with a town's icon.
    Town,
    /// A smaller place the game's map names, or a region.
    Landmark,
    Cottage,
    Villa,
    Mansion,
}

/// A place the map marks: what it is, its name and where it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub kind: PlaceKind,
    pub name: String,
    pub ns: f64,
    pub ew: f64,
}

/// A building the landblocks stand: its model and where it stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Building {
    pub model: u32,
    pub ns: f64,
    pub ew: f64,
}

/// The cottages' building models.
const COTTAGES: [u32; 10] = [
    0x0100_2226,
    0x0100_2228,
    0x0100_222F,
    0x0100_2232,
    0x0100_228B,
    0x0100_22EF,
    0x0100_25D4,
    0x0100_25E0,
    0x0100_25E3,
    0x0100_25E6,
];
/// The villas' building models.
const VILLAS: [u32; 2] = [0x0100_22CE, 0x0100_2383];
/// A mansion stands as four buildings of these models; the first is the one it is marked at.
const MANSIONS: [u32; 4] = [0x0100_2287, 0x0100_2286, 0x0100_2289, 0x0100_22D2];

/// The house a building of `model` is, if it is one.
#[must_use]
pub fn house_kind(model: u32) -> Option<PlaceKind> {
    if COTTAGES.contains(&model) {
        Some(PlaceKind::Cottage)
    } else if VILLAS.contains(&model) {
        Some(PlaceKind::Villa)
    } else if MANSIONS.contains(&model) {
        Some(PlaceKind::Mansion)
    } else {
        None
    }
}

/// The game's map picture's marker area, in the picture's own pixels: left, right, top, bottom.
const MARKER_AREA: (f64, f64, f64, f64) = (6.0, 247.0, 8.0, 258.0);

/// Where the game's map puts note `note`, the middle of its rectangle, as `(ns, ew)`.
#[must_use]
pub fn note_coords(note: &MapNote) -> (f64, f64) {
    let (x0, x1, y0, y1) = MARKER_AREA;
    let (aw, ah) = (x1 - x0 + 1.0, y1 - y0 + 1.0);
    let x = f64::from(note.x) + f64::from(note.w) / 2.0;
    let y = f64::from(note.y) + f64::from(note.h) / 2.0;
    let ew = ((x - x0) * 2048.0 / aw - 1024.0) / 10.0;
    let ns = (1023.0 - (y - y0) * 2048.0 / ah) / 10.0;
    (ns, ew)
}

/// How far from where the game's map puts a town its buildings are looked for, in map units.
const TOWN_REACH: f64 = 4.0;
/// How near one another a town's buildings stand, in map units.
const TOWN_SPREAD: f64 = 0.8;
/// How quickly a group of buildings counts for less the further it is from the note, in map units.
const TOWN_FALLOFF: f64 = 2.0;
/// The fewest buildings that make a place.
const FEWEST: usize = 2;

/// The game's map's notes that the data's buildings stand at, each placed on the group of them it
/// names: of the buildings near the note, the group with the most kinds of building, nearer ones
/// counting for more. The houses are no part of a town here.
#[must_use]
pub fn named_places(buildings: &[Building]) -> Vec<Place> {
    let others: Vec<&Building> = buildings
        .iter()
        .filter(|b| house_kind(b.model).is_none())
        .collect();
    let mut places = Vec::new();
    // The later map's notes: they say where each place is on the later picture, which is the
    // world drawn the way this map draws it. A place the data has no buildings at is left out,
    // which is what leaves out the places older data has not built yet.
    for note in dereth_client_contract::panels::map::notes(EraId::Eor) {
        let (ns, ew) = note_coords(&note);
        let near: Vec<&Building> = others
            .iter()
            .copied()
            .filter(|b| within(b, (ns, ew), TOWN_REACH))
            .collect();
        let mut best: Option<(f64, Vec<&Building>)> = None;
        for b in &near {
            let group: Vec<&Building> = near
                .iter()
                .copied()
                .filter(|o| within(o, (b.ns, b.ew), TOWN_SPREAD))
                .collect();
            let mut models: Vec<u32> = group.iter().map(|o| o.model).collect();
            models.sort_unstable();
            models.dedup();
            let d2 = (b.ns - ns).powi(2) + (b.ew - ew).powi(2);
            #[allow(clippy::cast_precision_loss)] // LINT-OK: a few dozen kinds of building.
            let score = models.len() as f64
                * dereth_primitives::num::math::exp(-d2 / (TOWN_FALLOFF * TOWN_FALLOFF));
            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                best = Some((score, group));
            }
        }
        let Some((_, group)) = best.filter(|(_, g)| g.len() >= FEWEST) else {
            continue;
        };
        #[allow(clippy::cast_precision_loss)] // LINT-OK: a few dozen buildings.
        let n = group.len() as f64;
        places.push(Place {
            kind: if note.w >= 7 && note.w <= 9 {
                PlaceKind::Town
            } else {
                PlaceKind::Landmark
            },
            name: note.name.to_owned(),
            ns: group.iter().map(|b| b.ns).sum::<f64>() / n,
            ew: group.iter().map(|b| b.ew).sum::<f64>() / n,
        });
    }
    places
}

/// Whether `b` stands within `reach` of `(ns, ew)`.
fn within(b: &Building, (ns, ew): (f64, f64), reach: f64) -> bool {
    (b.ns - ns).powi(2) + (b.ew - ew).powi(2) <= reach * reach
}

/// Every place the data names: the towns and landmarks, then the houses.
#[must_use]
pub fn places(buildings: &[Building]) -> Vec<Place> {
    let mut places = named_places(buildings);
    places.extend(buildings.iter().filter_map(|b| {
        let kind = house_kind(b.model)?;
        // A mansion is marked at its first building alone.
        if kind == PlaceKind::Mansion && b.model != MANSIONS[0] {
            return None;
        }
        Some(Place {
            kind,
            name: match kind {
                PlaceKind::Cottage => "Cottage",
                PlaceKind::Villa => "Villa",
                _ => "Mansion",
            }
            .to_owned(),
            ns: b.ns,
            ew: b.ew,
        })
    }));
    places
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
