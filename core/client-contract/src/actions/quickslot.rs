//! Shortcut-bar actions. Slot numbers are one-based, matching their serialized names.
use super::ActionId;

/// Create a shortcut for the selected item.
pub const CREATE: ActionId = ActionId(0x1000_010D);

/// The shortcut named by an action, including the second, separately numbered group.
#[must_use]
pub const fn number(action: ActionId) -> Option<u32> {
    match action.0 {
        0x1000_0042..=0x1000_004D => Some(action.0 - 0x1000_0042 + 1),
        0x1000_0132..=0x1000_0137 => Some(action.0 - 0x1000_0132 + 13),
        _ => None,
    }
}

/// The classic shortcut bar has nine positions.
#[must_use]
pub const fn classic_number(action: ActionId) -> Option<u32> {
    match number(action) {
        Some(n) if n <= 9 => Some(n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the numeric shortcut vocabulary matches the serialized action names).
    use super::*;

    #[test]
    fn shortcut_groups_match_names_without_treating_the_gap_as_slots() {
        for raw in 0x1000_0041..=0x1000_0138 {
            let id = ActionId(raw);
            let name = super::super::names::enum_name_for_action(id);
            let old = name
                .strip_prefix("UseQuickSlot_")
                .and_then(|n| n.parse().ok());
            assert_eq!(number(id), old, "{raw:#x}");
            assert_eq!(classic_number(id), old.filter(|n| (1..=9).contains(n)));
        }
        for id in [ActionId(0), ActionId(u32::MAX), CREATE] {
            assert_eq!(number(id), None);
        }
    }
}
