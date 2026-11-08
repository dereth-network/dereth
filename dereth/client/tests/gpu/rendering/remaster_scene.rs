//! Behaviour: none (experimental)
//!
//! The remaster's copy of the scene, and the walk that only decides.
//!
//! - **The copy.** What the scene publishes after each frame's walk builds a retained scene with
//!   no device that holds exactly what the walk holds: the same blocks, the same static geometry
//!   in every block and every room (counted vertex for vertex from the dats against the batches
//!   the walk draws), the same rooms and openings, the same lights, the same moving objects at the
//!   same levels, and the same degrade levels. It is told exactly what the walk drew: the rooms,
//!   their statics' parts, the objects' parts, and the blocks of the landscape, whose cut-outs it
//!   counts as the walk does. That holds on every frame of a streamed walk across Holtburg, and
//!   indoors, in a doorway and underground.
//! - **Deciding only.** A walk that only decides makes every decision the drawing walk makes on
//!   the same frame (what can be picked, which cells and parts are drawn, the selected object's
//!   latch, every degrade level, the cut-outs and the cells' batches, the openings seen) and puts
//!   nothing on the device: no draw, no opening stamped. The walk drawing the same frame again
//!   after it decides the same.
//! - **Off is off.** With a receiver installed, and after the walk has only decided and draws
//!   again, the frame is the one drawn with neither: the same recording, the same pixels and the
//!   same counters, on the `wgpu` device and the Vulkan device.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.

#![cfg(all(gpu, feature = "remaster"))]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, ObjectId};
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::remaster::{self, ObjectKey, Retained};
use dereth_scene::world_scene::WorldScene;

use crate::instruments::stations::{self, capture, moved, Clock, Place, Shot, Station};

const W: u32 = 640;
const H: u32 = 480;

/// Holtburg's server objects made for the tests: humans, standing in the town.
const PEOPLE: [u32; 4] = [0x8300_2000, 0x8300_2001, 0x8300_2002, 0x8300_2003];

/// The station named `name`.
fn station(store: &RetailDatStore, name: &str) -> Station {
    stations::stations(store)
        .into_iter()
        .chain(stations::close_stations(store))
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no station {name}"))
}

/// A device of the default backend for the tier, on hardware.
fn device() -> Gpu {
    let gpu = crate::common::test_gpu(W, H);
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasteriser",
        gpu.adapter_name()
    );
    gpu
}

/// A retained scene, installed as `shot`'s receiver.
fn retain(shot: &mut Shot) -> Rc<RefCell<Retained>> {
    let r = Rc::new(RefCell::new(Retained::new()));
    assert!(shot
        .scene
        .draw
        .install_remaster_receiver(Box::new(Rc::clone(&r)))
        .is_none());
    r
}

/// The block `(x, y)` as the remaster names it.
fn block_id((x, y): (i32, i32)) -> u16 {
    remaster::block_id(
        u8::try_from(x).expect("a block index"),
        u8::try_from(y).expect("a block index"),
    )
}

/// Each GfxObj's vertex count, as the bake lays its triangles down, read once.
struct Meshes<'a> {
    store: &'a RetailDatStore,
    // ORDER-OK: a memo, only looked up.
    vertices: HashMap<u32, usize>,
}

impl Meshes<'_> {
    fn vertices(&mut self, id: u32) -> usize {
        let store = self.store;
        *self.vertices.entry(id).or_insert_with(|| {
            dereth_client_runtime::models::build_gfxobj(store, DataId(id))
                .iter()
                .map(|g| g.vertices.len())
                .sum()
        })
    }

    /// Every vertex `p`'s parts lay down at every level, its degrade records `degrades`.
    fn placement(&mut self, p: &remaster::Placement, degrades: &[remaster::Degrade]) -> usize {
        p.parts
            .iter()
            .map(|q| match q.degrade {
                Some(i) => degrades[i as usize]
                    .levels
                    .iter()
                    .filter(|l| **l != 0)
                    .map(|l| self.vertices(*l))
                    .sum(),
                None => self.vertices(q.gfxobj),
            })
            .sum()
    }
}

/// What one comparison found.
#[derive(Debug, Default, Clone, Copy)]
struct Seen {
    blocks: usize,
    placements: usize,
    vertices: usize,
    cells: usize,
    static_lights: usize,
    frame_lights: usize,
    degrades: usize,
    objects: usize,
    drawn_cells: usize,
    drawn_cell_parts: usize,
    drawn_parts: usize,
    drawn_blocks: usize,
    cut_outs: usize,
}

/// Check that the retained scene `r` holds what `scene` holds and was told what its last walk
/// drew; with `geometry`, count every block's and room's static vertices from the dats too.
#[allow(clippy::too_many_lines)]
fn agree(scene: &WorldScene, r: &Retained, meshes: &mut Meshes<'_>, geometry: bool) -> Seen {
    let counts = r.counts();
    assert_eq!(counts.faults, 0, "the deltas did not fit what was held");
    let draw = &scene.draw;
    let ws = &scene.world;
    let mut seen = Seen {
        blocks: counts.blocks,
        placements: counts.placements,
        cells: counts.cells,
        static_lights: counts.static_lights,
        frame_lights: counts.frame_lights,
        degrades: counts.degrades,
        objects: counts.objects,
        ..Seen::default()
    };

    // The blocks, their statics, their rooms and their openings.
    let census = draw.static_census();
    // ORDER-OK: keyed lookups.
    let held: BTreeMap<u16, &remaster::Block> = r.blocks().map(|b| (b.block, b)).collect();
    let resident: BTreeSet<u16> = census.iter().map(|c| block_id(c.block)).collect();
    assert_eq!(
        held.keys().copied().collect::<BTreeSet<u16>>(),
        resident,
        "the blocks held"
    );
    for c in &census {
        let b = held[&block_id(c.block)];
        let rooms: Vec<(u32, usize)> = b.rooms.iter().map(|r| (r.id, r.portals.len())).collect();
        let want: Vec<(u32, usize)> = c.cells.iter().map(|&(id, p, _)| (id, p)).collect();
        assert_eq!(rooms, want, "block {:04x}'s rooms", b.block);
        if geometry {
            let own: usize = b
                .placements
                .iter()
                .map(|p| meshes.placement(p, &b.degrades))
                .sum();
            assert_eq!(own, c.vertices, "block {:04x}'s statics", b.block);
            seen.vertices += own;
            for (room, &(id, _, vertices)) in b.rooms.iter().zip(&c.cells) {
                let got: usize = room
                    .placements
                    .iter()
                    .map(|p| meshes.placement(p, &b.degrades))
                    .sum();
                assert_eq!(got, vertices, "room {id:08x}'s statics");
                seen.vertices += got;
            }
        }
    }

    // The degrade levels, the blocks' own and then each room's, block by block.
    let levels: Vec<u8> = held
        .keys()
        .flat_map(|id| r.levels(*id).map(|l| l.levels.clone()).unwrap_or_default())
        .collect();
    let want: Vec<u8> = draw
        .degrade_probe(ws)
        .iter()
        .map(|p| u8::try_from(p.level).expect("a level"))
        .collect();
    assert_eq!(levels, want, "the degrade levels");

    // The lights.
    assert_eq!(
        counts.static_lights,
        draw.cell_light_objects(),
        "the rooms' lights"
    );
    let pools = draw.light_pools();
    assert_eq!(
        counts.frame_lights,
        pools.statics.len() + pools.dynamics.len(),
        "the light pools"
    );

    // The moving objects, and the level each part draws at.
    let frame = r.frame();
    // ORDER-OK: keyed lookups.
    let posed: HashMap<ObjectKey, &remaster::ObjectPose> =
        frame.objects.iter().map(|p| (p.key, p)).collect();
    let servers = r
        .objects()
        .filter(|o| matches!(o.key, ObjectKey::Server(_)))
        .count();
    assert_eq!(servers, ws.objects.len(), "the server's objects");
    seen.objects = servers;
    assert_eq!(
        r.objects().any(|o| o.key == ObjectKey::Body),
        ws.character.is_some(),
        "the body"
    );
    for p in draw.part_degrade_probe(ws) {
        let key = p
            .object
            .map_or(ObjectKey::Body, |id| ObjectKey::Server(id.0));
        let pose = posed
            .get(&key)
            .unwrap_or_else(|| panic!("{key:?} is not posed"));
        assert_eq!(
            u32::from(pose.parts[p.part].level),
            p.level,
            "{key:?} part {}'s level",
            p.part
        );
    }

    // What the walk drew.
    let cells: BTreeSet<u32> = frame.drawn_cells.iter().copied().collect();
    assert_eq!(
        Some(cells),
        draw.drawn_cells(),
        "the interior cells the walk drew"
    );
    seen.drawn_cells = frame.drawn_cells.len();
    let parts: BTreeSet<(u32, u32)> = draw
        .cell_static_parts()
        .iter()
        .filter(|p| p.drawn_by.is_some())
        .map(|p| (p.cell.0, p.part))
        .collect();
    assert_eq!(
        frame
            .drawn_cell_parts
            .iter()
            .copied()
            .collect::<BTreeSet<_>>(),
        parts,
        "the rooms' statics' parts the walk drew"
    );
    seen.drawn_cell_parts = frame.drawn_cell_parts.len();
    let body = ws.character.as_ref().map(|c| c.object_id());
    let picked: BTreeSet<ObjectId> = frame
        .drawn_parts
        .iter()
        .filter_map(|p| match p.key {
            ObjectKey::Server(id) => Some(ObjectId(id)),
            ObjectKey::Body => body,
            ObjectKey::Animated { .. } => None,
        })
        .collect();
    assert_eq!(
        Some(picked),
        draw.drawn_objects(),
        "the objects the walk drew"
    );
    seen.drawn_parts = frame.drawn_parts.len();
    for p in &frame.drawn_parts {
        assert!(
            usize::from(p.count) <= 8
                && p.lights[..usize::from(p.count)]
                    .iter()
                    .all(|i| usize::from(*i) < frame.light.lights.len()),
            "{p:?}'s lights"
        );
        assert!(
            !p.outdoors || p.count == 0,
            "{p:?} lit by more than the sun"
        );
    }
    let cut_outs: BTreeMap<u16, usize> = census
        .iter()
        .map(|c| (block_id(c.block), c.cut_outs))
        .collect();
    let drawn_cut_outs: usize = frame.drawn_blocks.iter().map(|b| cut_outs[b]).sum();
    assert_eq!(
        drawn_cut_outs,
        draw.drawn_landscape_alpha().clip,
        "the cut-outs of the blocks the walk drew"
    );
    if frame.landscape && ws.viewer_cell().is_none() {
        assert_eq!(
            frame
                .drawn_blocks
                .iter()
                .copied()
                .collect::<BTreeSet<u16>>(),
            resident,
            "outdoors, the walk draws every block"
        );
    }
    if !frame.landscape {
        assert!(frame.drawn_blocks.is_empty());
    }
    seen.drawn_blocks = frame.drawn_blocks.len();
    seen.cut_outs = drawn_cut_outs;
    seen
}

/// After a streamed walk across Holtburg and back, 300 frames, with people standing in the town,
/// the retained scene holds on every frame exactly what the walk holds, and was told exactly what
/// it drew.
#[test]
fn the_retained_scene_holds_what_the_walk_holds_through_a_streamed_walk() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut gpu = device();
    let town = station(&store, "holtburg");
    let mut shot = Shot::open(&store, &mut gpu, &town);
    let r = retain(&mut shot);
    let mut meshes = Meshes {
        store: &store,
        vertices: HashMap::new(),
    };
    for (i, id) in PEOPLE.iter().take(3).enumerate() {
        #[allow(clippy::cast_precision_loss)] // three people
        let x = 90.0 + i as f32 * 3.0;
        shot.create_object(
            *id,
            (0x0200_0001, Some(0x0900_0001)),
            stations::HOLTBURG,
            (x, 80.0),
            180.0,
        );
    }
    let mut most = Seen::default();
    let mut origins = BTreeSet::new();
    let mut published = 0u64;
    for frame in 0..300u32 {
        // East two metres a frame for 150 frames, across two of the town's block lines, then
        // back.
        let east = if frame < 150 { 2.0 } else { -2.0 };
        shot.scene.camera.position.x += east;
        if frame == 150 {
            shot.create_object(
                PEOPLE[3],
                (0x0200_0001, Some(0x0900_0001)),
                stations::HOLTBURG,
                (96.0, 84.0),
                0.0,
            );
        }
        shot.step(&mut gpu);
        let held = r.borrow();
        let seen = agree(
            &shot.scene,
            &held,
            &mut meshes,
            frame % 50 == 0 || frame == 299,
        );
        origins.insert(held.frame().origin);
        published = held.counts().frames;
        most.blocks = most.blocks.max(seen.blocks);
        most.placements = most.placements.max(seen.placements);
        most.vertices = most.vertices.max(seen.vertices);
        most.objects = most.objects.max(seen.objects);
        most.drawn_parts = most.drawn_parts.max(seen.drawn_parts);
        most.drawn_blocks = most.drawn_blocks.max(seen.drawn_blocks);
        most.cut_outs = most.cut_outs.max(seen.cut_outs);
        most.degrades = most.degrades.max(seen.degrades);
        most.cells = most.cells.max(seen.cells);
        most.frame_lights = most.frame_lights.max(seen.frame_lights);
    }
    eprintln!("streamed walk: {most:?}, {published} frames, origins {origins:?}");
    assert_eq!(published, 300, "a frame published for every frame drawn");
    assert!(
        origins.len() >= 3,
        "the walk crossed two block lines: {origins:?}"
    );
    assert!(most.blocks >= 25 && most.placements > 1000, "{most:?}");
    assert!(most.vertices > 100_000 && most.degrades > 100, "{most:?}");
    assert_eq!(most.objects, PEOPLE.len(), "{most:?}");
    assert!(most.drawn_parts > 0 && most.cut_outs > 0, "{most:?}");
    assert!(most.cells > 0, "Holtburg's buildings have rooms: {most:?}");
}

/// In a house, in its doorway and underground, the retained scene holds on every frame what the
/// walk holds, with the body and its light, and was told which rooms and which of their statics
/// the walk drew, and which lights each object part indoors was drawn with.
#[test]
fn the_retained_scene_holds_what_the_walk_holds_indoors() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut gpu = device();
    let mut meshes = Meshes {
        store: &store,
        vertices: HashMap::new(),
    };
    for name in ["indoor", "doorway", "dungeon"] {
        let at = station(&store, name);
        let mut shot = Shot::open(&store, &mut gpu, &at);
        let r = retain(&mut shot);
        let mut most = Seen::default();
        let mut lit = 0usize;
        for frame in 0..60u32 {
            shot.step(&mut gpu);
            let held = r.borrow();
            let seen = agree(
                &shot.scene,
                &held,
                &mut meshes,
                frame % 20 == 0 || frame == 59,
            );
            lit += held
                .frame()
                .drawn_parts
                .iter()
                .filter(|p| !p.outdoors && p.count > 0)
                .count();
            most.cells = most.cells.max(seen.cells);
            most.drawn_cells = most.drawn_cells.max(seen.drawn_cells);
            most.drawn_cell_parts = most.drawn_cell_parts.max(seen.drawn_cell_parts);
            most.frame_lights = most.frame_lights.max(seen.frame_lights);
            most.drawn_parts = most.drawn_parts.max(seen.drawn_parts);
            most.static_lights = most.static_lights.max(seen.static_lights);
        }
        eprintln!("{name}: {most:?}, {lit} part draws lit indoors");
        assert!(
            most.drawn_cells > 0 && most.drawn_cell_parts > 0,
            "{name}: {most:?}"
        );
        assert!(most.frame_lights > 0, "{name}: the body's light: {most:?}");
        assert!(lit > 0, "{name}: no part was drawn with the lights indoors");
    }
}

/// Everything the walk decided on the last frame, as its probes say.
#[derive(Debug, Clone, PartialEq)]
struct Decided {
    picked: Option<BTreeSet<ObjectId>>,
    cells: Option<BTreeSet<u32>>,
    latch: bool,
    static_levels: Vec<(u32, dereth_world_render::objects::degrade::DegradeMode)>,
    part_levels: Vec<(Option<ObjectId>, usize, u32)>,
    cell_parts: Vec<(u32, u32, Option<u32>)>,
    landscape: dereth_scene::world_scene::LandscapeAlphaStats,
    cell_statics: dereth_scene::world_scene::CellStaticDrawStats,
    cone: dereth_scene::world_scene::ObjectConeStats,
    openings: Vec<Vec<(f32, f32)>>,
    stamps: (u64, u64),
    published: Option<Published>,
}

/// What the last published frame said the walk drew: the outdoors, its blocks, the rooms'
/// statics' parts and the objects' parts.
type Published = (bool, Vec<u16>, Vec<(u32, u32)>, Vec<remaster::DrawnPart>);

fn decided(scene: &WorldScene, published: Option<&Rc<RefCell<Retained>>>) -> Decided {
    let draw = &scene.draw;
    let ws = &scene.world;
    Decided {
        picked: draw.drawn_objects(),
        cells: draw.drawn_cells(),
        latch: draw.take_selected_part_drawn(),
        static_levels: draw
            .degrade_probe(ws)
            .iter()
            .map(|p| (p.level, p.mode))
            .collect(),
        part_levels: draw
            .part_degrade_probe(ws)
            .iter()
            .map(|p| (p.object, p.part, p.level))
            .collect(),
        cell_parts: draw
            .cell_static_parts()
            .iter()
            .map(|p| (p.cell.0, p.part, p.drawn_by.map(|c| c.0)))
            .collect(),
        landscape: draw.drawn_landscape_alpha(),
        cell_statics: draw.drawn_cell_statics(),
        cone: draw.drawn_object_cone(),
        openings: draw.outside_view_polys(),
        stamps: draw.drawn_portal_stamps(),
        published: published.map(|r| {
            let r = r.borrow();
            let f = r.frame();
            (
                f.landscape,
                f.drawn_blocks.clone(),
                f.drawn_cell_parts.clone(),
                f.drawn_parts.clone(),
            )
        }),
    }
}

/// On every frame at five stations (a walk through Holtburg's streets past people, a selected one
/// among them, a house, its doorway, underground and a dungeon's candles), the walk that only
/// decides makes exactly the decisions the drawing walk made on that frame and draws nothing: no
/// draw and no opening stamped. Drawing the same frame again decides the same once more.
#[test]
fn the_walk_that_only_decides_decides_as_the_drawing_walk_and_draws_nothing() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut gpu = device();
    for name in [
        "holtburg",
        "indoor",
        "doorway",
        "dungeon",
        "dungeon-candles",
    ] {
        let at = station(&store, name);
        let mut shot = Shot::open(&store, &mut gpu, &at);
        let published = (name != "indoor").then(|| retain(&mut shot));
        if name == "holtburg" {
            for (i, id) in PEOPLE.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)] // four people
                let x = 90.0 + i as f32 * 3.0;
                shot.create_object(
                    *id,
                    (0x0200_0001, Some(0x0900_0001)),
                    stations::HOLTBURG,
                    (x, 80.0),
                    180.0,
                );
            }
        }
        let mut latched = 0;
        let mut drawn = 0u64;
        for frame in 0..40u32 {
            if name == "holtburg" {
                shot.scene.camera.position.y += 0.6;
                shot.scene.camera.yaw += 0.01;
            }
            shot.step(&mut gpu);
            // What the step's own frame saw goes; then the same frame three times: drawn,
            // decided only, and drawn again, each with the selected object watched.
            let _ = shot.scene.draw.take_selected_part_drawn();
            let selected = if name == "holtburg" {
                Some(ObjectId(PEOPLE[1]))
            } else {
                shot.scene.character.as_ref().map(|c| c.object_id())
            };
            shot.scene.draw.set_selected_object_id(selected);
            let calls = gpu.draw_calls();
            shot.draw(&mut gpu);
            drawn += gpu.draw_calls() - calls;
            let drawing = decided(&shot.scene, published.as_ref());
            shot.scene.draw.set_decide_only(true);
            let (calls, stamps) = (gpu.draw_calls(), gpu.portal_stamps());
            shot.draw(&mut gpu);
            assert_eq!(
                gpu.draw_calls() - calls,
                0,
                "{name} frame {frame}: the walk that only decides drew"
            );
            assert_eq!(
                gpu.portal_stamps() - stamps,
                0,
                "{name} frame {frame}: the walk that only decides stamped an opening"
            );
            let deciding = decided(&shot.scene, published.as_ref());
            shot.scene.draw.set_decide_only(false);
            assert_eq!(
                deciding, drawing,
                "{name} frame {frame}: the walk that only decides decided otherwise"
            );
            shot.draw(&mut gpu);
            let again = decided(&shot.scene, published.as_ref());
            assert_eq!(
                again, drawing,
                "{name} frame {frame}: the walk drew the same frame otherwise"
            );
            latched += usize::from(drawing.latch);
        }
        eprintln!("{name}: {drawn} draws over 40 frames, the latch up on {latched}");
        assert!(drawn > 40, "{name}: the drawing walk drew");
        assert!(latched > 0, "{name}: the selected object was never in view");
    }
}

/// A still view of Holtburg: the ground under the town looked straight down at, no sky, no
/// weather, so every frame is the same frame.
fn still() -> Station {
    Station {
        name: "still",
        why: "the ground under the town, looked straight down at, with no sky in view",
        place: Place::Free {
            block: stations::HOLTBURG,
            x: 96.0,
            y: 96.0,
            eye: 30.0,
            yaw: 0.0,
            pitch: -1.2,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    }
}

/// The frame `shot` stands at drawn again: its recording digest (on the `wgpu` device), its
/// pixels, and what the device counted for it (its draws, the frames it presented, the textures
/// and texture slots it holds after it, the render passes it encoded where it counts them).
fn frame(shot: &mut Shot, gpu: &mut Gpu) -> (Option<u64>, Vec<u8>, String) {
    let (calls, stamp, passes) = (gpu.draw_calls(), gpu.frame_stamp(), gpu.passes_encoded());
    shot.draw(gpu);
    let line = format!(
        "draw_calls={} frames={} live_textures={} descriptors={:?} passes={:?}",
        gpu.draw_calls() - calls,
        gpu.frame_stamp() - stamp,
        gpu.live_textures(),
        gpu.descriptor_stats(),
        gpu.passes_encoded().zip(passes).map(|(a, b)| a - b),
    );
    (gpu.last_frame_digest(), capture(gpu), line)
}

/// A still view of Holtburg draws the same frame with nothing installed, with a retained scene
/// receiving it, after the walk has only decided five times, and with the receiver removed: the
/// same recording on the `wgpu` device, and the same pixels and counts on it and on the Vulkan
/// and Direct3D 12 devices. Deciding only, the walk draws nothing.
#[test]
fn the_frame_is_unchanged_with_a_receiver_and_after_deciding_only() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut backends = vec![Backend::Wgpu, Backend::Vulkan];
    if cfg!(all(windows, feature = "d3d12")) {
        backends.push(Backend::D3d12);
    }
    for backend in backends {
        let cfg = DeviceConfig {
            width: W,
            height: H,
            ..DeviceConfig::default()
        };
        let mut gpu = Gpu::new_on(backend, None, &cfg).expect("the device opens");
        gpu.set_frame_digest(true);
        let mut shot = Shot::open(&store, &mut gpu, &still());
        for _ in 0..6 {
            shot.step(&mut gpu);
        }
        let plain = frame(&mut shot, &mut gpu);
        let again = frame(&mut shot, &mut gpu);
        assert_eq!(
            (again.0, &again.2, moved(&again.1, &plain.1)),
            (plain.0, &plain.2, 0),
            "{}: the view is not still, so this test cannot see",
            backend.name()
        );
        let r = retain(&mut shot);
        let received = frame(&mut shot, &mut gpu);
        assert_eq!(r.borrow().counts().frames, 1);
        shot.scene.draw.set_decide_only(true);
        for _ in 0..5 {
            let calls = gpu.draw_calls();
            shot.draw(&mut gpu);
            assert_eq!(
                gpu.draw_calls(),
                calls,
                "{}: drew while deciding",
                backend.name()
            );
        }
        shot.scene.draw.set_decide_only(false);
        let toggled = frame(&mut shot, &mut gpu);
        drop(shot.scene.draw.remove_remaster_receiver());
        let removed = frame(&mut shot, &mut gpu);
        if backend == Backend::Wgpu {
            assert!(plain.0.is_some(), "the wgpu device digests its frames");
        }
        for (what, f) in [
            ("with a receiver", &received),
            ("after deciding only", &toggled),
            ("with the receiver removed", &removed),
        ] {
            let name = backend.name();
            assert_eq!(f.0, plain.0, "{name} {what}: the recording");
            assert_eq!(f.2, plain.2, "{name} {what}: the counts");
            assert_eq!(moved(&f.1, &plain.1), 0, "{name} {what}: the pixels");
        }
        assert_eq!(r.borrow().counts().faults, 0);
    }
}

/// Not a claim: each isolation station drawn by Retail Look on the `wgpu`, Vulkan and Direct3D 12
/// devices as [`every_station_draws_what_the_base_build_did_with_the_remasters_seams_in_use`]
/// draws it, written for that comparison to read in a later build: `<device>/<station>-off.png`,
/// `<device>/counters.txt` and, on `wgpu`, `<device>/digest.txt`, under the folder
/// `DERETH_REMASTER_BASELINE_DIR` names.
#[test]
#[ignore = "instrument: writes a capture of this build; set DERETH_REMASTER_BASELINE_DIR and run with \
            --ignored"]
fn capture_retail_looks_frames_for_a_later_build() {
    use std::fmt::Write as _;
    let _gpu = crate::common::gpu_lock();
    let Ok(root) = std::env::var("DERETH_REMASTER_BASELINE_DIR") else {
        return;
    };
    let store = crate::common::dats();
    let stations = stations::isolation_stations(&store);
    let mut backends = vec![Backend::Wgpu, Backend::Vulkan];
    if cfg!(all(windows, feature = "d3d12")) {
        backends.push(Backend::D3d12);
    }
    for backend in backends {
        let dir = std::path::Path::new(&root).join(backend.name());
        std::fs::create_dir_all(&dir).expect("the capture folder");
        let (mut digests, mut counts) = (String::new(), String::new());
        for station in &stations {
            let cfg = DeviceConfig {
                width: W,
                height: H,
                ..DeviceConfig::default()
            };
            let mut gpu = Gpu::new_on(backend, None, &cfg).expect("the device opens");
            gpu.set_frame_digest(true);
            let mut shot = Shot::open(&store, &mut gpu, station);
            shot.poll(&mut gpu);
            for _ in 1..6 {
                shot.step(&mut gpu);
            }
            let counted = stations::counted_step(&mut shot, &mut gpu);
            let pixels = capture(&mut gpu);
            stations::write_png(
                &dir.join(format!("{}-off.png", station.name)),
                W,
                H,
                &pixels,
            );
            let _ = writeln!(counts, "{} {counted}", station.name);
            if let Some(d) = gpu.last_frame_digest().filter(|_| backend == Backend::Wgpu) {
                let _ = writeln!(digests, "{} {d:016x}", station.name);
            }
        }
        std::fs::write(dir.join("counters.txt"), counts).expect("counters.txt");
        if !digests.is_empty() {
            std::fs::write(dir.join("digest.txt"), digests).expect("digest.txt");
        }
        eprintln!("wrote {}", dir.display());
    }
}

/// How a run draws a station: with a retained scene receiving every frame, or with the walk
/// deciding only for three of its frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    Received,
    Toggled,
}

/// Every isolation station, drawn by this build with a retained scene receiving every frame, and
/// with the walk only deciding for three of its six frames (its second to its fourth), records
/// the same frame, counts the same and draws the same pixels as the build before the remaster's
/// seams existed, on the `wgpu`, Vulkan and Direct3D 12 devices. The base build's frames are the
/// capture [`capture_retail_looks_frames_for_a_later_build`] writes there, one folder per device.
#[test]
#[ignore = "compares with a capture of the base build: set DERETH_REMASTER_BASELINE_DIR and run with \
            --ignored"]
fn every_station_draws_what_the_base_build_did_with_the_remasters_seams_in_use() {
    let _gpu = crate::common::gpu_lock();
    let root = std::env::var("DERETH_REMASTER_BASELINE_DIR")
        .expect("DERETH_REMASTER_BASELINE_DIR names the base build's capture");
    let store = crate::common::dats();
    let stations = stations::isolation_stations(&store);
    let mut backends = vec![Backend::Wgpu, Backend::Vulkan];
    if cfg!(all(windows, feature = "d3d12")) {
        backends.push(Backend::D3d12);
    }
    let mut checked = 0;
    for backend in backends {
        let dir = std::path::Path::new(&root).join(backend.name());
        let digests = std::fs::read_to_string(dir.join("digest.txt")).unwrap_or_default();
        let counts = std::fs::read_to_string(dir.join("counters.txt")).unwrap_or_default();
        for station in &stations {
            let Some((width, height, base)) =
                stations::read_png(&dir.join(format!("{}-off.png", station.name)))
            else {
                panic!(
                    "the base capture has no {} frame of {}",
                    backend.name(),
                    station.name
                );
            };
            for way in [Way::Received, Way::Toggled] {
                let cfg = DeviceConfig {
                    width,
                    height,
                    ..DeviceConfig::default()
                };
                let mut gpu = Gpu::new_on(backend, None, &cfg).expect("the device opens");
                gpu.set_frame_digest(true);
                let mut shot = Shot::open(&store, &mut gpu, station);
                let r = (way == Way::Received).then(|| retain(&mut shot));
                shot.poll(&mut gpu);
                // Toggled, the walk draws its first frame, only decides on the next three and
                // draws again from the fifth: the device makes a frame's pipelines when it first
                // draws with them, and numbers them in that order.
                for step in 1..6 {
                    shot.scene
                        .draw
                        .set_decide_only(way == Way::Toggled && (2..=4).contains(&step));
                    shot.step(&mut gpu);
                }
                let counted = stations::counted_step(&mut shot, &mut gpu);
                let pixels = capture(&mut gpu);
                let what = format!("{} {} {way:?}", backend.name(), station.name);
                if let Some(r) = r {
                    let r = r.borrow();
                    assert_eq!(r.counts().faults, 0, "{what}");
                    assert_eq!(r.counts().frames, 6, "{what}: a frame published a frame");
                }
                let want = stations::listed(&counts, station.name).unwrap_or_else(|| {
                    panic!(
                        "the base capture has no {} counts of {}",
                        backend.name(),
                        station.name
                    )
                });
                assert_eq!(
                    counted, want,
                    "{what}: the device or the scene counted another frame than the base build's"
                );
                let changed = moved(&pixels, &base);
                eprintln!("{what}: {changed} bytes differ");
                assert_eq!(
                    changed, 0,
                    "{what}: the pixels differ from the base build's"
                );
                if backend == Backend::Wgpu {
                    let digest = gpu
                        .last_frame_digest()
                        .expect("the wgpu device digests its frames");
                    let want = stations::listed(&digests, station.name).unwrap_or_else(|| {
                        panic!("the base capture has no digest of {}", station.name)
                    });
                    assert_eq!(
                        format!("{digest:016x}"),
                        want,
                        "{what}: the recorded frame differs from the base build's"
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(checked >= 4 * stations.len(), "{checked}");
}
