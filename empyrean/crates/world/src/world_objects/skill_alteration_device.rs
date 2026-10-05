// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/SkillAlterationDevice.cs
//! Port of `Source/ACE.Server/WorldObjects/SkillAlterationDevice.cs`: the Gem of Enlightenment
//! (specialize a trained skill) and the Gem of Forgetfulness (lower a skill one step).

use dereth_assets::tables::SkillBase;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    PropertyInt, Skill, SkillAdvancementClass, WeenieError, WeenieErrorWithString, WieldRequirement,
};
use empyrean_entity::ObjectGuid;

use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{enqueue_send, enqueue_send_many, GameMessage};
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_private_update_skill::game_message_private_update_skill;
use crate::world_objects::entity::creature_skill::CreatureSkill;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `SkillAlterationDevice.cs`.
#[derive(Debug, Default)]
pub struct SkillAlterationDeviceFields {}

// ACE: SkillAlterationDevice.SkillAlterationType
/// The kind of gem: `Undef` 0, `Specialize` 1 (Gem of Enlightenment), `Lower` 2 (Gem of
/// Forgetfulness).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkillAlterationType(pub i32);

#[allow(non_upper_case_globals)]
impl SkillAlterationType {
    pub const Undef: Self = Self(0);
    pub const Specialize: Self = Self(1);
    pub const Lower: Self = Self(2);
}

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn session_of(w: &World, player: ObjectGuid) -> empyrean_net::SessionId {
    crate::world_objects::world_object_networking::shims::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)")
}

fn send_error_with_string(
    w: &mut World,
    player: ObjectGuid,
    error: WeenieErrorWithString,
    skill: Skill,
) {
    let session = session_of(w, player);
    let msg =
        game_event_weenie_error_with_string(session_data(w, session), error, &skill.to_sentence());
    enqueue_send(w, session, msg);
}

/// `DatManager.PortalDat.SkillTable.SkillBaseHash[(uint)skill]`.
///
/// # Panics
/// For a skill the table lacks (ACE: `KeyNotFoundException`).
fn skill_base(w: &World, skill: Skill) -> SkillBase {
    let key: u32 = skill.0.cs_cast();
    w.dats
        .portal_dat()
        .skill_table()
        .skills
        .get(&key)
        .cloned()
        .unwrap_or_else(|| panic!("KeyNotFoundException: SkillBaseHash[{key}]"))
}

/// `SkillBase.UpgradeCostFromTrainedToSpecialized`: `SpecializedCost - TrainedCost`.
fn upgrade_cost_from_trained_to_specialized(skill_base: &SkillBase) -> i32 {
    skill_base
        .specialized_cost
        .wrapping_sub(skill_base.trained_cost)
}

/// A skill's specialization cost, adjusted by the player's heritage (e.g. Arcane Lore) when the
/// character generator lists the skill for it: `(uint)player.Heritage` throws for a null heritage.
fn heritage_specialized_cost(
    w: &World,
    player: ObjectGuid,
    skill: Skill,
    specialized_cost: i32,
) -> i32 {
    let heritage: u32 = object(w, player)
        .heritage()
        .expect("ACE: Heritage is null (InvalidOperationException)")
        .cs_cast();
    let char_gen = w.dats.portal_dat().char_gen();
    if let Some(heritage_group) = char_gen.heritage_groups.get(&heritage) {
        // check for adjusted costs of Specialization due to player's heritage (e.g. Arcane Lore)
        let skill_num: u32 = skill.0.cs_cast();
        if let Some(&(_, _, primary_cost)) = heritage_group.skills.iter().find(|s| s.0 == skill_num)
        {
            return primary_cost;
        }
    }
    specialized_cost
}

// ACE: SkillAlterationDevice.ActOnUse
/// `ActOnUse(activator)`: `ActOnUse(activator, false)`.
pub fn skill_alteration_device_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    act_on_use(w, this, activator, false);
}

// ACE: SkillAlterationDevice.ActOnUse
/// Checks the gem's requirements, then asks for a confirmation (the answer comes back here with
/// `confirmed`), then alters the skill.
pub fn act_on_use(w: &mut World, this: ObjectGuid, activator: ObjectGuid, confirmed: bool) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    // verify skill
    let skill_to_be_altered = object(w, this).skill_to_be_altered();
    let skill = w
        .objects
        .get_mut(player)
        .expect("ACE: player")
        .get_creature_skill(skill_to_be_altered, true);

    let Some(skill) = skill else {
        let session = session_of(w, player);
        let msg =
            game_event_weenie_error(session_data(w, session), WeenieError::YouFailToAlterSkill);
        enqueue_send(w, session, msg);
        return;
    };

    // get skill training / specialization costs
    let skill_base = skill_base(w, skill.skill);

    if !verify_requirements(w, this, player, skill, &skill_base) {
        return;
    }

    if !confirmed {
        let mut msg = "This action will ".to_owned();
        match object(w, this).type_of_alteration() {
            SkillAlterationType::Specialize => {
                msg += &format!(
                    "specialize your {} skill and cost {} credits.",
                    skill.skill.to_sentence(),
                    upgrade_cost_from_trained_to_specialized(&skill_base)
                );
            }
            SkillAlterationType::Lower => {
                let from_to = if skill.advancement_class(object(w, player))
                    == SkillAdvancementClass::Specialized
                {
                    "specialized to trained"
                } else {
                    "trained to untrained"
                };
                msg += &format!(
                    "lower your {} skill from {from_to} and refund the skill credits and experience invested in this skill.",
                    skill.skill.to_sentence()
                );
            }
            _ => {}
        }

        let confirmation = crate::entity::confirmation::Confirmation::alter_skill(player, this);
        if !crate::world_objects::managers::confirmation_manager::enqueue_send(
            w,
            player,
            confirmation,
            &msg,
        ) {
            crate::world_objects::player_networking::send_weenie_error(
                w,
                player,
                WeenieError::ConfirmationInProgress,
            );
        }

        return;
    }

    alter_skill(w, this, player, skill, &skill_base);
}

// ACE: SkillAlterationDevice.VerifyRequirements
pub fn verify_requirements(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
    skill: CreatureSkill,
    skill_base: &SkillBase,
) -> bool {
    match object(w, this).type_of_alteration() {
        // Gem of Enlightenment
        SkillAlterationType::Specialize => {
            // ensure skill is trained
            if skill.advancement_class(object(w, player)) != SkillAdvancementClass::Trained {
                send_error_with_string(
                    w,
                    player,
                    WeenieErrorWithString::Your_SkillMustBeTrained,
                    skill.skill,
                );
                return false;
            }

            // ensure player has enough available skill credits
            // (`AvailableSkillCredits < cost`: a null lifts to false)
            if object(w, player)
                .available_skill_credits()
                .is_some_and(|c| c < upgrade_cost_from_trained_to_specialized(skill_base))
            {
                send_error_with_string(
                    w,
                    player,
                    WeenieErrorWithString::NotEnoughSkillCreditsToSpecialize,
                    skill.skill,
                );
                return false;
            }

            // ensure player won't exceed limit of 70 specialized credits after operation
            let specialized_cost =
                heritage_specialized_cost(w, player, skill.skill, skill_base.specialized_cost);

            if get_total_specialized_credits(w, player).wrapping_add(specialized_cost) > 70 {
                send_error_with_string(
                    w,
                    player,
                    WeenieErrorWithString::TooManyCreditsInSpecializedSkills,
                    skill.skill,
                );
                return false;
            }
        }

        // Gem of Forgetfulness
        SkillAlterationType::Lower => {
            // ensure skill is trained or specialized
            if skill.advancement_class(object(w, player)) < SkillAdvancementClass::Trained {
                send_error_with_string(
                    w,
                    player,
                    WeenieErrorWithString::Your_SkillIsAlreadyUntrained,
                    skill.skill,
                );
                return false;
            }

            // Check for equipped items that have requirements in the skill we're lowering
            if check_wielded_items(w, this, player) {
                // Items are wielded which might be affected by a lowering operation
                send_error_with_string(
                    w,
                    player,
                    WeenieErrorWithString::CannotLowerSkillWhileWieldingItem,
                    skill.skill,
                );
                return false;
            }
        }
        _ => {}
    }
    true
}

/// `player.Session.Network.EnqueueSend(updateSkill, availableSkillCredits, message)`, then
/// `TryConsumeFromInventoryWithNetworking(this, 1)`.
fn send_result(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
    skill: CreatureSkill,
    result: WeenieErrorWithString,
) {
    let session = session_of(w, player);
    let o = w.objects.get_mut(player).expect("ACE: player");
    let update_skill = game_message_private_update_skill(o, skill);
    let credits = o.available_skill_credits().unwrap_or(0);
    let available_skill_credits =
        game_message_private_update_property_int(o, PropertyInt::AvailableSkillCredits, credits);
    let message = game_event_weenie_error_with_string(
        session_data(w, session),
        result,
        &skill.skill.to_sentence(),
    );

    let msgs: [GameMessage; 3] = [update_skill, available_skill_credits, message];
    enqueue_send_many(w, session, msgs);

    crate::world_objects::player_inventory::try_consume_from_inventory_with_networking(
        w, player, this, 1,
    );
}

// ACE: SkillAlterationDevice.AlterSkill
pub fn alter_skill(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
    skill: CreatureSkill,
    skill_base: &SkillBase,
) {
    use crate::world_objects::player_skills;

    match object(w, this).type_of_alteration() {
        // Gem of Enlightenment
        SkillAlterationType::Specialize => {
            if player_skills::specialize_skill_with(
                w,
                player,
                skill.skill,
                upgrade_cost_from_trained_to_specialized(skill_base),
                false,
            ) {
                send_result(
                    w,
                    this,
                    player,
                    skill,
                    WeenieErrorWithString::YouHaveSucceededSpecializing_Skill,
                );
            }
        }

        // Gem of Forgetfulness
        SkillAlterationType::Lower => {
            let advancement_class = skill.advancement_class(object(w, player));

            // specialized => trained
            if advancement_class == SkillAdvancementClass::Specialized {
                let (specialized, player_has_augmentation) =
                    player_skills::is_skill_specialized_via_augmentation(
                        object(w, player),
                        skill.skill,
                    );
                let specialized_via_augmentation = specialized && player_has_augmentation;

                if player_skills::unspecialize_skill(
                    w,
                    player,
                    skill.skill,
                    upgrade_cost_from_trained_to_specialized(skill_base),
                ) {
                    let msg = if specialized_via_augmentation {
                        WeenieErrorWithString::YouSucceededRecoveringXPFromSkill_AugmentationNotUntrainable
                    } else {
                        WeenieErrorWithString::YouHaveSucceededUnspecializing_Skill
                    };
                    send_result(w, this, player, skill, msg);
                }
            }
            // trained => untrained
            // in the case of skills which can't be untrained,
            // keep trained, but recover the xp spent
            else if advancement_class == SkillAdvancementClass::Trained {
                let untrainable = player_skills::is_skill_untrainable(skill.skill);

                if player_skills::untrain_skill(w, player, skill.skill, skill_base.trained_cost) {
                    let msg = if untrainable {
                        WeenieErrorWithString::YouHaveSucceededUntraining_Skill
                    } else {
                        WeenieErrorWithString::CannotUntrain_SkillButRecoveredXP
                    };
                    send_result(w, this, player, skill, msg);
                }
            }
        }
        _ => {}
    }
}

// ACE: SkillAlterationDevice.GetTotalSpecializedCredits
/// Calculates and returns the current total number of specialized credits.
fn get_total_specialized_credits(w: &World, player: ObjectGuid) -> i32 {
    let mut specialized_credits_total = 0i32;

    let o = object(w, player);
    let skills: Vec<(Skill, CreatureSkill)> = o.skills().iter().map(|(k, v)| (*k, *v)).collect();
    for (key, value) in skills {
        if value.advancement_class(o) == SkillAdvancementClass::Specialized {
            match key {
                // exclude None/Undef skill
                Skill::None
                // exclude aug specs
                | Skill::ArmorTinkering
                | Skill::ItemTinkering
                | Skill::MagicItemTinkering
                | Skill::WeaponTinkering
                | Skill::Salvaging => continue,
                _ => {}
            }

            let skill = skill_base(w, key);

            let specialized_cost =
                heritage_specialized_cost(w, player, key, skill.specialized_cost);
            specialized_credits_total = specialized_credits_total.wrapping_add(specialized_cost);
        }
    }

    specialized_credits_total
}

// ACE: SkillAlterationDevice.CheckWieldedItems
/// Checks wielded items and their requirements to see if they'd be violated by an impending skill
/// lowering operation.
fn check_wielded_items(w: &mut World, this: ObjectGuid, player: ObjectGuid) -> bool {
    for equipped_item in
        crate::world_objects::creature_equipment::equipped_objects_values(w, player)
    {
        let i = object(w, equipped_item);
        let reqs = [
            (
                i.wield_requirements(),
                i.wield_skill_type(),
                i.wield_difficulty(),
            ),
            (
                i.wield_requirements2(),
                i.wield_skill_type2(),
                i.wield_difficulty2(),
            ),
            (
                i.wield_requirements3(),
                i.wield_skill_type3(),
                i.wield_difficulty3(),
            ),
            (
                i.wield_requirements4(),
                i.wield_skill_type4(),
                i.wield_difficulty4(),
            ),
        ];
        for (req, skill_type, difficulty) in reqs {
            if check_wield_requirement(w, this, player, req, skill_type, difficulty) {
                //|| CheckActivationRequirements(player, equippedItem))
                return true;
            }
        }
    }
    false
}

// ACE: SkillAlterationDevice.CheckWieldRequirement
fn check_wield_requirement(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
    item_wield_req: WieldRequirement,
    wield_skill_type: Option<i32>,
    wield_skill_difficulty: Option<i32>,
) -> bool {
    let skill_to_be_altered = object(w, this).skill_to_be_altered();

    if item_wield_req == WieldRequirement::Training {
        let skill = crate::world_objects::world_object::convert_to_mo_a_skill(
            w,
            player,
            Skill(wield_skill_type.unwrap_or(0)),
        );
        if skill != skill_to_be_altered {
            return false;
        }

        let creature_skill = w
            .objects
            .get_mut(player)
            .expect("ACE: player")
            .get_creature_skill(skill, false);

        let (Some(creature_skill), Some(difficulty)) = (creature_skill, wield_skill_difficulty)
        else {
            return false;
        };

        return SkillAdvancementClass(difficulty.cs_cast())
            >= creature_skill.advancement_class(object(w, player));
    }

    if item_wield_req != WieldRequirement::RawSkill && item_wield_req != WieldRequirement::Skill {
        return false;
    }

    crate::world_objects::world_object::convert_to_mo_a_skill(
        w,
        player,
        Skill(wield_skill_type.unwrap_or(0)),
    ) == skill_to_be_altered
}

// ACE: SkillAlterationDevice.CheckActivationRequirements
/// Unused by ACE (its call in `CheckWieldedItems` is commented out).
#[allow(dead_code)] // unused in ACE: its one caller is commented out
fn check_activation_requirements(
    w: &mut World,
    this: ObjectGuid,
    player: ObjectGuid,
    equipped_item: ObjectGuid,
) -> bool {
    use crate::world_objects::world_object::convert_to_mo_a_skill;

    let skill_to_be_altered = object(w, this).skill_to_be_altered();
    let i = object(w, equipped_item);
    let (
        item_difficulty,
        item_skill_limit,
        item_specialized_only,
        use_requires_skill,
        use_requires_skill_spec,
    ) = (
        i.item_difficulty(),
        i.item_skill_limit(),
        i.item_specialized_only(),
        i.use_requires_skill(),
        i.use_requires_skill_spec(),
    );

    if item_difficulty.is_some_and(|d| d > 0) && skill_to_be_altered == Skill::ArcaneLore {
        return true;
    }

    if convert_to_mo_a_skill(w, player, item_skill_limit.unwrap_or(Skill::None))
        == skill_to_be_altered
    {
        return true;
    }

    if convert_to_mo_a_skill(w, player, item_specialized_only.unwrap_or(Skill::None))
        == skill_to_be_altered
    {
        return true;
    }

    if convert_to_mo_a_skill(w, player, Skill(use_requires_skill.unwrap_or(0)))
        == skill_to_be_altered
    {
        return true;
    }

    if convert_to_mo_a_skill(w, player, Skill(use_requires_skill_spec.unwrap_or(0)))
        == skill_to_be_altered
    {
        return true;
    }

    false
}

// ---- constructors and SetEphemeralValues ----

/// `new SkillAlterationDevice(weenie, guid)` / `new SkillAlterationDevice(biota)`: the `WorldObject` constructor, then
/// SkillAlterationDevice's `SetEphemeralValues`.
// ACE: SkillAlterationDevice.SkillAlterationDevice
pub fn skill_alteration_device_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    skill_alteration_device_set_ephemeral_values(o, env);
}

/// Empty in ACE.
// ACE: SkillAlterationDevice.SetEphemeralValues
fn skill_alteration_device_set_ephemeral_values(
    _o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
}
