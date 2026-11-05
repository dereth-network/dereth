//! What moves a body each tick besides the motion asked of it: the element it is in (earth, air,
//! water, the sea floor) from the water around it, gravity and lift, the pushes of what it stands
//! in, landing and the jump.
//!
//! The element: a body touching nothing is in the air, on the ground on the earth. In a cell with
//! water, the depth of a point a share of the body's height above its feet (the water multiplier's
//! share) decides: under the surface it swims; above it, with the point the sea floor multiplier
//! gives under, it walks the sea floor; a body leaving the water upward stays in it until that point
//! is 0.3 m clear. A body that walks on air is always in the air's own element.
//!
//! Gravity pulls a body under gravity that is not standing on walkable ground. In water the body
//! is lifted by its lift times its depth while within a meter of the surface, by its whole lift
//! deeper, and falls above. In the air a body whose motion values have no gravity is lifted unless
//! it is within a tenth of a meter of its hover height (over the ground or the water below).

use dereth_physics::geom::Plane;
use dereth_physics::V3;
use dereth_primitives::{CellId, Position, Vec3};

use auberean_assets::animation::{ModeKey, MotionParams};
use auberean_assets::property::{Property, PropertyCollection, PropertyList};

use super::{Animate, Body, Notice};
use crate::collision::manager::BodyKey;
use crate::land::Land;
use crate::motion::values::default_values;
use crate::motion::{shows, Owner, AIR, EARTH, PEACE, SEA_FLOOR, WATER};
use crate::world::World;

/// The element of a body walking on air.
pub const WALK_ON_AIR: u32 = 0x4000_0007;
/// Gravity where nothing else sets it, meters per second squared.
pub const DEFAULT_GRAVITY: Vec3 = Vec3 {
    x: 0.0,
    y: 0.0,
    z: super::GRAVITY,
};
/// Below this a field counts as none.
const STILL_SQ: f32 = 0.0002;
/// How fast a body may move along the ground and still play the jump's lead-in.
const JUMP_LEAD_IN_SPEED: f32 = 2.0;

/// What the pushers a body is in or touches do to it this tick: whether it walks on air, and the
/// velocity and acceleration they give it. Added to before each tick from the objects its
/// collisions touch ([`Body::take_pushers`]); none by default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pushers {
    pub walk_on_air: bool,
    pub velocity: Vec3,
    pub acceleration: Vec3,
}

/// The world's wind, which effects and missiles feel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wind(pub Vec3);

impl Wind {
    // D2-10998
    pub fn set(&mut self, wind: Vec3) {
        self.0 = wind;
    }
}

impl Body {
    /// The element the body is in.
    // D2-06389, D2-05892
    #[must_use]
    pub const fn element(&self) -> u32 {
        self.element
    }

    /// The element the body is going to.
    // D2-06390, D2-05893
    #[must_use]
    pub const fn desired_element(&self) -> u32 {
        self.mode.element
    }

    /// The mode the body is going to.
    // D2-06391, D2-05895
    #[must_use]
    pub const fn desired_mode(&self) -> u32 {
        self.mode.mode
    }

    // D2-05894
    #[must_use]
    pub const fn mode(&self) -> u32 {
        self.mode.mode
    }

    /// The element and mode the body is in.
    // D2-06510
    #[must_use]
    pub const fn mode_state(&self) -> ModeKey {
        ModeKey {
            element: self.element,
            mode: self.mode.mode,
        }
    }

    /// The element and mode the body is going to.
    // D2-06511
    #[must_use]
    pub const fn desired_mode_state(&self) -> ModeKey {
        self.mode
    }

    /// Goes to an element. Another element than the one it is in wakes its animation and is news
    /// to the children it holds when next drawn; another than the one it is going to asks its
    /// animation to go there (the air's pose while it jumps or falls). A body with no figure is
    /// there at once; one with a figure is there when its figure is ([`Body::figure_stands_in`]).
    // D2-05882, D2-06377
    pub fn do_element(&mut self, element: u32) {
        if element != self.element {
            self.set_animation_active(true);
            self.element_changed = true;
        }
        if element != self.mode.element {
            // Its animation goes there too.
            self.notices
                .push(Notice::Animate(Animate::Element(element)));
        }
        self.mode.element = element;
        if !self.element_follows_figure {
            self.element = element;
        }
    }

    /// The element its figure stands in: the body's own from here on, which lags the element it
    /// is going to through the step of animation that takes it there.
    // D2-15163
    pub fn figure_stands_in(&mut self, element: u32) {
        self.element_follows_figure = true;
        self.element = element;
    }

    /// Goes to a mode.
    // D2-05883, D2-06376
    pub fn set_mode(&mut self, mode: u32) {
        self.mode.mode = mode;
    }

    /// The motion values of the element and mode the body is going to, else of the element in
    /// the everyday mode, else the defaults.
    // D2-05899, D2-06522
    #[must_use]
    pub fn desired_motion_values(&self) -> MotionParams {
        self.minterp.desired_motion_values(self.mode)
    }

    /// The motion values of walking on the earth in the everyday mode.
    // D2-05900, D2-06523
    #[must_use]
    pub fn standard_motion_values(&self) -> MotionParams {
        self.minterp.desired_motion_values(ModeKey {
            element: EARTH,
            mode: PEACE,
        })
    }

    /// The motion values of an element in the mode the body is going to, else the defaults.
    // D2-06520
    #[must_use]
    pub fn motion_values_for_element(&self, element: u32) -> MotionParams {
        self.minterp
            .find_motion_values(ModeKey {
                element,
                mode: self.mode.mode,
            })
            .unwrap_or_else(default_values)
    }

    // D2-06399
    #[must_use]
    pub fn sea_floor_multiplier(&self) -> f32 {
        self.minterp.sea_floor_multiplier()
    }

    // D2-06400
    #[must_use]
    pub fn water_multiplier(&self) -> f32 {
        self.minterp.water_multiplier()
    }

    /// How deep under the water's surface the body's water line is (above it: below zero).
    // D2-05872, D2-06432
    #[must_use]
    pub const fn depth(&self) -> f32 {
        self.depth
    }

    /// Whether the body's cell has water.
    // D2-06433
    #[must_use]
    pub const fn has_water(&self) -> bool {
        self.has_water
    }

    /// The height of the body above what is walkable under it, as measured at the tick's start.
    // D2-06181, D2-06431
    #[must_use]
    pub const fn height_above_walkable(&self) -> f32 {
        self.height_above
    }

    /// Gravity where the body is.
    // D2-05955
    #[must_use]
    pub const fn current_gravity(&self) -> Vec3 {
        self.local_gravity
    }

    /// Takes gravity from the material of the cell the body is in, when it gives one; whether it
    /// did.
    // D2-06049
    pub fn update_gravity(&mut self, material_gravity: Option<Vec3>) -> bool {
        match material_gravity {
            Some(g) => {
                self.local_gravity = g;
                true
            }
            None => false,
        }
    }

    /// The cell `at` is in updates the body's material with what holds there: its gravity is the
    /// body's, its properties are the surroundings' to the body's collisions.
    // D2-06156, D2-13693
    pub(crate) fn update_cell_material(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        at: &Position,
    ) {
        self.material_cell = Some(at.cell);
        let material = self.material.get_or_insert_with(Box::default);
        land.update_cell_material(at.cell, at, &mut material.properties, world.property_list());
        let gravity = material
            .properties
            .get(world.property_list(), crate::material::GRAVITY)
            .and_then(|p| p.value.inq_vector())
            .map(|v| Vec3::new(v.x, v.y, v.z));
        world.add_environmental_properties(key, material);
        self.update_gravity(gravity);
    }

    /// The cell the body last took its material from; none before the first.
    #[must_use]
    pub const fn material_cell(&self) -> Option<CellId> {
        self.material_cell
    }

    /// What the cells the body has been in made of it; none before the first.
    // D2-06044
    #[must_use]
    pub fn current_properties(&self) -> Option<&PropertyCollection> {
        self.material.as_ref().map(|m| &m.properties)
    }

    /// One property of what the cells the body has been in made of it.
    // D2-06045
    #[must_use]
    pub fn current_property(&self, list: &PropertyList, id: u32) -> Option<&Property> {
        self.current_properties()?.get(list, id)
    }

    /// The element from the water around the body (see the module's description).
    // D2-06135
    pub fn calc_element(&mut self) {
        if !self.is_mobile() {
            return;
        }
        let height = Owner::height(&*self);
        let water_line = self.water_multiplier();
        let floor_line = self.sea_floor_multiplier();
        let values = self.desired_motion_values();
        if self.walk_on_air {
            self.do_element(WALK_ON_AIR);
            self.wake_if_weightless(&values);
            return;
        }
        self.has_water = self.water.is_some();
        let Some(plane) = self.water else {
            if self.contact {
                self.wake_if_weightless(&values);
                self.do_element(EARTH);
            } else {
                self.do_element(AIR);
            }
            return;
        };
        let o = self.position.frame.origin;
        let below = -(plane.normal.x * o.x + plane.normal.y * o.y + plane.normal.z * o.z + plane.d);
        self.depth = below - water_line * height;
        if below - floor_line * height > 0.0 && self.depth < 0.0 {
            if self.contact {
                self.do_element(SEA_FLOOR);
            } else if self.velocity.z > 0.0 && self.depth < -0.3 {
                self.do_element(AIR);
            }
            return;
        }
        if self.depth <= 0.0 {
            if self.contact {
                self.do_element(EARTH);
                let values = self.desired_motion_values();
                self.wake_if_weightless(&values);
            } else {
                self.do_element(AIR);
            }
            return;
        }
        if self.desired_element() != WATER {
            self.handle_hit_water();
        }
        self.do_element(WATER);
        self.wake_if_weightless(&values);
    }

    /// A body whose motion values have no gravity touches nothing, and its other updates wake.
    fn wake_if_weightless(&mut self, values: &MotionParams) {
        if values.gravity == 0 {
            self.contact = false;
            self.set_misc_active(true);
        }
    }

    /// This tick's acceleration in the landblock's frame: gravity, lift, and the motion
    /// interpreter's acceleration turned by the body's heading.
    // D2-05953, D2-06306, D2-06512
    pub fn calc_acceleration(&mut self) {
        let mut a = Vec3::ZERO;
        let values = self.desired_motion_values();
        let gravity = self.current_gravity();
        if self.is_affected_by_gravity()
            && values.gravity != 0
            && !(self.contact && self.on_walkable)
        {
            a = a.add(gravity);
        }
        let element = self.desired_element();
        if (element == WATER || element == SEA_FLOOR) && !self.contact && self.has_water {
            if self.depth <= -1.0 || self.depth >= 1.0 {
                if self.depth > 1.0 {
                    a.z += values.lift;
                } else {
                    a = a.add(gravity);
                }
            } else {
                a.z += values.lift * self.depth;
            }
        } else if element == AIR {
            let mut lift = true;
            if values.gravity == 0 {
                let mut h = self.height_above_walkable();
                if self.has_water {
                    let over_water = -self.depth;
                    if over_water < h {
                        h = over_water;
                    }
                }
                let hover = self.minterp.get_hover_height(&*self);
                if (f64::from(h) - f64::from(hover)).abs() < f64::from(0.1_f32) {
                    lift = false;
                }
            }
            if lift {
                a.z += values.lift;
            }
        }
        self.acceleration = a.add(self.to_world(self.minterp.get_acceleration()));
    }

    /// What the objects the body touches push it with this tick, for where it is now.
    pub(crate) fn take_pushers(
        &mut self,
        world: &crate::world::World,
        key: crate::collision::manager::BodyKey,
    ) {
        let element = self.desired_element();
        if let Some(c) = world.collisions(key) {
            let mut p = self.pushers;
            c.update_pusher_values(
                element,
                &mut p.walk_on_air,
                &mut p.velocity,
                &mut p.acceleration,
            );
            self.pushers = p;
        }
    }

    /// The pushers' share of this tick's motion: their velocity and acceleration, and the speed
    /// their acceleration has built up (which bleeds away by a fifth a tick on the ground or in
    /// water, and is pulled down by gravity while it carries the body up, unless a pusher lifts
    /// it). Returns where they move the body this tick.
    // D2-06136
    pub fn calc_external_forces(&mut self, dt: f32) {
        let p = std::mem::take(&mut self.pushers);
        self.walk_on_air = p.walk_on_air;
        self.velocity_field = p.velocity;
        self.acceleration_field = p.acceleration;
        if self.contact || self.desired_element() == WATER {
            self.acceleration_field_velocity = self.acceleration_field_velocity.mul(0.8);
        }
        if self.external_field.mag2() < STILL_SQ {
            self.external_field = Vec3::ZERO;
        }
        if self.acceleration_field_velocity.mag2() < STILL_SQ {
            self.acceleration_field_velocity = Vec3::ZERO;
        }
        if self.velocity_field.z > 0.0002 || self.acceleration_field.z > 0.0002 {
            self.contact = false;
            self.set_misc_active(true);
            self.set_affected_by_gravity(false);
        } else {
            self.set_affected_by_gravity(true);
            if self.acceleration_field_velocity.z > 0.0 {
                self.acceleration_field_velocity =
                    self.acceleration_field_velocity.add(self.current_gravity());
            }
        }
        let half = 0.5 * dt * dt;
        self.external_field = self
            .velocity_field
            .mul(dt)
            .add(self.acceleration_field_velocity.mul(dt))
            .add(self.acceleration_field.mul(half));
        self.acceleration_field_velocity = self
            .acceleration_field_velocity
            .add(self.acceleration_field);
    }

    /// Landing: the motion interpreter hears of it unless the body is going into water or onto
    /// the sea floor; the time on the ground restarts; the speed the pushers built up is kept as
    /// the body's own.
    // D2-05976, D2-06420
    pub fn handle_hit_ground(&mut self) {
        let e = self.desired_element();
        if e != SEA_FLOOR && e != WATER {
            let mut mi = std::mem::take(&mut self.minterp);
            mi.hit_ground(self);
            self.minterp = mi;
        }
        self.time_on_ground = 0.0;
        self.take_field_velocity();
    }

    /// Entering water: the speed the pushers built up is kept as the body's own.
    // D2-05977
    pub fn handle_hit_water(&mut self) {
        self.take_field_velocity();
    }

    fn take_field_velocity(&mut self) {
        self.velocity = self.velocity.add(self.acceleration_field_velocity);
        self.acceleration_field_velocity = Vec3::ZERO;
    }

    /// Whether the body may jump: touching the ground for half a second, not already starting a
    /// jump, and both in and going to the earth or the sea floor.
    // D2-05943
    #[must_use]
    pub fn can_jump(&self) -> bool {
        if !self.contact || self.time_on_ground < super::JUMP_COOLDOWN {
            return false;
        }
        if self.behaviors.contains(&super::JUMP_BEHAVIOR) {
            return false;
        }
        let ground = |e: u32| e == EARTH || e == SEA_FLOOR;
        ground(self.desired_element()) && ground(self.element())
    }

    /// Jumps: off the ground, up at the jump impulse of walking on the earth times the jump scale
    /// and the body's own height scale (at least half; not its look's), into the air.
    // D2-06134
    pub fn jump(&mut self) -> bool {
        if !self.contact {
            return false;
        }
        let ground = |e: u32| e == EARTH || e == SEA_FLOOR;
        if !ground(self.desired_element()) || !ground(self.element()) {
            return false;
        }
        self.contact = false;
        self.set_misc_active(true);
        let scale = self.object_scale.z.max(0.5);
        let values = self.motion_values_for_element(EARTH);
        self.velocity.z = self.jump_scale * values.jump_impulse * scale;
        self.do_element(AIR);
        self.jumps += 1;
        true
    }

    /// The jump asked for: if the body may jump, it stops landing; then, moving under 2 m/s
    /// along the ground, the player's body starts the jump's lead-in (which jumps when it ends);
    /// otherwise it jumps at once. Whether it may jump.
    // D2-06180
    pub fn jump_logic(&mut self) -> bool {
        if !self.can_jump() {
            return false;
        }
        let mut v = self.local_velocity();
        v.z = 0.0;
        self.behaviors.remove(&shows::LANDING);
        if v.mag2().sqrt() < JUMP_LEAD_IN_SPEED && self.is_player {
            self.behaviors.insert(super::JUMP_BEHAVIOR);
            return true;
        }
        self.jump();
        true
    }

    /// Hover height through the body: the motion interpreter ignores it (its setter does nothing),
    /// so a hover height the server sends has no effect.
    // D2-06061, D2-06412, D2-05857
    pub fn set_hover_height(&mut self, _height: f32) {}

    // D2-06062, D2-06413
    pub fn reset_hover_height(&mut self) {
        self.minterp.reset_hover_height();
    }

    // D2-06063, D2-06414
    #[must_use]
    pub fn hover_height(&self) -> f32 {
        self.minterp.get_hover_height(self)
    }

    // D2-06064, D2-06415
    #[must_use]
    pub fn default_hover_height(&self) -> f32 {
        self.minterp.default_hover_height(self)
    }

    // D2-06065, D2-06416
    #[must_use]
    pub const fn velocity_scale(&self) -> f32 {
        self.minterp.velocity_scale
    }

    // D2-06066, D2-06417
    #[must_use]
    pub const fn acceleration_scale(&self) -> f32 {
        self.minterp.acceleration_scale
    }

    /// The server's speed scale; the slowest side impact reported grows with it (a tenth of a
    /// meter a second over the scaled forward speed).
    // D2-06067, D2-06418, D2-05855
    pub fn set_velocity_scale(&mut self, scale: f32) {
        self.minterp.velocity_scale = scale;
        self.min_impact.0 = scale * self.standard_motion_values().max_forward + 0.1;
    }

    // D2-06068, D2-06419, D2-05854
    pub fn set_acceleration_scale(&mut self, scale: f32) {
        self.minterp.acceleration_scale = scale;
    }

    /// The server's jump scale (none below zero); the slowest landing reported grows with it.
    // D2-06069, D2-05856
    pub fn set_jump_scale(&mut self, scale: f32) {
        if scale.is_nan() || scale < 0.0 {
            return;
        }
        self.jump_scale = scale;
        self.min_impact.1 = self.standard_motion_values().jump_impulse * scale + 0.1;
    }

    // D2-06070
    #[must_use]
    pub const fn jump_scale(&self) -> f32 {
        self.jump_scale
    }

    /// The slowest impacts reported, along the ground and up or down.
    #[must_use]
    pub const fn min_reported_impact(&self) -> (f32, f32) {
        self.min_impact
    }

    // D2-05877, D2-06408, D2-05841, D2-15853
    pub fn set_global_modifier(&mut self, modifier: u32) {
        self.minterp.global_mod = modifier;
    }

    // D2-05878, D2-06409, D2-05842
    pub fn add_terrain_modifier(&mut self, terrain: u32, modifier: u32) {
        self.minterp.add_terrain_modifier(terrain, modifier);
    }

    // D2-05879, D2-06410, D2-05843
    pub fn clear_terrain_modifiers(&mut self) {
        self.minterp.clear_terrain_modifiers();
    }

    /// Whether the body is moving to something the server ordered.
    // D2-05901, D2-06406
    #[must_use]
    pub fn is_moving_to(&self) -> bool {
        self.minterp.is_moving_to()
    }

    // D2-06407
    #[must_use]
    pub const fn is_turning_to(&self) -> bool {
        self.minterp.turning_to
    }

    /// The velocity the motion interpreter asks for, in the body's frame.
    // D2-06513
    #[must_use]
    pub const fn desired_velocity(&self) -> Vec3 {
        self.minterp.local_desired_velocity
    }

    /// The rate of turn's acceleration this tick.
    // D2-06514, D2-06328, D2-07039
    #[must_use]
    pub const fn omega_acceleration(&self) -> Vec3 {
        Vec3 {
            x: 0.0,
            y: 0.0,
            z: self.turn_acceleration,
        }
    }

    /// The plane of what the body touches and its cell.
    // D2-06317, D2-06318, D2-06493
    #[must_use]
    pub const fn contact_plane(&self) -> Option<(Plane, dereth_primitives::CellId)> {
        self.contact_plane
    }
}
