//! The three `Camera.*` preferences in the player's profile (`AlignToSlope`, `Stiffness`,
//! `AdjustmentSpeed`) reach the running camera at start-up, and the options page's run-time writes
//! reach it too while a preference that is not the camera's is handed back untouched. Fixture: a
//! profile written in the test with non-default values, and a body attached to a scene on the
//! retail dats, on a software GPU device. Fails when the dats or the device are absent.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client_runtime::camera::CameraPreferences;
use dereth_scene::world_scene::SceneWrites;
use {dereth_client_runtime::config::Config, dereth_client_runtime::config::Preferences};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

/// The landblock the body is attached in, Holtburg; any block with a body would do.
const LANDBLOCK: u16 = 0xA9B4;

/// Behaviour: camera.preferences.reach-the-camera-manager
/// A profile with non-default `Camera.*` values lands in the running camera's
/// align-to-slope, adjustment-speed, and stiffness fields, including the stiffness the smoother
/// actually uses.
#[test]
fn the_profile_s_camera_preferences_reach_the_camera_manager() {
    let prefs = Preferences::parse(
        "[Camera]\r\nAdjustmentSpeed=12.50\r\nStiffness=0.80\r\nAlignToSlope=False\r\n",
    );
    let argv = ["--no-connect".to_string()];
    let cfg = Config::from_args_and_prefs_with(&argv, &prefs).expect("the profile parses");
    assert_eq!(
        cfg.camera,
        CameraPreferences {
            align_to_slope: false,
            stiffness: 0.8,
            adjustment_speed: 12.5
        },
        "Config::apply_preferences did not read the three Camera.* keys"
    );

    let mut gpu = crate::common::test_gpu(800, 600);
    let store = crate::common::dats();
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
    let scene_config = SceneConfig {
        landblock: LANDBLOCK,
        camera: cfg.camera,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, scene_config).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    {
        let cam = &scene.character.as_ref().expect("a body").camera;
        assert!(
            !cam.manager.align_camera_to_slope,
            "Camera.AlignToSlope=False did not reach the manager"
        );
        assert_eq!(
            cam.manager.camera_adjustment_speed, 12.5,
            "Camera.AdjustmentSpeed did not reach the manager"
        );
        assert_eq!(
            cam.manager.camera_stiffness, 0.8,
            "Camera.Stiffness did not reach the manager"
        );
        assert_eq!(
            cam.set.current_stiffness, 0.8,
            "the camera-set installation did not apply stiffness"
        );
        assert_eq!(
            cam.manager.t_stiffness, 0.8,
            "the smoother is not running at the profile's stiffness"
        );
        assert_eq!(cam.manager.r_stiffness, 0.8);
    }

    // The options page at run time: `SetPreference` for the three names, and a fourth name that
    // is not the camera's and must come back untouched, in order.
    use dereth_ui_screens::{PrefValue, UiRequest};
    let requests = vec![
        UiRequest::SetPreference("Camera.Stiffness".into(), PrefValue::Float(0.6)),
        UiRequest::SetPreference("Sound.SoundVolume".into(), PrefValue::Float(0.5)),
        UiRequest::SetPreference("camera.alignToSlope".into(), PrefValue::Bool(true)),
        UiRequest::SetPreference("Camera.AdjustmentSpeed".into(), PrefValue::Float(55.0)),
    ];
    let left = dereth_client_runtime::camera::apply_preference_requests(
        scene.character.as_mut(),
        requests,
    );
    assert_eq!(
        left.len(),
        1,
        "the sound request is not the camera's: {left:?}"
    );
    assert!(matches!(&left[0], UiRequest::SetPreference(n, _) if *n == "Sound.SoundVolume"));
    let cam = &scene.character.as_ref().expect("a body").camera;
    assert_eq!(cam.manager.camera_stiffness, 0.6);
    assert_eq!(
        cam.set.current_stiffness, 0.6,
        "the stiffness-change notice did not reach the camera set"
    );
    assert_eq!(cam.manager.t_stiffness, 0.6);
    assert!(
        cam.manager.align_camera_to_slope,
        "the run-time AlignToSlope write was dropped"
    );
    assert_eq!(cam.manager.camera_adjustment_speed, 55.0);
}
