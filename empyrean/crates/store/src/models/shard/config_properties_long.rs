// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/ConfigPropertiesLong.cs
//! `ConfigPropertiesLong`: a row of the `shard` database (Entity Framework model).

// ACE: ConfigPropertiesLong
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigPropertiesLong {
    // ACE: ConfigPropertiesLong.Key
    pub key: String,
    // ACE: ConfigPropertiesLong.Value
    pub value: i64,
    // ACE: ConfigPropertiesLong.Description
    pub description: Option<String>,
}
