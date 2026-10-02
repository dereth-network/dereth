//! The Client Options page, its six sections and the three defaults that disagree
//! with the preference registration.
//!
//! The trap here is deliberate and must **not** be reconciled: the value the page passes as its
//! row default — which global message `0x0C`, *Restore Defaults*, restores — is not always
//! the value the preference was registered with. Three disagree, and they are the ones a player
//! notices: restoring defaults drops a 1024×768 client to 800×600, turns automatic degrades off
//! rather than on, and doubles the mouse sensitivity.

use crate::view::{PrefValue, UiRequest};

/// Which control type a row is, and therefore which page helper built it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// A toggle option `(preference name)`, template 2, child `0x10000219`.
    Check,
    /// A slider option `(preference name, wide)`, template `wide ? 6 : 3`, child `0x1000021C`.
    Slider { wide: bool },
    /// A menu option `(preference name, is UI preference)`, template 4, child `0x10000224`.
    Menu,
    /// A toggle-with-slider option `(bool preference, float preference)`, template 5.
    CheckSlider,
}

/// One row of the config panel's option build.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfigRow {
    /// The section header this row sits under, a string id in table enum `0x10000003`.
    pub section: &'static str,
    pub control: Control,
    /// The preference name the control writes.
    pub preference: &'static str,
    /// For a check+slider row, the float preference the slider writes.
    pub slider_preference: Option<&'static str>,
    /// The default the page gives the option — what *Restore Defaults* restores.
    pub ui_default: PrefValueConst,
    /// The slider's two end captions, where it has them.
    pub slider_ends: Option<(&'static str, &'static str)>,
    /// Confirm-change on: the change applies first, then opens a confirmation dialog whose
    /// No/expiry arm restores the saved value; Yes keeps the already-applied value.
    pub confirm_change: bool,
}

/// The value enum the two tables are written with lives in
/// [`dereth_client_contract::options::config`], with the store that reads them, and is re-exported
/// here.
pub use dereth_client_contract::options::config::PrefValueConst;

use Control::{Check, CheckSlider, Menu, Slider};
use PrefValueConst::{Bool, Float, Int};

const fn row(
    section: &'static str,
    control: Control,
    preference: &'static str,
    ui_default: PrefValueConst,
) -> ConfigRow {
    ConfigRow {
        section,
        control,
        preference,
        slider_preference: None,
        ui_default,
        slider_ends: None,
        confirm_change: false,
    }
}

const fn pair(
    section: &'static str,
    bool_pref: &'static str,
    float_pref: &'static str,
) -> ConfigRow {
    ConfigRow {
        section,
        control: CheckSlider,
        preference: bool_pref,
        slider_preference: Some(float_pref),
        // The check box's own default. The slider's is 1.0 for all three sound pairs.
        ui_default: Bool(true),
        slider_ends: None,
        confirm_change: false,
    }
}

const fn slider(
    section: &'static str,
    preference: &'static str,
    left: &'static str,
    right: &'static str,
    d: f32,
    wide: bool,
) -> ConfigRow {
    ConfigRow {
        section,
        control: Slider { wide },
        preference,
        slider_preference: None,
        ui_default: Float(d),
        slider_ends: Some((left, right)),
        confirm_change: false,
    }
}

/// The six section headers, in page order.
pub const SECTIONS: [&str; 6] = [
    "ID_Sound_SoundSection",
    "ID_Camera_CameraSection",
    "ID_Graphics_GraphicsSection",
    "ID_Graphics_TextureSection",
    "ID_Input_InputSection",
    "ID_UI_UISection",
];

const SOUND: &str = SECTIONS[0];
const CAMERA: &str = SECTIONS[1];
const GRAPHICS: &str = SECTIONS[2];
const TEXTURES: &str = SECTIONS[3];
const INPUT: &str = SECTIONS[4];
const INTERFACE: &str = SECTIONS[5];

/// The Client Options page, row by row, in the order the option build adds them.
///
/// The three `Sound.*Disabled` preferences are **inverted** — `True` means the sound is *on*,
/// despite the name. The UI default is `on` for all three,
/// which is the `True` this table carries.
pub const CONFIG_PAGE: [ConfigRow; 27] = [
    // --- Sound -------------------------------------------------------------------------------
    row(SOUND, Menu, "Sound.SoundFeatures", Int(0)),
    pair(SOUND, "Sound.SoundDisabled", "Sound.SoundVolume"),
    pair(
        SOUND,
        "Sound.AmbientSoundDisabled",
        "Sound.AmbientSoundVolume",
    ),
    pair(
        SOUND,
        "Sound.InterfaceSoundDisabled",
        "Sound.InterfaceSoundVolume",
    ),
    row(SOUND, Check, "Sound.PlaySoundOnlyWhenActive", Bool(true)),
    // --- Camera ------------------------------------------------------------------------------
    slider(
        CAMERA,
        "Camera.Stiffness",
        "ID_Graphics_Value_Soft",
        "ID_Graphics_Value_Hard",
        0.45,
        true,
    ),
    slider(
        CAMERA,
        "Camera.AdjustmentSpeed",
        "ID_Graphics_Value_Slow",
        "ID_Graphics_Value_Fast",
        40.0,
        true,
    ),
    slider(
        CAMERA,
        "Render.FieldOfView",
        "ID_Graphics_Value_Narrow",
        "ID_Graphics_Value_Wide",
        90.0,
        true,
    ),
    row(CAMERA, Check, "Camera.AlignToSlope", Bool(true)),
    // --- Graphics ----------------------------------------------------------------------------
    ConfirmedResolution::ROW,
    row(GRAPHICS, Check, "Display.FullScreen", Bool(true)),
    row(GRAPHICS, Check, "Display.SyncToRefresh", Bool(false)),
    slider(
        GRAPHICS,
        "Render.ScreenBrightness",
        "ID_Graphics_Value_Dark",
        "ID_Graphics_Value_Bright",
        0.0,
        true,
    ),
    row(GRAPHICS, Check, "Render.AutomaticDegrades", Bool(false)),
    slider(
        GRAPHICS,
        "Render.GraphicsPerformance",
        "ID_Graphics_Value_Speed",
        "ID_Graphics_Value_Detail",
        0.0,
        true,
    ),
    slider(
        GRAPHICS,
        "Render.DegradeDistance",
        "ID_Graphics_Value_Close",
        "ID_Graphics_Value_Far",
        50.0,
        true,
    ),
    // --- Textures ----------------------------------------------------------------------------
    row(TEXTURES, Menu, "Render.LandscapeTextureDetail", Int(2)),
    row(TEXTURES, Menu, "Render.EnvironmentTextureDetail", Int(1)),
    row(TEXTURES, Menu, "Render.TextureFiltering", Int(1)),
    row(TEXTURES, Menu, "Render.LandscapeDrawDistance", Int(8)),
    row(TEXTURES, Check, "Render.BuildingDetailTextures", Bool(true)),
    row(TEXTURES, Check, "Render.MultiPassAlpha", Bool(false)),
    // --- Input -------------------------------------------------------------------------------
    ConfigRow {
        section: INPUT,
        control: Slider { wide: false },
        preference: "Input.MouseLookSensitivity",
        slider_preference: None,
        ui_default: Float(0.55),
        slider_ends: None,
        confirm_change: false,
    },
    row(INPUT, Check, "Input.InvertMouseLookYAxis", Bool(false)),
    row(INPUT, Check, "Input.UseMouseTurning", Bool(false)),
    // --- Interface ---------------------------------------------------------------------------
    row(INTERFACE, Menu, "UI.ChatFontFace", Int(2)),
    row(INTERFACE, Menu, "UI.ChatFontSize", Int(1)),
];

/// This client's own three rows, the presentation options from another era
/// ([`dereth_client_contract::options::landscape`]): the ground's era, the sky's and the objects'
/// look. Not retail rows, and not in [`CONFIG_PAGE`]; the page adds them at the end of the
/// Graphics section, after its retail rows. Each is a menu of literal captions (the world's own
/// and its styles), so it needs no string table, and *Restore Defaults* puts it back to World
/// Default.
pub const LANDSCAPE_ROWS: [ConfigRow; 3] = [
    row(
        GRAPHICS,
        Menu,
        dereth_client_contract::options::landscape::GROUND,
        Int(dereth_client_contract::options::landscape::WORLD_DEFAULT),
    ),
    row(
        GRAPHICS,
        Menu,
        dereth_client_contract::options::landscape::SKY,
        Int(dereth_client_contract::options::landscape::WORLD_DEFAULT),
    ),
    row(
        GRAPHICS,
        Menu,
        dereth_client_contract::options::landscape::OBJECTS,
        Int(dereth_client_contract::options::landscape::WORLD_DEFAULT),
    ),
];

/// The section [`LANDSCAPE_ROWS`] close.
pub const LANDSCAPE_SECTION: &str = GRAPHICS;

/// This client's interface choice (the retail interface or the classic one), after the landscape
/// rows: a menu of literal captions, and *Restore Defaults* puts it back to the retail interface.
pub const INTERFACE_ROW: ConfigRow = row(
    GRAPHICS,
    Menu,
    dereth_client_contract::options::interface::INTERFACE,
    Int(0),
);

/// The performance panel, after the interface row: a check box with a literal caption, and
/// *Restore Defaults* turns it off.
pub const PERFORMANCE_ROW: ConfigRow = row(
    GRAPHICS,
    Check,
    dereth_client_contract::options::performance::PERFORMANCE_PANEL,
    Bool(false),
);

/// The volume every one of the three sound check+slider pairs defaults its slider to.
pub const SOUND_SLIDER_DEFAULT: f32 = 1.0;

/// The menus the page adds as **user**-preference menus rather than UI-preference ones.
///
/// Adding a menu option branches on that flag between
/// the menu option control's UI-preference bind, which fills the drop-down from
/// the preference's enum choices (a list of string ids), and
/// its user-preference bind, which fills it from
/// the preference's choice strings — the **ASCII** choices the owning subsystem passed when it
/// registered the preference.
///
/// There is exactly one `false` in the whole function, and it is the resolution list: its choices
/// are the `"%ix%i"` labels the display-preference initialisation builds at device
/// init out of the adapter's enumerated modes, so they cannot be a compiled-in table of string
/// ids. UI-preference initialisation attaches no enum choices for it either, so routing it
/// through the UI-preference leg would produce a drop-down with a popup and no rows.
pub const USER_PREFERENCE_MENUS: [&str; 1] = ["Display.Resolution"];

/// The one control in the whole client that confirms its change: changing the resolution applies
/// first and opens a confirmation whose No/expiry arm restores the saved resolution.
struct ConfirmedResolution;
impl ConfirmedResolution {
    const ROW: ConfigRow = ConfigRow {
        section: GRAPHICS,
        control: Menu,
        preference: "Display.Resolution",
        slider_preference: None,
        // 0x03200258 = 800 << 16 | 600.
        ui_default: Int(0x0320_0258),
        slider_ends: None,
        confirm_change: true,
    };
}

/// One preference whose *Restore Defaults* value differs from its registration default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefaultDisagreement {
    pub preference: &'static str,
    /// The value the preference is registered with.
    pub registered: PrefValueConst,
    /// The value the Client Options page assigns as the row default.
    pub ui_restore: PrefValueConst,
}

/// The four preferences whose registered default and page default disagree.
///
/// **Do not reconcile them.** Both values are real: the registered default is what a fresh
/// `UserPreferences.ini` gets, and the UI value is what *Restore Defaults* writes over it.
pub const DEFAULT_DISAGREEMENTS: [DefaultDisagreement; 4] = [
    DefaultDisagreement {
        preference: "Display.Resolution",
        registered: Int(0x0400_0300), // 1024x768
        ui_restore: Int(0x0320_0258), // 800x600
    },
    DefaultDisagreement {
        preference: "Render.AutomaticDegrades",
        registered: Bool(true),
        ui_restore: Bool(false),
    },
    DefaultDisagreement {
        preference: "Input.MouseLookSensitivity",
        registered: Float(0.25),
        ui_restore: Float(0.55),
    },
    DefaultDisagreement {
        preference: "Render.TextureFiltering",
        registered: Int(0), // Startup feeds quality 3 to the overall-graphics-quality update.
        ui_restore: Int(1),
    },
];

/// The config panel's mouse turning defaults write — one preset row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseTurningPreset {
    pub preference: &'static str,
    /// The value the preset writes, where it is known.
    ///
    /// `None` for the three floats: the chat lines the function prints use `%f`, so the *fact* of
    /// the change is known but not the number.
    // UNVERIFIED: the three float values the mouse-turning defaults write to
    // `Camera.AdjustmentSpeed`, `Camera.Stiffness` and `Input.MouseLookSensitivity` are printed
    // with `%f` in the chat lines and are not otherwise known, and the sixth line ("Turn to Face
    // Camera") names no registered preference. Everything else about the preset — which five
    // preferences it rewrites, the two booleans' values, and the six chat lines — is known.
    pub value: Option<PrefValueConst>,
    /// The chat line the preset prints.
    pub message: &'static str,
}

/// The five preferences the mouse-turning preset rewrites, plus the sixth line it prints for
/// `Turn to Face Camera`.
///
/// The mouse-turning preset writes six other settings and prints six chat lines. It is a
/// visible side effect in retail, not a bug.
pub const MOUSE_TURNING_PRESET: [MouseTurningPreset; 6] = [
    MouseTurningPreset {
        preference: "Camera.AlignToSlope",
        value: Some(Bool(false)),
        message: "Align To Slope was changed from TRUE to the mouse turning default of FALSE.",
    },
    MouseTurningPreset {
        preference: "Camera.AdjustmentSpeed",
        value: None,
        message: "Camera Adjustment was changed from %f to the mouse turning default of %f.",
    },
    MouseTurningPreset {
        preference: "Camera.Stiffness",
        value: None,
        message: "Camera Stiffness was changed from %f to the mouse turning default of %f.",
    },
    MouseTurningPreset {
        preference: "Input.MouseLookSensitivity",
        value: None,
        message: "Mouse Sensitivity was changed from %f to the mouse turning default of %f.",
    },
    MouseTurningPreset {
        preference: "Input.InvertMouseLookYAxis",
        value: Some(Bool(true)),
        message:
            "Invert Mouselook Axes was changed from FALSE to the mouse turning default of TRUE.",
    },
    MouseTurningPreset {
        // The sixth line names no registered preference and no `PlayerOption`; it is printed
        // alongside the five rewrites. UNVERIFIED: which setting, if any, it corresponds to.
        preference: "(unrecovered: \"Turn to Face Camera\")",
        value: Some(Bool(true)),
        message: "Turn to Face Camera was changed from FALSE to the mouse turning default of TRUE.",
    },
];

/// The two key rebindings prints.
pub const MOUSE_TURNING_KEY_MESSAGES: [&str; 2] = [
    "The key for Camera Zoom In has been changed to the mouse wheel up.",
    "The key for Camera Zoom Out has been changed to the mouse wheel down.",
];

/// The option page's restore-defaults, driven by global message `0x0C`.
///
/// Returns `(preference, value)` for every row, using the **UI** default — including the three that
/// disagree with the registration.
#[must_use]
pub fn restore_default_values() -> Vec<(&'static str, PrefValue)> {
    let mut out = Vec::new();
    for (i, r) in CONFIG_PAGE.iter().enumerate() {
        out.push((r.preference, r.ui_default.into()));
        if let Some(s) = r.slider_preference {
            out.push((s, PrefValue::Float(SOUND_SLIDER_DEFAULT)));
        }
        // This client's landscape rows close the Graphics section, as the page builds them.
        let closes = r.section == LANDSCAPE_SECTION
            && CONFIG_PAGE
                .get(i + 1)
                .is_none_or(|n| n.section != LANDSCAPE_SECTION);
        if closes {
            for l in LANDSCAPE_ROWS {
                out.push((l.preference, l.ui_default.into()));
            }
            out.push((INTERFACE_ROW.preference, INTERFACE_ROW.ui_default.into()));
            out.push((
                PERFORMANCE_ROW.preference,
                PERFORMANCE_ROW.ui_default.into(),
            ));
        }
    }
    out
}

/// The three buttons the options-page base puts under every page.
///
/// The character-settings and Client Options pages share the same handler. It
/// switches on the element id for element message 1
/// and makes one call on the option page:
///
/// | element | function |
/// |---|---|
/// | `0x100001FC` | the option page base's save of the current values |
/// | `0x100001FD` | the option page base's restore saved values |
/// | `0x100001FE` | the option page base's restore of the defaults |
///
/// [verified against retail]
///
/// **Only Cancel and Defaults write anything.** Apply *snapshots*: it re-reads each option's
/// current preference into its saved value so that a later Cancel has something to revert to
/// (both current and saved become the preference value), and
/// the player-option page additionally saves its current values to the server. The
/// preference itself was already written while the control was being dragged — see
/// [`restore_defaults_requests`].
pub mod button {
    use dereth_ui::ElementId;
    /// Apply. — a snapshot, not a write.
    pub const APPLY: ElementId = ElementId(0x1000_01FC);
    /// Cancel. — writes the snapshot back.
    pub const CANCEL: ElementId = ElementId(0x1000_01FD);
    /// Defaults. The option page base's restore of the defaults.
    pub const DEFAULTS: ElementId = ElementId(0x1000_01FE);
}

/// The Client Options page's element id, where the option list box binds.
///
/// The three option pages carry the *same* Apply/Cancel/Defaults child ids, so a click is
/// attributed to a page by walking up to one of these — exactly as the client attributes it by
/// which page object's element-message handler ran. [verified against the shipped
/// `classic_gameplay` tree: 1,870 elements, one `0x10000028`, its option box `0x10000200`, and
/// three copies of `0x100001FC`..`0x100001FE` under three different pages.]
pub const CONFIG_PAGE_ELEMENT: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0213);
/// The option box — the config panel's post-init binds `0x10000200`.
pub const OPTION_BOX: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0200);

/// Restore the page defaults as **preference writes** for the Defaults button.
///
/// The client chain, verified at each step, is:
/// restore the slider's default -> write its current value ->
/// apply it -> modify the UI preference ->
/// write the preference store -> write the typed variable,
/// whose body copies the incoming float into the bound variable. A check box takes
/// the corresponding checkbox-apply and preference-write route.
///
/// **This is a real write and it is immediate.** Sound preference registration binds
/// each `Sound.*` key directly to the sound-manager global with a **null** change callback, so
/// the store *is* the global's store; there is no apply step and no reload.
///
/// **The one thing this build supplies statically.** In the client the walk is over the page's
/// option array, which page initialization fills. Here the row list comes from [`CONFIG_PAGE`],
/// which is that function transcribed; the values written are the same either way, because both
/// use the initialization defaults' arguments.
#[must_use]
pub fn restore_defaults_requests() -> Vec<UiRequest> {
    restore_default_values()
        .into_iter()
        .map(|(k, v)| UiRequest::SetPreference(k, v))
        .collect()
}

/// The string table enum every option label and tooltip comes from. Labels and tooltips on option
/// pages come from string table enum `0x10000003`; every other UI string comes from `0x10000001`
/// (and error/status strings from `0x10000002`).
pub const OPTION_STRING_TABLE_ENUM: u32 = 0x1000_0003;

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered options behavior's six section tables, row by row and in page order.
    #[test]
    fn the_client_options_page_is_the_documented_twenty_seven_rows_in_six_sections() {
        assert_eq!(CONFIG_PAGE.len(), 27);
        let counts = SECTIONS.map(|s| CONFIG_PAGE.iter().filter(|r| r.section == s).count());
        assert_eq!(counts, [5, 4, 7, 6, 3, 2], "the six sections' row counts");
        // Rows are grouped: the page never returns to a section it has left.
        let mut order: Vec<&str> = Vec::new();
        for r in CONFIG_PAGE {
            if order.last() != Some(&r.section) {
                assert!(
                    !order.contains(&r.section),
                    "section {} is not contiguous",
                    r.section
                );
                order.push(r.section);
            }
        }
        assert_eq!(order, SECTIONS.to_vec());
    }

    /// Oracle: the historical three differences in §2 and the configuration-registration table,
    /// plus the startup path's quality-3 call into the overall-graphics-quality update. Both values
    /// are asserted for all four preferences, so neither can quietly become the other.
    #[test]
    fn the_four_restore_defaults_values_differ_from_the_registered_defaults() {
        assert_eq!(DEFAULT_DISAGREEMENTS.len(), 4);
        for d in DEFAULT_DISAGREEMENTS {
            assert_ne!(d.registered, d.ui_restore, "{} must disagree", d.preference);
            let row = CONFIG_PAGE
                .iter()
                .find(|r| r.preference == d.preference)
                .unwrap_or_else(|| panic!("{} is not on the page", d.preference));
            assert_eq!(
                row.ui_default, d.ui_restore,
                "{} restores the UI value",
                d.preference
            );
        }
        // The specific values, spelled out.
        assert_eq!(DEFAULT_DISAGREEMENTS[0].registered, Int(0x0400_0300)); // 1024x768
        assert_eq!(DEFAULT_DISAGREEMENTS[0].ui_restore, Int(0x0320_0258)); // 800x600
        assert_eq!(DEFAULT_DISAGREEMENTS[1].registered, Bool(true));
        assert_eq!(DEFAULT_DISAGREEMENTS[1].ui_restore, Bool(false));
        assert_eq!(DEFAULT_DISAGREEMENTS[2].registered, Float(0.25));
        assert_eq!(DEFAULT_DISAGREEMENTS[2].ui_restore, Float(0.55));

        // The resolution words pack as width<<16 | height, which is what makes 0x03200258 800x600.
        assert_eq!(0x0320_0258 >> 16, 800);
        assert_eq!(0x0320_0258 & 0xFFFF, 600);
        assert_eq!(0x0400_0300 >> 16, 1024);
        assert_eq!(0x0400_0300 & 0xFFFF, 768);
    }

    /// Oracle: §2's Camera and Input tables, which give the float defaults as both a decimal and
    /// the IEEE-754 word retail holds. Checking the two against each other is what catches a
    /// transcription slip.
    #[test]
    fn the_float_defaults_match_the_recovered_ieee_words() {
        let f = |p: &str| match CONFIG_PAGE
            .iter()
            .find(|r| r.preference == p)
            .unwrap()
            .ui_default
        {
            Float(v) => v,
            other => panic!("{p} is {other:?}, not a float"),
        };
        assert_eq!(f("Camera.Stiffness").to_bits(), 0x3EE6_6666);
        assert_eq!(f("Camera.AdjustmentSpeed").to_bits(), 0x4220_0000);
        assert_eq!(f("Render.FieldOfView").to_bits(), 0x42B4_0000);
        assert_eq!(f("Render.DegradeDistance").to_bits(), 0x4248_0000);
        assert_eq!(f("Input.MouseLookSensitivity").to_bits(), 0x3F0C_CCCD);
        assert_eq!(f("Render.ScreenBrightness"), 0.0);
        assert_eq!(f("Render.GraphicsPerformance"), 0.0);
    }

    /// Oracle: §8's second rebuild note — only one control confirms its change:
    /// `Display.Resolution`.
    #[test]
    fn exactly_one_control_confirms_its_change() {
        let confirming: Vec<&str> = CONFIG_PAGE
            .iter()
            .filter(|r| r.confirm_change)
            .map(|r| r.preference)
            .collect();
        assert_eq!(confirming, vec!["Display.Resolution"]);
    }

    /// Oracle: §2's Sound table — three check+slider pairs, each defaulting to on with volume 1.0,
    /// and the `*Disabled` inversion note.
    #[test]
    fn the_three_sound_pairs_default_to_on_at_full_volume() {
        let pairs: Vec<&ConfigRow> = CONFIG_PAGE
            .iter()
            .filter(|r| r.control == CheckSlider)
            .collect();
        assert_eq!(pairs.len(), 3);
        for p in pairs {
            assert!(p.preference.ends_with("Disabled"), "{}", p.preference);
            assert_eq!(p.ui_default, Bool(true), "True means the sound is ON");
            assert!(p.slider_preference.unwrap().ends_with("Volume"));
        }
        assert_eq!(SOUND_SLIDER_DEFAULT, 1.0);
    }

    /// Oracle: — global message `0x0C` restores every
    /// control on the page, including the slider half of each check+slider pair.
    #[test]
    fn restore_defaults_writes_every_control_including_the_paired_sliders() {
        let get_landscape = |v: &[(&str, PrefValue)]| {
            [
                dereth_client_contract::options::landscape::GROUND,
                dereth_client_contract::options::landscape::SKY,
                dereth_client_contract::options::landscape::OBJECTS,
            ]
            .map(|p| v.iter().find(|(k, _)| *k == p).map(|(_, x)| x.clone()))
        };
        let v = restore_default_values();
        assert_eq!(
            v.len(),
            27 + 3 + 5,
            "27 rows plus the three paired volume sliders, this client's three era rows, its \
             interface and its performance panel"
        );
        assert_eq!(
            get_landscape(&v),
            [
                Some(PrefValue::Int(0)),
                Some(PrefValue::Int(0)),
                Some(PrefValue::Int(0))
            ]
        );
        let get = |p: &str| v.iter().find(|(k, _)| *k == p).map(|(_, x)| x.clone());
        assert_eq!(get("Display.Resolution"), Some(PrefValue::Int(0x0320_0258)));
        assert_eq!(
            get("Render.AutomaticDegrades"),
            Some(PrefValue::Bool(false))
        );
        assert_eq!(
            get("Input.MouseLookSensitivity"),
            Some(PrefValue::Float(0.55))
        );
        assert_eq!(get("Sound.AmbientSoundVolume"), Some(PrefValue::Float(1.0)));
        assert_eq!(
            dereth_ui::msg::global::RESTORE_DEFAULTS,
            dereth_ui::MessageId(0x0C)
        );
    }

    /// Oracle: the client's six quoted chat lines.
    #[test]
    fn the_mouse_turning_preset_is_five_rewrites_and_six_chat_lines() {
        assert_eq!(MOUSE_TURNING_PRESET.len(), 6);
        let named: Vec<&str> = MOUSE_TURNING_PRESET.iter().map(|p| p.preference).collect();
        assert_eq!(
            named,
            vec![
                "Camera.AlignToSlope",
                "Camera.AdjustmentSpeed",
                "Camera.Stiffness",
                "Input.MouseLookSensitivity",
                "Input.InvertMouseLookYAxis",
                "(unrecovered: \"Turn to Face Camera\")",
            ]
        );
        // The five named preferences are all controls on the Client Options page, which is why
        // ticking Use Mouse Turning visibly moves five other rows.
        for p in &MOUSE_TURNING_PRESET[..5] {
            assert!(
                CONFIG_PAGE.iter().any(|r| r.preference == p.preference),
                "{} should be a row on the page",
                p.preference
            );
        }
        // The two booleans are recovered; the three floats are not.
        assert_eq!(MOUSE_TURNING_PRESET[0].value, Some(Bool(false)));
        assert_eq!(MOUSE_TURNING_PRESET[4].value, Some(Bool(true)));
        assert_eq!(MOUSE_TURNING_PRESET[5].value, Some(Bool(true)));
        let unknown = MOUSE_TURNING_PRESET
            .iter()
            .filter(|p| p.value.is_none())
            .count();
        assert_eq!(
            unknown, 3,
            "the three `%f` values are not in the recovered text"
        );
        assert_eq!(MOUSE_TURNING_KEY_MESSAGES.len(), 2);
    }

    /// Oracle: §8's third rebuild note on the string tables.
    #[test]
    fn option_labels_come_from_string_table_enum_three() {
        assert_eq!(OPTION_STRING_TABLE_ENUM, 0x1000_0003);
    }
}
