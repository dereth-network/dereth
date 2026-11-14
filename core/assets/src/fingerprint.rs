//! What a retail picture or model is, told apart from what another era's files hold under the same
//! id: an art pack's entry names the record it was made from by its fingerprint, and the client
//! draws the entry whose fingerprint matches the world's record ([`crate::art_pack`]).
//!
//! - A **picture**'s fingerprint is its size and a 64-bit average hash: the picture boxed down to
//!   8 by 8 cells of luminance, a bit for each cell brighter than the cells' mean, the top row
//!   first. Two pictures match when their sides are in the same proportion and their hashes
//!   differ in at most [`PICTURE_BITS_APART`] bits: a picture redrawn larger or repainted over
//!   the same layout matches, one with other art does not.
//! - A **model**'s fingerprint is a hash of its parts' vertex and polygon counts and bounds to a
//!   centimetre. Two models match when the hashes are equal.
//!
//! Both are facts about the art, so a pack holding them holds none of it. In text (a manifest's
//! `fingerprint` key) a picture's reads `picture 64x64 0123456789abcdef` and a model's
//! `model 0123456789abcdef`.

use dereth_dat::{store::RetailDatStore, DbType};
use dereth_primitives::DataId;

use crate::{Decode, GfxObj, Setup};

/// How many bits two pictures' hashes may differ in and still match. Measured on the pack's
/// pictures against their 1999 and 2005 records, it takes 94% of those whose 16 by 16 luminance
/// correlates by 0.8 or more, 19% of those between 0.5 and 0.8, and none below.
pub const PICTURE_BITS_APART: u32 = 10;

/// A record's fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fingerprint {
    /// A picture's size and average hash.
    Picture { width: u32, height: u32, hash: u64 },
    /// A model's hash.
    Model(u64),
}

impl Fingerprint {
    /// The fingerprint of a picture `width` by `height`, its texels blue, green, red and alpha,
    /// rows from the top.
    #[must_use]
    pub fn of_picture(width: u32, height: u32, bgra: &[u8]) -> Self {
        let (w, h) = (width.max(1) as usize, height.max(1) as usize);
        let mut sum = [0f64; 64];
        let mut count = [0u32; 64];
        for (i, p) in bgra.as_chunks::<4>().0.iter().take(w * h).enumerate() {
            let (x, y) = (i % w, i / w);
            let cell = (y * 8 / h) * 8 + x * 8 / w;
            sum[cell] +=
                0.299 * f64::from(p[2]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[0]);
            count[cell] += 1;
        }
        let cells: Vec<f64> = sum
            .iter()
            .zip(count)
            .map(|(s, n)| if n == 0 { 0.0 } else { s / f64::from(n) })
            .collect();
        let mean = cells.iter().sum::<f64>() / 64.0;
        let hash = cells
            .iter()
            .enumerate()
            .fold(0u64, |h, (k, &v)| if v > mean { h | 1 << k } else { h });
        Self::Picture {
            width,
            height,
            hash,
        }
    }

    /// The fingerprint of a model of `parts`: each part's vertex count, polygon count and bounds
    /// (the least corner, then the greatest), metres.
    #[must_use]
    pub fn of_parts(parts: &[(usize, usize, [f32; 6])]) -> Self {
        // FNV-1a: the same in every build, with nothing to depend on.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for &b in bytes {
                h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        eat(&(parts.len() as u64).to_le_bytes());
        for (vertices, polygons, bounds) in parts {
            eat(&(*vertices as u64).to_le_bytes());
            eat(&(*polygons as u64).to_le_bytes());
            for b in bounds {
                let cm = dereth_primitives::num::to_i32((b * 100.0).round());
                eat(&cm.to_le_bytes());
            }
        }
        Self::Model(h)
    }

    /// The fingerprint of the setup or graphics object `id` in `store`, when it reads.
    #[must_use]
    pub fn of_model(store: &RetailDatStore, id: DataId) -> Option<Self> {
        let gfx = |id: DataId| -> Option<(usize, usize, [f32; 6])> {
            let bytes = store.read_portal(id).ok()?;
            let g = GfxObj::decode_payload_in(store.era_of(id), id, &bytes).ok()?;
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &g.vertex_array.vertices {
                for (k, c) in [v.position.x, v.position.y, v.position.z]
                    .into_iter()
                    .enumerate()
                {
                    lo[k] = lo[k].min(c);
                    hi[k] = hi[k].max(c);
                }
            }
            if g.vertex_array.vertices.is_empty() {
                (lo, hi) = ([0.0; 3], [0.0; 3]);
            }
            Some((
                g.vertex_array.vertices.len(),
                g.polygons.len(),
                [lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]],
            ))
        };
        let parts = match dereth_dat::divine_type(id)? {
            DbType::GfxObj => vec![gfx(id)?],
            DbType::Setup => {
                let bytes = store.read_portal(id).ok()?;
                let s = Setup::decode_payload_in(store.era_of(id), id, &bytes).ok()?;
                s.parts
                    .iter()
                    .map(|&p| gfx(p))
                    .collect::<Option<Vec<_>>>()?
            }
            _ => return None,
        };
        Some(Self::of_parts(&parts))
    }

    /// Whether a record of fingerprint `other` is the one this fingerprint was taken from.
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Picture {
                    width: w0,
                    height: h0,
                    hash: a,
                },
                Self::Picture {
                    width: w1,
                    height: h1,
                    hash: b,
                },
            ) => {
                u64::from(*w0) * u64::from(*h1) == u64::from(*w1) * u64::from(*h0)
                    && (a ^ b).count_ones() <= PICTURE_BITS_APART
            }
            (Self::Model(a), Self::Model(b)) => a == b,
            _ => false,
        }
    }

    /// The fingerprint `text` spells, as [`std::fmt::Display`] writes it.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let hex = |s: &str| {
            (s.len() == 16)
                .then(|| u64::from_str_radix(s, 16).ok())
                .flatten()
        };
        match words.as_slice() {
            ["picture", size, hash] => {
                let (w, h) = size.split_once('x')?;
                Some(Self::Picture {
                    width: w.parse().ok()?,
                    height: h.parse().ok()?,
                    hash: hex(hash)?,
                })
            }
            ["model", hash] => Some(Self::Model(hex(hash)?)),
            _ => None,
        }
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Picture {
                width,
                height,
                hash,
            } => write!(f, "picture {width}x{height} {hash:016x}"),
            Self::Model(hash) => write!(f, "model {hash:016x}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture `w` by `h` whose luminance at `(x, y)` is `f(x, y)`, kept to a byte.
    fn picture(w: u32, h: u32, f: impl Fn(u32, u32) -> u32) -> Vec<u8> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                let v = u8::try_from(f(x, y).min(255)).unwrap_or(u8::MAX);
                [v, v, v, 255]
            })
            .collect()
    }

    #[test]
    fn a_picture_drawn_larger_matches_and_other_art_does_not() {
        let small = Fingerprint::of_picture(32, 32, &picture(32, 32, |x, _| x * 8));
        let large = Fingerprint::of_picture(64, 64, &picture(64, 64, |x, _| x * 4));
        let turned = Fingerprint::of_picture(64, 64, &picture(64, 64, |_, y| y * 4));
        assert!(small.matches(&large));
        assert!(!small.matches(&turned));
    }

    #[test]
    fn a_picture_of_other_proportions_does_not_match() {
        let square = Fingerprint::of_picture(32, 32, &picture(32, 32, |x, _| x * 8));
        let wide = Fingerprint::of_picture(64, 32, &picture(64, 32, |x, _| x * 4));
        assert!(!square.matches(&wide));
    }

    #[test]
    fn a_few_cells_apart_still_match() {
        let a = Fingerprint::Picture {
            width: 64,
            height: 64,
            hash: 0,
        };
        let near = Fingerprint::Picture {
            width: 128,
            height: 128,
            hash: (1 << PICTURE_BITS_APART) - 1,
        };
        let far = Fingerprint::Picture {
            width: 64,
            height: 64,
            hash: (1 << (PICTURE_BITS_APART + 1)) - 1,
        };
        assert!(a.matches(&near));
        assert!(!a.matches(&far));
    }

    #[test]
    fn models_match_on_their_parts_alone() {
        let tree = Fingerprint::of_parts(&[(30, 28, [-1.0, -1.0, 0.0, 1.0, 1.0, 9.5])]);
        let again = Fingerprint::of_parts(&[(30, 28, [-1.0, -1.0, 0.0, 1.0, 1.0, 9.501])]);
        let three_d = Fingerprint::of_parts(&[
            (26, 42, [-1.0, -1.0, 0.0, 1.0, 1.0, 9.5]),
            (194, 210, [-3.0, -3.0, 4.0, 3.0, 3.0, 12.0]),
        ]);
        assert!(tree.matches(&again));
        assert!(!tree.matches(&three_d));
        assert!(!tree.matches(&Fingerprint::Picture {
            width: 1,
            height: 1,
            hash: 0
        }));
    }

    #[test]
    fn the_text_reads_back() {
        for f in [
            Fingerprint::Picture {
                width: 128,
                height: 64,
                hash: 0x0123_4567_89ab_cdef,
            },
            Fingerprint::Model(0xfedc_ba98_7654_3210),
        ] {
            assert_eq!(Fingerprint::parse(&f.to_string()), Some(f));
        }
        assert_eq!(Fingerprint::parse("picture 64 0123456789abcdef"), None);
        assert_eq!(Fingerprint::parse("model 12"), None);
    }
}
