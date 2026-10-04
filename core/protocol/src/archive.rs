//! The wire `PackObj` primitives: the cursor, the alignment rule, the two string forms, the
//! compressed integer and the aggregate headers.
//!
//! Source: `docs/formats/03-serialisation-primitives.md` §§3, 4, 5 and its rebuild notes.
//! `PackObj` is the AC1 network format: size, pack and unpack against a
//! raw cursor with hand-written 4-byte alignment. Every network message body is a `PackObj`.
//!
//! Two things here are easy to get wrong:
//!
//! * **The align rule is per-blob, not per-buffer.** The pointer align computes
//!   `pad = (-(uintptr)p) & 3` on the *absolute* pointer. Blob buffers are 4-aligned, so that is the
//!   offset from the blob's start — which is not the same as the offset from a message *body*, since
//!   the body begins 4 bytes into the blob, after the opcode. [`Reader::body`] and [`Writer::body`]
//!   carry that origin.
//! * **The two string encodings look alike.** `Archive` strings are compressed-length + payload with
//!   no padding ([`Reader::astring`]); `PackObj` strings are `u16` length with a `0xFFFF` escape to
//!   `u32`, payload, and zero-pad to 4 ([`Reader::pstring`]). The wire uses the second.

use crate::cp1252;
use crate::error::MessageError;

/// Byte cursor over one message body, reading little-endian.
///
/// `origin` is the offset of `buf[0]` from the enclosing blob's start, because the alignment rule is
/// computed there and not here.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    origin: usize,
}

impl<'a> Reader<'a> {
    /// A reader whose buffer *is* the blob: offset 0 is the blob's start.
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            buf,
            pos: 0,
            origin: 0,
        }
    }

    /// A reader over a message body, i.e. the blob with its 4-byte opcode already lifted off.
    ///
    /// This is what `dereth_primitives::IncomingMessage::body` holds, and using [`Reader::new`] on it would
    /// align on the wrong origin and corrupt every message with a string in it.
    #[must_use]
    pub fn body(buf: &'a [u8]) -> Self {
        Self {
            buf,
            pos: 0,
            origin: 4,
        }
    }

    /// A reader at an explicit blob offset, for nested blobs.
    #[must_use]
    pub fn with_origin(buf: &'a [u8], origin: usize) -> Self {
        Self {
            buf,
            pos: 0,
            origin,
        }
    }

    /// Bytes not yet consumed.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// The cursor's offset within this reader's buffer.
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// The cursor's offset from the enclosing blob's start — what the alignment rule uses.
    #[must_use]
    pub fn blob_offset(&self) -> usize {
        self.origin + self.pos
    }

    /// The assertion every `Message::read` must end with.
    ///
    /// Decoders are proved by consuming their input exactly; a lenient parser that ignores a
    /// trailing byte hides a layout error.
    pub fn expect_exhausted(&self) -> Result<(), MessageError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(MessageError::TrailingBytes {
                left: self.remaining(),
            })
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], MessageError> {
        if self.remaining() < n {
            return Err(MessageError::UnexpectedEof {
                at: self.blob_offset(),
                needed: n,
                available: self.remaining(),
            });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// `n` raw bytes, no alignment.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], MessageError> {
        self.take(n)
    }

    /// Everything left, consuming it. Used by the pass-through messages the client does not parse.
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }

    pub fn u8(&mut self) -> Result<u8, MessageError> {
        Ok(self.take(1)?[0])
    }

    pub fn i8(&mut self) -> Result<i8, MessageError> {
        Ok(self.u8()? as i8)
    }

    pub fn u16(&mut self) -> Result<u16, MessageError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Result<i16, MessageError> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32, MessageError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32, MessageError> {
        Ok(self.u32()? as i32)
    }

    pub fn u64(&mut self) -> Result<u64, MessageError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub fn i64(&mut self) -> Result<i64, MessageError> {
        Ok(self.u64()? as i64)
    }

    pub fn f32(&mut self) -> Result<f32, MessageError> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub fn f64(&mut self) -> Result<f64, MessageError> {
        Ok(f64::from_bits(self.u64()?))
    }

    /// `ALIGN_PTR(&p) `: skip `(-blob_offset) & 3` bytes, i.e. pad **only** when
    /// `(offset & 3) != 0`. The client does not verify that the padding is zero.
    pub fn align4(&mut self) -> Result<(), MessageError> {
        let pad = self.blob_offset().wrapping_neg() & 3;
        if pad != 0 {
            self.take(pad)?;
        }
        Ok(())
    }

    /// The network string.
    ///
    /// `u16` length, escaping to a `u32` when it reads `0xFFFF`; then the raw bytes; then align to 4.
    /// Two special cases in the client are reproduced here because they change the decoded value: a
    /// length of 1 whose single byte is NUL is the empty string, and a payload whose last byte is NUL
    /// has that byte dropped (some senders pack the terminator). Both are lossy, so a payload that
    /// exercises them does not re-encode byte-identically — the client has the same property.
    pub fn pstring(&mut self) -> Result<String, MessageError> {
        let mut len = usize::from(self.u16()?);
        if len == 0xFFFF {
            len = self.u32()? as usize;
        }
        if len > self.remaining() {
            return Err(MessageError::LengthOverrun {
                field: "string byte length",
                len,
                available: self.remaining(),
            });
        }
        let raw = self.take(len)?;
        let text = if raw.is_empty() || raw == [0u8] {
            String::new()
        } else if raw.last() == Some(&0) {
            cp1252::decode(&raw[..raw.len() - 1])
        } else {
            cp1252::decode(raw)
        };
        self.align4()?;
        Ok(text)
    }

    /// The *`Archive`* narrow-string form: compressed length then
    /// payload, **no padding**. Rare on the wire; present in the `Archive`-framed DDD messages.
    pub fn astring(&mut self) -> Result<String, MessageError> {
        let len = self.compressed_u32()? as usize;
        if len > self.remaining() {
            return Err(MessageError::LengthOverrun {
                field: "string byte length",
                len,
                available: self.remaining(),
            });
        }
        let raw = self.take(len)?;
        Ok(cp1252::decode(raw))
    }

    /// The compressed 32-bit integer form.
    ///
    /// Note the hybrid endianness: the top two bytes are big-endian and the low 16 bits are a native
    /// little-endian `u16`.
    pub fn compressed_u32(&mut self) -> Result<u32, MessageError> {
        let b0 = self.u8()?;
        if b0 & 0x80 == 0 {
            return Ok(u32::from(b0));
        }
        let b1 = self.u8()?;
        if b0 & 0x40 == 0 {
            return Ok((u32::from(b0 & 0x7F) << 8) | u32::from(b1));
        }
        let tail = self.u16()?;
        Ok((((u32::from(b0 & 0x3F) << 8) | u32::from(b1)) << 16) | u32::from(tail))
    }

    /// A DataID stored as a delta from a known
    /// `0x0X000000` base.
    pub fn packed_data_id(&mut self, known_type: u32) -> Result<u32, MessageError> {
        let v = self.u16()?;
        if v & 0x8000 != 0 {
            let lo = self.u16()?;
            Ok(known_type.wrapping_add((u32::from(v & 0x3FFF) << 16) | u32::from(lo)))
        } else {
            Ok(known_type.wrapping_add(u32::from(v)))
        }
    }

    /// The `PackableList` format: a `u32` count then that many elements in list order.
    ///
    /// The count is bounded against the bytes remaining before allocating, because a corrupt count
    /// must not make us reserve gigabytes. Every element is at least one byte, so this is safe.
    pub fn packed_list<T, F>(&mut self, mut elem: F) -> Result<Vec<T>, MessageError>
    where
        F: FnMut(&mut Self) -> Result<T, MessageError>,
    {
        let count = self.u32()? as usize;
        if count > self.remaining() {
            return Err(MessageError::LengthOverrun {
                field: "list count",
                len: count,
                available: self.remaining(),
            });
        }
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            out.push(elem(self)?);
        }
        Ok(out)
    }

    /// The `PackableHashTable` format: one dword header `(table_size << 16) | count`,
    /// then `count` key/value pairs in bucket order.
    ///
    /// The client rejects `table_size > 0x10000` or `count > 0x10000` (neither is representable in
    /// the header's 16 bits except the boundary value itself), and treats `table_size == 0` as an
    /// empty table that is only valid when the count is 0 too.
    pub fn packed_hash_header(&mut self) -> Result<PackedHashHeader, MessageError> {
        let raw = self.u32()?;
        let h = PackedHashHeader {
            table_size: raw >> 16,
            count: raw & 0xFFFF,
        };
        if h.table_size == 0 && h.count != 0 {
            return Err(MessageError::InvalidValue {
                field: "hash-table size",
                value: 0,
            });
        }
        if h.count as usize > self.remaining() {
            return Err(MessageError::LengthOverrun {
                field: "hash-table count",
                len: h.count as usize,
                available: self.remaining(),
            });
        }
        Ok(h)
    }

    /// A `PackableHashTable` in full: the header then its pairs.
    pub fn packed_hash<K, V, F>(&mut self, mut pair: F) -> Result<PackedHash<K, V>, MessageError>
    where
        F: FnMut(&mut Self) -> Result<(K, V), MessageError>,
    {
        let h = self.packed_hash_header()?;
        let mut entries = Vec::with_capacity(h.count as usize);
        for _ in 0..h.count {
            entries.push(pair(self)?);
        }
        Ok(PackedHash {
            table_size: h.table_size,
            entries,
        })
    }

    /// The `PHashTable` stream header — the **third** hash-table header, used by
    /// [`crate::qualities::PropertySequences`] and by `RestrictionDB`.
    ///
    /// One dword: `(bucket_index << 24) | count`, where `bucket_index` is an index into
    /// [`BUCKET_SIZES`] (clamped to 22 on read) and the count is 24 bits.
    ///
    /// The time stamper's own read takes the top byte, clamps it to 22, sizes the table from
    /// `BUCKET_SIZES[index]`, then reads `raw & 0xFFFFFF` entries.
    ///
    /// **Correction to the generated catalogue.** The generated hash-table catalogue says
    /// `buckets = 1 << (packedSize >> 24)` and `count = packedSize & 0xFFFFFF`. The count is right;
    /// the bucket derivation is not — the client indexes [`BUCKET_SIZES`], the same 23-prime table
    /// `SerializeIntrusiveHashTable` uses. Recorded in `docs/CORRECTIONS.md`.
    pub fn phash_header(&mut self) -> Result<PHashHeader, MessageError> {
        let raw = self.u32()?;
        let idx = raw >> 24;
        // The client clamps rather than rejecting, so a header with a silly index still decodes.
        #[allow(clippy::cast_possible_truncation)] // idx >> 24 is at most 0xFF, and min() caps it
        let bucket_index = idx.min(22) as u8;
        let count = raw & 0x00FF_FFFF;
        if count as usize > self.remaining() {
            return Err(MessageError::LengthOverrun {
                field: "PHashTable::count",
                len: count as usize,
                available: self.remaining(),
            });
        }
        Ok(PHashHeader {
            bucket_index,
            count,
        })
    }

    /// A `PHashTable` in full. The header's bucket index is carried on the decoded value so that
    /// re-encoding is byte-exact even where the two flavours of `PHashTable` derive it differently
    /// (see [`PHash`]).
    pub fn phash<K, V, F>(&mut self, mut pair: F) -> Result<PHash<K, V>, MessageError>
    where
        F: FnMut(&mut Self) -> Result<(K, V), MessageError>,
    {
        let h = self.phash_header()?;
        let mut entries = Vec::with_capacity(h.count as usize);
        for _ in 0..h.count {
            entries.push(pair(self)?);
        }
        Ok(PHash {
            bucket_index: h.bucket_index,
            entries,
        })
    }

    /// The `Archive` hash table: a `u8` index into
    /// [`BUCKET_SIZES`] then a compressed element count. Distinct from both other headers, and only
    /// reachable through the `Archive`-framed messages.
    pub fn intrusive_hash_header(&mut self) -> Result<IntrusiveHashHeader, MessageError> {
        let bucket_index = self.u8()?;
        if bucket_index >= 23 {
            return Err(MessageError::InvalidValue {
                field: "SerializeIntrusiveHashTable::bucket_index",
                value: u64::from(bucket_index),
            });
        }
        let count = self.compressed_u32()?;
        if count as usize > self.remaining() {
            return Err(MessageError::LengthOverrun {
                field: "SerializeIntrusiveHashTable::count",
                len: count as usize,
                available: self.remaining(),
            });
        }
        Ok(IntrusiveHashHeader {
            bucket_index,
            count,
        })
    }
}

/// The `PackableHashTable` dword header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedHashHeader {
    pub table_size: u32,
    pub count: u32,
}

/// A decoded `PackableHashTable`. The bucket count is kept because the write order is
/// `key % table_size` and a round trip must reproduce it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackedHash<K, V> {
    pub table_size: u32,
    pub entries: Vec<(K, V)>,
}

impl<K, V> PackedHash<K, V> {
    /// The entries in the order a hash table's bucket walk visits them: ascending
    /// `hash(key) % table_size`, and within one bucket the order held (a stable sort; the retail
    /// server's order inside a bucket varies between captures). A table of size 0 is as held.
    pub fn in_bucket_order(&self, hash: impl Fn(&K) -> u32) -> Vec<&(K, V)> {
        let mut out: Vec<&(K, V)> = self.entries.iter().collect();
        if self.table_size != 0 {
            out.sort_by_key(|(k, _)| hash(k) % self.table_size);
        }
        out
    }
}

/// The hash a hash table keyed by a string buckets its keys by (the same hash as the spell
/// table's names): four bits in per byte, each byte a *signed* windows-1252 char, the top nibble
/// folded back in and cleared.
#[must_use]
pub fn string_hash(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0;
    for &b in bytes {
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // a signed char, sign-extended
        let c = i32::from(b as i8) as u32;
        h = (h << 4).wrapping_add(c);
        if h & 0xF000_0000 != 0 {
            h = (h ^ ((h & 0xF000_0000) >> 24)) & 0x0FFF_FFFF;
        }
    }
    h
}

/// [`string_hash`] of the bytes `s` is written as (windows-1252; a character it lacks as `?`).
#[must_use]
pub fn string_key_hash(s: &str) -> u32 {
    string_hash(&cp1252::encode_lossy(s))
}

/// The `Archive`-side hash-table header: a bucket-size *index*, not a count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntrusiveHashHeader {
    pub bucket_index: u8,
    pub count: u32,
}

/// The `PHashTable` header: a bucket-size index in the top byte, a 24-bit count below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PHashHeader {
    pub bucket_index: u8,
    pub count: u32,
}

/// A decoded `PHashTable`.
///
/// The header's top byte is kept verbatim because there are **two** table classes sharing this wire
/// shape and they derive it differently:
///
/// * `PHashTable` (`PropertySequenceGate`, `RestrictionDB` version `0x10000002`) indexes [`BUCKET_SIZES`];
/// * (the pre-`0x10000002` `RestrictionDB`) takes
///   `buckets = 1 << (index − 1)`, rejecting an index above 0x20.
///
/// Neither derivation is observable on the wire, but the *write* side differs, so carrying the byte
/// is what makes a round trip byte-exact for both. [`PHash::new`] derives it the `PHashTable` way.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PHash<K, V> {
    pub bucket_index: u8,
    pub entries: Vec<(K, V)>,
}

impl<K, V> PHash<K, V> {
    /// A table whose bucket index is derived the way `PHashTable`'s pack side does.
    #[must_use]
    pub fn new(entries: Vec<(K, V)>) -> Self {
        let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
        Self {
            bucket_index: bucket_index_for(count),
            entries,
        }
    }
}

/// The client's 23 bucket-size primes, indexed by every hash-table header that carries a
/// bucket *index*.
pub const BUCKET_SIZES: [u32; 23] = [
    11, 23, 47, 89, 191, 383, 761, 1531, 3067, 6143, 12281, 24571, 49139, 98299, 196_597, 393_209,
    786_431, 1_572_853, 3_145_721, 6_291_449, 12_582_893, 25_165_813, 50_331_599,
];

/// Select the first bucket-size entry `>= n`, clamped to
/// the last. This is what the write side of a `PHashTable` header stores, so it is derivable from
/// the element count alone and a round trip is byte-exact.
#[must_use]
pub fn bucket_index_for(count: u32) -> u8 {
    let i = BUCKET_SIZES.iter().position(|&b| b >= count).unwrap_or(22);
    #[allow(clippy::cast_possible_truncation)] // the table has 23 entries
    {
        i as u8
    }
}

/// Byte sink mirroring [`Reader`]. Paired with it rather than sharing one body that serialises
/// in both directions.
#[derive(Debug, Clone)]
pub struct Writer {
    buf: Vec<u8>,
    origin: usize,
    /// A known-type DataID whose delta is below this is written in the 2-byte short form; see
    /// [`Writer::with_known_type_short_limit`].
    known_type_short_limit: u32,
    /// Counts and strings a field cannot hold wrap instead of failing; see
    /// [`Writer::with_wrapping_counts`].
    wrapping_counts: bool,
    /// How many counts this writer has wrapped; see [`Writer::counts_wrapped`].
    counts_wrapped: u32,
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

impl Writer {
    /// The retail client's short-form range for a known-type DataID: deltas below `0x4000` take 2
    /// bytes, the rest 4. It is every writer's default.
    pub const RETAIL_KNOWN_TYPE_SHORT_LIMIT: u32 = 0x4000;

    /// The widest short-form range the format allows: a short form's top bit would read as the
    /// long form's flag.
    pub const MAX_KNOWN_TYPE_SHORT_LIMIT: u32 = 0x8000;

    /// A writer whose buffer *is* the blob.
    #[must_use]
    pub fn new() -> Self {
        Self::with_origin(0)
    }

    /// A writer for a message body, i.e. with the 4-byte opcode notionally already emitted.
    #[must_use]
    pub fn body() -> Self {
        Self::with_origin(4)
    }

    #[must_use]
    pub fn with_origin(origin: usize) -> Self {
        Self {
            buf: Vec::new(),
            origin,
            known_type_short_limit: Self::RETAIL_KNOWN_TYPE_SHORT_LIMIT,
            wrapping_counts: false,
            counts_wrapped: 0,
        }
    }

    /// A writer that continues a blob already begun in `buf`: `buf[0]` is the blob's first byte,
    /// and what is written is appended to it. [`Writer::into_inner`] hands the whole blob back, so
    /// a body can be written straight after a header without a copy.
    #[must_use]
    pub fn from_vec(buf: Vec<u8>) -> Self {
        Self {
            buf,
            origin: 0,
            known_type_short_limit: Self::RETAIL_KNOWN_TYPE_SHORT_LIMIT,
            wrapping_counts: false,
            counts_wrapped: 0,
        }
    }

    /// This writer with **wrapping** counts: a string a field cannot encode is written with `?` for
    /// each character windows-1252 lacks, a hash-table header's count and size keep their low 16 bits,
    /// and an object description's byte counts keep their low 8, where the default refuses to encode
    /// them. The client never writes such values; a server reproducing another server's casts may
    /// opt in. Off by default.
    #[must_use]
    pub fn with_wrapping_counts(mut self) -> Self {
        self.wrapping_counts = true;
        self
    }

    /// Whether this writer wraps what a field cannot hold ([`Writer::with_wrapping_counts`]).
    #[must_use]
    pub fn wraps_counts(&self) -> bool {
        self.wrapping_counts
    }

    /// How many counts a [wrapping](Writer::with_wrapping_counts) writer has cut to fit their
    /// field so far: a hash-table header's count or size past 16 bits, an object description's
    /// count past 255. A writer that does not wrap refuses those instead, and its tally stays 0.
    #[must_use]
    pub fn counts_wrapped(&self) -> u32 {
        self.counts_wrapped
    }

    /// Records one count [`Writer::counts_wrapped`] tallies.
    pub(crate) fn note_count_wrapped(&mut self) {
        self.counts_wrapped = self.counts_wrapped.saturating_add(1);
    }

    /// This writer with a different short-form range for known-type DataIDs
    /// ([`Writer::packed_data_id`]): a delta below `limit` is written in 2 bytes, anything else in
    /// 4. Servers differ in the short-form range; both forms read the same, so this changes the
    /// bytes and never the value. The default is
    /// [`RETAIL_KNOWN_TYPE_SHORT_LIMIT`](Self::RETAIL_KNOWN_TYPE_SHORT_LIMIT).
    ///
    /// # Panics
    /// When `limit` is above [`MAX_KNOWN_TYPE_SHORT_LIMIT`](Self::MAX_KNOWN_TYPE_SHORT_LIMIT).
    #[must_use]
    pub fn with_known_type_short_limit(mut self, limit: u32) -> Self {
        assert!(
            limit <= Self::MAX_KNOWN_TYPE_SHORT_LIMIT,
            "a known-type DataID short form cannot reach {limit:#x}"
        );
        self.known_type_short_limit = limit;
        self
    }

    /// The short-form range this writer uses for known-type DataIDs.
    #[must_use]
    pub fn known_type_short_limit(&self) -> u32 {
        self.known_type_short_limit
    }

    #[must_use]
    pub fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Offset from the enclosing blob's start.
    #[must_use]
    pub fn blob_offset(&self) -> usize {
        self.origin + self.buf.len()
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn i8(&mut self, v: i8) {
        self.buf.push(v as u8);
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i16(&mut self, v: i16) {
        self.u16(v as u16);
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.u64(v as u64);
    }

    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    pub fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }

    /// Pad with zeroes to the next 4-byte boundary *of the blob*, and only when misaligned.
    pub fn align4(&mut self) {
        let pad = self.blob_offset().wrapping_neg() & 3;
        for _ in 0..pad {
            self.buf.push(0);
        }
    }

    /// The packed string, write direction.
    pub fn pstring(&mut self, s: &str) -> Result<(), MessageError> {
        let raw = if self.wrapping_counts {
            cp1252::encode_lossy(s)
        } else {
            cp1252::encode(s).ok_or(MessageError::Unencodable {
                field: "byte string",
                reason: "character has no windows-1252 representation",
            })?
        };
        if raw.len() >= 0xFFFF {
            let n = u32::try_from(raw.len()).map_err(|_| MessageError::Unencodable {
                field: "byte string",
                reason: "string longer than 4 GiB",
            })?;
            self.u16(0xFFFF);
            self.u32(n);
        } else {
            // Guarded by the branch: raw.len() < 0xFFFF.
            #[allow(clippy::cast_possible_truncation)]
            self.u16(raw.len() as u16);
        }
        self.bytes(&raw);
        self.align4();
        Ok(())
    }

    /// The `Archive` string: compressed length, payload, no padding.
    pub fn astring(&mut self, s: &str) -> Result<(), MessageError> {
        let raw = if self.wrapping_counts {
            cp1252::encode_lossy(s)
        } else {
            cp1252::encode(s).ok_or(MessageError::Unencodable {
                field: "byte string",
                reason: "character has no windows-1252 representation",
            })?
        };
        let n = u32::try_from(raw.len()).map_err(|_| MessageError::Unencodable {
            field: "byte string",
            reason: "string longer than 4 GiB",
        })?;
        self.compressed_u32(n);
        self.bytes(&raw);
        Ok(())
    }

    /// Write a compressed unsigned integer.
    pub fn compressed_u32(&mut self, v: u32) {
        // Each arm masks explicitly to the byte it writes.
        #[allow(clippy::cast_possible_truncation)]
        if v < 0x80 {
            self.u8(v as u8);
        } else if v < 0x4000 {
            self.u8(((v >> 8) as u8) | 0x80);
            self.u8((v & 0xFF) as u8);
        } else {
            self.u8(((v >> 24) as u8) | 0xC0);
            self.u8(((v >> 16) & 0xFF) as u8);
            self.u16((v & 0xFFFF) as u16);
        }
    }

    /// The known-type DataID, write direction. A null id packs as delta 0. A delta below the
    /// writer's [short-form limit](Writer::with_known_type_short_limit) takes 2 bytes.
    pub fn packed_data_id(&mut self, known_type: u32, did: u32) -> Result<(), MessageError> {
        let did = if did == 0 { known_type } else { did };
        let delta = did.wrapping_sub(known_type);
        // Each arm masks explicitly to 16 bits.
        #[allow(clippy::cast_possible_truncation)]
        if delta < self.known_type_short_limit {
            self.u16(delta as u16);
            Ok(())
        } else if delta < 0x4000_0000 {
            self.u16(((delta >> 16) as u16) | 0x8000);
            self.u16((delta & 0xFFFF) as u16);
            Ok(())
        } else {
            Err(MessageError::Unencodable {
                field: "packed data id",
                reason: "delta from the known type exceeds 0x3FFFFFFF",
            })
        }
    }

    /// A packable list, write direction.
    pub fn packed_list<T, F>(&mut self, items: &[T], mut elem: F) -> Result<(), MessageError>
    where
        F: FnMut(&mut Self, &T) -> Result<(), MessageError>,
    {
        let n = u32::try_from(items.len()).map_err(|_| MessageError::Unencodable {
            field: "list count",
            reason: "more than 4 G elements",
        })?;
        self.u32(n);
        for it in items {
            elem(self, it)?;
        }
        Ok(())
    }

    /// The `PackableHashTable` dword header.
    pub fn packed_hash_header(&mut self, h: PackedHashHeader) -> Result<(), MessageError> {
        if self.wrapping_counts {
            if h.table_size > 0xFFFF || h.count > 0xFFFF {
                self.note_count_wrapped();
            }
            self.u32(((h.table_size & 0xFFFF) << 16) | (h.count & 0xFFFF));
            return Ok(());
        }
        if h.table_size > 0x1_0000 || h.count > 0x1_0000 {
            return Err(MessageError::Unencodable {
                field: "hash table",
                reason: "table size or count exceeds 0x10000",
            });
        }
        self.u32((h.table_size << 16) | h.count);
        Ok(())
    }

    /// A `PackableHashTable` in full. Entries are written **in the order held**; the caller is
    /// responsible for having them in `key % table_size` bucket order, which is what the client's
    /// bucket walk produces. The within-bucket order of colliding keys is not settled.
    pub fn packed_hash<K, V, F>(
        &mut self,
        t: &PackedHash<K, V>,
        mut pair: F,
    ) -> Result<(), MessageError>
    where
        F: FnMut(&mut Self, &K, &V) -> Result<(), MessageError>,
    {
        let count = u32::try_from(t.entries.len()).map_err(|_| MessageError::Unencodable {
            field: "hash-table count",
            reason: "more than 4 G entries",
        })?;
        self.packed_hash_header(PackedHashHeader {
            table_size: t.table_size,
            count,
        })?;
        for (k, v) in &t.entries {
            pair(self, k, v)?;
        }
        Ok(())
    }

    /// A `PackableHashTable` in full, its entries written in bucket order under `hash`
    /// ([`PackedHash::in_bucket_order`]) whatever order they are held in.
    pub fn packed_hash_in_bucket_order<K, V, H, F>(
        &mut self,
        t: &PackedHash<K, V>,
        hash: H,
        mut pair: F,
    ) -> Result<(), MessageError>
    where
        H: Fn(&K) -> u32,
        F: FnMut(&mut Self, &K, &V) -> Result<(), MessageError>,
    {
        let count = u32::try_from(t.entries.len()).map_err(|_| MessageError::Unencodable {
            field: "hash-table count",
            reason: "more than 4 G entries",
        })?;
        self.packed_hash_header(PackedHashHeader {
            table_size: t.table_size,
            count,
        })?;
        for (k, v) in t.in_bucket_order(hash) {
            pair(self, k, v)?;
        }
        Ok(())
    }

    /// The `PHashTable` header. The client's write side stores
    /// the [`BUCKET_SIZES`] index chosen for the element count in the top byte; the decoded
    /// value carries that byte so a round trip reproduces it exactly.
    pub fn phash_header(&mut self, h: PHashHeader) -> Result<(), MessageError> {
        if h.count > 0x00FF_FFFF {
            return Err(MessageError::Unencodable {
                field: "PHashTable::count",
                reason: "count does not fit in 24 bits",
            });
        }
        self.u32((u32::from(h.bucket_index) << 24) | h.count);
        Ok(())
    }

    /// A `PHashTable` in full.
    pub fn phash<K, V, F>(&mut self, t: &PHash<K, V>, mut pair: F) -> Result<(), MessageError>
    where
        F: FnMut(&mut Self, &K, &V) -> Result<(), MessageError>,
    {
        let count = u32::try_from(t.entries.len()).map_err(|_| MessageError::Unencodable {
            field: "PHashTable::count",
            reason: "more than 4 G entries",
        })?;
        self.phash_header(PHashHeader {
            bucket_index: t.bucket_index,
            count,
        })?;
        for (k, v) in &t.entries {
            pair(self, k, v)?;
        }
        Ok(())
    }

    /// The `Archive`-side hash-table header.
    pub fn intrusive_hash_header(&mut self, h: IntrusiveHashHeader) -> Result<(), MessageError> {
        if h.bucket_index >= 23 {
            return Err(MessageError::Unencodable {
                field: "SerializeIntrusiveHashTable::bucket_index",
                reason: "bucket index must be below 23",
            });
        }
        self.u8(h.bucket_index);
        self.compressed_u32(h.count);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// `with_wrapping_counts` wraps what the default refuses: the hash header's 16-bit fields and
    /// the string's unencodable characters.
    #[test]
    fn wrapping_counts_wraps_what_the_default_refuses() {
        let h = PackedHashHeader {
            table_size: 0x1_0001,
            count: 0x1_0002,
        };
        assert!(Writer::new().packed_hash_header(h).is_err());
        let mut w = Writer::new().with_wrapping_counts();
        w.packed_hash_header(PackedHashHeader {
            table_size: 0xFFFF,
            count: 0xFFFF,
        })
        .unwrap();
        assert_eq!(w.counts_wrapped(), 0, "a count that fits is not wrapped");
        let mut w = Writer::new().with_wrapping_counts();
        w.packed_hash_header(h).unwrap();
        assert_eq!(w.counts_wrapped(), 1);
        assert_eq!(w.into_inner(), (0x0001_0002_u32).to_le_bytes());

        assert!(Writer::new().pstring("a\u{4E2D}").is_err());
        let mut w = Writer::new().with_wrapping_counts();
        w.pstring("a\u{4E2D}").unwrap();
        assert_eq!(&w.into_inner()[..4], &[2, 0, b'a', b'?']);
    }

    use super::*;

    /// Oracle: `docs/formats/03-serialisation-primitives.md` §3.1, which transcribes
    /// the compressed 32-bit form, cross-checked there against ACE's
    /// `ReadCompressedUInt32`.
    #[test]
    fn compressed_integer_matches_the_documented_three_forms() {
        let cases: &[(u32, &[u8])] = &[
            (0x00, &[0x00]),
            (0x7F, &[0x7F]),
            (0x80, &[0x80, 0x80]),
            (0x369, &[0x83, 0x69]), // the worked example in §7.1: 0x83 0x69 -> 873
            (0x3FFF, &[0xBF, 0xFF]),
            // 4-byte form: top two bytes big-endian, low 16 bits a little-endian u16.
            (0x4000, &[0xC0, 0x00, 0x00, 0x40]),
            (0x1234_5678, &[0xD2, 0x34, 0x78, 0x56]),
        ];
        for (value, bytes) in cases {
            let mut w = Writer::new();
            w.compressed_u32(*value);
            assert_eq!(w.as_slice(), *bytes, "writing 0x{value:X}");
            let mut r = Reader::new(bytes);
            assert_eq!(r.compressed_u32().unwrap(), *value, "reading {bytes:02X?}");
            assert_eq!(r.remaining(), 0);
        }
    }

    /// Oracle: §3.1's worked example, which is real data from `client_local_English.dat` file
    /// `0x2300000E` at offset 9.
    #[test]
    fn the_string_table_worked_example_decodes() {
        let mut r = Reader::new(&[0x83, 0x69]);
        assert_eq!(r.compressed_u32().unwrap(), 873);
    }

    /// Oracle: the pointer-align rule as transcribed in §5.1 — `pad = (-p) & 3`, so the pad is
    /// written only when the offset is not already 4-aligned.
    #[test]
    fn align_fires_only_when_misaligned() {
        for start in 0..8usize {
            let mut w = Writer::with_origin(start);
            w.align4();
            let expected = start.wrapping_neg() & 3;
            assert_eq!(w.len(), expected, "origin {start}");
        }
    }

    /// The origin matters: a message body is 4 bytes into its blob, so aligning it as if it began at
    /// zero pads in the wrong places. This is the "align rule is per-blob, not per-buffer" trap.
    #[test]
    fn align_is_relative_to_the_blob_not_the_buffer() {
        // Two bytes into the body is six bytes into the blob: both need two bytes of pad, so use a
        // one-byte field to separate them.
        let mut body = Writer::body();
        body.u8(0xAA);
        body.align4();
        // blob offset was 5, so pad 3.
        assert_eq!(body.as_slice(), &[0xAA, 0, 0, 0]);

        let mut blob = Writer::new();
        blob.u8(0xAA);
        blob.align4();
        // blob offset was 1, so pad 3 as well — but now check an offset where they differ.
        assert_eq!(blob.as_slice(), &[0xAA, 0, 0, 0]);

        let mut body2 = Writer::body();
        body2.bytes(&[1, 2, 3, 4, 5]); // blob offset 9
        body2.align4();
        assert_eq!(body2.len(), 8, "9 -> 12 is three bytes of pad");

        let mut blob2 = Writer::new();
        blob2.bytes(&[1, 2, 3, 4, 5]); // blob offset 5
        blob2.align4();
        assert_eq!(blob2.len(), 8, "5 -> 8 is three bytes of pad");
    }

    /// Oracle: the packed string's pack and unpack pair, transcribed in
    /// §5.2, plus the worked example from `client_portal.dat` file `0x0E000004`.
    #[test]
    fn pstring_round_trips_with_the_documented_padding() {
        let cases: &[(&str, usize)] = &[
            ("", 4),                  // u16 len 0 + 2 pad
            ("a", 4),                 // 2 + 1 + 1 pad
            ("ab", 4),                // 2 + 2 + 0 pad
            ("Item Enchantment", 20), // §5.2's worked example: 2 + 16 + 2 pad
        ];
        for (s, size) in cases {
            let mut w = Writer::new();
            w.pstring(s).unwrap();
            assert_eq!(w.len(), *size, "packed size of {s:?}");
            let bytes = w.into_inner();
            let mut r = Reader::new(&bytes);
            assert_eq!(&r.pstring().unwrap(), s);
            r.expect_exhausted().unwrap();
        }
    }

    /// The 0xFFFF escape to a u32 length. GDLE and ACE both omit it; §5.2 says a rebuild that writes
    /// must implement it.
    #[test]
    fn pstring_long_form_escapes_through_ffff() {
        let s = "x".repeat(0xFFFF);
        let mut w = Writer::new();
        w.pstring(&s).unwrap();
        let bytes = w.into_inner();
        assert_eq!(&bytes[0..2], &[0xFF, 0xFF]);
        assert_eq!(
            u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]),
            0xFFFF
        );
        let mut r = Reader::new(&bytes);
        assert_eq!(r.pstring().unwrap().len(), 0xFFFF);
        r.expect_exhausted().unwrap();
    }

    /// The two client special cases in `UnPack`: a lone NUL is the empty string, and a packed
    /// terminator is dropped.
    #[test]
    fn pstring_reproduces_the_two_nul_special_cases() {
        // len 1, byte 0 -> empty
        let mut r = Reader::new(&[0x01, 0x00, 0x00, 0x00]);
        assert_eq!(r.pstring().unwrap(), "");

        // "hi\0" -> "hi"
        let mut r = Reader::new(&[0x03, 0x00, b'h', b'i', 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(r.pstring().unwrap(), "hi");
        assert_eq!(r.remaining(), 0);
    }

    /// The `Archive` string form has no padding at all — swapping the two is the classic failure.
    #[test]
    fn astring_has_no_padding() {
        let mut w = Writer::new();
        w.astring("abc").unwrap();
        assert_eq!(w.as_slice(), b"\x03abc");
        let mut r = Reader::new(w.as_slice());
        assert_eq!(r.astring().unwrap(), "abc");
        r.expect_exhausted().unwrap();
    }

    /// Oracle: §5.4 — writes `(table_size << 16) | count`;
    /// §6.3 — writes a bucket *index* byte then a
    /// compressed count. The two headers must not be confused, and neither is a `PackableList`
    /// header (a plain `u32` count).
    #[test]
    fn the_three_aggregate_headers_decode_distinctly() {
        // Each reader bounds its count against the bytes that follow, so pad generously.
        let pad = [0u8; 8000];

        // PackableHashTable: 8 buckets, 2 entries.
        let bytes: Vec<u8> = [0x02, 0x00, 0x08, 0x00].into_iter().chain(pad).collect();
        let mut r = Reader::new(&bytes);
        let h = r.packed_hash_header().unwrap();
        assert_eq!((h.table_size, h.count), (8, 2));

        // SerializeIntrusiveHashTable: bucket index 7 (= 1531 buckets), 873 entries — §7.1's real
        // bytes from client_local_English.dat.
        let bytes: Vec<u8> = [0x07, 0x83, 0x69].into_iter().chain(pad).collect();
        let mut r = Reader::new(&bytes);
        let h = r.intrusive_hash_header().unwrap();
        assert_eq!((h.bucket_index, h.count), (7, 873));

        // PHashTable: bucket index 1 (= 23 buckets), 2 entries.
        let bytes: Vec<u8> = [0x02, 0x00, 0x00, 0x01].into_iter().chain(pad).collect();
        let mut r = Reader::new(&bytes);
        let h = r.phash_header().unwrap();
        assert_eq!((h.bucket_index, h.count), (1, 2));

        // The same first dword read as a PackableList header is a count of 0x00080002 — which is
        // why reading one as the other is a silent disaster and each has its own accessor.
        let bytes = [0x02, 0x00, 0x08, 0x00];
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u32().unwrap(), 0x0008_0002);
    }

    /// Oracle: the time stamper's own hash-table header, as retail reads it.
    /// The read side clamps the index at 0x16 and takes the low 24 bits as the count; the write side
    /// derives the index from the element count, which makes the header byte-exact on a round trip.
    ///
    /// The generated hash-table catalogue derives the bucket count as `1 << (packedSize >> 24)`;
    /// that is wrong and is recorded in `docs/CORRECTIONS.md`.
    #[test]
    fn phash_header_uses_the_bucket_size_table_not_a_shift() {
        for count in [0u32, 1, 11, 12, 23, 24, 100, 1531, 1532] {
            let h = PHashHeader {
                bucket_index: bucket_index_for(count),
                count,
            };
            let mut w = Writer::new();
            w.phash_header(h).unwrap();
            // The reader bounds the count against the buffer, so feed it a body big enough.
            let mut padded = w.into_inner();
            padded.extend(std::iter::repeat_n(0u8, count as usize));
            let mut r = Reader::new(&padded);
            assert_eq!(r.phash_header().unwrap(), h, "count {count}");
            assert!(
                BUCKET_SIZES[h.bucket_index as usize] >= count || h.bucket_index == 22,
                "count {count} must land in a bucket size that can hold it"
            );
        }
        // The index is a table position, not a log: 11 entries want index 0 (11 buckets), 12 want
        // index 1 (23 buckets). A `1 << idx` reading would give 1 and 2 buckets.
        assert_eq!(bucket_index_for(11), 0);
        assert_eq!(bucket_index_for(12), 1);
        assert_eq!(bucket_index_for(24), 2);
        assert_eq!(bucket_index_for(u32::MAX), 22, "clamped to the last prime");
    }

    #[test]
    fn packed_list_round_trips() {
        let items = vec![1u32, 2, 3];
        let mut w = Writer::new();
        w.packed_list(&items, |w, v| {
            w.u32(*v);
            Ok(())
        })
        .unwrap();
        assert_eq!(w.len(), 16);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.packed_list(Reader::u32).unwrap(), items);
        r.expect_exhausted().unwrap();
    }

    /// Oracle: §3.2, the known-type DataID's pack and unpack pair.
    #[test]
    fn packed_data_id_round_trips_both_forms() {
        let base = 0x0600_0000;
        for did in [
            base,
            base + 1,
            base + 0x3FFF,
            base + 0x4000,
            base + 0x0012_3456,
        ] {
            let mut w = Writer::new();
            w.packed_data_id(base, did).unwrap();
            let expected = if did - base < 0x4000 { 2 } else { 4 };
            assert_eq!(w.len(), expected, "size for 0x{did:08X}");
            let bytes = w.into_inner();
            let mut r = Reader::new(&bytes);
            assert_eq!(r.packed_data_id(base).unwrap(), did);
        }
        // A null id packs as delta 0 and reads back as the known type itself.
        let mut w = Writer::new();
        w.packed_data_id(base, 0).unwrap();
        assert_eq!(w.as_slice(), &[0, 0]);
    }

    /// The short-form range is a writer option: the retail range by default, a wider one on
    /// request, and the reader takes either form to the same value.
    #[test]
    fn the_known_type_short_form_range_is_a_writer_option() {
        let base = 0x0600_0000;
        for (limit, did, len) in [
            (Writer::RETAIL_KNOWN_TYPE_SHORT_LIMIT, 0x0600_3FFF, 2),
            (Writer::RETAIL_KNOWN_TYPE_SHORT_LIMIT, 0x0600_4000, 4),
            (Writer::RETAIL_KNOWN_TYPE_SHORT_LIMIT, 0x0600_5555, 4),
            (0x8000, 0x0600_5555, 2),
            (0x8000, 0x0600_7FFF, 2),
            (0x8000, 0x0600_8000, 4),
            (0x8000, 0x0601_0000, 4),
        ] {
            let mut w = Writer::new().with_known_type_short_limit(limit);
            assert_eq!(w.known_type_short_limit(), limit);
            w.packed_data_id(base, did).unwrap();
            assert_eq!(w.len(), len, "0x{did:08X} below 0x{limit:X}");
            let bytes = w.into_inner();
            assert_eq!(Reader::new(&bytes).packed_data_id(base).unwrap(), did);
        }
        assert_eq!(Writer::new().known_type_short_limit(), 0x4000);
        assert_eq!(Writer::default().known_type_short_limit(), 0x4000);
        let mut w = Writer::new().with_known_type_short_limit(0x8000);
        w.packed_data_id(base, 0x0600_5555).unwrap();
        assert_eq!(w.as_slice(), &[0x55, 0x55]);
    }

    #[test]
    #[should_panic(expected = "short form cannot reach")]
    fn a_short_form_range_past_the_flag_bit_is_refused() {
        let _ = Writer::new().with_known_type_short_limit(0x8001);
    }

    /// A writer continuing a blob aligns on the blob's start, as a fresh one with the same origin
    /// does.
    #[test]
    fn from_vec_continues_the_blob_it_is_given() {
        let mut w = Writer::from_vec(vec![0xB0, 0xF7, 0, 0, 1]);
        assert_eq!(w.blob_offset(), 5);
        w.pstring("ab").unwrap();
        let mut fresh = Writer::with_origin(5);
        fresh.pstring("ab").unwrap();
        let out = w.into_inner();
        assert_eq!(&out[..5], &[0xB0, 0xF7, 0, 0, 1]);
        assert_eq!(&out[5..], fresh.as_slice());
        assert_eq!(out.len() % 4, 0);
    }

    #[test]
    fn a_short_buffer_errors_rather_than_panicking() {
        let mut r = Reader::new(&[0x01]);
        assert!(matches!(r.u32(), Err(MessageError::UnexpectedEof { .. })));
        let mut r = Reader::new(&[0xFF, 0x7F]);
        assert!(matches!(
            r.pstring(),
            Err(MessageError::LengthOverrun { .. })
        ));
    }

    #[test]
    fn expect_exhausted_is_the_trailing_byte_assertion() {
        let mut r = Reader::new(&[1, 2, 3, 4, 5]);
        r.u32().unwrap();
        assert_eq!(
            r.expect_exhausted(),
            Err(MessageError::TrailingBytes { left: 1 })
        );
    }
}
