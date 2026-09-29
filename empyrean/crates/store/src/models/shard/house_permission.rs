// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/HousePermission.cs
//! `HousePermission`: a row of the `shard` database (Entity Framework model).

// ACE: HousePermission
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HousePermission {
    // ACE: HousePermission.HouseId
    pub house_id: u32,
    // ACE: HousePermission.PlayerGuid
    pub player_guid: u32,
    // ACE: HousePermission.Storage
    pub storage: bool,
}
