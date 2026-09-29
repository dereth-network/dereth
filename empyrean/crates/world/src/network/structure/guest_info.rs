// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/GuestInfo.cs
//! Port of `Source/ACE.Server/Network/Structure/GuestInfo.cs`.

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::write_record;

// ACE: GuestInfo
/// Set of information related to a house guest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuestInfo {
    // ACE: GuestInfo.ItemStoragePermission
    /// False = access to house, True = access to house+storage
    pub item_storage_permission: bool,
    // ACE: GuestInfo.GuestName
    /// Name of the guest (`null` for the parameterless constructor).
    pub guest_name: Option<String>,
}

impl GuestInfo {
    // ACE: GuestInfo.GuestInfo
    /// `new GuestInfo(bool itemStoragePermission, string name)`.
    #[must_use]
    pub fn new(item_storage_permission: bool, name: &str) -> Self {
        Self {
            item_storage_permission,
            guest_name: Some(name.to_owned()),
        }
    }
}

// ACE: GuestInfoExtensions.Write
pub fn write(writer: &mut Vec<u8>, info: &GuestInfo) {
    write_record(writer, &[info.guest_name.as_deref().unwrap_or("")], |w| {
        record(info).write(w)
    });
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(info: &GuestInfo) -> dereth_protocol::trade::GuestInfo {
    dereth_protocol::trade::GuestInfo {
        item_storage_permission: i32::from(info.item_storage_permission),
        char_name: ace_str(info.guest_name.as_deref()),
    }
}
