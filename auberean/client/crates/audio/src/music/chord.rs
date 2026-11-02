//! Chords, scales and music values: how a style's note, written as a place in the chord and the
//! scale, becomes a key.
//!
//! A music value holds an octave (its top four bits), a chord position (the next four: which note
//! of the chord, counted up from its root), a scale position (scale steps up from that chord note)
//! and an accidental (the low four bits, a signed count of semitones). A play mode says what the
//! value is relative to: under the fixed mode the value is the key itself; otherwise it counts from
//! the chord's root or the key's, through the chord's notes, the scale's, or both.

/// The play modes' bits.
pub mod mode {
    /// Relative to the key's root.
    pub const KEY_ROOT: u8 = 1;
    /// Relative to the chord's root.
    pub const CHORD_ROOT: u8 = 2;
    /// Counting the scale's steps.
    pub const SCALE_INTERVALS: u8 = 4;
    /// Counting the chord's notes.
    pub const CHORD_INTERVALS: u8 = 8;
    /// None: the part's mode applies.
    pub const NONE: u8 = 16;
    /// The value is the key.
    pub const FIXED: u8 = 0;
}

/// A chord as the music hears it: the notes over its root (bit n a note n semitones up, over two
/// octaves), the scale (bit n a scale note n semitones above the scale's root), and the roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChordKey {
    pub chord_pattern: u32,
    pub scale_pattern: u32,
    pub chord_root: u8,
    pub scale_root: u8,
}

/// C major over the C major scale, rooted an octave up: the chord a style plays over when
/// nothing gives it one.
pub const DEFAULT_CHORD: ChordKey = ChordKey {
    chord_pattern: 0x91,
    scale_pattern: 0b1010_1011_0101_1010_1011_0101,
    chord_root: 12,
    scale_root: 0,
};

/// The semitones above the pattern's root of its notes, in order, over the 24 bits it holds.
fn notes_of(pattern: u32) -> Vec<i32> {
    (0..24).filter(|b| pattern & (1 << b) != 0).collect()
}

/// The key `steps` scale steps above `from` (a key off the scale counts from the scale note below
/// it and keeps its distance above it).
fn up_scale(c: &ChordKey, from: i32, steps: i32) -> i32 {
    let scale = notes_of(c.scale_pattern & 0xFFF);
    if scale.is_empty() {
        return from + steps;
    }
    let root = i32::from(c.scale_root);
    let rel = from - root;
    let (octave, within) = (rel.div_euclid(12), rel.rem_euclid(12));
    let idx = scale.iter().rposition(|&s| s <= within).unwrap_or(0);
    let off = within - scale[idx];
    let len = i32::try_from(scale.len()).unwrap_or(1);
    let n = i32::try_from(idx).unwrap_or(0) + steps;
    let i = usize::try_from(n.rem_euclid(len)).unwrap_or(0);
    root + 12 * (octave + n.div_euclid(len)) + scale[i] + off
}

/// A key brought into 0..127 by whole octaves.
fn fold(mut key: i32) -> u8 {
    while key < 0 {
        key += 12;
    }
    while key > 127 {
        key -= 12;
    }
    u8::try_from(key).unwrap_or(0)
}

/// The key a music value plays under a play mode over a chord.
///
/// The octave counts from one below the root (octave 1 is the root itself). Counting the chord's
/// notes and the scale's, the chord position picks the chord's note (and, past the chord's
/// notes, the scale note two steps a position above the root) and the scale position moves that
/// many scale steps up; counting the scale's alone, each chord position is two scale steps. A key
/// out of range is brought in by octaves; under the fixed mode so is the value itself.
#[must_use]
pub fn music_to_midi(value: u16, play_mode: u8, chord: &ChordKey) -> Option<u8> {
    if play_mode == mode::FIXED {
        return Some(fold(i32::from(value)));
    }
    let octave = i32::from(value >> 12);
    let chord_pos = i32::from((value >> 8) & 0xF);
    let scale_pos = i32::from((value >> 4) & 0xF);
    let acc = i32::from(value & 0xF);
    let accidental = if acc > 7 { acc - 16 } else { acc };
    let root = if play_mode & mode::CHORD_ROOT != 0 {
        i32::from(chord.chord_root)
    } else if play_mode & mode::KEY_ROOT != 0 {
        i32::from(chord.scale_root)
    } else {
        0
    };
    let base = root + 12 * (octave - 1);
    let key = if play_mode & mode::CHORD_INTERVALS != 0 {
        let notes = notes_of(chord.chord_pattern);
        match usize::try_from(chord_pos).ok().and_then(|p| notes.get(p)) {
            Some(n) => up_scale(chord, base + n, scale_pos),
            None if play_mode & mode::SCALE_INTERVALS != 0 => {
                up_scale(chord, base, 2 * chord_pos + scale_pos)
            }
            // Past the chord's notes with no scale to count: no key.
            None => return None,
        }
    } else if play_mode & mode::SCALE_INTERVALS != 0 {
        up_scale(chord, base, 2 * chord_pos + scale_pos)
    } else {
        base + chord_pos + scale_pos
    };
    Some(fold(key + accidental))
}
