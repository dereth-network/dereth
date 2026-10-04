//! The selection survives what a player does between two presses, and a range exit keeps a drawn
//! selection: the cycle advances one object per press across examines, uses and idle frames; a
//! wrap is a re-call and not a cleared selection; the draw-time setter matches the selected id; a
//! world click lowers the in-view latch and the next draw raises it; a driven run keeps the
//! selection across every range exit once drawn; and the range edge is the radar radius. Fixture:
//! a software GPU device, retail data and a `WorldScene` with a real body in a real cell; tests
//! call `dereth_client::interaction::use_time` with constructed events and optionally run the
//! scene sync/update/stream/draw helper in `App::frame`'s order (no OS input, no full app frame).
//! Objects sit on 3/4/5 headings, and every placement asserts the offset it actually landed at.
//!
//! # 1. Persistence over a sequence, not at a station
//!
//! A one-action test cannot see a persistence defect, so
//! [`the_cycle_advances_one_object_per_press_across_the_acts_between_them`] drives
//! **select, act, select, act, select** through the frame and asserts the walk **advances**, which
//! a stuck cycle and a cleared selection both fail. The acts in between are the ones a player
//! performs without meaning to change the target: an examine, a use, and plain frames that run the
//! object-range poll.
//!
//! # 2. The selection-survival edge
//!
//! The range-exit path re-arms the watch when the selected object was drawn in view and deselects
//! when it was not. Drawing the selected object's physics part is the flag's only setter, and a
//! click-pick is its only clear, so it is a **latch**: up for anything a frame has drawn since the
//! last click. The drawn run requires at least one exit per outward walk, exactly six re-arms and
//! no clears, with the target still selected at the end; the never-drawn arm is its control, and
//! every exit there clears.
//!
//! **The discriminator is [`Bench::draw_frames`], not a flag set by hand.** Both arms drive the
//! same six outward and return movements. The drawn arm also runs the scene sync, update, stream
//! and draw helper; the never-drawn control omits that helper and reselects after each clear so the
//! next cycle can continue.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::{SceneReads, SceneWrites};
use std::sync::Arc;

use dereth_client::character::PLAYER_OBJECT_ID;
use dereth_client::interaction::{action as ia, Interaction};
use dereth_client::objects::ObjectStream;
use dereth_client::selection_geometry::SceneSelectionPhysics;
use dereth_client::ui::UiMouseEvent;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_model::range::RADAR_RADIUS_OUTDOORS;
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::RetailDatStore;
use dereth_input::ActionId;
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, ObjectId, Position, Quat, ServerTime, Vec3};
use dereth_render::device::{DeviceConfig, Gpu};

// Three monsters on a 53.13° heading at 3 m, 9 m and 30 m — non-cardinal throughout.
const NEAR: ObjectId = ObjectId(0x8300_0001);
const MID: ObjectId = ObjectId(0x8300_0002);
const FAR: ObjectId = ObjectId(0x8300_0003);
const NEAR_AT: (f32, f32, f32) = (1.8, 2.4, 0.0);
const MID_AT: (f32, f32, f32) = (5.4, 7.2, 0.0);
const FAR_AT: (f32, f32, f32) = (18.0, 24.0, 0.0);
/// Beyond the outdoor radar radius, which is the range the selection watch is armed at.
const OUT_OF_RANGE_AT: (f32, f32, f32) = (54.0, 72.0, 0.0); // 90 m
/// The same 90 m, behind the player, where no frame draws it.
const OUT_OF_RANGE_BEHIND_AT: (f32, f32, f32) = (-54.0, -72.0, 0.0);
/// `0x02000001`, the Aluvian male setup — a creature body, which is what a monster station wants
/// and what `SceneRangeGeometry` needs on both ends of its measurement.
const MONSTER_SETUP: u32 = 0x0200_0001;

// =================================================================================================
// Bench — the selection bench, with an optional scene draw after each interaction pass
// =================================================================================================

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    inter: Interaction,
    player: Position,
    now: f64,
    instance: u16,
    /// Whether [`Bench::frame`] runs the local scene sync/update/stream/draw helper after
    /// interaction processing.
    ///
    /// Running that helper can mark the selected object in view; omitting it models an object
    /// selected from a panel and moved away before a scene draw carried its parts. It drives the
    /// two range-exit outcomes with the different control behavior stated by the station below.
    draw_frames: bool,
}

impl Bench {
    fn new() -> Self {
        let store = Arc::new(
            dereth_dat::testing::open_store()
                .expect("the retail dats are `use_time`'s own argument: set DERETH_TEST_DAT_DIR"),
        );
        let cfg = DeviceConfig {
            width: 800,
            height: 600,
            ..DeviceConfig::default()
        };
        let mut gpu = Gpu::new(None, &cfg).expect("a software GPU device");
        let region = dereth_client::world::load_region(&store).expect("the region decodes");
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
        assert!(
            dereth_physics::landdefs::is_outdoors(player.cell),
            "the premise: the body is outdoors, so the watch is armed at the outdoor 75.0"
        );

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
            inter: Interaction::new(),
            player,
            now: 1.0,
            instance: 0,
            draw_frames: true,
        }
    }

    /// The global origin of a player-space offset, in the body's own cell.
    fn origin_of(&self, offset: (f32, f32, f32)) -> Vec3 {
        dereth_physics::math::localtoglobal(
            &self.player.frame,
            Vec3::new(offset.0, offset.1, offset.2),
        )
    }

    /// Assert an object really is at the player-space offset the station asked for — the premise,
    /// without which a filter that refused it and a placement that missed read alike.
    ///
    /// **The presence and the physics body are one position.** `SceneSelectionPhysics` is built
    /// from `ObjectStream::presences()`, while `SceneRangeGeometry`, which the watch under test
    /// measures with, reads the physics body's position; `ObjectStream::publish_physics_cells`
    /// republishes the physics position onto the presence every frame, so both read the same
    /// position, as the client's player-space conversion and range query do. A station that
    /// checked only the presence could not tell "the body is there" from "the body ignored the
    /// update and only the record moved". The station checks the requested horizontal offset
    /// against the seam, the requested horizontal position against the physics body, and all
    /// three seam coordinates against that body.
    ///
    /// **The offset checks are horizontal only.** The station's offsets all carry `z == 0`, i.e.
    /// the player's own height, and the create arm enters the world through full position
    /// placement, which drops the body onto the terrain. Holtburg slopes, so at these offsets the
    /// body rests up to 1.4 m below the player's height — and in retail that dropped `z` is what
    /// the player-space conversion returns for the blip, name, and distance, because the placed
    /// body is the only position there is. A `z` assertion against the wire's word would be
    /// asserting a field retail does not have.
    fn assert_at(&self, id: ObjectId, offset: (f32, f32, f32)) {
        let measured = self
            .phys()
            .get(id)
            .expect("the seam can answer for it")
            .player_space;
        for (got, want, axis) in [(measured.0, offset.0, 'x'), (measured.1, offset.1, 'y')] {
            assert!(
                (got - want).abs() < 1e-3,
                "{id:?} is at player-space {axis} = {got}, the station asked for {want}"
            );
        }
        let want = self.origin_of(offset);
        let h = self.objects.physics.handle(id).expect("the body exists");
        let ch = self.scene.character.as_ref().expect("a body");
        let body = ch
            .world
            .get(h)
            .expect("the body is in the world")
            .position
            .frame
            .origin;
        let d = ((body.x - want.x).powi(2) + (body.y - want.y).powi(2)).sqrt();
        assert!(
            d < 1e-2,
            "{id:?}'s physics-body position is {body:?}, the station asked for {want:?} \
             ({d} m away horizontally) — the record moved and the body did not"
        );
        // And the two are the *same* position: the seam's player space must be the body's player
        // space, not merely near the offset asked for.
        let from_body = dereth_physics::math::localtolocal(
            &self.player,
            &ch.world.get(h).expect("the body is in the world").position,
            Vec3::ZERO,
        );
        for (got, want, axis) in [
            (measured.0, from_body.x, 'x'),
            (measured.1, from_body.y, 'y'),
            (measured.2, from_body.z, 'z'),
        ] {
            assert!(
                (got - want).abs() < 1e-3,
                "{id:?}: the selection seam says player-space {axis} = {got} and \
                 the physics-body position says {want} — there is supposed to be one position"
            );
        }
    }

    /// Create an object at an exact **player-space** offset through the client's own `0xF745`.
    fn put(&mut self, id: ObjectId, offset: (f32, f32, f32)) {
        use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};

        let origin = self.origin_of(offset);
        let payload = dereth_protocol::objects::ObjectCreatePayload {
            id,
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                // `SETUP` as well as `POSITION`, because `SceneRangeGeometry::distance` resolves
                // both ends through object lookup — an object with no physics body answers `None`,
                // which the range query reads as **out
                // of range**, and every watch on it would fire on its first poll. That is a real
                // client behaviour and it is not the one this file measures, so the station has
                // to have a body. It is asserted below rather than assumed.
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
        self.sync_bodies();
        self.assert_at(id, offset);
        self.objects.world.update_visible_object_list();
    }

    /// `ObjectPhysics::sync` — the step `App::frame` runs and `use_time` does not, which is what
    /// gives a created object the physics body the range geometry measures from.
    fn sync_bodies(&mut self) {
        let Self {
            store,
            objects,
            scene,
            ..
        } = self;
        let ch = scene.character.as_mut().expect("a body");
        objects.sync_physics(store, &mut ch.world);
    }

    /// The monster selection kind wants an attackable creature the radar shows.
    fn set_monster(&mut self, id: ObjectId) {
        let w = self
            .objects
            .world
            .tables
            .weenies
            .get_mut(id)
            .expect("placed");
        w.pwd.obj_type |= item_type::CREATURE;
        w.pwd.bitfield |= bitfield::ATTACKABLE;
        w.pwd.radar_enum = Some(4); // ShowAlways
        assert!(
            self.objects.world.object_is_attackable(id),
            "premise: {id:?} is a monster"
        );
        self.objects.world.update_visible_object_list();
        assert!(
            self.objects.world.tables.visible.contains(&id),
            "the visible-object-list refresh must have made {id:?} visible"
        );
        assert!(
            self.objects.physics.handle(id).is_some(),
            "premise: {id:?} has a physics body, so the range geometry can measure to it"
        );
    }

    fn place_monster(&mut self, id: ObjectId, offset: (f32, f32, f32)) {
        self.put(id, offset);
        self.set_monster(id);
    }

    /// **Walking away, as `0xF748 Movement_PositionEvent` — not as a recreate.**
    ///
    /// `0xF7DB Item_UpdateObject` is *wrong for this measurement*: it is a destroy-and-recreate,
    /// so it empties the selection on its own through world recreation and UI removal, and a run
    /// driven with it reports the selection gone with `range_selection_clears` at **0**. Two
    /// different producers of the same visible state, and only one of them is the subject.
    /// `0xF748` moves the object in place through the received-position timestamp gate, which is
    /// what a player walking away actually generates.
    ///
    /// # Two fields the position pack needs, and both are refusals in retail
    ///
    /// The received-position handler passes a non-player body `(position, position_timestamp,
    /// has_contact, velocity)`, and movement then has four arms:
    ///
    /// ```text
    /// if (older POSITION_TS)                       return 0;
    /// if (newer_event(TELEPORT_TS, ts) || !cell) { prepare teleport;
    ///                                              place(pos, flags 0x1012); return 1; }
    /// if (has_contact) {
    ///     if (player_distance < 96.0) { interpolate toward pos; return 1; }
    ///     stop interpolation; place simply at pos;                         return 1;
    /// }
    /// return 0;
    /// ```
    ///
    /// **1. On the ordinary movement arm, with `has_contact` clear, nothing is repositioned.** A
    /// default position pack has `flags == 0`, and the pack encodes `has_contact` as
    /// `(flags >> 2) & 1`, so such an update falls through to `return 0`. The *presence* in
    /// `ObjectStream` would still move while the body stood still, and `SceneRangeGeometry`,
    /// which measures the physics body, would never see an exit. The bit is set here because a
    /// monster standing on Holtburg **is** in contact, and every real `0xF748` for one carries it.
    ///
    /// **2. With contact set the body still does not arrive this frame.** Interpolation only
    /// snaps past the 100 m outdoor autonomy distance. The 90 m and 75 m stations below are
    /// *under* it, so the body walks at twice its adjusted maximum speed: about ten seconds of
    /// frames for one of these hops. One update, one arrival is not retail's behaviour on the
    /// ordinary arm.
    ///
    /// So the relocation is sent as the one `0xF748` arm that *does* land a body at a distance and
    /// bypasses that ordinary-arm contact refusal: the teleport arm, taken when
    /// `newer_event(TELEPORT_TS, ts)` answers true, which then places the body with
    /// `TELEPORT_SPF | DONT_CREATE_CELLS_SPF` (combined value `0x1012`). It is still a `0xF748`
    /// moving the object **in place** — the property the paragraph above needs — and it is
    /// retail's own producer for a discontinuous relocation. The arm is asserted, not assumed.
    fn move_to(&mut self, id: ObjectId, offset: (f32, f32, f32)) {
        use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};

        self.instance += 1;
        let at = self.origin_of(offset);
        let pp = PositionPack {
            // `has_contact`, `(flags >> 2) & 1`, is the movement path's third input. On the
            // ordinary non-teleport arm, clear means "do not reposition this body".
            flags: position_flags::IS_GROUNDED,
            origin: dereth_protocol::types::Origin {
                objcell_id: self.player.cell.0,
                origin: dereth_protocol::types::Vec3 {
                    x: at.x,
                    y: at.y,
                    z: at.z,
                },
            },
            position_timestamp: self.instance,
            // Make the teleport timestamp newer than the body's current timestamp.
            teleport_timestamp: self.instance,
            ..PositionPack::default()
        };
        let body = dereth_protocol::write_body(&MovementPositionEvent { id, position: pp })
            .expect("encode");
        let before = self.objects.physics.stats.teleport_arm;
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::MOVEMENT_POSITION_EVENT,
                body,
            },
            LocalTime(self.now),
        );
        assert_eq!(
            self.objects.stats.stale_positions, 0,
            "every `0xF748` here is newer"
        );
        self.sync_bodies();
        assert_eq!(
            self.objects.physics.stats.teleport_arm,
            before + 1,
            "premise: the movement update took the teleport arm"
        );
        assert_eq!(
            self.objects.physics.stats.no_contact_arm, 0,
            "and none of these updates was refused for `!has_contact`"
        );
        self.assert_at(id, offset);
    }

    fn phys(&self) -> SceneSelectionPhysics {
        SceneSelectionPhysics::new(Some(&self.player), &self.objects)
    }

    /// One manual bench frame: interaction processing followed, when enabled, by the local scene
    /// sync/update/stream/draw helper.
    ///
    /// Drawing writes the latch and the following interaction pass reads it, so a bench with no
    /// draw can only observe the clearing arm. The two steps are in this order rather than merged
    /// because that ordering is the whole reason the flag a range check reads was written by an
    /// *earlier* frame.
    fn frame(&mut self, events: Vec<dereth_client_runtime::actions::Action>) -> usize {
        self.now += 1.0;
        let (unowned, left) = dereth_client::interaction::use_time(
            &mut self.inter,
            &self.store,
            Some(&self.scene),
            &mut self.objects,
            None,
            events,
            false,
            (800, 600),
            LocalTime(self.now),
        );
        assert!(unowned.is_empty(), "no unowned requests were expected");
        if self.draw_frames {
            self.draw();
        }
        left.len()
    }

    /// The scene and rendering half, through `WorldScene::draw` on a software GPU device.
    ///
    /// These calls follow the corresponding `App::frame` order:
    /// `sync_objects` gives a created object its geometry, `update` is the scene half of world
    /// processing (which places the parts that drawing submits), `stream` fetches what the window gained, and
    /// the arena reservation is the device bookkeeping that must sit outside the frame bracket.
    fn draw(&mut self) {
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
            dereth_client::character::CharacterInput::default(),
            LocalTime(self.now),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
    }

    /// Whether the last frame's draw actually submitted a part of `id` — the premise every
    /// latch assertion in this file rests on, asserted rather than assumed.
    ///
    /// `WorldScene::drawn_part_order` is the submission trace `draw` publishes; a part
    /// in it is a part the draw path submitted.
    fn parts_drawn_for(&self, id: ObjectId) -> usize {
        self.scene
            .drawn_part_order()
            .iter()
            .filter(|e| e.object == Some(id))
            .count()
    }

    /// A manual bench frame with no input events. It still runs interaction processing and, when
    /// enabled, the scene helper.
    fn idle(&mut self) {
        assert_eq!(self.frame(Vec::new()), 0);
    }

    /// A manual bench frame with the pointer resting on the middle of the world view: the hover
    /// search the frame loop runs whenever no click or drop is in flight, then the frame.
    ///
    /// The hover is an object search like a click's, so it lowers the in-view latch exactly as a
    /// click does; the draw that follows puts it back up for whatever it drew.
    fn hover_frame(&mut self) {
        self.inter.dispatch_ui_hover(
            (400, 300),
            None,
            (800, 600),
            &mut self.objects.world,
            ServerTime(self.now),
        );
        self.idle();
    }

    fn press(&mut self, action: u32) {
        let e = dereth_client_runtime::actions::Action {
            id: ActionId(action),
            phase: dereth_client_runtime::actions::ActionPhase::Begin,
            extent: 1.0,
            repeats: 0,
        };
        assert_eq!(
            self.frame(vec![e]),
            0,
            "{action:#010X} must be consumed by `on_actions`"
        );
    }

    fn selected(&self) -> Option<ObjectId> {
        self.objects.world.selected
    }

    fn prev_selected(&self) -> Option<ObjectId> {
        self.objects.world.prev_selected
    }

    fn clears(&self) -> u64 {
        self.inter.stats.range_selection_clears
    }

    fn rearms(&self) -> u64 {
        self.inter.stats.range_selection_rearms
    }

    fn exits(&self) -> u64 {
        self.inter.stats.range_exits
    }
}

fn three_monsters() -> Bench {
    let mut b = Bench::new();
    b.place_monster(NEAR, NEAR_AT);
    b.place_monster(MID, MID_AT);
    b.place_monster(FAR, FAR_AT);
    b
}

// =================================================================================================
// 0. Calibration — the bench can select, can advance, and can end up with nothing
// =================================================================================================

/// **Both directions calibrate the instrument.** Every assertion below is either "the cycle moved
/// to X" or "the selection is empty"; the bench has to produce both before either is worth
/// anything.
#[test]
fn the_bench_can_select_can_advance_and_can_end_up_empty() {
    let mut b = three_monsters();
    assert_eq!(
        b.selected(),
        None,
        "nothing is selected before the first press"
    );

    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(b.selected(), Some(NEAR), "the closest of the three");

    b.press(ia::SELECTION_NEXT_MONSTER);
    assert_eq!(b.selected(), Some(MID), "and it advances");

    // And it can be emptied, which is the negative every measurement below rests on.
    let mut out = dereth_client_model::RecordingSink::default();
    b.objects.world.set_selected_object(None, false, &mut out);
    assert_eq!(b.selected(), None);
    assert_eq!(
        b.prev_selected(),
        Some(MID),
        "the previous-selection fallback retained the outgoing id"
    );
}

// =================================================================================================
// 1. Persistence over a SEQUENCE
// =================================================================================================

/// **Select, act, select, act, select — through `use_time`.**
///
/// The assertion is the *advance*, not the survival: a client that cleared the selection on the
/// examine would answer `FAR` on the next press through the empty-selection inversion, and
/// a client that froze would answer `NEAR` twice. Only a selection that survived each act in
/// between produces `NEAR`, `MID`, `FAR` in that order.
///
/// The acts are the ones a player performs without meaning to retarget: `SELECTION_EXAMINE`
/// (`0x1000002B`), `USE` (`0x10000025`), and idle frames, each of which runs
/// the object-range checks and the rest of player-system processing.
#[test]
fn the_cycle_advances_one_object_per_press_across_the_acts_between_them() {
    let mut b = three_monsters();

    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(b.selected(), Some(NEAR));

    // Act 1: examine the thing you just selected, then let two frames run.
    b.press(ia::SELECTION_EXAMINE);
    b.idle();
    b.idle();
    assert_eq!(b.selected(), Some(NEAR), "an examine must not retarget");

    b.press(ia::SELECTION_NEXT_MONSTER);
    assert_eq!(
        b.selected(),
        Some(MID),
        "press 2 advances outward from the surviving reference"
    );

    // Act 2: use it, and let more frames run.
    b.press(ia::USE);
    for _ in 0..3 {
        b.idle();
    }
    assert_eq!(b.selected(), Some(MID), "a use must not retarget either");

    b.press(ia::SELECTION_NEXT_MONSTER);
    assert_eq!(b.selected(), Some(FAR), "press 3 advances again");

    // The discrimination: had any act emptied the selection, this walk would have been
    // farthest-first from the start. It was not.
    assert_eq!(b.clears(), 0, "and no range exit dropped it along the way");
}

/// The other half of the same claim: the cycle **wraps**, and the wrap is the action handler's re-call
/// with `(!closer, true)` rather than an artefact of an empty selection.
///
/// Driven from the outermost object, where the outward walk has nowhere left to go, so the retry
/// is the only thing that can produce an answer.
#[test]
fn the_wrap_is_a_re_call_and_not_a_cleared_selection() {
    let mut b = three_monsters();
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    b.press(ia::SELECTION_NEXT_MONSTER);
    b.press(ia::SELECTION_NEXT_MONSTER);
    assert_eq!(b.selected(), Some(FAR), "at the outer end of the walk");

    b.press(ia::SELECTION_NEXT_MONSTER);
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "the outward walk found no next object and retried inward"
    );
    assert_eq!(b.clears(), 0, "nothing was emptied to make that happen");
}

/// **The draw-time selected-id comparison, driven on its own.**
///
/// ```text
/// if (draw state == inside view cone && selected id != 0)
///     if (selected id == drawn object's id)
///         mark selected object in view;
/// ```
///
/// The driven runs below all have the selected object drawn *among others*, so a setter that
/// ignored the id would pass every one of them. This drives the comparison directly, against a scene that is
/// drawing three objects the whole time.
///
/// **The `!= 0` half is deliberately not asserted as a discrimination**: no object carries id 0
/// (`s.object` is `None` for the local body, represented by no object id), so selected id zero
/// fails the id compare on its own. The client's outer test is a short-circuit, and deleting it
/// here changes no observable behaviour.
#[test]
fn the_draw_time_setter_matches_the_id_and_not_merely_the_fact_that_something_was_drawn() {
    let mut b = three_monsters();
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(b.selected(), Some(NEAR));
    b.idle();
    assert!(
        b.parts_drawn_for(MID) > 0 && b.parts_drawn_for(FAR) > 0,
        "premise: others draw too"
    );

    // The selection's own id: a draw raises the observation.
    b.scene.set_selected_object_id(Some(NEAR));
    b.draw();
    assert!(
        b.scene.take_selected_part_drawn(),
        "{NEAR:?}'s parts were submitted"
    );

    // An id no part carries, while the very same three objects are drawn.
    b.scene.set_selected_object_id(Some(ObjectId(0x8300_00FF)));
    b.draw();
    assert!(
        !b.scene.take_selected_part_drawn(),
        "three objects were drawn and none of them was the one named"
    );

    // And nothing selected at all.
    b.scene.set_selected_object_id(None);
    b.draw();
    assert!(
        !b.scene.take_selected_part_drawn(),
        "no selection produces no selected-part draw observation"
    );
}

/// **The latch clear, driven by a real world click.**
///
/// The latch's *only* clear, and the reason it is a latch and not a permanent truth: a world click
/// takes it down, and the next frame that draws the selected object puts it back up.
///
/// The test calls `wrapper_mouse` directly with the world-view button-down event; it does not drive
/// the full mouse producer. That wrapper arm reaches object lookup, and the manual frame is then
/// completed so the player-system drain runs. The clear lands after that frame's range check, as
/// the client's UI update follows its range checks, and before the next one.
#[test]
fn a_world_click_takes_the_latch_down_and_the_next_draw_puts_it_back_up() {
    let mut b = three_monsters();
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    b.idle();
    assert!(
        b.objects.world.selected_object_in_view,
        "up, because a frame drew {NEAR:?}"
    );

    let armed = b.inter.pick.stats.requests;
    b.inter.wrapper_mouse(
        UiMouseEvent {
            action: dereth_ui::focus::action::PRIMARY_CLICK,
            start: true,
            x: 400,
            y: 300,
            over: Some(dereth_ui_screens::screens::gameplay::window::SMART_BOX),
        },
        (800, 600),
        true,
    );
    assert_eq!(
        b.inter.pick.stats.requests,
        armed + 1,
        "premise: the click armed a pick"
    );

    // Frame step 8 drains the clear; this bench's step 11 then re-raises it, exactly as a retail
    // frame does — which is why the assertion below is taken on a frame with **no** draw.
    b.draw_frames = false;
    b.idle();
    assert!(
        !b.objects.world.selected_object_in_view,
        "the click lookup cleared the latch and nothing drew afterwards to put it back"
    );

    // And the re-latch, with the frame boundary shown rather than hidden: the draw is step 11 and
    // the fold is step 8, so the frame that draws leaves the observation pending and the frame
    // *after* it raises the latch. That is the client's order (drawing follows world
    // processing by three steps), not a lag introduced here.
    b.draw_frames = true;
    b.idle();
    assert!(
        !b.objects.world.selected_object_in_view,
        "the drawing frame's own step 8 ran BEFORE its step 11"
    );
    b.idle();
    assert!(
        b.objects.world.selected_object_in_view,
        "and the next frame's step 8 folds it in"
    );
}

// =================================================================================================
// 2. The selection-survival edge, measured over a driven run
// =================================================================================================

/// Behaviour: selection.range-watch.a-drawn-selection-survives-every-range-exit
///
/// **How often a range exit empties the selection, in a driven run.**
///
/// The player selects a monster and then walks it in and out of radar range six times. The drawn
/// arm asserts `exits >= walks`, `clears == 0` and `rearms == 6`, with the target still selected.
///
/// **Both arms drive the same six outward and return movements.** Arm (a) runs the scene helper, so
/// drawing raises the latch and every outward cycle re-arms. Arm (b) omits that helper and, because
/// each exit clears the selection, explicitly reselects after every return so the next cycle can
/// continue. The discriminator is the draw-produced latch rather than a flag set by hand. The
/// never-drawn condition is a **reachable retail state** — an object selected out of a panel and walked out of range before any frame carried its
/// parts has never been drawn there either — and every exit clears. A run in which the subject was
/// absent from both arms would report the same clears in both, so the two-arm discriminator is
/// required.
///
/// The premise is asserted rather than assumed: arm (a) checks that the draw really did submit
/// parts for the selected object, and arm (b) checks that it did not.
#[test]
fn a_driven_run_keeps_the_selection_across_every_range_exit_once_the_object_has_been_drawn() {
    // (a) The drawn arm, using the manual frame plus the scene helper.
    let mut b = three_monsters();
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(b.selected(), Some(NEAR));
    b.idle(); // arms the range-exit watch
    assert!(
        b.parts_drawn_for(NEAR) > 0,
        "premise: the scene helper submitted {NEAR:?}'s parts; without that the latch below would be a coincidence"
    );

    let mut walks = 0_u64;
    for _ in 0..6 {
        b.move_to(NEAR, OUT_OF_RANGE_AT);
        b.idle();
        b.idle();
        walks += 1;
        // Come back, as a player does. No re-selection: the point is that there is nothing to
        // re-select, because the selection was never emptied.
        b.move_to(NEAR, NEAR_AT);
        b.idle();
        assert_eq!(
            b.selected(),
            Some(NEAR),
            "walk {walks}: the target survived the round trip"
        );
        b.idle();
    }

    assert_eq!(
        b.clears(),
        0,
        "MEASURED: {walks} walks out of radar range, {} of them emptied the selection",
        b.clears()
    );
    assert_eq!(
        b.rearms(),
        walks,
        "they re-armed instead — {walks} walks, {} exits, {} re-arms",
        b.exits(),
        b.rearms()
    );
    assert!(
        b.exits() >= walks,
        "every re-arm came from a real range exit: {}",
        b.exits()
    );
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "and the target survived the whole run"
    );
    assert!(
        b.objects.world.selected_object_in_view,
        "the latch is up, and selected-object part submission is the only producer exercised here"
    );
    eprintln!(
        "selection_persistence driven run, drawing: walks={walks} exits={} clears={} rearms={}",
        b.exits(),
        b.clears(),
        b.rearms()
    );

    // (b) The control: the same movement cycle with the scene helper removed, so the object is
    // never drawn. It reselects after each clear to start the next cycle.
    let mut c = three_monsters();
    c.draw_frames = false;
    c.press(ia::SELECTION_CLOSEST_MONSTER);
    c.idle();
    assert_eq!(
        c.parts_drawn_for(NEAR),
        0,
        "premise: nothing was drawn in this arm at all"
    );
    for _ in 0..6 {
        c.move_to(NEAR, OUT_OF_RANGE_AT);
        c.idle();
        c.idle();
        c.move_to(NEAR, NEAR_AT);
        c.idle();
        c.press(ia::SELECTION_CLOSEST_MONSTER);
        c.idle();
    }
    assert_eq!(c.rearms(), 0, "with the latch down, nothing ever re-armed");
    assert_eq!(
        c.clears(),
        walks,
        "every exit dropped the selection instead"
    );
    assert!(
        !c.objects.world.selected_object_in_view,
        "and the latch stayed down, because no frame drew"
    );
    eprintln!(
        "selection_persistence driven run, never drawn: walks={walks} exits={} clears={} rearms={}",
        c.exits(),
        c.clears(),
        c.rearms()
    );
}

/// Behaviour: selection.range-watch.a-drawn-far-selection-survives-the-pointer-resting-on-the-world
///
/// **A far selection, with the pointer resting on the world view.**
///
/// While no click or drop is in flight, every frame runs an object search at the pointer, the
/// hover search, and that search lowers the in-view latch exactly as a click does. The client
/// polls the ranges at the start of its frame, before that search, and draws after it, so the
/// latch a range exit reads is always the one the previous frame's draw raised. A selection 90 m
/// away that every frame draws is therefore kept at every exit however long the pointer rests
/// there.
///
/// The control is the same run with the selection 90 m **behind** the player, where no frame
/// draws it: the hover search takes the latch down, nothing puts it back, and the first exit
/// empties the selection. Both arms hover and draw every frame; only whether the selected object
/// is drawn differs.
#[test]
fn a_drawn_far_selection_is_kept_while_the_pointer_rests_on_the_world_view() {
    // (a) In front of the player, so every frame draws it.
    let mut b = three_monsters();
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(b.selected(), Some(NEAR));
    b.idle(); // arms the range-exit watch
    b.move_to(NEAR, OUT_OF_RANGE_AT);
    let searches = b.inter.pick.stats.requests;
    for _ in 0..12 {
        b.hover_frame();
    }
    assert_eq!(
        b.inter.pick.stats.requests,
        searches + 12,
        "premise: every frame's hover started an object search, so every frame lowered the latch"
    );
    assert!(
        b.parts_drawn_for(NEAR) > 0,
        "premise: the draw submitted {NEAR:?}'s parts at 90 m"
    );
    assert!(
        b.exits() > 0,
        "premise: the selection's watch fired at 90 m"
    );
    assert_eq!(
        b.clears(),
        0,
        "MEASURED: {} range exits with the pointer resting on the world view, {} of them emptied \
         the selection",
        b.exits(),
        b.clears()
    );
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "and the far target is still selected"
    );
    assert!(
        b.exits() >= 5,
        "the watch kept firing at every poll, {} exits",
        b.exits()
    );
    assert_eq!(b.rearms(), b.exits(), "and every exit re-armed it");

    // (b) The control: 90 m behind the player, never drawn there.
    let mut c = three_monsters();
    c.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(c.selected(), Some(NEAR));
    c.idle();
    c.move_to(NEAR, OUT_OF_RANGE_BEHIND_AT);
    for _ in 0..12 {
        c.hover_frame();
    }
    assert_eq!(
        c.parts_drawn_for(NEAR),
        0,
        "premise: nothing of {NEAR:?} was drawn behind the player"
    );
    assert_eq!(c.selected(), None, "the undrawn far target was dropped");
    assert_eq!(c.clears(), 1, "by a range exit");
}

/// The state an emptied selection leaves behind, asserted rather than assumed — because it is what
/// decides whether the drop is *also* a direction flip.
///
/// After a range exit that clears, the world has `selected == None` and `prev_selected ==
/// Some(the dropped object)`. Next-selection then falls back to the previous selected id, finds
/// the object still present, and walks **outward from its current distance** — so the
/// next press selects the object beyond it and not the farthest one in range.
///
/// **The drop is rare, not unreachable.** It happens for a selection whose object no frame has
/// drawn, which is what `draw_frames = false` is, so the state below is worth pinning.
#[test]
fn after_a_drop_the_cycle_still_continues_from_the_dropped_object() {
    let mut b = three_monsters();
    b.draw_frames = false;
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    assert_eq!(b.selected(), Some(NEAR));
    b.idle();

    // Walk it out and leave it there, so the reference is a live object at 90 m.
    b.move_to(NEAR, OUT_OF_RANGE_AT);
    b.idle();
    b.idle();
    assert_eq!(b.selected(), None, "dropped");
    assert_eq!(b.prev_selected(), Some(NEAR), "and retained");
    assert_eq!(b.clears(), 1);

    // Bring it back to 3 m without re-selecting it: the reference is now the nearest object.
    b.move_to(NEAR, NEAR_AT);
    b.idle();
    b.press(ia::SELECTION_NEXT_MONSTER);
    assert_eq!(
        b.selected(),
        Some(MID),
        "outward from the dropped object's CURRENT position, not farthest-first"
    );
    assert_ne!(
        b.selected(),
        Some(FAR),
        "the empty-selection farthest-first inversion did not fire"
    );
}

/// The radius the watch is armed at is the radar's own, so the distance a target's watch fires at
/// is the distance its blip leaves the radar — which is why a player notices either outcome.
///
/// The 74.8 m and 75.2 m stations bracket `RADAR_RADIUS_OUTDOORS` on the 3/4/5 heading; the
/// exactness of those two distances is asserted as a premise so the answer cannot be a placement error, and
/// **both arms of `is_selected_object_in_view` driven across the same boundary** — a drawn object
/// re-arms there, an undrawn one is dropped there. Testing only one arm would leave the boundary
/// measured for one outcome and assumed for the other.
#[test]
fn the_range_edge_is_the_radar_radius_on_both_arms_and_not_before_it() {
    // (45, 60) is the 75 m reference; these two points sit 0.2 m to either side.
    let inside = (44.88_f32, 59.84, 0.0);
    let outside = (45.12_f32, 60.16, 0.0);
    assert!(
        (math::hypotf(inside.0, inside.1) - 74.8).abs() < 1e-3
            && (math::hypotf(outside.0, outside.1) - 75.2).abs() < 1e-3,
        "premise: the two stations bracket {RADAR_RADIUS_OUTDOORS} by 0.2 m"
    );

    // (a) Drawn: the exit re-arms, and the selection survives it.
    let mut b = three_monsters();
    b.press(ia::SELECTION_CLOSEST_MONSTER);
    b.idle();
    assert!(
        b.parts_drawn_for(NEAR) > 0,
        "premise: the latch was raised by a real draw"
    );

    b.move_to(NEAR, inside);
    b.idle();
    b.idle();
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "inside 75 m the selection is kept"
    );
    assert_eq!(
        (b.clears(), b.rearms()),
        (0, 0),
        "and no edge has fired at all yet"
    );

    b.move_to(NEAR, outside);
    b.idle();
    b.idle();
    assert_eq!(
        b.selected(),
        Some(NEAR),
        "outside it the watch re-arms and the target is kept"
    );
    assert_eq!(
        (b.clears(), b.rearms()),
        (0, 1),
        "exactly one edge, and it took the re-arm arm"
    );

    // (b) Never drawn: the same boundary, the other arm.
    let mut c = three_monsters();
    c.draw_frames = false;
    c.press(ia::SELECTION_CLOSEST_MONSTER);
    c.idle();

    c.move_to(NEAR, inside);
    c.idle();
    c.idle();
    assert_eq!(
        c.selected(),
        Some(NEAR),
        "inside 75 m the selection is kept on this arm too"
    );
    assert_eq!(c.clears(), 0);

    c.move_to(NEAR, outside);
    c.idle();
    c.idle();
    assert_eq!(
        c.selected(),
        None,
        "outside it, and never drawn, it is dropped"
    );
    assert_eq!((c.clears(), c.rearms()), (1, 0));
}
