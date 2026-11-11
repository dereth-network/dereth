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
//! the tree is drawn as far as its block is.

use std::collections::{BTreeMap, HashMap};

use glam::{Mat4, Vec3};

use super::scene::{class, BlockGpu, GpuScene, PartItems};
use crate::assets::{AssetTables, Mesh, MeshKey};
use crate::scene::{BlockId, BlockLevels, Degrade, DegradeBias, FrameDelta, SceneDelta};

/// How levels are chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodSettings {
    /// Levels chosen here; off, every part draws the level retail chose.
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
fn retail_switches_into(d: &Degrade, bias: &DegradeBias, out: &mut Vec<f32>) {
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
}

/// What [`Lod::prepare`] works each record's hand-overs out in, kept from frame to frame.
#[derive(Debug, Default)]
struct Scratch {
    errors: Vec<f32>,
    switches: Vec<f32>,
    hand_overs: Vec<f32>,
}

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
        }
        if d.turned.is_some() {
            self.turned += 1;
        }
    }
}

/// What the last frame's choice of levels did, over the parts chosen on the processor: an art
/// pack's models chosen on the device are counted by it ([`super::Renderer::pack_states`]).
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
    /// Records whose levels' errors were measured, in all.
    pub measured: u32,
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
                    let groups: Vec<u32> = b.degrades.iter().map(|r| self.group(r)).collect();
                    self.record_groups.insert(b.block, groups);
                    self.records.insert(b.block, b.degrades.clone());
                    self.turned.remove(&b.block);
                }
                SceneDelta::BlockOut(id) => {
                    self.records.remove(id);
                    self.record_groups.remove(id);
                    self.cpu_records.remove(id);
                    self.turned.remove(id);
                    self.picks.remove(id);
                    self.walked.remove(id);
                    self.impostor_levels.remove(id);
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
        if !self.settings.on {
            self.picks.clear();
            return;
        }
        self.frames += 1;
        let frame_no = self.frames;
        let k = pixels_at_one_metre(height, frame.camera.fov_y);
        let eye = frame.camera.eye;
        let mut budget = ERRORS_PER_FRAME;
        let (error_px, fade, impostors) = (
            self.settings.error_px,
            self.settings.fade,
            self.settings.impostors,
        );
        // Retail's bias follows its frame rate from frame to frame; its hand-overs are taken at
        // the farthest it slides them, so a still view stays still and no level comes sooner
        // than retail's would at any frame rate.
        let farthest = DegradeBias {
            bias: frame.degrade.bias.max(1.0),
            ..frame.degrade
        };
        let Self {
            records,
            record_groups,
            cpu_records,
            groups,
            scratch,
            picks: all_picks,
            impostor_levels: all_impostor_levels,
            stats,
            ..
        } = self;
        for (block, records) in records.iter() {
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
            let mut choose = |i: usize| -> (Option<Pick>, Option<u8>) {
                let Some(r) = records.get(i) else {
                    return (None, None);
                };
                let Some(group) = of.get(i).and_then(|g| groups.get_mut(*g as usize)) else {
                    return (None, None);
                };
                if group.errors.is_none() {
                    if budget == 0 {
                        return (None, None);
                    }
                    let Some(e) = record_errors(r, tables) else {
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
                    switches.extend_from_slice(&group.switches);
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
}
