//! Shared isolated-world fixture for the area tests.

#![allow(unused_imports)]

pub(crate) use dereth_primitives::Vec3;
pub(crate) use dereth_protocol::login::CharacterLoginCompleteNotification;
pub(crate) use empyrean_common::clock::ClockSnapshot;
pub(crate) use empyrean_common::dotnet::datetime::DotNetDateTime;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::{
    CombatMode, ImbuedEffectType, PropertyFloat, PropertyInt, Skill,
};
pub(crate) use empyrean_entity::{ObjectGuid, Position};
pub(crate) use empyrean_world::dispatch;
pub(crate) use empyrean_world::managers::landblock_manager as lm;
pub(crate) use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
pub(crate) use empyrean_world::physics::phys_ext;
pub(crate) use empyrean_world::world_objects::world_object::{self as wo, WorldObject};
pub(crate) use empyrean_world::world_objects::world_object_networking::shims;
pub(crate) use empyrean_world::world_objects::{
    creature_combat, creature_navigation, monster_awareness, player_location,
};
pub(crate) use empyrean_world::World;

pub(crate) use crate::monsters::monster_ai::{lb_id, H, MONSTER, PLAYER};
pub(crate) use crate::movement::player_movement::{action, body, g, obj, spawn_portal, world};

pub(crate) fn bare_world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: std::time::Duration::ZERO,
    };
    World::new(now, FakeDats::new().build().expect("empty fake dats"))
}

pub(crate) fn at(g: ObjectGuid, cell: u32, x: f32, y: f32) -> WorldObject {
    let mut o = WorldObject {
        guid: g,
        ..Default::default()
    };
    o.set_location(Some(Position::from_components(
        cell, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false,
    )));
    o
}
