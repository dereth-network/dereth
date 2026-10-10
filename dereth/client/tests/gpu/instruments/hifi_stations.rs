//! The stations the high-fidelity presentation is captured and checked at: nine places and
//! times chosen to cover what the presentation draws over. Each says why it is there.
//!
//! A station is loaded and stepped exactly the same way with the presentation off and on, so two
//! runs of it draw the same frames; only what the presentation does can tell them apart.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`).

#![cfg(gpu)]

use std::sync::Arc;

use dereth_assets::world::CellLandblock;
use dereth_assets::Decode;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;
use dereth_world_data::env_cells::EnvCellLoader;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

/// Holtburg.
pub(crate) const HOLTBURG: u16 = 0xA9B4;

/// A body standing in the front doorway of a Holtburg house, its camera swept back into the room:
/// a frame split between outdoors and an interior.
const DOORWAY_CELL: u32 = 0xA9B4_0029;
const DOORWAY_ORIGIN: Vec3 = Vec3::new(136.29, 5.155, 94.082);

/// An interior cell: a room of a building in the town.
const INDOOR_CELL: u32 = 0xA9B4_0143;

/// When in the calendar a station is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Clock {
    /// The first day of a Sunny day group, at this fraction of the day.
    Sunny(f32),
    /// The offline clock's own day, a Rainy one, at this fraction of the day.
    Rainy(f32),
    /// The first Sunny day, in the evening when the sun stands this many degrees up.
    SunUp(f32),
}

/// Where the viewer is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Place {
    /// A free camera over `block` at block-local `(x, y)`, `eye` metres over the ground there,
    /// turned `yaw` radians from north and pitched `pitch` radians.
    Free {
        block: u16,
        x: f32,
        y: f32,
        eye: f32,
        yaw: f32,
        pitch: f32,
    },
    /// A body standing in `cell` at `origin`, facing `yaw` degrees, with its chase camera.
    Body { cell: u32, origin: Vec3, yaw: f32 },
    /// A body standing on the ground of `block` at block-local `(x, y)`, facing `yaw` degrees,
    /// with its chase camera.
    Ground {
        block: u16,
        x: f32,
        y: f32,
        yaw: f32,
    },
    /// A free camera over `block` at block-local `(x, y)`, `eye` metres over the ground there,
    /// looking at the block-local point `at` (its height absolute, or the ground's when it is
    /// not a number).
    Look {
        block: u16,
        x: f32,
        y: f32,
        eye: f32,
        at: [f32; 3],
    },
    /// A body standing on the ground of `block` at block-local `(x, y)`, facing `yaw` degrees,
    /// seen from the front: the camera `dist` metres ahead of it, `eye` metres over the ground,
    /// looking back at its head.
    Facing {
        block: u16,
        x: f32,
        y: f32,
        yaw: f32,
        dist: f32,
        eye: f32,
    },
    /// A free camera where the Horizon interface's orbit camera stands, at its own distance and
    /// tilt, behind a player whose feet are at block-local `feet` in `block` and who faces `yaw`
    /// radians from north. No body is drawn.
    Orbit { block: u16, feet: Vec3, yaw: f32 },
}

/// One station.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Station {
    /// Its name, as file names and the selection variable spell it.
    pub name: &'static str,
    /// Why it is one of the stations.
    pub why: &'static str,
    /// Where.
    pub place: Place,
    /// When.
    pub clock: Clock,
    /// Whether particles and the weather layer are drawn.
    pub weather: bool,
    /// The landscape window's radius, blocks.
    pub land_radius: u32,
}

impl Station {
    /// The landblock the scene is loaded around.
    pub(crate) fn landblock(&self) -> u16 {
        match self.place {
            Place::Free { block, .. }
            | Place::Ground { block, .. }
            | Place::Look { block, .. }
            | Place::Facing { block, .. }
            | Place::Orbit { block, .. } => block,
            #[allow(clippy::cast_possible_truncation)] // LINT-OK: the high half of a cell id
            Place::Body { cell, .. } => (cell >> 16) as u16,
        }
    }
}

/// The landblock's vertex heights in metres and its terrain words, indexed `x * 9 + y`.
fn block_record(
    store: &RetailDatStore,
    table: &[f32],
    block: u16,
) -> Option<([f32; 81], [u16; 81])> {
    let did = dereth_world_data::landblock::landblock_did(block);
    let bytes = store.read_typed(DbType::LandBlock, did).ok()?;
    let lb = CellLandblock::decode_payload(did, &bytes).ok()?;
    let mut heights = [0.0f32; 81];
    for (o, h) in heights.iter_mut().zip(lb.height.iter()) {
        *o = table[usize::from(*h)];
    }
    Some((heights, lb.terrain))
}

/// The blocks within `r` of Holtburg, nearest first.
fn near_holtburg(r: i32) -> Vec<u16> {
    let (hx, hy) = dereth_world_data::landblock::block_xy(HOLTBURG);
    let mut out = Vec::new();
    for dx in -r..=r {
        for dy in -r..=r {
            let (x, y) = (hx + dx, hy + dy);
            if (0..255).contains(&x) && (0..255).contains(&y) {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // LINT-OK: both are in 0..255 on the line above. Not a float conversion.
                out.push((dx.abs().max(dy.abs()), ((x as u16) << 8) | (y as u16)));
            }
        }
    }
    out.sort_unstable();
    out.into_iter().map(|(_, b)| b).collect()
}

/// A terrain word's surface type.
const fn terrain_type(word: u16) -> u16 {
    (word >> 2) & 0x1F
}

/// A terrain word's scenery choice.
const fn scenery(word: u16) -> u16 {
    (word >> 11) & 0x1F
}

/// The nine stations, found in the dats where a station is defined by what the land is.
pub(crate) fn stations(store: &RetailDatStore) -> Vec<Station> {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let table = region.land_defs.land_height_table.clone();
    let town = Place::Free {
        block: HOLTBURG,
        x: 96.0,
        y: 72.0,
        eye: 1.8,
        yaw: 0.75,
        pitch: -0.05,
    };
    // The nearest block to Holtburg whose vertices are 30% to 70% water.
    let coast = near_holtburg(12)
        .into_iter()
        .find(|b| {
            block_record(store, &table, *b).is_some_and(|(_, words)| {
                let water = words
                    .iter()
                    .filter(|w| (16..=20).contains(&terrain_type(**w)))
                    .count();
                (24..=57).contains(&water)
            })
        })
        .unwrap_or(HOLTBURG);
    // The block near Holtburg with the most scenery-bearing vertices on dry land.
    let forest = near_holtburg(4)
        .into_iter()
        .filter(|b| *b != HOLTBURG)
        .max_by_key(|b| {
            block_record(store, &table, *b).map_or(0, |(_, words)| {
                words
                    .iter()
                    .filter(|w| scenery(**w) != 0 && terrain_type(**w) < 16)
                    .count()
            })
        })
        .unwrap_or(HOLTBURG);
    // The highest vertex within six blocks of Holtburg.
    let (vista, vx, vy, vz) = near_holtburg(6)
        .into_iter()
        .filter_map(|b| {
            let (h, _) = block_record(store, &table, b)?;
            let (i, top) = h
                .iter()
                .copied()
                .enumerate()
                .max_by(|a, c| a.1.total_cmp(&c.1))?;
            #[allow(clippy::cast_precision_loss)] // a vertex index, 0..9
            Some((b, (i / 9) as f32 * 24.0, (i % 9) as f32 * 24.0, top))
        })
        .max_by(|a, b| a.3.total_cmp(&b.3))
        .unwrap_or((HOLTBURG, 96.0, 96.0, 0.0));
    let _ = vz;
    // The block near Holtburg with the steepest rise: the eye at its lowest vertex, looking at
    // its highest.
    let (steep, (lx, ly, lz), (hx, hy, hz)) = near_holtburg(8)
        .into_iter()
        .filter_map(|b| {
            let (h, _) = block_record(store, &table, b)?;
            let at = |i: usize| {
                #[allow(clippy::cast_precision_loss)] // a vertex index, 0..9
                ((i / 9) as f32 * 24.0, (i % 9) as f32 * 24.0, h[i])
            };
            let lo = (0..81).min_by(|a, c| h[*a].total_cmp(&h[*c]))?;
            let hi = (0..81).max_by(|a, c| h[*a].total_cmp(&h[*c]))?;
            Some((b, at(lo), at(hi)))
        })
        .max_by(|a, b| {
            let rise = |(_, l, h): &(u16, (f32, f32, f32), (f32, f32, f32))| {
                (h.2 - l.2) / ((h.0 - l.0).hypot(h.1 - l.1)).max(24.0)
            };
            rise(a).total_cmp(&rise(b))
        })
        .unwrap_or((HOLTBURG, (0.0, 0.0, 0.0), (96.0, 96.0, 0.0)));
    let mut extra = Vec::new();
    // Light-shaft stations: the forest block in the low evening sun, looking each way round.
    for (k, name) in ["glade-0", "glade-1", "glade-2", "glade-3"]
        .into_iter()
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss)]
        let yaw = k as f32 * std::f32::consts::FRAC_PI_2;
        extra.push(Station {
            name,
            why: "the forest block in the evening sun, at eye height: light shafts through trees",
            place: Place::Free {
                block: forest,
                x: 96.0,
                y: 96.0,
                eye: 2.0,
                yaw,
                pitch: 0.06,
            },
            clock: Clock::SunUp(10.0),
            weather: false,
            land_radius: 3,
        });
    }
    for (k, name) in ["town-0", "town-1", "town-2", "town-3"]
        .into_iter()
        .enumerate()
    {
        #[allow(clippy::cast_precision_loss)]
        let yaw = k as f32 * std::f32::consts::FRAC_PI_2;
        extra.push(Station {
            name,
            why: "the town in the evening sun, at eye height: light shafts around buildings",
            place: Place::Free {
                block: HOLTBURG,
                x: 96.0,
                y: 72.0,
                eye: 1.8,
                yaw,
                pitch: 0.04,
            },
            clock: Clock::SunUp(8.0),
            weather: false,
            land_radius: 3,
        });
    }
    let mut all = vec![
        Station {
            name: "holtburg",
            why: "the town at noon on a Sunny day, at eye height: the reference daylight frame",
            place: town,
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "coast",
            why: "the nearest shore to Holtburg, a block that is 30-70% water, in the afternoon",
            place: Place::Free {
                block: coast,
                x: 96.0,
                y: 96.0,
                eye: 30.0,
                yaw: 0.6,
                pitch: -0.35,
            },
            clock: Clock::Sunny(0.62),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "shore",
            why: "the coast block from four metres up, looking along the water in the afternoon",
            place: Place::Free {
                block: coast,
                x: 96.0,
                y: 96.0,
                eye: 4.0,
                yaw: 0.6,
                pitch: -0.1,
            },
            clock: Clock::Sunny(0.62),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "shore-sun",
            why: "the coast block from four metres up, looking the other way in the afternoon",
            place: Place::Free {
                block: coast,
                x: 96.0,
                y: 96.0,
                eye: 4.0,
                yaw: 0.6 + std::f32::consts::PI,
                pitch: -0.1,
            },
            clock: Clock::Sunny(0.62),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "shore-dusk",
            why: "the coast block from four metres up in the evening",
            place: Place::Free {
                block: coast,
                x: 96.0,
                y: 96.0,
                eye: 4.0,
                yaw: 0.6 + std::f32::consts::PI,
                pitch: -0.08,
            },
            clock: Clock::SunUp(5.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "shore-eye",
            why: "the coast block at a standing eye's height, looking at the shoreline in the afternoon",
            place: Place::Free {
                block: coast,
                x: 96.0,
                y: 96.0,
                eye: 1.8,
                yaw: 0.6 + std::f32::consts::PI,
                pitch: -0.04,
            },
            clock: Clock::Sunny(0.62),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "shore-night",
            why: "the coast block from four metres up at midnight",
            place: Place::Free {
                block: coast,
                x: 96.0,
                y: 96.0,
                eye: 4.0,
                yaw: 0.6 + std::f32::consts::PI,
                pitch: -0.1,
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "coast-low",
            why: "the coast block from twelve metres up, the islands' shores near and far",
            place: Place::Free {
                block: coast,
                x: 96.0,
                y: 96.0,
                eye: 12.0,
                yaw: 0.6,
                pitch: -0.18,
            },
            clock: Clock::Sunny(0.62),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "forest",
            why: "the block near Holtburg with the most scenery on dry land, in the morning",
            place: Place::Free {
                block: forest,
                x: 96.0,
                y: 96.0,
                eye: 6.0,
                yaw: 0.3,
                pitch: -0.12,
            },
            clock: Clock::Sunny(0.35),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "vista",
            why: "the highest ground within six blocks of Holtburg, looking out at noon with a wide \
                  landscape window",
            place: Place::Free {
                block: vista,
                x: vx.clamp(8.0, 184.0),
                y: vy.clamp(8.0, 184.0),
                eye: 40.0,
                yaw: 2.4,
                pitch: -0.15,
            },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 11,
        },
        Station {
            name: "ridge",
            why: "low on the slope below the highest ground near Holtburg, looking up at its                   crest against the sky: the landscape's own silhouette",
            place: Place::Free {
                block: vista,
                x: if vx > 96.0 { vx - 70.0 } else { vx + 70.0 },
                y: if vy > 96.0 { vy - 70.0 } else { vy + 70.0 },
                eye: 1.8,
                yaw: ridge_yaw(vx, vy),
                pitch: 0.05,
            },
            clock: Clock::Sunny(0.4),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "mountain",
            why: "the foot of the steepest rise near Holtburg, looking up the slope at its top:                   the landscape's own silhouette on its roughest ground",
            place: Place::Free {
                block: steep,
                x: lx.clamp(4.0, 188.0),
                y: ly.clamp(4.0, 188.0),
                eye: 1.8,
                yaw: f32::atan2(hx - lx, hy - ly),
                pitch: ((hz - lz) / (hx - lx).hypot(hy - ly).max(1.0)).atan() * 0.5,
            },
            clock: Clock::Sunny(0.4),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "rain",
            why: "the town on the offline clock's own day, a Rainy one, with the weather layer and \
                  particles drawn",
            place: town,
            clock: Clock::Rainy(0.5),
            weather: true,
            land_radius: 3,
        },
        Station {
            name: "rain-coast",
            why: "the coast station's shore under the Rainy day's sky, weather drawn",
            place: Place::Free {
                block: coast,
                x: 96.0,
                y: 96.0,
                eye: 30.0,
                yaw: 0.6,
                pitch: -0.35,
            },
            clock: Clock::Rainy(0.62),
            weather: true,
            land_radius: 3,
        },
        Station {
            name: "rain-forest",
            why: "the forest station under the Rainy day's sky, weather drawn",
            place: Place::Free {
                block: forest,
                x: 96.0,
                y: 96.0,
                eye: 6.0,
                yaw: 0.3,
                pitch: -0.12,
            },
            clock: Clock::Rainy(0.5),
            weather: true,
            land_radius: 3,
        },
        Station {
            name: "rain-street",
            why: "the town's flat ground close in, looking down a little, in the rain",
            place: Place::Free {
                block: HOLTBURG,
                x: 128.0,
                y: 120.0,
                eye: 1.7,
                yaw: 0.75,
                pitch: -0.12,
            },
            clock: Clock::Rainy(0.5),
            weather: true,
            land_radius: 3,
        },
        Station {
            name: "dusk",
            why: "the town in the evening with the sun five degrees up: the low warm light",
            place: town,
            clock: Clock::SunUp(5.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "night",
            why: "the town at midnight, its lamps in view",
            place: town,
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "lamps-street",
            why: "Holtburg's north street at midnight, looking along its lamp posts",
            place: Place::Free {
                block: HOLTBURG,
                x: 120.0,
                y: 104.0,
                eye: 2.0,
                yaw: -0.75,
                pitch: -0.06,
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "lamps-west",
            why: "the west of Holtburg at midnight: lamps by the houses and a glowing lantern",
            place: Place::Free {
                block: HOLTBURG,
                x: 62.0,
                y: 108.0,
                eye: 2.2,
                yaw: 0.98,
                pitch: -0.06,
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "lamps-hill",
            why: "the lanterns and torches on the rise south of Holtburg at midnight",
            place: Place::Look {
                block: HOLTBURG,
                x: 118.0,
                y: 44.0,
                eye: 2.0,
                at: [132.0, 20.0, 96.0],
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "lamps-above",
            why: "Holtburg at midnight from above: the pools of its lamps",
            place: Place::Free {
                block: HOLTBURG,
                x: 96.0,
                y: 58.0,
                eye: 26.0,
                yaw: -0.2,
                pitch: -0.38,
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "lamps-torch",
            why: "four flames and two hung lamps on the block south-east of Holtburg at midnight",
            place: Place::Look {
                block: 0xAAB3,
                x: 20.0,
                y: 60.0,
                eye: 4.0,
                at: [36.0, 85.0, 130.0],
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "lamps-lantern",
            why: "the lanterns by the houses on Holtburg's north-east corner at midnight",
            place: Place::Look {
                block: HOLTBURG,
                x: 136.0,
                y: 116.0,
                eye: 1.8,
                at: [153.0, 130.0, 69.0],
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "lanterns-facade",
            why: "a building hung with lanterns on two floors, in the town on block E74E, at midnight",
            place: Place::Look {
                block: 0xE74E,
                x: 155.0,
                y: 38.0,
                eye: 1.8,
                at: [156.0, 61.0, 37.0],
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "lanterns-square",
            why: "a square ringed by hanging lanterns and lamp posts, on block E74E, at midnight",
            place: Place::Look {
                block: 0xE74E,
                x: 58.0,
                y: 36.0,
                eye: 2.0,
                at: [58.0, 13.0, 34.5],
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "lamps-dusk",
            why: "Holtburg's north street as the sun sets: the lamps coming on",
            place: Place::Free {
                block: HOLTBURG,
                x: 120.0,
                y: 104.0,
                eye: 2.0,
                yaw: -0.75,
                pitch: -0.06,
            },
            clock: Clock::SunUp(2.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "shop-north",
            why: "on the porch of a Holtburg shop at noon, looking in at a window and out through \
                  the window across the room at the land beyond",
            place: Place::Look {
                block: HOLTBURG,
                x: 89.09,
                y: 141.5,
                eye: 2.2,
                at: [89.09, 112.0, f32::NAN],
            },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "shop-south",
            why: "the same shop from its other porch, looking in and out the other way",
            place: Place::Look {
                block: HOLTBURG,
                x: 89.09,
                y: 121.5,
                eye: 2.2,
                at: [89.09, 152.0, f32::NAN],
            },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "doorway",
            why: "a body in a house's doorway with its camera in the room: a frame split between \
                  outdoors and an interior",
            place: Place::Body {
                cell: DOORWAY_CELL,
                origin: DOORWAY_ORIGIN,
                yaw: 90.0,
            },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "indoor",
            why: "a body inside a room of a town building: an interior frame",
            place: Place::Body {
                cell: INDOOR_CELL,
                origin: Vec3::ZERO,
                yaw: 0.0,
            },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        },
    ];
    all.extend(extra);
    all
}

/// The stations the classic and modern interfaces' frames are compared with the base build's
/// at: outdoors by day, at dusk and at night, in town and the wilds, at the lamps, at a doorway
/// split between outdoors and a room, indoors, underground, round a body and in water.
pub(crate) fn isolation_stations(store: &RetailDatStore) -> Vec<Station> {
    const NAMES: [&str; 19] = [
        "holtburg",
        "forest",
        "vista",
        "ridge",
        "mountain",
        "dusk",
        "night",
        "lamps-street",
        "lanterns-square",
        "doorway",
        "indoor",
        "body",
        "square",
        "figure",
        "wade",
        "trees-n-noon",
        "glade-1",
        "town-1",
        "dungeon",
    ];
    let all: Vec<Station> = stations(store)
        .into_iter()
        .chain(close_stations(store))
        .collect();
    NAMES
        .iter()
        .map(|n| {
            all.iter()
                .find(|s| s.name == *n)
                .cloned()
                .unwrap_or_else(|| panic!("no station {n}"))
        })
        .collect()
}

/// The heading from the ridge station's eye back up to the crest at block-local `(vx, vy)`.
fn ridge_yaw(vx: f32, vy: f32) -> f32 {
    let dx = if vx > 96.0 { 70.0 } else { -70.0 };
    let dy = if vy > 96.0 { 70.0 } else { -70.0 };
    f32::atan2(dx, dy)
}

/// Close views of objects, captured by the instrument but not part of the nine stations the
/// claims run at: a body standing in the town with its chase camera, and the forest at the
/// height of a walker.
pub(crate) fn close_stations(store: &RetailDatStore) -> Vec<Station> {
    let forest = stations(store)
        .into_iter()
        .find(|s| s.name == "forest")
        .map_or(HOLTBURG, |s| s.landblock());
    // A body standing on open sloping ground, facing up the slope, seen from its chase camera:
    // the ground round a walker against the ground farther out.
    let on_foot = |name: &'static str, why: &'static str| {
        stations(store)
            .into_iter()
            .find(|s| s.name == name)
            .map(|s| match s.place {
                Place::Free {
                    block, x, y, yaw, ..
                } => Station {
                    name: if name == "ridge" { "slope" } else { "rise" },
                    why,
                    place: Place::Ground {
                        block,
                        x,
                        y,
                        yaw: yaw.to_degrees(),
                    },
                    clock: s.clock,
                    weather: false,
                    land_radius: s.land_radius,
                },
                _ => s,
            })
    };
    let walkers: Vec<Station> = [
        on_foot(
            "ridge",
            "a body on the slope below the highest ground near Holtburg, facing the crest, seen              from its chase camera",
        ),
        on_foot(
            "mountain",
            "a body at the foot of the steepest rise near Holtburg, facing up it, seen from its              chase camera",
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut out = tree_stations(forest);
    out.extend([
        Station {
            name: "body",
            why: "a body at a house's front door, facing it, seen from its chase camera at noon",
            place: Place::Body {
                cell: DOORWAY_CELL,
                origin: DOORWAY_ORIGIN,
                yaw: 270.0,
            },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "square",
            why: "a body standing in the open in the town at noon, feet on the ground, seen from its                   chase camera",
            place: Place::Ground {
                block: HOLTBURG,
                x: 96.0,
                y: 72.0,
                yaw: 45.0,
            },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "face",
            why: "a body standing in the open in the town at noon, seen from the front at arm's length: its face and its clothes",
            place: Place::Facing {
                block: HOLTBURG,
                x: 96.0,
                y: 72.0,
                yaw: 45.0,
                dist: 1.6,
                eye: 1.6,
            },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "figure",
            why: "the same body seen whole from the front, a few paces off",
            place: Place::Facing {
                block: HOLTBURG,
                x: 96.0,
                y: 72.0,
                yaw: 45.0,
                dist: 3.6,
                eye: 1.4,
            },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "square-night",
            why: "the body in the open in the town at midnight, seen from its chase camera: the ground round a walker after dark",
            place: Place::Ground {
                block: HOLTBURG,
                x: 96.0,
                y: 72.0,
                yaw: 45.0,
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "face-night",
            why: "the same body at midnight seen from the front at arm's length: lit as a walker is after dark",
            place: Place::Facing {
                block: HOLTBURG,
                x: 96.0,
                y: 72.0,
                yaw: 45.0,
                dist: 1.6,
                eye: 1.6,
            },
            clock: Clock::Sunny(0.0),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "grove",
            why: "the forest block at a walker's eye height, trees and rocks close by",
            place: Place::Free {
                block: forest,
                x: 96.0,
                y: 96.0,
                eye: 1.8,
                yaw: 0.3,
                pitch: 0.02,
            },
            clock: Clock::Sunny(0.4),
            weather: false,
            land_radius: 3,
        },
    ]);
    out.extend(walkers);
    out.push(Station {
        name: "hillside",
        why:
            "the hillside south of Holtburg in the evening, looking down it north-east toward the \
              town as the Horizon interface's orbit camera does: beside the player a steep \
              triangle of the landscape turned away from the low sun",
        place: Place::Orbit {
            block: HOLTBURG,
            feet: Vec3::new(116.109_28, 64.769_8, 84.222_62),
            // The heading of a body turned 2 * atan2(-0.321393, 0.946946) about the vertical.
            yaw: -0.654_4,
        },
        clock: Clock::Sunny(0.849),
        weather: false,
        land_radius: 3,
    });
    out.push(Station {
        name: "dungeon",
        why: "a body standing in a dungeon's corridor: a frame wholly underground",
        place: Place::Body {
            cell: 0x019E_0114,
            origin: Vec3::new(10.0, -40.0, 0.0),
            yaw: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    });
    out.extend(grass_stations(store));
    out.extend(wade_stations(store));
    out
}

/// A body on the coast block's beach facing open water, a few metres short of the waterline, so
/// a walk carries it into the shallows and on; and the same water from high over it, steeply,
/// where every cell and block edge of the surface is in view.
fn wade_stations(store: &RetailDatStore) -> Vec<Station> {
    let Some(coast) = stations(store)
        .into_iter()
        .find(|s| s.name == "coast")
        .map(|s| s.landblock())
    else {
        return Vec::new();
    };
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let table = region.land_defs.land_height_table.clone();
    let Some((_, words)) = block_record(store, &table, coast) else {
        return Vec::new();
    };
    let water = |e: i32, n: i32| {
        (0..9).contains(&e)
            && (0..9).contains(&n)
            && (16..=20).contains(&terrain_type(words[(e * 9 + n) as usize]))
    };
    let land = |e: i32, n: i32| (0..9).contains(&e) && (0..9).contains(&n) && !water(e, n);
    // A land vertex with three water vertices straight ahead of it, the land behind it.
    let found = (1..8)
        .flat_map(|e| (1..8).map(move |n| (e, n)))
        .flat_map(|(e, n)| {
            [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .map(move |d| (e, n, d))
        })
        .find(|&(e, n, (dx, dy))| {
            land(e, n) && land(e - dx, n - dy) && (1..=3).all(|k| water(e + dx * k, n + dy * k))
        });
    let Some((e, n, (dx, dy))) = found else {
        return Vec::new();
    };
    #[allow(clippy::cast_precision_loss)] // LINT-OK: vertex indices, 0..9
    let (x, y) = (
        e as f32 * 24.0 - dx as f32 * 4.0,
        n as f32 * 24.0 - dy as f32 * 4.0,
    );
    #[allow(clippy::cast_precision_loss)] // LINT-OK: -1..1
    let yaw = (-(dx as f32)).atan2(dy as f32).to_degrees();
    #[allow(clippy::cast_precision_loss)] // LINT-OK: -1..1
    let (wx, wy) = (x + dx as f32 * 30.0, y + dy as f32 * 30.0);
    vec![
        Station {
            name: "wade",
            why: "a body on the coast block's beach facing the water, seen from its chase camera in                   the afternoon: walked, it wades into the shallows",
            place: Place::Ground { block: coast, x, y, yaw },
            clock: Clock::Sunny(0.62),
            weather: false,
            land_radius: 3,
        },
        Station {
            name: "water-top",
            why: "the coast block's water from sixty metres up, looking steeply down: every cell                   and block edge of the surface in view",
            place: Place::Look {
                block: coast,
                x: x - dx as f32 * 10.0,
                y: y - dy as f32 * 10.0,
                eye: 60.0,
                at: [wx, wy, f32::NAN],
            },
            clock: Clock::Sunny(0.62),
            weather: false,
            land_radius: 3,
        },
    ]
}

/// The forest block's trees from a standing height, looking each way round, at noon, in the
/// evening and at night: the scenery's leaf cards against the sun, across it and away from it.
fn tree_stations(forest: u16) -> Vec<Station> {
    const NAMES: [[&str; 4]; 3] = [
        [
            "trees-n-noon",
            "trees-e-noon",
            "trees-s-noon",
            "trees-w-noon",
        ],
        [
            "trees-n-dusk",
            "trees-e-dusk",
            "trees-s-dusk",
            "trees-w-dusk",
        ],
        [
            "trees-n-night",
            "trees-e-night",
            "trees-s-night",
            "trees-w-night",
        ],
    ];
    let clocks = [Clock::Sunny(0.5), Clock::SunUp(8.0), Clock::Sunny(0.0)];
    let mut out = Vec::new();
    for (names, clock) in NAMES.iter().zip(clocks) {
        for (k, name) in names.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let yaw = k as f32 * std::f32::consts::FRAC_PI_2;
            out.push(Station {
                name,
                why: "the forest block's trees at a standing height, one heading of four",
                place: Place::Free {
                    block: forest,
                    x: 96.0,
                    y: 96.0,
                    eye: 2.5,
                    yaw,
                    pitch: 0.04,
                },
                clock,
                weather: false,
                land_radius: 3,
            });
        }
    }
    out
}

/// Whether terrain word `w` is grass with no road on it.
const fn grassy(w: u16) -> bool {
    w & 0x3 == 0 && matches!(terrain_type(w), 1 | 3 | 9)
}

/// Bodies in the grass near Holtburg, and a road through it: where grass meets the things that
/// stand in it and the ground it must keep off.
fn grass_stations(store: &RetailDatStore) -> Vec<Station> {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let table = region.land_defs.land_height_table.clone();
    let blocks: Vec<(u16, [u16; 81])> = near_holtburg(3)
        .into_iter()
        .filter(|b| *b != HOLTBURG)
        .filter_map(|b| block_record(store, &table, b).map(|(_, w)| (b, w)))
        .collect();
    let at = |w: &[u16; 81], e: i32, n: i32| {
        #[allow(clippy::cast_sign_loss)] // LINT-OK: 0..9, checked by the callers
        w[(e * 9 + n) as usize]
    };
    #[allow(clippy::cast_precision_loss)] // LINT-OK: vertex indices, 0..9
    let metres = |e: i32, n: i32| (e as f32 * 24.0, n as f32 * 24.0);
    let mut out = Vec::new();
    // A vertex whose 5x5 neighbourhood is all grass.
    let meadow = blocks.iter().find_map(|(b, w)| {
        (2..7)
            .flat_map(|e| (2..7).map(move |n| (e, n)))
            .find_map(|(e, n)| {
                let all = (-2..=2).all(|i| (-2..=2).all(|j| grassy(at(w, e + i, n + j))));
                all.then_some((*b, e, n))
            })
    });
    if let Some((block, e, n)) = meadow {
        let (x, y) = metres(e, n);
        for (name, yaw) in [("meadow-body", 30.0), ("meadow-body-sun", 210.0)] {
            out.push(Station {
                name,
                why: "a body standing in open grassland at noon, seen from its chase camera:                       blades around its feet and behind it",
                place: Place::Ground {
                    block,
                    x: x + 3.0,
                    y: y + 3.0,
                    yaw,
                },
                clock: Clock::Sunny(0.5),
                weather: false,
                land_radius: 3,
            });
        }
    }
    // A straight run of road with grass on both sides.
    let road = blocks.iter().find_map(|(b, w)| {
        (2..7)
            .flat_map(|e| (2..7).map(move |n| (e, n)))
            .find_map(|(e, n)| {
                if at(w, e, n) & 0x3 == 0 {
                    return None;
                }
                let road = |i, j| at(w, e + i, n + j) & 0x3 != 0;
                let grass = |i, j| grassy(at(w, e + i, n + j));
                if road(0, -1) && road(0, 1) && grass(-1, 0) && grass(1, 0) {
                    Some((*b, e, n, 0.0f32))
                } else if road(-1, 0) && road(1, 0) && grass(0, -1) && grass(0, 1) {
                    Some((*b, e, n, 90.0f32))
                } else {
                    None
                }
            })
    });
    if let Some((block, e, n, yaw)) = road {
        let (x, y) = metres(e, n);
        out.push(Station {
            name: "road-body",
            why: "a body on a road through grass at noon, facing along it: grass must keep off the                   road's painted shape",
            place: Place::Ground { block, x, y, yaw },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        });
        // Across the road, from a few metres up, a little off to one side.
        let (ox, oy) = if yaw == 0.0 {
            (14.0, -10.0)
        } else {
            (-10.0, 14.0)
        };
        out.push(Station {
            name: "road-across",
            why: "the same road from three metres up beside it, looking across: the grass's verge",
            place: Place::Look {
                block,
                x: x + ox,
                y: y + oy,
                eye: 3.0,
                at: [x, y + if yaw == 0.0 { 6.0 } else { 0.0 }, f32::NAN],
            },
            clock: Clock::Sunny(0.5),
            weather: false,
            land_radius: 3,
        });
    }
    out
}

/// The absolute second of `clock`.
fn game_time(store: &RetailDatStore, clock: Clock) -> Option<f64> {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let day = f64::from(region.game_time.day_length);
    let at = |k: u32, f: f32| {
        f64::from(k).mul_add(day, f64::from(f) * day) - region.game_time.zero_time_of_year
    };
    let group = |abs: f64| {
        let mut c = dereth_client_runtime::game_clock::GameClock::new(&region);
        c.set_game_time(0.0, abs);
        dereth_world_render::sky::present_day_group(&region, c.current_year, c.current_day)
            .map(|g| g.day_name.clone())
            .unwrap_or_default()
    };
    let sunny_day = (1..720).find(|k| group(at(*k, 0.5)).starts_with("Sunny"))?;
    match clock {
        Clock::Rainy(_) => None,
        Clock::Sunny(f) => Some(at(sunny_day, f)),
        Clock::SunUp(degrees) => {
            let mut c = dereth_client_runtime::game_clock::GameClock::new(&region);
            c.set_game_time(0.0, at(sunny_day, 0.5));
            let g = dereth_world_render::sky::present_day_group(
                &region,
                c.current_year,
                c.current_day,
            )?;
            let elevation = |t: f32| {
                let s = dereth_world_render::sky::get_lighting(g, t).sun_vec;
                let len = (s.x * s.x + s.y * s.y + s.z * s.z).sqrt().max(1e-6);
                math::asinf(s.z / len).to_degrees()
            };
            let f = (500..1000)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let t = i as f32 / 1000.0;
                    t
                })
                .find(|t| elevation(*t) <= degrees)?;
            Some(at(sunny_day, f))
        }
    }
}

/// A point a body can stand at inside `cell`.
fn point_in(store: &RetailDatStore, cell: u32) -> Option<Vec3> {
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: the high half of a cell id
    let block = (cell >> 16) as u16;
    let d = EnvCellLoader::new()
        .load_block(store, block)
        .into_iter()
        .find(|d| d.id.0 == cell)?;
    let g = dereth_world_data::env_cells::physics_geometry(&d);
    let bsp = g.cell_bsp.as_ref()?;
    for zi in -24i32..=24 {
        for i in -40i32..=40 {
            for j in -40i32..=40 {
                #[allow(clippy::cast_precision_loss)]
                let local = Vec3::new(i as f32 * 0.5, j as f32 * 0.5, zi as f32 * 0.5);
                if !bsp.point_inside_cell_bsp(local) {
                    continue;
                }
                if g.physics_bsp
                    .as_ref()
                    .is_some_and(|b| b.point_intersects_solid(local))
                {
                    continue;
                }
                return Some(dereth_physics::math::localtoglobal(&g.frame, local));
            }
        }
    }
    None
}

/// A loaded station, stepped one frame at a time.
pub(crate) struct Shot {
    pub scene: WorldScene,
    store: Arc<RetailDatStore>,
    stream: ObjectStream,
    now: f64,
    body: bool,
    /// The camera a facing station draws from (position, yaw, pitch), over the chase camera.
    front: Option<(Vec3, f32, f32)>,
    /// A camera held behind the body instead of its chase camera: metres back, metres up, the
    /// pitch (radians) and the heading (radians from north).
    pub follow: Option<[f32; 4]>,
}

impl Shot {
    /// Load `station` on `gpu`, with the degrade governor off so the frame does not follow the
    /// frame rate.
    pub(crate) fn open(store: &Arc<RetailDatStore>, gpu: &mut Gpu, station: &Station) -> Self {
        Self::open_blending(store, gpu, station, false)
    }

    /// [`Self::open`], with the landscape's layers blended as it is drawn when `splat` (the
    /// client's own default), or from the composites built for its cells (the scene's).
    pub(crate) fn open_blending(
        store: &Arc<RetailDatStore>,
        gpu: &mut Gpu,
        station: &Station,
        splat: bool,
    ) -> Self {
        let body = matches!(
            station.place,
            Place::Body { .. } | Place::Ground { .. } | Place::Facing { .. }
        );
        let mut front = None;
        // `DERETH_HIFI_SUN_UP=<degrees>`: every station in the evening, the sun that far up.
        let clock = std::env::var("DERETH_HIFI_SUN_UP")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .map_or(station.clock, Clock::SunUp);
        let mut cfg = SceneConfig {
            landblock: station.landblock(),
            character: body,
            particles: station.weather,
            land_radius: station.land_radius,
            time_of_day: match clock {
                Clock::Rainy(f) => Some(f),
                _ => None,
            },
            game_time: game_time(store, clock),
            ..SceneConfig::default()
        };
        cfg.render.automatic_degrades = false;
        cfg.terrain_splat = splat;
        let mut scene = WorldScene::load(store, gpu, cfg).expect("the station's scene loads");
        match station.place {
            Place::Free {
                block,
                x,
                y,
                eye,
                yaw,
                pitch,
            } => {
                let region =
                    dereth_world_data::landblock::load_region(store).expect("the region decodes");
                let land =
                    dereth_world_data::land_source::DatLandSource::new(Arc::clone(store), &region)
                        .expect("the land source opens");
                let ground = land.ground_height(LandblockId(block), x, y).unwrap_or(0.0);
                scene.camera.position = Vec3::new(x, y, ground + eye);
                scene.camera.yaw = yaw;
                scene.camera.pitch = pitch;
                scene.set_weather_enabled(station.weather);
            }
            Place::Ground { block, x, y, yaw }
            | Place::Facing {
                block, x, y, yaw, ..
            } => {
                let region =
                    dereth_world_data::landblock::load_region(store).expect("the region decodes");
                let land =
                    dereth_world_data::land_source::DatLandSource::new(Arc::clone(store), &region)
                        .expect("the land source opens");
                let ground = land.ground_height(LandblockId(block), x, y).unwrap_or(0.0);
                if let Place::Facing { dist, eye, .. } = station.place {
                    let r = yaw.to_radians();
                    let (fx, fy) = (-math::sinf(r), math::cosf(r));
                    let cam = Vec3::new(x + fx * dist, y + fy * dist, ground + eye);
                    let head = ground + 1.55;
                    front = Some((cam, r + std::f32::consts::PI, (head - cam.z).atan2(dist)));
                }
                scene
                    .attach_character(store, &region, gpu)
                    .expect("the body is created");
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // LINT-OK: a block-local position, 0..192, to its outdoor cell's index
                let cell =
                    (u32::from(block) << 16) | ((x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1);
                let r = yaw.to_radians();
                let q = Quat::new(math::cosf(r * 0.5), 0.0, 0.0, math::sinf(r * 0.5));
                scene
                    .character
                    .as_mut()
                    .expect("a body")
                    .teleport(Position::new(
                        CellId(cell),
                        Frame::new(Vec3::new(x, y, ground), q),
                    ));
            }
            Place::Look {
                block,
                x,
                y,
                eye,
                at,
            } => {
                let region =
                    dereth_world_data::landblock::load_region(store).expect("the region decodes");
                let land =
                    dereth_world_data::land_source::DatLandSource::new(Arc::clone(store), &region)
                        .expect("the land source opens");
                let ground = land.ground_height(LandblockId(block), x, y).unwrap_or(0.0);
                let z = ground + eye;
                scene.camera.position = Vec3::new(x, y, z);
                // A target height that is not a number is the ground's there.
                let target = if at[2].is_nan() {
                    land.ground_height(LandblockId(block), at[0], at[1])
                        .unwrap_or(ground)
                } else {
                    at[2]
                };
                let (dx, dy, dz) = (at[0] - x, at[1] - y, target - z);
                scene.camera.yaw = (-dx).atan2(dy);
                scene.camera.pitch = dz.atan2((dx * dx + dy * dy).sqrt());
                scene.set_weather_enabled(station.weather);
            }
            Place::Orbit { feet, yaw, .. } => {
                use dereth_client_runtime::camera::FreeCamera;
                use dereth_client_runtime::orbit::{OrbitCamera, DEFAULT_DISTANCE, DEFAULT_PITCH};
                let pivot = OrbitCamera::pivot(feet);
                let look = FreeCamera::new(Vec3::ZERO, yaw, DEFAULT_PITCH).forward();
                scene.camera.position = Vec3::new(
                    pivot.x - look.x * DEFAULT_DISTANCE,
                    pivot.y - look.y * DEFAULT_DISTANCE,
                    pivot.z - look.z * DEFAULT_DISTANCE,
                );
                scene.camera.yaw = yaw;
                scene.camera.pitch = DEFAULT_PITCH;
                scene.set_weather_enabled(station.weather);
            }
            Place::Body { cell, origin, yaw } => {
                let region =
                    dereth_world_data::landblock::load_region(store).expect("the region decodes");
                scene
                    .attach_character(store, &region, gpu)
                    .expect("the body is created");
                let at = if origin == Vec3::ZERO {
                    point_in(store, cell).expect("the room has a standable point")
                } else {
                    origin
                };
                let r = yaw.to_radians();
                let q = Quat::new(math::cosf(r * 0.5), 0.0, 0.0, math::sinf(r * 0.5));
                scene
                    .character
                    .as_mut()
                    .expect("a body")
                    .teleport(Position::new(CellId(cell), Frame::new(at, q)));
            }
        }
        Self {
            scene,
            store: Arc::clone(store),
            stream: ObjectStream::new(),
            now: 0.0,
            body,
            front,
            follow: None,
        }
    }

    /// Move the station's viewer `metres` along its heading from where the station puts it,
    /// on the ground: the free camera at its height over the ground, or the body.
    pub(crate) fn walk_to(&mut self, station: &Station, metres: f32) {
        let region =
            dereth_world_data::landblock::load_region(&self.store).expect("the region decodes");
        let land =
            dereth_world_data::land_source::DatLandSource::new(Arc::clone(&self.store), &region)
                .expect("the land source opens");
        match station.place {
            Place::Free {
                block,
                x,
                y,
                eye,
                yaw,
                ..
            } => {
                let (x, y) = (x - math::sinf(yaw) * metres, y + math::cosf(yaw) * metres);
                // A walk may leave the station's block: the ground is the next block's there.
                let (bx, by) = ((x / 192.0).floor(), (y / 192.0).floor());
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // LINT-OK: a walk of a few blocks at most, inside the map
                let next = ((i32::from(block >> 8) + bx as i32) << 8
                    | (i32::from(block & 0xFF) + by as i32)) as u16;
                let ground = land
                    .ground_height(LandblockId(next), x - bx * 192.0, y - by * 192.0)
                    .unwrap_or(0.0);
                self.scene.camera.position = Vec3::new(x, y, ground + eye);
            }
            Place::Ground { block, x, y, yaw } => {
                let r = yaw.to_radians();
                let (x, y) = (x - math::sinf(r) * metres, y + math::cosf(r) * metres);
                let (x, y) = (x.clamp(0.5, 191.5), y.clamp(0.5, 191.5));
                let ground = land.ground_height(LandblockId(block), x, y).unwrap_or(0.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // LINT-OK: a block-local position, 0..192, to its outdoor cell's index
                let cell =
                    (u32::from(block) << 16) | ((x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1);
                let q = Quat::new(math::cosf(r * 0.5), 0.0, 0.0, math::sinf(r * 0.5));
                if let Some(c) = self.scene.character.as_mut() {
                    c.teleport(Position::new(
                        CellId(cell),
                        Frame::new(Vec3::new(x, y, ground), q),
                    ));
                }
            }
            _ => {}
        }
    }

    /// Step the station one frame and draw it, leaving the frame on the device.
    pub(crate) fn step(&mut self, gpu: &mut Gpu) {
        self.now += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        let now = LocalTime(self.now);
        self.scene
            .sync_objects(&self.store, gpu, &mut self.stream)
            .expect("sync_objects");
        if let Some(c) = self.scene.character.as_mut() {
            self.stream.sync_physics_at(&self.store, &mut c.world, now);
        }
        self.scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            now,
            1.0 / 30.0,
        );
        if self.body {
            dereth_client_runtime::camera::update_viewer(
                &mut self.scene,
                dereth_client_runtime::camera::CameraInput::default(),
                now,
                1.0 / 30.0,
            );
            if let (Some([back, up, pitch, yaw]), Some(c)) =
                (self.follow, self.scene.character.as_ref())
            {
                let body = c.render_frame().origin;
                let (ax, ay) = (-math::sinf(yaw), math::cosf(yaw));
                self.scene.camera.position =
                    Vec3::new(body.x - ax * back, body.y - ay * back, body.z + up);
                self.scene.camera.yaw = yaw;
                self.scene.camera.pitch = pitch;
            }
        }
        if let Some((position, yaw, pitch)) = self.front {
            self.scene.camera.position = position;
            self.scene.camera.yaw = yaw;
            self.scene.camera.pitch = pitch;
        }
        self.scene.stream(&self.store, gpu).expect("stream");
        self.draw(gpu);
    }

    /// Draw the station again as it stands, without stepping it.
    pub(crate) fn draw(&mut self, gpu: &mut Gpu) {
        self.scene
            .reserve_upload_arena(gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        self.scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
    }

    /// Apply the scene's own preferences poll, as the application does once a frame.
    pub(crate) fn poll(&mut self, gpu: &mut Gpu) {
        self.scene
            .update_from_preferences(&self.store, gpu)
            .expect("the preferences poll");
    }
}

/// Step `shot` one frame, as [`Shot::step`] does, and what the device and the scene counted for
/// it, as one line naming each count, so two builds' frames can be compared count for count: the
/// draws and the frames presented in the step, the textures and texture slots held after it, the
/// render passes the device encoded (`-` where it does not count them) and the scene's census.
pub(crate) fn counted_step(shot: &mut Shot, gpu: &mut Gpu) -> String {
    let calls = gpu.draw_calls();
    let passes = gpu.passes_encoded();
    let stamp = gpu.frame_stamp();
    shot.step(gpu);
    let passes = gpu
        .passes_encoded()
        .zip(passes)
        .map_or_else(|| "-".to_owned(), |(a, b)| (a - b).to_string());
    format!(
        "draw_calls={} live_textures={} descriptors={:?} passes={passes} frames={} census={:?}",
        gpu.draw_calls() - calls,
        gpu.live_textures(),
        gpu.descriptor_stats(),
        gpu.frame_stamp() - stamp,
        dereth_client_runtime::present::Scene::census(&shot.scene),
    )
}

/// The line `name` begins in a capture's list (`<name> <value>` a line), without the name.
pub(crate) fn listed<'a>(list: &'a str, name: &str) -> Option<&'a str> {
    list.lines()
        .find_map(|l| l.strip_prefix(&format!("{name} ")))
        .map(str::trim)
}

/// The last presented frame, RGBA.
pub(crate) fn capture(gpu: &mut Gpu) -> Vec<u8> {
    gpu.capture().expect("capture").to_rgba()
}

/// Write `rgba` as a PNG at `path`.
pub(crate) fn write_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the PNG's folder");
    }
    let f = std::fs::File::create(path).expect("create the PNG");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("the PNG header");
    w.write_image_data(rgba).expect("the PNG data");
}

/// A PNG's RGBA and size, when the file reads.
pub(crate) fn read_png(path: &std::path::Path) -> Option<(u32, u32, Vec<u8>)> {
    let f = std::fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(std::io::BufReader::new(f));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    Some((info.width, info.height, buf))
}

/// How many bytes of two pictures differ.
pub(crate) fn moved(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count() + a.len().abs_diff(b.len())
}
