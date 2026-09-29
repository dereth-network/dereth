// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/ItemProfile.cs
//! Port of `Source/ACE.Server/Entity/ItemProfile.cs`.

/// One entry of a vendor buy or sell request.
// ACE: ItemProfile
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemProfile {
    // original data struct
    /// sent as int, not as uint -- needs to be verified > 0
    // ACE: ItemProfile.Amount
    pub amount: i32,
    // ACE: ItemProfile.ObjectGuid
    pub object_guid: u32,

    // extended server data
    // ACE: ItemProfile.WeenieClassId
    pub weenie_class_id: u32,
    // ACE: ItemProfile.Palette
    pub palette: Option<i32>,
    // ACE: ItemProfile.Shade
    pub shade: Option<f64>,
}

impl ItemProfile {
    // ACE: ItemProfile.ItemProfile
    #[must_use]
    pub fn new(amount: i32, object_guid: u32) -> Self {
        ItemProfile {
            amount,
            object_guid,
            ..ItemProfile::default()
        }
    }

    /// If false, should be rejected as early as possible
    // ACE: ItemProfile.IsValidAmount
    #[must_use]
    pub fn is_valid_amount(&self) -> bool {
        self.amount > 0
    }
}
