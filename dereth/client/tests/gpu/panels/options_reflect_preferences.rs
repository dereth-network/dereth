//! The options page reflects the preferences file: `App::start_shell` applies the selected
//! `UserPreferences.ini` over the compiled-in `registered_default` values that
//! `options::store::init` installs, so the option store the page reads holds the file's values.
//! A preference with a live owner (the renderer's texture filtering) still agrees with the live
//! value, a missing file leaves the registered defaults standing, and loading never writes the
//! file back. The observable is `options::store::inq_value_as(name, type)`, the same typed query
//! the page's value getter makes for each control.
//! Fixture: a headless `App` and `UiShell` over the retail `client_local_English.dat`, with a
//! freshly created profile in the OS temp directory (never a user's `UserPreferences.ini`).

// This suite drives a real `App` through either Vulkan or Windows D3D12.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_ui_screens::{
    options::{
        preferences::UI_PREFERENCES,
        store::{self, DataType},
    },
    PrefValue,
};
use {dereth_client::app::App, dereth_client_runtime::config::Config};

/// A newly created OS-temp profile, removed on drop. Never a repository fixture and never a
/// user's file.
struct Profile(std::path::PathBuf);

impl Profile {
    fn new(tag: &str, body: &str) -> Self {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!("dere-f67-{tag}-{}.ini", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .expect("a fresh temp profile");
        f.write_all(body.as_bytes()).unwrap();
        Self(path)
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn app_with(profile: &Profile) -> App {
    let mut app = App::new(Config {
        dat_dir: dereth_dat::testing::dat_dir(),
        headless: true,
        connect: false,
        sound: false,
        ui: true,
        width: 64,
        height: 64,
        preferences_file: profile.0.clone(),
        ..Default::default()
    })
    .expect("an offline App");
    app.start_shell().expect("UI shell starts");
    app
}

/// `UI_PREFERENCES`' own `registered_default` for a name — what the page shows when the file is
/// not applied.
fn registered_default(name: &str) -> PrefValue {
    UI_PREFERENCES
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("{name} is a registered UI preference"))
        .registered_default
        .clone()
        .into()
}

/// The page's typed store query, using the control's own data type without its fallback value.
fn as_page_reads(name: &str, want: DataType) -> Option<PrefValue> {
    store::inq_value_as(name, want)
}

// ================================================================================================
// The headline. A profile that disagrees with every registered default, and the page must agree
// with the profile.
// ================================================================================================

/// Behaviour: options.preferences.the-options-page-reads-the-preferences-file-not-the-defaults
///
/// **A profile that disagrees with every registered default is what the page reads.** The file
/// has all three volumes set and every sound category enabled; the registered defaults are the
/// opposite on the three `*Disabled` flags (`Bool(true)` — muted), so a client that ignores the
/// file shows three muted check boxes over a file that enables all three.
///
/// Values here are deliberately **not** the registered defaults for any of the seven rows, so a
/// pass cannot be an accident of agreement. Each assertion names both values.
///
/// Falsified by removing the `options::store::load` call from `App::start_shell`; the first
/// assertion then reports:
///
/// ```text
/// Sound.SoundDisabled: the page reads Some(Bool(true)) and the file says Bool(false)
///   (registered default Bool(true) -- the file was not applied)
/// ```
#[test]
fn the_option_store_the_page_reads_reflects_the_preferences_file_and_not_the_defaults() {
    // Every one of these differs from `registered_default`, asserted below before it is compared.
    // A section per category, matching the retail preference writer: a single `[Default]` section
    // of already-dotted keys is a shape the client cannot produce.
    let body = "[Sound]\r\n\
                SoundDisabled=False\r\n\
                AmbientSoundDisabled=False\r\n\
                InterfaceSoundDisabled=False\r\n\
                SoundVolume=0.375\r\n\
                AmbientSoundVolume=0.25\r\n\
                InterfaceSoundVolume=0.50\r\n\
                [Render]\r\n\
                FieldOfView=120.00\r\n\
                [Net]\r\n\
                ComputeUniquePort=True\r\n";
    let profile = Profile::new("sound", body);
    let _app = app_with(&profile);

    let expected: [(&str, DataType, PrefValue); 7] = [
        (
            "Sound.SoundDisabled",
            DataType::Bool,
            PrefValue::Bool(false),
        ),
        (
            "Sound.AmbientSoundDisabled",
            DataType::Bool,
            PrefValue::Bool(false),
        ),
        (
            "Sound.InterfaceSoundDisabled",
            DataType::Bool,
            PrefValue::Bool(false),
        ),
        (
            "Sound.SoundVolume",
            DataType::Float,
            PrefValue::Float(0.375),
        ),
        (
            "Sound.AmbientSoundVolume",
            DataType::Float,
            PrefValue::Float(0.25),
        ),
        (
            "Sound.InterfaceSoundVolume",
            DataType::Float,
            PrefValue::Float(0.5),
        ),
        (
            "Render.FieldOfView",
            DataType::Float,
            PrefValue::Float(120.0),
        ),
    ];
    for (name, ty, want) in &expected {
        let default = registered_default(name);
        assert_ne!(
            &default, want,
            "{name}: the file must disagree with the registered default or this row proves nothing"
        );
        let got = as_page_reads(name, *ty);
        assert_eq!(
            got.as_ref(),
            Some(want),
            "{name}: the page reads {got:?} and the file says {want:?} \
             (registered default {default:?} -- the file was not applied)"
        );
    }
}

/// The polarity of the sound flags, stated on its own because getting it backwards would make the
/// test above pass for the wrong reason.
///
/// `Sound.SoundDisabled` is a **disable** flag and the registered default is `true`. So a client
/// that ignores the file comes up *muted with the box ticked*, contradicting a file that turns
/// sound on.
#[test]
fn sound_disabled_defaults_to_muted_so_ignoring_the_file_could_only_look_wrong() {
    assert_eq!(
        registered_default("Sound.SoundDisabled"),
        PrefValue::Bool(true)
    );
    assert_eq!(
        registered_default("Sound.AmbientSoundDisabled"),
        PrefValue::Bool(true)
    );
    assert_eq!(
        registered_default("Sound.InterfaceSoundDisabled"),
        PrefValue::Bool(true)
    );
    // And all three volumes already default to full, so a file at full volume cannot tell a
    // loaded profile from an ignored one; only the three check boxes can.
    assert_eq!(
        registered_default("Sound.SoundVolume"),
        PrefValue::Float(1.0)
    );
    assert_eq!(
        registered_default("Sound.AmbientSoundVolume"),
        PrefValue::Float(1.0)
    );
    assert_eq!(
        registered_default("Sound.InterfaceSoundVolume"),
        PrefValue::Float(1.0)
    );
}

/// A preference with a **live owner** still wins over the file, because the live value is the truth
/// about the machine. `render_prefs::seed_ui_registry` runs after the load in `start_shell` for
/// exactly this reason, and the ordering is load-bearing in the other direction from the rest.
///
/// The file asks for `Anisotropic` (3) and gets it, because the renderer honoured the same file at
/// construction. This pins agreement between the page and renderer for that profile; since both
/// use the same value here, this test alone does not distinguish reversed load/seed ordering.
#[test]
fn the_live_renderer_value_survives_the_preferences_load() {
    let profile = Profile::new("texfilter", "[Render]\r\nTextureFiltering=Anisotropic\r\n");
    let app = app_with(&profile);
    assert_eq!(
        app.renderer().texture_filtering(),
        3,
        "the profile reached the renderer"
    );
    assert_eq!(
        as_page_reads("Render.TextureFiltering", DataType::UInt),
        Some(PrefValue::Int(3)),
        "the page agrees with the live renderer, not with the registered default 0"
    );
    assert_ne!(
        registered_default("Render.TextureFiltering"),
        PrefValue::Int(3)
    );
}

/// Client initialization ignores a preference-initialization failure, so a missing file is not
/// fatal and the registered defaults are then what the page shows.
#[test]
fn a_missing_profile_leaves_the_registered_defaults_standing() {
    let path = std::env::temp_dir().join(format!("dere-f67-absent-{}.ini", std::process::id()));
    let _ = std::fs::remove_file(&path);
    assert!(!path.exists());
    let mut app = App::new(Config {
        dat_dir: dereth_dat::testing::dat_dir(),
        headless: true,
        connect: false,
        sound: false,
        ui: true,
        width: 64,
        height: 64,
        preferences_file: path,
        ..Default::default()
    })
    .expect("an offline App");
    app.start_shell().expect("UI shell starts");
    assert_eq!(
        as_page_reads("Sound.SoundDisabled", DataType::Bool),
        Some(registered_default("Sound.SoundDisabled")),
    );
}

/// The read-only half: loading preferences must not rewrite the selected file. This test selects
/// only its temporary profile and checks its exact bytes after application initialization.
#[test]
fn the_loader_does_not_write_the_file_back() {
    let body = "[Sound]\r\nSoundVolume=0.375\r\n[Whatever]\r\nUnowned=untouched\r\n";
    let profile = Profile::new("readonly", body);
    let before = std::fs::read(&profile.0).unwrap();
    let _app = app_with(&profile);
    assert_eq!(
        std::fs::read(&profile.0).unwrap(),
        before,
        "byte-identical after a load"
    );
}
