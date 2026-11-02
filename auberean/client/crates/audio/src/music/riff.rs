//! RIFF files as the music files are written: a tree of chunks, each a four-character code, a
//! size and its bytes, padded to an even length. A `RIFF` or `LIST` chunk holds a form type and
//! then more chunks.
//!
//! A chunk that claims more bytes than its parent has left is cut at the parent's end, so a short
//! file still yields what it holds.

/// One chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    /// The chunk's code, or for a `RIFF` or `LIST` chunk its form type.
    pub tag: [u8; 4],
    /// Whether it is a `RIFF` or `LIST` chunk, whose data is more chunks.
    pub list: bool,
    /// Its data (after the form type, for a list).
    pub data: &'a [u8],
}

impl<'a> Chunk<'a> {
    /// The chunks a list holds (none for a plain chunk).
    #[must_use]
    pub fn kids(&self) -> Chunks<'a> {
        Chunks(if self.list { self.data } else { &[] })
    }

    /// The first chunk it holds with `tag` (a list by its form type).
    #[must_use]
    pub fn find(&self, tag: &[u8; 4]) -> Option<Chunk<'a>> {
        self.kids().find(|c| &c.tag == tag)
    }

    /// Every chunk it holds with `tag`.
    pub fn all(&self, tag: &'a [u8; 4]) -> impl Iterator<Item = Chunk<'a>> + 'a {
        self.kids().filter(move |c| &c.tag == tag)
    }

    /// Its data read as little-endian fields.
    #[must_use]
    pub fn le(&self) -> Le<'a> {
        Le(self.data)
    }

    /// The text of a chunk holding a nul-terminated UTF-16 string.
    #[must_use]
    pub fn wide_text(&self) -> String {
        let units: Vec<u16> = self
            .data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .take_while(|&u| u != 0)
            .collect();
        String::from_utf16_lossy(&units)
    }
}

/// The chunks one after another in a run of bytes.
#[derive(Clone, Copy, Debug)]
pub struct Chunks<'a>(&'a [u8]);

impl<'a> Chunks<'a> {
    /// The chunks in a run of bytes (a plain chunk's data that holds chunks).
    #[must_use]
    pub fn of(data: &'a [u8]) -> Self {
        Self(data)
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Chunk<'a>> {
        let b = self.0;
        if b.len() < 8 {
            self.0 = &[];
            return None;
        }
        let id = [b[0], b[1], b[2], b[3]];
        let size = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
        let end = 8usize.saturating_add(size).min(b.len());
        let body = &b[8..end];
        let next = end.saturating_add(size & 1).min(b.len());
        self.0 = &b[next..];
        if (&id == b"RIFF" || &id == b"LIST") && body.len() >= 4 {
            Some(Chunk {
                tag: [body[0], body[1], body[2], body[3]],
                list: true,
                data: &body[4..],
            })
        } else {
            Some(Chunk {
                tag: id,
                list: false,
                data: body,
            })
        }
    }
}

/// The file's outer `RIFF` chunk, if the bytes start with one.
#[must_use]
pub fn root(bytes: &[u8]) -> Option<Chunk<'_>> {
    if bytes.get(0..4) != Some(b"RIFF") {
        return None;
    }
    Chunks(bytes).next().filter(|c| c.list)
}

/// Little-endian fields at offsets in a chunk's data; `None` past its end.
#[derive(Clone, Copy, Debug)]
pub struct Le<'a>(pub &'a [u8]);

impl Le<'_> {
    #[must_use]
    pub fn u8(&self, at: usize) -> Option<u8> {
        self.0.get(at).copied()
    }

    #[must_use]
    pub fn u16(&self, at: usize) -> Option<u16> {
        let b = self.0.get(at..at + 2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }

    #[must_use]
    pub fn i16(&self, at: usize) -> Option<i16> {
        let b = self.0.get(at..at + 2)?;
        Some(i16::from_le_bytes([b[0], b[1]]))
    }

    #[must_use]
    pub fn u32(&self, at: usize) -> Option<u32> {
        let b = self.0.get(at..at + 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    #[must_use]
    pub fn i32(&self, at: usize) -> Option<i32> {
        let b = self.0.get(at..at + 4)?;
        Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    #[must_use]
    pub fn f32(&self, at: usize) -> Option<f32> {
        self.u32(at).map(f32::from_bits)
    }

    #[must_use]
    pub fn f64(&self, at: usize) -> Option<f64> {
        let b = self.0.get(at..at + 8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Some(f64::from_le_bytes(a))
    }

    /// Sixteen bytes, as a GUID is stored.
    #[must_use]
    pub fn guid(&self, at: usize) -> Option<[u8; 16]> {
        let b = self.0.get(at..at + 16)?;
        let mut a = [0u8; 16];
        a.copy_from_slice(b);
        Some(a)
    }

    /// A table of fixed-size items led by the item's size, as the music files store their lists:
    /// each item's bytes (at least `min` of them; a longer item is a later version's, its extra
    /// bytes ignored).
    #[must_use]
    pub fn sized_items(&self, min: usize) -> Vec<&[u8]> {
        let Some(size) = self.u32(0) else {
            return Vec::new();
        };
        let size = size as usize;
        if size < min || size == 0 {
            return Vec::new();
        }
        self.0[4..].chunks_exact(size).collect()
    }
}
