//! Behaviour: none (a host inside another program: its C interface's own contract)
//!
//! The C interface's contract with the Decal plugin: its version, settings that do not read making
//! no host, a null host doing nothing, a typed line kept from the client only when a plugin claims
//! it, and every message of a known type the client receives read on the queue the opcode table
//! names for it (Decal hands over a message without its queue; one of a type nobody knows is
//! dropped, as the client drops it).
#![allow(unsafe_code)]

use dereth_client_sdk::net::client_session::testing::{Corpus, Direction};
use dereth_client_sdk::primitives::NetQueue;
use dereth_client_sdk::protocol::Opcode;
use dereth_decal_host::{ffi, host::queue_of};

use crate::abi::{plugin_dir, settings, Replay};

#[test]
fn the_interface_says_its_version() {
    assert_eq!(ffi::dereth_host_abi(), ffi::ABI);
}

#[test]
fn settings_that_do_not_read_make_no_host_and_none_make_a_bare_one() {
    for bad in ["not json", "[1, 2]", "{\"plugins\": "] {
        // SAFETY: the text's bytes, read during the call.
        let h = unsafe { ffi::dereth_host_new(bad.as_ptr(), bad.len()) };
        assert!(h.is_null(), "{bad:?} makes no host");
    }
    // SAFETY: no bytes; the host made is freed once.
    unsafe {
        let h = ffi::dereth_host_new(std::ptr::null(), 0);
        assert!(!h.is_null(), "no settings make a host with none");
        ffi::dereth_host_free(h);
    }
}

#[test]
fn a_null_host_does_nothing() {
    let blob = [0xE0_u8, 0xF7, 0, 0];
    let mut len = 7;
    // SAFETY: every function takes a null host and does nothing with it.
    unsafe {
        ffi::dereth_host_server_message(std::ptr::null_mut(), blob.as_ptr(), blob.len(), 0.0);
        ffi::dereth_host_frame(std::ptr::null_mut(), 0.0);
        ffi::dereth_host_game_state(std::ptr::null_mut(), 1, 0);
        ffi::dereth_host_bindings(std::ptr::null_mut(), blob.as_ptr(), blob.len());
        assert_eq!(
            ffi::dereth_host_typed(std::ptr::null_mut(), blob.as_ptr(), 1),
            0
        );
        assert!(ffi::dereth_host_ops(std::ptr::null_mut(), &mut len).is_null());
        ffi::dereth_host_free(std::ptr::null_mut());
    }
    assert_eq!(len, 7, "nothing written");
}

#[test]
fn a_typed_line_is_kept_from_the_client_only_when_a_plugin_claims_it() {
    let mut r = Replay::frames_only(&settings(&[plugin_dir("probe")]));
    r.frame();
    assert!(r.dll.typed("/probe"), "the plugin's own command");
    assert!(r.dll.typed("@Probe now"), "spelled with @, any case");
    assert!(r.dll.typed("/plugin list"), "the host's own command");
    assert!(!r.dll.typed("/tell Bob, hi"), "the client's own command");
    assert!(!r.dll.typed("hello"), "a line said");
}

#[test]
fn every_message_the_client_receives_is_read_on_the_queue_the_opcode_table_names() {
    let drained = [
        NetQueue::Logon,
        NetQueue::ClientCache,
        NetQueue::UiQueue,
        NetQueue::WorldObjects,
    ];
    for corpus in Corpus::shared_all() {
        for b in &corpus.blobs {
            let known = b.opcode == 0xF7B0 || Opcode(b.opcode).info().is_some();
            if b.dir == Direction::ServerToClient && drained.contains(&b.queue) && known {
                assert_eq!(
                    queue_of(b.opcode),
                    Some(b.queue),
                    "{} blob {}: opcode {:#06X}",
                    corpus.name,
                    b.idx,
                    b.opcode
                );
            }
        }
    }
}
