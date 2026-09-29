//! `FriendsPanel` — the Friends tab: the list, the three buttons and the name box.
//!
//! `element_types` registers `0x10000045`, `panels::catalogue` carries a six-child row for it and
//! `panels::rows` names its `0x10000085` row attribute; this module is the constructor and
//! behaviour behind them.
//!
//! # The halves that work together
//!
//! `0x0021 Social_FriendsUpdate` is common in recorded sessions. `dereth_client_model::friends`
//! writes it into `dereth_client_model::player::Social::friends`, and this module reads it;
//! neither is useful without the other.
//!
//! The chat entry point is the third half: `dereth_client_model::cmd::table` names the friends,
//! friends-add and friends-remove commands, and `@friends add Bob` is how a player usually adds a
//! friend — the panel's name box is the *other* way.
//!
//! # The client's own functions, and where they are here
//!
//! | client | here |
//! |---|---|
//! | — five child lookups, one check box, five notice registrations | [`FriendsPanel::post_init`] |
//! | — clear, deselect, one row insert per friend, the button refresh | [`FriendsPanel::update`] |
//! | — sorted insert, text, `0x10000085`, the row's state | [`FriendsPanel::add_friend_display`] |
//! | — online block above offline block, each by name | [`FriendsPanel::insert_position`] |
//! | — walk the rows for an id and delete that one | [`FriendsPanel::remove_friend_display`] |
//! | — Remove, Tell and Add, three separate rules | [`FriendsPanel::update_buttons`] |
//! | — three buttons, the list, the name box | [`FriendsPanel::on_element_message`] |
//! | — the 100-cap, then the add-friend request | split; see below |
//!
//! # The row's *state* is the online flag, and it is what two other things read back
//!
//! The row insert writes the friend's name into the row's `0x1000051A` text child and then
//! puts that **text element** into state `0x10000054` (online) or `0x10000055` (offline). Nothing
//! stores the flag anywhere else, and two separate functions read it back off the element:
//! the button refresh, to decide whether the Tell button is live, and
//! the display-friends chat-command notice, to decide whether a row gets the
//! `" (Online)"` suffix. That is why [`ROW_STATE_ONLINE`] is written to the tree here rather than
//! kept in [`FriendRow`] alone — a mirror would be a second store the client does not have.
//!
//! # Where the 100-cap lives, and why not here
//!
//! The client's add-friend request refuses once the list holds 100 (`count > 0x63`) and raises
//! weenie error `0x561`; the button refresh greys the Add button at the same boundary. The
//! **refusal** is `dereth_client_model::friends::add_friend`'s, because the list is the model's and because
//! the `@friends add` chat command reaches the same check by a path that does not pass through this
//! struct. What is here is only [`MAX_FRIENDS`], for the button state — which is exactly the use
//! the button refresh puts the number to.
//!
//! # The Tell button's send
//!
//! The element-message handler's `0x10000516` arm ends in a start-tell notice carrying the name,
//! whose only receiver is the main chat panel's start-tell handler — which forwards straight to
//! the chat interface's own start-tell, which puts `@tell <name>, ` into the main chat entry and
//! focuses it. The arm raises [`UiRequest::StartTell`], the host routes it to the gameplay screen the way
//! it routes `SetLockUi`, and `crate::chat::window::on_start_tell` is the
//! receiver. The **name is read off the row's `0x1000051A` text element**, not out of
//! [`FriendRow::name`], because that is where retail reads it — a row whose text never got
//! written must not start a tell to nobody.

use dereth_primitives::ObjectId;
use dereth_ui::{ElemHandle, ElementId, ElementType, StateId, UiSystem};

use super::listbox::ListBoxWidget;
use crate::view::{FriendEntry, GameView, UiRequest};

/// The friends panel's registered element class, `0x10000045`.
///
/// Found by **type** rather than by id, for the reason [`super::allegiance::PANEL_TYPE`] and
/// [`super::fellowship::PANEL_TYPE`] are: the panel binds six *children* and
/// never names its own id, which is layout data.
pub const PANEL_TYPE: ElementType = ElementType(0x1000_0045);

/// The Add button.
pub const ADD_BUTTON: ElementId = ElementId(0x1000_0514);
/// The Remove button.
pub const REMOVE_BUTTON: ElementId = ElementId(0x1000_0515);
/// The Tell button.
pub const TELL_BUTTON: ElementId = ElementId(0x1000_0516);
/// The friends list box.
pub const FRIENDS_LIST: ElementId = ElementId(0x1000_0517);
/// The friend-name entry box.
pub const NAME_ENTRY_BOX: ElementId = ElementId(0x1000_051B);
/// The sixth child the panel binds at initialization, wrapped as an option check box over the
/// `AppearOffline` player option.
///
/// It is the **player's own** appear-offline option and not a friend's; the character-options
/// mechanism owns it, exactly as `FellowshipPanel`'s four check boxes belong to that mechanism and
/// not to the fellowship.
///
/// It is driven, not merely bound: it reads the character's bit and acts on a press. See
/// [`APPEAR_OFFLINE_OPTION_BOX`].
pub const APPEAR_OFFLINE_CHECKBOX: ElementId = ElementId(0x1000_052C);

/// The client's one option check box on this panel, as
/// [`crate::options::toggle::PanelOptionBoxes`] wants it — the **sixth** and last such box.
///
/// The ordinal is the one the client binds:
///
/// Initialization finds child `0x1000052C`, casts it to option-checkbox type
/// `0x10000035` and binds AppearOffline, ordinal `0x27` (39). It then obtains the
/// global event handler; no further checkbox setup follows that binding.
///
/// **Initialization stops there.** No caption string and no tooltip are set for this box, unlike
/// the five in the two social panels: the client binds the option and nothing else. The caption
/// *"Appear Offline"* is authored in the shipped layout, and
/// `ID_PlayerOption_AppearOffline` is not in string table `0x10000003` at all — so this table is
/// bound with [`crate::options::toggle::Caption::FromLayout`], which reads the caption back instead
/// of writing one over it.
pub const APPEAR_OFFLINE_OPTION_BOX: [(ElementId, crate::view::PlayerOption); 1] = [(
    APPEAR_OFFLINE_CHECKBOX,
    crate::view::PlayerOption::AppearOffline,
)];

/// The row template's text child — the one the row insert looks up — and the element whose
/// *state* carries the online flag.
pub const ROW_NAME: ElementId = ElementId(0x1000_051A);

/// The instance-id attribute written onto each row — the id the Remove button reads back, and
/// the one the row removal walks the list comparing against.
///
/// `panels::rows::ROW_ATTRIBUTES` names the same number.
pub const ATTR_ROW_INSTANCE_ID: u32 = 0x1000_0085;

/// Rows are added from template 0 — the list carries exactly one template,
/// `catalogue::FRIENDS_TEMPLATES`' `0x1000051A`.
pub const FRIEND_ROW_TEMPLATE: usize = 0;

/// The row text element's state while the friend is **logged in**.
pub const ROW_STATE_ONLINE: StateId = StateId(0x1000_0054);
/// …and while they are not.
pub const ROW_STATE_OFFLINE: StateId = StateId(0x1000_0055);

/// The client's `count < 0x64` test — the same **100** that
/// `dereth_client_model::friends::MAX_FRIENDS` is, and the only reason this crate needs the number at all.
/// The refusal that uses it lives with the list; see the module header.
pub const MAX_FRIENDS: usize = 100;

/// One drawn row, as this panel wrote it — so a test can read back what a player would see
/// without walking the element tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendRow {
    /// What went into [`ATTR_ROW_INSTANCE_ID`].
    pub id: ObjectId,
    /// What went into the row's [`ROW_NAME`] text.
    pub name: String,
    /// Which of [`ROW_STATE_ONLINE`] / [`ROW_STATE_OFFLINE`] that text element was put into.
    pub online: bool,
    pub element: ElemHandle,
}

/// `FriendsPanel`, bound to a live tree.
#[derive(Debug, Default)]
pub struct FriendsPanel {
    /// The friends list box.
    pub list: Option<ListBoxWidget>,
    pub add_button: Option<ElemHandle>,
    pub remove_button: Option<ElemHandle>,
    pub tell_button: Option<ElemHandle>,
    /// The friend-name entry box.
    pub name_entry: Option<ElemHandle>,
    /// The Appear Offline option check box, with its value and its click, and the
    /// single store for that element: [`Self::appear_offline`] reads the handle back out of it
    /// rather than keeping a second copy. See [`APPEAR_OFFLINE_OPTION_BOX`].
    pub option_box: crate::options::toggle::PanelOptionBoxes,
    /// The `FriendsPanel` element itself, found by [`PANEL_TYPE`].
    pub panel: Option<ElemHandle>,
    /// The rows on screen, in list order — online block first, each block alphabetical.
    rows: Vec<FriendRow>,
    /// The list box's selected row, as an index into [`Self::rows`].
    pub selected: Option<usize>,
    /// The snapshot [`Self::update`] last drew, so an unchanged frame redraws nothing.
    last: Option<Vec<FriendEntry>>,
    /// How many times [`Self::update`] rebuilt the list. **Three states, not two**: this is what
    /// separates "rebuilt and the character has no friends" from "never ran", and it is the whole
    /// difference between the empty `0x0021` the locked corpus carries and an unwired panel.
    pub rebuilds: u32,
    /// What [`Self::update_buttons`] last wrote — Remove, Tell, Add — so a test can read the
    /// three decisions without a tree, and so "never refreshed" is distinguishable from
    /// "refreshed and decided disabled". `None` until the first refresh.
    pub button_states: Option<[bool; 3]>,
    /// How many `0x0018 Social_AddFriend` this panel has asked for, and how many `0x0017`. The
    /// observable that separates "the button is bound" from "the button does something".
    pub add_requests: u32,
    /// See [`Self::add_requests`].
    pub remove_requests: u32,
    /// Presses of the Tell button that reached the `0x10000516` arm and raised a
    /// [`UiRequest::StartTell`].
    pub tells_started: u32,
    /// Presses of the Appear Offline box that reached [`crate::options::toggle::PanelOptionBoxes`]
    /// and raised a [`UiRequest::SetPlayerOption`].
    pub appear_offline_presses: u32,
}

impl FriendsPanel {
    /// The friends panel's initialization, minus the five notice registrations (the
    /// friends-list update and the four chat-command notices) — this build has no notice
    /// bus at this seam, and the same five writes arrive as the per-frame [`Self::update`]
    /// snapshot and as `dereth_client_model::friends`' command handlers.
    ///
    /// Bound off the screen **root** rather than off a panel page, on the same terms as
    /// [`super::allegiance::AllegiancePanel::post_init`]: every id here is unique in the shipped
    /// tree, and the page `FriendsPanel` sits on is the toolbar's social page, which this crate's
    /// panel stack builds lazily.
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        *self = Self::default();
        self.panel = find_panel(ui, root);
        let from = self.panel.unwrap_or(root);
        self.add_button = ui.get_child_recursive(from, ADD_BUTTON);
        self.remove_button = ui.get_child_recursive(from, REMOVE_BUTTON);
        self.tell_button = ui.get_child_recursive(from, TELL_BUTTON);
        self.list = ui
            .get_child_recursive(from, FRIENDS_LIST)
            .map(|h| ListBoxWidget::bind(ui, h));
        self.name_entry = ui.get_child_recursive(from, NAME_ENTRY_BOX);
        // The sixth child lookup, plus the cast to option-checkbox type `0x10000035`
        // and the binding to player option `0x27` that follow it — and deliberately **not** a
        // caption or a tooltip, because the client sets neither. See [`APPEAR_OFFLINE_OPTION_BOX`].
        self.option_box.post_init_with_caption(
            ui,
            from,
            &APPEAR_OFFLINE_OPTION_BOX,
            crate::options::toggle::Caption::FromLayout,
        );
    }

    /// True once the list box was found — the one binding without which nothing can be drawn.
    #[must_use]
    pub fn bound(&self) -> bool {
        self.list.is_some()
    }

    /// The Appear Offline check box, read back out of [`Self::option_box`] so the element has
    /// exactly one store in this struct.
    #[must_use]
    pub fn appear_offline(&self) -> Option<ElemHandle> {
        self.option_box.boxes.first().map(|b| b.element)
    }

    /// The rows this panel last wrote, in list order.
    #[must_use]
    pub fn rows(&self) -> &[FriendRow] {
        &self.rows
    }

    /// The names on screen, in list order — what a test asserts against instead of a pixel.
    #[must_use]
    pub fn shown(&self) -> Vec<String> {
        self.rows.iter().map(|r| r.name.clone()).collect()
    }

    /// The friends panel's display refresh, guarded on the snapshot: clear the list, deselect,
    /// insert one row per friend in list order, then refresh the buttons.
    ///
    /// Retail reaches it from **one** of the five update-friends-list notice arms (type `0`,
    /// the full replace); the other four patch the rows incrementally
    /// (add one, remove one, update one). This build drives it once a frame
    /// off the `Vec<FriendEntry>` snapshot, so a change of any kind rebuilds — which reaches the
    /// same rows the four incremental paths would have, because all four are functions of the
    /// same list. [`Self::add_friend_display`] and [`Self::remove_friend_display`] are the
    /// incremental halves, transcribed and exercised, for the day this crate gains a notice bus.
    ///
    /// An empty list is **not** an early return: it flushes and counts a rebuild, because a
    /// character who clears their list must see the panel empty rather than see the old rows.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        // The checkbox option control's value read followed by the refresh,
        // for the Appear Offline box — **before** the friends-list guard, because the option is the
        // player's own word and does not move when the friends list does. `refresh` carries its own
        // guard and writes only when the bit moved, so this is a no-op on every frame but the first
        // and the ones a tick lands on. The three notices retail refreshes it from
        // (reload options, refresh options panel, player option changed) have no bus
        // in this build; see `options::toggle`'s declared deviation 2.
        let option_moved = self.option_box.refresh(ui, view) > 0;
        let friends = view.friends();
        if self.last.as_ref() == Some(&friends) {
            return option_moved;
        }
        self.rows.clear();
        self.selected = None;
        if let Some(mut list) = self.list.take() {
            list.flush(ui);
            self.list = Some(list);
        }
        for f in &friends {
            self.add_friend_display(ui, &f.name, f.id, f.online);
        }
        if let Some(list) = self.list.as_mut() {
            list.update_layout(ui);
        }
        self.update_buttons(ui, friends.len());
        self.last = Some(friends);
        self.rebuilds += 1;
        true
    }

    /// The index the new row goes **before**,
    /// or `None` for the tail.
    ///
    /// Retail walks the rows reading each row text element's state: an offline friend skips every
    /// row in the online state `0x10000054`, an online friend stops at the first row in the offline
    /// state `0x10000055`, and otherwise the new row goes before the first row whose text compares
    /// greater than the name (`wcscmp`); no such row means the tail.
    ///
    /// So the list is **two alphabetical blocks**, online above offline, and an online friend is
    /// inserted before the first offline row without any name comparison at all. The comparison is
    /// by UTF-16 code unit; for the ASCII character names the shard sends, Rust's `str` ordering
    /// is the same comparison — the same equivalence [`super::titles`] relies on.
    ///
    /// The state is read back off the **row element**, not out of [`Self::rows`], because that is
    /// where the client reads it and because a row whose state never got written must sort as the
    /// client would sort it.
    fn insert_position(&self, ui: &UiSystem, name: &str, online: bool) -> Option<usize> {
        for (i, row) in self.rows.iter().enumerate() {
            let state = ui
                .get_child_recursive(row.element, ROW_NAME)
                .and_then(|t| ui.node(t))
                .map_or(StateId(0), |n| n.state);
            if online {
                if state == ROW_STATE_OFFLINE {
                    return Some(i);
                }
            } else if state == ROW_STATE_ONLINE {
                continue;
            }
            if name < row.name.as_str() {
                return Some(i);
            }
        }
        None
    }

    /// The friends panel's row insert: find the sorted position, add a row from template 0 there,
    /// look up its `0x1000051A` text child (none: fail), write the name into it, write the friend's
    /// id into row attribute `0x10000085`, and put the text element into state `0x10000054`
    /// (online) or `0x10000055` (offline).
    ///
    /// **The instance-id attribute is the friend's id, not the name** — the Remove button proves
    /// it by comparing it against a `ulong` friend id.
    ///
    /// Returns whether a row was created — false is retail's "the template had no `0x1000051A`",
    /// which leaves no row and no identity.
    pub fn add_friend_display(
        &mut self,
        ui: &mut UiSystem,
        name: &str,
        id: ObjectId,
        online: bool,
    ) -> bool {
        let at = self.insert_position(ui, name, online);
        let Some(mut list) = self.list.take() else {
            return false;
        };
        let row = list.add_from_template(ui, FRIEND_ROW_TEMPLATE, at);
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
            if online {
                ROW_STATE_ONLINE
            } else {
                ROW_STATE_OFFLINE
            },
        );
        let at = at.unwrap_or(self.rows.len());
        self.rows.insert(
            at,
            FriendRow {
                id,
                name: name.to_owned(),
                online,
                element: row,
            },
        );
        true
    }

    /// Deselect, then walk the rows for the id and
    /// delete the **first** one that carries it.
    ///
    /// The deselect is unconditional and comes first, even when no row matches, which is why a
    /// `0x0021` for somebody who is not on screen still clears the selection.
    pub fn remove_friend_display(&mut self, ui: &mut UiSystem, id: ObjectId) -> bool {
        self.selected = None;
        set_list_selection(ui, self.list.as_ref().map(|l| l.handle), None);
        let Some(i) = self
            .rows
            .iter()
            .position(|r| attr_instance_id(ui, r.element, ATTR_ROW_INSTANCE_ID) == Some(id.0))
        else {
            return false;
        };
        if let Some(list) = self.list.as_mut() {
            list.delete_item(ui, i);
        }
        self.rows.remove(i);
        true
    }

    /// Three rules, one per button, each writing state 1 (live) or `0xD` (greyed): Remove is live
    /// when a row is selected; Tell when the selected row's `0x1000051A` text element is in state
    /// `0x10000054`; Add while the friends list holds fewer than `0x64` (100).
    ///
    /// The Tell rule reads the **row's state**, i.e. the online flag, which is the second of the
    /// two readers that make the state load-bearing (see the module header). `count` is the
    /// friends list's length, which is the model's and not the list box's — retail reads
    /// the friends list here and the list box's rows for the other two, and the two can differ for
    /// one frame after a `LoginChange`.
    ///
    /// Returns whether anything was written.
    pub fn update_buttons(&mut self, ui: &mut UiSystem, count: usize) -> bool {
        let row = self
            .selected
            .and_then(|i| self.rows.get(i))
            .map(|r| r.element);
        let tell = row.is_some_and(|r| {
            ui.get_child_recursive(r, ROW_NAME)
                .and_then(|t| ui.node(t))
                .is_some_and(|n| n.state == ROW_STATE_ONLINE)
        });
        let want = [row.is_some(), tell, count < MAX_FRIENDS];
        if self.button_states == Some(want) {
            return false;
        }
        for (h, on) in [
            (self.remove_button, want[0]),
            (self.tell_button, want[1]),
            (self.add_button, want[2]),
        ] {
            set_enabled(ui, h, on);
        }
        self.button_states = Some(want);
        true
    }

    /// The friends panel's element-message handler.
    ///
    /// - Message 1 (click): Add (`0x10000514`) requests the name box's text as a friend, then
    ///   clears the box and greys Add; Remove (`0x10000515`) reads the selected row's `0x10000085`
    ///   attribute and requests that friend's removal; Tell (`0x10000516`) reads the selected row's
    ///   `0x1000051A` text and starts a tell to it.
    /// - Messages 4 and `0x43`: refresh the buttons.
    /// - Messages `0x12` and `0x44`: while the list holds fewer than `0x64`, grey Add when the name
    ///   box is empty and enable it otherwise.
    ///
    /// The emptiness check is a length-of-1 test on a string whose length counts the
    /// terminator — the same shape [`super::fellowship`]'s Create button has — and the **whole**
    /// `0x12`/`0x44` arm sits inside the 100-cap test, so typing into the box while the list is
    /// full leaves the Add button greyed rather than re-enabling it.
    ///
    /// Note the Add arm's two tails are **unconditional**: retail clears the box and greys the
    /// button whether the add-friend request sent anything or refused on the cap.
    ///
    /// Returns true when the message was consumed.
    pub fn on_element_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use dereth_ui::msg::element::id as msg;
        if m.id == msg::LIST_SELECTION_CHANGED || m.id == msg::LIST_ITEM_ACTIVATED {
            let Some(list) = self.list.as_ref().map(|l| l.handle) else {
                return false;
            };
            if list != m.source {
                return false;
            }
            self.selected = list_selection(ui, list).filter(|i| *i < self.rows.len());
            self.update_buttons(ui, view.friends().len());
            return true;
        }
        if m.id == msg::CHARACTER || m.id == msg::TEXT_CHANGED {
            let Some(entry) = self.name_entry else {
                return false;
            };
            if entry != m.source {
                return false;
            }
            if view.friends().len() < MAX_FRIENDS {
                let empty = entry_text(ui, entry).is_empty();
                set_enabled(ui, self.add_button, !empty);
                if let Some(s) = self.button_states.as_mut() {
                    s[2] = !empty;
                }
            }
            return true;
        }
        if m.id != msg::BUTTON_CLICKED {
            return false;
        }
        match m.source_id {
            ADD_BUTTON => self.request_add_friend(ui),
            REMOVE_BUTTON => self.remove_selected(ui),
            TELL_BUTTON => self.start_tell(ui),
            // The option check box's message-1 arm: read attribute `0x0E` back off
            // the element — the click has already flipped it — and apply it, which sets the player
            // option to that current value. One option, never a composed word.
            APPEAR_OFFLINE_CHECKBOX => self.toggle_appear_offline(ui, m),
            _ => false,
        }
    }

    /// The `0x1000052C` arm. The request is raised and **not sent**: `AppearOffline` is
    /// player-option ordinal 39, one of the twenty-one auto-save option ordinals, so the host's
    /// `UiRequest::SetPlayerOption` arm answers it with a `0x0005
    /// Character_PlayerOptionChangedEvent` — which is the shard's business and not this crate's.
    /// This crate never touches a socket.
    fn toggle_appear_offline(&mut self, ui: &mut UiSystem, m: &dereth_ui::ElementMessage) -> bool {
        let Some(r) = self.option_box.on_element_message(ui, m) else {
            return false;
        };
        ui.requests.emit(r);
        self.appear_offline_presses += 1;
        true
    }

    /// The `0x10000514` arm: the box's text out, the request in, then the box cleared and the
    /// button greyed — both unconditional.
    ///
    /// The cap is not tested here; see the module header for where it is and why.
    fn request_add_friend(&mut self, ui: &mut UiSystem) -> bool {
        let Some(entry) = self.name_entry else {
            return false;
        };
        let name = entry_text(ui, entry);
        ui.requests.emit(UiRequest::AddFriend { name });
        self.add_requests += 1;
        if let Some(t) = ui.text_element_mut(entry) {
            t.set_text("");
        }
        set_enabled(ui, self.add_button, false);
        if let Some(s) = self.button_states.as_mut() {
            s[2] = false;
        }
        true
    }

    /// The `0x10000515` arm. The id comes off the **row element**, not out of [`Self::rows`],
    /// because that is where the client reads it and because a row whose attribute never got
    /// written must not be removable.
    fn remove_selected(&mut self, ui: &mut UiSystem) -> bool {
        let Some(row) = self
            .selected
            .and_then(|i| self.rows.get(i))
            .map(|r| r.element)
        else {
            return false;
        };
        let Some(id) = attr_instance_id(ui, row, ATTR_ROW_INSTANCE_ID) else {
            return false;
        };
        ui.requests.emit(UiRequest::RemoveFriend {
            target: ObjectId(id),
        });
        self.remove_requests += 1;
        true
    }

    /// The `0x10000516` arm. Reads the selected row's `0x1000051A` text and raises
    /// the start-tell notice with it; no selected row or no text child, nothing.
    ///
    /// The main chat window receives the notice and starts the tell; this build uses
    /// the `on_start_tell` method in `crate::chat::window`.
    /// It travels as a [`UiRequest`] because the chat window is the screen's and this panel
    /// is the `Hud`'s; a direct call would be a path retail does not have.
    fn start_tell(&mut self, ui: &mut UiSystem) -> bool {
        let Some(row) = self
            .selected
            .and_then(|i| self.rows.get(i))
            .map(|r| r.element)
        else {
            return false;
        };
        let Some(t) = ui.get_child_recursive(row, ROW_NAME) else {
            return false;
        };
        let name = entry_text(ui, t);
        ui.requests.emit(UiRequest::StartTell { name });
        self.tells_started += 1;
        true
    }
}

/// Depth-first search for the one element of [`PANEL_TYPE`], the same shape
/// [`super::fellowship`] and [`super::allegiance`] each use and for the same reason: the panel's
/// own id is layout data; only its element **type** is fixed by the client.
fn find_panel(ui: &UiSystem, h: ElemHandle) -> Option<ElemHandle> {
    if ui.node(h).is_some_and(|n| n.ty() == PANEL_TYPE) {
        return Some(h);
    }
    ui.children(h).into_iter().find_map(|c| find_panel(ui, c))
}

/// The name box's text, tags stripped the way every other reader in this
/// crate takes it.
fn entry_text(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// State 1 / state `0x0D` — the live/greyed pair the button refresh uses.
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
/// mirror. A free function taking the handle for the reason [`super::titles::button_enabled`] is.
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

/// Sets (or, with `None`, clears) the list box's selection, written straight into the
/// widget for the reason [`super::fellowship`]'s equivalent is.
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

/// Reads instance-id attribute `id` off `h`.
fn attr_instance_id(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<u32> {
    match ui.node(h)?.merged_properties().get(id)? {
        dereth_assets::ui::PropertyValue::InstanceId(v) => Some(*v),
        _ => None,
    }
}

/// Writes `v` as instance-id attribute `id` on `h`.
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
    /// Source: the friends panel's initialization, the row insert and the sorted insert
    /// position.
    #[test]
    fn the_element_ids_match_post_init() {
        assert_eq!(PANEL_TYPE, ElementType(0x1000_0045));
        assert_eq!(ADD_BUTTON, ElementId(0x1000_0514));
        assert_eq!(REMOVE_BUTTON, ElementId(0x1000_0515));
        assert_eq!(TELL_BUTTON, ElementId(0x1000_0516));
        assert_eq!(FRIENDS_LIST, ElementId(0x1000_0517));
        assert_eq!(ROW_NAME, ElementId(0x1000_051A));
        assert_eq!(NAME_ENTRY_BOX, ElementId(0x1000_051B));
        assert_eq!(APPEAR_OFFLINE_CHECKBOX, ElementId(0x1000_052C));
        assert_eq!(ATTR_ROW_INSTANCE_ID, 0x1000_0085);
        assert_eq!(ROW_STATE_ONLINE, StateId(0x1000_0054));
        assert_eq!(ROW_STATE_OFFLINE, StateId(0x1000_0055));
        assert_eq!(FRIEND_ROW_TEMPLATE, 0);
        assert_eq!(MAX_FRIENDS, 100);
    }

    /// A panel with no tree bound writes nothing and says so — the "never ran" state that has to
    /// be distinguishable from "ran and the character has no friends", which is the only state
    /// the five recorded `0x0021` events can put it in.
    #[test]
    fn an_unbound_panel_reports_that_it_is_unbound() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = FriendsPanel::default();
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

    /// The Add button stays live through the ninety-ninth friend and greys at the hundredth.
    #[test]
    fn the_add_button_greys_at_one_hundred_friends() {
        let mut ui = UiSystem::new((800, 600));
        let mut p = FriendsPanel::default();
        assert!(p.update_buttons(&mut ui, 50));
        assert_eq!(p.button_states, Some([false, false, true]));
        assert!(!p.update_buttons(&mut ui, 99), "still live, nothing moved");
        assert!(p.update_buttons(&mut ui, 100));
        assert_eq!(p.button_states, Some([false, false, false]));
    }
}
