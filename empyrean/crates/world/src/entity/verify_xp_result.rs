// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/VerifyXpResult.cs
//! Port of `Source/ACE.Server/Entity/VerifyXpResult.cs`.
//!
//! One result of the `verify-xp` command (`DeveloperFixCommands`). ACE holds the
//! `OfflinePlayer`; here it is the player's guid (`PlayerManager`'s offline entry).

use empyrean_entity::ObjectGuid;

// ACE: VerifyXpResult
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyXpResult {
    /// `Player`.
    pub player: ObjectGuid,
    /// `Calculated`.
    pub calculated: i64,
    /// `Current`.
    pub current: i64,
}

impl VerifyXpResult {
    // ACE: VerifyXpResult.VerifyXpResult
    #[must_use]
    pub const fn new(player: ObjectGuid, calculated: i64, current: i64) -> Self {
        Self {
            player,
            calculated,
            current,
        }
    }

    // ACE: VerifyXpResult.Diff
    /// `Current - Calculated`.
    #[must_use]
    pub const fn diff(&self) -> i64 {
        self.current.wrapping_sub(self.calculated)
    }
}
