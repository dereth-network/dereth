//! Fixture interaction boundaries with explicit default callbacks.

use super::frame::interaction_frame_tail;
use super::*;

/// **The reply half of the inventory loop.**
///
/// The network dispatcher inventory arms, which are the *only*
/// things that ever move an item or release the request lock. The client sends a
/// move byte-exactly, ghosts the icon and takes an inventory lock with **no timeout**; these
/// arms are the answer.
///
/// | opcode | client arm |
/// |---|---|
/// | `0x0022 Item_ServerSaysContainID` | the item's server-says-move-item `(container, slot, 0, 0, notify UI = 1)`; when the item is not created yet the id is recorded in the container's list at `slot` so the later `0xF745` lands in the right place |
/// | `0x0023 Item_WearItem` | server-says-move-item `(0, 0, player id, slot, 1)` |
/// | `0x019A Item_ServerSaysMoveItem` | server-says-move-item `(0, 0, 0, 0, 1)` — the item leaves the pack entirely |
/// | `0x00A0 Character_ServerSaysAttemptFailed` | server-says-attempt-failed `(reason, 1)` on the object the **lock** names, not the one the message names — see the arm below |
///
/// `0x0023` is here as well because it is the same apply function and
/// the same lock, and equipping is the half of "my inventory is not interactable" that a
/// double-click produces.
///
/// The `0x00A0` row does **not** always clear the lock: the attempt-failed handler carries the
/// same object-id equality guard as the other three clearers, and this function hands it the
/// previous request's object id itself. Both halves are transcribed and tested.
///
/// The interaction event boundary applies inventory and action replies; the HUD boundary
/// projects session events into chat, vitals and panel notices. Both receive the same ordered
/// `SessionEvent` stream and retain their own state responsibilities.
///
/// **The "item not created yet" branch of `0x0022`** — the container's
/// server-says-contain-id record, which is
/// the world's pending-containment record. Twenty-nine of the corpus's 100 `0x0022` take it.
/// Without it the item would land at the **head** of the pack whatever slot the
/// server asked for — the head, not the tail, because setting the weenie description
/// substitutes `0` for the place-in-list lookup's `-1`.
pub fn apply_events(
    inter: &mut Interaction,
    events: &[dereth_client_net::client_session::SessionEvent],
    game: &mut dereth_client_model::World,
) {
    apply_events_at_boundary(inter, events, game, None, &mut |_, _, _| {});
}

/// Run an isolated interaction frame with registered-system timers and no chat subscriber.
/// The application's drawing tail uses `draw_use_time_with_chat_focus` instead, because its
/// registered-system timers have already run at frame entry.
///
/// Returns the [`UiRequest`]s nothing owns yet, for the caller to report.
#[allow(clippy::too_many_arguments)]
pub fn use_time(
    inter: &mut Interaction,
    store: &RetailDatStore,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    actions: Vec<crate::actions::Action>,
    player_desc_received: bool,
    viewport: (u32, u32),
    now: dereth_primitives::LocalTime,
) -> (Vec<UiRequest>, Vec<crate::actions::Action>) {
    use_time_with_chat_focus(
        inter,
        store,
        world,
        objects,
        net,
        actions,
        player_desc_received,
        viewport,
        now,
        &mut |_| {},
    )
}

/// Run the timer-bearing fixture frame with a caller-supplied chat subscriber.
#[allow(clippy::too_many_arguments)]
pub fn use_time_with_chat_focus(
    inter: &mut Interaction,
    store: &RetailDatStore,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    actions: Vec<crate::actions::Action>,
    player_desc_received: bool,
    viewport: (u32, u32),
    now: dereth_primitives::LocalTime,
    chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
) -> (Vec<UiRequest>, Vec<crate::actions::Action>) {
    interaction_frame_tail(
        inter,
        store,
        world,
        objects,
        net,
        actions,
        player_desc_received,
        viewport,
        now,
        chat_focus,
        true,
    )
}
impl Interaction {
    pub fn run_ui_requests(
        &mut self,
        game: &mut dereth_client_model::World,
        player_desc_received: bool,
        now: ServerTime,
    ) -> Vec<UiRequest> {
        self.run_ui_requests_with_chat_focus(game, player_desc_received, now, &mut |_| {})
    }
}
