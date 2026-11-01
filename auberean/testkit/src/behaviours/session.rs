//! The session: the game login, the connections to the login and world servers, the queues, and
//! what the client says to the servers and when.
//!
//! A row claims what the September 2013 client does on the wire, held to the client itself and to
//! the recorded January 2017 sessions.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "session.connection.the-login-connection-is-held-and-every-referral-followed",
        says: "The login connection stays up from the login to the end of the session. A referral \
               to a world server the client has no connection to is followed with a world login, \
               and the server's switch makes it the current world; a referral to one it is still \
               connected to only refreshes that connection, and the switch follows on it. World \
               messages go to the current world server, the character list's to the login server, \
               and leaving the world drops every world connection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-REFERRAL"),
        station: "auberean-client-net::cpu::session::the_login_connection_is_held_and_every_referral_is_followed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.logon.the-login-carries-version-ticket-account-and-key-blob",
        says: "The game login carries the version 1802, a ticket login with flags 1, the time, the \
               account as typed, a session-key blob (its twelve-byte header, then 64 bytes) and the \
               ticket after its 16-bit length, every string scrambled.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-LOGON"),
        station: "auberean-client-net::cpu::logon::the_login_carries_version_ticket_account_and_key_blob",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.queues.data-before-events-and-the-logon-queue-keeps-only-chat",
        says: "The client receives on the control, events, logon and data queues, and each frame \
               reads the data queue, then the events queue, then the control queue (whose messages \
               it drops), then the logon queue, where it keeps only the chat service's. It sends \
               the data download's messages on the data queue, the character list's and chat on the \
               logon queue, and everything of the world on the game-events queue.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-QUEUES"),
        station: "auberean-client-net::cpu::session::the_client_reads_the_data_queue_first_and_keeps_only_chat_from_the_logon_queue",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.send.every-message-stamped-from-its-counter",
        says: "Each message the client sends carries the ordering type and stamp of its counter: \
               the game-events queue's are ordered, stamped from a counter counted up before each \
               send and started again at world entry; the data download's and chat have their own \
               type, stamped with the count so far from 0; everything else is unordered, counted \
               up before each send. One blob sequence, from 0, runs across every connection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-BLOB-STAMPS"),
        station: "auberean-client-net::cpu::session::every_message_is_stamped_from_its_counter_and_the_sequence_starts_at_zero",
        tier: Tier::Cpu,
    },
];
