//! The scramble every string in the data files (and on the wire) is stored with.
//!
//! A stored string's bytes are its text with every whole 32-bit word XORed with a 16-word key (the
//! key word chosen by the word's index modulo 16; a trailing partial word is left alone), then
//! every byte's two nibbles swapped. Revealing undoes it: swap the nibbles back, then XOR. The
//! same transform covers narrow (8-bit) and wide (UTF-16) strings, applied to their bytes.

use alloc::string::String;
use alloc::vec::Vec;

const KEY: [u32; 16] = [
    0xE584_9238,
    0xEB2D_767D,
    0xFC0F_DD03,
    0xCB45_A6A6,
    0x7F21_E6BB,
    0xF5A6_CDC8,
    0x22D9_AD12,
    0xB2A3_D4DE,
    0x3B83_4DB7,
    0xC406_C91C,
    0x8392_305E,
    0x0A9F_0099,
    0x0943_B90C,
    0xA66D_953C,
    0x0D92_9E1F,
    0x0E2E_9AD4,
];

fn xor_words(b: &mut [u8]) {
    for (k, word) in b.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        *word = (u32::from_le_bytes(*word) ^ KEY[k & 15]).to_le_bytes();
    }
}

fn swap_nibbles(b: &mut [u8]) {
    for x in b {
        *x = x.rotate_left(4);
    }
}

/// The plain bytes of a stored string's bytes.
// D2-00376, D2-00375, D2-00243, D2-00242
#[must_use]
pub fn reveal(stored: &[u8]) -> Vec<u8> {
    let mut b = stored.to_vec();
    xor_words(&mut b);
    swap_nibbles(&mut b);
    b
}

/// The stored bytes of a string's plain bytes: the inverse of [`reveal`].
#[must_use]
pub fn conceal(plain: &[u8]) -> Vec<u8> {
    let mut b = plain.to_vec();
    swap_nibbles(&mut b);
    xor_words(&mut b);
    b
}

/// A stored narrow string's text. The game's narrow strings are ASCII; any other byte is kept as
/// the Latin-1 character of the same value.
#[must_use]
pub fn reveal_narrow(stored: &[u8]) -> String {
    reveal(stored).into_iter().map(char::from).collect()
}

/// A stored wide string's text, from its UTF-16 code units' bytes (unpaired surrogates become
/// U+FFFD).
#[must_use]
pub fn reveal_wide(stored: &[u8]) -> String {
    let b = reveal(stored);
    let units = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c));
    char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a reversible byte transform)
    use super::*;

    #[test]
    fn concealing_then_revealing_gives_the_text_back_at_every_length() {
        let text: Vec<u8> = (0u8..70).collect();
        for n in 0..text.len() {
            assert_eq!(reveal(&conceal(&text[..n])), &text[..n], "length {n}");
        }
    }

    #[test]
    fn a_trailing_partial_word_is_only_nibble_swapped() {
        let stored = [0x12, 0x34, 0x56, 0x78, 0xAB];
        let plain = reveal(&stored);
        assert_eq!(plain[4], 0xBA);
        assert_ne!(&plain[..4], &[0x21, 0x43, 0x65, 0x87]);
    }

    #[test]
    fn a_wide_string_reveals_to_its_text() {
        let units: Vec<u8> = "Dereth".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(reveal_wide(&conceal(&units)), "Dereth");
    }
}
