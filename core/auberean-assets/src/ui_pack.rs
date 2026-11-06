//! Layouts written back as the records store them, their sizes and the records they name.

use auberean_primitives::scramble;

use crate::ui::{
    Attribute, AttributeValue, Element, ElementState, Media, Transition, TransitionChange, UiLayout,
};

/// Bytes written as the records store them (little-endian, aligned to four from the record's
/// start), or only counted.
struct Packer {
    out: Option<Vec<u8>>,
    len: usize,
}

impl Packer {
    const fn new(write: bool) -> Self {
        Self {
            out: if write { Some(Vec::new()) } else { None },
            len: 0,
        }
    }

    fn bytes(&mut self, b: &[u8]) {
        if let Some(o) = &mut self.out {
            o.extend_from_slice(b);
        }
        self.len += b.len();
    }

    fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }

    fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }

    fn i16(&mut self, v: i16) {
        self.bytes(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.bytes(&v.to_le_bytes());
    }

    fn f64(&mut self, v: f64) {
        self.bytes(&v.to_le_bytes());
    }

    fn align(&mut self) {
        let pad = (4 - self.len % 4) % 4;
        self.bytes(&[0; 3][..pad]);
    }

    fn count(&mut self, n: usize) {
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }

    /// A narrow string: its length (16 bits, or `0xFFFF` and 32 bits), its scrambled bytes,
    /// alignment.
    fn narrow_string(&mut self, s: &str) {
        let plain: Vec<u8> = s
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .collect();
        match u16::try_from(plain.len()) {
            Ok(n) if n != 0xFFFF => self.u16(n),
            _ => {
                self.u16(0xFFFF);
                self.count(plain.len());
            }
        }
        self.bytes(&scramble::conceal(&plain));
        self.align();
    }
}

impl Attribute {
    /// The id, a byte kind, alignment, the value its kind stores, alignment.
    // D2-11101, D2-11102
    fn pack(&self, p: &mut Packer) {
        p.u32(self.id);
        let kind = match &self.value {
            AttributeValue::Id(_) => 1,
            AttributeValue::Int(_) => 2,
            AttributeValue::Float(_) => 3,
            AttributeValue::Bool(_) => 4,
            AttributeValue::Text(_) => 5,
            AttributeValue::StringToken(_) => 6,
            AttributeValue::DataFile(_) => 7,
            AttributeValue::Empty(k) => *k,
        };
        p.u8(kind);
        p.align();
        match &self.value {
            AttributeValue::Id(v) | AttributeValue::StringToken(v) => p.u32(*v),
            AttributeValue::Int(v) => p.i32(*v),
            AttributeValue::Float(v) => p.f32(*v),
            AttributeValue::Bool(v) => p.u32(u32::from(*v)),
            AttributeValue::Text(s) => p.narrow_string(s),
            AttributeValue::DataFile(d) => p.u32(d.0),
            AttributeValue::Empty(_) => {}
        }
        p.align();
    }

    /// The record a data-file attribute names.
    // D2-11097
    #[must_use]
    pub fn sub_data_id(&self) -> Option<u32> {
        match self.value {
            AttributeValue::DataFile(d) => Some(d.0),
            _ => None,
        }
    }

    /// An attribute as made before it is read: an id of 0 and no value.
    // D2-11098
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            id: 0,
            value: AttributeValue::Empty(0),
        }
    }
}

impl Media {
    /// The item's kind number.
    #[must_use]
    pub const fn kind(&self) -> u32 {
        match self {
            Self::Animation { .. } => 1,
            Self::Pause { .. } => 2,
            Self::Image { .. } => 3,
            Self::Alpha { .. } => 4,
            Self::Sound { .. } => 5,
            Self::Jump { .. } => 6,
            Self::Message { .. } => 7,
            Self::State { .. } => 8,
            Self::Video { .. } => 9,
            Self::Cursor { .. } => 10,
        }
    }

    /// The kind word twice (the list's and the item's own), then the kind's fields.
    fn pack(&self, p: &mut Packer) {
        p.u32(self.kind());
        p.u32(self.kind());
        match self {
            Self::Animation {
                duration,
                draw_mode,
                x,
                y,
                frames,
            } => {
                p.f32(*duration);
                p.u32(*draw_mode);
                p.i32(*x);
                p.i32(*y);
                p.count(frames.len());
                for f in frames {
                    p.u32(*f);
                }
            }
            Self::Pause { min, max } => {
                p.f32(*min);
                p.f32(*max);
            }
            Self::Image {
                picture,
                draw_mode,
                x,
                y,
            } => {
                p.u32(picture.0);
                p.u32(*draw_mode);
                p.i32(*x);
                p.i32(*y);
            }
            Self::Alpha { picture, x, y } | Self::Cursor { picture, x, y } => {
                p.u32(picture.0);
                p.i32(*x);
                p.i32(*y);
            }
            Self::Sound { sound } => p.u32(sound.0),
            Self::Jump { index, probability } => {
                p.u32(*index);
                p.f32(*probability);
            }
            Self::Message {
                message,
                probability,
            } => {
                p.u32(*message);
                p.f32(*probability);
            }
            Self::State { state } => p.u32(*state),
            Self::Video { x, y, file } => {
                p.i32(*x);
                p.i32(*y);
                p.narrow_string(file);
            }
        }
    }

    /// The records a media item names: an animation's frames; a picture, a transparency
    /// picture, a sound or a cursor's picture.
    // D2-11112, D2-11144
    #[must_use]
    pub fn sub_data_ids(&self) -> Vec<u32> {
        match self {
            Self::Animation { frames, .. } => frames.clone(),
            Self::Image { picture, .. }
            | Self::Alpha { picture, .. }
            | Self::Cursor { picture, .. } => vec![picture.0],
            Self::Sound { sound } => vec![sound.0],
            _ => Vec::new(),
        }
    }
}

impl Transition {
    /// A transition as made before it is read: no kind, nothing to change.
    // D2-11104
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            kind: 0,
            when: 0,
            state: 0,
            change: TransitionChange::None,
        }
    }

    /// The kind, when, the state, then the kind's change.
    fn pack(&self, p: &mut Packer) {
        p.u32(self.kind);
        p.u32(self.when);
        p.u32(self.state);
        match &self.change {
            TransitionChange::Move {
                end_x,
                end_y,
                start_x,
                start_y,
                duration,
                flags,
            } => {
                p.i32(*end_x);
                p.i32(*end_y);
                p.i32(*start_x);
                p.i32(*start_y);
                p.f64(*duration);
                p.u32(*flags);
            }
            TransitionChange::Resize {
                end_height,
                end_width,
                start_height,
                start_width,
                duration,
                flags,
            } => {
                p.i32(*end_height);
                p.i32(*end_width);
                p.i32(*start_height);
                p.i32(*start_width);
                p.f64(*duration);
                p.u32(*flags);
            }
            TransitionChange::Fade {
                end_alpha,
                start_alpha,
                duration,
                flags,
            } => {
                p.f32(*end_alpha);
                p.f32(*start_alpha);
                p.f64(*duration);
                p.u32(*flags);
            }
            TransitionChange::Rotate {
                end_x,
                end_y,
                end_z,
                duration,
                flags,
            } => {
                p.i32(*end_x);
                p.i32(*end_y);
                p.i32(*end_z);
                p.f64(*duration);
                p.u32(*flags);
            }
            TransitionChange::None => {}
        }
    }
}

impl ElementState {
    /// The id, a flags byte, the rectangle and depth (16 bits each), alignment, then counted lists
    /// of attributes, media, transitions and child elements, then alignment.
    // D2-11084, D2-11085
    fn pack(&self, p: &mut Packer) {
        p.u32(self.id);
        p.u8(self.flags);
        p.i16(self.left);
        p.i16(self.top);
        p.i16(self.right);
        p.i16(self.bottom);
        p.u16(self.z);
        p.align();
        p.count(self.attributes.len());
        for a in &self.attributes {
            a.pack(p);
        }
        p.count(self.media.len());
        for m in &self.media {
            m.pack(p);
        }
        p.count(self.transitions.len());
        for t in &self.transitions {
            t.pack(p);
        }
        p.count(self.children.len());
        for c in &self.children {
            c.pack(p);
        }
        p.align();
    }

    /// The records a state names: its children's, then its attributes', then its media's.
    // D2-11086
    fn sub_data_ids(&self, out: &mut Vec<u32>) {
        for c in &self.children {
            c.sub_data_ids(out);
        }
        out.extend(self.attributes.iter().filter_map(Attribute::sub_data_id));
        for m in &self.media {
            out.extend(m.sub_data_ids());
        }
    }

    /// A state as made before it is read: no id, no rectangle of its own (every edge -1), the
    /// deepest depth, nothing in it.
    // D2-11091
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            id: 0,
            flags: 0,
            left: -1,
            top: -1,
            right: -1,
            bottom: -1,
            z: 0xFFFF,
            attributes: Vec::new(),
            media: Vec::new(),
            transitions: Vec::new(),
            children: Vec::new(),
        }
    }
}

impl Element {
    /// The id, the kind, the default state, the rectangle and depth (16 bits each), the four edge
    /// bytes, a 16-bit count of states, alignment, the states, alignment.
    // D2-11074, D2-11075
    fn pack(&self, p: &mut Packer) {
        p.u32(self.id);
        p.u32(self.kind);
        p.u32(self.default_state);
        p.i16(self.left);
        p.i16(self.top);
        p.i16(self.right);
        p.i16(self.bottom);
        p.u16(self.z);
        for e in self.edges {
            p.u8(e);
        }
        p.u16(u16::try_from(self.states.len()).unwrap_or(u16::MAX));
        p.align();
        for s in &self.states {
            s.pack(p);
        }
        p.align();
    }

    /// The records an element names, state by state.
    // D2-11076
    fn sub_data_ids(&self, out: &mut Vec<u32>) {
        for s in &self.states {
            s.sub_data_ids(out);
        }
    }
}

impl UiLayout {
    /// The layout as the record stores it under `id`: the id, the counted list of records it
    /// uses, the string table, the width and height (16 bits each), a 16-bit count of elements,
    /// alignment, the elements.
    // D2-11060
    #[must_use]
    pub fn pack(&self, id: u32) -> Vec<u8> {
        let mut p = Packer::new(true);
        self.pack_into(id, &mut p);
        p.out.unwrap_or_default()
    }

    /// How many bytes the record takes.
    // D2-11059
    #[must_use]
    pub fn pack_size(&self) -> usize {
        let mut p = Packer::new(false);
        self.pack_into(0, &mut p);
        p.len
    }

    fn pack_into(&self, id: u32, p: &mut Packer) {
        p.u32(id);
        p.count(self.references.len());
        for r in &self.references {
            p.u32(*r);
        }
        p.u32(self.string_table.0);
        p.u16(self.width);
        p.u16(self.height);
        p.u16(u16::try_from(self.elements.len()).unwrap_or(u16::MAX));
        p.align();
        for e in &self.elements {
            e.pack(p);
        }
    }

    /// The records a layout names: those it lists, then every element's pictures, sounds and
    /// files.
    // D2-11061
    #[must_use]
    pub fn sub_data_ids(&self) -> Vec<u32> {
        let mut out = self.references.clone();
        for e in &self.elements {
            e.sub_data_ids(&mut out);
        }
        out
    }
}
