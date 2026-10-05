//! A persisted GUID range with nothing in it, for worlds that start without a database.
use empyrean_world::managers::guid_manager::ShardGuidQueries;

/// `GuidManager.Initialize` over an empty shard: no GUID is in use in any range, and there are no
/// gaps to recycle, so every range starts at its minimum.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}
