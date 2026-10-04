//! Fellowships: the member table, the XP split and the even-split recomputation.
//!
//! The client recomputes the even-split flag so the panel can grey the state; **the server does the
//! real division**.

use dereth_assets::tables::XpTable;
use dereth_primitives::ObjectId;
use std::collections::BTreeMap;

/// The capacity test returns `count > 8`, i.e. the maximum fellowship is **9**.
pub const MAX_MEMBERS: usize = 9;

/// One fellowship member as decoded from the wire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fellow {
    pub name: String,
    pub level: u32,
    pub cp_cache: u32,
    pub lum_cache: u32,
    pub share_loot: i32,
    pub max_health: u32,
    pub max_stamina: u32,
    pub max_mana: u32,
    pub current_health: u32,
    pub current_stamina: u32,
    pub current_mana: u32,
}

/// A decoded fellowship and its member table.
#[derive(Debug, Clone, Default)]
pub struct Fellowship {
    pub members: BTreeMap<ObjectId, Fellow>,
    pub name: String,
    pub leader: ObjectId,
    pub share_xp: bool,
    pub even_xp_split: bool,
    pub open_fellow: bool,
    pub locked: bool,
    /// `_fellows_departed` — id → departure time, so a **locked** fellowship can re-admit exactly
    /// the people who were in it.
    pub fellows_departed: BTreeMap<ObjectId, i64>,
}

impl Fellowship {
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.members.len() > 8
    }

    /// Behavior: the member table contains `id`.
    ///
    /// Seven fellowship-panel call sites alone gate on it — the Recruit and
    /// Dismiss guards, the Recruit-button update, and the quit/dismiss paths'
    /// "was this person in it" test — and is the
    /// whole of the radar's fellow colour and blip shape.
    #[must_use]
    pub fn is_fellow(&self, id: ObjectId) -> bool {
        self.members.contains_key(&id)
    }

    /// The locked-fellowship remove: when `_locked` is set, a departing member is recorded
    /// with the real time, **replacing** any earlier entry.
    pub fn remove_fellow(&mut self, id: ObjectId, real_time: i64) -> Option<Fellow> {
        let f = self.members.remove(&id);
        if f.is_some() && self.locked {
            self.fellows_departed.insert(id, real_time);
        }
        f
    }

    /// The leader's level — `0xFFFFFFFF` when the leader is missing.
    #[must_use]
    pub fn leaders_level(&self) -> u32 {
        self.members.get(&self.leader).map_or(u32::MAX, |f| f.level)
    }

    /// Sum the fellowship members' experience proportions.
    #[must_use]
    pub fn experience_proportion_sum(&self, xp: &XpTable) -> u64 {
        if !self.share_xp {
            return 0;
        }
        self.members
            .values()
            .map(|m| get_experience_proportion(xp, m.level))
            .fold(0u64, u64::saturating_add)
    }

    /// Recalculate even experience splitting across fellowship members.
    ///
    /// An even split is possible when every member is level 50+, **or** the level spread around the
    /// leader is at most 5 in both directions.
    pub fn recalculate_even_xp_splitting(&mut self) {
        if !self.share_xp {
            return;
        }
        let leader_level = self.leaders_level();
        let mut min_level = 100_000u32;
        let mut max_level = 0u32;
        for m in self.members.values() {
            min_level = min_level.min(m.level);
            max_level = max_level.max(m.level);
        }
        self.even_xp_split = true;
        if min_level < 50 {
            if max_level > leader_level.saturating_add(5) {
                self.even_xp_split = false;
            }
            if leader_level > min_level.saturating_add(5) {
                self.even_xp_split = false;
            }
        }
    }
}

/// The fellowship experience proportion for a level — a member's weight is the XP
/// needed for their **next** level.
#[must_use]
pub fn get_experience_proportion(xp: &XpTable, level: u32) -> u64 {
    let l = usize::try_from(level).unwrap_or(0);
    crate::advancement::experience_to_raise_level(xp, l, l + 1)
}

/// The even-split percentage for a member count.
///
/// The table goes to **10** even though a fellowship holds at most 9, and any other count is 0.
/// The shares are **single precision**: the client's table holds `f32` constants and returns one,
/// so 0.45 is `0.449999988…` and nine members is the `f32` nearest the literal `0.3111111`
/// (`0x3E9F49F4`), which is one bit below the `f32` nearest 2.8 / 9. Callers that widen the share
/// see those values, which is what moves the fellowship panel's truncated percent.
#[must_use]
pub fn even_split_xp_percentage(member_count: usize) -> f32 {
    match member_count {
        1 => 1.0,
        2 => 0.75,
        3 => 0.6,
        4 => 0.55,
        5 => 0.5,
        6 => 0.45,
        7 => 0.4,
        8 => 0.35,
        9 => 0.311_111_1,
        10 => 0.28,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::DataId;

    fn xp_table() -> XpTable {
        XpTable {
            id: DataId(0x0E00_0018),
            attribute_xp: vec![0],
            vital_xp: vec![0],
            trained_xp: vec![0],
            specialized_xp: vec![0],
            // Level 0..4 thresholds.
            level_xp: vec![0, 1000, 3000, 7000, 20_000],
            level_credits: vec![0; 5],
        }
    }

    fn fellow(level: u32) -> Fellow {
        Fellow {
            level,
            ..Fellow::default()
        }
    }

    /// Oracle: the even-split table in `docs/CORRECTIONS.md`.
    #[test]
    fn the_even_split_table_matches_and_runs_to_ten() {
        let expected = [
            (1, 1.0),
            (2, 0.75),
            (3, 0.6),
            (4, 0.55),
            (5, 0.5),
            (6, 0.45),
            (7, 0.4),
            (8, 0.35),
            (9, 0.311_111_1),
            (10, 0.28),
        ];
        for (n, v) in expected {
            assert_eq!(
                even_split_xp_percentage(n).to_bits(),
                f32::to_bits(v),
                "{n} members"
            );
        }
        assert_eq!(even_split_xp_percentage(0), 0.0);
        assert_eq!(even_split_xp_percentage(11), 0.0);
        // Nine members: the literal's f32, not 2.8 / 9's.
        assert_eq!(even_split_xp_percentage(9).to_bits(), 0x3E9F_49F4);
    }

    /// The panel's percent is the `f32` share times 100, truncated, at double precision: six members
    /// show 44 and eight show 34, not 45 and 35.
    #[test]
    fn the_widened_single_precision_share_truncates_below_the_round_number() {
        #[allow(clippy::cast_possible_truncation)]
        let percent = |n| (f64::from(even_split_xp_percentage(n)) * 100.0) as i32;
        assert_eq!(
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10].map(percent),
            [100, 75, 60, 55, 50, 44, 40, 34, 31, 28]
        );
    }

    /// Oracle: §6's — `count > 8`, so nine members fit and ten do not.
    #[test]
    fn a_fellowship_holds_nine() {
        let mut f = Fellowship::default();
        for i in 0..9u32 {
            f.members.insert(ObjectId(i), fellow(10));
        }
        assert_eq!(f.members.len(), MAX_MEMBERS);
        assert!(
            f.is_full(),
            "`count > 8` already reads true at nine, which is what caps the fellowship there"
        );
        f.members.remove(&ObjectId(8));
        assert!(!f.is_full(), "eight members leaves room for a ninth");
    }

    /// Oracle: §6's, transcribed condition by condition.
    #[test]
    fn even_splitting_needs_level_fifty_or_a_tight_spread_around_the_leader() {
        let mut f = Fellowship {
            share_xp: true,
            leader: ObjectId(1),
            ..Default::default()
        };
        f.members.insert(ObjectId(1), fellow(60));
        f.members.insert(ObjectId(2), fellow(80));
        f.recalculate_even_xp_splitting();
        assert!(
            f.even_xp_split,
            "everyone at level 50+ is always an even split"
        );

        // A low-level member with a big spread above the leader.
        f.members.insert(ObjectId(3), fellow(10));
        f.recalculate_even_xp_splitting();
        assert!(!f.even_xp_split, "maxLevel 80 > leaderLevel 60 + 5");

        // A tight spread around the leader is fine even below 50.
        f.members.clear();
        f.members.insert(ObjectId(1), fellow(10));
        f.members.insert(ObjectId(2), fellow(14));
        f.members.insert(ObjectId(3), fellow(6));
        f.recalculate_even_xp_splitting();
        assert!(f.even_xp_split, "spread of 4 either side of the leader");

        // Leader too far above the lowest member.
        f.members.insert(ObjectId(3), fellow(4));
        f.recalculate_even_xp_splitting();
        assert!(!f.even_xp_split, "leaderLevel 10 > minLevel 4 + 5");

        // With sharing off the flag is not touched at all.
        f.share_xp = false;
        f.even_xp_split = true;
        f.members.insert(ObjectId(4), fellow(1));
        f.recalculate_even_xp_splitting();
        assert!(
            f.even_xp_split,
            "the whole function returns early when _share_xp is clear"
        );
    }

    /// Oracle: §6 — a member's weight is `experience_to_raise_level(level, level + 1)`.
    #[test]
    fn the_proportion_sum_weights_members_by_their_next_level_cost() {
        let t = xp_table();
        assert_eq!(get_experience_proportion(&t, 1), 3000 - 1000);
        assert_eq!(get_experience_proportion(&t, 2), 7000 - 3000);

        let mut f = Fellowship {
            share_xp: true,
            ..Default::default()
        };
        f.members.insert(ObjectId(1), fellow(1));
        f.members.insert(ObjectId(2), fellow(2));
        assert_eq!(f.experience_proportion_sum(&t), 2000 + 4000);
        f.share_xp = false;
        assert_eq!(f.experience_proportion_sum(&t), 0);
    }

    /// Oracle: §6's locked-fellowship remove.
    #[test]
    fn a_locked_fellowship_remembers_who_left() {
        let mut f = Fellowship::default();
        f.members.insert(ObjectId(1), fellow(10));
        f.members.insert(ObjectId(2), fellow(10));
        f.remove_fellow(ObjectId(1), 100);
        assert!(
            f.fellows_departed.is_empty(),
            "an unlocked fellowship forgets"
        );

        f.locked = true;
        f.remove_fellow(ObjectId(2), 200);
        assert_eq!(f.fellows_departed.get(&ObjectId(2)), Some(&200));
        // A second departure replaces the earlier entry.
        f.members.insert(ObjectId(2), fellow(10));
        f.remove_fellow(ObjectId(2), 300);
        assert_eq!(f.fellows_departed.get(&ObjectId(2)), Some(&300));
    }

    /// Oracle: §6 — the leader's level reads `0xFFFFFFFF` when the leader is missing, which is what
    /// makes the `maxLevel > leaderLevel + 5` test vacuously false.
    #[test]
    fn a_missing_leader_has_the_sentinel_level() {
        let mut f = Fellowship {
            share_xp: true,
            leader: ObjectId(99),
            ..Default::default()
        };
        f.members.insert(ObjectId(1), fellow(1));
        assert_eq!(f.leaders_level(), u32::MAX);
        f.recalculate_even_xp_splitting();
        assert!(
            !f.even_xp_split,
            "the sentinel makes the second test fire: leaderLevel > minLevel + 5"
        );
    }
}
