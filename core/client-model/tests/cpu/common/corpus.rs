//! Shared fixtures for corpus.

#![allow(dead_code)] // each test binary uses a different subset

use dereth_client_net::client_session::testing::{Corpus, Direction};

#[derive(Debug, Clone)]
pub struct Blob {
    pub dir: Dir,
    pub t: f64,
    pub queue: u16,
    pub blob_id_high: u32,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    C2s,
    S2c,
}

impl Blob {
    #[must_use]
    pub fn opcode(&self) -> u32 {
        u32::from_le_bytes([
            self.payload[0],
            self.payload[1],
            self.payload[2],
            self.payload[3],
        ])
    }

    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.payload[4..]
    }

    #[must_use]
    pub fn as_event(&self) -> Option<(u32, u32, u32, &[u8])> {
        if self.opcode() != 0xF7B0 || self.payload.len() < 16 {
            return None;
        }
        let d = |o: usize| {
            u32::from_le_bytes([
                self.payload[o],
                self.payload[o + 1],
                self.payload[o + 2],
                self.payload[o + 3],
            ])
        };
        Some((d(4), d(8), d(12), &self.payload[16..]))
    }

    #[must_use]
    pub fn as_action(&self) -> Option<(u32, u32, &[u8])> {
        if self.opcode() != 0xF7B1 || self.payload.len() < 12 {
            return None;
        }
        let d = |o: usize| {
            u32::from_le_bytes([
                self.payload[o],
                self.payload[o + 1],
                self.payload[o + 2],
                self.payload[o + 3],
            ])
        };
        Some((d(4), d(8), &self.payload[12..]))
    }
}

#[must_use]
pub fn sessions() -> Vec<String> {
    dereth_client_net::client_session::testing::session_names()
        .iter()
        .map(|name| (*name).to_owned())
        .collect()
}

#[must_use]
pub fn load(session: &str) -> Vec<Blob> {
    Corpus::shared(session)
        .blobs
        .iter()
        .map(|b| Blob {
            dir: match b.dir {
                Direction::ClientToServer => Dir::C2s,
                Direction::ServerToClient => Dir::S2c,
            },
            t: b.t_rel_micros as f64 / 1_000_000.0,
            queue: u16::from(dereth_client_net::client_session::testing::queue_id(
                b.queue,
            )),
            blob_id_high: (b.blob_id >> 32) as u32,
            payload: b.payload.clone(),
        })
        .collect()
}

#[must_use]
pub fn load_all() -> Vec<(String, Vec<Blob>)> {
    sessions()
        .into_iter()
        .map(|name| {
            let blobs = load(&name);
            (name, blobs)
        })
        .collect()
}
