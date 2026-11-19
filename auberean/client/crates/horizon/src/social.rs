//! The player's fellowship, allegiance, friends, kingdom and the conflict the place allows, as
//! Horizon's party list, social window and conflict lamp show them: followed once a frame from the
//! view's social part, with the names written out, and from the notices (a tell to start, a friend
//! added).

use auberean_client_sdk::contract::notice::{self as n, ChatNotice, FriendsNotice, Said};
use auberean_client_sdk::contract::view::{self as v, friend_mode};

use crate::state::{Frame, GameState, Vital};

/// What Horizon shows of the player's social life.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Social {
    /// The player's fellowship; none while it is in none.
    pub party: Option<Party>,
    /// The allegiance's profile last told (none before one is).
    pub allegiance: Option<Allegiance>,
    /// The friends on the chat service, in the order the service gives them.
    pub friends: Vec<Friend>,
    /// The player's own state on the chat service, as last told.
    pub own_state: Option<u32>,
    /// The player's kingdom (none: in none), and the kind of conflict the place allows.
    pub kingdom: Option<u32>,
    pub conflict: Option<u32>,
    /// The scripts asked for a tell to be started this frame: to whom.
    pub start_tell: Option<String>,
    /// The chat service added a friend this frame.
    pub friend_added: bool,
    /// The player's body is moving.
    pub moving: bool,
}

/// A fellowship.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Party {
    pub name: String,
    pub leader: u64,
    /// The fellows share the experience they earn.
    pub shares_experience: bool,
    /// How loot is shared ([`LOOTING`]).
    pub looting: i32,
    /// Its fellows, in the fellowship's order.
    pub fellows: Vec<Fellow>,
}

impl Party {
    /// The fellow `id`, when it is one.
    #[must_use]
    pub fn fellow(&self, id: u64) -> Option<&Fellow> {
        self.fellows.iter().find(|f| f.id == id)
    }
}

/// A fellow.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fellow {
    pub id: u64,
    pub name: String,
    pub level: u32,
    pub health: Vital,
    pub vigor: Vital,
    /// The health and vigor other players took, shown past what is left.
    pub health_taken: u32,
    pub vigor_taken: u32,
    /// Its share of the experience, 0 to 1, when it has one.
    pub share: Option<f32>,
}

/// The ways a fellowship shares loot, by their numbers: the words for each, and what it means.
pub const LOOTING: [(&str, &str); 5] = [
    ("None", "Loot is not shared among fellows."),
    ("Shared", "Loot is completely shared among fellows."),
    ("Rotational", "Fellows take turns looting."),
    (
        "Group Rotational",
        "Fellows take turns looting for all monsters.",
    ),
    ("Random", "A fellow chosen at random loots."),
];

/// The way a new fellowship shares loot unless the player chooses another, as the game's own page
/// starts.
pub const DEFAULT_LOOTING: u32 = 1;

/// An allegiance's profile about one member.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Allegiance {
    pub name: String,
    /// How many it has.
    pub total: u32,
    /// Its monarch: its own, else the member's patron, else the member.
    pub monarch: Option<Rank>,
    /// The member the profile is about; none when the player is in no allegiance.
    pub member: Option<Rank>,
    pub patron: Option<Rank>,
    pub vassals: Vec<Rank>,
    /// Its message of the day (empty for none).
    pub message: String,
    pub officers: Vec<u64>,
}

impl Allegiance {
    /// Whether the player `player` is among the vassals shown.
    #[must_use]
    pub fn vassal(&self, player: u64) -> bool {
        self.vassals.iter().any(|r| r.id == player)
    }
}

/// A member of an allegiance as the profile names it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rank {
    pub id: u64,
    pub name: String,
    pub title: String,
    pub level: u32,
    pub online: bool,
    /// The experience it passed up.
    pub experience: u64,
}

/// A friend on the chat service.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Friend {
    pub name: String,
    /// Its mode: 0 offline, else its state's bits ([`friend_mode`]).
    pub mode: u32,
}

impl Friend {
    #[must_use]
    pub const fn online(&self) -> bool {
        self.mode != 0
    }

    /// What its state adds to its name in the list: away, busy, or nothing.
    #[must_use]
    pub const fn state_word(&self) -> Option<&'static str> {
        if self.mode & friend_mode::AWAY != 0 {
            Some("Away")
        } else if self.mode & friend_mode::BUSY != 0 {
            Some("Busy")
        } else {
            None
        }
    }
}

/// Whether an object is a player, by its id: the high byte of its high word marks its kind.
// D2-16948, D2-16947
#[must_use]
pub const fn is_player_id(id: u64) -> bool {
    let kind = (id >> 56) as u32;
    kind & 0x80 == 0 && kind & 0x40 == 0 && kind & 0x20 != 0
}

/// A reference written, or empty while its strings are not at hand.
fn written(f: &Frame<'_>, t: &auberean_client_sdk::contract::text::TextRef) -> String {
    (f.write)(t).unwrap_or_default()
}

fn rank(f: &Frame<'_>, (m, online): &(n::Member, bool)) -> Option<Rank> {
    (m.id != 0).then(|| Rank {
        id: m.id,
        name: written(f, &m.name),
        title: written(f, &m.title),
        level: m.level,
        online: *online,
        experience: m.experience_passed,
    })
}

fn party(f: &Frame<'_>, p: &n::Fellowship, shares: &[(u64, f32)]) -> Party {
    Party {
        name: p.name.clone(),
        leader: p.leader,
        shares_experience: p.social == 0,
        looting: p.looting,
        fellows: p
            .fellows
            .iter()
            .map(|(id, fellow)| Fellow {
                id: *id,
                name: written(f, &fellow.name),
                level: fellow.level,
                health: Vital {
                    current: fellow.health.into(),
                    max: fellow.max_health.into(),
                },
                vigor: Vital {
                    current: fellow.vigor.into(),
                    max: fellow.max_vigor.into(),
                },
                health_taken: fellow.health_taken_by_players,
                vigor_taken: fellow.vigor_taken_by_players,
                share: shares
                    .iter()
                    .find(|(i, _)| i == id)
                    .map(|(_, s)| *s)
                    .filter(|s| *s > 0.0),
            })
            .collect(),
    }
}

fn allegiance(f: &Frame<'_>, a: &n::Allegiance) -> Allegiance {
    let member = rank(f, &a.member);
    let monarch = rank(f, &a.monarch)
        .or_else(|| rank(f, &a.patron))
        .or_else(|| member.clone());
    Allegiance {
        name: written(f, &a.name),
        total: a.total,
        monarch,
        patron: rank(f, &a.patron),
        vassals: a.vassals.iter().filter_map(|m| rank(f, m)).collect(),
        message: written(f, &a.message),
        officers: a.officers.clone(),
        member,
    }
}

/// Follow one frame.
pub fn follow(state: &mut GameState, f: &Frame<'_>) {
    let s = &mut state.social;
    s.start_tell = None;
    s.friend_added = false;
    for notice in f.notices {
        match &notice.said {
            Said::Chat(ChatNotice::StartTell { to: Some(to) }) => {
                s.start_tell = (f.write)(to).filter(|n| !n.is_empty());
            }
            Said::Friends(FriendsNotice::Added { .. }) => s.friend_added = true,
            _ => {}
        }
    }
    let Some(view) = f.view else {
        *s = Social::default();
        return;
    };
    let social: &v::Social = &view.social;
    let shares = view
        .world
        .as_ref()
        .map(|w| w.fellow_xp.as_slice())
        .unwrap_or_default();
    s.party = social.fellowship.as_ref().map(|p| party(f, p, shares));
    s.allegiance = social.allegiance.as_ref().map(|a| allegiance(f, a));
    s.friends = social
        .friends
        .iter()
        .map(|fr| Friend {
            name: fr.name.clone(),
            mode: fr.mode,
        })
        .collect();
    s.own_state = social.own_state;
    s.kingdom = social.kingdom;
    s.conflict = social.conflict;
    s.moving = view.player.as_ref().is_some_and(|p| p.body.moving);
}
