//! The gameplay reason a confirmation dialog is on screen.
//!
//! The server asks the player to confirm something (swearing allegiance, raising a skill, joining
//! a fellowship, ...) and names the reason with one of these numbers; the answer goes back with
//! the same number. The object model keeps the pending request and the gameplay screen shows the
//! dialog, so both read the reason here. `dereth_client_model::advancement` and
//! `dereth_ui::dialog` re-export it.

/// Why a confirmation is on screen, by the number the request carries. Any number the client does
/// not name reads as [`Undef`](Self::Undef).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
#[non_exhaustive]
pub enum ConfirmationType {
    Undef = 0,
    /// A request to confirm swearing allegiance.
    AllegianceSwear = 1,
    /// A request to confirm changing a skill.
    AlterSkill = 2,
    /// A request to confirm changing an attribute.
    AlterAttribute = 3,
    /// A request to confirm fellowship recruitment.
    FellowshipRecruit = 4,
    /// A request to confirm a crafting interaction.
    CraftInteraction = 5,
    /// A request to confirm using an augmentation.
    UseAugmentation = 6,
    /// A generic yes/no confirmation request.
    YesNo = 7,
}

impl ConfirmationType {
    /// The reason an unsigned number names; anything outside `1..=7` is `Undef`.
    #[must_use]
    pub const fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::AllegianceSwear,
            2 => Self::AlterSkill,
            3 => Self::AlterAttribute,
            4 => Self::FellowshipRecruit,
            5 => Self::CraftInteraction,
            6 => Self::UseAugmentation,
            7 => Self::YesNo,
            _ => Self::Undef,
        }
    }

    /// The reason a signed number names, as the request carries it; anything outside `1..=7`,
    /// negative numbers included, is `Undef`.
    #[must_use]
    pub const fn from_raw(v: i32) -> Self {
        if v < 0 {
            Self::Undef
        } else {
            Self::from_u32(v.unsigned_abs())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConfirmationType;

    #[test]
    fn signed_and_unsigned_numbers_name_the_same_reasons() {
        for v in -3..=12i32 {
            let signed = ConfirmationType::from_raw(v);
            if (1..=7).contains(&v) {
                assert_eq!(signed as i32, v);
                assert_eq!(ConfirmationType::from_u32(v.unsigned_abs()), signed);
            } else {
                assert_eq!(signed, ConfirmationType::Undef);
            }
        }
        assert_eq!(
            ConfirmationType::from_u32(u32::MAX),
            ConfirmationType::Undef
        );
        assert_eq!(
            ConfirmationType::from_raw(i32::MIN),
            ConfirmationType::Undef
        );
    }
}
