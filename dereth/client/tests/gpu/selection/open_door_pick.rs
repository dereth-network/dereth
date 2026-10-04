//! An **open** door stops swallowing the pick because its leaves swing out of the doorway, not
//! because it turns ethereal; a closed door, and a swung-open leaf clicked where it now stands,
//! still name the door. Fixture: the retail door setup `0x020005DA` and motion table `0x09000086`
//! played through the shipped motion table (no device), and a headless `App` over the recorded
//! training-academy door of `early-inventory-and-casting`, opened and closed with its own bytes.
//!
//! # Picking never examines the ethereal flag
//!
//! The ethereal setter changes physics state mask `0x4` and clears the separate check-ethereal
//! transient bit. The only two readers of that physics-state bit initialize collision information
//! and track object collisions; both are collision paths.
//!
//! The draw-and-pick path instead rejects only parts marked `NoDraw`. It makes a part eligible for
//! selection when its owning object has a nonzero id or creature mode applies, records the current
//! physics part, draws its mesh, then tests that part only when selection and current-object checks
//! are enabled. The ray intersects the drawing sphere and then the drawing polygons.
//!
//! ## Which geometry the ray walks
//!
//! Both tests use the **drawing** geometry of the graphics object in the transform already pushed
//! for that part. A drawing-sphere miss returns immediately; otherwise each drawing polygon is
//! tested, with no result when the polygon list is empty. There is no cell, no physics BSP and no
//! cached pose in the test: the part's own frame was pushed before the call. So "which geometry"
//! and "which pose" are two separate questions (`WorldPicker::gfx_obj_pick_geometry` reads the
//! drawing polygons and the same `drawing_sphere` fork `dereth_physics::source::SetupPart` uses;
//! `PickScene::object_part_frames` supplies the live per-part frames `WorldScene` publishes).
//!
//! So a **closed** door is pickable (it is drawn and its object id is non-zero) and so is an
//! **open** one, which is how you close a door in retail. What changes is **where its geometry
//! is**:
//!
//! Each part update uses the current animation frame when an animation is active, combines that
//! per-part frame with the object's world frame and scale, and leaves parts where they were when
//! there is no current frame. Setup placement frames are the fallback only while no animation is
//! active; the collision body follows the same rule. The retail door's open animation swings its
//! two leaves clean out of the doorway, and that is the whole of why retail's ray stops hitting
//! them. [`the_retail_open_animation_swings_the_door_leaves_out_of_the_doorway`] measures that
//! swing out of `client_portal.dat` rather than asserting it.
//!
//! # The fixture
//!
//! The recorded **training-academy door** of `early-inventory-and-casting`: `0x77F03033`, name
//! `"Door"`, setup `0x020005DA`, motion table `0x09000086`, standing in doorway cell `0x7F0301B4`,
//! opened with its own recorded bytes: row 148 (`0xF74C`, an interpreted `MotionCommand::ON`) and
//! row 149 (`0xF74B`, state `0x0001001C` = `0x00010018 | ETHEREAL_PS`). The **close** is row 148's
//! own buffer with its command index swapped to `MotionCommand::OFF` and its stamps advanced, which
//! is what the server sends. Row 147, the *player's* `TurnToObject`, is deliberately **not** fed:
//! the camera must not move between the picks, or the two arms would not be the same pick.
//!
//! No socket, no server, no GPU beyond the headless device `App` already needs.

use std::sync::Arc;

use dereth_animation::{AnimAssets, AnimEvent, MotionCommand, MotionDriver};
use dereth_client::anim_assets::DatAnimAssets;
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, Frame, LocalTime, Quat, Vec3};

/// `early-inventory-and-casting` row 52: the recorded academy door's setup.
const DOOR_SETUP: u32 = 0x0200_05DA;
/// ...and its recorded motion-table data ID. Both are asserted against the recording in
/// [`the_recorded_door_is_the_one_these_stations_animate`].
const DOOR_MTABLE: u32 = 0x0900_0086;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn sum_abs(a: &Frame, b: &Frame) -> f32 {
    (a.origin.x - b.origin.x).abs()
        + (a.origin.y - b.origin.y).abs()
        + (a.origin.z - b.origin.z).abs()
}

/// The retail door's own animation, played through the shipped motion table: which parts move,
/// how far, and **when** the ethereal hook fires relative to the swing.
///
/// This is the oracle the pick's rule rests on. It asserts three separate things:
///
/// 1. the door has parts that **do not** move (the two jambs and the lintel) and parts that do
///    (the two leaves), so "the whole object moved" cannot pass for "the leaves swung";
/// 2. each leaf leaves the doorway plane by more than a metre — far more than its own thickness,
///    so a ray through the opening cannot still be crossing it;
/// 3. `SetEthereal(true)` fires **near the start** of the swing, not at its end. The bit flips at
///    the hook, so a pick that followed the bit would clear the doorway while the leaves were
///    still across it. The geometry is the pick's rule; the bit is collision's.
#[test]
fn the_retail_open_animation_swings_the_door_leaves_out_of_the_doorway() {
    let store = store();
    let assets: Arc<dyn AnimAssets> = Arc::new(DatAnimAssets::new(Arc::clone(&store)));
    let setup = AnimAssets::setup(assets.as_ref(), DataId(DOOR_SETUP)).expect("the door setup");
    let mut d = MotionDriver::new(Arc::clone(&assets));
    assert!(
        d.set_setup(Arc::clone(&setup)),
        "part-array setup creation refused the door setup"
    );
    assert!(
        d.set_motion_table(DataId(DOOR_MTABLE)),
        "the recorded door motion table loads"
    );

    let world = Frame::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY);
    d.update_parts(&world);
    let rest: Vec<Frame> = d.part_array.parts.iter().map(|p| p.pos).collect();
    assert_eq!(
        rest.len(),
        5,
        "the retail door setup has five parts (two jambs, a lintel, two leaves)"
    );

    d.do_interpreted_motion(
        MotionCommand::ON,
        &dereth_animation::motion::MovementParameters::default(),
    );
    let mut t = 0.0;
    let mut ethereal_step: Option<usize> = None;
    let mut settled_step: Option<usize> = None;
    let mut last = 0.0f32;
    for step in 0..120 {
        t += 1.0 / 30.0;
        dereth_physics::MotionSource::advance(&mut d, 1.0 / 30.0);
        dereth_physics::MotionSource::tick_movement(&mut d, LocalTime(t));
        dereth_physics::MotionSource::process_hooks(&mut d);
        for e in d.take_events() {
            if matches!(e, AnimEvent::SetEthereal(true)) && ethereal_step.is_none() {
                ethereal_step = Some(step);
            }
        }
        d.update_parts(&world);
        let moved: f32 = d
            .part_array
            .parts
            .iter()
            .zip(&rest)
            .map(|(p, r)| sum_abs(&p.pos, r))
            .sum();
        if settled_step.is_none() && step > 0 && (moved - last).abs() < 1e-4 && moved > 1.0 {
            settled_step = Some(step);
        }
        last = moved;
    }
    let open: Vec<Frame> = d.part_array.parts.iter().map(|p| p.pos).collect();
    let travel: Vec<f32> = open.iter().zip(&rest).map(|(o, r)| sum_abs(o, r)).collect();
    eprintln!(
        "door animation: per-part travel {travel:?}; ethereal hook at step \
         {ethereal_step:?}, swing settled at step {settled_step:?}"
    );
    for (i, f) in open.iter().enumerate() {
        eprintln!(
            "  part {i}: rest ({:.3}, {:.3}, {:.3}) -> open ({:.3}, {:.3}, {:.3}), q.w {:.3} -> \
             {:.3}",
            rest[i].origin.x,
            rest[i].origin.y,
            rest[i].origin.z,
            f.origin.x,
            f.origin.y,
            f.origin.z,
            rest[i].rotation.w,
            f.rotation.w,
        );
    }

    let movers: Vec<usize> = travel
        .iter()
        .enumerate()
        .filter(|(_, t)| **t > 0.5)
        .map(|(i, _)| i)
        .collect();
    let still: Vec<usize> = travel
        .iter()
        .enumerate()
        .filter(|(_, t)| **t < 1e-3)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        movers,
        vec![3, 4],
        "the two door leaves are the parts that swing: travel {travel:?}"
    );
    assert_eq!(
        still,
        vec![0, 1, 2],
        "the jambs and the lintel do not move: travel {travel:?}"
    );
    for i in movers {
        let d2 = ((open[i].origin.x - rest[i].origin.x).powi(2)
            + (open[i].origin.y - rest[i].origin.y).powi(2))
        .sqrt();
        assert!(
            d2 > 1.0,
            "leaf {i} only travelled {d2:.3} m in the doorway plane; retail swings it clear"
        );
    }
    let settled = settled_step.expect("the swing reaches a final pose within four seconds");
    let hook = ethereal_step.expect("the retail ON animation raises SetEthereal(true)");
    assert!(
        hook * 4 < settled,
        "the intangible-state hook fired at step {hook} of a swing that settles at step {settled}: \
         a hook that fired at the END would make following the bit and following the geometry \
         indistinguishable"
    );

    // ...and OFF brings the leaves back, so the pick station's closing arm has an oracle too.
    d.do_interpreted_motion(
        MotionCommand::OFF,
        &dereth_animation::motion::MovementParameters::default(),
    );
    for _ in 0..120 {
        t += 1.0 / 30.0;
        dereth_physics::MotionSource::advance(&mut d, 1.0 / 30.0);
        dereth_physics::MotionSource::tick_movement(&mut d, LocalTime(t));
        dereth_physics::MotionSource::process_hooks(&mut d);
        d.update_parts(&world);
    }
    let closed: Vec<f32> = d
        .part_array
        .parts
        .iter()
        .zip(&rest)
        .map(|(p, r)| sum_abs(&p.pos, r))
        .collect();
    eprintln!("door animation: per-part travel after OFF {closed:?}");
    for (i, c) in closed.iter().enumerate() {
        assert!(
            *c < 0.05,
            "part {i} did not come back to the closed pose: {c:.4}"
        );
    }
}

/// The recording the App stations replay, asserted before anything is built on it.
#[test]
fn the_recorded_door_is_the_one_these_stations_animate() {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_primitives::ObjectId;
    use dereth_protocol::objects::{ItemCreateObject, ItemSetState};
    use dereth_protocol::Message;

    let rows = &Corpus::shared("early-inventory-and-casting").blobs;
    let row = |i: usize| rows.iter().find(|r| r.idx == i).expect("recorded station");
    let create = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row(52).payload[4..]))
        .unwrap()
        .0;
    assert_eq!(create.id, ObjectId(0x77f0_3033));
    assert_eq!(create.wdesc.name, "Door");
    assert_eq!(create.physicsdesc.setup_id, Some(DOOR_SETUP));
    assert_eq!(create.physicsdesc.mtable_id, Some(DOOR_MTABLE));
    assert_eq!(create.physicsdesc.state, 0x0001_0018);
    assert_eq!(create.physicsdesc.position.unwrap().objcell_id, 0x7f03_01b4);

    // Row 148 is the door's own `0xF74C`: an interpreted motion whose forward command is `ON`.
    let m = dereth_protocol::movement::MovementSetObjectMovement::read(
        &mut dereth_protocol::Reader::new(&row(148).payload[4..]),
    )
    .unwrap();
    assert_eq!(
        m.id, create.id,
        "row 148 is addressed to the door, not to the player"
    );
    let buffer = m.decoded_movement().unwrap();
    let interp = buffer
        .body
        .interpreted
        .clone()
        .expect("an interpreted motion state");
    assert_eq!(
        interp.forward_command,
        MotionCommand::ON.to_index(),
        "the recorded door movement plays `MotionCommand::ON`"
    );
    assert!(!buffer.autonomous);

    // Row 149 is the door's `0xF74B`: the create's state with `ETHEREAL_PS` set.
    let s = ItemSetState::read(&mut dereth_protocol::Reader::new(&row(149).payload[4..])).unwrap();
    assert_eq!(s.id, create.id);
    assert_eq!(s.state, 0x0001_001c);
    assert_eq!(
        s.state ^ create.physicsdesc.state,
        0x0000_0004,
        "the only bit the open adds is ETHEREAL_PS"
    );
    assert_eq!(row(149).dir, Direction::ServerToClient);
}

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
mod in_the_doorway {
    use crate::common::app::{frames, position};
    use dereth_animation::MotionCommand;
    use dereth_client::app::App;
    use dereth_client::config::Config;
    use dereth_client::pick::PickScene;
    use dereth_client::world::SceneConfig;
    use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_client_runtime::pick_geometry::selection_ray;
    use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
    use dereth_protocol::objects::{ItemCreateObject, ItemSetState};
    use dereth_protocol::types::PhysicsEventStamp;
    use dereth_protocol::{Message, Opcode};
    use dereth_ui::{ElemHandle, UiDrawCmd, UiSystem};

    const SCREEN: (u32, u32) = (800, 600);

    const ACADEMY: u16 = 0x7f03;
    const DOOR: ObjectId = ObjectId(0x77f0_3033);
    /// The doorway cell the recorded door stands in.
    const DOOR_CELL: u32 = 0x7f03_01b4;
    /// The state word that takes a teleported body out of hiding, so the local player is a drawn,
    /// pickable object like anyone else.
    const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;
    /// A fresh id for the object placed **behind** the door.
    const BEHIND: ObjectId = ObjectId(0x7de0_0001);

    /// The frames a station needs before it may believe a pick: `Character::teleport` leaves the
    /// viewer on the pivot, and the chase camera has to walk the eye out of the body first.
    const PICK_SETTLE: usize = 90;
    /// The frames the whole open/close swing needs to finish (it settles at ~29 of 30/s).
    const SWING: usize = 120;

    /// One recorded server blob into [`dereth_client::objects::ObjectStream`], and **only** there.
    ///
    /// Deliberately not `App::apply_hud_events`: the recorded login's `0xF746` raises the login
    /// tunnel, a full-screen UI element that covers `<SBOX>` for ~170 headless frames and therefore
    /// refuses every hover (`PickStats { requests: 0 }`, with the aim pixel resolving to
    /// `ElementId(0x3D)` instead of `<SBOX>`). This bench wants the recorded *door*, not the
    /// recorded login.
    fn feed(app: &mut App, row: &CorpusBlob, at: f64) {
        assert_eq!(row.dir, Direction::ServerToClient);
        app.objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode(row.opcode),
                body: row.payload[4..].to_vec(),
            },
            LocalTime(at),
        );
    }

    fn yaw_quat(yaw: f32) -> Quat {
        let half = yaw * 0.5;
        Quat::new(
            dereth_primitives::num::math::cosf(half),
            0.0,
            0.0,
            dereth_primitives::num::math::sinf(half),
        )
    }

    #[allow(clippy::cast_precision_loss)]
    fn pixel_of(local: Vec3, fov_y_rad: f32) -> (f32, f32) {
        let half_w = (SCREEN.0 as f32 - 1.0) * 0.5;
        let half_h = (SCREEN.1 as f32 - 1.0) * 0.5;
        let vdst = half_h / dereth_primitives::num::math::tanf(fov_y_rad * 0.5);
        let k = vdst / local.y;
        (half_w + local.x * k, half_h - local.z * k)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn aim(app: &App, world: Vec3) -> (i32, i32) {
        let scene = app.world_scene().unwrap();
        let viewer = PickScene::viewer(&scene);
        let fov = PickScene::fov_y_rad(&scene, SCREEN);
        let local = dereth_physics::math::globaltolocal(&viewer, world);
        assert!(
            local.y > 0.5,
            "the aim point is in front of the chase camera: {local:?}"
        );
        let (px, py) = pixel_of(local, fov);
        let ray = selection_ray(&viewer, px, py, SCREEN, fov);
        let to = Vec3::new(
            world.x - viewer.origin.x,
            world.y - viewer.origin.y,
            world.z - viewer.origin.z,
        );
        let len = (to.x * to.x + to.y * to.y + to.z * to.z).sqrt();
        let cos = (ray.x * to.x + ray.y * to.y + ray.z * to.z) / len;
        assert!(
            cos > 0.9999,
            "the pick ray through ({px}, {py}) points at the point: cos = {cos}"
        );
        (px.round() as i32, py.round() as i32)
    }

    /// [`aim`] without the assertions: `None` when the point is behind the chase camera or its
    /// pixel falls outside the window, so a station can say "this one was not on screen" rather
    /// than panic on a fixture fact it does not control.
    #[allow(clippy::cast_possible_truncation)]
    fn aim_if_visible(app: &App, world: Vec3) -> Option<(i32, i32)> {
        let scene = app.world_scene().unwrap();
        let viewer = PickScene::viewer(&scene);
        let fov = PickScene::fov_y_rad(&scene, SCREEN);
        let local = dereth_physics::math::globaltolocal(&viewer, world);
        if local.y <= 0.5 {
            return None;
        }
        let (px, py) = pixel_of(local, fov);
        #[allow(clippy::cast_precision_loss)]
        if !(0.0..SCREEN.0 as f32).contains(&px) || !(0.0..SCREEN.1 as f32).contains(&py) {
            return None;
        }
        Some((px.round() as i32, py.round() as i32))
    }

    fn move_pointer_to(app: &mut App, x: i32, y: i32, at: u32) {
        let mut pump = dereth_client::pump::Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        let message = pump.mouse_move_message(f64::from(x), f64::from(y), at);
        pump.dispatch(message);
        app.input_manager_mut()
            .expect("real input maps")
            .on_message(message);
    }

    fn ui_of(app: &mut App) -> &mut UiSystem {
        &mut app.ui_mut().expect("the shell is up").ui
    }

    /// The element the live screen hit-tests to at `(x, y)`, for the log line.
    ///
    /// Reported rather than asserted: the gate the pick arm actually reads is
    /// `Interaction::pointer_over_game_view` plus the world-object lookup's unsigned viewport
    /// comparison, and *that* is what [`pick_was_armed`] checks. A working run prints `<SBOX>`'s
    /// `0x1000049A` here; anything else means a UI element covers the world view.
    fn element_under(app: &mut App, x: i32, y: i32) -> Option<dereth_ui::ElementId> {
        let hit = ui_of(app).hit_test_screen(x, y)?;
        ui_of(app)
            .node(hit)
            .map(dereth_ui::element::ElementNode::element_id)
    }

    /// `(pick.requests, refused-by-the-HUD, refused-by-the-rectangle)`, for a before/after pair.
    ///
    /// The absolute counters cannot be the premise: every frame the station runs with the pointer
    /// still parked at its initial `(0, 0)` is a hover under the HUD, and those are hundreds.
    fn pick_counters(app: &App) -> (u64, u64, u64) {
        (
            app.interaction().pick.stats.requests,
            app.interaction().stats.hover_searches_under_the_hud,
            app.interaction().stats.hover_searches_not_armed,
        )
    }

    /// The current world-object lookup was armed on essentially every frame of this hover, and
    /// the viewport rectangle refused none of them.
    ///
    /// `hover_searches_under_the_hud` is allowed **one**: `Interaction::pointer_over_game_view` is
    /// last frame's answer, so the frame on which the pointer arrives is still measured against
    /// where it was. That is the client's own one-frame lag and not a refusal of this pixel; the 19
    /// that follow it are the measurement.
    fn pick_was_armed(before: (u64, u64, u64), after: (u64, u64, u64)) {
        assert!(
            after.0 >= before.0 + 15,
            "the hover armed a real pick on its frames: requests {} -> {}",
            before.0,
            after.0
        );
        assert_eq!(
            after.2 - before.2,
            0,
            "the world-object lookup viewport rectangle refused {} hover(s) at this \
             pixel",
            after.2 - before.2
        );
        assert!(
            after.1 - before.1 <= 1,
            "{} hover(s) in this arm were refused by the HUD; `pointer_over_game_view`'s one-frame \
             lag accounts for one",
            after.1 - before.1
        );
    }

    fn draw_list(app: &mut App) -> Vec<UiDrawCmd> {
        let mut back = dereth_ui::RecordingDrawBackend::default();
        ui_of(app).draw(&mut back);
        back.calls
    }

    /// Hover `(x, y)` for `n` frames and report what the completed pick named.
    ///
    /// `n` is a parameter because one arm below is **timed**: the door's swing settles 29 frames
    /// after the hook fires, so an arm that wants to read the pick while the leaves are still
    /// across the doorway cannot spend twenty frames waiting for a tooltip first.
    /// `Interaction::pointer_over_game_view` is last frame's answer, so the smallest useful `n`
    /// is 2 and this file never uses fewer than 4.
    fn hover_for(app: &mut App, x: i32, y: i32, at: u32, n: usize) -> ObjectId {
        let _ = draw_list(app);
        move_pointer_to(app, x, y, at);
        frames(app, n);
        app.interaction().pick.click_object().0
    }

    /// The settled hover the stable arms use.
    fn hover(app: &mut App, x: i32, y: i32, at: u32) -> ObjectId {
        hover_for(app, x, y, at, 20)
    }

    fn wire_position(
        cell: u32,
        origin: Vec3,
        like: &Position,
    ) -> dereth_protocol::types::PositionWire {
        dereth_protocol::types::PositionWire {
            objcell_id: cell,
            frame: dereth_protocol::types::Frame {
                origin: dereth_protocol::types::Vec3 {
                    x: origin.x,
                    y: origin.y,
                    z: origin.z,
                },
                orientation: dereth_protocol::types::Quat {
                    w: like.frame.rotation.w,
                    x: like.frame.rotation.x,
                    y: like.frame.rotation.y,
                    z: like.frame.rotation.z,
                },
            },
        }
    }

    /// The interior cell of `block` that contains `world` (landblock metres) with nothing solid at
    /// it, by the same `cell_bsp` / `physics_bsp` pair used to find a standable point.
    fn cell_containing(store: &dereth_dat::RetailDatStore, block: u16, world: Vec3) -> Option<u32> {
        for d in dereth_client::env_cells::EnvCellLoader::new().load_block(store, block) {
            let g = dereth_client::env_cells::physics_geometry(&d);
            let Some(bsp) = g.cell_bsp.as_ref() else {
                continue;
            };
            let local = dereth_physics::math::globaltolocal(&g.frame, world);
            if !bsp.point_inside_cell_bsp(local) {
                continue;
            }
            if g.physics_bsp
                .as_ref()
                .is_some_and(|b| b.point_intersects_solid(local))
            {
                continue;
            }
            return Some(d.id.0);
        }
        None
    }

    /// `early-inventory-and-casting`'s own recorded chest, created at `origin` of `cell` under
    /// `BEHIND`.
    fn place_the_corpus_chest(
        app: &mut App,
        rows: &[CorpusBlob],
        cell: u32,
        origin: Vec3,
    ) -> String {
        let mut chosen = None;
        for r in rows {
            if r.dir != Direction::ServerToClient || r.opcode != Opcode::ITEM_CREATE_OBJECT.0 {
                continue;
            }
            let Ok(c) = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&r.payload[4..]))
            else {
                continue;
            };
            if c.0.wdesc.name.contains("Chest")
                && c.0.physicsdesc.bitfield & dereth_protocol::types::physicsdesc::flags::SETUP != 0
            {
                chosen = Some(c);
                break;
            }
        }
        let mut create =
            chosen.expect("early-inventory-and-casting has a named chest with a SETUP");
        let here = position(app);
        create.0.physicsdesc.position = Some(wire_position(cell, origin, &here));
        create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
        create.0.id = BEHIND;
        let name = create.0.wdesc.name.clone();
        app.objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::ITEM_CREATE_OBJECT,
                body: dereth_protocol::write_body(&create).expect("re-encodes"),
            },
            LocalTime(50.0),
        );
        frames(app, 30);
        name
    }

    /// Row 148's own recorded buffer with its command index swapped and its stamps advanced —
    /// which is exactly what the server sends to close a door. The `0xF74C` route is the one a live
    /// door takes through the normal object-movement event path.
    fn send_motion(app: &mut App, rows: &[CorpusBlob], cmd: MotionCommand, stamp: u16, at: f64) {
        let row = rows
            .iter()
            .find(|r| r.idx == 148)
            .expect("the recorded door movement");
        let mut m = dereth_protocol::movement::MovementSetObjectMovement::read(
            &mut dereth_protocol::Reader::new(&row.payload[4..]),
        )
        .expect("the recorded buffer decodes");
        let mut b = m.decoded_movement().expect("...and its body");
        b.movement_timestamp = stamp;
        b.server_control_timestamp = stamp;
        b.body
            .interpreted
            .as_mut()
            .expect("an interpreted motion")
            .forward_command = cmd.to_index();
        m.movement = dereth_protocol::movement::MovementSetObjectMovement::encode_movement(&b)
            .expect("encodes");
        app.objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
                body: dereth_protocol::write_body(&m).expect("encodes"),
            },
            LocalTime(at),
        );
    }

    /// `0xF74B` with the door's physics state, the way the server sends it on both edges.
    fn send_state(app: &mut App, state: u32, at: f64) {
        let ts = app
            .objects()
            .presence(DOOR)
            .expect("the door's presence")
            .state_ts;
        app.objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: ItemSetState::OPCODE,
                body: dereth_protocol::write_body(&ItemSetState {
                    id: DOOR,
                    state,
                    timestamps: PhysicsEventStamp {
                        instance: 0,
                        event: ts.wrapping_add(1),
                    },
                })
                .expect("encodes"),
            },
            LocalTime(at),
        );
    }

    fn door_is_ethereal(app: &App) -> bool {
        let h = app
            .objects()
            .physics
            .handle(DOOR)
            .expect("the door has a physics body");
        app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .world
            .get(h)
            .unwrap()
            .state
            .is_ethereal()
    }

    /// The two leaves' live world positions this frame, for parts 3 and 4.
    fn leaves(app: &App) -> Vec<Vec3> {
        app.world_state()
            .unwrap()
            .server_object_part_frames(DOOR)
            .expect("the door has live part frames")
            .into_iter()
            .skip(3)
            .map(|f| f.origin)
            .collect()
    }

    /// The recorded academy scene with the recorded door in it, the body standing at the recorded
    /// pre-USE placement and facing the door.
    fn academy() -> (App, &'static [CorpusBlob], Vec3) {
        let rows: &'static [CorpusBlob] = &Corpus::shared("early-inventory-and-casting").blobs;
        let row143 = rows
            .iter()
            .find(|r| r.idx == 143)
            .expect("the recorded pre-USE placement");
        let mut a = dereth_protocol::actions::unpack_action(&row143.payload).unwrap();
        let recorded = dereth_protocol::movement::MovementMoveToState::read(&mut a.body)
            .unwrap()
            .0;
        let p = recorded.position;
        let origin = Position::new(
            CellId(p.objcell_id),
            Frame::new(
                Vec3::new(p.frame.origin.x, p.frame.origin.y, p.frame.origin.z),
                Quat::new(
                    p.frame.orientation.w,
                    p.frame.orientation.x,
                    p.frame.orientation.y,
                    p.frame.orientation.z,
                ),
            ),
        );

        let mut app = App::new(Config {
            headless: true,
            sound: false,
            ui: true,
            width: SCREEN.0,
            height: SCREEN.1,
            dat_dir: dereth_dat::testing::dat_dir(),
            preferences_file: std::env::temp_dir()
                .join("dereth-open-door-pick-not-created")
                .join("preferences.ini"),
            ..Config::default()
        })
        .unwrap_or_else(|e| panic!("the gpu tier needs a headless App on a software device: {e}"));
        app.start_shell().expect("UI shell");
        app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
        app.load_static_scene(SceneConfig {
            landblock: ACADEMY,
            start_cell: Some(origin.cell),
            character: true,
            land_radius: 1,
            scenery_radius: 0,
            cell_statics: false,
            mesh_collision: false,
            particles: false,
            ..Default::default()
        })
        .expect("real academy cell and collision scene");
        {
            let c = app.world_state_mut().unwrap().character.as_mut().unwrap();
            c.land().load_block_cells(origin.cell.landblock());
            c.teleport(origin);
            c.stop_completely_from_action();
        }
        frames(&mut app, 60);

        // The player's own identity and assets: the recorded `0xF746` and `0xF745` re-pointed at
        // where the body is standing, into `ObjectStream` only.
        let player_row = rows
            .iter()
            .find(|r| {
                r.dir == Direction::ServerToClient && r.opcode == Opcode::LOGIN_CREATE_PLAYER.0
            })
            .expect("recorded player identity");
        let id = ObjectId(u32::from_le_bytes(
            player_row.payload[4..8].try_into().unwrap(),
        ));
        let row = rows
            .iter()
            .find(|r| {
                r.dir == Direction::ServerToClient
                    && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                    && u32::from_le_bytes(r.payload[4..8].try_into().unwrap()) == id.0
            })
            .expect("recorded player assets");
        let mut create =
            ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
                .expect("recorded 0xF745");
        let here = position(&app);
        create.0.physicsdesc.position = Some(wire_position(here.cell.0, here.frame.origin, &here));
        create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
        app.objects_mut()
            .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
        app.objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::ITEM_CREATE_OBJECT,
                body: dereth_protocol::write_body(&create).expect("constructed placement"),
            },
            LocalTime(1.0),
        );
        frames(&mut app, 90);
        {
            let state_ts = app
                .objects()
                .presence(id)
                .expect("the player's presence")
                .state_ts;
            app.objects_mut().apply_event(
                &SessionEvent::WorldObject {
                    opcode: ItemSetState::OPCODE,
                    body: dereth_protocol::write_body(&ItemSetState {
                        id,
                        state: TELEPORT_UNHIDE_STATE,
                        timestamps: PhysicsEventStamp {
                            instance: 0,
                            event: state_ts.wrapping_add(1),
                        },
                    })
                    .expect("encodes"),
                },
                LocalTime(4.0),
            );
        }
        frames(&mut app, 60);

        // **The recorded door**, at its own recorded placement — the only thing this bench takes
        // from the recording besides the player's assets.
        let door_row = rows
            .iter()
            .find(|r| r.idx == 52)
            .expect("the recorded door create");
        feed(&mut app, door_row, 6.0);
        frames(&mut app, 60);
        let create =
            ItemCreateObject::read(&mut dereth_protocol::Reader::new(&door_row.payload[4..]))
                .unwrap()
                .0;
        let dp = create.physicsdesc.position.unwrap();
        let door_world = Vec3::new(dp.frame.origin.x, dp.frame.origin.y, dp.frame.origin.z);

        // Face the door: `yaw_quat` turns +y by `yaw` about +z, so the heading that points along
        // `d` is `atan2(-d.x, d.y)`.
        let here = position(&app);
        let d = Vec3::new(
            door_world.x - here.frame.origin.x,
            door_world.y - here.frame.origin.y,
            0.0,
        );
        let yaw = dereth_primitives::num::math::atan2f(-d.x, d.y);
        {
            let c = app.world_state_mut().unwrap().character.as_mut().unwrap();
            c.teleport(Position::new(
                here.cell,
                Frame::new(here.frame.origin, yaw_quat(yaw)),
            ));
        }
        frames(&mut app, PICK_SETTLE);
        eprintln!(
            "bench: body {:?} in {:#010X}, door at {door_world:?} in {DOOR_CELL:#010X}, \
             yaw {:.1} deg",
            position(&app).frame.origin,
            position(&app).cell.0,
            yaw.to_degrees()
        );
        (app, rows, door_world)
    }

    /// Behaviour: selection.pick.an-open-door-stops-swallowing-the-pick
    ///
    /// One scene, one pixel, one pick, walked through the door's four states.
    ///
    /// The pixel is aimed at a **chest standing behind the door**, so every arm is the same
    /// question — *"what is under this cursor?"* — and the only thing that changes between them is
    /// the door's animation frame. The camera never moves: the recorded player turn (row 147) is
    /// not fed, and nothing after the first settle teleports the body.
    #[test]
    fn an_open_door_stops_swallowing_the_pick_and_a_closed_one_resumes_it() {
        let (mut app, rows, door_world) = academy();
        let store = dereth_dat::testing::open_store_or_fail();

        // ---- premises -------------------------------------------------------------------------
        let door_frame = app
            .world_state()
            .unwrap()
            .server_object_frame(DOOR)
            .expect("the recorded door is drawn");
        assert!(
            app.world_scene()
                .unwrap()
                .drawn_objects()
                .is_some_and(|s| s.contains(&DOOR)),
            "the frame's object pass offered the door to the ray"
        );
        assert!(
            !door_is_ethereal(&app),
            "the recorded door starts closed and solid"
        );

        // A point beyond the doorway, **beside** the line the camera looks along.
        //
        // Beside, and not on it, because of the chase camera: it keeps the body in the centre of
        // the screen, and indoors it cannot back away far enough for the body to shrink. A pixel on
        // the centre line is therefore a pixel on the **player**, and the hover names him, which is
        // retail's answer too. Part translucency raises `NoDraw` only at exactly 1.0, and a bench
        // like this one sits near 0.737. The doorway is 4.65 m between its jambs and each leaf is
        // ~2 m wide, so a lateral metre is still behind a closed leaf and still inside the opening.
        //
        // Both the offset and the distance are **searched**, because which cell lies beyond the
        // doorway and how far it reaches is the dat's shape and not this test's.
        let here = position(&app);
        let dir = {
            let d = Vec3::new(
                door_world.x - here.frame.origin.x,
                door_world.y - here.frame.origin.y,
                0.0,
            );
            let n = (d.x * d.x + d.y * d.y).sqrt();
            Vec3::new(d.x / n, d.y / n, 0.0)
        };
        let perp = Vec3::new(-dir.y, dir.x, 0.0);
        let (behind_cell, behind_at) = [1.1f32, -1.1, 0.9, -0.9, 1.3, -1.3]
            .into_iter()
            .flat_map(|lat| {
                (6..=16).map(move |i: i32| {
                    #[allow(clippy::cast_precision_loss)]
                    let m = i as f32 * 0.25;
                    (lat, m)
                })
            })
            .map(|(lat, m)| {
                Vec3::new(
                    door_world.x + dir.x * m + perp.x * lat,
                    door_world.y + dir.y * m + perp.y * lat,
                    door_world.z,
                )
            })
            .find_map(|p| {
                let probe = Vec3::new(p.x, p.y, p.z + 0.6);
                cell_containing(&store, ACADEMY, probe)
                    .filter(|c| *c != DOOR_CELL)
                    .map(|c| (c, p))
            })
            .expect("a free point in a cell beyond the doorway");
        let name = place_the_corpus_chest(&mut app, &rows, behind_cell, behind_at);
        eprintln!(
            "{name:?} {:#010X} placed at {behind_at:?} in {behind_cell:#010X}",
            BEHIND.0
        );
        assert!(
            app.world_state()
                .unwrap()
                .server_object_frame(BEHIND)
                .is_some(),
            "the chest behind the door is drawn"
        );
        assert!(
            app.world_scene()
                .unwrap()
                .drawn_objects()
                .is_some_and(|s| s.contains(&BEHIND)),
            "...and the object pass offered it to the ray, so a pick that misses it is a miss and \
             not a cull"
        );

        let cf = app
            .world_state()
            .unwrap()
            .server_object_frame(BEHIND)
            .unwrap();
        let at = aim(&app, Vec3::new(cf.origin.x, cf.origin.y, cf.origin.z + 0.5));
        eprintln!(
            "aim pixel {at:?} is over {:?}",
            element_under(&mut app, at.0, at.1)
        );
        eprintln!(
            "aiming at {at:?}; door frame {:?}, closed leaves {:?}",
            door_frame.origin,
            leaves(&app)
        );

        // How many of the two swung-open leaves stage 3b could actually aim at.
        let mut leaf_arms = 0usize;

        // ---- 1. closed: the door is what the pick names ----------------------------------------
        let closed_leaves = leaves(&app);
        let counters = pick_counters(&app);
        let named = hover(&mut app, at.0, at.1, 2_000);
        eprintln!(
            "closed: hover named {:#010X}; {:?}",
            named.0,
            app.interaction().pick.stats
        );
        pick_was_armed(counters, pick_counters(&app));
        assert_ne!(
            named,
            app.objects()
                .player()
                .expect("the bench has a local player"),
            "the aim pixel {at:?} landed on the player's own body, which is what a chase camera \
             puts in the centre of the screen, and clicking your own body selects you; the \
             lateral offset of the point behind the door was not enough. Nothing about the door \
             has been measured yet"
        );
        assert_eq!(
            named, DOOR,
            "a closed door between the cursor and the chest must be what the pick names — that is \
             how you open it. It named {:#010X}",
            named.0
        );

        // ---- 2. mid-swing: ethereal already, but the leaves are still across -------------------
        // Row 148 is the door's own recorded `MotionCommand::ON`; row 149 the recorded
        // `state | ETHEREAL_PS`. Both go in, and then the pick is taken in the **next handful of
        // frames**: the hook fires on the first (measured in `the_retail_open_animation_...`: step
        // 1 of a swing that settles at 29), so the bit is set while the geometry has barely moved.
        //
        // The whole arm is inside that window on purpose, which is why it uses a four-frame hover
        // rather than the settled twenty — twenty frames is most of the swing, and an arm that
        // waited them out would be measuring an open door and calling it a partly-open one.
        for (idx, when) in [(148usize, 46.0), (149usize, 46.1)] {
            feed(&mut app, rows.iter().find(|r| r.idx == idx).unwrap(), when);
        }
        frames(&mut app, 2);
        assert!(
            door_is_ethereal(&app),
            "the intangible-state hook has already fired two frames in"
        );
        let named = hover_for(&mut app, at.0, at.1, 3_000, 4);
        // Read the geometry **after** the pick, so the displacement reported is the one the ray
        // was actually offered rather than an earlier frame's.
        let mid_leaves = leaves(&app);
        let mid_travel: f32 = mid_leaves
            .iter()
            .zip(&closed_leaves)
            .map(|(a, b)| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt())
            .fold(0.0, f32::max);
        eprintln!(
            "mid-swing: hover named {:#010X}; ethereal {}, leaves moved {mid_travel:.3} m, \
             {mid_leaves:?}",
            named.0,
            door_is_ethereal(&app)
        );
        assert!(
            mid_travel < 0.6,
            "by the time the pick ran the leaves had moved {mid_travel:.3} m; this arm needs them \
             still across the doorway"
        );
        assert_eq!(
            named, DOOR,
            "a door whose leaves are still across the doorway is still what the ray hits, however \
             ethereal collision already considers it: selection tests the drawing sphere and \
             polygons and nothing else. It named {:#010X}",
            named.0
        );

        // ---- 3. open: the pick reaches what is behind it ---------------------------------------
        frames(&mut app, SWING);
        let open_leaves = leaves(&app);
        let open_travel: f32 = open_leaves
            .iter()
            .zip(&closed_leaves)
            .map(|(a, b)| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt())
            .fold(f32::MAX, f32::min);
        eprintln!(
            "open: leaves moved >= {open_travel:.3} m, {open_leaves:?}; ethereal {}",
            door_is_ethereal(&app)
        );
        assert!(
            open_travel > 1.0,
            "the swing must actually have finished before the pick is asked: {open_travel:.3} m"
        );
        let before = draw_list(&mut app);
        let named = hover(&mut app, at.0, at.1, 4_000);
        let after = draw_list(&mut app);
        eprintln!(
            "open: hover named {:#010X}; {:?}",
            named.0,
            app.interaction().pick.stats
        );
        assert_ne!(
            named, DOOR,
            "with the door swung open the same pixel still named the door: its leaves are being \
             swept using setup-record placement frames instead of physics-part positions. Pick stats \
             {:?}",
            app.interaction().pick.stats
        );
        assert_eq!(
            named, BEHIND,
            "...and what it must name instead is the chest standing behind the doorway. It named \
             {:#010X}",
            named.0
        );
        let known: std::collections::BTreeSet<ElemHandle> = before.iter().map(|c| c.who).collect();
        let glyphs: String = after
            .iter()
            .filter(|c| !known.contains(&c.who))
            .flat_map(|c| {
                c.glyphs
                    .iter()
                    .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
            })
            .collect();
        eprintln!("open: glyphs that appeared {glyphs:?}");
        assert!(
            app.interaction().pick.stats.parts_at_the_animated_pose > 0,
            "the sweep is reading live part frames at all"
        );

        // ---- 3b. ...and the swung-open leaf is still the door --------------------------------
        //
        // Retail picks through an open door, but a click on the door's edge, even while opened,
        // picks the door. That is the half that stops "skip ethereal objects" from passing: the
        // leaf has **moved**, not vanished, and clicking it where it now stands must still name
        // the door, otherwise you could never close one. The selection ray has no ethereal test
        // and the draw path's only part gate is `NoDraw`, so the leaf is offered to the ray at its
        // animated frame like any other drawn part.
        //
        // The aim point is the leaf's **own live part frame**, read out of the scene, so this
        // is not a pixel this test chose — it is where the frame drew the leaf.
        for (i, leaf) in open_leaves.iter().enumerate() {
            let Some(hit) = aim_if_visible(&app, *leaf) else {
                eprintln!("leaf {i}: {leaf:?} does not project into the view");
                continue;
            };
            let counters = pick_counters(&app);
            let named = hover(&mut app, hit.0, hit.1, 5_000);
            eprintln!(
                "leaf {i}: aimed at {leaf:?} -> pixel {hit:?}, hover named {:#010X}",
                named.0
            );
            pick_was_armed(counters, pick_counters(&app));
            assert_eq!(
                named, DOOR,
                "clicking the swung-open leaf at {leaf:?} must still name the door -- that is how \
                 you close it. It named {:#010X}",
                named.0
            );
            leaf_arms += 1;
        }
        assert!(
            leaf_arms > 0,
            "neither swung-open leaf projected into the view, so the 'click the actual door edge' \
             case was not measured at all"
        );

        // ---- 4. closing: solid again ----------------------------------------------------------
        send_motion(&mut app, &rows, MotionCommand::OFF, 2, 60.0);
        send_state(&mut app, 0x0001_0018, 60.1);
        frames(&mut app, SWING);
        let shut_leaves = leaves(&app);
        let shut_travel: f32 = shut_leaves
            .iter()
            .zip(&closed_leaves)
            .map(|(a, b)| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt())
            .fold(0.0, f32::max);
        eprintln!(
            "closing: leaves back to within {shut_travel:.3} m, ethereal {}",
            door_is_ethereal(&app)
        );
        assert!(
            shut_travel < 0.05,
            "the OFF motion must put the leaves back in the doorway: {shut_travel:.3} m"
        );
        assert!(
            !door_is_ethereal(&app),
            "and the closing hook clears ETHEREAL_PS again"
        );
        let named = hover(&mut app, at.0, at.1, 6_000);
        eprintln!("closing: hover named {:#010X}", named.0);
        assert_eq!(
            named, DOOR,
            "a door that has closed again is solid to the pick again. It named {:#010X}",
            named.0
        );

        app.shutdown();
    }
}
