//! Shared inputs and adapters for this test tier.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use dereth_animation::data::{
    AnimAssets, AnimationData, DegradeInfo, GfxObjLookup, MotionTableData, ParticleEmitterInfo,
    PhysicsScriptData, PhysicsScriptTableData, SetupData,
};
use dereth_assets::Decode;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;
use dereth_world_data::anim_convert as convert;

/// A graphics object's degrade record and base palette, `None` when the dat does not hold it.
type GfxObjFacts = Option<(Option<DataId>, Option<DataId>)>;

pub struct DatAssets {
    store: RetailDatStore,
    motion_tables: Mutex<BTreeMap<u32, Option<Arc<MotionTableData>>>>,
    animations: Mutex<BTreeMap<u32, Option<Arc<AnimationData>>>>,
    setups: Mutex<BTreeMap<u32, Option<Arc<SetupData>>>>,
    scripts: Mutex<BTreeMap<u32, Option<Arc<PhysicsScriptData>>>>,
    script_tables: Mutex<BTreeMap<u32, Option<Arc<PhysicsScriptTableData>>>>,
    emitters: Mutex<BTreeMap<u32, Option<Arc<ParticleEmitterInfo>>>>,
    degrades: Mutex<BTreeMap<u32, Option<Arc<DegradeInfo>>>>,
    gfxobjs: Mutex<BTreeMap<u32, GfxObjFacts>>,
}

impl std::fmt::Debug for DatAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DatAssets")
    }
}

pub fn open() -> DatAssets {
    let Some(store) = dereth_dat::testing::open_store() else {
        panic!(
            "the retail dats are this crate's oracle and they are not under {} -- \
             set DERETH_TEST_DAT_DIR. \
             It fails rather than skips, because a skip reads as a pass.",
            dereth_dat::testing::dat_dir().display()
        )
    };
    DatAssets {
        store,
        motion_tables: Mutex::new(BTreeMap::new()),
        animations: Mutex::new(BTreeMap::new()),
        setups: Mutex::new(BTreeMap::new()),
        scripts: Mutex::new(BTreeMap::new()),
        script_tables: Mutex::new(BTreeMap::new()),
        emitters: Mutex::new(BTreeMap::new()),
        degrades: Mutex::new(BTreeMap::new()),
        gfxobjs: Mutex::new(BTreeMap::new()),
    }
}

impl DatAssets {
    pub fn store(&self) -> &RetailDatStore {
        &self.store
    }

    pub fn ids_of(&self, kind: DbType) -> Vec<DataId> {
        self.store.ids_of(kind)
    }

    fn load<T, R>(
        &self,
        cache: &Mutex<BTreeMap<u32, Option<Arc<R>>>>,
        kind: DbType,
        id: DataId,
        convert: impl Fn(&T) -> R,
    ) -> Option<Arc<R>>
    where
        T: Decode,
    {
        if let Some(hit) = cache.lock().expect("cache").get(&id.0) {
            return hit.clone();
        }
        let decoded = self
            .store
            .read_typed(kind, id)
            .ok()
            .and_then(|bytes| T::decode_payload(id, &bytes).ok())
            .map(|v| Arc::new(convert(&v)));
        cache.lock().expect("cache").insert(id.0, decoded.clone());
        decoded
    }
}

impl AnimAssets for DatAssets {
    fn motion_table(&self, id: DataId) -> Option<Arc<MotionTableData>> {
        self.load::<dereth_assets::MotionTable, _>(
            &self.motion_tables,
            DbType::MTable,
            id,
            convert::motion_table,
        )
    }

    fn animation(&self, id: DataId) -> Option<Arc<AnimationData>> {
        self.load::<dereth_assets::Animation, _>(
            &self.animations,
            DbType::Anim,
            id,
            convert::animation,
        )
    }

    fn setup(&self, id: DataId) -> Option<Arc<SetupData>> {
        self.load::<dereth_assets::Setup, _>(&self.setups, DbType::Setup, id, convert::setup)
    }

    fn script(&self, id: DataId) -> Option<Arc<PhysicsScriptData>> {
        self.load::<dereth_assets::PhysicsScript, _>(
            &self.scripts,
            DbType::PhysicsScript,
            id,
            convert::physics_script,
        )
    }

    fn script_table(&self, id: DataId) -> Option<Arc<PhysicsScriptTableData>> {
        self.load::<dereth_assets::PhysicsScriptTable, _>(
            &self.script_tables,
            DbType::PhysicsScriptTable,
            id,
            convert::physics_script_table,
        )
    }

    fn emitter_info(&self, id: DataId) -> Option<Arc<ParticleEmitterInfo>> {
        self.load::<dereth_assets::world::ParticleEmitterInfo, _>(
            &self.emitters,
            DbType::ParticleEmitter,
            id,
            convert::emitter_info,
        )
    }

    fn degrade_info(&self, id: DataId) -> Option<Arc<DegradeInfo>> {
        self.load::<dereth_assets::GfxObjDegradeInfo, _>(
            &self.degrades,
            DbType::DegradeInfo,
            id,
            convert::degrade_info,
        )
    }

    fn gfxobj(&self, id: DataId) -> GfxObjLookup {
        let hit = self.gfxobjs.lock().expect("cache").get(&id.0).copied();
        let found = hit.unwrap_or_else(|| {
            let lookup = dereth_assets::texture_lookup::TextureLookup::new(&self.store, 1);
            let found = self
                .store
                .read_typed(DbType::GfxObj, id)
                .ok()
                .and_then(|bytes| dereth_assets::GfxObj::decode_payload(id, &bytes).ok())
                .map(|o| {
                    let palette = o.surfaces.iter().find_map(|s| lookup.surface_palette(*s));
                    (o.did_degrade, palette)
                });
            self.gfxobjs.lock().expect("cache").insert(id.0, found);
            found
        });
        match found {
            Some((did_degrade, base_palette)) => GfxObjLookup::Present {
                did_degrade,
                base_palette,
            },
            None => GfxObjLookup::Absent,
        }
    }
}
