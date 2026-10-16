//! Character select draws the same Holtburg whether the landscape is built at once or streamed
//! over several frames, as a connected client streams it.
//!
//! A streamed window bakes its blocks' objects off the frame, but the block the viewer stands in
//! is built as the window is: the camera the backdrop is built with stands over Holtburg's hill,
//! where an unstreamed load stands it, and every building, static and piece of scenery comes in
//! over the frames after, every block held by Better Lighting.
//! Fixture: the retail dats' Holtburg (landblock 0xA9B4) behind character select, in a headless
//! client with Better Lighting's boxes ticked.
//!
//! Behaviour: none (an equivalence of the streamed and the unstreamed landscape inside the
//! client, not a retail claim)

#![cfg(all(gpu, feature = "hifi"))]

use std::time::{Duration, Instant};

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::present::Scene as _;
use dereth_scene::world_scene::SceneReads as _;

/// The block character select's backdrop is built round: Holtburg.
const HOLTBURG: u16 = 0xA9B4;

/// What the backdrop showed: the camera it was built with, the landscape's blocks, buildings,
/// statics and scenery once nothing was left to stream, whether Better Lighting drew it, and the
/// blocks it held for the sun's shadows, where it says.
#[derive(Debug, PartialEq)]
struct Backdrop {
    camera: [u32; 5],
    census: (usize, usize, usize, usize),
    composited: bool,
    held: Option<u64>,
}

/// Whether Better Lighting drew the last frame, and how many blocks it held for the sun's
/// shadows, where it says.
fn better_lighting(app: &App) -> (bool, Option<u64>) {
    let report = app.renderer().gpu.hifi_report();
    (
        report.as_ref().is_some_and(|r| r.composited),
        report.and_then(|r| {
            r.notes
                .iter()
                .find(|(n, _)| *n == "sun shadow blocks held")
                .map(|(_, v)| *v)
        }),
    )
}

/// Character select's backdrop in a client that streams its landscape `budget_ms` a frame (0
/// builds it at once), stepped until nothing is left to stream and Better Lighting holds every
/// block.
fn backdrop(budget_ms: u32) -> Backdrop {
    let mut cfg = Config {
        headless: true,
        sound: false,
        ui: false,
        width: 320,
        height: 240,
        preferences_file: std::env::temp_dir()
            .join("dereth-backdrop-streaming-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        stream_budget_ms: Some(budget_ms),
        ..Config::default()
    };
    cfg.renderer = Some(dereth_client_contract::RendererChoice::Wgpu);
    cfg.render.fidelity = dereth_client_runtime::render_prefs::FidelityPreferences::parse_switch(
        "Lighting=1,Shadows=1,Sky=1",
    )
    .expect("the boxes parse");
    let scene = cfg.scene_config();
    assert_eq!(
        scene.stream_budget.is_some(),
        budget_ms > 0,
        "the landscape streams as asked"
    );
    let mut app = App::new(cfg).expect("the client starts");
    app.start_shell().expect("the shell comes up");
    // As a client that connects: the landscape waits for the server, and character select
    // draws its backdrop meanwhile.
    app.defer_static_scene(scene);
    assert!(app.show_backdrop(HOLTBURG, 0.0, 0.0), "the backdrop is up");
    let c = app.world_state().expect("the backdrop's world").camera;
    let camera = [
        c.position.x.to_bits(),
        c.position.y.to_bits(),
        c.position.z.to_bits(),
        c.yaw.to_bits(),
        c.pitch.to_bits(),
    ];
    // A guard against a hang, not a measure: the threads finish in seconds.
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let w = app.world_scene().expect("the backdrop's world");
        let meshed = u64::try_from(w.census().blocks_meshed).unwrap_or(u64::MAX);
        let (composited, held) = better_lighting(&app);
        if w.pending_slot_count() == 0 && composited && held.is_none_or(|h| h == meshed) {
            break;
        }
        assert!(Instant::now() < deadline, "the backdrop never finished");
        app.frame();
        std::thread::sleep(Duration::from_millis(1));
    }
    let c = app.world_scene().expect("the backdrop's world").census();
    let (composited, held) = better_lighting(&app);
    Backdrop {
        camera,
        census: (
            c.blocks_meshed,
            c.buildings,
            c.static_objects,
            c.scenery_objects,
        ),
        composited,
        held,
    }
}

#[test]
fn character_select_draws_the_same_holtburg_from_the_same_camera_while_the_landscape_streams() {
    let _gpu = crate::common::gpu_lock();
    let at_once = backdrop(0);
    let streamed = backdrop(4);
    assert!(
        at_once.census.1 > 0 && at_once.composited && at_once.held != Some(0),
        "Holtburg's buildings, drawn by Better Lighting: {at_once:?}"
    );
    assert_eq!(
        streamed.camera, at_once.camera,
        "the camera the backdrop is built with"
    );
    assert_eq!(
        streamed, at_once,
        "the streamed backdrop (blocks, buildings, statics, scenery; Better Lighting's)"
    );
}
