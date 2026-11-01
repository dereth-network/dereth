//! Holding a body near a place: past a starting distance its motion is scaled down, reaching
//! nothing at the farthest distance.

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::geom;

/// A body held near a place.
// D2-07132
#[derive(Clone, Debug, PartialEq)]
pub struct Tether {
    pub id: u64,
    pub constrained: bool,
    pub enabled: bool,
    pub position: Position,
    /// From here the body's motion is scaled down...
    pub start: f32,
    /// ...to nothing here.
    pub max: f32,
    pub is_player: bool,
}

impl Tether {
    /// Not holding the body yet.
    // D2-07131, D2-07135
    #[must_use]
    pub fn new(id: u64) -> Self {
        Self {
            id,
            constrained: false,
            enabled: true,
            position: Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY)),
            start: 0.0,
            max: 0.0,
            is_player: false,
        }
    }

    // D2-07133
    pub fn set_distances(&mut self, start: f32, max: f32) {
        self.start = start;
        self.max = max;
    }

    /// Holds the body near `p`.
    // D2-07136
    pub fn constrain_to(&mut self, p: Position) {
        self.constrained = true;
        self.position = p;
    }

    /// Lets the body go.
    pub fn release(&mut self) {
        self.constrained = false;
    }

    /// How much of its motion a body at `p` keeps: all of it within the starting distance, none
    /// at the farthest, in proportion between.
    // D2-07134
    #[must_use]
    pub fn factor(&self, p: &Position) -> f32 {
        if self.constrained && self.enabled {
            let d = geom::distance(p, &self.position);
            if d > self.start {
                if self.max <= d {
                    return 0.0;
                }
                return (self.max - d) / (self.max - self.start);
            }
        }
        1.0
    }
}
