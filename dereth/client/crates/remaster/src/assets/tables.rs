//! The tables: every entry the scene's blocks and objects use, who uses each, and what is still to
//! be taken to the device.

use std::collections::{BTreeMap, BTreeSet};

use super::terrain::Splat;
use super::{
    terrain, AssetKey, AssetSink, AssetSource, CellPieceKey, Material, MaterialKey, Mesh, MeshKey,
    SurfaceRef, TerrainSurface, Texture, TextureKey, Use,
};
use crate::scene::{Block, BlockId, Object, ObjectKey, Placement, SceneDelta};

/// What holds entries: a block, with its ground, placements and cells, or an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum User {
    Block(BlockId),
    Object(ObjectKey),
    /// The sky's objects, held all at once.
    Sky,
    /// One particle card's graphics object, held from the first frame that draws it.
    Particle(u32),
}

/// A material as one subset draws it: the material, and whether the subset is two-sided and
/// tiled. Retail resolves a surface once per such triple.
type Group = (MaterialKey, bool, bool);

/// What one user holds, each key once.
#[derive(Debug, Default)]
struct Holdings {
    meshes: BTreeSet<MeshKey>,
    materials: BTreeSet<MaterialKey>,
    textures: BTreeSet<TextureKey>,
    groups: BTreeSet<Group>,
    splats: BTreeSet<u32>,
    terrain: bool,
}

/// An entry and how many users hold it.
#[derive(Debug)]
struct Held<T> {
    value: T,
    users: u32,
}

/// What the tables hold, counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AssetCounts {
    /// GfxObj meshes with drawing polygons.
    pub meshes: usize,
    /// Environment pieces' meshes.
    pub cell_pieces: usize,
    /// Sky objects' meshes.
    pub sky_objects: usize,
    /// Mesh keys the dat had nothing for.
    pub meshes_missing: usize,
    /// Triangles over every mesh held.
    pub triangles: usize,
    pub materials: usize,
    /// Distinct materials as subsets draw them, with their two polygon flags: the surfaces
    /// retail resolves.
    pub surface_groups: usize,
    /// Pictures held, the one-texel colors and the ground's included.
    pub textures: usize,
    /// Of those, untextured surfaces' one-texel colors.
    pub solid_textures: usize,
    /// Of those, ground cells' pictures.
    pub terrain_textures: usize,
    /// Of those, the pictures the ground's splats blend.
    pub terrain_sources: usize,
    /// Ground cells' splats.
    pub splats: usize,
    /// Texture keys whose picture would not decode.
    pub textures_missing: usize,
    /// Blocks whose ground is held.
    pub terrain_blocks: usize,
    /// Blocks and objects holding entries.
    pub blocks: usize,
    pub objects: usize,
}

/// What [`AssetTables::upload`] has handed a sink, cumulative.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UploadCounts {
    pub textures: u64,
    pub materials: u64,
    pub meshes: u64,
    pub terrain: u64,
    pub released: u64,
    /// Texture levels' bytes and meshes' vertex and index bytes.
    pub bytes: u64,
}

/// The remaster's assets: every mesh, material, texture and ground surface the blocks and objects
/// it was handed use. See the [module](super).
#[derive(Debug, Default)]
pub struct AssetTables {
    meshes: BTreeMap<MeshKey, Held<Option<Mesh>>>,
    materials: BTreeMap<MaterialKey, Held<Material>>,
    textures: BTreeMap<TextureKey, Held<Option<Texture>>>,
    terrain: BTreeMap<BlockId, TerrainSurface>,
    /// Whether the ground is held as its splats' layers rather than as composite pictures.
    splat: bool,
    splats: BTreeMap<u32, Held<Option<Splat>>>,
    users: BTreeMap<User, Holdings>,
    /// Each interior cell's materials, by the slot its piece's subsets name.
    cell_materials: BTreeMap<u32, Vec<MaterialKey>>,
    /// Each object's materials: per part, per level, per subset of that level's mesh.
    object_materials: BTreeMap<ObjectKey, Vec<Vec<Vec<MaterialKey>>>>,
    /// Each particle card's materials, per subset of its graphics object.
    particle_materials: BTreeMap<u32, Vec<MaterialKey>>,
    /// Entries made since the last upload, in the order they were made.
    made: Vec<AssetKey>,
    /// Entries let go since the last upload.
    let_go: Vec<AssetKey>,
    /// Entries a sink has been handed and not yet told to release.
    uploaded: BTreeSet<AssetKey>,
    uploads: UploadCounts,
}

impl AssetTables {
    /// Empty tables.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Take one of the scene's changes: a block or object coming, going or built again.
    pub fn apply(&mut self, delta: &SceneDelta, src: &mut dyn AssetSource) {
        match delta {
            SceneDelta::BlockIn(b) | SceneDelta::BlockChanged(b) => self.block_in(b, src),
            SceneDelta::GroundChanged(g) => self.ground_changed(g.block, &g.terrain, src),
            SceneDelta::BlockOut(id) => self.release(User::Block(*id)),
            SceneDelta::ObjectBuilt(o) | SceneDelta::ObjectChanged(o) => self.object_in(o, src),
            SceneDelta::ObjectRemoved(key) => {
                self.release(User::Object(*key));
                self.object_materials.remove(key);
            }
        }
    }

    /// Hold everything one block uses: its ground, its placements' meshes at every degrade level,
    /// its cells' pieces and statics, and their materials and textures. A block held already is
    /// replaced.
    pub fn block_in(&mut self, block: &Block, src: &mut dyn AssetSource) {
        let user = User::Block(block.block);
        self.release(user);
        let mut h = Holdings::default();
        let normals = terrain::normals(&block.terrain);
        let cells = src.terrain_cells(block.block, &block.terrain);
        for c in &cells {
            self.hold_ground_cell(&mut h, &c.texture, src);
        }
        self.terrain
            .insert(block.block, TerrainSurface { normals, cells });
        self.made.push(AssetKey::Terrain(block.block));
        h.terrain = true;
        for p in &block.placements {
            self.hold_placement(&mut h, block, p, src);
        }
        for cell in &block.rooms {
            let key = MeshKey::CellPiece(CellPieceKey {
                environment: cell.environment,
                structure: cell.structure,
            });
            self.hold_mesh(&mut h, key, src);
            let materials: Vec<MaterialKey> = cell
                .surfaces
                .iter()
                .map(|&s| MaterialKey::plain(s))
                .collect();
            let subsets = self.subsets_of(key);
            for (surface, two_sided, tiled) in subsets {
                let m = match surface {
                    SurfaceRef::Slot(s) => materials
                        .get(usize::from(s))
                        .cloned()
                        .unwrap_or_else(|| MaterialKey::plain(0)),
                    SurfaceRef::Surface(s) => MaterialKey::plain(s),
                    SurfaceRef::Missing => MaterialKey::plain(0),
                };
                self.hold_material(&mut h, &m, Use::Cell, src);
                h.groups.insert((m, two_sided, tiled));
            }
            self.cell_materials.insert(cell.id, materials);
            for p in &cell.placements {
                self.hold_placement(&mut h, block, p, src);
            }
        }
        self.users.insert(user, h);
    }

    /// Replace a held block's ground, alone: the cells' pictures it now draws are held, the ones it
    /// no longer draws are let go, and everything else the block holds is kept.
    pub fn ground_changed(
        &mut self,
        block: BlockId,
        terrain: &crate::scene::Terrain,
        src: &mut dyn AssetSource,
    ) {
        let Some(mut h) = self.users.remove(&User::Block(block)) else {
            return;
        };
        let old_textures: Vec<TextureKey> = h
            .textures
            .iter()
            .filter(|t| matches!(t, TextureKey::Terrain(_) | TextureKey::TerrainSource { .. }))
            .cloned()
            .collect();
        for t in &old_textures {
            h.textures.remove(t);
        }
        let old_splats = std::mem::take(&mut h.splats);
        let cells = src.terrain_cells(block, terrain);
        for c in &cells {
            self.hold_ground_cell(&mut h, &c.texture, src);
        }
        for t in old_textures {
            if drop_one(&mut self.textures, &t) {
                self.let_go.push(AssetKey::Texture(t));
            }
        }
        for k in old_splats {
            if drop_one(&mut self.splats, &k) {
                self.let_go.push(AssetKey::Splat(k));
            }
        }
        let normals = terrain::normals(terrain);
        self.terrain
            .insert(block, TerrainSurface { normals, cells });
        h.terrain = true;
        self.users.insert(User::Block(block), h);
    }

    /// Hold a placement's parts' meshes, every degrade level of each, with their surfaces as
    /// the dat has them. A degrading part draws its record's levels and never its own GfxObj,
    /// which only finds the record: the two differ for some parts.
    fn hold_placement(
        &mut self,
        h: &mut Holdings,
        block: &Block,
        p: &Placement,
        src: &mut dyn AssetSource,
    ) {
        for part in &p.parts {
            let levels = part
                .degrade
                .and_then(|d| block.degrades.get(usize::try_from(d).ok()?));
            let ids = match levels {
                Some(d) => d.levels.clone(),
                None => vec![part.gfxobj],
            };
            for id in ids.into_iter().filter(|id| *id != 0) {
                self.hold_gfxobj(h, id, None, Use::Static, src);
            }
        }
    }

    /// Hold a GfxObj's mesh and the materials its subsets draw with under `look`; their keys, in
    /// subset order.
    fn hold_gfxobj(
        &mut self,
        h: &mut Holdings,
        id: u32,
        look: Option<&crate::scene::PartLook>,
        used: Use,
        src: &mut dyn AssetSource,
    ) -> Vec<MaterialKey> {
        let key = MeshKey::GfxObj(id);
        self.hold_mesh(h, key, src);
        let mut out = Vec::new();
        for (surface, two_sided, tiled) in self.subsets_of(key) {
            let m = match surface {
                SurfaceRef::Surface(s) => MaterialKey {
                    surface: s,
                    appearance: src.appearance(s, look),
                },
                SurfaceRef::Slot(_) | SurfaceRef::Missing => MaterialKey::plain(0),
            };
            self.hold_material(h, &m, used, src);
            h.groups.insert((m.clone(), two_sided, tiled));
            out.push(m);
        }
        out
    }

    /// Hold everything one object uses: each part's mesh at every level and its materials under
    /// the part's look. An object held already is replaced.
    pub fn object_in(&mut self, object: &Object, src: &mut dyn AssetSource) {
        let user = User::Object(object.key);
        self.release(user);
        let mut h = Holdings::default();
        let mut parts = Vec::with_capacity(object.parts.len());
        for part in &object.parts {
            let mut levels = Vec::with_capacity(part.levels.len());
            for &id in &part.levels {
                levels.push(if id == 0 {
                    Vec::new()
                } else {
                    self.hold_gfxobj(&mut h, id, part.look.as_ref(), Use::Object, src)
                });
            }
            parts.push(levels);
        }
        self.object_materials.insert(object.key, parts);
        self.users.insert(user, h);
    }

    /// Hold the particle cards `ids` names that are not held yet: each one's graphics object and
    /// its materials. They are kept as long as the tables are: a world draws a few hundred.
    pub fn particles_in(&mut self, ids: impl IntoIterator<Item = u32>, src: &mut dyn AssetSource) {
        for id in ids {
            if id == 0 || self.particle_materials.contains_key(&id) {
                continue;
            }
            let mut h = Holdings::default();
            let materials = self.hold_gfxobj(&mut h, id, None, Use::Object, src);
            self.particle_materials.insert(id, materials);
            self.users.insert(User::Particle(id), h);
        }
    }

    /// A held particle card's materials, per subset of its graphics object.
    #[must_use]
    pub fn particle_materials(&self, id: u32) -> Option<&[MaterialKey]> {
        self.particle_materials.get(&id).map(Vec::as_slice)
    }

    /// Hold every sky object the region's days can show, and their materials. Held already, it
    /// is held again from the source.
    pub fn sky_in(&mut self, src: &mut dyn AssetSource) {
        self.release(User::Sky);
        let mut h = Holdings::default();
        for id in src.sky_objects() {
            let key = MeshKey::Sky(id);
            self.hold_mesh(&mut h, key, src);
            for (surface, two_sided, tiled) in self.subsets_of(key) {
                let m = match surface {
                    SurfaceRef::Surface(s) => MaterialKey::plain(s),
                    SurfaceRef::Slot(_) | SurfaceRef::Missing => MaterialKey::plain(0),
                };
                self.hold_material(&mut h, &m, Use::Sky, src);
                h.groups.insert((m, two_sided, tiled));
            }
        }
        self.users.insert(User::Sky, h);
    }

    /// Each subset of a held mesh: its surface and its two flags.
    fn subsets_of(&self, key: MeshKey) -> Vec<(SurfaceRef, bool, bool)> {
        self.meshes
            .get(&key)
            .and_then(|h| h.value.as_ref())
            .map(|m| {
                m.subsets
                    .iter()
                    .map(|s| (s.surface, s.two_sided, s.tiled))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn hold_mesh(&mut self, h: &mut Holdings, key: MeshKey, src: &mut dyn AssetSource) {
        if !h.meshes.insert(key) {
            return;
        }
        if let Some(e) = self.meshes.get_mut(&key) {
            e.users += 1;
            return;
        }
        let value = match key {
            MeshKey::GfxObj(id) => src.gfxobj(id),
            MeshKey::CellPiece(piece) => src.cell_piece(piece),
            MeshKey::Sky(id) => src.sky(id),
        };
        if value.is_some() {
            self.made.push(AssetKey::Mesh(key));
        }
        self.meshes.insert(key, Held { value, users: 1 });
    }

    fn hold_material(
        &mut self,
        h: &mut Holdings,
        key: &MaterialKey,
        used: Use,
        src: &mut dyn AssetSource,
    ) {
        if !h.materials.insert(key.clone()) {
            return;
        }
        let (texture, clip_map, maps) = match self.materials.get_mut(key) {
            Some(e) => {
                e.users += 1;
                (
                    e.value.texture.clone(),
                    e.value.clip_map,
                    e.value.maps.clone(),
                )
            }
            None => {
                let value = src.material(key, used);
                let t = (value.texture.clone(), value.clip_map, value.maps.clone());
                self.made.push(AssetKey::Material(key.clone()));
                self.materials.insert(key.clone(), Held { value, users: 1 });
                t
            }
        };
        if let Some(t) = texture {
            self.hold_texture(h, &t, clip_map, src);
        }
        for t in maps
            .iter()
            .flat_map(|m| m.normal.iter().chain(m.orm.iter()))
        {
            self.hold_texture(h, t, false, src);
        }
    }

    /// Hold one ground cell's picture: its composite, or its splat and the splat's layers.
    fn hold_ground_cell(&mut self, h: &mut Holdings, key: &TextureKey, src: &mut dyn AssetSource) {
        match key {
            TextureKey::Terrain(k) if self.splat => self.hold_splat(h, terrain::splat_key(*k), src),
            _ => self.hold_texture(h, key, false, src),
        }
    }

    fn hold_splat(&mut self, h: &mut Holdings, key: u32, src: &mut dyn AssetSource) {
        if !h.splats.insert(key) {
            return;
        }
        let value = match self.splats.get_mut(&key) {
            Some(e) => {
                e.users += 1;
                e.value.clone()
            }
            None => {
                let value = src.splat(key);
                if value.is_some() {
                    self.made.push(AssetKey::Splat(key));
                }
                self.splats.insert(
                    key,
                    Held {
                        value: value.clone(),
                        users: 1,
                    },
                );
                value
            }
        };
        if let Some(s) = value {
            let maps = |m: &Option<terrain::GroundMaps>| {
                m.iter()
                    .flat_map(|m| m.normal.iter().chain(m.surface.iter()))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            let mut held: Vec<TextureKey> = s.base.iter().cloned().collect();
            held.extend(maps(&s.base_maps));
            held.extend(s.base_broad.iter().map(|b| b.picture.clone()));
            for o in &s.overlays {
                held.push(o.alpha.clone());
                held.extend(o.tile.iter().cloned());
                held.extend(maps(&o.maps));
                held.extend(o.broad.iter().map(|b| b.picture.clone()));
            }
            for l in &s.slopes {
                for t in [&l.scree, &l.rock].into_iter().flatten() {
                    held.push(t.tile.clone());
                    held.extend(maps(&t.maps));
                }
            }
            if let Some(b) = &s.bank {
                for t in [&b.grass, &b.land, &b.bed] {
                    held.push(t.tile.clone());
                    held.extend(maps(&t.maps));
                }
            }
            for t in &held {
                self.hold_texture(h, t, false, src);
            }
        }
    }

    /// Hold the ground as its splats' layers (`on`) or as composite pictures, from now on: every
    /// block held lets go of the one and takes the other. Their surfaces are unchanged; what a
    /// cell's picture is drawn from is what changes.
    pub fn set_splat(&mut self, on: bool, src: &mut dyn AssetSource) {
        if self.splat == on {
            return;
        }
        self.splat = on;
        let blocks: Vec<BlockId> = self.terrain.keys().copied().collect();
        for b in blocks {
            let Some(mut h) = self.users.remove(&User::Block(b)) else {
                continue;
            };
            let keys: Vec<TextureKey> = self.terrain[&b]
                .cells
                .iter()
                .map(|c| c.texture.clone())
                .collect();
            // Let go of the other form.
            for k in std::mem::take(&mut h.splats) {
                if drop_one(&mut self.splats, &k) {
                    self.let_go.push(AssetKey::Splat(k));
                }
            }
            let ground: Vec<TextureKey> = h
                .textures
                .iter()
                .filter(|t| matches!(t, TextureKey::Terrain(_) | TextureKey::TerrainSource { .. }))
                .cloned()
                .collect();
            for t in ground {
                h.textures.remove(&t);
                if drop_one(&mut self.textures, &t) {
                    self.let_go.push(AssetKey::Texture(t));
                }
            }
            for k in &keys {
                self.hold_ground_cell(&mut h, k, src);
            }
            self.users.insert(User::Block(b), h);
        }
    }

    /// Whether the ground is held as its splats' layers.
    #[must_use]
    pub fn splat(&self) -> bool {
        self.splat
    }

    /// A ground cells' splat, when it is held.
    #[must_use]
    pub fn splat_of(&self, key: u32) -> Option<&Splat> {
        self.splats.get(&key)?.value.as_ref()
    }

    fn hold_texture(
        &mut self,
        h: &mut Holdings,
        key: &TextureKey,
        clip_map: bool,
        src: &mut dyn AssetSource,
    ) {
        if !h.textures.insert(key.clone()) {
            return;
        }
        if let Some(e) = self.textures.get_mut(key) {
            e.users += 1;
            return;
        }
        let value = src.texture(key, clip_map);
        if value.is_some() {
            self.made.push(AssetKey::Texture(key.clone()));
        }
        self.textures.insert(key.clone(), Held { value, users: 1 });
    }

    /// Let go of everything `user` held; an entry no one holds any more goes.
    fn release(&mut self, user: User) {
        let Some(h) = self.users.remove(&user) else {
            return;
        };
        for k in h.meshes {
            if drop_one(&mut self.meshes, &k) {
                self.let_go.push(AssetKey::Mesh(k));
            }
        }
        for k in h.materials {
            if drop_one(&mut self.materials, &k) {
                self.let_go.push(AssetKey::Material(k));
            }
        }
        for k in h.textures {
            if drop_one(&mut self.textures, &k) {
                self.let_go.push(AssetKey::Texture(k));
            }
        }
        for k in h.splats {
            if drop_one(&mut self.splats, &k) {
                self.let_go.push(AssetKey::Splat(k));
            }
        }
        if let User::Block(b) = user {
            if h.terrain && self.terrain.remove(&b).is_some() {
                self.let_go.push(AssetKey::Terrain(b));
            }
            self.cell_materials
                .retain(|cell, _| (cell >> 16) != u32::from(b));
        }
    }

    /// Hand `sink` every entry made since the last upload that is still held (textures, then
    /// materials, then meshes, then the ground), then every entry it was handed before and that
    /// has gone since. An entry made and gone between two uploads is never handed over.
    pub fn upload(&mut self, sink: &mut dyn AssetSink) {
        let made = std::mem::take(&mut self.made);
        let rank = |k: &AssetKey| match k {
            AssetKey::Texture(_) => 0,
            AssetKey::Material(_) | AssetKey::Splat(_) => 1,
            AssetKey::Mesh(_) => 2,
            AssetKey::Terrain(_) => 3,
        };
        let mut order: Vec<&AssetKey> = made.iter().collect();
        order.sort_by_key(|k| rank(k));
        for k in order {
            if self.uploaded.contains(k) {
                // Made again after going: the sink still holds the old one until told below.
                continue;
            }
            let handed = match k {
                AssetKey::Texture(t) => match self.textures.get(t).and_then(|e| e.value.as_ref()) {
                    Some(tex) => {
                        sink.texture(t, tex);
                        self.uploads.textures += 1;
                        self.uploads.bytes +=
                            tex.levels.iter().map(|l| l.len() as u64).sum::<u64>();
                        true
                    }
                    None => false,
                },
                AssetKey::Material(m) => match self.materials.get(m) {
                    Some(e) => {
                        sink.material(m, &e.value);
                        self.uploads.materials += 1;
                        true
                    }
                    None => false,
                },
                AssetKey::Mesh(m) => match self.meshes.get(m).and_then(|e| e.value.as_ref()) {
                    Some(mesh) => {
                        sink.mesh(*m, mesh);
                        self.uploads.meshes += 1;
                        self.uploads.bytes += (std::mem::size_of_val(mesh.vertices.as_slice())
                            + std::mem::size_of_val(mesh.indices.as_slice()))
                            as u64;
                        true
                    }
                    None => false,
                },
                AssetKey::Terrain(b) => match self.terrain.get(b) {
                    Some(t) => {
                        sink.terrain(*b, t);
                        self.uploads.terrain += 1;
                        true
                    }
                    None => false,
                },
                AssetKey::Splat(k) => match self.splats.get(k).and_then(|e| e.value.as_ref()) {
                    Some(s) => {
                        sink.splat(*k, s);
                        true
                    }
                    None => false,
                },
            };
            if handed {
                self.uploaded.insert(k.clone());
            }
        }
        for k in std::mem::take(&mut self.let_go) {
            let still_held = match &k {
                AssetKey::Texture(t) => self.textures.contains_key(t),
                AssetKey::Material(m) => self.materials.contains_key(m),
                AssetKey::Mesh(m) => self.meshes.contains_key(m),
                AssetKey::Terrain(b) => self.terrain.contains_key(b),
                AssetKey::Splat(k) => self.splats.contains_key(k),
            };
            if !still_held && self.uploaded.remove(&k) {
                sink.release(&k);
                self.uploads.released += 1;
            }
        }
    }

    /// A mesh, when it is held and the dat had one.
    #[must_use]
    pub fn mesh(&self, key: MeshKey) -> Option<&Mesh> {
        self.meshes.get(&key)?.value.as_ref()
    }

    /// A material, when it is held.
    #[must_use]
    pub fn material(&self, key: &MaterialKey) -> Option<&Material> {
        self.materials.get(key).map(|e| &e.value)
    }

    /// A texture, when it is held and its picture decoded.
    #[must_use]
    pub fn texture(&self, key: &TextureKey) -> Option<&Texture> {
        self.textures.get(key)?.value.as_ref()
    }

    /// A block's ground surface, when it is held.
    #[must_use]
    pub fn terrain(&self, block: BlockId) -> Option<&TerrainSurface> {
        self.terrain.get(&block)
    }

    /// An interior cell's materials, by the slot its piece's subsets name.
    #[must_use]
    pub fn cell_materials(&self, cell: u32) -> Option<&[MaterialKey]> {
        self.cell_materials.get(&cell).map(Vec::as_slice)
    }

    /// An object part's materials at one level, one per subset of that level's mesh.
    #[must_use]
    pub fn object_materials(
        &self,
        key: ObjectKey,
        part: usize,
        level: usize,
    ) -> Option<&[MaterialKey]> {
        self.object_materials
            .get(&key)?
            .get(part)?
            .get(level)
            .map(Vec::as_slice)
    }

    /// Every material held, in key order.
    pub fn materials(&self) -> impl Iterator<Item = (&MaterialKey, &Material)> {
        self.materials.iter().map(|(k, e)| (k, &e.value))
    }

    /// Every texture key held, in key order, with whether its picture decoded.
    pub fn texture_keys(&self) -> impl Iterator<Item = (&TextureKey, bool)> {
        self.textures.iter().map(|(k, e)| (k, e.value.is_some()))
    }

    /// Every mesh key held, in key order, with whether the dat had a mesh for it.
    pub fn mesh_keys(&self) -> impl Iterator<Item = (MeshKey, bool)> + '_ {
        self.meshes.iter().map(|(k, e)| (*k, e.value.is_some()))
    }

    /// Every material as subsets draw it, with their two polygon flags (two-sided, tiled): the
    /// surfaces retail resolves, once each.
    #[must_use]
    pub fn surface_groups(&self) -> BTreeSet<(MaterialKey, bool, bool)> {
        self.users
            .values()
            .flat_map(|h| h.groups.iter().cloned())
            .collect()
    }

    /// What the tables hold.
    #[must_use]
    pub fn counts(&self) -> AssetCounts {
        let mut c = AssetCounts::default();
        for (k, e) in &self.meshes {
            match (&e.value, k) {
                (None, _) => c.meshes_missing += 1,
                (Some(m), MeshKey::GfxObj(_)) => {
                    c.meshes += 1;
                    c.triangles += m.triangles();
                }
                (Some(m), MeshKey::CellPiece(_)) => {
                    c.cell_pieces += 1;
                    c.triangles += m.triangles();
                }
                (Some(m), MeshKey::Sky(_)) => {
                    c.sky_objects += 1;
                    c.triangles += m.triangles();
                }
            }
        }
        c.materials = self.materials.len();
        let groups: BTreeSet<&Group> = self.users.values().flat_map(|h| h.groups.iter()).collect();
        c.surface_groups = groups.len();
        for (k, e) in &self.textures {
            if e.value.is_none() {
                c.textures_missing += 1;
                continue;
            }
            c.textures += 1;
            match k {
                TextureKey::Solid(_) => c.solid_textures += 1,
                TextureKey::Terrain(_) => c.terrain_textures += 1,
                TextureKey::TerrainSource { .. } => c.terrain_sources += 1,
                TextureKey::Dat { .. } | TextureKey::Shifted { .. } => {}
            }
        }
        c.splats = self.splats.values().filter(|e| e.value.is_some()).count();
        c.terrain_blocks = self.terrain.len();
        for u in self.users.keys() {
            match u {
                User::Block(_) => c.blocks += 1,
                User::Object(_) => c.objects += 1,
                User::Sky | User::Particle(_) => {}
            }
        }
        c
    }

    /// What has been handed to sinks so far.
    #[must_use]
    pub fn uploads(&self) -> UploadCounts {
        self.uploads
    }
}

/// One user fewer for `key`; `true` when that was the last and the entry went.
fn drop_one<K: Ord, T>(map: &mut BTreeMap<K, Held<T>>, key: &K) -> bool {
    let Some(e) = map.get_mut(key) else {
        return false;
    };
    e.users = e.users.saturating_sub(1);
    if e.users == 0 {
        map.remove(key);
        true
    } else {
        false
    }
}
