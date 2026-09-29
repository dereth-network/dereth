// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Use.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Use.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.HandleActionUseOnTarget`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.HandleActionUseOnTarget(Player player, WorldObject target)` (Source/ACE.Server/WorldObjects/WorldObject_Use.cs).
/// Overridden by: CraftTool, Gem, Healer, Key, Lockpick, ManaStone.
/// Overrides calling `base.HandleActionUseOnTarget`: CraftTool, Gem.
pub fn handle_action_use_on_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::CraftTool => {
            crate::world_objects::craft_tool::craft_tool_handle_action_use_on_target(
                w, this, player, target,
            )
        }
        Class::Gem => {
            crate::world_objects::gem::gem_handle_action_use_on_target(w, this, player, target)
        }
        Class::Healer => crate::world_objects::healer::healer_handle_action_use_on_target(
            w, this, player, target,
        ),
        Class::Key => {
            crate::world_objects::key::key_handle_action_use_on_target(w, this, player, target)
        }
        Class::Lockpick => crate::world_objects::lockpick::lockpick_handle_action_use_on_target(
            w, this, player, target,
        ),
        Class::ManaStone => {
            crate::world_objects::mana_stone::mana_stone_handle_action_use_on_target(
                w, this, player, target,
            )
        }
        _ => crate::world_objects::world_object_use::world_object_handle_action_use_on_target(
            w, this, player, target,
        ),
    }
}
