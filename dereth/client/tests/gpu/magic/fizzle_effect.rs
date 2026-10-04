//! A recorded spell fizzle (`Effects_PlayScriptType 0xF755`, script type 0x51, intensity 0.5) on
//! the player selects the player table `0x34000004`'s single modifier-1.0 row, script `0x33000103`,
//! whose two time-zero hooks attach one body emitter (particle hook, emitter info `0x32000107`,
//! object frame, emitter id 1) and raise one sound-table trigger of type 94 (0x5E, the Fizzle
//! sound). A script type the table lacks creates nothing, and a script for an object the client
//! does not hold is not delivered. Pixels, draw submission and audio playback are not asserted.
//! Fixture: the recorded fizzles (long-solo-play's first one replayed), re-addressed to a
//! synthetic player created through the world-objects dispatch; the retail dats; a software
//! device driven by direct scene steps (no App, no socket).

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
#![allow(clippy::pedantic)]

use dereth_client::world::{SceneReads, SceneWrites};
use std::sync::Arc;

use dereth_assets::{Decode, HookData};
use dereth_client::audio::SoundTrigger;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{EmitterOwner, SceneConfig, WorldScene};
use dereth_client_net::client_session::dispatch::world_objects::{dispatch, InstanceTable};
use dereth_client_net::client_session::ordering::ParkedBlobs;
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{
    DataId, IncomingMessage, LocalTime, NetBlobId, NetQueue, ObjectId, RecipientId,
};
use dereth_protocol::objects::{
    EffectsPlayScriptType, ItemCreateObject, LoginCreatePlayer, ObjectCreatePayload,
};
use dereth_protocol::types::{PhysicsDesc, PublicWeenieDesc};
use dereth_protocol::Message;
use dereth_render::device::Gpu;

/// `PlayScript::Fizzle` — `ACE.Entity/Enum/PlayScript.cs:86`.
const PS_FIZZLE: u32 = 0x51;
/// `GameMessageScript(Guid, PlayScript.Fizzle, 0.5f)`'s third argument.
const FIZZLE_INTENSITY: f32 = 0.5;
/// `Sound::Fizzle` — `ACE.Entity/Enum/Sound.cs:99`, and the `SoundTableHook`'s `sound_type`.
const SOUND_FIZZLE: u32 = 0x5E;
/// Selected fizzle script row and the emitter-info record named by its particle hook.
const FIZZLE_SCRIPT: u32 = 0x3300_0103;
const FIZZLE_EMITTER_INFO: u32 = 0x3200_0107;
/// Selected player physics-script table, the one the recorded player creates carry.
const PLAYER_TABLE: u32 = 0x3400_0004;
/// Ordinary-creature table selected from the earlier sampled non-player creates; a data control.
const CREATURE_TABLE: u32 = 0x3400_002B;
/// The Aluvian male body. It carries **no** `default_phstable_id`, so the wire's `phstable_id` is
/// the only thing that can make a fizzle resolve at all.
const PLAYER_SETUP: u32 = 0x0200_0001;
/// A script type the player's table does not carry — the control's.
const PS_ABSENT: u32 = 90;

/// The id this file's player takes.
const PLAYER: ObjectId = ObjectId(0x5000_0167);
/// An id nothing ever creates — the object-existence gate's control.
const STRANGER: ObjectId = ObjectId(0x6167_0001);

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn table_of(store: &RetailDatStore, id: u32) -> dereth_assets::PhysicsScriptTable {
    let b = store
        .read_typed(DbType::PhysicsScriptTable, DataId(id))
        .expect("the table is shipped");
    dereth_assets::PhysicsScriptTable::decode_payload(DataId(id), &b).expect("it decodes")
}

fn script_of(store: &RetailDatStore, id: DataId) -> dereth_assets::PhysicsScript {
    let b = store
        .read_typed(DbType::PhysicsScript, id)
        .expect("the script is shipped");
    dereth_assets::PhysicsScript::decode_payload(id, &b).expect("it decodes")
}

// ---------------------------------------------------------------------------------------------
// The recording, read rather than transcribed.
// ---------------------------------------------------------------------------------------------

/// Every decoded server `0xF755` of script type 0x51 in every recording, with its session, raw
/// blob, target and intensity, plus a heuristic player: the first decoded create id with high
/// nibble 5 and that create's script table. Decode failures are skipped. This selection does not
/// derive the player identity from `LoginCreatePlayer`.
fn recorded_fizzle_scripts() -> Vec<(
    &'static str,
    Vec<u8>,
    ObjectId,
    f32,
    Option<ObjectId>,
    Option<u32>,
)> {
    let mut out = Vec::new();
    for c in dereth_client_net::client_session::testing::Corpus::shared_all() {
        let s: &'static str = &c.name;
        let mut player = None;
        let mut player_phs = None;
        for row in &c.blobs {
            if row.dir != dereth_client_net::client_session::testing::Direction::ServerToClient
                || row.opcode != ItemCreateObject::OPCODE.0
            {
                continue;
            }
            let Ok(ItemCreateObject(p)) =
                dereth_protocol::read_body_padded::<ItemCreateObject>(&row.payload[4..])
            else {
                continue;
            };
            if p.id.0 >> 28 == 5 && player.is_none() {
                player = Some(p.id);
                player_phs = p.physicsdesc.phstable_id;
            }
        }
        for row in &c.blobs {
            if row.dir != dereth_client_net::client_session::testing::Direction::ServerToClient
                || row.opcode != 0xF755
            {
                continue;
            }
            let Ok(m) =
                dereth_protocol::read_body_padded::<EffectsPlayScriptType>(&row.payload[4..])
            else {
                continue;
            };
            if u32::try_from(m.script_type).ok() == Some(PS_FIZZLE) {
                out.push((
                    s,
                    row.payload.clone(),
                    m.id,
                    m.intensity,
                    player,
                    player_phs,
                ));
            }
        }
    }
    out
}

/// Every recorded fizzle targets the session's player (the first decoded create id with top
/// nibble 5), at intensity 0.5, resolved through the player table, in a sixteen-byte blob. Then
/// the DAT premises: one modifier-1.0 script row, selection at 0.5 and two time-zero
/// particle/sound hooks. The script sound type is the public Fizzle value; these checks do not
/// establish that no wire stream can ever carry that sound, nor do they test the modifier
/// equality boundary.
#[test]
fn every_recorded_fizzle_targets_the_player_and_the_script_is_what_this_file_claims() {
    let fizzles = recorded_fizzle_scripts();
    assert!(
        !fizzles.is_empty(),
        "the recordings carry a decoded script type 0x51 event"
    );
    for (s, blob, target, intensity, player, phs) in &fizzles {
        assert_eq!(
            Some(*target),
            *player,
            "{s}: the fizzle target matches the first decoded create ID with top nibble 5"
        );
        assert!(
            (intensity - FIZZLE_INTENSITY).abs() < f32::EPSILON,
            "{s}: `GameMessageScript(Guid, PlayScript.Fizzle, 0.5f)`: {intensity}"
        );
        assert_eq!(
            *phs,
            Some(PLAYER_TABLE),
            "{s}: resolved through the player's own table"
        );
        assert_eq!(
            blob.len(),
            16,
            "{s}: `[opcode][id][script_type][intensity]`, and nothing else"
        );
    }

    let s = store();
    let rows = table_of(&s, PLAYER_TABLE)
        .script_table
        .remove(&PS_FIZZLE)
        .expect("the player's own table has a fizzle row");
    assert_eq!(
        rows.iter()
            .map(|r| (r.modifier, r.script_id.0))
            .collect::<Vec<_>>(),
        vec![(1.0, FIZZLE_SCRIPT)],
        "the selected fizzle table entry is the single modifier-1.0 row"
    );
    let chosen = dereth_animation::script::get_script(
        &dereth_world_data::anim_convert::physics_script_table(&table_of(&s, PLAYER_TABLE)),
        PS_FIZZLE,
        FIZZLE_INTENSITY,
    );
    assert_eq!(
        chosen,
        Some(DataId(FIZZLE_SCRIPT)),
        "script selection returns the modifier-1.0 row for intensity 0.5"
    );

    // The script itself: two hooks, both at t = 0, and the second is the sound.
    let script = script_of(&s, DataId(FIZZLE_SCRIPT));
    assert_eq!(script.script_data.len(), 2, "{:?}", script.script_data);
    assert!(
        script.script_data.iter().all(|st| st.start_time == 0.0),
        "both at t = 0"
    );
    match script.script_data[0].hook.data {
        HookData::CreateParticle {
            emitter_info_id,
            part_index,
            emitter_id,
            ..
        } => {
            assert_eq!(
                script.script_data[0].hook.hook_type, 13,
                "13 CREATE_PARTICLE"
            );
            assert_eq!(emitter_info_id, DataId(FIZZLE_EMITTER_INFO));
            assert_eq!(
                part_index,
                u32::MAX,
                "`0xFFFFFFFF` -- the object frame, not a part"
            );
            assert_eq!(emitter_id, 1);
        }
        ref other => panic!("hook 0 is the particle hook: {other:?}"),
    }
    match script.script_data[1].hook.data {
        HookData::SoundTable { sound_type } => {
            assert_eq!(script.script_data[1].hook.hook_type, 2, "2 SOUND_TABLE");
            assert_eq!(
                sound_type, SOUND_FIZZLE,
                "the script sound-table hook names the public Sound::Fizzle value 94 (0x5E)"
            );
        }
        ref other => panic!("hook 1 is the sound hook: {other:?}"),
    }

    // Data controls: the selected ordinary-creature table lacks fizzle and the player table
    // lacks type 90. This is not an exhaustive census of every other table in the corpus.
    assert!(
        !table_of(&s, CREATURE_TABLE)
            .script_table
            .contains_key(&PS_FIZZLE),
        "the selected ordinary-creature script table has no fizzle row"
    );
    assert!(
        !table_of(&s, PLAYER_TABLE)
            .script_table
            .contains_key(&PS_ABSENT),
        "PLAYER_TABLE lacks {PS_ABSENT}, which is what makes the control below a control"
    );
}

// ---------------------------------------------------------------------------------------------
// The direct dispatch wrapper and scene-step harness.
// ---------------------------------------------------------------------------------------------

struct Wire {
    table: InstanceTable,
    parked: ParkedBlobs,
    player: Option<ObjectId>,
}

impl Wire {
    fn new() -> Self {
        Self {
            table: InstanceTable::new(),
            parked: ParkedBlobs::default(),
            player: None,
        }
    }

    fn feed(&mut self, stream: &mut ObjectStream, opcode: u32, body: Vec<u8>) -> bool {
        let im = IncomingMessage {
            opcode,
            queue: NetQueue::WorldObjects,
            sender: RecipientId::default(),
            blob_id: NetBlobId::default(),
            body,
        };
        let d = dispatch(&mut self.table, &mut self.parked, self.player, &im);
        match d.event {
            Some(e @ SessionEvent::PlayerCreated(_)) => {
                if let SessionEvent::PlayerCreated(id) = e {
                    self.player = Some(id);
                }
                stream.apply_event(&e, LocalTime(0.0));
                true
            }
            Some(e @ SessionEvent::WorldObject { .. }) => {
                if opcode == ItemCreateObject::OPCODE.0 {
                    let mut r = dereth_protocol::Reader::body(&im.body);
                    let id = ObjectId(r.u32().expect("the id"));
                    let _ = dereth_protocol::types::ObjDesc::read(&mut r).expect("objdesc");
                    let pd =
                        dereth_protocol::types::PhysicsDesc::read(&mut r).expect("physicsdesc");
                    self.table.set(id, pd.timestamps.instance);
                }
                stream.apply_event(&e, LocalTime(0.0));
                true
            }
            _ => false,
        }
    }

    fn send<M: Message>(&mut self, stream: &mut ObjectStream, m: &M) -> bool {
        let body = dereth_protocol::write_body(m).expect("the message encodes");
        self.feed(stream, M::OPCODE.0, body)
    }

    /// Keep the recorded opcode/body bytes except target-ID bytes 4..8, rewritten to PLAYER.
    /// Feed the remaining body directly to dispatch; this is not packet transport/session replay.
    fn replay_fizzle(&mut self, stream: &mut ObjectStream, mut blob: Vec<u8>) -> bool {
        assert_eq!(
            u32::from_le_bytes(blob[0..4].try_into().expect("4")),
            0xF755
        );
        blob[4..8].copy_from_slice(&PLAYER.0.to_le_bytes());
        self.feed(stream, 0xF755, blob[4..].to_vec())
    }
}

fn cfg() -> SceneConfig {
    SceneConfig {
        character: true,
        scenery_radius: 0,
        land_radius: 1,
        time_of_day: Some(0.5),
        particles: true,
        ..SceneConfig::default()
    }
}

fn scene(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
    let mut s = WorldScene::load(store, gpu, cfg()).expect("the landscape loads");
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    s.attach_character(store, &region, gpu)
        .expect("the body is created");
    s
}

/// Direct scene step: synchronize objects, advance with zero input, drain sound triggers, then
/// stream/upload/draw/end the GPU frame. This models the relevant scene ordering without
/// invoking App::frame, an audio backend or the normal transport/session loop.
fn step(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    stream: &mut ObjectStream,
    t: &mut f64,
    sounds: &mut Vec<SoundTrigger>,
) {
    *t += dereth_client::app::HEADLESS_STEP;
    scene
        .sync_objects(store, gpu, stream)
        .expect("sync_objects");
    scene.update(
        dereth_client::camera::CameraInput::default(),
        dereth_client::character::CharacterInput::default(),
        LocalTime(*t),
        1.0 / 30.0,
    );
    sounds.extend(scene.take_sound_events());
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
}

/// Supply synthetic 0xF746 player identification then 0xF745 create through the dispatch wrapper;
/// this is not a complete login handshake. The wrapper records create instance stamps itself.
fn log_in(scene: &WorldScene, wire: &mut Wire, stream: &mut ObjectStream) {
    assert!(
        wire.send(stream, &LoginCreatePlayer { player_id: PLAYER }),
        "`0xF746 Login_CreatePlayer` must reach the stream"
    );
    let block = scene.viewer_block().expect("a resident block");
    let origin = scene
        .character
        .as_ref()
        .expect("a body")
        .render_frame()
        .origin;
    let payload = ObjectCreatePayload {
        id: PLAYER,
        objdesc: Default::default(),
        physicsdesc: PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                | dereth_protocol::types::physicsdesc::flags::SETUP
                | dereth_protocol::types::physicsdesc::flags::PETABLE,
            setup_id: Some(PLAYER_SETUP),
            phstable_id: Some(PLAYER_TABLE),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: (u32::try_from(block.0).expect("x") << 24)
                    | (u32::try_from(block.1).expect("y") << 16)
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
        "the player's own create"
    );
}

fn body_emitters(scene: &WorldScene) -> Vec<dereth_client::world::EmitterDegrade> {
    scene
        .emitter_degrade_probe()
        .into_iter()
        .filter(|e| matches!(e.owner, EmitterOwner::Body))
        .collect()
}

/// Scene plus synthetic identified player/body, after one initial step and three post-create
/// steps. Assert script-table binding/no body emitters, then clear collected sound triggers.
fn logged_in(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
) -> (WorldScene, ObjectStream, Wire, f64, Vec<SoundTrigger>) {
    let mut scene = scene(store, gpu);
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let mut t = 0.0f64;
    let mut sounds = Vec::new();
    step(store, gpu, &mut scene, &mut stream, &mut t, &mut sounds);
    log_in(&scene, &mut wire, &mut stream);
    for _ in 0..3 {
        step(store, gpu, &mut scene, &mut stream, &mut t, &mut sounds);
    }
    assert_eq!(
        stream.player(),
        Some(PLAYER),
        "`0xF746` gave the stream the player's id"
    );
    assert_eq!(
        scene
            .character
            .as_ref()
            .expect("a body")
            .driver()
            .script_table,
        Some(DataId(PLAYER_TABLE)),
        "description application installs the encoded player physics-script table on the body"
    );
    assert!(
        body_emitters(&scene).is_empty(),
        "the body starts with no emitters"
    );
    sounds.clear();
    (scene, stream, wire, t, sounds)
}

// ---------------------------------------------------------------------------------------------
// The tests.
// ---------------------------------------------------------------------------------------------

/// Behaviour: magic.fizzle.the-recorded-fizzle-plays-the-bodys-emitter-and-sound
///
/// Re-address the first selected recorded fizzle to the synthetic player. After four scene
/// steps, require one body emitter with the decoded graphics-object ID and live particles.
/// Collected triggers must contain exactly one Table event of type 94 with the body table ID;
/// other sound events are allowed, and no sound sample resolution/playback or pixel check occurs.
#[test]
fn the_recorded_fizzle_reaches_the_bodys_emitter_and_raises_its_sound() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let (mut scene, mut stream, mut wire, mut t, mut sounds) = logged_in(&store, &mut gpu);
    let table = scene
        .character_sound_table()
        .expect("the body supplies a sound-table ID for the script hook trigger");

    let blob = recorded_fizzle_scripts()
        .into_iter()
        .find(|(s, ..)| *s == "long-solo-play")
        .map(|(_, b, ..)| b)
        .expect("long-solo-play's first recorded fizzle");
    assert!(
        wire.replay_fizzle(&mut stream, blob),
        "the gate delivers a 0xF755 about a known object"
    );
    for _ in 0..4 {
        step(
            &store,
            &mut gpu,
            &mut scene,
            &mut stream,
            &mut t,
            &mut sounds,
        );
    }

    // --- the effect ------------------------------------------------------------------------
    let live = body_emitters(&scene);
    assert_eq!(
        live.len(),
        1,
        "script {FIZZLE_SCRIPT:#010X} has exactly one `particle-creation hook`, so exactly one \
         emitter belongs on the body: {live:?}"
    );
    let mesh = {
        let b = store
            .read_typed(DbType::ParticleEmitter, DataId(FIZZLE_EMITTER_INFO))
            .expect("shipped emitter info");
        dereth_assets::ParticleEmitterInfo::decode_payload(DataId(FIZZLE_EMITTER_INFO), &b)
            .expect("it decodes")
            .hw_gfxobj_id
    };
    assert_eq!(
        live[0].gfxobj, mesh,
        "the emitter probe names the graphics object from ParticleEmitterInfo {FIZZLE_EMITTER_INFO:#010X}"
    );
    assert!(live.iter().any(|e| e.live > 0), "and it holds particles");

    // --- the sound -------------------------------------------------------------------------
    let fizzle: Vec<&SoundTrigger> = sounds
        .iter()
        .filter(|s| matches!(s, SoundTrigger::Table { stype, .. } if *stype == SOUND_FIZZLE))
        .collect();
    assert_eq!(
        fizzle.len(),
        1,
        "the four steps must yield exactly one sound-table trigger with type 94; collected triggers: {sounds:?}"
    );
    match fizzle[0] {
        SoundTrigger::Table {
            table: t, stype, ..
        } => {
            assert_eq!(
                *t, table,
                "the trigger carries the body sound-table ID, not the region table"
            );
            assert_eq!(*stype, SOUND_FIZZLE);
        }
        other => panic!("{other:?}"),
    }
}

/// Reject unconditional effects for any player-targeted 0xF755: selected absent type 90 yields
/// no body emitters at the final sample and no collected Table trigger with type 94 over four
/// steps. The test does not require complete silence or absence of every other sound type.
#[test]
fn a_script_type_the_players_table_lacks_creates_nothing_and_is_silent() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let (mut scene, mut stream, mut wire, mut t, mut sounds) = logged_in(&store, &mut gpu);

    assert!(wire.send(
        &mut stream,
        &EffectsPlayScriptType {
            id: PLAYER,
            script_type: i32::try_from(PS_ABSENT).expect("small"),
            intensity: FIZZLE_INTENSITY,
        }
    ));
    for _ in 0..4 {
        step(
            &store,
            &mut gpu,
            &mut scene,
            &mut stream,
            &mut t,
            &mut sounds,
        );
    }
    assert!(body_emitters(&scene).is_empty(), "no row, no emitter");
    assert!(
        !sounds
            .iter()
            .any(|s| matches!(s, SoundTrigger::Table { stype, .. } if *stype == SOUND_FIZZLE)),
        "and no fizzle sound: {sounds:?}"
    );
}

/// Script dispatch is gated on object existence. For this unknown ID the wrapper must
/// report no delivered event; after four steps require no body emitters and no type 94 table
/// trigger. ParkedBlobs is supplied to dispatch but its contents are not asserted here.
#[test]
fn a_fizzle_for_an_object_the_client_does_not_hold_plays_nothing() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let (mut scene, mut stream, mut wire, mut t, mut sounds) = logged_in(&store, &mut gpu);

    let delivered = wire.send(
        &mut stream,
        &EffectsPlayScriptType {
            id: STRANGER,
            script_type: i32::try_from(PS_FIZZLE).expect("small"),
            intensity: FIZZLE_INTENSITY,
        },
    );
    assert!(
        !delivered,
        "the dispatch wrapper did not deliver an event for the unknown object"
    );
    for _ in 0..4 {
        step(
            &store,
            &mut gpu,
            &mut scene,
            &mut stream,
            &mut t,
            &mut sounds,
        );
    }
    assert!(
        body_emitters(&scene).is_empty(),
        "and nothing landed on the body"
    );
    assert!(
        !sounds
            .iter()
            .any(|s| matches!(s, SoundTrigger::Table { stype, .. } if *stype == SOUND_FIZZLE)),
        "{sounds:?}"
    );
}
