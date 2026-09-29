// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/PetDevice.cs
//! Port of `Source/ACE.Server/WorldObjects/PetDevice.cs`: the essences used to summon creatures.

use empyrean_entity::enums::{
    ChatMessageType, CombatMode, MotionCommand, PropertyInt, WeenieError, WeenieType,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::activation_result::ActivationResult;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::player_inventory::{find_object, SearchLocations};
use crate::world_objects::player_use::send_use_done_event;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `PetDevice.cs`.
#[derive(Debug, Default)]
pub struct PetDeviceFields {}

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn name(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

fn send_chat(w: &mut World, player: ObjectGuid, text: &str) {
    let session = crate::world_objects::world_object_networking::shims::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    enqueue_send(
        w,
        session,
        game_message_system_chat(text, ChatMessageType::Broadcast),
    );
}

/// `player.CurrentActivePet` (`Player_Use.cs`): a destroyed pet reads as null.
fn current_active_pet(w: &World, player: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::player_use::fields(w, player)
        .current_active_pet
        .filter(|&g| w.objects.contains(g))
}

// ACE: PetDevice.ActOnUse
/// Summons the device's creature (a charge is used unless the summon failed).
pub fn pet_device_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    // Good PCAP example of using a PetDevice to summon a pet:
    // Asherons-Call-packets-includes-3-towers\pkt_2017-1-30_1485823896_log.pcap lines 27837 - 27843

    let Some(pet_class) = object(w, this).pet_class() else {
        log::error!(
            "{}.ActOnUse({}) - PetClass is null for PetDevice {}",
            name(w, activator),
            name(w, this),
            object(w, this).weenie_class_id()
        );
        return;
    };

    if object(w, this).structure() == Some(0) {
        //player.Session.Network.EnqueueSend(new GameEventCommunicationTransientString(player.Session, "You must refill the essence to use it again."));
        send_chat(
            w,
            player,
            "Your summoning device does not have enough charges to function!",
        );
        return;
    }

    let wcid = pet_class.cast_unsigned();

    let result = summon_creature(w, this, player, wcid);

    if result != Some(false) {
        // CombatPet devices should always have structure
        if let Some(structure) = object(w, this).structure() {
            // decrease remaining uses
            let structure = structure.wrapping_sub(1);
            let o = w.objects.get_mut(this).expect("ACE: this");
            o.set_structure(Some(structure));

            let msg = game_message_public_update_property_int(
                o,
                PropertyInt::Structure,
                i32::from(structure),
            );
            let session =
                crate::world_objects::world_object_networking::shims::player_session(w, player)
                    .expect("ACE: Player.Session is null (NullReferenceException)");
            enqueue_send(w, session, msg);
        }
    } else {
        // this would be a good place to send a friendly reminder to install the latest summoning updates from ACE-World-Patch
    }
}

// ACE: PetDevice.CheckUseRequirements
/// The base requirements, the summoning mastery, and no combat pet already out (per
/// `pet_stow_replace`).
pub fn pet_device_check_use_requirements(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) -> crate::entity::activation_result::ActivationResult {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return ActivationResult::new(false);
    }
    let player = activator;

    let base_requirements =
        crate::world_objects::world_object_use::world_object_check_use_requirements(
            w, this, activator,
        );
    if !base_requirements.success {
        return base_requirements;
    }

    // verify summoning mastery
    let summoning_mastery = object(w, this).summoning_mastery();
    if let Some(mastery) = summoning_mastery {
        if object(w, player).summoning_mastery() != Some(mastery) {
            let text = format!(
                "You must be a {} to use the {}",
                mastery.to_dotnet_string(),
                name(w, this)
            );
            send_chat(w, player, &text);
            return ActivationResult::new(false);
        }
    }

    // duplicating some of this verification logic here from Pet.Init()
    // since the PetDevice owner and the summoned Pet are separate objects w/ potentially different heartbeat offsets,
    // the cooldown can still expire before the CombatPet's lifespan
    // in this case, if the player tries to re-activate the PetDevice while the CombatPet is still in the world,
    // we want to return an error without re-activating the cooldown

    if let Some(current) = current_active_pet(w, player).filter(|&p| object(w, p).is_combat_pet()) {
        if crate::managers::property_manager::get_bool(w, "pet_stow_replace", false, true).item {
            // original ace
            let msg = format!("{} is already active", name(w, current));
            crate::world_objects::player_networking::send_transient_error(w, player, &msg);
            return ActivationResult::new(false);
        }

        // retail stow
        // `(uint)PetClass`: a null PetClass throws (InvalidOperationException)
        let pet_class = object(w, this)
            .pet_class()
            .expect("ACE: PetClass is null (InvalidOperationException)")
            .cast_unsigned();
        let weenie = w.content.get_cached_weenie(pet_class);

        if weenie.is_none_or(|wn| wn.weenie_type != WeenieType::Pet) {
            let msg = format!("{} is already active", name(w, current));
            crate::world_objects::player_networking::send_transient_error(w, player, &msg);
            return ActivationResult::new(false);
        }
    }
    ActivationResult::new(true)
}

// ACE: PetDevice.SummonCreature
/// Creates the creature and runs its `Init`; a failure destroys it.
pub fn summon_creature(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
    wcid: u32,
) -> Option<bool> {
    let Some(wo) = crate::factories::player_factory::create_new_world_object_by_wcid(w, wcid)
    else {
        let wcid_self = object(w, this).weenie_class_id();
        let class_name =
            crate::world_objects::world_object_networking::shims::weenie_class_name(w, wcid_self);
        log::error!("{}.SummonCreature({wcid}) - couldn't find wcid for PetDevice {wcid_self} - {class_name}", name(w, player));
        return Some(false);
    };

    let guid = wo.guid;
    let is_pet = wo.is_pet();
    let (wo_wcid, wo_type) = (wo.weenie_class_id(), wo.weenie_type());
    assert!(
        w.objects.insert(wo).is_ok(),
        "a new dynamic guid 0x{:08X} is already live",
        guid.full()
    );
    crate::world_objects::creature::post_insert(w, guid);

    if !is_pet {
        let wcid_self = object(w, this).weenie_class_id();
        let class_name =
            crate::world_objects::world_object_networking::shims::weenie_class_name(w, wcid_self);
        let wo_class_name =
            crate::world_objects::world_object_networking::shims::weenie_class_name(w, wo_wcid);
        log::error!(
            "{}.SummonCreature({wcid}) - PetDevice {wcid_self} - {class_name} tried to summon {wo_wcid} - {wo_class_name} of unknown type {}",
            name(w, player),
            wo_type.to_dotnet_string()
        );
        // ACE leaves the object unreferenced
        w.objects.remove(guid);
        return Some(false);
    }
    let success = crate::dispatch::init::init(w, guid, player, this);

    if success != Some(true) {
        crate::world_objects::world_object::destroy(w, guid, true, false);
    }

    success
}

// ACE: PetDevice.IsEncapsulatedSpirit
/// Returns TRUE if wo is Encapsulated Spirit.
#[must_use]
pub fn is_encapsulated_spirit(wo: &WorldObject) -> bool {
    wo.weenie_class_id() == 49485
}

// ACE: PetDevice.Refill
/// Applies an encapsulated spirit to a PetDevice: a clap, then the device is full again.
pub fn refill(w: &mut World, this: ObjectGuid, player: ObjectGuid, spirit: ObjectGuid) {
    // TODO: this should be moved to recipe system
    if !is_encapsulated_spirit(object(w, spirit)) {
        send_use_done_event(w, player, WeenieError::None);
        return;
    }

    if crate::world_objects::player_magic::is_busy(w, player) {
        send_use_done_event(w, player, WeenieError::YoureTooBusy);
        return;
    }

    // verify use requirements
    let use_error = verify_use_requirements(w, player, spirit, this);
    if use_error != WeenieError::None {
        send_use_done_event(w, player, use_error);
        return;
    }

    crate::world_objects::player_magic::set_is_busy(w, player, true);

    let mut anim_time = 0.0f32;

    let mut action_chain = ActionChain::new();

    // handle switching to peace mode
    if crate::world_objects::player_move::creature_combat_mode(w, player) != CombatMode::NonCombat {
        let stance_time = crate::world_objects::creature_combat::set_combat_mode(
            w,
            player,
            CombatMode::NonCombat,
        );
        action_chain.add_delay_seconds(w, f64::from(stance_time));

        anim_time += stance_time;
    }

    // perform clapping motion
    anim_time += crate::world_objects::world_object_networking::enqueue_motion(
        w,
        player,
        &mut action_chain,
        MotionCommand::ClapHands,
        1.0,
        true,
        None,
        false,
        false,
    );

    action_chain.add_action(Actor::Object(player), move |w: &mut World| {
        // re-verify
        let use_error = verify_use_requirements(w, player, spirit, this);
        if use_error != WeenieError::None {
            send_use_done_event(w, player, use_error);
            crate::world_objects::player_magic::set_is_busy(w, player, false);
            return;
        }

        let (structure, max_structure) =
            (object(w, this).structure(), object(w, this).max_structure());
        if structure == max_structure {
            send_chat(w, player, "This essence is already full.");
            send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
            crate::world_objects::player_magic::set_is_busy(w, player, false);
            return;
        }

        crate::world_objects::player_properties::update_property_int(
            w,
            player,
            this,
            PropertyInt::Structure,
            max_structure.map(i32::from),
            false,
        );

        crate::world_objects::player_inventory::try_consume_from_inventory_with_networking(
            w, player, spirit, 1,
        );

        send_chat(w, player, "You add the spirit to the essence.");

        send_use_done_event(w, player, WeenieError::None);

        crate::world_objects::player_magic::set_is_busy(w, player, false);
    });

    crate::world_objects::world_object_networking::enqueue_motion(
        w,
        player,
        &mut action_chain,
        MotionCommand::Ready,
        1.0,
        true,
        None,
        false,
        false,
    );

    action_chain.enqueue_chain(w);

    let next = w.now.utc.add_seconds(f64::from(anim_time));
    crate::world_objects::player_combat::set_next_use_time(w, player, next);
}

// ACE: PetDevice.VerifyUseRequirements
/// Both the spirit and the device must be in the player's inventory.
pub fn verify_use_requirements(
    w: &World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) -> WeenieError {
    // ensure target is summoning essence? source.TargetType is Misc

    // ensure both source and target are in player's inventory
    if find_object(w, player, source, SearchLocations::MyInventory)
        .result
        .is_none()
    {
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    if find_object(w, player, target, SearchLocations::MyInventory)
        .result
        .is_none()
    {
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    WeenieError::None
}

// ---- constructors and SetEphemeralValues ----

/// `new PetDevice(weenie, guid)` / `new PetDevice(biota)`: the `WorldObject` constructor, then
/// PetDevice's `SetEphemeralValues`.
// ACE: PetDevice.PetDevice
pub fn pet_device_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    let from_biota = src.is_biota();
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    pet_device_set_ephemeral_values(o, env);

    if !from_biota {
        // todo: remove me when the data is fixed
        o.set_structure(o.max_structure());
    }
}

/// Empty in ACE.
// ACE: PetDevice.SetEphemeralValues
fn pet_device_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
