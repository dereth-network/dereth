// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Enlightenment.cs
//! Port of `Source/ACE.Server/Entity/Enlightenment.cs` (asheron.fandom.com/wiki/Enlightenment).
//!
//! Reset your character to level 1, losing all experience and luminance but gaining a title, two
//! points in vitality and one point in all of your skills. In order to be eligible for
//! enlightenment, you must be level 275, Master rank in a Society, and have all luminance auras with
//! the exception of the skill credit auras.
//!
//! Requirements: level 275; all luminance auras (crafting aura included) except the 2 skill credit
//! auras (20 million total luminance); mastery rank in a society; 25 unused pack spaces; at most 5
//! enlightenments.
//!
//! You lose all experience (reverting to level 1), all luminance and luminance auras except the
//! skill credit auras, the ability to use aetheria and to gain luminance (until re-earned), and the
//! ability to use items beyond a level 1 character (equipped items move to your pack). You keep
//! augmentations, skill credits from luminance auras, Aun Ralirea and Chasing Oswald, and all
//! quest flags except aetheria and luminance. You gain a new title, +2 vitality, +1 to all of your
//! skills and an attribute reset certificate.
//!
//! The client counts the +1 skills and +2 vitality too, through the shared rules; the server
//! follows ACE.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    AetheriaBitfield, Channel, CharacterTitle, ChatMessageType, PropertyAttribute,
    PropertyAttribute2nd, PropertyInt, PropertyInt64, Skill,
};
use empyrean_entity::ObjectGuid;

use crate::managers::quest_manager::{self, QuestOwner};
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_private_update_attribute::game_message_private_update_attribute;
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_private_update_property_int64::game_message_private_update_property_int64;
use crate::network::game_messages::messages::game_message_private_update_vital::game_message_private_update_vital;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::player::send_message;
use crate::world_objects::world_object::WorldObject;
use crate::World;

fn player(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects
        .get(g)
        .expect("ACE: player is null (NullReferenceException)")
}

fn player_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(g)
        .expect("ACE: player is null (NullReferenceException)")
}

/// `player.Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, p: ObjectGuid, msg: GameMessage) {
    let session = crate::world_objects::world_object_networking::shims::player_session(w, p)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    enqueue_send(w, session, msg);
}

fn send_chat(w: &mut World, p: ObjectGuid, text: &str) {
    send(
        w,
        p,
        game_message_system_chat(text, ChatMessageType::Broadcast),
    );
}

/// `new GameMessagePrivateUpdatePropertyInt(player, prop, value)`.
fn update_int(w: &mut World, p: ObjectGuid, prop: PropertyInt, value: i32) {
    let msg = game_message_private_update_property_int(player_mut(w, p), prop, value);
    send(w, p, msg);
}

/// `new GameMessagePrivateUpdatePropertyInt64(player, prop, value)`.
fn update_int64(w: &mut World, p: ObjectGuid, prop: PropertyInt64, value: i64) {
    let msg = game_message_private_update_property_int64(player_mut(w, p), prop, value);
    send(w, p, msg);
}

fn quest_erase(w: &mut World, p: ObjectGuid, quest: &str) {
    quest_manager::erase(w, &mut QuestOwner::Creature(p), quest);
}

// ACE: Enlightenment.HandleEnlightenment
/// The enlightenment an NPC's emote grants: the checks, then the reset and the perks, then a save.
pub fn handle_enlightenment(w: &mut World, npc: ObjectGuid, p: ObjectGuid) {
    if !verify_requirements(w, p) {
        return;
    }

    dequip_all_items(w, p);

    remove_ability(w, p);

    add_perks(w, npc, p);

    crate::dispatch::save_biota_to_database::save_biota_to_database(w, p, true);
}

// ACE: Enlightenment.VerifyRequirements
/// Level 275, all luminance auras, a society master, 25 free pack slots and fewer than 5
/// enlightenments; the first unmet one is told to the player.
pub fn verify_requirements(w: &mut World, p: ObjectGuid) -> bool {
    // `player.Level < 275`: a null level lifts to false
    if player(w, p).level().is_some_and(|l| l < 275) {
        send_chat(w, p, "You must be level 275 for enlightenment.");
        return false;
    }

    if !verify_lum_augs(player(w, p)) {
        send_chat(w, p, "You must have all luminance auras for enlightenment.");
        return false;
    }

    if !verify_society_master(player(w, p)) {
        send_chat(
            w,
            p,
            "You must be a Master of one of the Societies of Dereth for enlightenment.",
        );
        return false;
    }

    // `GetFreeInventorySlots(includeSidePacks = true)`
    if crate::world_objects::container::get_free_inventory_slots(w, p, true) < 25 {
        send_chat(
            w,
            p,
            "You must have at least 25 free inventory slots in your main pack for enlightenment.",
        );
        return false;
    }

    if player(w, p).enlightenment() >= 5 {
        send_chat(
            w,
            p,
            "You have already reached the maximum enlightenment level!",
        );
        return false;
    }
    true
}

// ACE: Enlightenment.VerifySocietyMaster
#[must_use]
pub fn verify_society_master(o: &WorldObject) -> bool {
    o.society_rank_celhan() == Some(1001)
        || o.society_rank_eldweb() == Some(1001)
        || o.society_rank_radblo() == Some(1001)
}

// ACE: Enlightenment.VerifyLumAugs
/// The eleven luminance aura counts sum to exactly 65.
#[must_use]
pub fn verify_lum_augs(o: &WorldObject) -> bool {
    let mut lum_aug_credits = 0i32;

    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_all_skills());
    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_surge_chance_rating());
    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_crit_damage_rating());
    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_crit_reduction_rating());
    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_damage_rating());
    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_damage_reduction_rating());
    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_item_mana_usage());
    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_item_mana_gain());
    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_healing_rating());
    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_skilled_craft());
    lum_aug_credits = lum_aug_credits.wrapping_add(o.lum_aug_skilled_spec());

    lum_aug_credits == 65
}

// ACE: Enlightenment.DequipAllItems
/// Every equipped item goes into the main pack (`HandleActionPutItemInContainer(item, player, 0)`).
pub fn dequip_all_items(w: &mut World, p: ObjectGuid) {
    let equipped_objects = crate::world_objects::creature_equipment::equipped_objects_values(w, p);

    for equipped_object in equipped_objects {
        crate::world_objects::player_inventory::handle_action_put_item_in_container(
            w,
            p,
            equipped_object.full(),
            p.full(),
            0,
        );
    }
}

// ACE: Enlightenment.RemoveAbility
pub fn remove_ability(w: &mut World, p: ObjectGuid) {
    remove_society(w, p);
    remove_luminance(w, p);
    remove_aetheria(w, p);
    remove_attributes(w, p);
    remove_skills(w, p);
    remove_level(w, p);
}

// ACE: Enlightenment.RemoveSociety
pub fn remove_society(w: &mut World, p: ObjectGuid) {
    for quest in [
        "SocietyMember",
        "CelestialHandMember",
        "EnlightenedCelestialHandMaster",
        "EldrytchWebMember",
        "EnlightenedEldrytchWebMaster",
        "RadiantBloodMember",
        "EnlightenedRadiantBloodMaster",
    ] {
        quest_erase(w, p, quest);
    }

    // after rejoining society, player can get promoted instantly to master when speaking to promotions officer
    if player(w, p).society_rank_celhan() == Some(1001) {
        quest_manager::stamp(
            w,
            &mut QuestOwner::Creature(p),
            "EnlightenedCelestialHandMaster",
        );
    }
    if player(w, p).society_rank_eldweb() == Some(1001) {
        quest_manager::stamp(
            w,
            &mut QuestOwner::Creature(p),
            "EnlightenedEldrytchWebMaster",
        );
    }
    if player(w, p).society_rank_radblo() == Some(1001) {
        quest_manager::stamp(
            w,
            &mut QuestOwner::Creature(p),
            "EnlightenedRadiantBloodMaster",
        );
    }

    player_mut(w, p).set_faction1_bits(None);
    update_int(w, p, PropertyInt::Faction1Bits, 0);
    player_mut(w, p).set_society_rank_celhan(None);
    update_int(w, p, PropertyInt::SocietyRankCelhan, 0);
    player_mut(w, p).set_society_rank_eldweb(None);
    update_int(w, p, PropertyInt::SocietyRankEldweb, 0);
    player_mut(w, p).set_society_rank_radblo(None);
    update_int(w, p, PropertyInt::SocietyRankRadblo, 0);
}

// ACE: Enlightenment.RemoveLevel
pub fn remove_level(w: &mut World, p: ObjectGuid) {
    player_mut(w, p).set_total_experience(Some(0));
    let total_experience = player(w, p).total_experience().unwrap_or(0);
    update_int64(w, p, PropertyInt64::TotalExperience, total_experience);

    player_mut(w, p).set_level(Some(1));
    let level = player(w, p).level().unwrap_or(0);
    update_int(w, p, PropertyInt::Level, level);
}

// ACE: Enlightenment.RemoveAetheria
pub fn remove_aetheria(w: &mut World, p: ObjectGuid) {
    for quest in [
        "EFULNorthManaFieldUsed",
        "EFULSouthManaFieldUsed",
        "EFULEastManaFieldUsed",
        "EFULWestManaFieldUsed",
        "EFULCenterManaFieldUsed",
        "EFMLNorthManaFieldUsed",
        "EFMLSouthManaFieldUsed",
        "EFMLEastManaFieldUsed",
        "EFMLWestManaFieldUsed",
        "EFMLCenterManaFieldUsed",
        "EFLLNorthManaFieldUsed",
        "EFLLSouthManaFieldUsed",
        "EFLLEastManaFieldUsed",
        "EFLLWestManaFieldUsed",
        "EFLLCenterManaFieldUsed",
    ] {
        quest_erase(w, p, quest);
    }

    player_mut(w, p).set_aetheria_flags(AetheriaBitfield::None);
    update_int(w, p, PropertyInt::AetheriaBitfield, 0);

    send_message(
        w,
        p,
        "Your mastery of Aetheric magics fades.",
        ChatMessageType::Broadcast,
    );
}

// ACE: Enlightenment.RemoveAttributes
/// Every attribute's and every max vital's ranks and experience go to 0.
pub fn remove_attributes(w: &mut World, p: ObjectGuid) {
    // `Enum.GetNames(typeof(PropertyAttribute)).Length`: Undef and the six attributes
    let property_count = PropertyAttribute::NAMES.len();
    for i in 1..property_count {
        let attribute = PropertyAttribute(u16::try_from(i).expect("enum value"));

        let a = *player(w, p)
            .attributes()
            .get(&attribute)
            .expect("KeyNotFoundException: Attributes[attribute]");
        a.set_ranks(player_mut(w, p), 0);
        a.set_experience_spent(player_mut(w, p), 0);
        let msg = game_message_private_update_attribute(player_mut(w, p), a);
        send(w, p, msg);
    }

    // the max vitals: 1, 3, 5
    let property_count = PropertyAttribute2nd::NAMES.len();
    for i in (1..property_count).step_by(2) {
        let attribute = PropertyAttribute2nd(u16::try_from(i).expect("enum value"));

        let v = player(w, p)
            .vitals()
            .get(&attribute)
            .cloned()
            .expect("KeyNotFoundException: Vitals[attribute]");
        v.set_ranks(player_mut(w, p), 0);
        v.set_experience_spent(player_mut(w, p), 0);
        let v = player(w, p)
            .vitals()
            .get(&attribute)
            .cloned()
            .expect("KeyNotFoundException: Vitals[attribute]");
        let msg = game_message_private_update_vital(player_mut(w, p), v);
        send(w, p, msg);
    }

    send_message(
        w,
        p,
        "Your attribute training fades.",
        ChatMessageType::Broadcast,
    );
}

// ACE: Enlightenment.RemoveSkills
/// Every skill is reset (no refund); the XP and skill credits go back to the heritage's base plus
/// the three skill credit quests.
///
/// # Panics
/// For a heritage the character generator does not have (ACE: `KeyNotFoundException`).
pub fn remove_skills(w: &mut World, p: ObjectGuid) {
    // `Enum.GetNames(typeof(Skill)).Length`
    let property_count = Skill::NAMES.len();
    for i in 1..property_count {
        let skill = Skill(i32::try_from(i).expect("enum value"));

        crate::world_objects::player_skills::reset_skill(w, p, skill, false);
    }

    player_mut(w, p).set_available_experience(Some(0));
    update_int64(w, p, PropertyInt64::AvailableExperience, 0);

    // `(uint)player.Heritage`: a null heritage throws (InvalidOperationException)
    let heritage: u32 = player(w, p)
        .heritage()
        .expect("ACE: Heritage is null (InvalidOperationException)")
        .cs_cast();
    let heritage_skill_credits = w
        .dats
        .portal_dat()
        .char_gen()
        .heritage_groups
        .get(&heritage)
        .unwrap_or_else(|| panic!("KeyNotFoundException: HeritageGroups[{heritage}]"))
        .skill_credits;
    let mut available_skill_credits = 0i32;

    available_skill_credits =
        available_skill_credits.wrapping_add(heritage_skill_credits.cs_cast()); // base skill credits allowed

    let owner = QuestOwner::Creature(p);
    available_skill_credits += quest_manager::get_current_solves(w, &owner, "ArantahKill1"); // additional quest skill credit
    available_skill_credits +=
        quest_manager::get_current_solves(w, &owner, "OswaldManualCompleted"); // additional quest skill credit
    available_skill_credits += quest_manager::get_current_solves(w, &owner, "LumAugSkillQuest"); // additional quest skill credits

    player_mut(w, p).set_available_skill_credits(Some(available_skill_credits));

    let credits = player(w, p).available_skill_credits().unwrap_or(0);
    update_int(w, p, PropertyInt::AvailableSkillCredits, credits);
}

// ACE: Enlightenment.RemoveLuminance
pub fn remove_luminance(w: &mut World, p: ObjectGuid) {
    for quest in [
        "OracleLuminanceRewardsAccess_1110",
        "LoyalToShadeOfLadyAdja",
        "LoyalToKahiri",
        "LoyalToLiamOfGelid",
        "LoyalToLordTyragar",
    ] {
        quest_erase(w, p, quest);
    }

    player_mut(w, p).set_lum_aug_damage_rating(0);
    update_int(w, p, PropertyInt::LumAugDamageRating, 0);
    player_mut(w, p).set_lum_aug_damage_reduction_rating(0);
    update_int(w, p, PropertyInt::LumAugDamageReductionRating, 0);
    player_mut(w, p).set_lum_aug_crit_damage_rating(0);
    update_int(w, p, PropertyInt::LumAugCritDamageRating, 0);
    player_mut(w, p).set_lum_aug_crit_reduction_rating(0);
    update_int(w, p, PropertyInt::LumAugCritReductionRating, 0);
    //player.LumAugSurgeEffectRating = 0;
    update_int(w, p, PropertyInt::LumAugSurgeEffectRating, 0);
    player_mut(w, p).set_lum_aug_surge_chance_rating(0);
    update_int(w, p, PropertyInt::LumAugSurgeChanceRating, 0);
    player_mut(w, p).set_lum_aug_item_mana_usage(0);
    update_int(w, p, PropertyInt::LumAugItemManaUsage, 0);
    player_mut(w, p).set_lum_aug_item_mana_gain(0);
    update_int(w, p, PropertyInt::LumAugItemManaGain, 0);
    player_mut(w, p).set_lum_aug_vitality(0);
    update_int(w, p, PropertyInt::LumAugVitality, 0);
    player_mut(w, p).set_lum_aug_healing_rating(0);
    update_int(w, p, PropertyInt::LumAugHealingRating, 0);
    player_mut(w, p).set_lum_aug_skilled_craft(0);
    update_int(w, p, PropertyInt::LumAugSkilledCraft, 0);
    player_mut(w, p).set_lum_aug_skilled_spec(0);
    update_int(w, p, PropertyInt::LumAugSkilledSpec, 0);
    player_mut(w, p).set_lum_aug_all_skills(0);
    update_int(w, p, PropertyInt::LumAugAllSkills, 0);

    player_mut(w, p).set_available_luminance(None);
    update_int64(w, p, PropertyInt64::AvailableLuminance, 0);
    player_mut(w, p).set_maximum_luminance(None);
    update_int64(w, p, PropertyInt64::MaximumLuminance, 0);

    send_message(
        w,
        p,
        "Your Luminance and Luminance Auras fade from your spirit.",
        ChatMessageType::Broadcast,
    );
}

// ACE: Enlightenment.AttributeResetCertificate
/// The wcid of the Attribute Reset Certificate.
pub const ATTRIBUTE_RESET_CERTIFICATE: u32 = 46421;

// ACE: Enlightenment.AddPerks
/// +1 enlightenment (the +1 skills and +2 vitality follow from it), its title, the certificate and
/// the world broadcast.
pub fn add_perks(w: &mut World, npc: ObjectGuid, p: ObjectGuid) {
    // +1 to all skills
    // this could be handled through InitLevel, since we are always using deltas when modifying that field
    // (ie. +5/-5, instead of specifically setting to 5 trained / 10 specialized in SkillAlterationDevice)
    // however, it just feels safer to handle this dynamically in CreatureSkill, based on Enlightenment (similar to augs)
    //var enlightenment = player.Enlightenment + 1;
    //player.UpdateProperty(player, PropertyInt.Enlightenment, enlightenment);

    let enlightenment = player(w, p).enlightenment().wrapping_add(1);
    player_mut(w, p).set_enlightenment(enlightenment);
    let enlightenment = player(w, p).enlightenment();
    update_int(w, p, PropertyInt::Enlightenment, enlightenment);

    send_message(
        w,
        p,
        "You have become enlightened and view the world with new eyes.",
        ChatMessageType::Broadcast,
    );
    send_message(
        w,
        p,
        "Your available skill credits have been adjusted.",
        ChatMessageType::Broadcast,
    );
    send_message(
        w,
        p,
        "You have risen to a higher tier of enlightenment!",
        ChatMessageType::Broadcast,
    );

    // add title
    let (title, lvl) = match player(w, p).enlightenment() {
        1 => (Some(CharacterTitle::Awakened), "1st"),
        2 => (Some(CharacterTitle::Enlightened), "2nd"),
        3 => (Some(CharacterTitle::Illuminated), "3rd"),
        4 => (Some(CharacterTitle::Transcended), "4th"),
        5 => (Some(CharacterTitle::CosmicConscious), "5th"),
        _ => (None, ""),
    };
    if let Some(title) = title {
        crate::world_objects::player_character::add_title(w, p, title.0, false);
    }

    crate::world_objects::player_inventory::give_from_emote(
        w,
        p,
        Some(npc),
        ATTRIBUTE_RESET_CERTIFICATE,
        1,
        0,
        0.0,
    );

    let name = crate::dispatch::name::name(w, p).unwrap_or_default();
    let msg = format!("{name} has achieved the {lvl} level of Enlightenment!");
    crate::managers::player_manager::broadcast_to_all(
        w,
        &game_message_system_chat(&msg, ChatMessageType::WorldBroadcast),
    );
    crate::managers::player_manager::log_broadcast_chat(w, Channel::AllBroadcast, None, &msg);

    // +2 vitality
    // handled automatically via PropertyInt.Enlightenment * 2

    /*var vitality = player.LumAugVitality + 2;
    player.UpdateProperty(player, PropertyInt.LumAugVitality, vitality);*/
}
