//! Shared isolated-world fixture for the area tests.

#![allow(unused_imports)]

pub(crate) use std::panic::{catch_unwind, AssertUnwindSafe};

pub(crate) use empyrean_common::thread_safe_random::ThreadSafeRandom;
pub(crate) use empyrean_entity::enums::{
    EmoteCategory, EmoteType, PropertyDataId, PropertyFloat, PropertyInt, PropertyInt64,
    PropertyString,
};
pub(crate) use empyrean_entity::{ObjectGuid, Position};
pub(crate) use empyrean_testkit::land;
pub(crate) use empyrean_world::dispatch::{self, Class};
pub(crate) use empyrean_world::entity::cloak;
pub(crate) use empyrean_world::managers::landblock_manager as lm;
pub(crate) use empyrean_world::managers::property_manager as pm;
pub(crate) use empyrean_world::network::game_messages::game_message::start_capture;
pub(crate) use empyrean_world::world_objects::hotspot;
pub(crate) use empyrean_world::world_objects::managers::emote_manager as em;
pub(crate) use empyrean_world::world_objects::world_object::WorldObject;
pub(crate) use empyrean_world::World;

pub(crate) use crate::content::emotes::{act, sent, set, H, NPC, P1};

pub(crate) const LB: u32 = 0xA9B4_0000;

/// A bare object of `class` in the store, standing on the test landblock.
pub(crate) fn object(w: &mut World, class: Class, guid: u32) -> ObjectGuid {
    let g = ObjectGuid::new(guid);
    let mut o = WorldObject::allocate(class);
    o.guid = g;
    o.biota.id = guid;
    o.set_property(PropertyString::Name, "Thing".to_owned());
    o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    o.set_location(Some(Position::from_components(
        LB | 0x0001,
        24.0,
        24.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )));
    w.objects.insert(o).expect("fresh guid");
    g
}

pub(crate) fn o(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).expect("live object")
}

// ------------------------------------------------------------------------------ V345 (V345)

// ------------------------------------------------------------------------------ V340 (V340)

/// A player's corpse at `guid`, with `time_to_rot` left.
pub(crate) fn corpse(w: &mut World, guid: u32, created: i32, time_to_rot: f64) -> ObjectGuid {
    let g = object(w, Class::Corpse, guid);
    let c = o(w, g);
    c.set_level(Some(10));
    c.set_victim_id(Some(P1.full()));
    c.set_creation_timestamp(Some(created));
    c.set_time_to_rot(Some(time_to_rot));
    g
}

// ------------------------------------------------------------------------------ V341 (V341)

// ------------------------------------------------------------------------------ V342 (V342)
