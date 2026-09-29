//! The retail capture index: reads packet captures, reassembles and decodes every session, and
//! writes a queryable SQLite index.
//!
//! **Depends on** `dereth-primitives`, the transport (`dereth-transport`), which reassembles every
//! session, and the codecs (`dereth-protocol`), which decode every message. **Used by** nothing: it
//! is a tool, the `dereth-pcap` binary (`ingest`, `stats`, `query`, `triage`) over this library.
//!
//! **Must never** reassemble or decode with anything but the real transport and protocol crates:
//! the index records what this code makes of a recording, so a disagreement in it is a finding
//! about those crates.
//!
//! Captures (pcap, pcapng, and either inside zip or 7z archives) are read as streams of frames
//! ([`pcap`], [`archive`]), cut down to IPv4/UDP datagrams ([`link`]), grouped into client sessions
//! ([`flows`]), reassembled ([`transport`]) and decoded ([`decode`]). The result is a SQLite index
//! ([`index`]) written by [`ingest`], read back by [`report`] and, from tests, by [`corpus`];
//! `dereth-pcap query` runs SQL against the tables [`index`] creates.

pub mod ac2;
pub mod archive;
pub mod corpus;
pub mod debug_json;
pub mod decode;
pub mod flows;
pub mod game;
pub mod index;
pub mod ingest;
pub mod link;
pub mod pcap;
pub mod report;
pub mod transport;

#[cfg(test)]
mod registry_scan;
