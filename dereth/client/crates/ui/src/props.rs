//! The element attribute surface: the property collection, the three-collection merge, the
//! state-change diff, and the base `on_set_attribute` id table.
//!
//! This covers all three property collections and property-table value
//! typing. `PropertyValue` comes from asset decoding — the property *types* live only in the
//! `MasterProperty` dat object and decoding a `BaseProperty` without them is impossible.
//!
//! The diff in [`state_diff`] is the whole of AC's skinning: a button's pressed state is a
//! `StateDesc` that overrides `Image` and adds a `Sound` media entry, and the element only ever
//! learns about it through `on_set_attribute`.

use std::collections::BTreeMap;

pub use dereth_assets::ui::{BasePropertyType, PropertyValue};

/// `PropertyCollection` — attribute id → value.
///
/// A `BTreeMap` rather than a `HashMap`: `dereth-ui` is on the lint's order-contract list because
/// several AC panels are built by walking a hash table with no sort. Nothing in the *property*
/// path is known to depend on hash order (`Initialize` step 4 applies every property), so an
/// ordered map is both deterministic and a strict improvement on a randomised one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertyCollection(pub BTreeMap<u32, PropertyValue>);

impl PropertyCollection {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn get(&self, id: u32) -> Option<&PropertyValue> {
        self.0.get(&id)
    }

    #[must_use]
    pub fn contains(&self, id: u32) -> bool {
        self.0.contains_key(&id)
    }

    pub fn set(&mut self, id: u32, v: PropertyValue) {
        self.0.insert(id, v);
    }

    pub fn remove(&mut self, id: u32) -> Option<PropertyValue> {
        self.0.remove(&id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Update one collection from another as used by state-description incorporation: entries in
    /// `src` overwrite entries in `dst` by id.
    pub fn update_from(&mut self, src: &Self) {
        for (k, v) in &src.0 {
            self.0.insert(*k, v.clone());
        }
    }

    /// Build from the decoded `(id, BaseProperty)` list.
    #[must_use]
    pub fn from_asset(list: &[(u32, dereth_assets::ui::BaseProperty)]) -> Self {
        let mut m = BTreeMap::new();
        for (id, p) in list {
            m.insert(*id, p.value.clone());
        }
        Self(m)
    }

    // ---- typed accessors ---------------------------------------------------------------------
    // Each typed accessor returns None when the property is
    // absent *or* carries a different value type, which is what the original's dynamic cast does.

    #[must_use]
    pub fn get_bool(&self, id: u32) -> Option<bool> {
        match self.get(id)? {
            PropertyValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_int(&self, id: u32) -> Option<i32> {
        match self.get(id)? {
            PropertyValue::Integer(v) => Some(*v),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_float(&self, id: u32) -> Option<f32> {
        match self.get(id)? {
            PropertyValue::Float(v) => Some(*v),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_enum(&self, id: u32) -> Option<u32> {
        match self.get(id)? {
            PropertyValue::Enum(v) => Some(*v),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_data_id(&self, id: u32) -> Option<dereth_primitives::DataId> {
        match self.get(id)? {
            PropertyValue::DataFile(d) | PropertyValue::Waveform(d) => Some(*d),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_color(&self, id: u32) -> Option<u32> {
        match self.get(id)? {
            PropertyValue::Color(c) => Some(*c),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_string_info(&self, id: u32) -> Option<&dereth_assets::ui::StringInfo> {
        match self.get(id)? {
            PropertyValue::StringInfo(s) => Some(s),
            _ => None,
        }
    }
}

/// The three collections an element merges, in increasing priority:
///
/// 1. the element description's properties;
/// 2. the current state description's properties;
/// 3. the element's runtime property overrides.
#[must_use]
pub fn merge_three(
    element: &PropertyCollection,
    state: Option<&PropertyCollection>,
    instance: &PropertyCollection,
) -> PropertyCollection {
    let mut out = element.clone();
    if let Some(s) = state {
        out.update_from(s);
    }
    out.update_from(instance);
    out
}

/// One attribute-changed call the state-change diff decides to make.
///
/// The state change computes the old merged set and the new merged set and calls the element's
/// attribute-changed handler for
///
/// * every property whose value **differs**,
/// * every property present **only in the new** set, and
/// * every property that **disappeared** — with a value-less `BaseProperty` carrying only the name,
///   so the subclass can reset it. That is [`AttrChange::value`] being `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct AttrChange {
    pub id: u32,
    pub value: Option<PropertyValue>,
}

/// The state-change diff. Deterministic order (ascending id) because the collection is ordered.
#[must_use]
pub fn state_diff(old: &PropertyCollection, new: &PropertyCollection) -> Vec<AttrChange> {
    let mut out = Vec::new();
    for (id, v) in &new.0 {
        match old.get(*id) {
            Some(prev) if prev == v => {}
            _ => out.push(AttrChange {
                id: *id,
                value: Some(v.clone()),
            }),
        }
    }
    for id in old.0.keys() {
        if !new.contains(*id) {
            out.push(AttrChange {
                id: *id,
                value: None,
            });
        }
    }
    out.sort_by_key(|c| c.id);
    out
}

/// Attribute ids handled by the base, plus the three ids the
/// base class consumes elsewhere.
///
/// Ids not in this table are not errors. Attributes not consumed by these handlers pass
/// through to the property bag unchanged.
pub mod attr {
    /// bool — a button is **disabled**. A button writes it when
    /// the state it is given is `0x0D`, and refuses to raise the click while
    /// it is set. The character-selection screen is the caller that matters:
    /// it enables and disables Enter Game and Create with a state change, not with this attribute.
    pub const DISABLED: u32 = 0x0D;
    /// bool — the button **toggles**. A click flips
    /// [`TOGGLED`] on a click when this is set, and turns a
    /// state set to 6 or to 1 into the same flip.
    pub const TOGGLE_BUTTON: u32 = 0x0B;
    /// bool — a disabled button still raises its click. Read beside [`DISABLED`] in the button's
    /// mouse-down and mouse-up.
    pub const CLICK_WHILE_DISABLED: u32 = 0x0C;
    /// bool — the toggle button's current position, which selects the 6/7/8 half of
    /// the client's state table.
    pub const TOGGLED: u32 = 0x0E;
    /// bool — auto-repeat ("hot clicking") while held. A held button raises element
    /// message 2 immediately and arms the repeat.
    pub const HOT_CLICK: u32 = 0x0F;
    /// float — seconds before the **first** auto-repeat, and only the first.
    /// The button's mouse-down ends with
    /// `next hot-click time = float attribute 0x10 + current time`.
    /// Every repeat *after* that one reads [`HOT_CLICK_REPEAT_INTERVAL`].
    ///
    /// **This is the first interval only; it does not stand in for both periods.** The
    /// master-property table names 0x10
    /// `UICore_Button_hot_click_first_interval` and 0x11 `UICore_Button_hot_click_interval`, and
    /// the scrollbar's default-hot-click setup defaults them to **0.5** and
    /// **0.125** — a four-to-one difference, so using one for the other is not a rounding error.
    pub const HOT_CLICK_FIRST_INTERVAL: u32 = 0x10;
    /// float — seconds between auto-repeats, from the second one on.
    /// The button's global-message listener reads it on **every** repeat
    /// and then *accumulates*:
    /// `next hot-click time += period`, measured from the previous **scheduled** time rather than
    /// from the current time, so a slow frame does not stretch the cadence and a fast one does
    /// not compress it. That is the whole reason the repeat count is a function of elapsed time
    /// and not of the frame rate.
    pub const HOT_CLICK_REPEAT_INTERVAL: u32 = 0x11;
    /// enum — the `InputAction` a click on this button fires.
    /// The button's click handler's first act is
    /// reading enum attribute `0x12` as the action; **38** elements of the shipped `0x21000005`
    /// tree carry one, including all seven toolbar panel buttons and the six indicator lamps.
    pub const BUTTON_INPUT_ACTION: u32 = 0x12;
    /// bool — highlight on rollover. Without it a button never enters state 2/7 no matter where
    /// the pointer is.
    pub const ROLLOVER_HIGHLIGHT: u32 = 0x13;
    /// bool — whether the element can be activated.
    pub const ACTIVATABLE: u32 = 0x33;
    /// bool — activate the element when it is shown.
    pub const ACTIVATE_ON_SHOW: u32 = 0x34;
    /// bool — this element catches drops.
    pub const DROP_CATCHER: u32 = 0x36;
    /// bool — the element has a context menu; the setter then updates mouse visibility.
    pub const CONTEXT_MENU: u32 = 0x37;
    /// bool — dropping on this element is disabled.
    pub const DROP_DISABLED: u32 = 0x38;
    /// bool — this element cannot spawn a drag proxy.
    pub const NO_DRAG_PROXY: u32 = 0x39;
    /// bool — whether the element can be dragged.
    pub const DRAGABLE: u32 = 0x3A;
    /// bool — **`UICore_Element_hide`**: applies visibility as `!value`. `true` means *hidden*.
    ///
    /// The sense is inverted with respect to [`crate::UiSystem::set_visible`] and getting it the
    /// wrong way up is a whole-HUD defect, so it is named for what the retail `MasterProperty`
    /// calls it rather than for the setter it drives. The base attribute handler's case `0x3B` does not
    /// by itself show the sign; four independent things fix it, and all
    /// four say `true` means hidden:
    ///
    /// 1. `MasterProperty 0x39000001` row `0x3B` (group 8, type `Bool`) has name enum **59 =
    ///    `UICore_Element_hide`** — read straight out of the retail `client_portal.dat`.
    /// 2. The toolbar's four stacked stance icons in `classic_toolbar` (`0x21000016`):
    ///    `0x10000192` (the peace-mode dove `0x06004CEC`) carries **no** `0x3B`, and
    ///    `0x10000193`/`0x10000194`/`0x10000195` each carry `0x3B = true`. Read as `hide`, the
    ///    layout comes up showing exactly the dove, matching the observed retail peace-mode
    ///    start state.
    /// 3. Read as `hide`, the shipped `0x21000005` layout reproduces the **whole** live
    ///    retail start state of the eighteen children of the gameplay root `0x10000495`,
    ///    `0x21000009` and `classic_admin` included — see
    ///    `dereth_ui_screens::screens::gameplay::GamePlayScreen::HUD_START_VISIBILITY`.
    /// 4. `classic_intro` (`0x21000001`): both media children carry `0x3B = true` at element
    ///    level and `0x3B = false` in each state that owns media, i.e. "hidden except in my own
    ///    frame". The other reading makes the intro fifteen seconds of black.
    pub const HIDE: u32 = 0x3B;
    /// int — **maximum height**. See [`MIN_WIDTH`] for the label correction.
    pub const MAX_HEIGHT: u32 = 0x3C;
    /// int — **maximum width**. See [`MIN_WIDTH`].
    pub const MAX_WIDTH: u32 = 0x3D;
    /// int — **minimum height**. See [`MIN_WIDTH`].
    pub const MIN_HEIGHT: u32 = 0x3E;
    /// int — **minimum width**, read in the element's [`crate::UiSystem::resize_to`].
    ///
    /// **The four clamps `0x3C`–`0x3F`, which are easy to get backwards**. Three
    /// functions read all four and agree exactly on
    /// which is which — the element's resize, the panel's resize
    /// (the same body, one class down) and the element's mouse resize:
    ///
    /// ```text
    /// 0x3C: if (v < height) height = v      -> MAX height
    /// 0x3E: if (height < v) height = v      -> MIN height
    /// 0x3D: if (v < width)  width  = v      -> MAX width
    /// 0x3F: if (width  < v) width  = v      -> MIN width
    /// ```
    ///
    /// The clamp *direction* is what names them, and it is not recoverable from the id order:
    /// "min width, min height, max width, max height" is the exact reverse.
    pub const MIN_WIDTH: u32 = 0x3F;
    /// bool — whether the element blocks clicks.
    pub const BLOCK_CLICKS: u32 = 0x40;
    /// bool — notify on resize.
    pub const NOTIFY_ON_RESIZE: u32 = 0x41;
    /// bool — notify on move.
    pub const NOTIFY_ON_MOVE: u32 = 0x42;
    /// bool — notify on create.
    pub const NOTIFY_ON_CREATE: u32 = 0x43;
    /// bool — the resize-line flag (flags bit 0).
    pub const RESIZE_LINE: u32 = 0x44;
    /// bool — save the element's location.
    pub const SAVE_LOCATION: u32 = 0x45;
    /// bool — save the element's size.
    pub const SAVE_SIZE: u32 = 0x46;
    /// enum — the **`ElementDesc` id of the tooltip window**, inside the layout
    /// [`TOOLTIP_LAYOUT`] names. `UICore_Element_tooltip_ID`.
    ///
    /// **This is the gate on the whole tooltip.** The element's start-tooltip-at-mouse
    /// reads it as an enum and **returns NULL
    /// when it is absent** — an element with tooltip text and no `0x47` shows nothing at all.
    /// 51 of `0x21000005`'s elements carry it, all but one naming an element of layout
    /// `0x21000041`.
    pub const TOOLTIP_ELEMENT: u32 = 0x47;
    /// data file — the layout the tooltip window is built from. `UICore_Element_tooltip_layout`.
    ///
    /// The start-tooltip path reads it as a DataID. When it is absent or invalid
    /// the element's **own** layout's DataID is used instead.
    ///
    pub const TOOLTIP_LAYOUT: u32 = 0x48;
    /// `StringInfo` — the tooltip text **authored in the layout**, used when the element's live tooltip text is empty.
    /// `UICore_Element_tooltip_entry`.
    ///
    /// Read through the generic property fetch, reached only after the live tooltip text fails its
    /// validity check. Twenty-one of `0x21000005`'s elements carry
    /// one and **nothing in the client sets a tooltip for them** — a shipped string is the
    /// whole of their tooltip.
    pub const TOOLTIP_ENTRY: u32 = 0x49;
    /// enum — the id of the child **inside the tooltip window** that receives the string.
    /// `UICore_Element_tooltip_text_ID`.
    ///
    /// Read on the *tooltip*, not on the owner: the manager reads `0x4A` as an enum,
    /// then fetches that child recursively and requires a text element before writing it.
    /// All four tooltip windows in layout `0x21000041` carry `0x4A = 0x10000396`.
    pub const TOOLTIP_TEXT_CHILD: u32 = 0x4A;
    /// bool — whether tooltips are enabled for the region.
    ///
    /// **The sole gate the element's mouse-hover handler tests**, with the manager's
    /// tooltips-enabled flag. It does **not** test whether the element has tooltip
    /// text — that decision belongs to `start_tooltip_at_mouse`, which falls back to
    /// [`TOOLTIP_ENTRY`].
    ///
    pub const TOOLTIP_ON: u32 = 0x4B;
    /// float — the alpha blend modifier.
    pub const ALPHA_BLEND_MOD: u32 = 0x4D;
    /// enum — the input map pushed while this element has focus.
    pub const INPUT_MAP: u32 = 0x4E;
    /// float — per-element tooltip delay in seconds, read in.
    pub const TOOLTIP_DELAY: u32 = 0x50;
    /// bool — `UICore_Text_auto_tooltip_truncated_text`: **a clipped label makes its own full
    /// text its tooltip.**
    ///
    /// The text element's truncation recalculation opens by reading `0xD0` as a bool and then, on
    /// the branch where the text does not fit, sets its own text as its tooltip and sets
    /// the element's has-tooltip flag; on the branch where it does fit it clears that flag
    /// and clears the tooltip. It is the one **generic** tooltip site in the
    /// client.
    pub const AUTO_TOOLTIP_TRUNCATED_TEXT: u32 = 0xD0;
    /// bool — whether the element erases its background.
    pub const ERASE_BACKGROUND: u32 = 0x51;
    /// enum — which game-view edge the element clamps to.
    pub const CLAMP_GAME_VIEW: u32 = 0x52;
    /// bool — draw after children; setting it also marks the root dirty here.
    pub const DRAW_AFTER_CHILDREN: u32 = 0x53;
    /// int — tiling offset X.
    pub const TILING_OFFSET_X: u32 = 0x54;
    /// int — tiling offset Y.
    pub const TILING_OFFSET_Y: u32 = 0x55;
    /// re-dispatches 0x55 and 0x54.
    pub const TILING_OFFSET_BOTH: u32 = 0x56;
    /// enum — registers the element for this input action.
    pub const INPUT_ACTION: u32 = 0x57;
    /// enum — visibility-toggle behaviour, read for element message 0x31.
    /// **1 = toggle, 2 = show, 3 = hide** — as the retail element message handler treats
    /// it; see the `0x31` arm in `crate::UiSystem::base_listen_to_element_message`. The order is
    /// easy to get backwards; all 41 shipped `0x57` listeners carry `1`.
    pub const VISIBILITY_TOGGLE_MODE: u32 = 0x58;
    /// **enum — the UI-object mode.** The element's attribute setter reads it into a local
    /// that starts at `3`, calls the should-own-object setter with `value != 0` and then updates for the parent
    /// size. See [`super::UiObjectMode`] for the four values and
    /// where they diverge; it is **not** a bool.
    pub const UI_OBJECT_MODE: u32 = 0xCD;
    /// The old name for [`UI_OBJECT_MODE`], kept so an out-of-crate reader does not break. It
    /// names the *bool* the enum collapses to, which is `mode != 0`.
    pub const SHOULD_OWN_OBJECT: u32 = UI_OBJECT_MODE;

    // ---- Text element ----------------------------------------------------------------------
    pub const TEXT_H_JUSTIFY: u32 = 0x14;
    pub const TEXT_V_JUSTIFY: u32 = 0x15;
    pub const TEXT_EDITABLE: u32 = 0x16;
    pub const TEXT_STRING: u32 = 0x17;
    /// An **array** of font `DataFile`s indexed by font number;
    /// the text element's current font is read from element 0.
    pub const TEXT_FONT_DID: u32 = 0x1A;
    /// An array of `Color`s; the text element's current font colour is read from element 0.
    ///
    /// 0x1B is not "font index/size", and 0x1D is not "font colour".
    /// The text element reads 0x1B into its font colour and 0x1D into its tag font colour, and every shipped layout
    /// carries a `Color` array at 0x1B. Some also carry 0x1D; see [`TEXT_TAG_FONT_COLOR`].
    pub const TEXT_FONT_COLOR: u32 = 0x1B;
    /// An array of `Color`s used as the colour for a `TextTag` run, and only for a run whose type is
    /// `Tell`
    /// (`0x10000001`): the text element's glyph builder compares the tag type against
    /// `0x10000001` and picks the tag font colour on a match and the font colour otherwise.
    ///
    /// **The shipped layouts do author it.** Measured over the gameplay screen's element tree:
    /// **14 of 14319 elements**
    /// carry a 0x1D property, all of them a **one**-entry `Color` array holding `#00B200` --
    /// `0x10000011` (the chat log) and `0x10000016` on each of the five chat windows, the three
    /// `<EXAM>` description panes `0x1000013C` / `0x1000013E` / `0x1000013F`, and `0x10000163`.
    ///
    /// One entry is what makes the colour constant across chat channels.
    /// The append-with-font passes the *same* run index to the font-colour helper
    /// for 0x1B and for 0x1D, and the helper bounds-checks that index against the array's own
    /// count and writes nothing when it is past the end
    /// (an absent property returns even earlier).
    /// The chat colour table
    /// replaces 0x1B with a 34-entry array and builds **no** 0x1D -- so chat type 0x1B = 27 resolves inside 0x1B and is
    /// past the end of 0x1D, leaving the tag colour on element 0 for every line.
    pub const TEXT_TAG_FONT_COLOR: u32 = 0x1D;
    pub const TEXT_MAX_CHARACTERS: u32 = 0x1E;
    pub const TEXT_NO_IME: u32 = 0x1F;
    pub const TEXT_ONE_LINE: u32 = 0x20;
    pub const TEXT_OUTLINE: u32 = 0x21;
    pub const TEXT_OUTLINE_COLOR: u32 = 0x22;
    pub const TEXT_MARGIN_LEFT: u32 = 0x23;
    pub const TEXT_MARGIN_RIGHT: u32 = 0x24;
    pub const TEXT_MARGIN_TOP: u32 = 0x25;
    pub const TEXT_MARGIN_BOTTOM: u32 = 0x26;
    pub const TEXT_SELECTABLE: u32 = 0x27;
    pub const TEXT_TRIM_FROM_TOP: u32 = 0x28;
    pub const TEXT_FIT_TO_TEXT: u32 = 0x29;
    pub const TEXT_TRUNCATION_SUFFIX: u32 = 0xC7;
    /// The text element's attribute setter routes this to its lose-focus-on-escape flag.
    pub const TEXT_LOSE_FOCUS_ON_ESCAPE: u32 = 0xCB;
    /// The same setter routes this to its lose-focus-on-accept-input flag.
    pub const TEXT_LOSE_FOCUS_ON_ACCEPT: u32 = 0xCC;
    /// 0xD1 — **select the whole box when a click first focuses it**, the only read of this id in
    /// the retail client: the text element's mouse-down reads it as a bool and, if the element did
    /// not already have focus and the bool is set, selects all and returns.
    pub const TEXT_SELECT_ALL_ON_FOCUS: u32 = 0xD1;

    // ---- Dialog ----------------------------------------------------------------------------
    /// Input — **a bool**: replace whatever is open on this queue with the new dialog.
    ///
    /// This key is read into a **one-byte**
    /// Boolean through the same kind of getter used for modal property `0xAC`.
    /// The float property `0xC6` and unsigned property `0xC3` use distinct typed getters,
    /// confirming that this value is neither of those types.
    ///
    /// When the Boolean is set and something is already open on the queue, dialog creation
    /// first cancels the open dialog,
    /// then inserts the new dialog at the **head**,
    /// and finally creates it — the ordering the documentation calls "jump the queue".
    ///
    /// Without a reader of this key the jump arm exists and cannot be reached.
    pub const DIALOG_REPLACE: u32 = 0x8D;
    /// Input — dialog kind, 1..=7.
    pub const DIALOG_KIND: u32 = 0x8E;
    /// Input — caption of button 1 (child element 0x17).
    pub const DIALOG_BUTTON1: u32 = 0x90;
    /// Input — caption of button 2 (child element 0x19).
    pub const DIALOG_BUTTON2: u32 = 0x91;
    /// **out** — Boolean: true for button 0x17, false for button 0x19 or the dialog's cancel.
    /// The handler uses compare/set-equal then a bool write, not an integer write of the button id.
    ///
    /// The confirmation dialog's answer property and **only** its own: the other five answering
    /// subclasses each write a different id. See [`crate::dialog::DialogKind::answer_property`].
    pub const DIALOG_ANSWER: u32 = 0x92;
    /// Input — caption of the message dialog's single button (child `0x26`).
    /// Read by its `set_data` under hash key `0x95`.
    pub const DIALOG_MESSAGE_BUTTON: u32 = 0x95;
    /// Input — caption of the text-input dialog's single button (child `0x2A`).
    /// Read by its `set_data` under hash key `0x97`.
    pub const DIALOG_TEXT_INPUT_BUTTON: u32 = 0x97;
    /// **out** — the string the text-input dialog harvested out of its box (child `0x2B`).
    /// Written by its message handler, and by its cancel, which writes an empty one.
    ///
    pub const DIALOG_TEXT_INPUT_ANSWER: u32 = 0x98;
    /// Input — caption of the confirmation text-input dialog's *Done* button (child `0x2E`).
    /// Read by its `set_data` under hash key `0x9A`.
    ///
    pub const DIALOG_TEXT_INPUT_ACCEPT_CAPTION: u32 = 0x9A;
    /// Input — caption of its *Cancel* button (child `0x2F`), hash key `0x9B`.
    ///
    pub const DIALOG_TEXT_INPUT_CANCEL_CAPTION: u32 = 0x9B;
    /// **out** — the string the confirmation text-input dialog harvested out of its box (child `0x2C`).
    ///
    /// Its message handler's `0x2E` arm writes the box's text;
    /// its cancel writes an **empty** one.
    /// That difference is the whole of *the player typed nothing* versus *the player cancelled*,
    /// and the character-management screen's close-dialog notice reads exactly
    /// this key before its case-insensitive compare.
    pub const DIALOG_TEXT_INPUT_ANSWER_TEXT: u32 = 0x9C;
    /// Input — **the rows of a menu dialog's drop-down**, an `Array` of `StringInfo`.
    ///
    /// **This is where a dialog menu's contents come from.**
    /// The menu dialog's `set_data` takes the array's
    /// length and walks it, and
    /// for each entry asks the menu for its item count and inserts a text item —
    /// one row per array element.
    ///
    /// Without these rows a `Menu` dialog's answer can never become non-`-1`.
    pub const DIALOG_MENU_ITEMS: u32 = 0xA0;
    /// Input — caption of the menu dialog's single button (child `0x1E`).
    /// Read by its `set_data` under hash key `0xA2`.
    pub const DIALOG_MENU_BUTTON: u32 = 0xA2;
    /// **in and out** — the menu dialog's selected index.
    ///
    /// Out: the menu dialog's message handler writes the selected menu index under it, and
    /// its cancel writes a literal **-1**.
    ///
    /// In: `set_data`'s own `0xA4` arm reads it as an integer and runs
    /// selects item `n` of the dialog's menu — **the initial
    /// selection and the answer are the same key**, which is how a caller pre-selects a row and
    /// reads the player's choice back out of one collection.
    pub const DIALOG_MENU_ANSWER: u32 = 0xA4;
    /// Input — caption of the confirmation menu dialog's accept button (child `0x22`).
    /// Read by its `set_data` under hash key `0xA8`.
    ///
    pub const DIALOG_CONFIRM_MENU_ACCEPT_CAPTION: u32 = 0xA8;
    /// Input — caption of its cancel button (child `0x23`), hash key `0xA9`.
    ///
    pub const DIALOG_CONFIRM_MENU_CANCEL_CAPTION: u32 = 0xA9;
    /// Input — **the rows of a confirmation menu dialog's drop-down**, as
    /// [`DIALOG_MENU_ITEMS`] is for a menu dialog,
    /// hash key `0xA6`, read only when the dialog has a menu.
    pub const DIALOG_CONFIRM_MENU_ITEMS: u32 = 0xA6;
    /// **in and out** — the confirmation menu dialog's selected index, or **-1** when it was
    /// cancelled: the accept arm alone asks the menu for its selected index, the answer having
    /// been primed with `-1`, and its cancel writes
    /// the literal -1. `set_data` reads the same key as the **initial**
    /// selection, exactly as [`DIALOG_MENU_ANSWER`] does.
    pub const DIALOG_CONFIRM_MENU_ANSWER: u32 = 0xAB;

    /// Input — modal: block clicks behind the dialog.
    pub const DIALOG_MODAL: u32 = 0xAC;
    /// Input — **the queue id**, not a flag. Default **2**; **1** is the all-at-once list.
    ///
    /// **It is not "1 = jump the queue".** Read as a bool, it would leave
    /// [`crate::dialog::DialogController`] hard-coding every dialog onto queue 2, never able to
    /// put one on the non-queued list at all. In retail the make-dialog seeds the
    /// queue slot with `2`; the `0xC3` probe's hit arm fetches
    /// the value **into that same slot**; it then compares that slot against 1, and only the
    /// equal arm creates the dialog on queue 1
    /// immediately. Every other value falls through and is used verbatim as the key into
    /// the current-dialog and dialog-queue tables.
    ///
    /// The two readings agree on the values that occur — `1` shows at once, anything else
    /// queues — and disagree on everything else: a `0xC3` of **3** is a *third queue*, not "do
    /// not jump". None of the character-management screen's five dialog builders sets it, which
    /// is why all five share queue **2** and why an error message raised while the delete warning
    /// is up **queues behind it** in retail.
    pub const DIALOG_QUEUE_ID: u32 = 0xC3;

    /// Input — countdown text template (child element 0x3E).
    pub const DIALOG_COUNTDOWN_TEXT: u32 = 0xC5;
    /// Input — timeout in seconds.
    pub const DIALOG_TIMEOUT: u32 = 0xC6;
}

/// Attribute [`attr::UI_OBJECT_MODE`] (`0xCD`) — **a four-valued enum, not a bool**.
///
/// Reading it as a should-own-object bool is half
/// of the truth: the element's attribute setter's `0xCD` arm does pass `value != 0` to
/// the should-own-object setter, so the *bool* is real — but it is the
/// collapse of an enum whose other three values choose **where the element's UI surface gets its
/// size**, and one of which also turns tiling on. Nothing shipped exercises the two middle values;
/// the census is asserted as a literal in the client's UI-object-mode test so a future layout
/// using one fails loudly instead of silently taking the bool's arm.
///
/// # The UI-object construction, which is where the four values differ
///
/// A clear should-own-object flag creates no surface. Otherwise the element reads enum attribute
/// `0xCD`, defaulting to mode 3. Modes 0 and 3 use the element size; modes 1 and 2 use the current
/// state-description size, except that color pickers are forced to mode 3. Mode 2 then replaces
/// that size with the graphic's size when both dimensions are powers of two, creates the surface,
/// and enables tiling. A non-power-of-two graphic falls back to mode 3.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum UiObjectMode {
    /// **0 — the element owns no render object at all.** The should-own-object setter writes flag
    /// bit 14 from its argument and then, when that argument is true, re-reads `0xCD` and
    /// overwrites the bit with `(value != 0)`, so an authored
    /// `0` clears the should-own-object flag **even where the caller asked for true** — the manager's
    /// root-element creation asks for should-own-object `true`
    /// and a `0xCD = 0` root still ends up without one. The UI-object factory then returns
    /// false and the element composes into the nearest owning ancestor's surface,
    /// which the current-UI-object-mode walk finds by walking up the parents. The nine shipped
    /// sites are all of this kind.
    NoObject,
    /// **1 — its own surface, sized from the state description.** The UI-object factory takes
    /// the current state description's width and height, falling back to the width and height
    /// in the element's own description when there is no state description or either of its
    /// dimensions is zero. Not authored
    /// by any shipped element.
    StateSize,
    /// **2 — its own surface, sized from the picture, and it tiles.** Same state-description path
    /// as `1`, then overwritten with the width and height of the element's graphic, and this
    /// is the *only* value that turns tiling on. Not
    /// authored by any shipped element, which is why tiling is unreachable in retail and why
    /// the renderer's `TEXADDRESS_WRAP` arm is inert there.
    /// **That is not a defect**: what a player sees tiling is
    /// handing the picture to the CPU rasteriser's modulo grid.
    TiledGraphic,
    /// **3 — its own surface, sized from the element's own box, and kept in step with it.** The
    /// value the current-UI-object-mode walk defaults to, the value the UI-object factory falls back to when the
    /// attribute is absent or a color-picker element is in the way, and the only value for
    /// which the object update changes physical size alongside
    /// virtual-screen position — see [`UiObjectMode::resizes_its_surface`]. 58 shipped
    /// sites.
    #[default]
    ElementSize,
}

impl UiObjectMode {
    /// The attribute's four authored values, in order.
    pub const ALL: [Self; 4] = [
        Self::NoObject,
        Self::StateSize,
        Self::TiledGraphic,
        Self::ElementSize,
    ];
    /// The stored enum value. `3` is also what an **absent** attribute means, at three separate
    /// sites: the UI-object construction's fallback, the current-mode walk's caller-supplied
    /// initial value, and the attribute setter's own seed of `3`.
    #[must_use]
    pub const fn value(self) -> u32 {
        match self {
            Self::NoObject => 0,
            Self::StateSize => 1,
            Self::TiledGraphic => 2,
            Self::ElementSize => 3,
        }
    }

    /// Decode an authored value. Anything outside `0..=3` is **not** silently folded: retail's own
    /// readers compare against `1`, `2` and `3` explicitly and leave anything else on the
    /// element-size arm, so a fifth value would be `ElementSize` in the UI-object factory and its own
    /// number in the current-UI-object-mode walk. Returning `None` keeps that disagreement visible to the
    /// caller instead of inventing a fifth arm here.
    #[must_use]
    pub const fn from_value(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::NoObject),
            1 => Some(Self::StateSize),
            2 => Some(Self::TiledGraphic),
            3 => Some(Self::ElementSize),
            _ => None,
        }
    }

    /// The should-own-object flag (flag bit 14) — the bool this attribute was modelled as.
    /// It is `mode != 0` and nothing more, which is exactly what the attribute setter
    /// computes.
    #[must_use]
    pub const fn owns_object(self) -> bool {
        self.value() != 0
    }

    /// The tile flag — the surface setter's second argument, which the UI-object construction
    /// computes as "the mode is exactly 2". **Mode 2 specifically**,
    /// not "any non-zero mode".
    #[must_use]
    pub const fn tiles(self) -> bool {
        matches!(self, Self::TiledGraphic)
    }

    /// Whether a resize moves the element's **physical** surface size as
    /// well as its virtual one.
    ///
    /// An element without an owned object changes neither size. For an owner, mode 3 updates both
    /// physical and virtual size; every other mode updates only the virtual rectangle.
    ///
    /// This is why modes 1 and 2 are the only way a shipped element could ever reach the `LINEAR`
    /// sampler: a resize would move the virtual size and leave the physical one, and
    /// the surface transform update compares exactly those two. Mode 0 owns no
    /// object, so it has no physical size to diverge; mode 3 moves both.
    ///
    #[must_use]
    pub const fn resizes_its_surface(self) -> bool {
        matches!(self, Self::ElementSize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(pairs: &[(u32, i32)]) -> PropertyCollection {
        let mut p = PropertyCollection::new();
        for (id, v) in pairs {
            p.set(*id, PropertyValue::Integer(*v));
        }
        p
    }

    /// Oracle: three collections are merged in increasing priority.
    #[test]
    fn instance_properties_beat_state_which_beats_element() {
        let elem = c(&[(1, 10), (2, 20)]);
        let state = c(&[(2, 21), (3, 30)]);
        let inst = c(&[(3, 31), (4, 40)]);
        let m = merge_three(&elem, Some(&state), &inst);
        assert_eq!(m.get_int(1), Some(10));
        assert_eq!(m.get_int(2), Some(21));
        assert_eq!(m.get_int(3), Some(31));
        assert_eq!(m.get_int(4), Some(40));
    }

    /// changed, added, and
    /// "disappeared, with a value-less `BaseProperty`".
    #[test]
    fn set_state_diff_reports_changed_added_and_disappeared() {
        let old = c(&[(1, 10), (2, 20), (3, 30)]);
        let new = c(&[(2, 20), (3, 33), (4, 40)]);
        let d = state_diff(&old, &new);
        assert_eq!(
            d,
            vec![
                // 1 disappeared -> value-less
                AttrChange { id: 1, value: None },
                // 2 unchanged -> not reported at all
                AttrChange {
                    id: 3,
                    value: Some(PropertyValue::Integer(33))
                },
                AttrChange {
                    id: 4,
                    value: Some(PropertyValue::Integer(40))
                },
            ]
        );
    }

    /// Oracle: the state description's incorporation calls
    /// Properties overwrite existing rows with the same id.
    #[test]
    fn update_from_overwrites_by_id_and_keeps_the_rest() {
        let mut base = c(&[(1, 1), (2, 2)]);
        base.update_from(&c(&[(2, 99), (5, 5)]));
        assert_eq!(base.get_int(1), Some(1));
        assert_eq!(base.get_int(2), Some(99));
        assert_eq!(base.get_int(5), Some(5));
    }

    /// Oracle: each typed getter dynamically casts the stored `BasePropertyValue` and yields
    /// nothing on a type mismatch.
    #[test]
    fn a_typed_accessor_refuses_a_value_of_the_wrong_type() {
        let mut p = PropertyCollection::new();
        p.set(attr::HIDE, PropertyValue::Integer(1));
        assert_eq!(p.get_bool(attr::HIDE), None);
        p.set(attr::HIDE, PropertyValue::Bool(true));
        assert_eq!(p.get_bool(attr::HIDE), Some(true));
    }
}
