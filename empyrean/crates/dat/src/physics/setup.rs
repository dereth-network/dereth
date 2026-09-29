//! The setup-geometry loader: a dat id to the shared crate's `SetupGeometry`, cached per id.
//!
//! Nothing here is ported from ACE; the conversion is in [`super::convert`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use dereth_physics::SetupGeometry;

use super::convert::{
    setup_geometry_with_parts_at, simple_setup_geometry, PLACEMENT_FRAME_DEFAULT,
};
use crate::file_types::SetupModel;
use crate::DatManager;

/// Collision geometry by setup (or bare graphics-object) id, built once per id and shared.
pub struct SetupGeometryCache {
    dats: Arc<DatManager>,
    geometry: Mutex<HashMap<u32, Option<Arc<SetupGeometry>>>>,
}

impl std::fmt::Debug for SetupGeometryCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.geometry.lock().map_or(0, |g| g.len());
        f.debug_struct("SetupGeometryCache")
            .field("cached", &n)
            .finish_non_exhaustive()
    }
}

impl SetupGeometryCache {
    #[must_use]
    pub fn new(dats: Arc<DatManager>) -> Self {
        Self {
            dats,
            geometry: Mutex::new(HashMap::new()),
        }
    }

    /// The geometry of `id`: a `0x02` setup with its parts, or a `0x01` graphics object as a
    /// one-part setup. `None` for any other id, or one the portal dat does not have.
    #[must_use]
    pub fn get(&self, id: u32) -> Option<Arc<SetupGeometry>> {
        if let Some(hit) = self.geometry.lock().ok().and_then(|g| g.get(&id).cloned()) {
            return hit;
        }
        let built = load_setup_geometry(&self.dats, id).map(Arc::new);
        if let Ok(mut g) = self.geometry.lock() {
            return g.entry(id).or_insert(built).clone();
        }
        built
    }
}

/// Build the collision geometry of `id` without caching it.
#[must_use]
pub fn load_setup_geometry(dats: &DatManager, id: u32) -> Option<SetupGeometry> {
    match id >> 24 {
        0x02 => {
            let setup = dats.portal_dat().read_from_dat::<SetupModel>(id)?;
            Some(setup_geometry_with_parts_at(
                dats,
                &setup,
                PLACEMENT_FRAME_DEFAULT,
            ))
        }
        0x01 => simple_setup_geometry(dats, id),
        _ => None,
    }
}
