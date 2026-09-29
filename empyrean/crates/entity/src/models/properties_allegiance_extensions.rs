// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesAllegianceExtensions.cs
//! `PropertiesAllegianceExtensions`: helpers over a biota's
//! `IDictionary<uint /* Character ID */, PropertiesAllegiance>`, which may be `null` (`None`).
//! ACE's `ReaderWriterLockSlim` parameter is dropped (see [`crate::models`]).

use empyrean_common::dotnet::DotNetDict;

use crate::models::properties_allegiance::PropertiesAllegiance;

/// `Where(...).ToDictionary(...)`: the matching entries in enumeration order. ACE's result
/// shares the record objects; these are copies.
fn filter(
    value: &DotNetDict<u32, PropertiesAllegiance>,
    predicate: impl Fn(&PropertiesAllegiance) -> bool,
) -> DotNetDict<u32, PropertiesAllegiance> {
    let mut result = DotNetDict::new();
    for (k, v) in value.iter() {
        if predicate(v) {
            result.add(*k, v.clone());
        }
    }
    result
}

/// The approved vassals; an empty dictionary for a null one.
// ACE: PropertiesAllegianceExtensions.GetApprovedVassals
#[must_use]
pub fn get_approved_vassals(
    value: Option<&DotNetDict<u32, PropertiesAllegiance>>,
) -> DotNetDict<u32, PropertiesAllegiance> {
    let Some(value) = value else {
        return DotNetDict::new();
    };

    filter(value, |i| i.approved_vassal)
}

/// The banned characters; an empty dictionary for a null one.
// ACE: PropertiesAllegianceExtensions.GetBanList
#[must_use]
pub fn get_ban_list(
    value: Option<&DotNetDict<u32, PropertiesAllegiance>>,
) -> DotNetDict<u32, PropertiesAllegiance> {
    let Some(value) = value else {
        return DotNetDict::new();
    };

    filter(value, |i| i.banned)
}

/// The record for `character_id`, if any.
// ACE: PropertiesAllegianceExtensions.GetFirstOrDefaultByCharacterId
#[must_use]
pub fn get_first_or_default_by_character_id(
    value: Option<&DotNetDict<u32, PropertiesAllegiance>>,
    character_id: u32,
) -> Option<&PropertiesAllegiance> {
    value?.get(&character_id)
}

/// Adds or updates the record for `character_id`. The dictionary must exist, as in ACE.
// ACE: PropertiesAllegianceExtensions.AddOrUpdateAllegiance
pub fn add_or_update_allegiance(
    value: &mut DotNetDict<u32, PropertiesAllegiance>,
    character_id: u32,
    is_banned: bool,
    approved_vassal: bool,
) {
    if !value.contains_key(&character_id) {
        let entity = PropertiesAllegiance {
            banned: is_banned,
            approved_vassal,
        };

        value.add(character_id, entity);
    }

    let entity = value
        .get_mut(&character_id)
        .expect("present: found or just added");
    entity.banned = is_banned;
    entity.approved_vassal = approved_vassal;
}

/// Removes the record for `character_id`; false if absent or the dictionary is null.
// ACE: PropertiesAllegianceExtensions.TryRemoveAllegiance
pub fn try_remove_allegiance(
    value: Option<&mut DotNetDict<u32, PropertiesAllegiance>>,
    character_id: u32,
) -> bool {
    let Some(value) = value else { return false };

    value.remove(&character_id).is_some()
}
