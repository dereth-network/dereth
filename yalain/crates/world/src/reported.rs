//! What a client reports of its player's place, and what the world takes of it. A report is taken
//! when it passes four checks, each measured from the 1,806,692 reports of January 2017, none of
//! which fails one:
//!
//! - **Reach:** it lies within [`AT_ONCE`] meters plus [`SPEED`] meters a second of the place last
//!   taken, by the time between their arrivals. The most the recordings needed: 26.2 m at that
//!   speed.
//! - **Steady going:** reports each within [`STEADY`] seconds of the last cover at most
//!   [`IN_A_WINDOW`] meters in any [`WINDOW`] seconds. The most recorded: 320 m.
//! - **Rooms:** an interior cell is entered from another only where a doorway joins them or the
//!   first sees it, and from outdoors only where it has a doorway to the outdoors. A room the world's
//!   records lack passes either way, as some the 2017 clients stood in are. Of 14,720 recorded
//!   steps into a room, none went another way.
//! - **The land:** a place outdoors is at most [`UNDER_THE_LAND`] meters below the land. None of
//!   1,486,997 recorded was below it at all.
//!
//! A place the server puts the player at (entering the world, a trip's end) starts the reckoning
//! again from there and then.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use auberean_physics::land::Land;
use auberean_world::records::WorldRecords;
use yalain_store::StoredPosition;

use crate::Ground;

/// Meters a report may go however soon after the last, and meters a second more.
pub const AT_ONCE: f32 = 30.0;
pub const SPEED: f32 = 15.0;
/// Seconds within which a report follows the last for it to count toward the steady going.
pub const STEADY: f64 = 1.0;
/// Seconds of steady going, and the meters they may cover.
pub const WINDOW: f64 = 10.0;
pub const IN_A_WINDOW: f32 = 400.0;
/// How far below the land a place outdoors may be, in meters.
pub const UNDER_THE_LAND: f32 = 0.5;

/// The other side of a doorway that leads outdoors.
pub const OUTDOORS: u32 = auberean_physics::cells::OUTDOORS;

/// Why a report was not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Further than its time since the last allows.
    Reach,
    /// More than steady going covers in a window.
    Steady,
    /// Into a room by no doorway or sight.
    Rooms,
    /// Below the land.
    Land,
}

/// The reckoning of a player's reports: when the last place was taken, and the steady going of
/// the last [`WINDOW`] seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Travel {
    since: f64,
    /// Each step of the steady going: when it began and its meters.
    steps: VecDeque<(f64, f32)>,
    total: f64,
}

impl Travel {
    /// A player the server put somewhere at `now`.
    #[must_use]
    pub fn placed(now: f64) -> Self {
        Self {
            since: now,
            steps: VecDeque::new(),
            total: 0.0,
        }
    }

    /// When the last place was taken, or the server put the player there.
    #[must_use]
    pub fn since(&self) -> f64 {
        self.since
    }

    /// Whether a step of `meters`, arriving at `now`, is within reach and steady going.
    ///
    /// # Errors
    /// Why it is not.
    pub fn reaches(&self, meters: f32, now: f64) -> Result<(), Refused> {
        let gap = (now - self.since).max(0.0);
        if f64::from(meters) > f64::from(AT_ONCE) + f64::from(SPEED) * gap {
            return Err(Refused::Reach);
        }
        if gap <= STEADY && self.within(now) + f64::from(meters) > f64::from(IN_A_WINDOW) {
            return Err(Refused::Steady);
        }
        Ok(())
    }

    /// The meters of steady going whose steps began in the window ending `now`.
    fn within(&self, now: f64) -> f64 {
        let gone: f64 = self
            .steps
            .iter()
            .take_while(|(began, _)| *began < now - WINDOW)
            .map(|(_, m)| f64::from(*m))
            .sum();
        self.total - gone
    }

    /// Take a step of `meters` arriving at `now`.
    pub fn take(&mut self, meters: f32, now: f64) {
        if now - self.since > STEADY {
            self.steps.clear();
            self.total = 0.0;
        } else if meters > 0.0 {
            self.steps.push_back((self.since, meters));
            self.total += f64::from(meters);
        }
        while let Some(&(began, m)) = self.steps.front() {
            if began >= now - WINDOW {
                break;
            }
            self.steps.pop_front();
            self.total -= f64::from(m);
        }
        if self.steps.is_empty() {
            self.total = 0.0;
        }
        self.since = self.since.max(now);
    }
}

fn indoors(cell: u32) -> bool {
    cell & 0xFFFF >= 0x100
}

/// Whether a player may go from a place in cell `from` to one in cell `to` as the rooms allow,
/// with what `ground` knows of them (none: everything passes).
#[must_use]
pub fn way_in(ground: Option<&dyn Ground>, from: u32, to: u32) -> bool {
    let Some(g) = ground else {
        return true;
    };
    if from == to || !indoors(to) {
        return true;
    }
    let Some(into) = g.joins(to) else {
        return true;
    };
    if !indoors(from) {
        return into.contains(&OUTDOORS);
    }
    let Some(out) = g.joins(from) else {
        return true;
    };
    out.contains(&to) || into.contains(&from) || g.sees(from).is_some_and(|s| s.contains(&to))
}

/// Whether a place outdoors lies further below the land than a body stands.
#[must_use]
pub fn under_the_land(ground: Option<&dyn Ground>, at: &StoredPosition) -> bool {
    if indoors(at.cell) {
        return false;
    }
    ground
        .and_then(|g| g.height(at.cell, at.origin))
        .is_some_and(|z| at.origin[2] < z - UNDER_THE_LAND)
}

/// The meters between two places, across blocks.
#[must_use]
pub fn meters(from: &StoredPosition, to: &StoredPosition) -> f32 {
    let (dx, dy, dz) = crate::ai::offset(from, to);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Whether a report of `to` may follow the place `from` last taken as the rooms and the land
/// allow.
///
/// # Errors
/// The check it fails.
pub fn rooms_and_land(
    ground: Option<&dyn Ground>,
    from: &StoredPosition,
    to: &StoredPosition,
) -> Result<(), Refused> {
    if !way_in(ground, from.cell, to.cell) {
        return Err(Refused::Rooms);
    }
    if under_the_land(ground, to) {
        return Err(Refused::Land);
    }
    Ok(())
}

/// Whether a report of `to`, arriving at `now`, may follow the place `from` last taken: its meters
/// if so. The land under `to` must already be read for its check to hold.
///
/// # Errors
/// The check it fails.
pub fn check(
    travel: &Travel,
    ground: Option<&dyn Ground>,
    from: &StoredPosition,
    to: &StoredPosition,
    now: f64,
) -> Result<f32, Refused> {
    let m = meters(from, to);
    travel.reaches(m, now)?;
    rooms_and_land(ground, from, to)?;
    Ok(m)
}

/// The rooms of a world's records: the cells each interior cell's doorways join (whole ids,
/// [`OUTDOORS`] for the outdoors) and those it sees.
#[derive(Debug, Clone, Default)]
pub struct Rooms {
    pub joins: HashMap<u32, Vec<u32>>,
    pub sees: HashMap<u32, Vec<u32>>,
}

/// A doorway whose other side is no cell: it leads outdoors.
const NO_OTHER_CELL: u16 = 0x2;

impl Rooms {
    /// Every interior cell of the records, read (about a second).
    #[must_use]
    pub fn read(records: &WorldRecords) -> Self {
        let mut rooms = Self::default();
        for place in 0..=0xFFFF_u32 {
            let [_, _, bx, by] = place.to_be_bytes();
            let (x, y) = (i32::from(bx), i32::from(by));
            let Some(info) = records.landblock_info(x, y) else {
                continue;
            };
            for i in 0..info.cell_count {
                let Ok(low) = u16::try_from(0x100 + i) else {
                    break;
                };
                let Some(c) = records.cell(x, y, low) else {
                    continue;
                };
                let whole = |l: u16| place << 16 | u32::from(l);
                rooms
                    .sees
                    .insert(whole(low), c.visible.iter().map(|l| whole(*l)).collect());
                rooms.joins.insert(
                    whole(low),
                    c.portals
                        .iter()
                        .map(|p| {
                            if p.flags & NO_OTHER_CELL == 0 {
                                whole(p.other_cell)
                            } else {
                                OUTDOORS
                            }
                        })
                        .collect(),
                );
            }
        }
        rooms
    }
}

/// The land as read so far and the rooms of a world's records: the ground and rooms the checks
/// read.
pub struct Known {
    pub land: Arc<Land>,
    pub rooms: Rooms,
}

impl std::fmt::Debug for Known {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Known({} interior cells)", self.rooms.joins.len())
    }
}

impl Ground for Known {
    fn height(&self, cell: u32, origin: [f32; 3]) -> Option<f32> {
        #[allow(clippy::cast_possible_truncation)]
        let block = dereth_primitives::LandblockId((cell >> 16) as u16);
        self.land
            .ground(
                block,
                dereth_primitives::Vec3::new(origin[0], origin[1], origin[2]),
            )
            .map(|(z, _)| z)
    }

    fn sees(&self, cell: u32) -> Option<&[u32]> {
        self.rooms.sees.get(&cell).map(Vec::as_slice)
    }

    fn joins(&self, cell: u32) -> Option<&[u32]> {
        self.rooms.joins.get(&cell).map(Vec::as_slice)
    }
}
