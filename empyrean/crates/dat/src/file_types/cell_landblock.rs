// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/FileTypes/CellLandblock.cs
//! The fields of a landblock vertex's terrain word: ACE's `CellLandblock.GetRoad`/`GetType`/
//! `GetScenery`.

pub const TERRAIN_MASK_ROAD: u16 = 0x3;
pub const TERRAIN_MASK_TYPE: u16 = 0x7C;
pub const TERRAIN_MASK_SCENERY: u16 = 0xF800;
pub const TERRAIN_SHIFT_ROAD: u8 = 0;
pub const TERRAIN_SHIFT_TYPE: u8 = 2;
pub const TERRAIN_SHIFT_SCENERY: u8 = 11;

// ACE: CellLandblock.GetRoad
#[must_use]
pub fn get_road(terrain: u16) -> u16 {
    get_terrain(terrain, TERRAIN_MASK_ROAD, TERRAIN_SHIFT_ROAD)
}

// ACE: CellLandblock.GetType
#[must_use]
pub fn get_type(terrain: u16) -> u16 {
    get_terrain(terrain, TERRAIN_MASK_TYPE, TERRAIN_SHIFT_TYPE)
}

// ACE: CellLandblock.GetScenery
#[must_use]
pub fn get_scenery(terrain: u16) -> u16 {
    get_terrain(terrain, TERRAIN_MASK_SCENERY, TERRAIN_SHIFT_SCENERY)
}

// ACE: CellLandblock.GetTerrain
#[must_use]
pub fn get_terrain(terrain: u16, mask: u16, shift: u8) -> u16 {
    (terrain & mask) >> shift
}
