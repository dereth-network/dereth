// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/HookProfile.cs
//! Port of `Source/ACE.Server/Network/Structure/HookProfile.cs`.

use empyrean_entity::enums::{AmmoType, EquipMask};

use crate::network::game_messages::game_message::write_record;

// ACE: HookFlags
/// `[Flags] enum HookFlags` (`int`).
#[allow(non_snake_case, non_upper_case_globals)]
pub mod HookFlags {
    pub const None: i32 = 0x0;
    pub const Inscribable: i32 = 0x1;
    pub const IsHealer: i32 = 0x2;
    pub const IsFood: i32 = 0x4;
    pub const IsLockpick: i32 = 0x8;
}

// ACE: HookProfile
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HookProfile {
    // ACE: HookProfile.Flags
    /// `HookFlags`.
    pub flags: i32,
    // ACE: HookProfile.ValidLocations
    pub valid_locations: EquipMask,
    // ACE: HookProfile.AmmoType
    pub ammo_type: AmmoType,
}

// ACE: HookProfileExtensions.Write
pub fn write(writer: &mut Vec<u8>, hook: &HookProfile) {
    write_record(writer, &[], |w| record(hook).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(hook: &HookProfile) -> dereth_protocol::types::HookAppraisalProfile {
    dereth_protocol::types::HookAppraisalProfile {
        bitfield: hook.flags.cast_unsigned(),
        valid_locations: hook.valid_locations.0,
        ammo_type: hook.ammo_type.into(),
    }
}
