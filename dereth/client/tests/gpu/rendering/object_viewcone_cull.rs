//! The per-object view-cone cull. A mesh is drawn only when its drawing sphere is not outside the
//! view cone, and the selected-object latch is raised only on that answer, not merely because a
//! part of the selection was submitted. `SceneConfig::object_viewcone` (shipped on) gates the draw
//! and nothing else, so every test drives both arms. The cull changes no pixel: at every station
//! a paired frame differing only in the cull differs by 0 pixels while the cull rejects parts, and
//! the harness's own noise floor is measured first. The cone's near plane and the projection's are
//! the same constant, pinned so a second producer fails here.
//! Fixture: the retail dats on a software device, with creatures created around the local body,
//! and the `first-login-walk-jump` recording replayed to its most populated datagram over its own
//! landblock, the camera aimed from the objects' centroid at eleven stations.

#![cfg(gpu)]

use dereth_client_net::client_session::testing::capture::{self, peer as addr, Datagram as Record};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_client_runtime::selection_geometry::SceneSelectionPhysics;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_render::device::{DeviceConfig, Gpu};
use std::sync::Arc;
use {
    dereth_client_runtime::character::CharacterInput,
    dereth_client_runtime::character::PLAYER_OBJECT_ID,
};
use {
    dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::ObjectConeStats,
    dereth_scene::world_scene::WorldScene,
};
use {dereth_rules::weenie::bitfield, dereth_rules::weenie::item_type};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

/// In front of the player, well inside any field of view — the calibration positive.
const AHEAD: ObjectId = ObjectId(0x8300_0011);
const AHEAD_AT: (f32, f32, f32) = (1.8, 2.4, 0.0);
/// Behind the player, and therefore behind the third-person camera, which sits *between* them.
/// Inside the outdoor radar radius (75 m) so nothing else drops it, and far enough back that no
/// field of view this client can be configured with reaches it.
const BEHIND: ObjectId = ObjectId(0x8300_0012);
const BEHIND_AT: (f32, f32, f32) = (-9.0, -42.0, 0.0);
/// In **front** of the camera and far off to the side: about 85° off axis, so
/// its positive camera-forward coordinate admits it and only the view polygon's edge planes reject it.
///
/// Without it, removing the view polygon's planes from `WorldScene::part_cone` (leaving the near
/// plane alone) would go unnoticed against [`BEHIND`], because for a symmetric full-screen cone the four side planes
/// already imply `forward > |lateral|` and so the near plane rejects anything behind the camera on
/// its own. The two are only distinguishable off to the side.
const SIDE: ObjectId = ObjectId(0x8300_0013);
const SIDE_AT: (f32, f32, f32) = (40.0, 3.0, 0.0);
/// In front of the camera and standing **across** the view polygon's right-hand edge plane: some of
/// its parts are wholly outside, and the ones still drawn are there only because their drawing
/// spheres reach back across the plane. Measured with the bench's camera, one body at a time
/// stepped outward along `y = 10` in quarter metres: 23 subsets drawn at `x = 13.5`, 3 at `14.75`,
/// none from `15.0`. At `14.75` a sphere tested at a quarter of its radius draws none, and so does
/// a cone that also rejects straddling parts.
const EDGE: ObjectId = ObjectId(0x8300_0014);
const EDGE_AT: (f32, f32, f32) = (14.75, 10.0, 0.0);
/// `0x02000001`, the Aluvian male setup.
const MONSTER_SETUP: u32 = 0x0200_0001;
/// The Holtburg landscape's own height under each station offset, in player space — see
/// [`Bench::assert_at`], which is where these were measured and where the rule is stated.
const SETTLED_Z: [(ObjectId, f32); 3] = [(AHEAD, -1.394_997), (BEHIND, 11.004_997), (SIDE, 0.0)];

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    player: Position,
    now: f64,
    instance: u16,
}

impl Bench {
    fn new() -> Self {
        let store = Arc::new(
            dereth_dat::testing::open_store()
                .expect("the retail dats are the draw's own argument: set DERETH_TEST_DAT_DIR"),
        );
        let dev = DeviceConfig {
            width: 800,
            height: 600,
            ..DeviceConfig::default()
        };
        let mut gpu = Gpu::new(None, &dev).expect("a D3D12 WARP device");
        let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
        let cfg = SceneConfig {
            cell_statics: false,
            mesh_collision: false,
            land_radius: 1,
            scenery_radius: 0,
            particles: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");
        let player = scene.character.as_ref().expect("a body").position();

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
            store,
            objects,
            player,
            now: 1.0,
            instance: 0,
        }
    }

    fn origin_of(&self, offset: (f32, f32, f32)) -> Vec3 {
        dereth_physics::math::localtoglobal(
            &self.player.frame,
            Vec3::new(offset.0, offset.1, offset.2),
        )
    }

    /// The premise, asserted rather than assumed: the object really is at the player-space offset
    /// the station asked for — **on the wire**, which is the thing this bench controls — and its
    /// body is on the ground under that spot.
    ///
    /// The two halves are read separately: `Presence::position` is the *body's physics position*,
    /// republished by `ObjectStream::publish_physics_cells` at the end of `sync_physics` (one line
    /// above this call), and `SceneSelectionPhysics` reads that, as the client's consumers use the
    /// physics position; the server's word is `server_position`.
    ///
    /// So the body is not where the create asked. Enter-world placement puts the body
    /// **on the landscape**, and this bench asks for `z = 0` in player
    /// space — the player's own `z`, which is `SceneConfig`'s spawn constant `80.00001` in cell
    /// `0xA9B40025` and not a settled height. Measured on the retail dats:
    ///
    /// ```text
    ///   AHEAD  (1.8, 2.4)    wire z 80.00001   body z 78.60501   -> player-space z -1.394997
    ///   BEHIND (-9.0, -42.0) wire z 80.00001   body z 91.00500   -> player-space z +11.004997
    ///   SIDE   (40.0, 3.0)   wire z 80.00001   body z 80.00001   -> player-space z  0.000000
    /// ```
    ///
    /// `BEHIND` settles **upward**: outdoors the placement lands on the terrain, which may be
    /// above the requested point, so "only ever downward" is an indoor-floor rule and not this
    /// one. The wire is asserted exactly, the body's `x`/`y` are asserted exactly, and the body's
    /// `z` is asserted against the landscape height measured above.
    fn assert_at(&self, id: ObjectId, offset: (f32, f32, f32)) {
        // Half one: the create carried exactly what the bench asked for. This is `server_position`
        // — the wire's own word, whose single reader is `ObjectPhysics::sync` — so it is unaffected
        // by whatever the placement then did with the body.
        let want_origin = self.origin_of(offset);
        let wire = self
            .objects
            .presence(id)
            .and_then(|p| p.server_position)
            .expect("the create's own position reached the presence");
        assert_eq!(
            wire.cell, self.player.cell,
            "{id:?} was created in the player's own cell"
        );
        assert_eq!(
            (
                wire.frame.origin.x,
                wire.frame.origin.y,
                wire.frame.origin.z
            ),
            (want_origin.x, want_origin.y, want_origin.z),
            "{id:?}'s create carried the origin this bench asked for"
        );
        // Half two: the body is under that spot, on the ground.
        let settled = SETTLED_Z
            .iter()
            .find(|(who, _)| *who == id)
            .map_or(offset.2, |(_, z)| *z);
        let m = SceneSelectionPhysics::new(Some(&self.player), &self.objects)
            .get(id)
            .expect("the seam can answer for it")
            .player_space;
        for (got, want, axis) in [
            (m.0, offset.0, 'x'),
            (m.1, offset.1, 'y'),
            (m.2, settled, 'z'),
        ] {
            assert!(
                (got - want).abs() < 1e-3,
                "{id:?} is at player-space {axis} = {got}, the station asked for {want}"
            );
        }
    }

    fn place_monster(&mut self, id: ObjectId, offset: (f32, f32, f32)) {
        use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};

        self.instance += 1;
        let origin = self.origin_of(offset);
        let payload = dereth_protocol::objects::ObjectCreatePayload {
            id,
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION | flags::SETUP,
                setup_id: Some(MONSTER_SETUP),
                state: 0,
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: self.player.cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: origin.into(),
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
        self.assert_at(id, offset);

        let w = self
            .objects
            .world
            .tables
            .weenies
            .get_mut(id)
            .expect("placed");
        w.pwd.obj_type |= item_type::CREATURE;
        w.pwd.bitfield |= bitfield::ATTACKABLE;
        w.pwd.radar_enum = Some(4);
        self.objects.world.update_visible_object_list();
        assert!(
            self.objects.world.tables.visible.contains(&id),
            "premise: the actual visibility sweep made {id:?} visible"
        );
    }

    /// The draw step: `WorldScene::draw` on a software device, with the surrounding four calls from
    /// `App::frame` kept in their production order.
    fn draw(&mut self) {
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
            dereth_client_runtime::camera::CameraInput::default(),
            dereth_client_runtime::character::CharacterInput::default(),
            LocalTime(self.now),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
    }

    /// The **submission predicate**: how many parts of `id` the last draw submitted. A latch that
    /// fired on `s.object == watched` inside exactly this loop would rise on a non-zero here.
    fn parts_submitted_for(&self, id: ObjectId) -> usize {
        self.scene
            .drawn_part_order()
            .iter()
            .filter(|e| e.object == Some(id))
            .count()
    }
}

fn two_monsters() -> Bench {
    let mut b = Bench::new();
    b.place_monster(AHEAD, AHEAD_AT);
    b.place_monster(BEHIND, BEHIND_AT);
    b.place_monster(SIDE, SIDE_AT);
    b
}

// =================================================================================================
// 1. Calibration — the cone runs, and it is not answering the same thing for everything
// =================================================================================================

/// **The instrument's own denominator.** `ObjectConeStats::tested` is the number of parts actually
/// offered to the cone test. Without it, "nothing was culled" and "the cull did not run" are the
/// same reading.
///
/// It also pins `no_sphere` at **0** on the shipped creature body. A mesh's drawing sphere comes
/// only from its drawing BSP, so the view-cone path cannot represent a submitted mesh without one.
/// A non-zero here would mean this build draws something retail could not.
#[test]
fn the_cone_runs_on_every_submitted_part_and_every_part_has_a_sphere() {
    let mut b = two_monsters();
    // **Set, not inherited.** The shipped default is `true`; this arm is the one where the cone runs and gates nothing, which is what makes `outside` and `culled`
    // two distinguishable numbers below.
    b.scene.draw.cfg.object_viewcone = false;
    b.draw();
    let cone = b.scene.drawn_object_cone();
    assert!(
        cone.tested > 0,
        "the cone was never asked about anything: {cone:?}"
    );
    assert_eq!(
        cone.no_sphere, 0,
        "every drawn level has a graphics-object drawing sphere: {cone:?}"
    );
    // And it is discriminating rather than firing: something passed and something did not.
    assert!(
        cone.outside > 0,
        "nothing was rejected, so the station is wrong: {cone:?}"
    );
    assert!(
        cone.outside < cone.tested,
        "everything was rejected: {cone:?}"
    );
    // `outside` and `culled` are two numbers precisely so this arm can be told from the other:
    // with the draw ungated nothing is skipped and the count of *would-have-been* rejections is
    // still reported. A single number could not say that.
    assert_eq!(
        cone.culled, 0,
        "the shipped default gates no draw: {cone:?}"
    );

    b.scene.draw.cfg.object_viewcone = true;
    b.draw();
    let on = b.scene.drawn_object_cone();
    assert_eq!(
        on.tested, cone.tested,
        "the cone runs on the same parts either way: {on:?}"
    );
    assert_eq!(
        on.outside, cone.outside,
        "and answers the same for them: {on:?}"
    );
    assert_eq!(
        on.culled, on.outside,
        "with the cull on, every rejection skips its draw: {on:?}"
    );
}

/// **The shipping configuration, shown rather than stated.**
///
/// The default is `true`: `SceneConfig::default().object_viewcone` follows retail's mesh cull, and
/// this build draws no mesh retail refuses. That the cull changes no pixel is proved by
/// `rendering::object_viewcone_cull`.
///
/// **`object_viewcone` gates the DRAW and nothing else, and that separation is what this test
/// pins.** It is the reason the flip is safe for the selection latch: `WorldScene::part_cone` runs
/// on every submitted part on both arms, and the latch reads the real cone answer on both.
/// So **both arms are driven here**,
/// not just the shipped one — the `false` arm is the only configuration in which "submitted" and
/// "in the cone" are different sets *observable on the same frame*.
///
/// If either arm reverted to latching on submission, the latch assertion on that arm would fail.
#[test]
fn the_shipping_default_is_the_clients_cull_and_the_latch_is_real() {
    // The default itself, pinned as a literal: a reader must be able to see which way round it is
    // without chasing a symbol.
    assert!(
        SceneConfig::default().object_viewcone,
        "the view-cone cull changes no pixel, so this build culls what retail's mesh drawing \
         culls"
    );

    // ---- the shipped arm: inherited, not set ----
    let mut b = two_monsters();
    assert!(
        b.scene.draw.cfg.object_viewcone,
        "the bench inherits the default and does not set it"
    );
    b.scene.set_selected_object_id(Some(BEHIND));
    b.draw();

    let cone = b.scene.drawn_object_cone();
    assert!(cone.tested > 0 && cone.outside > 0, "{cone:?}");
    assert_eq!(
        cone.culled, cone.outside,
        "at the shipped default every rejection skips: {cone:?}"
    );
    // The object behind the camera never reaches the device draw, exactly as in retail.
    assert_eq!(
        b.parts_submitted_for(BEHIND),
        0,
        "at the shipped default an OUTSIDE mesh is not drawn at all"
    );
    assert!(!b.scene.take_selected_part_drawn());
    // The premise that makes that zero a measurement: the frame drew something.
    assert!(
        b.parts_submitted_for(AHEAD) > 0,
        "the frame is not simply empty"
    );

    // ---- the ungated arm, driven on the same bench: the latch is the cone's answer and not the
    // submission, which is only visible where the two differ ----
    b.scene.draw.cfg.object_viewcone = false;
    b.draw();
    let cone = b.scene.drawn_object_cone();
    assert!(cone.tested > 0 && cone.outside > 0, "{cone:?}");
    assert_eq!(
        cone.culled, 0,
        "with the cull off nothing is skipped: {cone:?}"
    );
    assert!(
        b.parts_submitted_for(BEHIND) > 0,
        "premise: with the cull off the out-of-cone object still reaches the device, which is the \
         only state in which the next assertion says anything"
    );
    assert!(
        !b.scene.take_selected_part_drawn(),
        "physics-part drawing's first test is the cone and not the submission: a part was \
         submitted for {BEHIND:?} and the cone still said outside"
    );

    // And the positive on both arms, so the two `false`s above are about the cone and not about
    // the configuration having disabled the producer altogether.
    for viewcone in [false, true] {
        b.scene.draw.cfg.object_viewcone = viewcone;
        b.scene.set_selected_object_id(Some(AHEAD));
        b.draw();
        assert!(b.parts_submitted_for(AHEAD) > 0, "arm {viewcone}");
        assert!(
            b.scene.take_selected_part_drawn(),
            "the latch rises for what is in the cone on both arms (arm {viewcone})"
        );
    }
}

/// **The calibration positive.** An object in front of the camera is inside the cone, is submitted,
/// and raises the latch — so a cone that rejected everything could not pass this.
#[test]
fn an_object_in_front_is_inside_the_cone_and_raises_the_latch() {
    let mut b = two_monsters();
    b.scene.set_selected_object_id(Some(AHEAD));
    b.draw();
    assert!(
        b.parts_submitted_for(AHEAD) > 0,
        "premise: {AHEAD:?} was submitted"
    );
    assert!(
        b.scene.take_selected_part_drawn(),
        "the mesh cone test answered inside for a part of the selection"
    );
}

// =================================================================================================
// 2. Submitted, and outside the cone
// =================================================================================================

/// **The latch is the cone's answer, not the submission.**
///
/// With the cull **off** ([`SceneConfig::object_viewcone`] false: every submitted part is drawn
/// whatever the cone says) the object behind the camera is *submitted*. So on one frame:
///
/// * `parts_submitted_for(BEHIND) > 0`. That is the submission predicate, and it is **true**: a
///   latch whose whole condition was `watched != 0 && s.object == watched` inside this loop would
///   rise here.
/// * `take_selected_part_drawn()` is **false**. That is the mesh draw's answer, and the cone says
///   outside.
///
/// The submission is the same on both arms, which is why the switch is on the *cull* and not on
/// the *latch*. The latch's
/// rule is the same on both arms; what the flag changes is whether the part
/// that the cone rejected still reaches the device, and it is set to `false` precisely so the part
/// **is** submitted and the two predicates can be read on the same frame.
#[test]
fn an_object_submitted_but_outside_the_cone_does_not_raise_the_latch() {
    let mut b = two_monsters();
    b.scene.draw.cfg.object_viewcone = false;
    b.scene.set_selected_object_id(Some(BEHIND));
    b.draw();

    let cone = b.scene.drawn_object_cone();
    assert!(
        cone.tested > 0,
        "the cone still runs with the cull off: {cone:?}"
    );
    assert_eq!(
        cone.culled, 0,
        "with the cull off nothing is skipped: {cone:?}"
    );
    assert!(
        cone.outside > 0,
        "and it still rejects the object behind the camera: {cone:?}"
    );

    // The submission predicate, on this frame: yes.
    assert!(
        b.parts_submitted_for(BEHIND) > 0,
        "the station requires the object to be SUBMITTED; it was not, so the two predicates \
         cannot be compared on this frame"
    );
    // The cone predicate, on the same frame: no.
    assert!(
        !b.scene.take_selected_part_drawn(),
        "mesh drawing answered outside the view cone, so physics-part drawing must not raise the selected-object latch"
    );

    // And the same object, in front, on the same bench and the same arm: the latch does rise. So
    // the `false` above is about the cone and not about the object, the id, or the arm.
    b.scene.set_selected_object_id(Some(AHEAD));
    b.draw();
    assert!(b.parts_submitted_for(AHEAD) > 0);
    assert!(
        b.scene.take_selected_part_drawn(),
        "the discriminator is the cone and nothing else"
    );
}

/// Behaviour: rendering.culling.an-object-outside-the-view-cone-is-not-drawn
/// With the cull **on** — the shipped configuration — the same object is not submitted at all, so
/// the latch cannot rise for a second, independent reason.
///
/// Both facts are worth having: retail does not draw it either. Its mesh draw reaches the device
/// only on a not-outside answer. A reader who checks only the shipped arm
/// cannot tell "not drawn" from "drawn and not latched". The test above is the one that separates
/// them.
#[test]
fn with_the_cull_on_the_object_behind_the_camera_is_not_drawn_at_all() {
    let mut b = two_monsters();
    // **Inherited**: this is the shipped configuration; see [`the_shipping_default_is_the_clients_cull_and_the_latch_is_real`]. Asserted rather
    // than set, so that a default moving back to `false` reddens here instead of turning this test
    // into a silent duplicate of the ungated arm.
    assert!(
        b.scene.draw.cfg.object_viewcone,
        "premise: this arm is the shipped default, inherited and not set"
    );
    b.scene.set_selected_object_id(Some(BEHIND));
    b.draw();

    assert_eq!(
        b.parts_submitted_for(BEHIND),
        0,
        "an OUTSIDE mesh never reaches internal mesh submission"
    );
    assert!(!b.scene.take_selected_part_drawn());
    // The premise that makes that zero mean something: the frame drew *something*.
    assert!(
        b.parts_submitted_for(AHEAD) > 0,
        "the frame is not simply empty"
    );
}

/// **The view polygon's own edge planes, told apart from the near plane.**
///
/// [`BEHIND`] is rejected by its camera-forward coordinate alone, so it cannot show whether the
/// view planes reach the cone (removing them from `WorldScene::part_cone` leaves the tests above
/// green). This station can.
///
/// [`SIDE`] is ~85° off axis and **in front** of the near plane, so only the edge planes can reject
/// it. Both arms are driven: with the cull on it is not submitted at all, and with the cull off it
/// is submitted and still does not latch — the same submitted-but-outside station as [`BEHIND`],
/// reached through a different plane of the same cone.
#[test]
fn an_object_in_front_but_off_to_the_side_is_rejected_by_the_view_polygons_own_planes() {
    let mut b = two_monsters();
    // The shipped arm; see
    // [`the_shipping_default_is_the_clients_cull_and_the_latch_is_real`]. Set explicitly anyway, so
    // that this station states which arm it is about rather than depending on a default.
    b.scene.draw.cfg.object_viewcone = true;
    b.scene.set_selected_object_id(Some(SIDE));
    b.draw();
    assert!(
        b.parts_submitted_for(AHEAD) > 0,
        "premise: the frame draws something"
    );
    assert_eq!(
        b.parts_submitted_for(SIDE),
        0,
        "an object 85 degrees off axis is outside the view polygon's edge planes, and the near \
         plane -- which admits it -- is not what decides that"
    );
    assert!(!b.scene.take_selected_part_drawn());

    b.scene.draw.cfg.object_viewcone = false;
    b.draw();
    assert!(
        b.parts_submitted_for(SIDE) > 0,
        "with the cull off it is submitted"
    );
    assert!(
        !b.scene.take_selected_part_drawn(),
        "and the mesh cone test still answers outside for it"
    );
    // The premise that makes the zero above a measurement rather than a silence: the cone was
    // asked, and it rejected exactly the two objects the stations put outside it.
    let cone = b.scene.drawn_object_cone();
    assert!(cone.tested >= 3, "{cone:?}");
    assert!(cone.outside > 0 && cone.outside < cone.tested, "{cone:?}");
}

/// **A part is drawn when its whole drawing sphere is not outside the cone: straddling the edge
/// is inside.** [`EDGE`] stands across the right-hand edge plane, so with the cull on some of its
/// parts are culled and the rest are drawn; the drawn ones are exactly those whose sphere, at its
/// full scaled radius, still reaches into the view. The selection latch rises for it too.
///
/// Falsified by testing a smaller sphere than the part's own (a quarter radius draws none of it)
/// and by culling parts that straddle the cone rather than only parts wholly outside it (also
/// none). The stations above cannot see either: they are wholly in or wholly out.
#[test]
fn a_body_standing_across_the_edge_draws_the_parts_whose_spheres_reach_into_the_view() {
    let mut b = Bench::new();
    b.place_monster(AHEAD, AHEAD_AT);
    b.place_monster(EDGE, EDGE_AT);

    b.scene.draw.cfg.object_viewcone = false;
    b.draw();
    let all = b.parts_submitted_for(EDGE);
    assert!(all > 0, "premise: with the cull off the body is submitted");

    b.scene.draw.cfg.object_viewcone = true;
    b.scene.set_selected_object_id(Some(EDGE));
    b.draw();
    let drawn = b.parts_submitted_for(EDGE);
    assert!(
        drawn < all,
        "premise: the edge plane cuts the body, so some of its {all} subsets are culled; {drawn} \
         were drawn"
    );
    assert!(
        drawn > 0,
        "a body across the edge drew none of its {all} subsets: the parts whose spheres reach \
         back into the view were culled as if wholly outside ({:?})",
        b.scene.drawn_object_cone()
    );
    assert!(
        b.scene.take_selected_part_drawn(),
        "and the cone answered not-outside for a part of it, so the latch rises"
    );
}

// =================================================================================================
// 3. The cone latch is a subset of the submission latch
// =================================================================================================

/// **Latching on submission alone can only make the latch rise EARLIER than the cone rule, never
/// fail to rise.**
///
/// That is a claim about set inclusion, so it is tested as one: over the same frames, every object
/// the **cone** rule latches on is one the **submission** rule would have latched on too. The
/// submission rule is "a part of the selection was submitted"; with the cull off, the parts the new rule
/// accepts are a subset of the parts submitted, because the acceptance is a filter applied inside
/// that loop.
///
/// So the submission rule's only error is a selection kept that retail drops (a target the player
/// walked out of range of **without ever having it on screen**); it never clears a selection
/// retail keeps.
#[test]
fn the_new_latch_is_a_strict_subset_of_the_old_one_so_nothing_retail_kept_was_dropped() {
    let mut b = two_monsters();
    b.scene.draw.cfg.object_viewcone = false;

    for (id, in_cone) in [(AHEAD, true), (BEHIND, false)] {
        b.scene.set_selected_object_id(Some(id));
        b.draw();
        let submitted = b.parts_submitted_for(id) > 0;
        let latched = b.scene.take_selected_part_drawn();
        assert!(
            submitted,
            "premise: with the cull off both objects are submitted, {id:?}"
        );
        assert_eq!(latched, in_cone, "the cone rule's answer for {id:?}");
        // Inclusion, stated as the implication it is: latched -> submitted.
        assert!(
            !latched || submitted,
            "the cone rule cannot fire where the submission rule did not"
        );
    }

    // And the case that would refute it (latched by the cone rule, not submitted) is
    // unreachable by construction, because the latch is raised inside the submission loop. Said
    // rather than left as an untested branch: there is no station to build for it.
}

const W: u32 = 640;
const H: u32 = 480;

/// The retail store, or **fail**: a skipped test and a passing test are the same green line, so
/// this file has no skip in it.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

// -------------------------------------------------------------------------------------------
// The capture reader and the replay; `load` is read once so one file read serves every arm of a
// differential.
// -------------------------------------------------------------------------------------------

/// A recording's datagrams, through the shared reader (parsed once per test binary).
fn load(session: &str) -> Vec<Record> {
    capture::shared_session(session).to_vec()
}

fn connection_sequence_number(records: &[Record]) -> u32 {
    recording::connection_sequence_number(records).expect("the recording has a LoginRequest")
}

struct Replayed {
    objects: ObjectStream,
    landblock: u16,
}

/// Replay one capture up to the last datagram at which the client still held objects: every
/// recording ends with a clean logout and the end-of-session teardown empties the object model,
/// so replaying to the end leaves a scene with nothing in it.
///
/// **Deterministic, and re-run per arm rather than shared.** `WorldScene::sync_objects` takes the
/// stream by `&mut` and advances its physics, so the two halves of a differential cannot share one:
/// the second would start from the first's end state and the "same frame number" premise would be
/// false. The replay is a pure function of the records and the port, so re-running it gives the two
/// arms genuinely equal starting states.
fn replay(records: &[Record]) -> Replayed {
    let seq = connection_sequence_number(records);
    let run = |limit: usize| -> (ObjectStream, usize, Option<u16>) {
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", seq).expect("host");
        let mut objects = ObjectStream::new();
        let mut entered = false;
        let mut last = 0usize;
        let mut block = None;
        for (index, r) in records.iter().enumerate() {
            if index >= limit {
                break;
            }
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
            if !objects.is_empty() {
                last = index;
            }
            if block.is_none() {
                if let Some(p) = objects.player().and_then(|id| objects.presence(id)) {
                    if let Some(pos) = p.position {
                        let b = pos.cell.landblock();
                        block = Some((u16::from(b.x()) << 8) | u16::from(b.y()));
                    }
                }
            }
        }
        (objects, last, block)
    };
    let (_, last, _) = run(usize::MAX);
    let (objects, _, block) = run(last + 1);
    assert!(
        !objects.is_empty(),
        "the scene is empty at its most populated datagram"
    );
    Replayed {
        objects,
        landblock: block.expect("the capture's player has a position"),
    }
}

// -------------------------------------------------------------------------------------------
// The scene and the frame
// -------------------------------------------------------------------------------------------

/// The capture's own landblock at the **shipped** land and scenery radii, with the cull as the
/// argument and nothing else moved.
///
/// `character: false` because the station has to be a fixed pose: with a body,
/// `crate::camera::update_viewer` overwrites `self.camera` from the chase orbit every frame and a
/// camera set by hand does not survive `update`. What is lost is the local body's own parts, which
/// also pass through the cone; the replayed objects are dozens of creatures and items and they are
/// the population this measures.
///
/// `particles: false` and a pinned `time_of_day` because a differential is evidence only when the
/// control is, and both of those move on wall-clock-ish inputs. Weather is switched off at the
/// scene for the same reason.
fn cfg_for(landblock: u16, viewcone: bool) -> SceneConfig {
    SceneConfig {
        landblock,
        character: false,
        // The shipped radii, deliberately: the backdrop a culled object would have been drawn
        // *against* is the thing that decides whether its absence shows, so it is real terrain and
        // real scenery and not a cleared buffer.
        land_radius: 3,
        scenery_radius: 1,
        time_of_day: Some(0.5),
        particles: false,
        // **The only difference between the two arms of every differential below.**
        object_viewcone: viewcone,
        ..SceneConfig::default()
    }
}

fn step(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    s: &mut ObjectStream,
    t: f64,
) {
    scene.sync_objects(store, gpu, s).expect("sync_objects");
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(t),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
}

fn object_centroid(scene: &WorldScene, s: &ObjectStream) -> Vec3 {
    let origins: Vec<Vec3> = s
        .presences()
        .filter_map(|(id, _)| scene.server_object_frame(id))
        .map(|f| f.origin)
        .collect();
    assert!(
        !origins.is_empty(),
        "the scene is drawing nothing to measure a centroid from"
    );
    // LINT-OK: a count of drawn objects, in the hundreds.
    #[allow(clippy::cast_precision_loss)]
    let n = origins.len() as f32;
    let sum = origins.iter().fold(Vec3::ZERO, |a, b| {
        Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
    });
    Vec3::new(sum.x / n, sum.y / n, sum.z / n)
}

/// Park the camera over the objects and let the render space settle, returning the centroid in the
/// space every later station is measured in. The capture's objects straddle a landblock boundary, so moving the camera there re-centres the
/// streaming window and re-derives every object frame against a new origin. Move, step, re-measure,
/// and assert the point has stopped moving before anything is read off it.
fn settle(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    s: &mut ObjectStream,
) -> Vec3 {
    let mut centre = Vec3::ZERO;
    let mut last = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    for i in 0..6 {
        step(store, gpu, scene, s, 0.1 * f64::from(i));
        centre = object_centroid(scene, s);
        let moved = ((centre.x - last.x).powi(2)
            + (centre.y - last.y).powi(2)
            + (centre.z - last.z).powi(2))
        .sqrt();
        last = centre;
        if moved < 0.5 {
            return centre;
        }
        scene.camera.position = Vec3::new(centre.x, centre.y, centre.z + 4.0);
    }
    panic!("the render space never settled: the centroid is still moving at {centre:?}");
}

/// One camera station: an offset from the settled centroid, and where it looks.
struct Station {
    label: &'static str,
    /// Metres from the centroid, in render space.
    offset: (f32, f32, f32),
    yaw: f32,
    pitch: f32,
}

/// The frame a station produced, plus the cone's own census of it.
struct Shot {
    rgba: Vec<u8>,
    cone: ObjectConeStats,
    /// Parts `WorldScene::draw` submitted this frame — the denominator behind `tested`.
    parts: usize,
}

/// Drive one arm to one station and read the frame back.
fn shoot(store: &Arc<RetailDatStore>, records: &[Record], viewcone: bool, st: &Station) -> Shot {
    tour(store, records, viewcone, std::slice::from_ref(st))
        .pop()
        .expect("one station, one shot")
}

/// Drive one arm through `stations` in order and read each station's frame back.
///
/// Every arm gets its **own device and its own scene**, so the two halves of a differential cannot
/// share descriptor state, and its own replay, so they cannot share object physics. Within an arm
/// the stations share the one scene. The two arms are driven through the identical sequence of
/// `step` calls at the identical times, visit the stations in the identical order, and set each
/// pose from the identical settled centroid, so *"same pose, same frame number"* is a property of
/// the construction rather than a comment.
fn tour(
    store: &Arc<RetailDatStore>,
    records: &[Record],
    viewcone: bool,
    stations: &[Station],
) -> Vec<Shot> {
    let mut gpu = crate::common::test_gpu(W, H);
    let mut r = replay(records);
    let mut scene =
        WorldScene::load(store, &mut gpu, cfg_for(r.landblock, viewcone)).expect("the scene loads");
    scene.set_weather_enabled(false);
    let centre = settle(store, &mut gpu, &mut scene, &mut r.objects);

    let mut shots = Vec::with_capacity(stations.len());
    for (k, st) in stations.iter().enumerate() {
        // Two seconds of client time per station, the first station at 40 s.
        let t = 40.0 + 2.0 * f64::from(u32::try_from(k).expect("a station index"));
        let pose = |scene: &mut WorldScene| {
            scene.camera.position = Vec3::new(
                centre.x + st.offset.0,
                centre.y + st.offset.1,
                centre.z + st.offset.2,
            );
            scene.camera.yaw = st.yaw;
            scene.camera.pitch = st.pitch;
        };
        pose(&mut scene);

        // One settling frame at the new pose — the streaming window and the degrade state both
        // react to where the camera is — and then the frame that is measured, at the same time on
        // both arms.
        step(store, &mut gpu, &mut scene, &mut r.objects, t);
        pose(&mut scene);

        scene
            .sync_objects(store, &mut gpu, &mut r.objects)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(t + 1.0),
            1.0 / 30.0,
        );
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");

        let img = gpu.capture().expect("capture");
        assert_eq!(
            (img.width, img.height),
            (W, H),
            "the capture is not the station's viewport"
        );
        shots.push(Shot {
            rgba: img.to_rgba(),
            cone: scene.drawn_object_cone(),
            parts: scene.drawn_part_order().len(),
        });
    }
    shots
}

/// The differing pixels between two captures, as indices.
fn differing(a: &[u8], b: &[u8]) -> Vec<usize> {
    assert_eq!(a.len(), b.len(), "two captures of different sizes");
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0.iter())
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .map(|(i, _)| i)
        .collect()
}

/// Where the changed pixels are, so a non-zero reading is a finding and not just a count: a
/// non-zero difference means the cone is wrong somewhere, and this says where.
fn bounds(pixels: &[usize]) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for &i in pixels {
        // LINT-OK: a pixel index into a 640x480 buffer, back to its (x, y).
        #[allow(clippy::cast_possible_truncation)]
        let (x, y) = (i as u32 % W, i as u32 / W);
        b = Some(match b {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        });
    }
    b
}

/// The stations. Eight yaws around the full circle at eye height, two vertical looks, and one
/// close-quarters station.
///
/// The yaw sweep is what makes *"objects behind the camera"* a fact about the scene rather than a
/// hope: at any one heading the town's population is split by the cone, and over eight headings
/// every object in it is on both sides of that split at some station. The two vertical looks and
/// the close-quarters station exist for the **near plane** specifically, the residual risk:
/// looking straight down from four metres puts objects within the first metre of the view, and
/// `NEAR` stands inside the crowd so that objects straddle the viewer's own forward plane rather
/// than only the edge planes.
fn stations() -> Vec<Station> {
    let mut v: Vec<Station> = Vec::new();
    let labels = [
        "yaw0", "yaw45", "yaw90", "yaw135", "yaw180", "yaw225", "yaw270", "yaw315",
    ];
    for (i, label) in labels.into_iter().enumerate() {
        // LINT-OK: a station index below 8.
        #[allow(clippy::cast_precision_loss)]
        let yaw = std::f32::consts::FRAC_PI_4 * i as f32;
        v.push(Station {
            label,
            offset: (0.0, 0.0, 1.8),
            yaw,
            pitch: 0.0,
        });
    }
    // Straight down from just above the crowd: the objects are within the first few metres of the
    // view, which is where a near plane that disagreed with the projection would show.
    v.push(Station {
        label: "down",
        offset: (0.0, 0.0, 4.0),
        yaw: 0.0,
        pitch: -1.5,
    });
    // Straight up: almost everything is behind the cone, so `culled` is at its largest and the
    // picture is at its most sensitive to a cull that took one object too many.
    v.push(Station {
        label: "up",
        offset: (0.0, 0.0, 1.8),
        yaw: 0.0,
        pitch: 1.5,
    });
    // Close quarters: displaced off the centroid so the nearest objects are metres away and
    // partially behind the eye.
    v.push(Station {
        label: "near",
        offset: (3.0, 3.0, 1.2),
        yaw: 2.2,
        pitch: -0.3,
    });
    v
}

/// The reference station for the control: the middle of the crowd, looking horizontally.
fn reference() -> Station {
    Station {
        label: "yaw0",
        offset: (0.0, 0.0, 1.8),
        yaw: 0.0,
        pitch: 0.0,
    }
}

// =================================================================================================
// 1. The control, taken first
// =================================================================================================

/// **The noise floor.** Two independent builds of the same scene at the same station, with
/// `SceneConfig::object_viewcone` **not** changed between them — a fresh WARP device, a fresh
/// replay of the capture and a fresh `WorldScene` each time.
///
/// A differential is evidence only when the control is stable. Everything the next test
/// measures is measured against this number, so it is taken **first** and reported whatever it is.
/// If it were not zero, the cull differential would have to clear it rather than clear zero, and
/// the whole reading would change character.
///
/// It also pins the thing that makes the next test's premise legible: the cone runs, and rejects
/// things, on an arm that culls nothing. Both arms of every differential here name their flag
/// explicitly rather than inheriting it, so this file measures the same two configurations whatever
/// `SceneConfig::default()` says.
#[test]
fn the_control_is_the_noise_floor() {
    let store = store();
    let records = load("first-login-walk-jump");
    let st = reference();

    let a = shoot(&store, &records, false, &st);
    let b = shoot(&store, &records, false, &st);

    let changed = differing(&a.rgba, &b.rgba);
    eprintln!(
        "view-cone control @ {}: {} of {} pixels differ (noise floor). \
         a: parts={} tested={} outside={} culled={} no_sphere={}; \
         b: parts={} tested={} outside={} culled={} no_sphere={}",
        st.label,
        changed.len(),
        W * H,
        a.parts,
        a.cone.tested,
        a.cone.outside,
        a.cone.culled,
        a.cone.no_sphere,
        b.parts,
        b.cone.tested,
        b.cone.outside,
        b.cone.culled,
        b.cone.no_sphere,
    );

    // The premise that makes the control a control: the two runs really are the same arm.
    assert_eq!(
        a.cone, b.cone,
        "the two control runs did not even agree on the cone's census"
    );
    assert_eq!(
        a.parts, b.parts,
        "the two control runs submitted different numbers of parts"
    );
    assert_eq!(
        a.cone.culled, 0,
        "the control arm culled something: {:?}",
        a.cone
    );
    // And the premise that makes it a *scene*: the cone was asked, and it discriminated.
    assert!(
        a.cone.tested > 0,
        "the cone was never asked about anything: {:?}",
        a.cone
    );
    assert!(
        a.cone.outside > 0,
        "nothing is outside the cone at the control station: {:?}",
        a.cone
    );
    assert!(
        a.cone.outside < a.cone.tested,
        "everything is outside it: {:?}",
        a.cone
    );

    assert!(
        changed.is_empty(),
        "the noise floor is {} pixels in {:?}, not zero -- every differential in this file must \
         then be read against that number rather than against zero",
        changed.len(),
        bounds(&changed)
    );
}

// =================================================================================================
// 2. The differential
// =================================================================================================

/// Behaviour: rendering.culling.the-view-cone-cull-changes-no-pixel
/// **The paired-frame differential.** One driven scene, same pose, same frame number,
/// `SceneConfig::object_viewcone` the only difference — and the two frames must be identical.
///
/// The premise is asserted at every station: `culled == 0` on the off
/// arm and `culled > 0` on the on arm, so the two runs are demonstrably *not* identical by
/// construction and a zero pixel difference is a fact about the cull rather than about the harness.
/// `tested` and `outside` are asserted **equal** across the arms as well, which is the separate
/// claim that the flag gates the draw and nothing else: the cone runs on the same parts and answers
/// the same for them either way, and only the `continue` before `draw_part` moves.
///
/// `tested` / `outside` / `culled` are printed beside the pixel count at every station so the
/// denominator is visible.
#[test]
fn the_cull_changes_no_pixel_at_any_station() {
    let store = store();
    let records = load("first-login-walk-jump");

    let mut total_culled = 0u32;
    let mut total_changed = 0usize;
    let mut worst: Option<(&'static str, usize, Option<(u32, u32, u32, u32)>)> = None;

    let stations = stations();
    let offs = tour(&store, &records, false, &stations);
    let ons = tour(&store, &records, true, &stations);
    for ((st, off), on) in stations.iter().zip(offs).zip(ons) {
        let changed = differing(&off.rgba, &on.rgba);
        eprintln!(
            "view-cone @ {:>6}: tested={:<5} outside={:<5} culled={:<5} no_sphere={} \
             parts off={:<5} on={:<5} | {} of {} pixels differ",
            st.label,
            on.cone.tested,
            on.cone.outside,
            on.cone.culled,
            on.cone.no_sphere,
            off.parts,
            on.parts,
            changed.len(),
            W * H,
        );

        // (a) The premise. Without this the zero below means nothing.
        assert_eq!(
            off.cone.culled, 0,
            "@{}: the off arm culled something: {:?}",
            st.label, off.cone
        );
        assert!(
            on.cone.culled > 0,
            "@{}: the ON arm culled NOTHING, so the two frames are identical by construction and \
             a zero pixel difference would be measuring the harness: {:?}",
            st.label,
            on.cone
        );
        // (b) The flag gates the draw and nothing else -- asserted rather than stated.
        assert_eq!(
            on.cone.tested, off.cone.tested,
            "@{}: the cone ran on a different number of parts with the cull on",
            st.label
        );
        assert_eq!(
            on.cone.outside, off.cone.outside,
            "@{}: the cone answered differently with the cull on",
            st.label
        );
        assert_eq!(
            on.cone.culled, on.cone.outside,
            "@{}: with the cull on, every rejection must skip its draw: {:?}",
            st.label, on.cone
        );
        // (c) And the cull really did remove submissions, which is the mechanism the pixels are
        //     being asked about.
        assert!(
            on.parts < off.parts,
            "@{}: the cull skipped {} draws and yet the same number of parts reached the device",
            st.label,
            on.cone.culled
        );

        total_culled += on.cone.culled;
        total_changed += changed.len();
        if worst.is_none_or(|(_, n, _)| changed.len() > n) {
            worst = Some((st.label, changed.len(), bounds(&changed)));
        }
    }

    assert!(total_culled > 0, "no station culled anything");
    assert_eq!(
        total_changed, 0,
        "the cull changed {total_changed} pixels across the stations; the worst is {worst:?}. \
         The control in `the_control_is_the_noise_floor` is 0, so this is the cull and not the \
         harness -- an object whose view-cone classification is OUTSIDE still \
         projected onto the viewport. Suspect `WorldScene::viewer_near_plane`'s `ZNEAR` against \
         `WorldScene::view_params`'s projection, `eye_transform`'s matrices, or `part_cone`'s \
         spaces, in that order."
    );
}

// =================================================================================================
// 3. The residual risk, pinned rather than argued
// =================================================================================================

/// **The cone's near plane and the projection's near plane are the same number, and there is one
/// producer for it.**
///
/// A `znear` disagreeing with the projection the frame is actually drawn with would clip objects
/// the picture still wants.
/// `WorldScene::viewer_near_plane` passes `dereth_render::camera::ZNEAR` to
/// `cells::cull::viewer_near_plane`, and `WorldScene::view_params` builds its `ViewParams` from
/// `..ViewParams::default()`, whose `znear` is that same constant, overriding only `view`,
/// `fov_y_rad`, `aspect` and `viewport`. The differential above is the measurement; this is the
/// guard that keeps it true, because a second producer for `ViewParams::znear` — the obvious
/// candidate being the view-distance adjustment, which drops it to `d * 0.25` below `d = 0.4` —
/// would silently break the agreement without changing any frame this file drives.
#[test]
fn the_cone_and_the_projection_share_a_znear() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);
    let r = replay(&load("first-login-walk-jump"));
    let scene =
        WorldScene::load(&store, &mut gpu, cfg_for(r.landblock, true)).expect("the scene loads");

    let view = scene.view_params(W, H);
    assert!(
        (view.znear - dereth_render::camera::ZNEAR).abs() < f32::EPSILON,
        "the frame is drawn with znear {} and the cone's viewer near plane is built from \
         `dereth_render::camera::ZNEAR` = {}; the pixel differential is only valid while these \
         agree",
        view.znear,
        dereth_render::camera::ZNEAR
    );
    // `set_vdst`'s lower branch is the one that could break this, and it is named so a reader
    // knows which producer to look for rather than having to rediscover it.
    let (_, vdst_znear) = dereth_render::camera::set_vdst(0.2);
    assert!(
        vdst_znear < dereth_render::camera::ZNEAR,
        "the near-plane distance calculation is the producer that would disagree; if it no longer \
         produces a smaller znear, this guard is pointing at the wrong function"
    );
}
