//! With a real string resolver every Client Options row, the six section headers and twelve slider
//! end captions carry retail strings on the element the client writes; no registry means no labels
//! and 0..1 sliders; no string table records the requested id; slider ranges come from the
//! registry.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use crate::common::layout::Strings;
use std::rc::Rc;

use dereth_primitives::DataId;
use dereth_ui::framework::{LayoutEnum, Screen};
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::common::*;
use dereth_ui_screens::env::create_and_add_root_element;
use dereth_ui_screens::options::page::{OptionControl, PlayerOptionPage};
use dereth_ui_screens::options::{config, page, preferences};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

fn env(with_strings: bool) -> UiSystem {
    let (mut ui, _flow, store) =
        crate::common::layout::load((800, 600), RegistrationOrder::AfterResolver);
    if with_strings {
        ui.strings = Some(Rc::new(Strings(Rc::clone(&store))));
    }
    ui
}

fn screen(with_strings: bool) -> (UiSystem, GamePlayScreen) {
    let mut ui = env(with_strings);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    (ui, s)
}

/// The text on one element, or `""`.
fn text_of(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

/// The text on the first descendant of `row` with element id `id`.
fn child_text(ui: &mut UiSystem, row: ElemHandle, id: u32) -> Option<String> {
    let h = ui.get_child_recursive(row, ElementId(id))?;
    let t = text_of(ui, h);
    (!t.is_empty()).then_some(t)
}

// -------------------------------------------------------------------------------------------
// The captions
// -------------------------------------------------------------------------------------------

/// Every row on the client options page carries its retail caption.
#[test]
fn every_row_on_the_client_options_page_carries_its_retail_caption() {
    let (mut ui, s) = screen(true);
    let p = &s.config_page;
    assert_eq!(p.retail_options().count(), 30, "30 controls over 27 rows");
    assert_eq!(p.failures, 0);

    // Every control that has a caption element got a caption.
    let labelled = p.retail_options().filter(|o| o.label.is_some()).count();
    assert_eq!(labelled, 27, "27 of the 30 controls are captioned");
    let token_but_no_text = p
        .retail_options()
        .filter(|o| o.label_token != 0 && o.label.is_none())
        .count();
    assert_eq!(
        token_but_no_text, 3,
        "the three paired sliders have a token and no element"
    );
    assert_eq!(
        p.retail_options().filter(|o| o.label_token == 0).count(),
        0,
        "every retail control on the page resolved a string id through inq_preference"
    );

    // …and every one of the 27 *rows* shows text, counted off the tree rather than off the page's
    // own bookkeeping: a row is captioned when one of the three caption elements has glyphs.
    let rows: Vec<ElemHandle> = p.option_box.as_ref().expect("option box").items.clone();
    assert_eq!(
        rows.len(),
        43,
        "27 control rows, this client's 3 rows from another era and its interface and performance rows, 6 headers,          5 separators"
    );
    let mut captioned = 0;
    let mut blank: Vec<usize> = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        let is_control = p.retail_options().any(|o| o.row == *r);
        if !is_control {
            continue;
        }
        let any = [0x1000_0219u32, 0x1000_021B, 0x1000_0223]
            .into_iter()
            .any(|id| child_text(&mut ui, *r, id).is_some());
        if any {
            captioned += 1;
        } else {
            blank.push(i);
        }
    }
    assert_eq!(
        captioned, 27,
        "rows with a caption; blank rows at {blank:?}"
    );

    // The strings themselves, from the shipped `Preference` table. One per section, plus every
    // row whose caption is not obviously derivable from its preference name.
    let cap = |pref: &str| -> String {
        p.options
            .iter()
            .find(|o| o.preference == pref)
            .unwrap_or_else(|| panic!("{pref} is not on the page"))
            .label
            .clone()
            .unwrap_or_else(|| panic!("{pref} has no caption"))
    };
    assert_eq!(cap("Sound.SoundFeatures"), "Sound Features");
    assert_eq!(cap("Sound.SoundDisabled"), "Sound Effects");
    assert_eq!(cap("Sound.AmbientSoundDisabled"), "Ambient Sounds");
    assert_eq!(cap("Sound.InterfaceSoundDisabled"), "Interface Sounds");
    assert_eq!(
        cap("Sound.PlaySoundOnlyWhenActive"),
        "Play Sounds Only When Active"
    );
    assert_eq!(cap("Camera.Stiffness"), "Camera Stiffness");
    assert_eq!(cap("Camera.AdjustmentSpeed"), "Camera Adjustment Speed");
    assert_eq!(cap("Render.FieldOfView"), "Field of View");
    assert_eq!(cap("Camera.AlignToSlope"), "Align camera to slope");
    assert_eq!(cap("Display.Resolution"), "Resolution");
    assert_eq!(cap("Display.FullScreen"), "Full Screen");
    assert_eq!(cap("Display.SyncToRefresh"), "Sync with Refresh Rate");
    assert_eq!(cap("Render.ScreenBrightness"), "Screen Brightness");
    assert_eq!(cap("Render.AutomaticDegrades"), "Adaptive Degrade");
    assert_eq!(cap("Render.GraphicsPerformance"), "Adaptive Degrade Bias");
    assert_eq!(cap("Render.DegradeDistance"), "Degrade Distance");
    assert_eq!(
        cap("Render.LandscapeTextureDetail"),
        "Landscape Texture Detail"
    );
    assert_eq!(
        cap("Render.EnvironmentTextureDetail"),
        "Environment Texture Detail"
    );
    assert_eq!(cap("Render.TextureFiltering"), "Texture Filtering");
    assert_eq!(
        cap("Render.LandscapeDrawDistance"),
        "Landscape Draw Distance"
    );
    // …and the one whose caption names something other than the preference does: the
    // preference is `BuildingDetailTextures`, the token is `ID_Graphics_BuildingDetailTextures`
    // and the string says *Environment*. A rebuild that "tidied" either name would break this.
    assert_eq!(
        cap("Render.BuildingDetailTextures"),
        "Environment Detail Textures"
    );
    assert_eq!(cap("Render.MultiPassAlpha"), "Multiple Pass Alpha");
    assert_eq!(cap("Input.MouseLookSensitivity"), "Mouselook Sensitivity");
    assert_eq!(cap("Input.InvertMouseLookYAxis"), "Invert mouselook axes");
    assert_eq!(
        cap("Input.UseMouseTurning"),
        "Turn your character with camera turning."
    );
    assert_eq!(cap("UI.ChatFontFace"), "Chat Font Face");
    assert_eq!(cap("UI.ChatFontSize"), "Chat Font Size");
    // This client's three rows from another era carry literal captions, on the tree as well.
    assert_eq!(cap("Render.Ground"), "Terrain Mode");
    assert_eq!(cap("Render.Sky"), "Sky Mode");
    assert_eq!(cap("Render.Objects"), "Object Mode");
    for o in p.landscape_options() {
        assert_eq!(
            child_text(&mut ui, o.row, 0x1000_0223).as_deref(),
            o.label.as_deref(),
            "{}",
            o.preference
        );
    }
}

/// The caption lands on the element the client writes it to.
#[test]
fn the_caption_lands_on_the_element_the_client_writes_it_to() {
    let (mut ui, s) = screen(true);
    let p = &s.config_page;
    let by = |pref: &str| {
        p.options
            .iter()
            .find(|o| o.preference == pref)
            .unwrap_or_else(|| panic!("{pref}"))
    };

    // A plain toggle: the text is on the control.
    let cb = by("Camera.AlignToSlope");
    assert_eq!(cb.control, OptionControl::Checkbox);
    assert_eq!(text_of(&mut ui, cb.element), "Align camera to slope");
    assert_eq!(
        ui.node(cb.element).map(|n| n.element_id()),
        Some(ElementId(0x1000_0219)),
        "the check box is 0x10000219"
    );

    // A slider row: the text is on 0x1000021B and the bar carries none.
    let sl = by("Camera.Stiffness");
    assert_eq!(sl.control, OptionControl::Slider);
    assert_eq!(
        child_text(&mut ui, sl.row, 0x1000_021B).as_deref(),
        Some("Camera Stiffness")
    );
    assert_eq!(
        text_of(&mut ui, sl.element),
        "",
        "0x1000021C is a scrollbar, not a label"
    );

    let mn = by("UI.ChatFontFace");
    assert_eq!(mn.control, OptionControl::Menu);
    assert_eq!(
        child_text(&mut ui, mn.row, 0x1000_0223).as_deref(),
        Some("Chat Font Face")
    );
    assert_eq!(
        text_of(&mut ui, mn.element),
        "",
        "0x10000224 is the menu, not the label"
    );
    assert_eq!(
        ui.node(mn.element).map(|n| n.element_id()),
        Some(ElementId(0x1000_0224))
    );

    // A check+slider row: the caption is the check box's, and the row has no 0x1000021B at all.
    let pair_box = by("Sound.AmbientSoundDisabled");
    let pair_bar = by("Sound.AmbientSoundVolume");
    assert_eq!(pair_box.row, pair_bar.row, "the two share one list-box row");
    assert_eq!(text_of(&mut ui, pair_box.element), "Ambient Sounds");
    assert!(
        ui.get_child_recursive(pair_bar.row, ElementId(0x1000_021B))
            .is_none(),
        "template 5 carries no slider name label in the shipped layout"
    );
    assert_eq!(
        pair_bar.label, None,
        "…so the paired slider records no caption"
    );
    assert_ne!(pair_bar.label_token, 0, "…but it did resolve a string id");
    assert_eq!(
        pair_bar.label_token,
        dereth_primitives::num::hash::str_hash(b"ID_Sound_AmbientVolume")
    );
}

/// Behaviour: options.client-page.every-row-header-and-slider-end-carries-its-shipped-caption
/// Headers and slider end labels — the two
/// caption kinds whose string ids are **literals in retail** rather
/// than coming out of the preference registry.
///
/// Six headers, and six slider labels making twelve end captions. Both counts asserted,
/// and both sets of strings spelled out.
#[test]
fn the_six_section_headers_and_the_twelve_slider_end_captions_are_the_shipped_strings() {
    let (mut ui, s) = screen(true);
    let p = &s.config_page;
    assert_eq!(p.headers, 6);
    assert_eq!(
        p.separators, 5,
        "a separator between sections, and one at the end"
    );
    assert_eq!(p.header_captions, 6, "every AddHeader caption resolved");
    assert_eq!(
        p.slider_end_captions, 12,
        "six set_slider_label calls, two captions each"
    );

    // The headers, in page order, read off the rows themselves — template 0's row *is* the text.
    let rows = &p.option_box.as_ref().expect("option box").items;
    let mut headers: Vec<String> = Vec::new();
    for r in rows.clone() {
        if p.options.iter().any(|o| o.row == r) {
            continue;
        }
        let t = text_of(&mut ui, r);
        if !t.is_empty() {
            headers.push(t);
        }
    }
    assert_eq!(
        headers,
        vec![
            "Sound Options",
            "Camera Options",
            "Graphics Options",
            "Rendering Quality Options",
            "Input Options",
            "UI Options",
        ]
    );
    assert_eq!(config::SECTIONS.len(), headers.len());

    // The twelve end captions, per row. `Input.MouseLookSensitivity` is the narrow slider and
    // must have none — that is the assertion that would catch a rebuild that gave every slider
    // template 6.
    let ends = |ui: &mut UiSystem, pref: &str| -> (Option<String>, Option<String>) {
        let row = p
            .options
            .iter()
            .find(|o| o.preference == pref)
            .expect("on the page")
            .row;
        (
            child_text(ui, row, 0x1000_021E),
            child_text(ui, row, 0x1000_021F),
        )
    };
    for (pref, l, r) in [
        ("Camera.Stiffness", "Soft", "Hard"),
        ("Camera.AdjustmentSpeed", "Slow", "Fast"),
        ("Render.FieldOfView", "Narrow", "Wide"),
        ("Render.ScreenBrightness", "Dark", "Bright"),
        ("Render.GraphicsPerformance", "Speed", "Detail"),
        ("Render.DegradeDistance", "Close", "Far"),
    ] {
        assert_eq!(
            ends(&mut ui, pref),
            (Some(l.to_string()), Some(r.to_string())),
            "{pref}"
        );
    }
    assert_eq!(
        ends(&mut ui, "Input.MouseLookSensitivity"),
        (None, None),
        "the narrow slider"
    );
    assert_eq!(
        ends(&mut ui, "Sound.AmbientSoundVolume"),
        (None, None),
        "the paired sliders"
    );
}

/// With no registry no row is labelled and every slider is zero to one.
#[test]
fn with_no_registry_no_row_is_labelled_and_every_slider_is_zero_to_one() {
    let mut ui = env(true);
    // `register_all` ran `init_ui_preferences`; take it away again and rebuild the page.
    preferences::clear();
    assert_eq!(preferences::len(), 0);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let p = &s.config_page;

    assert_eq!(
        p.retail_options().count(),
        30,
        "the rows still build; only the captions are gone"
    );
    assert_eq!(p.retail_options().filter(|o| o.label.is_some()).count(), 0);
    assert_eq!(p.options.iter().filter(|o| o.label_token != 0).count(), 0);
    // This client's own rows have no registry entry to lose: their captions are literal.
    assert_eq!(
        p.landscape_options().filter(|o| o.label.is_some()).count(),
        3
    );
    assert_eq!(
        p.slider_end_captions, 12,
        "set_slider_label's ids are literals, not registry rows"
    );
    assert_eq!(p.header_captions, 6, "AddHeader's ids are literals too");
    // Every slider falls back to the slider option's constructor range.
    for o in &p.options {
        if o.control == OptionControl::Slider {
            assert_eq!((o.lower, o.upper), (0.0, 1.0), "{}", o.preference);
        }
    }
    assert_eq!(page::slider_range("Camera.AdjustmentSpeed"), (0.0, 1.0));
}

/// With no string table the page records which string it asked for.
#[test]
fn with_no_string_table_the_page_records_which_string_it_asked_for() {
    let (_ui, s) = screen(false);
    let p = &s.config_page;
    assert_eq!(p.retail_options().count(), 30);
    assert_eq!(
        p.retail_options().filter(|o| o.label.is_some()).count(),
        0,
        "nothing resolved"
    );
    assert_eq!(
        p.options.iter().filter(|o| o.label_token != 0).count(),
        30,
        "all 30 asked"
    );
    assert_eq!(p.header_captions, 0);
    assert_eq!(p.slider_end_captions, 0);
    // The ids are the real ones, as literals.
    let by = |pref: &str| {
        p.options
            .iter()
            .find(|o| o.preference == pref)
            .expect("on the page")
    };
    assert_eq!(
        by("Camera.Stiffness").label_token,
        dereth_primitives::num::hash::str_hash(b"ID_Camera_Stiffness")
    );
    assert_eq!(
        by("Camera.Stiffness").tooltip_token,
        dereth_primitives::num::hash::str_hash(b"ID_Camera_Stiffness_Help")
    );
    // …and the ranges still come through, because they are not strings.
    assert_eq!(
        (by("Camera.Stiffness").lower, by("Camera.Stiffness").upper),
        (0.285_714_3, 1.0)
    );
}

/// The slider ranges come from the registry.
#[test]
fn the_slider_ranges_come_from_the_registry() {
    let (ui, s) = screen(true);
    let p = &s.config_page;
    let r = |pref: &str| {
        let o = p
            .options
            .iter()
            .find(|o| o.preference == pref)
            .expect("on the page");
        (o.lower, o.upper)
    };
    assert_eq!(r("Sound.SoundVolume"), (0.0, 1.0));
    assert_eq!(r("Camera.Stiffness"), (0.285_714_3, 1.0));
    assert_eq!(r("Camera.AdjustmentSpeed"), (5.0, 80.0));
    assert_eq!(r("Render.FieldOfView"), (10.0, 160.0));
    assert_eq!(r("Render.ScreenBrightness"), (-1.0, 1.0));
    assert_eq!(r("Render.GraphicsPerformance"), (-1.0, 1.0));
    assert_eq!(r("Render.DegradeDistance"), (0.0, 100.0));
    assert_eq!(r("Input.MouseLookSensitivity"), (0.01, 1.0));
    // Two ranges the page never uses, straight off the registry.
    assert_eq!(
        preferences::inq_preference_range("Misc.TooltipDelay"),
        Some((0.0, 10.0))
    );
    assert_eq!(
        preferences::inq_preference_range("Camera.AlignToSlope"),
        None
    );
    assert_eq!(preferences::table(&ui), DataId(0x2300_0003));
}

/// The registry is filled for **every** host, not only for a screen that happens to build the
/// options page: `register_all` is the client's UI-init registration half, and the client
/// calls it once.
///
/// This is the wire. Without it `init_ui_preferences` would be one more transcribed function whose
/// only caller is a test.
#[test]
fn register_all_fills_the_registry() {
    let mut ui = UiSystem::new((800, 600));
    let mut flow = dereth_ui::UiFlow::new();
    preferences::clear();
    dereth_ui_screens::options::store::clear();
    assert_eq!(preferences::len(), 0);
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    assert_eq!(
        preferences::len(),
        34,
        "all 34 preference attachments landed"
    );
    assert_eq!(
        dereth_ui_screens::options::store::len(),
        39,
        "…over 34 registered variables, this client's three options from another era, its          interface choice and its performance panel"
    );
    let (table, label, _) =
        preferences::inq_preference("Camera.AlignToSlope").expect("inq_preference answers");
    assert_eq!(table, 0x1000_0003);
    assert_eq!(
        label,
        dereth_primitives::num::hash::str_hash(b"ID_Camera_AlignToSlope")
    );
}

/// The option box under the Client Options panel is the one this page binds, and it is reachable from the
/// shipped tree without a `GamePlayScreen` — the same route
/// `options_client_page::every_child_id_in_the_control_table_is_the_one_the_shipped_layout_carries`
/// takes. Guards against the page above being built from something other than the retail layout.
#[test]
fn the_page_under_test_is_the_shipped_one() {
    let mut ui = env(true);
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
            .expect("classic_gameplay builds");
    let mut all = Vec::new();
    walk(&ui, root, &mut all);
    let page = all
        .iter()
        .copied()
        .find(|h| {
            ui.node(*h)
                .is_some_and(|n| n.element_id() == config::CONFIG_PAGE_ELEMENT)
        })
        .expect("configuration panel");
    let built = page::config_post_init(&mut ui, page).expect("the config page's post-init");
    assert_eq!(built.retail_options().count(), 30);
    assert_eq!(
        built.retail_options().filter(|o| o.label.is_some()).count(),
        27
    );
    assert_eq!(built.landscape_options().count(), 3);
    assert_eq!(built.header_captions, 6);
    assert_eq!(built.slider_end_captions, 12);
    assert_eq!(built.failures, 0);
    let _: &PlayerOptionPage = &built;
}
