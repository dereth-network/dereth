//! [`Region`] — the single `0x13000000` region-description record.
//!
//! Region records are described in `docs/formats/15-region.md`. The reference parser decodes the
//! one shipped region with zero trailing bytes.

use dereth_dat::{packobj::read_n, ContainerEra, Cursor, DbType};
use dereth_primitives::DataId;

use crate::error::AssetError;
use crate::Decode;

/// The land-height table contains 256 floats.
pub const LAND_HEIGHT_TABLE_LEN: usize = 256;

/// `LandDefs`.
#[derive(Debug, Clone, PartialEq)]
pub struct LandDefs {
    pub num_block_length: u32,
    pub num_block_width: u32,
    pub square_length: f32,
    pub lblock_length: u32,
    pub vertex_per_cell: u32,
    pub max_obj_height: f32,
    pub sky_height: f32,
    pub road_width: f32,
    /// 256 entries, `2.0 * i` up to index 200 and non-linear above 201, reaching
    /// 700.0 at index 255.
    pub land_height_table: Vec<f32>,
}

/// One row of `times_of_day`.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeOfDay {
    pub begin: f32,
    pub is_night: u32,
    pub name: String,
}

/// One row of `seasons`.
#[derive(Debug, Clone, PartialEq)]
pub struct Season {
    pub begin: u32,
    pub name: String,
}

/// `RegionGameTime`: the four calendar constants.
#[derive(Debug, Clone, PartialEq)]
pub struct GameTime {
    pub zero_time_of_year: f64,
    pub zero_year: u32,
    pub day_length: f32,
    pub days_per_year: u32,
    pub year_spec: String,
    pub times_of_day: Vec<TimeOfDay>,
    pub days_of_the_week: Vec<String>,
    pub seasons: Vec<Season>,
}

/// One `SkyObject`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkyObject {
    pub begin_time: f32,
    pub end_time: f32,
    pub begin_angle: f32,
    pub end_angle: f32,
    pub tex_velocity: (f32, f32),
    pub default_gfx_object: DataId,
    pub default_pes_object: DataId,
    pub properties: u32,
}

/// One `SkyObjectReplace`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkyObjectReplace {
    pub object_index: u32,
    pub gfx_obj_id: DataId,
    pub rotate: f32,
    pub transparent: f32,
    pub luminosity: f32,
    pub max_bright: f32,
}

/// One `SkyTimeOfDay`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkyTime {
    pub begin: f32,
    pub dir_bright: f32,
    pub dir_heading: f32,
    pub dir_pitch: f32,
    pub dir_color: u32,
    pub amb_bright: f32,
    pub amb_color: u32,
    pub min_world_fog: f32,
    pub max_world_fog: f32,
    pub world_fog_color: u32,
    pub world_fog: u32,
    pub sky_obj_replace: Vec<SkyObjectReplace>,
}

/// One `SkyDayPreset`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkyDayPreset {
    pub chance_of_occur: f32,
    pub day_name: String,
    pub sky_objects: Vec<SkyObject>,
    pub sky_time: Vec<SkyTime>,
}

/// Sky information, present iff `parts_mask & 0x10`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkyInfo {
    pub tick_size: f64,
    pub light_tick_size: f64,
    pub day_groups: Vec<SkyDayPreset>,
}

/// One ambient-sound row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbientSound {
    pub stype: u32,
    pub volume: f32,
    pub base_chance: f32,
    pub min_rate: f32,
    pub max_rate: f32,
}

/// One `SoundDesc`.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundDesc {
    pub stb_id: DataId,
    pub ambient_sounds: Vec<AmbientSound>,
}

/// One `SceneDesc`.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneDesc {
    pub stb_index: i32,
    pub scenes: Vec<DataId>,
}

/// One `TerrainType`.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainType {
    pub terrain_name: String,
    pub terrain_color: u32,
    pub scene_types: Vec<i32>,
}

/// One `TerrainDesc`, the per-terrain-type texture set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainDesc {
    pub terrain_type: u32,
    pub tex_gid: DataId,
    pub tex_tiling: u32,
    pub max_vert_bright: u32,
    pub min_vert_bright: u32,
    pub max_vert_saturate: u32,
    pub min_vert_saturate: u32,
    pub max_vert_hue: u32,
    pub min_vert_hue: u32,
    pub detail_tex_tiling: u32,
    pub detail_tex_gid: DataId,
}

/// One code → texture mapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CodeTexture {
    pub code: u32,
    pub tex_gid: DataId,
}

/// `TexMerge`, the `LandSurf` variant the shipped region uses (`type == 0`).
#[derive(Debug, Clone, PartialEq)]
pub struct TexMerge {
    pub base_tex_size: u32,
    pub corner_terrain_maps: Vec<CodeTexture>,
    pub side_terrain_maps: Vec<CodeTexture>,
    pub road_maps: Vec<CodeTexture>,
    pub terrain_desc: Vec<TerrainDesc>,
}

/// One road code of a [`PalShiftTexture`]: a sub-palette type per sub-palette range.
#[derive(Debug, Clone, PartialEq)]
pub struct PalShiftRoad {
    pub road_code: u32,
    /// Per range: 0-3 a corner counted from the cell's rotation, 4 the road.
    pub sub_palette_types: Vec<u32>,
}

/// One texture of the palette-shift land surface: the texture, its `(index, length)` sub-palette
/// ranges, the road codes and the palette each terrain type shifts it to.
#[derive(Debug, Clone, PartialEq)]
pub struct PalShiftTexture {
    pub tex_gid: DataId,
    pub sub_palettes: Vec<(u32, u32)>,
    pub road_maps: Vec<PalShiftRoad>,
    /// `(terrain type, palette)`.
    pub terrain_palettes: Vec<(u32, DataId)>,
}

/// `PalShift`, the land surface before Throne of Destiny (`type == 1`): terrain drawn by shifting
/// the palettes of a few textures.
#[derive(Debug, Clone, PartialEq)]
pub struct PalShift {
    pub textures: Vec<PalShiftTexture>,
}

/// `LandSurf`.
///
/// `type == 0` is texture merging, the only kind from Throne of Destiny on; `type == 1` is the
/// palette-shift technique of the dat set before it, and a later-layout region naming it is
/// refused ([`AssetError::Unsupported`]).
#[derive(Debug, Clone, PartialEq)]
pub struct LandSurf {
    pub surf_type: u32,
    pub tex_merge: Option<TexMerge>,
    /// Present exactly when `tex_merge` is not.
    pub pal_shift: Option<PalShift>,
}

/// `RegionMisc`, present iff `parts_mask & 0x200`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionMisc {
    pub version: u32,
    pub game_map: DataId,
    pub autotest_map: DataId,
    pub autotest_map_size: u32,
    pub clear_cell: DataId,
    pub clear_monster: DataId,
}

/// A decoded region: its land definitions, game time, sky, sounds, scenes and terrain types.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub id: DataId,
    pub region_number: u32,
    pub version: u32,
    pub region_name: String,
    pub land_defs: LandDefs,
    pub game_time: GameTime,
    /// Bit `0x8` is set but the encounter description is never serialised — nothing extra
    /// is read for it, and the record still ends exactly.
    pub parts_mask: u32,
    pub sky_info: Option<SkyInfo>,
    pub sound_info: Option<Vec<SoundDesc>>,
    pub scene_info: Option<Vec<SceneDesc>>,
    pub terrain_types: Vec<TerrainType>,
    pub land_surf: LandSurf,
    pub region_misc: Option<RegionMisc>,
}

impl Decode for Region {
    const TYPE: DbType = DbType::Region;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    /// A sky object has a particle-script id only from August 2012; the regions of the later
    /// files before then have eight-word sky objects, as the older files do. Nothing else in the
    /// record says which, so the record is read with the particle-script id first and, when that
    /// does not end exactly on the record's end, without it.
    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let start = c.position();
        if let Ok(r) = decode_region(c, ContainerEra::Modern, true) {
            if c.remaining() == 0 {
                return Ok(r);
            }
        }
        c.seek(start)?;
        if let Ok(r) = decode_region(c, ContainerEra::Modern, false) {
            if c.remaining() == 0 {
                return Ok(r);
            }
        }
        // Neither fits: report the reading with the particle-script id.
        c.seek(start)?;
        decode_region(c, ContainerEra::Modern, true)
    }

    /// Before Throne of Destiny a sky object has no particle-script id (eight words), and the land
    /// surface is the palette-shift technique ([`PalShift`], type 1) rather than texture merging.
    fn decode_classic(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        decode_region(c, ContainerEra::Classic, false)
    }
}

/// `sky_particles`: whether a sky object carries a particle-script id (nine words, not eight).
fn decode_region(
    c: &mut Cursor<'_>,
    era: ContainerEra,
    sky_particles: bool,
) -> Result<Region, AssetError> {
    let id = c.data_id()?;
    let region_number = c.u32()?;
    let version = c.u32()?;
    let region_name = c.packobj_string()?;

    let land_defs = read_land_defs(c)?;

    let game_time = read_game_time(c)?;

    let parts_mask = c.u32()?;

    let sky_info = read_sky(c, parts_mask, sky_particles)?;

    let sound_info = read_sounds(c, parts_mask)?;

    let scene_info = read_scenes(c, parts_mask)?;

    let terrain_types = read_terrain_types(c)?;

    let land_surf = read_land_surface(c, era)?;

    let region_misc = read_misc(c, parts_mask)?;

    Ok(Region {
        id,
        region_number,
        version,
        region_name,
        land_defs,
        game_time,
        parts_mask,
        sky_info,
        sound_info,
        scene_info,
        terrain_types,
        land_surf,
        region_misc,
    })
}

fn read_land_defs(c: &mut Cursor<'_>) -> Result<LandDefs, AssetError> {
    let land_defs = LandDefs {
        num_block_length: c.u32()?,
        num_block_width: c.u32()?,
        square_length: c.f32()?,
        lblock_length: c.u32()?,
        vertex_per_cell: c.u32()?,
        max_obj_height: c.f32()?,
        sky_height: c.f32()?,
        road_width: c.f32()?,
        land_height_table: read_n(c, LAND_HEIGHT_TABLE_LEN, Cursor::f32)?,
    };

    Ok(land_defs)
}

fn read_game_time(c: &mut Cursor<'_>) -> Result<GameTime, AssetError> {
    let zero_time_of_year = c.f64()?;
    let zero_year = c.u32()?;
    let day_length = c.f32()?;
    let days_per_year = c.u32()?;
    let year_spec = c.packobj_string()?;
    let n = c.u32()? as usize;
    let times_of_day = read_n(c, n, |c| {
        Ok(TimeOfDay {
            begin: c.f32()?,
            is_night: c.u32()?,
            name: c.packobj_string()?,
        })
    })?;
    let n = c.u32()? as usize;
    let days_of_the_week = read_n(c, n, Cursor::packobj_string)?;
    let n = c.u32()? as usize;
    let seasons = read_n(c, n, |c| {
        Ok(Season {
            begin: c.u32()?,
            name: c.packobj_string()?,
        })
    })?;
    let game_time = GameTime {
        zero_time_of_year,
        zero_year,
        day_length,
        days_per_year,
        year_spec,
        times_of_day,
        days_of_the_week,
        seasons,
    };

    Ok(game_time)
}

fn read_sky(
    c: &mut Cursor<'_>,
    parts_mask: u32,
    sky_particles: bool,
) -> Result<Option<SkyInfo>, AssetError> {
    let sky_info = if parts_mask & 0x10 != 0 {
        let tick_size = c.f64()?;
        let light_tick_size = c.f64()?;
        c.align_ptr();
        let n = c.u32()? as usize;
        let mut day_groups = Vec::new();
        for _ in 0..n {
            let chance_of_occur = c.f32()?;
            let day_name = c.packobj_string()?;
            let m = c.u32()? as usize;
            let mut sky_objects = Vec::new();
            for _ in 0..m {
                let o = read_sky_object(c, sky_particles)?;
                sky_objects.push(o);
            }
            let m = c.u32()? as usize;
            let mut sky_time = Vec::new();
            for _ in 0..m {
                let begin = c.f32()?;
                let dir_bright = c.f32()?;
                let dir_heading = c.f32()?;
                let dir_pitch = c.f32()?;
                let dir_color = c.u32()?;
                let amb_bright = c.f32()?;
                let amb_color = c.u32()?;
                let min_world_fog = c.f32()?;
                let max_world_fog = c.f32()?;
                let world_fog_color = c.u32()?;
                let world_fog = c.u32()?;
                c.align_ptr();
                let k = c.u32()? as usize;
                let mut sky_obj_replace = Vec::new();
                for _ in 0..k {
                    let r = read_sky_replacement(c)?;
                    sky_obj_replace.push(r);
                }
                sky_time.push(SkyTime {
                    begin,
                    dir_bright,
                    dir_heading,
                    dir_pitch,
                    dir_color,
                    amb_bright,
                    amb_color,
                    min_world_fog,
                    max_world_fog,
                    world_fog_color,
                    world_fog,
                    sky_obj_replace,
                });
            }
            day_groups.push(SkyDayPreset {
                chance_of_occur,
                day_name,
                sky_objects,
                sky_time,
            });
        }
        Some(SkyInfo {
            tick_size,
            light_tick_size,
            day_groups,
        })
    } else {
        None
    };

    Ok(sky_info)
}

fn read_sounds(c: &mut Cursor<'_>, parts_mask: u32) -> Result<Option<Vec<SoundDesc>>, AssetError> {
    let sound_info = if parts_mask & 0x01 != 0 {
        let n = c.u32()? as usize;
        Some(read_n(c, n, |c| {
            let stb_id = c.data_id()?;
            let m = c.u32()? as usize;
            let ambient_sounds = read_n(c, m, |c| {
                Ok(AmbientSound {
                    stype: c.u32()?,
                    volume: c.f32()?,
                    base_chance: c.f32()?,
                    min_rate: c.f32()?,
                    max_rate: c.f32()?,
                })
            })?;
            Ok(SoundDesc {
                stb_id,
                ambient_sounds,
            })
        })?)
    } else {
        None
    };

    Ok(sound_info)
}

fn read_scenes(c: &mut Cursor<'_>, parts_mask: u32) -> Result<Option<Vec<SceneDesc>>, AssetError> {
    let scene_info = if parts_mask & 0x02 != 0 {
        let n = c.u32()? as usize;
        Some(read_n(c, n, |c| {
            let stb_index = c.i32()?;
            let m = c.u32()? as usize;
            Ok(SceneDesc {
                stb_index,
                scenes: read_n(c, m, Cursor::data_id)?,
            })
        })?)
    } else {
        None
    };

    Ok(scene_info)
}

fn read_terrain_types(c: &mut Cursor<'_>) -> Result<Vec<TerrainType>, AssetError> {
    let n = c.u32()? as usize;
    let terrain_types = read_n(c, n, |c| {
        let terrain_name = c.packobj_string()?;
        let terrain_color = c.u32()?;
        let m = c.u32()? as usize;
        Ok(TerrainType {
            terrain_name,
            terrain_color,
            scene_types: read_n(c, m, Cursor::i32)?,
        })
    })?;

    Ok(terrain_types)
}

fn read_land_surface(c: &mut Cursor<'_>, era: ContainerEra) -> Result<LandSurf, AssetError> {
    let pre_tod = era == ContainerEra::Classic;
    let surf_type = c.u32()?;
    let tex_merge = if surf_type == 0 {
        let base_tex_size = c.u32()?;
        let code_tex = |c: &mut Cursor<'_>| {
            let n = c.u32()? as usize;
            read_n(c, n, |c| {
                Ok(CodeTexture {
                    code: c.u32()?,
                    tex_gid: c.data_id()?,
                })
            })
        };
        let corner_terrain_maps = code_tex(c)?;
        let side_terrain_maps = code_tex(c)?;
        let road_maps = code_tex(c)?;
        let n = c.u32()? as usize;
        let terrain_desc = read_n(c, n, |c| {
            Ok(TerrainDesc {
                terrain_type: c.u32()?,
                tex_gid: c.data_id()?,
                tex_tiling: c.u32()?,
                max_vert_bright: c.u32()?,
                min_vert_bright: c.u32()?,
                max_vert_saturate: c.u32()?,
                min_vert_saturate: c.u32()?,
                max_vert_hue: c.u32()?,
                min_vert_hue: c.u32()?,
                detail_tex_tiling: c.u32()?,
                detail_tex_gid: c.data_id()?,
            })
        })?;
        Some(TexMerge {
            base_tex_size,
            corner_terrain_maps,
            side_terrain_maps,
            road_maps,
            terrain_desc,
        })
    } else if surf_type == 1 && pre_tod {
        None
    } else {
        // Never taken in data from Throne of Destiny on.
        return Err(AssetError::Unsupported {
            what: "land-surface type (PalShift)",
            value: surf_type,
        });
    };
    let pal_shift = if tex_merge.is_none() {
        let n = c.u32()? as usize;
        Some(PalShift {
            textures: read_n(c, n, |c| {
                let tex_gid = c.data_id()?;
                let n = c.u32()? as usize;
                let sub_palettes = read_n(c, n, |c| Ok((c.u32()?, c.u32()?)))?;
                let r = c.u32()? as usize;
                let road_maps = read_n(c, r, |c| {
                    Ok(PalShiftRoad {
                        road_code: c.u32()?,
                        sub_palette_types: read_n(c, n, Cursor::u32)?,
                    })
                })?;
                let t = c.u32()? as usize;
                let terrain_palettes = read_n(c, t, |c| Ok((c.u32()?, c.data_id()?)))?;
                Ok(PalShiftTexture {
                    tex_gid,
                    sub_palettes,
                    road_maps,
                    terrain_palettes,
                })
            })?,
        })
    } else {
        None
    };

    Ok(LandSurf {
        surf_type,
        tex_merge,
        pal_shift,
    })
}

fn read_misc(c: &mut Cursor<'_>, parts_mask: u32) -> Result<Option<RegionMisc>, AssetError> {
    let region_misc = if parts_mask & 0x200 != 0 {
        Some(RegionMisc {
            version: c.u32()?,
            game_map: c.data_id()?,
            autotest_map: c.data_id()?,
            autotest_map_size: c.u32()?,
            clear_cell: c.data_id()?,
            clear_monster: c.data_id()?,
        })
    } else {
        None
    };

    Ok(region_misc)
}

fn read_sky_object(c: &mut Cursor<'_>, sky_particles: bool) -> Result<SkyObject, AssetError> {
    let value = SkyObject {
        begin_time: c.f32()?,
        end_time: c.f32()?,
        begin_angle: c.f32()?,
        end_angle: c.f32()?,
        tex_velocity: (c.f32()?, c.f32()?),
        default_gfx_object: c.data_id()?,
        // No particle script before August 2012: eight words.
        default_pes_object: if sky_particles {
            c.data_id()?
        } else {
            DataId(0)
        },
        properties: c.u32()?,
    };
    c.align_ptr();
    Ok(value)
}

fn read_sky_replacement(c: &mut Cursor<'_>) -> Result<SkyObjectReplace, AssetError> {
    let value = SkyObjectReplace {
        object_index: c.u32()?,
        gfx_obj_id: c.data_id()?,
        rotate: c.f32()?,
        transparent: c.f32()?,
        luminosity: c.f32()?,
        max_bright: c.f32()?,
    };
    c.align_ptr();
    Ok(value)
}
