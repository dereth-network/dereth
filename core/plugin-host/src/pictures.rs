//! A plugin's pictures: the PNG files of its package and the ones it fetches, each looked at once,
//! as it arrives. A picture is kept when it is at most [`MAX_SIDE`] pixels a side and the plugin's
//! pictures, decoded, stay within [`MAX_DECODED`] bytes; it is hashed then, and never again, so
//! a presentation tells one picture from another by that hash rather than by reading it again.

use std::collections::BTreeMap;

/// The most pixels a picture may be, a side.
pub const MAX_SIDE: u32 = 1024;
/// The most bytes one plugin's pictures may take decoded (four a pixel), all of them together.
pub const MAX_DECODED: usize = 32 * 1024 * 1024;

/// A PNG file's width and height, read from its header without decoding it; `None` for anything
/// that is not a PNG file.
#[must_use]
pub fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.get(12..16) != Some(b"IHDR") {
        return None;
    }
    let be = |at: usize| {
        bytes
            .get(at..at + 4)
            .and_then(|b| <[u8; 4]>::try_from(b).ok())
            .map(u32::from_be_bytes)
    };
    Some((be(16)?, be(20)?))
}

/// A file's hash, which tells one picture from another under the same name.
#[must_use]
pub fn hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01B3);
    }
    h ^ (bytes.len() as u64)
}

/// The pictures kept for one plugin: each by its name with its hash, and what they take decoded.
#[derive(Debug, Clone, Default)]
pub struct Pictures {
    pub kept: BTreeMap<String, u64>,
    pub decoded: usize,
}

impl Pictures {
    /// Look at picture `name` as it arrives: kept, or why not.
    ///
    /// # Errors
    /// It is not a PNG file, it is over [`MAX_SIDE`] a side, or it would take the plugin's
    /// pictures over [`MAX_DECODED`].
    pub fn take(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let (w, h) = size(bytes).ok_or_else(|| "it is not a PNG picture".to_owned())?;
        if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
            return Err(format!(
                "the picture {name} is {w} by {h} pixels, over {MAX_SIDE} by {MAX_SIDE}"
            ));
        }
        let decoded = usize::try_from(u64::from(w) * u64::from(h) * 4).unwrap_or(usize::MAX);
        if self.decoded.saturating_add(decoded) > MAX_DECODED {
            return Err(format!(
                "the picture {name} would take the plugin's pictures over {} MiB",
                MAX_DECODED / (1024 * 1024)
            ));
        }
        self.decoded += decoded;
        self.kept.insert(name.to_owned(), hash(bytes));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the host's bounds on a plugin's pictures)
    use super::*;

    /// A PNG file's header alone, `w` by `h` pixels.
    pub(crate) fn header(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([8, 6, 0, 0, 0]);
        b
    }

    /// A picture is kept when it is at most 1,024 pixels a side and the plugin's pictures stay
    /// within 32 MiB decoded, hashed as it arrives; anything else is refused with the reason.
    #[test]
    fn a_picture_is_kept_within_its_size_and_the_plugins_budget() {
        assert_eq!(size(&header(640, 480)), Some((640, 480)));
        assert_eq!(size(b"not a picture"), None);
        let mut p = Pictures::default();
        assert!(p.take("wide.png", &header(1025, 1)).is_err());
        assert!(p.take("tall.png", &header(1, 1025)).is_err());
        assert!(p.take("notes.txt", b"plain words").is_err());
        for i in 0..8 {
            p.take(&format!("p{i}.png"), &header(1024, 1024))
                .expect("4 MiB each, 32 MiB in all");
        }
        assert_eq!(p.decoded, MAX_DECODED);
        let over = p
            .take("one-more.png", &header(1, 1))
            .expect_err("over 32 MiB");
        assert!(over.contains("32 MiB"), "{over}");
        assert_eq!(p.kept.len(), 8);
        assert_eq!(p.kept["p0.png"], hash(&header(1024, 1024)));
    }
}
