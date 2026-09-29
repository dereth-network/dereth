//! Queue 5 — the DDD / dat-cache queue.
//!
//! Six opcodes are handled: `0xF7E2`, `0xF7E4`, `0xF7E5`, `0xF7E7`, `0xF7EA` and `0xF7EB`.
//! Anything else is dropped.
//!
//! **The end message is `0xF7EA` in both directions** and `0xF7EB` is a received-only "patching
//! pending, wait". The community catalogue has this backwards; the verified direction is recorded
//! in `docs/CORRECTIONS.md`.
//!
//! The client's dat *writes* are not done here: the cache hands each `DDD_DataMessage` to an async
//! save. This crate hands it to the caller as a [`DddEvent::Data`] and does nothing with it because
//! DAT persistence belongs to the cache layer.

use crate::client_session::{DropReason, SessionEvent};
use dereth_primitives::IncomingMessage;
use dereth_protocol::admin::{
    DddBeginDdd, DddData, DddError, DddInterrogation, DddInterrogationResponse,
};
use dereth_protocol::{read_body, Message, Opcode};

/// The DDD state machine's own value. The name misspells "Received" exactly
/// as the client's enum does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DddState {
    #[default]
    Idle,
    InterrogationReceved,
    Patching,
    EndSent,
    RunTime,
}

/// What the DDD exchange produced this tick.
#[derive(Debug, Clone, PartialEq)]
pub enum DddEvent {
    /// `0xF7E5` — the server asks what the client already has. The caller must answer with a
    /// [`DddInterrogationResponse`] built from its own dat iteration lists.
    Interrogation(DddInterrogation),
    /// `0xF7E7` — a patch session is starting.
    Begin(DddBeginDdd),
    /// `0xF7E2` — one record. **The caller writes it**; this crate does not touch the dats.
    Data(Box<DddData>),
    /// `0xF7E4`.
    Error(DddError),
    /// `0xF7EA` received. The client answers with the same opcode when it was still in
    /// `InterrogationReceved`, and always ends in `RunTime`.
    End,
    /// `0xF7EB` — "patching pending, wait". Raises the patch-time-pending notice.
    PatchtimePending,
}

/// The queue-5 dispatcher's decision: an event for the caller, and optionally a reply to send.
#[derive(Debug)]
pub struct DddDispatch {
    pub event: SessionEvent,
    /// `true` when the end-of-DDD handler would send `0xF7EA` back.
    pub send_end: bool,
}

/// Dispatch one queue-5 blob, advancing the DDD state machine.
pub fn dispatch(state: &mut DddState, m: &IncomingMessage) -> DddDispatch {
    let op = Opcode(m.opcode);
    let decoded = |e: DddEvent| DddDispatch {
        event: SessionEvent::Ddd(e),
        send_end: false,
    };

    macro_rules! decode {
        ($t:ty, $wrap:expr) => {
            match read_body::<$t>(&m.body) {
                Ok(v) => $wrap(v),
                Err(e) => {
                    return DddDispatch {
                        event: SessionEvent::Dropped {
                            queue: m.queue,
                            opcode: op,
                            reason: DropReason::Malformed(e),
                        },
                        send_end: false,
                    }
                }
            }
        };
    }

    match op {
        o if o == DddInterrogation::OPCODE => {
            *state = DddState::InterrogationReceved;
            decoded(decode!(DddInterrogation, DddEvent::Interrogation))
        }
        o if o == DddBeginDdd::OPCODE => {
            *state = DddState::Patching;
            decoded(decode!(DddBeginDdd, DddEvent::Begin))
        }
        o if o == DddData::OPCODE => decoded(decode!(DddData, |v| DddEvent::Data(Box::new(v)))),
        o if o == DddError::OPCODE => decoded(decode!(DddError, DddEvent::Error)),
        o if o == Opcode::DDD_ON_END_DDD => {
            // The end-of-DDD handler: the reply goes out only from `InterrogationReceved`,
            // and the state always ends at `RunTime`.
            let send_end = *state == DddState::InterrogationReceved;
            if send_end {
                *state = DddState::EndSent;
            }
            *state = DddState::RunTime;
            DddDispatch {
                event: SessionEvent::Ddd(DddEvent::End),
                send_end,
            }
        }
        o if o == Opcode::DDD_END_DDDMESSAGE => decoded(DddEvent::PatchtimePending),
        _ => DddDispatch {
            event: SessionEvent::Dropped {
                queue: m.queue,
                opcode: op,
                reason: DropReason::NoHandler,
            },
            send_end: false,
        },
    }
}

/// The response the client owes an interrogation.
///
/// The iteration lists come from the caller's own dat files, which this crate cannot see; it builds
/// the message so that the framing and the queue are in one place. `client_language` and `flags` are
/// the caller's too.
#[must_use]
pub fn interrogation_response(
    client_language: u32,
    iters_with_keys: Vec<dereth_protocol::admin::TaggedIterationList>,
    flags: u32,
) -> DddInterrogationResponse {
    DddInterrogationResponse {
        client_language,
        iters_with_keys,
        // The iterations-without-keys list is empty in every observed session.
        iters_without_keys: Vec::new(),
        flags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::{NetBlobId, NetQueue, RecipientId};
    use dereth_protocol::write_body;

    fn msg(opcode: u32, body: Vec<u8>) -> IncomingMessage {
        IncomingMessage {
            opcode,
            queue: NetQueue::ClientCache,
            sender: RecipientId(0),
            blob_id: NetBlobId(0),
            body,
        }
    }

    /// The H4 acceptance test: the interrogation exchange completes, and **the end message is
    /// `0xF7EA` in both directions**.
    ///
    /// Oracle: the cache's per-frame switch and its end-of-DDD handler, plus
    /// the recovered DAT-patching flow.
    #[test]
    fn the_interrogation_exchange_ends_with_f7ea_going_back() {
        let mut state = DddState::Idle;

        let ask = DddInterrogation {
            servers_region: 1,
            name_rule_language: 1,
            product_id: 1,
            supported_languages: vec![0, 1],
        };
        let d = dispatch(&mut state, &msg(0xF7E5, write_body(&ask).unwrap()));
        assert_eq!(state, DddState::InterrogationReceved);
        assert!(!d.send_end);
        assert_eq!(d.event, SessionEvent::Ddd(DddEvent::Interrogation(ask)));

        // ACE answers "you already have everything" with 0xF7EA, and the client sends 0xF7EA back.
        let d = dispatch(&mut state, &msg(0xF7EA, vec![]));
        assert!(d.send_end, "the reply is 0xF7EA, not 0xF7EB");
        assert_eq!(state, DddState::RunTime);
        assert_eq!(d.event, SessionEvent::Ddd(DddEvent::End));
    }

    /// `0xF7EB` is received-only and means "wait" — it must not end the exchange.
    #[test]
    fn f7eb_means_wait_and_sends_nothing() {
        let mut state = DddState::InterrogationReceved;
        let d = dispatch(&mut state, &msg(0xF7EB, vec![]));
        assert!(!d.send_end);
        assert_eq!(
            state,
            DddState::InterrogationReceved,
            "the state does not advance"
        );
        assert_eq!(d.event, SessionEvent::Ddd(DddEvent::PatchtimePending));
    }

    /// A `0xF7EA` that arrives when the client was already patching still ends the session but
    /// sends nothing back — the reply is gated on `dddInterrogationReceved`.
    #[test]
    fn the_end_reply_is_gated_on_the_interrogation_state() {
        let mut state = DddState::Patching;
        let d = dispatch(&mut state, &msg(0xF7EA, vec![]));
        assert!(!d.send_end);
        assert_eq!(state, DddState::RunTime);
    }

    /// The dat writes are the caller's: a data message is handed over intact.
    #[test]
    fn a_data_message_is_handed_to_the_caller_untouched() {
        let mut state = DddState::Patching;
        let data = DddData {
            dat_file_type: 0,
            dat_file_id: 1,
            resource_type: 1,
            resource_id: 0x0100_0001,
            iteration: 5,
            compressed: 1,
            version: 3,
            data_size: 8,
            data: vec![9, 9, 9, 9],
        };
        let d = dispatch(&mut state, &msg(0xF7E2, write_body(&data).unwrap()));
        assert_eq!(d.event, SessionEvent::Ddd(DddEvent::Data(Box::new(data))));
    }

    /// An opcode with no arm on this queue is dropped, not misrouted.
    #[test]
    fn an_unknown_database_opcode_is_dropped() {
        let mut state = DddState::Idle;
        let d = dispatch(&mut state, &msg(0x0013, vec![]));
        assert!(matches!(
            d.event,
            SessionEvent::Dropped {
                reason: DropReason::NoHandler,
                ..
            }
        ));
    }

    /// A malformed body is reported rather than panicking; parsers meet malformed input.
    #[test]
    fn a_malformed_body_is_reported() {
        let mut state = DddState::Idle;
        let d = dispatch(&mut state, &msg(0xF7E5, vec![1, 2]));
        assert!(matches!(
            d.event,
            SessionEvent::Dropped {
                reason: DropReason::Malformed(_),
                ..
            }
        ));
    }
}
