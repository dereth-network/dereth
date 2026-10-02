//! The option page and its 27 rows.
//!
//! The shipped `classic_gameplay` tree has the Client Options page `0x10000213` with an option box
//! `0x10000200` carrying **0 children** and none of the seven option-control types instantiated:
//! the rows are built at run time. This module joins the pieces — [`super::config::CONFIG_PAGE`]
//! is the option-row initialization, [`crate::panels::listbox::ListBoxWidget::add_from_template`]
//! is the list box's add-from-template-list, and the eight row templates are roots of the shipped
//! `classic_options` layout `0x2100002B`.
//!
//! # The template indices, measured rather than transcribed
//!
//! The slider-row insertion is easy to read as `wide ? 3 : 6`; **that is backwards**.
//! The option box's template index is `(-wide & 3) + 3`, with `wide` as an unsigned 0 or 1:
//! `-1` is `0xFFFFFFFF`, so `& 3` is `3` and the index is **6**; `wide == false`
//! gives `0 + 3` = **3**. The shipped layout settles it independently: template **6**
//! (`0x1000021D`, 35 px tall) is the only slider row carrying the two end captions `0x1000021E`
//! and `0x1000021F` that the slider-label write fills, and the rows this page marks
//! `wide` are exactly the ones that [`super::config::ConfigRow::slider_ends`] gives captions to.
//! (**Six** of them: the config panel's option build makes exactly six slider-label calls.)
//! Template **3**
//! (`0x1000021A`, 19 px) has no captions, and the one narrow slider on
//! the page — `Input.MouseLookSensitivity` — has none either.
//!
//! The option box's own template list has **eight** entries, not seven:
//!
//! | index | template root | what it is |
//! |---:|---|---|
//! | 0 | `0x10000216` | header — a bare text element |
//! | 1 | `0x10000217` | separator |
//! | 2 | `0x10000218` | toggle; the check box is `0x10000219` |
//! | 3 | `0x1000021A` | **narrow** slider; label `0x1000021B`, bar `0x1000021C` |
//! | 4 | `0x10000222` | menu; label `0x10000223`, control `0x10000224` |
//! | 5 | `0x10000220` | toggle + slider, no end captions |
//! | 6 | `0x1000021D` | **wide** slider, with end captions `0x1000021E`/`0x1000021F` |
//! | 7 | `0x10000221` | toggle + wide slider — **unused by the option-row helpers** |
//!
//! This is verified against the shipped tree by a dat-tier test.
//!
//! # What writes the preference, and when
//!
//! This is the whole reason this page needs no Apply button to work: **a control writes its preference the moment the player moves it**, not on Apply.
//! On element message `0x0A` the slider option control reads its float attribute `0x86` as
//! `frac`, sets the current value to `lower + frac * (upper - lower)`, and applies it at once
//! (modify the preference, which sets the value and writes the variable),
//! and the check-box option control's element-message handler is the same shape on message 1 with
//! attribute `0x0E`. The three buttons underneath do something else entirely:
//!
//! | element | function | effect on the preference |
//! |---|---|---|
//! | `0x100001FC` Apply | | **none** — re-snapshots the saved value so Cancel has something to revert to |
//! | `0x100001FD` Cancel | | writes the **old** value back through the same apply |
//! | `0x100001FE` Defaults | the option page's restore-defaults | writes each option's default value |
//!
//! and the player-option page's visibility-changed handler calls `restore_saved_values` when the page
//! is **hidden**, so closing it without pressing Apply rolls every uncommitted change back.
//!
//! # Three deliberate divergences, named
//!
//! 1. **The value model lives on the page, not on the element.** In the client each option
//!    control *is* the element and keeps its current, saved and default values in its own object. Here the
//!    element carries only its base widget's behaviour (see [`super::controls`]) and the page owns
//!    an array of [`UiOption`], which folds in the original option-array values.
//!    That is this crate's standing convention — "the screen that owns the subtree does the
//!    binding" — and it is what lets every assertion below run without an arena dispatch.
//! 2. **A combined check-box-and-slider row becomes two entries**, a check box on `0x10000219` and a
//!    slider on `0x1000021C`, plus one row in the page's `gated` list so message 1 on the box
//!    still greys the bar. The client registers the composite and fans out inside it
//!    (its apply path calls both children's apply behavior); the observable
//!    behaviour — two preferences written, both restored by Defaults — is the same, and
//!    [`super::config::restore_default_values`]'s "27 rows plus the three paired volume sliders"
//!    count is the same 30 either way.
//! 3. **An unregistered preference keeps its current value on read.** The value read re-reads
//!    [`super::store`], which is an inquiry over the variables the preference registry bound — so
//!    showing the page and pressing Apply both re-snapshot from the store, as the player-option
//!    page's visibility-changed handler and the option page's save-current-values do. The one
//!    divergence is named on its value-read path: a read of an **unregistered** preference keeps
//!    the current value where retail's value read returns its zero-initialised local.
//!
//! # The labels
//!
//! The client writes six kinds of caption, and every one of them is a
//! `StringInfo(stringId, 0x10000003)` handed to the string-info setter (attribute
//! [`ATTR_STRING_INFO`]):
//!
//! | caption | written by | goes on |
//! |---|---|---|
//! | section header | header insertion | the template-0 row itself, `0x10000216` |
//! | check box | label write | the check box `0x10000219`, whose button also carries text |
//! | slider name | the slider option control's label write | the parent row's `0x1000021B` |
//! | menu name | the menu option control's label write | the parent row's `0x10000223` |
//! | slider ends | slider-label write | the parent row's `0x1000021E` and `0x1000021F` |
//! | tooltip | tooltip write | tooltip metadata, not a text child |
//!
//! The string id for a control's own caption is **not** a literal in the option build: it comes
//! from the preference query (name in; table, label and tooltip out) inside each preference-binding
//! path, and that registry is [`super::preferences`]. Only the six **section** ids and the twelve
//! **slider end** ids are literals in the option build, and those are `super::config::SECTIONS`
//! and `ConfigRow::slider_ends`.
//!
//! **A check+slider row's slider gets no name label, and that is the shipped data, not a miss.**
//! Template 5 (`0x10000220`) and template 7 (`0x10000221`) carry no `0x1000021B`, so
//! the recursive child lookup answers null and the row's caption is the
//! check box's. Six sliders on this page have end captions, not seven: the six `slider(...)` rows
//! of [`super::config::CONFIG_PAGE`], all of them wide; `Input.MouseLookSensitivity` is the one
//! narrow slider and has none.

use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};

use crate::element_types::ty;
use crate::panels::listbox::ListBoxWidget;
use crate::view::{PrefValue, UiRequest};

use super::config::{ConfigRow, Control, PrefValueConst, CONFIG_PAGE, SOUND_SLIDER_DEFAULT};

// -------------------------------------------------------------------------------------------
// The template list
// -------------------------------------------------------------------------------------------

/// The template index supplied by each option-row insertion helper.
///
/// These are the *measured* indices — see this module's docs for why `SLIDER_WIDE` is 6 and not
/// 3. [`super::pages::template`] is the same table.
pub mod template {
    /// The header insert.
    pub const HEADER: usize = 0;
    /// The separator insert.
    pub const SEPARATOR: usize = 1;
    /// Both toggle-option insert overloads.
    pub const TOGGLE: usize = 2;
    /// The narrow slider insert — `(-0 & 3) + 3`.
    pub const SLIDER_NARROW: usize = 3;
    /// The menu option insert.
    pub const MENU: usize = 4;
    /// The toggle with slider option insert.
    pub const TOGGLE_WITH_SLIDER: usize = 5;
    /// The wide slider insert — `(-1 & 3) + 3`.
    pub const SLIDER_WIDE: usize = 6;
}

/// The child id each helper looks the control up by, with a recursive child search.
pub mod child {
    use dereth_ui::ElementId;
    /// The toggle-option insert, and the client's first lookup.
    pub const CHECKBOX: ElementId = ElementId(0x1000_0219);
    /// The slider row's **name label**.
    pub const SLIDER_LABEL: ElementId = ElementId(0x1000_021B);
    /// The slider-option insert, and the post-init's second lookup.
    pub const SLIDER: ElementId = ElementId(0x1000_021C);
    /// The wide slider's two end captions.
    pub const SLIDER_END_LEFT: ElementId = ElementId(0x1000_021E);
    pub const SLIDER_END_RIGHT: ElementId = ElementId(0x1000_021F);
    /// The menu row's **name label**.
    pub const MENU_LABEL: ElementId = ElementId(0x1000_0223);
    /// The menu-option insert's recursive lookup of `0x10000224`. **Not** `0x10000223`, which is
    /// the menu's name label; the slider row has the same label/control pair and is as easy to
    /// swap.
    pub const MENU: ElementId = ElementId(0x1000_0224);
}

/// The scrollbar position attribute — normalised `0.0 ..= 1.0`.
pub const ATTR_POSITION: u32 = dereth_ui::widgets::scrollbar::attr::POSITION;
/// The button's checked attribute.
pub const ATTR_CHECKED: u32 = 0x0E;
/// The menu's selected-value attribute, read off the chosen **item**.
pub const ATTR_MENU_VALUE: u32 = 0x1000_0025;
/// The string-info attribute — the text element's attribute-set handler's
/// `0x17` arm, which resolves the `(string id, table)` pair through the string tables **at the
/// moment it is set**.
pub const ATTR_STRING_INFO: u32 = dereth_ui::props::attr::TEXT_STRING;

/// Build a string reference from a string id and a table, then set it on the target — the two
/// steps every caption on an option page is written with.
///
/// Returns the text that landed, or `None` when the target is not a text element or the string
/// did not resolve. **A `UiSystem` with no string resolver installed leaves the element's existing
/// text alone** (`resolve_string_info` answers `None`), so a headless caller gets `None` and can
/// report "no string table" rather than "no label" — a third state, distinct from both.
///
/// The original path validates the string info before setting it; a string id
/// of 0 is the case that fails, and it is refused here for the same reason.
pub fn set_string_info(
    ui: &mut UiSystem,
    h: ElemHandle,
    table: dereth_primitives::DataId,
    string_id: u32,
) -> Option<String> {
    if string_id == 0 {
        return None;
    }
    let si = dereth_assets::ui::StringInfo {
        override_flag: 0,
        literal: None,
        string_id: Some(string_id),
        table_id: Some(table),
        is_adder: 0,
        adder: None,
        variables: Vec::new(),
    };
    let v = dereth_assets::ui::PropertyValue::StringInfo(Box::new(si));
    if let Some(n) = ui.node_mut(h) {
        n.instance_properties.set(ATTR_STRING_INFO, v.clone());
    }
    ui.on_set_attribute(h, ATTR_STRING_INFO, Some(&v));
    let text = ui.text_element_mut(h)?.glyphs.inq_text(false);
    (!text.is_empty()).then_some(text)
}

/// The state puts the slider in when
/// its check box is unticked — `(-(checked != 0) & 0xFFFFFFF4) + 0x0D`, which is `0x0D` for
/// unchecked and `1` for checked.
pub const SLIDER_DISABLED_STATE: StateId = StateId(0x0D);
/// The other half of that expression.
pub const SLIDER_ENABLED_STATE: StateId = StateId(1);

/// `(-(checked != 0) & 0xFFFFFFF4) + 0x0D`, evaluated rather than tabulated — the one line in
/// the combined check-box-and-slider control's message-1 arm.
#[must_use]
pub fn gate_state(checked: bool) -> StateId {
    StateId((0u32.wrapping_sub(u32::from(checked)) & 0xFFFF_FFF4).wrapping_add(0x0D))
}

// -------------------------------------------------------------------------------------------
// Slider ranges
// -------------------------------------------------------------------------------------------

/// The client's preference-range table for the ten float
/// preferences this page puts a slider on.
///
/// The ranges are the client's preference-registration arguments (the UI range). They
/// are **not** guessable from the defaults and they are what turns a normalised `0x86` into a
/// preference value: `current = lower + frac · (upper - lower)`.
pub const SLIDER_RANGES: [(&str, (f32, f32)); 10] = [
    ("Sound.SoundVolume", (0.0, 1.0)),
    ("Sound.AmbientSoundVolume", (0.0, 1.0)),
    ("Sound.InterfaceSoundVolume", (0.0, 1.0)),
    ("Camera.Stiffness", (0.285_714_3, 1.0)),
    ("Camera.AdjustmentSpeed", (5.0, 80.0)),
    ("Render.FieldOfView", (10.0, 160.0)),
    ("Render.ScreenBrightness", (-1.0, 1.0)),
    ("Render.GraphicsPerformance", (-1.0, 1.0)),
    ("Render.DegradeDistance", (0.0, 100.0)),
    ("Input.MouseLookSensitivity", (0.01, 1.0)),
];

/// The range for one preference, or `(0.0, 1.0)` when it has none registered.
///
/// **This asks the registry rather than looking in [`SLIDER_RANGES`].** The registry is filled by
/// [`super::preferences`]. The table above is the **oracle**: the two were written independently,
/// and `preferences::tests::every_range_agrees_with_the_independently_transcribed_slider_table`
/// checks them against each other. A single table would be self-consistent and unfalsifiable.
///
/// The fallback is the client's: the preference binding leaves the lower and upper bounds at the
/// constructor's values when the preference range query ([`super::preferences::inq_preference_range`])
/// fails, and the slider option control's
/// constructor initialises them to 0.0 and 1.0. **So a host that never ran `init_ui_preferences`
/// gets 0.0..1.0 on every slider**, which is exactly what retail would do with an empty registry.
#[must_use]
pub fn slider_range(preference: &str) -> (f32, f32) {
    super::preferences::inq_preference_range(preference).unwrap_or((0.0, 1.0))
}

// -------------------------------------------------------------------------------------------
// One control
// -------------------------------------------------------------------------------------------

/// Which of the seven option-control types a live control is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionControl {
    /// Check box — attribute `0x0E`, element message 1.
    Checkbox,
    /// Slider — attribute `0x86`, element message `0x0A`.
    Slider,
    /// Menu — attribute `0x10000025` on the chosen item, element message 7.
    Menu,
}

/// One entry of the page's option array, with the option control's value fields.
#[derive(Debug, Clone, PartialEq)]
pub struct UiOption {
    pub control: OptionControl,
    /// The control element — `0x10000219`, `0x1000021C` or `0x10000224`.
    pub element: ElemHandle,
    /// The list-box row it sits in, which is the parent the label write reaches.
    pub row: ElemHandle,
    /// The preference name.
    pub preference: &'static str,
    /// The range bounds, meaningful for [`OptionControl::Slider`].
    pub lower: f32,
    pub upper: f32,
    /// The current value.
    pub current: PrefValue,
    /// The saved value.
    pub saved: PrefValue,
    /// The default, the fourth argument of the page's set-default call.
    pub default: PrefValue,
    /// The label string id the registry gave, or 0 when
    /// the preference query did not answer. Recorded so a headless run can tell *"the registry had no
    /// row"* from *"the row had no string table to resolve against"*; see [`Self::label`].
    pub label_token: u32,
    /// The tooltip string id the registry gave.
    pub tooltip_token: u32,
    /// What the string-info write actually put on the element: `Some(text)` when the
    /// string resolved, `None` when there is no string resolver installed or the row carries no
    /// caption element. **Three states, not two** — `label_token == 0` is "no registry row",
    /// `label_token != 0` with `label == None` is "no string table", and `Some` is a caption.
    pub label: Option<String>,
    /// The option asks for change confirmation. The client raises the dialog **after** the write has already
    /// landed; dialog and notice handling follows. [`PlayerOptionPage`] therefore
    /// schedules confirmation only after applying the option and restores `saved` on
    /// No/expiry.
    pub confirm_change: bool,
    /// The client's two arrays, paired: `(string id, 0x10000025)` per
    /// drop-down row, in the order the popup lists them.
    ///
    /// Empty for every control that is not a [`OptionControl::Menu`], and empty for the one menu
    /// whose list retail builds at run time — see `set_menu_entries`.
    pub entries: Vec<(u32, i32)>,
}

impl UiOption {
    /// The change-handler predicate is `saved != current`.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.saved != self.current
    }

    /// The normalised `0x86` this value corresponds to — the client's
    /// `(current - lower) / (upper - lower)`, with the same clamp into the range first.
    #[must_use]
    pub fn position(&self) -> f32 {
        let PrefValue::Float(mut v) = self.current else {
            return 0.0;
        };
        v = v.clamp(self.lower, self.upper);
        if (self.upper - self.lower).abs() < f32::EPSILON {
            return 0.0;
        }
        (v - self.lower) / (self.upper - self.lower)
    }

    /// `current = lower + frac · (upper - lower)` — the slider's own drag
    /// arithmetic, in one place so a test can state it without an element.
    #[must_use]
    pub fn value_at(&self, frac: f32) -> f32 {
        self.lower + frac * (self.upper - self.lower)
    }
}

// -------------------------------------------------------------------------------------------
// The page
// -------------------------------------------------------------------------------------------

/// The option-page state: its option box and ordered option array.
#[derive(Debug, Clone, Default)]
pub struct PlayerOptionPage {
    /// The page element itself (`0x10000213` for the client-options panel).
    pub page: Option<ElemHandle>,
    /// The option-box list widget.
    pub option_box: Option<ListBoxWidget>,
    /// The option array, in order.
    pub options: Vec<UiOption>,
    /// Combined check-box-and-slider pairing: `(index of the toggle, index of the slider)`.
    pub gated: Vec<(usize, usize)>,
    /// The state [`Self::sync_gates`] last asked each paired slider for, in [`Self::gated`] order.
    ///
    /// **Recorded because the shipped layout makes the greying invisible.** `0x1000021C` declares
    /// **no states at all** in `classic_options`, and answers a
    /// state its `ElementDesc` does not declare by recording **state 0** — so the bar's `n.state`
    /// is 0 whichever way the check box goes, in this build *and in retail*. That is a fact about
    /// the shipped data, not a defect to fix by inventing a state description; what can be
    /// asserted is the id the composite asked for, and this is where it is.
    pub gate_states: Vec<StateId>,
    /// How many header rows were built.
    pub headers: usize,
    /// How many separator rows were built.
    pub separators: usize,
    /// Add-from-template-list calls that produced no element. Counted for the reason
    /// `ListBoxWidget::create_failures` is: a page that silently builds nothing must show up as a
    /// number.
    pub failures: usize,
    /// How many section-header captions resolved to text. Six on the Client Options
    /// page, or 0 with no string resolver installed.
    pub header_captions: usize,
    /// How many wide-slider end captions resolved — twelve on the Client Options page, two per
    /// wide-slider label write.
    pub slider_end_captions: usize,
    /// **The number that moves.** How many rows
    /// were actually put inside the eight drop-downs, counted off the list boxes and not off this
    /// page's intentions.
    ///
    /// 32 on the Client Options page against the shipped dats, 0 with no asset environment (the
    /// popup cannot be built) and 0 with no preference registry (there are no choice lists to
    /// read). Those three cases are otherwise indistinguishable, which is why this is a count and
    /// not a bool: a regression reads as this number falling.
    pub menu_entries: usize,
    /// How many of the menus got a popup out of — the denominator for
    /// [`Self::menu_entries`]. Eight on the shipped tree; 0 without an asset source.
    pub menu_popups: usize,
    /// The menu option control's dialog and notices handling's delayed resolution confirmation.
    /// The client registers the changed menu for global message 3 and opens only after the second
    /// later tick; the context then remains on DialogController's non-queued list until Yes, No or
    /// its ten-second expiry.
    confirmation: Option<ConfirmationState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConfirmationState {
    option: usize,
    ticks: u8,
    context: Option<u64>,
}

const CONFIRM_CHANGE_TOKEN: &str = "ID_Option_ConfirmChange";
const CONFIRM_CHANGE_SECONDS: f32 = 10.0;

impl PlayerOptionPage {
    /// The client's first act — bind the option box (`0x10000200`).
    #[must_use]
    pub fn bind(ui: &UiSystem, page: ElemHandle) -> Self {
        let option_box = ui
            .get_child_recursive(page, super::config::OPTION_BOX)
            .map(|h| ListBoxWidget::bind(ui, h));
        Self {
            page: Some(page),
            option_box,
            ..Self::default()
        }
    }

    /// The rows currently in the option box.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.option_box.as_ref().map_or(0, |b| b.items.len())
    }

    fn add_row(&mut self, ui: &mut UiSystem, index: usize) -> Option<ElemHandle> {
        let b = self.option_box.as_mut()?;
        let h = b.add_from_template(ui, index, None);
        if h.is_none() {
            self.failures += 1;
        }
        h
    }

    /// Header insertion uses template 0, casts the row to a text element, then constructs
    /// `StringInfo(stringId, 0x10000003)`, validates it, and sets it on the row.
    /// This is the complete caption path.
    ///
    /// `token` is the `ID_*` name whose hash is the string id; the six
    /// this page passes are [`super::config::SECTIONS`], and they are the only caption ids that
    /// are literals in the option build rather than coming out of the preference registry.
    pub fn add_header(&mut self, ui: &mut UiSystem, token: &str) -> bool {
        let Some(row) = self.add_row(ui, template::HEADER) else {
            return false;
        };
        self.headers += 1;
        self.set_header_label(ui, row, token);
        true
    }

    /// The player-option page's separator insert — template 1.
    pub fn add_separator(&mut self, ui: &mut UiSystem) -> bool {
        let ok = self.add_row(ui, template::SEPARATOR).is_some();
        self.separators += usize::from(ok);
        ok
    }

    /// Toggle insertion uses template 2, then recursively finds `0x10000219`, verifies the
    /// check-box control, binds the UI preference, and registers the option.
    /// The returned index addresses the page's option array.
    pub fn add_toggle_option(
        &mut self,
        ui: &mut UiSystem,
        preference: &'static str,
        default: bool,
        confirm_change: bool,
    ) -> Option<usize> {
        let row = self.add_row(ui, template::TOGGLE)?;
        self.register_child(
            ui,
            row,
            child::CHECKBOX,
            ty::OPTION_CHECKBOX,
            |element, row| UiOption {
                control: OptionControl::Checkbox,
                element,
                row,
                preference,
                lower: 0.0,
                upper: 1.0,
                current: PrefValue::Bool(default),
                saved: PrefValue::Bool(default),
                default: PrefValue::Bool(default),
                label_token: 0,
                tooltip_token: 0,
                label: None,
                confirm_change,
                entries: Vec::new(),
            },
        )
    }

    /// The slider-option insert for a preference, wide or not — template
    /// `(-wide & 3) + 3`, then a recursive lookup of `0x1000021C`.
    pub fn add_slider_option(
        &mut self,
        ui: &mut UiSystem,
        preference: &'static str,
        wide: bool,
        default: f32,
    ) -> Option<usize> {
        let t = if wide {
            template::SLIDER_WIDE
        } else {
            template::SLIDER_NARROW
        };
        let row = self.add_row(ui, t)?;
        let (lower, upper) = slider_range(preference);
        self.register_child(ui, row, child::SLIDER, ty::OPTION_SLIDER, |element, row| {
            UiOption {
                control: OptionControl::Slider,
                element,
                row,
                preference,
                lower,
                upper,
                current: PrefValue::Float(default),
                saved: PrefValue::Float(default),
                default: PrefValue::Float(default),
                label_token: 0,
                tooltip_token: 0,
                label: None,
                confirm_change: false,
                entries: Vec::new(),
            }
        })
    }

    /// The menu-option insert `(preference name, is UI preference)` — template 4, then
    /// a recursive lookup of `0x10000224`.
    pub fn add_menu_option(
        &mut self,
        ui: &mut UiSystem,
        preference: &'static str,
        default: i32,
        confirm_change: bool,
    ) -> Option<usize> {
        let row = self.add_row(ui, template::MENU)?;
        let i = self.register_child(ui, row, child::MENU, ty::OPTION_MENU, |element, row| {
            UiOption {
                control: OptionControl::Menu,
                element,
                row,
                preference,
                lower: 0.0,
                upper: 1.0,
                current: PrefValue::Int(default),
                saved: PrefValue::Int(default),
                default: PrefValue::Int(default),
                label_token: 0,
                tooltip_token: 0,
                label: None,
                confirm_change,
                entries: Vec::new(),
            }
        })?;
        // The menu element's popup build is the menu element's initialisation's, not
        // `set_entries`'s, so **every** menu gets one — including the one whose choice list is
        // empty. A drop-down with a popup and no rows is exactly retail's state for
        // `Display.Resolution` before an adapter has been enumerated: the menu-opening path
        // refuses it when the item list is empty, and that is a different thing from a menu with
        // no popup at all, which is what a missing asset environment gives. [`Self::menu_popups`]
        // is the counter that keeps the two apart.
        self.bind_menu_popup(ui, self.options[i].element);
        Some(i)
    }

    /// The toggle-with-slider insert for a Boolean and a float preference — template 5,
    /// then the client's two recursive child lookups
    /// (`0x10000219` and `0x1000021C`) and the preference binding of both names.
    ///
    /// Registered here as two entries plus a [`Self::gated`] pair; see divergence 2 in the module
    /// docs.
    pub fn add_toggle_with_slider_option(
        &mut self,
        ui: &mut UiSystem,
        bool_pref: &'static str,
        bool_default: bool,
        float_pref: &'static str,
        float_default: f32,
    ) -> Option<(usize, usize)> {
        let row = self.add_row(ui, template::TOGGLE_WITH_SLIDER)?;
        let toggle = self.register_child(
            ui,
            row,
            child::CHECKBOX,
            ty::OPTION_CHECKBOX,
            |element, row| UiOption {
                control: OptionControl::Checkbox,
                element,
                row,
                preference: bool_pref,
                lower: 0.0,
                upper: 1.0,
                current: PrefValue::Bool(bool_default),
                saved: PrefValue::Bool(bool_default),
                default: PrefValue::Bool(bool_default),
                label_token: 0,
                tooltip_token: 0,
                label: None,
                confirm_change: false,
                entries: Vec::new(),
            },
        )?;
        let (lower, upper) = slider_range(float_pref);
        let slider =
            self.register_child(ui, row, child::SLIDER, ty::OPTION_SLIDER, |element, row| {
                UiOption {
                    control: OptionControl::Slider,
                    element,
                    row,
                    preference: float_pref,
                    lower,
                    upper,
                    current: PrefValue::Float(float_default),
                    saved: PrefValue::Float(float_default),
                    default: PrefValue::Float(float_default),
                    label_token: 0,
                    tooltip_token: 0,
                    label: None,
                    confirm_change: false,
                    entries: Vec::new(),
                }
            })?;
        self.gated.push((toggle, slider));
        Some((toggle, slider))
    }

    /// Recursive child lookup + runtime type check + option registration — the shape every helper
    /// above shares.
    ///
    /// The type check is the one each helper performs before touching the control, and
    /// **it is not a formality**: the slider-option insert bails out and registers nothing when the cast
    /// fails, which is exactly what a row template with the wrong child id would produce. Getting
    /// a `None` back here means the layout does not carry the control the helper names.
    fn register_child(
        &mut self,
        ui: &mut UiSystem,
        row: ElemHandle,
        id: ElementId,
        want: dereth_ui::ElementType,
        make: impl FnOnce(ElemHandle, ElemHandle) -> UiOption,
    ) -> Option<usize> {
        let h = ui.get_child_recursive(row, id)?;
        if ui.node(h)?.ty() != want {
            self.failures += 1;
            return None;
        }
        let idx = self.options.len();
        self.options.push(make(h, row));
        // Preference binding supplies the metadata half, which every helper performs before
        // registering the option; then the value is read, because in the client the control's
        // value is the store's from the moment it is bound.
        self.set_ui_preference(ui, idx);
        self.options[idx].current = self.get_value(idx);
        self.options[idx].saved = self.options[idx].current.clone();
        self.refresh(ui, idx);
        Some(idx)
    }

    // ---- the preference binding: the label, the tooltip and the range -------------------------

    /// Bind a slider, check box, or menu to its preference — the metadata half rather than the
    /// value read.
    ///
    ///
    /// All three do the same thing: record the preference name, then — only if the registry
    /// answers for that name with a table, a label id and a tooltip id — set the label and the
    /// tooltip from that table, and then read the slider's range or the menu's enum choices.
    ///
    /// **That condition is load-bearing.** With an empty registry the preference query answers
    /// `false` for all 30 controls and neither label, tooltip nor choices are set.
    ///
    /// [`UiOption::lower`]/[`UiOption::upper`] are already set from [`slider_range`] at
    /// construction, which is the same range query; the label is what this adds.
    fn set_ui_preference(&mut self, ui: &mut UiSystem, i: usize) {
        let Some(o) = self.options.get(i) else { return };
        let (element, row, control, preference) = (o.element, o.row, o.control, o.preference);
        let Some((table_enum, label, tooltip)) = super::preferences::inq_preference(preference)
        else {
            // This client's landscape options are not in the string tables: their caption and
            // their choices are literal text.
            if let Some(which) =
                dereth_client_contract::options::landscape::Landscape::of(preference)
            {
                self.set_literal_preference(ui, i, which.caption());
            }
            return;
        };
        debug_assert_eq!(table_enum, super::preferences::STRING_TABLE_ENUM);
        let table = super::preferences::table(ui);
        // Label placement depends on the control type. The check box is a button that also
        // carries text, so its
        // caption is attribute 0x17 on the control itself; the slider and the menu reach a sibling
        // text child through the control's parent.
        let target = match control {
            OptionControl::Checkbox => Some(element),
            OptionControl::Slider => ui.get_child_recursive(row, child::SLIDER_LABEL),
            OptionControl::Menu => ui.get_child_recursive(row, child::MENU_LABEL),
        };
        let text = target.and_then(|h| set_string_info(ui, h, table, label));
        if let Some(o) = self.options.get_mut(i) {
            o.label_token = label;
            o.tooltip_token = tooltip;
            o.label = text;
        }
        // Tooltip metadata does not live on a text child:
        // the tooltip is shown by the UI manager's hover timer against `Misc.TooltipDelay`.
        // The id is recorded above; the hover surface that would show it is `dereth_ui`'s
        // `dialog::tooltip`, and no option control carries a `StringInfo` tooltip in this build.
        //
        // Menu binding has one branch more than the other controls: filling the drop-down's rows.
        // Without it every drop-down is empty.
        //
        // **Which of the two legs.** The player-option page's menu-option insert branches on
        // whether the preference is a UI preference: true binds a UI preference
        // and false binds a user preference. The config panel's option build
        // passes `false` exactly once, for `Display.Resolution`
        // (`super::config::USER_PREFERENCE_MENUS`), and that leg reads the **run-time**
        // choices the device registered rather than the compiled-in enum-choice list. Routing it
        // through `set_menu_entries` would leave the resolution drop-down empty.
        if control == OptionControl::Menu {
            if super::config::USER_PREFERENCE_MENUS
                .iter()
                .any(|n| n.eq_ignore_ascii_case(preference))
            {
                self.set_user_preference_entries(ui, i);
            } else {
                self.set_menu_entries(ui, i);
            }
        }
    }

    /// Caption a menu row with literal text and fill it from the option store's choice rows: the
    /// binding for an option of this client's own, which has no string-table entries.
    fn set_literal_preference(&mut self, ui: &mut UiSystem, i: usize, caption: &str) {
        let Some(o) = self.options.get(i) else { return };
        if o.control != OptionControl::Menu {
            return;
        }
        let landed = ui
            .get_child_recursive(o.row, child::MENU_LABEL)
            .and_then(|h| ui.text_element_mut(h))
            .map(|t| {
                t.set_text(caption);
                caption.to_string()
            });
        if let Some(o) = self.options.get_mut(i) {
            o.label = landed;
        }
        self.set_user_preference_entries(ui, i);
    }

    /// Fill the drop-down from the client's UI-preference branch.
    ///
    /// When the preference has enum choices, the client takes its choice values — or, when there
    /// are none, the indices `0 .. n` — and sets the menu's entries from the two lists and the
    /// table enum.
    ///
    /// [`super::preferences::items`] **is** the enum-choices read, and [`super::store::ENUM_CHOICES`]
    /// holds the value arrays retail registers.
    ///
    /// **The values are not the indices for two of the eight**, and getting that wrong shows on
    /// screen as a plausible wrong setting rather than as a blank: the texture-detail scale runs
    /// backwards (the client = `[4, 3, 2, 1, 0]`, so `VeryHigh` is **0**) and the landscape draw
    /// distance runs `[3, 5, 8, 11, 15, 25]`. The other six take the choice-value inquiry's empty-array
    /// branch and get `0 .. n`.
    ///
    /// Returns how many rows landed. **0 is the right answer for `Display.Resolution`**, which is
    /// the config panel's one user-preference menu insertion
    /// and therefore takes the user-preference
    /// choice-string inquiry path instead: its list is the one the display-preference initialisation
    /// builds out of the adapter's enumerated modes. `init_ui_preferences` attaches no static enum
    /// choices for it, so an uninitialized adapter correctly answers 0 through the user-preference
    /// path.
    pub fn set_menu_entries(&mut self, ui: &mut UiSystem, i: usize) -> usize {
        let Some(o) = self.options.get(i) else {
            return 0;
        };
        if o.control != OptionControl::Menu {
            return 0;
        }
        let preference = o.preference;
        // The UI preference registry's enum choices read.
        let choices = super::preferences::items(preference);
        if choices.is_empty() {
            return 0;
        }
        // The preference store's choice-values read, and the client's own fall-back.
        let registered = super::store::inq_choice_values(preference).unwrap_or(&[]);
        let values: Vec<i32> = if registered.is_empty() {
            (0..choices.len())
                .map(|k| i32::try_from(k).unwrap_or(0))
                .collect()
        } else {
            registered.to_vec()
        };
        // `zip` is the guard retail does not have: its menu fill indexes `values[i]` for every
        // `choices[i]`, so a value array shorter than the choice list would over-read there. Here
        // it truncates instead, and the shortfall shows in the returned count.
        let entries: Vec<(u32, i32)> = choices.into_iter().zip(values).collect();
        self.set_entries(ui, i, &entries)
    }

    /// Bind and populate the **other** menu-option branch: a user preference rather than a UI
    /// preference.
    ///
    /// When the registry answers for the preference, the client sets the label and tooltip, then
    /// — only if both the choice values and the choice strings are available — sets the menu's
    /// literal entries from them.
    ///
    /// Three differences from [`Self::set_menu_entries`], and the first is the one that matters:
    ///
    /// 1. **The labels come from the user preferences, not from the UI preferences.** They are
    ///    the choices the owning subsystem registered with the preference — for
    ///    `Display.Resolution` the `"%ix%i"` strings that display-preference initialisation
    ///    builds from the adapter's enumerated modes at device init. There is no enum-choice
    ///    list and therefore no string id: the enum-choice inquiry answers empty, which is
    ///    why the preference-binding leg puts **no rows at all** in this drop-down.
    /// 2. **Each label is wrapped as a literal value**
    ///    (after wide-string conversion) instead of as a string-table id. The row text is literal,
    ///    so a missing string table cannot empty this list.
    /// 3. **The value array is required.** The client returns when there are no choice values; no
    ///    `0 .. n` fall-back, unlike the preference binding's. A preference registered as
    ///    anything but an unsigned 32-bit value therefore gets nothing here.
    ///
    /// The `0x10000025` attribute each row carries is the mode descriptor `width << 16 | height`,
    /// which is
    /// exactly what the menu selection reads back into the current value
    /// and what the control's apply writes to the preference.
    ///
    /// Returns how many rows landed — **0 before the device has enumerated anything**, which is
    /// the state retail has while its renderer is null.
    pub fn set_user_preference_entries(&mut self, ui: &mut UiSystem, i: usize) -> usize {
        let Some(o) = self.options.get(i) else {
            return 0;
        };
        if o.control != OptionControl::Menu {
            return 0;
        }
        // The choice-value inquiry, then the choice-string one, both behind the unsigned-32-bit
        // gate.
        let Some(rows) = super::store::choice_rows(o.preference) else {
            return 0;
        };
        let entries: Vec<(String, i32)> = rows.into_iter().map(|c| (c.label, c.value)).collect();
        self.set_literal_entries(ui, i, &entries)
    }

    /// Populate string-table-backed menu rows.
    ///
    /// For each choice, in order, the client inserts a text item whose caption is that choice's
    /// string id in the given table, and sets the item's int attribute `0x10000025` to the matching
    /// value.
    ///
    /// The menu pop-up build and the second half of its initialisation run first, lazily. The
    /// original client runs them during element initialisation, which has the element manager to
    /// hand, and here the asset source arrives through [`crate::env`]. **The menu-opening path
    /// returns early when the item list is empty**, so a menu with no rows does not open at
    /// all.
    ///
    /// The caption is resolved through the installed string resolver rather than handed over as a
    /// `StringInfo`, which is [`dereth_ui::widgets::menu::insert_text_item`]'s documented shape and
    /// the route `chat::mainchat` already takes. A row whose string does not resolve is still
    /// created, with no text: that is what the client does with a missing string, and it keeps the
    /// `0x10000025` values aligned with the choice list, which is what the refresh matches on.
    ///
    /// Returns how many rows landed.
    pub fn set_entries(&mut self, ui: &mut UiSystem, i: usize, entries: &[(u32, i32)]) -> usize {
        let table = super::preferences::table(ui);
        let texts: Vec<Option<String>> = entries
            .iter()
            .map(|(sid, _)| ui.resolve_string(table, *sid))
            .collect();
        let n = self.insert_menu_rows(ui, i, &texts, entries);
        if let Some(o) = self.options.get_mut(i) {
            o.entries = entries.iter().copied().take(n).collect();
        }
        n
    }

    /// Populate literal-text menu rows — [`Self::set_entries`]'s
    /// sibling, and the only difference between them is where the row's text comes from.
    ///
    /// For each choice, in order, the client inserts a text item whose caption is that choice's
    /// text as a literal, and sets the item's int attribute `0x10000025` to the matching value.
    ///
    /// The label is already text, so there is no `resolve_string` and no missing-string case: a
    /// row's caption is exactly the text the subsystem registered. `o.entries` records
    /// `(0, value)` for each row, because a literal has no string id — so the row count stays
    /// correct.
    ///
    /// Returns how many rows landed.
    pub fn set_literal_entries(
        &mut self,
        ui: &mut UiSystem,
        i: usize,
        entries: &[(String, i32)],
    ) -> usize {
        let texts: Vec<Option<String>> = entries.iter().map(|(t, _)| Some(t.clone())).collect();
        let values: Vec<(u32, i32)> = entries.iter().map(|(_, v)| (0u32, *v)).collect();
        let n = self.insert_menu_rows(ui, i, &texts, &values);
        if let Some(o) = self.options.get_mut(i) {
            o.entries = values.into_iter().take(n).collect();
        }
        n
    }

    /// The insertion loop [`Self::set_entries`] and [`Self::set_literal_entries`] share, with the
    /// caption already resolved.
    fn insert_menu_rows(
        &mut self,
        ui: &mut UiSystem,
        i: usize,
        texts: &[Option<String>],
        entries: &[(u32, i32)],
    ) -> usize {
        let Some(o) = self.options.get(i) else {
            return 0;
        };
        if o.control != OptionControl::Menu {
            return 0;
        }
        let menu = o.element;
        if !self.bind_menu_popup(ui, menu) {
            return 0;
        }
        // The menu flush. The client never fills a menu's entries twice on one
        // control; a rebuild that re-ran the option build over a live page would, and a drop-down
        // holding two copies of its list is worse than one holding none.
        dereth_ui::widgets::menu::flush(ui, menu);
        let made = ui.env().cloned().map(|e| {
            e.with_assets(|assets| {
                let mut out: Vec<(ElemHandle, i32)> = Vec::with_capacity(entries.len());
                for (k, ((_, value), text)) in entries.iter().zip(texts).enumerate() {
                    if let Some(h) = dereth_ui::widgets::menu::insert_text_item(
                        ui,
                        assets,
                        menu,
                        text.as_deref().unwrap_or(""),
                        k,
                    ) {
                        out.push((h, *value));
                    }
                }
                out
            })
        });
        let Some(made) = made else { return 0 };
        let n = made.len();
        for (h, value) in made {
            // Integer attribute `0x10000025` on each item carries its value.
            // The refresh path matches it against the current value, and it is the value
            // the client reads back out of the row the player picked.
            ui.set_attribute_int(h, ATTR_MENU_VALUE, value);
        }
        // **One more after the last row, and it is load-bearing.**
        // `insert_text_item` ends in `menu::layout_items`, which places the rows and *then*
        // resizes the popup to fit them; the resize re-lays
        // the popup's children out of their `ElementDesc`, which puts every row back at the
        // design position it shares with all the others. So after the final insert the rows are
        // correct in *size* and stacked on top of each other in *position*, and hit testing
        // then answers row 0 for a press anywhere in the list —
        // a drop-down that opens, draws its rows and always chooses the first one.
        //
        // The client does not have this problem because the layout update's dirty bit `0x200` is
        // consumed by the per-frame layout pass, which runs *after* the resize; this crate has no
        // such pass (see `update_layout`'s note), so the last pass is made here.
        // The second resize is a no-op because the popup is already the right size, which is what
        // stops this from being an infinite alternation.
        dereth_ui::widgets::menu::layout_items(ui, menu);
        self.menu_entries += n;
        n
    }

    /// Bind the lazily created popup, then run the client's popup initialization,
    /// once per menu.
    ///
    /// `false` is "no popup", which in this crate means either that no asset environment is
    /// installed or that the row template carries no popup root id. Both are the loud form of
    /// "there is nothing to put entries in", and neither can be told from an empty choice list
    /// without [`Self::menu_popups`].
    fn bind_menu_popup(&mut self, ui: &mut UiSystem, menu: ElemHandle) -> bool {
        if dereth_ui::widgets::menu::list_box_handle(ui, menu).is_some() {
            return true;
        }
        let made = ui.env().cloned().map(|e| {
            e.with_assets(|assets| dereth_ui::widgets::menu::make_popup(ui, assets, menu))
        });
        if made.flatten().is_none() {
            return false;
        }
        let ok = dereth_ui::widgets::menu::initialize_popup(ui, menu).is_some();
        self.menu_popups += usize::from(ok);
        ok
    }

    /// Set the two end
    /// captions on a **wide** slider row, `0x1000021E` and `0x1000021F` under the control's parent.
    ///
    /// Returns how many of the two landed. A narrow row (template 3) has neither child, so this is
    /// `0` there and that is the shipped layout rather than a failure — the option build
    /// calls it for exactly the six wide sliders.
    pub fn set_slider_label(
        &mut self,
        ui: &mut UiSystem,
        i: usize,
        left: &str,
        right: &str,
    ) -> usize {
        let Some(o) = self.options.get(i) else {
            return 0;
        };
        let row = o.row;
        let table = super::preferences::table(ui);
        let mut n = 0;
        for (id, token) in [
            (child::SLIDER_END_LEFT, left),
            (child::SLIDER_END_RIGHT, right),
        ] {
            let Some(h) = ui.get_child_recursive(row, id) else {
                continue;
            };
            let sid = dereth_primitives::num::hash::str_hash(token.as_bytes());
            n += usize::from(set_string_info(ui, h, table, sid).is_some());
        }
        self.slider_end_captions += n;
        n
    }

    /// Set a header caption. The template-0 row is itself a text element, so the
    /// `StringInfo` goes on the row.
    fn set_header_label(&mut self, ui: &mut UiSystem, row: ElemHandle, token: &str) -> bool {
        let table = super::preferences::table(ui);
        let sid = dereth_primitives::num::hash::str_hash(token.as_bytes());
        let ok = set_string_info(ui, row, table, sid).is_some();
        self.header_captions += usize::from(ok);
        ok
    }

    // ---- the four option-page fan-outs -------------------------------------------------------

    /// Push the current value onto the option element.
    ///
    /// The slider clamps into the range and writes
    /// `(current - lower) / (upper - lower)` to attribute `0x86`;
    /// the check box writes the current value to attribute `0x0E`;
    /// the menu selects the item whose `0x10000025` equals the current value,
    /// falling back to item 0.
    pub fn refresh(&mut self, ui: &mut UiSystem, i: usize) {
        let Some(o) = self.options.get(i) else { return };
        let (element, control) = (o.element, o.control);
        match control {
            OptionControl::Slider => {
                let pos = o.position();
                // Setting float attribute `0x86` reaches the scrollbar's attribute-set handler,
                // whose `0x86` arm lays the thumb out — but only while the bar's
                // behaviour is in its slot. `update_layout_of` is the same layout performed from
                // outside, for exactly this case.
                ui.set_attribute_float(element, ATTR_POSITION, pos);
                dereth_ui::widgets::scrollbar::update_layout_of(ui, element);
            }
            OptionControl::Checkbox => {
                let PrefValue::Bool(v) = o.current else {
                    return;
                };
                ui.set_attribute_bool(element, ATTR_CHECKED, v);
            }
            OptionControl::Menu => {
                // The menu option control's refresh, whole: select the first item whose int
                // attribute `0x10000025` equals the current value; failing that, select item 0.
                //
                // **A menu with no items still does nothing**, in the client and here: there is
                // nothing to match and no item 0. [`Self::set_entries`] is what gives it items.
                //
                // Selecting the item without a broadcast is what puts the current setting's text on the
                // closed drop-down's own face through the menu-selection path, so this arm is
                // also the whole of "the page shows my current settings" for the eight menus.
                let PrefValue::Int(want) = o.current else {
                    return;
                };
                let n = dereth_ui::widgets::menu::num_items(ui, element);
                let mut chosen = None;
                for k in 0..n {
                    let Some(item) = dereth_ui::widgets::menu::get_item(ui, element, k) else {
                        continue;
                    };
                    if crate::bind::attr_int(ui, item, ATTR_MENU_VALUE) == Some(want) {
                        chosen = Some(item);
                        break;
                    }
                }
                let item = chosen.or_else(|| dereth_ui::widgets::menu::get_item(ui, element, 0));
                if let Some(item) = item {
                    dereth_ui::widgets::menu::set_selected_item(ui, element, Some(item), false);
                }
            }
        }
        self.sync_gates(ui);
    }

    /// Apply the option value — the preference write, and the only thing on this page that
    /// changes anything outside it.
    ///
    /// The two preference writes land in the preference store's value write
    /// and then in the typed-variable write, which is a plain store straight through the pointer
    /// the sound manager's preference registration installed.
    ///
    /// It goes two places, and both are the client's one place seen from two sides:
    ///
    /// * [`super::store::set_value`] is the variable itself, which is what makes the next
    ///   [`Self::get_value`] see it — the client has nothing else, because the variable *is* the
    ///   subsystem's global;
    /// * [`UiRequest::SetPreference`] tells the subsystem, which the host routes to
    ///   `AudioSystem::set_preference`. It stands in for the address binding this crate cannot
    ///   have, and for the eleven registrations that pass a real change callback.
    ///
    /// A type mismatch is a silent no-op in the client's typed-variable write and is one here too; the request is
    /// still emitted, because it is the *notification* and not the store.
    pub fn apply(&self, requests_out: &mut crate::requests::Outbox, i: usize) {
        let Some(o) = self.options.get(i) else { return };
        super::store::set_value(o.preference, o.current.clone());
        requests_out.emit(UiRequest::SetPreference(o.preference, o.current.clone()));
    }

    /// The menu option's global-message timer for a change that requires confirmation.
    /// This implements the two-tick delay for the confirmation dialog.
    ///
    /// `UiSystem::use_time` broadcasts global message 3 before it asks the input pump for this
    /// frame's pointer work. Consequently a menu choice made later in that frame cannot count the
    /// already-broadcast tick: the next two calls here are exactly the client's two later edges.
    /// The preference was already written by [`Self::apply`].
    pub fn confirmation_tick(&mut self, ui: &mut UiSystem) -> Vec<ElemHandle> {
        let Some(state) = self.confirmation.as_mut() else {
            return Vec::new();
        };

        if let Some(context) = state.context {
            let answer = ui
                .dialogs
                .info(context)
                .and_then(|info| info.element)
                .and_then(|root| dereth_ui::dialog::types::dialog_element(ui, root))
                .and_then(|dialog| dialog.answer_property())
                .and_then(|(_, value)| {
                    if let dereth_assets::ui::PropertyValue::Bool(answer) = value {
                        Some(answer)
                    } else {
                        None
                    }
                });
            if let Some(answer) = answer {
                self.finish_confirmation(ui, answer);
                return Vec::new();
            }
            if ui.dialogs.tick(ui.now.0).contains(&context) {
                self.finish_confirmation(ui, false);
            } else {
                self.update_confirmation_prompt(ui, context);
            }
            return Vec::new();
        }

        state.ticks = state.ticks.saturating_add(1);
        if state.ticks < 2 {
            return Vec::new();
        }

        let mut data = dereth_ui::PropertyCollection::new();
        data.set(
            dereth_ui::props::attr::DIALOG_KIND,
            dereth_assets::ui::PropertyValue::Integer(
                dereth_ui::dialog::DialogKind::Confirmation.property(),
            ),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_QUEUE_ID,
            dereth_assets::ui::PropertyValue::Integer(1),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_MODAL,
            dereth_assets::ui::PropertyValue::Bool(true),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_TIMEOUT,
            dereth_assets::ui::PropertyValue::Float(CONFIRM_CHANGE_SECONDS),
        );
        #[allow(clippy::cast_possible_truncation)] // a whole number of seconds
        let prompt = confirmation_prompt(ui, i64::from(CONFIRM_CHANGE_SECONDS as i32));
        data.set(
            dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
            dereth_assets::ui::PropertyValue::String(prompt),
        );
        let Some(context) = ui.dialogs.make_dialog(data, ui.now.0) else {
            self.confirmation = None;
            return Vec::new();
        };
        if let Some(state) = self.confirmation.as_mut() {
            state.context = Some(context);
        }
        self.service_confirmation_element(ui)
    }

    fn update_confirmation_prompt(&self, ui: &mut UiSystem, context: u64) {
        #[allow(clippy::cast_possible_truncation)] // a countdown of a few seconds
        let Some((root, remaining)) = ui.dialogs.info(context).and_then(|info| {
            Some((
                info.element?,
                info.dialog.as_ref()?.remaining(ui.now.0)?.max(0.0) as i64,
            ))
        }) else {
            return;
        };
        let text = confirmation_prompt(ui, remaining);
        if let Some(prompt) = ui
            .get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)
            .and_then(|child| ui.text_element_mut(child))
        {
            prompt.set_text(&text);
        }
        dereth_ui::dialog::base::update_popup_size_and_position(ui, root);
    }

    fn service_confirmation_element(&self, ui: &mut UiSystem) -> Vec<ElemHandle> {
        let Some(context) = self.confirmation.and_then(|state| state.context) else {
            return Vec::new();
        };
        let Some(info) = ui.dialogs.info(context).cloned() else {
            return Vec::new();
        };
        if info.element.is_some() {
            return Vec::new();
        }
        let Ok(root) = ui.require_env().and_then(|e| {
            e.create_and_add_root_element(
                ui,
                super::keybinding::DIALOG_LAYOUT,
                info.kind.root_element_id(),
            )
        }) else {
            return Vec::new();
        };
        ui.set_attribute_bool(
            root,
            dereth_ui::props::attr::DIALOG_MODAL,
            info.data
                .get_bool(dereth_ui::props::attr::DIALOG_MODAL)
                .unwrap_or(false),
        );
        if let Some(dereth_assets::ui::PropertyValue::String(text)) =
            info.data.get(dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT)
        {
            if let Some(prompt) = ui
                .get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)
                .and_then(|child| ui.text_element_mut(child))
            {
                prompt.set_text(text);
            }
        }
        dereth_ui::dialog::types::set_dialog_data(ui, root, &info.data);
        dereth_ui::dialog::base::update_popup_size_and_position(ui, root);
        if ui.bind_dialog_element(context, root) {
            vec![root]
        } else {
            ui.remove_and_delete_root(root);
            Vec::new()
        }
    }

    fn finish_confirmation(&mut self, ui: &mut UiSystem, keep: bool) {
        let Some(state) = self.confirmation.take() else {
            return;
        };
        if !keep {
            if let Some(saved) = self
                .options
                .get(state.option)
                .map(|option| option.saved.clone())
            {
                self.options[state.option].current = saved;
                self.apply(&mut ui.requests, state.option);
                self.refresh(ui, state.option);
            }
        }
        if let Some(context) = state.context {
            if let Some(root) = ui.dialogs.close_dialog(context, ui.now.0) {
                ui.remove_and_delete_root(root);
            }
        }
    }

    /// **Read the preference back out of the store.** Slider, checkbox, and menu controls
    /// each use the value type associated with that control.
    ///
    /// Each is one preference-value inquiry in the control's own type, followed by
    /// the registry's value inquiry and then a read through the variable pointer. The type is the
    /// control's, not the value's:
    /// a check box asks for a bool and a slider for a float, and the registry's value inquiry
    /// answers `false` for the wrong value type rather than converting.
    ///
    /// **Named divergence.** Retail's value read returns its zero-initialised local when the read
    /// fails, so an unregistered preference reads as 0.0 / false / 0. This keeps the current value
    /// instead: a rebuild whose registry was never filled would otherwise silently zero every
    /// option on the page the first time it was shown, which is a much worse failure than the one
    /// it reproduces. The read can only fail for a preference `init_ui_preferences` does not attach,
    /// and every control on this page has one — asserted in
    /// `preferences::tests::the_registered_defaults_disagree_with_the_ui_defaults_in_exactly_three_places`,
    /// which panics on a page row with no attached UI preference.
    #[must_use]
    pub fn get_value(&self, i: usize) -> PrefValue {
        let Some(o) = self.options.get(i) else {
            return PrefValue::Int(0);
        };
        let want = match o.control {
            OptionControl::Checkbox => super::store::DataType::Bool,
            OptionControl::Slider => super::store::DataType::Float,
            OptionControl::Menu => super::store::DataType::UInt,
        };
        super::store::inq_value_as(o.preference, want).unwrap_or_else(|| o.current.clone())
    }

    /// Apply, and the page's **show** arm.
    ///
    /// Each option's save-current-value sets both the current and the saved value to the value
    /// read (the slider's save-current-value), so this **writes no preference and reads every one
    /// of them**. It is not `saved = current`: that would make the page's own snapshot the
    /// store.
    ///
    /// Returns how many options came back with a value different from what the page was showing —
    /// 0 on Apply immediately after a drag, non-zero when the store moved under a page that was
    /// not looking. The count is the observable half of the read: a `save_current_values` that
    /// silently did nothing and one that re-read 30 identical values are otherwise the same.
    pub fn save_current_values(&mut self) -> usize {
        let mut moved = 0;
        for i in 0..self.options.len() {
            let v = self.get_value(i);
            if v != self.options[i].current {
                moved += 1;
            }
            self.options[i].current = v.clone();
            self.options[i].saved = v;
        }
        moved
    }

    /// Cancel, and the hide path.
    ///
    /// The changed test, then restore-saved-value **on the ones that changed** — so an untouched
    /// control writes nothing, which is why cancelling out of the page does not rewrite every
    /// preference on it. Returns how many were reverted.
    pub fn restore_saved_values(&mut self, ui: &mut UiSystem) -> usize {
        let mut n = 0;
        for i in 0..self.options.len() {
            if !self.options[i].changed() {
                continue;
            }
            self.options[i].current = self.options[i].saved.clone();
            self.refresh(ui, i);
            self.apply(&mut ui.requests, i);
            n += 1;
        }
        n
    }

    /// The Defaults button and global message
    /// `0x0C`. Unconditional: every control, changed or not, this client's landscape rows
    /// included (back to World Default).
    pub fn restore_default_values(&mut self, ui: &mut UiSystem) -> usize {
        for i in 0..self.options.len() {
            self.options[i].current = self.options[i].default.clone();
            self.refresh(ui, i);
            self.apply(&mut ui.requests, i);
        }
        self.options.len()
    }

    /// True as soon as one option reports that it changed.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.options.iter().any(UiOption::changed)
    }

    /// The page's retail controls: every option but this client's own landscape rows.
    pub fn retail_options(&self) -> impl Iterator<Item = &UiOption> + '_ {
        self.options.iter().filter(|o| {
            dereth_client_contract::options::landscape::Landscape::of(o.preference).is_none()
        })
    }

    /// This client's own landscape rows ([`super::config::LANDSCAPE_ROWS`]).
    pub fn landscape_options(&self) -> impl Iterator<Item = &UiOption> + '_ {
        self.options.iter().filter(|o| {
            dereth_client_contract::options::landscape::Landscape::of(o.preference).is_some()
        })
    }

    /// The player-option page's visibility-changed handler: `save_current_values` on show,
    /// **`restore_saved_values` on hide** — which is what rolls an uncommitted change back when the
    /// player closes the page instead of pressing Apply.
    pub fn on_visibility_changed(&mut self, ui: &mut UiSystem, visible: bool) -> usize {
        if visible {
            // `save_current_values` re-reads the store into the current and saved values, so
            // the elements have to be pushed the new values or the page would show the old ones.
            // The client gets this for free: each option's save-current-value is followed by
            // the page's own refresh fan-out.
            let moved = self.save_current_values();
            for i in 0..self.options.len() {
                self.refresh(ui, i);
            }
            moved
        } else {
            self.restore_saved_values(ui)
        }
    }

    // ---- messages ----------------------------------------------------------------------------

    /// The option-control element-message arms, dispatched by which control raised the
    /// message.
    ///
    /// Returns the index of the option that took it. The three arms:
    ///
    /// - **`0x0A` on a slider** — read attribute `0x86`,
    ///   `current = lower + frac·(upper - lower)`, then apply.
    /// - **`1` on a check box** — read attribute `0x0E`,
    ///   apply; then greys or ungreys the paired
    ///   slider.
    ///
    /// **No refresh runs on the way back.** The client does not call it either: the element
    /// already holds the value the player just put there, and re-writing `0x86` from a rounded
    /// current value mid-drag would make the thumb stutter.
    ///
    /// - **`7` on a menu** — read the selected item's int attribute `0x10000025` into the current
    ///   value, then apply.
    ///
    /// [`Self::set_entries`] builds the rows through `make_popup`, `initialize_popup` and
    /// `insert_text_item`, so the whole path is live — a press on the menu opens the popup
    /// (which needs a nonempty item list), a press on a row becomes the
    /// list box's message 4, and the menu turns that into the client's
    /// message **7**, and this arm reads the chosen row's `0x10000025` back out.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::msg::ElementMessage,
    ) -> Option<usize> {
        use dereth_ui::msg::element::id;
        let i = self.options.iter().position(|o| o.element == m.source)?;
        let o = &self.options[i];
        match (o.control, m.id) {
            (OptionControl::Slider, id::SCROLL_POSITION) => {
                let frac = crate::bind::attr_float(ui, o.element, ATTR_POSITION).unwrap_or(0.0);
                let v = o.value_at(frac);
                self.options[i].current = PrefValue::Float(v);
            }
            (OptionControl::Checkbox, id::BUTTON_CLICKED) => {
                let v = crate::bind::attr_bool(ui, o.element, ATTR_CHECKED).unwrap_or(false);
                self.options[i].current = PrefValue::Bool(v);
                self.sync_gates(ui);
            }
            (OptionControl::Menu, id::MENU_CHOSEN) => {
                // The selected item, which is the list box's selected-item field. The message also
                // carries the row in `p2`; another client path reads it that way.
                // The option menu does not, so
                // this does not either. A `MENU_CHOSEN` with no selection is the client's
                // null-item guard and leaves the current value alone.
                let sel = dereth_ui::widgets::menu::selected_index(ui, o.element);
                let item = usize::try_from(sel)
                    .ok()
                    .and_then(|k| dereth_ui::widgets::menu::get_item(ui, o.element, k));
                let item = item?;
                // The client reads attribute `0x10000025` into a zero-initialised value, so a row
                // carrying no value applies 0 rather than keeping the old setting. That is the client's, and it cannot happen on a row
                // [`Self::set_entries`] made.
                let v = crate::bind::attr_int(ui, item, ATTR_MENU_VALUE).unwrap_or(0);
                self.options[i].current = PrefValue::Int(v);
            }
            _ => return None,
        }
        self.apply(&mut ui.requests, i);
        if self.options[i].confirm_change {
            self.confirmation = Some(ConfirmationState {
                option: i,
                ticks: 0,
                context: None,
            });
        }
        Some(i)
    }

    /// The checkbox slider option control's element-message handler's message-1 arm: set the
    /// slider's state to `(-(checked != 0) & 0xFFFFFFF4) + 0x0D`.
    fn sync_gates(&mut self, ui: &mut UiSystem) {
        self.gate_states
            .resize(self.gated.len(), SLIDER_ENABLED_STATE);
        for (k, (toggle, slider)) in self.gated.clone().into_iter().enumerate() {
            let Some(t) = self.options.get(toggle) else {
                continue;
            };
            let on = matches!(t.current, PrefValue::Bool(true));
            let Some(s) = self.options.get(slider) else {
                continue;
            };
            let h = s.element;
            let want = gate_state(on);
            self.gate_states[k] = want;
            ui.set_state(h, want);
        }
    }

    // ---- the page itself ---------------------------------------------------------------------

    /// The config panel's option build — the 27 rows of
    /// [`super::config::CONFIG_PAGE`], in page order, with a header before each section and a
    /// separator between sections.
    ///
    /// **The labels.** The section header takes its token straight from
    /// [`super::config::SECTIONS`]; each control's own caption comes out of the preference query
    /// inside [`Self::set_ui_preference`], against the registry
    /// [`super::preferences::init_ui_preferences`] fills; and each wide slider's two end captions
    /// come from [`super::config::ConfigRow::slider_ends`] through [`Self::set_slider_label`],
    /// which is the slider label write.
    ///
    /// A caption only *appears* if the host installed a string resolver — the ids resolve in
    /// `DataId(0x23000003)`, the `Preference` table. [`Self::header_captions`],
    /// [`Self::slider_end_captions`] and [`UiOption::label`] record what actually landed, so a
    /// headless caller can tell "no registry" from "no string table" from "captioned".
    pub fn init_options(&mut self, ui: &mut UiSystem) -> usize {
        let mut section: Option<&'static str> = None;
        for r in CONFIG_PAGE {
            if section != Some(r.section) {
                if let Some(done) = section {
                    self.add_closing_rows(ui, done);
                    self.add_separator(ui);
                }
                self.add_header(ui, r.section);
                section = Some(r.section);
            }
            self.add_config_row(ui, &r);
        }
        if let Some(done) = section {
            self.add_closing_rows(ui, done);
        }
        // Place the rows down the box.
        if let Some(b) = self.option_box.as_mut() {
            b.update_layout(ui);
        }
        self.options.len()
    }

    /// This client's own rows that close `section`, after its retail rows: the two landscape
    /// options close the Graphics section.
    fn add_closing_rows(&mut self, ui: &mut UiSystem, section: &'static str) {
        if section == super::config::LANDSCAPE_SECTION {
            for r in super::config::LANDSCAPE_ROWS {
                self.add_config_row(ui, &r);
            }
        }
    }

    fn add_config_row(&mut self, ui: &mut UiSystem, r: &ConfigRow) {
        match r.control {
            Control::Check => {
                let PrefValueConst::Bool(d) = r.ui_default else {
                    return;
                };
                self.add_toggle_option(ui, r.preference, d, r.confirm_change);
            }
            Control::Slider { wide } => {
                let PrefValueConst::Float(d) = r.ui_default else {
                    return;
                };
                let Some(i) = self.add_slider_option(ui, r.preference, wide, d) else {
                    return;
                };
                // Retail follows six of its seven slider-option inserts with the end-caption
                // write (`set_slider_label`, left and right tokens); the seventh,
                // `Input.MouseLookSensitivity`, is the narrow row and gets none.
                if let Some((left, right)) = r.slider_ends {
                    self.set_slider_label(ui, i, left, right);
                }
            }
            Control::Menu => {
                let PrefValueConst::Int(d) = r.ui_default else {
                    return;
                };
                self.add_menu_option(ui, r.preference, d, r.confirm_change);
            }
            Control::CheckSlider => {
                let PrefValueConst::Bool(d) = r.ui_default else {
                    return;
                };
                let Some(f) = r.slider_preference else { return };
                self.add_toggle_with_slider_option(ui, r.preference, d, f, SOUND_SLIDER_DEFAULT);
            }
        }
    }
}

fn confirmation_prompt(ui: &UiSystem, seconds: i64) -> String {
    let id = dereth_primitives::num::hash::str_hash(CONFIRM_CHANGE_TOKEN.as_bytes());
    ui.resolve_string_rendered(
        super::keybinding::string_table(ui, super::keybinding::STRING_TABLE_ENUM),
        id,
        &[seconds.to_string()],
    )
    .filter(|text| !text.is_empty())
    .unwrap_or_else(|| CONFIRM_CHANGE_TOKEN.to_owned())
}

/// Bind the option box, then run the option build.
///
/// `page` is the client-options element (`0x10000213`). Returns the built page, or `None` when the
/// option box is not under it — which is the loud form of "the layout changed", not a default.
#[must_use]
pub fn config_post_init(ui: &mut UiSystem, page: ElemHandle) -> Option<PlayerOptionPage> {
    let mut p = PlayerOptionPage::bind(ui, page);
    p.option_box.as_ref()?;
    p.init_options(ui);
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slider template index is the expression the client evaluates.
    #[test]
    fn the_slider_template_index_is_the_expression_the_client_evaluates() {
        let client = |wide: bool| ((0u32.wrapping_sub(u32::from(wide))) & 3) + 3;
        assert_eq!(
            client(true),
            6,
            "wide: -1 as u32 = 0xFFFFFFFF, & 3 = 3, + 3 = 6"
        );
        assert_eq!(client(false), 3, "narrow: 0 & 3 = 0, + 3 = 3");
        assert_eq!(template::SLIDER_WIDE, 6);
        assert_eq!(template::SLIDER_NARROW, 3);
        // …and the other five, as literals.
        assert_eq!(template::HEADER, 0);
        assert_eq!(template::SEPARATOR, 1);
        assert_eq!(template::TOGGLE, 2);
        assert_eq!(template::MENU, 4);
        assert_eq!(template::TOGGLE_WITH_SLIDER, 5);
    }

    /// Oracle: the child ids each option-row helper passes to recursive child lookup, read
    /// from the recovered behavior. Literals on purpose, per the rule above; `0x10000224` is the
    /// one this crate had wrong.
    #[test]
    fn the_helper_child_ids_are_the_ones_the_client_looks_up() {
        assert_eq!(
            child::CHECKBOX,
            ElementId(0x1000_0219),
            " the toggle option insert "
        );
        assert_eq!(
            child::SLIDER,
            ElementId(0x1000_021C),
            " the slider option insert "
        );
        assert_eq!(
            child::MENU,
            ElementId(0x1000_0224),
            " the menu option insert "
        );
        assert_eq!(
            child::SLIDER_LABEL,
            ElementId(0x1000_021B),
            " the label write "
        );
        assert_eq!(
            child::MENU_LABEL,
            ElementId(0x1000_0223),
            " the label write "
        );
        assert_eq!(
            child::SLIDER_END_LEFT,
            ElementId(0x1000_021E),
            " the slider label write "
        );
        assert_eq!(
            child::SLIDER_END_RIGHT,
            ElementId(0x1000_021F),
            " the slider label write "
        );
        // The attributes, likewise as literals.
        assert_eq!(ATTR_CHECKED, 0x0E);
        assert_eq!(ATTR_POSITION, 0x86);
        assert_eq!(ATTR_MENU_VALUE, 0x1000_0025);
        // `(-(checked != 0) & 0xFFFFFFF4) + 0x0D` — the ui option checkbox slider.
        let client = |on: bool| (0u32.wrapping_sub(u32::from(on)) & 0xFFFF_FFF4).wrapping_add(0x0D);
        assert_eq!(client(true), SLIDER_ENABLED_STATE.0);
        assert_eq!(client(false), SLIDER_DISABLED_STATE.0);
    }

    /// Oracle: the recovered preference registrations' "UI range" column, which is
    /// the client's UI preference initialisation's range arguments.
    ///
    /// Every slider on the page has a range here, and the three sound sliders' `(0.0, 1.0)` is the
    /// same pair `dereth_audio::VOLUME_SLIDER_RANGE` carries — asserted against it, so the two
    /// cannot drift apart.
    #[test]
    fn every_slider_on_the_page_has_the_registered_range() {
        // `slider_range` is the registry's range inquiry now, so the producer has to have run —
        // which is the point: without every slider is 0.0..1.0, and the two assertions at the end
        // of this test say so out loud.
        assert_eq!(
            slider_range("Camera.Stiffness"),
            (0.0, 1.0),
            "no registry, no range"
        );
        assert_eq!(super::super::preferences::init_ui_preferences(), 34);
        for r in CONFIG_PAGE {
            let names: Vec<&str> = match r.control {
                Control::Slider { .. } => vec![r.preference],
                Control::CheckSlider => r.slider_preference.into_iter().collect(),
                _ => vec![],
            };
            for n in names {
                assert!(
                    SLIDER_RANGES.iter().any(|(p, _)| *p == n),
                    "{n} is a slider with no registered range"
                );
            }
        }
        assert_eq!(slider_range("Sound.SoundVolume"), (0.0, 1.0));
        assert_eq!(slider_range("Camera.Stiffness"), (0.285_714_3, 1.0));
        assert_eq!(slider_range("Camera.AdjustmentSpeed"), (5.0, 80.0));
        assert_eq!(slider_range("Render.FieldOfView"), (10.0, 160.0));
        assert_eq!(slider_range("Render.ScreenBrightness"), (-1.0, 1.0));
        assert_eq!(slider_range("Render.GraphicsPerformance"), (-1.0, 1.0));
        assert_eq!(slider_range("Render.DegradeDistance"), (0.0, 100.0));
        assert_eq!(slider_range("Input.MouseLookSensitivity"), (0.01, 1.0));
        // An unregistered name falls back to the constructor's 0.0..1.0, not to a panic.
        assert_eq!(slider_range("Display.FullScreen"), (0.0, 1.0));
    }

    /// The drag arithmetic and its inverse, without an element: `current = lower + frac ·
    /// (upper - lower)` and `Refresh`'s `(current - lower) / (upper - lower)`.
    #[test]
    fn the_slider_maps_its_normalised_position_onto_the_registered_range_both_ways() {
        assert_eq!(super::super::preferences::init_ui_preferences(), 34);
        // A handle is only ever minted by the arena, so an empty system's root stands in for one;
        // nothing below this line touches the element.
        let root = UiSystem::new((800, 600)).root();
        let o = |pref: &'static str, v: f32| {
            let (lower, upper) = slider_range(pref);
            UiOption {
                control: OptionControl::Slider,
                element: root,
                row: root,
                preference: pref,
                lower,
                upper,
                current: PrefValue::Float(v),
                saved: PrefValue::Float(v),
                default: PrefValue::Float(v),
                label_token: 0,
                tooltip_token: 0,
                label: None,
                confirm_change: false,
                entries: Vec::new(),
            }
        };
        // A 0..1 preference: position and value are the same number, which is why the sound
        // sliders are the ones a value test can be written against most directly.
        let s = o("Sound.SoundVolume", 0.75);
        assert!((s.value_at(0.75) - 0.75).abs() < 1e-6);
        assert!((s.position() - 0.75).abs() < 1e-6);
        // A range that is not 0..1: half-way along `Camera.AdjustmentSpeed` is 42.5, not 0.5.
        let c = o("Camera.AdjustmentSpeed", 42.5);
        assert!((c.value_at(0.5) - 42.5).abs() < 1e-4);
        assert!((c.position() - 0.5).abs() < 1e-4);
        // …and the page's own default, 40.0, is *below* the middle of 5..80.
        let d = o("Camera.AdjustmentSpeed", 40.0);
        assert!(
            (d.position() - 0.466_666_7).abs() < 1e-4,
            "{}",
            d.position()
        );
        // `Refresh` clamps into the range before dividing.
        let lo = o("Render.ScreenBrightness", -5.0);
        assert!((lo.position() - 0.0).abs() < 1e-6);
        let hi = o("Render.ScreenBrightness", 5.0);
        assert!((hi.position() - 1.0).abs() < 1e-6);
    }
}
