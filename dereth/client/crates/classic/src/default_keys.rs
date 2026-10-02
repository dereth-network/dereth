//! The default key map when the player has none of their own: the final client's shipped key map
//! carried onto the classic commands.
//!
//! The classic interface reads its default keys from a `Default.map` that came with the game's
//! installation. A player who does not have that file would otherwise start with nothing bound,
//! so this builds a stand-in: for every classic command that has the same action in the final
//! client, the unmodified keyboard keys the final default key map (and its default overlay) give
//! that action, written in the classic six-field format (map 0, virtual key, chord 0). It is not
//! the historical preset: keys whose final meaning has no classic command stay unbound, and each
//! key keeps only the first classic command that claims it.

use crate::keybindings::Catalogue;
use dereth_input::keymap::MasterInputMap;
use dereth_input::spec::DeviceType;
use dereth_primitives::{AssetSource, DataId};
use std::collections::BTreeMap;

/// The shipped game key map, then its default overlay.
const MAPS: [DataId; 2] = [DataId(0x1400_0000), DataId(0x1400_0002)];

/// The classic key code a keyboard control's offset stands for, on a US layout. The offset is the
/// key's scan code, with bit 7 standing for the extended (E0) prefix.
///
/// Classic key maps name keys by Windows virtual key, with two changes that keep the keypad apart
/// from the keys it shares a virtual key with: the keypad's Enter is `0x2B`, not the main Enter's
/// `0x0D`, and the keypad's navigation keys (the digits with Num Lock off) are the keypad digit
/// codes `0x60`–`0x69` and the keypad's Delete `0x6E`, not Home, End, the arrows, Insert and Delete.
#[must_use]
pub fn virtual_key(offset: u16) -> Option<u16> {
    const PLAIN: [u16; 0x59] = [
        0, 0x1B, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0xBD, 0xBB, 0x08,
        0x09, 0x51, 0x57, 0x45, 0x52, 0x54, 0x59, 0x55, 0x49, 0x4F, 0x50, 0xDB, 0xDD, 0x0D, 0xA2,
        0x41, 0x53, 0x44, 0x46, 0x47, 0x48, 0x4A, 0x4B, 0x4C, 0xBA, 0xDE, 0xC0, 0xA0, 0xDC, 0x5A,
        0x58, 0x43, 0x56, 0x42, 0x4E, 0x4D, 0xBC, 0xBE, 0xBF, 0xA1, 0x6A, 0xA4, 0x20, 0x14, 0x70,
        0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x90, 0x91, 0x67, 0x68, 0x69, 0x6D,
        0x64, 0x65, 0x66, 0x6B, 0x61, 0x62, 0x63, 0x60, 0x6E, 0, 0, 0, 0x7A, 0x7B,
    ];
    let vk = if offset & 0x80 == 0 {
        *PLAIN.get(usize::from(offset))?
    } else {
        match offset & 0x7F {
            0x1C => 0x2B,
            0x1D => 0xA3,
            0x35 => 0x6F,
            0x38 => 0xA5,
            0x47 => 0x24,
            0x48 => 0x26,
            0x49 => 0x21,
            0x4B => 0x25,
            0x4D => 0x27,
            0x4F => 0x23,
            0x50 => 0x28,
            0x51 => 0x22,
            0x52 => 0x2D,
            0x53 => 0x2E,
            _ => 0,
        }
    };
    (vk != 0).then_some(vk)
}

/// The stand-in default key map, as the text of a classic `.map` file, from the final client's
/// key maps in `assets`. `None` when the final key maps cannot be read.
#[must_use]
pub fn from_final_maps(assets: &dyn AssetSource, catalogue: &Catalogue) -> Option<String> {
    let maps = MAPS
        .iter()
        .map(|id| MasterInputMap::read(&assets.read(*id).ok()?).ok())
        .collect::<Option<Vec<_>>>()?;
    let keyboard = |map: &MasterInputMap, device: u8| {
        map.devices
            .get(usize::from(device))
            .is_some_and(|d| d.device_type == DeviceType::Keyboard)
    };
    let mut rows: BTreeMap<u16, &str> = BTreeMap::new();
    for command in &catalogue.actions {
        // Every action the command is, each stance's for the combat keys.
        let actions = crate::keystore::shared_actions(&command.name);
        if actions.is_empty() {
            continue;
        }
        for (map, section) in maps
            .iter()
            .flat_map(|m| m.sections.iter().map(move |s| (m, s)))
        {
            for (chord, bound) in section.bindings() {
                if !actions.contains(&bound.0)
                    || chord.meta_mode != 0
                    || !keyboard(map, chord.control.device_index())
                {
                    continue;
                }
                if let Some(vk) = virtual_key(chord.control.offset())
                    .filter(|vk| !crate::keybindings::reserved(*vk))
                {
                    rows.entry(vk).or_insert(&command.name);
                }
            }
        }
    }
    let mut text = format!("{}\n", rows.len());
    for (vk, name) in &rows {
        text.push_str(&format!("0 {vk} 0 {name} 0 0\n"));
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the stand-in key map's scan-code table).
    use super::*;

    #[test]
    fn scan_codes_become_classic_key_codes_with_the_keypad_kept_apart() {
        assert_eq!(virtual_key(0x11), Some(0x57));
        assert_eq!(virtual_key(0x1C), Some(0x0D));
        assert_eq!(virtual_key(0x80 | 0x1C), Some(0x2B));
        assert_eq!(virtual_key(0x39), Some(0x20));
        assert_eq!(virtual_key(0x58), Some(0x7B));
        assert_eq!(virtual_key(0x80 | 0x48), Some(0x26));
        assert_eq!(virtual_key(0x48), Some(0x68));
        assert_eq!(virtual_key(0x4C), Some(0x65));
        assert_eq!(virtual_key(0x53), Some(0x6E));
        assert_eq!(virtual_key(0x80 | 0x53), Some(0x2E));
        assert_eq!(virtual_key(0x7F), None);
    }
}
