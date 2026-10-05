//! A villa's doors stay visible: the courtyard and interior doors survive depth testing at a
//! recorded pose, and a continuous walk across the villa does not reuse the building cells drawn
//! before the depth clear. Fixture: the villa landblock `0x9DAF` over the retail dats, its doors
//! decoded from the recorded `house-purchase-and-trade` session, a WARP device and no socket.
#![cfg(gpu)]

use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::{Message, Reader};
use dereth_render::device::{DeviceConfig, Gpu};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

const BLOCK: u16 = 0x9DAF;

// A raw, retransmit-deduplicated blob reconstruction of the recording. Only decoded
// villa door descriptors are retained; login/account messages are never printed or stored.
fn recorded_doors() -> Vec<ObjectCreatePayload> {
    fn field<'a>(line: &'a str, key: &str) -> &'a str {
        let key = format!("\"{key}\"");
        let rest = line
            .split_once(&key)
            .expect("field")
            .1
            .trim_start_matches([' ', ':', '"']);
        &rest[..rest.find(['"', ',', '}']).expect("terminator")]
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/packet-captures/house-purchase-and-trade.jsonl");
    let text = std::fs::read_to_string(path).expect("recorded villa session");
    let mut pending = BTreeMap::<(u16, u64), (u16, BTreeMap<u16, Vec<u8>>)>::new();
    let mut seen = BTreeSet::new();
    let mut doors = BTreeMap::new();
    for line in text.lines().filter(|s| !s.trim().is_empty()) {
        if field(line, "dir") != "s2c" {
            continue;
        }
        let pair: u16 = field(line, "pair").parse().expect("pair");
        let hex = field(line, "data");
        let raw: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
            .collect();
        let Ok(packet) = dereth_transport::wire::ParsedPacket::parse(&raw) else {
            continue;
        };
        if packet.optional.contains_key(&0x0004_0000) {
            seen.clear();
            pending.clear();
        }
        if !packet.fragments.is_empty() && !seen.insert((pair, packet.header.seq_id)) {
            continue;
        }
        for fragment in packet.fragments {
            let key = (pair, fragment.header.blob_id());
            let e = pending
                .entry(key)
                .or_insert_with(|| (fragment.header.num_frags, BTreeMap::new()));
            e.1.insert(fragment.header.blob_num, fragment.payload);
            if e.1.len() != usize::from(e.0) {
                continue;
            }
            let (_, parts) = pending.remove(&key).expect("complete");
            let bytes: Vec<u8> = parts.into_values().flatten().collect();
            if bytes.len() < 4 || bytes[..4] != 0xF745_u32.to_le_bytes() {
                continue;
            }
            let Ok(ItemCreateObject(p)) = ItemCreateObject::read(&mut Reader::new(&bytes[4..]))
            else {
                continue;
            };
            if p.wdesc.name == "Door"
                && p.physicsdesc
                    .position
                    .as_ref()
                    .is_some_and(|p| p.objcell_id >> 16 == u32::from(BLOCK))
            {
                doors.entry(p.id).or_insert(p);
            }
        }
    }
    doors.into_values().collect()
}

#[test]
fn the_paired_villa_station_has_recorded_door_descriptors() {
    let door = recorded_doors()
        .into_iter()
        .find(|d| d.id == ObjectId(0x79DA_F01D))
        .expect("the existing villa recording supplies the actual courtyard door");
    assert_eq!(
        door.physicsdesc.position.as_ref().unwrap().objcell_id,
        0x9DAF_0122
    );
}

struct Shot {
    rgba: Vec<u8>,
    camera: Vec3,
    camera_cell: Option<CellId>,
    shadows: Vec<CellId>,
    parts: usize,
}

fn door_shot(no_draw: bool, interior: bool) -> Shot {
    const W: u32 = 800;
    const H: u32 = 600;
    const DOOR: ObjectId = ObjectId(0x79DA_F01D);
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: W,
            height: H,
            ..DeviceConfig::default()
        },
    )
    .expect("D3D12 WARP required");
    let region = dereth_world_data::landblock::load_region(&store).expect("region");
    let cfg = SceneConfig {
        landblock: BLOCK,
        time_of_day: Some(0.35),
        particles: false,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("villa scene");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("local body");
    let pose = if interior {
        Position::new(
            CellId(0x9DAF_0127),
            Frame::new(
                Vec3::new(134.657_913, 34.905_537, 138.004_990),
                Quat::new(0.499316, 0.0, 0.0, -0.866420),
            ),
        )
    } else {
        Position::new(
            CellId(0x9DAF_002A),
            Frame::new(
                Vec3::new(140.038_086, 28.935_699, 138.004_990),
                Quat::new(-0.960095, 0.0, 0.0, -0.279675),
            ),
        )
    };
    scene.character.as_mut().expect("body").teleport(pose);
    let mut door = recorded_doors()
        .into_iter()
        .find(|p| p.id == DOOR)
        .expect("courtyard door");
    if no_draw {
        door.physicsdesc.state |= dereth_physics::PhysicsState::NODRAW_PS;
    }
    let mut stream = ObjectStream::new();
    stream.apply_event(
        &dereth_client_net::client_session::SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&ItemCreateObject(door)).expect("door body"),
        },
        LocalTime(0.0),
    );
    let mut rgba = Vec::new();
    for frame in 1..=60 {
        let now = LocalTime(f64::from(frame) / 30.0);
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("sync");
        stream.sync_physics_at(
            &store,
            &mut scene.character.as_mut().expect("body").world,
            now,
        );
        scene.update(
            Default::default(),
            CharacterInput::default(),
            now,
            1.0 / 30.0,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            Default::default(),
            now,
            1.0 / 30.0,
        );
        scene.stream(&store, &mut gpu).expect("stream");
        scene.reserve_upload_arena(&mut gpu).expect("arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        if frame == 60 {
            rgba = gpu.capture().expect("capture").to_rgba();
        }
    }
    let character = scene.character.as_ref().expect("body");
    let body = character
        .world
        .by_object_id(DOOR)
        .and_then(|h| character.world.get(h))
        .expect("placed door");
    let shadows = body
        .shadow_objects
        .iter()
        .filter(|s| s.cell_present)
        .map(|s| s.cell_id)
        .collect();
    let parts = scene
        .drawn_part_order()
        .iter()
        .filter(|p| p.object == Some(DOOR))
        .count();
    let station = if interior { "interior" } else { "courtyard" };
    eprintln!("villa door: {station} nodraw={no_draw} origin={:?} shadows={shadows:?} parts={parts} camera={:?}/{:?} cells={:?}",
        body.cell, scene.camera, character.camera.viewer_cell, scene.drawn_cells());
    if let Ok(dir) = std::env::var("DERETH_TEST_VILLA_DOOR_DIR") {
        let path = std::path::Path::new(&dir).join(format!("{station}-nodraw-{no_draw}.png"));
        let file = std::fs::File::create(path).expect("png file");
        let mut png = png::Encoder::new(std::io::BufWriter::new(file), W, H);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .expect("png header")
            .write_image_data(&rgba)
            .expect("png pixels");
    }
    Shot {
        rgba,
        camera: scene.camera.position,
        camera_cell: character.camera.viewer_cell,
        shadows,
        parts,
    }
}

#[test]
/// Behaviour: rendering.interior.a-visible-door-survives-depth-testing-at-the-recorded-pose
fn the_villa_courtyard_door_is_visible_at_the_recorded_pose() {
    assert_visible_door(false);
}

#[test]
fn the_villa_interior_door_is_visible_at_the_recorded_pose() {
    assert_visible_door(true);
}

fn assert_visible_door(interior: bool) {
    let visible = door_shot(false, interior);
    let hidden = door_shot(true, interior);
    assert_eq!(
        visible.camera, hidden.camera,
        "NODRAW retains the physical camera sweep"
    );
    assert_eq!(visible.camera_cell, hidden.camera_cell);
    assert_eq!(visible.shadows, hidden.shadows);
    assert!(
        visible.shadows.contains(&CellId(0x9DAF_002A)),
        "the real door crosses into the courtyard"
    );
    assert_eq!(hidden.parts, 0);
    if !interior {
        assert_eq!(
            visible.camera_cell,
            Some(CellId(0x9DAF_0124)),
            "wait for the camera to settle in the front-gate cell, as in the recorded pose"
        );
    }
    assert!(
        visible.parts > 0,
        "a visible door must reach draw submission"
    );
    let pixels = visible
        .rgba
        .chunks_exact(4)
        .zip(hidden.rgba.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    eprintln!("villa door: interior={interior} door changes {pixels} pixels");
    assert!(
        pixels > 100,
        "retail's clearly visible courtyard door must survive depth testing"
    );
}

/// Settle the production swept camera from the recorded courtyard pose into the gate cell, then walk the real local body toward the villa instead of photographing
/// only the endpoints. The control follows the identical body and camera path with only
/// outdoor-building portal traversal disabled. Its `drawn_cells` therefore come from the main
/// visibility traversal of the current camera cell; cells present only in the live arm were
/// submitted by outdoor-building traversal before that main pass advances the frame stamp and
/// clears depth.
///
/// A door submitted in the *interior* object phase with a present shadow in those pre-clear-only
/// cells and no present shadow in the control or main-visibility cells is solely admitted by the
/// stale frame-wide union. That would make a door flicker as the body crosses, and this station
/// rejects it. It does not replay a recorded player's exact key timestamps; the input is the normal `CharacterInput`
/// forward/run path and the eye is the production swept camera on every frame.
#[test]
fn a_continuous_villa_crossing_does_not_reuse_preclear_building_cells_after_depth_clear() {
    const W: u32 = 800;
    const H: u32 = 600;
    const SETTLE_FRAMES: u32 = 60;
    const FRAMES: u32 = 120;
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: W,
            height: H,
            ..DeviceConfig::default()
        },
    )
    .expect("D3D12 WARP required");
    let region = dereth_world_data::landblock::load_region(&store).expect("region");
    let doors = recorded_doors();
    assert!(
        doors.len() > 4,
        "the recording must supply a villa, not one hand-picked door"
    );

    let build = |building_portals: bool, gpu: &mut Gpu| {
        let cfg = SceneConfig {
            landblock: BLOCK,
            time_of_day: Some(0.35),
            particles: true,
            building_portals,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, gpu, cfg).expect("villa scene");
        scene
            .attach_character(&store, &region, gpu)
            .expect("local body");
        scene
            .character
            .as_mut()
            .expect("body")
            .teleport(Position::new(
                CellId(0x9DAF_002A),
                Frame::new(
                    Vec3::new(140.038_086, 28.935_699, 138.004_990),
                    Quat::new(-0.960095, 0.0, 0.0, -0.279675),
                ),
            ));
        let mut stream = ObjectStream::new();
        for door in doors.iter().cloned() {
            stream.apply_event(
                &dereth_client_net::client_session::SessionEvent::WorldObject {
                    opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                    body: dereth_protocol::write_body(&ItemCreateObject(door)).expect("door body"),
                },
                LocalTime(0.0),
            );
        }
        (scene, stream)
    };
    let (mut subject, mut subject_stream) = build(true, &mut gpu);
    let (mut control, mut control_stream) = build(false, &mut gpu);
    let mut camera_cells = BTreeSet::new();
    let mut contaminated = Vec::new();
    let mut split_frames = 0;
    let mut building_union_frames = 0;
    let mut solely_building_doors: BTreeSet<ObjectId> = BTreeSet::new();
    let mut preclear_env_doors: BTreeSet<ObjectId> = BTreeSet::new();
    let mut preclear_mislit_doors: BTreeSet<ObjectId> = BTreeSet::new();
    let mut preclear_sun_doors: BTreeSet<ObjectId> = BTreeSet::new();
    let mut settled_cell = None;

    for frame in 1..=FRAMES {
        let now = LocalTime(f64::from(frame) / 30.0);
        let input = if frame <= SETTLE_FRAMES {
            CharacterInput::default()
        } else {
            CharacterInput {
                forward: true,
                run: true,
                ..CharacterInput::default()
            }
        };
        let mut draw = |scene: &mut WorldScene, stream: &mut ObjectStream| {
            scene.sync_objects(&store, &mut gpu, stream).expect("sync");
            stream.sync_physics_at(
                &store,
                &mut scene.character.as_mut().expect("body").world,
                now,
            );
            scene.update(Default::default(), input, now, 1.0 / 30.0);
            dereth_client_runtime::camera::update_viewer(
                scene,
                Default::default(),
                now,
                1.0 / 30.0,
            );
            scene.stream(&store, &mut gpu).expect("stream");
            scene.reserve_upload_arena(&mut gpu).expect("arena");
            gpu.begin_frame().expect("begin");
            scene.draw(&mut gpu).expect("draw");
            gpu.end_frame().expect("end");
        };
        draw(&mut control, &mut control_stream);
        // Leave the retail-path subject in the back buffer so the first contaminated frame can be
        // inspected, rather than accidentally saving the differential control.
        draw(&mut subject, &mut subject_stream);
        if [67, 68].contains(&frame) {
            if let Ok(dir) = std::env::var("DERETH_TEST_VILLA_WALK_DIR") {
                let path = std::path::Path::new(&dir).join(format!("crossing-{frame:03}.png"));
                let rgba = gpu.capture().expect("capture").to_rgba();
                let file = std::fs::File::create(path).expect("png file");
                let mut png = png::Encoder::new(std::io::BufWriter::new(file), W, H);
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.write_header()
                    .expect("png header")
                    .write_image_data(&rgba)
                    .expect("png pixels");
            }
        }

        let camera_cell = subject.character.as_ref().expect("body").camera.viewer_cell;
        assert_eq!(
            camera_cell,
            control.character.as_ref().expect("body").camera.viewer_cell
        );
        if let Some(cell) = camera_cell {
            camera_cells.insert(cell);
        }
        if frame == SETTLE_FRAMES {
            settled_cell = camera_cell;
        }
        let split = camera_cell.is_some_and(|cell| !dereth_physics::landdefs::is_outdoors(cell))
            && subject
                .indoor_outside_view_count()
                .is_some_and(|count| count > 0);
        if !split {
            continue;
        }
        split_frames += 1;
        let subject_cells = subject.drawn_cells().expect("subject drew a frame");
        let control_cells = control.drawn_cells().expect("control drew a frame");
        let preclear_only: BTreeSet<u32> =
            subject_cells.difference(&control_cells).copied().collect();
        if preclear_only.is_empty() {
            continue;
        }
        building_union_frames += 1;

        let building_only_doors: BTreeSet<ObjectId> = doors
            .iter()
            .map(|door| door.id)
            .filter(|id| {
                let body = subject
                    .character
                    .as_ref()
                    .expect("body")
                    .world
                    .by_object_id(*id)
                    .and_then(|h| subject.character.as_ref().expect("body").world.get(h));
                body.is_some_and(|body| {
                    let has_preclear_shadow = body.shadow_objects.iter().any(|shadow| {
                        shadow.cell_present && preclear_only.contains(&shadow.cell_id.0)
                    });
                    let has_main_pview_shadow = body.shadow_objects.iter().any(|shadow| {
                        shadow.cell_present && control_cells.contains(&shadow.cell_id.0)
                    });
                    has_preclear_shadow && !has_main_pview_shadow
                })
            })
            .collect();
        let solely_building_this_frame: BTreeSet<ObjectId> = building_only_doors
            .iter()
            .copied()
            .filter(|id| {
                let body = subject
                    .character
                    .as_ref()
                    .expect("body")
                    .world
                    .by_object_id(*id)
                    .and_then(|h| subject.character.as_ref().expect("body").world.get(h))
                    .expect("recorded door body");
                !body.cell.is_some_and(dereth_physics::landdefs::is_outdoors)
                    && !body.shadow_objects.iter().any(|shadow| {
                        shadow.cell_present && dereth_physics::landdefs::is_outdoors(shadow.cell_id)
                    })
            })
            .collect();
        solely_building_doors.extend(&solely_building_this_frame);
        let trace = subject.drawn_part_order();
        for part in &trace {
            let Some(id) = part.object else { continue };
            if part.before_depth_clear && solely_building_this_frame.contains(&id) {
                if part.outdoors {
                    preclear_mislit_doors.insert(id);
                } else {
                    preclear_env_doors.insert(id);
                }
            }
            if part.before_depth_clear
                && part.outdoors
                && building_only_doors.contains(&id)
                && !solely_building_this_frame.contains(&id)
            {
                preclear_sun_doors.insert(id);
            }
        }

        let mut delayed = BTreeSet::new();
        for part in trace.into_iter().filter(|p| !p.before_depth_clear) {
            let Some(id) = part.object else { continue };
            if building_only_doors.contains(&id) {
                delayed.insert(id);
            }
        }
        if !delayed.is_empty() {
            if contaminated.is_empty() {
                if let Ok(dir) = std::env::var("DERETH_TEST_VILLA_WALK_DIR") {
                    let path = std::path::Path::new(&dir).join(format!("frame-{frame:03}.png"));
                    let rgba = gpu.capture().expect("capture").to_rgba();
                    let file = std::fs::File::create(path).expect("png file");
                    let mut png = png::Encoder::new(std::io::BufWriter::new(file), W, H);
                    png.set_color(png::ColorType::Rgba);
                    png.set_depth(png::BitDepth::Eight);
                    png.write_header()
                        .expect("png header")
                        .write_image_data(&rgba)
                        .expect("png pixels");
                }
            }
            contaminated.push((frame, camera_cell, preclear_only, delayed));
        }
    }

    assert_eq!(
        settled_cell,
        Some(CellId(0x9DAF_0124)),
        "the known courtyard pose did not settle in the gate cell"
    );
    assert!(
        camera_cells.contains(&CellId(0x9DAF_002A)),
        "the swept camera never crossed back outdoors: {camera_cells:?}"
    );
    assert!(
        split_frames > 0,
        "no interior frame reached an outdoor portal"
    );
    assert!(
        building_union_frames > 0,
        "the building pass contributed no cells to the preserved frame/pick union"
    );
    assert!(
        preclear_mislit_doors.is_empty(),
        "building-only doors incorrectly used sunlight before the clear: {preclear_mislit_doors:?}"
    );
    assert!(
        !preclear_sun_doors.is_empty(),
        "the actual outdoor-shadow doors did not retain their pre-clear sunlight stage"
    );
    eprintln!(
        "villa walk: solely_building={solely_building_doors:?} env_lit={preclear_env_doors:?} outdoor_shadow_sun={preclear_sun_doors:?}"
    );
    eprintln!(
        "villa walk: camera_cells={camera_cells:?} split_frames={split_frames} contaminated={contaminated:?}"
    );
    assert!(
        contaminated.is_empty(),
        "pre-clear building cells selected post-clear door submissions: {contaminated:?}"
    );
}
