//! Art packs as swaps of the tables' entries, made when an entry is made: a pack's model in
//! place of a placement's parts, a pack's pictures in place of retail pictures, and its ground in
//! place of a terrain type's tile. Nothing is swapped while a frame is drawn: an entry is the
//! pack's or retail's from when it is made until it goes.
//!
//! **The switches** ([`PackSettings`]) choose what of a pack is drawn: its models by kind (trees,
//! plants, rocks, the growth in the water), its pictures for retail surfaces (the buildings'
//! materials) and its ground. All are off by default, so a pack offered changes nothing until a
//! switch is turned on.
//!
//! **Names.** The pack's own entries are named past every retail id ([`PACK_MESH`],
//! [`PACK_SURFACE`], [`PACK_PICTURE`]), so they go through the tables, the device and the frame
//! as retail's entries do. The source ([`super::AssetSource`]) that has a pack makes them:
//! [`super::AssetSource::pack_model`] says which objects it has a model for, and its meshes,
//! materials and pictures answer for the pack's names.
//!
//! **A placement drawn as the pack's model** ([`swap_placements`]): its parts become one, whose
//! mesh is the model's nearest level, its other levels a degrade record of their own.
//!
//! **Cut-out pictures** ([`fill_cut_away`]): every texel a cut-out picture's cut leaves out takes
//! the color of the nearest texel it keeps, before its levels are made, so filtering brings no
//! other color (a magenta key, say) into its edge.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Which kind of thing a pack's model is, for the switches that turn its models on by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Tree,
    Plant,
    Rock,
    /// Growth in and by the water: lily pads, reeds.
    Water,
}

impl Category {
    /// Every kind, in order.
    pub const ALL: [Self; 4] = [Self::Tree, Self::Plant, Self::Rock, Self::Water];
}

use super::{AssetTables, MaterialKey, MeshKey, SurfaceRef, TextureKey};
use crate::scene::{Block, Degrade, Part, SceneDelta};

/// What of a pack is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PackSettings {
    /// The models of each kind.
    pub trees: bool,
    pub plants: bool,
    pub rocks: bool,
    /// The growth in and by the water: lily pads, reeds.
    pub water: bool,
    /// The pictures drawn in place of retail pictures on buildings, statics and rooms.
    pub materials: bool,
    /// The ground's pictures, behind retail's blend of the terrain types, varied over the land and
    /// grown over its muddy banks.
    pub ground: bool,
    /// The scree and rock its ground lays on steep ground, while its ground is drawn.
    pub slopes: bool,
}

impl PackSettings {
    /// Everything of a pack drawn.
    #[must_use]
    pub fn all() -> Self {
        Self {
            trees: true,
            plants: true,
            rocks: true,
            water: true,
            materials: true,
            ground: true,
            slopes: true,
        }
    }

    /// Whether any of a pack is drawn.
    #[must_use]
    pub fn any(&self) -> bool {
        *self != Self::default()
    }

    /// Whether the models of `category` are drawn.
    #[must_use]
    pub fn models(&self, category: Category) -> bool {
        match category {
            Category::Tree => self.trees,
            Category::Plant => self.plants,
            Category::Rock => self.rocks,
            Category::Water => self.water,
        }
    }
}

/// The first id of the pack's meshes: `PACK_MESH | model << 8 | level`, the model by its place in
/// the pack, the level counted from the nearest drawn into the view.
pub const PACK_MESH: u32 = 0xF100_0000;
/// The first id of the surfaces of the pack's models: `PACK_SURFACE | model << 12 | level << 8 |
/// part`, the part by its place in the level.
pub const PACK_SURFACE: u32 = 0xF200_0000;
/// The first id of the pack's pictures: `PACK_PICTURE | picture`, the picture by its place in the
/// pack.
pub const PACK_PICTURE: u32 = 0xF300_0000;

/// The id of model `model`'s level `level`.
#[must_use]
pub fn pack_mesh(model: u32, level: u8) -> u32 {
    PACK_MESH | (model & 0xFFFF) << 8 | u32::from(level)
}

/// The model and level a pack mesh id names; `None` for any other id.
#[must_use]
pub fn of_pack_mesh(id: u32) -> Option<(u32, u8)> {
    (id & 0xFF00_0000 == PACK_MESH).then_some(((id >> 8) & 0xFFFF, (id & 0xFF) as u8))
}

/// The id of model `model`'s level `level`'s part `part`'s surface.
#[must_use]
pub fn pack_surface(model: u32, level: u8, part: u8) -> u32 {
    PACK_SURFACE | (model & 0xFFF) << 12 | u32::from(level & 0xF) << 8 | u32::from(part)
}

/// The model, level and part a pack surface id names; `None` for any other id.
#[must_use]
pub fn of_pack_surface(id: u32) -> Option<(u32, u8, u8)> {
    (id & 0xFF00_0000 == PACK_SURFACE).then_some((
        (id >> 12) & 0xFFF,
        ((id >> 8) & 0xF) as u8,
        (id & 0xFF) as u8,
    ))
}

/// The id of the pack's picture `picture`.
#[must_use]
pub fn pack_picture(picture: u32) -> u32 {
    PACK_PICTURE | (picture & 0xFF_FFFF)
}

/// The picture a pack picture id names; `None` for any other id.
#[must_use]
pub fn of_pack_picture(id: u32) -> Option<u32> {
    (id & 0xFF00_0000 == PACK_PICTURE).then_some(id & 0xFF_FFFF)
}

/// One level of a pack's model drawn into the view: its mesh, and how far from the eye the pack
/// says it is drawn (infinite for the last).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackLevel {
    pub mesh: u32,
    pub distance: f32,
}

/// A pack's model for an object, as the tables take it: its kind and its levels, nearest first.
#[derive(Debug, Clone, PartialEq)]
pub struct PackModel {
    pub category: Category,
    pub levels: Vec<PackLevel>,
}

/// `block` with each placement the pack has a model for (`model` of the placement's object)
/// drawn as that model: its parts made one, at the placement's own frame and scale, whose mesh
/// is the model's nearest level and whose degrade record, added after the block's own, holds its
/// levels. Returns how many placements it swapped.
pub fn swap_placements(
    block: &mut Block,
    model: &mut dyn FnMut(u32) -> Option<PackModel>,
) -> usize {
    let mut swapped = 0;
    for p in &mut block.placements {
        let Some(m) = model(p.id) else {
            continue;
        };
        let Some(first) = m.levels.first() else {
            continue;
        };
        let degrade = (m.levels.len() > 1).then(|| {
            // Its distance is measured from where the model stands, over its scale; it has no
            // bands of retail's, so the levels' own distances choose them.
            block.degrades.push(Degrade {
                levels: m.levels.iter().map(|l| l.mesh).collect(),
                centre: p.frame.w_axis.truncate(),
                scale_z: p.scale,
                ..Degrade::default()
            });
            u32::try_from(block.degrades.len() - 1).unwrap_or(u32::MAX)
        });
        p.parts = vec![Part {
            gfxobj: first.mesh,
            frame: p.frame,
            scale: glam::Vec3::splat(p.scale),
            degrade,
        }];
        swapped += 1;
    }
    swapped
}

/// `deltas` with every block's placements the pack has models for swapped
/// ([`swap_placements`]).
pub fn swap_deltas(
    mut deltas: Vec<SceneDelta>,
    model: &mut dyn FnMut(u32) -> Option<PackModel>,
) -> Vec<SceneDelta> {
    for d in &mut deltas {
        if let SceneDelta::BlockIn(b) | SceneDelta::BlockChanged(b) = d {
            swap_placements(b, model);
        }
    }
    deltas
}

/// The swaps a renderer has made: how many placements, and every pack mesh they drew.
#[derive(Debug, Clone, Default)]
pub struct PackSwaps {
    /// What of a pack is drawn: only models of a kind switched on are swapped in.
    pub settings: PackSettings,
    placements: u64,
    meshes: BTreeSet<u32>,
    /// How far from the eye the pack says each of its levels is drawn, by the level's mesh.
    distances: BTreeMap<u32, f32>,
}

impl PackSwaps {
    /// `deltas` swapped as [`swap_deltas`] swaps them, for the models of a kind switched on,
    /// counted.
    pub fn swap(
        &mut self,
        deltas: Vec<SceneDelta>,
        model: &mut dyn FnMut(u32) -> Option<PackModel>,
    ) -> Vec<SceneDelta> {
        let (meshes, distances) = (&mut self.meshes, &mut self.distances);
        let on = self.settings;
        let mut counted = |id: u32| {
            let m = model(id).filter(|m| on.models(m.category));
            if let Some(m) = &m {
                for l in &m.levels {
                    meshes.insert(l.mesh);
                    distances.insert(l.mesh, l.distance);
                }
            }
            m
        };
        let mut deltas = deltas;
        for d in &mut deltas {
            if let SceneDelta::BlockIn(b) | SceneDelta::BlockChanged(b) = d {
                self.placements += swap_placements(b, &mut counted) as u64;
            }
        }
        deltas
    }

    /// How far from the eye the pack says the level whose mesh is `mesh` is drawn (infinite for
    /// a model's last); `None` for a mesh that is not a pack level swapped in.
    #[must_use]
    pub fn distance(&self, mesh: u32) -> Option<f32> {
        self.distances.get(&mesh).copied()
    }

    /// What of the pack `tables` hold, against what the swaps drew.
    #[must_use]
    pub fn counts(&self, tables: &AssetTables) -> PackCounts {
        let mut c = PackCounts {
            placements: self.placements,
            meshes: self.meshes.len(),
            ..PackCounts::default()
        };
        let mut surfaces = BTreeSet::new();
        for id in &self.meshes {
            let Some(mesh) = tables.mesh(MeshKey::GfxObj(*id)) else {
                continue;
            };
            c.meshes_held += 1;
            for s in &mesh.subsets {
                if let SurfaceRef::Surface(s) = s.surface {
                    surfaces.insert(s);
                }
            }
        }
        let mut pictures = BTreeSet::new();
        for s in &surfaces {
            c.surfaces += 1;
            let Some(m) = tables.material(&MaterialKey::plain(*s)) else {
                continue;
            };
            c.surfaces_held += 1;
            if let Some(t @ TextureKey::Dat { picture, .. }) = &m.texture {
                if of_pack_picture(*picture).is_some() {
                    pictures.insert(t.clone());
                }
            }
        }
        c.pictures = pictures.len();
        c.pictures_held = pictures
            .iter()
            .filter(|t| tables.texture(t).is_some())
            .count();
        c
    }
}

/// What of a pack the tables hold: the placements swapped for its models, the meshes they drew
/// and how many of those are held, the surfaces of the held meshes and how many are held, and the
/// pictures of the held surfaces and how many are held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PackCounts {
    pub placements: u64,
    pub meshes: usize,
    pub meshes_held: usize,
    pub surfaces: usize,
    pub surfaces_held: usize,
    pub pictures: usize,
    pub pictures_held: usize,
}

/// `rgba` (`width` by `height`, four bytes a texel), a cut-out picture whose cut keeps the
/// texels of alpha `cutoff` and over, with every texel the cut leaves out given the color of the
/// nearest texel it keeps, its alpha as it was. Whatever color a picture holds where it is cut
/// away is drawn at its edge once the texels are filtered and the levels averaged; so the cut-away
/// texels hold the edge's own color out to the picture's sides. A picture the cut keeps nothing of
/// is left as it is.
pub fn fill_cut_away(rgba: &mut [u8], width: u32, height: u32, cutoff: f32) {
    let (w, h) = (width as usize, height as usize);
    if rgba.len() < w * h * 4 {
        return;
    }
    let kept = |t: &[u8]| f32::from(t[3]) / 255.0 >= cutoff;
    let mut filled: Vec<bool> = rgba[..w * h * 4]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|t| kept(t))
        .collect();
    // Outward from every texel kept, a ring at a time: each texel reached takes the color of the
    // one it was reached from, the nearest kept texel's.
    let mut queue: VecDeque<usize> = (0..w * h).filter(|&i| filled[i]).collect();
    if queue.is_empty() {
        return;
    }
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        let neighbours = [
            (x > 0).then(|| i - 1),
            (x + 1 < w).then(|| i + 1),
            (y > 0).then(|| i - w),
            (y + 1 < h).then(|| i + w),
        ];
        for n in neighbours.into_iter().flatten() {
            if !filled[n] {
                filled[n] = true;
                rgba.copy_within(i * 4..i * 4 + 3, n * 4);
                queue.push_back(n);
            }
        }
    }
}

#[cfg(test)]
mod tests;
