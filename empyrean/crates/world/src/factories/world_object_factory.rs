// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/WorldObjectFactory.cs
//! Port of `Source/ACE.Server/Factories/WorldObjectFactory.cs`.
//!
//! The `WeenieType` switches pick the class; [`WorldObject::from_weenie`] and
//! [`WorldObject::from_biota`] run its constructor chain. Two collaborators are passed in: the world
//! database (`DatabaseManager.World.GetCachedWeenie`, [`CtorEnv::get_cached_weenie`]) and
//! `GuidManager.NewDynamicGuid`, whose guid the caller allocates and passes in.

use std::sync::Arc;

use empyrean_common::extensions::date_time_extensions::to_common_string;
use empyrean_common::time::Time;
use empyrean_content::models::world::LandblockInstance;
use empyrean_entity::enums::{DestinationType, PropertyString, WeenieType};
use empyrean_entity::models::properties_create_list::PropertiesCreateList;
use empyrean_entity::{Biota, ObjectGuid, Position, Weenie};

use crate::dispatch::Class;
use crate::world_objects::world_object::{CtorEnv, WorldObject};

/// The class `CreateWorldObject(Weenie, ObjectGuid)` instantiates for `weenie_type`: `None` for
/// `Undef`, `GenericObject` for every type without a case.
#[must_use]
pub fn weenie_class(weenie_type: WeenieType) -> Option<Class> {
    match weenie_type {
        WeenieType::Undef => None,
        WeenieType::LifeStone => Some(Class::Lifestone),
        WeenieType::Door => Some(Class::Door),
        WeenieType::Portal => Some(Class::Portal),
        WeenieType::Book => Some(Class::Book),
        WeenieType::PKModifier => Some(Class::PKModifier),
        WeenieType::Cow => Some(Class::Cow),
        WeenieType::Creature => Some(Class::Creature),
        WeenieType::Container => Some(Class::Container),
        WeenieType::Scroll => Some(Class::Scroll),
        WeenieType::Vendor => Some(Class::Vendor),
        WeenieType::Coin => Some(Class::Coin),
        WeenieType::Key => Some(Class::Key),
        WeenieType::Food => Some(Class::Food),
        WeenieType::Gem => Some(Class::Gem),
        WeenieType::Game => Some(Class::Game),
        WeenieType::GamePiece => Some(Class::GamePiece),
        WeenieType::AllegianceBindstone => Some(Class::Bindstone),
        WeenieType::Clothing => Some(Class::Clothing),
        WeenieType::MeleeWeapon => Some(Class::MeleeWeapon),
        WeenieType::MissileLauncher => Some(Class::MissileLauncher),
        WeenieType::Ammunition => Some(Class::Ammunition),
        WeenieType::Missile => Some(Class::Missile),
        WeenieType::Corpse => Some(Class::Corpse),
        WeenieType::Chest => Some(Class::Chest),
        WeenieType::Stackable => Some(Class::Stackable),
        WeenieType::SpellComponent => Some(Class::SpellComponent),
        WeenieType::Switch => Some(Class::Switch),
        WeenieType::AdvocateFane => Some(Class::AdvocateFane),
        WeenieType::AdvocateItem => Some(Class::AdvocateItem),
        WeenieType::Healer => Some(Class::Healer),
        WeenieType::Lockpick => Some(Class::Lockpick),
        WeenieType::Caster => Some(Class::Caster),
        WeenieType::ProjectileSpell => Some(Class::SpellProjectile),
        WeenieType::HotSpot => Some(Class::Hotspot),
        WeenieType::ManaStone => Some(Class::ManaStone),
        WeenieType::House => Some(Class::House),
        WeenieType::SlumLord => Some(Class::SlumLord),
        WeenieType::Storage => Some(Class::Storage),
        WeenieType::Hook => Some(Class::Hook),
        WeenieType::Hooker => Some(Class::Hooker),
        WeenieType::HousePortal => Some(Class::HousePortal),
        WeenieType::SkillAlterationDevice => Some(Class::SkillAlterationDevice),
        WeenieType::PressurePlate => Some(Class::PressurePlate),
        WeenieType::PetDevice => Some(Class::PetDevice),
        WeenieType::Pet => Some(Class::Pet),
        WeenieType::CombatPet => Some(Class::CombatPet),
        WeenieType::Allegiance => Some(Class::Allegiance),
        WeenieType::AugmentationDevice => Some(Class::AugmentationDevice),
        WeenieType::AttributeTransferDevice => Some(Class::AttributeTransferDevice),
        WeenieType::CraftTool => Some(Class::CraftTool),
        WeenieType::LightSource => Some(Class::LightSource),
        _ => Some(Class::GenericObject),
    }
}

/// The class `CreateWorldObject(Biota)` instantiates for `weenie_type`. Unlike the weenie switch
/// it has no `ProjectileSpell` case, so a spell projectile restored from a biota becomes a
/// `GenericObject` (ACE never saves spell projectiles).
#[must_use]
pub fn biota_class(weenie_type: WeenieType) -> Option<Class> {
    match weenie_type {
        WeenieType::Undef => None,
        WeenieType::LifeStone => Some(Class::Lifestone),
        WeenieType::Door => Some(Class::Door),
        WeenieType::Portal => Some(Class::Portal),
        WeenieType::Book => Some(Class::Book),
        WeenieType::PKModifier => Some(Class::PKModifier),
        WeenieType::Cow => Some(Class::Cow),
        WeenieType::Creature => Some(Class::Creature),
        WeenieType::Container => Some(Class::Container),
        WeenieType::Scroll => Some(Class::Scroll),
        WeenieType::Vendor => Some(Class::Vendor),
        WeenieType::Coin => Some(Class::Coin),
        WeenieType::Key => Some(Class::Key),
        WeenieType::Food => Some(Class::Food),
        WeenieType::Gem => Some(Class::Gem),
        WeenieType::Game => Some(Class::Game),
        WeenieType::GamePiece => Some(Class::GamePiece),
        WeenieType::AllegianceBindstone => Some(Class::Bindstone),
        WeenieType::Clothing => Some(Class::Clothing),
        WeenieType::MeleeWeapon => Some(Class::MeleeWeapon),
        WeenieType::MissileLauncher => Some(Class::MissileLauncher),
        WeenieType::Ammunition => Some(Class::Ammunition),
        WeenieType::Missile => Some(Class::Missile),
        WeenieType::Corpse => Some(Class::Corpse),
        WeenieType::Chest => Some(Class::Chest),
        WeenieType::Stackable => Some(Class::Stackable),
        WeenieType::SpellComponent => Some(Class::SpellComponent),
        WeenieType::Switch => Some(Class::Switch),
        WeenieType::AdvocateFane => Some(Class::AdvocateFane),
        WeenieType::AdvocateItem => Some(Class::AdvocateItem),
        WeenieType::Healer => Some(Class::Healer),
        WeenieType::Lockpick => Some(Class::Lockpick),
        WeenieType::Caster => Some(Class::Caster),
        WeenieType::HotSpot => Some(Class::Hotspot),
        WeenieType::ManaStone => Some(Class::ManaStone),
        WeenieType::House => Some(Class::House),
        WeenieType::SlumLord => Some(Class::SlumLord),
        WeenieType::Storage => Some(Class::Storage),
        WeenieType::Hook => Some(Class::Hook),
        WeenieType::Hooker => Some(Class::Hooker),
        WeenieType::HousePortal => Some(Class::HousePortal),
        WeenieType::SkillAlterationDevice => Some(Class::SkillAlterationDevice),
        WeenieType::PressurePlate => Some(Class::PressurePlate),
        WeenieType::PetDevice => Some(Class::PetDevice),
        WeenieType::Pet => Some(Class::Pet),
        WeenieType::CombatPet => Some(Class::CombatPet),
        WeenieType::Allegiance => Some(Class::Allegiance),
        WeenieType::AugmentationDevice => Some(Class::AugmentationDevice),
        WeenieType::AttributeTransferDevice => Some(Class::AttributeTransferDevice),
        WeenieType::CraftTool => Some(Class::CraftTool),
        WeenieType::LightSource => Some(Class::LightSource),
        _ => Some(Class::GenericObject),
    }
}

/// A new biota be created taking all of its values from weenie.
// ACE: WorldObjectFactory.CreateWorldObject
#[must_use]
pub fn create_world_object(
    env: &CtorEnv<'_>,
    weenie: Option<Arc<Weenie>>,
    guid: ObjectGuid,
) -> Option<WorldObject> {
    let weenie = weenie?;

    let obj_weenie_type = weenie.weenie_type;

    let Some(class) = weenie_class(obj_weenie_type) else {
        log::warn!(
            "CreateWorldObject: {} (0x{}:{}) - WeenieType is Undef, Object cannot be created.",
            weenie.get_name().unwrap_or_default(),
            guid,
            weenie.weenie_class_id
        );
        return None;
    };

    Some(WorldObject::from_weenie(env, class, weenie, guid))
}

/// Restore a WorldObject from the database. Any properties tagged as Ephemeral will be removed
/// from the biota. (ACE's third overload takes a shard-database `Biota` and converts it first with
/// `BiotaConverter.ConvertToEntityBiota`; that converter belongs to the shard store.)
// ACE: WorldObjectFactory.CreateWorldObject
#[must_use]
pub fn create_world_object_from_biota(env: &CtorEnv<'_>, biota: Biota) -> Option<WorldObject> {
    let class = biota_class(biota.weenie_type)?;
    Some(WorldObject::from_biota(env, class, biota))
}

/// The `NullReferenceException` `CreateNewWorldObjects` throws when an instance's object cannot be
/// built (see the ACE-BUG there): the instance that threw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateNewWorldObjectsAborted {
    /// `instance.Guid`.
    pub instance_guid: u32,
    /// `instance.WeenieClassId`.
    pub weenie_class_id: u32,
}

/// This will create a list of WorldObjects, all with new GUIDs and for every position provided:
/// each non-link-child instance built from its weenie at the instance's position, or from the
/// shard's biota for it when there is one. `restrict_wcid` keeps only instances of that weenie.
// ACE: WorldObjectFactory.CreateNewWorldObjects
pub fn create_new_world_objects(
    env: &CtorEnv<'_>,
    source_objects: &[LandblockInstance],
    biotas: &[Biota],
    restrict_wcid: Option<u32>,
) -> Result<Vec<WorldObject>, CreateNewWorldObjectsAborted> {
    let mut results = Vec::new();

    // spawn direct landblock objects
    for instance in source_objects.iter().filter(|x| !x.is_link_child) {
        let instance_position = || {
            Position::from_components(
                instance.obj_cell_id,
                instance.origin_x,
                instance.origin_y,
                instance.origin_z,
                instance.angles_x,
                instance.angles_y,
                instance.angles_z,
                instance.angles_w,
                false,
            )
        };

        let Some(weenie) = (env.get_cached_weenie)(instance.weenie_class_id) else {
            log::warn!(
                "CreateNewWorldObjects: Database does not contain weenie {} for instance 0x{:08X} at {}",
                instance.weenie_class_id,
                instance.guid,
                instance_position().to_loc_string()
            );
            continue;
        };

        if restrict_wcid.is_some_and(|r| r != instance.weenie_class_id) {
            continue;
        }

        let guid = ObjectGuid::new(instance.guid);

        // ACE-BUG: `CreateWorldObject` returns null for a WeenieType of Undef (either overload),
        // and ACE then reads `worldObject.Location` on it: the NullReferenceException ends the
        // call, so the caller gets no list at all (a landblock load aborts).
        let aborted = CreateNewWorldObjectsAborted {
            instance_guid: instance.guid,
            weenie_class_id: instance.weenie_class_id,
        };

        let world_object = match biotas.iter().find(|b| b.id == instance.guid) {
            None => {
                let mut world_object =
                    create_world_object(env, Some(weenie), guid).ok_or(aborted)?;

                world_object.set_location(Some(instance_position()));
                world_object
            }
            Some(biota) => {
                let mut world_object =
                    create_world_object_from_biota(env, biota.clone()).ok_or(aborted)?;

                if world_object.location().is_none() {
                    // DIVERGE: the warning names the object by its PropertyString.Name (it is not in the store yet) and prints the creation time in UTC (ACE: local time).
                    let creation_timestamp = world_object.creation_timestamp();
                    log::warn!(
                        "CreateNewWorldObjects: {} (0x{}) Location was null. CreationTimestamp = {} ({}) | Location restored from world db instance.",
                        world_object.get_property(PropertyString::Name).unwrap_or_default(),
                        world_object.guid,
                        creation_timestamp.map_or_else(String::new, |t| t.to_string()),
                        to_common_string(Time::get_date_time_from_timestamp(f64::from(creation_timestamp.unwrap_or(0))))
                    );
                    world_object.set_location(Some(instance_position()));
                }
                world_object
            }
        };

        // queue linked child objects
        let mut world_object = world_object;
        for link in &instance.landblock_instance_link {
            if let Some(link_instance) = source_objects.iter().find(|x| x.guid == link.child_guid) {
                world_object
                    .wo
                    .world_object_links
                    .linked_instances
                    .push(link_instance.clone());
            }
        }

        results.push(world_object);
    }
    Ok(results)
}

/// Creates a list of WorldObjects from a list of Biotas, skipping the ones that cannot be created.
// ACE: WorldObjectFactory.CreateWorldObjects
#[must_use]
pub fn create_world_objects(env: &CtorEnv<'_>, biotas: Vec<Biota>) -> Vec<WorldObject> {
    let mut results = Vec::new();

    for biota in biotas {
        let world_object = create_world_object_from_biota(env, biota);
        //worldObject.CalculateObjDesc();

        if let Some(world_object) = world_object {
            results.push(world_object);
        }
    }
    results
}

/// This will create a new WorldObject with a new GUID: `GuidManager.NewDynamicGuid()`, the
/// constructor, and the guid recycled (`GuidManager.RecycleDynamicGuid`) when it builds nothing.
/// The object is returned; the caller adds it to `World.objects` (and runs the insertion hook,
/// `creature::post_insert`).
// ACE: WorldObjectFactory.CreateNewWorldObject
#[must_use]
pub fn create_new_world_object_in_world(
    w: &mut crate::World,
    weenie: Arc<Weenie>,
) -> Option<WorldObject> {
    let guid = crate::managers::guid_manager::new_dynamic_guid(w);

    let world_object = CtorEnv::with_world(w, |env| create_world_object(env, Some(weenie), guid));

    if world_object.is_none() {
        crate::managers::guid_manager::recycle_dynamic_guid(w, guid);
    }

    world_object
}

/// `CreateNewWorldObject(uint weenieClassId)` on the world: null (and no guid taken) when the
/// weenie does not exist, otherwise [`create_new_world_object_in_world`].
// ACE: WorldObjectFactory.CreateNewWorldObject
#[must_use]
pub fn create_new_world_object_by_wcid_in_world(
    w: &mut crate::World,
    weenie_class_id: u32,
) -> Option<WorldObject> {
    let weenie = w.content.get_cached_weenie(weenie_class_id)?;

    create_new_world_object_in_world(w, weenie)
}

/// `CreateNewWorldObject(string weenieClassName)` on the world: null (and no guid taken) when no
/// weenie has that class name, otherwise [`create_new_world_object_by_wcid_in_world`]. The object
/// is returned, not added to `World.objects`.
// ACE: WorldObjectFactory.CreateNewWorldObject
#[must_use]
pub fn create_new_world_object_by_name_detached(
    w: &mut crate::World,
    weenie_class_name: &str,
) -> Option<WorldObject> {
    let weenie = w
        .content
        .get_cached_weenie_by_class_name(weenie_class_name)?;

    create_new_world_object_by_wcid_in_world(w, weenie.weenie_class_id)
}

/// `CreateNewWorldObject(PropertiesCreateList item)` on the world: as
/// [`create_new_world_object_from_create_list`], with the guid taken and recycled as
/// [`create_new_world_object_in_world`] does.
// ACE: WorldObjectFactory.CreateNewWorldObject
#[must_use]
pub fn create_new_world_object_from_create_list_in_world(
    w: &mut crate::World,
    item: &PropertiesCreateList,
) -> Option<WorldObject> {
    let is_treasure =
        (item.destination_type & DestinationType::Treasure) != DestinationType::default();

    let mut wo = create_new_world_object_by_wcid_in_world(w, item.weenie_class_id)?;

    apply_create_list_item(&mut wo, item, is_treasure);

    Some(wo)
}

/// The constructor half of `CreateNewWorldObject(Weenie weenie)`, for a caller that has taken
/// `guid` from `GuidManager.NewDynamicGuid()` itself (tests pin guids this way). That caller
/// recycles the guid when nothing is built; [`create_new_world_object_in_world`] does both.
// ACE: WorldObjectFactory.CreateNewWorldObject
#[must_use]
pub fn create_new_world_object(
    env: &CtorEnv<'_>,
    weenie: Arc<Weenie>,
    guid: ObjectGuid,
) -> Option<WorldObject> {
    create_world_object(env, Some(weenie), guid)
}

/// This will create a new WorldObject with a new GUID. It will return null if weenieClassId was
/// not found.
// ACE: WorldObjectFactory.CreateNewWorldObject
#[must_use]
pub fn create_new_world_object_by_wcid(
    env: &CtorEnv<'_>,
    weenie_class_id: u32,
    guid: ObjectGuid,
) -> Option<WorldObject> {
    let weenie = (env.get_cached_weenie)(weenie_class_id)?;

    create_new_world_object(env, weenie, guid)
}

/// This will create a new WorldObject with a new GUID. It will return null if weenieClassName was
/// not found. The by-name lookup is the world database's (`env.w.content`); the by-id one that
/// follows is `env`'s. [`create_new_world_object_by_name_in_world`] runs it on the world.
// ACE: WorldObjectFactory.CreateNewWorldObject
#[must_use]
pub fn create_new_world_object_by_name(
    env: &CtorEnv<'_>,
    weenie_class_name: &str,
    guid: ObjectGuid,
) -> Option<WorldObject> {
    let weenie = env
        .w
        .content
        .get_cached_weenie_by_class_name(weenie_class_name)?;

    create_new_world_object_by_wcid(env, weenie.weenie_class_id, guid)
}

/// Not ACE: `WorldObjectFactory.CreateNewWorldObject(string weenieClassName)` run on the world as
/// ACE runs it: the cached weenie of that class name (none: null, and no guid is taken), then
/// `CreateNewWorldObject(weenie.WeenieClassId)`: the cached weenie by id, a new dynamic guid, the
/// constructor, and the guid recycled when it builds nothing. The new object joins
/// `World.objects`; its guid is returned.
///
/// # Panics
/// When the new dynamic guid is already live (a guid manager defect).
pub fn create_new_world_object_by_name_in_world(
    w: &mut crate::World,
    weenie_class_name: &str,
) -> Option<ObjectGuid> {
    let world_object = create_new_world_object_by_name_detached(w, weenie_class_name)?;
    let guid = world_object.guid;
    assert!(
        w.objects.insert(world_object).is_ok(),
        "a new dynamic guid 0x{:08X} is already live",
        guid.full()
    );
    crate::world_objects::creature::post_insert(w, guid);
    Some(guid)
}

/// Creates a new WorldObject from a CreateList item.
// ACE: WorldObjectFactory.CreateNewWorldObject
#[must_use]
pub fn create_new_world_object_from_create_list(
    env: &CtorEnv<'_>,
    item: &PropertiesCreateList,
    guid: ObjectGuid,
) -> Option<WorldObject> {
    let is_treasure =
        (item.destination_type & DestinationType::Treasure) != DestinationType::default();

    let mut wo = create_new_world_object_by_wcid(env, item.weenie_class_id, guid)?;

    apply_create_list_item(&mut wo, item, is_treasure);

    Some(wo)
}

/// The create-list item's destination, stack size, palette and (for non-treasure) shade, as
/// `CreateNewWorldObject(PropertiesCreateList item)` sets them on the new object.
fn apply_create_list_item(wo: &mut WorldObject, item: &PropertiesCreateList, is_treasure: bool) {
    wo.wo.world_object.destination_type = item.destination_type;

    if item.stack_size > 1 {
        wo.set_stack_size(Some(item.stack_size));
    }

    if item.palette > 0 {
        wo.set_palette_template(Some(i32::from(item.palette)));
    }

    // if treasure, this is probability instead of shade
    if !is_treasure {
        wo.set_shade(Some(f64::from(item.shade)));
    }
}
