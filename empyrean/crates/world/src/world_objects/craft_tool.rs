// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/CraftTool.cs
//! Port of `Source/ACE.Server/WorldObjects/CraftTool.cs`.

use empyrean_entity::ObjectGuid;

use crate::entity::{aetheria, core_plating};
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `CraftTool.cs`.
#[derive(Debug, Default)]
pub struct CraftToolFields {}

// ---- virtual-dispatch targets ----

// ACE: CraftTool.HandleActionUseOnTarget
/// An encapsulated spirit refills a pet device, an aetheria mana stone goes to `Aetheria`, a core
/// plating device to `CorePlating`; anything else falls back on the recipe manager.
pub fn craft_tool_handle_action_use_on_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    if pet_device_is_encapsulated_spirit(w, this)
        && w.objects.get(target).is_some_and(|o| o.is_pet_device())
    {
        pet_device_refill(w, target, player, this);
        return;
    }

    if aetheria::is_aetheria_mana_stone(obj(w, this))
        && aetheria::is_aetheria(obj(w, target).biota.weenie_class_id)
    {
        aetheria_use_object_on_target(w, player, this, target);
        return;
    }

    if core_plating_is_core_plating_device(w, this) {
        core_plating_use_object_on_target(w, player, this, target);
        return;
    }

    // fallback on recipe manager
    crate::world_objects::world_object_use::world_object_handle_action_use_on_target(
        w, this, player, target,
    );
}

// ACE: CraftTool.ActOnUse
pub fn craft_tool_act_on_use(
    _w: &mut crate::World,
    _this: empyrean_entity::ObjectGuid,
    _activator: empyrean_entity::ObjectGuid,
) {
    // Do nothing
}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

// ================================================================================ pointers

/// `PetDevice.IsEncapsulatedSpirit(wo)` (`PetDevice.cs`).
fn pet_device_is_encapsulated_spirit(w: &World, wo: ObjectGuid) -> bool {
    crate::world_objects::pet_device::is_encapsulated_spirit(obj(w, wo))
}

/// `petDevice.Refill(player, spirit)` (`PetDevice.cs`).
fn pet_device_refill(
    w: &mut World,
    pet_device: ObjectGuid,
    player: ObjectGuid,
    spirit: ObjectGuid,
) {
    crate::world_objects::pet_device::refill(w, pet_device, player, spirit);
}

/// `Aetheria.UseObjectOnTarget(player, source, target)` (`Entity/Aetheria.cs`).
fn aetheria_use_object_on_target(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) {
    crate::entity::aetheria::use_object_on_target(w, player, source, target);
}

/// `CorePlating.IsCorePlatingDevice(wo)` (`Entity/CorePlating.cs`).
fn core_plating_is_core_plating_device(w: &World, wo: ObjectGuid) -> bool {
    core_plating::is_core_plating_device(obj(w, wo))
}

/// `CorePlating.UseObjectOnTarget(player, source, target)` (`Entity/CorePlating.cs`).
fn core_plating_use_object_on_target(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) {
    core_plating::use_object_on_target(w, player, source, target);
}

// ---- constructors and SetEphemeralValues ----

/// `new CraftTool(weenie, guid)` / `new CraftTool(biota)`: the `Stackable` constructor, then
/// CraftTool's `SetEphemeralValues`.
// ACE: CraftTool.CraftTool
pub fn craft_tool_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::stackable::stackable_ctor(o, env, src);
    craft_tool_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: CraftTool.SetEphemeralValues
fn craft_tool_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
