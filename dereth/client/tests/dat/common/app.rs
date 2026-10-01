//! A headless `App` in gameplay over the retail dats: the shell up, the static scene loaded and
//! the gameplay screen current, with no graphics device (`NullPresentation`) and no network.

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::present::NullPresentation;
use dereth_primitives::ObjectId;
use dereth_ui::UiSystem;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

use super::client_dir;

/// A headless app in gameplay after `frames` frames.
///
/// With `player`, that id is seeded as the world's player before the first frame. The player's
/// qualities record is allocated when the player object is made, before any update can arrive;
/// a test that applies HUD events without replaying the creates that build the player needs the
/// same state, so the row is adopted through `set_player`, which allocates the qualities and
/// installs any parked description. A bare `Weenie::new` has no qualities record, and an update
/// arriving at one is refused as unstorable rather than creating it.
///
/// The app answers for the server it does not have (`dereth_client::server_stub`), as every
/// headless app with no connection does; [`app_in_gameplay_unanswered`] is the one that does not.
pub fn app_in_gameplay(frames: u32, player: Option<ObjectId>) -> App {
    build(frames, player, true)
}

/// [`app_in_gameplay`] with no stand-in server: nothing the client asks for is answered unless
/// the test answers it, so whatever waits on an answer waits until the test gives one.
pub fn app_in_gameplay_unanswered(frames: u32, player: Option<ObjectId>) -> App {
    build(frames, player, false)
}

fn build(frames: u32, player: Option<ObjectId>, answered: bool) -> App {
    let cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::with_presentation(cfg, Box::new(NullPresentation::new(800, 600)))
        .expect("the headless app starts");
    assert!(
        app.server_stub.is_some(),
        "a headless app with no connection answers for its server"
    );
    if !answered {
        app.server_stub = None;
    }
    app.start_shell().expect("the shell comes up");
    if let Some(player) = player {
        let w = &mut app.objects_mut().world;
        w.tables
            .weenies
            .insert(player, dereth_client_model::weenie::Weenie::new(player));
        assert!(w.set_player(player), "the identity is adopted once");
    }
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

/// The UI system and the current gameplay screen of an app built by [`app_in_gameplay`].
pub fn gameplay_screen(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the shell exists");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen is current");
    (ui, screen)
}
