//! Every recorded `Movement_SetObjectMovement 0xF74C` buffer decodes and consumes its bytes
//! exactly, through `dereth_protocol::movement::MovementBuffer`, and lands in one of its two
//! populations: the animating `MovementType::Invalid` arm or the four `MoveTo`/`TurnTo` arms. The
//! header's five bytes are followed by a pad computed from the **blob** offset (10 + 5 -> 16), not
//! from the buffer's own start; the other convention leaves three bytes over on every type-0
//! buffer.
//!
//! Fixture: every recording the corpus index names, its server datagrams reassembled by the
//! client's own network endpoint; the denominator is the corpus's own `0xF74C` count.
//!
//! Behaviour: none (decoder conformance over the recorded corpus)

use dereth_client_net::client_session::testing::{capture, session_names};
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::net::ClientNetwork;
use dereth_primitives::LocalTime;

use crate::common::recorded_movement_events;

/// Every recorded movement buffer decodes with its bytes consumed exactly, and every one of them is
/// either the animating arm or a `MoveTo`/`TurnTo` arm. The scan is measured against the corpus's
/// own `0xF74C` count, so a scan that read half the corpus cannot pass.
#[test]
fn every_movement_buffer_in_the_corpus_decodes_exactly() {
    let mut seen = 0usize;
    let mut interpreted = 0usize;
    let mut move_to = 0usize;
    for name in session_names() {
        let records = capture::shared_session(name);
        let mut net = ClientNetwork::new(
            "127.0.0.1:19000",
            7304,
            "ac01",
            "pass",
            connection_sequence_number(records).expect("the capture has a LoginRequest"),
        )
        .expect("host");
        for r in records.iter().filter(|r| !r.c2s) {
            net.feed(&r.raw, r.peer(), LocalTime(r.t));
        }
        while let Some(m) = dereth_primitives::Transport::poll(&mut net.session.transport) {
            if m.opcode != 0xF74C {
                continue;
            }
            let msg = dereth_protocol::read_body::<
                dereth_protocol::movement::MovementSetObjectMovement,
            >(&m.body)
            .expect("0xF74C decodes");
            let buf = msg
                .decoded_movement()
                .expect("every movement buffer must decode with its bytes consumed exactly");
            seen += 1;
            if buf.body.interpreted.is_some() {
                interpreted += 1;
            } else {
                move_to += 1;
            }
        }
    }
    assert!(seen > 0, "no movement buffers in the corpus");
    assert_eq!(seen, interpreted + move_to);
    assert!(
        interpreted > 0,
        "not one buffer took the MovementType::Invalid arm, which is the animating one"
    );
    assert!(
        move_to > 0,
        "not one buffer took a MoveTo/TurnTo arm, so the second population was never decoded"
    );
    assert_eq!(
        seen,
        recorded_movement_events(),
        "every recorded 0xF74C buffer was decoded"
    );
}
