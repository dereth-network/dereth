//! Particles.
//!
//! A particle's update and init, the emitter's particle update and emit steps, the kill step,
//! and the object's "should draw particles" test.
//!
//! Particle records are described in `docs/formats/17-scene-and-particles.md`.
//!
//! **Particles are not a special renderer.** A `ParticleEmitter` owns a hidden physics object (id 0,
//! `STATIC_PS | PARTICLE_EMITTER_PS`) whose part array has `max_particles` parts all sharing one
//! graphics object; each live particle drives one part's frame, uniform scale and translucency, and the
//! parts are then drawn through the ordinary object path. Because particle surfaces are almost
//! always alpha or additive they end up in the **alpha lists** and are drawn in insertion order.
//!
//! There is **no per-frame integration and no force accumulation**: each update evaluates
//! `p(t)` directly from the particle's birth parameters and its age, so a particle's whole path is
//! reproducible from its birth draw. All randomisation happens once, at birth.
//!
//! Three shipped oddities are reproduced verbatim below, each marked where it occurs.

use dereth_primitives::num::math;
use dereth_primitives::{Frame, Vec3};

use crate::math::V3;

/// `ParticleType`. `NumParticleType` is 13.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ParticleType {
    Unknown = 0,
    Still = 1,
    LocalVelocity = 2,
    ParabolicLvga = 3,
    ParabolicLvgaGr = 4,
    Swarm = 5,
    Explode = 6,
    Implode = 7,
    ParabolicLvla = 8,
    ParabolicLvlaLr = 9,
    ParabolicGvga = 10,
    ParabolicGvgaGr = 11,
    GlobalVelocity = 12,
}

impl ParticleType {
    /// Decode the serialised `u32`. The per-type meaning of the `a`/`b`/`c`
    /// vectors is inferred from the enum names; the interpretation implemented here is that one.
    #[must_use]
    pub const fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Still,
            2 => Self::LocalVelocity,
            3 => Self::ParabolicLvga,
            4 => Self::ParabolicLvgaGr,
            5 => Self::Swarm,
            6 => Self::Explode,
            7 => Self::Implode,
            8 => Self::ParabolicLvla,
            9 => Self::ParabolicLvlaLr,
            10 => Self::ParabolicGvga,
            11 => Self::ParabolicGvgaGr,
            12 => Self::GlobalVelocity,
            _ => Self::Unknown,
        }
    }

    /// The `*GR`/`*LR` types rotate the whole frame rather than just moving the origin.
    #[must_use]
    pub const fn rotates(self) -> bool {
        matches!(
            self,
            Self::ParabolicLvgaGr | Self::ParabolicLvlaLr | Self::ParabolicGvgaGr
        )
    }
}

/// One particle's birth parameters — everything seeds, drawn once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub offset: Vec3,
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
    pub start_scale: f32,
    pub final_scale: f32,
    pub start_trans: f32,
    pub final_trans: f32,
    pub lifespan: f32,
    /// Accumulated age for a looping emitter, absolute age for a finite one.
    pub lifetime: f32,
}

/// What one particle contributes to its part each frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParticleDrawState {
    pub frame: Frame,
    /// `part.gfxobj_scale` — **uniform**; there is no non-uniform particle scale.
    pub scale: f32,
    /// Translucency. Reaching exactly **1.0** sets NoDraw, so a
    /// particle that fades to `final_trans = 1.0` disappears cleanly on its last frame.
    pub translucency: f32,
    pub no_draw: bool,
}

/// Evaluate a particle's position, scale and translucency in closed form.
///
/// `t` is the particle's age in seconds and `parent` the base frame this update.
///
/// Three oddities are reproduced exactly, and each looks like an original bug:
///
/// * **`Explode` uses `a.x` for all three axes** in the `c·a` term, and adds `a.z` only to z.
/// * **`Implode` uses `cos(a.x * t)` for all three axes.**
/// * The acceleration term is `0.5 * b * t²`, so `b` is a true acceleration; a gravity emitter
///   simply sets `b = (0, 0, -9.8)` in the dat. There is no global gravity on this path.
#[must_use]
pub fn update_particle(
    p: &Particle,
    t: f32,
    kind: ParticleType,
    parent: &Frame,
) -> ParticleDrawState {
    let o = parent.origin;
    let mut frame = *parent;
    let origin = match kind {
        ParticleType::Still | ParticleType::Unknown => o.add(p.offset),
        ParticleType::LocalVelocity | ParticleType::GlobalVelocity => {
            o.add(p.offset).add(p.a.mul(t))
        }
        ParticleType::ParabolicLvga | ParticleType::ParabolicLvla | ParticleType::ParabolicGvga => {
            o.add(p.offset).add(p.a.mul(t)).add(p.b.mul(0.5 * t * t))
        }
        ParticleType::ParabolicLvgaGr
        | ParticleType::ParabolicLvlaLr
        | ParticleType::ParabolicGvgaGr => {
            let origin = o.add(p.offset).add(p.a.mul(t)).add(p.b.mul(0.5 * t * t));
            frame.origin = origin;
            crate::sky::grotate(&mut frame, p.c.mul(t));
            frame.origin
        }
        ParticleType::Swarm => Vec3::new(
            o.x + p.offset.x + p.a.x * t + p.c.x * math::cosf(p.b.x * t),
            o.y + p.offset.y + p.a.y * t + p.c.y * math::sinf(p.b.y * t),
            o.z + p.offset.z + p.a.z * t + p.c.z * math::cosf(p.b.z * t),
        ),
        ParticleType::Explode => Vec3::new(
            // NOTE `a.x` on all three axes, and the extra `a.z` on z only. Shipped as written.
            o.x + p.offset.x + (p.c.x * p.a.x + p.b.x * t) * t,
            o.y + p.offset.y + (p.c.y * p.a.x + p.b.y * t) * t,
            o.z + p.offset.z + (p.c.z * p.a.x + p.b.z * t + p.a.z) * t,
        ),
        ParticleType::Implode => Vec3::new(
            // NOTE `cos(a.x * t)` on all three axes. Shipped as written.
            o.x + p.offset.x + p.b.x * t * t + p.c.x * math::cosf(p.a.x * t),
            o.y + p.offset.y + p.b.y * t * t + p.c.y * math::cosf(p.a.x * t),
            o.z + p.offset.z + p.b.z * t * t + p.c.z * math::cosf(p.a.x * t),
        ),
    };
    if !kind.rotates() {
        frame.origin = origin;
    }

    let f = if t < p.lifespan { t / p.lifespan } else { 1.0 };
    let scale = p.start_scale + (p.final_scale - p.start_scale) * f;
    let translucency = p.start_trans + (p.final_trans - p.start_trans) * f;
    ParticleDrawState {
        frame,
        scale,
        translucency,
        no_draw: translucency == 1.0,
    }
}

/// The kill condition: `lifespan <= lifetime`.
#[must_use]
pub fn should_kill(p: &Particle) -> bool {
    p.lifespan <= p.lifetime
}

/// The default `degrade_distance` when the particle gfxobj has no degrade info:
/// Particle parts use a maximum degradation distance of `100.0`.
pub const DEFAULT_PARTICLE_DEGRADE_DISTANCE: f32 = 100.0;

/// Apply the particle distance cutoff.
///
/// ```text
/// if examination_object: return true          // the preview panel always shows them
/// return cypt <= d and cell is present and cell's in-view test != OUTSIDE
/// ```
///
/// Beyond `d`, or when the emitter's cell is culled, the whole emitter is set to no-draw — the
/// particles keep simulating (a looping emitter has its ages reset so it does not pop on return)
/// but nothing is drawn. It is a **draw** cut-off, not a simulation one.
#[must_use]
pub fn should_draw_particles(
    examination_object: bool,
    cypt: f32,
    degrade_distance: f32,
    cell_in_view: Option<crate::cells::cull::Bounding>,
) -> bool {
    if examination_object {
        return true;
    }
    cypt <= degrade_distance
        && cell_in_view.is_some_and(|b| b != crate::cells::cull::Bounding::Outside)
}

/// Particle-distance threshold from adaptive degradation: `(16 + 8*deg_mul)²` for
/// `deg_mul >= 0`, `(16 + 9*deg_mul)²` below.
///
/// Inside that squared horizontal distance every particle part measures its own distance and
/// heading, so billboards face the camera individually; outside it, all parts share the object's.
/// **It is not a cull.**
#[must_use]
pub fn particle_distance_2dsq(deg_mul: f32) -> f32 {
    let d = deg_mul.clamp(-1.0, 1.0);
    let r = if d >= 0.0 {
        16.0 + 8.0 * d
    } else {
        16.0 + 9.0 * d
    };
    r * r
}

/// The order in which draws from the PRNG:
/// **lifespan, finalTrans, startTrans, finalScale, startScale, C, B, A, offset** — and
/// [`get_random_offset`] itself draws 3 or 4 more, in the component order **z, y, x**.
///
/// This is a strong test of `ran2`'s draw order and will fail loudly if the shared PRNG is wrong.
/// The names are here so a test can assert the sequence without reimplementing the emitter.
pub const EMIT_DRAW_ORDER: [&str; 9] = [
    "lifespan",
    "final_trans",
    "start_trans",
    "final_scale",
    "start_scale",
    "c",
    "b",
    "a",
    "offset",
];

/// Choose a random point on the disc perpendicular to
/// `offset_dir`.
///
/// **The order of the three rolls matters**: the client draws them `z`, `y`, `x` — the first
/// `-1..1` dice roll becomes the *z* component, the second the *y*, the third the *x*. A
/// rebuild that draws x, y, z diverges from the original PRNG stream immediately.
///
/// `rolls` is `[first, second, third]` from the `-1..1` generator and `len_roll` the `0..1` draw
/// that follows. Returns `(0,0,0)` when `r` was parallel to `offset_dir`.
#[must_use]
pub fn get_random_offset(
    rolls: [f32; 3],
    len_roll: f32,
    offset_dir: Vec3,
    min_offset: f32,
    max_offset: f32,
) -> Vec3 {
    // z, y, x -- in that order.
    let r = Vec3::new(rolls[2], rolls[1], rolls[0]);
    let d = r.dot(offset_dir);
    let mut v = r.sub(offset_dir.mul(d));
    if v.normalize_check_small() {
        return Vec3::ZERO;
    }
    v.mul(min_offset + len_roll * (max_offset - min_offset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::Quat;

    fn particle() -> Particle {
        Particle {
            offset: Vec3::new(1.0, 2.0, 3.0),
            a: Vec3::new(0.5, 0.25, 0.125),
            b: Vec3::new(0.0, 0.0, -9.8),
            c: Vec3::new(2.0, 3.0, 4.0),
            start_scale: 1.0,
            final_scale: 3.0,
            start_trans: 0.0,
            final_trans: 1.0,
            lifespan: 4.0,
            lifetime: 0.0,
        }
    }

    /// Oracle: the particle-type switch, evaluated by hand for each closed form.
    #[test]
    fn the_closed_forms_match_the_transcribed_equations() {
        let p = particle();
        let o = Frame::new(Vec3::new(10.0, 20.0, 30.0), Quat::IDENTITY);
        let t = 2.0f32;

        let s = update_particle(&p, t, ParticleType::Still, &o);
        assert_eq!(
            s.frame.origin,
            Vec3::new(11.0, 22.0, 33.0),
            "o + offset, no time term"
        );

        let s = update_particle(&p, t, ParticleType::LocalVelocity, &o);
        assert_eq!(
            s.frame.origin,
            Vec3::new(12.0, 22.5, 33.25),
            "o + offset + a*t"
        );

        let s = update_particle(&p, t, ParticleType::ParabolicLvga, &o);
        // z: 30 + 3 + 0.125*2 + 0.5*(-9.8)*4 = 33.25 - 19.6 = 13.65
        assert!(
            (s.frame.origin.z - 13.65).abs() < 1e-4,
            "{:?}",
            s.frame.origin
        );
        assert!((s.frame.origin.x - 12.0).abs() < 1e-4);

        let s = update_particle(&p, t, ParticleType::Swarm, &o);
        let want = Vec3::new(
            10.0 + 1.0 + 0.5 * 2.0 + 2.0 * math::cosf(0.0f32),
            20.0 + 2.0 + 0.25 * 2.0 + 3.0 * math::sinf(0.0f32),
            30.0 + 3.0 + 0.125 * 2.0 + 4.0 * math::cosf(-19.6f32),
        );
        assert!(
            (s.frame.origin.x - want.x).abs() < 1e-4,
            "{:?} vs {want:?}",
            s.frame.origin
        );
        assert!((s.frame.origin.y - want.y).abs() < 1e-4);
        assert!((s.frame.origin.z - want.z).abs() < 1e-4);
    }

    /// Oracle: the three particle-update oddities reproduced exactly.
    ///
    /// `Explode` uses **`a.x` on all three axes** in the `c·a` term and adds `a.z` only to z;
    /// `Implode` uses **`cos(a.x * t)` on all three axes**. Both look like original bugs and both
    /// ship. A test that "fixes" them by using `a.y`/`a.z` would produce a visibly different burst.
    #[test]
    fn the_explode_and_implode_oddities_are_reproduced_verbatim() {
        let mut p = particle();
        // Make the three components of `a` clearly distinct so using the wrong one shows.
        p.a = Vec3::new(1.0, 100.0, 10_000.0);
        p.b = Vec3::ZERO;
        p.c = Vec3::new(1.0, 1.0, 1.0);
        p.offset = Vec3::ZERO;
        let o = Frame::default();
        let t = 1.0f32;

        let s = update_particle(&p, t, ParticleType::Explode, &o);
        // x: (c.x*a.x + b.x*t)*t = 1*1 = 1
        // y: (c.y*a.x + b.y*t)*t = 1*1 = 1     <- a.x, NOT a.y
        // z: (c.z*a.x + b.z*t + a.z)*t = 1 + 10000 = 10001
        assert_eq!(s.frame.origin, Vec3::new(1.0, 1.0, 10_001.0));
        assert_ne!(
            s.frame.origin.y, 100.0,
            "using a.y here would be the 'fixed' version"
        );

        // Implode: cos(a.x * t) everywhere. a.x = 1, so all three cosines are cos(1).
        let s = update_particle(&p, t, ParticleType::Implode, &o);
        let cos1 = math::cosf(1.0f32);
        assert!((s.frame.origin.x - cos1).abs() < 1e-6);
        assert!(
            (s.frame.origin.y - cos1).abs() < 1e-6,
            "cos(a.x*t), not cos(a.y*t)"
        );
        assert!((s.frame.origin.z - cos1).abs() < 1e-6);
    }

    /// Oracle: the particle-update tail — "the only per-particle appearance channels are **uniform
    /// scale** and **translucency**, both linear in normalised age", and
    /// sets NoDraw at exactly 1.0 so a particle fading to
    /// `final_trans = 1.0` disappears cleanly on its last frame.
    #[test]
    fn scale_and_translucency_are_linear_in_age_and_one_means_nodraw() {
        let p = particle();
        let o = Frame::default();
        let at = |t: f32| update_particle(&p, t, ParticleType::Still, &o);
        assert_eq!(at(0.0).scale, 1.0);
        assert_eq!(at(2.0).scale, 2.0, "halfway through a 4 s life");
        assert_eq!(at(4.0).scale, 3.0);
        assert_eq!(
            at(99.0).scale,
            3.0,
            "the age clamps at 1.0, it does not extrapolate"
        );
        assert_eq!(at(0.0).translucency, 0.0);
        assert!(!at(0.0).no_draw);
        assert_eq!(at(4.0).translucency, 1.0);
        assert!(at(4.0).no_draw, "translucency exactly 1.0 sets NoDraw");
    }

    /// Oracle: the distance cut-off applies to **drawing**, and the
    /// examination-object branch overrides everything.
    #[test]
    fn the_distance_cut_off_is_a_draw_gate_not_a_simulation_gate() {
        use crate::cells::cull::Bounding;
        let d = DEFAULT_PARTICLE_DEGRADE_DISTANCE;
        assert_eq!(d, 100.0);
        assert!(should_draw_particles(
            false,
            50.0,
            d,
            Some(Bounding::PartiallyInside)
        ));
        assert!(!should_draw_particles(
            false,
            150.0,
            d,
            Some(Bounding::PartiallyInside)
        ));
        assert!(
            !should_draw_particles(false, 10.0, d, Some(Bounding::Outside)),
            "culled cell"
        );
        assert!(
            !should_draw_particles(false, 10.0, d, None),
            "no cell at all"
        );
        assert!(
            should_draw_particles(true, 10_000.0, d, None),
            "the preview panel always shows them"
        );
        // The boundary is inclusive.
        assert!(should_draw_particles(
            false,
            100.0,
            d,
            Some(Bounding::EntirelyInside)
        ));
    }

    /// Oracle: the formula is `(16 + 8*deg_mul)²` for `deg_mul >= 0` and
    /// `(16 + 9*deg_mul)²` below, and it is **not a cull**: it only chooses how distances are
    /// measured.
    #[test]
    fn the_particle_measurement_radius_follows_the_degrade_multiplier() {
        assert!((particle_distance_2dsq(0.0) - 256.0).abs() < 1e-3);
        assert!((particle_distance_2dsq(1.0) - 24.0 * 24.0).abs() < 1e-3);
        assert!((particle_distance_2dsq(-1.0) - 7.0 * 7.0).abs() < 1e-3);
        assert_eq!(particle_distance_2dsq(crate::consts::PINNED_DEG_MUL), 256.0);
    }

    /// The offset rolls land on z then y then x.
    #[test]
    fn the_offset_rolls_land_on_z_then_y_then_x() {
        // Three clearly distinct rolls, and an offset_dir that keeps all of them.
        let rolls = [0.1f32, 0.5, 0.9];
        let dir = Vec3::new(0.0, 0.0, 1.0);
        let v = get_random_offset(rolls, 0.0, dir, 7.0, 7.0);
        // r = (third, second, first) = (0.9, 0.5, 0.1); projecting out the z component leaves
        // (0.9, 0.5, 0) normalised, times the length 7.
        let n = (0.9f32 * 0.9 + 0.5 * 0.5).sqrt();
        assert!((v.x - 7.0 * 0.9 / n).abs() < 1e-4, "{v:?}");
        assert!((v.y - 7.0 * 0.5 / n).abs() < 1e-4);
        assert!(
            v.z.abs() < 1e-6,
            "the component along offset_dir is projected out"
        );
        // The x,y,z order would put 0.1 on x, which is a visibly different direction.
        let wrong = Vec3::new(rolls[0], rolls[1], rolls[2]);
        assert!((v.x / v.y - 0.9 / 0.5).abs() < 1e-4);
        assert!((wrong.x / wrong.y - 0.9 / 0.5).abs() > 0.1);
        // A roll parallel to offset_dir degenerates to zero rather than exploding.
        assert_eq!(
            get_random_offset([1.0, 0.0, 0.0], 0.5, dir, 1.0, 2.0),
            Vec3::ZERO
        );
    }

    /// Oracle: the PRNG call order is **lifespan, finalTrans,
    /// startTrans, finalScale, startScale, C, B, A, offset**. Pinned as data so a future emitter
    /// implementation is written against the order rather than against intuition.
    #[test]
    fn the_emit_draw_order_is_pinned() {
        assert_eq!(
            EMIT_DRAW_ORDER,
            [
                "lifespan",
                "final_trans",
                "start_trans",
                "final_scale",
                "start_scale",
                "c",
                "b",
                "a",
                "offset"
            ]
        );
        assert_eq!(
            EMIT_DRAW_ORDER.len(),
            9,
            "nine draws before get_random_offset's own 3 or 4"
        );
    }

    /// Oracle: the particle kill check's death condition is
    /// `lifespan <= lifetime`, so a particle whose age has exactly reached its lifespan dies.
    #[test]
    fn a_particle_dies_when_its_age_reaches_its_lifespan() {
        let mut p = particle();
        p.lifetime = 3.999;
        assert!(!should_kill(&p));
        p.lifetime = 4.0;
        assert!(should_kill(&p), "the comparison is <=, so equality kills");
    }

    /// Oracle: the `*GR`/`*LR` types set the **whole frame**,
    /// rotating it by `c * t`; every other type only moves the origin and leaves the rotation alone.
    #[test]
    fn only_the_rotating_types_touch_the_frames_rotation() {
        let mut p = particle();
        p.c = Vec3::new(0.0, 0.0, 1.0);
        let o = Frame::new(Vec3::ZERO, Quat::IDENTITY);
        for k in [
            ParticleType::Still,
            ParticleType::Swarm,
            ParticleType::ParabolicLvga,
        ] {
            assert_eq!(
                update_particle(&p, 1.0, k, &o).frame.rotation,
                Quat::IDENTITY,
                "{k:?}"
            );
            assert!(!k.rotates());
        }
        for k in [
            ParticleType::ParabolicLvgaGr,
            ParticleType::ParabolicLvlaLr,
            ParticleType::ParabolicGvgaGr,
        ] {
            assert!(k.rotates());
            assert_ne!(
                update_particle(&p, 1.0, k, &o).frame.rotation,
                Quat::IDENTITY,
                "{k:?}"
            );
        }
    }
}
