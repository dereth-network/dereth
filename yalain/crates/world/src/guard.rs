//! A panic in the world stops the world.
//!
//! The world's work changes many records by hand with nothing to take it back: a character can be
//! out of [`Server::players`] while a step runs, an item can have left one holder and not reached
//! another. So after a panic the memory can't be told from the truth, and keeping one player out of
//! the saves wouldn't do: what it had reached another player would be held twice once the other is
//! kept. So, whatever panicked and however small it looks:
//!
//! - what whole pieces of work held goes to the store, all a pass had kept before it, and all that
//!   saves failing had been holding, the way a stop writes it;
//! - what the piece that panicked held is put back as it was, and no one's memory is saved again:
//!   each character goes back to its last save, whole, and every item that moved was saved with both
//!   its sides, so the store has each in one place;
//! - everyone in the world is told, and every connection is ended;
//! - the operator reads an error line, and the server exits with a failure, so a service manager
//!   starts it again and no staged update goes in.

use std::panic::{catch_unwind, AssertUnwindSafe};

use auberean_protocol::login::CharacterRefused;
use yalain_net::ServerNet;
use yalain_store::Store;

use crate::{Clock, Server};

/// What each player in the world is told as it stops.
pub const STOPPING: &str =
    "This world hit an error and is stopping. You'll be back as of your last save.";

/// What a panic carried, as words.
#[must_use]
pub fn words(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no words".to_owned())
}

/// Something in the world's work panicked: from here on nothing of its memory is saved, what the
/// piece under way held is put back, and everyone is told and let go. The caller then writes out what
/// is held, as any stop does.
pub fn stopped<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock, what: &str) {
    s.saves.take_back();
    s.panicked = Some(what.to_owned());
    tracing::error!(
        "THE WORLD PANICKED: {what}. It is stopping: what whole events kept is written, and nothing \
         since. Characters go back to their last saves. This is a bug; the line above says where."
    );
    // The connections may be what panicked; the store's writing must still come after.
    let told = catch_unwind(AssertUnwindSafe(|| {
        for world in s.players.keys() {
            crate::login::notice(net, *world, STOPPING);
        }
        crate::login::let_everyone_go(s, net, CharacterRefused::SERVER_CRASH, clock.local);
    }));
    if told.is_err() {
        tracing::error!("the connections could not be told the world is stopping");
    }
}
