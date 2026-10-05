//! How a moving object's parts reach the device. Parts are submitted farthest first by their
//! viewer-distance key, and a part whose selected degrade level names no geometry is not submitted.
//! Non-opaque subsets (mask meeting the alpha-delay mask 0x0E) are deferred: every deferred subset
//! follows the immediate ones, the clip list drains before the blend list regardless of distance,
//! and `first_of_kind` marks only the first deferred subset of a part on each list. Control arms
//! with the sort and the lists switched off submit in server-id order and queue nothing.
//! Fixture: the `first-login-walk-jump` recording replayed into a scene over its own landblock on a
//! software device, no local body; published surface ids are re-read from the dat and rerun
//! through the shared helpers, so these check the scene's wiring. Missing fixtures fail.

#![cfg(gpu)]

use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo};
use dereth_client_net::client_session::testing::capture::{self, peer as addr, Datagram as Record};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, LocalTime, ObjectId, Vec3};
use dereth_render::device::Gpu;
use dereth_terrain::consts::S_ALPHA_DELAY_MASK;
use dereth_world_render::degrade_loop::STARTUP_OBJECT_DISTANCE_2DSQ;
use dereth_world_render::objects::alpha::AlphaList;
use dereth_world_render::objects::degrade::DegradeMode;
use dereth_world_render::objects::draw::{classify_subset, subset_mask};
use dereth_world_render::objects::parts::{update_viewer_distance, PartDraw};
use std::collections::BTreeMap;
use std::sync::Arc;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// The `GfxObjDegradeInfo` at an id, straight from the dat.
fn record(store: &RetailDatStore, did: DataId) -> GfxObjDegradeInfo {
    let bytes = store
        .read_typed(DbType::DegradeInfo, did)
        .unwrap_or_else(|e| {
            panic!("{did:?}: the record the scene is holding is not in the dat: {e}")
        });
    GfxObjDegradeInfo::decode_payload(did, &bytes).expect("it decodes")
}

// ---------------------------------------------------------------------------------------------
// Read fixed capture-line fields directly without adding a JSON parser to the app crate.
// ---------------------------------------------------------------------------------------------

/// A recording's datagrams, through the shared reader (parsed once per test binary).
fn load(session: &str) -> Vec<Record> {
    capture::shared_session(session).to_vec()
}

fn connection_sequence_number(records: &[Record]) -> u32 {
    recording::connection_sequence_number(records).expect("the recording has a LoginRequest")
}

struct Replayed {
    objects: ObjectStream,
    /// The first player-position landblock observed during the replay prefix.
    landblock: u16,
}

/// Replay through the last datagram that leaves the object model nonempty. A later logout can
/// erase that population, so do not blindly replay to the end. This is the last nonempty prefix,
/// not the largest population, and it does not assume every recording ends with a clean logout.
fn populated(session: &str) -> Replayed {
    let records = load(session);
    let seq = connection_sequence_number(&records);
    let run = |limit: usize| -> (ObjectStream, usize, Option<u16>) {
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", seq).expect("host");
        let mut objects = ObjectStream::new();
        let mut entered = false;
        let mut last = 0usize;
        let mut block = None;
        for (index, r) in records.iter().enumerate() {
            if index >= limit {
                break;
            }
            let now = LocalTime(r.t);
            if !r.c2s {
                net.feed(&r.raw, addr(r.pair), now);
            }
            net.tick(now);
            let _ = net.take_outgoing();
            for e in objects.pump(&mut net, now) {
                if let SessionEvent::CharacterSet(set) = &e {
                    if !entered {
                        if let Some(c) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(c.gid, &account);
                            entered = true;
                        }
                    }
                }
            }
            if !objects.is_empty() {
                last = index;
            }
            if block.is_none() {
                if let Some(p) = objects.player().and_then(|id| objects.presence(id)) {
                    if let Some(pos) = p.position {
                        let b = pos.cell.landblock();
                        block = Some((u16::from(b.x()) << 8) | u16::from(b.y()));
                    }
                }
            }
        }
        (objects, last, block)
    };
    let (_, last, _) = run(usize::MAX);
    let (objects, _, block) = run(last + 1);
    assert!(
        !objects.is_empty(),
        "{session}: the scene is empty at its last populated datagram"
    );
    Replayed {
        objects,
        landblock: block.expect("the capture's player has a position"),
    }
}

/// One frame with the camera parked wherever the caller put it, and the object stream synced.
fn frame(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    s: &mut ObjectStream,
    t: f64,
) {
    scene.sync_objects(store, gpu, s).expect("sync_objects");
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(t),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
}

/// The same frame, captured. A separate entry point so that the tests that do not need pixels do
/// not pay for a read-back.
fn shot(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    s: &mut ObjectStream,
    t: f64,
) -> (Vec<u8>, u32, u32) {
    scene.sync_objects(store, gpu, s).expect("sync_objects");
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(t),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    let img = gpu.capture().expect("capture");
    (img.to_rgba(), img.width, img.height)
}

/// Changed pixels between two RGBA buffers of the same size.
fn diff(a: &(Vec<u8>, u32, u32), b: &(Vec<u8>, u32, u32)) -> usize {
    assert_eq!((a.1, a.2), (b.1, b.2), "two captures of different sizes");
    a.0.as_chunks::<4>()
        .0
        .iter()
        .zip(b.0.as_chunks::<4>().0.iter())
        .filter(|(p, q)| p != q)
        .count()
}

/// The mean origin of everything the scene is drawing, in render space. The stations below are
/// offsets from this rather than absolute heights: the capture's objects stand on terrain, and a
/// camera at a fixed low altitude over a hill can be **further** from them than one raised above
/// it.
fn object_centroid(scene: &WorldScene, s: &ObjectStream) -> Vec3 {
    let origins: Vec<Vec3> = s
        .presences()
        .filter_map(|(id, _)| scene.server_object_frame(id))
        .map(|f| f.origin)
        .collect();
    assert!(
        !origins.is_empty(),
        "the scene is drawing nothing to measure a centroid from"
    );
    // LINT-OK: a count of drawn objects, in the hundreds.
    #[allow(clippy::cast_precision_loss)]
    let n = origins.len() as f32;
    let sum = origins.iter().fold(Vec3::ZERO, |a, b| {
        Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
    });
    Vec3::new(sum.x / n, sum.y / n, sum.z / n)
}

/// Park the camera over the objects and let the render space settle, returning the centroid in
/// the space every later station will be measured in.
///
/// The objects' centroid can lie across a landblock boundary (y = 253 m in a 192 m block); moving
/// there recentres streaming and rederives frames against a new origin, so stations measured from
/// the old centroid would appear to jump 100 m. Move, step and remeasure, allowing six attempts and
/// requiring movement below 0.5 m before use.
fn park_over_objects(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    s: &mut ObjectStream,
) -> Vec3 {
    let mut centre = Vec3::ZERO;
    let mut last = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    for i in 0..6 {
        frame(store, gpu, scene, s, 0.1 * f64::from(i));
        centre = object_centroid(scene, s);
        let moved = ((centre.x - last.x).powi(2)
            + (centre.y - last.y).powi(2)
            + (centre.z - last.z).powi(2))
        .sqrt();
        last = centre;
        if moved < 0.5 {
            return centre;
        }
        scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + 50.0);
    }
    panic!("the render space never settled: the centroid is still moving at {centre:?}");
}

/// A scene over the capture's own landblock, with no local body: the objects the *server* owns are
/// this module's subject and a chase camera would move on its own.
fn scene_for(store: &Arc<RetailDatStore>, gpu: &mut Gpu, r: &Replayed, on: bool) -> WorldScene {
    let cfg = SceneConfig {
        landblock: r.landblock,
        character: false,
        land_radius: 1,
        // No scenery: holding the statics' levels too would put a second moving part in a
        // measurement about creatures.
        scenery_radius: 0,
        // Part degrade levels and part billboards stay **on** in both arms -- the level a part
        // picks and the frame it is drawn at.
        part_degrade_levels: true,
        part_billboards: true,
        // Toggle alpha lists and depth sorting together for the combined object-pass comparison;
        // the disabled arm is the unsorted, undeferred submission. The inversion test varies only
        // depth sorting, so the combined pixel arm cannot attribute its result to either.
        part_alpha_lists: on,
        part_depth_sort: on,
        // These stations move the camera above the objects without aiming it, and the view-cone
        // cull would reject much of the population, so it is off: this bench measures sorting, not
        // visibility, whatever the shipped default (on) is.
        object_viewcone: false,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
    scene.set_weather_enabled(false);
    scene
}

/// The distance and level heading every object in an outdoor cell takes when that cell's centre
/// is more than 50 m from the camera across the ground, from the 24 m land-cell grid of the render
/// space (whose origin is a landblock corner); `None` for a nearer or an interior cell.
fn far_cell(origin: Vec3, outdoors: bool, cam: Vec3) -> Option<(f32, Vec3)> {
    let centre = |v: f32| (v / 24.0).floor() * 24.0 + 12.0;
    let (x, y) = (centre(origin.x) - cam.x, centre(origin.y) - cam.y);
    let d = (x * x + y * y).sqrt();
    (outdoors && d > 50.0).then(|| (d, Vec3::new(x / d, y / d, 0.0)))
}

// ---------------------------------------------------------------------------------------------
// 1. Parts use the measured viewer-distance key and descend within each submission run.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.object-parts.are-submitted-farthest-first-by-sort-centre
/// Re-read the graphics object's sort center and rerun update_viewer_distance with the probe's
/// transform/scale and current camera. Shared arithmetic checks wiring; it is not a new formula oracle.
#[test]
fn every_part_is_submitted_farthest_first_on_the_clients_own_cypt() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut r = populated("first-login-walk-jump");
    let mut scene = scene_for(&store, &mut gpu, &r, true);
    let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);

    let mut centres: BTreeMap<u32, Vec3> = BTreeMap::new();
    let (mut checked, mut compared) = (0usize, 0usize);
    let mut runs_checked = 0usize;

    for (i, dz) in [8.0f32, 25.0, 80.0, 250.0].into_iter().enumerate() {
        scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
        // LINT-OK: a station index.
        #[allow(clippy::cast_precision_loss)]
        let t = 40.0 + i as f64;
        frame(&store, &mut gpu, &mut scene, &mut r.objects, t);
        let cam = scene.camera.position;

        // Recompute the scene's distance key using the independently reread DAT sort center
        // and the same public helper. Published transform/scale remain the other inputs.
        let probe = scene.part_degrade_probe();
        assert!(probe.len() > 100, "station {i}: only {} parts", probe.len());
        for p in &probe {
            let want_centre = *centres.entry(p.gfxobj.0).or_insert_with(|| {
                let bytes = store.read_typed(DbType::GfxObj, p.gfxobj).unwrap_or_else(|e| {
                    panic!("{:?}: the graphics object from which level 0 was built is not in the dat: {e}", p.gfxobj)
                });
                GfxObj::decode_payload(p.gfxobj, &bytes).expect("it decodes").sort_center
            });
            assert_eq!(
                p.sort_center, want_centre,
                "{:?}: the bake is holding sort_center {:?} and the dat says {want_centre:?}",
                p.gfxobj, p.sort_center
            );
            let pd = PartDraw {
                pos: p.pos,
                draw_pos: p.pos,
                gfxobj_scale: p.gfxobj_scale,
                cypt: 0.0,
                deg_level: 0,
                deg_mode: DegradeMode::None,
                no_draw: false,
            };
            // A part of an object in a land cell more than 50 m away sorts at the cell's distance.
            // Otherwise the scene's governor is pinned, so the object share distance is its
            // startup one: a part of an object at or past it horizontally sorts at the object's
            // own distance.
            let h = Vec3::new(
                p.object_origin.x - cam.x,
                p.object_origin.y - cam.y,
                p.object_origin.z - cam.z,
            );
            let cell = far_cell(p.object_origin, p.outdoors, cam);
            let shared = cell.is_some() || h.x * h.x + h.y * h.y >= STARTUP_OBJECT_DISTANCE_2DSQ;
            assert_eq!(p.shared, shared, "part {} of {:?}", p.part, p.object);
            let want = if let Some((d, _)) = cell {
                d
            } else if shared {
                h.magnitude()
            } else {
                update_viewer_distance(&pd, want_centre, cam).0
            };
            assert!(
                (p.cypt - want).abs() <= 1e-3 * want.abs().max(1.0),
                "part {} of {:?}: the scene sorted on distance key {} and update_viewer_distance says \
                 {want}",
                p.part,
                p.object,
                p.cypt
            );
            checked += 1;
        }

        // The trace must carry the same key as the probe, or the order check could accept a
        // substituted key (for example the viewer distance divided by the part's Z scale, which
        // the degrade selection uses).
        let by_part: BTreeMap<(Option<ObjectId>, usize), f32> =
            probe.iter().map(|p| ((p.object, p.part), p.cypt)).collect();

        // Check each run separately: in-place subsets, then deferred clip and blend subsets.
        // Deferral moves subsets to later passes, so the entire concatenated trace need not
        // descend globally. Runs shorter than two are skipped; total compared pairs must
        // still exceed the explicit floor. Equal adjacent keys pass without a stability test.
        let trace = scene.drawn_part_order();
        assert!(!trace.is_empty(), "station {i}: the trace is empty");
        for (name, run) in [
            (
                "in place",
                trace
                    .iter()
                    .filter(|e| e.list.is_none())
                    .collect::<Vec<_>>(),
            ),
            (
                "clip list",
                trace
                    .iter()
                    .filter(|e| e.list == Some(AlphaList::Clip))
                    .collect::<Vec<_>>(),
            ),
            (
                "blend list",
                trace
                    .iter()
                    .filter(|e| e.list == Some(AlphaList::Blend))
                    .collect::<Vec<_>>(),
            ),
        ] {
            if run.len() < 2 {
                continue;
            }
            runs_checked += 1;
            for e in &run {
                let want = by_part
                    .get(&(e.object, e.part))
                    .copied()
                    .unwrap_or_else(|| {
                        panic!("part {} of {:?} is not in the probe", e.part, e.object)
                    });
                assert!(
                    (e.cypt - want).abs() <= 1e-4 * want.abs().max(1.0),
                    "station {i}, {name}: the trace says part {} of {:?} was sorted on distance key {} \
                     and the probe measured {want}",
                    e.part,
                    e.object,
                    e.cypt
                );
            }
            for w in run.windows(2) {
                assert!(
                    w[0].cypt >= w[1].cypt,
                    "station {i}, {name}: part {} of {:?} at distance key {} was submitted before part \
                     {} of {:?} at distance key {}, which is nearer-first",
                    w[0].part,
                    w[0].object,
                    w[0].cypt,
                    w[1].part,
                    w[1].object,
                    w[1].cypt
                );
                compared += 1;
            }
        }
        eprintln!(
            "  +{dz:5.0} m: {} parts probed, {} subsets in the trace, distance keys from {:.1} m to {:.1} m",
            probe.len(),
            trace.len(),
            trace.first().map_or(0.0, |e| e.cypt),
            trace.last().map_or(0.0, |e| e.cypt)
        );
    }

    eprintln!(
        "part order: {checked} distance keys checked against a viewer-distance update re-run on the graphics object \
         read back from the dat ({} distinct sort centres); {compared} adjacent pairs asserted \
         non-increasing over {runs_checked} device-order runs",
        centres.len()
    );
    assert!(checked > 400, "only {checked} parts over four stations");
    assert!(compared > 400, "only {compared} ordered pairs");
    scene.release_textures(&mut gpu);
}

/// Behaviour: rendering.degrade.an-objects-parts-share-its-distance-past-the-share-distance
/// With the camera just above one object, that object's parts are each drawn at their own
/// distance and every part of an object 5 m or more away horizontally is sorted at, and turned
/// toward, its object's origin. The keys read back are the draw's own, from the device-order
/// trace.
#[test]
fn past_the_share_distance_every_part_sorts_and_turns_at_its_objects_own_distance() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut r = populated("first-login-walk-jump");
    let mut scene = scene_for(&store, &mut gpu, &r, true);
    let _ = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);

    // An object with more than one part whose parts are measured at different distances, so
    // sharing is visible in its keys.
    let probe = scene.part_degrade_probe();
    let mut parts_of: BTreeMap<ObjectId, usize> = BTreeMap::new();
    for p in &probe {
        if let Some(id) = p.object {
            *parts_of.entry(id).or_default() += 1;
        }
    }
    let (&subject, _) = parts_of
        .iter()
        .find(|(_, n)| **n > 2)
        .expect("the recording holds an object of more than two parts");
    let origin = probe
        .iter()
        .find(|p| p.object == Some(subject))
        .map(|p| p.object_origin)
        .expect("the subject is in the probe");
    scene.camera.position = Vec3::new(origin.x, origin.y, origin.z + 3.0);
    frame(&store, &mut gpu, &mut scene, &mut r.objects, 10.0);
    let cam = scene.camera.position;
    let probe = scene.part_degrade_probe();
    let trace = scene.drawn_part_order();
    let drawn: BTreeMap<(Option<ObjectId>, usize), f32> =
        trace.iter().map(|e| ((e.object, e.part), e.cypt)).collect();

    let (mut own, mut shared, mut own_keys) = (0usize, 0usize, Vec::new());
    for p in &probe {
        let v = Vec3::new(
            p.object_origin.x - cam.x,
            p.object_origin.y - cam.y,
            p.object_origin.z - cam.z,
        );
        // An object in a land cell more than 50 m away takes the cell's distance and level
        // heading instead of its own.
        let cell = far_cell(p.object_origin, p.outdoors, cam);
        let past = cell.is_some() || v.x * v.x + v.y * v.y >= STARTUP_OBJECT_DISTANCE_2DSQ;
        assert_eq!(p.shared, past, "part {} of {:?}", p.part, p.object);
        let Some(&key) = drawn.get(&(p.object, p.part)) else {
            continue;
        };
        if past {
            let (d, heading) = cell.unwrap_or_else(|| {
                let d = v.magnitude();
                (d, Vec3::new(v.x / d, v.y / d, v.z / d))
            });
            assert!(
                (key - d).abs() <= 1e-3 * d.max(1.0),
                "part {} of {:?} sorted at {key}, and its object or cell is {d} away",
                p.part,
                p.object
            );
            assert!(
                (p.viewer_heading.x - heading.x).abs() < 1e-4
                    && (p.viewer_heading.y - heading.y).abs() < 1e-4
                    && (p.viewer_heading.z - heading.z).abs() < 1e-4,
                "part {} of {:?} turned toward {:?}, its object lies along {heading:?}",
                p.part,
                p.object,
                p.viewer_heading
            );
            shared += 1;
        } else {
            assert!(
                (key - p.cypt).abs() <= 1e-3 * p.cypt.max(1.0),
                "part {} of {:?}: drawn at {key}, measured at {}",
                p.part,
                p.object,
                p.cypt
            );
            if p.object == Some(subject) {
                own_keys.push(key);
            }
            own += 1;
        }
    }
    eprintln!("{own} parts measured themselves, {shared} took their object's distance");
    assert!(own >= 2, "only {own} parts inside the share distance");
    assert!(shared > 100, "only {shared} parts past the share distance");
    own_keys.sort_by(f32::total_cmp);
    assert!(
        own_keys.first() != own_keys.last(),
        "the subject's parts were all drawn at one distance: {own_keys:?}"
    );
    scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// 2. The control arm: without the sort the order is server-id order, and it really is different
// ---------------------------------------------------------------------------------------------

/// With SceneConfig::part_depth_sort disabled and alpha lists off, require exact server-ID then
/// part-index order. Count adjacent nearer-first inversions, require a positive control count and
/// zero in the sorted arm, and hold the part count equal. This measures ordering rather than pixel
/// impact.
#[test]
fn the_control_arm_is_server_id_order_and_the_sort_removes_every_inversion() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);

    // Vary only sorting, with alpha lists disabled in both arms so all subsets form one in-place
    // run; varying both switches would change the compared runs.
    let sorted_only = |store: &Arc<RetailDatStore>, gpu: &mut Gpu, r: &Replayed, on: bool| {
        let cfg = SceneConfig {
            landblock: r.landblock,
            character: false,
            land_radius: 1,
            scenery_radius: 0,
            part_degrade_levels: true,
            part_billboards: true,
            part_alpha_lists: false,
            part_depth_sort: on,
            // Keep the view-cone cull disabled, as in scene_for: these elevated, un-aimed stations
            // are about sorting, not visibility.
            object_viewcone: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
        scene.set_weather_enabled(false);
        scene
    };

    let mut arm = |on: bool| -> (Vec<(Option<ObjectId>, usize, f32)>, usize) {
        let mut r = populated("first-login-walk-jump");
        let mut scene = sorted_only(&store, &mut gpu, &r, on);
        let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);
        scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + 12.0);
        frame(&store, &mut gpu, &mut scene, &mut r.objects, 60.0);
        // One row per *part*, not per subset: the sort orders parts and every subset of a part
        // carries its part's distance key.
        let mut parts: Vec<(Option<ObjectId>, usize, f32)> = Vec::new();
        for e in scene.drawn_part_order().iter().filter(|e| e.list.is_none()) {
            if parts.last().map(|(o, p, _)| (*o, *p)) != Some((e.object, e.part)) {
                parts.push((e.object, e.part, e.cypt));
            }
        }
        let inversions = parts.windows(2).filter(|w| w[0].2 < w[1].2).count();
        scene.release_textures(&mut gpu);
        (parts, inversions)
    };

    let (off, inv_off) = arm(false);
    let (on, inv_on) = arm(true);

    assert_eq!(
        off.len(),
        on.len(),
        "the two arms drew different numbers of parts"
    );
    assert!(
        off.len() > 100,
        "only {} parts in the in-place run",
        off.len()
    );
    eprintln!(
        "part order: {} parts in the in-place run; adjacent pairs in nearer-first order: {inv_off} \
         without the sort, {inv_on} with it",
        off.len()
    );

    // Require server-ID then part-index order in the control arm.
    let mut by_id = off.clone();
    by_id.sort_by_key(|(o, p, _)| (o.map(|i| i.0), *p));
    assert_eq!(off, by_id, "the control arm is not in server-id order");

    assert_eq!(
        inv_on, 0,
        "the sorted arm still submits {inv_on} pairs nearer-first"
    );
    assert!(
        inv_off > 0,
        "the unsorted arm has no inversions, so this scene cannot tell the two arms apart"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The degrade visibility gate: an empty selected level draws nothing
// ---------------------------------------------------------------------------------------------

/// A part whose selected level's graphics-object id is zero draws nothing. Re-read the dat record
/// and require no submission-trace entry for such a selected part (not a fallback to level zero).
/// A positive empty-level population is required overall; the count of stations holding one is
/// reported.
#[test]
fn the_degrade_visibility_gate_is_already_wired() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut r = populated("first-login-walk-jump");
    let mut scene = scene_for(&store, &mut gpu, &r, true);
    let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);

    let mut records: BTreeMap<u32, GfxObjDegradeInfo> = BTreeMap::new();
    let (mut terminated, mut stations_with_a_terminator) = (0usize, 0usize);

    for (i, dz) in [8.0f32, 60.0, 250.0, 900.0].into_iter().enumerate() {
        scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
        // LINT-OK: a station index.
        #[allow(clippy::cast_precision_loss)]
        let t = 70.0 + i as f64;
        frame(&store, &mut gpu, &mut scene, &mut r.objects, t);

        let drawn: std::collections::BTreeSet<(Option<ObjectId>, usize)> = scene
            .drawn_part_order()
            .iter()
            .map(|e| (e.object, e.part))
            .collect();
        let mut here = 0usize;
        for p in scene.part_degrade_probe() {
            let Some(did) = p.record else { continue };
            let info = records.entry(did.0).or_insert_with(|| record(&store, did));
            let empty = info.degrades[p.level as usize].gfxobj_id.0 == 0;
            if empty {
                here += 1;
                assert!(
                    !drawn.contains(&(p.object, p.part)),
                    "part {} of {:?} selected level {}, whose gfxobj_id is 0, and was still \
                     submitted",
                    p.part,
                    p.object,
                    p.level
                );
            }
        }
        if here > 0 {
            stations_with_a_terminator += 1;
        }
        terminated += here;
        eprintln!("  +{dz:5.0} m: {here} parts on a level whose gfxobj_id is 0");
    }

    eprintln!(
        "part order: {terminated} part-selections over four stations landed on a terminator level, at \
         {stations_with_a_terminator} of 4 stations, and none of them reached the device"
    );

    // Report the hidden-state population separately from selected-level visibility; hiding an
    // object propagates no-draw to its direct children, which is covered elsewhere, and this count
    // tests neither that nor visibility.
    const HIDDEN_PS: u32 = 0x0000_4000;
    let (hidden, total) = r
        .objects
        .presences()
        .fold((0usize, 0usize), |(h, t), (id, _)| {
            (
                h + usize::from(r.objects.physics_state(id).unwrap_or(0) & HIDDEN_PS != 0),
                t + 1,
            )
        });
    eprintln!(
        "state census: {hidden} of {total} objects in this capture carry HIDDEN_PS; child \
         visibility propagation is covered separately"
    );
    assert!(
        terminated > 0,
        "no part ever selects a level with gfxobj_id 0 in this scene, so the guard is \
         unexercised here and this assertion proves nothing about it"
    );
    scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// 4. The pixels
// ---------------------------------------------------------------------------------------------

/// Compare combined SceneConfig::part_depth_sort and part_alpha_lists off/on. This does not
/// isolate sorting: scene_for changes both. First require equal sorted object-ID, sequence-frame
/// and forward-command tuples between arms; these controls do not assert all transforms.
/// Calibrate different disabled stations as positive and independent repeated enabled draws
/// as exactly equal before reporting the cross-arm pixel differences.
///
/// Reordering ordinary depth-tested opaque geometry need not change pixels. No positive or
/// upper-bound cross-arm difference is asserted here, so the report alone cannot prove a
/// visible change. The order tests above carry that claim.
#[test]
fn sorting_the_parts_leaves_the_scene_recognisable_and_the_control_arm_is_the_old_build() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    const STATIONS: [f32; 4] = [8.0, 15.0, 30.0, 60.0];

    type Station = (
        (Vec<u8>, u32, u32),
        Vec<(ObjectId, i32, dereth_animation::MotionCommand)>,
    );

    let mut arm = |on: bool| -> Vec<Station> {
        let mut r = populated("first-login-walk-jump");
        let mut scene = scene_for(&store, &mut gpu, &r, on);
        let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);
        let mut out: Vec<Station> = Vec::with_capacity(STATIONS.len());
        for (i, dz) in STATIONS.iter().enumerate() {
            scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
            // LINT-OK: a station index.
            #[allow(clippy::cast_precision_loss)]
            let t = 20.0 + 2.0 * i as f64;
            let _ = shot(&store, &mut gpu, &mut scene, &mut r.objects, t);
            let img = shot(&store, &mut gpu, &mut scene, &mut r.objects, t + 1.0);
            let mut pose: Vec<(ObjectId, i32, dereth_animation::MotionCommand)> = r
                .objects
                .presences()
                .filter_map(|(id, _)| scene.server_object_motion(id).map(|(f, c)| (id, f, c)))
                .collect();
            pose.sort_by_key(|(id, _, _)| id.0);
            out.push((img, pose));
        }
        scene.release_textures(&mut gpu);
        out
    };

    let off = arm(false);
    let on = arm(true);
    let again = arm(true);

    for (k, (a, b)) in off.iter().zip(on.iter()).enumerate() {
        assert!(
            !a.1.is_empty(),
            "station {k}: no object published an animation frame"
        );
        assert_eq!(
            a.1, b.1,
            "station {k}: the two arms are at different points of their animations, so a pixel \
             differential between them would be about pose and not about the sort"
        );
    }
    eprintln!(
        "pose gate: {} objects, identical in both arms at all 4 stations",
        off[0].1.len()
    );

    let total = (on[0].0 .1 * on[0].0 .2) as usize;
    let positive = diff(&off[0].0, &off[STATIONS.len() - 1].0);
    assert!(
        positive > 0,
        "the differ read 0 on a pair that is known to differ"
    );
    let mut negative = 0usize;
    for (k, (a, b)) in on.iter().zip(again.iter()).enumerate() {
        let d = diff(&a.0, &b.0);
        assert_eq!(d, 0, "two identical runs differ by {d} px at station {k}");
        negative += d;
    }
    eprintln!(
        "differ calibration: {positive} of {total} px on a known-different pair, {negative} px \
         over {} known-identical pairs (a separate replay, build, draw and capture each)",
        STATIONS.len()
    );

    for (k, dz) in STATIONS.iter().enumerate() {
        let changed = diff(&off[k].0, &on[k].0);
        eprintln!("  +{dz:5.0} m above the objects: {changed} of {total} px changed");
    }
}

// ---------------------------------------------------------------------------------------------
// Read the fixed capture-line fields directly without an application JSON-parser dependency.
// ---------------------------------------------------------------------------------------------

/// A scene over the capture's own landblock, with no local body: the objects the *server* owns are
/// the subject and a chase camera would move on its own.
fn scene_for_alpha(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    r: &Replayed,
    on: bool,
) -> WorldScene {
    let cfg = SceneConfig {
        landblock: r.landblock,
        character: false,
        land_radius: 1,
        // No scenery: the statics already defer their blending batches through `BakedObjects`,
        // and holding their levels too would put a second moving part in a measurement about
        // creatures.
        scenery_radius: 0,
        // Part degrade levels and billboards stay **on** in both arms: the level a part picks and
        // the frame it is drawn at are theirs, and an arm that varied either would be measuring
        // three mechanisms at once. The only thing that moves is
        // whether a non-opaque subset is deferred to `AlphaLists` or drawn in place.
        part_degrade_levels: true,
        part_billboards: true,
        // The part depth sort is pinned **on** in both arms for the same reason: it changes the
        // order subsets enter the list, which is the one thing the list preserves.
        part_depth_sort: true,
        part_alpha_lists: on,
        // Position-only stations above the objects do not aim yaw/pitch at the population, so the
        // view-cone rejection would remove part of it. Disable rejection explicitly to isolate
        // alpha-list submission (SceneConfig's default is on); the evaluated cone is available
        // through WorldScene::drawn_object_cone but is not asserted in this file. Re-aiming these
        // stations so the cull can stay on is a separate change.
        object_viewcone: false,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
    scene.set_weather_enabled(false);
    scene
}

// ---------------------------------------------------------------------------------------------
// 1. Recomputed masks/classification and device-order traces constrain subset submission.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.object-parts.transparent-subsets-are-deferred-clip-before-blend
/// Re-read DAT surfaces and invoke the shared decoder, mask and classification helpers outside
/// the scene. This checks wiring and trace agreement, not independent decoding or mask arithmetic.
#[test]
fn every_subset_lands_where_add_mesh_to_alpha_list_would_put_it() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut r = populated("first-login-walk-jump");
    let mut scene = scene_for_alpha(&store, &mut gpu, &r, true);
    let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);

    let mut surfaces: BTreeMap<u32, u32> = BTreeMap::new();
    let mut checked = 0usize;
    let mut totals = (0usize, 0usize, 0usize, 0usize); // clip, blend, immediate, no-surface
                                                       // Stations at which the clip run holds an entry *nearer* than one on the blend run, i.e. at
                                                       // which "clip first" is not what a depth sort would have produced anyway.
    let mut depth_inverted = 0usize;
    // (part, list) pairs carrying two or more deferred subsets -- see below.
    let mut first_of_kind_discriminating = 0usize;

    for (i, dz) in [8.0f32, 25.0, 80.0, 250.0].into_iter().enumerate() {
        scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
        // LINT-OK: a station index.
        #[allow(clippy::cast_precision_loss)]
        let t = 30.0 + i as f64;
        frame(&store, &mut gpu, &mut scene, &mut r.objects, t);

        let stats = scene.drawn_alpha_lists();
        let trace = scene.drawn_part_order();
        assert!(
            stats.parts > 0,
            "station {i}: the scene submitted no part at all"
        );
        assert!(
            !trace.is_empty(),
            "station {i}: the trace is empty on a populated scene"
        );
        assert_eq!(
            stats.clip + stats.blend,
            stats.flushed,
            "station {i}: {} entries were queued and {} were drawn",
            stats.clip + stats.blend,
            stats.flushed
        );
        assert_eq!(
            stats.dropped, 0,
            "station {i}: a list hit the 3 000-entry cap"
        );
        assert_eq!(
            stats.clip + stats.blend + stats.immediate,
            trace.len(),
            "station {i}: the counters and the trace disagree about how many subsets were drawn"
        );

        // Every entry, against the dat.
        let mut first_seen: BTreeMap<(Option<ObjectId>, usize, bool), usize> = BTreeMap::new();
        for (n, e) in trace.iter().enumerate() {
            let ty = match e.surface {
                Some(did) => *surfaces.entry(did.0).or_insert_with(|| {
                    let bytes = store.read_typed(DbType::Surface, did).unwrap_or_else(|err| {
                        panic!("{did:?}: the surface the scene is holding is not in the dat: {err}")
                    });
                    dereth_assets::Surface::decode_payload(did, &bytes)
                        .expect("it decodes")
                        .surface_type
                }),
                None => {
                    totals.3 += 1;
                    0
                }
            };
            let want_mask = subset_mask(ty);
            assert_eq!(
                e.mask, want_mask,
                "station {i} entry {n}: surface {:?} has type {ty:#x}, whose mask is {want_mask}, \
                 and the scene recorded {}",
                e.surface, e.mask
            );
            let want_list = classify_subset(want_mask, S_ALPHA_DELAY_MASK, false);
            assert_eq!(
                e.list, want_list,
                "station {i} entry {n}: mask {want_mask} at alpha-delay mask 0x0E belongs on \
                 {want_list:?} and the scene put it on {:?}",
                e.list
            );
            match e.list {
                Some(AlphaList::Clip) => totals.0 += 1,
                Some(AlphaList::Blend) => totals.1 += 1,
                None => totals.2 += 1,
            }
            // first_of_kind marks only the first deferred subset for this part on this list.
            if let Some(list) = e.list {
                let key = (e.object, e.part, matches!(list, AlphaList::Clip));
                let seen = first_seen.entry(key).or_insert(0);
                assert_eq!(
                    e.first_of_kind,
                    *seen == 0,
                    "station {i} entry {n}: first_of_kind is {} for subset {} of part {} on \
                     {list:?}, which is occurrence {} of that (part, list)",
                    e.first_of_kind,
                    e.subset,
                    e.part,
                    *seen + 1
                );
                *seen += 1;
                if *seen == 2 {
                    // Count each (part, list) group once when its second subset appears.
                    // Only such a repeated group can reject a flag forced true everywhere, and
                    // this corpus may have none. Report the coverage limit without adding a
                    // positive denominator assertion.
                    first_of_kind_discriminating += 1;
                }
            } else {
                assert!(
                    !e.first_of_kind,
                    "an immediately-drawn subset carries first_of_kind"
                );
            }
            checked += 1;
        }
        // Trace order exposes both flush rules: every deferred subset follows all immediate
        // subsets, and the entire clip run precedes the blend run regardless of distance.
        let deferred_from = trace
            .iter()
            .position(|e| e.list.is_some())
            .expect("something queued");
        assert!(
            trace[deferred_from..].iter().all(|e| e.list.is_some()),
            "station {i}: an in-place subset was submitted after a deferred one"
        );
        let blend_from = trace[deferred_from..]
            .iter()
            .position(|e| e.list == Some(AlphaList::Blend))
            .map_or(trace.len(), |k| deferred_from + k);
        assert!(
            trace[deferred_from..blend_from]
                .iter()
                .all(|e| e.list == Some(AlphaList::Clip)),
            "station {i}: the clip list did not drain as one run"
        );
        assert!(
            trace[blend_from..]
                .iter()
                .all(|e| e.list == Some(AlphaList::Blend)),
            "station {i}: a clip-list entry was drawn after a blend-list one"
        );
        // Count stations where the nearest clip entry is nearer than the farthest blend entry.
        // There, clip-first differs from global descending distance. This is reported only:
        // neither every station nor even one station is required to have that configuration.
        if let (Some(clip_min), Some(blend_max)) = (
            trace[deferred_from..blend_from]
                .iter()
                .map(|e| e.cypt)
                .reduce(f32::min),
            trace[blend_from..].iter().map(|e| e.cypt).reduce(f32::max),
        ) {
            if clip_min < blend_max {
                depth_inverted += 1;
            }
        }
        eprintln!(
            "  +{dz:5.0} m: {} parts, {} subsets -- clip {}, blend {}, in place {}",
            stats.parts,
            trace.len(),
            stats.clip,
            stats.blend,
            stats.immediate
        );
    }

    eprintln!(
        "alpha lists: {checked} subset classifications checked against the dat over four stations; \
         clip {}, blend {}, drawn in place {}; {} subsets belong to a group with no surface record; \
         {} distinct surfaces read back from client_portal.dat",
        totals.0,
        totals.1,
        totals.2,
        totals.3,
        surfaces.len()
    );
    assert!(
        checked > 100,
        "only {checked} subsets over the whole capture"
    );
    assert!(
        totals.0 > 0,
        "no subset reached the CLIP list: this scene cannot exercise the clip list"
    );
    assert!(
        totals.1 > 0,
        "no subset reached the BLEND list: this scene cannot exercise the blend list"
    );
    eprintln!(
        "alpha lists: {first_of_kind_discriminating} (part, list) groups among {checked} subsets had a \
         second deferred subset. Those groups can reject a flag forced true on every entry. \
         Zero limits that mutation control in this corpus; alpha.rs tests the flag separately."
    );
    eprintln!(
        "alpha lists: the clip list drained before the blend list at all 4 stations, and at \
         {depth_inverted} of them it held an entry nearer the camera than the blend list's \
         farthest -- i.e. at {depth_inverted} of 4 the rule is not what a depth sort would give"
    );
    scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// 2. The control arm: with the switch clear, nothing is queued and everything draws in place
// ---------------------------------------------------------------------------------------------

/// Vary only SceneConfig::part_alpha_lists while retaining depth sorting, degradation and
/// billboarding. Require equal sorted object-ID/sequence-frame/forward-command tuples before
/// pixel comparison; these are animation controls, not complete transform equality.
///
/// Calibrate different disabled stations as positive and independent repeated enabled draws
/// as exactly equal. The cross-arm pixel difference is reported without a positive or upper
/// bound, so it does not itself prove a visible change. The trace test carries classification
/// and order claims. Here the last station's counters require no deferred control entries,
/// positive clip/blend populations when enabled, equal parts and equal total submitted subsets.
#[test]
fn deferring_the_transparent_subsets_is_visible_and_the_control_arm_queues_nothing() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    const STATIONS: [f32; 4] = [8.0, 15.0, 30.0, 60.0];

    type Station = (
        (Vec<u8>, u32, u32),
        Vec<(ObjectId, i32, dereth_animation::MotionCommand)>,
    );

    let mut arm = |on: bool| -> (Vec<Station>, dereth_scene::world_scene::AlphaListStats) {
        let mut r = populated("first-login-walk-jump");
        let mut scene = scene_for_alpha(&store, &mut gpu, &r, on);
        let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);
        let mut out: Vec<Station> = Vec::with_capacity(STATIONS.len());
        let mut last = dereth_scene::world_scene::AlphaListStats::default();
        for (i, dz) in STATIONS.iter().enumerate() {
            scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
            // LINT-OK: a station index.
            #[allow(clippy::cast_precision_loss)]
            let t = 20.0 + 2.0 * i as f64;
            let _ = shot(&store, &mut gpu, &mut scene, &mut r.objects, t);
            let img = shot(&store, &mut gpu, &mut scene, &mut r.objects, t + 1.0);
            last = scene.drawn_alpha_lists();
            let mut pose: Vec<(ObjectId, i32, dereth_animation::MotionCommand)> = r
                .objects
                .presences()
                .filter_map(|(id, _)| scene.server_object_motion(id).map(|(f, c)| (id, f, c)))
                .collect();
            pose.sort_by_key(|(id, _, _)| id.0);
            out.push((img, pose));
        }
        scene.release_textures(&mut gpu);
        (out, last)
    };

    let (off, stats_off) = arm(false);
    let (on, stats_on) = arm(true);
    let (again, _) = arm(true);

    for (k, (a, b)) in off.iter().zip(on.iter()).enumerate() {
        assert!(
            !a.1.is_empty(),
            "station {k}: no object published an animation frame"
        );
        assert_eq!(
            a.1, b.1,
            "station {k}: the two arms are at different points of their animations, so a pixel \
             differential between them would be about pose and not about the alpha lists"
        );
    }
    eprintln!(
        "pose gate: {} objects, identical in both arms at all {} stations",
        off[0].1.len(),
        STATIONS.len()
    );

    eprintln!(
        "control arm: parts {} clip {} blend {} in place {}; live arm: parts {} clip {} blend {} \
         in place {}",
        stats_off.parts,
        stats_off.clip,
        stats_off.blend,
        stats_off.immediate,
        stats_on.parts,
        stats_on.clip,
        stats_on.blend,
        stats_on.immediate
    );
    assert_eq!(
        stats_off.parts, stats_on.parts,
        "the two arms drew different numbers of parts"
    );
    assert_eq!(
        (stats_off.clip, stats_off.blend),
        (0, 0),
        "the control arm queued something"
    );
    assert!(
        stats_on.clip > 0 && stats_on.blend > 0,
        "the live arm queued nothing on a list"
    );
    assert_eq!(
        stats_off.immediate,
        stats_on.clip + stats_on.blend + stats_on.immediate,
        "the two arms put different numbers of subsets on the device"
    );

    let total = (on[0].0 .1 * on[0].0 .2) as usize;
    let positive = diff(&off[0].0, &off[STATIONS.len() - 1].0);
    assert!(
        positive > 0,
        "the differ read 0 on a pair that is known to differ"
    );
    let mut negative = 0usize;
    for (k, (a, b)) in on.iter().zip(again.iter()).enumerate() {
        let d = diff(&a.0, &b.0);
        assert_eq!(d, 0, "two identical runs differ by {d} px at station {k}");
        negative += d;
    }
    eprintln!(
        "differ calibration: {positive} of {total} px on a known-different pair, {negative} px \
         over {} known-identical pairs (a separate replay, build, draw and capture each)",
        STATIONS.len()
    );

    let mut best = 0usize;
    for (k, dz) in STATIONS.iter().enumerate() {
        let changed = diff(&off[k].0, &on[k].0);
        eprintln!("  +{dz:5.0} m above the objects: {changed} of {total} px changed");
        best = best.max(changed);
    }
    eprintln!("alpha lists differential: {best} of {total} px at the widest station");
}

/// Behaviour: rendering.preferences.multiple-pass-alpha-s-soft-edges-go-out-before-translucent-objects
/// **Rejecting.** With "Multiple Pass Alpha" on, the soft second pass of every cut-out (the
/// scenery's trees and the creatures' own) belongs to the clip list, and every flush drains the
/// whole clip list before any of the alpha list. So within one flush no second pass follows a
/// blended draw: a translucent object standing in front of a tree is blended over the tree's soft
/// edge, and never has that edge painted over it.
#[test]
fn with_multiple_pass_alpha_no_soft_edge_is_drawn_after_a_blended_object_of_its_flush() {
    use dereth_scene::world_scene::AlphaDraw;
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut r = populated("first-login-walk-jump");
    let cfg = SceneConfig {
        landblock: r.landblock,
        character: false,
        land_radius: 1,
        // The trees: the scenery is what carries the landscape's clip-mapped batches.
        scenery_radius: 1,
        part_degrade_levels: true,
        part_billboards: true,
        part_depth_sort: true,
        part_alpha_lists: true,
        object_viewcone: false,
        render: dereth_client_runtime::render_prefs::RenderPreferences {
            multi_pass_alpha: true,
            ..dereth_client_runtime::render_prefs::RenderPreferences::default()
        },
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    scene.set_weather_enabled(false);
    let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);

    let mut both = 0usize;
    for (i, dz) in [8.0f32, 25.0, 80.0].into_iter().enumerate() {
        scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
        // LINT-OK: a station index.
        #[allow(clippy::cast_precision_loss)]
        let t = 40.0 + i as f64;
        frame(&store, &mut gpu, &mut scene, &mut r.objects, t);
        let order = scene.drawn_alpha_order();
        let count = |k: AlphaDraw| order.iter().filter(|d| **d == k).count();
        eprintln!(
            "  +{dz:3.0} m: {} flush(es); clip: {} part, {} part second, {} static second, \
             {} particle second; blend: {} part, {} static",
            count(AlphaDraw::FlushStart),
            count(AlphaDraw::PartClip),
            count(AlphaDraw::PartForced),
            count(AlphaDraw::StaticForced),
            count(AlphaDraw::ParticleForced),
            count(AlphaDraw::PartBlend),
            count(AlphaDraw::StaticBlend),
        );
        for (f, flush) in order.split(|d| *d == AlphaDraw::FlushStart).enumerate() {
            if let Some(b) = flush.iter().position(|d| !d.is_clip_list()) {
                if let Some(late) = flush[b..].iter().find(|d| d.is_clip_list()) {
                    panic!(
                        "station {i}, flush {f}: {late:?} was drawn after {:?}, so a blended \
                         draw lies under a soft edge it stands in front of",
                        flush[b]
                    );
                }
            }
            let forced = flush
                .iter()
                .any(|d| matches!(d, AlphaDraw::StaticForced | AlphaDraw::PartForced));
            if forced && flush.contains(&AlphaDraw::PartBlend) {
                both += 1;
            }
        }
    }
    assert!(
        both > 0,
        "no flush held both a soft second pass and a blended part, so the order was never tested"
    );
}

/// Behaviour: rendering.object-parts.a-translucent-static-takes-its-place-among-the-translucent-parts
/// **Rejecting.** A static's blended subsets go on the alpha list where its cell draws it, among
/// every creature's and particle's, and the list is flushed in that order, far to near. So the
/// landscape's translucent scenery behind a nearer translucent object is drawn first and blended
/// under it; drawn after the object pass, a far blended bush would paint over the nearer object,
/// which writes no depth to stop it.
#[test]
fn a_far_translucent_static_draws_before_a_nearer_translucent_object() {
    use dereth_scene::world_scene::AlphaDraw;
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    for multi_pass_alpha in [false, true] {
        let mut r = populated("first-login-walk-jump");
        let cfg = SceneConfig {
            landblock: r.landblock,
            character: false,
            land_radius: 1,
            // The blended scenery: the town's bushes stand in the next landblock south.
            scenery_radius: 1,
            particles: true,
            part_degrade_levels: true,
            part_billboards: true,
            part_depth_sort: true,
            part_alpha_lists: true,
            object_viewcone: false,
            render: dereth_client_runtime::render_prefs::RenderPreferences {
                multi_pass_alpha,
                ..dereth_client_runtime::render_prefs::RenderPreferences::default()
            },
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
        scene.set_weather_enabled(false);
        let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);
        let mut crossings = 0usize;
        for (i, dz) in [8.0f32, 25.0, 80.0].into_iter().enumerate() {
            scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
            for f in 0..10 {
                // LINT-OK: a station and frame index.
                #[allow(clippy::cast_precision_loss)]
                let t = 60.0 + i as f64 + f64::from(f) * 0.1;
                frame(&store, &mut gpu, &mut scene, &mut r.objects, t);
            }
            let blend = scene.drawn_blend_order();
            let statics = blend
                .iter()
                .filter(|(k, _)| *k == AlphaDraw::StaticBlend)
                .count();
            eprintln!(
                "  multipass {multi_pass_alpha}, +{dz:3.0} m: {statics} blended static(s) among {} \
                 blended draw(s); landscape blend count {}",
                blend.len(),
                scene.drawn_landscape_alpha().blend
            );
            for w in blend.windows(2) {
                assert!(
                    w[0].1 >= w[1].1,
                    "station {i}: {:?} at {:.2} m was drawn before {:?} at {:.2} m",
                    w[0].0,
                    w[0].1,
                    w[1].0,
                    w[1].1
                );
            }
            // Blended statics farther than the nearest moving blended draw of the frame: the case
            // drawing the statics after the object pass gets wrong.
            let nearest_moving = blend
                .iter()
                .filter(|(k, _)| *k != AlphaDraw::StaticBlend)
                .map(|(_, d)| *d)
                .reduce(f32::min);
            if let Some(near) = nearest_moving {
                crossings += blend
                    .iter()
                    .filter(|(k, d)| *k == AlphaDraw::StaticBlend && *d > near)
                    .count();
            }
        }
        assert!(
            crossings > 0,
            "multipass {multi_pass_alpha}: no blended static lay beyond a blended part or \
             particle, so the order was never tested"
        );
    }
}

/// Behaviour: rendering.particles.an-emitter-s-particles-take-their-place-among-the-parts
/// **Rejecting.** An emitter's particles are parts of their object: they are sorted by viewer
/// distance with every other part and go out through the same alpha list, so a far chimney's
/// blended smoke is drawn before a nearer blended object and never over it. Drawn after the whole
/// object pass, a far particle would follow every nearer blended part, which writes no depth to
/// stop it.
#[test]
fn a_far_emitter_s_blended_particles_draw_before_a_nearer_blended_object() {
    use dereth_scene::world_scene::AlphaDraw;
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    for multi_pass_alpha in [false, true] {
        // A fresh replay per scene: the object stream hands its objects to the scene that
        // syncs it.
        let mut r = populated("first-login-walk-jump");
        let cfg = SceneConfig {
            landblock: r.landblock,
            character: false,
            land_radius: 1,
            // The scripted statics around the town are the chimneys and their smoke.
            scenery_radius: 1,
            particles: true,
            part_degrade_levels: true,
            part_billboards: true,
            part_depth_sort: true,
            part_alpha_lists: true,
            object_viewcone: false,
            render: dereth_client_runtime::render_prefs::RenderPreferences {
                multi_pass_alpha,
                ..dereth_client_runtime::render_prefs::RenderPreferences::default()
            },
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
        scene.set_weather_enabled(false);
        let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);
        let mut crossings = 0usize;
        for (i, dz) in [8.0f32, 25.0, 80.0].into_iter().enumerate() {
            scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
            // Several frames, so the emitters have particles alive.
            for f in 0..20 {
                // LINT-OK: a station and frame index.
                #[allow(clippy::cast_precision_loss)]
                let t = 50.0 + i as f64 + f64::from(f) * 0.1;
                frame(&store, &mut gpu, &mut scene, &mut r.objects, t);
            }
            let blend = scene.drawn_blend_order();
            let particles = blend
                .iter()
                .filter(|(k, _)| *k == AlphaDraw::ParticleBlend)
                .count();
            let parts = blend.len() - particles;
            eprintln!(
                "  multipass {multi_pass_alpha}, +{dz:3.0} m: {parts} blended part draw(s), \
                 {particles} blended particle draw(s)"
            );
            // Far to near, parts and particles together, in every frame's blend list.
            for w in blend.windows(2) {
                assert!(
                    w[0].1 >= w[1].1,
                    "station {i}: {:?} at {:.2} m was drawn before {:?} at {:.2} m",
                    w[0].0,
                    w[0].1,
                    w[1].0,
                    w[1].1
                );
            }
            // How many blended particles lie farther than some blended part of the same frame:
            // the case drawing particles last gets wrong.
            let nearest_part = blend
                .iter()
                .filter(|(k, _)| *k == AlphaDraw::PartBlend)
                .map(|(_, d)| *d)
                .reduce(f32::min);
            if let Some(near) = nearest_part {
                crossings += blend
                    .iter()
                    .filter(|(k, d)| *k == AlphaDraw::ParticleBlend && *d > near)
                    .count();
            }
        }
        assert!(
            crossings > 0,
            "multipass {multi_pass_alpha}: no blended particle lay beyond a blended part, so the \
             order was never tested"
        );
    }
}
