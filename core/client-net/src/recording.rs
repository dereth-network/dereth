//! What a raw recording says that only a datagram-header parse can read: the connection sequence
//! number of its login request, and the client blobs each time the recorded client entered the
//! world.
//!
//! Recording values and blob parsing come from the storage-independent
//! [`client_session::recording`](crate::client_session::recording) module.
//! The half here parses datagram headers and fragments, which is the transport's work, and the
//! client session reaches the transport only through [`Transport`](dereth_primitives::Transport).
//! Every harness that replays a recording into a client needs both halves, so they are here once
//! instead of in every test file that replays one.

use std::collections::BTreeSet;

use dereth_transport::wire::{PacketFlags, ParsedPacket};

use crate::client_session::recording::entries;
pub use crate::client_session::recording::{parse, peer, CaptureError, Datagram, RecordedEntry};

/// The connection sequence number this endpoint's authenticator carries, read out of the
/// recording's own login request rather than invented, or `None` when the recording has no login
/// request.
///
/// The shard does nothing with it, but it is part of the authenticator a replay endpoint is built
/// with, so taking the recorded one keeps a replay byte-identical to what was recorded.
#[must_use]
pub fn connection_sequence_number(records: &[Datagram]) -> Option<u32> {
    // The field sits after the login-request header's four leading dwords.
    const AUTH_SEQ_OFFSET: usize = 8 + 4 + 4 + 4;
    for r in records.iter().filter(|r| r.c2s) {
        let Ok(p) = ParsedPacket::parse(&r.raw) else {
            continue;
        };
        if let Some(body) = p.optional.get(&PacketFlags::LOGIN_REQUEST) {
            let b = body.get(AUTH_SEQ_OFFSET..AUTH_SEQ_OFFSET + 4)?;
            return Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        }
    }
    None
}

/// Every enter-world in a recording, in order: the whole-blob fragments of the client's
/// datagrams, each blob once however many times it was retransmitted, read by
/// [`entries`](crate::client_session::recording::entries). Both messages
/// an enter-world is made of fit in one fragment, so no reassembly is needed.
///
/// `c2s` yields `(index, datagram)` for the client-to-server datagrams only, in capture order; the
/// index is whatever the caller uses to find its place again.
///
/// # Panics
/// As [`entries`](crate::client_session::recording::entries): an enter-world request with no answer, or an answer with no request.
pub fn recorded_enter_world_requests<'a>(
    c2s: impl IntoIterator<Item = (usize, &'a [u8])>,
) -> Vec<RecordedEntry> {
    let mut seen = BTreeSet::new();
    let mut blobs: Vec<(usize, Vec<u8>)> = Vec::new();
    for (record, raw) in c2s {
        let Ok(packet) = ParsedPacket::parse(raw) else {
            continue;
        };
        for fragment in &packet.fragments {
            if fragment.header.num_frags != 1 || fragment.payload.len() < 4 {
                continue;
            }
            if seen.insert((fragment.header.queue_id, fragment.header.blob_id())) {
                blobs.push((record, fragment.payload.to_vec()));
            }
        }
    }
    entries(blobs.iter().map(|(r, b)| (*r, b.as_slice())))
}

/// [`recorded_enter_world_requests`] over a whole recording, each entry's `record` an index into
/// `records`.
///
/// # Panics
/// As [`recorded_enter_world_requests`].
#[must_use]
pub fn enter_world_requests(records: &[Datagram]) -> Vec<RecordedEntry> {
    recorded_enter_world_requests(
        records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.c2s)
            .map(|(i, r)| (i, r.raw.as_slice())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_session::testing::capture;

    /// A recording with no login request has no sequence number, which a caller turns into a zero
    /// rather than a panic.
    #[test]
    fn a_recording_with_no_login_request_has_no_sequence_number() {
        let only_server_traffic = vec![Datagram {
            t: 0.0,
            c2s: false,
            pair: 0,
            raw: vec![0; 32],
        }];
        assert_eq!(connection_sequence_number(&only_server_traffic), None);
    }

    /// And a committed recording does have one, read from its own login request.
    #[test]
    fn a_committed_recording_carries_its_own_connection_sequence_number() {
        let recs = capture::shared_session("first-login-walk-jump");
        assert!(
            connection_sequence_number(recs).is_some(),
            "the recording opens with a login request; if this is None the reader has drifted"
        );
    }

    /// Every enter-world is read at a client datagram and names a character and an account.
    #[test]
    fn a_recordings_enter_worlds_are_read_at_client_datagrams() {
        let recs = capture::shared_session("first-login-walk-jump");
        let entries = enter_world_requests(recs);
        assert!(!entries.is_empty(), "the recording enters the world");
        for e in &entries {
            assert!(recs[e.record].c2s, "{e:?}");
            assert!(!e.account.is_empty(), "{e:?}");
        }
    }
}
