//! `ParticleEmitterInfo`'s randomisers, in **draw order**.
//!
//! Every one of these draws from the Numerical Recipes `ran2` stream (`dereth_primitives::num::Ran2`), and
//! **the draw order is observable**: all randomisation happens once, at birth, so a rebuild that
//! draws in a different order diverges from the PRNG stream immediately and every particle after it
//! is wrong.
//!
//! The trap inside the trap is [`get_random_offset`], which draws its three components in the order
//! **z, y, x**.

use dereth_primitives::num::rng::Ran2;
use dereth_primitives::Vec3;

use crate::data::ParticleEmitterInfo;
use crate::frame::V3;

/// Returns `clamp(start_scale + rand(-1,1) * scale_rand, 0.1, 10.0)`.
pub fn get_random_start_scale(i: &ParticleEmitterInfo, rng: &mut Ran2) -> f32 {
    (i.start_scale + rng.roll_f32(-1.0, 1.0) * i.scale_rand).clamp(0.1, 10.0)
}

/// A random final scale for one particle.
pub fn get_random_final_scale(i: &ParticleEmitterInfo, rng: &mut Ran2) -> f32 {
    (i.final_scale + rng.roll_f32(-1.0, 1.0) * i.scale_rand).clamp(0.1, 10.0)
}

/// Returns `clamp(start_trans + rand(-1,1) * trans_rand, 0.0, 1.0)`.
pub fn get_random_start_trans(i: &ParticleEmitterInfo, rng: &mut Ran2) -> f32 {
    (i.start_trans + rng.roll_f32(-1.0, 1.0) * i.trans_rand).clamp(0.0, 1.0)
}

/// A random final translucency for one particle.
pub fn get_random_final_trans(i: &ParticleEmitterInfo, rng: &mut Ran2) -> f32 {
    (i.final_trans + rng.roll_f32(-1.0, 1.0) * i.trans_rand).clamp(0.0, 1.0)
}

/// Returns `max(lifespan + rand(-1,1) * lifespan_rand, 0.0)`.
pub fn get_random_lifespan(i: &ParticleEmitterInfo, rng: &mut Ran2) -> f64 {
    let r = f64::from(rng.roll_f32(-1.0, 1.0));
    (i.lifespan + r * i.lifespan_rand).max(0.0)
}

/// Returns `a * (min_a + rand(0,1) * (max_a - min_a))`.
pub fn get_random_a(i: &ParticleEmitterInfo, rng: &mut Ran2) -> Vec3 {
    i.a.mul(i.min_a + rng.roll_f32(0.0, 1.0) * (i.max_a - i.min_a))
}

/// A random `b` vector for one particle.
pub fn get_random_b(i: &ParticleEmitterInfo, rng: &mut Ran2) -> Vec3 {
    i.b.mul(i.min_b + rng.roll_f32(0.0, 1.0) * (i.max_b - i.min_b))
}

/// A random `c` vector for one particle.
pub fn get_random_c(i: &ParticleEmitterInfo, rng: &mut Ran2) -> Vec3 {
    i.c.mul(i.min_c + rng.roll_f32(0.0, 1.0) * (i.max_c - i.min_c))
}

/// Returns a random point on a disc perpendicular to `offset_dir`.
///
/// **The three rolls are drawn z, y, x.** Retail assigns the first uniform `[-1, 1]` roll
/// to `z` and the third to `x`; a rebuild that draws
/// x, y, z diverges from the `ran2` stream immediately.
///
/// A fourth roll happens **only** when the projected vector was normalisable — the degenerate case
/// (`r` parallel to `offset_dir`) returns the zero vector without consuming it.
pub fn get_random_offset(i: &ParticleEmitterInfo, rng: &mut Ran2) -> Vec3 {
    let z = rng.roll_f32(-1.0, 1.0);
    let y = rng.roll_f32(-1.0, 1.0);
    let x = rng.roll_f32(-1.0, 1.0);
    let r = Vec3::new(x, y, z);
    let d = r.dot(i.offset_dir);
    let mut v = r.sub(i.offset_dir.mul(d));
    if v.normalize_check_small() {
        return Vec3::ZERO;
    }
    let len = i.min_offset + rng.roll_f32(0.0, 1.0) * (i.max_offset - i.min_offset);
    v.mul(len)
}

/// The emitter info's own "should emit" test.
///
/// `emitter_type` is tested as **bit flags**, not as an enum, and the per-metre test compares the
/// **squared** distance the emitter has moved against `birthrate²` — so in that mode `birthrate` is
/// a distance, not a time.
#[must_use]
pub fn should_emit_particle(
    i: &ParticleEmitterInfo,
    num_particles: i32,
    total_emitted: i32,
    travel: Vec3,
    last_emit_time: f64,
    now: f64,
) -> bool {
    if i.total_particles > 0 && total_emitted >= i.total_particles {
        return false;
    }
    if num_particles >= i.max_particles {
        return false;
    }
    if i.emitter_type & crate::data::BIRTHRATE_PER_SEC != 0 {
        return now - last_emit_time > i.birthrate;
    }
    if i.emitter_type & crate::data::BIRTHRATE_PER_METER != 0 {
        return f64::from(travel.mag2()) > i.birthrate * i.birthrate;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> ParticleEmitterInfo {
        ParticleEmitterInfo {
            offset_dir: Vec3::new(0.0, 0.0, 1.0),
            min_offset: 1.0,
            max_offset: 2.0,
            a: Vec3::new(1.0, 0.0, 0.0),
            min_a: 1.0,
            max_a: 3.0,
            scale_rand: 0.5,
            trans_rand: 0.5,
            lifespan: 2.0,
            lifespan_rand: 1.0,
            ..ParticleEmitterInfo::default()
        }
    }

    /// ORACLE: the recovered particle behavior, "The
    /// randomisers", cross-read against the `ran2` golden draws in `dereth_primitives::num` through
    /// `dereth_primitives::num::Ran2`.
    ///
    /// `GetRandomOffset` draws **z, y, x** and then, only on the non-degenerate path, a fourth roll
    /// for the radius. Drawing four values from a fresh generator and assembling them by hand must
    /// reproduce the function exactly.
    #[test]
    fn get_random_offset_draws_z_then_y_then_x_then_the_radius() {
        let i = info();
        // Reference: pull the four values first, in the documented order.
        let mut ref_rng = Ran2::new(12345);
        let z = ref_rng.roll_f32(-1.0, 1.0);
        let y = ref_rng.roll_f32(-1.0, 1.0);
        let x = ref_rng.roll_f32(-1.0, 1.0);
        let r = Vec3::new(x, y, z);
        let d = r.dot(i.offset_dir);
        let mut v = r.sub(i.offset_dir.mul(d));
        assert!(
            !v.normalize_check_small(),
            "the fixture direction is not degenerate"
        );
        let len = i.min_offset + ref_rng.roll_f32(0.0, 1.0) * (i.max_offset - i.min_offset);
        let expect = v.mul(len);

        let mut rng = Ran2::new(12345);
        let got = get_random_offset(&i, &mut rng);
        assert_eq!(got, expect);
        // And the generator is left in the same state: exactly four draws were consumed.
        assert_eq!(rng.next_f64(), ref_rng.next_f64());
    }

    /// The projected vector is perpendicular to `offset_dir` and its length is inside
    /// `[min_offset, max_offset]`, which is the geometric claim the formula makes.
    #[test]
    fn the_offset_lies_on_the_disc_perpendicular_to_offset_dir() {
        let i = info();
        let mut rng = Ran2::new(7);
        for _ in 0..50 {
            let o = get_random_offset(&i, &mut rng);
            if o == Vec3::ZERO {
                continue;
            }
            assert!(o.dot(i.offset_dir).abs() < 1e-5, "{o:?}");
            let len = o.mag2().sqrt();
            assert!((1.0..=2.0).contains(&len), "{len}");
        }
    }

    /// The clamps are the documented ones, and a zero randomiser leaves the base value alone
    /// without drawing anything different.
    #[test]
    fn the_scale_and_translucency_randomisers_clamp() {
        let i = ParticleEmitterInfo {
            start_scale: 100.0,
            final_scale: 0.0,
            scale_rand: 0.0,
            start_trans: 5.0,
            final_trans: -5.0,
            trans_rand: 0.0,
            ..ParticleEmitterInfo::default()
        };
        let mut rng = Ran2::new(1);
        assert_eq!(get_random_start_scale(&i, &mut rng), 10.0);
        assert_eq!(get_random_final_scale(&i, &mut rng), 0.1);
        assert_eq!(get_random_start_trans(&i, &mut rng), 1.0);
        assert_eq!(get_random_final_trans(&i, &mut rng), 0.0);
        // A lifespan can never go negative.
        let i = ParticleEmitterInfo {
            lifespan: 0.0,
            lifespan_rand: 5.0,
            ..i
        };
        for _ in 0..20 {
            assert!(get_random_lifespan(&i, &mut rng) >= 0.0);
        }
    }

    /// `ShouldEmitParticle`: the per-second test is a time comparison, the per-metre test is a
    /// **squared** distance comparison, and both quota tests come first.
    #[test]
    fn should_emit_particle_honours_the_quotas_then_the_mode() {
        let per_sec = ParticleEmitterInfo {
            emitter_type: crate::data::BIRTHRATE_PER_SEC,
            birthrate: 0.5,
            max_particles: 10,
            total_particles: 3,
            ..ParticleEmitterInfo::default()
        };
        assert!(should_emit_particle(&per_sec, 0, 0, Vec3::ZERO, 0.0, 1.0));
        assert!(
            !should_emit_particle(&per_sec, 0, 0, Vec3::ZERO, 0.0, 0.5),
            "not > birthrate"
        );
        assert!(
            !should_emit_particle(&per_sec, 0, 3, Vec3::ZERO, 0.0, 1.0),
            "total reached"
        );
        assert!(
            !should_emit_particle(&per_sec, 10, 0, Vec3::ZERO, 0.0, 1.0),
            "slots full"
        );

        let per_m = ParticleEmitterInfo {
            emitter_type: crate::data::BIRTHRATE_PER_METER,
            birthrate: 2.0,
            max_particles: 10,
            ..ParticleEmitterInfo::default()
        };
        assert!(should_emit_particle(
            &per_m,
            0,
            0,
            Vec3::new(3.0, 0.0, 0.0),
            0.0,
            1e9
        ));
        assert!(!should_emit_particle(
            &per_m,
            0,
            0,
            Vec3::new(1.0, 0.0, 0.0),
            0.0,
            1e9
        ));

        // Neither bit set: never emits, whatever the clock says.
        let none = ParticleEmitterInfo {
            max_particles: 10,
            ..ParticleEmitterInfo::default()
        };
        assert!(!should_emit_particle(&none, 0, 0, Vec3::ZERO, 0.0, 1e9));
    }
}
