//! The client's own preferences for the options window's client page, kept in the bank
//! (`auberean_ui_game::options::bank`).
//!
//! At startup the client's parts put their preferences in, each read from the preferences file:
//! the graphics (the world's drawing, which handles them), the camera's, the mouse's and the
//! avatar's (the camera, which handles them), the key maps' files. The graphics start as the
//! settings computed them. The page attaches its sections and is listed each one's preferences
//! (and again when they are recalled); what the player sets is written through to the bank's and
//! told to its handler; applying a section hands each handler its changed preferences, which the
//! world takes (the graphics) or the camera does (the camera's options); after each graphics apply
//! the overall graphics quality they match is worked out again. The sound manager's preferences
//! (the sound section: four volume sliders, the sound features, the number of sounds, sound off
//! and music off) are told to the sound as they change and as they are applied, and their values
//! as read handed to it at startup. The effects' preferences (how much gore, blood's color, steps
//! raising particles) choose among the entries of effects keyed to them, and are told as they
//! change and as they are applied. The interface's own two (how long the pointer rests before a
//! tooltip shows, and whether tooltips show) are the page's last section, handed to the interface
//! as read and as they are applied. Keeping writes every preference to the file. The rendering
//! section (the display's size, full screen, triple buffering, the refresh rate, waiting for the
//! display's refresh, antialiasing) lists the sizes the host's display can take; its values start as
//! the settings hold them, follow the window's size while it is a window, and go to the host when
//! submitted or put back. A flip of full screen goes through the same preference, and shows on the
//! page.

use auberean_audio::options as sound;
use auberean_draw::quality::{Graphics, Kind, GRAPHICS, PREFERENCES};
use auberean_scene::camera::Options;
use auberean_shell::preferences::EnumNames;
use auberean_shell::profile::Profile;
use auberean_shell::{Presentation, Settings};
use auberean_ui::Token;
use auberean_ui_game::options::bank::{
    Bank, Notice, Party, PreferenceFile, KEY_NAMES, SECTION_NAMES,
};
use auberean_ui_game::options::client::{Listing, Request, PREFERENCE_STRINGS};
use auberean_ui_game::options::helper::Change;
use auberean_ui_game::options::preference::{
    Choice, Description, Enumerations, Flag, Number, Pref, Text,
};
use auberean_world::records::WorldRecords;

/// The enumeration the graphics choices are named in.
const GRAPHICS_VALUES: u32 = 0x4000_004A;
/// The label prefix of a graphics choice's name.
const LEVEL: &str = "ID_Graphics_Value_";

/// The mapping that names the language's string state, and its value there.
const TABLE_MAPPING: u32 = 0x4000_0004;
const LANGUAGE_VALUE: u32 = 0x4000_0001;

/// Who handles a preference: the world's drawing, the camera, the sound, the effects.
const DRAWING: u32 = 1;
const CAMERA_HANDLER: u32 = 2;
const SOUND_HANDLER: u32 = 3;
const FX_HANDLER: u32 = 4;
const TOOLTIP_HANDLER: u32 = 5;
const RENDERING_HANDLER: u32 = 6;
/// The page, the monitor of its sections.
const PAGE: u32 = 1;

/// The rendering preferences: their section and keys, the yes-or-no ones in the order they are
/// put in with their labels and defaults, the display's size chosen when none is.
mod rendering {
    pub const SECTION: u32 = 0x4000_000B;
    pub const RESOLUTION: u32 = 0x4000_0047;
    pub const FULL_SCREEN: u32 = 0x4000_0003;
    pub const TRIPLE_BUFFERING: u32 = 0x4000_0005;
    pub const SYNC: u32 = 0x4000_0006;
    pub const OPTIMAL_REFRESH: u32 = 0x4000_0007;
    pub const ANTIALIASING: u32 = 0x4000_0060;
    pub const FLAGS: [(u32, &str, &str, bool); 5] = [
        (
            FULL_SCREEN,
            "ID_Rendering_FullScreen",
            "ID_Rendering_FullScreen_Help",
            true,
        ),
        (
            TRIPLE_BUFFERING,
            "ID_Rendering_TripleBuffering",
            "ID_Rendering_TripleBuffering_Help",
            false,
        ),
        (
            OPTIMAL_REFRESH,
            "ID_Rendering_OptimalRefreshRate",
            "ID_Rendering_OptimalRefreshRate_Help",
            false,
        ),
        (
            SYNC,
            "ID_Rendering_SyncToDisplayRefresh",
            "ID_Rendering_SyncToDisplayRefresh_Help",
            true,
        ),
        (
            ANTIALIASING,
            "ID_Rendering_Antialiasing",
            "ID_Rendering_Antialiasing_Help",
            false,
        ),
    ];
    pub const DEFAULT_SIZE: (u32, u32) = (1024, 768);
    /// The least width and height listed.
    pub const LEAST_SIZE: (u32, u32) = (800, 600);
}

/// A display's size as one number: its width above its height.
#[allow(clippy::cast_possible_wrap)] // sizes are far under the top bit
const fn packed_size(size: (u32, u32)) -> i32 {
    (size.0 << 16 | size.1 & 0xFFFF) as i32
}

/// The sections and keys of the camera's preferences.
mod section {
    pub const KEY_MAP: u32 = 0x4000_0006;
    pub const CAMERA: u32 = 0x4000_0008;
    pub const AVATAR: u32 = 0x4000_000D;
    pub const INPUT: u32 = 0x4000_000E;
    pub const OTHER: u32 = 0x4000_000C;
}
mod key {
    pub const MOUSE_LOOK_SENSITIVITY: u32 = 0x4000_0025;
    pub const MOUSE_INVERT_Y: u32 = 0x4000_0026;
    pub const CAMERA_HEIGHT: u32 = 0x4000_0027;
    pub const CAMERA_DISTANCE: u32 = 0x4000_002A;
    pub const CAMERA_ZOOM: u32 = 0x4000_002C;
    pub const CAMERA_ROTATE: u32 = 0x4000_002D;
    pub const CAMERA_TRANSITION: u32 = 0x4000_002F;
    pub const NORMAL: u32 = 0x4000_0030;
    pub const SHIFT: u32 = 0x4000_0031;
    pub const CONTROL: u32 = 0x4000_0032;
    pub const CAMERA_AUTO_DISTANCE: u32 = 0x4000_0051;
    pub const CAMERA_TILT: u32 = 0x4000_0054;
    pub const CAMERA_ALIGN_TO_SLOPE: u32 = 0x4000_005A;
    pub const AVATAR_LOOK_AROUND: u32 = 0x4000_0068;
    pub const AVATAR_LOOK_AT_CAMERA: u32 = 0x4000_0069;
    pub const AVATAR_LOOK_AT_SELECTION: u32 = 0x4000_006A;
    pub const AVATAR_IN_FIRST_PERSON: u32 = 0x4000_006C;
    pub const AVATAR_HOVER_TRANSPARENCY: u32 = 0x4000_006D;
    pub const MOUSE_LOOK_STEERS: u32 = 0x4000_006B;
    pub const TOOLTIP_DELAY: u32 = 0x4000_005B;
    pub const TOOLTIP_ENABLE: u32 = 0x4000_005C;
}

/// A camera preference's kind: a number's range and default, or a yes-or-no's default.
enum CameraKind {
    Number(f32, f32, f32),
    Flag(bool),
}

/// The camera's preferences, in the order the camera puts them in: each one's section and key,
/// label and help, and kind.
// D2-10689
const CAMERA_PREFERENCES: [(u32, u32, &str, &str, CameraKind); 14] = [
    (
        section::CAMERA,
        key::CAMERA_HEIGHT,
        "ID_Camera_Height",
        "ID_Camera_Height_Help",
        CameraKind::Number(-1.0, 1.0, 0.0),
    ),
    (
        section::CAMERA,
        key::CAMERA_DISTANCE,
        "ID_Camera_Distance",
        "ID_Camera_Distance_Help",
        CameraKind::Number(-0.2, 10.0, 4.5),
    ),
    (
        section::CAMERA,
        key::CAMERA_TILT,
        "ID_Camera_Tilt",
        "ID_Camera_Tilt_Help",
        CameraKind::Number(-0.25, 1.0, 0.25),
    ),
    (
        section::CAMERA,
        key::CAMERA_TRANSITION,
        "ID_Camera_Transition_Time",
        "ID_Camera_Transition_Help",
        CameraKind::Number(0.1, 2.0, 1.0),
    ),
    (
        section::CAMERA,
        key::CAMERA_ROTATE,
        "ID_Camera_Rotate_Speed",
        "ID_Camera_Rotate_Help",
        CameraKind::Number(0.1, 5.0, 1.0),
    ),
    (
        section::CAMERA,
        key::CAMERA_ZOOM,
        "ID_Camera_Zoom_Speed",
        "ID_Camera_Zoom_Help",
        CameraKind::Number(0.1, 5.0, 1.0),
    ),
    (
        section::CAMERA,
        key::CAMERA_AUTO_DISTANCE,
        "ID_Camera_Auto_Distance",
        "ID_Camera_Auto_Distance_Help",
        CameraKind::Flag(true),
    ),
    (
        section::CAMERA,
        key::CAMERA_ALIGN_TO_SLOPE,
        "ID_Camera_Align_To_Slope",
        "ID_Camera_Align_To_Slope_Help",
        CameraKind::Flag(true),
    ),
    (
        section::INPUT,
        key::MOUSE_LOOK_SENSITIVITY,
        "ID_Input_Mouse_Look_Sensitivity",
        "ID_Input_Mouse_Look_Sensitivity_Help",
        CameraKind::Number(0.5, 2.0, 1.0),
    ),
    (
        section::INPUT,
        key::MOUSE_INVERT_Y,
        "ID_Input_Invert_Y",
        "ID_Input_Invert_Y_Help",
        CameraKind::Flag(false),
    ),
    (
        section::INPUT,
        key::MOUSE_LOOK_STEERS,
        "ID_Input_Steer_Player",
        "ID_Input_Steer_Player_Help",
        CameraKind::Flag(true),
    ),
    (
        section::AVATAR,
        key::AVATAR_LOOK_AROUND,
        "ID_Avatar_Look_Around",
        "ID_Avatar_Look_Around_Help",
        CameraKind::Flag(true),
    ),
    (
        section::AVATAR,
        key::AVATAR_LOOK_AT_CAMERA,
        "ID_Avatar_Looks_At_Camera",
        "ID_Avatar_Looks_At_Camera_Help",
        CameraKind::Flag(true),
    ),
    (
        section::AVATAR,
        key::AVATAR_LOOK_AT_SELECTION,
        "ID_Avatar_Look_At_Selection",
        "ID_Avatar_Look_At_Selection_Help",
        CameraKind::Flag(true),
    ),
];

/// When the avatar shows in first person: its enumeration, and each choice's number, name in that
/// enumeration and label. The file holds the name.
const FIRST_PERSON_VALUES: u32 = 0x4000_004A;
const FIRST_PERSON_CHOICES: [(i32, u32, &str); 3] = [
    (0, 0x4000_0012, "ID_Avatar_Value_Never"),
    (1, 0x4000_0014, "ID_Avatar_Value_Always"),
    (2, 0x4000_0013, "ID_Avatar_Value_DuringCombat"),
];
/// Which choice shows the avatar until it is chosen: during combat.
const FIRST_PERSON_DEFAULT: i32 = 2;

/// The avatar's two preferences that are the drawing's: put in ahead of the camera's, so the avatar
/// section lists them first.
// D2-09703
fn drawing_avatar_preferences() -> [(u32, Pref); 2] {
    let describe = |label: &str, help: &str| {
        let mut d = Description::default();
        d.describe(
            PREFERENCE_STRINGS,
            Token::of(label).raw(),
            Token::of(help).raw(),
        );
        d
    };
    let mut first_person = Choice::new();
    first_person.map(FIRST_PERSON_VALUES);
    for (v, named, label) in FIRST_PERSON_CHOICES {
        first_person.assign_named_choice(v, (PREFERENCE_STRINGS, Token::of(label).raw()), named);
    }
    first_person.set_default(FIRST_PERSON_DEFAULT);
    first_person.description = describe(
        "ID_Avatar_VisibleInFirstPerson",
        "ID_Avatar_VisibleInFirstPerson_Help",
    );
    let mut hover = Flag::new();
    hover.set_default(false);
    hover.description = describe(
        "ID_Avatar_MouseHoverTransparency",
        "ID_Avatar_MouseHoverTransparency_Help",
    );
    [
        (key::AVATAR_IN_FIRST_PERSON, Pref::Choice(first_person)),
        (key::AVATAR_HOVER_TRANSPARENCY, Pref::Flag(hover)),
    ]
}

/// The key maps' files, as the client puts them in: each key and its default file.
const KEY_MAP_FILES: [(u32, &str); 3] = [
    (key::NORMAL, "Main.keymap"),
    (key::SHIFT, "Shift.keymap"),
    (key::CONTROL, "Control.keymap"),
];

/// The names the data gives the preferences' sections and keys and the graphics choices.
#[derive(Clone, Debug, Default, PartialEq)]
struct Names(Vec<(u32, EnumNames)>);

impl Names {
    fn read(records: &WorldRecords) -> Self {
        Self(
            [
                SECTION_NAMES,
                KEY_NAMES,
                GRAPHICS_VALUES,
                FIRST_PERSON_VALUES,
                fx::GORE_VALUES,
                fx::BLOOD_VALUES,
            ]
            .into_iter()
            .map(|e| (e, EnumNames::read(records, e)))
            .collect(),
        )
    }

    fn of(&self, e: u32) -> Option<&EnumNames> {
        self.0.iter().find(|(x, _)| *x == e).map(|(_, n)| n)
    }
}

impl Enumerations for Names {
    fn value_of(&self, enumeration: u32, name: &str) -> Option<u32> {
        self.of(enumeration)?.value(name)
    }
    fn name_of(&self, enumeration: u32, value: u32) -> Option<String> {
        self.of(enumeration)?.name(value).map(str::to_owned)
    }
}

/// The preferences file as the bank reads it at startup.
struct File<'a>(&'a Profile);

impl PreferenceFile for File<'_> {
    fn read(&self, section: &str, key: &str, default: &str, size: usize) -> String {
        self.0.read(section, key, default, size)
    }
    fn write(&mut self, _section: &str, _key: &str, _value: &str) {}
}

/// What the bank keeps, gathered to be written by the shell.
#[derive(Default)]
struct Writes(Vec<(String, String, String)>);

impl PreferenceFile for Writes {
    fn read(&self, _section: &str, _key: &str, default: &str, _size: usize) -> String {
        default.to_owned()
    }
    fn write(&mut self, section: &str, key: &str, value: &str) {
        self.0
            .push((section.to_owned(), key.to_owned(), value.to_owned()));
    }
}

/// The client's preferences as the client page sees them.
#[derive(Debug)]
pub struct Preferences {
    bank: Bank,
    names: Names,
    /// The page's sections, in the order it attached them.
    shown: Vec<u32>,
    listing: Option<Listing>,
    serial: u32,
    /// The overall graphics quality the applied preferences match (0: the player's own).
    pub quality: u32,
    /// The camera's options as last applied.
    camera: Options,
    /// Applied since the world or the camera last took them.
    fresh_graphics: bool,
    fresh_camera: bool,
    /// What the page asked to keep in the file, not yet handed on.
    keep: Option<Vec<(String, String, String)>>,
    /// The sound preferences told to the sound, not yet handed on: each key, value, and whether
    /// applied (else changed).
    sound: Vec<(u32, f32, bool)>,
    /// The effects' preferences changed or applied since last handed on (and, first, as read).
    fresh_channels: bool,
    /// The tooltip delay and whether tooltips show, each when applied since last handed on (and,
    /// first, as read).
    tooltips: (Option<f32>, Option<bool>),
    /// The rendering preferences submitted or put back since the host last took them.
    fresh_presentation: bool,
    /// The window's size when last looked at.
    window: (u32, u32),
    /// The preferences the client changed itself, for the page.
    changed: Vec<Change>,
    /// The top gore level and blood color the language allows.
    tops: (u32, u32),
    /// What the command line forces over them (0: nothing).
    forced_tops: (u32, u32),
}

impl Default for Preferences {
    fn default() -> Self {
        let mut me = Self {
            bank: Bank::new(),
            names: Names::default(),
            shown: Vec::new(),
            listing: None,
            serial: 0,
            quality: 0,
            camera: Options::default(),
            fresh_graphics: false,
            fresh_camera: false,
            keep: None,
            sound: Vec::new(),
            fresh_channels: false,
            tooltips: (None, None),
            fresh_presentation: false,
            window: (0, 0),
            changed: Vec::new(),
            tops: (fx::NORMAL, fx::RED),
            forced_tops: (0, 0),
        };
        me.put_in(&Profile::default());
        me.set(&Graphics::first_run());
        me
    }
}

/// A graphics preference as the bank keeps it: its label and help, its choices (each named in the
/// graphics choices' enumeration) or range, its default.
fn graphics_preference(p: &auberean_draw::quality::Preference, names: &Names) -> Pref {
    let mut d = Description::default();
    d.describe(
        PREFERENCE_STRINGS,
        Token::of(p.label).raw(),
        Token::of(p.help).raw(),
    );
    match p.kind {
        Kind::Choice { choices, default } => {
            let mut c = Choice::new();
            c.map(GRAPHICS_VALUES);
            for (v, label) in choices {
                let words = (PREFERENCE_STRINGS, Token::of(label).raw());
                match names.value_of(GRAPHICS_VALUES, label.trim_start_matches(LEVEL)) {
                    Some(named) => c.assign_named_choice(*v, words, named),
                    None => c.assign_choice(*v, words),
                };
            }
            c.set_default(default);
            c.description = d;
            Pref::Choice(c)
        }
        Kind::Flag { default } => {
            let mut f = Flag::new();
            f.set_default(default);
            f.description = d;
            Pref::Flag(f)
        }
        Kind::Number {
            least,
            greatest,
            default,
        } => {
            let mut n = Number::new();
            n.set_range(least, greatest);
            n.set_default(default);
            n.description = d;
            Pref::Number(n)
        }
    }
}

/// A camera preference as the bank keeps it.
fn camera_preference(label: &str, help: &str, kind: &CameraKind) -> Pref {
    let mut d = Description::default();
    d.describe(
        PREFERENCE_STRINGS,
        Token::of(label).raw(),
        Token::of(help).raw(),
    );
    match kind {
        CameraKind::Number(least, greatest, default) => {
            let mut n = Number::new();
            n.set_range(*least, *greatest);
            n.set_default(*default);
            n.description = d;
            Pref::Number(n)
        }
        CameraKind::Flag(default) => {
            let mut f = Flag::new();
            f.set_default(*default);
            f.description = d;
            Pref::Flag(f)
        }
    }
}

/// The interface's preferences, in the order it puts them in: each one's key, label and help, and
/// kind. The defaults are the interface's own.
// D2-11271
const TOOLTIP_PREFERENCES: [(u32, &str, &str, CameraKind); 2] = [
    (
        key::TOOLTIP_DELAY,
        "ID_Misc_TooltipDelay",
        "ID_Misc_TooltipDelay_Help",
        CameraKind::Number(0.0, 10.0, 0.25),
    ),
    (
        key::TOOLTIP_ENABLE,
        "ID_Misc_TooltipEnable",
        "ID_Misc_TooltipEnable_Help",
        CameraKind::Flag(true),
    ),
];

/// A sound preference as the bank keeps it: its label and help, its range, the features or yes
/// or no, and its default.
fn sound_preference(p: &sound::SoundPreference) -> Pref {
    let mut d = Description::default();
    d.describe(
        PREFERENCE_STRINGS,
        Token::of(p.label).raw(),
        Token::of(p.help).raw(),
    );
    match p.kind {
        sound::Kind::Number {
            least,
            greatest,
            default,
        } => {
            let mut n = Number::new();
            n.set_range(least, greatest);
            n.set_default(default);
            n.description = d;
            Pref::Number(n)
        }
        sound::Kind::Features { default } => {
            let mut c = Choice::new();
            c.map(sound::FEATURE_NAMES);
            for (v, label, named) in sound::FEATURES {
                c.assign_named_choice(v, (PREFERENCE_STRINGS, Token::of(label).raw()), named);
            }
            c.set_default(default);
            c.description = d;
            Pref::Choice(c)
        }
        sound::Kind::Flag { default } => {
            let mut f = Flag::new();
            f.set_default(default);
            f.description = d;
            Pref::Flag(f)
        }
    }
}

/// The effects' preferences: their section, keys and choices.
mod fx {
    pub const SECTION: u32 = 0x4000_0009;
    pub const GORE: u32 = 0x4000_0035;
    pub const BLOOD: u32 = 0x4000_0036;
    pub const FOOT_STEP_PARTICLES: u32 = 0x4000_0039;
    pub const GORE_VALUES: u32 = 0x4000_0044;
    pub const BLOOD_VALUES: u32 = 0x4000_0045;
    pub const NORMAL: u32 = 4;
    pub const RED: u32 = 2;
    /// Each level, its words, and the language's top level it asks for.
    pub const GORE_CHOICES: [(i32, &str, u32); 4] = [
        (1, "ID_Graphics_Value_Fun", 0),
        (2, "ID_Graphics_Value_None", 0),
        (3, "ID_Graphics_Value_Low", 3),
        (4, "ID_Graphics_Value_Normal", 4),
    ];
    /// Each color, its words, and the language's top color it asks for.
    pub const BLOOD_CHOICES: [(i32, &str, u32); 2] = [
        (1, "ID_FX_Hue_Value_Green", 0),
        (2, "ID_FX_Hue_Value_Red", 2),
    ];
}

/// A choice among the effects' preferences as the bank keeps it: those the language's top level
/// allows, that top level its default.
#[allow(clippy::cast_possible_wrap)] // a few levels
fn fx_choice(
    label: &str,
    help: &str,
    enumeration: u32,
    choices: &[(i32, &str, u32)],
    top: u32,
) -> Pref {
    let mut d = Description::default();
    d.describe(
        PREFERENCE_STRINGS,
        Token::of(label).raw(),
        Token::of(help).raw(),
    );
    let mut c = Choice::new();
    c.map(enumeration);
    for (v, words, least) in choices {
        if top >= *least {
            c.assign_named_choice(
                *v,
                (PREFERENCE_STRINGS, Token::of(words).raw()),
                v.unsigned_abs(),
            );
        }
    }
    c.set_default(top as i32);
    c.description = d;
    Pref::Choice(c)
}

/// A preference's value as a number (a choice's number, 1 or 0 for yes or no).
#[allow(clippy::cast_precision_loss)] // a few levels
fn number_of(p: &Pref) -> f32 {
    match p {
        Pref::Flag(f) => f32::from(u8::from(f.get())),
        Pref::Number(n) => n.get(),
        Pref::Choice(c) => c.get() as f32,
        Pref::Text(_) => 0.0,
    }
}

impl Preferences {
    /// The command line's top gore level and top blood color, over the language's (0: its own).
    // D2-14179, D2-14180, D2-14185, D2-14186
    pub fn force_tops(&mut self, gore: u32, blood: u32) {
        self.forced_tops = (gore, blood);
    }

    /// Every part's preferences put in, read from the file the settings were read from, with the
    /// world's names for them; the camera's options as the settings hold them.
    pub fn start(&mut self, settings: &Settings, records: &WorldRecords) {
        self.names = Names::read(records);
        self.bank = Bank::new();
        self.camera = settings.camera;
        let language = records.data_id_by_enum(LANGUAGE_VALUE, TABLE_MAPPING);
        if let Some(s) = records.get::<auberean_assets::strings::StringState>(language) {
            self.tops = (s.top_gore_level(), s.top_blood_color());
        }
        if self.forced_tops.0 != 0 {
            self.tops.0 = self.forced_tops.0;
        }
        if self.forced_tops.1 != 0 {
            self.tops.1 = self.forced_tops.1;
        }
        self.put_in(settings.profile());
        self.start_sound();
        self.fresh_channels = true;
        self.tooltips = (
            self.bank.find(key::TOOLTIP_DELAY).map(number_of),
            self.bank
                .find(key::TOOLTIP_ENABLE)
                .map(|p| number_of(p) != 0.0),
        );
    }

    /// The rendering preferences put in, read from the file the settings were read from: the sizes
    /// the display can take that are large enough and the size chosen when none is, smallest first,
    /// each named by its width and height; the values as the settings hold them, as applied.
    // D2-08358
    pub fn set_presentation(&mut self, modes: &[(u32, u32)], settings: &Settings) {
        let mut sizes: Vec<(u32, u32)> = modes
            .iter()
            .copied()
            .filter(|&(w, h)| w >= rendering::LEAST_SIZE.0 && h >= rendering::LEAST_SIZE.1)
            .chain([rendering::DEFAULT_SIZE])
            .collect();
        sizes.sort_by_key(|&(w, h)| (u64::from(w) * u64::from(h), w));
        sizes.dedup();
        let describe = |label: &str, help: &str| {
            let mut d = Description::default();
            d.describe(
                PREFERENCE_STRINGS,
                Token::of(label).raw(),
                Token::of(help).raw(),
            );
            d
        };
        let mut size = Choice::new();
        for &(w, h) in &sizes {
            size.assign_literal_choice(packed_size((w, h)), &format!("{w}x{h}"));
        }
        size.set_default(packed_size(rendering::DEFAULT_SIZE));
        size.description = describe(
            "ID_Rendering_DisplayResolution",
            "ID_Rendering_DisplayResolution_Help",
        );
        let file = File(settings.profile());
        let mut out = Vec::new();
        let mut size = Pref::Choice(size);
        self.bank.bless(
            &mut size,
            rendering::SECTION,
            rendering::RESOLUTION,
            &file,
            &self.names,
            &mut out,
        );
        self.bank.link(rendering::RESOLUTION, RENDERING_HANDLER);
        for (k, label, help, default) in rendering::FLAGS {
            let mut f = Flag::new();
            f.set_default(default);
            f.description = describe(label, help);
            self.bank.bless(
                &mut Pref::Flag(f),
                rendering::SECTION,
                k,
                &file,
                &self.names,
                &mut out,
            );
            self.bank.link(k, RENDERING_HANDLER);
        }
        let p = settings.presentation();
        self.show_size(p.size);
        for (k, on) in [
            (rendering::FULL_SCREEN, p.full_screen),
            (rendering::TRIPLE_BUFFERING, p.triple_buffering),
            (rendering::SYNC, p.sync_to_display),
            (rendering::ANTIALIASING, p.antialiasing),
        ] {
            if let Some(Pref::Flag(mut f)) = self.bank.find(k).cloned() {
                f.set_value(on);
                self.bank.take_value(&Pref::Flag(f));
            }
        }
        self.bank.synchronize(rendering::SECTION);
        self.relist();
    }

    /// The display's size made this one, a choice of its own when the host's display does not list
    /// it (a window can be any size).
    fn show_size(&mut self, size: (u32, u32)) {
        let Some(Pref::Choice(mut c)) = self.bank.find(rendering::RESOLUTION).cloned() else {
            return;
        };
        let v = packed_size(size);
        if c.examine_choice(v).is_none() {
            c.assign_literal_choice(v, &format!("{}x{}", size.0, size.1));
            c.set_value(v);
            let mut out = Vec::new();
            self.bank.set(
                &Pref::Choice(c),
                &File(&Profile::default()),
                &self.names,
                &mut out,
            );
        } else {
            c.set_value(v);
            self.bank.take_value(&Pref::Choice(c));
        }
    }

    /// How the rendering preferences show the game now.
    fn presentation(&self) -> Presentation {
        let on = |k: u32| matches!(self.bank.find(k), Some(Pref::Flag(f)) if f.get());
        #[allow(clippy::cast_sign_loss)] // a size, its top bit clear
        let size = match self.bank.find(rendering::RESOLUTION) {
            Some(Pref::Choice(c)) => {
                let v = c.get() as u32;
                (v >> 16, v & 0xFFFF)
            }
            _ => rendering::DEFAULT_SIZE,
        };
        Presentation {
            size,
            full_screen: on(rendering::FULL_SCREEN),
            triple_buffering: on(rendering::TRIPLE_BUFFERING),
            sync_to_display: on(rendering::SYNC),
            antialiasing: on(rendering::ANTIALIASING),
        }
    }

    /// How the game is to be shown, when the rendering preferences were submitted or put back
    /// since last taken.
    pub fn take_presentation(&mut self) -> Option<Presentation> {
        std::mem::take(&mut self.fresh_presentation).then(|| self.presentation())
    }

    /// The preferences the client changed itself since last taken, for the page.
    pub fn take_changed(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.changed)
    }

    /// The window's size now: when it moved and the game is not full screen, the display's size
    /// preference is it, as applied.
    pub fn follow_window(&mut self, size: (u32, u32)) {
        if size == self.window {
            return;
        }
        self.window = size;
        if self.presentation().full_screen || self.bank.find(rendering::RESOLUTION).is_none() {
            return;
        }
        if self.presentation().size != size {
            self.show_size(size);
            let mut out = Vec::new();
            if let Some(p) = self.bank.find(rendering::RESOLUTION).cloned() {
                self.bank.apply_one(&p, &mut out);
            }
            self.relist();
        }
    }

    /// Full screen flipped: the preference's value, applied (the host told) and changed (the page
    /// told).
    // D2-08327
    pub fn toggle_full_screen(&mut self) {
        let Some(Pref::Flag(mut f)) = self.bank.find(rendering::FULL_SCREEN).cloned() else {
            return;
        };
        f.set_value(!f.get());
        self.bank.take_value(&Pref::Flag(f));
        let mut out = Vec::new();
        self.bank.apply_one(&Pref::Flag(f), &mut out);
        let Some(kept) = self.bank.find(rendering::FULL_SCREEN).cloned() else {
            return;
        };
        self.bank.change(&kept, &mut out);
        for n in &out {
            if let Notice::Change {
                party: Party::Monitor(_),
                pref: Pref::Flag(f),
            } = n
            {
                self.changed.push(Change::Flag(*f));
            }
        }
        self.told(out);
    }

    /// The tooltip delay and whether tooltips show, each when applied since last taken (and,
    /// first, as read).
    pub fn take_tooltips(&mut self) -> (Option<f32>, Option<bool>) {
        std::mem::take(&mut self.tooltips)
    }

    /// What the effects' preferences ask of the effects, when changed since last taken.
    pub fn take_channels(&mut self) -> Option<auberean_draw::effects::Channels> {
        std::mem::take(&mut self.fresh_channels).then(|| self.channels())
    }

    /// What the effects' preferences ask of the effects.
    #[allow(clippy::cast_sign_loss)] // the choices are small and positive
    fn channels(&self) -> auberean_draw::effects::Channels {
        let choice = |k: u32, or: u32| match self.bank.find(k) {
            Some(Pref::Choice(c)) => c.get() as u32,
            _ => or,
        };
        auberean_draw::effects::Channels {
            gore: choice(fx::GORE, fx::NORMAL),
            blood: choice(fx::BLOOD, fx::RED),
            foot_step_particles: match self.bank.find(fx::FOOT_STEP_PARTICLES) {
                Some(Pref::Flag(f)) => f.get(),
                _ => true,
            },
        }
    }

    /// The sound takes the sound preferences' values as read, as it starts.
    // D2-07680
    fn start_sound(&mut self) {
        self.sound = sound::AT_START
            .iter()
            .filter_map(|&(k, applied)| Some((k, number_of(self.bank.find(k)?), applied)))
            .collect();
    }

    fn put_in(&mut self, profile: &Profile) {
        let file = File(profile);
        let mut out = Vec::new();
        for p in &PREFERENCES {
            let mut pref = graphics_preference(p, &self.names);
            self.bank
                .bless(&mut pref, GRAPHICS, p.key, &file, &self.names, &mut out);
            self.bank.link(p.key, DRAWING);
        }
        for (k, mut pref) in drawing_avatar_preferences() {
            self.bank
                .bless(&mut pref, section::AVATAR, k, &file, &self.names, &mut out);
            self.bank.link(k, CAMERA_HANDLER);
        }
        for (s, k, label, help, kind) in &CAMERA_PREFERENCES {
            let mut pref = camera_preference(label, help, kind);
            self.bank
                .bless(&mut pref, *s, *k, &file, &self.names, &mut out);
            self.bank.link(*k, CAMERA_HANDLER);
        }
        // D2-07685
        for p in &sound::PREFERENCES {
            let mut pref = sound_preference(p);
            self.bank.bless(
                &mut pref,
                sound::SECTION,
                p.key,
                &file,
                &self.names,
                &mut out,
            );
            self.bank.link(p.key, SOUND_HANDLER);
        }
        // D2-06886
        let (top_gore, top_blood) = self.tops;
        let mut steps = Flag::new();
        steps.set_default(true);
        steps.description.describe(
            PREFERENCE_STRINGS,
            Token::of("ID_FX_Foot_Step_Particles").raw(),
            Token::of("ID_FX_Foot_Step_Particles_Help").raw(),
        );
        for (k, mut pref) in [
            (
                fx::GORE,
                fx_choice(
                    "ID_FX_Gore_Amount",
                    "ID_FX_Gore_Amount_Help",
                    fx::GORE_VALUES,
                    &fx::GORE_CHOICES,
                    top_gore,
                ),
            ),
            (
                fx::BLOOD,
                fx_choice(
                    "ID_FX_Gore_Hue",
                    "ID_FX_Gore_Hue_Help",
                    fx::BLOOD_VALUES,
                    &fx::BLOOD_CHOICES,
                    top_blood,
                ),
            ),
            (fx::FOOT_STEP_PARTICLES, Pref::Flag(steps)),
        ] {
            self.bank
                .bless(&mut pref, fx::SECTION, k, &file, &self.names, &mut out);
            self.bank.link(k, FX_HANDLER);
        }
        for (k, label, help, kind) in &TOOLTIP_PREFERENCES {
            let mut pref = camera_preference(label, help, kind);
            self.bank
                .bless(&mut pref, section::OTHER, *k, &file, &self.names, &mut out);
            self.bank.link(*k, TOOLTIP_HANDLER);
        }
        // D2-01694
        for (k, default) in KEY_MAP_FILES {
            let mut t = Text::new();
            t.set_default(default);
            self.bank.bless(
                &mut Pref::Text(t),
                section::KEY_MAP,
                k,
                &file,
                &self.names,
                &mut out,
            );
        }
    }

    /// The preferences as last listed for the page.
    #[must_use]
    pub fn listing(&self) -> Option<&Listing> {
        self.listing.as_ref()
    }

    /// The graphics preferences the client starts with, as applied.
    pub fn set(&mut self, graphics: &Graphics) {
        let mut out = Vec::new();
        let level = dereth_primitives::num::to_i32;
        for p in &PREFERENCES {
            let (Some(v), Some(d)) = (graphics.get(p.key), graphics.default_of(p.key)) else {
                continue;
            };
            let Some(mut pref) = self.bank.find(p.key).cloned() else {
                continue;
            };
            match &mut pref {
                Pref::Choice(c) => {
                    c.set_default(level(d));
                    c.set_value(level(v));
                }
                Pref::Flag(f) => {
                    f.set_default(d != 0.0);
                    f.set_value(v != 0.0);
                }
                Pref::Number(n) => {
                    n.set_default(d);
                    n.set_value(v);
                }
                Pref::Text(_) => {}
            }
            self.bank
                .set(&pref, &File(&Profile::default()), &self.names, &mut out);
        }
        self.bank.synchronize(GRAPHICS);
        self.quality = graphics.overall();
        self.relist();
    }

    /// The graphics preferences as the bank holds them now.
    fn graphics(&self) -> Graphics {
        let mut g = Graphics::default();
        for p in self.bank.list(GRAPHICS) {
            g.set(p.description().key, number_of(&p));
        }
        g
    }

    /// The camera's options as the bank holds them now.
    fn camera_options(&self) -> Options {
        let mut o = self.camera;
        let n = |k: u32| self.bank.find(k).map(number_of);
        let on = |k: u32| self.bank.find(k).map(|p| number_of(p) != 0.0);
        o.height = n(key::CAMERA_HEIGHT).unwrap_or(o.height);
        o.distance = n(key::CAMERA_DISTANCE).unwrap_or(o.distance);
        o.tilt = n(key::CAMERA_TILT).unwrap_or(o.tilt);
        o.transition_time = n(key::CAMERA_TRANSITION).unwrap_or(o.transition_time);
        o.rotate_speed = n(key::CAMERA_ROTATE).unwrap_or(o.rotate_speed);
        o.zoom_speed = n(key::CAMERA_ZOOM).unwrap_or(o.zoom_speed);
        o.auto_distance = on(key::CAMERA_AUTO_DISTANCE).unwrap_or(o.auto_distance);
        o.align_to_slope = on(key::CAMERA_ALIGN_TO_SLOPE).unwrap_or(o.align_to_slope);
        o.mouse_sensitivity = n(key::MOUSE_LOOK_SENSITIVITY).unwrap_or(o.mouse_sensitivity);
        o.invert_y = on(key::MOUSE_INVERT_Y).unwrap_or(o.invert_y);
        o.mouse_look_steers = on(key::MOUSE_LOOK_STEERS).unwrap_or(o.mouse_look_steers);
        o.look_around = on(key::AVATAR_LOOK_AROUND).unwrap_or(o.look_around);
        o.look_at_camera = on(key::AVATAR_LOOK_AT_CAMERA).unwrap_or(o.look_at_camera);
        o.look_at_selection = on(key::AVATAR_LOOK_AT_SELECTION).unwrap_or(o.look_at_selection);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        if let Some(v) = n(key::AVATAR_IN_FIRST_PERSON)
            .and_then(|v| auberean_scene::camera::AvatarInFirstPerson::of_number(v as u32))
        {
            o.avatar_in_first_person = v;
        }
        o.avatar_hover_transparency =
            on(key::AVATAR_HOVER_TRANSPARENCY).unwrap_or(o.avatar_hover_transparency);
        o
    }

    /// The graphics preferences applied since the world last took them.
    pub fn take_applied(&mut self) -> Option<Graphics> {
        std::mem::take(&mut self.fresh_graphics).then(|| self.graphics())
    }

    /// The camera's options applied since the camera last took them.
    pub fn take_camera(&mut self) -> Option<Options> {
        std::mem::take(&mut self.fresh_camera).then_some(self.camera)
    }

    /// The sound preferences told to the sound since it last took them: each key, value, and
    /// whether applied (else changed).
    pub fn take_sound(&mut self) -> Vec<(u32, f32, bool)> {
        std::mem::take(&mut self.sound)
    }

    /// The preferences to keep in the file, when the page asked to keep them.
    pub fn take_kept(&mut self) -> Option<Vec<(String, String, String)>> {
        self.keep.take()
    }

    /// What the bank told, carried out.
    fn told(&mut self, out: Vec<Notice>) {
        for n in out {
            if let Notice::Change {
                party: Party::Handler(SOUND_HANDLER),
                pref,
            } = &n
            {
                self.sound
                    .push((pref.description().key, number_of(pref), false));
                continue;
            }
            if let Notice::Change {
                party: Party::Handler(FX_HANDLER),
                ..
            } = &n
            {
                self.fresh_channels = true;
                continue;
            }
            if let Notice::Handle { handler, prefs } = n {
                match handler {
                    FX_HANDLER => self.fresh_channels = true,
                    RENDERING_HANDLER => self.fresh_presentation = true,
                    SOUND_HANDLER => self.sound.extend(
                        prefs
                            .iter()
                            .map(|p| (p.description().key, number_of(p), true)),
                    ),
                    DRAWING => {
                        self.quality = self.graphics().overall();
                        self.fresh_graphics = true;
                        tracing::debug!(quality = self.quality, "graphics preferences applied");
                    }
                    CAMERA_HANDLER => {
                        self.camera = self.camera_options();
                        self.fresh_camera = true;
                    }
                    TOOLTIP_HANDLER => {
                        for p in &prefs {
                            match p.description().key {
                                key::TOOLTIP_DELAY => self.tooltips.0 = Some(number_of(p)),
                                key::TOOLTIP_ENABLE => {
                                    self.tooltips.1 = Some(number_of(p) != 0.0);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// One of the page's requests carried out.
    // D2-09722
    pub fn handle(&mut self, r: &Request) {
        let mut out = Vec::new();
        match r {
            Request::Attach(sections) => {
                for s in sections {
                    if self.bank.attach(*s, PAGE, &mut out) && !self.shown.contains(s) {
                        self.shown.push(*s);
                    }
                }
                self.relist();
            }
            Request::Detach(sections) => {
                for s in sections {
                    self.bank.detach(*s);
                }
                self.shown.retain(|s| !sections.contains(s));
                if self.shown.is_empty() {
                    self.listing = None;
                }
            }
            Request::Change(c) => {
                let pref = match c {
                    Change::Flag(f) => Pref::Flag(*f),
                    Change::Number(n) => Pref::Number(*n),
                    Change::Choice(ch) => Pref::Choice(ch.clone()),
                };
                self.bank.take_value(&pref);
                self.bank.change(&pref, &mut out);
            }
            Request::Submit(s) => self.bank.submit(*s, &mut out),
            Request::Apply(s) => self.bank.apply(*s, &mut out),
            Request::Synchronize(s) => self.bank.synchronize(*s),
            Request::Recall(s) => self.bank.recall(*s, &mut out),
            Request::Save => {
                let mut w = Writes::default();
                self.bank.save(&mut w, &self.names);
                self.keep = Some(w.0);
            }
        }
        self.told(out);
    }

    /// The page's sections listed again, each with its preferences as the page takes them.
    fn relist(&mut self) {
        if self.shown.is_empty() {
            return;
        }
        self.serial += 1;
        let sections = self
            .shown
            .iter()
            .map(|s| {
                let prefs = self
                    .bank
                    .list(*s)
                    .into_iter()
                    .filter_map(|p| match p {
                        Pref::Flag(f) => Some(Change::Flag(f)),
                        Pref::Number(n) => Some(Change::Number(n)),
                        Pref::Choice(c) => Some(Change::Choice(c)),
                        Pref::Text(_) => None,
                    })
                    .collect();
                (*s, prefs)
            })
            .collect();
        self.listing = Some(Listing {
            serial: self.serial,
            sections,
        });
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (how the client's own preferences reach the options window's client page)
    use super::*;
    use auberean_draw::quality::{key as gkey, FIRST_RUN};

    fn choice(l: &Listing, k: u32) -> Choice {
        l.sections
            .iter()
            .flat_map(|(_, p)| p.iter())
            .find_map(|c| match c {
                Change::Choice(c) if c.description.key == k => Some(c.clone()),
                _ => None,
            })
            .expect("listed")
    }

    #[test]
    fn the_graphics_section_lists_tries_applies_and_recalls() {
        let mut p = Preferences::default();
        assert_eq!(p.quality, FIRST_RUN, "a fresh install's");
        assert!(p.listing().is_none(), "nothing listed until asked");
        p.handle(&Request::Attach(vec![0x4000_000B, GRAPHICS]));
        let l = p.listing().expect("listed").clone();
        assert_eq!(l.sections.len(), 2);
        assert_eq!(l.sections[1].1.len(), PREFERENCES.len());
        let texture = choice(&l, gkey::TEXTURE_DETAIL);
        assert_eq!(texture.get(), 3);
        assert_eq!(texture.list_choices().len(), 5);
        assert_eq!(texture.description.table, PREFERENCE_STRINGS);
        // A lower texture detail tried, then applied: no quality matches it.
        let mut lower = texture.clone();
        lower.set_value(2);
        p.handle(&Request::Change(Change::Choice(lower)));
        assert_eq!(p.quality, FIRST_RUN, "tried, not applied");
        p.handle(&Request::Apply(GRAPHICS));
        assert_eq!(p.quality, 0);
        assert_eq!(
            p.take_applied().and_then(|g| g.get(gkey::TEXTURE_DETAIL)),
            Some(2.0)
        );
        // Tried back to high and recalled: the applied value handed back, the page left as it is.
        let mut back = texture;
        back.set_value(3);
        p.handle(&Request::Change(Change::Choice(back)));
        p.handle(&Request::Recall(GRAPHICS));
        assert_eq!(
            p.take_applied().and_then(|g| g.get(gkey::TEXTURE_DETAIL)),
            Some(2.0)
        );
        assert_eq!(p.listing().expect("listed").serial, l.serial);
        p.handle(&Request::Detach(vec![0x4000_000B, GRAPHICS]));
        assert!(p.listing().is_none());
    }

    #[test]
    fn the_avatars_looks_applied_reach_the_camera() {
        let mut p = Preferences::default();
        p.handle(&Request::Attach(vec![section::AVATAR]));
        let l = p.listing().expect("listed").clone();
        let mut f = l
            .sections
            .iter()
            .flat_map(|(_, p)| p.iter())
            .find_map(|c| match c {
                Change::Flag(f) if f.description.key == key::AVATAR_LOOK_AT_SELECTION => Some(*f),
                _ => None,
            })
            .expect("listed");
        assert!(f.get(), "on by default");
        f.set_value(false);
        p.handle(&Request::Change(Change::Flag(f)));
        p.handle(&Request::Apply(section::AVATAR));
        let o = p.take_camera().expect("applied");
        assert!(!o.look_at_selection && o.look_around && o.look_at_camera);
    }

    #[test]
    fn the_sound_section_lists_and_its_changes_reach_the_sound() {
        let mut p = Preferences::default();
        p.handle(&Request::Attach(vec![sound::SECTION]));
        let l = p.listing().expect("listed").clone();
        let listed: Vec<u32> = l.sections[0]
            .1
            .iter()
            .map(|c| match c {
                Change::Flag(f) => f.description.key,
                Change::Number(n) => n.description.key,
                Change::Choice(c) => c.description.key,
            })
            .collect();
        let keys: Vec<u32> = sound::PREFERENCES.iter().map(|q| q.key).collect();
        assert_eq!(listed, keys, "the manager's eight, in its order");
        let mut music = l.sections[0]
            .1
            .iter()
            .find_map(|c| match c {
                Change::Number(n) if n.description.key == sound::key::MUSIC_VOLUME => Some(*n),
                _ => None,
            })
            .expect("listed");
        assert_eq!(music.get(), 1.0, "full by default");
        // A slider moved reaches the sound at once.
        music.set_value(0.5);
        p.handle(&Request::Change(Change::Number(music)));
        assert_eq!(p.take_sound(), vec![(sound::key::MUSIC_VOLUME, 0.5, false)]);
        // Music off ticked, then applied: changed, then applied.
        let mut off = l.sections[0]
            .1
            .iter()
            .find_map(|c| match c {
                Change::Flag(f) if f.description.key == sound::key::MUSIC_OFF => Some(*f),
                _ => None,
            })
            .expect("listed");
        off.set_value(true);
        p.handle(&Request::Change(Change::Flag(off)));
        p.handle(&Request::Apply(sound::SECTION));
        let told = p.take_sound();
        assert_eq!(told[0], (sound::key::MUSIC_OFF, 1.0, false));
        assert!(told.contains(&(sound::key::MUSIC_OFF, 1.0, true)));
    }

    #[test]
    fn the_sound_takes_the_values_read_as_it_starts() {
        let mut p = Preferences::default();
        p.start_sound();
        let told = p.take_sound();
        assert_eq!(told.len(), sound::AT_START.len());
        assert!(told.contains(&(sound::key::PLAYING_SOUNDS, 32.0, false)));
        // The features as applied (their value read through the data's names, none here).
        assert!(told
            .iter()
            .any(|&(k, _, applied)| k == sound::key::FEATURES && applied));
        assert!(told.contains(&(sound::key::SOUND_OFF, 0.0, true)));
    }
}
