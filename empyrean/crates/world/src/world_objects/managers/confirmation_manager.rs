// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Managers/ConfirmationManager.cs
//! Port of `Source/ACE.Server/WorldObjects/Managers/ConfirmationManager.cs`.
//!
//! The confirmation popups a player has open. The manager lives on the player
//! (`PlayerFields.confirmation_manager`); its `Player` is the `this` every function takes. ACE's
//! `ConcurrentDictionary` is only read by key, so its order never shows.

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{ChatMessageType, ConfirmationType};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::confirmation::Confirmation;
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_confirmation_done::game_event_confirmation_done;
use crate::network::game_event::events::game_event_confirmation_request::game_event_confirmation_request;
use crate::network::game_messages::game_message::{self, GameMessage};
use crate::network::sequence::u_int_sequence::UIntSequence;
use crate::sessions::SessionData;
use crate::world_objects::player;
use crate::World;

// ACE: ConfirmationManager
#[derive(Debug)]
pub struct ConfirmationManager {
    // ACE: ConfirmationManager.Player
    // The player that owns the manager: the `this` of every function below.

    // ACE: ConfirmationManager.confirmations
    confirmations: DotNetDict<ConfirmationType, Confirmation>,

    // ACE: ConfirmationManager.contextSequence
    context_sequence: UIntSequence,
}

impl Default for ConfirmationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// `ConfirmationManager.confirmationTimeout`.
pub const CONFIRMATION_TIMEOUT: f64 = 30.0;

impl ConfirmationManager {
    // ACE: ConfirmationManager.ConfirmationManager
    /// `new ConfirmationManager(player)`: `Player` is the owning player's guid, passed to every call.
    #[must_use]
    pub fn new() -> Self {
        Self {
            confirmations: DotNetDict::new(),
            context_sequence: UIntSequence::new_primed(true, u32::MAX),
        }
    }

    /// Whether a confirmation of `confirmation_type` is open (test and diagnostics read).
    #[must_use]
    pub fn contains(&self, confirmation_type: ConfirmationType) -> bool {
        self.confirmations.contains_key(&confirmation_type)
    }

    /// The open confirmation of `confirmation_type`, if any.
    #[must_use]
    pub fn get(&self, confirmation_type: ConfirmationType) -> Option<&Confirmation> {
        self.confirmations.get(&confirmation_type)
    }
}

fn manager(w: &mut World, this: ObjectGuid) -> &mut ConfirmationManager {
    &mut w
        .objects
        .get_mut(this)
        .and_then(|o| o.player.as_mut())
        .expect("ACE: ConfirmationManager.Player is null (NullReferenceException)")
        .player
        .confirmation_manager
}

/// `Player.Session` (ACE dereferences it: a player without one is a `NullReferenceException`).
fn session_of(w: &World, this: ObjectGuid) -> empyrean_net::SessionId {
    player_manager::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)")
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
    game_message::enqueue_send(w, session, msg);
}

fn name(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

// ACE: ConfirmationManager.EnqueueSend
/// Builds a new confirmation request on the server, and sends the request to the client. False
/// when a confirmation of the same type is already open.
pub fn enqueue_send(
    w: &mut World,
    this: ObjectGuid,
    mut confirmation: Confirmation,
    text: &str,
) -> bool {
    let m = manager(w, this);
    confirmation.context_id = m.context_sequence.next_value();
    let (confirmation_type, context_id) = (confirmation.confirmation_type, confirmation.context_id);
    if m.confirmations.try_add(confirmation_type, confirmation) {
        send_event(w, this, |s| {
            game_event_confirmation_request(s, confirmation_type, context_id, text)
        });
        let mut timeout_confirmation = ActionChain::new();
        timeout_confirmation.add_delay_seconds(w, CONFIRMATION_TIMEOUT);
        timeout_confirmation.add_action(Actor::Object(this), move |w: &mut World| {
            enqueue_abort(w, this, confirmation_type, context_id)
        });
        timeout_confirmation.enqueue_chain(w);
    } else {
        //log.Error($"{Player.Name}.ConfirmationManager.EnqueueSend({confirmation.ConfirmationType}, {confirmation.ContextId}) - duplicate confirmation type");
        return false;
    }

    true
}

// ACE: ConfirmationManager.EnqueueAbort
/// This only needs to be sent in the rare event the server needs to force close a confirmation
/// dialog that is still active on the client.
pub fn enqueue_abort(
    w: &mut World,
    this: ObjectGuid,
    confirmation_type: ConfirmationType,
    context_id: u32,
) {
    // A player that has left the world never runs this: ACE's timeout action waits in the player's
    // own action queue, which only its tick drains.
    if w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .is_none()
    {
        return;
    }

    let found = manager(w, this)
        .confirmations
        .get(&confirmation_type)
        .map(|c| (c.confirmation_type, c.context_id));
    let Some((confirm_type, confirm_context_id)) = found else {
        return;
    };
    if confirm_context_id != context_id {
        return;
    }

    send_event(w, this, |s| {
        game_event_confirmation_done(s, confirmation_type, context_id)
    });

    match confirm_type {
        ConfirmationType::SwearAllegiance => {
            // This event automatically triggers a response from client, however due to the way ACE works, we want it to process as a timeout to match pcap output as best we can and inform players of results.
            handle_response(w, this, confirm_type, confirm_context_id, false, true);
        }
        ConfirmationType::AlterSkill
        | ConfirmationType::AlterAttribute
        | ConfirmationType::CraftInteraction
        | ConfirmationType::Augmentation
        | ConfirmationType::Yes_No => {
            player::send_message(
                w,
                this,
                "You waited too long to answer the question!",
                ChatMessageType::Broadcast,
            );
            // These events automatically trigger a response from client, others do not.
            // do nothing further
        }
        _ => {
            handle_response(w, this, confirm_type, confirm_context_id, false, true);
        }
    }
}

// ACE: ConfirmationManager.HandleResponse
/// The client has responded to a confirmation box. `this` is the player that owns the
/// `ConfirmationManager`; ACE's default: `timeout = false`.
pub fn handle_response(
    w: &mut World,
    this: ObjectGuid,
    confirm_type: ConfirmationType,
    context_id: u32,
    response: bool,
    timeout: bool,
) -> bool {
    let Some(confirm) = manager(w, this).confirmations.remove(&confirm_type) else {
        match confirm_type {
            ConfirmationType::SwearAllegiance => {
                // do nothing.
            }
            ConfirmationType::Fellowship => {
                // dialog box does not dismiss on ConfirmationDone, unlike on all other types, so we must let the player know when they click either yes or no, nothing occured because the offer has already expired.
                player::send_message(
                    w,
                    this,
                    "That offer of fellowship has expired.",
                    ChatMessageType::Broadcast,
                ); // still looking for pcap accurate response
            }
            _ => {
                log::error!(
                    "{}.ConfirmationManager.HandleResponse({confirm_type:?}, {context_id}, {response}, {timeout}) - confirmType not found",
                    name(w, this)
                );
            }
        }

        return false;
    };

    if confirm.context_id != context_id {
        let (confirm_type2, confirm_context_id) = (confirm.confirmation_type, confirm.context_id);
        if confirm_type2 == ConfirmationType::Fellowship {
            // dialog box does not dismiss on ConfirmationDone, unlike on all other types, so we must let the player know when they click either yes or no, nothing occured because the offer has already expired.
            if !manager(w, this)
                .confirmations
                .try_add(confirm_type2, confirm)
            {
                log::error!(
                    "{}.ConfirmationManager.HandleResponse({confirm_type2:?}, {confirm_context_id}) - Unable to re-add confirmation, duplicate confirmation type",
                    name(w, this)
                );
            }

            player::send_message(
                w,
                this,
                "That offer of fellowship has expired.",
                ChatMessageType::Broadcast,
            ); // still looking for pcap accurate response

            return false;
        }

        log::error!(
            "{}.ConfirmationManager.HandleResponse({confirm_type:?}, {context_id}, {response}, {timeout}) - contextId != confirm.ContextId",
            name(w, this)
        );

        if !manager(w, this)
            .confirmations
            .try_add(confirm_type2, confirm)
        {
            log::error!(
                "{}.ConfirmationManager.HandleResponse({confirm_type2:?}, {confirm_context_id}) - Unable to re-add confirmation, duplicate confirmation type",
                name(w, this)
            );
        }

        return false;
    }

    confirm.process_confirmation(w, response, timeout);

    true
}
