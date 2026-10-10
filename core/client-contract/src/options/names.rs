//! Canonical preference keys shared by the value store and its consumers.

/// `Camera.AdjustmentSpeed`.
pub const CAMERA_ADJUSTMENT_SPEED: &str = "Camera.AdjustmentSpeed";
/// `Camera.AlignToSlope`.
pub const CAMERA_ALIGN_TO_SLOPE: &str = "Camera.AlignToSlope";
/// `Camera.Stiffness`.
pub const CAMERA_STIFFNESS: &str = "Camera.Stiffness";
/// `Debug.PerformancePanel`.
pub const PERFORMANCE_PANEL: &str = "Debug.PerformancePanel";
/// `Display.FullScreen`.
pub const DISPLAY_FULL_SCREEN: &str = "Display.FullScreen";
/// `Display.RefreshRate`.
pub const DISPLAY_REFRESH_RATE: &str = "Display.RefreshRate";
/// `Display.Resolution`.
pub const DISPLAY_RESOLUTION: &str = "Display.Resolution";
/// `Display.SyncToRefresh`.
pub const DISPLAY_SYNC_TO_REFRESH: &str = "Display.SyncToRefresh";
/// `Input.InvertMouseLookYAxis`.
pub const INVERT_MOUSE_LOOK_Y_AXIS: &str = "Input.InvertMouseLookYAxis";
/// `Input.KeymapFile`.
pub const KEYMAP_FILE: &str = "Input.KeymapFile";
/// `Input.MouseLookSensitivity`.
pub const MOUSE_LOOK_SENSITIVITY: &str = "Input.MouseLookSensitivity";
/// `Input.MouseLookSmoothingAmount`.
pub const MOUSE_LOOK_SMOOTHING_AMOUNT: &str = "Input.MouseLookSmoothingAmount";
/// `Input.UseMouseTurning`.
pub const USE_MOUSE_TURNING: &str = "Input.UseMouseTurning";
/// `International.UseIME`.
pub const USE_IME: &str = "International.UseIME";
/// `Misc.TooltipDelay`.
pub const TOOLTIP_DELAY: &str = "Misc.TooltipDelay";
/// `Misc.TooltipEnable`.
pub const TOOLTIP_ENABLE: &str = "Misc.TooltipEnable";
/// `Render.AspectRatio`.
pub const ASPECT_RATIO: &str = "Render.AspectRatio";
/// `Render.AutomaticDegrades`.
pub const AUTOMATIC_DEGRADES: &str = "Render.AutomaticDegrades";
/// `Render.BuildingDetailTextures`.
pub const BUILDING_DETAIL_TEXTURES: &str = "Render.BuildingDetailTextures";
/// `Render.DegradeDistance`.
pub const DEGRADE_DISTANCE: &str = "Render.DegradeDistance";
/// `Render.EnvironmentTextureDetail`.
pub const ENVIRONMENT_TEXTURE_DETAIL: &str = "Render.EnvironmentTextureDetail";
/// `Render.FieldOfView`.
pub const FIELD_OF_VIEW: &str = "Render.FieldOfView";
/// `Render.GraphicsPerformance`.
pub const GRAPHICS_PERFORMANCE: &str = "Render.GraphicsPerformance";
/// `Render.Ground`.
pub const GROUND: &str = "Render.Ground";
/// `Render.LandscapeDetailTextures`.
pub const LANDSCAPE_DETAIL_TEXTURES: &str = "Render.LandscapeDetailTextures";
/// `Render.LandscapeDrawDistance`.
pub const LANDSCAPE_DRAW_DISTANCE: &str = "Render.LandscapeDrawDistance";
/// `Render.LandscapeTextureDetail`.
pub const LANDSCAPE_TEXTURE_DETAIL: &str = "Render.LandscapeTextureDetail";
/// `Render.MultiPassAlpha`.
pub const MULTI_PASS_ALPHA: &str = "Render.MultiPassAlpha";
/// `Render.Objects`.
pub const OBJECTS: &str = "Render.Objects";
/// `Render.SceneryDrawDistance`.
pub const SCENERY_DRAW_DISTANCE: &str = "Render.SceneryDrawDistance";
/// `Render.ScreenBrightness`.
pub const SCREEN_BRIGHTNESS: &str = "Render.ScreenBrightness";
/// `Render.Sky`.
pub const SKY: &str = "Render.Sky";
/// `Render.TerrainBlending`.
pub const TERRAIN_BLENDING: &str = "Render.TerrainBlending";
/// `Render.TextureFiltering`.
pub const TEXTURE_FILTERING: &str = "Render.TextureFiltering";
/// `Sound.AmbientSoundDisabled`.
pub const AMBIENT_SOUND_DISABLED: &str = "Sound.AmbientSoundDisabled";
/// `Sound.AmbientSoundVolume`.
pub const AMBIENT_SOUND_VOLUME: &str = "Sound.AmbientSoundVolume";
/// `Sound.InterfaceSoundDisabled`.
pub const INTERFACE_SOUND_DISABLED: &str = "Sound.InterfaceSoundDisabled";
/// `Sound.InterfaceSoundVolume`.
pub const INTERFACE_SOUND_VOLUME: &str = "Sound.InterfaceSoundVolume";
/// `Sound.PlaySoundOnlyWhenActive`.
pub const PLAY_SOUND_ONLY_WHEN_ACTIVE: &str = "Sound.PlaySoundOnlyWhenActive";
/// `Sound.SoundDisabled`.
pub const SOUND_DISABLED: &str = "Sound.SoundDisabled";
/// `Sound.SoundFeatures`.
pub const SOUND_FEATURES: &str = "Sound.SoundFeatures";
/// `Sound.SoundVolume`.
pub const SOUND_VOLUME: &str = "Sound.SoundVolume";
/// `UI.ChatFontFace`.
pub const CHAT_FONT_FACE: &str = "UI.ChatFontFace";
/// `UI.ChatFontSize`.
pub const CHAT_FONT_SIZE: &str = "UI.ChatFontSize";
/// `UI.Classic.InvertMouseLook`.
pub const CLASSIC_INVERT_MOUSE_LOOK: &str = "UI.Classic.InvertMouseLook";
/// `UI.Classic.RightClickMouseLook`.
pub const CLASSIC_RIGHT_CLICK_MOUSE_LOOK: &str = "UI.Classic.RightClickMouseLook";
/// `UI.Classic.ShowFriendsTab`.
pub const CLASSIC_SHOW_FRIENDS_TAB: &str = "UI.Classic.ShowFriendsTab";
/// `UI.Classic.ShowSquelchTab`.
pub const CLASSIC_SHOW_SQUELCH_TAB: &str = "UI.Classic.ShowSquelchTab";
/// `UI.Classic.ShowTradeTab`.
pub const CLASSIC_SHOW_TRADE_TAB: &str = "UI.Classic.ShowTradeTab";
/// `UI.Classic.StretchUI`.
pub const CLASSIC_STRETCH_UI: &str = "UI.Classic.StretchUI";
/// `UI.Interface`.
pub const INTERFACE: &str = "UI.Interface";
/// `Render.Renderer`: the graphics backend the client starts on, `vulkan`, `d3d12` or `wgpu`.
/// Read once, at start-up; `--renderer` on the command line wins over it.
pub const RENDERER: &str = "Render.Renderer";

/// The `[Fidelity]` section: the optional high-fidelity presentation of the world, drawn only
/// while the Horizon interface is shown. None of these is a retail preference. Each box is a
/// switch the profile keeps as `True` or `False`; a number above 1 (from `--set-at`) asks its
/// effect for that quality level, 0 Off, 1 Low, 2 Medium, 3 High, 4 Ultra.
pub mod fidelity {
    /// `Fidelity.Lighting`: per-pixel sun and sky light in high dynamic range.
    pub const LIGHTING: &str = "Fidelity.Lighting";
    /// `Fidelity.Shadows`: the sun's shadows, drawn inside the lighting.
    pub const SHADOWS: &str = "Fidelity.Shadows";
    /// `Fidelity.GlobalIllumination`: bounced light, drawn inside the lighting.
    pub const GLOBAL_ILLUMINATION: &str = "Fidelity.GlobalIllumination";
    /// `Fidelity.AmbientOcclusion`: ambient and contact occlusion.
    pub const AMBIENT_OCCLUSION: &str = "Fidelity.AmbientOcclusion";
    /// `Fidelity.Lamps`: outdoor lamps, lanterns, torches and braziers give light at night,
    /// drawn inside the lighting on a device that traces rays.
    pub const LAMPS: &str = "Fidelity.Lamps";
    /// `Fidelity.Sky`: a physical sky and aerial perspective.
    pub const SKY: &str = "Fidelity.Sky";
    /// `Fidelity.Weather`: rain, snow and wet ground, when the day's own weather brings them.
    pub const WEATHER: &str = "Fidelity.Weather";
    /// `Fidelity.Debug`: a diagnostic view, set only by `--set-at`: 0 Off, 1 Passthrough,
    /// 2 Depth, 3 Normals, 4 AO, 5 Shadows, 6 Census, 7 Parity (re-shaded with no pass).
    pub const DEBUG: &str = "Fidelity.Debug";
    /// `Fidelity.Interface`: whether the interface shown is Horizon, the only one the
    /// presentation draws under. The client sets it as the interface changes; it is never kept
    /// in the profile.
    pub const INTERFACE: &str = "Fidelity.Interface";

    /// Every name the profile and `--set-at` may set, the seven boxes first.
    pub const NAMES: &[&str] = &[
        LIGHTING,
        SHADOWS,
        GLOBAL_ILLUMINATION,
        AMBIENT_OCCLUSION,
        LAMPS,
        SKY,
        WEATHER,
        DEBUG,
    ];
}
