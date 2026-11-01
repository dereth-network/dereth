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
//! through the walls, floors and ceilings of the cell and the cells it sees; outside, landblock by
//! landblock across the ground, cut where it leaves each one, stopping at the edge of a landblock
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
use crate::ethereal::{PhysicsTypes, SightLine};
use crate::geom::{self, cover, BBox, Ray};
use crate::interior::is_interior;
use crate::land::{cell_of, land_cell_id, Land};
use crate::solid;

/// The game objects for a host that has none: no object is of a kind a detection reports, and
/// nothing is noted of any.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoGameObjects;

impl GameObjects for NoGameObjects {
    fn matches(&self, _object: u64, _report: &ReportType) -> bool {
        false
    }
    fn note(&self, _object: u64, _sighting: &mut Sighting) {}
}

/// What a line of sight traces: the physics shapes of the surroundings and of what stands in them.
const PHYSICS_SHAPES: u32 = 0;
/// What a line of sight traces: doorways only.
const PORTALS_ONLY: [u32; 2] = [3, 4];
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
fn cell_loaded(land: &Land, cell: CellId) -> bool {
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

/// The land cell of landblock `block` a point of its frame falls in, the point first brought
/// inside the landblock.
fn land_cell_at(block: u32, p: Vec3) -> u32 {
    let mut v = p;
    cells::move_into_block(&mut v);
    let lb = CellId(block).landblock();
    cell_of(v).map_or(0, |(cx, cy)| land_cell_id(lb.x(), lb.y(), cx, cy).0)
}

/// How far along the line from `from` to `to` point `p` lies, as a fraction.
fn fraction(from: Vec3, to: Vec3, p: Vec3) -> f32 {
    let d = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
    let len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
    if len > 0.0 {
        let q = Vec3::new(p.x - from.x, p.y - from.y, p.z - from.z);
        (q.x * q.x + q.y * q.y + q.z * q.z).sqrt() / len
    } else {
        0.0
    }
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
                if g.portals.is_empty() {
                    return false;
                }
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
    // D2-05970, D2-05971
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

    /// Traces line of sight `los` from its first cell through the surroundings and what stands in
    /// them. False, the line untouched, when it has no first cell or that cell is not loaded.
    // D2-03851
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
            self.trace_inside(land, los);
        } else {
            self.trace_outside(land, los);
        }
        true
    }

    /// How far along the line from `from` to `to` (in the landblock frame of `cell`) the objects
    /// of the table that block `los` first meet it, as a fraction. The solid parts placed in the
    /// interior cells are left to the cells' own traces.
    fn objects_hit(&self, cell: CellId, from: Vec3, to: Vec3, los: &SightLine) -> Option<f32> {
        let mut best: Option<f32> = None;
        for (key, e) in &self.entries {
            if e.id == 0 && e.shape.has_physics_tree() {
                continue;
            }
            if !self.is_los_blocking(*key, los) {
                continue;
            }
            let Some(o) = self.objects.get(e.handle) else {
                continue;
            };
            let shift = geom::block_offset(cell, o.position.cell);
            let a = Vec3::new(from.x - shift.x, from.y - shift.y, from.z - shift.z);
            let b = Vec3::new(to.x - shift.x, to.y - shift.y, to.z - shift.z);
            if let Some(t) = e.shape.check_ray(&o.position.frame, a, b) {
                if best.is_none_or(|x| t < x) {
                    best = Some(t);
                }
            }
        }
        best
    }

    /// An interior cell's own trace: through the walls, floors and ceilings of the cell and the
    /// cells it sees and, tracing physics shapes, what stands in them. Stopped, the line ends where
    /// it was stopped; else it runs its whole length; either way it ends in the cell.
    fn trace_inside(&self, land: &Land, los: &mut SightLine) {
        let cell = CellId(los.initial_cell);
        let r = los.current_ray;
        let from = r.point;
        let to = end_of(&r);
        let mut best: Option<f32> = None;
        let mut take = |t: Option<f32>| {
            if let Some(t) = t {
                if best.is_none_or(|b| t < b) {
                    best = Some(t);
                }
            }
        };
        if !PORTALS_ONLY.contains(&los.intersect) {
            take(
                crate::interior::trace(land, cell, from, to, false)
                    .map(|(p, _)| fraction(from, to, p)),
            );
            if los.intersect == PHYSICS_SHAPES {
                if solid::is_blocking(PhysicsTypes::default(), 0, &los.objects, &los.physics) {
                    take(
                        crate::interior::trace(land, cell, from, to, true)
                            .map(|(p, _)| fraction(from, to, p)),
                    );
                }
                take(self.objects_hit(cell, from, to, los));
            }
        }
        los.final_cell = cell.0;
        match best.filter(|t| *t < los.delta) {
            Some(t) => {
                los.delta = t;
                los.calc_final_ray();
                los.trace_result = 1;
            }
            None => {
                los.final_ray = r;
                los.impact_normal = Vec3::ZERO;
                los.trace_result = 0;
            }
        }
    }

    /// How far along the line from `from` to `to` (in landblock `block`'s frame) the ground first
    /// meets it and, unless the line traces the surroundings only, the solid parts placed outside
    /// (as the kind of object the client makes of them) and the objects of the table.
    fn first_hit_outside(
        &self,
        land: &Land,
        block: u32,
        from: Vec3,
        to: Vec3,
        los: &SightLine,
    ) -> Option<f32> {
        let lb = CellId(block).landblock();
        let mut best: Option<f32> = None;
        let mut take = |t: Option<f32>| {
            if let Some(t) = t {
                if best.is_none_or(|b| t < b) {
                    best = Some(t);
                }
            }
        };
        take(
            land.trace(lb, from, to, false)
                .map(|(p, _)| fraction(from, to, p)),
        );
        if los.intersect != ENVIRONMENT_ONLY {
            if solid::is_blocking(self.bit(STATIC_ETHEREAL), 0, &los.objects, &los.physics) {
                take(
                    land.trace(lb, from, to, true)
                        .map(|(p, _)| fraction(from, to, p)),
                );
            }
            take(self.objects_hit(CellId(block), from, to, los));
        }
        best
    }

    /// The line's current ray traced through landblock `block`: stopped, it ends in the land cell
    /// where it was stopped; else it ends in the land cell its end falls in. The trace's result.
    fn trace_block(&self, land: &Land, block: u32, los: &mut SightLine) -> u32 {
        let r = los.current_ray;
        let (from, to) = (r.point, end_of(&r));
        los.final_cell = 0;
        if let Some(t) = self
            .first_hit_outside(land, block, from, to, los)
            .filter(|t| *t < los.delta)
        {
            los.delta = t;
            let hit = Vec3::new(
                from.x + (to.x - from.x) * t,
                from.y + (to.y - from.y) * t,
                from.z + (to.z - from.z) * t,
            );
            los.final_cell = land_cell_at(block, hit);
            los.trace_result = 1;
            los.calc_final_ray();
            return 1;
        }
        let (mut c, mut end) = (CellId(block), to);
        los.final_cell = if adjust_to_outside(&mut c, &mut end) {
            c.0
        } else {
            0
        };
        los.trace_result = 0;
        los.calc_final_ray();
        0
    }

    /// A land cell's own trace: its start first brought into the land cell it falls in; a line
    /// straight up or down stays in that cell; any other is traced landblock by landblock, cut
    /// where it leaves each, until it is stopped or ends, or reaches the edge of a landblock that
    /// is not loaded (result 2: it ends at that edge). Traced once until the line starts over.
    fn trace_outside(&self, land: &Land, los: &mut SightLine) {
        if los.checked_outside {
            return;
        }
        los.checked_outside = true;
        let own = los.initial_cell;
        let (mut cell, mut start) = (CellId(own), los.initial_ray.point);
        let _ = adjust_to_outside(&mut cell, &mut start);
        if cell.0 != own {
            let r = los.initial_ray;
            let reach = Vec3::new(r.length * r.dir.x, r.length * r.dir.y, r.length * r.dir.z);
            los.set_ray(Ray::from_offset(start, reach), cell.0);
            los.current_ray = los.initial_ray;
        }
        let cur = los.current_ray;
        if cur.dir.x == 0.0 && cur.dir.y == 0.0 {
            let id = CellId(los.initial_cell);
            if cell_loaded(land, id) {
                los.trace_result = self.trace_block(land, id.0 | BLOCK_CELL, los);
                los.final_cell = id.0;
                los.calc_final_ray();
            } else {
                los.final_cell = id.0;
            }
            return;
        }
        let mut block = cell.0 & 0xFFFF_0000 | BLOCK_CELL;
        if !block_loaded(land, block) {
            los.final_cell = los.initial_cell;
            return;
        }
        for _ in 0..MOST_BLOCKS {
            let mut rest = los.current_ray;
            let mut piece = Ray::default();
            let mut dir = 0;
            if !cells::block_ray_clip(&mut rest, &mut piece, &mut dir) {
                los.current_ray = rest;
                los.delta = 1.0;
                los.trace_result = self.trace_block(land, block, los);
                return;
            }
            let next = cells::block_beside(block, dir);
            los.current_ray = piece;
            los.delta = 1.0;
            let r = self.trace_block(land, block, los);
            if r != 0 {
                los.trace_result = r;
                return;
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
                return;
            }
            los.current_ray = rest;
            los.delta = 1.0;
            block = next;
        }
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
