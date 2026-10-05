//! A landblock round trip moves no pixel: after walking out and back, the frame at home is the
//! frame at home before the walk, bit for bit, whether or not the home block left the window
//! (`land_radius` 3 keeps it resident on a one-block trip; a four-block leg pushes it out). The
//! landscape re-bakes bit-identically, and the local body's parts and the chase camera are placed
//! after the window re-centres (`WorldScene::place_local_body`), in the viewer-block-relative space
//! the re-centre moved to. Placing the parts first leaves them one landblock out per axis of shift;
//! computing the camera first and subtracting the shift loses low `f32` bits at a four-block hop.
//! Calibrated both ways: a frame five blocks away must differ, and one further update at the same
//! station must restore the frame exactly. Fixture: the retail dats on a WARP device at 800x600,
//! walking from Holtburg. Fails when the retail dats are absent or a device is absent.

#![cfg(gpu)]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_client_runtime::character::CharacterInput;
use dereth_dat::RetailDatStore;
use dereth_primitives::{Frame, LandblockId, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;
use {
    dereth_client_runtime::landblock::block_xy,
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};

const W: usize = 800;
const H: usize = 600;
/// `BLOCK_LENGTH`: one landblock, and therefore the exact size of a part misplacement.
const BLOCK: f32 = 192.0;

/// The thresholds every count in this module is reported at. **A count produced by a threshold is
/// not meaningful until the threshold is stated**: a hard-edged object survives `t >= 128`, while
/// a camera's lost low bits give a sub-LSB haze that vanishes by `t >= 8`, and one count at
/// `t >= 1` would mix the two populations.
const BRACKETS: [u8; 8] = [1, 2, 4, 8, 16, 32, 64, 128];

/// The retail dats, or a failed test: a skipped test would read as a pass.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn block_at(x: i32, y: i32) -> LandblockId {
    assert!(
        (0..=0xFE).contains(&x) && (0..=0xFE).contains(&y),
        "block ({x},{y}) is off the world"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: both bounded to 0..=0xFE by the assertion above.
    LandblockId(((x as u16) << 8) | (y as u16))
}

/// `world::landblock_interior_release`'s own scene, unchanged, so the two measure the same subject.
fn embodied(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
    let cfg = SceneConfig::default();
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    scene
}

/// `world::landblock_interior_release`'s `go_to`: a teleport to the middle of one landblock and
/// one `WorldScene::update`, then the stream. Nothing advances a clock, so two frames taken at the
/// same station are comparable byte for byte and the pose is by construction the same one.
///
/// **The body is stood ON the ground, not dropped a metre above it, and that is load-bearing.**
/// Teleport runs position placement, and at `hx+1, hy+1` the step-down does not reach a metre, so
/// a one-metre drop commits with no contact: it clears the walkable state, leaves the ground and
/// removes link animations, which moves the body's **pose** (sequence frame 0 to 15.9998) with
/// its position bit-identical, thousands of pixels that are not the landscape. Five centimetres
/// is inside the step-down on every leg, so no leg raises a ground edge.
fn go_to(scene: &mut WorldScene, store: &Arc<RetailDatStore>, gpu: &mut Gpu, block: LandblockId) {
    let land = Arc::clone(scene.character.as_ref().expect("a body").land());
    let mid = 96.0f32;
    let z = land.ground_height(block, mid, mid).unwrap_or(0.0) + 0.05;
    // **The destination block is made resident *before* the teleport, which is what every
    // teleport in the client does.**
    //
    // `app::apply_player_teleport_at` is the production path: it runs
    // `character.land().load_block_cells(block)` and then `character.teleport(pos)`, as retail
    // updates the landscape block and prefetches its cells before the body is placed.
    // `ground_height` only *decodes* a block without *prefetching* it, and a merely decoded block
    // resolves **no cells**, so position placement would find no ground and commit with no
    // contact. The four-block leg, the only one that takes the home block out of residency, would
    // come home to a block with no cells and settle at the uncontacted z it was handed.
    land.load_block_cells(block);
    {
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(
            block.cell(1),
            Frame::new(Vec3::new(mid, mid, z), Quat::IDENTITY),
        ));
    }
    scene.follow_character_now();
    tick(scene);
    scene.stream(store, gpu).expect("the streamed blocks build");
    tick(scene);
    scene.stream(store, gpu).expect("nothing is left to stream");
    // **A second frame, so the station is sampled after the window has settled and not in the
    // update that replaced it.**
    //
    // A four-block leg replaces the whole 49-block window, and the frame drawn inside the update
    // that scrolled the window, re-baked its blocks and assembled their batches differs from the
    // next one by a band of LOD/billboard choices one step out (none at delta >= 64), with the
    // camera, `render_frame`, the parts, the pose and the resident count identical. The client
    // draws continuously and gets the next frame for free, and this module compares the camera's
    // arithmetic across a block shift, so both frames it compares are taken at the same point in
    // the window's life. That transient is not claimed to be correct here, only kept out.
}

/// One `WorldScene::update` and nothing else: no teleport, no move, no clock.
fn tick(scene: &mut WorldScene) {
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(0.0),
        0.0,
    );
}

fn shot(scene: &mut WorldScene, gpu: &mut Gpu) -> Vec<u8> {
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

/// Per-pixel maximum absolute channel difference over R, G and B. Alpha is forced opaque by
/// `CapturedImage::to_rgba` (`COLORWRITEENABLE = 7`, so the back buffer's alpha is never written),
/// and comparing a channel nothing writes would be comparing the clear.
fn deltas(a: &[u8], b: &[u8]) -> Vec<u8> {
    assert_eq!(a.len(), b.len(), "two frames of different sizes");
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0.iter())
        .map(|(p, q)| (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0))
        .collect()
}

fn at(d: &[u8], t: u8) -> usize {
    d.iter().filter(|v| **v >= t).count()
}

fn brackets(d: &[u8]) -> String {
    BRACKETS
        .iter()
        .map(|&t| format!("t>={t}: {}", at(d, t)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Bounding box, connected components and the size of the largest, at one threshold. A count says
/// how many pixels moved; this says whether they are *one object* or noise spread over the frame,
/// and the two readings have completely different causes.
fn shape(d: &[u8], t: u8) -> (usize, (usize, usize, usize, usize), usize, usize) {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    let mut n = 0usize;
    for (i, v) in d.iter().enumerate() {
        if *v >= t {
            n += 1;
            let (x, y) = (i % W, i / W);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    if n == 0 {
        return (0, (0, 0, 0, 0), 0, 0);
    }
    let mut seen = vec![false; W * H];
    let (mut comps, mut largest) = (0usize, 0usize);
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..W * H {
        if d[i] < t || seen[i] {
            continue;
        }
        comps += 1;
        let mut size = 0usize;
        stack.push(i);
        seen[i] = true;
        while let Some(p) = stack.pop() {
            size += 1;
            let (px, py) = (p % W, p / W);
            let mut push = |q: usize| {
                if d[q] >= t && !seen[q] {
                    seen[q] = true;
                    stack.push(q);
                }
            };
            if px > 0 {
                push(p - 1);
            }
            if px + 1 < W {
                push(p + 1);
            }
            if py > 0 {
                push(p - W);
            }
            if py + 1 < H {
                push(p + W);
            }
        }
        largest = largest.max(size);
    }
    (n, (x0, y0, x1, y1), comps, largest)
}

/// Everything one round trip produced, so a test can assert over it without re-running the walk.
struct Trip {
    /// The frame at home on the first visit.
    home: Vec<u8>,
    /// The frame at home after the walk.
    back: Vec<u8>,
    /// The frame at home after the walk **and one further `WorldScene::update`**: the
    /// single-variable differential. Nothing is teleported, streamed or re-baked between `back`
    /// and this; the only thing that runs is the body's own part placement.
    settled: Vec<u8>,
    /// A frame five blocks away, the differ's known-different pair.
    far: Vec<u8>,
    /// Did the home block ever leave the window during the walk?
    left_window: bool,
    /// Was it named by the window at home, before and after? (The positive control for the above:
    /// "not named at the far station" means nothing unless it *is* named here.)
    named_at_home: (bool, bool),
    /// Blocks resident at home, before and after.
    resident: (usize, usize),
    /// The body's first part's placement at home, before and after, and after the extra update.
    part0: (Vec3, Vec3, Vec3),
    /// `Character::render_frame().origin` at home, before and after. This is what the *camera* is
    /// derived from, and it is the control: if it moves, the body really went somewhere else.
    render_frame: (Vec3, Vec3),
    /// The camera's own position bits at home, before and after.
    camera: ([u32; 3], [u32; 3]),
    /// The ground edges raised so far, at each station, and the animation sequence's frame number
    /// there. A teleport can raise a `HitGround`/`LeaveGround` pair, and ground-hit handling
    /// removes link animations before applying current movement, so the body's **pose** can
    /// change across a round trip with its **position** bit-identical, a moving-pixel count that
    /// has nothing to do with re-baking. These two fields tell the two apart; without them the
    /// failure reads as a landscape defect.
    edges: (
        dereth_client_runtime::character::GroundEdges,
        dereth_client_runtime::character::GroundEdges,
        dereth_client_runtime::character::GroundEdges,
    ),
    frames: (f64, f64, f64),
}

/// Walk out along `legs`, come back, and record everything above.
fn round_trip(store: &Arc<RetailDatStore>, gpu: &mut Gpu, legs: &[(i32, i32)]) -> Trip {
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let home_id = block_at(hx, hy);
    let mut scene = embodied(store, gpu);
    let names_home = |s: &WorldScene| s.window_rings().iter().any(|r| r.block == (hx, hy));

    go_to(&mut scene, store, gpu, home_id);
    let named_before = names_home(&scene);
    let resident_before = scene.resident_blocks();
    let rf_before = scene
        .character
        .as_ref()
        .expect("a body")
        .render_frame()
        .origin;
    let part0_before = first_part(&scene);
    let edges_before = scene.character.as_ref().expect("a body").ground_edges();
    let frame_before = seq_frame(&scene);
    let cam_before = cam_bits(&scene);
    let home = shot(&mut scene, gpu);

    let mut left_window = false;
    for &(dx, dy) in legs {
        go_to(&mut scene, store, gpu, block_at(hx + dx, hy + dy));
        if !names_home(&scene) {
            left_window = true;
        }
    }
    go_to(&mut scene, store, gpu, home_id);
    let named_after = names_home(&scene);
    let resident_after = scene.resident_blocks();
    let rf_after = scene
        .character
        .as_ref()
        .expect("a body")
        .render_frame()
        .origin;
    let part0_after = first_part(&scene);
    let edges_after = scene.character.as_ref().expect("a body").ground_edges();
    let frame_after = seq_frame(&scene);
    let cam_after = cam_bits(&scene);
    let back = shot(&mut scene, gpu);

    // **The single-variable differential.** One more `WorldScene::update` at the same station: no
    // teleport, no input, no clock, and `stream` has nothing pending, so no block is fetched,
    // released, re-meshed or re-baked. The only thing that changes is the body's part placement.
    let blocks_before_tick = scene.resident_blocks();
    tick(&mut scene);
    scene.stream(store, gpu).expect("nothing to stream");
    assert_eq!(
        scene.resident_blocks(),
        blocks_before_tick,
        "the settling update streamed a block, so it is not a single-variable differential"
    );
    let part0_settled = first_part(&scene);
    let edges_settled = scene.character.as_ref().expect("a body").ground_edges();
    let frame_settled = seq_frame(&scene);
    let settled = shot(&mut scene, gpu);

    go_to(&mut scene, store, gpu, block_at(hx + 5, hy + 5));
    let far = shot(&mut scene, gpu);
    scene.release_textures(gpu);

    Trip {
        home,
        back,
        settled,
        far,
        left_window,
        named_at_home: (named_before, named_after),
        resident: (resident_before, resident_after),
        part0: (part0_before, part0_after, part0_settled),
        render_frame: (rf_before, rf_after),
        camera: (cam_before, cam_after),
        edges: (edges_before, edges_after, edges_settled),
        frames: (frame_before, frame_after, frame_settled),
    }
}

/// The animation sequence's current frame number: the pose, as one number.
fn seq_frame(scene: &WorldScene) -> f64 {
    let c = scene.character.as_ref().expect("a body");
    let d = c.driver();
    d.sequence.frame_number()
}

/// Where character part placement last put the body's first part, in the renderer's
/// viewer-block-relative space.
fn first_part(scene: &WorldScene) -> Vec3 {
    let c = scene.character.as_ref().expect("a body");
    let d = c.driver();
    d.part_array
        .parts
        .first()
        .map_or(Vec3::ZERO, |p| p.pos.origin)
}

fn cam_bits(scene: &WorldScene) -> [u32; 3] {
    let p = scene.camera.position;
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
}

/// A walk that keeps the home block resident throughout: `land_radius` is 3, so a 7x7 window, and
/// the block only leaves at an offset of 4.
const SHORT: [(i32, i32); 1] = [(1, 1)];
/// `world::landblock_interior_release`'s own `WALK`, minus the two home stations at its ends: out
/// to five blocks away and back, taking the home block out of the window.
const LONG: [(i32, i32); 9] = [
    (1, 1),
    (2, 2),
    (3, 3),
    (4, 4),
    (5, 5),
    (4, 4),
    (3, 3),
    (2, 2),
    (1, 1),
];

// ---------------------------------------------------------------------------------------------
// 1. The measurement
// ---------------------------------------------------------------------------------------------

/// Behaviour: world.streaming.a-block-that-returns-re-bakes-bit-identically
///
/// **Oracle: the scene's own draw, and the body's own part placement, not a golden frame.**
///
/// Calibrated in both directions inside the same run, because the claim is a *difference*:
///
/// * the **non-zero**: the differ is shown the home frame against a frame five landblocks away
///   and must read a large count, or a zero from it means nothing;
/// * the **zero**: the differ is shown a pair it is *known* to agree about (the home frame
///   against itself after one further update) and must read exactly 0 at every threshold, or a
///   difference from it means nothing either.
///
/// **And the block is asserted to have left.** A round-trip test that never pushed the block off
/// the window would pass trivially, so the long walk asserts departure, and the short walk
/// asserts *non*-departure, which shows that leaving the window is not what matters.
#[test]
fn a_round_trip_moves_no_pixel_and_the_landscape_re_bakes_bit_identically() {
    let store = store();

    let mut counts: Vec<(&str, usize, usize)> = Vec::new();
    for (label, legs, must_leave) in [
        ("adjacent block, stays in the window", &SHORT[..], false),
        ("five blocks out, leaves the window", &LONG[..], true),
    ] {
        let mut gpu = crate::common::test_gpu(
            u32::try_from(W).expect("a back-buffer width"),
            u32::try_from(H).expect("a back-buffer height"),
        );
        let t = round_trip(&store, &mut gpu, legs);

        let moved = deltas(&t.home, &t.back);
        let residual = deltas(&t.home, &t.settled);
        let control = deltas(&t.home, &t.far);
        let (n, bbox, comps, largest) = shape(&moved, 16);

        eprintln!("=== {label}");
        eprintln!(
            "    home block named by the window at home: {:?}; it left the window during the \
             walk: {}; blocks resident at home {:?}",
            t.named_at_home, t.left_window, t.resident
        );
        eprintln!("    home vs after the round trip : {}", brackets(&moved));
        eprintln!(
            "    home vs the far station      : {} (the known-different pair)",
            brackets(&control)
        );
        eprintln!(
            "    home vs after ONE more update: {} (the same station, nothing streamed)",
            brackets(&residual)
        );
        eprintln!(
            "    at t>=16 the moving pixels are {n} px in {comps} component(s), largest {largest}, \
             bounding box {bbox:?} of {W}x{H}"
        );
        eprintln!(
            "    body part[0] at home {:?} -> after the trip {:?} -> after one more update {:?}",
            t.part0.0, t.part0.1, t.part0.2
        );
        eprintln!(
            "    Character::render_frame origin {:?} -> {:?}; camera bits {:08x?} -> {:08x?}",
            t.render_frame.0, t.render_frame.1, t.camera.0, t.camera.1
        );
        eprintln!(
            "    ground edges {:?} -> {:?} -> {:?}; sequence frame {} -> {} -> {}",
            t.edges.0, t.edges.1, t.edges.2, t.frames.0, t.frames.1, t.frames.2
        );

        // --- the premise, before the calibrations: the module rests on "two frames taken at the
        // same station are comparable byte for byte", and that is a claim about the *body* as
        // much as about the landscape. A teleport that changes whether the body is on walkable
        // ground raises a ground edge; ground-hit and leave-ground handling each remove link
        // animations, which drains the motion ledger and moves the pose. That is thousands of
        // moving pixels with the position bit-identical, and it reads exactly like a re-baking
        // defect. Assert it instead of hoping.
        assert_eq!(
            (t.edges.0, t.edges.1),
            (t.edges.2, t.edges.2),
            "a teleport in this walk raised a ground edge, so the body's pose is not constant \
             across the stations and the pixel counts below are not about the landscape. \
             Sequence frame {} -> {} -> {}",
            t.frames.0,
            t.frames.1,
            t.frames.2
        );
        assert_eq!(
            (t.frames.0, t.frames.1),
            (t.frames.2, t.frames.2),
            "the animation sequence moved between the stations, so the poses differ"
        );

        // --- the two calibrations, before either number below is worth anything.
        assert!(
            at(&control, 1) > 400_000,
            "the differ cannot see a frame taken five landblocks away, so nothing it reports is a \
             measurement: {}",
            brackets(&control)
        );

        // --- the block: departed on the long walk, resident throughout on the short one, and
        // named by the window at home in both, so "not named" is a reading and not a vacuum.
        assert_eq!(
            t.named_at_home,
            (true, true),
            "the window does not name the home block at home, so it cannot say whether the block \
             left"
        );
        assert_eq!(
            t.left_window, must_leave,
            "{label}: the home block leaving the window is the premise this arm is built on"
        );

        // --- **the claim.** One further `WorldScene::update` at the same station, which streams,
        // releases and re-bakes nothing, restores the frame *exactly*. So none of the moving
        // pixels is a re-bake: every terrain mesh, merged land surface and baked object batch was
        // already identical, and what had moved was the body's part placement alone.
        for t2 in BRACKETS {
            assert_eq!(
                at(&residual, t2),
                0,
                "after one further update at the same station the frame still differs from the \
                 first visit by {} px at t>={t2}, so something other than the body's part \
                 placement changed across the round trip: {}",
                at(&residual, t2),
                brackets(&residual)
            );
        }

        // --- **and nothing moves at all.** The body's parts are placed and the chase camera
        // re-derived *after* `recenter()`, so the frame at home after the round trip is the frame
        // at home before it, bit for bit. `shape` is still called and reported above, because a
        // regression's *shape* (one hard-edged component, or a haze over the frame) is the first
        // thing that says which of the two orderings came back.
        for t2 in BRACKETS {
            assert_eq!(
                at(&moved, t2),
                0,
                "the round trip moved {} px at t>={t2}; the single-variable differential \
                 (0 px, above) says the landscape is not what moves, so the body's parts or the \
                 camera are being placed before the re-centre: {}",
                at(&moved, t2),
                brackets(&moved)
            );
        }
        assert_eq!(
            (n, comps, largest),
            (0, 0, 0),
            "the round trip left {n} px in {comps} component(s), largest {largest}, at t>=16, in \
             bounding box {bbox:?}"
        );
        counts.push((label, at(&moved, 1), at(&moved, 16)));
    }

    // --- leaving the window changes nothing. With both walks at zero this line cannot tell them
    // apart, but a regression that comes back **asymmetric** (one walk moving and the other not)
    // would be about the block leaving the window rather than about ordering, and this is the
    // only line here that would say so.
    assert_eq!(
        counts[0].2, counts[1].2,
        "at t>=16 the adjacent-block round trip moves {} px and the five-block walk moves {} px; \
         if the block leaving the window mattered these would differ",
        counts[0].2, counts[1].2
    );
    eprintln!(
        "round trip: {} -> {} px at t>=1 / {} at t>=16; {} -> {} px at t>=1 / {} at t>=16",
        counts[0].0, counts[0].1, counts[0].2, counts[1].0, counts[1].1, counts[1].2
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The body's parts follow the re-centre
// ---------------------------------------------------------------------------------------------

/// **The local body's parts are placed in the space the window re-centre moved to.**
///
/// `Character::update` ends by placing the parts from `render_frame()`, which is
/// viewer-block-relative. `WorldScene::place_local_body` runs immediately after `recenter()` and
/// re-places them in the new space, as `advance_objects` does for the server's objects. Placed
/// before the re-centre, a shift of `(dx, dy)` landblocks would leave them `(-192·dx, -192·dy)`
/// from where the camera is, so `off` is computed and printed: a whole number of landblocks is
/// that ordering returning.
#[test]
fn the_bodys_parts_do_not_lag_the_window_recentre() {
    let store = store();
    let mut gpu = crate::common::test_gpu(
        u32::try_from(W).expect("a back-buffer width"),
        u32::try_from(H).expect("a back-buffer height"),
    );
    let t = round_trip(&store, &mut gpu, &SHORT);

    // The control: the body did not move. `render_frame` is the object's own position in the same
    // space the parts are supposed to be in, and it is bit-identical across the round trip -- so
    // the parts being elsewhere is not the body having gone somewhere.
    assert_eq!(
        t.render_frame.0, t.render_frame.1,
        "the body's own render frame moved across the round trip, so a part offset below is not \
         a stale placement -- it is the body actually being somewhere else"
    );
    // And the parts are right on the first visit, so any offset is a lag, not a permanent
    // mis-placement. "Right" is asserted against the placement one further update produces at the
    // same station, not against `render_frame`: a part sits at its own offset inside the body
    // (`part[0].pos.z` is 81.963 where the body's own origin is 81.000), so equality with the
    // body's frame would be the wrong invariant.
    assert_eq!(
        t.part0.0, t.part0.2,
        "the first visit's part placement and the settled one disagree, so there is no single \
         correct placement here and the offset below is not a lag"
    );

    let off = (
        t.part0.1.x - t.part0.0.x,
        t.part0.1.y - t.part0.0.y,
        t.part0.1.z - t.part0.0.z,
    );
    eprintln!(
        "part lag: after one landblock out and back, body part[0] is at {:?} where its render \
         frame says {:?} -- an offset of {:?}, i.e. ({}, {}) landblocks",
        t.part0.1,
        t.render_frame.1,
        off,
        off.0 / BLOCK,
        off.1 / BLOCK
    );

    // **The invariant.** The last leg of `SHORT` walks from (+1, +1) back to home, a shift of
    // (-1, -1); parts placed before the re-centre would sit exactly -192 m out in each axis. The
    // round trip's placement must equal the settled one, i.e. part placement runs on the right
    // side of `recenter()`. The *size* of any `off` printed above identifies a regression: a whole
    // number of landblocks is that ordering, and anything else is something new.
    assert_eq!(
        t.part0.1,
        t.part0.2,
        "the body's parts are off by {off:?}, i.e. ({}, {}) landblocks, after a round trip that \
         ends on a one-block shift -- `update_parts` is running on the wrong side of `recenter()` \
         again",
        off.0 / BLOCK,
        off.1 / BLOCK
    );
    // The same equality where it holds regardless of ordering, which is what makes any offset
    // above a statement about ordering rather than about placement: one further
    // `WorldScene::update`, with no teleport and no re-centre, puts the parts exactly back.
    assert_eq!(
        t.part0.2, t.part0.0,
        "one further `WorldScene::update` at the same station does not put the parts back, so the \
         cause is not the ordering of `update_parts` against `recenter`"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The chase camera is re-derived after the re-centre, not patched
// ---------------------------------------------------------------------------------------------

/// **The chase camera comes back bit-identical after a multi-block shift, because it is
/// re-derived in the new space rather than shifted into it.**
///
/// `WorldScene::place_local_body` re-derives the camera with `follow_character()` after
/// `recenter()`. Computing it first and subtracting the shift would give `(v + 192·k) - 192·k` in
/// `f32`, which is not the identity: at a four-block hop the addend is 768, where the `f32`
/// spacing is 6.1e-5 against 7.6e-6 at the camera's own magnitude, and three low bits are lost,
/// a sub-LSB haze over the whole frame that is gone by `t >= 8`. At a one-block hop the same
/// subtraction happens to be exact, so the one-block arm separates "re-derived" from "no longer
/// computed" and the four-block arm is the only station where the lossy form is visible.
/// (`recenter` still shifts the body-less flycam, which owns its position.)
#[test]
fn a_multi_block_shift_keeps_the_cameras_bits_because_it_is_re_derived_not_shifted() {
    let store = store();

    /// One arm's camera bits before and after the round trip, and the pixels that moved.
    struct Arm {
        before: [u32; 3],
        after: [u32; 3],
        moved: Vec<u8>,
    }
    let mut arms: Vec<Arm> = Vec::new();
    for (label, legs) in [("one block", &SHORT[..]), ("four blocks", &[(4, 4)][..])] {
        let mut gpu = crate::common::test_gpu(
            u32::try_from(W).expect("a back-buffer width"),
            u32::try_from(H).expect("a back-buffer height"),
        );
        let t = round_trip(&store, &mut gpu, legs);
        // The control, per arm: the body's own position is bit-identical across the trip, so
        // anything the camera does is the camera's own arithmetic and not the subject moving.
        assert_eq!(
            t.render_frame.0, t.render_frame.1,
            "{label}: the body moved, so the camera differing says nothing about how the camera \
             was computed"
        );
        let moved = deltas(&t.home, &t.back);
        eprintln!(
            "camera, {label} out and back: bits {:08x?} -> {:08x?}, render_frame {:?}; \
             home vs back {}",
            t.camera.0,
            t.camera.1,
            t.render_frame.1,
            brackets(&moved)
        );
        arms.push(Arm {
            before: t.camera.0,
            after: t.camera.1,
            moved,
        });
    }
    let (one, four) = (&arms[0], &arms[1]);

    // **Both arms bit-identical**: the camera does not depend on the size of the shift. The
    // four-block arm reads `[42c00000, 42b72ded, 42a7174e]` on both sides of the trip. The
    // one-block arm is the shift at which even a subtracted camera is exact, so a loss there is
    // something other than the subtraction.
    assert_eq!(
        one.before, one.after,
        "a one-block round trip loses camera bits, which not even a patched camera does -- the \
         subtraction is exact at one block, so this is something other than the subtraction"
    );
    assert_eq!(
        four.before, four.after,
        "the camera does not come back bit-identical after a four-block shift, so it is being \
         patched by subtracting the block shift rather than re-derived after `recenter()`: \
         `(v + 192k) - 192k` is not the identity in `f32` and loses three low bits at k = 4"
    );

    // The pixel cost of lost camera bits, bracketed at `t >= 1` and `t >= 8` so that the haze
    // (present at 1, gone by 8) cannot be mistaken for an object or hidden inside one count.
    let (a1, a8) = (at(&one.moved, 1), at(&one.moved, 8));
    let (b1, b8) = (at(&four.moved, 1), at(&four.moved, 8));
    eprintln!(
        "camera shift: one block {a1} px at t>=1 / {a8} at t>=8; four blocks {b1} / {b8}; the \
         excess the four-block shift adds is {} px at t>=1 and {} px at t>=8",
        b1.saturating_sub(a1),
        b8.saturating_sub(a8)
    );
    assert_eq!(
        (a1, a8, b1, b8),
        (0, 0, 0, 0),
        "a round trip still moves pixels: one block {a1} px at t>=1 and {a8} at t>=8, four blocks \
         {b1} and {b8}. The haze is a function of the size of the shift, so an excess that is \
         present at four blocks and absent at one is the camera's low bits and not the body"
    );
}
