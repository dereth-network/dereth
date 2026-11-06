//! The social window (a toolbar window): the fellowship, the allegiance and the friends, a page
//! each behind its tab; and the fellowship's small panel beside the play screen.
//!
//! [`SocialWindow`] is the window: its button and key show and hide it, its close button hides it,
//! its title bar moves it (the windows' places kept when let go), and it comes to the front of the
//! toolbar's windows whenever it shows or its title bar is pressed. Its pages are panels of their
//! own under it: [`fellowship::FellowshipPage`], [`allegiance::AllegiancePage`],
//! [`friends::FriendsPage`]. The fellows' rows, on the fellowship page and on the small panel
//! ([`mini::MiniFellowship`]), are kept by [`fellows::Fellows`].
//!
//! What the pages show comes from the player's scripts' notices (the fellowship and its fellows,
//! the allegiance's profile, a recruitment or an oath offered) and from the chat service (the
//! friends and their states); what they ask is a [`Request`] the client carries out.

pub mod allegiance;
pub mod fellows;
pub mod fellowship;
pub mod friends;
pub mod mini;

use std::any::Any;

use auberean_text::StringInfo;
use auberean_ui::framework::FrameId;
use auberean_ui::token::msg;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::examination::{window_to_front, BRING_TO_FRONT, WINDOW_FIELDS};
use crate::flow;
use crate::gameplay::{self, Child, Notice, Request as PlayRequest, View};

/// The player's scripts' notices these panels hear, by the native that sends each (its package,
/// then its number), and the client's own notices among the panels.
pub mod notice {
    /// The fellowship (the record: the fellowship), or none any more.
    pub const UPDATE_FELLOWSHIP: u32 = 0x008A << 16 | 0x0001;
    pub const CLEAR_FELLOWSHIP: u32 = 0x008A << 16 | 0x0002;
    /// A fellow (an instance id, the record: the fellow), or one gone (an instance id).
    pub const UPDATE_FELLOW: u32 = 0x008A << 16 | 0x0003;
    pub const CLEAR_FELLOW: u32 = 0x008A << 16 | 0x0004;
    /// A fellow's level (an instance id, the level).
    pub const UPDATE_FELLOW_LEVEL: u32 = 0x008A << 16 | 0x0005;
    /// Someone offers the player a place in a fellowship (an instance id, then the leader's name
    /// and the fellowship's, the records: string references), or the offer ran out.
    pub const REQUEST_RECRUITMENT: u32 = 0x008A << 16 | 0x0006;
    pub const EXPIRE_RECRUITMENT: u32 = 0x008A << 16 | 0x0007;
    /// A fellow's health (an instance id, the health, the damage taken from players), its
    /// greatest; its vigor (likewise), its greatest.
    pub const UPDATE_FELLOW_HEALTH: u32 = 0x008A << 16 | 0x0008;
    pub const UPDATE_FELLOW_MAX_HEALTH: u32 = 0x008A << 16 | 0x0009;
    pub const UPDATE_FELLOW_VIGOR: u32 = 0x008A << 16 | 0x000A;
    pub const UPDATE_FELLOW_MAX_VIGOR: u32 = 0x008A << 16 | 0x000B;
    /// The fellowship's leader (an instance id).
    pub const UPDATE_LEADER: u32 = 0x008A << 16 | 0x000C;
    /// Where a fellow is (an instance id, its cell).
    pub const UPDATE_FELLOW_CELL: u32 = 0x008A << 16 | 0x000D;
    /// The allegiance's profile (the record: the profile).
    pub const UPDATE_ALLEGIANCE_PROFILE: u32 = 0x0084 << 16 | 0x0001;
    /// Someone offers to swear to the player (an instance id, the record: the name).
    pub const REQUEST_VASSAL: u32 = 0x0084 << 16 | 0x0002;
    /// An oath sworn to the player was done (an instance id, a word).
    pub const PATRON_SWEAR_DONE: u32 = 0x0084 << 16 | 0x0003;
    /// The chat service's answers: the friends (the record: the list), a friend added or taken
    /// off (the record: the name), a friend's state (the record: the state); and the player's own
    /// state (a word: its mode).
    pub const GET_FRIENDS_LIST: u32 = 0x1AD2 << 16 | 0x0001;
    pub const ADD_FRIEND: u32 = 0x1AD2 << 16 | 0x0002;
    pub const REMOVE_FRIEND: u32 = 0x1AD2 << 16 | 0x0003;
    pub const FRIEND_STATUS: u32 = 0x1AD2 << 16 | 0x0004;
    pub const LOCAL_STATUS: u32 = 0x1AD2 << 16 | 0x0005;
    /// The selection changed (an instance id).
    pub const SELECTION_CHANGED: u32 = 0x0085 << 16 | 0x0003;
    /// The client's own notice that the player's options changed.
    pub const OPTIONS_CHANGED: u32 = 0x46D;
    /// A question closed (the dialog's number, then 1 for yes).
    pub const CLOSE_CONFIRMATION: u32 = crate::dialog::notice::CLOSE_CONFIRMATION_DIALOG;
}

/// A fellow as the scripts describe it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fellow {
    pub name: StringInfo,
    pub level: u32,
    pub health: u32,
    pub max_health: u32,
    pub vigor: u32,
    pub max_vigor: u32,
    /// The health and vigor taken by players, shown beyond what is left.
    pub pk_damage: u32,
    pub pk_vigor_loss: u32,
    pub cell: u32,
}

/// A fellowship as the scripts describe it: its fellows (in the order they were added), its name,
/// its leader, whether experience is shared, how loot is shared.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fellowship {
    pub fellows: Vec<(u64, Fellow)>,
    pub name: String,
    pub leader: u64,
    pub social: i32,
    pub looting: i32,
}

/// A member of an allegiance as the scripts describe one.
// D2-00751
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Member {
    pub id: u64,
    pub name: StringInfo,
    pub title: StringInfo,
    pub xp_passed: u64,
    pub species: u32,
    pub level: u32,
    pub gender: u32,
    pub officer: bool,
    pub faction: u32,
    pub rank: u32,
}

/// An allegiance's profile as the scripts describe it, about one member: the monarch, the
/// member's patron, the member, each with whether they are online; the member's vassals (each
/// with whether online); the allegiance's size, its message of the day, its name, its faction, its
/// officers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub monarch: (Member, bool),
    pub patron: (Member, bool),
    pub member: (Member, bool),
    pub vassals: Vec<(Member, bool)>,
    pub total: u32,
    pub motd: StringInfo,
    pub name: StringInfo,
    pub faction: u32,
    pub officers: Vec<u64>,
}

/// A friend's state as the chat service reports it: the name, where, the mode (0 offline).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserStatus {
    pub name: String,
    pub location: String,
    pub mode: u32,
}

/// What the social panels ask of the client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Make a fellowship: its name, its experience setting (the experience button not lit),
    /// how loot is shared.
    CreateFellowship {
        name: String,
        social: bool,
        looting: u32,
    },
    /// Break the fellowship up (its leader), leave it.
    DisbandFellowship,
    LeaveFellowship,
    /// Ask someone into the fellowship, or send one out (the leader).
    Recruit(u64),
    Dismiss(u64),
    /// Take the place offered in a fellowship, or turn it down (with the reason's number).
    AcceptRecruitment,
    DeclineRecruitment(u32),
    /// Ask for the allegiance's profile about a member (0: no longer).
    QueryAllegiance(u64),
    /// Swear to the one selected; break with a patron or a vassal.
    Swear,
    Break(u64),
    /// Take the oath offered, or turn it down (with the reason's number).
    TakeOath,
    RefuseOath(u32),
    /// The chat service: a friend added or taken off, the friends asked for, the player's own
    /// state set (the mode, where).
    AddFriend(String),
    RemoveFriend(String),
    GetFriends,
    SetLocation {
        mode: u32,
        location: String,
    },
    /// Play an emote: its string (by token) in the emotes' string table.
    Emote {
        string: u32,
        table: u32,
    },
    /// Start a tell to someone (the name), as the chat line's tell command.
    AssistedTell(String),
}

/// The string table of the panels' own words, by enumeration value.
pub const MESSAGES: u32 = flow::MESSAGES;

/// A string reference of the panels' own words.
#[must_use]
pub fn entry(ui: &Ui, name: &str) -> StringInfo {
    let table = ui.assets().data_id_by_enum(MESSAGES, flow::TABLE_MAPPING);
    StringInfo::entry(table, Token::of(name).raw())
}

/// A string reference written as the client writes it.
#[must_use]
pub fn write(ui: &Ui, info: &StringInfo) -> String {
    ui.assets().write(info).unwrap_or_default()
}

/// A string reference written into a text element.
pub fn set_string_info(ui: &mut Ui, h: Handle, info: &StringInfo) {
    let text = write(ui, info);
    ui.set_text(h, &text);
}

/// The buckets a table of instance ids spreads them over (the client asks for 32; it takes the
/// next of its sizes).
const ID_BUCKETS: u64 = 47;

/// The order the client's table of instance ids walks its entries in, given in the order they
/// were added: by bucket (the id modulo the bucket count), each bucket's latest first.
#[must_use]
pub fn hash_order(ids: &[u64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..ids.len()).collect();
    order.sort_by_key(|&i| (ids[i] % ID_BUCKETS, std::cmp::Reverse(i)));
    order
}

mod name {
    use auberean_ui::Token;
    pub const FIELD: Token = Token::of("SocialField");
    pub const BUTTON: Token = Token::of("SocialButton");
    pub const CLOSE_BUTTON: Token = Token::of("SocialCloseButton");
    pub const HELP_BUTTON: Token = Token::of("SocialHelpButton");
    pub const PANEL_TOP: Token = Token::of("SocialPanelTop");
}

/// The social window's panel: the window, and its three pages under it.
#[derive(Debug)]
// D2-02740
pub struct SocialWindow {
    frame: FrameId,
    field: Option<Handle>,
    pub allegiance: Option<allegiance::AllegiancePage>,
    pub fellowship: Option<fellowship::FellowshipPage>,
    pub friends: Option<friends::FriendsPage>,
}

impl SocialWindow {
    /// The window under the toolbar's framework: its field found; its pages made (the
    /// allegiance, the fellowship, the friends); its buttons, its title bar and the windows'
    /// fields heard; hidden.
    // D2-02739
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.element(name::FIELD);
        let mut me = Self {
            frame,
            field,
            allegiance: None,
            fellowship: None,
            friends: None,
        };
        if field.is_none() {
            return me;
        }
        me.allegiance = Some(allegiance::AllegiancePage::new(ui, frame));
        me.fellowship = Some(fellowship::FellowshipPage::new(ui, frame));
        me.friends = Some(friends::FriendsPage::new(ui, frame));
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::CLOSE_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::HELP_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::PANEL_TOP, msg::MOUSE_PRESS);
            ui.listen_for_message(l, name::PANEL_TOP, msg::MOUSE_RELEASE);
            ui.listen_for_message(l, name::BUTTON, msg::BUTTON_RELEASE_ON);
            for f in WINDOW_FIELDS {
                ui.listen_for_message(l, Token::of(f), BRING_TO_FRONT);
            }
        }
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// The pages, each a panel of its own.
    fn pages(&mut self) -> impl Iterator<Item = &mut dyn Child> {
        let a = self.allegiance.as_mut().map(|p| p as &mut dyn Child);
        let f = self.fellowship.as_mut().map(|p| p as &mut dyn Child);
        let r = self.friends.as_mut().map(|p| p as &mut dyn Child);
        a.into_iter().chain(f).chain(r)
    }
}

impl SocialWindow {
    /// The notices its panels register for as they are made, in the order they are made.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::union([
            friends::FriendsPage::notices_of(),
            fellowship::FellowshipPage::notices_of(),
            allegiance::AllegiancePage::notices_of(),
        ])
    }
}

impl Child for SocialWindow {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: it comes to the front, its field shown as it is.
    // D2-02737
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<PlayRequest>) {
        ui.frame_show(self.frame, on);
        for p in self.pages() {
            gameplay::hear(p, ui, out);
        }
        let Some(f) = self.field else { return };
        ui.broadcast(f, BRING_TO_FRONT, 0);
        let shown = ui.frame_shown(self.frame);
        ui.set_visible(f, shown);
    }

    /// A window come to the front; the title bar pressed (to the front) and let go (the places
    /// kept); its button, close and help buttons.
    // D2-02738
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<PlayRequest>) {
        let Mail::Element {
            element,
            what,
            from,
            ..
        } = *mail
        else {
            return;
        };
        if what == BRING_TO_FRONT {
            window_to_front(ui, self.field, Some(from));
        }
        if element == name::PANEL_TOP {
            if what == msg::MOUSE_PRESS {
                if let Some(f) = self.field {
                    ui.broadcast(f, BRING_TO_FRONT, 0);
                }
            }
            if what == msg::MOUSE_RELEASE {
                gameplay::GamePlay::save_locations_changed(out);
            }
        }
        if element == name::BUTTON {
            let shown = ui.frame_shown(self.frame);
            self.show(ui, !shown, out);
        } else if element == name::CLOSE_BUTTON {
            self.show(ui, false, out);
        } else if element == name::HELP_BUTTON {
            out.push(PlayRequest::Help { open: true });
        }
    }

    /// The scripts' notices reach the pages.
    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<PlayRequest>) {
        for p in self.pages() {
            gameplay::hand(p, ui, n, out);
        }
    }

    /// Its pages' frame.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<PlayRequest>) {
        for p in self.pages() {
            gameplay::hear(p, ui, out);
            p.update(ui, view, out);
        }
    }

    fn close(mut self: Box<Self>, ui: &mut Ui) {
        if let Some(p) = self.friends.take() {
            Box::new(p).close(ui);
        }
        if let Some(p) = self.fellowship.take() {
            Box::new(p).close(ui);
        }
        if let Some(p) = self.allegiance.take() {
            Box::new(p).close(ui);
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
