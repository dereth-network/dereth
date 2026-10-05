//! Texture filtering: a preview surface flagged sharp samples its minified levels sharper while
//! the filtering preference is below its highest setting, the App alone applies texture-preference
//! requests (a shell rebuild does not reload them), and the configured choice is read before the
//! first draw without the profile being written.
//! Fixture: a retail dat setup drawn through the production preview on a software device, and
//! synthetic viewports and preference profiles (not retail captures).
#![cfg(gpu)]

use dereth_primitives::{DataId, Vec3};
use dereth_render::{
    device::{DeviceConfig, Gpu},
    Viewport,
};
use std::sync::Arc;
use {dereth_scene::preview::PreviewSpace, dereth_world_data::anim_assets::DatAnimAssets};

#[test]
/// Behaviour: rendering.textures.the-sharp-flag-sharpens-minified-samples
fn actual_dat_preview_sharp_flag_changes_minified_samples() {
    let store = Arc::new(dereth_dat::testing::open_store().expect("pristine retail DATs"));
    let assets = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: 64,
            height: 64,
            ..Default::default()
        },
    )
    .expect("headless WARP");
    let mut preview = PreviewSpace::new(assets);
    assert_eq!(
        preview
            .add_object(&store, &mut gpu, DataId(0x0200_0001))
            .unwrap(),
        Some(0)
    );
    preview
        .mode
        .set_camera_position(Vec3::new(0.12, -2.4, 0.88));
    gpu.set_texture_filtering(0);
    let draw = |gpu: &mut Gpu, preview: &PreviewSpace| {
        gpu.begin_frame().unwrap();
        preview
            .draw(
                gpu,
                Viewport {
                    x: 8,
                    y: 8,
                    width: 48,
                    height: 48,
                },
                None,
                1.2,
            )
            .unwrap();
        gpu.end_frame().unwrap();
        gpu.capture().unwrap().bgra
    };
    let plain = draw(&mut gpu, &preview);
    assert!(
        plain.chunks_exact(4).any(|p| p[..3] != [0, 0, 0]),
        "real preview draws pixels"
    );
    assert_eq!(
        plain,
        draw(&mut gpu, &preview),
        "stationary preview has zero noise"
    );
    preview.mode.use_sharp_mode();
    let sharp = draw(&mut gpu, &preview);
    let changed = plain
        .chunks_exact(4)
        .zip(sharp.chunks_exact(4))
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    eprintln!("preview Sharp changed {changed}/4096 pixels");
    assert!(
        changed > 0,
        "preview rendering must apply -1.4 for Sharp && preference<2"
    );
    for pref in [0, 1, 2, 3] {
        gpu.set_texture_filtering(pref);
        preview.mode.use_sharp_mode = false;
        let plain = draw(&mut gpu, &preview);
        preview.mode.use_sharp_mode = true;
        let sharp = draw(&mut gpu, &preview);
        assert_eq!(
            plain == sharp,
            pref >= 2,
            "temporary Sharp guard preference{pref}"
        );
        preview.mode.use_sharp_mode = false;
        assert_eq!(
            plain,
            draw(&mut gpu, &preview),
            "following non-Sharp preview restores preference{pref}"
        );
    }
}

#[test]
fn app_owns_texture_preference_requests_and_does_not_reload_on_shell_rebuild() {
    use dereth_ui_screens::{options::store, PrefValue, UiRequest};
    use {dereth_client::app::App, dereth_client_runtime::config::Config};
    let cfg = Config {
        dat_dir: dereth_dat::testing::dat_dir(),
        headless: true,
        connect: false,
        sound: false,
        ui: true,
        width: 64,
        height: 64,
        preferences_file: std::path::PathBuf::new(),
        ..Default::default()
    };
    let mut app = App::new(cfg).expect("actual offline App");
    assert_eq!(
        app.renderer().texture_filtering(),
        0,
        "startup, before first draw"
    );
    app.start_shell().unwrap();
    assert_eq!(
        store::inq_value("Render.TextureFiltering"),
        Some(PrefValue::Int(0))
    );
    assert!(app.frame());
    for preference in [3, 2, 0, 1] {
        app.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                "Render.TextureFiltering",
                PrefValue::Int(preference),
            ));
        assert!(app.frame());
        assert_eq!(
            app.renderer().texture_filtering(),
            preference as u32,
            "actual frame request consumer"
        );
        assert_eq!(
            store::inq_value("Render.TextureFiltering"),
            Some(PrefValue::Int(preference))
        );
    }
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(UiRequest::SetPreference(
            "Render.TextureFiltering",
            PrefValue::Int(0),
        ));
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(UiRequest::SetPreference(
            "Render.TextureFiltering",
            PrefValue::Int(3),
        ));
    assert!(app.frame());
    assert_eq!(
        app.renderer().texture_filtering(),
        3,
        "ordered writes last one wins"
    );
    app.start_shell().unwrap();
    assert_eq!(
        app.renderer().texture_filtering(),
        3,
        "shell does not reload startup profile"
    );
    assert_eq!(
        store::inq_value("Render.TextureFiltering"),
        Some(PrefValue::Int(3))
    );
    let left = dereth_client_shell::render_prefs::apply_preference_requests(
        app.renderer_mut(),
        vec![
            UiRequest::SetPreference("Render.TextureFiltering", PrefValue::Bool(false)),
            UiRequest::SetPreference("Render.OtherOwner", PrefValue::Int(1)),
        ],
    );
    assert_eq!(left.len(), 2, "wrong type and unowned preference retained");
    assert_eq!(app.renderer().texture_filtering(), 3);
}

#[test]
/// Behaviour: rendering.textures.the-filtering-preference-is-read-before-the-first-draw
fn app_loads_the_configured_choice_name_before_first_draw_without_writing_the_profile() {
    use std::io::Write;
    use {dereth_client::app::App, dereth_client_runtime::config::Config};
    // Newly created synthetic OS-temp profile, never the owner's file or a repository fixture.
    let path = std::env::temp_dir().join(format!("dereth-render-prefs-{}.ini", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let cleanup = Cleanup(path.clone());
    let original = b"[Render]\r\nTextureFiltering=Anisotropic\r\n[Other]\r\nUnowned=untouched\r\n";
    file.write_all(original).unwrap();
    drop(file);
    let mut app = App::new(Config {
        dat_dir: dereth_dat::testing::dat_dir(),
        headless: true,
        connect: false,
        sound: false,
        ui: true,
        width: 64,
        height: 64,
        preferences_file: path.clone(),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        app.renderer().texture_filtering(),
        3,
        "configured profile before first frame"
    );
    assert_eq!(app.frames_drawn(), 0);
    app.start_shell().unwrap();
    assert_eq!(
        dereth_ui_screens::options::store::inq_value("Render.TextureFiltering"),
        Some(dereth_ui_screens::PrefValue::Int(3))
    );
    assert!(app.frame());
    assert_eq!(app.renderer().texture_filtering(), 3);
    assert_eq!(std::fs::read(&path).unwrap(), original, "read-only loader");
    drop(app);
    drop(cleanup);
}
