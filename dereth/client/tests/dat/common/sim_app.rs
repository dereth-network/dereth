//! The actual client shell and frame loop with a world simulation and no graphics device.
//!
//! Behaviour: none (device-free application fixture and its presentation boundary)

use dereth_client_runtime::sim_present::SimPresentation;
use dereth_client_shell::platform::host::Host;
use {
    dereth_client::app::App, dereth_client_runtime::app::Platform,
    dereth_client_runtime::app::StartupError, dereth_client_runtime::config::Config,
};

#[path = "../../common/app.rs"]
#[allow(dead_code)]
mod shared;

#[allow(unused_imports)]
pub use shared::{
    body, frames, gameplay, key, movement_key, player_description, position, unhide_player,
    unhide_recorded_player,
};

/// Bring up the application at the dimensions selected by its normal startup policy.
/// The caller retains control of shell startup, scene loading and frame advancement.
pub fn new(cfg: Config) -> Result<App, StartupError> {
    assert!(
        cfg.headless,
        "the simulation fixture requires a headless platform"
    );
    dereth_client::hud::install_platform();
    dereth_client::Desktop::install_default_output();
    App::bring_up_with_store(
        cfg,
        None,
        Default::default(),
        |cfg| Ok(Platform::headless(cfg.width, cfg.height)),
        |_, width, height, _| Ok(Box::new(SimPresentation::new(width, height))),
    )
}

/// Start the UI and configured static scene, then advance into gameplay.
#[allow(dead_code)]
pub fn app_in_gameplay(frames: u32) -> App {
    shared::app_in_gameplay_with(frames, new)
}

/// Build the recorded player on real terrain and finish its recorded unhide.
pub fn app_with_recorded_body() -> App {
    shared::app_with_recorded_body_with(new)
}

#[test]
fn simulated_ui_operations_are_counted_without_a_graphics_device() {
    use dereth_primitives::{DataId, ObjectId, TextureData, TextureFormat, Viewport};
    use {
        dereth_client_shell::present::ClientPresentation,
        dereth_client_shell::present::UiReleaseReport,
    };
    let store = dereth_dat::testing::open_store_or_fail();
    let mut sim = SimPresentation::new(320, 240);
    let rect = Viewport {
        x: 1,
        y: 2,
        width: 30,
        height: 40,
    };
    let preview = dereth_client_contract::overlay::PreviewSpace::PaperDoll;
    let handle = dereth_ui::ElemHandle::for_test(1);
    sim.prepare_ui(&store, &store, &[]);
    sim.draw_ui(&[]).unwrap();
    assert_eq!(sim.release_ui_textures(), UiReleaseReport::default());
    sim.set_movie_frame(
        DataId(1),
        &TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0; 4]],
        },
    );
    sim.preview_queue(preview, handle, rect);
    sim.preview_queue_under_text(preview, vec![handle], rect);
    sim.preview_queue_under_text(preview, vec![], rect);
    assert!(sim.target_projection(ObjectId(1), None).is_none());
    let counts = sim.device().counts();
    assert_eq!(counts.prepare_ui, 1);
    assert_eq!(counts.draw_ui, 1);
    assert_eq!(counts.release_ui_textures, 1);
    assert_eq!(counts.set_movie_frame, 1);
    assert_eq!(counts.preview_calls, 2);
}

#[test]
fn the_real_shell_frames_and_resizes_its_simulated_world() {
    use {dereth_client_runtime::config::Preferences, dereth_client_runtime::scene::SceneConfig};
    let scratch = dereth_dat::testing::ScratchDir::new("simulated-shell").unwrap();
    let mut cfg = Config {
        headless: true,
        connect: false,
        sound: false,
        ui: true,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: scratch.path().join("prefs.ini"),
        ..Default::default()
    };
    cfg.apply_preferences(&Preferences::parse("[Display]\nResolution=1024x768\n"));
    let mut app = new(cfg).expect("real shell with simulation");
    assert_eq!(app.presentation().size(), (800, 600), "late login size");
    assert_eq!(app.window.client_size(), (800, 600));
    app.start_shell().expect("real input and UI");
    app.load_static_scene(SceneConfig {
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("real terrain and body");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    frames(&mut app, 8);
    assert_eq!(
        app.presentation().size(),
        (1024, 768),
        "gameplay restores preference"
    );
    assert_eq!(app.ui_mut().expect("real UI").ui.display(), (1024, 768));
    assert!(!app.ui_draw_list().is_empty(), "real UI produces commands");
    let handle = body(&app).handle;
    let sim = app
        .presentation()
        .as_any()
        .downcast_ref::<SimPresentation>()
        .unwrap();
    assert!(sim.steps.updates > 0, "frames drive world physics");
    assert!(sim.resident_blocks().next().is_some());
    let counts = sim.device().counts();
    assert!(counts.prepare_ui > 0 && counts.draw_ui > 0);
    assert!(counts.start_frame > 0 && counts.draw_scene > 0);
    assert_eq!(counts.start_frame, counts.end_frame);
    app.force_display_resolution(true, 1280, 720);
    assert_eq!(app.presentation().size(), (1280, 720));
    assert_eq!(app.ui_mut().expect("real UI").ui.display(), (1280, 720));
    assert_eq!(
        body(&app).handle,
        handle,
        "resize keeps the same physical body"
    );
    let sim = app
        .presentation()
        .as_any()
        .downcast_ref::<SimPresentation>()
        .unwrap();
    assert!(sim.device().counts().resize > counts.resize);
    app.shutdown();
}

#[test]
fn the_shared_recorded_body_fixture_runs_through_the_simulated_app() {
    let mut app = app_with_recorded_body();
    let before = position(&app);
    let forward = dereth_client_runtime::actions::names::action_for_enum_name("MovementForward")
        .expect("movement action");
    app.inject_action(dereth_client_runtime::actions::Action::begin(forward));
    frames(&mut app, 60);
    app.inject_action(dereth_client_runtime::actions::Action::end(forward));
    frames(&mut app, 1);
    let after = position(&app);
    assert!(dereth_animation::motion::moveto::distance(&before, &after) > 2.0);
    app.shutdown();
}
