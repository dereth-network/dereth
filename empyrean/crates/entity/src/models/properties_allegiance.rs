// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesAllegiance.cs
//! `PropertiesAllegiance`: one allegiance record of a biota, keyed by character id.

/// ACE: PropertiesAllegiance
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesAllegiance {
    // ACE: PropertiesAllegiance.Banned
    pub banned: bool,
    // ACE: PropertiesAllegiance.ApprovedVassal
    pub approved_vassal: bool,
}
