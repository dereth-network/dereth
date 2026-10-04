//! The eight `Sound.*` preferences, with their inverted names and their two dead members.
//!
//! Registered in this order at sound start-up and unregistered at shutdown. No preference has a
//! change callback; the variables are polled on every play. The full table is encoded below.
//!
//! **The polarity trap.** The three `*Disabled` INI keys are bound to `*_enabled` variables with no
//! inversion anywhere, so `Sound.SoundDisabled=True` means sound is **on**. That is part of the file
//! format: inverting it here silently flips every existing `UserPreferences.ini`.
//!

use std::collections::BTreeMap;

/// Which of the three category volumes and enables a sound is mixed under.
///
/// The client has no such type — the category is implied by which of the eight entry points was
/// called — but every entry point maps onto exactly one of
/// these, so naming it keeps the mixing table in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Entries 1-4: world objects, animation hooks, physics scripts, the `0xF750` message.
    Effect,
    /// Entries 5-6: `play_ambient_sound` and `play_ambient_sound_from_center`.
    Ambient,
    /// Entries 7-8: the UI `MediaPlayback`, portal transitions, `/environ`.
    Interface,
}

/// The stereo/mono selector, preference `Sound.SoundFeatures`.
///
/// Mono disables panning and changes nothing else — the output is still a stereo DirectSound buffer,
/// just always centred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SoundFeatures {
    #[default]
    Stereo = 0,
    Mono = 1,
}

/// The eight `Sound.*` preferences.
///
/// Two of them are dead and are kept dead deliberately:
///
/// * `interface_volume` (`Sound.InterfaceSoundVolume`) has exactly one reference in the
///   whole client — its registration at start-up. The Options page shows the slider and the INI carries
///   the key; moving it does nothing, because interface sounds are scaled by `effect_volume`.
///
/// * `interface_enabled` is read, but `Sound.InterfaceSoundVolume` is what the *slider* changes, so
///   the pair behaves as "an on/off switch and an inert slider".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prefs {
    /// `Sound.SoundVolume` -> the effect volume. Default 1.0, 0.0..1.0.
    pub effect_volume: f32,
    /// `Sound.AmbientSoundVolume` -> the ambient volume. Default 1.0, 0.0..1.0.
    ///
    /// Applied **twice** — see [`crate::atten::attenuation`] and [`crate::trigger`].
    pub ambient_volume: f32,
    /// `Sound.InterfaceSoundVolume` -> the interface volume. Default 1.0. **DEAD.**
    pub interface_volume: f32,
    /// `Sound.SoundFeatures` -> the stereo/mono selector. Default `Stereo`.
    pub features: SoundFeatures,
    /// `Sound.SoundDisabled` -> the effect enable. **Not inverted**: `True` == on.
    pub effects_enabled: bool,
    /// `Sound.AmbientSoundDisabled` -> the ambient enable. Likewise.
    pub ambient_enabled: bool,
    /// `Sound.InterfaceSoundDisabled` -> the interface enable. Likewise.
    pub interface_enabled: bool,
    /// `Sound.PlaySoundOnlyWhenActive` -> the focus preference. Default true.
    pub only_when_active: bool,
}

impl Default for Prefs {
    /// The defaults from the client's sound-preference registration table.
    fn default() -> Self {
        Self {
            effect_volume: 1.0,
            ambient_volume: 1.0,
            interface_volume: 1.0,
            features: SoundFeatures::Stereo,
            effects_enabled: true,
            ambient_enabled: true,
            interface_enabled: true,
            only_when_active: true,
        }
    }
}

/// The eight INI keys, in retail's registration order. Exported so a test can assert the set.
pub const SOUND_KEYS: [&str; 8] = [
    "Sound.SoundVolume",
    "Sound.AmbientSoundVolume",
    "Sound.InterfaceSoundVolume",
    "Sound.SoundFeatures",
    "Sound.SoundDisabled",
    "Sound.AmbientSoundDisabled",
    "Sound.InterfaceSoundDisabled",
    "Sound.PlaySoundOnlyWhenActive",
];

/// The span the Options page's volume sliders cover: the client's Sound table registers `0.0 …
/// 1.0` for all three volumes.
///
/// It is the **slider's** span and not a validator. The client pushes
/// whatever the INI holds straight into the variable through its global-variable setter, and
/// [`crate::atten::attenuation`] caps the distance term at `1.0` *before* multiplying the category
/// volume in — so a hand-edited `SoundVolume=2.0` really does come out 6 dB hot. Do not clamp on
/// load; see `a_volume_above_the_slider_range_is_not_clamped`.
pub const VOLUME_SLIDER_RANGE: (f32, f32) = (0.0, 1.0);

/// A `UserPreferences` value in the three preference value types the eight `Sound.*` entries use.
///
/// `dereth-audio` cannot see `dereth_ui_screens::PrefValue` (the UI sits above it and this crate
/// has no UI dependency), so the host converts. The fourth type, `PString`, is deliberately absent:
/// no sound preference is a string.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrefScalar {
    Bool(bool),
    /// An unsigned 32-bit preference value — only `Sound.SoundFeatures`.
    Int(i32),
    Float(f32),
}

/// What [`Prefs::set_named`] did. **Three states, not two**: "this is not a sound preference" and
/// "this is a sound preference and the caller passed the wrong type" must not look alike, because
/// the second is a bug at the seam and the first is the normal case for the other 35 preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefWrite {
    /// The field was written.
    Applied,
    /// One of [`SOUND_KEYS`], but the value carried the wrong preference value type.
    WrongType,
    /// Not one of [`SOUND_KEYS`]. `Render.FieldOfView` lands here and that is correct.
    NotASoundPreference,
}

impl Prefs {
    /// Write one preference by its `UserPreferences` name — the rebuild's stand-in for the
    /// client's preference registration having bound the variable **by address**.
    ///
    /// All eight preferences bind directly to the audio system's globals with
    /// **no change callback**. The sound manager reads the variables
    /// directly, so in the original the option control's write **is** the global's
    /// write. There is nothing to notify and nothing to re-read: the new value is in force for the
    /// next playback request, and every already-playing sound buffer keeps the gain
    /// the play path gave it, because the volume is set once at play time and never
    /// changed mid-play. That is why this returns nothing to schedule.
    ///
    /// Names are matched case-insensitively: loading step 5 lower-cases every name before storing it.
    ///
    /// The `*Disabled` polarity is **not** inverted here, for the same reason
    /// [`Self::from_ini`] does not invert it.
    pub fn set_named(&mut self, name: &str, value: PrefScalar) -> PrefWrite {
        let float = |v: PrefScalar| match v {
            PrefScalar::Float(f) => Some(f),
            _ => None,
        };
        let boolean = |v: PrefScalar| match v {
            PrefScalar::Bool(b) => Some(b),
            _ => None,
        };
        macro_rules! put {
            ($field:ident, $get:ident) => {
                match $get(value) {
                    Some(v) => {
                        self.$field = v;
                        PrefWrite::Applied
                    }
                    None => PrefWrite::WrongType,
                }
            };
        }
        // The eight, in retail's registration order.
        if name.eq_ignore_ascii_case(SOUND_KEYS[0]) {
            put!(effect_volume, float)
        } else if name.eq_ignore_ascii_case(SOUND_KEYS[1]) {
            put!(ambient_volume, float)
        } else if name.eq_ignore_ascii_case(SOUND_KEYS[2]) {
            put!(interface_volume, float)
        } else if name.eq_ignore_ascii_case(SOUND_KEYS[3]) {
            // A `UInt32` preference with two choices. An unknown ordinal keeps the
            // current value rather than inventing a third mode.
            match value {
                PrefScalar::Int(0) => {
                    self.features = SoundFeatures::Stereo;
                    PrefWrite::Applied
                }
                PrefScalar::Int(1) => {
                    self.features = SoundFeatures::Mono;
                    PrefWrite::Applied
                }
                _ => PrefWrite::WrongType,
            }
        } else if name.eq_ignore_ascii_case(SOUND_KEYS[4]) {
            put!(effects_enabled, boolean)
        } else if name.eq_ignore_ascii_case(SOUND_KEYS[5]) {
            put!(ambient_enabled, boolean)
        } else if name.eq_ignore_ascii_case(SOUND_KEYS[6]) {
            put!(interface_enabled, boolean)
        } else if name.eq_ignore_ascii_case(SOUND_KEYS[7]) {
            put!(only_when_active, boolean)
        } else {
            PrefWrite::NotASoundPreference
        }
    }
}

impl Prefs {
    /// The category volume [`crate::atten::attenuation`] multiplies in.
    ///
    /// The client's attenuation takes only an is-ambient flag and picks between the ambient
    /// volume and the effect volume. Interface therefore lands on the *effect* volume: both
    /// interface play paths pass
    /// an is-ambient flag of 0.
    #[must_use]
    pub fn category_volume(&self, cat: Category) -> f32 {
        match cat {
            Category::Ambient => self.ambient_volume,
            Category::Effect | Category::Interface => self.effect_volume,
        }
    }

    /// The category enable flag every entry point tests before doing anything.
    ///
    /// The eight entry points in [`crate::AudioSystem`] call this rather than reading the three
    /// booleans by hand, so the sentence above is a fact about the code and not a claim about it.
    #[must_use]
    pub fn category_enabled(&self, cat: Category) -> bool {
        match cat {
            Category::Effect => self.effects_enabled,
            Category::Ambient => self.ambient_enabled,
            Category::Interface => self.interface_enabled,
        }
    }

    /// Read the `Sound` section out of a `UserPreferences.ini`.
    ///
    /// The parse mirrors the client's load, step 4, including the documented
    /// corruption of values containing `=`: the line is split on every `=` and the parts after the
    /// first are concatenated **without** the separators. Keys absent from the file keep the
    /// registered default, because registration only overwrites when a shadow value exists.
    ///
    /// the client writes booleans through the `KW_TRUE`/`KW_FALSE` globals and
    /// `KW_TRUE` is the literal `"True"`, but which arm the client takes for booleans is not
    /// settled. Both that spelling and the numeric one are accepted on load, which is right under
    /// either answer; see `dereth-client`'s `config.rs`, which resolves it the same way.
    #[must_use]
    pub fn from_ini(text: &str) -> Self {
        let values = parse_profile(text);
        let mut p = Self::default();
        let get = |k: &str| values.get(&k.to_ascii_lowercase()).map(String::as_str);
        if let Some(v) = get("Sound.SoundVolume").and_then(parse_f32) {
            p.effect_volume = v;
        }
        if let Some(v) = get("Sound.AmbientSoundVolume").and_then(parse_f32) {
            p.ambient_volume = v;
        }
        if let Some(v) = get("Sound.InterfaceSoundVolume").and_then(parse_f32) {
            p.interface_volume = v;
        }
        if let Some(v) = get("Sound.SoundFeatures").and_then(parse_features) {
            p.features = v;
        }
        if let Some(v) = get("Sound.SoundDisabled").and_then(parse_bool) {
            p.effects_enabled = v;
        }
        if let Some(v) = get("Sound.AmbientSoundDisabled").and_then(parse_bool) {
            p.ambient_enabled = v;
        }
        if let Some(v) = get("Sound.InterfaceSoundDisabled").and_then(parse_bool) {
            p.interface_enabled = v;
        }
        if let Some(v) = get("Sound.PlaySoundOnlyWhenActive").and_then(parse_bool) {
            p.only_when_active = v;
        }
        p
    }

    /// Serialize the eight Sound preferences in registration order under `[Sound]`.
    /// Booleans use `True`/`False`, and volumes use the `f32` display format.
    /// The preference-name prefix is omitted from each key.
    #[must_use]
    pub fn to_ini_section(&self) -> String {
        let b = |v: bool| if v { "True" } else { "False" };
        let f = |v: f32| format!("{v}");
        let feat = match self.features {
            SoundFeatures::Stereo => "Stereo",
            SoundFeatures::Mono => "Mono",
        };
        format!(
            "[Sound]\nSoundVolume={}\nAmbientSoundVolume={}\nInterfaceSoundVolume={}\n\
             SoundFeatures={}\nSoundDisabled={}\nAmbientSoundDisabled={}\n\
             InterfaceSoundDisabled={}\nPlaySoundOnlyWhenActive={}\n",
            f(self.effect_volume),
            f(self.ambient_volume),
            f(self.interface_volume),
            feat,
            b(self.effects_enabled),
            b(self.ambient_enabled),
            b(self.interface_enabled),
            b(self.only_when_active),
        )
    }
}

/// The client's own steps 3-5, lower-cased for lookup. A name with no section goes
/// to `Default`, matching `Save`'s split-at-the-last-dot rule in reverse.
fn parse_profile(text: &str) -> BTreeMap<String, String> {
    // ORDER-OK: a BTreeMap keyed by preference name and only ever looked up by name; the registry's
    // own iteration order is not observable here because this crate reads only the eight Sound keys.
    let mut out = BTreeMap::new();
    let mut section = String::from("Default");
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            if let Some(name) = rest.strip_suffix(']') {
                section = name.to_string();
            }
            continue;
        }
        let mut parts = line.split('=');
        let Some(key) = parts.next() else { continue };
        let value: String = parts.collect();
        out.insert(format!("{section}.{key}").to_ascii_lowercase(), value);
    }
    out
}

fn parse_f32(v: &str) -> Option<f32> {
    v.trim().parse::<f32>().ok()
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim() {
        s if s.eq_ignore_ascii_case("true") || s == "1" => Some(true),
        s if s.eq_ignore_ascii_case("false") || s == "0" => Some(false),
        _ => None,
    }
}

/// `Sound.SoundFeatures` is a `UInt32` with two choices, `Stereo` and `Mono`.
/// The client's variable query writes the choice *label* when one matches, so both spellings must load.
fn parse_features(v: &str) -> Option<SoundFeatures> {
    match v.trim() {
        s if s.eq_ignore_ascii_case("mono") || s == "1" => Some(SoundFeatures::Mono),
        s if s.eq_ignore_ascii_case("stereo") || s == "0" => Some(SoundFeatures::Stereo),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the eight recovered Sound preference rows, their defaults, and the inverted
    /// `SoundDisabled` polarity.
    #[test]
    fn sound_disabled_true_leaves_sound_on() {
        let p = Prefs::from_ini("[Sound]\nSoundDisabled=True\nAmbientSoundDisabled=True\n");
        assert!(
            p.effects_enabled,
            "Sound.SoundDisabled=True means sound is ON"
        );
        assert!(p.ambient_enabled);
        let off = Prefs::from_ini("[Sound]\nSoundDisabled=False\n");
        assert!(!off.effects_enabled);
    }

    /// A whole file round-trips: the eight keys parse, and re-emitting then re-parsing is a fixed
    /// point. Oracle: section 5.3's load and save rules and the section 5.4 defaults.
    #[test]
    fn a_realistic_preferences_file_round_trips() {
        // Shaped like a real UserPreferences.ini: several sections, and keys this crate ignores.
        let ini = "[Display]\nFullScreen=False\nResolution=83887104\n\
                   [Sound]\nSoundVolume=0.75\nAmbientSoundVolume=0.5\n\
                   InterfaceSoundVolume=0.25\nSoundFeatures=Mono\nSoundDisabled=True\n\
                   AmbientSoundDisabled=False\nInterfaceSoundDisabled=True\n\
                   PlaySoundOnlyWhenActive=False\n[Render]\nFieldOfView=90.000000\n";
        let p = Prefs::from_ini(ini);
        assert_eq!(p.effect_volume, 0.75);
        assert_eq!(p.ambient_volume, 0.5);
        assert_eq!(p.interface_volume, 0.25);
        assert_eq!(p.features, SoundFeatures::Mono);
        assert!(p.effects_enabled);
        assert!(!p.ambient_enabled);
        assert!(p.interface_enabled);
        assert!(!p.only_when_active);

        let again = Prefs::from_ini(&p.to_ini_section());
        assert_eq!(p, again, "writing then reading must be a fixed point");
    }

    /// Missing keys keep the registered defaults, because registering a preference only overwrites
    /// when the load left a shadow variable behind. Oracle: section 5.2's "load first, register
    /// later".
    #[test]
    fn absent_keys_keep_the_registered_defaults() {
        let p = Prefs::from_ini("[Sound]\nSoundVolume=0.1\n");
        assert_eq!(p.effect_volume, 0.1);
        assert_eq!(
            Prefs::default(),
            Prefs {
                effect_volume: 1.0,
                ..p
            }
        );
    }

    /// The eight keys and their exact spelling are part of the file format.
    #[test]
    fn the_eight_keys_are_named_and_ordered_as_init_prefs_registers_them() {
        assert_eq!(SOUND_KEYS.len(), 8);
        assert_eq!(SOUND_KEYS[0], "Sound.SoundVolume");
        assert_eq!(SOUND_KEYS[7], "Sound.PlaySoundOnlyWhenActive");
        let section = Prefs::default().to_ini_section();
        for k in SOUND_KEYS {
            let key = k.strip_prefix("Sound.").expect("Sound-prefixed");
            assert!(
                section.contains(&format!("{key}=")),
                "{k} missing from the written section"
            );
        }
    }

    /// `Sound.InterfaceSoundVolume` survives a round trip and changes nothing about the mix.
    /// Oracle: the category-volume table's first quirk.
    #[test]
    fn the_interface_volume_is_preserved_and_inert() {
        let p = Prefs {
            interface_volume: 0.0,
            ..Prefs::default()
        };
        assert!(p.to_ini_section().contains("InterfaceSoundVolume=0"));
        // Inert: the Interface category reads the effect slider.
        assert_eq!(p.category_volume(Category::Interface), p.effect_volume);
        assert_eq!(p.category_volume(Category::Effect), p.effect_volume);
        assert_eq!(p.category_volume(Category::Ambient), p.ambient_volume);
    }

    /// Section 5.3 step 4: a value containing `=` is corrupted on load, and that is the behaviour.
    #[test]
    fn a_value_containing_an_equals_sign_is_corrupted_exactly_as_the_original_corrupts_it() {
        let p = Prefs::from_ini("[Sound]\nSoundVolume=0=5\n");
        // "0=5" loads as "05", which parses as 5.0 rather than 0.5 or an error.
        assert_eq!(p.effect_volume, 5.0);
    }

    /// **The literal the rest of the crate is allowed to use the symbol for.**
    ///
    /// A test that reads a constant through the same symbol it writes through cannot detect a
    /// wrong constant. Every other test here says `SOUND_KEYS[1]`; this one spells out all eight
    /// strings and checks their recovered registration order.
    ///
    /// Oracle: the recovered Sound table's eight rows in registration order and its UI-range
    /// column for [`VOLUME_SLIDER_RANGE`].
    #[test]
    fn the_eight_names_and_the_slider_range_are_the_numbers_the_client_registers() {
        assert_eq!(
            SOUND_KEYS,
            [
                "Sound.SoundVolume",
                "Sound.AmbientSoundVolume",
                "Sound.InterfaceSoundVolume",
                "Sound.SoundFeatures",
                "Sound.SoundDisabled",
                "Sound.AmbientSoundDisabled",
                "Sound.InterfaceSoundDisabled",
                "Sound.PlaySoundOnlyWhenActive",
            ]
        );
        assert_eq!(VOLUME_SLIDER_RANGE, (0.0f32, 1.0f32));
        // The registered defaults, also spelled out rather than read back through `Prefs::default`.
        let d = Prefs::default();
        assert_eq!(d.effect_volume, 1.0);
        assert_eq!(d.ambient_volume, 1.0);
        assert_eq!(d.interface_volume, 1.0);
        assert_eq!(d.features as u32, 0); // 0 = Stereo
        assert!(d.effects_enabled && d.ambient_enabled && d.interface_enabled);
        assert!(d.only_when_active);
    }

    /// Each of the eight names writes **its own** field and nothing else, and the type has to
    /// match. Driven off [`SOUND_KEYS`] so a ninth name cannot be added without a case here.
    #[test]
    fn each_sound_name_writes_exactly_its_own_field() {
        use PrefScalar::{Bool, Float, Int};
        #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
        let cases: [(&str, PrefScalar, fn(&Prefs) -> bool); 8] = [
            (SOUND_KEYS[0], Float(0.25), |p| p.effect_volume == 0.25),
            (SOUND_KEYS[1], Float(0.25), |p| p.ambient_volume == 0.25),
            (SOUND_KEYS[2], Float(0.25), |p| p.interface_volume == 0.25),
            (SOUND_KEYS[3], Int(1), |p| p.features == SoundFeatures::Mono),
            (SOUND_KEYS[4], Bool(false), |p| !p.effects_enabled),
            (SOUND_KEYS[5], Bool(false), |p| !p.ambient_enabled),
            (SOUND_KEYS[6], Bool(false), |p| !p.interface_enabled),
            (SOUND_KEYS[7], Bool(false), |p| !p.only_when_active),
        ];
        assert_eq!(cases.len(), SOUND_KEYS.len());
        for (name, value, check) in cases {
            let mut p = Prefs::default();
            assert_eq!(p.set_named(name, value), PrefWrite::Applied, "{name}");
            assert!(check(&p), "{name} did not write its own field");
            // Exactly one field moved: everything else still equals the default.
            let mut back = p;
            assert_ne!(back, Prefs::default(), "{name} wrote nothing");
            match value {
                Float(_) => {
                    // Put the one field back and the whole struct must be the default again.
                    let d = Prefs::default();
                    if name == SOUND_KEYS[0] {
                        back.effect_volume = d.effect_volume;
                    } else if name == SOUND_KEYS[1] {
                        back.ambient_volume = d.ambient_volume;
                    } else {
                        back.interface_volume = d.interface_volume;
                    }
                }
                Int(_) => back.features = SoundFeatures::Stereo,
                Bool(_) => {
                    let d = Prefs::default();
                    if name == SOUND_KEYS[4] {
                        back.effects_enabled = d.effects_enabled;
                    } else if name == SOUND_KEYS[5] {
                        back.ambient_enabled = d.ambient_enabled;
                    } else if name == SOUND_KEYS[6] {
                        back.interface_enabled = d.interface_enabled;
                    } else {
                        back.only_when_active = d.only_when_active;
                    }
                }
            }
            assert_eq!(back, Prefs::default(), "{name} moved a second field");
        }
    }

    /// The three states are distinguishable: a foreign name, a sound name with the wrong type, and
    /// a write. A two-state result would make the middle one look like the first.
    #[test]
    fn a_foreign_name_and_a_wrong_type_are_different_answers() {
        let mut p = Prefs::default();
        assert_eq!(
            p.set_named("Render.FieldOfView", PrefScalar::Float(90.0)),
            PrefWrite::NotASoundPreference
        );
        assert_eq!(
            p.set_named("Sound.SoundVolume", PrefScalar::Bool(true)),
            PrefWrite::WrongType,
            "a float preference must reject a Bool"
        );
        assert_eq!(
            p.set_named("Sound.SoundDisabled", PrefScalar::Float(1.0)),
            PrefWrite::WrongType,
            "a bool preference must reject a Float"
        );
        assert_eq!(
            p.set_named("Sound.SoundFeatures", PrefScalar::Int(7)),
            PrefWrite::WrongType,
            "there are exactly two choices "
        );
        assert_eq!(
            p,
            Prefs::default(),
            "a rejected write must not have written"
        );
    }

    /// Registration pushes every name through
    /// The registry lower-cases names as it stores them, so lookup is case-insensitive.
    ///
    /// **All eight arms, not one.** A mutation that made a single arm case-sensitive survived this
    /// test when it only lower-cased `Sound.AmbientSoundVolume`; the reading was *the test is
    /// weak*, and the fix is the loop.
    #[test]
    fn every_name_is_matched_case_insensitively() {
        let typed = |k: &str| {
            if k.ends_with("Volume") {
                PrefScalar::Float(0.5)
            } else if k.ends_with("Features") {
                PrefScalar::Int(1)
            } else {
                PrefScalar::Bool(false)
            }
        };
        for k in SOUND_KEYS {
            let mut p = Prefs::default();
            assert_eq!(
                p.set_named(&k.to_ascii_lowercase(), typed(k)),
                PrefWrite::Applied,
                "{k} was not matched in lower case"
            );
            assert_ne!(p, Prefs::default(), "{k} matched but wrote nothing");
            let mut q = Prefs::default();
            assert_eq!(
                q.set_named(&k.to_ascii_uppercase(), typed(k)),
                PrefWrite::Applied,
                "{k} was not matched in upper case"
            );
            assert_eq!(p, q, "{k} wrote different fields in different cases");
        }
    }

    /// `Sound.SoundDisabled` keeps its inverted sense through the by-name path too: writing `true`
    /// leaves sound **on**. Oracle: §5.4's polarity trap.
    #[test]
    fn the_by_name_path_does_not_invert_the_disabled_polarity() {
        let mut p = Prefs {
            effects_enabled: false,
            ..Prefs::default()
        };
        assert_eq!(
            p.set_named("Sound.SoundDisabled", PrefScalar::Bool(true)),
            PrefWrite::Applied
        );
        assert!(
            p.effects_enabled,
            "Sound.SoundDisabled=True means sound is ON"
        );
    }

    /// The slider range is the slider's, not a validator: nothing clamps, and
    /// [`crate::atten::attenuation`] caps the *distance* term before the category volume is
    /// multiplied in, so a value above 1.0 really is louder.
    #[test]
    fn a_volume_above_the_slider_range_is_not_clamped() {
        let mut p = Prefs::default();
        assert_eq!(
            p.set_named("Sound.SoundVolume", PrefScalar::Float(2.0)),
            PrefWrite::Applied
        );
        assert_eq!(p.effect_volume, 2.0);
        assert!(p.effect_volume > VOLUME_SLIDER_RANGE.1);
        let hot = crate::atten::attenuation(0.0, 1.0, Category::Effect, &p).expect("audible");
        let flat = crate::atten::attenuation(0.0, 1.0, Category::Effect, &Prefs::default())
            .expect("audible");
        assert_eq!(flat, 0, "a full slider at zero distance is 0 dB");
        assert!(
            hot > flat,
            "the cap is on the distance term, not on the category volume"
        );
    }
}
