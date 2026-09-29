// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/CorePlating.cs
//! Port of `Source/ACE.Server/Entity/CorePlating.cs`.
//!
//! The pure statics, and the members that touch world objects (`IsCorePlatingDevice`,
//! `UseObjectOnTarget`, `VerifyUseRequirements`, `Integrate`, `Deintegrate`).
// http://acpedia.org/wiki/Announcements_-_2010/06_-_Shifting_Gears#Gear_Knights
// http://acpedia.org/wiki/Core_Plating_Integrator
// http://acpedia.org/wiki/Core_Plating_Deintegrator

use empyrean_entity::enums::{
    ChatMessageType, CombatMode, EquipMask, HeritageGroup, MotionCommand, PropertyDataId,
    PropertyInt, PropertyString, WeenieError,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::world_objects::player_inventory::{find_object, SearchLocations};
use crate::world_objects::player_properties::{
    update_property_data_id, update_property_int, update_property_string,
};
use crate::world_objects::player_use::send_use_done_event;
use crate::world_objects::world_object::WorldObject;
use crate::World;

// ACE: CorePlating.CorePlatingIntegrator
pub const CORE_PLATING_INTEGRATOR: u32 = 42979;
// ACE: CorePlating.CorePlatingDeintegrator
pub const CORE_PLATING_DEINTEGRATOR: u32 = 43022;

// ACE: CorePlating.IsIntegrator
#[must_use]
pub fn is_integrator(wcid: u32) -> bool {
    wcid == CORE_PLATING_INTEGRATOR
}

// ACE: CorePlating.IsDeintegrator
#[must_use]
pub fn is_deintegrator(wcid: u32) -> bool {
    wcid == CORE_PLATING_DEINTEGRATOR
}

/// `A | B | ...` of `EquipMask` members, usable as a pattern constant.
macro_rules! mask {
    ($($m:ident)|+) => { EquipMask(0 $(| EquipMask::$m.0)+) };
}

/// What a Gear Knight may wear only once integrated, and what the Core Plating Integrator accepts:
/// the client's clothing and armour locations **without the cloak** (V229).
///
/// Turbine's "Cloaks!" article for the Cloak of Darkness update (October 2011): "even Gearknights,
/// which cannot normally wear normal clothing or armor, can drape these cloaks ... no integrator is
/// required to wear these cloaks." ACE's `Clothing | Armor` (0x80007FFF) reaches the same answer
/// because its `Clothing` lacks the cloak bit; this is the same set, 0x7FFF, from retail's own
/// constants (`CLOTHING_LOC` 0x080001FF and `ARMOR_LOC` 0x7E00) with the cloak taken out.
pub const GEAR_KNIGHT_PLATED_LOCATIONS: EquipMask = EquipMask(
    (dereth_rules::slots::loc::CLOTHING & !dereth_rules::slots::loc::CLOAK)
        | dereth_rules::slots::loc::ARMOR,
);

const CUIRASS: EquipMask = mask!(ChestArmor | AbdomenArmor);
const CHEST_2: EquipMask = mask!(ChestArmor | AbdomenArmor | UpperArmArmor);
const SHIRT_1: EquipMask = mask!(ChestArmor | UpperArmArmor);
const SHIRT_2: EquipMask = mask!(HeadWear | ChestArmor | UpperArmArmor);
const HAUBERK: EquipMask = mask!(ChestArmor | AbdomenArmor | UpperArmArmor | LowerArmArmor);
const COAT_1: EquipMask = mask!(ChestArmor | LowerArmArmor | UpperArmArmor | HeadWear);
const COAT_2: EquipMask = mask!(ChestArmor | UpperArmArmor | LowerArmArmor);
const GIRTH_2: EquipMask = mask!(AbdomenArmor | UpperLegArmor);
const SLEEVE: EquipMask = mask!(UpperArmArmor | LowerArmArmor);
const LEG: EquipMask = mask!(UpperLegArmor | LowerLegArmor);
const PANTS: EquipMask = mask!(AbdomenArmor | UpperLegArmor | LowerLegArmor);
const SOLLERET_2: EquipMask = mask!(LowerLegWear | FootWear);
const BODY_2: EquipMask = mask!(Armor | HandWear);
const BODY_3: EquipMask = mask!(Armor | HeadWear | HandWear);
const BODY_4: EquipMask =
    mask!(ChestArmor | UpperArmArmor | LowerArmArmor | UpperLegArmor | LowerLegArmor);
const BODY_5: EquipMask = mask!(HeadWear | Armor);
const UNDERWEAR: EquipMask =
    mask!(ChestWear | AbdomenWear | UpperArmWear | LowerArmWear | UpperLegWear | LowerLegWear);

// ACE: CorePlating.GetGearPlatingName
/// The name a core plating gives a piece of gear covering `locations`.
#[must_use]
pub fn get_gear_plating_name(locations: EquipMask) -> String {
    let mut slot_name = "";
    let mut plating_type;

    // Underoos are called "Underplating". Just make sure we ignore shoes.
    if !(locations & UNDERWEAR).is_empty() && (locations & EquipMask::FootWear).is_empty() {
        plating_type = "Underplating";

        if !(locations & EquipMask::ChestWear).is_empty() {
            slot_name = " Upper Body ";
        } else {
            slot_name = " Lower Body ";
        }
    } else {
        plating_type = "Plating";

        match locations {
            EquipMask::HeadWear => slot_name = " Helm ",
            CUIRASS => slot_name = " Cuirass ",
            EquipMask::ChestArmor | CHEST_2 => slot_name = " Chest ",
            // No logs found for these exact combos, but "Shirt Mesh" was used for Chainmail Shirts,
            // which falls under "Coat" now
            SHIRT_1 | SHIRT_2 => {
                slot_name = " Shirt ";
                plating_type = "Mesh";
            }
            HAUBERK => slot_name = " Hauberk ",
            // COAT_1: No logs found for this combo
            COAT_1 | COAT_2 => slot_name = " Coat ",
            // GIRTH_2: No logs found for this combo, only currently applies to "Leather Shorts"
            // WCID 25650, which appears to be a bug in the data
            EquipMask::AbdomenArmor | GIRTH_2 => slot_name = " Girth ",
            EquipMask::UpperArmArmor => slot_name = " Pauldron ",
            EquipMask::LowerArmArmor => slot_name = " Bracer ",
            SLEEVE => slot_name = " Sleeve ",
            EquipMask::HandWear => slot_name = " Gauntlet ",
            EquipMask::UpperLegArmor => slot_name = " Tasset ",
            EquipMask::LowerLegArmor => slot_name = " Greaves ",
            LEG => slot_name = " Leg ",
            PANTS => {
                slot_name = " Pants ";
                plating_type = "Mesh";
            }
            EquipMask::FootWear | SOLLERET_2 => slot_name = " Solleret ",
            // `Armor` here is ACE's 0x7F00 (the six armour bits and FootWear): a data pattern, the
            // ValidLocations of 104 robes, not the client's ARMOR_LOC (0x7E00, which no item has).
            // BODY_2: No logs found for this combo, only Ursuin Guise. BODY_3: Guises/Costumes.
            // BODY_4: Swamp Lord's War Pain, WCID 27889.
            EquipMask::Armor | BODY_2 | BODY_3 | BODY_4 | BODY_5 => slot_name = " Body ",
            _ => {}
        }
    }

    format!("Core{slot_name}{plating_type}")
}

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

// ACE: CorePlating.IsCorePlatingDevice
#[must_use]
pub fn is_core_plating_device(wo: &WorldObject) -> bool {
    wo.weenie_class_id() == CORE_PLATING_INTEGRATOR
        || wo.weenie_class_id() == CORE_PLATING_DEINTEGRATOR
}

// ACE: CorePlating.UseObjectOnTarget
/// The player uses a Core Plating device on a piece of armor or clothing: a clap, then the
/// integration or deintegration.
pub fn use_object_on_target(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) {
    //Console.WriteLine($"CorePlating.UseObjectOnTarget({player.Name}, {source.Name}, {target.Name})");

    if crate::world_objects::player_magic::is_busy(w, player) {
        send_use_done_event(w, player, WeenieError::YoureTooBusy);
        return;
    }

    let allow_craft_in_combat =
        crate::managers::property_manager::get_bool(w, "allow_combat_mode_crafting", false, true)
            .item;

    if !allow_craft_in_combat
        && crate::world_objects::player_move::creature_combat_mode(w, player)
            != CombatMode::NonCombat
    {
        send_use_done_event(w, player, WeenieError::YouMustBeInPeaceModeToTrade);
        return;
    }

    // verify use requirements
    let use_error = verify_use_requirements(w, player, source, target);
    if use_error != WeenieError::None {
        send_use_done_event(w, player, use_error);
        return;
    }

    let motion_command = MotionCommand::ClapHands;

    let mut action_chain = ActionChain::new();
    let mut next_use_time = 0.0f32;

    crate::world_objects::player_magic::set_is_busy(w, player, true);

    if allow_craft_in_combat
        && crate::world_objects::player_move::creature_combat_mode(w, player)
            != CombatMode::NonCombat
    {
        // Drop out of combat mode.  This depends on the server property "allow_combat_mode_craft" being True.
        // If not, this action would have aborted due to not being in NonCombat mode.
        let stance_time = crate::world_objects::creature_combat::set_combat_mode(
            w,
            player,
            CombatMode::NonCombat,
        );
        action_chain.add_delay_seconds(w, f64::from(stance_time));

        next_use_time += stance_time;
    }

    let _motion = crate::network::motion::movement_data::Motion::from_world_object(
        w,
        player,
        motion_command,
        1.0,
    );
    let current_stance = object(w, player)
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
        .expect("ACE: CurrentMotionState is null (NullReferenceException)")
        .stance; // expected to be MotionStance.NonCombat
    let motion_table_id = object(w, player).motion_table_id();
    let clap_time = crate::physics::motion_table::get_animation_length(
        w,
        motion_table_id,
        current_stance,
        motion_command,
        1.0,
    );

    action_chain.add_action(Actor::Object(player), move |w: &mut World| {
        crate::world_objects::player_location::send_motion_as_commands(
            w,
            player,
            motion_command,
            current_stance,
        );
    });
    action_chain.add_delay_seconds(w, f64::from(clap_time));

    next_use_time += clap_time;

    action_chain.add_action(Actor::Object(player), move |w: &mut World| {
        // re-verify
        let use_error = verify_use_requirements(w, player, source, target);
        if use_error != WeenieError::None {
            send_use_done_event(w, player, use_error);
            return;
        }

        let source_wcid = object(w, source).weenie_class_id();
        if is_integrator(source_wcid) {
            integrate(w, player, source, target);
        } else if is_deintegrator(source_wcid) {
            deintegrate(w, player, source, target);
        } else {
            send_use_done_event(w, player, WeenieError::CraftGeneralErrorNoUiMsg);
        }
    });

    //player.EnqueueMotion(actionChain, MotionCommand.Ready);

    action_chain.add_action(Actor::Object(player), move |w: &mut World| {
        crate::world_objects::player_magic::set_is_busy(w, player, false)
    });

    action_chain.enqueue_chain(w);

    let next = w.now.utc.add_seconds(f64::from(next_use_time));
    crate::world_objects::player_combat::set_next_use_time(w, player, next);
}

// ACE: CorePlating.VerifyUseRequirements
pub fn verify_use_requirements(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) -> WeenieError {
    use crate::world_objects::player::send_message;
    use crate::world_objects::player_networking::send_transient_error;

    if source == target {
        let msg = format!("You can't use the {} on itself.", name(w, source));
        send_transient_error(w, player, &msg);
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

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

    let source_wcid = object(w, source).weenie_class_id();
    if source_wcid != CORE_PLATING_INTEGRATOR && source_wcid != CORE_PLATING_DEINTEGRATOR {
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    // `(target.ValidLocations & (Clothing | Armor)) == 0`: a null lifts to false. ACE's composites,
    // as the named set they amount to (a cloak is not a target; see GEAR_KNIGHT_PLATED_LOCATIONS).
    if object(w, target)
        .valid_locations()
        .is_some_and(|v| (v & GEAR_KNIGHT_PLATED_LOCATIONS).is_empty())
    {
        let msg = format!(
            "You can't use the {} on {} because it is not a piece of armor or clothing.",
            name(w, source),
            name(w, target)
        );
        send_transient_error(w, player, &msg);
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    let heritage_specific_armor =
        object(w, target).get_property(PropertyInt::HeritageSpecificArmor);

    if is_integrator(source_wcid) {
        if heritage_specific_armor == Some(HeritageGroup::Gearknight.0) {
            //player.SendTransientError($"This armor has already been integrated into gear plating.");
            send_message(
                w,
                player,
                "This armor has already been integrated into gear plating.",
                ChatMessageType::Broadcast,
            );
            return WeenieError::YouDoNotPassCraftingRequirements;
        }

        if heritage_specific_armor.is_some_and(|h| h > 0) {
            //player.SendTransientError($"This armor cannot be integrated into gear plating as it is created specifically for another race.");
            send_message(
                w,
                player,
                "This armor cannot be integrated into gear plating as it is created specifically for another race.",
                ChatMessageType::Broadcast,
            );
            return WeenieError::YouDoNotPassCraftingRequirements;
        }
    } else if is_deintegrator(source_wcid)
        && heritage_specific_armor != Some(HeritageGroup::Gearknight.0)
    {
        //player.SendTransientError($"This armor has not been integrated into gear plating.");
        send_message(
            w,
            player,
            "This armor has not been integrated into gear plating.",
            ChatMessageType::Broadcast,
        );
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    WeenieError::None
}

// ACE: CorePlating.CorePlatingGearOverlay
pub const CORE_PLATING_GEAR_OVERLAY: u32 = 0x0600_6D70;

// ACE: CorePlating.Integrate
pub fn integrate(w: &mut World, player: ObjectGuid, _source: ObjectGuid, target: ObjectGuid) {
    crate::world_objects::player::send_message(
        w,
        player,
        "Your integrator forges the piece into gear plating for a Gear Knight.",
        ChatMessageType::Broadcast,
    );

    // `IconOverlayId != 0`: a null overlay is not 0, so it copies the null
    let icon_overlay_id = object(w, target).icon_overlay_id();
    if icon_overlay_id != Some(0) {
        w.objects
            .get_mut(target)
            .expect("ACE: target")
            .set_icon_overlay_secondary(icon_overlay_id);
    }

    // ValidLocations has already been verified prior to this, so we can be safe casting it -- it won't null
    let valid_locations = object(w, target)
        .valid_locations()
        .expect("ACE: ValidLocations is null (InvalidOperationException)");
    let gear_plating_name = get_gear_plating_name(valid_locations);

    update_property_int(
        w,
        player,
        target,
        PropertyInt::HeritageSpecificArmor,
        Some(HeritageGroup::Gearknight.0),
        false,
    );
    update_property_data_id(
        w,
        player,
        target,
        PropertyDataId::IconOverlay,
        Some(CORE_PLATING_GEAR_OVERLAY),
        false,
    );
    update_property_string(
        w,
        player,
        target,
        PropertyString::GearPlatingName,
        Some(&gear_plating_name),
        false,
    );
    update_property_string(
        w,
        player,
        target,
        PropertyString::Use,
        Some("This Aetherium core plating installs into the frame of a Gear Knight to strengthen it."),
        false,
    );

    crate::dispatch::save_biota_to_database::save_biota_to_database(w, target, true);

    send_use_done_event(w, player, WeenieError::None);
}

// ACE: CorePlating.Deintegrate
pub fn deintegrate(w: &mut World, player: ObjectGuid, _source: ObjectGuid, target: ObjectGuid) {
    crate::world_objects::player::send_message(
        w,
        player,
        "Your deintegrator restores the original form of this piece of gear.",
        ChatMessageType::Broadcast,
    );

    update_property_int(
        w,
        player,
        target,
        PropertyInt::HeritageSpecificArmor,
        None,
        false,
    );
    if object(w, target).icon_overlay_id() == Some(CORE_PLATING_GEAR_OVERLAY) {
        let secondary = object(w, target).icon_overlay_secondary();
        update_property_data_id(
            w,
            player,
            target,
            PropertyDataId::IconOverlay,
            secondary,
            false,
        );
    }
    update_property_string(
        w,
        player,
        target,
        PropertyString::GearPlatingName,
        None,
        false,
    );

    let target_weenie = w
        .content
        .get_cached_weenie(object(w, target).weenie_class_id());

    match target_weenie {
        Some(target_weenie) => {
            let use_string = target_weenie.get_property(PropertyString::Use);
            update_property_string(
                w,
                player,
                target,
                PropertyString::Use,
                use_string.as_deref(),
                false,
            );
        }
        None => update_property_string(w, player, target, PropertyString::Use, None, false),
    }

    if object(w, target).icon_overlay_secondary().is_some() {
        w.objects
            .get_mut(target)
            .expect("ACE: target")
            .set_icon_overlay_secondary(None);
    }

    crate::dispatch::save_biota_to_database::save_biota_to_database(w, target, true);

    send_use_done_event(w, player, WeenieError::None);
}
