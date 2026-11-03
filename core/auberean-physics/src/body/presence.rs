//! A body entering and leaving the world.
//!
//! Entering, its game object and the bookkeeping are told, its element is worked out, and all
//! three of its updates wake. Leaving, they are told it left; its target is dropped and its
//! watchers told, its detections end, its motion stops, every touch it had ends, and its updates
//! stop; it leaves the cells it stood in and forgets what it was touching and doing.

use super::missile::Objects;
use dereth_primitives::Position;

use super::{transient, Body, Notice};
use crate::detect::manager::DetectionHost;
use crate::land::Land;
use crate::world::World;

impl Body {
    /// The body has entered the world.
    // D2-06160
    pub fn handle_enter_world(&mut self) {
        self.in_world = true;
        self.notices.push(Notice::EnteredWorld);
        self.calc_element();
        self.set_movement_active(true);
        self.set_animation_active(true);
        self.set_misc_active(true);
    }

    /// The body is leaving the world: see the module. Its detections end through `detections`
    /// when a host for them is given.
    // D2-06359
    pub fn handle_leave_world(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        detections: Option<&mut dyn DetectionHost>,
    ) {
        self.notices.push(Notice::LeftWorld);
        self.with_tracking(false, |t, h| t.handle_leave_world(h));
        if let (Some(d), Some(host)) = (self.detections.as_mut(), detections) {
            d.handle_leave_world(host);
        }
        let now = self.clock;
        let mut mi = std::mem::take(&mut self.minterp);
        mi.handle_leave_world(self, now);
        self.minterp = mi;
        self.end_all_collisions(land, world, objects);
        self.transient |= transient::LEFT_WORLD;
        self.set_movement_active(false);
        self.set_animation_active(false);
        self.set_misc_active(false);
    }

    /// The body leaves the world: told so (see [`Body::handle_leave_world`]), out of the cells it
    /// stood in, and with nothing touched or under way.
    // D2-06159
    pub fn leave_world(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        detections: Option<&mut dyn DetectionHost>,
    ) {
        if !self.in_world {
            return;
        }
        self.handle_leave_world(land, world, objects, detections);
        if let Some(k) = self.key {
            world.take_out_of_cells(k);
        }
        self.in_world = false;
        self.clear_transient_states();
    }

    /// The server says the body left the world: when its position stamp is newer, a body that is
    /// not the player's lets go of its holder and leaves.
    pub fn receive_leave_world(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        detections: Option<&mut dyn DetectionHost>,
        stamp: u16,
    ) -> bool {
        if !self.is_newer_than_update(super::POSITION_STAMP, stamp) {
            return false;
        }
        if !self.is_player {
            self.has_parent = false;
            self.leave_world(land, world, objects, detections);
        }
        true
    }

    /// A teleport begins: the body stops following sent positions and is unconstrained; it drops
    /// its target and tells its watchers it is teleporting; its teleport stamp moves on.
    pub fn handle_teleport_start(&mut self) {
        self.minterp.stop_interpolating();
        self.minterp.unconstrain();
        self.with_tracking(false, |t, h| {
            t.clear_target(h);
            t.notify_voyeurs(h, crate::target::status::TELEPORTED);
        });
        self.inc_update_time(super::TELEPORT_STAMP);
    }

    /// Puts the body at `p` as it stands outside any cell: there now and before.
    pub fn set_no_cell_position(&mut self, p: Position) {
        let heading = crate::geom::frame_heading(&p.frame).degrees();
        self.position = p;
        self.heading = heading;
        self.previous = (p, heading);
    }
}
