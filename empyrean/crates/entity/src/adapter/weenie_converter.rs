// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Adapter/WeenieConverter.cs
//! `WeenieConverter`: a new [`Biota`] from a [`Weenie`].
//!
//! Every collection is copied (records through their ACE `Clone()`), except that with
//! `reference_weenie_collections_for_common_properties` the create list, emotes, event filter,
//! generator profiles and body parts are shared with the weenie. ACE shares the very objects;
//! here the biota gets a clone of the weenie's `Arc`, and writes go through the biota's `*_mut`
//! accessors, which copy on write, so a biota never changes the cached weenie (see
//! [`crate::models`]).
//!
//! Dictionary copies are ACE's `new Dictionary<K, V>(source)` (or `Add` in source order): the
//! entries keep their enumeration order and any free slots of the source are compacted away.

use std::sync::Arc;

use empyrean_common::dotnet::DotNetDict;

use crate::models::{Biota, Weenie};

/// `new Dictionary<K, V>(source)`: same enumeration order, no free slots.
fn copy_dict<K: std::hash::Hash + Eq + Clone, V: Clone>(
    source: &DotNetDict<K, V>,
) -> DotNetDict<K, V> {
    source.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Converts `weenie` into a new biota with id `id`.
///
/// * `instantiate_empty_collections`: an empty (non-null) weenie collection gives an empty biota
///   collection instead of a null one.
/// * `reference_weenie_collections_for_common_properties`: share the common-property
///   collections (create list, emotes, event filter, generators, body parts) instead of copying.
///   Shared collections are passed through even when null or empty.
// ACE: WeenieConverter.ConvertToBiota
#[must_use]
pub fn convert_to_biota(
    weenie: &Weenie,
    id: u32,
    instantiate_empty_collections: bool,
    reference_weenie_collections_for_common_properties: bool,
) -> Biota {
    let mut result = Biota {
        id,
        weenie_class_id: weenie.weenie_class_id,
        weenie_type: weenie.weenie_type,
        ..Biota::default()
    };

    let keep = |len: usize| instantiate_empty_collections || len > 0;

    if let Some(v) = weenie.properties_bool.as_ref().filter(|v| keep(v.len())) {
        result.properties_bool = Some(copy_dict(v));
    }
    if let Some(v) = weenie.properties_did.as_ref().filter(|v| keep(v.len())) {
        result.properties_did = Some(copy_dict(v));
    }
    if let Some(v) = weenie.properties_float.as_ref().filter(|v| keep(v.len())) {
        result.properties_float = Some(copy_dict(v));
    }
    if let Some(v) = weenie.properties_iid.as_ref().filter(|v| keep(v.len())) {
        result.properties_iid = Some(copy_dict(v));
    }
    if let Some(v) = weenie.properties_int.as_ref().filter(|v| keep(v.len())) {
        result.properties_int = Some(copy_dict(v));
    }
    if let Some(v) = weenie.properties_int64.as_ref().filter(|v| keep(v.len())) {
        result.properties_int64 = Some(copy_dict(v));
    }
    if let Some(v) = weenie.properties_string.as_ref().filter(|v| keep(v.len())) {
        result.properties_string = Some(copy_dict(v));
    }

    if let Some(v) = weenie
        .properties_position
        .as_ref()
        .filter(|v| keep(v.len()))
    {
        let mut d = DotNetDict::new();

        for (k, value) in v.iter() {
            d.add(*k, value.ace_clone());
        }
        result.properties_position = Some(d);
    }

    if let Some(v) = weenie
        .properties_spell_book
        .as_ref()
        .filter(|v| keep(v.len()))
    {
        result.properties_spell_book = Some(copy_dict(v));
    }

    if let Some(v) = weenie
        .properties_anim_part
        .as_ref()
        .filter(|v| keep(v.len()))
    {
        result.properties_anim_part = Some(v.iter().map(|record| record.ace_clone()).collect());
    }

    if let Some(v) = weenie.properties_palette.as_ref().filter(|v| keep(v.len())) {
        result.properties_palette = Some(v.iter().map(|record| record.ace_clone()).collect());
    }

    if let Some(v) = weenie
        .properties_texture_map
        .as_ref()
        .filter(|v| keep(v.len()))
    {
        result.properties_texture_map = Some(v.iter().map(|record| record.ace_clone()).collect());
    }

    // Properties for all world objects that typically aren't modified over the original weenie

    if reference_weenie_collections_for_common_properties {
        result.properties_create_list = weenie.properties_create_list.clone();
        result.properties_emote = weenie.properties_emote.clone();
        result.properties_event_filter = weenie.properties_event_filter.clone();
        result.properties_generator = weenie.properties_generator.clone();
    } else {
        if let Some(v) = weenie
            .properties_create_list
            .as_ref()
            .filter(|v| keep(v.len()))
        {
            result.properties_create_list = Some(Arc::new(
                v.iter().map(|record| record.ace_clone()).collect(),
            ));
        }

        if let Some(v) = weenie.properties_emote.as_ref().filter(|v| keep(v.len())) {
            result.properties_emote = Some(Arc::new(
                v.iter().map(|record| record.ace_clone()).collect(),
            ));
        }

        // `new HashSet<int>(source)` of a HashSet whose buckets are not oversized copies its
        // entries array as is (free slots included), which is what `Clone` does.
        if let Some(v) = weenie
            .properties_event_filter
            .as_ref()
            .filter(|v| keep(v.len()))
        {
            result.properties_event_filter = Some(Arc::new((**v).clone()));
        }

        if let Some(v) = weenie
            .properties_generator
            .as_ref()
            .filter(|v| keep(v.len()))
        {
            result.properties_generator = Some(Arc::new(
                v.iter().map(|record| record.ace_clone()).collect(),
            ));
        }
    }

    // Properties for creatures

    if let Some(v) = weenie
        .properties_attribute
        .as_ref()
        .filter(|v| keep(v.len()))
    {
        let mut d = DotNetDict::new();

        for (k, value) in v.iter() {
            d.add(*k, value.ace_clone());
        }
        result.properties_attribute = Some(d);
    }

    if let Some(v) = weenie
        .properties_attribute_2nd
        .as_ref()
        .filter(|v| keep(v.len()))
    {
        let mut d = DotNetDict::new();

        for (k, value) in v.iter() {
            d.add(*k, value.ace_clone());
        }
        result.properties_attribute_2nd = Some(d);
    }

    if reference_weenie_collections_for_common_properties {
        result.properties_body_part = weenie.properties_body_part.clone();
    } else if let Some(v) = weenie
        .properties_body_part
        .as_ref()
        .filter(|v| keep(v.len()))
    {
        let mut d = DotNetDict::new();

        for (k, value) in v.iter() {
            d.add(*k, value.ace_clone());
        }
        result.properties_body_part = Some(Arc::new(d));
    }

    if let Some(v) = weenie.properties_skill.as_ref().filter(|v| keep(v.len())) {
        let mut d = DotNetDict::new();

        for (k, value) in v.iter() {
            d.add(*k, value.ace_clone());
        }
        result.properties_skill = Some(d);
    }

    // Properties for books

    if let Some(book) = weenie.properties_book.as_ref() {
        result.properties_book = Some(book.ace_clone());
    }

    if let Some(v) = weenie
        .properties_book_page_data
        .as_ref()
        .filter(|v| keep(v.len()))
    {
        result.properties_book_page_data = Some(v.iter().map(|page| page.ace_clone()).collect());
    }

    result
}
