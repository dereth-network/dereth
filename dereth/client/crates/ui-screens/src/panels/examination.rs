//! `ExaminationPanel` — the identify panel.
//!
//! It is the floating examination window (`<EXAM>`, element `0x100005F7`, type `0x1000004C`).
//!
//! # The chain an identify goes through
//!
//! Identifying an object is a **chain of three**, and only the middle link is a panel arm:
//!
//! | link | where it lives |
//! |---|---|
//! | the request — `Item_Appraise` `0x00C8` | `on_target_mode_button`'s `EXAMINE_BUTTON` arm emits `UiRequest::Examine`, and `interaction.rs` calls attempt_appraise, which sends it |
//! | the subject — the awaited appraisal id | written by the examine-object notice ([`ExaminationPanel::examine_object`]) |
//! | the reply — `0x00C9 Item_SetAppraiseInfo` | decoded and stored by the model, raising `Notice::AppraisalReady` |
//! | the panel — `ExaminationPanel` | this module; `<EXAM>` is in `HUD_START_VISIBILITY` as a window the examine call shows |
//!
//! The use button's whole job is one request; the identify button's request is only the start,
//! and everything else here happens when the answer comes back.
//!
//! # The one arm that decides whether the panel opens
//!
//! The examination panel's appraise-info write returns at once for id zero, or for an id that is
//! neither the awaited appraisal id nor the current one. A reply for the awaited id is *new*: the
//! client clears the awaited id, drops its busy count and makes this the current id. It then picks
//! the pane — a creature profile carrying string `5` or int `0x105` is the character pane, any
//! other creature the creature pane, anything else the item pane — and notes whether the object
//! differs from the one that pane last showed. With no live client object for the id it returns.
//! The title is string `0x34`, or else the object's name, with its stack size; the pane is
//! initialized with the object and given the profile; it becomes the active pane if it was not;
//! and only a *new* reply shows the window.
//!
//! Two things fall out of it that a plainer reading would have got wrong:
//!
//! * **The panel opens only for the appraisal it was *waiting* for.** A reply that matches
//!   the current appraisal id — which is what the client's 0.75-second combat re-poll
//!   produces — refills the panel and does **not** re-show it, so a panel the player closed stays
//!   closed while the poll runs.
//! * **A reply for an object with no live client object does nothing at all**, not
//!   even the title: the live-object check is above every write.
//!
//! # The first two blocks
//!
//! The item-examine panel's appraise-info write is **twenty-five** appraisal blocks, each
//! with its own formatting table (see the formatting-tables section below). The first two are the
//! two it reaches before any table:
//!
//! * The value block reads int property `0x13` and emits `"Value: " + commas` or `"Value: ???"`.
//! * The burden block reads int property `5` and emits `"Burden: " + commas` or
//!   `"Burden: Unknown"`.
//!
//! **Both of those run only because the shipped layout has no separate value or burden element.**
//! The item pane's constructor looks for `0x10000138` (a value element) and `0x10000139` (a
//! burden element) under `0x1000012E`, and **neither is in `classic_gameplay`** (measured: the
//! base field's children are `0x10000135`, `0x10000137`, `0x1000013C`, `0x100005F8`,
//! `0x1000013E`, `0x1000013F` and `0x1000013D`). The appraise-info write's
//! value-element-or-description fork therefore takes the *second* branch in retail too, and the
//! value and burden go into the description block.
//!
//! The same is true of the item's icon. The pane would draw the examined object's icon in
//! `0x1000013A`, but only the docked examination panel the floating window replaced (`0x1000012C`
//! in layout `0x2100001C`) has that element, so the item pane draws no icon of the item.

use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};

use crate::view::{AppraisalView, GameView};

/// `<EXAM>` — `FloatingExamination`, the window the appraise-info write's last line shows.
pub const WINDOW: ElementId = ElementId(0x1000_05F7);
/// The window's close control.
///
/// The floaty examination panel hides its window when message 1 comes from element
/// `0x100005F3`, then forwards to the base examination panel's element-message handler.
/// Other messages forward without hiding the window.
///
/// Message `1` is [`dereth_ui::msg::element::id::BUTTON_CLICKED`], which is what a
/// `Button` raises on release — and the shipped `classic_gameplay` gives `0x100005F3`
/// element type **1**, a 13x13 box at (284, 8)-(297, 21), the X in the frame's top-right corner.
/// Read out of the live tree, not assumed. \[verified\]
///
/// The hide is on the **window**, not on the button's parent chain: it is the floating
/// examination window itself.
pub const CLOSE_BUTTON: ElementId = ElementId(0x1000_05F3);
/// The displayed-name text — the title line.
pub const DISPLAYED_NAME_TEXT: ElementId = ElementId(0x1000_012D);
/// The item pane's base field, from its constructor.
pub const ITEM_BASE: ElementId = ElementId(0x1000_012E);
/// The item description text, the block every appraisal block appends to.
pub const ITEM_DISPLAY_TEXT: ElementId = ElementId(0x1000_013C);
/// Shared by the creature
/// and character sub-UIs, which differ only in the sub-panel each shows.
pub const CREATURE_BASE: ElementId = ElementId(0x1000_0140);
/// The creature info sub-panel.
pub const CREATURE_SUB_PANEL: ElementId = ElementId(0x1000_014D);
/// The **3D portrait of the appraised
/// creature**, a `Viewport` (engine class `0x0D`).
///
/// Two things about this binding are unlike every other element in this panel, and both are in
/// the constructor:
///
/// * it is looked up off the pane's **parent element** — the `<EXAM>` window — not off the creature
///   base field that `0x1000014C`, `0x1000014D`, `0x1000014F`, `0x10000149` and `0x10000335` use;
/// * the result is cast to viewport type `0xD`, so a layout whose `0x10000148` were some other
///   class would leave the paper doll unbound and the panel would simply draw no portrait.
///
/// In the shipped layout it is a child of `CREATURE_BASE` with the **same box as
/// [`CREATURE_STAT_LIST`]** — retail draws the model behind the numbers, not beside them.
pub const CREATURE_PAPER_DOLL: ElementId = ElementId(0x1000_0148);
/// The `<Inscribe here>` edit box.
pub const ITEM_INSCRIPTION_TEXT: ElementId = ElementId(0x1000_013E);
/// The inscription signature text — the `--<scribe>` line under it.
pub const ITEM_INSCRIPTION_SIGNATURE_TEXT: ElementId = ElementId(0x1000_013F);
/// The inscription background — the panel the inscription-editable-state write
/// shows and hides with the box.
pub const ITEM_INSCRIPTION_BACKGROUND: ElementId = ElementId(0x1000_0137);
/// **The state that centres the placeholder, and it is authored in `client_portal.dat`.**
///
/// The inscription write first puts the inscription box into this state, and the "this item
/// already has an inscription" leg takes it straight off again with state `1`. The inscription
/// gaining-focus handling also sets 1, and the losing-focus handling puts `0x10000050`
/// back when the box is left empty.
///
/// Element `0x1000013E` ships `0x14 = Enum(2)` / `0x15 = Enum(4)` — near on both axes — on its
/// base state, and state `0x10000050` carries exactly two properties, `0x14 = Enum(1)` and
/// `0x15 = Enum(1)`, which [`dereth_ui::text::glyph::calc_justification`] reads as **centre**. So
/// the whole of "the placeholder is centred and a real inscription is not" is one state change
/// against data the layout already had. \[measured\]
///
/// State **1** is not authored on this element at all; a state change to a state with no
/// description records 0, which restores the base's near/near.
pub const INSCRIPTION_PLACEHOLDER_STATE: StateId = StateId(0x1000_0050);
/// The state the client asks for whenever the box is showing something other than the invitation.
pub const INSCRIPTION_NORMAL_STATE: StateId = StateId(1);
/// The level value text. Left unwritten, retail's *Character Level 1* would read as the words
/// *Character Level* and a gap.
pub const LEVEL_VALUE_TEXT: ElementId = ElementId(0x1000_014C);
/// The creature display name — the creature pane's first line, e.g. `Golem`.
pub const CREATURE_DISPLAY_NAME: ElementId = ElementId(0x1000_014E);
/// The list box receives nine information regions: six attributes followed by three vitals.
pub const CREATURE_STAT_LIST: ElementId = ElementId(0x1000_0149);

/// The pose, the light and the camera of the identify
/// window's 3D portrait.
///
/// Every constant here is the client's own initialization value, including the camera
/// argument that is not a look-at target.
pub mod portrait {
    use dereth_primitives::Vec3;

    /// The light's direction, `(0.3, 1.9, 0.65)`, set by initialization. The same direction the paper doll uses. \[verified\]
    pub const LIGHT_DIRECTION: (f32, f32, f32) = (0.3, 1.9, 0.65);

    /// The light's intensity, `2.0`.
    /// \[verified\]
    pub const LIGHT_INTENSITY: f32 = 2.0;

    /// The light's type: `1`, which is `DISTANT_LIGHT`. \[verified\]
    pub const LIGHT_TYPE: u32 = 1;

    /// The heading the portrait clone is set to, `191.3679` degrees.
    /// The same three-quarter pose the paper doll takes, and the reason the creature is not seen
    /// edge-on. \[verified\]
    pub const HEADING_DEGREES: f32 = 191.3679;

    /// The camera-fit factor, `1.2071068`. \[verified\]
    ///
    /// It is `cot(22.5 deg) / 2`, and the viewport's field of view is
    /// `0.7853982` radians (45 degrees). So [`camera_position`] is the textbook "back off
    /// far enough to fit this extent in a 45-degree vertical field of view".
    pub const CAMERA_FIT: f32 = 1.2071068;

    /// The camera write's **second** argument, explicitly zeroed before the call.
    ///
    /// It is not a look-at target. The viewport's camera write hands it to
    /// the creature mode's camera-direction write, which is `euler_set_rotate(0,0,0)` followed
    /// by `rotate(v)` — so `(0,0,0)` leaves the camera's orientation at **identity**,
    /// looking straight down `+Y`. A rebuild that read it as a look-at point would get the same
    /// picture here only by accident.
    pub const CAMERA_DIRECTION: (f32, f32, f32) = (0.0, 0.0, 0.0);

    /// Initialization's camera placement, given the object's bounding box and the
    /// **viewport element's own** width and height: with the box extents `dx`, `dy`, `dz`, when
    /// `dz/dx < h/w` then `dz` becomes `dx * (h/w)`, and the camera sits at the box centre moved
    /// back along `-Y` by `dz * 1.2071068 + dy * 0.5`.
    ///
    /// The clamp is the piece worth naming: when the element is taller relative to its width than
    /// the object is deep relative to its width, the *fitted* extent becomes the width scaled by
    /// the element's aspect, so a wide flat creature in a tall box is pushed back far enough for
    /// its width to fit rather than its depth. The comparison is strict — the multiply runs
    /// only when `dz/dx < h/w` — and a
    /// degenerate box that makes `dz/dx` NaN takes the *unordered* branch and keeps `dz`, which is
    /// what Rust's `NaN < aspect == false` does here.
    #[must_use]
    pub fn camera_position(min: Vec3, max: Vec3, width: i32, height: i32) -> Vec3 {
        let dx = max.x - min.x;
        let dy = max.y - min.y;
        let mut dz = max.z - min.z;
        #[allow(clippy::cast_precision_loss)]
        // LINT-OK: element boxes are a few hundred pixels; the client's own int-to-float load is
        // the same conversion.
        let aspect = height as f32 / width as f32;
        if dz / dx < aspect {
            dz = dx * aspect;
        }
        // The midpoint, component by component.
        let c = Vec3::new(
            (max.x + min.x) * 0.5,
            (max.y + min.y) * 0.5,
            (max.z + min.z) * 0.5,
        );
        Vec3::new(c.x, c.y - (dz * CAMERA_FIT + dy * 0.5), c.z)
    }
}
/// The client's three row children, shared with `AttributesPanel`'s rows
/// (`panels::attributes`' `row::LABEL` / `row::VALUE`): `0x10000129` icon, `0x1000012A` label,
/// `0x1000012B` value.
pub const ROW_LABEL: ElementId = ElementId(0x1000_012A);
/// See [`ROW_LABEL`].
pub const ROW_VALUE: ElementId = ElementId(0x1000_012B);
/// The character info sub-panel.
pub const CHARACTER_SUB_PANEL: ElementId = ElementId(0x1000_014F);
/// The extra-info list, bound from the creature base as a list box (type `5`): the list
/// box every row lands in.
///
/// The shipped layout has it at `(0, 278)-(291, 364)`, a `ListBox` whose template row 0
/// carries the same [`ROW_LABEL`] / [`ROW_VALUE`] pair the stat list uses.
pub const MISC_LIST: ElementId = ElementId(0x1000_0335);
/// The heritage text.
pub const CHAR_HERITAGE_TEXT: ElementId = ElementId(0x1000_0150);
/// The profession text.
pub const CHAR_PROFESSION_TEXT: ElementId = ElementId(0x1000_0151);
/// The PK-status text.
pub const CHAR_PK_STATUS_TEXT: ElementId = ElementId(0x1000_0152);
/// The allegiance-name text; the **creature**
/// pane clears the same element by looking it up afresh.
pub const ALLEGIANCE_NAME_TEXT: ElementId = ElementId(0x1000_053A);
/// The spell pane's base field — the third sibling of `<EXAM>`'s three panes.
pub const SPELL_BASE: ElementId = ElementId(0x1000_0153);
/// The spell pane's component list — the element the examination panel's element-message
/// handler compares against as a literal (`0x10000137 + 7 + 0x1EF`).
///
/// This panel does not bind it; the spell pane does. It is named here because
/// [`ExaminationPanel::select_spell_component`] is that arm's body and a literal pinned once is
/// what makes a wrong id falsifiable.
pub const SPELL_COMPONENT_LIST: ElementId = ElementId(0x1000_032D);
/// The message that arm answers: `ListBox`'s selection-changed, **4**.
pub const SPELL_COMPONENT_LIST_MESSAGE: u32 = 4;

/// Which examination pane the appraisal result selects: item, creature, character or spell.
/// The original selector stores four pane pointers; this enum carries the same choice without
/// claiming their native offsets as a Rust layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExamineSubUi {
    Item,
    Creature,
    Char,
    Spell,
}

/// The appraisal formatting rules, shared by every interface that shows an appraisal.
pub use dereth_presentation::appraisal::*;

/// `ExaminationPanel` — the panel, bound off the gameplay screen's root.
///
/// `Default` is the constructor, which is all zeroes
/// **except** the newly-selected-item flag, which starts `true` — so it is written by hand rather
/// than derived.
#[derive(Debug)]
pub struct ExaminationPanel {
    /// `<EXAM>` itself; the visibility writes go to this.
    pub window: Option<ElemHandle>,
    /// [`CLOSE_BUTTON`]. Bound so that a layout without the control is a `None` a test can see,
    /// rather than a click that silently matches nothing.
    close_button: Option<ElemHandle>,
    title: Option<ElemHandle>,
    item_base: Option<ElemHandle>,
    item_display_text: Option<ElemHandle>,
    creature_base: Option<ElemHandle>,
    creature_sub: Option<ElemHandle>,
    char_sub: Option<ElemHandle>,
    spell_base: Option<ElemHandle>,
    /// The spell pane — the fourth sub-UI. `spell_base` is bound by the constructor and
    /// [`Self::set_active_examine_ui`] shows it; this fills it. See
    /// [`crate::panels::spell_examine`].
    pub spell: crate::panels::spell_examine::SpellExamineUi,
    /// The inscription box, its signature line and the background
    /// panel behind them -- the item pane's constructor binds all three.
    inscription_text: Option<ElemHandle>,
    inscription_signature: Option<ElemHandle>,
    inscription_background: Option<ElemHandle>,
    /// The level value text and the creature display name.
    level_value: Option<ElemHandle>,
    creature_name: Option<ElemHandle>,
    /// The nine-row stat list the creature and character panes share.
    stat_list: Option<crate::panels::listbox::ListBoxWidget>,
    /// [`MISC_LIST`] — the extra-info list, the **second** list box of the
    /// creature base and the one every [`MiscRow`] lands in.
    misc_list: Option<crate::panels::listbox::ListBoxWidget>,
    /// The character pane's four text elements — [`CHAR_HERITAGE_TEXT`],
    /// [`CHAR_PROFESSION_TEXT`], [`CHAR_PK_STATUS_TEXT`] and [`ALLEGIANCE_NAME_TEXT`].
    char_heritage: Option<ElemHandle>,
    char_profession: Option<ElemHandle>,
    char_pk_status: Option<ElemHandle>,
    allegiance_name: Option<ElemHandle>,

    /// The awaited appraisal id — the object the
    /// examine request stored and has not yet been answered about.
    pub awaiting: Option<dereth_primitives::ObjectId>,
    /// The current appraisal id — the object the panel is currently showing.
    pub current: Option<dereth_primitives::ObjectId>,
    /// The active pane.
    pub active: Option<ExamineSubUi>,
    /// The object id of the **active** pane, which is what the appraise-info write's
    /// new-object flag compares against.
    sub_object: Option<dereth_primitives::ObjectId>,
    /// The paper doll — the viewport element that hosts the appraised
    /// creature's 3D portrait. `None` when the layout has no `0x10000148`, which is
    /// the client's own null check and makes the portrait absent rather than the panel broken.
    pub creature_viewport: Option<ElemHandle>,
    /// The title last written, so a test can read it back without the glyph list.
    pub title_text: Option<String>,
    /// The item description text's content, likewise.
    pub item_text: Option<String>,
    /// What the inscription write put in the edit box, or `None` for an object
    /// that is not inscribable at all -- in which case the client takes the box away.
    pub inscription: Option<String>,
    /// The `--<scribe>` signature line, empty while the box invites an inscription.
    pub inscription_signature_text: String,
    /// The scribe name — string `8`, or the player's own name once he
    /// has written on it and the box has lost focus.
    ///
    /// It is a **cache, not a view of the profile**: the gaining-focus handler
    /// tests it against `""` to decide whether the box is showing the invitation, the
    /// editable-state write tests it against `""` and against the player's
    /// name to decide who may write, and the losing-focus handler tests it to
    /// decide whether clearing the box is worth a message.
    pub scribe_name: String,
    /// The inscription — string `7`, replaced by whatever was typed
    /// once the box loses focus. The `!=` against this field is what
    /// stops a second focus loss re-sending the same inscription.
    pub inscription_value: String,
    /// The player's own name, refreshed by [`Self::update`].
    ///
    /// The client reads the player through a global; this panel is handed a [`GameView`] only on
    /// the per-frame pull, and the three inscription handlers run from an element message that has
    /// no view. So the name is latched on the pull, which is the same value at the same moment.
    pub player_name: String,
    /// The client's answer for the object now shown — whether the
    /// two `BoolProperty`s `0x16` and `0x27` went true.
    pub inscription_editable: bool,
    /// The live object facts the mouse-press handler reads after an
    /// unsigned locked inscription background is pressed: `(inscribable, location)`, or
    /// `None` when the object is not known.
    inscription_mouse_facts: Option<(bool, u32)>,
    /// How many `0xBF Writing_SetInscription` requests this panel has emitted. A denominator, so
    /// "the commit edge never fired" and "it fired and the guards refused" are different answers.
    pub inscriptions_sent: u32,
    /// The basic creature examine panel's level value text write's answer.
    pub level_text: Option<String>,
    /// The appraisal system's creature display name read's answer.
    pub creature_name_text: Option<String>,
    /// The nine `(label, value)` rows `creature_rows` produced, in drawn order, so a test can read
    /// the pane back without walking the list box's children.
    pub creature_row_text: Vec<(String, String)>,
    /// The colour index each of those rows' **value** cells was drawn in, in the same order —
    /// [`creature_row_colors`]' answer for the reply now showing.
    ///
    /// Kept beside [`Self::creature_row_text`] rather than folded into it because that field is
    /// read by name outside this crate; the two are always the same length.
    pub creature_row_colors: Vec<u8>,
    /// How many of those actually reached a row **element**. A denominator: "the pane computed
    /// nine rows" and "nine rows are on screen" are different facts, and a layout whose list box
    /// carries no row template would otherwise fail silently.
    pub creature_rows_drawn: u32,
    /// The [`MiscRow`]s [`char_misc_rows`] or [`creature_misc_rows`] produced for the reply now
    /// showing, in drawn order.
    pub misc_row_text: Vec<MiscRow>,
    /// How many of those reached a row **element** of [`MISC_LIST`] — the same denominator
    /// [`Self::creature_rows_drawn`] is, and for the same reason: "the pane computed the rows"
    /// and "the rows are on screen" are different facts.
    pub misc_rows_drawn: u32,
    /// The heritage text's content — the gender-and-heritage display read's answer, or `None` when
    /// the client's `if` left the element alone.
    pub heritage_text: Option<String>,
    /// The profession text's content — the character title, else the template.
    pub profession_text: Option<String>,
    /// The PK-status text's content, one of [`pk_status_text`]'s three literals. `None`
    /// only when the pane has never been a character pane, because the client's guard is on
    /// the current object being known rather than on the profile.
    pub pk_status_text: Option<String>,
    /// The allegiance-name text's content. The allegiance write runs on **every**
    /// character reply, so this is `None` for a player with no allegiance rank and not a stale
    /// name from the object before.
    pub allegiance_name_text: Option<String>,
    /// How many replies this panel has taken — a denominator (the stated testability rule, "assert the
    /// denominator"), so "the panel did not open" and "no reply reached it" are different answers.
    pub replies_applied: u32,
    /// How many of those were the *awaited* one, i.e. how many opened the window.
    pub opened: u32,
    /// `dereth_client_model::AppraisalCache::delivery` for the reply last applied, so the pull in
    /// [`Self::update`] can tell a fresh reply from the same one being offered again. The client
    /// has no such field because it is called *by* the reply; see that method.
    last_delivery: u64,
    /// `dereth_client_model::AppraisalCache::examine_serial` last seen, so the pull in [`Self::update`] can
    /// tell a **new** examine-object notice from the same one being re-offered every
    /// frame; see [`crate::view::GameView::examine_request`].
    last_examine_serial: u64,
    /// How many notices [`Self::update`]'s pull has turned into a
    /// examine-object notice — a denominator, so "the cursor route never asked" and "it asked
    /// and the panel refused" are different answers.
    pub examines_pulled: u32,
    /// How many examine-spell notices [`Self::update`]'s pull has taken.
    /// A denominator, so "the right-click never produced a notice" and "it did
    /// and the pane refused the spell" are different answers.
    pub spell_examines_pulled: u32,
    /// How many appraise-`0` cancels [`Self::examine_spell`] has raised.
    /// A denominator, so "the spell examine never cancelled" and "it cancelled
    /// when the client says it should have jumped over the call" are different answers.
    pub appraisals_cancelled: u32,
    /// How many times the window has been hidden — the close half's denominator, so "it never
    /// closed" and "it closed and something re-opened it" are different answers.
    pub closed: u32,
    /// The newly-selected-item flag, initialized to `true`. It gates the
    /// selection-change reaction while a spell component changes the selected object.
    ///
    /// It has exactly **one** clearer, immediately before the call it guards:
    ///
    /// Resolve the clicked component to an object id; zero does nothing. Otherwise clear
    /// the flag, select that object with the second argument zero, then restore the flag.
    ///
    /// So it is not a mode and never a latch: it is a **re-entrancy bracket** around one
    /// synchronous call. The selected object write broadcasts
    /// the selection-changed notice, *inside* the bracket, and
    /// the panel's selection-changed receiver reads the flag, which is this same byte. The two
    /// writes sit next to each other.
    ///
    /// **The bracket alone would not work here, and that is the one thing this build must do
    /// differently.** The client's reader is *pushed* inside the assignment; [`Self::update`]'s is
    /// a *pull* on the next frame, by which time an eager restore would have re-opened the gate.
    /// So [`Self::select_spell_component`] clears it and **[`Self::update`] performs the restore**,
    /// on the frame that absorbs the change the clear was hiding — the same one write,
    /// moved to the far side of the same broadcast.
    pub examine_newly_selected_item: bool,
    /// Selection changes [`Self::update`] absorbed with the guard down — i.e. the
    /// ones this panel made itself through [`Self::select_spell_component`], and which therefore
    /// did **not** produce a re-examine.
    ///
    /// A denominator: "the spell pane clicked a component and the panel did not re-examine" and
    /// "the spell pane never clicked anything" are otherwise the same observation.
    pub self_selections_absorbed: u32,

    /// The SCID of the component row the formula list last reported selected,
    /// waiting for [`Self::update`]'s pull to resolve it against the component tracker.
    ///
    /// The client resolves inside the message, because it reaches
    /// the component tracker through a global. Here the tracker is behind the
    /// [`GameView`] seam and `on_element_message` carries no view, so the row is latched and the
    /// lookup happens on the next pull — one frame later, exactly as the `examine_request`,
    /// `selection_changed` and examine-spell producers already are. See
    /// [`Self::spell_component_selected`].
    pending_component_scid: Option<u32>,
    /// How many component rows the formula list has reported selected — the denominator that
    /// separates "the click never arrived" from "it arrived and resolved to nothing".
    pub component_clicks: u32,
    /// How many of those resolved to an object and reached the selected-object write.
    pub component_selections: u32,
    /// The update-spell-components notice's serial as this panel last saw it.
    /// The pull half of the panel's update-spell-components receiver.
    /// See [`crate::view::GameView::component_serial`].
    last_component_serial: u64,
    /// How many update-spell-components notices [`Self::update`]'s pull has turned into a
    /// re-mark of the spell pane's formula rows.
    pub component_notices_pulled: u32,
    /// The world selection as this panel last saw it. The client is *pushed*
    /// the selection-changed notice; this build has no such notice, so [`Self::update`] pulls
    /// [`GameView::selected_object`] and raises the handler on a change. Same shape as the
    /// `examine_request` pull above, and the same one difference: the client's handler
    /// runs on the notice, this one on the frame that first sees a different id.
    last_selected: Option<Option<dereth_primitives::ObjectId>>,
}

impl Default for ExaminationPanel {
    fn default() -> Self {
        Self {
            window: None,
            close_button: None,
            title: None,
            item_base: None,
            item_display_text: None,
            creature_base: None,
            creature_viewport: None,
            creature_sub: None,
            char_sub: None,
            spell_base: None,
            spell: crate::panels::spell_examine::SpellExamineUi::default(),
            inscription_text: None,
            inscription_signature: None,
            inscription_background: None,
            level_value: None,
            creature_name: None,
            stat_list: None,
            misc_list: None,
            char_heritage: None,
            char_profession: None,
            char_pk_status: None,
            allegiance_name: None,
            misc_row_text: Vec::new(),
            misc_rows_drawn: 0,
            heritage_text: None,
            profession_text: None,
            pk_status_text: None,
            allegiance_name_text: None,
            awaiting: None,
            current: None,
            active: None,
            sub_object: None,
            title_text: None,
            item_text: None,
            inscription: None,
            inscription_signature_text: String::new(),
            // The item-examine panel initializes all three text fields as empty.
            scribe_name: String::new(),
            inscription_value: String::new(),
            player_name: String::new(),
            inscription_editable: false,
            inscription_mouse_facts: None,
            inscriptions_sent: 0,
            level_text: None,
            creature_name_text: None,
            creature_row_text: Vec::new(),
            creature_row_colors: Vec::new(),
            creature_rows_drawn: 0,
            replies_applied: 0,
            opened: 0,
            last_delivery: 0,
            last_examine_serial: 0,
            examines_pulled: 0,
            spell_examines_pulled: 0,
            appraisals_cancelled: 0,
            closed: 0,
            // The examination panel's constructor's last line.
            examine_newly_selected_item: true,
            self_selections_absorbed: 0,
            pending_component_scid: None,
            component_clicks: 0,
            component_selections: 0,
            last_component_serial: 0,
            component_notices_pulled: 0,
            last_selected: None,
        }
    }
}

impl ExaminationPanel {
    /// The examination panel's initialization plus the three sub-UI constructors' bindings, off the
    /// gameplay screen root — `<EXAM>` is a window of that root, not a page of the panel stack.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.window = ui.get_child_recursive(root, WINDOW);
        let Some(w) = self.window else { return };
        self.close_button = ui.get_child_recursive(w, CLOSE_BUTTON);
        self.title = ui.get_child_recursive(w, DISPLAYED_NAME_TEXT);
        self.item_base = ui.get_child_recursive(w, ITEM_BASE);
        self.item_display_text = self
            .item_base
            .and_then(|b| ui.get_child_recursive(b, ITEM_DISPLAY_TEXT));
        self.creature_base = ui.get_child_recursive(w, CREATURE_BASE);
        // The paper doll: off the window, recursively, exactly as the
        // client does -- the one binding in this constructor that does not start at the
        // creature base field.
        self.creature_viewport = ui.get_child_recursive(w, CREATURE_PAPER_DOLL);
        self.creature_sub = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CREATURE_SUB_PANEL));
        self.char_sub = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CHARACTER_SUB_PANEL));
        self.spell_base = ui.get_child_recursive(w, SPELL_BASE);
        // The fourth sub-UI's constructor, which
        // binds its own seven children off the same `0x10000153`.
        self.spell.post_init(ui, w);
        // The inscription trio hangs off the item base field, and the creature pane's
        // level cell, name line and stat list off the creature base -- the same two constructors
        // that bound the rest.
        self.inscription_text = self
            .item_base
            .and_then(|b| ui.get_child_recursive(b, ITEM_INSCRIPTION_TEXT));
        self.inscription_signature = self
            .item_base
            .and_then(|b| ui.get_child_recursive(b, ITEM_INSCRIPTION_SIGNATURE_TEXT));
        self.inscription_background = self
            .item_base
            .and_then(|b| ui.get_child_recursive(b, ITEM_INSCRIPTION_BACKGROUND));
        self.level_value = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, LEVEL_VALUE_TEXT));
        self.creature_name = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CREATURE_DISPLAY_NAME));
        self.stat_list = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CREATURE_STAT_LIST))
            .map(|h| crate::panels::listbox::ListBoxWidget::bind(ui, h));
        // The character pane's constructor binds five more children off the same creature-base
        // field as the stat list: the misc list and four text elements.
        self.misc_list = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, MISC_LIST))
            .map(|h| crate::panels::listbox::ListBoxWidget::bind(ui, h));
        self.char_heritage = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CHAR_HERITAGE_TEXT));
        self.char_profession = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CHAR_PROFESSION_TEXT));
        self.char_pk_status = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, CHAR_PK_STATUS_TEXT));
        self.allegiance_name = self
            .creature_base
            .and_then(|b| ui.get_child_recursive(b, ALLEGIANCE_NAME_TEXT));
        self.awaiting = None;
        self.current = None;
        self.active = None;
        self.sub_object = None;
    }

    /// Whether `post_init` found the window at all.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.window.is_some()
    }

    /// Whether `post_init` found the close control. A layout without it is a `None` here rather
    /// than a click that quietly matches nothing.
    #[must_use]
    pub fn close_control_bound(&self) -> bool {
        self.close_button.is_some()
    }

    /// Whether `<EXAM>` is on screen right now — on the window,
    /// which is what both close paths test first.
    #[must_use]
    pub fn is_open(&self, ui: &UiSystem) -> bool {
        self.window
            .and_then(|w| ui.node(w))
            .is_some_and(|n| n.region.flags.visible)
    }

    /// The floaty examination panel's element-message handler — **the close control.**
    ///
    /// Message 1 from `0x100005F3` hides the window; every message then goes on to the base
    /// examination panel's handler. See [`CLOSE_BUTTON`]. Returns whether this call closed the
    /// window.
    ///
    /// **It does not touch `awaiting` or `current`**, and that is the point: the current appraisal
    /// id stays set, so the client's 0.75-second combat re-poll keeps arriving and keeps taking the
    /// appraise-info write's not-new path — refilling a hidden panel and **not** re-showing it.
    /// Clearing them here would make the next re-poll look like a fresh appraisal and pop the
    /// window straight back open, which is the defect this arm exists to avoid.
    pub fn on_element_message(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        // The inscription arms: focus-changed
        // (`0x2F`) on the edit box `0x1000013E` goes to the losing-focus handler when `p1 == 0`
        // and to the gaining-focus handler otherwise, and a press on the background `0x10000137`
        // goes to the mouse-press handler.
        if m.source_id == ITEM_INSCRIPTION_TEXT
            && m.id == dereth_ui::msg::element::id::FOCUS_CHANGED
        {
            if m.p1 == 0 {
                self.handle_inscription_losing_focus(ui);
            } else {
                self.handle_inscription_gaining_focus(ui);
            }
            return false;
        }
        if m.source_id == ITEM_INSCRIPTION_BACKGROUND
            && m.id == dereth_ui::msg::element::id::MOUSE_PRESS
            && m.p1 == dereth_ui::focus::action::PRIMARY_CLICK
        {
            self.handle_inscription_mouse_press(&mut ui.requests);
            return false;
        }
        // The third arm handles the formula list box reporting a
        // new selection. `SPELL_COMPONENT_LIST_MESSAGE` is `4`, which is
        // the list box's selected-item write broadcasting message 4 with the index and the
        // selected row — so the *gesture* is a **primary** press or a double-click on
        // a row (its mouse-down handler takes actions `7` and `0x0A`; the `0x1C` arm
        // takes `7`), gated on this list's `0x59 ClickSelect`, which `0x1000032D` declares in
        // `0x2100001C`. A secondary press reaches neither site and does nothing here.
        //
        // A cleared selection — index `-1`, handle 0 — falls straight out. The SCID comes
        // off that row's `0x10000010` property; see
        // [`crate::panels::spell_examine::row_component_scid`].
        if m.source_id == SPELL_COMPONENT_LIST && m.id.0 == SPELL_COMPONENT_LIST_MESSAGE {
            if m.p2 != 0 {
                if let Some(scid) = crate::panels::spell_examine::row_component_scid(
                    ui,
                    dereth_ui::ElemHandle::from_raw(m.p2),
                ) {
                    self.component_clicks += 1;
                    self.pending_component_scid = Some(scid);
                }
            }
            return false;
        }
        if m.id != dereth_ui::msg::element::id::BUTTON_CLICKED || m.source_id != CLOSE_BUTTON {
            return false;
        }
        self.hide(ui)
    }

    /// The item-examine panel's inscription mouse presses handling's ordinary-player arms.
    ///
    /// The editable property is checked first and returns silently. A different scribe is named;
    /// a matching scribe on a locked item gets the inventory refusal. An unsigned object then
    /// distinguishes the not-inscribable bit, the missing/zero-location inventory refusal, and
    /// the non-zero-location silent return.
    ///
    /// The client's PSR override remains separate: once
    /// that privilege and its editability branch are projected, the editable first guard will
    /// suppress all three messages.
    fn handle_inscription_mouse_press(&self, requests_out: &mut crate::requests::Outbox) {
        if self.inscription_editable {
            return;
        }
        let text = if !self.scribe_name.is_empty() {
            if self.scribe_name.eq_ignore_ascii_case(&self.player_name) {
                "Item must be in your inventory to inscribe.".to_string()
            } else {
                format!("Only {} can change the inscription", self.scribe_name)
            }
        } else {
            match self.inscription_mouse_facts {
                Some((false, _)) => "This item is not inscribable.".to_string(),
                Some((true, location)) if location != 0 => return,
                Some((true, _)) | None => "Item must be in your inventory to inscribe.".to_string(),
            }
        };
        requests_out.emit(crate::view::UiRequest::DisplayChatText {
            channel: 0x1A,
            text,
            feedback: dereth_client_contract::feedback::Feedback::LOCAL,
        });
    }

    /// The inscription box gaining focus.
    ///
    /// With no scribe name the box goes to state 1 and is cleared. Either way the signature line is
    /// then set to `--` followed by the player's own name.
    ///
    /// So clicking in **empties the box** rather than dropping a caret into the literal
    /// `<Inscribe here>`, takes the centring off with it, and writes `--<your name>` underneath
    /// straight away — before anything has been sent — so the player can see whose signature he is
    /// about to leave. The signature is rewritten on **every** focus gain, not only the empty one.
    fn handle_inscription_gaining_focus(&mut self, ui: &mut UiSystem) {
        if self.scribe_name.is_empty() {
            if let Some(h) = self.inscription_text {
                ui.set_state(h, INSCRIPTION_NORMAL_STATE);
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text("");
                }
            }
            self.inscription = Some(String::new());
        }
        let signature = format!("--{}", self.player_name);
        if let Some(h) = self.inscription_signature {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&signature);
            }
        }
        self.inscription_signature_text = signature;
    }

    /// This is **the commit edge and the only client caller.**
    ///
    /// With no current object it returns. It reads the box's text and sends it as the inscription
    /// when it is worth saying — non-empty, or empty on an item that has a scribe — and differs
    /// from the cached inscription. Then an empty box goes back to the placeholder state and
    /// `<Inscribe here>`, the signature line is cleared and the scribe name forgotten; a non-empty
    /// one makes the player the scribe. Either way the cached inscription becomes the text.
    ///
    /// The two guards are worth naming because neither is obvious from the outside:
    ///
    /// * **an empty box on an unsigned item is not a message.** Opening the panel, clicking in
    ///   (which empties it) and clicking away again would otherwise send an empty inscription for
    ///   every inscribable thing the player ever looked at;
    /// * **the text has to have changed.** The cached inscription is the last thing the box
    ///   committed or was told, so losing focus twice with the same words sends once.
    ///
    /// The tail is what makes an inscription *look* persistent before the shard has answered: the
    /// scribe becomes the player, the cached inscription becomes the text, and the next
    /// inscription write for the same object — the 0.75 s combat re-poll included — draws them.
    fn handle_inscription_losing_focus(&mut self, ui: &mut UiSystem) {
        // The object is the *item* pane's current object; a creature or spell pane
        // has no inscription box at all.
        let Some(object) = self
            .sub_object
            .filter(|_| self.active == Some(ExamineSubUi::Item))
        else {
            return;
        };
        if object.0 == 0 {
            return;
        }
        let text = self
            .inscription_text
            .and_then(|h| ui.text_element_mut(h))
            .map(|t| t.glyphs.inq_text(false))
            .unwrap_or_default();
        let worth_saying = !(text.is_empty() && self.scribe_name.is_empty());
        // **The wart, and it is deliberate: it matches retail.** There is no
        // `text != PLACEHOLDER` here, and retail has none either: on the inscription write's
        // invitation leg the box holds `L"<Inscribe here>"` while the cached inscription is still
        // the client's `""`, so for an item that is inscribable, unsigned and **not** the player's,
        // the non-empty box passes the first test, `"<Inscribe here>" != ""` passes the second, and
        // the client sends the placeholder as an inscription for the new target, and the server
        // accepts it. Adding the missing test would change what goes on the wire.
        if worth_saying && text != self.inscription_value {
            ui.requests.emit(crate::view::UiRequest::SetInscription {
                object,
                text: text.clone(),
            });
            self.inscriptions_sent += 1;
        }
        if text.is_empty() {
            if let Some(h) = self.inscription_text {
                ui.set_state(h, INSCRIPTION_PLACEHOLDER_STATE);
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(PLACEHOLDER);
                }
            }
            if let Some(h) = self.inscription_signature {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text("");
                }
            }
            self.scribe_name.clear();
            self.inscription = Some(PLACEHOLDER.to_string());
            self.inscription_signature_text.clear();
        } else {
            self.scribe_name.clone_from(&self.player_name);
            self.inscription = Some(text.clone());
        }
        self.inscription_value = text;
    }

    /// The client's `0x1000002B` arm — **the third close path, and the
    /// one that makes the examine key a toggle.**
    ///
    /// Look up window `0x100005F7`. If it exists and is visible, hide it and return true
    /// without examining an object. A missing or hidden window takes the examine path.
    ///
    /// **It shares [`Self::hide`] with the close button and with the deselection path**, so all
    /// three increment `closed` and none of them touches `awaiting` or `current` — which is what
    /// keeps the combat re-poll from popping the window straight back open. See [`Self::hide`].
    ///
    /// The *decision* is not here: retail makes it in the action handler, before it reads
    /// the world selection, so this build makes it in
    /// `dereth_client::interaction`'s `0x1000002B` arm from a visibility answer pushed in by `App`.
    /// If it were made here, on the pull, the wire would be wrong — the examine-object path sends
    /// `0x00C8 Item_Appraise` on the way past, and retail's close-first leg sends nothing at all.
    ///
    /// Returns whether this call closed the window, i.e. whether it was open.
    pub fn close_from_action(&mut self, ui: &mut UiSystem) -> bool {
        self.hide(ui)
    }

    /// The hide all three close paths make, and the only place `closed` is counted.
    fn hide(&mut self, ui: &mut UiSystem) -> bool {
        let Some(w) = self.window else { return false };
        if !ui.node(w).is_some_and(|n| n.region.flags.visible) {
            return false;
        }
        ui.set_visible(w, false);
        self.closed += 1;
        true
    }

    /// The examination panel's element-message handler's `0x1000032D` arm — **the one
    /// producer of the newly-selected-item flag's clear.**
    ///
    /// The arm uses element `0x1000032D`, message **4**, and a non-null
    /// second parameter; the clicked row's `0x10000010` property is fetched and turned into a
    /// component number; the spell pane maps that number to an object id. If non-zero, clear the
    /// selection-reaction flag, select the object with second argument zero, and restore it.
    ///
    /// **Why the restore is not here.** Those three steps bracket a *synchronous*
    /// broadcast: selecting the object raises the selection-changed notice, and that notice's
    /// handler reads the flag while it is down. This build's reader is
    /// [`Self::update`]'s pull on the following frame, so restoring here would restore it before
    /// anything had looked and the panel would re-examine the component it had just selected —
    /// exactly the defect the bracket exists to prevent. `update` performs the restore, one
    /// statement after the reader, which is the same position relative to the broadcast.
    ///
    /// `0x1000032D` ([`SPELL_COMPONENT_LIST`]) is a real child of `<EXAM>` in the shipped layout,
    /// so the click is reachable. The id resolution — the spell-examine panel's
    /// component-object-id read, which turns the clicked row's `0x10000010` property into an
    /// object id — is the caller's; see `spell_component_selected`.
    ///
    /// Returns the `UiRequest::Select` to emit, or `None` for the arm's own zero-object guard.
    pub fn select_spell_component(
        &mut self,
        component: dereth_primitives::ObjectId,
    ) -> Option<crate::view::UiRequest> {
        if component.0 == 0 {
            return None;
        }
        self.examine_newly_selected_item = false;
        Some(crate::view::UiRequest::Select(component))
    }

    /// The complete client sequence around [`Self::select_spell_component`].
    ///
    /// A row with no SCID does nothing; an SCID the component tracker maps to no object does
    /// nothing at all; otherwise the flag is cleared, the object is selected (second argument
    /// zero), and the flag is restored.
    ///
    /// **Selecting the object is the whole of it.** There is no examine here, no
    /// use, no drag: the pane's rows are plain regions in a `ListBox`, not
    /// item tiles in an item list, so none of the client's arms is on this path. What
    /// the player sees is the component *selected* — and, because the panel is showing a spell
    /// rather than an item, the guard above is what stops the selection immediately re-examining
    /// it and replacing the spell in the window.
    ///
    /// Returns the request to emit, or `None` for either of the two early outs.
    fn spell_component_selected(
        &mut self,
        view: &dyn GameView,
        scid: u32,
    ) -> Option<crate::view::UiRequest> {
        let object = view.component_object_id(scid)?;
        let r = self.select_spell_component(object)?;
        self.component_selections += 1;
        Some(r)
    }

    /// **The other close path**, and
    /// the re-examine that shares its `if`.
    ///
    /// A hidden window or a cleared selection-reaction flag returns without action.
    /// Otherwise a nonzero selected object is examined; a zero selection hides the window.
    ///
    /// So: **the window closes when the selection is cleared, and re-examines when the selection
    /// moves to another object** — and neither happens while it is already hidden, which is the
    /// second reason a closed panel stays closed.
    ///
    /// The re-examine arm returns `UiRequest::Examine`, which is the examine-object call
    /// on this side of the seam: it reaches `examine_object` and comes back as the
    /// `examine_request` notice [`Self::update`] already pulls, so the *notice* half is not
    /// duplicated here. Returns `(closed, request)`.
    pub fn selection_changed(
        &mut self,
        ui: &mut UiSystem,
        selected: Option<dereth_primitives::ObjectId>,
    ) -> (bool, Option<crate::view::UiRequest>) {
        if !self.is_open(ui) || !self.examine_newly_selected_item {
            return (false, None);
        }
        match selected.filter(|id| id.0 != 0) {
            Some(id) => (false, Some(crate::view::UiRequest::Examine(id))),
            None => (self.hide(ui), None),
        }
    }

    /// The examination panel's examine-object notice.
    ///
    /// Id zero returns. Otherwise it raises the busy count if nothing was awaited, makes the id the
    /// awaited one, sends the appraise request and clears the current appraisal id.
    ///
    /// **It does not show the panel** — only the appraise-info write does, and only when the reply
    /// for this id arrives. The appraise-request half is the host's here: `UiRequest::Examine`
    /// carries it and reaches attempt_appraise, which is the same message.
    ///
    /// Clearing the current appraisal id is the line that makes the *second* examine of a different
    /// object re-open a closed panel: without it the old id would still match the guard and the
    /// reply would refill in place without showing.
    pub fn examine_object(&mut self, id: dereth_primitives::ObjectId) {
        if id.0 == 0 {
            return;
        }
        self.awaiting = Some(id);
        self.current = None;
    }

    /// The whole "right-click a spell" path and the operations it forwards to.
    ///
    /// An awaited appraisal is dropped, with its busy count. If there was an awaited or a current
    /// one, the current id is cleared and an appraise request for object `0` is sent — a
    /// **cancel**, not a request. Then the spell pane is filled, becomes the active pane if it was
    /// not, and the window is shown — **unconditionally**.
    ///
    /// Three things fall out of it that the appraisal path would have got wrong:
    ///
    /// * **The window is shown every time**, with no *new-reply* guard. A spell examine re-opens a
    ///   panel the player closed, because there is no reply to wait for — the pane is filled from
    ///   two dat tables before the window is shown, synchronously.
    /// * **Any appraisal in flight is cancelled**, and both id fields are cleared, so a `0x00C9`
    ///   that arrives afterwards fails the appraise-info write's guard and cannot overwrite the
    ///   spell. That cancel is the appraise request for object `0` — `0x00C8` with a **zero**
    ///   object id — and it is the only message this path can put on the wire.
    /// * **A spell the table does not know changes nothing at all.** The client's spell examine
    ///   returns 0 before its first write, and this build keeps that: the pane refuses, and the
    ///   window is not shown.
    ///
    /// Returns whether the window was shown by this call.
    pub fn examine_spell(&mut self, ui: &mut UiSystem, view: &dyn GameView, spell: u32) -> bool {
        // **The cancel, both halves.**
        //
        // With both ids zero the client goes straight to the fill and sends nothing.
        //
        // The **guard is the panel's**, which is why it is here and not in `dereth_client_model`: it asks
        // about `ExaminationPanel`'s own two ids, and a cancel sent unconditionally would put a
        // `0x00C8` on the wire for every spell right-click a player ever makes.
        //
        // `UiRequest::CancelAppraisal` and **not** `UiRequest::Examine(ObjectId(0))`: the zero
        // means the opposite thing in the other function. The client's examine-object
        // path reads a zero as *arm examine targeting* and never sends the request; the spell
        // examine calls the packer directly. Emitting the `Examine` spelling here would put the
        // pointer into examine mode on every spell right-click.
        //
        // Clearing both ids is the observable half and stays: it is what makes a `0x00C9` that
        // arrives after this call fail the appraise-info write's guard, so the spell stays on
        // screen instead of being overwritten by the item that was being appraised.
        if self.awaiting.is_some() || self.current.is_some() {
            ui.requests.emit(crate::view::UiRequest::CancelAppraisal);
            self.appraisals_cancelled += 1;
        }
        self.awaiting = None;
        self.current = None;
        let Some(v) = self.spell.examine_spell(ui, view, spell) else {
            return false;
        };
        // The parent's displayed-name text is written by the *sub*-UI here, not by
        // the title-text write, and there is **no stack-size suffix**: a spell is not an
        // object, so `title_text` here is the bare spell name.
        if let Some(h) = self.title {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&v.name);
            }
        }
        self.title_text = Some(v.name.clone());
        // The current object id belongs to the *item* pane and is not touched by this path;
        // clearing it here is what stops a later appraisal for the object that was showing before
        // from taking the appraise-info write's same-object path against a pane that has moved.
        self.sub_object = None;
        if self.active != Some(ExamineSubUi::Spell) {
            self.set_active_examine_ui(ui, ExamineSubUi::Spell);
        }
        let Some(w) = self.window else { return false };
        ui.set_visible(w, true);
        self.opened += 1;
        true
    }

    /// The appraise-info write; see the module header for what it does. Returns whether the window
    /// was shown by this call.
    ///
    /// `name` is the object's name and `stack_size` its stack size; `None` for the pair is the
    /// live-object check failing, which returns
    /// without writing anything.
    pub fn set_appraise_info(
        &mut self,
        ui: &mut UiSystem,
        id: dereth_primitives::ObjectId,
        p: &AppraisalView,
        weenie: Option<(&str, u32)>,
    ) -> bool {
        if id.0 == 0 || (self.awaiting != Some(id) && self.current != Some(id)) {
            return false;
        }
        let is_new = self.awaiting == Some(id);
        if is_new {
            self.awaiting = None;
            self.current = Some(id);
        }
        // The pane fork, as retail makes it: not a creature is the item pane; a creature carrying
        // either `Template` (PropertyString 5) or `CharacterTitleId` (PropertyInt 0x105) is a
        // *player* and takes the character pane; anything else is the creature pane.
        let sub = match dereth_client_contract::examination::appraisal_pane(
            p.creature,
            p.template,
            p.character_title,
        ) {
            dereth_client_contract::examination::AppraisalPane::Item => ExamineSubUi::Item,
            dereth_client_contract::examination::AppraisalPane::Creature => ExamineSubUi::Creature,
            dereth_client_contract::examination::AppraisalPane::Character => ExamineSubUi::Char,
        };
        let new_object = self.sub_object != Some(id);
        // The live-object check — above every write, so an appraisal for an
        // object this client does not have leaves the panel exactly as it was.
        let Some((name, stack_size)) = weenie else {
            return false;
        };
        self.replies_applied += 1;
        self.sub_object = Some(id);

        // The title is string `0x34` if present, else the name, with the stack size.
        // `GearPlatingName` overrides
        // the object's own name, which is how a plated weapon shows its plating.
        let display_name = p.gear_plating_name.as_deref().unwrap_or(name);
        let title = title_text(display_name, stack_size);
        if let Some(h) = self.title {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&title);
            }
        }
        self.title_text = Some(title);

        // The pane's own appraise-info write.
        //
        // The item-examine panel's appraise-info write runs the inscription write and then
        // twenty-five appraisal blocks in a fixed order; [`item_description`] is the six of them
        // this build draws, in that order, and [`ITEM_BLOCKS_NOT_IMPLEMENTED`] names the rest. The
        // creature and character panes use their own block lists plus the nine information regions
        // their constructors put into the list box.
        match sub {
            ExamineSubUi::Item => {
                self.set_inscription(ui, p);
                // **One per run, not one text write for the lot.** A single flat string would
                // draw every block in the element's plain colour, and the enchantment
                // highlighting would have nowhere to go. [`add_item_info`] has the separator rule;
                // the append passes font index `0`
                // and the block's **colour** index; see the `HIGHLIGHTED_PROPERTIES` section. The
                // glyph-count guard is the client's own and is why a fresh pane's first line starts
                // at the top.
                let runs = item_description_runs(p);
                // The description block ends with a direct text append of the augmentation
                // cost, not an add-item-info line: the shipped row owns its leading/trailing
                // newline. Its one variable is named "EXPERIENCE". Reuse the shared localized
                // integer/string services.
                let cost = p.augmentation_cost.map(|value| {
                    // The value is named. The row has exactly one variable and it is the hash of
                    // `"EXPERIENCE"` [measured], so the value goes in under that name rather than
                    // into whatever slot comes first.
                    let xp = super::numfmt::number(value);
                    super::characterinfo::compose_in(
                        ui,
                        super::statmgmt::STRING_TABLE,
                        "ID_Examine_Item_AugmentationCost",
                        &[("EXPERIENCE", xp.as_str())],
                    )
                });
                if let Some(h) = self.item_display_text {
                    if let Some(t) = ui.text_element_mut(h) {
                        t.set_text("");
                        for r in &runs {
                            if !t.glyphs.is_empty() {
                                t.append_text(if r.same_line { "\n" } else { "\n\n" });
                            }
                            t.append_text_with_font_and_color(&r.text, 0, r.color);
                        }
                        if let Some(cost) = &cost {
                            t.append_text(cost);
                        }
                    }
                }
                let mut text = flatten_item_info(&runs);
                if let Some(cost) = cost {
                    text.push_str(&cost);
                }
                self.item_text = Some(text);
            }
            ExamineSubUi::Creature | ExamineSubUi::Char => {
                self.set_creature_info(ui, p, sub, name);
            }
            ExamineSubUi::Spell => {}
        }
        let _ = new_object;

        if self.active != Some(sub) {
            self.set_active_examine_ui(ui, sub);
        }
        if is_new {
            if let Some(w) = self.window {
                ui.set_visible(w, true);
                self.opened += 1;
            }
        }
        is_new
    }

    /// The `<Inscribe here>` edit box.
    ///
    /// The client clears both text elements and the two cached strings first, then takes the box
    /// away entirely for an object that is not inscribable (it hides the *text element*, not the
    /// window). Only then does it read the profile. See [`inscription_text`] for the **four**-way
    /// fork.
    ///
    /// The caches the client keeps beside the two text
    /// elements (the scribe name and the inscription) are kept here too, because every
    /// later decision — may this player write, is the box showing the invitation, is what it now
    /// holds worth a message — is made against them and not against the profile.
    ///
    /// **How those two are written matters.** The string reads store them directly,
    /// so each is set the moment its `PropertyString` answers, and an
    /// item with a scribe and no inscription keeps the scribe. See the comment at the assignments.
    ///
    /// The **state** calls are the placeholder's centring: state `0x10000050` on the way in,
    /// state `1` on the
    /// leg that has a real inscription to show. See [`INSCRIPTION_PLACEHOLDER_STATE`].
    fn set_inscription(&mut self, ui: &mut UiSystem, p: &AppraisalView) {
        let text = inscription_text(
            p.inscribable,
            p.scribe_name.as_deref(),
            p.inscription.as_deref(),
        );
        let signature = inscription_signature(
            p.inscribable,
            p.scribe_name.as_deref(),
            p.inscription.as_deref(),
        );
        // The box goes to the placeholder state, and both text elements and both caches are
        // cleared.
        // Unconditional, and before the profile is looked at: the centred invitation is the
        // element's resting state and every other leg moves it off.
        if let Some(h) = self.inscription_text {
            ui.set_state(h, INSCRIPTION_PLACEHOLDER_STATE);
        }
        self.scribe_name.clear();
        self.inscription_value.clear();
        // A not-inscribable object hides the text and signature only. The editable-state write
        // keeps the background present and gives it the mouse on the locked leg, so the mouse-press
        // handler can explain the refusal.
        for h in [self.inscription_text, self.inscription_signature]
            .into_iter()
            .flatten()
        {
            ui.set_visible(h, text.is_some());
        }
        // The two caches are written by the string reads **directly**, one at a
        // time, and neither waits for the other: the scribe name (string `8`), then the
        // inscription (string `7`).
        //
        // Writing them as a pair on the "both present" leg would be wrong: an item with a scribe
        // and no text would leave the panel believing nobody had signed it, so the box would
        // invite an inscription the editable-state write would refuse to send. The
        // scribe's read runs first and unconditionally; the inscription's is reached only
        // when the scribe is non-empty, which is why the second is nested.
        if let Some(scribe) = p.scribe_name.as_deref() {
            self.scribe_name = scribe.to_string();
        }
        if !self.scribe_name.is_empty() {
            if let Some(written) = p.inscription.as_deref() {
                self.inscription_value = written.to_string();
            }
        }
        // On the "both strings present" leg, state `1` belongs to this path alone. A
        // signed-but-blank box skips it and keeps the centred state assigned earlier.
        if !self.scribe_name.is_empty() && !self.inscription_value.is_empty() {
            if let Some(h) = self.inscription_text {
                ui.set_state(h, INSCRIPTION_NORMAL_STATE);
            }
        }
        if let Some(h) = self.inscription_text {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(text.as_deref().unwrap_or(""));
            }
        }
        if let Some(h) = self.inscription_signature {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&signature);
            }
        }
        self.inscription = text;
        self.inscription_signature_text = signature;
        // The editable-state write — the last line, on every
        // leg including the one that took the box away.
        self.set_inscription_editable_state(ui, p);
    }

    /// **Who is allowed to write on it.**
    ///
    /// The box's editable (`0x16`) and selectable (`0x27`) properties are written with one value.
    /// With no live object, or one whose inscribable bit (`0x2`) is clear, it is false. Otherwise the
    /// ordinary rule is that the box is editable when the item is unsigned or signed by this player
    /// (case-insensitive) **and** the player owns it; a PSR may edit regardless. The background is
    /// then made mouse-invisible on the editable leg and mouse-visible on the other.
    ///
    /// **The background takes the mouse only when the box does not**, which is what lets a click on
    /// `0x10000137` (element message `0x1C` with `p1 == 7`) be answered with *"Only %hs can change
    /// the inscription"*. That explanatory arm is implemented by
    /// [`Self::handle_inscription_mouse_press`]; its three literals include
    /// `"This item is not inscribable."`, all written to chat channel `0x1A`.
    ///
    /// The ordinary-player messages are implemented by
    /// [`Self::handle_inscription_mouse_press`]. The PSR test reads the same authoritative
    /// Boolean qualities as the client's, but only behind the live-object gate. The profile's
    /// `inscribable` field is deliberately not that gate: hook appraisal data may substitute it
    /// for display, while this function reads the current object's public-description bit 2.
    fn set_inscription_editable_state(&mut self, ui: &mut UiSystem, p: &AppraisalView) {
        let editable = dereth_client_contract::examination::inscription_editable(
            self.inscription_mouse_facts,
            p.owned_by_player,
            p.viewer_is_psr,
            &self.scribe_name,
            &self.player_name,
        );
        self.inscription_editable = editable;
        if let Some(h) = self.inscription_text {
            ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_EDITABLE, editable);
            ui.set_attribute_bool(h, dereth_ui::props::attr::TEXT_SELECTABLE, editable);
            // The text element's attribute-set handler refreshes
            // the element's should-be-mouse-visible state after either setter; without this local
            // refresh a text element that just became locked continues to intercept the background
            // press.
            ui.update_mouse_visibility(h);
        }
        if let Some(h) = self.inscription_background {
            ui.set_mouse_visible(h, !editable);
        }
    }

    /// The basic creature examine panel's appraise-info write and
    /// the creature examine panel's appraise-info write's first block.
    ///
    /// With no creature block there is no level and no rows. Otherwise the level (int `0x19`) is
    /// written to the level value text, and each of the nine info regions updates from the profile.
    ///
    /// The nine tokens are six attribute info regions and three secondary-attribute info regions,
    /// built once by the constructor in the order [`CREATURE_ATTRIBUTE_ROWS`] then
    /// [`CREATURE_VITAL_ROWS`].
    ///
    /// **This build rebuilds the rows on every reply rather than updating nine live tokens.** The
    /// client's tokens are `static` — one set shared by every creature ever examined — and this
    /// panel has no equivalent lifetime; rebuilding produces the same nine rows with the same nine
    /// labels in the same order, which is what is observable. The *count* is asserted separately
    /// from the text ([`Self::creature_rows_drawn`]) so a list box with no row template is a
    /// number and not a blank pane.
    fn set_creature_info(
        &mut self,
        ui: &mut UiSystem,
        p: &AppraisalView,
        sub: ExamineSubUi,
        name: &str,
    ) {
        let level = level_value(p.level);
        if let Some(h) = self.level_value {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&level);
            }
        }
        self.level_text = Some(level);

        // The appraise-info write's int `2` -> the creature display name read. A miss
        // leaves the element alone, as in the client.
        if let Some(name) = p.creature_display_name.as_deref() {
            if let Some(h) = self.creature_name {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(name);
                }
            }
        }
        self.creature_name_text = p.creature_display_name.clone();

        let rows = creature_rows(p);
        // The info region's update does not write the value cell as plain text; it writes it
        // with font 0 and a colour, and the colour is `3` whenever the assess failed. Drawing
        // the value with `set_text` would leave a failed assess's `???` in the element's own
        // colour — white — where retail paints all nine.
        // The **label** cell keeps `set_text`: neither update touches the label text.
        let colors = creature_row_colors(p);
        self.creature_rows_drawn = 0;
        if let Some(mut list) = self.stat_list.take() {
            list.flush(ui);
            for ((label, value), color) in rows.iter().zip(colors.iter()) {
                let Some(h) = list.add_from_template(ui, 0, None) else {
                    continue;
                };
                if let Some(c) = ui.get_child_recursive(h, ROW_LABEL) {
                    if let Some(t) = ui.text_element_mut(c) {
                        t.set_text(label);
                    }
                }
                if let Some(c) = ui.get_child_recursive(h, ROW_VALUE) {
                    if let Some(t) = ui.text_element_mut(c) {
                        t.set_text("");
                        t.append_text_with_font_and_color(value, 0, *color);
                    }
                }
                self.creature_rows_drawn += 1;
            }
            list.update_layout(ui);
            self.stat_list = Some(list);
        }
        self.creature_row_text = rows;
        self.creature_row_colors = colors;

        // =========================================================================================
        // **The rest of both panes' appraise-info writes.**
        // =========================================================================================
        //
        // Everything above is the base creature-appraisal implementation, which both panes
        // call last. What follows is what the two overrides do **before** that call.

        // The text clear on the allegiance-name text —
        // unconditional in both panes, before either looks at the
        // profile. Without it the last player's allegiance would stay under the next creature.
        if let Some(h) = self.allegiance_name {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text("");
            }
        }
        self.allegiance_name_text = None;

        if sub == ExamineSubUi::Char {
            let write = |ui: &mut UiSystem, h: Option<ElemHandle>, text: &str| {
                if let Some(h) = h {
                    if let Some(t) = ui.text_element_mut(h) {
                        t.set_text(text);
                    }
                }
            };
            // Write the element only when
            // the gender-and-heritage display read answered, so a miss leaves whatever was there —
            // which is why this is an `if let` and not a clear-then-set.
            if let Some(text) = p.gender_heritage_display.as_deref() {
                write(ui, self.char_heritage, text);
                self.heritage_text = Some(text.to_string());
            }
            // Use the title table first and the template only as its fallback.
            // `Hud::appraisal` resolves the fork because both halves are dat lookups.
            if let Some(text) = p.profession.as_deref() {
                write(ui, self.char_profession, text);
                self.profession_text = Some(text.to_string());
            }
            // Guarded on the current object being known, which is the same object
            // the live-object check above already required.
            let pk = pk_status_text(p.weenie_is_pk, p.weenie_is_pk_lite);
            write(ui, self.char_pk_status, pk);
            self.pk_status_text = Some(pk.to_string());
            // The allegiance full-name read over the title bar the examination panel's
            // appraise-info write had already filled: the allegiance title
            // for this rank/heritage/gender, a space, then the object's name — or the bare name
            // when the allegiance title read refuses. No stack count: a player is
            // never a stack.
            let full = match p.allegiance_title.as_deref() {
                Some(t) => format!("{t} {name}"),
                None => name.to_string(),
            };
            write(ui, self.title, &full);
            self.title_text = Some(full);
            // Set the allegiance name only when rank is at least one and the string is present.
            if p.allegiance_rank.unwrap_or(0) >= 1 {
                if let Some(text) = p.allegiance_name.as_deref() {
                    write(ui, self.allegiance_name, text);
                    self.allegiance_name_text = Some(text.to_string());
                }
            }
        }

        // The extra-info list runs first in both panes, followed by one row-helper call per
        // row.
        let misc = if sub == ExamineSubUi::Char {
            char_misc_rows(p)
        } else {
            creature_misc_rows(p)
        };
        self.misc_rows_drawn = 0;
        if let Some(mut list) = self.misc_list.take() {
            list.flush(ui);
            for row in &misc {
                let Some(h) = list.add_from_template(ui, 0, None) else {
                    continue;
                };
                for (id, text) in [(ROW_LABEL, &row.label), (ROW_VALUE, &row.value)] {
                    if let Some(c) = ui.get_child_recursive(h, id) {
                        if let Some(t) = ui.text_element_mut(c) {
                            // The coloured text write — font 0, the row's colour, and
                            // the **same** colour on both cells.
                            t.set_text("");
                            t.append_text_with_font_and_color(text, 0, row.color);
                        }
                    }
                }
                self.misc_rows_drawn += 1;
            }
            list.update_layout(ui);
            self.misc_list = Some(list);
        }
        self.misc_row_text = misc;
    }

    /// Hide the other three panes
    /// and show the chosen one, in that order.
    ///
    /// The creature and character examine panes share the base
    /// field `0x10000140` and differ only in the sub-panel, so showing one of them shows the base
    /// and its own sub-panel and hides the other's.
    fn set_active_examine_ui(&mut self, ui: &mut UiSystem, sub: ExamineSubUi) {
        self.active = Some(sub);
        let show = |ui: &mut UiSystem, h: Option<ElemHandle>, v: bool| {
            if let Some(h) = h {
                ui.set_visible(h, v);
            }
        };
        show(ui, self.item_base, sub == ExamineSubUi::Item);
        show(ui, self.spell_base, sub == ExamineSubUi::Spell);
        let creature_like = matches!(sub, ExamineSubUi::Creature | ExamineSubUi::Char);
        show(ui, self.creature_base, creature_like);
        show(ui, self.creature_sub, sub == ExamineSubUi::Creature);
        show(ui, self.char_sub, sub == ExamineSubUi::Char);
    }

    /// One frame's drive: if a reply has landed for the object this panel is waiting on, apply it.
    ///
    /// This is the pull half of a notice the client pushes. `set_appraise_info` raises
    /// `Notice::AppraisalReady`, which nothing consumed; rather than add a second notice bus, the
    /// panel asks the view for the profile of the id it is already waiting on, which it can only
    /// answer once `0x00C9` has landed. **The frame is the same one**: `Hud::drive` runs after
    /// `interaction::apply_events` in `App::frame`, so `set_appraise_info` happens on the frame the
    /// reply lands and not the frame after.
    ///
    /// Returns whether the panel changed.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let changed = self.pull(ui, view);
        // **The description pane's scrollbar.**
        //
        // The client re-measures, inside its own layout pass, every text element flagged as needing
        // it (`0x100`): it recalculates the glyphs, resizes the scrollable area to the text plus
        // its margins, and re-applies the scroll offset.
        //
        // Without it the item description text `0x1000013C` would report a content extent of
        // **0 x 0** however much text it held, so the scrollbar sizing would see
        // `content <= view`, set `0x1000013D` disabled, and its authored `0x79 hide-when-disabled`
        // would take the bar off the screen -- leaving the static plate `0x100005F8` behind it, a
        // darkened strip on the right side of the window instead of a scrollbar. The pane names
        // its bar (attribute `0x72 = 0x1000013D`) and binds it.
        //
        // There is no hand call here: setting text raises the `0x100` flag, and `UiSystem::draw`
        // consumes it from the root, which is where the client's per-frame region pass consumes
        // it too. Both panes in this subtree (`0x1000013C` and the inscription box `0x1000013E`)
        // are covered without either being named.
        changed
    }

    /// The pull itself. [`Self::update`] performs it and then re-measures the panes, matching
    /// the client's tail documented there.
    fn pull(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        // The examine-object notice first: the examine **cursor** and the
        // `0x2B` key raise it from `dereth_client::interaction`, which has no screen to call, so the
        // notice is pulled off the view here. In the client this is a push and this line does not
        // exist; the serial is what makes a repeat examine of the same object a new notice. See
        // [`crate::view::GameView::examine_request`].
        //
        // It runs *before* the reply is looked at, which is the client's order too: the notice sets
        // the awaited appraisal id and only then can the appraise-info write match it.
        // Selection changes are pulled the same way. This runs
        // **before** the examine notice for the client's own reason: its re-examine arm *is* what
        // raises that notice, so a selection change and the examine it produces must not be seen
        // in the other order. The first frame only records the selection -- a panel that has never
        // seen one has not had a *change*, and treating the initial `None` as a deselection would
        // close a window nothing had opened.
        // The player's own name —
        // read by the inscription gaining-focus handling, the losing-focus handling
        // and the inscription editable-state write, all three from a global the
        // client can reach from anywhere. Those three run from an element message that carries no
        // `GameView`, so the name is latched here, on the same pull that fills the panel.
        if let Some(n) = view.character_name() {
            if self.player_name != n {
                self.player_name = n.to_string();
            }
        }
        // The client's handler reads the current object id, which remains the displayed item
        // while a different appraisal is pending. Refresh that identity, not `awaiting`, so a
        // press during the wait cannot combine the old inscription with the new object's facts.
        if self.active == Some(ExamineSubUi::Item) {
            self.inscription_mouse_facts = self
                .sub_object
                .and_then(|id| view.inscription_mouse_facts(id));
        }
        let selected = view.selected_object();
        let mut changed = false;
        match self.last_selected {
            None => self.last_selected = Some(selected),
            Some(prev) if prev != selected => {
                self.last_selected = Some(selected);
                let (closed, request) = self.selection_changed(ui, selected);
                // Set the newly-selected-item flag unconditionally, immediately
                // after the selected-object write returns. In the client that is
                // after the broadcast has already run; here the broadcast is this pull, so the
                // restore is here rather than at the clear. Counted, so that "the guard suppressed
                // a re-examine" is a number and not a silence.
                if !self.examine_newly_selected_item {
                    self.examine_newly_selected_item = true;
                    self.self_selections_absorbed += 1;
                }
                changed |= closed;
                if let Some(r) = request {
                    ui.requests.emit(r);
                }
            }
            Some(_) => {}
        }
        // The formula list's click, resolved. It runs **after** the selection
        // reader above and not before, and the order is load-bearing: the clear it performs is
        // meant to be read by the *next* frame's reader, over the selection this request is about
        // to make. Resolving first would put the guard down in front of this frame's reader, which
        // is still looking at the old selection, and the restore would happen a change too early.
        if let Some(scid) = self.pending_component_scid.take() {
            if let Some(r) = self.spell_component_selected(view, scid) {
                ui.requests.emit(r);
            }
        }
        if let Some((id, serial)) = view.examine_request() {
            if serial != self.last_examine_serial {
                self.last_examine_serial = serial;
                self.examines_pulled += 1;
                self.examine_object(id);
            }
        }
        // Spell-examine notices are drained the same
        // way and for the same reason: its producer is the spellbook's or the spell bar's item
        // list, which live on `dereth_client::hud::Hud` and cannot reach this panel. Unlike the
        // object notice this one **fills and shows the panel immediately** — there is no reply to
        // wait for — so it runs after the object notice and before the appraisal is looked at, and
        // a spell examine on the same frame as an appraisal wins, which is the client's order too
        // (spell examination cancels the appraisal it arrives beside).
        for spell in ui.notice_inbox.take_examine_spell() {
            self.spell_examines_pulled += 1;
            changed |= self.examine_spell(ui, view, spell);
        }
        // The examination panel's update-spell-components notice:
        // it forwards straight to the spell pane's component update, with **no** visibility guard
        // and no test of the sub-UI in play, so a hidden `<EXAM>` re-marks its formula rows too.
        // The notice's component-tracker argument is not read by the listener, which is why
        // the pull side is a bare serial; see [`crate::view::GameView::component_serial`].
        //
        // It runs **after** the spell drain above so that a spell examined on the same frame as a
        // tracker change is marked once, over the rows it has just drawn, rather than over the
        // previous spell's.
        let serial = view.component_serial();
        if serial != self.last_component_serial {
            self.last_component_serial = serial;
            self.component_notices_pulled += 1;
            self.spell.update_components(ui, view);
        }
        // The client's guard is on the reply; here the pull has to ask about *some* id, and the two
        // it can be asked about are exactly the client's two.
        let Some(id) = self.awaiting.or(self.current) else {
            return changed;
        };
        let Some(p) = view.appraisal(id) else {
            return changed;
        };
        // A reply already applied is not a new reply. The client cannot ask this question because
        // it is called *by* the reply; this is the pull model's one difference, and it is a
        // **delivery serial** rather than a profile comparison precisely so that a combat re-poll
        // answering with a byte-identical profile still counts as a new reply — which is what the
        // client's appraise-info write sees and what refills the panel.
        if self.awaiting.is_none() && self.last_delivery == p.delivery {
            return changed;
        }
        let weenie = view.name(id).map(|n| {
            let s = view.slot_decoration(id).map_or(1, |d| d.stack_size);
            (n.to_string(), s)
        });
        // `set_appraise_info` changes the current object id only when the object is still present.
        // Carry the matching live snapshot across that same transition; the next pull refreshes it
        // again.
        if weenie.is_some() {
            self.inscription_mouse_facts = view.inscription_mouse_facts(id);
        }
        let applied =
            self.set_appraise_info(ui, id, &p, weenie.as_ref().map(|(n, s)| (n.as_str(), *s)));
        self.last_delivery = p.delivery;
        applied || changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: appraisal.presentation.variants-preserve-order-and-world-facts
    #[test]
    fn item_reply_uses_modern_property_order_and_exact_experience() {
        let mut ui = UiSystem::new((800, 600));
        let mut panel = ExaminationPanel::default();
        let id = dereth_primitives::ObjectId(9);
        let mut p = AppraisalView {
            success: true,
            ..Default::default()
        };
        p.special.imbued = Some(0xa0000000);
        p.special.absorb_magic_damage = true;
        p.item_level = Some(dereth_client_contract::view::ItemLevelView {
            total_xp: u64::MAX,
            base_xp: 1,
            max_level: 1,
            xp_style: 1,
        });
        panel.examine_object(id);
        assert!(panel.set_appraise_info(&mut ui, id, &p, Some(("Book", 1))));
        let text = panel.item_text.as_deref().unwrap();
        assert!(text.contains("Properties: Phantasmal, Magic Absorbing"));
        assert!(!text.contains("Special Properties:"));
        assert!(text.contains("Item XP: 18,446,744,073,709,551,615 /"));
    }

    #[test]
    fn inscription_privilege_still_requires_a_live_inscribable_object() {
        let mut ui = UiSystem::new((800, 600));
        let mut panel = ExaminationPanel {
            player_name: "Aerin".into(),
            ..Default::default()
        };
        for (live, owned, privileged, scribe, expected) in [
            (None, true, true, "", false),
            (Some((false, 0)), true, true, "", false),
            (Some((true, 0)), false, true, "Other", true),
            (Some((true, 0)), true, false, "aErIn", true),
            (Some((true, 0)), true, false, "Other", false),
            (Some((true, 0)), false, false, "", false),
        ] {
            panel.inscription_mouse_facts = live;
            panel.scribe_name = scribe.into();
            let profile = AppraisalView {
                inscribable: true,
                owned_by_player: owned,
                viewer_is_psr: privileged,
                ..Default::default()
            };
            panel.set_inscription_editable_state(&mut ui, &profile);
            assert_eq!(
                panel.inscription_editable, expected,
                "{live:?}/{owned}/{privileged}/{scribe}"
            );
        }
    }
    use dereth_primitives::ObjectId;

    /// The first two lines are driven
    /// **directly**, because the per-frame pull cannot reach them.
    ///
    /// **This test exists because the mutation that drops that guard survived the driven suite.**
    /// [`ExaminationPanel::update`] only ever asks the view about `awaiting.or(current)`, so
    /// through the pull the inner guard is unfalsifiable: removing it changes nothing an
    /// application-level test can see. The guard is still the client's own and is kept (a direct
    /// caller — the notice route this build does not have — would need it), and this is what makes
    /// it falsifiable. The stated testability rule, the third reading of a survivor.
    #[test]
    fn a_reply_for_an_id_the_panel_is_not_waiting_on_writes_nothing() {
        let mut ui = UiSystem::new((800, 600));
        let p = AppraisalView {
            value: Some(5),
            burden: Some(10),
            ..AppraisalView::default()
        };
        let mut panel = ExaminationPanel::default();

        // Nothing awaited and nothing current: refused.
        assert!(!panel.set_appraise_info(&mut ui, ObjectId(0x1234), &p, Some(("Sack", 1))));
        assert_eq!(panel.replies_applied, 0);
        assert_eq!(panel.title_text, None);

        // Awaiting a *different* object: still refused.
        panel.examine_object(ObjectId(0x5678));
        assert!(!panel.set_appraise_info(&mut ui, ObjectId(0x1234), &p, Some(("Sack", 1))));
        assert_eq!(panel.replies_applied, 0);

        // The one it asked for: applied.
        assert!(panel.set_appraise_info(&mut ui, ObjectId(0x5678), &p, Some(("Sack", 1))));
        assert_eq!(panel.replies_applied, 1);
        assert_eq!(panel.title_text.as_deref(), Some("Sack"));
        assert_eq!(panel.current, Some(ObjectId(0x5678)));
        assert_eq!(panel.awaiting, None);

        // Id 0 is refused before anything else, even when it is the current one.
        panel.current = Some(ObjectId(0));
        assert!(!panel.set_appraise_info(&mut ui, ObjectId(0), &p, Some(("Sack", 1))));
        assert_eq!(panel.replies_applied, 1);
    }

    /// Oracle: the pane fork — string `5` **or**
    /// int `0x105`, two separate tests leading to the same pane.
    ///
    /// **This test exists because the `||` -> `&&` mutation survived the corpus sweep**, and the
    /// reason is a fact about the corpus rather than about the code: its **one** character
    /// appraisal (`+Aldwyne`, `early-inventory-and-casting` `0x50000003`) carries **both**
    /// properties, so no recorded body can tell the two operators apart. Asserted here over the two
    /// one-sided cases, which is legitimate because the fork is a pure function of two booleans and
    /// nothing is being synthesised about the wire.
    #[test]
    fn either_a_template_or_a_character_title_sends_a_creature_to_the_character_pane() {
        let mut ui = UiSystem::new((800, 600));
        let mut pane = |creature: bool, template: bool, character_title: bool| {
            let p = AppraisalView {
                creature,
                template,
                character_title,
                ..AppraisalView::default()
            };
            let mut panel = ExaminationPanel::default();
            panel.examine_object(ObjectId(1));
            assert!(panel.set_appraise_info(&mut ui, ObjectId(1), &p, Some(("x", 1))));
            panel.active
        };
        assert_eq!(
            pane(false, false, false),
            Some(ExamineSubUi::Item),
            "not a creature"
        );
        assert_eq!(
            pane(false, true, true),
            Some(ExamineSubUi::Item),
            "still not a creature"
        );
        assert_eq!(pane(true, false, false), Some(ExamineSubUi::Creature));
        assert_eq!(
            pane(true, true, false),
            Some(ExamineSubUi::Char),
            "a template alone"
        );
        assert_eq!(
            pane(true, false, true),
            Some(ExamineSubUi::Char),
            "a title alone"
        );
        assert_eq!(
            pane(true, true, true),
            Some(ExamineSubUi::Char),
            "and both, which is all the corpus has"
        );
    }
    /// The portrait camera frames square and tall boxes using the element aspect ratio.
    /// Hand-computed distances cover both clamp branches, the equality boundary, and an
    /// off-origin centre. Equality cannot distinguish a strict comparison because both
    /// branches produce the same distance there.
    #[test]
    fn the_portrait_camera_frames_the_box_by_its_aspect_clamp() {
        use dereth_primitives::Vec3;

        let tall = portrait::camera_position(
            Vec3::new(-0.5, -0.5, -1.0),
            Vec3::new(0.5, 0.5, 1.0),
            300,
            300,
        );
        assert_eq!(
            (tall.x, tall.z),
            (0.0, 0.0),
            "the camera sits on the box's centre in x and z"
        );
        assert!(
            (tall.y - -2.914_213_6).abs() < 1e-4,
            "dz=2 unclamped: {}",
            tall.y
        );

        // The clamp does not fire on equality.
        let equal = portrait::camera_position(
            Vec3::new(-0.5, -0.5, -1.0),
            Vec3::new(0.5, 0.5, 1.0),
            150,
            300,
        );
        assert!(
            (equal.y - -2.914_213_6).abs() < 1e-4,
            "dz/dx == h/w keeps dz: {}",
            equal.y
        );

        // And it does fire when the object is shallow for the element's aspect.
        let wide = portrait::camera_position(
            Vec3::new(-1.0, -0.5, -0.5),
            Vec3::new(1.0, 0.5, 0.5),
            150,
            300,
        );
        assert!(
            (wide.y - -5.328_427).abs() < 1e-4,
            "dz clamped to dx*(h/w)=4: {}",
            wide.y
        );

        // The centre is carried through in x and z, so an off-origin creature is still framed.
        let off = portrait::camera_position(
            Vec3::new(9.5, -0.5, 4.0),
            Vec3::new(10.5, 0.5, 6.0),
            300,
            300,
        );
        assert!(
            (off.x - 10.0).abs() < 1e-4 && (off.z - 5.0).abs() < 1e-4,
            "centre in x and z"
        );
    }
}
