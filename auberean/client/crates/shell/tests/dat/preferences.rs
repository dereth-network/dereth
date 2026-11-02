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
