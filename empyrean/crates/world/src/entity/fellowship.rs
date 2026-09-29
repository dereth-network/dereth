// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Fellowship.cs
//! Port of `Source/ACE.Server/Entity/Fellowship.cs`.
//!
//! A `Fellowship` is a C# reference object shared by every member (`Player.Fellowship`). Here the
//! fellowships live in the world's store, `World.fellowships` ([`FellowshipStore`]), and a
//! [`FellowshipRef`] is the id each member holds in `PlayerFellowshipFields.fellowship`.
//! Equality is by id, which is C#'s reference equality (`player.Fellowship == this`). The members'
//! `WeakReference<Player>` become guids resolved in `World.objects` on use; a guid whose player is
//! gone, has no session, or no longer points at a fellowship is "dropped", as a collected or
//! logged-off `WeakReference` target is in ACE.
//!
//! The instance members are free functions taking `this: &FellowshipRef` and reading the
//! fellowship through the world (`this.get(w)`), never across a call that takes the `World`
//! (`GrantXP` re-enters `Fellowship` through `OnFellowLevelUp`). A fellowship no player refers to
//! any more (ACE's garbage) is removed from the store when the next one is created.

use empyrean_common::dotnet::{math, CsCast, DotNetDict, DotNetHashSet};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::time::Time;
use empyrean_entity::enums::{
    Channel, CharacterOption, ChatMessageType, FellowUpdateType, MotionCommand, MotionStance,
    ShareType, WeenieError, WeenieErrorWithString, XpType,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;

use crate::entity::i_player;
use crate::managers::quest_manager::QuestManager;
use crate::managers::{player_manager, property_manager};
use crate::network::game_event::events::game_event_channel_broadcast::game_event_channel_broadcast;
use crate::network::game_event::events::game_event_fellowship_disband::game_event_fellowship_disband;
use crate::network::game_event::events::game_event_fellowship_dismiss::game_event_fellowship_dismiss;
use crate::network::game_event::events::game_event_fellowship_full_update::game_event_fellowship_full_update;
use crate::network::game_event::events::game_event_fellowship_quit::game_event_fellowship_quit;
use crate::network::game_event::events::game_event_fellowship_update_fellow::game_event_fellowship_update_fellow;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::structure::fellowship_lock_data::FellowshipLockData;
use crate::network::structure::hash_comparer::{self, HashComparer};
use crate::physics::object_maint;
use crate::sessions::SessionData;
use crate::world_objects::world_object_networking::shims::current_motion_state;
use crate::world_objects::{player_character, player_fellowship, player_location, player_xp};
use crate::World;

/// ACE `Fellowship.MaxFellows`.
/// The maximum # of fellowship members (a `public static int` nothing reassigns).
pub const MAX_FELLOWS: usize = 9;

/// ACE `Fellowship.MaxDistance`.
pub const MAX_DISTANCE: i32 = 600;

// ACE: Fellowship
/// The state of one fellowship (see the module documentation for how members share it).
#[derive(Debug)]
pub struct Fellowship {
    pub fellowship_name: String,
    pub fellowship_leader_guid: u32,

    /// determined by the leader's 'ShareFellowshipExpAndLuminance' client option when fellowship is created
    pub desired_share_xp: bool,
    /// determined by the leader's 'ShareFellowshipLoot' client option when fellowship is created
    pub share_loot: bool,

    /// whether or not XP sharing is currently enabled, as determined by DesiredShareXP && level restrictions
    pub share_xp: bool,
    /// true if all fellows are >= level 50, or all fellows are within 5 levels of the leader
    pub even_share: bool,

    /// indicates if non-leaders can invite new fellowship members
    pub open: bool,
    /// only set through emotes. if a fellowship is locked, new fellowship members cannot be added
    pub is_locked: bool,

    /// `Dictionary<uint, WeakReference<Player>>`: the guid of each member, in dictionary order.
    pub fellowship_members: DotNetDict<u32, ObjectGuid>,

    pub departed_members: DotNetDict<u32, i32>,

    pub fellowship_locks: DotNetDict<String, FellowshipLockData>,

    pub quest_manager: QuestManager,
}

/// `Fellowship`, the reference: an id into `World.fellowships`. Copying it copies the
/// reference; equality is C#'s reference equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FellowshipRef(u32);

impl FellowshipRef {
    /// The fellowship's fields.
    ///
    /// # Panics
    /// When the fellowship has left the store (no player referred to it any more).
    #[must_use]
    pub fn get(self, w: &World) -> &Fellowship {
        &w.fellowships
            .entries
            .get(&self.0)
            .expect("a live fellowship")
            .fellowship
    }

    /// The fellowship's fields, to change.
    ///
    /// # Panics
    /// When the fellowship has left the store.
    pub fn get_mut(self, w: &mut World) -> &mut Fellowship {
        &mut w
            .fellowships
            .entries
            .get_mut(&self.0)
            .expect("a live fellowship")
            .fellowship
    }

    /// `FellowshipLeaderGuid`.
    #[must_use]
    pub fn leader_guid(self, w: &World) -> u32 {
        self.get(w).fellowship_leader_guid
    }

    /// `fellowship.QuestManager`, lent to `f` with the world (pass it on as
    /// `QuestOwner::Fellowship(qm)`). It is taken out of the store for the call and put back
    /// after: the quest calls never reach back into the fellowship.
    pub fn with_quest_manager<R>(
        self,
        w: &mut World,
        f: impl FnOnce(&mut World, &mut QuestManager) -> R,
    ) -> R {
        let mut qm = std::mem::take(&mut self.get_mut(w).quest_manager);
        let result = f(w, &mut qm);
        self.get_mut(w).quest_manager = qm;
        result
    }

    /// `fellowship.QuestManager` for a read (the read-only quest calls take a `QuestOwner` too):
    /// `f` gets a copy.
    pub fn read_quest_manager<R>(self, w: &World, f: impl FnOnce(&mut QuestManager) -> R) -> R {
        let mut qm = self.get(w).quest_manager.clone();
        f(&mut qm)
    }
}

/// One fellowship in the store, with the number of players whose `Player.Fellowship` names it.
#[derive(Debug)]
struct FellowshipEntry {
    fellowship: Fellowship,
    references: u32,
}

/// `World.fellowships`: every fellowship, by id. Players refer to one through
/// `Player.Fellowship` ([`crate::world_objects::player_fellowship::set_fellowship`] counts the
/// references); one nobody refers to is dropped at the next [`new`].
#[derive(Debug, Default)]
pub struct FellowshipStore {
    entries: std::collections::HashMap<u32, FellowshipEntry>,
    next_id: u32,
}

impl FellowshipStore {
    /// The number of fellowships in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// One more player refers to `f` (`Player.Fellowship = f`).
    pub fn add_reference(&mut self, f: FellowshipRef) {
        if let Some(e) = self.entries.get_mut(&f.0) {
            e.references += 1;
        }
    }

    /// One player fewer refers to `f`.
    pub fn remove_reference(&mut self, f: FellowshipRef) {
        if let Some(e) = self.entries.get_mut(&f.0) {
            e.references = e.references.saturating_sub(1);
        }
    }

    /// Drops the fellowships no player refers to (what ACE's garbage collector frees).
    fn collect(&mut self) {
        self.entries.retain(|_, e| e.references > 0);
    }

    fn insert(&mut self, fellowship: Fellowship) -> FellowshipRef {
        self.collect();
        self.next_id = self.next_id.wrapping_add(1);
        self.entries.insert(
            self.next_id,
            FellowshipEntry {
                fellowship,
                references: 0,
            },
        );
        FellowshipRef(self.next_id)
    }
}

// ================================================================================ helpers

fn name(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

/// `player.Session` (every member `GetFellowshipMembers` returns has one; anyone else without one
/// is ACE's `NullReferenceException`).
fn session_of(w: &World, player: ObjectGuid) -> SessionId {
    player_manager::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `player.Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    let session = session_of(w, player);
    enqueue_send(w, session, msg);
}

/// Builds a game event on the player's session (its `GameEventSequence` is consumed here) and
/// sends it.
fn send_event(
    w: &mut World,
    player: ObjectGuid,
    build: impl FnOnce(&mut SessionData) -> GameMessage,
) {
    let session = session_of(w, player);
    let data = w
        .sessions
        .get_mut(session)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let msg = build(data);
    enqueue_send(w, session, msg);
}

/// `new GameMessageSystemChat(message, type)`, sent to `player`.
fn chat(w: &mut World, player: ObjectGuid, message: &str, chat_message_type: ChatMessageType) {
    send(
        w,
        player,
        game_message_system_chat(message, chat_message_type),
    );
}

/// `new GameEventFellowshipFullUpdate(player.Session)`, sent.
fn send_full_update(w: &mut World, player: ObjectGuid) {
    let session = session_of(w, player);
    let msg = game_event_fellowship_full_update(w, session);
    enqueue_send(w, session, msg);
}

/// `player.Level`.
fn level(w: &World, player: ObjectGuid) -> Option<i32> {
    w.objects.get(player).and_then(|o| o.level())
}

/// `player.Location` (a player without one is ACE's `NullReferenceException`).
fn location(w: &World, player: ObjectGuid) -> Position {
    w.objects
        .get(player)
        .and_then(|o| o.location())
        .expect("ACE: Player.Location is null (NullReferenceException)")
}

// ================================================================================ Fellowship.cs

// ACE: Fellowship.Fellowship
/// Called when a player first creates a Fellowship
#[must_use]
pub fn new(
    w: &mut World,
    leader: ObjectGuid,
    fellowship_name: &str,
    share_xp: bool,
) -> FellowshipRef {
    let desired_share_xp = share_xp;

    // get loot sharing from leader's character options
    let share_loot =
        player_character::get_character_option(w, leader, CharacterOption::ShareFellowshipLoot);

    let mut fellowship_members = DotNetDict::new();
    fellowship_members.add(leader.full(), leader);

    w.fellowships.insert(Fellowship {
        fellowship_name: fellowship_name.to_owned(),
        fellowship_leader_guid: leader.full(),
        desired_share_xp,
        share_loot,
        share_xp,
        even_share: false,
        open: false,
        is_locked: false,
        fellowship_members,
        departed_members: DotNetDict::new(),
        fellowship_locks: DotNetDict::new(),
        quest_manager: QuestManager::new_fellowship(fellowship_name),
    })
}

// ACE: Fellowship.AddFellowshipMember
/// Called when a player clicks the 'add fellow' button
pub fn add_fellowship_member(
    w: &mut World,
    this: &FellowshipRef,
    inviter: Option<ObjectGuid>,
    new_member: Option<ObjectGuid>,
) {
    let (Some(inviter), Some(new_member)) = (inviter, new_member) else {
        return;
    };

    let (is_locked, time_departed, count, contains) = {
        let f = this.get_mut(w);
        (
            f.is_locked,
            f.departed_members.get(&new_member.full()).copied(),
            f.fellowship_members.len(),
            f.fellowship_members.contains_key(&new_member.full()),
        )
    };

    if is_locked {
        let Some(time_departed) = time_departed else {
            let new_member_name = name(w, new_member);
            send_event(w, inviter, |s| {
                game_event_weenie_error_with_string(
                    s,
                    WeenieErrorWithString::LockedFellowshipCannotRecruit_,
                    &new_member_name,
                )
            });
            //newMember.SendWeenieError(WeenieError.LockedFellowshipCannotRecruitYou);
            return;
        };

        // Not ACE's (retail, V274): a departed member may be recruited back for 900 s,
        // the "15 minutes" of the lock message (inferred from retail's own text: 470 captured lock
        // messages say it word for word; no captured rejoin shows the timing). ACE allowed 600 s.
        let time_limit =
            Time::get_date_time_from_timestamp(f64::from(time_departed)).add_seconds(900.0);
        if w.now.utc > time_limit {
            let new_member_name = name(w, new_member);
            send_event(w, inviter, |s| {
                game_event_weenie_error_with_string(
                    s,
                    WeenieErrorWithString::LockedFellowshipCannotRecruit_,
                    &new_member_name,
                )
            });
            //newMember.SendWeenieError(WeenieError.LockedFellowshipCannotRecruitYou);
            return;
        }
    }

    if count >= MAX_FELLOWS {
        send_event(w, inviter, |s| {
            game_event_weenie_error(s, WeenieError::YourFellowshipIsFull)
        });
        return;
    }

    if player_fellowship::fellowship(w, new_member).is_some() || contains {
        let text = format!(
            "{} is already a member of a Fellowship.",
            name(w, new_member)
        );
        chat(w, inviter, &text, ChatMessageType::Broadcast);
    } else {
        let is_busy = w
            .objects
            .get(new_member)
            .is_some_and(|o| o.wo.world_object.is_busy);
        if property_manager::get_bool(w, "fellow_busy_no_recruit", false, true).item && is_busy {
            let text = format!("{} is busy.", name(w, new_member));
            chat(w, inviter, &text, ChatMessageType::Broadcast);
            return;
        }

        if player_character::get_character_option(
            w,
            new_member,
            CharacterOption::AutomaticallyAcceptFellowshipRequests,
        ) {
            add_confirmed_member(w, this, Some(inviter), Some(new_member), true);
        } else {
            let inviter_name = name(w, inviter);
            let confirmation =
                crate::entity::confirmation::Confirmation::fellowship(inviter, new_member);
            if !crate::world_objects::managers::confirmation_manager::enqueue_send(
                w,
                new_member,
                confirmation,
                &inviter_name,
            ) {
                let text = format!("{} is busy.", name(w, new_member));
                chat(w, inviter, &text, ChatMessageType::Broadcast);
            }
        }
    }
}

// ACE: Fellowship.AddConfirmedMember
/// Finalizes the process of adding a player to the fellowship
/// If the player doesn't have the 'automatically accept fellowship requests' option set,
/// this would be after they responded to the popup window
pub fn add_confirmed_member(
    w: &mut World,
    this: &FellowshipRef,
    inviter: Option<ObjectGuid>,
    player: Option<ObjectGuid>,
    response: bool,
) {
    let (Some(inviter), Some(player)) = (inviter, player) else {
        return;
    };
    if w.objects.get(inviter).is_none()
        || player_manager::player_session(w, inviter).is_none()
        || w.objects.get(player).is_none()
    {
        return;
    }

    if !response {
        // player clicked 'no' on the fellowship popup
        let text = format!("{} declines your invite", name(w, player));
        chat(w, inviter, &text, ChatMessageType::Fellowship);
        send_event(w, inviter, |s| {
            game_event_weenie_error(s, WeenieError::FellowshipDeclined)
        });
        return;
    }

    if this.get_mut(w).fellowship_members.len() >= MAX_FELLOWS {
        send_event(w, inviter, |s| {
            game_event_weenie_error(s, WeenieError::YourFellowshipIsFull)
        });
        return;
    }

    this.get_mut(w)
        .fellowship_members
        .try_add(player.full(), player);
    let inviter_fellowship = player_fellowship::fellowship(w, inviter);
    player_fellowship::set_fellowship(w, player, inviter_fellowship);

    calculate_xp_sharing(w, this);

    let fellowship_members = get_fellowship_members(w, this);

    let share_loot = this.get_mut(w).share_loot;

    for &member in fellowship_members.values().filter(|&&m| m != player) {
        // Not ACE's (a fix, V258): ACE passed ShareXP as the update's shareLoot
        // argument; OnVitalUpdate passes ShareLoot.
        let session = session_of(w, member);
        let msg = game_event_fellowship_update_fellow(
            w,
            session,
            player,
            share_loot,
            FellowUpdateType::Full,
        );
        enqueue_send(w, session, msg);
    }

    if share_loot {
        let player_name = name(w, player);
        for &member in fellowship_members.values().filter(|&&m| m != player) {
            let member_name = name(w, member);
            chat(
                w,
                member,
                &format!("{player_name} has given you permission to loot his or her kills."),
                ChatMessageType::Broadcast,
            );
            chat(
                w,
                member,
                &format!("{player_name} may now loot your kills."),
                ChatMessageType::Broadcast,
            );

            chat(
                w,
                player,
                &format!("{member_name} has given you permission to loot his or her kills."),
                ChatMessageType::Broadcast,
            );
            chat(
                w,
                player,
                &format!("{member_name} may now loot your kills."),
                ChatMessageType::Broadcast,
            );
        }
    }

    update_all_members(w, this);

    let stance = w
        .objects
        .get(inviter)
        .and_then(current_motion_state)
        .map(|m| m.stance)
        .expect("ACE: CurrentMotionState is null (NullReferenceException)");
    if stance == MotionStance::NonCombat {
        // only do this motion if inviter is at peace, other times motion is skipped.
        player_location::send_motion_as_commands(
            w,
            inviter,
            MotionCommand::BowDeep,
            MotionStance::NonCombat,
        );
    }
}

// ACE: Fellowship.RemoveFellowshipMember
pub fn remove_fellowship_member(
    w: &mut World,
    this: &FellowshipRef,
    player: Option<ObjectGuid>,
    leader: ObjectGuid,
) {
    let Some(player) = player else { return };

    let fellowship_members = get_fellowship_members(w, this);

    if !fellowship_members.contains_key(&player.full()) {
        let (leader_name, player_name) = (name(w, leader), name(w, player));
        log::warn!("{leader_name} tried to dismiss {player_name} from the fellowship, but {player_name} was not found in the fellowship");

        let mut done = true;

        if let Some(player_fellowship) = player_fellowship::fellowship(w, player) {
            if player_fellowship == *this {
                log::warn!("{player_name} still has a reference to this fellowship somehow. This shouldn't happen");
                done = false;
            } else {
                log::warn!("{player_name} has a reference to a different fellowship. {leader_name} is possibly sending crafted data!");
            }
        }

        if done {
            return;
        }
    }

    let player_name = name(w, player);
    for &member in fellowship_members.values() {
        send_event(w, member, |s| game_event_fellowship_dismiss(s, player));
        chat(
            w,
            member,
            &format!("{player_name} dismissed from fellowship"),
            ChatMessageType::Fellowship,
        );
    }

    this.get_mut(w).fellowship_members.remove(&player.full());
    player_fellowship::set_fellowship(w, player, None);

    calculate_xp_sharing(w, this);

    update_all_members(w, this);
}

// ACE: Fellowship.UpdateAllMembers
fn update_all_members(w: &mut World, this: &FellowshipRef) {
    let fellowship_members = get_fellowship_members(w, this);

    for &member in fellowship_members.values() {
        send_full_update(w, member);
    }
}

// ACE: Fellowship.SendMessageAndUpdate
/// No caller in ACE.
pub fn send_message_and_update(w: &mut World, this: &FellowshipRef, message: &str) {
    let fellowship_members = get_fellowship_members(w, this);

    for &member in fellowship_members.values() {
        chat(w, member, message, ChatMessageType::Fellowship);

        send_full_update(w, member);
    }
}

// ACE: Fellowship.SendBroadcastAndUpdate
fn send_broadcast_and_update(w: &mut World, this: &FellowshipRef, message: &str) {
    let fellowship_members = get_fellowship_members(w, this);

    for &member in fellowship_members.values() {
        send_event(w, member, |s| {
            game_event_channel_broadcast(s, Channel::FellowBroadcast, "", message)
        });

        send_full_update(w, member);
    }
}

// ACE: Fellowship.BroadcastToFellow
pub fn broadcast_to_fellow(w: &mut World, this: &FellowshipRef, message: &str) {
    let fellowship_members = get_fellowship_members(w, this);

    for &member in fellowship_members.values() {
        send_event(w, member, |s| {
            game_event_channel_broadcast(s, Channel::FellowBroadcast, "", message)
        });
    }
}

// ACE: Fellowship.TellFellow
pub fn tell_fellow(w: &mut World, this: &FellowshipRef, sender: ObjectGuid, message: &str) {
    let fellowship_members = get_fellowship_members(w, this);

    let sender_name = name(w, sender);
    for &member in fellowship_members.values() {
        send_event(w, member, |s| {
            game_event_channel_broadcast(s, Channel::Fellow, &sender_name, message)
        });
    }
}

// ACE: Fellowship.SendWeenieErrorWithStringAndUpdate
fn send_weenie_error_with_string_and_update(
    w: &mut World,
    this: &FellowshipRef,
    error: WeenieErrorWithString,
    message: &str,
) {
    let fellowship_members = get_fellowship_members(w, this);

    for &member in fellowship_members.values() {
        send_event(w, member, |s| {
            game_event_weenie_error_with_string(s, error, message)
        });

        send_full_update(w, member);
    }
}

/// `var timestamp = (int)Time.GetUnixTime(); if (!DepartedMembers.TryAdd(guid, timestamp))
/// DepartedMembers[guid] = timestamp;`
fn record_departure(w: &mut World, this: &FellowshipRef, player: ObjectGuid) {
    let timestamp: i32 = w.now.unix_time.cs_cast();
    let f = this.get_mut(w);
    if !f.departed_members.try_add(player.full(), timestamp) {
        f.departed_members.insert(player.full(), timestamp);
    }
}

// ACE: Fellowship.QuitFellowship
pub fn quit_fellowship(
    w: &mut World,
    this: &FellowshipRef,
    player: Option<ObjectGuid>,
    disband: bool,
) {
    let Some(player) = player else { return };

    if player.full() == this.leader_guid(w) {
        if disband {
            let fellowship_members = get_fellowship_members(w, this);
            let share_loot = this.get_mut(w).share_loot;

            for &member in fellowship_members.values() {
                send_event(w, member, game_event_fellowship_disband);

                if share_loot {
                    chat(
                        w,
                        member,
                        "You no longer have permission to loot anyone else's kills.",
                        ChatMessageType::Broadcast,
                    );

                    // you would expect this occur, but it did not in retail pcaps
                    //foreach (var fellow in fellowshipMembers.Values)
                    //    member.Session.Network.EnqueueSend(new GameMessageSystemChat($"{fellow.Name} does not have permission to loot your kills.", ChatMessageType.Broadcast));
                }

                player_fellowship::set_fellowship(w, member, None);
            }
        } else {
            this.get_mut(w).fellowship_members.remove(&player.full());

            if this.get_mut(w).is_locked {
                record_departure(w, this, player);
            }

            player_fellowship::set_fellowship(w, player, None);

            send_event(w, player, |s| game_event_fellowship_quit(s, player.full()));
            chat(
                w,
                player,
                "You no longer have permission to loot anyone else's kills.",
                ChatMessageType::Broadcast,
            );

            let fellowship_members = get_fellowship_members(w, this);
            let share_loot = this.get_mut(w).share_loot;
            let player_name = name(w, player);

            for &member in fellowship_members.values() {
                send_event(w, member, |s| game_event_fellowship_quit(s, player.full()));

                if share_loot {
                    chat(
                        w,
                        member,
                        &format!("You have lost permission to loot the kills of {player_name}."),
                        ChatMessageType::Broadcast,
                    );
                    let member_name = name(w, member);
                    chat(
                        w,
                        player,
                        &format!("{member_name} does not have permission to loot your kills."),
                        ChatMessageType::Broadcast,
                    );
                }
            }
            assign_new_leader(w, this, None, None);

            calculate_xp_sharing(w, this);
        }
    } else if !disband {
        this.get_mut(w).fellowship_members.remove(&player.full());

        if this.get_mut(w).is_locked {
            record_departure(w, this, player);
        }

        send_event(w, player, |s| game_event_fellowship_quit(s, player.full()));

        let fellowship_members = get_fellowship_members(w, this);
        let share_loot = this.get_mut(w).share_loot;
        let player_name = name(w, player);

        for &member in fellowship_members.values() {
            send_event(w, member, |s| game_event_fellowship_quit(s, player.full()));

            if share_loot {
                chat(
                    w,
                    member,
                    &format!("You have lost permission to loot the kills of {player_name}."),
                    ChatMessageType::Broadcast,
                );
                let member_name = name(w, member);
                chat(
                    w,
                    player,
                    &format!("{member_name} does not have permission to loot your kills."),
                    ChatMessageType::Broadcast,
                );
            }
        }

        player_fellowship::set_fellowship(w, player, None);

        calculate_xp_sharing(w, this);
    }
}

// ACE: Fellowship.AssignNewLeader
pub fn assign_new_leader(
    w: &mut World,
    this: &FellowshipRef,
    old_leader: Option<ObjectGuid>,
    new_leader: Option<ObjectGuid>,
) {
    if let Some(new_leader) = new_leader {
        this.get_mut(w).fellowship_leader_guid = new_leader.full();

        let new_leader_name = name(w, new_leader);
        if let Some(old_leader) = old_leader {
            send_event(w, old_leader, |s| {
                game_event_weenie_error_with_string(
                    s,
                    WeenieErrorWithString::YouHavePassedFellowshipLeadershipTo_,
                    &new_leader_name,
                )
            });
        }

        send_weenie_error_with_string_and_update(
            w,
            this,
            WeenieErrorWithString::_IsNowLeaderOfFellowship,
            &new_leader_name,
        );
    } else {
        // leader has dropped, assign new random leader
        let fellowship_members = get_fellowship_members(w, this);

        if fellowship_members.is_empty() {
            return;
        }

        let count = i32::try_from(fellowship_members.len()).expect("a dictionary count is an int");
        let rng = ThreadSafeRandom::next(0, count - 1);

        let fellow_guids: Vec<u32> = fellowship_members.keys().copied().collect();

        let leader_guid =
            fellow_guids[usize::try_from(rng).expect("System.ArgumentOutOfRangeException")];
        this.get_mut(w).fellowship_leader_guid = leader_guid;

        let new_leader_name = name(
            w,
            fellowship_members
                .get(&leader_guid)
                .copied()
                .expect("KeyNotFoundException"),
        );

        if let Some(old_leader) = old_leader {
            send_event(w, old_leader, |s| {
                game_event_weenie_error_with_string(
                    s,
                    WeenieErrorWithString::YouHavePassedFellowshipLeadershipTo_,
                    &new_leader_name,
                )
            });
        }

        send_weenie_error_with_string_and_update(
            w,
            this,
            WeenieErrorWithString::_IsNowLeaderOfFellowship,
            &new_leader_name,
        );
    }
}

// ACE: Fellowship.UpdateOpenness
pub fn update_openness(w: &mut World, this: &FellowshipRef, is_open: bool) {
    let fellowship_name = {
        let f = this.get_mut(w);
        f.open = is_open;
        f.fellowship_name.clone()
    };
    let openness = if is_open {
        WeenieErrorWithString::_IsNowOpenFellowship
    } else {
        WeenieErrorWithString::_IsNowClosedFellowship
    };
    send_weenie_error_with_string_and_update(w, this, openness, &fellowship_name);
}

// ACE: Fellowship.UpdateLock
pub fn update_lock(w: &mut World, this: &FellowshipRef, is_locked: bool, lock_name: Option<&str>) {
    // Unlocking a fellowship is not possible without disbanding in retail worlds, so in all likelihood, this is only firing for fellowships being locked by emotemanager

    this.get_mut(w).is_locked = is_locked;

    // `string.IsNullOrWhiteSpace(lockName)`
    let lock_name = match lock_name {
        Some(s) if !s.chars().all(char::is_whitespace) => s.to_owned(),
        _ => "Undefined".to_owned(),
    };

    if is_locked {
        let timestamp = w.now.unix_time;
        {
            let f = this.get_mut(w);
            f.open = false;

            f.departed_members.clear();

            if !f
                .fellowship_locks
                .try_add(lock_name.clone(), FellowshipLockData::new(timestamp))
            {
                f.fellowship_locks
                    .get_mut(&lock_name)
                    .expect("KeyNotFoundException")
                    .update_timestamp(timestamp);
            }
        }

        send_broadcast_and_update(
            w,
            this,
            "Your fellowship is now locked.  You may not recruit new members.  If you leave the fellowship, you have 15 minutes to be recruited back into the fellowship.",
        );
    } else {
        // Unlocking a fellowship is not possible without disbanding in retail worlds, so in all likelihood, this never occurs

        {
            let f = this.get_mut(w);
            f.departed_members.clear();

            f.fellowship_locks.remove(&lock_name);
        }

        send_broadcast_and_update(w, this, "Your fellowship is now unlocked.");
    }
}

// ACE: Fellowship.CalculateXPSharing
/// Calculates fellowship XP sharing (ShareXP, EvenShare) from fellow levels
pub fn calculate_xp_sharing(w: &mut World, this: &FellowshipRef) {
    // - If all members of the fellowship are level 50 or above, all members will share XP equally

    // - If all members of the fellowship are within 5 levels of the founder, XP will be shared equally

    // - If members are all within ten levels of the founder, XP will be shared proportionally.

    let fellows = get_fellowship_members(w, this);

    let all_even_share_level =
        property_manager::get_long(w, "fellowship_even_share_level", 0, true).item;
    let all_over_even_share_level = !fellows
        .values()
        .any(|&f| i64::from(level(w, f).unwrap_or(1)) < all_even_share_level);

    if all_over_even_share_level {
        let f = this.get_mut(w);
        f.share_xp = f.desired_share_xp;
        f.even_share = true;
        return;
    }

    let Some(leader) = player_manager::get_online_player(w, this.leader_guid(w)) else {
        return;
    };

    let leader_level = level(w, leader).unwrap_or(1);
    let max_level_diff = fellows
        .values()
        .map(|&f| {
            leader_level
                .wrapping_sub(level(w, f).unwrap_or(1))
                .checked_abs()
                .expect("System.OverflowException")
        })
        .max()
        .expect("System.InvalidOperationException: Sequence contains no elements");

    let f = this.get_mut(w);
    if max_level_diff <= 5 {
        f.share_xp = f.desired_share_xp;
        f.even_share = true;
    } else if max_level_diff <= 10 {
        f.share_xp = f.desired_share_xp;
        f.even_share = false;
    } else {
        f.share_xp = false;
        f.even_share = false;
    }
}

/// `shareType &= ~ShareType.Fellowship;`
fn without_fellowship(share_type: ShareType) -> ShareType {
    ShareType(share_type.0 & !ShareType::Fellowship.0)
}

// ACE: Fellowship.SplitXp
/// Splits XP amongst fellowship members, depending on XP type and fellow settings.
/// `amount`: the input amount of XP; `xp_type`: the type of XP (quest XP is handled differently);
/// `player`: the fellowship member who originated the XP.
pub fn split_xp(
    w: &mut World,
    this: &FellowshipRef,
    amount: u64,
    xp_type: XpType,
    share_type: ShareType,
    player: ObjectGuid,
) {
    // https://asheron.fandom.com/wiki/Announcements_-_2002/02_-_Fever_Dreams#Letter_to_the_Players_1

    let fellowship_members = get_fellowship_members(w, this);

    let share_type = without_fellowship(share_type);

    // quest turn-ins: flat share (retail default)
    if xp_type == XpType::Quest
        && !property_manager::get_bool(w, "fellow_quest_bonus", false, true).item
    {
        let signed_amount: i64 = amount.cs_cast();
        let count = i64::try_from(fellowship_members.len()).expect("a dictionary count is an int");
        let per_amount = signed_amount
            .checked_div(count)
            .expect("System.DivideByZeroException");

        for &member in fellowship_members.values() {
            let fellow_xp_type = if player == member {
                XpType::Quest
            } else {
                XpType::Fellowship
            };

            player_xp::grant_xp(w, member, per_amount, fellow_xp_type, share_type);
        }
    }
    // divides XP evenly to all the sharable fellows within level range,
    // but with a significant boost to the amount of xp, based on # of fellowship members
    else if this.get_mut(w).even_share {
        let amount_d: f64 = amount.cs_cast();
        let total_amount: u64 = math::round(amount_d * get_member_share_percent(w, this)).cs_cast();

        for &member in fellowship_members.values() {
            let total_d: f64 = total_amount.cs_cast();
            let share_amount: u64 =
                math::round(total_d * get_distance_scalar(w, Some(player), Some(member), xp_type))
                    .cs_cast();

            let fellow_xp_type = if player == member {
                xp_type
            } else {
                XpType::Fellowship
            };

            player_xp::grant_xp(
                w,
                member,
                share_amount.cs_cast(),
                fellow_xp_type,
                share_type,
            );
        }
    }
    // divides XP to all sharable fellows within level range
    // based on each fellowship member's level
    else {
        let level_value = |w: &World, p: ObjectGuid| {
            level(w, p)
                .expect("System.InvalidOperationException: Nullable object must have a value.")
        };

        // ACE.Entity's LINQExtensions.Sum(IEnumerable<ulong>): an unchecked ulong sum
        let level_xp_sum = fellowship_members
            .values()
            .map(|&p| player_xp::get_xp_to_next_level(w, level_value(w, p)))
            .fold(0u64, u64::wrapping_add);

        for &member in fellowship_members.values() {
            let to_next: f64 = player_xp::get_xp_to_next_level(w, level_value(w, member)).cs_cast();
            let sum: f64 = level_xp_sum.cs_cast();
            let level_xp_scale = to_next / sum;

            let amount_d: f64 = amount.cs_cast();
            let player_total: u64 = math::round(
                amount_d
                    * level_xp_scale
                    * get_distance_scalar(w, Some(player), Some(member), xp_type),
            )
            .cs_cast();

            let fellow_xp_type = if player == member {
                xp_type
            } else {
                XpType::Fellowship
            };

            player_xp::grant_xp(
                w,
                member,
                player_total.cs_cast(),
                fellow_xp_type,
                share_type,
            );
        }
    }
}

// ACE: Fellowship.SplitLuminance
/// Splits luminance amongst fellowship members, depending on XP type and fellow settings
pub fn split_luminance(
    w: &mut World,
    this: &FellowshipRef,
    amount: u64,
    xp_type: XpType,
    share_type: ShareType,
    player: ObjectGuid,
) {
    // https://asheron.fandom.com/wiki/Announcements_-_2002/02_-_Fever_Dreams#Letter_to_the_Players_1

    let share_type = without_fellowship(share_type);

    if xp_type == XpType::Quest {
        // quest luminance is not shared
        player_grant_luminance(w, player, amount.cs_cast(), XpType::Quest, share_type);
    } else {
        // pre-filter: evenly divide between luminance-eligible fellows
        // updated: retail supposedly did not do this
        //var shareableMembers = GetFellowshipMembers().Values.Where(f => f.MaximumLuminance != null).ToList();

        let shareable_members: Vec<ObjectGuid> =
            get_fellowship_members(w, this).values().copied().collect();

        if shareable_members.is_empty() {
            return;
        }

        let count = u64::try_from(shareable_members.len()).expect("a list count is an int");
        let per_amount_d: f64 = (amount / count).cs_cast();
        let per_amount: i64 = math::round(per_amount_d).cs_cast();

        // further filter to fellows in radar range
        let within = within_range(w, this, player, true);
        let mut in_range: Vec<ObjectGuid> = Vec::new();
        for m in shareable_members {
            // LINQ Intersect: the first sequence's order, distinct
            if within.contains(&m) && !in_range.contains(&m) {
                in_range.push(m);
            }
        }

        for member in in_range {
            if w.objects
                .get(member)
                .and_then(|o| o.maximum_luminance())
                .is_none()
            {
                continue;
            }

            let fellow_xp_type = if player == member {
                xp_type
            } else {
                XpType::Fellowship
            };

            player_grant_luminance(w, member, per_amount, fellow_xp_type, share_type);
        }
    }
}

// ACE: Fellowship.GetMemberSharePercent
#[must_use]
///
/// **Retail's table, not ACE's (V226).** ACE paid nine members
/// 0.30 each where the client's even-split table (which its fellowship panel displays) has
/// 0.3111111: the fellowship's total stays at 2.8 from seven to ten members. The shares are the
/// table's single-precision values, widened, as the client computes them. ACE's fall-through to
/// 1.0 for counts outside 1–9 is unreachable (a fellowship holds at most nine).
pub fn get_member_share_percent(w: &mut World, this: &FellowshipRef) -> f64 {
    let fellowship_members = get_fellowship_members(w, this);
    f64::from(dereth_rules::fellowship::even_split_xp_percentage(
        fellowship_members.len(),
    ))
}

// ACE: Fellowship.GetDistanceScalar
/// Returns the amount to scale the XP for a fellow based on distance from the earner. ACE's
/// arithmetic is `float` (`Distance2D`, the `f` literals), widened to `double` on return.
#[must_use]
pub fn get_distance_scalar(
    w: &World,
    earner: Option<ObjectGuid>,
    fellow: Option<ObjectGuid>,
    xp_type: XpType,
) -> f64 {
    let (Some(earner), Some(fellow)) = (earner, fellow) else {
        return 0.0;
    };

    if xp_type == XpType::Quest {
        return 1.0;
    }

    distance_scalar(&location(w, earner), &location(w, fellow))
}

/// The positional part of [`get_distance_scalar`] (after its null and quest checks).
#[must_use]
pub fn distance_scalar(earner: &Position, fellow: &Position) -> f64 {
    // https://asheron.fandom.com/wiki/Announcements_-_2004/01_-_Mirror,_Mirror#Rollout_Article

    // If they are indoors while you are outdoors, or vice-versa.
    if earner.indoors() != fellow.indoors() {
        return 0.0;
    }

    // If you are both indoors but in different landblocks.
    if earner.indoors() && fellow.indoors() && earner.landblock() != fellow.landblock() {
        return 0.0;
    }

    let dist = earner.distance_2d(fellow);

    #[allow(clippy::cast_precision_loss)] // `MaxDistance * 2.0f`: int to float, exact
    let max_distance = MAX_DISTANCE as f32;

    if dist >= max_distance * 2.0 {
        return 0.0;
    }

    if dist <= max_distance {
        return 1.0;
    }

    let scalar = 1.0f32 - (dist - max_distance) / max_distance;

    f64::from(math::max_f32(0.0, scalar))
}

/// `Player.MaxRadarRange_Indoors` / `MaxRadarRange_Outdoors` (`Player.cs` constants).
const MAX_RADAR_RANGE_INDOORS: f32 = 25.0;
const MAX_RADAR_RANGE_OUTDOORS: f32 = 75.0;

/// `Player.CurrentRadarRange` (`Player.cs`): `Location.Indoors ? 25 : 75`.
fn current_radar_range(w: &World, player: ObjectGuid) -> f32 {
    if location(w, player).indoors() {
        MAX_RADAR_RANGE_INDOORS
    } else {
        MAX_RADAR_RANGE_OUTDOORS
    }
}

// ACE: Fellowship.WithinRange
/// Returns fellows within radar range (75 units outdoors, 25 units indoors)
pub fn within_range(
    w: &mut World,
    this: &FellowshipRef,
    player: ObjectGuid,
    include_self: bool,
) -> Vec<ObjectGuid> {
    let fellows = get_fellowship_members(w, this);

    let landblock_range = property_manager::get_bool(w, "fellow_kt_landblock", false, true).item;

    let mut results = Vec::new();

    for &fellow in fellows.values() {
        if player == fellow && !include_self {
            continue;
        }

        let shareable = if player == fellow || landblock_range {
            let player_lb = w
                .objects
                .get(player)
                .expect("ACE: player is null (NullReferenceException)")
                .current_landblock;
            let fellow_lb = w
                .objects
                .get(fellow)
                .expect("ACE: fellow is null (NullReferenceException)")
                .current_landblock;
            player_lb == fellow_lb || location(w, player).distance_to(&location(w, fellow)) <= 192.0
        } else {
            // 2d visible distance / radar range?
            location(w, player).distance_2d(&location(w, fellow)) <= current_radar_range(w, player)
                && {
                    let phys = w
                        .objects
                        .get(player)
                        .and_then(|o| o.phys)
                        .expect("ACE: Player.PhysicsObj is null (NullReferenceException)");
                    object_maint::visible_objects_contains_key(w, phys, fellow.full())
                }
        };

        if shareable {
            results.push(fellow);
        }
    }
    results
}

// ACE: Fellowship.OnFellowLevelUp
/// Called when someone in the fellowship levels up
pub fn on_fellow_level_up(w: &mut World, this: &FellowshipRef, player: ObjectGuid) {
    calculate_xp_sharing(w, this);

    let fellowship_members = get_fellowship_members(w, this);

    let text = format!(
        "{} is now level {}!",
        name(w, player),
        level(w, player).map(|l| l.to_string()).unwrap_or_default()
    );
    for &fellow in fellowship_members.values() {
        if fellow == player {
            continue;
        }

        chat(w, fellow, &text, ChatMessageType::Broadcast);
    }
}

// ACE: Fellowship.OnVitalUpdate
pub fn on_vital_update(w: &mut World, this: &FellowshipRef, player: ObjectGuid) {
    // cap max update interval?

    let fellowship_members = get_fellowship_members(w, this);
    let share_loot = this.get_mut(w).share_loot;

    for &fellow in fellowship_members.values() {
        if player_fellowship::fellowship_panel_open(w, fellow) {
            let session = session_of(w, fellow);
            let msg = game_event_fellowship_update_fellow(
                w,
                session,
                player,
                share_loot,
                FellowUpdateType::Vitals,
            );
            enqueue_send(w, session, msg);
        }
    }
}

// ACE: Fellowship.OnDeath
pub fn on_death(w: &mut World, this: &FellowshipRef, player: ObjectGuid) {
    let fellowship_members = get_fellowship_members(w, this);

    let text = format!("Your fellow {} has died!", name(w, player));
    for &fellow in fellowship_members.values() {
        if fellow != player {
            chat(w, fellow, &text, ChatMessageType::Broadcast);
        }
    }
}

/// `player != null && player.Session != null && player.Session.Player != null && player.Fellowship != null`
/// for a member's weak reference.
fn is_live_member(w: &World, player: ObjectGuid) -> bool {
    w.objects
        .get(player)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
        && player_manager::player_session(w, player).is_some()
        && player_fellowship::fellowship(w, player).is_some()
}

// ACE: Fellowship.GetFellowshipMembers
/// The live members, keyed by guid, in `FellowshipMembers` order; the dropped ones are removed
/// (`ProcessDropList`).
pub fn get_fellowship_members(w: &mut World, this: &FellowshipRef) -> DotNetDict<u32, ObjectGuid> {
    let mut results = DotNetDict::new();
    let mut dropped = DotNetHashSet::new();

    let members: Vec<(u32, ObjectGuid)> = this
        .get_mut(w)
        .fellowship_members
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    for (player_guid, player) in members {
        if is_live_member(w, player) {
            results.add(player_guid, player);
        } else {
            dropped.insert(player_guid);
        }
    }

    // TODO: process dropped list
    if !dropped.is_empty() {
        process_drop_list(w, this, &dropped);
    }

    results
}

/// `GetFellowshipMembers()` for callers that hold only a `&World` (`Player.GetFellowshipTargets`,
/// `Creature.GetManaCost`): the live members in `FellowshipMembers` order.
// DIVERGE: the dropped members are skipped but not removed; ProcessDropList (the warning, a new leader, the full updates) runs at the next mutating GetFellowshipMembers.
#[must_use]
pub fn get_fellowship_members_read_only(w: &World, this: &FellowshipRef) -> Vec<ObjectGuid> {
    let members: Vec<ObjectGuid> = this.get(w).fellowship_members.values().copied().collect();
    members
        .into_iter()
        .filter(|&p| is_live_member(w, p))
        .collect()
}

// ACE: Fellowship.ProcessDropList
/// `fellowshipMembers` is always this fellowship's own `FellowshipMembers` in ACE.
pub fn process_drop_list(w: &mut World, this: &FellowshipRef, fellow_guids: &DotNetHashSet<u32>) {
    for &fellow_guid in fellow_guids.iter() {
        let (offline_player, _) = player_manager::find_by_guid(w, fellow_guid);
        let offline_name = offline_player
            .and_then(|p| i_player::name(w, p))
            .unwrap_or_else(|| "NULL".to_owned());

        log::warn!("Dropped fellow: {offline_name}");
        this.get_mut(w).fellowship_members.remove(&fellow_guid);
    }
    if fellow_guids.contains(&this.leader_guid(w)) {
        assign_new_leader(w, this, None, None);
    }

    calculate_xp_sharing(w, this);
    update_all_members(w, this);
}

// ACE: FellowshipExtensions.Write
/// `writer.Write(Dictionary<uint, int> departedFellows)`: a `HashComparer(32)` table.
pub fn write_departed_fellows(writer: &mut Vec<u8>, departed_fellows: &DotNetDict<u32, i32>) {
    let record = departed_fellows_record(departed_fellows);
    crate::network::game_messages::game_message::write_record(writer, &[], |w| {
        w.packed_hash(&record, |w, k, v| {
            w.u32(*k);
            w.i32(*v);
            Ok(())
        })
    });
}

/// The departed fellows as dereth-protocol's table record: `HashComparer(32)`'s bucket count and
/// order, as [`write_departed_fellows`] writes them.
#[must_use]
pub fn departed_fellows_record(
    departed_fellows: &DotNetDict<u32, i32>,
) -> dereth_protocol::archive::PackedHash<u32, i32> {
    let hash_comparer = HashComparer::new(32);
    let _ = i32::try_from(departed_fellows.len()).expect("a dictionary count is an int");
    let sorted = hash_comparer::sorted(
        departed_fellows.iter().map(|(k, v)| (*k, *v)),
        &hash_comparer,
    );
    dereth_protocol::archive::PackedHash {
        table_size: u32::from(hash_comparer.num_buckets),
        entries: sorted,
    }
}

// ================================================================================ pointers

/// `member.GrantLuminance(amount, xpType, shareType)` (`Player_Luminance.cs`).
fn player_grant_luminance(
    w: &mut World,
    player: ObjectGuid,
    amount: i64,
    xp_type: XpType,
    share_type: ShareType,
) {
    crate::world_objects::player_luminance::grant_luminance(w, player, amount, xp_type, share_type);
}
