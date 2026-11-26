//! The walk along a ramp's foot, in AC1's physics over the converted world as the client reads it
//! ([`crate::ac2_world::climb`]): AC1's human at 4 m/s, 30 frames a second, from beyond one end of the ramp
//! to beyond the other, half the ramp's run out from the ledge; in the world before the ledges
//! were touched and in the world with the land raised and the ramps in, so a ramp or a raise that
//! cuts a way along a ledge's foot is known.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use dereth_assets::world::LandblockInfo;
use dereth_assets::{Decode, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::{LandSource, PhysicsWorld, SetupGeometry};
use dereth_primitives::frame::V3 as _;
use dereth_primitives::{
    CellId, DataId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3,
};
use dereth_world_data::env_cells::{static_geometry, CellStatic, CellStaticObjects};
use dereth_world_data::land_source::DatLandSource;
use dereth_world_data::setup::SetupPartStats;

const BLOCK: f32 = 192.0;
/// AC1's human, the body walked.
const HUMAN: u32 = 0x0200_0001;
const SPEED: f32 = 4.0;
const HZ: f64 = 30.0;

/// One walk: in the AC1 block it starts in, from `start` (block meters, on the floor) along the
/// heading `along` (a unit vector) for `dist` meters, or until it is `until` meters along it.
/// `set_down`: the body is put into the world on the land under `start`, as the client puts a
/// character in, rather than let fall from `start` (a body come down on a slope holds it as a
/// wall to slide along, and walked straight up it does not move).
#[derive(Clone, Copy, Debug)]
pub struct Walk {
    pub block: (usize, usize),
    pub start: [f32; 3],
    pub along: [f32; 2],
    pub dist: f32,
    pub until: Option<f32>,
    pub set_down: bool,
}

/// How far each walk got along its heading in the world written as the overlay in `overlay`,
/// meters.
pub fn walks(
    ac1: &Path,
    overlay: &Path,
    world_key: &str,
    list: &[Walk],
) -> Result<Vec<f32>, String> {
    Ok(ends(ac1, overlay, world_key, list)?
        .iter()
        .zip(list)
        .map(|(&(_, last), w)| {
            ((last.x - w.start[0]) * w.along[0] + (last.y - w.start[1]) * w.along[1]).max(0.0)
        })
        .collect())
}

/// Where each walk ended in the world written as the overlay in `overlay`: its cell, and its
/// point in meters of the block it started in.
pub fn ends(
    ac1: &Path,
    overlay: &Path,
    world_key: &str,
    list: &[Walk],
) -> Result<Vec<(u32, Vec3)>, String> {
    let dir = dereth_dat::overlay::OverlayDir::new(overlay).map_err(|e| format!("{e:?}"))?;
    let store = Arc::new(
        RetailDatStore::open_dir(ac1)
            .map_err(|e| format!("{e:?}"))?
            .with_overlay(&dir, Some(world_key))
            .map_err(|e| format!("{e:?}"))?,
    );
    let region = dereth_world_data::landblock::load_region(&store).map_err(|e| format!("{e:?}"))?;
    // The human as the install has it (the overlay may write over AC1's own setups).
    let base = RetailDatStore::open_dir(ac1).map_err(|e| format!("{e:?}"))?;
    let b = base
        .read_typed(DbType::Setup, DataId(HUMAN))
        .map_err(|e| format!("{e:?}"))?;
    let human = Arc::new(dereth_world_data::setup::setup_geometry(
        &Setup::decode_payload(DataId(HUMAN), &b).map_err(|e| format!("{e:?}"))?,
    ));
    let mut by_block: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (k, w) in list.iter().enumerate() {
        by_block.entry(w.block).or_default().push(k);
    }
    let blocks: Vec<(usize, usize)> = by_block.keys().copied().collect();
    let memo: std::sync::Mutex<HashMap<u32, Option<Arc<SetupGeometry>>>> =
        std::sync::Mutex::new(HashMap::new());
    let geometry = |id: DataId| -> Option<Arc<SetupGeometry>> {
        if let Some(g) = memo.lock().expect("memo").get(&id.0) {
            return g.clone();
        }
        let g = static_geometry(&store, id, &mut SetupPartStats::default()).map(Arc::new);
        memo.lock().expect("memo").insert(id.0, g.clone());
        g
    };
    let out = std::sync::Mutex::new(vec![(0u32, Vec3::ZERO); list.len()]);
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                let Ok(land) = DatLandSource::new(Arc::clone(&store), &region) else {
                    return;
                };
                let land = Arc::new(land);
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&block) = blocks.get(i) else { break };
                    let statics = statics_round(&store, block);
                    // do not always go as each would alone.
                    let got: Vec<(usize, (u32, Vec3))> = by_block[&block]
                        .iter()
                        .map(|&k| {
                            let mut world = physics_world(&land, block, &statics, &geometry);
                            (k, walk(&mut world, &land, &human, &list[k], &mut 0.0))
                        })
                        .collect();
                    let mut o = out.lock().expect("results");
                    for (k, g) in got {
                        o[k] = g;
                    }
                }
            });
        }
    });
    Ok(out.into_inner().expect("results"))
}

/// The statics of a block and its eight neighbors, each with its block.
fn statics_round(
    store: &RetailDatStore,
    block: (usize, usize),
) -> Vec<((i32, i32), DataId, Frame)> {
    let mut v = Vec::new();
    for dx in -1..=1i32 {
        for dy in -1..=1i32 {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            let (bx, by) = (block.0 as i32 + dx, block.1 as i32 + dy);
            if !(0..255).contains(&bx) || !(0..255).contains(&by) {
                continue;
            }
            #[allow(clippy::cast_sign_loss)]
            let id = DataId(((bx as u32) << 24) | ((by as u32) << 16) | 0xFFFE);
            let Some(info) = store
                .read_typed(DbType::Lbi, id)
                .ok()
                .and_then(|b| LandblockInfo::decode_payload(id, &b).ok())
            else {
                continue;
            };
            v.extend(info.objects.into_iter().map(|e| ((bx, by), e.id, e.frame)));
        }
    }
    v
}

/// The outdoor cell holding block-local point `p` of `block`, and the point in it.
fn land_cell(block: (i32, i32), p: Vec3) -> (CellId, Vec3) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mut c = LandblockId::new(block.0 as u8, block.1 as u8).cell(1);
    let mut o = p;
    dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o);
    (c, o)
}

fn physics_world(
    land: &Arc<DatLandSource>,
    block: (usize, usize),
    statics: &[((i32, i32), DataId, Frame)],
    geometry: &(dyn Fn(DataId) -> Option<Arc<SetupGeometry>> + Sync),
) -> PhysicsWorld {
    let mut world = PhysicsWorld::new(Arc::clone(land) as Arc<dyn LandSource>);
    for dx in -2..=2i32 {
        for dy in -2..=2i32 {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            let (bx, by) = (block.0 as i32 + dx, block.1 as i32 + dy);
            if (0..255).contains(&bx) && (0..255).contains(&by) {
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                let id = LandblockId::new(bx as u8, by as u8);
                land.load_block_cells(id);
                let _ = land.landblock(id);
            }
        }
    }
    let list: Vec<CellStatic> = statics
        .iter()
        .map(|&(b, id, frame)| CellStatic {
            cell: land_cell(b, frame.origin).0,
            id,
            frame,
            scale: 1.0,
        })
        .collect();
    let mut objs = CellStaticObjects::new();
    objs.init_with(&mut |id, _| geometry(id), &mut world, &list);
    world
}

/// Where `w` ended: its cell, and its point in meters of the block it started in; `clock` is the
/// world's time, which runs on from one walk to the next (a world given an earlier time moves
/// nothing).
fn walk(
    world: &mut PhysicsWorld,
    land: &DatLandSource,
    body: &Arc<SetupGeometry>,
    w: &Walk,
    clock: &mut f64,
) -> (u32, Vec3) {
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    // LINT-OK: a landblock index, 0 to 254.
    let block = (w.block.0 as i32, w.block.1 as i32);
    let mut start = Vec3::new(w.start[0], w.start[1], w.start[2] + 0.02);
    let (cell, mut origin) = land_cell(block, start);
    let h = world.create(ObjectId(0x5000_0001), Arc::clone(body), true);
    if w.set_down {
        if let Some(z) = land.ground_height(cell.landblock(), origin.x, origin.y) {
            start.z += z - origin.z;
            origin.z = z;
        }
        if let Some(o) = world.get_mut(h) {
            o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
                Vec::new(),
                true,
            )));
            o.update_time = *clock;
        }
        world.enter_world(h, &Position::new(cell, Frame::new(origin, Quat::IDENTITY)));
    } else {
        world.enter_cell(h, cell);
        if let Some(o) = world.get_mut(h) {
            o.position = Position::new(cell, Frame::new(origin, Quat::IDENTITY));
            o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
                Vec::new(),
                true,
            )));
            o.transient_state.set_active_bit(true);
            o.calc_acceleration();
            o.update_time = *clock;
        }
        world.calc_cross_cells(h, false);
    }
    // LINT-OK: the physics tick rate, a small whole number.
    #[allow(clippy::cast_possible_truncation)]
    let step = SPEED / HZ as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (f64::from(w.dist / SPEED) * HZ).ceil() as usize;
    let offsets = vec![
        Frame::new(
            Vec3::new(w.along[0] * step, w.along[1] * step, 0.0),
            Quat::IDENTITY
        );
        frames + 5
    ];
    if let Some(o) = world.get_mut(h) {
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(offsets, true)));
    }
    let mut last = start;
    let mut last_cell = cell.0;
    for _ in 0..frames {
        *clock += 1.0 / HZ;
        world.use_time(LocalTime(*clock), false);
        if let Some(o) = world.get(h) {
            let cid = o.position.cell.0;
            #[allow(clippy::cast_possible_wrap)]
            let (cbx, cby) = ((cid >> 24) as i32, ((cid >> 16) & 0xFF) as i32);
            #[allow(clippy::cast_precision_loss)]
            let off = Vec3::new(
                (cbx - block.0) as f32 * BLOCK,
                (cby - block.1) as f32 * BLOCK,
                0.0,
            );
            last = o.position.frame.origin.add(off);
            last_cell = cid;
        }
        let gone = (last.x - start.x) * w.along[0] + (last.y - start.y) * w.along[1];
        if w.until.is_some_and(|u| gone >= u) {
            break;
        }
    }
    world.destroy(h);
    (last_cell, last)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: an instrument that walks into the AC2 world's buildings)
    use super::*;

    /// Lanes across each doorway, as shares of its width either side of its middle.
    const LANES: [f32; 5] = [-0.3, -0.15, 0.0, 0.15, 0.3];
    /// A lane starts this far outside the doorway, this far over its floor, and walks this far.
    /// A lane that never leaves its start is walked again from each of these in turn, set down
    /// on the land there.
    const OUT: f32 = 6.0;
    const AGAIN: [f32; 3] = [6.0, 12.0, 18.0];
    const DROP: f32 = 1.0;
    const DIST: f32 = 20.0;
    /// A lane stops this far past its doorway: it is in.
    const PAST: f32 = 2.0;

    /// The walk-in sweep: AC1's human walked straight in through every doorway out to the land
    /// of a built New Dereth world, five lanes across each, from 6 m outside, for as long as a
    /// 20 m walk takes or until 2 m past the doorway (a lane that drops to the ground short of a
    /// doorway climbs back up slowly). A lane is in when it ends in one of the building's rooms
    /// at least 1.5 m past the doorway, standing less than 1.2 m over the doorway's floor; it is
    /// stuck when it never left its start: it began inside something, or came down on a slope
    /// it then walks straight up, which holds a body still (it keeps the slope as a wall to slide
    /// along until it steps away from it; a character put into the world is not held so). A
    /// lane that is stuck is walked again set down on the land as the client puts a character
    /// in, from 6 m out, then 12 m, then 18 m, the walk longer by as much; lanes stuck from every
    /// start are not counted. Prints a line a lane (with the start it was walked from), a
    /// line a doorway, and the totals: the lanes in of those that left their start, and the
    /// doorways at least one lane entered.
    ///
    /// It reads a whole built world, which takes minutes to make, so it runs only when asked:
    ///
    /// ```text
    /// dereth-art newdereth-world --world <checkout>/world.toml --dats <AC1 install> \
    ///     --ac2-dats <AC2 install> --out <dir>/overlay [--new-land]
    /// DERETH_ART_WORLD_OVERLAY=<dir>/overlay cargo test --profile test-release -p dereth-art \
    ///     --lib walk_in_sweep -- --ignored --nocapture
    /// ```
    ///
    /// `--new-land` builds a land other than the one the repository pins (with a copy of the
    /// repository, so its ledger is left as it was). The AC1 install is `DERETH_TEST_DAT_DIR`'s;
    /// the doorways are `doorways-ac2.tsv` beside the overlay folder.
    #[test]
    #[ignore = "walks a built world: set DERETH_ART_WORLD_OVERLAY (see the doc)"]
    fn walk_in_sweep() {
        let overlay = std::path::PathBuf::from(
            std::env::var("DERETH_ART_WORLD_OVERLAY").expect("DERETH_ART_WORLD_OVERLAY"),
        );
        let ac1 = dereth_dat::testing::dat_dir();
        let tsv = std::fs::read_to_string(
            overlay
                .parent()
                .expect("the overlay's folder")
                .join("doorways-ac2.tsv"),
        )
        .expect("doorways-ac2.tsv beside the overlay");
        // Each lane: its doorway's name, its share of the width, the doorway's floor, and where
        // it is walked from at `out` meters outside.
        let mut rows = Vec::new();
        for line in tsv.lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            let n = |k: usize| f[k].parse::<f32>().expect("a number");
            let (mx, my, floor, ix, iy, width) = (n(2), n(3), n(4), n(5), n(6), n(7));
            let b = u16::from_str_radix(f[1], 16).expect("a block");
            for share in LANES {
                let t = share * width;
                let lane = move |out: f32, set_down: bool| Walk {
                    block: (usize::from(b >> 8), usize::from(b & 0xFF)),
                    start: [mx - iy * t - ix * out, my + ix * t - iy * out, floor + DROP],
                    along: [ix, iy],
                    dist: DIST + out - OUT,
                    until: Some(out + PAST),
                    set_down,
                };
                rows.push((f[0].to_owned(), share, floor, lane));
            }
        }
        // How far past its doorway a lane walked from `out` meters outside ended.
        let past_of = |p: &Vec3, w: &Walk, out: f32| {
            (p.x - w.start[0]) * w.along[0] + (p.y - w.start[1]) * w.along[1] - out
        };
        let mut results: Vec<(f32, Walk, (u32, Vec3))> = Vec::with_capacity(rows.len());
        let list: Vec<Walk> = rows.iter().map(|r| (r.3)(OUT, false)).collect();
        let got = ends(&ac1, &overlay, crate::ac2_world::WORLD_KEY, &list).expect("the walks");
        results.extend(list.into_iter().zip(got).map(|(w, g)| (OUT, w, g)));
        for out in AGAIN {
            let again: Vec<usize> = (0..rows.len())
                .filter(|&k| {
                    let (o, w, (_, p)) = &results[k];
                    past_of(p, w, *o) < -(o - 0.3)
                })
                .collect();
            let list: Vec<Walk> = again.iter().map(|&k| (rows[k].3)(out, true)).collect();
            let got = ends(&ac1, &overlay, crate::ac2_world::WORLD_KEY, &list).expect("the walks");
            for ((k, w), g) in again.into_iter().zip(list).zip(got) {
                results[k] = (out, w, g);
            }
        }
        let (mut lanes, mut lanes_in) = (0, 0);
        let mut doorways: Vec<(String, usize, usize)> = Vec::new();
        for ((name, share, floor, _), (out, w, (cell, p))) in rows.iter().zip(&results) {
            let past = past_of(p, w, *out);
            let stuck = past < -(out - 0.3);
            let entered = cell & 0xFFFF >= 0x0100 && past >= 1.5 && p.z - floor < 1.2;
            let what = if entered {
                "in"
            } else if stuck {
                "stuck"
            } else {
                "caught"
            };
            println!(
                "lane\t{name}\t{share:+.2}\tfrom {out:.0}{}\t{cell:08X}\tpast {past:.2}\tover floor {:.2}\t{what}",
                if w.set_down { " set down" } else { "" },
                p.z - floor
            );
            if doorways.last().is_none_or(|d| d.0 != *name) {
                doorways.push((name.clone(), 0, 0));
            }
            let d = doorways.last_mut().expect("a doorway");
            lanes += usize::from(!stuck);
            lanes_in += usize::from(entered);
            d.1 += usize::from(!stuck);
            d.2 += usize::from(entered);
        }
        for (name, left, entered) in &doorways {
            println!(
                "doorway\t{name}\t{entered} of {left} lanes in\t{}",
                if *entered > 0 {
                    "entered"
                } else {
                    "not entered"
                }
            );
        }
        let entered = doorways.iter().filter(|d| d.2 > 0).count();
        println!(
            "walked in: {lanes_in} of {lanes} lanes that left their start ({} lanes); {entered} of {} doorways entered by at least one lane",
            rows.len(),
            doorways.len()
        );
    }
}
