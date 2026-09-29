//! The inputs physics does not own.
//!
//! Physics needs decoded collision geometry, which the dat decoders produce. To
//! stay independent, `dereth-physics` declares the shapes it needs and a source trait; the tests
//! implement it from the retail dat and the application wires `dereth-assets` onto it with a free
//! function (no orphan-rule problem, because the conversion lives in the binary, not in either
//! library).

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_primitives::{CellId, Frame, LandblockId, Position, Vec3};

use crate::geom::plane::Plane;
use crate::geom::polygon::Polygon;
use crate::geom::sphere::{CylSphere, Sphere};
use crate::geom::BspTree;
use crate::geom::SphereExt;
use crate::globals::LAND_HEIGHT_TABLE_LEN;
use crate::land::{LandblockCollision, VERTEX_COUNT};
use crate::math::V3;

/// The setup collision geometry: spheres, cylinder-spheres, step heights, radius, height and box.
#[derive(Debug, Clone, Default)]
pub struct SetupGeometry {
    pub sorting_sphere: Sphere,
    pub selection_sphere: Sphere,
    pub spheres: Vec<Sphere>,
    pub cyl_spheres: Vec<CylSphere>,
    /// The dat reads `step_up_height` **before** `step_down_height`.
    pub step_up_height: f32,
    pub step_down_height: f32,
    pub radius: f32,
    pub height: f32,
    /// The serialized **data** field (packed flag bit 3).
    ///
    /// It is *not* the source of `HAS_PHYSICS_BSP_PS`: that cached flag asks whether any
    /// part's graphics object carries a tree. The original client reads this data bit only
    /// during serialization. It remains here because the dat carries it, and the part
    /// loader compares it with [`Self::caches_physics_bsp`].
    pub has_physics_bsp: bool,
    pub physics_bsp: Option<Arc<BspTree>>,
    /// Whether setup allows free heading; this becomes `ObjectInfoState::FreeRotate (0x10)`.
    pub allow_free_heading: bool,
    /// The parts, in order -- what the BSP arm of the object's collision search
    /// walks.
    ///
    /// **Empty means "not modelled", not "no parts".** A `SetupGeometry` that carries spheres
    /// and nothing else keeps taking the sphere arm, because [`Self::caches_physics_bsp`] is
    /// then false.
    pub parts: Vec<SetupPart>,
}

/// One setup part as needed for BSP collision.
///
/// A part's world position is **derived, not stored**. Construction takes that part
/// index's default scale, or `(1, 1, 1)` when absent, and selects placement frame `0x65`.
/// The part update then combines the object frame, selected placement frame and
/// part-array scale. The setup supplies only the local frame and scale;
/// [`SetupGeometry::placed_part`] composes them with the current object frame and
/// scale when collision is tested.
#[derive(Debug, Clone)]
pub struct SetupPart {
    /// The part's frame from setup placement frame `0x65`, or from frame 0 — the fallback
    /// takes when `0x65` is absent, which is the case
    /// for every shipped setup that has any placement frame at all.
    pub placement_frame: Frame,
    /// Setup default scale at index `i`, or `(1, 1, 1)`. Only `z` reaches the collision path (the
    /// loader passes `gfxobj_scale.z` twice), but all three are carried because the data record
    /// writes all three.
    pub default_scale: Vec3,
    /// The graphics object's own physics BSP.
    pub physics_bsp: Option<Arc<BspTree>>,
    /// The graphics-object box uses min/max over the **vertex array**, not the physics
    /// polygons. Part bounding-box lookup returns this stored box without consulting
    /// other geometry.
    ///
    /// `None` means "not decoded", as for [`physics_bsp`](Self::physics_bsp). A part
    /// without a box registers no cell of its own, matching the original client when
    /// the part has no graphics object.
    pub bound_box: Option<crate::geom::BBox>,
    /// Root bounding sphere of the **drawing** tree.
    ///
    /// The only reader in the collision half of the client is the environment cell's transit
    /// search, whose prescreen is
    /// `physics_sphere ? physics_sphere : drawing_sphere` -- so a part with a drawing mesh and no
    /// physics mesh still carries its owner through a portal. Without it the
    /// retail door's third part (`0x0100097D`, no physics BSP) would be invisible to the
    /// expansion where the client sees it.
    pub drawing_sphere: Option<Sphere>,
    /// First degrade mode for the part's graphics object, or `None` when the
    /// object carries no degrade info or an empty list.
    ///
    /// It exists for one predicate, [`PhysicsPart::always_2d`].
    pub first_degrade_mode: Option<i32>,
}

impl Default for SetupPart {
    /// An unplaced part at unit scale with no mesh.
    fn default() -> Self {
        Self {
            placement_frame: Frame::new(Vec3::ZERO, dereth_primitives::Quat::IDENTITY),
            default_scale: Vec3::new(1.0, 1.0, 1.0),
            physics_bsp: None,
            bound_box: None,
            drawing_sphere: None,
            first_degrade_mode: None,
        }
    }
}

impl SetupGeometry {
    /// The geometry substitutes when the object has no part
    /// array or no spheres: one `dummy_sphere` at `(0, 0, 0.1)` with radius `0.1`, unscaled.
    #[must_use]
    pub fn dummy() -> Self {
        Self {
            spheres: vec![Sphere::dummy()],
            ..Self::default()
        }
    }

    /// The client stores at most two spheres, so this is what the transition system actually sees.
    #[must_use]
    pub fn path_spheres(&self) -> &[Sphere] {
        let n = self.spheres.len().min(2);
        &self.spheres[..n]
    }

    /// Whether **any** part's graphics object carries a physics BSP. This is the
    /// part-array initialization predicate that sets `HAS_PHYSICS_BSP_PS (0x10000)`.
    ///
    /// It deliberately does not read [`Self::has_physics_bsp`], the serialized setup
    /// field that collision ignores.
    #[must_use]
    pub fn caches_physics_bsp(&self) -> bool {
        self.parts.iter().any(|p| p.physics_bsp.is_some())
    }

    /// Place part `i` for a resting object by composing its placement frame with the object's
    /// frame. The object supplies the scalar `scale` uniformly on all three axes.
    /// `gfxobj_scale` is the setup default scale at `i` multiplied by `scale`; only `z` is read
    /// downstream.
    #[must_use]
    pub fn placed_part(&self, i: usize, pos: &Position, scale: f32) -> Option<PhysicsPart> {
        self.placed_part_posed(i, pos, scale, None)
    }

    /// [`Self::placed_part`] posed by an object's **current animation frame**.
    ///
    /// Animated posing uses the current animation's part frame at `floor(frame_number)`.
    /// It falls back to the setup placement frame only before an animation is available.
    /// A body with a motion table therefore supplies its **animated** frame to the BSP
    /// throughout animation playback.
    ///
    /// The training-academy door setup `0x0200024F` has one placement frame, key `0`; a
    /// request for `0x65` falls back to it. Its leaves are at **-150 deg** and **-30 deg**
    /// about z, displaced 0.44 m out of the doorway. That is frame **~10.7 of 31** of
    /// the door's open animation `0x03000559`; closed frame 0 places the leaves at
    /// -179.75 deg and +0.03 deg. The setup ships a **third-open** door.
    ///
    /// `frames` supplies one [`Frame`] per animated part. `None` selects setup placement.
    /// A slice shorter than the part list falls back per part: animation supplies
    /// `min(part count, the animation frame's part count)` frames.
    #[must_use]
    pub fn placed_part_posed(
        &self,
        i: usize,
        pos: &Position,
        scale: f32,
        frames: Option<&[Frame]>,
    ) -> Option<PhysicsPart> {
        let p = self.parts.get(i)?;
        let f = frames.and_then(|f| f.get(i)).unwrap_or(&p.placement_frame);
        let local = Frame::new(f.origin.mul(scale), f.rotation);
        Some(PhysicsPart {
            pos: Position::new(pos.cell, crate::math::combine(&pos.frame, &local)),
            gfxobj_scale: p.default_scale.z * scale,
            physics_bsp: p.physics_bsp.clone(),
            bound_box: p.bound_box,
            drawing_sphere: p.drawing_sphere,
            first_degrade_mode: p.first_degrade_mode,
        })
    }
}

/// One interior cell's collision geometry, from `client_cell_1.dat`.
///
/// # Two spaces, and [`frame`](Self::frame) is the transform between them
///
/// **Read this before deriving a position from anything in here.** The geometry fields —
/// [`physics_polygons`](Self::physics_polygons), [`physics_bsp`](Self::physics_bsp),
/// [`cell_bsp`](Self::cell_bsp) and each [`CellPortal::portal`] — are in the **environment's own
/// cell-local space**, taken directly from the environment geometry's vertex array.
/// A [`Position`] origin is in **landblock space**, the space used by sphere, sweep
/// and cell tests here. Those tests transform a global sphere through
/// [`frame`](Self::frame) into cell-local space before testing the geometry.
///
/// So a point derived from the polygons — a bounding-box centre, a wall centroid — has to go
/// through [`crate::math::localtoglobal`] with [`frame`](Self::frame) before it can be a
/// `Position` origin. Using it raw places the object wherever the *landblock* happens to have that
/// coordinate, which is a plausible-looking point in a cell it is not inside.
///
/// That failure is silent and looks exactly like broken collision: the sweep walks the cell's tree
/// once at the wrong point, finds nothing, discovers the sphere is in no cell, and — per the
/// sphere path's `if check_cell == NULL: return OK_TS` — does nothing for the
/// rest of the path. A position derived this way has landed 175 m from the room it named.
/// `dereth-client`'s `tests/dat/world/interior_sight_probe.rs` holds both halves.
///
/// [`static_objects`](Self::static_objects) frames are the exception: the loader hands them to the
/// cell insertion path already in landblock space.
#[derive(Debug, Clone, Default)]
pub struct EnvCellGeometry {
    pub id: CellId,
    /// The cell's placement frame inside its landblock: the cell-local to landblock transform used
    /// by the rest of this geometry.
    pub frame: Frame,
    pub portals: Vec<CellPortal>,
    /// Collision polygons in **cell-local** space.
    pub physics_polygons: Vec<Polygon>,
    /// The tree the environment-collision search walks. **Cell-local.**
    pub physics_bsp: Option<Arc<BspTree>>,
    /// The tree the point and sphere cell-containment queries walk.
    /// **Cell-local** — both of those take their argument through [`frame`](Self::frame) first.
    pub cell_bsp: Option<Arc<BspTree>>,
    /// The ids of the cells visible from this one.
    pub stab_list: Vec<CellId>,
    /// Whether the cell is visible from outside.
    pub seen_outside: bool,
    /// Objects baked into the cell, as `(setup id, frame)`. Those frames are **landblock**-space,
    /// unlike the geometry above; measured over 905 placements in five dungeon landblocks, every
    /// one within 30 m of its own cell's [`frame`](Self::frame) origin.
    pub static_objects: Vec<(dereth_primitives::DataId, Frame)>,
}

/// The collision data for one part of an object.
///
/// The original client's part also carries a mesh, palette, clip-plane list and render
/// fields. This Rust model retains only the fields collision reads.
#[derive(Debug, Clone)]
pub struct PhysicsPart {
    /// The part's position, already composed with the owning object's frame
    /// by combining the object frame, animation frame and scale.
    /// For a building that composition is the identity because building creation receives a
    /// graphics-object id and builds a one-part setup whose only placement frame is the default.
    pub pos: Position,
    /// The physics part's graphics-object Z scale. Only the Z component reaches collision, which
    /// is why this is a scalar. Construction sets all three scale components to `1.0`; setup data
    /// overwrites them when present, while a simple building setup stays unscaled.
    pub gfxobj_scale: f32,
    /// Physics BSP. Its root bounding sphere is read from the tree instead of duplicated beside it.
    pub physics_bsp: Option<Arc<BspTree>>,
    /// Setup bounding box; see [`SetupPart::bound_box`].
    pub bound_box: Option<crate::geom::BBox>,
    /// Drawing sphere; see [`SetupPart::drawing_sphere`].
    pub drawing_sphere: Option<Sphere>,
    /// First degrade mode of the physics part — see
    /// [`SetupPart::first_degrade_mode`] and [`Self::always_2d`].
    pub first_degrade_mode: Option<i32>,
}

impl Default for PhysicsPart {
    /// An unplaced part at unit scale with no mesh.
    fn default() -> Self {
        Self {
            pos: Position::new(
                CellId(0),
                Frame::new(Vec3::ZERO, dereth_primitives::Quat::IDENTITY),
            ),
            gfxobj_scale: 1.0,
            physics_bsp: None,
            bound_box: None,
            drawing_sphere: None,
            first_degrade_mode: None,
        }
    }
}

impl PhysicsPart {
    /// Return the root node's bounding sphere.
    #[must_use]
    pub fn physics_sphere(&self) -> Option<Sphere> {
        self.physics_bsp.as_ref()?.root().map(|n| n.sphere)
    }

    /// The whole of the always-2D predicate: there is degrade info, it has at least one degrade,
    /// and the first degrade's `degrade_mode` is not 1.
    ///
    /// `degrade_mode` 1 is the ordinary 3D part; 2 through 5 are the billboard modes the renderer
    /// switches on, and a part drawn as a billboard has
    /// no meaningful bounding **box**, which is why
    /// [`CellResolver::add_all_outside_cells_bbox`](crate::cell::CellResolver) reaches for its
    /// bounding *sphere* instead.
    ///
    /// **It is transcribed because the dat shows it is live, not dead.** Over all **5,935**
    /// shipped setups and their **22,960** part references: **15** parts are
    /// `Always2D` (six at `degrade_mode` 2, nine at 5, against **15,516** at mode 1), **973** part
    /// references carry a physics BSP; **4** of those reference graphics object `0x0100215F`, which is
    /// both, and **5** of the **530** setups that carry a physics BSP anywhere have an `Always2D`
    /// part -- `0x02000359`, `0x02000A02`, `0x020017D5`, `0x02001BCF` and `0x02001C2B`. It is
    /// rare and it is not unreachable, so it is transcribed rather than pinned.
    #[must_use]
    pub fn always_2d(&self) -> bool {
        matches!(self.first_degrade_mode, Some(m) if m != 1)
    }
}

/// The collision geometry of a building.
///
/// Buildings initialize as static and non-dynamic (`STATIC_PS`). They also carry
/// portal and leaf-cell lists, but collision uses neither list: it tests **part 0**
/// of the part array. The complete array is retained here so that "part 0 and no
/// other" remains a testable claim.
#[derive(Debug, Clone, Default)]
pub struct BuildingGeometry {
    /// Building part-array entries. Empty models `part_array == NULL`, which
    /// `find_building_collisions` answers with `OK_TS` without ever touching `bldg_check`.
    pub parts: Vec<PhysicsPart>,
}

/// A portal: adjacent cell id, polygon, side, matching portal id and exact-match flag.
#[derive(Debug, Clone)]
pub struct CellPortal {
    /// `0xFFFFFFFF` marks a portal to the **outdoors**.
    pub other_cell_id: u32,
    pub portal: Polygon,
    /// Which side of the portal polygon's plane the *other* cell lies on.
    /// `POSITIVE = 1`, `NEGATIVE = 0`.
    pub portal_side: bool,
    pub other_portal_id: i32,
    /// Stored and packed, but no physics path reads it.
    pub exact_match: bool,
}

impl CellPortal {
    /// The portal's plane, in the owning cell's local space.
    #[must_use]
    pub fn plane(&self) -> Plane {
        self.portal.plane
    }
}

/// Everything physics needs from outside itself.
pub trait LandSource: Send + Sync {
    /// The landblock's collision geometry, or `None` for a block that is not resident or is
    /// degenerate. The client lookup returns NULL for both.
    fn landblock(&self, id: LandblockId) -> Option<Arc<LandblockCollision>>;

    /// **Is this block in the resident array at all?** This exposes the visible-block lookup's
    /// other two refusals, separated from "the dat has no such block".
    ///
    /// ```text
    ///   blocks == NULL                   -> NULL
    ///   lcoord outside the window        -> NULL
    ///   the slot is empty                -> NULL
    ///   a load state other than 8        -> NULL
    ///   otherwise                        -> the slot
    /// ```
    ///
    /// [`Self::landblock`] answers the *geometry* question and an implementation is free to read
    /// it off disk on demand; this answers the **residency** question, which is what decides
    /// whether visible-cell lookup can hand a cell back at all. A source with no
    /// window of its own has the two questions collapse into one, which is this default.
    ///
    /// **The resolver reads this, not one caller.**
    /// [`crate::cell::CellResolver::get_visible`] asks it on every outdoor id, which is where
    /// the retail check sits: below the visible-cell lookup and therefore below every
    /// transition, cell-array build, collision sweep and `find_cell_list` in the client. Asked
    /// only at the client seam (`ObjectPhysics::place`), a *transition* toward a block the
    /// window had released would still resolve cells there; this is the one place it is asked.
    fn landblock_resident(&self, id: LandblockId) -> bool {
        self.landblock(id).is_some()
    }

    /// The 256-entry land height table from the Region file. Must already have been validated:
    /// rejects the table if any entry is negative or
    /// greater than 800.
    fn height_table(&self) -> &[f32; LAND_HEIGHT_TABLE_LEN];

    /// Env cells for an interior landblock, keyed by the full cell id.
    ///
    /// This is the visible-only cell lookup, not the loading lookup: it never loads. Almost all
    /// physics uses the visible-only form, which is why physics silently stops at the edge of the
    /// loaded window instead of stalling on disk I/O.
    fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>>;

    /// The interior cells reachable from a **land** cell through a building standing in it.
    ///
    /// A land cell records at most one building: the one whose frame origin lies in that
    /// cell. Each of its portals names the interior cell behind it, and the portal walk
    /// visits those cells.
    /// This returns exactly those cell ids; the transit test itself is
    /// [`crate::cell::CellResolver::find_cell_list`]'s, because it needs the spheres.
    ///
    /// The default is empty, which is a world with no buildings — every existing implementation and
    /// every test in this crate is such a world, and stays byte-identical.
    fn building_cells(&self, _cell: CellId) -> Vec<CellId> {
        Vec::new()
    }

    /// The building standing in a **land** cell, or `None` if there is no building.
    ///
    /// A land cell keeps the first building offered, whose frame origin lies in that cell.
    /// Its building-collision arm checks this field for absence, then searches the
    /// building's collision geometry when present.
    ///
    /// This is the shell — the outer walls you walk into from the street. The *interior* cells
    /// behind the building's portals are [`LandSource::building_cells`]; the two are separate
    /// because the shell is geometry and the interiors are cell-list expansion.
    ///
    /// The default is `None`, which is a world with no buildings — every existing implementation
    /// and every existing test in this crate is such a world, and stays byte-identical.
    fn building(&self, _cell: CellId) -> Option<Arc<BuildingGeometry>> {
        None
    }
}

/// A `LandSource` backed by an explicit map. Used by every test in the crate, and a perfectly
/// serviceable production implementation for a fixed world slice.
#[derive(Debug)]
pub struct StaticLandSource {
    blocks: BTreeMap<u16, Arc<LandblockCollision>>,
    cells: BTreeMap<u32, Arc<EnvCellGeometry>>,
    /// Buildings keyed by the land cell to which each was added.
    buildings: BTreeMap<u32, Arc<BuildingGeometry>>,
    table: Box<[f32; LAND_HEIGHT_TABLE_LEN]>,
}

impl Default for StaticLandSource {
    fn default() -> Self {
        Self::linear()
    }
}

impl StaticLandSource {
    #[must_use]
    pub fn new(table: [f32; LAND_HEIGHT_TABLE_LEN]) -> Self {
        Self {
            blocks: BTreeMap::new(),
            cells: BTreeMap::new(),
            buildings: BTreeMap::new(),
            table: Box::new(table),
        }
    }

    /// A source with the linear `2 * i` height table, which is what the retail table is for its
    /// first 201 entries. Convenient for a synthetic world.
    #[must_use]
    pub fn linear() -> Self {
        let mut t = [0.0_f32; LAND_HEIGHT_TABLE_LEN];
        for (i, v) in t.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            {
                *v = i as f32 * 2.0;
            }
        }
        Self::new(t)
    }

    pub fn add_block(&mut self, b: LandblockCollision) {
        self.blocks.insert(b.id.0, Arc::new(b));
    }

    pub fn add_cell(&mut self, c: EnvCellGeometry) {
        self.cells.insert(c.id.0, Arc::new(c));
    }

    /// **The first one wins.** A land cell holds at most one building; a second offer is
    /// discarded rather than stacked.
    pub fn add_building(&mut self, cell: CellId, b: BuildingGeometry) {
        self.buildings.entry(cell.0).or_insert_with(|| Arc::new(b));
    }

    /// A perfectly flat landblock at the given height index — the simplest possible world, and
    /// the one every integration-style test in this crate stands on.
    pub fn add_flat_block(&mut self, id: LandblockId, height_index: u8) {
        let block = LandblockCollision::build(
            id,
            Box::new([height_index; VERTEX_COUNT]),
            Box::new([0; VERTEX_COUNT]),
            false,
            8,
            &self.table,
        )
        .expect("a full-detail block");
        self.add_block(block);
    }

    /// A landblock whose height rises linearly in +X by `rise` table indices per vertex column,
    /// giving a constant slope. Used for the walkable-slope tests.
    pub fn add_ramp_block(&mut self, id: LandblockId, base: u8, rise: u8) {
        let mut h = [base; VERTEX_COUNT];
        for i in 0..9_usize {
            for j in 0..9_usize {
                #[allow(clippy::cast_possible_truncation)]
                {
                    h[i * 9 + j] = base.saturating_add((i as u8).saturating_mul(rise));
                }
            }
        }
        let block = LandblockCollision::build(
            id,
            Box::new(h),
            Box::new([0; VERTEX_COUNT]),
            false,
            8,
            &self.table,
        )
        .expect("a full-detail block");
        self.add_block(block);
    }
}

impl LandSource for StaticLandSource {
    fn landblock(&self, id: LandblockId) -> Option<Arc<LandblockCollision>> {
        self.blocks.get(&id.0).cloned()
    }
    fn height_table(&self) -> &[f32; LAND_HEIGHT_TABLE_LEN] {
        &self.table
    }
    fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>> {
        self.cells.get(&cell.0).cloned()
    }
    fn building(&self, cell: CellId) -> Option<Arc<BuildingGeometry>> {
        self.buildings.get(&cell.0).cloned()
    }
}

/// A `Position` helper the world uses when it has to synthesise one.
#[must_use]
pub fn position_at(cell: CellId, origin: Vec3) -> Position {
    Position::new(cell, Frame::new(origin, dereth_primitives::Quat::IDENTITY))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_source_produces_a_walkable_block() {
        let mut s = StaticLandSource::linear();
        let id = LandblockId::new(0xA9, 0xB4);
        s.add_flat_block(id, 10);
        let b = s.landblock(id).expect("the block is resident");
        assert!(b.vertices.iter().all(|v| v.z == 20.0));
        assert!(b.polygons.iter().all(|p| p.plane.normal.z > 0.99));
        assert!(s.landblock(LandblockId::new(0, 0)).is_none());
    }

    #[test]
    fn a_ramp_block_has_a_constant_slope_in_x() {
        let mut s = StaticLandSource::linear();
        let id = LandblockId::new(1, 1);
        // 2 table steps per 24 m column = 4 m rise per 24 m run.
        s.add_ramp_block(id, 0, 2);
        let b = s.landblock(id).expect("resident");
        assert_eq!(b.vertices[0].z, 0.0);
        assert_eq!(b.vertices[9].z, 4.0, "one column east is 4 m higher");
        let n = b.polygons[0].plane.normal;
        // cos of the slope angle: 24 / sqrt(24^2 + 4^2)
        let expect = 24.0_f32 / (24.0_f32 * 24.0 + 16.0).sqrt();
        assert!((n.z - expect).abs() < 1e-4, "{n:?} vs {expect}");
    }

    #[test]
    fn the_dummy_setup_carries_exactly_the_static_initialiser_sphere() {
        let d = SetupGeometry::dummy();
        assert_eq!(d.spheres.len(), 1);
        assert_eq!(d.spheres[0], Sphere::dummy());
        assert_eq!(d.path_spheres().len(), 1);
    }

    #[test]
    fn path_spheres_is_clamped_to_two() {
        let g = SetupGeometry {
            spheres: vec![Sphere::dummy(), Sphere::dummy(), Sphere::dummy()],
            ..SetupGeometry::default()
        };
        assert_eq!(
            g.path_spheres().len(),
            2,
            "sphere-path initialization stores min(n, 2)"
        );
    }
}
