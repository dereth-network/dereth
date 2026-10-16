//! Block compression for pictures made at run time: BC4 (one channel), BC5 (two) and BC7 (four),
//! each block four by four texels, and the decoding of what it writes.
//!
//! A picture compressed so takes a quarter of the memory of eight bits a channel (BC7, BC5) or an
//! eighth (BC4), and the device samples it as it samples the uncompressed picture. The encoders
//! work on the bytes as they are stored, so a colour picture is compressed in the display's
//! encoding and decodes back to it.
//!
//! - **BC4** keeps red: two end values and an index of three bits a texel into the eight values
//!   between them.
//! - **BC5** keeps red and green, a BC4 block each.
//! - **BC7** keeps all four channels with one line through them (its mode 6): two end colours of
//!   seven bits a channel and one shared low bit each, and an index of four bits a texel into the
//!   sixteen colours between them. The decoder reads that mode alone, the only one written here.
//!
//! A picture whose sides are not whole blocks is compressed as if its last row and column were
//! repeated out to the block's edge; the decoder returns the picture's own texels only.

/// One of the block formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockFormat {
    /// Red alone, eight bytes a block.
    Bc4,
    /// Red and green, sixteen bytes a block.
    Bc5,
    /// All four channels, sixteen bytes a block.
    Bc7,
}

impl BlockFormat {
    /// The bytes of one block.
    #[must_use]
    pub const fn block_bytes(self) -> usize {
        match self {
            Self::Bc4 => 8,
            Self::Bc5 | Self::Bc7 => 16,
        }
    }

    /// The bytes of a `width` by `height` picture in this format.
    #[must_use]
    pub fn len(self, width: u32, height: u32) -> usize {
        blocks(width) * blocks(height) * self.block_bytes()
    }
}

/// The blocks across `texels`.
fn blocks(texels: u32) -> usize {
    (texels.max(1) as usize).div_ceil(4)
}

/// The sixteen texels of the block at `(bx, by)`, four bytes each, rows top first, clamped to
/// the picture's edge.
fn gather(rgba: &[u8], width: usize, height: usize, bx: usize, by: usize) -> [[u8; 4]; 16] {
    let mut out = [[0u8; 4]; 16];
    for (k, t) in out.iter_mut().enumerate() {
        let x = (bx * 4 + k % 4).min(width - 1);
        let y = (by * 4 + k / 4).min(height - 1);
        let i = (y * width + x) * 4;
        t.copy_from_slice(&rgba[i..i + 4]);
    }
    out
}

/// The most threads one picture's encode is spread across, the calling thread included.
#[cfg(not(target_arch = "wasm32"))]
const MAX_THREADS: usize = 16;
/// The fewest block rows a thread is given.
const MIN_ROWS: usize = 16;

fn threads() -> usize {
    #[cfg(target_arch = "wasm32")]
    {
        1
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .min(MAX_THREADS)
    }
}

/// Run `f(first_row, band)` over the block rows of `out` (`row_bytes` a row), in bands on
/// several threads when there are rows enough. Each band is its own slice, so the bytes are
/// those one thread writes.
fn par_rows(out: &mut [u8], row_bytes: usize, f: &(dyn Fn(usize, &mut [u8]) + Sync)) {
    let rows = out.len() / row_bytes.max(1);
    let bands = threads().min(rows / MIN_ROWS).max(1);
    if bands == 1 {
        f(0, out);
        return;
    }
    let per = rows.div_ceil(bands);
    std::thread::scope(|scope| {
        let mut chunks = out.chunks_mut(per * row_bytes).enumerate();
        let first = chunks.next();
        for (t, band) in chunks {
            scope.spawn(move || f(t * per, band));
        }
        if let Some((_, band)) = first {
            f(0, band);
        }
    });
}

/// `rgba` (`width` by `height`, four bytes a texel) compressed to `format`: BC4 from red, BC5
/// from red and green, BC7 from all four. Blocks run row by row from the top.
///
/// # Panics
/// `rgba` is shorter than `width * height * 4`.
#[must_use]
pub fn encode(format: BlockFormat, width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let (w, h) = (width.max(1) as usize, height.max(1) as usize);
    assert!(
        rgba.len() >= w * h * 4,
        "a picture of {w}x{h} needs {} bytes",
        w * h * 4
    );
    let bw = blocks(width);
    let size = format.block_bytes();
    let mut out = vec![0u8; format.len(width, height)];
    par_rows(&mut out, bw * size, &|first, band| {
        for (r, row) in band.chunks_mut(bw * size).enumerate() {
            let by = first + r;
            for (bx, block) in row.chunks_mut(size).enumerate() {
                let texels = gather(rgba, w, h, bx, by);
                match format {
                    BlockFormat::Bc4 => block.copy_from_slice(&bc4_block(&texels.map(|t| t[0]))),
                    BlockFormat::Bc5 => {
                        block[..8].copy_from_slice(&bc4_block(&texels.map(|t| t[0])));
                        block[8..].copy_from_slice(&bc4_block(&texels.map(|t| t[1])));
                    }
                    BlockFormat::Bc7 => block.copy_from_slice(&bc7_block(&texels)),
                }
            }
        }
    });
    out
}

/// `blocks` (as [`encode`] wrote them) back to four bytes a texel, `width` by `height`: BC4 as
/// red with green and blue nought and alpha whole, BC5 as red and green with blue nought and
/// alpha whole, BC7 as written.
///
/// # Panics
/// `blocks` is shorter than [`BlockFormat::len`].
#[must_use]
pub fn decode(format: BlockFormat, width: u32, height: u32, blocks_in: &[u8]) -> Vec<u8> {
    let (w, h) = (width.max(1) as usize, height.max(1) as usize);
    let bw = blocks(width);
    let size = format.block_bytes();
    assert!(
        blocks_in.len() >= format.len(width, height),
        "{format:?} {w}x{h} needs {} bytes",
        format.len(width, height)
    );
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let at = ((y / 4) * bw + x / 4) * size;
            let block = &blocks_in[at..at + size];
            let k = (y % 4) * 4 + x % 4;
            let t = match format {
                BlockFormat::Bc4 => [bc4_texel(block, k), 0, 0, 255],
                BlockFormat::Bc5 => [bc4_texel(&block[..8], k), bc4_texel(&block[8..], k), 0, 255],
                BlockFormat::Bc7 => bc7_texel(block, k),
            };
            out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&t);
        }
    }
    out
}

// --- BC4 ------------------------------------------------------------------------------------

/// The eight values of a BC4 block with end values `a` > `b`, as the device reads them, in
/// eighths of a level: `a`, `b`, then six steps from `a` toward `b`.
fn bc4_palette(a: u8, b: u8) -> [f32; 8] {
    let (a, b) = (f32::from(a), f32::from(b));
    let mut p = [a, b, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    for (k, v) in p.iter_mut().enumerate().skip(2) {
        // LINT-OK: k is 2..8.
        #[allow(clippy::cast_precision_loss)]
        let j = (k - 1) as f32;
        *v = (a * (7.0 - j) + b * j) / 7.0;
    }
    p
}

/// The value texel `k` of a BC4 block reads as, rounded to a byte.
fn bc4_texel(block: &[u8], k: usize) -> u8 {
    let (a, b) = (block[0], block[1]);
    let bits = u64::from_le_bytes([
        block[2], block[3], block[4], block[5], block[6], block[7], 0, 0,
    ]);
    // LINT-OK: three bits.
    #[allow(clippy::cast_possible_truncation)]
    let i = ((bits >> (3 * k)) & 7) as usize;
    let v = if a > b {
        bc4_palette(a, b)[i]
    } else {
        // The six-value form: six steps between, then nought and whole.
        let (fa, fb) = (f32::from(a), f32::from(b));
        // LINT-OK: i is 0..8.
        #[allow(clippy::cast_precision_loss)]
        let j = i as f32;
        match i {
            0 => fa,
            1 => fb,
            6 => 0.0,
            7 => 255.0,
            _ => (fa * (6.0 - j) + fb * (j - 1.0)) / 5.0,
        }
    };
    to_byte(v)
}

fn to_byte(v: f32) -> u8 {
    // LINT-OK: a value clamped to 0..255 and rounded.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (v.clamp(0.0, 255.0) + 0.5) as u8;
    b
}

/// The nearest of `palette` to each of `values`, and the squared error.
fn bc4_fit(values: &[u8; 16], palette: &[f32; 8]) -> ([u8; 16], f32) {
    let mut idx = [0u8; 16];
    let mut err = 0.0;
    for (i, v) in values.iter().enumerate() {
        let v = f32::from(*v);
        let (mut best, mut best_d) = (0u8, f32::INFINITY);
        for (k, p) in (0u8..).zip(palette) {
            let d = (p - v) * (p - v);
            if d < best_d {
                (best, best_d) = (k, d);
            }
        }
        idx[i] = best;
        err += best_d;
    }
    (idx, err)
}

/// The share of the second end value in each of a BC4 block's eight values.
const BC4_SHARE: [f32; 8] = [
    0.0,
    1.0,
    1.0 / 7.0,
    2.0 / 7.0,
    3.0 / 7.0,
    4.0 / 7.0,
    5.0 / 7.0,
    6.0 / 7.0,
];

/// One BC4 block of `values`: end values the extremes, then fitted to the indices they give by
/// least squares, the best kept.
fn bc4_block(values: &[u8; 16]) -> [u8; 8] {
    let hi = *values.iter().max().unwrap_or(&0);
    let lo = *values.iter().min().unwrap_or(&0);
    if hi == lo {
        // One value: both ends it, every index the first.
        return [hi, lo, 0, 0, 0, 0, 0, 0];
    }
    let (mut a, mut b) = (hi, lo);
    let (mut idx, mut err) = bc4_fit(values, &bc4_palette(a, b));
    for _ in 0..2 {
        // The end values that best fit these indices.
        let (mut s00, mut s01, mut s11, mut r0, mut r1) = (0.0f32, 0.0, 0.0, 0.0, 0.0);
        for (v, i) in values.iter().zip(idx) {
            let t = BC4_SHARE[usize::from(i)];
            let v = f32::from(*v);
            s00 += (1.0 - t) * (1.0 - t);
            s01 += t * (1.0 - t);
            s11 += t * t;
            r0 += (1.0 - t) * v;
            r1 += t * v;
        }
        let det = s00 * s11 - s01 * s01;
        if det.abs() < 1e-6 {
            break;
        }
        let na = to_byte((s11 * r0 - s01 * r1) / det);
        let nb = to_byte((s00 * r1 - s01 * r0) / det);
        let (na, nb) = match na.cmp(&nb) {
            std::cmp::Ordering::Greater => (na, nb),
            std::cmp::Ordering::Less => (nb, na),
            std::cmp::Ordering::Equal => break,
        };
        let (ni, ne) = bc4_fit(values, &bc4_palette(na, nb));
        if ne >= err {
            break;
        }
        (a, b, idx, err) = (na, nb, ni, ne);
    }
    let mut bits = 0u64;
    for (k, i) in idx.iter().enumerate() {
        bits |= u64::from(*i) << (3 * k);
    }
    let i = bits.to_le_bytes();
    [a, b, i[0], i[1], i[2], i[3], i[4], i[5]]
}

// --- BC7 mode 6 ---------------------------------------------------------------------------------

/// The sixteen weights of a four-bit index, in sixty-fourths of the second end colour.
const BC7_WEIGHTS: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

/// Channel `c` of the colour between `a` and `b` at weight `w` sixty-fourths, as the device
/// reads it.
fn bc7_mix(a: u8, b: u8, w: u32) -> u8 {
    let v = ((64 - w) * u32::from(a) + w * u32::from(b) + 32) >> 6;
    // LINT-OK: a weighted mean of two bytes.
    #[allow(clippy::cast_possible_truncation)]
    let v = v as u8;
    v
}

/// The sixteen colours between `a` and `b`.
fn bc7_palette(a: [u8; 4], b: [u8; 4]) -> [[u8; 4]; 16] {
    BC7_WEIGHTS.map(|w| [0, 1, 2, 3].map(|c| bc7_mix(a[c], b[c], w)))
}

/// Each texel's index into the colours between `a` and `b`, by where it falls along the line
/// between them (the colours all lie on it), and the squared error.
fn bc7_fit(texels: &[[u8; 4]; 16], a: [u8; 4], b: [u8; 4]) -> ([u8; 16], u32) {
    let palette = bc7_palette(a, b);
    let d: [f32; 4] = [0, 1, 2, 3].map(|c| f32::from(b[c]) - f32::from(a[c]));
    let dd: f32 = d.iter().map(|v| v * v).sum();
    let mut idx = [0u8; 16];
    let mut err = 0u32;
    for (k, t) in texels.iter().enumerate() {
        let i = if dd < 1e-6 {
            0
        } else {
            let along: f32 = (0..4)
                .map(|c| (f32::from(t[c]) - f32::from(a[c])) * d[c])
                .sum::<f32>()
                / dd;
            // LINT-OK: a share clamped to 0..1, scaled to sixty-fourths.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let w = (along.clamp(0.0, 1.0) * 64.0) as u32;
            // The nearest weight, and its neighbour above when that is nearer in colour.
            let near = BC7_WEIGHTS.partition_point(|x| *x < w).min(15);
            let below = near.saturating_sub(1);
            let sq = |p: &[u8; 4]| -> u32 {
                (0..4)
                    .map(|c| {
                        let e = i32::from(p[c]) - i32::from(t[c]);
                        e.unsigned_abs() * e.unsigned_abs()
                    })
                    .sum()
            };
            if sq(&palette[below]) <= sq(&palette[near]) {
                below
            } else {
                near
            }
        };
        let e: u32 = (0..4)
            .map(|c| {
                let e = i32::from(palette[i][c]) - i32::from(t[c]);
                e.unsigned_abs() * e.unsigned_abs()
            })
            .sum();
        err += e;
        // LINT-OK: an index of sixteen.
        #[allow(clippy::cast_possible_truncation)]
        {
            idx[k] = i as u8;
        }
    }
    (idx, err)
}

/// `v` as seven bits and the low bit `p`: the nearest byte whose low bit is `p`.
fn with_low_bit(v: f32, p: u8) -> u8 {
    let q = ((v - f32::from(p)) / 2.0).round().clamp(0.0, 127.0);
    // LINT-OK: seven bits.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let q = q as u8;
    q * 2 + p
}

/// `v` as a byte of each channel sharing one low bit: whichever low bit lands nearer.
fn shared_low_bit(v: [f32; 4]) -> [u8; 4] {
    let take = |p: u8| v.map(|c| with_low_bit(c, p));
    let off = |q: [u8; 4]| -> f32 { (0..4).map(|c| (f32::from(q[c]) - v[c]).powi(2)).sum() };
    let (even, odd) = (take(0), take(1));
    if off(even) <= off(odd) {
        even
    } else {
        odd
    }
}

/// The end colours `a` and `b` taken to bytes with a shared low bit each, and fitted: the end
/// colours, the indices and the squared error.
fn bc7_quantised(
    texels: &[[u8; 4]; 16],
    a: [f32; 4],
    b: [f32; 4],
) -> ([u8; 4], [u8; 4], [u8; 16], u32) {
    let (qa, qb) = (shared_low_bit(a), shared_low_bit(b));
    let (idx, err) = bc7_fit(texels, qa, qb);
    (qa, qb, idx, err)
}

/// One BC7 block of `texels` in mode 6: the end colours from the line the texels spread along
/// most, then fitted to the indices they give by least squares, the best kept.
fn bc7_block(texels: &[[u8; 4]; 16]) -> [u8; 16] {
    let px: [[f32; 4]; 16] = texels.map(|t| t.map(f32::from));
    let mut mean = [0.0f32; 4];
    for p in &px {
        for c in 0..4 {
            mean[c] += p[c] / 16.0;
        }
    }
    // The texels' spread, and the direction of most of it (by repeated multiplication).
    let mut cov = [[0.0f32; 4]; 4];
    for p in &px {
        for i in 0..4 {
            for j in 0..4 {
                cov[i][j] += (p[i] - mean[i]) * (p[j] - mean[j]);
            }
        }
    }
    let mut axis = [0.0f32; 4];
    // Start from the channel that varies most.
    let widest = (0..4)
        .max_by(|x, y| cov[*x][*x].total_cmp(&cov[*y][*y]))
        .unwrap_or(0);
    axis[widest] = 1.0;
    for _ in 0..8 {
        let mut next = [0.0f32; 4];
        for i in 0..4 {
            next[i] = (0..4).map(|j| cov[i][j] * axis[j]).sum();
        }
        let len = next.iter().map(|v| v * v).sum::<f32>().sqrt();
        if len < 1e-6 {
            break;
        }
        axis = next.map(|v| v / len);
    }
    let along = |p: &[f32; 4]| -> f32 { (0..4).map(|c| (p[c] - mean[c]) * axis[c]).sum() };
    let lo = px.iter().map(along).fold(f32::INFINITY, f32::min);
    let hi = px.iter().map(along).fold(f32::NEG_INFINITY, f32::max);
    let end =
        |t: f32| -> [f32; 4] { [0, 1, 2, 3].map(|c| (mean[c] + axis[c] * t).clamp(0.0, 255.0)) };
    let (mut a, mut b, mut idx, mut err) = bc7_quantised(texels, end(lo), end(hi));
    for _ in 0..2 {
        if err == 0 {
            break;
        }
        // The end colours that best fit these indices.
        let (mut s00, mut s01, mut s11) = (0.0f32, 0.0, 0.0);
        let (mut r0, mut r1) = ([0.0f32; 4], [0.0f32; 4]);
        for (p, i) in px.iter().zip(idx) {
            // LINT-OK: sixty-fourths.
            #[allow(clippy::cast_precision_loss)]
            let t = BC7_WEIGHTS[usize::from(i)] as f32 / 64.0;
            s00 += (1.0 - t) * (1.0 - t);
            s01 += t * (1.0 - t);
            s11 += t * t;
            for c in 0..4 {
                r0[c] += (1.0 - t) * p[c];
                r1[c] += t * p[c];
            }
        }
        let det = s00 * s11 - s01 * s01;
        if det.abs() < 1e-6 {
            break;
        }
        let na = [0, 1, 2, 3].map(|c| ((s11 * r0[c] - s01 * r1[c]) / det).clamp(0.0, 255.0));
        let nb = [0, 1, 2, 3].map(|c| ((s00 * r1[c] - s01 * r0[c]) / det).clamp(0.0, 255.0));
        let (qa, qb, qi, qe) = bc7_quantised(texels, na, nb);
        if qe >= err {
            break;
        }
        (a, b, idx, err) = (qa, qb, qi, qe);
    }
    // The first texel's index is written with three bits: its top bit must be clear, so the end
    // colours swap round where it is not.
    if idx[0] >= 8 {
        std::mem::swap(&mut a, &mut b);
        idx = idx.map(|i| 15 - i);
    }
    let mut bits: u128 = 1 << 6;
    let mut at = 7;
    let mut put = |v: u128, n: u32| {
        bits |= v << at;
        at += n;
    };
    for c in 0..4 {
        put(u128::from(a[c] >> 1), 7);
        put(u128::from(b[c] >> 1), 7);
    }
    put(u128::from(a[0] & 1), 1);
    put(u128::from(b[0] & 1), 1);
    for (k, i) in idx.iter().enumerate() {
        put(u128::from(*i), if k == 0 { 3 } else { 4 });
    }
    bits.to_le_bytes()
}

/// Texel `k` of a BC7 block, as the device reads it; a block in a mode other than 6 reads as
/// nought.
fn bc7_texel(block: &[u8], k: usize) -> [u8; 4] {
    let mut raw = [0u8; 16];
    raw.copy_from_slice(&block[..16]);
    let bits = u128::from_le_bytes(raw);
    if bits & 0x7F != 1 << 6 {
        return [0; 4];
    }
    // LINT-OK: fields of at most seven bits.
    #[allow(clippy::cast_possible_truncation)]
    let field = |at: u32, n: u32| -> u8 { ((bits >> at) & ((1 << n) - 1)) as u8 };
    let (pa, pb) = (field(63, 1), field(64, 1));
    let mut a = [0u8; 4];
    let mut b = [0u8; 4];
    for c in 0..4 {
        // LINT-OK: c is 0..4.
        #[allow(clippy::cast_possible_truncation)]
        let at = 7 + 14 * c as u32;
        a[c] = field(at, 7) << 1 | pa;
        b[c] = field(at + 7, 7) << 1 | pb;
    }
    // LINT-OK: k is 0..16.
    #[allow(clippy::cast_possible_truncation)]
    let k = k as u32;
    let i = if k == 0 {
        field(65, 3)
    } else {
        field(68 + 4 * (k - 1), 4)
    };
    let w = BC7_WEIGHTS[usize::from(i)];
    [0, 1, 2, 3].map(|c| bc7_mix(a[c], b[c], w))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs: their pictures are compressed as they open)
    use super::*;

    /// A picture `w` by `h` of `f(x, y)`.
    fn picture(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| f(x, y))
            .collect()
    }

    /// The largest difference of channel `c` between two pictures.
    fn worst(a: &[u8], b: &[u8], channels: &[usize]) -> u8 {
        a.chunks(4)
            .zip(b.chunks(4))
            .flat_map(|(p, q)| channels.iter().map(move |c| p[*c].abs_diff(q[*c])))
            .max()
            .unwrap_or(0)
    }

    /// A pseudo-random byte for texel `(x, y)` and channel `c`.
    fn noise(x: u32, y: u32, c: u32) -> u8 {
        let h =
            x.wrapping_mul(0x8DA6_B343) ^ y.wrapping_mul(0xD816_3841) ^ c.wrapping_mul(0xCB1A_B31F);
        let h = (h ^ (h >> 13)).wrapping_mul(0x2C1B_3C6D);
        // LINT-OK: the top byte.
        #[allow(clippy::cast_possible_truncation)]
        let b = (h >> 24) as u8;
        b
    }

    #[test]
    fn each_format_takes_its_share_of_the_bytes_of_eight_bits_a_channel() {
        assert_eq!(BlockFormat::Bc7.len(1024, 1024), 1024 * 1024);
        assert_eq!(BlockFormat::Bc5.len(1024, 1024), 1024 * 1024);
        assert_eq!(BlockFormat::Bc4.len(1024, 1024), 512 * 1024);
        // Part blocks count whole.
        assert_eq!(BlockFormat::Bc7.len(2, 2), 16);
        assert_eq!(BlockFormat::Bc4.len(5, 9), 2 * 3 * 8);
        assert_eq!(encode(BlockFormat::Bc5, 8, 4, &[0; 8 * 4 * 4]).len(), 32);
    }

    #[test]
    fn a_flat_picture_comes_back_within_a_level_in_every_format() {
        for colour in [
            [0, 0, 0, 0],
            [255, 255, 255, 255],
            [37, 200, 91, 128],
            [1, 2, 3, 4],
        ] {
            let p = picture(8, 8, |_, _| colour);
            for f in [BlockFormat::Bc4, BlockFormat::Bc5, BlockFormat::Bc7] {
                let back = decode(f, 8, 8, &encode(f, 8, 8, &p));
                let channels: &[usize] = match f {
                    BlockFormat::Bc4 => &[0],
                    BlockFormat::Bc5 => &[0, 1],
                    BlockFormat::Bc7 => &[0, 1, 2, 3],
                };
                assert!(worst(&p, &back, channels) <= 1, "{f:?} {colour:?}");
            }
        }
    }

    #[test]
    fn a_smooth_ramp_keeps_its_values_closely() {
        let p = picture(32, 32, |x, y| {
            // LINT-OK: small coordinates.
            #[allow(clippy::cast_possible_truncation)]
            let v = |k: u32| (x * k + y * (8 - k)) as u8;
            [v(1), v(3), v(5), v(7)]
        });
        let bc4 = decode(
            BlockFormat::Bc4,
            32,
            32,
            &encode(BlockFormat::Bc4, 32, 32, &p),
        );
        assert!(worst(&p, &bc4, &[0]) <= 3, "BC4 {}", worst(&p, &bc4, &[0]));
        let bc5 = decode(
            BlockFormat::Bc5,
            32,
            32,
            &encode(BlockFormat::Bc5, 32, 32, &p),
        );
        assert!(worst(&p, &bc5, &[0, 1]) <= 3);
        let bc7 = decode(
            BlockFormat::Bc7,
            32,
            32,
            &encode(BlockFormat::Bc7, 32, 32, &p),
        );
        assert!(
            worst(&p, &bc7, &[0, 1, 2, 3]) <= 12,
            "BC7 {}",
            worst(&p, &bc7, &[0, 1, 2, 3])
        );
    }

    #[test]
    fn colours_along_one_line_come_back_closely_in_bc7() {
        // Every block's texels spread along one line through the four channels, as a picture's
        // colour and its height do: BC7 keeps them within half of one of its sixteen steps across
        // the widest channel (190 levels), and a level of rounding.
        let p = picture(64, 64, |x, y| {
            let t = f32::from(noise(x, y, 0)) / 255.0;
            // LINT-OK: values inside a byte.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let v = |a: f32, b: f32| (a + (b - a) * t) as u8;
            [
                v(40.0, 180.0),
                v(60.0, 150.0),
                v(10.0, 40.0),
                v(30.0, 220.0),
            ]
        });
        let back = decode(
            BlockFormat::Bc7,
            64,
            64,
            &encode(BlockFormat::Bc7, 64, 64, &p),
        );
        assert!(
            worst(&p, &back, &[0, 1, 2, 3]) <= 8,
            "{}",
            worst(&p, &back, &[0, 1, 2, 3])
        );
    }

    #[test]
    fn two_channels_that_vary_apart_keep_their_own_detail_in_bc5() {
        let p = picture(64, 64, |x, y| [noise(x, y, 0), noise(x, y, 1), 0, 255]);
        let back = decode(
            BlockFormat::Bc5,
            64,
            64,
            &encode(BlockFormat::Bc5, 64, 64, &p),
        );
        // Sixteen unrelated values into eight steps of their range: within half a step of 255/7.
        assert!(
            worst(&p, &back, &[0, 1]) <= 19,
            "{}",
            worst(&p, &back, &[0, 1])
        );
        // Each channel is its own: green's values never leak into red.
        let rg = picture(8, 8, |x, _| [if x < 4 { 0 } else { 255 }, 128, 0, 255]);
        let back = decode(BlockFormat::Bc5, 8, 8, &encode(BlockFormat::Bc5, 8, 8, &rg));
        assert_eq!(worst(&rg, &back, &[0, 1]), 0);
    }

    #[test]
    fn a_picture_of_part_blocks_decodes_to_its_own_texels() {
        let p = picture(6, 3, |x, y| {
            // LINT-OK: small coordinates.
            #[allow(clippy::cast_possible_truncation)]
            let v = (x * 40 + y * 10) as u8;
            [v, v, v, 255]
        });
        let blocks_out = encode(BlockFormat::Bc7, 6, 3, &p);
        assert_eq!(blocks_out.len(), 2 * 16);
        let back = decode(BlockFormat::Bc7, 6, 3, &blocks_out);
        assert_eq!(back.len(), p.len());
        assert!(worst(&p, &back, &[0, 1, 2, 3]) <= 6);
    }

    #[test]
    fn a_bc7_block_is_mode_6_with_the_first_index_top_bit_clear() {
        let p = picture(4, 4, |x, y| {
            // LINT-OK: small coordinates.
            #[allow(clippy::cast_possible_truncation)]
            let v = 255 - (x * 60 + y * 5) as u8;
            [v, v / 2, 0, 255]
        });
        let block = encode(BlockFormat::Bc7, 4, 4, &p);
        assert_eq!(block[0] & 0x7F, 0x40, "mode 6");
        // The first texel is the brightest and its index is written with three bits, so the
        // first end colour is the bright one.
        let bits = u128::from_le_bytes(block.try_into().expect("one block"));
        let (first_red, second_red) = ((bits >> 7) & 0x7F, (bits >> 14) & 0x7F);
        assert!(first_red > second_red, "{first_red} {second_red}");
        let back = decode(BlockFormat::Bc7, 4, 4, &encode(BlockFormat::Bc7, 4, 4, &p));
        assert!(worst(&p, &back, &[0, 1, 2, 3]) <= 8);
    }

    #[test]
    fn a_large_picture_compresses_the_same_on_one_thread_or_several() {
        let p = picture(256, 256, |x, y| {
            [noise(x, y, 0), noise(x, y, 1), noise(x, y, 2), 255]
        });
        let many = encode(BlockFormat::Bc7, 256, 256, &p);
        // The blocks one at a time, in order.
        let mut one = Vec::new();
        for by in 0..64 {
            for bx in 0..64 {
                one.extend(bc7_block(&gather(&p, 256, 256, bx, by)));
            }
        }
        assert_eq!(many, one);
    }
}
