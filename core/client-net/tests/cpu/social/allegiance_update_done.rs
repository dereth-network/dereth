//! Contracts for allegiance update done.
//! Fixture: shared recorded messages and synthetic state.
//! Behaviour: none (codec, fixture conformance or host-state contracts)

use crate::common::attack_notifications::*;

#[test]
fn allegiance_update_done_is_dropped_by_retail_and_needs_no_decoder() {
    let done = bodies_of(ALLEGIANCE_UPDATE_DONE);
    assert!(!done.is_empty());
    for b in &done {
        assert_eq!(
            b.bytes,
            vec![0u8; 4],
            "{} idx {}: ACE's `errorType`, four bytes and WeenieError::None -- not an empty body",
            b.scenario,
            b.idx
        );
    }
    assert!(
        dereth_protocol::Opcode(ALLEGIANCE_UPDATE_DONE)
            .name()
            .is_none(),
        "the opcode table is the client's switch arms; the client has none for `0x01C8`"
    );
    assert_eq!(
        bodies_of(0x0020).len(),
        done.len(),
        "one `0x01C8` per `0x0020 Allegiance_AllegianceUpdate`"
    );
}
