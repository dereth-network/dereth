//! The social window's friends page.
//!
//! The friends are kept by the chat service: the page lists those online (each lit as away from
//! the keyboard, not to be disturbed, or online, the first two with a word saying so) and those
//! offline, each under its heading. Add (lit when another player not yet a friend is selected)
//! asks the chat service to add the one selected; Remove (lit when a friend's row is selected)
//! takes that friend off. The state menu sets the player's own state (available, not to be
//! disturbed, away from the keyboard), playing that state's emote; moving while away sets the
//! player available again.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::{attr, msg};
use auberean_ui::{Handle, Mail, Token, Ui};

use super::{entry, notice, write, Request, UserStatus};
use crate::gameplay::{Child, Notice, Record, Request as PlayRequest, View};
use crate::shortcuts::TOOLBAR_LAYOUT_ENUM;

mod name {
    use auberean_ui::Token;
    pub const PAGE: Token = Token::of("FriendsPage");
    pub const LIST: Token = Token::of("FriendsListBox");
    pub const ADD_BUTTON: Token = Token::of("FriendsAddButton");
    pub const REMOVE_BUTTON: Token = Token::of("FriendsRemoveButton");
    pub const STATUS_MENU: Token = Token::of("StatusDropDownMenu");
    pub const ONLINE_MARKER: Token = Token::of("OnlineFriendsMarker");
    pub const OFFLINE_MARKER: Token = Token::of("OfflineFriendsMarker");
    pub const TEMPLATE: Token = Token::of("FriendTemplate");
    pub const FRIEND_NAME: Token = Token::of("FriendName");
    pub const NAME_ID: Token = Token::of("name_id");
    pub const ONLINE: Token = Token::of("online");
    pub const OFFLINE: Token = Token::of("offline");
    pub const AWAY: Token = Token::of("awayfromkeyboard");
    pub const NOT_DISTURBED: Token = Token::of("donotdisturb");
}

/// The player's own states, in the menu's order: the chat service's mode, the menu's words, the
/// emote played.
pub const STATUSES: [(u32, &str, &str); 3] = [
    (1, "ID_FriendStatus_Available", "ID_EmoteYawnCmd"),
    (4, "ID_FriendStatus_DoNotDisturb", "ID_EmoteYawnCmd"),
    (2, "ID_FriendStatus_AwayFromKeyboard", "ID_EmoteAFKCmd"),
];

/// The emotes' string table, by enumeration value.
pub const EMOTES: u32 = 0x4100_001C;

/// The away mode, and the modes' bits a friend's state shows.
const AWAY: u32 = 2;
const AWAY_BIT: u32 = 2;
const NOT_DISTURBED_BIT: u32 = 4;

/// Whether an object's kind bits are a player's.
// D2-08058
#[must_use]
pub const fn is_player(kind: u32) -> bool {
    kind & 0x1F == 0x1F
}

/// A friend: the name, the mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Friend {
    pub name: String,
    pub mode: u32,
}

/// The friends page's panel.
#[derive(Debug)]
pub struct FriendsPage {
    frame: FrameId,
    pub online: Vec<Friend>,
    pub offline: Vec<Friend>,
    page: Option<Handle>,
    list: Option<Handle>,
    add: Option<Handle>,
    remove: Option<Handle>,
    status: Option<Handle>,
    /// The game as last shown: the player, its name, the selection (a player or not, its name),
    /// whether the player moves.
    player: u64,
    player_name: String,
    selection: Option<(bool, String)>,
    moving: bool,
}

impl FriendsPage {
    /// The page under the social window: its parts found; the state menu filled, available
    /// chosen; its buttons, its list, the state menu and the clock heard; the buttons lit as
    /// they may be.
    // D2-02803
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let page = ui.element(name::PAGE);
        let mut me = Self {
            frame,
            online: Vec::new(),
            offline: Vec::new(),
            page,
            list: None,
            add: None,
            remove: None,
            status: None,
            player: 0,
            player_name: String::new(),
            selection: None,
            moving: false,
        };
        let Some(p) = page else { return me };
        me.list = ui.child_recursive(p, name::LIST);
        me.add = ui.child_recursive(p, name::ADD_BUTTON);
        me.remove = ui.child_recursive(p, name::REMOVE_BUTTON);
        me.status = ui.child_recursive(p, name::STATUS_MENU);
        if let Some(m) = me.status {
            for (i, (_, label, _)) in STATUSES.iter().enumerate() {
                let words = write(ui, &entry(ui, label));
                ui.menu_insert_text(m, &words, i);
            }
            ui.menu_select_index(m, 0);
        }
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::ADD_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::REMOVE_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::LIST, msg::NEW_SELECTION);
            ui.listen_for_message(l, name::STATUS_MENU, msg::NEW_SELECTION);
            ui.listen_for_global_message(l, global::TIME);
        }
        me.update_button_states(ui);
        me
    }

    fn ghost(ui: &mut Ui, b: Option<Handle>, on: bool) {
        if let Some(b) = b {
            ui.set_attr(b, attr::GHOSTED, auberean_ui::Value::Bool(on));
        }
    }

    /// The state chosen on the menu.
    fn chosen(&self, ui: &Ui) -> Option<&(u32, &'static str, &'static str)> {
        self.status
            .and_then(|m| ui.menu_selected_index(m))
            .and_then(|i| STATUSES.get(i))
    }

    /// The clock: moving while away sets the player available.
    // D2-02780
    pub fn tick(&self, ui: &mut Ui) {
        if self.chosen(ui).is_some_and(|(mode, _, _)| *mode == AWAY) && self.moving {
            if let Some(m) = self.status {
                ui.menu_select_index(m, 0);
            }
        }
    }

    /// The chat service added a friend: the friends asked for again.
    // D2-02781
    pub fn add_friend_result(&self, out: &mut Vec<PlayRequest>) {
        if self.player != 0 {
            out.push(PlayRequest::Social(Request::GetFriends));
        }
    }

    /// The player's own state (a mode): the menu's row for it chosen.
    // D2-02782
    pub fn local_status(&self, ui: &mut Ui, mode: u32) {
        if let Some(i) = STATUSES.iter().position(|(m, _, _)| *m == mode) {
            if let Some(m) = self.status {
                ui.menu_select_index(m, i);
            }
        }
    }

    /// Add pressed: the one selected (a player) asked to be added.
    // D2-02787, D2-02784, D2-02786
    pub fn handle_add_button(&self, out: &mut Vec<PlayRequest>) {
        if self.page.is_none() || self.player == 0 {
            return;
        }
        if let Some((true, who)) = &self.selection {
            out.push(PlayRequest::Social(Request::AddFriend(who.clone())));
        }
    }

    /// Remove pressed: the friend whose row is selected asked to be taken off (no name when
    /// none is).
    // D2-02788
    pub fn handle_remove_button(&self, ui: &Ui, out: &mut Vec<PlayRequest>) {
        if self.player == 0 {
            return;
        }
        let who = self
            .list
            .and_then(|l| ui.list_selected_item(l))
            .and_then(|i| ui.child_recursive(i, name::FRIEND_NAME))
            .and_then(|n| ui.node(n))
            .and_then(|n| n.text_attr(name::NAME_ID).map(str::to_owned))
            .unwrap_or_default();
        out.push(PlayRequest::Social(Request::RemoveFriend(who)));
    }

    /// The state menu chose: the player's state set with the chat service (nowhere), its emote
    /// played.
    // D2-02789
    pub fn handle_status_change(&self, ui: &Ui, out: &mut Vec<PlayRequest>) {
        let Some(&(mode, _, emote)) = self.chosen(ui) else {
            return;
        };
        if self.player == 0 {
            return;
        }
        out.push(PlayRequest::Social(Request::SetLocation {
            mode,
            location: String::new(),
        }));
        let table = ui
            .assets()
            .data_id_by_enum(EMOTES, crate::flow::TABLE_MAPPING);
        out.push(PlayRequest::Social(Request::Emote {
            string: Token::of(emote).raw(),
            table,
        }));
    }

    fn known(&self, who: &str) -> bool {
        self.online
            .iter()
            .chain(&self.offline)
            .any(|f| f.name == who)
    }

    /// Remove lit when a friend's row (not a heading) is selected; with none selected, Add lit
    /// when the selection is a player other than the player and not a friend.
    // D2-02796
    pub fn update_button_states(&self, ui: &mut Ui) {
        Self::ghost(ui, self.remove, true);
        Self::ghost(ui, self.add, true);
        let selected = self.list.and_then(|l| ui.list_selected_item(l));
        if let Some(row) = selected {
            let id = ui.node(row).map(|n| n.id);
            if id != Some(name::ONLINE_MARKER) && id != Some(name::OFFLINE_MARKER) {
                Self::ghost(ui, self.remove, false);
            }
            return;
        }
        if self.player == 0 {
            return;
        }
        let Some((true, who)) = &self.selection else {
            return;
        };
        if *who != self.player_name && !self.known(who) {
            Self::ghost(ui, self.add, false);
        }
    }

    /// The selection changed: no row selected; the selection's row selected when it is a friend
    /// (counting the headings); the buttons lit as they may be.
    // D2-02799
    pub fn selection_changed(&self, ui: &mut Ui) {
        let Some(l) = self.list else { return };
        ui.list_select_item(l, None, false);
        if let Some((true, who)) = &self.selection {
            let on = self.online.iter().position(|f| f.name == *who);
            let off = self.offline.iter().position(|f| f.name == *who);
            if on.is_some() || off.is_some() {
                let before = on.unwrap_or(self.online.len());
                let at = off.map_or(before + 1, |k| before + 2 + k);
                ui.list_select_index(l, at, false);
            }
        }
        self.update_button_states(ui);
    }

    /// The list made again: the online heading and each friend online (lit by its state, with a
    /// word for away or not to be disturbed), the offline heading and each friend offline.
    // D2-02800
    pub fn build_ui_list(&self, ui: &mut Ui) {
        let Some(l) = self.list else { return };
        ui.list_flush(l);
        let make = |ui: &mut Ui, n| ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, n);
        if !self.online.is_empty() {
            if let Some(m) = make(ui, name::ONLINE_MARKER) {
                ui.list_multi_add(l, m);
            }
            for f in &self.online {
                let Some(row) = make(ui, name::TEMPLATE) else {
                    continue;
                };
                if let Some(n) = ui.child_recursive(row, name::FRIEND_NAME) {
                    ui.set_attr(
                        n,
                        name::NAME_ID,
                        auberean_ui::Value::Unicode(f.name.clone()),
                    );
                    let mut text = f.name.clone();
                    let (look, word) = if f.mode & AWAY_BIT != 0 {
                        (name::AWAY, Some("ID_FriendStatus_AwayFromKeyboardMessage"))
                    } else if f.mode & NOT_DISTURBED_BIT != 0 {
                        (
                            name::NOT_DISTURBED,
                            Some("ID_FriendStatus_DoNotDisturbMessage"),
                        )
                    } else {
                        (name::ONLINE, None)
                    };
                    ui.set_state(row, look);
                    if let Some(w) = word {
                        text.push_str(&write(ui, &entry(ui, w)));
                    }
                    ui.set_text(n, &text);
                }
                ui.list_multi_add(l, row);
            }
        }
        if !self.offline.is_empty() {
            if let Some(m) = make(ui, name::OFFLINE_MARKER) {
                ui.list_multi_add(l, m);
            }
            for f in &self.offline {
                let Some(row) = make(ui, name::TEMPLATE) else {
                    continue;
                };
                if let Some(n) = ui.child_recursive(row, name::FRIEND_NAME) {
                    ui.set_attr(
                        n,
                        name::NAME_ID,
                        auberean_ui::Value::Unicode(f.name.clone()),
                    );
                    ui.set_text(n, &f.name);
                    ui.set_state(row, name::OFFLINE);
                }
                ui.list_multi_add(l, row);
            }
        }
        ui.list_changes_done(l);
        self.update_button_states(ui);
    }

    /// The chat service took a friend off: off the list (online first, else offline).
    // D2-02801
    pub fn remove_friend_result(&mut self, ui: &mut Ui, who: &str) {
        if let Some(i) = self.online.iter().position(|f| f.name == who) {
            self.online.remove(i);
        } else if let Some(i) = self.offline.iter().position(|f| f.name == who) {
            self.offline.remove(i);
        } else {
            return;
        }
        self.build_ui_list(ui);
    }

    /// The chat service's friends: each named one online (a mode) or offline, the list made
    /// again.
    // D2-02807
    pub fn friends_list_result(&mut self, ui: &mut Ui, all: &[UserStatus]) {
        if self.list.is_none() {
            return;
        }
        self.online.clear();
        self.offline.clear();
        for s in all.iter().filter(|s| !s.name.is_empty()) {
            let f = Friend {
                name: s.name.clone(),
                mode: s.mode,
            };
            if s.mode == 0 {
                self.offline.push(f);
            } else {
                self.online.push(f);
            }
        }
        self.build_ui_list(ui);
    }

    /// A friend's state: gone offline (moved there from online), come online (moved from
    /// offline) or another mode; the list made again.
    // D2-02808
    pub fn friend_status_result(&mut self, ui: &mut Ui, s: &UserStatus) {
        let on = self.online.iter().position(|f| f.name == s.name);
        let off = self.offline.iter().position(|f| f.name == s.name);
        let moved = Friend {
            name: s.name.clone(),
            mode: s.mode,
        };
        if s.mode == 0 {
            if let Some(i) = on {
                self.online.remove(i);
                self.offline.push(moved);
            }
        } else if let Some(i) = off {
            self.offline.remove(i);
            self.online.push(moved);
        } else if let Some(i) = on {
            self.online[i].mode = s.mode;
        }
        self.build_ui_list(ui);
    }
}

impl FriendsPage {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x472, 0x473, 0x474, 0x475, 0x408, 0x476];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for FriendsPage {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Add, Remove, a row selected (the buttons lit as they may be), the state menu; the clock.
    // D2-02798
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<PlayRequest>) {
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } => self.tick(ui),
            Mail::Element {
                element,
                what,
                from,
                ..
            } => {
                if element == name::ADD_BUTTON {
                    self.handle_add_button(out);
                } else if element == name::REMOVE_BUTTON {
                    self.handle_remove_button(ui, out);
                } else if element == name::LIST {
                    if what == msg::NEW_SELECTION {
                        self.update_button_states(ui);
                    }
                } else if Some(from) == self.status {
                    self.handle_status_change(ui, out);
                }
            }
            _ => {}
        }
    }

    /// The chat service's answers and the selection.
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<PlayRequest>) {
        let name_of = |i: usize| match n.records.get(i) {
            Some(Some(Record::Text(t))) => Some(String::from_utf16_lossy(&t.literal_value(false))),
            _ => None,
        };
        match n.native {
            notice::ADD_FRIEND => self.add_friend_result(out),
            notice::REMOVE_FRIEND => {
                if let Some(who) = name_of(0) {
                    self.remove_friend_result(ui, &who);
                }
            }
            notice::GET_FRIENDS_LIST => {
                if let Some(Some(Record::Friends(all))) = n.records.first() {
                    let all = all.clone();
                    self.friends_list_result(ui, &all);
                }
            }
            notice::FRIEND_STATUS => {
                if let Some(Some(Record::Friend(s))) = n.records.first() {
                    let s = s.clone();
                    self.friend_status_result(ui, &s);
                }
            }
            notice::LOCAL_STATUS => self.local_status(ui, n.word()),
            notice::SELECTION_CHANGED => self.selection_changed(ui),
            _ => {}
        }
    }

    /// The game as shown this frame kept.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, _out: &mut Vec<PlayRequest>) {
        let Some(w) = view.world else { return };
        self.player = w.player;
        self.player_name = w
            .thing(w.player)
            .map(|t| t.name.clone())
            .unwrap_or_default();
        let selection = w
            .thing(w.selection)
            .map(|t| (is_player(t.kind), t.name.clone()));
        // The notice of a new selection comes before this frame's view of it.
        let changed = selection != self.selection;
        self.selection = selection;
        if changed {
            self.selection_changed(ui);
        }
        self.moving = w.moving;
    }

    // D2-02804
    fn close(self: Box<Self>, ui: &mut Ui) {
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
