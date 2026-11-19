//! The player's pets: the nine modes a pet takes, in the order the game turns them through, and
//! how a pet's name is shown.

/// The modes a pet takes ([`crate::PetAction::Mode`], [`crate::view::Pet::mode`]).
pub mod mode {
    pub const AGGRESSIVE: u32 = 0x20;
    pub const OPPORTUNISTIC: u32 = 0x1000;
    pub const COURAGEOUS: u32 = 0x2000;
    pub const PROTECTIVE: u32 = 0x4000_0001;
    pub const ASSISTING: u32 = 0x4000_0003;
    pub const DEFENSIVE: u32 = 0x4000_0004;
    pub const SUPPORTING: u32 = 0x4000_0002;
    pub const COOPERATIVE: u32 = 0x4000_0006;
    pub const IDLE: u32 = 0x4000_0005;
}

/// The modes in the order the game turns them through.
pub const MODES: [u32; 9] = [
    mode::AGGRESSIVE,
    mode::OPPORTUNISTIC,
    mode::COURAGEOUS,
    mode::PROTECTIVE,
    mode::ASSISTING,
    mode::DEFENSIVE,
    mode::SUPPORTING,
    mode::COOPERATIVE,
    mode::IDLE,
];

/// The mode after one (the first after the last, or after one not among them).
// D2-02368
#[must_use]
pub fn next_mode(mode: u32) -> u32 {
    match MODES.iter().position(|m| *m == mode) {
        Some(i) => MODES[(i + 1) % MODES.len()],
        None => MODES[0],
    }
}

/// The mode before one (the last before the first, or before one not among them).
// D2-02369
#[must_use]
pub fn previous_mode(mode: u32) -> u32 {
    match MODES.iter().position(|m| *m == mode) {
        Some(0) | None => MODES[MODES.len() - 1],
        Some(i) => MODES[i - 1],
    }
}

/// A pet's name as it is shown: without the part in parentheses (its owner's name), from an
/// opening one after the first letter to the closing one after it, every place that part occurs.
#[must_use]
pub fn shown_name(name: &str) -> String {
    let units: Vec<u16> = name.encode_utf16().collect();
    let open = units.iter().position(|c| *c == u16::from(b'('));
    let close = units.iter().position(|c| *c == u16::from(b')'));
    match (open, close) {
        (Some(o), Some(c)) if o > 0 && c > o => {
            let part = String::from_utf16_lossy(&units[o..=c]);
            name.replace(&part, "")
        }
        _ => name.to_owned(),
    }
}
