//! A planned route, and why a route could not be planned.

use dereth_primitives::{CellId, ObjectId, Position};

use dereth_primitives::Vec3;

use crate::vmath::dist;

/// A position in world space: its landblock's corner plus its offset in the landblock, so two
/// positions in different landblocks can be compared.
#[must_use]
pub fn global(p: &Position) -> Vec3 {
    let b = p.cell.0 >> 16;
    #[allow(clippy::cast_precision_loss)]
    let (bx, by) = ((b >> 8) as f32, (b & 0xFF) as f32);
    Vec3::new(
        bx * crate::region::BLOCK_LENGTH + p.frame.origin.x,
        by * crate::region::BLOCK_LENGTH + p.frame.origin.y,
        p.frame.origin.z,
    )
}

/// The distance between two positions, wherever they are.
#[must_use]
pub fn global_distance(a: &Position, b: &Position) -> f32 {
    dist(global(a), global(b))
}

/// Something the body has to do on reaching a waypoint before it walks on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaypointAction {
    /// Open this door: the next leg walks through it.
    OpenDoor(ObjectId),
}

/// One corner of a route.
#[derive(Debug, Clone, PartialEq)]
pub struct Waypoint {
    /// Where, in the cell whose floor it stands on.
    pub position: Position,
    /// What to do here before walking on.
    pub action: Option<WaypointAction>,
}

/// A route: straight legs between waypoints, from where the body is to the standing place nearest
/// the destination.
#[derive(Debug, Clone, PartialEq)]
pub struct NavPath {
    /// The corners, the start first.
    pub waypoints: Vec<Waypoint>,
    /// For each leg (waypoint `i` to `i + 1`), the standing places the search went through,
    /// spaced one sample apart and ending at waypoint `i + 1`. A leg the body cannot walk
    /// straight is walked through these instead.
    pub legs: Vec<Vec<Position>>,
    /// How far the route's end is from the destination asked for.
    pub goal_offset: f32,
    /// Per waypoint, how far before and after it a body may cut straight across the corner it
    /// makes, in metres: the line between those two points is as walkable as the legs. Zero (or
    /// missing) where it may not.
    pub cuts: Vec<f32>,
}

impl NavPath {
    /// The route's length along its legs.
    #[must_use]
    pub fn length(&self) -> f32 {
        self.waypoints
            .windows(2)
            .map(|w| global_distance(&w[0].position, &w[1].position))
            .sum()
    }

    /// The distinct cells the route's places stand in, in the order it enters them.
    #[must_use]
    pub fn cells(&self) -> Vec<CellId> {
        let mut out: Vec<CellId> = Vec::new();
        let places = self
            .waypoints
            .first()
            .map(|w| &w.position)
            .into_iter()
            .chain(self.legs.iter().flatten());
        for p in places {
            if out.last() != Some(&p.cell) {
                out.push(p.cell);
            }
        }
        out
    }

    /// How far before and after waypoint `i` a body may cut across its corner.
    #[must_use]
    pub fn cut(&self, i: usize) -> f32 {
        self.cuts.get(i).copied().unwrap_or(0.0)
    }

    /// End the route short of `at`: at the last of its places, along the legs the search went
    /// through, before the first that is within `distance` of `at`. A body walks it no nearer
    /// (a portal the walk is not taking, say). False, and the route as it was, when no place of it
    /// comes that near, or its start already does.
    pub fn stop_short_of(&mut self, at: &Position, distance: f32) -> bool {
        let near = |p: &Position| global_distance(p, at) < distance;
        let Some(start) = self.waypoints.first() else {
            return false;
        };
        if near(&start.position) {
            return false;
        }
        let end = self.waypoints.last().map(|w| w.position);
        for i in 0..self.waypoints.len().saturating_sub(1) {
            // The leg's places, or its end alone where the search kept none.
            let places: Vec<Position> = match self.legs.get(i) {
                Some(l) if !l.is_empty() => l.clone(),
                _ => vec![self.waypoints[i + 1].position],
            };
            let Some(k) = places.iter().position(near) else {
                continue;
            };
            let kept: Vec<Position> = places[..k].to_vec();
            self.waypoints.truncate(i + 1);
            self.legs.truncate(i);
            if let Some(&last) = kept.last() {
                self.waypoints.push(Waypoint {
                    position: last,
                    action: None,
                });
                self.legs.push(kept);
            }
            self.cuts.truncate(self.waypoints.len());
            if let Some(c) = self.cuts.last_mut() {
                *c = 0.0;
            }
            let stop = self.waypoints.last().map(|w| w.position);
            if let (Some(stop), Some(end)) = (stop, end) {
                self.goal_offset += global_distance(&stop, &end);
            }
            return true;
        }
        false
    }

    /// Replace leg `i`'s straight line by the places the search went through.
    pub(crate) fn densify_leg(&mut self, i: usize) {
        let Some(places) = self.legs.get(i).cloned() else {
            return;
        };
        if places.len() < 2 {
            return;
        }
        let action = self.waypoints[i + 1].action;
        let mut inserted: Vec<Waypoint> = places
            .iter()
            .map(|p| Waypoint {
                position: *p,
                action: None,
            })
            .collect();
        if let Some(last) = inserted.last_mut() {
            last.action = action;
        }
        let new_legs: Vec<Vec<Position>> = places.iter().map(|p| vec![*p]).collect();
        let n = inserted.len();
        self.waypoints.splice(i + 1..=i + 1, inserted);
        self.legs.splice(i..=i, new_legs);
        if self.cuts.len() > i + 1 {
            // Walked place by place: no corner of it is cut, nor the one it ends at.
            self.cuts.splice(i + 1..=i + 1, std::iter::repeat_n(0.0, n));
        }
    }
}

/// Which end of a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// Where the route starts.
    Start,
    /// Where it is going.
    Goal,
}

/// Why no route was planned.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NoPath {
    /// A position outdoors; this planner routes inside one dungeon.
    #[error("{0:?} is outdoors")]
    Outdoors(CellId),
    /// The source has no cell there (it is not loaded, or does not exist).
    #[error("no interior cell {0:?} is loaded")]
    NoCell(CellId),
    /// The two positions are in cells no portal chain joins.
    #[error("{from:?} and {to:?} are not in one connected set of cells")]
    Disconnected {
        /// The start's cell.
        from: CellId,
        /// The destination's cell.
        to: CellId,
    },
    /// Nowhere near one end can the body stand.
    #[error("no place to stand near the {0:?}")]
    NoStandingPlace(End),
    /// The places are connected by cells but not by floor the body can walk.
    #[error("no walkable route")]
    Unreachable,
    /// The two ends are farther apart than one outdoor region reaches.
    #[error("too far apart for one region")]
    TooFar,
    /// No walk gets there, but a drop does: off a ledge at `stand`, the body lands at `land`,
    /// `height` metres down. A route never takes a drop by itself.
    #[error(
        "a drop of {height:.1} m, off the ledge at {} to {}",
        place(stand),
        place(land)
    )]
    OnlyByDrop {
        /// Where to stand at the ledge's edge.
        stand: Position,
        /// Where the body lands.
        land: Position,
        /// How far it falls.
        height: f32,
    },
}

/// A position as a cell and its coordinates, for a message.
fn place(p: &Position) -> String {
    let o = p.frame.origin;
    format!("{:08X} ({:.1}, {:.1}, {:.1})", p.cell.0, o.x, o.y, o.z)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks a route's arithmetic).
    use super::*;
    use dereth_primitives::{Frame, Quat, Vec3};

    fn at(x: f32) -> Position {
        Position::new(
            CellId(0x0101_0100),
            Frame::new(Vec3::new(x, 0.0, 0.0), Quat::IDENTITY),
        )
    }

    /// From 0 to 30 by a corner at 20, a place a meter along each leg.
    fn route() -> NavPath {
        let w = |x: f32| Waypoint {
            position: at(x),
            action: None,
        };
        NavPath {
            waypoints: vec![w(0.0), w(20.0), w(30.0)],
            legs: vec![
                (1..=20).map(|i| at(i as f32)).collect(),
                (21..=30).map(|i| at(i as f32)).collect(),
            ],
            goal_offset: 0.0,
            cuts: vec![0.0, 2.0, 0.0],
        }
    }

    #[test]
    fn a_route_ends_at_its_last_place_before_one_too_near_the_point() {
        let mut r = route();
        assert!(r.stop_short_of(&at(30.0), 6.0));
        let end = r.waypoints.last().expect("an end").position.frame.origin.x;
        assert!((end - 24.0).abs() < 1e-3, "{end}");
        assert_eq!(r.waypoints.len(), r.legs.len() + 1);
        assert_eq!(r.cuts.len(), r.waypoints.len());
        assert!((r.goal_offset - 6.0).abs() < 1e-3);
        // Short of a point on the first leg: the second leg goes.
        let mut r = route();
        assert!(r.stop_short_of(&at(18.0), 6.0));
        assert_eq!(r.waypoints.len(), 2);
        let end = r.waypoints[1].position.frame.origin.x;
        assert!((end - 12.0).abs() < 1e-3, "{end}");
    }

    #[test]
    fn a_route_that_never_comes_near_or_starts_near_is_left_as_it_was() {
        let mut r = route();
        assert!(!r.stop_short_of(&at(60.0), 6.0));
        assert_eq!(r, route());
        assert!(
            !r.stop_short_of(&at(2.0), 6.0),
            "its start is already that near"
        );
        assert_eq!(r, route());
    }
}
