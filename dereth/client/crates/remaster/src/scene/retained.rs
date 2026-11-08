//! The retained scene: what the scene's changes build, held with no device.

use std::collections::BTreeMap;

use super::{Block, BlockId, BlockLevels, FrameDelta, Object, ObjectKey, Receiver, SceneDelta};

/// The scene as the remaster retains it: every block and object it was sent, the levels its
/// degrading placements draw at, and the last frame. A change that does not fit what it holds (a
/// block sent twice, an object removed it never had, levels of another length) is counted as a
/// fault and otherwise ignored.
#[derive(Debug, Default)]
pub struct Retained {
    // ORDER-OK: keyed lookups; counts sum over them in key order.
    blocks: BTreeMap<BlockId, Block>,
    // ORDER-OK: as above.
    objects: BTreeMap<ObjectKey, Object>,
    // ORDER-OK: as above.
    levels: BTreeMap<BlockId, BlockLevels>,
    frame: FrameDelta,
    frames: u64,
    faults: u64,
}

/// What a [`Retained`] scene holds, counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetainedCounts {
    pub blocks: usize,
    /// The blocks' own placements and their cells' statics.
    pub placements: usize,
    /// Their drawn parts.
    pub parts: usize,
    /// Their degrading parts.
    pub degrades: usize,
    pub cells: usize,
    pub portals: usize,
    /// The lights the blocks' cells' statics give off.
    pub static_lights: usize,
    pub objects: usize,
    pub object_parts: usize,
    /// The light pools' lights in the last frame.
    pub frame_lights: usize,
    pub frames: u64,
    pub faults: u64,
}

impl Retained {
    /// An empty scene.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one change.
    pub fn apply(&mut self, delta: SceneDelta) {
        match delta {
            SceneDelta::BlockIn(b) => {
                let id = b.block;
                if self.blocks.insert(id, *b).is_some() {
                    self.faults += 1;
                }
                self.levels.remove(&id);
            }
            SceneDelta::BlockChanged(b) => {
                let id = b.block;
                match self.blocks.get_mut(&id) {
                    Some(held) if held.generation < b.generation => *held = *b,
                    _ => {
                        self.faults += 1;
                        self.blocks.insert(id, *b);
                    }
                }
                self.levels.remove(&id);
            }
            SceneDelta::GroundChanged(g) => match self.blocks.get_mut(&g.block) {
                Some(held) if held.generation < g.generation => {
                    held.generation = g.generation;
                    held.terrain = g.terrain;
                }
                _ => self.faults += 1,
            },
            SceneDelta::BlockOut(id) => {
                if self.blocks.remove(&id).is_none() {
                    self.faults += 1;
                }
                self.levels.remove(&id);
            }
            SceneDelta::ObjectBuilt(o) => {
                if self.objects.insert(o.key, *o).is_some() {
                    self.faults += 1;
                }
            }
            SceneDelta::ObjectChanged(o) => {
                if self.objects.insert(o.key, *o).is_none() {
                    self.faults += 1;
                }
            }
            SceneDelta::ObjectRemoved(key) => {
                if self.objects.remove(&key).is_none() {
                    self.faults += 1;
                }
            }
        }
    }

    /// Take one frame's state, after its changes.
    pub fn take_frame(&mut self, frame: FrameDelta) {
        for l in &frame.levels {
            match self.blocks.get(&l.block) {
                Some(b) if b.degrades.len() == l.levels.len() => {
                    self.levels.insert(l.block, l.clone());
                }
                _ => self.faults += 1,
            }
        }
        // Every block it holds has its levels once it has been sent them, and every degrading
        // block is sent them in the frame it arrives.
        for (id, b) in &self.blocks {
            if !b.degrades.is_empty() && !self.levels.contains_key(id) {
                self.faults += 1;
            }
        }
        for p in &frame.objects {
            match self.objects.get(&p.key) {
                Some(o) if o.parts.len() == p.parts.len() => {}
                _ => self.faults += 1,
            }
        }
        if frame.objects.len() != self.objects.len() {
            self.faults += 1;
        }
        self.frames += 1;
        self.frame = frame;
    }

    /// The blocks held, by id.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.values()
    }

    /// The block `id`, when it is held.
    #[must_use]
    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.get(&id)
    }

    /// The objects held, by key.
    pub fn objects(&self) -> impl Iterator<Item = &Object> {
        self.objects.values()
    }

    /// The levels block `id`'s degrading placements last drew at.
    #[must_use]
    pub fn levels(&self, id: BlockId) -> Option<&BlockLevels> {
        self.levels.get(&id)
    }

    /// The last frame taken.
    #[must_use]
    pub fn frame(&self) -> &FrameDelta {
        &self.frame
    }

    /// What it holds, counted.
    #[must_use]
    pub fn counts(&self) -> RetainedCounts {
        let mut c = RetainedCounts {
            blocks: self.blocks.len(),
            objects: self.objects.len(),
            object_parts: self.objects.values().map(|o| o.parts.len()).sum(),
            frame_lights: self.frame.light.lights.len(),
            frames: self.frames,
            faults: self.faults,
            ..RetainedCounts::default()
        };
        for b in self.blocks.values() {
            let placements = b
                .placements
                .iter()
                .chain(b.rooms.iter().flat_map(|c| &c.placements));
            for p in placements {
                c.placements += 1;
                c.parts += p.parts.len();
            }
            c.degrades += b.degrades.len();
            c.cells += b.rooms.len();
            c.portals += b.rooms.iter().map(|c| c.portals.len()).sum::<usize>();
            c.static_lights += b.lights.len();
        }
        c
    }
}

impl Receiver for Retained {
    fn receive(&mut self, deltas: Vec<SceneDelta>, frame: FrameDelta) {
        for d in deltas {
            self.apply(d);
        }
        self.take_frame(frame);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::super::{
        block_id, Degrade, ObjectPart, ObjectPose, Part, PartPose, Placement, PlacementKind, Room,
        Terrain,
    };
    use super::*;
    use glam::{Mat4, Vec3};

    fn terrain() -> Terrain {
        Terrain {
            side: 1,
            vertices: vec![Vec3::ZERO; 4],
            cuts: vec![true],
            words: [0; 81],
        }
    }

    fn placement(parts: usize, degrade: Option<u32>) -> Placement {
        Placement {
            lifts: [0.0; 2],
            id: 0x0200_0001,
            kind: PlacementKind::Object,
            frame: Mat4::IDENTITY,
            scale: 1.0,
            parts: (0..parts)
                .map(|_| Part {
                    gfxobj: 0x0100_0001,
                    frame: Mat4::IDENTITY,
                    scale: Vec3::ONE,
                    degrade,
                })
                .collect(),
        }
    }

    fn block(id: BlockId, generation: u32) -> Box<Block> {
        Box::new(Block {
            block: id,
            generation,
            terrain: terrain(),
            placements: vec![placement(2, Some(0)), placement(1, None)],
            degrades: vec![Degrade {
                levels: vec![0x0100_0001, 0],
                ..Degrade::default()
            }],
            rooms: vec![Room {
                id: (u32::from(id) << 16) | 0x100,
                environment: 0x0D00_0001,
                structure: 0,
                surfaces: Vec::new(),
                frame: Mat4::IDENTITY,
                seen_outside: false,
                portals: Vec::new(),
                placements: vec![placement(3, None)],
            }],
            lights: Vec::new(),
            lamps: Vec::new(),
        })
    }

    fn levels(id: BlockId) -> BlockLevels {
        BlockLevels {
            block: id,
            levels: vec![1],
            turned: Vec::new(),
        }
    }

    #[test]
    fn blocks_in_out_and_rebuilt_leave_exactly_what_was_sent_last() {
        let mut r = Retained::new();
        let a = block_id(0xA9, 0xB4);
        let b = block_id(0xA9, 0xB5);
        r.receive(
            vec![
                SceneDelta::BlockIn(block(a, 1)),
                SceneDelta::BlockIn(block(b, 2)),
            ],
            FrameDelta {
                levels: vec![levels(a), levels(b)],
                ..FrameDelta::default()
            },
        );
        let c = r.counts();
        assert_eq!((c.blocks, c.placements, c.parts, c.cells), (2, 6, 12, 2));
        assert_eq!(c.faults, 0);
        let mut rebuilt = block(a, 3);
        rebuilt.placements.pop();
        r.receive(
            vec![SceneDelta::BlockChanged(rebuilt), SceneDelta::BlockOut(b)],
            FrameDelta {
                levels: vec![levels(a)],
                ..FrameDelta::default()
            },
        );
        let c = r.counts();
        assert_eq!((c.blocks, c.placements, c.parts), (1, 2, 5));
        assert_eq!(r.block(a).map(|b| b.generation), Some(3));
        assert_eq!(c.faults, 0);
    }

    #[test]
    fn a_change_that_does_not_fit_what_is_held_is_a_fault() {
        let mut r = Retained::new();
        let a = block_id(1, 2);
        r.apply(SceneDelta::BlockOut(a));
        assert_eq!(r.counts().faults, 1, "out before in");
        r.apply(SceneDelta::BlockIn(block(a, 1)));
        r.apply(SceneDelta::BlockIn(block(a, 1)));
        assert_eq!(r.counts().faults, 2, "in twice");
        r.apply(SceneDelta::BlockChanged(block(a, 1)));
        assert_eq!(r.counts().faults, 3, "rebuilt with no later generation");
        // Levels of another length, and a degrading block with none.
        r.take_frame(FrameDelta {
            levels: vec![BlockLevels {
                block: a,
                levels: vec![0, 0],
                turned: Vec::new(),
            }],
            ..FrameDelta::default()
        });
        assert_eq!(r.counts().faults, 5);
        r.apply(SceneDelta::ObjectRemoved(ObjectKey::Body));
        assert_eq!(r.counts().faults, 6, "an object it never had");
    }

    #[test]
    fn every_object_is_posed_every_frame_with_one_pose_per_part() {
        let mut r = Retained::new();
        let object = Object {
            key: ObjectKey::Server(0x5000_0001),
            setup: 0x0200_0001,
            parts: vec![
                ObjectPart {
                    levels: vec![0x0100_0001],
                    look: None,
                },
                ObjectPart {
                    levels: Vec::new(),
                    look: None,
                },
            ],
        };
        let pose = |parts: usize| ObjectPose {
            key: object.key,
            cell: 0,
            parts: vec![
                PartPose {
                    transform: Mat4::IDENTITY,
                    previous: Mat4::IDENTITY,
                    level: 0,
                    hidden: false,
                    material: None,
                };
                parts
            ],
        };
        r.receive(
            vec![SceneDelta::ObjectBuilt(Box::new(object.clone()))],
            FrameDelta {
                objects: vec![pose(2)],
                ..FrameDelta::default()
            },
        );
        assert_eq!(r.counts().faults, 0);
        assert_eq!(r.counts().object_parts, 2);
        r.receive(Vec::new(), FrameDelta::default());
        assert_eq!(r.counts().faults, 1, "a frame that left an object unposed");
        r.receive(
            Vec::new(),
            FrameDelta {
                objects: vec![pose(1)],
                ..FrameDelta::default()
            },
        );
        assert_eq!(r.counts().faults, 2, "a pose of another part count");
        r.receive(
            vec![SceneDelta::ObjectRemoved(object.key)],
            FrameDelta::default(),
        );
        assert_eq!(r.counts().faults, 2);
        assert_eq!(r.counts().objects, 0);
    }
}
