//! Portable control captions, with interface text and named-string composition supplied by a provider.

use crate::{ControlChord, ControlCode, DeviceType, MasterInputMap, SubControlIndex};

/// The text resources and caption policy of an interface.
pub trait LabelProvider {
    fn resolve_token(&self, table: u32, token: &str) -> Option<String>;
    fn format_subcontrol(&self, key: &str, subcontrol: &str) -> Option<String>;
    fn delimiter(&self) -> &str;

    /// Modifier masks in display order. The default visits every bit from low to high.
    fn modifier_order(&self) -> &[u32] {
        &MODIFIER_BITS
    }

    /// An interface-specific base caption, selected by control identity rather than translated text.
    fn override_name(
        &self,
        _device: DeviceType,
        _control: ControlCode,
        _meta: bool,
    ) -> Option<String> {
        None
    }
}

const MODIFIER_BITS: [u32; 32] = {
    let mut bits = [0; 32];
    let mut i = 0;
    while i < 32 {
        bits[i] = 1 << i;
        i += 1;
    }
    bits
};

/// Name the key first, then prepend its named modifiers in the provider's display order.
#[must_use]
pub fn binding_label(
    map: &MasterInputMap,
    control: ControlChord,
    provider: &impl LabelProvider,
) -> String {
    binding_label_on(COMMAND_IS_CONTROL, map, control, provider)
}

/// [`binding_label`] with the platform said outright: `command_is_control` on a Mac.
fn binding_label_on(
    command_is_control: bool,
    map: &MasterInputMap,
    control: ControlChord,
    provider: &impl LabelProvider,
) -> String {
    let device = map
        .device_type_of(control.control)
        .unwrap_or(DeviceType::Keyboard);
    let key = control_name_on(command_is_control, device, control.control, false, provider);
    if key.is_empty() {
        return key;
    }
    // A chord the system keeps for Command is only ever pressed with the Control key itself.
    let held_as_command = command_is_control && !kept_with_command(device, control.control);
    let mut parts = Vec::new();
    for &bit in provider.modifier_order() {
        if control.meta_mode & bit != 0 {
            if let Some(cs) = map.key_from_meta_mode(bit) {
                let device = map.device_type_of(cs).unwrap_or(DeviceType::Keyboard);
                let name = control_name_on(held_as_command, device, cs, true, provider);
                if !name.is_empty() {
                    parts.push(name);
                }
            }
        }
    }
    parts.push(key);
    parts.join(provider.delimiter())
}

/// Resolve a control caption and optionally decorate it using the named subcontrol template.
#[must_use]
pub fn control_name(
    device: DeviceType,
    control: ControlCode,
    meta: bool,
    provider: &impl LabelProvider,
) -> String {
    control_name_on(COMMAND_IS_CONTROL, device, control, meta, provider)
}

/// On a Mac the host hands either Command key over as Control, so that is what a binding of
/// Control is pressed with.
const COMMAND_IS_CONTROL: bool = cfg!(target_os = "macos");

/// The keys a Mac's system keeps for itself when held with Command, by keyboard offset: Tab, Q, H
/// and Space. The client never sees those chords, so a binding of one is pressed with Control.
pub const KEPT_WITH_COMMAND: [u16; 4] = [0x0F, 0x10, 0x23, 0x39];

/// Whether a chord of `control` held with Command would go to a Mac's system, not the client.
fn kept_with_command(device: DeviceType, control: ControlCode) -> bool {
    device == DeviceType::Keyboard && KEPT_WITH_COMMAND.contains(&control.offset())
}

/// What a Control key is called where Command stands in for it: plain when it is held with another
/// key, as "Control" is, otherwise by its side, as "Left Ctrl" is. `None` for any other control.
fn command_name(device: DeviceType, control: ControlCode, meta: bool) -> Option<&'static str> {
    if device != DeviceType::Keyboard {
        return None;
    }
    match (control.offset(), meta) {
        (0x1D | 0x9D, true) => Some("Cmd"),
        (0x1D, false) => Some("Left Cmd"),
        (0x9D, false) => Some("Right Cmd"),
        _ => None,
    }
}

/// [`control_name`] with the platform said outright: `command_is_control` on a Mac.
fn control_name_on(
    command_is_control: bool,
    device: DeviceType,
    control: ControlCode,
    meta: bool,
    provider: &impl LabelProvider,
) -> String {
    if command_is_control {
        if let Some(name) = command_name(device, control, meta) {
            return name.to_owned();
        }
    }
    let name = provider
        .override_name(device, control, meta)
        .or_else(|| {
            crate::spec::ControlNames::name_by_semantic(device, control.offset())
                .and_then(|token| provider.resolve_token(if meta { 5 } else { 4 }, token))
                .or_else(|| {
                    crate::objname::device_object_name(device, control.offset()).map(str::to_owned)
                })
        })
        .unwrap_or_default();
    if name.is_empty() {
        return name;
    }
    let Some(token) = sub_control_token(control.sub_control()) else {
        return name;
    };
    let Some(subcontrol) = provider.resolve_token(3, token) else {
        return name;
    };
    provider
        .format_subcontrol(&name, &subcontrol)
        .filter(|text| !text.is_empty())
        .unwrap_or(name)
}

/// The six directional subcontrol caption tokens in numeric order.
pub const SUB_CONTROL_TOKENS: [&str; 6] = [
    "ID_sci_PositiveAxis",
    "ID_sci_NegativeAxis",
    "ID_sci_POVUp",
    "ID_sci_POVRight",
    "ID_sci_POVDown",
    "ID_sci_POVLeft",
];

/// The caption token for each directional subcontrol; plain and unknown controls have none.
#[must_use]
pub const fn sub_control_token(sub: SubControlIndex) -> Option<&'static str> {
    Some(match sub {
        SubControlIndex::PositiveAxis => SUB_CONTROL_TOKENS[0],
        SubControlIndex::NegativeAxis => SUB_CONTROL_TOKENS[1],
        SubControlIndex::PovUp => SUB_CONTROL_TOKENS[2],
        SubControlIndex::PovRight => SUB_CONTROL_TOKENS[3],
        SubControlIndex::PovDown => SUB_CONTROL_TOKENS[4],
        SubControlIndex::PovLeft => SUB_CONTROL_TOKENS[5],
        SubControlIndex::None | SubControlIndex::Other(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{activation, keymap::DeviceKeyMapEntry};

    /// The names off a Mac, whatever this test runs on; the Mac's own have their test below.
    fn binding_label(
        map: &MasterInputMap,
        control: ControlChord,
        provider: &impl LabelProvider,
    ) -> String {
        binding_label_on(false, map, control, provider)
    }

    fn control_name(
        device: DeviceType,
        control: ControlCode,
        meta: bool,
        provider: &impl LabelProvider,
    ) -> String {
        control_name_on(false, device, control, meta, provider)
    }

    struct Text {
        classic: bool,
        composition: Option<&'static str>,
    }
    impl LabelProvider for Text {
        fn resolve_token(&self, table: u32, token: &str) -> Option<String> {
            match (table, token) {
                (5, "DIK_LCONTROL") => Some("Control".into()),
                (5, "DIK_RMENU") => Some("Right Alt".into()),
                (4, "DIK_LCONTROL") => Some("Left Ctrl".into()),
                (3, "ID_sci_PositiveAxis") => Some("positive".into()),
                _ => None,
            }
        }
        fn format_subcontrol(&self, key: &str, subcontrol: &str) -> Option<String> {
            self.composition.map(|prefix| {
                if prefix.is_empty() {
                    String::new()
                } else {
                    format!("{prefix}[{subcontrol}][{key}]")
                }
            })
        }
        fn delimiter(&self) -> &str {
            " / "
        }
        fn modifier_order(&self) -> &[u32] {
            if self.classic {
                &[0x8000_0000, 1]
            } else {
                &MODIFIER_BITS
            }
        }
        fn override_name(
            &self,
            device: DeviceType,
            control: ControlCode,
            _: bool,
        ) -> Option<String> {
            (self.classic && device == DeviceType::Keyboard && control.offset() == 0x39)
                .then(|| "Spacebar".into())
        }
    }
    fn key(offset: u16) -> ControlCode {
        ControlCode::new(0, SubControlIndex::None, offset)
    }
    fn map() -> MasterInputMap {
        MasterInputMap {
            devices: vec![DeviceKeyMapEntry {
                device_type: DeviceType::Keyboard,
                guid: [0; 16],
            }],
            // Storage order deliberately differs from modifier-bit order.
            meta_keys: vec![(key(0xb8), 0x8000_0000), (key(0x1d), 1)],
            ..MasterInputMap::default()
        }
    }

    /// Behaviour: options.key-bindings.a-cell-shows-the-key-the-way-the-desktop-names-it
    #[test]
    fn base_names_and_modifiers_keep_their_tables_and_bit_order() {
        let text = Text {
            classic: false,
            composition: None,
        };
        let chord = ControlChord::new(key(0x11), 0x8000_0001, activation::DOWN);
        assert_eq!(
            binding_label(&map(), chord, &text),
            "Control / Right Alt / W"
        );
        assert_eq!(
            control_name(DeviceType::Keyboard, key(0x1d), false, &text),
            "Left Ctrl"
        );
        let unknown = ControlChord::new(key(0xffff), chord.meta_mode, activation::DOWN);
        assert_eq!(binding_label(&map(), unknown, &text), "");
    }

    /// Behaviour: options.key-bindings.a-cell-shows-the-key-the-way-the-desktop-names-it
    #[test]
    fn provider_caption_policy_does_not_rewrite_translated_names() {
        let chord = ControlChord::new(key(0x39), 1, activation::DOWN);
        assert_eq!(
            binding_label(
                &map(),
                chord,
                &Text {
                    classic: false,
                    composition: None
                }
            ),
            "Control / Space"
        );
        assert_eq!(
            binding_label(
                &map(),
                chord,
                &Text {
                    classic: true,
                    composition: None
                }
            ),
            "Control / Spacebar"
        );
        assert_eq!(
            binding_label(
                &map(),
                ControlChord::new(key(0x39), 0x8000_0001, activation::DOWN),
                &Text {
                    classic: true,
                    composition: None
                }
            ),
            "Right Alt / Control / Spacebar"
        );
        let text = Text {
            classic: false,
            composition: None,
        };
        assert_ne!(
            control_name(DeviceType::Keyboard, key(0x1c), false, &text),
            control_name(DeviceType::Keyboard, key(0x9c), false, &text)
        );
        assert_ne!(
            control_name(DeviceType::Keyboard, key(0x47), false, &text),
            control_name(DeviceType::Keyboard, key(0xc7), false, &text)
        );
        assert!(!control_name(DeviceType::Mouse, key(0), false, &text).is_empty());
        assert!(!control_name(DeviceType::Virtual, key(1), false, &text).is_empty());
    }

    /// Behaviour: options.key-bindings.a-cell-shows-the-key-the-way-the-desktop-names-it
    /// Behaviour: keymap.mac.command-is-named-by-its-side-and-a-chord-the-system-keeps-stays-control
    #[test]
    fn a_mac_names_command_by_its_side_and_control_on_a_chord_the_system_keeps() {
        let text = Text {
            classic: false,
            composition: None,
        };
        let keyboard = DeviceType::Keyboard;
        for (offset, name) in [(0x1d, "Left Cmd"), (0x9d, "Right Cmd")] {
            assert_eq!(
                control_name_on(true, keyboard, key(offset), false, &text),
                name
            );
            assert_eq!(
                control_name_on(true, keyboard, key(offset), true, &text),
                "Cmd"
            );
        }
        // Off a Mac the same key keeps its own name.
        assert_eq!(
            control_name_on(false, keyboard, key(0x1d), false, &text),
            "Left Ctrl"
        );
        // Another device's control at the same offset is no Control key.
        assert_eq!(command_name(DeviceType::Mouse, key(0x1d), false), None);

        let bindings = map();
        let label = |offset| {
            let chord = ControlChord::new(key(offset), 1, activation::DOWN);
            binding_label_on(true, &bindings, chord, &text)
        };
        // Bit 0 is the Control of `map`. W isn't a chord the system keeps.
        assert_eq!(label(0x11), "Cmd / W");
        // Tab, Q, H and Space held with Command never reach the client.
        for kept in KEPT_WITH_COMMAND {
            assert!(label(kept).starts_with("Control / "), "{kept:#x}");
        }
    }

    #[test]
    fn subcontrols_use_provider_composition_and_retain_bare_name_on_failure() {
        let control = ControlCode::new(0, SubControlIndex::PositiveAxis, 0x11);
        assert_eq!(
            control_name(
                DeviceType::Keyboard,
                control,
                false,
                &Text {
                    classic: false,
                    composition: Some("localized")
                }
            ),
            "localized[positive][W]"
        );
        for composition in [None, Some("")] {
            assert_eq!(
                control_name(
                    DeviceType::Keyboard,
                    control,
                    false,
                    &Text {
                        classic: false,
                        composition
                    }
                ),
                "W"
            );
        }
        assert_eq!(sub_control_token(SubControlIndex::Other(99)), None);
    }
}
