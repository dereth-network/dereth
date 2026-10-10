//! The Horizon interface's own settings: the interface scale, the camera's movement scheme and
//! pointer directions, whether bodies are drawn between their animations' keyframes, and the log
//! window's tab names, opacities and where its tabs stand popped out. They are kept in a small file of their own
//! (`horizon.txt`) beside the preferences, and System Configuration changes them live.
//!
//! [`HorizonOptions`] also carries what a test or a scripted run starts the interface with: the first
//! screen, windows to open, an automatic log-in and steps to take once in the world.

/// The settings file's name, beside the preferences.
pub const FILE_NAME: &str = "horizon.txt";

/// One scripted step, taken once in the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptStep {
    /// A line typed into the log window and sent.
    Say(String),
    /// An action fired, as a key bound to it would.
    Action(u32),
    /// The nearest object on the radar with this name selected, as a click on it would.
    Select(String),
}

/// The first screen to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartScreen {
    /// Character select.
    #[default]
    Lobby,
    Game,
}

/// The interface's settings, and how a run starts it.
#[derive(Debug, Clone, PartialEq)]
pub struct HorizonOptions {
    /// The interface scale, 0.5 to 3; `None` is 1.
    pub scale: Option<f32>,
    pub screen: StartScreen,
    pub open: Vec<String>,
    pub auto_login: bool,
    /// The character to log in automatically.
    pub start_character: Option<String>,
    /// The scripted steps to take once in the world, in order.
    pub script: Vec<ScriptStep>,
    /// The camera's movement scheme, pointer directions, speeds and limits.
    pub orbit: dereth_client_runtime::orbit::OrbitSettings,
    /// The pad: whether gamepad mode is on, its sticks' dead zone and how fast it turns the camera.
    pub pad: crate::pad::PadSettings,
    /// The minimap turns with the character, rather than standing north up.
    pub minimap_rotates: bool,
    /// Every animated body is drawn between its animation's keyframes, at any frame rate, rather
    /// than at them as the game draws it. Only the drawing changes.
    pub smooth_animation: bool,
    /// The log window's tab names, one for each of the game's chat windows.
    pub chat_tabs: TabNames,
    /// How opaque the log window's ground is, docked and popped out.
    pub chat_opacity: ChatOpacity,
    /// Where each tab popped out was last put: its top-left, in layout units (screen pixels
    /// at scale 1). The main tab never pops out.
    pub chat_popped: [Option<(f32, f32)>; 5],
    /// How large the player made each log window, in layout units: the docked window's (by the
    /// main tab), then each tab's popped out.
    pub chat_sizes: [Option<(f32, f32)>; 5],
}

impl Default for HorizonOptions {
    /// With the keyboard and mouse the movement keys move along the character's facing; in
    /// gamepad mode the left stick moves where the camera looks (the pad's own setting).
    fn default() -> Self {
        Self {
            scale: None,
            screen: StartScreen::default(),
            open: Vec::new(),
            auto_login: false,
            start_character: None,
            script: Vec::new(),
            orbit: dereth_client_runtime::orbit::OrbitSettings {
                movement: dereth_client_runtime::orbit::MovementMode::Character,
                ..dereth_client_runtime::orbit::OrbitSettings::default()
            },
            pad: crate::pad::PadSettings::default(),
            minimap_rotates: false,
            smooth_animation: false,
            chat_tabs: TabNames::default(),
            chat_opacity: ChatOpacity::default(),
            chat_popped: [None; 5],
            chat_sizes: [None; 5],
        }
    }
}

impl HorizonOptions {
    /// The camera's settings in effect: the pad's movement scheme in gamepad mode, the
    /// keyboard's otherwise.
    #[must_use]
    pub fn orbit_in_effect(&self) -> dereth_client_runtime::orbit::OrbitSettings {
        let mut o = self.orbit;
        if self.pad.enabled {
            o.movement = self.pad.movement;
        }
        o
    }
}

/// The settings file's key for how large log window `slot` was made: the docked window's for
/// the main tab, else that tab's popped out.
#[must_use]
pub fn size_key(slot: usize) -> String {
    format!("chat-tab-{}-size", slot + 1)
}

/// The log window a settings file key for a size names.
#[must_use]
pub fn size_slot(key: &str) -> Option<usize> {
    (0..5).find(|slot| size_key(*slot) == key)
}

/// The settings file's key for where tab `slot` stands popped out.
#[must_use]
pub fn popped_key(slot: usize) -> String {
    format!("chat-tab-{}-at", slot + 1)
}

/// The tab a settings file key for a popped-out tab's place names.
#[must_use]
pub fn popped_slot(key: &str) -> Option<usize> {
    (1..5).find(|slot| popped_key(*slot) == key)
}

/// A place as the settings file writes it, `x,y`.
#[must_use]
pub fn parse_point(value: &str) -> Option<(f32, f32)> {
    let (x, y) = value.split_once(',')?;
    let (x, y) = (x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?);
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

/// How opaque a log window's ground is until the player sets it, active or not: the docked
/// window's first ground.
pub const DEFAULT_CHAT_OPACITY: f32 = 144.0 / 255.0;

/// How opaque the log window's ground is, while it is active (the pointer over it, or its line
/// being typed) and while it is not: the docked window's, and each tab's when popped out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChatOpacity {
    /// The docked window's: active, then inactive.
    pub docked: [f32; 2],
    /// Each tab's popped out, by tab: active, then inactive. The main tab never pops out.
    pub undocked: [[f32; 2]; 5],
}

impl Default for ChatOpacity {
    fn default() -> Self {
        Self {
            docked: [DEFAULT_CHAT_OPACITY; 2],
            undocked: [[DEFAULT_CHAT_OPACITY; 2]; 5],
        }
    }
}

impl ChatOpacity {
    /// The settings file's key for an opacity: the docked window's (`slot` `None`) or a tab's
    /// popped out, active or inactive.
    #[must_use]
    pub fn key(slot: Option<usize>, active: bool) -> String {
        let which = if active { "active" } else { "inactive" };
        match slot {
            None => format!("chat-opacity-{which}"),
            Some(slot) => format!("chat-tab-{}-opacity-{which}", slot + 1),
        }
    }

    /// The opacity a settings file key names.
    #[must_use]
    pub fn get(&self, slot: Option<usize>, active: bool) -> f32 {
        let i = usize::from(!active);
        match slot {
            None => self.docked[i],
            Some(slot) => self
                .undocked
                .get(slot)
                .map_or(DEFAULT_CHAT_OPACITY, |o| o[i]),
        }
    }

    /// Set the opacity the settings file's `key` names to `value`, held between 0 and 1;
    /// `false` when the key is not one of these.
    pub fn set_by_key(&mut self, key: &str, value: f32) -> bool {
        if !value.is_finite() {
            return false;
        }
        let value = value.clamp(0.0, 1.0);
        for active in [true, false] {
            let i = usize::from(!active);
            if key == Self::key(None, active) {
                self.docked[i] = value;
                return true;
            }
            for slot in 1..self.undocked.len() {
                if key == Self::key(Some(slot), active) {
                    self.undocked[slot][i] = value;
                    return true;
                }
            }
        }
        false
    }

    /// Every key and value, as the settings file writes them.
    fn entries(&self) -> Vec<(String, f32)> {
        let mut out = Vec::new();
        for slot in std::iter::once(None).chain((1..self.undocked.len()).map(Some)) {
            for active in [true, false] {
                out.push((Self::key(slot, active), self.get(slot, active)));
            }
        }
        out
    }
}

/// The longest name a log window tab keeps, in characters.
pub const TAB_NAME_MAX: usize = 32;

/// The log window's tab names until the player renames them: the main chat window's, then the
/// four floating ones'.
pub const DEFAULT_TAB_NAMES: [&str; 5] = ["Main", "Combat", "Allegiance", "Fellowship", "Global"];

/// The log window's tab names, the main chat window's first, as the player typed them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabNames(pub [String; 5]);

impl Default for TabNames {
    fn default() -> Self {
        Self(DEFAULT_TAB_NAMES.map(str::to_owned))
    }
}

impl TabNames {
    /// The name tab `slot` shows: the one typed, else its default when nothing but spaces was.
    #[must_use]
    pub fn name(&self, slot: usize) -> &str {
        match self.0.get(slot).map(|n| n.trim()) {
            Some(n) if !n.is_empty() => n,
            _ => DEFAULT_TAB_NAMES.get(slot).copied().unwrap_or(""),
        }
    }

    /// Tab `slot` renamed `name`, cut to [`TAB_NAME_MAX`] characters.
    pub fn set(&mut self, slot: usize, name: &str) {
        if let Some(n) = self.0.get_mut(slot) {
            *n = name.chars().take(TAB_NAME_MAX).collect();
        }
    }

    /// The settings file's key for tab `slot`'s name.
    #[must_use]
    pub fn key(slot: usize) -> String {
        format!("chat-tab-{}", slot + 1)
    }

    /// The tab a settings file key names.
    #[must_use]
    pub fn slot_of(key: &str) -> Option<usize> {
        key.strip_prefix("chat-tab-")?
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .filter(|n| *n < DEFAULT_TAB_NAMES.len())
    }
}

impl HorizonOptions {
    /// The settings a settings file holds; a line it cannot read is left out, and its setting
    /// keeps its default.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut o = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim().to_ascii_lowercase().as_str() {
                "scale" => {
                    o.scale = value
                        .parse::<f32>()
                        .ok()
                        .filter(|s| s.is_finite())
                        .map(|s| s.clamp(0.5, 3.0));
                }
                "movement" => {
                    if let Some(m) = parse_movement(value) {
                        o.orbit.movement = m;
                    }
                }
                "reverse-x" => o.orbit.reverse_x = parse_switch(value),
                "reverse-y" => o.orbit.reverse_y = parse_switch(value),
                "sidestep" => o.orbit.sidestep = parse_switch(value),
                "gamepad" => o.pad.enabled = parse_switch(value),
                "gamepad-movement" => {
                    if let Some(m) = parse_movement(value) {
                        o.pad.movement = m;
                    }
                }
                "gamepad-dead-zone" => {
                    if let Some(v) = parse_in(value, crate::pad::DEAD_ZONE_RANGE) {
                        o.pad.dead_zone = v;
                    }
                }
                "gamepad-camera-speed" => {
                    if let Some(v) = parse_in(value, crate::pad::CAMERA_SPEED_RANGE) {
                        o.pad.camera_speed = v;
                    }
                }
                "mouse-turn" => parse_number(value, &mut o.orbit.mouse_turn),
                "key-turn" => parse_number(value, &mut o.orbit.key_turn),
                "tilt-min" => parse_number(value, &mut o.orbit.pitch_min),
                "tilt-max" => parse_number(value, &mut o.orbit.pitch_max),
                "camera-height" => parse_number(value, &mut o.orbit.height),
                "camera-recentre" => parse_number(value, &mut o.orbit.recentre),
                "minimap-rotates" => o.minimap_rotates = parse_switch(value),
                "smooth-animation" => o.smooth_animation = parse_switch(value),
                other => {
                    if let Some(slot) = TabNames::slot_of(other) {
                        let name = if value.is_empty() {
                            DEFAULT_TAB_NAMES[slot]
                        } else {
                            value
                        };
                        o.chat_tabs.set(slot, name);
                    } else if let Some(slot) = popped_slot(other) {
                        o.chat_popped[slot] = parse_point(value);
                    } else if let Some(slot) = size_slot(other) {
                        o.chat_sizes[slot] = parse_point(value);
                    } else if let Ok(v) = value.parse::<f32>() {
                        o.chat_opacity.set_by_key(other, v);
                    }
                }
            }
        }
        o.orbit = o.orbit.held_to_ranges();
        o
    }

    /// The settings file's text.
    #[must_use]
    pub fn to_text(&self) -> String {
        use std::fmt::Write as _;
        let mut text = format!(
            "scale={}
movement={}
reverse-x={}
reverse-y={}
sidestep={}
gamepad={}
gamepad-movement={}
gamepad-dead-zone={}
gamepad-camera-speed={}
mouse-turn={}
key-turn={}
tilt-min={}
tilt-max={}
camera-height={}
camera-recentre={}
minimap-rotates={}
smooth-animation={}
",
            self.scale.unwrap_or(1.0),
            movement_word(self.orbit.movement),
            self.orbit.reverse_x,
            self.orbit.reverse_y,
            self.orbit.sidestep,
            self.pad.enabled,
            movement_word(self.pad.movement),
            self.pad.dead_zone,
            self.pad.camera_speed,
            self.orbit.mouse_turn,
            self.orbit.key_turn,
            self.orbit.pitch_min,
            self.orbit.pitch_max,
            self.orbit.height,
            self.orbit.recentre,
            self.minimap_rotates,
            self.smooth_animation,
        );
        for slot in 0..DEFAULT_TAB_NAMES.len() {
            let _ = writeln!(
                text,
                "{}={}",
                TabNames::key(slot),
                self.chat_tabs.name(slot)
            );
        }
        for (key, value) in self.chat_opacity.entries() {
            let _ = writeln!(text, "{key}={value}");
        }
        for (slot, at) in self.chat_popped.iter().enumerate() {
            if let Some((x, y)) = at {
                let _ = writeln!(text, "{}={x},{y}", popped_key(slot));
            }
        }
        for (slot, size) in self.chat_sizes.iter().enumerate() {
            if let Some((w, h)) = size {
                let _ = writeln!(text, "{}={w},{h}", size_key(slot));
            }
        }
        text
    }

    /// The settings in the file at `path`; the defaults when there is none.
    #[must_use]
    pub fn load(path: &std::path::Path) -> Self {
        dereth_client_runtime::platform::files::read_to_string(path)
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    /// Write the settings to `path`.
    ///
    /// # Errors
    /// When the file cannot be written.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_text())
    }
}

/// A number from the settings file into `into`; one that does not read leaves it as it was.
fn parse_number(value: &str, into: &mut f32) {
    if let Ok(v) = value.parse::<f32>() {
        *into = v;
    }
}

/// A movement scheme's word in the settings file.
#[must_use]
pub fn movement_word(m: dereth_client_runtime::orbit::MovementMode) -> &'static str {
    match m {
        dereth_client_runtime::orbit::MovementMode::Camera => "camera",
        dereth_client_runtime::orbit::MovementMode::Character => "character",
    }
}

/// The movement scheme a settings file's word names.
#[must_use]
pub fn parse_movement(s: &str) -> Option<dereth_client_runtime::orbit::MovementMode> {
    use dereth_client_runtime::orbit::MovementMode;
    match s.trim().to_ascii_lowercase().as_str() {
        "camera" => Some(MovementMode::Camera),
        "character" => Some(MovementMode::Character),
        _ => None,
    }
}

/// A switch's word in the settings file: on for `true`, `yes`, `on` or `1`.
fn parse_switch(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "on" | "1"
    )
}

/// A number a settings file holds, brought into `range`; `None` when it is not a number.
#[must_use]
pub fn parse_in(s: &str, range: (f32, f32)) -> Option<f32> {
    s.trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(range.0, range.1))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    #[test]
    fn bodies_are_drawn_at_their_keyframes_until_smooth_animation_is_set_and_it_reads_back() {
        assert!(!HorizonOptions::default().smooth_animation);
        assert!(!HorizonOptions::parse("").smooth_animation);
        let on = HorizonOptions {
            smooth_animation: true,
            ..HorizonOptions::default()
        };
        assert!(HorizonOptions::parse(&on.to_text()).smooth_animation);
        assert!(!HorizonOptions::parse(&HorizonOptions::default().to_text()).smooth_animation);
    }

    #[test]
    fn the_keyboard_defaults_to_character_based_movement_the_pad_to_camera_based_and_both_read_back(
    ) {
        use dereth_client_runtime::orbit::{MovementMode, OrbitSettings};
        assert_eq!(
            HorizonOptions::default().orbit.movement,
            MovementMode::Character,
            "the keyboard's movement along the character's facing"
        );
        assert_eq!(
            HorizonOptions::default().pad.movement,
            MovementMode::Camera,
            "the pad's where the camera looks"
        );
        let mut both = HorizonOptions::default();
        assert_eq!(both.orbit_in_effect().movement, MovementMode::Character);
        both.pad.enabled = true;
        assert_eq!(
            both.orbit_in_effect().movement,
            MovementMode::Camera,
            "gamepad mode on: the pad's"
        );
        let o = HorizonOptions {
            orbit: OrbitSettings {
                movement: MovementMode::Character,
                reverse_x: true,
                reverse_y: false,
                sidestep: true,
                mouse_turn: 0.008,
                key_turn: 2.5,
                pitch_min: -1.0,
                pitch_max: 0.5,
                height: 0.75,
                recentre: 0.5,
            },
            ..HorizonOptions::default()
        };
        assert_eq!(HorizonOptions::parse(&o.to_text()).orbit, o.orbit);
        assert_eq!(
            HorizonOptions::parse("movement=sideways\nreverse-y=yes").orbit,
            OrbitSettings {
                movement: MovementMode::Character,
                reverse_y: true,
                ..OrbitSettings::default()
            },
            "an unknown scheme keeps the default"
        );
        // Out of range, the camera's numbers are held to their ranges; unreadable, they keep
        // their defaults.
        let held = HorizonOptions::parse(
            "mouse-turn=5
tilt-max=-2
camera-height=lots",
        )
        .orbit;
        assert!(
            (held.mouse_turn - dereth_client_runtime::orbit::limits::MOUSE_TURN.1).abs() < 1e-6
        );
        assert!(held.pitch_max.abs() < 1e-6);
        assert!(held.height.abs() < 1e-6);
    }

    #[test]
    fn the_camera_comes_round_behind_quickly_until_set_and_its_time_reads_back_within_its_range() {
        use dereth_client_runtime::orbit::{limits, RECENTRE};
        assert!((HorizonOptions::default().orbit.recentre - RECENTRE).abs() < 1e-6);
        assert!((HorizonOptions::parse("").orbit.recentre - RECENTRE).abs() < 1e-6);
        for recentre in [0.0, 0.35, limits::RECENTRE.1] {
            let mut o = HorizonOptions::default();
            o.orbit.recentre = recentre;
            let back = HorizonOptions::parse(&o.to_text()).orbit.recentre;
            assert!((back - recentre).abs() < 1e-6, "{recentre}: {back}");
        }
        let read = |line: &str| HorizonOptions::parse(line).orbit.recentre;
        assert!((read("camera-recentre=9") - limits::RECENTRE.1).abs() < 1e-6);
        assert!(read("camera-recentre=-1").abs() < 1e-6);
        assert!((read("camera-recentre=soon") - RECENTRE).abs() < 1e-6);
    }

    #[test]
    fn the_settings_file_reads_back_what_it_was_written_with() {
        let o = HorizonOptions {
            scale: Some(1.5),
            ..HorizonOptions::default()
        };
        assert_eq!(HorizonOptions::parse(&o.to_text()).scale, Some(1.5));
    }

    #[test]
    fn a_renamed_tab_reads_back_and_one_left_blank_keeps_its_default_name() {
        let mut o = HorizonOptions::default();
        assert_eq!(o.chat_tabs.name(1), "Combat");
        o.chat_tabs.set(1, "Fights");
        o.chat_tabs.set(4, "   ");
        let back = HorizonOptions::parse(&o.to_text());
        assert_eq!(back.chat_tabs.name(1), "Fights");
        assert_eq!(back.chat_tabs.name(4), "Global");
        assert_eq!(back.chat_tabs.name(0), "Main");
        assert_eq!(
            HorizonOptions::parse(
                "chat-tab-3=Monarchy
chat-tab-9=Nothing
chat-tab-0=x"
            )
            .chat_tabs
            .0,
            ["Main", "Combat", "Monarchy", "Fellowship", "Global"].map(str::to_owned),
            "only the five tabs have names"
        );
        let long = "x".repeat(50);
        o.chat_tabs.set(0, &long);
        assert_eq!(o.chat_tabs.name(0).len(), TAB_NAME_MAX);
    }

    #[test]
    fn the_log_window_s_opacities_read_back_and_start_as_its_first_ground() {
        let mut o = HorizonOptions::default();
        assert_eq!(o.chat_opacity.get(None, true), DEFAULT_CHAT_OPACITY);
        assert_eq!(o.chat_opacity.get(Some(3), false), DEFAULT_CHAT_OPACITY);
        assert!(o.chat_opacity.set_by_key("chat-opacity-inactive", 0.25));
        assert!(o.chat_opacity.set_by_key("chat-tab-2-opacity-active", 1.5));
        assert!(
            !o.chat_opacity.set_by_key("chat-tab-1-opacity-active", 0.5),
            "the main tab is never popped out"
        );
        let back = HorizonOptions::parse(&o.to_text()).chat_opacity;
        assert_eq!(back.get(None, false), 0.25);
        assert_eq!(back.get(None, true), DEFAULT_CHAT_OPACITY);
        assert_eq!(back.get(Some(1), true), 1.0, "held to fully opaque");
        assert_eq!(back.get(Some(1), false), DEFAULT_CHAT_OPACITY);
    }

    #[test]
    fn where_a_popped_out_tab_was_put_reads_back() {
        let o = HorizonOptions {
            chat_popped: [None, Some((12.5, 40.0)), None, None, Some((0.0, 3.0))],
            ..HorizonOptions::default()
        };
        assert_eq!(
            HorizonOptions::parse(&o.to_text()).chat_popped,
            o.chat_popped
        );
        let o = HorizonOptions {
            chat_sizes: [Some((520.0, 300.0)), None, Some((260.0, 140.0)), None, None],
            ..HorizonOptions::default()
        };
        assert_eq!(HorizonOptions::parse(&o.to_text()).chat_sizes, o.chat_sizes);
        assert_eq!(
            HorizonOptions::parse("chat-tab-1-at=5,5\nchat-tab-3-at=oops").chat_popped,
            [None; 5],
            "the main tab has no place of its own, and an unreadable one is none"
        );
    }

    #[test]
    fn a_setting_the_file_cannot_read_keeps_its_default() {
        let o = HorizonOptions::parse(
            "theme=light
scale=huge
resolution=standard
noise
",
        );
        assert_eq!(
            o,
            HorizonOptions::default(),
            "an older file's theme and resolution lines are ignored too"
        );
        assert_eq!(HorizonOptions::parse("scale=9").scale, Some(3.0));
    }

    #[test]
    fn the_pad_settings_default_to_off_and_read_back_what_was_written_within_their_ranges() {
        let d = HorizonOptions::default().pad;
        assert!(
            !d.enabled,
            "gamepad mode waits for a pad's button, or the setting"
        );
        let o = HorizonOptions {
            pad: crate::pad::PadSettings {
                enabled: true,
                movement: dereth_client_runtime::orbit::MovementMode::Character,
                dead_zone: 0.3,
                camera_speed: 1.75,
            },
            ..HorizonOptions::default()
        };
        assert_eq!(HorizonOptions::parse(&o.to_text()).pad, o.pad);
        let odd = HorizonOptions::parse("gamepad-dead-zone=0.9\ngamepad-camera-speed=fast");
        assert!((odd.pad.dead_zone - crate::pad::DEAD_ZONE_RANGE.1).abs() < 1e-6);
        assert!((odd.pad.camera_speed - d.camera_speed).abs() < 1e-6);
    }
}
