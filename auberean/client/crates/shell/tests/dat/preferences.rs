//! The preferences' names in the data.

use auberean_shell::preferences::{key, section, Names, Preferences};
use auberean_shell::profile::Profile;

/// The data names the preferences' sections and their keys in two enumerations, the names the
/// preferences file is written with: the camera's section `Camera` and its `CameraDistance`, the
/// key maps' `KeyMap` and `Normal`, the network's `Net` and `UserSpecifiedPort`, `International`
/// and `Language`; the keys' map builds on another, and one of its own names the friends
/// service's local port. A preference asked for by its numbers reads the file by those names.
///
/// Behaviour: desktop.preferences.named-by-the-datas-two-enumerations
#[test]
fn named_by_the_datas_two_enumerations() {
    let records = &crate::session::data().0.records;
    let names = Names::read(records);
    for (s, k, sn, kn) in [
        (0x4000_0008, 0x4000_002A, "Camera", "CameraDistance"),
        (section::KEY_MAP, 0x4000_0030, "KeyMap", "Normal"),
        (
            section::NET,
            key::USER_SPECIFIED_PORT,
            "Net",
            "UserSpecifiedPort",
        ),
        (
            section::NET,
            key::COMPUTE_UNIQUE_PORT,
            "Net",
            "ComputeUniquePort",
        ),
        (section::NET, key::BIND_INTERFACE, "Net", "BindInterface"),
        (
            section::INTERNATIONAL,
            key::LANGUAGE,
            "International",
            "Language",
        ),
    ] {
        assert_eq!(names.of(s, k), Some((sn, kn)));
    }
    assert_eq!(names.keys.name(0x4100_0001), Some("GunLocalPort"));
    assert_eq!(names.keys.value("cameradistance"), Some(0x4000_002A));
    assert_eq!(names.sections.name(0x4000_0003), None);
    let p = Preferences::startup(
        Profile::new("[Camera]\r\nCameraDistance=4.500000\r\n[Net]\r\nUserSpecifiedPort=9000\r\n"),
        names,
    );
    assert_eq!(
        p.inq_string(0x4000_0008, 0x4000_002A).as_deref(),
        Some("4.500000")
    );
    assert_eq!(
        p.inq_uint32(section::NET, key::USER_SPECIFIED_PORT),
        Some(9000)
    );
}

/// The graphics preferences the September 2013 client asks at startup are named in the data as
/// this client's settings name them: the overall quality and the land's blending in `Graphics`,
/// so the settings read the same values from the same file, and the line's `-lb` and `-nolb` go
/// over the file's in both. The others it asks there (curved surfaces, drawing into textures, a
/// screenshot's tiles) have names of their own and nothing here reads them.
///
/// Behaviour: desktop.preferences.graphics-at-startup-are-the-settings
#[test]
fn graphics_at_startup_are_the_settings() {
    use auberean_shell::command_line::ClientLine;
    use auberean_shell::settings::Settings;
    let names = Names::read(&crate::session::data().0.records);
    for (k, kn) in [
        (key::OVERALL_GRAPHICS_QUALITY, "OverallGraphicsQuality"),
        (key::TERRAIN_BLENDING, "LandscapeTerrainBlending"),
        (key::USE_N_PATCHES, "UseNPatches"),
        (key::USE_TEXTURE_TARGETS, "TextureTargets"),
        (key::SCREENSHOT_WIDTH, "ScreenshotWidthMultiply"),
        (key::SCREENSHOT_HEIGHT, "ScreenshotHeightMultiply"),
    ] {
        assert_eq!(
            names.of(section::GRAPHICS, k),
            Some(("Graphics", kn)),
            "{k:#x}"
        );
    }
    let file = "[Graphics]\r\nOverallGraphicsQuality=2\r\nLandscapeTerrainBlending=1\r\n";
    let (line, ok) = ClientLine::parse(&["client", "-a", "kara", "-h", "here", "-nolb"]);
    assert!(ok);
    let prefs = line.preferences(Profile::new(file), names);
    let mut settings = Settings::read(file);
    line.apply_to(&mut settings);
    assert_eq!(
        prefs.inq_uint32(section::GRAPHICS, key::OVERALL_GRAPHICS_QUALITY),
        settings.graphics.as_ref().map(|g| g.overall())
    );
    assert_eq!(
        prefs.inq_bool(section::GRAPHICS, key::TERRAIN_BLENDING),
        Some(settings.terrain_blending)
    );
    assert!(!settings.terrain_blending);
}
