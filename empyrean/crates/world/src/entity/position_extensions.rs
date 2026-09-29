// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/PositionExtensions.cs
//! Port of `Source/ACE.Server/Entity/PositionExtensions.cs`.
//!
//! The pure members are ported and replayed against `fixtures/vectors/position/`. The terrain
//! queries (`FindZ`, `GetTerrainZ`, `IsWalkable`, `IsRestrictable`) take the `World` and read the
//! shared physics crate's land cells and terrain polygons; `GetCell` and
//! `GetIndoorCell` read its building and env cells (`AdjustCell`), and `AdjustMapCoords`
//! the terrain and the building's environment geometry.

use empyrean_common::dotnet::cast::CsCast;
use empyrean_common::dotnet::format::format;
use empyrean_entity::enums::{PositionType, PropertyString};
use empyrean_entity::{LandblockId, Position, Quaternion, Vector2, Vector3};

use dereth_physics::geom::polygon::Polygon;
use dereth_physics::PlaneExt;
use dereth_primitives::{CellId, Vec3};
use empyrean_dat::file_types::{EnvCell, Environment, LandblockInfo};
use empyrean_tables::house_cell::HOUSE_CELLS;

use crate::entity::landblock::Landblock;
use crate::physics::phys_ext;
use crate::world_objects::world_object::WorldObject;
use crate::World;

// ACE: PositionExtensions.ToGlobal
#[must_use]
pub fn to_global(p: &Position, skip_indoors: bool) -> Vector3 {
    // TODO: Is this necessary? It seemed to be loading rogue physics landblocks. Commented out 2019-04 Mag-nus
    //var landblock = LScape.get_landblock(p.LandblockId.Raw);

    // TODO: investigate dungeons that are below actual traversable overworld terrain
    // ex., 010AFFFF
    //if (landblock.IsDungeon)
    if p.indoors() && skip_indoors {
        return p.pos();
    }

    // `byte * int + float`: the integer product converts to float before the add.
    let landblock_id = p.landblock_id();
    let x = (i32::from(landblock_id.landblock_x()) * Position::BLOCK_LENGTH) as f32 + p.position_x;
    let y = (i32::from(landblock_id.landblock_y()) * Position::BLOCK_LENGTH) as f32 + p.position_y;
    let z = p.position_z;

    Vector3::new(x, y, z)
}

// ACE: PositionExtensions.FromGlobal
#[must_use]
pub fn from_global(w: &World, p: &Position, pos: Vector3) -> Position {
    // TODO: investigate dungeons that are below actual traversable overworld terrain
    if p.indoors() {
        let mut i_pos = Position::new();
        i_pos.set_landblock_id(p.landblock_id());
        i_pos.set_pos(Vector3::new(pos.x, pos.y, pos.z));
        i_pos.set_rotation(p.rotation());
        i_pos.set_landblock_id(LandblockId::new(get_cell(w, &i_pos)));
        return i_pos;
    }

    // `(uint)pos.X / Position.BlockLength`: a saturating float cast, then a long division.
    let block_x: u32 =
        (i64::from(CsCast::<u32>::cs_cast(pos.x)) / i64::from(Position::BLOCK_LENGTH)).cs_cast();
    let block_y: u32 =
        (i64::from(CsCast::<u32>::cs_cast(pos.y)) / i64::from(Position::BLOCK_LENGTH)).cs_cast();

    let local_x = pos.x % Position::BLOCK_LENGTH as f32;
    let local_y = pos.y % Position::BLOCK_LENGTH as f32;

    // ACE computes `landblockID = blockX << 24 | blockY << 16 | 0xFFFF` here and never uses it.

    let mut position = Position::new();
    position.set_landblock_id(LandblockId::from_xy(block_x.cs_cast(), block_y.cs_cast()));
    position.position_x = local_x;
    position.position_y = local_y;
    position.position_z = pos.z;
    position.set_rotation(p.rotation());
    position.set_landblock_id(LandblockId::new(get_cell(w, &position)));
    position
}

/// Gets the cell ID for a position within a landblock: indoors through the dungeon's env cells
/// (`GetIndoorCell`); outdoors the land cell, or the building cell or underground env cell that
/// holds the point. ACE's `try`/`catch` (answering 0) guards nothing these reads can throw.
// ACE: PositionExtensions.GetCell
#[must_use]
pub fn get_cell(w: &World, p: &Position) -> u32 {
    //var landblock = LScape.get_landblock(p.LandblockId.Raw);

    // dungeons
    // TODO: investigate dungeons that are below actual traversable overworld terrain
    // ex., 010AFFFF
    //if (landblock.IsDungeon)
    if p.indoors() {
        return get_indoor_cell(w, p);
    }

    // outside - could be on landscape, in building, or underground cave
    let cell_id = get_outdoor_cell(p);
    let Some(landcell) = phys_ext::get_landcell(w, cell_id) else {
        return cell_id;
    };

    // `landcell.has_building()`
    if w.physics.land().building(landcell).is_some() {
        let env_cells = get_building_cells(w, landcell);
        for env_cell in env_cells {
            if env_cell_point_in_cell(w, env_cell, p.pos()) {
                return env_cell.0;
            }
        }
    }

    // handle underground areas ie. caves
    // get the terrain Z-height for this X/Y
    if let Some(walkable) = find_terrain_poly(w, landcell, p.pos()) {
        let mut terrain_pos = Vec3::new(p.position_x, p.position_y, p.position_z);
        walkable.plane.set_height(&mut terrain_pos);

        // are we below ground? if so, search all of the indoor cells for this landblock
        if terrain_pos.z > p.pos().z {
            let env_cells = landblock_get_envcells(w, p.landblock_id().raw());
            for env_cell in env_cells {
                if env_cell_point_in_cell(w, env_cell, p.pos()) {
                    return env_cell.0;
                }
            }
        }
    }

    cell_id
}

/// Gets an outdoor cell ID for a position within a landblock.
// ACE: PositionExtensions.GetOutdoorCell
#[must_use]
pub fn get_outdoor_cell(p: &Position) -> u32 {
    // uint / int promotes both sides to long in C#.
    let cell_x = i64::from(CsCast::<u32>::cs_cast(p.position_x)) / i64::from(Position::CELL_LENGTH);
    let cell_y = i64::from(CsCast::<u32>::cs_cast(p.position_y)) / i64::from(Position::CELL_LENGTH);

    let cell_id = cell_x * i64::from(Position::CELL_SIDE) + cell_y + 1;

    (i64::from(p.landblock_id().raw() & 0xFFFF_0000) | cell_id).cs_cast()
}

/// Gets an indoor cell ID for a position within a dungeon.
// ACE: PositionExtensions.GetIndoorCell
#[must_use]
pub fn get_indoor_cell(w: &World, p: &Position) -> u32 {
    let adjust_cell = adjust_cell_get(w, p.landblock());
    let env_cell = adjust_cell_get_cell(w, &adjust_cell, p.pos());
    env_cell.unwrap_or_else(|| p.cell())
}

// ---- the shared physics members these read (ACE's Physics/, dereth-physics' land source) ----------

/// `AdjustCell.Get(dungeonID).EnvCells` (`Physics/Util/AdjustCell.cs`): the env cells
/// `0x100..0x100 + LandblockInfo.NumCells` of the landblock that `LScape.get_landcell` finds.
// DIVERGE: ACE caches AdjustCell per landblock for the process's life; the list is rebuilt from the physics land on each call (the same cells while the landblock is loaded).
pub(crate) fn adjust_cell_get(w: &World, dungeon_id: u32) -> Vec<CellId> {
    let block_info_id = dungeon_id << 16 | 0xFFFE;
    // `ReadFromDat` answers an empty `LandblockInfo` (no cells) for a missing file
    let num_cells = w
        .dats
        .cell_dat()
        .read_from_dat::<LandblockInfo>(block_info_id)
        .map_or(0, |i| i.num_cells);

    // BuildEnv
    let mut env_cells = Vec::new();
    let first_cell_id = 0x100u32;
    for i in 0..num_cells {
        let cell_id = first_cell_id + i;
        let block_cell = dungeon_id << 16 | cell_id;

        if let Some(env_cell) =
            phys_ext::get_landcell(w, block_cell).filter(|c| phys_ext::is_env_cell(*c))
        {
            env_cells.push(env_cell);
        }
    }
    env_cells
}

/// `AdjustCell.GetCell(point)`: the first env cell that holds the point.
pub(crate) fn adjust_cell_get_cell(w: &World, env_cells: &[CellId], point: Vector3) -> Option<u32> {
    env_cells
        .iter()
        .find(|c| env_cell_point_in_cell(w, **c, point))
        .map(|c| c.0)
}

/// `EnvCell.point_in_cell(point)`: the point in the cell's frame, tested against its cell BSP
/// (`CellStruct.point_in_cell`). A cell with no cell BSP answers false (ACE would dereference
/// null).
fn env_cell_point_in_cell(w: &World, cell: CellId, point: Vector3) -> bool {
    let Some(geom) = w.physics.land().env_cell(cell) else {
        return false;
    };
    let local_point =
        dereth_physics::math::globaltolocal(&geom.frame, Vec3::new(point.x, point.y, point.z));
    geom.cell_bsp
        .as_ref()
        .is_some_and(|b| b.point_inside_cell_bsp(local_point))
}

/// `landcell.Building.get_building_cells()` (`BuildingObj`): the building's entry cells (behind
/// its portals), each followed recursively by the cells it can see.
fn get_building_cells(w: &World, landcell: CellId) -> Vec<CellId> {
    fn add_cells_recursive(w: &World, cell: CellId, building_cells: &mut Vec<CellId>) {
        if building_cells.contains(&cell) {
            return;
        }
        let Some(geom) = w.physics.land().env_cell(cell) else {
            return;
        };

        building_cells.push(cell);

        for visible_cell in &geom.stab_list {
            add_cells_recursive(w, *visible_cell, building_cells);
        }
    }

    let mut building_cells = Vec::new();

    // entry points into the building,
    // aka cells touching the outdoor landblock
    for entrypoint in w.physics.land().building_cells(landcell) {
        add_cells_recursive(w, entrypoint, &mut building_cells);
    }
    building_cells
}

/// `LScape.get_landblock(id).get_envcells()` (`Physics/Common/Landblock.cs`): the landblock's env
/// cells in order, stopping at the first one that is not loaded.
fn landblock_get_envcells(w: &World, landblock_id: u32) -> Vec<CellId> {
    let mut env_cells = Vec::new();
    let Some(info) = w
        .dats
        .cell_dat()
        .read_from_dat::<LandblockInfo>(landblock_id | 0xFFFE)
    else {
        return env_cells;
    };

    let start_cell = landblock_id & 0xFFFF_0000 | 0x100;
    for cell_id in start_cell..start_cell + info.num_cells {
        match phys_ext::get_landcell(w, cell_id) {
            Some(env_cell) => env_cells.push(env_cell),
            None => break,
        }
    }
    env_cells
}

/// Returns the greatest single-dimension square distance between 2 positions.
// ACE: PositionExtensions.CellDist
// ACE-BUG: the outdoor branch returns Max(p1.GlobalCellX, p1.GlobalCellY), p1's own global cell coordinates rather than a distance to p2; the other branch moves an indoor p2 to its indoor (not outdoor) cell and returns Max(_p1.GlobalCellX, _p2.GlobalCellY), again no difference.
#[must_use]
pub fn cell_dist(w: &World, p1: &Position, p2: &Position) -> u32 {
    if !p1.indoors() && !p2.indoors() {
        return p1.global_cell_x().max(p1.global_cell_y());
    }

    // handle dungeons (commented out in ACE)

    let mut p1_copy = Position::from_position(p1);
    let mut p2_copy = Position::from_position(p2);

    if p1_copy.indoors() {
        p1_copy.set_landblock_id(LandblockId::new(get_outdoor_cell(&p1_copy)));
    }
    if p2_copy.indoors() {
        p2_copy.set_landblock_id(LandblockId::new(get_indoor_cell(w, &p2_copy)));
    }

    p1_copy.global_cell_x().max(p2_copy.global_cell_y())
}

// ACE: PositionExtensions.GetDungeonCellDist
#[must_use]
pub fn get_dungeon_cell_dist(_p1: &Position, _p2: &Position) -> u32 {
    // not implemented yet
    u32::MAX
}

// ACE: PositionExtensions.GetMapCoords
#[must_use]
pub fn get_map_coords(pos: &Position) -> Option<Vector2> {
    // no map coords available for dungeons / indoors?
    if (pos.cell() & 0xFFFF) >= 0x100 {
        return None;
    }

    let global_pos = to_global(pos, false);

    // 1 landblock = 192 meters
    // 1 landblock = 0.8 map units

    // 1 map unit = 1.25 landblocks
    // 1 map unit = 240 meters

    let map_coords = Vector2::new(global_pos.x / 240.0, global_pos.y / 240.0);

    // dereth is 204 map units across, -102 to +102
    let offset = Vector2::ONE * 102.0;

    Some(Vector2::new(
        map_coords.x - offset.x,
        map_coords.y - offset.y,
    ))
}

// ACE: PositionExtensions.GetMapCoordStr
#[must_use]
pub fn get_map_coord_str(pos: &Position) -> Option<String> {
    let map_coords = get_map_coords(pos)?;

    let north_south = if map_coords.y >= 0.0 { "N" } else { "S" };
    let east_west = if map_coords.x >= 0.0 { "E" } else { "W" };

    Some(
        format(map_coords.y.abs() - 0.05f32, "0.0")
            + north_south
            + ", "
            + &format(map_coords.x.abs() - 0.05f32, "0.0")
            + east_west,
    )
}

/// Puts a position built from map coordinates on the ground: Z at the terrain height, then, in a
/// land cell with a building, raised by the building's lowest vertex (when that is above 0) and
/// moved into the cell that holds it (a building's interior cell, or the land cell).
///
/// # Errors
/// ACE's exception, as its text: a position in landblock row or column 0xFF, whose
/// `LandblockManager.GetLandblock` indexes past its 255 x 255 array
/// (`IndexOutOfRangeException`). The position is left as it is.
// ACE: PositionExtensions.AdjustMapCoords
pub fn adjust_map_coords(w: &mut World, pos: &mut Position) -> Result<(), String> {
    // adjust Z to terrain height
    // (`GetTerrainZ` starts with `LScape.get_landblock(p.LandblockId.Raw)`, which on the server
    // loads the landblock.)
    let lbid = LandblockId::new(pos.landblock_id().raw() | 0xFFFF);
    if lbid.landblock_x() == 0xFF || lbid.landblock_y() == 0xFF {
        return Err(format!(
            "System.IndexOutOfRangeException: landblocks[0x{:02X}, 0x{:02X}]",
            lbid.landblock_x(),
            lbid.landblock_y()
        ));
    }
    crate::managers::landblock_manager::get_landblock(w, lbid, false, false);
    pos.position_z = get_terrain_z(w, pos);

    // adjust to building height, if applicable
    // `LScape.get_landcell(pos.Cell) as SortCell`: an interior cell is not a SortCell.
    let sort_cell =
        phys_ext::get_landcell_loading(w, pos.cell()).filter(|c| !phys_ext::is_env_cell(*c));
    if let Some(sort_cell) = sort_cell.filter(|c| w.physics.land().building(*c).is_some()) {
        let min_z = building_get_min_z(w, sort_cell);

        if min_z > 0.0 && min_z < f32::MAX {
            pos.position_z += min_z;
        }

        let cell = get_cell(w, pos);
        pos.set_landblock_id(LandblockId::new(cell));
    }
    Ok(())
}

/// `BuildingObj.GetMinZ()`: the lowest vertex Z, in its environment's own frame, over every cell
/// structure of the environment of each of the building's cells (`float.MaxValue` when none).
// ACE: Physics.Common.BuildingObj.GetMinZ
fn building_get_min_z(w: &World, landcell: CellId) -> f32 {
    let mut min_z = f32::MAX;

    for building_cell in get_building_cells(w, landcell) {
        // `buildingCell.Environment`: the env cell's `0x0D` environment from the portal dat
        let Some(env_cell) = w.dats.cell_dat().read_from_dat::<EnvCell>(building_cell.0) else {
            continue;
        };
        let Some(environment) = w
            .dats
            .portal_dat()
            .read_from_dat::<Environment>(env_cell.environment.0)
        else {
            continue;
        };

        for cell_struct in &environment.cells {
            for vertex in &cell_struct.vertex_array.vertices {
                if vertex.position.z < min_z {
                    min_z = vertex.position.z;
                }
            }
        }
    }
    min_z
}

// ACE: PositionExtensions.Translate
pub fn translate(pos: &mut Position, block_cell: u32) {
    let new_block_x = block_cell >> 24;
    let new_block_y = (block_cell >> 16) & 0xFF;

    // `(int)newBlockX - pos.LandblockX` is int - uint, which C# evaluates as long.
    let x_diff = i64::from(new_block_x as i32) - i64::from(pos.landblock_x());
    let y_diff = i64::from(new_block_y as i32) - i64::from(pos.landblock_y());

    //pos.Origin.X -= xDiff * 192;
    pos.position_x -= (x_diff * 192) as f32;
    //pos.Origin.Y -= yDiff * 192;
    pos.position_y -= (y_diff * 192) as f32;

    //pos.ObjCellID = blockCell;
    pos.set_landblock_id(LandblockId::new(block_cell));
}

// ACE: PositionExtensions.FindZ
/// A cell missing from the cell dat reads as ACE's empty `EnvCell` (`ReadFromDat` returns
/// `new T()`), whose frame origin is zero.
pub fn find_z(w: &World, pos: &mut Position) {
    let env_cell = w.dats.cell_dat().read_from_dat::<EnvCell>(pos.cell());
    pos.position_z = env_cell.map_or(0.0, |c| c.frame.origin.z);
}

/// The terrain height under an outdoor position (its outdoor cell's walkable terrain polygon),
/// or the position's own Z when there is no land cell or no polygon under it.
// ACE: PositionExtensions.GetTerrainZ
#[must_use]
pub fn get_terrain_z(w: &World, p: &Position) -> f32 {
    // `var landblock = LScape.get_landblock(p.LandblockId.Raw);` is never read; in ACE it only
    // loads the landblock, and our physics land answers for any landblock it has (V1).

    let cell_id = get_outdoor_cell(p);
    let Some(landcell) = phys_ext::get_landcell(w, cell_id) else {
        return p.pos().z;
    };

    let Some(walkable) = find_terrain_poly(w, landcell, p.pos()) else {
        return p.pos().z;
    };

    let mut terrain_pos = Vec3::new(p.position_x, p.position_y, p.position_z);
    walkable.plane.set_height(&mut terrain_pos);

    terrain_pos.z
}

/// Returns TRUE if outdoor position is located on walkable slope.
// ACE: PositionExtensions.IsWalkable
#[must_use]
pub fn is_walkable(w: &World, p: &Position) -> bool {
    if p.indoors() {
        return true;
    }

    // `(LandCell)LScape.get_landcell(p.Cell)`: ACE dereferences it without a check. It is null only
    // for an outdoor cell index of 0 or 0x41..0xFF, which neither the world database (generator
    // `obj_Cell_Id`, `landblock_instance` after `Position`'s constructor resolves index 0) nor
    // `Position.SetLandCell` produce; our physics land also has no block where ACE would load one.
    let Some(landcell) = phys_ext::get_landcell(w, p.cell()) else {
        return false;
    };

    let Some(walkable) = find_terrain_poly(w, landcell, p.pos()) else {
        return false;
    };

    dereth_physics::is_valid_walkable(walkable.plane.normal)
}

/// `landcell.find_terrain_poly(origin, ref walkable)`: the first of the land cell's two terrain
/// triangles that holds the point in 2-D (the shared crate's `LandblockCollision.find_terrain_poly`,
/// the same `point_in_poly2D(origin, Sidedness.Positive)` test over the same two polygons).
fn find_terrain_poly(w: &World, landcell: CellId, origin: Vector3) -> Option<Polygon> {
    let block = w.physics.land().landblock(landcell.landblock())?;
    block
        .find_terrain_poly(landcell.index(), Vec3::new(origin.x, origin.y, origin.z))
        .cloned()
}

/// Returns TRUE if current cell is a House cell.
// ACE: PositionExtensions.IsRestrictable
#[must_use]
pub fn is_restrictable(p: &Position, landblock: &mut Landblock) -> bool {
    let cell = if landblock.is_dungeon() {
        p.cell()
    } else {
        get_outdoor_cell(p)
    };

    HOUSE_CELLS.contains_key(&cell)
}

/// The physics engine's position (the shared crate's) as an ACE `Position`:
/// `new Position(pos.ObjCellID, pos.Frame.Origin, pos.Frame.Orientation)`.
// ACE: PositionExtensions.ACEPosition
#[must_use]
pub fn ace_position(pos: &dereth_primitives::Position) -> Position {
    let r = pos.frame.rotation;
    Position::from_vectors(
        pos.cell.0,
        Vector3::new(pos.frame.origin.x, pos.frame.origin.y, pos.frame.origin.z),
        Quaternion::new(r.x, r.y, r.z, r.w),
    )
}

/// An ACE `Position` as the physics engine's: `new Physics.Common.Position(pos.Cell, new
/// AFrame(pos.Pos, pos.Rotation))`.
// ACE: PositionExtensions.PhysPosition
#[must_use]
pub fn phys_position(pos: &Position) -> dereth_primitives::Position {
    phys_ext::to_physics_position(pos)
}

/// differs from ac physics engine
// ACE: PositionExtensions.RotationEpsilon
pub const ROTATION_EPSILON: f32 = 0.0001;

// ACE: PositionExtensions.IsRotationValid
#[must_use]
pub fn is_rotation_valid(q: Quaternion) -> bool {
    // `==` on Quaternion is IEEE equality per lane.
    if q == Quaternion::IDENTITY {
        return true;
    }

    if q.x.is_nan() || q.y.is_nan() || q.z.is_nan() || q.w.is_nan() {
        return false;
    }

    let length = q.length();
    if length.is_nan() {
        return false;
    }

    if (1.0f32 - length).abs() > ROTATION_EPSILON {
        return false;
    }

    true
}

/// Called by `WorldObject.GetPosition` once it has found `pos.Rotation` invalid.
// ACE: PositionExtensions.AttemptToFixRotation
// DIVERGE: the warning names the object by its PropertyString.Name; ACE prints the virtual Name (a Player's carries a `+` prefix), which needs the World, and this runs inside a `&self` property read.
pub fn attempt_to_fix_rotation(
    pos: &mut Position,
    wo: &WorldObject,
    position_type: PositionType,
) -> bool {
    log::warn!(
        "detected bad quaternion x y z w for {} (0x{}) | WCID: {} | WeenieType: {:?} | PositionType: {:?}",
        wo.get_property(PropertyString::Name).unwrap_or_default(),
        wo.guid,
        wo.biota.weenie_class_id,
        wo.biota.weenie_type,
        position_type
    );
    log::warn!("before fix: {}", pos.to_loc_string());

    let normalized = Quaternion::normalize(pos.rotation());

    let success = is_rotation_valid(normalized);

    if success {
        pos.set_rotation(normalized);
    }

    log::warn!(" after fix: {}", pos.to_loc_string());

    success
}
