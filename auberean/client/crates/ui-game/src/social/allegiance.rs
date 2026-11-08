//! The social window's allegiance page.
//!
//! The page shows one member of the player's allegiance at a time (the player, at first): the
//! allegiance's name and size, its monarch, the member (title, name, level, race and sex; lit as
//! the player, or as online or offline), its patron, and each of its vassals (with the experience
//! each passed up) or that it has none, and the message of the day. Home goes back to the player,
//! Up to the member's patron, a vassal's Down to that vassal; each asks the scripts for the
//! profile about that member, and the page waits for it. With no allegiance the page says so.
//!
//! Swear (lit when the player has no patron and has selected another player) offers the player's
//! oath; Break (lit when the member shown is the player's patron or vassal, or the player is an
//! officer) asks first, then breaks with the member shown. Someone offering an oath to the player
//! is asked about; the answer goes back to the scripts. The page asks for profiles only while it
//! shows: hidden, it asks for none.

use std::any::Any;

use auberean_text::{StringInfo, Value};
use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::{Handle, Mail, Token, Ui};

use super::{entry, notice, set_string_info, write, Member, Profile, Request};
use crate::dialog;
use crate::gameplay::{
    dialog as kind, Child, Dialog, Notice, Record, Request as PlayRequest, View,
};
use crate::shortcuts::TOOLBAR_LAYOUT_ENUM;

mod name {
    use auberean_ui::Token;
    pub const PAGE: Token = Token::of("AllegiancePage");
    pub const TAB: Token = Token::of("AllegianceTab");
    pub const SOCIAL_FIELD: Token = Token::of("SocialField");
    pub const SWEAR_BUTTON: Token = Token::of("SwearButton");
    pub const BREAK_BUTTON: Token = Token::of("BreakButton");
    pub const HOME_BUTTON: Token = Token::of("HomeButton");
    pub const UP_BUTTON: Token = Token::of("UpButton");
    pub const DOWN_BUTTON: Token = Token::of("DownButton");
    pub const ALLEGIANCE_NAME: Token = Token::of("AllegianceName");
    pub const ALLEGIANCE_SIZE: Token = Token::of("AllegianceSize");
    pub const MONARCH_NAME: Token = Token::of("MonarchName");
    pub const MEMBER_LIST: Token = Token::of("MemberListBox");
    pub const MEMBER_ELEMENT: Token = Token::of("MemberElement");
    pub const VASSAL_ELEMENT: Token = Token::of("VassalElement");
    pub const NO_VASSALS_ELEMENT: Token = Token::of("NoVassalsElement");
    pub const MOTD_ELEMENT: Token = Token::of("MotDTextElement");
    pub const MEMBER_NAME_TEXT: Token = Token::of("MemberNameText");
    pub const MEMBER_MISC_TEXT: Token = Token::of("MemberMiscText");
    pub const CONNECT_GRAPHIC: Token = Token::of("ConnectGraphic");
    pub const IID: Token = Token::of("iid");
    pub const OPEN: Token = Token::of("open");
    pub const OPENED: Token = Token::of("opened");
    pub const CLOSED: Token = Token::of("closed");
    pub const PENDING: Token = Token::of("pending");
    pub const ALLEGIANCE: Token = Token::of("allegiance");
    pub const NO_ALLEGIANCE: Token = Token::of("noallegiance");
    pub const SELF: Token = Token::of("self");
    pub const ONLINE: Token = Token::of("online");
    pub const OFFLINE: Token = Token::of("offline");
    pub const ACTIVE_ONLINE: Token = Token::of("activememberonline");
    pub const ACTIVE_OFFLINE: Token = Token::of("activememberoffline");
    pub const END: Token = Token::of("end");
    pub const TITLE: Token = Token::of("title");
    pub const VALUE: Token = Token::of("value");
    pub const LEVEL: Token = Token::of("level");
    pub const SPECIES_GENDER: Token = Token::of("speciesgender");
}

/// Why the player turned down an oath offered.
pub const DECLINED: u32 = 0x1585;

/// The string table of the races and sexes, and of the message of the day, by enumeration value.
const RACE_AND_SEX_TABLE: u32 = 0x4100_000B;
const MOTD_TABLE: u32 = 0x4100_0001;

/// Whether an instance id is a player's: the high byte of its high word marks its kind.
// D2-16948, D2-16947
#[must_use]
pub const fn is_player_id(id: u64) -> bool {
    let kind = (id >> 56) as u32;
    kind & 0x80 == 0 && kind & 0x40 == 0 && kind & 0x20 != 0
}

/// A member's race and sex, as a string reference: none (an empty literal) when the sex is not
/// known, or the race is none of the five; a drudge's comes out empty too, as the client's does.
// D2-12378
#[must_use]
pub fn species_gender(ui: &Ui, m: &Member) -> StringInfo {
    let table = ui
        .assets()
        .data_id_by_enum(RACE_AND_SEX_TABLE, crate::flow::TABLE_MAPPING);
    let empty = || StringInfo {
        table,
        ..StringInfo::literal("")
    };
    if m.gender == 0 {
        return empty();
    }
    let male = m.gender == 0x1000;
    let (him, her) = match m.species {
        1 => ("ID_RaceAndSex_HumanMale", "ID_RaceAndSex_HumanFemale"),
        2 => ("ID_RaceAndSex_LugianMale", "ID_RaceAndSex_LugianFemale"),
        4 => ("ID_RaceAndSex_TumerokMale", "ID_RaceAndSex_TumerokFemale"),
        16 => ("ID_RaceAndSex_EmpyreanMale", "ID_RaceAndSex_EmpyreanFemale"),
        8 => {
            let id = Token::of(if male {
                "ID_RaceAndSex_DrudgeMale"
            } else {
                "ID_RaceAndSex_DrudgeFemale"
            });
            return StringInfo {
                string_id: id.raw(),
                ..empty()
            };
        }
        _ => return empty(),
    };
    StringInfo::entry(table, Token::of(if male { him } else { her }).raw())
}

/// The allegiance's real monarch: its monarch, else the member's patron, else the member;
/// `None` when none is known.
// D2-12370
#[must_use]
pub fn real_monarch(p: &Profile) -> Option<&Member> {
    let m = if p.monarch.0.id != 0 || p.patron.0.id != 0 {
        if p.monarch.0.id != 0 {
            &p.monarch.0
        } else {
            &p.patron.0
        }
    } else {
        &p.member.0
    };
    (m.id != 0).then_some(m)
}

/// The allegiance page's panel.
#[derive(Debug)]
pub struct AllegiancePage {
    frame: FrameId,
    /// The social window's framework, whose showing the page follows.
    parent: FrameId,
    page: Option<Handle>,
    swear: Option<Handle>,
    breaks: Option<Handle>,
    /// The questions up: an oath offered, a break asked (0: none).
    accept_swear: u32,
    break_context: u32,
    /// The selection, as the scripts last said.
    selection: u64,
    /// The member shown, the player's patron, the player, the member's patron, the monarch.
    cur_target: u64,
    patron: u64,
    player: u64,
    cur_patron: u64,
    monarch: u64,
    officers: Vec<u64>,
    /// The member shown's name.
    cur_name: StringInfo,
    /// The player is in an allegiance; the player is a vassal of the member shown.
    in_allegiance: bool,
    vassal_is_self: bool,
    /// The notices heard, acted on with the frame's game.
    pending: Vec<Notice>,
}

impl AllegiancePage {
    /// The page under the social window: its page and buttons found; the social window's
    /// showing, the tab, the buttons heard. Once the player is known (the first frame of the
    /// world), the player is the member shown and profiles are asked for as the page shows.
    // D2-02776
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let page = ui.element(name::PAGE);
        let mut me = Self {
            frame,
            parent,
            page,
            swear: None,
            breaks: None,
            accept_swear: 0,
            break_context: 0,
            selection: 0,
            cur_target: 0,
            patron: 0,
            player: 0,
            cur_patron: 0,
            monarch: 0,
            officers: Vec::new(),
            cur_name: StringInfo::default(),
            in_allegiance: false,
            vassal_is_self: false,
            pending: Vec::new(),
        };
        let Some(p) = page else { return me };
        me.swear = ui.child_recursive(p, name::SWEAR_BUTTON);
        if me.swear.is_none() {
            return me;
        }
        me.breaks = ui.child_recursive(p, name::BREAK_BUTTON);
        if me.breaks.is_none() {
            return me;
        }
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::SOCIAL_FIELD, msg::SET_SHOW_STATE);
            ui.listen_for_message(l, name::TAB, name::OPENED);
            ui.listen_for_message(l, name::TAB, name::CLOSED);
            for b in [
                name::SWEAR_BUTTON,
                name::BREAK_BUTTON,
                name::HOME_BUTTON,
                name::UP_BUTTON,
                name::DOWN_BUTTON,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
        }
        me
    }

    /// The player known: the member shown; profiles asked for as the page shows.
    fn start(&mut self, ui: &mut Ui, player: u64, out: &mut Vec<PlayRequest>) {
        self.player = player;
        self.cur_target = player;
        self.update_show_state(ui, out);
    }

    /// The member shown.
    #[must_use]
    pub const fn target(&self) -> u64 {
        self.cur_target
    }

    fn page_state(&self, ui: &mut Ui, s: Token) {
        if let Some(p) = self.page {
            ui.set_state(p, s);
        }
    }

    /// The member shown, as the list's first row: its title, name, level, race and sex (title and
    /// name alone with no level), lit as the player or as online or offline; its patron (or that
    /// it has none), lit when the player; Home lit unless it is the player (the player's patron
    /// kept); Up ghosted with no patron.
    // D2-02759
    pub fn make_current(
        &mut self,
        ui: &mut Ui,
        member: &Member,
        online: bool,
        patron: &Member,
        el: Option<Handle>,
    ) {
        let Some(el) = el else { return };
        let mut s = entry(ui, "ID_Allegiance_TitleAndNameLevelSpeciesGender");
        let Some(name_text) = ui.child_recursive(el, name::MEMBER_NAME_TEXT) else {
            return;
        };
        s.add_var(
            name::TITLE.raw(),
            Value::Text(Box::new(member.title.clone())),
        );
        s.add_var(
            name::VALUE.raw(),
            Value::Text(Box::new(member.name.clone())),
        );
        s.add_var(name::LEVEL.raw(), Value::UInt(member.level));
        s.add_var(
            name::SPECIES_GENDER.raw(),
            Value::Text(Box::new(species_gender(ui, member))),
        );
        if member.level == 0 {
            s.string_id = Token::of("ID_Allegiance_TitleAndName").raw();
        }
        let look = if member.id == self.player {
            name::SELF
        } else if online {
            name::ACTIVE_ONLINE
        } else {
            name::ACTIVE_OFFLINE
        };
        ui.set_state(name_text, look);
        set_string_info(ui, name_text, &s);
        let Some(misc) = ui.child_recursive(el, name::MEMBER_MISC_TEXT) else {
            return;
        };
        if patron.id == 0 {
            s.string_id = Token::of("ID_Allegiance_NoPatron").raw();
        } else {
            s.string_id = Token::of("ID_Allegiance_PatronName").raw();
            s.add_var(
                name::VALUE.raw(),
                Value::Text(Box::new(patron.name.clone())),
            );
        }
        let look = if patron.id == self.player {
            name::SELF
        } else {
            state::STATELESS
        };
        ui.set_state(misc, look);
        set_string_info(ui, misc, &s);
        let Some(home) = ui.child_recursive(el, name::HOME_BUTTON) else {
            return;
        };
        self.cur_patron = patron.id;
        let is_player = member.id == self.player;
        if is_player {
            self.patron = self.cur_patron;
        }
        ui.set_attr(home, attr::GHOSTED, auberean_ui::Value::Bool(is_player));
        if let Some(up) = ui.child_recursive(el, name::UP_BUTTON) {
            let none = self.cur_patron == 0;
            ui.set_attr(up, attr::GHOSTED, auberean_ui::Value::Bool(none));
        }
    }

    /// A vassal's row: title and name, lit as the player (who is then the member's vassal) or as
    /// online or offline; its Down marked with its id; the experience it passed up, with its
    /// level, race and sex (or the experience alone with no level).
    // D2-02760
    pub fn make_vassal(&mut self, ui: &mut Ui, vassal: &Member, online: bool, el: Option<Handle>) {
        let Some(el) = el else { return };
        let mut s = entry(ui, "ID_Allegiance_TitleAndName");
        let Some(name_text) = ui.child_recursive(el, name::MEMBER_NAME_TEXT) else {
            return;
        };
        s.add_var(
            name::TITLE.raw(),
            Value::Text(Box::new(vassal.title.clone())),
        );
        s.add_var(
            name::VALUE.raw(),
            Value::Text(Box::new(vassal.name.clone())),
        );
        set_string_info(ui, name_text, &s);
        let look = if vassal.id == self.player {
            self.vassal_is_self = true;
            name::SELF
        } else if online {
            name::ONLINE
        } else {
            name::OFFLINE
        };
        ui.set_state(name_text, look);
        let Some(down) = ui.child_recursive(el, name::DOWN_BUTTON) else {
            return;
        };
        ui.set_attr(down, name::IID, auberean_ui::Value::InstanceId(vassal.id));
        let Some(misc) = ui.child_recursive(el, name::MEMBER_MISC_TEXT) else {
            return;
        };
        s.string_id = Token::of("ID_Allegiance_VassalLevelSpeciesGenderXp").raw();
        s.add_var(name::VALUE.raw(), Value::ULong(vassal.xp_passed));
        s.add_var(name::LEVEL.raw(), Value::UInt(vassal.level));
        s.add_var(
            name::SPECIES_GENDER.raw(),
            Value::Text(Box::new(species_gender(ui, vassal))),
        );
        if vassal.level == 0 {
            s.string_id = Token::of("ID_Allegiance_XpUp").raw();
        }
        set_string_info(ui, misc, &s);
    }

    /// Ask the scripts for the profile about a member (0: no longer); for a member, the page
    /// waits for it.
    // D2-02761
    pub fn send_request(&self, ui: &mut Ui, id: u64, out: &mut Vec<PlayRequest>) {
        if self.player == 0 {
            return;
        }
        out.push(PlayRequest::Social(Request::QueryAllegiance(id)));
        if id != 0 {
            self.page_state(ui, name::PENDING);
        }
    }

    /// Swear lit only with no question up, no patron, and another player selected; Break lit
    /// only with no question up, in an allegiance, a member shown other than the player, and the
    /// member the player's patron or vassal (or the player an officer).
    // D2-02762
    pub fn update_buttons(&self, ui: &mut Ui) {
        let (Some(swear), Some(breaks)) = (self.swear, self.breaks) else {
            return;
        };
        let break_ghosted = self.cur_target == self.player
            || self.break_context != 0
            || self.accept_swear != 0
            || self.cur_target == 0
            || !self.in_allegiance
            || (self.cur_target != self.patron
                && self.cur_patron != self.player
                && !self.officers.contains(&self.player));
        let swear_lit = self.break_context == 0
            && self.accept_swear == 0
            && self.patron == 0
            && self.selection != 0
            && self.selection != self.player
            && is_player_id(self.selection);
        ui.set_attr(swear, attr::GHOSTED, auberean_ui::Value::Bool(!swear_lit));
        ui.set_attr(
            breaks,
            attr::GHOSTED,
            auberean_ui::Value::Bool(break_ghosted),
        );
    }

    /// Show a member (unless a break is being asked).
    // D2-02763
    pub fn focus_tree(&self, ui: &mut Ui, id: u64, out: &mut Vec<PlayRequest>) {
        if self.break_context == 0 {
            self.send_request(ui, id, out);
            self.update_buttons(ui);
        }
    }

    /// The selection changed.
    // D2-02765
    pub fn selection_changed(&mut self, ui: &mut Ui, id: u64) {
        self.selection = id;
        self.update_buttons(ui);
    }

    /// Whether the page shows (its tab open and the window shown): the member shown asked for,
    /// else no more profiles.
    // D2-02766
    pub fn update_show_state(&self, ui: &mut Ui, out: &mut Vec<PlayRequest>) {
        let Some(tab) = ui.element(name::TAB) else {
            return;
        };
        if self.page.is_none() {
            return;
        }
        let open = ui.node(tab).is_some_and(|n| n.state == name::OPEN);
        let id = if open && ui.frame_shown(self.parent) {
            self.cur_target
        } else {
            0
        };
        self.send_request(ui, id, out);
        self.update_buttons(ui);
    }

    /// The question of an oath offered (unless one is up): the one offering.
    // D2-02767
    pub fn make_accept_swear_dialog(
        &mut self,
        ui: &mut Ui,
        who: &StringInfo,
        out: &mut Vec<PlayRequest>,
    ) {
        if self.accept_swear != 0 {
            return;
        }
        let mut s = entry(ui, "ID_Allegiance_SwearText");
        let words = write(ui, who);
        s.add_var(
            name::VALUE.raw(),
            Value::Text(Box::new(StringInfo::literal(&words))),
        );
        self.accept_swear = dialog::new_context();
        out.push(PlayRequest::Dialog(Dialog {
            id: self.accept_swear,
            kind: kind::CONFIRMATION,
            text: write(ui, &s),
            modal: false,
            info: None,
        }));
        self.update_buttons(ui);
    }

    /// The question of breaking with the member shown (unless one is up).
    // D2-02768
    pub fn make_break_dialog(&mut self, ui: &mut Ui, out: &mut Vec<PlayRequest>) {
        if self.break_context != 0 {
            return;
        }
        let mut s = entry(ui, "ID_Allegiance_BreakText");
        let words = write(ui, &self.cur_name);
        s.add_var(
            name::VALUE.raw(),
            Value::Text(Box::new(StringInfo::literal(&words))),
        );
        self.break_context = dialog::new_context();
        out.push(PlayRequest::Dialog(Dialog {
            id: self.break_context,
            kind: kind::CONFIRMATION,
            text: write(ui, &s),
            modal: false,
            info: None,
        }));
        self.update_buttons(ui);
    }

    /// The break answered: yes breaks with the member shown, then shows its patron (or the
    /// player, when the player was its vassal).
    // D2-02769
    pub fn do_break(&mut self, ui: &mut Ui, yes: bool, out: &mut Vec<PlayRequest>) {
        self.break_context = 0;
        self.update_buttons(ui);
        if !yes || self.player == 0 {
            return;
        }
        out.push(PlayRequest::Social(Request::Break(self.cur_target)));
        let next = if self.vassal_is_self {
            self.player
        } else {
            self.cur_patron
        };
        if self.break_context == 0 {
            self.send_request(ui, next, out);
            self.update_buttons(ui);
        }
    }

    /// A question closed: the oath offered taken (yes) or turned down; the break answered.
    // D2-02773
    pub fn close_confirmation_dialog(
        &mut self,
        ui: &mut Ui,
        id: u32,
        yes: bool,
        out: &mut Vec<PlayRequest>,
    ) {
        if self.accept_swear == id {
            self.accept_swear = 0;
            self.update_buttons(ui);
            if self.player != 0 {
                out.push(PlayRequest::Social(if yes {
                    Request::TakeOath
                } else {
                    Request::RefuseOath(DECLINED)
                }));
            }
        }
        if self.break_context == id {
            self.do_break(ui, yes, out);
        }
    }

    /// The oath sworn to the player was done: its question taken down.
    // D2-02775
    pub fn patron_swear_done(&mut self, ui: &mut Ui, out: &mut Vec<PlayRequest>) {
        if self.accept_swear != 0 {
            out.push(PlayRequest::DialogDown(self.accept_swear));
        }
        self.accept_swear = 0;
        self.update_buttons(ui);
    }

    /// A profile: the page made again for its member. With no member, no allegiance (the page
    /// says so). Else the allegiance's name, its size, its monarch (lit when the player); the
    /// member's row; each vassal's row (the last one's line ended), or that there are none; the
    /// message of the day, its row as tall as its text.
    // D2-02778
    pub fn update(&mut self, ui: &mut Ui, p: &Profile) {
        let Some(page) = self.page else { return };
        self.cur_patron = 0;
        self.cur_name = StringInfo::default();
        ui.set_state(page, state::STATELESS);
        self.officers.clear();
        if p.member.0.id == 0 {
            self.in_allegiance = false;
            self.patron = 0;
            ui.set_state(page, name::NO_ALLEGIANCE);
            self.update_buttons(ui);
            return;
        }
        self.officers.clone_from(&p.officers);
        ui.set_state(page, name::PENDING);
        self.in_allegiance = true;
        self.cur_target = p.member.0.id;
        self.cur_name = p.member.0.name.clone();
        ui.set_state(page, name::ALLEGIANCE);
        let monarch = real_monarch(p).cloned().unwrap_or_default();
        self.monarch = monarch.id;
        let part = |ui: &Ui, n| ui.child_recursive(page, n);
        let Some(t) = part(ui, name::ALLEGIANCE_NAME) else {
            return;
        };
        set_string_info(ui, t, &p.name);
        let mut s = entry(ui, "ID_Allegiance_Size");
        s.add_var(name::VALUE.raw(), Value::UInt(p.total));
        let Some(t) = part(ui, name::ALLEGIANCE_SIZE) else {
            return;
        };
        set_string_info(ui, t, &s);
        s.string_id = Token::of("ID_Allegiance_MonarchName").raw();
        s.add_var(
            name::VALUE.raw(),
            Value::Text(Box::new(monarch.name.clone())),
        );
        let Some(t) = part(ui, name::MONARCH_NAME) else {
            return;
        };
        let look = if monarch.id == self.player {
            name::SELF
        } else {
            state::STATELESS
        };
        ui.set_state(t, look);
        set_string_info(ui, t, &s);
        let Some(list) = part(ui, name::MEMBER_LIST) else {
            return;
        };
        let make = |ui: &mut Ui, n| ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, n);
        let Some(current) = make(ui, name::MEMBER_ELEMENT) else {
            return;
        };
        self.make_current(ui, &p.member.0, p.member.1, &p.patron.0, Some(current));
        ui.list_multi_add(list, current);
        self.vassal_is_self = false;
        if p.vassals.is_empty() {
            let Some(none) = make(ui, name::NO_VASSALS_ELEMENT) else {
                return;
            };
            ui.list_multi_add(list, none);
        } else {
            let mut last = None;
            for (v, online) in &p.vassals {
                last = make(ui, name::VASSAL_ELEMENT);
                if let Some(el) = last {
                    self.make_vassal(ui, v, *online, Some(el));
                    ui.list_multi_add(list, el);
                }
            }
            let Some(el) = last else { return };
            let Some(line) = ui.child_recursive(el, name::CONNECT_GRAPHIC) else {
                return;
            };
            ui.set_state(line, name::END);
        }
        let Some(motd) = make(ui, name::MOTD_ELEMENT) else {
            return;
        };
        let table = ui
            .assets()
            .data_id_by_enum(MOTD_TABLE, crate::flow::TABLE_MAPPING);
        let words = if p.motd == StringInfo::default() {
            StringInfo::entry(table, Token::of("ID_NoMotD").raw())
        } else {
            let mut m = StringInfo::entry(table, Token::of("ID_MotD").raw());
            m.add_var(
                Token::of("MESSAGE").raw(),
                Value::Text(Box::new(p.motd.clone())),
            );
            m
        };
        set_string_info(ui, motd, &words);
        ui.recalculate_glyph_list(motd);
        if let Some(r) = ui.node(motd).map(|n| n.rect) {
            let tall = ui.text_state(motd).map_or(0, |t| t.paper_h);
            ui.resize_to(motd, r.width(), tall);
        }
        ui.list_multi_add(list, motd);
        ui.list_changes_done(list);
        self.update_buttons(ui);
    }

    /// A profile came: the page made again; unless the page shows, no more profiles asked for.
    // D2-02779
    pub fn update_allegiance_profile(
        &mut self,
        ui: &mut Ui,
        p: &Profile,
        out: &mut Vec<PlayRequest>,
    ) {
        self.update(ui, p);
        let Some(tab) = ui.element(name::TAB) else {
            return;
        };
        let open = ui.node(tab).is_some_and(|n| n.state == name::OPEN);
        if open && ui.frame_shown(self.parent) {
            return;
        }
        self.send_request(ui, 0, out);
    }

    /// A notice of the player's scripts, acted on with the frame's game.
    // D2-02774
    fn act(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<PlayRequest>) {
        match n.native {
            notice::UPDATE_ALLEGIANCE_PROFILE => {
                if let Some(Some(Record::Allegiance(p))) = n.records.first() {
                    let p = p.as_ref().clone();
                    self.update_allegiance_profile(ui, &p, out);
                }
            }
            notice::REQUEST_VASSAL => {
                let who = match n.records.get(2) {
                    Some(Some(Record::Text(s))) => s.clone(),
                    _ => StringInfo::default(),
                };
                self.make_accept_swear_dialog(ui, &who, out);
            }
            notice::PATRON_SWEAR_DONE => self.patron_swear_done(ui, out),
            notice::SELECTION_CHANGED => self.selection_changed(ui, n.long(0)),
            notice::CLOSE_CONFIRMATION => {
                if let Some((id, yes)) = dialog::confirmation_closed(n) {
                    self.close_confirmation_dialog(ui, id, yes, out);
                }
            }
            _ => {}
        }
    }
}

impl AllegiancePage {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x436, 0x431, 0x432, 0x434, 0x435, 0x408, 0x1_040C];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for AllegiancePage {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// The window shown or hidden, the tab opened or closed (asked again as the page shows);
    /// Swear (the oath offered to the one selected), Break (asked first), Home (the player), Up
    /// (the member's patron), a vassal's Down (that vassal).
    // D2-02772
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<PlayRequest>) {
        let Mail::Element { element, from, .. } = *mail else {
            return;
        };
        if element == name::TAB || element == name::SOCIAL_FIELD {
            self.update_show_state(ui, out);
        } else if element == name::SWEAR_BUTTON {
            if self.player != 0 {
                out.push(PlayRequest::Social(Request::Swear));
            }
        } else if element == name::BREAK_BUTTON {
            self.make_break_dialog(ui, out);
        } else if element == name::HOME_BUTTON {
            self.focus_tree(ui, self.player, out);
        } else if element == name::UP_BUTTON {
            self.focus_tree(ui, self.cur_patron, out);
        } else if element == name::DOWN_BUTTON {
            let id = ui
                .node(from)
                .and_then(|n| n.instance_id_attr(name::IID))
                .unwrap_or(0);
            self.focus_tree(ui, id, out);
        }
    }

    /// Heard now, acted on with the frame's game.
    fn notice(&mut self, _ui: &mut Ui, n: &Notice, _out: &mut Vec<PlayRequest>) {
        self.pending.push(n.clone());
    }

    /// The player known (the first frame of the world); the notices heard acted on.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<PlayRequest>) {
        if self.player == 0 && self.breaks.is_some() {
            if let Some(w) = view.world.filter(|w| w.player != 0) {
                self.start(ui, w.player, out);
            }
        }
        for n in std::mem::take(&mut self.pending) {
            self.act(ui, &n, out);
        }
    }

    /// It goes: its questions taken down.
    // D2-02771, D2-02777
    fn close(self: Box<Self>, ui: &mut Ui, _out: &mut Vec<PlayRequest>) {
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
