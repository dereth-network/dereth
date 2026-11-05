//! What kind of object an instance id names, read from its top byte: a dynamic object the server
//! made (`0x80` set), a static object placed in a landblock (`0x40`, the block in the next two
//! bytes), a player (`0x20`), something of the interface (`0x11`), one of the camera's own
//! objects, or a temporary object the client made itself (`0x13`). An id is 64 bits: the high
//! word carries the kind, the low word the number.

/// The id that names nothing.
pub const INVALID: u64 = 0;

/// The kinds of instance id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdKind {
    Unknown,
    Dynamic,
    Static,
    Player,
    Interface,
    Camera,
    Temporary,
}

fn high(id: u64) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    let h = (id >> 32) as u32;
    h
}

fn low(id: u64) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    let l = id as u32;
    l
}

fn make(high: u32, low: u32) -> u64 {
    u64::from(high) << 32 | u64::from(low)
}

/// An interface object's id, numbered `n` (at most 0xFFF; else no id).
// D2-16945
#[must_use]
pub fn interface(n: u64) -> u64 {
    if n & 0xFFF == n {
        make(0x1100_0000, low(n))
    } else {
        INVALID
    }
}

/// A temporary object's id, numbered `n` (at most 0xFFF; else no id).
// D2-16946
#[must_use]
pub fn temporary(n: u64) -> u64 {
    if n & 0xFFF == n {
        make(0x1300_0000, low(n))
    } else {
        INVALID
    }
}

/// A static object's id: number `n` (its high word at most 0xFF; else no id) placed in the
/// landblock of cell or block id `block`.
// D2-16957
#[must_use]
pub fn static_object(n: u64, block: u32) -> u64 {
    let h = high(n);
    if h & 0xFF == h {
        make(((block >> 16) << 8) | h | 0x4000_0000, low(n))
    } else {
        INVALID
    }
}

/// What kind of object an id names. The camera test reads bits the camera's own ids do not set
/// (as the client does), so those ids are of no known kind.
// D2-16947
#[must_use]
pub fn kind(id: u64) -> IdKind {
    let top = high(id) >> 24;
    if top & 0x80 != 0 {
        IdKind::Dynamic
    } else if top & 0x40 != 0 {
        IdKind::Static
    } else if top & 0x20 != 0 {
        IdKind::Player
    } else if top == 0x11 {
        IdKind::Interface
    } else if (high(id) >> 18) & 0xFF == 0x12 {
        IdKind::Camera
    } else if top == 0x13 {
        IdKind::Temporary
    } else {
        IdKind::Unknown
    }
}

// D2-16948
#[must_use]
pub fn is_player(id: u64) -> bool {
    kind(id) == IdKind::Player
}

// D2-16951
#[must_use]
pub fn is_temporary(id: u64) -> bool {
    kind(id) == IdKind::Temporary
}

/// Whether an id names a static object; an id of no known kind counts as one.
// D2-16952
#[must_use]
pub fn is_static(id: u64) -> bool {
    matches!(kind(id), IdKind::Unknown | IdKind::Static)
}

// D2-16953
#[must_use]
pub fn is_dynamic(id: u64) -> bool {
    kind(id) == IdKind::Dynamic
}

/// The id of the landblock that holds static object `id`, when it is one: the static id of that
/// block with the highest number.
// D2-13218
#[must_use]
pub fn landblock_of_static(id: u64) -> Option<u64> {
    (id != 0 && is_static(id)).then(|| static_object(0xFF_FFFF_FFFF, block_of(id)))
}

/// The landblock a static object's id names, as a block id (`0xXXYYFFFF`).
// D2-16954
#[must_use]
pub fn block_of(id: u64) -> u32 {
    ((high(id) >> 8) << 16) | 0xFFFF
}

/// The id of the object the camera hangs from.
// D2-16955
#[must_use]
pub const fn camera_parent() -> u64 {
    0x1200_0000_0000_0FFF
}

/// The id of the object the camera looks at.
// D2-16956
#[must_use]
pub const fn camera_target() -> u64 {
    0x1200_0000_0000_0FFE
}

#[cfg(test)]
mod block_tests {
    //! Behaviour: none (a static object's landblock, held to hand-worked values)
    use super::*;

    #[test]
    fn a_static_objects_landblock_keeps_its_block() {
        let id = static_object(5, 0x1234_FFFF);
        let block = landblock_of_static(id).expect("a static object");
        assert_eq!(block_of(block), 0x1234_FFFF);
        assert_eq!(block, static_object(0xFF_FFFF_FFFF, 0x1234_FFFF));
        assert_eq!(landblock_of_static(0), None);
    }
}
