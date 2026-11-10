//! The story's scenes: what each plays (its pictures, music, voice and subtitles, each step's
//! length), which act it belongs to, which shard bestows it, which scenes every new character
//! holds from the start, and the quest steps that run only for a player whose act is whole.

use std::collections::BTreeMap;

use crate::quests::Step;

/// An act is whole with this many of its scenes held, its hidden ones aside.
pub const ACT_WHOLE: usize = 7;

/// One step of a scene.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Keyframe {
    /// How long it lasts, in seconds.
    pub seconds: f64,
    pub picture: u32,
    pub music: u32,
    pub voice: u32,
    /// Its subtitle (string id, table), if it has one.
    pub subtitle: Option<(u32, u32)>,
    /// Which of the tracks it defines (picture 0x1, music 0x2, voice 0x4, the subtitle 0x10).
    pub tracks: u32,
}

/// One scene of the story.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scene {
    pub id: u32,
    /// Its act, and its number in the act.
    pub act: u32,
    pub number: u32,
    /// Its name (string id, table).
    pub name: (u32, u32),
    pub hidden: bool,
    pub playable: bool,
    pub keyframes: Vec<Keyframe>,
}

/// The story's scenes, the shards that bestow them, the scenes a new character holds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StoryTable {
    pub scenes: BTreeMap<u32, Scene>,
    /// The scene each shard (by its object id) bestows.
    pub shards: BTreeMap<u64, u32>,
    /// The quest each shard that has one (by its object id) completes as the player comes out.
    pub shard_quests: BTreeMap<u64, u32>,
    /// The scenes a new character holds from its first entry, in the order it is told of them.
    pub at_start: Vec<u32>,
    /// The scene each door (by its cell and template) plays on the way to its destination.
    pub doors: BTreeMap<(u32, u32), u32>,
    /// Quest steps that run only for a player whose act is whole: the step and the act.
    pub step_acts: Vec<(Step, u32)>,
}

impl StoryTable {
    /// The scenes of an act, in their order.
    #[must_use]
    pub fn act(&self, act: u32) -> Vec<&Scene> {
        let mut v: Vec<&Scene> = self.scenes.values().filter(|s| s.act == act).collect();
        v.sort_by_key(|s| (s.number, s.id));
        v
    }

    /// Is an act whole for a player holding these scenes: [`ACT_WHOLE`] of its scenes held, its
    /// hidden ones aside.
    #[must_use]
    pub fn act_whole(&self, act: u32, held: &[u32]) -> bool {
        self.scenes
            .values()
            .filter(|s| s.act == act && !s.hidden && held.contains(&s.id))
            .count()
            >= ACT_WHOLE
    }

    /// Does a step run for a player holding these scenes: every act it is gated by is whole.
    #[must_use]
    pub fn step_for(&self, step: &Step, held: &[u32]) -> bool {
        self.step_acts
            .iter()
            .filter(|(st, _)| st == step)
            .all(|(_, act)| self.act_whole(*act, held))
    }
}
