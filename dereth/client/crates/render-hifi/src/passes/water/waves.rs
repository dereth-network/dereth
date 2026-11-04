//! The waves on lakes and the sea: a few long waves laid over the ripples, each running at the
//! speed deep water carries a wave of its length. The first four are a lake's chop, running with
//! the wind. Then the sea's swell, which bends to meet the shore whichever way it faces: four
//! waves laid toward each of [`HEADINGS`] headings, of which the shader takes the two nearest the
//! way to the shore where it is drawn, and the wind's out past the shore's reach.
//!
//! Each wave's phase is worked out here in double precision, about where render space's origin
//! lies in the world, so the shader's arithmetic stays exact and the waves hold still while the
//! clock does.

use dereth_primitives::num::math::{atan2, cos, sin};
use glam::Vec2;

/// How many headings the swell is laid toward, evenly round from east.
pub const HEADINGS: usize = 8;
/// How many waves of chop, and of swell toward each heading.
pub const CHOP: usize = 4;
/// How many waves there are: the chop, then the swell toward each heading in turn.
pub const COUNT: usize = CHOP + 4 * HEADINGS;
/// The wind's heading the waves mostly run along, as east and north: the ripples' own.
const WIND: (f32, f32) = (0.82, 0.57);
/// How fast gravity pulls, meters a second a second.
const GRAVITY: f64 = 9.81;
/// How long the surf takes to roll in once, seconds.
pub const SURF_PERIOD: f64 = 7.0;
/// The period the shader's lattices repeat over, meters: where render space's origin lies in
/// the world is handed over wrapped to it.
pub const FOLD: f32 = 1000.0;

/// One wave: its length, meters; its turn from the wind, radians; and its steepness, the most
/// slope it gives the surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wave {
    /// Meters from crest to crest.
    pub length: f32,
    /// Its turn from the wind, radians.
    pub turn: f32,
    /// The most slope it gives the surface.
    pub steepness: f32,
}

/// The waves: a lake's chop, turned from the wind, then the sea's swell, turned from the heading it
/// is laid toward.
pub const WAVES: [Wave; 8] = [
    Wave {
        length: 8.5,
        turn: 0.0,
        steepness: 0.10,
    },
    Wave {
        length: 5.3,
        turn: 0.55,
        steepness: 0.09,
    },
    Wave {
        length: 3.4,
        turn: -0.7,
        steepness: 0.08,
    },
    Wave {
        length: 2.2,
        turn: 1.2,
        steepness: 0.06,
    },
    Wave {
        length: 26.0,
        turn: 0.1,
        steepness: 0.07,
    },
    Wave {
        length: 17.0,
        turn: -0.35,
        steepness: 0.07,
    },
    Wave {
        length: 11.5,
        turn: 0.45,
        steepness: 0.06,
    },
    Wave {
        length: 7.8,
        turn: -0.9,
        steepness: 0.05,
    },
];

/// The waves' constants at `clock` seconds, render space's origin lying at `global` in the world:
/// each wave's wave vector (radians a meter east and north), its steepness and its phase at the
/// origin (radians, 0..2π). The chop first, then four swell waves toward each heading.
#[must_use]
pub fn constants(clock: f64, global: Vec2) -> [[f32; 4]; COUNT] {
    let tau = std::f64::consts::TAU;
    let wind = atan2(f64::from(WIND.1), f64::from(WIND.0));
    let laid = WAVES[..CHOP]
        .iter()
        .map(|w| (wind, w))
        .chain((0..HEADINGS).flat_map(|h| {
            // LINT-OK: eight headings.
            #[allow(clippy::cast_precision_loss)]
            let toward = tau * h as f64 / HEADINGS as f64;
            WAVES[CHOP..].iter().map(move |w| (toward, w))
        }));
    let mut out = [[0.0; 4]; COUNT];
    for (i, ((base, w), o)) in laid.zip(&mut out).enumerate() {
        let heading = base + f64::from(w.turn);
        let k = tau / f64::from(w.length);
        let (kx, ky) = (k * cos(heading), k * sin(heading));
        let omega = (GRAVITY * k).sqrt();
        // LINT-OK: a few dozen waves.
        #[allow(clippy::cast_precision_loss)]
        let seed = i as f64 * 2.39;
        let phase = (kx * f64::from(global.x) + ky * f64::from(global.y) - omega * clock + seed)
            .rem_euclid(tau);
        // LINT-OK: radians, small.
        #[allow(clippy::cast_possible_truncation)]
        {
            *o = [kx as f32, ky as f32, w.steepness, phase as f32];
        }
    }
    out
}

/// How far the surf has rolled in at `clock` seconds, a share of its period, 0..1.
#[must_use]
pub fn surf(clock: f64) -> f32 {
    // LINT-OK: a share, 0..1.
    #[allow(clippy::cast_possible_truncation)]
    let s = (clock / SURF_PERIOD).rem_euclid(1.0) as f32;
    s
}

/// Where render space's origin lies in the world, wrapped to [`FOLD`].
#[must_use]
pub fn fold(global: Vec2) -> Vec2 {
    Vec2::new(global.x.rem_euclid(FOLD), global.y.rem_euclid(FOLD))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the water, an art-pack presentation's own)
    use super::*;

    #[test]
    fn each_wave_runs_with_the_clock_and_lies_still_where_the_world_does_whatever_the_origin() {
        let tau = std::f32::consts::TAU;
        let at = |c: &[[f32; 4]; COUNT], k: usize, p: Vec2| {
            (c[k][0] * p.x + c[k][1] * p.y + c[k][3]).rem_euclid(tau)
        };
        let global = Vec2::new(31_250.0, 18_000.0);
        let now = constants(5_000.25, global);
        assert_eq!(now, constants(5_000.25, global), "a held clock holds still");
        for w in &now {
            assert!((0.0..tau).contains(&w[3]), "{w:?}");
        }
        // The same point of the world, its origin moved: the same phase.
        let moved = constants(5_000.25, global + Vec2::new(24.0, -48.0));
        for k in 0..COUNT {
            let a = at(&now, k, Vec2::new(30.0, 10.0));
            let b = at(&moved, k, Vec2::new(6.0, 58.0));
            let d = (a - b).abs();
            assert!(d < 2e-3 || d > tau - 2e-3, "wave {k}: {a} {b}");
        }
        // A moment later every wave has moved on, the longer the faster.
        let later = constants(5_000.35, global);
        for k in 0..COUNT {
            assert!(now[k][3] != later[k][3], "wave {k}");
        }
        assert!(WAVES[4].length > WAVES[0].length && WAVES[0].length > WAVES[3].length);
        // Each heading's swell runs that way: toward the east first, then on round to the north.
        let along = |k: usize, x: f32, y: f32| now[k][0] * x + now[k][1] * y;
        assert!(along(CHOP, 1.0, 0.0) > 0.0 && along(CHOP + 4 * 2, 0.0, 1.0) > 0.0);
        assert!(along(CHOP + 4 * 4, -1.0, 0.0) > 0.0 && along(CHOP + 4 * 6, 0.0, -1.0) > 0.0);
    }

    #[test]
    fn the_surf_rolls_in_once_a_period_and_the_origin_folds_into_the_lattices_period() {
        assert!((surf(0.0) - surf(SURF_PERIOD * 40.0)).abs() < 1e-4);
        assert!((surf(SURF_PERIOD * 0.5) - 0.5).abs() < 1e-4);
        let f = fold(Vec2::new(31_250.5, -18.0));
        assert!(
            (f.x - 250.5).abs() < 1e-2 && (f.y - 982.0).abs() < 1e-2,
            "{f:?}"
        );
    }
}
