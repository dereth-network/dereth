//! The 34 preference attachments registered by the shared store, as plain data.
//!
//! The modern options page resolves these label and help tokens through its string table and
//! pairs enum-choice labels with the registered values when constructing menus.

use super::config::PrefValueConst;

/// One attached preference, with the two configuration calls that may follow it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiPref {
    /// The `UserPreferences` name — `Category.Name`.
    pub name: &'static str,
    /// The attachment's kind argument: 2 = enum, 3 = float, 4 = bool.
    pub kind: u32,
    /// The preference token — the `ID_*` whose hash is the label's string id.
    pub label: &'static str,
    /// The tooltip token — always the label token with `_Help` appended, in all 34.
    pub help: &'static str,
    /// The preference's `(lo, hi)` range, for the 11 that have one.
    pub range: Option<(f32, f32)>,
    /// The preference's enum-choice tokens, for the 8 that have them, in call order.
    ///
    /// The options page pairs these labels with the registered enum values, which are not
    /// necessarily indices: landscape detail runs `VeryLow`=4 … `VeryHigh`=0, and landscape
    /// draw distance runs 3, 5, 8, 11, 15, 25.
    pub choices: &'static [&'static str],
    /// The value preference registration gave the bound variable. **Not** the `SetDefault` value
    /// the option page restores; adaptive degrades, mouse sensitivity and texture filtering differ.
    pub registered_default: PrefValueConst,
}

use PrefValueConst::{Bool, Float, Int};

/// The five landscape/environment texture-detail choice tokens, shared by two preferences.
const DETAIL_5: &[&str] = &[
    "ID_Graphics_Value_VeryLow",
    "ID_Graphics_Value_Low",
    "ID_Graphics_Value_Medium",
    "ID_Graphics_Value_High",
    "ID_Graphics_Value_VeryHigh",
];

/// The 34 UI preferences the client registers at start-up, in registration order.
///
/// The order is the function's own and is load-bearing for nothing except a test that wants to
/// state it; the registry is a hash table in the client and a `BTreeMap` here.
pub const UI_PREFERENCES: [UiPref; 34] = [
    UiPref {
        name: crate::options::names::SOUND_DISABLED,
        kind: 4,
        label: "ID_Sound_DisableSound",
        help: "ID_Sound_DisableSound_Help",
        range: None,
        choices: &[],
        registered_default: Bool(true),
    },
    UiPref {
        name: crate::options::names::SOUND_VOLUME,
        kind: 3,
        label: "ID_Sound_EffectVolume",
        help: "ID_Sound_EffectVolume_Help",
        range: Some((0.0, 1.0)),
        choices: &[],
        registered_default: Float(1.0),
    },
    UiPref {
        name: crate::options::names::AMBIENT_SOUND_DISABLED,
        kind: 4,
        label: "ID_Sound_DisableAmbientSound",
        help: "ID_Sound_DisableAmbientSound_Help",
        range: None,
        choices: &[],
        registered_default: Bool(true),
    },
    UiPref {
        name: crate::options::names::AMBIENT_SOUND_VOLUME,
        kind: 3,
        label: "ID_Sound_AmbientVolume",
        help: "ID_Sound_AmbientVolume_Help",
        range: Some((0.0, 1.0)),
        choices: &[],
        registered_default: Float(1.0),
    },
    UiPref {
        name: crate::options::names::INTERFACE_SOUND_DISABLED,
        kind: 4,
        label: "ID_Sound_DisableInterfaceSound",
        help: "ID_Sound_DisableInterfaceSound_Help",
        range: None,
        choices: &[],
        registered_default: Bool(true),
    },
    UiPref {
        name: crate::options::names::INTERFACE_SOUND_VOLUME,
        kind: 3,
        label: "ID_Sound_InterfaceVolume",
        help: "ID_Sound_InterfaceVolume_Help",
        range: Some((0.0, 1.0)),
        choices: &[],
        registered_default: Float(1.0),
    },
    UiPref {
        name: crate::options::names::SOUND_FEATURES,
        kind: 2,
        label: "ID_Sound_SoundFeatures",
        help: "ID_Sound_SoundFeatures_Help",
        range: None,
        choices: &["ID_Sound_Stereo", "ID_Sound_Mono"],
        registered_default: Int(0),
    },
    UiPref {
        name: crate::options::names::PLAY_SOUND_ONLY_WHEN_ACTIVE,
        kind: 4,
        label: "ID_Sound_NoFocusNoSound",
        help: "ID_Sound_NoFocusNoSound_Help",
        range: None,
        choices: &[],
        registered_default: Bool(true),
    },
    UiPref {
        name: crate::options::names::TOOLTIP_DELAY,
        kind: 3,
        label: "ID_Misc_TooltipDelay",
        help: "ID_Misc_TooltipDelay_Help",
        range: Some((0.0, 10.0)),
        choices: &[],
        registered_default: Float(0.25),
    },
    UiPref {
        name: crate::options::names::TOOLTIP_ENABLE,
        kind: 4,
        label: "ID_Misc_TooltipEnable",
        help: "ID_Misc_TooltipEnable_Help",
        range: None,
        choices: &[],
        registered_default: Bool(true),
    },
    UiPref {
        name: crate::options::names::CHAT_FONT_FACE,
        kind: 2,
        label: "ID_UI_ChatFontFace",
        help: "ID_UI_ChatFontFace_Help",
        range: None,
        choices: &[
            "ID_UI_Value_Arial",
            "ID_UI_Value_CourierNew",
            "ID_UI_Value_PalatinoLinotype",
            "ID_UI_Value_Tahoma",
            "ID_UI_Value_TimesNewRoman",
        ],
        registered_default: Int(2),
    },
    UiPref {
        name: crate::options::names::CHAT_FONT_SIZE,
        kind: 2,
        label: "ID_UI_ChatFontSize",
        help: "ID_UI_ChatFontSize_Help",
        range: None,
        choices: &[
            "ID_UI_Value_Tiny",
            "ID_UI_Value_Small",
            "ID_UI_Value_Medium",
            "ID_UI_Value_Large",
            "ID_UI_Value_XLarge",
        ],
        registered_default: Int(1),
    },
    UiPref {
        name: crate::options::names::TEXTURE_FILTERING,
        kind: 2,
        label: "ID_Graphics_TextureFiltering",
        help: "ID_Graphics_TextureFiltering_Help",
        range: None,
        choices: &[
            "ID_Graphics_TextureFiltering_Bilinear",
            "ID_Graphics_TextureFiltering_Trilinear",
            "ID_Graphics_TextureFiltering_Sharp",
            "ID_Graphics_TextureFiltering_Anisotropic",
        ],
        // 0 (Bilinear) is what startup sets through overall graphics quality preset 3; the UI's
        // Restore Defaults is a separate owner and puts 1 (Trilinear) instead.
        registered_default: Int(0),
    },
    UiPref {
        name: crate::options::names::BUILDING_DETAIL_TEXTURES,
        kind: 4,
        label: "ID_Graphics_BuildingDetailTextures",
        help: "ID_Graphics_BuildingDetailTextures_Help",
        range: None,
        choices: &[],
        registered_default: Bool(true),
    },
    UiPref {
        name: crate::options::names::MULTI_PASS_ALPHA,
        kind: 4,
        label: "ID_Graphics_MultiPassAlpha",
        help: "ID_Graphics_MultiPassAlpha_Help",
        range: None,
        choices: &[],
        registered_default: Bool(false),
    },
    UiPref {
        name: crate::options::names::LANDSCAPE_TEXTURE_DETAIL,
        kind: 2,
        label: "ID_Graphics_LandscapeTextureDetail",
        help: "ID_Graphics_LandscapeTextureDetail_Help",
        range: None,
        choices: DETAIL_5,
        registered_default: Int(2),
    },
    UiPref {
        name: crate::options::names::ENVIRONMENT_TEXTURE_DETAIL,
        kind: 2,
        label: "ID_Graphics_EnvironmentTextureDetail",
        help: "ID_Graphics_EnvironmentTextureDetail_Help",
        range: None,
        choices: DETAIL_5,
        registered_default: Int(1),
    },
    UiPref {
        name: crate::options::names::SCENERY_DRAW_DISTANCE,
        kind: 2,
        label: "ID_Graphics_SceneryDrawDistance",
        help: "ID_Graphics_SceneryDrawDistance_Help",
        range: None,
        choices: &[
            "ID_Graphics_Value_Low",
            "ID_Graphics_Value_Medium",
            "ID_Graphics_Value_High",
        ],
        registered_default: Int(1),
    },
    UiPref {
        name: crate::options::names::LANDSCAPE_DRAW_DISTANCE,
        kind: 2,
        label: "ID_Graphics_LandscapeDrawDistance",
        help: "ID_Graphics_LandscapeDrawDistance_Help",
        range: None,
        choices: &[
            "ID_Graphics_Value_VeryLow",
            "ID_Graphics_Value_Low",
            "ID_Graphics_Value_Medium",
            "ID_Graphics_Value_High",
            "ID_Graphics_Value_VeryHigh",
            "ID_Graphics_Value_Extreme",
        ],
        registered_default: Int(8),
    },
    UiPref {
        name: crate::options::names::FIELD_OF_VIEW,
        kind: 3,
        label: "ID_Graphics_FieldOfView",
        help: "ID_Graphics_FieldOfView_Help",
        range: Some((10.0, 160.0)),
        choices: &[],
        registered_default: Float(90.0),
    },
    UiPref {
        name: crate::options::names::SCREEN_BRIGHTNESS,
        kind: 3,
        label: "ID_Graphics_ScreenBrightness",
        help: "ID_Graphics_ScreenBrightness_Help",
        range: Some((-1.0, 1.0)),
        choices: &[],
        registered_default: Float(0.0),
    },
    UiPref {
        name: crate::options::names::AUTOMATIC_DEGRADES,
        kind: 4,
        label: "ID_Graphics_AdaptiveDegrade",
        help: "ID_Graphics_AdaptiveDegrade_Help",
        range: None,
        choices: &[],
        registered_default: Bool(true),
    },
    UiPref {
        name: crate::options::names::GRAPHICS_PERFORMANCE,
        kind: 3,
        label: "ID_Graphics_AdaptiveDegradeBias",
        help: "ID_Graphics_AdaptiveDegradeBias_Help",
        range: Some((-1.0, 1.0)),
        choices: &[],
        registered_default: Float(0.0),
    },
    UiPref {
        name: crate::options::names::DEGRADE_DISTANCE,
        kind: 3,
        label: "ID_Graphics_DegradeDistance",
        help: "ID_Graphics_DegradeDistance_Help",
        range: Some((0.0, 100.0)),
        choices: &[],
        registered_default: Float(50.0),
    },
    UiPref {
        name: crate::options::names::DISPLAY_FULL_SCREEN,
        kind: 4,
        label: "ID_Rendering_FullScreen",
        help: "ID_Rendering_FullScreen_Help",
        range: None,
        choices: &[],
        // Off at first: the client starts in a window (retail started full screen).
        registered_default: Bool(false),
    },
    UiPref {
        name: crate::options::names::DISPLAY_SYNC_TO_REFRESH,
        kind: 4,
        label: "ID_Rendering_SyncToDisplayRefresh",
        help: "ID_Rendering_SyncToDisplayRefresh_Help",
        range: None,
        choices: &[],
        registered_default: Bool(false),
    },
    UiPref {
        name: crate::options::names::DISPLAY_RESOLUTION,
        kind: 2,
        label: "ID_Rendering_DisplayResolution",
        help: "ID_Rendering_DisplayResolution_Help",
        range: None,
        // No enum choices: the display-preference setup builds the list from the enumerated
        // adapter modes at run time, which is why the options screen adds this one as a non-UI
        // preference and it takes the menu widget's "ask the preference for its choice strings"
        // path instead.
        choices: &[],
        registered_default: Int(0x0400_0300),
    },
    UiPref {
        name: crate::options::names::DISPLAY_REFRESH_RATE,
        kind: 2,
        label: "ID_Rendering_RefreshRate",
        help: "ID_Rendering_RefreshRate_Help",
        range: None,
        choices: &[],
        registered_default: Int(0),
    },
    UiPref {
        name: crate::options::names::MOUSE_LOOK_SENSITIVITY,
        kind: 3,
        label: "ID_Input_MouseLookSensitivity",
        help: "ID_Input_MouseLookSensitivity_Help",
        range: Some((0.01, 1.0)),
        choices: &[],
        registered_default: Float(0.25),
    },
    UiPref {
        name: crate::options::names::INVERT_MOUSE_LOOK_Y_AXIS,
        kind: 4,
        label: "ID_Input_InvertMouseLookYAxis",
        help: "ID_Input_InvertMouseLookYAxis_Help",
        range: None,
        choices: &[],
        registered_default: Bool(false),
    },
    UiPref {
        name: crate::options::names::USE_MOUSE_TURNING,
        kind: 4,
        label: "ID_Input_UseMouseTurning",
        help: "ID_Input_UseMouseTurning_Help",
        range: None,
        choices: &[],
        registered_default: Bool(false),
    },
    UiPref {
        name: crate::options::names::CAMERA_ALIGN_TO_SLOPE,
        kind: 4,
        label: "ID_Camera_AlignToSlope",
        help: "ID_Camera_AlignToSlope_Help",
        range: None,
        choices: &[],
        registered_default: Bool(true),
    },
    UiPref {
        name: crate::options::names::CAMERA_STIFFNESS,
        kind: 3,
        label: "ID_Camera_Stiffness",
        help: "ID_Camera_Stiffness_Help",
        range: Some((0.285_714_3, 1.0)),
        choices: &[],
        registered_default: Float(0.45),
    },
    UiPref {
        name: crate::options::names::CAMERA_ADJUSTMENT_SPEED,
        kind: 3,
        label: "ID_Camera_AdjustmentSpeed",
        help: "ID_Camera_AdjustmentSpeed_Help",
        range: Some((5.0, 80.0)),
        choices: &[],
        registered_default: Float(40.0),
    },
];

/// Static registered bounds, using the slider's 0..1 default when no range is named.
/// Names match the table exactly.
#[must_use]
pub fn range(name: &str) -> (f32, f32) {
    range_or_default(
        UI_PREFERENCES
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.range),
    )
}

/// A slider's bounds after its caller has queried the appropriate metadata registry.
#[must_use]
pub const fn range_or_default(bounds: Option<(f32, f32)>) -> (f32, f32) {
    match bounds {
        Some(bounds) => bounds,
        None => (0.0, 1.0),
    }
}
