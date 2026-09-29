// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/ConfigPropertiesBoolean.cs
//! `ConfigPropertiesBoolean`: a row of the `shard` database (Entity Framework model).

// ACE: ConfigPropertiesBoolean
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigPropertiesBoolean {
    // ACE: ConfigPropertiesBoolean.Key
    pub key: String,
    // ACE: ConfigPropertiesBoolean.Value
    pub value: bool,
    // ACE: ConfigPropertiesBoolean.Description
    pub description: Option<String>,
}
