//! The interior cells of a landblock, wired onto both of their consumers.
//!
//! **This module wires; it does not implement.** Everything here is a conversion between shapes
//! other crates already own:
//!
//! | Decision | Whose |
//! |---|---|
//! | decoding environment-cell and environment records | [`dereth_assets`] |
//! | what a cell BSP, a physics BSP and a polygon mean | [`dereth_physics::geom`] |
//! | interior collision and the portal transit | [`dereth_physics::cell`], `transition::collide` |
//! | the portal traversal and the indoor draw order | `dereth_world_render::cells::portal_view` |
//!
//! The conversion belongs in the application: `dereth-physics` declares the shapes it needs and a
//! source trait, and the application connects `dereth-assets` through free functions. This avoids an
//! orphan-rule problem by keeping conversion outside either library.
//! [`crate::land_source`] supplies that conversion for landblocks; this module supplies it for
//! interior cells.
//!
//! # Why this exists
//!
//! `DatLandSource::env_cell` returned `None` unconditionally, so no interior geometry was ever
//! loaded — which removed **both** the surface physics would collide against and the geometry the
//! renderer would draw. The two symptoms a player reported ("no collision with buildings" and
//! "interiors are invisible from inside") are that one omission.
//!
//! # The trap in the seam
//!
//! [`dereth_physics::LandSource::env_cell`] only looks up resident cells;
//! it never loads from disk. That is deliberate — it is why physics stops at the
//! edge of the loaded window instead of stalling on I/O. So the loading happens here, on the same
//! path that loads a landblock, and `env_cell` only ever
//! looks in the table this fills.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use dereth_assets::common::{BspKind, BspTree as RawBspTree, Polygon as RawPolygon, VertexArray};
use dereth_assets::geometry::CellStruct;
use dereth_assets::{Decode, EnvCell, Environment, GfxObj, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::geom::bsp::{BspNode, BspNodeKind, BspTree};
use dereth_physics::geom::{Plane, Polygon, Sphere};
use dereth_physics::source::{CellPortal, EnvCellGeometry};
use dereth_physics::{PhysHandle, PhysicsWorld, SetupGeometry};
use dereth_primitives::{CellId, DataId, Frame, ObjectId, Position, Vec3};

/// The first interior cell id inside a landblock: `0xXXYY0100`. Outdoor land cells occupy
/// `0x0001..=0x0040` of the same 16-bit space.
pub const FIRST_ENV_CELL: u32 = 0x0100;

/// The environment-record id space, `0x0D000000`. `EnvCell::environment` is already widened.
const ENVIRONMENT_SPACE: u32 = 0x0D00_0000;

/// One decoded interior cell, with the cell structure its environment record supplies.
///
/// The pair is what both consumers need: the cell carries the placement frame, the surface list and
/// the portal connections, and the cell struct carries the geometry all of them index into.
#[derive(Debug, Clone)]
pub struct DecodedCell {
    pub id: CellId,
    pub cell: EnvCell,
    pub structure: CellStruct,
}

/// What one block's worth of loading did. Counted, and **asserted on** by the tests.
///
/// This project has twice lost weeks to a tolerated failure recorded into a number nothing
/// compared. A cell
/// that will not decode is exactly that shape of failure — the world simply has no inside there —
/// so the count is part of the type and the tests assert it is zero over the retail data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellLoadStats {
    /// Blocks whose landblock-info record named at least one cell.
    pub blocks: u64,
    /// Cells decoded and converted.
    pub loaded: u64,
    /// Ids promised that the cell dat does not carry. Non-zero would
    /// mean the id arithmetic is wrong, not that the data is short.
    pub missing: u64,
    /// Cells whose environment-cell record would not decode.
    pub undecodable: u64,
    /// Cells whose environment record is missing, or whose `cell_struct` index is out of range.
    pub no_environment: u64,
    /// Cells handed back during block release — i.e. taken out of
    /// the visible-cell table because the landblock that stabs them scrolled off the window.
    ///
    /// It is a **counter and not a subtraction** for the reason the rest of this struct is: the
    /// invariant a test can hold is `resident == loaded - released`, and with only `loaded` a
    /// release that removed the wrong cells and a release that removed none are the same reading.
    pub released: u64,
    /// Blocks [`super::land_source::DatLandSource::release_visible_cells`] was asked to release,
    /// including ones that turned out to hold no cells. The denominator for [`Self::released`]:
    /// "released 0 cells over 12 blocks" and "was never called" are different facts.
    pub blocks_released: u64,
}

/// Reads and decodes a landblock's interior cells, memoising the environment records they share.
///
/// One environment holds every cell struct of a building type, so a village of nine identical
/// cottages decodes one environment record and not nine — which is the shared asset cache's own behaviour.
#[derive(Debug, Default)]
pub struct EnvCellLoader {
    // ORDER-OK: a decode memo, only ever looked up.
    environments: HashMap<DataId, Option<Arc<Environment>>>,
    pub stats: CellLoadStats,
}

impl EnvCellLoader {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// pull the interior cells of one landblock.
    ///
    /// The ids are `blockId << 16 | (0x0100 + i)` for `i` in `0 .. num_cells` of the landblock info, which is the
    /// bound supplied by the landblock's cell count (`dereth_assets::world::LandblockInfo`'s own
    /// note: "the client uses it only to bound cell prefetch"). A block with no landblock-info record,
    /// or one whose `num_cells` is zero, has no interior and returns nothing.
    pub fn load_block(&mut self, store: &RetailDatStore, block: u16) -> Vec<DecodedCell> {
        let lbi_id = crate::landblock::lbi_did(block);
        let Ok(bytes) = store.read_typed(DbType::Lbi, lbi_id) else {
            return Vec::new();
        };
        let Ok(lbi) = dereth_assets::world::LandblockInfo::decode_payload_in(
            store.era_of(lbi_id),
            lbi_id,
            &bytes,
        ) else {
            return Vec::new();
        };
        if lbi.num_cells == 0 {
            return Vec::new();
        }
        self.stats.blocks += 1;
        let mut out = Vec::new();
        for i in 0..lbi.num_cells {
            let id = DataId((u32::from(block) << 16) | (FIRST_ENV_CELL + i));
            match self.load_one(store, id) {
                Ok(d) => {
                    self.stats.loaded += 1;
                    out.push(d);
                }
                Err(CellMiss::Missing) => self.stats.missing += 1,
                Err(CellMiss::Undecodable) => self.stats.undecodable += 1,
                Err(CellMiss::NoEnvironment) => self.stats.no_environment += 1,
            }
        }
        out
    }

    /// One interior cell, by full id, from `store`'s cell file and the environment its portal
    /// holds; `None` when either is missing or will not decode. Not counted: the drawing side
    /// reads another era's record of a cell the world already loaded with this.
    pub fn load_cell(&mut self, store: &RetailDatStore, id: CellId) -> Option<DecodedCell> {
        self.load_one(store, DataId(id.0)).ok()
    }

    fn load_one(&mut self, store: &RetailDatStore, id: DataId) -> Result<DecodedCell, CellMiss> {
        let bytes = store
            .read_typed(DbType::Cell, id)
            .map_err(|_| CellMiss::Missing)?;
        let cell = EnvCell::decode_payload_in(store.era_of(id), id, &bytes)
            .map_err(|_| CellMiss::Undecodable)?;
        let env = self
            .environment(store, cell.environment)
            .ok_or(CellMiss::NoEnvironment)?;
        let structure = env
            .cells
            .get(cell.cell_struct as usize)
            .cloned()
            .ok_or(CellMiss::NoEnvironment)?;
        Ok(DecodedCell {
            id: CellId(id.0),
            cell,
            structure,
        })
    }

    fn environment(&mut self, store: &RetailDatStore, id: DataId) -> Option<Arc<Environment>> {
        // Interior-cell unpacking widens the stored `u16` with `0x0D000000`; a record outside that space
        // is a decode bug rather than missing data, and is refused rather than read.
        if id.0 & 0xFF00_0000 != ENVIRONMENT_SPACE {
            return None;
        }
        self.environments
            .entry(id)
            .or_insert_with(|| {
                let bytes = store.read_typed(DbType::Environment, id).ok()?;
                Environment::decode_payload_in(store.era_of(id), id, &bytes)
                    .ok()
                    .map(Arc::new)
            })
            .clone()
    }
}

/// Why one cell did not load.
enum CellMiss {
    Missing,
    Undecodable,
    NoEnvironment,
}

/// The interior cells of each building of a block, in the building list's order: every cell
/// reached from the building's outdoor portals through the cells' own portals. `cells` are the
/// block's decoded cells; a cell no building reaches (a dungeon's) is in no list.
#[must_use]
pub fn building_cells(
    block: u16,
    buildings: &[dereth_assets::world::BuildInfo],
    cells: &[DecodedCell],
) -> Vec<Vec<CellId>> {
    let base = u32::from(block) << 16;
    let by_id: HashMap<u32, &DecodedCell> = cells.iter().map(|d| (d.id.0, d)).collect();
    buildings
        .iter()
        .map(|b| {
            let mut seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
            let mut todo: Vec<u32> = b
                .portals
                .iter()
                .map(|p| base | u32::from(p.other_cell_id))
                .collect();
            while let Some(id) = todo.pop() {
                let Some(d) = by_id.get(&id) else {
                    continue;
                };
                if !seen.insert(id) {
                    continue;
                }
                for p in &d.cell.portals {
                    if p.other_cell_id != 0xFFFF_FFFF {
                        todo.push(base | (p.other_cell_id & 0xFFFF));
                    }
                }
            }
            seen.into_iter().map(CellId).collect()
        })
        .collect()
}

/// The vertex positions a polygon names, from its container's own vertex array.
///
/// A cell structure and a graphics-object record both hold a vertex array and index it the same way, which is
/// why this is shared between [`physics_geometry`] and [`gfxobj_physics_bsp`].
fn vertices(va: &VertexArray, p: &RawPolygon) -> Vec<Vec3> {
    p.vertex_ids
        .iter()
        .filter_map(|&i| va.vertices.get(i as usize))
        .map(|v| v.position)
        .collect()
}

/// The decoded dat BSP into the physics BSP, resolving each leaf's polygon **ids** against the
/// pool.
///
/// This conversion also applies to graphics-object records. `'PORT'` is `0x504F5254`.
fn convert_bsp(src: &RawBspTree, polygons: Vec<Polygon>, ids: &[i16]) -> BspTree {
    // ORDER-OK: keyed by the polygon id and only ever looked up.
    let mut by_id: HashMap<i16, u32> = HashMap::new();
    for (i, id) in ids.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a polygon index; a cell struct has at most a few thousand. Not a float
        // conversion.
        by_id.insert(*id, i as u32);
    }
    let nodes = src
        .nodes
        .iter()
        .map(|n| {
            let kind = match n.leaf_index {
                Some(leaf) => BspNodeKind::Leaf {
                    leaf_index: i32::try_from(leaf).unwrap_or(-1),
                    solid: n.solid == Some(1),
                },
                None if n.tag == 0x504F_5254 => BspNodeKind::Portal,
                None => BspNodeKind::Node,
            };
            BspNode {
                sphere: n
                    .sphere
                    .map_or_else(Sphere::default, |s| Sphere::new(s.center, s.radius)),
                splitting_plane: n.plane.map_or_else(Plane::default, |p| Plane {
                    normal: p.normal,
                    d: p.d,
                }),
                pos_child: n.pos_child,
                neg_child: n.neg_child,
                kind,
                in_polys: n
                    .in_polys
                    .iter()
                    .filter_map(|id| by_id.get(id).copied())
                    .collect(),
            }
        })
        .collect();
    BspTree { nodes, polygons }
}

/// One graphics object's physics BSP in physics shape — the [`physics_geometry`] sibling for the
/// *other* geometry container the client collides against.
///
/// A building's shell is a graphics-object record, not a cell structure: landblock initialization
/// hands the building constructor a `BuildInfo::id`; simple setup creation
/// wraps it in a one-part setup, and object-collision discovery
/// walks this tree on part 0. The conversion itself is the same one the cell path does —
/// a leaf's `in_polys` are polygon **ids** and are resolved against the polygon pool rather than
/// used as indices — which is why both go through [`convert_bsp`].
///
/// `None` for a graphics-object record whose flag bit 0 is clear (no physics polygons and no physics BSP): the
/// client's physics-BSP pointer is then NULL and `find_obj_collisions` returns `OK_TS` without
/// caching the localspace spheres. The caller counts that rather than dropping the building, so a
/// shell that would be intangible is visible in a counter instead of being silently absent.
#[must_use]
pub fn gfxobj_physics_bsp(g: &GfxObj) -> Option<BspTree> {
    let src = g.physics_bsp.as_ref()?;
    if src.nodes.is_empty() {
        return None;
    }
    let polygons: Vec<Polygon> = g
        .physics_polygons
        .iter()
        .map(|p| Polygon::new(vertices(&g.vertex_array, p)))
        .collect();
    let ids: Vec<i16> = g.physics_polygons.iter().map(|p| p.poly_id).collect();
    Some(convert_bsp(src, polygons, &ids))
}

/// One decoded cell as [`dereth_physics::LandSource`] wants it.
///
/// Three details of the cell's serialized layout are load-bearing:
///
/// * `other_cell_id` is stored as the **low 16 bits** and has to be OR-ed with the landblock base;
///   the leads-outside flag (bit 2) is already resolved to `0xFFFFFFFF` by the dat decoder.
/// * `portal_side = ((~flags) >> 1) & 1` — it is 1 when bit 1 is **clear**. Getting this backwards
///   inverts every "which way does this doorway face" test.
/// * A portal's `polygon_id` is an index into the cell struct's polygon array
///   (resolved to `polygons + id`), not the polygon's stored `poly_id`.
#[must_use]
pub fn physics_geometry(d: &DecodedCell) -> EnvCellGeometry {
    let cs = &d.structure;
    let block = d.id.0 & 0xFFFF_0000;
    let portals = d
        .cell
        .portals
        .iter()
        .filter_map(|p| {
            let poly = cs.polygons.get(p.polygon_id as usize)?;
            Some(CellPortal {
                other_cell_id: if p.other_cell_id == 0xFFFF_FFFF {
                    0xFFFF_FFFF
                } else {
                    block | (p.other_cell_id & 0xFFFF)
                },
                portal: Polygon::new(vertices(&cs.vertex_array, poly)),
                portal_side: (!p.flags >> 1) & 1 != 0,
                other_portal_id: i32::from(p.other_portal_id),
                exact_match: p.flags & 1 != 0,
            })
        })
        .collect();

    let physics_polygons: Vec<Polygon> = cs
        .physics_polygons
        .iter()
        .map(|p| Polygon::new(vertices(&cs.vertex_array, p)))
        .collect();
    let physics_ids: Vec<i16> = cs.physics_polygons.iter().map(|p| p.poly_id).collect();
    let physics_bsp = (!cs.physics_bsp.nodes.is_empty()).then(|| {
        Arc::new(convert_bsp(
            &cs.physics_bsp,
            physics_polygons.clone(),
            &physics_ids,
        ))
    });
    // The cell BSP's nodes carry no polygons at all (`BspKind::Cell` reads nothing after the
    // children), and `point_inside_cell_bsp` / `sphere_intersects_cell_bsp` read only the
    // splitting planes, so its polygon pool is empty by construction rather than by omission.
    debug_assert!(matches!(BspKind::Cell, BspKind::Cell));
    let cell_bsp = (!cs.cell_bsp.nodes.is_empty())
        .then(|| Arc::new(convert_bsp(&cs.cell_bsp, Vec::new(), &[])));

    EnvCellGeometry {
        id: d.id,
        frame: d.cell.frame,
        portals,
        physics_polygons,
        physics_bsp,
        cell_bsp,
        // The cell's stab-list reference is stored as low 16 bits, like every other cell reference here.
        stab_list: d
            .cell
            .visible_cells
            .iter()
            .map(|&c| CellId(block | u32::from(c)))
            .collect(),
        // Environment-cell unpacking stores `flags & 1` into the cell's seen-outside flag.
        seen_outside: d.cell.flags & 1 != 0,
        static_objects: d
            .cell
            .static_objects
            .iter()
            .map(|o| (o.id, o.frame))
            .collect::<Vec<(DataId, Frame)>>(),
    }
}

// -------------------------------------------------------------------------------------------
// The objects a cell bakes in
// -------------------------------------------------------------------------------------------

/// One static-object placement, with the cell that owns it.
///
/// **The frame is in the landblock's space, not the cell's.**
/// Static-object initialization hands `static_object_frames[i]` straight to object creation,
/// which writes it into `position.frame` — the same
/// space every other `Position` in the block uses — so it is **not** composed with
/// the cell frame. Measured over the training-dungeon block `0x8602`, whose cell `0x86020102`
/// sits at `(160, -230, -12)` and whose three statics sit at `(158.998, -228.986, -6.806)`,
/// `(156.570, -231.001, -11.995)` and `(158.951, -234.053, -11.995)`: neighbours of the cell
/// origin, not offsets from it.
///
/// **It has a second producer**, and it is the same three steps in a different function:
/// land-scene collection and static-object initialization both end in object creation, adding
/// the object to the cell and registering it as a static object, with a
/// **outdoor land cell** as the cell rather than an environment cell. So a tree and a dungeon chair are the
/// same record here; only [`Self::cell`] says which kind of cell it was added to, and
/// [`Self::scale`] is non-1 only for the scenery half.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellStatic {
    /// The environment cell — or the outdoor land cell — this object was added to.
    pub cell: CellId,
    /// `static_object_ids[i]` — a `0x02……` setup-record id, or a `0x01……` graphics-object id
    /// that object creation wraps in a simple setup.
    pub id: DataId,
    /// `static_object_frames[i]`, block-local.
    pub frame: Frame,
    /// The object's internal scale setter receives this argument.
    ///
    /// **1.0 for everything but generated scenery.** Interior and landblock static-object
    /// initialization never set the scale, so those objects keep the constructor's 1.0.
    /// Scenery generation applies the object description's random scale draw after adding
    /// the object to its cell — which is why the
    /// registration below sets the frame, the scale and only then crosses the cells.
    pub scale: f32,
}

/// Every object one decoded cell bakes in, in the dat's own order.
///
/// Static-object initialization skips an id of zero (`static_objects[i] = NULL`) and this does
/// the same, which is why the vector can be shorter than `num_static_objects`.
#[must_use]
pub fn cell_statics(d: &DecodedCell) -> Vec<CellStatic> {
    d.cell
        .static_objects
        .iter()
        .filter(|o| o.id != DataId(0))
        .map(|o| CellStatic {
            cell: d.id,
            id: o.id,
            frame: o.frame,
            scale: 1.0,
        })
        .collect()
}

/// Whether physics has been given the cell a placement names — the one predicate the client does
/// not need, because in retail both static-object initializers are methods on the thing they add
/// to. It is split in two because a land cell and an interior cell are resident for different
/// reasons.
///
/// * an interior cell (`index >= 0x0100`) uses visible-interior lookup;
/// * an outdoor land cell (`index` in `1..=0x40`) uses landblock cell lookup,
///   which requires a resident landblock.
fn cell_is_resident(world: &PhysicsWorld, cell: CellId) -> bool {
    let index = cell.0 & 0xFFFF;
    if index >= FIRST_ENV_CELL {
        return dereth_physics::LandSource::env_cell(world.land(), cell).is_some();
    }
    if !(1..=0x40).contains(&index) {
        return false;
    }
    // LINT-OK: the high half of a cell id is the landblock id, by construction two bytes.
    let block = dereth_primitives::LandblockId((cell.0 >> 16) as u16);
    dereth_physics::LandSource::landblock(world.land(), block).is_some()
}

/// What [`CellStaticObjects::init`] did. Counted, and **asserted on** by the tests.
///
/// Every field after `created` records a placement that is **not** solid. The tests must compare
/// those counts rather than tolerate silent failures: without them, 644 objects a person expects
/// to see can be read out of the dat and dropped in silence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellStaticStats {
    /// Placements offered, i.e. summed over the cells, minus the
    /// zero ids static-object initialization itself skips.
    pub placements: u64,
    /// Creating `(id, 0, 0)` and adding the object to the cell puts bodies in a cell's lists.
    pub created: u64,
    /// The cell would not resolve through the world, so the object stayed
    /// out of every list. Non-zero means a cell was baked for the draw that physics has not been
    /// given, which is a wiring bug rather than bad data.
    pub unplaced: u64,
    /// A `0x01……` graphics-object id gives it no spheres and only
    /// the gfxobj's physics BSP, which is the `HAS_PHYSICS_BSP_PS (0x10000)` branch
    /// the sphere-only path omits.
    ///
    /// This is zero on the normal path and non-zero only with `--no-mesh-collision`, which is the
    /// control that turns part loading off. The counter is what a reader compares the two runs on.
    pub simple_setup: u64,
    /// A setup record that is missing or would not decode. A layout bug, not bad input.
    pub undecodable: u64,
    /// A body was created and its setup record carries **no** collision spheres. In the client an object like
    /// this is tested against its cylspheres or its parts' physics BSP; here
    /// `cell_find_obj_collisions` runs its (empty) sphere list and it is intangible. It is drawn,
    /// and it is in the cell lists, so adding the missing collision arms makes it solid without
    /// changing this registration path.
    pub intangible: u64,
    /// Bodies whose part arrays contain physics BSPs, so collision checking
    /// walks those parts' BSPs.
    pub bsp_arm: u64,
    /// Of those, the ones with no collision sphere at all, which would be wholly intangible
    /// without part loading.
    pub bsp_arm_without_spheres: u64,
    /// Bodies with no part BSP but a nonzero cylinder-sphere count,
    /// so object-collision checking takes the cylinder-sphere arm.
    pub cylsphere_arm: u64,
    /// Bodies destroyed by [`CellStaticObjects::release_block`] — the
    /// object-release half of environment-cell flushing.
    ///
    /// `created - destroyed` is [`CellStaticObjects::len`], and the tests assert that identity:
    /// a release that destroyed the handles without taking them out of the table, or took them
    /// out without destroying them, reads the same in `len` alone.
    pub destroyed: u64,
    /// Cells [`CellStaticObjects::release_block`] took out of its own tables, whether or not any
    /// of them held a body. The denominator for [`Self::destroyed`].
    pub cells_released: u64,
}

/// The bodies of every interior cell's baked objects.
///
/// The interior sibling of `dereth_client_runtime::object_physics::ObjectPhysics`, and deliberately much
/// smaller: a cell static never moves, is never destroyed by the server and carries no `ObjDesc`,
/// so there is no diff to take. It is created once per cell, when the block that owns the cell is
/// baked, and destroyed with that block.
///
/// **Static object creation uses `(id, 0, 0)`** — object id **0** and `dynamic = 0`, both read out of
/// static-object initialization itself. The client's static objects are consequently *not* in
/// its object table. This build's physics-world creation API has no way to say so, so every one of
/// them is inserted under the key `0` and the last one wins. That is inert rather than wrong:
/// the physics sweep calls `update_object`, `update_object` calls `set_active`, and
/// activation refuses a `STATIC_PS` object — so the one entry under key
/// `0` is stepped and does nothing, exactly as all 644 of them would. It does not disturb contract
/// item 7.3 either: a `LongHash` insert moves no other key, so the sweep order of the objects that
/// *are* the client's is unchanged and only an inert step is added ahead of them.
#[derive(Debug)]
pub struct CellStaticObjects {
    /// setup record's collision half, memoised. A dungeon places the same chair forty-one times.
    /// `None` is cached too.
    geometry: BTreeMap<u32, Option<Arc<SetupGeometry>>>,
    /// The live bodies, by the cell that owns them, so a block's cells can be released together.
    handles: BTreeMap<u32, Vec<PhysHandle>>,
    /// Cells already initialised. Separate from `handles` because a cell every one of whose
    /// statics is refused still has to count as done, or a re-mesh re-counts it for ever.
    done: std::collections::BTreeSet<u32>,
    pub stats: CellStaticStats,
    /// What loading those setups' parts did.
    pub part_stats: crate::setup::SetupPartStats,
    /// `SceneConfig::mesh_collision`; clear, no part is loaded and every object falls to its
    /// spheres. **Read once per setup**, at the
    /// memoised load, so flipping it mid-run would leave the cache behind — nothing does.
    pub mesh_collision: bool,
}

impl Default for CellStaticObjects {
    fn default() -> Self {
        Self {
            geometry: BTreeMap::new(),
            handles: BTreeMap::new(),
            done: std::collections::BTreeSet::new(),
            stats: CellStaticStats::default(),
            part_stats: crate::setup::SetupPartStats::default(),
            mesh_collision: true,
        }
    }
}

impl CellStaticObjects {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many bodies are live.
    #[must_use]
    pub fn len(&self) -> usize {
        self.handles.values().map(Vec::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// The bodies one cell owns, for the tests.
    #[must_use]
    pub fn handles(&self, cell: CellId) -> &[PhysHandle] {
        self.handles.get(&cell.0).map_or(&[], Vec::as_slice)
    }

    /// Every cell [`Self::init`] has registered and [`Self::release_block`] has not taken back, in
    /// id order, for the tests: `len()` alone cannot say *which* blocks a residency
    /// spans, and that is the question a release has to answer.
    #[must_use]
    pub fn registered_cells(&self) -> Vec<CellId> {
        self.done.iter().map(|c| CellId(*c)).collect()
    }

    /// Initialize static objects for a set of placements.
    ///
    /// ```text
    /// for i in 0 .. num_static_objects:
    ///     if static_object_ids[i] == 0: static_objects[i] = NULL; continue
    ///     static_objects[i] = make_object(static_object_ids[i], 0, 0)
    ///     add_object_to_cell(static_objects[i], this, &static_object_frames[i])
    /// ```
    ///
    /// Adding an object to a cell is `enter_cell` + `position.frame = frame` +
    /// `calc_cross_cells_static`, and `calc_cross_cells_static` differs from
    /// `calc_cross_cells` in exactly one store: the do-not-load-cells flag becomes 1.
    /// The physics world's cross-cell calculation accepts that flag, so the three lines below are the
    /// whole of it.
    ///
    /// A cell is skipped if it is already registered, so this is idempotent per cell: a landblock
    /// window slot that is re-meshed for a LOD change must not stack a second body on every chair.
    ///
    /// # What the return value is
    ///
    /// `static_objects`, index-parallel with the placements it was handed, which is the client's
    /// own array. Static-object initialization walks the three arrays together:
    /// for each index it makes the object from `static_object_ids[i]`, stores the pointer in
    /// `static_objects[i]`, and adds it to the cell at `static_object_frames[i]`; a zero id
    /// leaves `static_objects[i]` NULL.
    ///
    /// The **index** is the key: `static_object_ids[i]`, `static_object_frames[i]` and
    /// `static_objects[i]` are one placement. Its single created object owns both the rendered
    /// parts and the physics body, and successful setup queues the setup's `default_script`
    /// during that same construction. The outdoor half preserves the same object identity by
    /// creating it, adding it to the cell, and only then appending it to `static_objects`.
    /// That append is reached **only** when land-cell lookup succeeds; when lookup fails,
    /// the just-created object is destroyed
    /// straight away.
    ///
    /// This build splits that one pointer in two — the collision body here, the script's
    /// `MotionDriver` in `crate::particles::EmitterHost` — so the index has to be carried
    /// explicitly. `None` is the client's `static_objects[i] = NULL`: a zero id, a refused setup,
    /// a cell physics has not been given, or a cell this call skipped because it was already
    /// registered.
    pub fn init(
        &mut self,
        store: &RetailDatStore,
        world: &mut PhysicsWorld,
        statics: &[CellStatic],
    ) -> Vec<Option<PhysHandle>> {
        // `static_objects`, one slot per placement, all NULL until object creation fills them.
        let mut made: Vec<Option<PhysHandle>> = vec![None; statics.len()];
        // The cells that are new, taken first so that a cell counts as done even if every one of
        // its placements is refused below — otherwise a re-mesh re-counts it for ever.
        let fresh: Vec<(usize, CellStatic)> = statics
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, s)| !self.done.contains(&s.cell.0))
            .collect();
        for (_, s) in &fresh {
            self.done.insert(s.cell.0);
        }
        for (slot, s) in &fresh {
            self.stats.placements += 1;
            if s.id.0 >> 24 != 0x02 && !(s.id.0 >> 24 == 0x01 && self.mesh_collision) {
                // See `CellStaticStats::simple_setup`.
                self.stats.simple_setup += 1;
                continue;
            }
            let Some(geometry) = self.setup_geometry(store, s.id) else {
                self.stats.undecodable += 1;
                continue;
            };
            // The client cannot reach this: static-object initialization is a method *on* the
            // cell, so
            // the cell it adds to is by construction resident. Here the draw side and the physics
            // side load cells on two different paths, and an object added to a cell the land
            // source has never seen would register no shadow and silently not be there — so the
            // client's visible-cell lookup is asked
            // first and a miss is counted.
            //
            // **The outdoor arm.** A land cell is not an environment cell and
            // `LandSource::env_cell` will never answer for one; the client's own predicate for
            // the scenery/statics path is land-cell lookup: "is this
            // landblock resident and is the index in `1..=64`".
            // A missing landscape block is exactly `LandSource::landblock` answering `None`.
            if !cell_is_resident(world, s.cell) {
                self.stats.unplaced += 1;
                continue;
            }
            // A setup with no spheres but a part BSP is **not** intangible. Object-collision
            // checking takes the BSP arm whenever the part array contains a physics BSP, whatever
            // the sphere count. The middle arm means a setup with a cylinder-sphere collider and
            // nothing else is not intangible either.
            let intangible = geometry.spheres.is_empty()
                && !geometry.caches_physics_bsp()
                && geometry.cyl_spheres.is_empty();
            if geometry.caches_physics_bsp() {
                self.stats.bsp_arm += 1;
                if geometry.spheres.is_empty() {
                    self.stats.bsp_arm_without_spheres += 1;
                }
            } else if !geometry.cyl_spheres.is_empty() {
                self.stats.cylsphere_arm += 1;
            }
            // Object creation with `(id, 0, 0)`: object id 0, and not dynamic — so
            // `PhysicsObj::new` raises
            // `STATIC_PS`, which is what `find_obj_collisions` reports a hit against as
            // *environment* rather than as an object.
            let h = world.create(ObjectId(0), geometry, false);
            // Add the object to this cell at the frame. The cell is named directly rather than
            // resolved from the frame: the dat says which cell bakes the object in, and
            // outdoor-coordinate adjustment would answer with the land cell the dungeon is under.
            world.enter_cell(h, s.cell);
            if let Some(o) = world.get_mut(h) {
                o.set_frame(s.frame);
                o.position = Position::new(s.cell, s.frame);
                // The object's internal scale setter, which the scenery half runs between adding
                // the object to the cell and registering it as a static object. It is set
                // **before** `calc_cross_cells` here rather than after, because this build's
                // `calc_cross_cells` reads `scale` to size the sphere it crosses cells with
                // (`step.rs`, arm 2) — the client's object-scale setter calls the
                // part-array scale setter, which rebuilds the parts, and the
                // cross-cell set is recomputed on the object's next `set_position`. A tree scaled
                // 2.4 that crossed its cells at scale 1 would be solid in one cell and thin air
                // in the next.
                o.scale = s.scale;
            }
            world.calc_cross_cells(h, true);
            self.handles.entry(s.cell.0).or_default().push(h);
            // `static_objects[i] = obj`: the placement's own slot, so
            // that the script half of the same physics object can find its body again.
            made[*slot] = Some(h);
            self.stats.created += 1;
            if intangible {
                self.stats.intangible += 1;
            }
        }
        made
    }

    /// Release the bodies of every cell in one landblock when the cells go.
    ///
    /// Called from `dereth_scene::world_scene::WorldScene::release_block_interiors`, the middle step of
    /// releasing an
    /// entire land block.
    ///
    /// **What survives, and it is the reason a return visit is cheap**: [`Self::geometry`], the
    /// memoised setup-record/graphics-object collision halves. That memo belongs to the shared asset cache, not the cell —
    /// the client's `flush_cells` releases the environment cell and its objects and leaves the database cache
    /// holding every decoded record they were built from, which is why adding a visible cell
    /// on a return visit uses a cache hit and not a disk read. Clearing
    /// it here would be a *different* build from the client's, and a slower one.
    ///
    /// Returns how many bodies were destroyed.
    pub fn release_block(&mut self, world: &mut PhysicsWorld, block: u16) -> u64 {
        let base = u32::from(block) << 16;
        let cells: Vec<u32> = self
            .done
            .iter()
            .copied()
            .filter(|c| c & 0xFFFF_0000 == base)
            .collect();
        let mut destroyed = 0u64;
        for c in cells {
            self.done.remove(&c);
            self.stats.cells_released += 1;
            for h in self.handles.remove(&c).unwrap_or_default() {
                world.destroy(h);
                destroyed += 1;
            }
        }
        self.stats.destroyed += destroyed;
        destroyed
    }

    /// One placement's collision half, memoised, or `None` for anything that cannot produce one.
    fn setup_geometry(&mut self, store: &RetailDatStore, id: DataId) -> Option<Arc<SetupGeometry>> {
        if let Some(hit) = self.geometry.get(&id.0) {
            return hit.clone();
        }
        // **With the parts**, so a table's own mesh is what the body meets. The same
        // loader the object stream uses (`crate::setup`), because a chair baked into a
        // cell and a door sent by the server are the same physics object to the client.
        let loaded = if id.0 >> 24 == 0x01 {
            // A graphics-object id is a legal object-creation argument through simple setup
            // construction, and 522 of the training dungeon's placements are one.
            crate::setup::simple_setup_geometry(store, id, &mut self.part_stats).map(Arc::new)
        } else {
            store
                .read_typed(DbType::Setup, id)
                .ok()
                .and_then(|b| Setup::decode_payload_in(store.era_of(id), id, &b).ok())
                .map(|s| {
                    Arc::new(if self.mesh_collision {
                        crate::setup::setup_geometry_with_parts(store, &s, &mut self.part_stats)
                    } else {
                        crate::setup::setup_geometry(&s)
                    })
                })
        };
        self.geometry.insert(id.0, loaded.clone());
        loaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: `client_cell_1.dat`, through the environment-cell and environment decoders (whose
    // own tests consume all 805,347 cell records' payloads exactly) and the cell geometry.
    // What this adds is the claim the *wiring* makes: that the ids this file computes address the
    // records the client addresses, and that the three polarity details above are right.
    //
    // The decoded Holtburg (0xA9B4) cell data names cell `0xA9B40100`
    // and its portal to `0xA9B40110`.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn holtburgs_interior_cells_load_and_their_portals_point_at_each_other() {
        // Absent dats fail rather than skip: a checkout with no retail dats would otherwise
        // report `... ok` having loaded no cell.
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        let mut loader = EnvCellLoader::new();
        let cells = loader.load_block(&store, 0xA9B4);
        assert!(
            !cells.is_empty(),
            "Holtburg has buildings, so it has interior cells"
        );
        assert_eq!(
            loader.stats.undecodable, 0,
            "an environment cell would not decode"
        );
        assert_eq!(
            loader.stats.missing, 0,
            "num_cells named an id the dat does not carry"
        );
        assert_eq!(
            loader.stats.no_environment, 0,
            "a cell's environment record is missing"
        );
        assert_eq!(loader.stats.loaded, cells.len() as u64);

        // The ids are contiguous from 0xA9B40100.
        for (i, c) in cells.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let want = 0xA9B4_0000 | (FIRST_ENV_CELL + i as u32);
            assert_eq!(c.id.0, want, "cell {i}");
        }

        let geom: Vec<EnvCellGeometry> = cells.iter().map(physics_geometry).collect();
        // Every cell has a volume and something solid in it, or there is nothing to stand in and
        // nothing to collide with.
        assert!(
            geom.iter().all(|g| g.cell_bsp.is_some()),
            "a cell has no cell BSP"
        );
        assert!(
            geom.iter().all(|g| g.physics_bsp.is_some()),
            "a cell has no physics BSP"
        );
        assert!(
            geom.iter().all(|g| !g.physics_polygons.is_empty()),
            "a cell has no physics polygons"
        );

        // Portals are symmetric: whatever a cell names as its neighbour must name it back. This is
        // what catches the landblock base being left off the id, which would otherwise produce a
        // self-consistent-looking `0x0110`.
        let ids: std::collections::BTreeSet<u32> = geom.iter().map(|g| g.id.0).collect();
        let mut interior = 0usize;
        let mut outside = 0usize;
        for g in &geom {
            for p in &g.portals {
                if p.other_cell_id == 0xFFFF_FFFF {
                    outside += 1;
                    continue;
                }
                interior += 1;
                assert!(
                    ids.contains(&p.other_cell_id),
                    "{:#010X} opens onto {:#010X}, which is not a cell of this block",
                    g.id.0,
                    p.other_cell_id
                );
                let other = geom
                    .iter()
                    .find(|o| o.id.0 == p.other_cell_id)
                    .expect("just checked");
                assert!(
                    other.portals.iter().any(|q| q.other_cell_id == g.id.0),
                    "{:#010X} -> {:#010X} is one-way",
                    g.id.0,
                    p.other_cell_id
                );
                // Two cells sharing a doorway sit on opposite sides of its plane.
                let back = other
                    .portals
                    .iter()
                    .find(|q| q.other_cell_id == g.id.0)
                    .expect("just checked");
                assert_ne!(
                    p.portal_side, back.portal_side,
                    "{:#010X} and {:#010X} claim the same side of their shared portal",
                    g.id.0, p.other_cell_id
                );
            }
        }
        assert!(interior > 0, "no cell connects to another");
        assert!(
            outside > 0,
            "no cell opens to the outdoors, so nobody could walk in"
        );
    }
}
