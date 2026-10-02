// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Skills.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Skills.cs`.
//!
//! Members touching the network, the action queue or the dats are free functions over
//! `(w, this)`. `this` must be a Player with a session (`Player.Session`, found through
//! `PlayerManager`); ACE would throw a `NullReferenceException` without one, and so do these.
//!
//! The end of the file holds pointers to members ported in other files (`Player_Xp`,
//! `Player_Networking`, `Player_Move`, `QuestManager`, `PropertyManager`, ...), named after them.

use empyrean_common::dotnet::math::round as math_round;
use empyrean_common::dotnet::{format as dotnet_format, CsCast};
use empyrean_entity::enums::{
    ChatMessageType, PlayScript, PropertyBool, PropertyInt, ShareType, Skill,
    SkillAdvancementClass, Sound, WeaponType, XpType,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_private_update_skill::game_message_private_update_skill;
use crate::network::game_messages::messages::game_message_sound::game_message_sound;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::entity::creature_skill::CreatureSkill;
use crate::world_objects::world_object::{play_particle_effect, WorldObject};
use crate::World;

/// Non-property fields declared in `Player_Skills.cs`.
#[derive(Debug, Default)]
pub struct PlayerSkillsFields {}

// ---------------------------------------------------------------------------------------------
// Shared plumbing for the Player stat partials (not ACE)
// ---------------------------------------------------------------------------------------------

fn obj(w: &World, this: ObjectGuid) -> &WorldObject {
    w.objects
        .get(this)
        .unwrap_or_else(|| panic!("Player {this:?}: missing object"))
}

fn obj_mut(w: &mut World, this: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(this)
        .unwrap_or_else(|| panic!("Player {this:?}: missing object"))
}

/// `Player.Session`, which ACE dereferences without a check.
///
/// # Panics
/// When the player has no session (ACE: `NullReferenceException`).
pub(crate) fn session(w: &World, this: ObjectGuid) -> SessionId {
    player_manager::player_session(w, this).expect("NullReferenceException: Player.Session")
}

/// `Session.Network.EnqueueSend(msgs...)`, in order.
pub(crate) fn send(w: &mut World, this: ObjectGuid, msgs: impl IntoIterator<Item = GameMessage>) {
    let s = session(w, this);
    for msg in msgs {
        enqueue_send(w, s, msg);
    }
}

/// `Name` for log lines (`Player.Name` is virtual; the plain property is enough for a log).
pub(crate) fn name(w: &World, this: ObjectGuid) -> String {
    obj(w, this)
        .get_property(empyrean_entity::enums::PropertyString::Name)
        .unwrap_or_default()
}

/// `new GameMessagePrivateUpdateSkill(this, creatureSkill)`.
fn update_skill_message(
    w: &mut World,
    this: ObjectGuid,
    creature_skill: CreatureSkill,
) -> GameMessage {
    game_message_private_update_skill(obj_mut(w, this), creature_skill)
}

/// `new GameMessagePrivateUpdatePropertyInt(this, PropertyInt.AvailableSkillCredits, AvailableSkillCredits ?? 0)`.
fn available_skill_credits_message(w: &mut World, this: ObjectGuid) -> GameMessage {
    let o = obj_mut(w, this);
    let v = o.available_skill_credits().unwrap_or(0);
    game_message_private_update_property_int(o, PropertyInt::AvailableSkillCredits, v)
}

/// `AvailableExperience` (`long?`) compared as `amount > AvailableExperience` (false for null).
pub(crate) fn exceeds_available_experience(w: &World, this: ObjectGuid, amount: i64) -> bool {
    obj(w, this)
        .available_experience()
        .is_some_and(|available| amount > available)
}

/// `AvailableSkillCredits += amount` on the `int?` property (null stays null).
fn add_available_skill_credits(w: &mut World, this: ObjectGuid, amount: i32) {
    let o = obj_mut(w, this);
    let v = o.available_skill_credits().map(|c| c.wrapping_add(amount));
    o.set_available_skill_credits(v);
}

/// `creditsSpent > AvailableSkillCredits` (false for null).
fn exceeds_available_skill_credits(w: &World, this: ObjectGuid, credits: i32) -> bool {
    obj(w, this)
        .available_skill_credits()
        .is_some_and(|available| credits > available)
}

/// `$"{AvailableSkillCredits}"`: an `int?` formats as its number, or empty for null.
fn available_skill_credits_text(w: &World, this: ObjectGuid) -> String {
    obj(w, this)
        .available_skill_credits()
        .map_or_else(String::new, |v| v.to_string())
}

/// `GetCreatureSkill(skill, add)` on the player.
fn creature_skill(
    w: &mut World,
    this: ObjectGuid,
    skill: Skill,
    add: bool,
) -> Option<CreatureSkill> {
    obj_mut(w, this).get_creature_skill(skill, add)
}

/// `DatManager.PortalDat.SkillTable.SkillBaseHash.TryGetValue((uint)skill, ...)`: the trained
/// and the specialized cost.
fn skill_base_costs(w: &World, skill: Skill) -> Option<(i32, i32)> {
    let key: u32 = skill.0.cs_cast();
    w.dats
        .portal_dat()
        .skill_table()
        .skills
        .get(&key)
        .map(|b| (b.trained_cost, b.specialized_cost))
}

// ---------------------------------------------------------------------------------------------
// Player_Skills.cs
// ---------------------------------------------------------------------------------------------

// ACE: Player.HandleActionRaiseSkill
/// GameAction 0x46 RaiseSkill: spends `amount` XP on a trained or specialized skill.
pub fn handle_action_raise_skill(
    w: &mut World,
    this: ObjectGuid,
    skill: Skill,
    amount: u32,
) -> bool {
    let creature_skill = creature_skill(w, this, skill, false);

    let Some(creature_skill) = creature_skill
        .filter(|cs| cs.advancement_class(obj(w, this)) >= SkillAdvancementClass::Trained)
    else {
        log::warn!(
            "{}.HandleActionRaiseSkill({}, {amount}) - trained or specialized skill not found",
            name(w, this),
            skill.to_dotnet_string()
        );
        return false;
    };

    if exceeds_available_experience(w, this, i64::from(amount)) {
        log::warn!(
            "{}.HandleActionRaiseSkill({}, {amount}) - amount > AvailableExperience",
            name(w, this),
            skill.to_dotnet_string()
        );
        return false;
    }

    let prev_rank = creature_skill.ranks(obj(w, this));

    if !spend_skill_xp(w, this, creature_skill, amount, true) {
        return false;
    }

    let update = update_skill_message(w, this, creature_skill);
    send(w, this, [update]);

    if prev_rank != creature_skill.ranks(obj(w, this)) {
        // if the skill ranks out at the top of our xp chart
        // then we will start fireworks effects and have special text!
        let mut suffix = "";
        if creature_skill.is_max_rank(w, obj(w, this)) {
            // fireworks on rank up is 0x8D
            play_particle_effect(w, this, PlayScript::WeddingBliss, this, 1.0f32);
            suffix = " and has reached its upper limit";
        }

        let sound = game_message_sound(this, Sound::RaiseTrait, 1.0f32);
        let base = creature_skill.base(w, obj(w, this));
        let msg = game_message_system_chat(
            &format!(
                "Your base {} skill is now {base}{suffix}!",
                skill.to_sentence()
            ),
            ChatMessageType::Advancement,
        );

        send(w, this, [sound, msg]);

        // retail was missing the 'raise skill' runrate hook here
        if skill == Skill::Run && property_manager_get_bool(w, "runrate_add_hooks") {
            handle_run_rate_update(w, this);
        }
    }

    true
}

// ACE: Player.SpendSkillXp
/// Spends `amount` on the skill if it is below max rank and the amount fits; recomputes ranks.
pub(crate) fn spend_skill_xp(
    w: &mut World,
    this: ObjectGuid,
    creature_skill: CreatureSkill,
    amount: u32,
    send_network_update: bool,
) -> bool {
    let advancement_class = creature_skill.advancement_class(obj(w, this));
    if get_skill_xp_table(w, advancement_class).is_none() {
        log::warn!(
            "{}.SpendSkillXp({}, {amount}) - player tried to raise {} skill",
            name(w, this),
            creature_skill.skill.to_dotnet_string(),
            advancement_class.to_dotnet_string()
        );
        return false;
    }

    // ensure skill is not already max rank
    if creature_skill.is_max_rank(w, obj(w, this)) {
        log::warn!(
            "{}.SpendSkillXp({}, {amount}) - player tried to raise skill beyond max rank",
            name(w, this),
            creature_skill.skill.to_dotnet_string()
        );
        return false;
    }

    // the client should already handle this naturally,
    // but ensure player can't spend xp beyond the max rank
    let amount_to_end = creature_skill.experience_left(w, obj(w, this));

    if amount > amount_to_end {
        //log.Warn($"{Name}.SpendSkillXp({creatureSkill.Skill}, {amount}) - player tried to raise skill beyond {amountToEnd} experience");
        return false; // returning error here, instead of setting amount to amountToEnd
    }

    // everything looks good at this point,
    // spend xp on skill
    if !spend_xp(w, this, i64::from(amount), send_network_update) {
        log::warn!(
            "{}.SpendSkillXp({}, {amount}) - SpendXP failed",
            name(w, this),
            creature_skill.skill.to_dotnet_string()
        );
        return false;
    }

    let o = obj_mut(w, this);
    let spent = creature_skill.experience_spent(o).wrapping_add(amount);
    creature_skill.set_experience_spent(o, spent);

    // calculate new rank
    let rank: u16 =
        calc_skill_rank(w, creature_skill.advancement_class(obj(w, this)), spent).cs_cast();
    creature_skill.set_ranks(obj_mut(w, this), rank);

    true
}

// ACE: Player.HandleActionTrainSkill
/// GameAction 0x47 TrainSkill: trains an untrained skill for its `SkillTable` cost.
pub fn handle_action_train_skill(
    w: &mut World,
    this: ObjectGuid,
    skill: Skill,
    credits_spent: i32,
) -> bool {
    if exceeds_available_skill_credits(w, this, credits_spent) {
        log::warn!(
            "{}.HandleActionTrainSkill({}, {credits_spent}) - not enough skill credits",
            name(w, this),
            skill.to_dotnet_string()
        );
        return false;
    }

    // get the actual cost to train the skill.
    let Some((trained_cost, _)) = skill_base_costs(w, skill) else {
        log::warn!(
            "{}.HandleActionTrainSkill({}, {credits_spent}) - couldn't find skill base",
            name(w, this),
            skill.to_dotnet_string()
        );
        return false;
    };

    if credits_spent != trained_cost {
        log::warn!(
            "{}.HandleActionTrainSkill({}, {credits_spent}) - client value differs from skillBase.TrainedCost({trained_cost})",
            name(w, this),
            skill.to_dotnet_string()
        );
        return false;
    }

    // attempt to train the specified skill
    let success = train_skill_with(w, this, skill, credits_spent, false);

    let available_skill_credits = format!(
        "You now have {} credits available.",
        available_skill_credits_text(w, this)
    );

    if success {
        let cs =
            creature_skill(w, this, skill, true).expect("GetCreatureSkill(skill) always answers");
        let update_skill = update_skill_message(w, this, cs);
        let skill_credits = available_skill_credits_message(w, this);

        let msg = game_message_system_chat(
            &format!("{} trained. {available_skill_credits}", skill.to_sentence()),
            ChatMessageType::Advancement,
        );

        send(w, this, [update_skill, skill_credits, msg]);
    } else {
        let msg = game_message_system_chat(
            &format!(
                "Failed to train {}! {available_skill_credits}",
                skill.to_sentence()
            ),
            ChatMessageType::Advancement,
        );
        send(w, this, [msg]);
    }

    success
}

// ACE: Player.TrainSkill
/// `TrainSkill(skill)`: trains for the `SkillTable`'s trained cost.
pub fn train_skill(w: &mut World, this: ObjectGuid, skill: Skill) -> bool {
    // get the amount of skill credits required to train this skill
    let Some((trained_cost, _)) = skill_base_costs(w, skill) else {
        log::error!(
            "{}.TrainSkill({}) - couldn't find skill base",
            name(w, this),
            skill.to_dotnet_string()
        );
        return false;
    };

    // attempt to train the specified skill
    train_skill_with(w, this, skill, trained_cost, false)
}

// ACE: Player.TrainSkill
/// `TrainSkill(skill, creditsSpent, applyCreationBonusXP)`: sets the skill to trained status.
pub fn train_skill_with(
    w: &mut World,
    this: ObjectGuid,
    skill: Skill,
    credits_spent: i32,
    apply_creation_bonus_xp: bool,
) -> bool {
    let cs = creature_skill(w, this, skill, true).expect("GetCreatureSkill(skill) always answers");

    if cs.advancement_class(obj(w, this)) >= SkillAdvancementClass::Trained
        || exceeds_available_skill_credits(w, this, credits_spent)
    {
        return false;
    }

    let o = obj_mut(w, this);
    cs.set_advancement_class(o, SkillAdvancementClass::Trained);
    cs.set_ranks(o, 0);
    cs.set_init_level(o, 0);

    if apply_creation_bonus_xp {
        cs.set_experience_spent(o, 526);
        cs.set_ranks(o, 5);
    } else {
        cs.set_experience_spent(o, 0);
    }

    add_available_skill_credits(w, this, credits_spent.wrapping_neg());

    // Tinkering skills can be reset at Asheron's Castle and Enlightenment, so if player has the augmentation when they train the skill again immediately specialize it again.
    let (is_aug_spec, player_has_augmentation) =
        is_skill_specialized_via_augmentation(obj(w, this), skill);
    if is_aug_spec && player_has_augmentation {
        specialize_skill_with(w, this, skill, 0, false);
    }

    true
}

// ACE: Player.SpecializeSkill
/// `SpecializeSkill(skill, resetSkill)`: specializes for the `SkillTable`'s upgrade cost.
// ACE-BUG: `resetSkill` is never passed on: the inner call takes its default (true), so a
// temple/castle caller passing false still resets the ranks and experience.
pub fn specialize_skill(w: &mut World, this: ObjectGuid, skill: Skill, reset_skill: bool) -> bool {
    // get the amount of skill credits required to upgrade this skill
    // from trained -> specialized
    let Some((trained_cost, specialized_cost)) = skill_base_costs(w, skill) else {
        log::error!(
            "{}.SpecializeSkill({}, {reset_skill}) - couldn't find skill base",
            name(w, this),
            skill.to_dotnet_string()
        );
        return false;
    };

    // attempt to specialize the specified skill
    specialize_skill_with(
        w,
        this,
        skill,
        specialized_cost.wrapping_sub(trained_cost),
        true,
    )
}

// ACE: Player.SpecializeSkill
/// `SpecializeSkill(skill, creditsSpent, resetSkill)`: sets the skill to specialized status.
/// `reset_skill` is true only during character creation, false at the temple / Asheron's Castle.
pub fn specialize_skill_with(
    w: &mut World,
    this: ObjectGuid,
    skill: Skill,
    credits_spent: i32,
    reset_skill: bool,
) -> bool {
    let cs = creature_skill(w, this, skill, true).expect("GetCreatureSkill(skill) always answers");

    if cs.advancement_class(obj(w, this)) != SkillAdvancementClass::Trained
        || exceeds_available_skill_credits(w, this, credits_spent)
    {
        return false;
    }

    if reset_skill {
        // this path only during char creation
        let o = obj_mut(w, this);
        cs.set_ranks(o, 0);
        cs.set_experience_spent(o, 0);
    } else {
        // this path only during temple / asheron's castle
        let spent = cs.experience_spent(obj(w, this));
        let rank: u16 = calc_skill_rank(w, SkillAdvancementClass::Specialized, spent).cs_cast();
        cs.set_ranks(obj_mut(w, this), rank);
    }

    let o = obj_mut(w, this);
    cs.set_init_level(o, 10);
    cs.set_advancement_class(o, SkillAdvancementClass::Specialized);

    add_available_skill_credits(w, this, credits_spent.wrapping_neg());

    true
}

// ACE: Player.UntrainSkill
/// Sets the skill to untrained status, refunding its experience (and credits for an untrainable
/// skill); a specialized skill is refused.
pub fn untrain_skill(w: &mut World, this: ObjectGuid, skill: Skill, credits_spent: i32) -> bool {
    let cs = creature_skill(w, this, skill, true);

    let Some(cs) =
        cs.filter(|cs| cs.advancement_class(obj(w, this)) != SkillAdvancementClass::Specialized)
    else {
        return false;
    };

    if cs.advancement_class(obj(w, this)) < SkillAdvancementClass::Trained {
        // only used to initialize untrained skills for character creation?
        let o = obj_mut(w, this);
        cs.set_advancement_class(o, SkillAdvancementClass::Untrained); // should this always be Untrained? what about Inactive?
        cs.set_init_level(o, 0);
        cs.set_ranks(o, 0);
        cs.set_experience_spent(o, 0);
    } else {
        // refund xp and skill credits
        let spent = cs.experience_spent(obj(w, this));
        refund_xp(w, this, i64::from(spent));

        // temple untraining 'always trained' skills:
        // cannot be untrained, but skill XP can be recovered
        if is_skill_untrainable(skill) {
            let o = obj_mut(w, this);
            cs.set_advancement_class(o, SkillAdvancementClass::Untrained);
            cs.set_init_level(o, 0);
            add_available_skill_credits(w, this, credits_spent);
        }

        let o = obj_mut(w, this);
        cs.set_ranks(o, 0);
        cs.set_experience_spent(o, 0);
    }

    true
}

// ACE: Player.UnspecializeSkill
/// Lowers a skill from specialized to trained and returns both skill credits and invested XP
/// (only the XP for a skill specialized through an augmentation).
pub fn unspecialize_skill(
    w: &mut World,
    this: ObjectGuid,
    skill: Skill,
    credits_spent: i32,
) -> bool {
    let cs = creature_skill(w, this, skill, true);

    let Some(cs) =
        cs.filter(|cs| cs.advancement_class(obj(w, this)) == SkillAdvancementClass::Specialized)
    else {
        return false;
    };

    // refund xp and skill credits
    let spent = cs.experience_spent(obj(w, this));
    refund_xp(w, this, i64::from(spent));

    // salvaging / tinkering skills specialized through augmentation only
    // cannot be unspecialized here, only refund xp
    let (is_aug_spec, player_has_augmentation) =
        is_skill_specialized_via_augmentation(obj(w, this), skill);
    if !is_aug_spec || !player_has_augmentation {
        let o = obj_mut(w, this);
        cs.set_advancement_class(o, SkillAdvancementClass::Trained);
        cs.set_init_level(o, 0);
        add_available_skill_credits(w, this, credits_spent);
    }

    let o = obj_mut(w, this);
    cs.set_ranks(o, 0);
    cs.set_experience_spent(o, 0);

    true
}

// ACE: Player.AwardSkillPoints
/// Increases a skill by some number of points.
// Not ACE's (fix, V297): the XP for all `amount` ranks (from the current rank,
// up to the skill's last rank) is granted at once. ACE granted the next rank's cost `amount`
// times, reading it before the raise it had queued had run, so several points bought fewer ranks.
pub fn award_skill_points(w: &mut World, this: ObjectGuid, skill: Skill, amount: u32) {
    let cs = creature_skill(w, this, skill, true).expect("GetCreatureSkill(skill) always answers");

    if amount == 0 || get_xp_to_next_rank(w, obj(w, this), cs).is_none() {
        return;
    }
    let o = obj(w, this);
    let table = get_skill_xp_table(w, cs.advancement_class(o))
        .expect("NullReferenceException: GetSkillXPTable");
    let target = (usize::from(cs.ranks(o))
        .saturating_add(usize::try_from(amount).unwrap_or(usize::MAX)))
    .min(table.len() - 1);
    let xp = table[target].wrapping_sub(cs.experience_spent(o));

    // AwardSkillXP caps the grant at the skill's remaining XP
    award_skill_xp(w, this, skill, xp, false);
}

// ACE: Player.AwardSkillXP
/// Grants `amount` XP (capped at the skill's remaining XP) and raises the skill by it one tick
/// later.
pub fn award_skill_xp(
    w: &mut World,
    this: ObjectGuid,
    skill: Skill,
    amount: u32,
    alert_player: bool,
) {
    let player_skill =
        creature_skill(w, this, skill, true).expect("GetCreatureSkill(skill) always answers");

    if player_skill.advancement_class(obj(w, this)) < SkillAdvancementClass::Trained
        || player_skill.is_max_rank(w, obj(w, this))
    {
        return;
    }

    let amount = amount.min(player_skill.experience_left(w, obj(w, this)));

    grant_xp(w, this, i64::from(amount), XpType::Emote, ShareType::None);
    let mut raise_chain = ActionChain::new();
    raise_chain.add_delay_for_one_tick(w);
    raise_chain.add_action(Actor::Object(this), move |w| {
        handle_action_raise_skill(w, this, skill, amount);
    });
    raise_chain.enqueue_chain(w);

    if alert_player {
        let msg = game_message_system_chat(
            &format!(
                "You've earned {} experience in your {} skill.",
                dotnet_format(amount, "N0"),
                player_skill.skill.to_sentence()
            ),
            ChatMessageType::Broadcast,
        );
        send(w, this, [msg]);
    }
}

// ACE: Player.SpendAllAvailableSkillXp
/// Spends as much available XP on the skill as it can take.
pub fn spend_all_available_skill_xp(
    w: &mut World,
    this: ObjectGuid,
    creature_skill: CreatureSkill,
    send_network_update: bool,
) {
    let mut amount_remaining = creature_skill.experience_left(w, obj(w, this));

    if exceeds_available_experience(w, this, i64::from(amount_remaining)) {
        let available = obj(w, this).available_experience().unwrap_or(0);
        amount_remaining = available.cs_cast();
    }

    spend_skill_xp(
        w,
        this,
        creature_skill,
        amount_remaining,
        send_network_update,
    );
}

// ACE: Player.GrantLevelProportionalSkillXP
/// Grants skill XP proportional to the XP of the skill's next rank, within `[min, max]`.
pub fn grant_level_proportional_skill_xp(
    w: &mut World,
    this: ObjectGuid,
    skill: Skill,
    percent: f64,
    min: i64,
    max: i64,
) {
    let Some(cs) = creature_skill(w, this, skill, false) else {
        return;
    };
    if cs.is_max_rank(w, obj(w, this)) {
        return;
    }

    let (advancement_class, ranks) = {
        let o = obj(w, this);
        (cs.advancement_class(o), i32::from(cs.ranks(o)))
    };
    let Some(next_level_xp) =
        get_xp_between_skill_levels(w, advancement_class, ranks, ranks.wrapping_add(1))
    else {
        return;
    };

    #[allow(clippy::cast_precision_loss)]
    let mut amount: u32 = math_round(next_level_xp as f64 * percent).cs_cast();

    if max > 0 && max <= i64::from(u32::MAX) {
        amount = amount.min(max.cs_cast());
    }

    amount = amount.min(cs.experience_left(w, obj(w, this)));

    if min > 0 {
        amount = amount.max(min.cs_cast());
    }

    //Console.WriteLine($"{Name}.GrantLevelProportionalSkillXP({skill}, {percent}, {max:N0})");
    //Console.WriteLine($"Amount: {amount:N0}");

    award_skill_xp(w, this, skill, amount, true);
}

// ACE: Player.GetXpToNextRank
/// The XP still needed for the next rank; `None` below trained or at max rank (`uint`: wraps if
/// more than the next rank's total has been spent).
#[must_use]
pub fn get_xp_to_next_rank(w: &World, o: &WorldObject, skill: CreatureSkill) -> Option<u32> {
    if skill.advancement_class(o) < SkillAdvancementClass::Trained || skill.is_max_rank(w, o) {
        return None;
    }

    let skill_xp_table = get_skill_xp_table(w, skill.advancement_class(o))
        .expect("NullReferenceException: GetSkillXPTable");

    Some(skill_xp_table[usize::from(skill.ranks(o)) + 1].wrapping_sub(skill.experience_spent(o)))
}

// ACE: Player.GetSkillXPTable
/// The XP curve for a trained or specialized skill; `None` otherwise.
#[must_use]
pub fn get_skill_xp_table(w: &World, status: SkillAdvancementClass) -> Option<&Vec<u32>> {
    let xp_table = w.dats.portal_dat().xp_table();

    match status {
        SkillAdvancementClass::Trained => Some(&xp_table.trained_xp),
        SkillAdvancementClass::Specialized => Some(&xp_table.specialized_xp),
        _ => None,
    }
}

// ACE: Player.GetXPBetweenSkillLevels
/// The skill XP between `from_rank` and `to_rank` (`uint` subtraction, so a lower `to_rank`
/// wraps); `None` for a status without a table.
///
/// # Panics
/// On a rank outside the table (ACE: `ArgumentOutOfRangeException`).
#[must_use]
pub fn get_xp_between_skill_levels(
    w: &World,
    status: SkillAdvancementClass,
    from_rank: i32,
    to_rank: i32,
) -> Option<u64> {
    let skill_xp_table = get_skill_xp_table(w, status)?;

    let at = |rank: i32| {
        usize::try_from(rank)
            .ok()
            .and_then(|i| skill_xp_table.get(i).copied())
            .unwrap_or_else(|| panic!("ArgumentOutOfRangeException: skill XP table index {rank}"))
    };
    let to = at(to_rank);
    let from = at(from_rank);
    Some(u64::from(to.wrapping_sub(from)))
}

// ACE: Player.CalcSkillRank
/// The highest rank `xp_amount` buys: the shared rules' search over the trained or specialized
/// table (see [`crate::world_objects::player_attributes::calc_attribute_rank`] for why ACE's -1
/// below the first entry is never reached).
///
/// # Panics
/// For a status without a table (ACE: `NullReferenceException`).
#[must_use]
pub fn calc_skill_rank(w: &World, sac: SkillAdvancementClass, xp_amount: u32) -> i32 {
    use dereth_rules::skills::Sac;

    let sac = match sac {
        SkillAdvancementClass::Trained => Sac::Trained,
        SkillAdvancementClass::Specialized => Sac::Specialized,
        _ => panic!("NullReferenceException: GetSkillXPTable"),
    };
    let xp_table = w.dats.portal_dat().xp_table();
    let rank = dereth_rules::advancement::skill_level_from_experience(xp_table, sac, xp_amount);
    i32::try_from(rank).expect("a List index fits an int")
}

// ACE: Player.magicSkillCheckMargin
const MAGIC_SKILL_CHECK_MARGIN: u32 = 50;

// ACE: Player.CanReadScroll
/// Whether the player's magic skill is high enough to learn the scroll's spell.
///
/// # Panics
/// When the scroll has no `Spell` (ACE: `NullReferenceException`).
pub fn can_read_scroll(w: &mut World, this: ObjectGuid, scroll: ObjectGuid) -> bool {
    let spell = w
        .objects
        .get(scroll)
        .and_then(|o| crate::world_objects::scroll::spell(o).cloned())
        .expect("System.NullReferenceException: scroll.Spell");
    let power = spell.power();

    // level 1/7/8 scrolls can be learned by anyone?
    if !(50..300).contains(&power) {
        return true;
    }

    let magic_skill = spell.get_magic_skill();
    let player_skill = w
        .objects
        .get_mut(this)
        .expect("ACE: this")
        .get_creature_skill(magic_skill, true)
        .expect("GetCreatureSkill adds a missing skill");

    let min_skill = power.wrapping_sub(MAGIC_SKILL_CHECK_MARGIN);

    let advancement_class = player_skill.advancement_class(w.objects.get(this).expect("ACE: this"));
    advancement_class.0 >= empyrean_entity::enums::SkillAdvancementClass::Trained.0
        && player_skill.current(w, this) >= min_skill
}

// ACE: Player.AddSkillCredits
/// Adds to both the total and the available skill credits, and tells the player.
pub fn add_skill_credits(w: &mut World, this: ObjectGuid, amount: i32) {
    {
        let o = obj_mut(w, this);
        let total = o.total_skill_credits().map(|c| c.wrapping_add(amount));
        o.set_total_skill_credits(total);
    }
    add_available_skill_credits(w, this, amount);

    let msg = available_skill_credits_message(w, this);
    send(w, this, [msg]);

    if amount > 1 {
        send_transient_error(
            w,
            this,
            &format!(
                "You have been awarded {} additional skill credits.",
                dotnet_format(amount, "N0")
            ),
        );
    } else {
        send_transient_error(w, this, "You have been awarded an additional skill credit.");
    }
}

// ACE: Player.HandleDBUpdates
/// On login: for trained Dirty Fighting, Void Magic or Summoning whose content is missing from
/// the world database, tells the player (3 seconds later) that the world data lacks it and logs
/// where an operator gets the patches.
// DIVERGE: ACE tells the player to apply ACE's world patches; ours tells the player to report it to the operator, and the patches' address goes to the operator's log (brand).
pub fn handle_db_updates(w: &mut World, this: ObjectGuid) {
    // dirty fighting
    let df_skill = creature_skill(w, this, Skill::DirtyFighting, true)
        .expect("GetCreatureSkill(skill) always answers");
    if df_skill.advancement_class(obj(w, this)) >= SkillAdvancementClass::Trained {
        // `foreach (var spellID in SpellExtensions.DirtyFightingSpells) { if (new Spell(spellID).NotFound) ...; break; }`
        if first_spell_not_found(w, "DirtyFightingSpells") {
            missing_skill_content(w, this, "Dirty Fighting");
        }
    }

    // void magic
    let void_skill = creature_skill(w, this, Skill::VoidMagic, true)
        .expect("GetCreatureSkill(skill) always answers");
    if void_skill.advancement_class(obj(w, this)) >= SkillAdvancementClass::Trained {
        // performance improvement: only check first spell (measured 102ms to check 75 uncached void spells)
        if first_spell_not_found(w, "VoidMagicSpells") {
            missing_skill_content(w, this, "Void Magic");
        }
    }

    // summoning
    let summoning = creature_skill(w, this, Skill::Summoning, true)
        .expect("GetCreatureSkill(skill) always answers");
    if summoning.advancement_class(obj(w, this)) >= SkillAdvancementClass::Trained {
        let essence_wcid: u32 = 48878;
        let weenie = w.content.get_cached_weenie(essence_wcid);
        if weenie.is_none() {
            missing_skill_content(w, this, "Summoning");
        }
    }
}

/// Tells the player (3 seconds later) that the world data lacks a skill's content, and logs
/// where the operator gets it.
fn missing_skill_content(w: &mut World, this: ObjectGuid, skill_name: &str) {
    let world_name = crate::sessions::config_server_world_name();
    log::warn!(
        "{world_name}'s world data does not include {skill_name}; apply the latest patches from https://github.com/ACEmulator/ACE-World-16PY-Patches to the world content"
    );
    let message = format!("{world_name}'s world data does not include {skill_name} yet; please report this to the server operator.");
    delayed_chat(
        w,
        this,
        3.0,
        message,
        ChatMessageType::Broadcast,
        Vec::new(),
    );
}

/// What a delayed chat action does after sending its message.
enum AfterChat {
    RemoveBool(PropertyBool),
    EraseQuest(&'static str),
}

/// `actionChain.AddDelaySeconds(secs); actionChain.AddAction(this, () => { EnqueueSend(new
/// GameMessageSystemChat(text, type)); <after> }); actionChain.EnqueueChain();`
fn delayed_chat(
    w: &mut World,
    this: ObjectGuid,
    secs: f64,
    text: impl Into<String>,
    chat_type: ChatMessageType,
    after: Vec<AfterChat>,
) {
    let text: String = text.into();
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, secs);
    action_chain.add_action(Actor::Object(this), move |w| {
        send(w, this, [game_message_system_chat(&text, chat_type)]);

        for a in after {
            match a {
                AfterChat::RemoveBool(p) => obj_mut(w, this).remove_property(p),
                AfterChat::EraseQuest(q) => quest_manager_erase(w, this, q),
            }
        }
    });
    action_chain.enqueue_chain(w);
}

/// `GetProperty(PropertyBool) ?? false`.
fn bool_prop(w: &World, this: ObjectGuid, p: PropertyBool) -> bool {
    obj(w, this).get_property(p).unwrap_or(false)
}

// ACE: Player.MeleeSkills
/// The melee skills (a `HashSet`; only `Contains` is used).
pub const MELEE_SKILLS: &[Skill] = &[
    Skill::LightWeapons,
    Skill::HeavyWeapons,
    Skill::FinesseWeapons,
    Skill::DualWield,
    Skill::TwoHandedCombat,
    // legacy
    Skill::Axe,
    Skill::Dagger,
    Skill::Mace,
    Skill::Spear,
    Skill::Staff,
    Skill::Sword,
    Skill::UnarmedCombat,
];

// ACE: Player.MissileSkills
pub const MISSILE_SKILLS: &[Skill] = &[
    Skill::MissileWeapons,
    // legacy
    Skill::Bow,
    Skill::Crossbow,
    Skill::Sling,
    Skill::ThrownWeapon,
];

// ACE: Player.MagicSkills
pub const MAGIC_SKILLS: &[Skill] = &[
    Skill::CreatureEnchantment,
    Skill::ItemEnchantment,
    Skill::LifeMagic,
    Skill::VoidMagic,
    Skill::WarMagic,
];

// ACE: Player.AlwaysTrained
pub const ALWAYS_TRAINED: &[Skill] = &[
    Skill::ArcaneLore,
    Skill::Jump,
    Skill::Loyalty,
    Skill::MagicDefense,
    Skill::Run,
    Skill::Salvaging,
];

// ACE: Player.AugSpecSkills
pub const AUG_SPEC_SKILLS: &[Skill] = &[
    Skill::ArmorTinkering,
    Skill::ItemTinkering,
    Skill::MagicItemTinkering,
    Skill::WeaponTinkering,
    Skill::Salvaging,
];

// ACE: Player.IsSkillUntrainable
/// True unless the skill is one of the always-trained ones.
#[must_use]
pub fn is_skill_untrainable(skill: Skill) -> bool {
    !ALWAYS_TRAINED.contains(&skill)
}

// ACE: Player.IsSkillSpecializedViaAugmentation
/// `(AugSpecSkills.Contains(skill), playerHasAugmentation)`: whether the skill is one an
/// augmentation specializes, and whether this player has that augmentation.
#[must_use]
pub fn is_skill_specialized_via_augmentation(player: &WorldObject, skill: Skill) -> (bool, bool) {
    let mut player_has_augmentation = false;

    match skill {
        Skill::ArmorTinkering => {
            player_has_augmentation = player.augmentation_specialize_armor_tinkering() > 0
        }
        Skill::ItemTinkering => {
            player_has_augmentation = player.augmentation_specialize_item_tinkering() > 0
        }
        Skill::MagicItemTinkering => {
            player_has_augmentation = player.augmentation_specialize_magic_item_tinkering() > 0
        }
        Skill::WeaponTinkering => {
            player_has_augmentation = player.augmentation_specialize_weapon_tinkering() > 0
        }
        Skill::Salvaging => {
            player_has_augmentation = player.augmentation_specialize_salvaging() > 0
        }
        _ => {}
    }

    (AUG_SPEC_SKILLS.contains(&skill), player_has_augmentation)
}

// ACE: Player.GetHeritageBonus
/// The Player override: a masterable weapon gets the heritage bonus always under
/// `universal_masteries` (end of retail), else by heritage and weapon type.
pub fn player_get_heritage_bonus(w: &World, this: ObjectGuid, weapon: ObjectGuid) -> bool {
    // DIVERGE: an era without the heritage weapon masteries (`EraFeatures::weapon_masteries`)
    // gives no heritage bonus (ClassicACE's `GetHeritageBonus` at its older rulesets).
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Skills.cs
    if !w.era.features.weapon_masteries {
        return false;
    }
    let Some(weapon_obj) = w.objects.get(weapon) else {
        return false;
    };
    if !is_masterable(weapon_obj) {
        return false;
    }

    if property_manager_get_bool(w, "universal_masteries") {
        // https://asheron.fandom.com/wiki/Spring_2014_Update
        // end of retail - universal masteries
        true
    } else {
        let weapon_type = get_weapon_type(Some(weapon_obj));
        get_heritage_bonus_weapon_type(obj(w, this), weapon_type)
    }
}

// ACE: Player.GetHeritageBonus
/// The `WeaponType` overload: each heritage's two mastery weapon types.
#[must_use]
#[allow(clippy::collapsible_match)] // ACE's switch of ifs, kept as written
pub fn get_heritage_bonus_weapon_type(player: &WorldObject, weapon_type: WeaponType) -> bool {
    use empyrean_entity::enums::HeritageGroup;

    match player.heritage_group() {
        HeritageGroup::Aluvian => {
            if weapon_type == WeaponType::Dagger || weapon_type == WeaponType::Bow {
                return true;
            }
        }
        HeritageGroup::Gharundim => {
            if weapon_type == WeaponType::Staff || weapon_type == WeaponType::Magic {
                return true;
            }
        }
        HeritageGroup::Sho => {
            if weapon_type == WeaponType::Unarmed || weapon_type == WeaponType::Bow {
                return true;
            }
        }
        HeritageGroup::Viamontian => {
            if weapon_type == WeaponType::Sword || weapon_type == WeaponType::Crossbow {
                return true;
            }
        }
        // umbraen
        HeritageGroup::Shadowbound | HeritageGroup::Penumbraen => {
            if weapon_type == WeaponType::Unarmed || weapon_type == WeaponType::Crossbow {
                return true;
            }
        }
        HeritageGroup::Gearknight => {
            if weapon_type == WeaponType::Mace || weapon_type == WeaponType::Crossbow {
                return true;
            }
        }
        HeritageGroup::Undead => {
            if weapon_type == WeaponType::Axe || weapon_type == WeaponType::Thrown {
                return true;
            }
        }
        HeritageGroup::Empyrean => {
            if weapon_type == WeaponType::Sword || weapon_type == WeaponType::Magic {
                return true;
            }
        }
        HeritageGroup::Tumerok => {
            if weapon_type == WeaponType::Spear || weapon_type == WeaponType::Thrown {
                return true;
            }
        }
        HeritageGroup::Lugian => {
            if weapon_type == WeaponType::Axe || weapon_type == WeaponType::Thrown {
                return true;
            }
        }
        _ => {}
    }
    false
}

// ACE: Player.GetWeaponType
/// The weapon's `WeaponType`, converted from its `WeaponSkill` for old data; `Magic` for a
/// caster, `Undef` for no weapon.
#[must_use]
pub fn get_weapon_type(weapon: Option<&WorldObject>) -> WeaponType {
    let Some(weapon) = weapon else {
        return WeaponType::Undef; // unarmed?
    };

    if weapon.is_caster() {
        return WeaponType::Magic;
    }

    if let Some(weapon_type) = weapon.get_property(PropertyInt::WeaponType) {
        return WeaponType(weapon_type);
    }

    let weapon_skill = weapon.get_property(PropertyInt::WeaponSkill);
    match weapon_skill.and_then(|s| skill_to_weapon_type(Skill(s))) {
        Some(converted) => converted,
        None => WeaponType::Undef,
    }
}

// ACE: Player.SkillToWeaponType
/// `SkillToWeaponType.TryGetValue(skill, out converted)`.
#[must_use]
pub fn skill_to_weapon_type(skill: Skill) -> Option<WeaponType> {
    Some(match skill {
        Skill::UnarmedCombat => WeaponType::Unarmed,
        Skill::Sword => WeaponType::Sword,
        Skill::Axe => WeaponType::Axe,
        Skill::Mace => WeaponType::Mace,
        Skill::Spear => WeaponType::Spear,
        Skill::Dagger => WeaponType::Dagger,
        Skill::Staff => WeaponType::Staff,
        Skill::Bow => WeaponType::Bow,
        Skill::Crossbow => WeaponType::Crossbow,
        Skill::ThrownWeapon => WeaponType::Thrown,
        Skill::TwoHandedCombat => WeaponType::TwoHanded,
        Skill::CreatureEnchantment => WeaponType::Magic, // only for war/void?
        Skill::ItemEnchantment => WeaponType::Magic,
        Skill::LifeMagic => WeaponType::Magic,
        Skill::WarMagic => WeaponType::Magic,
        Skill::VoidMagic => WeaponType::Magic,
        _ => return None,
    })
}

// ACE: Player.HandleSkillCreditRefund
pub fn handle_skill_credit_refund(w: &mut World, this: ObjectGuid) {
    if !bool_prop(w, this, PropertyBool::UntrainedSkills) {
        return;
    }

    delayed_chat(
        w,
        this,
        5.0,
        "Your trained skills have been reset due to an error with skill credits.\nYou have received a refund for these skill credits and experience.",
        ChatMessageType::Broadcast,
        vec![AfterChat::RemoveBool(PropertyBool::UntrainedSkills)],
    );
}

// ACE: Player.HandleSkillSpecCreditRefund
pub fn handle_skill_spec_credit_refund(w: &mut World, this: ObjectGuid) {
    if !bool_prop(w, this, PropertyBool::UnspecializedSkills) {
        return;
    }

    delayed_chat(
        w,
        this,
        5.0,
        "Your specialized skills have been unspecialized due to an error with skill credits.\nYou have received a refund for these skill credits and experience.",
        ChatMessageType::Broadcast,
        vec![AfterChat::RemoveBool(PropertyBool::UnspecializedSkills)],
    );
}

// ACE: Player.HandleFreeSkillResetRenewal
pub fn handle_free_skill_reset_renewal(w: &mut World, this: ObjectGuid) {
    if !bool_prop(w, this, PropertyBool::FreeSkillResetRenewed) {
        return;
    }

    delayed_chat(
        w,
        this,
        5.0,
        "Your opportunity to change your skills is renewed! Visit Fianhe to reset your skills.",
        ChatMessageType::Magic,
        vec![
            AfterChat::RemoveBool(PropertyBool::FreeSkillResetRenewed),
            AfterChat::EraseQuest("UsedFreeSkillReset"),
        ],
    );
}

// ACE: Player.HandleFreeAttributeResetRenewal
pub fn handle_free_attribute_reset_renewal(w: &mut World, this: ObjectGuid) {
    if !bool_prop(w, this, PropertyBool::FreeAttributeResetRenewed) {
        return;
    }

    // Your opportunity to change your attributes is renewed! Visit Chafulumisa to reset your skills [sic attributes].
    delayed_chat(
        w,
        this,
        5.0,
        "Your opportunity to change your attributes is renewed! Visit Chafulumisa to reset your attributes.",
        ChatMessageType::Magic,
        vec![AfterChat::RemoveBool(PropertyBool::FreeAttributeResetRenewed), AfterChat::EraseQuest("UsedFreeAttributeReset")],
    );
}

// ACE: Player.HandleSkillTemplesReset
pub fn handle_skill_temples_reset(w: &mut World, this: ObjectGuid) {
    if !bool_prop(w, this, PropertyBool::SkillTemplesTimerReset) {
        return;
    }

    delayed_chat(
        w,
        this,
        5.0,
        "The Temples of Forgetfulness and Enlightenment have had the timer for their use reset due to skill changes.",
        ChatMessageType::Magic,
        vec![
            AfterChat::RemoveBool(PropertyBool::SkillTemplesTimerReset),
            AfterChat::EraseQuest("ForgetfulnessGems1"),
            AfterChat::EraseQuest("ForgetfulnessGems2"),
            AfterChat::EraseQuest("ForgetfulnessGems3"),
            AfterChat::EraseQuest("ForgetfulnessGems4"),
            AfterChat::EraseQuest("Forgetfulness6days"),
            AfterChat::EraseQuest("Forgetfulness13days"),
            AfterChat::EraseQuest("Forgetfulness20days"),
        ],
    );
}

// ACE: Player.HandleFreeMasteryResetRenewal
pub fn handle_free_mastery_reset_renewal(w: &mut World, this: ObjectGuid) {
    if !bool_prop(w, this, PropertyBool::FreeMasteryResetRenewed) {
        return;
    }

    delayed_chat(
        w,
        this,
        5.0,
        "Your opportunity to change your Masteries is renewed!",
        ChatMessageType::Magic,
        vec![
            AfterChat::RemoveBool(PropertyBool::FreeMasteryResetRenewed),
            AfterChat::EraseQuest("UsedFreeMeleeMasteryReset"),
            AfterChat::EraseQuest("UsedFreeRangedMasteryReset"),
            AfterChat::EraseQuest("UsedFreeSummoningMasteryReset"),
        ],
    );
}

// ACE: Player.ResetSkill
/// Resets the skill, refunding all experience and skill credits where allowed.
pub fn reset_skill(w: &mut World, this: ObjectGuid, skill: Skill, refund: bool) -> bool {
    let cs = creature_skill(w, this, skill, false);

    let Some(cs) =
        cs.filter(|cs| cs.advancement_class(obj(w, this)) >= SkillAdvancementClass::Trained)
    else {
        return false;
    };

    // gather skill credits to refund
    let Some((trained_cost, specialized_cost)) = skill_base_costs(w, cs.skill) else {
        return false;
    };
    let upgrade_cost_from_trained_to_specialized = specialized_cost.wrapping_sub(trained_cost);

    // salvage / tinkering skills specialized via augmentations
    // Salvaging cannot be untrained or unspecialized => skillIsSpecializedViaAugmentation && !untrainable
    let (_, skill_is_specialized_via_augmentation) =
        is_skill_specialized_via_augmentation(obj(w, this), cs.skill);

    let advancement_class = cs.advancement_class(obj(w, this));
    let type_of_skill = advancement_class.to_dotnet_string().to_lowercase() + " ";
    let untrainable = is_skill_untrainable(skill);
    let credit_refund = (advancement_class == SkillAdvancementClass::Specialized
        && !(skill_is_specialized_via_augmentation && !untrainable))
        || untrainable;

    if advancement_class == SkillAdvancementClass::Specialized
        && !(skill_is_specialized_via_augmentation && !untrainable)
    {
        let o = obj_mut(w, this);
        cs.set_advancement_class(o, SkillAdvancementClass::Trained);
        cs.set_init_level(o, 0);
        if !skill_is_specialized_via_augmentation {
            // Tinkering skills can be unspecialized, but do not refund upgrade cost.
            add_available_skill_credits(w, this, upgrade_cost_from_trained_to_specialized);
        }
    }

    // temple untraining 'always trained' skills:
    // cannot be untrained, but skill XP can be recovered
    if untrainable {
        let o = obj_mut(w, this);
        cs.set_advancement_class(o, SkillAdvancementClass::Untrained);
        cs.set_init_level(o, 0);
        add_available_skill_credits(w, this, trained_cost);
    }

    if refund {
        let spent = cs.experience_spent(obj(w, this));
        refund_xp(w, this, i64::from(spent));
    }

    let o = obj_mut(w, this);
    cs.set_experience_spent(o, 0);
    cs.set_ranks(o, 0);

    let update_skill = update_skill_message(w, this, cs);
    let available_skill_credits = available_skill_credits_message(w, this);

    let mut msg = format!(
        "Your {}{} skill has been {}. ",
        if untrainable {
            type_of_skill.as_str()
        } else {
            ""
        },
        skill.to_sentence(),
        if untrainable { "removed" } else { "reset" }
    );
    msg += &format!(
        "All the experience {}that you spent on this skill have been refunded to you.",
        if credit_refund {
            "and skill credits "
        } else {
            ""
        }
    );

    if refund {
        send(
            w,
            this,
            [
                update_skill,
                available_skill_credits,
                game_message_system_chat(&msg, ChatMessageType::Broadcast),
            ],
        );
    } else {
        send(
            w,
            this,
            [
                update_skill,
                game_message_system_chat(&msg, ChatMessageType::Broadcast),
            ],
        );
    }

    true
}

// ACE: Player.PlayerSkills
/// All of the skills players have access to at the end of retail (a `HashSet`; only `Contains`
/// is used).
pub const PLAYER_SKILLS: &[Skill] = &[
    Skill::MeleeDefense,
    Skill::MissileDefense,
    Skill::ArcaneLore,
    Skill::MagicDefense,
    Skill::ManaConversion,
    Skill::ItemTinkering,
    Skill::AssessPerson,
    Skill::Deception,
    Skill::Healing,
    Skill::Jump,
    Skill::Lockpick,
    Skill::Run,
    Skill::AssessCreature,
    Skill::WeaponTinkering,
    Skill::ArmorTinkering,
    Skill::MagicItemTinkering,
    Skill::CreatureEnchantment,
    Skill::ItemEnchantment,
    Skill::LifeMagic,
    Skill::WarMagic,
    Skill::Leadership,
    Skill::Loyalty,
    Skill::Fletching,
    Skill::Alchemy,
    Skill::Cooking,
    Skill::Salvaging,
    Skill::TwoHandedCombat,
    Skill::VoidMagic,
    Skill::HeavyWeapons,
    Skill::LightWeapons,
    Skill::FinesseWeapons,
    Skill::MissileWeapons,
    Skill::Shield,
    Skill::DualWield,
    Skill::Recklessness,
    Skill::SneakAttack,
    Skill::DirtyFighting,
    Skill::Summoning,
];

// ---------------------------------------------------------------------------------------------
// Player_Xp.cs members the raise paths need: ported in `player_xp.rs`, re-exported here for this
// file's callers.
// ---------------------------------------------------------------------------------------------

pub use crate::world_objects::player_xp::{refund_xp, spend_xp};

// ---------------------------------------------------------------------------------------------
// Not ACE: pointers to members ported in other files, named after them.
// ---------------------------------------------------------------------------------------------

/// `Player.GrantXP(amount, xpType, shareType)` (`Player_Xp.cs`).
fn grant_xp(w: &mut World, this: ObjectGuid, amount: i64, xp_type: XpType, share_type: ShareType) {
    crate::world_objects::player_xp::grant_xp(w, this, amount, xp_type, share_type);
}

/// `Player.HandleRunRateUpdate()` (`Player.cs`).
pub(crate) fn handle_run_rate_update(w: &mut World, this: ObjectGuid) {
    crate::world_objects::player::handle_run_rate_update(w, this);
}

/// `QuestManager.Erase(questName)` (`Managers/QuestManager.cs`).
fn quest_manager_erase(w: &mut World, this: ObjectGuid, quest_name: &str) {
    crate::managers::quest_manager::erase(
        w,
        &mut crate::managers::quest_manager::QuestOwner::Creature(this),
        quest_name,
    );
}

/// `new Spell(spellID).NotFound` for the first spell of a `SpellExtensions` list (ACE breaks after
/// the first).
fn first_spell_not_found(w: &World, list: &str) -> bool {
    use empyrean_entity::enums::ext::spell_extensions;
    let spells = match list {
        "DirtyFightingSpells" => spell_extensions::DIRTY_FIGHTING_SPELLS,
        "VoidMagicSpells" => spell_extensions::VOID_MAGIC_SPELLS,
        _ => unreachable!("SpellExtensions has no {list} list"),
    };
    spells
        .first()
        .is_some_and(|&id| crate::entity::spell::Spell::from_spell_id(w, id, true).not_found())
}

/// `Player.SendTransientError(msg)` (`Player_Networking.cs`), whose body is one send:
/// `Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, msg))`.
pub(crate) fn send_transient_error(w: &mut World, this: ObjectGuid, msg: &str) {
    let s = session(w, this);
    let session_data = w
        .sessions
        .get_mut(s)
        .expect("NullReferenceException: Player.Session");
    let m = game_event_communication_transient_string(session_data, msg);
    enqueue_send(w, s, m);
}

/// `WorldObject.IsMasterable` (`WorldObject_Weapon.cs`): true unless the long description says
/// "This weapon seems tough to master." (ignoring case). ACE caches the answer per object; this
/// recomputes it, which differs only if `LongDesc` changes after the first read.
fn is_masterable(weapon: &WorldObject) -> bool {
    let long_desc = weapon.get_property(empyrean_entity::enums::PropertyString::LongDesc);
    match long_desc {
        None => true,
        Some(d) => !d
            .to_lowercase()
            .contains(&"This weapon seems tough to master.".to_lowercase()),
    }
}

/// `PropertyManager.GetBool(key).Item`.
pub(crate) fn property_manager_get_bool(w: &World, key: &str) -> bool {
    crate::managers::property_manager::get_bool(w, key, false, true).item
}
