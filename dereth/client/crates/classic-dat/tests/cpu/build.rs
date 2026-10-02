//! Behaviour: none (test fixtures: builders for hand-made records and containers)
//!
//! Byte builders for the tests: a pre-Throne-of-Destiny container holding any records, and an
//! encoder for the table fields (words, both string shapes, object descriptions).

/// A little-endian record encoder.
#[derive(Default)]
pub struct Enc(pub Vec<u8>);

impl Enc {
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn words(&mut self, v: &[u32]) -> &mut Self {
        for w in v {
            self.u32(*w);
        }
        self
    }
    /// A 32-bit count, then the words.
    pub fn list(&mut self, v: &[u32]) -> &mut Self {
        self.u32(u32::try_from(v.len()).unwrap()).words(v)
    }
    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }
    pub fn align(&mut self) -> &mut Self {
        while !self.0.len().is_multiple_of(4) {
            self.0.push(0);
        }
        self
    }
    /// A 32-bit length and the bytes.
    pub fn text(&mut self, s: &str) -> &mut Self {
        self.u32(u32::try_from(s.len()).unwrap())
            .bytes(s.as_bytes())
    }
    /// A 16-bit length, the bytes and padding to four.
    pub fn legacy_text(&mut self, s: &str) -> &mut Self {
        self.u16(u16::try_from(s.len()).unwrap())
            .bytes(s.as_bytes())
            .align()
    }
    /// A name, an icon and a resource.
    pub fn named(&mut self, name: &str, icon: u32, resource: u32) -> &mut Self {
        self.text(name).u32(icon).u32(resource)
    }
    /// An aligned object description with no palette or part swaps and at most one texture swap
    /// replacing texture 1 with `texture`'s low 16 bits.
    pub fn objdesc(&mut self, texture: Option<u16>) -> &mut Self {
        self.align();
        match texture {
            None => self.bytes(&[0x11, 0, 0, 0]),
            Some(t) => self.bytes(&[0x11, 0, 1, 0]).u8(0).u16(1).u16(t),
        };
        self.align()
    }
}

/// A pre-Throne-of-Destiny container with `records`, iteration `iteration`, and a directory of
/// a root with leaves of `leaf` entries under it once the records outgrow one node.
pub fn container(block: usize, iteration: u32, records: &[(u32, Vec<u8>)], leaf: usize) -> Vec<u8> {
    let mut sorted = records.to_vec();
    sorted.sort_by_key(|r| r.0);
    let mut image = vec![0u8; 0x400];
    let mut entries = Vec::new();
    for (id, data) in &sorted {
        let offset = chain(&mut image, block, data);
        entries.push((*id, offset, u32::try_from(data.len()).unwrap()));
    }
    let root = if entries.len() <= 61 {
        let node = node(&[], &entries);
        chain(&mut image, block, &node)
    } else {
        let mut children = Vec::new();
        let mut separators = Vec::new();
        let mut rest = &entries[..];
        while !rest.is_empty() {
            let take = leaf.min(rest.len());
            let node = node(&[], &rest[..take]);
            children.push(chain(&mut image, block, &node));
            rest = &rest[take..];
            if let Some((first, tail)) = rest.split_first() {
                separators.push(*first);
                rest = tail;
            }
        }
        if separators.len() == children.len() {
            let empty = node(&[], &[]);
            children.push(chain(&mut image, block, &empty));
        }
        let node = node(&children, &separators);
        chain(&mut image, block, &node)
    };
    let size = u32::try_from(image.len()).unwrap();
    let header = [
        0x5442,
        u32::try_from(block).unwrap(),
        size,
        iteration,
        0,
        0,
        0,
        root,
        0,
    ];
    for (i, w) in header.iter().enumerate() {
        image[0x12C + i * 4..0x130 + i * 4].copy_from_slice(&w.to_le_bytes());
    }
    image
}

/// One directory node: child links, the count, then 12-byte entries.
pub fn node(children: &[u32], entries: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut e = Enc::default();
    for i in 0..62 {
        e.u32(children.get(i).copied().unwrap_or(0));
    }
    e.u32(u32::try_from(entries.len()).unwrap());
    for i in 0..61 {
        let (id, offset, size) = entries.get(i).copied().unwrap_or((0, 0, 0));
        e.u32(id).u32(offset).u32(size);
    }
    e.0
}

/// Append `data` as a chain of blocks and return its first block.
pub fn chain(image: &mut Vec<u8>, block: usize, data: &[u8]) -> u32 {
    let start = image.len();
    let per = block - 4;
    let count = data.len().div_ceil(per).max(1);
    for i in 0..count {
        let next = if i + 1 < count {
            start + (i + 1) * block
        } else {
            0
        };
        image.extend_from_slice(&u32::try_from(next).unwrap().to_le_bytes());
        let piece = &data[(i * per).min(data.len())..((i + 1) * per).min(data.len())];
        image.extend_from_slice(piece);
        image.resize(image.len() + per - piece.len(), 0);
    }
    u32::try_from(start).unwrap()
}

/// A palette of 256 colours, colour `i` being `0x00RRGGBB = i * 0x010203`.
pub fn palette(id: u32) -> Vec<u8> {
    let mut e = Enc::default();
    e.u32(id).u32(256);
    for i in 0..256u32 {
        e.u32(i.wrapping_mul(0x0001_0203) & 0x00FF_FFFF);
    }
    e.0
}

/// An indexed texture with the given pixels and default palette.
pub fn indexed(id: u32, width: u32, height: u32, pixels: &[u8], palette: u32) -> Vec<u8> {
    let mut e = Enc::default();
    e.u32(id)
        .u32(2)
        .u32(width)
        .u32(height)
        .bytes(pixels)
        .u32(palette)
        .align();
    e.0
}
