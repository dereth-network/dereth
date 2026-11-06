//! What a body is and what it is doing, as flags: its standing state (static, mobile, under
//! gravity, edge sliding, drawn or not, ...) and its passing state (touching the ground, on
//! walkable ground, in water, stuck, and which of its updates are running).
//!
//! A body is made mobile, under gravity, edge sliding, lit and reporting its collisions, with its
//! movement, animation and other updates running. Touching the ground and standing on walkable
//! ground are the body's `contact` and `on_walkable`; every other passing flag is kept here.

use super::Body;
use crate::motion::WATER;

/// The standing state's flags.
pub mod flags {
    /// Never moves: no updates run for it.
    pub const STATIC: u32 = 0x1;
    pub const REPORT_COLLISIONS: u32 = 0x4;
    pub const IGNORE_COLLISIONS: u32 = 0x8;
    pub const NO_DRAW: u32 = 0x10;
    pub const MISSILE: u32 = 0x20;
    pub const PUSHABLE: u32 = 0x40;
    /// Faces the way it moves.
    pub const ALIGN_TO_PATH: u32 = 0x80;
    pub const PATH_CLIPPED: u32 = 0x100;
    pub const GRAVITY: u32 = 0x200;
    pub const LIGHTS: u32 = 0x400;
    pub const UNDETECTABLE: u32 = 0x800;
    /// A part of its part group has a physics tree (worked out when it takes its part group).
    pub const HAS_PHYSICS_TREE: u32 = 0x1000;
    pub const INELASTIC: u32 = 0x2000;
    pub const CLOAKED: u32 = 0x4000;
    pub const EDGE_SLIDE: u32 = 0x8000;
    pub const MOVEMENT_FROZEN: u32 = 0x2_0000;
    pub const ANIMATION_FROZEN: u32 = 0x4_0000;
    pub const GAME_DRAW: u32 = 0x8_0000;
    pub const TOOLS_DRAW: u32 = 0x10_0000;
    pub const MOBILE: u32 = 0x20_0000;
    /// Placed in a workspace (a world being built), whether or not in the world.
    pub const IN_WORKSPACE: u32 = 0x1000_0000;
    /// Has a physics tree and a whole scale that differs between axes.
    pub const NON_UNIFORM_SCALE: u32 = 0x2000_0000;
    /// Follows the transition's result exactly.
    pub const SYNCHRONIZE: u32 = 0x8000_0000;
    /// A new body's: mobile, edge sliding, lit, under gravity, reporting its collisions.
    pub const NEW: u32 = MOBILE | EDGE_SLIDE | LIGHTS | GRAVITY | REPORT_COLLISIONS;
}

/// The passing state's flags.
pub mod transient {
    pub const CONTACT: u32 = 0x1;
    pub const ON_WALKABLE: u32 = 0x2;
    pub const SLIDING: u32 = 0x4;
    pub const WATER_CONTACT: u32 = 0x8;
    pub const STATIONARY_FALL: u32 = 0x10;
    pub const STATIONARY_STOP: u32 = 0x20;
    pub const STATIONARY_STUCK: u32 = 0x40;
    pub const MOVEMENT_ACTIVE: u32 = 0x80;
    pub const ANIMATION_ACTIVE: u32 = 0x100;
    pub const MISC_ACTIVE: u32 = 0x200;
    /// The plane touched is the landscape's ground, or an interior's own floors and walls (the
    /// latter never set by the body itself); both are cleared with the rest when a body leaves the
    /// world.
    pub const LANDSCAPE_CONTACT: u32 = 0x400;
    pub const ENVIRONMENT_CONTACT: u32 = 0x800;
    /// Set as the body leaves the world.
    pub const LEFT_WORLD: u32 = 0x2000;
    /// A new body's: its movement, animation and other updates running.
    pub const NEW: u32 = MOVEMENT_ACTIVE | ANIMATION_ACTIVE | MISC_ACTIVE;
}

/// Below this a velocity or acceleration counts as none.
const STILL: f32 = 0.0002;

/// The longest chain of parents a sound asks up.
const MAX_PARENTS: usize = 16;

impl Body {
    /// The standing state.
    // D2-06286
    #[must_use]
    pub const fn state(&self) -> u32 {
        self.state
    }

    /// The passing state, touching and walkable included.
    #[must_use]
    pub const fn transient_state(&self) -> u32 {
        let mut t = self.transient & !(transient::CONTACT | transient::ON_WALKABLE);
        if self.contact {
            t |= transient::CONTACT;
        }
        if self.on_walkable {
            t |= transient::ON_WALKABLE;
        }
        t
    }

    pub(super) const fn has(&self, flag: u32) -> bool {
        self.state & flag != 0
    }

    pub(super) fn put(&mut self, flag: u32, on: bool) {
        if on {
            self.state |= flag;
        } else {
            self.state &= !flag;
        }
    }

    const fn passing(&self, flag: u32) -> bool {
        self.transient & flag != 0
    }

    /// Static: never moves. The world's own objects are the static ones.
    // D2-06288, D2-06289
    #[must_use]
    pub const fn is_static(&self) -> bool {
        self.has(flags::STATIC)
    }
    // D2-06287
    #[must_use]
    pub const fn is_dynamic(&self) -> bool {
        !self.is_static()
    }
    // D2-06292
    #[must_use]
    pub const fn reports_collisions(&self) -> bool {
        self.has(flags::REPORT_COLLISIONS)
    }
    // D2-06293
    #[must_use]
    pub const fn does_edge_slide(&self) -> bool {
        self.has(flags::EDGE_SLIDE)
    }
    // D2-06294
    #[must_use]
    pub const fn ignores_collisions(&self) -> bool {
        self.has(flags::IGNORE_COLLISIONS)
    }
    // D2-06295
    #[must_use]
    pub const fn is_mobile(&self) -> bool {
        self.has(flags::MOBILE)
    }
    // D2-06296
    #[must_use]
    pub const fn is_no_draw(&self) -> bool {
        self.has(flags::NO_DRAW)
    }
    #[must_use]
    pub const fn is_cloaked(&self) -> bool {
        self.has(flags::CLOAKED)
    }
    /// Pushable only while it moves at all.
    #[must_use]
    pub const fn is_pushable(&self) -> bool {
        self.has(flags::PUSHABLE) && self.is_dynamic()
    }
    // D2-06300
    #[must_use]
    pub const fn is_path_aligned(&self) -> bool {
        self.has(flags::ALIGN_TO_PATH)
    }
    // D2-06319
    #[must_use]
    pub const fn is_affected_by_gravity(&self) -> bool {
        self.has(flags::GRAVITY)
    }
    // D2-06320
    #[must_use]
    pub const fn lights_on(&self) -> bool {
        self.has(flags::LIGHTS)
    }
    // D2-06321
    #[must_use]
    pub const fn is_undetectable(&self) -> bool {
        self.has(flags::UNDETECTABLE)
    }
    // D2-06322
    #[must_use]
    pub const fn is_inelastic(&self) -> bool {
        self.has(flags::INELASTIC)
    }
    // D2-06323
    #[must_use]
    pub const fn is_movement_frozen(&self) -> bool {
        self.has(flags::MOVEMENT_FROZEN)
    }
    // D2-06324
    #[must_use]
    pub const fn is_animation_frozen(&self) -> bool {
        self.has(flags::ANIMATION_FROZEN)
    }

    // D2-06012
    pub fn set_edge_slide(&mut self, on: bool) {
        self.put(flags::EDGE_SLIDE, on);
    }
    // D2-06013
    pub fn set_synchronize_transition(&mut self, on: bool) {
        self.put(flags::SYNCHRONIZE, on);
        self.synchronize = on;
    }
    // D2-06014
    pub fn set_ignore_collisions(&mut self, on: bool) {
        self.put(flags::IGNORE_COLLISIONS, on);
    }
    // D2-06249
    pub fn set_report_collisions(&mut self, on: bool) {
        self.put(flags::REPORT_COLLISIONS, on);
    }
    pub fn set_tools_draw(&mut self, on: bool) {
        self.put(flags::TOOLS_DRAW, on);
    }
    // D2-06017
    pub fn set_no_draw(&mut self, on: bool) {
        self.put(flags::NO_DRAW, on);
    }
    /// A cloaked body is not drawn either.
    // D2-06018
    pub fn set_cloaked(&mut self, on: bool) {
        self.put(flags::CLOAKED, on);
        self.set_no_draw(on);
    }
    pub fn set_missile(&mut self, on: bool) {
        self.put(flags::MISSILE, on);
    }
    // D2-06020
    pub fn set_mobile(&mut self, on: bool) {
        self.put(flags::MOBILE, on);
    }
    // D2-06021
    pub fn align_to_path(&mut self, on: bool) {
        self.put(flags::ALIGN_TO_PATH, on);
    }
    /// Sticky objects clip to their path.
    pub fn set_path_clipping(&mut self, on: bool) {
        self.put(flags::PATH_CLIPPED, on);
    }
    // D2-06023
    pub fn set_affected_by_gravity(&mut self, on: bool) {
        self.put(flags::GRAVITY, on);
    }
    /// An undetectable body neither collides nor reports collisions (its cell hides it from
    /// others). Each call is told to the world the body next ticks in: its cell's watchers hear it
    /// leave (or come back), even when it already was.
    // D2-06024
    pub fn set_undetectable(&mut self, on: bool) {
        self.set_ignore_collisions(on);
        self.set_report_collisions(!on);
        self.put(flags::UNDETECTABLE, on);
        self.world_told.push(on);
    }

    /// The standing state becomes `state`; a change to whether it is lit, drawn or detectable is
    /// then set as its own setter sets it.
    // D2-06035
    pub fn set_state(&mut self, state: u32) {
        let changed = self.state ^ state;
        self.state = state;
        if changed & flags::LIGHTS != 0 {
            self.set_lights(self.lights_on());
        }
        if changed & flags::NO_DRAW != 0 {
            self.set_no_draw(self.is_no_draw());
        }
        if changed & flags::UNDETECTABLE != 0 {
            self.set_undetectable(self.is_undetectable());
        }
    }
    // D2-06025
    pub fn set_inelastic(&mut self, on: bool) {
        self.put(flags::INELASTIC, on);
    }
    // D2-06026
    pub fn set_lights(&mut self, on: bool) {
        self.put(flags::LIGHTS, on);
    }
    pub fn set_pushable(&mut self, on: bool) {
        self.put(flags::PUSHABLE, on);
    }
    // D2-06029
    pub fn set_movement_frozen(&mut self, on: bool) {
        self.put(flags::MOVEMENT_FROZEN, on);
    }
    // D2-06030
    pub fn set_animation_frozen(&mut self, on: bool) {
        self.put(flags::ANIMATION_FROZEN, on);
    }

    // D2-06301
    #[must_use]
    pub const fn is_movement_active(&self) -> bool {
        self.passing(transient::MOVEMENT_ACTIVE)
    }
    // D2-06302
    #[must_use]
    pub const fn is_animation_active(&self) -> bool {
        self.passing(transient::ANIMATION_ACTIVE)
    }
    // D2-06303
    #[must_use]
    pub const fn is_misc_active(&self) -> bool {
        self.passing(transient::MISC_ACTIVE)
    }
    // D2-06304
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.is_animation_active() || self.is_movement_active() || self.is_misc_active()
    }
    #[must_use]
    pub const fn is_inactive(&self) -> bool {
        !self.is_active()
    }

    /// Starts or stops the body's movement updates (never for a static body); stopping keeps
    /// where it stopped as where it was.
    // D2-06283
    #[must_use]
    pub const fn is_in_workspace(&self) -> bool {
        self.has(flags::IN_WORKSPACE)
    }
    pub fn set_in_workspace(&mut self, on: bool) {
        self.put(flags::IN_WORKSPACE, on);
    }

    /// The cell the body stands in is among the free blocks, or back from them.
    // D2-06055, D2-06056
    pub fn set_cell_on_free_list(&mut self, on: bool) {
        self.cell_on_free_list = on;
    }

    /// Out of the player's physics range, or back in it: its movement and animation stop, or
    /// wake. A body keeps what it last found until a position or its own movement looks again.
    // D2-06102, D2-06103
    pub fn set_out_of_physics_range(&mut self, out: bool) {
        self.out_of_physics_range = out;
        self.set_movement_active(!out);
        self.set_animation_active(!out);
    }

    /// Whether the body's movement looks at the player's range this tick: its movement runs, it
    /// stands in a cell or hangs from a parent, and it hangs from a parent, or is a missile, or
    /// stands in the world able to move.
    // D2-06344
    #[must_use]
    pub fn movement_checks_range(&self) -> bool {
        self.is_movement_active()
            && (self.has_parent || self.position.cell.0 != 0)
            && (self.has_parent
                || self.missile.is_some()
                || (self.in_world && !self.is_movement_frozen() && self.is_mobile()))
    }

    /// Whether the sounds the body makes are silent just now, to play again when they are not: a
    /// static body's while its cell is among the free blocks; a moving body's out of the world
    /// with no parent, or out of the player's physics range; what a parent holds, as its parent's
    /// (`find`); any other body's out of both the world and the workspace.
    // D2-06153
    pub fn should_mute_sound<'a>(&'a self, find: &dyn Fn(u64) -> Option<&'a Self>) -> bool {
        let mut body = self;
        // A chain of parents is short; one that loops is heard.
        for _ in 0..MAX_PARENTS {
            let silent = if body.is_static() {
                body.cell_on_free_list
            } else {
                (!body.in_world && !body.has_parent) || body.out_of_physics_range
            };
            if silent {
                return true;
            }
            if !body.has_parent {
                return !body.in_world && !body.is_in_workspace();
            }
            match find(body.parent) {
                Some(parent) => body = parent,
                None => return false,
            }
        }
        false
    }

    // D2-06009
    pub fn set_movement_active(&mut self, on: bool) -> bool {
        if self.is_static() {
            return false;
        }
        if on {
            self.transient |= transient::MOVEMENT_ACTIVE;
            return true;
        }
        if self.is_movement_active() {
            self.previous = (self.position, self.heading);
        }
        self.transient &= !transient::MOVEMENT_ACTIVE;
        true
    }
    // D2-06010
    pub fn set_animation_active(&mut self, on: bool) -> bool {
        if self.is_static() {
            return false;
        }
        if on {
            self.transient |= transient::ANIMATION_ACTIVE;
        } else {
            self.transient &= !transient::ANIMATION_ACTIVE;
        }
        true
    }
    // D2-06011
    pub fn set_misc_active(&mut self, on: bool) -> bool {
        if self.is_static() {
            return false;
        }
        if on {
            self.transient |= transient::MISC_ACTIVE;
        } else {
            self.transient &= !transient::MISC_ACTIVE;
        }
        true
    }

    /// Whether the body's movement updates may stop: out of the world, not mobile, or carried by
    /// a parent; or at rest (no velocity, acceleration, turning or field over 0.0002 on any
    /// axis) and neither moving to anything, turning to a heading nor following sent positions.
    /// A missile never stops.
    // D2-06007
    #[must_use]
    pub fn movement_can_be_inactive(&self) -> bool {
        if self.missile.is_some() {
            return false;
        }
        if !self.in_world || !self.is_mobile() || self.has_parent {
            return true;
        }
        let still = |v: dereth_primitives::Vec3| {
            v.x.abs() < STILL && v.y.abs() < STILL && v.z.abs() < STILL
        };
        let at_rest = still(self.velocity)
            && still(self.acceleration)
            && still(dereth_primitives::Vec3::new(0.0, 0.0, self.turn_rate))
            && still(dereth_primitives::Vec3::new(
                0.0,
                0.0,
                self.turn_acceleration,
            ))
            && still(self.velocity_field)
            && still(self.acceleration_field)
            && still(self.external_field);
        if !at_rest {
            return false;
        }
        !(self.minterp.is_moving_to() || self.minterp.turning_to || self.minterp.is_interpolating())
    }

    /// Animation may stop only out of the world.
    #[must_use]
    pub const fn animation_can_be_inactive(&self) -> bool {
        !self.in_world
    }

    /// The body's other updates may stop out of the world (and any workspace), or while it
    /// touches nothing (its collisions in `world`) and none of its detections runs.
    // D2-06152
    #[must_use]
    pub fn misc_can_be_inactive(&self, world: &crate::world::World) -> bool {
        if !self.in_world && !self.is_in_workspace() {
            return true;
        }
        if self
            .key
            .and_then(|k| world.collisions(k))
            .is_some_and(|c| !c.is_empty())
        {
            return false;
        }
        !self
            .detections
            .as_ref()
            .is_some_and(crate::detect::manager::Detections::is_active)
    }

    // D2-06290
    #[must_use]
    pub const fn on_ground(&self) -> bool {
        self.contact && self.on_walkable
    }
    #[must_use]
    pub const fn off_ground(&self) -> bool {
        !self.on_ground()
    }
    // D2-06314
    #[must_use]
    pub const fn has_contact(&self) -> bool {
        self.contact
    }
    // D2-06315
    #[must_use]
    pub const fn contact_is_water(&self) -> bool {
        self.passing(transient::WATER_CONTACT)
    }
    // D2-06316
    #[must_use]
    pub const fn has_sliding(&self) -> bool {
        self.passing(transient::SLIDING)
    }
    // D2-06311
    #[must_use]
    pub const fn is_stationary_fall(&self) -> bool {
        self.passing(transient::STATIONARY_FALL)
    }
    // D2-06313
    #[must_use]
    pub const fn is_stationary_stop(&self) -> bool {
        self.passing(transient::STATIONARY_STOP)
    }
    // D2-06312
    #[must_use]
    pub const fn is_stationary_stuck(&self) -> bool {
        self.passing(transient::STATIONARY_STUCK)
    }
    // D2-06307
    pub fn reset_stationary_stuck_fall(&mut self) {
        self.transient &= !(transient::STATIONARY_FALL
            | transient::STATIONARY_STOP
            | transient::STATIONARY_STUCK);
    }
    // D2-06308
    pub fn activate_stationary_fall(&mut self) {
        self.transient |= transient::STATIONARY_FALL;
    }
    // D2-06309
    pub fn activate_stationary_stop(&mut self) {
        self.transient |= transient::STATIONARY_STOP;
    }
    // D2-06310
    pub fn activate_stationary_stuck(&mut self) {
        self.transient |= transient::STATIONARY_STUCK;
    }

    /// Touching the ground or not; leaving it wakes the body's other updates.
    // D2-05975
    pub fn set_contact_state(&mut self, on: bool) {
        if on {
            self.contact = true;
        } else {
            self.contact = false;
            self.set_misc_active(true);
        }
    }

    /// Standing on walkable ground or not: coming onto it is landing; leaving it (out of water)
    /// tells the motion interpreter the body left the ground.
    // D2-06142
    pub fn set_on_walkable(&mut self, on: bool) {
        let was = self.on_walkable;
        self.on_walkable = on;
        if !was {
            if on {
                self.handle_hit_ground();
            }
        } else if !on && self.desired_element() != WATER {
            self.leave_ground();
        }
    }

    /// Clears the passing state as a body leaves the world: not touching anything (its other
    /// updates woken first), not in water, off walkable ground (the motion interpreter told, out of
    /// water), not sliding or stuck, and every update stopped.
    // D2-06151
    pub fn clear_transient_states(&mut self) {
        self.contact = false;
        self.set_misc_active(true);
        self.transient &= !transient::WATER_CONTACT;
        let was_walkable = self.on_walkable;
        self.on_walkable = false;
        if was_walkable && self.desired_element() != WATER {
            self.leave_ground();
        }
        self.transient &= !transient::SLIDING;
        self.reset_stationary_stuck_fall();
        self.set_movement_active(false);
        self.set_animation_active(false);
        self.set_misc_active(false);
        self.transient &= !transient::LANDSCAPE_CONTACT;
    }
}
