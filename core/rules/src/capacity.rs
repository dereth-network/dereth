//! A container's slot counts.
//!
//! The item and side-pack capacities travel as one byte each and are read signed: `-1` means
//! unlimited, and any other negative count (a byte of 128..=254) leaves no room at all.

/// The capacity that means "no limit".
pub const UNLIMITED: i32 = -1;

/// The slot count a capacity byte means.
#[must_use]
pub fn capacity(byte: u8) -> i32 {
    #[allow(clippy::cast_possible_wrap)] // the byte is signed
    i32::from(byte as i8)
}

/// The free slots with `used` of them occupied: `None` when the container is unlimited, negative
/// when it holds more than it allows.
#[must_use]
pub fn free_slots(byte: u8, used: i32) -> Option<i32> {
    let cap = capacity(byte);
    (cap != UNLIMITED).then(|| cap.saturating_sub(used))
}

/// Whether one more fits beside `used` occupied slots.
#[must_use]
pub fn has_room(byte: u8, used: i32) -> bool {
    free_slots(byte, used).is_none_or(|free| free > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_byte_is_signed_and_255_is_unlimited() {
        assert_eq!(capacity(102), 102);
        assert_eq!(capacity(127), 127);
        assert_eq!(capacity(128), -128);
        assert_eq!(capacity(255), UNLIMITED);
        assert_eq!(free_slots(102, 100), Some(2));
        assert_eq!(free_slots(255, 1000), None);
        assert!(has_room(255, 1000));
        assert!(has_room(102, 101));
        assert!(!has_room(102, 102));
        assert!(!has_room(200, 0), "128..=254 leaves no room");
        assert!(!has_room(0, 0));
    }
}
