//! Not ACE: the hub's question whether one of this world's characters, not online, would have been
//! shown a tell (`docs/network/02-link-protocol.md` section 8), asked before the hub carries the
//! tell to the character's person elsewhere: its Discord.
//!
//! A tell to a character that is not online is answered `offline` ([`super::tells`]). The hub then
//! asks `reach`, and the world answers `reachable` true only for a character of its own, not
//! online, on the network, whose link is its own (it asked for its code: the hub asks about one
//! that never did for an earlier character of its name, since deleted), squelching neither the
//! sender nor the sender's world, and blocking neither (the operator's blocks). A `true` takes one
//! from the world's pace of tells from the sender's world to the character, as a shown tell does,
//! and one past it is `false`. Anything else is `false`, alike: a character online now, one off
//! the network, one deleted or waiting to be, one this world does not have. The answer shows the
//! character nothing and gives no stand-in id.
//!
//! An answer waits with the answers owed to the sender's world: at most
//! [`super::MAX_ANSWERS_PER_WORLD`] for one world (a question past that is not answered, and the
//! hub carries the tell nowhere else), a world at a time in turn, dropped once the hub has stopped
//! waiting for it, or when the link goes.

use dereth_network_protocol::{Address, ReachMessage};

use super::{blocked, may_owe, may_receive, owe, tells, LinkState};
use crate::entity::i_player;
use crate::managers::player_manager;
use crate::World;

/// One of the reach messages from the hub: a question is answered; an answer, which only a world
/// sends, is ignored.
pub(super) fn on_message(w: &mut World, m: ReachMessage) {
    let ReachMessage::Ask { id, from, to } = m else {
        return;
    };
    if w.federation.link != LinkState::Up {
        return;
    }
    let origin = from.world().to_owned();
    if !may_owe(w, &origin) {
        log::debug!(
            "Dereth Network: {origin} is owed its share of answers; a question is not taken"
        );
        return;
    }
    let reachable = reachable(w, &from, &to);
    owe(w, &origin, ReachMessage::Result { id, reachable }.to_json());
}

/// Whether `to`, one of this world's characters not online, would have been shown a tell from
/// `from`; a `true` takes one from the world's pace of tells from `from`'s world to `to`.
fn reachable(w: &mut World, from: &Address, to: &Address) -> bool {
    let slug = w.federation.settings.slug.as_str();
    // This world's own characters never reach it through the hub.
    if to.world() != slug || from.world() == slug {
        return false;
    }
    let (Some(player), false) = player_manager::find_by_name(w, to.name()) else {
        return false;
    };
    let character = player.guid().full();
    let own = w
        .federation
        .store
        .characters
        .get(&character)
        .is_some_and(|r| r.settings.on && r.asked_link);
    let shown = own
        && !blocked(w, from)
        && !tells::squelches(w, character, from)
        && tells::display_name(from).is_some()
        // The shard is read last, only for a character that would be shown the tell.
        && !i_player::is_deleted(w, player)
        && !i_player::is_pending_deletion(w, player);
    let rate = w.federation.limits.tells_per_recipient;
    shown && may_receive(w, from.world(), &to.key(), rate)
}
