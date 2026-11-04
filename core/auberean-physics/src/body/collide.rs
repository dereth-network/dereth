//! What a body's collisions are told after it moves, and the body's other updates.
//!
//! After every move (and after a placement that found no room) the body's collisions are told of
//! it: the physics types it passes through start again from those it would; each object it hit
//! is touched by it and, when that object moves, touches it back (both woken); the collision
//! normal is kept or cleared; what it touches is noted (the plane, whether in water, whether on
//! the landscape: ground or an interior's floors and walls, not an object); and then its velocity
//! comes off what it hit, by 1.005 of its speed into the normal (all of it for an inelastic body),
//! unless it stood on walkable ground both before and after. Its stationary state follows the
//! transition's count of steps it could not fall or move: 0 or 3 clear it, 1 falling, 2 stopped,
//! more stuck; from 2 its velocity is gone.
//!
//! A body's other updates run each tick while woken: its detection spheres are checked and what
//! began and ended of its collisions is reported (to the host, and to its guidance when it flies
//! as a missile). They stop once nothing touches it and none of its detections runs.

use dereth_physics::transition::Transition;
use dereth_physics::V3;
use dereth_primitives::shape::Plane;
use dereth_primitives::Vec3;

use super::missile::Objects;
use super::Body;
use crate::collision::manager::BodyKey;
use crate::detect::manager::DetectionHost;
use crate::land::Land;
use crate::target::TargetInfo;
use crate::world::{GameObjects, World};

/// What a body asks of its game object, of its host, or of another object, in the order it
/// asked. The host takes them from [`Body::notices`]; [`Body::deliver`] hands those addressed to
/// another body to it.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// Its guidance failed (the game object is told the failure).
    MissileError(u32),
    MissileOutOfFuel,
    MissileExpired,
    /// Its missile's streak starts or stops.
    StreakOn,
    StreakOff,
    /// An effect it plays stops.
    StopFx(u32),
    /// Fade it out over `fade` seconds after `delay` and remove it.
    FadeOut {
        fade: f64,
        delay: f64,
    },
    /// Hold it on the object it struck.
    StickTo(u64),
    /// What it stands on has a new terrain type: its effects' variants for that.
    TerrainType(u32),
    /// It entered or left the world (its game object and the bookkeeping are told).
    EnteredWorld,
    LeftWorld,
    /// A request for its animation.
    Animate(super::Animate),
    /// Object `target` is to tell `watcher` where it is whenever it drifts past `radius` from
    /// what it told, projecting `quantum` seconds ahead.
    Watch {
        target: u64,
        watcher: u64,
        radius: f32,
        quantum: f64,
    },
    /// Object `target` stops telling `watcher`.
    Unwatch {
        target: u64,
        watcher: u64,
    },
    /// Object `watcher` hears where its target is.
    Whereabouts {
        watcher: u64,
        info: TargetInfo,
    },
    /// Holder `parent` lets go of `child`.
    RemoveChild {
        parent: u64,
        child: u64,
    },
}

impl Notice {
    /// The other object a notice is for (its target asked to be watched or to stop, or the
    /// watcher told where its target is); none for what goes to the body's own game object, host
    /// or animation.
    #[must_use]
    pub const fn addressee(&self) -> Option<u64> {
        match *self {
            Self::Watch { target, .. } | Self::Unwatch { target, .. } => Some(target),
            Self::Whereabouts { watcher, .. } => Some(watcher),
            Self::RemoveChild { parent, .. } => Some(parent),
            _ => None,
        }
    }
}

/// The transition's contact plane is the landscape's ground or an interior's own floors and
/// walls, not an object's.
fn on_landscape(t: &Transition) -> bool {
    let ci = &t.collision_info;
    ci.contact_plane_is_landscape || ci.contact_plane_is_environment
}

impl Body {
    /// Tells the body's collisions of a move: see the module. `was_walkable`: it stood on walkable
    /// ground before the move. True when it began touching an object.
    // D2-06183
    pub(crate) fn handle_all_collisions(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        t: &Transition,
        was_walkable: bool,
    ) -> bool {
        if !world.begin_collision_check(key) {
            return false;
        }
        let bounce = !(was_walkable && self.on_walkable);
        let ci = &t.collision_info;
        let hit: Vec<_> = ci.collide_object.iter().map(|(h, _)| *h).collect();
        let normal = ci.collision_normal_valid.then_some(ci.collision_normal);
        if !self.is_static() && (!hit.is_empty() || normal.is_some()) {
            self.set_misc_active(true);
        }
        let touched =
            world.handle_collisions(key, &hit, normal, ci.last_collided_object, self.velocity);
        if ci.contact_plane_valid {
            let water = land.water_plane(self.position.cell);
            self.water = water;
            self.has_water = water.is_some();
            let o = self.position.frame.origin;
            let under = water.is_some_and(|p| 0.0 < -(p.normal.dot(o) + p.d));
            self.set_contact_info(land, world, key, ci.contact_plane, under, on_landscape(t));
        } else {
            world.set_no_contact(key);
        }
        world.end_collision_check(key);
        let frames = ci.frames_stationary_fall;
        if frames < 2 {
            if bounce && ci.collision_normal_valid {
                if self.is_inelastic() {
                    self.velocity = Vec3::ZERO;
                } else {
                    let n = ci.collision_normal;
                    let k = -(n.dot(self.velocity) * 1.005);
                    self.velocity = self.velocity.add(n.mul(k));
                }
            }
        } else {
            self.velocity = Vec3::ZERO;
        }
        if t.kill_velocity {
            self.velocity = Vec3::ZERO;
        }
        self.stationary = frames;
        match frames {
            0 | 3 => self.reset_stationary_stuck_fall(),
            1 => self.activate_stationary_fall(),
            2 => self.activate_stationary_stop(),
            _ => self.activate_stationary_stuck(),
        }
        touched
    }

    /// The body touches `plane`, under water or not, on the landscape or not: told to its
    /// collisions (a static body has none to tell), which wakes its other updates.
    // D2-05960
    pub fn set_contact_info(
        &mut self,
        land: &Land,
        world: &mut World,
        key: BodyKey,
        plane: Plane,
        in_water: bool,
        landscape: bool,
    ) -> bool {
        if self.is_static() || world.collisions(key).is_none() {
            return false;
        }
        world.set_contact(
            key,
            plane,
            in_water.then(|| land.water_material(self.position.cell)),
            landscape.then(|| land.ground_material(self.position.cell, self.position.frame.origin)),
        );
        self.set_misc_active(true);
        true
    }

    /// Reports what began and ended of the body's collisions (`end_all`: every touch ends): kept
    /// for the host, and heard by its guidance when it flies as a missile.
    // D2-06244, D2-06215
    pub fn report_collisions(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        end_all: bool,
    ) {
        let Some(key) = self.key else {
            return;
        };
        if let Some(report) = world.report_collisions(key, end_all) {
            self.missile_collision(land, world, objects, &report);
        }
    }

    /// Ends every touch of the body as it leaves: reported as [`Body::report_collisions`] does.
    // D2-06245
    pub fn end_all_collisions(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
    ) {
        let Some(key) = self.key else {
            return;
        };
        if let Some(report) = world.end_all_collisions(key) {
            self.missile_collision(land, world, objects, &report);
        }
    }

    /// The body's other updates: its detection spheres checked (when a host for them is given),
    /// then its collisions reported.
    pub fn update_misc(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        detections: Option<&mut dyn DetectionHost>,
    ) {
        if let (Some(d), Some(host)) = (self.detections.as_mut(), detections) {
            d.check_detection(false, host);
        }
        self.report_collisions(land, world, objects, false);
    }

    /// Wakes the body's other updates when something it touches woke them.
    pub fn take_wake(&mut self, world: &mut World) {
        if self.key.is_some_and(|k| world.take_woken(k)) {
            self.set_misc_active(true);
        }
    }

    /// One tick of the body as the physics update runs it: while its movement runs, it moves,
    /// and stops moving once it may (the player's body never stops); while its other updates run
    /// (woken by anything that touched it), they run, and stop once they may.
    // D2-05767
    pub fn update(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        detections: Option<&mut dyn DetectionHost>,
        dt: f32,
    ) {
        if let Some(key) = self.key {
            self.tell_world(world, key);
        }
        if self.is_movement_active() {
            self.tick_with(land, world, objects, dt);
            if self.movement_can_be_inactive() && !self.is_player {
                self.set_movement_active(false);
            }
        }
        self.take_wake(world);
        if self.is_misc_active() {
            self.update_misc(land, world, objects, detections);
            if self.misc_can_be_inactive(world) {
                self.set_misc_active(false);
            }
        }
    }

    /// One tick as [`Body::update`], its detection spheres checked against the world it moves in
    /// ([`World::detection`], with `game` answering for the game objects).
    // D2-06253
    pub fn update_in(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        game: &dyn GameObjects,
        dt: f32,
    ) {
        if self.is_movement_active() {
            self.tick_with(land, world, objects, dt);
            if self.movement_can_be_inactive() && !self.is_player {
                self.set_movement_active(false);
            }
        }
        self.take_wake(world);
        if self.is_misc_active() {
            if let (Some(d), Some(key)) = (self.detections.as_mut(), self.key) {
                d.check_detection(false, &mut world.detection(land, key, game));
            }
            self.report_collisions(land, world, objects, false);
            if self.misc_can_be_inactive(world) {
                self.set_misc_active(false);
            }
        }
    }
}
