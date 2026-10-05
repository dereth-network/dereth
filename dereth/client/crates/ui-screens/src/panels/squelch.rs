//! The Squelch panel: the list, the name box and the three buttons.
//!
//! `element_types` registers `0x10000047`, `panels::catalogue` carries a five-child row for it and
//! `panels::rows` names its `0x1000008F` row attribute; this module is the constructor and
//! behaviour behind them. The shape is [`super::friends`]'s exactly.
//!
//! # The client's own functions, and where they are here
//!
//! | client | here |
//! |---|---|
//! | post-init — five child lookups, one notice registration | [`SquelchPanel::post_init`] |
//! | display refresh — flush, deselect, the squelch iteration, the button update | [`SquelchPanel::update`] |
//! | display insert — sorted insert, text, `0x1000008F`, the row's state | [`SquelchPanel::add_squelch_display`] |
//! | sorted insert position — one alphabetical block | `SquelchPanel::insert_position` |
//! | button update — Remove from the selection, the two squelch buttons from the cap | [`SquelchPanel::update_buttons`] |
//! | element-message handler — three buttons, the list, the name box | [`SquelchPanel::on_element_message`] |
//! | squelch-panel notice — straight to the display refresh | the per-frame [`SquelchPanel::update`] |
//!
//! # What the list actually holds
//!
//! The display refresh does not walk a list of its own. It iterates the communication system's
//! **character** squelch hash. The separate account hash is never iterated by this panel.
//!
//! Each step yields two values read from the stored squelch entry:
//!
//! The iterator copies the zone-squelch flag and name, ending the walk at an empty entry.
//! Otherwise it returns the name through the first output and the saved zone-squelch flag
//! through the second output.
//!
//! **The zone-squelch flag is the account flag, and the client does read it.** The client reads it, the display refresh turns it into the display insert's
//! third argument, and that argument is what picks the row's state between
//! [`ROW_STATE_CHARACTER`] and [`ROW_STATE_ACCOUNT`] — which is in turn the only thing the Remove
//! button reads to decide between `0x0059` and `0x0058`. A server may send the account hash
//! **empty**, folding account squelches into the character hash with this flag set — and then
//! the flag is the only way an account squelch is visible to the client at all.
//!
//! # The panel's private squelch list is a field retail never writes
//!
//! The button update and the keystroke arm both test the private list's element count against
//! `0x32`. The Squelch panel never pushes to that list: construction initializes it, destruction
//! releases it, and nothing in between touches it. It mirrors the Friends panel's list
//! because the button code was copied, and it is
//! **permanently zero** — so the two squelch buttons are never greyed by the cap. That is
//! transcribed rather than corrected: see [`SquelchPanel::squelch_list_len`].
//!
//! # The row's identity is its **text**, not its attribute
//!
//! Adding a squelch display row writes instance-id attribute `0x1000008F` from the id argument
//! — not the name pointer — and its one production caller passes **0**:
//!
//! Each display row receives the widened name, literal instance id zero, and the iterator's
//! account-squelch output. The instance id is zero for every row.
//!
//! So `0x1000008F` carries zero on every row and nothing can be identified by it. The Remove
//! button reads the text child at `0x10000542` and the
//! **state** of that same text element, and sends a *name*. `panels::rows::ROW_ATTRIBUTES` names
//! `0x1000008F` and this panel writes it, but the row is identified by its text.

use dereth_primitives::ObjectId;
use dereth_ui::{ElemHandle, ElementId, ElementType, StateId, UiSystem};

use super::listbox::ListBoxWidget;
use crate::view::{GameView, SquelchEntry, UiRequest};

/// The squelch panel's element class registration, type `0x10000047`.
///
/// Found by **type** rather than by id, for the reason [`super::friends::PANEL_TYPE`] is:
/// the post-init binds five *children* and never names its own id, which is layout data.
/// (It is `0x1000054A` in the shipped `0x21000005` tree; that is a measurement, not a
/// constant of the client.)
pub const PANEL_TYPE: ElementType = ElementType(0x1000_0047);

/// The squelch list box — the fourth recursive child lookup, checked as a list box (type 5).
pub const SQUELCH_LIST: ElementId = ElementId(0x1000_053E);
/// The name edit box — the fifth, checked as a text element (type `0xC`).
pub const NAME_ENTRY_BOX: ElementId = ElementId(0x1000_0540);
/// The Remove button — the third.
pub const REMOVE_BUTTON: ElementId = ElementId(0x1000_0547);
/// The Squelch Character button — the **first**.
pub const SQUELCH_CHARACTER_BUTTON: ElementId = ElementId(0x1000_054B);
/// The Squelch Account button — the second.
pub const SQUELCH_ACCOUNT_BUTTON: ElementId = ElementId(0x1000_054C);

/// The row template's text child — the display insert looks it up under the new row — and the
/// element whose *text* is the row's identity and whose *state* is the account flag.
pub const ROW_NAME: ElementId = ElementId(0x1000_0542);

/// The row's instance-id attribute. Written with **zero** on every row;
/// see the module header for why it is written at all.
pub const ATTR_ROW_INSTANCE_ID: u32 = 0x1000_008F;

/// The list box template a row is created from — the list carries exactly one template,
/// `catalogue::SQUELCH_TEMPLATES`' `0x10000542`.
pub const SQUELCH_ROW_TEMPLATE: usize = 0;

/// The row text element's state for a **character** squelch.
pub const ROW_STATE_CHARACTER: StateId = StateId(0x1000_0056);
/// …and for an **account** squelch, and the one value the Remove button tests the row's state
/// against.
pub const ROW_STATE_ACCOUNT: StateId = StateId(0x1000_0057);

/// The modify-character-squelch event's `(add, id, name, type)` fourth operand as this
/// panel sends it — `1`, i.e. `AllChannels`.
///
/// The panel has **no per-channel control at all**: the post-init binds five children and not
/// one of them is a check box, and the modify-global-squelch event has three
/// senders in the client (the chat toggle, the no-tell toggle and
/// the global squelch modification) and **none of them is the Squelch panel**. Per-channel and
/// global squelch belong to the chat-options page and the `@chat`/`@notell` commands.
pub const ALL_CHANNELS: u32 = 1;

/// The modify-character-squelch event's second operand as this panel sends it — a literal `0` for
/// both Squelch Character and Remove. The shard is asked
/// to squelch a **name**, not an id: the server reads the guid and falls back to the name when
/// the guid is zero.
pub const NO_CHARACTER_ID: ObjectId = ObjectId(0);

/// One drawn row, as this panel wrote it — so a test can read back what a player would see
/// without walking the element tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SquelchRow {
    /// What went into the row's [`ROW_NAME`] text.
    pub name: String,
    /// Which of [`ROW_STATE_ACCOUNT`] / [`ROW_STATE_CHARACTER`] that text element was put into.
    pub account: bool,
    pub element: ElemHandle,
}

/// The Squelch panel, bound to a live tree.
#[derive(Debug, Default)]
pub struct SquelchPanel {
    /// The squelch list box.
    pub list: Option<ListBoxWidget>,
    /// The Squelch Character button.
    pub squelch_character_button: Option<ElemHandle>,
    /// The Squelch Account button.
    pub squelch_account_button: Option<ElemHandle>,
    /// The Remove button.
    pub remove_button: Option<ElemHandle>,
    /// The name edit box.
    pub name_entry: Option<ElemHandle>,
    /// The Squelch panel element itself, found by [`PANEL_TYPE`].
    pub panel: Option<ElemHandle>,
    /// The rows on screen, in list order — one alphabetical block.
    rows: Vec<SquelchRow>,
    /// The list box's selected item, as an index into [`Self::rows`].
    pub selected: Option<usize>,
    /// The snapshot [`Self::update`] last drew, so an unchanged frame redraws nothing.
    last: Option<Vec<SquelchEntry>>,
    /// How many times [`Self::update`] rebuilt the list. **Three states, not two**: this is what
    /// separates "rebuilt and nobody is squelched" — which is every recorded session, because
    /// `0x01F4` arrives at login with an empty DB — from "never ran".
    pub rebuilds: u32,
    /// What [`Self::update_buttons`] last wrote — Remove, Squelch Character, Squelch Account — so
    /// a test can read the three decisions without a tree. `None` until the first refresh.
    pub button_states: Option<[bool; 3]>,
    /// The count of a separate capability list. **Retail never populates it.**
    ///
    /// The original client's panel carried a friend-data-style capability list separate from the
    /// visible list box. Its lifecycle initializes and destroys the list, but no intervening path
    /// adds an entry. The refresh path fills the *list box* from the communication database and
    /// never touches this capability list. The Friends panel, whose behavior this panel copied,
    /// does populate its corresponding list.
    ///
    /// It is kept as a field rather than folded into a `0 < 0x32` constant so that the two readers
    /// retail has (the button update and the keystroke arm) read the same thing retail reads, and so
    /// that the day somebody finds the writer there is one place to put it.
    pub squelch_list_len: usize,
    /// Presses of Squelch Character that reached the `0x1000054B` arm and raised a
    /// [`UiRequest::ModifyCharacterSquelch`].
    pub character_squelch_requests: u32,
    /// Presses of Squelch Account that reached the `0x1000054C` arm and raised a
    /// [`UiRequest::ModifyAccountSquelch`].
    pub account_squelch_requests: u32,
    /// Presses of Remove that reached the `0x10000547` arm and raised the un-squelch of whichever
    /// kind the selected row's state named.
    pub remove_requests: u32,
}

impl SquelchPanel {
    /// The squelch panel's post-init, minus the one notice registration (the squelch-panel
    /// update notice) — this build has no notice bus at this seam, and the
    /// same write arrives as the per-frame [`Self::update`] snapshot. The communication system's
    /// squelch-DB set and clear are the only two raisers, and
    /// both are already transcribed in `dereth_client_model::chat` as writes to the model this reads.
    ///
    /// Bound off the screen **root** rather than off a panel page, on the same terms as
    /// [`super::friends::FriendsPanel::post_init`]: every id here is unique in the shipped tree.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        *self = Self::default();
        self.panel = find_panel(ui, root);
        let from = self.panel.unwrap_or(root);
        // In the post-init's own order.
        self.squelch_character_button = ui.get_child_recursive(from, SQUELCH_CHARACTER_BUTTON);
        self.squelch_account_button = ui.get_child_recursive(from, SQUELCH_ACCOUNT_BUTTON);
        self.remove_button = ui.get_child_recursive(from, REMOVE_BUTTON);
        self.list = ui
            .get_child_recursive(from, SQUELCH_LIST)
            .map(|h| ListBoxWidget::bind(ui, h));
        self.name_entry = ui.get_child_recursive(from, NAME_ENTRY_BOX);
    }

    /// True once the list box was found — the one binding without which nothing can be drawn.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.list.is_some()
    }

    /// The rows this panel last wrote, in list order.
    #[must_use]
    pub fn rows(&self) -> &[SquelchRow] {
        &self.rows
    }

    /// The names on screen, in list order — what a test asserts against instead of a pixel.
    #[must_use]
    pub fn shown(&self) -> Vec<String> {
        self.rows.iter().map(|r| r.name.clone()).collect()
    }

    /// The squelch panel's display refresh, guarded on the snapshot.
    ///
    /// Retail flushes the list box, clears its selection, iterates the squelch entries adding a
    /// row for each (widened name, id `0`, the account flag), then runs the button update.
    ///
    /// The iteration itself is `dereth_client_model::chat::ChatState::squelch_iteration` — it is the
    /// communication system's function, not the panel's — and reaches this crate as
    /// [`GameView::squelch_list`]. An empty list is **not** an early return: it flushes and counts
    /// a rebuild, because a character whose shard clears the DB must see the panel empty rather
    /// than see the old rows. That is the ordinary case: the refresh runs on every
    /// character change and the login `0x01F4` in every recorded session carries nothing.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let entries = view.squelch_list();
        if self.last.as_ref() == Some(&entries) {
            return false;
        }
        self.rows.clear();
        self.selected = None;
        if let Some(mut list) = self.list.take() {
            list.flush(ui);
            self.list = Some(list);
        }
        set_list_selection(ui, self.list.as_ref().map(|l| l.handle), None);
        for e in &entries {
            self.add_squelch_display(ui, &e.name, NO_CHARACTER_ID, e.account);
        }
        if let Some(list) = self.list.as_mut() {
            list.update_layout(ui);
        }
        self.update_buttons(ui);
        self.last = Some(entries);
        self.rebuilds += 1;
        true
    }

    /// The index the new row goes **before**,
    /// or `None` for the tail.
    ///
    /// Retail walks the rows and returns the first whose `0x10000542` text sorts after the new
    /// name (`wcscmp < 0`), or none for the tail.
    ///
    /// **It takes an `account` argument and never reads it**: nothing in the function tests
    /// `0x10000057`. So the squelch list is **one** alphabetical block, unlike the Friends
    /// panel's, whose otherwise identical loop splits online above offline — the two functions are visibly the same code with that test
    /// removed. The argument is not carried here, because a parameter nothing reads is a
    /// transcription of a mistake rather than of a behaviour.
    ///
    /// The `wcscmp` is by UTF-16 code unit; for the ASCII character names the shard sends, Rust's
    /// `str` ordering is the same comparison.
    fn insert_position(&self, ui: &mut UiSystem, name: &str) -> Option<usize> {
        let rows: Vec<ElemHandle> = self.rows.iter().map(|r| r.element).collect();
        for (i, row) in rows.into_iter().enumerate() {
            let text = ui
                .get_child_recursive(row, ROW_NAME)
                .map_or_else(String::new, |t| entry_text(ui, t));
            if name < text.as_str() {
                return Some(i);
            }
        }
        None
    }

    /// The squelch panel's squelch display insert.
    ///
    /// Retail finds the sorted insert position, creates a row from template 0 there, finds its
    /// `0x10000542` text child (failing if absent), sets the text to the name, writes `id` to
    /// attribute `0x1000008F`, and puts the text element in state `0x10000057` (account) or
    /// `0x10000056` (character).
    ///
    /// Returns whether a row was created — false is retail's "the template had no `0x10000542`",
    /// which leaves no row and no identity.
    pub fn add_squelch_display(
        &mut self,
        ui: &mut UiSystem,
        name: &str,
        id: ObjectId,
        account: bool,
    ) -> bool {
        let at = self.insert_position(ui, name);
        let Some(mut list) = self.list.take() else {
            return false;
        };
        let row = list.add_from_template(ui, SQUELCH_ROW_TEMPLATE, at);
        self.list = Some(list);
        let Some(row) = row else { return false };
        let Some(text) = ui.get_child_recursive(row, ROW_NAME) else {
            return false;
        };
        if let Some(t) = ui.text_element_mut(text) {
            t.set_text(name);
        }
        set_attr_instance_id(ui, row, ATTR_ROW_INSTANCE_ID, id.0);
        ui.set_state(
            text,
            if account {
                ROW_STATE_ACCOUNT
            } else {
                ROW_STATE_CHARACTER
            },
        );
        let at = at.unwrap_or(self.rows.len());
        self.rows.insert(
            at,
            SquelchRow {
                name: name.to_owned(),
                account,
                element: row,
            },
        );
        true
    }

    /// Two rules for three buttons.
    ///
    /// Remove is enabled (state `1`) exactly when a row is selected, else disabled (`0xD`); both
    /// squelch buttons are enabled exactly when the private list holds fewer than `0x32` entries.
    ///
    /// Retail also looks up the selected row's `0x10000542` child, and that lookup is **dead**:
    /// the client fetches and casts the child, then ignores the result without
    /// storing or testing it. It is the remains of the client's Tell
    /// rule, which reads the same child's state to decide a third button this panel does not
    /// have. Nothing here reproduces it, because a call with no effect is not a behaviour.
    ///
    /// The cap reads [`Self::squelch_list_len`], which retail never writes — so in practice both
    /// squelch buttons are re-enabled on every refresh **and on every list selection**, including
    /// with an empty name box. The keystroke arm is the only thing that greys them.
    ///
    /// A *list selection* re-enables the buttons over an empty box: the list sends
    /// message `0x04` (`LIST_SELECTION_CHANGED`) to the panel, which runs this function, and this
    /// function does not read the box. See [`Self::squelch_list_len`] for why the cap is dead.
    /// **This is pinned retail behaviour, not an accident: do not change it without deciding to
    /// diverge from retail.**
    ///
    /// Returns whether anything was written.
    pub fn update_buttons(&mut self, ui: &mut UiSystem) -> bool {
        let row = self.selected.and_then(|i| self.rows.get(i)).is_some();
        let under_cap = self.squelch_list_len < MAX_SQUELCHES;
        let want = [row, under_cap, under_cap];
        if self.button_states == Some(want) {
            return false;
        }
        for (h, on) in [
            (self.remove_button, want[0]),
            (self.squelch_character_button, want[1]),
            (self.squelch_account_button, want[2]),
        ] {
            set_enabled(ui, h, on);
        }
        self.button_states = Some(want);
        true
    }

    /// The squelch panel's element-message handler.
    ///
    /// * message `1` (click) from Remove (`0x10000547`): with a row selected, read its
    ///   `0x10000542` text as a narrow name; if that element's state is `0x10000057` send
    ///   [`UiRequest::ModifyAccountSquelch`] (add 0, the name), else the
    ///   modify-character-squelch event `(0, 0, name, 1)`;
    /// * `1` from Squelch Character (`0x1000054B`): send
    ///   the modify-character-squelch event `(1, 0, name, 1)` with the edit box's text, clear
    ///   the box, disable the Character button;
    /// * `1` from Squelch Account (`0x1000054C`): send [`UiRequest::ModifyAccountSquelch`] (add 1, the name), clear
    ///   the box, disable the Account button;
    /// * `4` or `0x43`: the button update;
    /// * `0x12` or `0x44`: under the cap, enable both squelch buttons exactly when the edit box is
    ///   non-empty.
    ///
    /// Two details that are easy to get backwards:
    ///
    /// * **Each button greys only itself.** The character action disables the Character button and
    ///   the account action disables the Account button; neither touches the other. So after
    ///   squelching a character the Account button is still lit over an empty box until the next
    ///   keystroke or refresh. The keystroke arm, by contrast, writes both.
    /// * **Remove sends a *name* and never an id.** The `0x1000008F` attribute the row carries is
    ///   zero (see the module header); the client reads the row's text child and the row state.
    ///
    /// A refcounted string is empty when its stored length is 1 because that length counts the
    /// terminator — the same shape [`super::friends`]' Add button has.
    ///
    /// # Where the keystroke arm's producer is, and where it is not
    ///
    /// Both buttons can stay lit over an empty box. The three
    /// writers are the ones above and no others, and **none of them is a "the box is empty"
    /// poll**: the button update reads the cap alone, the click arms each grey one button, and the
    /// only rule that consults the box is this arm, which runs on a *message*. So the question is
    /// which messages exist.
    ///
    /// * The text element's text clear raises **nothing** — it releases pending string
    ///   downloads, flushes, resets the selection and marks the root dirty, and that is all.
    ///   The programmatic clear after a squelch therefore does not dim in retail either, and
    ///   *"both go dim after a squelch"* is **not** what the shipped client does:
    ///   the clicked button dims, its neighbour stays lit, and the shard's `0x01F4` relights both.
    /// * The text element's delete **does**: when it actually deletes something it broadcasts
    ///   `0x44`. That is the producer a player reaches by emptying the box with
    ///   backspace; `dereth_ui`'s `TextElement` raises it from the delete path as well as the
    ///   insert path (see `TextElement::text_deleted`).
    ///
    /// One narrowing remains, declared rather than silent: retail's `0x12`/`0x44` arm has **no**
    /// source-element test — any such message reaching the Squelch panel re-reads the name edit
    /// box — and this build's arm requires the source to be the box. Nothing
    /// else in the panel's subtree produces either message, so the two agree today.
    ///
    /// Returns true when the message was consumed.
    pub fn on_element_message(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::LIST_SELECTION_CHANGED || m.id == msg::LIST_ITEM_ACTIVATED {
            let Some(list) = self.list.as_ref().map(|l| l.handle) else {
                return false;
            };
            if list != m.source {
                return false;
            }
            self.selected = list_selection(ui, list).filter(|i| *i < self.rows.len());
            self.update_buttons(ui);
            return true;
        }
        if m.id == msg::CHARACTER || m.id == msg::TEXT_CHANGED {
            let Some(entry) = self.name_entry else {
                return false;
            };
            if entry != m.source {
                return false;
            }
            if self.squelch_list_len < MAX_SQUELCHES {
                let empty = entry_text(ui, entry).is_empty();
                set_enabled(ui, self.squelch_character_button, !empty);
                set_enabled(ui, self.squelch_account_button, !empty);
                if let Some(s) = self.button_states.as_mut() {
                    s[1] = !empty;
                    s[2] = !empty;
                }
            }
            return true;
        }
        if m.id != msg::BUTTON_CLICKED {
            return false;
        }
        match m.source_id {
            REMOVE_BUTTON => self.remove_selected(ui),
            SQUELCH_CHARACTER_BUTTON => self.squelch_character(ui),
            SQUELCH_ACCOUNT_BUTTON => self.squelch_account(ui),
            _ => false,
        }
    }

    /// The `0x1000054B` arm — Squelch Character.
    fn squelch_character(&mut self, ui: &mut UiSystem) -> bool {
        let Some(entry) = self.name_entry else {
            return false;
        };
        let name = entry_text(ui, entry);
        ui.requests.emit(UiRequest::ModifyCharacterSquelch {
            object: NO_CHARACTER_ID,
            add: true,
            account: name,
            message_type: ALL_CHANNELS,
        });
        self.character_squelch_requests += 1;
        clear_all_text(ui, entry);
        set_enabled(ui, self.squelch_character_button, false);
        if let Some(s) = self.button_states.as_mut() {
            s[1] = false;
        }
        true
    }

    /// The `0x1000054C` arm — Squelch Account, and the one send this panel is the **only** raiser
    /// of in the client.
    fn squelch_account(&mut self, ui: &mut UiSystem) -> bool {
        let Some(entry) = self.name_entry else {
            return false;
        };
        let name = entry_text(ui, entry);
        ui.requests
            .emit(UiRequest::ModifyAccountSquelch { add: true, name });
        self.account_squelch_requests += 1;
        clear_all_text(ui, entry);
        set_enabled(ui, self.squelch_account_button, false);
        if let Some(s) = self.button_states.as_mut() {
            s[2] = false;
        }
        true
    }

    /// The `0x10000547` arm — Remove, i.e. the un-squelch.
    ///
    /// The kind comes off the **row's text element state**, not out of [`SquelchRow::account`],
    /// because that is where the client reads it and because a row whose state never got written
    /// must un-squelch the way the client would.
    fn remove_selected(&mut self, ui: &mut UiSystem) -> bool {
        let Some(row) = self
            .selected
            .and_then(|i| self.rows.get(i))
            .map(|r| r.element)
        else {
            return false;
        };
        let Some(text) = ui.get_child_recursive(row, ROW_NAME) else {
            return false;
        };
        let account = ui.node(text).is_some_and(|n| n.state == ROW_STATE_ACCOUNT);
        let name = entry_text(ui, text);
        ui.requests.emit(if account {
            UiRequest::ModifyAccountSquelch { add: false, name }
        } else {
            UiRequest::ModifyCharacterSquelch {
                object: NO_CHARACTER_ID,
                add: false,
                account: name,
                message_type: ALL_CHANNELS,
            }
        });
        self.remove_requests += 1;
        true
    }
}

/// The cap the button update and the keystroke arm test, a list of 100, against a list this class
/// never fills. See
/// [`SquelchPanel::squelch_list_len`].
pub const MAX_SQUELCHES: usize = 100;

/// Depth-first search for the one element of [`PANEL_TYPE`], the same shape
/// [`super::friends`] uses and for the same reason.
fn find_panel(ui: &UiSystem, h: ElemHandle) -> Option<ElemHandle> {
    if ui.node(h).is_some_and(|n| n.ty() == PANEL_TYPE) {
        return Some(h);
    }
    ui.children(h).into_iter().find_map(|c| find_panel(ui, c))
}

/// The element's text, tags stripped the way every other reader in this crate takes it.
fn entry_text(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// The text element's text clear.
fn clear_all_text(ui: &mut UiSystem, h: ElemHandle) {
    if let Some(t) = ui.text_element_mut(h) {
        t.set_text("");
    }
}

/// State `1` / state `0x0D` — the pair the button update uses.
fn set_enabled(ui: &mut UiSystem, h: Option<ElemHandle>, on: bool) {
    if let Some(h) = h {
        ui.set_state(
            h,
            if on {
                dereth_ui::widgets::button::state::NORMAL
            } else {
                dereth_ui::widgets::button::state::DISABLED
            },
        );
    }
}

/// Whether a bound button is currently offered, read back off the element rather than off a
/// mirror. A free function taking the handle for the reason [`super::friends::button_enabled`] is.
#[must_use]
pub fn button_enabled(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h)
        .is_some_and(|n| n.state != dereth_ui::widgets::button::state::DISABLED)
}

/// The list box's selected index, read off the live widget.
fn list_selection(ui: &UiSystem, list: ElemHandle) -> Option<usize> {
    ui.node(list)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| (**b).as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .and_then(|l| l.selected)
}

/// Sets (or, with `None`, clears) the list box's selected item.
fn set_list_selection(ui: &mut UiSystem, list: Option<ElemHandle>, index: Option<usize>) {
    let Some(h) = list else { return };
    if let Some(l) = ui.node_mut(h).and_then(|n| {
        n.behaviour
            .as_mut()?
            .as_any_mut()?
            .downcast_mut::<dereth_ui::widgets::listbox::ListBox>()
    }) {
        l.selected = index.filter(|i| *i < l.items.len());
    }
}

/// Writes instance-id attribute `id` = `v` on `h`.
fn set_attr_instance_id(ui: &mut UiSystem, h: ElemHandle, id: u32, v: u32) {
    let value = dereth_assets::ui::PropertyValue::InstanceId(v);
    if let Some(n) = ui.node_mut(h) {
        n.instance_properties.set(id, value.clone());
    }
    ui.on_set_attribute(h, id, Some(&value));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The element ids are transcribed constants, so one test states them as **literals** rather
    /// than reading them back through the same symbol that wrote them (the stated testability rule).
    /// Source: the squelch panel's post-init, the squelch display insert and the
    /// element-message handler.
    #[test]
    fn the_element_ids_match_post_init() {
        assert_eq!(PANEL_TYPE, ElementType(0x1000_0047));
        assert_eq!(SQUELCH_LIST, ElementId(0x1000_053E));
        assert_eq!(NAME_ENTRY_BOX, ElementId(0x1000_0540));
        assert_eq!(REMOVE_BUTTON, ElementId(0x1000_0547));
        assert_eq!(SQUELCH_CHARACTER_BUTTON, ElementId(0x1000_054B));
        assert_eq!(SQUELCH_ACCOUNT_BUTTON, ElementId(0x1000_054C));
        assert_eq!(ROW_NAME, ElementId(0x1000_0542));
        assert_eq!(ATTR_ROW_INSTANCE_ID, 0x1000_008F);
        assert_eq!(ROW_STATE_CHARACTER, StateId(0x1000_0056));
        assert_eq!(ROW_STATE_ACCOUNT, StateId(0x1000_0057));
        assert_eq!(SQUELCH_ROW_TEMPLATE, 0);
        assert_eq!(MAX_SQUELCHES, 100);
        assert_eq!(ALL_CHANNELS, 1);
        assert_eq!(NO_CHARACTER_ID, ObjectId(0));
    }

    /// A panel with no tree bound writes nothing and says so — the "never ran" state that has to
    /// be distinguishable from "ran and nobody is squelched", which is the state every recorded
    /// session's login `0x01F4` puts it in.
    #[test]
    fn an_unbound_panel_reports_that_it_is_unbound() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = SquelchPanel::default();
        assert!(!p.bound());
        assert_eq!(p.rebuilds, 0);
        assert!(p.update(&mut ui, &crate::view::EmptyGameView));
        assert_eq!(p.rebuilds, 1, "the panel was driven and found nothing");
        assert!(p.rows().is_empty());
        assert!(
            !p.update(&mut ui, &crate::view::EmptyGameView),
            "and it is idempotent"
        );
        assert_eq!(p.rebuilds, 1);
    }

    /// The client's cap reads a list retail never fills, so the two squelch
    /// buttons are offered on a panel with nothing selected and an empty name box. The Remove
    /// button is the only one the selection moves.
    #[test]
    fn the_cap_never_bites_because_its_list_has_no_writer() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = SquelchPanel::default();
        assert_eq!(
            p.squelch_list_len, 0,
            "the squelch panel has no path that populates its list"
        );
        assert!(p.update_buttons(&mut ui));
        assert_eq!(p.button_states, Some([false, true, true]));
    }

    /// Were the list ever filled, the squelch buttons would stay live through the ninety-ninth
    /// entry and grey at the hundredth.
    #[test]
    fn the_squelch_cap_is_one_hundred() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = SquelchPanel {
            squelch_list_len: 99,
            ..SquelchPanel::default()
        };
        assert!(p.update_buttons(&mut ui));
        assert_eq!(p.button_states, Some([false, true, true]));
        p.squelch_list_len = 100;
        assert!(p.update_buttons(&mut ui));
        assert_eq!(p.button_states, Some([false, false, false]));
    }
}
