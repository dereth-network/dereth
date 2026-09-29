//! Reading a raw recording: the shared readers, under the name this crate's callers use.
//!
//! **Nothing is read here.** The datagrams come from the reader beside the decoded corpus,
//! [`dereth_client_sdk::net::client_session::testing::capture`], and what only a datagram-header
//! parse can read -- the login request's connection sequence number, and each enter-world -- from
//! [`dereth_client_sdk::net::recording`]. This module re-exports both so that there is one reader
//! in the workspace rather than one per harness.

pub use dereth_client_sdk::net::client_session::testing::capture::{
    load, load_session, peer, shared_session, CaptureError, Datagram,
};
pub use dereth_client_sdk::net::recording::{
    connection_sequence_number, enter_world_requests, recorded_enter_world_requests, RecordedEntry,
};
