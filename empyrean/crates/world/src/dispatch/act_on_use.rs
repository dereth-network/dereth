// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Use.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Use.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.ActOnUse`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.ActOnUse(WorldObject activator)` (Source/ACE.Server/WorldObjects/WorldObject_Use.cs).
/// Overridden by: AdvocateFane, Ammunition, AttributeTransferDevice, AugmentationDevice, Bindstone, Book, Chest, Container, Cow, CraftTool, Creature, Door, Food, Game, Gem, GenericObject, Hook, Hooker, HousePortal, Lifestone, LightSource, PKModifier, PetDevice, Portal, PressurePlate, Scroll, SkillAlterationDevice, SlumLord, Stackable, Switch, Vendor.
/// Overrides calling `base.ActOnUse`: Hook, HousePortal.
pub fn act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::AdvocateFane => {
            crate::world_objects::advocate_fane::advocate_fane_act_on_use(w, this, activator)
        }
        Class::Ammunition => {
            crate::world_objects::ammunition::ammunition_act_on_use(w, this, activator)
        }
        Class::AttributeTransferDevice => {
            crate::world_objects::attribute_transfer_device::attribute_transfer_device_act_on_use(
                w, this, activator,
            )
        }
        Class::AugmentationDevice => {
            crate::world_objects::augmentation_device::augmentation_device_act_on_use(
                w, this, activator,
            )
        }
        Class::Bindstone => {
            crate::world_objects::bindstone::bindstone_act_on_use(w, this, activator)
        }
        Class::Book => crate::world_objects::book::book_act_on_use(w, this, activator),
        Class::Chest | Class::Storage => {
            crate::world_objects::chest::chest_act_on_use(w, this, activator)
        }
        Class::Container | Class::Corpse => {
            crate::world_objects::container::container_act_on_use(w, this, activator)
        }
        Class::Cow => crate::world_objects::cow::cow_act_on_use(w, this, activator),
        Class::CraftTool => {
            crate::world_objects::craft_tool::craft_tool_act_on_use(w, this, activator)
        }
        Class::Admin
        | Class::CombatPet
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Player
        | Class::Sentinel => {
            crate::world_objects::creature::creature_act_on_use(w, this, activator)
        }
        Class::Door => crate::world_objects::door::door_act_on_use(w, this, activator),
        Class::Food => crate::world_objects::food::food_act_on_use(w, this, activator),
        Class::Game => crate::world_objects::game::game_act_on_use(w, this, activator),
        Class::Gem => crate::world_objects::gem::gem_act_on_use(w, this, activator),
        Class::AdvocateItem | Class::GenericObject => {
            crate::world_objects::generic_object::generic_object_act_on_use(w, this, activator)
        }
        Class::Hook => crate::world_objects::hook::hook_act_on_use(w, this, activator),
        Class::Hooker => crate::world_objects::hooker::hooker_act_on_use(w, this, activator),
        Class::HousePortal => {
            crate::world_objects::house_portal::house_portal_act_on_use(w, this, activator)
        }
        Class::Lifestone => {
            crate::world_objects::lifestone::lifestone_act_on_use(w, this, activator)
        }
        Class::LightSource => {
            crate::world_objects::light_source::light_source_act_on_use(w, this, activator)
        }
        Class::PKModifier => {
            crate::world_objects::pk_modifier::pk_modifier_act_on_use(w, this, activator)
        }
        Class::PetDevice => {
            crate::world_objects::pet_device::pet_device_act_on_use(w, this, activator)
        }
        Class::Portal => crate::world_objects::portal::portal_act_on_use(w, this, activator),
        Class::PressurePlate => {
            crate::world_objects::pressure_plate::pressure_plate_act_on_use(w, this, activator)
        }
        Class::Scroll => crate::world_objects::scroll::scroll_act_on_use(w, this, activator),
        Class::SkillAlterationDevice => {
            crate::world_objects::skill_alteration_device::skill_alteration_device_act_on_use(
                w, this, activator,
            )
        }
        Class::SlumLord => {
            crate::world_objects::slum_lord::slum_lord_act_on_use(w, this, activator)
        }
        Class::Coin | Class::Missile | Class::SpellComponent | Class::Stackable => {
            crate::world_objects::stackable::stackable_act_on_use(w, this, activator)
        }
        Class::Switch => crate::world_objects::switch::switch_act_on_use(w, this, activator),
        Class::Vendor => crate::world_objects::vendor::vendor_act_on_use(w, this, activator),
        _ => crate::world_objects::world_object_use::world_object_act_on_use(w, this, activator),
    }
}
