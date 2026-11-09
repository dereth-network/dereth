//! Far pictures of retail's own trees, for the far forest: each tree among the region's scenery
//! drawn from its own meshes, level, from sixteen ways round it, as an art pack lays the pictures
//! of a model seen from round it (`dereth_assets::art_pack`, `views`), so the far forest stands
//! retail's trees out to the horizon where no art pack draws them.
//!
//! - **Which objects are trees** is worked out once for the region, on a thread of its own: every
//!   object the region's scenes place, kept where most of its surfaces are plants' and it stands
//!   at least [`LEAST_RADIUS`] across, as the near impostors choose their trees.
//! - **The pictures** of a tree are drawn on that thread when the far forest first asks for them:
//!   each triangle of its meshes laid flat on the view's picture, the nearest kept, its surface's
//!   texel cut as the surface cuts it. The color picture holds the surface's own color and the
//!   coverage; the normal picture the model's own normal and how far along the view the surface
//!   stands, as the far forest reads a pack's.

use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc, Mutex, PoisonError};

use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, TextureData, TextureFormat};
use dereth_remaster::assets::{kind::Kind, AssetSource, MaterialKey, SurfaceRef, Use};
use glam::{Mat4, Quat, Vec2, Vec3};

use super::remaster_assets::DatAssets;

/// The views round a tree, and how many to a row of its pictures.
pub(crate) const VIEWS: u32 = 16;
const COLUMNS: u32 = 8;
/// A view's side, texels.
const TILE: u32 = 64;
/// The least radius of a tree, metres: smaller plants have no far pictures, as they have no
/// impostors near.
const LEAST_RADIUS: f32 = 3.0;
/// Where the pictures' coverage is cut.
const CUT: f32 = 0.45;

/// One triangle of a tree in the tree's own space: its corners, their normals and texture
/// coordinates, and its picture.
#[derive(Debug, Clone, Copy)]
struct Triangle {
    p: [Vec3; 3],
    n: [Vec3; 3],
    uv: [Vec2; 3],
    picture: usize,
}

/// A surface's picture, decoded: its texels in display values, red first, and the coverage under
/// which a texel is cut away (0 where the surface cuts nothing away).
#[derive(Debug, Clone)]
struct Picture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    cut: u8,
}

impl Picture {
    /// The texel at `uv`, repeating.
    fn at(&self, uv: Vec2) -> [u8; 4] {
        #[allow(clippy::cast_precision_loss)] // a picture's size
        let (w, h) = (self.width as f32, self.height as f32);
        let x = (uv.x - uv.x.floor()) * w;
        let y = (uv.y - uv.y.floor()) * h;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x, y) = (
            (x as u32).min(self.width - 1),
            (y as u32).min(self.height - 1),
        );
        let k = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[k],
            self.rgba[k + 1],
            self.rgba[k + 2],
            self.rgba[k + 3],
        ]
    }
}

/// A tree's shape: its triangles, the pictures they draw, and its bounds.
#[derive(Debug, Clone, Default)]
struct Shape {
    triangles: Vec<Triangle>,
    pictures: Vec<Picture>,
}

/// A surface's picture as RGBA texels, from its first level.
fn rgba_of(t: &TextureData) -> Option<Vec<u8>> {
    use dereth_render::PixelFormatId as P;
    let level = t.levels.first()?;
    let bgra = match t.format {
        TextureFormat::Bgra8 => level.clone(),
        TextureFormat::Bc1 => dereth_render::dxt::decode(P::Dxt1, level, t.width, t.height).ok()?,
        TextureFormat::Bc2 | TextureFormat::Bc2Premultiplied => {
            dereth_render::dxt::decode(P::Dxt3, level, t.width, t.height).ok()?
        }
        TextureFormat::Bc3 | TextureFormat::Bc3Premultiplied => {
            dereth_render::dxt::decode(P::Dxt5, level, t.width, t.height).ok()?
        }
        _ => return None,
    };
    Some(
        bgra.as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect(),
    )
}

/// The parts of the object `id` as meshes in its own space, with whether it is a tree: most of
/// its surfaces plants', and at least [`LEAST_RADIUS`] across. `pictures` says whether its
/// surfaces' pictures are read too.
fn shape_of(
    assets: &mut DatAssets<'_>,
    store: &RetailDatStore,
    id: u32,
    pictures: bool,
) -> Option<(Shape, bool)> {
    let parts = dereth_client_runtime::models::resolve_parts(store, DataId(id));
    if parts.is_empty() {
        return None;
    }
    let mut shape = Shape::default();
    let mut by_surface: HashMap<u32, Option<usize>> = HashMap::new();
    let (mut subsets, mut foliage) = (0usize, 0usize);
    let mut radius = 0.0f32;
    for part in &parts {
        let mesh = assets.gfxobj(part.gfxobj.0)?;
        let o = part.frame.origin;
        let r = part.frame.rotation;
        let place = Mat4::from_scale_rotation_translation(
            Vec3::new(part.scale.x, part.scale.y, part.scale.z),
            Quat::from_xyzw(r.x, r.y, r.z, r.w),
            Vec3::new(o.x, o.y, o.z),
        );
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
            let picture = if pictures {
                *by_surface.entry(surface).or_insert_with(|| {
                    let key = material.texture?;
                    let t = assets.texture(&key, material.clip_map)?;
                    let rgba = rgba_of(&t)?;
                    shape.pictures.push(Picture {
                        width: t.width.max(1),
                        height: t.height.max(1),
                        rgba,
                        cut: material.alpha_test.unwrap_or(0),
                    });
                    Some(shape.pictures.len() - 1)
                })
            } else {
                Some(0)
            };
            let Some(picture) = picture else {
                continue;
            };
            let first = s.first_index as usize;
            let count = s.index_count as usize;
            for tri in mesh.indices[first..first + count].as_chunks::<3>().0 {
                let v = tri.map(|i| mesh.vertices[i as usize]);
                let p = v.map(|v| place.transform_point3(Vec3::from_array(v.position)));
                let n = v.map(|v| {
                    place
                        .transform_vector3(Vec3::from_array(v.normal))
                        .normalize_or(Vec3::Z)
                });
                for q in p {
                    radius = radius.max(q.length());
                }
                shape.triangles.push(Triangle {
                    p,
                    n,
                    uv: v.map(|v| Vec2::from_array(v.uv)),
                    picture,
                });
            }
        }
    }
    let tree = subsets > 0 && foliage * 2 >= subsets && radius >= LEAST_RADIUS;
    Some((shape, tree))
}

/// The far model of a tree of `shape`: its bounds, and its quad's half width, foot and top.
fn model_of(id: u32, shape: &Shape) -> Option<dereth_remaster::scene::FarPackModel> {
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
    if shape.triangles.is_empty() || half <= 0.0 || top <= foot {
        return None;
    }
    Some(dereth_remaster::scene::FarPackModel {
        id,
        bounds: ((lo + hi) * 0.5, (hi - lo).length() * 0.5),
        quad: [half, foot, top],
        views: [VIEWS, COLUMNS],
        color: [1.0; 4],
        cut: CUT,
        foliage: true,
    })
}

/// The far pictures of a tree of `shape`, its model and its two pictures: color and coverage,
/// then normal and depth, [`VIEWS`] round it laid [`COLUMNS`] to a row.
fn bake(
    id: u32,
    shape: &Shape,
) -> Option<(dereth_remaster::scene::FarPackModel, [TextureData; 2])> {
    let model = model_of(id, shape)?;
    let [half, foot, top] = model.quad;
    let rows = VIEWS.div_ceil(COLUMNS);
    let (w, h) = (COLUMNS * TILE, rows * TILE);
    let mut color = vec![0u8; (w * h * 4) as usize];
    let mut normal = vec![0u8; (w * h * 4) as usize];
    let mut depth = vec![f32::MAX; (w * h) as usize];
    #[allow(clippy::cast_precision_loss)] // a view's side
    let side = TILE as f32;
    for k in 0..VIEWS {
        #[allow(clippy::cast_precision_loss)] // sixteen views
        let theta = k as f32 / VIEWS as f32 * std::f32::consts::TAU;
        let (sin, cos) = dereth_primitives::num::math::sin_cosf(theta);
        let across = Vec3::new(-sin, cos, 0.0);
        let inward = Vec3::new(-cos, -sin, 0.0);
        let (ox, oy) = ((k % COLUMNS) * TILE, (k / COLUMNS) * TILE);
        // A point's place on the view's picture, texels, and how far along the view it stands.
        let project = |p: Vec3| {
            Vec3::new(
                (p.dot(across) / half * 0.5 + 0.5) * side,
                (top - p.z) / (top - foot) * side,
                p.dot(inward),
            )
        };
        for t in &shape.triangles {
            let s = t.p.map(project);
            let (a, b, c) = (s[0], s[1], s[2]);
            let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            if area.abs() < 1e-8 {
                continue;
            }
            let (x0, x1) = (
                a.x.min(b.x).min(c.x).floor().max(0.0),
                a.x.max(b.x).max(c.x).ceil().min(side),
            );
            let (y0, y1) = (
                a.y.min(b.y).min(c.y).floor().max(0.0),
                a.y.max(b.y).max(c.y).ceil().min(side),
            );
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (x0, x1, y0, y1) = (x0 as u32, x1 as u32, y0 as u32, y1 as u32);
            let picture = shape.pictures.get(t.picture);
            for y in y0..y1 {
                for x in x0..x1 {
                    #[allow(clippy::cast_precision_loss)]
                    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((b.x - px) * (c.y - py) - (b.y - py) * (c.x - px)) / area;
                    let w1 = ((c.x - px) * (a.y - py) - (c.y - py) * (a.x - px)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let d = a.z * w0 + b.z * w1 + c.z * w2;
                    let at = ((oy + y) * w + ox + x) as usize;
                    if d >= depth[at] {
                        continue;
                    }
                    let uv = t.uv[0] * w0 + t.uv[1] * w1 + t.uv[2] * w2;
                    let texel = picture.map_or([255, 255, 255, 255], |p| p.at(uv));
                    // Only a surface that cuts its picture away by its coverage is cut.
                    if picture.is_some_and(|p| p.cut > 0 && texel[3] < p.cut) {
                        continue;
                    }
                    depth[at] = d;
                    let mut n = (t.n[0] * w0 + t.n[1] * w1 + t.n[2] * w2).normalize_or(Vec3::Z);
                    // A leaf seen from behind is lit as the side the eye sees.
                    if n.dot(inward) > 0.0 {
                        n = -n;
                    }
                    color[at * 4..at * 4 + 4].copy_from_slice(&[texel[0], texel[1], texel[2], 255]);
                    let e = |v: f32| {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let b = ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                        b
                    };
                    normal[at * 4..at * 4 + 4].copy_from_slice(&[
                        e(n.x),
                        e(n.y),
                        e(n.z),
                        e(d / half),
                    ]);
                }
            }
        }
    }
    // The cut-away texels take the mean of what is covered, so the smaller levels bleed no black
    // into the crowns' edges.
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for p in color.as_chunks::<4>().0 {
        if p[3] > 0 {
            for c in 0..3 {
                sum[c] += u64::from(p[c]);
            }
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)] // a mean of bytes
    let mean = sum.map(|s| (s / n) as u8);
    for (c, nd) in color
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(normal.as_chunks_mut::<4>().0)
    {
        if c[3] == 0 {
            c[..3].copy_from_slice(&mean);
            nd.copy_from_slice(&[128, 128, 255, 128]);
        }
    }
    let to_bgra = |rgba: &[u8]| -> Vec<u8> {
        rgba.as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect()
    };
    let levels = |rgba: Vec<u8>, coverage: Option<&[u8]>| -> Vec<Vec<u8>> {
        let mut out = vec![to_bgra(&rgba)];
        let (mut cw, mut ch, mut cur) = (w, h, rgba);
        let mut cov: Option<Vec<u8>> = coverage.map(<[u8]>::to_vec);
        while cw > 1 && ch > 1 {
            let (nw, nh) = (cw / 2, ch / 2);
            let mut next = vec![0u8; (nw * nh * 4) as usize];
            let mut next_cov = vec![0u8; (nw * nh * 4) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    let (mut acc, mut wsum, mut alpha) = ([0u32; 4], 0u32, 0u32);
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let k = (((y * 2 + dy) * cw + x * 2 + dx) * 4) as usize;
                        let a = cov
                            .as_ref()
                            .map_or(u32::from(cur[k + 3]), |c| u32::from(c[k + 3]));
                        let weight = a + 1;
                        for c in 0..4 {
                            acc[c] += u32::from(cur[k + c]) * weight;
                        }
                        wsum += weight;
                        alpha += u32::from(cur[k + 3]);
                    }
                    let o = ((y * nw + x) * 4) as usize;
                    for c in 0..3 {
                        #[allow(clippy::cast_possible_truncation)] // a mean of bytes
                        let v = (acc[c] / wsum) as u8;
                        next[o + c] = v;
                    }
                    #[allow(clippy::cast_possible_truncation)] // a mean of bytes
                    let a = if cov.is_some() {
                        (acc[3] / wsum) as u8
                    } else {
                        (alpha / 4) as u8
                    };
                    next[o + 3] = a;
                    if let Some(c) = cov.as_ref() {
                        let mut s = 0u32;
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let k = (((y * 2 + dy) * cw + x * 2 + dx) * 4) as usize;
                            s += u32::from(c[k + 3]);
                        }
                        #[allow(clippy::cast_possible_truncation)]
                        let v = (s / 4) as u8;
                        next_cov[o + 3] = v;
                    }
                }
            }
            out.push(to_bgra(&next));
            if cov.is_some() {
                cov = Some(next_cov);
            }
            cur = next;
            cw = nw;
            ch = nh;
        }
        out
    };
    let color_levels = levels(color.clone(), None);
    let normal_levels = levels(normal, Some(&color));
    let picture = |levels| TextureData {
        width: w,
        height: h,
        format: TextureFormat::Bgra8,
        levels,
    };
    Some((model, [picture(color_levels), picture(normal_levels)]))
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

/// Retail's trees with far pictures: which they are, worked out on a thread of its own, and their
/// pictures, drawn there as the far forest asks for them.
#[derive(Debug, Default)]
pub(crate) struct RetailFar {
    /// The trees' models, in a fixed order, once worked out.
    models: Mutex<Option<Arc<Vec<dereth_remaster::scene::FarPackModel>>>>,
    /// The pictures drawn, by the model's place.
    drawn: Mutex<HashMap<usize, [TextureData; 2]>>,
    /// The models whose pictures have been asked for.
    asked: Mutex<HashSet<usize>>,
    /// Where a model's pictures are asked for.
    ask: Mutex<Option<mpsc::Sender<usize>>>,
    /// How many asks the thread has answered, drawn or not.
    answered: Mutex<usize>,
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
        let (send, receive) = mpsc::channel::<usize>();
        *me.ask.lock().unwrap_or_else(PoisonError::into_inner) = Some(send);
        let held = Arc::clone(&me);
        let spawned = crate::workers::spawn("retail far trees", move || {
            let mut assets =
                DatAssets::new(&store, 0, dereth_render::texture::ImageScale::default());
            let mut models = Vec::new();
            let mut ids = Vec::new();
            for id in scenery_objects(&store, &region) {
                let Some((shape, true)) = shape_of(&mut assets, &store, id, false) else {
                    continue;
                };
                if let Some(m) = model_of(id, &shape) {
                    models.push(m);
                    ids.push(id);
                }
            }
            *held.models.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(models));
            while let Ok(k) = receive.recv() {
                let Some(&id) = ids.get(k) else {
                    continue;
                };
                let baked = shape_of(&mut assets, &store, id, true).and_then(|(s, _)| bake(id, &s));
                if let Some((_, pictures)) = baked {
                    held.drawn
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(k, pictures);
                }
                *held.answered.lock().unwrap_or_else(PoisonError::into_inner) += 1;
            }
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

    /// Wait until the trees are worked out and every picture asked for is drawn: for a capture
    /// that draws them from its first frames. It gives up after two minutes.
    pub(crate) fn wait(&self) {
        let started = web_time::Instant::now();
        while started.elapsed() < std::time::Duration::from_secs(120) {
            let ready = self
                .models
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_some();
            let asked = self
                .asked
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .len();
            let answered = *self.answered.lock().unwrap_or_else(PoisonError::into_inner);
            if ready && answered >= asked {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// The `k`th tree's pictures, once drawn: asked for the first time they are wanted, and kept,
    /// so a renderer that starts over is handed them again.
    fn pictures(&self, k: usize) -> Option<[TextureData; 2]> {
        if let Some(p) = self
            .drawn
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&k)
        {
            return Some(p.clone());
        }
        if self
            .asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(k)
        {
            if let Some(ask) = self
                .ask
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
            {
                let _ = ask.send(k);
            }
        }
        None
    }
}

/// The far pictures of an art pack's trees and of retail's, the pack's models first.
#[derive(Debug)]
pub(crate) struct FarPictures {
    pub(crate) pack: Option<Arc<dyn dereth_remaster::scene::FarPictures>>,
    pub(crate) pack_models: usize,
    pub(crate) retail: Arc<RetailFar>,
    /// Each retail model's place among retail's trees.
    pub(crate) retail_places: Vec<usize>,
}

impl dereth_remaster::scene::FarPictures for FarPictures {
    fn pictures(&self, model: usize) -> Option<[TextureData; 2]> {
        if model < self.pack_models {
            return self.pack.as_ref()?.pictures(model);
        }
        self.retail
            .pictures(*self.retail_places.get(model - self.pack_models)?)
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
                .filter(|m| m.lods.iter().any(|l| l.views.is_some()))
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

/// The far pictures the remaster draws: the art pack's (`pack`, its far pictures), then retail's
/// trees the pack draws none of.
pub(crate) fn far_pack(
    pack: Option<dereth_remaster::scene::FarPack>,
    retail: Option<&Arc<RetailFar>>,
) -> Option<dereth_remaster::scene::FarPack> {
    let models = retail.and_then(|r| r.models());
    let (Some(retail), Some(retail_models)) = (retail, models) else {
        return pack;
    };
    let pack_serial = pack.as_ref().map(|p| p.serial);
    let (mut models, pictures, pack_models) = match pack {
        Some(p) => {
            let n = p.models.len();
            (p.models, Some(p.pictures), n)
        }
        None => (Vec::new(), None, 0),
    };
    let drawn: HashSet<u32> = models.iter().map(|m| m.id).collect();
    let mut retail_places = Vec::new();
    for (k, m) in retail_models.iter().enumerate() {
        if !drawn.contains(&m.id) {
            models.push(m.clone());
            retail_places.push(k);
        }
    }
    Some(dereth_remaster::scene::FarPack {
        serial: set_serial(pack_serial, Some(retail_models.len())),
        models,
        pictures: Arc::new(FarPictures {
            pack: pictures,
            pack_models,
            retail: Arc::clone(retail),
            retail_places,
        }),
    })
}
