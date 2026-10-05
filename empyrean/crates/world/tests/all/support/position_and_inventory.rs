//! Shared isolated-world fixture for the area tests.

#![allow(unused_imports)]

pub(crate) use std::collections::BTreeMap;
pub(crate) use std::sync::Arc;

pub(crate) use dereth_assets::tables::{SpellBase, SpellSet, SpellTable};
pub(crate) use dereth_primitives::DataId;
pub(crate) use empyrean_common::clock::{ClockSnapshot, VirtualClock};
pub(crate) use empyrean_content::models::world::{Weenie as WeenieRow, WeeniePropertiesCreateList};
pub(crate) use empyrean_content::MemContent;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::{
    EquipmentSet, PropertyDataId, PropertyInt, PropertyString, WeenieType,
};
pub(crate) use empyrean_entity::{LandblockId, ObjectGuid, Position, Quaternion, Vector3};
pub(crate) use empyrean_testkit::land;
pub(crate) use empyrean_world::entity::position_extensions;
pub(crate) use empyrean_world::entity::timers::TimersState;
pub(crate) use empyrean_world::factories::world_object_factory as factory;
pub(crate) use empyrean_world::managers::event_manager::EventManagerState;
pub(crate) use empyrean_world::managers::guid_manager::{self as gm, ShardGuidQueries};
pub(crate) use empyrean_world::managers::landblock_manager as lm;
pub(crate) use empyrean_world::world_objects::world_object::WorldObject;
pub(crate) use empyrean_world::world_objects::{container, world_object_set};
pub(crate) use empyrean_world::World;

pub(crate) use empyrean_testkit::EmptyShard;

pub(crate) const LB: u16 = 0xA9B4;
pub(crate) const SACK: u32 = 4000;
pub(crate) const COIN: u32 = 4001;
pub(crate) const UNDEF: u32 = 4002;
pub(crate) const DRUDGE: u32 = 4003;

pub(crate) fn row(wcid: u32, class_name: &str, weenie_type: WeenieType) -> WeenieRow {
    WeenieRow::new(wcid, class_name, weenie_type)
        .with_string(PropertyString::Name, class_name)
        .with_did(PropertyDataId::Setup, land::TEST_SETUP)
}

pub(crate) fn contain(
    object_id: u32,
    destination_type: i8,
    wcid: u32,
    stack_size: i32,
) -> WeeniePropertiesCreateList {
    WeeniePropertiesCreateList {
        id: 1,
        object_id,
        destination_type,
        weenie_class_id: wcid,
        stack_size,
        palette: 0,
        shade: 0.0,
        try_to_bond: false,
    }
}

/// A sack whose create list holds 5 coins (`Contain`) and a missing weenie; a creature with the
/// same list (creatures never generate a contain list in the constructor).
pub(crate) fn content() -> MemContent {
    let mut sack =
        row(SACK, "sack", WeenieType::Container).with_int(PropertyInt::ItemsCapacity, 24);
    sack.weenie_properties_create_list =
        vec![contain(SACK, 1, COIN, 5), contain(SACK, 1, 99_999, 1)];
    let mut drudge = row(DRUDGE, "drudge", WeenieType::Creature);
    drudge.weenie_properties_create_list = vec![contain(DRUDGE, 1, COIN, 5)];
    MemContent::new()
        .weenie(sack)
        .weenie(row(COIN, "coin", WeenieType::Coin).with_int(PropertyInt::MaxStackSize, 1000))
        .weenie(row(UNDEF, "nothing", WeenieType::Undef))
        .weenie(drudge)
}

/// The equipment sets of `spell_table`: set 7 with tiers 2 -> [10, 11], 4 -> [12, 10], 6 -> [13].
pub(crate) fn spell_sets() -> BTreeMap<u32, SpellSet> {
    BTreeMap::from([(
        7,
        SpellSet {
            tiers: BTreeMap::from([(2, vec![10, 11]), (4, vec![12, 10]), (6, vec![13])]),
        },
    )])
}

/// A dat spell with only its id-bearing fields set (the set members read nothing else).
pub(crate) fn spell_base(id: u32) -> SpellBase {
    SpellBase {
        name: format!("Spell {id}"),
        description: String::new(),
        school: 1,
        icon: 0,
        category: id,
        bitfield: 0x4,
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 1,
        spell_economy_mod: 1.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 1,
        meta_spell_id: id,
        duration: Some((1800.0, 0.0, 0.0)),
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
    }
}

pub(crate) fn world() -> World {
    let clock = VirtualClock::default();
    let timers = TimersState::new(&clock);
    let spells = (10..=13).map(|id| (id, spell_base(id))).collect();
    let table = SpellTable {
        id: DataId(0x0E00_000E),
        spell_buckets: 64,
        spells,
        spellset_bucket_index: 1,
        spellsets: spell_sets(),
    };
    let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_spell_table(table)
        .build()
        .expect("fake dats");
    let mut w = World::new(ClockSnapshot::take(&clock, timers.portal_year_ticks), dats);
    w.timers = timers;
    w.content = Arc::new(content());
    gm::initialize(&mut w, &mut EmptyShard);
    land::use_flat_land_with_test_setup(&mut w, &[LB], 0);
    lm::get_landblock(
        &mut w,
        LandblockId::new(u32::from(LB) << 16 | 0xFFFF),
        false,
        false,
    );
    w
}

/// A generic item in `set` (none: no set), with ItemXpStyle `xp_style` when given.
pub(crate) fn set_item(w: &mut World, set: Option<i32>, xp_style: Option<i32>) -> ObjectGuid {
    let g = gm::new_dynamic_guid(w);
    let mut o = WorldObject::allocate(empyrean_world::dispatch::Class::GenericObject);
    o.guid = g;
    o.biota.id = g.full();
    if let Some(set) = set {
        o.set_property(PropertyInt::EquipmentSetId, set);
    }
    if let Some(style) = xp_style {
        o.set_property(PropertyInt::ItemXpStyle, style);
    }
    w.objects.insert(o).expect("fresh guid");
    g
}

pub(crate) fn ids(spells: &[empyrean_world::entity::spell::Spell]) -> Vec<u32> {
    spells
        .iter()
        .map(empyrean_world::entity::spell::Spell::id)
        .collect()
}

// ------------------------------------------------------------------------ WorldObject_Set.cs

// ----------------------------------------------------------------------- WorldObjectFactory.cs

// ------------------------------------------------------------------------------ Container.cs

// ---------------------------------------------------------------------------- EventManager.cs

// ---------------------------------------------------------------------- PositionExtensions.cs

// ------------------------------------------------------------------------------ real content

/// ACE's own `AdjustMapCoords` and `AdjustDungeon*` on the retail dats (the harness's
/// `position_real` area), replayed through the port.
#[cfg(feature = "real-content")]
pub(crate) mod real_content {
    pub(crate) use std::path::PathBuf;

    pub(crate) use empyrean_common::vectors::{self, f32_of, u64_of};
    pub(crate) use empyrean_dat::{DatManager, RealDats};

    pub(crate) use super::*;

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

    pub(crate) fn real_world() -> World {
        let clock = VirtualClock::default();
        let mut w = World::new(ClockSnapshot::take(&clock, 0.0), dats());
        gm::initialize(&mut w, &mut EmptyShard);
        w
    }

    pub(crate) fn assert_pos(p: &Position, e: &serde_json::Value, what: &str) {
        assert_eq!(
            u64::from(p.landblock_id().raw()),
            u64_of(&e["cell"]).unwrap(),
            "{what}: cell"
        );
        for (got, key) in [(p.position_x, "x"), (p.position_y, "y")] {
            let want = f32_of(&e[key]).unwrap();
            assert!(got == want, "{what}: {key} {got} vs ACE {want}");
        }
        // Z comes from the terrain polygon's plane, the shared physics crate's (V1): on 12 of the
        // 220 retail points it lands 1 to 3 ULPs from ACE's own plane arithmetic (V194).
        let want = f32_of(&e["z"]).unwrap();
        let ulps = (got_bits(p.position_z) - got_bits(want)).abs();
        assert!(
            ulps <= 4,
            "{what}: z {} vs ACE {want} ({ulps} ULPs)",
            p.position_z
        );
    }

    pub(crate) fn got_bits(f: f32) -> i64 {
        i64::from(f.to_bits().cast_signed())
    }
}
