// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Equipment.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Equipment.cs`.

use std::sync::Arc;

use empyrean_common::dotnet::CsCast;
use empyrean_common::extensions::float_extensions;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::treasure_wielded::TreasureWielded;
use empyrean_entity::enums::DestinationType;
use empyrean_entity::models::PropertiesCreateList;
use empyrean_entity::{ObjectGuid, Weenie};

use crate::factories::world_object_factory;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// A wielded item a creature holds as a physics child (`Entity/HeldItem.cs`).
pub use crate::entity::held_item::HeldItem;

/// Non-property fields declared in `WorldObject_Equipment.cs`: none.
#[derive(Debug, Default)]
pub struct WorldObjectEquipmentFields {}

/// `WorldObjectFactory.CreateNewWorldObject(uint weenieClassId)` with the guid from
/// `GuidManager.NewDynamicGuid`: the cached weenie, a new dynamic guid, construction; the guid is
/// recycled when construction fails. The object is returned outside `World.objects`.
pub(crate) fn create_new_world_object_by_wcid(
    w: &mut World,
    weenie_class_id: u32,
) -> Option<WorldObject> {
    world_object_factory::create_new_world_object_by_wcid_in_world(w, weenie_class_id)
}

/// `WorldObjectFactory.CreateNewWorldObject(Weenie weenie)`, as above.
pub(crate) fn create_new_world_object(w: &mut World, weenie: Arc<Weenie>) -> Option<WorldObject> {
    world_object_factory::create_new_world_object_in_world(w, weenie)
}

/// `WorldObjectFactory.CreateNewWorldObject(PropertiesCreateList item)`, as above.
pub(crate) fn create_new_world_object_from_create_list(
    w: &mut World,
    item: &PropertiesCreateList,
) -> Option<WorldObject> {
    world_object_factory::create_new_world_object_from_create_list_in_world(w, item)
}

/// The new objects of the create-list entries with `type`. ACE hands them to the slum lord without
/// placing them anywhere, so they are returned outside `World.objects`. `None` is a `null` entry
/// (a weenie that does not exist, on an entry with no palette, shade or stack size).
///
/// # Panics
/// Where ACE throws `NullReferenceException`: a biota with no create list, or a missing weenie on
/// an entry that sets a palette, a shade or a stack size.
// ACE: WorldObject.GetCreateListForSlumLord
// ACE-BUG: a null PropertiesCreateList, and a missing create-list weenie (CreateNewWorldObject returns null) on an entry with a palette, shade or stack size, throw NullReferenceException; a missing weenie on any other entry puts null in the list.
pub fn get_create_list_for_slum_lord(
    w: &mut World,
    this: ObjectGuid,
    r#type: DestinationType,
) -> Vec<Option<WorldObject>> {
    let create_list = w
        .objects
        .get(this)
        .and_then(|o| o.biota.properties_create_list.clone())
        .expect("System.NullReferenceException: Biota.PropertiesCreateList");

    let mut items = Vec::new();

    for item in create_list.iter().filter(|x| x.destination_type == r#type) {
        let mut wo = create_new_world_object_by_wcid(w, item.weenie_class_id);

        if item.palette > 0 {
            let wo = wo.as_mut().expect("System.NullReferenceException: wo");
            wo.set_palette_template(Some(i32::from(item.palette)));
        }

        if item.shade > 0.0 {
            let wo = wo.as_mut().expect("System.NullReferenceException: wo");
            wo.set_shade(Some(f64::from(item.shade)));
        }

        if item.stack_size > 0 {
            // `wo is Stackable` is false for null, so a missing weenie reaches the setter.
            let wo = wo.as_mut().expect("System.NullReferenceException: wo");
            if wo.is_stackable() {
                wo.set_stack_size(Some(item.stack_size));
            } else {
                wo.set_stack_size_prop(Some(item.stack_size)); // item isn't a stackable object, but we want multiples of it while not displaying multiple single items in the profile. Munge stacksize to get us there.
            }
        }

        items.push(wo);
    }
    items
}

/// Rolls a wielded-treasure table: `None` when nothing was generated (ACE's `null` list). The
/// objects are returned outside `World.objects`.
// ACE: WorldObject.GenerateWieldedTreasureSets
pub fn generate_wielded_treasure_sets(
    w: &mut World,
    items: &[TreasureWielded],
) -> Option<Vec<WorldObject>> {
    let mut cur_idx: i32 = 0;
    let mut results = None;
    generate_wielded_treasure_sets_ref(w, items, &mut results, &mut cur_idx, false);
    results
}

/// The private recursive overload (`ref results`, `ref curIdx`, `skip`).
// ACE: WorldObject.GenerateWieldedTreasureSets
fn generate_wielded_treasure_sets_ref(
    w: &mut World,
    items: &[TreasureWielded],
    results: &mut Option<Vec<WorldObject>>,
    cur_idx: &mut i32,
    skip: bool,
) {
    let mut rng = ThreadSafeRandom::next_float(0.0, 1.0);
    let mut probability = 0.0f32;
    let mut rolled = false;
    let mut continued = false;

    let count = i32::try_from(items.len()).unwrap_or(i32::MAX);
    while *cur_idx < count {
        let item = &items[usize::try_from(*cur_idx).expect("curIdx >= 0")];

        if item.continues_previous_set {
            if !continued {
                *cur_idx -= 1;
                return;
            }
            continued = false;
        }

        let mut skip_next = true;

        if !skip {
            if item.set_start || probability >= 1.0 {
                rng = ThreadSafeRandom::next_float(0.0, 1.0);
                probability = 0.0;
                rolled = false;
            }

            probability += item.probability;

            if rng < f64::from(probability) && !rolled {
                rolled = true;
                skip_next = false;

                // item roll successful, add to generated list
                let wo = create_wielded_treasure(w, item);

                if let Some(wo) = wo {
                    results.get_or_insert_with(Vec::new).push(wo);
                }
            }
        }

        if item.has_sub_set {
            *cur_idx += 1;
            generate_wielded_treasure_sets_ref(w, items, results, cur_idx, skip_next);
            continued = true;
        }

        *cur_idx += 1;
    }
}

/// Creates one wielded-treasure item (outside `World.objects`), or `None` for a missing weenie.
///
/// # Panics
/// When a negative stack size variance puts the minimum stack above the maximum: `Random.Next`
/// throws `ArgumentOutOfRangeException`.
// ACE: WorldObject.CreateWieldedTreasure
pub fn create_wielded_treasure(w: &mut World, item: &TreasureWielded) -> Option<WorldObject> {
    let mut wo = create_new_world_object_by_wcid(w, item.weenie_class_id)?;

    if item.palette_id > 0 {
        let palette: i32 = item.palette_id.cs_cast();
        wo.set_palette_template(Some(palette));
    }

    if item.shade > 0.0 {
        wo.set_shade(Some(f64::from(item.shade)));
    }

    if item.stack_size > 0 {
        let mut stack_size = item.stack_size;

        let has_variance = item.stack_size_variance > 0.0;
        if has_variance {
            #[allow(clippy::cast_precision_loss)] // C#'s int * float
            let scaled = item.stack_size as f32 * (1.0f32 - item.stack_size_variance);
            let min_stack = std::cmp::max(1, float_extensions::round(scaled, 0));
            let max_stack = item.stack_size;
            stack_size = ThreadSafeRandom::next(min_stack, max_stack);
        }
        wo.set_stack_size(Some(stack_size));
    }
    Some(wo)
}

// ---- virtual-dispatch targets: each `not_ported!` until it is ported ----

// ACE: WorldObject.OnWield
#[allow(unused_variables)]
pub fn world_object_on_wield(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    creature: empyrean_entity::ObjectGuid,
) {
    // `EmoteManager.OnWield(creature);`
    crate::world_objects::managers::emote_manager::on_wield(w, this, creature);
}

// ACE: WorldObject.OnUnWield
#[allow(unused_variables)]
pub fn world_object_on_un_wield(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    creature: empyrean_entity::ObjectGuid,
) {
    // `EmoteManager.OnUnwield(creature);`
    crate::world_objects::managers::emote_manager::on_unwield(w, this, creature);
}
