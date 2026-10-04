//! Panel previews draw graphics-object level zero of every part, which is not necessarily the
//! setup part's own id. A preview pass disables degrading, so the level selector picks entry zero
//! of each part's degrade record before distance or preferences are consulted; the part's box is
//! read from the same entry. For the Aluvian male body (setup `0x02000001`) 16 of 34 parts have a
//! higher-detail mesh at level zero and their own id at level one (772 triangles against 417).
//! The four preview surfaces (paper doll, identify-window model, character-generation turntable,
//! portal space) share one bake; the portal background `0x02000306` has no differing part and is
//! the unchanged-geometry control. Fixture: the retail dats, read independently of the preview
//! selection helper, and the recording first-login-walk-jump for the paper doll's player; baked
//! ids and triangle counts are observed, not pixels. No datagram leaves the process.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use std::sync::Arc;

use dereth_animation::parts::PhysicsPart;
use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo, Setup};
use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::gpu::PreviewId;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::preview::PreviewObject;
use dereth_client_net::client_session::testing::capture::{self, peer as addr, Datagram as Record};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording;
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::BBoxExt;
use dereth_primitives::{DataId, LocalTime, Vec3};
use dereth_ui_screens::screens::gameplay::{window::INVENTORY_PAGE, GamePlayScreen};

const SESSION: &str = "first-login-walk-jump";

/// Setup 0x02000001 is the Aluvian male body used by the recorded player and these previews.
const BODY: DataId = DataId(0x0200_0001);

/// The shipped UI asset mapping resolves enum 0x10000001 to this portal background setup.
/// The mapping is asserted in the_portal_space_is_the_control, not assumed from this constant.
const PORTAL_BACKGROUND: DataId = DataId(0x0200_0306);

// =================================================================================================
// The archive oracle: independently select each part's first degrade entry
// =================================================================================================

fn store() -> Arc<RetailDatStore> {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    Arc::new(dereth_client::assets::open_data_files(&d).expect("the retail dats open"))
}

fn gfxobj(store: &RetailDatStore, id: DataId) -> Option<GfxObj> {
    let b = store.read_typed(DbType::GfxObj, id).ok()?;
    GfxObj::decode_payload(id, &b).ok()
}

/// Resolve level zero directly from archive data rather than the preview-selection helper.
/// Missing/undecodable objects or degrade records, and empty entry lists, fall back to the part
/// ID here. The body calibration below ensures the relevant differing records are actually read.
fn level_zero(store: &RetailDatStore, part: DataId) -> DataId {
    let Some(g) = gfxobj(store, part) else {
        return part;
    };
    let Some(did) = g.did_degrade else {
        return part;
    };
    let Ok(b) = store.read_typed(DbType::DegradeInfo, did) else {
        return part;
    };
    let Ok(rec) = GfxObjDegradeInfo::decode_payload(did, &b) else {
        return part;
    };
    rec.degrades.first().map_or(part, |e| e.gfxobj_id)
}

/// Count drawing polygons as the production triangulator does: an n-gon contributes n - 2
/// triangles for n >= 3, otherwise zero. This is a vertex-count oracle, not a pixel readback.
fn triangles(store: &RetailDatStore, id: DataId) -> usize {
    gfxobj(store, id).map_or(0, |o| {
        o.polygons
            .iter()
            .filter(|p| p.num_pts >= 3)
            .map(|p| p.num_pts as usize - 2)
            .sum()
    })
}

/// Compute min/max over decoded graphics-object vertices; an empty array has no box.
fn bound_box(store: &RetailDatStore, id: DataId) -> Option<(Vec3, Vec3)> {
    let g = gfxobj(store, id)?;
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for v in &g.vertex_array.vertices {
        lo = Vec3::new(
            lo.x.min(v.position.x),
            lo.y.min(v.position.y),
            lo.z.min(v.position.z),
        );
        hi = Vec3::new(
            hi.x.max(v.position.x),
            hi.y.max(v.position.y),
            hi.z.max(v.position.z),
        );
    }
    (lo.x <= hi.x).then_some((lo, hi))
}

fn setup_parts(store: &RetailDatStore, id: DataId) -> Vec<DataId> {
    let b = store
        .read_typed(DbType::Setup, id)
        .expect("the setup record is in the dat");
    Setup::decode_payload(id, &b)
        .expect("the setup record decodes")
        .parts
}

/// Expected baked IDs: level zero of each current part after its object description was applied.
fn wanted(store: &RetailDatStore, parts: &[PhysicsPart]) -> Vec<DataId> {
    parts
        .iter()
        .map(|p| level_zero(store, p.gfxobj_id))
        .collect()
}

/// Compare baked-from IDs and mesh triangle counts with archive-derived level-zero results.
/// Also report the own-ID total; the separate calibration establishes a nonconstant body case.
fn assert_draws_level_zero(store: &RetailDatStore, o: &PreviewObject, who: &str) {
    let want = wanted(store, &o.part_array.parts);
    let model: Vec<DataId> = o.part_array.parts.iter().map(|p| p.gfxobj_id).collect();
    let wrong: Vec<usize> = (0..want.len())
        .filter(|&i| o.built_from().get(i) != want.get(i))
        .collect();
    let want_tris: usize = want.iter().map(|&id| triangles(store, id)).sum();
    let model_tris: usize = model.iter().map(|&id| triangles(store, id)).sum();
    println!(
        "{who}: {} parts, {} name a level 0 that is not their own id; gfxobj[0] is \
         {want_tris} triangles, the setup's own ids are {model_tris}; the GPU holds {}",
        want.len(),
        (0..want.len()).filter(|&i| want[i] != model[i]).count(),
        o.triangles()
    );
    assert!(
        wrong.is_empty(),
        "{who}: {} of {} parts were baked from the part's own graphics object instead of gfxobj[0] \
         (preview geometry must use \
         the level-zero graphics object); first offender is part {} -- baked {:#010X}, \
         gfxobj[0] is {:#010X}",
        wrong.len(),
        want.len(),
        wrong[0],
        o.built_from()[wrong[0]].0,
        want[wrong[0]].0
    );
    assert_eq!(
        o.triangles(),
        want_tris,
        "{who}: the geometry on the GPU is not gfxobj[0]'s ({want_tris} triangles); the setup's \
         own part ids would be {model_tris}"
    );
}

// =================================================================================================
// Station 0 — the instrument, calibrated against the file
// =================================================================================================

/// Calibrate the body discriminator from archive data: 34 parts, 16 differing level-zero IDs,
/// a strictly larger aggregate triangle count, and 16 own IDs present at level one. Selection
/// and counting are independent of the preview helper but use shared asset decoders. If these
/// records were flattened, this calibration must fail rather than leave later checks vacuous.
#[test]
fn the_dat_puts_the_bodys_high_poly_mesh_at_level_zero_and_the_setups_own_part_at_level_one() {
    let store = store();
    let parts = setup_parts(&store, BODY);
    assert_eq!(
        parts.len(),
        34,
        "setup record 0x02000001 is the 34-part Aluvian male body"
    );

    let zero: Vec<DataId> = parts.iter().map(|&p| level_zero(&store, p)).collect();
    let differ: Vec<usize> = (0..parts.len()).filter(|&i| zero[i] != parts[i]).collect();
    let own_tris: usize = parts.iter().map(|&p| triangles(&store, p)).sum();
    let zero_tris: usize = zero.iter().map(|&p| triangles(&store, p)).sum();
    println!(
        "oracle: {} of 34 parts of {:#010X} have a level 0 that is not their own graphics object; \
         own ids {own_tris} triangles, gfxobj[0] {zero_tris}",
        differ.len(),
        BODY.0
    );
    assert_eq!(
        differ.len(),
        16,
        "the shipped dat puts 16 of the body's parts behind a swap"
    );
    assert!(
        zero_tris > own_tris,
        "level 0 must be the *higher* detail mesh or 'the doll draws the lowest LOD' has no \
         meaning: {zero_tris} vs {own_tris}"
    );

    // Count own IDs at level one as an additional calibration. Together with the differing-ID
    // count, this shows the own ids are exactly one step down for this body.
    let mut at_one = 0;
    for &p in &parts {
        let Some(g) = gfxobj(&store, p) else { continue };
        let Some(did) = g.did_degrade else { continue };
        let Ok(b) = store.read_typed(DbType::DegradeInfo, did) else {
            continue;
        };
        let Ok(rec) = GfxObjDegradeInfo::decode_payload(did, &b) else {
            continue;
        };
        if rec.degrades.get(1).is_some_and(|e| e.gfxobj_id == p) {
            at_one += 1;
        }
    }
    assert_eq!(
        at_one, 16,
        "every one of the 16 sits at level 1 of its own record"
    );
}

// =================================================================================================
// The application harness
// =================================================================================================

/// A recording's datagrams, through the shared reader (parsed once per test binary).
fn load(session: &str) -> Vec<Record> {
    capture::shared_session(session).to_vec()
}

fn connection_sequence_number(records: &[Record]) -> u32 {
    recording::connection_sequence_number(records).expect("the recording has a LoginRequest")
}

fn replay_to(session: &str, limit: usize) -> (Vec<SessionEvent>, ObjectStream) {
    let records = load(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in records.iter().take(limit.saturating_add(1)) {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            events.push(e);
        }
    }
    (events, objects)
}

/// Replay through the last record with a populated player presence. A logout/session-end can
/// reset the object table, so the final recording row is not assumed to be the useful boundary.
fn replay(session: &str) -> (Vec<SessionEvent>, ObjectStream) {
    let records = load(session);
    let mut objects = ObjectStream::new();
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("host");
    let mut entered = false;
    let mut last_with_player: Option<usize> = None;
    for (i, r) in records.iter().enumerate() {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
        }
        if objects
            .player()
            .is_some_and(|p| objects.presence(p).is_some())
        {
            last_with_player = Some(i);
        }
    }
    let limit =
        last_with_player.expect("first-login-walk-jump never created the player's own object");
    replay_to(session, limit)
}

fn app_in_gameplay(frames: u32) -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the headless GPU device and the shipped UI");
    app.start_shell().expect("the shell starts");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn open_the_backpack(app: &mut App) {
    let shell = app.ui_mut().expect("the UI shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen");
    let page = screen
        .panels
        .pages
        .iter()
        .find(|p| p.element == INVENTORY_PAGE)
        .copied()
        .expect("the inventory page is in the shipped panel stack");
    screen.recv_set_panel_visibility(ui, page.panel_id, true);
}

/// The running application with the capture's player in it and the backpack open, so that
/// `App::paper_doll_use_time` — not this file — has built the doll.
fn app_with_the_doll() -> App {
    let (events, objects) = replay(SESSION);
    let mut app = app_in_gameplay(4);
    *app.probe_mut().objects_mut() = objects;
    let _ = app.apply_hud_events(&events);
    open_the_backpack(&mut app);
    for _ in 0..6 {
        app.frame();
    }
    app
}

// =================================================================================================
// Station 1 — the paper doll, built by the application itself
// =================================================================================================

/// Behaviour: rendering.preview.panel-previews-draw-graphics-object-level-zero
/// The application-built paper doll must bake each part's level-zero graphics object.
/// paper_doll_use_time constructs and dresses it from the recorded player. This test requires
/// one object, the expected body setup and an applied description before inspecting its meshes.
/// 16 of 34 slots have their own id at level one: 417 triangles against level zero's 772.
#[test]
fn the_paper_doll_bakes_gfxobj_zero_for_every_part() {
    let store = store();
    let mut app = app_with_the_doll();
    let space = app
        .renderer_mut()
        .preview(PreviewId::PaperDoll)
        .expect("the doll's space");
    assert_eq!(space.object_count(), 1, "one doll, not one per frame");
    let o = space.object(0).expect("the doll is object 0");
    assert_eq!(
        o.setup, BODY,
        "the capture's player wears the 34-part Aluvian male body"
    );
    assert_eq!(
        o.dressed,
        Some(true),
        "the capture's ObjDesc reached the clone"
    );
    assert_draws_level_zero(&store, o, "the paper doll");
    app.shutdown();
}

// =================================================================================================
// Station 2 — the char-gen turntable
// =================================================================================================

/// Exercise the shared preview bake through the CharGen space with the affected Aluvian body.
/// This directly adds the object; it does not traverse character-generation UI updates or prove
/// that every selectable body is affected.
#[test]
fn the_char_gen_turntable_bakes_gfxobj_zero_for_every_part() {
    let store = store();
    let mut app = app_in_gameplay(2);
    let assets = Arc::new(dereth_client::anim_assets::DatAnimAssets::new(Arc::clone(
        &store,
    )));
    assert!(
        app.renderer_mut()
            .ensure_preview(PreviewId::CharGen, &assets),
        "the space is made"
    );
    let i = app
        .renderer_mut()
        .add_preview_object(PreviewId::CharGen, &store, BODY)
        .expect("the object is built")
        .expect("the setup record loads");
    let o = app
        .renderer_mut()
        .preview(PreviewId::CharGen)
        .expect("the space")
        .object(i)
        .expect("the model");
    assert_draws_level_zero(&store, o, "the char-gen turntable");
    app.shutdown();
}

// =================================================================================================
// Station 3 — the identify window's model, and the box that frames its camera
// =================================================================================================

/// The Examine space must bake level-zero geometry and derive its box from the same level.
/// portrait::camera_position consumes that box in production. This station checks the input
/// box, not camera placement or portrait pixels, and adds the model directly to the space.
/// The part box is always read from level zero, independently of the draw level.
#[test]
fn the_identify_portrait_bakes_gfxobj_zero_and_frames_from_level_zeros_box() {
    let store = store();
    let mut app = app_in_gameplay(2);
    let assets = Arc::new(dereth_client::anim_assets::DatAnimAssets::new(Arc::clone(
        &store,
    )));
    assert!(
        app.renderer_mut()
            .ensure_preview(PreviewId::Examine, &assets),
        "the space is made"
    );
    let i = app
        .renderer_mut()
        .add_preview_object(PreviewId::Examine, &store, BODY)
        .expect("the object is built")
        .expect("the setup record loads");
    let space = app
        .renderer_mut()
        .preview(PreviewId::Examine)
        .expect("the space");
    let o = space.object(i).expect("the creature");
    assert_draws_level_zero(&store, o, "the identify portrait");

    // Seed the aggregate at the origin, scale each part's vertex box, transform it by the part
    // pose and union the result. Compare level-zero versus own-ID candidates. The selection
    // and extrema reads are separate from production, but convert_to_global is shared math.
    let observed = o.bounding_box(&store);
    let expected = |pick: &dyn Fn(DataId) -> DataId| -> (Vec3, Vec3) {
        let mut lo = Vec3::ZERO;
        let mut hi = Vec3::ZERO;
        for p in &o.part_array.parts {
            let Some((bl, bh)) = bound_box(&store, pick(p.gfxobj_id)) else {
                continue;
            };
            let s = p.gfxobj_scale;
            let local = dereth_physics::geom::BBox::new(
                Vec3::new(bl.x * s.x, bl.y * s.y, bl.z * s.z),
                Vec3::new(bh.x * s.x, bh.y * s.y, bh.z * s.z),
            )
            .convert_to_global(&p.pos);
            lo = Vec3::new(
                lo.x.min(local.min.x),
                lo.y.min(local.min.y),
                lo.z.min(local.min.z),
            );
            hi = Vec3::new(
                hi.x.max(local.max.x),
                hi.y.max(local.max.y),
                hi.z.max(local.max.z),
            );
        }
        (lo, hi)
    };
    let want = expected(&|id| level_zero(&store, id));
    let own = expected(&|id| id);
    println!(
        "the identify portrait: box is {:?}..{:?}; gfxobj[0]'s is {:?}..{:?}, the parts' \
         own is {:?}..{:?}",
        observed.min, observed.max, want.0, want.1, own.0, own.1
    );
    assert_ne!(
        (want.0.z, want.1.z),
        (own.0.z, own.1.z),
        "the two candidate boxes must differ or this station cannot discriminate"
    );
    let close = |a: Vec3, b: Vec3| {
        (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4 && (a.z - b.z).abs() < 1e-4
    };
    assert!(
        close(observed.min, want.0) && close(observed.max, want.1),
        "the preview bounding box must use level-zero geometry, so the box is \
         always level 0's: got {:?}..{:?}, gfxobj[0]'s is {:?}..{:?}",
        observed.min,
        observed.max,
        want.0,
        want.1
    );
    app.shutdown();
}

// =================================================================================================
// Station 4 — the portal space, which is the control
// =================================================================================================

/// The portal background is the unchanged-geometry control. Its public UIASSET mapping
/// portalspace_background resolves to setup 0x02000306. The record has two parts;
/// this test requires no level-zero ID swaps and compares all baked IDs and triangle totals.
/// It does not assert the part count or render/read back portal pixels.
#[test]
fn the_portal_space_is_the_control() {
    let store = store();
    let a: &dyn dereth_primitives::AssetSource = &*store;
    let setup = dereth_client::assets::enum_did(
        a,
        dereth_client::preview::UIASSET_GROUP,
        dereth_client::preview::ENUM_PORTALSPACE_BACKGROUND,
    )
    .expect("portalspace_background resolves");
    assert_eq!(setup, PORTAL_BACKGROUND, "the shipped DidMapper's answer");

    let parts = setup_parts(&store, setup);
    let differ = parts
        .iter()
        .filter(|&&p| level_zero(&store, p) != p)
        .count();
    assert_eq!(
        differ, 0,
        "the portal-background setup has no parts with a different level-zero graphics object"
    );

    let mut app = app_in_gameplay(2);
    let assets = Arc::new(dereth_client::anim_assets::DatAnimAssets::new(Arc::clone(
        &store,
    )));
    assert!(
        app.renderer_mut()
            .ensure_preview(PreviewId::Portal, &assets),
        "the space is made"
    );
    let i = app
        .renderer_mut()
        .add_preview_object(PreviewId::Portal, &store, setup)
        .expect("the object is built")
        .expect("the setup record loads");
    let o = app
        .renderer_mut()
        .preview(PreviewId::Portal)
        .expect("the space")
        .object(i)
        .expect("the swirl");
    let own: usize = parts.iter().map(|&p| triangles(&store, p)).sum();
    println!(
        "the portal space: {} parts, {own} triangles either way",
        parts.len()
    );
    assert_eq!(
        o.built_from(),
        parts.as_slice(),
        "nothing to swap, so nothing swapped"
    );
    assert_eq!(
        o.triangles(),
        own,
        "the control's geometry is unchanged by the level-zero bake"
    );
    app.shutdown();
}
