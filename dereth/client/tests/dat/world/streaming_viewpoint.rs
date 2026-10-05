//! The landblock window re-centres on the swept viewer, not the debug chase camera. Retail's
//! normal-mode rendering reads one viewer value, written from the swept sphere just before, and
//! outdoors hands its cell id to the landscape viewpoint update, which takes both the block (the
//! high 16 bits changed) and the draw-order cell from that one id. Here that value is
//! `Character::camera.viewer`; the chase camera (`WorldState::follow_character`, 4.5 m behind the
//! body along its heading, through walls) never reaches a drawn frame with a body, so it must not
//! drive `recenter()` or `update_viewer_cell()` either. Legs of opposite polarity cover the block,
//! including the teleport frame before any sweep; stations on two axes cover the cell; a real walk
//! at the shipped distance shows the two sources disagree. Fixture: the retail dats and a body in
//! Holtburg, in the world the runtime simulates with no device: the window's re-centre, the viewer
//! cell and the swept camera are the simulation's, and nothing here reads a pixel. Fails without
//! the dats.

use std::sync::Arc;

use crate::common::sim::SimWorld;

use dereth_client_runtime::camera::CameraInput;
use dereth_client_runtime::character::CharacterInput;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{Frame, LandblockId, LocalTime, Position, Quat, Vec3};
use {
    dereth_client_runtime::landblock::block_xy,
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_client_runtime::scene::SceneConfig,
};

/// `BLOCK_LENGTH`.
const BLOCK: f32 = 192.0;
/// `CELL_SIZE`: the side of one of a landblock's 8x8 cells.
const CELL: f32 = 24.0;

/// Far enough behind the body that the chase camera lands in a different landblock from the swept
/// viewer by construction rather than by luck. It is a debug knob with no production writer (see
/// `WorldState::set_camera_distance`), and nothing drawn depends on it once
/// `crate::camera::update_viewer` has run, which is exactly what makes it usable as a lever here.
const FAR_CHASE: f32 = 60.0;

/// The retail dats, or a failed test: a skipped test would read as a pass.
fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
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

/// The default scene with its body.
fn embodied(store: &Arc<RetailDatStore>) -> SimWorld {
    SimWorld::load(store, SceneConfig::default())
}

/// Update the scene, then sweep the viewer, in `App::frame`'s order.
fn app_frame(scene: &mut SimWorld, now: f64, dt: f32) {
    scene.update(
        CameraInput::default(),
        CharacterInput::default(),
        LocalTime(now),
        dt,
    );
    scene.update_viewer(CameraInput::default(), LocalTime(now), f64::from(dt));
}

/// Put the body at `(x, y)` of `block` at heading `yaw` (0 is north), and run the sweep so that
/// `Character::camera.viewer` holds a real swept-viewer answer rather than the pivot snapshot left
/// by teleport initialization.
///
/// The window is **not** re-centred here: no scene `update` runs, which is what leaves the
/// frame under test as the single variable.
fn place(scene: &mut SimWorld, block: LandblockId, x: f32, y: f32, yaw: f32) {
    teleport_to(scene, block, x, y, yaw);
    // Teleport initialization snaps both `viewer` and the smoother's `sought` onto the pivot, so
    // one sweep would leave `viewer` sitting exactly on the body and this module's "swept viewer"
    // would be the body wearing another name. Fifteen viewer updates, with nothing else between
    // them, let the smoother pull the eye back to its configured offset and the sweep place it.
    for i in 1..=15 {
        scene.update_viewer(
            CameraInput::default(),
            LocalTime(f64::from(i) / 30.0),
            1.0 / 30.0,
        );
    }
}

/// The teleport alone, with **no sweep after it**: retail's world entry is followed by a viewer
/// reset that leaves `viewer` on the pivot and `viewer_cell` **NULL**. That is a state the client
/// enters on every teleport frame, and retail's normal-mode rendering still takes its viewpoint
/// from `viewer` there.
fn teleport_to(scene: &mut SimWorld, block: LandblockId, x: f32, y: f32, yaw: f32) {
    let land = Arc::clone(scene.ws().character.as_ref().expect("a body").land());
    let z = land.ground_height(block, x, y).unwrap_or(0.0) + 1.0;
    {
        // Yaw about the client's +z, so heading extraction (and therefore `follow_character`,
        // which negates it) sees the body turn.
        let half = yaw * 0.5;
        let c = scene.ws_mut().character.as_mut().expect("a body");
        c.teleport(Position::new(
            block.cell(1),
            Frame::new(
                Vec3::new(x, y, z),
                Quat::new(math::cosf(half), 0.0, 0.0, math::sinf(half)),
            ),
        ));
    }
}

/// The swept viewer in renderer viewer-block-relative space, computed here from the two public
/// parts rather than through `Character::viewer_render_frame`, so this module's oracle is not the
/// accessor the code under test uses.
fn swept(scene: &SimWorld) -> Vec3 {
    let c = scene.ws().character.as_ref().expect("a body");
    c.render_frame_of(c.camera.viewer).origin
}

/// Where `WorldState::follow_character` puts the debug chase camera, in the same space, read out
/// of the world's `camera` after asking for it. This is the wrong source `recenter` must not be
/// handed.
fn chase(scene: &mut SimWorld) -> Vec3 {
    scene.ws_mut().follow_character_now();
    scene.ws().camera.position
}

fn block_of(p: Vec3) -> (i32, i32) {
    (
        dereth_primitives::num::floor_to_i32(p.x / BLOCK),
        dereth_primitives::num::floor_to_i32(p.y / BLOCK),
    )
}

/// Which cell of the centre block a **position** falls in.
///
/// This is the right question to ask of the chase camera, which is a bare `Vec3` with no cell id
/// of its own, and it is the question this module's premise assertions ask. It is **not** the
/// derivation retail's landscape viewpoint update uses: see [`cell_of_swept`].
fn cell_of(p: Vec3) -> (i32, i32) {
    (
        dereth_primitives::num::floor_to_i32(p.x / CELL).clamp(0, 7),
        dereth_primitives::num::floor_to_i32(p.y / CELL).clamp(0, 7),
    )
}

/// The draw-order cell handed to `cell_draw_order` for the swept viewer: the low three bits of the
/// x and y coordinates obtained from the viewer's cell **id**. For an indoor viewer, retail first
/// derives the corresponding outside cell id.
///
/// # Why the id and not the position
///
/// The viewer's *position* divided by 24 and clamped (`cell_of(swept(scene))`) is not the same
/// answer. On the `north` station the swept viewer sits at `x = 96.000`, exactly on the `4 * 24`
/// cell boundary: its position says cell x 4, its own `objcell_id` says **3**. The id comes from
/// the transition's sphere-path position, and the cell search keeps the **last** land cell in the
/// array that contains the point. On a cell edge both neighbours contain it, because the polygon
/// test rejects only when `v > 0`, and boundary-cell construction appends the `-1` neighbour after
/// the `floor` cell, so the id is the cell below the line. Retail's normal-mode rendering passes
/// that id, so the id is the answer. It is a pure function of the position, the block its origin
/// is expressed in and the sphere's radius (the physics boundary-cell tests cover it).
fn cell_of_swept(scene: &SimWorld) -> (i32, i32) {
    use dereth_physics::landdefs as ld;
    let c = scene.ws().character.as_ref().expect("a body");
    let pos = c.camera.viewer;
    let id = if ld::is_outdoors(pos.cell) {
        pos.cell
    } else {
        ld::get_outside_cell_id(pos.cell, pos.frame.origin)
    };
    let (x, y) = ld::gid_to_lcoord(id).expect("the swept viewer has a land cell");
    (x & 7, y & 7)
}

// ---------------------------------------------------------------------------------------------
// 1. The block half of the landscape viewpoint update
// ---------------------------------------------------------------------------------------------

/// One leg: place the body, check the two candidate viewpoints disagree about the block, run the
/// frame, and see which one the window went with.
struct Leg {
    /// The two candidate viewpoints, and the block each of them names.
    swept: (Vec3, (i32, i32)),
    chase: (Vec3, (i32, i32)),
    /// The viewer block before and after the frame under test.
    viewer_block: ((i32, i32), (i32, i32)),
}

fn leg(
    store: &Arc<RetailDatStore>,
    (bdx, bdy): (i32, i32),
    (x, y): (f32, f32),
    sweep: bool,
) -> Leg {
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let mut scene = embodied(store);

    // Settle at home first, so the sweep, the smoother and the window all agree before anything
    // is arranged.
    place(&mut scene, block_at(hx, hy), 96.0, 96.0, 0.0);
    for i in 1..=10 {
        app_frame(&mut scene, f64::from(i) / 30.0, 1.0 / 30.0);
        scene.stream();
    }

    scene.ws_mut().set_camera_distance(FAR_CHASE);
    let target = block_at(hx + bdx, hy + bdy);
    if sweep {
        place(&mut scene, target, x, y, 0.0);
    } else {
        teleport_to(&mut scene, target, x, y, 0.0);
    }
    assert_eq!(
        scene
            .ws()
            .character
            .as_ref()
            .expect("a body")
            .camera
            .viewer_cell
            .is_some(),
        sweep,
        "the fixture did not produce the `viewer_cell` state it meant to"
    );

    let s = swept(&scene);
    let c = chase(&mut scene);
    let before = scene.ws().viewer_block().expect("a viewer block");

    scene.update(
        CameraInput::default(),
        CharacterInput::default(),
        LocalTime(0.0),
        0.0,
    );
    let after = scene.ws().viewer_block().expect("a viewer block");

    Leg {
        swept: (s, block_of(s)),
        chase: (c, block_of(c)),
        viewer_block: (before, after),
    }
}

/// Behaviour: world.streaming.the-window-recentres-on-the-swept-viewer
///
/// **The window re-centres on the block containing the swept viewer.**
///
/// Two legs of opposite polarity, so that neither direction of the wiring can pass by doing
/// nothing:
///
/// * `stay`: the body 24 m inside the home block facing north. The swept viewer is ~3 m behind it
///   and stays in the block; the chase camera is 60 m behind it and is in the block to the south.
///   The correct answer is **no shift**, and the chase camera would have shifted `(0, -1)`.
/// * `move`: the body 24 m into the block to the north. The swept viewer is in that block; the
///   chase camera is 60 m behind and still in the home block. The correct answer is **shift
///   `(0, 1)`**, and the chase camera would not have shifted at all.
/// * `move-no-sweep`: the same arrangement, but the frame under test is the **teleport frame**:
///   teleport initialization has put `viewer` on the pivot and cleared `viewer_cell` to NULL, and
///   no sweep has run since. Retail's normal-mode rendering still takes the viewpoint from
///   `viewer` in that state (its two fallback writes use the *body's* position and clear
///   `viewer_cell` on the next line), so the block decision must still follow it. This leg pins
///   `Character::viewer_render_frame`'s guard to `attached` rather than to `viewer_cell`;
///   without it, either gate passes.
#[test]
fn the_block_decision_follows_the_swept_viewer() {
    let store = store();

    for (name, legs, at, want, sweep) in [
        ("stay", (0, 0), (96.0f32, 24.0f32), (0, 0), true),
        ("move", (0, 1), (96.0f32, 24.0f32), (0, 1), true),
        ("move-no-sweep", (0, 1), (96.0f32, 24.0f32), (0, 1), false),
    ] {
        let l = leg(&store, legs, at, sweep);
        let shift = (
            l.viewer_block.1 .0 - l.viewer_block.0 .0,
            l.viewer_block.1 .1 - l.viewer_block.0 .1,
        );

        eprintln!(
            "viewpoint {name}: swept {:?} -> block {:?}; chase {:?} -> block {:?}; window {:?} \
             -> {:?} (shift {shift:?})",
            l.swept.0, l.swept.1, l.chase.0, l.chase.1, l.viewer_block.0, l.viewer_block.1
        );

        // **The premise.** Without a disagreement this frame says nothing about which source was
        // read, and a green run would mean only that the two happened to agree.
        assert_ne!(
            l.swept.1, l.chase.1,
            "the swept viewer and the chase camera are both in block {:?} on the {name} leg, so \
             this frame cannot tell the two sources apart",
            l.swept.1
        );

        assert_eq!(
            shift, want,
            "the {name} leg re-centred by {shift:?}; the swept viewer asked for {:?} and the \
             chase camera for {:?}, so the block decision is being taken from the chase camera",
            l.swept.1, l.chase.1
        );
        // The same statement said the other way round, against the value rather than the delta:
        // after the frame, the swept viewer stands in the window's own centre block.
        assert_eq!(
            l.swept.1, want,
            "the {name} leg's fixture is wrong: the swept viewer names block {:?}, not {want:?}",
            l.swept.1
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The cell half of the landscape viewpoint update
// ---------------------------------------------------------------------------------------------

/// **`update_viewer_cell`, `cell_draw_order`'s input, comes from the same swept viewer.**
///
/// Asserted on its own value rather than through the block, because it is a different consumer
/// with a different granularity: 24 m rather than 192 m, so the two sources part company far more
/// often here than they do over a landblock.
#[test]
fn the_viewer_cell_follows_the_swept_viewer() {
    let store = store();
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);

    // Two stations, so the disagreement is exercised on each axis: facing north puts the chase
    // camera due south of the body, and the second station is rotated by placing the body where
    // the y disagreement is largest while x agrees, then the reverse via a quarter turn.
    for (name, x, y, yaw) in [
        ("north", 96.0f32, 100.0f32, 0.0f32),
        ("west", 100.0f32, 96.0f32, std::f32::consts::FRAC_PI_2),
    ] {
        let mut scene = embodied(&store);
        place(&mut scene, block_at(hx, hy), 96.0, 96.0, 0.0);
        for i in 1..=10 {
            app_frame(&mut scene, f64::from(i) / 30.0, 1.0 / 30.0);
            scene.stream();
        }

        scene.ws_mut().set_camera_distance(40.0);
        place(&mut scene, block_at(hx, hy), x, y, yaw);

        let s = swept(&scene);
        let c = chase(&mut scene);
        scene.update(
            CameraInput::default(),
            CharacterInput::default(),
            LocalTime(0.0),
            0.0,
        );
        let got = scene.ws().viewer_draw_cell();

        eprintln!(
            "viewpoint cell {name}: swept {:?} -> cell {:?}; chase {:?} -> cell {:?}; scene says \
             {got:?}",
            s,
            cell_of(s),
            c,
            cell_of(c)
        );

        // The premise, again: the two sources must name different cells or this proves nothing.
        assert_ne!(
            cell_of(s),
            cell_of(c),
            "both viewpoints are in cell {:?} on the {name} station, so this frame cannot tell \
             the two sources apart",
            cell_of(s)
        );

        // The swept viewer's own cell **id**, which is what retail's normal-mode rendering passes
        // and what `cell_of_swept` explains. On this station it differs from `cell_of(s)`.
        let (cx, cy) = cell_of_swept(&scene);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: `& 7` bounds both to 0..=7.
        let want = (cx as u8, cy as u8);
        assert_eq!(
            got,
            want,
            "`update_viewer_cell` answered {got:?}; the swept viewer's own cell id is {want:?} \
             and the chase camera is in {:?}",
            cell_of(c)
        );
        // And the module's actual claim, stated against the source rather than through the value:
        // whatever the scene answered, it is not the chase camera's cell.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: `cell_of` clamps both to 0..=7.
        let chase_cell = (cell_of(c).0 as u8, cell_of(c).1 as u8);
        assert_ne!(
            got, chase_cell,
            "`update_viewer_cell` answered the chase camera's cell {chase_cell:?} on the {name} \
             station, so the cell half is being taken from the debug camera"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 3. Does it matter on the shipped path?
// ---------------------------------------------------------------------------------------------

/// **The two sources disagree on a plain walk, at the shipped `camera_distance`.**
///
/// The first two tests exaggerate the chase distance so the disagreement is deterministic. This
/// one does not touch it: the body runs north across a landblock boundary at the default 4.5 m,
/// and every frame both candidate viewpoints are read at the same instant, immediately after
/// the scene's `update`, where `place_local_body` has just left the chase camera in `camera`
/// and `Character::camera.viewer` still holds the sweep the previous frame produced. Those are
/// exactly the two values the block decision could have been taken from.
///
/// A **null** here would need a mechanism: it would mean the wrong source is harmless on the
/// shipped path. The two viewpoints are a planar offset of order `camera_distance -
/// viewer_offset` apart, so they name different cells whenever a 24 m cell boundary falls inside
/// that gap, and different blocks whenever a 192 m one does, which is every crossing.
#[test]
fn the_two_sources_disagree_on_the_shipped_path() {
    let store = store();
    let (hx, hy) = block_xy(DEFAULT_LANDBLOCK);
    let mut scene = embodied(&store);

    // Six metres short of the northern boundary of the home block, facing north.
    place(&mut scene, block_at(hx, hy), 96.0, 186.0, 0.0);
    for i in 1..=10 {
        app_frame(&mut scene, f64::from(i) / 30.0, 1.0 / 30.0);
        scene.stream();
    }

    let start = scene
        .ws()
        .character
        .as_ref()
        .expect("a body")
        .position()
        .frame
        .origin;
    let start_block = scene.ws().viewer_block().expect("a viewer block");

    let mut frames = 0usize;
    let mut block_disagreements = 0usize;
    let mut cell_disagreements = 0usize;
    let mut max_gap = 0.0f32;
    let mut crossings = 0usize;
    let mut last_block = start_block;

    let dt = 1.0f32 / 30.0;
    // Twenty seconds of running at 30 Hz: long enough to cross one landblock boundary and
    // several 24 m cell boundaries, which is what makes the two counts below distinguishable.
    for i in 1..=600u32 {
        let now = f64::from(i + 10) / 30.0;
        // Through the scene's `update`, which is the only writer of `Character::input`: setting
        // the field directly is overwritten by this argument on the next line.
        let running = CharacterInput {
            forward: true,
            run: true,
            ..CharacterInput::default()
        };
        scene.update(CameraInput::default(), running, LocalTime(now), dt);

        // Both candidates, at the same instant and in the same space.
        let s = swept(&scene);
        let c = scene.ws().camera.position;
        frames += 1;
        if block_of(s) != block_of(c) {
            block_disagreements += 1;
        }
        if cell_of(s) != cell_of(c) {
            cell_disagreements += 1;
        }
        max_gap = max_gap.max(math::hypotf(s.x - c.x, s.y - c.y));

        scene.update_viewer(CameraInput::default(), LocalTime(now), f64::from(dt));
        scene.stream();

        let b = scene.ws().viewer_block().expect("a viewer block");
        if b != last_block {
            crossings += 1;
            last_block = b;
        }
    }

    let end = scene
        .ws()
        .character
        .as_ref()
        .expect("a body")
        .position()
        .frame
        .origin;

    eprintln!(
        "viewpoint walk: {frames} frames, body {:?} -> {:?}, window {:?} -> {:?} ({crossings} \
         crossing(s)); the swept viewer and the chase camera name different blocks on {} frame(s) \
         and different cells on {} frame(s); largest planar gap {:.3} m",
        start, end, start_block, last_block, block_disagreements, cell_disagreements, max_gap
    );

    // The premise: the body actually walked across a landblock boundary. Two frames of a walk
    // that never happened agree perfectly (the same premise `world::landblock_recentre` asserts).
    assert!(
        crossings >= 1,
        "the body never crossed a landblock boundary in {frames} frames ({:?} -> {:?}), so this \
         measurement is about a walk that did not happen",
        start,
        end
    );

    // And the claim. Both counts are over the same {frames} frames; the thresholds underneath
    // them are the 192 m block grid and the 24 m cell grid, and there is no epsilon.
    assert!(
        cell_disagreements > 0,
        "the two viewpoints named the same cell on all {frames} frames, largest gap {max_gap:.3} \
         m: a null needs a mechanism, and this one would say the chase camera is a harmless \
         stand-in on the shipped path"
    );
    assert!(
        block_disagreements > 0,
        "the two viewpoints named the same block on all {frames} frames despite {crossings} \
         crossing(s), largest gap {max_gap:.3} m"
    );
}
