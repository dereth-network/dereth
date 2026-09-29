//! The one render preference whose runtime owner is implemented here. No preference-file writes.
//! Preference initialization loads section.key shadow values from the configured path.
//! Render startup registers this UInt32 after applying quality-3 defaults; variable registration
//! then applies any preloaded shadow through string conversion.

use crate::config::Preferences;
pub const TEXTURE_FILTERING: &str = "Render.TextureFiltering";

/// `Render.LandscapeDrawDistance` controls the landscape's mid radius.
/// Updating render preferences hands it to the smart box's mid-radius setter,
/// which resets the cell manager and sets the landscape radius. Render startup registers
/// it as an unsigned 32-bit preference with the choice list
/// `[VeryLow, Low, Medium, High, VeryHigh, Extreme]` and the value array `[3, 5, 8, 11, 15, 25]`
/// in the client; the registered default is **8** (`Medium`).
pub const LANDSCAPE_DRAW_DISTANCE: &str = "Render.LandscapeDrawDistance";

/// `Render.LandscapeDrawDistance`'s registered default, i.e. on a fresh
/// install. The landscape's constructor default is 5, but render startup registers 8 and
/// the preference update applies it before the first landscape is generated, so 8 is what a
/// player who has never opened the options page runs.
pub const LANDSCAPE_DRAW_DISTANCE_DEFAULT: u32 = 8;

/// `[Render] TerrainBlending`: how the landscape blends each cell's terrain layers. Not a retail
/// preference and not on the options page; the preferences file is its only control, and the
/// save merge keeps it because it keeps every key it does not register.
pub const TERRAIN_BLENDING: &str = "Render.TerrainBlending";

/// The values of [`TERRAIN_BLENDING`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerrainBlending {
    /// `cpu`: retail's per-cell composite textures, built on the CPU as retail builds them.
    Cpu,
    /// `gpu`: the same composites, built by the device's compute shader. Bit-identical to `cpu`.
    Gpu,
    /// `splat`: no composites; the pixel shader blends each cell's layers as it draws. Not
    /// bit-identical to the composites, and keeps them out of video memory. The default.
    #[default]
    Splat,
}

impl TerrainBlending {
    /// A value as the file spells it, in any case. `None` for anything else.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "cpu" => Some(Self::Cpu),
            "gpu" => Some(Self::Gpu),
            "splat" => Some(Self::Splat),
            _ => None,
        }
    }
}

/// [`TERRAIN_BLENDING`] from the profile: the default when it is absent or not one of the three.
#[must_use]
pub fn terrain_blending(prefs: &Preferences) -> TerrainBlending {
    prefs
        .get(TERRAIN_BLENDING)
        .and_then(TerrainBlending::parse)
        .unwrap_or_default()
}

/// Decode this preference's registered choice list with
/// `dereth_client_contract::options::store::EnumChoices::resolve` — the same function the
/// options page loads the profile through — rather than a second copy of the label table here:
/// a label match wins, an unmatched string is `strtol`'d as an **index** into the value array
/// (`LandscapeDrawDistance=3` is `High` = 11), and a negative or out-of-range index selects 0.
///
/// `None` when the profile does not name it, so the caller keeps its own default.
#[must_use]
pub fn landscape_draw_distance(prefs: &Preferences) -> Option<u32> {
    let raw = prefs.get(LANDSCAPE_DRAW_DISTANCE)?;
    let choices = dereth_client_contract::options::store::enum_choices(LANDSCAPE_DRAW_DISTANCE)?;
    u32::try_from(choices.resolve(raw)).ok()
}

/// The 48-byte render-preference record plus its three adjacent static preferences,
/// initialized to each name's registered default.
///
/// Graphics-device preparation applies almost all of these preferences **every frame**,
/// both in the main client tick and while keeping the UI alive. It is a poll, not a
/// callback: each field is compared against a shadow copy and only a *change* does any work.
/// The shadows are one static word per preference: the five `Display.*` ones, the two
/// texture-detail levels, and one each for LandscapeDrawDistance, FieldOfView, AspectRatio,
/// ScreenBrightness and the environment-detail textures setting.
/// The render-preference-change callback, supplied for every registration, does **only**
/// recompute the cached overall graphics quality -- it applies nothing.
/// That is why a preference changed on the options page reaches the picture on the next frame
/// rather than on the Apply click, and why this build applies them the same way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderPreferences {
    /// Used by sampler-filter setup, scene begin and creature rendering.
    /// Value 2 selects the `-1.4` mipmap LOD bias.
    pub texture_filtering: u32,
    /// **No reader anywhere in the retail client.** Applying render preferences passes
    /// a hard-coded false for landscape detail texturing and zeroes the cached value
    /// unconditionally, so this preference never reaches the landscape detail setter.
    /// It is also one of the nine
    /// with no options-page row. Kept because it is registered and therefore round-trips.
    pub landscape_detail_textures: bool,
    /// Registered as `Render.BuildingDetailTextures`. Applying it updates landscape
    /// detail texturing; two further rendering sites read the value directly.
    pub environment_detail_textures: bool,
    /// The second alpha-list insertion in mesh drawing,
    /// for a clip-mapped subset.
    pub multi_pass_alpha: bool,
    /// Landscape texture detail is `max(detail, 1) - 1` (0 stays 0, else
    /// one less).
    pub landscape_texture_detail: u32,
    /// The same arithmetic sets clipmap, RGBA and indexed texture scales.
    /// The high-detail drop decision and unwanted texture-level removal also read it.
    /// **Three readers, not none.**
    pub environment_texture_detail: u32,
    /// **No consumer in the retail client beyond the quality-level round trip**: the value is
    /// written and read back only by preference handling.
    pub scenery_draw_distance: u32,
    /// The smart box passes the mid radius to the landscape.
    pub landscape_draw_distance: u32,
    /// Display-aspect setup maps 1 to `0x3FAAAAAB` (4/3),
    /// 2 -> `0x3FE38E39` (16/9), anything else -> display width / display height.
    pub aspect_ratio: u32,
    /// Brightness sets the 256-entry gamma ramp.
    pub screen_brightness: f32,
    /// Default-FOV setup computes the game FOV as `pref * 0.017453292`.
    pub field_of_view: f32,
    /// The automatic degradation multiplier, read by object-degradation selection
    /// to choose which bias it uses.
    pub automatic_degrades: bool,
    /// The bias `get_degrade` reads when
    /// `automatic_degrades` is clear.
    pub graphics_performance: f32,
    /// The offset subtracted from every degrade distance.
    pub degrade_distance: f32,
}

impl Default for RenderPreferences {
    fn default() -> Self {
        Self {
            texture_filtering: dereth_client_contract::sampler::STARTUP_FILTERING,
            landscape_detail_textures: false,
            environment_detail_textures: true,
            multi_pass_alpha: false,
            landscape_texture_detail: LANDSCAPE_TEXTURE_DETAIL_DEFAULT,
            environment_texture_detail: 1,
            scenery_draw_distance: 1,
            landscape_draw_distance: LANDSCAPE_DRAW_DISTANCE_DEFAULT,
            aspect_ratio: ASPECT_RATIO_DEFAULT,
            screen_brightness: 0.0,
            field_of_view: dereth_client_contract::camera::DEFAULT_FOV_DEGREES,
            automatic_degrades: true,
            graphics_performance: 0.0,
            degrade_distance: dereth_animation::parts::S_R_DEGRADE_DISTANCE,
        }
    }
}

/// `Render.LandscapeTextureDetail`'s registered default, `Medium`.
pub const LANDSCAPE_TEXTURE_DETAIL_DEFAULT: u32 = 2;
/// `Render.AspectRatio`'s registered default, `Normal` -- 4:3.
pub const ASPECT_RATIO_DEFAULT: u32 = 1;

/// `Render.EnvironmentTextureDetail`.
pub const ENVIRONMENT_TEXTURE_DETAIL: &str = "Render.EnvironmentTextureDetail";
/// `Render.LandscapeTextureDetail`.
pub const LANDSCAPE_TEXTURE_DETAIL: &str = "Render.LandscapeTextureDetail";
/// `Render.SceneryDrawDistance`.
pub const SCENERY_DRAW_DISTANCE: &str = "Render.SceneryDrawDistance";
/// `Render.AspectRatio`. One of the nine with no options-page row.
pub const ASPECT_RATIO: &str = "Render.AspectRatio";
/// `Render.FieldOfView`.
pub const FIELD_OF_VIEW: &str = "Render.FieldOfView";
/// `Render.ScreenBrightness`.
pub const SCREEN_BRIGHTNESS: &str = "Render.ScreenBrightness";
/// `Render.MultiPassAlpha`.
pub const MULTI_PASS_ALPHA: &str = "Render.MultiPassAlpha";
/// `Render.BuildingDetailTextures`, bound to `RenderPreferences::environment_detail_textures`.
pub const BUILDING_DETAIL_TEXTURES: &str = "Render.BuildingDetailTextures";
/// `Render.LandscapeDetailTextures`. One of the nine with no options-page row.
pub const LANDSCAPE_DETAIL_TEXTURES: &str = "Render.LandscapeDetailTextures";
/// `Render.AutomaticDegrades`.
pub const AUTOMATIC_DEGRADES: &str = "Render.AutomaticDegrades";
/// `Render.GraphicsPerformance`.
pub const GRAPHICS_PERFORMANCE: &str = "Render.GraphicsPerformance";
/// `Render.DegradeDistance`.
pub const DEGRADE_DISTANCE: &str = "Render.DegradeDistance";

/// One unsigned 32-bit preference with a registered choice list: a case-insensitive label match
/// wins, an unmatched string is `strtol`'d as an **index** into the value array, and a negative or
/// out-of-range index selects choice 0.
fn choice(prefs: &Preferences, name: &str) -> Option<u32> {
    let raw = prefs.get(name)?;
    let choices = dereth_client_contract::options::store::enum_choices(name)?;
    u32::try_from(choices.resolve(raw)).ok()
}

/// `Render.AspectRatio`'s three labels, as registered during render startup
/// (`Auto`, `Normal`, `Wide`, with no value array, so the index is the value). It is one of the
/// nine names UI preference initialization never attaches, so
/// `dereth_client_contract::options::store::ENUM_CHOICES` has no row for it and [`choice`] cannot serve
/// it; the registry's own list still decodes the file.
fn aspect_choice(raw: Option<&str>) -> Option<u32> {
    let raw = raw?.trim();
    const LABELS: [&str; 3] = ["Auto", "Normal", "Wide"];
    if let Some(i) = LABELS.iter().position(|l| l.eq_ignore_ascii_case(raw)) {
        return u32::try_from(i).ok();
    }
    // The string conversion's numeric arm: an index into the choice list, 0 when out of range.
    Some(raw.parse::<u32>().ok().filter(|n| *n < 3).unwrap_or(0))
}

impl RenderPreferences {
    /// User preferences load before render startup registers:
    /// every name the file does not carry keeps the registered default.
    #[must_use]
    pub fn from_preferences(prefs: &Preferences) -> Self {
        let mut r = Self {
            texture_filtering: texture_filtering(prefs),
            ..Self::default()
        };
        if let Some(v) = choice(prefs, LANDSCAPE_TEXTURE_DETAIL) {
            r.landscape_texture_detail = v;
        }
        if let Some(v) = choice(prefs, ENVIRONMENT_TEXTURE_DETAIL) {
            r.environment_texture_detail = v;
        }
        if let Some(v) = choice(prefs, SCENERY_DRAW_DISTANCE) {
            r.scenery_draw_distance = v;
        }
        if let Some(v) = landscape_draw_distance(prefs) {
            r.landscape_draw_distance = v;
        }
        if let Some(v) = aspect_choice(prefs.get(ASPECT_RATIO)) {
            r.aspect_ratio = v;
        }
        if let Some(v) = prefs.f32(FIELD_OF_VIEW) {
            r.field_of_view = v;
        }
        if let Some(v) = prefs.f32(SCREEN_BRIGHTNESS) {
            r.screen_brightness = v;
        }
        if let Some(v) = prefs.bool(MULTI_PASS_ALPHA) {
            r.multi_pass_alpha = v;
        }
        if let Some(v) = prefs.bool(BUILDING_DETAIL_TEXTURES) {
            r.environment_detail_textures = v;
        }
        if let Some(v) = prefs.bool(LANDSCAPE_DETAIL_TEXTURES) {
            r.landscape_detail_textures = v;
        }
        if let Some(v) = prefs.bool(AUTOMATIC_DEGRADES) {
            r.automatic_degrades = v;
        }
        if let Some(v) = prefs.f32(GRAPHICS_PERFORMANCE) {
            r.graphics_performance = v;
        }
        if let Some(v) = prefs.f32(DEGRADE_DISTANCE) {
            r.degrade_distance = v;
        }
        r
    }

    /// Default-FOV setup stores the game FOV in radians.
    #[must_use]
    pub fn game_fov_rad(&self) -> f32 {
        self.field_of_view * dereth_client_contract::camera::DEG_TO_RAD
    }

    /// The three display-aspect setup arms.
    #[must_use]
    pub fn aspect(&self) -> dereth_client_contract::camera::AspectPreference {
        match self.aspect_ratio {
            1 => dereth_client_contract::camera::AspectPreference::Normal,
            2 => dereth_client_contract::camera::AspectPreference::Wide,
            _ => dereth_client_contract::camera::AspectPreference::Auto,
        }
    }

    /// Landscape and clipmap texture scales both use `max(detail, 1) - 1`.
    #[must_use]
    pub const fn image_scale(detail: u32) -> u32 {
        if detail == 0 {
            0
        } else {
            detail - 1
        }
    }

    /// The `UiRequest::SetPreference` names this struct owns, applied to the struct itself.
    /// Returns whether the value landed, so the caller can hand an unowned request on.
    ///
    /// **What "landed" means, per name.** `Render.FieldOfView`, `Render.AspectRatio`,
    /// `Render.MultiPassAlpha`, `Render.GraphicsPerformance` and `Render.DegradeDistance` are
    /// read out of this struct on the next frame and change the picture then, which is exactly
    /// what the render-preference poll does with them.
    ///
    /// The two texture-detail levels are different, and the difference is stated rather than
    /// hidden: retail's poll changes the landscape texture scale, then flushes graphics
    /// resources, throwing away every cached
    /// texture so the next build re-creates them at the new size. This build has no such flush —
    /// `crate::world::WorldScene`'s `TerrainMergeCache` takes its shift once, at
    /// `WorldScene::load`. So the **variable** moves here and the **textures** follow at the next
    /// scene load, not at the next frame. A live flush would be new work.
    /// `Render.SceneryDrawDistance` and `Render.AutomaticDegrades` record for the same reason and
    /// with the same caveat; see the struct's field docs for what reads each in retail.
    pub fn set_named(&mut self, name: &str, value: &dereth_client_contract::PrefValue) -> bool {
        use dereth_client_contract::PrefValue;
        let int = |v: &PrefValue| match v {
            PrefValue::Int(i) => Some(u32::from_ne_bytes(i.to_ne_bytes())),
            _ => None,
        };
        let float = |v: &PrefValue| match v {
            PrefValue::Float(f) => Some(*f),
            _ => None,
        };
        let boolean = |v: &PrefValue| match v {
            PrefValue::Bool(b) => Some(*b),
            _ => None,
        };
        macro_rules! take {
            ($field:ident, $get:ident) => {{
                let Some(v) = $get(value) else { return false };
                self.$field = v;
                return true;
            }};
        }
        if name.eq_ignore_ascii_case(FIELD_OF_VIEW) {
            take!(field_of_view, float);
        }
        if name.eq_ignore_ascii_case(ASPECT_RATIO) {
            take!(aspect_ratio, int);
        }
        if name.eq_ignore_ascii_case(SCREEN_BRIGHTNESS) {
            take!(screen_brightness, float);
        }
        if name.eq_ignore_ascii_case(MULTI_PASS_ALPHA) {
            take!(multi_pass_alpha, boolean);
        }
        if name.eq_ignore_ascii_case(BUILDING_DETAIL_TEXTURES) {
            take!(environment_detail_textures, boolean);
        }
        if name.eq_ignore_ascii_case(GRAPHICS_PERFORMANCE) {
            take!(graphics_performance, float);
        }
        if name.eq_ignore_ascii_case(DEGRADE_DISTANCE) {
            take!(degrade_distance, float);
        }
        if name.eq_ignore_ascii_case(LANDSCAPE_TEXTURE_DETAIL) {
            take!(landscape_texture_detail, int);
        }
        if name.eq_ignore_ascii_case(ENVIRONMENT_TEXTURE_DETAIL) {
            take!(environment_texture_detail, int);
        }
        if name.eq_ignore_ascii_case(SCENERY_DRAW_DISTANCE) {
            take!(scenery_draw_distance, int);
        }
        // **Without this arm the draw distance needs a restart.** The options page's write
        // would fall through every owner and end on `App`'s *"UI request with no owner yet"*
        // line; nothing reads it after start-up, so the preference could only take effect
        // through `Config::apply_preferences` on the next launch. The live path is
        // [`crate::world::WorldScene::update_from_preferences`], which polls the field this writes.
        if name.eq_ignore_ascii_case(LANDSCAPE_DRAW_DISTANCE) {
            take!(landscape_draw_distance, int);
        }
        if name.eq_ignore_ascii_case(AUTOMATIC_DEGRADES) {
            take!(automatic_degrades, boolean);
        }
        false
    }
}

/// String conversion compares choice names without case, then tries `strtol` with base 0.
/// Negative or out-of-range indices select choice 0. Converting back to a string writes
/// the choice NAME, so loading only a numeric UInt32 would ignore normal retail preferences.
fn filtering_choice(raw: &str) -> u32 {
    const CHOICES: [&str; 4] = ["Bilinear", "Trilinear", "Sharp", "Anisotropic"];
    if let Some((_, i)) = CHOICES
        .iter()
        .zip(0..4u32)
        .find(|(c, _)| c.eq_ignore_ascii_case(raw))
    {
        return i;
    }
    let s = raw.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, s) = if let Some(s) = s.strip_prefix('-') {
        (true, s)
    } else {
        (false, s.strip_prefix('+').unwrap_or(s))
    };
    let (radix, digits) = if let Some(s) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, s)
    } else if s.starts_with('0') {
        (8, s)
    } else {
        (10, s)
    };
    let end = digits
        .char_indices()
        .find(|(_, c)| !c.is_digit(radix))
        .map_or(digits.len(), |(i, _)| i);
    // Overflow and all values>=4 have the same retail fallback; no unbounded integer needed.
    let n = u32::from_str_radix(&digits[..end], radix).unwrap_or(0);
    if !negative && n < 4 {
        n
    } else {
        0
    }
}

#[must_use]
pub fn texture_filtering(prefs: &Preferences) -> u32 {
    prefs.get(TEXTURE_FILTERING).map_or(
        dereth_client_contract::sampler::STARTUP_FILTERING,
        filtering_choice,
    )
}

/// The configured file/error policy is shared with Display preferences: missing/unreadable
/// files retain startup defaults. Nothing is logged, saved, normalized, or loaded into other owners.
#[must_use]
pub fn texture_filtering_from_file(path: &std::path::Path) -> u32 {
    texture_filtering(&Preferences::load(path))
}

/// Shell creation resets its mirrored registry. Seed from the LIVE owner, never re-read the file
/// here: a shell rebuild must not undo options already changed in this session.
pub fn seed_ui_registry(preference: u32) {
    dereth_client_contract::options::store::set_value(
        TEXTURE_FILTERING,
        dereth_client_contract::PrefValue::Int(i32::from_ne_bytes(preference.to_ne_bytes())),
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_profile_section_choice_names_and_numeric_fallbacks() {
        for (raw, expected) in [
            ("Bilinear", 0),
            ("Trilinear", 1),
            ("sHaRp", 2),
            ("ANISOTROPIC", 3),
            ("0", 0),
            ("1", 1),
            ("+2", 2),
            ("0x3", 3),
            ("03", 3),
            ("2tail", 2),
            ("4", 0),
            ("-1", 0),
            ("4294967296", 0),
            ("unknown", 0),
            ("", 0),
        ] {
            let p = Preferences::parse(&format!(
                "[rEnDeR]
TextureFiltering={raw}
"
            ));
            assert_eq!(texture_filtering(&p), expected, "{raw}");
        }
        for text in [
            "",
            "[Other]
TextureFiltering=Sharp",
            "[Default]
Render.TextureFiltering=Sharp",
        ] {
            assert_eq!(texture_filtering(&Preferences::parse(text)), 0);
        }
        assert_eq!(texture_filtering_from_file(std::path::Path::new("")), 0);
    }
}
