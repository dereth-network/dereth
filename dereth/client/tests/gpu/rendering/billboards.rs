//! Billboards: a placement or a creature part whose selected degrade level names a billboarding
//! mode turns toward the viewer (mode 5 about its upright axis; mode 1 never), a forced level pins
//! every baked placement, and billboarding costs batch reassembly, not memory. The per-placement
//! and per-part checks rerun the production degrade and draw-frame helpers on what the scene
//! published and compare exactly; paired frames differing only in the billboard switch prove the
//! pixels move, with the differ calibrated in both directions.
//! Fixture: the retail dats' degrade records; Holtburg (landblock 0xA9B4) with no character, and
//! the `first-login-walk-jump` recording replayed into a live scene, on a software device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo};
use dereth_client::character::CharacterInput;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, StaticLevelProbe, WorldScene};
use dereth_client_net::client_session::testing::capture::{self, peer as addr, Datagram as Record};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, LocalTime, ObjectId, Vec3};
use dereth_render::device::Gpu;
use dereth_world_render::objects::degrade::{
    calc_draw_frame, get_degrade, DegradeGlobals, DegradeMode,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Holtburg, the fixed outdoor landblock used for these static-scene tests.
const HOLTBURG: u16 = 0xA9B4;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn warp() -> Gpu {
    crate::common::software_gpu(800, 600)
}

/// No local body or server-object stream is introduced, so there is no character animation or
/// chase-camera motion. Repeated builds follow identical time steps; the pixel tests separately
/// require repeated enabled arms to be identical rather than assuming all rendering is static.
fn still(store: &Arc<RetailDatStore>, gpu: &mut Gpu, billboards: bool) -> WorldScene {
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        character: false,
        scenery_radius: 1,
        static_billboards: billboards,
        ..SceneConfig::default()
    };
    WorldScene::load(store, gpu, cfg).expect("the scene loads")
}

/// One frame, with the camera parked wherever the caller put it.
fn frame(store: &Arc<RetailDatStore>, gpu: &mut Gpu, scene: &mut WorldScene, t: f64) -> Vec<u8> {
    let mut stream = ObjectStream::new();
    scene
        .sync_objects(store, gpu, &mut stream)
        .expect("sync_objects");
    scene.update(
        dereth_client::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(t),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

/// Changed pixels between two RGBA captures of the same size.
fn diff(a: &[u8], b: &[u8]) -> usize {
    assert_eq!(a.len(), b.len(), "two captures of different sizes");
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0.iter())
        .filter(|(p, q)| p != q)
        .count()
}

/// A cache of `GfxObjDegradeInfo`s read back from the dat, so a per-placement comparison costs one
/// decode per distinct record rather than one per placement.
#[derive(Default)]
struct Records(BTreeMap<u32, GfxObjDegradeInfo>);

impl Records {
    fn get(&mut self, store: &RetailDatStore, id: DataId) -> &GfxObjDegradeInfo {
        self.0.entry(id.0).or_insert_with(|| {
            let bytes = store
                .read_typed(DbType::DegradeInfo, id)
                .expect("the record the scene is holding is in the dat");
            GfxObjDegradeInfo::decode_payload(id, &bytes).expect("it decodes")
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 1. The constants, spelled as numbers
// ---------------------------------------------------------------------------------------------

/// Pin the mode values and the initial globals as literals, independently of their production
/// symbols. The multiplier is fixed at zero here. The worked bands are 10/25/50, 25/50/100,
/// 50/100/200 and FLT_MAX.
///
/// The degrade distance is subtracted before the bands are consulted: distance 75 becomes 25 and
/// selects level 1. Using 75 directly would select level 2 and skip that intermediate band.
#[test]
fn the_transcribed_constants_are_pinned_as_literals() {
    // Raw public DAT mode numbers, independent of the enum variants used below.
    assert_eq!(
        DegradeMode::from_raw(1) as i32,
        1,
        "mode 1: draw with the part's own frame"
    );
    assert_eq!(
        DegradeMode::from_raw(2) as i32,
        2,
        "mode 2: face the camera direction"
    );
    assert_eq!(DegradeMode::from_raw(3) as i32, 3, "mode 3: rotate about X");
    assert_eq!(DegradeMode::from_raw(4) as i32, 4, "mode 4: rotate about Y");
    assert_eq!(
        DegradeMode::from_raw(5) as i32,
        5,
        "mode 5: rotate about Z, the foliage card"
    );
    // Unrecognized modes map to no billboard. Exercise 0, 6 and -1 explicitly; baking uses this
    // interpretation when deciding whether a record requires part-local geometry.
    assert_eq!(DegradeMode::from_raw(0), DegradeMode::None);
    assert_eq!(DegradeMode::from_raw(6), DegradeMode::None);
    assert_eq!(DegradeMode::from_raw(-1), DegradeMode::None);

    // The retail forced-level and disable-state initial values, plus the pinned multiplier.
    let g = DegradeGlobals::default();
    assert_eq!(
        g.force_level, -1,
        "the forced degradation level starts at -1"
    );
    assert!(!g.degrades_disabled, "degrades_disabled ships at 0");
    assert_eq!(g.degrade_distance, 50.0, "the initial degradation distance");
    assert_eq!(
        g.deg_mul, 0.0,
        "the pinned multiplier starts at zero; every band below is quoted at bias 0"
    );

    // Worked bands written explicitly: 10/25/50, 25/50/100, 50/100/200, then FLT_MAX.
    let info = GfxObjDegradeInfo {
        id: DataId(0x1100_0000),
        degrades: vec![
            dereth_assets::motion::GfxObjInfo {
                gfxobj_id: DataId(0x0100_3769),
                degrade_mode: 1,
                min_dist: 10.0,
                ideal_dist: 25.0,
                max_dist: 50.0,
            },
            dereth_assets::motion::GfxObjInfo {
                gfxobj_id: DataId(0x0100_376E),
                degrade_mode: 5,
                min_dist: 25.0,
                ideal_dist: 50.0,
                max_dist: 100.0,
            },
            dereth_assets::motion::GfxObjInfo {
                gfxobj_id: DataId(0x0100_376F),
                degrade_mode: 1,
                min_dist: 50.0,
                ideal_dist: 100.0,
                max_dist: 200.0,
            },
            dereth_assets::motion::GfxObjInfo {
                gfxobj_id: DataId(0),
                degrade_mode: 1,
                min_dist: f32::MAX,
                ideal_dist: f32::MAX,
                max_dist: f32::MAX,
            },
        ],
    };
    // d = max(|distance| - 50, 0). Reading 75 directly would select level 2 and leave level 1
    // unreachable.
    assert_eq!(
        get_degrade(&info, 50.0, &g),
        (0, DegradeMode::None),
        "50 m is d = 0"
    );
    assert_eq!(get_degrade(&info, 74.999, &g).0, 0);
    assert_eq!(
        get_degrade(&info, 75.0, &g),
        (1, DegradeMode::AxisZ),
        "75 m is d = 25: level 1, and its mode 5 comes back with it"
    );
    assert_eq!(get_degrade(&info, 120.0, &g).0, 2, "120 m is d = 70");

    // The second branch: `force_level` pins the level, clamped to the record's last one.
    let pinned = DegradeGlobals {
        force_level: 1,
        ..g
    };
    assert_eq!(
        get_degrade(&info, 0.0, &pinned),
        (1, DegradeMode::AxisZ),
        "pinned at 0 m"
    );
    assert_eq!(
        get_degrade(&info, 10_000.0, &pinned),
        (1, DegradeMode::AxisZ),
        "and at 10 km"
    );
    let over = DegradeGlobals {
        force_level: 99,
        ..g
    };
    assert_eq!(
        get_degrade(&info, 0.0, &over).0,
        3,
        "force_level clamps to num_degrades - 1"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The corpus: how much of the dat billboards at all
// ---------------------------------------------------------------------------------------------

/// Census the distinct degrade records referenced by the dat's graphics objects. Failed reads or
/// decodes are skipped; the counts pin the measured scope: 4,131 distinct records and the mode
/// 2/3/4/5 level counts. Total levels and mode 1 are reported but not asserted. Any billboarding
/// mode in a record requires the baker to keep part-local geometry, whichever level a later frame
/// selects.
#[test]
fn the_shipped_mode_census_over_the_whole_dat() {
    let store = store();
    let mut modes: BTreeMap<i32, usize> = BTreeMap::new();
    let (mut records, mut billboarding_records) = (0usize, 0usize);
    let mut seen = std::collections::HashSet::new();
    let mut levels = 0usize;
    for id in store.ids_of(DbType::GfxObj) {
        let Ok(bytes) = store.read_typed(DbType::GfxObj, id) else {
            continue;
        };
        let Ok(obj) = GfxObj::decode_payload(id, &bytes) else {
            continue;
        };
        let Some(did) = obj.did_degrade else { continue };
        if !seen.insert(did.0) {
            continue;
        }
        let Ok(bytes) = store.read_typed(DbType::DegradeInfo, did) else {
            continue;
        };
        let Ok(info) = GfxObjDegradeInfo::decode_payload(did, &bytes) else {
            continue;
        };
        records += 1;
        let mut any = false;
        for e in &info.degrades {
            levels += 1;
            *modes.entry(e.degrade_mode).or_default() += 1;
            any |= DegradeMode::from_raw(e.degrade_mode) != DegradeMode::None;
        }
        billboarding_records += usize::from(any);
    }
    eprintln!(
        "{records} distinct GfxObjDegradeInfo records, {levels} levels, mode histogram {modes:?}; \
         {billboarding_records} records name a billboarding mode at some level"
    );
    // Independent expected counts from the shipped-data census.
    assert_eq!(records, 4_131, "distinct degrade records in the dat");
    assert_eq!(
        modes.get(&2).copied().unwrap_or(0),
        259,
        "mode 2, the full billboard"
    );
    assert_eq!(
        modes.get(&3).copied().unwrap_or(0),
        0,
        "mode 3 never occurs in shipped data"
    );
    assert_eq!(modes.get(&4).copied().unwrap_or(0), 11, "mode 4, about Y");
    assert_eq!(
        modes.get(&5).copied().unwrap_or(0),
        573,
        "mode 5, the upright foliage card"
    );
    assert!(
        billboarding_records > 0,
        "no record billboards, so this module has no subject"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. Forced-level state
// ---------------------------------------------------------------------------------------------

/// Drive forced-level selection through the scene's per-frame path and its published probe. The
/// setter is an explicit test input, not a claim about what sets the level in play. Pinning and
/// releasing it checks the scene's wiring beyond the isolated `get_degrade` arithmetic.
#[test]
fn force_level_pins_every_baked_placement_through_the_scene() {
    let store = store();
    let mut gpu = warp();
    let mut scene = still(&store, &mut gpu, true);
    assert_eq!(
        scene.force_level(),
        -1,
        "a scene starts unpinned, matching the retail initial value"
    );

    scene.camera.position = Vec3::new(96.0, 96.0, 300.0);
    let _ = frame(&store, &mut gpu, &mut scene, 1.0);
    let free = scene.degrade_probe();
    let spread: std::collections::BTreeSet<u32> = free.iter().map(|p| p.level).collect();
    eprintln!("unpinned: {} placements over levels {spread:?}", free.len());
    assert!(
        free.len() > 1_000,
        "only {} placements over Holtburg",
        free.len()
    );
    assert!(
        spread.len() > 1,
        "every placement is already on one level: nothing to pin"
    );

    for pin in [0i32, 1, 2] {
        scene.set_force_level(pin);
        assert_eq!(
            scene.degrade_globals().force_level,
            pin,
            "the pin did not reach the globals"
        );
        let _ = frame(&store, &mut gpu, &mut scene, 2.0 + f64::from(pin));
        let probe = scene.degrade_probe();
        assert_eq!(
            probe.len(),
            free.len(),
            "the resident placement set moved with the pin"
        );
        let mut records = Records::default();
        let mut hist: BTreeMap<u32, usize> = BTreeMap::new();
        for p in &probe {
            let n = records.get(&store, p.record).degrades.len();
            // LINT-OK: a level index into a record whose longest shipped form has six levels.
            #[allow(clippy::cast_possible_truncation)]
            let want = (pin as usize).min(n - 1) as u32;
            assert_eq!(
                p.level, want,
                "{:?} has {n} levels: pinned at {pin} it drew {} and get_degrade says {want}",
                p.record, p.level
            );
            *hist.entry(p.level).or_default() += 1;
        }
        eprintln!("force_level = {pin}: levels {hist:?}");
    }

    // And releasing it returns every placement to the distance-driven answer it had before.
    scene.set_force_level(-1);
    let _ = frame(&store, &mut gpu, &mut scene, 9.0);
    let released = scene.degrade_probe();
    assert_eq!(
        released.iter().map(|p| p.level).collect::<Vec<_>>(),
        free.iter().map(|p| p.level).collect::<Vec<_>>(),
        "releasing the pin did not restore the distance-driven levels"
    );
    scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// 4. The wire: every placement's mode and draw frame, against calc_draw_frame's own answer
// ---------------------------------------------------------------------------------------------

/// Re-read each record from client_portal.dat and rerun `get_degrade` and `calc_draw_frame`
/// outside the scene on `WorldScene::degrade_probe`'s published distance, model frame and viewer
/// heading. This checks assembly against the shared production helpers, not independent
/// arithmetic: level, mode and the full draw frame must agree, and mode 1 must keep the model frame.
///
/// The live static denominator is reported at three stations; a non-zero population must select a
/// billboarding mode and actually turn.
#[test]
fn every_placement_billboards_the_way_calc_draw_frame_says_at_three_stations() {
    let store = store();
    let mut gpu = warp();
    let mut scene = still(&store, &mut gpu, true);
    let mut records = Records::default();

    let (mut checked, mut billboarded, mut turned) = (0usize, 0usize, 0usize);
    let mut baked_local = 0usize;
    let mut mode_hist: BTreeMap<i32, usize> = BTreeMap::new();
    for (i, height) in [40.0f32, 160.0, 600.0].into_iter().enumerate() {
        scene.camera.position = Vec3::new(96.0, 96.0, height);
        // LINT-OK: a station index, 0..3.
        #[allow(clippy::cast_precision_loss)]
        let t = 1.0 + i as f64;
        let _ = frame(&store, &mut gpu, &mut scene, t);
        let g = scene.degrade_globals();
        let probe = scene.degrade_probe();
        assert!(
            probe.len() > 1_000,
            "only {} placements over Holtburg",
            probe.len()
        );

        for p in &probe {
            let info = records.get(&store, p.record);
            let (want_level, want_mode) = get_degrade(info, p.distance, &g);
            // LINT-OK: a level index; the longest shipped record has six. Not a float.
            #[allow(clippy::cast_possible_truncation)]
            let want_level = want_level as u32;
            assert_eq!(
                p.level, want_level,
                "{:?} at d = {}: the scene drew level {} and get_degrade says {want_level}",
                p.record, p.distance, p.level
            );
            assert_eq!(
                p.mode, want_mode,
                "{:?} level {}: the scene decoded mode {:?} and get_degrade says {want_mode:?}",
                p.record, p.level, p.mode
            );
            // **The claim.** `draw_pos` is what `calc_draw_frame` returns for the frame and the
            // heading the scene measured -- not merely "different from `pos`".
            let want = calc_draw_frame(&p.pos, p.mode, p.viewer_heading);
            assert_eq!(
                p.draw_pos, want,
                "{:?} level {} mode {:?}: draw_pos is not calc_draw_frame's answer",
                p.record, p.level, p.mode
            );
            if p.mode == DegradeMode::None {
                assert_eq!(p.draw_pos, p.pos, "mode 1 must leave the frame alone");
            } else {
                billboarded += 1;
                if p.draw_pos != p.pos {
                    turned += 1;
                }
            }
            if i == 0 {
                baked_local += usize::from(p.billboards);
            }
            *mode_hist.entry(p.mode as i32).or_default() += 1;
            checked += 1;
        }
        eprintln!(
            "z = {height:6.1} m: {} placements, {} baked part-local, {} drawing a billboarding \
             level, {} turned this frame; {} triangles drawn of {} held",
            probe.len(),
            scene.draw.stats.degrade_placements_billboarding,
            scene.draw.stats.degrade_placements_billboarded,
            scene.draw.stats.degrade_billboard_turns,
            scene.draw.stats.object_triangles,
            scene.draw.stats.object_triangles_resident
        );
    }
    eprintln!(
        "{checked} placement-level comparisons against get_degrade and calc_draw_frame, none \
         disagreeing; {billboarded} of those {checked} selected levels ask for a mode other than \
         1, and {turned} of them actually moved the frame; mode histogram {mode_hist:?}; \
         {baked_local} of the resident placements are baked part-local"
    );
    assert!(
        checked > 3_000,
        "only {checked} comparisons; the denominator is too small to mean much"
    );
    assert!(
        billboarded > 0,
        "no selected level billboards, so this module's subject is not resident"
    );
    // A billboard whose card happens to be aligned with the viewer already needs no rotation, so
    // `turned < billboarded` is legitimate; `turned == 0` would mean nothing ever moved.
    assert!(
        turned > 0,
        "{billboarded} levels asked to billboard and not one frame moved"
    );
    assert!(
        baked_local > 0,
        "no placement was baked part-local, so nothing can turn"
    );
    scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// 5. The card turns as the camera orbits it
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.billboards.a-mode-five-card-turns-with-the-camera-and-a-mode-one-static-never-does
/// Orbit the fixed block center at four bearings with radius 70 and height 120. A placement's
/// distance can still vary with its offset from that center; only pairs retaining their level
/// qualify. Consecutive probes must preserve record order and each static's model frame.
///
/// Across the three consecutive bearing pairs, at least one non-mode 1 frame must turn, while
/// observed mode 1 frames must not. Mode 5 separately has a positive upright-check denominator:
/// the local-Z matrix component must remain near 1. The moved counter covers all billboard modes,
/// so this test does not separately require a mode 5 turn or a positive mode 1 population.
#[test]
fn a_mode_five_card_turns_as_the_camera_orbits_and_a_mode_one_static_never_does() {
    let store = store();
    let mut gpu = warp();
    let mut scene = still(&store, &mut gpu, true);

    // Four bearings at the same radius and the same height about the block's centre.
    const R: f32 = 70.0;
    const CENTRE: (f32, f32) = (96.0, 96.0);
    let stations: [(f32, f32); 4] = [
        (CENTRE.0 + R, CENTRE.1),
        (CENTRE.0, CENTRE.1 + R),
        (CENTRE.0 - R, CENTRE.1),
        (CENTRE.0, CENTRE.1 - R),
    ];

    let mut shots: Vec<Vec<StaticLevelProbe>> = Vec::new();
    for (i, (x, y)) in stations.into_iter().enumerate() {
        scene.camera.position = Vec3::new(x, y, 120.0);
        // LINT-OK: a station index, 0..4.
        #[allow(clippy::cast_precision_loss)]
        let t = 1.0 + i as f64;
        let _ = frame(&store, &mut gpu, &mut scene, t);
        shots.push(scene.degrade_probe());
    }
    let n = shots[0].len();
    assert!(
        shots.iter().all(|s| s.len() == n),
        "the resident placement set moved between bearings"
    );

    // Every placement, at every consecutive pair of bearings.
    let (mut comparable, mut moved, mut mode1, mut mode1_moved) = (0usize, 0usize, 0usize, 0usize);
    let mut upright = 0usize;
    for k in 0..stations.len() - 1 {
        for (a, b) in shots[k].iter().zip(shots[k + 1].iter()) {
            assert_eq!(
                a.record, b.record,
                "the probe order is not stable between frames"
            );
            assert_eq!(
                a.pos, b.pos,
                "a static's pos.frame moved, which is not what a static is"
            );
            if a.level != b.level {
                continue;
            }
            if a.mode == DegradeMode::None {
                mode1 += 1;
                mode1_moved += usize::from(a.draw_pos != b.draw_pos);
                continue;
            }
            comparable += 1;
            moved += usize::from(a.draw_pos != b.draw_pos);
            if a.mode == DegradeMode::AxisZ {
                // Mode 5 keeps the card upright: `l2g(draw_pos.rotation)`'s local Z stays (0,0,1).
                let m = dereth_world_render::math::l2g(a.draw_pos.rotation).0;
                assert!(
                    (m[8] - 1.0).abs() < 1e-3,
                    "a mode-5 card is not upright: {:?} m[8] = {}",
                    a.record,
                    m[8]
                );
                upright += 1;
            }
        }
    }
    eprintln!(
        "orbit: {comparable} billboarding placement-pairs held their level across a bearing \
         change, {moved} of them turned; {upright} of the mode-5 pairs were checked upright; \
         {mode1} mode-1 pairs, of which {mode1_moved} moved"
    );
    assert!(
        comparable > 0,
        "no billboarding placement held its level across the orbit"
    );
    assert!(
        moved > 0,
        "{comparable} cards faced a new bearing and not one of them turned"
    );
    assert_eq!(
        mode1_moved, 0,
        "a mode-1 placement's draw frame moved with the camera"
    );
    assert!(
        upright > 0,
        "no mode-5 placement was resident to check for uprightness"
    );
    scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// 6. The cost: resident triangles, descriptors, batches and reassembly
// ---------------------------------------------------------------------------------------------

/// Compare SceneConfig::static_billboards off/on over the same eight-station walk.
/// The resource expectations are equal resident triangle counts, held texture-descriptor counts
/// and batch counts, with equal per-station drawn-triangle counts. These are resource counters,
/// not total memory bytes or timing measurements. The runtime cost measured here is batch-group
/// reassembly: no station may do less work when enabled, and its total must be greater.
#[test]
fn the_cost_is_reassembly_and_not_memory() {
    let store = store();
    let mut gpu = warp();
    const WALK: [f32; 8] = [60.0, 90.0, 130.0, 180.0, 250.0, 340.0, 460.0, 620.0];

    let mut arm = |on: bool| {
        let mut scene = still(&store, &mut gpu, on);
        let mut drawn = Vec::new();
        let mut assemblies = Vec::new();
        let mut turns = Vec::new();
        // By the far station (620 m) the nearby cards have passed their billboarding levels, so
        // keep the whole series: some station must select one, and the far one fewer.
        let mut billboarded = Vec::new();
        for (i, z) in WALK.into_iter().enumerate() {
            scene.camera.position = Vec3::new(96.0, 96.0, z);
            // LINT-OK: a station index.
            #[allow(clippy::cast_precision_loss)]
            let t = 1.0 + i as f64;
            let _ = frame(&store, &mut gpu, &mut scene, t);
            drawn.push(scene.draw.stats.object_triangles);
            assemblies.push(scene.draw.stats.degrade_assemblies);
            turns.push(scene.draw.stats.degrade_billboard_turns);
            billboarded.push(scene.draw.stats.degrade_placements_billboarded);
        }
        // One more stationary frame advances LocalTime by the same one-second step as the walk. A
        // larger jump can cross a sky-lighting update that rebakes and reassembles blocks, which
        // would be counted as billboarding cost.
        // LINT-OK: the station count.
        #[allow(clippy::cast_precision_loss)]
        let t = 1.0 + WALK.len() as f64;
        let _ = frame(&store, &mut gpu, &mut scene, t);
        let out = (
            drawn,
            assemblies,
            turns,
            billboarded,
            scene.draw.stats.degrade_assemblies,
            scene.draw.stats.degrade_billboard_turns,
            scene.draw.stats.object_triangles_resident,
            scene.draw.stats.textures_uploaded,
            scene.draw.stats.object_batches,
            scene.draw.stats.degrade_placements_billboarding,
        );
        scene.release_textures(&mut gpu);
        out
    };

    let (d_off, a_off, tn_off, sel_off, s_off, t_off, held_off, slots_off, batches_off, local_off) =
        arm(false);
    let (d_on, a_on, tn_on, sel_on, s_on, t_on, held_on, slots_on, batches_on, local_on) =
        arm(true);

    #[allow(clippy::cast_precision_loss)]
    let ratio = held_on as f64 / held_off as f64;
    let slot_ratio = f64::from(slots_on) / f64::from(slots_off);
    eprintln!("object triangles drawn per station, static_billboards OFF: {d_off:?}");
    eprintln!("object triangles drawn per station, static_billboards ON : {d_on:?}");
    eprintln!("batch groups reassembled per station,           OFF: {a_off:?}");
    eprintln!("batch groups reassembled per station,           ON : {a_on:?}");
    eprintln!("cards that turned per station,                  OFF: {tn_off:?}");
    eprintln!("cards that turned per station,                  ON : {tn_on:?}");
    eprintln!("placements selecting a billboarding level,      OFF: {sel_off:?}");
    eprintln!("placements selecting a billboarding level,      ON : {sel_on:?}");
    eprintln!(
        "triangles held: {held_off} -> {held_on} ({ratio:.2}x); descriptor slots: {slots_off} -> \
         {slots_on} ({slot_ratio:.2}x); object batches: {batches_off} -> {batches_on}; placements \
         baked part-local: {local_off} -> {local_on}; on a still frame at the last station the \
         reassembly count is {s_off} -> {s_on} and the turn count {t_off} -> {t_on}"
    );

    // The denominator is a property of the dat and the window, not of the switch, so it must read
    // the same in both arms at every station. This is the counter that would flatter itself if it
    // were gated on whether the bake can honour the mode.
    assert_eq!(
        sel_off, sel_on,
        "the billboarding denominator moved with the switch"
    );
    assert!(
        sel_on.iter().any(|n| *n > 0),
        "no station of the walk has a placement selecting a billboarding level: {sel_on:?}"
    );
    // A card only billboards while it is close enough to be on a billboarding level, so the series
    // falls as the camera climbs. It does not fall to zero: a card in a land cell more than 50 m
    // away across the ground is measured at that cell's ground distance, which the camera's height
    // does not change, so the top two stations select the same cards.
    let peak = *sel_on.iter().max().expect("eight stations");
    assert!(
        sel_on[WALK.len() - 1] < peak,
        "the far station selects as many billboarding levels as the peak: {sel_on:?}"
    );
    assert_eq!(
        sel_on[WALK.len() - 1],
        sel_on[WALK.len() - 2],
        "the camera's height moved a far cell's cards: {sel_on:?}"
    );
    // The control holds nothing part-local; the switched arm does.
    assert_eq!(local_off, 0, "the control arm baked something part-local");
    assert!(local_on > 0, "the switched arm baked nothing part-local");
    // Hold these resource counts equal; this is not a complete memory-allocation census.
    assert_eq!(held_off, held_on, "billboarding changed the triangles held");
    assert_eq!(
        slots_off, slots_on,
        "billboarding changed the descriptor slots"
    );
    assert_eq!(
        batches_off, batches_on,
        "billboarding changed the batch count"
    );
    assert_eq!(
        d_off, d_on,
        "billboarding changed which triangles are drawn, which it must not"
    );
    // And the assembly cost. A still camera reassembles nothing in either arm, so a still frame
    // costs nothing extra; the switched arm reassembles *more* than the control while the camera
    // moves, which is what billboarding costs.
    assert_eq!(s_off, 0, "the control arm reassembled on a still frame");
    assert_eq!(
        s_on, 0,
        "the switched arm reassembled on a still frame: the turn guard is not working"
    );
    assert_eq!(t_on, 0, "a card turned with the camera standing still");
    assert!(
        tn_off.iter().all(|n| *n == 0),
        "the control arm turned a card: {tn_off:?}"
    );
    assert!(
        tn_on.iter().any(|n| *n > 0),
        "no card turned anywhere along the walk: {tn_on:?}"
    );
    for (k, (b, a)) in a_off.iter().zip(&a_on).enumerate() {
        assert!(
            a >= b,
            "station {k} reassembled less with the switch on: {a} < {b}"
        );
    }
    assert!(
        a_on.iter().sum::<usize>() > a_off.iter().sum::<usize>(),
        "the switch cost nothing"
    );
}

// ---------------------------------------------------------------------------------------------
// 7. Pixels, with the differ calibrated in both directions
// ---------------------------------------------------------------------------------------------

/// Compare the same scene/stations with SceneConfig::static_billboards off and on.
/// A third, independently built enabled arm must match every enabled image exactly. A pair of
/// different stations in the disabled arm must differ, calibrating both answers of the instrument.
///
/// No local body or server objects are added, and every arm uses identical dt, times and station
/// order. Repeated-arm pixel equality validates stability beyond that construction. The final
/// selected-placement count is a last-station report, while the measured difference spans all
/// four stations; the per-placement tests carry the mode/frame claim.
#[test]
fn the_frame_changes_and_the_differ_is_calibrated_in_both_directions() {
    let store = store();
    let mut gpu = warp();
    // Four stations. A billboard is only visible where the cards are, so a single station would
    // understate or overstate the count by accident.
    const STATIONS: [(f32, f32, f32); 4] = [
        (96.0, 96.0, 70.0),
        (96.0, 40.0, 90.0),
        (40.0, 96.0, 120.0),
        (150.0, 150.0, 160.0),
    ];

    let mut arm = |on: bool| {
        let mut scene = still(&store, &mut gpu, on);
        let mut out = Vec::new();
        for (i, (x, y, z)) in STATIONS.into_iter().enumerate() {
            scene.camera.position = Vec3::new(x, y, z);
            // LINT-OK: a station index.
            #[allow(clippy::cast_precision_loss)]
            let t = 10.0 + 2.0 * i as f64;
            let _ = frame(&store, &mut gpu, &mut scene, t);
            out.push(frame(&store, &mut gpu, &mut scene, t + 1.0));
        }
        let selected = scene.draw.stats.degrade_placements_billboarded;
        scene.release_textures(&mut gpu);
        (out, selected)
    };

    let (off, _) = arm(false);
    let (on, selected) = arm(true);
    // The known-identical pair: a **separate** scene build, draw and capture of the same arm.
    let (again, _) = arm(true);

    let total = off[0].len() / 4;

    // Known positive: the same arm at two different stations. The camera has moved, so these two
    // frames cannot legitimately be equal.
    let positive = diff(&off[0], &off[STATIONS.len() - 1]);
    assert!(
        positive > 0,
        "the differ read 0 on a pair that is known to differ"
    );

    // Known negative: every station of the two identical arms.
    let mut negative = 0usize;
    for (k, (a, b)) in on.iter().zip(again.iter()).enumerate() {
        let d = diff(a, b);
        assert_eq!(
            d, 0,
            "two identical runs differ by {d} px at station {k}: the frame is unstable"
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
    let mut sum = 0usize;
    for (k, (x, y, z)) in STATIONS.into_iter().enumerate() {
        let changed = diff(&off[k], &on[k]);
        eprintln!("  camera ({x}, {y}, {z}): {changed} of {total} px changed");
        best = best.max(changed);
        sum += changed;
    }
    eprintln!(
        "{sum} px changed over {} stations; {selected} resident placements selected a billboarding \
         level. The claim is carried by the per-placement oracle, not by this number.",
        STATIONS.len()
    );
    assert!(
        best > 0,
        "turning every billboarding card changed no pixel at any station"
    );
    assert!(
        best * 4 < total,
        "{best} of {total} px changed -- the two arms hold the same landscape, the same meshes \
         and the same levels, so this is not a repaint of the scene"
    );
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
// Parse the fixed recording-line format locally, as the neighboring replay harnesses do.
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
    /// The first player landblock observed during the retained replay prefix.
    landblock: u16,
}

/// Replay through the last datagram after which the object stream is nonempty.
/// A logoff can empty the final stream, so find that last populated prefix first. This is not
/// a maximum-population search and does not assume every recording ends in a clean logout.
/// The returned landblock is the first player position observed within the retained prefix.
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

/// One frame_parts with the camera parked wherever the caller put it, and the object stream synced.
fn frame_parts(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    s: &mut ObjectStream,
    t: f64,
) {
    scene.sync_objects(store, gpu, s).expect("sync_objects");
    scene.update(
        dereth_client::camera::CameraInput::default(),
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

/// The same frame_parts, captured. A separate entry point so that the tests that do not need pixels do
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
        dereth_client::camera::CameraInput::default(),
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
fn diff_parts(a: &(Vec<u8>, u32, u32), b: &(Vec<u8>, u32, u32)) -> usize {
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
/// it. The static placements have the same trap.
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
/// The centroid can lie across a landblock boundary (y = 253 m in a 192 m block). Moving the
/// camera recenters the streaming window and derives object frames against a new origin, so a
/// station based on the old centroid is in the wrong space. Move, step and remeasure, requiring less than 0.5 m centroid change within
/// six attempts before using the returned point for later stations.
fn park_over_objects(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    s: &mut ObjectStream,
) -> Vec3 {
    let mut centre = Vec3::ZERO;
    let mut last = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    for i in 0..6 {
        frame_parts(store, gpu, scene, s, 0.1 * f64::from(i));
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
        // Keep degrade-level selection enabled in both arms. Only the viewer-facing frame_parts
        // substitution varies; changing selected geometry too would mix two mechanisms.
        part_degrade_levels: true,
        part_billboards: on,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
    scene.set_weather_enabled(false);
    scene
}
// ---------------------------------------------------------------------------------------------
// 1. The wire: draw_pos is calc_draw_frame's answer, for every part of every object
// ---------------------------------------------------------------------------------------------

/// Compare published mode/frame_parts results with helpers invoked outside the scene at four stations.
/// The mode comes from a freshly decoded DAT record, and the frame_parts uses published model position
/// and viewer heading. The live denominator is reported, not pinned.
#[test]
fn every_part_draws_the_frame_calc_draw_frame_names_at_four_stations() {
    let store = store();
    let mut gpu = warp();
    let mut r = populated("first-login-walk-jump");
    let mut scene = scene_for(&store, &mut gpu, &r, true);
    let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);

    let mut records: BTreeMap<u32, GfxObjDegradeInfo> = BTreeMap::new();
    let (mut checked, mut billboarded, mut turned) = (0usize, 0usize, 0usize);
    let mut modes: BTreeMap<i32, usize> = BTreeMap::new();
    for (i, dz) in [10.0f32, 60.0, 150.0, 400.0].into_iter().enumerate() {
        scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
        // LINT-OK: a station index.
        #[allow(clippy::cast_precision_loss)]
        let t = 1.0 + i as f64;
        frame_parts(&store, &mut gpu, &mut scene, &mut r.objects, t);
        let g = scene.degrade_globals();
        let probe = scene.part_degrade_probe();
        assert!(
            !probe.is_empty(),
            "station {i}: the scene published no parts"
        );
        for p in &probe {
            // The mode, against the record rather than against the scene's own decode.
            let want_mode = match (p.record, p.is_player) {
                (Some(id), false) => {
                    let info = records.entry(id.0).or_insert_with(|| record(&store, id));
                    get_degrade(info, p.distance, &g).1
                }
                // A player object bypasses the record, retaining level 0 and mode 1.
                // A missing record also takes this mode fallback. Even a player's record naming
                // mode 5 must not induce billboarding through this selection branch.
                _ => DegradeMode::None,
            };
            assert_eq!(
                p.mode, want_mode,
                "{:?} part {} at d = {}: the scene decoded {:?}, get_degrade says {want_mode:?}",
                p.record, p.part, p.distance, p.mode
            );
            let want = calc_draw_frame(&p.pos, p.mode, p.viewer_heading);
            assert_eq!(
                p.draw_pos, want,
                "{:?} part {} mode {:?}: draw_pos is not calc_draw_frame's answer",
                p.record, p.part, p.mode
            );
            if p.mode == DegradeMode::None {
                assert_eq!(p.draw_pos, p.pos, "mode 1 must leave the frame_parts alone");
            } else {
                billboarded += 1;
                turned += usize::from(p.draw_pos != p.pos);
            }
            *modes.entry(p.mode as i32).or_default() += 1;
            checked += 1;
        }
        eprintln!(
            "  +{dz:5.0} m: {} parts, {} on a billboarding level, {} turned",
            probe.len(),
            scene.draw.stats.part_billboards,
            scene.draw.stats.part_billboards_turned
        );
    }
    eprintln!(
        "{checked} part comparisons against get_degrade and calc_draw_frame, none disagreeing; \
         {billboarded} of them are on a billboarding level and {turned} of those turned; mode \
         histogram {modes:?}"
    );
    assert!(
        checked > 1_000,
        "only {checked} comparisons; the denominator is too small"
    );
    assert!(
        billboarded > 0,
        "no part selected a billboarding level over this capture"
    );
    assert!(
        turned > 0,
        "{billboarded} parts asked to billboard and not one frame_parts moved"
    );
    scene.release_textures(&mut gpu);
}

// ---------------------------------------------------------------------------------------------
// 2. The control arm, and the pixels, with the differ calibrated in both directions
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.billboards.a-billboarding-part-faces-the-viewer
/// Compare independently replayed scenes differing only in SceneConfig::part_billboards.
/// Both use the same retained recording prefix, fixed dt and ordered stations. Before comparing
/// pixels, require identical sorted object-id/animation-frame_parts/forward-command tuples for the
/// enabled and disabled arms. This is the explicit animation-state gate, not a comparison of
/// every model transform: an animation mismatch would otherwise read as a rendering change.
///
/// Calibrate the differ with disabled-arm images at different stations and with repeated enabled
/// builds at matching stations. Require a positive result for the former and zero for every
/// latter pair, before testing enabled-versus-disabled pixels.
#[test]
fn a_billboarding_part_turns_and_the_control_arm_does_not() {
    let store = store();
    let mut gpu = warp();
    const STATIONS: [f32; 4] = [8.0, 15.0, 30.0, 60.0];

    type Station = (
        (Vec<u8>, u32, u32),
        Vec<(ObjectId, i32, dereth_animation::MotionCommand)>,
    );

    let mut arm = |on: bool| -> (Vec<Station>, usize, usize) {
        let mut r = populated("first-login-walk-jump");
        let mut scene = scene_for(&store, &mut gpu, &r, on);
        let centre = park_over_objects(&store, &mut gpu, &mut scene, &mut r.objects);
        let mut out: Vec<Station> = Vec::with_capacity(STATIONS.len());
        let (mut asked, mut moved) = (0usize, 0usize);
        for (i, dz) in STATIONS.iter().enumerate() {
            scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + dz);
            // LINT-OK: a station index.
            #[allow(clippy::cast_precision_loss)]
            let t = 20.0 + 2.0 * i as f64;
            let _ = shot(&store, &mut gpu, &mut scene, &mut r.objects, t);
            let img = shot(&store, &mut gpu, &mut scene, &mut r.objects, t + 1.0);
            asked = asked.max(scene.draw.stats.part_billboards);
            moved = moved.max(scene.draw.stats.part_billboards_turned);
            let mut pose: Vec<(ObjectId, i32, dereth_animation::MotionCommand)> = r
                .objects
                .presences()
                .filter_map(|(id, _)| scene.server_object_motion(id).map(|(f, c)| (id, f, c)))
                .collect();
            pose.sort_by_key(|(id, _, _)| id.0);
            out.push((img, pose));
        }
        scene.release_textures(&mut gpu);
        (out, asked, moved)
    };

    let (off, asked_off, moved_off) = arm(false);
    let (on, asked_on, moved_on) = arm(true);
    let (again, _, _) = arm(true);

    for (k, (a, b)) in off.iter().zip(on.iter()).enumerate() {
        assert!(
            !a.1.is_empty(),
            "station {k}: no object published an animation frame_parts"
        );
        assert_eq!(
            a.1, b.1,
            "station {k}: the two arms are at different points of their animations, so a pixel \
             differential between them would be about pose and not about billboarding"
        );
    }
    eprintln!(
        "pose gate: {} objects, identical in both arms at all {} stations; station 0 frame_parts \
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

    // These counters are each arm's maximum over its stations, not stationwise totals.
    // Require equal positive maxima for selected billboard modes, zero turned in the disabled
    // arm and a positive turned maximum when enabled.
    eprintln!(
        "parts on a billboarding level: {asked_off} (off) / {asked_on} (on); parts whose frame_parts \
         moved: {moved_off} (off) / {moved_on} (on)"
    );
    assert_eq!(
        asked_off, asked_on,
        "the billboarding denominator moved with the switch"
    );
    assert!(
        asked_on > 0,
        "no part selects a billboarding level: the arms cannot differ"
    );
    assert_eq!(moved_off, 0, "the control arm turned a part");
    assert!(moved_on > 0, "the switched arm turned nothing");

    let total = (on[0].0 .1 * on[0].0 .2) as usize;
    let positive = diff_parts(&off[0].0, &off[STATIONS.len() - 1].0);
    assert!(
        positive > 0,
        "the differ read 0 on a pair that is known to differ"
    );
    let mut negative = 0usize;
    for (k, (a, b)) in on.iter().zip(again.iter()).enumerate() {
        let d = diff_parts(&a.0, &b.0);
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
        let changed = diff_parts(&off[k].0, &on[k].0);
        eprintln!("  +{dz:5.0} m above the objects: {changed} of {total} px changed");
        best = best.max(changed);
    }
    assert!(
        best > 0,
        "turning every billboarding part changed no pixel at any station"
    );
    assert!(
        best * 4 < total,
        "{best} of {total} px changed -- both arms hold the same objects in the same pose at the \
         same levels, so this is not a repaint of the scene"
    );
}
