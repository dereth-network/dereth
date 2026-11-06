//! A player's options: the shortcut bar, the interface's window places and opacities, the chat
//! window settings and the game options. The client sends each as it changes; the server keeps
//! them and hands them back, all together, in the session start of the character's next entry.
//!
//! They travel as one packed record: a flags word saying which parts differ from the client's
//! defaults (and a second word, zero), then each part present, in a fixed order:
//!
//! ```text
//! 0x1     aliases              table of (text, text)
//! 0x2     shortcuts            u32 n, n shortcuts (a kind word, then 0, 1 or 2 words or a text)
//! 0x4     shortcut set         u32
//! 0x1000  shortcut bar height  u32
//! 0x8     damage text range    f32
//! 0x10    window places        table of (window, table of 6-word places)
//! 0x20    radar mask           u32
//! 0x40    chat filters         table of (window, mask)
//! 0x80    option bits          u32 (top bit always set)
//!         version              u32 (6), when the option bits' top bit is set
//! 0x100   chat font colors     table of (text kind, color)
//! 0x200   chat font sizes      table of (text kind, size)
//! 0x2000  devoted chat windows table of (window, channel mask)
//! 0x400   chat pop-up flags    table of (text kind, flag)
//! 0x800   window opacities     table of (window, opacity)
//! ```
//!
//! A table is a header word (the count in the low 24 bits, the bucket count's place on top),
//! then its entries. The server keeps the record in this packed form, so what it stores is what
//! it sends.

use auberean_script::wire::{In, Out};

/// The record's version, the client's own.
pub const VERSION: u32 = 6;

/// The option bits' top bit, which the client's packer always sets.
pub const BITS_PACKED: u32 = 0x8000_0000;

/// How many shortcut places the bar has.
pub const SHORTCUTS: usize = 100;

/// A shortcut's kind word for a skill.
pub const SHORTCUT_SKILL: u32 = 0x4000_0001;

/// Part flags.
pub mod part {
    pub const ALIASES: u32 = 0x1;
    pub const SHORTCUTS: u32 = 0x2;
    pub const SHORTCUT_SET: u32 = 0x4;
    pub const DAMAGE_TEXT_RANGE: u32 = 0x8;
    pub const WINDOW_PLACES: u32 = 0x10;
    pub const RADAR_MASK: u32 = 0x20;
    pub const CHAT_FILTERS: u32 = 0x40;
    pub const OPTION_BITS: u32 = 0x80;
    pub const FONT_COLORS: u32 = 0x100;
    pub const FONT_SIZES: u32 = 0x200;
    pub const POPUP_FLAGS: u32 = 0x400;
    pub const OPACITIES: u32 = 0x800;
    pub const SHORTCUT_HEIGHT: u32 = 0x1000;
    pub const DEVOTED_WINDOWS: u32 = 0x2000;
}

/// A table of word pairs, in packing order, with its bucket count's place.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pairs {
    pub size_index: u8,
    pub pairs: Vec<(u32, u32)>,
}

impl Pairs {
    fn read(r: &mut In<'_>) -> Result<Self, String> {
        let h = r.u32().map_err(|e| e.to_string())?;
        let n = (h & 0x00FF_FFFF) as usize;
        let n = r.count(n, 8).map_err(|e| e.to_string())?;
        let mut pairs = Vec::with_capacity(n);
        for _ in 0..n {
            pairs.push((
                r.u32().map_err(|e| e.to_string())?,
                r.u32().map_err(|e| e.to_string())?,
            ));
        }
        #[allow(clippy::cast_possible_truncation)]
        Ok(Self {
            size_index: (h >> 24) as u8,
            pairs,
        })
    }

    fn write(&self, w: &mut Out) {
        #[allow(clippy::cast_possible_truncation)]
        w.u32(u32::from(self.size_index) << 24 | (self.pairs.len() as u32 & 0x00FF_FFFF));
        for (k, v) in &self.pairs {
            w.u32(*k);
            w.u32(*v);
        }
    }

    /// The table in a packed native hash's form (header word and pairs).
    ///
    /// # Errors
    /// The bytes are not one such table, exactly.
    pub fn from_packed(bytes: &[u8]) -> Result<Self, String> {
        let mut r = In::new(bytes);
        let t = Self::read(&mut r)?;
        r.finish().map_err(|e| e.to_string())?;
        Ok(t)
    }
}

/// A player's options.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Options {
    /// The aliases part as packed (its table header and entries), kept as it came.
    pub aliases: Option<Vec<u8>>,
    /// Each shortcut place's packed shortcut (a kind word and its data); empty when the part is
    /// absent.
    pub shortcuts: Vec<Vec<u8>>,
    pub shortcut_set: Option<u32>,
    pub shortcut_height: Option<u32>,
    pub damage_text_range: Option<f32>,
    /// The window places as packed (a table of tables), kept as the client sent them.
    pub window_places: Option<Vec<u8>>,
    pub radar_mask: Option<u32>,
    pub chat_filters: Option<Pairs>,
    pub option_bits: u32,
    pub font_colors: Option<Pairs>,
    pub font_sizes: Option<Pairs>,
    pub devoted_windows: Option<Pairs>,
    pub popup_flags: Option<Pairs>,
    pub opacities: Option<Pairs>,
}

/// The bytes a reading step consumes.
fn span(
    r: &mut In<'_>,
    step: impl FnOnce(&mut In<'_>) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    let mut from = r.clone();
    let start = r.pos();
    step(r)?;
    Ok(from
        .bytes(r.pos() - start)
        .map_err(|e| e.to_string())?
        .to_vec())
}

fn shortcut(r: &mut In<'_>) -> Result<Vec<u8>, String> {
    span(r, shortcut_step)
}

fn shortcut_step(r: &mut In<'_>) -> Result<(), String> {
    let e = |e: auberean_script::wire::CodecError| e.to_string();
    match r.u32().map_err(e)? {
        0 => {}
        0x4000_0001 | 0x4000_0004 | 0x4000_0006 => {
            r.u32().map_err(e)?;
        }
        0x4000_0002 => {
            r.u32().map_err(e)?;
            r.u32().map_err(e)?;
        }
        0x4000_0003 => {
            r.wide().map_err(e)?;
        }
        k => return Err(format!("a shortcut of kind {k:#x}")),
    }
    Ok(())
}

/// Read one shortcut's packed form, exactly.
///
/// # Errors
/// Not a shortcut, or bytes left over.
pub fn shortcut_from_packed(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut r = In::new(bytes);
    let s = shortcut(&mut r)?;
    r.finish().map_err(|e| e.to_string())?;
    Ok(s)
}

/// Step over the window places' packed form (a table of windows, each a table of six-word
/// places), returning its bytes.
fn window_places(r: &mut In<'_>) -> Result<Vec<u8>, String> {
    span(r, window_places_step)
}

fn window_places_step(r: &mut In<'_>) -> Result<(), String> {
    let e = |e: auberean_script::wire::CodecError| e.to_string();
    let n = (r.u32().map_err(e)? & 0x00FF_FFFF) as usize;
    let n = r.count(n, 8).map_err(e)?;
    for _ in 0..n {
        r.u32().map_err(e)?;
        let m = (r.u32().map_err(e)? & 0x00FF_FFFF) as usize;
        let m = r.count(m, 24).map_err(e)?;
        r.bytes(m * 24).map_err(e)?;
    }
    Ok(())
}

/// Read the window places' packed form, exactly.
///
/// # Errors
/// Not window places, or bytes left over.
pub fn window_places_from_packed(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut r = In::new(bytes);
    let s = window_places(&mut r)?;
    r.finish().map_err(|e| e.to_string())?;
    Ok(s)
}

fn aliases(r: &mut In<'_>) -> Result<Vec<u8>, String> {
    span(r, |r| {
        let e = |e: auberean_script::wire::CodecError| e.to_string();
        let n = (r.u32().map_err(e)? & 0x00FF_FFFF) as usize;
        let n = r.count(n, 4).map_err(e)?;
        for _ in 0..n {
            r.wide().map_err(e)?;
            r.wide().map_err(e)?;
        }
        Ok(())
    })
}

/// The longest alias, and the longest text one stands for, in units. The 39 recorded calls that set
/// one were 128 bytes at most, and a session's most was 3.
pub const MOST_ALIAS_UNITS: usize = 1024;
/// The most a character's aliases take in all, in bytes: over two thousand of the longest recorded.
pub const MOST_ALIAS_BYTES: usize = 128 * 1024;
/// The most a call that sets a part of the options may carry, in bytes. The largest recorded of the
/// 19,000 such calls was 644.
pub const MOST_PART_BYTES: usize = 4 * 1024;

/// What [`Options::keep_within_bounds`] left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LeftOut {
    pub aliases: usize,
    pub parts: usize,
}

impl LeftOut {
    /// Did it leave out anything?
    #[must_use]
    pub const fn any(self) -> bool {
        self.aliases > 0 || self.parts > 0
    }
}

/// The bytes a table of aliases takes in all.
fn alias_bytes(pairs: &[(Vec<u16>, Vec<u16>)]) -> usize {
    pairs
        .iter()
        .map(|(a, t)| 2 * (a.len() + t.len()) + 8)
        .sum::<usize>()
}

/// Aliases as wide strings: each (alias, text).
type AliasPairs = Vec<(Vec<u16>, Vec<u16>)>;

/// The aliases of a packed aliases part: its bucket count's place and each (alias, text).
fn alias_pairs(packed: &[u8]) -> Result<(u8, AliasPairs), String> {
    let e = |e: auberean_script::wire::CodecError| e.to_string();
    let mut r = In::new(packed);
    let h = r.u32().map_err(e)?;
    let n = r.count((h & 0x00FF_FFFF) as usize, 4).map_err(e)?;
    let mut pairs = Vec::with_capacity(n);
    for _ in 0..n {
        pairs.push((r.wide().map_err(e)?, r.wide().map_err(e)?));
    }
    r.finish().map_err(e)?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(((h >> 24) as u8, pairs))
}

impl Options {
    /// Add an alias for a text (one of the same name replaced), or remove it. The client keeps its
    /// own table and only tells the server, which hands the table back at the next entry.
    ///
    /// Returns false, changing nothing, for one past [`MOST_ALIAS_UNITS`] or [`MOST_ALIAS_BYTES`].
    pub fn set_alias(&mut self, alias: &str, text: &str, add: bool) -> bool {
        let (size_index, mut pairs) = self
            .aliases
            .as_deref()
            .and_then(|a| alias_pairs(a).ok())
            .unwrap_or_default();
        let name: Vec<u16> = alias.encode_utf16().collect();
        let text: Vec<u16> = text.encode_utf16().collect();
        pairs.retain(|(a, _)| *a != name);
        if add {
            let longest = name.len().max(text.len());
            let after = alias_bytes(&pairs) + 2 * (name.len() + text.len()) + 8;
            if longest > MOST_ALIAS_UNITS || after > MOST_ALIAS_BYTES {
                return false;
            }
            pairs.push((name, text));
        }
        self.set_aliases(size_index, &pairs);
        true
    }

    fn set_aliases(&mut self, size_index: u8, pairs: &[(Vec<u16>, Vec<u16>)]) {
        if pairs.is_empty() {
            self.aliases = None;
            return;
        }
        let mut w = Out::new();
        #[allow(clippy::cast_possible_truncation)]
        w.u32(u32::from(size_index) << 24 | (pairs.len() as u32 & 0x00FF_FFFF));
        for (a, t) in pairs {
            w.wide(a);
            w.wide(t);
        }
        self.aliases = Some(w.buf);
    }

    /// Bring options kept before these bounds, or from a file, within them, as a character enters.
    /// An alias keeps its place unless it is itself too long or the ones before it have used up the
    /// room; a part too large to have come from a client's own use goes back to the client's default.
    /// What is within the bounds is left as it is, to the byte.
    pub fn keep_within_bounds(&mut self) -> LeftOut {
        let mut out = LeftOut::default();
        if let Some((size_index, pairs)) = self.aliases.as_deref().and_then(|a| alias_pairs(a).ok())
        {
            if alias_bytes(&pairs) > MOST_ALIAS_BYTES
                || pairs
                    .iter()
                    .any(|(a, t)| a.len().max(t.len()) > MOST_ALIAS_UNITS)
            {
                let mut kept = Vec::new();
                let mut room = MOST_ALIAS_BYTES;
                for (a, t) in pairs {
                    let takes = 2 * (a.len() + t.len()) + 8;
                    if a.len().max(t.len()) > MOST_ALIAS_UNITS || takes > room {
                        out.aliases += 1;
                    } else {
                        room -= takes;
                        kept.push((a, t));
                    }
                }
                self.set_aliases(size_index, &kept);
            }
        }
        for place in &mut self.shortcuts {
            if place.len() > MOST_PART_BYTES {
                *place = 0u32.to_le_bytes().to_vec();
                out.parts += 1;
            }
        }
        if self
            .window_places
            .as_ref()
            .is_some_and(|w| w.len() > MOST_PART_BYTES)
        {
            self.window_places = None;
            out.parts += 1;
        }
        for table in [
            &mut self.chat_filters,
            &mut self.font_colors,
            &mut self.font_sizes,
            &mut self.devoted_windows,
            &mut self.popup_flags,
            &mut self.opacities,
        ] {
            if table
                .as_ref()
                .is_some_and(|t| t.pairs.len() * 8 > MOST_PART_BYTES)
            {
                *table = None;
                out.parts += 1;
            }
        }
        out
    }

    /// The aliases kept, each (alias, text).
    #[must_use]
    pub fn alias_list(&self) -> Vec<(String, String)> {
        self.aliases
            .as_deref()
            .and_then(|a| alias_pairs(a).ok())
            .map(|(_, p)| {
                p.iter()
                    .map(|(a, t)| (String::from_utf16_lossy(a), String::from_utf16_lossy(t)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The flags word: which parts are present.
    #[must_use]
    pub fn flags(&self) -> u32 {
        let mut f = part::OPTION_BITS;
        let mut set = |b: bool, bit: u32| {
            if b {
                f |= bit;
            }
        };
        set(self.aliases.is_some(), part::ALIASES);
        set(!self.shortcuts.is_empty(), part::SHORTCUTS);
        set(self.shortcut_set.is_some(), part::SHORTCUT_SET);
        set(self.shortcut_height.is_some(), part::SHORTCUT_HEIGHT);
        set(self.damage_text_range.is_some(), part::DAMAGE_TEXT_RANGE);
        set(self.window_places.is_some(), part::WINDOW_PLACES);
        set(self.radar_mask.is_some(), part::RADAR_MASK);
        set(self.chat_filters.is_some(), part::CHAT_FILTERS);
        set(self.font_colors.is_some(), part::FONT_COLORS);
        set(self.font_sizes.is_some(), part::FONT_SIZES);
        set(self.devoted_windows.is_some(), part::DEVOTED_WINDOWS);
        set(self.popup_flags.is_some(), part::POPUP_FLAGS);
        set(self.opacities.is_some(), part::OPACITIES);
        f
    }

    /// The packed record.
    #[must_use]
    pub fn to_packed(&self) -> Vec<u8> {
        let mut w = Out::new();
        w.u32(self.flags());
        w.u32(0);
        if let Some(a) = &self.aliases {
            w.bytes(a);
        }
        if !self.shortcuts.is_empty() {
            #[allow(clippy::cast_possible_truncation)]
            w.u32(self.shortcuts.len() as u32);
            for s in &self.shortcuts {
                w.bytes(s);
            }
        }
        for v in [self.shortcut_set, self.shortcut_height]
            .into_iter()
            .flatten()
        {
            w.u32(v);
        }
        if let Some(v) = self.damage_text_range {
            w.u32(v.to_bits());
        }
        if let Some(p) = &self.window_places {
            w.bytes(p);
        }
        if let Some(v) = self.radar_mask {
            w.u32(v);
        }
        if let Some(t) = &self.chat_filters {
            t.write(&mut w);
        }
        w.u32(self.option_bits | BITS_PACKED);
        w.u32(VERSION);
        for t in [
            &self.font_colors,
            &self.font_sizes,
            &self.devoted_windows,
            &self.popup_flags,
            &self.opacities,
        ]
        .into_iter()
        .flatten()
        {
            t.write(&mut w);
        }
        w.buf
    }

    /// Read a packed record, exactly.
    ///
    /// # Errors
    /// A part that does not read, a version this server does not know, or bytes left over.
    pub fn from_packed(bytes: &[u8]) -> Result<Self, String> {
        let mut r = In::new(bytes);
        let e = |e: auberean_script::wire::CodecError| e.to_string();
        let f = r.u32().map_err(e)?;
        r.u32().map_err(e)?;
        let has = |bit: u32| f & bit != 0;
        let mut o = Self::default();
        if has(part::ALIASES) {
            o.aliases = Some(aliases(&mut r)?);
        }
        if has(part::SHORTCUTS) {
            let n = r.u32().map_err(e)? as usize;
            let n = r.count(n, 4).map_err(e)?;
            for _ in 0..n {
                o.shortcuts.push(shortcut(&mut r)?);
            }
        }
        if has(part::SHORTCUT_SET) {
            o.shortcut_set = Some(r.u32().map_err(e)?);
        }
        if has(part::SHORTCUT_HEIGHT) {
            o.shortcut_height = Some(r.u32().map_err(e)?);
        }
        if has(part::DAMAGE_TEXT_RANGE) {
            o.damage_text_range = Some(f32::from_bits(r.u32().map_err(e)?));
        }
        if has(part::WINDOW_PLACES) {
            o.window_places = Some(window_places(&mut r)?);
        }
        if has(part::RADAR_MASK) {
            o.radar_mask = Some(r.u32().map_err(e)?);
        }
        if has(part::CHAT_FILTERS) {
            o.chat_filters = Some(Pairs::read(&mut r)?);
        }
        if !has(part::OPTION_BITS) {
            return Err("option bits are always sent".into());
        }
        o.option_bits = r.u32().map_err(e)? & !BITS_PACKED;
        let version = r.u32().map_err(e)?;
        if version != VERSION {
            return Err(format!("options of version {version}"));
        }
        for (bit, slot) in [
            (part::FONT_COLORS, &mut o.font_colors),
            (part::FONT_SIZES, &mut o.font_sizes),
            (part::DEVOTED_WINDOWS, &mut o.devoted_windows),
            (part::POPUP_FLAGS, &mut o.popup_flags),
            (part::OPACITIES, &mut o.opacities),
        ] {
            if has(bit) {
                *slot = Some(Pairs::read(&mut r)?);
            }
        }
        r.finish().map_err(e)?;
        Ok(o)
    }

    /// A new character's options, as the January 2017 servers gave the one character created in
    /// the recordings: the shortcut bar holding the race's first three starting skills in its
    /// first places and its common skill in the tenth; every chat and window setting at the
    /// value the servers sent.
    #[must_use]
    pub fn new_character(skills: &[(u32, u32)]) -> Self {
        let mut shortcuts = vec![0u32.to_le_bytes().to_vec(); SHORTCUTS];
        let skill = |id: u32| {
            let mut v = SHORTCUT_SKILL.to_le_bytes().to_vec();
            v.extend_from_slice(&id.to_le_bytes());
            v
        };
        let first: Vec<u32> = skills
            .iter()
            .filter(|(_, level)| *level == 0)
            .map(|(id, _)| *id)
            .collect();
        if let Some((common, own)) = first.split_first() {
            for (i, id) in own.iter().take(3).enumerate() {
                shortcuts[i] = skill(*id);
            }
            shortcuts[9] = skill(*common);
        }
        let pairs = |v: &[(u32, u32)]| {
            Some(Pairs {
                size_index: 2,
                pairs: v.to_vec(),
            })
        };
        Self {
            shortcuts,
            damage_text_range: Some(1.0),
            radar_mask: Some(0xFFFF_FFFF),
            chat_filters: pairs(&NEW_CHAT_FILTERS),
            option_bits: NEW_OPTION_BITS,
            font_colors: pairs(&NEW_FONT_COLORS),
            font_sizes: pairs(&NEW_FONT_COLORS.map(|(k, _)| (k, 0))),
            devoted_windows: pairs(&NEW_DEVOTED_WINDOWS),
            popup_flags: pairs(&NEW_POPUP_FLAGS),
            ..Self::default()
        }
    }

    /// Take the game options the client sends: the damage text range, the shortcut bar's height,
    /// the radar mask and the option bits.
    pub fn set_game_options(&mut self, range: f32, height: u32, radar: u32, bits: u32) {
        self.damage_text_range = Some(range);
        self.shortcut_height = Some(height);
        self.radar_mask = Some(radar);
        self.option_bits = bits & !BITS_PACKED;
    }

    /// Put a shortcut in a place of the bar (a place past the bar is ignored).
    pub fn set_shortcut(&mut self, place: usize, packed: Vec<u8>) {
        if place >= SHORTCUTS {
            return;
        }
        if self.shortcuts.len() < SHORTCUTS {
            self.shortcuts
                .resize(SHORTCUTS, 0u32.to_le_bytes().to_vec());
        }
        self.shortcuts[place] = packed;
    }
}

/// The option bits the 2017 servers gave a new character.
pub const NEW_OPTION_BITS: u32 = 0x0000_03F5;

/// A new character's chat filters: per chat window, the kinds of text it shows.
pub const NEW_CHAT_FILTERS: [(u32, u32); 4] = [
    (1, 0xFFFF_FFFF),
    (2, 0x0008_0000),
    (3, 0x0001_0000),
    (4, 0x0002_0000),
];

/// A new character's chat font colors, per kind of text (each kind a bit; the sizes are the same
/// kinds, all 0).
pub const NEW_FONT_COLORS: [(u32, u32); 18] = [
    (1 << 23, 4),
    (1 << 0, 0),
    (1 << 24, 4),
    (1 << 1, 1),
    (1 << 19, 4),
    (1 << 2, 2),
    (1 << 20, 4),
    (1 << 3, 3),
    (1 << 21, 4),
    (1 << 4, 4),
    (1 << 6, 6),
    (1 << 16, 7),
    (1 << 8, 9),
    (1 << 22, 4),
    (1 << 18, 4),
    (1 << 5, 5),
    (1 << 7, 4),
    (1 << 17, 8),
];

/// A new character's chat windows given to one channel each.
pub const NEW_DEVOTED_WINDOWS: [(u32, u32); 4] = [
    (1, 0x10),
    (2, 0x0008_0000),
    (3, 0x0001_0000),
    (4, 0x0002_0000),
];

/// A new character's chat pop-up flags.
pub const NEW_POPUP_FLAGS: [(u32, u32); 3] = [(0x0001_0000, 1), (0x0000_0100, 1), (0x0002_0000, 1)];
