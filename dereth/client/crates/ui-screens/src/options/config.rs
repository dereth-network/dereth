//! The Client Options controls projected from the shared sheet, with defaults distinct from
//! preference registration.
//!
//! The trap here is deliberate and must **not** be reconciled: the value the page passes as its
//! row default — which the page's own *Defaults* button restores — is not always the value the
//! preference was registered with. Three disagree: restoring defaults turns automatic degrades
//! off rather than on, doubles the mouse sensitivity and picks trilinear filtering. The
//! resolution is not one of them: *Defaults* puts it at 1024×768, the size the client starts at
//! (retail's page put it at 800×600).

use crate::view::{PrefValue, UiRequest};

/// Resolve a shared caption through the current UI's existing string service.
#[must_use]
pub fn resolve_text(
    ui: &dereth_ui::UiSystem,
    text: dereth_client_contract::options::sheet::Text,
) -> String {
    text.token
        .and_then(|token| {
            let strings = ui.strings.as_ref()?;
            let table = ui
                .env()
                .and_then(|env| env.did_by_enum(4, text.table_enum))
                .unwrap_or(dereth_primitives::DataId(
                    0x2300_0000 | (text.table_enum & 0xffff),
                ));
            dereth_ui::text::render_token(strings.as_ref(), table, token, &[])
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| text.fallback.to_owned())
}

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
    /// The fallback caption of the shared section containing this row.
    pub section: &'static str,
    pub control: Control,
    /// The preference name the control writes.
    pub preference: &'static str,
    /// For a check+slider row, the float preference the slider writes.
    pub slider_preference: Option<&'static str>,
    /// The shared default of the paired volume.
    pub slider_default: Option<f32>,
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
use PrefValueConst::{Bool, Float};

/// Derive a control from the shared sheet, including controls hosted by the Chat page.
#[must_use]
pub fn config_row(preference: &str) -> Option<ConfigRow> {
    config_rows().find(|r| r.preference == preference)
}

/// Every preference row available to this interface, in shared page order.
pub fn config_rows() -> impl Iterator<Item = ConfigRow> {
    use dereth_client_contract::options::interface::Interface;
    use dereth_client_contract::options::sheet::{self, Value};
    sheet::PAGES.iter().flat_map(|p| p.headings).flat_map(|h| {
        h.rows
            .iter()
            .filter(|r| r.shown.on(Interface::Modern))
            .filter_map(|r| {
                let control = match r.value {
                    Value::Check(_) => Check,
                    Value::Slider(_) => Slider {
                        wide: r.slider_ends.is_some(),
                    },
                    Value::Menu(_) => Menu,
                    Value::Sound { .. } => CheckSlider,
                    _ => return None,
                };
                Some(ConfigRow {
                    section: h.text.fallback,
                    control,
                    preference: r.preference()?,
                    slider_preference: match r.value {
                        Value::Sound { volume, .. } => Some(volume),
                        _ => None,
                    },
                    slider_default: r.volume_default,
                    ui_default: r.default?,
                    slider_ends: r.slider_ends,
                    confirm_change: r.confirm_change,
                })
            })
    })
}

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

/// One row of the mouse-turning preset: the preference the Game / Support page's *Use Mouse
/// Turning Settings* button sets, the value it sets it to, and the chat line it prints when the
/// value changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseTurningPreset {
    pub preference: &'static str,
    /// The value the preset writes.
    pub value: PrefValueConst,
    /// The chat line the preset prints, the old value first and the preset's second for the
    /// three sliders (each `%f`, six decimal places).
    pub message: &'static str,
}

/// The mouse-turning preset, in the order the button sets it: a row whose current value already
/// equals the preset's is left alone and prints nothing.
///
/// The sixth row, *Turn to Face Camera* in the chat line, is the page's *Turn your character with
/// camera turning* check box, `Input.UseMouseTurning`.
pub const MOUSE_TURNING_PRESET: [MouseTurningPreset; 6] = [
    MouseTurningPreset {
        preference: "Camera.Stiffness",
        value: Float(0.95),
        message: "Camera Stiffness was changed from %f to the mouse turning default of %f.",
    },
    MouseTurningPreset {
        preference: "Camera.AdjustmentSpeed",
        value: Float(50.0),
        message: "Camera Adjustment was changed from %f to the mouse turning default of %f.",
    },
    MouseTurningPreset {
        preference: "Input.MouseLookSensitivity",
        value: Float(0.7),
        message: "Mouse Sensitivity was changed from %f to the mouse turning default of %f.",
    },
    MouseTurningPreset {
        preference: "Camera.AlignToSlope",
        value: Bool(false),
        message: "Align To Slope was changed from TRUE to the mouse turning default of FALSE.",
    },
    MouseTurningPreset {
        preference: "Input.InvertMouseLookYAxis",
        value: Bool(true),
        message:
            "Invert Mouselook Axes was changed from FALSE to the mouse turning default of TRUE.",
    },
    MouseTurningPreset {
        preference: "Input.UseMouseTurning",
        value: Bool(true),
        message: "Turn to Face Camera was changed from FALSE to the mouse turning default of TRUE.",
    },
];

/// The chat line a preset row prints when it moves `old` to its value: the slider rows fill
/// their two `%f`s with six decimal places, as C's `%f` does.
#[must_use]
pub fn mouse_turning_message(row: &MouseTurningPreset, old: &PrefValue) -> String {
    match (row.value, old) {
        (Float(new), PrefValue::Float(old)) => row
            .message
            .replacen("%f", &format!("{old:.6}"), 1)
            .replacen("%f", &format!("{new:.6}"), 1),
        _ => row.message.to_owned(),
    }
}

/// The chat channel the preset's lines are printed on.
pub const MOUSE_TURNING_CHANNEL: u32 = 7;

/// The two key rebindings prints.
pub const MOUSE_TURNING_KEY_MESSAGES: [&str; 2] = [
    "The key for Camera Zoom In has been changed to the mouse wheel up.",
    "The key for Camera Zoom Out has been changed to the mouse wheel down.",
];

/// The option page's restore-defaults, the page's own *Defaults* button.
///
/// Returns `(preference, value)` for every row, using the **UI** default — including the three that
/// disagree with the registration.
/// The rows run in the page's own order, the shared options set's Client Options page as the
/// modern interface shows it.
#[must_use]
pub fn restore_default_values() -> Vec<(&'static str, PrefValue)> {
    use dereth_client_contract::options::interface::Interface;
    use dereth_client_contract::options::sheet::{self, PageId};
    sheet::defaults(PageId::Client, Interface::Modern)
        .into_iter()
        .map(|(name, value)| (name, value.into()))
        .collect()
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
/// which page object's element-message handler ran.
pub const CONFIG_PAGE_ELEMENT: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0213);
/// The option box — the config panel's post-init binds `0x10000200`.
pub const OPTION_BOX: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0200);

/// Restore the page defaults as **preference writes** for the Defaults button.
///
/// The preference-write chain is:
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
/// The shared sheet supplies each row's UI restore value, including the paired sound volume.
#[must_use]
pub fn restore_defaults_requests() -> Vec<UiRequest> {
    restore_default_values()
        .into_iter()
        .map(|(k, v)| UiRequest::SetPreference(k, v))
        .collect()
}

/// The string table enum used by preference labels, tooltips and section headings.
pub const OPTION_STRING_TABLE_ENUM: u32 = 0x1000_0003;

#[cfg(test)]
mod tests {
    use super::*;
    use PrefValueConst::Int;

    /// Behaviour: options.client-page.every-row-header-and-slider-end-carries-its-shipped-caption
    #[test]
    fn heading_descriptors_resolve_escaped_text_and_fall_back_when_missing() {
        use dereth_client_contract::options::sheet::Text;
        use dereth_primitives::DataId;
        #[derive(Debug)]
        struct Strings;
        impl dereth_ui::text::StringResolver for Strings {
            fn resolve_raw(&self, table: DataId, id: u32) -> Option<String> {
                (table == DataId(0x2300_0003)
                    && id == dereth_primitives::num::hash::str_hash(b"ID_Sound_SoundSection"))
                .then(|| r"Sound\nOptions".to_owned())
            }
        }
        let mut ui = dereth_ui::UiSystem::new((800, 600));
        ui.strings = Some(std::rc::Rc::new(Strings));
        let text = Text::preference("ID_Sound_SoundSection", "Sound");
        assert_eq!(resolve_text(&ui, text), "Sound\nOptions");
        assert_eq!(
            resolve_text(&ui, Text::preference("absent", "Honest fallback")),
            "Honest fallback"
        );
        assert_eq!(resolve_text(&ui, Text::literal("Era Look")), "Era Look");
        ui.strings = None;
        assert_eq!(resolve_text(&ui, text), "Sound");
    }

    /// Every rendered preference row has its own shared control metadata.
    #[test]
    fn every_shared_preference_row_builds_a_control() {
        use dereth_client_contract::options::interface::Interface;
        use dereth_client_contract::options::sheet::{self};
        for page in sheet::PAGES {
            for row in sheet::rows_for(page.id, Interface::Modern) {
                if let Some(name) = row.preference() {
                    let config = config_row(name).expect(name);
                    assert_eq!(config.ui_default, row.default.unwrap());
                }
            }
        }
        assert!(config_row("Display.SyncToRefresh").is_none());
    }

    /// Two distinct sources supply these defaults: the configuration-registration table
    /// plus the startup path's quality-3 call into the overall-graphics-quality update. Both values
    /// are asserted for all three preferences, so neither can quietly become the other. The
    /// resolution is restored to the size the client starts at.
    #[test]
    fn the_three_restore_defaults_values_differ_from_the_registered_defaults() {
        for (name, registered, restored) in [
            ("Render.AutomaticDegrades", Bool(true), Bool(false)),
            ("Input.MouseLookSensitivity", Float(0.25), Float(0.55)),
            ("Render.TextureFiltering", Int(0), Int(1)),
        ] {
            assert_ne!(registered, restored, "{name} must disagree");
            let row = config_rows().find(|r| r.preference == name).unwrap();
            assert_eq!(row.ui_default, restored, "{name} restores the UI value");
            assert_eq!(
                super::super::preferences::find(name)
                    .unwrap()
                    .registered_default,
                registered,
                "{name} retains its registration default"
            );
        }

        // The resolution is restored to the size the client starts at, 1024x768: the words pack
        // as width<<16 | height.
        let resolution = config_rows()
            .find(|r| r.preference == "Display.Resolution")
            .unwrap();
        assert_eq!(resolution.ui_default, Int(0x0400_0300));
        assert_eq!(0x0400_0300 >> 16, 1024);
        assert_eq!(0x0400_0300 & 0xFFFF, 768);
        // Full screen is off, registered and restored.
        let full = config_rows()
            .find(|r| r.preference == "Display.FullScreen")
            .unwrap();
        assert_eq!(full.ui_default, Bool(false));
    }

    /// Camera and Input defaults are pinned as both a decimal and
    /// the IEEE-754 word retail holds. Checking the two against each other is what catches a
    /// transcription slip.
    #[test]
    fn the_float_defaults_match_the_registered_ieee_words() {
        let f = |p: &str| match config_rows()
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

    /// only one control confirms its change:
    /// `Display.Resolution`.
    #[test]
    fn exactly_one_control_confirms_its_change() {
        let confirming: Vec<&str> = config_rows()
            .filter(|r| r.confirm_change)
            .map(|r| r.preference)
            .collect();
        assert_eq!(confirming, vec!["Display.Resolution"]);
    }

    /// three check+slider pairs, each defaulting to on with volume 1.0,
    /// and the `*Disabled` inversion note.
    #[test]
    fn the_three_sound_pairs_default_to_on_at_full_volume() {
        let pairs: Vec<ConfigRow> = config_rows().filter(|r| r.control == CheckSlider).collect();
        assert_eq!(pairs.len(), 3);
        for p in &pairs {
            assert!(p.preference.ends_with("Disabled"), "{}", p.preference);
            assert_eq!(p.ui_default, Bool(true), "True means the sound is ON");
            assert!(p.slider_preference.unwrap().ends_with("Volume"));
        }
        assert!(pairs.iter().all(|p| p.slider_default == Some(1.0)));
    }

    /// The page's Defaults restores every control on the page, including the slider half of each
    /// check+slider pair.
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
            24 + 3 + 5,
            "retail's 26 rows (its 27 less Sync with Refresh Rate) less the chat font's two, \
             which are the Chat Options page's, plus the three paired volume sliders, this \
             client's three era rows, its performance panel and its landscape detail texture"
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
        assert_eq!(get("Display.Resolution"), Some(PrefValue::Int(0x0400_0300)));
        assert_eq!(
            get("Render.AutomaticDegrades"),
            Some(PrefValue::Bool(false))
        );
        assert_eq!(
            get("Input.MouseLookSensitivity"),
            Some(PrefValue::Float(0.55))
        );
        assert_eq!(get("Sound.AmbientSoundVolume"), Some(PrefValue::Float(1.0)));
    }

    /// The preset is the six rows the button sets, in its order, with their values.
    #[test]
    fn the_mouse_turning_preset_is_six_rows_with_their_values() {
        let named: Vec<(&str, PrefValueConst)> = MOUSE_TURNING_PRESET
            .iter()
            .map(|p| (p.preference, p.value))
            .collect();
        assert_eq!(
            named,
            vec![
                ("Camera.Stiffness", Float(0.95)),
                ("Camera.AdjustmentSpeed", Float(50.0)),
                ("Input.MouseLookSensitivity", Float(0.7)),
                ("Camera.AlignToSlope", Bool(false)),
                ("Input.InvertMouseLookYAxis", Bool(true)),
                ("Input.UseMouseTurning", Bool(true)),
            ]
        );
        // Every one is a control on the Client Options page.
        for p in &MOUSE_TURNING_PRESET {
            assert!(
                config_rows().any(|r| r.preference == p.preference),
                "{} should be a row on the page",
                p.preference
            );
        }
        assert_eq!(
            mouse_turning_message(&MOUSE_TURNING_PRESET[0], &PrefValue::Float(0.45)),
            "Camera Stiffness was changed from 0.450000 to the mouse turning default of 0.950000."
        );
        assert_eq!(
            mouse_turning_message(&MOUSE_TURNING_PRESET[3], &PrefValue::Bool(true)),
            MOUSE_TURNING_PRESET[3].message
        );
        assert_eq!(MOUSE_TURNING_KEY_MESSAGES.len(), 2);
    }

    /// The option headings use their expected string tables.
    #[test]
    fn option_labels_come_from_string_table_enum_three() {
        assert_eq!(OPTION_STRING_TABLE_ENUM, 0x1000_0003);
    }
}
