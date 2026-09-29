//! `LayoutDesc` — a screen's worth of element descriptions, and partial resolution
//! (`inq_full_desc`).
//!
//! Layout-description decoding, including element and state lookup.

use std::collections::BTreeMap;

use dereth_primitives::{AssetSource, DataId};

use crate::desc::cache::DescLibrary;
use crate::desc::element_desc::ElementDesc;
use crate::desc::incorporate::incorporate;
use crate::{ElementId, UiError};

pub use dereth_assets::ui::PropertyTypes;

/// A layout description with a dat-object header. Its dat-object type is **0x23**.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutDesc {
    pub did: DataId,
    /// The display width — the resolution the layout was authored for. **800** in every retail file.
    pub display_width: i32,
    /// The display height — **600** in all but `0x21000066`, `0x21000067` and `0x21000069`, which
    /// are 500.
    pub display_height: i32,
    /// The top-level elements, keyed by element id.
    pub elements: BTreeMap<ElementId, ElementDesc>,
}

impl LayoutDesc {
    /// The UI-layout dat-object type.
    pub const DBO_TYPE: u32 = 0x23;
    /// The `Archive` version token `'UIL '` expected by the layout decoder.
    pub const VERSION_TOKEN: u32 = 0x5549_4C20;
    /// The UI layout is the only dat type whose pack version is 3.
    pub const PACK_VERSION: u32 = 3;

    /// Read one layout out of its dat payload.
    ///
    /// This takes more than the payload bytes because a bare `read(payload: &[u8])` cannot work:
    /// property serialization writes only the property id and the raw value, so the stream is
    /// undecodable without the `MasterProperty` (`0x39000001`) id→type table. The byte-level
    /// decode itself is the asset crate's layout payload decoder, which consumes the payload to its exact end
    /// and is already proved against all 101 shipped objects.
    pub fn read(did: DataId, payload: &[u8], types: &PropertyTypes) -> Result<Self, UiError> {
        let raw =
            dereth_assets::ui::LayoutDesc::decode_payload(did, payload, types).map_err(|e| {
                UiError::Layout {
                    did,
                    reason: e.to_string(),
                }
            })?;
        Ok(Self::from_asset(&raw))
    }

    /// Load a layout through the asset seam using qualified data type `0x23`.
    pub fn load(
        assets: &dyn AssetSource,
        did: DataId,
        types: &PropertyTypes,
    ) -> Result<Self, UiError> {
        let bytes = assets.read(did).map_err(|e| UiError::Layout {
            did,
            reason: format!("asset source: {e}"),
        })?;
        Self::read(did, &bytes, types)
    }

    #[must_use]
    pub fn from_asset(raw: &dereth_assets::ui::LayoutDesc) -> Self {
        Self {
            did: raw.id,
            display_width: raw.display_width,
            display_height: raw.display_height,
            elements: raw
                .elements
                .iter()
                .map(|(k, v)| (ElementId(*k), ElementDesc::from_asset(v)))
                .collect(),
        }
    }

    /// One top-level element description.
    #[must_use]
    pub fn access_element(&self, id: ElementId) -> Option<&ElementDesc> {
        self.elements.get(&id)
    }

    /// The design reference box for anchoring: `(0, 0, display_width-1, display_height-1)`.
    #[must_use]
    pub fn design_box(&self) -> crate::region::Box2D {
        crate::region::Box2D::new(0, 0, self.display_width - 1, self.display_height - 1)
    }

    /// Total element count including every descendant, for the conformance gate.
    #[must_use]
    pub fn element_count(&self) -> usize {
        self.elements.values().map(ElementDesc::subtree_len).sum()
    }

    /// Resolve a partial description against its base.
    ///
    /// The spec's signature. It runs with a throwaway cache; the manager uses
    /// [`inq_full_desc_cached`](Self::inq_full_desc_cached) so that `ItemSlot` and `Dialog` are
    /// resolved once rather than hundreds of times.
    pub fn inq_full_desc(
        &self,
        assets: &dyn AssetSource,
        types: &PropertyTypes,
        partial: &ElementDesc,
    ) -> Result<ElementDesc, UiError> {
        let mut lib = DescLibrary::new();
        lib.insert(self.clone());
        self.inq_full_desc_cached(assets, types, &mut lib, partial)
    }

    /// Partial resolution with the shared layout store and the shared description cache.
    ///
    /// A description with a nonzero type is already complete. Otherwise the key is
    /// `(base_layout, base_element)`: a cached resolution is used directly; if not cached, the base
    /// layout is loaded, the base element is looked up in it, a key already seen on this walk is a
    /// cycle and fails, and the base is resolved recursively and cached. Either way the result is
    /// the base description with the partial incorporated over it.
    pub fn inq_full_desc_cached(
        &self,
        assets: &dyn AssetSource,
        types: &PropertyTypes,
        lib: &mut DescLibrary,
        partial: &ElementDesc,
    ) -> Result<ElementDesc, UiError> {
        let mut visited: Vec<(DataId, ElementId)> = Vec::new();
        self.resolve(assets, types, lib, partial, &mut visited)
    }

    fn resolve(
        &self,
        assets: &dyn AssetSource,
        types: &PropertyTypes,
        lib: &mut DescLibrary,
        partial: &ElementDesc,
        visited: &mut Vec<(DataId, ElementId)>,
    ) -> Result<ElementDesc, UiError> {
        // "already complete" — the overwhelming majority of nodes.
        if !partial.is_partial() {
            return Ok(partial.clone());
        }
        let key = (partial.base_layout, partial.base_element);

        let mut full = if let Some(hit) = lib.cached(key) {
            let hit = hit.clone();
            lib.note_hit();
            hit
        } else {
            // The cycle guard: a key already visited on this walk.
            if visited.contains(&key) {
                return Err(UiError::DescCycle {
                    layout: partial.base_layout,
                    element: partial.base_element,
                });
            }
            visited.push(key);

            if !lib.contains(partial.base_layout) {
                let l = Self::load(assets, partial.base_layout, types)?;
                lib.insert(l);
            }
            let base_desc = lib
                .get(partial.base_layout)
                .and_then(|l| l.access_element(partial.base_element))
                .cloned()
                .ok_or(UiError::MissingBase {
                    layout: partial.base_layout,
                    element: partial.base_element,
                })?;

            // The base layout resolves its own partials against itself.
            let base_layout =
                lib.get(partial.base_layout)
                    .cloned()
                    .ok_or(UiError::MissingBase {
                        layout: partial.base_layout,
                        element: partial.base_element,
                    })?;
            let resolved = base_layout.resolve(assets, types, lib, &base_desc, visited)?;
            lib.cache_put(key, resolved.clone());
            resolved
        };

        incorporate(&mut full, partial);
        Ok(full)
    }
}
