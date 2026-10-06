//! What the character screen's message box says when the session ends.
//!
//! The runtime hands every interface the reason as a string id, which the later interface resolves
//! in its own string table. This interface's era had no table for these messages: it wrote its own
//! sentences into the box, and those are what it shows here. A network refusal, which that era's
//! box never showed, reads the later client's sentence for it out of the later files; and a reason
//! that carries its own sentence (a boot, a ban) is shown as it came.

use dereth_client_contract::disconnect::{character_error_string_id, SERVER_DIED_STRING_ID};
use dereth_client_contract::pregame::DisconnectNotice;
use dereth_primitives::AssetSource;

/// The box's text when the server stopped answering.
pub const CONNECTION_LOST: &str = "\n\n\nServer connection lost";

/// What a network refusal reads when the later files have no sentence for it either: the later
/// client's catch-all.
pub const CONNECTION_ERROR: &str = "There was an error with this connection";

/// The box's text for a character error, by its code. A code with no sentence of its own reads
/// the catch-all.
#[must_use]
pub fn character_error(code: u32) -> &'static str {
    match code {
        1 => "\n\nCannot have two accounts logged on at the same time.",
        3 => "\nServer could not access your account information. Please try again in a few minutes.",
        4 => "\nThe server has disconnected. Please try again in a few minutes.",
        5 => "\n\nServer could not log off your character ",
        6 => "\n\nServer could not delete your character.",
        8 => "\n\nThe account you specified is already in use.",
        9 => "\n\nThe account name you specified was not valid.",
        10 => "\n\nThe account you specified\ndoesn't exist.",
        11 => "\nServer could not put your character in the game. Please try again in a few minutes.",
        12 => "\n\nYou cannot enter the game with a stress creating character.",
        13 => "\nOne of your characters is still in the world.  Please try again in a few minutes.",
        14 => "\n\nServer unable to find player account.  Please try again later.",
        15 => "\n\n\nYou do not own this character.",
        16 => "\nOne of your characters is currently in the world.  Please try again later.  This is likely an internal server error.",
        17 => "\n\nPlease try again in a few minutes.  If this problem persists, the character might be out of date and no longer usable.",
        18 => "\n\nThis character's data has been corrupted.  Please delete it and create a new character.",
        19 => "\n\nThis character's starting server is experiencing difficulties.  Please try again in a few minutes.",
        20 => "\n\nThis character couldn't be placed in the world right now. Please try again in a few minutes.",
        21 => "\n\nSorry, but the Asheron's Call server is full currently.  Please try again later.",
        23 => "\n\nA save of this character is still in progress, please try again later.",
        // Raised by the client itself when the subscription's announced end arrives.
        24 => "\n\n\nYour subscription to this game has expired",
        _ => "\n\nUnknown Character Error from the server",
    }
}

/// The text for the session's ending reason `error` (a string id, or a sentence already) and the
/// notice it came with. `later` is the later client's files, where a network refusal's sentence is.
#[must_use]
pub fn message(
    error: &str,
    notice: Option<&DisconnectNotice>,
    later: Option<&dyn AssetSource>,
) -> String {
    // A character error by its code first: two codes share one string id, with different
    // sentences.
    let code = match notice {
        Some(DisconnectNotice::CharacterError(code))
            if character_error_string_id(*code) == Some(error) =>
        {
            Some(*code)
        }
        _ => (0..32).find(|c| character_error_string_id(*c) == Some(error)),
    };
    if let Some(code) = code {
        return character_error(code).to_owned();
    }
    if error == SERVER_DIED_STRING_ID {
        return CONNECTION_LOST.to_owned();
    }
    if error.starts_with("ID_NetError_") || error.starts_with("ID_ConnectionError_") {
        let id = dereth_primitives::num::hash::str_hash(error.as_bytes());
        return later
            .and_then(|files| dereth_client_runtime::connect_failure::net_error_sentence(files, id))
            .unwrap_or_else(|| CONNECTION_ERROR.to_owned());
    }
    error.to_owned()
}
