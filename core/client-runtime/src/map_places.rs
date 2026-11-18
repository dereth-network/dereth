//! The places the game's data names, read by the client for every interface and host: the towns
//! and landmarks the game's own map names (its rollover notes), each placed on the buildings the
//! landblocks stand near where that map puts it, and every house by its building's model. The
//! world map draws them and the frame hook's plugins read them, one set. Portals, lifestones,
//! vendors and the rest the server places are not in the data: the client learns them per world.

use std::sync::Arc;

use dereth_client_contract::panels::map::{MapNote, MapPlace as Place, MapPlaceKind as PlaceKind};
use dereth_dat::RetailDatStore;
use dereth_primitives::EraId;

/// A building the landblocks stand: its model and where it stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Building {
    pub model: u32,
    pub ns: f64,
    pub ew: f64,
}

/// The cottages' building models.
pub const COTTAGES: [u32; 10] = [
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
pub const VILLAS: [u32; 2] = [0x0100_22CE, 0x0100_2383];
/// A mansion stands as four buildings of these models; the first is the one it is marked at.
pub const MANSIONS: [u32; 4] = [0x0100_2287, 0x0100_2286, 0x0100_2289, 0x0100_22D2];

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

/// Landblocks a side of the world.
const BLOCKS: i32 = 255;

/// The buildings of landblock column `bx`, added to `out`.
fn read_column(store: &RetailDatStore, bx: i32, out: &mut Vec<Building>) {
    use crate::world_build::{read_landblock, read_lbi};
    // A landblock is 192 meters across and each step of the map coordinates 240 meters; 0 stands
    // 101.95 steps in.
    let unit = |block: i32, m: f32| (f64::from(block) * 192.0 + f64::from(m)) / 240.0 - 101.95;
    for by in 0..BLOCKS {
        let Some(land) = read_landblock(store, bx, by) else {
            continue;
        };
        if land.lbi_exists == 0 {
            continue;
        }
        for b in read_lbi(store, bx, by)
            .map(|i| i.buildings)
            .unwrap_or_default()
        {
            out.push(Building {
                model: b.id.0,
                ns: unit(by, b.frame.origin.y),
                ew: unit(bx, b.frame.origin.x),
            });
        }
    }
}

/// The work of a [`MapPlacesBuild`], a column of landblocks a step.
struct Steps {
    store: Arc<RetailDatStore>,
    column: i32,
    buildings: Vec<Building>,
}

impl Steps {
    fn step(&mut self) -> Option<Arc<Vec<Place>>> {
        if self.column < BLOCKS {
            read_column(&self.store, self.column, &mut self.buildings);
            self.column += 1;
            return None;
        }
        Some(Arc::new(places(&self.buildings)))
    }
}

/// The places the data names, read off the frame: on a thread of their own where there are
/// threads, else a share of each frame's time.
pub struct MapPlacesBuild {
    #[cfg(not(target_arch = "wasm32"))]
    done: std::sync::mpsc::Receiver<Arc<Vec<Place>>>,
    #[cfg(target_arch = "wasm32")]
    steps: Steps,
}

impl std::fmt::Debug for MapPlacesBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MapPlacesBuild")
    }
}

/// The share of a frame a build in the frame's own thread takes.
#[cfg(target_arch = "wasm32")]
const FRAME_SHARE: std::time::Duration = std::time::Duration::from_millis(4);

impl MapPlacesBuild {
    /// Read the places from `store`'s landblocks.
    #[must_use]
    pub fn start(store: Arc<RetailDatStore>) -> Self {
        let steps = Steps {
            store,
            column: 0,
            buildings: Vec::new(),
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut steps = steps;
            let (tx, done) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("map places".to_owned())
                .spawn(move || loop {
                    if let Some(places) = steps.step() {
                        let _ = tx.send(places);
                        return;
                    }
                });
            if let Err(e) = spawned {
                tracing::warn!("the map places' thread did not start: {e}");
            }
            Self { done }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self { steps }
        }
    }

    /// The places, once read.
    pub fn poll(&mut self) -> Option<Arc<Vec<Place>>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.done.try_recv().ok()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let until = web_time::Instant::now() + FRAME_SHARE;
            loop {
                if let Some(done) = self.steps.step() {
                    return Some(done);
                }
                if web_time::Instant::now() >= until {
                    return None;
                }
            }
        }
    }

    /// The places, waited for.
    pub fn wait(self) -> Option<Arc<Vec<Place>>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.done.recv().ok()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut steps = self.steps;
            loop {
                if let Some(done) = steps.step() {
                    return Some(done);
                }
            }
        }
    }
}
