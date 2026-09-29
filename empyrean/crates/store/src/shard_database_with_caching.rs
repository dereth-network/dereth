// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/ShardDatabaseWithCaching.cs
//! `ShardDatabaseWithCaching`: keeps recently read or saved database biotas in memory, player
//! biotas for `player_biota_retention_time` and others for `non_player_biota_retention_time`, so a
//! save updates the cached rows instead of reading them again. ACE's `DatabaseManager` uses it with
//! `ShardPlayerBiotaCacheTime` / `ShardNonPlayerBiotaCacheTime` minutes.
//!
//! It wraps any [`ShardDatabase`] backend and overrides `GetBiota`, `SaveBiota` and `RemoveBiota`, as
//! ACE's subclass does; the other methods reach the override through the trait. DIVERGE (arch):
//! ACE caches the tracked Entity Framework object with its context; here the cache holds the row
//! model, and reads return a copy of it. `DateTime.UtcNow` is the injected clock.

use std::collections::HashMap;
use std::sync::Arc;

use empyrean_common::clock::Clock;
use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};

use crate::adapter::{BiotaConverter, BiotaUpdater};
use crate::error::StoreError;
use crate::models::shard::{Biota, Character};
use crate::shard_database::{
    is_player, BiotaQuery, CacheRetentionTimes, CharacterQuery, PopulatedCollectionFlags,
    ShardDatabase,
};

// ACE: ShardDatabaseWithCaching.CacheObject
#[derive(Debug, Clone)]
struct CacheObject {
    last_seen: DotNetDateTime,
    cached_object: Biota,
}

// ACE: ShardDatabaseWithCaching
/// A caching shard database over the backend `S`.
pub struct ShardDatabaseWithCaching<S> {
    base: S,
    clock: Arc<dyn Clock>,

    // ACE: ShardDatabaseWithCaching.PlayerBiotaRetentionTime
    pub player_biota_retention_time: TimeSpan,
    // ACE: ShardDatabaseWithCaching.NonPlayerBiotaRetentionTime
    pub non_player_biota_retention_time: TimeSpan,

    /// ACE's `Dictionary<uint, CacheObject>`: iteration order matters only for maintenance, which
    /// removes by key, so a `HashMap` keeps the same results.
    biota_cache: HashMap<u32, CacheObject>,

    last_maintenance_interval: DotNetDateTime,
}

impl<S: std::fmt::Debug> std::fmt::Debug for ShardDatabaseWithCaching<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardDatabaseWithCaching")
            .field("base", &self.base)
            .field(
                "player_biota_retention_time",
                &self.player_biota_retention_time,
            )
            .field(
                "non_player_biota_retention_time",
                &self.non_player_biota_retention_time,
            )
            .field("cached", &self.biota_cache.len())
            .finish_non_exhaustive()
    }
}

/// `MaintenanceInterval`: one minute.
fn maintenance_interval() -> TimeSpan {
    TimeSpan::from_minutes(1.0)
}

impl<S: ShardDatabase> ShardDatabaseWithCaching<S> {
    // ACE: ShardDatabaseWithCaching.ShardDatabaseWithCaching
    pub fn new(
        base: S,
        clock: Arc<dyn Clock>,
        player_biota_retention_time: TimeSpan,
        non_player_biota_retention_time: TimeSpan,
    ) -> Self {
        Self {
            base,
            clock,
            player_biota_retention_time,
            non_player_biota_retention_time,
            biota_cache: HashMap::new(),
            last_maintenance_interval: DotNetDateTime::default(),
        }
    }

    /// The wrapped backend.
    pub fn base(&mut self) -> &mut S {
        &mut self.base
    }

    // ACE: ShardDatabaseWithCaching.TryPerformMaintenance
    fn try_perform_maintenance(&mut self) {
        let now = self.clock.utc_now();
        if self
            .last_maintenance_interval
            .add_ticks(maintenance_interval().ticks())
            > now
        {
            return;
        }

        let player = self.player_biota_retention_time.ticks();
        let non_player = self.non_player_biota_retention_time.ticks();
        self.biota_cache.retain(|&key, value| {
            let retention = if is_player(key) { player } else { non_player };
            value.last_seen.add_ticks(retention) >= now
        });

        self.last_maintenance_interval = self.clock.utc_now();
    }

    // ACE: ShardDatabaseWithCaching.TryAddToCache
    fn try_add_to_cache(&mut self, biota: &Biota) {
        let last_seen = self.clock.utc_now();
        if is_player(biota.id) {
            if self.player_biota_retention_time > TimeSpan::ZERO {
                self.biota_cache.insert(
                    biota.id,
                    CacheObject {
                        last_seen,
                        cached_object: biota.clone(),
                    },
                );
            }
        } else if self.non_player_biota_retention_time > TimeSpan::ZERO {
            self.biota_cache.insert(
                biota.id,
                CacheObject {
                    last_seen,
                    cached_object: biota.clone(),
                },
            );
        }
    }

    // ACE: ShardDatabaseWithCaching.GetBiotaCacheKeys
    #[must_use]
    pub fn get_biota_cache_keys(&self) -> Vec<u32> {
        self.biota_cache.keys().copied().collect()
    }

    /// ACE's `GetBiota(ShardDbContext context, uint id, bool doNotAddToCache)` override.
    fn get_biota_cached(&mut self, id: u32, do_not_add_to_cache: bool) -> Option<Biota> {
        self.try_perform_maintenance();

        let now = self.clock.utc_now();
        if let Some(cached_biota) = self.biota_cache.get_mut(&id) {
            cached_biota.last_seen = now;

            return Some(cached_biota.cached_object.clone());
        }

        let biota = self.base.get_biota(id, false);

        if let Some(b) = &biota {
            if !do_not_add_to_cache {
                self.try_add_to_cache(b);
            }
        }

        biota
    }
}

impl<S: ShardDatabase> ShardDatabase for ShardDatabaseWithCaching<S> {
    fn load_biota_row(&mut self, id: u32) -> Result<Option<Biota>, StoreError> {
        self.base.load_biota_row(id)
    }
    fn load_biota_collections(
        &mut self,
        biota: &mut Biota,
        flags: PopulatedCollectionFlags,
    ) -> Result<(), StoreError> {
        self.base.load_biota_collections(biota, flags)
    }
    fn write_biota(&mut self, biota: &mut Biota) -> Result<(), StoreError> {
        self.base.write_biota(biota)
    }
    fn delete_biota(&mut self, id: u32) -> Result<(), StoreError> {
        self.base.delete_biota(id)
    }
    fn query_biota_ids(&mut self, query: BiotaQuery) -> Result<Vec<u32>, StoreError> {
        self.base.query_biota_ids(query)
    }
    fn count_biotas(&mut self) -> Result<i64, StoreError> {
        self.base.count_biotas()
    }
    fn query_characters(
        &mut self,
        query: CharacterQuery<'_>,
    ) -> Result<Vec<Character>, StoreError> {
        self.base.query_characters(query)
    }
    fn load_character_properties(&mut self, character: &mut Character) -> Result<(), StoreError> {
        self.base.load_character_properties(character)
    }
    fn write_character(&mut self, character: &Character) -> Result<(), StoreError> {
        self.base.write_character(character)
    }
    fn begin_batch(&mut self) -> Result<(), StoreError> {
        self.base.begin_batch()
    }
    fn commit_batch(&mut self) -> Result<(), StoreError> {
        let result = self.base.commit_batch();
        if result.is_err() {
            // The cached rows may hold writes that were rolled back.
            self.biota_cache.clear();
        }
        result
    }
    fn rollback_batch(&mut self) {
        self.base.rollback_batch();
        self.biota_cache.clear();
    }
    fn delete_character(&mut self, id: u32) -> Result<(), StoreError> {
        self.base.delete_character(id)
    }
    fn cache_retention_times(&mut self) -> Option<CacheRetentionTimes<'_>> {
        Some(CacheRetentionTimes {
            player_biota_retention_time: &mut self.player_biota_retention_time,
            non_player_biota_retention_time: &mut self.non_player_biota_retention_time,
        })
    }

    // ACE: ShardDatabaseWithCaching.GetBiota
    fn get_biota(&mut self, id: u32, do_not_add_to_cache: bool) -> Option<Biota> {
        if is_player(id) {
            if self.player_biota_retention_time > TimeSpan::ZERO {
                return self.get_biota_cached(id, do_not_add_to_cache); // This will add the result into the caches
            }
        } else if self.non_player_biota_retention_time > TimeSpan::ZERO {
            return self.get_biota_cached(id, do_not_add_to_cache); // This will add the result into the caches
        }

        // base.GetBiota(id) reaches the virtual GetBiota(context, id) override, which finds nothing
        // cached (nothing is added while this retention time is zero) and reads the backend.
        self.get_biota_cached(id, do_not_add_to_cache)
    }

    // ACE: ShardDatabaseWithCaching.SaveBiota
    fn save_biota(
        &mut self,
        biota: &mut empyrean_entity::Biota,
        do_not_add_to_cache: bool,
    ) -> bool {
        let now = self.clock.utc_now();
        if let Some(mut cached) = self.biota_cache.remove(&biota.id) {
            cached.last_seen = now;

            BiotaUpdater::update_database_biota(biota, &mut cached.cached_object);

            let result = self.base.do_save_biota(&mut cached.cached_object);
            self.biota_cache.insert(biota.id, cached);
            return result;
        }

        // Biota does not exist in the cache

        let existing_biota = self.base.get_biota(biota.id, do_not_add_to_cache);

        let mut existing_biota = match existing_biota {
            None => BiotaConverter::convert_from_entity_biota(biota, false),
            Some(mut existing_biota) => {
                BiotaUpdater::update_database_biota(biota, &mut existing_biota);
                existing_biota
            }
        };

        if self.base.do_save_biota(&mut existing_biota) {
            if !do_not_add_to_cache {
                self.try_add_to_cache(&existing_biota);
            }

            return true;
        }

        false
    }

    // ACE: ShardDatabaseWithCaching.RemoveBiota
    fn remove_biota(&mut self, id: u32) -> bool {
        self.biota_cache.remove(&id);

        self.base.remove_biota(id)
    }
}
