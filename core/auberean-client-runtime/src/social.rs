//! The social window's requests of the player's scripts: the fellowship's (make one, break it up,
//! leave it, ask someone in, send one out, answer an offer), the allegiance's (ask for a profile,
//! swear, break, answer an oath offered), an emote, a tell started; and the fellows' shares of
//! the experience as the scripts answer them.

use auberean_client_contract::SocialAction;
use auberean_script::native_object::{native_type, Item, NativeObject, TextInfo};

use crate::host::{CallResult, Scripts};
use crate::{Client, InterfaceArg};

/// The interface's functions the social window calls.
pub mod function {
    /// Make a fellowship: its looting, its experience setting, its name (in that order).
    pub const DO_FELLOWSHIP_CREATE: u32 = 0x00B4_00A0;
    pub const DO_FELLOWSHIP_DISBAND: u32 = 0x00B4_00A1;
    pub const DO_FELLOWSHIP_LEAVE: u32 = 0x00B4_00A2;
    /// Ask an instance in, send one out.
    pub const DO_FELLOWSHIP_RECRUIT: u32 = 0x00B4_00A3;
    pub const DO_FELLOWSHIP_DISMISS: u32 = 0x00B4_00A4;
    /// Take the place offered; turn it down (a reason).
    pub const DO_FELLOWSHIP_RECRUIT_ACCEPT: u32 = 0x00B4_00A5;
    pub const DO_FELLOWSHIP_RECRUIT_DECLINE: u32 = 0x00B4_00A6;
    /// Each fellow's share of the experience, into the table a reference names.
    pub const GET_FELLOW_XP_HASH: u32 = 0x00B4_00A8;
    /// Start a tell to a name.
    pub const DO_ASSISTED_TELL: u32 = 0x00B4_00A9;
    /// The allegiance's profile about an instance (0: no longer).
    pub const QUERY_ALLEGIANCE_PROFILE: u32 = 0x00B4_00B3;
    pub const SWEAR_ALLEGIANCE: u32 = 0x00B4_00B4;
    /// Break with an instance.
    pub const BREAK_ALLEGIANCE: u32 = 0x00B4_00B5;
    /// Take the oath offered; turn it down (a reason).
    pub const ACCEPT_VASSAL: u32 = 0x00B4_00B6;
    pub const DECLINE_VASSAL: u32 = 0x00B4_00B7;
    /// Play an emote (a string reference).
    pub const DO_EMOTE: u32 = 0x00B4_012F;
}

/// The native type of the table of the fellows' shares: instance ids to the shares' bits.
const LONG_WORD_HASH: u32 = 0x32;

impl Client {
    /// What the social window asks, of the player's scripts.
    // D2-00597, D2-01089, D2-00598, D2-01090, D2-00599, D2-01092, D2-00600, D2-01093
    // D2-00601, D2-01091, D2-00602, D2-01094, D2-00603, D2-01095, D2-00608, D2-01200
    // D2-00609, D2-01278, D2-00610, D2-01072, D2-00611, D2-01070, D2-00612, D2-01083
    pub(crate) fn social(&mut self, a: SocialAction) {
        use function as f;
        use InterfaceArg::{Long, Text, Word};
        match a {
            SocialAction::CreateFellowship {
                name,
                social,
                looting,
            } => {
                let args = [Word(looting), Word(u32::from(social)), Text(name)];
                self.ask(f::DO_FELLOWSHIP_CREATE, &args);
            }
            SocialAction::DisbandFellowship => self.ask(f::DO_FELLOWSHIP_DISBAND, &[]),
            SocialAction::LeaveFellowship => self.ask(f::DO_FELLOWSHIP_LEAVE, &[]),
            SocialAction::Recruit(id) => self.ask(f::DO_FELLOWSHIP_RECRUIT, &[Long(id)]),
            SocialAction::Dismiss(id) => self.ask(f::DO_FELLOWSHIP_DISMISS, &[Long(id)]),
            SocialAction::AcceptRecruitment => self.ask(f::DO_FELLOWSHIP_RECRUIT_ACCEPT, &[]),
            SocialAction::DeclineRecruitment(why) => {
                self.ask(f::DO_FELLOWSHIP_RECRUIT_DECLINE, &[Word(why)]);
            }
            SocialAction::QueryAllegiance(id) => {
                self.ask(f::QUERY_ALLEGIANCE_PROFILE, &[Long(id)]);
            }
            SocialAction::Swear => self.ask(f::SWEAR_ALLEGIANCE, &[]),
            SocialAction::Break(id) => self.ask(f::BREAK_ALLEGIANCE, &[Long(id)]),
            SocialAction::TakeOath => self.ask(f::ACCEPT_VASSAL, &[]),
            SocialAction::RefuseOath(why) => self.ask(f::DECLINE_VASSAL, &[Word(why)]),
            // The friends page asks the friends service itself; its answers reach the player's
            // scripts, which tell the page.
            SocialAction::AddFriend(name) => {
                self.with_friends(|l| l.friend_add(&name));
            }
            SocialAction::RemoveFriend(name) => {
                self.with_friends(|l| l.friend_delete(&name));
            }
            SocialAction::GetFriends => {
                self.with_friends(|l| l.get_friends_list(1));
            }
            SocialAction::SetState { mode, location } => {
                self.with_friends(|l| l.set_location(mode, &location));
            }
        }
    }

    /// Play an emote: a new string reference to it handed to the scripts.
    // D2-00446, D2-01088
    pub(crate) fn emote(&mut self, string: u32, table: u32) {
        let info = TextInfo {
            string_id: string,
            table,
            ..TextInfo::default()
        };
        let arg = InterfaceArg::Native {
            ty: native_type::STRING_INFO,
            bytes: info.pack(),
        };
        self.ask(function::DO_EMOTE, &[arg]);
    }

    /// Start a tell to a name.
    // D2-00606, D2-01087
    pub(crate) fn assisted_tell(&mut self, name: &str) {
        let units = name.encode_utf16().collect();
        self.ask(function::DO_ASSISTED_TELL, &[InterfaceArg::Text(units)]);
    }

    /// Each fellow's share of the experience, as the player's scripts answer (empty when they
    /// answer none, or no scripts run).
    pub fn fellow_xp(&mut self) -> Vec<(u64, f32)> {
        self.with_scripts(Scripts::fellow_xp)
            .flatten()
            .unwrap_or_default()
    }
}

impl Scripts {
    /// Each fellow's share of the experience: the interface's function fills a new table of
    /// instance ids to the shares' bits; the client copies it into a table of its own (as many
    /// buckets, each entry put at its chain's head), which walks each bucket's chain the other
    /// way round. `None` when the call fails or fills none.
    // D2-00605, D2-01119, D2-01038, D2-01013
    pub fn fellow_xp(&mut self) -> Option<Vec<(u64, f32)>> {
        let (r, held) = self.call_out(function::GET_FELLOW_XP_HASH, LONG_WORD_HASH)?;
        if !matches!(r, CallResult::Completed) {
            return None;
        }
        let NativeObject::Hash(h) = held else {
            return None;
        };
        Some(
            h.buckets
                .iter()
                .flat_map(|b| b.iter().rev())
                .filter_map(|(k, v)| match (k, v) {
                    (Item::Long(id), Item::Word(bits)) => Some((*id, f32::from_bits(*bits))),
                    _ => None,
                })
                .collect(),
        )
    }
}
