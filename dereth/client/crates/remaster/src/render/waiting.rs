//! The scene's changes held back while the pictures they hold are made off the frame.
//!
//! A block or an object can bring dozens of pictures the remaster does not hold yet, and making
//! each (decoding it, and building its levels) can take milliseconds. Where the source makes
//! pictures ahead of their use ([`AssetSource::prepare`]), a change whose pictures are not ready
//! waits, and is taken on the frame they are: until then the remaster draws what it held before.
//! Each block's and object's changes are taken in the order they came, and one that goes takes
//! what of it waited with it.

use std::collections::{BTreeMap, BTreeSet};

use crate::assets::{AssetSource, AssetTables, TextureKey};
use crate::scene::{BlockId, BlockLevels, ObjectKey, SceneDelta};

/// What a change is to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Subject {
    Block(BlockId),
    Object(ObjectKey),
}

fn subject(d: &SceneDelta) -> Subject {
    match d {
        SceneDelta::BlockIn(b) | SceneDelta::BlockChanged(b) => Subject::Block(b.block),
        SceneDelta::GroundChanged(g) => Subject::Block(g.block),
        SceneDelta::BlockOut(id) => Subject::Block(*id),
        SceneDelta::ObjectBuilt(o) | SceneDelta::ObjectChanged(o) => Subject::Object(o.key),
        SceneDelta::ObjectRemoved(k) => Subject::Object(*k),
    }
}

/// The pictures a change still waits for, each with the clip map its first material names.
type Wants = Vec<(TextureKey, bool)>;

/// The changes waiting, in the order they came.
#[derive(Debug, Default)]
pub(crate) struct Waiting {
    /// Each change and the pictures it still waits for: `None` for one not yet looked at (it
    /// waits behind an earlier change to the same block or object).
    deltas: Vec<(SceneDelta, Option<Wants>)>,
    /// The newest levels of each block whose changes wait, handed on with them.
    levels: BTreeMap<BlockId, BlockLevels>,
}

impl Waiting {
    /// How many changes wait.
    pub(crate) fn len(&self) -> usize {
        self.deltas.len()
    }

    /// Of the changes waiting and `incoming`, those to take this frame, in order; the rest wait.
    /// `levels`, the frame's block levels, keeps those of the blocks taken, the waiting blocks'
    /// own handed on once they are.
    pub(crate) fn admit(
        &mut self,
        incoming: Vec<SceneDelta>,
        levels: &mut Vec<BlockLevels>,
        tables: &mut AssetTables,
        src: &mut dyn AssetSource,
    ) -> Vec<SceneDelta> {
        let queue: Vec<_> = std::mem::take(&mut self.deltas)
            .into_iter()
            .chain(incoming.into_iter().map(|d| (d, None)))
            .collect();
        let mut out = Vec::with_capacity(queue.len());
        let mut blocked: BTreeSet<Subject> = BTreeSet::new();
        for (d, wants) in queue {
            let who = subject(&d);
            if matches!(d, SceneDelta::BlockOut(_) | SceneDelta::ObjectRemoved(_)) {
                // Going: what of it waited is never taken.
                self.deltas.retain(|(w, _)| subject(w) != who);
                blocked.remove(&who);
                if let Subject::Block(b) = who {
                    self.levels.remove(&b);
                }
                out.push(d);
                continue;
            }
            if blocked.contains(&who) {
                self.deltas.push((d, wants));
                continue;
            }
            let wants = wants.unwrap_or_else(|| tables.pictures_wanted(&d, src));
            let missing: Wants = wants
                .into_iter()
                .filter(|(k, clip)| !tables.holds_texture(k) && !src.prepare(k, *clip))
                .collect();
            if missing.is_empty() {
                out.push(d);
            } else {
                blocked.insert(who);
                self.deltas.push((d, Some(missing)));
            }
        }
        // The levels of a waiting block wait with it; those of one now taken come with it.
        for l in std::mem::take(levels) {
            if blocked.contains(&Subject::Block(l.block)) {
                self.levels.insert(l.block, l);
            } else {
                self.levels.remove(&l.block);
                levels.push(l);
            }
        }
        let taken: Vec<BlockId> = self
            .levels
            .keys()
            .copied()
            .filter(|b| !blocked.contains(&Subject::Block(*b)))
            .collect();
        for b in taken {
            if let Some(l) = self.levels.remove(&b) {
                levels.push(l);
            }
        }
        if self.deltas.is_empty() {
            tables.forget_staged();
        }
        out
    }
}
