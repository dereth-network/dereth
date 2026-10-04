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
    let device = map
        .device_type_of(control.control)
        .unwrap_or(DeviceType::Keyboard);
    let key = control_name(device, control.control, false, provider);
    if key.is_empty() {
        return key;
    }
    let mut parts = Vec::new();
    for &bit in provider.modifier_order() {
        if control.meta_mode & bit != 0 {
            if let Some(cs) = map.key_from_meta_mode(bit) {
                let device = map.device_type_of(cs).unwrap_or(DeviceType::Keyboard);
                let name = control_name(device, cs, true, provider);
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
