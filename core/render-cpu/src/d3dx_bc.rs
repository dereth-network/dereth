//! Bundled D3DX9 numeric BC path, not a generic BC encoder.
//!
//! Primary source: the bundled D3DX9 `dxtn` codec -- the DXT1, DXT3 and DXT5 decode and encode
//! leaves, the RGB and alpha optimisers, the 565 packer, the DXT2/4 wrappers and the
//! premultiply/unpremultiply pair.
//! The retail texture loader disables both dithering flags.
//! Retail arithmetic runs at 53-bit precision with f64 intermediates and explicit f32 stores;
//! `s` marks nearest stores and `t` directed stores. No source-operation fusion
//! or u8 decode intermediate; FMA is used only to measure rounding residuals.

type Color = [f32; 4];
const WEIGHT: [f32; 3] = [
    f32::from_bits(0x3e98_1530),
    1.0,
    f32::from_bits(0x3dce_6734),
];
const UNWEIGHT: [f32; 3] = [
    f32::from_bits(0x4057_762e),
    1.0,
    f32::from_bits(0x411e_c1dd),
];
const INV31: f32 = 1.0 / 31.0;
const INV63: f32 = 1.0 / 63.0;
const INV255: f32 = f32::from_bits(0x3b80_8081);
// Measured coefficient tables. The final eight-step B weight
// really is 8/7, not 1; substituting textbook coefficients changes the codec.
const ALPHA_B8: [u32; 8] = [
    0,
    0x3e12_4925,
    0x3e92_4925,
    0x3edb_6db7,
    0x3f12_4925,
    0x3f36_db6e,
    0x3f5b_6db7,
    0x3f92_4925,
];
const ALPHA_A8: [u32; 8] = [
    0x3f80_0000,
    0x3f5b_6db7,
    0x3f36_db6e,
    0x3f12_4925,
    0x3edb_6db7,
    0x3e92_4925,
    0x3e12_4925,
    0,
];
const ALPHA_B6: [u32; 6] = [
    0,
    0x3e4c_cccd,
    0x3ecc_cccd,
    0x3f19_999a,
    0x3f4c_cccd,
    0x3f80_0000,
];
const ALPHA_A6: [u32; 6] = [
    0x3f80_0000,
    0x3f4c_cccd,
    0x3f19_999a,
    0x3ecc_cccd,
    0x3e4c_cccd,
    0,
];

#[allow(clippy::cast_possible_truncation)]
fn s(x: f64) -> f32 {
    x as f32
}
fn d(x: f32) -> f64 {
    f64::from(x)
}

/// The codec switches to round-toward-zero for whole arithmetic regions, not just the
/// float-to-int conversions. Emulate 53-bit/round-toward-zero without mutating the host's
/// floating-point environment.
/// Nearest IEEE results plus an exact residual determine whether to move one ULP
/// toward zero. FMA is used ONLY for residual measurement, never as a replacement
/// for a source multiply/add. Inputs here are finite bounded codec intermediates:
/// normalized decoded BC/BOX channels, quantized weighted endpoints, and finite
/// optimizer sums. The diagonal gate is at 2^-12; nonzero optimizer divisors are
/// sums of fixed weights. No f64 overflow or subnormal product/quotient is reachable.
/// This is deliberately private, not a general all-IEEE-values rounding API.
#[derive(Clone, Copy)]
struct Rz(f64);
fn z(x: f32) -> Rz {
    Rz(d(x))
}
fn toward_zero(value: f64, residual: f64) -> f64 {
    if value > 0.0 && residual < 0.0 {
        value.next_down()
    } else if value < 0.0 && residual > 0.0 {
        value.next_up()
    } else {
        value
    }
}
impl std::ops::Add for Rz {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let sum = self.0 + rhs.0;
        let b = sum - self.0;
        let residual = (self.0 - (sum - b)) + (rhs.0 - b);
        Self(toward_zero(sum, residual))
    }
}
impl std::ops::Sub for Rz {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self + Self(-rhs.0)
    }
}
impl std::ops::Mul for Rz {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let product = self.0 * rhs.0;
        Self(toward_zero(product, self.0.mul_add(rhs.0, -product)))
    }
}
impl std::ops::Div for Rz {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        let quotient = self.0 / rhs.0;
        let residual = (-quotient).mul_add(rhs.0, self.0);
        Self(toward_zero(
            quotient,
            if rhs.0 < 0.0 { -residual } else { residual },
        ))
    }
}
fn t(value: Rz) -> f32 {
    let nearest = s(value.0);
    if d(nearest).abs() > value.0.abs() {
        if nearest > 0.0 {
            nearest.next_down()
        } else {
            nearest.next_up()
        }
    } else {
        nearest
    }
}

fn unpack565(p: u16) -> Color {
    [
        s(f64::from(p >> 11) * d(INV31)),
        s(f64::from((p >> 5) & 63) * d(INV63)),
        s(f64::from(p & 31) * d(INV31)),
        1.0,
    ]
}

pub(crate) fn decode_bc1(bytes: &[u8; 8]) -> [Color; 16] {
    let lo = u16::from_le_bytes([bytes[0], bytes[1]]);
    let hi = u16::from_le_bytes([bytes[2], bytes[3]]);
    let mut palette = [unpack565(lo), unpack565(hi), [0.0; 4], [0.0; 4]];
    for (channel, (a, b)) in palette[0].into_iter().zip(palette[1]).enumerate() {
        let (a, b) = (d(a), d(b));
        if lo > hi {
            // G/B/A are stored to f32 but the extended operand is retained
            // for entry 2. Entry 3 reloads those stores; R stays extended.
            let delta = if channel >= 1 { d(s(b - a)) } else { b - a };
            palette[2][channel] = s((b - a) * d(1.0f32 / 3.0) + a);
            palette[3][channel] = s(delta * d(2.0f32 / 3.0) + a);
        } else {
            palette[2][channel] = s((b - a) * 0.5 + a);
        }
    }
    let indices = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    std::array::from_fn(|i| palette[((indices >> (2 * i)) & 3) as usize])
}

pub(crate) fn decode_bc2(bytes: &[u8; 16]) -> [Color; 16] {
    // The DXT3 decode runs the DXT1 decode, including its endpoint-order behavior, and
    // then multiplies each explicit alpha nibble by the stored float 1/15 and stores f32.
    let mut colors = decode_bc1(bytes[8..].try_into().unwrap());
    for (i, color) in colors.iter_mut().enumerate() {
        let alpha = (bytes[i / 2] >> ((i % 2) * 4)) & 15;
        color[3] = s(f64::from(alpha) * d(f32::from_bits(0x3d88_8889)));
    }
    colors
}

pub(crate) fn decode_bc3(bytes: &[u8; 16]) -> [Color; 16] {
    let mut colors = decode_bc1(bytes[8..].try_into().unwrap());
    let mut palette = [0.0f32; 8];
    palette[0] = s(f64::from(bytes[0]) * d(INV255));
    palette[1] = s(f64::from(bytes[1]) * d(INV255));
    let last = if bytes[0] > bytes[1] { 7usize } else { 5 };
    let inverse = if last == 7 {
        f32::from_bits(0x3e12_4925)
    } else {
        0.2
    };
    for i in 1..last {
        // Normalized-float endpoints, two
        // extended products/add, multiply stored reciprocal, then store f32.
        palette[i + 1] =
            s(((last - i) as f64 * d(palette[0]) + i as f64 * d(palette[1])) * d(inverse));
    }
    if last == 5 {
        palette[6] = 0.0;
        palette[7] = 1.0;
    }
    let indices = bytes[2..8]
        .iter()
        .enumerate()
        .fold(0u64, |v, (i, b)| v | u64::from(*b) << (8 * i));
    for (i, color) in colors.iter_mut().enumerate() {
        color[3] = palette[((indices >> (3 * i)) & 7) as usize];
    }
    colors
}

fn unpremultiply(mut colors: [Color; 16]) -> [Color; 16] {
    for p in &mut colors {
        if p[3] == 0.0 {
            p[..3].fill(0.0);
        } else if p[3] < 1.0 {
            // The reciprocal stays at 53-bit extended precision across all three nearest
            // f32 result stores. Do not round it to f32 or replace with RGB/a.
            let inverse = 1.0 / d(p[3]);
            for c in 0..3 {
                p[c] = if p[c] < p[3] {
                    s(inverse * d(p[c]))
                } else {
                    1.0
                };
            }
        }
        // Alpha itself, and RGB when alpha>=1, are unchanged by the unpremultiply.
    }
    colors
}

fn premultiply(colors: &[Color; 16]) -> [Color; 16] {
    colors.map(|p| {
        [
            s(d(p[3]) * d(p[0])),
            s(d(p[1]) * d(p[3])),
            s(d(p[2]) * d(p[3])),
            p[3],
        ]
    })
}

pub(crate) fn decode_bc2_premultiplied(bytes: &[u8; 16]) -> [Color; 16] {
    unpremultiply(decode_bc2(bytes))
}

pub(crate) fn decode_bc3_premultiplied(bytes: &[u8; 16]) -> [Color; 16] {
    unpremultiply(decode_bc3(bytes))
}

fn optimize_alpha(alpha: &[f32; 16], steps: usize) -> (f32, f32) {
    let (wa, wb): (&[u32], &[u32]) = if steps == 6 {
        (&ALPHA_A6, &ALPHA_B6)
    } else {
        (&ALPHA_A8, &ALPHA_B8)
    };
    let (mut a, mut b) = (1.0f32, 0.0f32);
    for &p in alpha {
        if p < a && (steps == 8 || p > 0.0) {
            a = p;
        }
        if p > b && (steps == 8 || p < 1.0) {
            b = p;
        }
    }
    if steps == 6 && a == b {
        b = 1.0;
    }
    let last = (steps - 1) as f64;
    // The complete iterative region is round-toward-zero, not only
    // integer conversion. Scale/palette/error/divisor/endpoint stores are f32.
    for _ in 0..8 {
        let extent = z(b) - z(a);
        if extent.0 < 1.0 / 256.0 {
            break;
        }
        let scale = t(Rz(last) / extent);
        let palette: [f32; 8] = std::array::from_fn(|i| {
            if i < steps {
                t(z(b) * z(f32::from_bits(wb[i])) + z(a) * z(f32::from_bits(wa[i])))
            } else {
                0.0
            }
        });
        let (mut ea, mut eb, mut da, mut db) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for &p in alpha {
            let projection = (z(p) - z(a)) * z(scale);
            let index = if projection.0 <= 0.0 {
                if steps == 6 && (z(a) * Rz(0.5)).0 >= d(p) {
                    continue;
                }
                0
            } else if projection.0 >= last {
                if steps == 6 && ((z(b) + Rz(1.0)) * Rz(0.5)).0 <= d(p) {
                    continue;
                }
                steps - 1
            } else {
                usize::try_from(dereth_primitives::num::to_i32(t(projection + Rz(0.5))))
                    .expect("bounded alpha projection")
            };
            let (aw, bw) = (f32::from_bits(wa[index]), f32::from_bits(wb[index]));
            let error = z(p) - z(palette[index]);
            ea = t(error * z(aw) + z(ea));
            da = t(z(aw) * z(aw) + z(da));
            eb = t(error * z(bw) + z(eb));
            db = t(z(bw) * z(bw) + z(db));
        }
        if da > 0.0 {
            a = t(z(a) - z(ea) / z(da));
        }
        if db > 0.0 {
            b = t(z(b) - z(eb) / z(db));
        }
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        if (z(ea) * z(ea)).0 < 1.0 / 64.0 && (z(eb) * z(eb)).0 < 1.0 / 64.0 {
            break;
        }
    }
    (a.clamp(0.0, 1.0), b.clamp(0.0, 1.0))
}

fn pack565(p: &[f32; 3]) -> u16 {
    // Runs round-toward-zero including the explicit f32 store before the float-to-int conversion.
    let r = dereth_primitives::num::to_i32(t(z(p[0].clamp(0.0, 1.0)) * Rz(31.0) + Rz(0.5)));
    let g = dereth_primitives::num::to_i32(t(z(p[1].clamp(0.0, 1.0)) * Rz(63.0) + Rz(0.5)));
    let b = dereth_primitives::num::to_i32(t(z(p[2].clamp(0.0, 1.0)) * Rz(31.0) + Rz(0.5)));
    u16::try_from((r << 11) | (g << 5) | b).expect("clamped RGB565")
}

/// The RGB optimiser's weighted bounding diagonal, orientation, then eight-iteration refinement.
fn optimize(colors: &[Color; 16], steps: usize) -> ([f32; 3], [f32; 3]) {
    let mut a = WEIGHT;
    let mut b = [0.0f32; 3];
    for p in colors {
        for c in 0..3 {
            a[c] = a[c].min(p[c]);
            b[c] = b[c].max(p[c]);
        }
    }
    // R/G are stored to f32; B and the B²+G²+R² sum remain extended. The
    // stored length is reloaded only by the later small-diagonal gate.
    let delta = [
        d(s(d(b[0]) - d(a[0]))),
        d(s(d(b[1]) - d(a[1]))),
        d(b[2]) - d(a[2]),
    ];
    let length = (delta[2] * delta[2] + delta[1] * delta[1]) + delta[0] * delta[0];
    if length < d(f32::MIN_POSITIVE) {
        return (a, b);
    }
    let scale = 1.0 / length;
    let axis = delta.map(|v| s(v * scale)); // Stored to f32 before projection.
    let mid: [f32; 3] = std::array::from_fn(|c| s((d(a[c]) + d(b[c])) * 0.5));
    // Direction 0 accumulates in extended precision (stored but kept); the other
    // three directions store f32 on every iteration. Selection retains this 0.
    let mut directions = [0.0f64; 4];
    for p in colors {
        let r = s((d(p[0]) - d(mid[0])) * d(axis[0]));
        let g = s((d(p[1]) - d(mid[1])) * d(axis[1]));
        let z = (d(p[2]) - d(mid[2])) * d(axis[2]);
        for (i, projection) in [
            (d(g) + z) + d(r),
            (d(g) + d(r)) - z,
            (d(r) - d(g)) + z,
            (d(r) - d(g)) - z,
        ]
        .into_iter()
        .enumerate()
        {
            let sum = projection * projection + directions[i];
            directions[i] = if i == 0 { sum } else { d(s(sum)) };
        }
    }
    let mut direction = 0;
    for i in 1..4 {
        if directions[direction] < directions[i] {
            direction = i;
        }
    }
    if direction & 2 != 0 {
        std::mem::swap(&mut a[1], &mut b[1]);
    }
    if direction & 1 != 0 {
        std::mem::swap(&mut a[2], &mut b[2]);
    }
    if s(length) < 1.0 / 4096.0 {
        return (a, b);
    }
    let wa: &[f32] = if steps == 3 {
        &[1.0, 0.5, 0.0]
    } else {
        &[1.0, 2.0 / 3.0, 1.0 / 3.0, 0.0]
    };
    let wb: &[f32] = if steps == 3 {
        &[0.0, 0.5, 1.0]
    } else {
        &[0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0]
    };
    let last = Rz((steps - 1) as f64);
    for _ in 0..8 {
        let mut points = [[0.0f32; 3]; 4];
        for i in 0..steps {
            for c in 0..3 {
                points[i][c] = t(z(a[c]) * z(wa[i]) + z(b[c]) * z(wb[i]));
            }
        }
        let v = [
            z(b[0]) - z(a[0]),
            z(t(z(b[1]) - z(a[1]))),
            z(b[2]) - z(a[2]),
        ];
        let length = (v[2] * v[2] + v[1] * v[1]) + v[0] * v[0];
        if length.0 < 1.0 / 4096.0 {
            break;
        }
        let scale = last / length;
        let axis = v.map(|v| t(v * scale)); // All three components are stored to f32.
        let mut error_a = [0.0f32; 3];
        let mut error_b = [0.0f32; 3];
        let mut divisor_a = 0.0f32;
        let mut divisor_b = 0.0f32;
        for p in colors {
            let projection = (z(p[1]) - z(a[1])) * z(axis[1])
                + (z(p[0]) - z(a[0])) * z(axis[0])
                + (z(p[2]) - z(a[2])) * z(axis[2]);
            // Store, compare extended, then reload and store f32 before the float-to-int conversion.
            let i = if projection.0 < last.0 {
                dereth_primitives::num::to_i32(t(z(t(projection)) + Rz(0.5)))
            } else {
                i32::try_from(steps - 1).unwrap()
            };
            let i = usize::try_from(i)
                .expect("bounding diagonal contains source colors")
                .min(steps - 1);
            let fa = z(wa[i]) * Rz(0.125);
            let fb = z(wb[i]) * Rz(0.125);
            divisor_a = t(fa * z(wa[i]) + z(divisor_a));
            divisor_b = t(fb * z(wb[i]) + z(divisor_b));
            for c in 0..3 {
                let e = if c == 1 {
                    z(t(z(points[i][c]) - z(p[c])))
                } else {
                    z(points[i][c]) - z(p[c])
                };
                error_a[c] = t(fa * e + z(error_a[c]));
                error_b[c] = t(e * fb + z(error_b[c]));
            }
        }
        if divisor_a > 0.0 {
            let f = Rz(-1.0) / z(divisor_a); // Stays extended across all channels.
            for c in 0..3 {
                a[c] = t(z(error_a[c]) * f + z(a[c]));
            }
        }
        if divisor_b > 0.0 {
            let f = Rz(-1.0) / z(divisor_b);
            for c in 0..3 {
                b[c] = t(z(error_b[c]) * f + z(b[c]));
            }
        }
        if error_a
            .iter()
            .chain(&error_b)
            .all(|e| (z(*e) * z(*e)).0 < 1.0 / 65536.0)
        {
            break;
        }
    }
    (a, b)
}

pub(crate) fn encode_bc1(colors: &[Color; 16]) -> [u8; 8] {
    encode_rgb(colors, true)
}

pub(crate) fn encode_bc2(colors: &[Color; 16]) -> [u8; 16] {
    let mut result = [0u8; 16];
    for (i, color) in colors.iter().enumerate() {
        // Scoped round-toward-zero; stores float32 before the float-to-int conversion.
        // Production inputs are normalized decoded/BOX alpha, with no dithering.
        let alpha = dereth_primitives::num::to_i32(t(z(color[3]) * Rz(15.0) + Rz(0.5)));
        let alpha = u8::try_from(alpha).expect("normalized explicit alpha");
        result[i / 2] |= alpha << ((i % 2) * 4);
    }
    // Restores the caller's rounding mode before the RGB encoder (alpha=false).
    result[8..].copy_from_slice(&encode_rgb(colors, false));
    result
}

pub(crate) fn encode_bc3(colors: &[Color; 16]) -> [u8; 16] {
    let (mut low, mut high) = (colors[0][3], colors[0][3]);
    let alpha = colors.map(|p| {
        let n = dereth_primitives::num::to_i32(t(z(p[3]) * Rz(255.0) + Rz(0.5)));
        let expanded = Rz(f64::from(n)) * z(INV255);
        // Compares the retained extended value even after the f32 store;
        // changed extrema and optimizer inputs themselves store float32.
        if expanded.0 < d(low) {
            low = t(expanded);
        } else if expanded.0 > d(high) {
            high = t(expanded);
        }
        t(expanded)
    });
    let mut result = [0u8; 16];
    result[8..].copy_from_slice(&encode_rgb(colors, false));
    if low == 1.0 {
        result[0] = 255;
        result[1] = 255;
        return result;
    }
    let steps = if low == 0.0 || high == 1.0 { 6 } else { 8 };
    let (a, b) = optimize_alpha(&alpha, steps);
    // Scoped round-toward-zero packing, explicit f32 stores before the float-to-int conversion.
    let a = u8::try_from(dereth_primitives::num::to_i32(
        t(z(a) * Rz(255.0) + Rz(0.5)),
    ))
    .unwrap();
    let b = u8::try_from(dereth_primitives::num::to_i32(
        t(z(b) * Rz(255.0) + Rz(0.5)),
    ))
    .unwrap();
    if steps == 8 && a == b {
        result[0] = a;
        result[1] = b;
        return result;
    }
    let (a, b) = if steps == 6 { (a, b) } else { (b, a) };
    result[0] = a;
    result[1] = b;
    let (a, b) = (s(f64::from(a) * d(INV255)), s(f64::from(b) * d(INV255)));
    let last = (steps - 1) as f64;
    // Stores the nearest f32 scale before entering round-toward-zero.
    let scale = if a == b { 0.0 } else { s(last / (d(b) - d(a))) };
    let map: &[u64] = if steps == 6 {
        &[0, 2, 3, 4, 5, 1]
    } else {
        &[0, 2, 3, 4, 5, 6, 7, 1]
    };
    let mut indices = 0u64;
    for (i, p) in colors.iter().enumerate() {
        let projection = (z(p[3]) - z(a)) * z(scale);
        let index = if projection.0 <= 0.0 {
            if steps == 6 && (z(a) * Rz(0.5)).0 >= d(p[3]) {
                6
            } else {
                0
            }
        } else if projection.0 >= last {
            // The test includes equality: the fixed-one boundary is inclusive.
            if steps == 6 && ((z(b) + Rz(1.0)) * Rz(0.5)).0 <= d(p[3]) {
                7
            } else {
                1
            }
        } else {
            map[usize::try_from(dereth_primitives::num::to_i32(t(projection + Rz(0.5)))).unwrap()]
        };
        indices |= index << (3 * i);
    }
    result[2..8].copy_from_slice(&indices.to_le_bytes()[..6]);
    result
}

pub(crate) fn encode_bc2_premultiplied(colors: &[Color; 16]) -> [u8; 16] {
    // Nearest f32 premultiply products precede the inner scoped round-toward-zero.
    encode_bc2(&premultiply(colors))
}

pub(crate) fn encode_bc3_premultiplied(colors: &[Color; 16]) -> [u8; 16] {
    encode_bc3(&premultiply(colors))
}

fn encode_rgb(colors: &[Color; 16], alpha: bool) -> [u8; 8] {
    let transparent = if alpha {
        colors.iter().filter(|p| p[3] < 0.5).count()
    } else {
        0
    };
    if transparent == 16 {
        return [0, 0, 255, 255, 255, 255, 255, 255];
    }
    let steps = if transparent > 0 { 3 } else { 4 };
    let quantized = std::array::from_fn(|i| {
        let mut p = [1.0; 4];
        for c in 0..3 {
            let count = if c == 1 { 63.0 } else { 31.0 };
            let inv = if c == 1 { INV63 } else { INV31 };
            let n = dereth_primitives::num::to_i32(t(z(colors[i][c]) * Rz(count) + Rz(0.5)));
            let expanded = Rz(f64::from(n)) * z(inv);
            // Quantized R/G are reloaded from f32; B stays extended.
            let expanded = if c == 2 { expanded } else { z(t(expanded)) };
            p[c] = t(expanded * z(WEIGHT[c]));
        }
        p
    });
    let (a, b) = optimize(&quantized, steps);
    let a = std::array::from_fn(|c| s(d(a[c]) * d(UNWEIGHT[c])));
    let b = std::array::from_fn(|c| s(d(b[c]) * d(UNWEIGHT[c])));
    let mut c0 = pack565(&a);
    let mut c1 = pack565(&b);
    if steps == 4 && c0 == c1 {
        let mut result = [0; 8];
        result[..2].copy_from_slice(&c0.to_le_bytes());
        result[2..4].copy_from_slice(&c1.to_le_bytes());
        return result;
    }
    if (steps == 3) != (c0 <= c1) {
        std::mem::swap(&mut c0, &mut c1);
    }
    let p0 = unpack565(c0);
    let p1 = unpack565(c1);
    let a: [f32; 3] = std::array::from_fn(|c| s(d(p0[c]) * d(WEIGHT[c])));
    let b: [f32; 3] = std::array::from_fn(|c| s(d(p1[c]) * d(WEIGHT[c])));
    // R is stored and reloaded; G stays extended; B is reloaded from its f32 store.
    // The B²+G²+R² sum is taken before dividing; the order is observable at a
    // real-DAT projection boundary even though broad random corpora miss it.
    let v = [
        d(s(d(b[0]) - d(a[0]))),
        d(b[1]) - d(a[1]),
        d(s(d(b[2]) - d(a[2]))),
    ];
    let last = (steps - 1) as f64;
    let scale = if c0 == c1 {
        0.0
    } else {
        last / (v[2] * v[2] + v[1] * v[1] + v[0] * v[0])
    };
    let axis: [f32; 3] = std::array::from_fn(|c| s(v[c] * scale));
    let indices = if steps == 3 {
        [0u32, 2, 1, 1]
    } else {
        [0, 2, 3, 1]
    };
    let mut bits = 0u32;
    for (i, p) in colors.iter().enumerate() {
        let index = if steps == 3 && p[3] < 0.5 {
            3
        } else {
            let q: [f32; 3] = std::array::from_fn(|c| t(z(WEIGHT[c]) * z(p[c])));
            let projection = (z(q[2]) - z(a[2])) * z(axis[2])
                + (z(q[1]) - z(a[1])) * z(axis[1])
                + (z(q[0]) - z(a[0])) * z(axis[0]);
            if projection.0 <= 0.0 {
                0
            } else if projection.0 >= last {
                1
            } else {
                indices[usize::try_from(dereth_primitives::num::to_i32(t(projection + Rz(0.5))))
                    .unwrap()]
            }
        };
        bits |= index << (2 * i);
    }
    let mut result = [0; 8];
    result[..2].copy_from_slice(&c0.to_le_bytes());
    result[2..4].copy_from_slice(&c1.to_le_bytes());
    result[4..].copy_from_slice(&bits.to_le_bytes());
    result
}

/// DXT decode -> 2x2 box filter -> DXT encode for one successive BC1 level.
/// The codec pads incomplete blocks with table\[0,0,0,1\], not edge clamping.
pub(crate) fn half_bc1(src: &[u8], width: u32, height: u32) -> Result<Vec<u8>, crate::RenderError> {
    half_bc(
        src,
        width,
        height,
        crate::PixelFormatId::Dxt1,
        decode_bc1,
        encode_bc1,
    )
}

/// DXT3 only: DXT2 requires premultiplied decode/filter/encode ownership.
pub(crate) fn half_bc2(src: &[u8], width: u32, height: u32) -> Result<Vec<u8>, crate::RenderError> {
    half_bc(
        src,
        width,
        height,
        crate::PixelFormatId::Dxt3,
        decode_bc2,
        encode_bc2,
    )
}

/// DXT5 only: DXT4 retains distinct premultiplied ownership and remains unported.
pub(crate) fn half_bc3(src: &[u8], width: u32, height: u32) -> Result<Vec<u8>, crate::RenderError> {
    half_bc(
        src,
        width,
        height,
        crate::PixelFormatId::Dxt5,
        decode_bc3,
        encode_bc3,
    )
}

pub(crate) fn half_bc2_premultiplied(
    src: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, crate::RenderError> {
    half_bc(
        src,
        width,
        height,
        crate::PixelFormatId::Dxt2,
        decode_bc2_premultiplied,
        encode_bc2_premultiplied,
    )
}

pub(crate) fn half_bc3_premultiplied(
    src: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, crate::RenderError> {
    half_bc(
        src,
        width,
        height,
        crate::PixelFormatId::Dxt4,
        decode_bc3_premultiplied,
        encode_bc3_premultiplied,
    )
}

/// The fewest 4x4 blocks' worth of work handed to one thread: below it, starting a thread costs
/// more than the work it takes over.
const BLOCKS_PER_THREAD: usize = 32;

/// Run `f(first_row, rows)` over `rows_total` rows of `buf`, `row_len` elements a row, split into
/// bands on at most `threads` threads with at least `min_rows` rows each; on the calling thread
/// alone when that leaves one band. Every band is a disjoint slice, so the result is the one the
/// single-threaded loop writes.
fn par_rows<T: Send>(
    buf: &mut [T],
    row_len: usize,
    rows_total: usize,
    threads: usize,
    min_rows: usize,
    f: &(dyn Fn(usize, &mut [T]) + Sync),
) {
    let bands = threads.min(rows_total / min_rows.max(1)).max(1);
    if bands == 1 {
        f(0, buf);
        return;
    }
    let per = rows_total.div_ceil(bands);
    std::thread::scope(|scope| {
        let mut chunks = buf.chunks_mut(per * row_len).enumerate();
        // The first band on this thread, which would otherwise only wait.
        let first = chunks.next();
        for (t, band) in chunks {
            scope.spawn(move || f(t * per, band));
        }
        if let Some((_, band)) = first {
            f(0, band);
        }
    });
}

/// The most threads one level's encode is spread across, the calling thread included. Small on
/// purpose: the client runs on four- to eight-core machines, starting a thread is not free, and
/// the game has work of its own for the other cores.
const MAX_ENCODE_THREADS: usize = 4;

/// How many threads [`half_bc`] may spread one level's encode across.
fn encode_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(MAX_ENCODE_THREADS)
}

fn half_bc<const N: usize>(
    src: &[u8],
    width: u32,
    height: u32,
    format: crate::PixelFormatId,
    decode: fn(&[u8; N]) -> [Color; 16],
    encode: fn(&[Color; 16]) -> [u8; N],
) -> Result<Vec<u8>, crate::RenderError> {
    half_bc_with(src, width, height, format, decode, encode, encode_threads())
}

/// [`half_bc`] with the encode spread across at most `threads` threads.
///
/// The encoder is the expensive part of a level, and every output block is encoded from its own
/// sixteen filtered texels alone, so the level is split into bands of whole block rows, each band
/// encoded on its own thread into its own slice of the result. The blocks, the arithmetic and the
/// order of the bytes are exactly the single-threaded ones; only the wall-clock time differs.
fn half_bc_with<const N: usize>(
    src: &[u8],
    width: u32,
    height: u32,
    format: crate::PixelFormatId,
    decode: fn(&[u8; N]) -> [Color; 16],
    encode: fn(&[Color; 16]) -> [u8; N],
    threads: usize,
) -> Result<Vec<u8>, crate::RenderError> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 {
        return Err(crate::RenderError::BadDimensions {
            width,
            height,
            reason: "BC system source must have non-zero extents",
        });
    }
    // Texture decompression does not impose the UI surface's 2048 limit. Validate host storage
    // arithmetic instead; driver resource limits remain the upload owner's job.
    let checked_size = |a: usize, b: usize, bytes: usize| {
        a.checked_mul(b)
            .and_then(|count| count.checked_mul(bytes))
            .filter(|&size| size <= isize::MAX as usize)
            .ok_or(crate::RenderError::BadDimensions {
                width,
                height,
                reason: "BC system dimensions exceed addressable storage",
            })
    };
    let blocks_w = w.div_ceil(4);
    let expected = checked_size(blocks_w, h.div_ceil(4), N)?;
    let decoded_count = checked_size(w, h, size_of::<Color>())? / size_of::<Color>();
    let (dw, dh) = ((w / 2).max(1), (h / 2).max(1));
    let filtered_count = checked_size(dw, dh, size_of::<Color>())? / size_of::<Color>();
    let result_bytes = checked_size(dw.div_ceil(4), dh.div_ceil(4), N)?;
    if src.len() < expected {
        return Err(crate::RenderError::ShortSourceData {
            format,
            width,
            height,
            expected,
            actual: src.len(),
        });
    }
    let allocation_error = |_| crate::RenderError::BadDimensions {
        width,
        height,
        reason: "BC system workspace allocation unavailable for these dimensions",
    };
    // Decoding and filtering are cheap next to the encode, and stay on this thread.
    let mut decoded = Vec::new();
    decoded
        .try_reserve_exact(decoded_count)
        .map_err(allocation_error)?;
    decoded.resize(decoded_count, [0.0f32; 4]);
    for (i, bytes) in src[..expected].as_chunks::<N>().0.iter().enumerate() {
        let pixels = decode(bytes);
        let (bx, by) = ((i % blocks_w) * 4, (i / blocks_w) * 4);
        for y in 0..4.min(h - by) {
            for x in 0..4.min(w - bx) {
                decoded[(by + y) * w + bx + x] = pixels[y * 4 + x];
            }
        }
    }
    let mut filtered = Vec::new();
    filtered
        .try_reserve_exact(filtered_count)
        .map_err(allocation_error)?;
    filtered.resize(filtered_count, [0.0f32; 4]);
    for y in 0..dh {
        for x in 0..dw {
            // Odd dimensions are truncated to even; no last odd texel is
            // consumed. A dimension 1 repeats that one sample in the four-term box.
            let (x0, x1, y0, y1) = (x * 2, (x * 2 + 1).min(w - 1), y * 2, (y * 2 + 1).min(h - 1));
            for c in 0..4 {
                filtered[y * dw + x][c] = s(((d(decoded[y0 * w + x1][c])
                    + d(decoded[y0 * w + x0][c]))
                    + d(decoded[y1 * w + x0][c])
                    + d(decoded[y1 * w + x1][c]))
                    * 0.25);
            }
        }
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(result_bytes)
        .map_err(allocation_error)?;
    result.resize(result_bytes, 0);
    // One block row (four texel rows) into `out`, which is exactly that row's bytes.
    let encode_row = |by: usize, out: &mut [u8]| {
        for (i, bx) in (0..dw).step_by(4).enumerate() {
            let (used_x, used_y) = ((dw - bx).min(4), (dh - by).min(4));
            let mut block = [[0.0f32; 4]; 16];
            for y in 0..used_y {
                for x in 0..used_x {
                    block[y * 4 + x] = filtered[(by + y) * dw + bx + x];
                }
                for x in used_x..4 {
                    block[y * 4 + x] = block[y * 4 + [0, 0, 0, 1][x]];
                }
            }
            for y in used_y..4 {
                for x in 0..4 {
                    block[y * 4 + x] = block[[0, 0, 0, 1][y] * 4 + x];
                }
            }
            out[i * N..(i + 1) * N].copy_from_slice(&encode(&block));
        }
    };
    let (cols, row_bytes) = (dw.div_ceil(4), dw.div_ceil(4) * N);
    par_rows(
        &mut result,
        row_bytes,
        dh.div_ceil(4),
        threads,
        BLOCKS_PER_THREAD.div_ceil(cols),
        &|first, band| {
            for (r, out) in band.chunks_mut(row_bytes).enumerate() {
                encode_row((first + r) * 4, out);
            }
        },
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic noise: every byte string is a valid BC block, so this exercises the encoder
    /// on the widest range of inputs.
    fn noise(len: usize, seed: u32) -> Vec<u8> {
        let mut x = seed;
        (0..len)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 24) as u8
            })
            .collect()
    }

    /// Spreading a level's encode across threads changes nothing about its bytes, for every
    /// format, at sizes on both sides of the threshold and with rows that do not divide evenly.
    #[test]
    fn the_threaded_encode_is_the_single_threaded_one_byte_for_byte() {
        type Half = (fn(&[u8; 8]) -> [Color; 16], fn(&[Color; 16]) -> [u8; 8]);
        type Half16 = (fn(&[u8; 16]) -> [Color; 16], fn(&[Color; 16]) -> [u8; 16]);
        let eight: [(crate::PixelFormatId, Half); 1] =
            [(crate::PixelFormatId::Dxt1, (decode_bc1, encode_bc1))];
        let sixteen: [(crate::PixelFormatId, Half16); 4] = [
            (crate::PixelFormatId::Dxt3, (decode_bc2, encode_bc2)),
            (crate::PixelFormatId::Dxt5, (decode_bc3, encode_bc3)),
            (
                crate::PixelFormatId::Dxt2,
                (decode_bc2_premultiplied, encode_bc2_premultiplied),
            ),
            (
                crate::PixelFormatId::Dxt4,
                (decode_bc3_premultiplied, encode_bc3_premultiplied),
            ),
        ];
        for (w, h) in [(256u32, 256u32), (512, 200), (96, 64)] {
            let blocks = (w as usize).div_ceil(4) * (h as usize).div_ceil(4);
            for (format, (dec, enc)) in eight {
                let src = noise(blocks * 8, w ^ h);
                let one = half_bc_with(&src, w, h, format, dec, enc, 1).unwrap();
                let many = half_bc_with(&src, w, h, format, dec, enc, 7).unwrap();
                assert_eq!(one, many, "{format:?} {w}x{h}");
            }
            for (format, (dec, enc)) in sixteen {
                let src = noise(blocks * 16, w ^ h ^ 0x5A);
                let one = half_bc_with(&src, w, h, format, dec, enc, 1).unwrap();
                let many = half_bc_with(&src, w, h, format, dec, enc, 7).unwrap();
                assert_eq!(one, many, "{format:?} {w}x{h}");
            }
        }
    }

    #[test]
    fn directed_arithmetic_and_store_edges() {
        let ulp = f64::EPSILON;
        assert_eq!((Rz(1.0) + Rz(ulp * 1.5)).0, 1.0 + ulp);
        assert_eq!((Rz(-1.0) + Rz(-ulp * 1.5)).0, -1.0 - ulp);
        assert_eq!((Rz(1.0) - Rz(1.0)).0.to_bits(), 0);
        assert_eq!((Rz(1.0) - Rz(1.0 - ulp * 0.5)).0, ulp * 0.5);
        assert_eq!((Rz(1.0 + ulp) * Rz(1.0 - ulp)).0, 1.0f64.next_down());
        assert_eq!((Rz(-1.0 - ulp) * Rz(1.0 - ulp)).0, (-1.0f64).next_up());
        let a = 1.0 + 2.0f64.powi(-26);
        let b = 1.0 + 3.0 * 2.0f64.powi(-27);
        let trunc = 1.0 + 2.0f64.powi(-26) + 3.0 * 2.0f64.powi(-27) + ulp;
        assert_eq!(
            (Rz(a) * Rz(b)).0,
            trunc,
            "product at an odd half-ULP tie rounds toward zero"
        );
        assert_eq!((Rz(-a) * Rz(b)).0, -trunc);
        assert_eq!((Rz(1.0) / Rz(10.0)).0, 0.1f64.next_down());
        assert_eq!((Rz(-1.0) / Rz(10.0)).0, (-0.1f64).next_up());
        assert_eq!((Rz(1.0) / Rz(-10.0)).0, (-0.1f64).next_up());
        let tiny = d(f32::from_bits(1));
        assert_eq!(t(Rz(tiny * 0.75)).to_bits(), 0);
        assert_eq!(t(Rz(-tiny * 0.75)).to_bits(), 0x8000_0000);
        assert_eq!(t(Rz(tiny * 1.75)).to_bits(), 1);
        assert_eq!(t(Rz(1.0 + d(f32::EPSILON) * 0.75)), 1.0);
        assert_eq!(t(Rz(-1.0 - d(f32::EPSILON) * 0.75)), -1.0);
    }

    #[test]
    fn the_dxt1_encoder_matches_the_native_vectors_and_refuses_short_input() {
        // Retail 53-bit/nearest oracle; source alpha cutoff is strict <0.5.
        assert_eq!(
            encode_bc1(&[[1.0; 4]; 16]),
            [255, 255, 255, 255, 0, 0, 0, 0]
        );
        assert_eq!(
            encode_bc1(&[[0.0; 4]; 16]),
            [0, 0, 255, 255, 255, 255, 255, 255]
        );
        assert_eq!(encode_bc1(&[[0.0, 0.0, 0.0, 0.5]; 16]), [0; 8]);
        assert_eq!(
            decode_bc1(&[0x75, 0xd0, 0x23, 0x50, 0x50, 0x27, 0xbc, 0xee])[4][1].to_bits(),
            0x3cd8_b836,
            "retail reloads the stored palette 3 G delta"
        );
        assert!(half_bc1(&[0; 7], 4, 4).is_err());
        assert!(half_bc1(&[0; 8], 0, 4).is_err());
        for (w, h) in [(4, 4), (8, 4), (4, 8), (1, 8), (8, 1), (7, 3)] {
            let data = [255, 255, 255, 255, 0, 0, 0, 0]
                .repeat((w as usize).div_ceil(4) * (h as usize).div_ceil(4));
            let result = half_bc1(&data, w, h).unwrap();
            assert!(result
                .as_chunks::<8>()
                .0
                .iter()
                .all(|b| *b == [255, 255, 255, 255, 0, 0, 0, 0]));
        }
    }

    #[test]
    fn world_dxt1_textures_do_not_inherit_the_ui_surface_size_cap() {
        // Texture loading uses source dimensions (and texture-quality scaling), not
        // the UI surface's independent 2048 cap. Narrow synthetic sources keep this
        // resource-policy regression inexpensive; it does not assert DAT sizes.
        for (width, height) in [(4096, 1), (1, 4096), (2049, 3)] {
            let level0 = [255, 255, 255, 255, 0, 0, 0, 0]
                .repeat((width as usize).div_ceil(4) * (height as usize).div_ceil(4));
            let source = dereth_primitives::TextureData {
                width,
                height,
                format: dereth_primitives::TextureFormat::Bc1,
                levels: vec![level0.clone()],
            };
            let chain = crate::mip::compressed_system_chain(&source)
                .unwrap()
                .unwrap();
            assert_eq!(chain.levels.len(), 4);
            assert_eq!(chain.levels[0], level0);
            let (mut w, mut h) = (width, height);
            for level in &chain.levels[1..] {
                w = (w / 2).max(1);
                h = (h / 2).max(1);
                assert_eq!(
                    level.len(),
                    (w as usize).div_ceil(4) * (h as usize).div_ceil(4) * 8
                );
                assert!(level
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .all(|b| *b == [255, 255, 255, 255, 0, 0, 0, 0]));
            }
        }
        assert!(matches!(
            half_bc1(&[], u32::MAX, u32::MAX),
            Err(crate::RenderError::BadDimensions {
                reason: "BC system dimensions exceed addressable storage",
                ..
            })
        ));
        assert!(matches!(
            half_bc1(&[], 4096, 1),
            Err(crate::RenderError::ShortSourceData {
                expected: 8192,
                actual: 0,
                ..
            })
        ));
        assert!(matches!(
            half_bc1(&[], 1, 0),
            Err(crate::RenderError::BadDimensions { .. })
        ));
    }

    #[test]
    fn the_dxt3_encoder_quantises_explicit_alpha_and_encodes_colour_without_alpha() {
        for (alpha, packed) in [
            (0.0, 0x00),
            (1.0, 0xff),
            (0.5f32.next_down(), 0x77),
            (0.5, 0x88),
            (0.5f32.next_up(), 0x88),
        ] {
            let block = encode_bc2(&[[1.0, 1.0, 1.0, alpha]; 16]);
            assert_eq!(block[..8], [packed; 8]);
            assert_eq!(block[8..], [255, 255, 255, 255, 0, 0, 0, 0]);
        }
        let colors = std::array::from_fn(|i| {
            [
                i as f32 / 15.0,
                0.0,
                1.0 - i as f32 / 15.0,
                if i == 0 { 0.0 } else { 1.0 },
            ]
        });
        let encoded = encode_bc2(&colors);
        assert_ne!(
            encoded[8..],
            encode_bc1(&colors),
            "the RGB encoder must receive alpha=false"
        );
        // Retail's DXT3 decode delegates even the lo<=hi branch to the DXT1 decode, then
        // overwrites alpha: an arbitrary supplied block need not be encoder output.
        let mut encoded = [255; 16];
        encoded[8..10].fill(0);
        assert_eq!(decode_bc2(&encoded), [[0.0, 0.0, 0.0, 1.0]; 16]);
        assert!(matches!(
            half_bc2(&[0; 15], 4, 4),
            Err(crate::RenderError::ShortSourceData {
                format: crate::PixelFormatId::Dxt3,
                expected: 16,
                ..
            })
        ));
        assert!(half_bc2(&[], u32::MAX, u32::MAX).is_err());
        assert!(half_bc2(&[], 0, 1).is_err());
        for (w, h) in [(4096u32, 1u32), (1, 4096)] {
            let flat = encode_bc2(&[[1.0; 4]; 16]).repeat((w.div_ceil(4) * h.div_ceil(4)) as usize);
            assert!(half_bc2(&flat, w, h)
                .unwrap()
                .as_chunks::<16>()
                .0
                .iter()
                .all(|b| *b == encode_bc2(&[[1.0; 4]; 16])));
        }
    }

    fn real_dat_projection_colors() -> [Color; 16] {
        // Source-box float station from actual DXT3 DAT 0x06006992 mip 1 block 1023;
        // expected bytes are retail's DXT3 encode output, not a GPU sample.
        let bits = [
            [0x3de739ce, 0x3df93a3f, 0x3de739ce, 0x3f800000],
            [0x3d94a529, 0x3d8cde23, 0x3d94a529, 0x3f800000],
            [0x3cb02c0a, 0x3d124925, 0x3cb02c0a, 0x3f800000],
            [0x3db02c0a, 0x3dc30c31, 0x3db02c0a, 0x3f777778],
            [0x3d842108, 0x3d6e643b, 0x3d842108, 0x3f800000],
            [0x3dc6318c, 0x3dcde234, 0x3dc6318c, 0x3f800000],
            [0x3e302c0a, 0x3e32cb2d, 0x3e302c0a, 0x3f800000],
            [0x3e302c0a, 0x3e32cb2d, 0x3e302c0a, 0x3f777778],
            [0x3d51344c, 0x3d4de234, 0x3d51344c, 0x3f777778],
            [0x3ddc370d, 0x3dd8b836, 0x3ddc370d, 0x3f800000],
            [0x3e82c0b0, 0x3e77df7f, 0x3e82c0b0, 0x3f800000],
            [0x3ebc8f24, 0x3eb8e38f, 0x3ebc8f24, 0x3f777778],
            [0x3e016058, 0x3dfea541, 0x3e016058, 0x3f1dddde],
            [0x3e3b2ecb, 0x3e38362f, 0x3e3b2ecb, 0x3f800000],
            [0x3e5ef7be, 0x3e4f3cf4, 0x3e5ef7be, 0x3f800000],
            [0x3e82c0b0, 0x3e77df7f, 0x3e82c0b0, 0x3f777778],
        ];
        bits.map(|p| p.map(f32::from_bits))
    }

    #[test]
    fn a_real_dat_block_encodes_to_the_native_bytes_as_dxt3_and_as_dxt1() {
        let colors = real_dat_projection_colors();
        assert_eq!(
            encode_bc2(&colors),
            [
                0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xf9, 0xff, 0xab, 0x5a, 0x61, 0x08, 0x57, 0xfd,
                0x2d, 0xaf
            ]
        );
        assert_eq!(
            encode_bc1(&colors),
            [0xab, 0x5a, 0x61, 0x08, 0x57, 0xfd, 0x2d, 0xaf],
            "same source boundary with BC1 alpha=true (all station alpha is >=0.5)"
        );
    }

    fn real_dat_orientation_colors() -> [Color; 16] {
        // Actual DXT5 DAT 0x0600378A, source-box mip 1 block 1412. The alpha is
        // zero but RGB still participates in non-premultiplied DXT3/5 filtering.
        let a = [0x3e8f23c9, 0x3ec5c1b3, 0x3e3b2ecb, 0];
        let b = [0x3e94a529, 0x3ec30c32, 0x3e25294a, 0];
        let c = [0x3e89a268, 0x3ec30c32, 0x3e3b2ecb, 0];
        let d = [0x3e8b02c0, 0x3ec30c32, 0x3e386e1b, 0];
        [a, a, b, b, a, a, b, b, a, a, c, d, a, a, c, c].map(|p| p.map(f32::from_bits))
    }

    #[test]
    fn a_real_dat_dxt5_block_with_zero_alpha_keeps_the_native_colour_endpoints() {
        let colors = real_dat_orientation_colors();
        assert_eq!(
            encode_bc3(&colors),
            [
                0xff, 0, 0x49, 0x92, 0x24, 0x49, 0x92, 0x24, 0x06, 0x4b, 0x05, 0x43, 0x0a, 0x0a,
                0xfa, 0xfa
            ]
        );
        let expected_rgb = [0x06, 0x4b, 0x05, 0x43, 0x0a, 0x0a, 0xfa, 0xfa];
        assert_eq!(encode_bc2(&colors)[8..], expected_rgb);
        assert_eq!(
            encode_bc1(&colors.map(|mut p| {
                p[3] = 1.0;
                p
            })),
            expected_rgb
        );
    }

    #[test]
    fn the_dxt5_alpha_encoder_picks_fixed_one_at_the_inclusive_boundary_and_checks_extents() {
        for (alpha, bytes) in [
            (1.0, [255, 255, 0, 0, 0, 0, 0, 0]),
            (0.0, [255, 0, 0x49, 0x92, 0x24, 0x49, 0x92, 0x24]),
            (0.5, [128, 128, 0, 0, 0, 0, 0, 0]),
        ] {
            let block = encode_bc3(&[[1.0, 1.0, 1.0, alpha]; 16]);
            assert_eq!(block[..8], bytes);
            assert_eq!(block[8..], [255, 255, 255, 255, 0, 0, 0, 0]);
        }
        // Retail mode 5, first broad corpus station. Alpha 1 must select
        // fixed-one index 7 when b==1 (the boundary is inclusive), not endpoint index 1.
        let alpha = [
            0, 0x3ee07ae0, 0x3e8b3c8b, 0x3ee7a0e8, 0x3f65dce6, 0x3f2a1baa, 0x3ec15ac1, 0x3f800000,
            0x3f3dbdbe, 0x3f16e197, 0x3e7124f1, 0x3cb0e0b1, 0x3f2bd1ac, 0x3f5d00dd, 0, 0x3f195199,
        ];
        assert_eq!(
            encode_bc3(&alpha.map(|a| [1.0, 1.0, 1.0, f32::from_bits(a)]))[..8],
            [0, 255, 0x9e, 0x56, 0xee, 0xa5, 0xc0, 0x9a]
        );
        assert!(matches!(
            half_bc3(&[0; 15], 4, 4),
            Err(crate::RenderError::ShortSourceData {
                format: crate::PixelFormatId::Dxt5,
                expected: 16,
                ..
            })
        ));
        assert!(half_bc3(&[], u32::MAX, u32::MAX).is_err());
        assert!(half_bc3(&[], 0, 1).is_err());
        for (w, h) in [(4096u32, 1u32), (1, 4096)] {
            let block = encode_bc3(&[[1.0; 4]; 16]);
            let source = block.repeat((w.div_ceil(4) * h.div_ceil(4)) as usize);
            assert!(half_bc3(&source, w, h)
                .unwrap()
                .as_chunks::<16>()
                .0
                .iter()
                .all(|b| *b == block));
        }
    }

    #[test]
    fn premultiplied_zero_saturation_stores_and_checked_extents() {
        let mut colors = [[0.0; 4]; 16];
        colors[0] = [1.0, 0.5, 0.25, 0.0];
        colors[1] = [0.75, 0.5, 0.25, 0.5];
        colors[2] = [0.25, 0.5, 0.75, 1.0];
        colors[3] = [0.0, f32::from_bits(1), 1.0, f32::from_bits(1)];
        let straight = unpremultiply(colors);
        assert_eq!(straight[0], [0.0; 4]);
        assert_eq!(
            straight[1],
            [1.0, 1.0, 0.5, 0.5],
            "source saturates RGB>=alpha, including equality"
        );
        assert_eq!(straight[2], colors[2], "alpha1 is a true passthrough");
        assert_eq!(straight[3], [0.0, 1.0, 1.0, f32::from_bits(1)]);
        colors[4] = [0.5, 0.5f32.next_up(), 1.0, f32::from_bits(1)];
        assert_eq!(
            premultiply(&colors)[4].map(f32::to_bits),
            [0, 1, 1, 1],
            "nearest-store subnormal tie before codecRC"
        );
        for (format, encode, half) in [
            (
                crate::PixelFormatId::Dxt2,
                encode_bc2_premultiplied as fn(&[Color; 16]) -> [u8; 16],
                half_bc2_premultiplied
                    as fn(&[u8], u32, u32) -> Result<Vec<u8>, crate::RenderError>,
            ),
            (
                crate::PixelFormatId::Dxt4,
                encode_bc3_premultiplied,
                half_bc3_premultiplied,
            ),
        ] {
            assert_eq!(
                encode(&[[1.0, 0.4, 0.7, 0.0]; 16])[8..],
                [0; 8],
                "alpha0 removes invisible RGB before encode"
            );
            assert!(
                matches!(half(&[0;15],4,4),Err(crate::RenderError::ShortSourceData {format:f,expected:16,..}) if f==format)
            );
            assert!(half(&[], u32::MAX, u32::MAX).is_err());
            assert!(half(&[], 0, 1).is_err());
            for (w, h) in [(4096u32, 1u32), (1, 4096)] {
                let block = encode(&[[1.0; 4]; 16]);
                assert!(half(
                    &block.repeat((w.div_ceil(4) * h.div_ceil(4)) as usize),
                    w,
                    h
                )
                .unwrap()
                .as_chunks::<16>()
                .0
                .iter()
                .all(|b| *b == block));
            }
        }
    }
}
