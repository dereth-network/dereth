// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Tailoring.cs
//! Port of `Source/ACE.Server/Entity/Tailoring.cs`.
//!
//! The tailoring kits: taking the look off one piece of armor or weapon into an intermediate kit,
//! applying a kit to another piece, and the reduction and layering tools, with the pure statics
//! and the weenie class constants.
// http://acpedia.org/wiki/Tailoring
// https://asheron.fandom.com/wiki/Tailoring

use std::sync::LazyLock;

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{
    ChatMessageType, CombatMode, CoverageMask, DamageType, EquipMask, ItemType, MotionCommand,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInt, PropertyString, WeaponType,
    WeenieError,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::items_to_receive::ItemsToReceive;
use crate::managers::player_manager::player_session;
use crate::managers::{property_manager, recipe_manager};
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::game_messages::messages::game_message_update_object::game_message_update_object;
use crate::world_objects::player_inventory::{self, SearchLocations};
use crate::world_objects::world_object::{self, WorldObject};
use crate::world_objects::{creature_combat, player_combat, world_object_networking};
use crate::{dispatch, World};

// tailoring kits
// ACE: Tailoring.ArmorTailoringKit
pub const ARMOR_TAILORING_KIT: u32 = 41956;
// ACE: Tailoring.WeaponTailoringKit
pub const WEAPON_TAILORING_KIT: u32 = 51445;

// ACE: Tailoring.ArmorMainReductionTool
pub const ARMOR_MAIN_REDUCTION_TOOL: u32 = 42622;
// ACE: Tailoring.ArmorLowerReductionTool
pub const ARMOR_LOWER_REDUCTION_TOOL: u32 = 44879;
// ACE: Tailoring.ArmorMiddleReductionTool
pub const ARMOR_MIDDLE_REDUCTION_TOOL: u32 = 44880;

// ACE: Tailoring.ArmorLayeringToolTop
pub const ARMOR_LAYERING_TOOL_TOP: u32 = 42724;
// ACE: Tailoring.ArmorLayeringToolBottom
pub const ARMOR_LAYERING_TOOL_BOTTOM: u32 = 42726;

// intermediates
// ACE: Tailoring.LeatherVest
pub const LEATHER_VEST: u32 = 42403;
// ACE: Tailoring.WingedCoat
pub const WINGED_COAT: u32 = 42405;
// ACE: Tailoring.PlatemailGauntlets
pub const PLATEMAIL_GAUNTLETS: u32 = 42407;
// ACE: Tailoring.YoroiGirth
pub const YOROI_GIRTH: u32 = 42409;
// ACE: Tailoring.YoroiGreaves
pub const YOROI_GREAVES: u32 = 42411;
// ACE: Tailoring.Heaume
pub const HEAUME: u32 = 42414;
// ACE: Tailoring.YoroiLeggings
pub const YOROI_LEGGINGS: u32 = 42416;
// ACE: Tailoring.AmuliLeggings
pub const AMULI_LEGGINGS: u32 = 42417;
// ACE: Tailoring.YoroiPauldrons
pub const YOROI_PAULDRONS: u32 = 42418;
// ACE: Tailoring.CeldonSleeves
pub const CELDON_SLEEVES: u32 = 42421;
// ACE: Tailoring.LeatherBoots
pub const LEATHER_BOOTS: u32 = 42422;
// ACE: Tailoring.Tentacles
pub const TENTACLES: u32 = 44863;
// ACE: Tailoring.DarkHeart
pub const DARK_HEART: u32 = 51451;

const FOOT_AND_LOWER_LEG: EquipMask = EquipMask(EquipMask::FootWear.0 | EquipMask::LowerLegWear.0);

// ACE: Tailoring.GetArmorWCID
/// The intermediate tailoring weenie for an item's valid locations, or `None`.
#[must_use]
pub fn get_armor_wcid(valid_locations: EquipMask) -> Option<u32> {
    match valid_locations {
        EquipMask::HeadWear => return Some(HEAUME),
        EquipMask::HandWear => return Some(PLATEMAIL_GAUNTLETS),
        EquipMask::FootWear | FOOT_AND_LOWER_LEG => return Some(LEATHER_BOOTS),
        EquipMask::ChestArmor => return Some(LEATHER_VEST),
        EquipMask::AbdomenArmor => return Some(YOROI_GIRTH),
        EquipMask::UpperArmArmor => return Some(YOROI_PAULDRONS),
        EquipMask::LowerArmArmor => return Some(CELDON_SLEEVES),
        EquipMask::UpperLegArmor => return Some(YOROI_GREAVES),
        EquipMask::LowerLegArmor => return Some(YOROI_LEGGINGS),
        _ => {}
    }

    // `HasFlag`: every bit of the flag is set.
    if has_flag(valid_locations, EquipMask::ChestArmor)
        || has_flag(valid_locations, EquipMask::UpperArmArmor)
    {
        return Some(WINGED_COAT);
    }
    if has_flag(valid_locations, EquipMask::AbdomenArmor)
        || has_flag(valid_locations, EquipMask::UpperLegArmor)
    {
        return Some(AMULI_LEGGINGS);
    }

    if has_flag(valid_locations, EquipMask::Armor)
        || valid_locations == EquipMask::Cloak
        || valid_locations == EquipMask::Shield
        || has_flag(valid_locations, EquipMask::ChestWear)
        || has_flag(valid_locations, EquipMask::AbdomenWear)
    {
        return Some(TENTACLES);
    }

    None
}

/// `Enum.HasFlag`: `(value & flag) == flag`.
fn has_flag(value: EquipMask, flag: EquipMask) -> bool {
    value.0 & flag.0 == flag.0
}

// ACE: Tailoring.IsTailoringKit
/// Returns TRUE if the input wcid is a tailoring kit.
#[must_use]
pub fn is_tailoring_kit(wcid: u32) -> bool {
    matches!(
        wcid,
        ARMOR_TAILORING_KIT
            | WEAPON_TAILORING_KIT
            | ARMOR_MAIN_REDUCTION_TOOL
            | ARMOR_LOWER_REDUCTION_TOOL
            | ARMOR_MIDDLE_REDUCTION_TOOL
            | ARMOR_LAYERING_TOOL_TOP
            | ARMOR_LAYERING_TOOL_BOTTOM
            | HEAUME
            | PLATEMAIL_GAUNTLETS
            | LEATHER_BOOTS
            | LEATHER_VEST
            | YOROI_GIRTH
            | YOROI_PAULDRONS
            | CELDON_SLEEVES
            | YOROI_GREAVES
            | YOROI_LEGGINGS
            | AMULI_LEGGINGS
            | WINGED_COAT
            | TENTACLES
            | DARK_HEART
    )
}

// ACE: Tailoring.log
// (log4net; the `log` crate here.)

// ACE: Tailoring.ArmorOverlayIcons
/// Some WCIDs have Overlay Icons that need to be removed (e.g. Olthoi Alduressa Gauntlets or Boots)
/// There are other examples not here, like some stamped shields that might need to be added, as well.
static ARMOR_OVERLAY_ICONS: LazyLock<DotNetDict<u32, i32>> = LazyLock::new(|| {
    let rows: [(u32, i32); 21] = [
        // These are from cache.bin
        (22551, 100_673_784), // Atlatl Tattoo
        (22552, 100_673_758), // Axe Tattoo
        (22553, 100_673_759), // Bow Tattoo
        (22554, 100_673_762), // Crossbow Tattoo
        (22555, 100_673_763), // Dagger Tattoo
        (22556, 100_673_774), // Mace Tattoo
        (22557, 100_673_775), // Magic Defense Tattoo
        (22558, 100_673_777), // Mana Conversion Tattoo
        (22559, 100_673_778), // Melee Defense Tattoo
        (22560, 100_673_779), // Missile Defense Tattoo
        (22561, 100_673_781), // Spear Tattoo
        (22562, 100_673_782), // Staff Tattoo
        (22563, 100_673_783), // Sword Tattoo
        (22564, 100_673_785), // Unarmed Tattoo
        (31394, 100_691_319), // Circle of Raven Might
        // These items were stampable and could have had a number of different icons
        (25811, 0), // Shield of Power
        (25843, 0), // Nefane Shield
        // From pcaps
        (37187, 100_690_144), // Olthoi Alduressa Gauntlets
        (37207, 100_690_146), // Olthoi Alduressa Boots
        (41198, 100_690_144), // Gauntlets of Darkness
        (41201, 100_690_146), // Sollerets of Darkness
    ];
    let mut d = DotNetDict::new();
    for (k, v) in rows {
        d.add(k, v);
    }
    d
});

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

fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

/// `player.Session.Network.EnqueueSend(new GameMessageSystemChat(message, type))`.
fn system_chat(
    w: &mut World,
    player: ObjectGuid,
    message: &str,
    chat_message_type: ChatMessageType,
) {
    if let Some(s) = player_session(w, player) {
        enqueue_send(w, s, game_message_system_chat(message, chat_message_type));
    }
}

/// `new GameEventCommunicationTransientString(player.Session, message)`, sent.
fn transient(w: &mut World, player: ObjectGuid, message: &str) {
    let Some(s) = player_session(w, player) else {
        return;
    };
    let Some(data) = w.sessions.get_mut(s) else {
        return;
    };
    let m = game_event_communication_transient_string(data, message);
    enqueue_send(w, s, m);
}

fn send_use_done_event(w: &mut World, player: ObjectGuid, error: WeenieError) {
    recipe_manager::shims::send_use_done_event(w, player, error);
}

// ACE: Tailoring.UseObjectOnTarget
/// thanks for phenyl naphthylamine for a lot the initial work here!
pub fn use_object_on_target(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) {
    //Console.WriteLine($"Tailoring.UseObjectOnTarget({player.Name}, {source.Name}, {target.Name})");

    // verify use requirements
    let use_error = verify_use_requirements(w, player, source, target);
    if use_error != WeenieError::None {
        send_use_done_event(w, player, use_error);
        return;
    }

    let mut anim_time = 0.0f32;

    let mut action_chain = ActionChain::new();

    // handle switching to peace mode
    if creature_combat::combat_mode(w, player) != CombatMode::NonCombat {
        let stance_time = creature_combat::set_combat_mode(w, player, CombatMode::NonCombat);
        action_chain.add_delay_seconds(w, f64::from(stance_time));

        anim_time += stance_time;
    }

    // perform clapping motion
    anim_time += world_object_networking::enqueue_motion(
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

    action_chain.add_action(Actor::Object(player), move |w| {
        // re-verify
        // (A source or target destroyed meanwhile is no longer in the player's inventory.)
        let use_error = if w.objects.get(source).is_none() || w.objects.get(target).is_none() {
            WeenieError::YouDoNotPassCraftingRequirements
        } else {
            verify_use_requirements(w, player, source, target)
        };
        if use_error != WeenieError::None {
            send_use_done_event(w, player, use_error);
            return;
        }

        do_tailoring(w, player, source, target);
    });

    action_chain.enqueue_chain(w);

    let t = w.now.utc.add_seconds(f64::from(anim_time));
    player_combat::set_next_use_time(w, player, t);
}

// ACE: Tailoring.VerifyUseRequirements
/// Both items in the player's inventory, the target not retained, neither society armor.
pub fn verify_use_requirements(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) -> WeenieError {
    if source == target {
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    // ensure both source and target are in player's inventory
    if player_inventory::find_object(w, player, source, SearchLocations::MyInventory)
        .result
        .is_none()
    {
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    if player_inventory::find_object(w, player, target, SearchLocations::MyInventory)
        .result
        .is_none()
    {
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    // verify not retained item
    if obj(w, target).retained() {
        system_chat(
            w,
            player,
            "You must use Sandstone Salvage to remove the retained property before tailoring.",
            ChatMessageType::Craft,
        );
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    // verify not society armor
    if obj(w, source).is_society_armor() || obj(w, target).is_society_armor() {
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    WeenieError::None
}

// ACE: Tailoring.DoTailoring
/// Dispatches on the kit.
pub fn do_tailoring(w: &mut World, player: ObjectGuid, source: ObjectGuid, target: ObjectGuid) {
    match obj(w, source).biota.weenie_class_id {
        ARMOR_TAILORING_KIT => {
            tailor_armor(w, player, source, target);
            return;
        }

        WEAPON_TAILORING_KIT => {
            tailor_weapon(w, player, source, target);
            return;
        }

        ARMOR_MAIN_REDUCTION_TOOL | ARMOR_LOWER_REDUCTION_TOOL | ARMOR_MIDDLE_REDUCTION_TOOL => {
            tailor_reduce_armor(w, player, source, target);
            return;
        }

        ARMOR_LAYERING_TOOL_TOP | ARMOR_LAYERING_TOOL_BOTTOM => {
            tailor_layer_armor(w, player, source, target);
            return;
        }

        // intermediates
        HEAUME              // helm
        | PLATEMAIL_GAUNTLETS // gauntlets
        | LEATHER_BOOTS       // boots
        | LEATHER_VEST        // breastplate
        | YOROI_GIRTH         // girth
        | YOROI_PAULDRONS     // pauldrons
        | CELDON_SLEEVES      // vambraces
        | YOROI_GREAVES       // tassets
        | YOROI_LEGGINGS      // greaves
        | AMULI_LEGGINGS      // lower-body multislot
        | WINGED_COAT         // upper-body multislot
        | TENTACLES => {
            // clothing or shield
            armor_apply(w, player, source, target);
            return;
        }

        DARK_HEART => {
            weapon_apply(w, player, source, target);
            return;
        }

        _ => {}
    }

    send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
}

// ACE: Tailoring.TailorArmor
/// Consumes the source armor, and creates an intermediate tailoring kit
/// to apply to the destination armor
pub fn tailor_armor(w: &mut World, player: ObjectGuid, source: ObjectGuid, target: ObjectGuid) {
    //Console.WriteLine($"TailorArmor({player.Name}, {source.Name}, {target.Name})");

    let wcid = get_armor_wcid(obj(w, target).valid_locations().unwrap_or(EquipMask::None));
    let Some(wcid) = wcid else {
        send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
        return;
    };

    let (source_wcid, target_wcid) = (
        obj(w, source).biota.weenie_class_id,
        obj(w, target).biota.weenie_class_id,
    );
    if !has_available_space(w, player, source_wcid, target_wcid, wcid) {
        send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
        return;
    }

    let wo = recipe_manager::shims::create_new_world_object(w, wcid)
        .expect("System.NullReferenceException: CreateNewWorldObject returned null");

    set_armor_properties(w, target, wo);

    system_chat(
        w,
        player,
        "You tailor the appearance off an existing piece of armor.",
        ChatMessageType::Broadcast,
    );

    finalize(w, player, source, target, wo);
}

// ACE: Tailoring.SetCommonProperties
/// Copies the look of `source` onto `target` (an intermediate kit).
pub fn set_common_properties(w: &mut World, source: ObjectGuid, target: ObjectGuid) {
    // a lot of this was probably done with recipes and mutations in the original
    // here a lot is done directly in code..

    let copy_ui_effects =
        property_manager::get_bool(w, "tailoring_intermediate_uieffects", false, true).item;
    let s = obj(w, source);
    let palette_template = s.palette_template();
    let ui_effects = s.ui_effects();
    let material_type = s.material_type();
    let obj_scale = s.obj_scale();
    let shade = s.shade();
    let shade2 = s.shade2();
    let shade3 = s.shade3();
    let shade4 = s.shade4();
    let lights_status = s.lights_status();
    let translucency = s.translucency();
    let setup_table_id = s.setup_table_id();
    let palette_base_id = s.palette_base_id();
    let clothing_base = s.clothing_base();
    let physics_table_id = s.physics_table_id();
    let sound_table_id = s.sound_table_id();
    let ignore_clo_icons = s.ignore_clo_icons();
    let icon_id = s.icon_id();
    let source_name = dispatch::name::name(w, source);

    let t = obj_mut(w, target);
    t.set_palette_template(palette_template);
    if copy_ui_effects {
        t.set_ui_effects(ui_effects);
    }
    t.set_material_type(material_type);

    t.set_obj_scale(obj_scale);

    t.set_shade(shade);

    // This might not even be needed, but we'll do it anyways
    t.set_shade2(shade2);
    t.set_shade3(shade3);
    t.set_shade4(shade4);

    crate::world_objects::world_object_properties::set_lights_status(w, target, lights_status);
    let t = obj_mut(w, target);
    t.set_translucency(translucency);

    t.set_setup_table_id(setup_table_id);
    t.set_palette_base_id(palette_base_id);
    t.set_clothing_base(clothing_base);

    t.set_physics_table_id(physics_table_id);
    t.set_sound_table_id(sound_table_id);

    // `target.Name = source.Name`
    match source_name {
        Some(n) => t.set_property(PropertyString::Name, n),
        None => t.remove_property(PropertyString::Name),
    }
    let long_desc = crate::factories::loot_generation_factory::get_long_desc(t);
    t.set_long_desc(long_desc);

    t.set_ignore_clo_icons(ignore_clo_icons);
    t.set_icon_id(icon_id);
}

// ACE: Tailoring.SetArmorProperties
/// The common look plus the armor's layering; a stashed overlay icon rides along.
pub fn set_armor_properties(w: &mut World, source: ObjectGuid, target: ObjectGuid) {
    set_common_properties(w, source, target);

    let s = obj(w, source);
    let clothing_priority = s.clothing_priority();
    let dyable = s.dyable();
    let source_wcid = s.biota.weenie_class_id;
    let icon_overlay_id = s.icon_overlay_id();

    let t = obj_mut(w, target);
    // ensure armor/clothing that covers head/hands/feet are cross-compatible
    // for something like shirt/breastplate, this will still be be prevented with ClothingPriority / CoverageMask check
    // (Outerwear vs. Underwear)
    t.set_target_type(Some(ItemType(ItemType::Armor.0 | ItemType::Clothing.0)));

    t.set_clothing_priority(clothing_priority);
    t.set_dyable(dyable);

    // If this source item is one of the icons that contains an icon overlay as part of it, we will stash that icon in the
    // IconOverlaySecondary slot (it is unused) to be applied on the next step.
    if let Some(icon_overlay_id) =
        icon_overlay_id.filter(|_| ARMOR_OVERLAY_ICONS.contains_key(&source_wcid))
    {
        t.set_property(PropertyDataId::IconOverlaySecondary, icon_overlay_id);
    }

    // ObjDescOverride.Clear()
}

// ACE: Tailoring.SetWeaponProperties
/// Applies the weapon properties to an in-between tailoring item, ready to be applied to a new weapon.
pub fn set_weapon_properties(w: &mut World, source: ObjectGuid, target: ObjectGuid) {
    set_common_properties(w, source, target);

    let s = obj(w, source);
    let item_type = s.item_type();
    let hook_type = s.hook_type();
    let hook_placement = s.hook_placement();
    let is_melee = s.is_melee_weapon();
    let is_missile_launcher = s.is_missile_launcher();
    let default_combat_style = s.default_combat_style();
    let w_attack_type = s.w_attack_type();
    let w_weapon_type = s.w_weapon_type();
    let w_damage_type = s.w_damage_type();

    let t = obj_mut(w, target);
    t.set_target_type(Some(item_type));

    t.set_hook_type(hook_type);
    t.set_hook_placement(hook_placement);

    // These values are all set just for verification purposes. Likely originally handled by unique WCID and recipe system.
    if is_melee {
        t.set_default_combat_style(default_combat_style); // unused currently, keeping this around in case its needed..
        t.set_w_attack_type(w_attack_type);
        t.set_w_weapon_type(w_weapon_type);
    } else if is_missile_launcher {
        t.set_default_combat_style(default_combat_style);
    }

    t.set_w_damage_type(w_damage_type);
}

// ACE: Tailoring.HasAvailableSpace
/// ensure player has enough free inventory slots / container slots / available burden to mutate items
pub fn has_available_space(
    w: &mut World,
    player: ObjectGuid,
    source_wcid: u32,
    target_wcid: u32,
    result_wcid: u32,
) -> bool {
    let mut items_to_receive = ItemsToReceive::new(w, player);

    items_to_receive.remove(w, source_wcid, 1);
    items_to_receive.remove(w, target_wcid, 1);
    items_to_receive.add(w, result_wcid, 1);

    if items_to_receive.player_exceeds_limits() {
        if items_to_receive.player_exceeds_available_burden() {
            transient(w, player, "You are too encumbered to tailor that!");
        } else if items_to_receive.player_out_of_inventory_slots() {
            transient(
                w,
                player,
                "You do not have enough pack space to tailor that!",
            );
        } else if items_to_receive.player_out_of_container_slots() {
            transient(
                w,
                player,
                "You do not have enough container slots to tailor that!",
            );
        }

        return false;
    }

    true
}

// ACE: Tailoring.Finalize
/// Consumes the kit and the donor item and gives the player the intermediate.
pub fn finalize(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
    result: ObjectGuid,
) {
    let (source_name, target_name) = (name(w, source), name(w, target));
    player_inventory::try_consume_from_inventory_with_networking(w, player, source, 1);
    player_inventory::try_consume_from_inventory_with_networking(w, player, target, 1);

    // errors shouldn't be possible here, since the items were pre-validated, but just in case...
    if player_inventory::try_create_in_inventory_with_networking(w, player, result).is_none() {
        log::error!(
            "[TAILORING] Tailoring.Finalize({} (0x{player}), {source_name} (0x{source}), {target_name} (0x{target}), {}) - couldn't add {} ({result}) to player inventory after validation, this shouldn't happen!",
            name(w, player),
            name(w, result),
            name(w, result)
        );
        world_object::destroy(w, result, true, false); // cleanup for guid manager
    }

    if property_manager::get_bool(w, "player_receive_immediate_save", false, true).item {
        shims::rush_next_player_save(w, player, 5);
    }

    send_use_done_event(w, player, WeenieError::None);
}

// ACE: Tailoring.TailorWeapon
/// Consumes the source weapon, and creates an intermediate tailoring kit
/// to apply to the destination weapon
pub fn tailor_weapon(w: &mut World, player: ObjectGuid, source: ObjectGuid, target: ObjectGuid) {
    //Console.WriteLine($"TailorWeapon({player.Name}, {source.Name}, {target.Name})");

    // ensure target is valid weapon
    let t = obj(w, target);
    if !t.is_melee_weapon() && !t.is_missile_launcher() && !t.is_caster() {
        send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
        return;
    }

    if t.is_melee_weapon() && t.w_weapon_type() == WeaponType::Undef {
        // 'difficult to master' weapons were not tailorable
        send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
        return;
    }

    let (source_wcid, target_wcid) = (
        obj(w, source).biota.weenie_class_id,
        obj(w, target).biota.weenie_class_id,
    );
    if !has_available_space(w, player, source_wcid, target_wcid, DARK_HEART) {
        send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
        return;
    }

    // create intermediate weapon tailoring kit
    let wo = recipe_manager::shims::create_new_world_object(w, DARK_HEART)
        .expect("System.NullReferenceException: CreateNewWorldObject returned null");
    set_weapon_properties(w, target, wo);

    system_chat(
        w,
        player,
        "You tailor the appearance off the weapon.",
        ChatMessageType::Broadcast,
    );

    finalize(w, player, source, target, wo);
}

// ACE: Tailoring.TailorReduceArmor
/// Reduces the coverage for a piece of armor
#[allow(clippy::collapsible_match)] // ACE's switch-then-if shape
pub fn tailor_reduce_armor(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) {
    //Console.WriteLine($"TailorReduceArmor({player.Name}, {source.Name}, {target.Name})");

    // Verify requirements - Can only reduce LootGen Armor
    if obj(w, target).item_workmanship().is_none() {
        send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
        return;
    }

    let valid_locations = obj(w, target).valid_locations().unwrap_or(EquipMask::None);
    let mut clothing_priority = CoverageMask::Unknown;

    let set_locations = |w: &mut World, m: EquipMask| {
        crate::world_objects::player_properties::update_property_int(
            w,
            player,
            target,
            PropertyInt::ValidLocations,
            Some(m.0.cast_signed()),
            false,
        );
    };

    match obj(w, source).biota.weenie_class_id {
        ARMOR_MAIN_REDUCTION_TOOL => {
            if has_flag(valid_locations, EquipMask::ChestArmor) {
                set_locations(w, EquipMask::ChestArmor);
                clothing_priority = CoverageMask::OuterwearChest;
            } else if has_flag(valid_locations, EquipMask::UpperArmArmor) {
                set_locations(w, EquipMask::UpperArmArmor);
                clothing_priority = CoverageMask::OuterwearUpperArms;
            } else if has_flag(valid_locations, EquipMask::AbdomenArmor) {
                set_locations(w, EquipMask::AbdomenArmor);
                clothing_priority = CoverageMask::OuterwearAbdomen;
            }
        }

        ARMOR_LOWER_REDUCTION_TOOL => {
            // Can't reduce Chest Armor to anything but chest!
            if has_flag(valid_locations, EquipMask::ChestArmor) {
                // break
            } else if has_flag(valid_locations, EquipMask::UpperArmArmor) {
                set_locations(w, EquipMask::LowerArmArmor);
                clothing_priority = CoverageMask::OuterwearLowerArms;
            } else if has_flag(valid_locations, EquipMask::UpperLegArmor) {
                set_locations(w, EquipMask::LowerLegArmor);
                clothing_priority = CoverageMask::OuterwearLowerLegs;
            } else if has_flag(
                valid_locations,
                EquipMask(EquipMask::LowerLegArmor.0 | EquipMask::FootWear.0),
            ) {
                set_locations(w, EquipMask::FootWear);
                clothing_priority = CoverageMask::Feet;
            }
        }

        ARMOR_MIDDLE_REDUCTION_TOOL => {
            if has_flag(valid_locations, EquipMask::UpperLegArmor) {
                set_locations(w, EquipMask::UpperLegArmor);
                clothing_priority = CoverageMask::OuterwearUpperLegs;
            }
        }

        _ => {}
    }

    if clothing_priority == CoverageMask::Unknown {
        send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
        return;
    }

    system_chat(
        w,
        player,
        "You modify your armor.",
        ChatMessageType::Broadcast,
    );

    crate::world_objects::player_properties::update_property_int(
        w,
        player,
        target,
        PropertyInt::ClothingPriority,
        Some(clothing_priority.0.cast_signed()),
        false,
    );
    player_inventory::try_consume_from_inventory_with_networking(w, player, source, 1);

    dispatch::save_biota_to_database::save_biota_to_database(w, target, true);

    send_use_done_event(w, player, WeenieError::None);
}

// ACE: Tailoring.TailorLayerArmor
/// Adjusts the layering priority for a piece of armor
pub fn tailor_layer_armor(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) {
    //Console.WriteLine($"TailorLayerArmor({player.Name}, {source.Name}, {target.Name})");

    let top_layer = obj(w, source).biota.weenie_class_id == ARMOR_LAYERING_TOOL_TOP;
    crate::world_objects::player_properties::update_property_bool(
        w,
        player,
        target,
        PropertyBool::TopLayerPriority,
        Some(top_layer),
        false,
    );

    player_inventory::try_consume_from_inventory_with_networking(w, player, source, 1);

    dispatch::save_biota_to_database::save_biota_to_database(w, target, true);

    send_use_done_event(w, player, WeenieError::None);
}

// ACE: Tailoring.ArmorApply
/// Applies an intermediate tailoring kit to a destination piece of armor
pub fn armor_apply(w: &mut World, player: ObjectGuid, source: ObjectGuid, target: ObjectGuid) {
    //Console.WriteLine($"ArmorApply({player.Name}, {source.Name}, {target.Name})");

    // verify armor type
    if obj(w, source).clothing_priority() != obj(w, target).clothing_priority() {
        send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
        return;
    }

    system_chat(
        w,
        player,
        "You tailor the appearance onto a different piece of armor.",
        ChatMessageType::Broadcast,
    );

    // update properties
    update_armor_props(w, player, source, target);

    // Send UpdateObject, mostly for the client to register the new name.
    let m = game_message_update_object(w, target, false, false);
    if let Some(s) = player_session(w, player) {
        enqueue_send(w, s, m);
    }

    player_inventory::try_consume_from_inventory_with_networking(w, player, source, 1);

    dispatch::save_biota_to_database::save_biota_to_database(w, target, true);

    send_use_done_event(w, player, WeenieError::None);
}

// ACE: Tailoring.WeaponApply
/// Applies an intermediate tailoring kit to a destination weapon
pub fn weapon_apply(w: &mut World, player: ObjectGuid, source: ObjectGuid, target: ObjectGuid) {
    //Console.WriteLine($"WeaponApply({player.Name}, {source.Name}, {target.Name})");

    let (s, t) = (obj(w, source), obj(w, target));

    // verify weapon type
    match s.target_type() {
        Some(ItemType::MeleeWeapon) => {
            if s.w_weapon_type() != t.w_weapon_type() || s.w_damage_type() != t.w_damage_type() {
                send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
                return;
            }
        }

        Some(ItemType::MissileWeapon) => {
            if s.default_combat_style() != t.default_combat_style()
                || s.w_damage_type() != DamageType::Undef && s.w_damage_type() != t.w_damage_type()
            {
                send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
                return;
            }
        }

        Some(ItemType::Caster) => {
            if s.w_damage_type() != DamageType::Undef && s.w_damage_type() != t.w_damage_type() {
                send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
                return;
            }
        }

        _ => {
            send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
            return;
        }
    }

    system_chat(
        w,
        player,
        "You tailor the appearance onto a different weapon.",
        ChatMessageType::Broadcast,
    );

    // Update all of the relevant properties
    update_weapon_props(w, player, source, target);

    // Send UpdateObject, mostly for the client to register the new name.
    let m = game_message_update_object(w, target, false, false);
    if let Some(s) = player_session(w, player) {
        enqueue_send(w, s, m);
    }

    player_inventory::try_consume_from_inventory_with_networking(w, player, source, 1);

    dispatch::save_biota_to_database::save_biota_to_database(w, target, true);

    send_use_done_event(w, player, WeenieError::None);
}

// ACE: Tailoring.UpdateCommonProps
/// Writes the kit's look onto the target, announcing each property to the player.
pub fn update_common_props(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) {
    use crate::world_objects::player_properties::{
        update_property_bool, update_property_data_id, update_property_float, update_property_int,
        update_property_string,
    };

    let s = obj(w, source);
    let palette_template = s.palette_template();
    let material_type = s.material_type();
    let obj_scale = s.obj_scale();
    let (shade, shade2, shade3, shade4) = (s.shade(), s.shade2(), s.shade3(), s.shade4());
    let lights_status = s.lights_status();
    let translucency = s.translucency();
    let setup_table_id = s.setup_table_id();
    let clothing_base = s.clothing_base();
    let palette_base_id = s.palette_base_id();
    let long_desc = s.long_desc();
    let ignore_clo_icons = s.ignore_clo_icons();
    let icon_id = s.icon_id();
    let source_name = dispatch::name::name(w, source);

    update_property_int(
        w,
        player,
        target,
        PropertyInt::PaletteTemplate,
        palette_template,
        false,
    );
    //player.UpdateProperty(target, PropertyInt.UiEffects, (int?)source.UiEffects);
    if let Some(m) = material_type {
        update_property_int(
            w,
            player,
            target,
            PropertyInt::MaterialType,
            Some(m.0.cast_signed()),
            false,
        );
    }

    update_property_float(
        w,
        player,
        target,
        PropertyFloat::DefaultScale,
        obj_scale.map(f64::from),
        false,
    );

    update_property_float(w, player, target, PropertyFloat::Shade, shade, false);
    update_property_float(w, player, target, PropertyFloat::Shade2, shade2, false);
    update_property_float(w, player, target, PropertyFloat::Shade3, shade3, false);
    update_property_float(w, player, target, PropertyFloat::Shade4, shade4, false);

    update_property_bool(
        w,
        player,
        target,
        PropertyBool::LightsStatus,
        lights_status,
        false,
    );
    update_property_float(
        w,
        player,
        target,
        PropertyFloat::Translucency,
        translucency.map(f64::from),
        false,
    );

    update_property_data_id(
        w,
        player,
        target,
        PropertyDataId::Setup,
        Some(setup_table_id),
        false,
    );
    update_property_data_id(
        w,
        player,
        target,
        PropertyDataId::ClothingBase,
        clothing_base,
        false,
    );
    update_property_data_id(
        w,
        player,
        target,
        PropertyDataId::PaletteBase,
        palette_base_id,
        false,
    );

    update_property_string(
        w,
        player,
        target,
        PropertyString::Name,
        source_name.as_deref(),
        false,
    );
    update_property_string(
        w,
        player,
        target,
        PropertyString::LongDesc,
        long_desc.as_deref(),
        false,
    );

    update_property_bool(
        w,
        player,
        target,
        PropertyBool::IgnoreCloIcons,
        ignore_clo_icons,
        false,
    );
    update_property_data_id(
        w,
        player,
        target,
        PropertyDataId::Icon,
        Some(icon_id),
        false,
    );
}

// ACE: Tailoring.UpdateArmorProps
/// The common look plus dyability and the overlay icon.
pub fn update_armor_props(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) {
    update_common_props(w, player, source, target);

    let dyable = obj(w, source).dyable();
    crate::world_objects::player_properties::update_property_bool(
        w,
        player,
        target,
        PropertyBool::Dyable,
        dyable,
        false,
    );

    // If the item we are replacing is one of our preset icons with an overlay, we need to remove it.
    if ARMOR_OVERLAY_ICONS.contains_key(&obj(w, target).biota.weenie_class_id) {
        crate::world_objects::player_properties::update_property_data_id(
            w,
            player,
            target,
            PropertyDataId::IconOverlay,
            None,
            false,
        );
    }

    // If the source item has an icon stashed in the Secondary Overlay, it's because we put it there!
    // Apply this overlay if the target does not already have one.
    let secondary: Option<u32> = obj(w, source).get_property(PropertyDataId::IconOverlaySecondary);
    if secondary.is_some() && obj(w, target).icon_overlay_id().is_none() {
        crate::world_objects::player_properties::update_property_data_id(
            w,
            player,
            target,
            PropertyDataId::IconOverlay,
            secondary,
            false,
        );
    }

    // ObjDescOverride.Clear()
}

// ACE: Tailoring.UpdateWeaponProps
/// The common look plus the hook placement.
pub fn update_weapon_props(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) {
    update_common_props(w, player, source, target);

    let (hook_type, hook_placement) = (obj(w, source).hook_type(), obj(w, source).hook_placement());
    crate::world_objects::player_properties::update_property_int(
        w,
        player,
        target,
        PropertyInt::HookType,
        hook_type.map(i32::from),
        false,
    );
    crate::world_objects::player_properties::update_property_int(
        w,
        player,
        target,
        PropertyInt::HookPlacement,
        hook_placement,
        false,
    );
}

mod shims {
    use empyrean_entity::ObjectGuid;

    use crate::World;

    /// `Player.RushNextPlayerSave(seconds)` (`Player_Database.cs`).
    pub(super) fn rush_next_player_save(w: &mut World, player: ObjectGuid, seconds: i32) {
        crate::world_objects::player_database::rush_next_player_save(w, player, seconds);
    }
}
