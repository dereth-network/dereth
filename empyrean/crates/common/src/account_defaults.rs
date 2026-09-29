// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/AccountDefaults.cs
//! `AccountDefaults` (`Config.js` → `Server.Accounts`).

use serde::{Deserialize, Serialize};

use crate::json;

// ACE: AccountDefaults
/// Account creation and permission defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountDefaults {
    // ACE: AccountDefaults.OverrideCharacterPermissions
    /// Account-level (true) rather than character-level (retail) permissions.
    #[serde(rename = "OverrideCharacterPermissions")]
    pub override_character_permissions: bool,

    // ACE: AccountDefaults.DefaultAccessLevel
    /// Access level for new accounts (0 = Player).
    #[serde(rename = "DefaultAccessLevel", deserialize_with = "json::num_u32")]
    pub default_access_level: u32,

    // ACE: AccountDefaults.AllowAutoAccountCreation
    /// Create an account on first login.
    #[serde(rename = "AllowAutoAccountCreation")]
    pub allow_auto_account_creation: bool,

    // ACE: AccountDefaults.PasswordHashWorkFactor
    /// BCrypt work factor for passwords.
    #[serde(rename = "PasswordHashWorkFactor", deserialize_with = "json::num_i32")]
    pub password_hash_work_factor: i32,

    // ACE: AccountDefaults.ForceWorkFactorMigration
    /// Rehash passwords whose work factor differs from `PasswordHashWorkFactor`.
    #[serde(rename = "ForceWorkFactorMigration")]
    pub force_work_factor_migration: bool,
}

impl Default for AccountDefaults {
    fn default() -> Self {
        Self {
            override_character_permissions: true,
            default_access_level: 0,
            allow_auto_account_creation: true,
            password_hash_work_factor: 8,
            force_work_factor_migration: true,
        }
    }
}
