//! Drawing across the outdoor/indoor boundary of a building: a closed door in a house's front
//! doorway keeps its outdoor origin cell while it shadows into the room and is drawn across the
//! depth reset; from outdoors, static interiors appear only through the building's openings and
//! the house's wall hides an object standing inside it; an object that claims the outdoor cell
//! while standing inside the room is not drawn, because that placement is refused.
//! Fixture: the house with a cellar on Holtburg's landblock 0xA9B4 from the retail dats, a door
//! created through the application's own object stream (no datagram leaves this process), and a
//! body at a recorded pose in the doorway, on a software device. The ignored generators write
//! PNGs to `DERETH_TEST_BUILDING_BOUNDARY_DUMP`.

#![cfg(gpu)]

use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
use std::sync::Arc;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

pub(crate) const W: u32 = 1200;
pub(crate) const H: u32 = 900;
pub(crate) const HOLTBURG: u16 = 0xA9B4;

/// A recorded player pose, as retail's `/loc` reads it: `0xA9B40029 [136.289993 5.155000
/// 94.082001] 0.707107 0.000000 0.000000 -0.707107`.
pub(crate) const RECORDED_CELL: u32 = 0xA9B4_0029;
pub(crate) const RECORDED_ORIGIN: Vec3 = Vec3::new(136.289_993, 5.155, 94.082_001);
pub(crate) const RECORDED_ROT: Quat = Quat::new(0.707_107, 0.0, 0.0, -0.707_107);

pub(crate) fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

// ---------------------------------------------------------------------------------------------
// The recorded station: a door in the house's front doorway, seen from inside the room.
// ---------------------------------------------------------------------------------------------

/// The door setup at this landblock.
const DOOR_SETUP: u32 = 0x0200_024F;
/// The table the server supplies for this setup. Its default/off cycle is the closed frame of
/// animation `0x03000559`; the setup itself has no default motion table and its placement frame
/// is the one-third-open fallback.
const DOOR_MTABLE: u32 = 0x0900_0016;
const DOOR: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0D25);

/// Create the door through the application's own object stream.
///
/// **No datagram leaves this process** — the payload is encoded and handed straight to
/// `ObjectStream::apply_event`, exactly as `rendering/objects_through_doorways.rs` does.
fn place_door(stream: &mut ObjectStream, at: Position, now: f64, no_draw: bool) {
    let mut movement = dereth_protocol::Writer::new();
    let style = dereth_animation::MotionCommand::NON_COMBAT
        .to_index()
        .expect("NonCombat is on the wire");
    dereth_protocol::movement::MovementBody {
        current_style: style,
        interpreted: Some(dereth_protocol::movement::InterpretedMotionState {
            current_style: Some(style),
            forward_command: Some(
                dereth_animation::MotionCommand::OFF
                    .to_index()
                    .expect("Off is on the wire"),
            ),
            ..dereth_protocol::movement::InterpretedMotionState::default()
        }),
        ..dereth_protocol::movement::MovementBody::default()
    }
    .write(&mut movement)
    .expect("the closed-door movement body encodes");
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id: DOOR,
        objdesc: dereth_protocol::types::ObjDesc::default(),
        physicsdesc: dereth_protocol::types::physicsdesc::PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                | dereth_protocol::types::physicsdesc::flags::SETUP
                | dereth_protocol::types::physicsdesc::flags::MTABLE
                | dereth_protocol::types::physicsdesc::flags::MOVEMENT,
            // All eight recorded creates of this setup carry HAS_PHYSICS_BSP, IGNORE_COLLISIONS
            // and REPORT_COLLISIONS. In particular this prevents the player's body, which is in the
            // doorway too, from pushing the fixture away before the captured frame.
            state: 0x0001_0018
                | if no_draw {
                    dereth_physics::PhysicsState::NODRAW_PS
                } else {
                    0
                },
            setup_id: Some(DOOR_SETUP),
            mtable_id: Some(DOOR_MTABLE),
            movement: Some((movement.into_inner(), 0)),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: at.cell.0,
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: at.frame.rotation.into(),
                },
            }),
            timestamps: dereth_protocol::types::PhysicsTimestamps {
                instance: 1,
                ..dereth_protocol::types::PhysicsTimestamps::default()
            },
            ..dereth_protocol::types::physicsdesc::PhysicsDesc::default()
        },
        wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
    };
    let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
        .expect("encode");
    stream.apply_event(
        &dereth_client_net::client_session::SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(now),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn door_shot(
    store: &Arc<RetailDatStore>,
    body_cell: u32,
    body: Vec3,
    yaw_deg: f32,
    door: Option<Position>,
    tag: &str,
) -> Option<Vec<u8>> {
    door_shot_with_state(store, body_cell, body, yaw_deg, door, tag).map(|shot| shot.rgba)
}

#[allow(clippy::too_many_arguments)]
fn door_shot_nodraw(
    store: &Arc<RetailDatStore>,
    body_cell: u32,
    body: Vec3,
    yaw_deg: f32,
    door: Position,
    tag: &str,
) -> Option<DoorShot> {
    door_shot_inner(
        store,
        body_cell,
        body,
        yaw_deg,
        None,
        Some(door),
        true,
        None,
        tag,
    )
}

struct DoorShot {
    rgba: Vec<u8>,
    physical_cell: Option<CellId>,
    shadow_cells: Vec<CellId>,
    submitted_parts: usize,
    submitted: Vec<(usize, usize, bool)>,
    camera_cell: CellId,
    camera_position: Vec3,
    camera_yaw: f32,
    camera_pitch: f32,
    portal_polygons: Vec<Vec<(f32, f32)>>,
}

fn door_shot_with_state(
    store: &Arc<RetailDatStore>,
    body_cell: u32,
    body: Vec3,
    yaw_deg: f32,
    door: Option<Position>,
    tag: &str,
) -> Option<DoorShot> {
    door_shot_inner(
        store, body_cell, body, yaw_deg, None, door, false, None, tag,
    )
}

#[allow(clippy::too_many_arguments)]
fn door_shot_inner(
    store: &Arc<RetailDatStore>,
    body_cell: u32,
    body: Vec3,
    yaw_deg: f32,
    body_rotation: Option<Quat>,
    door: Option<Position>,
    no_draw: bool,
    isolated_building_portals: Option<bool>,
    tag: &str,
) -> Option<DoorShot> {
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let mut cfg = SceneConfig {
        landblock: HOLTBURG,
        time_of_day: Some(0.35),
        ..SceneConfig::default()
    };
    if let Some(enabled) = isolated_building_portals {
        cfg.building_portals = enabled;
        // Isolate the static env-cell pass from the building pass's preceding alpha-list flush. The
        // latter is a separate landscape reorder and is already covered by clipped_outdoor_pass.
        cfg.portal_alpha_flush = false;
    }
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    let yaw = yaw_deg.to_radians();
    let q = body_rotation
        .unwrap_or_else(|| Quat::new(math::cosf(yaw * 0.5), 0.0, 0.0, math::sinf(yaw * 0.5)));
    scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(CellId(body_cell), Frame::new(body, q)));
    let mut stream = ObjectStream::new();
    let mut now = 0.0f64;
    if let Some(d) = door {
        place_door(&mut stream, d, now, no_draw);
    }
    let mut rgba = Vec::new();
    for _ in 0..6 {
        now += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(store, &mut gpu, &mut stream)
            .expect("sync_objects");
        // Follow the application's object-create integration: `sync_objects` prepares the
        // scene object, then `sync_physics_at` places its physics body. Without the latter,
        // rendering would use the wire cell instead of the body's achieved cell.
        if let Some(c) = scene.character.as_mut() {
            stream.sync_physics_at(store, &mut c.world, LocalTime(now));
        }
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
        // Follow the application frame order: `crate::camera::update_viewer` sweeps the camera
        // after the world update and records `CameraControl::viewer_cell`. If the viewer update
        // is omitted here, `WorldScene::viewer_cell` falls back to the body's cell, which
        // measures the doorway from the body rather than from the swept camera.
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            dereth_client_runtime::camera::CameraInput::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    if let Some(c) = scene
        .character
        .as_ref()
        .expect("a body")
        .world
        .by_object_id(DOOR)
    {
        let pc = scene
            .character
            .as_ref()
            .expect("a body")
            .world
            .get(c)
            .map(|b| (b.cell, b.position));
        eprintln!("{tag}: the door physics object's cell = {pc:?}");
        let shadows = scene
            .character
            .as_ref()
            .expect("a body")
            .world
            .get(c)
            .map(|b| &b.shadow_objects);
        eprintln!(
            "{tag}: door shadows = {shadows:?}; drawn cells = {:?}",
            scene.drawn_cells()
        );
    } else {
        eprintln!("{tag}: the door has no physics body");
    }
    if let Some(p) = stream.presence(DOOR).and_then(|p| p.position) {
        eprintln!(
            "{tag}: the door settled in {:#010X} at [{:.3} {:.3} {:.3}]",
            p.cell.0, p.frame.origin.x, p.frame.origin.y, p.frame.origin.z
        );
    } else {
        eprintln!("{tag}: the door has no presence/position");
    }
    let cam = scene.character.as_ref().expect("a body").camera.viewer_cell;
    let cs = scene.character.as_ref().expect("a body").camera.stats;
    eprintln!("{tag}: render camera = {:?}", scene.camera.position);
    eprintln!(
        "{tag}: camera sweeps={} blocked={} failed={} no-viewer={}",
        cs.sweeps, cs.sweeps_blocked, cs.sweeps_failed, cs.frames_without_viewer
    );
    let settled = scene.character.as_ref().expect("a body").position();
    let submitted: Vec<_> = scene
        .drawn_part_order()
        .iter()
        .filter(|e| e.object == Some(DOOR))
        .map(|e| (e.part, e.subset, e.outdoors))
        .collect();
    let door_parts = submitted.len();
    eprintln!(
        "{tag}: body {:#010X} [{:.3} {:.3} {:.3}], camera cell {:?}, door parts drawn {door_parts}",
        settled.cell.0,
        settled.frame.origin.x,
        settled.frame.origin.y,
        settled.frame.origin.z,
        cam.map(|c| format!("{:#010X}", c.0)),
    );
    let physics = &scene.character.as_ref().expect("a body").world;
    let shadow_cells = physics
        .by_object_id(DOOR)
        .and_then(|h| physics.get(h))
        .map(|b| {
            b.shadow_objects
                .iter()
                .filter(|s| s.cell_present)
                .map(|s| s.cell_id)
                .collect()
        })
        .unwrap_or_default();
    let result = DoorShot {
        rgba,
        physical_cell: physics
            .by_object_id(DOOR)
            .and_then(|h| physics.get(h))
            .and_then(|b| b.cell),
        shadow_cells,
        submitted_parts: door_parts,
        submitted,
        camera_cell: cam.unwrap_or(settled.cell),
        camera_position: scene.camera.position,
        camera_yaw: scene.camera.yaw,
        camera_pitch: scene.camera.pitch,
        portal_polygons: scene.building_portal_screen_polygons(W, H),
    };
    let Ok(dir) = std::env::var("DERETH_TEST_BUILDING_BOUNDARY_DUMP") else {
        return Some(result);
    };
    let path = format!("{dir}/{tag}.png");
    let f = std::fs::File::create(&path).expect("create the png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("png header");
    w.write_image_data(&result.rgba).expect("png data");
    Some(result)
}

/// Behaviour: rendering.portals.a-closed-door-crosses-the-outdoor-indoor-boundary
/// The door station. This create carries the server motion table and state word measured on all
/// eight captured creates of the same setup, then drives that table's authored closed `Off` cycle. The test
/// establishes that the naturally posed physical door has an indoor shadow while retaining the
/// outdoor origin cell, then checks both submissions and final pixels across the depth reset.
#[test]
fn the_closed_holtburg_door_naturally_crosses_the_outdoor_indoor_boundary() {
    let store = store();
    let door = Position::new(CellId(RECORDED_CELL), Frame::new(DOORWAY, DOOR_ROT));
    let with = door_shot_with_state(
        &store,
        0xA9B4_0143,
        Vec3::new(138.20, 5.15, 94.00),
        90.0,
        Some(door),
        "closed-crossing",
    )
    .expect("this rendering regression requires D3D12 WARP");
    let hidden = door_shot_nodraw(
        &store,
        0xA9B4_0143,
        Vec3::new(138.20, 5.15, 94.00),
        90.0,
        door,
        "closed-crossing-nodraw",
    )
    .expect("the NODRAW control also requires D3D12 WARP");
    let changed = with
        .rgba
        .chunks_exact(4)
        .zip(hidden.rgba.chunks_exact(4))
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    eprintln!(
        "closed crossing: origin={:?}, camera={:#010X}, shadows={:?}, parts={}, pixels={changed}",
        with.physical_cell, with.camera_cell.0, with.shadow_cells, with.submitted_parts,
    );
    assert_eq!(
        with.physical_cell,
        Some(CellId(RECORDED_CELL)),
        "the server-authored origin is outdoors"
    );
    assert_eq!(
        with.physical_cell, hidden.physical_cell,
        "NODRAW preserves the physical door"
    );
    assert_eq!(
        with.shadow_cells, hidden.shadow_cells,
        "NODRAW preserves cell registration"
    );
    assert_eq!(
        with.camera_cell, hidden.camera_cell,
        "NODRAW preserves the camera sweep"
    );
    assert_eq!(
        (with.camera_position, with.camera_yaw, with.camera_pitch),
        (
            hidden.camera_position,
            hidden.camera_yaw,
            hidden.camera_pitch
        ),
        "NODRAW preserves the exact final render camera, not only its cell"
    );
    assert!(
        !dereth_physics::landdefs::is_outdoors(with.camera_cell),
        "the nearby west-facing control must keep its camera in the room"
    );
    assert!(
        with.shadow_cells
            .iter()
            .any(|cell| !dereth_physics::landdefs::is_outdoors(*cell)),
        "the actual closed pose has no indoor physical shadow, so the proposed second-pass seam \
         is not a natural explanation of this door"
    );
    assert!(
        changed > 115_000,
        "the closed door changes {changed} pixels; the one-pass rejecting frame changed 109,105, \
         so the post-clear draw has not restored the missing door pixels"
    );
    assert_eq!(
        with.submitted_parts, 12,
        "the six shipped part/subset entries must be submitted before and after the depth reset"
    );
    let (first_pass, second_pass) = with.submitted.split_at(6);
    assert!(
        first_pass.iter().all(|(_, _, outdoors)| *outdoors),
        "the first six entries must use the outdoor sunlight phase"
    );
    assert!(
        second_pass.iter().all(|(_, _, outdoors)| !*outdoors),
        "the six entries after the depth reset must use the interior light set"
    );
    let outdoor: Vec<_> = with
        .submitted
        .iter()
        .filter(|(_, _, outdoors)| *outdoors)
        .map(|(part, subset, _)| (*part, *subset))
        .collect();
    let interior: Vec<_> = with
        .submitted
        .iter()
        .filter(|(_, _, outdoors)| !*outdoors)
        .map(|(part, subset, _)| (*part, *subset))
        .collect();
    assert_eq!(
        outdoor, interior,
        "the identical part/subset set must cross the depth reset"
    );
}

// ---------------------------------------------------------------------------------------------
// The exterior station: static env-cell geometry seen through the building openings.
// ---------------------------------------------------------------------------------------------

const EXTERIOR_CELL: u32 = 0xA9B4_002A;
const EXTERIOR_ORIGIN: Vec3 = Vec3::new(133.168_503, 27.490_582, 94.005_005);
const EXTERIOR_ROT: Quat = Quat::new(-0.991_022, 0.0, 0.0, -0.133_696);

fn exterior_shot(store: &Arc<RetailDatStore>, portals: bool, tag: &str) -> Option<DoorShot> {
    door_shot_inner(
        store,
        EXTERIOR_CELL,
        EXTERIOR_ORIGIN,
        0.0,
        Some(EXTERIOR_ROT),
        None,
        false,
        Some(portals),
        tag,
    )
}

fn edge_distance(poly: &[(f32, f32)], x: f32, y: f32) -> f32 {
    let mut best = f32::MAX;
    for i in 0..poly.len() {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % poly.len()];
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = dx.mul_add(dx, dy * dy);
        let t = if len2 <= 1.0e-6 {
            0.0
        } else {
            (((x - x0) * dx + (y - y0) * dy) / len2).clamp(0.0, 1.0)
        };
        let (cx, cy) = (x0 + dx * t, y0 + dy * t);
        best = best.min((x - cx).mul_add(x - cx, (y - cy) * (y - cy)).sqrt());
    }
    best
}

fn inside_polygon(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut hit = false;
    for i in 0..poly.len() {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % poly.len()];
        if (y0 > y) != (y1 > y) {
            let t = (y - y0) / (y1 - y0);
            if x < x0 + t * (x1 - x0) {
                hit = !hit;
            }
        }
    }
    hit
}

/// The recorded outdoor pose, with the portal interior isolated from the building pass's separate alpha
/// flush. The off/on differential is static interior content only: env-cell structure and its
/// baked furniture. Retail's land-cell order covers a farther building's portal stamp with every
/// nearer terrain cell; a floor, wall or furnishing fleck on that foreground terrain is therefore
/// a concrete ordering failure rather than a lighting difference.
/// The assertions require all changed RGB pixels to lie in the measured terrain band, then
/// require that count to be zero. Polygon containment and edge distance are reported only.
#[test]
fn the_recorded_outdoor_pose_confines_static_interiors_to_building_openings() {
    let store = store();
    let off = exterior_shot(&store, false, "exterior-no-interiors")
        .expect("this rendering regression requires D3D12 WARP");
    let on = exterior_shot(&store, true, "exterior-with-interiors")
        .expect("the portal-pass arm also requires D3D12 WARP");
    assert_eq!(
        off.camera_cell, on.camera_cell,
        "the portal pass must not move the camera cell"
    );
    assert_eq!(
        (off.camera_position, off.camera_yaw, off.camera_pitch),
        (on.camera_position, on.camera_yaw, on.camera_pitch),
        "the portal pass must not move the exact render camera"
    );
    assert!(
        dereth_physics::landdefs::is_outdoors(on.camera_cell),
        "the recorded station's swept camera must remain outdoors"
    );
    assert!(
        !on.portal_polygons.is_empty(),
        "no building opening survived the exact screen clip"
    );

    let mut changed = 0usize;
    let mut outside = 0usize;
    let mut worst = 0.0f32;
    let mut terrain_leak = 0usize;
    for (i, (a, b)) in off
        .rgba
        .chunks_exact(4)
        .zip(on.rgba.chunks_exact(4))
        .enumerate()
    {
        if a[..3] == b[..3] {
            continue;
        }
        changed += 1;
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let (x, y) = ((i as u32 % W) as f32 + 0.5, (i as u32 / W) as f32 + 0.5);
        // A misordered draw puts its flecks in x=265..796, y=457..523, the central terrain
        // band, clear of the upper-left house shell. The bound below is
        // deliberately broader. Nearer terrain must cover a farther cell's portal stamp in the
        // ordered land-cell pass; a late portal draw cannot punch a new far-depth hole there.
        if x >= 220.0 && (430.0..=560.0).contains(&y) {
            terrain_leak += 1;
        }
        let inside = on.portal_polygons.iter().any(|p| inside_polygon(p, x, y));
        if !inside {
            outside += 1;
            let distance = on
                .portal_polygons
                .iter()
                .map(|p| edge_distance(p, x, y))
                .fold(f32::MAX, f32::min);
            worst = worst.max(distance);
        }
    }
    eprintln!(
        "exterior recorded pose: {} opening(s), {changed} static-interior pixels, {outside} outside, worst {worst:.1}px, terrain leak {terrain_leak}",
        on.portal_polygons.len(),
    );
    assert_eq!(
        terrain_leak, changed,
        "{terrain_leak} of {changed} portal pixels lie in the measured terrain band; the subject \
         changed somewhere beyond the terrain-fleck band and needs a new bounded attribution"
    );
    assert_eq!(
        terrain_leak, 0,
        "{terrain_leak} portal-interior pixels overwrite the foreground terrain below the recorded pose; \
         the land-cell pass must draw each building after its own terrain cell and before nearer cells"
    );
}

/// The doorway cell `0xA9B40145` is x [136.30, 136.70], y [4.20, 6.10] — the house's **front
/// door**, facing west. The recorded pose `[136.289993 5.155000 94.082001]` is approximately the
/// centre of that doorway's west face (y 5.15).
pub(crate) const DOORWAY: Vec3 = Vec3::new(136.289_993, 5.155, 94.082_001);
pub(crate) const DOOR_ROT: Quat = Quat::new(0.707_107, 0.0, 0.0, -0.707_107);

// ---------------------------------------------------------------------------------------------
// The rejecting test.
//
// The room is `0xA9B40143`, x [136.70, 142.10], y [3.90, 13.10]. Its **west** wall carries
// the house's front doorway `0xA9B40145` at y [4.20, 6.10] and the wide interior opening
// `0xA9B40144` at y [7.84, 13.10]. Between them, y [6.10, 7.84] is **1.74 m of solid stone**.
//
// A door standing just outside that solid stretch is behind wall geometry from every viewpoint in
// the room, and must paint nothing. The premise arm puts the same door two metres nearer, inside
// the room with nothing in front of it, so the station is shown to be pointed at the door and the
// door shown to be drawable before the occluded arm is believed.
// ---------------------------------------------------------------------------------------------

/// The same door, two metres into the room, with nothing between it and the camera.
const IN_THE_ROOM: Vec3 = Vec3::new(138.60, 7.00, 94.082_001);
/// Where the body stands. **Not** x >= 140.50: the stairwell head `0xA9B40146` is
/// x [140.50, 142.10], y [3.90, 8.70] with its floor at z 93.80, and a body put there walks into
/// the cellar before the first frame is captured, which the premise assertion below reports.
const STAND: Vec3 = Vec3::new(139.50, 7.00, 94.00);
const FACING_WEST: f32 = 90.0;

fn door_pixels_in(
    store: &Arc<RetailDatStore>,
    cell: u32,
    at: Vec3,
    tag: &str,
) -> Option<(usize, usize)> {
    let door = Position::new(CellId(cell), Frame::new(at, DOOR_ROT));
    let with = door_shot(
        store,
        0xA9B4_0143,
        STAND,
        FACING_WEST,
        Some(door),
        &format!("{tag}-with"),
    )?;
    let base = door_shot(
        store,
        0xA9B4_0143,
        STAND,
        FACING_WEST,
        None,
        &format!("{tag}-without"),
    )?;
    let n = with
        .chunks_exact(4)
        .zip(base.chunks_exact(4))
        .filter(|(a, b)| a[..3] != b[..3])
        .count();
    Some((n, with.len() / 4))
}

/// The same door at the same point about two metres inside the room, rendered by the same helper
/// on separately created devices. The two input arms differ in the `objcell_id` carried by
/// `PhysicsDesc`, not in the door's position or appearance.
///
/// An outdoor cell paired with an indoor point is an invalid placement, and retail's create path
/// refuses it before anything is drawn:
///
///  1. Position adjustment never descends into a building for an outdoor id: visible-child
///     lookup is confined to the interior arm, so placement starts in the land cell and stays
///     there.
///  2. Placement inserts into that land cell before checking other cells, so building collision
///     runs before an interior-cell hit is recorded; the enclosed volume is solid, collision
///     rejects world entry and the create handler queues the object for destruction. ACE agrees.
///  3. The input does not arise against ACE, which gives an object standing indoors the env cell
///     it is standing in.
///
/// The door is created by encoding an `ItemCreateObject` payload and handing it straight to
/// [`ObjectStream::apply_event`]; no datagram leaves this process.
#[test]
fn an_object_inside_the_room_is_not_drawn_when_its_cell_is_the_outdoor_one() {
    let store = store();
    // Supply the outdoor landcell `0xA9B40029` paired with an indoor point. This is the
    // deliberately invalid placement arm, not a claim that its body survives six frames.
    let Some((outdoor, total)) = door_pixels_in(&store, RECORDED_CELL, IN_THE_ROOM, "cell-outdoor")
    else {
        return;
    };
    // The cell that geometrically contains the same point: the room itself.
    let Some((interior, _)) = door_pixels_in(&store, 0xA9B4_0143, IN_THE_ROOM, "cell-interior")
    else {
        return;
    };
    eprintln!(
        "a door at {IN_THE_ROOM:?}, two metres in front of the camera, {total} px:
           cell {RECORDED_CELL:#010X} (outdoor, what the client holds): {outdoor} px
           cell 0xA9B40143 (the room that contains it)             : {interior} px"
    );
    // The premise: the door is drawable and the station is pointed at it.
    assert!(
        interior > 5_000,
        "the door paints only {interior} px of {total} even as an interior-cell object, so this \
         station is measuring a cull or a bad viewpoint rather than the draw-order split"
    );
    // Retail does not draw this invalid placement (see the doc comment). This pixel check
    // requires less than one twentieth of the valid interior arm's count; it does not by itself
    // prove destruction or exact zero.
    assert!(
        outdoor * 20 < interior,
        "the door paints {outdoor} px of {total} while holding the outdoor cell -- more than a \
         twentieth of the {interior} px it paints as an interior-cell object. Retail refuses this \
         placement outright (collision rejects world entry and queues object destruction), \
         so a substantial draw here means the placement is being accepted where retail destroys \
         the object."
    );
}

// ---------------------------------------------------------------------------------------------
// A wall hides what stands behind it, with the camera outdoors.
//
// With the viewer outdoors, retail's landscape draws building portals and their visible
// interior cells; this is separate from drawing outward from an indoor viewer. This client's
// server-object pass follows the landscape, with reached-cell membership checked.
//
// `SceneConfig::portal_depth_stamp`'s own note says the stamp is a pre-transformed quad at the
// constant device depth `0.999999`. Drawing interiors before the block's opaque batches and
// nearer blocks can mask a missing stamp; the later server-object pass does not get that
// protection: its depth test must encounter the portal pass's resulting depth.
// ---------------------------------------------------------------------------------------------

/// Outside the house, west of its front door, on the terrain.
const OUTSIDE_THE_HOUSE: Vec3 = Vec3::new(133.00, 5.15, 94.20);
/// Inside room `0xA9B40143`. From `OUTSIDE_THE_HOUSE` the sight line to this point crosses the
/// house's west wall at y ~ 6.5, inside the solid stretch y [6.10, 7.84] between the front doorway
/// `0xA9B40145` and the interior opening `0xA9B40144`.
const INSIDE_BEHIND_THE_WALL: Vec3 = Vec3::new(140.00, 9.00, 94.50);

/// Behaviour: rendering.portals.a-wall-occludes-an-object-inside-from-outdoors
/// No datagram leaves this process.
#[test]
fn the_buildings_wall_occludes_an_object_inside_it_from_outdoors() {
    let store = store();
    let shot = |cell: u32, at: Vec3, tag: &str| -> usize {
        let door = Position::new(CellId(cell), Frame::new(at, DOOR_ROT));
        let with = door_shot_with_state(
            &store,
            RECORDED_CELL,
            OUTSIDE_THE_HOUSE,
            270.0,
            Some(door),
            &format!("{tag}-with"),
        )
        .expect("this rendering regression requires D3D12 WARP");
        assert_eq!(
            with.physical_cell,
            Some(CellId(cell)),
            "{tag}: the object must be placed, not silently rejected"
        );
        assert!(
            dereth_physics::landdefs::is_outdoors(with.camera_cell),
            "{tag}: camera must remain outdoors"
        );
        assert!(
            with.submitted_parts > 0,
            "{tag}: depth occlusion requires a submitted object, not a membership rejection"
        );
        let base = door_shot(
            &store,
            RECORDED_CELL,
            OUTSIDE_THE_HOUSE,
            270.0,
            None,
            &format!("{tag}-without"),
        )
        .expect("the control also requires D3D12 WARP");
        with.rgba
            .chunks_exact(4)
            .zip(base.chunks_exact(4))
            .filter(|(a, b)| a[..3] != b[..3])
            .count()
    };
    // The premise: the same object standing in the open, between the camera and the house.
    let open = shot(RECORDED_CELL, Vec3::new(135.00, 5.15, 94.20), "out-open");
    // The object holds the room's own cell: an outdoor cell paired with an indoor position is
    // refused at placement, and zero pixels from that would prove the refusal, not the depth
    // ordering.
    let through = shot(0xA9B4_0143, INSIDE_BEHIND_THE_WALL, "out-through");
    eprintln!(
        "camera outdoors at {OUTSIDE_THE_HOUSE:?} looking east at the house:
           the object in the open in front of the house: {open} px
           the object inside the room behind its wall  : {through} px"
    );
    assert!(
        open > 5_000,
        "the object paints only {open} px standing in the open in front of the camera, so this \
         station cannot show anything being occluded"
    );
    assert_eq!(
        through, 0,
        "{through} px of an object standing inside the room are painted **through** the house's \
         west wall from outdoors. The interior is drawn by the building portal pass inside \
         the landscape draw and every server object is drawn after the whole landscape, so the object \
         meets the interior only at the depth the portal stamp left"
    );
}
