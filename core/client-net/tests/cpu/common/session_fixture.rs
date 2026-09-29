//! Shared fixtures for session replay.

#![allow(dead_code, unused_imports)]

pub(crate) use crate::common::*;
pub(crate) use dereth_client_net::client_session::testing::{Corpus, Direction, Unmodelled};
pub(crate) use dereth_client_net::client_session::{DisconnectReason, SessionEvent, SessionState};
pub(crate) use dereth_primitives::{NetBlobId, NetQueue, ObjectId};
pub(crate) use dereth_protocol::login::{
    CharacterIdentity, LoginCharacterSet, LoginEnterGameServerReady, LoginWorldInfo,
};
pub(crate) use dereth_protocol::qualities::{PrivateUpdate, QualitiesPrivateUpdateInt};
pub(crate) use dereth_protocol::{events::pack_event, write_blob, Message, Opcode};

pub(crate) const DRIVABLE: &[u32] = &[
    0xF7C8, 0xF7E6, 0xF653, 0xF655, 0xF656, 0xF657, 0xF6EA, 0xF7D9, 0xF7DE, 0xF7EA,
];

pub(crate) fn drivable(b: &dereth_client_net::client_session::testing::CorpusBlob) -> bool {
    if b.opcode == dereth_protocol::OrderedActionHeader::MAGIC {
        return b
            .payload
            .get(8..12)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
            .is_some_and(|sub| {
                dereth_client_net::client_session::testing::DRIVABLE_ACTIONS.contains(&sub)
            });
    }
    DRIVABLE.contains(&b.opcode)
}

pub(crate) fn character_set() -> LoginCharacterSet {
    LoginCharacterSet {
        status: 0,
        characters: vec![CharacterIdentity {
            gid: ObjectId(0x5000_0001),
            name: "Lark".into(),
            seconds_greyed_out: 0,
        }],
        deleted: vec![],
        num_allowed_characters: 5,
        account: "ac01".into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 0,
    }
}
