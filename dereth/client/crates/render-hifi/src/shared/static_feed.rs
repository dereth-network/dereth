//! The retained geometry of each resident block: its landscape triangles and static batches, held
//! on the device once per block contents and replaced when the block is rebuilt.

use std::collections::HashMap;

/// Which block contents are held.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaticFeed {
    generations: HashMap<u16, u32>,
}

impl StaticFeed {
    /// Whether `block` at `generation` still has to be sent.
    #[must_use]
    pub fn needs(&self, block: u16, generation: u32) -> bool {
        self.generations.get(&block) != Some(&generation)
    }

    /// Record that `block` at `generation` is held.
    pub fn hold(&mut self, block: u16, generation: u32) {
        self.generations.insert(block, generation);
    }

    /// Forget `block`, as when it leaves the resident window.
    pub fn drop_block(&mut self, block: u16) {
        self.generations.remove(&block);
    }
}
