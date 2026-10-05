//! `ChatOptionsPanel` (type `0x10000042`) — the Chat Options page's two sliders and five filter
//! controls.
//!
//! The shared options sheet supplies section captions, opacity properties and filter windows. The
//! client's masks live in [`crate::chat::interface::FILTER_GROUPS`]; the opacity properties
//! `0x10000080`/`0x10000081` are read by the chat window's fade. This module is what writes them:
//! the retail chat-option initialization, plus the option-registration fan-out the options-page
//! base gives every page. Without it the tab draws an empty list box.
//!
//! # The option build, in order
//!
//! ```text
//!   header ID_ChatOption_GeneralOptions_Section
//!   template 3 row, child 0x1000021C (type 0x10000037)   property 0x10000080 ; idle
//!   template 6 row, child 0x1000021C (type 0x10000037)   property 0x10000081 ; active
//!     slider ends ID_UI_Value_Transparent / ID_UI_Value_Opaque on the active slider
//!     link the idle and active sliders
//!   separator
//!   header ID_ChatOption_MainChatWindow_Section
//!   template 8 row (type 0x10000044)
//!     property 0x1000007F (chat property type), user data 8 ; the window id
//!     default 0xFBFFFFFF
//!     12 filter groups  (Combat first — NO Gameplay)
//!     create the children; register the option
//!   separator; header FloatyChatWindow1; filter option (window 2)
//!   …Window2, 3;  …Window3, 4
//!   header FloatyChatWindow4 then the window-5 block inline, WITH Gameplay
//!   separator
//! ```
//!
//! ## Which windows get the *Gameplay* group
//!
//! It is easy to read this as *"only the main window and floaty 4 get the extra **Gameplay**
//! group; the other three start at **Combat**."* **That is exactly backwards**, and both the
//! helper and the two inline blocks say so:
//!
//! The checkbox builder dispatches ids 2..8 through a switch; ids outside that range
//! go directly to the common Combat tail. Id 8 sets default mask `0xFBFFFFFF` and skips
//! Gameplay. Ids 2, 3, 4 and 5 set defaults `0x101C`, `0x40C00`, `0x80000` and `0x78000000`,
//! respectively, then add Gameplay with mask `0x83912021`. The common tail adds the Combat
//! group with mask `0x600040`, then adds the eleven remaining groups.
//!
//! and the option initialisation's own two inline blocks agree: the **main window** (user data 8)
//! goes straight from its default to the Combat group `0x600040`, while **floaty 4**
//! (user data 5) adds `0x83912021 Gameplay` first. So the main window shows **twelve**
//! boxes and every floaty shows **thirteen** —
//! 64 in all. See also [`crate::chat::interface::filter_groups_for`]. \[measured\]
//!
//! # Where a value comes from and where it goes
//!
//! | control | read | write |
//! |---|---|---|
//! | opacity slider | → the gameplay option `prop` | → the gameplay option `prop` — the **top-level** gameplay-option collection, no window index |
//! | filter box | → the chat-window option `(window id, 0x1000007F)` | → the chat-window option `(window id, prop)` |
//!
//! **Neither sends a datagram.** The client's tail is the option-changed step for
//! `(prop, windowId)`, whose whole body raises the gameplay-option-changed notice with
//! `(prop, windowId)` and, if the options are not already dirty, marks them dirty
//! and stamps the time.
//!
//! A **local notice** and the same 480-second deferred `0x01A1 CharacterOptions` flush
//! the player module already owns. There is no `0x0005` for a chat option:
//! The player-option-changed event is only reached from a `PlayerOption` change.
//!
//! The notice's receivers are the chat windows —
//! the chat interface's handler writes its text-type filter when
//! `propName == 0x1000007F` **and** `windowId` is its own window id, and
//! the floaty main chat panel's handler intercepts `0x10000080` /
//! `0x10000081` first and hands them to its default / active opacity setters **without** a
//! window-id test, which is why one pair of sliders moves all five windows.
//!
//! # The captions are dat, not tokens
//!
//! The slider option control's gameplay option property write calls
//! the ui option's gameplay option name and tooltip read, which walks
//! enum `0x15` in group 2 = property collection `0x78000000`
//! (`GameplayOptionList`) for the `0xD3` struct whose `0xD6` equals the property, and takes its
//! `0xD4` (label) and `0xD5` (tooltip) `StringInfo`s whole — table `0x2300000D`, the *Options*
//! table. The default read does the same for enum `0x16` =
//! `0x78000001` (`GameplayOptionDefaults`) and makes the value the option's default.
//!
//! Measured off the shipped dats:
//!
//! | property | label | tooltip | max `0xD7` | min `0xD8` | default |
//! |---|---|---|---:|---:|---:|
//! | `0x10000080` | `0x0BB249E9` | `0x0E688803` | 1.0 | 0.0 | **0.5** |
//! | `0x10000081` | `0x0CE72E99` | `0x053DE883` | 1.0 | 0.0 | **1.0** |
//! | `0x1000007F` | — | — | — | — | `0x17BFFFFFF` |
//!
//! `0xD7`/`0xD8` are **not read** by the gameplay option property write — it never sets a
//! range — so the slider keeps the `[0.0, 1.0]` its constructor set (lower 0, upper 1.0). They happen to be the
//! same numbers, and this module reads the ctor's pair rather than the dat's so that a dat change
//! cannot silently move a control the client would not have moved.
//!
//! The thirteen filter captions are **not** from that table: the filter-group add writes both its
//! ids with table enum `0x10000006`, a different table enum from the
//! `0x10000003` every other option page uses.

use dereth_primitives::DataId;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::panels::listbox::ListBoxWidget;
use crate::view::{GameView, UiRequest};

// -------------------------------------------------------------------------------------------
// Ids, measured off the shipped `0x21000005` tree
// -------------------------------------------------------------------------------------------

/// The `ChatOptionsPanel` element. The one element of type `0x10000042` in the shipped tree; its
/// tab is `0x1000050B` on the options panel `0x1000018D` (panel id 10). \[measured\]
pub const CHAT_PAGE_ELEMENT: ElementId = ElementId(0x1000_050C);
/// The chat options panel's post-init's one child — a recursive child lookup of `0x1000050D`,
/// checked as type 5. **Not** `ClientOptionsPanel`'s `0x10000200` nor `CharacterSettingsPanel`'s
/// `0x100001FA`: every option page names its own list box.
pub const OPTION_BOX: ElementId = ElementId(0x1000_050D);

/// The **ninth** template of `0x1000050D`'s template list — `0x10000520`, a
/// `WideBitfieldCheckboxOption`, whose own one-entry template list is `0x10000521`.
///
/// `PlayerOptionPage`'s eight templates ([`super::page::template`]) stop at 7; this page's list
/// box carries a ninth that no `PlayerOptionPage` helper names, which is why
/// the chat page's filter-option add inserts template 8 by hand.
/// [measured against the shipped layout `0x2100002B`]
pub const TEMPLATE_CHECKBOX_BITFIELD64: usize = 8;

/// The bitfield option's child-construction loop finds each checkbox
/// inside its newly created `0x10000521` row: a recursive child lookup of `0x10000219` checked as
/// type 1 — a plain `Button`, **not** a `CheckboxOption`.
pub const FILTER_CHECKBOX: ElementId = ElementId(0x1000_0219);

/// Integer attribute `0x10000084` on each child — the bit index
/// the client reads back off the pressed element.
pub const ATTR_BIT_INDEX: u32 = 0x1000_0084;
/// The list box's DataID attribute `0x10000082` — the picture for a group
/// whose mask is **fully** set.
pub const ATTR_IMAGE_ALL: u32 = 0x1000_0082;
/// …and `0x10000083`, the picture for a group only **partly** set.
pub const ATTR_IMAGE_SOME: u32 = 0x1000_0083;

/// The string table enum the filter-group add writes both of its
/// ids in. **Not** the `0x10000003` the rest of the option pages use.
pub const FILTER_STRING_TABLE_ENUM: u32 = 0x1000_0006;

/// Enum `0x15` in group 2 — `GameplayOptionList`, the label/tooltip/min/max
/// table the gameplay option property read walks.
pub const GAMEPLAY_OPTION_LIST_ENUM: u32 = 0x15;
/// Enum `0x16` in group 2 — `GameplayOptionDefaults`.
pub const GAMEPLAY_OPTION_DEFAULTS_ENUM: u32 = 0x16;
/// The `DidMapper` group both lookups pass.
pub const GAMEPLAY_OPTION_GROUP: u32 = 2;
/// What [`GAMEPLAY_OPTION_LIST_ENUM`] resolves to against the shipped dats.
pub const GAMEPLAY_OPTION_LIST: DataId = DataId(0x7800_0000);
/// What [`GAMEPLAY_OPTION_DEFAULTS_ENUM`] resolves to against the shipped dats.
pub const GAMEPLAY_OPTION_DEFAULTS: DataId = DataId(0x7800_0001);

/// `GameplayOptionList`'s array property — `0xD2`.
pub const GAMEPLAY_OPTION_ARRAY: u32 = 0xD2;
/// Each row's label `StringInfo`.
pub const GAMEPLAY_OPTION_LABEL: u32 = 0xD4;
/// Each row's tooltip `StringInfo`.
pub const GAMEPLAY_OPTION_TOOLTIP: u32 = 0xD5;
/// Each row's option id.
pub const GAMEPLAY_OPTION_ID: u32 = 0xD6;

/// `SliderOption`'s constructor range — its lower and upper values,
/// which the gameplay option property write never overwrites.
pub const OPACITY_RANGE: (f32, f32) = (0.0, 1.0);

/// The two end captions the page puts on the **active** slider, in string table
/// enum `0x10000003`. The idle slider is template 3 and carries neither child.
pub const OPACITY_SLIDER_ENDS: (&str, &str) = ("ID_UI_Value_Transparent", "ID_UI_Value_Opaque");

/// `_Desc` — the suffix every filter group's tooltip token has over its label token.
pub const FILTER_TOOLTIP_SUFFIX: &str = "_Desc";

// -------------------------------------------------------------------------------------------
// The gameplay-option dats
// -------------------------------------------------------------------------------------------

/// The ui option's gameplay option name and tooltip read — `(label, tooltip)` as
/// `(table, string id)` pairs, out of `GameplayOptionList`.
///
/// `None` is "no environment installed" or "no row for that property", which is the same `false`
/// the client answers with and which leaves the control uncaptioned rather than mis-captioned.
#[must_use]
pub fn inq_gameplay_option_name_and_tooltip(
    ui: &UiSystem,
    property: u32,
) -> Option<((DataId, u32), (DataId, u32))> {
    let row = gameplay_option_row(ui, property)?;
    let si = |id: u32| match row.iter().find(|(k, _)| *k == id).map(|(_, p)| &p.value) {
        Some(dereth_assets::ui::PropertyValue::StringInfo(s)) => Some((s.table_id?, s.string_id?)),
        _ => None,
    };
    Some((si(GAMEPLAY_OPTION_LABEL)?, si(GAMEPLAY_OPTION_TOOLTIP)?))
}

/// The ui option's default gameplay option property read, for a `Float` property —
/// `GameplayOptionDefaults`' own value, which is what becomes the option's default.
#[must_use]
pub fn inq_default_gameplay_option_float(ui: &UiSystem, property: u32) -> Option<f32> {
    match read_db_properties(ui, GAMEPLAY_OPTION_DEFAULTS_ENUM, GAMEPLAY_OPTION_DEFAULTS)?
        .into_iter()
        .find(|(k, _)| *k == property)
        .map(|(_, p)| p.value)
    {
        Some(dereth_assets::ui::PropertyValue::Float(v)) => Some(v),
        _ => None,
    }
}

fn gameplay_option_row(
    ui: &UiSystem,
    property: u32,
) -> Option<Vec<(u32, dereth_assets::ui::BaseProperty)>> {
    let props = read_db_properties(ui, GAMEPLAY_OPTION_LIST_ENUM, GAMEPLAY_OPTION_LIST)?;
    let (_, array) = props
        .into_iter()
        .find(|(k, _)| *k == GAMEPLAY_OPTION_ARRAY)?;
    let dereth_assets::ui::PropertyValue::Array(rows) = array.value else {
        return None;
    };
    for row in rows {
        let dereth_assets::ui::PropertyValue::Struct(members) = row.value else {
            continue;
        };
        let matches = members.iter().any(|(k, p)| {
            *k == GAMEPLAY_OPTION_ID
                && matches!(p.value, dereth_assets::ui::PropertyValue::Enum(v) if v == property)
        });
        if matches {
            return Some(members);
        }
    }
    None
}

/// Load the property collection that enum `value` in group 2 resolves to.
fn read_db_properties(
    ui: &UiSystem,
    enum_value: u32,
    fallback: DataId,
) -> Option<Vec<(u32, dereth_assets::ui::BaseProperty)>> {
    let did = ui
        .env()
        .cloned()
        .and_then(|e| e.did_by_enum(GAMEPLAY_OPTION_GROUP, enum_value))
        .unwrap_or(fallback);
    let bytes = ui
        .env()
        .cloned()
        .map(|e| e.with_assets(|a| a.read(did)))?
        .ok()?;
    dereth_assets::ui::PropertyAsset::decode_payload(did, &bytes, &ui.property_types)
        .ok()
        .map(|c| c.properties)
}

// -------------------------------------------------------------------------------------------
// The controls
// -------------------------------------------------------------------------------------------

/// One of the two general-section sliders — `SliderOption` bound to a gameplay-option property
/// rather than to a `UserPreferences` name.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatOpacityOption {
    /// The property name — `0x10000080` (idle) or `0x10000081` (focused / moused-over).
    pub property: u32,
    /// The `Scrollbar` `0x1000021C` this control *is*.
    pub element: ElemHandle,
    /// The list-box row it sits in, which is the parent the label write reaches.
    pub row: ElemHandle,
    /// The lower and upper values — the constructor's, see [`OPACITY_RANGE`].
    pub lower: f32,
    pub upper: f32,
    /// The current, saved and default values.
    pub current: f32,
    pub saved: f32,
    pub default: f32,
    /// What the label write was handed, as `(table, string id)`, or `None` when
    /// [`inq_gameplay_option_name_and_tooltip`] answered nothing — three states, as elsewhere on these
    /// pages.
    pub label_source: Option<(DataId, u32)>,
    pub tooltip_source: Option<(DataId, u32)>,
    /// The text that actually landed on `0x1000021B`.
    pub label: Option<String>,
}

impl ChatOpacityOption {
    /// The slider option control's change handler — saved differs from current.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.saved.to_bits() != self.current.to_bits()
    }

    /// The client's arithmetic — clamp into the range, then
    /// `(current - lower) / (upper - lower)`.
    #[must_use]
    pub fn position(&self) -> f32 {
        let v = self.current.clamp(self.lower, self.upper);
        if (self.upper - self.lower).abs() < f32::EPSILON {
            return 0.0;
        }
        (v - self.lower) / (self.upper - self.lower)
    }

    /// `lower + frac · (upper - lower)`.
    #[must_use]
    pub fn value_at(&self, frac: f32) -> f32 {
        self.lower + frac * (self.upper - self.lower)
    }
}

/// One check box of a `WideBitfieldCheckboxOption` — one `(mask, label, tooltip)` filter group
/// plus the element child creation made for it.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatFilterChild {
    /// The group's 64-bit mask.
    pub mask: u64,
    /// The `ID_ChatOption_TextFilter_*` token whose hash is the label string id.
    pub label_token: &'static str,
    /// …and `<token>_Desc`, the tooltip.
    pub tooltip_token: String,
    /// The `0x10000219` `Button` inside the `0x10000521` row, or `None` when the row
    /// could not be made.
    pub element: Option<ElemHandle>,
    /// The `0x10000521` row itself.
    pub row: Option<ElemHandle>,
    /// The caption that landed, or `None` with no string resolver installed.
    pub label: Option<String>,
    /// What the refresh last set as the first child's state-6 image:
    /// [`ATTR_IMAGE_ALL`]'s DataID when every bit of the mask is set, [`ATTR_IMAGE_SOME`]'s when
    /// only some are, `None` when the group is off (the client does not touch the picture then).
    /// The id is retained for model inspection as well as written into state 6 on the button's
    /// first child, which is the image the shipped checkbox actually submits to the draw list.
    pub image: Option<DataId>,
}

/// One `WideBitfieldCheckboxOption` — a whole window's text-type filter.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatFilterOption {
    /// The control's user data, which for this control is the chat window id.
    pub window_id: u32,
    /// The property name — always [`CHAT_FILTER_PROPERTY`].
    pub property: u32,
    /// The control element — template 8's row **is** the `WideBitfieldCheckboxOption`.
    pub element: ElemHandle,
    /// The current, saved and default values.
    pub current: u64,
    pub saved: u64,
    pub default: u64,
    /// The filter groups, in the order they were added.
    pub children: Vec<ChatFilterChild>,
}

impl ChatFilterOption {
    /// The change handler.
    #[must_use]
    pub const fn changed(&self) -> bool {
        self.saved != self.current
    }

    /// Set or clear `mask` in the current value.
    pub fn set_bits(&mut self, mask: u64, on: bool) {
        if on {
            self.current |= mask;
        } else {
            self.current &= !mask;
        }
    }

    /// The child index whose mask is fully covered / partly covered by the current value — the
    /// pair the refresh computes per row.
    #[must_use]
    pub fn child_state(&self, i: usize) -> (bool, bool) {
        let Some(c) = self.children.get(i) else {
            return (false, false);
        };
        let hit = self.current & c.mask;
        (hit != 0, hit == c.mask)
    }
}

/// `CHAT_FILTER_PROPERTY` re-exported at the place the page uses it.
pub use super::pages::CHAT_FILTER_PROPERTY;

/// One entry of the page's option array, in order.
///
/// The order matters and is not cosmetic: save-current, restore-saved and
/// restore-defaults fan out over this array, and the two sliders are registered **before**
/// the five filters.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatOption {
    Opacity(ChatOpacityOption),
    Filter(ChatFilterOption),
}

impl ChatOption {
    #[must_use]
    pub fn changed(&self) -> bool {
        match self {
            Self::Opacity(o) => o.changed(),
            Self::Filter(f) => f.changed(),
        }
    }
}

/// What one accepted gesture on this page did, for the screen that owns the chat windows to turn
/// into the gameplay-option-changed notice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChatOptionEffect {
    /// The gameplay option `property` — the top-level float, all five windows.
    Opacity { property: u32, value: f32 },
    /// The chat-window option `(window, 0x1000007F)`.
    Filter { window_id: u32, mask: u64 },
}

// -------------------------------------------------------------------------------------------
// The page
// -------------------------------------------------------------------------------------------

/// `ChatOptionsPanel` — the built page.
#[derive(Debug, Clone, Default)]
pub struct ChatOptionsPage {
    /// The page element (`0x1000050C`).
    pub page: Option<ElemHandle>,
    /// The option box — `0x1000050D`.
    pub option_box: Option<ListBoxWidget>,
    /// The page's option array.
    pub options: Vec<ChatOption>,
    /// The slider links — `(left, right)` indices into [`Self::options`], in the client's own
    /// argument order (idle, active).
    pub slider_links: Vec<(usize, usize)>,
    /// Header insertions that produced a row — six.
    pub headers: usize,
    /// Separator insertions that produced a row — six (there is a trailing one).
    pub separators: usize,
    /// How many section headers resolved to text.
    pub header_captions: usize,
    /// How many of the two slider end captions landed — two, both on the active slider.
    pub slider_end_captions: usize,
    /// How many of the 64 filter check boxes got a caption out of the string table.
    pub child_captions: usize,
    /// Template-row insertions, child lookups and their runtime type checks that produced
    /// nothing. A page that silently builds nothing must be visible as a number.
    pub failures: usize,
    /// How many controls got a value out of the [`GameView`] — the denominator that separates
    /// *"the player has everything off"* from *"nothing answered"*.
    pub values_seen: usize,
}

impl ChatOptionsPage {
    /// The chat options panel's post-init — a recursive child lookup of `0x1000050D`, checked as
    /// type 5.
    #[must_use]
    pub fn bind(ui: &UiSystem, page: ElemHandle) -> Self {
        let option_box = ui
            .get_child_recursive(page, OPTION_BOX)
            .map(|h| ListBoxWidget::bind(ui, h));
        Self {
            page: Some(page),
            option_box,
            ..Self::default()
        }
    }

    /// The place in the option box just under the windows' opacity sliders.
    #[must_use]
    pub fn after_opacity(&self) -> Option<usize> {
        let b = self.option_box.as_ref()?;
        self.options
            .iter()
            .filter_map(|o| match o {
                ChatOption::Opacity(o) => b.index_of(o.row),
                ChatOption::Filter(_) => None,
            })
            .max()
            .map(|i| i + 1)
    }

    /// The rows currently in the option box — headers, separators and controls together.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.option_box.as_ref().map_or(0, |b| b.items.len())
    }

    /// How many check boxes the five filter controls hold in all. **64** on the shipped data:
    /// twelve on the main window and thirteen on each of the four floaties.
    #[must_use]
    pub fn filter_child_count(&self) -> usize {
        self.options
            .iter()
            .filter_map(|o| match o {
                ChatOption::Filter(f) => Some(f.children.len()),
                ChatOption::Opacity(_) => None,
            })
            .sum()
    }

    /// The chat options panel's option build, in order. Returns how many controls were
    /// registered — **seven**.
    pub fn init_options(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        use crate::chat::interface::opacity_attr;
        use dereth_client_contract::options::interface::Interface;
        use dereth_client_contract::options::sheet::{self, PageId, Value};
        let mut idle = None;
        let mut active = None;
        for (heading, rows) in sheet::headings_for(PageId::Chat, Interface::Modern) {
            self.add_header(ui, heading.text);
            let mut windows = Vec::new();
            for row in rows {
                match row.value {
                    Value::Opacity(property) => {
                        let wide = property == opacity_attr::ACTIVE;
                        let control = self.add_slider_option(ui, property, wide, view);
                        if wide {
                            active = control;
                            if let Some(i) = control {
                                let (left, right) = OPACITY_SLIDER_ENDS;
                                self.set_slider_label(ui, i, left, right);
                            }
                        } else if property == opacity_attr::DEFAULT {
                            idle = control;
                        }
                    }
                    Value::Filter { window, .. } if !windows.contains(&window) => {
                        // The sheet lists individual masks; this control edits the whole window.
                        windows.push(window);
                        self.add_checkbox_bitfield64_option(ui, window, CHAT_FILTER_PROPERTY, view);
                    }
                    _ => {}
                }
            }
            self.add_separator(ui);
        }
        if let (Some(a), Some(b)) = (idle, active) {
            self.slider_links.push((a, b));
        }

        if let Some(b) = self.option_box.as_mut() {
            b.update_layout(ui);
        }
        self.options.len()
    }

    fn add_row(&mut self, ui: &mut UiSystem, index: usize) -> Option<ElemHandle> {
        let b = self.option_box.as_mut()?;
        let h = b.add_from_template(ui, index, None);
        if h.is_none() {
            self.failures += 1;
        }
        h
    }

    /// Add a section caption resolved from the shared sheet, with its literal fallback.
    pub fn add_header(
        &mut self,
        ui: &mut UiSystem,
        text: dereth_client_contract::options::sheet::Text,
    ) -> bool {
        let Some(row) = self.add_row(ui, super::page::template::HEADER) else {
            return false;
        };
        self.headers += 1;
        let caption = super::config::resolve_text(ui, text);
        super::page::set_literal_text(ui, row, &caption);
        self.header_captions += 1;
        true
    }

    /// The player-option page's separator insert — template 1.
    pub fn add_separator(&mut self, ui: &mut UiSystem) -> bool {
        let ok = self.add_row(ui, super::page::template::SEPARATOR).is_some();
        self.separators += usize::from(ok);
        ok
    }

    /// Add a slider option with an empty preference name, followed by the gameplay option
    /// property write for `property`.
    ///
    /// The template is 3 narrow, 6 wide; the control is the recursive child `0x1000021C`, and the
    /// runtime type check against `0x10000037` is reproduced because the client registers nothing
    /// when it fails.
    pub fn add_slider_option(
        &mut self,
        ui: &mut UiSystem,
        property: u32,
        wide: bool,
        view: &dyn GameView,
    ) -> Option<usize> {
        let t = if wide {
            super::page::template::SLIDER_WIDE
        } else {
            super::page::template::SLIDER_NARROW
        };
        let row = self.add_row(ui, t)?;
        let Some(element) = ui.get_child_recursive(row, super::page::child::SLIDER) else {
            self.failures += 1;
            return None;
        };
        if ui.node(element).map(dereth_ui::ElementNode::ty)
            != Some(crate::element_types::ty::OPTION_SLIDER)
        {
            self.failures += 1;
            return None;
        }
        // The gameplay option property write's three steps: the property, the caption pair, the
        // default.
        let names = inq_gameplay_option_name_and_tooltip(ui, property);
        let default = inq_default_gameplay_option_float(ui, property).unwrap_or(0.0);
        // The gameplay option for the property, falling back to the default — the client's value
        // read preloads its result from the default rather than from zero.
        let current = view.gameplay_option_float(property).unwrap_or(default);
        self.values_seen += usize::from(view.gameplay_option_float(property).is_some());
        let (lower, upper) = OPACITY_RANGE;
        let mut o = ChatOpacityOption {
            property,
            element,
            row,
            lower,
            upper,
            current,
            saved: current,
            default,
            label_source: names.map(|(l, _)| l),
            tooltip_source: names.map(|(_, t)| t),
            label: None,
        };
        // The slider option control's label write — the parent row's `0x1000021B`, a `TextElement`.
        if let (Some((table, sid)), Some(h)) = (
            o.label_source,
            ui.get_child_recursive(row, super::page::child::SLIDER_LABEL),
        ) {
            o.label = super::page::set_string_info(ui, h, table, sid);
        }
        let i = self.options.len();
        self.options.push(ChatOption::Opacity(o));
        self.refresh(ui, i);
        Some(i)
    }

    /// The slider-label write `(slider, leftId, rightId)` — the two end
    /// captions `0x1000021E` / `0x1000021F`, in string table enum `0x10000003`.
    pub fn set_slider_label(
        &mut self,
        ui: &mut UiSystem,
        i: usize,
        left: &str,
        right: &str,
    ) -> usize {
        let Some(ChatOption::Opacity(o)) = self.options.get(i) else {
            return 0;
        };
        let row = o.row;
        let table = super::preferences::table(ui);
        let mut n = 0;
        for (id, token) in [
            (super::page::child::SLIDER_END_LEFT, left),
            (super::page::child::SLIDER_END_RIGHT, right),
        ] {
            let Some(h) = ui.get_child_recursive(row, id) else {
                continue;
            };
            let sid = dereth_primitives::num::hash::str_hash(token.as_bytes());
            n += usize::from(super::page::set_string_info(ui, h, table, sid).is_some());
        }
        self.slider_end_captions += n;
        n
    }

    /// Add a 64-bit checkbox-bitfield option `(property, windowId)`, and the two
    /// blocks the option initialisation inlines instead of calling it — they are the same code.
    ///
    /// Template 8, checked as type `0x10000044`; the gameplay option property (chat property
    /// type), the window id as user data, the default mask, the filter-group run, child
    /// creation, and option registration.
    ///
    /// **The `Gameplay` group is offered to every window except the main one** — see the module
    /// docs; `crate::chat::interface::filter_groups_for` is the table.
    pub fn add_checkbox_bitfield64_option(
        &mut self,
        ui: &mut UiSystem,
        window_id: u32,
        property: u32,
        view: &dyn GameView,
    ) -> Option<usize> {
        let row = self.add_row(ui, TEMPLATE_CHECKBOX_BITFIELD64)?;
        if ui.node(row).map(dereth_ui::ElementNode::ty)
            != Some(crate::element_types::ty::OPTION_CB_BITFIELD64)
        {
            self.failures += 1;
            return None;
        }
        // The default mask — the per-window arm, which is the same table the chat interface's
        // post-init sets its text-type filter from.
        let default = crate::chat::interface::default_filter(window_id);
        // The checkbox bitfield64 option control's value read — the chat-window option for its
        // window and property, whose result is preloaded from the default,
        // so a window with no stored blob reads as its default rather than as zero.
        let stored = view.chat_window_filter(window_id);
        self.values_seen += usize::from(stored.is_some());
        let current = stored.unwrap_or(default);
        let mut o = ChatFilterOption {
            window_id,
            property,
            element: row,
            current,
            saved: current,
            default,
            children: Vec::new(),
        };
        // The `(mask, label, tooltip)` filter-group run.
        for g in crate::chat::interface::filter_groups_for(window_id) {
            o.children.push(ChatFilterChild {
                mask: g.mask,
                label_token: g.label,
                tooltip_token: format!("{}{FILTER_TOOLTIP_SUFFIX}", g.label),
                element: None,
                row: None,
                label: None,
                image: None,
            });
        }
        let i = self.options.len();
        self.options.push(ChatOption::Filter(o));
        self.create_children(ui, i);
        self.refresh(ui, i);
        Some(i)
    }

    /// The checkbox bitfield64 option control's create children — one `0x10000521` row per
    /// filter group, each carrying its bit index in `0x10000084` and its caption on the
    /// `0x10000219` button itself.
    fn create_children(&mut self, ui: &mut UiSystem, i: usize) {
        let Some(ChatOption::Filter(o)) = self.options.get(i) else {
            return;
        };
        let (control, n) = (o.element, o.children.len());
        let table = ui.env().cloned().and_then(|e| {
            e.did_by_enum(
                super::preferences::STRING_TABLE_GROUP,
                FILTER_STRING_TABLE_ENUM,
            )
        });
        let mut list = ListBoxWidget::bind(ui, control);
        for k in 0..n {
            let Some(row) = list.add_from_template(ui, 0, None) else {
                self.failures += 1;
                continue;
            };
            let Some(cb) = ui.get_child_recursive(row, FILTER_CHECKBOX) else {
                self.failures += 1;
                continue;
            };
            // Type 1 — a `Button`, not a `CheckboxOption`.
            if ui.node(cb).map(dereth_ui::ElementNode::ty) != Some(dereth_ui::ElementType(1)) {
                self.failures += 1;
                continue;
            }
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            // a bit index < 32
            ui.set_attribute_int(cb, ATTR_BIT_INDEX, k as i32);
            let caption = {
                let Some(ChatOption::Filter(o)) = self.options.get(i) else {
                    continue;
                };
                let token = o.children[k].label_token;
                let sid = dereth_primitives::num::hash::str_hash(token.as_bytes());
                table.and_then(|t| super::page::set_string_info(ui, cb, t, sid))
            };
            self.child_captions += usize::from(caption.is_some());
            let Some(ChatOption::Filter(o)) = self.options.get_mut(i) else {
                continue;
            };
            o.children[k].row = Some(row);
            o.children[k].element = Some(cb);
            o.children[k].label = caption;
        }
        // The client's tail, which is **three** calls, in this order, not a bare
        // `update_layout`:
        //
        // Calculate the list's full paper height from all row heights, then resize
        // the list box to that height while preserving its current width.
        //
        // **The resize is what matters.** The `0x10000520` row this control is
        // ships 99 pixels tall — five of its twelve or thirteen check boxes — and
        // `PlayerOptionPage`'s own list box stacks the page's rows by
        // `ElementNode::region.box_.height()`. Without the resize the sixth box onwards falls
        // outside the row that owns it, is overdrawn by the next section, and — for the
        // windows below the first — lands past the bottom of the page and is clipped away.
        // Without it *General Channel*, *Trade Channel*, *LFG Channel*, *Roleplay Channel* and
        // *Society Channel* — boxes 7..11 of every window — are exactly the ones the missing 140
        // pixels swallow.
        //
        // The paper-size calculation for a vertical list takes the count from the row count
        // and sums the item heights — which is `ListBoxWidget::item_heights`, written by the
        // `update_layout` immediately above. The bound widget is dropped after: every row it made
        // is already recorded on the filter group it belongs to, which is where the refresh and
        // the message arm look.
        list.update_layout(ui);
        let paper = list.item_heights.iter().sum::<i32>();
        let width = ui.node(control).map_or(0, |n| n.region.box_.width());
        ui.resize_to(control, width, paper);
    }

    /// The option controls' refresh — push the current value onto the elements.
    ///
    /// * The slider option control's refresh — clamp, then attribute `0x86`.
    /// * The checkbox bitfield64 option control's refresh — per child, boolean attribute `0x0E`
    ///   = `(current & mask) != 0` and, when that is true, the
    ///   *fully set* / *partly set* picture. See [`ChatFilterChild::image`] for the deviation.
    pub fn refresh(&mut self, ui: &mut UiSystem, i: usize) {
        match self.options.get(i) {
            Some(ChatOption::Opacity(o)) => {
                let (element, pos) = (o.element, o.position());
                ui.set_attribute_float(element, super::page::ATTR_POSITION, pos);
                dereth_ui::widgets::scrollbar::update_layout_of(ui, element);
            }
            Some(ChatOption::Filter(f)) => {
                let control = f.element;
                let all = crate::bind::attr_data_id(ui, control, ATTR_IMAGE_ALL);
                let some = crate::bind::attr_data_id(ui, control, ATTR_IMAGE_SOME);
                let states: Vec<(Option<ElemHandle>, bool, bool)> = (0..f.children.len())
                    .map(|k| {
                        let (any, whole) = f.child_state(k);
                        (f.children[k].element, any, whole)
                    })
                    .collect();
                for (k, (h, any, whole)) in states.into_iter().enumerate() {
                    if let Some(h) = h {
                        ui.set_attribute_bool(h, super::page::ATTR_CHECKED, any);
                    }
                    let image = if any {
                        if whole {
                            all
                        } else {
                            some
                        }
                    } else {
                        None
                    };
                    if let (Some(h), Some(image)) = (h, image) {
                        if let Some(first_child) = ui.children(h).into_iter().next() {
                            ui.set_media_image_for_state(
                                first_child,
                                image,
                                1,
                                dereth_ui::StateId(6),
                            );
                        }
                    }
                    let Some(ChatOption::Filter(f)) = self.options.get_mut(i) else {
                        continue;
                    };
                    f.children[k].image = image;
                }
            }
            None => {}
        }
    }

    /// The option controls' apply — the write, and the only thing on this page that changes
    /// anything outside it.
    ///
    /// Returns the effect so the caller can deliver the gameplay-option-changed notice to the chat
    /// windows, which are the receivers and which this page cannot reach.
    pub fn apply(
        &self,
        requests_out: &mut crate::requests::Outbox,
        i: usize,
    ) -> Option<ChatOptionEffect> {
        let e = match self.options.get(i)? {
            // The slider option control's apply's "has a property name" arm —
            // the gameplay option `prop`, in the top-level collection.
            ChatOption::Opacity(o) => ChatOptionEffect::Opacity {
                property: o.property,
                value: o.current,
            },
            // The checkbox bitfield64 option control's apply's chat-property-type arm —
            // the chat-window option `(window id, prop)`.
            ChatOption::Filter(f) => ChatOptionEffect::Filter {
                window_id: f.window_id,
                mask: f.current,
            },
        };
        match e {
            ChatOptionEffect::Opacity { property, value } => {
                requests_out.emit(UiRequest::SetChatOpacity { property, value });
            }
            ChatOptionEffect::Filter { window_id, mask } => {
                requests_out.emit(UiRequest::SetChatWindowFilter {
                    window: window_id,
                    mask,
                });
            }
        }
        Some(e)
    }

    /// The option controls' element-message arms.
    ///
    /// * **`0x0A` on a slider** — read attribute `0x86`,
    ///   `current = lower + frac·(upper - lower)`, apply.
    /// * **`1` on a filter check box** — read the bit index (`0x10000084`) and the checked state
    ///   (`0x0E`) off the pressed element; when the index names a filter group, set or clear
    ///   that group's mask in the current value, then apply.
    ///
    /// The check box is matched by **handle**, not by element id: all 64 of them carry
    /// `0x10000219`.
    ///
    /// [`Self::on_option_changed`] runs after the arm, which is what keeps the
    /// two opacity sliders ordered. Returns every effect the gesture produced, in order — the
    /// paired slider's included.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::msg::ElementMessage,
    ) -> Vec<ChatOptionEffect> {
        use dereth_ui::msg::element::id;
        let mut out = Vec::new();
        // Slider first: its element is unique and its message is `0x0A`.
        if m.id == id::SCROLL_POSITION {
            if let Some(i) = self
                .options
                .iter()
                .position(|o| matches!(o, ChatOption::Opacity(s) if s.element == m.source))
            {
                let frac = crate::bind::attr_float(ui, m.source, super::page::ATTR_POSITION)
                    .unwrap_or(0.0);
                let Some(ChatOption::Opacity(o)) = self.options.get_mut(i) else {
                    return out;
                };
                o.current = o.value_at(frac);
                out.extend(self.apply(&mut ui.requests, i));
                out.extend(self.on_option_changed(ui, i));
                return out;
            }
        }
        if m.id != id::BUTTON_CLICKED {
            return out;
        }
        let found = self.options.iter().enumerate().find_map(|(i, o)| match o {
            ChatOption::Filter(f) => f
                .children
                .iter()
                .position(|c| c.element == Some(m.source))
                .map(|k| (i, k)),
            ChatOption::Opacity(_) => None,
        });
        let Some((i, k)) = found else { return out };
        let on = crate::bind::attr_bool(ui, m.source, super::page::ATTR_CHECKED).unwrap_or(false);
        let Some(ChatOption::Filter(f)) = self.options.get_mut(i) else {
            return out;
        };
        let mask = f.children[k].mask;
        f.set_bits(mask, on);
        out.extend(self.apply(&mut ui.requests, i));
        // Retail does not refresh here — the element-message arm's tail is the base class's
        // element-message handler, not the option's own refresh. But the
        // *other* boxes of the same control share bits with the one just clicked (`Gameplay`
        // overlaps `Combat`, `Magic`, `AreaSpeech`, `General`, `Trade`, `LFG`, `Roleplay` and
        // `Error`), and the option-changed notice handler's refresh is what runs
        // when the notice comes back round. This build has no notice bus for the refresh-options-panel notice, so the
        // redraw is done here — the same edge, one call earlier.
        self.refresh(ui, i);
        out
    }

    /// The slider-link clamp that keeps the
    /// **idle** opacity at or below the **active** one.
    ///
    /// ```text
    ///   sliders only (type 0x10000037)
    ///   found as a link's left  (this is the idle slider)   -> if (active < idle)  active = idle
    ///   found as a link's right (this is the active slider) -> if (active < idle)  idle   = active
    /// ```
    ///
    /// The partner is written through the slider's set-current-value, which sets the current
    /// value, refreshes and applies — so moving one slider past the other **also writes the other
    /// property**. Returns that second effect when it fires.
    pub fn on_option_changed(&mut self, ui: &mut UiSystem, i: usize) -> Option<ChatOptionEffect> {
        let ChatOption::Opacity(_) = self.options.get(i)? else {
            return None;
        };
        let (partner, raise) = self.slider_links.iter().find_map(|(l, r)| {
            if *l == i {
                Some((*r, true))
            } else if *r == i {
                Some((*l, false))
            } else {
                None
            }
        })?;
        let me = match self.options.get(i)? {
            ChatOption::Opacity(o) => o.current,
            ChatOption::Filter(_) => return None,
        };
        let theirs = match self.options.get(partner)? {
            ChatOption::Opacity(o) => o.current,
            ChatOption::Filter(_) => return None,
        };
        // Found as the left of a link: this is the idle slider, the partner is active — raise it
        // to `me` when it is below. Found as the right: this is active, the partner idle — lower it.
        let needs = if raise { theirs < me } else { me < theirs };
        if !needs {
            return None;
        }
        let ChatOption::Opacity(p) = self.options.get_mut(partner)? else {
            return None;
        };
        p.current = me;
        self.refresh(ui, partner);
        self.apply(&mut ui.requests, partner)
    }

    /// Apply, and the page's **show** arm.
    ///
    /// Each option's save-current sets both current and saved to the value read, so this **writes
    /// nothing and reads everything**. Returns how many controls came back different from what
    /// the page was showing.
    pub fn save_current_values(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        let mut moved = 0;
        for i in 0..self.options.len() {
            let changed = match &self.options[i] {
                ChatOption::Opacity(o) => {
                    let v = view.gameplay_option_float(o.property).unwrap_or(o.current);
                    let moved = v.to_bits() != o.current.to_bits();
                    let ChatOption::Opacity(o) = &mut self.options[i] else {
                        continue;
                    };
                    o.current = v;
                    o.saved = v;
                    moved
                }
                ChatOption::Filter(f) => {
                    let v = view.chat_window_filter(f.window_id).unwrap_or(f.current);
                    let moved = v != f.current;
                    let ChatOption::Filter(f) = &mut self.options[i] else {
                        continue;
                    };
                    f.current = v;
                    f.saved = v;
                    moved
                }
            };
            moved += usize::from(changed);
            self.refresh(ui, i);
        }
        moved
    }

    /// Cancel, and the hide path.
    ///
    /// The change test **then** the restore of the saved value, on the ones that changed only — which is why
    /// closing the page does not rewrite seven properties. Returns how many were reverted, and
    /// every effect, so the caller can deliver the notices.
    pub fn restore_saved_values(&mut self, ui: &mut UiSystem) -> (usize, Vec<ChatOptionEffect>) {
        let mut out = Vec::new();
        for i in 0..self.options.len() {
            if !self.options[i].changed() {
                continue;
            }
            match &mut self.options[i] {
                ChatOption::Opacity(o) => o.current = o.saved,
                ChatOption::Filter(f) => f.current = f.saved,
            }
            self.refresh(ui, i);
            out.extend(self.apply(&mut ui.requests, i));
        }
        (out.len(), out)
    }

    /// The *Restore Defaults* button and global
    /// message `0x0C`. **Unconditional**: every control, changed or not.
    pub fn restore_default_values(&mut self, ui: &mut UiSystem) -> (usize, Vec<ChatOptionEffect>) {
        let mut out = Vec::new();
        for i in 0..self.options.len() {
            match &mut self.options[i] {
                ChatOption::Opacity(o) => o.current = o.default,
                ChatOption::Filter(f) => f.current = f.default,
            }
            self.refresh(ui, i);
            out.extend(self.apply(&mut ui.requests, i));
        }
        (self.options.len(), out)
    }

    /// The option page base's change handler.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.options.iter().any(ChatOption::changed)
    }

    /// The player-option page's visibility-changed handler — [`Self::save_current_values`] on
    /// show, [`Self::restore_saved_values`] on hide.
    pub fn on_visibility_changed(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        visible: bool,
    ) -> (usize, Vec<ChatOptionEffect>) {
        if visible {
            (self.save_current_values(ui, view), Vec::new())
        } else {
            self.restore_saved_values(ui)
        }
    }

    /// The index of the control bound to one window's filter.
    #[must_use]
    pub fn filter_of(&self, window_id: u32) -> Option<usize> {
        self.options
            .iter()
            .position(|o| matches!(o, ChatOption::Filter(f) if f.window_id == window_id))
    }

    /// The index of the slider bound to one gameplay-option property.
    #[must_use]
    pub fn slider_of(&self, property: u32) -> Option<usize> {
        self.options
            .iter()
            .position(|o| matches!(o, ChatOption::Opacity(s) if s.property == property))
    }
}

/// Find the Chat Options page under `root`, by id **and** type.
///
/// `get_child_recursive` alone would do today — `0x1000050C` occurs once in the shipped tree —
/// but the three option pages are siblings under the one panel `0x1000018D` and they share their
/// Apply / Cancel / Defaults child ids, so a page bound to the wrong element finds no
/// `0x1000050D` under it and silently draws nothing. The type check is the runtime type check
/// the client's element factory performs for free; `super::character::find_page` exists for the same
/// reason, and there the id really is ambiguous.
#[must_use]
pub fn find_page(ui: &UiSystem, root: ElemHandle) -> Option<ElemHandle> {
    ui.element_list().iter().copied().find(|h| {
        ui.node(*h).is_some_and(|n| {
            n.element_id() == CHAT_PAGE_ELEMENT && n.ty() == crate::element_types::ty::CHAT_OPTIONS
        }) && is_under(ui, *h, root)
    })
}

fn is_under(ui: &UiSystem, mut h: ElemHandle, root: ElemHandle) -> bool {
    loop {
        if h == root {
            return true;
        }
        match ui.parent(h) {
            Some(p) => h = p,
            None => return false,
        }
    }
}

/// Bind the option box, then [`ChatOptionsPage::init_options`].
///
/// `None` when `0x1000050D` is not under the page, which is the loud form of *"the layout
/// changed"* rather than an empty page.
#[must_use]
pub fn chat_options_post_init(
    ui: &mut UiSystem,
    page: ElemHandle,
    view: &dyn GameView,
) -> Option<ChatOptionsPage> {
    let mut p = ChatOptionsPage::bind(ui, page);
    p.option_box.as_ref()?;
    p.init_options(ui, view);
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::interface::{filter_groups_for, window, FILTER_GROUPS};

    /// The **main window is the one without `Gameplay`**. Its initialization goes from
    /// its default straight to the Combat group `0x600040`,
    /// while the inline window-5 block adds `0x83912021` first.
    #[test]
    fn the_main_window_is_the_one_without_the_gameplay_group() {
        assert_eq!(filter_groups_for(window::MAIN).len(), 12);
        assert_eq!(filter_groups_for(window::MAIN_ALT).len(), 12);
        for w in [
            window::FLOATY_1,
            window::FLOATY_2,
            window::FLOATY_3,
            window::FLOATY_4,
        ] {
            assert_eq!(filter_groups_for(w).len(), 13, "floaty {w} offers Gameplay");
            assert_eq!(
                filter_groups_for(w)[0].label,
                "ID_ChatOption_TextFilter_Gameplay"
            );
        }
        assert_eq!(
            filter_groups_for(window::MAIN)[0].label,
            "ID_ChatOption_TextFilter_Combat"
        );
        // 12 + 13*4 = 64 check boxes over the five controls.
        let total: usize = [
            window::MAIN,
            window::FLOATY_1,
            window::FLOATY_2,
            window::FLOATY_3,
            window::FLOATY_4,
        ]
        .into_iter()
        .map(|w| filter_groups_for(w).len())
        .sum();
        assert_eq!(total, 64);
        assert_eq!(FILTER_GROUPS[0].mask, 0x8391_2021);
        assert_eq!(
            FILTER_GROUPS[11].mask, 0x1_0000_0000,
            "Society is the only bit above 32"
        );
    }

    /// Oracle: the default-mask arms of the checkbox builder, which are the same five
    /// masks the chat interface's post-init sets.
    #[test]
    fn every_window_gets_its_own_default_mask() {
        use crate::chat::interface::default_filter;
        assert_eq!(default_filter(window::MAIN), 0xFBFF_FFFF);
        assert_eq!(default_filter(window::FLOATY_1), 0x0000_101C);
        assert_eq!(default_filter(window::FLOATY_2), 0x0004_0C00);
        assert_eq!(default_filter(window::FLOATY_3), 0x0008_0000);
        assert_eq!(default_filter(window::FLOATY_4), 0x7800_0000);
    }

    /// Oracle: the ids in the module docs — the ids this page names, none of which are the
    /// other option pages'.
    #[test]
    fn the_page_names_its_own_elements() {
        assert_eq!(CHAT_PAGE_ELEMENT.0, 0x1000_050C);
        assert_eq!(OPTION_BOX.0, 0x1000_050D);
        assert_ne!(OPTION_BOX, super::super::config::OPTION_BOX);
        assert_ne!(OPTION_BOX, super::super::character::OPTION_BOX);
        assert_eq!(TEMPLATE_CHECKBOX_BITFIELD64, 8);
        assert_eq!(FILTER_CHECKBOX.0, 0x1000_0219);
        assert_eq!(ATTR_BIT_INDEX, 0x1000_0084);
        // The filter-group add writes `0x10000006`, not the `0x10000003` of every other page.
        assert_eq!(FILTER_STRING_TABLE_ENUM, 0x1000_0006);
        assert_ne!(
            FILTER_STRING_TABLE_ENUM,
            super::super::preferences::STRING_TABLE_ENUM
        );
        assert_eq!(super::super::pages::CHAT_FILTER_PROPERTY, 0x1000_007F);
        assert_eq!(
            dereth_client_contract::options::sheet::rows_for(
                dereth_client_contract::options::sheet::PageId::Chat,
                dereth_client_contract::options::interface::Interface::Modern,
            )
            .filter_map(|r| match r.value {
                dereth_client_contract::options::sheet::Value::Opacity(p) => Some(p),
                _ => None,
            })
            .collect::<Vec<_>>(),
            [0x1000_0080, 0x1000_0081]
        );
        assert_eq!(OPACITY_RANGE, (0.0, 1.0));
    }
}
