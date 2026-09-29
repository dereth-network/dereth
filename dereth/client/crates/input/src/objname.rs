//! `IDirectInputDevice8::GetObjectInfo` — the **display** name of a control, as opposed to the
//! `DIK_*`/`DIMOFS_*` constant name [`crate::spec::ControlNames`] carries.
//!
//! # Why both exist
//!
//! Naming a control is a two-step lookup, and the two
//! steps answer different questions:
//!
//! ```text
//!   name = the control's DIK constant name        ; "DIK_W"
//!   look up the string hash of name in table enum 4 or 5
//!   if that row exists, return it                 ; an override exists
//!   take the control's device
//!   if the device has no DirectInput device, return the Mouse-Look arm
//!   if GetObjectInfo fails, return empty
//!   return its tszName                            ; the device's own object name
//! ```
//!
//! So the `DIK_*` constant is **the key into a string table**, never the caption. The table is
//! the *override* list, and it is tiny: measured against the shipped `client_local_English.dat`,
//! string table enum 4 (`0x2300000A`) holds exactly two rows — `DIK_LCONTROL` → `"Left Ctrl"` and
//! `DIK_LMENU` → `"Left Alt"` — and enum 5 (`0x2300000B`), the *meta* key table, holds
//! `DIK_LWIN` and `DIK_RWIN` → `"Windows"`. Everything else is the device's own object name.
//!
//! # The declared deviation
//!
//! **This table is transcribed, not queried.** Retail asks the live DirectInput device, which
//! fills `DIDEVICEOBJECTINSTANCE::tszName` for a keyboard from `GetKeyNameText` against the
//! *currently installed keyboard layout* — so on a German layout retail shows `"Z"` where a US
//! layout shows `"Y"`. This build has no DirectInput device to ask (`dereth-input` is
//! `#![forbid(unsafe_code)]` and platform-free by construction, and the whole crate is driven from
//! `WM_KEYDOWN` scan codes rather than from a DirectInput device state block), so the table below
//! is the **US-English** answer, which is the layout the English client ships for.
//!
//! A caller that wants the layout-correct answer should install a provider from the host —
//! `dereth-client` may call `GetKeyNameTextW(scan << 16 | ext << 24, …)` — and fall back here. The
//! seam is [`keyboard_object_name`]: one function, one call site.
//! [transcribed, US-English layout]

use crate::DeviceType;

/// `DIDEVICEOBJECTINSTANCE::tszName` for a keyboard, by DIK code, on the US-English layout.
///
/// A DIK code is a PS/2 set-1 scan code with bit 7 set for the `E0`-prefixed keys, which is
/// exactly the `(scan, extended)` pair `GetKeyNameText` takes; the names below are what it
/// answers. Offsets with no row here have no DirectInput name either and show as nothing, which
/// is retail's own behaviour: an empty key name leaves the binding cell blank.
const KEYBOARD_OBJECT_NAMES: &[(u16, &str)] = &[
    (0x01, "Esc"),
    (0x02, "1"),
    (0x03, "2"),
    (0x04, "3"),
    (0x05, "4"),
    (0x06, "5"),
    (0x07, "6"),
    (0x08, "7"),
    (0x09, "8"),
    (0x0A, "9"),
    (0x0B, "0"),
    (0x0C, "-"),
    (0x0D, "="),
    (0x0E, "Backspace"),
    (0x0F, "Tab"),
    (0x10, "Q"),
    (0x11, "W"),
    (0x12, "E"),
    (0x13, "R"),
    (0x14, "T"),
    (0x15, "Y"),
    (0x16, "U"),
    (0x17, "I"),
    (0x18, "O"),
    (0x19, "P"),
    (0x1A, "["),
    (0x1B, "]"),
    (0x1C, "Enter"),
    (0x1D, "Ctrl"),
    (0x1E, "A"),
    (0x1F, "S"),
    (0x20, "D"),
    (0x21, "F"),
    (0x22, "G"),
    (0x23, "H"),
    (0x24, "J"),
    (0x25, "K"),
    (0x26, "L"),
    (0x27, ";"),
    (0x28, "'"),
    (0x29, "`"),
    (0x2A, "Shift"),
    (0x2B, "\\"),
    (0x2C, "Z"),
    (0x2D, "X"),
    (0x2E, "C"),
    (0x2F, "V"),
    (0x30, "B"),
    (0x31, "N"),
    (0x32, "M"),
    (0x33, ","),
    (0x34, "."),
    (0x35, "/"),
    (0x36, "Right Shift"),
    (0x37, "Num *"),
    (0x38, "Alt"),
    (0x39, "Space"),
    (0x3A, "Caps Lock"),
    (0x3B, "F1"),
    (0x3C, "F2"),
    (0x3D, "F3"),
    (0x3E, "F4"),
    (0x3F, "F5"),
    (0x40, "F6"),
    (0x41, "F7"),
    (0x42, "F8"),
    (0x43, "F9"),
    (0x44, "F10"),
    (0x45, "Num Lock"),
    (0x46, "Scroll Lock"),
    (0x47, "Num 7"),
    (0x48, "Num 8"),
    (0x49, "Num 9"),
    (0x4A, "Num -"),
    (0x4B, "Num 4"),
    (0x4C, "Num 5"),
    (0x4D, "Num 6"),
    (0x4E, "Num +"),
    (0x4F, "Num 1"),
    (0x50, "Num 2"),
    (0x51, "Num 3"),
    (0x52, "Num 0"),
    (0x53, "Num Del"),
    (0x56, "\\"),
    (0x57, "F11"),
    (0x58, "F12"),
    (0x64, "F13"),
    (0x65, "F14"),
    (0x66, "F15"),
    (0x73, "ABNT C1"),
    (0x7D, "\u{a5}"),
    (0x7E, "ABNT C2"),
    (0x8D, "Num ="),
    (0x90, "Previous Track"),
    (0x91, "@"),
    (0x92, ":"),
    (0x93, "_"),
    (0x95, "Stop"),
    (0x96, "AX"),
    (0x97, "Unlabeled"),
    (0x99, "Next Track"),
    (0x9C, "Num Enter"),
    (0x9D, "Right Ctrl"),
    (0xA0, "Mute"),
    (0xA1, "Calculator"),
    (0xA2, "Play/Pause"),
    (0xA4, "Media Stop"),
    (0xAE, "Volume Down"),
    (0xB0, "Volume Up"),
    (0xB2, "Web Home"),
    (0xB3, "Num ,"),
    (0xB5, "Num /"),
    (0xB7, "Prnt Scrn"),
    (0xB8, "Right Alt"),
    (0xC5, "Pause"),
    (0xC7, "Home"),
    (0xC8, "Up"),
    (0xC9, "Page Up"),
    (0xCB, "Left"),
    (0xCD, "Right"),
    (0xCF, "End"),
    (0xD0, "Down"),
    (0xD1, "Page Down"),
    (0xD2, "Insert"),
    (0xD3, "Delete"),
    (0xDB, "Left Windows"),
    (0xDC, "Right Windows"),
    (0xDD, "Application"),
    (0xE5, "Web Search"),
    (0xE6, "Web Favorites"),
    (0xE7, "Web Refresh"),
    (0xE8, "Web Stop"),
    (0xE9, "Web Forward"),
    (0xEA, "Web Back"),
    (0xEB, "My Computer"),
    (0xEC, "Mail"),
    (0xED, "Media Select"),
];

/// The DirectInput mouse device's own object names, by `DIMOFS_*` offset.
///
/// Retail refuses to bind buttons 0 and 1 at all (its own enumeration drops device type 2 with
/// object index 0 or 4), so only the wheel, the axes and
/// buttons 2 and up can reach a cell.
const MOUSE_OBJECT_NAMES: &[(u16, &str)] = &[
    (0x00, "X-axis"),
    (0x04, "Y-axis"),
    (0x08, "Wheel"),
    (0x0C, "Button 0"),
    (0x0D, "Button 1"),
    (0x0E, "Button 2"),
    (0x0F, "Button 3"),
    (0x10, "Button 4"),
    (0x11, "Button 5"),
    (0x12, "Button 6"),
    (0x13, "Button 7"),
];

/// The literal the client writes for the **virtual** device -- the arm taken when a device
/// record carries no `IDirectInputDevice8*`.
pub const MOUSE_LOOK_NAME: &str = "Mouse-Look";

/// `IDirectInputDevice8::GetObjectInfo(&doi, offset, DIPH_BYOFFSET).tszName`, transcribed.
///
/// `None` is both "this device has no such object" and "`GetObjectInfo` failed", which retail
/// treats identically: it leaves the result string empty (` jl `).
#[must_use]
pub fn keyboard_object_name(dik: u16) -> Option<&'static str> {
    KEYBOARD_OBJECT_NAMES
        .iter()
        .find(|(k, _)| *k == dik)
        .map(|(_, n)| *n)
}

/// The same, dispatched on the device class the control belongs to.
#[must_use]
pub fn device_object_name(device: DeviceType, offset: u16) -> Option<&'static str> {
    match device {
        DeviceType::Keyboard => keyboard_object_name(offset),
        DeviceType::Mouse => MOUSE_OBJECT_NAMES
            .iter()
            .find(|(k, _)| *k == offset)
            .map(|(_, n)| *n),
        // A joystick's object names come from the attached stick, which no fixture describes, and
        // the virtual device has exactly one control.
        DeviceType::Joystick => None,
        DeviceType::Virtual => (offset == 1).then_some(MOUSE_LOOK_NAME),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two claims this unit's page rests on, stated against the constant rather than against
    /// the page: `W` is the caption of `DIK_W`, and the `DIK_*` spelling never reaches a cell.
    #[test]
    fn the_movement_keys_have_their_keycap_names() {
        assert_eq!(keyboard_object_name(0x11), Some("W"));
        assert_eq!(keyboard_object_name(0x1E), Some("A"));
        assert_eq!(keyboard_object_name(0x1F), Some("S"));
        assert_eq!(keyboard_object_name(0x20), Some("D"));
        assert_eq!(keyboard_object_name(0x41), Some("F7"));
        assert_eq!(keyboard_object_name(0xC8), Some("Up"));
        // Nothing in the table is a DIK_* constant name.
        assert!(KEYBOARD_OBJECT_NAMES
            .iter()
            .all(|(_, n)| !n.starts_with("DIK_")));
        // Every DIK code the table names is one `ControlNames` knows, and `0x00` is not a
        // control at all -- an instrument that answered for everything would prove nothing.
        assert_eq!(keyboard_object_name(0x00), None);
    }

    /// `DIK_LCONTROL` and `DIK_LMENU` are the two the shipped string table **overrides**, so the
    /// object name here is deliberately the unqualified one: if this said "Left Ctrl" the
    /// override could be removed without a test noticing.
    #[test]
    fn the_two_overridden_keys_keep_their_unqualified_object_names() {
        assert_eq!(keyboard_object_name(0x1D), Some("Ctrl"));
        assert_eq!(keyboard_object_name(0x38), Some("Alt"));
    }

    #[test]
    fn the_mouse_and_virtual_devices_answer_too() {
        assert_eq!(device_object_name(DeviceType::Mouse, 0x08), Some("Wheel"));
        assert_eq!(
            device_object_name(DeviceType::Mouse, 0x0E),
            Some("Button 2")
        );
        assert_eq!(
            device_object_name(DeviceType::Virtual, 1),
            Some(MOUSE_LOOK_NAME)
        );
        assert_eq!(device_object_name(DeviceType::Virtual, 2), None);
    }
}
