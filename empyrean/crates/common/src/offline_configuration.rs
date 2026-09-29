// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/OfflineConfiguration.cs
//! `OfflineConfiguration` (`empyrean.toml` → `[offline]`): maintenance before the world starts.
//!
//! DIVERGE: ACE's update switches (`AutoUpdateWorldDatabase`, `AutoServerUpdateCheck`,
//! `AutoApplyWorldCustomizations`, `WorldCustomizationAddedPaths`, `RecurseWorldCustomizationPaths`,
//! `AutoApplyDatabaseUpdates`) are not part of Empyrean's configuration: world content changes by
//! rebuilding `world.pack` with `empyrean-import`, and the shard schema is created and migrated by
//! the store itself.

use serde::{Deserialize, Serialize};

use crate::json;

// ACE: OfflineConfiguration
/// Offline maintenance switches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OfflineConfiguration {
    // ACE: OfflineConfiguration.PurgeDeletedCharacters
    /// Purge characters deleted longer than `PurgeDeletedCharactersDays` ago.
    #[serde(rename = "PurgeDeletedCharacters")]
    pub purge_deleted_characters: bool,

    // ACE: OfflineConfiguration.PurgeDeletedCharactersDays
    /// Days a character must have been deleted before it is purged.
    #[serde(
        rename = "PurgeDeletedCharactersDays",
        deserialize_with = "json::num_i32"
    )]
    pub purge_deleted_characters_days: i32,

    // ACE: OfflineConfiguration.PurgeOrphanedBiotas
    /// Purge biotas disconnected from the world.
    #[serde(rename = "PurgeOrphanedBiotas")]
    pub purge_orphaned_biotas: bool,

    // ACE: OfflineConfiguration.PruneDeletedCharactersFromFriendLists
    /// Prune deleted characters from friend lists.
    #[serde(rename = "PruneDeletedCharactersFromFriendLists")]
    pub prune_deleted_characters_from_friend_lists: bool,

    // ACE: OfflineConfiguration.PruneDeletedObjectsFromShortcutBars
    /// Prune deleted objects from shortcut bars.
    #[serde(rename = "PruneDeletedObjectsFromShortcutBars")]
    pub prune_deleted_objects_from_shortcut_bars: bool,

    // ACE: OfflineConfiguration.PruneDeletedCharactersFromSquelchLists
    /// Prune deleted characters from squelch lists.
    #[serde(rename = "PruneDeletedCharactersFromSquelchLists")]
    pub prune_deleted_characters_from_squelch_lists: bool,
}

impl Default for OfflineConfiguration {
    fn default() -> Self {
        Self {
            purge_deleted_characters: false,
            purge_deleted_characters_days: 30,
            purge_orphaned_biotas: false,
            prune_deleted_characters_from_friend_lists: true,
            prune_deleted_objects_from_shortcut_bars: false,
            prune_deleted_characters_from_squelch_lists: false,
        }
    }
}
