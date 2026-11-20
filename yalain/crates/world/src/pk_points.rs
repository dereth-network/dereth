//! Kingdom points won and lost in fights between players.
//!
//! **What counts.** Each hit or harm one player does another (the health it takes, told as PK
//! damage; a pet's is its owner's) is kept as that attacker's part in a fight with the player, for [`RECENT`] seconds after
//! its last harm (the library's own span of a player having taken part in a fight with players
//! lately).
//!
//! **A death at a player's hand** (or its pet's) moves points between the dead player and those who harmed it
//! lately, shared by the harm each did (Turbine, January 2003: points are shared "in the same way
//! experience is shared by players who aren't in a fellowship and both damage the same creature"; a
//! player who did no harm gets none). Each one's gain is its share of [`EXCHANGE`] times the chance
//! the rating scale gives the dead player of beating it: `share × 20 / (1 + 10^((mine − its) /
//! 400))`, to the nearest point. The dead player loses the sum of what the others gain, before
//! each is rounded, to the nearest point, never below nothing. So two players of equal points
//! trade ten; a player 640 or more points above the one it kills gains nothing (Turbine, January
//! 2003: "You only receive PK points from someone with fewer points than you within a certain
//! range"; a 2015 guide: nothing from a target 640 or more away). 2017's one recorded kill fits
//! it: the killer at 1,600 with 68% of the harm gained 11, a helper at 1,956 with 32% gained 3, both
//! as the rule gives for a dead player at 1,879 to 1,895 points; the five losses of exactly 10
//! recorded, from players at 500 and 490, fit an even trade. The harm kept ends with the death.
//!
//! The points a fight moves are kept with the character; the shrine grants the skills they reach
//! when the player next uses it ([`crate::kingdom_skills`]).

use std::collections::BTreeMap;

use auberean_protocol::ServerMessage;
use yalain_net::ConnId;
use yalain_store::Store;

use crate::kingdom_skills::{POINTS, POINTS_UNSET};
use crate::{quals, Server};

/// How long a player's harm to another counts toward a death, in seconds after its last.
pub const RECENT: f64 = 300.0;
/// The points a death moves between two players of equal points, all of it at stake.
pub const EXCHANGE: f64 = 20.0;
/// The difference in points at which one player is ten times as likely as the other to win.
pub const SCALE: f64 = 400.0;

/// One attacker's part in the harm a player took lately: how much, and when its last came (world
/// seconds).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Part {
    pub harm: i64,
    pub last: f64,
}

/// A player's points (its rating while unset).
#[must_use]
pub fn points_of(qualities: &[u8]) -> i32 {
    quals::int(qualities, POINTS).unwrap_or(POINTS_UNSET)
}

/// What one fighter gains from a death: its share of the exchange times the dead player's chance
/// of beating it, before rounding.
#[must_use]
pub fn gain(share: f64, mine: i32, its: i32) -> f64 {
    share * EXCHANGE
        / (1.0 + dereth_primitives::num::math::pow(10.0, f64::from(mine - its) / SCALE))
}

/// A story told of a player's harm by another (its PK damage set): each hook's health taken from
/// the player is kept as its attacker's part. Harm told of a player already dead counts for
/// nothing.
pub fn harmed<S: Store>(s: &mut Server<S>, m: &ServerMessage, player: u64, now: f64) {
    let ServerMessage::DoStory(e) = m else {
        return;
    };
    // A pet's harm is its owner's.
    let attacker = crate::pets::player_behind(s, e.action.attacker).unwrap_or(e.action.attacker);
    if attacker == 0 || attacker == player {
        return;
    }
    let harm: i64 = e
        .action
        .hooks
        .iter()
        .filter(|h| h.target == player && h.numbers[5] > 0)
        .map(|h| i64::from(h.numbers[5]))
        .sum();
    if harm == 0 {
        return;
    }
    let Some(p) = s
        .players
        .values_mut()
        .find(|p| p.character == player && p.dying.is_none() && !crate::combat::is_dead(p))
    else {
        return;
    };
    let part = p.harmed_by.entry(attacker).or_default();
    part.harm += harm;
    part.last = now;
}

/// A player has died (`now` world seconds): the harm kept ends; at another player's hand
/// (`by_player`), points move between it and those who harmed it lately first, by the share of the
/// harm each did.
pub fn died<S: Store>(s: &mut Server<S>, world: ConnId, by_player: bool, now: f64) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let parts = std::mem::take(&mut p.harmed_by);
    if !by_player {
        return;
    }
    let its = points_of(&p.qualities);
    let lately: BTreeMap<u64, i64> = parts
        .into_iter()
        .filter(|(_, part)| now - part.last <= RECENT && part.harm > 0)
        .map(|(who, part)| (who, part.harm))
        .collect();
    let total: i64 = lately.values().sum();
    if total == 0 {
        return;
    }
    let mut lost = 0.0;
    for (who, harm) in lately {
        let Some(c) = crate::social::world_of(s, who) else {
            continue;
        };
        let Some(q) = s.players.get_mut(&c) else {
            continue;
        };
        #[allow(clippy::cast_precision_loss)]
        let share = harm as f64 / total as f64;
        let mine = points_of(&q.qualities);
        let won = gain(share, mine, its);
        lost += won;
        let won = dereth_primitives::num::to_i32_f64(won.round());
        if won != 0 {
            quals::set_int(&mut q.qualities, POINTS, mine.saturating_add(won));
            crate::world::keep_player(s, c);
        }
    }
    let lost = dereth_primitives::num::to_i32_f64(lost.round());
    if lost != 0 {
        if let Some(p) = s.players.get_mut(&world) {
            quals::set_int(&mut p.qualities, POINTS, (its - lost).max(0));
        }
        crate::world::keep_player(s, world);
    }
}
