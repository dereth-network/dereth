//! The social window's fellowship page.
//!
//! With no fellowship it offers to make one: a name, whether experience is shared, how loot is
//! shared (five ways, the second chosen), and Create. In a fellowship it shows the fellowship's
//! name, the experience and loot settings it was made with (their buttons ghosted, their tooltips
//! saying which), and a row per fellow; Leave, and for the leader only Disband, Recruit (the one
//! selected) and Dismiss (the fellow selected). The fellows' numbers toggle with the number
//! button. Someone offering the player a place asks first; the answer goes back to the scripts.

use std::any::Any;

use auberean_text::{StringInfo, Value};
use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg};
use auberean_ui::{Handle, Mail, Token, Ui};

use super::fellows::{Facts, Fellows};
use super::{entry, notice, write, Fellowship, Request};
use crate::dialog;
use crate::gameplay::{
    dialog as kind, Child, Dialog, Notice, Record, Request as PlayRequest, View,
};

mod name {
    use auberean_ui::Token;
    pub const PAGE: Token = Token::of("FellowshipPage");
    pub const LIST: Token = Token::of("FellowshipListBox");
    pub const CREATE_BUTTON: Token = Token::of("FellowshipCreateButton");
    pub const LEAVE_BUTTON: Token = Token::of("FellowshipLeaveButton");
    pub const RECRUIT_BUTTON: Token = Token::of("FellowshipRecruitButton");
    pub const DISBAND_BUTTON: Token = Token::of("FellowshipDisbandButton");
    pub const DISMISS_BUTTON: Token = Token::of("FellowshipDismissButton");
    pub const NUMBER_BUTTON: Token = Token::of("FellowshipNumberButton");
    pub const NAME_TEXT: Token = Token::of("FellowshipNameText");
    pub const XP_BUTTON: Token = Token::of("FellowshipXPButton");
    pub const LOOT_FIELD: Token = Token::of("FellowshipLootField");
    pub const LOOTING_MENU: Token = Token::of("LootingMethodMenu");
    pub const NAME_ENTRY: Token = Token::of("NoFellowshipNameEntryText");
    pub const XP_TOGGLE: Token = Token::of("NoFellowshipXPToggleButton");
    pub const FELLOWSHIP: Token = Token::of("fellowship");
    pub const NO_FELLOWSHIP: Token = Token::of("nofellowship");
    pub const LEADER: Token = Token::of("leader");
    pub const NAME: Token = Token::of("name");
}

/// How loot is shared: each way's name, its tooltip on the menu, the loot field's state for it and
/// the field's tooltip.
const LOOTING: [(&str, &str, &str, &str); 5] = [
    (
        "ID_Fellowship_LootMethod0",
        "ID_Fellowship_TT_LootMethod0",
        "method0",
        "ID_Fellowship_TT_LootButtonMethod0",
    ),
    (
        "ID_Fellowship_LootMethod1",
        "ID_Fellowship_TT_LootMethod1",
        "method1",
        "ID_Fellowship_TT_LootButtonMethod1",
    ),
    (
        "ID_Fellowship_LootMethod2",
        "ID_Fellowship_TT_LootMethod2",
        "method2",
        "ID_Fellowship_TT_LootButtonMethod2",
    ),
    (
        "ID_Fellowship_LootMethod3",
        "ID_Fellowship_TT_LootMethod3",
        "method3",
        "ID_Fellowship_TT_LootButtonMethod3",
    ),
    (
        "ID_Fellowship_LootMethod4",
        "ID_Fellowship_TT_LootMethod4",
        "method4",
        "ID_Fellowship_TT_LootButtonMethod4",
    ),
];

/// Why the player turned down a place offered.
pub const DECLINED: u32 = 0x138D;

/// The fellowship page's panel.
#[derive(Debug)]
pub struct FellowshipPage {
    frame: FrameId,
    page: Option<Handle>,
    /// The fellows' rows, while in a fellowship.
    pub fellows: Option<Fellows>,
    name: String,
    social: i32,
    looting: i32,
    /// The question of a place offered (0: none).
    recruit: u32,
    /// The notices heard, acted on with the frame's game.
    pending: Vec<Notice>,
    /// The game as last shown: the player, the selection, the options, the shares.
    player: u64,
    selection: u64,
    options: Option<u32>,
    shares: Vec<(u64, f32)>,
}

/// The game's facts the page acts on, from what the frame shows.
fn facts_of(view: &View<'_>) -> (u64, u64, Option<u32>, Vec<(u64, f32)>) {
    let w = view.world;
    (
        w.map_or(0, |w| w.player),
        w.map_or(0, |w| w.selection),
        view.options.map(|o| o.bits),
        w.map(|w| w.fellow_xp.clone()).unwrap_or_default(),
    )
}

impl FellowshipPage {
    /// The page under the social window: its page found; the looting menu filled; its buttons
    /// and the menu heard; with no fellowship.
    // D2-02758
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let page = ui.element(name::PAGE);
        let me = Self {
            frame,
            page,
            fellows: None,
            name: String::new(),
            social: 0,
            looting: 0,
            recruit: 0,
            pending: Vec::new(),
            player: 0,
            selection: 0,
            options: None,
            shares: Vec::new(),
        };
        if page.is_none() {
            return me;
        }
        me.init_looting_menu(ui);
        if let Some(l) = ui.frame_listener(frame) {
            for b in [
                name::CREATE_BUTTON,
                name::LEAVE_BUTTON,
                name::RECRUIT_BUTTON,
                name::DISBAND_BUTTON,
                name::DISMISS_BUTTON,
                name::NUMBER_BUTTON,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
            ui.listen_for_message(l, name::LOOTING_MENU, msg::NEW_SELECTION);
        }
        me.set_no_fellowship_state(ui);
        me
    }

    fn part(&self, ui: &Ui, n: Token) -> Option<Handle> {
        self.page.and_then(|p| ui.child_recursive(p, n))
    }

    fn facts(&self) -> Facts<'_> {
        Facts {
            player: self.player,
            options: self.options,
            shares: &self.shares,
        }
    }

    /// The looting menu filled again: each way with its tooltip, the second chosen.
    // D2-02741
    pub fn init_looting_menu(&self, ui: &mut Ui) {
        let Some(m) = self.part(ui, name::LOOTING_MENU) else {
            return;
        };
        ui.menu_flush(m);
        for (i, (label, tip, _, _)) in LOOTING.iter().enumerate() {
            let words = write(ui, &entry(ui, label));
            let Some(item) = ui.menu_insert_text(m, &words, i) else {
                return;
            };
            let tip = write(ui, &entry(ui, tip));
            ui.set_tooltip(item, &tip);
            ui.set_attr(item, attr::TOOLTIPS_ON, auberean_ui::Value::Bool(true));
        }
        ui.menu_select_index(m, 1);
    }

    /// Recruit pressed: the one selected asked in.
    // D2-02742
    pub fn handle_recruit_button(&self, out: &mut Vec<PlayRequest>) {
        if self.player != 0 && self.selection != 0 {
            out.push(PlayRequest::Social(Request::Recruit(self.selection)));
        }
    }

    /// Dismiss pressed: the one selected sent out.
    // D2-02743
    pub fn handle_dismiss_button(&self, out: &mut Vec<PlayRequest>) {
        if self.player != 0 && self.selection != 0 {
            out.push(PlayRequest::Social(Request::Dismiss(self.selection)));
        }
    }

    /// Whether the player leads: Disband, Recruit and Dismiss ghosted unless so.
    // D2-02744
    pub fn set_leader_state(&self, ui: &mut Ui, leader: bool) {
        for b in [
            name::DISBAND_BUTTON,
            name::RECRUIT_BUTTON,
            name::DISMISS_BUTTON,
        ] {
            let Some(h) = self.part(ui, b) else { return };
            ui.set_attr(h, attr::GHOSTED, auberean_ui::Value::Bool(!leader));
        }
    }

    /// The question of a place offered (unless one is up): the leader's name, the fellowship's.
    // D2-02745
    pub fn make_recruit_dialog(
        &mut self,
        ui: &Ui,
        leader: &StringInfo,
        fellowship: &StringInfo,
        out: &mut Vec<PlayRequest>,
    ) {
        if self.recruit != 0 {
            return;
        }
        let mut s = entry(ui, "ID_Fellowship_RecruitText");
        s.add_var(name::LEADER.raw(), Value::Text(Box::new(leader.clone())));
        s.add_var(name::NAME.raw(), Value::Text(Box::new(fellowship.clone())));
        let id = dialog::new_context();
        out.push(PlayRequest::Dialog(Dialog {
            id,
            kind: kind::CONFIRMATION,
            text: write(ui, &s),
            modal: false,
        }));
        self.recruit = id;
    }

    /// A question closed: the place offered taken (yes) or turned down.
    // D2-02746
    pub fn close_confirmation_dialog(&mut self, id: u32, yes: bool, out: &mut Vec<PlayRequest>) {
        if self.recruit != id || self.player == 0 {
            return;
        }
        out.push(PlayRequest::Social(if yes {
            Request::AcceptRecruitment
        } else {
            Request::DeclineRecruitment(DECLINED)
        }));
        self.recruit = 0;
    }

    /// The offer ran out: its question taken down.
    // D2-02748
    pub fn expire_recruitment(&mut self, out: &mut Vec<PlayRequest>) {
        if self.recruit != 0 {
            out.push(PlayRequest::DialogDown(self.recruit));
        }
        self.recruit = 0;
    }

    /// Create pressed: a fellowship asked for with the name entered, whether the experience
    /// button is lit (sent as its opposite) and the way loot is shared.
    // D2-02751
    pub fn handle_create_button(&self, ui: &Ui, out: &mut Vec<PlayRequest>) {
        if self.player == 0 {
            return;
        }
        let Some(entry_text) = self.part(ui, name::NAME_ENTRY) else {
            return;
        };
        let text = ui.text(entry_text).unwrap_or_default();
        let Some(toggle) = self.part(ui, name::XP_TOGGLE) else {
            return;
        };
        let lit = ui
            .node(toggle)
            .is_some_and(|n| n.bool_attr(attr::HIGHLIGHTED));
        let looting = self
            .part(ui, name::LOOTING_MENU)
            .and_then(|m| ui.menu_selected_index(m))
            .map_or(u32::MAX, |i| u32::try_from(i).unwrap_or(u32::MAX));
        out.push(PlayRequest::Social(Request::CreateFellowship {
            name: text,
            social: !lit,
            looting,
        }));
    }

    /// In a fellowship: the page in its fellowship state, the name, the experience button lit as
    /// it is shared and ghosted, its tooltip; the loot field in the way's state, its tooltip.
    // D2-02752
    pub fn set_fellowship_state(&self, ui: &mut Ui) {
        let Some(p) = self.page else { return };
        if ui.node(p).map(|n| n.state) != Some(name::FELLOWSHIP) {
            ui.set_state(p, name::FELLOWSHIP);
        }
        let Some(t) = self.part(ui, name::NAME_TEXT) else {
            return;
        };
        ui.set_text(t, &self.name);
        let Some(xp) = self.part(ui, name::XP_BUTTON) else {
            return;
        };
        ui.set_attr(
            xp,
            attr::HIGHLIGHTED,
            auberean_ui::Value::Bool(self.social != 0),
        );
        ui.set_attr(xp, attr::GHOSTED, auberean_ui::Value::Bool(true));
        let tip = if self.social != 0 {
            "ID_Fellowship_TT_XPButtonOff"
        } else {
            "ID_Fellowship_TT_XPButtonOn"
        };
        let tip = write(ui, &entry(ui, tip));
        ui.set_tooltip(xp, &tip);
        if let Some(f) = self.part(ui, name::LOOT_FIELD) {
            let way = usize::try_from(self.looting)
                .ok()
                .and_then(|i| LOOTING.get(i));
            if let Some((_, _, st, tip)) = way {
                ui.set_state(f, Token::of(st));
                let tip = write(ui, &entry(ui, tip));
                ui.set_tooltip(f, &tip);
            }
        }
    }

    /// The fellowship described: its name, experience and loot settings kept and shown; whether
    /// the player leads; the fellows' rows made again.
    // D2-02753
    pub fn update_fellowship(&mut self, ui: &mut Ui, f: &Fellowship, out: &mut Vec<PlayRequest>) {
        if self.page.is_none() {
            return;
        }
        self.name.clone_from(&f.name);
        self.social = f.social;
        self.looting = f.looting;
        self.set_fellowship_state(ui);
        if self.player != 0 {
            self.set_leader_state(ui, f.leader == self.player);
        }
        if let Some(old) = self.fellows.take() {
            let facts = self.facts();
            old.close(ui, &facts, out);
        }
        let list = self.part(ui, name::LIST);
        if list.is_some() {
            let facts = self.facts();
            self.fellows = Some(Fellows::new(ui, list, f, false, &facts, out));
        }
    }

    /// No fellowship: the page in its no-fellowship state, no name, the looting menu filled again.
    // D2-02756
    pub fn set_no_fellowship_state(&self, ui: &mut Ui) {
        let Some(p) = self.page else { return };
        ui.set_state(p, name::NO_FELLOWSHIP);
        if let Some(t) = self.part(ui, name::NAME_TEXT) {
            ui.set_text(t, "");
            self.init_looting_menu(ui);
        }
    }

    /// The fellowship gone: the rows gone, the page with no fellowship.
    // D2-02757
    pub fn clear_fellowship(&mut self, ui: &mut Ui, out: &mut Vec<PlayRequest>) {
        if let Some(old) = self.fellows.take() {
            let facts = self.facts();
            old.close(ui, &facts, out);
        }
        self.set_no_fellowship_state(ui);
    }

    /// A notice of the player's scripts, acted on with the frame's game.
    // D2-02747, D2-02749
    fn act(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<PlayRequest>) {
        match n.native {
            notice::UPDATE_FELLOWSHIP => {
                if let Some(Some(Record::Fellowship(f))) = n.records.first() {
                    let f = f.as_ref().clone();
                    self.update_fellowship(ui, &f, out);
                }
            }
            notice::CLEAR_FELLOWSHIP => self.clear_fellowship(ui, out),
            notice::REQUEST_RECRUITMENT => {
                let text = |i: usize| match n.records.get(i) {
                    Some(Some(Record::Text(s))) => s.clone(),
                    _ => StringInfo::default(),
                };
                let (leader, fellowship) = (text(2), text(3));
                self.make_recruit_dialog(ui, &leader, &fellowship, out);
            }
            notice::EXPIRE_RECRUITMENT => self.expire_recruitment(out),
            notice::UPDATE_LEADER => {
                if self.page.is_some() && self.player != 0 {
                    let leader = n.long(0) == self.player;
                    self.set_leader_state(ui, leader);
                }
            }
            notice::CLOSE_CONFIRMATION => {
                if let Some((id, yes)) = dialog::confirmation_closed(n) {
                    self.close_confirmation_dialog(id, yes, out);
                }
            }
            _ => {}
        }
        let facts = Facts {
            player: self.player,
            options: self.options,
            shares: &self.shares,
        };
        if let Some(f) = &mut self.fellows {
            f.notice(ui, n, &facts, out);
        }
    }
}

impl Child for FellowshipPage {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Its buttons: Create, Leave, Recruit, Disband, Dismiss, and the number button (the fellows'
    /// numbers shown or hidden).
    // D2-02755
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<PlayRequest>) {
        let Mail::Element { element, .. } = *mail else {
            return;
        };
        match element {
            e if e == name::CREATE_BUTTON => self.handle_create_button(ui, out),
            e if e == name::LEAVE_BUTTON => {
                if self.player != 0 {
                    out.push(PlayRequest::Social(Request::LeaveFellowship));
                }
            }
            e if e == name::RECRUIT_BUTTON => self.handle_recruit_button(out),
            e if e == name::DISBAND_BUTTON => {
                if self.player != 0 {
                    out.push(PlayRequest::Social(Request::DisbandFellowship));
                }
            }
            e if e == name::DISMISS_BUTTON => self.handle_dismiss_button(out),
            e if e == name::NUMBER_BUTTON => {
                if let Some(f) = &self.fellows {
                    f.toggle_text_state(ui);
                }
            }
            _ => {}
        }
    }

    /// Heard now, acted on with the frame's game.
    fn notice(&mut self, _ui: &mut Ui, n: &Notice, _out: &mut Vec<PlayRequest>) {
        self.pending.push(n.clone());
    }

    /// The frame's game kept; the notices heard acted on; the rows' presses and keys.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<PlayRequest>) {
        let (player, selection, options, shares) = facts_of(view);
        self.player = player;
        self.selection = selection;
        if options.is_some() {
            self.options = options;
        }
        // The shares are asked of the scripts after the notices that change them, so they come a
        // frame later: shown as they come.
        let again = shares != self.shares;
        self.shares = shares;
        if view.world.is_some_and(|w| view.gone.contains(&w.player)) {
            if let Some(f) = &mut self.fellows {
                f.being_deleted();
            }
        }
        for n in std::mem::take(&mut self.pending) {
            self.act(ui, &n, out);
        }
        let facts = Facts {
            player: self.player,
            options: self.options,
            shares: &self.shares,
        };
        if let Some(f) = &mut self.fellows {
            if again {
                f.update_xp(ui, &facts);
            }
            f.hear(ui, out);
        }
    }

    /// It goes: the question taken down, the rows gone.
    // D2-02750, D2-02754
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        let mut out = Vec::new();
        if let Some(f) = self.fellows.take() {
            let facts = self.facts();
            f.close(ui, &facts, &mut out);
        }
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
