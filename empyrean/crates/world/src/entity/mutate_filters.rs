// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/MutateFilters.cs
//! Port of `Source/ACE.Server/Entity/MutateFilters.cs`.

use empyrean_entity::enums::MutateFilter;

use crate::world_objects::world_object::WorldObject;

// ACE: MutateFilters.Filters, MutateFilters.MutateFilters
/// The mutate filters are the 0x0E records: only 6 mutate filter ids total in 16py?
#[must_use]
pub fn filters(id: u32) -> Option<MutateFilter> {
    match id {
        // armor
        0x0E00_0012 => {
            Some(MutateFilter::Base | MutateFilter::ArmorModVsType | MutateFilter::EncumbranceVal)
        }
        // shields
        0x0E00_0013 => {
            Some(MutateFilter::Base | MutateFilter::ArmorModVsType | MutateFilter::ShieldValue)
        }
        // weapons - exactly the same?
        0x0E00_0014 | 0x0E00_0015 | 0x0E00_001D => {
            Some(MutateFilter::Base | MutateFilter::WeaponTime)
        }
        0x0E00_0016 => Some(MutateFilter::Base),
        _ => None,
    }
}

// ACE: MutateFilters.GetMutateFilters
#[must_use]
pub fn get_mutate_filters(wo: &WorldObject) -> MutateFilter {
    let Some(mutate_filter) = wo.mutate_filter() else {
        return MutateFilter::Undef;
    };

    //Console.WriteLine($"Unknown MutateFilter {wo.MutateFilter:X8} on {wo.Name} ({wo.WeenieClassId})");
    filters(mutate_filter).unwrap_or(MutateFilter::Undef)
}

// ACE: MutateFilters.HasMutateFilter
#[must_use]
pub fn has_mutate_filter(wo: &WorldObject, filter: MutateFilter) -> bool {
    let Some(mutate_filter) = wo.mutate_filter() else {
        return false;
    };

    filters(mutate_filter).is_some_and(|filters| (filters & filter) == filter)
}
