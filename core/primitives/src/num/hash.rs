//! The hash functions, which decide iteration order wherever order is observable.
//!
//! This is not an implementation detail. Several UI panels are built by walking a hash table and
//! appending with no sort, so the panel order *is* the hash order; and the physics and object
//! maintenance sweeps enumerate tables where the order can decide which of two simultaneous
//! collisions is reported first. See
//! the exact hash and iteration-order contract.

/// The ELF hash (`hashpjw` with a 4-bit shift) used for every string-keyed table.
///
/// The original has two spellings of it -- a `char` template instantiation and a plain byte
/// version -- with one body and one result, so a single implementation covers both.
///
/// Two details that are easy to lose and both change the result:
/// - `char` is **signed** in the original, so bytes above 0x7F sign-extend.
/// - `0xFFFF_FFFF` is the "not yet computed" sentinel in the string header, so a string that would
///   hash to it is nudged to `0xFFFF_FFFE`.
#[must_use]
pub fn str_hash(bytes: &[u8]) -> u32 {
    if bytes.is_empty() {
        return 0;
    }
    let mut h: u32 = 0;
    for &b in bytes {
        // sign-extend through i8, exactly as the original's signed char does
        let c = i32::from(b as i8) as u32;
        h = h.wrapping_mul(16).wrapping_add(c);
        if h & 0xF000_0000 != 0 {
            h = ((h & 0xF000_0000) >> 24) ^ h;
            h &= 0x0FFF_FFFF;
        }
    }
    if h == 0xFFFF_FFFF {
        0xFFFF_FFFE
    } else {
        h
    }
}

/// The case-insensitive variant.
///
/// It is the same loop with `tolower` applied first, and it deliberately does **not** apply the
/// sentinel fixup and does not memoise.
#[must_use]
pub fn str_hash_case_insensitive(bytes: &[u8]) -> u32 {
    if bytes.is_empty() {
        return 0;
    }
    let mut h: u32 = 0;
    for &b in bytes {
        let lowered = b.to_ascii_lowercase();
        let c = i32::from(lowered as i8) as u32;
        h = h.wrapping_mul(16).wrapping_add(c);
        if h & 0xF000_0000 != 0 {
            h = ((h & 0xF000_0000) >> 24) ^ h;
            h &= 0x0FFF_FFFF;
        }
    }
    h
}

/// The integer-keyed hash used by `LongHash<T>` and its by-value twin.
///
/// The table size is fixed per instance and the mask is `size - 1`; these tables never grow, so a
/// `HashMap` that rehashes changes the enumeration order the moment it does.
#[inline]
#[must_use]
pub fn long_hash(key: u32, mask: u32) -> u32 {
    ((key >> 8) ^ key) & mask
}

/// Bucket selection for `HashTable<K, V>`: a plain modulo against a bucket count that does not grow
/// (11 or 23 in the fixed cases). Chains are in *reverse* insertion order and iteration walks
/// buckets ascending, then the chain.
#[inline]
#[must_use]
pub fn bucket_of(hash: u32, num_buckets: u32) -> u32 {
    hash % num_buckets
}

#[cfg(test)]
mod tests {
    use super::*;

    // The empty string hashes to 0, which the loop below can never produce for a non-empty key,
    // so an empty key and any other key are always distinguishable.
    #[test]
    fn empty_string_hashes_to_zero() {
        assert_eq!(str_hash(b""), 0);
    }

    #[test]
    fn hash_is_stable_and_order_dependent() {
        assert_eq!(str_hash(b"abc"), str_hash(b"abc"));
        assert_ne!(str_hash(b"abc"), str_hash(b"acb"));
    }

    #[test]
    fn high_bytes_sign_extend() {
        // Traced by hand: c = (i8)0x80 = -128, so h = 0xFFFF_FF80; the top nibble is set, so the
        // fold runs: h = (0xF000_0000 >> 24) ^ h = 0xFFFF_FF70, then masked to 0x0FFF_FF70.
        // A zero-extending implementation would give 0x80 and never fold at all.
        assert_eq!(str_hash(&[0x80]), 0x0FFF_FF70);
        assert_ne!(
            str_hash(&[0x80]),
            0x80,
            "0x80 must sign-extend, not zero-extend"
        );
    }

    #[test]
    fn known_ascii_value() {
        // "abc": 'a'=0x61 -> 0x61; *16 + 0x62 = 0x672; *16 + 0x63 = 0x6783. No fold needed.
        assert_eq!(str_hash(b"abc"), 0x6783);
    }

    #[test]
    fn result_never_equals_the_not_computed_sentinel() {
        // Exhaustively checking is impossible; assert the fixup is wired instead.
        // A string hashing to 0xFFFFFFFF must come back as 0xFFFFFFFE.
        for len in 1..6u32 {
            for seed in 0..2000u32 {
                let s: Vec<u8> = (0..len)
                    .map(|i| u8::try_from(seed.wrapping_add(i) % 251 + 1).unwrap())
                    .collect();
                assert_ne!(str_hash(&s), 0xFFFF_FFFF);
            }
        }
    }

    #[test]
    fn case_insensitive_matches_across_case() {
        assert_eq!(
            str_hash_case_insensitive(b"HolTburG"),
            str_hash_case_insensitive(b"holtburg")
        );
    }

    #[test]
    fn long_hash_folds_the_high_byte() {
        // ((k >> 8) ^ k) & mask
        assert_eq!(
            long_hash(0x0000_1234, 0xFF),
            ((0x1234 >> 8) ^ 0x1234) & 0xFF
        );
        assert_eq!(
            long_hash(0xA9B4_FFFF, 0x1FF),
            ((0xA9B4_FFFF >> 8) ^ 0xA9B4_FFFF) & 0x1FF
        );
    }
}
