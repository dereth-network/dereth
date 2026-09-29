//! When a recording's client asked to enter the world, and as which character.
//!
//! A replay harness stands in for the player at the character screen, and the choice it makes is
//! not on the server's side of the wire. Entering as the first character of the first
//! `0xF658 Login_LoginCharacterSet`, once, is indistinguishable from the truth over a recording
//! that enters the world one time and stays, and wrong over one that logs off and re-enters on
//! more than one character: the rest of that recording would replay on a client sitting at the
//! character screen.
//!
//! So the answer is read from the recording's **own client-to-server blobs**: the empty
//! `0xF7C8 Login_SendEnterWorldRequest` marks the moment the player pressed enter, and the
//! `0xF657 Login_SendEnterWorld` that follows it names the character and the account. A replay
//! calls `enter_world` when it reaches the datagram that carried the `0xF7C8`, which is exactly
//! where the recorded client did.
//!
//! This half reads blobs. Taking them out of the recorded datagrams, once each however often they
//! were retransmitted, is the transport's work, and `dereth_client_net::recording` does it and
//! calls this.

use dereth_primitives::ObjectId;
use dereth_protocol::login::LoginSendEnterWorld;
use dereth_protocol::{Message, Opcode, Reader};

/// One enter-world the recorded client performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedEntry {
    /// The caller's index of the client-to-server datagram that carried the `0xF7C8`.
    pub record: usize,
    /// The character the following `0xF657` named.
    pub character: ObjectId,
    /// The account the following `0xF657` named.
    pub account: String,
}

/// Every enter-world in a sequence of client-to-server blobs, in order.
///
/// `blobs` yields `(index, blob)` in capture order, each blob whole (its opcode dword and its body)
/// and each once; the index is whatever the caller uses to find its place again.
///
/// # Panics
/// On an `0xF7C8` with no `0xF657` after it before the next `0xF7C8` or the end, and on an
/// `0xF657` with no `0xF7C8` before it: either means the recording is not what this reads it as,
/// and a replay that guessed would be measuring the guess.
pub fn entries<'a>(blobs: impl IntoIterator<Item = (usize, &'a [u8])>) -> Vec<RecordedEntry> {
    let mut out = Vec::new();
    let mut pending: Option<usize> = None;
    for (record, blob) in blobs {
        let Some(head) = blob.get(..4) else {
            continue;
        };
        let opcode = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        if opcode == Opcode::LOGIN_SEND_ENTER_WORLD_REQUEST.0 {
            assert!(
                pending.is_none(),
                "datagram {record}: a second `0xF7C8` before the first one's `0xF657`"
            );
            pending = Some(record);
        } else if opcode == Opcode::LOGIN_SEND_ENTER_WORLD.0 {
            let mut rd = Reader::new(&blob[4..]);
            let m = LoginSendEnterWorld::read(&mut rd).unwrap_or_else(|e| {
                panic!("datagram {record}: a `0xF657` that does not decode: {e:?}")
            });
            let ask = pending.take().unwrap_or_else(|| {
                panic!("datagram {record}: a `0xF657` with no `0xF7C8` before it")
            });
            out.push(RecordedEntry {
                record: ask,
                character: m.character,
                account: m.account,
            });
        }
    }
    assert!(
        pending.is_none(),
        "a trailing `0xF7C8` that no `0xF657` answered"
    );
    out
}
