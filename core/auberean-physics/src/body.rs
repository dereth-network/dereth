//! A body that moves: the physics step that turns the motion interpreter's accelerations, gravity
//! and the ground into where the body is.
//!
//! Physics runs in fixed ticks of a tenth of a second ([`Ticker`]). Each tick the motion
//! interpreter accelerates the body toward what was asked for (by the player, a move the server
//! ordered, or the positions the server sends for it); gravity pulls it unless it stands on
//! walkable ground; it moves by its velocity over the tick (a body at rest moves nothing on the
//! tick it starts: its velocity rises first), no farther across the ground than interpolation
//! allows and along its rail; it turns by its rate of turn, stopping on a heading it is turning
//! to; then the shared transition carries it from where it was to where it would be, against the
//! ground and the solid objects of this world's cells, and says whether it stands on something.
//!
//! Inside, what stands in the interior cells (pillars, steps, trees, furniture) is solid too: the
//! first time the body is in a cell of a block, the solid parts placed in that block's interior
//! cells become static objects of the body's object table, each in the collision list of its own
//! cell and of every cell it sees whose bounds it reaches. The objects the host puts in the world
//! (signs, statues, people standing still) are in the table too, each in the collision lists of
//! the cells it reaches, inside or out ([`crate::world::WorldObject`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dereth_primitives::num::math;

use auberean_assets::animation::{ModeKey, MotionDesc, MotionParams, MotionSet};
use auberean_assets::property::PhysicsMaterial;
use dereth_physics::geom::{Plane, Sphere};
use dereth_physics::transition::insert::find_transitional_position;
use dereth_physics::transition::{ObjectInfoState, Transition};
use dereth_physics::{PhysicsState, V3};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

mod animate;
mod collide;
mod forces;
mod interp;
mod measure;
pub mod missile;
mod moving;
pub mod parent;
pub mod placing;
mod presence;
pub mod profile;
mod rep;
mod state;
mod stepping;
pub mod watch;

pub use animate::{Animate, MOVE_TO_STAMP, POSITION_STAMP, TELEPORT_STAMP};
pub use collide::Notice;
pub use forces::{Pushers, Wind, DEFAULT_GRAVITY, WALK_ON_AIR};
pub use measure::{cylinder_distance_squared, offset_position, ENVIRONMENT_ONLY};
pub use state::{flags, transient};

use crate::collision::manager::BodyKey;
use crate::detect::manager::Detections;
use crate::geom::{self, Heading};
use crate::land::Land;
use crate::missile::Guidance;
use crate::motion::moveto::TargetBody;
use crate::motion::params::MovementReturn;
use crate::motion::values::empty_description;
use crate::motion::{MotionInput, MotionInterp, Owner, AIR, EARTH, PEACE, WATER};
use crate::rules::rules;
use crate::target::Tracking;
use crate::world::World;

/// The physics tick, in seconds.
pub const TICK: f64 = 0.1;
/// A backlog longer than this is dropped rather than run.
pub const MAX_BACKLOG: f64 = 1.0;
/// Gravity, meters per second squared, down.
pub const GRAVITY: f32 = -9.8;
/// How long a body must stand on the ground before it may jump again.
pub const JUMP_COOLDOWN: f64 = 0.5;
/// The behavior a jump plays before it leaves the ground.
pub const JUMP_BEHAVIOR: u32 = 0x4000_000A;
/// Below this a value counts as zero.
const EPS: f32 = 0.0002;
/// Far above anything walkable.
const HIGH_ABOVE: f32 = 1000.0;

/// When physics ticks: in whole tenths of a second as the frames' time passes.
// D2-13682, D2-13683
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ticker {
    /// The time physics has reached.
    pub reached: f64,
    /// The time of the tick before.
    pub previous: f64,
}

impl Ticker {
    #[must_use]
    // D2-05760
    pub const fn new(now: f64) -> Self {
        Self {
            reached: now,
            previous: now,
        }
    }

    /// How many ticks the frame at time `now` runs: none before a tenth of a second has passed,
    /// none (and the backlog dropped) after more than a second, or when time ran backward.
    // D2-13684, D2-05761, D2-05763
    pub fn ticks(&mut self, now: f64) -> u32 {
        let mut d = now - self.reached;
        if d < 0.0 || d > MAX_BACKLOG {
            self.reached = now;
            self.previous = now;
            return 0;
        }
        let mut n = 0;
        while d >= TICK {
            self.previous = self.reached;
            self.reached += TICK;
            d -= TICK;
            n += 1;
        }
        n
    }

    /// How far the frame at time `now` is past the last tick, as a share of the span between the
    /// last two ticks (a tick, once ticks run; none when they have not), held to 0 to 1: what a
    /// body is drawn at between its last two ticks.
    // D2-05765
    #[must_use]
    pub fn alpha(&self, now: f64) -> f32 {
        let span = self.reached - self.previous;
        #[allow(clippy::cast_possible_truncation)]
        let a = if span.abs() <= 0.0 {
            0.0
        } else {
            ((now - self.reached) / span) as f32
        };
        if a > 0.0 {
            a.min(1.0)
        } else {
            0.0
        }
    }
}

/// The animation's step: a thirtieth of a second.
pub const ANIM_QUANTUM: f32 = 0.033_333_335;
/// The longest time since the last step that one step takes whole.
pub const ANIM_CATCH_UP: f32 = 0.2;
/// The step taken after a longer gap.
pub const ANIM_AFTER_GAP: f32 = 0.05;

/// When animation steps: once a frame at least a thirtieth of a second after the last, by all the
/// time since; after a gap of more than a fifth of a second, by a twentieth of a second only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationClock {
    pub last: f64,
}

impl AnimationClock {
    #[must_use]
    pub const fn new(now: f64) -> Self {
        Self { last: now }
    }

    /// The animation step the frame at `now` runs, if any.
    // D2-13685, D2-05764
    pub fn step(&mut self, now: f64) -> Option<f32> {
        #[allow(clippy::cast_possible_truncation)]
        let e = (now - self.last) as f32;
        if (now - self.last) < 0.0 {
            self.last = now;
            return None;
        }
        if e < ANIM_QUANTUM {
            return None;
        }
        self.last = now;
        Some(if e > ANIM_CATCH_UP { ANIM_AFTER_GAP } else { e })
    }
}

/// What a body is: its collision spheres, its step heights, its scale, and its motion values on
/// the ground and in the air.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyConfig {
    pub spheres: Vec<Sphere>,
    pub step_up: f32,
    // D2-06461
    pub step_down: f32,
    pub scale: f32,
    pub ground: MotionParams,
    pub air: MotionParams,
}

impl BodyConfig {
    /// The body's motion description: its ground values on the earth and its air values in the
    /// air, in the everyday mode.
    #[must_use]
    pub fn description(&self) -> MotionDesc {
        let set = |v: MotionParams| MotionSet {
            by_terrain: Vec::new(),
            by_modifier: Vec::new(),
            default: v,
        };
        MotionDesc {
            set_of: vec![
                (
                    ModeKey {
                        element: EARTH,
                        mode: PEACE,
                    },
                    0,
                ),
                (
                    ModeKey {
                        element: AIR,
                        mode: PEACE,
                    },
                    1,
                ),
            ],
            sets: vec![set(self.ground), set(self.air)],
            ..empty_description()
        }
    }
}

/// A moving body.
// D2-06979, D2-06970
#[derive(Debug)]
pub struct Body {
    pub config: BodyConfig,
    /// Where it is: its cell and its frame in that cell's landblock.
    // D2-06334
    pub position: Position,
    /// Degrees clockwise from north (+y).
    pub heading: f32,
    /// Where it was and its heading before the last tick: a body is drawn between the two.
    pub previous: (Position, f32),
    /// Meters per second, in the landblock's frame.
    pub velocity: Vec3,
    /// Degrees per second, left positive.
    pub turn_rate: f32,
    pub turn_acceleration: f32,
    pub contact: bool,
    pub on_walkable: bool,
    pub contact_plane: Option<(Plane, CellId)>,
    pub sliding_normal: Option<Vec3>,
    pub time_on_ground: f64,
    pub jumps: u32,
    pub jump_scale: f32,
    pub stationary: u32,
    /// The heading a turn stops on, and the rate of turn a turn may not pass.
    pub heading_clamp: Option<Heading>,
    pub omega_clamp: Option<f32>,
    /// How far across the ground the body may still move (below zero: no limit), and the
    /// direction its motion across the ground keeps to.
    pub distance_cap: f32,
    pub rail: Option<Vec3>,
    /// Following the transition's result exactly.
    pub synchronize: bool,
    /// The body's own clock: the ticks it has run.
    pub clock: f64,
    /// Whether this is the player's own body.
    // D2-06060, D2-06495
    pub is_player: bool,
    /// The behaviors the body is showing, and the effects it played (taken by the host).
    pub behaviors: BTreeSet<u32>,
    pub effects: Vec<(u32, f32)>,
    /// The element and mode changes the body asked of its animation, in order (taken by the
    /// host, which hands them to the body's mode tracker).
    pub modes: Vec<(u32, u32)>,
    /// The objects a move may go to, kept by the host.
    pub targets: BTreeMap<u64, TargetBody>,
    /// The moves that ended (taken by the host).
    pub moves_done: Vec<MovementReturn>,
    /// The body's height above what is walkable under it, worked out at the start of a tick.
    pub height_above: f32,
    /// What the body is, and what it is doing besides touching and walking ([`state`]).
    state: u32,
    transient: u32,
    /// The element it is in, and the element and mode it is going to.
    element: u32,
    /// It has a figure, whose word on the element it is in is taken.
    element_follows_figure: bool,
    mode: ModeKey,
    /// Where it was two ticks back.
    pos_anim: Position,
    /// How fast it moved over the last tick, in its frame then.
    // D2-06494
    pub anim_velocity: Vec3,
    /// This tick's acceleration, in the landblock's frame.
    pub acceleration: Vec3,
    /// The pushers' velocity and acceleration this tick, the speed their acceleration has built
    /// up, and where they move the body this tick.
    pub velocity_field: Vec3,
    pub acceleration_field: Vec3,
    pub acceleration_field_velocity: Vec3,
    pub external_field: Vec3,
    /// What the pushers will do next tick (taken by the tick).
    pub pushers: Pushers,
    pub walk_on_air: bool,
    /// Gravity where the body is.
    pub local_gravity: Vec3,
    /// What the cells the body has been in made of it ([`crate::material`]); none until the first.
    pub material: Option<Box<PhysicsMaterial>>,
    /// The cell that last updated the material.
    material_cell: Option<CellId>,
    /// The water plane of the body's cell, the body's depth under it, and whether there is one.
    pub water: Option<Plane>,
    depth: f32,
    has_water: bool,
    /// The slowest impacts reported, along the ground and up or down.
    min_impact: (f32, f32),
    /// The object's id: the host's (0 until it gives one).
    // D2-06272, D2-06398
    pub id: u64,
    /// In the world; carried by a parent.
    // D2-06282
    pub in_world: bool,
    // D2-06331
    pub has_parent: bool,
    /// The object holding it (0: none), and what it holds, once it holds anything ([`parent`]).
    pub parent: u64,
    pub children: Option<parent::ChildList>,
    /// Its element changed since it was last drawn with its children.
    pub element_changed: bool,
    /// The cell it stands in is among the free blocks.
    cell_on_free_list: bool,
    /// It was past the player's physics range when last looked at.
    out_of_physics_range: bool,
    /// Its guidance, when it flies as a missile ([`missile`]).
    pub missile: Option<Box<Guidance>>,
    /// What it last went off on as a missile (0 for no object), for the host to read.
    pub struck: Option<u64>,
    /// The object it follows and the objects following it, once it has either ([`watch`]).
    pub tracking: Option<Tracking>,
    /// Its detection spheres and the detections it makes at once, once it has any ([`watch`]).
    pub detections: Option<Detections>,
    /// What the body asks of its game object, its host and the other objects, in order (taken by
    /// the host).
    pub notices: Vec<Notice>,
    /// The object its targeted effects aim at, and the look its effects clone (none: 0).
    pub fx_target: u64,
    pub look_to_clone: u32,
    /// The appearance table and values laid over the look its effects clone.
    pub clone_appearance: u32,
    pub clone_values: Vec<(u32, f32)>,
    /// Its setup's radius and height, unscaled: what it measures itself by (its spheres' until
    /// the host gives them).
    pub setup_radius: f32,
    pub setup_height: f32,
    /// The stamps of the updates the server sent, by kind ([`animate`]).
    pub update_times: [u16; 4],
    /// Its motion interpreter.
    // D2-05906, D2-06395
    pub minterp: MotionInterp,
    /// The world the body moves against when it ticks on its own ([`Body::tick`]), and its entry
    /// there.
    world: World,
    own_key: Option<BodyKey>,
    /// Its entry in the world it ticks in ([`Body::tick_in`]).
    pub(crate) key: Option<BodyKey>,
    /// Where it was last put that is to count as its placement.
    pub placement: Option<Position>,
    /// The part group its look puts together, once the host gives it one ([`rep`]).
    pub parts: Option<Box<crate::parts::PartGroup>>,
    /// The scale its look gives it, the scale its look gives what it holds, and its own scale.
    pub look_scale: Vec3,
    pub child_scale: f32,
    pub object_scale: Vec3,
    /// What its undetectable setter was asked since the world it ticks in last heard.
    world_told: Vec<bool>,
}

/// The rotation of a heading, degrees clockwise from north.
#[must_use]
pub fn heading_rotation(heading: f32) -> Quat {
    let half = -heading.to_radians() * 0.5;
    let mut f = Frame::default();
    let mut q = Quat::new(math::cosf(half), 0.0, 0.0, math::sinf(half));
    // Unit length as a frame keeps it: made unit length again, it stays as it is.
    for _ in 0..4 {
        dereth_physics::math::set_rotate_plain(&mut f, q);
        if f.rotation == q {
            break;
        }
        q = f.rotation;
    }
    q
}

impl Body {
    /// A body standing at `position`, facing `heading`, at rest on the ground.
    // D2-06352, D2-06981
    #[must_use]
    pub fn new(config: BodyConfig, position: Position, heading: f32) -> Self {
        let mut minterp = MotionInterp::with_description(0, Arc::new(config.description()));
        let mut position = position;
        position.frame.rotation = heading_rotation(heading);
        minterp.turn_to = Heading::new(heading);
        let config_scale = config.scale;
        let setup_radius = config.spheres.first().map_or(0.0, |s| s.radius);
        let setup_height = config
            .spheres
            .iter()
            .map(|s| s.center.z + s.radius)
            .fold(0.0, f32::max);
        Self {
            config,
            position,
            heading,
            previous: (position, heading),
            velocity: Vec3::ZERO,
            turn_rate: 0.0,
            turn_acceleration: 0.0,
            contact: true,
            on_walkable: true,
            contact_plane: None,
            sliding_normal: None,
            time_on_ground: 0.0,
            jumps: 0,
            jump_scale: 1.0,
            stationary: 0,
            heading_clamp: None,
            omega_clamp: None,
            distance_cap: -1.0,
            rail: None,
            synchronize: false,
            clock: 0.0,
            is_player: true,
            behaviors: BTreeSet::new(),
            effects: Vec::new(),
            modes: Vec::new(),
            targets: BTreeMap::new(),
            moves_done: Vec::new(),
            height_above: 0.0,
            state: flags::NEW,
            transient: transient::NEW,
            element: EARTH,
            element_follows_figure: false,
            mode: ModeKey {
                element: EARTH,
                mode: PEACE,
            },
            pos_anim: position,
            anim_velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            velocity_field: Vec3::ZERO,
            acceleration_field: Vec3::ZERO,
            acceleration_field_velocity: Vec3::ZERO,
            external_field: Vec3::ZERO,
            pushers: Pushers::default(),
            walk_on_air: false,
            local_gravity: DEFAULT_GRAVITY,
            material: None,
            material_cell: None,
            water: None,
            depth: 0.0,
            has_water: false,
            min_impact: (0.0, 0.0),
            id: 0,
            in_world: true,
            has_parent: false,
            parent: 0,
            children: None,
            element_changed: false,
            cell_on_free_list: false,
            out_of_physics_range: false,
            missile: None,
            struck: None,
            tracking: None,
            detections: None,
            notices: Vec::new(),
            fx_target: 0,
            look_to_clone: 0,
            clone_appearance: 0,
            clone_values: Vec::new(),
            setup_radius,
            setup_height,
            update_times: [0; 4],
            minterp: {
                minterp.move_to = Some(crate::motion::moveto::Mover::default());
                minterp
            },
            world: World::new(),
            own_key: None,
            key: None,
            placement: None,
            parts: None,
            look_scale: Vec3::new(config_scale, config_scale, config_scale),
            child_scale: 1.0,
            object_scale: Vec3::new(1.0, 1.0, 1.0),
            world_told: Vec::new(),
        }
    }

    /// How many objects stand in the collision list of cell `cell` of the world the body ticks
    /// in on its own.
    #[must_use]
    pub fn statics_in(&self, cell: CellId) -> usize {
        self.world.in_cell(cell)
    }

    /// The body's entry in the world it ticks in ([`World::insert_body`]).
    #[must_use]
    pub const fn key(&self) -> Option<BodyKey> {
        self.key
    }

    /// The body's velocity in its own frame: x to its right, y ahead.
    // D2-05991, D2-06439
    #[must_use]
    pub fn local_velocity(&self) -> Vec3 {
        let (s, c) = math::sin_cosf(self.heading.to_radians());
        Vec3::new(
            self.velocity.x * c - self.velocity.y * s,
            self.velocity.x * s + self.velocity.y * c,
            self.velocity.z,
        )
    }

    fn to_world(&self, v: Vec3) -> Vec3 {
        let (s, c) = math::sin_cosf(self.heading.to_radians());
        Vec3::new(v.x * c + v.y * s, -v.x * s + v.y * c, v.z)
    }

    /// Ask for movement and turning this frame.
    pub fn steer(&mut self, input: MotionInput) {
        self.minterp.steer(input);
    }

    /// Whether the body stands on ground too steep to walk.
    // D2-05944
    #[must_use]
    pub const fn is_sliding_down(&self) -> bool {
        self.contact && !self.on_walkable
    }

    /// The body's height above the ground under it: none while touching it.
    fn measure_height(&self, land: &Land) -> f32 {
        if self.contact {
            return 0.0;
        }
        let at = self.position.frame.origin;
        land.ground(self.position.cell.landblock(), at)
            .map_or(HIGH_ABOVE, |(z, _)| (at.z - z).max(0.0))
    }

    /// One physics tick of `dt` seconds. A body out of the world, frozen, not mobile or carried by
    /// a parent does not move (its movement updates stop), unless it flies as a missile.
    pub fn tick(&mut self, land: &Land, dt: f32) {
        let mut world = std::mem::take(&mut self.world);
        let key = match self.own_key.filter(|k| world.handle(*k).is_some()) {
            Some(k) => k,
            None => world.insert_mover(land, 0, self),
        };
        self.own_key = Some(key);
        self.step(land, &mut world, key, &mut missile::NoObjects, dt);
        self.world = world;
    }

    /// One physics tick of `dt` seconds against `world`, shared with the other bodies: the body
    /// meets what the world holds, and its entry there moves with it. A body not yet in the world
    /// is put in it ([`World::insert_body`], with no object id).
    pub fn tick_in(&mut self, land: &Land, world: &mut World, dt: f32) {
        self.tick_with(land, world, &mut missile::NoObjects, dt);
    }

    /// One physics tick against `world`, as [`Body::tick_in`], with the other objects a missile
    /// asks after.
    pub fn tick_with(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn missile::Objects,
        dt: f32,
    ) {
        let key = match self.key.filter(|k| world.handle(*k).is_some()) {
            Some(k) => k,
            None => world.insert_body(land, 0, self),
        };
        self.step(land, world, key, objects, dt);
    }

    // D2-06344, D2-06265, D2-06213, D2-06243, D2-05952, D2-06137, D2-05990
    fn step(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        objects: &mut dyn missile::Objects,
        dt: f32,
    ) {
        self.tell_world(world, key);
        if self.material_cell != Some(self.position.cell) {
            let at = self.position;
            self.update_cell_material(land, world, key, &at);
        }
        self.advance(land, world, key, objects, dt);
        if let Some(terrain) = world.take_terrain(key) {
            self.set_terrain_type(terrain);
        }
        if world.position(key) != Some(self.position) {
            world.place(land, key, self.position);
        }
    }

    /// What the body stands on has a new terrain type: its motion takes the values for it, and its
    /// effects from now on play their variants for it.
    // D2-06336, D2-06447
    pub fn set_terrain_type(&mut self, terrain: u32) {
        self.minterp.set_terrain_type(terrain);
        self.notices.push(Notice::TerrainType(terrain));
    }

    fn advance(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        objects: &mut dyn missile::Objects,
        dt: f32,
    ) {
        let still =
            self.has_parent || !self.in_world || self.is_movement_frozen() || !self.is_mobile();
        if still && self.missile.is_none() {
            self.set_movement_active(false);
            return;
        }
        self.pos_anim = self.previous.0;
        self.previous = (self.position, self.heading);
        self.clock += f64::from(dt);
        if self.missile.is_some() {
            // A missile's guidance steps, then it moves as its guidance says.
            self.missile_update(land, world, objects);
            if !self.in_world {
                return;
            }
            let d = self.missile_step(land, world, objects, dt);
            if self.is_mobile() {
                self.move_by(land, world, key, d, self.heading);
            }
            return;
        }
        if self.contact {
            self.time_on_ground += f64::from(dt);
        }
        self.height_above = self.measure_height(land);
        self.water = land.water_plane(self.position.cell);
        let now = self.clock;
        self.calc_element();
        self.update_motion(now, dt);
        self.calc_acceleration();
        self.turn_acceleration = self.minterp.omega_acceleration.z;
        self.take_pushers(world, key);
        self.calc_external_forces(dt);
        let values = self.minterp.motion_values(&*self);
        let mut d = Vec3::ZERO;
        let v2 = self.velocity.mag2();
        if v2 > 0.0 || self.external_field.mag2() > 0.0 {
            let terminal = values.terminal_speed;
            if terminal > 0.0 && v2 > terminal * terminal {
                self.velocity = self.velocity.mul(terminal / v2.sqrt());
            }
            let a = self.acceleration;
            if v2 - 0.002_500_000_2 < EPS && a.x.abs() < EPS && a.y.abs() < EPS && a.z.abs() < EPS {
                self.velocity = Vec3::ZERO;
                self.acceleration = Vec3::ZERO;
            }
            let a = self.acceleration;
            d = self
                .velocity
                .mul(dt)
                .add(self.external_field)
                .add(a.mul(0.5 * dt * dt));
            if self.desired_element() != AIR {
                d = self.limit_across(d);
            }
        }
        let moved = geom::offset(&self.pos_anim, &self.position).mul(10.0);
        self.anim_velocity = geom::global_to_local(&self.pos_anim.frame, moved);
        let mut heading = self.heading;
        let wa = self.turn_acceleration;
        if self.turn_rate != 0.0 || wa != 0.0 {
            if self.turn_rate * self.turn_rate - 0.0025 >= EPS || wa.abs() >= EPS {
                heading = self.update_rotation(dt, heading);
            } else {
                self.turn_rate = 0.0;
                self.turn_acceleration = 0.0;
            }
        }
        self.velocity = self.velocity.add(self.acceleration.mul(dt));
        self.calc_friction(dt);
        let heading = heading.rem_euclid(360.0);
        self.move_by(land, world, key, d, heading);
    }

    /// Moves the body by `d`, facing `heading`: a move under 0.0002 m on every axis only turns
    /// it; one aligned to its path faces the way it moves; then the transition carries it, and it
    /// hears from its target and tells its watchers.
    fn move_by(&mut self, land: &Land, world: &mut World, key: BodyKey, d: Vec3, heading: f32) {
        let mut heading = heading;
        let mut rotation = heading_rotation(heading);
        self.heading = heading;
        if d.x.abs() < EPS && d.y.abs() < EPS && d.z.abs() < EPS {
            self.position.frame.rotation = rotation;
            return;
        }
        if self.is_path_aligned() {
            let mut f = self.position.frame;
            f.rotation = rotation;
            geom::set_vector_heading(&mut f, self.velocity);
            rotation = f.rotation;
            heading = geom::frame_heading(&f).degrees();
            self.heading = heading;
        }
        let from = self.position;
        let mut to = from;
        to.frame.origin = from.frame.origin.add(d);
        to.frame.rotation = rotation;
        self.transition(land, world, key, from, to);
        self.handle_targetting();
    }

    /// The world hears what the body's setters changed of its entry there: each time it was made
    /// undetectable or detectable (the cell's watchers told each time), and whether it ignores
    /// collisions.
    pub(crate) fn tell_world(&mut self, world: &mut World, key: BodyKey) {
        for on in std::mem::take(&mut self.world_told) {
            world.set_undetectable(key, on);
        }
        world.set_ignore_collisions(key, self.has(flags::IGNORE_COLLISIONS));
        world.set_mobile(key, self.has(flags::MOBILE));
        world.set_reporting(key, self.has(flags::REPORT_COLLISIONS));
        if self.min_impact != (0.0, 0.0) {
            if let Some(c) = world.collisions_mut(key) {
                c.set_min_reported_impact_xy(self.min_impact.0);
                c.set_min_reported_impact_z(self.min_impact.1);
            }
        }
    }

    /// Drag, where the motion values have it: the velocity decays by `(1 - drag)` a second, all of
    /// it when nothing pushes the body, else all but the part along the push; on walkable ground,
    /// a velocity nearly along the ground is laid onto it.
    // D2-05954
    fn calc_friction(&mut self, dt: f32) {
        let m = self
            .minterp
            .desired_motion_values(Owner::desired_mode_state(&*self));
        if m.drag == 0.0 {
            return;
        }
        let k = math::powf(1.0 - m.drag, dt);
        let la = self.minterp.acceleration;
        let v = self.velocity;
        if la.y * la.y + la.x * la.x + la.z * la.z <= crate::geom::EPS_SQ {
            self.velocity = v.mul(k);
        } else {
            let g = self.to_world(la);
            let n = (g.x * g.x + g.y * g.y + g.z * g.z).sqrt();
            let g = Vec3::new(g.x / n, g.y / n, g.z / n);
            let d = g.x * v.x + g.y * v.y + g.z * v.z;
            let along = if d <= 0.0 { Vec3::ZERO } else { g.mul(d) };
            self.velocity = v.sub(along).mul(k).add(along);
        }
        if self.on_walkable {
            if let Some((plane, _)) = self.contact_plane {
                let n = plane.normal;
                let v = self.velocity;
                let d = v.x * n.x + v.y * n.y + v.z * n.z;
                if d.abs() < 0.25 {
                    self.velocity = v.sub(n.mul(d));
                }
            }
        }
    }

    /// Interpolation's limits on a move across the ground: no farther than the distance left (which
    /// then shrinks by the move), and along the rail.
    fn limit_across(&mut self, d: Vec3) -> Vec3 {
        let (mut x, mut y) = (d.x, d.y);
        if self.distance_cap >= 0.0 {
            let len = (x * x + y * y).sqrt();
            if self.distance_cap < len {
                if len < EPS {
                    (x, y) = (0.0, 0.0);
                } else {
                    let k = 1.0 / len;
                    (x, y) = (self.distance_cap * (x * k), self.distance_cap * (k * y));
                }
            }
            self.distance_cap -= len;
        }
        if let Some(r) = self.rail {
            let l = (d.z * d.z + x * x + y * y).sqrt();
            (x, y) = (l * r.x, l * r.y);
        }
        Vec3::new(x, y, d.z)
    }

    /// Turns by the rate of turn over the tick, stopping on the heading a turn-to clamps it to; the
    /// rate then rises by the turning acceleration, never past the rate the turn may not pass.
    // D2-05951
    fn update_rotation(&mut self, dt: f32, heading: f32) -> f32 {
        let w = self.turn_rate;
        let wa = self.turn_acceleration;
        let k = 0.005_555_555_7 * std::f32::consts::PI;
        let mut theta = w * k * dt + wa * 0.5 * k * dt * dt;
        let mut heading = heading;
        if let Some(target) = self.heading_clamp {
            let current = Heading::new(heading).radians();
            let mut t = target.radians();
            let crosses = if theta >= 0.0 {
                if current < t {
                    t -= 6.283_185_5;
                }
                t > current - theta || t.is_nan()
            } else {
                if t < current {
                    t += 6.283_185_5;
                }
                current - theta > t || t.is_nan()
            };
            if crosses {
                heading = target.degrees();
                self.turn_rate = 0.0;
                self.turn_acceleration = 0.0;
                theta = 0.0;
            }
        }
        heading -= theta.to_degrees();
        let w = self.turn_rate;
        let wa = self.turn_acceleration;
        let mut next = dt * wa + w;
        if let Some(c) = self.omega_clamp {
            if w < c && c < next {
                next = c;
            }
            if c < w && next < c {
                next = c;
            }
        }
        self.turn_rate = next;
        heading
    }

    fn transition(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        from: Position,
        to: Position,
    ) {
        let r = rules();
        let mut t = Transition::default();
        t.init();
        t.set_rules(r);
        world.add_scenery(land, from.cell);
        t.object_info.object = world.handle(key);
        t.object_info.state = self.object_info(&mut t);
        // D2-05825
        t.object_info.scale = self.config.scale;
        t.object_info.step_up_height = self.step_up_height();
        t.object_info.step_down_height = self.step_down_height();
        t.object_info.step_down = t.object_info.step_down_height != 0.0;
        t.collision_info.frames_stationary_fall = if self.is_stationary_stuck() {
            3
        } else if self.is_stationary_stop() {
            2
        } else {
            u32::from(self.is_stationary_fall())
        };
        let (spheres, scale) = self.moving_spheres();
        t.sphere_path.init_sphere(spheres, scale);
        t.sphere_path.init_path(Some(from.cell), Some(from), &to);
        t.sphere_path.set_check_pos(&from, Some(from.cell));
        let was_contact = self.contact;
        let was_walkable = self.on_walkable;
        world.begin_attempts(&mut t);
        let view = world
            .view(land, Some(key), self.has(flags::SYNCHRONIZE))
            .with_fuse(self.proximity_fuse());
        let ok = find_transitional_position(&world.ctx(&view), &mut t, PhysicsState::DEFAULT);
        world.apply(view.into_found());
        world.end_attempts(&t);
        if !ok {
            // A move that finds nowhere to go only turns the body.
            self.position.frame.rotation = to.frame.rotation;
            return;
        }
        self.take_transition(
            land,
            world,
            key,
            &t,
            (was_contact, was_walkable),
            Some(to.frame.rotation),
        );
    }

    /// What the transition knows of the body before it moves: following another exactly (else
    /// sliding along edges when it does), touching the ground (its contact plane, with whether it
    /// is water, the landscape or an interior's own) and on walkable ground, turning freely when
    /// its setup lets it (it takes its end facing before it moves), flying as a missile; and the
    /// plane it slid along.
    // D2-06046
    pub fn object_info(&self, t: &mut Transition) -> u32 {
        let mut state = 0;
        if !self.has(flags::SYNCHRONIZE) && self.does_edge_slide() {
            state |= ObjectInfoState::EDGE_SLIDE;
        }
        if self.contact {
            if let Some((plane, cell)) = self.contact_plane {
                t.init_contact_plane_on(
                    cell,
                    plane,
                    self.contact_is_water(),
                    self.transient & transient::LANDSCAPE_CONTACT != 0,
                    self.transient & transient::ENVIRONMENT_CONTACT != 0,
                );
            }
            state |= ObjectInfoState::CONTACT;
            if self.on_walkable {
                state |= ObjectInfoState::ON_WALKABLE;
            }
        }
        if let Some(n) = self.sliding_normal.filter(|_| self.has_sliding()) {
            t.init_sliding_normal(n);
        }
        if self.allows_free_heading() {
            state |= ObjectInfoState::FREE_ROTATE;
        }
        if self.missile.is_some() {
            state |= ObjectInfoState::PATH_CLIPPED;
        }
        state
    }

    /// Puts the body where a transition brought it: its place (turned to `rotation` when given),
    /// its contact and walkable ground (a missile that may not land never stands on ground), its
    /// sliding, then its collisions told of the move ([`Body::handle_all_collisions`]: the objects
    /// it hit, what it touches, its velocity off what it hit). `was` is whether it touched the
    /// ground and stood on walkable ground before the move.
    // D2-06256
    pub(crate) fn take_transition(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        t: &Transition,
        was: (bool, bool),
        rotation: Option<Quat>,
    ) {
        let r = rules();
        let ci = t.collision_info.clone();
        if t.sphere_path.curr_pos.cell != self.position.cell {
            self.update_cell_material(land, world, key, &t.sphere_path.curr_pos);
        }
        self.position = t.sphere_path.curr_pos;
        if let Some(q) = rotation {
            self.position.frame.rotation = q;
        }
        if !self.has(flags::SYNCHRONIZE) {
            self.set_contact_state(ci.contact_plane_valid);
            self.contact_plane = self
                .contact
                .then_some((ci.contact_plane, ci.contact_plane_cell_id));
            if ci.contact_plane_is_landscape {
                self.transient |= transient::LANDSCAPE_CONTACT;
            } else {
                self.transient &= !transient::LANDSCAPE_CONTACT;
            }
        }
        let may_land = self.missile.as_ref().is_none_or(|m| m.params.can_land);
        if may_land && self.contact {
            self.set_on_walkable(r.is_valid_walkable(ci.contact_plane.normal));
        } else {
            let walked = self.on_walkable;
            self.on_walkable = false;
            if walked && self.desired_element() != WATER {
                self.leave_ground();
            }
        }
        if ci.sliding_normal_valid {
            self.transient |= transient::SLIDING;
        } else {
            self.transient &= !transient::SLIDING;
        }
        self.sliding_normal = ci.sliding_normal_valid.then_some(ci.sliding_normal);
        self.handle_all_collisions(land, world, key, t, was.1);
    }
}

impl Owner for Body {
    fn local_velocity(&self) -> Vec3 {
        Body::local_velocity(self)
    }
    fn velocity(&self) -> Vec3 {
        self.velocity
    }
    fn omega(&self) -> Vec3 {
        Vec3::new(0.0, 0.0, self.turn_rate)
    }
    fn facing(&self) -> Vec3 {
        let r = self.heading.to_radians();
        Vec3::new(math::sinf(r), math::cosf(r), 0.0)
    }
    fn heading(&self) -> Heading {
        Heading::new(self.heading)
    }
    // D2-06429
    fn position(&self) -> Position {
        self.position
    }
    fn radius(&self) -> f32 {
        Body::radius(self)
    }
    fn height(&self) -> f32 {
        Body::height(self)
    }
    fn element(&self) -> u32 {
        Body::element(self)
    }
    fn desired_element(&self) -> u32 {
        Body::desired_element(self)
    }
    fn desired_mode_state(&self) -> ModeKey {
        Body::desired_mode_state(self)
    }
    fn effects_scale(&self) -> Vec3 {
        self.object_scale
    }
    fn height_above_walkable(&self) -> f32 {
        self.height_above
    }
    fn depth(&self) -> f32 {
        Body::depth(self)
    }
    fn has_water(&self) -> bool {
        Body::has_water(self)
    }
    // D2-06492
    fn is_sliding_down(&self) -> bool {
        Body::is_sliding_down(self)
    }
    fn is_player(&self) -> bool {
        self.is_player
    }
    // D2-06491
    fn has_contact(&self) -> bool {
        self.contact
    }
    fn is_doing_behavior(&self, behavior: u32) -> bool {
        self.behaviors.contains(&behavior)
    }
    fn do_autonomous_behavior(&mut self, behavior: u32) {
        self.behaviors.insert(behavior);
    }
    fn stop_behavior(&mut self, behavior: u32) {
        self.behaviors.remove(&behavior);
    }
    /// Unless its animation is frozen, the body's animation runs and the change is recorded for
    /// the host's mode tracker.
    // D2-06203
    fn do_mode(&mut self, element: u32, mode: u32) {
        if self.is_animation_frozen() {
            return;
        }
        self.set_animation_active(true);
        self.modes.push((element, mode));
    }
    fn do_fx(&mut self, fx: u32, intensity: f32) {
        self.effects.push((fx, intensity));
    }
    // D2-05945
    fn clamp_heading(&mut self, h: Heading) {
        self.heading_clamp = Some(h);
    }
    // D2-05946
    fn unclamp_heading(&mut self) {
        self.heading_clamp = None;
    }
    // D2-05947
    fn clamp_omega(&mut self, w: f32) {
        self.omega_clamp = Some(w);
    }
    // D2-05948
    fn unclamp_omega(&mut self) {
        self.omega_clamp = None;
    }
    fn target(&self, id: u64) -> Option<TargetBody> {
        self.targets.get(&id).copied()
    }
    fn set_target(&mut self, _id: u64, _radius: f32, _quantum: f32) {}
    fn clear_target(&mut self) {}
    fn move_to_done(&mut self, result: MovementReturn) {
        self.moves_done.push(result);
    }
    fn snap_heading(&mut self, h: Heading) {
        self.heading = h.degrees();
        self.position.frame.rotation = heading_rotation(self.heading);
        self.turn_rate = 0.0;
    }
    // D2-05950
    fn release_rails(&mut self) {
        self.distance_cap = -1.0;
        self.rail = None;
    }
    fn set_distance_cap(&mut self, cap: f32) {
        self.distance_cap = cap;
    }
    // D2-05949
    fn set_velocity_rail(&mut self, dir: Vec3) {
        self.rail = Some(dir);
    }
    fn set_synchronize_transition(&mut self, on: bool) {
        Body::set_synchronize_transition(self, on);
    }
    fn set_contact(&mut self, on: bool) {
        self.set_contact_state(on);
    }
    // D2-06144
    fn set_velocity(&mut self, v: Vec3) {
        self.velocity = v;
        self.set_movement_active(true);
    }
    // D2-06123
    fn apply_impulse(&mut self, v: Vec3) -> bool {
        self.contact = false;
        self.do_element(AIR);
        self.velocity = v;
        true
    }
    fn blip_to(&mut self, p: Position) {
        self.position = p;
        self.heading = geom::frame_heading(&p.frame).degrees();
    }
    fn jump_logic(&mut self) -> bool {
        Body::jump_logic(self)
    }
}

/// The spheres of a body's setup, for collision: at most two.
#[must_use]
pub fn setup_spheres(setup: &auberean_assets::setup::Setup) -> Vec<Sphere> {
    setup
        .spheres
        .iter()
        .take(2)
        .map(|s| Sphere::new(Vec3::new(s.center.x, s.center.y, s.center.z), s.radius))
        .collect()
}

/// A tick's worth of frame time, for callers stepping by hand.
#[must_use]
pub fn tick_seconds() -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    {
        TICK as f32
    }
}

impl Body {
    /// The frame the body is drawn at.
    // D2-06285
    #[must_use]
    pub fn frame(&self) -> Frame {
        self.position.frame
    }

    /// Put the body somewhere at once, with nothing to draw it moving from.
    pub fn place(&mut self, position: Position, heading: f32) {
        self.position = position;
        self.heading = heading;
        self.previous = (position, heading);
    }

    /// The player's mouse turns the body at once, `degrees` to the right: it is drawn turned this
    /// frame, whatever the share of its tick, so it keeps its place before the camera turning with
    /// it. (Drawn between its last two ticks' headings, a turn made between ticks would come and go
    /// behind the camera as the tick runs.)
    pub fn turn_at_once(&mut self, degrees: f32) {
        self.heading = (self.heading + degrees).rem_euclid(360.0);
        self.previous.1 = (self.previous.1 + degrees).rem_euclid(360.0);
    }

    /// Where the body is drawn `alpha` of a tick past its last tick (from [`Ticker::alpha`]):
    /// between where it was before that tick and where it is, the origin along the line between
    /// them (across a block's edge, in the block it is in now), the heading turned the short way
    /// and the frame's whole rotation with it; in the cell it is in now. A body whose movement has
    /// stopped is drawn where it is.
    // D2-05830, D2-05829, D2-06219
    #[must_use]
    pub fn drawn(&self, alpha: f32) -> (Position, f32) {
        if !self.is_movement_active() {
            return (self.position, self.heading);
        }
        let (before, was) = self.previous;
        let now = self.position;
        let (b0, b1) = (before.cell.landblock(), now.cell.landblock());
        #[allow(clippy::cast_precision_loss)]
        let off = Vec3::new(
            (i32::from(b0.x()) - i32::from(b1.x())) as f32 * crate::rules::BLOCK_LENGTH,
            (i32::from(b0.y()) - i32::from(b1.y())) as f32 * crate::rules::BLOCK_LENGTH,
            0.0,
        );
        let from = before.frame.origin.add(off);
        let to = now.frame.origin;
        let origin = from.add(to.sub(from).mul(alpha));
        let turn = (self.heading - was + 540.0).rem_euclid(360.0) - 180.0;
        let heading = (was + turn * alpha).rem_euclid(360.0);
        let mut at = now;
        at.frame.origin = origin;
        // A missile flying along its path keeps its pitch.
        crate::geom::slerp_rotation(&mut at.frame, &before.frame, &now.frame, alpha);
        (at, heading)
    }
}
