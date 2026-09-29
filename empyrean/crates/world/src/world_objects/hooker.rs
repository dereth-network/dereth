// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Hooker.cs
//! Port of `Source/ACE.Server/WorldObjects/Hooker.cs`.

use empyrean_entity::enums::{HookGroupType, HouseType, WeenieError, WeenieErrorWithString};
use empyrean_entity::ObjectGuid;

use crate::entity::activation_result::ActivationResult;
use crate::entity::landblock;
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;
use crate::world_objects::world_object::{CtorEnv, CtorSource, WorldObject};
use crate::world_objects::{hook, house};
use crate::{dispatch, World};

/// Non-property fields declared in `Hooker.cs`.
#[derive(Debug, Default)]
pub struct HookerFields {}

// ---- constructors and SetEphemeralValues ----

/// `new Hooker(weenie, guid)` / `new Hooker(biota)`: the `WorldObject` constructor, then
/// Hooker's `SetEphemeralValues`.
// ACE: Hooker.Hooker
pub fn hooker_ctor(o: &mut WorldObject, env: &CtorEnv<'_>, src: CtorSource) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    hooker_set_ephemeral_values(o, env);
}

// ACE: Hooker.SetEphemeralValues
fn hooker_set_ephemeral_values(o: &mut WorldObject, _env: &CtorEnv<'_>) {
    o.set_activation_response(
        o.activation_response() | empyrean_entity::enums::ActivationResponse::Emote,
    );
}

// ACE: Hooker.ActOnUse
pub fn hooker_act_on_use(_w: &mut World, _this: ObjectGuid, _activator: ObjectGuid) {
    // handled in base.OnActivate -> EmoteManager.OnUse()
}

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

// ACE: Hooker.CheckUseRequirements
pub fn hooker_check_use_requirements(
    w: &mut World,
    this: ObjectGuid,
    activator: ObjectGuid,
) -> ActivationResult {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return ActivationResult::new(false);
    }
    let player = activator;

    let Some(hook) = is_hooked(w, this, player) else {
        let name = dispatch::name::name(w, this).unwrap_or_default();
        return refusal(w, player, |d| {
            game_event_weenie_error_with_string(
                d,
                WeenieErrorWithString::ItemOnlyUsableOnHook,
                &name,
            )
        });
    };

    let hook_owner = w.objects.get(hook).and_then(WorldObject::house_owner);
    let root_house = hook::house(w, hook).and_then(|h| house::root_house(w, h));
    if hook_owner.is_none()
        || (!root_house.is_some_and(|h| house::open_status(w, h))
            && !root_house.is_some_and(|h| house::has_permission(w, h, player, false)))
    {
        return refusal(w, player, |d| {
            game_event_weenie_error(d, WeenieError::YouAreNotPermittedToUseThatHook)
        });
    }

    let my_hook_group = w
        .objects
        .get(this)
        .and_then(WorldObject::hook_group)
        .unwrap_or(HookGroupType::Undef);
    let root_type = root_house
        .and_then(|h| w.objects.get(h))
        .map(WorldObject::house_type);
    if (my_hook_group == HookGroupType::PortalItems
        || my_hook_group == HookGroupType::SpellTeachingItems)
        && root_type != Some(HouseType::Mansion)
    {
        return refusal(w, player, |d| {
            game_event_weenie_error(d, WeenieError::YouAreNotPermittedToUseThatHook)
        });
    }

    let base_requirements =
        crate::world_objects::world_object_use::world_object_check_use_requirements(
            w, this, activator,
        );
    if !base_requirements.success {
        return base_requirements;
    }

    ActivationResult::new(true)
}

/// `IsHooked(checker, out hook)`: the hook this item hangs on, looked up on the checker's
/// landblock.
///
/// # Panics
/// When the checker is on no landblock (`CurrentLandblock.GetObject` on null).
// ACE: Hooker.IsHooked
pub fn is_hooked(w: &World, this: ObjectGuid, checker: ObjectGuid) -> Option<ObjectGuid> {
    let owner_id = w.objects.get(this)?.owner_id().filter(|&o| o != 0)?;

    let lb = w
        .objects
        .get(checker)
        .and_then(|o| o.current_landblock)
        .expect("System.NullReferenceException: checker.CurrentLandblock");
    let wo = landblock::get_object(w, lb, ObjectGuid::new(owner_id), true)?;

    w.objects
        .get(wo)
        .is_some_and(WorldObject::is_hook)
        .then_some(wo)
}
