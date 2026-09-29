//! `ParticleManager`.
//!
//! The one rule worth stating twice: **"blocking" has nothing to do with collision.**
//! A blocking spawn returns 0 and creates nothing when an emitter with that id already exists,
//! whereas a non-blocking spawn replaces it. That is what
//! stops a repeated buff from re-seeding its glow every time it is refreshed.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_primitives::num::rng::Ran2;
use dereth_primitives::Frame;

use crate::data::ParticleEmitterInfo;

use super::emitter::{EmitterContext, ParticleEmitter, NO_PART};

/// Particle emitters indexed by id, with a counter for the next emitter id.
///
/// ORDER-OK: a `BTreeMap` replaces a hash table. The observed iteration order is
/// `(key >> 16 ^ key) % table_size`, which is not reproducible without the table size, and nothing
/// observable depends on it: each emitter updates independently, and the only
/// cross-emitter interaction is creation and destruction by explicit id.
#[derive(Debug, Default, Clone)]
pub struct ParticleManager {
    next_emitter_id: u32,
    emitters: BTreeMap<u32, ParticleEmitter>,
}

impl ParticleManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of emitters the manager holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.emitters.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.emitters.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: u32) -> Option<&ParticleEmitter> {
        self.emitters.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ParticleEmitter> {
        self.emitters.values()
    }

    /// An explicit id **replaces** any
    /// existing emitter with that id. Returns the id used, or `None` if creation failed.
    pub fn create_particle_emitter(
        &mut self,
        info: Arc<ParticleEmitterInfo>,
        part_index: u32,
        offset: Frame,
        emitter_id: u32,
        ctx: &EmitterContext,
        rng: &mut Ran2,
    ) -> Option<u32> {
        if emitter_id != 0 {
            self.emitters.remove(&emitter_id);
        }
        let id = if emitter_id == 0 {
            self.next_emitter_id += 1;
            self.next_emitter_id
        } else {
            emitter_id
        };
        let e = ParticleEmitter::new(id, info, part_index, offset, ctx, rng)?;
        self.emitters.insert(id, e);
        Some(id)
    }

    /// Does nothing when the id
    /// is already live.
    pub fn create_blocking_particle_emitter(
        &mut self,
        info: Arc<ParticleEmitterInfo>,
        part_index: u32,
        offset: Frame,
        emitter_id: u32,
        ctx: &EmitterContext,
        rng: &mut Ran2,
    ) -> Option<u32> {
        if emitter_id != 0 && self.emitters.contains_key(&emitter_id) {
            return None;
        }
        self.create_particle_emitter(info, part_index, offset, emitter_id, ctx, rng)
    }

    /// Returns false for 0 and unknown ids.
    pub fn destroy_particle_emitter(&mut self, id: u32) -> bool {
        id != 0 && self.emitters.remove(&id).is_some()
    }

    /// The existing particles finish their
    /// lives.
    pub fn stop_particle_emitter(&mut self, id: u32) -> bool {
        match self.emitters.get_mut(&id) {
            Some(e) => {
                e.stop();
                true
            }
            None => false,
        }
    }

    /// Update every emitter and delete the ones
    /// that report they are finished.
    pub fn update_particles(&mut self, ctx: &EmitterContext, rng: &mut Ran2) {
        self.update_particles_with_part_frames(ctx, |_| ctx.part_frame, rng);
    }

    /// The same update with the owning object's current part-array lookup.
    ///
    /// The client's own emitter update selects
    /// `parent->part_array->parts[part_index]->pos` separately for every emitter. The manager's
    /// context API keeps the parent object out of this crate; this callback is that indexed lookup.
    /// [`Self::update_particles`] remains the context-only entry point for direct consumers.
    pub fn update_particles_with_part_frames(
        &mut self,
        ctx: &EmitterContext,
        mut part_frame: impl FnMut(u32) -> Option<Frame>,
        rng: &mut Ran2,
    ) {
        let mut dead = Vec::new();
        for (id, e) in &mut self.emitters {
            let mut emitter_ctx = *ctx;
            emitter_ctx.part_frame = if e.part_index == NO_PART {
                None
            } else {
                part_frame(e.part_index)
            };
            if !e.update_particles(&emitter_ctx, rng) {
                dead.push(*id);
            }
        }
        for id in dead {
            self.emitters.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ParticleType;
    use dereth_primitives::{DataId, Vec3};

    fn info() -> Arc<ParticleEmitterInfo> {
        let mut i = ParticleEmitterInfo {
            particle_type: ParticleType::Still,
            hw_gfxobj_id: DataId(0x0100_0001),
            max_particles: 2,
            initial_particles: 1,
            lifespan: 10.0,
            ..ParticleEmitterInfo::default()
        };
        i.init_end();
        Arc::new(i)
    }

    fn ctx() -> EmitterContext {
        EmitterContext {
            parent_frame: Frame::default(),
            part_frame: None,
            emitter_origin: Vec3::ZERO,
            now: 0.0,
            should_draw: true,
        }
    }

    /// ORACLE: the recovered particle behavior section 4 and
    /// section 5.
    #[test]
    fn an_explicit_id_replaces_but_a_blocking_create_does_not() {
        let mut m = ParticleManager::new();
        let mut rng = Ran2::new(1);
        assert_eq!(
            m.create_particle_emitter(info(), 0xFFFF_FFFF, Frame::default(), 7, &ctx(), &mut rng),
            Some(7)
        );
        assert_eq!(m.len(), 1);
        // Replacing resets the emitter, so `total_emitted` goes back to the initial burst.
        assert_eq!(
            m.create_particle_emitter(info(), 0xFFFF_FFFF, Frame::default(), 7, &ctx(), &mut rng),
            Some(7)
        );
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(7).expect("emitter").total_emitted, 1);
        // The blocking form declines outright.
        assert_eq!(
            m.create_blocking_particle_emitter(
                info(),
                0xFFFF_FFFF,
                Frame::default(),
                7,
                &ctx(),
                &mut rng
            ),
            None
        );
        assert_eq!(m.len(), 1);
    }

    /// An id of 0 asks the manager to allocate one, and the counter starts at 1.
    #[test]
    fn a_zero_id_allocates_from_the_counter() {
        let mut m = ParticleManager::new();
        let mut rng = Ran2::new(1);
        assert_eq!(
            m.create_particle_emitter(info(), 0xFFFF_FFFF, Frame::default(), 0, &ctx(), &mut rng),
            Some(1)
        );
        assert_eq!(
            m.create_particle_emitter(info(), 0xFFFF_FFFF, Frame::default(), 0, &ctx(), &mut rng),
            Some(2)
        );
        assert_eq!(m.len(), 2);
    }

    /// Destroy removes; stop lets the existing particles finish and the emitter is reaped on the
    /// update after its last particle dies.
    #[test]
    fn destroy_removes_and_stop_lets_the_particles_finish() {
        let mut m = ParticleManager::new();
        let mut rng = Ran2::new(1);
        m.create_particle_emitter(info(), 0xFFFF_FFFF, Frame::default(), 5, &ctx(), &mut rng);
        assert!(m.stop_particle_emitter(5));
        assert!(m.get(5).expect("emitter").stopped);
        let mut c = ctx();
        c.now = 1.0;
        m.update_particles(&c, &mut rng);
        assert_eq!(m.len(), 1, "the particle is still alive");
        c.now = 100.0;
        m.update_particles(&c, &mut rng);
        assert!(m.is_empty(), "reaped once empty");

        m.create_particle_emitter(info(), 0xFFFF_FFFF, Frame::default(), 6, &ctx(), &mut rng);
        assert!(!m.destroy_particle_emitter(0), "id 0 is not a valid target");
        assert!(m.destroy_particle_emitter(6));
        assert!(m.is_empty());
    }

    /// The indexed update keeps the object-level sentinel on the parent frame, does not move a
    /// finite particle after its birth, and retains the ordinary stop/reap lifetime.
    #[test]
    fn indexed_update_preserves_no_part_finite_birth_and_stop() {
        let mut finite = *info();
        finite.total_particles = 1;
        finite.is_parent_local = 0;
        finite.init_end();

        let born_at = Frame::new(Vec3::new(1.0, 2.0, 3.0), Default::default());
        let mut initial = ctx();
        initial.parent_frame = born_at;
        let mut m = ParticleManager::new();
        let mut rng = Ran2::new(1);
        assert_eq!(
            m.create_particle_emitter(
                Arc::new(finite),
                NO_PART,
                Frame::default(),
                9,
                &initial,
                &mut rng,
            ),
            Some(9)
        );

        let mut moved = initial;
        moved.parent_frame = Frame::new(Vec3::new(40.0, 50.0, 60.0), Default::default());
        moved.now = 1.0;
        m.update_particles_with_part_frames(
            &moved,
            |_| panic!("NO_PART must not request an indexed part frame"),
            &mut rng,
        );
        let particle = m
            .get(9)
            .expect("finite emitter remains")
            .live()
            .next()
            .expect("particle");
        assert_eq!(particle.start_frame, born_at);
        assert_eq!(
            particle.frame, born_at,
            "finite particle remains in its birth frame"
        );

        assert!(m.stop_particle_emitter(9));
        moved.now = 100.0;
        m.update_particles_with_part_frames(
            &moved,
            |_| panic!("NO_PART must not request an indexed part frame"),
            &mut rng,
        );
        assert!(
            m.is_empty(),
            "stopped emitter is reaped after its finite particle expires"
        );
    }
}
