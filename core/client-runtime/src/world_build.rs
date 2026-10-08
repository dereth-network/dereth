//! Building the world with no device: the [`WorldState`] a scene starts from, each landblock's
//! simulation content, and the physics registration of that content.
//!
//! Every presentation builds its world through here. A presentation that draws does its GPU work
//! beside these calls: it bakes the same placements [`land_content`] returns into batches and
//! pairs its particle emitters with the bodies [`init_cell_statics`] hands back. A presentation
//! that draws nothing calls them and nothing else. Either way the placements, the collision
//! records and the order they are registered in come from one place.
//!
//! **One landblock's simulation content** is [`BlockStatics`]: the static objects of its interior
//! cells, the scenery and landblock-information statics of its outdoor land cells, and the house
//! restrictions of both. Each presentation keeps one per resident block, alongside whatever it
//! draws the block with, because the block's lifetime (re-mesh, release) is the presentation's.
//!
//! [`WorldState`]: crate::world_state::WorldState
//! [`land_content`]: crate::world_build::land_content
//! [`init_cell_statics`]: crate::world_build::init_cell_statics
//! [`BlockStatics`]: crate::world_build::BlockStatics

use std::collections::BTreeMap;

use dereth_assets::world::{CellLandblock, LandblockInfo, Scene};
use dereth_assets::{Decode, Region};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::obj::PhysicsState;
use dereth_physics::PhysHandle;
use dereth_primitives::{CellId, DataId, ObjectId, Vec3};
use dereth_terrain::buildings::{Building, SortCells};
use dereth_terrain::land::mesh::LandblockMesh;
use dereth_terrain::scenery::{
    generate_scenery, outside_cell_index, PlacedScenery, SceneryEnv, WithinBlockShape,
};

use crate::camera::FreeCamera;
use crate::environment::EnvironmentOverrideState;
use crate::scene::SceneConfig;
use crate::world_state::WorldState;
use {dereth_world_data::env_cells::CellStatic, dereth_world_data::env_cells::DecodedCell};
use {dereth_world_data::landblock::landblock_did, dereth_world_data::landblock::lbi_did};

impl WorldState {
    /// The world as a scene starts it: no body, no objects, nothing resident yet, the calendar
    /// set from the configuration and the camera at the origin until the centre block's height
    /// is known.
    ///
    /// The clock is set before anything is built, because the landscape's vertex lighting is
    /// baked from the sky's light at the current time of day and every block generated after
    /// this reads it.
    #[must_use]
    pub fn new(region: &Region, cfg: &SceneConfig) -> Self {
        let mut clock = crate::game_clock::GameClock::new(region);
        // `game_time` first: it fixes the whole calendar, and the calendar is what the
        // present-day-group calculation hashes into a day group, so it decides the colour ramp
        // as well as the hour. `time_of_day` moves the hour alone and leaves the date at
        // whatever a zero current-time origin gives.
        if let Some(s) = cfg.game_time {
            clock.set_game_time(0.0, s);
        } else if let Some(f) = cfg.time_of_day {
            clock.set_time_of_day(0.0, f);
        } else {
            clock.use_time(0.0);
        }
        Self {
            // Replaced once the centre block's height is known.
            camera: FreeCamera::new(Vec3::ZERO, 0.0, -0.42),
            streamer: crate::world_stream::WorldStreamer::new(cfg.land_radius),
            released_interiors: Vec::new(),
            entering_world: Vec::new(),
            sweeps_seen: None,
            character: None,
            cell_static_objects: dereth_world_data::env_cells::CellStaticObjects::new(),
            restricted_cells: 0,
            objects: BTreeMap::new(),
            pending_sound: Vec::new(),
            pending_restriction_effects: Vec::new(),
            last_object_time: 0.0,
            last_bodyless_tick: 0.0,
            // Nothing has been dispatched yet, so the server holds no control.
            player_movement_applied: false,
            player_object: None,
            character_state: PhysicsState::DEFAULT,
            character_sound_table: None,
            camera_translucency: 0.0,
            anim_assets: None,
            clock,
            next_tick: 0.0,
            next_light_tick: 0.0,
            environment_override: EnvironmentOverrideState::default(),
            // Off by default in retail.
            always_daylight: false,
            // Behind and a little above, looking slightly down: the client camera's own
            // defaults are explicitly not reproduced here (see [`crate::camera`]).
            camera_orbit: (0.0, -0.20),
            camera_distance: 4.5,
        }
    }
}

/// The landblock record at block coordinates `(bx, by)`, or `None` outside the world or when the
/// cell file does not have it.
#[must_use]
pub fn read_landblock(store: &RetailDatStore, bx: i32, by: i32) -> Option<CellLandblock> {
    let id = landblock_did(block_id(bx, by)?);
    let bytes = store.read_typed(DbType::LandBlock, id).ok()?;
    CellLandblock::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// The landblock-information record at block coordinates `(bx, by)`: the block's buildings,
/// statics and restriction table, when it has any.
#[must_use]
pub fn read_lbi(store: &RetailDatStore, bx: i32, by: i32) -> Option<LandblockInfo> {
    let id = lbi_did(block_id(bx, by)?);
    let bytes = store.read_typed(DbType::Lbi, id).ok()?;
    LandblockInfo::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// A scenery-selection record.
#[must_use]
pub fn read_scene(store: &RetailDatStore, id: DataId) -> Option<Scene> {
    let bytes = store.read_typed(DbType::Scene, id).ok()?;
    Scene::decode_payload_in(store.era_of(id), id, &bytes).ok()
}

/// The 16-bit landblock id of `(bx, by)`, when both are inside the world.
fn block_id(bx: i32, by: i32) -> Option<u16> {
    let x = u16::try_from(bx).ok().filter(|&x| x <= 0xFE)?;
    let y = u16::try_from(by).ok().filter(|&y| y <= 0xFE)?;
    Some((x << 8) | y)
}

/// One landblock's outdoor contents: what static-object initialization places in its land cells.
#[derive(Debug, Default, Clone)]
pub struct LandContent {
    /// The landblock-information record, when the block has one.
    pub lbi: Option<LandblockInfo>,
    /// Whether objects of any kind are placed: at full detail, or always when the level-of-detail
    /// guard is off.
    pub full_detail: bool,
    /// The generated scenery, in placement order. Each one's collision record is
    /// `land_statics[i]` for its index `i` here.
    pub placed: Vec<PlacedScenery>,
    /// For each of the landblock-information record's statics, in record order (and only when
    /// objects are placed), the land cell it resolved to and the index of its collision record in
    /// `land_statics`; `None` for one whose cell lookup found no cell of this block.
    pub object_slots: Vec<Option<(CellId, usize)>>,
    /// Every outdoor collision record: the scenery first, then the resolved statics.
    pub land_statics: Vec<CellStatic>,
    /// The land cells a house fences, from the block's restriction table.
    pub cell_restrictions: Vec<(CellId, ObjectId)>,
}

/// Place one landblock's outdoor contents: the scenery generated over its mesh (never in a cell
/// that owns a building), and the landblock-information statics resolved to the land cell their
/// origin falls in.
///
/// **Objects of every kind need full detail.** Static-object, building and scenery
/// initialization all refuse unless `side_cell_count == 8`, so a block on an outer ring is bare
/// terrain; `lod_object_guard` off lifts that for every population alike.
///
/// Statics are resolved as static-object initialization resolves them: the cell is derived from
/// the block and the origin, and a placement whose origin lands in a *neighbouring* block is
/// dropped rather than registered next door, because the cell lookup only answers a cell whose
/// id matches exactly. Such a placement has no collision record.
#[must_use]
pub fn land_content(
    store: &RetailDatStore,
    region: &Region,
    lb: &CellLandblock,
    mesh: &LandblockMesh,
    bx: i32,
    by: i32,
    lod_object_guard: bool,
) -> LandContent {
    let lbi = read_lbi(store, bx, by);
    let block = block_id(bx, by).unwrap_or(0);
    let full_detail = !lod_object_guard || mesh.side_cell_count == 8;
    let mut cells = SortCells::new();
    if let Some(info) = &lbi {
        if full_detail {
            for b in &info.buildings {
                let cell = outside_cell_index(b.frame.origin.x, b.frame.origin.y);
                // Building registration adds each building to exactly one land cell -- the
                // one its origin is in.
                cells.add_building(
                    cell,
                    Building {
                        id: b.id,
                        frame: b.frame,
                    },
                );
            }
        }
    }
    let placed = {
        // A block places the same few objects many times, so each id's shape is read once.
        let shapes: std::cell::RefCell<BTreeMap<u32, Option<WithinBlockShape>>> =
            std::cell::RefCell::new(BTreeMap::new());
        let env = SceneryEnv {
            scenes: &|id| read_scene(store, id),
            has_building: &|cell| cells.has_building(cell),
            // The object's cylinders, spheres, sorting sphere and physics mesh -- the last
            // scenery filter's whole input.
            shape: &|id| {
                shapes
                    .borrow_mut()
                    .entry(id.0)
                    .or_insert_with(|| crate::models::within_block_shape(store, id))
                    .clone()
            },
        };
        generate_scenery(lb, mesh, region, bx, by, &env)
    };
    // Every generated tree, rock and bush is a physics body in its land cell, which is the list
    // collision traversal walks; there is no solidity flag, the setup decides.
    let mut land_statics: Vec<CellStatic> = placed
        .iter()
        .map(|p| CellStatic {
            cell: p.cell,
            id: p.gfxobj,
            frame: p.frame,
            scale: p.scale,
        })
        .collect();
    let mut object_slots = Vec::new();
    let mut cell_restrictions = Vec::new();
    if let Some(info) = lbi.as_ref().filter(|_| full_detail) {
        for o in &info.objects {
            let mut cell = CellId((u32::from(block) << 16) | 1);
            let mut origin = o.frame.origin;
            let placed_cell = dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin)
                .then_some(cell)
                .filter(|c| (c.0 >> 16) == u32::from(block));
            object_slots.push(placed_cell.map(|c| {
                // Inserted at the ORIGINAL frame, with no scale set.
                land_statics.push(CellStatic {
                    cell: c,
                    id: o.id,
                    frame: o.frame,
                    scale: 1.0,
                });
                (c, land_statics.len() - 1)
            }));
        }
        // Each land cell's restriction object is looked up by the cell's id and stored when it
        // is non-zero; zero is skipped rather than stored.
        if let Some(t) = info.restrictions.as_ref() {
            for &(cell, iid) in &t.entries {
                if iid != 0 && (cell >> 16) == u32::from(block) {
                    cell_restrictions.push((CellId(cell), ObjectId(iid)));
                }
            }
        }
    }
    LandContent {
        lbi,
        full_detail,
        placed,
        object_slots,
        land_statics,
        cell_restrictions,
    }
}

/// One landblock's interior contents, from its decoded cells in load order: the house
/// restriction each cell record names, and each cell's static objects when `want_statics`.
///
/// A cell's statics are `per_cell[k]` for the `k`th decoded cell; concatenated in that order they
/// are the block's interior collision records, so the `i`th static of cell `k` is record
/// `per_cell[..k]`'s total length plus `i`.
#[must_use]
pub fn interior_content(
    decoded: &[DecodedCell],
    want_statics: bool,
) -> (Vec<(CellId, ObjectId)>, Vec<Vec<CellStatic>>) {
    // The interior half of the house barrier: cell loading reads a restriction object id
    // straight out of the cell record and stores it in the cell's own restriction field. It names
    // the *house* object, not the slumlord.
    let restrictions = decoded
        .iter()
        .filter_map(|d| {
            d.cell
                .restriction_obj
                .filter(|o| *o != 0)
                .map(|o| (d.id, ObjectId(o)))
        })
        .collect();
    let per_cell = decoded
        .iter()
        .map(|d| {
            if want_statics {
                dereth_world_data::env_cells::cell_statics(d)
            } else {
                Vec::new()
            }
        })
        .collect();
    (restrictions, per_cell)
}

/// One landblock's simulation content, and whether each half of it has been registered with
/// physics yet.
#[derive(Debug, Default, Clone)]
pub struct BlockStatics {
    /// Every static object of the block's interior cells.
    pub cell_statics: Vec<CellStatic>,
    /// The block's outdoor land cells' scenery and landblock-information statics, each with the
    /// land cell it resolved to.
    pub land_statics: Vec<CellStatic>,
    /// The house restriction for every cell of this block that has one: land cells from the
    /// landblock restriction table, interior cells from their records.
    pub cell_restrictions: Vec<(CellId, ObjectId)>,
    /// Whether [`init_cell_statics`] has given those placements bodies. A re-mesh keeps the
    /// placements, and registering them twice would stand two chairs in every chair.
    pub statics_registered: bool,
    /// Whether [`init_cell_restrictions`] has written the restrictions into their cells.
    pub restrictions_registered: bool,
}

/// The bodies one block's statics were given, by index into its two lists; `None` for a
/// placement that got no body.
#[derive(Debug, Default, Clone)]
pub struct StaticBodies {
    pub cell: Vec<Option<PhysHandle>>,
    pub land: Vec<Option<PhysHandle>>,
}

/// Give every resident block's not-yet-registered statics physics bodies, interior cells first
/// and then the land cells, in the order `blocks` yields them. Returns, for each block that was
/// registered, its position in that order and the bodies it was given, so that a caller holding
/// anything paired with those placements (a particle emitter on a torch) can attach it.
///
/// Nothing happens while `cell_statics` is off in the configuration, or while there is no body
/// (the physics world is the body's).
pub fn init_cell_statics<'a>(
    ws: &mut WorldState,
    store: &RetailDatStore,
    cfg: &SceneConfig,
    blocks: impl IntoIterator<Item = &'a mut BlockStatics>,
) -> Vec<(usize, StaticBodies)> {
    let mut out = Vec::new();
    if !cfg.cell_statics {
        return out;
    }
    let before = ws.cell_static_objects.len();
    // Set here rather than at construction because the configuration arrives after; it never
    // changes during a run.
    ws.cell_static_objects.mesh_collision = cfg.mesh_collision;
    {
        let WorldState {
            character,
            cell_static_objects,
            ..
        } = ws;
        let Some(c) = character.as_mut() else {
            return out;
        };
        for (position, block) in blocks.into_iter().enumerate() {
            // Both populations, both halves of this guard: a block with no interior cells but a
            // hundred trees must not be skipped by `cell_statics.is_empty()` alone.
            if block.statics_registered
                || (block.cell_statics.is_empty() && block.land_statics.is_empty())
            {
                continue;
            }
            block.statics_registered = true;
            let cell = cell_static_objects.init(store, &mut c.world, &block.cell_statics);
            // Interior cells first, then landblock statics, so that a cell holding both keeps
            // the interior order visible-cell registration produces; the sweep is
            // order-independent but the counters are not.
            let land = cell_static_objects.init(store, &mut c.world, &block.land_statics);
            out.push((position, StaticBodies { cell, land }));
        }
    }
    // Printed on a change, because "the world looks right and nothing collides" is exactly the
    // failure this line exposes.
    let s = ws.cell_static_objects.stats;
    if ws.cell_static_objects.len() != before {
        tracing::debug!(
            "{} cell static(s) solid of {} placement(s) -- {} intangible \
             (no geometry at all), {} on the part-BSP arm and {} on the \
             cylsphere arm, {} simple setups (a GfxObj id: HAS_PHYSICS_BSP_PS), \
             {} undecodable, {} in a cell physics has not been \
             given",
            s.created - s.intangible,
            s.placements,
            s.intangible,
            s.bsp_arm,
            s.cylsphere_arm,
            s.simple_setup,
            s.undecodable,
            s.unplaced
        );
    }
    out
}

/// Write every resident block's not-yet-written house restrictions into their cells, so that the
/// restriction check on a transition finds them. Nothing happens while there is no body.
pub fn init_cell_restrictions<'a>(
    ws: &mut WorldState,
    blocks: impl IntoIterator<Item = &'a mut BlockStatics>,
) {
    let Some(c) = ws.character.as_mut() else {
        return;
    };
    let mut registered = 0usize;
    for block in blocks {
        if block.restrictions_registered {
            continue;
        }
        block.restrictions_registered = true;
        registered += crate::object_step::register_cell_restrictions(c, &block.cell_restrictions);
    }
    if registered != 0 {
        ws.restricted_cells += registered;
        tracing::debug!(
            "{registered} cell(s) fenced by a house restriction object \
             ({} in this scene so far)",
            ws.restricted_cells
        );
    }
}

/// A new body at the middle of the configured landblock, with the profile's camera and
/// mouse-look preferences applied before its first frame.
///
/// # Errors
/// [`CharacterError`](crate::character::CharacterError) when the body's setup, motion table or
/// part array will not load.
pub fn body_for(
    store: &std::sync::Arc<RetailDatStore>,
    region: &Region,
    cfg: &SceneConfig,
) -> Result<crate::character::Character, crate::character::CharacterError> {
    // The middle of the viewer's own block, which is where the free camera already looks.
    let mid = dereth_terrain::consts::BLOCK_LENGTH * 0.5;
    let mut character = crate::character::Character::new(store, region, cfg.landblock, (mid, mid))?;
    // The three camera preference registrations and the camera-set setter's stiffness hand-off,
    // i.e. the profile's `Camera.*` values reaching the camera before its first frame.
    character.camera.apply_preferences(&cfg.camera);
    // The same hand-off for the four `Input.*` mouse-look preferences, one of which decides
    // whether a mouse turn reaches the body.
    character.camera.prefs = cfg.mouse_look;
    Ok(character)
}

/// The sound table installed by the body's own setup record: what a `SoundType` animation hook
/// on the body resolves against. A body rebuilt from the server's setup record gets a different
/// one, so this is read again whenever it is.
#[must_use]
pub fn body_sound_table(c: &crate::character::Character) -> Option<DataId> {
    c.driver()
        .part_array
        .setup
        .as_ref()
        .and_then(|s| s.default_sound_table)
        .or(Some(crate::character::ALUVIAN_MALE_SOUND_TABLE))
}

/// Put `character` in the world: the interior cells of every block already resident are loaded
/// into its land source (the window was built before the body existed, so nothing has prefetched
/// them), and it becomes the world's body. The caller registers the blocks' statics and
/// restrictions next, because those were built with no physics world to add them to.
pub fn place_body(
    ws: &mut WorldState,
    character: crate::character::Character,
    resident: impl IntoIterator<Item = (i32, i32)>,
) {
    for (bx, by) in resident {
        if let Some(id) = block_id(bx, by) {
            character
                .land()
                .load_block_cells(dereth_primitives::LandblockId(id));
        }
    }
    ws.character = Some(character);
}

/// `--start-cell`: stand the body at the configured interior cell's standable point, facing the
/// cell's furthest static object (a dungeon corridor's first standable point is against a wall,
/// and the chase camera sits behind the body). Offline only: a connected run is put where the
/// server says, which teleports over this.
pub fn stand_at_start_cell(ws: &mut WorldState, store: &RetailDatStore, cfg: &SceneConfig) {
    let Some(cell) = cfg.start_cell else {
        return;
    };
    let Some(p) = ws.standable_point(cell) else {
        tracing::warn!(
            "--start-cell {:#010X} is not a resident interior cell with \
             a standable point; the body stays outdoors",
            cell.0
        );
        return;
    };
    // The placements are re-read from the dat rather than taken from the block, so that
    // `--no-cell-statics` looks at the *same* room from the same angle: a control that also
    // turned the camera would not be one.
    let block = u16::try_from(cell.0 >> 16).unwrap_or(0);
    let heading = dereth_world_data::env_cells::EnvCellLoader::new()
        .load_block(store, block)
        .iter()
        .filter(|d| d.id == cell)
        .flat_map(dereth_world_data::env_cells::cell_statics)
        .map(|s| (s.frame.origin.x - p.x, s.frame.origin.y - p.y))
        .max_by(|a, b| {
            dereth_primitives::num::math::hypotf(a.0, a.1)
                .total_cmp(&dereth_primitives::num::math::hypotf(b.0, b.1))
        })
        .map_or(dereth_primitives::Quat::IDENTITY, |(dx, dy)| {
            // Rotating the body's local +y by `t` about z gives `(-sin t, cos t)`, so this is the
            // yaw that points at `(dx, dy)`.
            let t = dereth_primitives::num::math::atan2f(-dx, dy) * 0.5;
            dereth_primitives::Quat::new(
                dereth_primitives::num::math::cosf(t),
                0.0,
                0.0,
                dereth_primitives::num::math::sinf(t),
            )
        });
    if let Some(c) = ws.character.as_mut() {
        c.teleport(dereth_primitives::Position::new(
            cell,
            dereth_primitives::Frame::new(p, heading),
        ));
    }
    tracing::info!(
        "standing in cell {:#010X} at ({:.2}, {:.2}, {:.2})",
        cell.0,
        p.x,
        p.y,
        p.z
    );
}

/// What [`release_block_interiors`] released.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InteriorRelease {
    pub cell_statics_destroyed: u64,
    pub cells_released: u64,
    pub buildings_released: u64,
    pub blocks_released: u64,
}

/// Release the departed blocks' visible cells and their static objects: the middle step of
/// whole-block release, between releasing the block's objects and the block itself.
///
/// It mirrors streaming in: that loads the block's cells into the land source and then gives
/// their statics bodies, and this destroys the bodies and then releases the cells, because a body
/// may not outlive the cell it stands in. Decode memos (the environment and geometry caches)
/// survive, so a re-entered block re-registers rather than re-decoding.
///
/// The object half is queued in [`WorldState::released_interiors`] even with no body, because
/// the object table is the server's; the geometry half needs the body's physics world.
pub fn release_block_interiors(
    ws: &mut WorldState,
    cfg: &SceneConfig,
    departed: &[(i32, i32)],
) -> InteriorRelease {
    let mut out = InteriorRelease::default();
    if departed.is_empty() || !cfg.release_interiors {
        return out;
    }
    ws.released_interiors.extend(
        departed
            .iter()
            .filter_map(|&(bx, by)| block_id(bx, by).map(dereth_primitives::LandblockId)),
    );
    let WorldState {
        character,
        cell_static_objects,
        ..
    } = ws;
    let Some(c) = character.as_mut() else {
        return out;
    };
    let land = std::sync::Arc::clone(c.land());
    for &(bx, by) in departed {
        let Some(block) = block_id(bx, by) else {
            continue;
        };
        // The bodies first: a shadow left in a cell that no longer exists is the one ordering
        // that would leave the table inconsistent.
        out.cell_statics_destroyed += cell_static_objects.release_block(&mut c.world, block);
        let (cells, buildings) = land.release_visible_cells(dereth_primitives::LandblockId(block));
        out.cells_released += cells;
        out.buildings_released += buildings;
        out.blocks_released += 1;
    }
    out
}

/// Place the camera for a scene with no body yet: over the middle of the viewer's block, high
/// enough to see it, looking north and down. Fixed, so a capture of it is a regression test.
///
/// # Errors
/// [`WorldError::NoSuchLandblock`](dereth_world_data::landblock::WorldError::NoSuchLandblock) when no block of the window has a mesh.
pub fn place_load_camera(
    ws: &mut WorldState,
    cfg: &SceneConfig,
) -> Result<(), dereth_world_data::landblock::WorldError> {
    use crate::world_stream::SlotMesh;
    let window = &ws.streamer.window;
    let r = window.mid_radius();
    let w = window.mid_width();
    let ground = window
        .slot(r, r)
        .and_then(|s| s.mesh.as_ref())
        .and_then(SlotMesh::get::<LandblockMesh>)
        .map(|m| m.max_zval)
        .or_else(|| {
            (0..w)
                .flat_map(|xi| (0..w).map(move |yi| (xi, yi)))
                .find_map(|(xi, yi)| {
                    window
                        .slot(xi, yi)
                        .and_then(|s| s.mesh.as_ref())
                        .and_then(SlotMesh::get::<LandblockMesh>)
                        .map(|m| m.max_zval)
                })
        })
        .ok_or(dereth_world_data::landblock::WorldError::NoSuchLandblock(
            cfg.landblock,
        ))?
        - dereth_terrain::consts::MAX_OBJECT_HEIGHT;
    let length = dereth_terrain::consts::BLOCK_LENGTH;
    ws.camera = FreeCamera::new(
        Vec3::new(length * 0.5, -length * 0.35, ground + cfg.camera_height),
        0.0,
        -0.42,
    );
    Ok(())
}

/// What a resident block's next build has to do: everything, or the mesh alone (its ring or
/// stitch direction changed and its objects stay). A full build covers a re-mesh, so `Full` is
/// never downgraded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockWork {
    Full,
    Mesh,
}

/// One resident block of a world with no device.
#[derive(Debug, Default)]
struct ResidentBlock {
    statics: BlockStatics,
    /// Whether the block was built with its objects: a block enters the window at the outermost
    /// ring, normally outside the scenery radius, and only later scrolls into it.
    with_objects: bool,
    /// The mesh's side cell count those objects were placed at; objects of every kind need full
    /// detail, so a change of detail level must not reuse them.
    side_cell_count: u8,
}

/// The resident landblocks of a world with no device, and the work queued for them: the
/// counterpart of a drawing presentation's own block table, holding only what physics needs.
///
/// It follows the streaming window as a drawing presentation does: a slot that arrives is built
/// in full, one whose ring or stitch changed is re-meshed and keeps its objects when its detail
/// level did not change, a block that scrolls into or out of the scenery radius gains or loses
/// its objects, and a block that leaves the window releases its interiors.
#[derive(Debug)]
pub struct BlockResidency {
    region: Region,
    table: [f32; dereth_terrain::consts::LAND_HEIGHT_TABLE_LEN],
    cells: dereth_world_data::env_cells::EnvCellLoader,
    pending: BTreeMap<(i32, i32), BlockWork>,
    blocks: BTreeMap<(i32, i32), ResidentBlock>,
}

impl BlockResidency {
    #[must_use]
    pub fn new(region: Region) -> Self {
        let table = dereth_terrain::land::mesh::height_table(&region);
        Self {
            region,
            table,
            cells: dereth_world_data::env_cells::EnvCellLoader::new(),
            pending: BTreeMap::new(),
            blocks: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn region(&self) -> &Region {
        &self.region
    }

    /// The resident blocks, ascending.
    pub fn resident(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.blocks.keys().copied()
    }

    /// Whether any block is still waiting to be built.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Every resident block's simulation content, in block order.
    pub fn statics_mut(&mut self) -> impl Iterator<Item = &mut BlockStatics> {
        self.blocks.values_mut().map(|b| &mut b.statics)
    }

    fn wants_objects(ws: &WorldState, cfg: &SceneConfig, xi: u32, yi: u32) -> bool {
        ws.streamer
            .wants_objects(xi, yi, cfg.land_radius, cfg.scenery_radius)
    }

    fn want(&mut self, ws: &WorldState, xi: u32, yi: u32, work: BlockWork) {
        let Some(slot) = ws.streamer.window.slot(xi, yi) else {
            return;
        };
        self.pending
            .entry((slot.block_x, slot.block_y))
            .and_modify(|w| {
                if work == BlockWork::Full {
                    *w = BlockWork::Full;
                }
            })
            .or_insert(work);
    }

    /// Take the window's actions from a scroll (or the first fill): queue the builds they ask
    /// for, and release every block the scroll pushed off the edge.
    pub fn queue(
        &mut self,
        ws: &mut WorldState,
        cfg: &SceneConfig,
        actions: &[dereth_landscape::SlotAction],
    ) -> InteriorRelease {
        use dereth_landscape::SlotAction;
        let w = ws.streamer.window.mid_width();
        let mut live = std::collections::BTreeSet::new();
        for (index, action) in actions.iter().enumerate() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            let (xi, yi) = (index / w, index % w);
            let Some(slot) = ws.streamer.window.slot(xi, yi) else {
                continue;
            };
            let block = (slot.block_x, slot.block_y);
            live.insert(block);
            match action {
                SlotAction::Fetched { .. } => self.want(ws, xi, yi, BlockWork::Full),
                SlotAction::Resized { .. } | SlotAction::Restitched { .. } => {
                    self.want(ws, xi, yi, BlockWork::Mesh);
                }
                SlotAction::Unchanged | SlotAction::Released => {}
            }
            // A block that scrolled into the scenery radius grows its objects now, and one that
            // scrolled out of it releases them.
            if let Some(b) = self.blocks.get(&block) {
                if b.with_objects != Self::wants_objects(ws, cfg, xi, yi) {
                    self.want(ws, xi, yi, BlockWork::Full);
                }
            }
        }
        let departed: Vec<(i32, i32)> = self
            .blocks
            .keys()
            .copied()
            .filter(|k| !live.contains(k))
            .collect();
        for k in &departed {
            self.blocks.remove(k);
        }
        release_block_interiors(ws, cfg, &departed)
    }

    /// The window slot currently holding `block`, if it is still in the window.
    fn slot_of_block(ws: &WorldState, (bx, by): (i32, i32)) -> Option<(u32, u32)> {
        let (vx, vy) = ws.streamer.window.viewer_block()?;
        let r = i32::try_from(ws.streamer.window.mid_radius()).ok()?;
        let (xi, yi) = (
            u32::try_from(bx - vx + r).ok()?,
            u32::try_from(by - vy + r).ok()?,
        );
        let slot = ws.streamer.window.slot(xi, yi)?;
        ((slot.block_x, slot.block_y) == (bx, by)).then_some((xi, yi))
    }

    /// Whether a block the player can reach (inside the scenery radius) is still waiting to be
    /// built.
    #[must_use]
    pub fn loading_near_viewer(&self, ws: &WorldState, cfg: &SceneConfig) -> bool {
        self.pending
            .keys()
            .filter_map(|&b| Self::slot_of_block(ws, b))
            .any(|(xi, yi)| Self::wants_objects(ws, cfg, xi, yi))
    }

    /// The landscape half of a teleport: every resident block released regardless of how near
    /// the destination is (a re-centre would keep the blocks the two windows share, so it cannot
    /// stand in for this), the queue dropped, and a window of the same radius scrolled onto the
    /// destination and queued in full.
    pub fn release_for_teleport(
        &mut self,
        ws: &mut WorldState,
        cfg: &SceneConfig,
        destination: dereth_primitives::LandblockId,
    ) -> InteriorRelease {
        let departed: Vec<(i32, i32)> = self.blocks.keys().copied().collect();
        self.pending.clear();
        self.blocks.clear();
        let released = release_block_interiors(ws, cfg, &departed);
        let radius = ws.streamer.window.mid_radius();
        let was = ws.streamer.window.viewer_block();
        ws.streamer.window = dereth_landscape::LandblockWindow::new(radius);
        let viewer = (i32::from(destination.x()), i32::from(destination.y()));
        if let Some(was) = was {
            crate::world_step::rebase_render_space_particles(
                ws,
                (viewer.0 - was.0, viewer.1 - was.1),
            );
        }
        let actions = ws.streamer.window.update_block(viewer);
        self.queue(ws, cfg, &actions);
        released
    }

    /// Build every queued block (mesh, then objects when the block wants them), load the built
    /// blocks' cells into the body's land source, and register their statics and restrictions.
    pub fn stream(&mut self, ws: &mut WorldState, store: &RetailDatStore, cfg: &SceneConfig) {
        if self.pending.is_empty() {
            return;
        }
        let order: Vec<((i32, i32), BlockWork)> =
            std::mem::take(&mut self.pending).into_iter().collect();
        let mut touched = Vec::new();
        for (block, work) in order {
            let Some((xi, yi)) = Self::slot_of_block(ws, block) else {
                continue;
            };
            touched.push(block);
            self.build(ws, store, cfg, xi, yi, work);
        }
        if let Some(land) = ws
            .character
            .as_ref()
            .map(|c| std::sync::Arc::clone(c.land()))
        {
            for (bx, by) in touched {
                if let Some(id) = block_id(bx, by) {
                    land.load_block_cells(dereth_primitives::LandblockId(id));
                }
            }
        }
        self.register(ws, store, cfg);
    }

    /// Register every resident block's not-yet-registered statics and restrictions: after a
    /// build, and again when a body arrives.
    pub fn register(&mut self, ws: &mut WorldState, store: &RetailDatStore, cfg: &SceneConfig) {
        init_cell_statics(ws, store, cfg, self.statics_mut());
        init_cell_restrictions(ws, self.statics_mut());
    }

    fn build(
        &mut self,
        ws: &mut WorldState,
        store: &RetailDatStore,
        cfg: &SceneConfig,
        xi: u32,
        yi: u32,
        work: BlockWork,
    ) {
        let Some(slot) = ws.streamer.window.slot(xi, yi) else {
            return;
        };
        let (bx, by, lod_div, dir) = (slot.block_x, slot.block_y, slot.lod_div, slot.trans_dir);
        let Some(lb) = read_landblock(store, bx, by) else {
            // The world edge: nothing is there and nothing is an error.
            self.blocks.remove(&(bx, by));
            return;
        };
        let mesh = dereth_terrain::land::mesh::generate_landblock_with_table(
            &lb,
            &self.region,
            &self.table,
            bx,
            by,
            lod_div,
            dir,
        );
        let wants_objects = Self::wants_objects(ws, cfg, xi, yi);
        let previous = self.blocks.remove(&(bx, by));
        // A re-mesh keeps the block's objects only at an unchanged detail level: objects of every
        // kind need full detail, so a demotion drops them and a promotion places them.
        let reuse = work == BlockWork::Mesh
            && wants_objects
            && previous
                .as_ref()
                .is_some_and(|b| b.with_objects && b.side_cell_count == mesh.side_cell_count);
        let block = match previous {
            Some(b) if reuse => b,
            _ if wants_objects => {
                let land = land_content(
                    store,
                    &self.region,
                    &lb,
                    &mesh,
                    bx,
                    by,
                    cfg.lod_object_guard,
                );
                let decoded = block_id(bx, by)
                    .map(|id| self.cells.load_block(store, id))
                    .unwrap_or_default();
                let (interior_restrictions, per_cell) =
                    interior_content(&decoded, cfg.cell_statics);
                let mut cell_restrictions = land.cell_restrictions;
                cell_restrictions.extend(interior_restrictions);
                ResidentBlock {
                    statics: BlockStatics {
                        cell_statics: per_cell.into_iter().flatten().collect(),
                        land_statics: land.land_statics,
                        cell_restrictions,
                        statics_registered: false,
                        restrictions_registered: false,
                    },
                    with_objects: true,
                    side_cell_count: mesh.side_cell_count,
                }
            }
            _ => ResidentBlock::default(),
        };
        ws.streamer
            .window
            .set_mesh(xi, yi, crate::world_stream::SlotMesh::new(mesh));
        self.blocks.insert((bx, by), block);
    }
}

/// Build a scene's world with no device: the starting state, the window scrolled onto the
/// configured landblock and built, and the camera placed over it. With `cfg.character`, a body
/// is attached too, exactly as a drawing presentation attaches one.
///
/// # Errors
/// The region will not load, the configured landblock is not in the cell file, or the body will
/// not build.
pub fn load(
    store: &std::sync::Arc<RetailDatStore>,
    cfg: &SceneConfig,
) -> Result<(WorldState, BlockResidency), dereth_world_data::landblock::WorldError> {
    let region = dereth_world_data::landblock::load_region(store)?;
    let mut ws = WorldState::new(&region, cfg);
    let mut residency = BlockResidency::new(region);
    // Every slot comes back `Fetched` because the window did not exist: the full-reload path.
    let actions = ws
        .streamer
        .window
        .update_block(dereth_world_data::landblock::block_xy(cfg.landblock));
    residency.queue(&mut ws, cfg, &actions);
    residency.stream(&mut ws, store, cfg);
    if residency.blocks.is_empty() {
        return Err(dereth_world_data::landblock::WorldError::NoSuchLandblock(
            cfg.landblock,
        ));
    }
    place_load_camera(&mut ws, cfg)?;
    ws.update_viewer_cell();
    if cfg.character {
        attach_body(&mut ws, &mut residency, store, cfg)?;
    }
    Ok((ws, residency))
}

/// Attach a body to a world with no device, in a drawing presentation's order with the drawing
/// left out: the body, its sound table, its place in the world, the resident blocks' statics and
/// restrictions, the start cell, and the camera following it.
///
/// # Errors
/// The body will not build.
pub fn attach_body(
    ws: &mut WorldState,
    residency: &mut BlockResidency,
    store: &std::sync::Arc<RetailDatStore>,
    cfg: &SceneConfig,
) -> Result<(), dereth_world_data::landblock::WorldError> {
    let character = body_for(store, &residency.region, cfg)
        .map_err(|e| dereth_world_data::landblock::WorldError::Render(e.to_string()))?;
    ws.character_sound_table = body_sound_table(&character);
    let resident: Vec<(i32, i32)> = residency.resident().collect();
    place_body(ws, character, resident);
    residency.register(ws, store, cfg);
    stand_at_start_cell(ws, store, cfg);
    ws.follow_character();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_id_is_only_inside_the_world() {
        assert_eq!(block_id(0xA9, 0xB4), Some(0xA9B4));
        assert_eq!(block_id(0xFE, 0), Some(0xFE00));
        assert_eq!(block_id(0xFF, 0), None);
        assert_eq!(block_id(-1, 3), None);
    }

    fn retail_store() -> std::sync::Arc<RetailDatStore> {
        std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }))
    }

    /// With no body there is no physics world to register into: nothing is marked registered,
    /// so the blocks are registered when a body arrives.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn statics_and_restrictions_wait_for_a_body() {
        let store = retail_store();
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let cfg = SceneConfig::default();
        let mut ws = WorldState::new(&region, &cfg);
        let mut block = BlockStatics {
            land_statics: vec![CellStatic {
                cell: CellId(0xA9B4_0001),
                id: DataId(0x0100_0001),
                frame: dereth_primitives::Frame::default(),
                scale: 1.0,
            }],
            cell_restrictions: vec![(CellId(0xA9B4_0001), ObjectId(1))],
            ..BlockStatics::default()
        };
        assert!(init_cell_statics(&mut ws, &store, &cfg, [&mut block]).is_empty());
        init_cell_restrictions(&mut ws, [&mut block]);
        assert!(!block.statics_registered);
        assert!(!block.restrictions_registered);
        assert_eq!(ws.restricted_cells, 0);
    }

    /// The nine blocks around Holtburg at full detail: every scenery placement has its collision
    /// record at its own index, every resolved static's recorded index names a record for that
    /// static, the scenery comes first, and the neighbourhood grows scenery (the town's own block,
    /// all road and buildings, grows none).
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn holtburgs_statics_pair_each_placement_with_its_record() {
        let store = retail_store();
        let region = dereth_world_data::landblock::load_region(&store).expect("the region");
        let table = dereth_terrain::land::mesh::height_table(&region);
        let mut scenery = 0;
        for bx in 0xA8..=0xAA {
            for by in 0xB3..=0xB5 {
                let lb = read_landblock(&store, bx, by).expect("the landblock");
                let mesh = dereth_terrain::land::mesh::generate_landblock_with_table(
                    &lb,
                    &region,
                    &table,
                    bx,
                    by,
                    1,
                    dereth_landscape::Direction::InViewerBlock,
                );
                let c = land_content(&store, &region, &lb, &mesh, bx, by, true);
                assert!(c.full_detail);
                scenery += c.placed.len();
                for (i, p) in c.placed.iter().enumerate() {
                    assert_eq!(c.land_statics[i].id, p.gfxobj);
                    assert_eq!(c.land_statics[i].cell, p.cell);
                }
                let objects = c.lbi.as_ref().map_or(0, |info| info.objects.len());
                assert_eq!(c.object_slots.len(), objects);
                if let Some(info) = &c.lbi {
                    for (o, slot) in info.objects.iter().zip(&c.object_slots) {
                        if let Some((cell, i)) = slot {
                            assert_eq!(c.land_statics[*i].id, o.id);
                            assert_eq!(c.land_statics[*i].cell, *cell);
                            assert!(*i >= c.placed.len());
                        }
                    }
                }
                assert_eq!(
                    c.land_statics.len(),
                    c.placed.len() + c.object_slots.iter().flatten().count()
                );
            }
        }
        assert!(scenery > 0, "the neighbourhood grows scenery");
    }

    /// Holtburg with no device: a body stands in it, every block of the window is resident, the
    /// neighbourhood's statics are solid and the camera follows the body.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn holtburg_loads_with_a_body_and_solid_statics_and_no_device() {
        let store = retail_store();
        let cfg = SceneConfig {
            landblock: 0xA9B4,
            character: true,
            ..SceneConfig::default()
        };
        let (ws, residency) = load(&store, &cfg).expect("the world loads");
        let c = ws.character.as_ref().expect("a body");
        assert_eq!(c.position().cell.0 >> 16, 0xA9B4);
        let w = ws.streamer.window.mid_width() as usize;
        assert_eq!(residency.resident().count(), w * w);
        assert!(!residency.has_pending());
        assert!(
            !ws.cell_static_objects.is_empty(),
            "the statics have bodies"
        );
        assert!(ws.character_sound_table.is_some());
    }
}
