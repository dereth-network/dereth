// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Auth/Accesslevel.cs
//! `Accesslevel`: a row of the `auth` database (Entity Framework model).

// ACE: Accesslevel
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Accesslevel {
    // ACE: Accesslevel.Level
    pub level: u32,
    // ACE: Accesslevel.Name
    pub name: String,
    // ACE: Accesslevel.Prefix
    pub prefix: Option<String>,
}
