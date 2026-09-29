//! Particles: `ParticleEmitterInfo`'s randomisers, `ParticleEmitter`, `Particle` and
//! `ParticleManager`.
//!
//! A particle system in the client is a hidden physics object whose part array
//! has `max_particles` empty slots, each occupied by a physics part drawing the emitter's gfxobj
//! and positioned every frame by a **closed-form** function of its own age. This crate owns the
//! simulation; the drawing is the renderer's.

pub mod emitter;
pub mod info;
pub mod manager;

pub use emitter::{
    draw_birth_params, BirthParams, EmitterContext, Particle, ParticleEmitter, NO_PART,
};
pub use info::{
    get_random_a, get_random_b, get_random_c, get_random_final_scale, get_random_final_trans,
    get_random_lifespan, get_random_offset, get_random_start_scale, get_random_start_trans,
    should_emit_particle,
};
pub use manager::ParticleManager;
