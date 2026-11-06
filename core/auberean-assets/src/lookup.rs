//! Finding the key at or before a time in a track, the client's way.
//!
//! A track's keys are found through a table of buckets laid over the clip's span: the span is cut
//! into as many equal buckets as the keys' average spacing fits into it (at most 50), and each
//! bucket lists the keys from the last one before it to the last one within 0.0002 of its end. A
//! time picks its bucket and is searched for within it only. The answer is the last key at or
//! before the time in nearly every case; where a bucket's range does not reach that key (keys
//! closer together than the slack at a bucket's edge), the client's answer is kept.

/// The most buckets a track gets.
pub const MAX_BUCKETS: u32 = 50;

/// The slack at a bucket's end.
const SLACK: f32 = 0.0002;

/// A track's lookup table: the buckets per second and each bucket's key range.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LookupTable {
    pub time_scale: f32,
    pub buckets: Vec<(u32, u32)>,
}

impl LookupTable {
    /// The table of keys at `times` (ascending, in seconds). `None` when there are no keys or the
    /// first is not at time 0 (the client leaves such a track without a table).
    // D2-15280, D2-15281, D2-15282, D2-15264, D2-15265, D2-15266, D2-15267, D2-15268, D2-15269, D2-15270, D2-15271
    #[must_use]
    pub fn new(times: &[f32]) -> Option<Self> {
        let n = times.len();
        if n == 0 || times[0] != 0.0 {
            return None;
        }
        let last = times[n - 1];
        let mut count: u32 = 1;
        if n > 1 {
            let mut sum = 0.0f32;
            for w in times.windows(2) {
                sum += w[1] - w[0];
            }
            #[allow(clippy::cast_precision_loss)]
            let average = sum / n as f32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let c = f64::from(last / average).ceil() as i64 as u32;
            count = c.min(MAX_BUCKETS);
        }
        #[allow(clippy::cast_precision_loss)]
        let fcount = count as f32;
        let time_scale = if last == 0.0 {
            0.0
        } else {
            fcount * (1.0 / last)
        };
        let step = last / fcount;
        let mut edge = step;
        let mut buckets: Vec<(u32, u32)> = Vec::with_capacity(count as usize);
        let mut i: u32 = 0;
        let n32 = u32::try_from(n).unwrap_or(u32::MAX);
        loop {
            let mut b = (i.saturating_sub(1), i);
            while i < n32 {
                let t = times[i as usize];
                if t > edge + SLACK || t.is_nan() {
                    break;
                }
                i += 1;
                b.1 += 1;
            }
            buckets.push(b);
            edge += step;
            if i >= n32 {
                break;
            }
        }
        while buckets.len() < count as usize {
            buckets.push((i, i + 1));
        }
        Some(Self {
            time_scale,
            buckets,
        })
    }

    /// The index of the last key at or before `t` within `t`'s bucket: one before the bucket's
    /// range when every key in it is later (so `u32::MAX` before the first key).
    // D2-15247, D2-15248, D2-15249
    #[must_use]
    pub fn find(&self, times: &[f32], t: f32) -> u32 {
        self.find_by(|k| times.get(k).copied().unwrap_or(f32::INFINITY), t)
    }

    /// [`Self::find`], with each key's time given by `at` (infinite past the last).
    #[must_use]
    pub fn find_by(&self, at: impl Fn(usize) -> f32, t: f32) -> u32 {
        let last = u32::try_from(self.buckets.len())
            .unwrap_or(1)
            .wrapping_sub(1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = (f64::from(t * self.time_scale - 0.499_999_88).round_ties_even() as i64) as u32;
        let (mut lo, mut hi) = self.buckets[b.min(last) as usize];
        let at = |k: u32| at(k as usize);
        while lo < hi {
            let mid = (hi + lo) >> 1;
            let f = at(mid);
            if f <= t {
                if t <= f {
                    return mid;
                }
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        hi.wrapping_sub(1)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the table's answers checked against a plain search)
    use super::*;

    #[test]
    fn the_bucketed_search_finds_the_last_key_at_or_before_a_time() {
        let times = [0.0, 0.1, 0.25, 0.3, 0.6, 0.95, 1.0];
        let table = LookupTable::new(&times).expect("starts at 0");
        assert!(table.buckets.len() <= MAX_BUCKETS as usize);
        for k in 0..=200 {
            let t = k as f32 * 0.005;
            let want = times.partition_point(|&x| x <= t) - 1;
            assert_eq!(table.find(&times, t) as usize, want, "t = {t}");
        }
        assert!(
            LookupTable::new(&[0.5, 1.0]).is_none(),
            "the first key must be at 0"
        );
    }
}
