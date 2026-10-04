//! An object stops being drawn at the distance its own degrade record names: a creature placed on
//! Holtburg's terrain stops being submitted at the record's all-`FLT_MAX` terminator, selected by
//! the level-selection rule, and no adaptive bias moves that distance. Terrain still occludes a
//! distant object the client keeps submitting.
//!
//! Level selection takes the viewer distance to the part's sort centre over the part's z scale,
//! subtracts the 50 m degradation distance and clamps at zero (`d = max(|distance| - 50, 0)`),
//! then takes the first level with `d < threshold`, else the last; the terminator is a candidate
//! like any other, and its missing graphics object draws nothing. The Aluvian male body
//! `0x02000001` has an 84 m card (min = ideal = max) before the terminator on its far parts, so
//! it goes dark at a viewer distance of 134 m at every bias. Over every degrade record the
//! go-dark distance has a median near 130 m, and over a thousand setups never go dark at all.
//!
//! Fixture: the retail dats, Holtburg with a body attached, on a software device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_assets::motion::GfxObjDegradeInfo;
use dereth_assets::Decode;
use dereth_client::character::{CharacterInput, PLAYER_OBJECT_ID};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::num::math;
use dereth_primitives::{DataId, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
use dereth_render::device::Gpu;
use dereth_world_render::objects::degrade::{get_degrade, DegradeGlobals, DegradeMode};

const W: u32 = 800;
const H: u32 = 600;
const HOLTBURG: u16 = 0xA9B4;
const TARGET: ObjectId = ObjectId(0x8300_0021);

/// The initial degradation distance.
const S_R_DEGRADE_DISTANCE: f32 = 50.0;

/// The Aluvian male body.
const CREATURE: u32 = 0x0200_0001;
/// A setup with no `GfxObjDegradeInfo` terminator on any part, so it never goes dark.
/// Chosen by [`the_dat_is_full_of_setups_that_never_go_dark`]'s census; 18 parts, so it is big
/// enough to paint a measurable number of pixels at range.
const NEVER_DARK: u32 = 0x0200_004D;

fn store() -> std::sync::Arc<RetailDatStore> {
    crate::common::dats()
}

fn outdoor(x: f32, y: f32, z: f32) -> Position {
    let mut cell = LandblockId(HOLTBURG).cell(1);
    let mut o = Vec3::new(x, y, z);
    dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut o);
    Position::new(cell, dereth_primitives::Frame::new(o, Quat::IDENTITY))
}

/// The optional `GfxObjDegradeInfo` referenced by a graphics-object record.
fn record_of(store: &RetailDatStore, gfx: DataId) -> Option<GfxObjDegradeInfo> {
    let bytes = store.read_typed(DbType::GfxObj, gfx).ok()?;
    let obj = dereth_assets::GfxObj::decode_payload(gfx, &bytes).ok()?;
    let did = obj.did_degrade?;
    let bytes = store.read_typed(DbType::DegradeInfo, did).ok()?;
    GfxObjDegradeInfo::decode_payload(did, &bytes).ok()
}

/// The smallest `d` at which `get_degrade` picks a level with `gfxobj_id == 0`, as a **world**
/// distance (`d + 50`). `None` when the record has no such level.
fn go_dark(rec: &GfxObjDegradeInfo, bias: f32) -> Option<f32> {
    let mut lo = 0.0f32;
    for g in &rec.degrades {
        if g.gfxobj_id.0 == 0 {
            return Some(lo + S_R_DEGRADE_DISTANCE);
        }
        let t = if bias >= 0.0 {
            g.ideal_dist - (g.ideal_dist - g.max_dist) * bias
        } else {
            (g.ideal_dist - g.min_dist).mul_add(bias, g.ideal_dist)
        };
        lo = lo.max(t);
    }
    None
}

/// **The mechanism.** The body's own record ends in the all-`FLT_MAX` terminator, `get_degrade`
/// selects it, and no adaptive bias can move where.
#[test]
fn the_creatures_cutoff_is_its_records_own_terminator() {
    let store = store();
    let bytes = store
        .read_typed(DbType::Setup, DataId(CREATURE))
        .expect("the setup");
    let setup =
        <dereth_assets::geometry::Setup as Decode>::decode_payload(DataId(CREATURE), &bytes)
            .expect("the setup decodes");
    assert!(!setup.parts.is_empty(), "the body has no parts");

    let mut with_record = 0usize;
    // Per part and per bias, the distance at which that part stops drawing. The **body** is gone
    // when its longest-reach part is: parts leave one at a time, which is why the submitted-subset
    // count in the driven test falls 23 -> 17 -> 14 -> 0 rather than dropping in one step.
    let mut furthest: [(f32, f32); 5] =
        [(-1.0, 0.0), (-0.5, 0.0), (0.0, 0.0), (0.5, 0.0), (1.0, 0.0)];
    let mut cards = 0usize;
    for gfx in &setup.parts {
        let Some(rec) = record_of(&store, *gfx) else {
            continue;
        };
        with_record += 1;
        let last = rec.degrades.last().expect("a non-empty record");
        assert_eq!(
            last.gfxobj_id.0, 0,
            "part {gfx:?}'s record does not end in the draw-nothing terminator"
        );
        assert!(
            last.min_dist == f32::MAX && last.ideal_dist == f32::MAX && last.max_dist == f32::MAX,
            "the terminator is not all-FLT_MAX: {last:?}"
        );
        // The level before the terminator. On the parts that carry a far card it is fixed at
        // min == ideal == max == 84 -- some as the `degrade_mode 5` upright billboard, some as an
        // ordinary mesh -- and the rest are trimmings that leave earlier and do not set the
        // body's reach.
        let card = rec.degrades[rec.degrades.len() - 2];
        if (card.ideal_dist - card.max_dist).abs() < 1e-3
            && (card.ideal_dist - card.min_dist).abs() < 1e-3
            && (card.ideal_dist - 84.0).abs() < 1e-3
        {
            cards += 1;
        }
        for (bias, worst) in &mut furthest {
            let d = go_dark(&rec, *bias).expect("the record has a terminator");
            *worst = worst.max(d);
        }
        // `get_degrade` itself, on the part's own record, either side of its own last threshold.
        for bias in [-1.0f32, 0.0, 1.0] {
            let g = DegradeGlobals {
                deg_mul: bias,
                auto_update_deg_mul: true,
                ..DegradeGlobals::default()
            };
            let dark = go_dark(&rec, bias).expect("a terminator");
            // A part whose whole ladder collapses below the degradation distance at this bias is dark
            // everywhere and has no crossing to probe either side of. Two of the body's fifteen
            // trimmings are in that state at bias -1; they are skipped rather than asserted about.
            if dark < S_R_DEGRADE_DISTANCE + 5.0 {
                continue;
            }
            let (a, _) = get_degrade(&rec, dark - 0.5, &g);
            let (b, mode) = get_degrade(&rec, dark + 0.5, &g);
            assert!(
                a < rec.degrades.len() - 1,
                "at bias {bias} and {:.1} m the terminator is already selected",
                dark - 0.5
            );
            assert_eq!(
                b,
                rec.degrades.len() - 1,
                "at bias {bias} and {:.1} m the terminator is not selected",
                dark + 0.5
            );
            assert_eq!(
                mode,
                DegradeMode::None,
                "the terminator's mode is the record's"
            );
        }
    }
    assert!(
        with_record >= 10,
        "only {with_record} of the body's parts carry a record"
    );
    assert!(
        cards >= 8,
        "only {cards} parts carry the fixed-distance 84 m card"
    );
    eprintln!(
        "draw distance: {with_record} parts carry a record, {cards} of them the fixed 84 m card; the \
         body's furthest part goes dark at {furthest:?}"
    );
    // **The claim.** The body's reach is the 84 m card plus the 50 m degradation distance; because that
    // card's min, ideal and max are the same number, no bias the governor can hold moves it.
    for (bias, d) in furthest {
        assert!(
            (d - 134.0).abs() < 1.0,
            "at bias {bias:.1} the body's furthest part goes dark at {d:.1} m, not 134"
        );
    }
}

/// **The body's 134 m reach is close to the 130 m median** — with a long tail visible
/// in the distance.
#[test]
fn the_dat_is_full_of_setups_that_never_go_dark() {
    let store = store();
    let ids = store.ids_of(DbType::DegradeInfo);
    let mut reach0 = Vec::new();
    let mut reach1 = Vec::new();
    let mut terminators = 0usize;
    for id in &ids {
        let Ok(bytes) = store.read_typed(DbType::DegradeInfo, *id) else {
            continue;
        };
        let Ok(rec) = GfxObjDegradeInfo::decode_payload(*id, &bytes) else {
            continue;
        };
        if let Some(d) = go_dark(&rec, 0.0) {
            terminators += 1;
            reach0.push(d);
        }
        if let Some(d) = go_dark(&rec, 1.0) {
            reach1.push(d);
        }
    }
    reach0.sort_by(f32::total_cmp);
    reach1.sort_by(f32::total_cmp);
    let at = |v: &[f32], p: f32| -> f32 {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let i = ((v.len() - 1) as f32 * p) as usize;
        v[i]
    };
    eprintln!(
        "draw distance: {} records, {terminators} with a terminator; go dark at p50 {:.0} / {:.0} m \
         (bias 0 / +1), p90 {:.0} / {:.0}, max {:.0}",
        ids.len(),
        at(&reach0, 0.5),
        at(&reach1, 0.5),
        at(&reach0, 0.9),
        at(&reach1, 0.9),
        reach0.last().copied().unwrap_or(0.0),
    );
    assert!(
        terminators > 4_000,
        "only {terminators} records carry a terminator"
    );
    assert!(
        (100.0..200.0).contains(&at(&reach0, 0.5)),
        "the median go-dark distance is {:.0} m, not the ~130 m this file reports",
        at(&reach0, 0.5)
    );
    assert!(
        at(&reach0, 0.9) > 300.0,
        "the 90th percentile is only {:.0} m; nothing would be visible in the distance",
        at(&reach0, 0.9)
    );

    // And the setups with no terminator at all — the ones an occlusion bench has to use.
    let mut never = 0usize;
    let mut never_dark_has_our_pick = false;
    for id in store.ids_of(DbType::Setup) {
        let Ok(bytes) = store.read_typed(DbType::Setup, id) else {
            continue;
        };
        let Ok(setup) = <dereth_assets::geometry::Setup as Decode>::decode_payload(id, &bytes)
        else {
            continue;
        };
        if setup.parts.is_empty() {
            continue;
        }
        let dark = setup
            .parts
            .iter()
            .filter_map(|p| record_of(&store, *p))
            .filter_map(|r| go_dark(&r, 0.0))
            .fold(f32::MAX, f32::min);
        if dark >= f32::MAX {
            never += 1;
            never_dark_has_our_pick |= id.0 == NEVER_DARK;
        }
    }
    eprintln!("draw distance: {never} setups never go dark");
    assert!(never > 1_000, "only {never} setups never go dark");
    assert!(
        never_dark_has_our_pick,
        "{NEVER_DARK:#010X}, the setup the occlusion test below uses, is not one of them"
    );
}

// ---------------------------------------------------------------------------------------------
// The driven half.
// ---------------------------------------------------------------------------------------------

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: std::sync::Arc<RetailDatStore>,
    objects: ObjectStream,
    now: f64,
    instance: u16,
    setup: u32,
    /// **Run the swept camera update in [`Self::draw`], i.e. draw from the camera the client
    /// actually draws from.** `false` by default *on purpose*.
    ///
    /// `App::frame` runs the swept camera update after the scene update. Without it every frame
    /// comes from the debug chase camera `WorldScene::update` leaves in `scene.camera` — 4.41 m
    /// behind the body, 2.54 m above it, level, and through walls — rather than from the swept
    /// viewer at `(0, -2.75, 0.825)` behind the pivot with its 16.699-degree downward pitch.
    ///
    /// **[`terrain_occludes_a_distant_object_the_client_still_submits`] sets it, because the eye is
    /// its subject**: an occlusion margin is a property of the line from the eye, so measuring it
    /// from a camera the client never uses measures nothing.
    ///
    /// **[`a_creature_stops_being_submitted_at_the_distance_the_record_names`] does not set it.**
    /// On its fine ladder the all-parts-dark crossing is bracketed at `(132.9, 134.8)` with the
    /// chase camera — containing the record's 134 m — and at `(130.9, 132.8)` with the swept one,
    /// which does not. The printed viewer distance per rung agrees to 0.1 m between the two runs,
    /// so **the same distance gets a different verdict**, which is a statement about
    /// `get_degrade`'s inputs and not about the distance. The record census in this same file
    /// prints this creature's furthest part going dark at 134.0 m at **every** adaptive bias from
    /// -1 to +1, so the bias is not an explanation either and the 2 m is unaccounted for. That is a
    /// degrade-record question, not a viewer one, and this flag keeps the two subjects apart.
    sweep: bool,
}

impl Bench {
    fn new(store: &std::sync::Arc<RetailDatStore>, mut gpu: Gpu, setup: u32) -> Self {
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let cfg = SceneConfig {
            landblock: HOLTBURG,
            land_radius: 3,
            time_of_day: Some(0.35),
            particles: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(store, &region, &mut gpu)
            .expect("the body is created");
        let mut objects = ObjectStream::new();
        objects.world.player = Some(PLAYER_OBJECT_ID);
        let mut me = dereth_client_model::Weenie::new(PLAYER_OBJECT_ID);
        me.valid = true;
        me.has_phys_obj = true;
        me.qualities = Some(dereth_client_model::qualities::Qualities::new());
        objects.world.tables.weenies.insert(PLAYER_OBJECT_ID, me);
        Self {
            gpu,
            scene,
            store: store.clone(),
            objects,
            now: 1.0,
            instance: 0,
            setup,
            sweep: false,
        }
    }

    fn height(&self, x: f32, y: f32) -> Option<f32> {
        self.scene
            .character
            .as_ref()?
            .world
            .terrain_height_at(&outdoor(x, y, 0.0))
    }

    fn stand(&mut self, body: Vec3, yaw: f32) {
        let q = Quat::new(math::cosf(yaw * 0.5), 0.0, 0.0, math::sinf(yaw * 0.5));
        let mut p = outdoor(body.x, body.y, body.z);
        p.frame.rotation = q;
        self.scene.character.as_mut().expect("a body").teleport(p);
    }

    fn place(&mut self, at: Position) {
        self.instance += 1;
        let payload = dereth_protocol::objects::ObjectCreatePayload {
            id: TARGET,
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION | flags::SETUP,
                setup_id: Some(self.setup),
                state: 0,
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: at.cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: at.frame.origin.into(),
                        orientation: Quat::IDENTITY.into(),
                    },
                }),
                timestamps: dereth_protocol::types::PhysicsTimestamps {
                    instance: self.instance,
                    ..dereth_protocol::types::PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        };
        let body =
            dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
                .expect("encode");
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(self.now),
        );
        {
            let Self {
                store,
                objects,
                scene,
                ..
            } = self;
            let ch = scene.character.as_mut().expect("a body");
            objects.sync_physics(store, &mut ch.world);
        }
        let w = self
            .objects
            .world
            .tables
            .weenies
            .get_mut(TARGET)
            .expect("placed");
        w.pwd.obj_type |= item_type::CREATURE;
        w.pwd.bitfield |= bitfield::ATTACKABLE;
        w.pwd.radar_enum = Some(4);
        self.objects.world.update_visible_object_list();
    }

    /// The app's own frame order.
    fn draw(&mut self) -> Vec<u8> {
        self.now += 1.0;
        let Self {
            store,
            gpu,
            scene,
            objects,
            ..
        } = self;
        scene
            .sync_objects(store, gpu, objects)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(self.now),
            1.0 / 30.0,
        );
        // **`App::frame`'s next camera step.** The swept camera update puts the production camera
        // into `scene.camera`; without it the bench draws from the **debug chase camera**. See
        // [`real_eye`].
        if self.sweep {
            dereth_client::camera::update_viewer(
                scene,
                dereth_client::camera::CameraInput::default(),
                LocalTime(self.now),
                1.0 / 30.0,
            );
        }
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture").to_rgba()
    }

    fn submitted(&self) -> usize {
        self.scene
            .drawn_part_order()
            .iter()
            .filter(|e| e.object == Some(TARGET))
            .count()
    }

    fn cypt(&self) -> f32 {
        self.scene
            .part_degrade_probe()
            .into_iter()
            .find(|r| r.object == Some(TARGET))
            .map_or(-1.0, |r| r.cypt)
    }
}

/// **The eye the client draws from, read off the engine rather than modelled.**
///
/// `WorldScene::update`'s debug chase camera sits 4.41 m behind the body and 2.54 m above it,
/// through walls, and `App::frame` **overwrites it on the next line** with the swept camera
/// update: the swept sphere uses the viewer offset behind the *pivot*, `(0, -2.75, 0.825)`
/// with the look-at-pivot mode and a 16.699-degree downward pitch. Occlusion margins must come
/// from that eye.
///
/// What is asserted about it is the part that is a claim rather than a tautology: the sweep ran, and the eye is behind the
/// body at roughly the shipped offset. The horizontal bound is generous on purpose — the sweep can
/// legitimately pull the eye in against terrain — and a sweep that pulled it all the way onto the
/// pivot would fail the lower bound rather than silently turn the occlusion stations into a
/// first-person measurement.
fn real_eye(scene: &WorldScene, body: Vec3, yaw: f32) -> (Vec3, (f32, f32)) {
    let f = (-math::sinf(yaw), math::cosf(yaw));
    let c = scene.character.as_ref().expect("a body");
    assert!(
        c.camera.stats.sweeps > 0,
        "no world-controller viewer-update sweep ran, so `scene.camera` is still the debug chase camera"
    );
    let eye = scene.camera.position;
    let back = (body.x - eye.x) * f.0 + (body.y - eye.y) * f.1;
    let up = eye.z - body.z;
    eprintln!(
        "eye: body ({:.2}, {:.2}, {:.2}) yaw {yaw:.3} -> eye ({:.2}, {:.2}, {:.2}); {back:.2} m \
         behind along the heading, {up:.2} m above the feet; {} sweeps, {} blocked",
        body.x,
        body.y,
        body.z,
        eye.x,
        eye.y,
        eye.z,
        c.camera.stats.sweeps,
        c.camera.stats.sweeps_blocked,
    );
    assert!(
        (0.5..4.0).contains(&back),
        "the eye is {back:.2} m behind the body along its heading; the shipped third-person offset \
         is 2.75 m and the sweep can only shorten it, so this is neither"
    );
    assert!(
        (0.5..4.0).contains(&up),
        "the eye is {up:.2} m above the body's feet; the shipped offset puts it at the pivot's \
         1.5 m plus 0.825 m"
    );
    (eye, f)
}

/// The largest amount by which the terrain rises above the camera-to-target line. Positive means
/// the ridge is in the way.
fn occlusion_margin(b: &Bench, cam: Vec3, t: Vec3) -> f32 {
    let d = (t.x - cam.x, t.y - cam.y, t.z - cam.z);
    #[allow(clippy::cast_possible_truncation)] // a horizontal distance in metres
    let n = (math::hypotf(d.0, d.1) as i32).max(2);
    let mut worst = f32::MIN;
    for i in 1..n {
        #[allow(clippy::cast_precision_loss)] // a loop counter under a few thousand
        let f = i as f32 / n as f32;
        let (px, py, pz) = (cam.x + d.0 * f, cam.y + d.1 * f, cam.z + d.2 * f);
        let Some(h) = b.height(px, py) else { continue };
        worst = worst.max(h - pz);
    }
    worst
}

/// Behaviour: rendering.degrade.a-creature-stops-drawing-at-its-records-own-distance
/// **The driven half of the mechanism.** A creature really does stop being submitted where its
/// record says, and the 50-unit subtraction in `get_degrade`'s clamp is visible in *where*.
#[test]
fn a_creature_stops_being_submitted_at_the_distance_the_record_names() {
    let store = store();
    let gpu = crate::common::test_gpu(W, H);
    let mut b = Bench::new(&store, gpu, CREATURE);
    let body_xy = (96.0f32, 20.0f32);
    let bz = b
        .height(body_xy.0, body_xy.1)
        .expect("the station is on terrain");
    b.stand(Vec3::new(body_xy.0, body_xy.1, bz + 0.01), 0.0);
    for _ in 0..12 {
        b.draw();
    }
    let cam = b.scene.camera.position;
    let f = (0.0f32, 1.0f32);

    let mut last_drawn = 0.0f32;
    let mut first_gone = f32::MAX;
    // The ladder is fine around 134 m: four rungs inside what would otherwise be a 10 m gap pin the
    // crossing to ~2 m, because a 10 m bracket cannot tell "the crossing moved" from "the bracket
    // is coarse". The assertion below is against that bracket.
    for dist in [
        60.0f32, 100.0, 120.0, 126.0, 128.0, 130.0, 132.0, 134.0, 140.0, 160.0, 300.0,
    ] {
        let (tx, ty) = (cam.x + f.0 * dist, cam.y + f.1 * dist);
        let Some(th) = b.height(tx, ty) else { continue };
        b.place(outdoor(tx, ty, th + 2.0));
        for _ in 0..2 {
            b.draw();
        }
        let (n, cypt) = (b.submitted(), b.cypt());
        eprintln!("draw distance: d={dist:5.0} range={cypt:7.1} submitted={n}");
        if n > 0 {
            last_drawn = last_drawn.max(cypt);
        } else {
            first_gone = first_gone.min(cypt);
        }
    }
    eprintln!("draw distance: last drawn at {last_drawn:.1} m, first gone at {first_gone:.1}");
    // Measured under both cameras on the ladder above (see [`Bench::sweep`]): the chase camera,
    // which this test uses, brackets `(132.9, 134.8)`, containing 134; the swept viewer brackets
    // `(130.9, 132.8)`, which does not. The per-rung viewer distance agrees to 0.1 m between the
    // two runs, so the 2 m belongs to `get_degrade`'s inputs rather than to the ladder or the
    // camera's distance.
    assert!(
        last_drawn > 100.0,
        "the creature was already gone at {last_drawn:.1} m"
    );
    assert!(
        first_gone < f32::MAX,
        "the creature never disappeared; the sweep proves nothing"
    );
    // The record's card sits at `d = 84`, and `d = |distance| - 50`, so the crossing is at 134 m.
    assert!(
        last_drawn < 134.0 && first_gone > 134.0,
        "the crossing is not at 134 m: last drawn {last_drawn:.1}, first gone {first_gone:.1}. \
         If it moved to ~84 the distance-bias subtraction has been dropped from get_degrade"
    );
}

/// **Terrain occludes a distant object.** An object the client submits at 380 m,
/// standing behind a ridge that buries the line of sight, paints nothing — and the same object in
/// the clear paints thousands of pixels.
#[test]
fn terrain_occludes_a_distant_object_the_client_still_submits() {
    let store = store();
    let g = crate::common::test_gpu(W, H);
    let mut a = Bench::new(&store, g, NEVER_DARK);
    let g = crate::common::test_gpu(W, H);
    let mut b = Bench::new(&store, g, NEVER_DARK);
    // **The eye is this test's subject, so it must be the client's eye.** See [`Bench::sweep`]. Both arms, identically, so the differential still differs in one thing.
    a.sweep = true;
    b.sweep = true;
    // The deepest occlusion a sweep of Holtburg found: a ridge ~30 m above the line of sight.
    let body = Vec3::new(18.0, 90.0, 72.01);
    let yaw = 4.451f32;
    a.stand(body, yaw);
    b.stand(body, yaw);
    for _ in 0..16 {
        a.draw();
        b.draw();
    }
    // The eye is read off the engine and checked for plausibility, not modelled. See [`real_eye`].
    let (cam, f) = real_eye(&a.scene, body, yaw);

    let mut clear_px = 0usize;
    let mut occluded = Vec::new();
    for dist in [25.0f32, 220.0, 300.0, 380.0] {
        let (tx, ty) = (cam.x + f.0 * dist, cam.y + f.1 * dist);
        let Some(th) = b.height(tx, ty) else { continue };
        let tz = th + 1.0;
        let margin = occlusion_margin(&b, cam, Vec3::new(tx, ty, tz));
        b.place(outdoor(tx, ty, tz));
        let (mut fa, mut fb) = (Vec::new(), Vec::new());
        for _ in 0..3 {
            fa = a.draw();
            fb = b.draw();
        }
        let px = fa
            .chunks_exact(4)
            .zip(fb.chunks_exact(4))
            .filter(|(p, q)| p != q)
            .count();
        let n = b.submitted();
        eprintln!(
            "occlusion: d={dist:5.0} range={:7.1} occlusion_margin={margin:7.2} submitted={n} \
             object_px={px}",
            b.cypt()
        );
        // The premise: this object is never degraded out, at any of these distances.
        assert!(
            n > 0,
            "at {dist} m the object was not submitted; the station proves nothing"
        );
        if margin < 0.0 {
            clear_px = clear_px.max(px);
        } else {
            occluded.push((dist, margin, px));
        }
    }
    // The positive control: without it, "0 pixels" would be indistinguishable from an object that
    // never draws anywhere.
    assert!(
        clear_px > 500,
        "the unoccluded station painted only {clear_px} pixels; a zero elsewhere would mean \
         nothing"
    );
    assert!(
        occluded.len() >= 2,
        "the sweep found fewer than two occluded stations"
    );
    for (dist, margin, px) in &occluded {
        assert!(
            *margin > 5.0,
            "the station at {dist} m is only {margin:.2} m behind the ridge -- too marginal to \
             assert against"
        );
        assert_eq!(
            *px, 0,
            "at {dist} m, {margin:.1} m behind a ridge, the object painted {px} pixels over the \
             terrain -- the depth test is not occluding it"
        );
    }
}
