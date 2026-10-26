//! Lines of sight: whether a straight line between two places is clear, as a bolt or an arrow
//! flies it.
//!
//! A small sphere is swept from one end toward the other through the collision the body walks in,
//! the camera's own sweep: the cells' walls and floors, the ground, buildings, and the objects that
//! stop a body (a closed door, a tree, a rock), creatures aside. The line is clear when the sphere
//! reaches the far end.

use std::cell::RefCell;

use dereth_physics::{PhysHandle, PhysicsWorld, Sphere};
use dereth_primitives::{Position, Vec3};

/// The radius of the sphere swept along a line of sight, metres: about a bolt's.
pub const SIGHT_RADIUS: f32 = 0.15;

/// How near the far end the sphere must come for the line to be clear, metres.
const REACH: f32 = 0.1;

/// The world a line of sight is swept through, for one frame.
pub struct Sight<'w> {
    world: RefCell<&'w mut PhysicsWorld>,
    mover: PhysHandle,
}

impl std::fmt::Debug for Sight<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sight").finish_non_exhaustive()
    }
}

impl<'w> Sight<'w> {
    /// Lines of sight through `world`, swept on behalf of `mover` (the player's body, which the
    /// sweep does not collide with).
    pub fn new(world: &'w mut PhysicsWorld, mover: PhysHandle) -> Self {
        Self {
            world: RefCell::new(world),
            mover,
        }
    }

    /// Whether the straight line from `from` to `to` is clear; `None` when the sweep cannot be
    /// made (no place to start it from).
    #[must_use]
    pub fn clear(&self, from: &Position, to: &Position) -> Option<bool> {
        let mut world = self.world.try_borrow_mut().ok()?;
        // The cell the raised start stands in: indoors, the interior cell holding the point (a
        // metre up may be another room's); outdoors, its land cell.
        let start_cell = world
            .adjust_position(from, Vec3::ZERO)
            .and_then(|(_, c)| c)
            .unwrap_or(from.cell);
        let start = Position::new(start_cell, from.frame);
        // The far end in the start's own space, across landblocks.
        let mut end = *to;
        end.frame.origin = dereth_physics::math::pos_localtoglobal(&start, to, Vec3::ZERO);
        let end = Position::new(start_cell, end.frame);
        let t = world.sweep_sphere(
            self.mover,
            dereth_physics::globals::VIEWER_OBJECT_INFO_STATE,
            Sphere::new(Vec3::ZERO, SIGHT_RADIUS),
            start_cell,
            &start,
            &end,
        )?;
        Some(dereth_physics::math::distance(&t.sphere_path.curr_pos, &end) <= REACH)
    }
}
