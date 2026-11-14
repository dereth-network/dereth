//! Levels of detail chosen by how far a level strays from the part's full mesh on the screen, and
//! faded from one level into the next.
//!
//! Retail hands a degrading part from one level to the next at distances its record gives,
//! made for screens a few hundred pixels tall: on a large screen its trees and scenery drop to
//! their coarse levels while the difference is still plain to see. Here a level takes over only
//! once it strays from the full mesh by less than [`LodSettings::error_px`] pixels on the screen,
//! and never before retail's own hand-over ([`retail_switches`]) at the farthest retail's frame
//! rate ever slides it: whichever is farther. From that
//! distance the part fades into the next level over a band of the distance ([`FADE_BAND`]), the
//! two levels drawn through complementary halves of a fixed screen pattern, so a still view stays
//! still and a moving one changes a few pixels at a time ([`pick`]).
//!
//! A level's error ([`level_error`]) is measured once per mesh: the farthest any corner of the
//! full mesh lies from the level's surface. A level that turns toward the viewer is a card
//! standing for the whole part, and errs by the part's radius; a level that draws nothing errs
//! by the part's whole size.
//!
//! A tree (a part whose full mesh is mostly foliage, and large) takes one level more: its
//! impostor ([`super::impostor`]), pictures of its full mesh from round it on an upright card.
//! The impostor takes the place of the level that draws nothing, or comes after the last, and
//! errs by how far the full mesh turns between two of its pictures ([`IMPOSTOR_ERROR`]); past it
//! the tree is drawn as far as its block is. Past the blocks whose own objects stand in the world
//! the far forest draws the trees as pictures, so a tree has wholly become its impostor before
//! it could stand there ([`impostor_cap`]): a step that moves those blocks changes nothing seen.
//! A level that would leave no room to fade out before then is passed over ([`pull_in`]), so
//! near that edge a tree fades once, from the last level it keeps into its impostor.
//!
//! An art pack's model hands its levels over at the distances its pack gives them
//! ([`pack_switches`]), never nearer than they stray by the bar; a tree of a pack takes its
//! impostor after its last level, as retail's trees do, however many levels it has.

use std::collections::{BTreeMap, HashMap};

use glam::{Mat4, Vec3};

use super::scene::{class, BlockGpu, GpuScene, PartItems};
use crate::assets::{AssetTables, Mesh, MeshKey};
use crate::scene::{BlockId, BlockLevels, Degrade, DegradeBias, FrameDelta, SceneDelta};

/// How levels are chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodSettings {
    /// Levels chosen here; off, every part retail stands draws the level retail chose. Past the
    /// blocks retail stands objects on, where it would draw them as nothing, levels are chosen
    /// here whatever this says, as [`Self::ON`] chooses them.
    pub on: bool,
    /// The most a level may stray from the full mesh on the screen before a finer one is drawn,
    /// pixels.
    pub error_px: f32,
    /// How much farther than its hand-over a part is still fading into its next level: a share
    /// of the hand-over distance; 0 hands over at once.
    pub fade: f32,
    /// Far trees drawn as impostors past their last level, out to the draw distance.
    pub impostors: bool,
}

impl LodSettings {
    /// Retail's levels.
    pub const OFF: Self = Self {
        on: false,
        error_px: 10.0,
        fade: FADE_BAND,
        impostors: false,
    };

    /// Levels chosen by their error on the screen, at ten pixels.
    pub const ON: Self = Self {
        on: true,
        error_px: 10.0,
        fade: FADE_BAND,
        impostors: true,
    };
}

impl Default for LodSettings {
    fn default() -> Self {
        Self::OFF
    }
}

/// How much farther than its hand-over a part is still fading into its next level, by default: a
/// share of the hand-over distance.
pub const FADE_BAND: f32 = 0.15;

/// How many rings of blocks round the viewer's retail stands objects on.
const RETAIL_RINGS: i32 = 1;

/// Whether block `block` lies past the rings of blocks retail stands objects on round the
/// viewer's, the frame's `origin`: its objects stand only as the scenery distance reaches past
/// retail's, and retail draws them as nothing there.
fn past_retail(block: BlockId, origin: [i32; 2]) -> bool {
    let (x, y) = (i32::from(block >> 8), i32::from(block & 0xFF));
    (x - origin[0]).abs().max((y - origin[1]).abs()) > RETAIL_RINGS
}

/// The distances, from the record's [`Degrade::centre`], at which retail hands each level of
/// `d` over to the next under `bias`: level `i` is drawn nearer than the `i`th, the last level
/// beyond them all. Retail takes its degrade distance off the distance and measures the rest
/// over the part's z scale, against a threshold the bias slides from the level's ideal toward its
/// nearest or farthest.
#[must_use]
pub fn retail_switches(d: &Degrade, bias: &DegradeBias) -> Vec<f32> {
    let mut out = Vec::new();
    retail_switches_into(d, bias, &mut out);
    out
}

/// [`retail_switches`], appended to `out`.
pub(crate) fn retail_switches_into(d: &Degrade, bias: &DegradeBias, out: &mut Vec<f32>) {
    let n = d.bands.len();
    if n < 2 {
        return;
    }
    if bias.disabled {
        out.extend(std::iter::repeat_n(f32::INFINITY, n - 1));
        return;
    }
    if bias.force >= 0 {
        // LINT-OK: a level index from a small console value.
        #[allow(clippy::cast_sign_loss)]
        let forced = (bias.force as usize).min(n - 1);
        out.extend((0..n - 1).map(|i| if i < forced { 0.0 } else { f32::INFINITY }));
        return;
    }
    let z = if d.scale_z == 0.0 {
        1.0
    } else {
        d.scale_z.abs()
    };
    out.extend(d.bands[..n - 1].iter().map(|[min, ideal, max]| {
        let threshold = if bias.bias < 0.0 {
            ideal + (ideal - min) * bias.bias
        } else {
            ideal - (ideal - max) * bias.bias
        };
        (threshold + bias.distance) * z
    }));
}

/// The distances at which a pack model's levels hand over, from the distances its pack gives
/// them (`distances`, each level's but the last), under `bias`: as the pack gives them, which is
/// where retail's own levels stand at the farthest the bias slides them; never, while degrades
/// are off; and a forced level from the first, as retail forces its own.
#[must_use]
pub fn pack_switches(distances: &[f32], bias: &DegradeBias) -> Vec<f32> {
    let mut out = Vec::new();
    pack_switches_into(distances, bias, &mut out);
    out
}

/// [`pack_switches`], appended to `out`.
pub(crate) fn pack_switches_into(distances: &[f32], bias: &DegradeBias, out: &mut Vec<f32>) {
    if bias.disabled {
        out.extend(std::iter::repeat_n(f32::INFINITY, distances.len()));
        return;
    }
    if bias.force >= 0 {
        // LINT-OK: a level index from a small console value.
        #[allow(clippy::cast_sign_loss)]
        let forced = bias.force as usize;
        out.extend((0..distances.len()).map(|i| if i < forced { 0.0 } else { f32::INFINITY }));
        return;
    }
    out.extend_from_slice(distances);
}

/// The farthest a tree hands over to its impostor, fade and all, where the blocks whose own
/// objects stand in the world reach `reach` meters from the eye at the least (0 for unknown):
/// infinite where nothing marks the reach, or while degrades are off or a level is forced.
#[must_use]
pub fn impostor_cap(reach: f32, band: f32, bias: &DegradeBias) -> f32 {
    if reach <= 0.0 || bias.disabled || bias.force >= 0 {
        return f32::INFINITY;
    }
    reach / (1.0 + band.max(0.0))
}

/// `hand_overs` drawn in so the last comes no farther than `cap`. A hand-over that comes too late
/// for its fade to end before the last begins is moved to the last: the part passes over the
/// levels between, fading from the one before them straight into the last. Those that come early
/// enough are kept where they are. So a tree keeps each level out to where it hands it over, or
/// keeps it out to the cap where the window leaves no room for it, and no two fades ever overlap
/// near the cap, however many levels the tree has.
pub fn pull_in(hand_overs: &mut [f32], cap: f32, band: f32) {
    if !cap.is_finite() {
        return;
    }
    let Some((last, before)) = hand_overs.split_last_mut() else {
        return;
    };
    *last = last.min(cap);
    let room = *last / (1.0 + band.max(0.0));
    // The hand-overs rise, each at least a fade band past the one before, so those from the
    // first that fits on all fit.
    for h in before.iter_mut().rev() {
        if *h <= room {
            break;
        }
        *h = *last;
    }
}

/// How far `level` strays from `full`, in the meshes' own units: the farthest any corner of the
/// full mesh lies from the level's surface. A level `turned` toward the viewer errs by the full
/// mesh's radius, and one that draws nothing (`None`) by its whole size.
#[must_use]
pub fn level_error(full: &Mesh, level: Option<&Mesh>, turned: bool) -> f32 {
    let radius = radius(full);
    let Some(level) = level else {
        return 2.0 * radius;
    };
    if turned {
        return radius;
    }
    let corners: Vec<glam::Vec3> = level
        .vertices
        .iter()
        .map(|v| glam::Vec3::from(v.position))
        .collect();
    let triangles: Vec<[glam::Vec3; 3]> = level
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|t| {
            Some([
                *corners.get(t[0] as usize)?,
                *corners.get(t[1] as usize)?,
                *corners.get(t[2] as usize)?,
            ])
        })
        .collect();
    if triangles.is_empty() {
        return 2.0 * radius;
    }
    let mut worst = 0.0f32;
    for v in &full.vertices {
        let p = glam::Vec3::from(v.position);
        let nearest = triangles
            .iter()
            .map(|t| distance_to_triangle(p, t))
            .fold(f32::INFINITY, f32::min);
        worst = worst.max(nearest);
    }
    worst
}

/// The radius of the sphere round `mesh`: its own when it has one, else round its corners.
fn radius(mesh: &Mesh) -> f32 {
    if let Some((_, r)) = mesh.sphere {
        return r;
    }
    let n = mesh.vertices.len().max(1);
    #[allow(clippy::cast_precision_loss)] // a mesh's corners
    let c = mesh
        .vertices
        .iter()
        .map(|v| glam::Vec3::from(v.position))
        .sum::<glam::Vec3>()
        / n as f32;
    mesh.vertices
        .iter()
        .map(|v| glam::Vec3::from(v.position).distance(c))
        .fold(0.0, f32::max)
}

/// How far `p` lies from the triangle `t`.
fn distance_to_triangle(p: glam::Vec3, [a, b, c]: &[glam::Vec3; 3]) -> f32 {
    let (ab, ac, ap) = (*b - *a, *c - *a, p - *a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return p.distance(*a);
    }
    let bp = p - *b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return p.distance(*b);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return p.distance(*a + ab * v);
    }
    let cp = p - *c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return p.distance(*c);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return p.distance(*a + ac * w);
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return p.distance(*b + (*c - *b) * w);
    }
    let denom = 1.0 / (va + vb + vc);
    let (v, w) = (vb * denom, vc * denom);
    p.distance(*a + ab * v + ac * w)
}

/// The pixels one metre covers at one metre from the eye, on a frame `height` pixels tall seen
/// with vertical field of view `fov_y` (radians): at distance `d` a metre covers this over `d`.
#[must_use]
pub fn pixels_at_one_metre(height: u32, fov_y: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // a frame's height
    let h = height as f32;
    h / (2.0 * dereth_primitives::num::math::tanf(0.5 * fov_y).max(1e-6))
}

/// The level a part draws, and the level it is fading into with how far it has faded (0 not at
/// all, 1 wholly).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    pub level: u8,
    pub next: u8,
    pub fade: f32,
}

impl Pick {
    /// The part drawing `level` alone.
    #[must_use]
    pub const fn only(level: u8) -> Self {
        Self {
            level,
            next: level,
            fade: 0.0,
        }
    }
}

/// The distances at which each level hands over to the next: retail's own (`switches`), or
/// farther where the next level still strays by more than `error_px` there (`errors`, each
/// level's error against the full mesh, drawn at `scale`, with `k` pixels to a metre at a metre);
/// never nearer than the end of the fade before it, `band` of its distance past it.
#[must_use]
pub fn hand_overs(
    errors: &[f32],
    switches: &[f32],
    scale: f32,
    k: f32,
    error_px: f32,
    band: f32,
) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::with_capacity(switches.len());
    hand_overs_into(errors, switches, scale, k, error_px, band, &mut out);
    out
}

/// [`hand_overs`], written over `out`.
fn hand_overs_into(
    errors: &[f32],
    switches: &[f32],
    scale: f32,
    k: f32,
    error_px: f32,
    band: f32,
    out: &mut Vec<f32>,
) {
    out.clear();
    for (i, s) in switches.iter().enumerate() {
        let strays = errors.get(i + 1).copied().unwrap_or(0.0) * scale * k / error_px.max(1e-3);
        let after = out.last().map_or(0.0, |h| h * (1.0 + band.max(0.0)));
        out.push(s.max(strays).max(after));
    }
}

/// The level a part at distance `d` draws, given its levels' hand-over distances
/// ([`hand_overs`]): from each hand-over it fades into the next level over `band` of the
/// distance, or until the next hand-over if that comes sooner, and draws the next alone beyond.
/// Levels whose hand-overs fall together are passed over: the part fades straight from the
/// level before them into the level after.
#[must_use]
pub fn pick(hand_overs: &[f32], d: f32, band: f32) -> Pick {
    let mut level = 0usize;
    let mut i = 0usize;
    while let Some(h) = hand_overs.get(i).copied() {
        if d < h {
            break;
        }
        // The last of the hand-overs at this distance.
        let mut j = i;
        while hand_overs.get(j + 1).is_some_and(|n| *n <= h) {
            j += 1;
        }
        let end =
            (h * (1.0 + band.max(0.0))).min(hand_overs.get(j + 1).copied().unwrap_or(f32::MAX));
        if d < end && end > h {
            let fade = ((d - h) / (end - h)).clamp(0.0, 1.0);
            return Pick {
                level: u8::try_from(i).unwrap_or(u8::MAX),
                next: u8::try_from(j + 1).unwrap_or(u8::MAX),
                fade,
            };
        }
        level = j + 1;
        i = j + 1;
    }
    Pick::only(u8::try_from(level).unwrap_or(u8::MAX))
}

/// The most degrade records whose levels' errors are measured in one frame; a record not yet
/// measured draws retail's level meanwhile.
pub const ERRORS_PER_FRAME: usize = 64;

/// A group's meshes, whether any of its levels turns, and its levels' errors once measured.
pub(crate) type GroupInfo<'a> = (&'a [u32], bool, Option<&'a [f32]>);

/// What records share: their levels and their turns. Records alike share their levels' errors.
type GroupKey = (Vec<u32>, Vec<u8>);

/// The records alike ([`GroupKey`]): their levels' errors, measured once; `None` until a mesh of
/// theirs is held. And for those retail gives no distances for, the distances their levels are
/// drawn to, worked out once a frame.
#[derive(Debug, Default)]
struct Group {
    /// Their levels' meshes and turns.
    levels: Vec<u32>,
    modes: Vec<u8>,
    /// Whether any of their levels turns toward the viewer.
    turns: bool,
    /// Whether they are a tree's, drawn far off as an impostor; `None` until it is known.
    tree: Option<bool>,
    errors: Option<Errors>,
    switches: Vec<f32>,
    /// The frame `switches` was worked out for.
    switches_frame: u64,
    /// The last frame their errors could not be measured, a mesh of theirs not being held: the
    /// other records alike are not measured again that frame, as they would fail alike.
    unmeasured_frame: u64,
}

/// What [`Lod::prepare`] works each record's hand-overs out in, kept from frame to frame.
#[derive(Debug, Default)]
struct Scratch {
    errors: Vec<f32>,
    switches: Vec<f32>,
    hand_overs: Vec<f32>,
}

/// One block's records' hand-over distances, kept from frame to frame: for each record, where
/// its distances start in `distances` and how many there are, with the level its impostor
/// stands as; `None` for a record not worked out yet.
#[derive(Debug, Default)]
struct HandOvers {
    /// The count of the art pack's distance changes they were worked out under
    /// ([`Lod::pack_distances`]).
    pack_distances: u64,
    at: Vec<Option<(u32, u8, Option<u8>)>>,
    distances: Vec<f32>,
}

/// What a record's hand-overs are worked out under besides its own record and its group's errors,
/// every float by its bits: retail's bias at its farthest and its distance, the pixels a metre at
/// a metre, the error allowed, the fade band and how far out trees must stand as impostors; the
/// forced level; whether degrades are off; whether trees stand as impostors.
type HandOverInputs = ([u32; 6], i32, bool, bool);

/// A record's levels' errors, and its impostor's when it is a tree.
#[derive(Debug, Clone, PartialEq)]
struct Errors {
    levels: Vec<f32>,
    impostor: Option<f32>,
}

/// How far a tree's full mesh turns between two of its impostor's pictures, at most, over its
/// radius: half the angle between two views, of sixteen round it.
pub const IMPOSTOR_ERROR: f32 = 0.195;

/// The least radius of a tree drawn as an impostor, metres.
pub const IMPOSTOR_RADIUS: f32 = 3.0;

/// The levels of the blocks held, chosen frame by frame.
#[derive(Debug, Default)]
pub struct Lod {
    settings: LodSettings,
    // ORDER-OK: walked in block order; each record's choice is its own.
    records: BTreeMap<BlockId, Vec<Degrade>>,
    // ORDER-OK: looked up by record.
    turned: BTreeMap<BlockId, HashMap<u32, Mat4>>,
    /// How many of each block's records retail chooses a level for: the walk's own. Records past
    /// them (an art pack's models) have no level of retail's to keep to.
    // ORDER-OK: looked up by block.
    walked: BTreeMap<BlockId, usize>,
    /// Each block's records' groups, by record: an index into `groups`.
    // ORDER-OK: looked up by block.
    record_groups: BTreeMap<BlockId, Vec<u32>>,
    /// The records whose levels are chosen here, by block, where not all of them are: the rest
    /// are chosen on the device ([`super::instances`]).
    // ORDER-OK: looked up by block.
    cpu_records: BTreeMap<BlockId, Vec<u32>>,
    /// The groups whose levels are chosen on the device, measured here as the budget allows.
    // ORDER-OK: walked in group order.
    wanted: std::collections::BTreeSet<u32>,
    // ORDER-OK: only looked up, as a block comes.
    group_of: HashMap<GroupKey, u32>,
    groups: Vec<Group>,
    scratch: Scratch,
    /// The frames chosen for, counting this one.
    frames: u64,
    /// Each block's records' impostor levels, by record: the level the impostor stands as.
    // ORDER-OK: looked up by block.
    impostor_levels: BTreeMap<BlockId, Vec<Option<u8>>>,
    // ORDER-OK: walked in block order.
    picks: BTreeMap<BlockId, Vec<Option<Pick>>>,
    /// Each block's records' hand-over distances as last worked out, kept while nothing they are
    /// worked out from changes, so a frame only measures each record's distance against them.
    // ORDER-OK: looked up by block.
    hand_overs: BTreeMap<BlockId, HandOvers>,
    /// What every record's hand-overs were last worked out under ([`HandOverInputs`]).
    hand_over_inputs: Option<HandOverInputs>,
    /// Counts the changes to the distances an art pack's models hand over at: the blocks' kept
    /// hand-overs are worked out again after one.
    pack_distances: u64,
    /// The instances whose rows the last frame wrote with a fade or a turn, to be written back,
    /// and where each stood: its block and its place among the block's parts.
    // ORDER-OK: only looked up and compared.
    written: BTreeMap<u32, (BlockId, usize)>,
    stats: LodStats,
}

/// What a frame's choice of levels did, counted part by part as the parts are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LodTally {
    parts: u32,
    fading: u32,
    finer: u32,
    coarser: u32,
    turned: u32,
    impostors: u32,
}

impl LodTally {
    /// One degrading part drawn as `d`, retail having chosen `retail`, with levels chosen here
    /// (`on`) or retail's.
    pub fn add(&mut self, on: bool, d: &PartDraw, retail: u8) {
        if on {
            self.parts += 1;
            if d.level < retail {
                self.finer += 1;
            }
            if d.level > retail && d.impostor == 0.0 {
                self.coarser += 1;
            }
            if d.fade.is_some() {
                self.fading += 1;
            }
            if d.impostor > 0.0 {
                self.impostors += 1;
            }
        }
        if d.turned.is_some() {
            self.turned += 1;
        }
    }
}

/// What the last frame's choice of levels did, over the parts chosen on the processor: an art
/// parts chosen on the device are counted by it ([`super::Renderer::lod_stats`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LodStats {
    /// Degrading parts chosen for, and of them those fading and those drawing a finer level than
    /// retail would.
    pub parts: u32,
    pub fading: u32,
    pub finer: u32,
    /// Parts drawing a coarser level than retail's, but for a tree's impostor standing in once
    /// retail draws its last level: none, ever.
    pub coarser: u32,
    /// Parts drawn turned toward the viewer, as retail turns its cards.
    pub turned: u32,
    /// Trees drawn wholly or partly as their impostors, in view or not.
    pub impostors: u32,
    /// Records whose levels' errors were measured, in all.
    pub measured: u32,
    /// Records whose hand-over distances the last frame worked out; the rest it kept from the
    /// frames before.
    pub worked_out: u32,
}

impl Lod {
    /// How levels are chosen, from the next frame on.
    pub fn set_settings(&mut self, settings: LodSettings) {
        self.settings = settings;
    }

    #[must_use]
    pub fn settings(&self) -> LodSettings {
        self.settings
    }

    #[must_use]
    pub fn stats(&self) -> LodStats {
        self.stats
    }

    /// Keep the degrade records of the blocks the scene's changes bring, and the turns the frame
    /// gives their parts; let go of the blocks taken away.
    pub fn take(&mut self, deltas: &[SceneDelta], levels: &[BlockLevels]) {
        for d in deltas {
            match d {
                SceneDelta::BlockIn(b) | SceneDelta::BlockChanged(b) => {
                    // Records alike come together (a cover's placements of one model): each
                    // takes the group of the one before it when it is alike.
                    let mut groups: Vec<u32> = Vec::with_capacity(b.degrades.len());
                    let mut last: Option<(&Degrade, u32)> = None;
                    for r in &b.degrades {
                        let g = match last {
                            Some((l, g)) if l.levels == r.levels && l.modes == r.modes => g,
                            _ => self.group(r),
                        };
                        last = Some((r, g));
                        groups.push(g);
                    }
                    self.record_groups.insert(b.block, groups);
                    self.records.insert(b.block, b.degrades.clone());
                    self.turned.remove(&b.block);
                    self.hand_overs.remove(&b.block);
                }
                SceneDelta::BlockOut(id) => {
                    self.records.remove(id);
                    self.record_groups.remove(id);
                    self.cpu_records.remove(id);
                    self.turned.remove(id);
                    self.picks.remove(id);
                    self.walked.remove(id);
                    self.impostor_levels.remove(id);
                    self.hand_overs.remove(id);
                }
                _ => {}
            }
        }
        for l in levels {
            self.turned
                .insert(l.block, l.turned.iter().copied().collect());
            self.walked.insert(l.block, l.levels.len());
        }
    }

    /// The group of the records alike `r`, made when it is the first of them.
    fn group(&mut self, r: &Degrade) -> u32 {
        let key = (r.levels.clone(), r.modes.clone());
        if let Some(g) = self.group_of.get(&key) {
            return *g;
        }
        let g = u32::try_from(self.groups.len()).unwrap_or(u32::MAX);
        self.groups.push(Group {
            levels: r.levels.clone(),
            modes: r.modes.clone(),
            turns: r.modes.iter().any(|m| (2..=5).contains(m)),
            ..Group::default()
        });
        self.group_of.insert(key, g);
        g
    }

    /// Choose every held degrading part's level for `frame`, drawn `height` pixels tall. A
    /// record retail gives no distances for (an art pack's model) hands each level over where
    /// `distance_of` says its mesh is drawn to.
    pub fn prepare(
        &mut self,
        frame: &FrameDelta,
        tables: &AssetTables,
        height: u32,
        distance_of: &dyn Fn(u32) -> Option<f32>,
    ) {
        self.stats.parts = 0;
        self.stats.fading = 0;
        self.stats.finer = 0;
        self.stats.coarser = 0;
        self.stats.turned = 0;
        self.stats.impostors = 0;
        self.stats.worked_out = 0;
        // Off, the blocks retail stands objects on draw its levels; those past them still have
        // their levels chosen here, as on.
        let everywhere = self.settings.on;
        let origin = frame.origin;
        if !everywhere {
            self.picks.retain(|b, _| past_retail(*b, origin));
            if !self.records.keys().any(|b| past_retail(*b, origin)) {
                return;
            }
        }
        self.frames += 1;
        let frame_no = self.frames;
        let k = pixels_at_one_metre(height, frame.camera.fov_y);
        let eye = frame.camera.eye;
        let mut budget = ERRORS_PER_FRAME;
        let chosen = if everywhere {
            self.settings
        } else {
            LodSettings::ON
        };
        let (error_px, fade, impostors) = (chosen.error_px, chosen.fade, chosen.impostors);
        // Retail's bias follows its frame rate from frame to frame; its hand-overs are taken at
        // the farthest it slides them, so a still view stays still and no level comes sooner
        // than retail's would at any frame rate.
        let farthest = DegradeBias {
            bias: frame.degrade.bias.max(1.0),
            ..frame.degrade
        };
        let cap = impostor_cap(frame.scenery_reach, fade, &frame.degrade);
        // The hand-overs kept from the frames before stand while what they were worked out under
        // stands: the bias, the picture's scale, the settings, and an art pack's distances.
        let inputs: HandOverInputs = (
            [
                farthest.bias.to_bits(),
                farthest.distance.to_bits(),
                k.to_bits(),
                error_px.to_bits(),
                fade.to_bits(),
                cap.to_bits(),
            ],
            farthest.force,
            farthest.disabled,
            impostors,
        );
        if self.hand_over_inputs != Some(inputs) {
            self.hand_overs.clear();
            self.hand_over_inputs = Some(inputs);
        }
        // The distances an art pack's models hand over at, worked out again each frame for every
        // group they were worked out for before: one that changed counts.
        for group in self.groups.iter_mut().filter(|g| g.switches_frame != 0) {
            let before = std::mem::take(&mut group.switches);
            group.switches.extend(
                group.levels[..group.levels.len().saturating_sub(1)]
                    .iter()
                    .map(|m| distance_of(*m).unwrap_or(f32::INFINITY)),
            );
            group.switches_frame = frame_no;
            if group.switches != before {
                self.pack_distances += 1;
            }
        }
        let pack_distances = self.pack_distances;
        let Self {
            records,
            record_groups,
            cpu_records,
            groups,
            scratch,
            picks: all_picks,
            impostor_levels: all_impostor_levels,
            hand_overs: all_hand_overs,
            stats,
            ..
        } = self;
        for (block, records) in records.iter() {
            if !everywhere && !past_retail(*block, origin) {
                continue;
            }
            let (x, y) = (i32::from(block >> 8), i32::from(block & 0xFF));
            #[allow(clippy::cast_precision_loss)] // a few hundred blocks
            let offset = Vec3::new(
                (x - frame.origin[0]) as f32 * 192.0,
                (y - frame.origin[1]) as f32 * 192.0,
                0.0,
            );
            let picks = all_picks.entry(*block).or_default();
            picks.resize(records.len(), None);
            let impostor_levels = all_impostor_levels.entry(*block).or_default();
            impostor_levels.resize(records.len(), None);
            let of = record_groups.get(block).map_or(&[][..], Vec::as_slice);
            let kept = all_hand_overs.entry(*block).or_default();
            if kept.pack_distances != pack_distances || kept.at.len() != records.len() {
                kept.pack_distances = pack_distances;
                kept.at.clear();
                kept.at.resize(records.len(), None);
                kept.distances.clear();
            }
            // A record's hand-overs are worked out once (its group's errors, once measured, never
            // change) and kept; each frame after only measures its distance against them.
            let mut choose = |i: usize| -> (Option<Pick>, Option<u8>) {
                let Some(r) = records.get(i) else {
                    return (None, None);
                };
                if let Some((start, len, impostor)) = kept.at.get(i).copied().flatten() {
                    let at = start as usize;
                    let hand_overs = kept.distances.get(at..at + usize::from(len)).unwrap_or(&[]);
                    let p = pick(hand_overs, (r.centre + offset).distance(eye), fade);
                    return (Some(p), impostor);
                }
                let Some(group) = of.get(i).and_then(|g| groups.get_mut(*g as usize)) else {
                    return (None, None);
                };
                if group.errors.is_none() {
                    if budget == 0 || group.unmeasured_frame == frame_no {
                        return (None, None);
                    }
                    let Some(e) = record_errors(r, tables) else {
                        group.unmeasured_frame = frame_no;
                        return (None, None);
                    };
                    budget -= 1;
                    stats.measured += 1;
                    group.errors = Some(e);
                }
                let switches = &mut scratch.switches;
                switches.clear();
                if r.bands.len() >= 2 {
                    retail_switches_into(r, &farthest, switches);
                } else {
                    if group.switches_frame != frame_no {
                        group.switches.clear();
                        group.switches.extend(
                            r.levels[..r.levels.len().saturating_sub(1)]
                                .iter()
                                .map(|m| distance_of(*m).unwrap_or(f32::INFINITY)),
                        );
                        group.switches_frame = frame_no;
                    }
                    pack_switches_into(&group.switches, &farthest, switches);
                }
                let Some(e) = group.errors.as_ref() else {
                    return (None, None);
                };
                let errors = &mut scratch.errors;
                errors.clear();
                errors.extend_from_slice(&e.levels);
                let scale = if r.scale_z == 0.0 {
                    1.0
                } else {
                    r.scale_z.abs()
                };
                // A tree's impostor takes the place of its level that draws nothing, or comes
                // after its last; retail hands over to it where it hands over to nothing.
                let mut impostor = None;
                if let (true, Some(at)) = (impostors, e.impostor) {
                    if r.levels.len() > 1 && r.levels.last() == Some(&0) {
                        let i = r.levels.len() - 1;
                        errors[i] = at;
                        impostor = u8::try_from(i).ok();
                    } else {
                        errors.push(at);
                        switches.push(0.0);
                        impostor = u8::try_from(r.levels.len()).ok();
                    }
                }
                hand_overs_into(
                    errors,
                    switches,
                    scale,
                    k,
                    error_px,
                    fade,
                    &mut scratch.hand_overs,
                );
                // A tree is wholly its impostor before it could stand where the far forest draws
                // its block's trees as pictures: the hand-overs past it come at it, and the
                // levels nearer keep their own.
                if impostor.is_some() {
                    pull_in(&mut scratch.hand_overs, cap, fade);
                }
                stats.worked_out += 1;
                if let (Some(slot), Ok(start), Ok(len)) = (
                    kept.at.get_mut(i),
                    u32::try_from(kept.distances.len()),
                    u8::try_from(scratch.hand_overs.len()),
                ) {
                    *slot = Some((start, len, impostor));
                    kept.distances.extend_from_slice(&scratch.hand_overs);
                }
                let p = pick(&scratch.hand_overs, (r.centre + offset).distance(eye), fade);
                (Some(p), impostor)
            };
            // Only the records chosen here; those chosen on the device are never asked of.
            match cpu_records.get(block) {
                Some(cpu) => {
                    for &i in cpu {
                        let i = i as usize;
                        let (p, impostor) = choose(i);
                        if let (Some(a), Some(b)) = (picks.get_mut(i), impostor_levels.get_mut(i)) {
                            (*a, *b) = (p, impostor);
                        }
                    }
                }
                None => {
                    for (i, (a, b)) in picks.iter_mut().zip(impostor_levels.iter_mut()).enumerate()
                    {
                        (*a, *b) = choose(i);
                    }
                }
            }
        }
        // The groups whose levels are chosen on the device, measured as the budget allows.
        let Self {
            wanted,
            groups,
            stats,
            ..
        } = self;
        for g in wanted.iter() {
            if budget == 0 {
                break;
            }
            let Some(group) = groups.get_mut(*g as usize) else {
                continue;
            };
            if group.errors.is_some() {
                continue;
            }
            let sample = Degrade {
                levels: group.levels.clone(),
                modes: group.modes.clone(),
                ..Degrade::default()
            };
            if let Some(e) = record_errors(&sample, tables) {
                budget -= 1;
                stats.measured += 1;
                group.errors = Some(e);
            }
        }
        wanted.retain(|g| groups.get(*g as usize).is_some_and(|x| x.errors.is_none()));
    }

    /// Choose levels here only for block `block`'s records `cpu`, or every one with `None`: the
    /// rest are chosen on the device.
    pub(crate) fn set_cpu_records(&mut self, block: BlockId, cpu: Option<Vec<u32>>) {
        match cpu {
            Some(c) => {
                self.cpu_records.insert(block, c);
            }
            None => {
                self.cpu_records.remove(&block);
            }
        }
    }

    /// Measure group `g`'s levels' errors as the budget allows, its levels being chosen on the
    /// device.
    pub(crate) fn want(&mut self, g: u32) {
        if self
            .groups
            .get(g as usize)
            .is_some_and(|x| x.errors.is_none())
        {
            self.wanted.insert(g);
        }
    }

    /// The group of block `block`'s record `record`.
    #[must_use]
    pub(crate) fn group_of_record(&self, block: BlockId, record: u32) -> Option<u32> {
        self.record_groups
            .get(&block)
            .and_then(|g| g.get(record as usize))
            .copied()
    }

    /// Group `g`'s meshes, whether any of its levels turns, and its levels' errors once measured.
    #[must_use]
    pub(crate) fn group_info(&self, g: u32) -> Option<GroupInfo<'_>> {
        self.groups.get(g as usize).map(|x| {
            (
                x.levels.as_slice(),
                x.turns,
                x.errors.as_ref().map(|e| e.levels.as_slice()),
            )
        })
    }

    /// Group `g`'s impostor's error, once its levels' errors are measured, when it is a tree's.
    #[must_use]
    pub(crate) fn group_impostor_error(&self, g: u32) -> Option<f32> {
        self.groups
            .get(g as usize)
            .and_then(|x| x.errors.as_ref())
            .and_then(|e| e.impostor)
    }

    /// Whether block `block`'s record `record` is a tree's, drawn far off as an impostor, whoever
    /// chooses its levels: its group's impostor's error is measured and impostors are drawn.
    #[must_use]
    pub(crate) fn record_is_tree(&self, block: BlockId, record: u32) -> bool {
        self.settings.impostors
            && self
                .group_of_record(block, record)
                .and_then(|g| self.group_impostor_error(g))
                .is_some()
    }

    /// How many of block `block`'s records retail chooses a level for: every one until the walk
    /// says.
    #[must_use]
    pub(crate) fn walked(&self, block: BlockId) -> usize {
        self.walked.get(&block).copied().unwrap_or(usize::MAX)
    }

    /// Whether group `g` is a tree's, drawn far off as an impostor; `None` while its first mesh
    /// is not held.
    pub(crate) fn group_tree(&mut self, g: u32, tables: &AssetTables) -> Option<bool> {
        let group = self.groups.get_mut(g as usize)?;
        if group.tree.is_none() {
            group.tree = tree_of(&group.levels, &group.modes, tables);
        }
        group.tree
    }

    /// Block `block`'s degrade records.
    #[must_use]
    pub(crate) fn records(&self, block: BlockId) -> &[Degrade] {
        self.records.get(&block).map_or(&[][..], Vec::as_slice)
    }

    /// Whether this frame chose levels for block `block`'s records here: every block's while the
    /// settings are on, and otherwise those past the blocks retail stands objects on.
    #[must_use]
    pub fn chooses(&self, block: BlockId) -> bool {
        self.settings.on || self.picks.contains_key(&block)
    }

    /// What this frame chose for block `block`'s records, to ask of part after part
    /// ([`BlockLod::part`]).
    #[must_use]
    pub fn block(&self, block: BlockId) -> BlockLod<'_> {
        BlockLod {
            picks: self.picks.get(&block).map_or(&[][..], Vec::as_slice),
            impostor_levels: self
                .impostor_levels
                .get(&block)
                .map_or(&[][..], Vec::as_slice),
            records: self.records.get(&block).map_or(&[][..], Vec::as_slice),
            record_groups: self
                .record_groups
                .get(&block)
                .map_or(&[][..], Vec::as_slice),
            groups: &self.groups,
            turned: self.turned.get(&block),
            walked: self.walked.get(&block).copied().unwrap_or(usize::MAX),
        }
    }

    /// What degrading part `part` of block `block` draws this frame, retail having chosen
    /// `retail` for it ([`BlockLod::part`]).
    #[must_use]
    pub fn part(&self, block: BlockId, part: &PartItems, retail: u8, outdoors: bool) -> PartDraw {
        self.block(block).part(part, retail, outdoors)
    }

    /// Whether block `block`'s record `record` is a tree's, drawn far off as an impostor.
    #[must_use]
    pub fn is_tree(&self, block: BlockId, record: u32) -> bool {
        self.block(block).is_tree(record)
    }

    /// The first level's GfxObj of block `block`'s record `record`.
    #[must_use]
    pub fn first_level(&self, block: BlockId, record: u32) -> Option<u32> {
        self.records
            .get(&block)
            .and_then(|r| r.get(record as usize))
            .and_then(|d| d.levels.first().copied())
    }

    /// The instance rows to write this frame for the drawn block parts in `draws` (each a block,
    /// its place among the block's parts, what it draws and the level retail chose), and for the
    /// parts written last frame and not now, written back as they stand. `tally` is what the
    /// frame's levels did over every degrading part drawn, for the stats.
    pub fn rows(
        &mut self,
        scene: &GpuScene,
        draws: &[(BlockId, usize, PartDraw, u8)],
        tally: LodTally,
    ) -> Vec<(u32, Vec<u8>)> {
        self.stats.parts += tally.parts;
        self.stats.fading += tally.fading;
        self.stats.finer += tally.finer;
        self.stats.coarser += tally.coarser;
        self.stats.turned += tally.turned;
        self.stats.impostors += tally.impostors;
        let mut out = Vec::new();
        let mut now = BTreeMap::new();
        for (block, k, d, _) in draws {
            if d.fade.is_none() && d.turned.is_none() {
                continue;
            }
            let Some(b) = scene.blocks.get(block) else {
                continue;
            };
            let Some(p) = b.parts.get(*k) else {
                continue;
            };
            now.insert(p.instance, (*block, *k));
            out.extend(part_rows(b, p, d));
        }
        for (gone, (block, k)) in &self.written {
            if now.contains_key(gone) {
                continue;
            }
            // Where it stood, unless its block was built again since.
            let at = scene
                .blocks
                .get(block)
                .and_then(|b| b.parts.get(*k).map(|p| (b, p)))
                .filter(|(_, p)| p.instance == *gone);
            if let Some((b, p)) = at {
                out.extend(part_rows(b, p, &PartDraw::default()));
                continue;
            }
            for b in scene.blocks.values() {
                if let Some(p) = b.parts.iter().find(|p| p.instance == *gone) {
                    out.extend(part_rows(b, p, &PartDraw::default()));
                }
            }
        }
        self.written = now;
        out
    }
}

/// What one frame chose for one block's records, from [`Lod::block`].
#[derive(Debug, Clone, Copy)]
pub struct BlockLod<'a> {
    picks: &'a [Option<Pick>],
    impostor_levels: &'a [Option<u8>],
    records: &'a [Degrade],
    record_groups: &'a [u32],
    groups: &'a [Group],
    turned: Option<&'a HashMap<u32, Mat4>>,
    walked: usize,
}

impl BlockLod<'_> {
    /// What degrading part `part` of the block draws this frame, retail having chosen `retail`
    /// for it: the level, the level it fades into and how far, and how it is turned toward the
    /// viewer. Never a coarser level than retail's. Only a part standing `outdoors` is drawn as
    /// an impostor; one in a room keeps its last level instead.
    #[must_use]
    pub fn part(&self, part: &PartItems, retail: u8, outdoors: bool) -> PartDraw {
        let record = part.degrade;
        let turned_at = |r: u32| self.turned.and_then(|t| t.get(&r)).copied();
        let picked = record.and_then(|r| self.picks.get(r as usize).copied().flatten());
        let Some(p) = picked else {
            // Retail's level, turned toward the viewer as retail turns it.
            return PartDraw {
                level: retail,
                turned: record.and_then(turned_at),
                ..PartDraw::default()
            };
        };
        let walked = record.is_some_and(|r| (r as usize) < self.walked);
        let impostor = record.and_then(|r| self.impostor_levels.get(r as usize).copied().flatten());
        // In a room, the impostor's level is never reached: the last level stays.
        let p = match (outdoors, impostor) {
            (false, Some(i)) if p.next >= i => Pick::only(p.level.min(i.saturating_sub(1))),
            _ => p,
        };
        let impostor = impostor.filter(|_| outdoors);
        // Retail's level keeps the part from coarser levels; the impostor may stand in once
        // retail draws the last level before it.
        let retail = match (walked, impostor) {
            (false, _) => u8::MAX,
            (true, Some(i)) if retail.saturating_add(1) >= i => retail.max(i),
            (true, _) => retail,
        };
        let level = p.level.min(retail);
        let fade = (p.fade > 0.0 && p.next > p.level && p.next <= retail && level == p.level)
            .then_some((p.next, p.fade));
        // A record's turns are looked at only when its group has any.
        let turning = record
            .and_then(|r| self.record_groups.get(r as usize))
            .and_then(|g| self.groups.get(*g as usize))
            .is_some_and(|g| g.turns);
        let modes = record
            .filter(|_| turning)
            .and_then(|r| self.records.get(r as usize))
            .map(|d| d.modes.as_slice());
        let turns = |l: u8| {
            modes
                .and_then(|m| m.get(usize::from(l)))
                .is_some_and(|m| (2..=5).contains(m))
        };
        let turned = record
            .filter(|_| turns(level) || fade.is_some_and(|(n, _)| turns(n)))
            .and_then(turned_at);
        let to_impostor = match (impostor, fade) {
            (Some(i), _) if level == i => 1.0,
            (Some(i), Some((n, f))) if n == i => f,
            _ => 0.0,
        };
        PartDraw {
            level,
            fade,
            turned,
            impostor: to_impostor,
        }
    }

    /// Whether the block's record `record` is a tree's, drawn far off as an impostor.
    #[must_use]
    pub fn is_tree(&self, record: u32) -> bool {
        self.impostor_levels
            .get(record as usize)
            .copied()
            .flatten()
            .is_some()
    }
}

impl super::Renderer {
    /// How levels are chosen.
    #[must_use]
    pub fn lod(&self) -> &Lod {
        &self.lod
    }

    /// How levels are chosen, from the next frame on.
    pub fn set_lod(&mut self, settings: LodSettings) {
        self.lod.set_settings(settings);
    }
}

/// What a degrading part draws this frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PartDraw {
    pub level: u8,
    /// The level it fades into, and how far (0 to 1).
    pub fade: Option<(u8, f32)>,
    /// Where it stands turned toward the viewer, block-local, its scale left out.
    pub turned: Option<Mat4>,
    /// How much of it its impostor draws: 0 none, 1 all of it, between while it fades in.
    pub impostor: f32,
}

/// A block part's instance row as `d` draws it: turned where it turns, with its fade.
fn part_rows(b: &BlockGpu, p: &PartItems, d: &PartDraw) -> Vec<(u32, Vec<u8>)> {
    turned_rows(p, d, |transform, fade| {
        GpuScene::instance_bytes_faded(transform, p.scale, b.slot, class::SUN, 0, super::NONE, fade)
    })
}

/// A part's instance rows as `d` draws it, each made by `row` from a transform and the fade: its
/// own instance as placed, and the instance its turning levels draw with turned toward the
/// viewer; both carry the fade.
pub fn turned_rows(
    p: &PartItems,
    d: &PartDraw,
    row: impl Fn(Mat4, (f32, u8, u8)) -> Vec<u8>,
) -> Vec<(u32, Vec<u8>)> {
    let fade = d
        .fade
        .map_or((0.0, d.level, d.level), |(n, f)| (f, d.level, n));
    let lifted = |transform| {
        let mut r = row(transform, fade);
        GpuScene::lift_row(&mut r, p.lifts);
        r
    };
    let mut out = vec![(p.instance, lifted(p.transform))];
    if let Some(t) = p.turned {
        let transform = d
            .turned
            .map_or(p.transform, |m| m * Mat4::from_scale(p.scale));
        out.push((t, lifted(transform)));
    }
    out
}

/// Write a fade into an instance's row: how far its part has faded from level `lower` into
/// level `next`, both levels in one number (`lower` plus 256 times `next`).
pub fn fade_row(row: &mut [u8], fade: f32, lower: u8, next: u8) {
    let at = 6 * 16 + 8;
    if row.len() >= at + 8 {
        let both = f32::from(u16::from(lower) | (u16::from(next) << 8));
        row[at..at + 4].copy_from_slice(&fade.to_le_bytes());
        row[at + 4..at + 8].copy_from_slice(&both.to_le_bytes());
    }
}

/// Each level's error against the record's first, in the meshes' units, and its impostor's when
/// it is a tree; `None` while a mesh of it is not held.
fn record_errors(r: &Degrade, tables: &AssetTables) -> Option<Errors> {
    let first = *r.levels.first()?;
    let full = tables.mesh(MeshKey::GfxObj(first))?;
    let tree = tree_of(&r.levels, &r.modes, tables)?;
    let r_full = radius(full);
    let mut out = Vec::with_capacity(r.levels.len());
    for (i, g) in r.levels.iter().enumerate() {
        let turned = r.modes.get(i).is_some_and(|m| (2..=5).contains(m));
        let level = if *g == 0 {
            None
        } else {
            Some(tables.mesh(MeshKey::GfxObj(*g))?)
        };
        out.push(if i == 0 && !turned {
            0.0
        } else {
            level_error(full, level, turned)
        });
    }
    Some(Errors {
        levels: out,
        impostor: tree.then_some(IMPOSTOR_ERROR * r_full),
    })
}

/// Whether a record of `levels` and `modes` is a tree's: its full mesh large and mostly foliage,
/// and none of its levels turning. `None` while its full mesh is not held.
fn tree_of(levels: &[u32], modes: &[u8], tables: &AssetTables) -> Option<bool> {
    let full = tables.mesh(MeshKey::GfxObj(*levels.first()?))?;
    let foliage = full
        .subsets
        .iter()
        .filter(|s| match s.surface {
            crate::assets::SurfaceRef::Surface(id) => tables
                .material(&crate::assets::MaterialKey::plain(id))
                .is_some_and(|m| m.kind == crate::assets::Kind::Foliage),
            _ => false,
        })
        .count();
    let turns = modes.iter().any(|m| (2..=5).contains(m));
    Some(!turns && radius(full) >= IMPOSTOR_RADIUS && foliage * 2 >= full.subsets.len().max(1))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;
    use crate::assets::Vertex;

    fn band(levels: &[[f32; 3]]) -> Degrade {
        Degrade {
            levels: (1..=u32::try_from(levels.len()).unwrap()).collect(),
            bands: levels.to_vec(),
            modes: vec![1; levels.len()],
            scale_z: 1.0,
            ..Degrade::default()
        }
    }

    #[test]
    fn retail_hands_over_at_its_ideal_distance_past_its_degrade_distance() {
        let tree = band(&[
            [10.0, 25.0, 50.0],
            [25.0, 50.0, 100.0],
            [50.0, 100.0, 200.0],
            [f32::MAX, f32::MAX, f32::MAX],
        ]);
        let even = DegradeBias::default();
        assert_eq!(retail_switches(&tree, &even), vec![75.0, 100.0, 150.0]);
        let good = DegradeBias { bias: 1.0, ..even };
        assert_eq!(retail_switches(&tree, &good), vec![100.0, 150.0, 250.0]);
        let bad = DegradeBias { bias: -1.0, ..even };
        assert_eq!(retail_switches(&tree, &bad), vec![60.0, 75.0, 100.0]);
        let big = Degrade {
            scale_z: 2.0,
            ..tree.clone()
        };
        assert_eq!(retail_switches(&big, &even), vec![150.0, 200.0, 300.0]);
        let off = DegradeBias {
            disabled: true,
            ..even
        };
        assert!(retail_switches(&tree, &off).iter().all(|s| s.is_infinite()));
        let forced = DegradeBias { force: 2, ..even };
        assert_eq!(
            retail_switches(&tree, &forced),
            vec![0.0, 0.0, f32::INFINITY]
        );
    }

    fn quad(z: f32) -> Mesh {
        let v = |x: f32, y: f32| Vertex {
            position: [x, y, z],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        };
        Mesh {
            vertices: vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)],
            indices: vec![0, 1, 2, 0, 2, 3],
            ..Mesh::default()
        }
    }

    #[test]
    fn a_level_errs_by_how_far_the_full_mesh_lies_from_its_surface() {
        let full = quad(0.0);
        assert_eq!(level_error(&full, Some(&full), false), 0.0);
        assert!((level_error(&full, Some(&quad(0.25)), false) - 0.25).abs() < 1e-6);
        let r = radius(&full);
        assert!((level_error(&full, None, false) - 2.0 * r).abs() < 1e-6);
        assert!((level_error(&full, Some(&full), true) - r).abs() < 1e-6);
        // A corner past the level's edge errs by its distance to that edge.
        let mut wide = quad(0.0);
        wide.vertices[1].position = [3.0, 0.0, 0.0];
        assert!((level_error(&wide, Some(&full), false) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_level_takes_over_once_it_strays_by_less_than_the_bar_and_not_before_retail() {
        // Two levels, the second erring by 0.5 m; retail hands over at 75 m.
        let k = pixels_at_one_metre(2160, 60f32.to_radians());
        let h = hand_overs(&[0.0, 0.5], &[75.0], 1.0, k, 10.0, FADE_BAND);
        // 0.5 m is ten pixels at 0.5 k / 10 metres, well past retail's 75.
        assert!((h[0] - 0.5 * k / 10.0).abs() < 1e-3, "{h:?}");
        assert!(h[0] > 75.0);
        // On a small frame, retail's own distance is the farther.
        let small = pixels_at_one_metre(240, 60f32.to_radians());
        assert_eq!(
            hand_overs(&[0.0, 0.5], &[75.0], 1.0, small, 10.0, FADE_BAND),
            vec![75.0]
        );
        // A larger part errs by more, and hands over farther.
        let big = hand_overs(&[0.0, 0.5], &[75.0], 2.0, k, 10.0, FADE_BAND);
        assert!((big[0] - 2.0 * h[0]).abs() < 1e-3);
        // Hand-overs never come nearer than the one before.
        assert_eq!(
            hand_overs(&[0.0, 5.0, 0.0], &[10.0, 20.0], 1.0, 100.0, 10.0, 0.0),
            vec![50.0, 50.0]
        );
        // And with a band, no nearer than the end of the fade before.
        assert_eq!(
            hand_overs(&[0.0, 5.0, 0.0], &[10.0, 20.0], 1.0, 100.0, 10.0, 0.5),
            vec![50.0, 75.0]
        );
    }

    #[test]
    fn a_part_fades_into_its_next_level_past_the_hand_over_and_draws_it_alone_beyond() {
        let h = [100.0, 300.0];
        assert_eq!(pick(&h, 50.0, FADE_BAND), Pick::only(0));
        assert_eq!(pick(&h, 99.9, FADE_BAND), Pick::only(0));
        let p = pick(&h, 100.0, FADE_BAND);
        assert_eq!((p.level, p.next, p.fade), (0, 1, 0.0));
        let p = pick(&h, 107.5, FADE_BAND);
        assert_eq!((p.level, p.next), (0, 1));
        assert!((p.fade - 0.5).abs() < 1e-5, "{p:?}");
        assert_eq!(pick(&h, 115.0, FADE_BAND), Pick::only(1));
        assert_eq!(pick(&h, 200.0, FADE_BAND), Pick::only(1));
        let p = pick(&h, 330.0, FADE_BAND);
        assert_eq!((p.level, p.next), (1, 2));
        assert_eq!(pick(&h, 400.0, FADE_BAND), Pick::only(2));
        // A hand-over that comes before the band ends cuts the fade short.
        let close = [100.0, 105.0];
        let p = pick(&close, 102.5, FADE_BAND);
        assert!((p.fade - 0.5).abs() < 1e-5, "{p:?}");
        assert_eq!(pick(&close, 104.0, FADE_BAND).next, 1);
        // No hand-overs: the first level.
        assert_eq!(pick(&[], 1.0e6, FADE_BAND), Pick::only(0));
        // Hand-overs that fall together are passed over: from level 0 straight into level 2.
        let together = [100.0, 100.0, 300.0];
        let p = pick(&together, 107.5, FADE_BAND);
        assert_eq!((p.level, p.next), (0, 2), "{p:?}");
        assert_eq!(pick(&together, 120.0, FADE_BAND), Pick::only(2));
        // With no band, the next level takes over at once.
        assert_eq!(pick(&h, 100.0, 0.0), Pick::only(1));
        assert_eq!(pick(&h, 99.9, 0.0), Pick::only(0));
    }

    /// GfxObjs 1 and 2: a meter-wide triangle, and the same one lifted 5 cm.
    struct Levels;

    impl crate::assets::AssetSource for Levels {
        fn gfxobj(&mut self, id: u32) -> Option<Mesh> {
            #[allow(clippy::cast_precision_loss)] // 1 or 2
            let z = (id as f32 - 1.0) * 0.05;
            let corner = |x: f32, y: f32| Vertex {
                position: [x, y, z],
                normal: [0.0, 0.0, 1.0],
                uv: [x, y],
            };
            Some(Mesh::from_triangle_lists([(
                crate::assets::SubsetInfo {
                    surface: crate::assets::SurfaceRef::Surface(7),
                    two_sided: false,
                    tiled: false,
                },
                vec![corner(0.0, 0.0), corner(1.0, 0.0), corner(0.0, 1.0)],
            )]))
        }

        fn cell_piece(&mut self, _key: crate::assets::CellPieceKey) -> Option<Mesh> {
            None
        }

        fn appearance(
            &mut self,
            _surface: u32,
            _look: Option<&crate::scene::PartLook>,
        ) -> crate::assets::Appearance {
            crate::assets::Appearance::default()
        }

        fn material(
            &mut self,
            _key: &crate::assets::MaterialKey,
            _used: crate::assets::Use,
        ) -> crate::assets::Material {
            crate::assets::Material {
                kind: crate::assets::Kind::Item,
                surface_type: 2,
                data_category: Some(1),
                texture: None,
                clip_map: false,
                alpha_test: None,
                vertex_alpha: 255,
                vertex_colored: false,
                translucent: None,
                additive: false,
                blended: false,
                src_blend: crate::assets::BlendFactor::One,
                dst_blend: crate::assets::BlendFactor::Zero,
                z_write: true,
                fogged: true,
                solid: None,
                luminosity: 0.0,
                diffuse: 1.0,
                maps: None,
            }
        }

        fn texture(
            &mut self,
            _key: &crate::assets::TextureKey,
            _clip_map: bool,
        ) -> Option<crate::assets::Texture> {
            None
        }

        fn terrain_cells(
            &mut self,
            _block: BlockId,
            _t: &crate::scene::Terrain,
        ) -> Vec<crate::assets::TerrainCell> {
            Vec::new()
        }

        fn sky_objects(&mut self) -> Vec<u32> {
            Vec::new()
        }

        fn sky(&mut self, _id: u32) -> Option<Mesh> {
            None
        }
    }

    /// Choosing levels again for a block of many parts of one model, as an art pack's ground
    /// cover makes, allocates nothing for each of them: the records alike are measured once, and
    /// a frame's choice reuses what the last one worked in. The levels chosen still follow each
    /// part's distance.
    #[test]
    fn choosing_levels_again_allocates_nothing_for_each_part() {
        const BLOCK: BlockId = 0xA9B4;
        const PARTS: usize = 2000;
        let degrades: Vec<Degrade> = (0..PARTS)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)] // under two thousand
                let (x, y) = ((i % 45) as f32 * 4.0, (i / 45) as f32 * 4.0);
                Degrade {
                    levels: vec![1, 2, 0],
                    modes: vec![1; 3],
                    scale_z: 1.0,
                    centre: Vec3::new(x, y, 0.0),
                    ..Degrade::default()
                }
            })
            .collect();
        let mut vertices = Vec::new();
        for i in 0..9u8 {
            for j in 0..9u8 {
                vertices.push(Vec3::new(f32::from(i) * 24.0, f32::from(j) * 24.0, 0.0));
            }
        }
        let block = crate::scene::Block {
            block: BLOCK,
            generation: 1,
            terrain: crate::scene::Terrain {
                side: 8,
                vertices,
                cuts: vec![false; 64],
                words: [4; 81],
            },
            placements: vec![crate::scene::Placement {
                id: 0x0100_0001,
                kind: crate::scene::PlacementKind::Object,
                frame: Mat4::IDENTITY,
                scale: 1.0,
                lifts: [0.0; 2],
                parts: vec![crate::scene::Part {
                    gfxobj: 1,
                    frame: Mat4::IDENTITY,
                    scale: Vec3::ONE,
                    degrade: Some(0),
                }],
            }],
            degrades,
            rooms: Vec::new(),
            lights: Vec::new(),
            lamps: Vec::new(),
        };
        let came = SceneDelta::BlockIn(Box::new(block));
        let mut tables = AssetTables::new();
        tables.apply(&came, &mut Levels);
        let mut lod = Lod::default();
        lod.set_settings(LodSettings::ON);
        lod.take(std::slice::from_ref(&came), &[]);
        let mut frame = FrameDelta::default();
        frame.camera.eye = Vec3::new(0.0, 0.0, 2.0);
        frame.camera.fov_y = 1.0;
        frame.origin = [0xA9, 0xB4];
        let distance_of = |_mesh: u32| Some(60.0);
        lod.prepare(&frame, &tables, 2160, &distance_of);
        assert_eq!(
            lod.stats().measured,
            1,
            "the records alike are measured once"
        );

        let ((), made) = crate::counting_alloc::allocations(|| {
            lod.prepare(&frame, &tables, 2160, &distance_of);
        });
        assert!(
            made < 16,
            "{made} allocations choosing again for {PARTS} parts"
        );
        let picks = &lod.picks[&BLOCK];
        assert_eq!(picks.len(), PARTS);
        // The nearest part draws its full mesh; one 100 m off its second level, alone.
        assert_eq!(picks[0], Some(Pick::only(0)));
        assert_eq!(
            picks[25],
            Some(Pick::only(1)),
            "25 parts along is 100 m off"
        );
    }
    /// Each record's hand-overs are worked out once and kept: a frame after the first, its eye
    /// moved, works none out again, and chooses for every part what a choice made afresh at that
    /// eye chooses. Another error allowed, or another distance a pack gives its model, has them
    /// all worked out again.
    #[test]
    fn hand_overs_are_worked_out_once_and_kept_until_what_they_follow_changes() {
        const BLOCK: BlockId = 0xA9B4;
        const PARTS: usize = 300;
        let degrades: Vec<Degrade> = (0..PARTS)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)] // a few hundred
                let (x, y) = ((i % 20) as f32 * 9.0, (i / 20) as f32 * 9.0);
                let mut d = if i % 3 == 0 {
                    band(&[[20.0, 40.0, 60.0], [60.0, 90.0, 120.0], [0.0; 3]])
                } else {
                    Degrade {
                        levels: vec![1, 2, 0],
                        modes: vec![1; 3],
                        scale_z: 1.0,
                        ..Degrade::default()
                    }
                };
                d.centre = Vec3::new(x, y, 0.0);
                d
            })
            .collect();
        let came = SceneDelta::BlockIn(Box::new(crate::scene::Block {
            block: BLOCK,
            generation: 1,
            terrain: crate::scene::Terrain {
                side: 1,
                vertices: vec![Vec3::ZERO; 4],
                cuts: vec![false],
                words: [4; 81],
            },
            placements: vec![crate::scene::Placement {
                id: 0x0100_0001,
                kind: crate::scene::PlacementKind::Object,
                frame: Mat4::IDENTITY,
                scale: 1.0,
                lifts: [0.0; 2],
                parts: vec![crate::scene::Part {
                    gfxobj: 1,
                    frame: Mat4::IDENTITY,
                    scale: Vec3::ONE,
                    degrade: Some(0),
                }],
            }],
            degrades,
            rooms: Vec::new(),
            lights: Vec::new(),
            lamps: Vec::new(),
        }));
        let mut tables = AssetTables::new();
        tables.apply(&came, &mut Levels);
        let fresh_lod = || {
            let mut lod = Lod::default();
            lod.set_settings(LodSettings::ON);
            lod.take(std::slice::from_ref(&came), &[]);
            lod
        };
        let at = |x: f32| {
            let mut frame = FrameDelta::default();
            frame.camera.eye = Vec3::new(x, 0.0, 2.0);
            frame.camera.fov_y = 1.0;
            frame.origin = [0xA9, 0xB4];
            frame
        };
        let pack = |d: f32| move |_mesh: u32| Some(d);
        let mut lod = fresh_lod();
        lod.prepare(&at(0.0), &tables, 2160, &pack(60.0));
        assert_eq!(
            lod.stats().worked_out,
            300,
            "the first frame works out every record's"
        );
        for x in [7.0, 31.0, 95.0] {
            lod.prepare(&at(x), &tables, 2160, &pack(60.0));
            assert_eq!(lod.stats().worked_out, 0, "kept at x = {x}");
            let mut afresh = fresh_lod();
            afresh.prepare(&at(x), &tables, 2160, &pack(60.0));
            assert_eq!(
                lod.picks[&BLOCK], afresh.picks[&BLOCK],
                "the same choice at x = {x}"
            );
            assert_eq!(lod.impostor_levels[&BLOCK], afresh.impostor_levels[&BLOCK]);
        }
        lod.set_settings(LodSettings {
            error_px: 4.0,
            ..LodSettings::ON
        });
        lod.prepare(&at(95.0), &tables, 2160, &pack(60.0));
        assert_eq!(lod.stats().worked_out, 300, "another error allowed");
        lod.prepare(&at(95.0), &tables, 2160, &pack(80.0));
        assert_eq!(
            lod.stats().worked_out,
            300,
            "another distance the pack gives"
        );
        lod.prepare(&at(95.0), &tables, 2160, &pack(80.0));
        assert_eq!(lod.stats().worked_out, 0);
    }

    /// **Records alike share a group, and only they**: a ground cover's placements of one model
    /// come together, each record alike the one before it; one between them whose levels turn
    /// otherwise, or that has other levels, is a group of its own.
    #[test]
    fn records_alike_share_a_group_and_only_they() {
        const BLOCK: BlockId = 0xA9B4;
        let record = |levels: Vec<u32>, modes: Vec<u8>| Degrade {
            levels,
            modes,
            scale_z: 1.0,
            ..Degrade::default()
        };
        let block = crate::scene::Block {
            block: BLOCK,
            generation: 1,
            terrain: crate::scene::Terrain {
                side: 1,
                vertices: vec![Vec3::ZERO; 4],
                cuts: vec![false],
                words: [4; 81],
            },
            placements: Vec::new(),
            degrades: vec![
                record(vec![1, 2, 0], vec![1; 3]),
                record(vec![1, 2, 0], vec![1; 3]),
                record(vec![1, 2, 0], vec![1, 2, 1]),
                record(vec![1, 2, 0], vec![1; 3]),
                record(vec![1, 3, 0], vec![1; 3]),
            ],
            rooms: Vec::new(),
            lights: Vec::new(),
            lamps: Vec::new(),
        };
        let mut lod = Lod::default();
        lod.take(&[SceneDelta::BlockIn(Box::new(block))], &[]);
        let g: Vec<u32> = (0..5)
            .map(|r| lod.group_of_record(BLOCK, r).expect("a group"))
            .collect();
        assert_eq!((g[1], g[3]), (g[0], g[0]), "alike");
        assert_ne!(g[2], g[0], "turning otherwise");
        assert_ne!(g[4], g[0], "other levels");
        assert_ne!(g[4], g[2]);
    }

    /// A pack's levels hand over where the pack says, at the farthest retail's bias slides its
    /// own; never while degrades are off; and from the first when a level is forced.
    #[test]
    fn a_packs_levels_hand_over_where_it_says_unless_degrades_are_off_or_forced() {
        let d = [40.0, 90.0];
        let even = DegradeBias::default();
        assert_eq!(pack_switches(&d, &even), vec![40.0, 90.0]);
        assert_eq!(
            pack_switches(&d, &DegradeBias { bias: -1.0, ..even }),
            vec![40.0, 90.0]
        );
        let off = DegradeBias {
            disabled: true,
            ..even
        };
        assert!(pack_switches(&d, &off).iter().all(|s| s.is_infinite()));
        let forced = DegradeBias { force: 1, ..even };
        assert_eq!(pack_switches(&d, &forced), vec![0.0, f32::INFINITY]);
    }

    /// A tree's impostor has wholly taken over, fade and all, before the reach of the blocks
    /// whose objects stand in the world; with no reach, or degrades off or forced, nothing caps it.
    #[test]
    fn a_trees_impostor_has_wholly_taken_over_before_the_reach() {
        let even = DegradeBias::default();
        let cap = impostor_cap(168.0, FADE_BAND, &even);
        assert!((cap * (1.0 + FADE_BAND) - 168.0).abs() < 1e-3, "{cap}");
        assert!(impostor_cap(0.0, FADE_BAND, &even).is_infinite());
        let off = DegradeBias {
            disabled: true,
            ..even
        };
        assert!(impostor_cap(168.0, FADE_BAND, &off).is_infinite());
        let forced = DegradeBias { force: 0, ..even };
        assert!(impostor_cap(168.0, FADE_BAND, &forced).is_infinite());
        // The last hand-over past the cap is drawn in to it, and the level before it, left no room
        // to fade out first, is passed over; those nearer stay.
        let mut h = [100.0, 200.0, 300.0];
        pull_in(&mut h, 150.0, FADE_BAND);
        assert_eq!(h, [100.0, 150.0, 150.0]);
        let mut near = [10.0, 20.0];
        pull_in(&mut near, 150.0, FADE_BAND);
        assert_eq!(near, [10.0, 20.0]);
        let mut never = [40.0, f32::INFINITY];
        pull_in(&mut never, 150.0, FADE_BAND);
        assert_eq!(never, [40.0, 150.0]);
    }

    /// GfxObj 1: a tree of one level, a ten-meter triangle of leaves.
    struct Leaves;

    impl crate::assets::AssetSource for Leaves {
        fn gfxobj(&mut self, _id: u32) -> Option<Mesh> {
            let corner = |x: f32, y: f32| Vertex {
                position: [x, y, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [x, y],
            };
            Some(Mesh::from_triangle_lists([(
                crate::assets::SubsetInfo {
                    surface: crate::assets::SurfaceRef::Surface(7),
                    two_sided: true,
                    tiled: false,
                },
                vec![corner(0.0, 0.0), corner(10.0, 0.0), corner(0.0, 10.0)],
            )]))
        }

        fn cell_piece(&mut self, key: crate::assets::CellPieceKey) -> Option<Mesh> {
            Levels.cell_piece(key)
        }

        fn appearance(
            &mut self,
            surface: u32,
            look: Option<&crate::scene::PartLook>,
        ) -> crate::assets::Appearance {
            Levels.appearance(surface, look)
        }

        fn material(
            &mut self,
            key: &crate::assets::MaterialKey,
            used: crate::assets::Use,
        ) -> crate::assets::Material {
            crate::assets::Material {
                kind: crate::assets::Kind::Foliage,
                ..Levels.material(key, used)
            }
        }

        fn texture(
            &mut self,
            key: &crate::assets::TextureKey,
            clip_map: bool,
        ) -> Option<crate::assets::Texture> {
            Levels.texture(key, clip_map)
        }

        fn terrain_cells(
            &mut self,
            block: BlockId,
            t: &crate::scene::Terrain,
        ) -> Vec<crate::assets::TerrainCell> {
            Levels.terrain_cells(block, t)
        }

        fn sky_objects(&mut self) -> Vec<u32> {
            Vec::new()
        }

        fn sky(&mut self, _id: u32) -> Option<Mesh> {
            None
        }
    }

    /// The level a tree of one level, its record alone in a block, draws `d` meters from the
    /// eye, with the blocks whose objects stand in the world reaching `reach` meters.
    fn tree_at(d: f32, reach: f32) -> PartDraw {
        const BLOCK: BlockId = 0xA9B4;
        let block = crate::scene::Block {
            block: BLOCK,
            generation: 1,
            terrain: crate::scene::Terrain {
                side: 1,
                vertices: vec![Vec3::ZERO; 4],
                cuts: vec![false],
                words: [4; 81],
            },
            placements: vec![crate::scene::Placement {
                id: 0x0100_0001,
                kind: crate::scene::PlacementKind::Object,
                frame: Mat4::from_translation(Vec3::new(d, 0.0, 0.0)),
                scale: 1.0,
                lifts: [0.0; 2],
                parts: vec![crate::scene::Part {
                    gfxobj: 1,
                    frame: Mat4::from_translation(Vec3::new(d, 0.0, 0.0)),
                    scale: Vec3::ONE,
                    degrade: Some(0),
                }],
            }],
            degrades: vec![Degrade {
                levels: vec![1],
                modes: vec![1],
                scale_z: 1.0,
                centre: Vec3::new(d, 0.0, 0.0),
                ..Degrade::default()
            }],
            rooms: Vec::new(),
            lights: Vec::new(),
            lamps: Vec::new(),
        };
        let came = SceneDelta::BlockIn(Box::new(block));
        let mut tables = AssetTables::new();
        tables.apply(&came, &mut Leaves);
        let mut lod = Lod::default();
        lod.set_settings(LodSettings::ON);
        lod.take(std::slice::from_ref(&came), &[]);
        let mut frame = FrameDelta::default();
        frame.camera.fov_y = 1.0;
        frame.origin = [0xA9, 0xB4];
        frame.scenery_reach = reach;
        lod.prepare(&frame, &tables, 2160, &|_| None);
        let part = PartItems {
            degrade: Some(0),
            ..PartItems::default()
        };
        lod.part(BLOCK, &part, 0, true)
    }

    /// A tree an art pack draws with one level fades into its impostor by distance, as retail's
    /// trees do past their last level; and has wholly become it before the reach of the blocks
    /// whose objects stand in the world, however large it stands on the screen.
    #[test]
    fn a_pack_tree_of_one_level_hands_over_to_its_impostor_before_the_reach() {
        // Near, the mesh alone.
        let near = tree_at(20.0, 0.0);
        assert_eq!((near.level, near.impostor), (0, 0.0), "{near:?}");
        // Far, by its error on a tall screen, the impostor alone.
        let far = tree_at(2000.0, 0.0);
        assert_eq!((far.level, far.impostor), (1, 1.0), "{far:?}");
        // At 170 m the error alone keeps the mesh, but the blocks reach only 168 m.
        let open = tree_at(170.0, 0.0);
        assert_eq!(open.impostor, 0.0, "{open:?}");
        let capped = tree_at(170.0, 168.0);
        assert_eq!((capped.level, capped.impostor), (1, 1.0), "{capped:?}");
        // On its way, the two drawn together.
        let fading = tree_at(155.0, 168.0);
        assert!(
            fading.impostor > 0.0 && fading.impostor < 1.0 && fading.fade.is_some(),
            "{fading:?}"
        );
    }

    /// Whether no two of `h`'s fades overlap: each hand-over either falls together with the next,
    /// passing over the level between, or ends its fade before the next begins.
    fn fades_apart(h: &[f32]) -> bool {
        h.windows(2)
            .all(|w| w[1] == w[0] || w[1] >= w[0] * (1.0 + FADE_BAND) * 0.999)
    }

    /// A large tree of many levels, whose levels stray by more than the bar until well past the
    /// blocks round the viewer, keeps its full mesh out to where it must become its impostor and
    /// fades once, straight into it; a smaller one keeps each level that comes early enough to
    /// fade out first where it comes. With the blocks reaching farther, the large tree keeps the
    /// levels that fit and fades through them, one at a time. No two fades ever overlap.
    #[test]
    fn a_tree_fades_once_into_its_impostor_where_its_levels_do_not_fit() {
        let near = DegradeBias::default();
        let cap = impostor_cap(168.0, FADE_BAND, &near);
        // A pine 23 m across, its five levels and its impostor, on a screen 2160 lines tall.
        let pine = [307.0, 354.0, 408.0, 470.0, 722.0, 898.0];
        let mut big = pine;
        pull_in(&mut big, cap, FADE_BAND);
        assert_eq!(pick(&big, 60.0, FADE_BAND), Pick::only(0), "{big:?}");
        assert_eq!(pick(&big, 140.0, FADE_BAND), Pick::only(0), "{big:?}");
        let fading = pick(&big, 150.0, FADE_BAND);
        assert_eq!((fading.level, fading.next), (0, 6), "{fading:?}");
        assert_eq!(pick(&big, 168.0, FADE_BAND), Pick::only(6));
        assert!(fades_apart(&big), "{big:?}");
        // A smaller tree's first levels come before the cap, and are kept there.
        let mut small = [42.0, 70.0, 93.0, 373.0];
        pull_in(&mut small, cap, FADE_BAND);
        assert_eq!(small, [42.0, 70.0, 93.0, cap]);
        // Three rings of blocks round the viewer's: the pine keeps its first three levels where
        // they come, and fades from its fourth into its impostor.
        let far = impostor_cap(3.0 * 192.0 - 24.0, FADE_BAND, &near);
        let mut wide = pine;
        pull_in(&mut wide, far, FADE_BAND);
        assert_eq!(wide, [307.0, 354.0, 408.0, far, far, far]);
        assert!(fades_apart(&wide), "{wide:?}");
        // As the bar moves a hand-over toward the cap, it is kept while its fade ends first, and
        // passed over once it would not.
        for raw in [100.0, 120.0, 127.0, 128.0, 140.0, 160.0, 400.0] {
            let mut h = [raw, 500.0];
            pull_in(&mut h, cap, FADE_BAND);
            assert!(fades_apart(&h), "{h:?}");
            assert_eq!(h[0] == raw, raw * (1.0 + FADE_BAND) <= cap, "{h:?}");
        }
    }

    /// Records the device chooses levels for are never walked here: a frame chooses only the
    /// block's others, and the device's group is still measured once for it.
    #[test]
    fn records_the_device_chooses_are_not_walked_and_their_group_is_still_measured() {
        const BLOCK: BlockId = 0xA9B4;
        const PARTS: usize = 2000;
        let degrades: Vec<Degrade> = (0..PARTS)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)] // under two thousand
                let (x, y) = ((i % 45) as f32 * 4.0, (i / 45) as f32 * 4.0);
                Degrade {
                    levels: vec![1, 2, 0],
                    modes: vec![1; 3],
                    scale_z: 1.0,
                    centre: Vec3::new(x, y, 0.0),
                    ..Degrade::default()
                }
            })
            .collect();
        let block = crate::scene::Block {
            block: BLOCK,
            generation: 1,
            terrain: crate::scene::Terrain {
                side: 1,
                vertices: vec![Vec3::ZERO; 4],
                cuts: vec![false],
                words: [4; 81],
            },
            placements: vec![crate::scene::Placement {
                id: 0x0100_0001,
                kind: crate::scene::PlacementKind::Object,
                frame: Mat4::IDENTITY,
                scale: 1.0,
                lifts: [0.0; 2],
                parts: vec![crate::scene::Part {
                    gfxobj: 1,
                    frame: Mat4::IDENTITY,
                    scale: Vec3::ONE,
                    degrade: Some(0),
                }],
            }],
            degrades,
            rooms: Vec::new(),
            lights: Vec::new(),
            lamps: Vec::new(),
        };
        let came = SceneDelta::BlockIn(Box::new(block));
        let mut tables = AssetTables::new();
        tables.apply(&came, &mut Levels);
        let mut lod = Lod::default();
        lod.set_settings(LodSettings::ON);
        lod.take(std::slice::from_ref(&came), &[]);
        // The device chooses every record but two.
        lod.set_cpu_records(BLOCK, Some(vec![0, 25]));
        let group = lod.group_of_record(BLOCK, 7).expect("a group");
        let mut frame = FrameDelta::default();
        frame.camera.eye = Vec3::new(0.0, 0.0, 2.0);
        frame.camera.fov_y = 1.0;
        frame.origin = [0xA9, 0xB4];
        let distance_of = |_mesh: u32| Some(60.0);
        lod.prepare(&frame, &tables, 2160, &distance_of);
        assert_eq!(lod.stats().measured, 1);
        let picks = &lod.picks[&BLOCK];
        assert_eq!(picks[0], Some(Pick::only(0)));
        assert_eq!(picks[25], Some(Pick::only(1)));
        assert!(picks.iter().filter(|p| p.is_some()).count() == 2);
        // With none of them chosen here, the device's group is measured as the budget allows.
        let mut lod = Lod::default();
        lod.set_settings(LodSettings::ON);
        lod.take(std::slice::from_ref(&came), &[]);
        lod.set_cpu_records(BLOCK, Some(Vec::new()));
        lod.want(group);
        lod.prepare(&frame, &tables, 2160, &distance_of);
        assert_eq!(lod.stats().measured, 1);
        assert!(lod.group_info(group).and_then(|g| g.2).is_some());
        assert!(lod.picks[&BLOCK].iter().all(Option::is_none));
    }

    /// **With levels off, the blocks retail stands objects on draw retail's levels, and those
    /// past them still have their levels chosen**: the viewer's block and a block in the ring round
    /// it are left to retail's levels, while a block two rings out has every part chosen for as
    /// levels on choose them, its trees standing as impostors where they would.
    #[test]
    fn with_levels_off_only_the_blocks_past_retails_ring_have_levels_chosen() {
        const ORIGIN: [i32; 2] = [0xA9, 0xB4];
        let id = |dx: i32| -> BlockId { u16::try_from((ORIGIN[0] + dx) << 8 | ORIGIN[1]).unwrap() };
        let block = |b: BlockId| {
            let degrades = (0..20u8)
                .map(|i| Degrade {
                    levels: vec![1, 2, 0],
                    modes: vec![1; 3],
                    scale_z: 1.0,
                    centre: Vec3::new(f32::from(i) * 9.0, 96.0, 0.0),
                    ..Degrade::default()
                })
                .collect();
            SceneDelta::BlockIn(Box::new(crate::scene::Block {
                block: b,
                generation: 1,
                terrain: crate::scene::Terrain {
                    side: 1,
                    vertices: vec![Vec3::ZERO; 4],
                    cuts: vec![false],
                    words: [4; 81],
                },
                placements: vec![crate::scene::Placement {
                    id: 0x0100_0001,
                    kind: crate::scene::PlacementKind::Object,
                    frame: Mat4::IDENTITY,
                    scale: 1.0,
                    lifts: [0.0; 2],
                    parts: vec![crate::scene::Part {
                        gfxobj: 1,
                        frame: Mat4::IDENTITY,
                        scale: Vec3::ONE,
                        degrade: Some(0),
                    }],
                }],
                degrades,
                rooms: Vec::new(),
                lights: Vec::new(),
                lamps: Vec::new(),
            }))
        };
        let came: Vec<SceneDelta> = [0, 1, 2].map(|dx| block(id(dx))).into();
        let mut tables = AssetTables::new();
        for c in &came {
            tables.apply(c, &mut Levels);
        }
        let mut frame = FrameDelta::default();
        frame.camera.eye = Vec3::new(96.0, 96.0, 2.0);
        frame.camera.fov_y = 1.0;
        frame.origin = ORIGIN;
        let chosen_with = |settings: LodSettings| {
            let mut lod = Lod::default();
            lod.set_settings(settings);
            lod.take(&came, &[]);
            let distance_of = |_mesh: u32| Some(60.0);
            lod.prepare(&frame, &tables, 2160, &distance_of);
            lod
        };
        let off = chosen_with(LodSettings::OFF);
        let on = chosen_with(LodSettings::ON);
        for dx in [0, 1] {
            assert!(!off.chooses(id(dx)), "{dx} blocks out: retail's levels");
            assert!(on.chooses(id(dx)));
        }
        assert!(off.chooses(id(2)), "two blocks out: chosen here");
        let far = &off.picks[&id(2)];
        assert!(far.iter().all(Option::is_some), "every part chosen for");
        assert_eq!(far, &on.picks[&id(2)], "as levels on choose them");
        assert_eq!(off.impostor_levels[&id(2)], on.impostor_levels[&id(2)]);
        // Nothing past retail's ring: nothing chosen at all.
        let mut near = Lod::default();
        near.take(&came[..2], &[]);
        near.prepare(&frame, &tables, 2160, &|_mesh: u32| Some(60.0));
        assert!(near.picks.is_empty());
        assert_eq!(near.stats().measured, 0);
    }
}
