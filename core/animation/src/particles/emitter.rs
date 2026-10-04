//! `Particle` and `ParticleEmitter`: birth, the closed-form trajectories, and death.
//!
//! **Particles are evaluated, not integrated.** Each trajectory evaluates
//! `p(t)` directly from birth parameters, without per-frame integration or accumulated
//! forces. Both `parent_local` rebasing of `birthtime` and degrade-out freezing depend
//! on that distinction.
//!
//! **Two shipped bugs are preserved here.** `Explode` uses `a.x` for all three axes in its `c·a`
//! term and adds `a.z` only to z; `Implode` uses `cos(a.x · t)` for all three axes. Both are what
//! the client does and both are visible.

use std::sync::Arc;

use dereth_primitives::num::math;
use dereth_primitives::num::rng::Ran2;
use dereth_primitives::{Frame, Vec3};

use crate::data::{ParticleEmitterInfo, ParticleType};
use crate::frame::{l2g, localtoglobalvec, rotate, V3};

use super::info::{
    get_random_a, get_random_b, get_random_c, get_random_final_scale, get_random_final_trans,
    get_random_lifespan, get_random_offset, get_random_start_scale, get_random_start_trans,
    should_emit_particle,
};

/// The state used to evaluate one particle over its lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    /// A union in the client: `birthtime` for a finite emitter, `last_update_time` for an infinite
    /// one, which re-bases it every frame.
    pub birthtime: f64,
    pub lifespan: f64,
    pub lifetime: f64,
    /// The parent's frame at birth.
    pub start_frame: Frame,
    /// The birth offset, already rotated into the start frame.
    pub offset: Vec3,
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
    pub start_scale: f32,
    pub final_scale: f32,
    pub start_trans: f32,
    pub final_trans: f32,
    // ---- the outputs `Update` writes into the part ----
    /// The part's world frame. For the non-rotating types only the origin is meaningful.
    pub frame: Frame,
    pub scale: f32,
    pub translucency: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            birthtime: 0.0,
            lifespan: 0.0,
            lifetime: 0.0,
            start_frame: Frame::default(),
            offset: Vec3::ZERO,
            a: Vec3::ZERO,
            b: Vec3::ZERO,
            c: Vec3::ZERO,
            start_scale: 1.0,
            final_scale: 1.0,
            start_trans: 0.0,
            final_trans: 0.0,
            frame: Frame::default(),
            scale: 1.0,
            translucency: 0.0,
        }
    }
}

/// The randomised birth parameters, drawn in `EmitParticle`'s order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BirthParams {
    pub lifespan: f64,
    pub final_trans: f32,
    pub start_trans: f32,
    pub final_scale: f32,
    pub start_scale: f32,
    pub c: Vec3,
    pub b: Vec3,
    pub a: Vec3,
    pub offset: Vec3,
}

/// The randomiser calls, in **exactly** this order.
///
/// lifespan, finalTrans, startTrans, finalScale, startScale, C, B, A, offset — and
/// `GetRandomOffset` itself draws three or four more. Reproduce it verbatim or every particle after
/// the first diverges.
pub fn draw_birth_params(info: &ParticleEmitterInfo, rng: &mut Ran2) -> BirthParams {
    let lifespan = get_random_lifespan(info, rng);
    let final_trans = get_random_final_trans(info, rng);
    let start_trans = get_random_start_trans(info, rng);
    let final_scale = get_random_final_scale(info, rng);
    let start_scale = get_random_start_scale(info, rng);
    let c = get_random_c(info, rng);
    let b = get_random_b(info, rng);
    let a = get_random_a(info, rng);
    let offset = get_random_offset(info, rng);
    BirthParams {
        lifespan,
        final_trans,
        start_trans,
        final_scale,
        start_scale,
        c,
        b,
        a,
        offset,
    }
}

impl Particle {
    /// Initialise one particle.
    ///
    /// `start_frame` is the parent object's frame, or the parent *part*'s frame when the emitter is
    /// parented to a part. The birth offset is rotated into that frame, and then the per-type
    /// switch decides which of `a`, `b`, `c` are local and which are global.
    ///
    /// `Explode` draws two more values (θ then φ) from the generator; every other type draws none.
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        start_frame: Frame,
        parent_offset: Frame,
        p: BirthParams,
        particle_type: ParticleType,
        now: f64,
        rng: &mut Ran2,
    ) -> Self {
        let m = l2g(start_frame.rotation);
        let local = parent_offset.origin.add(p.offset);
        let offset = localtoglobalvec(m, local);
        let to_global = |v: Vec3| localtoglobalvec(m, v);

        let (a, b, c, offset) = match particle_type {
            ParticleType::Unknown | ParticleType::Still => {
                (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, offset)
            }
            ParticleType::LocalVelocity => (to_global(p.a), Vec3::ZERO, Vec3::ZERO, offset),
            ParticleType::ParabolicLvga => (to_global(p.a), p.b, Vec3::ZERO, offset),
            ParticleType::ParabolicLvgagr => (to_global(p.a), p.b, p.c, offset),
            ParticleType::ParabolicLvla => (to_global(p.a), to_global(p.b), Vec3::ZERO, offset),
            ParticleType::ParabolicLvlalr => {
                (to_global(p.a), to_global(p.b), to_global(p.c), offset)
            }
            ParticleType::GlobalVelocity => (p.a, Vec3::ZERO, Vec3::ZERO, offset),
            ParticleType::ParabolicGvga => (p.a, p.b, Vec3::ZERO, offset),
            ParticleType::ParabolicGvgagr => (p.a, p.b, p.c, offset),
            // the per-type meaning of `a`, `b` and `c` for the
            // rarer types is inferred from the enum names, not from a symbol. `Still`,
            // `LocalVelocity`, `ParabolicLVGA` and `GlobalVelocity` are well understood; `Swarm`,
            // `Explode`, `Implode` and the `LR`/`GR` rotating variants below are the inferred ones.
            // `b` is read as the three angular frequencies and `c` as the three amplitudes.
            ParticleType::Swarm => (to_global(p.a), p.b, p.c, offset),
            ParticleType::Explode => {
                let pi = std::f32::consts::PI;
                let theta = rng.roll_f32(-pi, pi);
                let phi = rng.roll_f32(-pi, pi);
                let mut dir = Vec3::new(
                    p.c.x * math::cosf(theta) * math::cosf(phi),
                    p.c.y * math::sinf(theta) * math::cosf(phi),
                    p.c.z * math::sinf(phi),
                );
                if dir.normalize_check_small() {
                    dir = Vec3::ZERO;
                }
                (p.a, p.b, dir, offset)
            }
            ParticleType::Implode => {
                // The particle starts out at a scaled offset and collapses toward the origin.
                let scaled = offset.mul_componentwise(p.c);
                (p.a, p.b, scaled, scaled)
            }
        };

        let mut particle = Self {
            birthtime: now,
            lifespan: p.lifespan,
            lifetime: 0.0,
            start_frame,
            offset,
            a,
            b,
            c,
            start_scale: p.start_scale,
            final_scale: p.final_scale,
            start_trans: p.start_trans,
            final_trans: p.final_trans,
            frame: start_frame,
            scale: p.start_scale,
            translucency: p.start_trans,
        };
        particle.update(particle_type, false, &start_frame, now);
        particle
    }

    /// Advance one particle.
    ///
    /// `parent_local` is **not** `info.is_parent_local`: it is derived per update as
    /// `(total_particles == 0 && total_seconds == 0.0)`, i.e. "this emitter is infinite". An
    /// infinite emitter accumulates lifetime and re-bases `birthtime` every frame, which is what
    /// lets it survive a pause or a degrade-out; a finite one uses absolute age.
    pub fn update(
        &mut self,
        particle_type: ParticleType,
        parent_local: bool,
        base: &Frame,
        now: f64,
    ) {
        let dt = now - self.birthtime;
        if parent_local {
            self.lifetime += dt;
            self.birthtime = now;
        } else {
            self.lifetime = dt;
        }
        #[allow(clippy::cast_possible_truncation)]
        let t = self.lifetime as f32;
        let o = base.origin;
        let (a, b, c, off) = (self.a, self.b, self.c, self.offset);

        match particle_type {
            ParticleType::Unknown | ParticleType::Still => {
                self.frame = Frame::new(o.add(off), base.rotation);
            }
            ParticleType::LocalVelocity | ParticleType::GlobalVelocity => {
                self.frame = Frame::new(o.add(off).add(a.mul(t)), base.rotation);
            }
            ParticleType::ParabolicLvga
            | ParticleType::ParabolicLvla
            | ParticleType::ParabolicGvga => {
                let p = o.add(off).add(a.mul(t)).add(b.mul(0.5 * t * t));
                self.frame = Frame::new(p, base.rotation);
            }
            ParticleType::ParabolicLvgagr
            | ParticleType::ParabolicLvlalr
            | ParticleType::ParabolicGvgagr => {
                let mut f = *base;
                f.origin = f.origin.add(off).add(a.mul(t)).add(b.mul(0.5 * t * t));
                rotate(&mut f, c.mul(t));
                self.frame = f;
            }
            ParticleType::Swarm => {
                self.frame = Frame::new(
                    Vec3::new(
                        o.x + off.x + a.x * t + c.x * math::cosf(b.x * t),
                        o.y + off.y + a.y * t + c.y * math::sinf(b.y * t),
                        o.z + off.z + a.z * t + c.z * math::cosf(b.z * t),
                    ),
                    base.rotation,
                );
            }
            ParticleType::Explode => {
                // **Shipped bug, preserved**: `a.x` is used for all three axes in the `c·a` term,
                // and `a.z` is added only to z.
                self.frame = Frame::new(
                    Vec3::new(
                        o.x + off.x + (c.x * a.x + b.x * t) * t,
                        o.y + off.y + (c.y * a.x + b.y * t) * t,
                        o.z + off.z + (c.z * a.x + b.z * t + a.z) * t,
                    ),
                    base.rotation,
                );
            }
            ParticleType::Implode => {
                // **Shipped bug, preserved**: `cos(a.x · t)` for all three axes.
                let ca = math::cosf(a.x * t);
                self.frame = Frame::new(
                    Vec3::new(
                        o.x + off.x + b.x * t * t + c.x * ca,
                        o.y + off.y + b.y * t * t + c.y * ca,
                        o.z + off.z + b.z * t * t + c.z * ca,
                    ),
                    base.rotation,
                );
            }
        }

        #[allow(clippy::cast_possible_truncation)]
        let f = if self.lifetime < self.lifespan {
            (self.lifetime / self.lifespan) as f32
        } else {
            1.0
        };
        self.scale = self.start_scale + (self.final_scale - self.start_scale) * f;
        self.translucency = self.start_trans + (self.final_trans - self.start_trans) * f;
    }

    /// The world origin this particle's part is drawn at.
    #[must_use]
    pub const fn origin(&self) -> Vec3 {
        self.frame.origin
    }
}

/// The world facts the emitter needs each frame. The original client reads these from
/// the hidden particle physics object and its parent; this crate receives the facts
/// as data and needs no physics object of its own.
#[derive(Debug, Clone, Copy)]
pub struct EmitterContext {
    /// The parent object's frame.
    pub parent_frame: Frame,
    /// The parent *part*'s frame, when `part_index != 0xFFFFFFFF`.
    pub part_frame: Option<Frame>,
    /// The particle object's own world origin, for the per-metre birthrate test.
    pub emitter_origin: Vec3,
    /// The physics clock's current time.
    pub now: f64,
    /// Decide whether particles draw at `degrade_distance`.
    pub should_draw: bool,
}

/// `ParticleEmitter`.
#[derive(Debug, Clone)]
pub struct ParticleEmitter {
    pub id: u32,
    pub info: Arc<ParticleEmitterInfo>,
    /// `0xFFFFFFFF` means "the object itself, not a part".
    pub part_index: u32,
    pub parent_offset: Frame,
    /// One slot per `max_particles`. `None` is a free slot; the client nulls the `parts[]` pointer
    /// and keeps the mesh in `part_storage` for reuse.
    pub particles: Vec<Option<Particle>>,
    pub num_particles: i32,
    pub total_emitted: i32,
    pub last_emit_time: f64,
    pub last_emit_offset: Vec3,
    pub stopped: bool,
    pub creation_time: f64,
    pub degraded_out: bool,
    pub degrade_distance: f32,
    pub last_update_time: f64,
    /// The particle object's own world origin as of the last update: the parent's frame, or the
    /// parent part's when the emitter hangs off a part, carried through the parent offset. The
    /// particles are that object's parts, so it is the point a renderer measures the object-level
    /// viewer distance from.
    pub object_origin: Vec3,
}

/// `part_index` sentinel meaning "the object itself".
pub const NO_PART: u32 = 0xFFFF_FFFF;

impl ParticleEmitter {
    /// Set the emitter's info, and finish its initialisation.
    ///
    /// A `hw_gfxobj_id` of `INVALID_DID` fails emitter creation outright, which is why this returns
    /// an `Option`. `InitEnd` then emits `initial_particles` immediately.
    #[must_use]
    pub fn new(
        id: u32,
        info: Arc<ParticleEmitterInfo>,
        part_index: u32,
        parent_offset: Frame,
        ctx: &EmitterContext,
        rng: &mut Ran2,
    ) -> Option<Self> {
        if info.hw_gfxobj_id.0 == 0 {
            return None;
        }
        let slots = usize::try_from(info.max_particles.max(0)).unwrap_or(0);
        let mut e = Self {
            id,
            info,
            part_index,
            parent_offset,
            particles: vec![None; slots],
            num_particles: 0,
            total_emitted: 0,
            last_emit_time: ctx.now,
            last_emit_offset: ctx.emitter_origin,
            stopped: false,
            creation_time: ctx.now,
            degraded_out: false,
            degrade_distance: 100.0,
            last_update_time: ctx.now,
            object_origin: Vec3::ZERO,
        };
        e.object_origin = e.follow_parent(ctx);
        for _ in 0..e.info.initial_particles {
            e.emit_particle(ctx, rng);
        }
        Some(e)
    }

    /// `parent_local` — derived, not read from the info: an emitter with no particle and no time
    /// limit is infinite.
    #[must_use]
    pub fn is_infinite(&self) -> bool {
        self.info.total_particles == 0 && self.info.total_seconds == 0.0
    }

    /// Where the particle object sits: the base frame carried through the parent offset.
    fn follow_parent(&self, ctx: &EmitterContext) -> Vec3 {
        let base = self.base_frame(ctx);
        base.origin.add(localtoglobalvec(
            l2g(base.rotation),
            self.parent_offset.origin,
        ))
    }

    fn base_frame(&self, ctx: &EmitterContext) -> Frame {
        if self.part_index == NO_PART {
            ctx.parent_frame
        } else {
            ctx.part_frame.unwrap_or(ctx.parent_frame)
        }
    }

    /// Emit one particle.
    pub fn emit_particle(&mut self, ctx: &EmitterContext, rng: &mut Ran2) {
        let Some(i) = self.particles.iter().position(Option::is_none) else {
            return; // full
        };
        let p = draw_birth_params(&self.info, rng);
        let start_frame = self.base_frame(ctx);
        let particle = Particle::init(
            start_frame,
            self.parent_offset,
            p,
            self.info.particle_type,
            ctx.now,
            rng,
        );
        self.particles[i] = Some(particle);
        self.num_particles += 1;
        self.total_emitted += 1;
        self.last_emit_offset = ctx.emitter_origin;
        self.last_emit_time = ctx.now;
    }

    /// The bookkeeping half, used while the
    /// emitter is degraded out so the schedule keeps advancing.
    pub fn record_particle_emission(&mut self, ctx: &EmitterContext) {
        self.total_emitted += 1;
        self.last_emit_offset = ctx.emitter_origin;
        self.last_emit_time = ctx.now;
    }

    /// Whether the emitter should emit this step.
    #[must_use]
    pub fn should_emit(&self, ctx: &EmitterContext) -> bool {
        let travel = if self.info.emitter_type & crate::data::BIRTHRATE_PER_METER != 0 {
            ctx.emitter_origin.sub(self.last_emit_offset)
        } else {
            Vec3::ZERO
        };
        should_emit_particle(
            &self.info,
            self.num_particles,
            self.total_emitted,
            travel,
            self.last_emit_time,
            ctx.now,
        )
    }

    /// Frees the *slot*, not the mesh.
    fn kill_particle(&mut self, i: usize) -> bool {
        let dead = self.particles[i].is_some_and(|p| p.lifespan <= p.lifetime);
        if dead {
            self.particles[i] = None;
            self.num_particles -= 1;
        }
        dead
    }

    /// Stop the emitter.
    pub fn stop_emitter(&mut self, now: f64) -> bool {
        if !self.stopped {
            if self.info.total_seconds > 0.0 && self.creation_time + self.info.total_seconds < now {
                self.stopped = true;
            }
            if self.info.total_particles != 0 && self.total_emitted >= self.info.total_particles {
                self.stopped = true;
            }
        }
        self.stopped
    }

    /// Set the flag directly; the existing
    /// particles finish their lives.
    pub fn stop(&mut self) {
        self.stopped = true;
    }

    /// Returns false when the emitter should be
    /// destroyed.
    pub fn update_particles(&mut self, ctx: &EmitterContext, rng: &mut Ran2) -> bool {
        let parent_local = self.is_infinite();
        let particle_type = self.info.particle_type;
        // The particle object follows its parent whether or not its particles are drawn.
        self.object_origin = self.follow_parent(ctx);

        if ctx.should_draw {
            if self.degraded_out {
                self.degraded_out = false;
            }
            for i in 0..self.particles.len() {
                if self.particles[i].is_none() {
                    continue;
                }
                // `is_parent_local == 0` freezes the particle in its **birth** frame; otherwise it
                // rides the parent (or the parent's part).
                let base = if self.info.is_parent_local == 0 {
                    self.particles[i].map_or(ctx.parent_frame, |p| p.start_frame)
                } else {
                    self.base_frame(ctx)
                };
                if let Some(p) = &mut self.particles[i] {
                    p.update(particle_type, parent_local, &base, ctx.now);
                }
                self.kill_particle(i);
            }
            let result = if self.stopped {
                self.num_particles != 0
            } else {
                if self.should_emit(ctx) {
                    self.emit_particle(ctx, rng);
                }
                self.stop_emitter(ctx.now);
                true
            };
            self.last_update_time = ctx.now;
            return result;
        }

        // ---- degraded out ----
        self.degraded_out = true;
        self.last_update_time = ctx.now;
        if self.is_infinite() {
            // Freeze: re-base every birthtime so no time passes for the particles.
            for slot in self.particles.iter_mut().flatten() {
                slot.birthtime = ctx.now;
            }
            return true;
        }
        for i in 0..self.particles.len() {
            if let Some(p) = &mut self.particles[i] {
                p.lifetime = ctx.now - p.birthtime;
            }
            self.kill_particle(i);
        }
        if self.stopped {
            return self.num_particles != 0;
        }
        if self.should_emit(ctx) {
            self.record_particle_emission(ctx);
        }
        self.stop_emitter(ctx.now);
        true
    }

    /// Live particles, for a renderer or a test.
    pub fn live(&self) -> impl Iterator<Item = &Particle> {
        self.particles.iter().flatten()
    }

    /// Move the space this emitter was simulated in by `by`: every live particle's birth frame
    /// and drawn frame, the last emission point and the particle object's origin.
    ///
    /// For a host whose coordinates move under a fixed world (a renderer that re-centres on the
    /// viewer's landblock). A particle keeps its birth frame for its whole life, and a
    /// degraded-out infinite emitter's particles are frozen, so without this they would stay at
    /// their old coordinates and appear moved by the whole shift in the world.
    pub fn translate(&mut self, by: Vec3) {
        for p in self.particles.iter_mut().flatten() {
            p.start_frame.origin = p.start_frame.origin.add(by);
            p.frame.origin = p.frame.origin.add(by);
        }
        self.last_emit_offset = self.last_emit_offset.add(by);
        self.object_origin = self.object_origin.add(by);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::{DataId, Quat};

    fn info(particle_type: ParticleType) -> Arc<ParticleEmitterInfo> {
        let mut i = ParticleEmitterInfo {
            particle_type,
            hw_gfxobj_id: DataId(0x0100_0001),
            emitter_type: crate::data::BIRTHRATE_PER_SEC,
            birthrate: 0.1,
            max_particles: 4,
            initial_particles: 0,
            lifespan: 1.0,
            offset_dir: Vec3::new(0.0, 0.0, 1.0),
            min_offset: 0.5,
            max_offset: 1.5,
            a: Vec3::new(1.0, 2.0, 3.0),
            min_a: 1.0,
            max_a: 3.0,
            b: Vec3::new(0.5, 0.5, 0.5),
            min_b: 0.5,
            max_b: 2.0,
            c: Vec3::new(1.0, 1.0, 1.0),
            lifespan_rand: 0.25,
            ..ParticleEmitterInfo::default()
        };
        i.init_end();
        Arc::new(i)
    }

    fn ctx(now: f64) -> EmitterContext {
        EmitterContext {
            parent_frame: Frame::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY),
            part_frame: None,
            emitter_origin: Vec3::ZERO,
            now,
            should_draw: true,
        }
    }

    /// The particle object sits at its parent's frame carried through the parent offset, and
    /// follows the parent on every update, drawn or not.
    #[test]
    fn the_particle_object_follows_its_parent_through_the_offset() {
        let offset = Frame::new(Vec3::new(0.0, 2.0, 1.0), Quat::IDENTITY);
        let mut rng = Ran2::new(1);
        let mut e = ParticleEmitter::new(
            1,
            info(ParticleType::Still),
            NO_PART,
            offset,
            &ctx(0.0),
            &mut rng,
        )
        .expect("the emitter is created");
        assert_eq!(e.object_origin, Vec3::new(10.0, 2.0, 1.0));
        // The parent turns a quarter about z and moves; the offset turns with it.
        let turned = EmitterContext {
            parent_frame: Frame::new(
                Vec3::new(20.0, 0.0, 0.0),
                Quat {
                    w: std::f32::consts::FRAC_1_SQRT_2,
                    x: 0.0,
                    y: 0.0,
                    z: std::f32::consts::FRAC_1_SQRT_2,
                },
            ),
            should_draw: false,
            ..ctx(1.0)
        };
        e.update_particles(&turned, &mut rng);
        let o = e.object_origin;
        assert!(
            (o.x - 18.0).abs() < 1e-4 && o.y.abs() < 1e-4 && (o.z - 1.0).abs() < 1e-4,
            "{o:?}"
        );
    }

    /// Moving the emitter's space moves every particle with it: an emitter simulated in a space
    /// that shifts part-way through its life, and translated at the shift, draws every particle
    /// exactly where an emitter simulated in the unshifted space draws it, less the shift. That
    /// holds for particles frozen in their birth frame and for a degraded-out emitter's frozen
    /// ones, which nothing else would ever move.
    #[test]
    fn translating_the_space_keeps_every_particle_where_it_is_in_the_world() {
        let mut i = *info(ParticleType::LocalVelocity);
        i.is_parent_local = 0;
        i.lifespan = 20.0;
        i.lifespan_rand = 0.0;
        i.max_particles = 16;
        i.emitter_type = crate::data::BIRTHRATE_PER_METER;
        i.birthrate = 3.5;
        let i = Arc::new(i);
        let shift = Vec3::new(-192.0, 192.0, 0.0);
        let at = |t: f64, moved: bool, x: f32| EmitterContext {
            parent_frame: Frame::new(
                Vec3::new(x, 0.0, 0.0).add(if moved { shift } else { Vec3::ZERO }),
                Quat::IDENTITY,
            ),
            emitter_origin: Vec3::new(x, 0.0, 0.0).add(if moved { shift } else { Vec3::ZERO }),
            ..ctx(t)
        };
        let (mut ra, mut rb) = (Ran2::new(7), Ran2::new(7));
        let mut a = ParticleEmitter::new(
            1,
            Arc::clone(&i),
            NO_PART,
            Frame::default(),
            &at(0.0, false, 0.0),
            &mut ra,
        )
        .expect("a");
        let mut b = ParticleEmitter::new(
            1,
            Arc::clone(&i),
            NO_PART,
            Frame::default(),
            &at(0.0, false, 0.0),
            &mut rb,
        )
        .expect("b");
        // The parent walks, so particles are born along the way.
        for step in 1u8..=10 {
            let t = f64::from(step) * 0.1;
            let x = f32::from(step);
            a.update_particles(&at(t, false, x), &mut ra);
            b.update_particles(&at(t, false, x), &mut rb);
        }
        assert!(b.live().count() >= 2, "particles were born along the walk");
        let compare = |a: &ParticleEmitter, b: &ParticleEmitter, when: &str| {
            let (pa, pb): (Vec<Vec3>, Vec<Vec3>) = (
                a.live().map(|p| p.frame.origin).collect(),
                b.live().map(|p| p.frame.origin).collect(),
            );
            assert_eq!(pa.len(), pb.len(), "{when}: the same particles live");
            for (x, y) in pa.iter().zip(&pb) {
                let d = y.sub(x.add(shift));
                assert!(
                    d.mag2() < 1e-6,
                    "{when}: {y:?} is not {x:?} moved by the shift"
                );
            }
        };
        let step_both = |a: &mut ParticleEmitter,
                         b: &mut ParticleEmitter,
                         ra: &mut Ran2,
                         rb: &mut Ran2,
                         step: u8,
                         draw: bool,
                         moved: bool| {
            let (t, x) = (f64::from(step) * 0.1, f32::from(step));
            let c = |m| EmitterContext {
                should_draw: draw,
                ..at(t, m, x)
            };
            a.update_particles(&c(false), ra);
            b.update_particles(&c(moved), rb);
        };
        // Degraded out: the particles are frozen where they are.
        for step in 11u8..=13 {
            step_both(&mut a, &mut b, &mut ra, &mut rb, step, false, false);
        }
        // The space moves under the frozen emitter.
        b.translate(shift);
        compare(&a, &b, "frozen, straight after the shift");
        // Drawn again: every particle is re-evaluated from its birth frame, and the emitter
        // measures its travel since the last emission from the moved emission point.
        for step in 14u8..=20 {
            step_both(&mut a, &mut b, &mut ra, &mut rb, step, true, true);
        }
        compare(&a, &b, "drawn again");
        assert!(b.object_origin.sub(a.object_origin.add(shift)).mag2() < 1e-6);
    }

    /// ORACLE: the recovered particle behavior, "Emission" —
    /// the PRNG call order is lifespan, finalTrans, startTrans, finalScale, startScale, C, B, A,
    /// offset.
    #[test]
    fn the_birth_draw_order_is_the_documented_one() {
        let i = info(ParticleType::LocalVelocity);
        let mut a = Ran2::new(99);
        let got = draw_birth_params(&i, &mut a);

        let mut b = Ran2::new(99);
        let lifespan = get_random_lifespan(&i, &mut b);
        let final_trans = get_random_final_trans(&i, &mut b);
        let start_trans = get_random_start_trans(&i, &mut b);
        let final_scale = get_random_final_scale(&i, &mut b);
        let start_scale = get_random_start_scale(&i, &mut b);
        let c = get_random_c(&i, &mut b);
        let bb = get_random_b(&i, &mut b);
        let aa = get_random_a(&i, &mut b);
        let offset = get_random_offset(&i, &mut b);

        assert_eq!(got.lifespan, lifespan);
        assert_eq!(got.final_trans, final_trans);
        assert_eq!(got.start_trans, start_trans);
        assert_eq!(got.final_scale, final_scale);
        assert_eq!(got.start_scale, start_scale);
        assert_eq!(got.c, c);
        assert_eq!(got.b, bb);
        assert_eq!(got.a, aa);
        assert_eq!(got.offset, offset);
        // Draw-for-draw: the two generators are in the same state afterwards.
        assert_eq!(a.next_f64(), b.next_f64());
    }

    /// A seeded emitter reproduces an identical burst: same seed, same particles, and a different
    /// seed gives different ones.
    #[test]
    fn a_seeded_emitter_reproduces_an_identical_burst() {
        let i = info(ParticleType::ParabolicLvga);
        let burst = |seed: i32| {
            let mut rng = Ran2::new(seed);
            let mut e = ParticleEmitter::new(
                1,
                Arc::clone(&i),
                NO_PART,
                Frame::default(),
                &ctx(0.0),
                &mut rng,
            )
            .expect("emitter");
            for k in 0..4 {
                e.emit_particle(&ctx(f64::from(k) * 0.2), &mut rng);
            }
            e.live()
                .map(|p| (p.a, p.b, p.offset, p.lifespan))
                .collect::<Vec<_>>()
        };
        assert_eq!(burst(4242), burst(4242));
        assert_ne!(burst(4242), burst(1));
    }

    /// The trajectory is closed form: evaluating at `t` from the birth parameters gives the same
    /// answer whether or not intermediate updates happened. That is the property an integrator
    /// loses.
    #[test]
    fn the_trajectory_is_closed_form_and_independent_of_the_update_cadence() {
        let i = info(ParticleType::ParabolicLvga);
        let mut rng = Ran2::new(5);
        let p = draw_birth_params(&i, &mut rng);
        let base = Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY);

        let mut one = Particle::init(
            base,
            Frame::default(),
            p,
            i.particle_type,
            0.0,
            &mut Ran2::new(1),
        );
        one.update(i.particle_type, false, &base, 1.0);

        let mut many = Particle::init(
            base,
            Frame::default(),
            p,
            i.particle_type,
            0.0,
            &mut Ran2::new(1),
        );
        for k in 1..=10 {
            many.update(i.particle_type, false, &base, f64::from(k) * 0.1);
        }
        let (a, b) = (one.origin(), many.origin());
        assert!((a.x - b.x).abs() < 1e-5 && (a.y - b.y).abs() < 1e-5 && (a.z - b.z).abs() < 1e-5);
    }

    /// **The shipped bug.** `Explode` uses `a.x` in the `c·a` term of all three axes, and adds
    /// `a.z` only to z. Reproduced here by evaluating the formula by hand.
    #[test]
    fn explode_reuses_the_x_component_of_a_for_all_three_axes() {
        let mut p = Particle {
            offset: Vec3::ZERO,
            a: Vec3::new(2.0, 100.0, 7.0),
            b: Vec3::ZERO,
            c: Vec3::new(1.0, 1.0, 1.0),
            lifespan: 10.0,
            ..Particle::default()
        };
        let base = Frame::default();
        p.update(ParticleType::Explode, false, &base, 1.0);
        let o = p.origin();
        // x: (c.x * a.x + 0) * 1 = 2; y: (c.y * a.x + 0) * 1 = 2 -- NOT a.y = 100;
        // z: (c.z * a.x + 0 + a.z) * 1 = 2 + 7 = 9.
        assert_eq!((o.x, o.y, o.z), (2.0, 2.0, 9.0));
    }

    /// **The shipped bug.** `Implode` uses `cos(a.x · t)` on all three axes.
    #[test]
    fn implode_uses_cos_of_a_x_for_all_three_axes() {
        let mut p = Particle {
            offset: Vec3::ZERO,
            a: Vec3::new(1.0, 0.0, 0.0),
            b: Vec3::ZERO,
            c: Vec3::new(1.0, 2.0, 3.0),
            lifespan: 10.0,
            ..Particle::default()
        };
        p.update(ParticleType::Implode, false, &Frame::default(), 2.0);
        let ca = math::cosf(2.0_f32);
        let o = p.origin();
        assert!((o.x - ca).abs() < 1e-6);
        assert!((o.y - 2.0 * ca).abs() < 1e-6);
        assert!((o.z - 3.0 * ca).abs() < 1e-6);
    }

    /// `is_parent_local == 0` freezes the particle in its birth frame; a non-zero value makes it
    /// ride the parent.
    #[test]
    fn is_parent_local_decides_whether_the_particle_rides_the_parent() {
        let frozen = {
            let mut i = *info(ParticleType::Still);
            i.is_parent_local = 0;
            i.initial_particles = 1;
            Arc::new(i)
        };
        let riding = {
            let mut i = *frozen;
            i.is_parent_local = 1;
            Arc::new(i)
        };
        let run = |info: Arc<ParticleEmitterInfo>| {
            let mut rng = Ran2::new(3);
            let mut c = ctx(0.0);
            let mut e =
                ParticleEmitter::new(1, info, NO_PART, Frame::default(), &c, &mut rng).expect("e");
            // The parent moves.
            c.parent_frame = Frame::new(Vec3::new(1000.0, 0.0, 0.0), Quat::IDENTITY);
            c.now = 0.05;
            e.update_particles(&c, &mut rng);
            let x = e.live().next().expect("one particle").origin().x;
            x
        };
        assert!(run(frozen) < 100.0, "frozen in the birth frame");
        assert!(run(riding) > 100.0, "riding the parent");
    }

    /// An infinite emitter re-bases `birthtime` every update (`parent_local`), which is what lets
    /// it be frozen while degraded out; a finite one uses absolute age.
    #[test]
    fn an_infinite_emitter_accumulates_lifetime_and_re_bases_birthtime() {
        let mut p = Particle {
            lifespan: 100.0,
            ..Particle::default()
        };
        p.update(ParticleType::Still, true, &Frame::default(), 1.0);
        assert_eq!(p.lifetime, 1.0);
        assert_eq!(p.birthtime, 1.0, "re-based");
        p.update(ParticleType::Still, true, &Frame::default(), 3.0);
        assert_eq!(p.lifetime, 3.0);

        let mut p = Particle {
            lifespan: 100.0,
            ..Particle::default()
        };
        p.update(ParticleType::Still, false, &Frame::default(), 1.0);
        p.update(ParticleType::Still, false, &Frame::default(), 3.0);
        assert_eq!(p.lifetime, 3.0, "absolute age");
        assert_eq!(p.birthtime, 0.0, "never re-based");
    }

    /// A particle whose lifespan has elapsed frees its slot, and the slot is reused.
    #[test]
    fn a_dead_particle_frees_its_slot_for_reuse() {
        let i = info(ParticleType::Still);
        let mut rng = Ran2::new(11);
        let mut e =
            ParticleEmitter::new(1, i, NO_PART, Frame::default(), &ctx(0.0), &mut rng).expect("e");
        e.emit_particle(&ctx(0.0), &mut rng);
        assert_eq!(e.num_particles, 1);
        // Past the lifespan: the update kills it.
        e.update_particles(&ctx(5.0), &mut rng);
        assert_eq!(
            e.num_particles, 1,
            "one died and one was emitted in its place"
        );
        assert!(e.particles.iter().filter(|p| p.is_some()).count() == 1);
    }

    /// `StopEmitter` latches on either quota, and a stopped emitter reports "destroy me" once its
    /// last particle has died.
    #[test]
    fn stop_emitter_latches_and_the_emitter_dies_when_empty() {
        let i = {
            let mut i = *info(ParticleType::Still);
            i.total_particles = 1;
            i.initial_particles = 1;
            i.lifespan = 0.5;
            Arc::new(i)
        };
        let mut rng = Ran2::new(2);
        let mut e =
            ParticleEmitter::new(9, i, NO_PART, Frame::default(), &ctx(0.0), &mut rng).expect("e");
        assert_eq!(e.total_emitted, 1);
        assert!(e.update_particles(&ctx(0.1), &mut rng));
        assert!(e.stopped, "the total-particles quota latched it");
        assert!(
            !e.update_particles(&ctx(1.0), &mut rng),
            "the last particle died"
        );
    }
}
