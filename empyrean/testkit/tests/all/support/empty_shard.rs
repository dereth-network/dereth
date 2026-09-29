//! An empty persisted GUID range for isolated worlds.
use empyrean_world::managers::guid_manager::ShardGuidQueries;

/// `GuidManager.Initialize` over an empty shard.
pub(crate) struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}
