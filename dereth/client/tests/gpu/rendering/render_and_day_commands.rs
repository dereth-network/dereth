//! Typed chat commands that change what the renderer draws: `@render radius|fov <value>` goes
//! through the graphics-option parser (decimal `atoi`, radius `5..=25`, FOV `10..=160`) to the live
//! preferences, the projection and the landscape ring; `@day` toggles the landscape's
//! always-daylight state and the persisted at-day option; `@framerate` shows and hides the shipped
//! smart-box frame-rate meter. Each is driven from a line typed in the chat entry.
//!
//! Fixture: a real GPU-backed headless App and the shared typed-chat input hand, with a
//! small static landscape; no datagram leaves the process.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::chat::{
    app_with, bubbles, chat_log, forced_saved_player_module, gameplay_element,
    install_player_module, Hand,
};
use crate::common::gpu_lock;

use dereth_assets::Decode;
use dereth_client::{app::App, config::Config, world::SceneConfig};
use dereth_primitives::AssetSource;
use dereth_ui::framework::mode;

fn app() -> App {
    app_with(|cfg| App::new(cfg).expect("required retail DATs and headless graphics device"))
}

/// Behaviour: chat.commands.a-verb-the-client-knows-answers-it-and-a-word-it-does-not-know-is-passed-on
///
/// The render command forwards its option word and arguments to the graphics-option parser. The
/// parser uses decimal `atoi`, ignores tokens after the value, accepts radius `5..=25` and FOV
/// `10..=160`, and writes the live preferences. Its first output is type `0x1A`; the usage output
/// is type zero. A usage return is false and therefore also reaches the dispatcher's failure
/// `0x26`, while an unknown option is a silent true.
///
/// This station starts with a small radius-two landscape, so the smallest accepted radius proves
/// the real ring consumer without constructing the maximum 51-by-51 block window.
#[test]
fn typed_render_preserves_the_native_parser_and_reaches_projection_and_landscape_ring() {
    const RAW_USAGE_AFTER_SCROLL_TRIM: &str = "Usage:\n@render <option> <value>\n  radius #        : set landscape radius (between 5 and 25)\n  fov #           : set field of view (between 10 and 160)";
    // The text-tag formatter requires both colons and successful type and format lookups.
    // `<option>` and `<value>` fail before tag construction and remain literal.
    const VISIBLE_USAGE: &str = RAW_USAGE_AFTER_SCROLL_TRIM;
    const W: u32 = 800;
    const H: u32 = 600;
    let _gpu = gpu_lock();
    let dir = std::env::temp_dir().join(format!(
        "dereth-render-and-day-commands-{}",
        std::process::id()
    ));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("remove this test's old disposable directory");
    }
    std::fs::create_dir_all(&dir).expect("create the disposable preference directory");
    let preferences_file = dir.join("preferences.ini");
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: W,
        height: H,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: preferences_file.clone(),
        ..Config::default()
    })
    .expect("required retail DATs and headless graphics device");
    app.start_shell().expect("UI shell");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..4 {
        assert!(app.frame());
    }
    let store = std::sync::Arc::new(dereth_dat::testing::open_store_or_fail());
    app.load_world(
        &store,
        SceneConfig {
            land_radius: 2,
            scenery_radius: 0,
            character: false,
            render: dereth_client::render_prefs::RenderPreferences {
                landscape_draw_distance: 2,
                ..dereth_client::render_prefs::RenderPreferences::default()
            },
            ..SceneConfig::default()
        },
    )
    .expect("the small real landscape loads");
    for _ in 0..3 {
        assert!(app.frame());
    }
    let mut hand = Hand::new();
    let before_unimplemented = app.interaction().stats.chat_commands_unimplemented;
    let (before_view, before_radius, before_blocks) = {
        let scene = app.world_scene().expect("the real scene remains loaded");
        (
            scene.view_params(W, H),
            scene.mid_radius(),
            scene.resident_blocks(),
        )
    };
    assert_eq!(before_radius, 2);
    assert!(
        before_blocks > 0,
        "the radius consumer needs resident blocks to replace"
    );

    hand.submit(&mut app, "/render");
    assert!(
        app.objects()
            .world
            .scroll
            .pending()
            .iter()
            .any(|line| line.chat_type == 0 && line.body == RAW_USAGE_AFTER_SCROLL_TRIM),
        "Scroll preserves native's angle-bracket words before the shared glyph boundary; only its documented surrounding-whitespace trim has run"
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    let first_usage_log = chat_log(&mut app);
    assert!(
        first_usage_log.contains(VISIBLE_USAGE),
        "the exact type-zero usage text is visible; log={first_usage_log:?}; bubbles={:?}",
        bubbles(&mut app)
    );
    assert!(
        bubbles(&mut app)
            .iter()
            .any(|line| line == dereth_client_model::cmd::NOT_A_VALID_COMMAND),
        "the parser's false return reaches dispatcher failure 0x26"
    );
    hand.submit(&mut app, "@render usage");
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        chat_log(&mut app).matches(VISIBLE_USAGE).count() >= 2,
        "the explicit usage word follows the same false-return path"
    );

    for (line, expected) in [
        ("/render radius", "Must specify a radius"),
        ("/render radius 4", "Radius must be between 5 and 25"),
        ("/render fov", "Must specify a field of view"),
        (
            "/render fov 161",
            "Field of view must be between 10 and 160",
        ),
    ] {
        hand.submit(&mut app, line);
        for _ in 0..3 {
            assert!(app.frame());
        }
        assert!(
            bubbles(&mut app).iter().any(|text| text == expected),
            "{line} must print {expected:?} on type 0x1A"
        );
    }

    let silent_bubbles = bubbles(&mut app);
    let silent_log = chat_log(&mut app);
    hand.submit(&mut app, "@render unknown 99");
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert_eq!(
        bubbles(&mut app),
        silent_bubbles,
        "an unknown render option is a silent true"
    );
    assert_eq!(
        chat_log(&mut app),
        silent_log,
        "unknown does not synthesize usage either"
    );

    hand.submit(&mut app, "/render FoV 120tail ignored");
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(bubbles(&mut app)
        .iter()
        .any(|line| line == "Field of view set"));
    assert_eq!(
        dereth_ui_screens::options::store::inq_value("Render.FieldOfView"),
        Some(dereth_ui_screens::PrefValue::Float(120.0)),
        "decimal atoi keeps the numeric prefix and ignores the later token"
    );
    let view = app.world_scene().expect("world").view_params(W, H);
    let aspect = (W as f32 / H as f32) * (4.0 / 3.0) * dereth_render::camera::ASPECT_FACTOR;
    let expected_fov = (120.0 * dereth_render::camera::DEG_TO_RAD)
        / (aspect - dereth_render::camera::FOV_ASPECT_BIAS);
    assert_ne!(
        view.fov_y_rad, before_view.fov_y_rad,
        "the command must change the projection"
    );
    assert!(
        (view.fov_y_rad - expected_fov).abs() < 1e-6,
        "the actual projection has FOV {}, expected {expected_fov}",
        view.fov_y_rad
    );

    hand.submit(&mut app, "@render radius 5 ignored");
    assert!(app.frame());
    assert!(
        app.last_render_pref_work().mid_radius_changed,
        "the next render frame records the config change queued at the command-frame tail"
    );
    for _ in 0..2 {
        assert!(app.frame());
    }
    assert!(bubbles(&mut app)
        .iter()
        .any(|line| line == "Landscape radius set"));
    assert_eq!(
        dereth_ui_screens::options::store::inq_value("Render.LandscapeDrawDistance"),
        Some(dereth_ui_screens::PrefValue::Int(5))
    );
    let scene = app.world_scene().expect("world");
    assert_eq!(
        scene.mid_radius(),
        5,
        "the world controller's middle-radius setter reaches the live ring"
    );
    assert!(
        scene.resident_blocks() > before_blocks,
        "the radius-five ring was not repopulated"
    );
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        before_unimplemented
    );
    assert!(
        app.interaction().last_sent.is_empty(),
        "render options are local preferences"
    );
    assert!(app.interaction().pending_requests().is_empty());

    app.shutdown();
    let saved =
        std::fs::read_to_string(&preferences_file).expect("shutdown saved disposable prefs");
    assert!(
        saved.contains("FieldOfView=120.00\r\n"),
        "saved FOV: {saved}"
    );
    assert!(
        saved.contains("LandscapeDrawDistance=Low\r\n"),
        "radius 5 saves its choice label"
    );
    std::fs::remove_dir_all(&dir).expect("remove the disposable preference directory");
}

/// Behaviour: chat.commands.a-verb-the-client-knows-answers-it-and-a-word-it-does-not-know-is-passed-on
///
/// The day command ignores argc/argv, toggles the landscape's always-daylight state, prints one
/// type-`0x1A` line to the command's current window, and mirrors the result to the retained player's
/// persistent-at-day option. That option is not auto-saved, so the change dirties the retained
/// module without sending an immediate `0x0005`.
///
/// A scene already near noon cannot prove the renderer half of that path. This one starts at
/// `0.02` on the shipped region's night ramp, where ambient `0.3999922` differs from the exact
/// noon sample `0.35`, and drives both spellings through the physical chat entry.
#[test]
fn typed_day_toggles_persisted_noon_lighting_from_a_real_night_scene() {
    const NIGHT: f32 = 0.02;
    const PERSISTENT_AT_DAY: u32 = 0x0000_0001;
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let store = std::sync::Arc::new(dereth_dat::testing::open_store_or_fail());
    app.load_world(
        &store,
        SceneConfig {
            land_radius: 0,
            scenery_radius: 0,
            character: false,
            time_of_day: Some(NIGHT),
            ..SceneConfig::default()
        },
    )
    .expect("the one-block night landscape loads");
    let before = install_player_module(&mut app);
    for _ in 0..3 {
        assert!(app.frame());
    }
    let lighting = |app: &App| {
        app.world_scene()
            .expect("the night landscape remains live")
            .landscape_lighting()
    };
    let night = lighting(&app);
    assert_eq!(
        before.options2 & PERSISTENT_AT_DAY,
        0,
        "the fixture starts at normal time"
    );
    assert!(
        (night.ambient_level - 0.399_992_2).abs() < 1e-4,
        "the command must be discriminated at night, measured {}",
        night.ambient_level
    );
    let unimplemented = app.interaction().stats.chat_commands_unimplemented;

    hand.submit(&mut app, "/day");
    assert!(
        app.interaction().last_sent.is_empty(),
        "PersistentAtDay is deferred, not 0x0005"
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        bubbles(&mut app)
            .iter()
            .any(|line| line == "Let there be light!"),
        "the day command's exact current-window acknowledgement is visible"
    );
    assert_eq!(
        app.world_scene().map(|w| w.always_daylight()),
        Some(true),
        "the typed command sets daylight to 1"
    );
    let day = lighting(&app);
    assert!(
        (day.ambient_level - 0.35).abs() < 1e-4,
        "Always Day re-asks the shipped region at noon, measured {}",
        day.ambient_level
    );
    let saved = forced_saved_player_module(&mut app);
    assert_eq!(
        saved.options, before.options,
        "the first option word is untouched"
    );
    assert_eq!(
        saved.options2 & !PERSISTENT_AT_DAY,
        before.options2 & !PERSISTENT_AT_DAY,
        "every neighboring options2 bit survives"
    );
    assert_ne!(
        saved.options2 & PERSISTENT_AT_DAY,
        0,
        "the authoritative saved module sees the persisted day bit"
    );

    hand.submit(&mut app, "@day ignored");
    assert!(
        app.interaction().last_sent.is_empty(),
        "extra arguments remain local and are ignored"
    );
    for _ in 0..3 {
        assert!(app.frame());
    }
    assert!(
        bubbles(&mut app)
            .iter()
            .any(|line| line == "Normality has been restored."),
        "the second exact acknowledgement is visible"
    );
    assert_eq!(
        app.world_scene().map(|w| w.always_daylight()),
        Some(false),
        "the extra argument does not stop the native toggle"
    );
    let restored = lighting(&app);
    assert!(
        (restored.ambient_level - night.ambient_level).abs() < 1e-2,
        "the running clock returns to the night ramp: {} vs {}",
        restored.ambient_level,
        night.ambient_level
    );
    let saved = forced_saved_player_module(&mut app);
    assert_eq!(saved.options, before.options);
    assert_eq!(
        saved.options2, before.options2,
        "the persisted bit toggles back without collateral"
    );
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        unimplemented
    );
    assert!(app.interaction().pending_requests().is_empty());
    app.shutdown();
}

/// Behaviour: chat.commands.a-verb-the-client-knows-answers-it-and-a-word-it-does-not-know-is-passed-on
///
/// `@framerate` reaches the shipped smart-box meter.
///
/// The frame-rate command toggles one process flag and synchronously raises
/// a set-frame-rate-display notice. The receiving smart box resolves child `0x10000047`, writes
/// `ID_SmartBox_FPS` with the renderer's current FPS/degrade values, and shows it; a second
/// command hides it. Arguments print the type-`0x1A` refusal without changing the flag.
#[test]
fn typed_framerate_toggles_the_shipped_localized_meter_without_a_request() {
    let _gpu = gpu_lock();
    let mut app = app();
    let mut hand = Hand::new();
    let store = std::sync::Arc::new(dereth_dat::testing::open_store_or_fail());
    app.load_world(
        &store,
        SceneConfig {
            land_radius: 0,
            scenery_radius: 0,
            character: false,
            ..SceneConfig::default()
        },
    )
    .expect("the one-block scene supplies the real frame-rate/degrade globals");
    for _ in 0..3 {
        assert!(app.frame());
    }

    let meter = gameplay_element(&app, dereth_ui_screens::hud::world_view::FPS_DISPLAY);
    assert!(
        !app.ui().expect("shell").ui.is_visible(meter),
        "the authored meter starts hidden"
    );

    let table = {
        let table =
            dereth_ui_screens::env::did_by_enum(&app.ui().expect("shell").ui, 4, 0x1000_0001)
                .expect("the shipped DidMapper resolves the smart-box string table");
        assert_eq!(
            table,
            dereth_primitives::DataId(0x2300_0001),
            "group 4 / enum 0x10000001"
        );
        let bytes = store
            .read(table)
            .expect("the mapped smart-box string table is present");
        let decoded = dereth_assets::ui::StringTable::decode_payload(table, &bytes)
            .expect("the mapped smart-box string table decodes");
        let row = decoded
            .strings
            .iter()
            .find(|(id, _)| *id == dereth_primitives::num::hash::str_hash(b"ID_SmartBox_FPS"))
            .map(|(_, row)| row)
            .expect("ID_SmartBox_FPS is in the mapped table");
        assert_eq!(
            row.variables,
            vec![28019, 27319],
            "the unnamed shipped row retains its two authored variable slots in native add order"
        );
        table
    };

    hand.submit(&mut app, "/framerate");
    assert!(
        app.ui().expect("shell").ui.is_visible(meter),
        "the typed toggle did not show the shipped FPS child"
    );
    // The receiver's next ordinary timed update samples the completed renderer frame before this
    // explicit frame later pushes another delta. Observe that same boundary instead of comparing
    // against the older value from before `Hand::submit` advanced its input frames.
    let (fps, deg) = {
        let scene = app.world_scene().expect("the real scene is installed");
        let globals = scene.degrade_globals();
        (
            scene.draw.degrade.frame_rate.fps(),
            if globals.auto_update_deg_mul {
                globals.deg_mul
            } else {
                globals.user_bias
            },
        )
    };
    let expected = app
        .ui()
        .expect("shell")
        .ui
        .resolve_string_rendered(
            table,
            dereth_primitives::num::hash::str_hash(b"ID_SmartBox_FPS"),
            &[format!("{fps:.2}"), format!("{deg:.2}")],
        )
        .expect("the native FPS row resolves with both float variables");
    assert!(app.frame(), "drive the meter's ordinary timed update");
    let shell = app.ui_mut().expect("shell");
    assert_eq!(
        shell
            .ui
            .text_element_mut(meter)
            .expect("the shipped FPS child is a text element")
            .glyphs
            .inq_text(false),
        expected,
        "the receiver did not render the native string-table row from the live previous frame"
    );
    assert!(
        app.interaction().last_sent.is_empty(),
        "@framerate is local, not a game request"
    );

    let refused_before = app.interaction().stats.chat_commands_refused;
    hand.submit(&mut app, "/framerate extra");
    assert!(
        app.ui().expect("shell").ui.is_visible(meter),
        "invalid arguments changed the toggle"
    );
    assert_eq!(
        app.interaction().stats.chat_commands_refused,
        refused_before + 1
    );
    for _ in 0..3 {
        assert!(
            app.frame(),
            "deliver the command-frame refusal through the ordinary HUD drain"
        );
    }
    assert!(
        bubbles(&mut app)
            .iter()
            .any(|line| line == "Unexpected arguments to @framerate"),
        "the argc refusal did not reach native chat type 0x1A"
    );
    assert!(
        app.interaction().last_sent.is_empty(),
        "the refusal emitted a game request"
    );

    hand.submit(&mut app, "/framerate");
    assert!(
        !app.ui().expect("shell").ui.is_visible(meter),
        "the second toggle did not hide it"
    );
    assert!(
        app.interaction().last_sent.is_empty(),
        "the hide toggle emitted a game request"
    );

    hand.submit(&mut app, "/framerate");
    assert!(
        app.ui().expect("shell").ui.is_visible(meter),
        "the third toggle did not show it"
    );
    app.queue_ui_mode(mode::GAME_PLAY);
    assert!(
        app.frame(),
        "replace the gameplay framework as a retail mode change does"
    );
    let replacement = gameplay_element(&app, dereth_ui_screens::hud::world_view::FPS_DISPLAY);
    assert_ne!(
        replacement, meter,
        "the control needs a newly constructed world controller receiver"
    );
    assert!(
        !app.ui().expect("shell").ui.is_visible(replacement),
        "initial panel setup replayed the process flag even though native only registers the notice"
    );
    hand.submit(&mut app, "/framerate");
    assert!(
        !app.ui().expect("shell").ui.is_visible(replacement),
        "the persisted true process flag did not toggle false on the replacement screen"
    );
    hand.submit(&mut app, "/framerate");
    assert!(
        app.ui().expect("shell").ui.is_visible(replacement),
        "the following false-to-true notice did not bind the replacement receiver"
    );
    app.shutdown();
}
