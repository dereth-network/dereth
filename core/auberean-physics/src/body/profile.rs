//! What an object's qualities make of its body when the object is made: whether it reports and
//! ignores collisions, is lit, is inelastic, slides along edges and falls, the pose it is placed in
//! and its scale ([`Profile`], applied by [`Body::apply_profile`]).
//!
//! The qualities are asked as the object answers them ([`ProfileQualities`]). A body reports
//! collisions, ignores them and falls unless its qualities say it does not; it is lit, inelastic
//! and slides along edges only where they say it is. So a body slides along the edge of a drop it
//! may not step down (and is held there) only when its qualities say so: the player's do, as do
//! about a third of the qualities records; the rest walk off.

use dereth_primitives::Vec3;

use super::{flags, Body};

/// The qualities a profile reads.
pub mod quality {
    /// Yes-or-no qualities: ignores collisions, reports them, falls, is lit, is inelastic, slides
    /// along edges.
    pub const IGNORE_COLLISIONS: u32 = 1;
    pub const REPORT_COLLISIONS: u32 = 2;
    pub const GRAVITY: u32 = 4;
    pub const LIGHTS: u32 = 5;
    pub const INELASTIC: u32 = 6;
    pub const EDGE_SLIDE: u32 = 7;
    /// The pose it is placed in (a whole number).
    pub const PLACEMENT: u32 = 1;
    /// Its friction, elasticity, mass and scale (floats).
    pub const FRICTION: u32 = 3;
    pub const ELASTICITY: u32 = 4;
    pub const MASS: u32 = 5;
    pub const SCALE: u32 = 0x12A;
    /// The record its physics object is made from (a data id).
    pub const PHYSICS: u32 = 1;
}

/// What a profile asks of one object's qualities; `None` where the object has no value for the
/// quality.
pub trait ProfileQualities {
    fn truth(&mut self, key: u32) -> Option<bool>;
    fn int(&mut self, key: u32) -> Option<i32>;
    fn float(&mut self, key: u32) -> Option<f32>;
    fn data_id(&mut self, key: u32) -> Option<u32>;
}

/// What an object's qualities say of its physics object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Profile {
    /// The record its physics object is made from (0: none).
    pub physics: u32,
    /// Standing-state flags: reporting and ignoring collisions, lit, under gravity, inelastic and
    /// edge sliding are what a body takes of them.
    pub state: u32,
    /// The pose it is placed in.
    pub placement: u32,
    pub scale: f32,
    pub friction: f32,
    pub elasticity: f32,
    pub mass: f32,
}

impl Default for Profile {
    /// A new body's state, placed in pose 0, at scale 1, friction 0.95, elasticity 0.005, mass 1.
    // D2-13120
    fn default() -> Self {
        Self {
            physics: 0,
            state: flags::NEW,
            placement: 0,
            scale: 1.0,
            friction: 0.95,
            elasticity: 0.005,
            mass: 1.0,
        }
    }
}

impl Profile {
    /// The standing-state flags `q` gives: reports collisions unless it says it does not, ignores
    /// them unless it says it does not, under gravity unless it says not; lit, inelastic and
    /// edge sliding only where it says so. (Not mobile: whether the object moves is not its
    /// qualities' to say here.)
    // D2-08299
    pub fn state_of(q: &mut impl ProfileQualities) -> u32 {
        let mut state = 0;
        if q.truth(quality::REPORT_COLLISIONS) != Some(false) {
            state |= flags::REPORT_COLLISIONS;
        }
        if q.truth(quality::IGNORE_COLLISIONS) != Some(false) {
            state |= flags::IGNORE_COLLISIONS;
        }
        if q.truth(quality::LIGHTS) == Some(true) {
            state |= flags::LIGHTS;
        }
        if q.truth(quality::GRAVITY) != Some(false) {
            state |= flags::GRAVITY;
        }
        if q.truth(quality::INELASTIC) == Some(true) {
            state |= flags::INELASTIC;
        }
        if q.truth(quality::EDGE_SLIDE) == Some(true) {
            state |= flags::EDGE_SLIDE;
        }
        state
    }

    /// The profile `q` gives: its physics record, its state ([`Profile::state_of`]), its
    /// placement (pose 0 when it has none), its scale (1), friction (0.95), elasticity (0.005) and
    /// mass (1).
    // D2-08305
    pub fn of(q: &mut impl ProfileQualities) -> Self {
        let d = Self::default();
        let physics = q.data_id(quality::PHYSICS).unwrap_or(0);
        let state = Self::state_of(q);
        #[allow(clippy::cast_sign_loss)] // the pose's number as the object stores it
        let placement = q.int(quality::PLACEMENT).map_or(0, |p| p as u32);
        Self {
            physics,
            state,
            placement,
            scale: q.float(quality::SCALE).unwrap_or(d.scale),
            friction: q.float(quality::FRICTION).unwrap_or(d.friction),
            elasticity: q.float(quality::ELASTICITY).unwrap_or(d.elasticity),
            mass: q.float(quality::MASS).unwrap_or(d.mass),
        }
    }
}

impl Body {
    /// Takes `p`: whether it reports collisions, ignores them, is lit, inelastic, edge sliding and
    /// under gravity; the pose it is placed in; its own scale. (Its friction, elasticity and mass
    /// are not taken.)
    // D2-06001
    pub fn apply_profile(&mut self, p: &Profile) {
        self.set_report_collisions(p.state & flags::REPORT_COLLISIONS != 0);
        self.set_ignore_collisions(p.state & flags::IGNORE_COLLISIONS != 0);
        self.set_lights(p.state & flags::LIGHTS != 0);
        self.set_inelastic(p.state & flags::INELASTIC != 0);
        self.set_edge_slide(p.state & flags::EDGE_SLIDE != 0);
        self.set_affected_by_gravity(p.state & flags::GRAVITY != 0);
        self.set_placement_frame(p.placement);
        self.set_scale(p.scale);
    }

    /// Its own scale becomes `s` along every axis.
    // D2-06325
    pub fn set_scale(&mut self, s: f32) {
        self.set_scale_vector(Vec3::new(s, s, s));
    }

    /// Its own scale becomes `s`; with a physics tree, whether its whole scale differs between
    /// axes (by 0.0002 or more) is kept in its state. (A world it is put in sizes it by its whole
    /// scale then.)
    // D2-06186, D2-06150
    pub fn set_scale_vector(&mut self, s: Vec3) {
        self.object_scale = s;
        if self.has_physics_tree() {
            let t = self.total_scale(None);
            self.put(flags::NON_UNIFORM_SCALE, crate::object::uneven(t));
        }
    }

    /// Whether a part of its part group has a physics tree, as worked out when it took the group.
    #[must_use]
    pub const fn has_physics_tree(&self) -> bool {
        self.has(flags::HAS_PHYSICS_TREE)
    }
}
