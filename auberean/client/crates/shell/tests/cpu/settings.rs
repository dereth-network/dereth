//! The settings' text.

use auberean_scene::camera::AvatarInFirstPerson;
use auberean_shell::settings::{
    held_to_picture, in_picture_pixels, on_picture, on_window, Presentation,
};
use auberean_shell::Settings;

/// The camera, mouse and avatar options are read by the client's own names, each within its range
/// (a value out of range keeps the default); the text written back keeps every line it did not
/// set, and adds what is missing to its section. The avatar's showing in first person is one of
/// its choices by name or number, during combat by default.
///
/// Behaviour: desktop.settings.options-read-by-the-clients-names-and-kept
#[test]
fn options_read_by_the_clients_names_and_kept() {
    let text = "[Camera]\nCameraDistance=6\nCameraTilt=3.0\nCameraZoomMultiplier=2\n\
                CameraAlignToSlope=0\nCameraTransitionTime=0.5\nCameraHeight=5\n\
                [Input]\nMouseInvertY=1\nMouseLookSensitivity=1.5\n[Sound]\nVolume=7\n";
    let s = Settings::read(text);
    assert_eq!(s.camera.distance, 6.0);
    assert_eq!(s.camera.tilt, 0.25, "3.0 is out of range");
    assert_eq!(s.camera.zoom_speed, 2.0);
    assert!(s.camera.invert_y && s.camera.mouse_look_steers);
    assert_eq!(s.camera.mouse_sensitivity, 1.5);
    assert!(!s.camera.align_to_slope && s.camera.auto_distance);
    assert_eq!(s.camera.transition_time, 0.5);
    assert_eq!(s.camera.height, 0.0, "5 is out of range");
    assert_eq!(
        s.camera.avatar_in_first_person,
        AvatarInFirstPerson::DuringCombat
    );
    let avatar = |v: &str| {
        Settings::read(&format!("[Avatar]\nAvatarVisibleInFirstPerson={v}\n"))
            .camera
            .avatar_in_first_person
    };
    assert_eq!(avatar("Always"), AvatarInFirstPerson::Always);
    assert_eq!(avatar("never"), AvatarInFirstPerson::Never);
    assert_eq!(avatar("0"), AvatarInFirstPerson::Never);
    assert_eq!(avatar("Sometimes"), AvatarInFirstPerson::DuringCombat);
    let mut s = s;
    s.camera.distance = 3.5;
    s.window = (800, 600);
    let out = s.write();
    assert!(out.contains("CameraDistance=3.500000"), "{out}");
    assert!(out.contains("[Sound]\nVolume=7"), "{out}");
    assert!(
        out.contains("[Rendering]\r\nDisplayResolution=52429400\r\nFullScreen=0"),
        "{out}"
    );
    assert!(
        out.contains("[Avatar]\r\nAvatarVisibleInFirstPerson=DuringCombat"),
        "{out}"
    );
    let again = Settings::read(&out);
    assert_eq!(again.camera.distance, 3.5);
    assert_eq!(again.window, (800, 600));
    assert_eq!(Settings::read(&again.write()), again, "stable");
}

/// The brightness the screen takes is the preference kept; with none kept the picture stands as it
/// is.
///
/// Behaviour: desktop.settings.the-screen-brightness-is-the-preference-kept
#[test]
fn the_screen_brightness_is_the_preference_kept() {
    assert_eq!(Settings::read("").screen_brightness(), 0.0, "none kept");
    assert_eq!(
        Settings::read("[Graphics]\nScreenBrightness=0.25\n").screen_brightness(),
        0.25
    );
}

/// The graphics preferences are read from their own section by the client's names: a choice by the
/// name of its level or by its number, a yes-or-no as 1 or 0, a number within its range; settings
/// without the section hold none. They are written back the same way, keeping the lines the
/// section had.
///
/// Behaviour: desktop.settings.graphics-preferences-read-by-the-clients-names-and-kept
#[test]
fn graphics_preferences_read_by_the_clients_names_and_kept() {
    use auberean_draw::quality::key;
    assert_eq!(
        Settings::read("[Camera]\nCameraDistance=6\n").graphics,
        None
    );
    let text = "[Graphics]\nOverallGraphicsQuality=0\nModelDetail=High\n\
                LandscapeDrawDistance=low\nLandscapeFrillDistance=3\nLandscapeCloudShadows=1\n\
                WaterReflections=Sometimes\nScreenBrightness=0.451429\nAdaptiveDegradeBias=7\n";
    let s = Settings::read(text);
    let g = s.graphics.clone().expect("the graphics section");
    assert_eq!(g.get(key::MODEL_DETAIL), Some(2.0));
    assert_eq!(
        g.get(key::LANDSCAPE_DRAW_DISTANCE),
        Some(1.0),
        "by name, any case"
    );
    assert_eq!(g.get(key::LANDSCAPE_FRILL_DISTANCE), Some(3.0), "by number");
    assert_eq!(g.get(key::LANDSCAPE_CLOUD_SHADOWS), Some(1.0));
    assert_eq!(
        g.get(key::WATER_REFLECTIONS),
        g.default_of(key::WATER_REFLECTIONS),
        "not a level: the default"
    );
    assert_eq!(g.get(key::SCREEN_BRIGHTNESS), Some(0.451_429));
    assert_eq!(
        g.get(key::ADAPTIVE_DEGRADE_BIAS),
        g.default_of(key::ADAPTIVE_DEGRADE_BIAS),
        "out of its range: the default"
    );
    let out = s.write();
    assert!(out.contains("OverallGraphicsQuality=0"), "{out}");
    assert!(out.contains("LandscapeDrawDistance=Low"), "{out}");
    assert!(out.contains("LandscapeFrillDistance=High"), "{out}");
    assert!(out.contains("LandscapeCloudShadows=1"), "{out}");
    assert!(out.contains("ScreenBrightness=0.451429"), "{out}");
    let again = Settings::read(&out);
    assert_eq!(again.graphics, s.graphics);
    assert_eq!(Settings::read(&again.write()), again, "stable");
}

/// A fresh install, with no preferences kept, starts at overall quality 4, as the configuration
/// program the original runs first offers on any machine this client draws on and the player
/// keeps: high texture, material and model detail, high reflections, volumetric shadows, the high
/// landscape distance, low frills, no cloud shadows; the screen's brightness at its default.
/// A kept overall quality (1 to 5) sets its own preferences over those read beside it; 0 keeps
/// those read. The quality the preferences match is written back.
///
/// Behaviour: desktop.settings.a-fresh-install-starts-at-the-quality-the-configuration-offers
#[test]
fn a_fresh_install_starts_at_the_quality_the_configuration_offers() {
    use auberean_draw::quality::{key, FIRST_RUN};
    let fresh = Settings::read("");
    assert_eq!(fresh.graphics, None, "nothing kept");
    let g = fresh.starting_graphics();
    assert_eq!(g.overall(), FIRST_RUN);
    assert_eq!(FIRST_RUN, 4);
    for (k, v) in [
        (key::TEXTURE_DETAIL, 3.0),
        (key::MATERIAL_DETAIL, 2.0),
        (key::MODEL_DETAIL, 2.0),
        (key::WATER_REFLECTIONS, 2.0),
        (key::DYNAMIC_OBJECT_SHADOWS, 2.0),
        (key::LANDSCAPE_DRAW_DISTANCE, 3.0),
        (key::LANDSCAPE_FRILL_DISTANCE, 1.0),
        (key::LANDSCAPE_CLOUD_SHADOWS, 0.0),
        (key::ANISOTROPIC_FILTERING, 0.0),
        (key::ADAPTIVE_DEGRADE, 0.0),
        (key::SCREEN_BRIGHTNESS, 0.0),
    ] {
        assert_eq!(g.get(k), Some(v), "{k:#x}");
    }
    let kept = Settings::read("[Graphics]\nOverallGraphicsQuality=2\nTextureDetail=VeryHigh\n");
    let g = kept.starting_graphics();
    assert_eq!(g.overall(), 2, "the kept quality's");
    assert_eq!(
        g.get(key::TEXTURE_DETAIL),
        Some(2.0),
        "over the value beside it"
    );
    let own = Settings::read("[Graphics]\nOverallGraphicsQuality=0\nTextureDetail=VeryHigh\n");
    assert_eq!(own.starting_graphics().get(key::TEXTURE_DETAIL), Some(4.0));
    assert_eq!(own.starting_graphics().overall(), 0);
    let mut s = Settings::read("[Graphics]\nOverallGraphicsQuality=0\n");
    s.graphics = Some(fresh.starting_graphics());
    assert!(
        s.write().contains("OverallGraphicsQuality=4"),
        "{}",
        s.write()
    );
}

/// The display's size is kept as the client keeps it, its width above its height in one number
/// (`DisplayResolution`), with full screen beside it; an earlier version's two numbers are still
/// read, the client's own number taking their place. A yes-or-no reads only as 1 or 0 and a
/// number only when the whole text is one; numbers are written with six places, as the client
/// writes them.
///
/// Behaviour: desktop.settings.the-display-and-values-read-as-the-client-writes-them
#[test]
fn the_display_and_values_read_as_the_client_writes_them() {
    let s = Settings::read("[Rendering]\nDisplayResolution=125830152\nFullScreen=1\n");
    assert_eq!(s.window, (1920, 1032));
    assert!(s.full_screen);
    let old = Settings::read("[Display]\nWindowWidth=800\nWindowHeight=600\n");
    assert_eq!(old.window, (800, 600));
    assert!(!old.full_screen);
    let both = Settings::read(
        "[Display]\nWindowWidth=800\nWindowHeight=600\n[Rendering]\nDisplayResolution=83887104\n",
    );
    assert_eq!(both.window, (1280, 1024));
    let tiny = Settings::read("[Rendering]\nDisplayResolution=65537\n");
    assert_eq!(
        tiny.window,
        Settings::default().window,
        "one by one is no size"
    );
    let flags = Settings::read("[Input]\nMouseInvertY=true\nMouseLookSteersPlayer=0\n");
    assert!(!flags.camera.invert_y, "true is not a yes");
    assert!(!flags.camera.mouse_look_steers);
    let numbers = Settings::read("[Camera]\nCameraDistance=6m\nCameraTilt= 0.5\n");
    assert_eq!(numbers.camera.distance, Settings::default().camera.distance);
    assert_eq!(numbers.camera.tilt, 0.5);
    let mut w = Settings::default();
    w.full_screen = true;
    w.camera.mouse_sensitivity = 1.25;
    let mut tie = w.clone();
    tie.camera.tilt = 0.007_812_5;
    assert!(
        tie.write().contains("CameraTilt=0.007813"),
        "a tie rounds up, as `%f` rounds"
    );
    let out = w.write();
    assert!(out.contains("MouseLookSensitivity=1.250000"), "{out}");
    assert!(
        out.contains("DisplayResolution=83887040\r\nFullScreen=1"),
        "{out}"
    );
    let back = Settings::read(&out);
    assert_eq!(
        (back.window, back.full_screen, back.camera),
        (w.window, w.full_screen, w.camera)
    );
}

/// The avatar's three looks are read from its section by the client's names (each on unless the
/// file says 0) and written back: looking around with the camera, looking back at a camera in
/// front, and looking at each new selection.
///
/// Behaviour: desktop.settings.the-avatars-looks-read-by-the-clients-names
#[test]
fn the_avatars_looks_read_by_the_clients_names() {
    let on = Settings::default().camera;
    assert!(on.look_around && on.look_at_camera && on.look_at_selection);
    let s = Settings::read(
        "[Avatar]\nAvatarLookAround=1\nAvatarLookAtCamera=0\nAvatarLookAtSelection=0\n",
    );
    assert!(s.camera.look_around);
    assert!(!s.camera.look_at_camera && !s.camera.look_at_selection);
    let mut w = Settings::default();
    w.camera.look_around = false;
    let out = w.write();
    assert!(
        out.contains("AvatarLookAround=0\r\nAvatarLookAtCamera=1\r\nAvatarLookAtSelection=1"),
        "{out}"
    );
    assert_eq!(Settings::read(&out).camera, w.camera);
}

/// The rendering options are read by the client's own names, and written back by them: a first
/// start waits for the display's refresh and smooths its edges, with no third buffer.
///
/// Behaviour: desktop.settings.the-rendering-options-read-by-the-clients-names-and-kept
#[test]
fn the_rendering_options_read_by_the_clients_names_and_kept() {
    let first = Settings::default().presentation();
    assert!(first.sync_to_display && first.antialiasing && !first.triple_buffering);
    assert!(!first.full_screen);
    let s = Settings::read(
        "[Rendering]\nTripleBuffering=1\nSyncToDisplayRefresh=0\nAntialiasing=0\nFullScreen=1\n",
    );
    let p = s.presentation();
    assert!(p.triple_buffering && !p.sync_to_display && !p.antialiasing && p.full_screen);
    let mut s = s;
    let mut q = p;
    q.triple_buffering = false;
    q.sync_to_display = true;
    q.size = (800, 600);
    s.set_presentation(&q);
    assert_eq!(s.window, (800, 600));
    let text = s.write();
    assert!(text.contains("TripleBuffering=0"), "{text}");
    assert!(text.contains("SyncToDisplayRefresh=1"), "{text}");
    assert!(text.contains("Antialiasing=0"), "{text}");
    assert_eq!(Settings::read(&text).presentation(), q);
}

/// Full screen never changes the monitor's display mode: the window stays the monitor's size, the
/// picture is drawn at the size asked for and shows at its own shape in the middle, black bars
/// around it, and the pointer lands on the same place in the picture. A window's picture is the
/// window's size, whatever was asked for.
///
/// Behaviour: desktop.settings.full-screen-keeps-the-pictures-shape-at-the-size-asked-for
#[test]
fn full_screen_keeps_the_pictures_shape_at_the_size_asked_for() {
    let monitor = (3840, 2160);
    let asked = (1024, 768);
    let mut p = Presentation {
        size: asked,
        full_screen: true,
        triple_buffering: false,
        sync_to_display: true,
        antialiasing: true,
    };
    assert_eq!(p.drawn_in(monitor), asked);
    // 4:3 on a 16:9 monitor: bars of 480 at each side.
    let shown = p.shown_in(asked, monitor);
    assert_eq!(shown, [480.0, 0.0, 2880.0, 2160.0]);
    assert_eq!(
        on_picture((1920.0, 540.0), shown, asked),
        (512.0, 192.0),
        "the monitor's point on the picture"
    );
    assert_eq!(
        on_window((512.0, 192.0), shown, asked),
        (1920.0, 540.0),
        "and back"
    );
    assert_eq!(
        on_picture((480.0, 0.0), shown, asked),
        (0.0, 0.0),
        "the picture's corner is where it shows"
    );
    let on_bar = on_picture((100.0, 540.0), shown, asked);
    assert_eq!(
        held_to_picture(on_bar, asked),
        ((0.0, 192.0), false),
        "a point on a bar is off the picture, and held to its edge"
    );
    assert_eq!(
        held_to_picture((512.0, 192.0), asked),
        ((512.0, 192.0), true),
        "a point on the picture stays"
    );
    assert_eq!(
        held_to_picture((1024.0, 768.0), asked),
        ((1023.0, 767.0), false),
        "the picture ends before its size"
    );
    // A shape wider than the monitor's: bars at the top and the foot.
    assert_eq!(
        p.shown_in((1920, 800), (1920, 1080)),
        [0.0, 140.0, 1920.0, 800.0]
    );
    // The monitor's own shape: no bars.
    assert_eq!(
        p.shown_in((1920, 1080), monitor),
        [0.0, 0.0, 3840.0, 2160.0]
    );
    // A window's picture is the window's size, and fills it.
    p.full_screen = false;
    assert_eq!(p.drawn_in((1280, 960)), (1280, 960));
    let shown = p.shown_in((1280, 960), (1280, 960));
    assert_eq!(shown, [0.0, 0.0, 1280.0, 960.0]);
    assert_eq!(
        on_picture((7.5, 9.5), shown, (1280, 960)),
        (7.5, 9.5),
        "a window's point is the picture's"
    );
}

/// Behaviour: desktop.settings.a-mac-window-counts-its-size-in-points
#[test]
fn a_mac_window_counts_its_size_in_points() {
    assert_eq!(in_picture_pixels((2560, 1920), 2.0), (1280, 960));
    assert_eq!(in_picture_pixels((1280, 960), 1.0), (1280, 960));
    assert_eq!(in_picture_pixels((1, 1), 2.0), (1, 1), "never no picture");
    let (window, picture) = ((2560, 1920), (1280, 960));
    let shown = Presentation {
        size: picture,
        full_screen: false,
        triple_buffering: false,
        sync_to_display: true,
        antialiasing: true,
    }
    .shown_in(picture, window);
    assert_eq!(
        on_picture((2000.0, 1000.0), shown, picture),
        (1000.0, 500.0),
        "the pointer lands on the same place in the picture"
    );
}

/// The land's blending is on unless the file says otherwise, and the command line's say goes over
/// the file's.
///
/// Behaviour: desktop.settings.the-lands-blending-is-read-from-the-file-and-the-line
#[test]
fn the_lands_blending_is_read_from_the_file_and_the_line() {
    use auberean_shell::command_line::{parse_command_line, ClientLine};
    assert!(Settings::read("").terrain_blending);
    let off = "[Graphics]\nLandscapeTerrainBlending=0\n";
    assert!(!Settings::read(off).terrain_blending);
    let on_line = |words: &str, text: &str| {
        let mut line = ClientLine::default();
        let said = format!("ac2client.exe -h host -a acct {words}");
        assert!(parse_command_line(&mut line, &said, true), "{said}");
        let mut s = Settings::read(text);
        line.apply_to(&mut s);
        s
    };
    assert!(on_line("-lb", off).terrain_blending, "the line turns it on");
    assert!(!on_line("-nolb", "").terrain_blending, "and off");
    assert!(
        !on_line("", off).terrain_blending,
        "saying nothing leaves it"
    );
}
