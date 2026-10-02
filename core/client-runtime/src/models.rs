//! Turning a `GfxObj` into triangles.
//!
//! **This is a gap between the object and rendering modules.** The object draw path decides which
//! part to draw, in what order, and at what degrade level, while the animation code owns the part
//! array; neither turns a decoded `GfxObj`'s polygon list into vertices. `RenderBackend`
//! accepts `MeshData`, but no object path produces it. Scenery still needs those vertices, so this
//! module builds them minimally.
//! The long-term owner is the object draw path; once it performs this
//! conversion, this file can go away.
//!
//! What it does is the positive-side half of drawing a part: a polygon is an **n-gon** and is
//! emitted as the triangle fan `(0, i, i+1)`; its `pos_surface` indexes the object's own
//! surface list; its `pos_uv_indices` index the *per-vertex* UV array, because a vertex carries
//! one UV pair per polygon corner it participates in.
//!
//! Not done here, and named rather than hidden:
//!
//! * the negative side of a `sides_type == 2` polygon (`neg_surface`, `neg_uv_indices`);
//! * per-part degrade levels and billboarding (`objects::degrade`);
//! * per-vertex lighting — the legacy shader set has no fixed-function lighting at all, so object
//!   vertices carry opaque white and the texture is the whole colour.

use std::collections::HashMap;

use dereth_assets::{Decode, GfxObj, Setup};
use dereth_dat::{divine_type, DbType, RetailDatStore};
use dereth_primitives::{DataId, Frame, Quat, Vec3};

/// One drawable piece of a setup: a `GfxObj` and where it sits in the object's own space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPart {
    pub gfxobj: DataId,
    pub frame: Frame,
}

/// The placement the client actually installs — `0x65`, 101, ACE's `Placement.Resting`
/// (ACE's `Source/ACE.Entity/Enum/Placement.cs`).
///
/// Both final object initialization and part-array setup install placement `0x65`, and nothing
/// between them asks for `0`, so
/// `0x65` — not `0` — is the pose an object is drawn at. Part-array setup does not install `0`,
/// and over the training dungeon's 557 setup loads 62 of the block's 170 distinct setups carry two
/// *different* frames at the two keys, so the difference is visible rather than academic.
pub const PLACEMENT_RESTING: u32 = 0x65;

/// ACE's `Placement.Default`, which is the placement lookup's **fallback**
/// key and not its first choice.
///
/// The fallback is load-bearing, not decoration: 86 of the training dungeon's 170 setups carry a
/// placement at `0` and none at `0x65`. The one-part setup the client wraps a bare
/// `0x01……` `GfxObj` in also stores its single entry here.
pub const PLACEMENT_DEFAULT: u32 = 0;

/// the requested id, then key `0`, then nothing.
///
/// Retail hashes the requested id and, on a miss, re-hashes the literal `0`; only when *that*
/// misses does it leave the parts at their existing setup frames, which are the identity. So "no
/// entry at either key" and "the
/// identity" are the same pose, and that is what an absent return means here.
///
/// [`crate::object_physics::setup_geometry_with_parts`] does the same lookup for the collision
/// side; the two are asserted to agree in `tests/dat/objects/resting_placement_pose.rs`.
#[must_use]
pub fn placement_frames(setup: &Setup, id: u32) -> Option<&[Frame]> {
    setup
        .placement_frames
        .get(&id)
        .or_else(|| setup.placement_frames.get(&PLACEMENT_DEFAULT))
        .map(|p| p.frames.as_slice())
}

/// **where a held object is**.
///
/// A wielded weapon has no position of its own: attachment
/// calls `leave_world` on it and from then on its frame is derived from its holder, once per
/// frame, by the child-update pass. The derivation is three steps and every one of
/// them matters:
///
/// 1. `pos` is the holder's part `part_id` position when `part_id` is below the holder's part
///    count, and otherwise the holder's own position;
/// 2. `out = combine(pos.frame, holding.frame)`;
/// 3. the child's frame is set to `out`.
///
/// `part_id` and the holding frame are the `LocationType` returned for the `ParentLocation` the
/// server named — read off the **holder's** setup,
/// which is why a creature setup with no entry at that key cannot hold anything there: the
/// location lookup returns 0 and `set_parent` refuses.
///
/// `holder_parts` are the holder's last updated part frames
/// — animated, so a swung sword follows the hand — and `holder_root` is its
/// physics-position frame, which is the fallback the client takes when the holding
/// location names a part index the setup does not have.
///
/// The composition is [`dereth_animation::frame::combine`]; there is no
/// second transcription of it here.
#[must_use]
pub fn child_frame(
    holder_root: &Frame,
    holder_parts: &[Frame],
    holding: &dereth_animation::data::LocationEntry,
) -> Frame {
    let base = holder_parts
        .get(holding.part_id as usize)
        .unwrap_or(holder_root);
    dereth_animation::frame::combine(base, &holding.frame)
}

/// One surface's worth of an object's triangles, in object space.
#[derive(Debug, Clone, Default)]
pub struct SurfaceGroup {
    /// The surface (dat type `0x08`) id, from the object's own surface list.
    pub surface: Option<DataId>,
    /// `sides_type == 1` on any polygon of the group.
    pub two_sided: bool,
    /// `stippling & STIPPLE_TILED` — the bit surface setup reads as its tiled flag.
    pub tiled: bool,
    /// `(position, u, v)` per vertex, three per triangle, in fan order.
    pub vertices: Vec<(Vec3, f32, f32)>,
    /// A normal for each entry of [`Self::vertices`], same order and
    /// length -- the `D3DFVF_NORMAL` of the `0x152`/`0x252` vertex format that mesh construction
    /// emits, which the fixed-function lighting reads; without it the lighting lights nothing.
    pub normals: Vec<Vec3>,
}

/// The surface renderer's tiling bit. `STIPPLE_NO_POS_UVS` is 4 and `STIPPLE_NO_NEG_UVS` is 8
/// (`dereth_assets::common::Polygon`), so bit 0 is the tiled flag used during surface setup.
pub const STIPPLE_TILED: u8 = 1;

/// The triangulation uses index zero when the polygon's UV-index array is
/// absent. Retail's vertex copy treats an authored index as a signed byte and emits
/// zero coordinates only when that index is negative/out of range or no vertex UVs
/// exist. An absent *index array* does not discard the vertex's first UV pair.
fn mesh_uv(indices: Option<&[u8]>, corner: usize, uvs: &[(f32, f32)]) -> (f32, f32) {
    let index = match indices {
        None => 0,
        Some(indices) => match indices.get(corner) {
            Some(&index) if index < 128 => usize::from(index),
            _ => return (0.0, 0.0),
        },
    };
    uvs.get(index).copied().unwrap_or((0.0, 0.0))
}

/// Resolve a scenery to the parts that draw it, at the **installed** pose.
///
/// A scene object is either a `Setup` (`0x02`), whose `parts` are `GfxObj` ids placed by the
/// **resting** placement's frames ([`PLACEMENT_RESTING`], falling back to [`PLACEMENT_DEFAULT`]),
/// or a bare `GfxObj` (`0x01`), which is a single part at the identity.
/// Object creation accepts both.
///
/// Scenery, cell statics and the sky are never told a placement by anyone, so this is
/// [`resolve_parts_at`] with the id installed during part-array setup. An object the
/// **server** names a placement for goes through [`resolve_parts_at`].
#[must_use]
pub fn resolve_parts(store: &RetailDatStore, id: DataId) -> Vec<ModelPart> {
    resolve_parts_at(store, id, PLACEMENT_RESTING)
}

/// [`resolve_parts`] at a **named** placement — the id passed to the placement setter
/// rather than the one installed during setup.
///
/// The fallback chain is unchanged and is [`placement_frames`]'s: the named id, then key `0`, then
/// the identity. A bare `GfxObj` has no setup and therefore no placement at all, so `placement`
/// does not reach it — matching the placement setter's null-`setup` guard.
#[must_use]
pub fn resolve_parts_at(store: &RetailDatStore, id: DataId, placement: u32) -> Vec<ModelPart> {
    match divine_type(id) {
        Some(DbType::GfxObj) => vec![ModelPart {
            gfxobj: id,
            frame: identity(),
        }],
        Some(DbType::Setup) => {
            let Ok(bytes) = store.read_typed(DbType::Setup, id) else {
                return Vec::new();
            };
            let Ok(setup) = Setup::decode_payload_in(store.era_of(id), id, &bytes) else {
                return Vec::new();
            };
            let frames = placement_frames(&setup, placement);
            setup
                .parts
                .iter()
                .enumerate()
                .map(|(i, &gfxobj)| ModelPart {
                    gfxobj,
                    frame: frames
                        .and_then(|f| f.get(i))
                        .copied()
                        .unwrap_or_else(identity),
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// The picture each surface of a `GfxObj` draws, slot by slot: each surface's own texture id, or
/// `None` for a surface with none. `None` when the object will not read or decode.
#[must_use]
pub fn surface_textures(store: &RetailDatStore, gfxobj: DataId) -> Option<Vec<Option<DataId>>> {
    let bytes = store.read_typed(DbType::GfxObj, gfxobj).ok()?;
    let g = GfxObj::decode_payload_in(store.era_of(gfxobj), gfxobj, &bytes).ok()?;
    Some(
        g.surfaces
            .iter()
            .map(|&s| {
                let b = store.read_typed(DbType::Surface, s).ok()?;
                dereth_assets::Surface::decode_payload_in(store.era_of(s), s, &b)
                    .ok()?
                    .orig_texture_id
            })
            .collect(),
    )
}

/// An object's parts as drawn with another era's look (`look`) while its world is `world`: the
/// same parts, with each texture change carried onto the look's own pictures.
///
/// A texture change replaces a picture on a part by matching the part's surfaces by their picture
/// id, and the eras painted the same part with different picture ids (the later files repainted
/// the human body, so a world's shirt replaces the older arm's picture, which the later arm does
/// not carry). A change whose old picture the look's part does not carry, but the world's part
/// does, goes to the same surface slot of the look's part: the slot the world's part has that
/// picture in. That is what the look's own clothing tables name for the same garment.
///
/// `None` when a change cannot be placed that way (the two eras' part list another number of
/// surfaces): the object is then drawn wholly with the world's records. A part either era cannot
/// read keeps its changes as they are.
#[must_use]
pub fn parts_for_look(
    world: &RetailDatStore,
    look: &RetailDatStore,
    parts: &[dereth_animation::parts::PhysicsPart],
) -> Option<Vec<dereth_animation::parts::PhysicsPart>> {
    let mut out = parts.to_vec();
    for p in &mut out {
        let Some(ov) = p.surface_overrides.as_mut() else {
            continue;
        };
        if ov.texture_maps.is_empty() {
            continue;
        }
        let (Some(w), Some(v)) = (
            surface_textures(world, p.gfxobj_id),
            surface_textures(look, p.gfxobj_id),
        ) else {
            continue;
        };
        let mut maps: Vec<(DataId, DataId)> = Vec::with_capacity(ov.texture_maps.len());
        for &(old, new) in &ov.texture_maps {
            if v.contains(&Some(old)) || !w.contains(&Some(old)) {
                maps.push((old, new));
                continue;
            }
            if w.len() != v.len() {
                return None;
            }
            for (slot, picture) in w.iter().enumerate() {
                if *picture != Some(old) {
                    continue;
                }
                if let Some(there) = v[slot] {
                    if !maps.iter().any(|(o, _)| *o == there) {
                        maps.push((there, new));
                    }
                }
            }
        }
        ov.texture_maps = maps;
    }
    Some(out)
}

fn identity() -> Frame {
    Frame::new(Vec3::ZERO, Quat::IDENTITY)
}

/// Suppress drawing when the selected degrade level has no mesh, evaluated at the **near band**,
/// which is the only band a baked mesh can stand for.
///
/// A `GfxObj` whose `flags & 8` is set names a `GfxObjDegradeInfo`, and what draws it is not the
/// object itself but `degrades[deg_level].gfxobj_id`, chosen every frame by
/// the part's viewer-distance update. The renderer pins
/// its degrade distance at 50.0, and degradation selection opens with
/// `d = (|distance| >= degrade_distance) ? distance : 0.0`, so **every object within 50 m is
/// selected at `d = 0`** — one answer, independent of where in that band the viewer stands, and
/// therefore an answer a bake may take once.
///
/// Eleven of the retail dat's 4,131 degrade records are `[self, min = ideal = max = 0.0]` followed
/// by the all-`FLT_MAX` terminator. `d < 0.0` is false for every distance and every bias, so the
/// terminator is selected always, and `gfxobj_id == 0` means **draw nothing** — never, not
/// merely far away. Those eleven are the level designers' markers: a 5 cm triangle at translucency
/// 1.0, a 5 x 5 m plate, a 12 x 40 m plane, and `0x010028CA`, the 2 m red-and-green cone that
/// stood in the Holtburg training academy's library. They are *placed* in the cell data and are
/// *never drawn*, which is why a paired retail frame of that room is empty.
///
/// Only the **draw** is refused. Static-object initialization still creates the physics
/// object, so a marker that is solid stays solid: this guard belongs to drawing,
/// not object creation.
#[must_use]
pub fn draws_at_near_band(store: &RetailDatStore, gfxobj: DataId) -> bool {
    use dereth_animation::data::DegradeInfo;
    use dereth_animation::parts::{get_degrade, DegradeSettings};

    let Ok(bytes) = store.read_typed(DbType::GfxObj, gfxobj) else {
        return true;
    };
    let Ok(obj) = GfxObj::decode_payload_in(store.era_of(gfxobj), gfxobj, &bytes) else {
        return true;
    };
    // With no degrade record, the part holds one mesh and always draws it.
    let Some(did) = obj.did_degrade else {
        return true;
    };
    let Ok(bytes) = store.read_typed(DbType::DegradeInfo, did) else {
        return true;
    };
    let Ok(info) =
        dereth_assets::GfxObjDegradeInfo::decode_payload_in(store.era_of(did), did, &bytes)
    else {
        return true;
    };
    // The shipped settings, with the automatic multiplier at its pinned 0: at `d = 0` the first
    // level whose ideal distance is above zero is the one drawn.
    let info = DegradeInfo {
        degrades: info.degrades,
    };
    let (level, _) = get_degrade(&info, 0.0, DegradeSettings::default());
    // A selected level whose mesh id is zero draws nothing.
    usize::try_from(level)
        .ok()
        .and_then(|i| info.degrades.get(i))
        .is_some_and(|e| e.gfxobj_id.0 != 0)
}

/// Return the setup's sorting sphere, or a static zero sphere if no setup exists.
///
/// This is the fifth scenery filter's input (`obj_within_block`): drawing asks for it rather than
/// computing it, because part arrays and collision spheres have separate owners. A bare
/// `GfxObj` has no setup and therefore no sphere, which is the zero-sphere case.
#[must_use]
pub fn sorting_sphere(store: &RetailDatStore, id: DataId) -> Option<(Vec3, f32)> {
    if divine_type(id) != Some(DbType::Setup) {
        return None;
    }
    let bytes = store.read_typed(DbType::Setup, id).ok()?;
    let setup = Setup::decode_payload_in(store.era_of(id), id, &bytes).ok()?;
    Some((setup.sorting_sphere.center, setup.sorting_sphere.radius))
}

/// Triangulate one `GfxObj`'s **drawing** polygons, grouped by positive surface.
///
/// Objects whose `flags & 2` is clear carry no drawing polygons at all — they are physics-only —
/// and come back empty, matching physics-part construction.
#[must_use]
pub fn build_gfxobj(store: &RetailDatStore, id: DataId) -> Vec<SurfaceGroup> {
    let Ok(bytes) = store.read_typed(DbType::GfxObj, id) else {
        return Vec::new();
    };
    let Ok(obj) = GfxObj::decode_payload_in(store.era_of(id), id, &bytes) else {
        return Vec::new();
    };
    triangulate(&obj)
}

/// [`build_gfxobj`]'s pure half, so it can be driven from a decoded object in a test.
#[must_use]
pub fn triangulate(obj: &GfxObj) -> Vec<SurfaceGroup> {
    // ORDER-OK: keyed by the object's own surface index and drained through `order` below, which is
    // first-appearance order, so nothing observable depends on the map's iteration.
    let mut groups: HashMap<u16, SurfaceGroup> = HashMap::new();
    let mut order: Vec<u16> = Vec::new();

    for p in &obj.polygons {
        if p.num_pts < 3 {
            continue;
        }
        let g = groups.entry(p.pos_surface).or_insert_with(|| {
            order.push(p.pos_surface);
            SurfaceGroup {
                surface: obj.surfaces.get(p.pos_surface as usize).copied(),
                ..SurfaceGroup::default()
            }
        });
        g.two_sided |= p.sides_type == 1;
        g.tiled |= p.stippling & STIPPLE_TILED != 0;

        let n = usize::from(p.num_pts);
        // The fan a polygon draws as: (0, i, i+1) for i in 1..n-1.
        for i in 1..n - 1 {
            for &k in &[0usize, i, i + 1] {
                let Some(&vi) = p.vertex_ids.get(k) else {
                    continue;
                };
                let Some(v) = obj.vertex_array.vertices.get(vi as usize) else {
                    continue;
                };
                let uv = mesh_uv(p.pos_uv_indices.as_deref(), k, &v.uvs);
                g.vertices.push((v.position, uv.0, uv.1));
                g.normals.push(v.normal);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|k| groups.remove(&k))
        .filter(|g| !g.vertices.is_empty())
        .collect()
}

/// Triangulate one `CellStruct`'s drawing polygons, grouped by positive surface.
///
/// Identical in shape to [`triangulate`], because it is the same polygon fan: an
/// env cell's mesh is built against the **env cell's** own surface
/// list rather than a gfx object's: the cell path selects the cell's surfaces before drawing the
/// constructed mesh.
///
/// The portal polygons are **not** excluded: the portal list holds indices into
/// `polygons`, and the same polygons are both drawn and used as portal openings.
#[must_use]
pub fn triangulate_cell(
    cs: &dereth_assets::geometry::CellStruct,
    surfaces: &[DataId],
) -> Vec<SurfaceGroup> {
    // ORDER-OK: keyed by the cell's own surface index and drained through `order` below, which is
    // first-appearance order, so nothing observable depends on the map's iteration.
    let mut groups: HashMap<u16, SurfaceGroup> = HashMap::new();
    let mut order: Vec<u16> = Vec::new();

    for p in &cs.polygons {
        if p.num_pts < 3 {
            continue;
        }
        let g = groups.entry(p.pos_surface).or_insert_with(|| {
            order.push(p.pos_surface);
            SurfaceGroup {
                surface: surfaces.get(p.pos_surface as usize).copied(),
                ..SurfaceGroup::default()
            }
        });
        g.two_sided |= p.sides_type == 1;
        g.tiled |= p.stippling & STIPPLE_TILED != 0;

        let n = usize::from(p.num_pts);
        for i in 1..n - 1 {
            for &k in &[0usize, i, i + 1] {
                let Some(&vi) = p.vertex_ids.get(k) else {
                    continue;
                };
                let Some(v) = cs.vertex_array.vertices.get(vi as usize) else {
                    continue;
                };
                let uv = mesh_uv(p.pos_uv_indices.as_deref(), k, &v.uvs);
                g.vertices.push((v.position, uv.0, uv.1));
                g.normals.push(v.normal);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|k| groups.remove(&k))
        .filter(|g| !g.vertices.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the retail dat files. Every graphics object with drawing polygons must triangulate to a
    // whole number of triangles whose vertex indices are all in range -- checked over a slice of
    // the retail object set rather than a single hand-picked object, because a fan built off the
    // wrong index array still produces triangles.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn retail_objects_triangulate_into_whole_triangles_with_in_range_surfaces() {
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        let ids = store.ids_of(DbType::GfxObj);
        assert!(!ids.is_empty(), "the portal dat has GfxObjs");
        let mut with_geometry = 0usize;
        // Every 64th object: about 1,200 of them, which is enough to meet every polygon shape and
        // still run in a second.
        for &id in ids.iter().step_by(64) {
            let bytes = store.read_typed(DbType::GfxObj, id).expect("reads");
            let obj = GfxObj::decode_payload(id, &bytes).expect("decodes");
            let groups = triangulate(&obj);
            if groups.is_empty() {
                assert!(
                    obj.polygons.is_empty(),
                    "{id} has polygons but produced no triangles"
                );
                continue;
            }
            with_geometry += 1;
            for g in &groups {
                assert_eq!(g.vertices.len() % 3, 0, "{id} emitted a partial triangle");
                if let Some(s) = g.surface {
                    assert!(
                        obj.surfaces.contains(&s),
                        "{id} names a surface it does not own"
                    );
                }
            }
            // A fan over an n-gon is n-2 triangles; the totals must agree.
            let expected: usize = obj
                .polygons
                .iter()
                .map(|p| usize::from(p.num_pts).saturating_sub(2))
                .sum();
            let got: usize = groups.iter().map(|g| g.vertices.len() / 3).sum();
            assert_eq!(got, expected, "{id} fan count");
        }
        assert!(
            with_geometry > 100,
            "only {with_geometry} objects had drawing polygons"
        );
    }

    // Oracle: the retail dat files, through the scene the region actually points at. A scenery
    // object id must resolve to at least one part with geometry, or Dereth would be bare.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_retail_scene_object_resolves_to_parts_with_geometry() {
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        let scenes = store.ids_of(DbType::Scene);
        assert!(!scenes.is_empty());
        let mut drawable = 0usize;
        for &sid in scenes.iter().take(40) {
            let bytes = store.read_typed(DbType::Scene, sid).expect("reads");
            let scene = dereth_assets::Scene::decode_payload(sid, &bytes).expect("decodes");
            for o in &scene.objects {
                for part in resolve_parts(&store, o.obj_id) {
                    if !build_gfxobj(&store, part.gfxobj).is_empty() {
                        drawable += 1;
                    }
                }
            }
        }
        assert!(
            drawable > 0,
            "no scene object in the first 40 scenes had drawable geometry"
        );
    }
}
