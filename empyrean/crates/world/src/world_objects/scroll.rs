// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Scroll.cs
//! Port of `Source/ACE.Server/WorldObjects/Scroll.cs`.
//!
//! A spell scroll: reading it (the Reading motion, one second) teaches its spell through
//! `Player.LearnSpellWithNetworking` and destroys it.

use empyrean_entity::enums::{
    ChatMessageType, CombatMode, MotionCommand, SkillAdvancementClass, WeenieError,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::spell::Spell;
use crate::managers::player_manager::player_session;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::kinds::KindData;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_combat, food, player_inventory, player_networking, player_skills, player_spells,
    player_use, world_object_networking,
};
use crate::{dispatch, World};

/// Non-property fields declared in `Scroll.cs`.
#[derive(Debug, Default)]
pub struct ScrollFields {
    /// The inscribed spell (`new Spell(SpellDID.Value, false)`), null without a `SpellDID`.
    // ACE: Scroll.Spell
    pub spell: Option<Spell>,
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

/// The scroll's `Spell` field.
///
/// # Panics
/// When `this` is not a Scroll.
#[must_use]
pub fn spell(o: &WorldObject) -> Option<&Spell> {
    match &o.kind {
        KindData::Scroll(s) => s.scroll.spell.as_ref(),
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a Scroll",
            o.guid.full()
        ),
    }
}

fn chat(w: &mut World, player: ObjectGuid, msg: &str) {
    let m = game_message_system_chat(msg, ChatMessageType::Broadcast);
    let session = player_session(w, player).expect("System.NullReferenceException: Player.Session");
    enqueue_send(w, session, m);
}

fn add_last_use_time(w: &mut World, player: ObjectGuid, seconds: f32) {
    player_use::fields_mut(w, player).last_use_time += seconds;
}

/// This is raised by Player.HandleActionUseItem. The item should be in the players possession.
/// (Research: <http://asheron.wikia.com/wiki/Announcements_-_2002/06_-_Castling>)
// ACE: Scroll.ActOnUse
pub fn scroll_act_on_use(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    let Some(spell) = spell(obj(w, this)).cloned() else {
        let name = dispatch::name::name(w, this).unwrap_or_default();
        let activator_name = dispatch::name::name(w, activator).unwrap_or_default();
        log::info!(
            "{name}.ActOnUse({activator_name}) - SpellDID not found for {}",
            obj(w, this).biota.weenie_class_id
        );
        return;
    };

    if food::player_is_busy_teleporting_or_suiciding(w, player) {
        player_networking::send_weenie_error(w, player, WeenieError::YoureTooBusy);
        return;
    }

    obj_mut(w, player).wo.world_object.is_busy = true;

    let mut action_chain = ActionChain::new();

    if creature_combat::combat_mode(w, player) != CombatMode::NonCombat {
        let stance_time = creature_combat::set_combat_mode(w, player, CombatMode::NonCombat);
        action_chain.add_delay_seconds(w, f64::from(stance_time));

        add_last_use_time(w, player, stance_time);
    }

    let anim_time = world_object_networking::enqueue_motion(
        w,
        player,
        &mut action_chain,
        MotionCommand::Reading,
        1.0,
        true,
        None,
        false,
        false,
    );
    add_last_use_time(w, player, anim_time);

    let read_time = 1.0f32;

    action_chain.add_delay_seconds(w, f64::from(read_time));
    add_last_use_time(w, player, read_time);

    action_chain.add_action(Actor::Object(player), move |w: &mut World| {
        if player_spells::spell_is_known(w, player, spell.id()) {
            // verify unknown spell
            chat(w, player, "You already know that spell!");
            return;
        }

        let skill = spell.get_magic_skill();
        let player_skill = obj_mut(w, player)
            .get_creature_skill(skill, true)
            .expect("GetCreatureSkill adds a missing skill");

        if !player_skills::can_read_scroll(w, player, this) {
            let advancement_class = player_skill.advancement_class(obj(w, player));
            let msg = if advancement_class.0 < SkillAdvancementClass::Trained.0 {
                format!(
                    "You are not trained in {}!",
                    player_skill.skill.to_sentence()
                )
            } else {
                format!(
                    "You are not skilled enough in {} to learn this spell.",
                    player_skill.skill.to_sentence()
                )
            };

            chat(w, player, &msg);
            return;
        }

        if player_inventory::try_consume_from_inventory_with_networking(w, player, this, i32::MAX) {
            player_spells::learn_spell_with_networking(w, player, spell.id(), true);

            chat(w, player, "The scroll is destroyed.");
        }
    });

    // FIXME: return stance time
    world_object_networking::enqueue_motion(
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

    add_last_use_time(w, player, anim_time); // return stance

    action_chain.add_delay_seconds(w, f64::from(anim_time));

    action_chain.add_action(Actor::Object(player), move |w: &mut World| {
        obj_mut(w, player).wo.world_object.is_busy = false
    });

    action_chain.enqueue_chain(w);
}

// ---- constructors and SetEphemeralValues ----

/// `new Scroll(weenie, guid)` / `new Scroll(biota)`: the `WorldObject` constructor, then
/// Scroll's `SetEphemeralValues`.
// ACE: Scroll.Scroll
pub fn scroll_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    scroll_set_ephemeral_values(o, env);
}

// ACE: Scroll.SetEphemeralValues
fn scroll_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    if let Some(spell_did) = o.spell_did() {
        let spell = Spell::new(_env.w, spell_did, false);

        if !spell.not_found() {
            o.set_long_desc(Some(format!(
                "Inscribed spell: {}\n{}",
                spell.name(),
                spell.description()
            )));
        }

        if let KindData::Scroll(s) = &mut o.kind {
            s.scroll.spell = Some(spell);
        }
    }

    o.set_use_(Some(
        "Use this item to attempt to learn its spell.".to_owned(),
    ));
}
