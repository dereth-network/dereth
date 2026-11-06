//! The player's options as the engine holds them, and the shortcuts among them.
//!
//! **A shortcut** is what a toolbar slot holds: a kind, and the one datum its kind uses (a skill
//! or an emote by its enumerated value, an item by its instance id, a line of text, a recipe by
//! its data id). A new one is of kind 0 with every datum zero or empty; it packs as its kind and
//! that one datum, so the others are not carried.
//!
//! **The options profile** holds the player's aliases, the shortcut slots, which set of shortcuts
//! the toolbar shows and its height, how far away others' damage shows, the windows' saved
//! places, the radar's mask, the chat filters, a word of yes-or-no options, and the chat's font
//! colors, font sizes, devoted windows, popup flags and the windows' opacities. Its packed form
//! leads with a 64-bit mask naming the parts that differ from a new profile's, then those parts
//! in a fixed order; the option word's top bit says a version follows, and the version says which
//! of the chat's parts the form holds (the font tables from version 1, the popup flags and
//! opacities from 2, the devoted windows from 6).
//!
//! A packed profile is read as a new profile reads one; the parts the engine fills with defaults
//! when a profile is made (chat filters, font colors and sizes, devoted windows) are held here
//! only when the packed form carries them.
//!
//! **The aliases** are a hash table of wide strings: an alias lands in bucket `hash % buckets` of
//! the table's bucket count (47 in a new profile; a packed table names its own), the hash being
//! the wide strings' own ([`auberean_text::info::string_hash`]). The table is walked bucket by
//! bucket, each chain from its head, and packs in that order; a new alias goes to the head of its
//! chain, a name already held takes its new text in place. Names are compared exactly.

use crate::native_object::{native_type, step_over, BUCKET_SIZES};
use crate::wire::{In, Out, Result};

/// The shortcut kinds.
pub mod kind {
    /// An attack or another skill, by its enumerated value.
    pub const SKILL: u32 = 0x4000_0001;
    /// An item, by its instance id.
    pub const ITEM: u32 = 0x4000_0002;
    /// A line of text, typed as if into the chat box.
    pub const TEXT: u32 = 0x4000_0003;
    /// An emote, by its enumerated value.
    pub const EMOTE: u32 = 0x4000_0004;
    /// A recipe, by its data id.
    pub const RECIPE: u32 = 0x4000_0006;
}

/// One shortcut. A copy is a copy of its kind and of every value it holds.
// D2-12179, D2-12178
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shortcut {
    pub kind: u32,
    pub item: u64,
    pub data_enum: u32,
    pub data_id: u32,
    pub text: Vec<u16>,
}

impl Shortcut {
    /// Read a packed shortcut into this one: its kind, then the datum the kind carries (the
    /// others keep their values; a kind that carries none reads nothing more).
    // D2-12180
    pub fn read_over(&mut self, r: &mut In<'_>) -> Result<()> {
        self.kind = r.u32()?;
        match self.kind {
            kind::SKILL | kind::EMOTE => self.data_enum = r.u32()?,
            kind::ITEM => self.item = r.u64()?,
            kind::TEXT => self.text = r.wide()?,
            kind::RECIPE => self.data_id = r.u32()?,
            _ => {}
        }
        Ok(())
    }

    /// A packed shortcut read into a new one.
    pub fn read(r: &mut In<'_>) -> Result<Self> {
        let mut s = Self::default();
        s.read_over(r)?;
        Ok(s)
    }

    /// The packed form: the kind, then the datum it carries.
    // D2-12180
    pub fn write(&self, w: &mut Out) {
        w.u32(self.kind);
        match self.kind {
            kind::SKILL | kind::EMOTE => w.u32(self.data_enum),
            kind::ITEM => w.u64(self.item),
            kind::TEXT => w.wide(&self.text),
            kind::RECIPE => w.u32(self.data_id),
            _ => {}
        }
    }

    /// The packed form.
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = Out::new();
        self.write(&mut w);
        w.buf
    }
}

/// A hash table of the profile kept as packed: its header word (the count, and the bucket size's
/// place in the top byte) and its entries in packed order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table<T> {
    pub header: u32,
    pub entries: Vec<T>,
}

impl<T> Table<T> {
    /// Whether it holds anything (the count the mask tests).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn read_words(r: &mut In<'_>) -> Result<Table<(u32, u32)>> {
    let header = r.u32()?;
    let n = r.count((header & 0x00FF_FFFF) as usize, 8)?;
    let mut entries = Vec::with_capacity(n);
    for _ in 0..n {
        entries.push((r.u32()?, r.u32()?));
    }
    Ok(Table { header, entries })
}

fn write_words(w: &mut Out, t: &Table<(u32, u32)>) {
    w.u32(t.header);
    for (k, v) in &t.entries {
        w.u32(*k);
        w.u32(*v);
    }
}

/// The mask bits of the profile's parts.
pub mod part {
    pub const ALIASES: u32 = 0x1;
    pub const SHORTCUTS: u32 = 0x2;
    pub const SHORTCUT_SET: u32 = 0x4;
    pub const DAMAGE_TEXT_RANGE: u32 = 0x8;
    pub const UI_LOCATIONS: u32 = 0x10;
    pub const RADAR_MASK: u32 = 0x20;
    pub const FILTERS: u32 = 0x40;
    pub const BITS: u32 = 0x80;
    pub const FONT_COLORS: u32 = 0x100;
    pub const FONT_SIZES: u32 = 0x200;
    pub const POPUP_FLAGS: u32 = 0x400;
    pub const OPACITIES: u32 = 0x800;
    pub const SHORTCUT_HEIGHT: u32 = 0x1000;
    pub const DEVOTED_WINDOWS: u32 = 0x2000;
}

/// The chat's tables as a new profile holds them (the engine fills them when it clears a
/// profile): text kinds' filters, colors and sizes, the devoted windows' channels, and the kinds
/// that pop the chat up. The rebuild holds a table only once a packed profile carried it; until
/// then the profile answers from these.
pub mod chat_defaults {
    /// Window to the text kinds it shows.
    // D2-11986
    pub const FILTERS: [(u32, u32); 4] = [
        (1, 0x60_0000 | 0x17F),
        (2, 0x0008_0000),
        (3, 0x0001_0000),
        (4, 0x0002_0000),
    ];
    /// Text kind to its color's number.
    // D2-11990
    pub const FONT_COLORS: [(u32, u32); 18] = [
        (0x1, 0),
        (0x2, 1),
        (0x4, 2),
        (0x8, 3),
        (0x10, 4),
        (0x20, 5),
        (0x40, 6),
        (0x80, 4),
        (0x1_0000, 7),
        (0x2_0000, 8),
        (0x40_0000, 4),
        (0x100_0000, 4),
        (0x80_0000, 4),
        (0x4_0000, 4),
        (0x8_0000, 4),
        (0x10_0000, 4),
        (0x20_0000, 4),
        (0x100, 9),
    ];
    /// Text kind to its size (every kind small).
    // D2-11990
    pub const FONT_SIZES: [(u32, u32); 18] = [
        (0x1, 0),
        (0x2, 0),
        (0x4, 0),
        (0x8, 0),
        (0x10, 0),
        (0x20, 0),
        (0x40, 0),
        (0x80, 0),
        (0x1_0000, 0),
        (0x2_0000, 0),
        (0x40_0000, 0),
        (0x100_0000, 0),
        (0x80_0000, 0),
        (0x4_0000, 0),
        (0x8_0000, 0),
        (0x10_0000, 0),
        (0x20_0000, 0),
        (0x100, 0),
    ];
    /// Devoted window to the channel it shows.
    // D2-11990
    pub const DEVOTED_WINDOWS: [(u32, u32); 4] =
        [(1, 0x10), (2, 0x8_0000), (3, 0x1_0000), (4, 0x2_0000)];
    /// Text kind to whether it pops the chat up.
    // D2-11992
    pub const POPUP_FLAGS: [(u32, u32); 3] = [(0x100, 1), (0x2_0000, 1), (0x1_0000, 1)];
    /// The channel a devoted window shows when it has none.
    pub const NO_DEVOTED_CHANNEL: u32 = 0x10;
}

/// A key's value in a table of words (the first entry with the key), else in the table a new
/// profile holds when this one never had the table.
fn word_in(table: Option<&Table<(u32, u32)>>, defaults: &[(u32, u32)], key: u32) -> Option<u32> {
    let found = |e: &[(u32, u32)]| e.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
    match table {
        Some(t) => found(&t.entries),
        None => found(defaults),
    }
}

/// The option word's top bit: a version follows it.
pub const VERSIONED: u32 = 0x8000_0000;
/// The version the engine writes.
pub const VERSION: u32 = 6;

/// The packed form of an empty table of the windows' saved places: no entries, in a table of 47
/// buckets.
pub const NO_UI_LOCATIONS: [u8; 4] = [0, 0, 0, 2];

/// The player's options. A copy takes every part; dropped, every part goes.
// D2-12007, D2-12003, D2-01961
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameplayOptions {
    /// Alias to what it stands for, both wide strings.
    // D2-01464
    pub aliases: Table<(Vec<u16>, Vec<u16>)>,
    pub shortcuts: Vec<Shortcut>,
    pub shortcut_set: u32,
    pub shortcut_height: u32,
    /// How far away others' damage shows, a 32-bit float's bits.
    pub damage_text_range: u32,
    /// The windows' saved places, as packed.
    pub ui_locations: Vec<u8>,
    pub radar_mask: u32,
    pub filters: Option<Table<(u32, u32)>>,
    /// The yes-or-no options (the top bit marks a versioned profile).
    pub bits: u32,
    pub version: u32,
    pub font_colors: Option<Table<(u32, u32)>>,
    pub font_sizes: Option<Table<(u32, u32)>>,
    pub devoted_windows: Option<Table<(u32, u32)>>,
    pub popup_flags: Table<(u32, u32)>,
    /// Window to opacity (a 32-bit float's bits).
    pub opacities: Table<(u32, u32)>,
}

impl Default for GameplayOptions {
    /// A new profile, as the engine makes one (less the chat's defaults, see above).
    // D2-12006, D2-11996
    fn default() -> Self {
        Self {
            aliases: Table {
                header: 0x0200_0000,
                entries: Vec::new(),
            },
            shortcuts: Vec::new(),
            shortcut_set: 0,
            shortcut_height: 1,
            damage_text_range: (-1.0f32).to_bits(),
            ui_locations: NO_UI_LOCATIONS.to_vec(),
            radar_mask: 0,
            filters: None,
            bits: 0,
            version: 0,
            font_colors: None,
            font_sizes: None,
            devoted_windows: None,
            popup_flags: Table {
                header: 0x0200_0000,
                entries: Vec::new(),
            },
            opacities: Table {
                header: 0x0200_0000,
                entries: Vec::new(),
            },
        }
    }
}

impl GameplayOptions {
    /// The mask of the parts that differ from a new profile's.
    // D2-11965
    #[must_use]
    pub fn mask(&self) -> u32 {
        let d = Self::default();
        let held = |t: &Option<Table<(u32, u32)>>| t.as_ref().is_some_and(|t| !t.is_empty());
        let mut m = 0;
        for (bit, on) in [
            (part::ALIASES, !self.aliases.is_empty()),
            (part::SHORTCUTS, !self.shortcuts.is_empty()),
            (part::SHORTCUT_SET, self.shortcut_set != d.shortcut_set),
            (
                part::SHORTCUT_HEIGHT,
                self.shortcut_height != d.shortcut_height,
            ),
            (
                part::DAMAGE_TEXT_RANGE,
                self.damage_text_range != d.damage_text_range,
            ),
            (part::UI_LOCATIONS, ui_locations_held(&self.ui_locations)),
            (part::RADAR_MASK, self.radar_mask != d.radar_mask),
            (part::FILTERS, held(&self.filters)),
            (part::BITS, self.bits != d.bits),
            (part::FONT_COLORS, held(&self.font_colors)),
            (part::FONT_SIZES, held(&self.font_sizes)),
            (part::POPUP_FLAGS, !self.popup_flags.is_empty()),
            (part::OPACITIES, !self.opacities.is_empty()),
            (part::DEVOTED_WINDOWS, held(&self.devoted_windows)),
        ] {
            if on {
                m |= bit;
            }
        }
        m
    }

    /// Read a packed profile into a new one, as the engine reads one.
    // D2-12002, D2-01447, D2-01468, D2-01469
    pub fn read(r: &mut In<'_>) -> Result<Self> {
        let mut o = Self::default();
        let m = r.u32()?;
        r.u32()?;
        if m & part::ALIASES != 0 {
            let header = r.u32()?;
            let n = r.count((header & 0x00FF_FFFF) as usize, 4)?;
            let mut entries = Vec::with_capacity(n);
            for _ in 0..n {
                entries.push((r.wide()?, r.wide()?));
            }
            o.aliases = Table { header, entries };
            // Each entry read joins the end of its bucket's chain: the table holds them by bucket.
            o.sort_aliases();
        }
        if m & part::SHORTCUTS != 0 {
            let n = r.u32()? as usize;
            let n = r.count(n, 4)?;
            o.shortcuts = (0..n).map(|_| Shortcut::read(r)).collect::<Result<_>>()?;
        }
        if m & part::SHORTCUT_SET != 0 {
            o.shortcut_set = r.u32()?;
        }
        if m & part::SHORTCUT_HEIGHT != 0 {
            o.shortcut_height = r.u32()?;
        }
        if m & part::DAMAGE_TEXT_RANGE != 0 {
            o.damage_text_range = r.u32()?;
        }
        if m & part::UI_LOCATIONS != 0 {
            let start = r.pos();
            let mut probe = r.clone();
            step_over(&mut probe, native_type::UI_SAVE_LOCATIONS)?;
            o.ui_locations = r.bytes(probe.pos() - start)?.to_vec();
        }
        if m & part::RADAR_MASK != 0 {
            o.radar_mask = r.u32()?;
        }
        if m & part::FILTERS != 0 {
            o.filters = Some(read_words(r)?);
        }
        if m & part::BITS != 0 {
            o.bits = r.u32()?;
        }
        if o.bits & VERSIONED == 0 {
            // An unversioned profile: the engine brings it to the current version, filling the
            // chat's parts with their defaults, which this reading does not hold.
            return r.fail("an options profile without a version");
        }
        o.version = r.u32()?;
        if o.version < VERSION {
            return r.fail(format!("an options profile of version {}", o.version));
        }
        if m & part::FONT_COLORS != 0 {
            o.font_colors = Some(read_words(r)?);
        }
        if m & part::FONT_SIZES != 0 {
            o.font_sizes = Some(read_words(r)?);
        }
        if m & part::DEVOTED_WINDOWS != 0 {
            o.devoted_windows = Some(read_words(r)?);
        }
        if m & part::POPUP_FLAGS != 0 {
            o.popup_flags = read_words(r)?;
        }
        if m & part::OPACITIES != 0 {
            o.opacities = read_words(r)?;
        }
        Ok(o)
    }

    /// The packed form: the mask of the parts that differ from a new profile's, then those parts.
    // D2-12002
    pub fn write(&self, w: &mut Out) {
        let m = self.mask();
        w.u32(m);
        w.u32(0);
        if m & part::ALIASES != 0 {
            w.u32(self.aliases.header);
            for (k, v) in &self.aliases.entries {
                w.wide(k);
                w.wide(v);
            }
        }
        if m & part::SHORTCUTS != 0 {
            #[allow(clippy::cast_possible_truncation)]
            w.u32(self.shortcuts.len() as u32);
            for s in &self.shortcuts {
                s.write(w);
            }
        }
        if m & part::SHORTCUT_SET != 0 {
            w.u32(self.shortcut_set);
        }
        if m & part::SHORTCUT_HEIGHT != 0 {
            w.u32(self.shortcut_height);
        }
        if m & part::DAMAGE_TEXT_RANGE != 0 {
            w.u32(self.damage_text_range);
        }
        if m & part::UI_LOCATIONS != 0 {
            w.bytes(&self.ui_locations);
        }
        if m & part::RADAR_MASK != 0 {
            w.u32(self.radar_mask);
        }
        if let Some(t) = self.filters.as_ref().filter(|_| m & part::FILTERS != 0) {
            write_words(w, t);
        }
        if m & part::BITS != 0 {
            w.u32(self.bits);
        }
        w.u32(VERSION);
        for (bit, t) in [
            (part::FONT_COLORS, self.font_colors.as_ref()),
            (part::FONT_SIZES, self.font_sizes.as_ref()),
            (part::DEVOTED_WINDOWS, self.devoted_windows.as_ref()),
            (part::POPUP_FLAGS, Some(&self.popup_flags)),
            (part::OPACITIES, Some(&self.opacities)),
        ] {
            if let Some(t) = t.filter(|_| m & bit != 0) {
                write_words(w, t);
            }
        }
    }

    /// The packed form.
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = Out::new();
        self.write(&mut w);
        w.buf
    }

    /// The aliases' bucket count: the place the table's header names.
    fn alias_buckets(&self) -> u32 {
        BUCKET_SIZES
            .get((self.aliases.header >> 24) as usize)
            .copied()
            .unwrap_or(BUCKET_SIZES[2])
    }

    fn alias_bucket(&self, name: &[u16]) -> u32 {
        auberean_text::info::string_hash(name) % self.alias_buckets()
    }

    /// The aliases in the table's order: by bucket, each chain in the order it holds them.
    fn sort_aliases(&mut self) {
        let n = self.alias_buckets();
        self.aliases
            .entries
            .sort_by_key(|(k, _)| auberean_text::info::string_hash(k) % n);
    }

    /// Keep the header's count with the entries.
    fn count_aliases(&mut self) {
        #[allow(clippy::cast_possible_truncation)]
        let n = self.aliases.entries.len() as u32 & 0x00FF_FFFF;
        self.aliases.header = self.aliases.header & 0xFF00_0000 | n;
    }

    /// What an alias stands for, when the table holds it.
    // D2-11981, D2-11980
    #[must_use]
    pub fn alias(&self, name: &[u16]) -> Option<&[u16]> {
        self.aliases
            .entries
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_slice())
    }

    /// Set an alias: a name already held takes the new text in place; a new one goes to the head
    /// of its bucket's chain. Answers 1, as the engine does.
    // D2-11997, D2-11995
    pub fn set_alias(&mut self, name: &[u16], text: &[u16]) -> bool {
        if let Some((_, v)) = self.aliases.entries.iter_mut().find(|(k, _)| k == name) {
            *v = text.to_vec();
            return true;
        }
        let b = self.alias_bucket(name);
        let at = self
            .aliases
            .entries
            .iter()
            .position(|(k, _)| self.alias_bucket(k) >= b)
            .unwrap_or(self.aliases.entries.len());
        self.aliases
            .entries
            .insert(at, (name.to_vec(), text.to_vec()));
        self.count_aliases();
        true
    }

    /// Take an alias out; whether the table held it.
    // D2-11983, D2-11979
    pub fn remove_alias(&mut self, name: &[u16]) -> bool {
        let Some(at) = self.aliases.entries.iter().position(|(k, _)| k == name) else {
            return false;
        };
        self.aliases.entries.remove(at);
        self.count_aliases();
        true
    }

    /// Every alias out; the table keeps its buckets.
    // D2-11982
    pub fn flush_aliases(&mut self) {
        self.aliases.entries.clear();
        self.count_aliases();
    }

    /// The aliases listed, a line `alias [name] == [text]` each, in the table's order.
    // D2-11969
    #[must_use]
    pub fn alias_list(&self) -> Vec<u16> {
        let mut out = Vec::new();
        for (k, v) in &self.aliases.entries {
            out.extend("alias [".encode_utf16());
            out.extend(k);
            out.extend("] == [".encode_utf16());
            out.extend(v);
            out.extend("]\n".encode_utf16());
        }
        out
    }

    /// The shortcut in a slot, when the slot is within the array.
    // D2-11966
    #[must_use]
    pub fn shortcut(&self, slot: u32) -> Option<&Shortcut> {
        self.shortcuts.get(slot as usize)
    }

    /// Put a shortcut in a slot within the array; whether the slot was there.
    // D2-11951
    pub fn set_shortcut(&mut self, slot: u32, s: &Shortcut) -> bool {
        match self.shortcuts.get_mut(slot as usize) {
            Some(at) => {
                at.clone_from(s);
                true
            }
            None => false,
        }
    }

    /// The text kinds a chat window shows (0 when it has no filter).
    // D2-11971
    #[must_use]
    pub fn filter(&self, window: u32) -> u32 {
        word_in(self.filters.as_ref(), &chat_defaults::FILTERS, window).unwrap_or(0)
    }

    /// A text kind's size (0, small, when it has none).
    // D2-11973
    #[must_use]
    pub fn chat_font_size(&self, kind: u32) -> u32 {
        word_in(self.font_sizes.as_ref(), &chat_defaults::FONT_SIZES, kind).unwrap_or(0)
    }

    /// A text kind's color's number (0 when it has none).
    // D2-11974
    #[must_use]
    pub fn chat_font_color(&self, kind: u32) -> u32 {
        word_in(self.font_colors.as_ref(), &chat_defaults::FONT_COLORS, kind).unwrap_or(0)
    }

    /// The channel a devoted window shows (the say channel when it has none).
    // D2-11972
    #[must_use]
    pub fn devoted_channel(&self, window: u32) -> u32 {
        word_in(
            self.devoted_windows.as_ref(),
            &chat_defaults::DEVOTED_WINDOWS,
            window,
        )
        .unwrap_or(chat_defaults::NO_DEVOTED_CHANNEL)
    }

    /// Whether a text kind pops the chat up (0 when it has no flag). A profile holds its popup
    /// table from its first packed form on (a new one holds the defaults; none is ever emptied).
    // D2-11975
    #[must_use]
    pub fn chat_popup_flag(&self, kind: u32) -> u32 {
        let held = (!self.popup_flags.is_empty()).then_some(&self.popup_flags);
        word_in(held, &chat_defaults::POPUP_FLAGS, kind).unwrap_or(0)
    }

    /// Whether any of a mask's options is on.
    // D2-11949
    #[must_use]
    pub fn test_bits(&self, mask: u32) -> bool {
        self.bits & mask != 0
    }

    /// Turn a mask's options on or off; the top bit is never turned off alone (answers whether
    /// anything was asked that the engine does).
    // D2-11950
    pub fn set_bits(&mut self, on: bool, mask: u32) -> bool {
        if on {
            self.bits |= mask;
            true
        } else if mask == VERSIONED {
            false
        } else {
            self.bits &= !mask;
            true
        }
    }
}

/// Whether a packed table of the windows' saved places holds any window.
fn ui_locations_held(bytes: &[u8]) -> bool {
    bytes
        .get(..4)
        .is_some_and(|h| u32::from_le_bytes([h[0], h[1], h[2], h[3]]) & 0x00FF_FFFF != 0)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (formats on hand-built values)
    use super::*;

    #[test]
    fn a_shortcut_packs_only_the_datum_its_kind_carries() {
        let s = Shortcut {
            kind: kind::SKILL,
            item: 7,
            data_enum: 0x4000_0123,
            data_id: 9,
            text: vec![65],
        };
        let b = s.pack();
        assert_eq!(b.len(), 8);
        let back = Shortcut::read(&mut In::new(&b)).unwrap();
        assert_eq!(back.data_enum, 0x4000_0123);
        assert_eq!((back.item, back.data_id, back.text.len()), (0, 0, 0));
    }

    #[test]
    fn a_new_profile_packs_its_mask_and_version_alone() {
        let o = GameplayOptions::default();
        assert_eq!(o.mask(), 0);
        let mut v = o.clone();
        v.bits = VERSIONED;
        let b = v.pack();
        assert_eq!(b.len(), 4 * 4);
        let back = GameplayOptions::read(&mut In::new(&b)).unwrap();
        assert_eq!(back.shortcut_height, 1);
        assert_eq!(back.bits, VERSIONED);
    }
}
