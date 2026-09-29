// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/ManaStone.cs
//! Port of `Source/ACE.Server/WorldObjects/ManaStone.cs`: draining mana from an item into a stone,
//! and pouring a charged stone into equipped items or one item.

use empyrean_common::dotnet::math::round as math_round;
use empyrean_common::dotnet::{format as dotnet_format, CsCast};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{ChatMessageType, PropertyInt, UiEffects, WeenieError};
use empyrean_entity::ObjectGuid;

use crate::managers::player_manager::player_session;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::player_inventory::{self, DequipObjectAction, SearchLocations};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_equipment, creature_rating, player_use, world_object_networking,
};
use crate::World;

/// Non-property fields declared in `ManaStone.cs`.
#[derive(Debug, Default)]
pub struct ManaStoneFields {}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects
        .get(g)
        .expect("ACE: object is null (NullReferenceException)")
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(g)
        .expect("ACE: object is null (NullReferenceException)")
}

fn name(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

fn send_chat(w: &mut World, player: ObjectGuid, text: &str) {
    let session =
        player_session(w, player).expect("ACE: player.Session is null (NullReferenceException)");
    enqueue_send(
        w,
        session,
        game_message_system_chat(text, ChatMessageType::Broadcast),
    );
}

/// `ItemCurMana < ItemMaxMana` over two nullable ints (false when either is null).
fn needs_mana(o: &WorldObject) -> bool {
    matches!((o.item_cur_mana(), o.item_max_mana()), (Some(cur), Some(max)) if cur < max)
}

// ACE: ManaStone.SetUiEffect
pub fn set_ui_effect(w: &mut World, this: ObjectGuid, player: ObjectGuid, effect: UiEffects) {
    let o = obj_mut(w, this);
    o.set_ui_effects(Some(effect));
    let msg =
        game_message_public_update_property_int(o, PropertyInt::UiEffects, effect.0.cs_cast());
    world_object_networking::enqueue_broadcast(w, player, true, &[msg]);
}

// ---- virtual-dispatch targets ----

// ACE: ManaStone.HandleActionUseOnTarget
#[allow(clippy::too_many_lines)] // ACE's one method
pub fn mana_stone_handle_action_use_on_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    let mut target = target;

    let mut use_result = WeenieError::None;

    if crate::entity::damage_history_info::player_is_olthoi_player(w, player) {
        player_use::send_use_done_event(w, player, WeenieError::OlthoiCannotInteractWithThat);
        return;
    }

    if player != target {
        let found = player_inventory::find_object(
            w,
            player,
            target,
            SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
        );
        let Some(inv_target) = found.result else {
            // Haven't looked to see if an error was sent for this case; however, this one fits
            player_use::send_use_done_event(w, player, WeenieError::YouDoNotOwnThatItem);
            return;
        };

        target = inv_target;
    }

    let item_cur_mana = obj(w, this).item_cur_mana();
    if item_cur_mana.is_none() {
        let t = obj(w, target);
        if target == player {
            use_result = WeenieError::ActionCancelled;
        } else if t.item_cur_mana().is_some_and(|m| m > 0)
            && t.item_max_mana().is_some_and(|m| m > 0)
        {
            // absorb mana from the item
            if t.retained() {
                use_result = WeenieError::ActionCancelled;
            } else {
                // ACE reads the consumed target's mana and name after the removal from its
                // still-live object; they do not change, so they are read here first (a consumed
                // item leaves the store).
                let target_cur_mana = t.item_cur_mana().expect("ACE: target.ItemCurMana.Value");
                let target_name = name(w, target);

                if !player_inventory::try_consume_from_inventory_with_networking(
                    w, player, target, 1,
                ) && player_inventory::try_dequip_object_with_networking(
                    w,
                    player,
                    target,
                    DequipObjectAction::ConsumeItem,
                )
                .is_none()
                {
                    log::error!("Failed to remove {target_name} from player inventory.");
                    player_use::send_use_done_event(w, player, WeenieError::ActionCancelled);
                    return;
                }

                //The Mana Stone drains 5,253 points of mana from the Wand.
                //The Wand is destroyed.

                //The Mana Stone drains 4,482 points of mana from the Pantaloons.
                //The Pantaloons is destroyed.

                let efficiency = obj(w, this)
                    .efficiency()
                    .expect("ACE: Efficiency.Value (InvalidOperationException)");
                let mana_drained: i32 =
                    math_round(efficiency * f64::from(target_cur_mana)).cs_cast();
                obj_mut(w, this).set_item_cur_mana(Some(mana_drained));
                let msg = format!(
                    "The Mana Stone drains {} points of mana from the {target_name}.\nThe {target_name} is destroyed.",
                    dotnet_format(mana_drained, "N0")
                );
                send_chat(w, player, &msg);
                set_ui_effect(w, this, player, UiEffects::Magical);
            }
        } else {
            use_result = WeenieError::ItemDoesntHaveEnoughMana;
        }
    } else if item_cur_mana.is_some_and(|m| m > 0) {
        if target == player {
            // dump mana into equipped items
            let orig_items_needing_mana: Vec<ObjectGuid> =
                creature_equipment::equipped_objects_values(w, player)
                    .into_iter()
                    .filter(|&k| needs_mana(obj(w, k)))
                    .collect();
            // `Dictionary<WorldObject, int>`: insertion order, nothing is removed
            let mut items_given_mana: Vec<(ObjectGuid, i32)> = Vec::new();

            while obj(w, this).item_cur_mana().is_some_and(|m| m > 0) {
                let items_needing_mana: Vec<ObjectGuid> = orig_items_needing_mana
                    .iter()
                    .copied()
                    .filter(|&k| needs_mana(obj(w, k)))
                    .collect();
                if items_needing_mana.is_empty() {
                    break;
                }

                let count =
                    i32::try_from(items_needing_mana.len()).expect("a list count is an int");
                let ration =
                    (obj(w, this).item_cur_mana().expect("ItemCurMana.Value") / count).max(1);

                for item in items_needing_mana {
                    let (item_max, item_cur) =
                        (obj(w, item).item_max_mana(), obj(w, item).item_cur_mana());
                    let mana_needed_for_topoff: i32 = item_max
                        .expect("ItemMaxMana")
                        .wrapping_sub(item_cur.expect("ItemCurMana"));
                    let mut adjusted_ration = ration.min(mana_needed_for_topoff);

                    let cur = obj(w, this)
                        .item_cur_mana()
                        .map(|m| m.wrapping_sub(adjusted_ration));
                    obj_mut(w, this).set_item_cur_mana(cur);

                    let lum_aug_item_mana_gain = obj(w, player).lum_aug_item_mana_gain();
                    if lum_aug_item_mana_gain != 0 {
                        let ration_f: f32 = adjusted_ration.cs_cast();
                        adjusted_ration = math_round(f64::from(
                            ration_f
                                * creature_rating::get_positive_rating_mod(
                                    lum_aug_item_mana_gain.wrapping_mul(5),
                                ),
                        ))
                        .cs_cast();
                        if adjusted_ration > mana_needed_for_topoff {
                            let diff = adjusted_ration.wrapping_sub(mana_needed_for_topoff);
                            adjusted_ration = mana_needed_for_topoff;
                            let cur = obj(w, this).item_cur_mana().map(|m| m.wrapping_add(diff));
                            obj_mut(w, this).set_item_cur_mana(cur);
                        }
                    }

                    let new_item_cur = obj(w, item)
                        .item_cur_mana()
                        .map(|m| m.wrapping_add(adjusted_ration));
                    obj_mut(w, item).set_item_cur_mana(new_item_cur);
                    match items_given_mana.iter_mut().find(|(g, _)| *g == item) {
                        None => items_given_mana.push((item, adjusted_ration)),
                        Some((_, v)) => *v = v.wrapping_add(adjusted_ration),
                    }

                    // `ItemCurMana <= 0`: a null ItemCurMana compares false
                    if obj(w, this).item_cur_mana().is_some_and(|m| m <= 0) {
                        break;
                    }
                }
            }

            if items_given_mana.is_empty() {
                send_chat(w, player, "You have no items equipped that need mana.");
                use_result = WeenieError::ActionCancelled;
            } else {
                //The Mana Stone gives 4,496 points of mana to the following items: Fire Compound Crossbow, Qafiya, Celdon Sleeves, Amuli Leggings, Messenger's Collar, Heavy Bracelet, Scalemail Bracers, Olthoi Alduressa Gauntlets, Studded Leather Girth, Shoes, Chainmail Greaves, Loose Pants, Mechanical Scarab, Ring, Ring, Heavy Bracelet
                //Your items are fully charged.

                //The Mana Stone gives 1,921 points of mana to the following items: Haebrean Girth, Chiran Helm, Ring, Baggy Breeches, Scalemail Greaves, Alduressa Boots, Heavy Bracelet, Heavy Bracelet, Lorica Breastplate, Pocket Watch, Heavy Necklace
                //You need 2,232 more mana to fully charge your items.

                let additional_mana_needed =
                    orig_items_needing_mana.iter().fold(0i32, |acc, &k| {
                        let o = obj(w, k);
                        acc.wrapping_add(
                            o.item_max_mana()
                                .expect("ItemMaxMana.Value")
                                .wrapping_sub(o.item_cur_mana().expect("ItemCurMana.Value")),
                        )
                    });
                let additional_mana_text = if additional_mana_needed > 0 {
                    format!(
                        "\nYou need {} more mana to fully charge your items.",
                        dotnet_format(additional_mana_needed, "N0")
                    )
                } else {
                    "\nYour items are fully charged.".to_owned()
                };
                let given: i32 = items_given_mana
                    .iter()
                    .fold(0i32, |acc, (_, v)| acc.wrapping_add(*v));
                let names: Vec<String> =
                    items_given_mana.iter().map(|(g, _)| name(w, *g)).collect();
                let msg = format!(
                    "The Mana Stone gives {} points of mana to the following items: {}.{additional_mana_text}",
                    dotnet_format(given, "N0"),
                    names.join(", ")
                );
                send_chat(w, player, &msg);

                if !do_destroy_dice_roll(w, this, player) && !obj(w, this).unlimited_use() {
                    obj_mut(w, this).set_item_cur_mana(None);
                    set_ui_effect(w, this, player, UiEffects::Undef);
                }

                if let Some(o) = w.objects.get(this) {
                    if o.unlimited_use() {
                        if let Some(item_max_mana) = o.item_max_mana() {
                            obj_mut(w, this).set_item_cur_mana(Some(item_max_mana));
                        }
                    }
                }
            }
        } else if obj(w, target).item_max_mana().is_some_and(|m| m > 0) {
            let target_item_max_mana = obj(w, target).item_max_mana().expect("checked above");
            let target_item_cur_mana = obj(w, target).item_cur_mana().unwrap_or(0);

            if target_item_cur_mana >= target_item_max_mana {
                let msg = format!("The {} is already full of mana.", name(w, target));
                send_chat(w, player, &msg);
            } else {
                // The Mana Stone gives 3,502 points of mana to the Focusing Stone.

                // The Mana Stone gives 3,267 points of mana to the Protective Drudge Charm.

                let target_mana_needed = target_item_max_mana.wrapping_sub(target_item_cur_mana);
                let mut mana_to_pour = target_mana_needed
                    .min(obj(w, this).item_cur_mana().expect("ItemCurMana.Value"));

                let lum_aug_item_mana_gain = obj(w, player).lum_aug_item_mana_gain();
                if lum_aug_item_mana_gain != 0 {
                    let pour_f: f32 = mana_to_pour.cs_cast();
                    mana_to_pour = math_round(f64::from(
                        pour_f
                            * creature_rating::get_positive_rating_mod(
                                lum_aug_item_mana_gain.wrapping_mul(5),
                            ),
                    ))
                    .cs_cast();
                    mana_to_pour = target_mana_needed.min(mana_to_pour);
                }

                obj_mut(w, target)
                    .set_item_cur_mana(Some(target_item_cur_mana.wrapping_add(mana_to_pour)));
                let msg = format!(
                    "The Mana Stone gives {} points of mana to the {}.",
                    dotnet_format(mana_to_pour, "N0"),
                    name(w, target)
                );
                send_chat(w, player, &msg);

                if !do_destroy_dice_roll(w, this, player) && !obj(w, this).unlimited_use() {
                    obj_mut(w, this).set_item_cur_mana(None);
                    set_ui_effect(w, this, player, UiEffects::Undef);
                }
            }
        } else {
            use_result = WeenieError::ActionCancelled;
        }
    }

    player_use::send_use_done_event(w, player, use_result);
}

// ACE: ManaStone.DoDestroyDiceRoll
/// A null `DestroyChance` still draws (`null == 0` is false) and never destroys.
fn do_destroy_dice_roll(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> bool {
    let destroy_chance = obj(w, this).destroy_chance();
    if destroy_chance == Some(0.0) {
        return false;
    }

    let dice = ThreadSafeRandom::next_float(0.0, 1.0);

    if destroy_chance.is_some_and(|c| dice < c) {
        player_inventory::try_consume_from_inventory_with_networking(w, player, this, i32::MAX);
        {
            send_chat(w, player, "The Mana Stone is destroyed.");
            return true;
        }
    }

    false
}

// ---- constructors and SetEphemeralValues ----

/// `new ManaStone(weenie, guid)` / `new ManaStone(biota)`: the `WorldObject` constructor, then
/// ManaStone's `SetEphemeralValues`.
// ACE: ManaStone.ManaStone
pub fn mana_stone_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    mana_stone_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: ManaStone.SetEphemeralValues
fn mana_stone_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
