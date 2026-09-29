//! `StateDesc` — the unit of "how an element looks in one state".
//!
//! State-description decoding and property inheritance for UI layouts.

use crate::props::PropertyCollection;
use crate::StateId;

pub use dereth_assets::ui::{MediaDesc, MediaFields};

/// Bits of the incorporation flags, as the element description's position fields and the state
/// description's serialised form use them.
pub mod incorporation {
    pub const PASS_TO_CHILDREN: u32 = 0x01;
    pub const X: u32 = 0x02;
    pub const Y: u32 = 0x04;
    pub const WIDTH: u32 = 0x08;
    pub const HEIGHT: u32 = 0x10;
    pub const Z_LEVEL: u32 = 0x20;
    /// What a version-0 archive forces: `x|y|w|h|z all present`. No retail file uses it.
    pub const LEGACY_ALL_GEOMETRY: u32 = 0x3E;
}

/// The properties, media and incorporation flags for one visual state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StateDesc {
    /// The incorporation flags — which of the fields below this record actually *defines*.
    pub incorporation: u32,
    /// The state id. 0 is the element's own base state.
    pub state_id: StateId,
    /// The state is driven by code rather than by the layout. Not serialised in the
    /// version-3 form, so it is always false on data read from the dat.
    pub is_code: bool,
    /// Setting this state on this element also sets the same state on every child.
    pub pass_to_children: bool,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub z_level: i32,
    pub properties: PropertyCollection,
    /// The media script for this state, run once per state entry.
    pub media: Vec<MediaDesc>,
}

impl StateDesc {
    /// The state incorporation, as retail does it: OR the flags in, copy pass-to-children and each
    /// geometry field whose bit is set in `src`, **append** `src`'s media, then update the
    /// properties from `src`.
    ///
    /// Note what it does **not** copy: `state_id` and `is_code` stay with the base.
    pub fn incorporate(&mut self, src: &Self) {
        self.incorporation |= src.incorporation;
        if src.incorporation & incorporation::PASS_TO_CHILDREN != 0 {
            self.pass_to_children = src.pass_to_children;
        }
        if src.incorporation & incorporation::X != 0 {
            self.x = src.x;
        }
        if src.incorporation & incorporation::Y != 0 {
            self.y = src.y;
        }
        if src.incorporation & incorporation::WIDTH != 0 {
            self.width = src.width;
        }
        if src.incorporation & incorporation::HEIGHT != 0 {
            self.height = src.height;
        }
        if src.incorporation & incorporation::Z_LEVEL != 0 {
            self.z_level = src.z_level;
        }
        // Media lists **append**, they do not replace. This is what lets a
        // derived state add a click sound to a base state's image without losing the image.
        self.media.extend(src.media.iter().cloned());
        self.properties.update_from(&src.properties);
    }

    /// The design rectangle this record carries, as an inclusive box.
    #[must_use]
    pub fn box_(&self) -> crate::region::Box2D {
        crate::region::Box2D::from_xywh(self.x, self.y, self.width, self.height)
    }

    /// Convert one of the asset crate's decoded records. Geometry lives on `ElementDesc` on the wire, so an
    /// alternate state's rectangle is always zero here — exactly as in the client, where the five
    /// conditional fields are written inside `ElementDesc`.
    #[must_use]
    pub fn from_asset(s: &dereth_assets::ui::StateDesc) -> Self {
        Self {
            incorporation: s.incorporation_flags,
            state_id: StateId(s.state_id),
            is_code: false,
            pass_to_children: s.pass_to_children,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            z_level: 0,
            properties: PropertyCollection::from_asset(&s.properties),
            media: s.media.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::PropertyValue;

    /// As in the retail client's state incorporation, a field whose bit is clear in `src` is not
    /// touched.
    #[test]
    fn only_flagged_fields_are_overwritten() {
        let mut base = StateDesc {
            incorporation: incorporation::LEGACY_ALL_GEOMETRY,
            x: 1,
            y: 2,
            width: 3,
            height: 4,
            z_level: 5,
            ..StateDesc::default()
        };
        let src = StateDesc {
            incorporation: incorporation::WIDTH,
            x: 100,
            y: 200,
            width: 300,
            height: 400,
            z_level: 500,
            ..StateDesc::default()
        };
        base.incorporate(&src);
        assert_eq!(
            (base.x, base.y, base.width, base.height, base.z_level),
            (1, 2, 300, 4, 5)
        );
        assert_eq!(
            base.incorporation,
            incorporation::LEGACY_ALL_GEOMETRY | incorporation::WIDTH
        );
    }

    /// Retail's state incorporation concatenates media; the lists **append**.
    #[test]
    fn media_lists_concatenate_and_properties_overwrite() {
        let img = |n: u32| MediaDesc {
            media_type: 5,
            type_echo_ok: true,
            fields: MediaFields::Image {
                file: dereth_primitives::DataId(n),
                draw_mode: 1,
            },
        };
        let mut base = StateDesc {
            media: vec![img(1)],
            ..StateDesc::default()
        };
        base.properties.set(7, PropertyValue::Integer(1));
        let mut src = StateDesc {
            media: vec![img(2)],
            ..StateDesc::default()
        };
        src.properties.set(7, PropertyValue::Integer(2));
        base.incorporate(&src);
        assert_eq!(base.media.len(), 2);
        assert_eq!(base.properties.get_int(7), Some(2));
    }

    /// Retail assigns neither the state id nor the is-code flag.
    #[test]
    fn incorporate_keeps_the_bases_state_id() {
        let mut base = StateDesc {
            state_id: StateId(7),
            ..StateDesc::default()
        };
        base.incorporate(&StateDesc {
            state_id: StateId(9),
            ..StateDesc::default()
        });
        assert_eq!(base.state_id, StateId(7));
    }
}
