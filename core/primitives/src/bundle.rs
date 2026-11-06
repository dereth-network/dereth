//! Named files in one run of bytes: each name's length and UTF-8 bytes, then its contents' length
//! and bytes, lengths little-endian `u32`. What a host keeps its small files in when it can only
//! reach one file at a time, a browser's worker among them.

use std::collections::BTreeMap;

/// The files in `bytes`. A short or malformed tail is dropped: what came before it is kept.
#[must_use]
pub fn decode(mut bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let take = |bytes: &mut &[u8]| -> Option<Vec<u8>> {
        let (len, rest) = bytes.split_first_chunk::<4>()?;
        let len = usize::try_from(u32::from_le_bytes(*len)).ok()?;
        if rest.len() < len {
            return None;
        }
        let (item, rest) = rest.split_at(len);
        *bytes = rest;
        Some(item.to_vec())
    };
    while !bytes.is_empty() {
        let (Some(name), Some(data)) = (take(&mut bytes), take(&mut bytes)) else {
            break;
        };
        let Ok(name) = String::from_utf8(name) else {
            break;
        };
        files.insert(name, data);
    }
    files
}

/// The bytes of `files`, each its name and its contents.
#[must_use]
pub fn encode<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut put = |item: &[u8]| {
        out.extend_from_slice(&u32::try_from(item.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(item);
    };
    for (name, data) in files {
        put(name.as_bytes());
        put(data);
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a host's own way of keeping small files; nothing of the game's).
    use super::*;

    #[test]
    fn a_bundle_round_trips_and_a_torn_tail_loses_only_itself() {
        let bytes = encode([("a", &b"one"[..]), ("b/c", &[0xFF; 3][..])]);
        let files = decode(&bytes);
        assert_eq!(files["a"], b"one");
        assert_eq!(files["b/c"], [0xFF; 3]);
        let torn = decode(&bytes[..bytes.len() - 1]);
        assert_eq!(torn.len(), 1);
        assert!(torn.contains_key("a"));
    }
}
