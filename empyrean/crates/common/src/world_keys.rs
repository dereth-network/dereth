//! Not ACE: the keys a world gives the placements of the snapshot its newest checkpoint takes,
//! carried in its `world.pack`.
//!
//! A world repository numbers its placements as its source does, except where a checkpoint layer
//! re-keys them (its `keys.toml`): a placement of the checkpoint's pinned dump that matches one the
//! world already has takes that one's guid, and a new one may take a guid of its own. The layers
//! above a checkpoint keep its keys. A shard saved under the dump's own numbering (a server that
//! ran on that dump's world database) names some placements by guids the world gives to others, or
//! to none; a shard saved under the world's numbering names them all as the world does.

use serde::{Deserialize, Serialize};

/// What a world's newest checkpoint up to its layer changes of the guids of the dump it pins: each
/// guid it moves, and where to. Guids it keeps are not listed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldKeys {
    /// The checkpoint layer whose keys these are.
    pub checkpoint: String,
    /// Each moved guid of the dump and the guid the world gives that placement, in ascending order
    /// of the first.
    pub moved: Vec<(u32, u32)>,
}

impl WorldKeys {
    /// The keys of checkpoint `checkpoint` that map each dump guid to the world's, `keys`: those
    /// that move it. `None` when none does, as the world then numbers every placement as its dump.
    #[must_use]
    pub fn of(checkpoint: &str, keys: impl IntoIterator<Item = (u32, u32)>) -> Option<Self> {
        let mut moved: Vec<(u32, u32)> = keys.into_iter().filter(|(f, t)| f != t).collect();
        moved.sort_unstable();
        moved.dedup();
        (!moved.is_empty()).then(|| Self {
            checkpoint: checkpoint.to_owned(),
            moved,
        })
    }

    /// Why a row the dump's numbering saved under `guid` would follow it elsewhere in this world:
    /// the keys move its placement, or give its guid to another. `None` when the world numbers that
    /// guid as the dump does.
    #[must_use]
    pub fn why(&self, guid: u32) -> Option<String> {
        if let Ok(i) = self.moved.binary_search_by_key(&guid, |&(f, _)| f) {
            return Some(format!(
                "{}'s keys.toml moves its placement to 0x{:08X}",
                self.checkpoint, self.moved[i].1
            ));
        }
        self.moved.iter().find(|&&(_, t)| t == guid).map(|&(f, _)| {
            format!(
                "{}'s keys.toml gives its guid to the placement that was 0x{f:08X}",
                self.checkpoint
            )
        })
    }

    /// A check that the keys read as written: in ascending order, each guid moved once, none kept.
    ///
    /// # Errors
    /// The first thing wrong.
    pub fn check(&self) -> Result<(), String> {
        if self.checkpoint.is_empty() {
            return Err("world keys without their checkpoint".into());
        }
        if let Some(&(f, _)) = self.moved.iter().find(|(f, t)| f == t) {
            return Err(format!("world keys list 0x{f:08X} as moved to itself"));
        }
        if let Some(w) = self.moved.windows(2).find(|w| w[0].0 >= w[1].0) {
            return Err(format!(
                "world keys out of order at 0x{:08X} and 0x{:08X}",
                w[0].0, w[1].0
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a world's re-keyed placement guids, as a pack carries them)
    use super::*;

    #[test]
    fn keys_list_only_the_guids_they_move_and_say_why_a_saved_row_would_follow_one() {
        assert_eq!(WorldKeys::of("c", [(1, 1), (2, 2)]), None);
        let k = WorldKeys::of("2005-02-infiltration", [(9, 9), (5, 7), (3, 4)]).expect("keys");
        assert_eq!(k.moved, [(3, 4), (5, 7)]);
        assert!(k.check().is_ok());
        assert_eq!(
            k.why(5).as_deref(),
            Some("2005-02-infiltration's keys.toml moves its placement to 0x00000007")
        );
        assert_eq!(
            k.why(4).as_deref(),
            Some("2005-02-infiltration's keys.toml gives its guid to the placement that was 0x00000003")
        );
        assert_eq!(k.why(9), None);
        let bad = WorldKeys {
            checkpoint: "c".into(),
            moved: vec![(5, 7), (3, 4)],
        };
        assert!(bad.check().unwrap_err().contains("out of order"));
    }
}
