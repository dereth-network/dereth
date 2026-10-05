//! Play-script effects. A script type and an intensity select a row of the object's physics-script
//! table (the threshold is inclusive: a row is chosen when the intensity is at most its modifier),
//! and the script's particle hooks give the object live emitters that reach the screen. The same
//! holds end to end off the wire: `0xF754 Effects_PlayScriptID` and `0xF755 Effects_PlayScriptType`
//! are decoded, dispatched through the session's object gate and drained by the scene, a wire
//! table id makes it work for an object whose setup has none, and a message for an object the
//! client does not hold is parked, not dropped. The controls assert absence: an intensity past the
//! table's last row, an unknown script type and an unknown object create nothing.
//! Fixture: the shipped dats (setups, their script tables, the scripts and emitters those name) on
//! a software device. Missing dats fail.

#![cfg(gpu)]

use dereth_assets::Decode;
use dereth_client_net::client_session::dispatch::world_objects::{dispatch, InstanceTable};
use dereth_client_net::client_session::ordering::ParkedBlobs;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{
    DataId, IncomingMessage, LocalTime, NetBlobId, NetQueue, ObjectId, RecipientId, Vec3,
};
use dereth_protocol::objects::{
    EffectsPlayScriptId, EffectsPlayScriptType, ItemCreateObject, ObjectCreatePayload,
};
use dereth_protocol::types::{PhysicsDesc, PublicWeenieDesc};
use dereth_protocol::{Message, Opcode};
use dereth_render::device::Gpu;
use std::sync::Arc;
use {
    dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::EmitterOwner,
    dereth_scene::world_scene::WorldScene,
};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

/// The level-up effect: 8 of the 302 recorded `0xF755`s, all at
/// intensity exactly `1.0`.
const PS_LEVEL_UP: u32 = 138;
/// The portal-storm effect. The table below does not carry it.
const PS_ABSENT: u32 = 115;

/// A shipped setup with a nonzero default physics-script table -- one of only **6 of 5,935**.
const SETUP_WITH_TABLE: u32 = 0x0200_0177;
/// `SETUP_WITH_TABLE`'s table, and the one the wire hands to `SETUP_WITHOUT_TABLE` below.
const TABLE: u32 = 0x3400_00A5;
/// A shipped setup without a default physics-script table, the normal case. Without the wire
/// descriptor's table reaching `MotionDriver::script_table`, an object built from this
/// setup can never play a script type at all.
const SETUP_WITHOUT_TABLE: u32 = 0x0200_0001;

/// The retail store, or **fail**: absent dats are a failure, never a skip.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn setup_of(store: &RetailDatStore, id: u32) -> dereth_assets::Setup {
    let b = store
        .read_typed(DbType::Setup, DataId(id))
        .expect("the setup is shipped");
    dereth_assets::Setup::decode_payload(DataId(id), &b).expect("it decodes")
}

fn table_of(store: &RetailDatStore, id: u32) -> dereth_assets::PhysicsScriptTable {
    let b = store
        .read_typed(DbType::PhysicsScriptTable, DataId(id))
        .expect("the table is shipped");
    dereth_assets::PhysicsScriptTable::decode_payload(DataId(id), &b).expect("it decodes")
}

fn cfg() -> SceneConfig {
    SceneConfig {
        character: false,
        scenery_radius: 2,
        time_of_day: Some(0.5),
        particles: true,
        ..SceneConfig::default()
    }
}

/// `App::frame`'s device steps. `sync_objects` is the one that drains the script queue.
fn step(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    stream: &mut ObjectStream,
    t: &mut f64,
) -> Vec<u8> {
    *t += dereth_client_runtime::platform::clock::HEADLESS_STEP;
    scene
        .sync_objects(store, gpu, stream)
        .expect("sync_objects");
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        dereth_client_runtime::character::CharacterInput::default(),
        LocalTime(*t),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

/// The wire, as far as this crate can see it: `dereth_client_net::client_session`'s WorldObjects gate, then the object
/// stream. Returns what the gate decided, so a test can assert the gate as well as the receiver.
///
/// This is the whole of what `App::frame` does with a queue-10 blob, minus the transport.
struct Wire {
    table: InstanceTable,
    parked: ParkedBlobs,
}

impl Wire {
    fn new() -> Self {
        Self {
            table: InstanceTable::new(),
            parked: ParkedBlobs::default(),
        }
    }

    /// Send one message. Returns `true` when the gate delivered it to the object stream.
    fn send<M: Message>(&mut self, stream: &mut ObjectStream, m: &M) -> bool {
        let body = dereth_protocol::write_body(m).expect("the message encodes");
        let im = IncomingMessage {
            opcode: M::OPCODE.0,
            queue: NetQueue::WorldObjects,
            sender: RecipientId::default(),
            blob_id: NetBlobId::default(),
            body,
        };
        let d = dispatch(&mut self.table, &mut self.parked, None, &im);
        match d.event {
            Some(e @ SessionEvent::WorldObject { .. }) => {
                // Successful create-object gating records the instance sequence, which
                // is what makes the object "known" to every later message about it.
                if M::OPCODE == ItemCreateObject::OPCODE {
                    let mut r = dereth_protocol::Reader::body(&im.body);
                    let id = ObjectId(r.u32().unwrap());
                    let _ = dereth_protocol::types::ObjDesc::read(&mut r).unwrap();
                    let pd = dereth_protocol::types::PhysicsDesc::read(&mut r).unwrap();
                    self.table.set(id, pd.timestamps.instance);
                }
                stream.apply_event(&e, LocalTime(0.0));
                true
            }
            _ => false,
        }
    }
}

/// Put one server object in front of the camera, through a real `0xF745 Item_CreateObject`.
///
/// `phstable` supplies the wire descriptor's physics-script table. Original description setup
/// installs it after the setup's default table, so the descriptor wins.
fn spawn(
    scene: &WorldScene,
    wire: &mut Wire,
    stream: &mut ObjectStream,
    id: ObjectId,
    setup: u32,
    phstable: Option<u32>,
) {
    let block = scene.viewer_block().expect("a resident block");
    let origin =
        dereth_physics::math::localtoglobal(&scene.camera.frame(), Vec3::new(0.0, 6.0, -1.0));
    let mut bitfield = dereth_protocol::types::physicsdesc::flags::POSITION
        | dereth_protocol::types::physicsdesc::flags::SETUP;
    if phstable.is_some() {
        bitfield |= dereth_protocol::types::physicsdesc::flags::PETABLE;
    }
    let payload = ObjectCreatePayload {
        id,
        objdesc: Default::default(),
        physicsdesc: PhysicsDesc {
            bitfield,
            setup_id: Some(setup),
            phstable_id: phstable,
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: (u32::try_from(block.0).unwrap() << 24)
                    | (u32::try_from(block.1).unwrap() << 16)
                    | 1,
                frame: dereth_protocol::types::Frame {
                    origin: origin.into(),
                    orientation: dereth_primitives::Quat::IDENTITY.into(),
                },
            }),
            ..Default::default()
        },
        wdesc: PublicWeenieDesc::default(),
    };
    assert!(
        wire.send(stream, &ItemCreateObject(payload)),
        "the create must be delivered"
    );
}

/// The emitters the scene says belong to `id`.
fn emitters_of(scene: &WorldScene, id: ObjectId) -> Vec<dereth_scene::world_scene::EmitterDegrade> {
    scene
        .emitter_degrade_probe()
        .into_iter()
        .filter(|e| matches!(e.owner, EmitterOwner::Object(o) if o == id))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// 0. Fixtures.
// ---------------------------------------------------------------------------------------------

/// Everything the rest of the file assumes about `client/`, asserted once, so the suite cannot
/// pass against a corpus that no longer has these properties.
#[test]
fn the_fixtures_are_what_this_file_claims() {
    let s = store();
    assert_eq!(
        setup_of(&s, SETUP_WITH_TABLE).default_phstable_id,
        DataId(TABLE),
        "the with-table setup names the table"
    );
    assert_eq!(
        setup_of(&s, SETUP_WITHOUT_TABLE).default_phstable_id,
        DataId(0),
        "the without-table setup must carry none -- otherwise the phstable_id test proves nothing"
    );

    let t = table_of(&s, TABLE);
    let rows = t
        .script_table
        .get(&PS_LEVEL_UP)
        .expect("the table has the level-up type");
    assert_eq!(
        rows.iter().map(|r| r.modifier).collect::<Vec<_>>(),
        vec![1.0],
        "one row at exactly 1.0 -- so 1.5 runs off the end and 1.0 lands on it"
    );
    assert!(
        !t.script_table.contains_key(&PS_ABSENT),
        "the table lacks the portal-storm type"
    );

    // Both opcodes really are on the WorldObjects queue: the census this receiver belongs to.
    for op in [
        Opcode::EFFECTS_PLAY_SCRIPT_TYPE,
        Opcode::EFFECTS_PLAY_SCRIPT_ID,
    ] {
        assert_eq!(
            op.info().and_then(|i| i.recv_queue),
            Some(NetQueue::WorldObjects),
            "{op:?} is dispatched by the world controller, so ObjectStream is its receiver"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 1. The rejecting tests.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.effects.a-play-script-message-reaches-a-visible-emitter
/// **A `0xF755` off the wire creates the emitters its script names.** Rejecting test.
///
/// A receiver that decodes, gates and delivers the message but counts it as `unhandled` fails here.
/// Nothing here touches `WorldScene::play_script_type`: the only thing done to the scene between
/// "no emitters" and "emitters" is `Wire::send`, and the only thing that runs it is `step`'s
/// `sync_objects`.
#[test]
fn a_wire_play_script_type_reaches_a_visible_emitter() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let mut t = 0.0f64;
    let id = ObjectId(0x8000_F190);

    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    spawn(&scene, &mut wire, &mut stream, id, SETUP_WITH_TABLE, None);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(
        emitters_of(&scene, id).is_empty(),
        "it starts with no emitters"
    );
    let played_before = scene.draw.stats.scripts_played;

    assert!(
        wire.send(
            &mut stream,
            &EffectsPlayScriptType {
                id,
                script_type: PS_LEVEL_UP as i32,
                intensity: 1.0
            }
        ),
        "the object is known, so the existence gate passes and the blob is delivered"
    );
    assert_eq!(
        stream.stats.script_type_events, 1,
        "the receiver must decode and park it -- not count it as unhandled"
    );
    assert_eq!(
        stream.stats.unhandled, 0,
        "nothing fell through to the catch-all arm"
    );

    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert_eq!(
        scene.draw.stats.scripts_played,
        played_before + 1,
        "the frame must have run play_script and got 1 back"
    );

    let live = emitters_of(&scene, id);
    assert!(
        !live.is_empty(),
        "the wire message must have created emitters"
    );
    assert!(
        live.iter().any(|e| e.live > 0),
        "the emitters must actually hold particles"
    );

    // The meshes are the ones the shipped script asks for, not merely some mesh.
    let script = table_of(&store, TABLE).script_table[&PS_LEVEL_UP][0].script_id;
    let sb = store
        .read_typed(DbType::PhysicsScript, script)
        .expect("shipped");
    let ps = dereth_assets::PhysicsScript::decode_payload(script, &sb).expect("decodes");
    let meshes: Vec<DataId> = ps
        .script_data
        .iter()
        .filter_map(|st| match &st.hook.data {
            dereth_assets::hook::HookData::CreateParticle {
                emitter_info_id, ..
            } => {
                let b = store
                    .read_typed(DbType::ParticleEmitter, *emitter_info_id)
                    .expect("shipped emitter info");
                Some(
                    dereth_assets::ParticleEmitterInfo::decode_payload(*emitter_info_id, &b)
                        .expect("decodes")
                        .hw_gfxobj_id,
                )
            }
            _ => None,
        })
        .collect();
    assert!(
        !meshes.is_empty(),
        "the shipped script does create particles"
    );
    for e in &live {
        assert!(
            meshes.contains(&e.gfxobj),
            "emitter {} draws {:08X}, which no particle-creation hook of script {:08X} asks for",
            e.emitter,
            e.gfxobj.0,
            script.0
        );
    }
}

/// **The wire-triggered effect reaches the screen.** Rejecting test, in pixels.
///
/// Two runs identical except that one is sent the `0xF755` and one is not, compared at the same
/// frame index. The control is the **same run twice**: two `send = false` runs must be
/// pixel-identical, which is what says the differential below is the effect and not the clock,
/// the particle RNG, or anything else in the scene that moves on its own.
#[test]
fn the_wire_triggered_effect_reaches_the_screen() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);

    let run = |gpu: &mut Gpu, send: bool| -> Vec<u8> {
        let mut scene = WorldScene::load(&store, gpu, cfg()).expect("the landscape loads");
        let mut stream = ObjectStream::new();
        let mut wire = Wire::new();
        let mut t = 0.0f64;
        let id = ObjectId(0x8000_F191);
        step(&store, gpu, &mut scene, &mut stream, &mut t);
        spawn(&scene, &mut wire, &mut stream, id, SETUP_WITH_TABLE, None);
        for _ in 0..3 {
            step(&store, gpu, &mut scene, &mut stream, &mut t);
        }
        if send {
            assert!(wire.send(
                &mut stream,
                &EffectsPlayScriptType {
                    id,
                    script_type: PS_LEVEL_UP as i32,
                    intensity: 1.0
                }
            ));
        }
        let mut last = Vec::new();
        for _ in 0..4 {
            last = step(&store, gpu, &mut scene, &mut stream, &mut t);
        }
        last
    };

    // The control. Nothing about the run is nondeterministic, so an unsent run is reproducible to
    // the byte -- if this fails the comparison below is measuring noise.
    let without = run(&mut gpu, false);
    let again = run(&mut gpu, false);
    assert_eq!(
        without, again,
        "the same frames drawn twice must be identical"
    );

    let with = run(&mut gpu, true);
    assert_eq!(with.len(), without.len());
    let changed = with.iter().zip(&without).filter(|(a, b)| a != b).count();
    assert!(
        changed > 0,
        "the 0xF755 that arrived on the wire must change what is on the screen"
    );
}

/// **The descriptor's physics-script table enables effects on an object with no setup default.**
///
/// Original description setup releases and reloads both sound and physics-script tables after
/// object creation installs setup defaults. The descriptor's table ID therefore wins.
///
/// `SETUP_WITHOUT_TABLE` carries none, which is the case for 5,929 of the 5,935 shipped setups --
/// including the Aluvian male body, i.e. the player and its level-up effect. A build
/// that resolved the script table from the setup alone plays nothing here and everything in
/// [`a_wire_play_script_type_reaches_a_visible_emitter`].
#[test]
fn a_wire_phstable_id_is_what_makes_it_work_for_a_normal_object() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let mut t = 0.0f64;
    let with = ObjectId(0x8000_F192);
    let without = ObjectId(0x8000_F193);

    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    // The same setup twice: one create carries the table on the wire, one does not. Nothing else
    // differs, so the emitters can only be the field.
    spawn(
        &scene,
        &mut wire,
        &mut stream,
        with,
        SETUP_WITHOUT_TABLE,
        Some(TABLE),
    );
    spawn(
        &scene,
        &mut wire,
        &mut stream,
        without,
        SETUP_WITHOUT_TABLE,
        None,
    );
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert_eq!(
        stream.presence(with).and_then(|p| p.phs_table),
        Some(DataId(TABLE)),
        "the descriptor's phstable_id must be kept on the presence"
    );
    assert_eq!(
        stream.presence(without).and_then(|p| p.phs_table),
        None,
        "and absent when the flag is not set"
    );

    for who in [with, without] {
        assert!(wire.send(
            &mut stream,
            &EffectsPlayScriptType {
                id: who,
                script_type: PS_LEVEL_UP as i32,
                intensity: 1.0
            }
        ));
    }
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }

    assert!(
        !emitters_of(&scene, with).is_empty(),
        "the object the server gave a PhysicsScriptTable must play the level-up"
    );
    assert!(
        emitters_of(&scene, without).is_empty(),
        "and the identical object without one must not -- script-type playback fails \
         when the physics-script table is absent, and this is the control that keeps the \
         assertion above about the *field* rather than about the setup"
    );
}

/// **`0xF754` off the wire lands on the same emitters.** Rejecting test for the second opcode.
///
/// Original script-ID dispatch follows the same object gate and playback path as type dispatch,
/// with one fewer argument and no table lookup. This also checks the receiver's payload shape:
/// a script `DataId`, not a script type and intensity pair.
#[test]
fn a_wire_play_script_id_reaches_a_visible_emitter() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let mut t = 0.0f64;
    // No table anywhere -- neither the setup's nor the wire's. `0xF754` must not need one.
    let id = ObjectId(0x8000_F194);

    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    spawn(
        &scene,
        &mut wire,
        &mut stream,
        id,
        SETUP_WITHOUT_TABLE,
        None,
    );
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(emitters_of(&scene, id).is_empty());

    // The script the table *would* have named, read from the dats, handed over directly.
    let script = table_of(&store, TABLE).script_table[&PS_LEVEL_UP][0].script_id;
    assert!(wire.send(
        &mut stream,
        &EffectsPlayScriptId {
            id,
            script_id: script.0
        }
    ));
    assert_eq!(
        stream.stats.script_id_events, 1,
        "the 0xF754 receiver decoded it"
    );
    assert_eq!(stream.stats.unhandled, 0);

    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(
        !emitters_of(&scene, id).is_empty(),
        "a named script needs no PhysicsScriptTable at all"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The controls. All four assert absence, and pass whether or not the receiver is wired.
// ---------------------------------------------------------------------------------------------

/// **An intensity past the table's last row creates nothing, even off the wire.**
///
/// The guard against "a `0xF755` arrived, so make particles". The level-up has one row, at `1.0`;
/// at `1.5` lookup runs past the last row and playback fails.
#[test]
fn a_wire_message_past_the_tables_last_row_creates_nothing() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let mut t = 0.0f64;
    let id = ObjectId(0x8000_F195);
    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    spawn(&scene, &mut wire, &mut stream, id, SETUP_WITH_TABLE, None);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(wire.send(
        &mut stream,
        &EffectsPlayScriptType {
            id,
            script_type: PS_LEVEL_UP as i32,
            intensity: 1.5
        }
    ));
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(
        emitters_of(&scene, id).is_empty(),
        "1.5 is past the table's only row"
    );
    assert_eq!(
        scene.draw.stats.scripts_unplayed, 1,
        "and the frame recorded the refusal"
    );
}

/// **A script type the object's table does not carry creates nothing.**
///
/// The guard against "resolve to any row". This table holds 112 of the 174 types and not
/// the portal-storm type (115).
#[test]
fn a_wire_message_for_a_script_type_the_table_lacks_creates_nothing() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let mut t = 0.0f64;
    let id = ObjectId(0x8000_F196);
    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    spawn(&scene, &mut wire, &mut stream, id, SETUP_WITH_TABLE, None);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(wire.send(
        &mut stream,
        &EffectsPlayScriptType {
            id,
            script_type: PS_ABSENT as i32,
            intensity: 1.0
        }
    ));
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(
        emitters_of(&scene, id).is_empty(),
        "the table has no portal-storm row"
    );
}

/// Behaviour: rendering.effects.a-play-script-for-an-unknown-object-is-parked
/// **A `0xF755` about an object the client does not hold is parked, not delivered.**
///
/// The original handler's sole gate is object existence: a missing-object lookup parks the blob
/// and returns status 4 rather than dropping it. `dereth_client_net::client_session` implements that gate here:
/// `instance_sequence` returns `None` for `0xF754`/`0xF755`, and that arm checks `table.knows(id)`
/// and parks messages for unknown objects. The object-stream receiver needs no duplicate
/// existence gate; this control checks that the message is not delivered to it prematurely.
#[test]
fn a_wire_message_for_an_object_the_client_does_not_hold_is_parked_not_delivered() {
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let ghost = ObjectId(0x8000_DEAD);
    assert!(
        !wire.send(
            &mut stream,
            &EffectsPlayScriptType {
                id: ghost,
                script_type: PS_LEVEL_UP as i32,
                intensity: 1.0
            }
        ),
        "an unknown object parks the blob; nothing reaches the object stream"
    );
    assert_eq!(
        stream.stats.script_type_events, 0,
        "so the receiver never saw it"
    );
    assert!(stream.take_script_events().is_empty());
}

/// **The queue is drained rather than accumulated.** The guard against a receiver that parks for
/// ever, which would pass every "emitters exist" assertion above exactly once and then leak.
#[test]
fn the_queue_is_drained_and_does_not_grow() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let mut t = 0.0f64;
    let id = ObjectId(0x8000_F197);
    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    spawn(&scene, &mut wire, &mut stream, id, SETUP_WITH_TABLE, None);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    for _ in 0..4 {
        assert!(wire.send(
            &mut stream,
            &EffectsPlayScriptType {
                id,
                script_type: PS_LEVEL_UP as i32,
                intensity: 1.0
            }
        ));
    }
    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    assert!(
        stream.take_script_events().is_empty(),
        "one frame drains everything that arrived before it"
    );
    assert_eq!(
        scene.draw.stats.scripts_played, 4,
        "and all four were played, in one frame"
    );
}

/// The materialise effect, the commonest `0xF755` recorded: 118 of 302, every one of them
/// at intensity exactly `1.0`.
const PS_CREATE: u32 = 88;
/// The launch effect -- a war-magic projectile leaving the caster.
const PS_LAUNCH: u32 = 4;
/// A second shipped table, whose launch rows are `(0.0, 0.5, 1.0)` -- three rows, so `<` and
/// `<=` pick *different* ones and the inclusive threshold is observable as a choice rather than
/// merely as the difference between something and nothing.
const TABLE_THREE_ROW: u32 = 0x3400_0005;

/// Put one server object of `setup` in front of the camera, through `0xF745 Item_CreateObject`.
fn spawn_direct(scene: &WorldScene, stream: &mut ObjectStream, id: ObjectId, setup: u32) {
    let block = scene.viewer_block().expect("a resident block");
    let origin =
        dereth_physics::math::localtoglobal(&scene.camera.frame(), Vec3::new(0.0, 6.0, -1.0));
    let payload = ObjectCreatePayload {
        id,
        objdesc: Default::default(),
        physicsdesc: PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                | dereth_protocol::types::physicsdesc::flags::SETUP,
            setup_id: Some(setup),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: (u32::try_from(block.0).unwrap() << 24)
                    | (u32::try_from(block.1).unwrap() << 16)
                    | 1,
                frame: dereth_protocol::types::Frame {
                    origin: origin.into(),
                    orientation: dereth_primitives::Quat::IDENTITY.into(),
                },
            }),
            ..Default::default()
        },
        wdesc: PublicWeenieDesc::default(),
    };
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body: dereth_protocol::write_body(&ItemCreateObject(payload)).unwrap(),
        },
        LocalTime(0.0),
    );
}

// ---------------------------------------------------------------------------------------------
// 0. The fixtures are the shipped data, not this file's belief about it.
// ---------------------------------------------------------------------------------------------

/// Everything the rest of the file assumes about `client/`, asserted once.
#[test]
fn the_fixtures_of_the_directly_played_scripts_are_what_this_file_claims() {
    let s = store();
    let b = s
        .read_typed(DbType::Setup, DataId(SETUP_WITH_TABLE))
        .expect("shipped");
    let setup =
        dereth_assets::Setup::decode_payload(DataId(SETUP_WITH_TABLE), &b).expect("decodes");
    assert_eq!(
        setup.default_phstable_id,
        DataId(TABLE),
        "the setup names the table"
    );

    let t = table_of(&s, TABLE);
    for (ty, name) in [(PS_CREATE, "materialise"), (PS_LEVEL_UP, "level-up")] {
        let rows = t
            .script_table
            .get(&ty)
            .unwrap_or_else(|| panic!("the table has {name}"));
        assert_eq!(
            rows.iter().map(|r| r.modifier).collect::<Vec<_>>(),
            vec![1.0],
            "{name}: one row at exactly 1.0 -- the shape `<` cannot resolve at intensity 1.0"
        );
    }
    assert!(
        !t.script_table.contains_key(&PS_ABSENT),
        "the table lacks the portal-storm type"
    );

    let t3 = table_of(&s, TABLE_THREE_ROW);
    let rows3 = t3
        .script_table
        .get(&PS_LAUNCH)
        .expect("the table has the launch type");
    assert_eq!(
        rows3.iter().map(|r| r.modifier).collect::<Vec<_>>(),
        vec![0.0, 0.5, 1.0],
        "three rows -- so `<` and `<=` name different scripts, not merely one and none"
    );
    assert_ne!(
        rows3[0].script_id, rows3[1].script_id,
        "the two rows are distinguishable"
    );
}

// ---------------------------------------------------------------------------------------------
// 1. The script-table threshold is inclusive.
// ---------------------------------------------------------------------------------------------

/// **Script-table lookup takes the row whose modifier equals the intensity**, not only rows it
/// is strictly below.
///
/// An intensity less than or equal to the row's modifier selects that row's script; a greater
/// intensity, or an unordered comparison (a NaN on either side), continues to the next row. So
/// the threshold is inclusive without treating every comparison as success.
#[test]
fn the_threshold_is_inclusive_so_an_exact_match_selects_its_own_row() {
    let s = store();
    let raw = table_of(&s, TABLE_THREE_ROW);
    let rows = raw.script_table.get(&PS_LAUNCH).unwrap().clone();
    let table = dereth_world_data::anim_convert::physics_script_table(&raw);

    // 0.0 against rows (0.0, 0.5, 1.0): the machine takes row 0, `<` took row 1.
    assert_eq!(
        dereth_animation::get_script(&table, PS_LAUNCH, 0.0),
        Some(rows[0].script_id),
        "an exact match must select its own row, not the next one"
    );

    // The single-row 1.0 shape that dominates the shipped data: `<` resolved to nothing at all.
    let raw1 = table_of(&s, TABLE);
    let one = raw1.script_table[&PS_CREATE][0].script_id;
    let t = dereth_world_data::anim_convert::physics_script_table(&raw1);
    assert_eq!(dereth_animation::get_script(&t, PS_CREATE, 1.0), Some(one));

    // ...and above the last row there is still nothing. The inclusive rule is not "always yes".
    assert_eq!(dereth_animation::get_script(&t, PS_CREATE, 1.5), None);
}

// ---------------------------------------------------------------------------------------------
// 2. A script type reaches an emitter, through the scene.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.effects.a-played-script-type-creates-the-objects-emitters
/// **`0xF755`'s payload, played on a real object in a real scene, creates live emitters.**
///
/// The original handler resolves type and intensity through the object's script table, queues
/// the selected script, then script updates execute particle-creation hooks on that object.
///
/// The emitter's mesh is asserted against the shipped `PhysicsScript`'s own `CreateParticle`
/// hooks, so a build that created *some* emitter would still fail.
#[test]
fn a_played_script_type_gives_the_object_it_names_live_emitters() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut t = 0.0f64;
    let id = ObjectId(0x8000_F170);

    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    spawn_direct(&scene, &mut stream, id, SETUP_WITH_TABLE);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(
        emitters_of(&scene, id).is_empty(),
        "it starts with no emitters"
    );

    assert!(
        scene.play_script_type(id, PS_LEVEL_UP, 1.0),
        "playing the level-up at 1.0 must resolve: the table has a row at exactly 1.0"
    );
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }

    let live = emitters_of(&scene, id);
    assert!(
        !live.is_empty(),
        "the script's particle-creation hooks must have created emitters"
    );

    // The mesh is the one the shipped script asks for, not merely *a* mesh.
    let script = table_of(&store, TABLE).script_table[&PS_LEVEL_UP][0].script_id;
    let sb = store
        .read_typed(DbType::PhysicsScript, script)
        .expect("shipped");
    let ps = dereth_assets::PhysicsScript::decode_payload(script, &sb).expect("decodes");
    let wanted: Vec<DataId> = ps
        .script_data
        .iter()
        .filter_map(|st| match &st.hook.data {
            dereth_assets::hook::HookData::CreateParticle {
                emitter_info_id, ..
            } => Some(*emitter_info_id),
            _ => None,
        })
        .collect();
    assert!(
        !wanted.is_empty(),
        "the shipped script does create particles"
    );
    assert_eq!(
        live.len(),
        wanted.len(),
        "one emitter per particle-creation hook: physics-object emitter creation is
         called once per hook and an `emitter_id` of 0 allocates a fresh id each time, so the
         hooks and the emitters are one-to-one"
    );

    // Emitter initialization builds every part from the one
    // `hw_gfxobj_id` its `ParticleEmitterInfo` names, and that is the id the probe reports. So
    // resolving each hook's emitter-info through the dats gives the exact set of meshes the
    // shipped script asks for -- a build that created *some* emitter fails here.
    let meshes: Vec<DataId> = wanted
        .iter()
        .map(|info| {
            let b = store
                .read_typed(DbType::ParticleEmitter, *info)
                .expect("shipped emitter info");
            dereth_assets::ParticleEmitterInfo::decode_payload(*info, &b)
                .expect("decodes")
                .hw_gfxobj_id
        })
        .collect();
    for e in &live {
        assert!(
            meshes.contains(&e.gfxobj),
            "emitter {} draws {:08X}, which no particle-creation hook of script {:08X} asks for \
             (the script's own meshes are {:08X?})",
            e.emitter,
            e.gfxobj.0,
            script.0,
            meshes.iter().map(|m| m.0).collect::<Vec<_>>()
        );
    }

    assert!(
        live.iter().any(|e| e.live > 0),
        "the emitters must actually hold particles"
    );
}

/// **`0xF754` names the script outright, and lands on the same emitters `0xF755` resolved to.**
///
/// Original script-ID playback bypasses table lookup and enters the shared script-playback
/// path. Handing it the ID that the table would produce for the level-up at 1.0 must
/// therefore give the same result as
/// [`a_played_script_type_gives_the_object_it_names_live_emitters`], which is what makes the two
/// entry points one path rather than two.
#[test]
fn a_played_script_id_takes_the_same_path_without_the_table() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut t = 0.0f64;
    let id = ObjectId(0x8000_F174);
    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    spawn_direct(&scene, &mut stream, id, SETUP_WITH_TABLE);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(emitters_of(&scene, id).is_empty());

    // The ID resolved for the level-up at intensity 1.0, read from the DATs rather than written
    // here, so the two entry points are being compared on the shipped data.
    let script = table_of(&store, TABLE).script_table[&PS_LEVEL_UP][0].script_id;
    assert!(
        scene.play_script_id(id, script),
        "a named script needs no table"
    );
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(
        !emitters_of(&scene, id).is_empty(),
        "the named script must create its emitters"
    );
}

/// **A `DataId` of zero creates nothing.** Original script playback rejects a null ID before
/// creating anything; this is the third control.
#[test]
fn a_null_script_id_creates_nothing() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut t = 0.0f64;
    let id = ObjectId(0x8000_F175);
    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    spawn_direct(&scene, &mut stream, id, SETUP_WITH_TABLE);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(
        !scene.play_script_id(id, DataId(0)),
        "a null script id is refused"
    );
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(emitters_of(&scene, id).is_empty());
}

// ---------------------------------------------------------------------------------------------
// 3. The control: the table is consulted, not bypassed.
// ---------------------------------------------------------------------------------------------

/// **An intensity past the table's last row resolves to nothing.**
///
/// This guards against unconditional emission. Like the materialise example above, the
/// level-up case here has one row at 1.0. An intensity of 1.5 passes the last row; the original
/// lookup returns an invalid data ID and playback fails without creating an emitter.
/// A build that emitted whenever asked passes
/// [`a_played_script_type_gives_the_object_it_names_live_emitters`] and fails here.
#[test]
fn an_intensity_past_the_tables_last_row_creates_nothing() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut t = 0.0f64;
    let id = ObjectId(0x8000_F171);
    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    spawn_direct(&scene, &mut stream, id, SETUP_WITH_TABLE);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }

    assert!(
        !scene.play_script_type(id, PS_LEVEL_UP, 1.5),
        "1.5 is past the table's only row"
    );
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(
        emitters_of(&scene, id).is_empty(),
        "nothing may be created off the end of the table"
    );
}

/// **A script type the object's table does not carry resolves to nothing.**
///
/// Original script-table lookup walks the bucket chain and returns an invalid data ID on a
/// miss. This is the second half of the same control.
#[test]
fn an_unknown_script_type_creates_nothing() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut t = 0.0f64;
    let id = ObjectId(0x8000_F172);
    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    spawn_direct(&scene, &mut stream, id, SETUP_WITH_TABLE);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(
        !scene.play_script_type(id, PS_ABSENT, 1.0),
        "a type the table lacks resolves to nothing"
    );
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(emitters_of(&scene, id).is_empty());
}

/// **An object the scene does not hold is not scripted.** The original network handler parks
/// its message for the missing object; this direct scene call checks only that nothing plays now.
#[test]
fn an_unknown_object_is_not_scripted() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    let mut t = 0.0f64;
    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    assert!(!scene.play_script_type(ObjectId(0x8000_DEAD), PS_LEVEL_UP, 1.0));
}

// ---------------------------------------------------------------------------------------------
// 4. It reaches the screen.
// ---------------------------------------------------------------------------------------------

/// **The played effect changes pixels**, and the identical run without it does not.
///
/// Two runs of the same scene over the same clock, differing only in whether
/// `play_script_type` was called. The second control -- two runs that both decline to play it
/// coming out byte-identical -- is what makes the first difference the effect rather than the
/// clock or the device.
#[test]
fn the_played_effect_reaches_the_screen() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);

    let run = |gpu: &mut Gpu, play: bool| -> Vec<u8> {
        let mut scene = WorldScene::load(&store, gpu, cfg()).expect("the landscape loads");
        let mut stream = ObjectStream::new();
        let mut t = 0.0f64;
        let id = ObjectId(0x8000_F173);
        step(&store, gpu, &mut scene, &mut stream, &mut t);
        spawn_direct(&scene, &mut stream, id, SETUP_WITH_TABLE);
        for _ in 0..3 {
            step(&store, gpu, &mut scene, &mut stream, &mut t);
        }
        if play {
            assert!(scene.play_script_type(id, PS_LEVEL_UP, 1.0));
        }
        let mut last = Vec::new();
        for _ in 0..4 {
            last = step(&store, gpu, &mut scene, &mut stream, &mut t);
        }
        last
    };

    let without = run(&mut gpu, false);
    let again = run(&mut gpu, false);
    assert_eq!(
        without, again,
        "the same frame drawn twice must be identical"
    );

    let with = run(&mut gpu, true);
    assert_eq!(with.len(), without.len());
    let changed = with.iter().zip(&without).filter(|(a, b)| a != b).count();
    assert!(changed > 0, "playing the script must change the frame");
}
