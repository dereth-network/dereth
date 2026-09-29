// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesAllegiance.cs
//! `BiotaPropertiesAllegiance`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesAllegiance
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesAllegiance {
    // ACE: BiotaPropertiesAllegiance.AllegianceId
    pub allegiance_id: u32,
    // ACE: BiotaPropertiesAllegiance.CharacterId
    pub character_id: u32,
    // ACE: BiotaPropertiesAllegiance.Banned
    pub banned: bool,
    // ACE: BiotaPropertiesAllegiance.ApprovedVassal
    pub approved_vassal: bool,
}
