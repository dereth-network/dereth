//! Gamepads: the device-enumeration callback's filter and the device ordering saved keymaps depend
//! on.
//!
//! This covers the client's device start-up, the suitable-device enumeration and its callback,
//! and the three steps that add a device, its pointer and its entry in the input map.
//!
//! **DirectInput is only for gamepads.** The keyboard and the mouse are acquired and then
//! their buffers are flushed and discarded every frame; keyboard and mouse input arrives through
//! the window-message path. A rebuild needs `gilrs`-style gamepad support and nothing else from
//! DirectInput — but it *does* need this filter and this ordering, because a saved keymap
//! serialises the device index.

use crate::keymap::{MasterInputMap, GUID_SYS_KEYBOARD, GUID_SYS_MOUSE, GUID_VIRTUAL};
use crate::spec::DeviceType;

/// `DI8DEVTYPE_MOUSE`.
pub const DI8DEVTYPE_MOUSE: u8 = 0x12;
/// `DI8DEVTYPE_KEYBOARD`.
pub const DI8DEVTYPE_KEYBOARD: u8 = 0x13;
/// The type maps to the virtual device type ([`DeviceType::Virtual`]).
pub const DI8DEVTYPE_VIRTUAL: u8 = 0x1C;

/// One row of a DirectInput enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnumeratedDevice {
    /// `dwDevType`; only the low byte is tested.
    pub dev_type: u32,
    pub guid_instance: [u8; 16],
}

/// A device is added only when `(dwDevType & 0xFF)` is
/// **outside** `[0x12, 0x13]`, i.e. `DI8DEVTYPE_MOUSE` and `DI8DEVTYPE_KEYBOARD` are deliberately
/// skipped during enumeration. Those two are added explicitly afterwards.
#[must_use]
pub fn is_suitable(dev_type: u32) -> bool {
    // The low byte is the device-type class; the mask makes the narrowing exact.
    #[allow(clippy::cast_possible_truncation)]
    let class = (dev_type & 0xFF) as u8;
    !(DI8DEVTYPE_MOUSE..=DI8DEVTYPE_KEYBOARD).contains(&class)
}

/// The DirectInput type becomes a `DeviceType`.
#[must_use]
pub fn device_type_of(dev_type: u32) -> DeviceType {
    #[allow(clippy::cast_possible_truncation)]
    let class = (dev_type & 0xFF) as u8;
    match class {
        DI8DEVTYPE_MOUSE => DeviceType::Mouse,
        DI8DEVTYPE_KEYBOARD => DeviceType::Keyboard,
        DI8DEVTYPE_VIRTUAL => DeviceType::Virtual,
        _ => DeviceType::Joystick,
    }
}

/// The input-device records the client appends, plus the three remembered
/// indices.
#[derive(Debug, Default)]
pub struct DeviceTable {
    pub devices: Vec<(DeviceType, [u8; 16])>,
    /// The keyboard's device index.
    pub keyboard: Option<u8>,
    /// The mouse's device index.
    pub mouse: Option<u8>,
    /// The virtual device's index.
    pub virtual_device: Option<u8>,
}

/// Device start-up, step 4, reproduced.
///
/// The keyboard, the mouse and the virtual device are added explicitly; the enumeration supplies
/// only what passes [`is_suitable`], in enumeration order. Each add reaches the device-table
/// insertion step, whose return value becomes the index stored in
/// bits 0–7 of every `ControlCode` — which is why the order has to be reproduced.
///
/// The shipped keymaps already declare keyboard at 0 and mouse at 1, so a client that loads them
/// first keeps those indices and the virtual device lands at 2.
pub fn enumerate(map: &mut MasterInputMap, enumerated: &[EnumeratedDevice]) -> DeviceTable {
    let mut t = DeviceTable {
        keyboard: Some(map.add_device_entry(DeviceType::Keyboard, GUID_SYS_KEYBOARD)),
        mouse: Some(map.add_device_entry(DeviceType::Mouse, GUID_SYS_MOUSE)),
        ..DeviceTable::default()
    };
    t.devices.push((DeviceType::Keyboard, GUID_SYS_KEYBOARD));
    t.devices.push((DeviceType::Mouse, GUID_SYS_MOUSE));
    for d in enumerated {
        if !is_suitable(d.dev_type) {
            continue;
        }
        let dt = device_type_of(d.dev_type);
        map.add_device_entry(dt, d.guid_instance);
        t.devices.push((dt, d.guid_instance));
    }
    t.virtual_device = Some(map.add_device_entry(DeviceType::Virtual, GUID_VIRTUAL));
    t.devices.push((DeviceType::Virtual, GUID_VIRTUAL));
    t
}

/// `DIV_MOUSELOOK` — the virtual device's only control, offset 1. The mouse-mode switch fires it as a
/// button press/release, which is how a keymap can bind an action to "mouse-look is on".
pub const DIV_MOUSELOOK_OFFSET: u16 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    /// the callback skips exactly `DI8DEVTYPE_MOUSE` (0x12)
    /// and `DI8DEVTYPE_KEYBOARD` (0x13).
    #[test]
    fn the_filter_skips_only_the_mouse_and_the_keyboard() {
        assert!(!is_suitable(0x12));
        assert!(!is_suitable(0x13));
        assert!(is_suitable(0x14)); // DI8DEVTYPE_JOYSTICK
        assert!(is_suitable(0x15)); // DI8DEVTYPE_GAMEPAD
        assert!(is_suitable(0x11)); // DI8DEVTYPE_DEVICE
                                    // Only the low byte is tested; the subtype in the high bytes is ignored.
        assert!(!is_suitable(0x0000_1312));
        assert!(is_suitable(0x0000_1215));
    }

    /// with the shipped
    /// keymap already loaded, keyboard is 0 and mouse is 1, so a gamepad lands at 2 and the
    /// virtual device after it.
    #[test]
    fn the_shipped_indices_survive_enumeration() {
        let mut map = MasterInputMap::default();
        map.add_device_entry(DeviceType::Keyboard, GUID_SYS_KEYBOARD);
        map.add_device_entry(DeviceType::Mouse, GUID_SYS_MOUSE);

        let pad = EnumeratedDevice {
            dev_type: 0x15,
            guid_instance: [7; 16],
        };
        let skipped = EnumeratedDevice {
            dev_type: 0x13,
            guid_instance: [9; 16],
        };
        let t = enumerate(&mut map, &[skipped, pad]);

        assert_eq!(t.keyboard, Some(0));
        assert_eq!(t.mouse, Some(1));
        assert_eq!(
            map.devices.len(),
            4,
            "the skipped keyboard must not be added twice"
        );
        assert_eq!(map.devices[2].guid, [7; 16]);
        assert_eq!(t.virtual_device, Some(3));
        assert_eq!(map.devices[3].device_type, DeviceType::Virtual);
    }

    /// Oracle: the client's own device-type mapping when it adds a device to the input map.
    #[test]
    fn di_types_map_to_device_types() {
        assert_eq!(device_type_of(0x12), DeviceType::Mouse);
        assert_eq!(device_type_of(0x13), DeviceType::Keyboard);
        assert_eq!(device_type_of(0x1C), DeviceType::Virtual);
        assert_eq!(device_type_of(0x15), DeviceType::Joystick);
    }
}
