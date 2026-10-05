//! A projectile's flight ends at its impact. Reaching the ground or a solid object clears
//! `MISSILE_PS | ALIGNPATH_PS | PATHCLIPPED_PS` (the `0xFFFFFCBF` mask) and `INELASTIC_PS` stops
//! the body; the impact runs the projectile's own default script only when it carries
//! `SCRIPTED_COLLISION_PS` (`0x8000`); the client does not remove the object, which is the
//! server's (`0xF747 Item_DeleteObject`). A missile ignores a non-target ethereal weenie, a
//! player step-down does not stand on a creature, and a player pair passes through each other
//! unless both are PK, both PKLite, or either is impenetrable.
//! Fixture: long-solo-play's remote-creature create (`0x800009D2`) re-sent as encoded `0xF745`
//! creates into a `WorldScene` on a software device, the retail dats, and a census over six
//! recordings' velocity-carrying creates.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_assets::{Decode, Setup};
use dereth_physics::math::V3 as _;
use dereth_physics::{PhysicsState, Transition};
use dereth_scene::world_scene::SceneWrites;

use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemSetState, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::{PhysicsDesc, PositionWire, PublicWeenieDesc};
use dereth_protocol::{Message, Opcode};
use {dereth_client_runtime::character::Character, dereth_world_data::setup::setup_geometry};
use {
    dereth_client_runtime::landblock::load_region,
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};

/// The bolt.
const BOLT: ObjectId = ObjectId(0x8000_0F50);
const BYSTANDER: ObjectId = ObjectId(0x8000_0F51);
const PLAYER_MOVER: ObjectId = ObjectId(0x8000_0F52);
const PLAYER_OBSTACLE: ObjectId = ObjectId(0x8000_0F53);
const STEP_DOWN_CANDIDATE: ObjectId = ObjectId(0x8000_0F54);

/// The exact pair retail's door qualities carry, differing only by `ETHEREAL_PS`: the solid
/// `0x08` is on 38 of 542 doors; adding `0x04` is its open state.
const SOLID_WEENIE_STATE: u32 = 0x0000_0008;
const ETHEREAL_WEENIE_STATE: u32 = 0x0000_000C;

/// 15 m/s.
const SPEED: f32 = 15.0;

/// **A spell bolt's real `PhysicsState`, taken from the shard rather than invented.**
///
/// `0x28B48` is a word the recorded velocity-carrying creates carry — see
/// [`every_recorded_velocity_create_reports_collisions_and_its_missiles_align_to_their_path`],
/// which asserts it is one of theirs. Its bits:
///
/// ```text
/// 0x00020000  INELASTIC_PS              velocity is zeroed on contact, not reflected
/// 0x00008000  SCRIPTED_COLLISION_PS     enables the impact-effect callback
/// 0x00000800  LIGHTING_ON_PS            a bolt is lit
/// 0x00000200  PATHCLIPPED_PS  \
/// 0x00000100  ALIGNPATH_PS     >  the three MISSILE_CLEAR_MASK takes away
/// 0x00000040  MISSILE_PS      /
/// 0x00000008  REPORT_COLLISIONS_PS      without it the contact still blocks, silently
/// ```
///
/// **No `GRAVITY_PS`** — a spell bolt flies straight, which is what makes the control below a
/// control rather than a race against a fall. The other missile word in the corpus, `0x20748`,
/// is the same thing *with* gravity and without the light and the script: an arrow.
const BOLT_STATE: u32 = 0x0002_8B48;

fn recorded_create() -> ObjectCreatePayload {
    let corpus = Corpus::shared("long-solo-play");
    let row = corpus
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && r.payload.get(4..8) == Some(&0x8000_09D2_u32.to_le_bytes())
        })
        .expect("long-solo-play carries the remote mover's create");
    ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .expect("decodes")
        .0
}

fn decoded_setup(store: &RetailDatStore, id: DataId) -> Setup {
    let bytes = store
        .read_typed(DbType::Setup, id)
        .expect("the captured setup exists");
    Setup::decode_payload(id, &bytes).expect("the captured setup decodes")
}

/// One production step-down probe by the real local-player body, starting 5 cm above
/// the crown of a candidate's real setup sphere. The candidate is accepted from an encoded
/// `0xF745`; only its PWD item type differs between the two cases.
fn step_down_over_encoded_candidate(obj_type: u32) -> (bool, f32) {
    let store = std::sync::Arc::new(
        dereth_dat::testing::open_store().expect("DERETH_TEST_DAT_DIR required"),
    );
    let region = load_region(&store).expect("the region decodes");
    let mut character = Character::new(&store, &region, DEFAULT_LANDBLOCK, (96.0, 96.0))
        .expect("the ordinary local body is created");
    let block = character.position().cell.landblock();
    character.land().load_block_cells(block);

    let recorded = recorded_create();
    let setup_id = DataId(
        recorded
            .physicsdesc
            .setup_id
            .expect("the capture names a setup"),
    );
    let setup = decoded_setup(&store, setup_id);
    let candidate_geometry = setup_geometry(&setup);
    let candidate_sphere = candidate_geometry
        .path_spheres()
        .iter()
        .max_by(|a, b| (a.center.z + a.radius).total_cmp(&(b.center.z + b.radius)))
        .copied()
        .expect("the recorded creature setup has a path sphere");
    let (mover_sphere, mover_scale) = {
        let body = character
            .world
            .get(character.handle)
            .expect("the player body is live");
        (
            *body
                .geometry
                .path_spheres()
                .first()
                .expect("the player has a path sphere"),
            body.scale,
        )
    };

    let mut crown = character.position();
    crown.frame.origin.z += 5.0;
    let mut candidate_at = crown;
    candidate_at.frame.origin = crown.frame.origin.sub(candidate_sphere.center);
    candidate_at.frame.rotation = Quat::IDENTITY;
    let rest_origin_z =
        crown.frame.origin.z + candidate_sphere.radius + mover_sphere.radius * mover_scale
            - mover_sphere.center.z * mover_scale;
    let mut start = crown;
    start.frame.origin.z = rest_origin_z + 0.05;

    // Place the mover before the candidate exists. That keeps teleport placement from performing
    // the very step-down this station is meant to observe in the controlled probe below.
    character.teleport(start);
    assert_eq!(
        character.stats.teleports_committed, 1,
        "the open-air placement was refused"
    );
    let before = character.position().frame.origin.z;
    assert!(
        (before - start.frame.origin.z).abs() < 1.0e-4,
        "the open-air start did not commit"
    );

    let mut candidate = recorded;
    candidate.id = STEP_DOWN_CANDIDATE;
    candidate.wdesc.obj_type = obj_type;
    candidate.wdesc.bitfield = 0;
    candidate.physicsdesc = PhysicsDesc {
        bitfield: flags::POSITION | flags::SETUP,
        setup_id: Some(setup_id.0),
        position: Some(PositionWire {
            objcell_id: candidate_at.cell.raw(),
            frame: dereth_protocol::types::Frame {
                origin: candidate_at.frame.origin.into(),
                orientation: Quat::IDENTITY.into(),
            },
        }),
        state: PhysicsState::REPORT_COLLISIONS_PS,
        ..Default::default()
    };
    let encoded = dereth_protocol::write_body(&ItemCreateObject(candidate)).expect("F745 encodes");
    let decoded = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&encoded))
        .expect("the encoded candidate round trips")
        .0;
    assert_eq!(
        decoded.wdesc.obj_type, obj_type,
        "the PWD type was not encoded"
    );

    let mut stream = ObjectStream::new();
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body: encoded,
        },
        LocalTime(0.0),
    );
    stream.sync_physics(&store, &mut character.world);
    let candidate_body = character
        .world
        .by_object_id(STEP_DOWN_CANDIDATE)
        .and_then(|h| character.world.get(h))
        .expect("the encoded candidate has a positioned body");
    assert!(
        candidate_body.cell.is_some(),
        "the candidate was not inserted in the cell graph"
    );
    assert_eq!(
        candidate_body
            .weenie
            .as_ref()
            .expect("the candidate has a weenie seam")
            .is_creature,
        obj_type == dereth_client_model::weenie::item_type::CREATURE,
        "the accepted PWD type did not reach candidate IsCreature"
    );
    assert_eq!(candidate_body.geometry.spheres, candidate_geometry.spheres);

    let (spheres, scale, step_down_height) = {
        let body = character
            .world
            .get(character.handle)
            .expect("the player body remains live");
        (
            body.geometry.path_spheres().to_vec(),
            body.scale,
            body.step_down_height(),
        )
    };
    let mut transition = Transition::default();
    transition.init();
    transition.init_object(
        character
            .world
            .get(character.handle)
            .expect("the player body remains live"),
        character.handle,
        0,
    );
    assert!(
        !transition
            .object_info
            .has(dereth_physics::transition::ObjectInfoState::IS_VIEWER)
            && !transition
                .object_info
                .has(dereth_physics::transition::ObjectInfoState::IGNORE_CREATURES),
        "the native zero placement seed must not import unrelated PhysicsState bits"
    );
    transition.sphere_path.init_sphere(&spheres, scale);
    transition
        .sphere_path
        .init_path(Some(start.cell), Some(start), &start);
    transition
        .sphere_path
        .set_check_pos(&start, Some(start.cell));
    transition.sphere_path.check_cell = Some(start.cell);
    let ctx = character.world.transition_ctx(Some(character.handle));
    let supported = dereth_physics::transition::walk::step_down(
        &ctx,
        &mut transition,
        step_down_height,
        dereth_physics::globals::FLOOR_Z,
    );
    (
        supported,
        transition.sphere_path.check_pos.frame.origin.z - before,
    )
}

/// A projectile's descriptor: a setup, a position, a velocity, the missile state word — and **no
/// motion table**, which is what a spell bolt is.
fn bolt_create(at: Position, velocity: Vec3, state: u32) -> ObjectCreatePayload {
    let recorded = recorded_create();
    ObjectCreatePayload {
        id: BOLT,
        objdesc: recorded.objdesc,
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP | flags::VELOCITY,
            setup_id: recorded.physicsdesc.setup_id,
            position: Some(PositionWire {
                objcell_id: at.cell.raw(),
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: Quat::IDENTITY.into(),
                },
            }),
            velocity: Some(velocity.into()),
            state,
            ..Default::default()
        },
        wdesc: PublicWeenieDesc::default(),
    }
}

/// The recording's real remote-creature description and setup, placed on the bolt's flight line.
/// Its initial and updated state words are retail door-quality words; making the geometry
/// controlled does not bypass either the accepted `PublicWeenieDesc` producer or the
/// physics-state producer.
fn bystander_create(at: Position) -> ObjectCreatePayload {
    let recorded = recorded_create();
    ObjectCreatePayload {
        id: BYSTANDER,
        objdesc: recorded.objdesc,
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            setup_id: recorded.physicsdesc.setup_id,
            position: Some(PositionWire {
                objcell_id: at.cell.raw(),
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: at.frame.rotation.into(),
                },
            }),
            state: SOLID_WEENIE_STATE,
            ..Default::default()
        },
        wdesc: recorded.wdesc,
    }
}

fn feed(stream: &mut ObjectStream, p: ObjectCreatePayload, now: f64) {
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body: dereth_protocol::write_body(&ItemCreateObject(p)).expect("encodes"),
        },
        LocalTime(now),
    );
}

/// One flight, run to `frames` at 30 Hz. Answers the bolt's live `PhysicsState` word — the body's,
/// not the descriptor's — and how far it got.
struct Flight {
    state: PhysicsState,
    travelled: Vec3,
    present: bool,
    /// How far it moved over the **last ten** of those frames. A flight that *ends* is a position
    /// that stops changing across simulated time, and only a clock can tell that apart from one
    /// still flying.
    travelled_last_third: Vec3,
    /// Impacts that reached the game-object callback with `SCRIPTED_COLLISION_PS` set and thus
    /// attempted default-script playback. A missing setup script table can prevent playback
    /// after that call, so this counts played + unplayed rather than successful playback alone.
    impact_scripts: u64,
}

fn fly(velocity: Vec3, frames: usize) -> Flight {
    fly_with_state(velocity, frames, BOLT_STATE)
}

fn fly_with_state(velocity: Vec3, frames: usize, bolt_state: u32) -> Flight {
    fly_with_bystander(velocity, frames, bolt_state, None)
}

fn fly_with_bystander(
    velocity: Vec3,
    frames: usize,
    bolt_state: u32,
    ethereal_bystander: Option<bool>,
) -> Flight {
    let store = std::sync::Arc::new(
        dereth_dat::testing::open_store().expect("DERETH_TEST_DAT_DIR required"),
    );
    let mut gpu = crate::common::test_gpu(320, 240);

    let recorded = recorded_create();
    let mut at: Position = recorded.physicsdesc.position.expect("placed").into();
    // Five metres up, so the bolt starts in air: this is the test declining an *unrelated*
    // collision, not the client declining one. The collision this suite is about is the one the
    // velocity aims at.
    at.frame.origin.z += 5.0;
    let block = at.cell.landblock();

    let mut scene = WorldScene::load(
        &store,
        &mut gpu,
        SceneConfig {
            landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
            character: false,
            land_radius: 1,
            scenery_radius: 0,
            cell_statics: false,
            mesh_collision: false,
            particles: false,
            ..SceneConfig::default()
        },
    )
    .expect("the recorded block loads");
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("a physics owner");
    {
        // **Off the flight line, and that is not cosmetic.** An observer three metres along the
        // +x the bolt is fired down would be reached in six frames and end the flight there,
        // making the open-air control fail on the wrong geometry.
        //
        // The missile filter's non-target ethereal game-object arm is covered below. Its
        // nonzero-target/creature arm is dormant: initialization clears the target id and the
        // client has no nonzero writer, so this fixture does not invent that branch to pass
        // through the observer.
        let mut observer = at;
        observer.frame.origin.y += 8.0;
        scene
            .character
            .as_ref()
            .expect("attached")
            .land()
            .load_block_cells(block);
        scene
            .character
            .as_mut()
            .expect("attached")
            .teleport(observer);
    }

    let mut stream = ObjectStream::new();
    let mut now = 1000.0_f64;
    feed(&mut stream, bolt_create(at, velocity, bolt_state), now);
    let mut bystander = None;
    if let Some(make_ethereal) = ethereal_bystander {
        let mut position = at;
        position.frame.origin.x += 3.0;
        bystander = Some(make_ethereal);
        feed(&mut stream, bystander_create(position), now);
    }
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the create reaches the scene");
    if let Some(c) = scene.character.as_mut() {
        stream.sync_physics(&store, &mut c.world);
    }
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the drain after the body exists");

    if let Some(make_ethereal) = bystander {
        if make_ethereal {
            let state = ItemSetState {
                id: BYSTANDER,
                state: ETHEREAL_WEENIE_STATE,
                timestamps: dereth_protocol::types::PhysicsEventStamp {
                    instance: 0,
                    event: 1,
                },
            };
            stream.apply_event(
                &SessionEvent::WorldObject {
                    opcode: ItemSetState::OPCODE,
                    body: dereth_protocol::write_body(&state).expect("the state encodes"),
                },
                LocalTime(now),
            );
            scene
                .sync_objects(&store, &mut gpu, &mut stream)
                .expect("the state reaches the scene");
            if let Some(c) = scene.character.as_mut() {
                stream.sync_physics(&store, &mut c.world);
            }
        }
        let c = scene.character.as_ref().expect("attached");
        let h = c
            .world
            .by_object_id(BYSTANDER)
            .expect("the recorded bystander has a physics body");
        let body = c.world.get(h).expect("the recorded bystander body is live");
        assert_eq!(
            body.state.is_ethereal(),
            make_ethereal,
            "the station must differ only by delivery of the recorded ethereal state"
        );
        assert!(
            !body.state.ignores_collisions(),
            "the mutual ETHEREAL|IGNORE_COLLISIONS exemption would test the wrong clause"
        );
        assert!(
            body.weenie.is_some(),
            "the accepted PublicWeenieDesc must reach the candidate body's weenie_obj seam"
        );
    }

    let start = scene
        .server_object_position(BOLT)
        .expect("the bolt is in the scene");
    let born = body_state(&scene).expect("the bolt has a physics body");
    assert_eq!(
        born.0, bolt_state,
        "the fixture itself is wrong if the descriptor's state word did not reach the body"
    );

    let mut ten_from_the_end = start;
    for i in 0..frames {
        if i + 10 == frames {
            ten_from_the_end = scene
                .server_object_position(BOLT)
                .expect("the bolt is still in the scene");
        }
        now += 1.0 / 30.0;
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
    }

    let end = scene.server_object_position(BOLT);
    Flight {
        state: body_state(&scene).unwrap_or(PhysicsState(0)),
        travelled: end.map_or(Vec3::ZERO, |e| e.frame.origin.sub(start.frame.origin)),
        travelled_last_third: end.map_or(Vec3::ZERO, |e| {
            e.frame.origin.sub(ten_from_the_end.frame.origin)
        }),
        present: end.is_some(),
        impact_scripts: scene.draw.stats.collision_scripts_played
            + scene.draw.stats.collision_scripts_unplayed,
    }
}

fn body_state(scene: &WorldScene) -> Option<PhysicsState> {
    let c = scene.character.as_ref()?;
    let h = c.world.by_object_id(BOLT)?;
    Some(c.world.get(h)?.state)
}

/// One ordinary player-player object sweep. The recording supplies the creature setup and
/// descriptor shape; only the controlled position, velocity, physics state and four PWD flags
/// used by collision-query initialization differ between cases.
fn move_player_pair(mover_flags: u32, obstacle_flags: u32) -> (Vec3, Vec3) {
    let store = std::sync::Arc::new(
        dereth_dat::testing::open_store().expect("DERETH_TEST_DAT_DIR required"),
    );
    let mut gpu = crate::common::test_gpu(320, 240);

    let recorded = recorded_create();
    let mut at: Position = recorded.physicsdesc.position.expect("placed").into();
    at.frame.origin.z += 5.0;
    let block = at.cell.landblock();
    let mut scene = WorldScene::load(
        &store,
        &mut gpu,
        SceneConfig {
            landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
            character: false,
            land_radius: 1,
            scenery_radius: 0,
            cell_statics: false,
            mesh_collision: false,
            particles: false,
            ..SceneConfig::default()
        },
    )
    .expect("the recorded block loads");
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("a physics owner");
    scene
        .character
        .as_ref()
        .expect("attached")
        .land()
        .load_block_cells(block);
    let mut observer = at;
    observer.frame.origin.y += 8.0;
    scene
        .character
        .as_mut()
        .expect("attached")
        .teleport(observer);

    let make = |id, position: Position, velocity: Option<Vec3>, pwd_flags| {
        let mut wdesc = recorded.wdesc.clone();
        wdesc.bitfield = pwd_flags;
        ObjectCreatePayload {
            id,
            objdesc: recorded.objdesc.clone(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION
                    | flags::SETUP
                    | if velocity.is_some() {
                        flags::VELOCITY
                    } else {
                        0
                    },
                setup_id: recorded.physicsdesc.setup_id,
                position: Some(PositionWire {
                    objcell_id: position.cell.raw(),
                    frame: dereth_protocol::types::Frame {
                        origin: position.frame.origin.into(),
                        orientation: Quat::IDENTITY.into(),
                    },
                }),
                velocity: velocity.map(Into::into),
                state: if velocity.is_some() {
                    PhysicsState::INELASTIC_PS | PhysicsState::REPORT_COLLISIONS_PS
                } else {
                    PhysicsState::REPORT_COLLISIONS_PS
                },
                ..Default::default()
            },
            wdesc,
        }
    };

    let mut obstacle_at = at;
    obstacle_at.frame.origin.x += 3.0;
    let mut stream = ObjectStream::new();
    let mut now = 2000.0_f64;
    feed(
        &mut stream,
        make(
            PLAYER_MOVER,
            at,
            Some(Vec3::new(SPEED, 0.0, 0.0)),
            mover_flags,
        ),
        now,
    );
    feed(
        &mut stream,
        make(PLAYER_OBSTACLE, obstacle_at, None, obstacle_flags),
        now,
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the creates reach the scene");
    if let Some(c) = scene.character.as_mut() {
        stream.sync_physics(&store, &mut c.world);
    }
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the bodies reach the scene");

    {
        let c = scene.character.as_ref().expect("attached");
        for (id, expected) in [
            (PLAYER_MOVER, mover_flags),
            (PLAYER_OBSTACLE, obstacle_flags),
        ] {
            let handle = c
                .world
                .by_object_id(id)
                .expect("the player has a physics body");
            let got = c
                .world
                .get(handle)
                .expect("the body is live")
                .weenie
                .as_ref()
                .expect("the accepted PublicWeenieDesc must reach the body's weenie restrictions");
            assert!(
                got.is_player,
                "the 0xF745 PWD PLAYER bit must reach the physics body"
            );
            assert_eq!(
                got.is_pk,
                expected & dereth_client_model::weenie::bitfield::PLAYER_KILLER != 0,
                "the 0xF745 PWD PLAYER_KILLER bit must reach the physics body"
            );
            assert_eq!(
                got.is_pk_lite,
                expected & dereth_client_model::weenie::bitfield::PK_LITE != 0,
                "the 0xF745 PWD PK_LITE bit must reach the physics body"
            );
            assert_eq!(
                got.is_impenetrable,
                expected & dereth_client_model::weenie::bitfield::IMPENETRABLE != 0,
                "the 0xF745 PWD IMPENETRABLE bit must reach the physics body"
            );
        }
    }

    let start = scene
        .server_object_position(PLAYER_MOVER)
        .expect("mover is visible");
    for _ in 0..10 {
        now += 1.0 / 30.0;
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
    }
    let end = scene
        .server_object_position(PLAYER_MOVER)
        .expect("mover remains visible");
    let velocity = {
        let c = scene.character.as_ref().expect("attached");
        let handle = c
            .world
            .by_object_id(PLAYER_MOVER)
            .expect("mover body remains live");
        c.world
            .get(handle)
            .expect("mover body remains live")
            .velocity_vector
    };
    (end.frame.origin.sub(start.frame.origin), velocity)
}

/// The object sweep's player-pair exemption: ordinary NPK
/// players pass through one another. Matching PK, matching PKLite, or an impenetrable body turns
/// the exemption off and retains ordinary collision handling. Each case starts at the same
/// captured creature setup and enters through an encoded `0xF745` descriptor; this is not a
/// helper-only collision-query state injection.
#[test]
fn player_pair_collision_uses_the_pwd_pk_and_impenetrable_qualities() {
    use dereth_client_model::weenie::bitfield as pwd;

    let npk = move_player_pair(pwd::PLAYER, pwd::PLAYER);
    let pk = move_player_pair(
        pwd::PLAYER | pwd::PLAYER_KILLER,
        pwd::PLAYER | pwd::PLAYER_KILLER,
    );
    let mixed_pk = move_player_pair(pwd::PLAYER | pwd::PLAYER_KILLER, pwd::PLAYER);
    let pk_lite = move_player_pair(pwd::PLAYER | pwd::PK_LITE, pwd::PLAYER | pwd::PK_LITE);
    let mover_impenetrable = move_player_pair(pwd::PLAYER | pwd::IMPENETRABLE, pwd::PLAYER);
    let obstacle_impenetrable = move_player_pair(pwd::PLAYER, pwd::PLAYER | pwd::IMPENETRABLE);

    assert!(
        npk.0.x > pk.0.x + 1.0,
        "ordinary NPK players did not take retail's pass-through arm: npk={:?}, pk={:?}",
        npk.0,
        pk.0
    );
    assert!(
        npk.1.x > 1.0,
        "the pass-through mover was stopped: {:?}",
        npk
    );
    assert!(
        mixed_pk.0.x > pk.0.x + 1.0 && mixed_pk.1.x > 1.0,
        "PK blocks only when both players carry the status: mixed={mixed_pk:?}, matched={pk:?}"
    );
    for (name, pair) in [
        ("matching PK", pk),
        ("matching PKLite", pk_lite),
        ("impenetrable mover", mover_impenetrable),
        ("impenetrable obstacle", obstacle_impenetrable),
    ] {
        assert!(
            pair.0.x < npk.0.x - 1.0,
            "{name} incorrectly passed through: npk={:?}, case={:?}",
            npk.0,
            pair.0
        );
        assert!(
            pair.1.mag2() < 0.01,
            "{name} did not retain inelastic collision: {pair:?}"
        );
    }
}

// -------------------------------------------------------------------------------------------
// The acceptance half: a flight that ends, and a control proving the instrument can say so.
// -------------------------------------------------------------------------------------------

/// A bolt fired level through open air is still a missile a third of a second
/// later. Without this the test below could pass because the instrument reads zero for everything.
#[test]
fn a_bolt_still_in_flight_is_still_a_missile() {
    let f = fly(Vec3::new(SPEED, 0.0, 0.0), 10);
    assert!(
        f.travelled.mag2().sqrt() > 1.0,
        "the control is only a control if the bolt is genuinely still flying: {:?}",
        f.travelled
    );
    assert_eq!(
        f.state.0, BOLT_STATE,
        "nothing has been hit, so object-collision handling has not applied the 0xFFFFFCBF mask and the state word is untouched -- all of it, not \
         just MISSILE_PS"
    );
    assert_eq!(
        f.impact_scripts, 0,
        "and the effect instrument reads zero when there has been no impact, which is the half \
         of a control that a `>= 1` assertion elsewhere depends on"
    );
    assert!(
        f.travelled_last_third.mag2().sqrt() > 1.0,
        "and the *stopped* instrument reads a big number for a bolt that is still going -- \
         without this, the impact test's `< 0.01 m` could be a body that never moved at all: {:?}",
        f.travelled_last_third
    );
}

/// Object-collision lookup derives a sphere-intersection flag from the candidate's
/// `MISSILE_PS` bit or its game-object creature predicate. During a step-down, that flag makes
/// sphere intersection return `OK_TS`: another
/// creature is not walkable support. Both candidates below use the same captured setup and state;
/// only the accepted `PublicWeenieDesc.obj_type` differs.
#[test]
fn a_player_step_down_does_not_treat_an_encoded_creature_as_walkable_support() {
    let solid = step_down_over_encoded_candidate(0x0000_0080);
    let creature =
        step_down_over_encoded_candidate(dereth_client_model::weenie::item_type::CREATURE);

    assert!(
        solid.0,
        "the identical solid non-creature was not accepted as support: {solid:?}"
    );
    assert!(
        solid.1 < 0.0,
        "the step-down did not lower the player onto the solid: {solid:?}"
    );
    assert!(
        !creature.0,
        "the ordinary mover incorrectly treated the encoded creature as walkable support: \
         {creature:?}"
    );
}

/// The missile filter's first reachable game-object arm: a missile ignores a
/// non-target candidate only when that candidate is both ethereal and backed by a weenie object.
/// Here both candidates are the same captured remote-creature descriptor and setup delivered by
/// `0xF745`; the ethereal arm additionally receives `0xF74B`, while the control remains in the
/// solid retail state word.
#[test]
fn a_bolt_ignores_an_ethereal_weenie_but_hits_the_same_body_while_solid() {
    let solid = fly_with_bystander(Vec3::new(SPEED, 0.0, 0.0), 10, BOLT_STATE, Some(false));
    assert!(
        !solid.state.is_missile(),
        "the solid same-body control did not collide, so the pass-through arm has no rejecting \
         boundary: travelled {:?}",
        solid.travelled
    );

    let ethereal = fly_with_bystander(Vec3::new(SPEED, 0.0, 0.0), 10, BOLT_STATE, Some(true));
    assert_eq!(
        ethereal.state.0, BOLT_STATE,
        "the bolt hit the non-target ethereal weenie instead of `missile_ignore` skipping its \
         geometry: travelled {:?}",
        ethereal.travelled
    );
    assert!(
        ethereal.travelled.x > solid.travelled.x + 1.0,
        "the state word must agree with a bolt that physically continued through the bystander: \
         solid={:?}, ethereal={:?}",
        solid.travelled,
        ethereal.travelled
    );
}

/// Behaviour: combat.projectile.a-bolt-stops-being-a-missile-when-it-hits
///
/// A bolt aimed at the ground reaches it, and reaching it ends the flight: environment reporting
/// clears `MISSILE | ALIGNPATH | PATHCLIPPED`.
#[test]
fn a_bolt_that_reaches_the_ground_stops_being_a_missile() {
    // Down and forward. Five metres of drop at 12 m/s is well under half a second, so 30 frames is
    // comfortably past the impact and comfortably inside the control's "still flying" window.
    let f = fly(Vec3::new(SPEED, 0.0, -12.0), 30);
    assert!(
        !f.state.is_missile(),
        "the bolt reached the ground ({:?} from where it was created) and is still a missile. \
         environment and object collision handling both clear the three missile-path bits with \
         0xFFFFFCBF whenever MISSILE_PS is set. \
         `PhysicsState::MISSILE_CLEAR_MASK` is not applied. state = {:#x}",
        f.travelled,
        f.state.0
    );
    assert!(
        !f.state.is_alignpath() && !f.state.is_pathclipped(),
        "0xFFFFFCBF is !(0x40 | 0x100 | 0x200) -- all three bits go, not just MISSILE_PS: {:#x}",
        f.state.0
    );
    assert_eq!(
        f.state.0,
        BOLT_STATE & PhysicsState::MISSILE_CLEAR_MASK,
        "and nothing else in the word moved"
    );
    // A flight that has *ended* is a position that has stopped changing across simulated time.
    // `INELASTIC_PS` (`0x20000`, and it
    // is in the shard's own `0x28B48`) is what does the stopping —
    // collision response zeroes its velocity outright instead of
    // reflecting. A test that only read the state word could pass on a bolt that cleared its bits
    // and kept sailing.
    assert!(
        f.travelled_last_third.mag2().sqrt() < 0.01,
        "the bolt is still moving a third of a second after it reached the ground: {:?} over the \
         last ten frames, against {:?} for the whole flight",
        f.travelled_last_third,
        f.travelled
    );
}

/// The client detects the impact; the *server* destroys the object. Object/environment
/// reporting and game-object collision
/// callbacks do not free it; server-directed deletion arrives on `0xF747`. A bolt that vanished
/// on impact would be gone before the server agreed.
#[test]
fn the_client_does_not_remove_the_projectile_it_has_seen_hit() {
    let f = fly(Vec3::new(SPEED, 0.0, -12.0), 30);
    assert!(!f.state.is_missile(), "precondition: the impact happened");
    assert!(
        f.present,
        "the object is gone from the scene, and no client code path may do that on a collision: \
         `0xF747 Item_DeleteObject` -> `server-directed object deletion` is the \
         client's only destruction path, and it is behind the server's `update_times[8]` instance test"
    );
}

// -------------------------------------------------------------------------------------------
// The impact effect: the collision callback attempts the object's default script.
// -------------------------------------------------------------------------------------------

/// Behaviour: combat.projectile.an-impact-plays-the-projectiles-default-script
///
/// The client's *whole* visible response to an impact, and its one gate. A spell bolt's real state
/// word carries `SCRIPTED_COLLISION_PS` (`0x8000` — see [`BOLT_STATE`]), which is precisely the
/// bit the impact-effect handler tests.
#[test]
fn an_impact_calls_the_projectiles_default_script() {
    let f = fly(Vec3::new(SPEED, 0.0, -12.0), 30);
    assert!(!f.state.is_missile(), "precondition: the impact happened");
    assert!(
        f.impact_scripts >= 1,
        "the bolt reached the ground with SCRIPTED_COLLISION_PS set and default-script playback \
         was never reached. The collision handler calls it behind exactly that bit"
    );
}

/// The same flight with `SCRIPTED_COLLISION_PS` cleared plays nothing: the handler
/// returns 1 without playback. Without this the assertion above could be passing
/// on a script that is queued for every collision regardless of state.
#[test]
fn an_impact_without_scripted_collision_calls_nothing() {
    let f = fly_with_state(
        Vec3::new(SPEED, 0.0, -12.0),
        30,
        BOLT_STATE & !PhysicsState::SCRIPTED_COLLISION_PS,
    );
    assert!(
        !f.state.is_missile(),
        "precondition: the same impact still happened"
    );
    assert_eq!(
        f.impact_scripts, 0,
        "the impact-effect gate tests SCRIPTED_COLLISION_PS on the body's state word, and with 0x8000 clear \
         it returns 1 having done nothing"
    );
}

/// The mask itself: `MISSILE | ALIGNPATH | PATHCLIPPED`, and not `PUSHABLE_PS` (`0x80`), which
/// `0xFFFFFCBF` leaves alone.
#[test]
fn the_missile_clear_mask_is_the_three_bits_the_image_clears() {
    assert_eq!(PhysicsState::MISSILE_CLEAR_MASK, 0xFFFF_FCBF);
    assert_eq!(
        !PhysicsState::MISSILE_CLEAR_MASK,
        PhysicsState::MISSILE_PS | PhysicsState::ALIGNPATH_PS | PhysicsState::PATHCLIPPED_PS,
        "MISSILE | ALIGNPATH | PATHCLIPPED, and nothing else"
    );
    assert_ne!(
        !PhysicsState::MISSILE_CLEAR_MASK & PhysicsState::PUSHABLE_PS,
        PhysicsState::PUSHABLE_PS,
        "PUSHABLE_PS (0x80) survives the mask"
    );
    // And it applied to the real bolt word: 0x28B48 -> 0x28808.
    assert_eq!(BOLT_STATE & PhysicsState::MISSILE_CLEAR_MASK, 0x0002_8808);
}

// -------------------------------------------------------------------------------------------
// The census: what the live shard actually puts in a velocity-carrying create's state word.
// -------------------------------------------------------------------------------------------

/// Every velocity-carrying create the shard sent in six recordings (none has a motion table):
/// each asks for its collisions to be reported, a missile always travels with
/// `ALIGNPATH_PS` and `PATHCLIPPED_PS`, and the spell-bolt word this suite flies is one of the
/// recorded words.
#[test]
fn every_recorded_velocity_create_reports_collisions_and_its_missiles_align_to_their_path() {
    let mut with_velocity = 0usize;
    let mut missile = 0usize;
    let mut words: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for name in [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "login-account-booted",
        "ddd-interrogation-only",
        "long-solo-play",
    ] {
        let c = Corpus::shared(name);
        for row in c.blobs.iter().filter(|r| {
            r.dir == Direction::ServerToClient
                && (r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                    || r.opcode == Opcode::ITEM_UPDATE_OBJECT.0)
        }) {
            let Ok(m) =
                ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
            else {
                continue;
            };
            let d = &m.0.physicsdesc;
            if d.bitfield & flags::VELOCITY == 0 {
                continue;
            }
            with_velocity += 1;
            let s = PhysicsState(d.state);
            *words.entry(d.state).or_default() += 1;
            assert!(
                s.reports_collisions(),
                "{name}: a velocity-carrying create with state {:#X} does not ask for its \
                 collisions to be reported",
                d.state
            );
            assert_eq!(
                s.is_alignpath(),
                s.is_missile(),
                "{name}: ALIGNPATH_PS and MISSILE_PS travel together (state {:#X})",
                d.state
            );
            if s.is_missile() {
                missile += 1;
                assert_eq!(
                    d.state & !PhysicsState::MISSILE_CLEAR_MASK,
                    0x0000_0340,
                    "{name}: a recorded missile carries all three bits the impact clears \
                     (state {:#X})",
                    d.state
                );
            }
        }
    }
    assert!(
        with_velocity > 0,
        "the recordings carry velocity-carrying creates"
    );
    assert!(
        missile > 0,
        "`MISSILE_PS` does arrive from the shard, so the bits `MISSILE_CLEAR_MASK` exists to take \
         away are really set on real traffic. words={words:?}"
    );
    assert!(
        words.contains_key(&BOLT_STATE),
        "the spell-bolt word this suite flies is one the shard sent: words={words:?}"
    );
}
