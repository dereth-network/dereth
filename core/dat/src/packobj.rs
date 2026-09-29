//! `PackObj`-side primitives.
//!
//! `PackObj` is the AC1 network format, and most portal and all cell dat types reach the byte
//! stream through it. What distinguishes it
//! from `Archive` is that it does its own alignment by hand: `ALIGN_PTR` rounds the cursor up to 4
//! at points the reader chooses, and nowhere else.
//!
//! See `docs/formats/03-serialisation-primitives.md`.

use crate::archive::HashHeader;
use crate::cursor::Cursor;
use crate::error::DatError;

/// The `PackableHashTable` format has one `u32` header holding the element count in the
/// low half and the bucket count in the high half. This is the *only* one of the
/// three hash headers that packs both numbers into a single dword.
///
/// A table with no buckets is read as empty, and only an empty one may say so: a zero bucket
/// count beside a non-zero element count is refused, as the client refuses it, rather than read
/// as entries with nowhere to go.
pub fn packable_hash_table_header(c: &mut Cursor<'_>) -> Result<HashHeader, DatError> {
    let head = c.u32()?;
    let (buckets, count) = (head >> 16, head & 0xFFFF);
    if buckets == 0 && count != 0 {
        return Err(DatError::BadHashTableHeader { buckets, count });
    }
    Ok(HashHeader {
        buckets,
        count,
        bucket_index: None,
    })
}

/// The `PackableList` format has a plain `u32` count, then the elements in list order. No
/// alignment of its own; the element type does whatever it does.
pub fn packable_list_count(c: &mut Cursor<'_>) -> Result<u32, DatError> {
    c.u32()
}

/// Read `n` elements with `f`, which is the shape almost every `PackObj` body has.
pub fn read_n<'a, T, F>(c: &mut Cursor<'a>, n: usize, mut f: F) -> Result<Vec<T>, DatError>
where
    F: FnMut(&mut Cursor<'a>) -> Result<T, DatError>,
{
    // No `with_capacity(n)`: `n` comes straight off the wire, and a corrupt count would otherwise
    // let a malformed file allocate gigabytes before the first bounds check fires.
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(f(c)?);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A packed hash table that declares entries in no buckets is refused; one that declares
    /// neither is an empty table.
    #[test]
    fn a_packed_hash_table_with_entries_but_no_buckets_is_refused() {
        let bad = 3u32.to_le_bytes();
        assert!(matches!(
            packable_hash_table_header(&mut Cursor::new(&bad)),
            Err(DatError::BadHashTableHeader {
                buckets: 0,
                count: 3
            })
        ));
        let empty = 0u32.to_le_bytes();
        let h = packable_hash_table_header(&mut Cursor::new(&empty)).expect("empty is fine");
        assert_eq!((h.buckets, h.count), (0, 0));
    }

    /// The packed header stores bucket count in the high 16 bits and entry count in the low 16.
    /// Unlike the archive form, this header contains a bucket count rather than a bucket-size index.
    #[test]
    fn packable_hash_table_header_splits_one_dword() {
        // 0x001F_0003 = 31 buckets, 3 entries.
        let buf = 0x001F_0003u32.to_le_bytes();
        let mut c = Cursor::new(&buf);
        let h = packable_hash_table_header(&mut c).unwrap();
        assert_eq!(h.buckets, 31);
        assert_eq!(h.count, 3);
        assert_eq!(h.bucket_index, None);
        c.expect_end().unwrap();
    }

    /// Contract 9.6 again, from the other side: the `PackObj` header and the `Archive` header read
    /// entirely different numbers out of the same bytes.
    #[test]
    fn the_packobj_header_is_not_an_archive_header() {
        let buf = [0x07u8, 0x83, 0x69, 0x00];
        let pack = packable_hash_table_header(&mut Cursor::new(&buf)).unwrap();
        let arch = crate::archive::intrusive_hash_table_header(&mut Cursor::new(&buf)).unwrap();
        assert_eq!((pack.buckets, pack.count), (0x0069, 0x8307));
        assert_eq!((arch.buckets, arch.count), (1531, 873));
    }

    #[test]
    fn read_n_stops_at_the_first_failure() {
        let buf = [1u8, 2, 3];
        let mut c = Cursor::new(&buf);
        let r = read_n(&mut c, 4, Cursor::u8);
        assert!(matches!(r, Err(DatError::Overrun(1))));
    }
}
