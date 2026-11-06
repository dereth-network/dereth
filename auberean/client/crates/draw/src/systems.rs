//! The particle systems every object sets going, kept together and drawn together once a frame.
//!
//! An object asks for a system by its record. A system whose material is not lit, that does not
//! live in the world and that its first owner allowed to be shared serves every later owner asking
//! for the same record: the objects placed with the world allow it, so the torches of a hall burn
//! with one set of flames, drawn at each torch. Each frame an owner queues its systems where it
//! stands; a system thinks once a frame, at the first place it is queued, and draws at every
//! place it was queued. A system its last owner lets go of discharges nothing more; it goes at once
//! when it has no particles left or may be shared, and otherwise keeps drawing where it was last
//! drawn (or where it lives, in the world) until its particles are gone.

use std::sync::{Arc, Mutex, PoisonError, Weak};

use auberean_primitives::random::Random;
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::inside::ParticleLight;
use auberean_world::math::Mat4;
use auberean_world::records::WorldRecords;

use crate::particles::{ParticleSet, Push};
use crate::scene::ObjectBatch;

/// Where a system is drawn: the block whose frame `place` is in, its place (origin and turn) and
/// the object's scale.
#[derive(Clone, Copy, Debug, PartialEq)]
struct At {
    home: (i32, i32),
    place: Mat4,
    scale: Vec3,
}

#[derive(Debug)]
struct System {
    id: u32,
    record: u32,
    set: ParticleSet,
    refs: u32,
    can_be_shared: bool,
    /// The frame (the client's running time) it was last queued in, and where.
    queued_at: Option<f64>,
    places: Vec<At>,
}

impl System {
    /// Whether another owner may take this system: its material's first layer is not lit, it was
    /// allowed to be shared, and its particles do not live in the world.
    // D2-10276
    fn can_be_shared(&self) -> bool {
        self.set.has_material()
            && !self.set.lit()
            && self.can_be_shared
            && !self.set.desc().world_space
    }
}

#[derive(Debug, Default)]
struct Inner {
    systems: Vec<System>,
    /// Systems no object holds, drawing where they were left until they are spent.
    autonomous: Vec<(ParticleSet, At)>,
    next_id: u32,
    /// The repulsors objects carry, by id.
    repulsors: Vec<(u32, Repulsor)>,
    next_repulsor: u32,
    /// Frames drawn so far, and the blocks' length the last was drawn with.
    frames: u64,
    block_length: f32,
    /// The client's random numbers, which every system rolls.
    dice: Random,
    /// The light source effects preference is off: the effects place no lights.
    no_lights: bool,
    /// The streak effects preference is off: the effects make no streaks.
    no_streaks: bool,
    /// What the player's preferences ask of the effects' entries.
    channels: crate::effects::Channels,
    /// The decal effects preference is off.
    no_decals: bool,
    /// The volumetric fog effects preference is off: the systems whose particles move by
    /// repulsors (the ground's mists) neither think nor draw.
    no_fog: bool,
}

/// A push (or pull) an object carries on the particles about it: how hard it pushes at its
/// center, how far it reaches, how long it lasts (less than 0.0002 s: for as long as its object
/// keeps it), the speed its object must move at for it to push (0.0002 or less: any), and whether
/// it pulls instead. Where its object stands, how fast it moves, and the frame it was last placed
/// in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Repulsor {
    pub acceleration: f32,
    pub range: f32,
    pub lifetime: f32,
    pub min_velocity: f32,
    pub attracts: bool,
    pub created: f64,
    pub home: (i32, i32),
    pub origin: Vec3,
    pub speed: f32,
    pub frame: u64,
}

impl Repulsor {
    /// A repulsor made at `now`, standing nowhere yet.
    // D2-10311
    #[must_use]
    pub fn new(
        acceleration: f32,
        range: f32,
        lifetime: f32,
        min_velocity: f32,
        attracts: bool,
        now: f64,
    ) -> Self {
        Self {
            acceleration,
            range,
            lifetime,
            min_velocity,
            attracts,
            created: now,
            home: (0, 0),
            origin: Vec3::ZERO,
            speed: 0.0,
            frame: 0,
        }
    }

    /// Whether it has outlived a lifetime of its own.
    // D2-10312
    #[must_use]
    pub fn ready_to_die(&self, now: f64) -> bool {
        self.lifetime >= 0.0002 && f64::from(self.lifetime) < now - self.created
    }

    /// Whether it pushes: its object fast enough (or no speed asked), and within its lifetime (or
    /// none).
    // D2-10313
    #[must_use]
    pub fn active(&self, now: f64) -> bool {
        (self.min_velocity <= 0.0002 || self.min_velocity <= self.speed)
            && (self.lifetime < 0.0002 || now - self.created < f64::from(self.lifetime))
    }
}

impl Inner {
    fn find(&mut self, id: u32) -> Option<&mut System> {
        self.systems.iter_mut().find(|s| s.id == id)
    }

    /// The repulsors pushing a system in block `home` that thinks at `now`: those placed this
    /// frame or the last that push now, each where it stands in `home`'s frame (one in another
    /// block only once the blocks' length is known).
    // D2-10261
    fn pushes(&self, home: (i32, i32), now: f64) -> Vec<Push> {
        self.repulsors
            .iter()
            .map(|(_, r)| r)
            .filter(|r| self.frames.saturating_sub(r.frame) < 2 && r.active(now))
            .filter(|r| r.home == home || self.block_length > 0.0)
            .map(|r| {
                #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
                let shift = Vec3::new(
                    (r.home.0 - home.0) as f32 * self.block_length,
                    (r.home.1 - home.1) as f32 * self.block_length,
                    0.0,
                );
                Push {
                    origin: Vec3::new(r.origin.x + shift.x, r.origin.y + shift.y, r.origin.z),
                    range: r.range,
                    acceleration: r.acceleration,
                    attracts: r.attracts,
                }
            })
            .collect()
    }

    /// One more owner.
    // D2-10269
    fn add_ref(&mut self, id: u32) {
        if let Some(s) = self.find(id) {
            s.refs += 1;
        }
    }

    /// One owner fewer; the last lets the system go: at once when it is spent or may be shared,
    /// else left to finish where it lives or was last drawn.
    // D2-10286, D2-10299, D2-10309
    fn release(&mut self, id: u32, now: f64) {
        let Some(k) = self.systems.iter().position(|s| s.id == id) else {
            return;
        };
        let s = &mut self.systems[k];
        s.refs = s.refs.saturating_sub(1);
        if s.refs != 0 {
            return;
        }
        let mut s = self.systems.swap_remove(k);
        // With no owner it discharges nothing more.
        s.set.stop();
        if s.set.ready_to_die(now) || s.can_be_shared() {
            return;
        }
        let at = if s.set.desc().world_space {
            Some(At {
                home: s.places.first().map_or((0, 0), |p| p.home),
                place: s.set.place(),
                scale: s.set.scale(),
            })
        } else {
            s.places.first().copied()
        };
        if let Some(at) = at {
            self.autonomous.push((s.set, at));
        }
    }
}

/// Every particle system objects have set going.
#[derive(Clone, Debug, Default)]
pub struct ParticleSystems {
    inner: Arc<Mutex<Inner>>,
}

/// An object's hold on a repulsor: the repulsor goes with the last hold on it.
#[derive(Clone, Debug)]
pub struct RepulsorRef(Arc<RepulsorHold>);

#[derive(Debug)]
struct RepulsorHold {
    id: u32,
    owner: Weak<Mutex<Inner>>,
}

impl Drop for RepulsorHold {
    /// The repulsor goes.
    // D2-10297
    fn drop(&mut self) {
        if let Some(o) = self.owner.upgrade() {
            let mut inner = o.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(k) = inner.repulsors.iter().position(|(i, _)| *i == self.id) {
                inner.repulsors.swap_remove(k);
            }
        }
    }
}

/// An object's hold on a system. Cloned, the system has one more owner; dropped, one fewer.
#[derive(Debug)]
pub struct SystemRef {
    id: u32,
    owner: Weak<Mutex<Inner>>,
    /// The client's running time it was last queued at: when it is let go of.
    last: Arc<Mutex<f64>>,
}

impl SystemRef {
    /// The id the manager knows the system by, unique for the run.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.id
    }
}

impl Clone for SystemRef {
    fn clone(&self) -> Self {
        if let Some(o) = self.owner.upgrade() {
            o.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .add_ref(self.id);
        }
        Self {
            id: self.id,
            owner: self.owner.clone(),
            last: self.last.clone(),
        }
    }
}

impl Drop for SystemRef {
    fn drop(&mut self) {
        if let Some(o) = self.owner.upgrade() {
            let now = *self.last.lock().unwrap_or_else(PoisonError::into_inner);
            o.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .release(self.id, now);
        }
    }
}

impl ParticleSystems {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Roll every system made from now on, and the effects' choices, with `dice`: the client's one
    /// generator.
    pub fn set_dice(&self, dice: &Random) {
        self.lock().dice = dice.clone();
    }

    /// The random numbers the systems roll.
    #[must_use]
    pub fn dice(&self) -> Random {
        self.lock().dice.clone()
    }

    /// Whether the effects played from now on place the lights they name: the light source effects
    /// preference (on unless set off). Lights placed before stay.
    pub fn set_light_sources(&self, on: bool) {
        self.lock().no_lights = !on;
    }

    /// Whether the effects place the lights they name.
    #[must_use]
    pub fn light_sources(&self) -> bool {
        !self.lock().no_lights
    }

    /// Whether the effects played from now on make the streaks they name, turn them on, and the
    /// streaks are drawn: the streak effects preference (on unless set off).
    pub fn set_streak_effects(&self, on: bool) {
        self.lock().no_streaks = !on;
    }

    /// Whether the effects make, turn on and draw streaks.
    #[must_use]
    pub fn streak_effects(&self) -> bool {
        !self.lock().no_streaks
    }

    /// Whether the effects played from now on lay the decals they name: the decal effects
    /// preference (on unless set off).
    pub fn set_decal_effects(&self, on: bool) {
        self.lock().no_decals = !on;
    }

    /// Whether the effects lay decals.
    #[must_use]
    pub fn decal_effects(&self) -> bool {
        !self.lock().no_decals
    }

    /// What the player's preferences ask of the effects' entries from now on.
    pub fn set_fx_channels(&self, channels: crate::effects::Channels) {
        self.lock().channels = channels;
    }

    /// What the player's preferences ask of the effects' entries.
    #[must_use]
    pub fn fx_channels(&self) -> crate::effects::Channels {
        self.lock().channels
    }

    /// Whether the systems whose particles move by repulsors (the ground's mists) think and draw:
    /// the volumetric fog effects preference (on unless set off). Off, they stand still and are
    /// not drawn until it is on again.
    // D2-10280, D2-10289
    pub fn set_volumetric_fog(&self, on: bool) {
        self.lock().no_fog = !on;
    }

    /// Whether the systems whose particles move by repulsors think and draw.
    #[must_use]
    pub fn volumetric_fog(&self) -> bool {
        !self.lock().no_fog
    }

    /// A system of record `record` for an owner standing at `place` in block `home`'s frame with
    /// `scale`, made at `now`: when `shareable`, a system of that record already going that may be
    /// shared, else a new one, registered with the next id (the first is 1).
    // D2-10298, D2-10292, D2-10308
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        records: &WorldRecords,
        record: u32,
        shareable: bool,
        home: (i32, i32),
        place: Mat4,
        scale: Vec3,
        particle: f32,
        now: f64,
    ) -> Option<SystemRef> {
        if record == 0 {
            return None;
        }
        let mut inner = self.lock();
        let shared = shareable
            .then(|| {
                inner
                    .systems
                    .iter()
                    .find(|s| s.record == record && s.can_be_shared())
                    .map(|s| s.id)
            })
            .flatten();
        let id = if let Some(id) = shared {
            inner.add_ref(id);
            id
        } else {
            let mut set = ParticleSet::load(records, record, place, scale, now, &inner.dice)?;
            let scale = set.drawn_scale(scale, particle);
            set.set_place(place, scale);
            inner.next_id += 1;
            let id = inner.next_id;
            inner.systems.push(System {
                id,
                record,
                set,
                refs: 1,
                can_be_shared: false,
                queued_at: None,
                places: vec![At { home, place, scale }],
            });
            id
        };
        Some(SystemRef {
            id,
            owner: Arc::downgrade(&self.inner),
            last: Arc::new(Mutex::new(now)),
        })
    }

    /// Set a system of record `record` going on its own, owned by nothing, at `place` in block
    /// `home`'s frame with `scale`, made at `now`: it draws there each frame until it is spent and
    /// its particles are dead. A record that is not a system sets nothing going.
    // D2-10310
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn create_autonomous(
        &self,
        records: &WorldRecords,
        record: u32,
        home: (i32, i32),
        place: Mat4,
        scale: Vec3,
        particle: f32,
        now: f64,
    ) -> bool {
        if record == 0 {
            return false;
        }
        let dice = self.dice();
        let Some(mut set) = ParticleSet::load(records, record, place, scale, now, &dice) else {
            return false;
        };
        let scale = set.drawn_scale(scale, particle);
        set.set_place(place, scale);
        set.place_emitters(now);
        self.lock()
            .autonomous
            .push((set, At { home, place, scale }));
        true
    }

    /// Allow or forbid the system to serve other owners.
    // D2-10270
    pub fn set_can_be_shared(&self, system: &SystemRef, yes: bool) {
        if let Some(s) = self.lock().find(system.id) {
            s.can_be_shared = yes;
        }
    }

    /// Let one particle of emitter `k` of the system go at `now` at `at`, in the frame of its place.
    pub fn discharge(&self, system: &SystemRef, k: usize, now: f64, at: Vec3) {
        if let Some(s) = self.lock().find(system.id) {
            s.set.discharge_at(k, now, at);
        }
    }

    /// The system draws whole however far off it is.
    pub fn set_no_fade(&self, system: &SystemRef) {
        if let Some(s) = self.lock().find(system.id) {
            s.set.set_no_fade(true);
        }
    }

    /// Draw the system this frame (the client's running time `now`) at `place` in block `home`'s
    /// frame with `scale`, lit by `light`. The first place a frame queues it moves it there (its
    /// emitters placed there the first time) and runs it to `now`.
    // D2-10293
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub fn queue(
        &self,
        system: &SystemRef,
        home: (i32, i32),
        place: Mat4,
        scale: Vec3,
        particle: f32,
        light: Color,
        now: f64,
    ) {
        *system.last.lock().unwrap_or_else(PoisonError::into_inner) = now;
        let mut inner = self.lock();
        let Some(s) = inner.find(system.id) else {
            return;
        };
        let scale = s.set.drawn_scale(scale, particle);
        let first = s.queued_at != Some(now);
        if first {
            s.places.clear();
        }
        s.queued_at = Some(now);
        s.places.push(At { home, place, scale });
        if first {
            let pushes = inner.pushes(home, now);
            let still = inner.no_fog;
            let Some(s) = inner.find(system.id) else {
                return;
            };
            s.set.set_place(place, scale);
            s.set.place_emitters(now);
            s.set.set_light(light);
            s.set.set_repulsors(pushes);
            if !(still && s.set.repulsed()) {
                s.set.think(now);
            }
        }
    }

    /// Light the lit systems drawn at `now` (those queued then, and those no object holds) by the
    /// lights about the first place each is drawn at: `lights(home, at)` gives them for a place
    /// `at` in block `home`'s frame, in the order they were kept, their places in that frame, or
    /// nothing to keep the light the system was queued with.
    // D2-10287
    pub fn light_queued(
        &self,
        now: f64,
        mut lights: impl FnMut((i32, i32), Vec3) -> Option<Vec<ParticleLight>>,
    ) {
        let mut inner = self.lock();
        let Inner {
            systems,
            autonomous,
            ..
        } = &mut *inner;
        let queued = systems
            .iter_mut()
            .filter(|s| s.queued_at == Some(now))
            .filter_map(|s| Some((s.places.first().copied()?, &mut s.set)));
        let left = autonomous.iter_mut().map(|(set, at)| (*at, set));
        for (at, set) in queued.chain(left) {
            if !set.lit() {
                continue;
            }
            if let Some(l) = lights(at.home, at.place.point(Vec3::ZERO)) {
                set.set_lights(l);
            }
        }
    }

    /// Whether the system can go at `now`: spent, its particles dead.
    // D2-10277
    #[must_use]
    pub fn ready_to_die(&self, system: &SystemRef, now: f64) -> bool {
        self.lock()
            .find(system.id)
            .is_none_or(|s| s.set.ready_to_die(now))
    }

    /// How many particles the system has alive.
    #[must_use]
    pub fn alive(&self, system: &SystemRef) -> usize {
        self.lock().find(system.id).map_or(0, |s| s.set.alive())
    }

    /// The scales the system draws at this frame, one for each place it was queued, and whether
    /// its description scales it with its object.
    #[must_use]
    pub fn drawn_scales(&self, system: &SystemRef) -> (Vec<Vec3>, bool) {
        self.lock()
            .find(system.id)
            .map_or((Vec::new(), false), |s| {
                (
                    s.places.iter().map(|at| at.scale).collect(),
                    s.set.scales_with_object(),
                )
            })
    }

    /// How many owners hold the system.
    #[must_use]
    pub fn owners(&self, system: &SystemRef) -> u32 {
        self.lock().find(system.id).map_or(0, |s| s.refs)
    }

    /// A repulsor for an object to carry ([`Repulsor::new`]): it goes when the object's last hold
    /// on it does.
    // D2-10306
    #[must_use]
    pub fn create_repulsor(&self, r: Repulsor) -> RepulsorRef {
        let mut inner = self.lock();
        inner.next_repulsor += 1;
        let id = inner.next_repulsor;
        inner.repulsors.push((id, r));
        RepulsorRef(Arc::new(RepulsorHold {
            id,
            owner: Arc::downgrade(&self.inner),
        }))
    }

    /// The repulsor stands at `origin` in block `home`'s frame this frame, its object moving at
    /// `speed`.
    // D2-10314
    pub fn place_repulsor(&self, r: &RepulsorRef, home: (i32, i32), origin: Vec3, speed: f32) {
        let mut inner = self.lock();
        let frame = inner.frames;
        if let Some((_, x)) = inner.repulsors.iter_mut().find(|(i, _)| *i == r.0.id) {
            x.home = home;
            x.origin = origin;
            x.speed = speed;
            x.frame = frame;
        }
    }

    /// Whether the repulsor has outlived its lifetime (none: it has gone).
    #[must_use]
    pub fn repulsor_ready_to_die(&self, r: &RepulsorRef, now: f64) -> bool {
        self.lock()
            .repulsors
            .iter()
            .find(|(i, _)| *i == r.0.id)
            .is_none_or(|(_, x)| x.ready_to_die(now))
    }

    /// How many repulsors objects carry.
    #[must_use]
    pub fn repulsors(&self) -> usize {
        self.lock().repulsors.len()
    }

    /// How many systems objects hold, and how many are left to finish on their own.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        let inner = self.lock();
        (inner.systems.len(), inner.autonomous.len())
    }

    /// What every system draws this frame (the client's running time `now`): each one queued at
    /// `now` at every place it was queued, then each left on its own, run to `now` where it was
    /// left (those spent go); placed in block `base`'s frame (blocks `block_length` long) and seen
    /// from `eye` there with right, forward and up. With the volumetric fog effects off, the
    /// systems moved by repulsors are passed over.
    // D2-10307, D2-10289, D2-10303
    #[must_use]
    pub fn batches(
        &self,
        base: (i32, i32),
        block_length: f32,
        now: f64,
        eye: Vec3,
        axes: (Vec3, Vec3, Vec3),
    ) -> Vec<ObjectBatch> {
        let mut inner = self.lock();
        inner.frames += 1;
        inner.block_length = block_length;
        let mut out = Vec::new();
        let still = inner.no_fog;
        for s in &inner.systems {
            if s.queued_at != Some(now) || (still && s.set.repulsed()) {
                continue;
            }
            for at in &s.places {
                out.extend(draw_at(&s.set, at, base, block_length, now, eye, axes));
            }
        }
        let mut k = 0;
        while k < inner.autonomous.len() {
            if still && inner.autonomous[k].0.repulsed() {
                k += 1;
                continue;
            }
            let pushes = inner.pushes(inner.autonomous[k].1.home, now);
            let (set, at) = &mut inner.autonomous[k];
            set.set_place(at.place, at.scale);
            set.set_repulsors(pushes);
            set.think(now);
            if set.ready_to_die(now) {
                inner.autonomous.swap_remove(k);
                continue;
            }
            let (set, at) = &inner.autonomous[k];
            out.extend(draw_at(set, at, base, block_length, now, eye, axes));
            k += 1;
        }
        out
    }
}

/// What `set` draws at `at`, placed in block `base`'s frame.
fn draw_at(
    set: &ParticleSet,
    at: &At,
    base: (i32, i32),
    block_length: f32,
    now: f64,
    eye: Vec3,
    axes: (Vec3, Vec3, Vec3),
) -> Vec<ObjectBatch> {
    #[allow(clippy::cast_precision_loss)] // block offsets are a few blocks
    let shift = Vec3::new(
        (at.home.0 - base.0) as f32 * block_length,
        (at.home.1 - base.1) as f32 * block_length,
        0.0,
    );
    let eye = Vec3::new(eye.x - shift.x, eye.y - shift.y, eye.z - shift.z);
    let mut out = set.batches_at(at.place, at.scale, now, eye, axes.0, axes.1, axes.2);
    if shift != Vec3::ZERO {
        for b in &mut out {
            b.transform = b.transform.then(&Mat4::translation(shift));
        }
    }
    out
}
