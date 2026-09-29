//! The sound-table row pick, and the probability roll.
//!
//! Sound-table records are described in `docs/formats/21-sound-tables.md`.
//!
//! **The last row of a multi-row entry is unreachable.** The index is
//! `trunc(roll_f32(0, 1) * (rows - 1))`, and the draw is capped at `RNMX = 0.99999988`,
//! so for `n >= 2` the index can never exceed `n - 2`. This was an internal contradiction in the
//! knowledge base and is settled by what the client actually does:
//!
//! it draws `u` from `ran2` over `[0, 1)`, multiplies it by `n - 1` in `f32` (`n` being the
//! table's row count), truncates toward zero, and plays no sound when that index, compared as
//! unsigned, is not below `n`.
//!
//! Sounds no player has ever heard must stay unheard.

use dereth_assets::audio::{SoundEntry, SoundTable};
use dereth_primitives::num::{rng::Ran2, to_i32_f64};

/// The row pick, and the same expression inlined into both of the ambient play entries.
///
/// `((float)rand() * 3.051851e-05f) < p`, where `rand` is the **CRT** generator, not `ran2` — the two
/// must never be merged. `srand((unsigned)time(NULL))` runs during sound start-up and
/// **only if DirectSound initialised**, so on a machine with no sound card the CRT PRNG keeps its
/// default seed of 1.
///
/// The scale is a **32-bit** `3.051851e-05` whose exact value is
/// `3.0518509447574615e-05` — a hair under `1/32767`.
///
/// The product is formed at extended precision (an integer load, then a multiply by the 32-bit
/// scale), i.e. at f64 precision, giving `32767 * scale = 0.9999999990686774`. That is strictly
/// less than 1.0, so a row with `probability = 1.0` **always** plays while `0.99999` can fail.
/// Computing the product in f32 instead rounds it up to exactly 1.0 and silently breaks
/// `probability = 1.0`, which is the common case in the shipped tables — hence the `f64::from`
/// below.
pub const PROBABILITY_SCALE: f32 = 3.051_851e-5;

/// The probability roll.
#[must_use]
pub fn play_probability(rand_value: u16, p: f32) -> bool {
    f64::from(rand_value) * f64::from(PROBABILITY_SCALE) < f64::from(p)
}

/// Look a `SoundType` up in a sound table.
///
/// The lookup searches **only the root's immediate children** — it does not recurse — so the
/// arbitrarily-recursive format is a generalisation the client never exercises. Retail data is always
/// exactly two levels: a root with key 0, one child per `SoundType`.
#[must_use]
pub fn lookup(table: &SoundTable, stype: u32) -> Option<&[SoundEntry]> {
    let root = table.nodes.first()?;
    root.children
        .iter()
        .filter_map(|&i| table.nodes.get(i as usize))
        .find(|n| n.key == stype)
        .map(|n| n.data.as_slice())
}

/// The row index selected from a `ran2` draw.
/// Returns `None` when the index lands outside the row list — retail's unsigned bounds check, which
/// for `n == 0` rejects everything. Note the multiplier is `n - 1`, not `n`.
#[must_use]
pub fn row_index(n: usize, u: f32) -> Option<usize> {
    if n == 0 {
        return None;
    }
    // `n - 1` loaded as a signed integer, multiplied by the float roll result widened to
    // double precision, then truncated to an integer.
    let n_minus_1 = i32::try_from(n).unwrap_or(i32::MAX) - 1;
    let i = to_i32_f64(f64::from(n_minus_1) * f64::from(u));
    // An *unsigned* compare, so a negative index is rejected too.
    let i = usize::try_from(i).ok()?;
    if i >= n {
        None
    } else {
        Some(i)
    }
}

/// Resolve a `SoundType` against a table to a wave and its row data.
///
/// Draws one `ran2` value — the same generator the ambient scheduler uses, and drawn from the same
/// stream, so the order of table picks and ambient timings is observable together.
///
/// Returns the chosen row. A row whose `sound_id_` is 0 yields `None`, exactly as the
/// client's own zero test does.
#[must_use]
pub fn get_sound(table: &SoundTable, stype: u32, rng: &mut Ran2) -> Option<SoundEntry> {
    let rows = lookup(table, stype)?;
    if rows.is_empty() {
        return None;
    }
    // The random roll uses the closed interval from 0.0 to 1.0.
    let u = rng.roll_f32(0.0, 1.0);
    let i = row_index(rows.len(), u)?;
    let row = rows[i];
    if row.sound_id.raw() == 0 {
        return None;
    }
    Some(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::audio::SoundTableNode;
    use dereth_primitives::DataId;

    fn table(rows_per_key: &[(u32, usize)]) -> SoundTable {
        let mut nodes = vec![SoundTableNode {
            key: 0,
            data: Vec::new(),
            children: Vec::new(),
        }];
        for (k, n) in rows_per_key {
            let idx = u32::try_from(nodes.len()).expect("small");
            nodes.push(SoundTableNode {
                key: *k,
                data: (0..*n)
                    .map(|i| SoundEntry {
                        sound_id: DataId(0x0A00_0000 + u32::try_from(i).expect("small") + 1),
                        priority: 0.0,
                        probability: 1.0,
                        volume: 1.0,
                    })
                    .collect(),
                children: Vec::new(),
            });
            nodes[0].children.push(idx);
        }
        SoundTable {
            id: DataId(0x2000_0001),
            nodes,
        }
    }

    /// The shipped bug, stated directly. Oracle: the recovered row-selection calculation quoted
    /// in this module's docs; contract 12.3.
    #[test]
    fn the_last_row_of_a_multi_row_entry_is_unreachable() {
        // The random draw is capped at RNMX = 0.99999988, so u never reaches 1.0.
        for n in 2..=8usize {
            for step in 0..=100_000u32 {
                let u = f32::from(u16::try_from(step % 65_536).expect("masked")) / 65_536.0;
                let u = u.min(crate::ambient::place::RNMX_F32);
                let i = row_index(n, u).expect("in range");
                assert!(
                    i < n - 1,
                    "n = {n}, u = {u}: index {i} reached the last row"
                );
            }
        }
    }

    /// For a single-row entry the index is 0 and the row *is* used: `trunc(u * 0) == 0`.
    #[test]
    fn a_single_row_entry_is_always_reachable() {
        for step in 0..1000u32 {
            let u = f32::from(u16::try_from(step).expect("small")) / 1000.0;
            assert_eq!(row_index(1, u), Some(0));
        }
    }

    /// Driven by the real `ran2`, which is what the client draws from. Oracle: `dereth_primitives::num::Ran2`,
    /// which reproduces the client's generator, plus the row-pick rule above.
    #[test]
    fn a_hundred_thousand_ran2_draws_never_return_the_last_row() {
        let t = table(&[(66, 2), (67, 3), (68, 5)]);
        let mut rng = Ran2::new(20_130_918);
        for key in [66u32, 67, 68] {
            let n = lookup(&t, key).expect("present").len();
            let mut seen = vec![false; n];
            for _ in 0..100_000 {
                let u = rng.roll_f32(0.0, 1.0);
                let i = row_index(n, u).expect("in range");
                seen[i] = true;
            }
            assert!(!seen[n - 1], "key {key}: the last of {n} rows was reached");
            assert!(
                seen[..n - 1].iter().all(|&s| s),
                "key {key}: every other row must be reachable"
            );
        }
    }

    /// A `SoundType` with no entry, and a row whose wave id is 0, both produce no sound.
    #[test]
    fn a_missing_key_or_a_zero_sound_id_yields_nothing() {
        let mut t = table(&[(66, 1)]);
        let mut rng = Ran2::new(1);
        assert!(get_sound(&t, 99, &mut rng).is_none(), "no such SoundType");
        t.nodes[1].data[0].sound_id = DataId(0);
        assert!(get_sound(&t, 66, &mut rng).is_none(), "sound_id_ == 0");
        assert_eq!(row_index(0, 0.5), None, "num_stdatas_ == 0");
    }

    /// Oracle: the recovered `play_probability` rule — a row with `probability_ = 1.0` always plays,
    /// while `p = 0.99999` can fail.
    #[test]
    fn probability_one_always_passes_and_the_scale_is_just_under_one() {
        assert!(
            play_probability(32_767, 1.0),
            "the largest rand() still passes p = 1.0"
        );
        assert!(!play_probability(32_767, 0.99999), "but not p = 0.99999");
        assert!(!play_probability(0, 0.0), "p = 0 never plays");
        assert!(
            play_probability(0, 0.000_001),
            "rand() == 0 passes any positive p"
        );
    }

    /// Lookup reads only the root's immediate children, never deeper. Oracle: the client's lookup
    /// and the two-level structure of the decoded sound table.
    #[test]
    fn lookup_does_not_recurse_into_grandchildren() {
        let mut t = table(&[(66, 1)]);
        // Hang a grandchild with key 70 off the child.
        let idx = u32::try_from(t.nodes.len()).expect("small");
        t.nodes.push(SoundTableNode {
            key: 70,
            data: vec![SoundEntry {
                sound_id: DataId(0x0A00_0500),
                priority: 0.0,
                probability: 1.0,
                volume: 1.0,
            }],
            children: Vec::new(),
        });
        t.nodes[1].children.push(idx);
        assert!(lookup(&t, 66).is_some());
        assert!(
            lookup(&t, 70).is_none(),
            "a grandchild is invisible to Lookup"
        );
    }
}
