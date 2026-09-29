//! `Archive`-side primitives that are not simple cursor reads.
//!
//! The cursor itself carries the compressed integer and both string forms (see [`crate::Cursor`]);
//! what lives here is the hash-table bucket primes, the two `Archive` hash
//! headers, and the versioned archive header.
//!
//! See `docs/formats/03-serialisation-primitives.md`.

use crate::cursor::Cursor;
use crate::error::DatError;

/// The 23 bucket-count primes used by the intrusive hash-table format.
///
/// These are the exact bucket sizes used by the archive hash table.
pub const BUCKET_SIZES: [u32; 23] = [
    11, 23, 47, 89, 191, 383, 761, 1531, 3067, 6143, 12281, 24571, 49139, 98299, 196_597, 393_209,
    786_431, 1_572_853, 3_145_721, 6_291_449, 12_582_893, 25_165_813, 50_331_599,
];

/// A hash-table header, whichever of the three forms produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashHeader {
    /// The bucket count the writer's table had. For [`intrusive_hash_table_header`] this is
    /// `BUCKET_SIZES[index]`; the raw index is in [`HashHeader::bucket_index`].
    pub buckets: u32,
    /// Number of key/value pairs that follow.
    pub count: u32,
    /// Only the intrusive hash-table form carries an index rather than a count.
    pub bucket_index: Option<u8>,
}

/// The intrusive hash-table header: a `u8` **index** into
/// [`BUCKET_SIZES`] followed by a compressed element count.
///
/// The client raises an error on an index of 23 or more.
pub fn intrusive_hash_table_header(c: &mut Cursor<'_>) -> Result<HashHeader, DatError> {
    let idx = c.u8()?;
    let buckets = BUCKET_SIZES
        .get(usize::from(idx))
        .copied()
        .ok_or(DatError::BadBucketIndex(idx))?;
    let count = c.compressed_u32()?;
    Ok(HashHeader {
        buckets,
        count,
        bucket_index: Some(idx),
    })
}

/// The intrusive hash-list header: a compressed **bucket count** followed by a compressed element
/// count. Note that this one stores the count itself, not an index.
pub fn intrusive_hash_list_header(c: &mut Cursor<'_>) -> Result<HashHeader, DatError> {
    let buckets = c.compressed_u32()?;
    let count = c.compressed_u32()?;
    Ok(HashHeader {
        buckets,
        count,
        bucket_index: None,
    })
}

/// The `'Core'` four-character token.
pub const TOKEN_CORE: u32 = 0x436F_7265;
/// The `'DObj'` token set from the directory entry's version field.
pub const TOKEN_DOBJ: u32 = 0x444F_626A;
/// The `'UIL '` token, 1 only for pack version 3.
pub const TOKEN_UIL: u32 = 0x5549_4C20;

/// The archive version-row header, read direction.
///
/// A `u32` whose bit 31 clear means "this is the bare `'Core'` version"; bit 31 set means the low
/// 30 bits are a stream offset at which the full token table lives. The magic `0x4D676963` never
/// survives a completed write.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionRow {
    pub entries: Vec<(u32, u32)>,
}

impl VersionRow {
    #[must_use]
    pub fn version_by_token(&self, token: u32) -> u32 {
        self.entries
            .iter()
            .find(|(t, _)| *t == token)
            .map_or(0, |(_, v)| *v)
    }

    /// The row the dat loader synthesises from the
    /// directory entry's version field.
    #[must_use]
    pub fn for_pack_version(v: u32) -> Self {
        Self {
            entries: vec![
                (TOKEN_CORE, if v < 2 { 1 } else { 2 }),
                (TOKEN_DOBJ, v),
                (TOKEN_UIL, u32::from(v > 2)),
            ],
        }
    }
}

/// The version-table magic, `0x4D676963` (`"cigM"` in memory order).
pub const VERSION_MAGIC: u32 = 0x4D67_6963;

/// The archive version row -- header plus row -- read direction.
///
/// Returns the row and leaves the cursor immediately after the four header bytes; the trailing
/// table (when there is one) is *not* consumed here, because the object's own fields come first.
pub fn read_version_header(c: &mut Cursor<'_>) -> Result<VersionRow, DatError> {
    let v = c.u32()?;
    if v & 0x8000_0000 == 0 {
        return Ok(VersionRow {
            entries: vec![(TOKEN_CORE, v & 0x3FFF_FFFF)],
        });
    }
    let save = c.position();
    c.seek((v & 0x3FFF_FFFF) as usize)?;
    let count = c.u32()?;
    // Retail's row writer writes the entries in reverse index order, each an 8-byte pair.
    let mut entries = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let tok = c.u32()?;
        let ver = c.u32()?;
        entries.push((tok, ver));
    }
    entries.reverse();
    c.seek(save)?;
    Ok(VersionRow { entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the static table read from the client image.
    #[test]
    fn bucket_sizes_are_the_twenty_three_primes_from_rdata() {
        assert_eq!(BUCKET_SIZES.len(), 23);
        assert_eq!(BUCKET_SIZES[0], 11);
        assert_eq!(BUCKET_SIZES[7], 1531);
        assert_eq!(BUCKET_SIZES[22], 50_331_599);
        assert!(BUCKET_SIZES.windows(2).all(|w| w[0] < w[1]));
    }

    /// Oracle: the first bytes of local-data file `0x2300000E`: bucket index 7 -> 1531
    /// buckets, then the compressed count 873.
    #[test]
    fn intrusive_hash_table_header_matches_the_worked_example() {
        let buf = [0x07u8, 0x83, 0x69];
        let mut c = Cursor::new(&buf);
        let h = intrusive_hash_table_header(&mut c).unwrap();
        assert_eq!(h.bucket_index, Some(7));
        assert_eq!(h.buckets, 1531);
        assert_eq!(h.count, 873);
        c.expect_end().unwrap();
    }

    /// A bucket-size index past the table is refused with its own error, not read as buckets.
    #[test]
    fn an_intrusive_hash_table_with_a_bucket_index_past_the_table_is_refused() {
        let buf = [23u8, 0x00];
        assert!(matches!(
            intrusive_hash_table_header(&mut Cursor::new(&buf)),
            Err(DatError::BadBucketIndex(23))
        ));
    }

    /// The three headers must stay distinct. The same three bytes mean different
    /// things to the two `Archive` forms.
    #[test]
    fn the_two_archive_hash_headers_are_different() {
        let buf = [0x07u8, 0x83, 0x69];
        let table = intrusive_hash_table_header(&mut Cursor::new(&buf)).unwrap();
        let list = intrusive_hash_list_header(&mut Cursor::new(&buf)).unwrap();
        assert_eq!((table.buckets, table.count), (1531, 873));
        assert_eq!((list.buckets, list.count), (7, 873));
    }

    /// Oracle: the synthesised pack-version rows.
    #[test]
    fn pack_version_rows_match_the_documented_formulae() {
        let v1 = VersionRow::for_pack_version(1);
        assert_eq!(v1.version_by_token(TOKEN_CORE), 1);
        assert_eq!(v1.version_by_token(TOKEN_DOBJ), 1);
        assert_eq!(v1.version_by_token(TOKEN_UIL), 0);
        let v3 = VersionRow::for_pack_version(3);
        assert_eq!(v3.version_by_token(TOKEN_CORE), 2);
        assert_eq!(v3.version_by_token(TOKEN_DOBJ), 3);
        assert_eq!(v3.version_by_token(TOKEN_UIL), 1);
    }

    /// Oracle:. A stream carrying only the current
    /// `'Core'` version 2 stores exactly `02 00 00 00`.
    #[test]
    fn version_header_short_form_is_a_bare_core_version() {
        let buf = 2u32.to_le_bytes();
        let mut c = Cursor::new(&buf);
        let row = read_version_header(&mut c).unwrap();
        assert_eq!(row.version_by_token(TOKEN_CORE), 2);
        assert_eq!(c.position(), 4);
    }
}
