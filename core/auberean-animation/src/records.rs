//! The records a body reads, from a world's data files, each decoded once.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::animation::{AnimMap, Animation, BehaviorTable, CharacterTemplate};
use auberean_assets::fx::FxScript;
use auberean_assets::setup::Setup;
use auberean_assets::visual::{Appearance, VisualDesc};
use auberean_assets::Decode;
use auberean_primitives::DataId;
use auberean_world::chargen::Random;
use auberean_world::look::LookRecords;
use auberean_world::records::WorldRecords;

use crate::body::BodyRecords;
use crate::graph::Animations;

type Cache<T> = RefCell<HashMap<u32, Option<Arc<T>>>>;

/// The character template's record id in the end-of-retail data.
pub const DEFAULT_TEMPLATE: u32 = 0x2F00_0000;

/// A world's records for bodies, decoded once each.
#[derive(Default)]
pub struct CachedRecords<'a> {
    pub world: Option<&'a WorldRecords>,
    animations: Cache<Animation>,
    visuals: Cache<VisualDesc>,
    setups: Cache<Setup>,
    maps: Cache<AnimMap>,
    tables: Cache<BehaviorTable>,
    appearances: Cache<Appearance>,
    templates: Cache<CharacterTemplate>,
    scripts: Cache<FxScript>,
    /// The character template's record id (found by enumeration where a database is at hand).
    pub template: DataId,
    /// The client's random numbers, which the bodies made from these records draw from.
    pub dice: Random,
    /// How hard the world's wind blows at the moment.
    pub wind: f32,
}

impl std::fmt::Debug for CachedRecords<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedRecords").finish_non_exhaustive()
    }
}

impl<'a> CachedRecords<'a> {
    #[must_use]
    pub fn new(world: &'a WorldRecords) -> Self {
        Self {
            world: Some(world),
            template: DataId(DEFAULT_TEMPLATE),
            ..Self::default()
        }
    }

    fn get<T: Decode>(&self, cache: &Cache<T>, id: DataId) -> Option<Arc<T>> {
        if id.is_none() {
            return None;
        }
        if let Some(hit) = cache.borrow().get(&id.0) {
            return hit.clone();
        }
        let v = self.world.and_then(|w| w.get::<T>(id.0)).map(Arc::new);
        cache.borrow_mut().insert(id.0, v.clone());
        v
    }
}

impl Animations for CachedRecords<'_> {
    fn animation(&self, id: DataId) -> Option<Arc<Animation>> {
        self.get(&self.animations, id)
    }
}

impl LookRecords for CachedRecords<'_> {
    fn visual(&self, id: DataId) -> Option<Arc<VisualDesc>> {
        self.get(&self.visuals, id)
    }
    fn appearance(&self, id: DataId) -> Option<Arc<Appearance>> {
        self.get(&self.appearances, id)
    }
}

impl BodyRecords for CachedRecords<'_> {
    fn setup(&self, id: DataId) -> Option<Arc<Setup>> {
        self.get(&self.setups, id)
    }
    fn anim_map(&self, id: DataId) -> Option<Arc<AnimMap>> {
        self.get(&self.maps, id)
    }
    fn behavior_table(&self, id: DataId) -> Option<Arc<BehaviorTable>> {
        self.get(&self.tables, id)
    }
    // D2-05054, D2-15087
    fn character_template(&self) -> Option<Arc<CharacterTemplate>> {
        self.get(&self.templates, self.template)
    }
    fn fx_script(&self, id: DataId) -> Option<Arc<FxScript>> {
        self.get(&self.scripts, id)
    }
    fn dice(&self) -> Option<&Random> {
        Some(&self.dice)
    }
    fn wind(&self) -> f32 {
        self.wind
    }
}
