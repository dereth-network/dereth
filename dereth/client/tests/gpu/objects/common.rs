//! Fixtures the objects modules share: the recorded captures read through the workspace's one
//! capture reader, the list of captures on disk, a test device and the retail dat store.
//!
//! Behaviour: none (shared fixtures)

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use dereth_client_net::client_session::testing::capture;
use dereth_dat::RetailDatStore;

use crate::common::{captures_dir, is_unclean_logout_recording, recorded_sessions};

/// One recorded datagram: `t`, `c2s`, `pair` and `raw`, as the capture wrote it.
pub use capture::Datagram as Record;

/// Every datagram of the named capture, in recorded order. A missing or malformed capture is a
/// broken checkout and fails the test.
pub fn load(session: &str) -> Vec<Record> {
    let path = captures_dir().join(format!("{session}.jsonl"));
    capture::load(&path).unwrap_or_else(|e| panic!("{e}"))
}

/// The connection sequence number the capture's own login request carries, which a replay
/// endpoint is built with.
pub fn connection_sequence_number(records: &[Record]) -> u32 {
    dereth_client_net::recording::connection_sequence_number(records)
        .expect("the capture has no LoginRequest")
}

/// The address a recorded pair's datagrams arrive from.
pub fn addr(pair: u16) -> SocketAddr {
    capture::peer(pair)
}

/// Every capture on disk, discovered rather than named, less the two unclean-logout recordings;
/// as many as the corpus index records.
pub fn corpus_sessions() -> Vec<String> {
    let dir = captures_dir();
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| !is_unclean_logout_recording(&e.path()))
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                return None;
            }
            p.file_stem().and_then(|x| x.to_str()).map(str::to_owned)
        })
        .collect();
    out.sort();
    assert_eq!(
        out.len(),
        recorded_sessions(),
        "the recorded captures, found {out:?}"
    );
    out
}

/// The binary's common device constructor, preserving renderer test configuration.
pub use crate::common::test_gpu;

/// The retail dat store, shared. Missing dats are a broken fixture and fail the test.
pub fn retail_store() -> Arc<RetailDatStore> {
    crate::common::dats()
}
