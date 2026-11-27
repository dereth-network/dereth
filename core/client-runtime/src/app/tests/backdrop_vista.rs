//! The view behind character select: a world that names one in its overlay folder is shown from
//! there, at its hour; one that names several shows one of them, the same all run; a world that
//! names none shows the interface's own block from the default camera.
//!
//! Behaviour: none (the backdrop is a screen of this client's own interface)

use std::path::Path;
use std::sync::Arc;

use dereth_dat::overlay::OverlayDir;
use dereth_dat::{ModernDat, RetailDatStore};
use dereth_primitives::DataId;

use super::*;

/// The block the interface asks for, and the one the world's view names instead.
const ASKED: u16 = 0xA9B4;
const NAMED: u16 = 0xA8B3;

fn retail_store() -> RetailDatStore {
    dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {}",
            dereth_dat::testing::dat_dir().display()
        )
    })
}

/// `store` with a world's overlay folder over it, `dir`, holding one picture of its own and, when
/// given, the view `vista` names.
fn world_over(store: RetailDatStore, dir: &Path, vista: Option<&str>) -> RetailDatStore {
    let folder = OverlayDir::new(dir).expect("an overlay folder");
    let base = store.portal();
    let mut w = folder
        .writer(
            ModernDat::Portal,
            base,
            "client_portal.dat",
            "Vista World",
            1,
        )
        .expect("an overlay writer");
    let next = dereth_dat::overlay::base_iterations(base)
        .last()
        .copied()
        .unwrap_or(0)
        + 1;
    w.put(DataId(0x0600_0001), b"a picture of its own", 1, next, 1)
        .expect("a record");
    w.add_iteration(next, 1).expect("its revision");
    w.flush(1).expect("written");
    if let Some(text) = vista {
        std::fs::write(dir.join(crate::vista::FILE), text).expect("the vista written");
    }
    let (laid, refused) = crate::world_overlay::lay_folder(
        store,
        Some(&OverlayDir::new(dir).expect("the folder")),
        &std::collections::BTreeSet::new(),
    );
    assert_eq!(refused, None, "the world's overlay is laid");
    laid
}

/// A client over `store` whose run picks its view with `seed`, its backdrop's scene armed.
fn client(store: RetailDatStore, seed: u64) -> App<NullShell> {
    let cfg = Config {
        headless: true,
        connect: false,
        sound: false,
        ..Config::default()
    };
    let mut app = App::<NullShell>::bring_up_with_store(
        cfg,
        Some(Arc::new(store)),
        |_| Ok(Platform::headless(64, 64)),
        |_, _, _, _| Ok(Box::new(crate::sim_present::SimPresentation::new(64, 64))),
    )
    .expect("bring-up");
    app.vista_seed = seed;
    app.defer_static_scene(crate::scene::SceneConfig {
        landblock: ASKED,
        character: false,
        ..crate::scene::SceneConfig::default()
    });
    app
}

/// The backdrop's block and its camera as it is built, and its hour once a frame has stepped it,
/// the interface asking for [`ASKED`].
fn shown(store: RetailDatStore) -> ((i32, i32), crate::camera::FreeCamera, f32) {
    let mut app = client(store, 0);
    assert!(app.show_backdrop(ASKED, 0.0, 0.0), "the backdrop is up");
    let ws = app.world.as_ref().expect("the backdrop's world");
    let (block, camera) = (ws.viewer_block().expect("a viewer block"), ws.camera);
    let mut shell = NullShell;
    assert!(app.frame(&mut shell));
    assert!(app.show_backdrop(ASKED, 0.0, 0.0), "the backdrop stays up");
    let ws = app.world.as_ref().expect("the backdrop's world");
    (block, camera, ws.clock.present_time_of_day)
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_world_that_names_a_view_shows_character_select_from_there_and_one_that_names_none_does_not() {
    let scratch = dereth_dat::testing::ScratchDir::new("backdrop-vista").expect("scratch");
    let (named, plain) = (scratch.path().join("named"), scratch.path().join("plain"));

    let (block, camera, hour) = shown(world_over(
        retail_store(),
        &named,
        Some(
            "version = 1\ncamera = \"0xA8B3 [60 150 120] yaw 2.5 pitch -0.3\"\ntime_of_day = 0.7\n",
        ),
    ));
    assert_eq!(
        block,
        crate::report::block_xy(NAMED),
        "the view's block is shown"
    );
    assert_eq!(
        (camera.position.x, camera.position.y, camera.position.z),
        (60.0, 150.0, 120.0),
        "the camera stands where the view says"
    );
    assert_eq!((camera.yaw, camera.pitch), (2.5, -0.3));
    assert!((0.65..0.75).contains(&hour), "the view's hour is {hour}");

    // The same world naming no view, and the files with no world over them: the interface's own
    // block from the default camera, at mid-morning.
    for store in [world_over(retail_store(), &plain, None), retail_store()] {
        let (block, camera, hour) = shown(store);
        assert_eq!(
            block,
            crate::report::block_xy(ASKED),
            "the interface's block is shown"
        );
        assert_ne!(
            (camera.position.x, camera.position.y),
            (60.0, 150.0),
            "the default camera"
        );
        assert!((0.35..0.45).contains(&hour), "the hour is {hour}");
    }
}

/// Two views, the first at [`NAMED`], the second at the block east of it.
const TWO: &str = "version = 1
[[vista]]
camera = \"0xA8B3 [60 150 120] yaw 2.5 pitch -0.3\"
[[vista]]
camera = \"0xA9B3 [30 40 130] yaw 1 pitch -0.1\"
";

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_world_that_names_several_views_shows_the_one_its_runs_seed_picks_all_run() {
    let scratch = dereth_dat::testing::ScratchDir::new("backdrop-vistas").expect("scratch");
    let dir = scratch.path().join("world");
    let views = crate::vista::parse(TWO).expect("the views read");
    let world = world_over(retail_store(), &dir, Some(TWO));
    // A seed that picks each view.
    let seeds: Vec<u64> = views
        .iter()
        .map(|v| {
            (0..64)
                .find(|s| crate::vista::pick(&views, *s).as_ref() == Some(v))
                .expect("a seed picks it")
        })
        .collect();
    for (view, seed) in views.iter().zip(seeds) {
        let mut app = client(world.clone(), seed);
        let block = |app: &App<NullShell>| {
            app.world
                .as_ref()
                .and_then(crate::world_state::WorldState::viewer_block)
                .expect("the backdrop's block")
        };
        assert!(app.show_backdrop(ASKED, 0.0, 0.0), "the backdrop is up");
        assert_eq!(block(&app), crate::report::block_xy(view.camera.block));
        // Released and built again in the same run, as after logging out: the same view.
        app.hide_backdrop();
        assert!(
            app.show_backdrop(ASKED, 0.0, 0.0),
            "the backdrop is up again"
        );
        assert_eq!(block(&app), crate::report::block_xy(view.camera.block));
    }
}
