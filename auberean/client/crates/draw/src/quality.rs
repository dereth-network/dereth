//! The graphics preferences and the overall graphics quality.
//!
//! The client keeps twenty-one graphics preferences in its own preferences' graphics section:
//! texture, material and model detail, the water's reflections, the shadows, the landscape's draw
//! and frill distances, the shore's effects, the filtering, the effects, the scenery's draw
//! distance, the adaptive degrade and its bias, and the screen's brightness; each a choice among
//! levels, a yes-or-no one or a number in a range. Five overall qualities, from the lowest to the
//! highest, each set every one of them but the brightness at once, values and defaults. After the
//! preferences change the client works out which of the five they now match (the first that
//! matches every one but the bias; none, 0, is a quality of the player's own) and keeps that.

/// The client's preferences' graphics section.
pub const GRAPHICS: u32 = 0x4000_0004;

/// The graphics preferences' keys.
pub mod key {
    pub const TEXTURE_DETAIL: u32 = 0x4000_004F;
    pub const MATERIAL_DETAIL: u32 = 0x4000_004D;
    pub const MODEL_DETAIL: u32 = 0x4000_004E;
    pub const WATER_REFLECTIONS: u32 = 0x4000_0021;
    pub const LANDSCAPE_SHORE_EFFECTS: u32 = 0x4000_005D;
    pub const DYNAMIC_OBJECT_SHADOWS: u32 = 0x4000_004C;
    pub const SCENERY_DRAW_DISTANCE: u32 = 0x4000_0076;
    pub const LANDSCAPE_DRAW_DISTANCE: u32 = 0x4000_0043;
    pub const LANDSCAPE_FRILL_DISTANCE: u32 = 0x4000_0044;
    pub const LANDSCAPE_STATIC_OBJECT_SHADOWS: u32 = 0x4000_0049;
    pub const LANDSCAPE_CLOUD_SHADOWS: u32 = 0x4000_0020;
    pub const LIGHT_CORONAS: u32 = 0x4000_0042;
    pub const TRILINEAR_FILTERING: u32 = 0x4000_005E;
    pub const ANISOTROPIC_FILTERING: u32 = 0x4000_005F;
    pub const VOLUMETRIC_FOG_EFFECTS: u32 = 0x4000_0074;
    pub const LIGHT_SOURCE_EFFECTS: u32 = 0x4000_0061;
    pub const DECAL_EFFECTS: u32 = 0x4000_0062;
    pub const STREAK_EFFECTS: u32 = 0x4000_0063;
    pub const ADAPTIVE_DEGRADE: u32 = 0x4000_0064;
    pub const ADAPTIVE_DEGRADE_BIAS: u32 = 0x4000_0065;
    pub const SCREEN_BRIGHTNESS: u32 = 0x4000_004B;
}

/// How fast frames are being drawn: the mean of the last forty frames' times.
#[derive(Clone, Debug)]
pub struct FrameRate {
    times: [f32; 40],
    last: f64,
    per_second: f32,
}

impl Default for FrameRate {
    fn default() -> Self {
        Self {
            times: [0.0; 40],
            last: 0.0,
            per_second: 0.0,
        }
    }
}

impl FrameRate {
    /// A frame ended at `now`, seconds.
    // D2-10624
    pub fn end_frame(&mut self, now: f64) {
        #[allow(clippy::cast_possible_truncation)]
        let spent = (now - self.last) as f32;
        self.times[0] = spent;
        let total: f32 = self.times.iter().sum();
        self.per_second = if total > 0.0002 { 40.0 / total } else { 0.0 };
        self.times.copy_within(0..39, 1);
        self.last = now;
    }

    /// The frames a second, 0 before any frame has taken any time.
    #[must_use]
    pub fn frames_per_second(&self) -> f32 {
        self.per_second
    }
}

/// What a preference holds, and its default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// One of its choices: each a number and the name of its label.
    Choice {
        choices: &'static [(i32, &'static str)],
        default: i32,
    },
    /// Yes or no.
    Flag { default: bool },
    /// A number within a range.
    Number {
        least: f32,
        greatest: f32,
        default: f32,
    },
}

impl Kind {
    /// Its default as a value (a choice's number, 1 or 0 for yes or no, or the number).
    #[must_use]
    pub fn default_value(&self) -> f32 {
        match *self {
            #[allow(clippy::cast_precision_loss)] // a few levels
            Kind::Choice { default, .. } => default as f32,
            Kind::Flag { default } => f32::from(u8::from(default)),
            Kind::Number { default, .. } => default,
        }
    }
}

/// A graphics preference: its key, the names of its label and its help (strings of the client's
/// table of preference strings), and what it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preference {
    pub key: u32,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
}

/// The graphics preferences, in the order the client lists them in its graphics section (the
/// interface's own preference, the high quality of its pictures, is not listed).
// D2-09703, D2-09693
pub const PREFERENCES: [Preference; 21] = [
    Preference {
        key: key::TEXTURE_DETAIL,
        label: "ID_Graphics_TextureDetail",
        help: "ID_Graphics_TextureDetail_Help",
        kind: Kind::Choice {
            choices: &[
                (0, "ID_Graphics_Value_VeryLow"),
                (1, "ID_Graphics_Value_Low"),
                (2, "ID_Graphics_Value_Medium"),
                (3, "ID_Graphics_Value_High"),
                (4, "ID_Graphics_Value_VeryHigh"),
            ],
            default: 2,
        },
    },
    Preference {
        key: key::MATERIAL_DETAIL,
        label: "ID_Graphics_MaterialDetail",
        help: "ID_Graphics_MaterialDetail_Help",
        kind: Kind::Choice {
            choices: &[
                (0, "ID_Graphics_Value_Low"),
                (1, "ID_Graphics_Value_Medium"),
                (2, "ID_Graphics_Value_High"),
            ],
            default: 1,
        },
    },
    Preference {
        key: key::MODEL_DETAIL,
        label: "ID_Graphics_ModelDetail",
        help: "ID_Graphics_ModelDetail_Help",
        kind: Kind::Choice {
            choices: &[
                (0, "ID_Graphics_Value_Low"),
                (1, "ID_Graphics_Value_Medium"),
                (2, "ID_Graphics_Value_High"),
            ],
            default: 1,
        },
    },
    Preference {
        key: key::WATER_REFLECTIONS,
        label: "ID_Graphics_WaterReflections",
        help: "ID_Graphics_WaterReflections_Help",
        kind: Kind::Choice {
            choices: &[
                (0, "ID_Graphics_Value_Disabled"),
                (1, "ID_Graphics_Value_Low"),
                (2, "ID_Graphics_Value_High"),
            ],
            default: 0,
        },
    },
    Preference {
        key: key::LANDSCAPE_SHORE_EFFECTS,
        label: "ID_Graphics_LandscapeShoreEffects",
        help: "ID_Graphics_LandscapeShoreEffects_Help",
        kind: Kind::Choice {
            choices: &[
                (0, "ID_Graphics_Value_Disabled"),
                (1, "ID_Graphics_Value_Medium"),
                (2, "ID_Graphics_Value_High"),
            ],
            default: 1,
        },
    },
    Preference {
        key: key::DYNAMIC_OBJECT_SHADOWS,
        label: "ID_Graphics_DynamicObjectShadows",
        help: "ID_Graphics_DynamicObjectShadows_Help",
        kind: Kind::Choice {
            choices: &[
                (0, "ID_Graphics_Value_Disabled"),
                (1, "ID_Graphics_Value_Simple"),
                (2, "ID_Graphics_Value_Volumetric"),
                (3, "ID_Graphics_Value_SelfShadowing"),
            ],
            default: 0,
        },
    },
    Preference {
        key: key::SCENERY_DRAW_DISTANCE,
        label: "ID_Graphics_SceneryDrawDistance",
        help: "ID_Graphics_SceneryDrawDistance_Help",
        kind: Kind::Choice {
            choices: &[
                (0, "ID_Graphics_Value_Low"),
                (1, "ID_Graphics_Value_Medium"),
                (2, "ID_Graphics_Value_High"),
            ],
            default: 1,
        },
    },
    Preference {
        key: key::LANDSCAPE_DRAW_DISTANCE,
        label: "ID_Graphics_LandscapeDrawDistance",
        help: "ID_Graphics_LandscapeDrawDistance_Help",
        kind: Kind::Choice {
            choices: &[
                (0, "ID_Graphics_Value_VeryLow"),
                (1, "ID_Graphics_Value_Low"),
                (2, "ID_Graphics_Value_Medium"),
                (3, "ID_Graphics_Value_High"),
                (4, "ID_Graphics_Value_VeryHigh"),
            ],
            default: 2,
        },
    },
    Preference {
        key: key::LANDSCAPE_FRILL_DISTANCE,
        label: "ID_Graphics_LandscapeFrillDistance",
        help: "ID_Graphics_LandscapeFrillDistance_Help",
        kind: Kind::Choice {
            choices: &[
                (0, "ID_Graphics_Value_Disabled"),
                (1, "ID_Graphics_Value_Low"),
                (2, "ID_Graphics_Value_Medium"),
                (3, "ID_Graphics_Value_High"),
            ],
            default: 0,
        },
    },
    Preference {
        key: key::LANDSCAPE_STATIC_OBJECT_SHADOWS,
        label: "ID_Graphics_LandscapeStaticObjectShadows",
        help: "ID_Graphics_LandscapeStaticObjectShadows_Help",
        kind: Kind::Flag { default: false },
    },
    Preference {
        key: key::LANDSCAPE_CLOUD_SHADOWS,
        label: "ID_Graphics_LandscapeCloudShadows",
        help: "ID_Graphics_LandscapeCloudShadows_Help",
        kind: Kind::Flag { default: false },
    },
    Preference {
        key: key::LIGHT_CORONAS,
        label: "ID_Graphics_LightCoronas",
        help: "ID_Graphics_LightCoronas_Help",
        kind: Kind::Flag { default: false },
    },
    Preference {
        key: key::TRILINEAR_FILTERING,
        label: "ID_Graphics_TrilinearFiltering",
        help: "ID_Graphics_TrilinearFiltering_Help",
        kind: Kind::Flag { default: true },
    },
    Preference {
        key: key::ANISOTROPIC_FILTERING,
        label: "ID_Graphics_AnisotropicFiltering",
        help: "ID_Graphics_AnisotropicFiltering_Help",
        kind: Kind::Flag { default: false },
    },
    Preference {
        key: key::VOLUMETRIC_FOG_EFFECTS,
        label: "ID_Graphics_VolumetricFogEffects",
        help: "ID_Graphics_VolumetricFogEffects_Help",
        kind: Kind::Flag { default: false },
    },
    Preference {
        key: key::LIGHT_SOURCE_EFFECTS,
        label: "ID_Graphics_LightSourceEffects",
        help: "ID_Graphics_LightSourceEffects_Help",
        kind: Kind::Flag { default: true },
    },
    Preference {
        key: key::DECAL_EFFECTS,
        label: "ID_Graphics_DecalEffects",
        help: "ID_Graphics_DecalEffects_Help",
        kind: Kind::Flag { default: true },
    },
    Preference {
        key: key::STREAK_EFFECTS,
        label: "ID_Graphics_StreakEffects",
        help: "ID_Graphics_StreakEffects_Help",
        kind: Kind::Flag { default: true },
    },
    Preference {
        key: key::ADAPTIVE_DEGRADE,
        label: "ID_Graphics_AdaptiveDegrade",
        help: "ID_Graphics_AdaptiveDegrade_Help",
        kind: Kind::Flag { default: false },
    },
    Preference {
        key: key::ADAPTIVE_DEGRADE_BIAS,
        label: "ID_Graphics_AdaptiveDegradeBias",
        help: "ID_Graphics_AdaptiveDegradeBias_Help",
        kind: Kind::Number {
            least: -1.0,
            greatest: 1.0,
            default: 0.0,
        },
    },
    Preference {
        key: key::SCREEN_BRIGHTNESS,
        label: "ID_Graphics_ScreenBrightness",
        help: "ID_Graphics_ScreenBrightness_Help",
        kind: Kind::Number {
            least: -0.2,
            greatest: 1.0,
            default: 0.0,
        },
    },
];

/// The preferences an overall quality sets, in the order of [`PRESETS`]' columns.
const PRESET_KEYS: [u32; 20] = [
    key::TEXTURE_DETAIL,
    key::MATERIAL_DETAIL,
    key::MODEL_DETAIL,
    key::WATER_REFLECTIONS,
    key::LIGHT_CORONAS,
    key::DYNAMIC_OBJECT_SHADOWS,
    key::LANDSCAPE_DRAW_DISTANCE,
    key::LANDSCAPE_FRILL_DISTANCE,
    key::LANDSCAPE_STATIC_OBJECT_SHADOWS,
    key::LANDSCAPE_CLOUD_SHADOWS,
    key::LANDSCAPE_SHORE_EFFECTS,
    key::TRILINEAR_FILTERING,
    key::ANISOTROPIC_FILTERING,
    key::VOLUMETRIC_FOG_EFFECTS,
    key::LIGHT_SOURCE_EFFECTS,
    key::DECAL_EFFECTS,
    key::STREAK_EFFECTS,
    key::SCENERY_DRAW_DISTANCE,
    key::ADAPTIVE_DEGRADE,
    key::ADAPTIVE_DEGRADE_BIAS,
];

/// Each overall quality's values, the lowest (1) first: a choice's number, 1 or 0 for yes or no,
/// the adaptive degrade's bias.
const PRESETS: [[f32; 20]; 5] = [
    [
        2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        1.0, 0.5,
    ],
    [
        2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0,
        1.0, 0.0,
    ],
    [
        3.0, 1.0, 1.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0,
        1.0, -0.5,
    ],
    [
        3.0, 2.0, 2.0, 2.0, 1.0, 2.0, 3.0, 1.0, 1.0, 0.0, 2.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 2.0,
        0.0, 0.0,
    ],
    [
        4.0, 2.0, 2.0, 2.0, 1.0, 2.0, 4.0, 2.0, 1.0, 1.0, 2.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 2.0,
        0.0, 0.0,
    ],
];

/// The highest overall quality.
pub const HIGHEST: u32 = 5;

/// The screen's brightness as a table of each color's 256 levels on 16 bits, for a full screen: a
/// level 0 to 255 goes to 255 times itself, plus twice the brightness of that (rounded toward zero),
/// held to the 16 bits; the brightness is held to -0.2 to 1.
// D2-09813
#[must_use]
pub fn brightness_ramp(brightness: f32) -> [u16; 256] {
    let b = f64::from(brightness.clamp(-0.2, 1.0));
    let mut ramp = [0; 256];
    for (level, out) in (0_i32..).zip(ramp.iter_mut()) {
        let x = level * 255;
        let add = dereth_primitives::num::to_i32_f64(f64::from(x) * b * 2.0);
        *out = u16::try_from((x + add).clamp(0, 0xFFFF)).unwrap_or(u16::MAX);
    }
    ramp
}

/// The overall quality's key in the graphics section, and its name in the preferences file.
pub const OVERALL: u32 = 0x4000_0073;
pub const OVERALL_NAME: &str = "OverallGraphicsQuality";

/// The overall quality a fresh install starts at. With no preferences file the client first runs
/// its configuration program, which offers the quality the machine earns: one plus each step its
/// processor and memory both pass (above 600, 1,000 and 2,000 MHz with shaders, and 255, 511 and
/// 767 MB), so never the highest; a machine that runs this client earns 4, and the player keeps
/// what is offered.
pub const FIRST_RUN: u32 = 4;

/// The graphics preferences' values and defaults, by their place in [`PREFERENCES`]: each a
/// choice's number, 1 or 0 for yes or no, or the number.
#[derive(Clone, Debug, PartialEq)]
pub struct Graphics {
    values: [f32; 21],
    defaults: [f32; 21],
}

impl Default for Graphics {
    /// Every preference at its own default.
    fn default() -> Self {
        let d = PREFERENCES.map(|p| p.kind.default_value());
        Self {
            values: d,
            defaults: d,
        }
    }
}

fn place(key: u32) -> Option<usize> {
    PREFERENCES.iter().position(|p| p.key == key)
}

impl Graphics {
    /// The preferences a fresh install starts with: [`FIRST_RUN`]'s.
    #[must_use]
    pub fn first_run() -> Self {
        let mut g = Self::default();
        g.set_overall(FIRST_RUN);
        g
    }

    /// A preference's value.
    #[must_use]
    pub fn get(&self, key: u32) -> Option<f32> {
        place(key).map(|k| self.values[k])
    }

    /// Whether the water's reflection ripples: only with the water reflections at their highest
    /// choice (High); at Low the reflection is laid as it stands.
    #[must_use]
    pub fn water_ripples(&self) -> bool {
        self.get(key::WATER_REFLECTIONS).is_some_and(|v| v >= 2.0)
    }

    /// A choice's number, `otherwise` when the preference is not one of the graphics'.
    fn level(&self, key: u32, otherwise: u32) -> u32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few levels
        self.get(key).map_or(otherwise, |v| v.max(0.0) as u32)
    }

    /// Whether a yes-or-no preference is on.
    fn on(&self, key: u32) -> bool {
        self.get(key).is_some_and(|v| v != 0.0)
    }

    /// The adaptive degrade's bias, with the adaptive degrade on.
    #[must_use]
    pub fn adaptive_degrade(&self) -> Option<f32> {
        self.get(key::ADAPTIVE_DEGRADE)
            .is_some_and(|v| v != 0.0)
            .then(|| self.get(key::ADAPTIVE_DEGRADE_BIAS).unwrap_or(0.0))
    }

    /// The material detail: 0 (low), 1 (medium) or 2 (high), which of each material's layers are
    /// drawn.
    #[must_use]
    pub fn material_detail(&self) -> u32 {
        self.level(key::MATERIAL_DETAIL, 2).min(2)
    }

    /// The scenery draw distance: 0 (low), 1 (medium) or 2 (high), how far placed objects reach
    /// (`crate::mesh::scenery_scale`).
    #[must_use]
    pub fn scenery_draw_distance(&self) -> u32 {
        self.level(key::SCENERY_DRAW_DISTANCE, 2).min(2)
    }

    /// How textures are taken: the texture detail and the trilinear and anisotropic filtering.
    #[must_use]
    pub fn textures(&self) -> crate::scene::Textures {
        crate::scene::Textures {
            detail: self.level(key::TEXTURE_DETAIL, 4).min(4),
            trilinear: self.on(key::TRILINEAR_FILTERING),
            anisotropic: self.on(key::ANISOTROPIC_FILTERING),
        }
    }

    /// Whether the particle systems moved by repulsors (the ground's mists) think and draw: the
    /// volumetric fog effects.
    #[must_use]
    pub fn volumetric_fog(&self) -> bool {
        self.on(key::VOLUMETRIC_FOG_EFFECTS)
    }

    /// A preference's default.
    #[must_use]
    pub fn default_of(&self, key: u32) -> Option<f32> {
        place(key).map(|k| self.defaults[k])
    }

    /// Set a preference's value; a choice that is not one of its choices is refused.
    pub fn set(&mut self, key: u32, value: f32) -> bool {
        let Some(k) = place(key) else {
            return false;
        };
        if let Kind::Choice { choices, .. } = PREFERENCES[k].kind {
            #[allow(clippy::cast_precision_loss)] // a few levels
            if !choices.iter().any(|(v, _)| *v as f32 == value) {
                return false;
            }
        }
        self.values[k] = value;
        true
    }

    /// Set a preference's default.
    fn set_default(&mut self, key: u32, value: f32) {
        if let Some(k) = place(key) {
            self.defaults[k] = value;
        }
    }

    /// Set every preference an overall quality (1, the lowest, to 5) names, its value and its
    /// default, to that quality's; any other quality changes nothing.
    // D2-09697
    pub fn set_overall(&mut self, quality: u32) {
        let Some(row) = quality
            .checked_sub(1)
            .and_then(|q| PRESETS.get(usize::try_from(q).ok()?))
        else {
            return;
        };
        for (key, v) in PRESET_KEYS.iter().zip(row) {
            self.set(*key, *v);
            self.set_default(*key, *v);
        }
    }

    /// The overall quality the preferences match: the first of the five whose every preference
    /// but the adaptive degrade's bias they hold; 0 when none.
    // D2-09720
    #[must_use]
    pub fn overall(&self) -> u32 {
        (1..=HIGHEST)
            .zip(PRESETS.iter())
            .find(|(_, row)| {
                PRESET_KEYS.iter().zip(row.iter()).all(|(key, v)| {
                    #[allow(clippy::float_cmp)] // levels and yes-or-no, exact
                    let same = *key == key::ADAPTIVE_DEGRADE_BIAS || self.get(*key) == Some(*v);
                    same
                })
            })
            .map_or(0, |(q, _)| q)
    }
}
