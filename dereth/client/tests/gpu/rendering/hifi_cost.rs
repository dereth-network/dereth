//! Behaviour: none (experimental Horizon rendering effects: how much work the presentation does
//! for a frame)
//!
//! What the presentation's world replay costs over the ordinary frame's: in a frame with
//! buildings in view, it puts the recording's state back only after a pass has drawn at a mark,
//! and it keeps the world's depth at the first building stamp, so the depth seen through the
//! buildings' openings needs only what follows that stamp drawn again, not the world up to it.
//! And on a picture larger than 2560x1440 the ambient occlusion searches a quarter of the texels.
//! The lamps' traced scene is built only for frames that trace through it: by day in the open
//! none of its structures is built, and at night under a still camera its top level is built once
//! and traced as it stands from then on. And walking through a forest, where the trees' detail
//! levels change frame after frame, the sun's shadow geometry is listed again for the new levels
//! and none of it is copied again, the lists of the copies whose objects drew the same kept as they
//! were; and walking a dungeon, whose whole level is one block, its objects' levels change as well,
//! and neither the traced scene's structures are built again for them nor the shadow geometry and
//! the rain's shelter, which the dungeon draws neither of, looked at.
//! The leaves' pictures the sun's shadows are cut by are bound once, not again every frame.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device.

#![cfg(gpu)]

use dereth_client_runtime::present::Scene as _;
use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_render::wgpu::sidecar::SidecarReport;
use dereth_scene::world_scene::SceneWrites;

use crate::instruments::hifi_stations::{self, Shot};

const W: u32 = 960;
const H: u32 = 540;
/// Frames the station is stepped before its report is read.
const STEPS: usize = 6;

fn device(width: u32, height: u32) -> Gpu {
    let cfg = DeviceConfig {
        width,
        height,
        hifi: true,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(Backend::Wgpu, None, &cfg)
        .unwrap_or_else(|e| panic!("this station needs a wgpu device and none opened: {e}"));
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasteriser",
        gpu.adapter_name()
    );
    gpu
}

/// The town at noon, with `spec` ticked, stepped [`STEPS`] frames, the presentation's pipelines
/// waited for after each: the presentation's report of the last frame.
fn town(spec: &str) -> SidecarReport {
    town_sized(spec, W, H)
}

/// The station `name` with `spec` ticked, stepped `steps` frames, the presentation's pipelines
/// waited for after each: the presentation's report after each frame.
fn stepped(name: &str, spec: &str, steps: usize) -> Vec<SidecarReport> {
    let store = crate::common::dats();
    let station = hifi_stations::stations(&store)
        .into_iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no station {name}"));
    let mut gpu = device(W, H);
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(spec).expect("a fidelity spec");
    shot.scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the preferences poll");
    (0..steps)
        .map(|_| {
            shot.step(&mut gpu);
            assert!(
                gpu.hifi_settle(std::time::Duration::from_secs(60)),
                "the pipelines were not built in a minute"
            );
            gpu.hifi_report().expect("the presentation is installed")
        })
        .collect()
}

/// [`town`] on a device `width` by `height`.
fn town_sized(spec: &str, width: u32, height: u32) -> SidecarReport {
    let store = crate::common::dats();
    let station = hifi_stations::stations(&store)
        .into_iter()
        .find(|s| s.name == "holtburg")
        .expect("the town's station");
    let mut gpu = device(width, height);
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(spec).expect("a fidelity spec");
    shot.scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the preferences poll");
    for _ in 0..STEPS {
        shot.step(&mut gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the pipelines were not built in a minute"
        );
    }
    gpu.hifi_report().expect("the presentation is installed")
}

/// The count the presentation noted under `name` for the last frame.
fn note(report: &SidecarReport, name: &str) -> u64 {
    report
        .notes
        .iter()
        .find(|(n, _)| *n == name)
        .map_or_else(|| panic!("no note {name:?} in {report:?}"), |(_, v)| *v)
}

/// A frame holds thousands of marks; the replay puts the recording's state back only after the
/// few at which a pass drew: none with the better lighting alone, and with the physical sky at
/// most twice, once for the sky and once for the horizon's veil.
#[test]
fn the_world_replay_puts_its_state_back_only_after_a_pass_draws_at_a_mark() {
    let lit = town("Lighting=1");
    assert!(lit.census.reshaded > 0, "re-shaded: {:?}", lit.census);
    assert_eq!(lit.census.restored, 0, "{:?}", lit.census);
    let sky = town("Lighting=1,Sky=1");
    assert!(sky.census.reshaded > 0, "re-shaded: {:?}", sky.census);
    assert!(
        (1..=2).contains(&sky.census.restored),
        "the sky drew at its marks: {:?}",
        sky.census
    );
}

/// With buildings in view, the world's depth is kept at the first building stamp, and of the
/// world's draws not one in ten is drawn again for the depth each pixel sees: the re-shaded world
/// and the world drawn as recorded alike.
#[test]
fn the_depth_seen_through_openings_draws_again_only_what_follows_the_first_building_stamp() {
    for spec in ["Lighting=1", "Sky=1"] {
        let report = town(spec);
        let drawn = u64::from(report.census.total());
        let again = note(&report, "drawn again for the unstamped depth");
        assert_eq!(
            note(&report, "kept the depth before the first building stamp"),
            1,
            "{spec}: the town's buildings are stamped: {report:?}"
        );
        assert!(
            drawn > 0 && again * 10 < drawn,
            "{spec}: {again} of {drawn} draws drawn again"
        );
    }
}

/// The ambient occlusion searches every pixel of a 1920x1080 or a 2560x1440 picture, and one
/// texel for each two by two pixels of a 5120x2160 one.
#[test]
fn on_a_picture_larger_than_1440p_the_occlusion_searches_a_quarter_of_the_texels() {
    let _gpu = crate::common::gpu_lock();
    for (width, height, share) in [(1920, 1080, 1), (2560, 1440, 1), (5120, 2160, 4)] {
        let report = town_sized("AmbientOcclusion=1", width, height);
        let searched = note(&report, "occlusion texels searched");
        assert_eq!(
            searched * share,
            u64::from(width) * u64::from(height),
            "{width}x{height}: {searched} texels searched"
        );
    }
}

/// By day in the open nothing traces through the lamps' scene: frame after frame, none of its
/// structures is built, the top level included.
#[test]
fn by_day_in_the_open_the_lamps_traced_scene_builds_no_structure() {
    let _gpu = crate::common::gpu_lock();
    assert!(
        device(W, H).hifi_rays() == Some(true),
        "this test needs a device that traces rays"
    );
    let reports = stepped("holtburg", "Lighting=1,Lamps=1", 40);
    let last = reports.last().expect("frames");
    assert!(last.census.reshaded > 0, "re-shaded: {:?}", last.census);
    assert_eq!(note(last, "traced structures built"), 0, "{last:?}");
    assert_eq!(note(last, "traced top levels built"), 0, "{last:?}");
}

/// At night the lamps trace through the scene: its structures are built, and once every block
/// has arrived, a still camera's frames trace the top level as it stands, building it again in
/// none of the last twenty frames.
#[test]
fn at_night_under_a_still_camera_the_traced_top_level_is_built_once_and_kept() {
    let _gpu = crate::common::gpu_lock();
    assert!(
        device(W, H).hifi_rays() == Some(true),
        "this test needs a device that traces rays"
    );
    let reports = stepped("lamps-street", "Lighting=1,Lamps=1", 40);
    let at = |k: usize| note(&reports[k], "traced top levels built");
    assert!(
        note(&reports[39], "traced structures built") > 0,
        "the lamps' scene was built: {:?}",
        reports[39]
    );
    assert!(at(19) >= 1, "the top level was built");
    assert_eq!(
        at(39),
        at(19),
        "the top level was built again under a still camera"
    );
}

/// Walking twenty metres through the forest, its objects' detail levels change: each change lists
/// again the vertices the shadow geometry draws, of the copies whose objects draw otherwise, the
/// others' lists kept as they were, and no block's geometry is copied again.
#[test]
fn walking_through_a_forest_lists_its_shadow_geometry_again_and_copies_none_of_it() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let station = hifi_stations::stations(&store)
        .into_iter()
        .find(|s| s.name == "forest")
        .expect("the forest's station");
    let mut gpu = device(W, H);
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch("Lighting=1,Shadows=1").expect("a fidelity spec");
    shot.scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the preferences poll");
    for _ in 0..STEPS {
        shot.step(&mut gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the pipelines were not built in a minute"
        );
    }
    // Every block's geometry is handed over, some again after the frames drawn while the
    // presentation's pipelines were built, before the walk.
    let meshed = u64::try_from(shot.scene.census().blocks_meshed).unwrap_or(u64::MAX);
    let held = |gpu: &Gpu| {
        let r = gpu.hifi_report().expect("the presentation is installed");
        note(&r, "sun shadow blocks held")
    };
    for _ in 0..200 {
        if held(&gpu) == meshed {
            break;
        }
        shot.step(&mut gpu);
    }
    assert_eq!(held(&gpu), meshed, "the sun's shadows hold every block");
    let at = |gpu: &Gpu| {
        let r = gpu.hifi_report().expect("the presentation is installed");
        (
            note(&r, "shadow geometry copied"),
            note(&r, "shadow geometry listed"),
            note(&r, "shadow geometry lists kept"),
        )
    };
    let (copied, listed, kept) = at(&gpu);
    assert!(copied > 0, "the blocks' shadow geometry was copied");
    for k in 1..=40u8 {
        shot.walk_to(&station, 0.5 * f32::from(k));
        shot.step(&mut gpu);
    }
    let (copied_after, listed_after, kept_after) = at(&gpu);
    assert_eq!(copied_after, copied, "shadow geometry copied again");
    assert!(
        listed_after > listed,
        "no detail level changed on the walk: listed {listed} then {listed_after}"
    );
    assert!(
        kept_after > kept,
        "every copy's list was made again: kept {kept} then {kept_after}"
    );
}

/// Walking a dungeon's corridors, its objects' detail levels change, and none of it is made again
/// for them: the lamps' traced scene, which takes each object at its finest level, builds none of
/// its structures again, and the vertices the sun's shadow geometry and the rain's shelter draw,
/// which a dungeon casts and shelters none of, are neither listed again nor looked over. The
/// dungeon is one block, and making any of it again costs milliseconds.
#[test]
fn walking_a_dungeon_builds_the_traced_scene_once_whatever_its_objects_levels() {
    let _gpu = crate::common::gpu_lock();
    assert!(
        device(W, H).hifi_rays() == Some(true),
        "this test needs a device that traces rays"
    );
    let store = crate::common::dats();
    let station = hifi_stations::Station {
        name: "dungeon-walk",
        why: "a body in a dungeon's corridor, walked forward and turned",
        place: hifi_stations::Place::Body {
            cell: 0x00B0_0647,
            origin: dereth_primitives::Vec3::new(0.0, -20.0, 18.0),
            yaw: 0.0,
        },
        clock: hifi_stations::Clock::Sunny(0.5),
        weather: false,
        land_radius: 3,
    };
    let mut gpu = device(W, H);
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch("Lighting=1,Shadows=1,Lamps=1,Weather=1")
            .expect("a fidelity spec");
    shot.scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the preferences poll");
    for _ in 0..STEPS * 4 {
        shot.step(&mut gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the pipelines were not built in a minute"
        );
    }
    let at = |gpu: &Gpu| {
        let r = gpu.hifi_report().expect("the presentation is installed");
        (
            note(&r, "traced structures built"),
            note(&r, "detail levels handed over"),
            [
                note(&r, "shadow geometry listed"),
                note(&r, "shadow geometry lists kept"),
                note(&r, "weather shelter geometry listed"),
                note(&r, "weather shelter geometry lists kept"),
            ],
        )
    };
    let (built, levels, lists) = at(&gpu);
    assert!(built > 0, "the dungeon's traced scene was built");
    shot.input = dereth_client_runtime::character::CharacterInput {
        forward: true,
        run: true,
        ..Default::default()
    };
    for k in 0..120 {
        shot.input.turn_right = k % 40 >= 30;
        shot.step(&mut gpu);
    }
    let (built_after, levels_after, lists_after) = at(&gpu);
    assert!(
        levels_after > levels,
        "no detail level changed on the walk: {levels} then {levels_after} handed over"
    );
    assert_eq!(
        built_after, built,
        "the traced scene was built again as the levels changed"
    );
    assert_eq!(
        lists_after, lists,
        "the shadow geometry's and the shelter's lists were looked over: listed and kept"
    );
}

/// Under a still camera in the forest, the pictures the leaves' shadows are cut by are bound once
/// and kept: the last twenty frames bind none of them again.
#[test]
fn the_leaves_shadow_pictures_are_bound_once_and_kept() {
    let _gpu = crate::common::gpu_lock();
    let reports = stepped("forest", "Lighting=1,Shadows=1", 40);
    let at = |k: usize| note(&reports[k], "leaf shadow pictures bound");
    assert!(at(19) > 0, "the forest's leaves cast shadows");
    assert_eq!(at(39), at(19), "the leaves' pictures were bound again");
}
