//! Bodies still dying when their objects leave the world, drawn on to the end of their deaths.
//!
//! **A ghost.** When an object drawn with a body leaves while the body still plays its death
//! (the server lets a creature go once its parts have died, and a body's own death may be
//! longer), the body is kept as a ghost: its object stays in the remaster's scene, standing as it
//! last stood, and its death plays on to its end. It is drawn and nothing more: the game has let
//! the object go, so nothing names it, selects it or bumps into it, and nothing it gave off goes
//! on. It lives at most the rest of its death, and never longer than [`GHOST_MOST`]; it ends at
//! once when the viewer is put somewhere else (a teleport), when its block leaves, when an object
//! is built again under its key, or when bodies are switched off.
//!
//! **Its corpse.** The server lays a corpse where a creature dies, as the creature goes. A corpse
//! built on the same setup and drawn with the same body, within [`CORPSE_REACH`] of a ghost, and
//! within [`CORPSE_WITHIN`] of the ghost's start, is hidden while the ghost lives, and shows the
//! moment the ghost ends, lying in its dead pose. One that comes a moment before its creature
//! leaves is hidden while that creature's body dies, for as long as [`CORPSE_WITHIN`] allows.
//! Nothing else is ever hidden, and a ghost hides one corpse at most.

use std::collections::BTreeMap;

use glam::{Mat4, Vec3};

use super::Skins;
use crate::scene::{BlockId, DrawnPart, FrameDelta, ObjectKey, ObjectPose, SceneDelta};

/// The longest a ghost lives, seconds, however much of its death is left.
pub const GHOST_MOST: f32 = 6.0;

/// How near a corpse must lie to a ghost to be its, meters.
pub const CORPSE_REACH: f32 = 3.0;

/// How long before or after a ghost starts its corpse may be built, seconds.
pub const CORPSE_WITHIN: f64 = 2.0;

/// A block's side, meters: how far the frame's space moves when its origin moves a block.
const BLOCK: f32 = 192.0;

/// The sphere a ghost's part indoors is lit as, meters: about a creature's.
const LIT_RADIUS: f32 = 1.5;

/// The ghosts, and what they are made from and hide.
#[derive(Debug, Default)]
pub(super) struct Ghosts {
    /// Each ghost, by the key its object had.
    living: BTreeMap<ObjectKey, Ghost>,
    /// Each body dying now, as the last frame posed it.
    dying: BTreeMap<ObjectKey, Kept>,
    /// The objects built with a body and not yet matched to a ghost, and the clock when each was.
    fresh: Vec<(ObjectKey, f64)>,
    /// Each corpse hidden, by its key.
    hidden: BTreeMap<ObjectKey, Hidden>,
    /// The clock of the frame whose changes are being taken.
    clock: f64,
}

impl Ghosts {
    /// An object was built with a body: a corpse, maybe.
    pub(super) fn built(&mut self, key: ObjectKey) {
        self.fresh.push((key, self.clock));
    }

    /// An object was removed: whatever was kept of it goes.
    pub(super) fn gone(&mut self, key: ObjectKey) {
        self.living.remove(&key);
        self.dying.remove(&key);
        self.fresh.retain(|(k, _)| *k != key);
        self.hidden.remove(&key);
    }
}

/// A body dying, as the last frame posed it: its object's pose and the parts the walk drew of it,
/// the frame's origin and teleports, its clock, and how many seconds of its death it had left.
#[derive(Debug, Clone)]
struct Kept {
    pose: ObjectPose,
    drawn: Vec<DrawnPart>,
    origin: [i32; 2],
    teleports: u64,
    clock: f64,
    left: f32,
}

/// One ghost: its object's last pose (its parts held where they stood) and the parts drawn of it,
/// the origin its pose is in, its block, when it started and ends, the teleports it was made at,
/// and the setup and body its corpse is told by.
#[derive(Debug, Clone)]
struct Ghost {
    pose: ObjectPose,
    drawn: Vec<DrawnPart>,
    origin: [i32; 2],
    block: BlockId,
    started: f64,
    ends: f64,
    teleports: u64,
    setup: u32,
    skin: u32,
}

/// A corpse hidden: the ghost (or the body still dying) it lies under, and when it was built.
#[derive(Debug, Clone, Copy)]
struct Hidden {
    by: ObjectKey,
    since: f64,
}

/// Where an object stands in a frame: its own place, else its first part's.
fn place(pose: &ObjectPose) -> Option<Vec3> {
    pose.motion
        .map(|m| m.frame.w_axis.truncate())
        .or_else(|| pose.parts.first().map(|p| p.transform.w_axis.truncate()))
}

impl Skins {
    /// `deltas` with each object that leaves while its body is still dying kept as a ghost (its
    /// removal held back), and the removal of each ghost `frame` ends put first. `on`: whether
    /// bodies are drawn; without them (or with [`Skins::ghosting`] off) every ghost ends and none
    /// is made.
    pub fn haunt(
        &mut self,
        deltas: Vec<SceneDelta>,
        frame: &FrameDelta,
        on: bool,
    ) -> Vec<SceneDelta> {
        let on = on && self.ghosting;
        self.ghosts.clock = frame.clock;
        let blocks_out: Vec<BlockId> = deltas
            .iter()
            .filter_map(|d| match d {
                SceneDelta::BlockOut(b) => Some(*b),
                _ => None,
            })
            .collect();
        let built_again = |key: ObjectKey| {
            deltas.iter().any(|d| {
                matches!(d, SceneDelta::ObjectBuilt(o) | SceneDelta::ObjectChanged(o) if o.key == key)
            })
        };
        let ended: Vec<ObjectKey> = self
            .ghosts
            .living
            .iter()
            .filter(|(key, g)| {
                !on || frame.clock >= g.ends
                    || frame.teleports != g.teleports
                    || blocks_out.contains(&g.block)
                    || built_again(**key)
            })
            .map(|(key, _)| *key)
            .collect();
        let mut out = Vec::with_capacity(deltas.len() + ended.len());
        for key in ended {
            self.ghosts.living.remove(&key);
            out.push(SceneDelta::ObjectRemoved(key));
        }
        for d in deltas {
            if let (true, SceneDelta::ObjectRemoved(key)) = (on, &d) {
                if let Some(ghost) = self.ghost_of(*key, frame, &blocks_out) {
                    self.ghosts.living.insert(*key, ghost);
                    continue;
                }
            }
            out.push(d);
        }
        out
    }

    /// The ghost of the object `key`, leaving in `frame` (whose changes take out `blocks_out`):
    /// `None` unless its body was dying in the last frame, with some of its death left, and the
    /// viewer has not been put somewhere else since.
    fn ghost_of(
        &mut self,
        key: ObjectKey,
        frame: &FrameDelta,
        blocks_out: &[BlockId],
    ) -> Option<Ghost> {
        let kept = self.ghosts.dying.remove(&key)?;
        let (model, _, setup) = self.objects.get(&key)?;
        #[allow(clippy::cast_possible_truncation)] // a cell's block, its top half
        let block = (kept.pose.cell >> 16) as BlockId;
        if kept.teleports != frame.teleports || blocks_out.contains(&block) {
            return None;
        }
        let ends = (kept.clock + f64::from(kept.left)).min(frame.clock + f64::from(GHOST_MOST));
        if ends <= frame.clock {
            return None;
        }
        let outdoors = kept.pose.cell & 0xFFFF < 0x100;
        let mut pose = kept.pose;
        // Held where it stood: its parts do not move from here on.
        for p in &mut pose.parts {
            p.previous = p.transform;
        }
        pose.held_by = None;
        let mut drawn = kept.drawn;
        if drawn.is_empty() {
            // The walk did not draw it last frame: every part, lit as where it stands is.
            drawn = (0..pose.parts.len())
                .filter_map(|k| u32::try_from(k).ok())
                .map(|part| DrawnPart {
                    key,
                    part,
                    outdoors,
                    lights: [0; 8],
                    count: 0,
                })
                .collect();
        }
        Some(Ghost {
            pose,
            drawn,
            origin: kept.origin,
            block,
            started: frame.clock,
            ends,
            teleports: frame.teleports,
            setup: *setup,
            skin: model.skin,
        })
    }

    /// `frame` with each ghost in it, standing where it stood, and each corpse a ghost lies over
    /// hidden: a corpse built since the last frame is matched to its ghost first.
    pub fn with_ghosts(&mut self, frame: &mut FrameDelta) {
        let clock = frame.clock;
        // The frame's space moved with its origin: each ghost stays where it stood in the world.
        for g in self.ghosts.living.values_mut() {
            if g.origin == frame.origin {
                continue;
            }
            #[allow(clippy::cast_precision_loss)] // a few blocks
            let shift = Mat4::from_translation(Vec3::new(
                (g.origin[0] - frame.origin[0]) as f32 * BLOCK,
                (g.origin[1] - frame.origin[1]) as f32 * BLOCK,
                0.0,
            ));
            for p in &mut g.pose.parts {
                p.transform = shift * p.transform;
                p.previous = p.transform;
            }
            if let Some(m) = g.pose.motion.as_mut() {
                m.frame = shift * m.frame;
            }
            g.origin = frame.origin;
        }
        self.match_corpses(frame);
        // A corpse shows once what it lay under is gone, or once its creature's body has been
        // dying too long for the corpse to be its.
        let (living, dying) = (&self.ghosts.living, &self.ghosts.dying);
        let objects = &self.objects;
        self.ghosts.hidden.retain(|corpse, h| {
            objects.contains_key(corpse)
                && (living.contains_key(&h.by)
                    || (dying.contains_key(&h.by) && clock - h.since <= CORPSE_WITHIN))
        });
        for g in self.ghosts.living.values() {
            frame.objects.push(g.pose.clone());
            for d in &g.drawn {
                let mut d = *d;
                if !d.outdoors {
                    let centre = g
                        .pose
                        .parts
                        .get(d.part as usize)
                        .map_or(Vec3::ZERO, |p| p.transform.w_axis.truncate());
                    let set = super::super::light::object_light_set(
                        &frame.light.lights,
                        frame.light.statics,
                        centre,
                        LIT_RADIUS,
                    );
                    d.lights = set.lights;
                    d.count = set.count;
                }
                frame.drawn_parts.push(d);
            }
        }
        for o in &mut frame.objects {
            if self.ghosts.hidden.contains_key(&o.key) {
                for p in &mut o.parts {
                    p.hidden = true;
                }
            }
        }
    }

    /// Each object built with a body since the last frame, matched to the nearest ghost (or body
    /// still dying) on its setup with its body, within reach, that hides nothing yet: hidden under
    /// it.
    fn match_corpses(&mut self, frame: &FrameDelta) {
        let clock = frame.clock;
        let poses: BTreeMap<ObjectKey, &ObjectPose> =
            frame.objects.iter().map(|p| (p.key, p)).collect();
        for (key, built) in std::mem::take(&mut self.ghosts.fresh) {
            let Some(at) = poses.get(&key).and_then(|p| place(p)) else {
                // Not posed yet: matched once it is, while it is still new enough.
                if clock - built <= CORPSE_WITHIN {
                    self.ghosts.fresh.push((key, built));
                }
                continue;
            };
            let Some((model, _, setup)) = self.objects.get(&key) else {
                continue;
            };
            let (skin, setup) = (model.skin, *setup);
            let hiding = |by: ObjectKey| self.ghosts.hidden.values().any(|h| h.by == by);
            let ghosts = self
                .ghosts
                .living
                .iter()
                .filter(|(_, g)| {
                    g.setup == setup && g.skin == skin && (g.started - built).abs() <= CORPSE_WITHIN
                })
                .filter_map(|(k, g)| Some((*k, place(&g.pose)?)));
            let dying = self
                .ghosts
                .dying
                .keys()
                .filter(|k| **k != key)
                .filter(|k| {
                    self.objects
                        .get(k)
                        .is_some_and(|(m, _, s)| m.skin == skin && *s == setup)
                })
                .filter_map(|k| Some((*k, place(poses.get(k)?)?)));
            let best = ghosts
                .chain(dying)
                .filter(|(k, _)| !hiding(*k))
                .map(|(k, p)| (p.distance(at), k))
                .filter(|(d, _)| *d <= CORPSE_REACH)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, by)) = best {
                self.ghosts.hidden.insert(key, Hidden { by, since: built });
            }
        }
    }

    /// Keep each body dying in `frame`, as it was posed, should its object leave.
    pub fn remember(&mut self, frame: &FrameDelta) {
        let mut dying = BTreeMap::new();
        for pose in &frame.objects {
            if !matches!(pose.key, ObjectKey::Server(_))
                || self.ghosts.living.contains_key(&pose.key)
            {
                continue;
            }
            let (Some((model, _, _)), Some(state)) =
                (self.objects.get(&pose.key), self.states.get(&pose.key))
            else {
                continue;
            };
            let Some(left) = model.death_left(state) else {
                continue;
            };
            dying.insert(
                pose.key,
                Kept {
                    pose: pose.clone(),
                    drawn: frame
                        .drawn_parts
                        .iter()
                        .filter(|d| d.key == pose.key)
                        .copied()
                        .collect(),
                    origin: frame.origin,
                    teleports: frame.teleports,
                    clock: frame.clock,
                    left,
                },
            );
        }
        self.ghosts.dying = dying;
    }

    /// The objects drawn as ghosts, and the corpses hidden under them: for an instrument to read.
    #[must_use]
    pub fn ghosts(&self) -> (Vec<ObjectKey>, Vec<ObjectKey>) {
        (
            self.ghosts.living.keys().copied().collect(),
            self.ghosts.hidden.keys().copied().collect(),
        )
    }
}

#[cfg(test)]
mod tests;
