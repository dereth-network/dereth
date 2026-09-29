// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/ConfigPropertiesDouble.cs
//! `ConfigPropertiesDouble`: a row of the `shard` database (Entity Framework model).

// ACE: ConfigPropertiesDouble
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigPropertiesDouble {
    // ACE: ConfigPropertiesDouble.Key
    pub key: String,
    // ACE: ConfigPropertiesDouble.Value
    pub value: f64,
    // ACE: ConfigPropertiesDouble.Description
    pub description: Option<String>,
}
