//! AC2's ledges made climbable for AC1's bodies in the converted world ([`crate::ac2_world::ledges`]):
//! the land raised where a ledge drops to the terrain, and a collision-only ramp where it drops to
//! another static (or to terrain that could not be raised).
//!
//! **How a ramp is written:** each placement's ramps are one AC1 model of physics polygons alone
//! (nothing drawn) and a setup of that one part, placed as a landblock static in the block the
//! placement stands in, at its origin and unturned. The client and Empyrean read it from the
//! overlay as they read every static's collision.
//!
//! **A building's own steps:** the land round a building's shell is held, never raised, so a
//! shell's ledges are not climbed by raised land; but a shell whose rooms are AC2's own
//! ([`crate::ac2_world::rooms`]) stands on a plinth whose steps are AC2's, and where one stands more than an
//! AC1 step over the ground or the step before it, it is ramped as any other ledge is
//! ([`climbable`]). A shell fitted with AC1's rooms collides with its drawn
//! polygons, not the collision these ledges are found in, and is left as it is.
//!
//! **No way cut:** each ramp is walked along its foot in AC1's physics over the world as the
//! client reads it ([`crate::ac2_world::passage`]), each way and a little to either side, once without the
//! ramps and once with them; a ramp that cuts any of those walks ([`ledges::cuts_passage`]) is
//! left out, and the rest walked again until none does. A walk goes as far as the land and the
//! statics round it let it, so after the first round only the walks that pass near a placement
//! whose ramps the round before changed, or near land it put back, are walked again; the others
//! went as they will go again.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::ac2_world::dense::DenseWorld;
use crate::ac2_world::ground::Ac2Ground;
use crate::ac2_world::ledges::{self, Floor, Land, Poly, Scene};
use dereth_assets::world::ObjectEntry;
use dereth_assets::{CellLandblock, DenseLand};
use dereth_dat::overlay::{container_name, OverlayWriter};
use dereth_dat::{DatFile, ModernDat};
use dereth_primitives::{DataId, Frame, Quat, Vec3};

use crate::ac2_world::art::{Ids, Records};
use crate::ac2_world::statics::{self, Collision, Solid, Statics};

/// How far round a building's shell the land is never raised, meters.
const HELD_MARGIN: f64 = 2.0;

/// What the walks along the ramps' feet are walked over: AC1's install, its two files, the
/// converted region record, the folder the world is written into for them, and the world's key.
pub struct Check<'a> {
    pub ac1: &'a Path,
    pub cell: &'a DatFile,
    pub portal: &'a DatFile,
    pub region: (u32, Vec<u8>),
    pub dir: PathBuf,
    pub world_key: &'a str,
    pub steps: usize,
}

/// What making the ledges climbable did.
pub struct Climb {
    pub line: String,
    pub tsv: String,
    /// A line per ramp: its placement's AC2 object and block, its ridge's ends, which way it
    /// faces, and its foot's height.
    pub ramps_tsv: String,
}

/// Whether the ledges of solid `i` among `solids` are made climbable: every static's but a
/// building shell's, and those of the shells in `steps_of` (the shells whose rooms are AC2's, which
/// stand on AC2's steps).
pub fn climbable<'a>(
    solids: &'a [Solid],
    steps_of: &'a BTreeSet<(usize, usize, usize)>,
) -> impl Fn(usize) -> bool + Sync + 'a {
    move |i| {
        solids
            .get(i)
            .is_some_and(|s| !s.shell || steps_of.contains(&s.ac2))
    }
}

/// Raise the land at, and ramp, the band ledges of `st`'s placements, and the steps of the shells
/// `steps_of` names ([`climbable`]); the ramps are added to the blocks' statics.
pub fn make(
    g: &Ac2Ground,
    w: &mut DenseWorld,
    st: &mut Statics,
    steps_of: &BTreeSet<(usize, usize, usize)>,
    ids: &mut Ids,
    out: &mut Records,
    check: &Check,
) -> Result<Climb, String> {
    let mut polys = Vec::new();
    for (i, s) in st.solids.iter().enumerate() {
        let Some(c) = st.collisions.get(&s.object) else {
            continue;
        };
        polys.extend(
            s.polygons(c)
                .into_iter()
                .filter_map(|pts| Poly::new(pts, i)),
        );
    }
    let scene = Scene::new(polys);
    let shell: Vec<bool> = st.solids.iter().map(|s| s.shell).collect();
    let solids = st.solids.clone();
    let fixable = climbable(&solids, steps_of);
    // The land round every building's shell is held: its doorways and rooms stand on it.
    let side = w.grid.side();
    let spacing = w.grid.spacing();
    let mut held: HashSet<(usize, usize)> = HashSet::new();
    for p in scene.polys.iter().filter(|p| shell[p.owner]) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let sq = |v: f64| ((v / spacing).floor().max(0.0) as usize).min(side - 2);
        for i in sq(p.lo[0] - HELD_MARGIN)..=sq(p.hi[0] + HELD_MARGIN) {
            for j in sq(p.lo[1] - HELD_MARGIN)..=sq(p.hi[1] + HELD_MARGIN) {
                held.insert((i, j));
            }
        }
    }
    let (found, sw) = {
        let water = |k: usize| w.vertex[k].water;
        let land = Land {
            spacing,
            side,
            table: &w.table,
            height: &w.height,
            cut: &w.cut,
            water: &water,
        };
        let found = ledges::band_ledges(&scene, &land, &fixable);
        let sw = ledges::swell(&found, &scene, &land, &|i, j| held.contains(&(i, j)));
        (found, sw)
    };
    // The world before any ledge is touched, which the walks along the ramps' feet are measured
    // against.
    let before_dir = check.dir.join("before");
    let after_dir = check.dir.join("after");
    write_check(
        check,
        &before_dir,
        w,
        &st.by_block,
        out,
        &Records::default(),
        None,
    )?;
    let pre = w.height.clone();
    let mut raised = w.raise_to(g, &sw.wants);
    let (left, mut rp) = {
        let water = |k: usize| w.vertex[k].water;
        let land = Land {
            spacing,
            side,
            table: &w.table,
            height: &w.height,
            cut: &w.cut,
            water: &water,
        };
        let left = ledges::band_ledges(&scene, &land, &fixable);
        let rp = ledges::ramps(&left, &scene, &land);
        (left, rp)
    };
    // Every ramp's walks, and how far each got before. A walk cut in the world with the ledges
    // made climbable leaves its ramp out; one still cut with its ramp out puts back the land
    // raised along it; one cut still leaves out the ramps beside it.
    let walks: Vec<crate::ac2_world::passage::Walk> = {
        let water = |k: usize| w.vertex[k].water;
        let land = Land {
            spacing,
            side,
            table: &w.table,
            height: &w.height,
            cut: &w.cut,
            water: &water,
        };
        rp.made
            .iter()
            .flat_map(|r| foot_walks(r, st, &scene, &land))
            .collect()
    };
    let before = crate::ac2_world::passage::walks(check.ac1, &before_dir, check.world_key, &walks)?;
    let mut alive = vec![true; rp.made.len()];
    let mut stage = vec![0u8; walks.len()];
    let (mut cut, mut beside, mut lowered, mut rounds) = (0usize, 0usize, 0usize, 0usize);
    // How far each walk got with the ramps, as of the round it was last walked; the walks this
    // round takes: every one first, then those near what the round before changed.
    let mut with = vec![0f32; walks.len()];
    let mut todo: Vec<usize> = (0..walks.len()).collect();
    // Where each placement's ramps are.
    let mut owner_area: BTreeMap<usize, Area> = BTreeMap::new();
    for r in &rp.made {
        let a = owner_area
            .entry(r.owner)
            .or_insert([f64::MAX, f64::MAX, f64::MIN, f64::MIN]);
        for p in r.faces.iter().flatten() {
            *a = [
                a[0].min(p[0]),
                a[1].min(p[1]),
                a[2].max(p[0]),
                a[3].max(p[1]),
            ];
        }
    }
    loop {
        rounds += 1;
        if !todo.is_empty() {
            let kept: Vec<ledges::Ramp> = rp
                .made
                .iter()
                .zip(&alive)
                .filter(|(_, &a)| a)
                .map(|(r, _)| r.clone())
                .collect();
            let mut trial_ids = ids.clone();
            let mut trial = Records::default();
            let mut by_block = st.by_block.clone();
            place(&kept, st, &mut by_block, &mut trial_ids, &mut trial)?;
            let walked: BTreeSet<(usize, usize)> = todo.iter().map(|&i| walks[i].block).collect();
            write_check(check, &after_dir, w, &by_block, out, &trial, Some(&walked))?;
            let list: Vec<crate::ac2_world::passage::Walk> =
                todo.iter().map(|&i| walks[i]).collect();
            let got =
                crate::ac2_world::passage::walks(check.ac1, &after_dir, check.world_key, &list)?;
            for (&i, g) in todo.iter().zip(got) {
                with[i] = g;
            }
        }
        let bad: Vec<usize> = (0..walks.len())
            .filter(|&i| ledges::cuts_passage(&[(before[i], with[i])]))
            .collect();
        eprintln!(
            "walks along the ramps' feet, round {rounds}: {} of {} cut ({} walked)",
            bad.len(),
            walks.len(),
            todo.len()
        );
        if bad.is_empty() || rounds == 8 {
            break;
        }
        let was = alive.clone();
        let mut back = std::collections::BTreeSet::new();
        for &i in &bad {
            let r = i / FOOT_WALKS;
            match stage[i] {
                0 if alive[r] => {
                    alive[r] = false;
                    cut += 1;
                }
                0 | 1 => {
                    back.extend(squares_along(&walks[i], spacing, side));
                    stage[i] = 2;
                    continue;
                }
                _ => {
                    for (k, other) in rp.made.iter().enumerate() {
                        if alive[k] && near_walk(other, &walks[i]) {
                            alive[k] = false;
                            beside += 1;
                        }
                    }
                }
            }
            stage[i] = stage[i].max(1);
        }
        lowered += w.restore(g, &pre, &back);
        // What this round changed: each placement a ramp was left out of (its ramps are one
        // static), and the land put back (a square's corners, and the cuts of the squares round
        // them).
        let mut changed: Vec<Area> = (0..alive.len())
            .filter(|&r| alive[r] != was[r])
            .map(|r| rp.made[r].owner)
            .collect::<BTreeSet<usize>>()
            .into_iter()
            .map(|o| owner_area[&o])
            .collect();
        #[allow(clippy::cast_precision_loss)]
        changed.extend(back.iter().map(|&(i, j)| {
            [
                (i as f64 - 1.0) * spacing,
                (j as f64 - 1.0) * spacing,
                (i as f64 + 2.0) * spacing,
                (j as f64 + 2.0) * spacing,
            ]
        }));
        let changed = Areas::new(changed);
        todo = (0..walks.len())
            .filter(|&i| changed.near(&walks[i]))
            .collect();
    }
    raised -= lowered.min(raised);
    let walked: Vec<(crate::ac2_world::passage::Walk, (f32, f32))> = (0..rp.made.len())
        .filter(|&k| alive[k])
        .map(|k| {
            (
                walks[k * FOOT_WALKS],
                (before[k * FOOT_WALKS], with[k * FOOT_WALKS]),
            )
        })
        .collect();
    let mut k = 0;
    rp.made.retain(|_| {
        k += 1;
        alive[k - 1]
    });
    let _ = std::fs::remove_dir_all(&check.dir);
    let water = |k: usize| w.vertex[k].water;
    let land = Land {
        spacing,
        side,
        table: &w.table,
        height: &w.height,
        cut: &w.cut,
        water: &water,
    };
    let mut by_owner: BTreeMap<usize, Vec<&ledges::Ramp>> = BTreeMap::new();
    for r in &rp.made {
        by_owner.entry(r.owner).or_default().push(r);
    }
    let mut blocks = std::mem::take(&mut st.by_block);
    place(&rp.made, st, &mut blocks, ids, out)?;
    st.by_block = blocks;
    // The ledges left with the ramps in.
    let mut with = scene.polys;
    let ramp_owner = st.solids.len();
    for r in &rp.made {
        with.extend(
            r.faces
                .iter()
                .filter_map(|f| Poly::new(f.clone(), ramp_owner)),
        );
    }
    let with = Scene::new(with);
    let after = ledges::band_ledges(&with, &land, &fixable);
    // By placement: its ledges, those onto the terrain, those the land serves, its ramps.
    let mut per: BTreeMap<usize, [usize; 6]> = BTreeMap::new();
    for l in &found {
        let e = per.entry(l.owner).or_default();
        e[0] += 1;
        e[1] += usize::from(matches!(l.floor, Floor::Terrain(_)));
    }
    for &li in &sw.served {
        per.entry(found[li].owner).or_default()[2] += 1;
    }
    for l in &left {
        per.entry(l.owner).or_default()[3] += 1;
    }
    for r in &rp.made {
        per.entry(r.owner).or_default()[4] += 1;
    }
    for l in &after {
        per.entry(l.owner).or_default()[5] += 1;
    }
    let mut tsv = String::from(
        "ac2_object\tac1_block\tworld_x\tworld_y\tz\tledge_samples\tonto_terrain\tserved_by_land\tleft_after_land\tramps\tleft_after_ramps\n",
    );
    for (&o, n) in &per {
        let s = &st.solids[o];
        let _ = writeln!(
            tsv,
            "{:08X}\t{:02X}{:02X}\t{:.1}\t{:.1}\t{:.2}\t{}\t{}\t{}\t{}\t{}\t{}",
            s.object,
            s.block.0,
            s.block.1,
            s.world.0,
            s.world.1,
            s.world.2,
            n[0],
            n[1],
            n[2],
            n[3],
            n[4],
            n[5]
        );
    }
    let statics_with = per.values().filter(|n| n[0] > 0).count();
    let swelled = per.values().filter(|n| n[2] > 0).count();
    let onto_terrain = found
        .iter()
        .filter(|l| matches!(l.floor, Floor::Terrain(_)))
        .count();
    let line = format!(
        "AC2's ledges AC1 does not climb ({}-{} m over the floor outside, ledges.tsv): {} edge samples on {statics_with} placements, {onto_terrain} onto the terrain; land raised at {} of them ({} refused: {:?}) under {swelled} placements, {raised} vertices; {} samples left after, {} ramps on {} placements ({} runs not ramped: {} with something over them or beside them, {} over a floor too far down; {cut} left out as they cut a walk along their foot and {beside} beside a walk still cut, the land put back under {lowered} vertices, in {rounds} rounds); {} samples on {} placements left with the ramps in",
        ledges::RISE_LO,
        ledges::RISE_HI,
        found.len(),
        sw.served.len(),
        sw.refused.len(),
        sw.why,
        left.len(),
        rp.made.len(),
        by_owner.len(),
        rp.blocked + rp.too_deep,
        rp.blocked,
        rp.too_deep,
        after.len(),
        per.values().filter(|n| n[5] > 0).count(),
    );
    let mut ramps_tsv = String::from(
        "ac2_object\tac1_block\tridge_ax\tridge_ay\tridge_bx\tridge_by\ttop\tout_x\tout_y\tbase\tfoot_walk_start_z\tfoot_walk_without_m\tfoot_walk_with_m\tfoot_walk_start_x\tfoot_walk_start_y\tfoot_walk_along_x\tfoot_walk_along_y\tfoot_walk_m\n",
    );
    for (r, (walk, got)) in rp.made.iter().zip(&walked) {
        let s = &st.solids[r.owner];
        let [a, b] = r.ridge;
        let _ = writeln!(
            ramps_tsv,
            "{:08X}\t{:02X}{:02X}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.6}\t{:.6}\t{:.4}\t{:.4}\t{:.2}\t{:.2}\t{:.6}\t{:.6}\t{:.8}\t{:.8}\t{:.6}",
            s.object,
            s.block.0,
            s.block.1,
            a[0],
            a[1],
            b[0],
            b[1],
            a[2],
            r.out[0],
            r.out[1],
            r.base,
            walk.start[2],
            got.0,
            got.1,
            walk.start[0],
            walk.start[1],
            walk.along[0],
            walk.along[1],
            walk.dist
        );
    }
    Ok(Climb {
        line,
        tsv,
        ramps_tsv,
    })
}

/// Each placement's ramps as one collision-only static in `blocks`, its records in `out`; the
/// ramps' setups.
fn place(
    ramps: &[ledges::Ramp],
    st: &Statics,
    blocks: &mut BTreeMap<(usize, usize), Vec<ObjectEntry>>,
    ids: &mut Ids,
    out: &mut Records,
) -> Result<HashSet<u32>, String> {
    let mut by_owner: BTreeMap<usize, Vec<&ledges::Ramp>> = BTreeMap::new();
    for r in ramps {
        by_owner.entry(r.owner).or_default().push(r);
    }
    let mut made = HashSet::new();
    for (&owner, rs) in &by_owner {
        let s = &st.solids[owner];
        let mut c = Collision::default();
        for r in rs {
            for f in &r.faces {
                let base = c.points.len();
                #[allow(clippy::cast_possible_truncation)]
                c.points.extend(f.iter().map(|v| {
                    Vec3::new(
                        (v[0] - s.world.0) as f32,
                        (v[1] - s.world.1) as f32,
                        (v[2] - s.world.2) as f32,
                    )
                }));
                c.rings.push(
                    (base..c.points.len())
                        .map(|k| u16::try_from(k).unwrap_or(u16::MAX))
                        .collect(),
                );
            }
        }
        let id = statics::collision_setup(&c, ids, out)?;
        made.insert(id.raw());
        blocks.entry(s.block).or_default().push(ObjectEntry {
            id,
            frame: Frame::new(s.frame.origin, Quat::IDENTITY),
        });
    }
    Ok(made)
}

/// How many walks are taken along each ramp's foot ([`foot_walks`]).
const FOOT_WALKS: usize = 6;

/// The walks along a ramp's foot: half its run out from the ledge, from beyond one end to beyond
/// the other and back, each also 2 cm to either side (a walk brushing a ramp can go either way
/// for a centimeter), starting on the floor there. The first is the one `ramps.tsv` reports.
fn foot_walks(
    r: &ledges::Ramp,
    st: &Statics,
    scene: &Scene,
    land: &Land,
) -> Vec<crate::ac2_world::passage::Walk> {
    let [a, b] = r.ridge;
    let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2))
        .sqrt()
        .max(1e-3);
    let e = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
    let run = (a[2] - r.base) * ledges::RUN;
    let (u, back) = (run * 0.5, run + ledges::FOOT_WALK_LEAD);
    let block = st.solids[r.owner].block;
    let mut out = Vec::with_capacity(FOOT_WALKS);
    for (from, dir) in [(a, e), (b, [-e[0], -e[1]])] {
        for side in [0.0, -0.02, 0.02] {
            let (x, y) = (
                from[0] - dir[0] * back + r.out[0] * (u + side),
                from[1] - dir[1] * back + r.out[1] * (u + side),
            );
            let t = land.at(x, y).unwrap_or(r.base);
            let z = ledges::floor(scene, land, x, y, t + 1.7).map_or(t, Floor::z);
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            out.push(crate::ac2_world::passage::Walk {
                block,
                start: [
                    (x - block.0 as f64 * 192.0) as f32,
                    (y - block.1 as f64 * 192.0) as f32,
                    z as f32,
                ],
                along: [dir[0] as f32, dir[1] as f32],
                dist: (len + 2.0 * back) as f32,
                until: None,
            });
        }
    }
    out
}

/// How far round its block a walk reads the world, blocks: the land two blocks round, the
/// statics one ([`crate::ac2_world::passage`]).
const WALK_READS: usize = 2;

/// How near a walk a change has to come to be walked again, meters: an outdoor cell, the
/// least a body's physics looks round it for the land and the statics it may meet.
const WALK_NEAR: f64 = 24.0;

/// A box on the map, world meters: west, south, east, north.
type Area = [f64; 4];

/// The areas a round changed, by the blocks they reach.
struct Areas {
    areas: Vec<Area>,
    by_block: BTreeMap<(i64, i64), Vec<usize>>,
}

impl Areas {
    fn new(areas: Vec<Area>) -> Self {
        let mut by_block: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
        for (k, a) in areas.iter().enumerate() {
            let [x0, y0, x1, y1] = blocks_of(*a);
            for x in x0..=x1 {
                for y in y0..=y1 {
                    by_block.entry((x, y)).or_default().push(k);
                }
            }
        }
        Self { areas, by_block }
    }

    /// Whether any area comes within [`WALK_NEAR`] of `walk`'s line.
    fn near(&self, walk: &crate::ac2_world::passage::Walk) -> bool {
        #[allow(clippy::cast_precision_loss)]
        let (sx, sy) = (
            walk.block.0 as f64 * 192.0 + f64::from(walk.start[0]),
            walk.block.1 as f64 * 192.0 + f64::from(walk.start[1]),
        );
        let (ex, ey) = (
            sx + f64::from(walk.along[0] * walk.dist),
            sy + f64::from(walk.along[1] * walk.dist),
        );
        let reach = [
            sx.min(ex) - WALK_NEAR,
            sy.min(ey) - WALK_NEAR,
            sx.max(ex) + WALK_NEAR,
            sy.max(ey) + WALK_NEAR,
        ];
        let [x0, y0, x1, y1] = blocks_of(reach);
        (x0..=x1).any(|x| {
            (y0..=y1).any(|y| {
                self.by_block.get(&(x, y)).is_some_and(|ks| {
                    ks.iter().any(|&k| {
                        let a = &self.areas[k];
                        a[0] <= reach[2] && reach[0] <= a[2] && a[1] <= reach[3] && reach[1] <= a[3]
                    })
                })
            })
        })
    }
}

/// The blocks an area reaches: west, south, east, north.
fn blocks_of(a: Area) -> [i64; 4] {
    // LINT-OK: world meters, well inside an i64.
    #[allow(clippy::cast_possible_truncation)]
    let b = |v: f64| (v / 192.0).floor() as i64;
    [b(a[0]), b(a[1]), b(a[2]), b(a[3])]
}

/// Whether a walk in `block` reads any block of `changed` ([`WALK_READS`]).
fn near_changed(block: (usize, usize), changed: &BTreeSet<(usize, usize)>) -> bool {
    changed
        .iter()
        .any(|&(x, y)| x.abs_diff(block.0) <= WALK_READS && y.abs_diff(block.1) <= WALK_READS)
}

/// The world as far as the walks need it, written as the world's overlay into `dir`: the
/// land and the statics (the ramps on trial among them) of the blocks round every block in
/// `blocks`, the region, and the models. With `walked`, only the blocks the walks in those
/// blocks read ([`WALK_READS`]) are written; they read nothing else.
fn write_check(
    check: &Check,
    dir: &Path,
    w: &DenseWorld,
    blocks: &BTreeMap<(usize, usize), Vec<ObjectEntry>>,
    out: &Records,
    trial: &Records,
    walked: Option<&BTreeSet<(usize, usize)>>,
) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut near: std::collections::BTreeSet<(usize, usize)> = std::collections::BTreeSet::new();
    for &(bx, by) in blocks.keys() {
        for dx in 0..5usize {
            for dy in 0..5usize {
                let (x, y) = ((bx + dx).wrapping_sub(2), (by + dy).wrapping_sub(2));
                if x < 255 && y < 255 {
                    near.insert((x, y));
                }
            }
        }
    }
    if let Some(walked) = walked {
        near.retain(|&b| near_changed(b, walked));
    }
    let version = |f: &DatFile, id: DataId| {
        f.base_entry(id)
            .map_or(1, |e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1))
    };
    let cell_it = dereth_dat::decompose::revision_over(check.cell) + 1;
    let portal_it = dereth_dat::decompose::revision_over(check.portal) + 1;
    let err = |e: dereth_dat::DatError| format!("{e:?}");
    let mut cw = OverlayWriter::open_or_create(
        &dir.join(container_name(ModernDat::Cell)),
        check.cell,
        "client_cell_1.dat",
        check.world_key,
        0,
    )
    .map_err(|e| format!("{e:?}"))?;
    for &(bx, by) in &near {
        #[allow(clippy::cast_possible_truncation)]
        let id = DataId(((bx as u32) << 24) | ((by as u32) << 16) | 0xFFFF);
        let (words, hs, split) = w.block(bx, by);
        let rec = CellLandblock::from_dense(
            id,
            1,
            DenseLand {
                steps: check.steps,
                terrain: words.into(),
                height: hs.into(),
                split: split.into(),
            },
        );
        cw.put(id, &rec.encode(), version(check.cell, id), cell_it, 0)
            .map_err(err)?;
        let lid = DataId(id.raw() & 0xFFFF_0000 | 0xFFFE);
        let mut info = crate::ac2_world::cleared_empty(lid);
        info.objects
            .extend(blocks.get(&(bx, by)).into_iter().flatten().cloned());
        cw.put(lid, &info.encode(), version(check.cell, lid), cell_it, 0)
            .map_err(err)?;
    }
    cw.add_iteration(cell_it, 0).map_err(err)?;
    cw.flush(0).map_err(err)?;
    drop(cw);
    let mut pw = OverlayWriter::open_or_create(
        &dir.join(container_name(ModernDat::Portal)),
        check.portal,
        "client_portal.dat",
        check.world_key,
        0,
    )
    .map_err(|e| format!("{e:?}"))?;
    let rid = DataId(check.region.0);
    pw.put(
        rid,
        &check.region.1,
        version(check.portal, rid),
        portal_it,
        0,
    )
    .map_err(err)?;
    for (id, bytes) in out.0.iter().chain(trial.0.iter()) {
        if matches!(id.raw() >> 24, 0x01 | 0x02) {
            pw.put(*id, bytes, version(check.portal, *id), portal_it, 0)
                .map_err(err)?;
        }
    }
    pw.add_iteration(portal_it, 0).map_err(err)?;
    pw.flush(0).map_err(err)?;
    Ok(())
}

/// The land squares a walk crosses, and those within 2 m of its line.
fn squares_along(
    walk: &crate::ac2_world::passage::Walk,
    spacing: f64,
    side: usize,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let (bx, by) = (walk.block.0 as f64 * 192.0, walk.block.1 as f64 * 192.0);
    // LINT-OK: a walk a few meters long.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = walk.dist.ceil() as usize;
    for t in 0..=steps {
        for side_off in [-2.0f64, 0.0, 2.0] {
            let (x, y) = (
                bx + f64::from(walk.start[0]) + f64::from(walk.along[0]) * t as f64
                    - f64::from(walk.along[1]) * side_off,
                by + f64::from(walk.start[1])
                    + f64::from(walk.along[1]) * t as f64
                    + f64::from(walk.along[0]) * side_off,
            );
            if x < 0.0 || y < 0.0 {
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (i, j) = ((x / spacing) as usize, (y / spacing) as usize);
            if i + 1 < side && j + 1 < side {
                out.push((i, j));
            }
        }
    }
    out
}

/// Whether a ramp's ridge comes within 3 m of a walk's line.
fn near_walk(r: &ledges::Ramp, walk: &crate::ac2_world::passage::Walk) -> bool {
    let (bx, by) = (walk.block.0 as f64 * 192.0, walk.block.1 as f64 * 192.0);
    let (sx, sy) = (bx + f64::from(walk.start[0]), by + f64::from(walk.start[1]));
    let (ex, ey) = (
        sx + f64::from(walk.along[0] * walk.dist),
        sy + f64::from(walk.along[1] * walk.dist),
    );
    r.ridge.iter().any(|p| {
        let (dx, dy) = (ex - sx, ey - sy);
        let l2 = (dx * dx + dy * dy).max(1e-9);
        let t = (((p[0] - sx) * dx + (p[1] - sy) * dy) / l2).clamp(0.0, 1.0);
        ((sx + t * dx - p[0]).powi(2) + (sy + t * dy - p[1]).powi(2)).sqrt() < 3.0
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's climbable ledges)
    use super::*;
    use crate::ac2_world::table;

    const SIDE: usize = 9;

    fn solid(shell: bool, ac2: (usize, usize, usize)) -> Solid {
        Solid {
            object: 0,
            scale: Vec3::new(1.0, 1.0, 1.0),
            block: (0, 0),
            frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
            world: (0.0, 0.0, 0.0),
            shell,
            ac2,
        }
    }

    /// A plinth's step (a 4 m top `rise` over flat ground at 10 m) on solid `owner`, with the land
    /// round it held as a shell's is: the ramps made for it when `which` says its ledges are
    /// climbed.
    fn ramps_for(owner: usize, which: &(dyn Fn(usize) -> bool + Sync), rise: f64) -> usize {
        let mut t = [0f32; table::LEN];
        for (k, v) in t.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let x = k as f32 * 0.1;
            *v = x;
        }
        let height = vec![crate::ac2_world::buried::at_least(&t, 10.0); SIDE * SIDE];
        let cut = vec![true; (SIDE - 1) * (SIDE - 1)];
        let dry = |_: usize| false;
        let land = Land {
            spacing: 8.0,
            side: SIDE,
            table: &t,
            height: &height,
            cut: &cut,
            water: &dry,
        };
        let top = 10.0 + rise;
        let step = Poly::new(
            vec![
                [30.0, 30.0, top],
                [34.0, 30.0, top],
                [34.0, 34.0, top],
                [30.0, 34.0, top],
            ],
            owner,
        )
        .unwrap();
        let scene = Scene::new(vec![step]);
        let found = ledges::band_ledges(&scene, &land, which);
        ledges::ramps(&found, &scene, &land).made.len()
    }

    #[test]
    fn only_walks_passing_near_what_a_round_changed_are_walked_again() {
        // A walk 10 m east along block (2, 3)'s south edge, from 20 m in.
        let walk = crate::ac2_world::passage::Walk {
            block: (2, 3),
            start: [20.0, 1.0, 0.0],
            along: [1.0, 0.0],
            dist: 10.0,
            until: None,
        };
        let (x, y) = (2.0 * 192.0 + 20.0, 3.0 * 192.0 + 1.0);
        // A change beside its line, and one just within reach of its end in the block south.
        assert!(Areas::new(vec![[x + 4.0, y + 2.0, x + 5.0, y + 3.0]]).near(&walk));
        assert!(Areas::new(vec![[x + 30.0, y - 20.0, x + 33.0, y - 22.0 + 1.0]]).near(&walk));
        // A change farther off than a cell, and none.
        assert!(!Areas::new(vec![[x + 40.0, y, x + 41.0, y + 1.0]]).near(&walk));
        assert!(!Areas::new(vec![[x, y + 30.0, x + 5.0, y + 31.0]]).near(&walk));
        assert!(!Areas::new(Vec::new()).near(&walk));
    }

    #[test]
    fn a_vault_entrances_step_too_high_for_ac1_is_ramped_and_a_fitted_shells_is_not() {
        let way2 = (0x62, 0x58, 7);
        let solids = vec![
            solid(false, (0, 0, 0)),
            solid(true, way2),
            solid(true, (0x6E, 0x8F, 3)),
        ];
        let steps_of = BTreeSet::from([way2]);
        let which = climbable(&solids, &steps_of);
        // A step 0.8 m over the ground, more than an AC1 body steps: the static's and the vault
        // entrance's are ramped, the fitted shell's (whose collision is its drawn polygons) is not.
        assert!(ramps_for(0, &which, 0.8) > 0);
        assert!(
            ramps_for(1, &which, 0.8) > 0,
            "the way-2 shell's step is ramped"
        );
        assert_eq!(ramps_for(2, &which, 0.8), 0);
        // A step an AC1 body takes is left as it is.
        assert_eq!(ramps_for(1, &which, 0.3), 0);
    }
}
