//! The iteration file `0xFFFF0001`, encoded as mostly consecutive integer ranges.
//!
//! Present in all four dats. It has no typed-object header: the disk controller reads it
//! directly before the typed-object machinery exists.
//!
//! See `docs/formats/01-dat-container.md`.

use crate::cursor::Cursor;
use crate::error::DatError;

/// The mostly-consecutive int set, read direction.
///
/// `i32 count`, then run-length items until `count` values have been produced: a non-negative dword
/// is a single value, a negative dword `-k` is followed by a dword `first` and expands to
/// `first .. first + k`.
///
/// Single values are written with bit 31 masked off, so the reader restores it when bit 30 is set,
/// as the client's reader does.
pub fn decode(bytes: &[u8]) -> Result<Vec<u32>, DatError> {
    let mut c = Cursor::new(bytes);
    let count = c.i32()?;
    let mut out: Vec<u32> = Vec::new();
    if count <= 0 {
        c.expect_end()?;
        return Ok(out);
    }
    while out.len() < count as usize {
        let v = c.i32()?;
        if v < 0 {
            let start = c.i32()?;
            let n = v.unsigned_abs();
            for k in 0..n {
                out.push((start as u32).wrapping_add(k));
            }
        } else {
            let mut u = v as u32;
            if u & 0x4000_0000 != 0 {
                u |= 0x8000_0000;
            }
            out.push(u);
        }
    }
    c.expect_end()?;
    Ok(out)
}

/// How many values an encoded set holds and the highest of them, without expanding its runs: what
/// a reader that only compares counts needs, at a cost bounded by the bytes rather than by the
/// values they describe. It counts what [`decode`] would produce.
///
/// # Errors
/// As [`decode`]: a truncated set, or bytes after its last item.
pub fn summarize(bytes: &[u8]) -> Result<(u32, u32), DatError> {
    let mut c = Cursor::new(bytes);
    let count = c.i32()?;
    if count <= 0 {
        c.expect_end()?;
        return Ok((0, 0));
    }
    let want = u64::from(count.unsigned_abs());
    let (mut have, mut highest) = (0u64, 0u32);
    while have < want {
        let v = c.i32()?;
        if v < 0 {
            let start = c.i32()? as u32;
            let n = v.unsigned_abs();
            have += u64::from(n);
            highest = highest.max(start.wrapping_add(n - 1));
        } else {
            let mut u = v as u32;
            if u & 0x4000_0000 != 0 {
                u |= 0x8000_0000;
            }
            have += 1;
            highest = highest.max(u);
        }
    }
    c.expect_end()?;
    Ok((u32::try_from(have).unwrap_or(u32::MAX), highest))
}

/// The mostly-consecutive int set, write direction.
///
/// `values` must already be sorted and deduplicated -- the client sorts and
/// deduplicates first, with an unstable `qsort`.
///
/// The count is written first, then items: a run of **three or more** consecutive values collapses
/// to `-run` followed by the first value (native's test is `i - j < -2`), and anything shorter is
/// written one value at a time with bit 31 masked off.
#[must_use]
pub fn encode(values: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4 + 4);
    #[allow(clippy::cast_possible_truncation)]
    out.extend_from_slice(&(values.len() as u32).to_le_bytes());
    let mut i = 0usize;
    while i < values.len() {
        let mut j = i;
        let mut expect = values[i];
        while j < values.len() && values[j] == expect {
            j += 1;
            expect = expect.wrapping_add(1);
        }
        let run = j - i;
        if run >= 3 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            out.extend_from_slice(&(-(run as i32)).to_le_bytes());
            out.extend_from_slice(&values[i].to_le_bytes());
            i = j;
        } else {
            out.extend_from_slice(&(values[i] & 0x7FFF_FFFF).to_le_bytes());
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_counts_what_decode_produces_without_expanding_runs() {
        for list in [
            vec![],
            vec![7],
            (1..=2072).collect::<Vec<u32>>(),
            (1..=990).chain([994, 0x4000_0001]).collect(),
        ] {
            let raw = encode(&list);
            let decoded = decode(&raw).unwrap();
            let (count, highest) = summarize(&raw).unwrap();
            assert_eq!(count as usize, decoded.len());
            assert_eq!(highest, decoded.iter().copied().max().unwrap_or(0));
        }
        // A run claiming two billion values costs no more than its eight bytes.
        let mut huge = 0x7FFF_FFFFu32.to_le_bytes().to_vec();
        huge.extend_from_slice(&(-0x7FFF_FFFFi32).to_le_bytes());
        huge.extend_from_slice(&1u32.to_le_bytes());
        assert_eq!(summarize(&huge).unwrap(), (0x7FFF_FFFF, 0x7FFF_FFFF));
        assert!(summarize(&huge[..8]).is_err(), "a truncated set");
    }

    /// Oracle: the portal dat payload for `0xFFFF0001`, captured byte for byte.
    #[test]
    fn the_retail_portal_iteration_list_decodes_to_one_through_2072() {
        let raw = [
            0x18, 0x08, 0x00, 0x00, // count = 2072
            0xE8, 0xF7, 0xFF, 0xFF, // -2072
            0x01, 0x00, 0x00, 0x00, // first = 1
        ];
        let v = decode(&raw).unwrap();
        assert_eq!(v.len(), 2072);
        assert_eq!(v[0], 1);
        assert_eq!(v[2071], 2072);
    }

    /// The same oracle, written rather than read: writing the iteration list for the retail portal set
    /// has to produce exactly the twelve bytes that are in the file.
    #[test]
    fn encoding_one_through_2072_reproduces_the_retail_portal_payload() {
        let set: Vec<u32> = (1..=2072).collect();
        assert_eq!(
            encode(&set),
            [
                0x18, 0x08, 0x00, 0x00, // count = 2072
                0xE8, 0xF7, 0xFF, 0xFF, // -2072
                0x01, 0x00, 0x00, 0x00, // first = 1
            ]
        );
    }

    /// A run of two does not collapse (`i - j < -2` is false at -2), a run of three does.
    #[test]
    fn runs_shorter_than_three_are_written_one_value_at_a_time() {
        assert_eq!(decode(&encode(&[4, 5])).unwrap(), vec![4, 5]);
        assert_eq!(encode(&[4, 5]).len(), 4 + 4 + 4);
        assert_eq!(encode(&[4, 5, 6]).len(), 4 + 4 + 4);
        assert_eq!(decode(&encode(&[4, 5, 6])).unwrap(), vec![4, 5, 6]);
        let mixed = vec![1, 2, 3, 4, 9, 11, 12, 13, 99];
        assert_eq!(decode(&encode(&mixed)).unwrap(), mixed);
    }

    #[test]
    fn a_single_value_item_is_a_run_of_one() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&2i32.to_le_bytes());
        raw.extend_from_slice(&7i32.to_le_bytes());
        raw.extend_from_slice(&9i32.to_le_bytes());
        assert_eq!(decode(&raw).unwrap(), vec![7, 9]);
    }
}
