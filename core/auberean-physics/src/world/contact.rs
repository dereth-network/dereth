//! What a mover's collisions are told after each move besides the objects it hit: the physics
//! types it passes through start again from those it would, the time of the check, where it
//! touches ground or water, and which objects it woke.

use dereth_primitives::shape::Plane;

use super::{Host, World};
use crate::collision::manager::{BodyKey, NO_TERRAIN};
use crate::collision::settings;

impl World {
    /// Starts entry `key`'s collision check after a move: the physics types it passes through go
    /// back to those it would (what the move's object tests added is dropped), and the check is
    /// stamped with the clock. False when the entry keeps no collisions.
    pub(crate) fn begin_collision_check(&mut self, key: BodyKey) -> bool {
        let now = self.now;
        let Some(c) = self.collisions_mut(key) else {
            return false;
        };
        c.collide_mask = c.desired_physics;
        c.last_check_time = now;
        true
    }

    /// Ends entry `key`'s collision check: what it passes through moving is what the check left.
    pub(crate) fn end_collision_check(&mut self, key: BodyKey) {
        if let Some(c) = self.collisions_mut(key) {
            c.current.movement_physics = c.collide_mask;
            c.current.set_bit(settings::MOVEMENT_PHYSICS);
        }
    }

    /// Entry `key` touches the plane `plane`: in water of physics material `water` (none: out of
    /// it), and on the landscape with the ground's physics material `ground` (none: off it).
    pub(crate) fn set_contact(
        &mut self,
        key: BodyKey,
        plane: Plane,
        water: Option<u32>,
        ground: Option<u32>,
    ) {
        let Some(c) = self.collisions_mut(key) else {
            return;
        };
        c.set_contact_plane(plane);
        match water {
            Some(m) => c.set_in_water(m),
            None => c.set_out_of_water(),
        }
        match ground {
            Some(m) => c.set_on_landscape(m),
            None => c.set_off_landscape(),
        }
    }

    /// Entry `key` touches nothing: off the landscape, and on no terrain.
    pub(crate) fn set_no_contact(&mut self, key: BodyKey) {
        let Some(mut c) = self.entries.get_mut(&key).and_then(|e| e.collisions.take()) else {
            return;
        };
        c.set_off_landscape();
        c.set_terrain_type(NO_TERRAIN, &mut Host { world: self, key });
        if let Some(e) = self.entries.get_mut(&key) {
            e.collisions = Some(c);
        }
    }

    /// The terrain type entry `key`'s collisions showed since last asked.
    pub(crate) fn take_terrain(&mut self, key: BodyKey) -> Option<u32> {
        self.entries.get_mut(&key)?.terrain.take()
    }

    /// Wakes entry `key`'s other updates: something it touches has told its collisions.
    pub(crate) fn wake(&mut self, key: BodyKey) {
        if self.entries.contains_key(&key) {
            self.woken.insert(key);
        }
    }

    /// Whether something woke entry `key`'s other updates since last asked (see
    /// [`crate::body::Body::take_wake`]).
    pub fn take_woken(&mut self, key: BodyKey) -> bool {
        self.woken.remove(&key)
    }
}

impl World {
    /// Takes entry `key` out of every cell's collision list, keeping it in the table (an object
    /// leaving the world); [`World::place`] puts it back.
    // D2-06140
    pub fn take_out_of_cells(&mut self, key: BodyKey) -> bool {
        let Some(e) = self.entries.get_mut(&key) else {
            return false;
        };
        for c in std::mem::take(&mut e.cells) {
            if let Some(rt) = self.cells.get_mut(&c.0) {
                rt.shadow_object_list.retain(|&o| o != e.handle);
            }
        }
        true
    }
}

impl World {
    /// Entry `key`'s collision settings as they stand: its physics type, and what its collisions
    /// pass through being placed and would pass through moving. None for a key not in the table.
    // D2-05926
    #[must_use]
    pub fn collision_state(&self, key: BodyKey) -> Option<crate::collision::CollisionSettings> {
        let e = self.entries.get(&key)?;
        let mut s = crate::collision::CollisionSettings {
            physics_type: e.physics_type,
            ..crate::collision::CollisionSettings::default()
        };
        s.set_bit(settings::PHYSICS_TYPE);
        if let Some(c) = e.collisions.as_deref() {
            c.collision_state(&mut s);
        }
        Some(s)
    }
}
