//! Shared isolated-world fixture for the area tests.

#![allow(unused_imports)]

pub(crate) use std::sync::Arc;
pub(crate) use std::time::Duration;

pub(crate) use empyrean_common::clock::{ClockSnapshot, VirtualClock};
pub(crate) use empyrean_common::dotnet::datetime::TimeSpan;
pub(crate) use empyrean_common::not_ported;
pub(crate) use empyrean_common::thread_safe_random::ThreadSafeRandom;
pub(crate) use empyrean_content::models::world::{
    Weenie as WeenieRow, WeeniePropertiesCreateList, WeeniePropertiesGenerator,
};
pub(crate) use empyrean_content::MemContent;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::{
    EquipMask, PhysicsState, PositionType, PropertyDataId, PropertyFloat, PropertyInt,
    PropertyInt64, PropertyString, WeenieType,
};
pub(crate) use empyrean_entity::{LandblockId, ObjectGuid, Position};
pub(crate) use empyrean_testkit::land;
pub(crate) use empyrean_world::entity::actions::delay_manager;
pub(crate) use empyrean_world::entity::timers::TimersState;
pub(crate) use empyrean_world::factories::world_object_factory as factory;
pub(crate) use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
pub(crate) use empyrean_world::managers::landblock_manager as lm;
pub(crate) use empyrean_world::managers::property_manager as pm;
pub(crate) use empyrean_world::physics::phys_ext;
pub(crate) use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
pub(crate) use empyrean_world::world_objects::{
    creature, creature_equipment, door, world_object_properties, world_object_tick as tick,
};
pub(crate) use empyrean_world::World;

// ------------------------------------------------------------------------------------ harness

pub(crate) use empyrean_testkit::EmptyShard;

pub(crate) const LB: u16 = 0xA9B4;
pub(crate) const GEN: u32 = 0x7A9B_4200;
pub(crate) const SEED: u64 = 1111;

pub(crate) const DRUDGE_WCID: u32 = 3000;
pub(crate) const SWORD_WCID: u32 = 3001;
pub(crate) const GEN_WCID: u32 = 3002;
pub(crate) const DOOR_WCID: u32 = 3003;
pub(crate) const GHOST_WCID: u32 = 3004;

pub(crate) fn setup_weenie(
    wcid: u32,
    class_name: &str,
    weenie_type: WeenieType,
    name: &str,
) -> WeenieRow {
    WeenieRow::new(wcid, class_name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, land::TEST_SETUP)
}

/// A creature whose create list wields one sword (`DestinationType.Wield`, no treasure set); its
/// pack capacities are the world database's for creatures (-1).
pub(crate) fn drudge() -> WeenieRow {
    let mut d = setup_weenie(DRUDGE_WCID, "drudge", WeenieType::Creature, "Drudge")
        .with_int(PropertyInt::ItemsCapacity, -1)
        .with_int(PropertyInt::ContainersCapacity, -1);
    d.weenie_properties_create_list = vec![WeeniePropertiesCreateList {
        id: 1,
        object_id: DRUDGE_WCID,
        destination_type: 2,
        weenie_class_id: SWORD_WCID,
        stack_size: 0,
        palette: 0,
        shade: 0.0,
        try_to_bond: false,
    }];
    d
}

pub(crate) fn sword() -> WeenieRow {
    setup_weenie(SWORD_WCID, "sword", WeenieType::MeleeWeapon, "Sword").with_int(
        PropertyInt::ValidLocations,
        EquipMask::MeleeWeapon.0.cast_signed(),
    )
}

/// A generator on the test landblock whose one profile spawns a drudge.
pub(crate) fn generator() -> WeenieRow {
    let mut g = setup_weenie(GEN_WCID, "gen", WeenieType::Generic, "gen")
        .with_int(PropertyInt::InitGeneratedObjects, 1)
        .with_int(PropertyInt::MaxGeneratedObjects, 1)
        .with_float(PropertyFloat::RegenerationInterval, 60.0)
        .with_position(
            PositionType::Location,
            u32::from(LB) << 16 | 0x0019,
            [84.0, 7.1, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        );
    g.weenie_properties_generator = vec![WeeniePropertiesGenerator {
        id: 1,
        object_id: GEN_WCID,
        probability: -1.0,
        weenie_class_id: DRUDGE_WCID,
        init_create: 1,
        max_create: 1,
        ..Default::default()
    }];
    g
}

/// A door with no motion table (its animations take no time).
pub(crate) fn door() -> WeenieRow {
    setup_weenie(DOOR_WCID, "door", WeenieType::Door, "Door")
}

/// A weenie whose `PhysicsState` is Static | Ethereal | ReportCollisions | Gravity (0x40D).
pub(crate) fn ghost() -> WeenieRow {
    setup_weenie(GHOST_WCID, "ghost", WeenieType::Generic, "Ghost")
        .with_int(PropertyInt::PhysicsState, 0x40D)
}

pub(crate) fn content() -> MemContent {
    MemContent::new()
        .weenie(drudge())
        .weenie(sword())
        .weenie(generator())
        .weenie(door())
        .weenie(ghost())
}

pub(crate) struct H {
    pub(crate) w: World,
    pub(crate) clock: VirtualClock,
}

impl H {
    pub(crate) fn new() -> Self {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let now = ClockSnapshot::take(&clock, timers.portal_year_ticks);
        let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats");
        let mut w = World::new(now, dats);
        w.timers = timers;
        w.content = Arc::new(content());
        gm::initialize(&mut w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(&mut w, &[LB], 0);
        pm::initialize(&mut w, true);
        ThreadSafeRandom::seed(SEED);
        lm::get_landblock(
            &mut w,
            LandblockId::new(u32::from(LB) << 16 | 0xFFFF),
            false,
            false,
        );
        H { w, clock }
    }

    pub(crate) fn tick(&mut self) {
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
        delay_manager::run_actions(&mut self.w);
        let pyt = self.w.timers.portal_year_ticks;
        lm::tick(&mut self.w, pyt);
    }

    pub(crate) fn advance(&mut self, secs: f64) {
        let d = Duration::from_secs_f64(secs);
        self.clock.advance(d);
        empyrean_world::entity::timers::advance_portal_year_ticks(
            &mut self.w,
            TimeSpan::from_ticks(i64::try_from(d.as_nanos() / 100).unwrap()),
        );
        self.w.now = ClockSnapshot::take(&self.clock, self.w.timers.portal_year_ticks);
    }

    pub(crate) fn o(&self, g: ObjectGuid) -> &WorldObject {
        self.w.objects.get(g).expect("object in store")
    }

    /// A new object of `wcid` in the store and on the landblock (at its own location or 20, 20).
    pub(crate) fn place_new(&mut self, wcid: u32, guid: u32) -> ObjectGuid {
        let g = ObjectGuid::new(guid);
        let mut o = CtorEnv::with_world(&self.w, |env| {
            factory::create_new_world_object_by_wcid(env, wcid, g)
        })
        .expect("weenie exists");
        if o.location().is_none() {
            o.set_location(Some(Position::from_components(
                u32::from(LB) << 16 | 0x0001,
                20.0,
                20.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                false,
            )));
        }
        self.w.objects.insert(o).expect("fresh guid");
        creature::post_insert(&mut self.w, g);
        assert!(
            lm::add_object(&mut self.w, g, false),
            "placed on the landblock"
        );
        g
    }

    /// The generator's first `GeneratorUpdate` and `GeneratorRegeneration` (its initial spawn).
    pub(crate) fn regenerate(&mut self, g: ObjectGuid) {
        let now = self.w.now.unix_time;
        tick::generator_update(&mut self.w, g, now);
        tick::generator_regeneration(&mut self.w, g, now);
    }

    pub(crate) fn spawned(&self, g: ObjectGuid) -> Vec<ObjectGuid> {
        self.o(g).wo.world_object_generators.generator_profiles[0]
            .spawned
            .keys()
            .map(|&k| ObjectGuid::new(k))
            .collect()
    }
}

// ------------------------------------------------------------------------------ post-insert

// ------------------------------------------------------------------------ SetPhysicsState

pub(crate) fn body_has(h: &H, g: ObjectGuid, flag: PhysicsState) -> bool {
    let body = phys_ext::physics_obj(&h.w, g).expect("a body");
    phys_ext::state(&h.w, body).contains(flag)
}

// ------------------------------------------------------------------------------ item level

// --------------------------------------------------------------------------- real content

#[cfg(feature = "real-content")]
pub(crate) mod real_content {
    //! The retail dats (`DERETH_TEST_DAT_DIR`) and `world.pack` (`EMPYREAN_TEST_WORLD_PACK`): a Holtburg-area
    //! drudge (the encounter generator 2007 on 0xA8B3, south-west of Holtburg) spawned by its
    //! generator.

    pub(crate) use std::path::PathBuf;

    pub(crate) use empyrean_content::PackContent;
    pub(crate) use empyrean_dat::{DatManager, RealDats};
    pub(crate) use empyrean_entity::enums::{PropertyBool, WeenieType as Wt};
    pub(crate) use empyrean_world::world_objects::kinds::KindData;
    pub(crate) use empyrean_world::world_objects::{monster_awareness, monster_combat};

    pub(crate) use super::*;

    pub(crate) const HOME: u16 = 0xA8B3;
    pub(crate) const PLAYER: u32 = 0x5000_0001;

    pub(crate) fn dats() -> Arc<DatManager> {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        DatManager::initialize(Arc::new(source)).expect("retail dats")
    }

    pub(crate) fn pack() -> PackContent {
        let path = empyrean_common::test_paths::world_pack();
        PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        })
    }

    pub(crate) fn world() -> H {
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let mut w = World::new(
            ClockSnapshot::take(&clock, timers.portal_year_ticks),
            dats(),
        );
        w.timers = timers;
        w.content = Arc::new(pack());
        gm::initialize(&mut w, &mut EmptyShard);
        pm::initialize(&mut w, true);
        ThreadSafeRandom::seed(SEED);
        H { w, clock }
    }

    /// A human player body (the character generator's first heritage) standing at `(x, y)` on
    /// `HOME`, attackable, out of the login pink bubble, with a session.
    pub(crate) fn player(h: &mut H, x: f32, y: f32) -> ObjectGuid {
        let w = &mut h.w;
        let char_gen = w.dats.portal_dat().char_gen();
        let setup = char_gen
            .heritage_groups
            .values()
            .next()
            .and_then(|g| g.sexes.values().next())
            .map(|s| s.setup.0)
            .expect("a body");
        let g = ObjectGuid::new(PLAYER);
        let mut o = WorldObject {
            guid: g,
            container: Some(Box::default()),
            creature: Some(Box::default()),
            player: Some(Box::default()),
            kind: KindData::Player,
            ..Default::default()
        };
        o.biota.id = PLAYER;
        o.biota.weenie_type = Wt::Creature;
        use empyrean_common::dotnet::DotNetDict;
        o.biota.properties_enchantment_registry = Some(Vec::new());
        o.biota.properties_attribute = Some(DotNetDict::new());
        o.biota.properties_attribute_2nd = Some(DotNetDict::new());
        o.biota.properties_skill = Some(DotNetDict::new());
        empyrean_world::world_objects::creature_vitals::set_ephemeral_stat_values(w, &mut o);
        o.set_property(PropertyDataId::Setup, setup);
        o.set_property(PropertyDataId::MotionTable, 0x0900_0001);
        o.set_property(PropertyString::Name, "Walker".to_owned());
        o.set_property(PropertyBool::Attackable, true);
        o.player.as_mut().unwrap().player.character =
            Some(empyrean_store::models::shard::Character::default());
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // a cell index from a position on the block
        let cell = u32::from(HOME) << 16 | (((x / 24.0) as u32) * 8 + (y / 24.0) as u32 + 1);
        let z = empyrean_world::entity::position_extensions::get_terrain_z(
            w,
            &Position::from_components(cell, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false),
        );
        o.set_location(Some(Position::from_components(
            cell,
            x,
            y,
            z + 0.005,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
        o.set_heartbeat_interval(Some(5.0));
        tick::world_object_initialize_heartbeats(&mut o, w.now.unix_time);
        w.objects.insert(o).expect("fresh");
        w.sessions.insert(
            empyrean_net::SessionId {
                client_id: 1,
                generation: 1,
            },
            empyrean_world::sessions::SessionData {
                player: Some(g),
                ..Default::default()
            },
        );
        assert!(
            lm::add_object(w, g, false),
            "the player joins the landblock"
        );
        phys_ext::set_physics_state(w, g, PhysicsState::Hidden, Some(false));
        g
    }

    pub(crate) fn drudges(h: &H) -> Vec<ObjectGuid> {
        let lb = LandblockId::new(u32::from(HOME) << 16 | 0xFFFF);
        let Some(l) = h.w.landblock_manager.landblocks.get(lb) else {
            return Vec::new();
        };
        l.world_object_guids()
            .copied()
            .filter(|&g| {
                h.w.objects.get(g).is_some_and(|o| {
                    o.is_creature()
                        && !o.is_player()
                        && o.get_property(PropertyString::Name)
                            .is_some_and(|n| n.contains("Drudge"))
                })
            })
            .collect()
    }

    pub(crate) fn distance(h: &H, a: ObjectGuid, b: ObjectGuid) -> f32 {
        let (pa, pb) = (h.o(a).location().unwrap(), h.o(b).location().unwrap());
        ((pa.position_x - pb.position_x).powi(2) + (pa.position_y - pb.position_y).powi(2)).sqrt()
    }

    pub(crate) const ACADEMY: u16 = 0x7203;
    pub(crate) const SENIOR_GUARD: u32 = 29318;
    pub(crate) const SINGULARITY_SWORD: u32 = 27849;
}
