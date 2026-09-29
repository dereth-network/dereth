// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Hook.cs
//! Port of `Source/ACE.Server/WorldObjects/Hook.cs`.
//!
//! House hooks for item placement

use empyrean_entity::enums::{
    HookType, MotionCommand, MotionStance, PhysicsState, Placement, PropertyBool, WeenieError,
    WeenieErrorWithString,
};
use empyrean_entity::ObjectGuid;

use crate::entity::activation_result::ActivationResult;
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::messages::game_message_public_update_property_bool::game_message_public_update_property_bool;
use crate::network::game_messages::messages::game_message_update_object::game_message_update_object;
use crate::network::motion::movement_data::Motion;
use crate::physics::phys_ext;
use crate::sessions::SessionData;
use crate::world_objects::managers::emote_manager;
use crate::world_objects::world_object::{CtorEnv, CtorSource, WorldObject};
use crate::world_objects::{container, house, player_house, world_object, world_object_networking};
use crate::{dispatch, World};

/// Non-property fields declared in `Hook.cs`.
#[derive(Debug, Default)]
pub struct HookFields {}

/// The values `OnRemoveItem` copies back from a pristine hook of the same weenie
/// (`cachedHookReferences`).
#[derive(Debug, Clone, Default)]
pub struct HookReference {
    pub setup_table_id: u32,
    pub motion_table_id: u32,
    pub physics_table_id: u32,
    pub sound_table_id: u32,
    pub placement: Option<Placement>,
    pub obj_scale: Option<f32>,
    pub name: Option<String>,
}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

// ACE: Hook.House
#[must_use]
pub fn house(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    let parent = w.objects.get(this)?.wo.world_object_links.parent_link?;
    w.objects
        .get(parent)
        .is_some_and(WorldObject::is_house)
        .then_some(parent)
}

// ACE: Hook.HasItem
#[must_use]
pub fn has_item(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .and_then(|o| o.container.as_ref())
        .is_some_and(|c| !c.container.inventory.is_empty())
}

// ACE: Hook.Item
#[must_use]
pub fn item(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    w.objects
        .get(this)
        .and_then(|o| o.container.as_ref())
        .and_then(|c| c.container.inventory.keys().next().copied())
}

/// `House?.RootHouse`.
fn root_house(w: &mut World, this: ObjectGuid) -> Option<ObjectGuid> {
    let h = house(w, this)?;
    house::root_house(w, h)
}

/// `new GameEventWeenieError(player.Session, error)` / `..WithString(..)` as an activation refusal.
fn refusal(
    w: &mut World,
    player: ObjectGuid,
    f: impl FnOnce(&mut SessionData) -> GameMessage,
) -> ActivationResult {
    let session = player_manager::player_session(w, player)
        .expect("System.NullReferenceException: player.Session");
    let data = w.sessions.get_mut(session).expect("the player's session");
    ActivationResult::with_message(f(data))
}

// ---- constructors and SetEphemeralValues ----

/// `new Hook(weenie, guid)` / `new Hook(biota)`: the `Container` constructor, then
/// Hook's `SetEphemeralValues`.
// ACE: Hook.Hook
pub fn hook_ctor(o: &mut WorldObject, env: &CtorEnv<'_>, src: CtorSource) {
    crate::world_objects::container::container_ctor(o, env, src);
    hook_set_ephemeral_values(o, env);
}

// ACE: Hook.SetEphemeralValues
fn hook_set_ephemeral_values(o: &mut WorldObject, _env: &CtorEnv<'_>) {
    o.set_is_locked(false);
    o.set_is_open(false);
}

// ================================================================================ Hook.cs

// ACE: Hook.CheckUseRequirements
pub fn hook_check_use_requirements(
    w: &mut World,
    this: ObjectGuid,
    activator: ObjectGuid,
) -> ActivationResult {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return ActivationResult::new(false);
    }
    let player = activator;

    if player_house::is_olthoi_player(w, player) {
        crate::world_objects::player_networking::send_weenie_error(
            w,
            player,
            WeenieError::OlthoiCannotInteractWithThat,
        );
        return ActivationResult::new(false);
    }

    if obj(w, player).ignore_house_barriers() {
        return ActivationResult::new(true);
    }

    let root_house = root_house(w, this);
    let house_owner = root_house.and_then(|h| obj(w, h).house_owner());

    let house_hooks_visible = root_house
        .and_then(|h| obj(w, h).house_hooks_visible())
        .unwrap_or(true);

    let item = item(w, this);
    let name = dispatch::name::name(w, this).unwrap_or_default();
    let player_house_owner = player_house::house(w, player)
        .and_then(|h| w.objects.get(h))
        .and_then(WorldObject::house_owner);

    if !house_hooks_visible {
        if let Some(item) = item.filter(|&i| {
            w.objects
                .get(i)
                .is_some_and(|o| o.is_hooker() || o.is_book())
        }) {
            // redirect to item.CheckUseRequirements
            return dispatch::check_use_requirements::check_use_requirements(w, item, activator);
        }

        if house_owner.is_some()
            && (Some(player.full()) == house_owner || player_house_owner == house_owner)
        {
            return refusal(w, player, |d| {
                game_event_weenie_error_with_string(
                    d,
                    WeenieErrorWithString::ItemUnusableOnHook_CanOpen,
                    &name,
                )
            });
        }
        return refusal(w, player, |d| {
            game_event_weenie_error_with_string(
                d,
                WeenieErrorWithString::ItemUnusableOnHook_CannotOpen,
                &name,
            )
        });
    }

    if player_house::house(w, player).is_none()
        || house_owner.is_none()
        || (Some(player.full()) != house_owner && player_house_owner != house_owner)
    {
        return match item {
            None => refusal(w, player, |d| {
                game_event_weenie_error(d, WeenieError::HookItemNotUsable_CannotOpen)
            }),
            Some(i) if obj(w, i).is_hooker() => refusal(w, player, |d| {
                game_event_weenie_error(d, WeenieError::YouAreNotPermittedToUseThatHook)
            }),
            Some(_) => refusal(w, player, |d| {
                game_event_weenie_error_with_string(
                    d,
                    WeenieErrorWithString::ItemUnusableOnHook_CannotOpen,
                    &name,
                )
            }),
        };
    }

    ActivationResult::new(true)
}

// ACE: Hook.ActOnUse
pub fn hook_act_on_use(w: &mut World, this: ObjectGuid, wo: ObjectGuid) {
    let visible = root_house(w, this)
        .and_then(|h| obj(w, h).house_hooks_visible())
        .unwrap_or(true);
    if let (false, Some(item)) = (visible, item(w, this)) {
        if w.objects.get(wo).is_some_and(WorldObject::is_player) {
            crate::world_objects::player_use::fields_mut(w, wo).las_used_hook_id = this;
        }

        // redirect to item.ActOnUse
        dispatch::on_activate::on_activate(w, item, wo);

        return;
    }

    container::container_act_on_use(w, this, wo);
}

// ACE: Hook.OnInitialInventoryLoadCompleted
pub fn hook_on_initial_inventory_load_completed(w: &mut World, this: ObjectGuid) {
    let hidden = !root_house(w, this)
        .and_then(|h| obj(w, h).house_hooks_visible())
        .unwrap_or(true);

    let has_item = has_item(w, this);
    phys_ext::set_physics_property_state(
        w,
        this,
        PropertyBool::Ethereal,
        PhysicsState::Ethereal,
        Some(!has_item),
    );
    if has_item {
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::NoDraw,
            PhysicsState::NoDraw,
            Some(false),
        );
        obj_mut(w, this).set_ui_hidden(false);
    } else {
        phys_ext::set_physics_property_state(
            w,
            this,
            PropertyBool::NoDraw,
            PhysicsState::NoDraw,
            Some(hidden),
        );
        obj_mut(w, this).set_ui_hidden(hidden);
    }

    if has_item {
        hook_on_add_item(w, this);
    }
}

/// `item.NameWithMaterial` (`WorldObject.GetNameWithMaterial()`).
fn name_with_material(w: &World, item: ObjectGuid) -> Option<String> {
    Some(crate::managers::recipe_manager::shims::get_name_with_material(w, item, None))
}

/// This event is raised when player adds item to hook
// ACE: Hook.OnAddItem
pub fn hook_on_add_item(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine("Hook.OnAddItem()");

    let Some(item) = item(w, this) else {
        log::error!("OnAddItem() raised for Hook but Inventory collection has no values.");
        return;
    };

    house::set_hook_physics(w, this, false, false, false);

    let (setup, motion, physics, sound, scale) = {
        let i = obj(w, item);
        (
            i.setup_table_id(),
            i.motion_table_id(),
            i.physics_table_id(),
            i.sound_table_id(),
            i.obj_scale(),
        )
    };
    let name = name_with_material(w, item);
    let hook_placement = obj(w, item).hook_placement();
    {
        let o = obj_mut(w, this);
        o.set_setup_table_id(setup);
        o.set_motion_table_id(motion);
        o.set_physics_table_id(physics);
        o.set_sound_table_id(sound);
        o.set_obj_scale(scale);
    }
    match name {
        Some(n) => dispatch::name::set_name(w, this, n),
        None => obj_mut(w, this).remove_property(empyrean_entity::enums::PropertyString::Name),
    }

    if obj(w, this).motion_table_id() != 0 {
        world_object_networking::shims::set_current_motion_state(
            w,
            this,
            Some(Motion::from_stance(MotionStance::Invalid)),
        );
    }

    let placement = Placement(
        hook_placement
            .unwrap_or(Placement::Hook.0.cast_signed())
            .cast_unsigned(),
    );
    obj_mut(w, this).set_placement(Some(placement));

    emote_manager::set_proxy(w, item, this);

    let house = house(w, this).expect("System.NullReferenceException: House");
    house::add_house_current_hooks_usable(w, house, -1);

    // Here we explicitly save the hook to the database to prevent item loss.
    // If the player adds an item to the hook, and the server crashes before the hook has been saved, the item will be lost.
    dispatch::save_biota_to_database::save_biota_to_database(w, this, true);

    let m = game_message_update_object(w, this, false, false);
    world_object_networking::enqueue_broadcast(w, this, true, &[m]);
}

/// `cachedHookReferences[WeenieClassId]`: a pristine hook of this weenie (built once per weenie).
fn cached_hook_reference(w: &mut World, weenie_class_id: u32) -> HookReference {
    if let Some(hook) = w.house_manager.cached_hook_references.get(&weenie_class_id) {
        return hook.clone();
    }
    let weenie = w.content.get_cached_weenie(weenie_class_id);
    let hook = CtorEnv::with_world(w, |env| {
        crate::factories::world_object_factory::create_world_object(
            env,
            weenie,
            ObjectGuid::INVALID,
        )
    })
    .expect("System.NullReferenceException: CreateWorldObject(weenie, ObjectGuid.Invalid)");
    let r = HookReference {
        setup_table_id: hook.setup_table_id(),
        motion_table_id: hook.motion_table_id(),
        physics_table_id: hook.physics_table_id(),
        sound_table_id: hook.sound_table_id(),
        placement: hook.placement(),
        obj_scale: hook.obj_scale(),
        name: hook.get_property(empyrean_entity::enums::PropertyString::Name),
    };
    w.house_manager
        .cached_hook_references
        .insert(weenie_class_id, r.clone());
    r
}

/// This event is raised when player removes item from hook
// ACE: Hook.OnRemoveItem
pub fn hook_on_remove_item(w: &mut World, this: ObjectGuid, removed_item: ObjectGuid) {
    //Console.WriteLine("Hook.OnRemoveItem()");

    let wcid = obj(w, this).biota.weenie_class_id;
    let hook = cached_hook_reference(w, wcid);

    {
        let o = obj_mut(w, this);
        o.set_setup_table_id(hook.setup_table_id);
        o.set_motion_table_id(hook.motion_table_id);
        o.set_physics_table_id(hook.physics_table_id);
        o.set_sound_table_id(hook.sound_table_id);
        o.set_placement(hook.placement);
        o.set_obj_scale(hook.obj_scale);
    }
    match hook.name {
        Some(n) => dispatch::name::set_name(w, this, n),
        None => obj_mut(w, this).remove_property(empyrean_entity::enums::PropertyString::Name),
    }

    house::set_hook_physics(w, this, false, false, true);

    if obj(w, this).motion_table_id() == 0 {
        world_object_networking::shims::set_current_motion_state(w, this, None);
    }

    emote_manager::clear_proxy(w, removed_item);

    let house = house(w, this).expect("System.NullReferenceException: House");
    house::add_house_current_hooks_usable(w, house, 1);

    let m = game_message_update_object(w, this, false, false);
    world_object_networking::enqueue_broadcast(w, this, true, &[m]);

    // Here we explicitly save the storage to the database to prevent property desync.
    dispatch::save_biota_to_database::save_biota_to_database(w, this, true);
}

// ACE: Hook.MotionPickup
#[must_use]
pub fn hook_motion_pickup(w: &World, this: ObjectGuid) -> MotionCommand {
    let hook_type = HookType(i32::from(obj(w, this).hook_type().unwrap_or(0)));

    match hook_type {
        HookType::Wall => MotionCommand::Pickup10,

        HookType::Ceiling | HookType::Roof => MotionCommand::Pickup20,

        _ => MotionCommand::Pickup,
    }
}

// ACE: Hook.UpdateHookVisibility
pub fn update_hook_visibility(w: &mut World, this: ObjectGuid) {
    if !has_item(w, this) {
        if root_house(w, this)
            .and_then(|h| obj(w, h).house_hooks_visible())
            .unwrap_or(false)
        {
            house::set_hook_physics(w, this, false, false, true);
        } else {
            house::set_hook_physics(w, this, true, true, true);
        }

        world_object::enqueue_broadcast_physics_state(w, this);
        let ui_hidden = obj(w, this).ui_hidden();
        let m = game_message_public_update_property_bool(
            obj_mut(w, this),
            PropertyBool::UiHidden,
            ui_hidden,
        );
        world_object_networking::enqueue_broadcast(w, this, true, &[m]);
    }
}
