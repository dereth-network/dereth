//! Steps over the **whole corpus** rather than over one recording.
//!
//! A scenario can say "for every recording that carries one of these, deliver the whole of it and
//! count what reached the screen". `Corpus::load_all`, `session_names` and `Corpus::count` in
//! `dereth_client_net::client_session` are the pieces; this is the step over them. It is separate
//! from [`crate::inbound`], which delivers one recording.
//!
//! # What is here, and what is deliberately not
//!
//! [`sweep`] delivers a recording one datagram at a time and lets the caller look **between**
//! them. That is the point: a claim about a surface the client *clears* -- the bubble
//! strip, which the log-off at the end of every recording empties -- cannot be made by replaying a
//! whole recording and then reading it, because what it is about has been thrown away by then.
//! [`crate::Inbound::from_raw_capture`] remains the step for everything else, and this does not
//! replace it.
//!
//! The census helpers beside it read the **decoded corpus index** and never a raw payload's text.
//! A count taken off a recording is taken from the index, so no recorded words reach a test's
//! output.

use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};

use crate::client::HeadlessClient;

/// The ordered game-event envelope, server to client.
const GAME_EVENT: u32 = 0xF7B0;

/// Every recording the locked corpus holds, in the index's own order.
#[must_use]
pub fn sessions() -> &'static [&'static str] {
    dereth_client_net::client_session::testing::session_names()
}

/// One recording, decoded.
///
/// # Panics
/// Panics when the recording does not load. The corpus is generated from the committed
/// recordings, so a missing one is a broken checkout and a sweep that carried on would be
/// asserting over nothing.
#[must_use]
pub fn corpus(session: &str) -> Corpus {
    Corpus::load(session)
        .unwrap_or_else(|e| panic!("the recording {session} does not parse: {e}"))
        .unwrap_or_else(|| {
            panic!(
                "the decoded corpus has no recording {session}; it is generated from the \
                 committed recordings and a missing one is a broken checkout"
            )
        })
}

/// Every recording that carries at least one server-to-client ordered **event** of `sub_type`,
/// with how many of them it carries.
///
/// The count is the corpus index's own, which is what makes it an oracle: a scenario that has
/// counted something the client did compares it against this rather than against an integer typed
/// into a source file, so promoting a recording moves the expectation instead of reddening it.
#[must_use]
pub fn sessions_carrying_event(sub_type: u32) -> Vec<(&'static str, usize)> {
    sessions()
        .iter()
        .filter_map(|name| {
            let n = corpus(name).count_event(sub_type);
            (n > 0).then_some((*name, n))
        })
        .collect()
}

/// Every server-to-client ordered **event** of `sub_type` in `session`, as the index holds them.
#[must_use]
pub fn events(session: &str, sub_type: u32) -> Vec<CorpusBlob> {
    corpus(session)
        .blobs
        .into_iter()
        .filter(|b| {
            b.dir == Direction::ServerToClient
                && b.opcode == GAME_EVENT
                && event_sub_type(b) == Some(sub_type)
        })
        .collect()
}

/// The sub-type of an ordered event blob: the envelope is `[opcode][object][stamp][sub-type]`, so
/// it is the dword at twelve -- four further in than an ordered **action**'s, which is a recorded
/// mistake and reads the stamp.
#[must_use]
pub fn event_sub_type(b: &CorpusBlob) -> Option<u32> {
    let s = b.payload.get(12..16)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// The body of an ordered event, without its envelope -- what the session layer hands on.
#[must_use]
pub fn event_body(b: &CorpusBlob) -> &[u8] {
    b.payload.get(16..).unwrap_or_default()
}

/// Deliver the whole of `session` through the client's real transport, one datagram at a time,
/// and call `watch` after each one the shard sent.
///
/// This is [`crate::Inbound::from_raw_capture`] with a look between the datagrams. The look is the
/// point: the surfaces a recording's own log-off clears -- the bubble strip is one -- hold what
/// they were given only until the ending arrives, so a claim about what reached one of them has to
/// be taken while it is still there. The panel's own sweep is what would take it in a running
/// client; `watch` stands in for that.
///
/// The client must already have an endpoint: a recorded login, or one attached with
/// [`crate::replay::Peer`].
///
/// # Panics
/// Panics when the recording carries no server-to-client datagram at all, which would read as a
/// pass over nothing.
pub fn sweep(
    client: &mut HeadlessClient,
    session: &str,
    mut watch: impl FnMut(&mut HeadlessClient),
) {
    sweep_until(client, session, |c| {
        watch(c);
        None::<()>
    });
}

/// [`sweep`], stopping as soon as `stop` answers `Some`.
///
/// Every recording ends in a log-off, and the ending empties the world: a scenario that wants an
/// object the recording created has to take it while the recording is still running, which is what
/// this is for.
///
/// # Panics
/// As [`sweep`].
pub fn sweep_until<T>(
    client: &mut HeadlessClient,
    session: &str,
    mut stop: impl FnMut(&mut HeadlessClient) -> Option<T>,
) -> Option<T> {
    let records = crate::replay::records(session);
    let mut fed = 0usize;
    let mut entered = false;
    for r in &records {
        // Both directions move the recording's clock; only the shard's half is delivered.
        let payload = if r.c2s {
            None
        } else {
            Some((r.raw.as_slice(), r.peer()))
        };
        let delivered = payload.is_some();
        fed += usize::from(delivered);
        client.replay_datagram(payload, r.t, &mut entered);
        if delivered {
            if let Some(v) = stop(client) {
                return Some(v);
            }
        }
    }
    assert!(
        fed > 0,
        "the recording {session} carries no server-to-client datagram; a sweep that delivered \
         nothing would read as a pass over nothing"
    );
    None
}
