//! [`SkyTints`]: a world's own colors for its sky, a record its overlay may carry beside the
//! region (`docs/formats/15-region.md`, "A world's sky tints").
//!
//! The region's sky gives each sky object a picture and, at each time of day, a brightness and a
//! transparency, but no color. A world whose sky is colored by the hour (a blue dome by day, an
//! orange gradient at dusk) carries this record: for each day group of the region's sky and each
//! of its times of day, a color for each sky object, and how much of the time's fog color veils
//! the sky. A world without it, as every retail world is, draws its sky as the region alone says.
//!
//! The record is not one of the game's: no retail file holds it. Its id ([`SKY_TINTS`]) is the one
//! after the region's, in the regions' own range, so a world's overlay carries it, a server
//! patches it into clients and a client keeps it as it does the region; a client that does not
//! read it keeps it unread, and nothing reads a region there.

use dereth_dat::Cursor;
use dereth_primitives::DataId;

use crate::error::AssetError;

/// The sky tints' record id, beside the region's (`0x13000000`) in the portal file.
pub const SKY_TINTS: DataId = DataId(0x1300_0001);

/// The record layout this crate reads and writes.
pub const VERSION: u32 = 1;

/// A world's sky tints, a day of them for each of the region's day groups in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkyTints {
    pub days: Vec<TintedDay>,
}

/// One day group's tints: a row for each of its times of day, in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TintedDay {
    /// How many sky objects each row colors: the day group's.
    pub objects: usize,
    pub times: Vec<TintedTime>,
}

/// One time of day's tints.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TintedTime {
    /// How much of the time's fog color is laid over the sky drawn before the land, 0 to 1.
    pub veil: f32,
    /// Each sky object's color, `0xAARRGGBB`: its color multiplies the object's drawn color and
    /// its alpha the object's opacity. As many as the day's objects.
    pub tints: Vec<u32>,
}

/// One sky object's tint at a moment: red, green, blue and alpha, 0 to 1.
pub type Tint = [f32; 4];

/// A packed `0xAARRGGBB` color as red, green, blue and alpha, 0 to 1.
#[must_use]
pub fn unpack(c: u32) -> Tint {
    let ch = |s: u32| f32::from(u8::try_from((c >> s) & 0xFF).unwrap_or(0)) / 255.0;
    [ch(16), ch(8), ch(0), ch(24)]
}

impl TintedDay {
    /// Whether it fits a day group of `objects` sky objects and `times` times of day.
    #[must_use]
    pub fn fits(&self, objects: usize, times: usize) -> bool {
        self.objects == objects && self.times.len() == times
    }

    /// The tints and the veil between rows `a` and `b`, a share `f` of the way from `a` to `b`:
    /// each channel carried straight across, as the sky's other values are. `None` for a row
    /// it does not hold.
    #[must_use]
    pub fn between(&self, a: usize, b: usize, f: f32) -> Option<(Vec<Tint>, f32)> {
        let (ra, rb) = (self.times.get(a)?, self.times.get(b)?);
        let lerp = |x: f32, y: f32| x + (y - x) * f;
        let tints = ra
            .tints
            .iter()
            .zip(&rb.tints)
            .map(|(&x, &y)| {
                let (x, y) = (unpack(x), unpack(y));
                [
                    lerp(x[0], y[0]),
                    lerp(x[1], y[1]),
                    lerp(x[2], y[2]),
                    lerp(x[3], y[3]),
                ]
            })
            .collect();
        Some((tints, lerp(ra.veil, rb.veil).clamp(0.0, 1.0)))
    }
}

impl SkyTints {
    /// The record's bytes: its id, its layout's version, the count of days, then each day's
    /// count of times and of objects, and each time's veil and its objects' colors.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let u32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        u32(&mut out, SKY_TINTS.raw());
        u32(&mut out, VERSION);
        u32(&mut out, count(self.days.len()));
        for d in &self.days {
            u32(&mut out, count(d.times.len()));
            u32(&mut out, count(d.objects));
            for t in &d.times {
                u32(&mut out, t.veil.to_bits());
                for k in 0..d.objects {
                    u32(&mut out, t.tints.get(k).copied().unwrap_or(0xFFFF_FFFF));
                }
            }
        }
        out
    }

    /// The tints a record's bytes hold.
    ///
    /// # Errors
    /// When the record is not one: another id, another version, too short, or bytes left over.
    pub fn decode(bytes: &[u8]) -> Result<Self, AssetError> {
        let mut c = Cursor::new(bytes);
        let id = c.data_id()?;
        if id != SKY_TINTS {
            return Err(AssetError::IdEchoMismatch {
                expected: SKY_TINTS,
                found: id,
            });
        }
        let version = c.u32()?;
        if version != VERSION {
            return Err(AssetError::Unsupported {
                what: "sky tints version",
                value: version,
            });
        }
        let days = c.u32()?;
        let mut out = Self::default();
        for _ in 0..days {
            let (times, objects) = (c.u32()?, c.u32()? as usize);
            // Each row is at least its veil: a count past the bytes left is no record.
            if times as usize > c.remaining() / 4 || objects > c.remaining() / 4 {
                return Err(AssetError::Unsupported {
                    what: "sky tints count",
                    value: times.max(u32::try_from(objects).unwrap_or(u32::MAX)),
                });
            }
            let mut day = TintedDay {
                objects,
                times: Vec::with_capacity(times as usize),
            };
            for _ in 0..times {
                let veil = c.f32()?;
                let tints = (0..objects).map(|_| c.u32()).collect::<Result<_, _>>()?;
                day.times.push(TintedTime { veil, tints });
            }
            out.days.push(day);
        }
        c.expect_end()?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tints() -> SkyTints {
        SkyTints {
            days: vec![
                TintedDay {
                    objects: 2,
                    times: vec![
                        TintedTime {
                            veil: 0.5,
                            tints: vec![0xFF20_4080, 0x00FF_8000],
                        },
                        TintedTime {
                            veil: 0.0,
                            tints: vec![0x80FF_FFFF, 0xFF00_0000],
                        },
                    ],
                },
                TintedDay {
                    objects: 0,
                    times: vec![TintedTime {
                        veil: 1.0,
                        tints: Vec::new(),
                    }],
                },
            ],
        }
    }

    /// Behaviour: rendering.sky.a-worlds-sky-tints-color-its-sky-objects-by-the-hour
    #[test]
    fn a_worlds_sky_tints_read_back_as_written() {
        let t = tints();
        let b = t.encode();
        assert_eq!(&b[..4], &SKY_TINTS.raw().to_le_bytes());
        assert_eq!(SkyTints::decode(&b).unwrap(), t);
        // Anything shorter, longer or of another layout is no record.
        assert!(SkyTints::decode(&b[..b.len() - 1]).is_err());
        let mut longer = b.clone();
        longer.push(0);
        assert!(SkyTints::decode(&longer).is_err());
        let mut other = b.clone();
        other[4] = 2;
        assert!(SkyTints::decode(&other).is_err());
        let mut huge = b;
        huge[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(SkyTints::decode(&huge).is_err());
    }

    /// Behaviour: rendering.sky.a-worlds-sky-tints-color-its-sky-objects-by-the-hour
    #[test]
    fn a_tint_between_two_times_is_carried_straight_across_each_channel() {
        let d = &tints().days[0];
        assert!(d.fits(2, 2));
        assert!(!d.fits(3, 2) && !d.fits(2, 3));
        let (at_a, veil) = d.between(0, 1, 0.0).unwrap();
        assert_eq!(veil, 0.5);
        assert_eq!(at_a[0], unpack(0xFF20_4080));
        let (half, veil) = d.between(0, 1, 0.5).unwrap();
        assert_eq!(veil, 0.25);
        let want = [
            (32.0 / 255.0 + 1.0) / 2.0,
            (64.0 / 255.0 + 1.0) / 2.0,
            (128.0 / 255.0 + 1.0) / 2.0,
            (1.0 + 128.0 / 255.0) / 2.0,
        ];
        for k in 0..4 {
            assert!((half[0][k] - want[k]).abs() < 1e-6, "{half:?}");
        }
        assert_eq!(half[1][3], 0.5, "alpha 0 to 1");
        assert!(d.between(0, 2, 0.5).is_none());
    }
}
