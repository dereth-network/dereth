//! The allegiance seam: `dereth_client_model`'s hierarchy walked into `dereth_ui_screens`' roster.
//!
//! This is the join the client's allegiance panel makes: `dereth-ui-screens` may not depend on
//! `dereth-client-model`, so the walk has to
//! happen on this side of the seam, in the crate that can see both. It lives in its own file
//! rather than in `hud.rs` so that the whole of the conversion is reviewable in one place — the
//! `GameView` implementation is a single forwarding line.
//!
//! The walk joins the world's monarch and patron ids with its ordered vassal traversal. Each id is
//! then joined to the world's allegiance record and object state to produce the panel's header,
//! display name, and online fields. Those are the same three lookups the original panel performs
//! for each node.

use dereth_client_contract::view::{AllegianceEntry, AllegianceRoster};
use dereth_rules::quality::QualityRead;

/// Int property 30, the player's allegiance rank as the shard stores it.
const ALLEGIANCE_RANK: u32 = 30;

/// The allegiance panel's three walks plus the header, for one world.
///
/// A world with no allegiance produces [`AllegianceRoster::default`] — no monarch, no patron, no
/// vassals, `total == 0`. That is the state **every** capture in the corpus is in: all twelve
/// `0x0020 Allegiance_AllegianceUpdate` carry `total_members = 0`.
///
/// `player` is the local player description's qualities and `filter` the quality filter: the rank
/// line reads the player's allegiance-rank quality through the ordinary, enchanted int read.
#[must_use]
pub fn roster(
    w: &dereth_client_model::World,
    player: Option<&dereth_client_model::Qualities>,
    filter: Option<&dereth_assets::tables::QualityFilter>,
) -> AllegianceRoster {
    let ids = w.allegiance_roster_ids();
    let entry = |id| entry_for(w, id);
    AllegianceRoster {
        allegiance_name: w.allegiance.allegiance_name.clone(),
        // The two `AllegianceProfile` header dwords, not the total: they are what
        // the player-data and monarch-data updates (the latter minus 1) format into the two
        // "Followers" boxes.
        total_members: w.allegiance.total_members,
        total_vassals: w.allegiance.total_vassals,
        // The patron-data update and the patron-is-monarch arm at load
        // `_cp_tithed` from the player's own `AllegianceData`, not `_cp_cached` from either
        // displayed person. The wire/model already preserve it; this is the projection.
        own_cp_tithed: ids
            .subject
            .and_then(|id| w.allegiance.look_up(id))
            .map_or(0, |data| data.cp_tithed),
        subject: ids.subject.and_then(entry),
        // Seeded with 0 and left there when the player carries no rank; otherwise enchanted,
        // never with negatives allowed.
        player_rank_quality: player
            .and_then(|q| q.inq_int_enchanted(ALLEGIANCE_RANK, false, filter))
            .unwrap_or(0),
        monarch: ids.monarch.and_then(entry),
        patron: ids.patron.and_then(entry),
        vassals: ids.vassals.into_iter().filter_map(entry).collect(),
    }
}

/// Look up one allegiance node and project the identity, login state, rank, and cached contribution
/// fields the panel reads from it.
fn entry_for(
    w: &dereth_client_model::World,
    id: dereth_primitives::ObjectId,
) -> Option<AllegianceEntry> {
    let d = w.allegiance.look_up(id)?;
    Some(AllegianceEntry {
        id: d.id,
        // `full_name` prepends the rank title selected by rank, heritage group, and gender, and
        // falls back to the bare name when the rank is out of range.
        full_name: d.full_name(),
        logged_in: d.is_logged_in(),
        rank: d.rank,
        cp_cached: d.cp_cached,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::tables::QualityFilter;
    use dereth_client_model::qualities::{Qualities, StatKey, StatType, StatValue};
    use dereth_primitives::{DataId, ObjectId};
    use dereth_protocol::types::qualities::StatMod;
    use {dereth_rules::enchant::ench_type, dereth_rules::enchant::Enchantment};

    fn filter(ints: Vec<u32>) -> QualityFilter {
        let mut property_lists: [Vec<u32>; 8] = Default::default();
        property_lists[0] = ints;
        QualityFilter {
            id: DataId(0x0E01_0001),
            property_lists,
            attribute_lists: Default::default(),
        }
    }

    /// A player stored at rank 2 under one additive +1 on the rank.
    fn buffed_player() -> Qualities {
        let mut q = Qualities::new();
        q.set(
            StatKey::new(StatType::Int, ALLEGIANCE_RANK),
            StatValue::Int(2),
        );
        q.enchantments.add_list.push(Enchantment {
            id: 2787,
            spell_category: 1,
            power_level: 1,
            start_time: 0.0,
            duration: 3600.0,
            caster: ObjectId(1),
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: 0.0,
            smod: StatMod {
                kind: ench_type::INT | ench_type::SINGLE_STAT | ench_type::ADDITIVE,
                key: ALLEGIANCE_RANK,
                value: 1.0,
            },
            spell_set_id: None,
        });
        q
    }

    /// Behaviour: allegiance.roster.carries-the-players-rank-read-enchanted
    #[test]
    fn the_roster_carries_the_players_rank_read_enchanted_through_the_filter() {
        let w = dereth_client_model::World::new();
        let q = buffed_player();
        assert_eq!(
            roster(&w, Some(&q), Some(&filter(vec![30]))).player_rank_quality,
            3,
            "listed: the stored 2 plus the buff"
        );
        assert_eq!(
            roster(&w, Some(&q), Some(&filter(vec![28]))).player_rank_quality,
            2,
            "not listed: the stored rank"
        );
        assert_eq!(
            roster(&w, Some(&q), None).player_rank_quality,
            2,
            "no filter: the stored rank"
        );
        assert_eq!(
            roster(&w, Some(&Qualities::new()), Some(&filter(vec![30]))).player_rank_quality,
            0,
            "no rank carried: the seeded 0"
        );
        assert_eq!(
            roster(&w, None, Some(&filter(vec![30]))).player_rank_quality,
            0
        );
    }
}
