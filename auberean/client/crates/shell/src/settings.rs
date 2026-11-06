//! The player's settings, kept as text the host stores (a `Preferences.ini` in the settings folder
//! on the desktop): sections of `name=value` lines ([`crate::profile`]), named as the September 2013
//! client names its camera, mouse and graphics options. Unknown lines are kept as they were; a value
//! that does not read, or is out of its option's range, leaves the default.

use auberean_dat::tree::number;
use auberean_draw::quality::{Graphics, Kind, HIGHEST, OVERALL_NAME, PREFERENCES};
use auberean_scene::camera::{AvatarInFirstPerson, Options};

use crate::profile::Profile;

/// The settings the shell uses.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub camera: Options,
    /// The graphics preferences, when the settings hold any (or the player applied some).
    pub graphics: Option<Graphics>,
    /// The window's drawing area, in pixels.
    pub window: (u32, u32),
    /// Full screen, or a window.
    pub full_screen: bool,
    /// In full screen, a third buffer to draw into.
    pub triple_buffering: bool,
    /// In full screen, each picture waits for the display's refresh.
    pub sync_to_display: bool,
    /// The edges smoothed.
    pub antialiasing: bool,
    /// The land's squares blend their corners' grounds.
    pub terrain_blending: bool,
    /// The files the key maps are kept in.
    pub key_files: crate::keys::KeyFiles,
    /// The text read, for writing back what this program does not use.
    profile: Profile,
}

/// How the game is shown on the screen, as the rendering preferences ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Presentation {
    /// The size of the picture in full screen, the window's drawing area otherwise, in pixels.
    pub size: (u32, u32),
    /// Full screen, or a window.
    pub full_screen: bool,
    /// In full screen, a third buffer to draw into.
    pub triple_buffering: bool,
    /// In full screen, each picture waits for the display's refresh.
    pub sync_to_display: bool,
    /// The edges smoothed.
    pub antialiasing: bool,
}

impl Presentation {
    /// The size of the picture shown in a window of this size: in full screen the size asked for,
    /// scaled to the monitor, otherwise the window's own.
    #[must_use]
    pub const fn drawn_in(&self, window: (u32, u32)) -> (u32, u32) {
        if self.full_screen {
            self.size
        } else {
            window
        }
    }

    /// Where a picture of size `drawn` shows in a window of size `window`, in the window's pixels
    /// (left, top, width, height): in full screen as large as it goes at its own shape in the
    /// middle, black bars around it; a window it fills.
    #[must_use]
    pub fn shown_in(&self, drawn: (u32, u32), window: (u32, u32)) -> [f32; 4] {
        if self.full_screen {
            crate::interface::fitted(drawn, window)
        } else {
            #[allow(clippy::cast_precision_loss)]
            [0.0, 0.0, window.0 as f32, window.1 as f32]
        }
    }
}

/// A window's size in the screen's pixels as the pixels of the picture it shows, at this many screen
/// pixels to one of the picture's.
#[must_use]
pub fn in_picture_pixels(window: (u32, u32), scale: f64) -> (u32, u32) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let along = |w: u32| (f64::from(w) / scale).round().max(1.0) as u32;
    (along(window.0), along(window.1))
}

/// A point of a window as the point of the picture of size `picture` that shows in part `shown` of
/// it ([`Presentation::shown_in`]). A point on a bar is off the picture.
#[must_use]
pub fn on_picture(point: (f32, f32), shown: [f32; 4], picture: (u32, u32)) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)]
    let along = |p: f32, at: f32, size: f32, to: u32| {
        if size > 0.0 {
            (p - at) * to as f32 / size
        } else {
            p - at
        }
    };
    (
        along(point.0, shown[0], shown[2], picture.0),
        along(point.1, shown[1], shown[3], picture.1),
    )
}

/// A point off the picture, on a bar, as the nearest point of it, and whether it was on the picture.
#[must_use]
pub fn held_to_picture(point: (f32, f32), picture: (u32, u32)) -> ((f32, f32), bool) {
    #[allow(clippy::cast_precision_loss)]
    let along = |p: f32, size: u32| p.clamp(0.0, size.max(1) as f32 - 1.0);
    let held = (along(point.0, picture.0), along(point.1, picture.1));
    #[allow(clippy::cast_precision_loss)]
    let on = point.0 >= 0.0
        && point.1 >= 0.0
        && point.0 < picture.0 as f32
        && point.1 < picture.1 as f32;
    (held, on)
}

/// A point of the picture of size `picture` as the point of the window where it shows: the other
/// way from [`on_picture`].
#[must_use]
pub fn on_window(point: (f32, f32), shown: [f32; 4], picture: (u32, u32)) -> (f32, f32) {
    #[allow(clippy::cast_precision_loss)]
    let along = |p: f32, at: f32, size: f32, from: u32| at + p * size / from.max(1) as f32;
    (
        along(point.0, shown[0], shown[2], picture.0),
        along(point.1, shown[1], shown[3], picture.1),
    )
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            camera: Options::default(),
            graphics: None,
            window: (1280, 960),
            full_screen: false,
            triple_buffering: false,
            sync_to_display: true,
            antialiasing: true,
            terrain_blending: true,
            key_files: crate::keys::KeyFiles::default(),
            profile: Profile::default(),
        }
    }
}

/// An option: its section, its name, its range.
const CAMERA_DISTANCE: (&str, &str, f32, f32) = ("Camera", "CameraDistance", -0.2, 10.0);
const CAMERA_TILT: (&str, &str, f32, f32) = ("Camera", "CameraTilt", -0.25, 1.0);
const ROTATE: (&str, &str, f32, f32) = ("Camera", "CameraRotateMultiplier", 0.1, 5.0);
const ZOOM: (&str, &str, f32, f32) = ("Camera", "CameraZoomMultiplier", 0.1, 5.0);
const CAMERA_HEIGHT: (&str, &str, f32, f32) = ("Camera", "CameraHeight", -1.0, 1.0);
const TRANSITION: (&str, &str, f32, f32) = ("Camera", "CameraTransitionTime", 0.1, 2.0);
const AUTO_DISTANCE: (&str, &str) = ("Camera", "CameraAutoDistance");
const ALIGN_TO_SLOPE: (&str, &str) = ("Camera", "CameraAlignToSlope");
const SENSITIVITY: (&str, &str, f32, f32) = ("Input", "MouseLookSensitivity", 0.5, 2.0);
const INVERT_Y: (&str, &str) = ("Input", "MouseInvertY");
const STEERS: (&str, &str) = ("Input", "MouseLookSteersPlayer");
const AVATAR: (&str, &str) = ("Avatar", "AvatarVisibleInFirstPerson");
const LOOK_AROUND: (&str, &str) = ("Avatar", "AvatarLookAround");
const LOOK_AT_CAMERA: (&str, &str) = ("Avatar", "AvatarLookAtCamera");
const LOOK_AT_SELECTION: (&str, &str) = ("Avatar", "AvatarLookAtSelection");
const HOVER_TRANSPARENCY: (&str, &str) = ("Avatar", "AvatarHoverTransparency");
/// The display's size, its width above its height in one number, and full screen; an earlier
/// version of this program kept the size as two numbers of its own, still read.
const TERRAIN_BLENDING: &str = "LandscapeTerrainBlending";
const DISPLAY_RESOLUTION: (&str, &str) = ("Rendering", "DisplayResolution");
const FULL_SCREEN: (&str, &str) = ("Rendering", "FullScreen");
const TRIPLE_BUFFERING: (&str, &str) = ("Rendering", "TripleBuffering");
const SYNC_TO_DISPLAY: (&str, &str) = ("Rendering", "SyncToDisplayRefresh");
const ANTIALIASING: (&str, &str) = ("Rendering", "Antialiasing");
const WIDTH: (&str, &str) = ("Display", "WindowWidth");
/// The graphics preferences' section; each preference is named by its label less the labels'
/// common start, a choice's value by its level's.
const GRAPHICS: &str = "Graphics";
const LABEL: &str = "ID_Graphics_";
const LEVEL: &str = "ID_Graphics_Value_";
const HEIGHT: (&str, &str) = ("Display", "WindowHeight");
/// The key maps' files.
const KEY_NORMAL: (&str, &str) = ("KeyMap", "Normal");
const KEY_SHIFT: (&str, &str) = ("KeyMap", "Shift");
const KEY_CONTROL: (&str, &str) = ("KeyMap", "Control");

/// A yes-or-no as the file holds it: `1` or `0`, anything else not read.
// D2-15942
fn flag_value(v: &str) -> Option<bool> {
    match v {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// A yes-or-no as the file is given it.
// D2-15944
const fn flag_text(v: bool) -> &'static str {
    if v {
        "1"
    } else {
        "0"
    }
}

/// A number as the file holds it: the whole text a number, as the C library reads one.
// D2-15963
#[allow(clippy::cast_possible_truncation)] // kept as single precision
fn number_value(v: &str) -> Option<f32> {
    number::is_double(v.as_bytes()).then(|| number::strtod(v.as_bytes()).value as f32)
}

/// A number as the file is given it: six places after the point, as `%f` writes it.
// D2-15965
fn number_text(v: f32) -> String {
    let mut text = Vec::new();
    auberean_client_sdk::text::wide::append_float(&mut text, v);
    String::from_utf16_lossy(&text)
}

impl Settings {
    /// The graphics preferences the client starts with: those kept, else a fresh install's.
    // D2-09703
    #[must_use]
    pub fn starting_graphics(&self) -> Graphics {
        self.graphics.clone().unwrap_or_else(Graphics::first_run)
    }

    /// The screen's brightness: the preference kept, or none, which leaves the picture as it stands.
    #[must_use]
    pub fn screen_brightness(&self) -> f32 {
        self.graphics
            .as_ref()
            .and_then(|g| g.get(auberean_draw::quality::key::SCREEN_BRIGHTNESS))
            .unwrap_or(0.0)
    }

    /// Read settings from their text.
    #[must_use]
    pub fn read(text: &str) -> Self {
        let mut s = Self {
            profile: Profile::new(text),
            ..Self::default()
        };
        let profile = s.profile.clone();
        let get = |sec: &str, name: &str| profile.value(sec, name);
        let ranged = |o: (&str, &str, f32, f32), default: f32| {
            get(o.0, o.1)
                .and_then(number_value)
                .filter(|v| (o.2..=o.3).contains(v))
                .unwrap_or(default)
        };
        let flag =
            |o: (&str, &str), default: bool| get(o.0, o.1).and_then(flag_value).unwrap_or(default);
        let c = &mut s.camera;
        c.distance = ranged(CAMERA_DISTANCE, c.distance);
        c.tilt = ranged(CAMERA_TILT, c.tilt);
        c.rotate_speed = ranged(ROTATE, c.rotate_speed);
        c.zoom_speed = ranged(ZOOM, c.zoom_speed);
        c.height = ranged(CAMERA_HEIGHT, c.height);
        c.transition_time = ranged(TRANSITION, c.transition_time);
        c.auto_distance = flag(AUTO_DISTANCE, c.auto_distance);
        c.align_to_slope = flag(ALIGN_TO_SLOPE, c.align_to_slope);
        c.mouse_sensitivity = ranged(SENSITIVITY, c.mouse_sensitivity);
        c.invert_y = flag(INVERT_Y, c.invert_y);
        c.mouse_look_steers = flag(STEERS, c.mouse_look_steers);
        c.look_around = flag(LOOK_AROUND, c.look_around);
        c.look_at_camera = flag(LOOK_AT_CAMERA, c.look_at_camera);
        c.look_at_selection = flag(LOOK_AT_SELECTION, c.look_at_selection);
        c.avatar_hover_transparency = flag(HOVER_TRANSPARENCY, c.avatar_hover_transparency);
        // A choice by its name, or by its number.
        let avatar = get(AVATAR.0, AVATAR.1).and_then(|v| {
            [
                AvatarInFirstPerson::Never,
                AvatarInFirstPerson::Always,
                AvatarInFirstPerson::DuringCombat,
            ]
            .into_iter()
            .find(|a| a.name().eq_ignore_ascii_case(v))
            .or_else(|| {
                v.parse::<u32>()
                    .ok()
                    .and_then(AvatarInFirstPerson::of_number)
            })
        });
        c.avatar_in_first_person = avatar.unwrap_or(c.avatar_in_first_person);
        let size = |o: (&str, &str), d: u32| {
            get(o.0, o.1)
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|v| (320..=16384).contains(v))
                .unwrap_or(d)
        };
        s.window = (size(WIDTH, s.window.0), size(HEIGHT, s.window.1));
        // D2-08325
        let packed = get(DISPLAY_RESOLUTION.0, DISPLAY_RESOLUTION.1)
            .filter(|v| number::is_uint32(v.as_bytes()))
            .map(|v| number::strtoul(v.as_bytes()).value);
        if let Some(r) = packed {
            let (w, h) = (r >> 16, r & 0xFFFF);
            if (320..=16384).contains(&w) && (320..=16384).contains(&h) {
                s.window = (w, h);
            }
        }
        s.full_screen = flag(FULL_SCREEN, s.full_screen);
        s.triple_buffering = flag(TRIPLE_BUFFERING, s.triple_buffering);
        s.sync_to_display = flag(SYNC_TO_DISPLAY, s.sync_to_display);
        s.antialiasing = flag(ANTIALIASING, s.antialiasing);
        // The key maps' files: any name kept, else the default.
        let file = |o: (&str, &str), d: &mut String| {
            if let Some(v) = get(o.0, o.1) {
                v.clone_into(d);
            }
        };
        file(KEY_NORMAL, &mut s.key_files.normal);
        file(KEY_SHIFT, &mut s.key_files.shift);
        file(KEY_CONTROL, &mut s.key_files.control);
        // The graphics section: a choice by its level's name or by its number, a flag, a number
        // within its range.
        let mut graphics = Graphics::default();
        let mut any = false;
        for p in &PREFERENCES {
            let Some(v) = get(GRAPHICS, p.label.trim_start_matches(LABEL)) else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)] // a few levels
            let value = match p.kind {
                Kind::Choice { choices, .. } => choices
                    .iter()
                    .find(|(_, l)| l.trim_start_matches(LEVEL).eq_ignore_ascii_case(v))
                    .map(|(n, _)| *n)
                    .or_else(|| v.parse::<i32>().ok())
                    .map(|n| n as f32),
                Kind::Flag { .. } => flag_value(v).map(|on| f32::from(u8::from(on))),
                Kind::Number {
                    least, greatest, ..
                } => number_value(v).filter(|x| (least..=greatest).contains(x)),
            };
            if let Some(x) = value {
                any |= graphics.set(p.key, x);
            }
        }
        // D2-10525
        if let Some(on) = get(GRAPHICS, TERRAIN_BLENDING).and_then(|v| flag_value(v.trim())) {
            s.terrain_blending = on;
        }
        // A kept overall quality (1 to 5) sets its own preferences over those read.
        if let Some(q) = get(GRAPHICS, OVERALL_NAME).and_then(|v| v.trim().parse::<u32>().ok()) {
            if (1..=HIGHEST).contains(&q) {
                graphics.set_overall(q);
                any = true;
            }
        }
        s.graphics = any.then_some(graphics);
        s
    }

    /// How the game is shown on the screen now.
    #[must_use]
    pub const fn presentation(&self) -> Presentation {
        Presentation {
            size: self.window,
            full_screen: self.full_screen,
            triple_buffering: self.triple_buffering,
            sync_to_display: self.sync_to_display,
            antialiasing: self.antialiasing,
        }
    }

    /// The game shown on the screen another way, from now on.
    pub const fn set_presentation(&mut self, p: &Presentation) {
        self.window = p.size;
        self.full_screen = p.full_screen;
        self.triple_buffering = p.triple_buffering;
        self.sync_to_display = p.sync_to_display;
        self.antialiasing = p.antialiasing;
    }

    /// The preferences file as read (and kept since).
    #[must_use]
    pub const fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Keep a value in the preferences file (as the client writes one), whether or not this
    /// program uses it.
    pub fn keep(&mut self, section: &str, key: &str, value: &str) {
        self.profile.write(section, key, Some(value));
    }

    /// The settings as text: the text read, with this program's options set to their values in
    /// place and any missing added to their sections, as the client writes each.
    #[must_use]
    pub fn write(&self) -> String {
        let c = &self.camera;
        let b = |v: bool| flag_text(v).to_owned();
        let f = number_text;
        let ours: Vec<(&str, &str, String)> = vec![
            (CAMERA_DISTANCE.0, CAMERA_DISTANCE.1, f(c.distance)),
            (CAMERA_TILT.0, CAMERA_TILT.1, f(c.tilt)),
            (ROTATE.0, ROTATE.1, f(c.rotate_speed)),
            (ZOOM.0, ZOOM.1, f(c.zoom_speed)),
            (CAMERA_HEIGHT.0, CAMERA_HEIGHT.1, f(c.height)),
            (TRANSITION.0, TRANSITION.1, f(c.transition_time)),
            (AUTO_DISTANCE.0, AUTO_DISTANCE.1, b(c.auto_distance)),
            (ALIGN_TO_SLOPE.0, ALIGN_TO_SLOPE.1, b(c.align_to_slope)),
            (SENSITIVITY.0, SENSITIVITY.1, f(c.mouse_sensitivity)),
            (INVERT_Y.0, INVERT_Y.1, b(c.invert_y)),
            (STEERS.0, STEERS.1, b(c.mouse_look_steers)),
            (
                AVATAR.0,
                AVATAR.1,
                c.avatar_in_first_person.name().to_owned(),
            ),
            (LOOK_AROUND.0, LOOK_AROUND.1, b(c.look_around)),
            (LOOK_AT_CAMERA.0, LOOK_AT_CAMERA.1, b(c.look_at_camera)),
            (
                LOOK_AT_SELECTION.0,
                LOOK_AT_SELECTION.1,
                b(c.look_at_selection),
            ),
            (
                HOVER_TRANSPARENCY.0,
                HOVER_TRANSPARENCY.1,
                b(c.avatar_hover_transparency),
            ),
            (
                DISPLAY_RESOLUTION.0,
                DISPLAY_RESOLUTION.1,
                (self.window.0 << 16 | self.window.1 & 0xFFFF).to_string(),
            ),
            (FULL_SCREEN.0, FULL_SCREEN.1, b(self.full_screen)),
            (
                TRIPLE_BUFFERING.0,
                TRIPLE_BUFFERING.1,
                b(self.triple_buffering),
            ),
            (
                SYNC_TO_DISPLAY.0,
                SYNC_TO_DISPLAY.1,
                b(self.sync_to_display),
            ),
            (ANTIALIASING.0, ANTIALIASING.1, b(self.antialiasing)),
            (KEY_NORMAL.0, KEY_NORMAL.1, self.key_files.normal.clone()),
            (KEY_SHIFT.0, KEY_SHIFT.1, self.key_files.shift.clone()),
            (KEY_CONTROL.0, KEY_CONTROL.1, self.key_files.control.clone()),
        ];
        let mut ours = ours;
        if let Some(g) = &self.graphics {
            // D2-16063
            ours.push((GRAPHICS, OVERALL_NAME, g.overall().to_string()));
            for p in &PREFERENCES {
                let Some(v) = g.get(p.key) else {
                    continue;
                };
                #[allow(clippy::cast_possible_truncation)] // a few levels
                let text = match p.kind {
                    Kind::Choice { choices, .. } => {
                        choices.iter().find(|(n, _)| *n == v as i32).map_or_else(
                            || v.to_string(),
                            |(_, l)| l.trim_start_matches(LEVEL).to_owned(),
                        )
                    }
                    Kind::Flag { .. } => b(v != 0.0),
                    Kind::Number { .. } => f(v),
                };
                ours.push((GRAPHICS, p.label.trim_start_matches(LABEL), text));
            }
        }
        let mut profile = self.profile.clone();
        for (sec, name, value) in ours {
            profile.write(sec, name, Some(&value));
        }
        profile.text().to_owned()
    }
}
