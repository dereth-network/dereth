//! The blocks a frame holds, as a set: whether a block is still resident is answered without
//! reading the frame's list of blocks again. The passes that keep something per block let go of
//! the blocks the frame no longer holds every frame, and at the longest draw distance a frame
//! holds thousands of blocks.

use crate::snapshot::HifiTerrainBlock;

/// One bit for every block id, set for the blocks the frame holds.
#[derive(Debug)]
pub struct ResidentBlocks {
    bits: Box<[u64; 1024]>,
    /// How many of the frame's blocks were read to make the set.
    read: usize,
}

impl ResidentBlocks {
    /// The blocks of `terrain`, each read once.
    #[must_use]
    pub fn of(terrain: &[HifiTerrainBlock]) -> Self {
        let mut bits = Box::new([0u64; 1024]);
        for t in terrain {
            bits[usize::from(t.block >> 6)] |= 1 << (t.block & 63);
        }
        Self {
            bits,
            read: terrain.len(),
        }
    }

    /// Whether the frame holds `block`.
    #[must_use]
    pub fn holds(&self, block: u16) -> bool {
        self.bits[usize::from(block >> 6)] & (1 << (block & 63)) != 0
    }

    /// How many of the frame's blocks were read to make the set: each once.
    #[must_use]
    pub fn read(&self) -> usize {
        self.read
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi presentation)
    use std::collections::{HashMap, HashSet};

    use super::*;

    fn block(id: u16) -> HifiTerrainBlock {
        HifiTerrainBlock {
            block: id,
            ..HifiTerrainBlock::default()
        }
    }

    /// The longest draw distance's window: 51 blocks each way, every one held by a pass, and a
    /// step's worth of blocks the frame has let go of.
    #[test]
    fn letting_go_of_the_blocks_a_frame_no_longer_holds_reads_its_blocks_once() {
        let ids = |x0: u16| (x0..x0 + 51).flat_map(|x| (0..51u16).map(move |y| (x << 8) | y));
        let terrain: Vec<HifiTerrainBlock> = ids(100).map(block).collect();
        let mut kept: HashMap<u16, u32> = ids(99).map(|id| (id, 0)).collect();
        let mut stale: HashSet<u16> = ids(99).collect();
        let resident = ResidentBlocks::of(&terrain);
        kept.retain(|id, _| resident.holds(*id));
        stale.retain(|id| resident.holds(*id));
        assert_eq!(
            kept.len(),
            50 * 51,
            "the column the frame let go of is gone"
        );
        assert_eq!(stale.len(), 50 * 51);
        assert!(kept.keys().all(|id| id >> 8 >= 100));
        assert_eq!(
            resident.read(),
            terrain.len(),
            "each of the frame's {} blocks read once, for {} kept and {} stale",
            terrain.len(),
            51 * 51,
            51 * 51
        );
    }
}
