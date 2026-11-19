//! The player's fellowship, allegiance, friends and the conflict the place allows, kept for every
//! front end ([`SocialRecords`]) from what the player's scripts tell the panels, as the game's own
//! social panels keep them:
//!
//! - **The fellowship.** Described whole, it replaces what was kept; gone, nothing is kept. A
//!   fellow described keeps its place (one new to it goes last); a fellow gone leaves; a fellow's
//!   health, vigor, their greatest, its level and its cell, and the fellowship's leader, are kept
//!   as each is told. What is told of a fellowship the player is in none of is let be.
//! - **The allegiance.** Each profile told replaces the last; the scripts tell one only while a
//!   member's is asked for.
//! - **The friends.** The service's list replaces the friends kept (a friend with no name left
//!   out); a friend taken off leaves (by name); a friend's state is kept for a friend listed, and
//!   one going online or offline goes last; the player's own state is kept as told.
//! - **The conflict** the place allows, as told about the player.
//!
//! Everything is let go when the player leaves the world. The player's kingdom is read from the
//! player's description at each refresh.

use auberean_client_contract::notice::{
    AllegianceNotice, Fellow, Fellowship, FellowshipNotice, FriendsNotice, Notice, PlayerNotice,
    Said,
};
use auberean_client_contract::view::Social;

/// What the scripts told of the player's fellowship, allegiance and friends, and the conflict the
/// place allows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SocialRecords {
    social: Social,
}

impl SocialRecords {
    /// What is kept, as the view carries it (the kingdom aside, which the refresh reads).
    #[must_use]
    pub const fn social(&self) -> &Social {
        &self.social
    }

    /// Everything let go.
    pub fn clear(&mut self) {
        self.social = Social::default();
    }

    /// A notice of the player's scripts heard (`player` the player's id, once named).
    // D2-02753, D2-02757, D2-03030, D2-03031, D2-03032, D2-03033, D2-03034, D2-03057
    // D2-03065, D2-03067, D2-03073, D2-02779, D2-02782, D2-02801, D2-02807, D2-02808
    // D2-02442
    pub fn hear(&mut self, n: &Notice, player: Option<u64>) {
        match &n.said {
            Said::Fellowship(f) => self.fellowship(f),
            Said::Allegiance(AllegianceNotice::Changed(Some(a))) => {
                self.social.allegiance = Some(a.as_ref().clone());
            }
            Said::Friends(f) => self.friends(f),
            Said::Player(PlayerNotice::Conflict { kind })
                if n.about.is_none() || n.about == player =>
            {
                self.social.conflict = Some(*kind);
            }
            _ => {}
        }
    }

    fn fellowship(&mut self, n: &FellowshipNotice) {
        match n {
            FellowshipNotice::Changed(Some(f)) => {
                self.social.fellowship = Some(f.as_ref().clone());
                return;
            }
            FellowshipNotice::Cleared => {
                self.social.fellowship = None;
                return;
            }
            _ => {}
        }
        let Some(f) = &mut self.social.fellowship else {
            return;
        };
        match n {
            FellowshipNotice::Fellow {
                fellow: id,
                record: Some(r),
            } => match fellow(f, *id) {
                Some(kept) => *kept = r.as_ref().clone(),
                None => f.fellows.push((*id, r.as_ref().clone())),
            },
            FellowshipNotice::FellowGone { fellow: id } => f.fellows.retain(|(i, _)| i != id),
            FellowshipNotice::Level { fellow: id, level } => {
                if let Some(k) = fellow(f, *id) {
                    k.level = *level;
                }
            }
            FellowshipNotice::Health {
                fellow: id,
                health,
                taken_by_players,
            } => {
                if let Some(k) = fellow(f, *id) {
                    k.health = *health;
                    k.health_taken_by_players = *taken_by_players;
                }
            }
            FellowshipNotice::MaxHealth { fellow: id, max } => {
                if let Some(k) = fellow(f, *id) {
                    k.max_health = *max;
                }
            }
            FellowshipNotice::Vigor {
                fellow: id,
                vigor,
                taken_by_players,
            } => {
                if let Some(k) = fellow(f, *id) {
                    k.vigor = *vigor;
                    k.vigor_taken_by_players = *taken_by_players;
                }
            }
            FellowshipNotice::MaxVigor { fellow: id, max } => {
                if let Some(k) = fellow(f, *id) {
                    k.max_vigor = *max;
                }
            }
            FellowshipNotice::Leader { leader } => f.leader = *leader,
            FellowshipNotice::Cell { fellow: id, cell } => {
                if let Some(k) = fellow(f, *id) {
                    k.cell = *cell;
                }
            }
            _ => {}
        }
    }

    fn friends(&mut self, n: &FriendsNotice) {
        let friends = &mut self.social.friends;
        match n {
            FriendsNotice::List(all) => {
                friends.clone_from(all);
                friends.retain(|f| !f.name.is_empty());
            }
            FriendsNotice::Removed { name: Some(name) } => {
                let name = name
                    .literal
                    .as_deref()
                    .map(String::from_utf16_lossy)
                    .unwrap_or_default();
                if let Some(at) = friends.iter().position(|f| f.name == name) {
                    friends.remove(at);
                }
            }
            FriendsNotice::State(Some(s)) => {
                let Some(at) = friends.iter().position(|f| f.name == s.name) else {
                    return;
                };
                let moved = (friends[at].mode == 0) != (s.mode == 0);
                if moved {
                    friends.remove(at);
                    friends.push(s.clone());
                } else {
                    friends[at].mode = s.mode;
                    friends[at].location.clone_from(&s.location);
                }
            }
            FriendsNotice::OwnState { mode } => self.social.own_state = Some(*mode),
            _ => {}
        }
    }
}

/// The fellow `id` of a fellowship, when it is one.
fn fellow(f: &mut Fellowship, id: u64) -> Option<&mut Fellow> {
    f.fellows
        .iter_mut()
        .find(|(i, _)| *i == id)
        .map(|(_, fellow)| fellow)
}
