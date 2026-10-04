//! Across the whole panel both faces stop a body at the same distance; an open/close cycle leaves
//! both surfaces unchanged; the head-on collapse is symmetric; no near-square heading passes;
//! shallow slides jam in a face-dependent band; the closed pose comes from the animation; the
//! 45-degree glide agrees on both faces.
//! Fixture: the shipped retail DAT records and recorded inputs.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use dereth_assets::{CellLandblock, Decode, GfxObj, Region, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::geom::bsp::{BspNode, BspNodeKind, BspTree};
use dereth_physics::geom::{Plane, PlaneExt, Polygon, Sphere};
use dereth_physics::source::SetupPart;
use dereth_physics::{
    landdefs, LandSource, LandblockCollision, PhysHandle, PhysicsWorld, SetupGeometry,
    StaticLandSource, V3,
};
use dereth_primitives::num::math;
use dereth_primitives::{DataId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

/// The region the landscape definitions read their variables from.
const REGION_ID: DataId = DataId(0x1300_0000);
/// Holtburg â€” used only as a patch of real terrain to stand the door on.
const HOLTBURG: LandblockId = LandblockId(0xA9B4);
/// The training-academy door setup. The literal, so a wrong id cannot hide behind a symbol.
const DOOR_SETUP: DataId = DataId(0x0200_024F);
const DOOR_MTABLE: DataId = DataId(0x0900_0016);
/// The animation named by both cycles — frame 0 closed, frame 30 open.
const DOOR_ANIM: DataId = DataId(0x0300_0559);
const PLACEMENT_FRAME_DEFAULT: u32 = 0x65;
/// The body's collision sphere radius â€” what a body stopped by a mesh rests off it.
const BODY_RADIUS: f32 = 0.5;

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are required: set DERETH_TEST_DAT_DIR (looked in {})",
            dereth_dat::testing::dat_dir().display()
        )
    })
}

fn convert_bsp(g: &GfxObj) -> Option<BspTree> {
    let src = g.physics_bsp.as_ref()?;
    let mut by_id: HashMap<i16, u32> = HashMap::new();
    let mut polygons = Vec::with_capacity(g.physics_polygons.len());
    for (i, p) in g.physics_polygons.iter().enumerate() {
        let verts: Vec<Vec3> = p
            .vertex_ids
            .iter()
            .map(|&vi| g.vertex_array.vertices[vi as usize].position)
            .collect();
        by_id.insert(p.poly_id, u32::try_from(i).expect("polygon index fits"));
        polygons.push(Polygon::new(verts));
    }
    let mut nodes = Vec::with_capacity(src.nodes.len());
    for n in &src.nodes {
        let kind = if let Some(l) = n.leaf_index {
            BspNodeKind::Leaf {
                leaf_index: i32::try_from(l).unwrap_or(-1),
                solid: n.solid == Some(1),
            }
        } else if n.tag == 0x504F_5254 {
            BspNodeKind::Portal
        } else {
            BspNodeKind::Node
        };
        nodes.push(BspNode {
            sphere: n
                .sphere
                .map_or_else(Sphere::default, |s| Sphere::new(s.center, s.radius)),
            splitting_plane: n.plane.map_or_else(Plane::default, |p| Plane {
                normal: p.normal,
                d: p.d,
            }),
            pos_child: n.pos_child,
            neg_child: n.neg_child,
            kind,
            in_polys: n
                .in_polys
                .iter()
                .filter_map(|id| by_id.get(id).copied())
                .collect(),
        });
    }
    Some(BspTree { nodes, polygons })
}

/// A patch of Holtburg's terrain to stand the door on.
fn holtburg(store: &RetailDatStore) -> StaticLandSource {
    let bytes = store
        .read_typed(DbType::Region, REGION_ID)
        .expect("region 0x13000000");
    let region = Region::decode_payload(REGION_ID, &bytes).expect("the region decodes");
    let table = landdefs::validate_height_table(&region.land_defs.land_height_table)
        .expect("the retail height table passes set_height_table");
    let mut land = StaticLandSource::new(table);
    let did = DataId((u32::from(HOLTBURG.0) << 16) | 0xFFFF);
    let bytes = store
        .read_typed(DbType::LandBlock, did)
        .expect("Holtburg's landblock record");
    let lb = CellLandblock::decode_payload(did, &bytes).expect("the landblock decodes");
    land.add_block(
        LandblockCollision::build(
            HOLTBURG,
            Box::new(lb.height),
            Box::new(lb.terrain),
            lb.lbi_exists != 0,
            8,
            &table,
        )
        .expect("a full-detail block"),
    );
    land
}

/// The door's `SetupGeometry`, parts and all â€” `setup_geometry_with_parts`'s work, written here
/// because `dereth-physics` cannot depend on the application.
fn door_geometry(store: &RetailDatStore) -> SetupGeometry {
    door_geometry_posed(store, None)
}

fn door_geometry_posed(store: &RetailDatStore, frames: Option<&[Frame]>) -> SetupGeometry {
    let bytes = store
        .read_typed(DbType::Setup, DOOR_SETUP)
        .expect("the door setup");
    let s = Setup::decode_payload(DOOR_SETUP, &bytes).expect("the setup decodes");
    let fallback = s
        .placement_frames
        .get(&PLACEMENT_FRAME_DEFAULT)
        .or_else(|| s.placement_frames.get(&0))
        .expect("the door setup carries a placement frame");
    let placement: &[Frame] = frames.unwrap_or(&fallback.frames);
    let sphere = |x: dereth_assets::Sphere| Sphere::new(x.center, x.radius);
    let mut g = SetupGeometry {
        sorting_sphere: sphere(s.sorting_sphere),
        selection_sphere: sphere(s.selection_sphere),
        spheres: s.spheres.iter().copied().map(sphere).collect(),
        cyl_spheres: s
            .cylspheres
            .iter()
            .map(|c| dereth_physics::geom::CylSphere {
                low_pt: c.low_pt,
                radius: c.radius,
                height: c.height,
            })
            .collect(),
        step_up_height: s.step_up_height,
        step_down_height: s.step_down_height,
        radius: s.radius,
        height: s.height,
        has_physics_bsp: s.has_physics_bsp,
        physics_bsp: None,
        allow_free_heading: s.allow_free_heading,
        parts: Vec::new(),
    };
    let mut parts = Vec::with_capacity(s.parts.len());
    for (i, part_id) in s.parts.iter().enumerate() {
        let Some(frame) = placement.get(i).copied() else {
            break;
        };
        let bsp = store
            .read_typed(DbType::GfxObj, *part_id)
            .ok()
            .and_then(|b| GfxObj::decode_payload(*part_id, &b).ok())
            .and_then(|gfx| convert_bsp(&gfx))
            .map(Arc::new);
        parts.push(SetupPart {
            placement_frame: frame,
            default_scale: s
                .default_scale
                .as_ref()
                .and_then(|d| d.get(i).copied())
                .unwrap_or(Vec3::new(1.0, 1.0, 1.0)),
            physics_bsp: bsp,
            bound_box: None,
            drawing_sphere: None,
            first_degrade_mode: None,
        });
    }
    g.parts = parts;
    g
}

/// The terrain height under a block-local `(x, y)`, off the same polygon
///  would stand a body on.
fn ground_at(land: &StaticLandSource, x: f32, y: f32) -> Option<f32> {
    let block = land.landblock(HOLTBURG)?;
    let mut cell = HOLTBURG.cell(1);
    let mut o = Vec3::new(x, y, 0.0);
    if !landdefs::adjust_to_outside(&mut cell, &mut o) {
        return None;
    }
    let poly = block.find_terrain_poly(cell.index(), o)?;
    let mut p = Vec3::new(x, y, 0.0);
    poly.plane.set_height(&mut p).then_some(p.z)
}

fn player_geometry() -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, BODY_RADIUS), BODY_RADIUS)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, BODY_RADIUS), 1.0),
        step_up_height: 0.3,
        step_down_height: 0.3,
        radius: BODY_RADIUS,
        height: 1.0,
        ..SetupGeometry::default()
    })
}

fn spawn(w: &mut PhysicsWorld, id: u32, at: Vec3, per_substep: Vec3) -> PhysHandle {
    let h = w.create(ObjectId(id), player_geometry(), true);
    let mut cell = HOLTBURG.cell(1);
    let mut origin = at;
    landdefs::adjust_to_outside(&mut cell, &mut origin);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(cell, Frame::new(at, Quat::IDENTITY));
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            vec![Frame::new(per_substep, Quat::IDENTITY); 400],
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    h
}

/// The door, placed as a **static** object on the terrain at `at`.
fn place_door(w: &mut PhysicsWorld, geom: &Arc<SetupGeometry>, at: Vec3) -> (PhysHandle, Position) {
    let h = w.create(ObjectId(0x7000_0009), Arc::clone(geom), false);
    let mut cell = HOLTBURG.cell(1);
    let mut origin = at;
    landdefs::adjust_to_outside(&mut cell, &mut origin);
    w.enter_cell(h, cell);
    let pos = Position::new(cell, Frame::new(at, Quat::IDENTITY));
    {
        let o = w.get_mut(h).expect("live");
        o.position = pos;
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    (h, pos)
}

///  on every part: is a sphere of `radius` centred at
/// `p` inside the door's own mesh?
fn inside_door(g: &SetupGeometry, pos: &Position, p: Vec3, radius: f32) -> bool {
    (0..g.parts.len()).any(|i| {
        let Some(part) = g.placed_part(i, pos, 1.0) else {
            return false;
        };
        let Some(tree) = part.physics_bsp.as_ref() else {
            return false;
        };
        let inv = 1.0 / part.gfxobj_scale;
        let m = dereth_physics::math::l2g(part.pos.frame.rotation);
        let local = dereth_physics::math::globaltolocalvec(m, p.sub(part.pos.frame.origin));
        tree.sphere_intersects_solid(&Sphere::new(local.mul(inv), radius * inv), true)
    })
}

fn slab_axis_local(tree: &BspTree) -> Vec3 {
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for p in &tree.polygons {
        for v in &p.vertices {
            lo = Vec3::new(lo.x.min(v.x), lo.y.min(v.y), lo.z.min(v.z));
            hi = Vec3::new(hi.x.max(v.x), hi.y.max(v.y), hi.z.max(v.z));
        }
    }
    // Only the horizontal axes: a door is thin in x or y, never in z.
    if hi.x - lo.x <= hi.y - lo.y {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    }
}

struct Side {
    /// Which way is "out" from the slab on this side.
    out: Vec3,
    /// Where the body starts, on the terrain.
    start: Vec3,
    /// Per-sub-step offset: twenty degrees off `out`, reflected through the slab plane so the two
    /// arms are true mirror images.
    step: Vec3,
    face: f32,
}

/// What one approach produced.
struct Approach {
    /// The closest the body's centre ever got to this side's face, along the slab normal, while it
    /// was still in front of the panel. **The number the report is about.**
    ///
    /// Sampled per frame rather than read off the end of the run, because a body stopped by a wall
    /// keeps sliding along it and eventually goes round the edge -- reading the final position
    /// measures where it wandered to, not where it was stopped.
    closest: f32,
    /// How many frames the body spent in front of the panel -- the denominator. Zero means the
    /// approach never faced the door and its `closest` is not a measurement.
    facing_frames: u32,
    /// The furthest the body's centre travelled from its start.
    travel: f32,
}

fn walk(
    land: &Arc<StaticLandSource>,
    geom: &Arc<SetupGeometry>,
    door_at: Vec3,
    centre: Vec3,
    tangent: Vec3,
    half_width: f32,
    side: &Side,
) -> Approach {
    let mut w = PhysicsWorld::new(Arc::clone(land) as Arc<dyn LandSource>);
    let (_door, _pos) = place_door(&mut w, geom, door_at);
    let body = spawn(&mut w, 1, side.start, side.step);
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut closest = f32::MAX;
    let mut facing_frames = 0u32;
    let mut travel = 0.0f32;
    for _ in 0..75 {
        t += dt;
        w.use_time(LocalTime(t), false);
        let p = w.get(body).expect("live").position.frame.origin;
        travel = travel.max(math::hypotf(p.x - side.start.x, p.y - side.start.y));
        let rel = p.add(Vec3::new(0.0, 0.0, BODY_RADIUS)).sub(centre);
        if rel.dot(tangent).abs() <= half_width {
            facing_frames += 1;
            closest = closest.min(rel.dot(side.out) - side.face);
        }
    }
    Approach {
        closest,
        facing_frames,
        travel,
    }
}

struct Scene {
    land: Arc<StaticLandSource>,
    geom: Arc<SetupGeometry>,
    door_at: Vec3,
    mesh_centre: Vec3,
    normal: Vec3,
    tangent: Vec3,
    face_pos: f32,
    face_neg: f32,
    half_width: f32,
}

fn door_poses(store: &RetailDatStore) -> (Vec<Frame>, Vec<Frame>) {
    let mt = store
        .read_typed(DbType::MTable, DOOR_MTABLE)
        .expect("the door motion table");
    let mt = dereth_assets::MotionTable::decode_payload(DOOR_MTABLE, &mt).expect("it decodes");
    let cycle = |key: u32| -> (DataId, i32) {
        let c = mt.cycles.iter().find(|c| c.key == key).expect("the cycle");
        let a = c.anims.first().expect("the cycle names an animation");
        assert!(
            (a.framerate).abs() < f32::EPSILON,
            "cycle {key:#010X} is a held frame, not a playing animation"
        );
        (a.anim_id, a.low_frame)
    };
    let (closed_id, closed_frame) = cycle(0x003D_000C);
    let (open_id, open_frame) = cycle(0x003D_000B);
    assert_eq!(
        closed_id, DOOR_ANIM,
        "the closed cycle names the door animation"
    );
    assert_eq!(
        open_id, DOOR_ANIM,
        "the open cycle names the same animation"
    );
    let bytes = store
        .read_typed(DbType::Anim, DOOR_ANIM)
        .expect("the door animation");
    let anim = dereth_assets::Animation::decode_payload(DOOR_ANIM, &bytes).expect("it decodes");
    let at = |n: i32| -> Vec<Frame> {
        anim.part_frames[usize::try_from(n).expect("a non-negative frame number")]
            .frames
            .clone()
    };
    (at(closed_frame), at(open_frame))
}

fn scene() -> Scene {
    scene_posed(None)
}

/// [`scene`] with the door's parts placed by `frames` rather than by the setup's placement frame.
fn scene_posed(frames: Option<&[Frame]>) -> Scene {
    let store = store();
    let land = Arc::new(holtburg(&store));
    let geom = Arc::new(door_geometry_posed(&store, frames));

    // Calibration: this door is BSP-only. With a sphere or cylinder-sphere in the setup the walk
    // would be measuring a fallback shape and the whole test would mean nothing.
    assert!(
        geom.spheres.is_empty() && geom.cyl_spheres.is_empty() && geom.caches_physics_bsp(),
        "setup {:#010X} must be BSP-only: {} spheres, {} cylspheres, bsp={}",
        DOOR_SETUP.0,
        geom.spheres.len(),
        geom.cyl_spheres.len(),
        geom.caches_physics_bsp()
    );
    let tree = geom.parts[0]
        .physics_bsp
        .as_ref()
        .expect("part 0 carries the door mesh");
    let local_normal = slab_axis_local(tree);
    // The slab normal **in world space**: the part's placement rotation applied, flattened to the
    // horizontal plane. See `slab_axis_local` for why reading it in world space directly is wrong.
    let normal = {
        let m = dereth_physics::math::l2g(geom.parts[0].placement_frame.rotation);
        let mut n = dereth_physics::math::localtoglobalvec(m, local_normal);
        n.z = 0.0;
        assert!(!n.normalize_check_small(), "the slab normal is vertical");
        n
    };
    let tangent = Vec3::new(-normal.y, normal.x, 0.0);

    // A flat, clear patch of Holtburg to stand the door on.
    let door_at = {
        let mut best = None;
        for i in 0..40_i16 {
            for j in 0..40_i16 {
                let x = 20.0 + f32::from(i) * 3.0;
                let y = 20.0 + f32::from(j) * 3.0;
                let Some(z) = ground_at(&land, x, y) else {
                    continue;
                };
                // Flat within 5 cm over the whole corridor the two walks use.
                let flat = (-12..=12_i8).all(|k| {
                    let p = Vec3::new(x, y, 0.0).add(normal.mul(f32::from(k) * 0.5));
                    ground_at(&land, p.x, p.y).is_some_and(|h| (h - z).abs() < 0.05)
                }) && (-12..=12_i8).all(|k| {
                    let p = Vec3::new(x, y, 0.0).add(tangent.mul(f32::from(k) * 0.5));
                    ground_at(&land, p.x, p.y).is_some_and(|h| (h - z).abs() < 0.05)
                });
                if flat {
                    best = Some(Vec3::new(x, y, z));
                    break;
                }
            }
            if best.is_some() {
                break;
            }
        }
        best.expect("a flat clear patch of Holtburg's terrain")
    };
    let door_cell_pos = {
        let mut cell = HOLTBURG.cell(1);
        let mut o = door_at;
        landdefs::adjust_to_outside(&mut cell, &mut o);
        Position::new(cell, Frame::new(door_at, Quat::IDENTITY))
    };

    // The mesh's own centre in world space -- of part 0, pushed
    // through `SetupGeometry::placed_part`. The object's origin is not the door: this setup's part
    // frame offsets the slab by (0.88, -0.44, 1.365) and turns it by -150 degrees about z.
    let mesh_centre = {
        let part = geom
            .placed_part(0, &door_cell_pos, 1.0)
            .expect("part 0 is placed");
        let root = part
            .physics_bsp
            .as_ref()
            .expect("mesh")
            .root()
            .expect("root node");
        let m = dereth_physics::math::l2g(part.pos.frame.rotation);
        part.pos
            .frame
            .origin
            .add(dereth_physics::math::localtoglobalvec(
                m,
                root.sphere.center.mul(part.gfxobj_scale),
            ))
    };

    let probe = |dir: Vec3| -> f32 {
        let mut u = 0.0f32;
        let base = Vec3::new(mesh_centre.x, mesh_centre.y, door_at.z + BODY_RADIUS);
        while u < 4.0 {
            if !inside_door(&geom, &door_cell_pos, base.add(dir.mul(u)), 0.0) {
                return u;
            }
            u += 0.001;
        }
        u
    };
    let face_pos = probe(normal);
    let face_neg = probe(normal.mul(-1.0));
    let half_width = probe(tangent).min(probe(tangent.mul(-1.0)));
    println!(
        "door at {door_at:?}, mesh centre {mesh_centre:?}, slab normal {normal:?}; the mesh's own faces are {face_pos:.3} m and {face_neg:.3} m from its centre along that normal and its panel reaches {half_width:.3} m either side"
    );
    assert!(
        face_pos > 0.0 && face_neg > 0.0 && half_width > 0.5,
        "the mesh probe found no slab: faces {face_pos:.3} / {face_neg:.3}, half width {half_width:.3}"
    );
    Scene {
        land,
        geom,
        door_at,
        mesh_centre,
        normal,
        tangent,
        face_pos,
        face_neg,
        half_width,
    }
}

const START_DISTANCE: f32 = 1.5;
/// The two approach angles the head-on station compares, as named constants so that a mutation of
/// either is visible to the aim calibration inside the test as well as to the walk.
const HEAD_ON_DEG: f32 = 0.0;
const OBLIQUE_DEG: f32 = 20.0;

/// The two mirrored approaches at `deg` degrees off the slab normal. `deg == 0` is **head on**.
///
/// A reflection through the slab plane, not a point reflection: the normal component flips with the
/// side and the tangential component does not. Negating both makes the two arms meet the slab at
/// different angles and invalidates the symmetry comparison.
fn sides_at(s: &Scene, deg: f32) -> Vec<(&'static str, Side)> {
    let r = deg.to_radians();
    let (cos, sin, tan) = (math::cosf(r), math::sinf(r), math::tanf(r));
    [1.0f32, -1.0]
        .iter()
        .map(|&sign| {
            let out = s.normal.mul(sign);
            let face = if sign > 0.0 { s.face_pos } else { s.face_neg };
            // Offset the start against the tangential drift so the body crosses the slab plane at
            // the mesh's own centre rather than near an edge.
            let drift = (START_DISTANCE + face) * tan;
            let start_xy = s
                .mesh_centre
                .add(out.mul(START_DISTANCE + face))
                .sub(s.tangent.mul(drift * 0.5));
            let gz = ground_at(&s.land, start_xy.x, start_xy.y).expect("terrain under the start");
            let dir = out.mul(-cos).add(s.tangent.mul(sin));
            (
                if sign > 0.0 { "front" } else { "back" },
                Side {
                    out,
                    start: Vec3::new(start_xy.x, start_xy.y, gz),
                    step: dir.mul(0.12),
                    face,
                },
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The lateral profile: the stop distance sampled across the whole panel, from each side.
// ---------------------------------------------------------------------------------------------

/// An approach `deg` degrees off the slab normal, offset `lateral` metres along the tangent, on
/// the `sign` side. The reflection is through the slab **plane**: the normal component flips with
/// the side and the tangential one does not, so the two arms are true mirror images. Negating both
/// changes their incidence angles and invalidates the symmetry comparison.
fn side_offset(s: &Scene, sign: f32, lateral: f32, deg: f32) -> Side {
    let r = deg.to_radians();
    let (cos, sin, tan) = (math::cosf(r), math::sinf(r), math::tanf(r));
    let out = s.normal.mul(sign);
    let face = if sign > 0.0 { s.face_pos } else { s.face_neg };
    let drift = (START_DISTANCE + face) * tan;
    let start_xy = s
        .mesh_centre
        .add(out.mul(START_DISTANCE + face))
        .add(s.tangent.mul(lateral))
        .sub(s.tangent.mul(drift * 0.5));
    let gz = ground_at(&s.land, start_xy.x, start_xy.y).expect("terrain under the start");
    let dir = out.mul(-cos).add(s.tangent.mul(sin));
    Side {
        out,
        start: Vec3::new(start_xy.x, start_xy.y, gz),
        step: dir.mul(0.12),
        face,
    }
}

/// Walk at the panel at `lateral`, from `sign`, and report how close the body's centre got.
fn profile_at(s: &Scene, sign: f32, lateral: f32, deg: f32) -> Approach {
    let side = side_offset(s, sign, lateral, deg);
    let centre = s.mesh_centre.add(s.tangent.mul(lateral));
    walk(&s.land, &s.geom, s.door_at, centre, s.tangent, 0.45, &side)
}

/// The door as the **server** owns one: non-static, which is what `door_opens_ethereal.rs` asserts
/// ("a door is never STATIC_PS").
///
/// The o283 harness's `place_door` makes a *static* body, and a static candidate never reaches
/// 's ethereal exemption: the `is_static()` branch of
/// the recording step claims the result first, so a static door goes on blocking after it opens.
/// That is right for a cell static and wrong for a door, and it is why the sequence station below
/// builds its own body rather than reusing `place_door`.
fn place_door_dynamic(
    w: &mut PhysicsWorld,
    geom: &Arc<SetupGeometry>,
    at: Vec3,
) -> (PhysHandle, Position) {
    let h = w.create(ObjectId(0x7000_000A), Arc::clone(geom), true);
    let mut cell = HOLTBURG.cell(1);
    let mut origin = at;
    landdefs::adjust_to_outside(&mut cell, &mut origin);
    w.enter_cell(h, cell);
    let pos = Position::new(cell, Frame::new(at, Quat::IDENTITY));
    {
        let o = w.get_mut(h).expect("live");
        o.position = pos;
        o.update_time = 0.0;
    }
    assert!(
        !w.get(h).expect("live").state.is_static(),
        "a door is never STATIC_PS"
    );
    assert!(
        w.get(h).expect("live").state.has_physics_bsp(),
        "the door must keep the BSP arm or the walk measures a fallback shape"
    );
    w.calc_cross_cells(h, false);
    (h, pos)
}

fn leg(w: &mut PhysicsWorld, s: &Scene, id: u32, side: &Side, lateral: f32) -> Approach {
    let body = spawn(w, id, side.start, side.step);
    let centre = s.mesh_centre.add(s.tangent.mul(lateral));
    let dt = 1.0 / 30.0;
    let mut t = w.get(body).map_or(0.0, |o| o.update_time);
    let mut closest = f32::MAX;
    let mut facing_frames = 0u32;
    let mut travel = 0.0f32;
    for _ in 0..75 {
        t += dt;
        w.use_time(LocalTime(t), false);
        let p = w.get(body).expect("live").position.frame.origin;
        travel = travel.max(math::hypotf(p.x - side.start.x, p.y - side.start.y));
        let rel = p.add(Vec3::new(0.0, 0.0, BODY_RADIUS)).sub(centre);
        if rel.dot(s.tangent).abs() <= 0.45 {
            facing_frames += 1;
            closest = closest.min(rel.dot(side.out) - side.face);
        }
    }
    w.destroy(body);
    Approach {
        closest,
        facing_frames,
        travel,
    }
}

/// How many lateral samples the sweep takes across the panel.
const PROFILE_SAMPLES: usize = 13;
/// The walk advances 0.12 m per sub-step, so one step of granularity is the most two mirrored
/// arms may legitimately differ by.
const WALK_STEP: f32 = 0.12;

/// The two faces present the same surface across the whole panel.
#[test]
fn the_two_faces_present_the_same_surface_across_the_whole_panel() {
    let s = scene();
    let mut worst = 0.0f32;
    let mut worst_at = (0.0f32, 0.0f32);
    for deg in [20.0f32, 10.0] {
        for i in 0..PROFILE_SAMPLES {
            #[allow(clippy::cast_precision_loss)]
            let frac = i as f32 / (PROFILE_SAMPLES - 1) as f32;
            let lateral = -s.half_width + 2.0 * s.half_width * frac;
            let f = profile_at(&s, 1.0, lateral, deg);
            let b = profile_at(&s, -1.0, lateral, deg);
            assert!(
                f.facing_frames > 5 && b.facing_frames > 5,
                "an arm never faced the panel at {deg:.0} deg, lateral {lateral:+.3}: front {} \
                 frames, back {} frames",
                f.facing_frames,
                b.facing_frames
            );
            // Both arms must have been stopped **by the mesh**: a body that walked through it
            // ends far past the face and its distance is not a measurement of a surface.
            for (name, a) in [("front", &f), ("back", &b)] {
                assert!(
                    a.closest > 0.0,
                    "at {deg:.0} deg, lateral {lateral:+.3}, the {name} arm ended {:.4} m past \
                     the face: it walked through the panel",
                    a.closest
                );
            }
            let diff = (f.closest - b.closest).abs();
            if diff > worst {
                worst = diff;
                worst_at = (deg, lateral);
            }
            assert!(
                diff < WALK_STEP + 0.01,
                "at {deg:.0} degrees, lateral {lateral:+.3}: the door stops a body {:.4} m from \
                 its mesh from the front and {:.4} m from the back, a {diff:.4} m difference \
                 between two mirrored approaches",
                f.closest,
                b.closest
            );
        }
    }
    println!(
        "{} lateral samples on each of two approach angles; worst front/back difference \
         {worst:.4} m at {:.0} deg, lateral {:+.3}, against a {WALK_STEP:.2} m walk step",
        PROFILE_SAMPLES * 2,
        worst_at.0,
        worst_at.1
    );
}

/// Behaviour: physics.doors.a-door-presents-the-same-surface-from-both-faces-after-an-open-and-close-cycle
#[test]
fn an_open_and_close_cycle_leaves_both_contact_surfaces_unchanged() {
    let s = scene();
    let (deg, lateral) = (20.0f32, 0.0f32);
    let front = side_offset(&s, 1.0, lateral, deg);
    let back = side_offset(&s, -1.0, lateral, deg);

    // The controls: a cold world for each face, the door never having moved.
    let cold_front = profile_at(&s, 1.0, lateral, deg);
    let cold_back = profile_at(&s, -1.0, lateral, deg);

    let mut w = PhysicsWorld::new(Arc::clone(&s.land) as Arc<dyn LandSource>);
    let (door, _pos) = place_door_dynamic(&mut w, &s.geom, s.door_at);
    let seq_front = leg(&mut w, &s, 11, &front, lateral);
    // Open it:, the hook the opening motion raises.
    assert_eq!(
        w.set_ethereal(door, true, true),
        dereth_physics::EtherealResult::Applied,
        "the door would not open"
    );
    let through = leg(&mut w, &s, 12, &front, lateral);
    assert_eq!(
        w.set_ethereal(door, false, true),
        dereth_physics::EtherealResult::Applied,
        "the door would not close"
    );
    let seq_back = leg(&mut w, &s, 13, &back, lateral);

    println!(
        "sequence: cold front {:.4} / back {:.4}; after the cycle front {:.4} / back {:.4}; \
         while open the body travelled {:.3} m and passed {:.4} m beyond the face",
        cold_front.closest,
        cold_back.closest,
        seq_front.closest,
        seq_back.closest,
        through.travel,
        -through.closest
    );

    // Calibration: the open door really did stop blocking, or the "closed" legs prove nothing
    // about a cycle that never happened.
    assert!(
        through.closest < 0.0 && through.travel > 2.0,
        "the opened door still blocked: the body travelled {:.3} m and got no closer than \
         {:.4} m to the face",
        through.travel,
        through.closest
    );

    for (name, cold, seq) in [
        ("front", &cold_front, &seq_front),
        ("back", &cold_back, &seq_back),
    ] {
        assert!(
            (cold.closest - seq.closest).abs() < WALK_STEP + 0.01,
            "the {name} face stops a body {:.4} m from the mesh cold and {:.4} m after an \
             open/close cycle",
            cold.closest,
            seq.closest
        );
    }
    assert!(
        (seq_front.closest - seq_back.closest).abs() < WALK_STEP + 0.01,
        "after the cycle the door stops a body {:.4} m from its mesh from the front and {:.4} m \
         from the back",
        seq_front.closest,
        seq_back.closest
    );
}

/// The head on collapse is symmetric and is the narrowest region left.
#[test]
fn the_head_on_collapse_is_symmetric_and_is_the_narrowest_region_left() {
    let s = scene();
    let lateral = 0.0f32;
    let f = profile_at(&s, 1.0, lateral, 0.0);
    let b = profile_at(&s, -1.0, lateral, 0.0);
    println!(
        "head-on: front {:.4} m (travelled {:.3} m), back {:.4} m (travelled {:.3} m)",
        f.closest, f.travel, b.closest, b.travel
    );
    for (name, a) in [("front", &f), ("back", &b)] {
        assert!(
            a.closest > 0.0,
            "the head-on {name} arm ended {:.4} m inside the panel after travelling {:.3} m. A \
             refused transition must not translate the body",
            -a.closest,
            a.travel
        );
        assert!(
            a.travel < 2.0,
            "and it must be held at the face rather than sliding round the panel: it travelled \
             {:.3} m",
            a.travel
        );
    }
    assert!(
        (f.closest - b.closest).abs() < WALK_STEP + 0.01,
        "the head-on collapse is one-sided: front {:.4} m, back {:.4} m",
        f.closest,
        b.closest
    );
    // And one step off the normal, the same door stops the body cleanly on both faces. This is
    // the contrast that makes the collapse worth a pin rather than a footnote.
    for deg in [10.0f32] {
        let of = profile_at(&s, 1.0, lateral, deg);
        let ob = profile_at(&s, -1.0, lateral, deg);
        assert!(
            of.closest > 0.0 && ob.closest > 0.0,
            "at {deg:.0} degrees off the normal the door should still stop the body: front \
             {:.4} m, back {:.4} m",
            of.closest,
            ob.closest
        );
        println!(
            "at {deg:.0} degrees off the normal the same door stops the body at \
             {:.4} m / {:.4} m",
            of.closest, ob.closest
        );
    }
}

/// No heading within a degree of square lets the body through this door.
#[test]
fn no_heading_within_a_degree_of_square_lets_the_body_through_this_door() {
    let s = scene();
    let mut census: Vec<(f32, f32, f32)> = Vec::new();
    for i in 0..=80u16 {
        let deg = f32::from(i) * 0.01;
        let a = profile_at(&s, 1.0, 0.0, deg);
        census.push((deg, a.closest, a.travel));
    }
    for (deg, closest, travel) in &census {
        println!("Door contacts: {deg:.3} deg -- closest {closest:.4} m, travelled {travel:.3} m");
    }
    for (deg, closest, travel) in &census {
        assert!(
            *closest > 0.0,
            "at {deg:.3} deg off square the body ended {:.4} m inside the panel; \
             no angle may pass through it",
            -closest
        );
        assert!(
            *travel < HELD_TRAVEL,
            "at {deg:.3} deg off square the body travelled {travel:.3} m -- it went round the \
             panel rather than being stopped by it"
        );
    }
    // Flat across the window: no discontinuity where the crease starts and stops cancelling, which
    // is the positive statement that the window has no consequence left.
    let lo = census.iter().map(|c| c.1).fold(f32::MAX, f32::min);
    let hi = census.iter().map(|c| c.1).fold(f32::MIN, f32::max);
    println!(
        "Door contacts: stop distance spans {lo:.4}..{hi:.4} m over 0..0.8 deg; analytic crease \
         half-width asin(0.0002/0.12) = {:.4} deg",
        math::asinf(0.0002f32 / 0.12).to_degrees()
    );
    assert!(
        hi - lo < WALK_STEP,
        "the stop distance is not flat across the window: {lo:.4}..{hi:.4} m. Something in the \
         crease branch or in the failed-transition arm has moved"
    );
}

const HELD_TRAVEL: f32 = 2.0;

// ---------------------------------------------------------------------------------------------
// The **tangential slide**: what the body does moving ALONG the panel, once in contact.
// ---------------------------------------------------------------------------------------------

/// One frame of a slide.
#[derive(Debug, Clone, Copy)]
struct SlideStep {
    /// Distance advanced along the slab tangent this frame.
    tangential: f32,
    /// Distance advanced along the slab normal this frame (into/out of the panel).
    normal: f32,
    /// The body's sliding normal after the frame, in world space, and whether it was armed.
    sliding: Vec3,
    is_sliding: bool,
    in_contact: bool,
    /// Perpendicular distance from the body centre to the face, positive outside.
    clearance: f32,
    lateral: f32,
}

fn slide_along(s: &Scene, sign: f32, deg: f32, frames: usize) -> Vec<SlideStep> {
    slide_along_opt(s, sign, deg, frames, true)
}

fn slide_along_opt(
    s: &Scene,
    sign: f32,
    deg: f32,
    frames: usize,
    with_door: bool,
) -> Vec<SlideStep> {
    // Built explicitly rather than through `side_offset`, whose drift correction is tuned for a
    // head-on approach and throws a shallow start clean off the panel. Here the two faces are
    // symmetric by construction: the same distance out along each face's own outward normal, the
    // same lateral start beyond the same edge, and a direction reflected through the slab plane.
    let out = s.normal.mul(sign);
    let face = if sign > 0.0 { s.face_pos } else { s.face_neg };
    let r = deg.to_radians();
    let start_xy = s
        .mesh_centre
        .add(out.mul(0.9 + face))
        .sub(s.tangent.mul(1.2 * s.half_width));
    let gz = ground_at(&s.land, start_xy.x, start_xy.y).expect("terrain under the start");
    let dir = out.mul(-math::cosf(r)).add(s.tangent.mul(math::sinf(r)));
    let side = Side {
        out,
        start: Vec3::new(start_xy.x, start_xy.y, gz),
        step: dir.mul(0.12),
        face,
    };
    let mut w = PhysicsWorld::new(Arc::clone(&s.land) as Arc<dyn LandSource>);
    if with_door {
        let _ = place_door_dynamic(&mut w, &s.geom, s.door_at);
    }
    let body = spawn(&mut w, 1, side.start, side.step);
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut prev = w.get(body).expect("live").position.frame.origin;
    let mut out = Vec::with_capacity(frames);
    for _ in 0..frames {
        t += dt;
        w.use_time(LocalTime(t), false);
        let o = w.get(body).expect("live");
        let p = o.position.frame.origin;
        let d = p.sub(prev);
        prev = p;
        let rel = p.add(Vec3::new(0.0, 0.0, BODY_RADIUS)).sub(s.mesh_centre);
        out.push(SlideStep {
            tangential: d.dot(s.tangent),
            normal: d.dot(side.out),
            sliding: o.sliding_normal,
            is_sliding: o.transient_state.is_sliding(),
            in_contact: o.transient_state.in_contact(),
            clearance: rel.dot(side.out) - side.face,
            lateral: rel.dot(s.tangent),
        });
    }
    out
}

/// The commanded tangential advance per frame: the walk step's own component along the tangent.
fn commanded_tangential(deg: f32) -> f32 {
    0.12 * math::sinf(deg.to_radians()).abs()
}

#[derive(Debug, Clone, Copy)]
struct SlideStats {
    /// Frames the body spent alongside the panel with the free run actually advancing.
    contact_frames: usize,
    /// Mean and coefficient of variation of `with_door / without_door` tangential advance.
    mean_ratio: f32,
    cv_ratio: f32,
    jammed: usize,
    /// Times the sliding normal turned by more than a degree between consecutive contact frames.
    normal_flips: usize,
    worst_flip_deg: f32,
}

fn slide_stats(with_door: &[SlideStep], free: &[SlideStep]) -> SlideStats {
    let mut ratios = Vec::new();
    let mut touching: Vec<&SlideStep> = Vec::new();
    for (a, b) in with_door.iter().zip(free.iter()) {
        // Only frames where the free run really moved, and where the body is alongside the panel.
        if b.tangential.abs() < 1e-4 {
            continue;
        }
        if a.clearance <= 0.0 || a.clearance >= BODY_RADIUS + 0.12 {
            continue;
        }
        ratios.push(a.tangential / b.tangential);
        touching.push(a);
    }
    let n = ratios.len();
    if n == 0 {
        return SlideStats {
            contact_frames: 0,
            mean_ratio: 0.0,
            cv_ratio: 0.0,
            jammed: 0,
            normal_flips: 0,
            worst_flip_deg: 0.0,
        };
    }
    #[allow(clippy::cast_precision_loss)]
    let nf = n as f32;
    let mean = ratios.iter().sum::<f32>() / nf;
    let var = ratios.iter().map(|r| (r - mean) * (r - mean)).sum::<f32>() / nf;
    let jammed = ratios.iter().filter(|r| **r < 0.05).count();
    let mut flips = 0usize;
    let mut worst = 0.0f32;
    for pair in touching.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if !a.is_sliding || !b.is_sliding {
            continue;
        }
        let (mut u, mut v) = (a.sliding, b.sliding);
        u.z = 0.0;
        v.z = 0.0;
        if u.normalize_check_small() || v.normalize_check_small() {
            continue;
        }
        let d = math::acosf(u.dot(v).clamp(-1.0, 1.0)).to_degrees();
        if d > 1.0 {
            flips += 1;
        }
        worst = worst.max(d);
    }
    SlideStats {
        contact_frames: n,
        mean_ratio: mean,
        cv_ratio: if mean.abs() > 1e-6 {
            var.sqrt() / mean
        } else {
            0.0
        },
        jammed,
        normal_flips: flips,
        worst_flip_deg: worst,
    }
}

/// Both runs of one configuration: the door present, and the identical walk with it removed.
fn slide_pair(s: &Scene, sign: f32, deg: f32) -> SlideStats {
    let with_door = slide_along_opt(s, sign, deg, 140, true);
    let free = slide_along_opt(s, sign, deg, 140, false);
    slide_stats(&with_door, &free)
}

/// A shallow slide jams in a band of angles and the band differs by face.
#[test]
fn a_shallow_slide_jams_in_a_band_of_angles_and_the_band_differs_by_face() {
    let s = scene();

    // Outside the band the slide is perfect on both faces. This is the control that makes the
    // jam a finding rather than a property of the instrument.
    for deg in [45.0f32, 50.0, 60.0, 70.0, 80.0] {
        for (name, sign) in [("front", 1.0f32), ("back", -1.0f32)] {
            let st = slide_pair(&s, sign, deg);
            assert!(
                st.contact_frames > 10,
                "{name} at {deg}: only {} frames alongside the panel, so nothing was measured",
                st.contact_frames
            );
            assert!(
                (st.mean_ratio - 1.0).abs() < 0.02 && st.jammed == 0,
                "{name} at {deg} degrees should slide freely: ratio {:.4}, {} jammed frames of {}",
                st.mean_ratio,
                st.jammed,
                st.contact_frames
            );
        }
    }

    // Inside the band it jams, and the two faces disagree about which angles.
    let jams = |sign: f32, deg: f32| -> bool {
        let st = slide_pair(&s, sign, deg);
        st.contact_frames > 10 && st.mean_ratio < 0.2
    };
    let mut disagreements = Vec::new();
    let mut jammed_angles = 0usize;
    let mut i = 330u16;
    while i <= 400 {
        let deg = f32::from(i) / 10.0;
        let (f, b) = (jams(1.0, deg), jams(-1.0, deg));
        if f || b {
            jammed_angles += 1;
        }
        if f != b {
            disagreements.push((deg, f, b));
        }
        i += 5;
    }
    println!(
        "slide: {jammed_angles} of 15 half-degree steps in 33.0..=40.0 jam on at least one \
         face; the faces disagree at {:?}",
        disagreements
    );
    assert!(
        jammed_angles >= 5,
        "the jam band has moved or closed: only {jammed_angles} of 15 sampled angles jam. If this \
         is a repair, replace this pin with the acceptance (ratio ~1.0, zero jammed, every angle, \
         both faces)"
    );
    assert!(
        !disagreements.is_empty(),
        "the jam band is now identical on both faces. It used to disagree at 36.0, 37.0 (front \
         slides, back jams) and 38.5, 39.0 (back slides, front jams) -- that face-dependence is \
         shows this is not a symmetric quirk"
    );
}

///  on every part, at a given **pose**.
fn inside_door_posed(
    g: &SetupGeometry,
    pos: &Position,
    pose: Option<&[Frame]>,
    p: Vec3,
    radius: f32,
) -> bool {
    (0..g.parts.len()).any(|i| {
        let Some(part) = g.placed_part_posed(i, pos, 1.0, pose) else {
            return false;
        };
        let Some(tree) = part.physics_bsp.as_ref() else {
            return false;
        };
        let inv = 1.0 / part.gfxobj_scale;
        let m = dereth_physics::math::l2g(part.pos.frame.rotation);
        let local = dereth_physics::math::globaltolocalvec(m, p.sub(part.pos.frame.origin));
        tree.sphere_intersects_solid(&Sphere::new(local.mul(inv), radius * inv), true)
    })
}

struct Doorway {
    land: Arc<StaticLandSource>,
    geom: Arc<SetupGeometry>,
    at: Vec3,
    pose: Option<Arc<Vec<Frame>>>,
    normal: Vec3,
    tangent: Vec3,
    reach_pos: f32,
    reach_neg: f32,
}

fn doorway(pose: Option<Arc<Vec<Frame>>>) -> Doorway {
    let store = store();
    let land = Arc::new(holtburg(&store));
    let geom = Arc::new(door_geometry(&store));
    let at = {
        let mut best = None;
        'outer: for i in 0..40_i16 {
            for j in 0..40_i16 {
                let x = 20.0 + f32::from(i) * 3.0;
                let y = 20.0 + f32::from(j) * 3.0;
                let Some(z) = ground_at(&land, x, y) else {
                    continue;
                };
                let flat = (-16..=16_i8).all(|k| {
                    let d = f32::from(k) * 0.25;
                    ground_at(&land, x + d, y).is_some_and(|h| (h - z).abs() < 0.05)
                        && ground_at(&land, x, y + d).is_some_and(|h| (h - z).abs() < 0.05)
                });
                if flat {
                    best = Some(Vec3::new(x, y, z));
                    break 'outer;
                }
            }
        }
        best.expect("a flat clear patch of Holtburg's terrain")
    };
    let pos = Position::new(HOLTBURG.cell(1), Frame::new(at, Quat::IDENTITY));
    let (normal, tangent) = (Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.0, 0.0));
    let base = Vec3::new(at.x, at.y, at.z + BODY_RADIUS);
    let p = pose.as_deref().map(Vec::as_slice);
    // How far the mesh reaches out of the doorway plane, swept **across** the whole doorway: the
    // two leaves do not meet at the object's origin (part 0 spans x in [0.02, 1.66] and part 1 the
    // mirror of it), so a probe fired from x = 0 alone finds nothing and reports an empty doorway.
    let reach = |dir: Vec3| -> f32 {
        let mut best = 0.0f32;
        for k in -80..=80_i16 {
            let side = base.add(tangent.mul(f32::from(k) * 0.025));
            let mut u = 0.0f32;
            while u < 4.0 {
                if inside_door_posed(&geom, &pos, p, side.add(dir.mul(u)), 0.0) {
                    best = best.max(u);
                }
                u += 0.01;
            }
        }
        best
    };
    let (reach_pos, reach_neg) = (reach(normal), reach(normal.mul(-1.0)));
    Doorway {
        land,
        geom,
        at,
        pose,
        normal,
        tangent,
        reach_pos,
        reach_neg,
    }
}

const ENTRY_LATERAL: f32 = 1.0;
/// The scoring window either side of the doorway's centre â€” the span both leaves cover, well
/// clear of their outer ends.
const GLIDE_WINDOW: f32 = 0.9;

fn glide(d: &Doorway, sign: f32, deg: f32, frames: usize, with_door: bool) -> Vec<(f32, f32)> {
    let out = d.normal.mul(sign);
    let reach = if sign > 0.0 { d.reach_pos } else { d.reach_neg };
    let r = deg.to_radians();
    let start_xy = Vec3::new(d.at.x, d.at.y, 0.0)
        .add(out.mul(0.9 + reach))
        .sub(d.tangent.mul(ENTRY_LATERAL));
    let gz = ground_at(&d.land, start_xy.x, start_xy.y).expect("terrain under the start");
    let start = Vec3::new(start_xy.x, start_xy.y, gz);
    let step = out
        .mul(-math::cosf(r))
        .add(d.tangent.mul(math::sinf(r)))
        .mul(0.12);
    let mut w = PhysicsWorld::new(Arc::clone(&d.land) as Arc<dyn LandSource>);
    if with_door {
        let (h, _) = place_door_dynamic(&mut w, &d.geom, d.at);
        if let Some(pose) = d.pose.clone() {
            // The open/close cycle. The part update runs once per frame with whatever
            // `get_curr_animframe` answers; here the two ends of the door's own animation are
            // written straight in, which is what its cycles hold (framerate 0, a held frame).
            let (_closed, open) = door_poses(&store());
            if let Some(o) = w.get_mut(h) {
                o.part_frames = Some(Arc::new(open));
            }
            w.calc_cross_cells(h, false);
            w.use_time(LocalTime(1.0 / 30.0), false);
            if let Some(o) = w.get_mut(h) {
                o.part_frames = Some(pose);
            }
            w.calc_cross_cells(h, false);
        }
    }
    let body = spawn(&mut w, 1, start, step);
    let dt = 1.0 / 30.0;
    let mut t = w.get(body).map_or(0.0, |o| o.update_time);
    let mut prev = w.get(body).expect("live").position.frame.origin;
    let mut series = Vec::with_capacity(frames);
    for _ in 0..frames {
        t += dt;
        w.use_time(LocalTime(t), false);
        let p = w.get(body).expect("live").position.frame.origin;
        let adv = p.sub(prev).dot(d.tangent);
        prev = p;
        series.push((adv, p.sub(d.at).dot(d.tangent)));
    }
    series
}

/// Tangential progress across the doorway, door present over door absent, and jammed frames.
///
/// The window is the **free** run's: the identical walk with the door taken out of the world, so
/// both runs are scored over exactly the frames in which an unobstructed body crosses the doorway.
/// Gating on the door-present body's own lateral instead scores a body that jams before it arrives
/// over *no frames at all*, and reports the worst jam there is as a clean zero.
fn glide_stats(d: &Doorway, sign: f32, deg: f32) -> (f32, usize, usize) {
    let with = glide(d, sign, deg, 200, true);
    let free = glide(d, sign, deg, 200, false);
    let mut ratios = Vec::new();
    let mut jammed = 0usize;
    for (a, b) in with.iter().zip(free.iter()) {
        if b.0.abs() < 1e-4 || b.1.abs() > GLIDE_WINDOW {
            continue;
        }
        let ratio = a.0 / b.0;
        if ratio < 0.05 {
            jammed += 1;
        }
        ratios.push(ratio);
    }
    let n = ratios.len();
    #[allow(clippy::cast_precision_loss)]
    let mean = if n == 0 {
        0.0
    } else {
        ratios.iter().sum::<f32>() / n as f32
    };
    (mean, jammed, n)
}

/// The z rotation of a frame, in degrees, wrapped to (-180, 180].
fn yaw_deg(f: &Frame) -> f32 {
    let a = 2.0 * math::atan2f(f.rotation.z, f.rotation.w).to_degrees();
    let a = a.rem_euclid(360.0);
    if a > 180.0 {
        a - 360.0
    } else {
        a
    }
}

/// The setups placement frame is a third open and the closed pose comes from the animation.
#[test]
fn the_setups_placement_frame_is_a_third_open_and_the_closed_pose_comes_from_the_animation() {
    let store = store();
    let bytes = store
        .read_typed(DbType::Setup, DOOR_SETUP)
        .expect("the door setup");
    let s = Setup::decode_payload(DOOR_SETUP, &bytes).expect("the setup decodes");
    assert!(
        !s.placement_frames.contains_key(&PLACEMENT_FRAME_DEFAULT),
        "this door carries no 0x65, so SetPlacementFrame(0x65) falls back to key 0: {:?}",
        s.placement_frames.keys().collect::<Vec<_>>()
    );
    let placement = s.placement_frames.get(&0).expect("key 0").frames.clone();
    let (closed, open) = door_poses(&store);
    assert_eq!(
        closed.len(),
        placement.len(),
        "the animation covers every part"
    );

    for i in 0..2 {
        let (p, c, o) = (
            yaw_deg(&placement[i]),
            yaw_deg(&closed[i]),
            yaw_deg(&open[i]),
        );
        // How far through the swing the placement frame sits: 0 = closed, 1 = open.
        let f = (p - c) / (o - c);
        let moved = placement[i].origin.sub(closed[i].origin).mag2().sqrt();
        println!(
            "F24 leaf {i}: placement {p:.2} deg, closed {c:.2} deg, open {o:.2} deg -- the \
             placement frame is {:.0}% of the way open, and its origin is {moved:.3} m off the \
             closed pose",
            f * 100.0
        );
        assert!(
            (0.2..0.5).contains(&f),
            "leaf {i}'s placement frame should sit a third of the way through the swing, not at \
             {:.0}%",
            f * 100.0
        );
        assert!(
            moved > 0.3,
            "leaf {i}'s placement frame should sit well out of the closed pose"
        );
    }

    // And the two poses really do present different surfaces to the doorway.
    let staged = doorway(None);
    let live = doorway(Some(Arc::new(closed)));
    println!(
        "F24 doorway reach: placement frame +{:.3} / -{:.3} m; animated closed +{:.3} / -{:.3} m",
        staged.reach_pos, staged.reach_neg, live.reach_pos, live.reach_neg
    );
    assert!(
        (staged.reach_pos - staged.reach_neg).abs() > 0.5,
        "the placement pose is a chevron: it should reach much further out of the doorway on one \
         side than the other ({:.3} vs {:.3})",
        staged.reach_pos,
        staged.reach_neg
    );
    assert!(
        (live.reach_pos - live.reach_neg).abs() < 0.05,
        "the animated closed pose is a flat panel: {:.3} vs {:.3}",
        live.reach_pos,
        live.reach_neg
    );
}

/// The owners forty five degree glide agrees on both faces after an open and close cycle.
#[test]
fn the_owners_forty_five_degree_glide_agrees_on_both_faces_after_an_open_and_close_cycle() {
    let (closed, _open) = door_poses(&store());
    let staged = doorway(None);
    let live = doorway(Some(Arc::new(closed)));

    let (sf, sfj, sfn) = glide_stats(&staged, 1.0, 45.0);
    let (sb, sbj, sbn) = glide_stats(&staged, -1.0, 45.0);
    let (lf, lfj, lfn) = glide_stats(&live, 1.0, 45.0);
    let (lb, lbj, lbn) = glide_stats(&live, -1.0, 45.0);
    println!(
        "F24 45-degree glide -- placement frame: front {sf:.4} ({sfj} jammed of {sfn}), back \
         {sb:.4} ({sbj} of {sbn}); animated closed: front {lf:.4} ({lfj} of {lfn}), back {lb:.4} \
         ({lbj} of {lbn})"
    );
    assert!(
        lfn > 10 && lbn > 10,
        "nothing was measured: {lfn} / {lbn} scored frames"
    );

    // The acceptance: the two faces agree, and both make real progress.
    assert!(
        (lf - lb).abs() < 0.05,
        "the two faces must present the same surface at 45 degrees: front {lf:.4}, back {lb:.4}"
    );
    assert!(
        lf > 0.75 && lb > 0.75,
        "both faces must glide rather than hold station: front {lf:.4}, back {lb:.4}"
    );

    // And the pose is what did it, rather than the instrument: at the setup's placement frame the
    // same walk disagrees by an order of magnitude more.
    assert!(
        (sf - sb).abs() > 4.0 * (lf - lb).abs(),
        "the placement pose should be the asymmetric one: {sf:.4} vs {sb:.4} against {lf:.4} vs \
         {lb:.4}. If this went quiet the seam stopped being exercised"
    );
}

/// Behaviour: physics.doors.a-door-presents-the-same-surface-from-both-faces-after-an-open-and-close-cycle
/// A head on walk at this door is stopped as an oblique one is.
#[test]
fn a_head_on_walk_at_this_door_is_stopped_as_an_oblique_one_is() {
    let sc = scene();
    let Scene {
        ref land,
        ref geom,
        door_at,
        mesh_centre,
        tangent,
        half_width,
        ..
    } = sc;
    let door_cell_pos = Position::new(HOLTBURG.cell(1), Frame::new(door_at, Quat::IDENTITY));

    let run = |deg: f32| -> Vec<(&'static str, Approach)> {
        sides_at(&sc, deg)
            .iter()
            .map(|(name, s)| {
                assert!(
                    !inside_door(
                        geom,
                        &door_cell_pos,
                        s.start.add(Vec3::new(0.0, 0.0, BODY_RADIUS)),
                        BODY_RADIUS
                    ),
                    "the {name} start at {deg} degrees is already inside the door"
                );
                (
                    *name,
                    walk(land, geom, door_at, mesh_centre, tangent, half_width, s),
                )
            })
            .collect()
    };

    // Calibration on the *aim*, before either walk: the head-on arm must actually be head on and
    // the oblique arm must actually be oblique, or the differential below is a measurement of
    // nothing. Validate the fixture geometry before comparing the outcomes.
    for (deg, want_cos) in [(HEAD_ON_DEG, 1.0f32), (OBLIQUE_DEG, 0.939_692_6)] {
        for (name, s) in sides_at(&sc, deg) {
            let mut d = s.step;
            assert!(
                !d.normalize_check_small(),
                "the {name} step at {deg} degrees is zero"
            );
            let cos = -d.dot(s.out);
            assert!(
                (cos - want_cos).abs() < 0.01,
                "the {name} arm at {deg} degrees meets the slab at cos {cos:.4}, not {want_cos:.4}"
            );
        }
    }

    let oblique = run(OBLIQUE_DEG);
    let head_on = run(HEAD_ON_DEG);

    for (label, rs) in [("oblique 20 deg", &oblique), ("head on", &head_on)] {
        for (name, r) in rs {
            println!(
                "{label} {name} -- closest approach {:.3} m, {} frames in front of the panel, travelled {:.3} m",
                r.closest, r.facing_frames, r.travel
            );
        }
    }

    // Calibrate both directions: every arm must have faced
    // the panel, or its `closest` is not a measurement of anything, and the two must have started
    // the same distance out.
    for (label, rs) in [("oblique 20 deg", &oblique), ("head on", &head_on)] {
        for (name, r) in rs {
            assert!(
                r.facing_frames > 10,
                "the {label} {name} arm faced the panel for only {} frames",
                r.facing_frames
            );
        }
    }

    // **The oblique approach is stopped by the mesh**: it rests about one collision radius off it
    // and never gets inside. This is the control, and it is what makes the head-on reading below a
    // statement about the angle.
    for (name, r) in &oblique {
        assert!(
            r.closest > BODY_RADIUS - 0.06,
            "the oblique {name} approach ended {:.3} m from the mesh, inside a {BODY_RADIUS:.2} m \
             collision radius -- the control is not controlling",
            r.closest
        );
    }

    for (name, r) in &head_on {
        assert!(
            r.closest > 0.0,
            "the head-on {name} approach ended {:.3} m inside the mesh. A refused transition must \
             not translate the body",
            -r.closest
        );
    }

    let ob = oblique
        .iter()
        .map(|(_, r)| r.closest)
        .fold(f32::MAX, f32::min);
    let ho = head_on
        .iter()
        .map(|(_, r)| r.closest)
        .fold(f32::MAX, f32::min);
    println!(
        "oblique closest {ob:.3} m, head-on closest {ho:.3} m, difference {:.3} m (the walk \
         advances 0.12 m per frame, so one step of granularity is expected)",
        (ob - ho).abs()
    );
    assert!(
        (ob - ho).abs() < 0.12 + 0.01,
        "the oblique approach is stopped {ob:.3} m from the mesh and the head-on one {ho:.3} m, a \
         difference of {:.3} m -- more than one 0.12 m walk step, so this door is not presenting \
         the same surface at the two angles",
        (ob - ho).abs()
    );
}
