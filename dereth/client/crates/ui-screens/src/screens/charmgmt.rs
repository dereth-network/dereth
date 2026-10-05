//! `CharacterManagementScreen` — mode `0x1000000A`, character select / create / delete.
//!
//! The screen holds the *table* — the layout enum, the seven control ids, the notices, the button
//! update's three rules and the enter-game guard — and the client's actual body, which builds one
//! row element per character against `ListBox`'s row template.
//!
//! # Where retail differs from the obvious reading
//!
//! * **The list is built from the live set alone.** Retail's rebuild loops over the live set's
//!   length and every per-character read — id, name, slot and the greyed-out duration — indexes
//!   that set. The deleted set is never read by this screen, and
//!   ACE writes a zero-length `delSet` (`GameMessageCharacterList.cs`: `Writer.Write(0u)`), putting
//!   a pending deletion in the *main* list with a non-zero grace period. Chaining the two lists
//!   would produce no rows at all for a pending deletion on this server and duplicates on one
//!   that filled both.
//! * **The row height is a `max`, not a `min`.** Retail selects the larger operand in both
//!   places of `max(listHeight / max(numAllowed, num), listHeight / 20)`. See [`row_height`].
//! * **Flag `0x800000` is the wants-double-clicks flag.** `ElementFlags` bit 23 is what makes a
//!   mouse-up raise `0x1A` at all, so it is the line that makes double-clicking a row enter the
//!   world.
//! * **A row is a `Button`.** The template `0x100003A5` is a partial whose base chain is
//!   `0x21000042/0x10000488` → `0x21000040/0x10000486` → `0x21000040/0x1000047F`, **type 1**. That
//!   is why the client treats it as a text element and sets its text — a button
//!   *is* a text element — and it is why a row raises element message 1 when clicked.
//!
//! # What is still a substitute, and why
//!
//! Retail appends each new element to the list
//! box's own item list, and the list box then owns the row geometry and raises message **4**
//! (selection changed) when a row is clicked. The UI crate models that item list
//! (`dereth_ui::widgets::listbox::items`) but exposes no way to reach a behaviour object
//! from outside the crate — `Element` is not `Any` and `UiSystem` has no `list_box_mut` — so this
//! screen cannot add to it, and there is no list-box layout pass either. So the rows are parented
//! to the list box and positioned here, and the **row's own message 1** stands in for the list
//! box's message 4. That is a gap in the UI crate; the *observable* behaviour — where the
//! rows are, what they say, which one is selected, what a click and a double-click do — is the
//! client's.

use dereth_primitives::ObjectId;
use dereth_ui::framework::ScreenCx;
use dereth_ui::framework::{LayoutEnum, Screen};
use dereth_ui::persist::CharacterSet;
use dereth_ui::props::PropertyValue;
use dereth_ui::{
    ElemHandle, ElementId, ElementMessage, ListenerId, MessageId, StateId, UiError, UiMode,
    UiSystem,
};

use crate::bind::{bind_children, child, Bound, ChildBinding};

/// The screen's root: layout enum `0x10000005`, element `0x1000039A`.
const LAYOUT: LayoutEnum = LayoutEnum(0x1000_0005);
const ROOT: ElementId = ElementId(0x1000_039A);

/// The framework's own listener id.
///
/// Retail registers the framework on the root element, which is how every message raised anywhere
/// in the subtree reaches it — the message switch has no per-element registrations of its own.
const ME: ListenerId = ListenerId::External(LAYOUT.0);

/// The screen's controls, by element id.
pub const CHILDREN: &[ChildBinding] = &[
    child("world_name_field", 0x1000_039B),
    child("char_list_field", 0x1000_039D),
    child("restore_character_button", 0x1000_039E),
    child("delete_character_button", 0x1000_039F),
    child("create_character_button", 0x1000_03A0),
    child("enter_game_button", 0x1000_03A2),
    child("credits_button", 0x1000_03A3),
    child("quit_button", 0x1000_03A4),
];

/// Element `0x100003A1` is present in the message switch with no handler. Kept named so a
/// reader looking for it finds the answer rather than an omission.
pub const UNHANDLED_SWITCH_CASE: ElementId = ElementId(0x1000_03A1);

/// The list row template; message `0x1A` (activate / double-click) enters the game.
pub const LIST_ROW: ElementId = ElementId(0x1000_03A5);

/// The attribute a list row carries its character instance id in.
pub const ATTR_ROW_INSTANCE_ID: u32 = 0x1000_0009;
/// The attribute [`CharacterManagementScreen::select_character`] sets on the newly selected row.
pub const ATTR_ROW_SELECTED: u32 = 0x0E;
/// `TextElement`'s font-colour array, element 0 — the red a pending-delete row is drawn in.
///
/// **0x1B is the font colour** and it is an array indexed by font number.
pub const ATTR_FONT_COLOR: u32 = 0x1B;

/// `(1, 0, 0, 1)` — the colour the list rebuild gives a row whose character is pending
/// deletion, as an RGBA colour in the `0xAARRGGBB` packing the asset layer decodes a `Color`
/// property into.
pub const GREYED_OUT_COLOR: u32 = 0xFFFF_0000;

/// The state the button update puts a button into to disable it.
///
/// The button element's state write: "if the requested state is `0x0D`, write attribute
/// `0x0D` (disabled) true, otherwise false". The shipped button bases carry media for states 1, 2,
/// 3 and 13, so the disabled look is in the data.
pub const STATE_DISABLED: StateId = StateId(0x0D);
/// The state a button is put back into to enable it.
pub const STATE_ENABLED: StateId = StateId(0);

/// The input map catches the cancel action through.
pub const INPUT_MAP: u32 = 9;

/// The client's answer: which of the five buttons are enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ButtonStates {
    pub enter_game: bool,
    pub delete: bool,
    pub restore: bool,
    pub create: bool,
}

/// One row of the character list box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterRow {
    pub id: ObjectId,
    pub name: String,
    /// [`greyed_out_for`] is non-zero — the character is on the server's delete timer.
    pub greyed_out: bool,
    /// The element created from the row template for this row, once it exists.
    pub element: Option<ElemHandle>,
}

/// The character management screen's buttons update.
///
/// "*Enter Game* is enabled only when a row is selected and its greyed-out time is 0; *Delete*
/// and *Restore* are mutually exclusive (a greyed-out — i.e. pending-delete — character shows
/// *Restore*); *Create* is disabled once the live count reaches the account's allowance."
///
/// Note which count gates *Create*: the live set's length, the **live** characters, not the row
/// count — a character pending deletion still occupies its slot until the grace period expires.
#[must_use]
pub fn update_buttons(
    selected: Option<&CharacterRow>,
    live_count: usize,
    num_allowed: usize,
) -> ButtonStates {
    let greyed = selected.map(|r| r.greyed_out);
    ButtonStates {
        enter_game: greyed == Some(false),
        delete: greyed == Some(false),
        restore: greyed == Some(true),
        create: live_count < num_allowed,
    }
}

/// The character set's slot for `gid`: the index of its entry in the set, in the order the server
/// sent it, or `-1` when no entry carries that id (and for id 0, which names no character).
#[must_use]
pub fn character_set_slot(set: &CharacterSet, gid: ObjectId) -> i32 {
    if gid.0 == 0 {
        return -1;
    }
    set.set
        .iter()
        .position(|c| c.id == gid)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1)
}

/// The character set's "greyed out for" answer, including the slot-cap guard the name does not
/// suggest: it returns the entry's grace period only when the slot is inside the live set, the
/// entry's id is non-zero, and the slot is below the account's allowance (an allowance below 1
/// disables that last check); otherwise 0.
///
/// So a character sitting past the account's slot cap is never drawn greyed and never offers
/// *Restore*, whatever its delete timer says.
#[must_use]
pub fn greyed_out_for(set: &CharacterSet, slot: usize) -> u32 {
    let allowed = set.num_allowed_characters;
    if allowed >= 1 && slot >= allowed as usize {
        return 0;
    }
    match set.set.get(slot) {
        Some(c) if c.id.0 != 0 => c.seconds_grace_period,
        _ => 0,
    }
}

/// The list rebuild's row height: the larger of the list height divided by the greater of the
/// allowance and the live count, and the list height divided by 20.
///
/// The outer operator is not `min`: retail selects the *larger* operand in both places, so
/// `listHeight / 20` is a floor on the row height. The divisor 20 is also the list box's own
/// attribute `0x60`, which the shipped `charactermanagement` layout sets to 20.
#[must_use]
pub fn row_height(list_height: i32, num_allowed: u32, live_count: usize) -> i32 {
    let slots = i32::try_from(num_allowed)
        .unwrap_or(i32::MAX)
        .max(i32::try_from(live_count).unwrap_or(i32::MAX));
    let h1 = if slots > 0 {
        list_height / slots
    } else {
        list_height
    };
    let h2 = list_height / 20;
    h1.max(h2)
}

/// Character-session operations the screen asks the host to perform.
///
/// The client does not go through a queue at all: it directly asks player-session state to log
/// on a character, delete a character, or restore a character. That singleton does not exist
/// here, so the screen records them and hands them to the host as
/// [`UiRequest::CharacterAction`](crate::view::UiRequest::CharacterAction)s at the end of its
/// update, where the shell's request drain routes them. The type lives in `dereth_client_contract::pregame`.
pub use dereth_client_contract::pregame::CharacterAction;

/// The phrase the delete-character dialog makes the user type back before it will
/// delete a character. Localised (table enum `0x10000002`), and compared case-insensitively.
pub const DELETE_CHARACTER_RESPONSE: &str = "ID_CharacterManagement_DeleteCharacterResponse";

/// The five dialog contexts the dialog-close handler routes an answer to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogContext {
    ErrorMessage,
    ConfirmExit,
    DeleteCharacter,
    PleaseWait,
    EnteringWorld,
}

/// The five contexts, in the client's own switch order.
pub const DIALOG_CONTEXTS: [DialogContext; 5] = [
    DialogContext::ErrorMessage,
    DialogContext::ConfirmExit,
    DialogContext::DeleteCharacter,
    DialogContext::PleaseWait,
    DialogContext::EnteringWorld,
];

impl DialogContext {
    /// Property **0x8E** — which of the seven dialog kinds each dialog build asks for.
    ///
    /// Each build writes the property value as an enum:
    ///
    /// | dialog | 0x8E | kind |
    /// |---|---:|---|
    /// | error message | 3 | `Message` |
    /// | delete character confirmation | 5 | `ConfirmationTextInput` |
    /// | please wait | 2 | `Wait` |
    /// | entering world | 2 | `Wait` |
    /// | confirm exit | 1 | `Confirmation` |
    ///
    ///
    /// It is also the value the dialog-close handler's switch demultiplexes on: its cases are
    /// **1**, **5**, **2** and **3**, one per distinct kind above, and case **4** (`TextInput`) is
    /// empty — which is the same reading from the other end.
    #[must_use]
    pub const fn kind(self) -> dereth_ui::dialog::DialogKind {
        use dereth_ui::dialog::DialogKind as K;
        match self {
            Self::ErrorMessage => K::Message,
            Self::ConfirmExit => K::Confirmation,
            Self::DeleteCharacter => K::ConfirmationTextInput,
            Self::PleaseWait | Self::EnteringWorld => K::Wait,
        }
    }

    /// The children whose element message **1** each subclass treats as the answer, in
    /// (accept, cancel) order — [`dereth_ui::dialog::DialogKind::answer_children`] for this
    /// context's own [`Self::kind`], and nothing else.
    ///
    /// The shared table in `dereth-ui` is the one source; this screen keeps no private copy. For
    /// this screen's contexts it reads:
    ///
    /// | context | kind | accept | cancel |
    /// |---|---|---:|---:|
    /// | `ConfirmExit` | `Confirmation` | 0x17 | 0x19 |
    /// | `DeleteCharacter` | `ConfirmationTextInput` | 0x2E | 0x2F |
    /// | `ErrorMessage` | `Message` | 0x26 | — |
    /// | `PleaseWait`, `EnteringWorld` | `Wait` | — | — |
    ///
    /// Two copies of a table that decides which button deletes a character is one copy too many;
    /// the one kept is the one the seven-kind test in `dereth-ui` covers.
    #[must_use]
    pub const fn answer_children(self) -> (Option<ElementId>, Option<ElementId>) {
        self.kind().answer_children()
    }
}

/// The `Dialog` layout — **enum 2**, `0x2100003C` in this dat build. Every dialog build in this
/// screen creates its element as a root of layout enum 2 at `kind.root_element_id()`, which is
/// what the dialog factory's create-dialog does with property 0x8E.
pub const DIALOG_LAYOUT: LayoutEnum = LayoutEnum(2);

/// The string table enum `0x10000002` resolves to in this dat build — the same table
/// `screens::chargen::ERROR_STRING_TABLE` names, and the one every `ID_CharacterManagement_*`
/// token below lives in.
pub const STRING_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0002);

/// The confirm-exit dialog's prompt, property `0xC5`.
pub const CONFIRM_EXIT_STRING: &str = "ID_CharacterManagement_ConfirmExit";
/// The delete-character confirmation dialog's prompt, property `0xC5`. Its row carries **one
/// variable**, the player-name string variable, so the shipped text is two literal halves with the
/// character's name between them — see [`CharacterManagementScreen::delete_confirmation_text`].
pub const DELETE_CONFIRMATION_STRING: &str = "ID_CharacterManagement_DeleteCharacterConfirmation";
/// The please-wait dialog's body, property `0xC5`.
pub const PLEASE_WAIT_STRING: &str = "ID_CharacterManagement_PleaseWait";
/// The entering-world dialog's body, property `0xC5`.
pub const ENTERING_WORLD_STRING: &str = "ID_Character_EnteringWorld";
/// The phrase the player must type back to confirm a deletion. The host resolves
/// it and hands it in through [`CharacterManagementScreen::set_delete_confirmation_phrase`]; the
/// token is named here so both ends of that seam quote the same row.
pub const DELETE_RESPONSE_STRING: &str = "ID_CharacterManagement_DeleteCharacterResponse";

/// The confirmation-text-input dialog's *Done* button — the one that harvests the typed text.
pub const TEXT_INPUT_ACCEPT: ElementId = ElementId(0x2E);
/// Its *Cancel* button.
pub const TEXT_INPUT_CANCEL: ElementId = ElementId(0x2F);
/// The edit box inside it, whose text becomes property `0x9C`.
///
/// It shares its element id with the confirmation-text-input dialog's **root**, which is not a
/// mistake: the shipped `Dialog` layout has root `0x2C` → panel `0x3D` → field `0x1000047E` →
/// text `0x2C`, and a recursive child lookup for `0x2C` finds the descendant because the walk starts
/// below the receiver. Dumped from `client_local_English.dat` layout `0x2100003C`; the box carries
/// property **0x16** (`editable`) and is a `TextElement` (type `0xC`) through base
/// `0x10000372`.
pub const TEXT_INPUT_FIELD: ElementId = ElementId(0x2C);
/// The message dialog's single button, element `0x26`.
pub const MESSAGE_BUTTON: ElementId = ElementId(0x26);

/// The three strings the char-gen verification response can show, from table enum
/// `0x10000002`.
pub const CHARGEN_VERIFICATION_STRINGS: [&str; 3] = [
    "ID_CharacterManagement_CG_VERIFICATION_RESPONSE_NAME_IN_USE",
    "ID_CharacterManagement_CG_VERIFICATION_RESPONSE_CORRUPT",
    "ID_CharacterManagement_CG_VERIFICATION_RESPONSE_DATABASE_DOWN",
];

/// One of the five dialog contexts tracked by the character-management screen.
///
/// It holds the factory's context id, which is what the field holds in the client, not only an
/// `ElemHandle`. The original screen stores the factory context and demultiplexes a closing
/// dialog by which of the five contexts matched.
///
/// The element is `Option` because a dialog the factory **queued** has a context and no element
/// until the one in front of it closes — the state the queue exists to represent, and the state
/// this screen could not be in before.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct OpenDialog {
    context: u64,
    element: Option<ElemHandle>,
}

/// The character-management screen's five dialog contexts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct OpenDialogs {
    error_message: Option<OpenDialog>,
    confirm_exit: Option<OpenDialog>,
    delete_character: Option<OpenDialog>,
    please_wait: Option<OpenDialog>,
    entering_world: Option<OpenDialog>,
}

impl OpenDialogs {
    fn slot(&mut self, c: DialogContext) -> &mut Option<OpenDialog> {
        match c {
            DialogContext::ErrorMessage => &mut self.error_message,
            DialogContext::ConfirmExit => &mut self.confirm_exit,
            DialogContext::DeleteCharacter => &mut self.delete_character,
            DialogContext::PleaseWait => &mut self.please_wait,
            DialogContext::EnteringWorld => &mut self.entering_world,
        }
    }

    fn at(self, c: DialogContext) -> Option<OpenDialog> {
        match c {
            DialogContext::ErrorMessage => self.error_message,
            DialogContext::ConfirmExit => self.confirm_exit,
            DialogContext::DeleteCharacter => self.delete_character,
            DialogContext::PleaseWait => self.please_wait,
            DialogContext::EnteringWorld => self.entering_world,
        }
    }

    /// The live **element** for a context, which is what every caller but the queue wants:
    /// `None` both for "no such dialog" and for "queued, not shown yet".
    fn get(self, c: DialogContext) -> Option<ElemHandle> {
        self.at(c)?.element
    }

    /// The "handle already set" guard every dialog build opens with. It is the
    /// **context**, not the element, so a queued dialog refuses a second request as the client
    /// does.
    fn is_taken(self, c: DialogContext) -> bool {
        self.at(c).is_some()
    }
}

/// `CharacterManagementScreen` — mode `0x1000000A`.
#[derive(Debug, Default)]
pub struct CharacterManagementScreen {
    roots: Vec<ElemHandle>,
    bound: Bound,
    /// A copy of the framework's persistent character set, which the list box is
    /// filled from.
    pub char_set: CharacterSet,
    /// The list box's rows, in the order [`Self::rebuild_character_list`] leaves them: sorted by name, then
    /// every pending-delete row moved to the end.
    pub rows: Vec<CharacterRow>,
    /// The selected row, or `None`.
    pub selected: Option<usize>,
    /// The selected character's id, which [`Self::select_character`] also mirrors into
    /// the framework's persistent "selected avatar".
    pub selected_id: ObjectId,
    /// The char-gen state's slot, which the creation request carries: selecting a character
    /// writes its index in the character set (server order, not the list's display order), and
    /// resetting the selection writes `-1`. The list rebuild reads it to pick a row, after its
    /// own reset has already written `-1`, so in practice that pick never fires. Mirrored into
    /// the flow's persistent data every frame, as the selected id is.
    pub chargen_slot: i32,
    /// The world name update / the world name notice.
    pub world_name: Option<String>,
    /// The error `set_error_msg` was handed.
    pub error_text: Option<String>,
    /// The dialog whose answer the dialog-close handler is waiting for.
    pub open_dialog: Option<DialogContext>,
    /// The live element behind each of the five dialog contexts.
    ///
    /// The client keeps five **separate** handles — error message, confirm exit, delete
    /// character, please wait and entering world — and every dialog build opens by checking that
    /// its own handle is empty, so a second click while one is up builds nothing.
    /// [`Self::open_dialog`] is the *most recent* of them and is what
    /// the dialog-close handler demultiplexes with; this is the per-context handle the guard and
    /// the teardown need.
    dialogs: OpenDialogs,
    /// Whether the last `rebuild_character_list` actually created elements. False when the screen
    /// has no root yet, which is the pre-`create` state a unit test builds.
    pub rows_built: bool,
    /// Whether the constructor's copy of the persistent selected avatar into the selected id has
    /// already happened. It is a **constructor** assignment, so it runs once per screen and never again;
    /// see the method that seeds it from the framework's persistent data.
    avatar_seeded: bool,
    /// Whether [`Self::chargen_slot`] has been read from the flow's persistent data yet. The slot
    /// lives in player-session state that outlives every screen, so a new screen starts from
    /// whatever the last one left there.
    slot_seeded: bool,
    /// `ID_CharacterManagement_DeleteCharacterResponse`, resolved by the host out of string table
    /// enum `0x10000002`. compares the typed text with it.
    pub delete_confirmation_phrase: Option<String>,
    /// What the host must ask player-session state for, oldest first. See [`CharacterAction`].
    pub actions: Vec<CharacterAction>,
    /// The window the world's character screen message is shown in, when it sent one.
    pub message: crate::screens::screen_message::ScreenMessageWindow,
}

impl CharacterManagementScreen {
    /// The factory registered for this screen's mode.
    #[must_use]
    pub fn create_screen() -> Box<dyn Screen> {
        Box::new(Self::default())
    }

    #[must_use]
    pub fn bound(&self) -> &Bound {
        &self.bound
    }

    /// The list box element, once `create` has bound it.
    #[must_use]
    pub fn list_box(&self) -> Option<ElemHandle> {
        self.bound.get("char_list_field")
    }

    /// The world name into element `0x1000039B`.
    ///
    /// The name reaches the client as `0xF7E1 Login_WorldInfo`, whose handler posts a world-name
    /// notice that stores it, so it is a **process global** by
    /// the time the screen reads it; the host pushes it in.
    pub fn update_world_name(&mut self, ui: &mut UiSystem, name: &str) {
        if self.world_name.as_deref() == Some(name) {
            return;
        }
        self.world_name = Some(name.to_string());
        if let Some(h) = self.bound.get("world_name_field") {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(name);
            }
        }
    }

    /// The character management screen's constructor's **last two statements** —
    /// the half of the selection round trip that reads. When the persistent data exists and has
    /// received a character set, the constructor copies the persistent selected avatar into the
    /// screen's selected id and rebuilds the list.
    ///
    /// The selected avatar is on the framework's persistent data, which is the one object the
    /// UI flow's mode switch does not destroy — so this is how the character the player was
    /// just playing is still known to a framework that was constructed after they logged off.
    /// [`Self::select_character`] and [`Self::reset_previously_selected_character_slot`] are the
    /// writing half; the host mirrors [`Self::selected_id`] back out because a `Screen` is handed
    /// only a `&mut UiSystem` and cannot reach the persistent data itself.
    ///
    /// It is a **constructor** assignment and runs once: a later call is ignored, so a screen that
    /// has already been given a selection is not dragged back to a stale one.
    ///
    /// Without it a returning list always falls through to the list rebuild's last fallback — "the
    /// first row that is not greyed out, in server order".
    pub fn take_selected_avatar_from_persistent_data(&mut self, avatar: ObjectId) {
        if self.avatar_seeded {
            return;
        }
        self.avatar_seeded = true;
        self.selected_id = avatar;
    }

    /// Refill the list box from the framework's persistent character set.
    ///
    /// The body, in retail's order:
    ///
    /// 1. remember the selected id, clear the previously selected row, flush;
    /// 2. compute the row height ([`row_height`]);
    /// 3. for each entry of the live set: create a row from the template as a `TextElement`,
    ///    resize it to its own width and the row height, make it mouse-visible, set flag
    ///    `0x800000` (wants double clicks), set property `0x10000009` to the id, set its text to
    ///    the name, and — when [`greyed_out_for`] is non-zero — the red font colour;
    /// 4. remember the first row whose character is **not** greyed out (in server order, which for
    ///    ACE is most-recently-played first);
    /// 5. bubble-sort the rows by `wcscmp` of their text;
    /// 6. move every greyed-out row to the end, order otherwise preserved;
    /// 7. select the char-gen slot's row, else the previously selected gid's row, else the row
    ///    from step 4; update the buttons.
    pub fn rebuild_character_list(&mut self, ui: &mut UiSystem, set: &CharacterSet) {
        // 1.
        let previously_selected = self.selected_id;
        self.reset_previously_selected_character_slot(ui);
        self.flush(ui);
        self.char_set = set.clone();

        // 2. The list box's live height.
        let list = self.list_box();
        let (list_w, list_h) = list
            .and_then(|h| ui.node(h))
            .map_or((0, 0), |n| (n.region.box_.width(), n.region.box_.height()));
        let rh = row_height(list_h, set.num_allowed_characters, set.set.len());

        // 3. The live set only; see the module comment.
        let mut rows: Vec<CharacterRow> = Vec::with_capacity(set.set.len());
        for (i, c) in set.set.iter().enumerate() {
            let greyed = greyed_out_for(set, i) != 0;
            let element = list.and_then(|lb| {
                let h = ui
                    .require_env()
                    .and_then(|e| e.create_child_element_by_enum(ui, lb, LAYOUT, LIST_ROW))
                    .ok()?;
                // Resize to the template's own width and the row height — the width it has,
                // which is the list box's own 160.
                let w = ui.node(h).map_or(list_w, |n| n.region.box_.width());
                ui.resize_to(h, w, rh);
                // Mouse-visible: without it the row is not hit-tested and
                // neither the click nor the double-click can happen.
                ui.set_mouse_visible(h, true);
                if let Some(n) = ui.node_mut(h) {
                    // Flag `0x800000`, wants double clicks, which is what makes a mouse-up
                    // raise 0x1A on this element at all.
                    n.flags.set_wants_dbl_clicks(true);
                    // Property `0x10000009` = the character id.
                    n.instance_properties.set(
                        ATTR_ROW_INSTANCE_ID,
                        dereth_assets::ui::PropertyValue::InstanceId(c.id.0),
                    );
                }
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(&c.name);
                }
                if greyed {
                    set_font_color(ui, h, GREYED_OUT_COLOR);
                }
                Some(h)
            });
            rows.push(CharacterRow {
                id: c.id,
                name: c.name.clone(),
                greyed_out: greyed,
                element,
            });
        }
        self.rows_built = list.is_some();

        // 4. The fallback selection, taken **before** the sort.
        let first_live = rows.iter().find(|r| !r.greyed_out).map(|r| r.id);

        // 5. `wcscmp` is a UTF-16 code-unit comparison, so compare the encoded units rather than
        //    the `str`s: they differ above the BMP, and a name is whatever the server sent.
        rows.sort_by(|a, b| {
            let (a, b): (Vec<u16>, Vec<u16>) = (
                a.name.encode_utf16().collect(),
                b.name.encode_utf16().collect(),
            );
            a.cmp(&b)
        });

        // 6. Remove and re-append every greyed row, in order: a
        //    stable partition with the pending deletions at the end.
        rows.sort_by_key(|r| r.greyed_out);

        // The rows' geometry follows their final order. In the client this is the list box's own
        // layout pass over its item list; see the module comment for why it is here.
        for (i, r) in rows.iter().enumerate() {
            if let Some(h) = r.element {
                ui.move_to(h, 0, i32::try_from(i).unwrap_or(0) * rh);
            }
        }
        self.rows = rows;

        // 7. The char-gen slot is read after step 1's reset has set it to -1, so its row is never
        //    the pick; the previously selected id is what restores the selection.
        let pick = usize::try_from(self.chargen_slot)
            .ok()
            .and_then(|s| set.set.get(s))
            .map(|c| c.id)
            .filter(|id| self.rows.iter().any(|r| r.id == *id))
            .or_else(|| {
                Some(previously_selected).filter(|id| self.rows.iter().any(|r| r.id == *id))
            })
            .or(first_live);
        if let Some(id) = pick {
            self.select_character(ui, id);
        } else {
            self.selected = None;
            self.selected_id = ObjectId(0);
        }
        self.apply_button_states(ui);

        // The client's tail, which is the **only** thing that takes the please-wait dialog down:
        // once a character set has been received it updates the world name, rebuilds the list,
        // then closes the please-wait dialog and clears its handle.
        //
        // A wait dialog has no buttons, so DELETE and RESTORE would otherwise leave a modal on
        // screen for ever: the arrival of the next character set is its dismissal.
        self.close_dialog_element(ui, DialogContext::PleaseWait);
        if self.open_dialog == Some(DialogContext::PleaseWait) {
            self.open_dialog = None;
        }
    }

    /// Queue every row for deletion and forget them.
    ///
    /// The elements stay parented until the UI drains the delete queue at the very top of the
    /// next frame's update, which is the client's own lifetime.
    fn flush(&mut self, ui: &mut UiSystem) {
        for r in std::mem::take(&mut self.rows) {
            if let Some(h) = r.element {
                ui.add_to_delete_queue(h);
            }
        }
        self.selected = None;
    }

    /// Clear attribute `0x0E` on the row that
    /// is currently marked selected.
    fn reset_previously_selected_character_slot(&mut self, ui: &mut UiSystem) {
        if let Some(h) = self
            .selected
            .and_then(|s| self.rows.get(s))
            .and_then(|r| r.element)
        {
            crate::bind::set_attr_bool(ui, h, ATTR_ROW_SELECTED, false);
            ui.set_state(h, STATE_ENABLED);
        }
        self.selected = None;
        // The reset also clears the char-gen slot.
        self.chargen_slot = -1;
    }

    /// Select a character — find the row carrying `gid` in attribute `0x10000009`,
    /// mark it, and store the id in [`Self::selected_id`] (and, in the client,
    /// the framework's persistent "selected avatar" and slot).
    ///
    /// The char-gen slot becomes the character's index in the character set as the server sent
    /// it, which is not the row's index: the rows are sorted by name.
    pub fn select_character(&mut self, ui: &mut UiSystem, gid: ObjectId) -> ButtonStates {
        self.reset_previously_selected_character_slot(ui);
        let Some(slot) = self.rows.iter().position(|r| r.id == gid) else {
            self.selected_id = ObjectId(0);
            return self.update_buttons();
        };
        self.selected = Some(slot);
        self.selected_id = gid;
        self.chargen_slot = character_set_slot(&self.char_set, gid);
        if let Some(h) = self.rows[slot].element {
            // Attribute `0xE` true — the list box's "this row is the
            // selected one" attribute, which the row's own states draw.
            crate::bind::set_attr_bool(ui, h, ATTR_ROW_SELECTED, true);
            // State 6 is the highlighted row in the shipped template, which carries states
            // 1, 2, 6 and 7.
            ui.set_state(h, StateId(6));
        }
        self.apply_button_states(ui);
        self.update_buttons()
    }

    /// The index of the selected row, for tests and for the host.
    #[must_use]
    pub fn selected_row(&self) -> Option<&CharacterRow> {
        self.selected.and_then(|s| self.rows.get(s))
    }

    /// [`update_buttons`] against this screen's state.
    #[must_use]
    pub fn update_buttons(&self) -> ButtonStates {
        update_buttons(
            self.selected_row(),
            self.char_set.set.len(),
            self.char_set.num_allowed_characters as usize,
        )
    }

    /// The button update's effect on the elements: a state change for Enter Game and Create, a
    /// visibility change for the mutually exclusive Delete / Restore pair.
    ///
    /// The two buttons occupy the *same* rectangle in the shipped layout — both `0x1000039E` and
    /// `0x1000039F` are at (36, 522), 170 × 99 — which is why swapping them is a visibility change
    /// and not a caption change.
    fn apply_button_states(&mut self, ui: &mut UiSystem) {
        let b = self.update_buttons();
        for (field, enabled) in [
            ("enter_game_button", b.enter_game),
            ("create_character_button", b.create),
        ] {
            if let Some(h) = self.bound.get(field) {
                ui.set_state(
                    h,
                    if enabled {
                        STATE_ENABLED
                    } else {
                        STATE_DISABLED
                    },
                );
            }
        }
        if let Some(h) = self.bound.get("delete_character_button") {
            ui.set_visible(h, !b.restore);
        }
        if let Some(h) = self.bound.get("restore_character_button") {
            ui.set_visible(h, b.restore);
        }
    }

    /// The enter-world request, which retail routes to the player session's log-on.
    ///
    /// It returns early when there is no player session or no selected character, looks up the
    /// selected character's slot, returns early if [`greyed_out_for`] that slot is non-zero, and
    /// otherwise raises the entering-world dialog and logs the character on.
    ///
    /// The greyed-out test is the client's own re-check and is deliberately not "trust the
    /// button state": the button update has already disabled the button, and this refuses anyway.
    fn enter_game(&mut self) {
        if self.selected_id.0 == 0 {
            return;
        }
        let Some(slot) = self
            .char_set
            .set
            .iter()
            .position(|c| c.id == self.selected_id)
        else {
            return;
        };
        if greyed_out_for(&self.char_set, slot) != 0 {
            return;
        }
        // The entering-world dialog — `ID_Character_EnteringWorld`.
        self.open_dialog = Some(DialogContext::EnteringWorld);
        self.actions.push(CharacterAction::LogOn(self.selected_id));
    }

    /// The dialog answers, demultiplexed by which context
    /// handle the closing dialog matches.
    ///
    /// | dialog | on "yes" |
    /// |---|---|
    /// | confirm exit | queue mode `0x10000009` — the epilogue, and then quit |
    /// | delete character | close the delete-character dialog with the typed text |
    /// | the other three | just clear the handle |
    pub fn close_dialog(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        yes: bool,
        typed: &str,
    ) {
        let Some(ctx) = self.open_dialog.take() else {
            return;
        };
        if !yes {
            return;
        }
        match ctx {
            DialogContext::ConfirmExit => {
                requests_out.emit(crate::view::UiRequest::QueueMode(
                    dereth_ui::framework::mode::EPILOGUE,
                ));
            }
            DialogContext::DeleteCharacter => self.close_delete_character_dialog(typed),
            DialogContext::ErrorMessage
            | DialogContext::PleaseWait
            | DialogContext::EnteringWorld => {}
        }
    }

    /// The "type the confirmation phrase" guard.
    ///
    /// The client loads `ID_CharacterManagement_DeleteCharacterResponse` from table `0x10000002`,
    /// narrows it and compares it with `_stricmp`; only an exact (case-insensitive) match puts up
    /// the please-wait dialog and asks the player session to delete the selected avatar.
    ///
    /// The phrase is **localised** and therefore not a constant here: the host resolves it and
    /// hands it in through [`Self::delete_confirmation_phrase`].
    fn close_delete_character_dialog(&mut self, typed: &str) {
        let Some(want) = self.delete_confirmation_phrase.as_deref() else {
            return;
        };
        if !typed.eq_ignore_ascii_case(want) {
            return;
        }
        self.open_dialog = Some(DialogContext::PleaseWait);
        self.actions.push(CharacterAction::Delete(self.selected_id));
    }

    // -------------------------------------------------------------------------------------------
    // The five dialog builds, from the click to an element on screen
    // -------------------------------------------------------------------------------------------
    //
    // Every arm above sets a *context*; the builds below turn it into an element on screen.
    //
    // **What the factory does and does not do.** `DialogController` is the *queue*: make-dialog,
    // open-next-dialog, the pending banner, reset. Element creation is explicitly not its job in
    // this build — create only records, and `pending_create()` reports "what the
    // host still has to instantiate" — and a `Screen` is handed a `&mut UiSystem`, never the
    // `UiFlow` that owns `flow.dialogs`. So raising the element is this side's work, exactly as
    // `GamePlayScreen` does it for its logout confirmation and the wizard for its Exit.

    /// The dialog factory's create-dialog for one context: create a root of layout enum 2 at the
    /// kind's root element, set its data, size and position the popup, and bring it to front.
    ///
    /// Returns whether a dialog was raised, as each retail dialog build's `bool` does — `false`
    /// both for "one is already up" (the empty-handle check every one of them opens with) and for
    /// a host with no dat behind it.
    ///
    /// **The empty-handle guard is load-bearing.** A click on the button behind a modal is already
    /// refused by property `0xAC`, but the character management screen's action handler (action
    /// `0x27`, i.e. **Esc**, raises the confirm-exit dialog; see [`Self::on_action`]) does not test
    /// the press flag, so a press-and-release pair delivers **two** `0x27`s and the second one is
    /// exactly what this refuses. Pressing Escape twice raises one dialog.
    fn make_dialog(&mut self, ui: &mut UiSystem, ctx: DialogContext, text: &str) -> bool {
        if self.dialogs.is_taken(ctx) {
            return false;
        }
        // Recorded **before** the element exists, so a host with no environment installed still
        // sees the arm that was reached — the same order `CharGenScreen::do_exit` uses.
        self.open_dialog = Some(ctx);
        // A dialog build's own body: fill a `PropertyCollection` and hand it to the factory.
        // Property `0x8E` is the kind, `0xAC` is modality (all five set `1`; a wait dialog with no
        // buttons is modal on purpose, because blocking the screen is the whole of what it does)
        // and `0xC5` is the prompt. None of the five sets `0x8D` or `0xC3`, so all five share
        // queue **2** and none of them replaces what is open — which is the behaviour the queue
        // exists to provide.
        let mut data = dereth_ui::PropertyCollection::new();
        data.set(
            dereth_ui::props::attr::DIALOG_KIND,
            PropertyValue::Integer(ctx.kind().property()),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_MODAL,
            PropertyValue::Bool(true),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
            PropertyValue::String(text.to_string()),
        );
        let Some(context) = ui.dialogs.make_dialog(data, ui.now.0) else {
            return false;
        };
        *self.dialogs.slot(ctx) = Some(OpenDialog {
            context,
            element: None,
        });
        // The factory either created this context's dialog or queued it behind the one
        // already open on queue 2. `service_dialog_queue` builds the element for whichever
        // contexts are current, now and on every later frame, so a queued dialog appears when its
        // turn comes rather than never.
        self.service_dialog_queue(ui);
        self.dialogs.get(ctx).is_some()
    }

    /// The client's element half, run for every context that the factory has opened for this
    /// screen and that has no element yet.
    ///
    /// **This is the queue's production caller**, and `pending_create()`'s. The
    /// factory decides *which* dialog is current and *when*; this builds it, because
    /// `DialogController` has no asset source and cannot. The order below is retail's
    /// create-dialog: create the element for property `0x8E`'s root, set its data, size and
    /// position the popup, bring it to front and send the open-dialog notice — the last two inside
    /// [`dereth_ui::UiSystem::bind_dialog_element`].
    ///
    /// **The "which contexts are owed an element" question is asked once, of the factory.**
    /// Re-implementing the `element.is_none()` filter here off `dereth_ui::dialog::info` would be
    /// a second copy of a rule that must not drift. `info()` is still consulted, for the `data` and
    /// the kind that `pending_create()` does not carry.
    fn service_dialog_queue(&mut self, ui: &mut UiSystem) {
        // The factory has created exactly these contexts and none of them has an
        // element yet. A context of ours that is absent is either not raised at all or still
        // waiting behind another dialog on its queue.
        let owed: Vec<u64> = ui
            .dialogs
            .pending_create()
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        for ctx in DIALOG_CONTEXTS {
            let Some(slot) = self.dialogs.at(ctx) else {
                continue;
            };
            if !owed.contains(&slot.context) {
                continue;
            }
            let Some(info) = ui.dialogs.info(slot.context) else {
                continue;
            };
            let data = info.data.clone();
            let root = info.kind.root_element_id();
            let Ok(h) = ui
                .require_env()
                .and_then(|e| e.create_and_add_root_element(ui, DIALOG_LAYOUT, root))
            else {
                continue;
            };
            // The dialog's modal-property (`0xAC`) arm makes it block clicks.
            ui.set_attribute_bool(
                h,
                dereth_ui::props::attr::DIALOG_MODAL,
                data.get_bool(dereth_ui::props::attr::DIALOG_MODAL)
                    .unwrap_or(false),
            );
            // The dialog's text update: property `0xC5`'s string goes on child
            // **`0x3E`**, found recursively — the panel `0x3D` is a
            // container.
            if let Some(PropertyValue::String(text)) = data
                .get(dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT)
                .cloned()
            {
                if let Some(t) = ui
                    .get_child_recursive(h, dereth_ui::dialog::base::child::TEXT)
                    .and_then(|c| ui.text_element_mut(c))
                {
                    t.set_text(&text);
                }
            }
            // Each kind's set-data — the button captions and, for the two menu kinds, the
            // initial selection.
            dereth_ui::dialog::types::set_dialog_data(ui, h, &data);
            // The answer route. **By element id, not by bubbling** — `DialogElement`'s own
            // `listen_to_element_message` returns `StopProcessing` for the ids it recognises, so a
            // listener registered on the dialog root would never be reached; the by-id table is
            // consulted first when an element message is broadcast. This is the same route the
            // logout confirmation takes.
            let (accept, cancel) = ctx.answer_children();
            for id in [accept, cancel].into_iter().flatten() {
                ui.register_for_element_message(id, MessageId(1), ME);
            }
            // Grow the shipped panel to fit the prompt and centre it. Without this, the delete
            // warning shows one
            // line of its five-line paragraph in the top-left corner.
            dereth_ui::dialog::base::update_popup_size_and_position(ui, h);
            // Recorded as one of this screen's roots so use_new_mode deletes it with the
            // screen, matching original screen destruction resetting the dialog.
            self.roots.push(h);
            ui.bind_dialog_element(slot.context, h);
            if let Some(s) = self.dialogs.slot(ctx).as_mut() {
                s.element = Some(h);
            }
        }
    }

    /// The character-management input action callback. It raises the confirm-exit
    /// dialog exactly when action `0x27` arrives and returns whether that action matched.
    ///
    /// Two details are the client's and are reproduced: it **does not test the press flag**, so it
    /// answers the release edge as well as the press (harmless because the confirm-exit build's
    /// own empty-handle check refuses the second), and it returns *whether the action was
    /// `0x27`*, which is what the input dispatcher tests before walking the input-handler list.
    ///
    /// **Action `0x27` is "cancel", i.e. Escape** — `DIK_ESCAPE` is bound to it in maps `0x7` and
    /// `0x9` in both shipped keymaps. It is
    /// *not* `0x10000027`, the game-layer "end character session and quit" action.
    ///
    /// # The producer, and where this build differs from retail
    ///
    /// The original screen constructor ends by registering itself for map 9, so **map 9
    /// (DialogBoxes) is owned by this screen**; the input dispatcher picks it as the winning map
    /// for Escape and calls the action callback on *this object* before shared element input
    /// handling is consulted at all. So this action callback is live, not dead code; the element
    /// manager's own action handler, which is the mouse router, is a different implementation of
    /// the same callback.
    ///
    /// Map 9 is registered for this mode and `UiShell::mode_on_action` — the transcription of "the
    /// winning map's callback gets first refusal" — answers all three pre-game screens. The
    /// client's **global message 1** is one step further down the same chain, and this screen does
    /// not listen for it: `CharacterManagementScreen` registers for **no** global message in
    /// retail.
    ///
    /// **Escape typed into the delete-confirmation dialog's text box does not reach this screen, in
    /// retail or here.** With that box focused the winning callback is the **element**, not the
    /// screen, on either map: the element base registers the element's own input map — which for
    /// that box is 9 — and the text element registers maps 1, 7 and 8 as well. Both go in at the
    /// focus priority 3000, *after* this screen's constructor did, and an equal-priority newcomer
    /// is prepended — so whichever of 7 and 9 wins, the callback is the text element. Its `0x27`
    /// arm then returns **true** unconditionally (flag bit `0x1000` decides only whether a further
    /// handler is called), so the event is consumed and no input handler runs. Retail's screen does
    /// **not** see that Escape either.
    pub fn on_action(&mut self, ui: &mut UiSystem, action: u32) -> bool {
        // The engine-layer "cancel" action, `0x27`. Taken from the one place it is already
        // transcribed rather than written a second time; the literal is pinned in this file's own
        // test so a wrong number there cannot hide behind a shared symbol.
        let hit = action == crate::toolbar::shortcuts::ACTION_CANCEL;
        if hit {
            self.make_confirm_exit_dialog(ui);
        }
        hit
    }

    /// The confirm exit dialog build — property `0x8E` = 1, `0xAC` = true,
    /// `0xC5` = `ID_CharacterManagement_ConfirmExit`.
    pub fn make_confirm_exit_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, CONFIRM_EXIT_STRING);
        self.make_dialog(ui, DialogContext::ConfirmExit, &text)
    }

    /// The delete-character confirmation dialog, in its own order: return false if its handle is
    /// already set; look up the selected character's slot and take the name from the character
    /// set at that slot; set property `0x8E` = 5 and `0xAC` = true; build the prompt from
    /// `ID_CharacterManagement_DeleteCharacterConfirmation` in table `0x10000002` with the name as
    /// its player-name string variable, into `0xC5`; and keep the context the factory returns.
    ///
    /// **The name is taken from the character set by slot, not from the row**, and it is the
    /// only thing on screen that says *which* character a *Done* would delete.
    pub fn make_delete_character_confirmation_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.delete_confirmation_text(ui);
        self.make_dialog(ui, DialogContext::DeleteCharacter, &text)
    }

    /// The please wait dialog build — property `0x8E` = 2 (`Wait`, **no buttons**),
    /// `0xC5` = `ID_CharacterManagement_PleaseWait`.
    ///
    /// It is taken down by the client's tail —
    /// by the **arrival of the next character set**, after updating the world name and rebuilding
    /// the character list. That is
    /// [`Self::rebuild_character_list`] here. There is no way for the player to dismiss one and
    /// that is the client's behaviour, not an omission.
    pub fn make_please_wait_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, PLEASE_WAIT_STRING);
        self.make_dialog(ui, DialogContext::PleaseWait, &text)
    }

    /// The entering world dialog build — property `0x8E` = 2,
    /// `0xC5` = `ID_Character_EnteringWorld`. Original screen destruction clears it; here,
    /// `use_new_mode` deletes this screen's roots on the way into the game.
    pub fn make_entering_world_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, ENTERING_WORLD_STRING);
        self.make_dialog(ui, DialogContext::EnteringWorld, &text)
    }

    /// The character management screen's char gen verification response notice — **the other
    /// thing that closes the please-wait modal**, and the only thing that closes it after a
    /// refusal.
    ///
    /// Retail closes the please-wait dialog first, then maps the response: 3 (name in use), 5
    /// (corrupt) and 6 (database down) each set the matching string id from table `0x10000002`
    /// as the error.
    ///
    /// Three things are load-bearing.
    ///
    /// * The close is **unconditional and first**, on the same wait-dialog handle the
    ///   normal-success tail clears. A wait dialog has no buttons; without this the only exit is
    ///   the normal-success tail, which needs a fresh character set — and ACE sends none after a
    ///   refusal, so the please-wait dialog would stay up for ever.
    /// * There is **no arm for 1 (`OK`)**: all three tests miss and the function returns. A
    ///   successful restore closes the modal and says nothing.
    /// * The error is set through the same entry point the mode switch uses for queueing a mode
    ///   with an error, so the error is raised exactly the way an entry error is.
    ///
    /// The three string ids are [`CHARGEN_VERIFICATION_STRINGS`].
    ///
    /// **Retail folds 5 and 6 one level up**, so case 6 is unreachable in practice: the notice
    /// dispatcher sends both `CORRUPT` and `DATABASE_DOWN` here as the literal **5**. The arm is
    /// kept anyway because it is this function's, not the dispatcher's, and a different sender
    /// could reach it.
    pub fn on_chargen_verification_response(&mut self, ui: &mut UiSystem, response: u32) {
        self.close_dialog_element(ui, DialogContext::PleaseWait);
        if self.open_dialog == Some(DialogContext::PleaseWait) {
            self.open_dialog = None;
        }
        let token = match response {
            3 => CHARGEN_VERIFICATION_STRINGS[0],
            5 => CHARGEN_VERIFICATION_STRINGS[1],
            6 => CHARGEN_VERIFICATION_STRINGS[2],
            _ => return,
        };
        let text = self.string(ui, token);
        // `set_error_msg` is a `Screen` method here because `use_new_mode` also calls it, and
        // it is handed no `UiSystem`; the dialog it asks for is built by `service_dialogs` on the
        // way out of this frame.
        Screen::set_error_msg(self, text);
        self.service_dialogs(ui);
    }

    /// The error message dialog build — property `0x8E` = 3 (`Message`, one button `0x26`). Its text is the resolved string handed to
    /// the screen; the host has already performed the string-table lookup.
    pub fn make_error_message_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let Some(text) = self.error_text.clone() else {
            return false;
        };
        self.make_dialog(ui, DialogContext::ErrorMessage, &text)
    }

    /// The typed delete-confirmation text is assembled from the row's literal pieces with the
    /// confirmation variable's value between them.
    ///
    /// The shipped row has **two** pieces — *"WARNING! "* and *" will be deleted. …type 'DELETE'
    /// in the box below and press the Done button."* — around its single player-name variable, so
    /// the sentence the player reads names the character. With no string table installed this falls
    /// back to the token, as [`Self::string`] does.
    fn delete_confirmation_text(&self, ui: &UiSystem) -> String {
        let name = self
            .char_set
            .set
            .iter()
            .find(|c| c.id == self.selected_id)
            .map_or("", |c| c.name.as_str());
        // The client takes the string lookup's rendered arm, not the plain (no meta-language) arm,
        // including its excess-space trim. This particular row ships **no** double space, so the
        // drawn sentence is the same either way; what differs is that a *name* carrying one does
        // not draw it, and that the row would render a `{a|b}` block if the translators ever put
        // one in.
        ui.resolve_string_rendered(
            STRING_TABLE,
            dereth_primitives::num::hash::str_hash(DELETE_CONFIRMATION_STRING.as_bytes()),
            &[name.to_owned()],
        )
        .unwrap_or_else(|| DELETE_CONFIRMATION_STRING.to_string())
    }

    /// A `StringInfo` in table enum `0x10000002`, resolved if the host has the table — the same
    /// helper `CharGenScreen::string` is.
    fn string(&self, ui: &UiSystem, token: &str) -> String {
        ui.resolve_string(
            STRING_TABLE,
            dereth_primitives::num::hash::str_hash(token.as_bytes()),
        )
        .unwrap_or_else(|| token.to_string())
    }

    /// The client's visible half: delete the element and forget it.
    fn close_dialog_element(&mut self, ui: &mut UiSystem, ctx: DialogContext) {
        let Some(slot) = self.dialogs.slot(ctx).take() else {
            return;
        };
        for id in {
            let (a, c) = ctx.answer_children();
            [a, c]
        }
        .into_iter()
        .flatten()
        {
            ui.unregister_for_element_message(id, MessageId(1), ME);
        }
        if let Some(h) = slot.element {
            self.roots.retain(|r| *r != h);
            ui.remove_and_delete_root(h);
        }
        // The dialog factory's close half: it takes the context off its queue and runs
        // its "open next dialog", so whatever was waiting behind this one becomes current.
        // Without it the queue would fill up and never drain, which is the failure mode a wire
        // that only *raises* dialogs would have.
        ui.dialogs.close_dialog(slot.context, ui.now.0);
        self.service_dialog_queue(ui);
    }

    /// The dialog-close handler, reached from the dialog's own buttons rather than from a
    /// notice, because nothing in this build raises the close-dialog notice.
    ///
    /// The element goes first (retail's close deletes it and *then* notifies), and the context is named explicitly rather than read off
    /// [`Self::open_dialog`] — the client demultiplexes on which of its five handles matched, and
    /// `open_dialog` is only the most recent of them.
    fn answer_dialog(&mut self, ui: &mut UiSystem, ctx: DialogContext, yes: bool, typed: &str) {
        self.close_dialog_element(ui, ctx);
        self.open_dialog = Some(ctx);
        self.close_dialog(&mut ui.requests, yes, typed);
        self.service_dialogs(ui);
    }

    /// Raise the element for a context that has been recorded and has none.
    ///
    /// Three of the client's dialog builds are made from functions this build cannot hand a
    /// `UiSystem` to — the error-message setter (the `Screen` trait's signature),
    /// the delete-character dialog close and the enter-game path — so they record the
    /// context and this raises it on the same frame, from a caller that has one.
    ///
    /// **It is called from two places and either one alone is enough**: removing the call from
    /// either `update` or `on_element_message`'s tail breaks nothing, and removing **both** breaks
    /// three tests. The reason is that this screen registers for element messages on its own root,
    /// so the visibility and state traffic a mode switch generates bubbles into
    /// `on_element_message` and reaches the tail there before `update` runs. Both are kept —
    /// `update` is the only one that fires on a frame with no element traffic at all — and the
    /// redundancy is written down because a later reader deleting "the one that is never needed"
    /// would be picking at random.
    ///
    fn service_dialogs(&mut self, ui: &mut UiSystem) {
        // First, any context the factory has made current since the last frame — a dialog that
        // was queued behind another one gets its element here.
        //
        // **This call is kept deliberately.** Deleting it breaks
        // nothing: `close_dialog_element` services the queue on its own tail, so the promoted
        // dialog gets its element on the way out of the one in front of it and never waits for a
        // frame. What this call covers is the case no test in this build can reach — a dialog
        // promoted by something other than a close, i.e. tick's timeout arm or a
        // `0x8D` replace, neither of which this screen raises. The line stays because the mechanism
        // is real even where the tests cannot exercise it; it is the same redundancy as the one
        // described on this function.
        self.service_dialog_queue(ui);
        let Some(ctx) = self.open_dialog else { return };
        if self.dialogs.is_taken(ctx) {
            return;
        }

        match ctx {
            DialogContext::ErrorMessage => self.make_error_message_dialog(ui),
            DialogContext::ConfirmExit => self.make_confirm_exit_dialog(ui),
            DialogContext::DeleteCharacter => self.make_delete_character_confirmation_dialog(ui),
            DialogContext::PleaseWait => self.make_please_wait_dialog(ui),
            DialogContext::EnteringWorld => self.make_entering_world_dialog(ui),
        };
    }

    /// The typed text a confirmation-text-input dialog is holding — property `0x9C`, which the
    /// client reads as text off child `0x2C`, found recursively, when `0x2E` is pressed.
    fn text_input_contents(ui: &mut UiSystem, dialog: ElemHandle) -> String {
        let Some(h) = ui.get_child_recursive(dialog, TEXT_INPUT_FIELD) else {
            return String::new();
        };
        ui.text_element_mut(h)
            .map(|t| t.glyphs.inq_text(false))
            .unwrap_or_default()
    }

    /// Whether an element id is the *accept* half of a context's pair — the one statement of
    /// "which button is yes", read out of [`DialogContext::answer_children`].
    fn is_accept(ctx: DialogContext, id: ElementId) -> bool {
        ctx.answer_children().0 == Some(id)
    }

    /// The dialog a context has up, for tests and for the host.
    #[must_use]
    pub fn dialog_element(&self, ctx: DialogContext) -> Option<ElemHandle> {
        self.dialogs.get(ctx)
    }

    /// The localised delete-confirmation phrase, resolved by the host out of string table enum
    /// `0x10000002` (`ID_CharacterManagement_DeleteCharacterResponse`).
    pub fn set_delete_confirmation_phrase(&mut self, s: String) {
        self.delete_confirmation_phrase = Some(s);
    }

    /// Take every pending player-session operation, in the order it was requested.
    pub fn take_actions(&mut self) -> Vec<CharacterAction> {
        std::mem::take(&mut self.actions)
    }
}

/// Property `0x1B` = `[colour]` on a row.
///
/// The attribute is an **array** indexed by font number and reads element
/// 0, so the value has to be built as a one-element array and not as a bare colour.
pub(crate) fn set_font_color(ui: &mut UiSystem, h: ElemHandle, argb: u32) {
    use dereth_assets::ui::{BaseProperty, PropertyValue};
    let v = PropertyValue::Array(vec![BaseProperty {
        id: 0x19,
        value: PropertyValue::Color(argb),
    }]);
    if let Some(n) = ui.node_mut(h) {
        n.instance_properties.set(ATTR_FONT_COLOR, v.clone());
    }
    ui.on_set_attribute(h, ATTR_FONT_COLOR, Some(&v));
}

impl Screen for CharacterManagementScreen {
    /// Returns *whether the action was `0x27`*, which is what listener dispatch tests — so Enter
    /// is **declined** here and reaches the registered handler list, which is retail's own answer.
    fn on_mode_action(
        &mut self,
        cx: &mut ScreenCx<'_>,
        e: &dereth_input::InputEvent,
    ) -> Option<dereth_ui::framework::ModeAction> {
        let consumed = self.on_action(cx.ui, e.action.0);
        Some(dereth_ui::framework::ModeAction {
            consumed,
            handled: true,
            queue: None,
        })
    }

    fn on_host_call(&mut self, _cx: &mut ScreenCx<'_>, call: &mut dyn std::any::Any) -> bool {
        use crate::screens::pregame_host::PregameCall;
        let Some(PregameCall::PickCharacterRow { wanted, out }) =
            call.downcast_mut::<PregameCall>()
        else {
            return false;
        };
        *out = if self.rows.is_empty() {
            None
        } else {
            self.rows
                .iter()
                .find(|r| !wanted.is_empty() && r.name.eq_ignore_ascii_case(wanted))
                .or_else(|| self.rows.iter().find(|r| !r.greyed_out))
                .and_then(|r| r.element)
        };
        true
    }

    fn create(&mut self, cx: &mut ScreenCx<'_>) -> Result<(), UiError> {
        let ui = &mut *cx.ui;
        let root = ui
            .require_env()
            .and_then(|e| e.create_and_add_root_element(ui, LAYOUT, ROOT))?;
        self.roots.push(root);
        // The client registers the framework as
        // a listener on the root; the messages then bubble to it from anywhere in the subtree.
        // Without this the screen's `on_element_message` is never called and no button on it
        // does anything.
        ui.register_for_element_messages(root, ME);
        // No `KEY_DOWN_UNCONSUMED` registration here. The constructor's closing registration of
        // input map 9 at priority 3000 is made by `dereth_client_shell::ui::PREGAME_MODE_INPUT_MAPS` and
        // answered by `UiShell::mode_on_action`. `CharacterManagementScreen` registers for **no**
        // global message at all — the retail screen registers for none and has no global-message
        // handler.
        self.bound = bind_children(ui, root, CHILDREN);
        if let Some(name) = self.world_name.clone() {
            self.update_world_name(ui, &name);
        }
        // Original screen setup rebuilds immediately when a character set was already received,
        // such as when returning from character generation or the game. The host pushes the set in; if it
        // already has one this rebuilds against it.
        if !self.char_set.set.is_empty() {
            let set = self.char_set.clone();
            self.rebuild_character_list(ui, &set);
        }
        self.apply_button_states(ui);
        Ok(())
    }

    fn destroy(&mut self, cx: &mut ScreenCx<'_>) {
        let ui = &mut *cx.ui;
        for r in std::mem::take(&mut self.rows) {
            if let Some(h) = r.element {
                ui.add_to_delete_queue(h);
            }
        }
        // Original character-management destruction closes all five dialogs. `use_new_mode`
        // has already deleted their elements with the rest of `roots()`; what is left is this
        // screen's by-id registrations, which are keyed on an `ElementId` shared with every other
        // dialog in the build and must not outlive it.
        self.dialogs = OpenDialogs::default();
        self.open_dialog = None;
        // The ids come from the shared table, one context at a time, so that this list cannot
        // drift from the one `make_dialog` registers — it was a third hand-written copy of the
        // same five ids.
        for ctx in DIALOG_CONTEXTS {
            let (accept, cancel) = ctx.answer_children();
            for id in [accept, cancel].into_iter().flatten() {
                ui.unregister_for_element_message(id, MessageId(1), ME);
            }
        }
    }

    fn set_error_msg(&mut self, s: String) {
        // Retail pops an error dialog through the error-message dialog build.
        self.error_text = Some(s);
        self.open_dialog = Some(DialogContext::ErrorMessage);
    }

    fn on_element_message(&mut self, cx: &mut ScreenCx<'_>, m: &ElementMessage) {
        let ui = &mut *cx.ui;
        use crate::view::UiRequest::QueueMode;
        let button = m.id == dereth_ui::msg::element::id::BUTTON_CLICKED;
        if button && self.message.on_close(ui, m.source_id) {
            return;
        }
        match m.source_id.0 {
            0x1000_03A0 if button => {
                ui.requests
                    .emit(QueueMode(dereth_ui::framework::mode::CHAR_GEN));
            }
            0x1000_03A2 if button => self.enter_game(),
            0x1000_03A3 if button => {
                ui.requests
                    .emit(QueueMode(dereth_ui::framework::mode::CREDITS));
            }
            0x1000_03A4 if button => {
                self.make_confirm_exit_dialog(ui);
            }
            // Both of these refuse without a selection, exactly as the switch does
            // (the list box must have a selected item).
            0x1000_039F if button && self.selected.is_some() => {
                self.make_delete_character_confirmation_dialog(ui);
            }
            0x1000_039E if button && self.selected.is_some() => {
                // The please-wait dialog, then a restore request.
                self.open_dialog = Some(DialogContext::PleaseWait);
                self.actions
                    .push(CharacterAction::Restore(self.selected_id));
            }
            // The list box's own message 4, if a list box ever raises one here.
            0x1000_039D if m.id == MessageId(4) => {
                if let Some(id) = self.rows.get(m.p1 as usize).map(|r| r.id) {
                    self.select_character(ui, id);
                }
            }
            // A row. Message 1 is the click (selection; the client gets this through the list
            // box's message 4 -- see the module comment); `0x1A` is the double-click, which
            // enters the world.
            0x1000_03A5 => {
                let id = ui
                    .node(m.source)
                    .and_then(|n| n.merged_properties().get(ATTR_ROW_INSTANCE_ID).cloned());
                let id = match id {
                    Some(dereth_assets::ui::PropertyValue::InstanceId(v)) => ObjectId(v),
                    _ => return,
                };
                if m.id == dereth_ui::msg::element::id::BUTTON_CLICKED {
                    self.select_character(ui, id);
                } else if m.id == MessageId(0x1A) {
                    self.select_character(ui, id);
                    self.enter_game();
                }
            }
            // ---- the dialogs' own buttons, registered by element id in `make_dialog` -----------
            //
            // One arm per kind rather than one per id, and the accept/cancel decision is taken from
            // [`DialogContext::answer_children`] rather than repeated here — so the table is the
            // single statement of *which button is yes* and a transposition in it is a behaviour
            // change rather than a comment change. Hard-coding `0x17` in this arm would make the
            // table inert: transposing it would change nothing at all.
            //
            // Each arm is guarded on the dialog that owns those ids actually being up, because the
            // registration is by **element id** and therefore global: a stale `0x17` from anywhere
            // else must not be read as an answer to a confirmation that is not on screen.
            //
            // The confirmation dialog: **button 1 is yes** (property 0x92 is set to whether
            // the id is `0x17`).
            0x17 | 0x19 if button && self.dialogs.confirm_exit.is_some() => {
                let yes = Self::is_accept(DialogContext::ConfirmExit, m.source_id);
                self.answer_dialog(ui, DialogContext::ConfirmExit, yes, "");
            }
            // The message dialog: its one button just closes.
            0x26 if button && self.dialogs.error_message.is_some() => {
                self.error_text = None;
                let yes = Self::is_accept(DialogContext::ErrorMessage, m.source_id);
                self.answer_dialog(ui, DialogContext::ErrorMessage, yes, "");
            }
            // The confirmation text input dialog. *Done* (`0x2E`) harvests child `0x2C`'s
            // text into property `0x9C` and closes; *Cancel* (`0x2F`) writes an **empty**
            // `0x9C` and closes.
            //
            // Both then run — the dialog-close handler's
            // case 5 has no yes/no of its own — and an empty string fails its `_stricmp`. So
            // *Cancel* deletes nothing for the same reason a typo does, which is the client's own
            // arrangement and not a second guard bolted on here.
            0x2E | 0x2F if button && self.dialogs.get(DialogContext::DeleteCharacter).is_some() => {
                let d = self
                    .dialogs
                    .get(DialogContext::DeleteCharacter)
                    .expect("guarded above");

                let typed = if Self::is_accept(DialogContext::DeleteCharacter, m.source_id) {
                    Self::text_input_contents(ui, d)
                } else {
                    String::new()
                };
                self.answer_dialog(ui, DialogContext::DeleteCharacter, true, &typed);
            }
            _ => {}
        }
        // Enter Game's entering-world dialog and RESTORE's please-wait dialog, neither of
        // which is handed a `UiSystem` where it is called.
        self.service_dialogs(ui);
    }

    fn update(
        &mut self,
        cx: &mut ScreenCx<'_>,
        _now: dereth_primitives::LocalTime,
    ) -> Option<UiMode> {
        let ui = &mut *cx.ui;
        // The client's error-message dialog, raised here because the `Screen`
        // trait hands `set_error_msg` no `UiSystem` — `use_new_mode` calls it between
        // constructing the framework and showing it. See [`Self::service_dialogs`] for why this
        // and `on_element_message`'s tail are individually unfalsifiable and jointly not.
        self.service_dialogs(ui);
        None
    }

    /// The player-session operations this frame's handlers asked for, in order, to the host —
    /// and the selected-character id, mirrored into the persistent selected avatar.
    ///
    /// **The writing half of the selection round trip.** Selecting a character writes
    /// the persistent selected-avatar id and resetting the previously selected slot writes it
    /// back to 0, both *synchronously* inside the handler; this screen has no handle on the
    /// flow's persistent data, so the mirror is taken off the screen's selected id instead — the
    /// field both of those functions assign in the same breath. The shell flushes after the
    /// frame's deliveries and before the mode switch, so a click and the mode switch it causes
    /// cannot be separated by a frame boundary.
    fn flush_to_host(&mut self, cx: &mut ScreenCx<'_>) {
        for a in self.take_actions() {
            cx.ui
                .requests
                .emit(crate::view::UiRequest::CharacterAction(a));
        }
        cx.ui
            .requests
            .emit(crate::view::UiRequest::SelectedAvatar(self.selected_id));
        // The char-gen slot is written in the same breath as the selected avatar. Not before
        // the screen has read the persistent value: a screen that has selected nothing yet must
        // not overwrite what the last one left.
        if self.slot_seeded {
            cx.ui
                .requests
                .emit(crate::view::UiRequest::CharGenSlot(self.chargen_slot));
        }
    }

    fn on_pregame(
        &mut self,
        cx: &mut ScreenCx<'_>,
        p: &dereth_ui::framework::PregameCx<'_>,
    ) -> Option<UiMode> {
        let host = p.view;
        let ui = &mut *cx.ui;
        // Update world name into element `0x1000039B`.
        if let Some(name) = host.world_name.as_deref() {
            self.update_world_name(ui, name);
        }
        // The world's message to the players choosing a character, in its own window.
        if let (Some(text), Some(root)) = (
            host.character_screen_message.as_deref(),
            self.roots.first().copied(),
        ) {
            self.message.show(ui, root, text);
        }
        // The delete-character dialog's localised phrase, from table enum `0x10000002`. The
        // client resolves it through its string table; the host hands the table over.
        if self.delete_confirmation_phrase.is_none() {
            let phrase = p.ui_strings.and_then(|t| {
                ui.resolve_string(
                    t,
                    dereth_primitives::num::hash::str_hash(DELETE_CHARACTER_RESPONSE.as_bytes()),
                )
            });
            if let Some(t) = phrase {
                self.set_delete_confirmation_phrase(t);
            }
        }
        // At the end of the original construction, a received persistent character
        // set copies the persistent selected-avatar id into the screen's selected-character state
        // before rebuilding the list. That is the *only* reason a character screen entered after a
        // log-off highlights the character that was just played: the persistent-data object
        // survives mode changes. The screen is constructed by a bare `fn() -> Box<dyn Screen>` with
        // no handle on the flow, so the assignment is made here — before the rebuild, in the same
        // order as the original constructor, and once per screen because construction runs once.
        if !self.slot_seeded {
            self.slot_seeded = true;
            self.chargen_slot = p.chargen_slot;
        }
        if p.received_set {
            self.take_selected_avatar_from_persistent_data(p.selected_avatar);
        }
        // The persistent character-set notice followed by the UI-flow update rebuilds the
        // character list. The set is a notice, so the rebuild happens on the **edge**:
        // rebuilding every frame would delete and recreate every row and throw the selection away.
        if p.received_set && self.char_set != *p.char_set {
            self.rebuild_character_list(ui, p.char_set);
        }
        // **The character-generation verification notice.** Both character
        // management and the wizard override the same notice slot, so the char-gen verification
        // notice reaches *both* screens. It is the only thing that takes the please-wait modal
        // down after a refused RESTORE, because ACE answers a refusal with `0xF643` and nothing
        // else — no `0xF658` for the update's tail to act on.
        //
        // **Ordered after the rebuild**, matching the client: verification raises the char-gen
        // notice before the character-set notice. Both close the modal, so on the successful arm
        // the second close is a no-op either way; on the refused arm only this one runs.
        if let (Some(code), true) = (host.chargen_response, p.chargen_response_changed) {
            self.on_chargen_verification_response(ui, code);
        }
        None
    }

    fn roots(&self) -> &[ElemHandle] {
        &self.roots
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_ui::persist::CharacterIdentity;

    fn ident(i: u32) -> CharacterIdentity {
        CharacterIdentity {
            id: ObjectId(0x5000_0000 + i),
            name: format!("Char{i}"),
            seconds_grace_period: 0,
        }
    }

    fn set(live: u32, pending: u32, allowed: u32) -> CharacterSet {
        // ACE puts a pending deletion in the live set with a non-zero grace period and always
        // writes a zero-length `delSet`, so that is what a test set looks like.
        let mut s: Vec<CharacterIdentity> = (0..live).map(ident).collect();
        for i in live..live + pending {
            s.push(CharacterIdentity {
                seconds_grace_period: 3600,
                ..ident(i)
            });
        }
        CharacterSet {
            set: s,
            del_set: Vec::new(),
            num_allowed_characters: allowed,
            ..CharacterSet::default()
        }
    }

    fn row(greyed: bool) -> CharacterRow {
        CharacterRow {
            id: ObjectId(1),
            name: "x".into(),
            greyed_out: greyed,
            element: None,
        }
    }

    /// Oracle: the screen catalogue's control table and the layout-enum map (enum
    /// `0x10000005` → `0x21000004` `charactermanagement`, roots `0x1000039A` and `0x100003A5`).
    #[test]
    fn the_screen_names_the_documented_layout_root_and_controls() {
        assert_eq!(LAYOUT, LayoutEnum(0x1000_0005));
        assert_eq!(ROOT, ElementId(0x1000_039A));
        let ids: Vec<u32> = CHILDREN.iter().map(|c| c.id.0).collect();
        assert_eq!(
            ids,
            vec![
                0x1000_039B,
                0x1000_039D,
                0x1000_039E,
                0x1000_039F,
                0x1000_03A0,
                0x1000_03A2,
                0x1000_03A3,
                0x1000_03A4
            ]
        );
        // 0x100003A1 is in the switch with no handler, so it is deliberately not bound.
        assert!(!CHILDREN.iter().any(|c| c.id == UNHANDLED_SWITCH_CASE));
        assert_eq!(INPUT_MAP, 9);
    }

    /// the three rules, each exercised in both
    /// directions.
    #[test]
    fn update_buttons_follows_the_three_documented_rules() {
        // Nothing selected: nothing to do with a character is possible.
        let b = update_buttons(None, 2, 5);
        assert_eq!(
            b,
            ButtonStates {
                enter_game: false,
                delete: false,
                restore: false,
                create: true
            }
        );

        // A live character: enter and delete, but not restore.
        let b = update_buttons(Some(&row(false)), 2, 5);
        assert!(b.enter_game);
        assert!(b.delete);
        assert!(!b.restore);

        // A greyed-out (pending-delete) character: restore only, and no entering the world.
        let b = update_buttons(Some(&row(true)), 2, 5);
        assert!(!b.enter_game);
        assert!(!b.delete);
        assert!(b.restore, "delete and restore are mutually exclusive");

        // Create is gated on the allowance.
        assert!(update_buttons(None, 4, 5).create);
        assert!(!update_buttons(None, 5, 5).create);
        assert!(!update_buttons(None, 6, 5).create);
    }

    /// Oracle: the client's body, including the slot-cap guard.
    #[test]
    fn greyed_out_reads_the_live_set_and_respects_the_slot_cap() {
        let s = set(2, 1, 5);
        assert_eq!(greyed_out_for(&s, 0), 0);
        assert_eq!(
            greyed_out_for(&s, 2),
            3600,
            "the pending deletion is row 2 of the live set"
        );
        assert_eq!(greyed_out_for(&s, 9), 0, "out of range");
        // A character past the cap is never greyed.
        let capped = CharacterSet {
            num_allowed_characters: 2,
            ..s.clone()
        };
        assert_eq!(greyed_out_for(&capped, 2), 0);
        // ...and a zero cap disables the guard entirely.
        let uncapped = CharacterSet {
            num_allowed_characters: 0,
            ..s
        };
        assert_eq!(greyed_out_for(&uncapped, 2), 3600);
    }

    /// Oracle: the client's two ternaries, both of which select the larger
    /// operand. The shipped `charactermanagement` list box is 160 × 320 and its attribute `0x60`
    /// is 20.
    #[test]
    fn the_row_height_is_the_larger_of_the_two_divisions() {
        // ACE's default `max_chars_per_account` is 11: 320/11 = 29, 320/20 = 16 -> 29.
        assert_eq!(row_height(320, 11, 3), 29);
        // A five-slot account: 320/5 = 64.
        assert_eq!(row_height(320, 5, 3), 64);
        // Past twenty slots the floor takes over: 320/40 = 8, and 320/20 = 16 wins.
        assert_eq!(row_height(320, 40, 3), 16);
        // The row count beats the cap when it is larger, which is the inner `max`.
        assert_eq!(row_height(320, 2, 8), 40);
        // Degenerate inputs do not divide by zero. The client would divide by zero.
        // An empty account with a zero slot cap is not a state `0xF658` can carry, and the guard
        // here is a guard, not a claim about the original.
        assert_eq!(row_height(320, 0, 0), 320);
    }

    /// the list is filled from the framework's persistent character set, and
    /// ENTER (or a row double-click) enters the world with the selected character.
    ///
    /// This runs with no environment installed, so no row elements exist; what it asserts is the
    /// *model* half — which rows, in what order, greyed how, and what Enter emits.
    #[test]
    fn feeding_a_character_set_populates_the_list_and_enter_emits_the_request() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = CharacterManagementScreen::default();
        s.rebuild_character_list(&mut ui, &set(3, 1, 5));
        assert_eq!(
            s.rows.len(),
            4,
            "three live and one pending deletion, all from the live set"
        );
        assert!(!s.rows[2].greyed_out);
        assert!(s.rows[3].greyed_out, "the pending deletion is last");

        // The fallback selection is the first non-greyed character in *server* order.
        assert_eq!(s.selected_id, ObjectId(0x5000_0000));

        let b = s.select_character(&mut ui, ObjectId(0x5000_0001));
        assert!(b.enter_game);
        // Selecting the pending-delete row offers Restore instead.
        let b = s.select_character(&mut ui, ObjectId(0x5000_0003));
        assert!(!b.enter_game && b.restore);
        s.select_character(&mut ui, ObjectId(0x5000_0001));

        let msg = |id: u32, m: u32| ElementMessage {
            source_id: ElementId(id),
            source: ElemHandle::for_test(1),
            id: MessageId(m),
            p1: 0,
            p2: 0,
            point: dereth_ui::msg::MessagePoint::default(),
            serial: 1,
        };
        s.take_actions();
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_03A2, 1),
        );
        assert_eq!(
            s.take_actions(),
            vec![CharacterAction::LogOn(ObjectId(0x5000_0001))]
        );
        assert_eq!(s.open_dialog, Some(DialogContext::EnteringWorld));

        // A greyed-out character can never be entered, even by asking directly.
        s.select_character(&mut ui, ObjectId(0x5000_0003));
        s.open_dialog = None;
        s.enter_game();
        assert!(s.take_actions().is_empty(), "the greyed-out check refuses");
        assert_eq!(s.open_dialog, None);
    }

    /// Oracle: the list rebuild's tail — a `wcscmp` bubble sort followed by moving every
    /// greyed row to the end, and the `0xF658` note "characters appear in the list ordered
    /// most-recently-used first, but are **displayed alphabetically**".
    #[test]
    fn the_rows_are_sorted_by_name_with_the_pending_deletions_last() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = CharacterManagementScreen::default();
        let named = |n: &str, gid: u32, grace: u32| CharacterIdentity {
            id: ObjectId(gid),
            name: n.into(),
            seconds_grace_period: grace,
        };
        s.rebuild_character_list(
            &mut ui,
            &CharacterSet {
                // Server order: most recently played first, and one pending deletion in the middle.
                set: vec![
                    named("+Aldwyne", 2, 0),
                    named("+Alba", 3, 900),
                    named("+Aldis", 1, 0),
                ],
                num_allowed_characters: 11,
                ..CharacterSet::default()
            },
        );
        let names: Vec<&str> = s.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["+Aldis", "+Aldwyne", "+Alba"],
            "alphabetical, greyed last"
        );
        assert!(s.rows[2].greyed_out);
        // The fallback selection was taken before the sort, in server order: +Aldwyne is the first
        // row of the live set that is not pending deletion.
        assert_eq!(s.selected_id, ObjectId(2));
    }

    /// Create goes to char-gen, Credits to the credits screen, and
    /// Quit/Delete/Restore open dialogs rather than acting directly.
    ///
    /// With no environment installed the dialog builds record their context and build nothing,
    /// which is what this asserts. This does not exercise the environment-backed element path in
    /// [`CharacterManagementScreen::make_dialog`].
    #[test]
    fn the_buttons_queue_the_documented_modes_and_open_the_documented_dialogs() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = CharacterManagementScreen::default();
        s.rebuild_character_list(&mut ui, &set(2, 1, 5));
        let msg = |id: u32| ElementMessage {
            source_id: ElementId(id),
            source: ElemHandle::for_test(1),
            id: dereth_ui::msg::element::id::BUTTON_CLICKED,
            p1: 0,
            p2: 0,
            point: dereth_ui::msg::MessagePoint::default(),
            serial: 1,
        };
        ui.requests.clear();
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_03A0),
        );
        assert_eq!(
            ui.requests.take(),
            vec![crate::view::UiRequest::QueueMode(
                dereth_ui::framework::mode::CHAR_GEN
            )]
        );
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_03A3),
        );
        assert_eq!(
            ui.requests.take(),
            vec![crate::view::UiRequest::QueueMode(
                dereth_ui::framework::mode::CREDITS
            )]
        );
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_03A4),
        );
        assert_eq!(s.open_dialog, Some(DialogContext::ConfirmExit));
        assert!(ui.requests.take().is_empty(), "Quit asks first");
        // Confirming the exit is what queues the epilogue.
        s.close_dialog(&mut ui.requests, true, "");
        assert_eq!(
            ui.requests.take(),
            vec![crate::view::UiRequest::QueueMode(
                dereth_ui::framework::mode::EPILOGUE
            )]
        );
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_039F),
        );
        assert_eq!(s.open_dialog, Some(DialogContext::DeleteCharacter));
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_039E),
        );
        assert_eq!(s.open_dialog, Some(DialogContext::PleaseWait));
        assert_eq!(
            s.take_actions(),
            vec![CharacterAction::Restore(ObjectId(0x5000_0000))]
        );
    }

    /// Oracle: — the typed phrase is compared with
    /// `_stricmp` against a **localised** string, and only an exact match deletes.
    ///
    /// **What this test is and is not.** It runs with no environment installed, so no dialog
    /// element can exist and the context is written by hand. That hand-write used to be the
    /// *only* writer of `open_dialog` that anything ever read — the shape
    /// the stated testability rule calls *"a test fixture standing in for the missing producer"*. The
    /// producer is now [`CharacterManagementScreen::make_delete_character_confirmation_dialog`],
    /// but this test exercises only the comparison rule, not the physical click and character path.
    #[test]
    fn deleting_a_character_needs_the_localised_phrase_typed_back() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = CharacterManagementScreen::default();
        s.rebuild_character_list(&mut ui, &set(2, 0, 5));
        s.select_character(&mut ui, ObjectId(0x5000_0001));
        s.set_delete_confirmation_phrase("Delete".into());

        s.take_actions();
        s.open_dialog = Some(DialogContext::DeleteCharacter);
        s.close_dialog(&mut ui.requests, true, "nope");
        assert!(
            s.take_actions().is_empty(),
            "a wrong phrase deletes nothing"
        );

        s.open_dialog = Some(DialogContext::DeleteCharacter);
        s.close_dialog(&mut ui.requests, true, "dELETe");
        assert_eq!(
            s.take_actions(),
            vec![CharacterAction::Delete(ObjectId(0x5000_0001))],
            "case-insensitive, as `_stricmp` is"
        );
        assert_eq!(s.open_dialog, Some(DialogContext::PleaseWait));

        // Answering "no" does nothing at all.
        s.open_dialog = Some(DialogContext::DeleteCharacter);
        s.close_dialog(&mut ui.requests, false, "Delete");
        assert!(s.take_actions().is_empty());
    }

    /// Each dialog context names the kind and the buttons its subclass watches.
    #[test]
    fn each_dialog_context_names_the_kind_and_the_buttons_its_subclass_watches() {
        use dereth_ui::dialog::DialogKind as K;
        let rows = [
            (
                DialogContext::ErrorMessage,
                K::Message,
                0x24_u32,
                Some(0x26_u32),
                None,
            ),
            (
                DialogContext::ConfirmExit,
                K::Confirmation,
                0x15,
                Some(0x17),
                Some(0x19),
            ),
            (
                DialogContext::DeleteCharacter,
                K::ConfirmationTextInput,
                0x2C,
                Some(0x2E),
                Some(0x2F),
            ),
            (DialogContext::PleaseWait, K::Wait, 0x31, None, None),
            (DialogContext::EnteringWorld, K::Wait, 0x31, None, None),
        ];
        for (ctx, kind, root, accept, cancel) in rows {
            assert_eq!(ctx.kind(), kind, "{ctx:?}");
            assert_eq!(ctx.kind().root_element_id().0, root, "{ctx:?}");
            let (a, c) = ctx.answer_children();
            assert_eq!(a.map(|e| e.0), accept, "{ctx:?} accept");
            assert_eq!(c.map(|e| e.0), cancel, "{ctx:?} cancel");
        }
        // The five string tokens, which the shipped table has to answer.
        assert_eq!(CONFIRM_EXIT_STRING, "ID_CharacterManagement_ConfirmExit");
        assert_eq!(
            DELETE_CONFIRMATION_STRING,
            "ID_CharacterManagement_DeleteCharacterConfirmation"
        );
        assert_eq!(
            DELETE_RESPONSE_STRING,
            "ID_CharacterManagement_DeleteCharacterResponse"
        );
        assert_eq!(PLEASE_WAIT_STRING, "ID_CharacterManagement_PleaseWait");
        assert_eq!(ENTERING_WORLD_STRING, "ID_Character_EnteringWorld");
        assert_eq!(DIALOG_LAYOUT.0, 2, "the Dialog layout enum");
        assert_eq!(STRING_TABLE.0, 0x2300_0002);
    }

    /// Oracle: the description's sentence on the character-generation verification-response notice.
    #[test]
    fn the_three_chargen_verification_strings_are_the_documented_ids() {
        assert_eq!(CHARGEN_VERIFICATION_STRINGS.len(), 3);
        assert!(CHARGEN_VERIFICATION_STRINGS[0].ends_with("NAME_IN_USE"));
        assert!(CHARGEN_VERIFICATION_STRINGS[1].ends_with("CORRUPT"));
        assert!(CHARGEN_VERIFICATION_STRINGS[2].ends_with("DATABASE_DOWN"));
    }
}
