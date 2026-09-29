//! Shared virtual-time server fixture and message helpers.

#![allow(unused_imports)]

pub(crate) use dereth_protocol::comms::{CommunicationTalk, CommunicationTransientString};
pub(crate) use dereth_protocol::login::CharacterError;
pub(crate) use empyrean_entity::ObjectGuid;

pub(crate) use crate::support::object_use_world::{
    all, at, during, in_pack, join, server, ALPHA, WAND,
};

pub(crate) const TRANSIENT: u32 = 0x02EB;
pub(crate) const PUBLIC_BOOL: u32 = 0x02D2;
pub(crate) const SOUND: u32 = 0xF750;

/// ACE's `CharacterError.ServerCrash1`.
pub(crate) const SERVER_CRASH_1: u32 = 0x4;
