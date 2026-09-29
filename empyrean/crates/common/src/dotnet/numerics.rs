//! The `System.Numerics` subset ACE.Entity uses: `Vector2`, `Vector3` and `Quaternion`, all `f32`.
//!
//! Not ported from ACE: a clean-room model of the .NET runtime types as **net10** computes them,
//! following dotnet/runtime `release/10.0`
//! `src/libraries/System.Private.CoreLib/src/System/Numerics/{Vector3,Vector4,Quaternion}.cs` and
//! `System/Runtime/Intrinsics/VectorMath.cs`. The client crates' `Vec3`/`Quat` carry no operations
//! and store the quaternion `w` first, so ACE's math gets its own types here. The members are
//! checked bit for bit against net10 by the `entity/numerics_*` and `entity/position_*` vectors.
//!
//! * Arithmetic is plain `f32`, evaluated in the source's order; RyuJIT never contracts `a * b + c`.
//!   `MultiplyAddEstimate` is fused ([`f32::mul_add`], [`f64::mul_add`]), as on every x64 CPU with
//!   FMA. The vectors come out identical with AVX (and so FMA) disabled, so the choice does not
//!   show on them.
//! * net10 differs from the .NET 8 scalar formulas in three places: `Quaternion * Quaternion` is
//!   DirectXMath's `XMQuaternionMultiply` (another summation order), `Vector3.Transform(v, q)` is
//!   `q * (v, 1) * Conjugate(q)`, and `CreateFromYawPitchRoll` takes its sines and cosines from
//!   `Vector128.SinCos` (AMD's `sinf`/`cosf` polynomials, whose branch depends on every lane).
//! * `==` is IEEE equality (the C# `==` operator); [`Vector3::equals`] and
//!   [`Quaternion::equals`] are `Equals(other)`, which counts NaN equal to NaN.
//! * `Vector3 / float` divides each component (the .NET Core operator), it does not multiply by a
//!   reciprocal.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

/// .NET `Equals` on one `float` lane: IEEE equality, except that NaN equals NaN.
fn lane_equals(a: f32, b: f32) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

/// `System.Numerics.Vector2`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

impl Vector2 {
    /// `Vector2.One`.
    pub const ONE: Vector2 = Vector2 { x: 1.0, y: 1.0 };

    /// `new Vector2(x, y)`.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Vector2 { x, y }
    }
}

impl Add for Vector2 {
    type Output = Vector2;
    fn add(self, o: Vector2) -> Vector2 {
        Vector2::new(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for Vector2 {
    fn add_assign(&mut self, o: Vector2) {
        *self = *self + o;
    }
}

impl Mul<f32> for Vector2 {
    type Output = Vector2;
    fn mul(self, s: f32) -> Vector2 {
        Vector2::new(self.x * s, self.y * s)
    }
}

/// `System.Numerics.Vector3`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3 {
    /// `Vector3.Zero`.
    pub const ZERO: Vector3 = Vector3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    /// `Vector3.UnitY`.
    pub const UNIT_Y: Vector3 = Vector3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };

    /// `new Vector3(x, y, z)`.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Vector3 { x, y, z }
    }

    /// `LengthSquared()`: `Dot(this, this)`.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// `Length()`: `MathF.Sqrt(LengthSquared())`.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// `Vector3.Normalize(value)`: `value / value.Length()`.
    #[must_use]
    pub fn normalize(value: Vector3) -> Vector3 {
        value / value.length()
    }

    /// `Vector3.Transform(value, rotation)`: net10's `Vector4.Transform(Create(value, 1f), rotation)`,
    /// `Concatenate(Concatenate(Conjugate(rotation), (value, 1)), rotation)`, which is
    /// `rotation * ((value, 1) * Conjugate(rotation))`, keeping `X`, `Y` and `Z`.
    #[must_use]
    pub fn transform(value: Vector3, rotation: Quaternion) -> Vector3 {
        let temp =
            Quaternion::new(value.x, value.y, value.z, 1.0) * Quaternion::conjugate(rotation);
        let r = rotation * temp;
        Vector3::new(r.x, r.y, r.z)
    }

    /// `Equals(Vector3 other)`: per-lane equality with NaN equal to NaN.
    #[must_use]
    pub fn equals(self, o: Vector3) -> bool {
        lane_equals(self.x, o.x) && lane_equals(self.y, o.y) && lane_equals(self.z, o.z)
    }
}

impl Add for Vector3 {
    type Output = Vector3;
    fn add(self, o: Vector3) -> Vector3 {
        Vector3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vector3 {
    type Output = Vector3;
    fn sub(self, o: Vector3) -> Vector3 {
        Vector3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Neg for Vector3 {
    type Output = Vector3;
    fn neg(self) -> Vector3 {
        Vector3::new(-self.x, -self.y, -self.z)
    }
}

impl Mul<f32> for Vector3 {
    type Output = Vector3;
    fn mul(self, s: f32) -> Vector3 {
        Vector3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Div<f32> for Vector3 {
    type Output = Vector3;
    fn div(self, s: f32) -> Vector3 {
        Vector3::new(self.x / s, self.y / s, self.z / s)
    }
}

/// `System.Numerics.Quaternion`, in .NET's field order (`X`, `Y`, `Z`, `W`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quaternion {
    /// `default(Quaternion)` is all zeroes, not the identity.
    fn default() -> Self {
        Quaternion::new(0.0, 0.0, 0.0, 0.0)
    }
}

impl Quaternion {
    /// `Quaternion.Identity`.
    pub const IDENTITY: Quaternion = Quaternion {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    /// `new Quaternion(x, y, z, w)`.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Quaternion { x, y, z, w }
    }

    /// `Quaternion.CreateFromYawPitchRoll(yaw, pitch, roll)`: roll about Z, then pitch about X,
    /// then yaw about Y. net10 takes the sines and cosines of the half angles from
    /// `Vector3.SinCos(Vector3.Create(roll, pitch, yaw) * 0.5f)`, that is `Vector128.SinCos` over
    /// the lanes `(roll / 2, pitch / 2, yaw / 2, 0)`.
    #[must_use]
    pub fn create_from_yaw_pitch_roll(yaw: f32, pitch: f32, roll: f32) -> Quaternion {
        let (sin, cos) = vector128_sin_cos([roll * 0.5, pitch * 0.5, yaw * 0.5, 0.0]);

        let (sr, cr) = (sin[0], cos[0]);
        let (sp, cp) = (sin[1], cos[1]);
        let (sy, cy) = (sin[2], cos[2]);

        Quaternion::new(
            cy * sp * cr + sy * cp * sr,
            sy * cp * cr - cy * sp * sr,
            cy * cp * sr - sy * sp * cr,
            cy * cp * cr + sy * sp * sr,
        )
    }

    /// `Quaternion.CreateFromAxisAngle(axis, angle)`: the half angle's sine scales the axis, as
    /// given (it is not normalised), and its cosine is `W`. net10 takes both from the scalar
    /// `float.SinCos`, which is [`crate::math::sin_cosf`] here: the two separate calls, so the
    /// result is the same as calling `sinf` and `cosf` in turn.
    #[must_use]
    pub fn create_from_axis_angle(axis: Vector3, angle: f32) -> Quaternion {
        let half_angle = angle * 0.5;
        let (s, c) = crate::math::sin_cosf(half_angle);
        Quaternion::new(axis.x * s, axis.y * s, axis.z * s, c)
    }

    /// `Quaternion.Conjugate(value)`: `value * (-1, -1, -1, 1)`.
    #[must_use]
    pub fn conjugate(value: Quaternion) -> Quaternion {
        Quaternion::new(-value.x, -value.y, -value.z, value.w)
    }

    /// `LengthSquared()`: `Dot(this, this)`, `Vector128.Dot` summed as `(x² + y²) + (z² + w²)`.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        (self.x * self.x + self.y * self.y) + (self.z * self.z + self.w * self.w)
    }

    /// `Length()`: `float.Sqrt(LengthSquared())`.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// `Quaternion.Normalize(value)`: each component divided by `value.Length()`.
    #[must_use]
    pub fn normalize(value: Quaternion) -> Quaternion {
        let l = value.length();
        Quaternion::new(value.x / l, value.y / l, value.z / l, value.w / l)
    }

    /// `Equals(Quaternion other)`: per-lane equality with NaN equal to NaN.
    #[must_use]
    pub fn equals(self, o: Quaternion) -> bool {
        lane_equals(self.x, o.x)
            && lane_equals(self.y, o.y)
            && lane_equals(self.z, o.z)
            && lane_equals(self.w, o.w)
    }
}

impl Mul for Quaternion {
    type Output = Quaternion;

    /// `value1 * value2` (Hamilton product, `value1` on the left), net10's DirectXMath form:
    /// `right * left.W`, then three `MultiplyAddEstimate(Shuffle(right) * left.X|Y|Z, ±1, acc)`.
    /// A fused multiply-add by ±1 rounds once, exactly like the add or subtract written here.
    fn mul(self, value2: Quaternion) -> Quaternion {
        let (l, r) = (self, value2);
        // result = right * left.W
        let (mut x, mut y, mut z, mut w) = (r.x * l.w, r.y * l.w, r.z * l.w, r.w * l.w);
        // Shuffle(right, 3, 2, 1, 0) * left.X, signs (+, -, +, -)
        x += r.w * l.x;
        y -= r.z * l.x;
        z += r.y * l.x;
        w -= r.x * l.x;
        // Shuffle(right, 2, 3, 0, 1) * left.Y, signs (+, +, -, -)
        x += r.z * l.y;
        y += r.w * l.y;
        z -= r.x * l.y;
        w -= r.y * l.y;
        // Shuffle(right, 1, 0, 3, 2) * left.Z, signs (-, +, +, -)
        x -= r.y * l.z;
        y += r.x * l.z;
        z += r.w * l.z;
        w -= r.z * l.z;
        Quaternion::new(x, y, z, w)
    }
}

// ---- Vector128.SinCos(Vector128<float>) --------------------------------------------------------
//
// dotnet/runtime `VectorMath.SinCosSingle` (net10), itself from AMD aocl-libm-ose `sinf`/`cosf`.
// The branch is chosen for the whole vector: the polynomial on the widened lanes when every lane
// is within pi/4 and one is at least 2^-7, the single-precision Taylor terms when every lane is
// below that, the pi/2 reduction when every lane is below 5e6, and otherwise the scalar
// `float.SinCos` (the C runtime's `sinf`/`cosf`) on every lane.
// DIVERGE: the scalar fallback is `empyrean_common::math`'s `sinf`/`cosf`: the same on every
// host, not always the UCRT's last bit (see DIVERGENCES.md).

const ARG_HUGE: u32 = 0x4A98_9680; // 5e6
const ARG_LARGE: u32 = 0x3F49_0FDB; // pi / 4
const ARG_SMALL: u32 = 0x3C00_0000; // 2^-7 (the runtime's comment says 2^-13)
const ARG_SMALLER: u32 = 0x3900_0000; // 2^-13 (the runtime's comment says 2^-27)

/// `Vector128.SinCos` over four `float` lanes.
#[allow(clippy::cast_possible_truncation)]
fn vector128_sin_cos(x: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    let ux = x.map(|v| v.abs().to_bits());
    let mut sin = [0f32; 4];
    let mut cos = [0f32; 4];

    if ux.iter().all(|&u| u < ARG_LARGE + 1) {
        if ux.iter().any(|&u| u > ARG_SMALL - 1) {
            for i in 0..4 {
                let dx = f64::from(x[i]);
                sin[i] = sin_single_poly(dx) as f32;
                cos[i] = cos_single_small(dx) as f32;
            }
        } else {
            for i in 0..4 {
                let x2 = x[i] * x[i];
                let x3 = x2 * x[i];
                sin[i] = (-0.166_666_67_f32).mul_add(x3, x[i]);
                cos[i] = (-0.5f32).mul_add(x2, 1.0);
            }
        }
    } else if ux.iter().all(|&u| u < ARG_HUGE) {
        for i in 0..4 {
            let (s, c) = sin_cos_core(f64::from(x[i]));
            sin[i] = s as f32;
            cos[i] = c as f32;
        }
    } else {
        // ScalarFallback returns without the small-argument selection below.
        return (x.map(crate::math::sinf), x.map(crate::math::cosf));
    }

    for i in 0..4 {
        if ux[i] < ARG_SMALLER {
            sin[i] = x[i];
            cos[i] = 1.0;
        }
    }
    (sin, cos)
}

/// `SinSinglePoly`.
fn sin_single_poly(r: f64) -> f64 {
    const S1: f64 = -0.166_666_666_666_666_66;
    const S2: f64 = 0.008_333_333_333_330_95;
    const S3: f64 = -0.000_198_412_698_367_611_27;
    const S4: f64 = 2.755_731_610_372_880_2e-6;

    let r2 = r * r;
    let r3 = r2 * r;
    let r4 = r2 * r2;

    S4.mul_add(r2, S3)
        .mul_add(r4, S2.mul_add(r2, S1))
        .mul_add(r3, r)
}

/// `CosSinglePoly`.
fn cos_single_poly(r: f64) -> f64 {
    const C1: f64 = 0.041_666_666_666_666_664;
    const C2: f64 = -0.001_388_888_888_888_739_8;
    const C3: f64 = 2.480_158_729_876_704_4e-5;
    const C4: f64 = -2.755_731_727_234_489e-7;

    let r2 = r * r;
    let r4 = r2 * r2;

    C4.mul_add(r2, C3).mul_add(r4, C2.mul_add(r2, C1))
}

/// `CosSingleSmall`.
fn cos_single_small(x: f64) -> f64 {
    let x2 = x * x;
    let x4 = x2 * x2;

    let r = x2 * 0.5;
    let t = 1.0 - r;
    let s = t + ((1.0 - t) - r);

    cos_single_poly(x).mul_add(x4, s)
}

/// `CosSingleLarge`.
fn cos_single_large(r: f64) -> f64 {
    let r2 = r * r;
    let r4 = r2 * r2;

    cos_single_poly(r).mul_add(r4, (-0.5f64).mul_add(r2, 1.0))
}

/// `SinCosReduce`: `|x|` reduced to about [-pi/4, pi/4], and its quadrant in the low bits.
#[allow(clippy::cast_possible_wrap)]
fn sin_cos_reduce(ax: f64) -> (f64, i64) {
    const V_ALM_SHIFT: f64 = 6_755_399_441_055_744.0;
    #[allow(clippy::approx_constant)] // the runtime's literal (it is `FRAC_2_PI`)
    const V_TWO_BY_PI: f64 = 0.636_619_772_367_581_4;
    const V_PI_BY_TWO_1: f64 = 1.570_796_326_734_125_6;
    const V_PI_BY_TWO_2: f64 = 6.077_100_506_303_966e-11;
    const V_PI_BY_TWO_2_TAIL: f64 = 2.022_266_248_795_950_6e-21;

    let mut npi2 = V_TWO_BY_PI.mul_add(ax, V_ALM_SHIFT);
    let region = npi2.to_bits() as i64;
    npi2 -= V_ALM_SHIFT;

    let rhead = (-V_PI_BY_TWO_1).mul_add(npi2, ax);
    let rtail = npi2 * V_PI_BY_TWO_2;
    let r = rhead - rtail;

    let rtail = V_PI_BY_TWO_2_TAIL.mul_add(npi2, -((rhead - r) - rtail));
    (r - rtail, region)
}

/// `SinCosSingle.CoreImpl` on one widened lane.
#[allow(clippy::cast_possible_wrap)]
fn sin_cos_core(x: f64) -> (f64, f64) {
    let (r, region) = sin_cos_reduce(x.abs());

    let sin = sin_single_poly(r);
    let cos = cos_single_large(r);

    let (s, c) = if region & 1 == 0 {
        (sin, cos)
    } else {
        (cos, sin)
    };

    let sign = (x.to_bits() >> 63) as i64;
    let half = region >> 1;
    let s = if ((sign & half) | (!sign & !half)) & 1 == 0 {
        -s
    } else {
        s
    };
    let c = if (region + 1) & 2 == 0 { c } else { -c };
    (s, c)
}
