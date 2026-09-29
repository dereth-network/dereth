// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Physics/Trajectory.cs
//! Port of `Source/ACE.Server/Physics/Trajectory.cs`.
//!
//! ACE's copy of Forrest Smith's ballistic-trajectory solvers (public domain, see the C# header),
//! with the Graphics Gems root solvers. Pure arithmetic in ACE's types: `float` where the C# has
//! `float` and `double` where it has `double`, in the same order. The `out` parameters are the
//! members of each result struct; every output keeps the value the C# leaves in it, including on
//! the early returns. Expected values come from ACE's compiled code (`physics/trajectory_*` vectors).

use std::cmp::Ordering;

use empyrean_common::dotnet::math::min_f32;
use empyrean_common::dotnet::Vector3;

/// `PhysicsGlobals.Gravity`.
// ACE: PhysicsGlobals.Gravity
pub const GRAVITY: f32 = -9.8;

/// `Vector3.UnitZ`.
const UNIT_Z: Vector3 = Vector3::new(0.0, 0.0, 1.0);

/// `Vector3.Dot(a, b)`, summed in the order `LengthSquared` sums (checked by the vectors).
fn dot(a: Vector3, b: Vector3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

/// Utility function used by SolveQuadratic, SolveCubic, and SolveQuartic.
// ACE: Trajectory.IsZero
#[must_use]
pub fn is_zero(d: f64) -> bool {
    const EPS: f64 = 1e-9;
    d > -EPS && d < EPS
}

/// `SolveQuadric`'s result: the number of solutions and the two `out` values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quadric {
    pub num: i32,
    pub s0: f64,
    pub s1: f64,
}

/// Solve quadratic equation: c0*x^2 + c1*x + c2. Returns number of solutions.
// ACE: Trajectory.SolveQuadric
#[must_use]
pub fn solve_quadric(c0: f64, c1: f64, c2: f64) -> Quadric {
    let mut r = Quadric {
        num: 0,
        s0: f64::NAN,
        s1: f64::NAN,
    };

    // normal form: x^2 + px + q = 0
    let p = c1 / (2.0 * c0);
    let q = c2 / c0;

    let d = p * p - q;

    if is_zero(d) {
        r.s0 = -p;
        r.num = 1;
    } else if d < 0.0 {
        r.num = 0;
    } else {
        let sqrt_d = d.sqrt();

        r.s0 = sqrt_d - p;
        r.s1 = -sqrt_d - p;
        r.num = 2;
    }
    r
}

/// `SolveCubic`'s result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cubic {
    pub num: i32,
    pub s0: f64,
    pub s1: f64,
    pub s2: f64,
}

/// Solve cubic equation: c0*x^3 + c1*x^2 + c2*x + c3. Returns number of solutions.
// ACE: Trajectory.SolveCubic
#[must_use]
pub fn solve_cubic(c0: f64, c1: f64, c2: f64, c3: f64) -> Cubic {
    let mut s0;
    let mut s1 = f64::NAN;
    let mut s2 = f64::NAN;

    let num;

    // normal form: x^3 + Ax^2 + Bx + C = 0
    let a = c1 / c0;
    let b = c2 / c0;
    let c = c3 / c0;

    // substitute x = y - A/3 to eliminate quadric term:  x^3 +px + q = 0
    let sq_a = a * a;
    let p = 1.0 / 3.0 * (-1.0 / 3.0 * sq_a + b);
    let q = 1.0 / 2.0 * (2.0 / 27.0 * a * sq_a - 1.0 / 3.0 * a * b + c);

    // use Cardano's formula
    let cb_p = p * p * p;
    let d = q * q + cb_p;

    if is_zero(d) {
        if is_zero(q) {
            // one triple solution
            s0 = 0.0;
            num = 1;
        } else {
            // one single and one double solution
            let u = empyrean_common::math::pow(-q, 1.0 / 3.0);
            s0 = 2.0 * u;
            s1 = -u;
            num = 2;
        }
    } else if d < 0.0 {
        // Casus irreducibilis: three real solutions
        let phi = 1.0 / 3.0 * empyrean_common::math::acos(-q / (-cb_p).sqrt());
        let t = 2.0 * (-p).sqrt();

        s0 = t * empyrean_common::math::cos(phi);
        s1 = -t * empyrean_common::math::cos(phi + std::f64::consts::PI / 3.0);
        s2 = -t * empyrean_common::math::cos(phi - std::f64::consts::PI / 3.0);
        num = 3;
    } else {
        // one real solution
        let sqrt_d = d.sqrt();
        let u = empyrean_common::math::pow(sqrt_d - q, 1.0 / 3.0);
        let v = -empyrean_common::math::pow(sqrt_d + q, 1.0 / 3.0);

        s0 = u + v;
        num = 1;
    }

    // resubstitute
    let sub = 1.0 / 3.0 * a;

    if num > 0 {
        s0 -= sub;
    }
    if num > 1 {
        s1 -= sub;
    }
    if num > 2 {
        s2 -= sub;
    }

    Cubic { num, s0, s1, s2 }
}

/// `SolveQuartic`'s result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quartic {
    pub num: i32,
    pub s0: f64,
    pub s1: f64,
    pub s2: f64,
    pub s3: f64,
}

/// Solve quartic function: c0*x^4 + c1*x^3 + c2*x^2 + c3*x + c4. Returns number of solutions.
// ACE: Trajectory.SolveQuartic
#[must_use]
pub fn solve_quartic(c0: f64, c1: f64, c2: f64, c3: f64, c4: f64) -> Quartic {
    let mut s0;
    let mut s1;
    let mut s2;
    let mut s3 = f64::NAN;

    let mut coeffs = [0.0_f64; 4];
    let mut num;

    // normal form: x^4 + Ax^3 + Bx^2 + Cx + D = 0
    let a = c1 / c0;
    let b = c2 / c0;
    let c = c3 / c0;
    let d = c4 / c0;

    // substitute x = y - A/4 to eliminate cubic term: x^4 + px^2 + qx + r = 0
    let sq_a = a * a;
    let p = -3.0 / 8.0 * sq_a + b;
    let q = 1.0 / 8.0 * sq_a * a - 1.0 / 2.0 * a * b + c;
    let r = -3.0 / 256.0 * sq_a * sq_a + 1.0 / 16.0 * sq_a * b - 1.0 / 4.0 * a * c + d;

    if is_zero(r) {
        // no absolute term: y(y^3 + py + q) = 0

        coeffs[3] = q;
        coeffs[2] = p;
        coeffs[1] = 0.0;
        coeffs[0] = 1.0;

        let cu = solve_cubic(coeffs[0], coeffs[1], coeffs[2], coeffs[3]);
        num = cu.num;
        s0 = cu.s0;
        s1 = cu.s1;
        s2 = cu.s2;
    } else {
        // solve the resolvent cubic ...
        coeffs[3] = 1.0 / 2.0 * r * p - 1.0 / 8.0 * q * q;
        coeffs[2] = -r;
        coeffs[1] = -1.0 / 2.0 * p;
        coeffs[0] = 1.0;

        let cu = solve_cubic(coeffs[0], coeffs[1], coeffs[2], coeffs[3]);
        s0 = cu.s0;
        s1 = cu.s1;
        s2 = cu.s2;

        // ... and take the one real solution ...
        let z = s0;

        // ... to build two quadric equations
        let mut u = z * z - r;
        let mut v = 2.0 * z - p;

        if is_zero(u) {
            u = 0.0;
        } else if u > 0.0 {
            u = u.sqrt();
        } else {
            return Quartic {
                num: 0,
                s0,
                s1,
                s2,
                s3,
            };
        }

        if is_zero(v) {
            v = 0.0;
        } else if v > 0.0 {
            v = v.sqrt();
        } else {
            return Quartic {
                num: 0,
                s0,
                s1,
                s2,
                s3,
            };
        }

        coeffs[2] = z - u;
        coeffs[1] = if q < 0.0 { -v } else { v };
        coeffs[0] = 1.0;

        let qd = solve_quadric(coeffs[0], coeffs[1], coeffs[2]);
        num = qd.num;
        s0 = qd.s0;
        s1 = qd.s1;

        coeffs[2] = z + u;
        coeffs[1] = if q < 0.0 { v } else { -v };
        coeffs[0] = 1.0;

        if num == 0 {
            let qd = solve_quadric(coeffs[0], coeffs[1], coeffs[2]);
            num += qd.num;
            s0 = qd.s0;
            s1 = qd.s1;
        }
        if num == 1 {
            let qd = solve_quadric(coeffs[0], coeffs[1], coeffs[2]);
            num += qd.num;
            s1 = qd.s0;
            s2 = qd.s1;
        }
        if num == 2 {
            let qd = solve_quadric(coeffs[0], coeffs[1], coeffs[2]);
            num += qd.num;
            s2 = qd.s0;
            s3 = qd.s1;
        }
    }

    // resubstitute
    let sub = 1.0 / 4.0 * a;

    if num > 0 {
        s0 -= sub;
    }
    if num > 1 {
        s1 -= sub;
    }
    if num > 2 {
        s2 -= sub;
    }
    if num > 3 {
        s3 -= sub;
    }

    Quartic {
        num,
        s0,
        s1,
        s2,
        s3,
    }
}

/// Calculate the maximum range that a ballistic projectile can be fired on given speed and
/// gravity (positive is down), from `initial_height` above flat terrain.
// ACE: Trajectory.ballistic_range
#[must_use]
#[allow(clippy::cast_possible_truncation)] // C#'s (float) cast
pub fn ballistic_range(speed: f32, gravity: f32, initial_height: f32) -> f32 {
    if speed <= 0.0 || gravity <= 0.0 || initial_height < 0.0 {
        return 0.0;
    }

    let angle = 45.0 * 0.0174533_f64; // no air resistence, so 45 degrees provides maximum range
    let cos = empyrean_common::math::cos(angle);
    let sin = empyrean_common::math::sin(angle);

    // C#'s promotions: `speed * speed` and `2 * gravity * initial_height` stay float; each meets a
    // double only at the next operator.
    let speed_sq = f64::from(speed * speed);
    let lift = f64::from(2.0 * gravity * initial_height);
    let range = (f64::from(speed) * cos / f64::from(gravity))
        * (f64::from(speed) * sin + (speed_sq * sin * sin + lift).sqrt());
    range as f32
}

/// The first `solve_ballistic_arc` overload's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BallisticArc {
    pub num: i32,
    pub s0: Vector3,
    pub s1: Vector3,
    pub t0: f32,
    pub t1: f32,
}

/// Solve firing angles for a ballistic projectile with speed and gravity to hit a fixed position.
/// `s0` is the low-angle solution and `s1` the high-angle one; returns 0, 1 or 2 solutions.
// ACE: Trajectory.solve_ballistic_arc
#[must_use]
#[allow(clippy::cast_possible_truncation)] // C#'s (float) casts
pub fn solve_ballistic_arc(
    proj_pos: Vector3,
    proj_speed: f32,
    target: Vector3,
    gravity: f32,
) -> BallisticArc {
    // C# requires out variables be set
    let mut r = BallisticArc {
        num: 0,
        s0: Vector3::ZERO,
        s1: Vector3::ZERO,
        t0: f32::INFINITY,
        t1: f32::INFINITY,
    };

    #[allow(clippy::float_cmp)] // C#'s Vector3 ==
    if proj_pos == target || proj_speed <= 0.0 || gravity <= 0.0 {
        return r;
    }

    let diff = target - proj_pos;
    let diff_xy = Vector3::new(diff.x, diff.y, 0.0);
    let ground_dist = diff_xy.length();

    let speed2 = proj_speed * proj_speed;
    let speed4 = proj_speed * proj_speed * proj_speed * proj_speed;
    let z = diff.z;
    let x = ground_dist;
    let gx = gravity * x;

    let mut root = speed4 - gravity * (gravity * x * x + 2.0 * z * speed2);

    // No solution
    if root < 0.0 {
        return r;
    }

    root = f64::from(root).sqrt() as f32;

    let low_ang = empyrean_common::math::atan2(f64::from(speed2 - root), f64::from(gx));
    let high_ang = empyrean_common::math::atan2(f64::from(speed2 + root), f64::from(gx));
    #[allow(clippy::float_cmp)]
    let num_solutions = if low_ang != high_ang { 2 } else { 1 };

    let ground_dir = Vector3::normalize(diff_xy);
    r.s0 = ground_dir * (empyrean_common::math::cos(low_ang) as f32) * proj_speed
        + UNIT_Z * (empyrean_common::math::sin(low_ang) as f32) * proj_speed;
    if num_solutions > 1 {
        r.s1 = ground_dir * (empyrean_common::math::cos(high_ang) as f32) * proj_speed
            + UNIT_Z * (empyrean_common::math::sin(high_ang) as f32) * proj_speed;
    }

    r.t0 = x / ((empyrean_common::math::cos(low_ang) as f32) * proj_speed);
    r.t1 = x / ((empyrean_common::math::cos(high_ang) as f32) * proj_speed);

    r.num = num_solutions;
    r
}

/// The second `solve_ballistic_arc` overload's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovingBallisticArc {
    pub num: i32,
    pub s0: Vector3,
    pub s1: Vector3,
    pub time: f32,
}

/// `Array.Sort(double[])`: `double.CompareTo`, which orders NaN before every number.
fn sort_doubles(v: &mut [f64]) {
    v.sort_by(|a, b| match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
    });
}

/// Solve firing angles for a ballistic projectile with speed and gravity to hit a target moving
/// with constant, linear velocity. Returns the number of solutions (at most 2 are written).
// ACE: Trajectory.solve_ballistic_arc
///
/// Not ACE's (a fix, V317): only the quartic's real roots are sorted and tried.
/// ACE sorted all four slots, whose NaN placeholders order first, so with fewer than four roots a
/// NaN was read in place of a real root and returned as the solution (a NaN launch velocity, e.g.
/// a monster's arrow at a player jumping or falling below it).
#[must_use]
#[allow(clippy::many_single_char_names, clippy::cast_possible_truncation)] // ACE's names; (float) casts
pub fn solve_ballistic_arc_moving(
    proj_pos: Vector3,
    proj_speed: f32,
    target_pos: Vector3,
    target_velocity: Vector3,
    gravity: f32,
) -> MovingBallisticArc {
    // Initialize output parameters
    let mut out = MovingBallisticArc {
        num: 0,
        s0: Vector3::ZERO,
        s1: Vector3::ZERO,
        time: 0.0,
    };

    let g = f64::from(gravity);

    let a = f64::from(proj_pos.x);
    let b = f64::from(proj_pos.z);
    let c = f64::from(proj_pos.y);
    let m = f64::from(target_pos.x);
    let n = f64::from(target_pos.z);
    let o = f64::from(target_pos.y);
    let p = f64::from(target_velocity.x);
    let q = f64::from(target_velocity.z);
    let r = f64::from(target_velocity.y);
    let s = f64::from(proj_speed);

    let h = m - a;
    let j = o - c;
    let k = n - b;
    let l = f64::from(-0.5_f32) * g;

    // Quartic Coeffecients
    let c0 = l * l;
    let c1 = 2.0 * q * l;
    let c2 = q * q + 2.0 * k * l - s * s + p * p + r * r;
    let c3 = 2.0 * k * q + 2.0 * h * p + 2.0 * j * r;
    let c4 = k * k + h * h + j * j;

    // Solve quartic
    let qr = solve_quartic(c0, c1, c2, c3, c4);
    let mut times = [qr.s0, qr.s1, qr.s2, qr.s3];
    let num_times = qr.num;

    // Sort so faster collision is found first
    sort_doubles(&mut times[..usize::try_from(num_times).unwrap_or(0).min(4)]);

    // Plug quartic solutions into base equations
    // There should never be more than 2 positive, real roots.
    let mut solutions = [Vector3::ZERO; 2];
    let mut num_solutions = 0_usize;

    out.time = 0.0;
    let mut i = 0_usize;
    while i32::try_from(i).unwrap_or(i32::MAX) < num_times && num_solutions < 2 {
        let t = times[i];
        i += 1;
        if t <= 0.0 {
            continue;
        }

        if num_solutions == 0 {
            out.time = t as f32;
        }

        solutions[num_solutions].x = ((h + p * t) / t) as f32;
        solutions[num_solutions].y = ((j + r * t) / t) as f32;
        solutions[num_solutions].z = ((k + q * t - l * t * t) / t) as f32;
        num_solutions += 1;
    }

    // Write out solutions
    if num_solutions > 0 {
        out.s0 = solutions[0];
    }
    if num_solutions > 1 {
        out.s1 = solutions[1];
    }

    out.num = i32::try_from(num_solutions).unwrap_or(0);
    out
}

/// `SolveBallisticArc`'s result: the success flag and the two `out` values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedGravityArc {
    pub ok: bool,
    pub velocity_vector: Vector3,
    pub time: f32,
}

/// Solve for a firing arc with a fixed gravity (`PhysicsGlobals.Gravity`), adapted for ACE by
/// gmriggs from the original.
// ACE: Trajectory.SolveBallisticArc
#[must_use]
pub fn solve_ballistic_arc_fixed(
    projectile_position: Vector3,
    lateral_speed: f32,
    target_position: Vector3,
) -> FixedGravityArc {
    let mut out = FixedGravityArc {
        ok: false,
        velocity_vector: Vector3::ZERO,
        time: f32::NAN,
    };

    #[allow(clippy::float_cmp)]
    if projectile_position == target_position || lateral_speed <= 0.0 {
        return out;
    }

    let diff = target_position - projectile_position;
    let diff_xy = Vector3::new(diff.x, diff.y, 0.0);
    let lateral_dist = diff_xy.length();

    #[allow(clippy::float_cmp)]
    if lateral_dist == 0.0 {
        return out;
    }

    let time = lateral_dist / lateral_speed;
    out.time = time;

    out.velocity_vector = Vector3::normalize(diff_xy) * lateral_speed;

    // System of equations. Hit max_height at t=.5*time. Hit target at t=time.
    let a = projectile_position.z; // initial
    let c = target_position.z; // final

    // Gravity value pulled from ACE property
    let g = GRAVITY;
    // ACE computes the peak `b = (4 * a + 4 * c - g * time * time) / 8` and never reads it.

    out.velocity_vector.z = -((2.0 * a - 2.0 * c + g * time * time) / (time * 2.0)); // C#: `... / (time * 2) * -1`

    out.ok = true;
    out
}

/// The first `solve_ballistic_arc_lateral` overload's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LateralArc {
    pub ok: bool,
    pub fire_velocity: Vector3,
    pub gravity: f32,
}

/// Solve the firing arc with a fixed lateral speed; vertical speed and gravity vary so the
/// projectile peaks at `max_height`. ACE's `Debug.Assert` is compiled out of its Release build.
// ACE: Trajectory.solve_ballistic_arc_lateral
#[must_use]
pub fn solve_ballistic_arc_lateral(
    proj_pos: Vector3,
    lateral_speed: f32,
    target_pos: Vector3,
    max_height: f32,
) -> LateralArc {
    let mut out = LateralArc {
        ok: false,
        fire_velocity: Vector3::ZERO,
        gravity: f32::NAN,
    };

    #[allow(clippy::float_cmp)]
    if proj_pos == target_pos || lateral_speed <= 0.0 || max_height <= proj_pos.z {
        return out;
    }

    let diff = target_pos - proj_pos;
    let diff_xy = Vector3::new(diff.x, diff.y, 0.0);
    let lateral_dist = diff_xy.length();

    #[allow(clippy::float_cmp)]
    if lateral_dist == 0.0 {
        return out;
    }

    let time = lateral_dist / lateral_speed;

    out.fire_velocity = Vector3::normalize(diff_xy) * lateral_speed;

    let a = proj_pos.z; // initial
    let b = max_height; // peak
    let c = target_pos.z; // final

    out.gravity = -4.0 * (a - 2.0 * b + c) / (time * time);
    out.fire_velocity.z = -(3.0 * a - 4.0 * b + c) / time;

    out.ok = true;
    out
}

/// The second `solve_ballistic_arc_lateral` overload's result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovingLateralArc {
    pub ok: bool,
    pub fire_velocity: Vector3,
    pub time: f32,
    pub impact_point: Vector3,
}

/// Solve the firing arc with a fixed lateral speed and gravity at a target moving with constant,
/// linear velocity.
// ACE: Trajectory.solve_ballistic_arc_lateral
#[must_use]
#[allow(clippy::cast_possible_truncation)] // C#'s (float) casts
pub fn solve_ballistic_arc_lateral_moving(
    proj_pos: Vector3,
    lateral_speed: f32,
    target: Vector3,
    target_velocity: Vector3,
    gravity: f32,
) -> MovingLateralArc {
    // Initialize output variables
    let mut out = MovingLateralArc {
        ok: false,
        fire_velocity: Vector3::ZERO,
        time: 0.0,
        impact_point: Vector3::ZERO,
    };

    #[allow(clippy::float_cmp)]
    if proj_pos == target || lateral_speed <= 0.0 {
        return out;
    }

    // Ground plane terms
    let target_vel_xy = Vector3::new(target_velocity.x, target_velocity.y, 0.0);
    let mut diff_xy = target - proj_pos;
    diff_xy.z = 0.0;

    let c0 = dot(target_vel_xy, target_vel_xy) - lateral_speed * lateral_speed;
    let c1 = 2.0 * dot(diff_xy, target_vel_xy);
    let c2 = dot(diff_xy, diff_xy);
    let qd = solve_quadric(f64::from(c0), f64::from(c1), f64::from(c2));
    let (n, t0, t1) = (qd.num, qd.s0, qd.s1);

    // pick smallest, positive time
    let valid0 = n > 0 && t0 > 0.0;
    let valid1 = n > 1 && t1 > 0.0;

    let t = if !valid0 && !valid1 {
        return out;
    } else if valid0 && valid1 {
        min_f32(t0 as f32, t1 as f32)
    } else if valid0 {
        t0 as f32
    } else {
        t1 as f32
    };

    // Calculate impact point
    out.impact_point = target + (target_velocity * t);

    // Calculate fire velocity along XZ plane
    let dir = out.impact_point - proj_pos;
    out.fire_velocity = Vector3::normalize(Vector3::new(dir.x, dir.y, 0.0)) * lateral_speed;

    let a = proj_pos.z; // initial
    let c = out.impact_point.z; // final

    let g = gravity;
    // ACE computes the peak `b = (4 * a + 4 * c - g * t * t) / 8` and never reads it.

    out.fire_velocity.z = -((2.0 * a - 2.0 * c + g * t * t) / (t * 2.0)); // C#: `... / (t * 2) * -1`

    out.time = t;

    out.ok = true;
    out
}
