//! Damage between players, and heals of it (Turbine, April 2004: "all PvP damage is about a third of
//! what it was").
//!
//! **A hit** of one player on another is reckoned against the target's armor at its full value (its
//! armor against an attack from where the attacker stands, [`crate::combat::armor_against`]), and a
//! third of what is left comes off, to the nearest point, at least one. **Harm and vigor taken by
//! another player's effects** come to a third of what they would, the same way. 2017 fits it: one
//! player's specials on another did 16 to 62 where the same skills did 661 to 948 on creatures,
//! about a third of what armor alone would leave and with a third of its spread.
//!
//! **Heals.** A player keeps how much of the health and the vigor it has lost another player took.
//! A heal (a skill's, a potion's, regeneration) restores the rest of what it lost first, at full
//! power, and then that part at a third: a third of what is left of the heal, rounded down, no
//! more than that part. Death clears it.

use yalain_net::ConnId;
use yalain_store::Store;

use crate::player::quality;
use crate::world::Player;
use crate::Server;

/// Damage between players is this many times less.
pub const DIVIDED_BY: f64 = 3.0;

/// The vital a quality is, among those a player keeps players' damage of.
fn index(cur: u32) -> Option<usize> {
    match cur {
        quality::HEALTH => Some(0),
        quality::VIGOR => Some(1),
        _ => None,
    }
}

/// A third of an amount, to the nearest point, its sign kept, at least one when there was any.
#[must_use]
pub fn third(amount: i32) -> i32 {
    if amount == 0 {
        return 0;
    }
    let t =
        dereth_primitives::num::to_i32_f64((f64::from(amount.abs()) / DIVIDED_BY).round()).max(1);
    t * amount.signum()
}

/// What a hit of `raw` (before armor) by one player takes off another: the target's armor against
/// the attacker off, at least one point left, a third of it; then the two players' kingdom perks
/// ([`crate::combat_perks::between_kingdoms`]).
#[must_use]
pub fn hit<S: Store>(s: &Server<S>, attacker: ConnId, target: ConnId, raw: i32) -> i32 {
    let from = s.players.get(&attacker).map(|p| p.position);
    hit_from(s, attacker, target, from.as_ref(), raw)
}

/// [`hit`], struck from a place of the attacker's (a pet's, its owner the attacker).
#[must_use]
pub fn hit_from<S: Store>(
    s: &Server<S>,
    attacker: ConnId,
    target: ConnId,
    from: Option<&yalain_store::StoredPosition>,
    raw: i32,
) -> i32 {
    if raw <= 0 {
        return raw;
    }
    let armor = crate::combat::armor_against(s, target, from);
    let left = dereth_primitives::num::to_i32((raw as f32 - armor).round()).max(1);
    let by = match (s.players.get(&attacker), s.players.get(&target)) {
        (Some(a), Some(t)) => crate::combat_perks::between_kingdoms(s, a, t),
        _ => 1.0,
    };
    dereth_primitives::num::to_i32_f64((f64::from(third(left)) * by).round()).max(1)
}

/// A player lost `lost` of a vital to another player.
pub fn harmed(p: &mut Player, cur: u32, lost: i32) {
    if let Some(i) = index(cur).filter(|_| lost > 0) {
        p.pk_lost[i] = p.pk_lost[i].saturating_add(lost);
    }
}

/// What a heal of `amount` restores of a vital that stands at `now` of `greatest`: the part other
/// players did not take first, in full, then a third of what is left of it on their part (that
/// part kept less by what it restores).
pub fn healed(p: &mut Player, cur: u32, (now, greatest): (i32, i32), amount: i32) -> i32 {
    let Some(i) = index(cur).filter(|_| amount > 0) else {
        return amount;
    };
    let lost = (greatest - now).max(0);
    let theirs = p.pk_lost[i].clamp(0, lost);
    let rest = lost - theirs;
    let full = amount.min(rest);
    let on_theirs =
        dereth_primitives::num::to_i32_f64((f64::from(amount - full) / DIVIDED_BY).floor())
            .min(theirs);
    p.pk_lost[i] = theirs - on_theirs;
    full + on_theirs
}

/// A player died: what other players took of it is forgotten.
pub fn died(p: &mut Player) {
    p.pk_lost = [0, 0];
}
