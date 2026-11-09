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
/// `Display.RenderScale`.
pub const DISPLAY_RENDER_SCALE: &str = "Display.RenderScale";
/// `Display.Resolution`.
pub const DISPLAY_RESOLUTION: &str = "Display.Resolution";
/// `Display.SlowDownInBackground`.
pub const DISPLAY_SLOW_DOWN_IN_BACKGROUND: &str = "Display.SlowDownInBackground";
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

/// The `[Fidelity]` section: the remaster's options, drawn only while the Horizon interface is
/// shown. None of these is a retail preference. Each box is a switch the profile keeps as `True`
/// or `False`. Names an earlier build kept here and this one does not know are not read.
pub mod fidelity {
    /// `Fidelity.Remaster`: the world is drawn by the remaster, on the Vulkan or Direct3D 12
    /// renderer. Its
    /// name on the page is [`crate::options::remaster::NAME`]'s, which this never spells out.
    pub const REMASTER: &str = "Fidelity.Remaster";
    /// The remaster's sun shadows.
    pub const REMASTER_SHADOWS: &str = "Fidelity.RemasterShadows";
    /// The remaster's sky.
    pub const REMASTER_SKY: &str = "Fidelity.RemasterSky";
    /// The remaster's fog.
    pub const REMASTER_FOG: &str = "Fidelity.RemasterFog";
    /// The remaster's ambient occlusion.
    pub const REMASTER_OCCLUSION: &str = "Fidelity.RemasterOcclusion";
    /// The remaster's lamps indoors.
    pub const REMASTER_INDOOR_LAMPS: &str = "Fidelity.RemasterIndoorLamps";
    /// The remaster's lamps outdoors.
    pub const REMASTER_OUTDOOR_LAMPS: &str = "Fidelity.RemasterOutdoorLamps";
    /// `Fidelity.RemasterBloom`: the remaster's hdr and bloom, while its box is ticked.
    pub const REMASTER_BLOOM: &str = "Fidelity.RemasterBloom";
    /// `Fidelity.RemasterSmoothing`: the remaster's smooth edges, while its box is ticked.
    pub const REMASTER_SMOOTHING: &str = "Fidelity.RemasterSmoothing";
    /// `Fidelity.RemasterWater`: the remaster's water, while its box is ticked.
    pub const REMASTER_WATER: &str = "Fidelity.RemasterWater";
    /// `Fidelity.RemasterPuddles`: the remaster's puddles, while its box is ticked.
    pub const REMASTER_PUDDLES: &str = "Fidelity.RemasterPuddles";
    /// The remaster's rain and snow.
    pub const REMASTER_RAIN: &str = "Fidelity.RemasterRain";
    /// `Fidelity.RemasterDetail`: the remaster's far detail, while its box is ticked.
    pub const REMASTER_DETAIL: &str = "Fidelity.RemasterDetail";
    /// `Fidelity.RemasterBounce`: the remaster's bounced light, while its box is ticked.
    pub const REMASTER_BOUNCE: &str = "Fidelity.RemasterBounce";
    /// `Fidelity.RemasterTracedShadows`: the remaster's sun shadows traced, where the card can.
    pub const REMASTER_TRACED_SHADOWS: &str = "Fidelity.RemasterTracedShadows";
    /// `Fidelity.RemasterTracedOcclusion`: the remaster's occlusion traced, where the card can.
    pub const REMASTER_TRACED_OCCLUSION: &str = "Fidelity.RemasterTracedOcclusion";
    /// `Fidelity.RemasterUpscale`: the remaster draws fewer pixels and FSR makes up the rest.
    pub const REMASTER_UPSCALE: &str = "Fidelity.RemasterUpscale";
    /// `Fidelity.RemasterDlss`: the remaster draws fewer pixels and DLSS makes up the rest, where
    /// its DLL stands beside the client.
    pub const REMASTER_DLSS: &str = "Fidelity.RemasterDlss";
    /// `Fidelity.RemasterScale`: the share of the pixels the remaster draws each way while it
    /// upscales, in percent.
    pub const REMASTER_SCALE: &str = "Fidelity.RemasterScale";
    /// `Fidelity.RemasterGodRays`: how strongly the remaster's sunlit air shows shafts of light,
    /// 0 none, 1 low, 2 high.
    pub const REMASTER_GOD_RAYS: &str = "Fidelity.RemasterGodRays";
    /// `Fidelity.RemasterArtPack`: the art pack in the remaster, while its box is ticked. Naming a
    /// pack ticks it.
    pub const REMASTER_ART_PACK: &str = "Fidelity.RemasterArtPack";
    /// `Fidelity.RemasterGrade`: the remaster's color grade for each kind of day.
    pub const REMASTER_GRADE: &str = "Fidelity.RemasterGrade";
    /// `Fidelity.RemasterAir`: the remaster's air over the land, sky light in the shade and low fog.
    pub const REMASTER_AIR: &str = "Fidelity.RemasterAir";
    /// `Fidelity.RemasterGrass`: the remaster's grass field.
    pub const REMASTER_GRASS: &str = "Fidelity.RemasterGrass";
    /// `Fidelity.RemasterFlowers`: the remaster's flowers and pebbles.
    pub const REMASTER_FLOWERS: &str = "Fidelity.RemasterFlowers";
    /// `Fidelity.Interface`: whether the interface shown is Horizon, the only one the
    /// remaster draws under. The client sets it as the interface changes; it is never kept
    /// in the profile.
    pub const INTERFACE: &str = "Fidelity.Interface";
    /// `Fidelity.ArtPack`: the art pack the remaster draws, the path of its folder or of the
    /// file it was packed into, read at start-up. `--art-pack` on the command line and `DERETH_ART_PACK` in the environment win
    /// over it. No box sets it.
    pub const ART_PACK: &str = "Fidelity.ArtPack";

    /// Every name the profile and `--set-at` may set.
    pub const NAMES: &[&str] = &[
        REMASTER,
        REMASTER_SHADOWS,
        REMASTER_SKY,
        REMASTER_FOG,
        REMASTER_OCCLUSION,
        REMASTER_INDOOR_LAMPS,
        REMASTER_OUTDOOR_LAMPS,
        REMASTER_BLOOM,
        REMASTER_SMOOTHING,
        REMASTER_WATER,
        REMASTER_PUDDLES,
        REMASTER_RAIN,
        REMASTER_DETAIL,
        REMASTER_BOUNCE,
        REMASTER_TRACED_SHADOWS,
        REMASTER_TRACED_OCCLUSION,
        REMASTER_UPSCALE,
        REMASTER_DLSS,
        REMASTER_SCALE,
        REMASTER_GOD_RAYS,
        REMASTER_ART_PACK,
        REMASTER_GRADE,
        REMASTER_AIR,
        REMASTER_GRASS,
        REMASTER_FLOWERS,
    ];

    /// The names among [`NAMES`] that are menus, each a number, rather than boxes.
    pub const MENUS: &[&str] = &[REMASTER_SCALE, REMASTER_GOD_RAYS];

    /// Whether `name` is one of the [`MENUS`].
    #[must_use]
    pub fn is_menu(name: &str) -> bool {
        MENUS.iter().any(|m| m.eq_ignore_ascii_case(name))
    }
}
