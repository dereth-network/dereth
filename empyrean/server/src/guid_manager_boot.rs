//! Not an ACE port: the shard reads `GuidManager.Initialize` makes at start-up.
//!
//! ACE's allocator constructors queue `DatabaseManager.Shard.GetMaxGuidFoundInRange` and
//! `GetSequenceGaps` on the serialized shard and block until the callback answers. Here the
//! answers come from the shard's base database directly (DIVERGE, arch): the world thread runs
//! the shard's callbacks with `&mut World`, which `guid_manager::initialize` already holds, and at
//! start-up nothing else is queued, so the answers are the same.

use std::sync::Arc;

use empyrean_common::clock::SystemClock;
use empyrean_store::{MemShard, ShardHandle};
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::World;

/// [`ShardGuidQueries`] over a shard handle's base database.
#[derive(Debug)]
pub struct ShardBaseGuidQueries<'a> {
    shard: &'a ShardHandle<World>,
}

impl ShardGuidQueries for ShardBaseGuidQueries<'_> {
    fn get_max_guid_found_in_range(&mut self, min: u32, max: u32) -> u32 {
        self.shard
            .base_database()
            .get_max_guid_found_in_range(min, max)
    }

    fn get_sequence_gaps(&mut self, min: u32, limit: u32) -> Vec<(u32, u32)> {
        self.shard.base_database().get_sequence_gaps(min, limit)
    }
}

/// `GuidManager.Initialize()` against `World.shard` (`DatabaseManager.Shard`). The handle is
/// lent out of the world for the call (an empty in-memory shard stands in meanwhile) and put back.
pub fn initialize(w: &mut World) {
    let stand_in =
        ShardHandle::synchronous(Box::new(MemShard::new()), Arc::new(SystemClock::new()));
    let shard = std::mem::replace(&mut w.shard, stand_in);

    // DIVERGE: the allocators read the base database directly instead of queueing on the serialized shard and waiting (see the module docs).
    guid_manager::initialize(w, &mut ShardBaseGuidQueries { shard: &shard });

    w.shard = shard;
}
