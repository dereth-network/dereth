//! The word before a treasure item's name: for each sort of item, each word with the combat delays
//! the January 2017 servers' items showed it with and how many of them did, by the item's level. Our
//! fit to those items: up to 9, 10 to 19, 20 to 29, 30 to 39, 40 to 49, 50 and up.

/// A word: its string, the combat delays it goes with, and how many items showed it by level band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Word {
    pub word: u32,
    pub delays: (i32, i32),
    pub by_level: [f32; 6],
}

const fn word(word: u32, least: i32, most: i32, by_level: [f32; 6]) -> Word {
    Word {
        word,
        delays: (least, most),
        by_level,
    }
}

/// The words of a weapon.
pub const WEAPON_WORDS: &[Word] = &[
    word(0xEF4F89B9, 7, 18, [0.0, 136.0, 149.0, 29.0, 0.0, 0.0]),
    word(0xEF2F89B9, 10, 18, [0.0, 0.0, 0.0, 0.0, 114.0, 148.0]),
    word(0xEF7F89B9, 7, 17, [120.0, 29.0, 11.0, 0.0, 0.0, 0.0]),
    word(0xEF5F89B9, 8, 17, [0.0, 0.0, 0.0, 98.0, 26.0, 0.0]),
    word(0xFB484475, 7, 17, [0.0, 4.0, 12.0, 22.0, 31.0, 50.0]),
    word(0xFB4E4475, 7, 18, [2.0, 8.0, 11.0, 7.0, 14.0, 19.0]),
    word(0xDFD0258D, 5, 7, [0.0, 2.0, 3.0, 12.0, 16.0, 25.0]),
    word(0xDFD0058D, 8, 9, [0.0, 0.0, 0.0, 3.0, 14.0, 35.0]),
    word(0xFB4A4475, 4, 18, [0.0, 0.0, 0.0, 5.0, 19.0, 21.0]),
    word(0xFB444475, 4, 19, [0.0, 0.0, 0.0, 0.0, 4.0, 28.0]),
    word(0xDFD0E58D, 0, 11, [0.0, 0.0, 0.0, 0.0, 3.0, 16.0]),
    word(0xDFD0458D, 4, 5, [0.0, 6.0, 2.0, 4.0, 0.0, 3.0]),
];

/// The words of a armor.
pub const ARMOR_WORDS: &[Word] = &[
    word(0x2202B8FD, 12, 24, [23.0, 48.0, 46.0, 36.0, 68.0, 101.0]),
    word(0x2202B8BD, 9, 21, [23.0, 31.0, 40.0, 35.0, 62.0, 62.0]),
    word(0x2202B93D, 5, 12, [12.0, 12.0, 8.0, 8.0, 21.0, 32.0]),
    word(0x2202B97D, 8, 16, [8.0, 9.0, 12.0, 13.0, 24.0, 25.0]),
    word(0x2202B83D, 17, 31, [10.0, 10.0, 12.0, 10.0, 7.0, 0.0]),
    word(0xF827B88B, 4, 19, [0.0, 9.0, 4.0, 6.0, 9.0, 16.0]),
    word(0xF827D88B, 16, 34, [0.0, 0.0, 0.0, 0.0, 0.0, 14.0]),
    word(0x2202B9FD, 5, 10, [1.0, 1.0, 0.0, 2.0, 3.0, 3.0]),
];

/// The words of a shield.
pub const SHIELD_WORDS: &[Word] = &[
    word(0xE73F06A9, 6, 11, [5.0, 20.0, 19.0, 0.0, 0.0, 0.0]),
    word(0xE73F1EA9, 7, 11, [0.0, 0.0, 2.0, 12.0, 10.0, 10.0]),
    word(0x943A075A, 6, 10, [0.0, 2.0, 1.0, 7.0, 8.0, 7.0]),
    word(0xE73F0AA9, 5, 9, [12.0, 10.0, 0.0, 0.0, 0.0, 0.0]),
    word(0x943A06DA, 6, 11, [0.0, 1.0, 3.0, 0.0, 6.0, 7.0]),
    word(0xA60C55E5, 7, 8, [0.0, 0.0, 0.0, 0.0, 6.0, 9.0]),
    word(0xC60C55E5, 5, 6, [0.0, 1.0, 1.0, 4.0, 2.0, 6.0]),
    word(0x943A045A, 8, 11, [0.0, 0.0, 0.0, 0.0, 5.0, 8.0]),
    word(0x943A07DA, 6, 11, [0.0, 0.0, 1.0, 1.0, 4.0, 7.0]),
    word(0xE73F1AA9, 8, 11, [0.0, 0.0, 0.0, 0.0, 3.0, 9.0]),
    word(0xE73F02A9, 7, 11, [0.0, 1.0, 5.0, 5.0, 0.0, 0.0]),
    word(0xD60C55E5, 6, 7, [0.0, 0.0, 0.0, 1.0, 3.0, 5.0]),
    word(0xF60C55E5, 5, 5, [0.0, 0.0, 4.0, 0.0, 0.0, 1.0]),
];
