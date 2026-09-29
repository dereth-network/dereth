//! The resolved-description cache.
//!
//! A two-level layout-id and element-id cache, guarded by the cache-enabled flag, memoises
//! resolved base descriptions per `(layout, element)` pair and flushed when the layout is
//! destroyed.
//!
//! It matters: `classic_*` layouts reference `ItemSlot` (`0x21000037`) and `Dialog`
//! (`0x2100003C`) hundreds of times, and 1088 of the 2162 shipped elements are pure overlays.

use std::collections::BTreeMap;

use dereth_primitives::DataId;

use crate::desc::element_desc::ElementDesc;
use crate::desc::layout_desc::LayoutDesc;
use crate::ElementId;

/// The loaded-layout store plus the resolved-description cache.
///
/// In the client the layout objects live in the asset cache and the desc cache is file-static; both are
/// process-wide. Here they are one owned object so that a test can build a library with no dat at
/// all, which is what `crate::desc::tests` and the widget tests do.
#[derive(Debug, Default)]
pub struct DescLibrary {
    layouts: BTreeMap<DataId, LayoutDesc>,
    /// The resolved-description cache, keyed by base layout and then base element.
    cache: BTreeMap<(DataId, ElementId), ElementDesc>,
    /// Whether the description cache is used at all.
    pub use_cache: bool,
    /// Cache hits, for the conformance test's report. Not in the original.
    pub hits: u64,
}

impl DescLibrary {
    #[must_use]
    pub fn new() -> Self {
        Self {
            use_cache: true,
            ..Self::default()
        }
    }

    pub fn insert(&mut self, l: LayoutDesc) {
        self.layouts.insert(l.did, l);
    }

    #[must_use]
    pub fn get(&self, did: DataId) -> Option<&LayoutDesc> {
        self.layouts.get(&did)
    }

    #[must_use]
    pub fn contains(&self, did: DataId) -> bool {
        self.layouts.contains_key(&did)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.layouts.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&DataId, &LayoutDesc)> {
        self.layouts.iter()
    }

    #[must_use]
    pub fn cached(&self, key: (DataId, ElementId)) -> Option<&ElementDesc> {
        if !self.use_cache {
            return None;
        }
        self.cache.get(&key)
    }

    pub fn cache_put(&mut self, key: (DataId, ElementId), d: ElementDesc) {
        if self.use_cache {
            self.cache.insert(key, d);
        }
    }

    pub fn note_hit(&mut self) {
        self.hits += 1;
    }

    /// Destroying a layout flushes the cache.
    pub fn flush_cache(&mut self) {
        self.cache.clear();
    }
}
