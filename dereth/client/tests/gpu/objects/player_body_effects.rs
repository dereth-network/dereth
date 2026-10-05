//! A `0xF755 Effects_PlayScriptType` addressed to the player's own object plays on the player's
//! body: a level-up puts the shipped script's emitters on it, and a script type the body's
//! table lacks puts nothing there.
//!
//! The client draws the player from [`dereth_client_runtime::character::Character`], not from
//! `WorldScene::objects`, so `WorldScene::play_script_type` has a player branch, and the body's
//! `MotionDriver` scripts must be pumped by the frame for particle-creation hooks to run. The scene
//! here has a local body (`SceneConfig { character: true, .. }`) and every message passes through
//! the WorldObjects gate and the object stream.
//!
//! Fixture: five recorded sessions and the retail dats, read rather than asserted from memory.
//! Every recorded `0xF755` with `script_type == 138` names the session's **own player** with
//! `intensity == 1.0`; that player's `0xF745` carries physics-script table `0x34000004`, which has
//! a level-up row at exactly `1.0`, while the creature table `0x3400002B` every sampled
//! non-player create carries has none. **Fails** when the retail dats are absent or no GPU device
//! can be created.

#![cfg(gpu)]

use super::common::{retail_store, test_gpu};
use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_assets::Decode;
use dereth_client_net::client_session::dispatch::world_objects::{dispatch, InstanceTable};
use dereth_client_net::client_session::ordering::ParkedBlobs;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
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
use {
    dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::EmitterOwner,
    dereth_scene::world_scene::WorldScene,
};

/// The level-up script type.
const PS_LEVEL_UP: u32 = 138;
/// A script type `PLAYER_TABLE` does **not** carry: the control. The player's table has 139 of
/// the 174 types; `90` is one of the 35 it lacks, and the assertion below reads that off the dat.
const PS_ABSENT: u32 = 90;

/// The physics-script table ID carried by every sampled player create.
const PLAYER_TABLE: u32 = 0x3400_0004;
/// The table every sampled non-player create in the corpus carries: creatures and items alike.
const CREATURE_TABLE: u32 = 0x3400_002B;
/// The Aluvian male body. It carries **no** `default_phstable_id`.
const PLAYER_SETUP: u32 = 0x0200_0001;
/// The id this file's synthetic player takes. The value does not matter; being the id `0xF746`
/// names does.
const PLAYER: ObjectId = ObjectId(0x5000_0F23);

fn table_of(store: &RetailDatStore, id: u32) -> dereth_assets::PhysicsScriptTable {
    let b = store
        .read_typed(DbType::PhysicsScriptTable, DataId(id))
        .expect("the table is shipped");
    dereth_assets::PhysicsScriptTable::decode_payload(DataId(id), &b).expect("it decodes")
}

// ---------------------------------------------------------------------------------------------
// The recording, read rather than transcribed.
// ---------------------------------------------------------------------------------------------

/// Every level-up in the selected recordings, as `(session, target, intensity, that session's
/// player, that player's `phstable_id`)`.
fn recorded_level_ups() -> Vec<(&'static str, ObjectId, f32, Option<ObjectId>, Option<u32>)> {
    let mut out = Vec::new();
    for s in [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "long-solo-play",
        "short-play-with-training",
    ] {
        let Ok(Some(c)) = dereth_client_net::client_session::testing::Corpus::load(s) else {
            continue;
        };
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
            // The player's own object is the one in the `0x5` id space; the corpus has exactly one
            // per session and it is always that session's own character.
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
            if u32::try_from(m.script_type).ok() == Some(PS_LEVEL_UP) {
                out.push((s, m.id, m.intensity, player, player_phs));
            }
        }
    }
    out
}

/// **Every recorded level-up is played on the session's own player, at intensity 1.0, and only
/// the player's table has the row.** The fixtures below are built on these facts.
#[test]
fn every_recorded_level_up_is_played_on_the_sessions_own_player() {
    let ups = recorded_level_ups();
    assert!(
        !ups.is_empty(),
        "the corpus carries a level-up (138); without one this file proves nothing"
    );
    for (s, target, intensity, player, phs) in &ups {
        assert_eq!(
            Some(*target),
            *player,
            "{s}: the level-up is addressed to the player's own object, not to a server object"
        );
        assert!(
            (intensity - 1.0).abs() < f32::EPSILON,
            "{s}: intensity is exactly 1.0"
        );
        assert_eq!(
            *phs,
            Some(PLAYER_TABLE),
            "{s}: the player's create carries the table the effect resolves through"
        );
    }

    // ...and that table is the only one of the two in the corpus that has the row.
    let s = retail_store();
    let player_rows = table_of(&s, PLAYER_TABLE).script_table[&PS_LEVEL_UP]
        .iter()
        .map(|r| r.modifier)
        .collect::<Vec<_>>();
    assert_eq!(
        player_rows,
        vec![1.0],
        "one row, at exactly the intensity the shard sends"
    );
    assert!(
        !table_of(&s, CREATURE_TABLE)
            .script_table
            .contains_key(&PS_LEVEL_UP),
        "the ordinary creature table has no level-up row, so the body's own table is the effect"
    );
    assert!(
        !table_of(&s, PLAYER_TABLE)
            .script_table
            .contains_key(&PS_ABSENT),
        "PLAYER_TABLE lacks script type 90, which is what makes the control below a control"
    );
}

// ---------------------------------------------------------------------------------------------
// The harness: the wire, and the frame.
// ---------------------------------------------------------------------------------------------

/// `dereth_client_net::client_session`'s WorldObjects gate, then the object stream.
struct Wire {
    table: InstanceTable,
    parked: ParkedBlobs,
    /// The player ID learned from `0xF746` and supplied to every later dispatch. Retaining it
    /// lets the gate use player-specific behavior rather than always receiving `None`.
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

    fn send<M: Message>(&mut self, stream: &mut ObjectStream, m: &M) -> bool {
        let body = dereth_protocol::write_body(m).expect("the message encodes");
        let im = IncomingMessage {
            opcode: M::OPCODE.0,
            queue: NetQueue::WorldObjects,
            sender: RecipientId::default(),
            blob_id: NetBlobId::default(),
            body,
        };
        let d = dispatch(&mut self.table, &mut self.parked, self.player, &im);
        match d.event {
            // `0xF746` is not a WorldObjects event: the gate answers `PlayerCreated`, which is what
            // gives the stream the ID that `play_script_type`'s player branch tests against.
            Some(e @ SessionEvent::PlayerCreated(_)) => {
                if let SessionEvent::PlayerCreated(id) = e {
                    self.player = Some(id);
                }
                stream.apply_event(&e, LocalTime(0.0));
                true
            }
            Some(e @ SessionEvent::WorldObject { .. }) => {
                if M::OPCODE == ItemCreateObject::OPCODE {
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

/// The scene with a local body, which is the configuration a logged-in client is always in.
fn scene(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
    let mut s = WorldScene::load(store, gpu, cfg()).expect("the landscape loads");
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    s.attach_character(store, &region, gpu)
        .expect("the body is created");
    s
}

/// One frame's device steps: sync, update, stream, draw.
fn step(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    stream: &mut ObjectStream,
    t: &mut f64,
) {
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
}

/// Log the synthetic player in: `0xF746` then his own `0xF745`, both through the gate.
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

/// The emitters the scene says belong to the **body**: `EmitterOwner::Body`.
fn body_emitters(scene: &WorldScene) -> Vec<dereth_scene::world_scene::EmitterDegrade> {
    scene
        .emitter_degrade_probe()
        .into_iter()
        .filter(|e| matches!(e.owner, EmitterOwner::Body))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The level-up on the body, and its control.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.effects.a-level-up-on-the-player-plays-on-his-own-body
///
/// **A `0xF755` level-up addressed to the player puts live emitters on the player's own body.**
///
/// Every link is the production one: the gate, `ObjectStream`, `WorldScene::sync_objects`'s script
/// drain, `WorldScene::play_script_type`'s **player** branch, and whatever the frame does with the
/// body's `MotionDriver` afterwards. Nothing here calls `play_script_type`.
///
/// The body's scripts are pumped by the character's update, so a script that resolves and queues
/// also executes its particle-creation hooks before the particles are ticked.
#[test]
fn a_level_up_on_the_players_own_object_reaches_the_bodys_emitters() {
    let store = retail_store();
    let mut gpu = test_gpu(640, 480);
    let mut scene = scene(&store, &mut gpu);
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let mut t = 0.0f64;

    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    log_in(&scene, &mut wire, &mut stream);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }

    // The two links before the effect, asserted separately so a partial failure names itself.
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
        "description application puts the wire's phstable on the body -- the Aluvian \
         male setup carries none, so without this a level-up resolves against nothing"
    );
    assert!(
        body_emitters(&scene).is_empty(),
        "the body starts with no emitters"
    );

    // The wire message, in the shape the recording carries.
    assert!(
        wire.send(
            &mut stream,
            &EffectsPlayScriptType {
                id: PLAYER,
                script_type: i32::try_from(PS_LEVEL_UP).expect("138"),
                intensity: 1.0,
            }
        ),
        "the gate delivers a `0xF755` about an object it knows"
    );
    for _ in 0..4 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }

    let live = body_emitters(&scene);
    assert!(
        !live.is_empty(),
        "the level-up's CreateParticleHooks must have put emitters on the body"
    );

    // The meshes are the shipped script's, not merely *a* mesh: one emitter per particle-creation
    // hook, each drawing the gfxobj its emitter info names.
    let script = table_of(&store, PLAYER_TABLE).script_table[&PS_LEVEL_UP][0].script_id;
    let sb = store
        .read_typed(DbType::PhysicsScript, script)
        .expect("the script is shipped");
    let ps = dereth_assets::PhysicsScript::decode_payload(script, &sb).expect("it decodes");
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
        "the shipped level-up script does create particles"
    );
    assert_eq!(
        live.len(),
        wanted.len(),
        "one emitter per particle-creation hook"
    );
    let meshes: Vec<DataId> = wanted
        .iter()
        .map(|info| {
            let b = store
                .read_typed(DbType::ParticleEmitter, *info)
                .expect("shipped emitter info");
            dereth_assets::ParticleEmitterInfo::decode_payload(*info, &b)
                .expect("it decodes")
                .hw_gfxobj_id
        })
        .collect();
    for e in &live {
        assert!(
            meshes.contains(&e.gfxobj),
            "emitter {} draws {:08X}, which no particle-creation hook of script {:08X} asks for",
            e.emitter,
            e.gfxobj.0,
            script.0
        );
    }
    assert!(
        live.iter().any(|e| e.live > 0),
        "and the emitters hold particles"
    );
}

/// Behaviour: objects.effects.a-level-up-on-the-player-plays-on-his-own-body
///
/// **The control.** A script type the player's table does not carry creates nothing on the body:
/// the body does not emit merely because a `0xF755` names the player.
#[test]
fn a_script_type_the_players_table_lacks_creates_nothing_on_the_body() {
    let store = retail_store();
    let mut gpu = test_gpu(640, 480);
    let mut scene = scene(&store, &mut gpu);
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let mut t = 0.0f64;

    step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    log_in(&scene, &mut wire, &mut stream);
    for _ in 0..3 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(wire.send(
        &mut stream,
        &EffectsPlayScriptType {
            id: PLAYER,
            script_type: i32::try_from(PS_ABSENT).expect("90"),
            intensity: 1.0,
        }
    ));
    for _ in 0..4 {
        step(&store, &mut gpu, &mut scene, &mut stream, &mut t);
    }
    assert!(
        body_emitters(&scene).is_empty(),
        "a type the table lacks resolves to nothing"
    );
}
