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
/// mouse, three for the avatar after the drawing's two), each read from the preferences file by
/// the data's names, the rest at the defaults; the client page lists them by section. A distance tried and
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
        [0, 8, 3, 5]
    );
    let avatar: Vec<u32> = l
        .sections
        .iter()
        .find(|(x, _)| *x == section::AVATARS)
        .expect("the avatar's section")
        .1
        .iter()
        .map(|c| match c {
            Change::Flag(f) => f.description.key,
            Change::Number(n) => n.description.key,
            Change::Choice(c) => c.description.key,
        })
        .collect();
    assert_eq!(
        avatar,
        [
            0x4000_006C,
            0x4000_006D,
            0x4000_0068,
            0x4000_0069,
            0x4000_006A
        ],
        "first person and the pointer's see-through first, then the looks"
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
    let misc: Vec<_> = kept.iter().filter(|w| w.0 == "Misc").collect();
    assert_eq!(misc.len(), 2, "{misc:?}");
    assert_eq!(kept.len(), 21 + 16 + 8 + 3 + 3 + 2);
}

/// The command line's gore level and blood color stand over the language's: the effects start at
/// them, and a level above them is not offered.
///
/// Behaviour: ui.options.the-command-line-forces-the-top-gore-and-blood
#[test]
fn the_command_line_forces_the_top_gore_and_blood() {
    use auberean_draw::effects::Channels;
    const GORE: u32 = 0x4000_0035;
    let records = &crate::screens::data().0.records;
    let settings = Settings::read(
        "[Graphics]
OverallGraphicsQuality=4
",
    );
    let mut p = Preferences::default();
    p.force_tops(2, 1);
    p.start(&settings, records);
    assert_eq!(
        p.take_channels().expect("told as read"),
        Channels {
            gore: 2,
            blood: 1,
            foot_step_particles: true
        }
    );
    p.handle(&Request::Attach(vec![section::EFFECTS]));
    let l = p.listing().expect("listed").clone();
    let prefs = &l
        .sections
        .iter()
        .find(|(s, _)| *s == section::EFFECTS)
        .expect("the section")
        .1;
    let offered = prefs
        .iter()
        .find_map(|c| match c {
            Change::Choice(c) if c.description.key == GORE => Some(c.list_choices().len()),
            _ => None,
        })
        .expect("the gore choice");
    assert!(
        offered < 4,
        "what is above the forced level goes: {offered}"
    );
}

/// The effects section lists how much gore, blood's color and steps raising particles, the gore
/// and the color choosing among what the language's top levels allow, at those top levels by
/// default; a choice tried is told to the effects at once, and the file's word is read back.
///
/// Behaviour: ui.options.the-effects-section-chooses-what-keyed-effects-play
#[test]
fn the_effects_section_chooses_what_keyed_effects_play() {
    use auberean_draw::effects::Channels;
    const GORE: u32 = 0x4000_0035;
    const BLOOD: u32 = 0x4000_0036;
    const STEPS: u32 = 0x4000_0039;
    let records = &crate::screens::data().0.records;
    let settings = Settings::read("[Graphics]\r\nOverallGraphicsQuality=4\r\n");
    let mut p = Preferences::default();
    p.start(&settings, records);
    let first = p.take_channels().expect("told as read");
    assert_eq!(
        first,
        Channels {
            gore: 4,
            blood: 2,
            foot_step_particles: true
        }
    );
    assert_eq!(p.take_channels(), None, "told once");
    p.handle(&Request::Attach(vec![section::EFFECTS]));
    let l = p.listing().expect("listed").clone();
    let prefs = &l
        .sections
        .iter()
        .find(|(s, _)| *s == section::EFFECTS)
        .expect("the section")
        .1;
    assert_eq!(prefs.len(), 3);
    let gore = prefs
        .iter()
        .find_map(|c| match c {
            Change::Choice(c) if c.description.key == GORE => Some(c.clone()),
            _ => None,
        })
        .expect("gore");
    assert_eq!(gore.get(), 4);
    assert_eq!(gore.list_choices().len(), 4);
    let blood = prefs
        .iter()
        .find_map(|c| match c {
            Change::Choice(c) if c.description.key == BLOOD => Some(c.clone()),
            _ => None,
        })
        .expect("blood");
    assert_eq!(blood.list_choices().len(), 2);
    let mut none = gore.clone();
    none.set_value(2);
    p.handle(&Request::Change(Change::Choice(none)));
    assert_eq!(
        p.take_channels(),
        Some(Channels { gore: 2, ..first }),
        "told as it changes"
    );
    let steps = prefs
        .iter()
        .find_map(|c| match c {
            Change::Flag(f) if f.description.key == STEPS => Some(*f),
            _ => None,
        })
        .expect("steps");
    let mut off = steps;
    off.set_value(false);
    p.handle(&Request::Change(Change::Flag(off)));
    assert!(
        !p.take_channels().expect("told").foot_step_particles,
        "steps off"
    );
    // The file's words for them, and read back.
    p.handle(&Request::Apply(section::EFFECTS));
    p.handle(&Request::Save);
    let kept = p.take_kept().expect("kept");
    for (key, word) in [
        ("Gore", "None"),
        ("BloodColor", "red"),
        ("FootStepParticles", "0"),
    ] {
        assert!(
            kept.iter()
                .any(|(s, k, v)| s == "FX" && k == key && v == word),
            "{key}"
        );
    }
}

/// The client page's last section holds the interface's two preferences, the tooltip delay and
/// whether tooltips show, read from the preferences file by the data's names and handed over as
/// read; what the player applies is handed over, what is only tried is not; keeping writes them.
///
/// Behaviour: ui.options.the-client-pages-other-section-sets-tooltips
#[test]
fn the_client_pages_other_section_sets_tooltips() {
    let records = &crate::screens::data().0.records;
    let settings = Settings::read("[Misc]\r\nTooltipDelay=2.000000\r\nTooltipEnable=0\r\n");
    let mut p = Preferences::default();
    p.start(&settings, records);
    assert_eq!(p.take_tooltips(), (Some(2.0), Some(false)), "as read");
    assert_eq!(p.take_tooltips(), (None, None), "once");
    p.handle(&Request::Attach(vec![section::OTHER]));
    let l = p.listing().expect("listed").clone();
    let rows = &l
        .sections
        .iter()
        .find(|(s, _)| *s == section::OTHER)
        .expect("listed")
        .1;
    assert_eq!(rows.len(), 2);
    let (Change::Number(mut delay), Change::Flag(enable)) = (rows[0].clone(), rows[1].clone())
    else {
        panic!("a number, then a yes or no: {rows:?}");
    };
    assert_eq!(delay.description.key, 0x4000_005B);
    assert_eq!((delay.get(), enable.get()), (2.0, false));
    assert_eq!(delay.range(), (0.0, 10.0));
    delay.set_value(1.0);
    p.handle(&Request::Change(Change::Number(delay)));
    assert_eq!(p.take_tooltips(), (None, None), "tried, not applied");
    p.handle(&Request::Apply(section::OTHER));
    assert_eq!(p.take_tooltips(), (Some(1.0), None), "only what changed");
    p.handle(&Request::Save);
    let kept = p.take_kept().expect("kept");
    let value = |key: &str| {
        kept.iter()
            .find(|(s, k, _)| s.ends_with("Misc") && k.ends_with(key))
            .map(|(_, _, v)| v.clone())
    };
    assert_eq!(
        value("TooltipDelay").as_deref(),
        Some("1.000000"),
        "{kept:?}"
    );
    assert_eq!(value("TooltipEnable").as_deref(), Some("0"), "{kept:?}");
}

const RESOLUTION: u32 = 0x4000_0047;
const FULL_SCREEN: u32 = 0x4000_0003;
const SYNC: u32 = 0x4000_0006;
const ANTIALIASING: u32 = 0x4000_0060;

/// The rendering section as the page sees it: a menu of sizes, then its yes-or-no rows.
fn rendering_rows(
    p: &Preferences,
) -> (
    auberean_ui_game::options::preference::Choice,
    Vec<(u32, bool, bool)>,
) {
    let l = p.listing().expect("listed");
    let rows = &l
        .sections
        .iter()
        .find(|(s, _)| *s == section::RENDERING)
        .expect("listed")
        .1;
    let Change::Choice(size) = rows[0].clone() else {
        panic!("a choice first: {rows:?}");
    };
    let flags = rows[1..]
        .iter()
        .map(|c| match c {
            Change::Flag(f) => (f.description.key, f.get(), f.default_value()),
            _ => panic!("yes or no: {c:?}"),
        })
        .collect();
    (size, flags)
}

/// The rendering section holds the display's size and five yes-or-no preferences, in the device's
/// order. The sizes are those the display can take from 800 by 600 up, with 1024 by 768 always
/// there and the default, each named by its numbers. The defaults are full screen, waiting for the
/// display's refresh and nothing else; the values start as the settings hold them, a window's own
/// size among the choices when the display does not list it.
///
/// Behaviour: ui.options.the-rendering-section-lists-the-displays-sizes-and-the-devices-preferences
#[test]
fn the_rendering_section_lists_the_displays_sizes_and_the_devices_preferences() {
    let records = &crate::screens::data().0.records;
    let settings = Settings::read("[Display]\r\nWindowWidth=1500\r\nWindowHeight=900\r\n");
    let mut p = Preferences::default();
    p.start(&settings, records);
    p.set_presentation(
        &[(640, 480), (800, 600), (1920, 1080), (800, 600)],
        &settings,
    );
    p.handle(&Request::Attach(vec![section::RENDERING]));
    let (size, flags) = rendering_rows(&p);
    assert_eq!(size.description.key, RESOLUTION);
    let listed: Vec<&str> = size
        .list_choices()
        .iter()
        .map(|(v, _)| size.literal_choice(*v).expect("named by its numbers"))
        .collect();
    assert_eq!(listed, ["800x600", "1024x768", "1920x1080", "1500x900"]);
    assert_eq!(size.default_value(), 1024 << 16 | 768, "the default");
    assert_eq!(size.get(), 1500 << 16 | 900, "the window's");
    assert!(!size.changed(), "as applied");
    assert_eq!(
        flags,
        [
            (FULL_SCREEN, false, true),
            (0x4000_0005, false, false),
            (0x4000_0007, false, false),
            (SYNC, true, true),
            (ANTIALIASING, true, false),
        ]
    );
}

/// What the player sets in the rendering section reaches the host only when submitted, and then
/// again when put back. Kept, each goes to the file by the data's names, the size as one number,
/// its width above its height; read back it is the same.
///
/// Behaviour: ui.options.the-rendering-section-reaches-the-host-when-submitted-and-is-kept
#[test]
fn the_rendering_section_reaches_the_host_when_submitted_and_is_kept() {
    let records = &crate::screens::data().0.records;
    let settings = Settings::read("[Rendering]\r\nDisplayResolution=67109632\r\nFullScreen=0\r\n");
    let mut p = Preferences::default();
    p.start(&settings, records);
    p.set_presentation(&[(800, 600), (1024, 768), (1280, 960)], &settings);
    p.handle(&Request::Attach(vec![section::RENDERING]));
    let (mut size, flags) = rendering_rows(&p);
    assert_eq!(size.get(), 1024 << 16 | 768, "as read");
    assert_eq!(flags[0], (FULL_SCREEN, false, true));
    // A smaller size and antialiasing off tried: the host hears nothing yet.
    size.set_value(800 << 16 | 600);
    p.handle(&Request::Change(Change::Choice(size)));
    let aa = p
        .listing()
        .and_then(|l| {
            l.sections
                .iter()
                .flat_map(|(_, r)| r.iter())
                .find_map(|c| match c {
                    Change::Flag(f) if f.description.key == ANTIALIASING => Some(*f),
                    _ => None,
                })
        })
        .expect("listed");
    let mut off = aa;
    off.set_value(false);
    p.handle(&Request::Change(Change::Flag(off)));
    assert_eq!(p.take_presentation(), None, "tried");
    // Submitted: the host told; not yet marked applied, so no puts it back.
    p.handle(&Request::Submit(section::RENDERING));
    let shown = p.take_presentation().expect("told");
    assert_eq!(shown.size, (800, 600));
    assert!(!shown.antialiasing && !shown.full_screen && shown.sync_to_display);
    p.handle(&Request::Recall(section::RENDERING));
    let back = p.take_presentation().expect("told again");
    assert_eq!(back.size, (1024, 768));
    assert!(back.antialiasing);
    // Submitted again and kept: the file's words.
    let (mut size, _) = rendering_rows(&p);
    size.set_value(1280 << 16 | 960);
    p.handle(&Request::Change(Change::Choice(size)));
    p.handle(&Request::Submit(section::RENDERING));
    p.handle(&Request::Synchronize(section::RENDERING));
    p.handle(&Request::Save);
    let kept = p.take_kept().expect("kept");
    for (key, value) in [
        ("DisplayResolution", (1280 << 16 | 960).to_string()),
        ("FullScreen", "0".to_owned()),
        ("TripleBuffering", "0".to_owned()),
        ("OptimalRefreshRate", "0".to_owned()),
        ("SyncToDisplayRefresh", "1".to_owned()),
        ("Antialiasing", "1".to_owned()),
    ] {
        assert!(
            kept.iter()
                .any(|(s, k, v)| s == "Rendering" && k == key && *v == value),
            "{key} {value}: {kept:?}"
        );
    }
    let read = Settings::read(&format!(
        "[Rendering]\r\nDisplayResolution={}\r\nFullScreen=0\r\n",
        1280 << 16 | 960
    ));
    assert_eq!(read.window, (1280, 960));
}

/// Full screen flipped goes through its preference: the host told, the page told so its row shows
/// it, marked applied, so the page sees nothing to apply; flipped back the same.
///
/// Behaviour: ui.options.a-flip-of-full-screen-goes-through-its-preference
#[test]
fn a_flip_of_full_screen_goes_through_its_preference() {
    let records = &crate::screens::data().0.records;
    let settings = Settings::default();
    let mut p = Preferences::default();
    p.start(&settings, records);
    p.set_presentation(&[(1024, 768)], &settings);
    p.handle(&Request::Attach(vec![section::RENDERING]));
    assert!(p.take_changed().is_empty(), "nothing yet");
    p.toggle_full_screen();
    assert!(p.take_presentation().expect("the host told").full_screen);
    let told = p.take_changed();
    let [Change::Flag(f)] = told.as_slice() else {
        panic!("the page told of the one: {told:?}");
    };
    assert_eq!(f.description.key, FULL_SCREEN);
    assert!(f.get() && !f.changed(), "on, and applied");
    p.toggle_full_screen();
    assert!(!p.take_presentation().expect("told").full_screen);
}

/// While the game is a window, a window the player resizes is the display's size: its row shows
/// it and keeping keeps it. In full screen the window's size says nothing of the preference.
///
/// Behaviour: ui.options.a-resized-window-is-the-displays-size
#[test]
fn a_resized_window_is_the_displays_size() {
    let records = &crate::screens::data().0.records;
    let settings = Settings::default();
    let mut p = Preferences::default();
    p.start(&settings, records);
    p.set_presentation(&[(1024, 768), (1920, 1080)], &settings);
    p.handle(&Request::Attach(vec![section::RENDERING]));
    p.follow_window((1280, 960));
    let serial = p.listing().expect("listed").serial;
    p.follow_window((1280, 960));
    assert_eq!(p.listing().expect("listed").serial, serial, "no change");
    p.follow_window((1500, 900));
    let (size, _) = rendering_rows(&p);
    assert!(p.listing().expect("listed").serial > serial, "listed again");
    assert_eq!(size.get(), 1500 << 16 | 900);
    assert_eq!(size.literal_choice(size.get()), Some("1500x900"));
    assert!(!size.changed(), "as applied");
    assert_eq!(p.take_presentation(), None, "the host did it");
    p.toggle_full_screen();
    p.take_presentation();
    p.follow_window((1920, 1080));
    let (size, _) = rendering_rows(&p);
    assert_eq!(size.get(), 1500 << 16 | 900, "full screen: not the size");
}

/// The client's data opened for a language reads that language's pictures: a signpost's.
///
/// Behaviour: draw.picture.a-picture-made-for-one-language-is-in-its-file
#[test]
fn the_data_opened_for_a_language_reads_its_pictures() {
    use auberean_dat::locate::{install_dats, DatName};
    use auberean_dat::DatFile;
    let dir = auberean_dat::testing::require_target_install();
    let listed = install_dats(&dir).expect("list the install");
    let found: Vec<DatName> = listed.iter().map(|(n, _)| n.clone()).collect();
    let open = |name: &DatName| {
        let (_, p) = listed.iter().find(|(n, _)| n == name)?;
        DatFile::open(p).ok()
    };
    let install = auberean_interface::data::Install { found, open: &open };
    let (world, _, name) =
        auberean_interface::data::open_data(&install, Some("English"), "").expect("opens");
    assert_eq!(name, "English");
    assert!(world.records.holds(0x4010_0001), "the signpost's picture");
}
