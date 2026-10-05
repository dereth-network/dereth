//! Texture links and descriptor accounting; device resources and completion policy stay local.

use crate::descriptor::{
    DescriptorAllocator, Released, TextureKey, TextureTable, TextureTableStats,
};
use crate::{DescriptorUsage, RenderError};

#[derive(Debug)]
pub(crate) struct TextureBook {
    table: TextureTable,
    pub(crate) descriptors: DescriptorAllocator,
}

impl TextureBook {
    pub(crate) fn new(descriptors: DescriptorAllocator) -> Self {
        Self {
            table: TextureTable::new(),
            descriptors,
        }
    }
    pub(crate) fn contains(&self, key: TextureKey) -> bool {
        self.table.contains(key)
    }
    pub(crate) fn get(&mut self, key: TextureKey) -> Option<u32> {
        self.table.get(key)
    }
    pub(crate) fn insert(&mut self, key: TextureKey, slot: u32) {
        self.table.insert(key, slot);
    }
    pub(crate) fn add_ref(&mut self, slot: u32) -> Option<u32> {
        self.table.add_ref(slot)
    }
    pub(crate) fn release(&mut self, slot: u32) -> Released {
        self.table.release(slot)
    }
    pub(crate) fn keys(&self) -> Vec<TextureKey> {
        self.table.keys()
    }
    pub(crate) fn stats(&self) -> TextureTableStats {
        self.table.stats()
    }
    pub(crate) fn len(&self) -> usize {
        self.table.len()
    }

    #[cfg(any(test, feature = "vulkan", all(windows, feature = "d3d12")))]
    pub(crate) fn waiting_for_capacity(&self) -> bool {
        self.descriptors.free_slots() == 0
            && self.descriptors.frontier() >= self.descriptors.capacity()
            && self.descriptors.pending_slots() > 0
    }

    pub(crate) fn exhausted(&self, wait: &str) -> RenderError {
        RenderError::Device(format!(
            "descriptor heap exhausted: {} of {} slots taken, {} live, {} waiting on the {} \
             ({} refusals so far)",
            self.descriptors.frontier(),
            self.descriptors.capacity(),
            self.descriptors.live(),
            self.descriptors.pending_slots(),
            wait,
            self.descriptors.stats().exhaustions,
        ))
    }

    pub(crate) fn usage(&self, deferred: u32) -> DescriptorUsage {
        DescriptorUsage {
            live: self.descriptors.live(),
            frontier: self.descriptors.frontier(),
            high_water: self.descriptors.high_water(),
            free: self.descriptors.free_slots(),
            pending: self.descriptors.pending_slots(),
            deferred,
            capacity: self.descriptors.capacity(),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (shared bookkeeping leaves release completion to its caller).
    use super::*;

    #[test]
    fn cached_links_and_deferred_slots_remain_distinct_until_completion() {
        let mut book = TextureBook::new(DescriptorAllocator::new(2, 2));
        let key = TextureKey::world(41);
        let slot = book.descriptors.alloc().unwrap();
        book.insert(key, slot);
        assert_eq!(book.get(key), Some(slot));
        assert_ne!(book.release(slot), Released::Freed);
        assert_eq!(book.release(slot), Released::Freed);
        assert!(!book.contains(key));
        assert_eq!(book.usage(1).live, 1);
        assert_eq!(book.usage(1).deferred, 1);
        assert!(!book.waiting_for_capacity());
        book.descriptors.release(slot, 7);
        assert!(book.waiting_for_capacity());
        book.descriptors.retire(6);
        assert!(book.descriptors.alloc().is_none());
        let RenderError::Device(message) = book.exhausted("fence") else {
            panic!()
        };
        assert_eq!(message, "descriptor heap exhausted: 1 of 1 slots taken, 0 live, 1 waiting on the fence (1 refusals so far)");
        book.descriptors.retire(7);
        assert_eq!(book.descriptors.alloc(), Some(slot));
        assert_eq!(book.stats().hits, 1);
    }
}
