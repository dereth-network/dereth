//! A dungeon's cobwebs are drawn over the creatures and doors standing behind them, with Multiple
//! Pass Alpha on or off. A cobweb is a `Translucent | ClipMap` surface: blended, with no alpha test
//! and no depth write, so it is ordered by nothing but when it is drawn, and it is drawn at the
//! frame's alpha flush, after every object the frame draws in place, from the clip list in its
//! place among the cut-out textures' entries. With the option on, its second pass goes out with
//! the same clip list, still after every object.
//!
//! Fixture: the retail dats; Drudge Hideout's entry room (cell `0x019E0114`), with the camera in the
//! room's south-west corner behind the web strung across it, looking into the room through the web
//! at a drudge or a cage door created on the floor beyond it. Each arm is its own scene, driven for
//! the same number of frames, unlit, with the room's statics on or off. Outdoors, landblock
//! `0x7B98`, whose statics carry sixteen stains of the same class, seen from the flycam.

#![cfg(gpu)]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::frame::V3 as _;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, DataId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
use dereth_scene::world_scene::AlphaDraw;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

pub(crate) const W: u32 = 640;
pub(crate) const H: u32 = 480;

/// Drudge Hideout, and its entry room: a ten-metre room with a cobweb strung across each of three
/// of its upper corners.
const HIDEOUT: u16 = 0x019E;
pub(crate) const ROOM: u32 = 0x019E_0114;

/// The cobweb: one quad, 3.3 m wide and 1 m tall, whose one surface `0x0800013C` is
/// `Translucent | ClipMap` at translucency 0.25.
const WEB: u32 = 0x0100_093B;
const WEB_SURFACE: u32 = 0x0800_013C;
/// Which of the room's webs the station looks through: the one across the south-west corner.
const WEB_NEAR: Vec3 = Vec3::new(6.642, -43.285, 2.5);

/// A drudge, setup `0x020007DD`: every surface of it opaque.
const DRUDGE: u32 = 0x0200_07DD;
/// The hideout's cage door, setup `0x02000281`: its bars are the pure `ClipMap` surface
/// `0x080004BE` (alpha-tested, depth written), the rest of it opaque.
pub(crate) const CAGE_DOOR: u32 = 0x0200_0281;
const BARS_SURFACE: u32 = 0x0800_04BE;
const SUBJECT: ObjectId = ObjectId(0x8300_0051);

/// How far behind the web the camera stands, and how far beyond it the subject does, along the
/// web's normal.
const CAMERA_BEHIND: f32 = 0.5;
const SUBJECT_BEYOND: f32 = 2.5;
/// Eye heights: through the web (its quad spans 2.0 to 3.0 m), and under it for the clear arm.
const EYE_THROUGH: f32 = 2.9;
const EYE_UNDER: f32 = 1.55;
/// Where the body stands unless it is the subject: in the room, to one side of the subject, and
/// in every arm alike.
const BODY_AT: Vec3 = Vec3::new(12.6, -43.4, 0.005);

pub(crate) fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// The station's web, read out of the cell dat: the `WEB` static of [`ROOM`] at [`WEB_NEAR`].
fn near_web(store: &RetailDatStore) -> Frame {
    let d = dereth_world_data::env_cells::EnvCellLoader::new()
        .load_cell(store, CellId(ROOM))
        .expect("the hideout's entry room decodes");
    let webs: Vec<Frame> = dereth_world_data::env_cells::cell_statics(&d)
        .into_iter()
        .filter(|s| s.id == DataId(WEB))
        .map(|s| s.frame)
        .collect();
    assert_eq!(webs.len(), 3, "the entry room holds three webs");
    *webs
        .iter()
        .find(|f| {
            let d = f.origin.sub(WEB_NEAR);
            d.dot(d) < 0.01
        })
        .unwrap_or_else(|| panic!("no web at {WEB_NEAR:?} any more: {webs:?}"))
}

/// What one arm photographs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Subject {
    Nothing,
    Drudge,
    Door,
    /// The body itself, standing where the others are created, facing the camera.
    Body,
}

/// One arm of the station.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Arm {
    pub multi_pass_alpha: bool,
    /// The room's statics, the web among them.
    pub statics: bool,
    pub subject: Subject,
    /// The camera looks through the web (`true`) or under it.
    pub through: bool,
    /// Fixed-function lighting. The room's torches are statics, so the differentials run unlit:
    /// lit, turning the statics off would relight every pixel of the room.
    pub lit: bool,
}

/// One arm's last frame, and how many of the subject's subsets the frame submitted.
pub(crate) struct Shot {
    pub rgba: Vec<u8>,
    pub subject_subsets: usize,
    pub alpha_order: Vec<AlphaDraw>,
    pub body_cell: CellId,
}

const FRAMES: usize = 24;
const PLACE_ON: usize = 6;

/// The camera's eye and the point it looks at, and where the subject stands, for an arm.
fn geometry(web: &Frame, through: bool) -> (Vec3, Vec3, Vec3) {
    // The web's normal is its local y axis, and it points into the corner it is strung across.
    let n = dereth_primitives::frame::localtoglobal(
        &Frame::new(Vec3::ZERO, web.rotation),
        Vec3::new(0.0, 1.0, 0.0),
    );
    let n = Vec3::new(n.x, n.y, 0.0);
    let eye_z = if through { EYE_THROUGH } else { EYE_UNDER };
    let eye = Vec3::new(
        web.origin.x + n.x * CAMERA_BEHIND,
        web.origin.y + n.y * CAMERA_BEHIND,
        eye_z,
    );
    let at = Vec3::new(
        web.origin.x - n.x * SUBJECT_BEYOND,
        web.origin.y - n.y * SUBJECT_BEYOND,
        0.005,
    );
    let target = Vec3::new(at.x, at.y, 0.75);
    (eye, target, at)
}

fn facing(from: Vec3, to: Vec3) -> Quat {
    let yaw = math::atan2f(-(to.x - from.x), to.y - from.y);
    Quat::new(math::cosf(yaw * 0.5), 0.0, 0.0, math::sinf(yaw * 0.5))
}

/// Create the subject at `at`, facing the camera at `eye`.
fn place(objects: &mut ObjectStream, setup: u32, at: Vec3, eye: Vec3, now: f64) {
    let rotation = facing(at, eye);
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id: SUBJECT,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            setup_id: Some(setup),
            state: 0,
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: ROOM,
                frame: dereth_protocol::types::Frame {
                    origin: at.into(),
                    orientation: rotation.into(),
                },
            }),
            timestamps: dereth_protocol::types::PhysicsTimestamps {
                instance: 1,
                ..dereth_protocol::types::PhysicsTimestamps::default()
            },
            ..PhysicsDesc::default()
        },
        wdesc: PublicWeenieDesc::default(),
    };
    let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
        .expect("encode");
    objects.apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(now),
    );
    objects.world.update_visible_object_list();
}

/// One arm: its own scene and device, driven for [`FRAMES`] frames with the camera held at the
/// station, and its last frame captured.
pub(crate) fn shot(store: &Arc<RetailDatStore>, arm: Arm) -> Shot {
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let mut cfg = SceneConfig {
        landblock: HIDEOUT,
        start_cell: Some(CellId(ROOM)),
        time_of_day: Some(0.5),
        particles: false,
        cell_statics: arm.statics,
        object_lighting: arm.lit,
        ..SceneConfig::default()
    };
    cfg.render.multi_pass_alpha = arm.multi_pass_alpha;
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    let web = near_web(store);
    let (eye, target, at) = geometry(&web, arm.through);
    {
        let c = scene.character.as_mut().expect("a body");
        c.land()
            .load_block_cells(dereth_primitives::LandblockId(HIDEOUT));
        let body = if arm.subject == Subject::Body {
            Frame::new(at, facing(at, eye))
        } else {
            Frame::new(BODY_AT, Quat::IDENTITY)
        };
        c.teleport(Position::new(CellId(ROOM), body));
    }
    let d = target.sub(eye);
    let len = d.dot(d).sqrt();
    let mut objects = ObjectStream::new();
    let mut now = 0.0f64;
    let mut rgba = Vec::new();
    for i in 0..FRAMES {
        now += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        if i == PLACE_ON {
            match arm.subject {
                Subject::Nothing | Subject::Body => {}
                Subject::Drudge => place(&mut objects, DRUDGE, at, eye, now),
                Subject::Door => place(&mut objects, CAGE_DOOR, at, eye, now),
            }
        }
        scene
            .sync_objects(store, &mut gpu, &mut objects)
            .expect("sync_objects");
        // The camera is held at the station: the body's own chase camera is not run, and its
        // viewer, which every part's distance is measured from, is put at the station too.
        {
            let c = scene.character.as_mut().expect("a body");
            c.camera.viewer = Position::new(CellId(ROOM), Frame::new(eye, facing(eye, target)));
            c.camera.viewer_cell = Some(CellId(ROOM));
        }
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
        scene.camera.position = eye;
        scene.camera.yaw = math::atan2f(-d.x / len, d.y / len);
        scene.camera.pitch = math::asinf(d.z / len);
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    let subject_subsets = scene
        .drawn_part_order()
        .iter()
        .filter(|e| e.object == Some(SUBJECT))
        .count();
    let body_cell = scene.character.as_ref().expect("a body").position().cell;
    let alpha_order = scene.drawn_alpha_order();
    scene.release_textures(&mut gpu);
    Shot {
        rgba,
        subject_subsets,
        alpha_order,
        body_cell,
    }
}

/// The pixels at which two frames differ.
fn differs(a: &[u8], b: &[u8]) -> Vec<bool> {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .map(|(p, q)| p[..3] != q[..3])
        .collect()
}

fn count(m: &[bool]) -> usize {
    m.iter().filter(|x| **x).count()
}

fn and(a: &[bool], b: &[bool]) -> Vec<bool> {
    a.iter().zip(b).map(|(x, y)| *x && *y).collect()
}

fn and_not(a: &[bool], b: &[bool]) -> Vec<bool> {
    a.iter().zip(b).map(|(x, y)| *x && !*y).collect()
}

/// The differential over one subject, with the camera looking through the web.
///
/// * `subject`: the subject's own pixels, where the frame with it differs from the frame without
///   it, both with the room's statics off;
/// * `web`: the statics' footprint, where the frame with them differs from the frame without them,
///   both without the subject;
/// * `covered`: the subject's pixels the statics' footprint lies over, of which `changed` differ
///   between the frame with the statics and the frame without them, both with the subject;
/// * `clear_changed`: the subject's pixels the footprint does not lie over that differ the same
///   way -- the control, which must be a sliver.
///
/// The arms are unlit: the room's torches are statics too, and lit, turning the statics off would
/// relight every pixel.
struct Covered {
    subject: usize,
    web: usize,
    covered: usize,
    changed: usize,
    clear_changed: usize,
    submitted: (usize, usize),
}

fn covered(store: &Arc<RetailDatStore>, multi_pass_alpha: bool, subject: Subject) -> Covered {
    let arm = |statics, s| Arm {
        multi_pass_alpha,
        statics,
        subject: s,
        through: true,
        lit: false,
    };
    let bare = shot(store, arm(false, Subject::Nothing));
    let alone = shot(store, arm(false, subject));
    let room = shot(store, arm(true, Subject::Nothing));
    let behind = shot(store, arm(true, subject));
    for s in [&bare, &alone, &room, &behind] {
        assert_eq!(s.body_cell, CellId(ROOM), "the body stands in the room");
    }
    let subject_px = differs(&bare.rgba, &alone.rgba);
    let web_px = differs(&bare.rgba, &room.rgba);
    let changed_px = differs(&alone.rgba, &behind.rgba);
    let over = and(&subject_px, &web_px);
    let clear = and_not(&subject_px, &web_px);
    Covered {
        subject: count(&subject_px),
        web: count(&web_px),
        covered: count(&over),
        changed: count(&and(&over, &changed_px)),
        clear_changed: count(&and(&clear, &changed_px)),
        submitted: (alone.subject_subsets, behind.subject_subsets),
    }
}

/// The premises every differential below stands on, before its claim.
fn premises(c: &Covered, what: &str) {
    assert!(
        c.submitted.0 > 0 && c.submitted.0 == c.submitted.1,
        "the {what} is submitted alike with and without the room's statics: {:?}",
        c.submitted
    );
    assert!(
        c.subject > 2_000,
        "the {what} paints only {} pixels; the station measures nothing",
        c.subject
    );
    assert!(
        c.web > 20_000,
        "the room's statics paint only {} pixels; the web is not in view",
        c.web
    );
    assert!(
        c.covered > 500,
        "the web lies over only {} of the {what}'s {} pixels; it is not between the camera and \
         the {what}",
        c.covered,
        c.subject
    );
    // A strand texel faint enough to round away over the wall can still show over the subject,
    // and a cut-out's soft edge blends over whatever stands behind it; neither is more than a
    // sliver of the subject.
    assert!(
        c.clear_changed * 100 <= c.subject,
        "{} of the {what}'s {} pixels the room's statics do not lie over change with them; \
         something other than the web stands in front of it",
        c.clear_changed,
        c.subject
    );
}

/// One surface's type and the state the renderer resolves it to, for a static placed in a cell.
fn resolved(store: &RetailDatStore, id: u32) -> (u32, dereth_render::PipelineKey) {
    use dereth_assets::Decode;
    let b = store
        .read_typed(dereth_dat::DbType::Surface, DataId(id))
        .expect("the surface record");
    let s = <dereth_assets::material::Surface as Decode>::decode_payload(DataId(id), &b)
        .expect("the surface decodes");
    let state = dereth_render::surface::Surface {
        r#type: s.surface_type,
        handler: dereth_render::surface::SurfaceHandler::Database,
        color_value: s.color_value.unwrap_or(0),
        translucency: s.translucency,
        luminosity: s.luminosity,
        diffuse: s.diffuse,
    };
    let ctx = dereth_render::SurfaceContext {
        vertex_format: dereth_render::VertexFormat::XyzNormalDiffuseTex1,
        texture_is_set: true,
        lighting: true,
        ..dereth_render::SurfaceContext::default()
    };
    (
        s.surface_type,
        dereth_render::PipelineKey::from_surface(&state, ctx).0,
    )
}

/// **The oracle for the stations below: the web blends and writes no depth, the cage door's bars
/// are a cut-out that writes depth, and both are clip-mapped subsets.**
#[test]
fn the_web_blends_without_writing_depth_and_the_cage_door_s_bars_are_a_cut_out() {
    use dereth_assets::Decode;
    use dereth_world_render::objects::draw::subset_mask;
    let store = store();
    let b = store
        .read_typed(dereth_dat::DbType::GfxObj, DataId(WEB))
        .expect("the web's graphics object");
    let web = <dereth_assets::geometry::GfxObj as Decode>::decode_payload(DataId(WEB), &b)
        .expect("the web decodes");
    assert_eq!(
        web.surfaces,
        vec![DataId(WEB_SURFACE)],
        "the web is one surface"
    );
    let (t, k) = resolved(&store, WEB_SURFACE);
    eprintln!("web surface {WEB_SURFACE:#010X}: type {t:#x}, {k:?}");
    assert_eq!(subset_mask(t), 8, "the web is a clip-mapped subset");
    assert!(
        k.alpha_blend && !k.alpha_test && !k.z_write,
        "the web's surface blends without an alpha test and writes no depth: {k:?}"
    );
    let (t, k) = resolved(&store, BARS_SURFACE);
    eprintln!("bars surface {BARS_SURFACE:#010X}: type {t:#x}, {k:?}");
    assert_eq!(subset_mask(t), 8, "the bars are a clip-mapped subset");
    assert!(
        k.alpha_test && k.z_write,
        "the bars are an alpha-tested cut-out that writes depth: {k:?}"
    );
    // And the station's web is where the station stands behind it.
    let _ = near_web(&store);
}

/// Behaviour: rendering.draw-order.a-dungeon-web-is-drawn-over-what-stands-behind-it
/// A creature standing behind a cobweb is seen through the web, with Multiple Pass Alpha on and
/// off: the web changes the creature's pixels it lies over.
#[test]
fn a_cobweb_is_drawn_over_a_creature_standing_behind_it_with_the_option_on_and_off() {
    let store = store();

    // The control, first and on its own: two identically driven scenes agree to the pixel.
    let arm = Arm {
        multi_pass_alpha: true,
        statics: true,
        subject: Subject::Nothing,
        through: true,
        lit: false,
    };
    let a = shot(&store, arm);
    let b = shot(&store, arm);
    let floor = count(&differs(&a.rgba, &b.rgba));
    eprintln!("web station control: {floor} of {} pixels", W * H);
    assert_eq!(
        floor, 0,
        "two identically driven scenes do not agree; nothing below means anything"
    );

    // The creature in the clear: seen under the web, the room's statics change none of its pixels.
    let clear = |statics, subject| {
        shot(
            &store,
            Arm {
                multi_pass_alpha: true,
                statics,
                subject,
                through: false,
                lit: false,
            },
        )
    };
    let (bare, alone, room, beside) = (
        clear(false, Subject::Nothing),
        clear(false, Subject::Drudge),
        clear(true, Subject::Nothing),
        clear(true, Subject::Drudge),
    );
    let subject_px = differs(&bare.rgba, &alone.rgba);
    let web_px = differs(&bare.rgba, &room.rgba);
    let changed = count(&and(&subject_px, &differs(&alone.rgba, &beside.rgba)));
    eprintln!(
        "web station, the creature in the clear: {} pixels, {} under the statics' footprint, {changed} \
         changed by the statics",
        count(&subject_px),
        count(&and(&subject_px, &web_px)),
    );
    assert!(
        count(&subject_px) > 2_000,
        "the creature in the clear paints only {} pixels",
        count(&subject_px)
    );
    assert_eq!(
        changed, 0,
        "nothing stands between the camera and the creature in the clear, yet the room's statics \
         change {changed} of its pixels"
    );

    for multi_pass_alpha in [false, true] {
        let c = covered(&store, multi_pass_alpha, Subject::Drudge);
        eprintln!(
            "web station, Multiple Pass Alpha {multi_pass_alpha}: the creature {} pixels \
             ({:?} subsets), the statics {}, the web over the creature {}, of them {} changed; \
             {} changed outside the web",
            c.subject, c.submitted, c.web, c.covered, c.changed, c.clear_changed
        );
        premises(&c, "creature");
        // **The claim.** The web is drawn over the creature where it lies over it.
        //
        // Measured: 3,033 of the 3,078 covered pixels change with the option on and off alike,
        // and with the option on and the web drawn under the creature, none of them does: the
        // creature is opaque and paints over a surface that wrote no depth. The covered pixels
        // count every static the creature stands in front of as well as the web in front of it,
        // and a strand texel can blend to the colour already there, so the share is not 1.
        #[allow(clippy::cast_precision_loss)] // LINT-OK: pixel counts under 307,200.
        let share = c.changed as f32 / c.covered as f32;
        assert!(
            share > 0.5,
            "Multiple Pass Alpha {multi_pass_alpha}: the web changes {share:.3} of the creature's \
             pixels it lies over ({} of {}); it is drawn under the creature",
            c.changed,
            c.covered
        );
    }
}

/// Behaviour: rendering.draw-order.a-dungeon-web-is-drawn-over-what-stands-behind-it
/// A cage door whose bars are a pure cut-out texture, standing behind a cobweb, is seen through
/// the web with Multiple Pass Alpha on: the bars write depth, and the web is drawn after them.
#[test]
fn a_cobweb_is_drawn_over_a_cage_door_s_bars_behind_it_with_multiple_pass_alpha_on() {
    let store = store();
    let c = covered(&store, true, Subject::Door);
    eprintln!(
        "web station, the cage door: {} pixels ({:?} subsets), the statics {}, the web over the \
         door {}, of them {} changed; {} changed outside the web",
        c.subject, c.submitted, c.web, c.covered, c.changed, c.clear_changed
    );
    premises(&c, "door");
    // **The claim.** Measured: 29,913 of the 33,723 covered pixels change. Drawn under the door,
    // the web changes 3,471 of them, the soft edge of the bars that the door's own second pass
    // blends over the strands behind it; everywhere the bars are solid the door paints over the
    // web. Much of the door stands in front of the room's far statics, which count as covered too.
    #[allow(clippy::cast_precision_loss)] // LINT-OK: pixel counts under 307,200.
    let share = c.changed as f32 / c.covered as f32;
    assert!(
        share > 0.5,
        "the web changes {share:.3} of the door's pixels it lies over ({} of {}); it is drawn \
         under the door",
        c.changed,
        c.covered
    );
}

/// Behaviour: rendering.draw-order.a-dungeon-web-is-drawn-over-what-stands-behind-it
/// With Multiple Pass Alpha on, a cell static's second pass goes out with the object pass's clip
/// list: after every object that pass draws in place, and before the first draw of its alpha list.
#[test]
fn a_cell_static_s_second_pass_goes_out_with_the_object_pass_s_clip_list() {
    let store = store();
    let s = shot(
        &store,
        Arm {
            multi_pass_alpha: true,
            statics: true,
            subject: Subject::Door,
            through: true,
            lit: false,
        },
    );
    let order = &s.alpha_order;
    let starts: Vec<usize> = order
        .iter()
        .enumerate()
        .filter(|(_, d)| **d == AlphaDraw::FlushStart)
        .map(|(i, _)| i)
        .collect();
    let forced: Vec<usize> = order
        .iter()
        .enumerate()
        .filter(|(_, d)| **d == AlphaDraw::CellForced)
        .map(|(i, _)| i)
        .collect();
    let count = |k: AlphaDraw| order.iter().filter(|d| **d == k).count();
    eprintln!(
        "web station trace: {} draws, {} flushes, {} cell second passes, {} part clip entries, \
         {} part second passes, {} blended statics, {} part blends",
        order.len(),
        starts.len(),
        forced.len(),
        count(AlphaDraw::PartClip),
        count(AlphaDraw::PartForced),
        count(AlphaDraw::StaticBlend),
        count(AlphaDraw::PartBlend),
    );
    // The premises: one flush, the object pass's, and the door's bars on its clip list.
    assert_eq!(
        starts.len(),
        1,
        "a sealed dungeon room's frame has one alpha flush, its object pass's: {order:?}"
    );
    assert!(
        count(AlphaDraw::PartForced) > 0,
        "the door's bars put no second pass on the clip list: {order:?}"
    );
    // **The claim.** The room's clip-mapped statics' second passes are drawn out of that flush,
    // so after every object the pass drew in place -- not inside the cell walk before them.
    assert!(
        !forced.is_empty(),
        "no cell static's second pass was drawn from the object pass's clip list: {order:?}"
    );
    let first_blend = order
        .iter()
        .position(|d| !d.is_clip_list() && *d != AlphaDraw::FlushStart)
        .unwrap_or(order.len());
    assert!(
        forced.iter().all(|&i| i > starts[0] && i < first_blend),
        "a cell static's second pass is drawn outside its flush's clip list: {order:?}"
    );
}

/// Behaviour: rendering.draw-order.a-cobweb-goes-on-the-clip-list-with-the-cut-outs
/// A cobweb's own draw goes on the clip list with Multiple Pass Alpha on and off, in its place
/// among the cut-out parts' entries: the web nearer than a cage door's bars after the bars, and
/// every clip-list entry before the first draw of the alpha list.
#[test]
fn a_cobweb_s_own_draw_goes_on_the_clip_list_in_its_place_among_the_cut_outs() {
    let store = store();
    for multi_pass_alpha in [false, true] {
        let s = shot(
            &store,
            Arm {
                multi_pass_alpha,
                statics: true,
                subject: Subject::Door,
                through: true,
                lit: false,
            },
        );
        let order = &s.alpha_order;
        let at = |k: AlphaDraw| -> Vec<usize> {
            order
                .iter()
                .enumerate()
                .filter(|(_, d)| **d == k)
                .map(|(i, _)| i)
                .collect()
        };
        // The door's bars: the clip list's own entries with the option off, its second passes
        // with it on.
        let bars = at(if multi_pass_alpha {
            AlphaDraw::PartForced
        } else {
            AlphaDraw::PartClip
        });
        let webs = at(AlphaDraw::StaticClip);
        let first_blend = order
            .iter()
            .position(|d| !d.is_clip_list() && *d != AlphaDraw::FlushStart)
            .unwrap_or(order.len());
        eprintln!(
            "web station trace, Multiple Pass Alpha {multi_pass_alpha}: bars at {bars:?}, the \
             statics' own clip-list draws at {webs:?}, the alpha list from {first_blend} of {}",
            order.len()
        );
        assert!(!bars.is_empty(), "the door's bars are not on the clip list");
        // **The claim.** The webs' own draws are clip-list draws, not alpha-list ones.
        assert!(
            !webs.is_empty(),
            "Multiple Pass Alpha {multi_pass_alpha}: no static's own draw went on the clip \
             list: {order:?}"
        );
        assert!(
            webs.iter().all(|&i| i < first_blend),
            "Multiple Pass Alpha {multi_pass_alpha}: a clip-list draw follows the alpha list: \
             {order:?}"
        );
        // In its place among the cut-outs: the room's webs, the nearest of them half a metre
        // from the camera, are drawn after the bars three metres away.
        let last_bar = *bars.last().expect("bars");
        assert!(
            webs.iter().any(|&i| i > last_bar),
            "Multiple Pass Alpha {multi_pass_alpha}: the room's webs are all drawn before the \
             bars behind them: {order:?}"
        );
    }
}

/// Behaviour: rendering.draw-order.a-cobweb-goes-on-the-clip-list-with-the-cut-outs
/// With Multiple Pass Alpha off a cage door's bars are drawn from the clip list, writing depth;
/// a cobweb nearer than them on the same list is drawn after them, over them.
#[test]
fn with_the_option_off_a_cobweb_nearer_than_a_cage_door_s_bars_is_drawn_over_them() {
    let store = store();
    let c = covered(&store, false, Subject::Door);
    eprintln!(
        "web station, the cage door with the option off: {} pixels ({:?} subsets), the statics \
         {}, the web over the door {}, of them {} changed; {} changed outside the web",
        c.subject, c.submitted, c.web, c.covered, c.changed, c.clear_changed
    );
    premises(&c, "door");
    #[allow(clippy::cast_precision_loss)] // LINT-OK: pixel counts under 307,200.
    let share = c.changed as f32 / c.covered as f32;
    assert!(
        share > 0.5,
        "the web changes {share:.3} of the door's pixels it lies over ({} of {}); the bars are \
         drawn over it",
        c.changed,
        c.covered
    );
}

/// A landblock whose outdoor statics include sixteen of the floor stains `0x010017B7` and
/// `0x010017B8`, the same `Translucent | ClipMap` class as the cobweb.
const STAINED_BLOCK: u16 = 0x7B98;

/// On one settled frame over [`STAINED_BLOCK`], from the flycam: the landscape's blended batches
/// drawn by an alpha flush, those drawn inside the block walk, and those resident.
fn landscape_flushed(store: &Arc<RetailDatStore>, multi_pass_alpha: bool) -> (usize, usize, usize) {
    let mut gpu = crate::common::test_gpu(W, H);
    let mut cfg = SceneConfig {
        landblock: STAINED_BLOCK,
        character: false,
        land_radius: 1,
        scenery_radius: 1,
        time_of_day: Some(0.5),
        particles: false,
        ..SceneConfig::default()
    };
    cfg.render.multi_pass_alpha = multi_pass_alpha;
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    let mut objects = ObjectStream::new();
    let mut now = 0.0f64;
    for _ in 0..8 {
        now += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(store, &mut gpu, &mut objects)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
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
    }
    let s = scene.drawn_landscape_alpha();
    let resident = scene.alpha_list_batches();
    scene.release_textures(&mut gpu);
    (s.blend + s.blend_early, s.blend_in_walk, resident)
}

/// Behaviour: rendering.draw-order.a-dungeon-web-is-drawn-over-what-stands-behind-it
/// Outdoors as well, a `Translucent | ClipMap` static's own draw waits for an alpha flush with
/// Multiple Pass Alpha on, as with it off: the option adds a second pass, moves nothing out of the
/// flush and draws nothing blended inside the block walk.
#[test]
fn an_outdoor_cut_out_translucent_static_s_own_draw_waits_for_the_flush_with_the_option_on() {
    let store = store();
    let (off, off_walk, resident) = landscape_flushed(&store, false);
    let (on, on_walk, _) = landscape_flushed(&store, true);
    eprintln!(
        "stained block {STAINED_BLOCK:#06X}: {resident} blended landscape batches resident; \
         {off} drawn by a flush with the option off, {on} with it on; {off_walk} and {on_walk} \
         drawn inside the walk"
    );
    assert!(off > 0, "no blended landscape batch was drawn by a flush");
    assert_eq!(
        on,
        off,
        "with Multiple Pass Alpha on, {} blended landscape batches were drawn outside an alpha \
         flush",
        off.saturating_sub(on)
    );
    assert_eq!(
        (off_walk, on_walk),
        (0, 0),
        "blended landscape batches were drawn inside the block walk, under the objects"
    );
}
