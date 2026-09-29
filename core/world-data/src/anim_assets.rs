//! A [`dereth_animation::AnimAssets`] over the retail dats.
//!
//! The decoders stay out of `dereth-animation`: its runtime shapes are built from `dereth-assets`
//! records with the free functions in [`crate::anim_convert`]. Every conversion below is one call
//! into it. Nothing here parses a byte or renames a field.
//!
//! The cache is the point. Setup construction and every motion-table lookup ask for the same
//! handful of records over and over, and following a motion-table link reads another table.
//! Without this cache, the same records would be decoded dozens of times a second.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::anim_convert as convert;
use dereth_animation::data::{
    AnimAssets, AnimationData, DegradeInfo, GfxObjLookup, MotionTableData, ParticleEmitterInfo,
    PhysicsScriptData, PhysicsScriptTableData, SetupData,
};
use dereth_assets::Decode;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;

/// One memo per record type. `None` is cached too: a missing record is asked for repeatedly (a
/// setup with no script table, say) and re-reading the index each time is the same waste.
type Memo<T> = Mutex<BTreeMap<u32, Option<Arc<T>>>>;

/// The animation track's asset seam, backed by `client_portal.dat`.
pub struct DatAnimAssets {
    store: Arc<RetailDatStore>,
    motion_tables: Memo<MotionTableData>,
    animations: Memo<AnimationData>,
    setups: Memo<SetupData>,
    scripts: Memo<PhysicsScriptData>,
    script_tables: Memo<PhysicsScriptTableData>,
    emitters: Memo<ParticleEmitterInfo>,
    degrades: Memo<DegradeInfo>,
    /// A qualified degradation-info lookup reduced to the two facts
    /// the degradation lookup reads — present or not, and `did_degrade`.
    ///
    /// This memo is not the ordinary "misses are asked for repeatedly" argument the others make:
    /// **a cache miss here costs a whole graphics-object decode**, vertex array, polygons and both BSP trees,
    /// because `did_degrade` is the *last* field of the record (present under flags bit 3),
    /// and there is no way to reach it without walking everything before it. The
    /// bodies this seam is asked about are the same three dozen ids over and over —
    /// object creation asks once per part, and a part swap
    /// asks again — so without the memo a crowded scene re-decodes megabytes per frame.
    gfxobjs: Mutex<BTreeMap<u32, GfxObjLookup>>,
}

impl std::fmt::Debug for DatAnimAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DatAnimAssets")
    }
}

impl DatAnimAssets {
    #[must_use]
    pub fn new(store: Arc<RetailDatStore>) -> Self {
        Self {
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

    /// Read, decode and convert one record, memoising both hits and misses.
    ///
    /// A malformed record becomes `None` rather than a panic: `AnimAssets` has no error channel,
    /// and the client's own cache lookup returns NULL for the same case.
    fn load<T, R>(
        &self,
        cache: &Memo<R>,
        kind: DbType,
        id: DataId,
        convert: impl Fn(&T) -> R,
    ) -> Option<Arc<R>>
    where
        T: Decode,
    {
        if let Ok(c) = cache.lock() {
            if let Some(hit) = c.get(&id.0) {
                return hit.clone();
            }
        }
        let decoded = self
            .store
            .read_typed(kind, id)
            .ok()
            .and_then(|bytes| T::decode_payload(id, &bytes).ok())
            .map(|v| Arc::new(convert(&v)));
        if let Ok(mut c) = cache.lock() {
            c.insert(id.0, decoded.clone());
        }
        decoded
    }
}

impl AnimAssets for DatAnimAssets {
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

    /// The client asset source for the graphics-object lookup.
    ///
    /// **This source can always look, so it never answers [`GfxObjLookup::Unknown`].** That is the
    /// whole point of the override: the defaulted `Unknown` is honest for a fixture with no dat
    /// behind it, and a lie for a client holding `client_portal.dat`. With it in place a part swap
    /// naming an object the dat does not hold takes the lookup's failure
    /// arm and the previous limb is kept, and a swap that succeeds carries the **new** object's
    /// `GfxObjDegradeInfo` into the per-frame viewer-distance update, so the swapped-in part
    /// degrades on its own thresholds and not on the replaced part's.
    ///
    /// A read failure and a decode failure are both `Absent`, matching database lookup returning
    /// NULL: the client cannot tell a record that is not in the index from one it could not parse,
    /// and graphics-object array loading treats them alike.
    fn gfxobj(&self, id: DataId) -> GfxObjLookup {
        if let Ok(c) = self.gfxobjs.lock() {
            if let Some(hit) = c.get(&id.0) {
                return *hit;
            }
        }
        let answer = match self
            .store
            .read_typed(DbType::GfxObj, id)
            .ok()
            .and_then(|bytes| dereth_assets::GfxObj::decode_payload(id, &bytes).ok())
        {
            Some(o) => GfxObjLookup::Present {
                did_degrade: o.did_degrade,
            },
            None => GfxObjLookup::Absent,
        };
        if let Ok(mut c) = self.gfxobjs.lock() {
            c.insert(id.0, answer);
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the retail dat files. The Aluvian male's five records named by the chargen table
    // must all resolve --
    // a missing one is how a character ends up invisible or motionless.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_aluvian_males_setup_and_motion_table_load_through_the_seam() {
        // Absent dats fail rather than skip: a checkout with no retail dats would otherwise
        // report `... ok` having loaded nothing.
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        let a = DatAnimAssets::new(Arc::new(store));
        let setup = a.setup(DataId(0x0200_0001)).expect("setup 0x02000001");
        assert_eq!(setup.parts.len(), 34, "the male body is 34 parts");
        assert_eq!(
            setup.spheres.len(),
            2,
            "two collision spheres, feet and chest"
        );
        let mt = a
            .motion_table(DataId(0x0900_0001))
            .expect("motion table 0x09000001");
        assert!(!mt.cycles.is_empty() && !mt.links.is_empty());
        // The memo returns the same allocation, which is what keeps a per-frame lookup cheap.
        assert!(Arc::ptr_eq(
            &setup,
            &a.setup(DataId(0x0200_0001)).expect("cached")
        ));
        // A record that does not exist caches as a miss rather than erroring.
        assert!(a.setup(DataId(0x02FF_FFFF)).is_none());
    }
}
