//! The world's side of detection: who watches each cell, the objects that stand in each cell (each
//! object in the one cell it stands in, not the others its shape reaches), the lines of sight the
//! cells trace, and the host a body's detections ask about the world ([`Detection`]).
//!
//! A body watches a cell through one of its detections; its watch is counted, so a second
//! detection watching the same cell adds to the count and tells the body nothing, and only the
//! last to stop ends the watch. The first watch tells the body of each object already standing in
//! the cell, as entering it; the last tells it of each as leaving. Whenever an object comes to
//! stand in a cell, leaves it (moving into another, or out of the world) or is hidden or shown,
//! every body watching the cell is told, except the object itself; an undetectable object or one
//! that never moves is never told of. What the bodies are told waits in the world for the host
//! ([`World::take_detection_updates`]).
//!
//! A line of sight starts in a cell and is traced through it the way the cell traces it: inside,
//! room by room, each stopping it at the solid side of its own walls, floors and ceilings and going
//! on through its doorways into the room beyond or out onto the land; outside, landblock by
//! landblock across the land cells, cut where it leaves each one, each cell stopping it at its ground
//! and going in through the doorways of the rooms that touch it, stopping at the edge of a landblock
//! that is not loaded. What stands in the world stops a line unless it is a kind the line passes
//! through, or one the line names as passed through; a line that asks for the surroundings only
//! passes every object.

use std::collections::{BTreeMap, BTreeSet};

use dereth_physics::arena::PhysHandle;
use dereth_primitives::shape::Sphere;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use super::{GameObjects, World, STATIC_ETHEREAL};
use crate::body::ENVIRONMENT_ONLY;
use crate::cells::{self, adjust_to_outside};
use crate::collision::manager::BodyKey;
use crate::detect::manager::{status, DetectionHost};
use crate::detect::{
    BlockRays, Detected, Detector, DetectorSettings, ObjectRays, ReportType, Sighting, Sightings,
};
use crate::ethereal::SightLine;
use crate::geom::{self, cover, BBox, Ray};
use crate::interior::is_interior;
use crate::land::{
    land_cell_id, solids_hit_in_cell, Block, Land, SolidNear, CELLS_PER_SIDE, CELL_LENGTH,
};
use crate::solid;

/// The game objects for a host that has none: an object with no game object is of every kind a
/// detection reports, and nothing is noted of any (an object's own note starts its sighting over
/// with its own id, which the sighting already has).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoGameObjects;

impl GameObjects for NoGameObjects {
    // D2-06131
    fn matches(&self, _object: u64, _report: &ReportType) -> bool {
        true
    }
    // D2-08064
    fn note(&self, _object: u64, _sighting: &mut Sighting) {}
}

/// What a line of sight traces: the physics shapes of the surroundings and of what stands in them.
pub(crate) const PHYSICS_SHAPES: u32 = 0;
/// What a line of sight traces: the drawn surfaces.
const DRAWN_SURFACES: u32 = 1;
/// What a line of sight traces: doorways only, the second of these going through those that lead
/// into rooms.
const PORTALS_ONLY: [u32; 2] = [3, DOORWAYS_INTO_ROOMS];
/// What a line of sight traces: doorways only, going through those that lead into rooms.
const DOORWAYS_INTO_ROOMS: u32 = 4;
/// More landblocks than any line crosses; a guard against one that never ends.
const MOST_BLOCKS: usize = 1024;
/// The number a whole landblock is filed under.
const BLOCK_CELL: u32 = 0xFFFF;

/// One body watching a cell: the kinds of object it reports, and how many of its detections watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Watcher {
    report: ReportType,
    count: i32,
}

/// What the world keeps for detection.
#[derive(Debug, Default)]
pub(super) struct Watching {
    /// Each cell's watchers, by body id.
    watchers: BTreeMap<u32, BTreeMap<u64, Watcher>>,
    /// Each cell's own objects, in the order they came.
    objects: BTreeMap<u32, Vec<BodyKey>>,
    /// The cell each entry stands in.
    cell_of: BTreeMap<BodyKey, u32>,
    undetectable: BTreeSet<BodyKey>,
    /// Each entry's setup radius and height, unscaled.
    sizes: BTreeMap<BodyKey, (f32, f32)>,
    /// The world's stamp each object was last taken by a search at, so an object reached through
    /// several cells is taken once a search.
    stamps: BTreeMap<BodyKey, u64>,
    /// What the watchers were told and have not yet been handed.
    updates: Vec<(u64, Detected)>,
}

fn nowhere() -> Position {
    Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY))
}

fn at(cell: u32, origin: Vec3) -> Position {
    Position::new(CellId(cell), Frame::new(origin, Quat::IDENTITY))
}

#[allow(clippy::cast_possible_truncation)]
fn narrow(v: f64) -> f32 {
    v as f32
}

/// Whether cell `cell` is loaded: its landblock for a land cell, the cell itself for an interior
/// one.
pub(super) fn cell_loaded(land: &Land, cell: CellId) -> bool {
    if cell.0 == 0 {
        return false;
    }
    if is_interior(cell) {
        return land.cell(cell).is_some();
    }
    let lb = cell.landblock();
    land.block(lb.x(), lb.y()).is_some()
}

/// Whether landblock `block` is loaded: its land, or any of its interior cells.
fn block_loaded(land: &Land, block: u32) -> bool {
    let lb = CellId(block).landblock();
    land.block(lb.x(), lb.y()).is_some() || !land.interior_cells_of(block & 0xFFFF_0000).is_empty()
}

fn end_of(r: &Ray) -> Vec3 {
    Vec3::new(
        r.length * r.dir.x + r.point.x,
        r.length * r.dir.y + r.point.y,
        r.length * r.dir.z + r.point.z,
    )
}

/// The interior cell of landblock `block` that point `p` of its frame lies in.
// D2-03621
fn find_interior_cell(land: &Land, block: u32, p: Vec3) -> Option<u32> {
    land.interior_cells_of(block & 0xFFFF_0000)
        .into_iter()
        .find(|id| {
            land.cell(*id).is_some_and(|g| {
                let local = dereth_physics::math::globaltolocal(&g.frame, p);
                g.cell_bsp
                    .as_ref()
                    .is_none_or(|b| b.point_inside_cell_bsp(local))
            })
        })
        .map(|id| id.0)
}

/// The pieces of `ray`, which starts in landblock `from` (in its frame), in each landblock it
/// crosses, each in that landblock's frame; past the map's edge the landblock is 0. A ray that does
/// not start inside its landblock gives one piece that is no part of it (the client's own does
/// the same).
#[must_use]
pub fn split_ray(ray: &Ray, from: u32) -> Vec<(u32, Ray)> {
    let mut out = Vec::new();
    let mut rest = *ray;
    let mut piece = Ray::default();
    let mut block = from;
    for _ in 0..MOST_BLOCKS {
        let mut dir = 0;
        let crossed = cells::block_ray_clip(&mut rest, &mut piece, &mut dir);
        out.push((block, piece));
        if !crossed {
            break;
        }
        block = if block == 0 {
            0
        } else {
            cells::block_beside(block, dir)
        };
    }
    out
}

impl World {
    /// Makes entry `key` undetectable, or detectable again. Hiding an entry tells its cell's
    /// watchers it left, then hides it; showing it shows it, then tells them it entered. Each call
    /// tells them, as the client's does, so call it when the object's own flag is set.
    pub fn set_undetectable(&mut self, key: BodyKey, on: bool) {
        if !self.entries.contains_key(&key) {
            return;
        }
        let cell = self.watching.cell_of.get(&key).copied();
        if on {
            if let Some(c) = cell {
                self.hide_object(c, key);
            }
            self.watching.undetectable.insert(key);
        } else {
            self.watching.undetectable.remove(&key);
            if let Some(c) = cell {
                self.unhide_object(c, key);
            }
        }
    }

    #[must_use]
    pub fn is_undetectable(&self, key: BodyKey) -> bool {
        self.watching.undetectable.contains(&key)
    }

    /// The radius and height of entry `key`'s setup, unscaled; both 0 until the host gives them.
    pub fn set_size(&mut self, key: BodyKey, radius: f32, height: f32) {
        if self.entries.contains_key(&key) {
            self.watching.sizes.insert(key, (radius, height));
        }
    }

    /// Entry `key`'s radius: its setup's, by its height scale.
    #[must_use]
    pub fn radius(&self, key: BodyKey) -> f32 {
        let (Some(e), Some((r, _))) = (self.entries.get(&key), self.watching.sizes.get(&key))
        else {
            return 0.0;
        };
        r * e.shape.scale.z
    }

    /// Entry `key`'s height: its setup's, by its height scale.
    #[must_use]
    pub fn height(&self, key: BodyKey) -> f32 {
        let (Some(e), Some((_, h))) = (self.entries.get(&key), self.watching.sizes.get(&key))
        else {
            return 0.0;
        };
        h * e.shape.scale.z
    }

    /// The cell entry `key` stands in, as its watchers know it.
    #[must_use]
    pub fn own_cell(&self, key: BodyKey) -> Option<CellId> {
        self.watching.cell_of.get(&key).map(|c| CellId(*c))
    }

    /// Takes what the cells' watchers were told since last asked: each the watching body's id and
    /// the change, in the order told.
    pub fn take_detection_updates(&mut self) -> Vec<(u64, Detected)> {
        std::mem::take(&mut self.watching.updates)
    }

    /// The world as the detections of the body at `key` see it, the game objects saying which
    /// objects a detection is after.
    pub fn detection<'a>(
        &'a mut self,
        land: &'a Land,
        key: BodyKey,
        game: &'a dyn GameObjects,
    ) -> Detection<'a> {
        Detection {
            world: self,
            land,
            key,
            game,
        }
    }

    /// Entry `key` now stands in `cell` (0: none): out of the cell it stood in when that is
    /// another, into the new one.
    // D2-05970, D2-05971, D2-05968
    pub(super) fn refile(&mut self, key: BodyKey, cell: CellId) {
        let old = self.watching.cell_of.get(&key).copied();
        if old == Some(cell.0) {
            return;
        }
        if let Some(c) = old {
            self.watching.cell_of.remove(&key);
            self.remove_object(c, key);
        }
        if cell.0 != 0 {
            self.watching.cell_of.insert(key, cell.0);
            self.add_object(cell.0, key);
        }
    }

    /// Entry `key` leaves the world: out of the cell it stood in, and forgotten.
    // D2-05972
    pub(super) fn unfile(&mut self, key: BodyKey) {
        if let Some(c) = self.watching.cell_of.remove(&key) {
            self.remove_object(c, key);
        }
        self.watching.undetectable.remove(&key);
        self.watching.sizes.remove(&key);
        self.watching.stamps.remove(&key);
    }

    fn is_static(&self, key: BodyKey) -> bool {
        self.entries.get(&key).is_some_and(|e| e.is_static)
    }

    /// Entry `key` comes to stand in `cell`: listed among its objects, and each watcher told.
    // D2-03885
    fn add_object(&mut self, cell: u32, key: BodyKey) {
        self.watching.objects.entry(cell).or_default().push(key);
        self.update_all_watchers(cell, key, status::ENTERED);
    }

    /// Entry `key` no longer stands in `cell`: the last of the cell's objects takes its place in
    /// the list, and each watcher is told. Nothing when it was not there.
    // D2-03886
    fn remove_object(&mut self, cell: u32, key: BodyKey) {
        let Some(list) = self.watching.objects.get_mut(&cell) else {
            return;
        };
        let Some(i) = list.iter().position(|k| *k == key) else {
            return;
        };
        list.swap_remove(i);
        self.update_all_watchers(cell, key, status::LEFT);
    }

    // D2-03882
    fn hide_object(&mut self, cell: u32, key: BodyKey) {
        self.update_all_watchers(cell, key, status::LEFT);
    }

    // D2-03883
    fn unhide_object(&mut self, cell: u32, key: BodyKey) {
        self.update_all_watchers(cell, key, status::ENTERED);
    }

    /// Tells every watcher of `cell` that entry `key` entered or left it; nothing for an
    /// undetectable object or one that never moves.
    // D2-03881
    fn update_all_watchers(&mut self, cell: u32, key: BodyKey, change: u32) {
        if self.is_undetectable(key) || self.is_static(key) {
            return;
        }
        let watchers: Vec<u64> = self
            .watching
            .watchers
            .get(&cell)
            .map(|w| w.keys().copied().collect())
            .unwrap_or_default();
        for w in watchers {
            self.send_object_detection(w, key, change);
        }
    }

    /// Tells body `watcher` that entry `key` entered or left a cell it watches: not of itself, not
    /// of an undetectable object or one that never moves, and only while the body is in the world.
    // D2-03844
    fn send_object_detection(&mut self, watcher: u64, key: BodyKey, change: u32) {
        if watcher == 0 || !self.entries.contains_key(&key) {
            return;
        }
        let object = self.object_id(key);
        if object == watcher || self.is_undetectable(key) || self.is_static(key) {
            return;
        }
        if self.find(watcher).is_none() {
            return;
        }
        self.watching.updates.push((
            watcher,
            Detected {
                object,
                status: change,
            },
        ));
    }

    /// A cell let go of forgets who watched it.
    // D2-03854, D2-03514
    pub fn release_cell_watchers(&mut self, cell: CellId) {
        self.watching.watchers.remove(&cell.0);
    }

    /// Body `watcher` watches `cell` for objects of the kinds `report` names: a watch it already
    /// has is counted again and tells it nothing; a new one tells it of each object standing in the
    /// cell, as entering. False for no body.
    // D2-03896
    pub fn add_watcher(&mut self, cell: CellId, watcher: u64, report: ReportType) -> bool {
        if watcher == 0 {
            return false;
        }
        let watchers = self.watching.watchers.entry(cell.0).or_default();
        if let Some(w) = watchers.get_mut(&watcher) {
            w.count += 1;
            return true;
        }
        watchers.insert(watcher, Watcher { report, count: 1 });
        let objects = self
            .watching
            .objects
            .get(&cell.0)
            .cloned()
            .unwrap_or_default();
        for k in objects {
            self.send_object_detection(watcher, k, status::ENTERED);
        }
        true
    }

    /// One of body `watcher`'s watches of `cell` ends; the last tells it of each object standing in
    /// the cell, as leaving. False when it did not watch the cell.
    // D2-03887
    pub fn remove_watcher(&mut self, cell: CellId, watcher: u64) -> bool {
        let Some(w) = self
            .watching
            .watchers
            .get_mut(&cell.0)
            .and_then(|m| m.get_mut(&watcher))
        else {
            return false;
        };
        w.count -= 1;
        if w.count < 1 {
            let objects = self
                .watching
                .objects
                .get(&cell.0)
                .cloned()
                .unwrap_or_default();
            for k in objects {
                self.send_object_detection(watcher, k, status::LEFT);
            }
            if let Some(m) = self.watching.watchers.get_mut(&cell.0) {
                m.remove(&watcher);
            }
        }
        true
    }

    /// How many of body `watcher`'s detections watch `cell`; 0 when it does not watch it.
    #[must_use]
    pub fn watch_count(&self, cell: CellId, watcher: u64) -> i32 {
        self.watching
            .watchers
            .get(&cell.0)
            .and_then(|m| m.get(&watcher))
            .map_or(0, |w| w.count)
    }

    /// The bodies watching `cell` that watch for entry `key`.
    // D2-03892
    #[must_use]
    pub fn watchers_of(&self, cell: CellId, key: BodyKey) -> Vec<u64> {
        self.watching
            .watchers
            .get(&cell.0)
            .map(|m| {
                m.keys()
                    .copied()
                    .filter(|w| self.watched_by(cell, key, *w))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether body `watcher`, watching `cell`, watches for entry `key`: never for itself, an
    /// undetectable object or one that never moves; else as the object answers for the kinds the
    /// watcher reports, and in this build every object answers no.
    // D2-03888
    #[must_use]
    pub fn watched_by(&self, cell: CellId, key: BodyKey, watcher: u64) -> bool {
        if self.is_undetectable(key) || self.is_static(key) {
            return false;
        }
        let Some(m) = self.watching.watchers.get(&cell.0) else {
            return false;
        };
        if self.object_id(key) == watcher {
            return false;
        }
        m.get(&watcher).is_some_and(|w| reports_on(&w.report))
    }

    /// Takes `key`'s turn in the current search: false when it already had it.
    fn restamp(&mut self, key: BodyKey) -> bool {
        let s = self.physics_stamp;
        self.watching.stamps.insert(key, s) != Some(s)
    }

    /// Entry `key` as a detection that targets it notes it: an undetectable object never; unless
    /// `again`, once a search; noted among `sightings` from the detection's origin.
    // D2-06343
    pub fn add_to_intersection_targeted(
        &mut self,
        key: BodyKey,
        settings: &DetectorSettings,
        sightings: &mut Sightings,
        again: bool,
        game: &dyn GameObjects,
    ) {
        if self.is_undetectable(key) {
            return;
        }
        if !again && !self.restamp(key) {
            return;
        }
        let origin = *settings.origin_point();
        self.add_to_target_list(key, &origin, sightings, game);
    }

    /// Entry `key` as a detection's search reaches it: once a search, not undetectable, of a kind
    /// the detection is after and inside its volume, it is noted among `sightings`, and given
    /// rays toward it in `rays` when lines of sight are wanted.
    // D2-06342
    pub fn add_to_intersection(
        &mut self,
        key: BodyKey,
        settings: &DetectorSettings,
        sightings: &mut Sightings,
        rays: Option<&mut BlockRays>,
        game: &dyn GameObjects,
    ) {
        if self.is_undetectable(key) || !self.restamp(key) {
            return;
        }
        if !self.check_type_match(key, settings, game) || !self.within_detect_volume(key, settings)
        {
            return;
        }
        let origin = *settings.origin_point();
        self.add_to_target_list(key, &origin, sightings, game);
        if let Some(r) = rays {
            self.add_to_los_list(key, &origin, r, settings.los_resolution);
        }
    }

    /// Rays from `origin` covering entry `key`, kept in `block` unless it already has the object:
    /// one straight to an object with no shape; to its spheres when it has any; else to the box
    /// around its physics.
    // D2-06251
    pub fn add_to_los_list(
        &self,
        key: BodyKey,
        origin: &Position,
        block: &mut BlockRays,
        resolution: u32,
    ) -> bool {
        let id = self.object_id(key);
        if block.objects.contains_key(&id) {
            return true;
        }
        let (Some(e), Some(place)) = (self.entries.get(&key), self.position(key)) else {
            return true;
        };
        let s = &e.shape;
        let mut o = ObjectRays::new(id);
        let present = !s.spheres.is_empty() || !s.cylinders.is_empty() || s.has_physics_tree();
        if !present {
            o.add_ray(cover::create_ray(origin, &place, Vec3::ZERO));
        } else if s.spheres.is_empty() {
            cover::box_cover_rays(
                &s.physics_bounding_box(),
                &place,
                origin,
                resolution,
                &mut o.rays,
            );
        } else {
            for sphere in &s.spheres {
                cover::sphere_cover_rays(sphere, &place, origin, resolution, &mut o.rays);
            }
        }
        block.add_object(o)
    }

    /// What landblock search finds for a detection: every object standing in a loaded cell the
    /// box `area` around its volume reaches from its origin's cell, each taken once.
    // D2-03671
    pub fn intersect_block(
        &mut self,
        land: &Land,
        settings: &DetectorSettings,
        area: &BBox,
        sightings: &mut Sightings,
        mut rays: Option<&mut BlockRays>,
        game: &dyn GameObjects,
    ) {
        self.increment_physics_stamp();
        let origin = *settings.origin_point();
        for cell in cells::find_cells(land, origin.cell, *area) {
            if cell_loaded(land, cell) {
                self.intersect_cell(cell, settings, sightings, rays.as_deref_mut(), game);
            }
        }
    }

    /// Every object whose shape reaches `cell`, for a detection's search.
    // D2-03859
    fn intersect_cell(
        &mut self,
        cell: CellId,
        settings: &DetectorSettings,
        sightings: &mut Sightings,
        mut rays: Option<&mut BlockRays>,
        game: &dyn GameObjects,
    ) {
        let handles: Vec<PhysHandle> = self
            .cells
            .get(&cell.0)
            .map(|c| c.shadow_object_list.clone())
            .unwrap_or_default();
        for h in handles {
            if let Some(key) = self.key_of(h) {
                self.add_to_intersection(key, settings, sightings, rays.as_deref_mut(), game);
            }
        }
    }

    /// Casts the rays toward each object of one landblock: each from the interior cell its start
    /// lies in, or else the land cell, with the object itself passed through; its coverage is the
    /// share of its rays that did not get through.
    // D2-03683
    pub fn detect_block(&mut self, land: &Land, settings: &DetectorSettings, rays: &mut BlockRays) {
        self.increment_physics_stamp();
        if rays.objects.is_empty() {
            return;
        }
        let mut los = SightLine::default();
        los.set_intersect_type(PHYSICS_SHAPES);
        let block = rays.block;
        for (id, o) in &mut rays.objects {
            let mut objects = settings.object_blocking.clone();
            objects.add_to_ethereal_list(*id);
            los.set_los_params(&objects, settings.physics_blocking);
            let mut clear = 0_u32;
            for ray in &o.rays {
                let cell = find_interior_cell(land, block, ray.point).unwrap_or_else(|| {
                    let (mut c, mut p) = (CellId(block), ray.point);
                    if adjust_to_outside(&mut c, &mut p) {
                        c.0
                    } else {
                        0
                    }
                });
                los.set_ray(*ray, cell);
                if self.los_check(land, &mut los) && los.trace_result == 0 {
                    clear += 1;
                }
            }
            let n = u32::try_from(o.rays.len()).unwrap_or(u32::MAX);
            o.coverage = narrow(1.0 - f64::from(clear) / f64::from(n));
        }
    }

    /// A place on entry `key`: `rise` of its height up and `offset` (by its scale when `scaled`)
    /// from where it stands, turned with it, carried from there by a line of sight through the
    /// surroundings ([`crate::body::offset_position`]); none when that line has no loaded cell to
    /// start from.
    #[must_use]
    pub fn object_offset_position(
        &self,
        land: &Land,
        key: BodyKey,
        rise: f32,
        scaled: bool,
        offset: Vec3,
    ) -> Option<Position> {
        let e = self.entries.get(&key)?;
        let p = self.position(key)?;
        let mut traced = true;
        let (place, _) = crate::body::offset_position(
            &p,
            self.height(key),
            e.shape.scale,
            rise,
            scaled,
            offset,
            &mut |l| traced = self.los_check(land, l),
        );
        traced.then_some(place)
    }

    /// The objects standing in cell `cell` (whose own cell it is), only those that move when
    /// `moving`.
    // D2-03889, D2-06100
    #[must_use]
    pub fn cell_objects(&self, cell: CellId, moving: bool) -> Vec<u64> {
        self.watching
            .objects
            .get(&cell.0)
            .into_iter()
            .flatten()
            .filter_map(|k| self.entries.get(k).map(|e| (*k, e)))
            .filter(|(_, e)| !moving || !e.is_static)
            .map(|(k, _)| self.object_id(k))
            .collect()
    }

    /// Traces line of sight `los` from its first cell through the surroundings and what stands in
    /// them. False, the line untouched, when it has no first cell or that cell is not loaded.
    // D2-03851, D2-05961
    pub fn los_check(&self, land: &Land, los: &mut SightLine) -> bool {
        let cell = CellId(los.initial_cell);
        if cell.0 == 0 {
            return false;
        }
        los.reset_state();
        if !cell_loaded(land, cell) {
            return false;
        }
        if is_interior(cell) {
            self.trace_inside(land, cell, None, los, &mut Vec::new());
        } else {
            self.trace_outside(land, cell, los, &mut Vec::new());
        }
        true
    }

    /// How far along the line from `from` to `to` (in the landblock frame of `cell`) entry `key`
    /// first meets it, as a fraction, and what the line meets facing: a solid part placed in an
    /// interior cell by the polygons of its trees at its whole scale, anything else by its shape
    /// ([`crate::object::Shape::check_ray`]).
    fn entry_met(&self, key: BodyKey, cell: CellId, from: Vec3, to: Vec3) -> Option<(f32, Vec3)> {
        let e = self.entries.get(&key)?;
        let o = self.objects.get(e.handle)?;
        let shift = geom::block_offset(cell, o.position.cell);
        let a = Vec3::new(from.x - shift.x, from.y - shift.y, from.z - shift.z);
        let b = Vec3::new(to.x - shift.x, to.y - shift.y, to.z - shift.z);
        let frame = &o.position.frame;
        if e.id == 0 && e.shape.has_physics_tree() {
            e.shape.check_trees_facing(frame, e.tree_scale, a, b)
        } else {
            e.shape.check_ray_facing(frame, a, b)
        }
    }

    /// How far along the line from `from` to `to` (in the landblock frame of `cell`) what stands
    /// in `cell` first meets it, as a fraction, and what the line meets facing: the objects whose own
    /// cell it is and those that reach into it, each that blocks `los`.
    // D2-03863, D2-06132
    fn cell_objects_met(
        &self,
        cell: CellId,
        from: Vec3,
        to: Vec3,
        los: &SightLine,
    ) -> Option<(f32, Vec3)> {
        let own = self.watching.objects.get(&cell.0).into_iter().flatten();
        let reaching = self
            .cells
            .get(&cell.0)
            .into_iter()
            .flat_map(|c| c.shadow_object_list.iter())
            .filter_map(|&h| self.key_of(h));
        let mut best: Option<(f32, Vec3)> = None;
        for key in own.copied().chain(reaching) {
            if !self.is_los_blocking(key, los) {
                continue;
            }
            if let Some(hit) = self.entry_met(key, cell, from, to) {
                if best.is_none_or(|(x, _)| hit.0 < x) {
                    best = Some(hit);
                }
            }
        }
        best
    }

    /// Interior cell `cell`'s own trace of the line's current ray, which came in through the
    /// cell's doorway `came_through` (none for the cell the line starts in); `tracing` holds the
    /// cells whose traces are under way, none of which is traced again until it ends (it answers
    /// clear, the line untouched). The trace's result.
    ///
    /// The ray is taken into the cell's frame. Unless the line traces doorways only, the solid
    /// side of the cell's physics tree (the far side of its walls, floors and ceilings) stops it,
    /// and, tracing physics shapes, so does what stands in the cell; tracing the drawn surfaces,
    /// nothing of the cell's stops it here. Then, when the line goes through doorways, every
    /// doorway but the one it came in by is crossed from either face; the nearest of all wins.
    ///
    /// - Nothing met: the line ends in this cell, whole (its final ray the ray it came in with).
    /// - A doorway: unless the line traces doorways only (or traces doorways into rooms and this
    ///   one leads into a room), the line goes on through it: its current ray runs from the
    ///   doorway to its end, whole, and the cell beyond traces it (from the doorway it comes in by)
    ///   or, through a doorway out, the land cell the doorway lies over; that trace is the answer.
    ///   Where there is nothing loaded to go on to, the line ends in this cell, whole; tracing
    ///   doorways only, it stops at the doorway, facing out of its plane (in the cell's own frame).
    /// - A wall: the line stops there, facing back out of the solid side, turned into the world.
    /// - Something standing in the cell: the line stops at it, facing nowhere.
    // D2-03594, D2-03597
    fn trace_inside(
        &self,
        land: &Land,
        cell: CellId,
        came_through: Option<usize>,
        los: &mut SightLine,
        tracing: &mut Vec<u32>,
    ) -> u32 {
        if tracing.contains(&cell.0) {
            return 0;
        }
        let Some(g) = land.cell(cell) else {
            return 0;
        };
        tracing.push(cell.0);
        let result = self.trace_cell(land, &g, came_through, los, tracing);
        tracing.pop();
        result
    }

    /// [`Self::trace_inside`] once the cell is known and marked.
    fn trace_cell(
        &self,
        land: &Land,
        g: &dereth_physics::source::EnvCellGeometry,
        came_through: Option<usize>,
        los: &mut SightLine,
        tracing: &mut Vec<u32>,
    ) -> u32 {
        /// What stopped the ray in the cell.
        enum Met {
            Nothing,
            Doorway(usize),
            Wall,
            Object,
        }
        let cell = g.id;
        let came_with = los.current_ray;
        let start = came_with.point;
        let len = came_with.length;
        let d = came_with.dir;
        let end = Vec3::new(
            start.x + d.x * len,
            start.y + d.y * len,
            start.z + d.z * len,
        );
        let a = geom::point_to_local(&g.frame, end);
        let b = geom::point_to_local(&g.frame, start);
        let local = Ray::from_offset(b, Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z));
        let doorways_only = PORTALS_ONLY.contains(&los.intersect);
        let mut met = Met::Nothing;
        let mut best = los.delta;
        let mut wall_normal = Vec3::ZERO;
        if !doorways_only {
            if los.intersect != DRAWN_SURFACES {
                if let Some(tree) = g.physics_bsp.as_ref() {
                    let mut t = 0.0;
                    if crate::parts::trace_ray_from_root(tree, &local, &mut t, &mut wall_normal)
                        && t < best
                    {
                        best = t;
                        los.delta = t;
                        met = Met::Wall;
                    }
                }
            }
            if los.intersect == PHYSICS_SHAPES {
                if let Some((t, _)) = self
                    .cell_objects_met(cell, start, end, los)
                    .filter(|(t, _)| *t < los.delta)
                {
                    los.delta = t;
                    best = t;
                    met = Met::Object;
                }
            }
        }
        if los.traverse_portals {
            for (i, p) in g.portals.iter().enumerate() {
                if came_through == Some(i) {
                    continue;
                }
                if let Some(t) = geom::polygon_trace_ray(&p.portal, &local).filter(|t| *t < best) {
                    best = t;
                    met = Met::Doorway(i);
                }
            }
        }
        let normal = match met {
            Met::Nothing => {
                los.final_cell = cell.0;
                los.final_ray = came_with;
                los.impact_normal = Vec3::ZERO;
                los.trace_result = 0;
                return 0;
            }
            Met::Doorway(i) => {
                let p = &g.portals[i];
                let into_room = p.other_cell_id != cells::OUTDOORS;
                if (!doorways_only || (los.intersect == DOORWAYS_INTO_ROOMS && into_room))
                    && los.traverse_portals
                {
                    let at = Vec3::new(
                        (end.x - start.x) * best + start.x,
                        (end.y - start.y) * best + start.y,
                        (end.z - start.z) * best + start.z,
                    );
                    los.current_ray = Ray::segment(at, end);
                    los.delta = 1.0;
                    if into_room {
                        let other = CellId(p.other_cell_id);
                        if land.cell(other).is_some() {
                            let through = usize::try_from(p.other_portal_id).ok();
                            return self.trace_inside(land, other, through, los, tracing);
                        }
                    } else {
                        let (mut c, mut over) = (cell, at);
                        if adjust_to_outside(&mut c, &mut over) && cell_loaded(land, c) {
                            return self.trace_outside(land, c, los, tracing);
                        }
                    }
                }
                if !doorways_only {
                    los.final_cell = cell.0;
                    los.final_ray = came_with;
                    los.impact_normal = Vec3::ZERO;
                    los.trace_result = 0;
                    return 0;
                }
                los.portal_index = u32::try_from(i).unwrap_or(u32::MAX);
                los.delta = best;
                p.portal.plane.normal
            }
            Met::Wall => geom::local_to_global(&g.frame, wall_normal),
            Met::Object => Vec3::ZERO,
        };
        los.final_cell = cell.0;
        los.calc_final_ray();
        los.impact_normal = normal;
        los.trace_result = 1;
        1
    }

    /// Landblock `block`'s own trace of the line's current ray: each land cell in the box around the
    /// ray traces it in turn, x major. Stopped, the line ends where the last stop left it, reported
    /// in the last land cell of the box; else it ends in the cell the last room left it in, or else
    /// the land cell its end falls in, clear. The trace's result: 1 when stopped.
    // D2-03647
    fn trace_block(
        &self,
        land: &Land,
        block: u32,
        los: &mut SightLine,
        tracing: &mut Vec<u32>,
    ) -> u32 {
        let r = los.current_ray;
        let (from, to) = (r.point, end_of(&r));
        let lb = CellId(block).landblock();
        los.final_cell = 0;
        let mut stopped = false;
        if let Some(b) = land.block(lb.x(), lb.y()) {
            let last = i32::try_from(CELLS_PER_SIDE - 1).unwrap_or(0);
            let line = |v: f32| dereth_primitives::num::floor_to_i32(v / CELL_LENGTH);
            let range = |p: f32, q: f32| {
                let (lo, hi) = (line(p).min(line(q)), line(p).max(line(q)));
                (lo.clamp(0, last), hi.clamp(0, last))
            };
            let (x0, x1) = range(from.x, to.x);
            let (y0, y1) = range(from.y, to.y);
            let solids = self.solids_near(land, lb, from, to, los);
            for cx in x0..=x1 {
                for cy in y0..=y1 {
                    let at = (cx.unsigned_abs() as usize, cy.unsigned_abs() as usize);
                    if self.trace_land_cell(land, &b, at, &solids, los, tracing) == 1 {
                        stopped = true;
                    }
                }
            }
        }
        if stopped {
            los.calc_final_ray();
            return 1;
        }
        if los.final_cell == 0 {
            let (mut c, mut end) = (CellId(block), end_of(&los.current_ray));
            los.final_cell = if adjust_to_outside(&mut c, &mut end) {
                c.0
            } else {
                0
            };
        }
        los.trace_result = 0;
        los.calc_final_ray();
        0
    }

    /// The solid parts placed outside near the line from `from` to `to`, unless the line passes
    /// through them or traces the surroundings only.
    fn solids_near(
        &self,
        land: &Land,
        block: dereth_primitives::LandblockId,
        from: Vec3,
        to: Vec3,
        los: &SightLine,
    ) -> Vec<SolidNear> {
        if los.intersect != ENVIRONMENT_ONLY
            && solid::is_blocking(self.bit(STATIC_ETHEREAL), 0, &los.objects, &los.physics)
        {
            land.solids_near(block, from, to)
        } else {
            Vec::new()
        }
    }

    /// Land cell `(cx, cy)` of block `b`'s own trace of the line, the trace's result. Its ground and
    /// what stands in it stop the current ray where they meet it first. Then each room that touches
    /// the cell is asked, along the line as far as it now goes: every doorway out of the room the
    /// line crosses, ahead of its start, starts the current ray again from there, whole, and the room
    /// traces it from that doorway. Rooms already being traced answer clear.
    // D2-03836
    fn trace_land_cell(
        &self,
        land: &Land,
        b: &Block,
        (cx, cy): (usize, usize),
        solids: &[SolidNear],
        los: &mut SightLine,
        tracing: &mut Vec<u32>,
    ) -> u32 {
        let id = land_cell_id(b.x, b.y, cx, cy);
        los.current_cell = id.0;
        let mut result = 0;
        let mut best = los.delta;
        let ray = los.current_ray;
        for poly in b.cell_triangles(cx, cy) {
            if let Some(t) = geom::polygon_trace_ray(&poly, &ray).filter(|t| *t < best) {
                best = t;
                los.delta = t;
                los.trace_result = 1;
                los.impact_normal = poly.plane.normal;
                result = 1;
            }
        }
        // A line that traces doorways asks what stands here, and nothing answers it.
        if los.intersect == PHYSICS_SHAPES || los.intersect == DRAWN_SURFACES {
            let (from, to) = (ray.point, end_of(&ray));
            let mut met = self.cell_objects_met(id, from, to, los);
            for (t, n) in solids_hit_in_cell(solids, (cx, cy), from, to) {
                if met.is_none_or(|(m, _)| t < m) {
                    met = Some((t, n));
                }
            }
            if let Some((t, n)) = met.filter(|(t, _)| *t < los.delta) {
                los.delta = t;
                los.impact_normal = n;
                los.final_cell = id.0;
                los.trace_result = 1;
                result = 1;
            }
        }
        let (start, end) = if los.trace_result == 0 {
            (ray.point, end_of(&ray))
        } else {
            los.final_cell = id.0;
            los.calc_final_ray();
            (los.final_ray.point, end_of(&los.final_ray))
        };
        for room in land.touching_cells(id) {
            let Some(g) = land.cell(room) else {
                continue;
            };
            let (a, c) = (
                geom::point_to_local(&g.frame, start),
                geom::point_to_local(&g.frame, end),
            );
            let local = Ray::from_offset(a, Vec3::new(c.x - a.x, c.y - a.y, c.z - a.z));
            for (i, p) in g.portals.iter().enumerate() {
                if p.other_cell_id != cells::OUTDOORS {
                    continue;
                }
                let Some(t) = geom::polygon_trace_ray(&p.portal, &local).filter(|t| *t > 0.0)
                else {
                    continue;
                };
                let at = Vec3::new(
                    start.x + (end.x - start.x) * t,
                    start.y + (end.y - start.y) * t,
                    start.z + (end.z - start.z) * t,
                );
                let left = Vec3::new(end.x - at.x, end.y - at.y, end.z - at.z);
                los.current_ray = Ray::from_offset(at, left);
                los.delta = 1.0;
                let r = self.trace_inside(land, room, Some(i), los, tracing);
                if r != 0 {
                    los.trace_result = r;
                    result = r;
                }
            }
        }
        result
    }

    /// Land cell `this`'s own trace of the line, the trace's result. The line's start is first
    /// brought into the land cell it falls in, reckoned from `this`: when that is another cell, the
    /// line starts over from there (its whole first ray, in that cell). A line straight up or down
    /// stays in its first cell; any other is traced from its current ray landblock by landblock,
    /// cut where it leaves each, until it is stopped or ends, or reaches the edge of a landblock
    /// that is not loaded (2: it ends at that edge). A line that comes out through a doorway has
    /// its current ray from the doorway, so it goes on from there, unless the start it was traced
    /// from inside lies over another land cell than the doorway does. Traced once until the line
    /// starts over (clear, untouched, after that); 2, the line ending in its first cell, when there
    /// is nothing loaded to trace it in.
    // D2-03840, D2-03692
    fn trace_outside(
        &self,
        land: &Land,
        this: CellId,
        los: &mut SightLine,
        tracing: &mut Vec<u32>,
    ) -> u32 {
        if los.checked_outside {
            return 0;
        }
        los.checked_outside = true;
        let (mut cell, mut start) = (this, los.initial_ray.point);
        let _ = adjust_to_outside(&mut cell, &mut start);
        if cell != this {
            let r = los.initial_ray;
            let reach = Vec3::new(r.length * r.dir.x, r.length * r.dir.y, r.length * r.dir.z);
            los.set_ray(Ray::from_offset(start, reach), cell.0);
            los.current_ray = los.initial_ray;
        }
        let cur = los.current_ray;
        if cur.dir.x == 0.0 && cur.dir.y == 0.0 {
            let id = CellId(los.initial_cell);
            los.final_cell = id.0;
            if !cell_loaded(land, id) {
                return 2;
            }
            let lb = id.landblock();
            los.trace_result = match land.block(lb.x(), lb.y()) {
                Some(b) => {
                    let k = usize::from(id.index()).saturating_sub(1);
                    let place = (k / CELLS_PER_SIDE, k % CELLS_PER_SIDE);
                    let solids = self.solids_near(land, lb, cur.point, end_of(&cur), los);
                    self.trace_land_cell(land, &b, place, &solids, los, tracing)
                }
                None => 0,
            };
            los.final_cell = id.0;
            los.calc_final_ray();
            return los.trace_result;
        }
        let mut block = cell.0 & 0xFFFF_0000 | BLOCK_CELL;
        if !block_loaded(land, block) {
            los.final_cell = los.initial_cell;
            return 2;
        }
        for _ in 0..MOST_BLOCKS {
            let mut rest = los.current_ray;
            let mut piece = Ray::default();
            let mut dir = 0;
            if !cells::block_ray_clip(&mut rest, &mut piece, &mut dir) {
                los.current_ray = rest;
                los.delta = 1.0;
                los.trace_result = self.trace_block(land, block, los, tracing);
                return los.trace_result;
            }
            let next = cells::block_beside(block, dir);
            los.current_ray = piece;
            los.delta = 1.0;
            let r = self.trace_block(land, block, los, tracing);
            if r != 0 {
                los.trace_result = r;
                return r;
            }
            if next == 0 || !block_loaded(land, next) {
                let (mut c, mut end) = (CellId(block), piece.end_point());
                let _ = adjust_to_outside(&mut c, &mut end);
                los.final_cell = c.0;
                let first = at(los.initial_cell, los.initial_ray.point);
                let off = geom::offset(&first, &at(c.0, end));
                los.final_ray = Ray::from_offset(los.initial_ray.point, off);
                los.next_block = next;
                los.direction = dir;
                los.trace_result = 2;
                return 2;
            }
            los.current_ray = rest;
            los.delta = 1.0;
            block = next;
        }
        los.trace_result
    }
}

/// Whether an object reports itself to a watcher after the kinds `report` names: in this build
/// every object answers no.
fn reports_on(_report: &ReportType) -> bool {
    false
}

/// The world as the detections of one body see it ([`World::detection`]).
pub struct Detection<'a> {
    world: &'a mut World,
    land: &'a Land,
    key: BodyKey,
    game: &'a dyn GameObjects,
}

impl std::fmt::Debug for Detection<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Detection")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl DetectionHost for Detection<'_> {
    fn id(&self) -> u64 {
        self.world.object_id(self.key)
    }

    fn position(&self) -> Position {
        self.world.position(self.key).unwrap_or_else(nowhere)
    }

    /// Where object `id` stands and its radius (its setup's, by its height scale).
    fn object(&self, id: u64) -> Option<(Position, f32)> {
        let key = self.world.find(id)?;
        Some((self.world.position(key)?, self.world.radius(key)))
    }

    fn offset_position(
        &self,
        id: u64,
        height: f32,
        use_scale: bool,
        offset: Vec3,
    ) -> Option<Position> {
        let key = if id == 0 {
            self.key
        } else {
            self.world.find(id)?
        };
        self.world
            .object_offset_position(self.land, key, height, use_scale, offset)
    }

    /// No object holds another yet: an object's own place.
    fn top_level_position(&self, id: u64) -> Option<Position> {
        self.world.position(self.world.find(id)?)
    }

    fn increment_physics_stamp(&mut self) {
        self.world.increment_physics_stamp();
    }

    fn add_targeted(&mut self, id: u64, settings: &DetectorSettings, sightings: &mut Sightings) {
        if let Some(key) = self.world.find(id) {
            self.world
                .add_to_intersection_targeted(key, settings, sightings, false, self.game);
        }
    }

    fn cells_within(&self, at: &Position, radius: f32) -> Vec<(CellId, bool)> {
        cells::find_cells_of_spheres(self.land, at, &[Sphere::new(Vec3::ZERO, radius)])
            .into_iter()
            .map(|c| (c, cell_loaded(self.land, c)))
            .collect()
    }

    fn watch_cell(&mut self, cell: CellId, watcher: u64, detector: &Detector) {
        if cell_loaded(self.land, cell) {
            self.world.add_watcher(cell, watcher, detector.report);
        }
    }

    fn unwatch_cell(&mut self, cell: CellId, watcher: u64, _detector: &Detector) {
        if cell_loaded(self.land, cell) {
            self.world.remove_watcher(cell, watcher);
        }
    }

    fn watchers(&self) -> Vec<u64> {
        self.world
            .own_cell(self.key)
            .map(|c| self.world.watchers_of(c, self.key))
            .unwrap_or_default()
    }

    fn line_of_sight(&mut self, los: &mut SightLine) {
        self.world.los_check(self.land, los);
    }

    fn block_loaded(&self, block: u32) -> bool {
        block_loaded(self.land, block)
    }

    fn blocks_of(&self, cell: CellId, area: &BBox) -> Vec<u32> {
        cells::block_list(cell, area)
    }

    fn intersect_block(
        &mut self,
        _block: u32,
        settings: &DetectorSettings,
        area: &BBox,
        sightings: &mut Sightings,
        rays: Option<&mut BlockRays>,
    ) {
        self.world
            .intersect_block(self.land, settings, area, sightings, rays, self.game);
    }

    fn detect_block(&mut self, settings: &DetectorSettings, rays: &mut BlockRays) {
        self.world.detect_block(self.land, settings, rays);
    }

    fn split_ray(&self, ray: &Ray, from: u32) -> Vec<(u32, Ray)> {
        split_ray(ray, from)
    }

    fn now(&self) -> f64 {
        self.world.now()
    }
}
