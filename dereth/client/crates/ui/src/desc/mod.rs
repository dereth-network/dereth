//! The description model: `LayoutDesc` / `ElementDesc` / `StateDesc`, incorporation and the cache.
//!
//! The description tree is **data**; the element tree is code. Everything an
//! element does that is not code — its picture, its font, its tooltip, whether it can be dragged,
//! what sound it plays when clicked — is a property here.

pub mod cache;
pub mod element_desc;
pub mod incorporate;
pub mod layout_desc;
pub mod state_desc;

pub use cache::DescLibrary;
pub use element_desc::ElementDesc;
pub use incorporate::{incorporate, update_desc_tree};
pub use layout_desc::{LayoutDesc, PropertyTypes};
pub use state_desc::{incorporation, MediaDesc, MediaFields, StateDesc};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use dereth_primitives::{AssetError, AssetSource, DataId, DataType};

    use super::*;
    use crate::props::PropertyValue;
    use crate::{ElementId, ElementType, StateId, UiError};

    /// An asset source that refuses everything, for tests whose layouts are all in the library.
    #[derive(Debug)]
    struct NoAssets;
    impl AssetSource for NoAssets {
        fn read(&self, id: DataId) -> Result<Vec<u8>, AssetError> {
            Err(AssetError::NotFound(id))
        }
        fn exists(&self, _: DataId) -> bool {
            false
        }
        fn iter_type(&self, _: DataType) -> Box<dyn Iterator<Item = DataId> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn concrete(ty: u32, w: i32, h: i32) -> ElementDesc {
        ElementDesc {
            ty: ElementType(ty),
            base: StateDesc {
                incorporation: incorporation::LEGACY_ALL_GEOMETRY,
                width: w,
                height: h,
                ..StateDesc::default()
            },
            ..ElementDesc::default()
        }
    }

    fn partial_of(layout: u32, element: u32) -> ElementDesc {
        ElementDesc {
            ty: ElementType(0),
            base_layout: DataId(layout),
            base_element: ElementId(element),
            ..ElementDesc::default()
        }
    }

    fn layout(did: u32, elements: Vec<(u32, ElementDesc)>) -> LayoutDesc {
        LayoutDesc {
            did: DataId(did),
            display_width: 800,
            display_height: 600,
            elements: elements
                .into_iter()
                .map(|(k, v)| (ElementId(k), v))
                .collect(),
        }
    }

    /// a partial inherits the base's type and
    /// overlays its own deltas.
    #[test]
    fn a_partial_resolves_up_the_chain_and_overlays() {
        let types: PropertyTypes = BTreeMap::new();
        let mut lib = DescLibrary::new();
        let mut base = concrete(3, 100, 40);
        base.base.properties.set(0x3B, PropertyValue::Bool(true));
        lib.insert(layout(0x2100_0037, vec![(0x10, base)]));

        let mut p = partial_of(0x2100_0037, 0x10);
        p.element_id = ElementId(0x1000_0001);
        p.base.incorporation = incorporation::WIDTH;
        p.base.width = 250;
        p.base.properties.set(0x4D, PropertyValue::Float(0.5));

        let here = layout(0x2100_0005, vec![(0x1000_0001, p.clone())]);
        let full = here
            .inq_full_desc_cached(&NoAssets, &types, &mut lib, &p)
            .unwrap();
        assert_eq!(full.ty, ElementType(3));
        assert_eq!(full.element_id, ElementId(0x1000_0001));
        assert_eq!(full.base.width, 250);
        assert_eq!(
            full.base.height, 40,
            "inherited: no height bit on the partial"
        );
        assert_eq!(full.base.properties.get_bool(0x3B), Some(true));
        assert_eq!(full.base.properties.get_float(0x4D), Some(0.5));
    }

    /// Oracle: the unique add into the base-info list, the cycle guard in `inq_full_desc`.
    #[test]
    fn a_cycle_is_rejected_rather_than_recursing_forever() {
        let types: PropertyTypes = BTreeMap::new();
        let mut lib = DescLibrary::new();
        lib.insert(layout(0x2100_0001, vec![(1, partial_of(0x2100_0002, 2))]));
        lib.insert(layout(0x2100_0002, vec![(2, partial_of(0x2100_0001, 1))]));
        let here = lib.get(DataId(0x2100_0001)).unwrap().clone();
        let start = here.access_element(ElementId(1)).unwrap().clone();
        let e = here
            .inq_full_desc_cached(&NoAssets, &types, &mut lib, &start)
            .unwrap_err();
        assert!(matches!(e, UiError::DescCycle { .. }), "got {e:?}");
    }

    /// "memoises resolved base descriptions per (layout, element)".
    /// `ItemSlot` is referenced by hundreds of shipped elements, which is why it exists.
    #[test]
    fn the_desc_cache_answers_the_second_reference_to_the_same_base() {
        let types: PropertyTypes = BTreeMap::new();
        let mut lib = DescLibrary::new();
        lib.insert(layout(0x2100_0037, vec![(0x10, concrete(3, 10, 10))]));
        let here = layout(0x2100_0005, vec![]);
        let p = partial_of(0x2100_0037, 0x10);
        let _ = here
            .inq_full_desc_cached(&NoAssets, &types, &mut lib, &p)
            .unwrap();
        assert_eq!(lib.hits, 0);
        let _ = here
            .inq_full_desc_cached(&NoAssets, &types, &mut lib, &p)
            .unwrap();
        assert_eq!(lib.hits, 1);
        lib.flush_cache();
        let _ = here
            .inq_full_desc_cached(&NoAssets, &types, &mut lib, &p)
            .unwrap();
        assert_eq!(lib.hits, 1, "a flushed cache resolves from scratch again");
    }

    /// A missing base element reports an error rather than an empty description.
    #[test]
    fn a_missing_base_element_is_an_error_not_a_silent_empty_desc() {
        let types: PropertyTypes = BTreeMap::new();
        let mut lib = DescLibrary::new();
        lib.insert(layout(0x2100_0037, vec![]));
        let here = layout(0x2100_0005, vec![]);
        let p = partial_of(0x2100_0037, 0x99);
        let e = here
            .inq_full_desc_cached(&NoAssets, &types, &mut lib, &p)
            .unwrap_err();
        assert!(matches!(e, UiError::MissingBase { .. }), "got {e:?}");
    }

    /// Oracle: "returns NULL for the default state 0, in which case
    /// the element uses the `ElementDesc`'s own `StateDesc` base".
    #[test]
    fn state_zero_is_the_elements_own_base_state() {
        let mut d = concrete(3, 10, 10);
        d.states.insert(
            StateId(0),
            StateDesc {
                state_id: StateId(0),
                ..StateDesc::default()
            },
        );
        d.states.insert(
            StateId(4),
            StateDesc {
                state_id: StateId(4),
                ..StateDesc::default()
            },
        );
        assert!(d.access_state(StateId(0)).is_none());
        assert!(d.access_state(StateId(4)).is_some());
    }
}
