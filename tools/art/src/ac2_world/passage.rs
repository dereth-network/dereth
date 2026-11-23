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
/// heading `along` (a unit vector) for `dist` meters.
#[derive(Clone, Copy, Debug)]
pub struct Walk {
    pub block: (usize, usize),
    pub start: [f32; 3],
    pub along: [f32; 2],
    pub dist: f32,
}

/// How far each walk got along its heading in the world written as the overlay in `overlay`,
/// meters.
pub fn walks(
    ac1: &Path,
    overlay: &Path,
    world_key: &str,
    list: &[Walk],
) -> Result<Vec<f32>, String> {
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
    let out = std::sync::Mutex::new(vec![0f32; list.len()]);
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
                    let got: Vec<(usize, f32)> = by_block[&block]
                        .iter()
                        .map(|&k| {
                            let mut world = physics_world(&land, block, &statics, &geometry);
                            (k, walk(&mut world, &human, &list[k], &mut 0.0))
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

/// How far `w` got along its heading; `clock` is the world's time, which runs on from one walk to
/// the next (a world given an earlier time moves nothing).
fn walk(world: &mut PhysicsWorld, body: &Arc<SetupGeometry>, w: &Walk, clock: &mut f64) -> f32 {
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    // LINT-OK: a landblock index, 0 to 254.
    let block = (w.block.0 as i32, w.block.1 as i32);
    let start = Vec3::new(w.start[0], w.start[1], w.start[2] + 0.02);
    let (cell, origin) = land_cell(block, start);
    let h = world.create(ObjectId(0x5000_0001), Arc::clone(body), true);
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
        }
    }
    world.destroy(h);
    ((last.x - start.x) * w.along[0] + (last.y - start.y) * w.along[1]).max(0.0)
}
