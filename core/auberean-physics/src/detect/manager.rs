//! A body's detections: the detection spheres it keeps, each around one object it watches or, with
//! no object, around the body itself, noticing what enters the cells it reaches; and the detections
//! it makes at once, each working out where its volume stands (on the body, at a place, or offset
//! toward a target, as far as a line of sight lets it go), what the volume takes in, and what a line
//! of sight to each of those finds hidden.
//!
//! What the body would ask of the world (where objects are, the cells near it, the cells' watchers,
//! lines of sight, what each landblock holds) arrives through [`DetectionHost`].

use std::collections::BTreeMap;

use auberean_assets::Context;
use auberean_dat::reader::Result;
use auberean_dat::Reader;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use super::{BlockRays, Detected, Detector, DetectorSettings, InstantDetection, Sightings};
use crate::ethereal::SightLine;
use crate::geom::{BBox, Ray, EPS};
use crate::pack::{write_table_header, Pack, Writer};

/// How a detection of one object changed.
pub mod status {
    pub const NO_CHANGE: u32 = 0;
    pub const ENTERED: u32 = 1;
    pub const LEFT: u32 = 2;
}

/// What a detection made at once came to.
pub mod outcome {
    /// It ran (or it targets one object only, which needs no search).
    pub const SUCCEEDED: u32 = 0;
    /// The way to its origin was blocked, or its volume has no extent to search.
    pub const BLOCKED: u32 = 3;
    /// Where it stands could not be worked out: no body, no target, or the target too far.
    pub const NOT_PLACED: u32 = 4;
    /// The target lies beyond the detection's reach (kept only until the caller overwrites it).
    pub const OUT_OF_REACH: u32 = 5;
    /// The detection has no state.
    pub const FAILED: u32 = 8;
}

/// How far a body moves before the cells its global detection watches are worked out again.
pub const CELL_REFRESH_DISTANCE: f32 = 5.0;
/// How often the detection spheres are checked, seconds.
pub const CHECK_INTERVAL: f64 = 1.0;

/// The world as a body's detections see it.
pub trait DetectionHost {
    /// The detecting body's id.
    fn id(&self) -> u64;
    /// Where the detecting body is.
    fn position(&self) -> Position;
    /// Where object `id` is and its radius, when it is in the world.
    fn object(&self, id: u64) -> Option<(Position, f32)>;
    /// Where object `id` (0 for the detecting body) stands raised `height` of its height (scaled
    /// when `use_scale`) and moved `offset` in its own frame; none when that cannot be worked out.
    fn offset_position(
        &self,
        id: u64,
        height: f32,
        use_scale: bool,
        offset: Vec3,
    ) -> Option<Position>;
    /// Where the object at the top of object `id`'s containment stands.
    fn top_level_position(&self, id: u64) -> Option<Position>;
    /// Notes object `id` among the sightings of a detection that targets it.
    fn add_targeted(&mut self, id: u64, settings: &DetectorSettings, sightings: &mut Sightings);
    /// The cells within `radius` of `at`, each with whether it is loaded; none are loaded for this.
    fn cells_within(&self, at: &Position, radius: f32) -> Vec<(CellId, bool)>;
    /// Makes `watcher` watch loaded cell `cell` through `detector`, or stop watching it (a cell
    /// not loaded is passed over).
    fn watch_cell(&mut self, cell: CellId, watcher: u64, detector: &Detector);
    fn unwatch_cell(&mut self, cell: CellId, watcher: u64, detector: &Detector);
    /// The bodies watching the detecting body's cell.
    fn watchers(&self) -> Vec<u64>;
    /// Traces a line of sight.
    fn line_of_sight(&mut self, los: &mut SightLine);
    /// Whether landblock `block` is loaded.
    fn block_loaded(&self, block: u32) -> bool;
    /// The landblocks the box `area` around a volume standing in outdoor cell `cell` reaches.
    fn blocks_of(&self, cell: CellId, area: &BBox) -> Vec<u32>;
    /// What landblock `block` holds that the volume of `settings` takes in, noted among
    /// `sightings`, with a ray to each in `rays` when lines of sight are wanted.
    fn intersect_block(
        &mut self,
        block: u32,
        settings: &DetectorSettings,
        area: &BBox,
        sightings: &mut Sightings,
        rays: Option<&mut BlockRays>,
    );
    /// Casts the rays of one landblock, each object's coverage set to how much of it they found
    /// hidden.
    fn detect_block(&mut self, settings: &DetectorSettings, rays: &mut BlockRays);
    /// The pieces of `ray`, which starts in landblock `from`, in each landblock it crosses.
    fn split_ray(&self, ray: &Ray, from: u32) -> Vec<(u32, Ray)>;
    /// A detection sphere's object entered or left it (the client's own copy does nothing with it).
    fn report(&mut self, _context: u32, _detected: &Detected) {}
    /// Counts the world's stamp on, as each search does before it takes anything.
    fn increment_physics_stamp(&mut self) {}
    /// The clock, seconds.
    fn now(&self) -> f64;
}

/// One body's detections.
// D2-06852
#[derive(Debug, PartialEq)]
pub struct Detections {
    /// What the global detection knows of each object in the cells it watches, with the change not
    /// yet reported; none until a global detection starts.
    pub known: Option<BTreeMap<u64, Detected>>,
    /// Changes received and not yet reported.
    pub pending_updates: u32,
    /// The cells the global detection watches; none until it starts.
    pub cells: Option<Vec<CellId>>,
    /// When the spheres were last checked.
    pub last_check: f64,
    /// Where the body was when it last worked out the cells it watches.
    pub cells_from: Position,
    /// The detection spheres, by context.
    pub detectors: BTreeMap<u32, Detector>,
    /// The contexts of spheres to be destroyed at the next check.
    pub pending_deletions: Vec<u32>,
    /// The detections made at once, by context.
    pub instant: BTreeMap<u32, InstantDetection>,
}

impl Default for Detections {
    fn default() -> Self {
        Self::new()
    }
}

fn place(cell: u32, origin: Vec3) -> Position {
    Position::new(CellId(cell), Frame::new(origin, Quat::IDENTITY))
}

fn is_zero(v: Vec3) -> bool {
    v.x.abs() < EPS && v.y.abs() < EPS && v.z.abs() < EPS
}

/// The id of the landblock of `cell`, as landblocks are named in a detection's lists.
fn block_of(cell: u32) -> u32 {
    cell & 0xFFFF_0000 | 0xFFFF
}

impl Detections {
    #[must_use]
    pub fn new() -> Self {
        Self {
            known: None,
            pending_updates: 0,
            cells: None,
            last_check: 0.0,
            cells_from: place(0, Vec3::ZERO),
            detectors: BTreeMap::new(),
            pending_deletions: Vec::new(),
            instant: BTreeMap::new(),
        }
    }

    /// The bodies watching the body's cell.
    // D2-06794
    #[must_use]
    pub fn watchers(host: &dyn DetectionHost) -> Vec<u64> {
        host.watchers()
    }

    /// Whether a sphere's object is inside it now: within the sphere's radius and the object's
    /// together (strictly). Entering and leaving are the changes; an object gone from the world
    /// has left.
    // D2-06795
    pub fn check_object(d: &mut Detector, host: &dyn DetectionHost) -> u32 {
        let Some((at, radius)) = host.object(d.info.object) else {
            if d.object_detected {
                d.object_detected = false;
                return status::LEFT;
            }
            return status::NO_CHANGE;
        };
        let dist = crate::geom::distance(&host.position(), &at);
        let reach = f64::from(radius) + f64::from(d.radius);
        if reach <= f64::from(dist) || reach.is_nan() || dist.is_nan() {
            if d.object_detected {
                d.object_detected = false;
                return status::LEFT;
            }
        } else if !d.object_detected {
            d.object_detected = true;
            return status::ENTERED;
        }
        status::NO_CHANGE
    }

    /// The cells within a sphere's radius of the body, loaded or not.
    // D2-06799
    #[must_use]
    pub fn find_cell_list(d: &Detector, host: &dyn DetectionHost) -> Vec<(CellId, bool)> {
        host.cells_within(&host.position(), d.radius)
    }

    /// Stops the global detection: forgets what it knew and stops watching its cells.
    // D2-06809
    pub fn cleanup_global_detection(&mut self, d: &Detector, host: &mut dyn DetectionHost) {
        if self.known.take().is_some() {
            self.pending_updates = 0;
        }
        if let Some(cells) = self.cells.take() {
            let id = host.id();
            for c in cells {
                host.unwatch_cell(c, id, d);
            }
        }
    }

    /// Works out the cells the global detection watches from where the body is now: it starts
    /// watching each loaded one, then stops watching each it watched (a cell in both stays
    /// watched, its watchers being counted), and keeps the new list. False when there is no global
    /// detection.
    // D2-06810
    pub fn update_cell_list_internal(
        &mut self,
        d: &Detector,
        host: &mut dyn DetectionHost,
    ) -> bool {
        let Some(old) = self.cells.take() else {
            return false;
        };
        self.cells_from = host.position();
        let found = Self::find_cell_list(d, host);
        let id = host.id();
        for (c, loaded) in &found {
            if *loaded {
                host.watch_cell(*c, id, d);
            }
        }
        for c in old {
            host.unwatch_cell(c, id, d);
        }
        let mut cells: Vec<CellId> = Vec::with_capacity(found.len());
        for (c, _) in found {
            if !cells.contains(&c) {
                cells.push(c);
            }
        }
        self.cells = Some(cells);
        true
    }

    /// How many spheres watch object `object`, those to be destroyed left out.
    // D2-06811
    #[must_use]
    pub fn num_targeted_on(&self, object: u64) -> usize {
        self.live().filter(|d| d.info.object == object).count()
    }

    /// How many spheres watch an object, those to be destroyed left out.
    // D2-06812
    #[must_use]
    pub fn num_targeted(&self) -> usize {
        self.live().filter(|d| d.info.object != 0).count()
    }

    /// How many spheres watch the cells around the body, those to be destroyed left out.
    // D2-06813
    #[must_use]
    pub fn num_global(&self) -> usize {
        self.live().filter(|d| d.info.object == 0).count()
    }

    fn live(&self) -> impl Iterator<Item = &Detector> + '_ {
        self.detectors
            .values()
            .filter(|d| !self.pending_deletions.contains(&d.context))
    }

    /// The body leaves the world: every global detection stops.
    // D2-06814
    pub fn handle_leave_world(&mut self, host: &mut dyn DetectionHost) {
        let globals: Vec<Detector> = self
            .detectors
            .values()
            .filter(|d| d.info.object == 0)
            .copied()
            .collect();
        for d in globals {
            self.cleanup_global_detection(&d, host);
        }
    }

    /// Whether the body has a sphere watching an object, or changes to report.
    // D2-06822
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.num_targeted() != 0 || self.pending_updates != 0
    }

    /// Whether a sphere of context `context` is kept and not to be destroyed.
    // D2-06823
    #[must_use]
    pub fn has_detector(&self, context: u32) -> bool {
        !self.pending_deletions.contains(&context) && self.detectors.contains_key(&context)
    }

    /// Marks the sphere of context `context` to be destroyed at the next check; false when there is
    /// none (one already marked counts).
    // D2-06824
    pub fn destroy_detector(&mut self, context: u32) -> bool {
        if self.pending_deletions.contains(&context) {
            return true;
        }
        if self.detectors.contains_key(&context) {
            self.pending_deletions.push(context);
            return true;
        }
        false
    }

    /// Marks every sphere that watches an object to be destroyed.
    // D2-06825
    pub fn delete_all_targeted(&mut self) -> bool {
        let targeted: Vec<u32> = self
            .detectors
            .values()
            .filter(|d| d.info.object != 0)
            .map(|d| d.context)
            .collect();
        for c in targeted {
            self.destroy_detector(c);
        }
        true
    }

    /// Marks every sphere that watches object `object` to be destroyed.
    // D2-06841
    pub fn remove_targeted(&mut self, object: u64) -> bool {
        let targeted: Vec<u32> = self
            .detectors
            .values()
            .filter(|d| d.info.object == object)
            .map(|d| d.context)
            .collect();
        for c in targeted {
            self.destroy_detector(c);
        }
        true
    }

    /// Destroys the sphere of context `context`; a global one stops the global detection.
    // D2-06826
    pub fn cleanup_detector(&mut self, context: u32, host: &mut dyn DetectionHost) {
        if let Some(d) = self.detectors.remove(&context) {
            if d.info.object == 0 {
                self.cleanup_global_detection(&d, host);
            }
        }
    }

    /// Destroys the spheres marked: the first marked each time, the last taking its place.
    // D2-06827
    pub fn clear_pending_deletions(&mut self, host: &mut dyn DetectionHost) {
        while !self.pending_deletions.is_empty() {
            let context = self.pending_deletions.swap_remove(0);
            self.cleanup_detector(context, host);
        }
    }

    /// Reports what the global detection noticed: each object that entered or left is reported;
    /// one that left is forgotten, the others start over unchanged.
    // D2-06828
    pub fn report_global_detection(&mut self, d: &Detector, host: &mut dyn DetectionHost) {
        let Some(known) = self.known.as_mut() else {
            return;
        };
        let mut gone = Vec::new();
        for (id, k) in known.iter_mut() {
            if k.status == status::ENTERED || k.status == status::LEFT {
                host.report(d.context, k);
            }
            if k.status == status::LEFT {
                gone.push(*id);
            } else {
                k.status = status::NO_CHANGE;
            }
        }
        for id in gone {
            known.remove(&id);
        }
    }

    /// Forgets the detection made at once under context `context`.
    // D2-06829
    pub fn remove_context(&mut self, context: u32) {
        self.instant.remove(&context);
    }

    /// The detection made at once under context `context`, made empty when there is none.
    // D2-06843
    pub fn state(&mut self, context: u32) -> &mut InstantDetection {
        self.instant.entry(context).or_default()
    }

    /// Starts a detection made at once under `context` with `settings`; false when one under that
    /// context is already running.
    // D2-06844
    pub fn init_intersection(&mut self, context: u32, settings: &DetectorSettings) -> bool {
        let s = self.state(context);
        if s.context != 0 {
            return false;
        }
        s.init(context, settings);
        true
    }

    /// The sightings of the detection made at once under `context`.
    // D2-06845
    pub fn profile_list(&mut self, context: u32) -> &mut Sightings {
        &mut self.state(context).sightings
    }

    /// Takes a change the cells report for the global detection: an object entering that was not
    /// known becomes known, entered; one known entering cancels a leaving not yet reported (else
    /// it is entered again); one leaving that had entered unreported is forgotten (else it is
    /// left). Each counts as a change to report but a cancelled or forgotten one.
    // D2-06842
    pub fn receive_update(&mut self, info: &Detected) {
        let Some(known) = self.known.as_mut() else {
            return;
        };
        match known.get_mut(&info.object) {
            None => {
                if info.status == status::ENTERED {
                    known.insert(info.object, *info);
                    self.pending_updates += 1;
                }
            }
            Some(k) if info.status == status::ENTERED => {
                if k.status == status::LEFT {
                    k.status = status::NO_CHANGE;
                    if self.pending_updates != 0 {
                        self.pending_updates -= 1;
                    }
                } else {
                    k.status = status::ENTERED;
                    self.pending_updates += 1;
                }
            }
            Some(k) if info.status == status::LEFT => {
                if k.status == status::ENTERED {
                    known.remove(&info.object);
                    if self.pending_updates != 0 {
                        self.pending_updates -= 1;
                    }
                } else {
                    k.status = status::LEFT;
                    self.pending_updates += 1;
                }
            }
            Some(_) => {}
        }
    }

    /// Starts the global detection of sphere `d` (one watching no object), when none runs yet:
    /// what it knows starts empty, and it watches the cells around the body.
    // D2-06853
    pub fn initialize_global_detection(
        &mut self,
        d: &Detector,
        host: &mut dyn DetectionHost,
    ) -> bool {
        if d.info.object != 0 || self.cells.is_some() || self.known.is_some() {
            return false;
        }
        self.known = Some(BTreeMap::new());
        self.cells = Some(Vec::new());
        self.update_cell_list_internal(d, host)
    }

    /// Keeps the cells the global detection watches up to date: started when it is not running,
    /// worked out again once the body has moved 5 m.
    // D2-06854
    pub fn update_cell_list(&mut self, d: &Detector, host: &mut dyn DetectionHost) {
        if self.cells.is_none() {
            self.initialize_global_detection(d, host);
            return;
        }
        if CELL_REFRESH_DISTANCE <= crate::geom::distance(&self.cells_from, &host.position()) {
            self.update_cell_list_internal(d, host);
        }
    }

    /// A new detection sphere under `context`, watching `object` (0: the cells around the body)
    /// within `radius`, reporting `report`. A sphere marked for destruction under the same context
    /// is destroyed first; false when one is kept under it, or when a second global one is asked
    /// for.
    // D2-06855
    pub fn create_detector(
        &mut self,
        context: u32,
        object: u64,
        radius: f32,
        report: super::ReportType,
        host: &mut dyn DetectionHost,
    ) -> bool {
        if let Some(i) = self.pending_deletions.iter().position(|c| *c == context) {
            self.pending_deletions.swap_remove(i);
            self.cleanup_detector(context, host);
        }
        if self.detectors.contains_key(&context) {
            return false;
        }
        let d = Detector::new(context, object, radius, report);
        if object == 0 && !self.initialize_global_detection(&d, host) {
            return false;
        }
        self.detectors.insert(context, d);
        true
    }

    /// Checks one sphere: a global one keeps its cells up to date and reports what they noticed;
    /// one watching an object reports it entering or leaving.
    // D2-06856
    pub fn check_detector(&mut self, context: u32, host: &mut dyn DetectionHost) {
        let Some(mut d) = self.detectors.get(&context).copied() else {
            return;
        };
        if d.info.object == 0 {
            self.update_cell_list(&d, host);
            self.report_global_detection(&d, host);
            self.pending_updates = 0;
        } else {
            d.info.status = Self::check_object(&mut d, host);
            if d.info.status == status::ENTERED || d.info.status == status::LEFT {
                host.report(d.context, &d.info);
            }
            self.detectors.insert(context, d);
        }
    }

    /// The spheres marked are destroyed; then, a second after the last check or when `force`,
    /// every sphere is checked.
    // D2-06857
    pub fn check_detection(&mut self, force: bool, host: &mut dyn DetectionHost) {
        self.clear_pending_deletions(host);
        let now = host.now();
        if CHECK_INTERVAL <= now - self.last_check || force {
            let contexts: Vec<u32> = self.detectors.keys().copied().collect();
            for c in contexts {
                self.check_detector(c, host);
            }
            self.last_check = now;
        }
    }

    /// Where a detection's volume reaches from: on the body (its base worked out once, then kept
    /// as a place), or at its place; then the offset to its target's place, or its own offset.
    /// False, with the outcome, when the body's place, the target or the target's reach fails.
    // D2-06830
    fn calc_positions(
        state: &mut InstantDetection,
        offset: &mut Vec3,
        out: &mut u32,
        host: &mut dyn DetectionHost,
    ) -> bool {
        let s = &mut state.settings;
        if s.based_on_object {
            let Some(base) = host.offset_position(
                0,
                s.height_adjustment,
                s.use_object_scale,
                s.base_offset_from_object,
            ) else {
                *out = 1;
                return false;
            };
            s.set_base_position(&base);
        }
        if s.center_on_target == 0 {
            *offset = s.offset_to_detection;
            return true;
        }
        if host.object(s.center_on_target).is_none() {
            *out = outcome::NOT_PLACED;
            return false;
        }
        Self::target_base_position(state, offset, out, host)
    }

    /// The offset from a detection's base to where its target stands (raised and moved as the
    /// settings say, else where the top of its containment stands); a target the detection
    /// targets is noted among its sightings. A target farther than the detection reaches clears
    /// the sightings and fails.
    // D2-06815
    fn target_base_position(
        state: &mut InstantDetection,
        offset: &mut Vec3,
        out: &mut u32,
        host: &mut dyn DetectionHost,
    ) -> bool {
        let s = &state.settings;
        let target = s.center_on_target;
        let at = host
            .offset_position(
                target,
                s.target_height_adjustment,
                s.target_use_scale,
                s.target_offset,
            )
            .or_else(|| host.top_level_position(target))
            .unwrap_or_else(|| place(0, Vec3::ZERO));
        if s.in_target_list(target) {
            host.add_targeted(target, &state.settings, &mut state.sightings);
        }
        let s = &state.settings;
        *offset = crate::geom::offset(&s.base_position, &at);
        let max = f64::from(s.max_target_distance);
        let (x, y, z) = (
            f64::from(offset.x),
            f64::from(offset.y),
            f64::from(offset.z),
        );
        if max * max < x * x + y * y + z * z {
            state.sightings.clear();
            *out = outcome::OUT_OF_REACH;
            return false;
        }
        true
    }

    /// Settles a detection's origin: its base when the offset is nothing; else as far along the
    /// offset as a line of sight gets. A line that hit something clears the sightings, one that
    /// could not be traced keeps them; either blocks the detection unless it is to be made where
    /// the line stopped.
    // D2-06816
    fn check_origin(
        state: &mut InstantDetection,
        offset: Vec3,
        out: &mut u32,
        host: &mut dyn DetectionHost,
    ) -> bool {
        if is_zero(offset) {
            let base = state.settings.base_position;
            state.settings.set_origin_point(&base);
            return true;
        }
        let mut untraced = false;
        let mut los = SightLine::default();
        if !Self::check_los_to_origin(
            offset,
            &mut state.settings,
            out,
            &mut untraced,
            &mut los,
            host,
        ) {
            if !untraced {
                state.sightings.clear();
            }
            if state.settings.detect_at_blocked_offset {
                let at = place(los.final_cell, los.end_point());
                state.settings.set_origin_point(&at);
            } else {
                *out = outcome::BLOCKED;
            }
        }
        true
    }

    /// Traces a line of sight from a detection's base along `offset`, stopped by what the
    /// settings say blocks the offset: the origin is where it ends. False when it could not be
    /// traced (`untraced` set), or hit something and the detection is not made where it stops.
    // D2-06800
    fn check_los_to_origin(
        offset: Vec3,
        s: &mut DetectorSettings,
        out: &mut u32,
        untraced: &mut bool,
        los: &mut SightLine,
        host: &mut dyn DetectionHost,
    ) -> bool {
        if is_zero(offset) {
            let base = s.base_position;
            s.set_origin_point(&base);
            return true;
        }
        let mut ray = Ray::default();
        ray.init_from_offset(s.base_position.frame.origin, offset);
        los.set_ray(ray, s.base_position.cell.0);
        los.set_intersect_type(0);
        los.set_los_params(&s.offset_object_blocking, s.offset_physics_blocking);
        host.line_of_sight(los);
        if los.trace_result == 2 {
            *untraced = true;
            return false;
        }
        if los.trace_result != 0 && !s.detect_at_blocked_offset {
            *out = outcome::BLOCKED;
            return false;
        }
        let at = place(los.final_cell, los.end_point());
        s.set_origin_point(&at);
        true
    }

    /// Searches what a detection's volume takes in: every landblock the box around it reaches
    /// (its own only, inside), each loaded one asked what it holds there, then lines of sight when
    /// anything blocks them. A detection targeting one object needs no search; a volume with no
    /// box blocks. False while blocks it needs are held elsewhere.
    // D2-06831
    fn do_detect(
        state: &mut InstantDetection,
        out: &mut u32,
        host: &mut dyn DetectionHost,
    ) -> bool {
        if state.settings.single_targeted_object() {
            *out = outcome::SUCCEEDED;
            return true;
        }
        let Some(area) = state.settings.global_box() else {
            *out = outcome::BLOCKED;
            return true;
        };
        let origin = *state.settings.origin_point();
        if origin.cell.0 == 0 {
            return true;
        }
        let blocks = if crate::interior::is_interior(origin.cell) {
            vec![block_of(origin.cell.0)]
        } else {
            host.blocks_of(origin.cell, &area)
        };
        let los_needed = state.settings.physics_blocking.mask != u64::MAX
            || state.settings.object_blocking.need_los_check();
        for b in blocks {
            let loaded = host.block_loaded(b);
            if los_needed {
                state.create_block_list_info(b);
            }
            if loaded {
                let InstantDetection {
                    settings,
                    sightings,
                    blocks: infos,
                    ..
                } = state;
                let info = infos
                    .iter_mut()
                    .find(|i| i.block == b)
                    .filter(|_| los_needed);
                host.intersect_block(b, settings, &area, sightings, info);
            }
            if let Some(i) = state.blocks.iter_mut().find(|i| i.block == b) {
                i.loaded = loaded;
            }
        }
        if state.remote_block_count != 0 || (los_needed && !Self::do_los_check(state, host)) {
            return false;
        }
        true
    }

    /// Casts the lines of sight of a detection: each ray toward each object of each landblock
    /// looked in, cut into a piece for every landblock it crosses from the origin's; then each
    /// landblock's pieces drop the objects not sighted, an unloaded landblock's objects lose their
    /// sightings, and a loaded one's rays are cast and their coverage taken. False while blocks
    /// are held elsewhere.
    // D2-06817
    fn do_los_check(state: &mut InstantDetection, host: &mut dyn DetectionHost) -> bool {
        let from = block_of(state.settings.origin_point().cell.0);
        let mut pieces: Vec<(Ray, u64, u32)> = Vec::new();
        for b in &state.blocks {
            for (object, rays) in &b.objects {
                for ray in &rays.rays {
                    if from == b.block {
                        pieces.push((*ray, *object, b.block));
                    } else {
                        for (block, piece) in host.split_ray(ray, from) {
                            pieces.push((piece, *object, block));
                        }
                    }
                }
            }
        }
        for (ray, object, block) in pieces {
            state.add_block_ray(ray, object, block);
        }
        let mut cast = std::mem::take(&mut state.block_rays);
        for b in &mut cast {
            state.prune_objects(b);
            if b.objects.is_empty() {
                continue;
            }
            if !b.loaded || !host.block_loaded(b.block) {
                state.remove_objects_in_unloaded_block(b);
            } else {
                host.detect_block(&state.settings, b);
                state.update_profiles(b);
            }
        }
        state.block_rays = cast;
        state.remote_block_count == 0
    }

    /// Makes the detection under `context` at once: where its volume stands, then what it takes
    /// in. The outcome says how it went; false while it waits for blocks held elsewhere.
    // D2-06846
    pub fn do_intersection(
        &mut self,
        context: u32,
        out: &mut u32,
        host: &mut dyn DetectionHost,
    ) -> bool {
        *out = outcome::SUCCEEDED;
        let state = self.state(context);
        let mut offset = Vec3::ZERO;
        if !Self::calc_positions(state, &mut offset, out, host) {
            // Whatever failed, the caller is told the detection could not be placed.
            *out = outcome::NOT_PLACED;
            return true;
        }
        if !Self::check_origin(state, offset, out, host) {
            return false;
        }
        if *out != outcome::SUCCEEDED {
            return true;
        }
        Self::do_detect(state, out, host)
    }

    /// Ends the body's detections: the spheres marked are destroyed, the others kept as they are.
    // D2-06840
    pub fn destroy(&mut self, host: &mut dyn DetectionHost) {
        self.clear_pending_deletions(host);
    }
}

impl Pack for Detections {
    /// The spheres by context, the time of the last check, and the detections made at once by
    /// context.
    // D2-06798, D2-06849, D2-06850
    fn pack(&self, w: &mut Writer) {
        write_table_header(w, self.detectors.len());
        for (k, d) in &self.detectors {
            w.u32(*k);
            d.pack(w);
        }
        w.u64(self.last_check.to_bits());
        write_table_header(w, self.instant.len());
        for (k, s) in &self.instant {
            w.u32(*k);
            s.pack(w);
        }
    }

    fn unpack(r: &mut Reader<'_>, ctx: &Context) -> Result<Self> {
        let detectors = r
            .hash_table(8, |r| Ok((r.u32()?, Detector::unpack(r, ctx)?)))?
            .into_iter()
            .collect();
        let last_check = f64::from_bits(r.u64()?);
        let instant = r
            .hash_table(8, |r| Ok((r.u32()?, InstantDetection::unpack(r, ctx)?)))?
            .into_iter()
            .collect();
        Ok(Self {
            detectors,
            last_check,
            instant,
            ..Self::new()
        })
    }
}
