//! `GetSubDataIDs` and the dependency closure.
//!
//! The dependency enumerator walks a landblock's whole asset closure
//! before it is drawn.
//!
//! The closure here is an **iterative work list**, not recursion:
//! a dungeon landblock can exhaust a small stack when this traversal is recursive.

use std::collections::BTreeSet;

use dereth_dat::{classify_cell_id, divine_type, DbType};
use dereth_primitives::{AssetSource, DataId};

use crate::error::AssetError;
use crate::{
    decode_any, Animation, CellLandblock, ClothingTable, DecodedAsset, EnvCell, GfxObj,
    GfxObjDegradeInfo, LandblockInfo, MotionTable, PaletteSet, ParticleEmitterInfo, PhysicsScript,
    PhysicsScriptTable, Region, Scene, Setup, Surface, SurfaceTexture,
};

/// Every DataID this asset references. Used to build the load closure.
pub trait SubDataIds {
    fn sub_data_ids(&self, out: &mut Vec<DataId>);
}

fn push_valid(out: &mut Vec<DataId>, id: DataId) {
    // INVALID_DID is rejected by every load path, so it never belongs in a closure.
    if id.raw() != 0 {
        out.push(id);
    }
}

impl SubDataIds for GfxObj {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for s in &self.surfaces {
            push_valid(out, *s);
        }
        if let Some(d) = self.did_degrade {
            push_valid(out, d);
        }
    }
}

impl SubDataIds for Setup {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for p in &self.parts {
            push_valid(out, *p);
        }
        for p in self.placement_frames.values() {
            for h in &p.hooks {
                h.sub_data_ids(out);
            }
        }
        for d in [
            self.default_anim_id,
            self.default_script_id,
            self.default_mtable_id,
            self.default_stable_id,
            self.default_phstable_id,
        ] {
            push_valid(out, d);
        }
    }
}

impl SubDataIds for Animation {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for f in &self.part_frames {
            for h in &f.hooks {
                h.sub_data_ids(out);
            }
        }
    }
}

impl SubDataIds for Surface {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        if let Some(t) = self.orig_texture_id {
            push_valid(out, t);
        }
        if let Some(p) = self.orig_palette_id {
            push_valid(out, p);
        }
    }
}

impl SubDataIds for SurfaceTexture {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for l in &self.source_levels {
            push_valid(out, *l);
        }
    }
}

impl SubDataIds for PaletteSet {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for p in &self.palette_ids {
            push_valid(out, *p);
        }
    }
}

impl SubDataIds for GfxObjDegradeInfo {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for d in &self.degrades {
            push_valid(out, d.gfxobj_id);
        }
    }
}

impl SubDataIds for MotionTable {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        let all = |m: &crate::motion::MotionData, out: &mut Vec<DataId>| {
            for a in &m.anims {
                push_valid(out, a.anim_id);
            }
        };
        for m in self.cycles.iter().chain(&self.modifiers) {
            all(m, out);
        }
        for v in self.links.values() {
            for m in v {
                all(m, out);
            }
        }
    }
}

impl SubDataIds for PhysicsScript {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for s in &self.script_data {
            s.hook.sub_data_ids(out);
        }
    }
}

impl SubDataIds for PhysicsScriptTable {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for v in self.script_table.values() {
            for e in v {
                push_valid(out, e.script_id);
            }
        }
    }
}

impl SubDataIds for ClothingTable {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for (setup, effects) in &self.clothing_bases {
            push_valid(out, *setup);
            for e in effects {
                push_valid(out, e.object_id);
                for t in &e.texture_effects {
                    push_valid(out, t.old_texture);
                    push_valid(out, t.new_texture);
                }
            }
        }
        for t in self.palette_templates.values() {
            push_valid(out, t.icon);
            for s in &t.subpalette_effects {
                push_valid(out, s.palette_set);
            }
        }
    }
}

impl SubDataIds for Scene {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for o in &self.objects {
            push_valid(out, o.obj_id);
        }
    }
}

impl SubDataIds for ParticleEmitterInfo {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        push_valid(out, self.gfxobj_id);
        push_valid(out, self.hw_gfxobj_id);
    }
}

impl SubDataIds for CellLandblock {
    /// The landblock's own information-record, and only when
    /// `lbi_exists` says there is one.
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        if self.lbi_exists != 0 {
            out.push(DataId((self.id.raw() & 0xFFFF_0000) | 0xFFFE));
        }
    }
}

impl SubDataIds for LandblockInfo {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        let base = self.id.raw() & 0xFFFF_0000;
        for o in &self.objects {
            push_valid(out, o.id);
        }
        for b in &self.buildings {
            push_valid(out, b.id);
        }
        // The block's interior cells, which the LBI counts but does not list.
        for i in 0..self.num_cells {
            out.push(DataId(base | (0x0100 + i)));
        }
    }
}

impl SubDataIds for EnvCell {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        for s in &self.surfaces {
            push_valid(out, *s);
        }
        push_valid(out, self.environment);
        for o in &self.static_objects {
            push_valid(out, o.id);
        }
    }
}

impl SubDataIds for Region {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        if let Some(s) = &self.sound_info {
            for d in s {
                push_valid(out, d.stb_id);
            }
        }
        if let Some(s) = &self.scene_info {
            for d in s {
                for sc in &d.scenes {
                    push_valid(out, *sc);
                }
            }
        }
        if let Some(tm) = &self.land_surf.tex_merge {
            for m in tm
                .corner_terrain_maps
                .iter()
                .chain(&tm.side_terrain_maps)
                .chain(&tm.road_maps)
            {
                push_valid(out, m.tex_gid);
            }
            for t in &tm.terrain_desc {
                push_valid(out, t.tex_gid);
                push_valid(out, t.detail_tex_gid);
            }
        }
    }
}

impl SubDataIds for DecodedAsset {
    fn sub_data_ids(&self, out: &mut Vec<DataId>) {
        match self {
            Self::GfxObj(v) => v.sub_data_ids(out),
            Self::Setup(v) => v.sub_data_ids(out),
            Self::Animation(v) => v.sub_data_ids(out),
            Self::PaletteSet(v) => v.sub_data_ids(out),
            Self::Surface(v) => v.sub_data_ids(out),
            Self::SurfaceTexture(v) => v.sub_data_ids(out),
            Self::MotionTable(v) => v.sub_data_ids(out),
            Self::DegradeInfo(v) => v.sub_data_ids(out),
            Self::ClothingTable(v) => v.sub_data_ids(out),
            Self::Scene(v) => v.sub_data_ids(out),
            Self::Region(v) => v.sub_data_ids(out),
            Self::ParticleEmitterInfo(v) => v.sub_data_ids(out),
            Self::PhysicsScript(v) => v.sub_data_ids(out),
            Self::PhysicsScriptTable(v) => v.sub_data_ids(out),
            Self::Landblock(v) => v.sub_data_ids(out),
            Self::LandblockInfo(v) => v.sub_data_ids(out),
            Self::EnvCell(v) => v.sub_data_ids(out),
            // Palette, RenderSurface, RenderTexture, Environment, Wave and SoundTable are leaves:
            // RenderTexture's source levels are 0x06 records it does not own, and SoundTable's
            // rows point at waves that the audio crate loads on demand, not through the closure.
            _ => {}
        }
    }
}

/// The type an id resolves to for closure purposes: the divined type, or the cell dat's
/// positional classification when the id is not in any type range.
#[must_use]
pub fn closure_type(id: DataId) -> Option<DbType> {
    divine_type(id)
        .filter(|t| !matches!(t, DbType::WeenieDef))
        .or_else(|| classify_cell_id(id))
}

/// The dependency closure as an iterative work list — **not** recursion.
///
/// Ids that do not exist in the store, or whose type nothing here decodes, are recorded as
/// visited but contribute no children; a missing sub-id is normal (`lbi_exists` is the only
/// existence flag the format carries, and the cell count is an upper bound).
pub fn closure(src: &dyn AssetSource, roots: &[DataId]) -> Result<Vec<DataId>, AssetError> {
    let mut seen: BTreeSet<DataId> = BTreeSet::new();
    let mut work: Vec<DataId> = roots.to_vec();
    let mut children: Vec<DataId> = Vec::new();
    while let Some(id) = work.pop() {
        if id.raw() == 0 || !seen.insert(id) {
            continue;
        }
        let Some(kind) = closure_type(id) else {
            continue;
        };
        if !src.exists(id) {
            continue;
        }
        let bytes = src.read(id).map_err(|_| AssetError::NotFound(id))?;
        let Ok(asset) = decode_any(kind, id, &bytes) else {
            continue;
        };
        children.clear();
        asset.sub_data_ids(&mut children);
        for c in &children {
            if !seen.contains(c) {
                work.push(*c);
            }
        }
    }
    Ok(seen.into_iter().collect())
}
