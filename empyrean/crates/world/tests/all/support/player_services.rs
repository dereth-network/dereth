//! Shared isolated-world fixture for the area tests.

#![allow(unused_imports)]

pub(crate) use empyrean_entity::enums::{AllegiancePermissionLevel, PropertyBool, WeenieError};
pub(crate) use empyrean_entity::ObjectGuid;
pub(crate) use empyrean_world::network::game_messages::game_message::start_capture;

pub(crate) use crate::social::fellowship::{events_to, sent, H};

pub(crate) const A: ObjectGuid = ObjectGuid::new(0x5000_0001);
