//! The one render preference whose runtime owner is implemented here. No preference-file writes.
//! Preference initialization loads section.key shadow values from the configured path.
//! Render startup registers this UInt32 after applying quality-3 defaults; variable registration
//! then applies any preloaded shadow through string conversion.

use crate::config::Preferences;
pub const TEXTURE_FILTERING: &str = dereth_client_contract::options::names::TEXTURE_FILTERING;

/// `Render.LandscapeDrawDistance` controls the landscape's mid radius.
/// Updating render preferences hands it to the smart box's mid-radius setter,
/// which resets the cell manager and sets the landscape radius. Render startup registers
/// it as an unsigned 32-bit preference with the choice list
/// `[VeryLow, Low, Medium, High, VeryHigh, Extreme]` and the value array `[3, 5, 8, 11, 15, 25]`
/// in the client; the registered default is **8** (`Medium`).
pub const LANDSCAPE_DRAW_DISTANCE: &str =
    dereth_client_contract::options::names::LANDSCAPE_DRAW_DISTANCE;

/// `Render.LandscapeDrawDistance`'s registered default, i.e. on a fresh
/// install. The landscape's constructor default is 5, but render startup registers 8 and
/// the preference update applies it before the first landscape is generated, so 8 is what a
/// player who has never opened the options page runs.
pub const LANDSCAPE_DRAW_DISTANCE_DEFAULT: u32 = 8;

/// `[Render] TerrainBlending`: how the landscape blends each cell's terrain layers. Not a retail
/// preference and not on the options page; the preferences file is its only control, and the
/// save merge keeps it because it keeps every key it does not register.
pub const TERRAIN_BLENDING: &str = dereth_client_contract::options::names::TERRAIN_BLENDING;

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

/// `[Render] Ground`, `[Render] Sky` and `[Render] Objects`: the presentation options from another
/// era, which choose the era of the ground, of the sky and of the objects' look. See
/// [`dereth_client_contract::options::landscape`], which owns the names, the values and their
/// spellings.
pub use dereth_client_contract::options::landscape::{
    RegionStyle, RequiredFiles, GROUND, OBJECTS, SKY,
};

/// The objects have one older look: either older style is it.
#[must_use]
pub const fn objects_style(style: RegionStyle) -> RegionStyle {
    match style {
        RegionStyle::LegacySoftware | RegionStyle::LegacyHardware => RegionStyle::LegacyHardware,
        RegionStyle::Late => RegionStyle::Late,
    }
}

/// One landscape option from the profile: `None` (the world's own) when it is absent or names
/// nothing this client reads.
#[must_use]
pub fn landscape_style(prefs: &Preferences, name: &str) -> Option<RegionStyle> {
    prefs
        .get(name)
        .and_then(dereth_client_contract::options::landscape::parse)
        .flatten()
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
    /// The landscape detail texture. The end-of-retail client reads this nowhere: applying
    /// render preferences passes a hard-coded false for landscape detail texturing. The clients
    /// before it (February 2005 through 2012) passed this preference instead, and their middle and
    /// higher quality presets turned it on; this client does as they did. Off by default, so a
    /// default profile draws what the end-of-retail client draws. One of the nine with no
    /// options-page row.
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
    /// `[Render] Ground`: the region whose land surface draws the ground; `None` is the world's
    /// own. Not a retail preference. The scene applies a change live.
    pub ground: Option<RegionStyle>,
    /// `[Render] Sky`: the region whose sky draws, with its light and fog; `None` is the world's
    /// own. Not a retail preference. The scene applies a change live.
    pub sky: Option<RegionStyle>,
    /// `[Render] Objects`: the era whose models, surfaces, pictures and palettes the world's
    /// objects draw with (`Some(LegacyHardware)` the older files, `Some(Late)` the later ones);
    /// `None` is the world's own. Not a retail preference. The scene applies a change live.
    pub objects: Option<RegionStyle>,
    /// `[Fidelity]`: the optional high-fidelity presentation, all off by default and in effect
    /// only under the Horizon interface. Not a retail preference; Horizon's options page, the
    /// file and `--set-at` set it. The scene applies a change live. Only in a build with the
    /// presentation.
    #[cfg(feature = "hifi")]
    pub fidelity: FidelityPreferences,
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
            ground: None,
            sky: None,
            objects: None,
            #[cfg(feature = "hifi")]
            fidelity: FidelityPreferences::default(),
        }
    }
}

/// `Render.LandscapeTextureDetail`'s registered default, `Medium`.
pub const LANDSCAPE_TEXTURE_DETAIL_DEFAULT: u32 = 2;
/// `Render.AspectRatio`'s registered default, `Normal` -- 4:3.
pub const ASPECT_RATIO_DEFAULT: u32 = 1;

/// `Render.EnvironmentTextureDetail`.
pub const ENVIRONMENT_TEXTURE_DETAIL: &str =
    dereth_client_contract::options::names::ENVIRONMENT_TEXTURE_DETAIL;
/// `Render.LandscapeTextureDetail`.
pub const LANDSCAPE_TEXTURE_DETAIL: &str =
    dereth_client_contract::options::names::LANDSCAPE_TEXTURE_DETAIL;
/// `Render.SceneryDrawDistance`.
pub const SCENERY_DRAW_DISTANCE: &str =
    dereth_client_contract::options::names::SCENERY_DRAW_DISTANCE;
/// `Render.AspectRatio`. One of the nine with no options-page row.
pub const ASPECT_RATIO: &str = dereth_client_contract::options::names::ASPECT_RATIO;
/// `Render.FieldOfView`.
pub const FIELD_OF_VIEW: &str = dereth_client_contract::options::names::FIELD_OF_VIEW;
/// `Render.ScreenBrightness`.
pub const SCREEN_BRIGHTNESS: &str = dereth_client_contract::options::names::SCREEN_BRIGHTNESS;
/// `Render.MultiPassAlpha`.
pub const MULTI_PASS_ALPHA: &str = dereth_client_contract::options::names::MULTI_PASS_ALPHA;
/// `Render.BuildingDetailTextures`, bound to `RenderPreferences::environment_detail_textures`.
pub const BUILDING_DETAIL_TEXTURES: &str =
    dereth_client_contract::options::names::BUILDING_DETAIL_TEXTURES;
/// `Render.LandscapeDetailTextures`: the detail texture over the ground. Retail's page had no
/// row for it; both interfaces' pages here do.
pub const LANDSCAPE_DETAIL_TEXTURES: &str =
    dereth_client_contract::options::store::LANDSCAPE_DETAIL_TEXTURES;
/// `Render.AutomaticDegrades`.
pub const AUTOMATIC_DEGRADES: &str = dereth_client_contract::options::names::AUTOMATIC_DEGRADES;
/// `Render.GraphicsPerformance`.
pub const GRAPHICS_PERFORMANCE: &str = dereth_client_contract::options::names::GRAPHICS_PERFORMANCE;
/// `Render.DegradeDistance`.
pub const DEGRADE_DISTANCE: &str = dereth_client_contract::options::names::DEGRADE_DISTANCE;

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
        r.ground = landscape_style(prefs, GROUND);
        r.sky = landscape_style(prefs, SKY);
        r.objects = landscape_style(prefs, OBJECTS).map(objects_style);
        if let Some(v) = prefs.f32(GRAPHICS_PERFORMANCE) {
            r.graphics_performance = v;
        }
        if let Some(v) = prefs.f32(DEGRADE_DISTANCE) {
            r.degrade_distance = v;
        }
        // The optional high-fidelity presentation's section, in a build that has it.
        #[cfg(feature = "hifi")]
        {
            r.fidelity = FidelityPreferences::from_preferences(prefs);
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
    /// The rest change the world live too, through the scene's per-frame poll of this struct:
    /// the two texture-detail levels flush the cached textures and rebuild the resident blocks at
    /// the new size, as retail's poll flushes its graphics resources; the landscape draw distance
    /// opens a new landblock window; the detail-texture switches re-apply the detail textures;
    /// `Render.AutomaticDegrades` restarts the detail loop. `Render.SceneryDrawDistance` is
    /// recorded and read by nothing; see the struct's field docs for what reads each in retail.
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
        if name.eq_ignore_ascii_case(LANDSCAPE_DETAIL_TEXTURES) {
            take!(landscape_detail_textures, boolean);
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
        // [`dereth_scene::world_scene::SceneWrites::update_from_preferences`], which polls the field this writes.
        if name.eq_ignore_ascii_case(LANDSCAPE_DRAW_DISTANCE) {
            take!(landscape_draw_distance, int);
        }
        if name.eq_ignore_ascii_case(AUTOMATIC_DEGRADES) {
            take!(automatic_degrades, boolean);
        }
        // The three presentation options from another era, which the scene's poll applies
        // live.
        if let Some(which) = dereth_client_contract::options::landscape::Landscape::of(name) {
            let PrefValue::Int(_) = value else {
                return false;
            };
            let style = dereth_client_contract::options::landscape::style_of(value);
            match which {
                dereth_client_contract::options::landscape::Landscape::Ground => {
                    self.ground = style;
                }
                dereth_client_contract::options::landscape::Landscape::Sky => self.sky = style,
                dereth_client_contract::options::landscape::Landscape::Objects => {
                    self.objects = style.map(objects_style);
                }
            }
            return true;
        }
        // The optional high-fidelity presentation, which the scene's poll applies live, in a
        // build that has it.
        #[cfg(feature = "hifi")]
        {
            self.fidelity.set_named(name, value)
        }
        #[cfg(not(feature = "hifi"))]
        {
            false
        }
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

/// The `[Fidelity]` names: the optional high-fidelity presentation. See
/// [`dereth_client_contract::options::names::fidelity`].
pub use dereth_client_contract::options::names::fidelity;

/// One option of the high-fidelity presentation.
#[cfg(feature = "hifi")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FidelityFeature {
    /// `Fidelity.Lighting`.
    Lighting,
    /// `Fidelity.Shadows`.
    Shadows,
    /// `Fidelity.GlobalIllumination`.
    GlobalIllumination,
    /// `Fidelity.AmbientOcclusion`.
    AmbientOcclusion,
    /// `Fidelity.Lamps`.
    Lamps,
    /// `Fidelity.Sky`.
    Sky,
    /// `Fidelity.Weather`.
    Weather,
    /// `Fidelity.Debug`.
    Debug,
}

#[cfg(feature = "hifi")]
impl FidelityFeature {
    /// Every option, in the order the names list them.
    pub const ALL: [Self; 8] = [
        Self::Lighting,
        Self::Shadows,
        Self::GlobalIllumination,
        Self::AmbientOcclusion,
        Self::Lamps,
        Self::Sky,
        Self::Weather,
        Self::Debug,
    ];

    /// The option's preference name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lighting => fidelity::LIGHTING,
            Self::Shadows => fidelity::SHADOWS,
            Self::GlobalIllumination => fidelity::GLOBAL_ILLUMINATION,
            Self::AmbientOcclusion => fidelity::AMBIENT_OCCLUSION,
            Self::Lamps => fidelity::LAMPS,
            Self::Sky => fidelity::SKY,
            Self::Weather => fidelity::WEATHER,
            Self::Debug => fidelity::DEBUG,
        }
    }

    /// The highest value the option takes: 4 for an effect (Ultra), the last view for the
    /// diagnostic views.
    #[must_use]
    pub const fn max(self) -> u32 {
        match self {
            Self::Debug => 7,
            _ => 4,
        }
    }

    /// `raw` as this option stores it: an effect past Ultra is Ultra; a diagnostic view past the
    /// last is off.
    #[must_use]
    pub const fn clamp(self, raw: u32) -> u32 {
        if raw <= self.max() {
            raw
        } else if matches!(self, Self::Debug) {
            0
        } else {
            self.max()
        }
    }
}

/// The `[Fidelity]` section: what the player asked of the optional high-fidelity presentation,
/// and whether the Horizon interface is shown, the only one it draws under. Plain data, all off
/// by default; the scene maps it onto the presentation's own settings.
#[cfg(feature = "hifi")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FidelityPreferences {
    /// Each option's stored value, in [`FidelityFeature::ALL`] order. 0 is off for every one; a
    /// ticked box is 1.
    pub values: [u32; 8],
    /// Whether the interface shown is Horizon. Never kept in the profile: it is read from the
    /// interface choice at start-up and follows the interface shown after that. Off, every option
    /// is off in effect, whatever it stores.
    pub interface: bool,
}

#[cfg(feature = "hifi")]
impl FidelityPreferences {
    /// The section from the profile: a box the file does not tick is off, and the interface
    /// flag is the interface the profile starts in.
    #[must_use]
    pub fn from_preferences(prefs: &Preferences) -> Self {
        let mut f = Self::default();
        for (i, feature) in FidelityFeature::ALL.into_iter().enumerate() {
            let raw = match feature {
                // The view a diagnostic run asks for, by number.
                FidelityFeature::Debug => prefs.u32(feature.name()),
                // A box the page ticked; a number the file holds otherwise (from an older
                // build) is not read, as the page does not read it.
                _ => prefs.bool(feature.name()).map(u32::from),
            };
            if let Some(raw) = raw {
                f.values[i] = feature.clamp(raw);
            }
        }
        f.interface = prefs
            .get(dereth_client_contract::options::names::INTERFACE)
            .and_then(|v| {
                dereth_client_contract::options::interface::parse_value(
                    dereth_client_contract::options::names::INTERFACE,
                    v,
                )
            })
            .is_some_and(|v| {
                v == dereth_client_contract::options::interface::Interface::Horizon.value()
            });
        f
    }

    /// The stored value of `feature`, whatever the interface.
    #[must_use]
    pub fn value(&self, feature: FidelityFeature) -> u32 {
        FidelityFeature::ALL
            .iter()
            .position(|f| *f == feature)
            .map_or(0, |i| self.values[i])
    }

    /// The value `feature` takes in effect: its stored value while the Horizon interface is
    /// shown, 0 otherwise.
    #[must_use]
    pub fn effective(&self, feature: FidelityFeature) -> u32 {
        if self.interface {
            self.value(feature)
        } else {
            0
        }
    }

    /// Whether any option is on in effect. Without one the presentation is not installed.
    #[must_use]
    pub fn any_effective(&self) -> bool {
        FidelityFeature::ALL
            .into_iter()
            .any(|f| self.effective(f) != 0)
    }

    /// Whether any option is stored on, whatever the interface.
    #[must_use]
    pub fn any_stored(&self) -> bool {
        self.values.iter().any(|v| *v != 0)
    }

    /// The settings a capture or a test names, as the Horizon interface draws them: a comma list
    /// of `Name=value` (`+` also separates), each name with or without its `Fidelity.` prefix and
    /// each value a number or `off`, `on`, `low`, `medium`, `high` or `ultra`. `off` alone is
    /// every option off. `Interface=0` names a run under another interface.
    ///
    /// # Errors
    /// A name no option has, or a value that is not one of those, as a sentence.
    pub fn parse_switch(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        let mut f = Self {
            interface: true,
            ..Self::default()
        };
        if raw.eq_ignore_ascii_case("off") {
            return Ok(f);
        }
        for item in raw
            .split([',', '+'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let (name, value) = item
                .split_once('=')
                .ok_or_else(|| format!("a fidelity setting is Name=value, not {item:?}"))?;
            let value = match value.trim().to_ascii_lowercase().as_str() {
                "off" => 0,
                "on" | "low" => 1,
                "medium" => 2,
                "high" => 3,
                "ultra" => 4,
                v => v
                    .parse::<u32>()
                    .map_err(|_| format!("{value:?} is not a value for {name}"))?,
            };
            let full = if name.trim().to_ascii_lowercase().starts_with("fidelity.") {
                name.trim().to_owned()
            } else {
                format!("Fidelity.{}", name.trim())
            };
            if !f.set_named(
                &full,
                &dereth_client_contract::PrefValue::Int(i32::try_from(value).unwrap_or(i32::MAX)),
            ) {
                return Err(format!("there is no fidelity option {name:?}"));
            }
        }
        Ok(f)
    }

    /// Apply one `[Fidelity]` preference, or the interface flag. Returns whether `name` is one
    /// of them and `value` landed: an integer or a boolean for every one.
    pub fn set_named(&mut self, name: &str, value: &dereth_client_contract::PrefValue) -> bool {
        use dereth_client_contract::PrefValue;
        let raw = match value {
            PrefValue::Bool(b) => u32::from(*b),
            PrefValue::Int(i) => u32::try_from(*i).unwrap_or(0),
            _ => return false,
        };
        if name.eq_ignore_ascii_case(fidelity::INTERFACE) {
            self.interface = raw != 0;
            return true;
        }
        for (i, feature) in FidelityFeature::ALL.into_iter().enumerate() {
            if name.eq_ignore_ascii_case(feature.name()) {
                self.values[i] = feature.clamp(raw);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: hifi.options.every-fidelity-option-is-off-by-default
    #[test]
    #[cfg(feature = "hifi")]
    fn every_fidelity_option_is_off_by_default_and_none_is_in_effect_outside_horizon() {
        let d = FidelityPreferences::default();
        assert!(!d.interface);
        assert!(FidelityFeature::ALL.into_iter().all(|f| d.value(f) == 0));
        assert!(!d.any_effective());
        assert_eq!(
            FidelityPreferences::from_preferences(&Preferences::parse("")),
            d
        );
        // Every box ticked, under the classic or the modern interface: nothing in effect.
        for interface in [
            "",
            "Interface=Modern\n",
            "Interface=Classic\n",
            "Interface=0\n",
        ] {
            let ticked = FidelityPreferences::from_preferences(&Preferences::parse(&format!(
                "[UI]\n{interface}[Fidelity]\nLighting=True\nShadows=True\n\
                 GlobalIllumination=True\nAmbientOcclusion=True\nLamps=True\nSky=True\n"
            )));
            assert!(!ticked.interface, "{interface:?}");
            assert_eq!(ticked.value(FidelityFeature::Lighting), 1);
            assert!(ticked.any_stored());
            assert!(
                FidelityFeature::ALL
                    .into_iter()
                    .all(|f| ticked.effective(f) == 0),
                "{interface:?}"
            );
            assert!(!ticked.any_effective(), "{interface:?}");
        }
        // Under Horizon the ticked boxes are in effect.
        let horizon = FidelityPreferences::from_preferences(&Preferences::parse(
            "[UI]\nInterface=Horizon\n[Fidelity]\nAmbientOcclusion=True\n",
        ));
        assert!(horizon.interface);
        assert_eq!(horizon.effective(FidelityFeature::AmbientOcclusion), 1);
        assert_eq!(horizon.effective(FidelityFeature::Lighting), 0);
        // An older build's levels, and the names of options this build does not have, are not
        // read; neither is an error.
        let old = FidelityPreferences::from_preferences(&Preferences::parse(
            "[UI]\nInterface=2\n[Fidelity]\nEnabled=1\nPreset=2\nWater=3\nLighting=3\n",
        ));
        assert!(old.interface);
        assert!(!old.any_effective());
        // Every option has its name, and every name its option.
        assert_eq!(fidelity::NAMES.len(), FidelityFeature::ALL.len());
        for f in FidelityFeature::ALL {
            assert!(fidelity::NAMES.contains(&f.name()), "{f:?}");
        }
    }

    #[test]
    #[cfg(feature = "hifi")]
    fn fidelity_values_read_and_set_by_name_and_out_of_range_values_clamp() {
        use dereth_client_contract::PrefValue;
        let p = FidelityPreferences::from_preferences(&Preferences::parse(
            "[UI]\nInterface=Horizon\n[Fidelity]\nDebug=9\nSky=1\n",
        ));
        assert_eq!(
            p.effective(FidelityFeature::Debug),
            0,
            "an unknown view is off"
        );
        assert_eq!(p.effective(FidelityFeature::Sky), 1);
        let mut q = FidelityPreferences::default();
        assert!(q.set_named("fidelity.interface", &PrefValue::Bool(true)));
        assert!(q.set_named(fidelity::SHADOWS, &PrefValue::Int(7)));
        assert!(!q.set_named(fidelity::SHADOWS, &PrefValue::Float(2.0)));
        assert!(!q.set_named("Render.FieldOfView", &PrefValue::Int(2)));
        assert!(!q.set_named("Fidelity.Water", &PrefValue::Int(2)));
        assert_eq!(
            q.effective(FidelityFeature::Shadows),
            4,
            "a level saturates at Ultra"
        );
        assert!(q.set_named(fidelity::INTERFACE, &PrefValue::Bool(false)));
        assert_eq!(q.effective(FidelityFeature::Shadows), 0);
        assert_eq!(
            q.value(FidelityFeature::Shadows),
            4,
            "the stored value is kept"
        );
    }

    /// Behaviour: hifi.options.the-fidelity-section-reaches-the-render-preferences
    #[test]
    #[cfg(feature = "hifi")]
    fn the_render_preferences_carry_the_fidelity_section_from_the_file_and_by_name() {
        use dereth_client_contract::PrefValue;
        assert_eq!(
            RenderPreferences::default().fidelity,
            FidelityPreferences::default()
        );
        let r = RenderPreferences::from_preferences(&Preferences::parse(
            "[UI]\nInterface=Horizon\n[Fidelity]\nDebug=1\n",
        ));
        assert!(r.fidelity.any_effective());
        assert_eq!(r.fidelity.effective(FidelityFeature::Debug), 1);
        // The live path: a name the render preferences do not own themselves lands in the
        // section, the interface flag with them.
        let mut live = RenderPreferences::default();
        assert!(live.set_named(fidelity::AMBIENT_OCCLUSION, &PrefValue::Bool(true)));
        assert!(!live.fidelity.any_effective());
        assert!(live.set_named(fidelity::INTERFACE, &PrefValue::Bool(true)));
        assert_eq!(
            live.fidelity.effective(FidelityFeature::AmbientOcclusion),
            1
        );
        assert!(!live.set_named("Fidelity.NoSuchOption", &PrefValue::Int(1)));
        // A capture's settings, as Horizon draws them.
        let off = FidelityPreferences::parse_switch("off").expect("off");
        assert!(off.interface && !off.any_effective());
        let list = FidelityPreferences::parse_switch("Debug=1, Fidelity.Lighting=high+Sky=on")
            .expect("list");
        assert!(list.interface);
        assert_eq!(list.effective(FidelityFeature::Debug), 1);
        assert_eq!(list.effective(FidelityFeature::Lighting), 3);
        assert_eq!(list.effective(FidelityFeature::Sky), 1);
        let classic = FidelityPreferences::parse_switch("Lighting=1,Interface=0").expect("classic");
        assert!(!classic.any_effective() && classic.any_stored());
        assert!(FidelityPreferences::parse_switch("Lighting").is_err());
        assert!(FidelityPreferences::parse_switch("Water=1").is_err());
        assert!(FidelityPreferences::parse_switch("Sky=lots").is_err());
    }

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
