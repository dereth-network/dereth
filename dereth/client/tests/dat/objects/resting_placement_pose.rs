//! A static object is drawn at the placement frame the client installs, and that is the frame
//! collision uses.
//!
//! Placement selection looks up key `0x65` (ACE's `Placement.Resting`) and, on a miss, key `0`
//! (`Default`); only when both miss are the parts left at the identity. The tests name specific
//! setups and the frame each resolves to, then cover the training dungeon's whole population, which
//! exercises both arms: 84 of its 170 setups carry both keys (62 with different frames) and 86 carry
//! only key `0`.
//!
//! Fixture: `client_cell_1.dat` and `client_portal.dat` under `$DERETH_TEST_DAT_DIR`; the test fails
//! when they are absent. No GPU.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::{Decode, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, Frame, Quat, Vec3};
use {
    dereth_client_runtime::models::resolve_parts, dereth_client_runtime::models::PLACEMENT_DEFAULT,
    dereth_client_runtime::models::PLACEMENT_RESTING,
};
use {
    dereth_client_runtime::object_physics::setup_geometry_with_parts,
    dereth_client_runtime::object_physics::SetupPartStats,
};
use {dereth_world_data::env_cells::cell_statics, dereth_world_data::env_cells::EnvCellLoader};

/// Starter-area entry 0, "Holtburg": the training-academy block.
const TRAINING_DUNGEON: u16 = 0x8602;

/// The room `--start-cell 0x860201AD` stands in.
const SHOT_ROOM: u32 = 0x8602_01AD;

/// The retail store, or **fail**: a missing install is a failure, never a skip.
fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

fn setup(store: &RetailDatStore, id: DataId) -> Option<Setup> {
    let b = store.read_typed(DbType::Setup, id).ok()?;
    Setup::decode_payload(id, &b).ok()
}

/// Every distinct setup record the block's cells instantiate, with how many times it is placed.
fn block_setups(store: &RetailDatStore) -> BTreeMap<DataId, usize> {
    let mut loader = EnvCellLoader::new();
    let mut ids: BTreeMap<DataId, usize> = BTreeMap::new();
    for d in &loader.load_block(store, TRAINING_DUNGEON) {
        for s in cell_statics(d) {
            // A `0x01……` ID is a bare graphics object. The client wraps it in a one-part setup at
            // the identity, with no placement hash to query.
            if s.id.0 >> 24 == 0x02 {
                *ids.entry(s.id).or_default() += 1;
            }
        }
    }
    ids
}

fn identity() -> Frame {
    Frame::new(Vec3::ZERO, Quat::IDENTITY)
}

fn moved(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z).magnitude()
}

// ---------------------------------------------------------------------------------------------
// 1. The anchors: named setups, and the frame each one resolves to
// ---------------------------------------------------------------------------------------------

/// Oracle: setup records `0x02000183` and `0x02000294` in `client_portal.dat`, both statics of
/// cell `0x860201AD`.
///
/// These two are the largest movers of that room: the library case at the back wall, which
/// `Placement.Resting` lifts **0.915 m**, and the rubble bench beside it, lifted **0.5825 m**.
/// Drawn at key `0` both would sink into the flagstones, the case showing four shelves instead of
/// five.
#[test]
fn the_two_largest_movers_of_the_shot_room_resolve_to_the_resting_frame() {
    let store = store();

    // The library case: one part, and `Placement.Resting` is a pure +Z lift of 0.915 m.
    let bookcase = setup(&store, DataId(0x0200_0183)).expect("0x02000183 decodes");
    let parts = resolve_parts(&store, DataId(0x0200_0183));
    assert_eq!(parts.len(), 1);
    let resting = bookcase.placement_frames[&PLACEMENT_RESTING].frames[0];
    let default = bookcase.placement_frames[&PLACEMENT_DEFAULT].frames[0];
    assert_eq!(parts[0].frame, resting, "0x02000183 must be drawn at 0x65");
    assert_ne!(
        parts[0].frame, default,
        "0x02000183's two placements differ; this is the point"
    );
    assert_eq!(default, identity(), "key 0 is the identity here");
    assert!(
        (resting.origin.z - 0.914_999).abs() < 1e-5,
        "0x02000183 rests 0.915 m up, not {:?}",
        resting.origin
    );

    // The rubble bench.
    let bench = setup(&store, DataId(0x0200_0294)).expect("0x02000294 decodes");
    let parts = resolve_parts(&store, DataId(0x0200_0294));
    assert_eq!(parts.len(), 1);
    assert_eq!(
        parts[0].frame,
        bench.placement_frames[&PLACEMENT_RESTING].frames[0]
    );
    assert!(
        (parts[0].frame.origin.z - 0.5825).abs() < 1e-5,
        "0x02000294 rests 0.5825 m up, not {:?}",
        parts[0].frame.origin
    );
}

/// Oracle: the same DAT, for a **multi-part** setup, because placement selection installs one
/// placement frame for the whole part array and indexes it by part.
///
/// `0x02000129` is nine parts carrying both keys, and **all nine frames differ between them**, so
/// every part must come back at `0x65`'s frame for its own index — not at key `0`'s, not at frame
/// 0's and not at the identity. A per-index mix-up shows here rather than cancelling.
#[test]
fn every_part_of_a_multi_part_setup_takes_its_own_resting_frame() {
    let store = store();
    let id = DataId(0x0200_0129);
    let s = setup(&store, id).expect("0x02000129 decodes");
    let resting = &s.placement_frames[&PLACEMENT_RESTING].frames;
    let default = &s.placement_frames[&PLACEMENT_DEFAULT].frames;
    assert_eq!((s.parts.len(), resting.len(), default.len()), (9, 9, 9));
    let parts = resolve_parts(&store, id);
    assert_eq!(parts.len(), 9);
    for (i, p) in parts.iter().enumerate() {
        assert_eq!(p.frame, resting[i], "part {i} of 0x02000129");
        assert_ne!(
            p.frame, default[i],
            "part {i} of 0x02000129 must not be key 0's"
        );
        assert_eq!(p.gfxobj, s.parts[i]);
    }
    // The nine frames are distinct from one another too, so swapping two would be caught.
    for i in 1..9 {
        assert_ne!(resting[i], resting[0], "0x02000129 frame {i}");
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The whole block, both arms of the fallback
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.placement.a-static-is-drawn-at-its-resting-frame
/// Oracle: `client_cell_1.dat` block `0x8602` and every setup record its cells name.
///
/// Asserts the block's census *and* that every setup resolves to the frame
/// selected for key `0x65`. The census numbers are here
/// so that a change to the decoder or to the cell walk that quietly emptied the sample cannot leave
/// the equality assertions passing over nothing.
#[test]
fn the_training_dungeons_setups_are_all_drawn_at_the_frame_the_client_installs() {
    let store = store();
    let ids = block_setups(&store);
    let (mut both, mut differ, mut only_default, mut only_resting, mut neither) = (0, 0, 0, 0, 0);
    let mut placements = 0usize;

    for (id, n) in &ids {
        placements += n;
        let Some(s) = setup(&store, *id) else {
            continue;
        };
        let resting = s.placement_frames.get(&PLACEMENT_RESTING);
        let default = s.placement_frames.get(&PLACEMENT_DEFAULT);
        match (resting, default) {
            (Some(r), Some(d)) => {
                both += 1;
                if r.frames != d.frames {
                    differ += 1;
                }
            }
            (None, Some(_)) => only_default += 1,
            (Some(_), None) => only_resting += 1,
            (None, None) => neither += 1,
        }
        // The pose `SetPlacementFrame` would install: `0x65`, else `0`, else nothing at all.
        let want = resting.or(default);
        let drawn = resolve_parts(&store, *id);
        assert_eq!(drawn.len(), s.parts.len(), "{id} part count");
        for (i, p) in drawn.iter().enumerate() {
            let expected = want
                .and_then(|f| f.frames.get(i).copied())
                .unwrap_or_else(identity);
            assert_eq!(p.frame, expected, "{id} part {i}");
        }
    }

    eprintln!(
        "block {TRAINING_DUNGEON:04X}: {placements} setup loads naming {} distinct setups -- \
         {both} carry both placement keys ({differ} of them with different frames), \
         {only_default} carry only Placement.Default, {only_resting} only Placement.Resting, \
         {neither} neither",
        ids.len()
    );
    assert_eq!((placements, ids.len()), (557, 170));
    assert_eq!(
        (both, differ, only_default, only_resting, neither),
        (84, 62, 86, 0, 0)
    );
}

/// Oracle: the 86 setups of block `0x8602` that carry **no** `0x65` entry.
///
/// Placement selection's second lookup is not decoration — half this block depends on it, and a
/// synthesized one-part setup writes its only entry at key `0`. Eleven of the 86 carry a
/// key-`0` frame that is *not* the identity, so dropping the fallback would visibly flatten them;
/// those eleven are the ones this test can see.
#[test]
fn a_setup_with_no_resting_placement_still_draws_at_the_default() {
    let store = store();

    // Two named ones first. `0x020003B5` is two parts standing 0.5 m up, 16 placements in the
    // block; `0x0200009E` is twelve parts whose key-0 frames are rotations, so a fallback that
    // returned the identity would leave the whole object unrotated rather than merely unlifted.
    for id in [0x0200_03B5u32, 0x0200_009E] {
        let did = DataId(id);
        let s = setup(&store, did).expect("decodes");
        assert!(
            !s.placement_frames.contains_key(&PLACEMENT_RESTING),
            "{did} must have no 0x65"
        );
        let default = &s.placement_frames[&PLACEMENT_DEFAULT].frames;
        let drawn = resolve_parts(&store, did);
        assert_eq!(drawn.len(), s.parts.len());
        for (i, p) in drawn.iter().enumerate() {
            assert_eq!(p.frame, default[i], "{did} part {i} falls back to key 0");
        }
        assert!(
            default.iter().any(|f| *f != identity()),
            "{did}'s key-0 pose must not be the identity, or this proves nothing"
        );
    }

    // And the whole arm.
    let ids = block_setups(&store);
    let (mut fallback, mut non_identity) = (0usize, 0usize);
    for id in ids.keys() {
        let Some(s) = setup(&store, *id) else {
            continue;
        };
        if s.placement_frames.contains_key(&PLACEMENT_RESTING) {
            continue;
        }
        let Some(default) = s.placement_frames.get(&PLACEMENT_DEFAULT) else {
            continue;
        };
        fallback += 1;
        let drawn = resolve_parts(&store, *id);
        for (i, p) in drawn.iter().enumerate() {
            assert_eq!(p.frame, default.frames[i], "{id} part {i}");
        }
        if default.frames.iter().any(|f| *f != identity()) {
            non_identity += 1;
        }
    }
    eprintln!(
        "{fallback} setups fall back to Placement.Default, {non_identity} of them to a pose that \
         is not the identity"
    );
    assert_eq!((fallback, non_identity), (86, 11));
}

// ---------------------------------------------------------------------------------------------
// 3. The acceptance: the drawn pose is the collided pose
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.placement.the-drawn-pose-is-the-collision-pose
/// Oracle: the same dat, read twice — once by the draw path and once by the collision path.
///
/// [`dereth_physics::SetupGeometry::placed_part`] composes the object's frame with
/// `SetupPart::placement_frame`, which is filled from `0x65` with the same fallback. A setup that
/// carries both placement frames is drawn at `0x65`, the same frame `placed_part` collides against;
/// drawing at key `0` would disagree for 62 of the block's 170 setups.
#[test]
fn the_drawn_pose_is_the_pose_collision_uses() {
    let store = store();
    let ids = block_setups(&store);
    let mut stats = SetupPartStats::default();
    let (mut compared, mut both_keys) = (0usize, 0usize);
    for id in ids.keys() {
        let Some(s) = setup(&store, *id) else {
            continue;
        };
        if s.placement_frames.contains_key(&PLACEMENT_RESTING)
            && s.placement_frames.contains_key(&PLACEMENT_DEFAULT)
        {
            both_keys += 1;
        }
        let drawn = resolve_parts(&store, *id);
        let collided = setup_geometry_with_parts(&store, &s, &mut stats);
        if collided.parts.is_empty() {
            // No placement frame at either key: the collision side models no parts and the draw
            // side puts them at the identity. Nothing to compare, and nothing moves either way.
            assert!(!s.placement_frames.contains_key(&PLACEMENT_RESTING));
            assert!(!s.placement_frames.contains_key(&PLACEMENT_DEFAULT));
            continue;
        }
        assert_eq!(collided.parts.len(), drawn.len(), "{id} part count");
        for (i, (d, c)) in drawn.iter().zip(collided.parts.iter()).enumerate() {
            assert_eq!(
                d.frame, c.placement_frame,
                "{id} part {i}: drawn vs collided"
            );
            compared += 1;
        }
    }
    eprintln!(
        "{compared} parts compared across {} setups, {both_keys} of them carrying both keys",
        ids.len()
    );
    assert_eq!((both_keys, compared), (84, 289));
}

// ---------------------------------------------------------------------------------------------
// 4. One room, movers and non-movers
// ---------------------------------------------------------------------------------------------

/// Oracle: cell `0x860201AD` of `client_cell_1.dat` — the room `--start-cell` stands in.
///
/// Names, per static, which ones the resting frame moves: 22 statics, 17 of which resolve to a pose
/// that differs from key `0`, and 5 of which do not move at all -- three of those because they
/// carry no `0x65` entry and so take the fallback. The walls, floor, rug and map are among the
/// non-movers.
#[test]
fn the_shot_rooms_statics_split_into_movers_and_non_movers() {
    let store = store();
    let mut loader = EnvCellLoader::new();
    let cells = loader.load_block(&store, TRAINING_DUNGEON);
    let (mut statics, mut movers, mut fallback) = (0usize, 0usize, 0usize);
    let mut biggest = (0.0f32, DataId(0));
    for d in cells.iter().filter(|d| d.id.0 == SHOT_ROOM) {
        for s in cell_statics(d) {
            let Some(dec) = setup(&store, s.id) else {
                continue;
            };
            statics += 1;
            if !dec.placement_frames.contains_key(&PLACEMENT_RESTING) {
                fallback += 1;
            }
            // The key-`0` pose: key `0` outright, identity when absent.
            let old = |i: usize| {
                dec.placement_frames
                    .get(&PLACEMENT_DEFAULT)
                    .and_then(|p| p.frames.get(i).copied())
                    .unwrap_or_else(identity)
            };
            let mut worst = 0.0f32;
            for (i, p) in resolve_parts(&store, s.id).iter().enumerate() {
                worst = worst.max(moved(p.frame.origin, old(i).origin));
            }
            if worst > 0.0 {
                movers += 1;
            }
            if worst > biggest.0 {
                biggest = (worst, s.id);
            }
        }
    }
    eprintln!(
        "cell {SHOT_ROOM:#010X}: {statics} statics, {movers} drawn at a different pose than key 0, \
         {fallback} with no Placement.Resting entry at all; largest move {:.4} m by {}",
        biggest.0, biggest.1
    );
    assert_eq!((statics, movers, fallback), (22, 17, 3));
    assert_eq!(biggest.1, DataId(0x0200_0183));
    assert!((biggest.0 - 0.939_9).abs() < 1e-3, "{}", biggest.0);
}
