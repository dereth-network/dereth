// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/ConfigPropertiesString.cs
//! `ConfigPropertiesString`: a row of the `shard` database (Entity Framework model).

// ACE: ConfigPropertiesString
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigPropertiesString {
    // ACE: ConfigPropertiesString.Key
    pub key: String,
    // ACE: ConfigPropertiesString.Value
    pub value: String,
    // ACE: ConfigPropertiesString.Description
    pub description: Option<String>,
}
