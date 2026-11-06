//! Reading and changing a player's qualities as the server keeps them (their encoded form, as the
//! player's description carries them), keeping each table laid out as the client lays its own out.

use auberean_protocol::table::{Table, BUCKET_COUNTS};
use auberean_protocol::types::{Qualities, WeenieDesc};
use auberean_protocol::wire::{Field, Reader, Writer};

/// Read encoded qualities (empty ones for bytes that do not read).
#[must_use]
pub fn read(bytes: &[u8]) -> Qualities {
    Qualities::read(&mut Reader::new(bytes)).unwrap_or_default()
}

/// Encode qualities.
#[must_use]
pub fn write(q: &Qualities) -> Vec<u8> {
    let mut w = Writer::default();
    q.write(&mut w);
    w.finish()
}

/// Set a key of a table: in place if it is there, else as the newest entry of its bucket.
pub fn set<V>(t: &mut Table<V>, key: u32, value: V) {
    if let Some(e) = t.entries.iter_mut().find(|(k, _)| *k == key) {
        e.1 = value;
        return;
    }
    let buckets = BUCKET_COUNTS
        .get(usize::from(t.size_class))
        .copied()
        .unwrap_or(BUCKET_COUNTS[0]);
    let b = key % buckets;
    let at = t
        .entries
        .iter()
        .position(|(k, _)| k % buckets >= b)
        .unwrap_or(t.entries.len());
    t.entries.insert(at, (key, value));
}

/// A whole-number quality. Only the description before the numbers and the numbers themselves are
/// read: the server asks this of players far more often than it changes them.
#[must_use]
pub fn int(bytes: &[u8], key: u32) -> Option<i32> {
    let mut r = Reader::new(bytes);
    let header = r.u32().ok()?;
    r.u32().ok()?;
    if header & Qualities::WEENIE != 0 {
        WeenieDesc::read(&mut r).ok()?;
    }
    if header & Qualities::INTS == 0 {
        return None;
    }
    Table::<i32>::read(&mut r).ok()?.get(key).copied()
}

/// A long whole-number quality.
#[must_use]
pub fn long(bytes: &[u8], key: u32) -> Option<u64> {
    read(bytes).long_ints.get(key).copied()
}

/// Set a whole-number quality.
pub fn set_int(bytes: &mut Vec<u8>, key: u32, value: i32) {
    let mut q = read(bytes);
    q.header |= Qualities::INTS;
    set(&mut q.ints, key, value);
    *bytes = write(&q);
}

/// Set a long whole-number quality.
pub fn set_long(bytes: &mut Vec<u8>, key: u32, value: u64) {
    let mut q = read(bytes);
    q.header |= Qualities::LONG_INTS;
    set(&mut q.long_ints, key, value);
    *bytes = write(&q);
}

/// A true-or-false quality (`None` when the player has none).
#[must_use]
pub fn bool(bytes: &[u8], key: u32) -> Option<bool> {
    read(bytes).bools.get(key).map(|v| *v != 0)
}

/// Set a true-or-false quality.
pub fn set_bool(bytes: &mut Vec<u8>, key: u32, value: bool) {
    let mut q = read(bytes);
    q.header |= Qualities::BOOLS;
    set(&mut q.bools, key, i32::from(value));
    *bytes = write(&q);
}

/// A floating-point quality.
#[must_use]
pub fn float(bytes: &[u8], key: u32) -> Option<f32> {
    read(bytes).floats.get(key).copied()
}

/// Set a floating-point quality.
pub fn set_float(bytes: &mut Vec<u8>, key: u32, value: f32) {
    let mut q = read(bytes);
    q.header |= Qualities::FLOATS;
    set(&mut q.floats, key, value);
    *bytes = write(&q);
}

/// A timestamp quality (game time).
#[must_use]
pub fn timestamp(bytes: &[u8], key: u32) -> Option<f64> {
    read(bytes).timestamps.get(key).copied()
}

/// Set a timestamp quality (game time).
pub fn set_timestamp(bytes: &mut Vec<u8>, key: u32, value: f64) {
    let mut q = read(bytes);
    q.header |= Qualities::TIMESTAMPS;
    set(&mut q.timestamps, key, value);
    *bytes = write(&q);
}
