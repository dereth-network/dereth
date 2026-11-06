//! A body flying as a missile: its guidance, made the first time it is asked for (which wakes
//! the body's movement), the guidance's calls made through the body (setting it up, placing and
//! launching it, aiming it again, asking its direction and velocity), and what the guidance asks
//! of the body and of the world ([`Flight`]): the other objects ([`Objects`]), the dice and the
//! wind come from the host; what the missile does to itself (its flight effect and streak, fading
//! out, sticking to what it struck) and what it tells its game object go out as [`Notice`]s.
//!
//! A failure of the guidance (anything but none or "nothing heard of the target yet") is told to
//! the body's game object; asking a body with no guidance to place or launch tells it 5.

use auberean_primitives::random::Random;
use dereth_primitives::{Position, Vec3};

use super::{Body, Notice, Wind};
use crate::collision::CollisionReport;
use crate::geom::{self, Heading};
use crate::land::Land;
use crate::missile::{fail, Guidance, GuidanceHost};
use crate::missile::{MissileParameters, MissileTargetingParameters};
use crate::place::SetPosition;
use crate::target::Aim;
use crate::world::World;

/// The failure told to a body's game object asked to place or launch a missile it is not.
pub const NOT_A_MISSILE: u32 = 5;

/// Another object of the world, as a missile sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub position: Position,
    pub heading: Heading,
    pub velocity: Vec3,
    /// Its body is not static.
    pub dynamic: bool,
    /// It is itself a guided missile.
    pub guided: bool,
    /// Its game object is a player or a creature.
    pub player_or_creature: bool,
}

/// The rest of the world as one body asks after it: the other objects by id, where on one of them
/// a place is, the dice, and the wind.
pub trait Objects {
    fn object(&self, id: u64) -> Option<Seen>;
    /// A place on object `id`: `height` of the way up it (its height scaled), `offset` from there
    /// (by its scale when `scaled`), turned with it (see [`Body::object_offset_position`]); none
    /// when it cannot be had.
    fn offset_position(&self, id: u64, height: f32, scaled: bool, offset: Vec3)
        -> Option<Position>;
    /// True with the given chance.
    fn roll(&mut self, chance: f32) -> bool;
    /// The world's wind.
    fn wind(&self) -> Vec3;
}

/// A world with no other objects, no wind, and dice that never come up.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoObjects;

impl Objects for NoObjects {
    fn object(&self, _id: u64) -> Option<Seen> {
        None
    }
    fn offset_position(
        &self,
        _id: u64,
        _height: f32,
        _scaled: bool,
        _offset: Vec3,
    ) -> Option<Position> {
        None
    }
    fn roll(&mut self, _chance: f32) -> bool {
        false
    }
    fn wind(&self) -> Vec3 {
        Vec3::ZERO
    }
}

/// The other objects as a host last saw them: each one's place, heading, velocity and kind, with
/// its height and scale for the places on it; the world's wind; and the client's random numbers.
/// A place on an object is not kept from going through walls here (the line toward it meets
/// nothing).
#[derive(Clone, Debug, Default)]
pub struct ObjectTable {
    seen: std::collections::BTreeMap<u64, (Seen, f32, Vec3)>,
    pub wind: Wind,
    /// The client's one generator (a table of its own by default).
    pub dice: Random,
}

impl ObjectTable {
    /// Forgets every object.
    pub fn clear(&mut self) {
        self.seen.clear();
    }

    /// Notes body `body` as object `id`, a player or creature or not.
    pub fn note(&mut self, id: u64, body: &Body, player_or_creature: bool) {
        let s = body.config.scale;
        self.seen.insert(
            id,
            (
                Seen {
                    position: body.position,
                    heading: Heading::new(body.heading),
                    velocity: body.velocity,
                    dynamic: !body.is_static(),
                    guided: body.missile.is_some(),
                    player_or_creature,
                },
                body.height(),
                Vec3::new(s, s, s),
            ),
        );
    }

    /// Notes body `body` as object `id`, one that stands where it is put at scale `scale` rather
    /// than moving as a body, a player or creature or not. Standing still does not make it static:
    /// that is its state's to say.
    pub fn note_still(&mut self, id: u64, body: &Body, scale: Vec3, player_or_creature: bool) {
        let at = body.position;
        self.seen.insert(
            id,
            (
                Seen {
                    position: at,
                    heading: geom::frame_heading(&at.frame),
                    velocity: Vec3::ZERO,
                    dynamic: !body.is_static(),
                    guided: body.missile.is_some(),
                    player_or_creature,
                },
                body.height(),
                scale,
            ),
        );
    }
}

impl Objects for ObjectTable {
    fn object(&self, id: u64) -> Option<Seen> {
        self.seen.get(&id).map(|s| s.0)
    }
    fn offset_position(
        &self,
        id: u64,
        height: f32,
        scaled: bool,
        offset: Vec3,
    ) -> Option<Position> {
        let (seen, tall, scale) = self.seen.get(&id)?;
        let mut clear = |line: &mut crate::ethereal::SightLine| {
            line.trace_result = 0;
            line.final_cell = line.initial_cell;
            line.final_ray = line.initial_ray;
        };
        Some(
            super::offset_position(
                &seen.position,
                *tall,
                *scale,
                height,
                scaled,
                offset,
                &mut clear,
            )
            .0,
        )
    }
    fn roll(&mut self, chance: f32) -> bool {
        self.dice.check(chance)
    }
    fn wind(&self) -> Vec3 {
        self.wind.0
    }
}

/// A body's guidance at work: the body, the world it is placed in, and the other objects.
pub(crate) struct Flight<'a> {
    body: &'a mut Body,
    land: &'a Land,
    world: &'a mut World,
    objects: &'a mut dyn Objects,
}

impl GuidanceHost for Flight<'_> {
    fn now(&self) -> f64 {
        self.body.clock
    }
    fn position(&self) -> Position {
        self.body.position
    }
    fn drawing_position(&self) -> Position {
        self.body.position
    }
    fn has_parent(&self) -> bool {
        self.body.has_parent
    }
    fn is_in_world(&self) -> bool {
        self.body.in_world
    }
    fn is_valid(&self, p: &Position) -> bool {
        geom::position_is_valid(p)
    }
    fn gravity(&self) -> Vec3 {
        self.body.current_gravity()
    }
    fn wind(&self) -> Vec3 {
        self.objects.wind()
    }
    fn height_above_walkable(&self) -> Option<f32> {
        Some(self.body.height_above_walkable())
    }
    fn set_missile_velocity(&mut self, v: Vec3) {
        self.body.set_missile_velocity(v, false);
    }
    fn align_to_path(&mut self, on: bool) {
        self.body.align_to_path(on);
    }
    fn report_out_of_fuel(&mut self) {
        self.body.missile_report_out_of_fuel();
    }
    fn report_expired(&mut self) {
        self.body.missile_report_expired();
    }
    fn init_end(&mut self, at: &Position) -> bool {
        self.body.missile_init_end(self.land, self.world, at)
    }
    fn target_id(&self) -> u64 {
        self.body.target_id()
    }
    fn set_target(&mut self, context: u32, object: u64, radius: f32, quantum: f32) {
        self.body.set_target(context, object, radius, quantum);
    }
    fn start_streak(&mut self) {
        self.body.notices.push(Notice::StreakOn);
    }
    fn stop_streak(&mut self) {
        self.body.notices.push(Notice::StreakOff);
    }
    fn do_fx(&mut self, fx: u32, intensity: f32) {
        crate::motion::Owner::do_fx(&mut *self.body, fx, intensity);
    }
    fn stop_fx(&mut self, fx: u32) {
        self.body.notices.push(Notice::StopFx(fx));
    }
    fn delete_self(&mut self, fade: f64, delay: f64) {
        self.body.delete_self(fade, delay);
    }
    fn stick_to(&mut self, object: u64) {
        self.body.notices.push(Notice::StickTo(object));
    }
    fn random_check(&mut self, chance: f32) -> bool {
        self.objects.roll(chance)
    }
    fn object_exists(&self, object: u64) -> bool {
        self.objects.object(object).is_some()
    }
    fn object_position(&self, object: u64) -> Option<Position> {
        self.objects.object(object).map(|o| o.position)
    }
    fn object_heading(&self, object: u64) -> Option<Heading> {
        self.objects.object(object).map(|o| o.heading)
    }
    fn object_offset_position(
        &self,
        object: u64,
        height: f32,
        scaled: bool,
        offset: Vec3,
    ) -> Option<Position> {
        self.objects.offset_position(object, height, scaled, offset)
    }
    fn object_velocity(&self, object: u64) -> Vec3 {
        self.objects
            .object(object)
            .map_or(Vec3::ZERO, |o| o.velocity)
    }
    fn object_is_dynamic(&self, object: u64) -> bool {
        self.objects.object(object).is_some_and(|o| o.dynamic)
    }
    fn object_has_guidance(&self, object: u64) -> bool {
        self.objects.object(object).is_some_and(|o| o.guided)
    }
    fn object_is_player_or_creature(&self, object: u64) -> bool {
        self.objects
            .object(object)
            .is_some_and(|o| o.player_or_creature)
    }
}

/// Runs `f` on the body's guidance (made first if it has none) with the body as its host.
fn with_guidance<R>(
    body: &mut Body,
    land: &Land,
    world: &mut World,
    objects: &mut dyn Objects,
    f: impl FnOnce(&mut Guidance, &mut Flight<'_>) -> R,
) -> R {
    let mut g = body.take_guidance();
    let r = f(
        &mut g,
        &mut Flight {
            body: &mut *body,
            land,
            world,
            objects,
        },
    );
    body.missile = Some(g);
    r
}

impl Body {
    /// The body's guidance, taken out to run; made when it has none, which wakes its movement.
    fn take_guidance(&mut self) -> Box<Guidance> {
        if let Some(g) = self.missile.take() {
            return g;
        }
        self.set_movement_active(true);
        Box::default()
    }

    /// A failure of the guidance, told to the game object unless it is none or "nothing heard of
    /// the target yet".
    fn missile_status(&mut self, status: u32) {
        if status != fail::NONE && status != fail::TARGET_UNHEARD {
            self.notices.push(Notice::MissileError(status));
        }
    }

    /// Sets the missile up: its flight settings and aim, then looks for its target.
    // D2-06240
    pub fn missile_initialize(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        p: &MissileParameters,
        t: &MissileTargetingParameters,
    ) -> bool {
        let status = with_guidance(self, land, world, objects, |g, h| g.initialize(h, p, t));
        self.missile_status(status);
        status == fail::NONE
    }

    /// New flight settings; its lifespan and fuel start again.
    pub fn missile_init_missile_parameters(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        p: &MissileParameters,
    ) -> bool {
        let status = with_guidance(self, land, world, objects, |g, h| {
            g.client_init_missile_parameters(&*h, p)
        });
        self.missile_status(status);
        status == fail::NONE
    }

    /// New aim; the target is looked for again.
    pub fn missile_init_targeting_parameters(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        t: &MissileTargetingParameters,
    ) -> bool {
        let status = with_guidance(self, land, world, objects, |g, h| {
            g.client_init_targeting_parameters(h, t)
        });
        self.missile_status(status);
        status == fail::NONE
    }

    /// Sets the missile up, then places and launches it when due; true when both took.
    pub fn missile_init_place_and_fire(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        p: &MissileParameters,
        t: &MissileTargetingParameters,
    ) -> bool {
        let a = self.missile_initialize(land, world, objects, p, t);
        let b = self.missile_place_and_fire();
        a & b
    }

    /// The missile is placed and launched at its next step; a body that is no missile tells its
    /// game object instead.
    // D2-06205
    pub fn missile_place_and_fire(&mut self) -> bool {
        self.missile_order(Guidance::place_and_fire)
    }

    pub fn missile_place(&mut self) -> bool {
        self.missile_order(Guidance::place)
    }

    pub fn missile_fire(&mut self) -> bool {
        self.missile_order(Guidance::fire)
    }

    fn missile_order(&mut self, f: fn(&mut Guidance)) -> bool {
        if let Some(g) = self.missile.as_deref_mut() {
            f(g);
            return true;
        }
        self.notices.push(Notice::MissileError(NOT_A_MISSILE));
        false
    }

    /// The direction the missile would launch in and how fast; a failure (or no guidance, 5) is
    /// told to the game object.
    pub fn missile_inquire_direction(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
    ) -> Option<(Heading, f32)> {
        if self.missile.is_none() {
            self.notices.push(Notice::MissileError(NOT_A_MISSILE));
            return None;
        }
        let (h, speed, status) =
            with_guidance(self, land, world, objects, |g, f| g.inquire_direction(f));
        if status != fail::NONE {
            self.notices.push(Notice::MissileError(status));
            return None;
        }
        Some((h, speed))
    }

    /// The velocity the missile flies at, when it is one.
    #[must_use]
    pub fn missile_inquire_current_velocity(&self) -> Option<Vec3> {
        self.missile.as_deref().map(|g| g.velocity)
    }

    /// New settings and aim in flight; a failure (or no guidance, 5) is told to the game object.
    pub fn missile_retarget(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        p: &MissileParameters,
        t: &MissileTargetingParameters,
    ) -> bool {
        if self.missile.is_none() {
            self.notices.push(Notice::MissileError(NOT_A_MISSILE));
            return false;
        }
        let status = with_guidance(self, land, world, objects, |g, h| g.retarget(h, p, t));
        if status != fail::NONE {
            self.notices.push(Notice::MissileError(status));
            return false;
        }
        true
    }

    /// The server launched the missile: it flies at `v` toward where its target is (the guidance
    /// made first when it has none).
    // D2-06238
    pub fn receive_initialize_missile(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        v: Vec3,
        target: &Aim,
    ) -> bool {
        with_guidance(self, land, world, objects, |g, h| {
            g.client_activate(h, v, target);
        });
        true
    }

    /// What a description says of a body flying as a missile (the guidance made first when it has
    /// none, which wakes its movement): where it aims, whether it is lit and in flight, the object
    /// it aims at when there is one, and the velocity it flies at.
    #[allow(clippy::too_many_arguments)]
    pub fn take_missile_description(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        target_position: Position,
        activated: bool,
        moving: bool,
        target: Option<(u64, bool, Vec3)>,
        velocity: Vec3,
    ) {
        with_guidance(self, land, world, objects, |g, h| {
            g.targeting.set_target_position(target_position);
            g.set_activated(activated);
            g.set_moving(h, moving);
            if let Some((object, scaled, offset)) = target {
                g.targeting.set_target_object(object, 0.0, scaled, offset);
            }
        });
        self.set_missile_velocity(velocity, false);
    }

    /// The server aimed the missile again: new settings and where its target is.
    pub fn receive_retarget_missile(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        p: &MissileParameters,
        target: &Aim,
    ) -> bool {
        with_guidance(self, land, world, objects, |g, h| {
            g.client_retarget(h, p, target)
        })
    }

    /// The missile is put in the world at `at`: let go of its holder (which takes it off its
    /// list), then entered there.
    // D2-05913
    pub fn missile_init_end(&mut self, land: &Land, world: &mut World, at: &Position) -> bool {
        self.unset_parent();
        self.has_parent = false;
        self.enter_world_at(land, world, at, false)
    }

    // D2-05914
    pub fn missile_report_out_of_fuel(&mut self) {
        self.notices.push(Notice::MissileOutOfFuel);
    }

    // D2-05915
    pub fn missile_report_expired(&mut self) {
        self.notices.push(Notice::MissileExpired);
    }

    /// Enters the world at `at` (keeping it as the placement), when it is a place in the world and
    /// the body is neither in the world nor held; `slide`: a free spot nearby will do.
    // D2-06053, D2-06365
    pub fn enter_world_at(
        &mut self,
        land: &Land,
        world: &mut World,
        at: &Position,
        slide: bool,
    ) -> bool {
        if !geom::position_is_valid(at) || self.in_world || self.has_parent {
            return false;
        }
        let mut s = SetPosition::default();
        s.set_position(*at);
        s.add_flags(if slide {
            crate::place::flags::PLACEMENT | crate::place::flags::SLIDE
        } else {
            crate::place::flags::PLACEMENT
        });
        if self.set_position(land, world, &s, &mut || 0.0) != super::placing::error::OK {
            return false;
        }
        self.handle_enter_world();
        true
    }

    /// Fades the body out over `fade` seconds after `delay`, and removes it (the host's to do).
    // D2-06097
    pub fn delete_self(&mut self, fade: f64, delay: f64) {
        self.notices.push(Notice::FadeOut { fade, delay });
    }

    /// The missile's guidance hears of a collision the body reported; what it went off on is kept
    /// ([`Body::struck`]).
    pub(crate) fn missile_collision(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        report: &CollisionReport,
    ) {
        if self.missile.is_none() {
            return;
        }
        let hit = with_guidance(self, land, world, objects, |g, h| {
            g.handle_collision(h, report)
        });
        if let Some(hit) = hit {
            self.struck = Some(hit.unwrap_or(crate::target::NO_OBJECT));
        }
    }

    /// The missile's guidance steps; its failure is told to the game object.
    pub(crate) fn missile_update(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
    ) {
        if self.missile.is_none() {
            return;
        }
        let status = with_guidance(self, land, world, objects, |g, h| g.update(h));
        self.missile_status(status);
    }

    /// Where the missile moves this tick, from its velocity: the step's offset.
    pub(crate) fn missile_step(
        &mut self,
        land: &Land,
        world: &mut World,
        objects: &mut dyn Objects,
        dt: f32,
    ) -> Vec3 {
        let v = self.velocity;
        let mut frame =
            dereth_primitives::Frame::new(Vec3::ZERO, dereth_primitives::Quat::IDENTITY);
        with_guidance(self, land, world, objects, |g, h| {
            g.update_missile_physics(h, v, dt, &mut frame);
        });
        frame.origin
    }

    /// The object a flying missile goes off near, and how near: its target, when it goes off by
    /// proximity and the chance its aim drew is within its own.
    #[must_use]
    pub fn proximity_fuse(&self) -> Option<(u64, f32)> {
        let g = self.missile.as_deref()?;
        let target = g.targeting.target;
        (target != crate::target::NO_OBJECT && g.detonates_on_proximity_to(target))
            .then_some((target, g.params.detonation_proximity))
    }
}
