//! The per-panel **row attributes**: where a list row keeps the id of the thing it stands for.
//!
//! Every list-driven panel puts the row's subject id in a layout attribute rather than in a
//! parallel array, so a click handler reads it back off the row element. The attribute id differs
//! per panel and each one is recovered from that panel's set-up and element-message handling; they
//! are gathered here because getting one wrong is a silent mis-selection rather than a crash.

use dereth_ui::ElementType;

use crate::element_types::ty;

/// One panel's row-subject attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowAttribute {
    /// The panel or screen behavior label used by the local lookup tables.
    pub class: &'static str,
    pub ty: ElementType,
    /// The attribute the row carries its subject id in.
    pub attribute: u32,
    /// What that id is.
    pub subject: &'static str,
}

const fn ra(
    class: &'static str,
    ty: ElementType,
    attribute: u32,
    subject: &'static str,
) -> RowAttribute {
    RowAttribute {
        class,
        ty,
        attribute,
        subject,
    }
}

/// The seven recovered row attributes.
pub const ROW_ATTRIBUTES: [RowAttribute; 7] = [
    // Rows carry the title id in attribute `0x1000008E`.
    ra(
        "TitlesPanel",
        ty::CHARACTER_TITLE,
        0x1000_008E,
        "the character title id",
    ),
    // Not the component icon: the spell-component panel's component update writes the component's *class id*
    // (its WCID) into that attribute, not an icon id, and the same value is then passed to the
    // icon maker (which resolves the icon from it) and to the buy-rate update. Three readers
    // confirm it — the selection-changed notice, the element-message handler and the component
    // update itself — all comparing it against the class id.
    ra(
        "SpellComponentPanel",
        ty::SPELL_COMPONENT,
        0x1000_004C,
        "the component's WCID",
    ),
    // Vassal rows carry the character id in attribute `0x10000001`.
    ra(
        "AllegiancePanel",
        ty::ALLEGIANCE,
        0x1000_0001,
        "the vassal's character id",
    ),
    // Rows use attribute `0x1000000D` for the fellow id.
    ra(
        "FellowshipPanel",
        ty::FELLOWSHIP,
        0x1000_000D,
        "the fellow's object id",
    ),
    // Rows carry the friend id in attribute `0x10000085`.
    ra(
        "FriendsPanel",
        ty::FRIENDS,
        0x1000_0085,
        "the friend's object id",
    ),
    // Rows carry the squelch key in attribute `0x1000008F`.
    ra("SquelchPanel", ty::SQUELCH, 0x1000_008F, "the squelch key"),
    // The recovered screen catalogue supplies the final attribute: the character-list row's instance id.
    ra(
        "CharacterManagementScreen",
        ElementType(0),
        0x1000_0009,
        "the character's instance id",
    ),
];

/// The attribute one panel's rows use.
#[must_use]
pub fn row_attribute(class: &str) -> Option<u32> {
    ROW_ATTRIBUTES
        .iter()
        .find(|r| r.class == class)
        .map(|r| r.attribute)
}

/// The two per-row meters `FellowshipPanel`'s rows carry, both driven with attribute `0x69`.
/// Members' vitals live at `0x10000283`–`0x1000028A`.
pub const FELLOWSHIP_VITAL_ELEMENTS: std::ops::RangeInclusive<u32> = 0x1000_0283..=0x1000_028A;

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered toolbar and panel behavior's per-panel "Rows carry …" sentences, and
    /// the recovered screen catalogue's character-list row.
    #[test]
    fn every_row_attribute_is_the_one_its_panel_document_names() {
        assert_eq!(row_attribute("TitlesPanel"), Some(0x1000_008E));
        assert_eq!(row_attribute("SpellComponentPanel"), Some(0x1000_004C));
        assert_eq!(row_attribute("AllegiancePanel"), Some(0x1000_0001));
        assert_eq!(row_attribute("FellowshipPanel"), Some(0x1000_000D));
        assert_eq!(row_attribute("FriendsPanel"), Some(0x1000_0085));
        assert_eq!(row_attribute("SquelchPanel"), Some(0x1000_008F));
        assert_eq!(
            row_attribute("CharacterManagementScreen"),
            Some(0x1000_0009)
        );
        assert_eq!(
            row_attribute("VendorPanel"),
            None,
            "no row attribute was recovered for it"
        );

        // Two panels never share a row attribute, which is what makes reading one back unambiguous.
        let mut a: Vec<u32> = ROW_ATTRIBUTES.iter().map(|r| r.attribute).collect();
        let n = a.len();
        a.sort_unstable();
        a.dedup();
        assert_eq!(a.len(), n);
    }

    /// Oracle: `12` §3.4 — the six panels named there are all registered game element types, which
    /// is the cheap check that a behavior label has not drifted.
    #[test]
    fn every_named_panel_is_a_registered_type_or_a_screen() {
        for r in ROW_ATTRIBUTES {
            if r.ty.0 == 0 {
                // The character list belongs to a screen, not to a registered element type.
                assert!(crate::screens::spec(r.class).is_some(), "{}", r.class);
                continue;
            }
            assert!(crate::is_game_element_type(r.ty), "{}", r.class);
            assert_eq!(
                crate::panels::catalogue::spec(r.class).map(|s| s.ty),
                Some(r.ty),
                "{}",
                r.class
            );
        }
        assert_eq!(
            FELLOWSHIP_VITAL_ELEMENTS.count(),
            8,
            "eight per-row vital elements"
        );
    }
}
