//! The landscape's level-of-detail seams close, and scenery stops at the full-detail ring.
//!
//! Block generation adjusts transitions after building vertices and before making polygons, for
//! a block that is not the viewer's and has between one and eight cells a side. With
//! `m = max(|dx|, |dy|)` a block has 8 cells a side for `m <= 1`, 4 for `m == 2`, 2 for `m` 3 or 4
//! and 1 for `m >= 5`, and only rings 2 and 4 get a transition direction. The seams:
//!
//! | seam | detail | covered by |
//! |---|---|---|
//! | ring 1 \| ring 2 | 8 \| 4 | the inward clamp: cardinals only (a ring-2 diagonal's inward neighbours are also ring 2) |
//! | ring 2 \| ring 3 | 4 \| 2 | the outward average on the ring-2 block's outward edge: the straight run through grid 0, 4, 8, **exactly** the 2-cell neighbour's polyline |
//! | ring 3 \| ring 4 | 2 \| 2 | no seam: same detail |
//! | ring 4 \| ring 5+ | 2 \| 1 | the outward average again: the straight run through grid 0, 8, **exactly** the 1-cell neighbour's span |
//!
//! The inward clamp keeps the coarse edge at or below the fine edge rather than making them equal;
//! the tests check those two properties on sampled cardinal seams within 1 mm. Two further claims:
//! a request to place an object far below ground is discarded (so a "buried" object painting pixels
//! is not a terrain leak), and the shipped `scenery_radius: 1` selects exactly the full-detail 3x3
//! core, the same blocks the building, static and dynamic scenery gates accept.
//!
//! Fixture: the retail dats around Holtburg (0xA9B4); the burial arm drives a scene on a software
//! device, with an `ObjectStream` fed by hand. No datagram leaves the process; missing dats fail.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_assets::world::CellLandblock;
use dereth_assets::Decode;
use dereth_client::world::SceneReads;
use dereth_client::world::{block_xy, landblock_did, load_region};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::num::math;
use dereth_world_render::land::mesh::{generate_landblock_with_table, Direction};
use dereth_world_render::land::order::{block_orient, side_cell_count};
use dereth_world_render::LandblockMesh;

/// Holtburg, the landblock every station here stands on.
const HOLTBURG: u16 = 0xA9B4;
/// The terrain grid's cell side length, in metres.
const CELL: f32 = 24.0;

fn store() -> RetailDatStore {
    crate::common::dat_store()
}

/// One landblock's `CellLandblock`, or `None` outside the 0..255 block grid.
fn landblock(store: &RetailDatStore, bx: i32, by: i32) -> Option<CellLandblock> {
    if !(0..255).contains(&bx) || !(0..255).contains(&by) {
        return None;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let id = ((bx as u16) << 8) | (by as u16);
    let did = landblock_did(id);
    let bytes = store.read_typed(DbType::LandBlock, did).ok()?;
    CellLandblock::decode_payload(did, &bytes).ok()
}

/// Which of a block's four edges a seam is on, and how to walk it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    /// `i == 0` — the block's west edge; the varying index is `j` (y).
    West,
    /// `i == side_cell_count` — the east edge; the varying index is `j`.
    East,
    /// `j == 0` — the south edge; the varying index is `i` (x).
    South,
    /// `j == side_cell_count` — the north edge; the varying index is `i`.
    North,
}

/// The `side_cell_count + 1` z values along one edge of a mesh, in ascending order of the varying
/// axis.
fn edge_z(m: &LandblockMesh, e: Edge) -> Vec<f32> {
    let n = usize::from(m.side_cell_count);
    let svc = n + 1;
    let at = |i: usize, j: usize| m.vertices[i * svc + j].z;
    (0..svc)
        .map(|k| match e {
            Edge::West => at(0, k),
            Edge::East => at(n, k),
            Edge::South => at(k, 0),
            Edge::North => at(k, n),
        })
        .collect()
}

/// An edge polyline of `n + 1` knots, sampled at the **8-cell** grid's nine positions. `n` is 8, 4,
/// 2 or 1, so every fine knot is either a coarse knot or an exact midpoint chain.
fn sample_edge(z: &[f32]) -> [f32; 9] {
    let n = z.len() - 1;
    let step = 8 / n; // grid columns per coarse span
    let mut out = [0.0f32; 9];
    for (k, o) in out.iter_mut().enumerate() {
        let s = k / step; // which coarse span
        #[allow(clippy::cast_precision_loss)] // step <= 8
        let t = (k % step) as f32 / step as f32;
        *o = if t == 0.0 {
            z[s]
        } else {
            z[s] * (1.0 - t) + z[s + 1] * t
        };
    }
    out
}

/// Generate one window slot the way the drawn scene does: `block_orient` picks the detail and the
/// stitch direction, and `generate_landblock_with_table` is the function `WorldScene`'s
/// `LandContext::generate` calls.
fn slot(
    store: &RetailDatStore,
    table: &[f32; dereth_world_render::consts::LAND_HEIGHT_TABLE_LEN],
    region: &dereth_assets::region::Region,
    viewer: (i32, i32),
    d: (i32, i32),
) -> Option<(LandblockMesh, u8, Direction)> {
    let (lod_div, dir) = block_orient(d.0, d.1);
    let (bx, by) = (viewer.0 + d.0, viewer.1 + d.1);
    let lb = landblock(store, bx, by)?;
    let m = generate_landblock_with_table(&lb, region, table, bx, by, lod_div, dir);
    Some((m, side_cell_count(lod_div), dir))
}

/// The ring-2 cardinal offsets, each with the inward step that reaches its full-detail neighbour,
/// the coarse block's edge that faces it and the fine block's matching edge.
const RING2_CARDINALS: [((i32, i32), (i32, i32), Edge, Edge); 12] = [
    ((2, -1), (1, -1), Edge::West, Edge::East),
    ((2, 0), (1, 0), Edge::West, Edge::East),
    ((2, 1), (1, 1), Edge::West, Edge::East),
    ((-2, -1), (-1, -1), Edge::East, Edge::West),
    ((-2, 0), (-1, 0), Edge::East, Edge::West),
    ((-2, 1), (-1, 1), Edge::East, Edge::West),
    ((-1, 2), (-1, 1), Edge::South, Edge::North),
    ((0, 2), (0, 1), Edge::South, Edge::North),
    ((1, 2), (1, 1), Edge::South, Edge::North),
    ((-1, -2), (-1, -1), Edge::North, Edge::South),
    ((0, -2), (0, -1), Edge::North, Edge::South),
    ((1, -2), (1, -1), Edge::North, Edge::South),
];

/// **The premise, and it is load-bearing for both tests below.** Adjacent landblocks *share* their
/// boundary row of the 9x9 height grid: row `i == 8` of block `(bx, by)` is row `i == 0` of block
/// `(bx+1, by)`. Without that, two neighbours' edges are not the same world line and comparing them
/// measures nothing.
#[test]
fn adjacent_landblocks_share_their_boundary_height_row() {
    let store = store();
    let (bx, by) = block_xy(HOLTBURG);
    let mut checked = 0usize;
    for d in -3..=3 {
        let (Some(a), Some(b)) = (
            landblock(&store, bx + d, by),
            landblock(&store, bx + d + 1, by),
        ) else {
            continue;
        };
        for j in 0..9usize {
            assert_eq!(
                a.height[8 * 9 + j],
                b.height[j],
                "east/west boundary row disagrees at d={d} j={j}"
            );
        }
        let (Some(a), Some(b)) = (
            landblock(&store, bx, by + d),
            landblock(&store, bx, by + d + 1),
        ) else {
            continue;
        };
        for i in 0..9usize {
            assert_eq!(
                a.height[i * 9 + 8],
                b.height[i * 9],
                "north/south boundary row disagrees at d={d} i={i}"
            );
        }
        checked += 1;
    }
    eprintln!("premise: {checked} landblock pairs share their boundary height row exactly");
    assert!(
        checked >= 6,
        "only {checked} pairs were reachable, so this proves little"
    );
}

/// **The inward clamp.**
///
/// At a ring-1 | ring-2 cardinal seam the coarse (4-cell) block's boundary edge must lie **at or
/// below** the fine (8-cell) block's, everywhere along it. That is exactly what
/// `inward_crack_guard` buys, and the arithmetic is not approximate: clamping the coarse knot at
/// grid `2n` to `min(z, 2H(2n-1) - H(2n-2), 2H(2n+1) - H(2n+2))` is the statement that the coarse
/// chord passes at or below the fine terrain at grid `2n-1` and at grid `2n+1`. Both polylines are
/// linear between the nine grid columns and the coarse one is below at all nine, so it is below
/// everywhere.
///
/// Where the coarse edge stands **above** the fine one, the viewer — who is on the fine side —
/// can see the coarse rim cut across the nearer ground. The transition adjustment keeps that
/// edge at or below the fine edge, where the foreground fine block covers the gap from this side.
#[test]
fn the_coarse_side_of_a_ring_two_seam_never_stands_above_the_fine_side() {
    let store = store();
    let region = load_region(&store).expect("the region decodes");
    let table = dereth_world_render::land::mesh::height_table(&region);
    let viewer = block_xy(HOLTBURG);

    let mut seams = 0usize;
    let mut above = 0usize;
    let mut worst = 0.0f32;
    for (d, inward, coarse_edge, fine_edge) in RING2_CARDINALS {
        let Some((coarse, cn, dir)) = slot(&store, &table, &region, viewer, d) else {
            continue;
        };
        let Some((fine, fnn, _)) = slot(&store, &table, &region, viewer, inward) else {
            continue;
        };
        assert_eq!(cn, 4, "offset {d:?} is not a 4-cell ring-2 block");
        assert_eq!(fnn, 8, "offset {inward:?} is not a full-detail block");
        assert!(
            dir.is_cardinal(),
            "offset {d:?} is not a cardinal ring-2 block ({dir:?})"
        );
        seams += 1;
        let cs = sample_edge(&edge_z(&coarse, coarse_edge));
        let fs = sample_edge(&edge_z(&fine, fine_edge));
        for k in 0..9 {
            // 1 mm, far under the 2 m granularity of the land height table.
            if cs[k] > fs[k] + 1e-3 {
                above += 1;
                worst = worst.max(cs[k] - fs[k]);
            }
        }
    }
    eprintln!(
        "ring-1|ring-2 seam: {seams} cardinal seams around {HOLTBURG:04X}, {above} of {} \
         samples have the coarse edge standing above the fine one, worst {worst:.2} m",
        seams * 9
    );
    assert!(
        seams >= 8,
        "only {seams} ring-2 cardinal seams were reachable; the reading is thin"
    );
    assert_eq!(
        above, 0,
        "the 4-cell ring-2 edge rises {worst:.2} m above the 8-cell ground it abuts at {above} of \
         {} samples, so the seam is open -- the transition adjustment's inward clamp is not reaching the drawn \
         mesh",
        seams * 9
    );
}

/// Behaviour: rendering.terrain.lod-seams-share-the-coarser-neighbours-edge
/// **The outward average, which closes its seams *exactly*.**
///
/// A ring-2 block's outward edge, after the outward average, runs straight through grid 0, 4 and 8; a ring-3
/// block's edge has knots at exactly grid 0, 4 and 8. They are the same polyline. The same argument
/// one ring out: a ring-4 block's outward edge becomes the single span grid 0 to 8, and a ring-5
/// block *is* one span. Thus the mathematical polylines coincide rather than merely hiding one
/// behind the other. The test allows 1 mm at the nine sample positions; the inward test above
/// instead checks that the coarse side is no more than 1 mm above the fine side.
#[test]
fn the_outward_edge_of_a_stitched_block_is_exactly_its_coarser_neighbours_polyline() {
    let store = store();
    let region = load_region(&store).expect("the region decodes");
    let table = dereth_world_render::land::mesh::height_table(&region);
    let viewer = block_xy(HOLTBURG);

    // (ring-2 -> ring-3) and (ring-4 -> ring-5) on each of the four cardinals.
    let cases: [((i32, i32), (i32, i32), Edge, Edge); 8] = [
        ((2, 0), (3, 0), Edge::East, Edge::West),
        ((-2, 0), (-3, 0), Edge::West, Edge::East),
        ((0, 2), (0, 3), Edge::North, Edge::South),
        ((0, -2), (0, -3), Edge::South, Edge::North),
        ((4, 0), (5, 0), Edge::East, Edge::West),
        ((-4, 0), (-5, 0), Edge::West, Edge::East),
        ((0, 4), (0, 5), Edge::North, Edge::South),
        ((0, -4), (0, -5), Edge::South, Edge::North),
    ];
    let mut seams = 0usize;
    let mut open = 0usize;
    let mut worst = 0.0f32;
    for (d, outward, inner_edge, outer_edge) in cases {
        let Some((inner, icn, dir)) = slot(&store, &table, &region, viewer, d) else {
            continue;
        };
        let Some((outer, ocn, _)) = slot(&store, &table, &region, viewer, outward) else {
            continue;
        };
        assert!(
            dir.is_cardinal(),
            "offset {d:?} should be a cardinal stitched block ({dir:?})"
        );
        assert!(
            ocn < icn,
            "offset {outward:?} ({ocn}) is not coarser than {d:?} ({icn})"
        );
        seams += 1;
        let a = sample_edge(&edge_z(&inner, inner_edge));
        let b = sample_edge(&edge_z(&outer, outer_edge));
        for k in 0..9 {
            if (a[k] - b[k]).abs() > 1e-3 {
                open += 1;
                worst = worst.max((a[k] - b[k]).abs());
            }
        }
    }
    eprintln!(
        "outward seam: {seams} seams around {HOLTBURG:04X} ({CELL:.0} m grid), {open} of {} \
         samples differ across the boundary, worst {worst:.2} m",
        seams * 9
    );
    assert!(
        seams >= 6,
        "only {seams} outward seams were reachable; the reading is thin"
    );
    assert_eq!(
        open, 0,
        "the stitched block's outward edge and its coarser neighbour's edge disagree by up to \
         {worst:.2} m at {open} of {} samples, so the ring boundary is a hole -- the transition adjustment's \
         outward average is not reaching the drawn mesh",
        seams * 9
    );
}

// ---------------------------------------------------------------------------------------------
// The driven bench, for the one arm that needs a device
// ---------------------------------------------------------------------------------------------

/// A bench reduced to what the burial measurement needs: one scene, one object, and the drawn
/// part order. It takes **no** differential, because `cypt` is read off the submission trace rather
/// than off pixels, so two benches in lockstep are not needed here.
///
/// **No datagram leaves this process.** `ObjectStream::apply_event` is handed an encoded body
/// directly; nothing opens a socket.
mod bench {
    use dereth_client::character::{CharacterInput, PLAYER_OBJECT_ID};
    use dereth_client::objects::ObjectStream;
    use dereth_client::world::{SceneConfig, WorldScene};
    use dereth_client::world::{SceneReads, SceneWrites};
    use dereth_client_model::weenie::{bitfield, item_type};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_dat::RetailDatStore;
    use dereth_primitives::{LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
    use dereth_render::device::Gpu;
    use std::sync::Arc;

    pub const W: u32 = 800;
    pub const H: u32 = 600;
    /// Holtburg.
    const HOLTBURG: u16 = 0xA9B4;
    /// A recorded `/loc`, block-local, transcribed digit for digit.
    #[allow(clippy::excessive_precision)]
    pub const STATION: (f32, f32) = (130.097_656, 24.053_997);
    pub const TARGET: ObjectId = ObjectId(0x8300_0021);
    /// A long-reach setup: 18 parts and no `GfxObjDegradeInfo` terminator, so it never goes dark
    /// and a zero at range is occlusion rather than the degrade cutoff.
    const NEVER_DARK: u32 = 0x0200_004D;
    /// `ETHEREAL_PS`, so the placement is not turned into a transition that lands the object on
    /// walkable ground. Included to show that the clamp this file measures is **not** gravity.
    pub const ETHEREAL_PS: u32 = 0x0000_0004;

    pub fn outdoor(x: f32, y: f32, z: f32) -> Position {
        let mut cell = LandblockId(HOLTBURG).cell(1);
        let mut o = Vec3::new(x, y, z);
        dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut o);
        Position::new(cell, dereth_primitives::Frame::new(o, Quat::IDENTITY))
    }

    pub struct Bench {
        gpu: Gpu,
        pub scene: WorldScene,
        store: Arc<RetailDatStore>,
        objects: ObjectStream,
        now: f64,
        instance: u16,
    }

    impl Bench {
        pub fn new(store: &Arc<RetailDatStore>, mut gpu: Gpu, land_radius: u32) -> Self {
            let region = dereth_client::world::load_region(store).expect("the region decodes");
            let cfg = SceneConfig {
                landblock: HOLTBURG,
                land_radius,
                time_of_day: Some(0.78),
                particles: false,
                ..SceneConfig::default()
            };
            let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
            scene
                .attach_character(store, &region, &mut gpu)
                .expect("the body is created");
            let mut objects = ObjectStream::new();
            objects.world.player = Some(PLAYER_OBJECT_ID);
            let mut me = dereth_client_model::Weenie::new(PLAYER_OBJECT_ID);
            me.valid = true;
            me.has_phys_obj = true;
            me.qualities = Some(dereth_client_model::qualities::Qualities::new());
            objects.world.tables.weenies.insert(PLAYER_OBJECT_ID, me);
            Self {
                gpu,
                scene,
                store: store.clone(),
                objects,
                now: 1.0,
                instance: 0,
            }
        }

        pub fn height(&self, x: f32, y: f32) -> Option<f32> {
            self.scene
                .character
                .as_ref()?
                .world
                .terrain_height_at(&outdoor(x, y, 0.0))
        }

        pub fn stand_at_the_recorded_station(&mut self) {
            let z = self
                .height(STATION.0, STATION.1)
                .expect("the station is on terrain");
            let mut p = outdoor(STATION.0, STATION.1, z + 0.01);
            p.frame.rotation = Quat::IDENTITY; // yaw 0 = north
            self.scene.character.as_mut().expect("a body").teleport(p);
        }

        /// `scale` supplies `PhysicsDesc.object_scale`, setting body size; a distant object needs
        /// it, because an unscaled body at a kilometre is two or three pixels.
        pub fn place(&mut self, at: Position, scale: f32, state: u32) {
            self.instance += 1;
            let payload = dereth_protocol::objects::ObjectCreatePayload {
                id: TARGET,
                objdesc: ObjDesc::default(),
                physicsdesc: PhysicsDesc {
                    bitfield: flags::POSITION | flags::SETUP | flags::OBJSCALE,
                    object_scale: Some(scale),
                    setup_id: Some(NEVER_DARK),
                    state,
                    position: Some(dereth_protocol::types::PositionWire {
                        objcell_id: at.cell.0,
                        frame: dereth_protocol::types::Frame {
                            origin: at.frame.origin.into(),
                            orientation: Quat::IDENTITY.into(),
                        },
                    }),
                    timestamps: dereth_protocol::types::PhysicsTimestamps {
                        instance: self.instance,
                        ..dereth_protocol::types::PhysicsTimestamps::default()
                    },
                    ..PhysicsDesc::default()
                },
                wdesc: PublicWeenieDesc::default(),
            };
            let body =
                dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
                    .expect("encode");
            self.objects.apply_event(
                &SessionEvent::WorldObject {
                    opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                    body,
                },
                LocalTime(self.now),
            );
            {
                let Self {
                    store,
                    objects,
                    scene,
                    ..
                } = self;
                let ch = scene.character.as_mut().expect("a body");
                objects.sync_physics(store, &mut ch.world);
            }
            let w = self
                .objects
                .world
                .tables
                .weenies
                .get_mut(TARGET)
                .expect("placed");
            w.pwd.obj_type |= item_type::CREATURE;
            w.pwd.bitfield |= bitfield::ATTACKABLE;
            w.pwd.radar_enum = Some(4);
            self.objects.world.update_visible_object_list();
        }

        pub fn draw(&mut self) {
            self.now += 1.0;
            let Self {
                store,
                gpu,
                scene,
                objects,
                ..
            } = self;
            scene
                .sync_objects(store, gpu, objects)
                .expect("sync_objects");
            scene.update(
                dereth_client::camera::CameraInput::default(),
                CharacterInput::default(),
                LocalTime(self.now),
                1.0 / 30.0,
            );
            scene.stream(store, gpu).expect("stream");
            scene.reserve_upload_arena(gpu).expect("reserve the arena");
            gpu.begin_frame().expect("begin");
            scene.draw(gpu).expect("draw");
            gpu.end_frame().expect("end");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The watertightness question, and the control that could not answer it
// ---------------------------------------------------------------------------------------------

/// **A request to bury an object is discarded, so a "buried" object's pixels are not a leak
/// through the far terrain.** An object requested 60 m under the ground can still paint a few
/// pixels because the placement did not bury it, not because the terrain is not watertight.
///
/// The test reads the object's drawn origin for requests 60 m, 200 m and **2 000 m** below ground
/// at 1 008 m range: its height is bit-identical to the ground-level request's, while a 40 m lift
/// moves it 40 m. That shows the downward placement is rejected; it does not count pixels. The
/// submission trace's sort key cannot answer this: an outdoor object more than fifty units away
/// sorts at its land cell's horizontal distance, the same at every height.
///
/// A buried-object control must therefore establish that burial occurred before treating its
/// pixels as a leak floor. This test sets `ETHEREAL_PS`, retaining the control that excludes
/// ordinary gravity as the explanation. Which placement step rejects the request is not located
/// by this instrument.
///
/// The seam tests above independently inspect generated mesh edges: sampled inward cardinal
/// seams must not rise above their fine neighbors, and sampled outward cardinal seams must agree,
/// each within 1 mm. These checks cannot be fooled by an object that did not move; they are not
/// a census of every boundary or an assertion of identical inward-edge heights.
#[test]
fn a_request_to_bury_an_object_is_discarded_so_it_cannot_be_a_leak_floor() {
    let gpu = crate::common::test_gpu(bench::W, bench::H);
    let store = std::sync::Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut b = bench::Bench::new(&store, gpu, 8);
    b.stand_at_the_recorded_station();
    for _ in 0..12 {
        b.draw();
    }
    // The landscape draw-distance station's bearing and range: the crest at 864 m, the target
    // ground at 1 008 m.
    let (bs, bc) = math::sin_cosf(17.5f32.to_radians());
    let d = 1008.0f32;
    let (x, y) = (bench::STATION.0 + bs * d, bench::STATION.1 + bc * d);
    let ground = b
        .height(x, y)
        .expect("the target bearing has ground under it");

    let mut reading = Vec::new();
    for depth in [0.0f32, 60.0, 200.0, 2000.0, -40.0] {
        b.place(
            bench::outdoor(x, y, ground - depth),
            4.0,
            bench::ETHEREAL_PS,
        );
        for _ in 0..3 {
            b.draw();
        }
        let subsets = b
            .scene
            .drawn_part_order()
            .iter()
            .filter(|e| e.object == Some(bench::TARGET))
            .count();
        assert!(
            subsets > 0,
            "the object was not submitted at depth {depth}, so nothing here is measured"
        );
        // The drawn origin, not the sort key: an outdoor object this far away sorts at its land
        // cell's horizontal distance, which no change of height can move.
        let z = b
            .scene
            .objects
            .get(&bench::TARGET)
            .expect("the object is in the world")
            .frame
            .origin
            .z;
        eprintln!(
            "burial: requested z = ground - {depth:6.0} m (ground {ground:.1} m at {d:.0} m) \
             -> {subsets} subsets, drawn at z {z:.4}"
        );
        reading.push((depth, z));
    }
    let on_ground = reading[0].1;
    // **The positive control first**, because without it the equalities below are a blind
    // instrument: the drawn height must be *able* to move when the object does.
    let lifted = reading[4].1;
    assert!(
        (lifted - on_ground - 40.0).abs() < 0.5,
        "lifting the object 40 m moved its drawn height by {:.4} m, so this instrument cannot \
         see the object move and proves nothing about burial",
        lifted - on_ground
    );
    // The finding for all three tested burial depths: the drawn height is unchanged.
    for (depth, z) in &reading[1..4] {
        assert_eq!(
            z.to_bits(),
            on_ground.to_bits(),
            "a request to bury the object {depth} m changed its drawn height from {on_ground} to \
             {z}; if burial now works then a buried object's pixels are a real leak reading \
             again and this test should be rewritten rather than relaxed"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Why `scenery_radius` is right to ship 1
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.scenery.scenery-stops-at-the-full-detail-ring
/// **`SceneConfig::scenery_radius: 1` is not a budget that undershoots retail; it *is* retail's
/// gate, to the block.**
///
/// Visible-cell collection visits every slot of the square landscape window without
/// a separate scenery-radius test, invoking building, static-scenery and dynamic-scenery
/// initialization. Each initializer rejects blocks that are not full detail, so these
/// populations require **`side_cell_count == 8`**.
///
/// The block-orientation rule gives 8 exactly for
/// `max(|dx|, |dy|) <= 1`. That is the 3x3 core, which is precisely `wants_objects` at
/// `scenery_radius == 1`, and no window size can widen it because the full-detail ring is fixed.
///
/// So retail's own scenery reach is one block beyond the viewer's while its terrain reaches eight.
/// What is worth keeping is the coupling: if `scenery_radius` is ever raised, `lod_object_guard`
/// is what still makes the rule *objects if and only if full detail*.
#[test]
fn the_shipped_scenery_radius_selects_exactly_the_full_detail_blocks() {
    // `scenery_radius == 1` selects exactly the blocks `block_orient` gives full detail, over a
    // window far wider than either rule needs.
    let shipped = dereth_client::world::SceneConfig::default().scenery_radius;
    assert_eq!(
        shipped, 1,
        "scenery_radius no longer ships 1, so the equality below is about a different knob"
    );
    #[allow(clippy::cast_possible_wrap)]
    // LINT-OK: a window radius, at most 15. Not a float conversion.
    let sr = shipped as i32;
    let mut both = 0usize;
    for dx in -15..=15 {
        for dy in -15..=15 {
            let full_detail = side_cell_count(block_orient(dx, dy).0) == 8;
            let within = dx.abs() <= sr && dy.abs() <= sr;
            assert_eq!(
                full_detail, within,
                "block ({dx}, {dy}): retail's gate says full_detail={full_detail} but \
                 scenery_radius {sr} says {within}"
            );
            if within {
                both += 1;
            }
        }
    }
    assert_eq!(
        both, 9,
        "the full-detail core is not the 3x3 it should be ({both} blocks)"
    );
    eprintln!(
        "scenery gate: over a 31x31 window, `scenery_radius {sr}` and `side_cell_count == 8` \
         select the same {both} blocks -- the shipped value is retail's own reach"
    );
}
