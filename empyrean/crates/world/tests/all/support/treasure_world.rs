//! Shared isolated-world fixture for the area tests.

#![allow(unused_imports)]

pub(crate) use std::sync::Arc;
pub(crate) use std::time::Duration;

pub(crate) use dereth_assets::tables::SpellBase;
pub(crate) use dereth_assets::SpellTable;
pub(crate) use dereth_primitives::DataId;
pub(crate) use empyrean_common::clock::{ClockSnapshot, VirtualClock};
pub(crate) use empyrean_common::dotnet::datetime::TimeSpan;
pub(crate) use empyrean_common::thread_safe_random::ThreadSafeRandom;
pub(crate) use empyrean_content::models::world::{
    Spell as DbSpell, TreasureDeath, Weenie as WeenieRow, WeeniePropertiesGenerator,
};
pub(crate) use empyrean_content::MemContent;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::{
    DamageType, EnchantmentTypeFlags as F, EquipMask, ImbuedEffectType, PositionType,
    PropertyAttribute2nd, PropertyDataId, PropertyFloat, PropertyInt, PropertyString,
    RegenLocationType, Skill, WeenieType,
};
pub(crate) use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
pub(crate) use empyrean_entity::models::PropertiesSkill;
pub(crate) use empyrean_entity::{LandblockId, ObjectGuid, Position};
pub(crate) use empyrean_testkit::land;
pub(crate) use empyrean_world::dispatch::Class;
pub(crate) use empyrean_world::entity::actions::delay_manager;
pub(crate) use empyrean_world::entity::damage_history::{self, DamageHistory};
pub(crate) use empyrean_world::entity::spell::Spell;
pub(crate) use empyrean_world::entity::timers::TimersState;
pub(crate) use empyrean_world::factories::loot_generation_factory as lgf;
pub(crate) use empyrean_world::factories::world_object_factory as factory;
pub(crate) use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
pub(crate) use empyrean_world::managers::landblock_manager as lm;
pub(crate) use empyrean_world::managers::property_manager as pm;
pub(crate) use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
pub(crate) use empyrean_world::world_objects::managers::enchantment_manager_with_caching as emc;
pub(crate) use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
pub(crate) use empyrean_world::world_objects::{
    creature_death, creature_equipment, creature_magic, world_object_tick as tick,
};
pub(crate) use empyrean_world::World;

// ------------------------------------------------------------------------------------ harness

pub(crate) use empyrean_testkit::EmptyShard;

pub(crate) const LB: u16 = 0xA9B4;
pub(crate) const MONSTER: ObjectGuid = ObjectGuid::new(0x7A9B_4100);
pub(crate) const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);
pub(crate) const GEN: u32 = 0x7A9B_4200;
pub(crate) const SEED: u64 = 4242;

pub(crate) const CORPSE_WCID: u32 = 21;
pub(crate) const COINSTACK_WCID: u32 = 273; // WeenieClassName.coinstack
pub(crate) const STACK_WCID: u32 = 2002;
pub(crate) const GEN_WCID: u32 = 1000;
pub(crate) const STACK_GEN_WCID: u32 = 1001;
/// A `treasure_death` table DID: tier 1, a certain mundane group of exactly two items, mundane
/// profile 6 (pyreals only); no item or magic item group.
pub(crate) const PYREALS_DID: u32 = 9001;
pub(crate) const WARD_SPELL: u32 = 1;

pub(crate) fn pyreal_profile() -> TreasureDeath {
    TreasureDeath {
        id: 1,
        treasure_type: PYREALS_DID,
        tier: 1,
        mundane_item_chance: 100,
        mundane_item_min_amount: 2,
        mundane_item_max_amount: 2,
        mundane_item_type_selection_chances: 6,
        ..TreasureDeath::default()
    }
}

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

/// A generator standing on the test landblock with one profile row.
pub(crate) fn generator(wcid: u32, row: WeeniePropertiesGenerator) -> WeenieRow {
    let mut g = setup_weenie(wcid, "gen", WeenieType::Generic, "gen")
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
        object_id: wcid,
        ..row
    }];
    g
}

/// A one-hour armour ward (dat and database halves), for the heartbeat test.
pub(crate) fn spell_table() -> SpellTable {
    let ward = SpellBase {
        name: "Ward".to_owned(),
        description: String::new(),
        school: 3,
        icon: 0,
        category: 1,
        bitfield: 0x4,
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 1,
        spell_economy_mod: 1.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 1,
        meta_spell_id: WARD_SPELL,
        duration: Some((20.0, 0.0, 0.0)),
        portal_lifetime: None,
        raw_comps: [0; 8],
        comp_key: 0,
        comps: Vec::new(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0,
        mana_mod: 0,
    };
    SpellTable {
        id: DataId(0x0E00_000E),
        spell_buckets: 64,
        spells: [(WARD_SPELL, ward)].into_iter().collect(),
        spellset_bucket_index: 1,
        spellsets: std::collections::BTreeMap::new(),
    }
}

pub(crate) fn content() -> MemContent {
    let treasure = WeeniePropertiesGenerator {
        probability: -1.0,
        weenie_class_id: PYREALS_DID,
        init_create: 1,
        max_create: 1,
        // Treasure, placed by Spawn_Default.
        where_create: RegenLocationType::Treasure.0,
        ..Default::default()
    };
    let stack = WeeniePropertiesGenerator {
        probability: -1.0,
        weenie_class_id: STACK_WCID,
        init_create: 1,
        max_create: 1,
        stack_size: Some(5),
        ..Default::default()
    };
    MemContent::new()
        .weenie(setup_weenie(
            CORPSE_WCID,
            "corpse",
            WeenieType::Corpse,
            "Corpse",
        ))
        .weenie(
            setup_weenie(COINSTACK_WCID, "coinstack", WeenieType::Coin, "Pyreal")
                .with_int(PropertyInt::StackUnitValue, 1)
                .with_int(PropertyInt::StackUnitEncumbrance, 0)
                .with_int(PropertyInt::MaxStackSize, 25000),
        )
        .weenie(
            setup_weenie(STACK_WCID, "trinket", WeenieType::Stackable, "Trinket")
                .with_int(PropertyInt::StackUnitValue, 3)
                .with_int(PropertyInt::StackUnitEncumbrance, 2)
                .with_int(PropertyInt::MaxStackSize, 100),
        )
        .weenie(generator(GEN_WCID, treasure))
        .weenie(generator(STACK_GEN_WCID, stack))
        .treasure_death(pyreal_profile())
        .spell(DbSpell {
            id: WARD_SPELL,
            name: "Ward".to_owned(),
            stat_mod_type: Some((F::Float.0 | F::SingleStat.0 | F::Additive.0).cast_unsigned()),
            stat_mod_key: Some(u32::from(PropertyFloat::ArmorModVsSlash.0)),
            stat_mod_val: Some(0.5),
            ..DbSpell::default()
        })
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
            .with_spell_table(spell_table())
            .build()
            .expect("fake dats");
        let mut w = World::new(now, dats);
        w.timers = timers;
        w.content = Arc::new(content());
        gm::initialize(&mut w, &mut EmptyShard);
        land::use_flat_land_with_test_setup(&mut w, &[LB], 0);
        pm::initialize(&mut w, true);
        ThreadSafeRandom::seed(SEED);
        H { w, clock }
    }

    /// The delay manager, then `LandblockManager.Tick` (the world loop's landblock half).
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

    pub(crate) fn o_mut(&mut self, g: ObjectGuid) -> &mut WorldObject {
        self.w.objects.get_mut(g).expect("object in store")
    }

    /// A creature built as the constructor would (live vitals, DamageHistory), on the landblock's
    /// cell, not yet placed; no heartbeats.
    pub(crate) fn creature(&mut self, class: Class, guid: ObjectGuid, name: &str) {
        let mut o = WorldObject::allocate(class);
        o.guid = guid;
        o.biota.id = guid.full();
        o.biota.properties_enchantment_registry = Some(Vec::new());
        let mut vitals = empyrean_common::dotnet::DotNetDict::new();
        for v in [
            PropertyAttribute2nd::MaxHealth,
            PropertyAttribute2nd::MaxStamina,
            PropertyAttribute2nd::MaxMana,
        ] {
            vitals.insert(
                v,
                PropertiesAttribute2nd {
                    init_level: 100,
                    level_from_cp: 0,
                    cp_spent: 0,
                    current_level: 100,
                },
            );
        }
        o.biota.properties_attribute_2nd = Some(vitals);
        for v in [
            PropertyAttribute2nd::MaxHealth,
            PropertyAttribute2nd::MaxStamina,
            PropertyAttribute2nd::MaxMana,
        ] {
            let cv = CreatureVital::new(&mut o, v);
            o.vitals_mut().insert(v, cv);
        }
        o.set_property(PropertyString::Name, name.to_owned());
        o.set_property(PropertyDataId::Setup, land::TEST_SETUP);
        o.creature.as_mut().unwrap().creature_death.damage_history =
            DamageHistory::new(guid, self.w.now.utc);
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
        o.set_heartbeat_interval(Some(0.0));
        tick::world_object_initialize_heartbeats(&mut o, self.w.now.unix_time);
        self.w.objects.insert(o).expect("fresh guid");
    }

    /// `WorldObjectFactory.CreateNewWorldObject(wcid)` with a fixed guid, on its landblock.
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

    pub(crate) fn load_landblock(&mut self) {
        lm::get_landblock(
            &mut self.w,
            LandblockId::new(u32::from(LB) << 16 | 0xFFFF),
            false,
            false,
        );
    }
}

/// `(wcid, StackSize, Value, EncumbranceVal)` of an item.
pub(crate) type Stack = (u32, Option<i32>, Option<i32>, Option<i32>);

/// Each item's [`Stack`], in order.
pub(crate) fn stacks<'a>(items: impl IntoIterator<Item = &'a WorldObject>) -> Vec<Stack> {
    items
        .into_iter()
        .map(|o| {
            (
                o.biota.weenie_class_id,
                o.stack_size(),
                o.value(),
                o.encumbrance_val(),
            )
        })
        .collect()
}

/// What factory makes for [`pyreal_profile`] on a fresh world whose RNG is at the same
/// point: `SEED`, then `before` (the draws the caller makes first, on that world).
pub(crate) fn factory_replay(before: impl FnOnce(&mut World)) -> Vec<Stack> {
    let mut h = H::new();
    before(&mut h.w);
    let items = lgf::create_random_loot_objects(&mut h.w, &pyreal_profile());
    stacks(items.iter())
}

// ------------------------------------------------------------------------------------ corpses

/// `PlayerFactoryEx.Create275*` over the real world database and dats: each template's twelve
/// weapons come from generators (`CreateMeleeWeapon` kept when heavy, `CreateMissileWeapon`,
/// `CreateCaster` kept when an elemental war wand), each with `AddRend`'s imbue. The heavy
/// template (Strength 100) carries all twelve; the other two (Strength 10) only what their burden
/// allows, as `Container.TryAddToInventory` checks it in ACE too.
#[cfg(feature = "real-content")]
pub(crate) mod real_content {
    pub(crate) use std::path::PathBuf;

    pub(crate) use empyrean_content::PackContent;
    pub(crate) use empyrean_dat::{DatManager, RealDats};
    pub(crate) use empyrean_world::factories::player_factory_ex;
    pub(crate) use empyrean_world::managers::property_manager::default_property_manager as dpm;

    pub(crate) use super::*;

    pub(crate) fn real_world() -> World {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        let dats = DatManager::initialize(Arc::new(source)).expect("retail dats");
        let path = empyrean_common::test_paths::world_pack();
        let pack = PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        });
        let clock = VirtualClock::default();
        let now = ClockSnapshot::take(&clock, 0.0);
        let mut w = World::new(now, dats);
        dpm::load_default_properties(&w.property_manager);
        w.content = Arc::new(pack);
        gm::initialize(&mut w, &mut EmptyShard);
        w
    }

    /// The imbued weapons among the template's possessions: `(wcid, ImbuedEffect)`.
    pub(crate) fn rended(
        p: &empyrean_world::factories::player_factory::CreatedPlayer,
    ) -> Vec<(u32, i32)> {
        p.inventory
            .iter()
            .filter_map(|o| {
                o.get_property(PropertyInt::ImbuedEffect)
                    .map(|e| (o.biota.weenie_class_id, e))
            })
            .collect()
    }
}
