//! The region and its descriptions: the region record (`.rgn`), what the landscape is made of
//! (terrain, surfaces, scenes, sky, day, fog, water, sound, encounters, properties), block maps,
//! game time, the landscape constants, detail maps and map notes.
//!
//! The region record lists, for each kind of description, the records of that kind and the index
//! each is known by. The descriptions are mostly lists of "types" (a terrain type, a water type, a
//! fog type...) indexed by the numbers a landblock's vertex words and the block maps carry.

use auberean_dat::reader::Result;
use auberean_dat::{QualifiedIds, Reader, Writer};
use auberean_primitives::DataId;

use crate::common::Color32;
use crate::property::Property;
use crate::{Context, Decode, Encode};

/// A 32-bit truth word, alignment, then the item when the word is set.
fn optional<T>(
    r: &mut Reader<'_>,
    item: impl FnOnce(&mut Reader<'_>) -> Result<T>,
) -> Result<Option<T>> {
    let present = r.u32()? != 0;
    r.align()?;
    if present {
        Ok(Some(item(r)?))
    } else {
        Ok(None)
    }
}

/// A 32-bit truth word, alignment, then the item when there is one.
fn write_optional<T>(w: &mut Writer, item: Option<&T>, write: impl FnOnce(&mut Writer, &T)) {
    w.u32(u32::from(item.is_some()));
    w.align();
    if let Some(item) = item {
        write(w, item);
    }
}

/// A grid of bytes laid over the world (`.bmd`, `0x19000000`+).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BlockMap {
    pub height: u32,
    pub width: u32,
    /// `height * width` bytes.
    pub cells: Vec<u8>,
}

// D2-12592, D2-12594, D2-12596, D2-05039, D2-12593, D2-12595
impl Decode for BlockMap {
    /// The id, the height, the width, then `height * width` bytes.
    // D2-12589
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let height = r.u32()?;
        let width = r.u32()?;
        let n = (height as usize).saturating_mul(width as usize);
        let cells = r.bytes(n)?.to_vec();
        Ok(Self {
            height,
            width,
            cells,
        })
    }
}

impl Encode for BlockMap {
    // D2-12590
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        w.u32(self.height);
        w.u32(self.width);
        w.bytes(&self.cells);
    }
}

/// One entry of a region's list of descriptions: the record and the index it is known by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct IndexedRecord {
    pub record: DataId,
    pub index: u32,
}

/// A counted list of record ids, then the same count of indices, then alignment.
// D2-07496
fn indexed_records(r: &mut Reader<'_>) -> Result<Vec<IndexedRecord>> {
    let n = r.count(8)?;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        v.push(IndexedRecord {
            record: r.data_id()?,
            index: 0,
        });
    }
    for e in &mut v {
        e.index = r.u32()?;
    }
    r.align()?;
    Ok(v)
}

/// A counted list of record ids, then the same count of indices, then alignment.
// D2-07478
fn write_indexed_records(w: &mut Writer, v: &[IndexedRecord]) {
    w.u32(u32::try_from(v.len()).unwrap_or(u32::MAX));
    for e in v {
        w.data_id(e.record);
    }
    for e in v {
        w.u32(e.index);
    }
    w.align();
}

/// The region (`.rgn`, `0x0F000000`+): the world's name, its maps, its size in landblocks, the
/// landscape constants it uses, and the descriptions of every kind it draws on.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Region {
    pub number: u32,
    pub name: String,
    /// The world map picture and its scale.
    pub map: DataId,
    pub map_scale: f32,
    /// A second map picture and its size, for test tooling.
    pub test_map: DataId,
    pub test_map_size: u32,
    /// The landscape constants (`.lnd`).
    pub landscape: DataId,
    /// The world's size in landblocks, along each axis.
    pub blocks_long: u32,
    pub blocks_wide: u32,
    pub sounds: Vec<IndexedRecord>,
    pub scenes: Vec<IndexedRecord>,
    pub terrains: Vec<IndexedRecord>,
    pub surfaces: Vec<IndexedRecord>,
    pub encounters: Vec<IndexedRecord>,
    pub detail_maps: Vec<IndexedRecord>,
    pub skies: Vec<IndexedRecord>,
    pub waters: Vec<IndexedRecord>,
    pub fogs: Vec<IndexedRecord>,
    pub properties: Vec<IndexedRecord>,
    pub world_maps: Vec<IndexedRecord>,
}

// D2-07477, D2-07488, D2-07490, D2-05029, D2-07489, D2-07494, D2-07501
impl Decode for Region {
    /// The id, the region number, the name (a narrow string), the map, its scale, the test map,
    /// its size, the landscape constants' id, the length and width in landblocks, then eleven
    /// indexed lists: sounds, scenes, terrains, surfaces, encounters, detail maps, skies, waters,
    /// fogs, properties, world maps; then alignment.
    // D2-07500
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let number = r.u32()?;
        let name = r.narrow_string()?;
        let map = r.data_id()?;
        let map_scale = r.f32()?;
        let test_map = r.data_id()?;
        let test_map_size = r.u32()?;
        let landscape = r.data_id()?;
        let blocks_long = r.u32()?;
        let blocks_wide = r.u32()?;
        let sounds = indexed_records(r)?;
        let scenes = indexed_records(r)?;
        let terrains = indexed_records(r)?;
        let surfaces = indexed_records(r)?;
        let encounters = indexed_records(r)?;
        let detail_maps = indexed_records(r)?;
        let skies = indexed_records(r)?;
        let waters = indexed_records(r)?;
        let fogs = indexed_records(r)?;
        let properties = indexed_records(r)?;
        let world_maps = indexed_records(r)?;
        r.align()?;
        Ok(Self {
            number,
            name,
            map,
            map_scale,
            test_map,
            test_map_size,
            landscape,
            blocks_long,
            blocks_wide,
            sounds,
            scenes,
            terrains,
            surfaces,
            encounters,
            detail_maps,
            skies,
            waters,
            fogs,
            properties,
            world_maps,
        })
    }
}

impl Encode for Region {
    // D2-07499, D2-07491
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        w.u32(self.number);
        w.narrow_string(&self.name);
        w.data_id(self.map);
        w.f32(self.map_scale);
        w.data_id(self.test_map);
        w.u32(self.test_map_size);
        w.data_id(self.landscape);
        w.u32(self.blocks_long);
        w.u32(self.blocks_wide);
        for list in [
            &self.sounds,
            &self.scenes,
            &self.terrains,
            &self.surfaces,
            &self.encounters,
            &self.detail_maps,
            &self.skies,
            &self.waters,
            &self.fogs,
            &self.properties,
            &self.world_maps,
        ] {
            write_indexed_records(w, list);
        }
        w.align();
    }
}

/// A sound that plays continuously in an ambient sound.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ContinuousSound {
    pub flags: u32,
    /// The sound or music setting it plays (`.sif`, `.mif`).
    pub sound: DataId,
    pub volume: f32,
    pub min_pitch: f32,
    pub max_pitch: f32,
    pub min_height: f32,
    pub max_height: f32,
}

// D2-07648, D2-07649, D2-07650
impl ContinuousSound {
    // D2-07651
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            flags: r.u32()?,
            sound: r.data_id()?,
            volume: r.f32()?,
            min_pitch: r.f32()?,
            max_pitch: r.f32()?,
            min_height: r.f32()?,
            max_height: r.f32()?,
        })
    }
}

/// A sound that plays now and then in an ambient sound.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct IntermittentSound {
    pub flags: u32,
    pub sound: DataId,
    pub volume: f32,
    pub probability: f32,
    pub min_pitch: f32,
    pub max_pitch: f32,
    pub min_height: f32,
    pub max_height: f32,
    /// The time between plays, in seconds.
    pub min_time: f32,
    pub max_time: f32,
}

// D2-07652, D2-07653, D2-07654
impl IntermittentSound {
    // D2-07655
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            flags: r.u32()?,
            sound: r.data_id()?,
            volume: r.f32()?,
            probability: r.f32()?,
            min_pitch: r.f32()?,
            max_pitch: r.f32()?,
            min_height: r.f32()?,
            max_height: r.f32()?,
            min_time: r.f32()?,
            max_time: r.f32()?,
        })
    }
}

/// The ambient sound of one kind of place: its sound environment, and what plays now and then
/// and continuously.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AmbientSound {
    pub kind: u32,
    /// The sound environment (`.snv`).
    pub environment: DataId,
    pub intermittent: Vec<IntermittentSound>,
    pub continuous: Vec<ContinuousSound>,
}

// D2-07636, D2-07641, D2-07642, D2-07647
impl AmbientSound {
    /// The kind, the environment, the intermittent sounds, then the continuous ones.
    // D2-07638, D2-07644, D2-07646
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            kind: r.u32()?,
            environment: r.data_id()?,
            intermittent: r.list(40, IntermittentSound::read)?,
            continuous: r.list(28, ContinuousSound::read)?,
        })
    }
}

/// The region's ambient sounds (`.sod`, `0x10000000`+).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SoundDesc {
    pub version: u32,
    pub sounds: Vec<AmbientSound>,
}

// D2-07625, D2-07630, D2-07632, D2-05030, D2-07631, D2-07634
impl Decode for SoundDesc {
    /// The id, a version word, then a counted list of ambient sounds.
    // D2-07627, D2-07633
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self {
            version: r.u32()?,
            sounds: r.list(16, AmbientSound::read)?,
        })
    }
}

/// A scene that may be placed on terrain, with how often.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct WeightedScene {
    /// The scene (`.scn`, `0x02...`).
    pub scene: DataId,
    pub frequency: f32,
    /// Whether the scene is turned at random when placed.
    pub rotate: u32,
}

/// One kind of scenery: its index, its ambient sound, and the scenes it chooses among.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SceneryKind {
    pub index: u32,
    pub sound: u32,
    pub scenes: Vec<WeightedScene>,
}

// D2-12399, D2-12404, D2-12391, D2-12396, D2-12397
impl SceneryKind {
    /// The index, alignment, the sound, alignment, then a counted list of scenes, each its id,
    /// frequency and rotate word.
    // D2-12398, D2-12401
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let index = r.u32()?;
        r.align()?;
        let sound = r.u32()?;
        r.align()?;
        let scenes = r.list(12, |r| {
            Ok(WeightedScene {
                scene: r.data_id()?,
                frequency: r.f32()?,
                rotate: r.u32()?,
            })
        })?;
        Ok(Self {
            index,
            sound,
            scenes,
        })
    }
}

impl SceneryKind {
    // D2-12392, D2-12393, D2-12400, D2-12402
    fn write(&self, w: &mut Writer) {
        w.u32(self.index);
        w.align();
        w.u32(self.sound);
        w.align();
        w.list(&self.scenes, |w, e| {
            w.data_id(e.scene);
            w.f32(e.frequency);
            w.u32(e.rotate);
        });
    }
}

/// The region's scenery (`.scd`, `0x11000000`+): every kind of scenery.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SceneDesc {
    pub version: u32,
    pub scene_types: Vec<SceneryKind>,
}

// D2-07426, D2-07428, D2-05031, D2-07427, D2-07435, D2-07436
impl Decode for SceneDesc {
    /// A version word **before** the id, the id, then a counted list of scenery kinds.
    // D2-07437
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let version = r.u32()?;
        r.expect_id(id)?;
        Ok(Self {
            version,
            scene_types: r.list(12, SceneryKind::read)?,
        })
    }
}

impl Encode for SceneDesc {
    // D2-07430, D2-07429
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(self.version);
        w.u32(id);
        w.list(&self.scene_types, |w, k| k.write(w));
    }
}

/// Which scenery a terrain type grows: a scenery kind and its index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TerrainScenery {
    pub kind: u32,
    pub index: u32,
}

impl TerrainScenery {
    /// The kind, alignment, the index.
    // D2-12499
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let kind = r.u32()?;
        r.align()?;
        Ok(Self {
            kind,
            index: r.u32()?,
        })
    }
}

impl TerrainScenery {
    // D2-12498
    fn write(self, w: &mut Writer) {
        w.u32(self.kind);
        w.align();
        w.u32(self.index);
    }
}

/// One kind of terrain: its name, surface, index, the angle above which its edges are hard, and
/// the scenery it grows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TerrainType {
    pub name: String,
    pub surface: u32,
    pub index: u32,
    pub hard_edge_angle: f32,
    pub default_scenery: Option<TerrainScenery>,
    pub scenery: Vec<TerrainScenery>,
}

// D2-12493, D2-12494, D2-12495
impl TerrainType {
    /// The name (a narrow string), the surface, alignment, the index, the hard-edge angle, a word
    /// saying a default scenery follows, alignment, the default scenery, then a counted list of
    /// sceneries.
    // D2-12497
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let name = r.narrow_string()?;
        let surface = r.u32()?;
        r.align()?;
        let index = r.u32()?;
        let hard_edge_angle = r.f32()?;
        let default_scenery = optional(r, TerrainScenery::read)?;
        let scenery = r.list(8, TerrainScenery::read)?;
        Ok(Self {
            name,
            surface,
            index,
            hard_edge_angle,
            default_scenery,
            scenery,
        })
    }
}

impl TerrainType {
    // D2-12496, D2-12491
    fn write(&self, w: &mut Writer) {
        w.narrow_string(&self.name);
        w.u32(self.surface);
        w.align();
        w.u32(self.index);
        w.f32(self.hard_edge_angle);
        write_optional(w, self.default_scenery.as_ref(), |w, e| e.write(w));
        w.list(&self.scenery, |w, e| e.write(w));
    }
}

/// The region's terrain (`.trd`, `0x12000000`+): every kind of terrain.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TerrainDesc {
    pub version: u32,
    pub default_terrain: Option<TerrainType>,
    pub terrains: Vec<TerrainType>,
}

// D2-07439, D2-07441, D2-07450, D2-05032, D2-07440, D2-07446
impl Decode for TerrainDesc {
    /// The id, a version word, a word saying a default terrain follows, alignment, the default
    /// terrain, then a counted list of terrains.
    // D2-07451
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let version = r.u32()?;
        let default_terrain = optional(r, TerrainType::read)?;
        let terrains = r.list(20, TerrainType::read)?;
        Ok(Self {
            version,
            default_terrain,
            terrains,
        })
    }
}

impl Encode for TerrainDesc {
    // D2-07447, D2-07442
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        w.u32(self.version);
        write_optional(w, self.default_terrain.as_ref(), |w, t| t.write(w));
        w.list(&self.terrains, |w, t| t.write(w));
    }
}

/// A material a surface draws with, how often, and how wide one tile of it is.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct WeightedMaterial {
    /// A material instance (`0x31...`) or, for the blend masks, a texture (`0x40...`).
    pub material: DataId,
    pub frequency: f32,
    pub tile_width: f32,
}

// D2-12477, D2-12474, D2-12475
impl WeightedMaterial {
    // D2-12476
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            material: r.data_id()?,
            frequency: r.f32()?,
            tile_width: r.f32()?,
        })
    }
}

/// How a surface's vertex colors vary: the colors near and far, and the ranges and curves of hue,
/// saturation and brightness, and how often this variation is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct VertexColoring {
    pub far_color: Color32,
    pub color: Color32,
    pub min_hue: f32,
    pub max_hue: f32,
    pub hue_power: u32,
    pub min_saturation: f32,
    pub max_saturation: f32,
    pub saturation_power: u32,
    pub min_brightness: f32,
    pub max_brightness: f32,
    pub brightness_power: u32,
    pub frequency: f32,
}

// D2-12471, D2-07552
impl VertexColoring {
    // D2-12472
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            far_color: Color32::read(r)?,
            color: Color32::read(r)?,
            min_hue: r.f32()?,
            max_hue: r.f32()?,
            hue_power: r.u32()?,
            min_saturation: r.f32()?,
            max_saturation: r.f32()?,
            saturation_power: r.u32()?,
            min_brightness: r.f32()?,
            max_brightness: r.f32()?,
            brightness_power: r.u32()?,
            frequency: r.f32()?,
        })
    }
}

/// The materials a surface uses on ground of one range of slopes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MaterialGroup {
    pub min_pitch: f32,
    pub max_pitch: f32,
    pub color_distance: f32,
    pub color_interpolate: u32,
    pub auto_edge: u32,
    pub physics_material: DataId,
    pub vertex_colors: Vec<VertexColoring>,
    pub vertex_color_frequency: f32,
    pub base: Vec<WeightedMaterial>,
    pub base_frequency: f32,
    /// The blend masks for a corner, three corners, a diagonal and a side.
    pub corner_masks: Vec<WeightedMaterial>,
    pub corner_frequency: f32,
    pub three_corner_masks: Vec<WeightedMaterial>,
    pub three_corner_frequency: f32,
    pub diagonal_masks: Vec<WeightedMaterial>,
    pub diagonal_frequency: f32,
    pub side_masks: Vec<WeightedMaterial>,
    pub side_frequency: f32,
    pub detail: Vec<WeightedMaterial>,
    pub detail_frequency: f32,
    pub bump: Vec<WeightedMaterial>,
    pub bump_frequency: f32,
}

// D2-12479, D2-12485, D2-12486
impl MaterialGroup {
    /// The pitch range, color distance, interpolation and auto-edge words, alignment, the physics
    /// material, then each list followed by its total frequency: vertex colorings, base
    /// materials, corner masks, three-corner masks, diagonal masks, side masks, detail materials,
    /// bump materials.
    // D2-12480, D2-12488, D2-12489, D2-12490
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let min_pitch = r.f32()?;
        let max_pitch = r.f32()?;
        let color_distance = r.f32()?;
        let color_interpolate = r.u32()?;
        let auto_edge = r.u32()?;
        r.align()?;
        let physics_material = r.data_id()?;
        let vertex_colors = r.list(48, VertexColoring::read)?;
        let vertex_color_frequency = r.f32()?;
        let base = r.list(12, WeightedMaterial::read)?;
        let base_frequency = r.f32()?;
        let corner_masks = r.list(12, WeightedMaterial::read)?;
        let corner_frequency = r.f32()?;
        let three_corner_masks = r.list(12, WeightedMaterial::read)?;
        let three_corner_frequency = r.f32()?;
        let diagonal_masks = r.list(12, WeightedMaterial::read)?;
        let diagonal_frequency = r.f32()?;
        let side_masks = r.list(12, WeightedMaterial::read)?;
        let side_frequency = r.f32()?;
        let detail = r.list(12, WeightedMaterial::read)?;
        let detail_frequency = r.f32()?;
        let bump = r.list(12, WeightedMaterial::read)?;
        let bump_frequency = r.f32()?;
        Ok(Self {
            min_pitch,
            max_pitch,
            color_distance,
            color_interpolate,
            auto_edge,
            physics_material,
            vertex_colors,
            vertex_color_frequency,
            base,
            base_frequency,
            corner_masks,
            corner_frequency,
            three_corner_masks,
            three_corner_frequency,
            diagonal_masks,
            diagonal_frequency,
            side_masks,
            side_frequency,
            detail,
            detail_frequency,
            bump,
            bump_frequency,
        })
    }
}

/// One kind of ground surface (or road): its index, its material groups by slope, and whether it
/// is the empty road.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SurfaceType {
    pub index: u32,
    pub groups: Vec<MaterialGroup>,
    pub empty_road: u32,
}

// D2-12467, D2-12468
impl SurfaceType {
    /// The index, alignment, a counted list of material groups, the empty-road word.
    // D2-12463, D2-12469
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let index = r.u32()?;
        r.align()?;
        let groups = r.list(100, MaterialGroup::read)?;
        Ok(Self {
            index,
            groups,
            empty_road: r.u32()?,
        })
    }
}

/// The region's ground surfaces (`.sfd`, `0x13000000`+): the far terrain and cloud textures, every
/// ground surface and every road.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SurfaceDesc {
    pub version: u32,
    pub far_texture: DataId,
    pub far_texture_scale: f32,
    pub far_water_material: DataId,
    pub cloud_texture: DataId,
    pub cloud_texture_scale: f32,
    pub cloud_scroll_u: f32,
    pub cloud_scroll_v: f32,
    pub surfaces: Vec<SurfaceType>,
    pub roads: Vec<SurfaceType>,
}

// D2-07553, D2-07555, D2-05033, D2-07554, D2-07545
impl Decode for SurfaceDesc {
    /// A version word **before** the id, the id, the far texture, its scale, the far water
    /// material, the cloud texture, its scale and scroll speeds, the surfaces, the roads.
    // D2-07546, D2-07569
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let version = r.u32()?;
        r.expect_id(id)?;
        Ok(Self {
            version,
            far_texture: r.data_id()?,
            far_texture_scale: r.f32()?,
            far_water_material: r.data_id()?,
            cloud_texture: r.data_id()?,
            cloud_texture_scale: r.f32()?,
            cloud_scroll_u: r.f32()?,
            cloud_scroll_v: r.f32()?,
            surfaces: r.list(12, SurfaceType::read)?,
            roads: r.list(12, SurfaceType::read)?,
        })
    }
}

/// An encounter replaced on one kind of terrain or water.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct EncounterException {
    pub index: u32,
    pub encounter: u32,
}

// D2-12512
impl EncounterException {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            index: r.u32()?,
            encounter: r.u32()?,
        })
    }
}

/// One encounter of a row: what it spawns, how its chance is adjusted, whether it depends on the
/// time of day, and its exceptions by terrain and by water.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Encounter {
    pub index: u32,
    pub encounter: u32,
    pub adjust_chance: f32,
    pub day_based: u32,
    pub terrain_exceptions: Vec<EncounterException>,
    pub water_exceptions: Vec<EncounterException>,
}

// D2-12509, D2-12510, D2-07511
impl Encounter {
    // D2-12507, D2-12511
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            index: r.u32()?,
            encounter: r.u32()?,
            adjust_chance: r.f32()?,
            day_based: r.u32()?,
            terrain_exceptions: r.list(8, EncounterException::read)?,
            water_exceptions: r.list(8, EncounterException::read)?,
        })
    }
}

/// A row of the encounter table: its index, its name and its encounters.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EncounterRow {
    pub index: u32,
    pub name: String,
    pub encounters: Vec<Encounter>,
}

// D2-12502, D2-12503, D2-12504, D2-07513
impl EncounterRow {
    // D2-12505, D2-12506
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            index: r.u32()?,
            name: r.narrow_string()?,
            encounters: r.list(24, Encounter::read)?,
        })
    }
}

/// The region's encounters (`.end`, `0x14000000`+).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EncounterDesc {
    pub version: u32,
    /// The block map saying which row applies where.
    pub block_map: DataId,
    pub rows: Vec<EncounterRow>,
}

// D2-07505, D2-07507, D2-05034, D2-07506, D2-07509, D2-07502
impl Decode for EncounterDesc {
    /// A version word **before** the id, the id, the block map, a counted list of rows.
    // D2-07503, D2-07515
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let version = r.u32()?;
        r.expect_id(id)?;
        Ok(Self {
            version,
            block_map: r.data_id()?,
            rows: r.list(12, EncounterRow::read)?,
        })
    }
}

/// A day the sky may show, and how often.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct WeightedDay {
    /// The day description (`.dyd`, `0x1A...`).
    pub day: DataId,
    pub frequency: f32,
}

/// The region's sky (`.skd`, `0x15000000`+): how often the sky and the light are recomputed, the
/// calendar, and the days it chooses among.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SkyDesc {
    pub version: u32,
    pub min_tick: f64,
    pub max_tick: f64,
    pub min_light_tick: f64,
    pub max_light_tick: f64,
    /// The calendar (`.gmt`).
    pub game_time: DataId,
    pub days: Vec<WeightedDay>,
}

// D2-12514, D2-12515, D2-07454, D2-07460, D2-05035, D2-07461, D2-07464, D2-07465
impl Decode for SkyDesc {
    /// A version word **before** the id, the id, four 64-bit tick intervals, the calendar, a
    /// counted list of days and frequencies.
    // D2-07462, D2-07466
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let version = r.u32()?;
        r.expect_id(id)?;
        Ok(Self {
            version,
            min_tick: r.f64()?,
            max_tick: r.f64()?,
            min_light_tick: r.f64()?,
            max_light_tick: r.f64()?,
            game_time: r.data_id()?,
            days: r.list(8, |r| {
                Ok(WeightedDay {
                    day: r.data_id()?,
                    frequency: r.f32()?,
                })
            })?,
        })
    }
}

/// One property laid over the world by a block map: which property, the map, whether the map's
/// values are discrete, and the value per map value.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MappedProperty {
    pub property: u32,
    pub block_map: DataId,
    pub discrete: u32,
    pub values: Vec<Property>,
}

// D2-12570, D2-12571, D2-12586
impl MappedProperty {
    // D2-12563, D2-04378
    fn read(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        Ok(Self {
            property: r.u32()?,
            block_map: r.data_id()?,
            discrete: r.u32()?,
            values: r.list(4, |r| Property::read(r, ctx))?,
        })
    }
}

/// The region's properties laid over the world (`.prd`, `0x18000000`+).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PropertyDesc {
    pub version: u32,
    pub properties: Vec<MappedProperty>,
}

// D2-12578, D2-12580, D2-05038, D2-12579, D2-12583
impl Decode for PropertyDesc {
    /// The id, a version word, a counted list of mapped properties.
    // D2-07627, D2-12588
    fn decode(r: &mut Reader<'_>, id: u32, ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self {
            version: r.u32()?,
            properties: r.list(16, |r| MappedProperty::read(r, ctx))?,
        })
    }
}

/// How one channel of fog or light combines with the one it meets: a mode, and for adding the
/// two multipliers.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Combine {
    /// 0 multiply, 1 add, 2 use the first, 3 use the second, 4 use the greater.
    pub mode: u32,
    pub first: f32,
    pub second: f32,
}

/// Fog and light: the fog's distances, density, kind and color; the direct and ambient light; and
/// how each channel combines (ambient brightness, ambient color, direct brightness, direct color,
/// daylight, day fog, distance fog).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FogAndLight {
    pub fog_min: f32,
    pub fog_max: f32,
    pub fog_sky: f32,
    pub fog_density: f32,
    /// 0 no fog, 1 linear.
    pub fog_kind: u32,
    pub fog_color: Color32,
    pub direct_brightness: f32,
    pub direct_color: Color32,
    pub ambient_brightness: f32,
    pub ambient_color: Color32,
    /// The combine code: seven 3-bit modes, ambient brightness lowest.
    pub combine_code: u32,
    pub combines: [Combine; 7],
}

// D2-12536, D2-12540, D2-12534, D2-12535, D2-10863, D2-10864
impl FogAndLight {
    /// The four fog values, the fog kind, the fog color, the direct brightness and color, the
    /// ambient brightness and color, the combine code, then the two multipliers of each channel
    /// whose mode is adding. A mode field above 4 counts as multiply.
    // D2-12541, D2-12539, D2-12538, D2-13245
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let fog_min = r.f32()?;
        let fog_max = r.f32()?;
        let fog_sky = r.f32()?;
        let fog_density = r.f32()?;
        let fog_kind = r.u32()?;
        let fog_color = Color32::read(r)?;
        let direct_brightness = r.f32()?;
        let direct_color = Color32::read(r)?;
        let ambient_brightness = r.f32()?;
        let ambient_color = Color32::read(r)?;
        let combine_code = r.u32()?;
        let mut combines = [Combine::default(); 7];
        for (i, c) in combines.iter_mut().enumerate() {
            let field = (combine_code >> (3 * i)) & 7;
            c.mode = if field <= 4 { field } else { 0 };
            if c.mode == 1 {
                c.first = r.f32()?;
                c.second = r.f32()?;
            }
        }
        Ok(Self {
            fog_min,
            fog_max,
            fog_sky,
            fog_density,
            fog_kind,
            fog_color,
            direct_brightness,
            direct_color,
            ambient_brightness,
            ambient_color,
            combine_code,
            combines,
        })
    }
}

impl FogAndLight {
    /// The combine code the channels' modes make: seven 3-bit modes, ambient brightness lowest.
    // D2-12542, D2-12537
    #[must_use]
    pub fn code(&self) -> u32 {
        self.combines
            .iter()
            .enumerate()
            .fold(0, |code, (i, c)| code | ((c.mode & 7) << (3 * i)))
    }

    // D2-12543, D2-12544
    fn write(&self, w: &mut Writer) {
        w.f32(self.fog_min);
        w.f32(self.fog_max);
        w.f32(self.fog_sky);
        w.f32(self.fog_density);
        w.u32(self.fog_kind);
        self.fog_color.write(w);
        w.f32(self.direct_brightness);
        self.direct_color.write(w);
        w.f32(self.ambient_brightness);
        self.ambient_color.write(w);
        // A stored mode above 4, which reads as multiply, stays as it was stored.
        let kept = (0..7).fold(0, |kept, i| {
            let field = (self.combine_code >> (3 * i)) & 7;
            if field > 4 && self.combines[i].mode == 0 {
                kept | (field << (3 * i))
            } else {
                kept
            }
        });
        w.u32(self.code() | kept);
        for c in &self.combines {
            if c.mode == 1 {
                w.f32(c.first);
                w.f32(c.second);
            }
        }
    }
}

/// The fog and light under water from one depth down.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WaterLight {
    pub depth: f32,
    pub light: FogAndLight,
}

/// One kind of water: its index, and unless it is "no water", its materials, tiling, color and
/// the fog and light at each depth.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WaterType {
    pub index: u32,
    pub no_water: u32,
    pub above_material: DataId,
    pub below_material: DataId,
    pub physics_material: DataId,
    pub tile_width: f32,
    pub poly_width: f32,
    pub color: Color32,
    pub lighting: Vec<WaterLight>,
}

// D2-12545, D2-12556, D2-12561, D2-12548, D2-12549, D2-12553
impl WaterType {
    /// The index and the no-water word; then, for water, the above, below and physics materials,
    /// the tile and polygon widths, the color, and a counted list of depths with their fog and
    /// light.
    // D2-12554, D2-12558
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let index = r.u32()?;
        let no_water = r.u32()?;
        let mut t = Self {
            index,
            no_water,
            ..Self::default()
        };
        if no_water == 0 {
            t.above_material = r.data_id()?;
            t.below_material = r.data_id()?;
            t.physics_material = r.data_id()?;
            t.tile_width = r.f32()?;
            t.poly_width = r.f32()?;
            t.color = Color32::read(r)?;
            t.lighting = r.list(48, |r| {
                Ok(WaterLight {
                    depth: r.f32()?,
                    light: FogAndLight::read(r)?,
                })
            })?;
        }
        Ok(t)
    }
}

impl WaterType {
    // D2-12550, D2-12551, D2-12557
    fn write(&self, w: &mut Writer) {
        w.u32(self.index);
        w.u32(self.no_water);
        if self.no_water == 0 {
            w.data_id(self.above_material);
            w.data_id(self.below_material);
            w.data_id(self.physics_material);
            w.f32(self.tile_width);
            w.f32(self.poly_width);
            self.color.write(w);
            w.list(&self.lighting, |w, l| {
                w.f32(l.depth);
                l.light.write(w);
            });
        }
    }
}

/// The region's water (`.wtd`, `0x16000000`+).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WaterDesc {
    pub version: u32,
    /// The block maps of water heights and water kinds.
    pub height_map: DataId,
    pub kind_map: DataId,
    pub default_water: Option<WaterType>,
    pub waters: Vec<WaterType>,
}

// D2-07516, D2-07518, D2-05036, D2-07517, D2-07530, D2-07531
impl Decode for WaterDesc {
    /// A version word **before** the id, the id, the two maps, a word saying a default water
    /// follows, alignment, the default water, then a counted list of waters.
    // D2-07532
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let version = r.u32()?;
        r.expect_id(id)?;
        let height_map = r.data_id()?;
        let kind_map = r.data_id()?;
        let default_water = optional(r, WaterType::read)?;
        let waters = r.list(8, WaterType::read)?;
        Ok(Self {
            version,
            height_map,
            kind_map,
            default_water,
            waters,
        })
    }
}

impl Encode for WaterDesc {
    // D2-07519, D2-07520
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(self.version);
        w.u32(id);
        w.data_id(self.height_map);
        w.data_id(self.kind_map);
        write_optional(w, self.default_water.as_ref(), |w, t| t.write(w));
        w.list(&self.waters, |w, t| t.write(w));
    }
}

/// One kind of fog: its index and its fog and light.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FogType {
    pub index: u32,
    pub light: FogAndLight,
}

// D2-12529, D2-12533
impl FogType {
    // D2-12530
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            index: r.u32()?,
            light: FogAndLight::read(r)?,
        })
    }
}

impl FogType {
    // D2-12559
    fn write(&self, w: &mut Writer) {
        w.u32(self.index);
        self.light.write(w);
    }
}

/// The region's fog (`.fgd`, `0x17000000`+).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FogDesc {
    pub version: u32,
    /// The block maps of distance, volume, bottom and top fog.
    pub distance_map: DataId,
    pub volume_map: DataId,
    pub bottom_map: DataId,
    pub top_map: DataId,
    pub default_fog: Option<FogType>,
    pub fogs: Vec<FogType>,
}

// D2-07533, D2-07535, D2-05037, D2-07534, D2-07542, D2-07543
impl Decode for FogDesc {
    /// A version word **before** the id, the id, four maps, a word saying a default fog follows,
    /// alignment, the default fog, then a counted list of fogs.
    // D2-07544
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let version = r.u32()?;
        r.expect_id(id)?;
        let distance_map = r.data_id()?;
        let volume_map = r.data_id()?;
        let bottom_map = r.data_id()?;
        let top_map = r.data_id()?;
        let default_fog = optional(r, FogType::read)?;
        let fogs = r.list(48, FogType::read)?;
        Ok(Self {
            version,
            distance_map,
            volume_map,
            bottom_map,
            top_map,
            default_fog,
            fogs,
        })
    }
}

impl Encode for FogDesc {
    // D2-07536, D2-07537
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(self.version);
        w.u32(id);
        w.data_id(self.distance_map);
        w.data_id(self.volume_map);
        w.data_id(self.bottom_map);
        w.data_id(self.top_map);
        write_optional(w, self.default_fog.as_ref(), |w, t| t.write(w));
        w.list(&self.fogs, |w, t| t.write(w));
    }
}

/// A sky object's look replaced from one time of day: which object, what it shows, its offsets
/// and its color.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SkyReplacement {
    pub object: u32,
    pub mesh: DataId,
    pub pitch_offset: f32,
    pub heading_offset: f32,
    pub color: Color32,
}

// D2-12518
impl SkyReplacement {
    /// The object, what it shows, the pitch offset, the heading offset, the color.
    // D2-12519
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            object: r.u32()?,
            mesh: r.data_id()?,
            pitch_offset: r.f32()?,
            heading_offset: r.f32()?,
            color: Color32::read(r)?,
        })
    }
}

/// The sky and light from one time of day on.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SkyTime {
    /// When this time begins, as a fraction of the day.
    pub begin: f32,
    pub fog_min: f32,
    pub fog_max: f32,
    pub fog_sky: f32,
    pub fog_color: Color32,
    pub direct_brightness: f32,
    pub direct_heading: f32,
    pub direct_pitch: f32,
    pub direct_color: Color32,
    pub ambient_brightness: f32,
    pub ambient_color: Color32,
    pub shadow_color: Color32,
    pub replacements: Vec<SkyReplacement>,
    pub rain_amount: f32,
    pub rain_wind_x: f32,
    pub rain_wind_y: f32,
}

// D2-12520, D2-12524, D2-12525, D2-12528
impl SkyTime {
    /// The beginning, the fog distances, the fog color, the direct brightness, heading and pitch,
    /// the direct color, the ambient brightness and color, the shadow color, a counted list of
    /// sky-object replacements, then the rain amount and wind.
    // D2-12521, D2-12527
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            begin: r.f32()?,
            fog_min: r.f32()?,
            fog_max: r.f32()?,
            fog_sky: r.f32()?,
            fog_color: Color32::read(r)?,
            direct_brightness: r.f32()?,
            direct_heading: r.f32()?,
            direct_pitch: r.f32()?,
            direct_color: Color32::read(r)?,
            ambient_brightness: r.f32()?,
            ambient_color: Color32::read(r)?,
            shadow_color: Color32::read(r)?,
            replacements: r.list(20, SkyReplacement::read)?,
            rain_amount: r.f32()?,
            rain_wind_x: r.f32()?,
            rain_wind_y: r.f32()?,
        })
    }
}

/// A point of a lens flare: where along the flare's axis, its texture and its size.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FlarePoint {
    pub axis_position: f32,
    pub texture: DataId,
    pub size: f32,
}

/// A lens flare: how fast it fades, and its points.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LensFlare {
    pub fade_speed: f32,
    pub points: Vec<FlarePoint>,
}

impl LensFlare {
    // D2-10368
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            fade_speed: r.f32()?,
            points: r.list(12, |r| {
                Ok(FlarePoint {
                    axis_position: r.f32()?,
                    texture: r.data_id()?,
                    size: r.f32()?,
                })
            })?,
        })
    }
}

/// An object crossing the sky (a sun, a moon, clouds): what it shows, when, along which arc, and
/// its lens flare.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SkyObject {
    pub name: u32,
    pub mesh: DataId,
    pub begin_time: f32,
    pub end_time: f32,
    pub begin_pitch: f32,
    pub end_pitch: f32,
    pub begin_heading: f32,
    pub end_heading: f32,
    pub height: f32,
    pub after: u32,
    pub fixed_position: u32,
    pub water_effect: u32,
    pub fixed_x: f32,
    pub fixed_y: f32,
    pub west_to_east: u32,
    pub flare: LensFlare,
}

// D2-07468, D2-07470, D2-07471
impl SkyObject {
    /// The name, the mesh, the begin and end times, pitches and headings, the height, the after,
    /// fixed-position and water-effect words, the fixed position, the west-to-east word, then the
    /// lens flare.
    // D2-07472
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            name: r.u32()?,
            mesh: r.data_id()?,
            begin_time: r.f32()?,
            end_time: r.f32()?,
            begin_pitch: r.f32()?,
            end_pitch: r.f32()?,
            begin_heading: r.f32()?,
            end_heading: r.f32()?,
            height: r.f32()?,
            after: r.u32()?,
            fixed_position: r.u32()?,
            water_effect: r.u32()?,
            fixed_x: r.f32()?,
            fixed_y: r.f32()?,
            west_to_east: r.u32()?,
            flare: LensFlare::read(r)?,
        })
    }
}

/// A day of the sky (`.dyd`, `0x1A000000`+): its times of day and its sky objects.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DayDesc {
    pub version: u32,
    pub times: Vec<SkyTime>,
    pub objects: Vec<SkyObject>,
}

// D2-07576, D2-07586, D2-07588, D2-05040, D2-07587, D2-07593
impl Decode for DayDesc {
    /// A version word **before** the id, the id, a counted list of times of day, a counted list of
    /// sky objects.
    // D2-07577, D2-07589, D2-07598
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let version = r.u32()?;
        r.expect_id(id)?;
        Ok(Self {
            version,
            times: r.list(64, SkyTime::read)?,
            objects: r.list(68, SkyObject::read)?,
        })
    }
}

/// A part of the day: when it begins (a fraction of the day), whether it is night, and its name's
/// string token.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct DayPart {
    pub begin: f32,
    pub night: u32,
    pub name: u32,
}

/// A season: its name's string token and the day of the year it begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Season {
    pub name: u32,
    pub begin: i32,
}

/// The calendar (`.gmt`, `0x35000000`+): where game time starts, the length of a day and a year,
/// the parts of a day, the days of the week and the seasons.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct GameTime {
    /// The game time, in seconds, at which the zero year began.
    pub zero_time: f64,
    pub zero_year: i32,
    /// A day's length in seconds.
    pub day_length: f32,
    pub days_per_year: i32,
    /// The year's name's string token.
    pub year_name: u32,
    pub day_parts: Vec<DayPart>,
    /// The days of the week, by name token.
    pub week_days: Vec<u32>,
    pub seasons: Vec<Season>,
}

// D2-13052, D2-13053, D2-13055, D2-13057, D2-13065, D2-13066, D2-04426, D2-04427, D2-04428
impl Decode for GameTime {
    /// The id, the zero time (64-bit), the zero year, the day length, the days per year, the year
    /// name, then counted lists of day parts, week days and seasons (a season stores its first day
    /// before its name).
    // D2-13058, D2-04374, D2-04375, D2-04376, D2-13056
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self {
            zero_time: r.f64()?,
            zero_year: r.i32()?,
            day_length: r.f32()?,
            days_per_year: r.i32()?,
            year_name: r.u32()?,
            day_parts: r.list(12, |r| {
                Ok(DayPart {
                    begin: r.f32()?,
                    night: r.u32()?,
                    name: r.u32()?,
                })
            })?,
            // D2-05483
            week_days: r.list(4, Reader::u32)?,
            seasons: r.list(8, |r| {
                let begin = r.i32()?;
                Ok(Season {
                    name: r.u32()?,
                    begin,
                })
            })?,
        })
    }
}

/// How a landblock vertex word is split: the width in bits of each field, lowest field first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VertexFields {
    pub terrain: u32,
    pub scene: u32,
    pub encounter: u32,
    pub color: u32,
    pub road: u32,
    pub replace: u32,
}

/// The landscape constants (`.lnd`, `0x36000000`+): the size of a land cell, how many make a
/// landblock side, how the vertex word splits, and the height of each height byte.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Landscape {
    /// A land cell's side, in meters.
    pub cell_side: f32,
    pub road_width: f32,
    pub cells_per_block_side: i32,
    /// Vertex intervals per land cell side.
    pub pixels_per_cell_side: i32,
    pub max_object_height: f32,
    pub sky_height: f32,
    pub vertex_fields: VertexFields,
    /// The height in meters of each value of a landblock's height byte.
    pub heights: Vec<f32>,
}

// D2-05058, D2-13311, D2-13317
impl Decode for Landscape {
    /// The id, the land cell side, the cells per block side, the vertex intervals per cell side,
    /// the greatest object height, the sky height, the road width, the six vertex field widths
    /// (terrain, scene, encounter, color, road, replace), then 256 heights.
    // D2-13316
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let cell_side = r.f32()?;
        let cells_per_block_side = r.i32()?;
        let pixels_per_cell_side = r.i32()?;
        let max_object_height = r.f32()?;
        let sky_height = r.f32()?;
        let road_width = r.f32()?;
        let vertex_fields = VertexFields {
            terrain: r.u32()?,
            scene: r.u32()?,
            encounter: r.u32()?,
            color: r.u32()?,
            road: r.u32()?,
            replace: r.u32()?,
        };
        let mut heights = Vec::with_capacity(256);
        for _ in 0..256 {
            heights.push(r.f32()?);
        }
        Ok(Self {
            cell_side,
            road_width,
            cells_per_block_side,
            pixels_per_cell_side,
            max_object_height,
            sky_height,
            vertex_fields,
            heights,
        })
    }
}

impl Landscape {
    /// The constants until a record gives them: 24 m land cells, eight to a block's side, two
    /// vertex steps to a cell's, objects to 300 m, the sky at 1000 m, roads 5 m wide; the vertex
    /// word's terrain 6 bits, scene 4, encounter 4, color 8, road 2 and replace 8.
    // D2-13310
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            cell_side: 24.0,
            road_width: 5.0,
            cells_per_block_side: 8,
            pixels_per_cell_side: 2,
            max_object_height: 300.0,
            sky_height: 1000.0,
            vertex_fields: VertexFields {
                terrain: 6,
                scene: 4,
                encounter: 4,
                color: 8,
                road: 2,
                replace: 8,
            },
            heights: Vec::new(),
        }
    }
}

impl Encode for Landscape {
    // D2-13315, D2-13314
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        w.f32(self.cell_side);
        w.i32(self.cells_per_block_side);
        w.i32(self.pixels_per_cell_side);
        w.f32(self.max_object_height);
        w.f32(self.sky_height);
        w.f32(self.road_width);
        let f = self.vertex_fields;
        for v in [f.terrain, f.scene, f.encounter, f.color, f.road, f.replace] {
            w.u32(v);
        }
        for h in &self.heights {
            w.f32(*h);
        }
    }
}

/// A rectangle of a detail map: the value it marks, its picture, where it lies and its size, and
/// the map notes it shows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DetailMapRow {
    pub value: u32,
    pub picture: DataId,
    pub x: u32,
    pub y: u32,
    pub height: u32,
    pub width: u32,
    pub notes: Vec<u32>,
}

// D2-07611, D2-07614, D2-07615, D2-07606
impl DetailMapRow {
    // D2-07612
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            value: r.u32()?,
            picture: r.data_id()?,
            x: r.u32()?,
            y: r.u32()?,
            height: r.u32()?,
            width: r.u32()?,
            notes: r.u32_list()?,
        })
    }
}

/// A detail map (`.detailmap`, `0x59000000`-`0x59000FFF`): the zoomed-in map pictures over the
/// world map, keyed by a block map.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DetailMap {
    pub block_map: DataId,
    pub rows: Vec<DetailMapRow>,
}

// D2-07602, D2-05076, D2-07603, D2-07604, D2-07616, D2-07605
impl Decode for DetailMap {
    /// The id, the block map, a counted list of rows.
    // D2-07627, D2-07610
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self {
            block_map: r.data_id()?,
            rows: r.list(28, DetailMapRow::read)?,
        })
    }
}

/// A label on a map: its string token, its icon and its position on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MapNote {
    pub text: u32,
    pub icon: DataId,
    pub x: u32,
    pub y: u32,
}

/// Map notes (`.mapnotes`, `0x5900F000`+): the labels of one map.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MapNotes {
    /// The string table the labels' tokens are in.
    pub strings: DataId,
    pub notes: Vec<MapNote>,
}

// D2-07618, D2-05077, D2-07619, D2-07620, D2-07622, D2-07623, D2-07621
impl Decode for MapNotes {
    /// The id, the string table, a counted list of notes.
    // D2-07627, D2-02569, D2-07624
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self {
            strings: r.data_id()?,
            notes: r.list(16, |r| {
                Ok(MapNote {
                    text: r.u32()?,
                    icon: r.data_id()?,
                    x: r.u32()?,
                    y: r.u32()?,
                })
            })?,
        })
    }
}

// ------------------------------------------------------------------- the records each one names

/// Each id of a list of indexed records.
fn add_records(list: &[IndexedRecord], out: &mut QualifiedIds) {
    for r in list {
        out.add(r.record.0);
    }
}

impl Region {
    /// The records the region names: its map, its test map and its landscape constants, then the
    /// record of every entry of its eleven lists, sounds to world maps.
    // D2-07479
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.map.0);
        out.add(self.test_map.0);
        out.add(self.landscape.0);
        for list in [
            &self.sounds,
            &self.scenes,
            &self.terrains,
            &self.surfaces,
            &self.encounters,
            &self.detail_maps,
            &self.skies,
            &self.waters,
            &self.fogs,
            &self.properties,
            &self.world_maps,
        ] {
            add_records(list, out);
        }
    }
}

impl AmbientSound {
    /// Its sound environment, then each intermittent sound's sound, then each continuous one's.
    // D2-07637
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.environment.0);
        for s in &self.intermittent {
            out.add(s.sound.0);
        }
        for s in &self.continuous {
            out.add(s.sound.0);
        }
    }
}

impl SoundDesc {
    /// Every ambient sound's records.
    // D2-07626
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for s in &self.sounds {
            s.sub_ids(out);
        }
    }
}

impl WeightedScene {
    /// The scene.
    // D2-12403
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.scene.0);
    }
}

impl SceneryKind {
    /// Each of its scenes.
    // D2-12394
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for s in &self.scenes {
            s.sub_ids(out);
        }
    }
}

impl SceneDesc {
    /// Each scenery kind's scenes.
    // D2-07431
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for k in &self.scene_types {
            k.sub_ids(out);
        }
    }
}

impl TerrainDesc {
    /// None: a terrain kind names no record.
    // D2-07443
    pub fn sub_ids(&self, _out: &mut QualifiedIds) {}
}

impl MaterialGroup {
    /// Its physics material, then every material and mask of its base, corner, three-corner,
    /// diagonal, side, detail and bump lists.
    // D2-12481
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.physics_material.0);
        for list in [
            &self.base,
            &self.corner_masks,
            &self.three_corner_masks,
            &self.diagonal_masks,
            &self.side_masks,
            &self.detail,
            &self.bump,
        ] {
            for m in list {
                out.add(m.material.0);
            }
        }
    }
}

impl SurfaceType {
    /// Each material group's records.
    // D2-12464
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for g in &self.groups {
            g.sub_ids(out);
        }
    }
}

impl SurfaceDesc {
    /// The far landscape's picture, the far water's material and the clouds' picture, then every
    /// surface's and every road's records.
    // D2-07547
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.far_texture.0);
        out.add(self.far_water_material.0);
        out.add(self.cloud_texture.0);
        for s in self.surfaces.iter().chain(&self.roads) {
            s.sub_ids(out);
        }
    }
}

impl Encounter {
    /// Its encounter, then each terrain exception's and each water exception's.
    // D2-12508
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.encounter);
        for e in self.terrain_exceptions.iter().chain(&self.water_exceptions) {
            out.add(e.encounter);
        }
    }
}

impl EncounterRow {
    /// Each encounter's records.
    // D2-12501
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for e in &self.encounters {
            e.sub_ids(out);
        }
    }
}

impl EncounterDesc {
    /// Its block map, then every row's records.
    // D2-07504
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.block_map.0);
        for r in &self.rows {
            r.sub_ids(out);
        }
    }
}

impl SkyDesc {
    /// Each day, then the calendar.
    // D2-07455
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for d in &self.days {
            out.add(d.day.0);
        }
        out.add(self.game_time.0);
    }
}

impl PropertyDesc {
    /// Each mapped property's block map (not what its values name).
    // D2-12577
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for p in &self.properties {
            out.add(p.block_map.0);
        }
    }
}

impl WaterType {
    /// Its materials seen from above and below, and its physics material (a kind with no water
    /// too).
    // D2-12546
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.above_material.0);
        out.add(self.below_material.0);
        out.add(self.physics_material.0);
    }
}

impl WaterDesc {
    /// Every listed water kind's records (not the default kind's), then the height and kind maps.
    // D2-07521
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for w in &self.waters {
            w.sub_ids(out);
        }
        out.add(self.height_map.0);
        out.add(self.kind_map.0);
    }
}

impl FogDesc {
    /// Its four block maps: distance, volume, bottom and top (a fog kind names no record).
    // D2-07538
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for m in [
            self.distance_map,
            self.volume_map,
            self.bottom_map,
            self.top_map,
        ] {
            out.add(m.0);
        }
    }
}

impl SkyReplacement {
    /// The model it puts in the object's place.
    // D2-12516
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.mesh.0);
    }
}

impl SkyTime {
    /// Each replacement's model.
    // D2-12522
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for r in &self.replacements {
            r.sub_ids(out);
        }
    }
}

impl LensFlare {
    /// Each point's picture.
    // D2-10365
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for p in &self.points {
            out.add(p.texture.0);
        }
    }
}

impl SkyObject {
    /// Its model, then its lens flare's pictures.
    // D2-07469
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.mesh.0);
        self.flare.sub_ids(out);
    }
}

impl DayDesc {
    /// Every time of day's replacement models, then every sky object's model and flare pictures.
    // D2-07578
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        for t in &self.times {
            t.sub_ids(out);
        }
        for o in &self.objects {
            o.sub_ids(out);
        }
    }
}

impl DetailMapRow {
    /// Its picture, then every map note it lists.
    // D2-07613
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.picture.0);
        for n in &self.notes {
            out.add(*n);
        }
    }
}

impl DetailMap {
    /// Its block map, then every row's records.
    // D2-07601
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.block_map.0);
        for r in &self.rows {
            r.sub_ids(out);
        }
    }
}

impl MapNotes {
    /// Its string table, then each note's icon.
    // D2-07617
    pub fn sub_ids(&self, out: &mut QualifiedIds) {
        out.add(self.strings.0);
        for n in &self.notes {
            out.add(n.icon.0);
        }
    }
}
