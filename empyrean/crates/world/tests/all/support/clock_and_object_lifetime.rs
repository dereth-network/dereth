//! Shared isolated-world fixture for the area tests.

#![allow(unused_imports)]

pub(crate) use std::sync::Arc;
pub(crate) use std::time::Duration;

pub(crate) use empyrean_common::clock::{Clock, ClockSnapshot, VirtualClock};
pub(crate) use empyrean_common::dotnet::datetime::DotNetDateTime;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::WeenieType;
pub(crate) use empyrean_entity::{Biota, LandblockId, ObjectGuid};
pub(crate) use empyrean_world::dispatch::Class;
pub(crate) use empyrean_world::entity::offline_player::OfflinePlayer;
pub(crate) use empyrean_world::entity::timers::TimersState;
pub(crate) use empyrean_world::managers::player_manager;
pub(crate) use empyrean_world::managers::server_performance_monitor as perf;
pub(crate) use empyrean_world::world_objects::world_object::{self, WorldObject};
pub(crate) use empyrean_world::world_objects::{player, player_combat};
pub(crate) use empyrean_world::World;

pub(crate) const PLAYER: u32 = 0x5000_0001;

pub(crate) fn world() -> World {
    let clock = VirtualClock::default();
    let timers = TimersState::new(&clock);
    let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
    let mut w = World::new(
        now,
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .unwrap(),
    );
    w.timers = timers;
    w
}

pub(crate) fn object(
    w: &mut World,
    guid: u32,
    class: Class,
    weenie_type: WeenieType,
) -> ObjectGuid {
    let mut o = WorldObject::allocate(class);
    o.guid = ObjectGuid::new(guid);
    o.biota = Biota {
        id: guid,
        weenie_class_id: 50,
        weenie_type,
        ..Default::default()
    };
    o.biota.properties_enchantment_registry = Some(Vec::new());
    w.objects.insert(o).unwrap();
    ObjectGuid::new(guid)
}

pub(crate) fn o(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap()
}

/// A clock whose stopwatch moves 1 ms each time it is read, so every timed section is non-zero.
#[derive(Debug, Default)]
pub(crate) struct SteppingClock(std::sync::Mutex<Duration>);

impl Clock for SteppingClock {
    fn utc_now(&self) -> DotNetDateTime {
        DotNetDateTime::new_hms(2026, 9, 23, 12, 0, 0)
    }

    fn monotonic(&self) -> Duration {
        let mut t = self.0.lock().unwrap();
        *t += Duration::from_millis(1);
        *t
    }
}
