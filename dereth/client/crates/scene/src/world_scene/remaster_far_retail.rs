//! Retail's own trees for the far forest: which of the region's scenery are trees, and what each
//! is drawn with, so the far forest stands retail's trees out to the horizon where no art pack
//! draws them.
//!
//! Which objects are trees is worked out once for the region, on a thread of its own: every
//! object the region's scenes place, kept where most of its surfaces are plants' and it stands at
//! least [`LEAST_RADIUS`] across, as the near impostors choose their trees. A tree's far model
//! names its meshes, each at the level the world draws it nearest and where it stands in the
//! tree, and the remaster makes its pictures from them on the device, as it makes its impostors'.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use dereth_assets::Decode;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, TextureData};
use dereth_remaster::assets::{kind::Kind, AssetSource, MaterialKey, SurfaceRef, Use};
use dereth_remaster::scene::FarPart;
use glam::{Mat4, Quat, Vec3};

use super::remaster_assets::DatAssets;

/// The views round a tree, and how many to a row of its pictures.
pub(crate) const VIEWS: u32 = 16;
/// The least radius of a tree, metres: smaller plants have no far pictures, as they have no
/// impostors near.
const LEAST_RADIUS: f32 = 3.0;
/// Where the pictures' coverage is cut.
const CUT: f32 = 0.45;

/// One triangle of a tree in the tree's own space: its corners.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Triangle {
    pub(crate) p: [Vec3; 3],
}

/// A tree's shape: its triangles, and the meshes it is drawn with.
#[derive(Debug, Clone, Default)]
pub(crate) struct Shape {
    pub(crate) triangles: Vec<Triangle>,
    pub(crate) parts: Vec<FarPart>,
}

/// The mesh a part of the GfxObj `gfxobj` draws nearest: its degrade record's first level, where
/// it has a record of several, as the world draws it; else the GfxObj itself.
fn nearest_level(store: &RetailDatStore, gfxobj: DataId) -> u32 {
    let first = (|| {
        let bytes = store.read_typed(DbType::GfxObj, gfxobj).ok()?;
        let obj =
            dereth_assets::GfxObj::decode_payload_in(store.era_of(gfxobj), gfxobj, &bytes).ok()?;
        let did = obj.did_degrade?;
        let bytes = store.read_typed(DbType::DegradeInfo, did).ok()?;
        let info =
            dereth_assets::GfxObjDegradeInfo::decode_payload_in(store.era_of(did), did, &bytes)
                .ok()?;
        let first = info.degrades.first()?.gfxobj_id.0;
        (info.degrades.len() > 1 && first != 0).then_some(first)
    })();
    first.unwrap_or(gfxobj.0)
}

/// The parts of the object `id` as meshes in its own space, each at the level the world draws it
/// nearest, with whether it is a tree: most of its surfaces plants', and at least
/// [`LEAST_RADIUS`] across.
fn shape_of(assets: &mut DatAssets<'_>, store: &RetailDatStore, id: u32) -> Option<(Shape, bool)> {
    let parts = dereth_client_runtime::models::resolve_parts(store, DataId(id));
    if parts.is_empty() {
        return None;
    }
    let mut shape = Shape::default();
    let (mut subsets, mut foliage) = (0usize, 0usize);
    let mut radius = 0.0f32;
    for part in &parts {
        let gfxobj = nearest_level(store, part.gfxobj);
        let mesh = assets.gfxobj(gfxobj)?;
        let o = part.frame.origin;
        let r = part.frame.rotation;
        let place = Mat4::from_scale_rotation_translation(
            Vec3::new(part.scale.x, part.scale.y, part.scale.z),
            Quat::from_xyzw(r.x, r.y, r.z, r.w),
            Vec3::new(o.x, o.y, o.z),
        );
        shape.parts.push(FarPart {
            gfxobj,
            frame: place,
        });
        for s in &mesh.subsets {
            let SurfaceRef::Surface(surface) = s.surface else {
                continue;
            };
            subsets += 1;
            let material = assets.material(&MaterialKey::plain(surface), Use::Static);
            if material.additive || material.translucent.is_some() {
                continue;
            }
            if material.kind == Kind::Foliage {
                foliage += 1;
            }
            let first = s.first_index as usize;
            let count = s.index_count as usize;
            for tri in mesh.indices[first..first + count].as_chunks::<3>().0 {
                let p = tri.map(|i| {
                    place.transform_point3(Vec3::from_array(mesh.vertices[i as usize].position))
                });
                for q in p {
                    radius = radius.max(q.length());
                }
                shape.triangles.push(Triangle { p });
            }
        }
    }
    let tree = subsets > 0 && foliage * 2 >= subsets && radius >= LEAST_RADIUS;
    Some((shape, tree))
}

/// The far model of a tree of `shape`: its bounds, its quad's half width, foot and top, and the
/// meshes its pictures are made of.
pub(crate) fn model_of(id: u32, shape: &Shape) -> Option<dereth_remaster::scene::FarPackModel> {
    let (mut half, mut foot, mut top) = (0.0f32, f32::MAX, f32::MIN);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for t in &shape.triangles {
        for p in t.p {
            half = half.max(p.truncate().length());
            foot = foot.min(p.z);
            top = top.max(p.z);
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    if shape.triangles.is_empty() || half <= 0.0 || top <= foot || shape.parts.is_empty() {
        return None;
    }
    Some(dereth_remaster::scene::FarPackModel {
        id,
        bounds: ((lo + hi) * 0.5, (hi - lo).length() * 0.5),
        quad: [half, foot, top],
        views: [VIEWS, VIEWS],
        color: [1.0; 4],
        cut: CUT,
        foliage: true,
        parts: shape.parts.clone(),
    })
}

/// The objects every scene of `region` places.
fn scenery_objects(store: &RetailDatStore, region: &dereth_assets::region::Region) -> Vec<u32> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in region.scene_info.iter().flatten() {
        for id in &d.scenes {
            let Some(scene) = dereth_client_runtime::world_build::read_scene(store, *id) else {
                continue;
            };
            for o in &scene.objects {
                if o.weenie_obj == 0 && seen.insert(o.obj_id.0) {
                    out.push(o.obj_id.0);
                }
            }
        }
    }
    out.sort_unstable();
    out
}

/// Retail's trees with far pictures: which they are, worked out on a thread of its own.
#[derive(Debug, Default)]
pub(crate) struct RetailFar {
    /// The trees' models, in a fixed order, once worked out.
    models: Mutex<Option<Arc<Vec<dereth_remaster::scene::FarPackModel>>>>,
    /// The trees' models as the frame sees them: taken from what the thread worked out once a
    /// frame ([`Self::observe`]), so everything one frame draws agrees on whether they are there.
    seen: Mutex<Option<Arc<Vec<dereth_remaster::scene::FarPackModel>>>>,
}

impl RetailFar {
    /// Work out retail's trees for `region`, on a thread of its own.
    pub(crate) fn start(
        store: Arc<RetailDatStore>,
        region: Arc<dereth_assets::region::Region>,
    ) -> Arc<Self> {
        let me = Arc::new(Self::default());
        let held = Arc::clone(&me);
        let spawned = crate::workers::spawn("retail far trees", move || {
            let mut assets =
                DatAssets::new(&store, 0, dereth_render::texture::ImageScale::default());
            let mut models = Vec::new();
            for id in scenery_objects(&store, &region) {
                let Some((shape, true)) = shape_of(&mut assets, &store, id) else {
                    continue;
                };
                if let Some(m) = model_of(id, &shape) {
                    models.push(m);
                }
            }
            *held.models.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(models));
        });
        if let Err(e) = spawned {
            tracing::warn!(target: "dereth::remaster", "retail's far trees are not worked out: {e}");
        }
        me
    }

    /// The trees' models as this frame sees them, once worked out.
    pub(crate) fn models(&self) -> Option<Arc<Vec<dereth_remaster::scene::FarPackModel>>> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Take what the thread has worked out as what this frame sees: once a frame, before anything
    /// reads [`Self::models`].
    pub(crate) fn observe(&self) {
        let made = self
            .models
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        *self.seen.lock().unwrap_or_else(PoisonError::into_inner) = made;
    }

    /// Wait until the trees are worked out: for a capture that draws them from its first frames.
    /// It gives up after two minutes.
    pub(crate) fn wait(&self) {
        let started = web_time::Instant::now();
        while started.elapsed() < std::time::Duration::from_secs(120) {
            let ready = self
                .models
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_some();
            if ready {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

/// The far pictures of an art pack's trees and of retail's, the pack's models first: an art pack's
/// own pictures of its models. Retail's trees' are made on the device.
#[derive(Debug)]
pub(crate) struct FarPictures {
    pub(crate) pack: Option<Arc<dyn dereth_remaster::scene::FarPictures>>,
    pub(crate) pack_models: usize,
}

impl dereth_remaster::scene::FarPictures for FarPictures {
    fn pictures(&self, model: usize) -> Option<[TextureData; 2]> {
        if model < self.pack_models {
            return self.pack.as_ref()?.pictures(model);
        }
        None
    }
}

/// The retail objects with far pictures, an art pack's (`pack`'s, where its trees are drawn) and
/// retail's own, with a serial of their set: `cache` holds the last, made again only when either
/// changes. `None` while there are none.
pub(crate) fn tree_set(
    cache: &mut Option<(u64, Arc<HashSet<u32>>)>,
    pack: Option<&crate::art_pack::ArtPack>,
    retail: Option<&RetailFar>,
) -> Option<(u64, Arc<HashSet<u32>>)> {
    let models = retail.and_then(RetailFar::models);
    if pack.is_none() && models.is_none() {
        *cache = None;
        return None;
    }
    let serial = set_serial(pack.map(|p| p.serial), models.as_ref().map(|m| m.len()));
    if let Some(c) = cache.as_ref().filter(|c| c.0 == serial) {
        return Some(c.clone());
    }
    let mut ids: HashSet<u32> = pack
        .map(|p| {
            p.models
                .iter()
                .filter(|m| {
                    m.category == crate::art_pack::Category::Tree
                        || m.lods.iter().any(|l| l.views.is_some())
                })
                .map(|m| m.id.0)
                .collect()
        })
        .unwrap_or_default();
    ids.extend(models.iter().flat_map(|m| m.iter().map(|m| m.id)));
    *cache = Some((serial, Arc::new(ids)));
    cache.clone()
}

/// The serial of a set of far pictures: the art pack's, where one is drawn, and how many of
/// retail's trees, once worked out.
pub(crate) fn set_serial(pack: Option<u64>, retail: Option<usize>) -> u64 {
    let pack = pack.map_or(0, |p| p.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    pack ^ retail.map_or(0, |n| (n as u64) << 1 | 0x8000_0000_0000_0000)
}

/// The far pictures the remaster draws, `serial` their number: the art pack's (`pack`, its far
/// pictures), then retail's trees the pack draws none of; none of the objects the pack draws its
/// own models for near (`swapped`), whatever their kind, so a tree the pack draws as a plant near
/// is no retail tree far off.
pub(crate) fn far_pack(
    pack: Option<dereth_remaster::scene::FarPack>,
    retail: Option<&Arc<RetailFar>>,
    swapped: &HashSet<u32>,
    serial: u64,
) -> Option<dereth_remaster::scene::FarPack> {
    let models = retail.and_then(|r| r.models());
    let Some(retail_models) = models else {
        return pack;
    };
    let (mut models, pictures, pack_models) = match pack {
        Some(p) => {
            let n = p.models.len();
            (p.models, Some(p.pictures), n)
        }
        None => (Vec::new(), None, 0),
    };
    let drawn: HashSet<u32> = models.iter().map(|m| m.id).collect();
    for m in retail_models.iter() {
        if !drawn.contains(&m.id) && !swapped.contains(&m.id) {
            models.push(m.clone());
        }
    }
    Some(dereth_remaster::scene::FarPack {
        serial,
        models,
        pictures: Arc::new(FarPictures {
            pack: pictures,
            pack_models,
        }),
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    fn model(id: u32) -> dereth_remaster::scene::FarPackModel {
        dereth_remaster::scene::FarPackModel {
            id,
            bounds: (Vec3::ZERO, 1.0),
            quad: [1.0, 0.0, 2.0],
            views: [VIEWS, VIEWS],
            color: [1.0; 4],
            cut: CUT,
            foliage: true,
            parts: Vec::new(),
        }
    }

    /// Retail's trees stand in the far forest but for those an art pack draws its own models for
    /// near, whatever their kind: a tree the pack draws as a plant is no retail tree far off.
    #[test]
    fn retails_far_trees_leave_out_those_a_pack_draws_its_own_models_for() {
        let retail = Arc::new(RetailFar::default());
        *retail.models.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(Arc::new(vec![model(1), model(2), model(3)]));
        retail.observe();
        let swapped: HashSet<u32> = [2].into_iter().collect();
        let far = far_pack(None, Some(&retail), &swapped, 7).expect("retail's trees");
        let ids: Vec<u32> = far.models.iter().map(|m| m.id).collect();
        assert_eq!(ids, [1, 3]);
        assert_eq!(far.serial, 7);
    }
}
