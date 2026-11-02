//! The client's preferences behind the options window's client page, read from a preferences file
//! by the data's names.

use auberean_interface::preferences::Preferences;
use auberean_shell::preferences::{EnumNames, KEY_NAMES};
use auberean_shell::Settings;
use auberean_ui_game::options::client::{section, Request};
use auberean_ui_game::options::helper::Change;

const DISTANCE: u32 = 0x4000_002A;
const INVERT_Y: u32 = 0x4000_0026;
const TEXTURE_DETAIL: u32 = 0x4000_004F;

/// At startup the camera puts in its preferences (eight in the camera section, three for the
/// mouse, three for the avatar), each read from the preferences file by the data's names, the
/// rest at the camera's defaults; the client page lists them by section. A distance tried and
/// applied goes to the camera with the other options as the file had them, and keeping writes
/// every preference by its names: the camera's, the graphics (a choice by its name in the data)
/// and the key maps' files.
///
/// Behaviour: ui.options.the-client-page-lists-the-cameras-preferences-and-keeps-them
#[test]
fn the_client_page_lists_the_cameras_preferences_and_keeps_them() {
    let records = &crate::screens::data().0.records;
    let settings = Settings::read(
        "[Camera]\r\nCameraDistance=6.000000\r\n[Input]\r\nMouseInvertY=1\r\n\
         [Graphics]\r\nOverallGraphicsQuality=0\r\nTextureDetail=Medium\r\n",
    );
    let mut p = Preferences::default();
    p.start(&settings, records);
    p.set(&settings.starting_graphics());
    p.handle(&Request::Attach(vec![
        section::RENDERING,
        section::GRAPHICS,
        section::CAMERA,
        section::AVATARS,
        section::INPUT,
    ]));
    let l = p.listing().expect("listed").clone();
    let count = |s: u32| {
        l.sections
            .iter()
            .find(|(x, _)| *x == s)
            .map_or(0, |(_, p)| p.len())
    };
    assert_eq!(
        [
            section::RENDERING,
            section::CAMERA,
            section::INPUT,
            section::AVATARS
        ]
        .map(count),
        [0, 8, 3, 3]
    );
    let find = |k: u32| {
        l.sections
            .iter()
            .flat_map(|(_, p)| p.iter())
            .find(|c| match c {
                Change::Flag(f) => f.description.key == k,
                Change::Number(n) => n.description.key == k,
                Change::Choice(c) => c.description.key == k,
            })
            .cloned()
            .expect("listed")
    };
    let Change::Number(mut distance) = find(DISTANCE) else {
        panic!("a number")
    };
    assert_eq!(distance.get(), 6.0, "from the file");
    assert_eq!(distance.range(), (-0.2, 10.0));
    let Change::Flag(invert) = find(INVERT_Y) else {
        panic!("a yes-or-no")
    };
    assert!(invert.get());
    distance.set_value(3.0);
    p.handle(&Request::Change(Change::Number(distance)));
    p.handle(&Request::Apply(section::CAMERA));
    let o = p.take_camera().expect("the camera told");
    assert_eq!(o.distance, 3.0);
    assert!(o.invert_y, "as the file had it");
    assert!(p.take_camera().is_none(), "once");
    p.handle(&Request::Save);
    let kept = p.take_kept().expect("kept");
    let has = |s: &str, k: &str, v: &str| kept.iter().any(|w| w == &(s.into(), k.into(), v.into()));
    assert!(has("Camera", "CameraDistance", "3.000000"), "{kept:?}");
    assert!(has("Input", "MouseInvertY", "1"));
    assert!(has("Avatar", "AvatarLookAtSelection", "1"));
    let texture = EnumNames::read(records, KEY_NAMES)
        .name(TEXTURE_DETAIL)
        .map(str::to_owned)
        .expect("named in the data");
    assert!(has("Graphics", &texture, "Medium"), "a choice by its name");
    assert!(has("KeyMap", "Normal", "Main.keymap"));
    // The sound manager's eight, by their names in the data: the music slider full, the
    // features the hardware's.
    let name = |k: u32| {
        EnumNames::read(records, KEY_NAMES)
            .name(k)
            .map(str::to_owned)
            .expect("named in the data")
    };
    let music = name(auberean_audio::options::key::MUSIC_VOLUME);
    let features = name(auberean_audio::options::key::FEATURES);
    let sound: Vec<_> = kept.iter().filter(|w| w.0 == "Sound").collect();
    assert_eq!(sound.len(), 8, "{sound:?}");
    assert!(has("Sound", &music, "1.000000"), "{sound:?}");
    assert!(has("Sound", &features, "Hardware"), "{sound:?}");
    assert_eq!(kept.len(), 21 + 14 + 8 + 3);
}
