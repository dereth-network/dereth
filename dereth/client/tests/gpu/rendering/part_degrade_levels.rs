//! Creatures, server objects and their parts draw the detail level their degrade record selects
//! for the current camera distance, re-selected every frame; the local player's body stays at
//! level zero even when its record would select a farther level.
//!
//! The checks: each drawn part's level is re-derived from its dat record with `get_degrade` at
//! the distance the frame used; the body's exemption is shown beside a body record that would
//! degrade; a control arm that bakes the near-band level once (switch off) is walked against the
//! per-frame arm over twelve heights and in pixels; a multilevel part uses its record's level-zero
//! mesh; and the degrade constants and raw record layout are pinned as literals. The distance
//! clamp subtracts: d = max(|dist| - 50, 0).
//! Fixture: the retail dats and the server-created population of a recorded session replayed
//! through the object stack, on a software device; missing fixtures fail, nothing skips.

#![cfg(gpu)]

use std::collections::BTreeMap;
use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

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
use dereth_world_render::objects::degrade::get_degrade;
use {
    dereth_client_runtime::landblock::load_region, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};

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

/// Decode the record named by a graphics object. Missing/undecodable objects or absent record
/// IDs return None; a named record is required to read and decode successfully.
fn record_of(store: &RetailDatStore, gfxobj: DataId) -> Option<GfxObjDegradeInfo> {
    let bytes = store.read_typed(DbType::GfxObj, gfxobj).ok()?;
    let obj = GfxObj::decode_payload(gfxobj, &bytes).ok()?;
    Some(record(store, obj.did_degrade?))
}

// ---------------------------------------------------------------------------------------------
// Read the fixed capture-line fields directly without adding a JSON parser to the app crate.
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
    /// The first player-position landblock observed during this replay prefix.
    landblock: u16,
}

/// Replay through the last datagram whose resulting object model is nonempty.
/// A logout can empty the model, so replaying every remaining datagram can erase the population
/// under test. This selects the last nonempty prefix, not the largest population, and does not
/// assume every recording ends with a clean logout.
fn populated(session: &str) -> Replayed {
    let records = load(session);
    let seq = connection_sequence_number(&records);
    let run = |limit: usize| -> (ObjectStream, usize, Option<u16>) {
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", seq).expect("host");
        let mut objects = ObjectStream::with_store(store());
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
/// it. The static half has the same trap.
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
/// Objects can straddle a landblock boundary (a centroid at y = 253 m in a 192 m block). Moving
/// there recentres streaming and rederives object frames against a new origin, so a centroid
/// measured before the move would make the first station jump 100 m. Move, step and remeasure,
/// allowing six attempts and requiring movement below 0.5 m before later stations use that
/// render-space centroid.
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
/// the subject here and a chase camera would move on its own.
fn scene_for(store: &Arc<RetailDatStore>, gpu: &mut Gpu, r: &Replayed, on: bool) -> WorldScene {
    let cfg = SceneConfig {
        landblock: r.landblock,
        character: false,
        land_radius: 1,
        // No scenery: static levels are measured elsewhere, and holding theirs too would put a
        // second moving part in a measurement about creatures.
        scenery_radius: 0,
        part_degrade_levels: on,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
    scene.set_weather_enabled(false);
    scene
}

// ---------------------------------------------------------------------------------------------
// 1. The acceptance: every part draws the level `get_degrade` names, at three distances.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.degrade.a-creatures-parts-draw-the-level-their-record-selects
/// Re-read each selected record and run get_degrade outside the scene at its published input
/// distance. WorldScene::part_degrade_probe exposes the distance used by the just-drawn frame.
/// This shared-helper comparison detects assembly disagreement with the record; the independent
/// byte/layout expectations below guard the common arithmetic and decoder inputs.
///
/// Three camera heights are visited. Each frame advances scene time, so this is not a claim that
/// nothing animates between stations. The probe is read immediately after the frame that used
/// it, keeping each comparison tied to that frame's inputs and selected level.
#[test]
fn every_part_draws_the_level_get_degrade_names_at_three_distances() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut r = populated("first-login-walk-jump");
    let mut scene = scene_for(&store, &mut gpu, &r, true);

    let mut records: BTreeMap<u32, GfxObjDegradeInfo> = BTreeMap::new();
    let mut checked = 0usize;
    let mut with_record = 0usize;
    let mut histograms = Vec::new();
    // Count selected billboard modes on the part side. Their draw frames are checked by the
    // billboard tests; this count is reported, not required to be positive here.
    let mut billboarded = 0usize;

    let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);
    eprintln!("centroid of the drawn objects, in the settled render space: {centre:?}");
    for (i, height) in [10.0f32, 120.0, 400.0].into_iter().enumerate() {
        scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + height);
        frame(&store, &mut gpu, &mut scene, &mut r.objects, 1.0 + i as f64);
        let g = scene.degrade_globals();
        let probe = scene.part_degrade_probe();
        assert!(
            probe.len() > 100,
            "only {} parts over the capture's scene",
            probe.len()
        );

        let mut hist: BTreeMap<u32, usize> = BTreeMap::new();
        for p in &probe {
            // The scene must at least agree with itself: the level it drew is the level its own
            // selection chooses at this camera.
            assert_eq!(
                p.level, p.would_choose,
                "part {} of {:?} drew level {} and would now choose {}",
                p.part, p.object, p.level, p.would_choose
            );
            match p.record {
                None => {
                    assert_eq!(p.level, 0, "a part with no record drew level {}", p.level);
                    assert_eq!(
                        p.levels_held, 1,
                        "a part with no record holds more than one level"
                    );
                }
                Some(did) => {
                    if i == 0 {
                        with_record += 1;
                    }
                    let info = records.entry(did.0).or_insert_with(|| record(&store, did));
                    // LINT-OK: a level index; the longest shipped record has six.
                    #[allow(clippy::cast_possible_truncation)]
                    let want = if p.is_player {
                        0
                    } else {
                        get_degrade(info, p.distance, &g).0 as u32
                    };
                    assert_eq!(
                        p.level, want,
                        "{did:?} at d = {} (player: {}): the scene drew level {}, get_degrade \
                         says {want}",
                        p.distance, p.is_player, p.level
                    );
                    assert_eq!(
                        p.levels_held,
                        info.degrades.len(),
                        "{did:?} has {} levels and the bake holds {}",
                        info.degrades.len(),
                        p.levels_held
                    );
                    if info
                        .degrades
                        .get(p.level as usize)
                        .is_some_and(|e| e.degrade_mode != 1)
                    {
                        billboarded += 1;
                    }
                }
            }
            *hist.entry(p.level).or_default() += 1;
            checked += 1;
        }
        eprintln!(
            "z = {height:6.1} m: {} parts, levels {hist:?}, {} object triangles drawn of {} held",
            probe.len(),
            scene.draw.stats.server_object_triangles,
            scene.draw.stats.server_object_triangles_resident
        );
        histograms.push(hist);
    }

    eprintln!(
        "{checked} part-level comparisons against get_degrade, none disagreeing; {with_record} of \
         the scene's parts carry a multi-level record"
    );
    eprintln!(
        "{billboarded} of those {checked} selected levels ask for a billboarding mode other than \
         1 -- the part-side denominator whose frame application the billboard tests check"
    );
    assert!(
        with_record > 0,
        "not one part in the scene carries a record: the test is vacuous"
    );
    assert!(
        checked > 300,
        "only {checked} comparisons; the denominator is too small to mean much"
    );
    // The comparison would be vacuous if every part sat on level 0 for ever.
    assert!(
        histograms[0] != histograms[2],
        "the level histogram did not move between 10 m and 400 m: {histograms:?}"
    );
    assert!(
        histograms[2].keys().any(|k| *k > 0),
        "nothing left level 0 even at 400 m: {:?}",
        histograms[2]
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The exemption, with a discriminator beside it.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.degrade.the-local-body-never-degrades
/// Preserve the original exemption: a degradation record is consulted only when its owner is
/// not the local player; otherwise the selected level stays zero.
///
/// Level zero alone would also pass if nothing degraded. Re-run get_degrade for each recorded
/// body part at the scene's measured distance and require a nonzero would-degrade population
/// at the far station. Also require another object to degrade and a body distance above 500 m.
/// The body still holds all its record levels; the exemption controls selection, not baking.
///
/// WorldScene::set_camera_distance drives the debug chase camera directly; the application's
/// camera update and its zoom commands are not involved. The exemption depends on the camera
/// position actually used, not on which camera-control route supplied it.
#[test]
fn the_local_body_does_not_degrade_however_far_the_chase_camera_pulls_back() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut r = populated("first-login-walk-jump");
    let cfg = SceneConfig {
        landblock: r.landblock,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let region = load_region(&store).expect("the region decodes");
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    scene.set_weather_enabled(false);
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    assert!(
        (scene.camera_distance() - 4.5).abs() < f32::EPSILON,
        "the chase camera's default distance moved; this test's stations assume 4.5 m"
    );

    let mut records: BTreeMap<u32, GfxObjDegradeInfo> = BTreeMap::new();
    let mut body_with_record = 0usize;
    let mut would_have_degraded_far = 0usize;
    let mut worst_level_the_record_names = 0u32;
    let mut body_max_distance = 0.0f32;
    let mut object_degraded_somewhere = false;

    let stations = [4.5f32, 60.0, 400.0, 2_000.0];
    for (i, distance) in stations.into_iter().enumerate() {
        scene.set_camera_distance(distance);
        frame(&store, &mut gpu, &mut scene, &mut r.objects, 1.0 + i as f64);
        let g = scene.degrade_globals();
        let far = i == stations.len() - 1;
        for p in scene.part_degrade_probe() {
            let Some(_) = p.object else {
                assert!(p.is_player, "the local body is not flagged as the player");
                assert_eq!(
                    p.level, 0,
                    "the local body's part {} drew level {} at {:.1} m: level selection \
                     exempts the local player",
                    p.part, p.level, p.distance
                );
                body_max_distance = body_max_distance.max(p.distance);
                if let Some(did) = p.record {
                    if i == 0 {
                        body_with_record += 1;
                    }
                    let info = records.entry(did.0).or_insert_with(|| record(&store, did));
                    // LINT-OK: a level index; the longest shipped record has six.
                    #[allow(clippy::cast_possible_truncation)]
                    let want = get_degrade(info, p.distance, &g).0 as u32;
                    if far && want > 0 {
                        would_have_degraded_far += 1;
                        worst_level_the_record_names = worst_level_the_record_names.max(want);
                    }
                }
                continue;
            };
            if p.level > 0 {
                object_degraded_somewhere = true;
            }
        }
    }

    eprintln!(
        "the local body: {body_with_record} parts carry a multi-level record; the chase camera \
         reached {body_max_distance:.1} m, where the record would have named a level for \
         {would_have_degraded_far} of them (up to level {worst_level_the_record_names}) and the \
         client drew level 0 for every one"
    );
    assert!(
        body_with_record > 0,
        "no part of the body carries a record: a body with nothing to degrade to cannot show \
         that it does not degrade"
    );
    assert!(
        body_max_distance > 500.0,
        "the chase camera never got further than {body_max_distance:.1} m from the body"
    );
    assert!(
        would_have_degraded_far > 0,
        "at {body_max_distance:.1} m the records still name level 0 for every part, so the \
         exemption was never actually tested"
    );
    assert!(
        object_degraded_somewhere,
        "nothing else in the scene ever left level 0, so the frame carries no positive control"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The walk: what a creature draws with the level baked once, and with it selected per frame.
// ---------------------------------------------------------------------------------------------

/// Twelve fixed heights above the settled object centroid. Both arms use the same stations
/// and differ in SceneConfig::part_degrade_levels. Upward motion does not by itself prove that
/// every object's distance increases; the aggregate drawn-count trend is asserted below.
const WALK: [f32; 12] = [
    2.0, 10.0, 25.0, 50.0, 90.0, 140.0, 200.0, 280.0, 400.0, 550.0, 750.0, 1_000.0,
];

/// Compare current scenes with the switch disabled and enabled over identical stations.
/// The disabled arm is the near-band policy: bake once using get_degrade at d = 0 and retain
/// that answer. Its drawn count must stay identical at every station, with no registered
/// multilevel parts or switches.
#[test]
fn baking_the_level_once_draws_the_same_parts_at_every_height() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);

    let mut arm = |on: bool| -> (Vec<usize>, usize, u64, usize, u32, usize) {
        let mut r = populated("first-login-walk-jump");
        let mut scene = scene_for(&store, &mut gpu, &r, on);
        let mut drawn = Vec::new();
        let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);
        eprintln!(
            "arm on={on}: centroid {centre:?}, {} objects",
            scene.server_object_count()
        );
        for (i, dz) in WALK.into_iter().enumerate() {
            scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
            frame(&store, &mut gpu, &mut scene, &mut r.objects, 1.0 + i as f64);
            drawn.push(scene.draw.stats.server_object_triangles);
        }
        let out = (
            drawn,
            scene.draw.stats.server_object_triangles_resident,
            scene.draw.stats.part_degrade_switches,
            scene.draw.stats.part_degrade_parts,
            scene.draw.stats.textures_uploaded,
            scene.draw.stats.server_object_batches,
        );
        // Release each arm's textures, so one arm's descriptors never count against the
        // other's.
        scene.release_textures(&mut gpu);
        out
    };

    let (before, held_before, switches_before, parts_before, slots_before, batches_before) =
        arm(false);
    let (after, held_after, switches_after, parts_after, slots_after, batches_after) = arm(true);

    eprintln!("object triangles drawn per station, part_degrade_levels OFF: {before:?}");
    eprintln!("object triangles drawn per station, part_degrade_levels ON : {after:?}");
    #[allow(clippy::cast_precision_loss)]
    let ratio = held_after as f64 / held_before as f64;
    #[allow(clippy::cast_precision_loss)]
    let slot_ratio = f64::from(slots_after) / f64::from(slots_before);
    eprintln!(
        "triangles held: {held_before} -> {held_after} ({ratio:.2}x); descriptor slots: \
         {slots_before} -> {slots_after} ({slot_ratio:.2}x); object batches drawn at the last \
         station: {batches_before} -> {batches_after}; level changes over the walk: \
         {switches_before} -> {switches_after}; parts carrying a multi-level record: \
         {parts_before} -> {parts_after}"
    );

    // The near-band control draws the same triangle count at every height from 2 to 1,000 m.
    assert!(
        before.iter().all(|t| *t == before[0]),
        "the control arm's triangle count moved, so it is not one bake: {before:?}"
    );
    assert_eq!(
        switches_before, 0,
        "the control arm changed a level, so it is not the control"
    );
    assert_eq!(
        parts_before, 0,
        "the control arm registered a record, so it is not the control"
    );

    // The per-frame arm.
    assert!(
        switches_after > 0,
        "nothing ever changed level, so the wire does not run"
    );
    assert!(
        parts_after > 0,
        "no part carries a record: the arms cannot differ"
    );
    assert_ne!(
        after, before,
        "the two arms drew the same thing at every station"
    );
    for (i, (b, a)) in before.iter().zip(&after).enumerate() {
        assert!(
            a <= b,
            "station {i} drew more with the switch on: {a} > {b}"
        );
    }
    assert!(
        after[11] < before[11],
        "at the far station the switch saved nothing: {} vs {}",
        after[11],
        before[11]
    );
    // Require the measured aggregate drawn count not to rise along this upward walk.
    // The assertion is an expected result for this population, not a geometric proof that
    // the camera moves farther from every individual object at every step.
    for w in after.windows(2) {
        assert!(
            w[1] <= w[0],
            "the drawn count rose along the upward walk: {after:?}"
        );
    }
    // And the price: every level is on the device.
    assert!(
        held_after > held_before,
        "no extra levels were held: {held_before} -> {held_after}"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Pixels, with the differ calibrated in both directions first.
// ---------------------------------------------------------------------------------------------

/// Compare SceneConfig::part_degrade_levels off/on at identical stations, plus an independently
/// built repeated enabled arm. Disabled images at different stations must differ; every enabled
/// image must equal its repeated enabled image exactly. Calibrate both outcomes before reading
/// the enabled-versus-disabled measurement.
///
/// All arms omit the local body and replay the same capture prefix with the same fixed time
/// steps and order. Before pixel comparison, require equal object-ID, sequence-frame and forward
/// command tuples. These are explicit animation-state controls, not full transform equality.
#[test]
fn the_frame_changes_and_the_differ_is_calibrated_in_both_directions() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);

    // Four stations rather than one. A creature only degrades when it is far enough away to be
    // *small* on screen, so the changed-pixel count is bounded by the thing under test, and a
    // single station would understate or overstate it by accident.
    const STATIONS: [f32; 4] = [20.0, 35.0, 50.0, 80.0];

    // Walk one scene per arm through all stations: a scene per station would build twelve
    // scenes' descriptors, and reusing each scene keeps station progression comparable across
    // the three independently built arms.
    /// One captured station and its sorted object-ID/sequence-frame/forward-command tuples.
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
            // Two stepped frames at every station, and the same number of steps in the same
            // order in both arms, so the objects are in the same animation frame when station
            // *k* of one arm is compared with station *k* of the other.
            // LINT-OK: a station index, 0..4.
            #[allow(clippy::cast_precision_loss)]
            let t = 20.0 + 2.0 * i as f64;
            let _ = shot(&store, &mut gpu, &mut scene, &mut r.objects, t);
            let img = shot(&store, &mut gpu, &mut scene, &mut r.objects, t + 1.0);
            // An idle animation offset by one frame produces a large unrelated pixel
            // differential. Record each available object's sequence frame and forward command,
            // then compare the sorted tuples before comparing images.
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
    // The known-identical pair: a **separate** replay, scene build, draw and capture of the same
    // arm. Not a file against itself, which would prove only that memcmp works.
    let again = arm(true);

    // The pose gate, before any pixel is counted.
    for (k, (a, b)) in off.iter().zip(on.iter()).enumerate() {
        assert!(
            !a.1.is_empty(),
            "station {k}: no object published an animation frame"
        );
        assert_eq!(
            a.1, b.1,
            "station {k}: the two arms are at different points of their animations, so a pixel \
             differential between them would be about pose and not about the level of detail"
        );
    }
    eprintln!(
        "pose gate: {} objects, identical in both arms at all {} stations; station 0 frame \
         numbers {:?}",
        off[0].1.len(),
        STATIONS.len(),
        off[0]
            .1
            .iter()
            .take(6)
            .map(|(_, f, _)| *f)
            .collect::<Vec<_>>()
    );

    let total = (on[0].0 .1 * on[0].0 .2) as usize;

    // Known positive: the same arm at the nearest and the farthest station. The camera has moved
    // 60 m, so these two frames cannot legitimately be equal.
    let positive = diff(&off[0].0, &off[STATIONS.len() - 1].0);
    assert!(
        positive > 0,
        "the differ read 0 on a pair that is known to differ"
    );

    // Known negative: every station of the two identical arms.
    let mut negative = 0usize;
    for (k, (a, b)) in on.iter().zip(again.iter()).enumerate() {
        let d = diff(&a.0, &b.0);
        assert_eq!(
            d, 0,
            "two identical runs differ by {d} px at station {k}: the frame is not stable"
        );
        negative += d;
    }
    eprintln!(
        "differ calibration: {positive} of {total} px on a known-different pair, {negative} px \
         over {} known-identical pairs (a separate build, draw and capture each)",
        STATIONS.len()
    );

    // The measurement.
    let mut best = 0usize;
    for (k, dz) in STATIONS.iter().enumerate() {
        let changed = diff(&off[k].0, &on[k].0);
        eprintln!("  {dz:5.0} m above the objects: {changed} of {total} px changed");
        best = best.max(changed);
    }
    assert!(
        best > 0,
        "switching every creature's level of detail changed no pixel at any station"
    );
    assert!(
        best * 4 < total,
        "{best} of {total} px changed -- the two frames hold the same landscape and the same \
         objects in the same pose, so this is not a repaint of the scene"
    );
}

// ---------------------------------------------------------------------------------------------
// 5. A multilevel part uses its record's level-zero mesh, not its own graphics-object ID.
// ---------------------------------------------------------------------------------------------

/// The original mesh-array behavior selects each level's graphics-object ID from the record.
/// In particular, level zero need not equal the part's own ID (229 of the 4,131 shipped records
/// differ). Check the body's baked IDs against setup 0x02000001 and its records.
/// Require a positive multilevel population. The differing-ID count is reported, but is not
/// separately required to be positive for this setup.
#[test]
fn a_parts_level_zero_mesh_is_the_records_and_not_the_parts_own_id() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = load_region(&store).expect("the region decodes");
    let cfg = SceneConfig {
        landblock: 0xA9B4,
        character: true,
        land_radius: 0,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let parts: Vec<DataId> = scene
        .character
        .as_ref()
        .expect("a body")
        .driver()
        .part_array
        .parts
        .iter()
        .map(|p| p.gfxobj_id)
        .collect();
    let built = scene.character_built_from().to_vec();
    assert_eq!(built.len(), parts.len(), "one baked id per part");

    let (mut with_record, mut differing) = (0usize, 0usize);
    for (i, id) in parts.iter().enumerate() {
        match record_of(&store, *id) {
            // `BakeCache::degrade_record` treats a single-level record as no record: it cannot
            // switch, so it is drawn as ordinary geometry from the part's own id.
            Some(info) if info.degrades.len() > 1 => {
                with_record += 1;
                let level0 = info.degrades[0].gfxobj_id;
                assert_eq!(
                    built[i], level0,
                    "part {i} carries {id:?} and a record whose level 0 is {level0:?}; the mesh \
                     was baked from {:?}",
                    built[i]
                );
                if level0 != *id {
                    differing += 1;
                }
            }
            _ => assert_eq!(built[i], *id, "part {i} has no multi-level record"),
        }
    }
    eprintln!(
        "setup record 0x02000001: {with_record} of {} parts carry a multi-level GfxObjDegradeInfo, and \
         {differing} of those name a level-0 mesh that is not the part's own id -- those {differing} \
         would draw the wrong mesh at every distance if baked from their own id",
        parts.len()
    );
    assert!(
        with_record > 0,
        "no part of the shipped body carries a record, so this assertion is vacuous"
    );
}

// ---------------------------------------------------------------------------------------------
// 6. The constants, as literals, against the retail dat.
// ---------------------------------------------------------------------------------------------

/// Pin globals and raw record layout rather than reading both sides solely through the same
/// production symbol or decoder. Shared-helper comparisons above could agree on the same
/// mistake; these explicit expectations constrain their common inputs.
///
/// 1. The initial degradation distance, dereth_world_render::consts::S_R_DEGRADE_DISTANCE, is 50.0.
/// 2. Each 20-byte DAT entry stores gfxobj_id, degrade_mode, min, ideal, max at offsets 0, 4,
///    8, 0xC, 0x10. Re-read a raw payload at these offsets and pin its bands in literal metres.
///    This detects an ideal/max decoder swap that comparisons sharing the decoder could miss.
#[test]
fn the_transcribed_constants_are_pinned_as_literals() {
    assert_eq!(
        dereth_world_render::consts::S_R_DEGRADE_DISTANCE,
        50.0,
        "initial degradation distance"
    );
    // And the globals the selection runs with, stated rather than inherited.
    let g = scene_globals_default();
    assert_eq!(g.degrade_distance, 50.0, "the clamp");
    assert_eq!(
        g.deg_mul, 0.0,
        "PINNED_DEG_MUL -- every band below is quoted at bias 0"
    );
    assert_eq!(
        g.force_level, -1,
        "the forced degradation level starts at -1"
    );
    assert!(
        !g.degrades_disabled,
        "the degradation-disable flag starts at zero"
    );

    // ---- 2: the record layout, from the raw payload of one shipped record.
    //
    // Graphics-object ID 0x01000001 names a record with maximum distance 200 m.
    // State its bands below in literal metres, independently of the decoder's field order.
    let store = store();
    let obj_bytes = store
        .read_typed(DbType::GfxObj, DataId(0x0100_0001))
        .expect("graphics-object record reads");
    let obj = GfxObj::decode_payload(DataId(0x0100_0001), &obj_bytes).expect("it decodes");
    let did = obj
        .did_degrade
        .expect("0x01000001 names a GfxObjDegradeInfo");
    let raw = store
        .read_typed(DbType::DegradeInfo, did)
        .expect("the record reads");
    let info = record(&store, did);

    // Payload: `DataId id`, `u32 num_degrades`, then `num_degrades` x 20 bytes.
    let le32 = |o: usize| u32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
    let lef32 = |o: usize| f32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
    assert_eq!(le32(0), did.0, "the record's declared id");
    let n = le32(4) as usize;
    assert_eq!(n, info.degrades.len(), "num_degrades");
    assert_eq!(raw.len(), 8 + n * 20, "a degrade entry is 20 bytes");
    for (i, e) in info.degrades.iter().enumerate() {
        let o = 8 + i * 20;
        assert_eq!(le32(o), e.gfxobj_id.0, "entry {i}: gfxobj_id at +0x00");
        // LINT-OK: a mode is a small enum written as an i32.
        #[allow(clippy::cast_possible_wrap)]
        let mode = le32(o + 4) as i32;
        assert_eq!(mode, e.degrade_mode, "entry {i}: degrade_mode at +0x04");
        assert_eq!(lef32(o + 8), e.min_dist, "entry {i}: min_dist at +0x08");
        assert_eq!(
            lef32(o + 12),
            e.ideal_dist,
            "entry {i}: ideal_dist at +0x0C"
        );
        assert_eq!(lef32(o + 16), e.max_dist, "entry {i}: max_dist at +0x10");
    }
    // The bands themselves, as literals. `min <= ideal <= max` within a level and `ideal`
    // increasing between levels is what makes the field order readable at all: swap `ideal` and
    // `max` and these numbers move.
    let bands: Vec<(f32, f32, f32)> = info
        .degrades
        .iter()
        .map(|e| (e.min_dist, e.ideal_dist, e.max_dist))
        .collect();
    eprintln!("{did:?} ({} levels): {bands:?}", bands.len());
    assert_eq!(
        bands,
        vec![
            (10.0, 25.0, 50.0),
            (25.0, 50.0, 100.0),
            (50.0, 100.0, 200.0),
            (f32::MAX, f32::MAX, f32::MAX),
        ],
        "GfxObjDegradeInfo {did:?}, the record 0x01000001 names"
    );
    // With n > 2, degrades[n-2].max_dist gives 200: the last real level's far edge,
    // excluding the FLT_MAX terminator; the object seam reads this result.
    assert_eq!(
        dereth_world_render::objects::degrade::get_max_degrade_distance(&info),
        200.0,
        "maximum distance excludes the terminator"
    );
    // And the clamp, exercised through `get_degrade` with the bands above. At bias 0 every
    // threshold is that level's `ideal_dist`, so the boundaries are d = 25, 50 and 100 — and
    // `d = max(|dist| - 50, 0)` puts them at 75 m, 100 m and 150 m of actual distance.
    assert_eq!(get_degrade(&info, 0.0, &g).0, 0, "d = 0");
    assert_eq!(get_degrade(&info, 50.0, &g).0, 0, "d = 0 at the clamp");
    assert_eq!(get_degrade(&info, 70.0, &g).0, 0, "d = 20 < ideal 25");
    assert_eq!(get_degrade(&info, 80.0, &g).0, 1, "d = 30, past ideal 25");
    assert_eq!(get_degrade(&info, 105.0, &g).0, 2, "d = 55, past ideal 50");
    assert_eq!(
        get_degrade(&info, 160.0, &g).0,
        3,
        "d = 110, past ideal 100: the terminator"
    );
    // Under the discarded unsubtracted reading, both 70 m and 80 m select level 2.
    // The two counterfactual checks distinguish that reading from the outcomes above.
    let unclamped = |d: f32| {
        info.degrades
            .iter()
            .position(|e| d < e.ideal_dist)
            .unwrap_or(info.degrades.len() - 1)
    };
    assert_eq!(
        unclamped(70.0),
        2,
        "the reading the selection does *not* use"
    );
    assert_eq!(unclamped(80.0), 2);
}

/// Construct the default degradation globals with the multiplier pinned. The literal test
/// checks these fields explicitly and independently checks retail's initial distance.
/// Integration comparisons use scene.degrade_globals() instead; they do not separately assert
/// equality of every live global to every original value.
fn scene_globals_default() -> dereth_world_render::objects::degrade::DegradeGlobals {
    dereth_world_render::objects::degrade::DegradeGlobals::default()
}
