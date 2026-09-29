// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Fellowship.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Fellowship.cs`.
//!
//! The player's side of fellowships; the shared `Fellowship` object is
//! [`crate::entity::fellowship::FellowshipRef`]. Other files read and write `Player.Fellowship`
//! through [`fellowship`](fn@fellowship) and [`set_fellowship`].

use empyrean_entity::enums::{ChatMessageType, WeenieError};
use empyrean_entity::ObjectGuid;

use crate::entity::fellowship::{self, FellowshipRef};
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_fellowship_fellow_update_done::game_event_fellowship_fellow_update_done;
use crate::network::game_event::events::game_event_fellowship_full_update::game_event_fellowship_full_update;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::sessions::SessionData;
use crate::world_objects::{player_character, player_networking};
use crate::World;

/// Non-property fields declared in `Player_Fellowship.cs`.
#[derive(Debug, Default)]
pub struct PlayerFellowshipFields {
    /// `public Fellowship Fellowship;`: the fellowship that this player belongs to.
    pub fellowship: Option<FellowshipRef>,

    /// `public bool FellowVitalUpdate;`
    pub fellow_vital_update: bool,

    /// `public bool FellowshipPanelOpen { get; set; }`
    pub fellowship_panel_open: bool,
}

// ================================================================================ helpers

fn fields(w: &World, this: ObjectGuid) -> Option<&PlayerFellowshipFields> {
    w.objects
        .get(this)?
        .player
        .as_ref()
        .map(|p| &p.player_fellowship)
}

fn fields_mut(w: &mut World, this: ObjectGuid) -> Option<&mut PlayerFellowshipFields> {
    w.objects
        .get_mut(this)?
        .player
        .as_mut()
        .map(|p| &mut p.player_fellowship)
}

/// `Player.Fellowship` (the reference; `None` for null or a non-player).
#[must_use]
pub fn fellowship(w: &World, this: ObjectGuid) -> Option<FellowshipRef> {
    fields(w, this)?.fellowship
}

/// `Player.Fellowship = value`, counting the reference in `World.fellowships`.
pub fn set_fellowship(w: &mut World, this: ObjectGuid, value: Option<FellowshipRef>) {
    let Some(f) = fields_mut(w, this) else { return };
    let old = std::mem::replace(&mut f.fellowship, value);
    if let Some(old) = old {
        w.fellowships.remove_reference(old);
    }
    if let Some(new) = value {
        w.fellowships.add_reference(new);
    }
}

/// `Player.FellowshipPanelOpen`.
#[must_use]
pub fn fellowship_panel_open(w: &World, this: ObjectGuid) -> bool {
    fields(w, this).is_some_and(|f| f.fellowship_panel_open)
}

/// `Player.FellowVitalUpdate`.
#[must_use]
pub fn fellow_vital_update(w: &World, this: ObjectGuid) -> bool {
    fields(w, this).is_some_and(|f| f.fellow_vital_update)
}

/// `Player.FellowVitalUpdate = value`.
pub fn set_fellow_vital_update(w: &mut World, this: ObjectGuid, value: bool) {
    if let Some(f) = fields_mut(w, this) {
        f.fellow_vital_update = value;
    }
}

/// `Player.Session` (ACE dereferences it: a player without one is a `NullReferenceException`).
fn session_of(w: &World, this: ObjectGuid) -> empyrean_net::SessionId {
    player_manager::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    let session = session_of(w, this);
    enqueue_send(w, session, msg);
}

/// Builds a game event on the player's session and sends it.
fn send_event(
    w: &mut World,
    this: ObjectGuid,
    build: impl FnOnce(&mut SessionData) -> GameMessage,
) {
    let session = session_of(w, this);
    let data = w
        .sessions
        .get_mut(session)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let msg = build(data);
    enqueue_send(w, session, msg);
}

fn name(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

/// `Player.IsOlthoiPlayer` (`Player_Properties.cs`, the value `SetEphemeralValues` stores).
fn is_olthoi_player(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

// ================================================================================ Player_Fellowship.cs

// ACE: Player.FellowshipCreate
// todo: Figure out if this is the best place to do this, and whether there are concurrency issues associated with it.
pub fn fellowship_create(w: &mut World, this: ObjectGuid, fellowship_name: &str, share_xp: bool) {
    // An Olthoi player cannot create a fellowship
    if is_olthoi_player(w, this) {
        send_event(w, this, |s| {
            game_event_weenie_error(s, WeenieError::OlthoiCannotJoinFellowship)
        });
        return;
    }

    let fellowship = fellowship::new(w, this, fellowship_name, share_xp);
    set_fellowship(w, this, Some(fellowship));
    let session = session_of(w, this);
    let msg = game_event_fellowship_full_update(w, session);
    enqueue_send(w, session, msg);
    send_event(w, this, |s| {
        game_event_fellowship_fellow_update_done(s, WeenieError::None)
    });
}

// ACE: Player.HandleActionFellowshipChangeOpenness
pub fn handle_action_fellowship_change_openness(w: &mut World, this: ObjectGuid, openness: bool) {
    if let Some(fellowship) = fellowship(w, this) {
        if this.full() != fellowship.leader_guid(w) {
            send_event(w, this, |s| {
                game_event_weenie_error(s, WeenieError::YouMustBeLeaderOfFellowship)
            });
            return;
        }

        if fellowship.get(w).is_locked {
            send_event(w, this, |s| {
                game_event_weenie_error(s, WeenieError::FellowshipIsLocked)
            });
        } else {
            fellowship::update_openness(w, &fellowship, openness);
        }
    }
}

// ACE: Player.HandleActionFellowshipChangeLock
pub fn handle_action_fellowship_change_lock(
    w: &mut World,
    this: ObjectGuid,
    lock_state: bool,
    lock_name: Option<&str>,
) {
    if let Some(fellowship) = fellowship(w, this) {
        fellowship::update_lock(w, &fellowship, lock_state, lock_name);
    }
}

// ACE: Player.FellowshipQuit
pub fn fellowship_quit(w: &mut World, this: ObjectGuid, disband: bool) {
    if let Some(fellowship) = fellowship(w, this) {
        fellowship::quit_fellowship(w, &fellowship, Some(this), disband);
    }
}

// ACE: Player.FellowshipDismissPlayer
pub fn fellowship_dismiss_player(w: &mut World, this: ObjectGuid, dismiss_guid: u32) {
    let Some(fellowship) = fellowship(w, this) else {
        return;
    };

    if this.full() != fellowship.leader_guid(w) {
        send_event(w, this, |s| {
            game_event_weenie_error(s, WeenieError::YouMustBeLeaderOfFellowship)
        });
        return;
    }

    if this.full() == dismiss_guid {
        send(
            w,
            this,
            game_message_system_chat(
                "You can't dismiss yourself from the fellowship",
                ChatMessageType::Broadcast,
            ),
        );
        return;
    }

    let Some(fellow_to_dismiss) = player_manager::get_online_player(w, dismiss_guid) else {
        return;
    };

    fellowship::remove_fellowship_member(w, &fellowship, Some(fellow_to_dismiss), this);
}

// ACE: Player.FellowshipRecruit
pub fn fellowship_recruit(w: &mut World, this: ObjectGuid, new_player: Option<ObjectGuid>) {
    let Some(new_player) = new_player else { return };

    // An Olthoi player cannot join a fellowship
    if is_olthoi_player(w, new_player) {
        send(
            w,
            this,
            game_message_system_chat("The Olthoi's hunger for destruction is too great to understand a request for fellowship.", ChatMessageType::Broadcast),
        );
        player_networking::send_weenie_error(w, this, WeenieError::None);
        return;
    }

    if player_character::get_character_option(
        w,
        new_player,
        empyrean_entity::enums::CharacterOption::IgnoreFellowshipRequests,
    ) {
        let text = format!(
            "{} is not accepting fellowship requests.",
            name(w, new_player)
        );
        send(
            w,
            this,
            game_message_system_chat(&text, ChatMessageType::Fellowship),
        );
        send_event(w, this, |s| {
            game_event_weenie_error(s, WeenieError::FellowshipIgnoringRequests)
        });
    } else if let Some(fellowship) = fellowship(w, this) {
        if this.full() == fellowship.leader_guid(w) || fellowship.get(w).open {
            fellowship::add_fellowship_member(w, &fellowship, Some(this), Some(new_player));
        } else {
            send_event(w, this, |s| {
                game_event_weenie_error(s, WeenieError::YouMustBeLeaderOfFellowship)
            });
        }
    }
}

// ACE: Player.FellowshipNewLeader
pub fn fellowship_new_leader(w: &mut World, this: ObjectGuid, new_leader_guid: u32) {
    let Some(fellowship) = fellowship(w, this) else {
        return;
    };
    if this.full() == new_leader_guid {
        return;
    }

    if this.full() != fellowship.leader_guid(w) {
        log::warn!(
            "{} tried to assign new fellowship leader from {:08X} to {:08X}",
            name(w, this),
            fellowship.leader_guid(w),
            new_leader_guid
        );
        return;
    }

    let Some(new_leader) = player_manager::get_online_player(w, new_leader_guid) else {
        return;
    };

    if self::fellowship(w, new_leader).as_ref() != Some(&fellowship) {
        let text = format!("{} is not a member of the fellowship!", name(w, new_leader));
        send(
            w,
            this,
            game_message_system_chat(&text, ChatMessageType::Broadcast),
        );
        return;
    }

    fellowship::assign_new_leader(w, &fellowship, Some(this), Some(new_leader));
}

// ACE: Player.HandleFellowshipUpdateRequest
/// Called when player opens / closes the fellowship panel
pub fn handle_fellowship_update_request(w: &mut World, this: ObjectGuid, panel_open: bool) {
    if let Some(f) = fields_mut(w, this) {
        f.fellowship_panel_open = panel_open;
    }

    if fellowship(w, this).is_some() && fellowship_panel_open(w, this) {
        let session = session_of(w, this);
        let msg = game_event_fellowship_full_update(w, session);
        enqueue_send(w, session, msg);
    }
}
