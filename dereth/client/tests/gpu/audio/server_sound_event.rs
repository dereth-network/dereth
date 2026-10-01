//! A server `0xF750 Effects_SoundEvent` reaches mixed samples: the object's sound table (the
//! descriptor's override, else the setup's default) plays the event's sound type at the message's
//! volume rather than the table row's; the queue drains with no audio device; and an event for an
//! object with no render position counts a miss. Fixture: encoded `WorldObject` bodies injected
//! into an `ObjectStream`, a synthetic object on setup 0x02000124 with table 0x20000014 and sound
//! type 47 placed in a training-dungeon cell, and the frame's audio seam invoked directly; a
//! software GPU builds the `WorldScene` meshes that supply the sound position, and no frame is
//! drawn. Missing dats or a missing software GPU fail the test.
//!
//! The route resolves the object's sound table and position, then plays the selected sound type
//! at the event's supplied volume. The render frame, not the listener position, is the sound
//! position. These tests do not replay the transport/session queue.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;

use dereth_client::audio::{Audio, SoundTrigger};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_protocol::objects::EffectsSoundEvent;
use dereth_protocol::types::{
    physicsdesc::flags, ObjDesc, PhysicsDesc, PositionWire, PublicWeenieDesc,
};
use dereth_protocol::{write_body, Opcode};
use dereth_render::device::Gpu;

use crate::common::software_gpu;

/// One second of interleaved stereo at the client's own primary-buffer rate.
const BLOCK: usize = (dereth_audio::MIX_RATE as usize) * 2;

// Pin the recorded opcode/setup/table/type as literals rather than deriving the expected opcode
// from Opcode::EFFECTS_SOUND_EVENT. The cell below is a chosen existing placement fixture and
// OBJ is a synthetic object ID; this is encoded test traffic, not a replayed raw capture.

/// `0xF750 Effects_SoundEvent`.
const F750: u32 = 0xF750;
/// The most-sounded setup in the recordings.
const SETUP: u32 = 0x0200_0124;
/// The recorded sound-table override paired with that setup.
const STABLE: u32 = 0x2000_0014;
/// Sound type paired with that setup/table.
const STYPE: i32 = 47;
/// The training dungeon's shot room: a cell that exists, so the object gets a real frame.
const CELL: u32 = 0x8602_01AD;
const LANDBLOCK: u16 = 0x8602;
const OBJ: u32 = 0x5000_0239;

/// The retail dats, or **fail**: a test that skips without them prints a green line
/// indistinguishable from a real pass.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// The peak absolute sample of each channel.
fn peaks(buf: &[f32]) -> (f32, f32) {
    let mut l = 0.0f32;
    let mut r = 0.0f32;
    for f in buf.as_chunks::<2>().0 {
        l = l.max(f[0].abs());
        r = r.max(f[1].abs());
    }
    (l, r)
}

/// Encode a create with recorded setup/table values and a chosen existing cell for a synthetic
/// object.
fn create() -> SessionEvent {
    let physicsdesc = PhysicsDesc {
        bitfield: flags::SETUP | flags::STABLE | flags::POSITION,
        setup_id: Some(SETUP),
        stable_id: Some(STABLE),
        position: Some(PositionWire {
            objcell_id: CELL,
            ..PositionWire::default()
        }),
        ..PhysicsDesc::default()
    };
    let body = write_body(&dereth_protocol::objects::ItemCreateObject(
        dereth_protocol::objects::ObjectCreatePayload {
            id: ObjectId(OBJ),
            objdesc: ObjDesc::default(),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        },
    ))
    .expect("encode");
    SessionEvent::WorldObject {
        opcode: Opcode::ITEM_CREATE_OBJECT,
        body,
    }
}

/// One `0xF750`, encoded by `dereth-protocol` and addressed by the **literal** opcode.
fn sound_event(volume: f32) -> SessionEvent {
    let body = write_body(&EffectsSoundEvent {
        id: ObjectId(OBJ),
        sound_type: STYPE,
        volume,
    })
    .expect("encode");
    assert_eq!(body.len(), 12, "id, SoundType, volume");
    SessionEvent::WorldObject {
        opcode: Opcode(F750),
        body,
    }
}

/// The scene, the stream and the audio, with the object created and synced.
struct Rig {
    store: Arc<RetailDatStore>,
    /// Held for the length of the test: `WorldScene` keeps device resources the object's meshes
    /// live in, so dropping the device early would take the scene's batches with it.
    _gpu: Gpu,
    scene: WorldScene,
    stream: ObjectStream,
    audio: Audio,
    anim: dereth_client::anim_assets::DatAnimAssets,
}

fn rig() -> Rig {
    let store = store();
    let mut gpu = software_gpu(320, 240);
    let cfg = SceneConfig {
        landblock: LANDBLOCK,
        character: false,
        land_radius: 1,
        scenery_radius: 0,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    stream.apply_event(&create(), LocalTime(0.0));
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the object syncs");
    assert_eq!(
        scene.server_object_count(),
        1,
        "the object reached the scene"
    );
    // Ambience off: what is measured here can then only be the sound event.
    let audio = Audio::new(
        dereth_audio::Prefs {
            ambient_enabled: false,
            ..dereth_audio::Prefs::default()
        },
        1,
        false,
    );
    let anim = dereth_client::anim_assets::DatAnimAssets::new(Arc::clone(&store));
    Rig {
        store,
        _gpu: gpu,
        scene,
        stream,
        audio,
        anim,
    }
}

impl Rig {
    /// Invoke the world_use_time audio seam used by App::frame, without running a full app frame.
    fn frame(&mut self, t: f64) {
        dereth_client::audio::world_use_time(
            Some(&mut self.audio),
            Some(&mut self.scene),
            &mut self.stream,
            &self.anim,
            &self.store,
            LocalTime(t),
        );
    }

    /// Put the listener at the object's render origin to avoid the 94 m drop threshold and
    /// obtain balanced channels. world_use_time publishes that camera position to audio.
    fn stand_on_the_object(&mut self) {
        let f = self
            .scene
            .server_object_frame(ObjectId(OBJ))
            .expect("the object has a render frame");
        self.scene.camera.position = f.origin;
    }

    fn mixed_peak(&mut self) -> (f32, f32) {
        let mut buf = vec![0.0f32; BLOCK];
        self.audio.mix(&mut buf);
        peaks(&buf)
    }
}

/// One encoded sound event produces one voice and nonzero samples in both channels.
/// ObjectStats::sound_events establishes decoding, WorldAudioStats::server_sounds establishes
/// draining, and zero misses rules out unresolved table/position/wave. A second frame with no
/// new event must not increment the drain count. This is body-to-mixer routing, not transport
/// playback.
#[test]
fn a_server_sound_event_reaches_the_mixer() {
    let mut r = rig();
    r.stand_on_the_object();
    r.stream.apply_event(&sound_event(1.0), LocalTime(0.0));
    assert_eq!(r.stream.stats.sound_events, 1, "the 0xF750 decoded");
    assert_eq!(
        r.stream.stats.unhandled, 0,
        "and was not counted as unhandled"
    );

    r.frame(0.0);
    let w = r.audio.world_stats();
    assert_eq!(w.server_sounds, 1, "the frame loop drained it");
    assert_eq!(
        w.server_sound_misses, 0,
        "no table, no position, or no wave"
    );
    assert_eq!(
        r.audio.active_voices(),
        1,
        "the server's sound started no voice"
    );

    let (l, right) = r.mixed_peak();
    assert!(
        l > 0.0 && right > 0.0,
        "a voice started and the mixed block was silent: {l} {right}"
    );
    eprintln!("0xF750 peak L {l:.4} R {right:.4}");

    // The queue is a queue: a second frame with nothing new plays nothing new.
    r.frame(0.1);
    assert_eq!(
        r.audio.world_stats().server_sounds,
        1,
        "the event was replayed"
    );
}

/// Behaviour: audio.sound-event.plays-the-objects-table-at-the-messages-volume
///
/// Compare the encoded event's quarter-volume route with a direct table trigger using the
/// table's own volume. The event arm tests routing of the supplied volume rather than building
/// that trigger directly; the control deliberately enters through SoundTrigger::Table.
///
/// Both seeded rigs use the same table, sound type, object and listener position. Read the first
/// table row's volume and require it to differ from 0.25. Both peaks must be positive, with the
/// message peak less than half the table-trigger peak, so silently using the row volume fails.
#[test]
fn the_messages_volume_overrides_the_rows() {
    // Entry 3, through the wire, at a quarter volume.
    let mut r = rig();
    r.stand_on_the_object();
    r.stream.apply_event(&sound_event(0.25), LocalTime(0.0));
    r.frame(0.0);
    assert_eq!(r.audio.world_stats().server_sound_misses, 0);
    assert_eq!(r.audio.active_voices(), 1);
    let msg_peak = r.mixed_peak().0;

    // Read the first row's volume from the resident DAT table as the stated discriminator.
    let table = DataId(STABLE);
    #[allow(clippy::cast_sign_loss)]
    // LINT-OK: `SoundType` is a `long` on the wire and an unsigned node key in the table.
    let stype = STYPE as u32;
    let row_volume = {
        let t = r
            .audio
            .assets()
            .table(table)
            .expect("the retail table is resident");
        let rows = dereth_audio::table::lookup(t, stype).expect("the table has this SoundType");
        assert!(
            !rows.is_empty(),
            "SoundType {STYPE} has rows in table {STABLE:08X}"
        );
        eprintln!(
            "table {STABLE:08X} SoundType {STYPE}: {} row(s)",
            rows.len()
        );
        rows[0].volume
    };
    assert!(
        (row_volume - 0.25).abs() > 1e-6,
        "the row's volume is 0.25, the same as the message's, so this test proves nothing"
    );

    // Entry 4, the animation `SoundTable` hook, at the row's own volume, same everything else.
    let mut q = rig();
    q.stand_on_the_object();
    q.frame(0.0);
    assert_eq!(
        q.audio.active_voices(),
        0,
        "no event: nothing should be playing yet"
    );
    // The residency `server_sound_events` would have done, done by hand because this half is not
    // driven by a message.
    let store = Arc::clone(&q.store);
    assert!(q.audio.load_sound_table(&store, table), "the table loads");
    q.audio.create_table_waves(&store, table);
    let at = q
        .scene
        .server_object_frame(ObjectId(OBJ))
        .expect("render frame")
        .origin;
    assert_eq!(
        q.audio.listener().pos,
        r.audio.listener().pos,
        "the same listener"
    );
    q.audio
        .play_trigger(SoundTrigger::Table { table, stype, at });
    assert_eq!(q.audio.active_voices(), 1);
    let row_peak = q.mixed_peak().0;

    eprintln!("row volume {row_volume} -> peak {row_peak:.5}; message 0.25 -> {msg_peak:.5}");
    assert!(
        row_peak > 0.0 && msg_peak > 0.0,
        "both entry points must be audible to compare"
    );
    // Quarter gain is about 12 dB down. Attenuation quantizes whole decibels, so allow a
    // generous half-peak bound while still rejecting equal peaks from using the row volume.
    assert!(
        msg_peak < row_peak * 0.5,
        "the message's volume did not reach the mixer: {msg_peak} vs row {row_peak}"
    );
}

/// Preserve the descriptor's sound-table override instead of losing it before playback. The
/// recorded creates of sounded objects carry one. Confirm the selected setup's default differs,
/// so silently substituting it is detectable.
///
/// The production route can fall back to a setup default when no override is present. The
/// second create here only checks that no override is stored; it does not sync or play that
/// object, so this test does not independently exercise successful fallback playback.
#[test]
fn the_sound_table_is_the_descriptors_override_and_then_the_setups_default() {
    let mut r = rig();
    let p = r.stream.presence(ObjectId(OBJ)).expect("the object exists");
    assert_eq!(
        p.sound_table,
        Some(DataId(STABLE)),
        "the descriptor's override is kept"
    );
    assert_eq!(p.setup_id, Some(DataId(SETUP)));

    // The setup's own default, read out of the dat, is a *different* table, so a build that used
    // it would be reading the wrong rows rather than the same ones by luck.
    let setup_default = {
        use dereth_animation::data::AnimAssets as _;
        r.anim
            .setup(DataId(SETUP))
            .and_then(|s| s.default_sound_table)
    };
    eprintln!(
        "setup {SETUP:08X} default_stable_id {setup_default:08X?}, server override {STABLE:08X}"
    );
    assert_ne!(
        setup_default,
        Some(DataId(STABLE)),
        "the override and the setup default coincide, so this test proves nothing"
    );

    // These eight setup IDs are a fixed sounded population from the recordings. Read their
    // defaults from DAT; this fixed list does not recount every setup sounded by the corpus.
    const SOUNDED_SETUPS: [u32; 8] = [
        0x0200_0001,
        0x0200_004E,
        0x0200_007C,
        0x0200_0124,
        0x0200_024F,
        0x0200_07CC,
        0x0200_1121,
        0x0200_1253,
    ];
    let mut with_default = 0;
    let mut with_motion_table = 0;
    for sid in SOUNDED_SETUPS {
        use dereth_animation::data::AnimAssets as _;
        let sd = r.anim.setup(DataId(sid)).expect("the setup decodes");
        eprintln!(
            "setup {sid:08X} default_stable_id {:08X?} default_mtable_id {:08X?}",
            sd.default_sound_table, sd.default_motion_table
        );
        with_default += usize::from(sd.default_sound_table.is_some());
        with_motion_table += usize::from(sd.default_motion_table.is_some());
    }
    // Require no default motion table in these eight setups as well. A bounded scan of IDs
    // 1..0x0400 then supplies a positive sound-table decode control. Failed setup lookups are
    // skipped. Motion-table positives and total decoded records are reported, but only a
    // positive sound-table count is asserted by this scan.
    assert_eq!(
        with_motion_table, 0,
        "the same eight also carry no default motion table"
    );
    let mut scanned = 0usize;
    let mut any_stable = 0usize;
    let mut any_mtable = 0usize;
    for raw in 1u32..0x0400 {
        use dereth_animation::data::AnimAssets as _;
        let Some(sd) = r.anim.setup(DataId(0x0200_0000 | raw)) else {
            continue;
        };
        scanned += 1;
        any_stable += usize::from(sd.default_sound_table.is_some());
        any_mtable += usize::from(sd.default_motion_table.is_some());
    }
    eprintln!(
        "{scanned} setups scanned: {any_stable} with a default sound table, \
         {any_mtable} with a default motion table"
    );
    assert!(
        any_stable > 0,
        "no decoded setup in the scanned ID range has a default sound table"
    );
    assert_eq!(
        with_default, 0,
        "the eight selected setups have no default sound table; their recorded sound events \
         require the descriptor override"
    );

    // A second encoded create omits the override; check stored absence, without playing it.
    let physicsdesc = PhysicsDesc {
        bitfield: flags::SETUP | flags::POSITION,
        setup_id: Some(SETUP),
        position: Some(PositionWire {
            objcell_id: CELL,
            ..PositionWire::default()
        }),
        ..PhysicsDesc::default()
    };
    let body = write_body(&dereth_protocol::objects::ItemCreateObject(
        dereth_protocol::objects::ObjectCreatePayload {
            id: ObjectId(OBJ + 1),
            objdesc: ObjDesc::default(),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        },
    ))
    .expect("encode");
    r.stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(0.0),
    );
    assert_eq!(
        r.stream
            .presence(ObjectId(OBJ + 1))
            .expect("exists")
            .sound_table,
        None,
        "no override means nothing is stored; the setup's default is resolved at play time"
    );
}

/// Drain queued events even when the audio option passed to world_use_time is None.
/// Retaining them would grow an unplayed queue. This test still constructs the GPU rig
/// and its Audio value; it disables consumption through the None argument, not device removal.
#[test]
fn the_sound_event_queue_is_drained_without_an_audio_device() {
    let mut r = rig();
    for i in 0..8 {
        r.stream
            .apply_event(&sound_event(1.0), LocalTime(f64::from(i)));
    }
    assert_eq!(r.stream.stats.sound_events, 8);
    dereth_client::audio::world_use_time(
        None,
        Some(&mut r.scene),
        &mut r.stream,
        &r.anim,
        &r.store,
        LocalTime(1.0),
    );
    assert!(
        r.stream.take_sound_events().is_empty(),
        "the queue grew instead of being drained"
    );
}

/// An event for an ID absent from both this stream and scene is drained and counted as a miss,
/// with no voice started. The fixture ID has never been created, so no render position can be
/// resolved. This distinguishes missing-object handling from broken decoding; it does not
/// establish that every culled or currently undrawn object lacks a usable sound position.
#[test]
fn a_sound_event_for_an_undrawn_object_is_counted_as_a_miss() {
    let mut r = rig();
    let body = write_body(&EffectsSoundEvent {
        id: ObjectId(0x5000_0999),
        sound_type: STYPE,
        volume: 1.0,
    })
    .expect("encode");
    r.stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode(F750),
            body,
        },
        LocalTime(0.0),
    );
    r.frame(0.0);
    let w = r.audio.world_stats();
    assert_eq!(w.server_sounds, 1, "it was taken off the queue");
    assert_eq!(w.server_sound_misses, 1, "and could not be placed");
    assert_eq!(r.audio.active_voices(), 0);
}

/// A server object's animation sound hooks (a door's open and close, a person's footsteps) play
/// a sound type from the object's own table, and that table is the one its description names
/// before its setup's default: this object's setup names none, so without the description's table
/// every such hook on it is silent.
/// Behaviour: audio.hooks.a-server-objects-sound-type-hooks-play-from-its-described-table
#[test]
fn a_server_objects_hooks_take_the_sound_table_its_description_names() {
    let r = rig();
    let o = r
        .scene
        .objects
        .get(&ObjectId(OBJ))
        .expect("the object is in the world");
    assert_eq!(
        o.sound_table,
        Some(DataId(STABLE)),
        "the description's table, not the setup's (absent) default"
    );
}
