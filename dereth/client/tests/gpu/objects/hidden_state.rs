//! `HIDDEN_PS` edges on an object's state word drive the teleport's hide and unhide: the shimmer
//! the hide starts runs until the server clears `HIDDEN_PS`, a hidden player drops out of attack
//! detection, and both edges remove queued link animations on the local body and on remote
//! objects while keeping the cycle. At login the player's own create carries `HIDDEN_PS`, so his
//! body starts hidden and is drawn again only after the server clears it and the unhide ramp runs.
//!
//! # The teleport on the wire
//!
//! A teleport is **three** messages on the player's own object, and only the first is a
//! `0xF755`:
//!
//! | # | wire | what it does |
//! |---|---|---|
//! | 1 | `0xF755 Effects_PlayScriptType` the hide script type (116) | plays script `0x33000332`: `Transparent 0 -> 1` over 0.75 s and **14 `CreateParticleHook`s**, emitter ids `1000..=1013`, on 14 body parts |
//! | 2 | `0xF74B Item_SetState` with `HIDDEN_PS` **set** (`0x00404410`) | applying the hidden edge plays the hidden script type (118), script `0x33000331`, with the same 14 ids again |
//! | 3 | `0xF74B Item_SetState` with `HIDDEN_PS` **clear** (`0x00400408`), ~6.7 s later | applying the visible edge plays **the unhide script type (117)**, script `0x3300032F`: **14 `StopParticleHook`s on exactly those ids** and `Transparent 1 -> 0` |
//!
//! **The hide emitters cannot end themselves.** Every one of the 14 emitter infos the script
//! names carries `total_particles == 0` **and** `total_seconds == 0.0`, which is
//! `ParticleEmitter`'s "infinite": the automatic stop condition never latches, so no intrinsic
//! lifetime reaps it. Of the 4,248 shipped `PhysicsScript`s, exactly **one** has a
//! `DESTROY_PARTICLE` hook and **15** have `STOP_PARTICLE` hooks; 14 of the latter are in
//! `0x3300032F`, the unhide. The terminating script is triggered by the state word, not by a
//! separately sent script message or a timer.
//!
//! The hide and unhide edges play integer script types:
//!
//! ```text
//! hide:   resolve script type 118 (0x76, hidden) at intensity 1.0;
//!         play the returned script on the object;
//! unhide: resolve script type 117 (0x75, unhide) at intensity 1.0;
//!         play the returned script on the object;
//! // Both intensity arguments are float bit pattern 0x3F800000.
//! ```
//!
//! Table lookup followed by playback at intensity `1.0` is
//! `dereth_animation::MotionDriver::play_script_type`.
//!
//! # Observing a lifetime
//!
//! The lifetime assertions compare two points in time within **one** run of the real `App::frame`:
//! the emitters are **present** after the hide and **gone** N frames after the unhide. The control
//! withholds message 3 and still has all 14 emitters after the same number of frames, so the
//! client does not simply reap every emitter after a while.
//!
//! # At login
//!
//! Applying a create's physics description passes its state word through the same state
//! application a live `Item_SetState 0xF74B` uses, against the constructor's previous state
//! `0x00400C08`, so a create carrying `HIDDEN_PS` plays the hidden script (type `0x76`). The
//! player's scripts each carry one `TransparentHook` (type 20): the hide ramps `0.0 -> 1.0` over
//! 0.75 s, the hidden script sets `1.0` at once, the unhide ramps `1.0 -> 0.0` over 0.75 s. Part
//! translucency `1.0` sets NoDraw, so only the unhide ramp makes a hidden body visible again;
//! visibility is counted with the part-drawing no-draw mask `(draw_state & 1)` the draw loop uses.
//!
//! Fixture: the retail dats' player script table and emitters, the recorded sessions' teleports
//! and logins, and a headless `App` with the UI up; an `App` that cannot be built fails the test.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use super::common::retail_store;
use dereth_scene::world_scene::SceneReads;
use std::collections::BTreeSet;
use std::sync::Arc;

use dereth_animation::motion::MovementParameters;
use dereth_animation::MotionCommand;
use dereth_assets::Decode;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_protocol::objects::{
    EffectsPlayScriptType, ItemCreateObject, ItemSetState, ObjectCreatePayload,
};
use dereth_protocol::types::{PhysicsDesc, PhysicsEventStamp, PublicWeenieDesc};
use dereth_protocol::Message;
use {dereth_client::app::App, dereth_client_runtime::config::Config};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::EmitterOwner};

/// Protocol script types for the hide, unhide, hidden and materialise effects.
const PS_HIDE: u32 = 116;
const PS_UNHIDE: u32 = 117;
const PS_HIDDEN: u32 = 118;
const PS_CREATE: u32 = 88;

/// The hidden-state bit, set by OR-ing 0x4000 into the physics state word.
const HIDDEN_PS: u32 = 0x0000_4000;

/// The two state words every recorded teleport in the corpus uses, as literals. A test that read
/// them through the same symbol the client writes them through could not detect a wrong constant.
const TELEPORT_HIDE_STATE: u32 = 0x0040_4410;
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;

/// The physics-script table carried by sampled player creates.
const PLAYER_TABLE: u32 = 0x3400_0004;
/// The motion table on every recorded ordinary Aluvian player create.
const PLAYER_MTABLE: u32 = 0x0900_0001;
/// The Aluvian male body, which carries no `default_phstable_id` of its own.
const PLAYER_SETUP: u32 = 0x0200_0001;
const PLAYER: ObjectId = ObjectId(0x5000_0F37);
const REMOTE: ObjectId = ObjectId(0x5000_1F37);

// ---------------------------------------------------------------------------------------------
// The oracles: the recording and the shipped dats, asserted before anything is built on them.
// ---------------------------------------------------------------------------------------------

fn table(store: &RetailDatStore, id: u32) -> dereth_assets::PhysicsScriptTable {
    let b = store
        .read_typed(DbType::PhysicsScriptTable, DataId(id))
        .expect("the table is shipped");
    dereth_assets::PhysicsScriptTable::decode_payload(DataId(id), &b).expect("it decodes")
}

fn script_of(store: &RetailDatStore, ty: u32) -> dereth_assets::PhysicsScript {
    let t = table(store, PLAYER_TABLE);
    let rows = &t.script_table[&ty];
    assert_eq!(rows.len(), 1, "type {ty} has one row in the player's table");
    assert!(
        (rows[0].modifier - 1.0).abs() < f32::EPSILON,
        "at exactly the 1.0 the shard sends"
    );
    let id = rows[0].script_id;
    let b = store
        .read_typed(DbType::PhysicsScript, id)
        .expect("the script is shipped");
    dereth_assets::PhysicsScript::decode_payload(id, &b).expect("it decodes")
}

/// The emitter infos a script's `CreateParticleHook`s name, with their emitter ids.
fn creates(s: &dereth_assets::PhysicsScript) -> Vec<(u32, DataId)> {
    s.script_data
        .iter()
        .filter_map(|st| match &st.hook.data {
            dereth_assets::hook::HookData::CreateParticle {
                emitter_info_id,
                emitter_id,
                ..
            } => Some((*emitter_id, *emitter_info_id)),
            _ => None,
        })
        .collect()
}

/// The emitter ids a script's `StopParticleHook`s name. `HookData::Particle` is shared by
/// `DESTROY_PARTICLE` (14) and `STOP_PARTICLE` (15), so the type is what separates them.
fn stops(s: &dereth_assets::PhysicsScript) -> BTreeSet<u32> {
    s.script_data
        .iter()
        .filter_map(|st| match (st.hook.hook_type, &st.hook.data) {
            (15, dereth_assets::hook::HookData::Particle { emitter_id }) => Some(*emitter_id),
            _ => None,
        })
        .collect()
}

fn emitter_info(store: &RetailDatStore, id: DataId) -> dereth_assets::ParticleEmitterInfo {
    let b = store
        .read_typed(DbType::ParticleEmitter, id)
        .expect("shipped emitter info");
    dereth_assets::ParticleEmitterInfo::decode_payload(id, &b).expect("it decodes")
}

/// Behaviour: objects.hidden.the-hide-shimmer-lasts-until-the-shard-unhides
///
/// **The hide script cannot end itself, and the unhide script is the only thing that ends it.** The
/// materialise script, by contrast, is finite.
#[test]
fn the_hide_shimmer_is_immortal_and_the_unhide_is_the_only_thing_that_stops_it() {
    let s = retail_store();

    let hide = script_of(&s, PS_HIDE);
    let hidden = script_of(&s, PS_HIDDEN);
    let unhide = script_of(&s, PS_UNHIDE);

    let hide_creates = creates(&hide);
    assert_eq!(
        hide_creates.len(),
        14,
        "the hide script creates fourteen emitters"
    );
    for (eid, info) in &hide_creates {
        assert!(
            *eid >= 1000,
            "every hide emitter carries an explicit id, not the 0 that allocates"
        );
        let i = emitter_info(&s, *info);
        assert_eq!(
            (i.total_particles, i.total_seconds),
            (0, 0.0),
            "emitter {info:?} is infinite -- the automatic stop condition can never latch on it, so \
             nothing intrinsic to the effect ends it"
        );
    }

    // The hidden script re-creates exactly the same ids, which is why the pair is idempotent.
    assert_eq!(
        creates(&hidden)
            .iter()
            .map(|(e, _)| *e)
            .collect::<BTreeSet<_>>(),
        hide_creates
            .iter()
            .map(|(e, _)| *e)
            .collect::<BTreeSet<_>>(),
        "the hidden script re-creates the same emitter ids -- emitter creation replaces"
    );

    // ...and the unhide stops exactly those, and creates nothing.
    assert_eq!(
        stops(&unhide),
        hide_creates
            .iter()
            .map(|(e, _)| *e)
            .collect::<BTreeSet<_>>(),
        "the unhide script stops exactly the ids the hide script created"
    );
    assert!(creates(&unhide).is_empty(), "the unhide creates nothing");

    // The contrast: the *materialise* proper is finite and ends itself.
    let create = script_of(&s, PS_CREATE);
    let create_creates = creates(&create);
    assert!(
        !create_creates.is_empty(),
        "the materialise script does create particles"
    );
    for (_, info) in &create_creates {
        let i = emitter_info(&s, *info);
        assert!(
            i.total_particles > 0 || i.total_seconds > 0.0,
            "materialise emitters are finite, which is why that effect needs no message to end it"
        );
    }
}

/// **The recording says the teleport is a hide and a later unhide, both on the player's own
/// object, and the unhide is a `0xF74B` and not a `0xF755`.**
#[test]
fn every_recorded_teleport_hides_the_player_and_a_later_set_state_unhides_him() {
    let mut hides = 0usize;
    let mut pairs = 0usize;
    let mut unhide_script_messages = 0usize;
    for name in [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "login-account-booted",
        "ddd-interrogation-only",
        "long-solo-play",
        "short-play-with-training",
    ] {
        let Ok(Some(c)) = Corpus::load(name) else {
            continue;
        };
        // `(t, state)` for every `0xF74B` the server sent about a player object.
        let mut states: Vec<(f64, u32)> = Vec::new();
        for row in &c.blobs {
            if row.dir != Direction::ServerToClient {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let t = row.t_rel_micros as f64 / 1e6;
            if row.opcode == ItemSetState::OPCODE.0 {
                let Ok(m) = dereth_protocol::read_body_padded::<ItemSetState>(&row.payload[4..])
                else {
                    continue;
                };
                if m.id.0 >> 28 == 5 {
                    states.push((t, m.state));
                }
            }
            if row.opcode == EffectsPlayScriptType::OPCODE.0 {
                let Ok(m) =
                    dereth_protocol::read_body_padded::<EffectsPlayScriptType>(&row.payload[4..])
                else {
                    continue;
                };
                if u32::try_from(m.script_type).ok() == Some(PS_UNHIDE) {
                    unhide_script_messages += 1;
                }
            }
        }
        for (i, (t, s)) in states.iter().enumerate() {
            if s & HIDDEN_PS == 0 {
                continue;
            }
            hides += 1;
            assert_eq!(
                *s, TELEPORT_HIDE_STATE,
                "{name}: the recorded teleport hide word"
            );
            let after = states[i + 1..].iter().find(|(_, n)| n & HIDDEN_PS == 0);
            let (ut, us) = after.expect("a later 0xF74B clears HIDDEN_PS");
            assert_eq!(
                *us, TELEPORT_UNHIDE_STATE,
                "{name}: the recorded unhide word"
            );
            assert!(
                (0.5..30.0).contains(&(ut - t)),
                "{name}: the shimmer lasts {:.2}s, which is a lifetime and not a frame",
                ut - t
            );
            pairs += 1;
        }
    }
    assert!(
        hides >= 2,
        "the corpus carries recorded teleports; without one this file proves nothing"
    );
    assert_eq!(
        pairs, hides,
        "every recorded hide is closed by a later unhide"
    );
    assert_eq!(
        unhide_script_messages, 0,
        "no server in the corpus sends the unhide as a 0xF755 -- the unhide is the state word, \
         which is why a client that only listens to 0xF755 shimmers for ever"
    );
}

// ---------------------------------------------------------------------------------------------
// At login: the hide the player's own create plays, and the ramp that undoes it.
// ---------------------------------------------------------------------------------------------

/// `TransparentHook`'s hook type in a shipped `PhysicsScript`.
const TRANSPARENT_HOOK: u32 = 20;

/// The one `TransparentHook` in a script on the player's table, as `(start, end, time)`.
fn transparent_of(store: &RetailDatStore, ty: u32) -> (f32, f32, f32) {
    let b = store
        .read_typed(DbType::PhysicsScriptTable, DataId(PLAYER_TABLE))
        .expect("the player's table is shipped");
    let t = dereth_assets::PhysicsScriptTable::decode_payload(DataId(PLAYER_TABLE), &b)
        .expect("it decodes");
    let rows = &t.script_table[&ty];
    assert_eq!(rows.len(), 1, "type {ty} has one row in the player's table");
    let sb = store
        .read_typed(DbType::PhysicsScript, rows[0].script_id)
        .expect("shipped script");
    let sc = dereth_assets::PhysicsScript::decode_payload(rows[0].script_id, &sb).expect("decodes");
    let mut found = Vec::new();
    for st in &sc.script_data {
        if st.hook.hook_type == TRANSPARENT_HOOK {
            if let dereth_assets::hook::HookData::Ramp { start, end, time } = st.hook.data {
                found.push((start, end, time));
            }
        }
    }
    assert_eq!(
        found.len(),
        1,
        "type {ty} carries exactly one TransparentHook"
    );
    found[0]
}

/// **The hidden script sets translucency at once and the unhide ramps it back over 0.75 s.** A
/// client that applies only the immediate form can make the body invisible and can never make it
/// visible again.
#[test]
fn the_hidden_script_is_immediate_and_only_the_unhide_ramp_can_undo_it() {
    let s = retail_store();

    let hidden = transparent_of(&s, PS_HIDDEN);
    assert_eq!(
        hidden,
        (1.0, 1.0, 0.0),
        "the hidden script's TransparentHook is `time == 0.0`, so translucency setting takes its \
         immediate arm and physics-part translucency sets NoDraw at once"
    );

    let unhide = transparent_of(&s, PS_UNHIDE);
    assert_eq!(
        unhide,
        (1.0, 0.0, 0.75),
        "the unhide's TransparentHook is a 0.75 s ramp back to opaque -- a timed interpolation hook, and the only \
         thing in the client that ever clears the NoDraw the hidden script set"
    );

    let hide = transparent_of(&s, PS_HIDE);
    assert_eq!(
        hide,
        (0.0, 1.0, 0.75),
        "and the hide fades out over the same 0.75 s, which is why the pair reads as a fade and \
         not as a switch"
    );
    assert!(
        unhide.2 >= dereth_primitives::num::consts::EPSILON
            && hide.2 >= dereth_primitives::num::consts::EPSILON,
        "both ramps are above the timed-translucency setter's 0.0002 threshold"
    );
}

/// **Every recorded login creates the player with `HIDDEN_PS`, and a bare `Item_SetState 0xF74B`
/// clears it seconds later.**
///
/// This is the wire sequence the visibility tests below replay.
/// Without this the test below would be asserting against an invented wire sequence.
#[test]
fn every_recorded_login_creates_the_player_hidden_and_unhides_him_later() {
    let mut logins = 0usize;
    for name in [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "login-account-booted",
        "ddd-interrogation-only",
        "long-solo-play",
        "short-play-with-training",
    ] {
        let Ok(Some(c)) = Corpus::load(name) else {
            continue;
        };
        // The first `0xF745` naming a player object is his own create: the login one.
        let mut created: Option<(f64, u32)> = None;
        let mut unhide_after: Option<f64> = None;
        for row in &c.blobs {
            if row.dir != Direction::ServerToClient {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let t = row.t_rel_micros as f64 / 1e6;
            if created.is_none() && row.opcode == ItemCreateObject::OPCODE.0 {
                let Ok(m) =
                    dereth_protocol::read_body_padded::<ItemCreateObject>(&row.payload[4..])
                else {
                    continue;
                };
                if m.0.id.0 >> 28 == 5 && m.0.physicsdesc.phstable_id == Some(PLAYER_TABLE) {
                    created = Some((t, m.0.physicsdesc.state));
                }
            }
            if let Some((ct, _)) = created {
                if unhide_after.is_none() && row.opcode == ItemSetState::OPCODE.0 && t > ct {
                    let Ok(m) =
                        dereth_protocol::read_body_padded::<ItemSetState>(&row.payload[4..])
                    else {
                        continue;
                    };
                    if m.id.0 >> 28 == 5 && m.state & HIDDEN_PS == 0 {
                        assert_eq!(m.state, TELEPORT_UNHIDE_STATE, "{name}: the unhide word");
                        unhide_after = Some(t - ct);
                    }
                }
            }
        }
        let Some((_, state)) = created else { continue };
        assert_eq!(
            state, TELEPORT_HIDE_STATE,
            "{name}: the player's login create carries the same word the teleport hide does -- \
             description application hands it to state application like any other"
        );
        let gap = unhide_after.expect("a later 0xF74B clears HIDDEN_PS after the login create");
        assert!(
            (0.5..60.0).contains(&gap),
            "{name}: the login fade-in lasts {gap:.2}s, which is a lifetime and not a frame"
        );
        logins += 1;
    }
    assert!(
        logins >= 3,
        "the corpus carries recorded logins; without them this file proves nothing"
    );
}

// ---------------------------------------------------------------------------------------------
// The harness: the real `App::frame`.
// ---------------------------------------------------------------------------------------------

fn app_with_object_state_draw(object_state_draw: bool) -> App {
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        preferences_file: std::env::temp_dir().join("dere-hidden-state-not-created/prefs.ini"),
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).unwrap_or_else(|e| panic!("a headless App with a device: {e}"));
    app.start_shell().expect("the UI comes up");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: true,
        object_state_draw,
        ..SceneConfig::default()
    })
    .expect("the static scene loads");
    assert!(app.frame());
    app
}

fn app() -> App {
    app_with_object_state_draw(true)
}

fn send<M: Message>(app: &mut App, m: &M) {
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: M::OPCODE,
            body: dereth_protocol::write_body(m).expect("encodes"),
        },
        LocalTime(0.0),
    );
}

/// The player's own `0xF745`, carrying the state word the caller names.
fn create_player(app: &mut App, state: u32) {
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(PLAYER), LocalTime(0.0));
    let (block, origin) = {
        let s = app.world_scene().expect("a scene");
        (
            s.viewer_block().expect("a resident block"),
            s.character.as_ref().expect("a body").render_frame().origin,
        )
    };
    send(
        app,
        &ItemCreateObject(ObjectCreatePayload {
            id: PLAYER,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                    | dereth_protocol::types::physicsdesc::flags::SETUP
                    | dereth_protocol::types::physicsdesc::flags::PETABLE,
                state,
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
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        }),
    );
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(
        app.world_state()
            .expect("a scene")
            .character
            .as_ref()
            .expect("a body")
            .driver()
            .script_table,
        Some(DataId(PLAYER_TABLE)),
        "description application puts the wire's phstable on the body"
    );
}

/// Log the synthetic player in, with the `phstable_id` every recorded player create carries
/// and no state word of its own.
fn log_in(app: &mut App) {
    create_player(app, 0);
}

fn create_remote_player(app: &mut App) {
    let (cell, origin) = {
        let s = app.world_scene().expect("a scene");
        let block = s.viewer_block().expect("a resident block");
        (
            (u32::try_from(block.0).expect("x") << 24)
                | (u32::try_from(block.1).expect("y") << 16)
                | 1,
            s.character.as_ref().expect("a body").render_frame().origin,
        )
    };
    send(
        app,
        &ItemCreateObject(ObjectCreatePayload {
            id: REMOTE,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                    | dereth_protocol::types::physicsdesc::flags::SETUP
                    | dereth_protocol::types::physicsdesc::flags::MTABLE
                    | dereth_protocol::types::physicsdesc::flags::PETABLE,
                setup_id: Some(PLAYER_SETUP),
                mtable_id: Some(PLAYER_MTABLE),
                phstable_id: Some(PLAYER_TABLE),
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: cell,
                    frame: dereth_protocol::types::Frame {
                        origin: origin.into(),
                        orientation: dereth_primitives::Quat::IDENTITY.into(),
                    },
                }),
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc {
                name: "Remote hidden-state player".into(),
                obj_type: 0x10,
                bitfield: 0x08,
                ..PublicWeenieDesc::default()
            },
        }),
    );
    assert!(app.frame(), "the encoded remote create reaches the scene");
    let scene = app.world_scene().expect("a scene");
    assert!(
        scene.server_object_frame(REMOTE).is_some(),
        "the remote has a retained render frame"
    );
    let sequence = scene
        .server_object_sequence(REMOTE)
        .expect("the remote has a real MotionDriver");
    assert!(
        sequence.has_anims(),
        "the encoded recorded motion table animates the remote"
    );
    assert_eq!(
        sequence.first_cyclic(),
        Some(0),
        "entering the world installed the table's cycle"
    );
}

fn queue_remote_wave_link(app: &App) -> usize {
    let weak = app
        .world_state()
        .expect("a scene")
        .server_object_motion_lifetime(REMOTE)
        .expect("the accepted 0xF745 owns a SceneObject MotionDriver");
    let driver = weak
        .upgrade()
        .expect("the retained remote driver remains live");
    let mut driver = driver.borrow_mut();
    assert_eq!(
        driver.do_interpreted_motion(MotionCommand::WAVE, &MovementParameters::default()),
        0,
        "the shipped player table accepts Wave on the remote driver"
    );
    let links = driver
        .sequence
        .first_cyclic()
        .expect("Wave queued its cyclic successor");
    assert!(
        links > 0,
        "Wave queued at least one link node before its cycle"
    );
    assert!(
        driver.movement.motions_pending(),
        "the interpreter ledger holds the action"
    );
    assert!(
        driver.motion_table.pending_len() > 0,
        "the table ledger holds the action"
    );
    links
}

fn assert_remote_cycle_only(app: &App, old_links: usize) {
    let scene = app.world_scene().expect("a scene");
    assert!(
        scene.server_object_frame(REMOTE).is_some(),
        "hidden retains the remote render frame"
    );
    let seq = scene
        .server_object_sequence(REMOTE)
        .expect("remote sequence");
    assert_eq!(
        seq.first_cyclic(),
        Some(0),
        "link-animation removal removed the {old_links} remote link node(s), not the cycle"
    );
    assert!(
        seq.has_anims(),
        "the remote cyclic pose remains visible/reachable"
    );
    assert_eq!(scene.server_object_motions_pending(REMOTE), Some(0));
    assert_eq!(scene.server_object_table_motions_pending(REMOTE), Some(0));
}

/// The teleport's first two messages, in the order the recording carries them.
fn teleport_hide(app: &mut App) {
    send(
        app,
        &EffectsPlayScriptType {
            id: PLAYER,
            script_type: i32::try_from(PS_HIDE).expect("116"),
            intensity: 1.0,
        },
    );
    send(
        app,
        &ItemSetState {
            id: PLAYER,
            state: TELEPORT_HIDE_STATE,
            timestamps: PhysicsEventStamp {
                instance: 0,
                event: 1,
            },
        },
    );
}

/// The teleport's third message — the one that ends it.
fn teleport_unhide(app: &mut App) {
    send(
        app,
        &ItemSetState {
            id: PLAYER,
            state: TELEPORT_UNHIDE_STATE,
            timestamps: PhysicsEventStamp {
                instance: 0,
                event: 2,
            },
        },
    );
}

fn send_player_state(app: &mut App, state: u32, event: u16) {
    send_player_state_for(app, PLAYER, state, event);
}

fn send_player_state_for(app: &mut App, id: ObjectId, state: u32, event: u16) {
    send(
        app,
        &ItemSetState {
            id,
            state,
            timestamps: PhysicsEventStamp { instance: 0, event },
        },
    );
}

fn queue_wave_link(app: &mut App) -> usize {
    let scene = app.world_scene_mut().expect("a scene");
    let c = scene.character.as_ref().expect("the player body");
    let mut d = c.driver_mut();
    assert_eq!(
        d.do_interpreted_motion(MotionCommand::WAVE, &MovementParameters::default()),
        0,
        "the shipped player table accepts Wave"
    );
    let links = d
        .sequence
        .first_cyclic()
        .expect("Wave queued its cyclic successor");
    assert!(
        links > 0,
        "Wave queued at least one link node before its cycle"
    );
    assert!(
        d.movement.motions_pending(),
        "the interpreter ledger holds the action"
    );
    assert!(
        d.motion_table.pending_len() > 0,
        "the table ledger holds the action"
    );
    assert_eq!(
        d.movement
            .interp
            .interpreted_state
            .actions
            .iter()
            .map(|n| n.action)
            .collect::<Vec<_>>(),
        vec![MotionCommand::WAVE],
        "the interpreted state holds the same action"
    );
    links
}

fn assert_only_cycle_remains(app: &App, old_links: usize) {
    let scene = app.world_scene().expect("a scene");
    let c = scene.character.as_ref().expect("the player body");
    let d = c.driver();
    assert_eq!(
        d.sequence.first_cyclic(),
        Some(0),
        "link-animation removal removed the {old_links} node(s) before the cycle, not the cycle"
    );
    assert!(
        d.sequence.has_anims(),
        "the cyclic pose survives the link-only removal"
    );
    assert!(
        !d.movement.motions_pending(),
        "the interpreter ledger was completed"
    );
    assert_eq!(
        d.motion_table.pending_len(),
        0,
        "the table ledger was completed"
    );
    assert!(
        d.movement.interp.interpreted_state.actions.is_empty(),
        "AnimationDone routed the removed action out of the interpreted state"
    );
}

fn body_emitters(app: &App) -> usize {
    app.world_scene()
        .expect("a scene")
        .emitter_degrade_probe()
        .into_iter()
        .filter(|e| matches!(e.owner, EmitterOwner::Body))
        .count()
}

/// Ask the production physics world's attack search whether a dynamic object placed exactly on
/// the player finds him. The attack search rejects a target carrying `IGNORE_COLLISIONS_PS`; using
/// the same real scene body before, during and after the hide makes that observable rather than
/// merely reading the state bit back.
fn attack_detects_player(app: &mut App) -> bool {
    let mut scene = app.world_scene_mut().expect("a scene");
    let c = scene.character.as_mut().expect("the player body");
    let player = c.handle;
    let (geometry, position) = {
        let body = c.world.get(player).expect("the player body is registered");
        (Arc::clone(&body.geometry), body.position)
    };
    let probe = c.world.create(ObjectId(0x6000_0F37), geometry, true);
    c.world.force_into_cell(probe, &position);
    let cone = dereth_physics::detect::AttackCone {
        part_index: -1,
        left: (-0.5, 0.866_025_4),
        right: (0.5, 0.866_025_4),
        radius: 2.0,
        height: 1.0,
    };
    let hit = c
        .world
        .attack(probe, &cone)
        .iter()
        .any(|profile| profile.id == PLAYER);
    c.world.destroy(probe);
    hit
}

fn player_physics_state(app: &App) -> dereth_physics::PhysicsState {
    let c = app
        .world_state()
        .expect("a scene")
        .character
        .as_ref()
        .expect("the player body");
    c.world
        .get(c.handle)
        .expect("the player body is registered")
        .state()
}

/// **Both hidden-state edges on the local body remove queued link animations; an unchanged word
/// does not.** The action and cycle are resolved from the shipped Aluvian player table; only the
/// state messages are synthetic, and they use the two exact words recorded around every teleport.
///
/// The draw experiment is disabled deliberately: link removal is a body and animation lifecycle
/// and does not depend on `object_state_draw`, which controls only the part-array draw adaptation.
/// The repeated hidden word is the equality control: no edge means no removal.
#[test]
fn encoded_hidden_edges_remove_link_animations_but_an_unchanged_word_does_not() {
    let mut app = app_with_object_state_draw(false);
    log_in(&mut app);

    let visible_links = queue_wave_link(&mut app);
    teleport_hide(&mut app);
    assert!(app.frame(), "the encoded hide reaches the local player");
    assert_only_cycle_remains(&app, visible_links);

    let hidden_links = queue_wave_link(&mut app);
    send_player_state(&mut app, TELEPORT_HIDE_STATE, 2);
    assert!(app.frame(), "the newer equal word is accepted");
    {
        let scene = app.world_scene().expect("a scene");
        let c = scene.character.as_ref().expect("the player body");
        let d = c.driver();
        assert_eq!(
            d.sequence.first_cyclic(),
            Some(hidden_links),
            "the unchanged HIDDEN word did not fabricate another hidden-state transition"
        );
        assert!(
            d.movement.motions_pending(),
            "the unchanged word leaves the action pending"
        );
    }

    send_player_state(&mut app, TELEPORT_UNHIDE_STATE, 3);
    assert!(app.frame(), "the encoded unhide reaches the local player");
    assert_only_cycle_remains(&app, hidden_links);
}

/// **The same edges on a remote object remove its link animations but keep the object, its draw
/// frame and its cycle.** An accepted `0xF745` creates the real `SceneObject` driver, its shipped
/// table queues a real Wave link and action, and only then do `0xF74B` hide and unhide words
/// exercise `apply_object_state`. The draw experiment is disabled so this lifecycle cannot pass
/// merely because the renderer-side branch ran.
#[test]
fn remote_hidden_edges_remove_link_animations_but_keep_the_object_and_cycle() {
    let mut app = app_with_object_state_draw(false);
    log_in(&mut app);
    create_remote_player(&mut app);

    let before_frame = app
        .world_state()
        .expect("a scene")
        .server_object_frame(REMOTE)
        .expect("remote frame");
    let visible_links = queue_remote_wave_link(&app);
    send_player_state_for(&mut app, REMOTE, TELEPORT_HIDE_STATE, 1);
    assert!(
        app.frame(),
        "the encoded remote hide reaches apply_object_state"
    );
    assert_remote_cycle_only(&app, visible_links);
    assert_eq!(
        app.world_state()
            .expect("a scene")
            .server_object_frame(REMOTE),
        Some(before_frame),
        "HIDDEN does not remove or move the remote object's retained draw frame"
    );

    let hidden_links = queue_remote_wave_link(&app);
    send_player_state_for(&mut app, REMOTE, TELEPORT_HIDE_STATE, 2);
    assert!(app.frame(), "the newer equal remote word is accepted");
    {
        let scene = app.world_scene().expect("a scene");
        assert_eq!(
            scene
                .server_object_sequence(REMOTE)
                .expect("remote sequence")
                .first_cyclic(),
            Some(hidden_links),
            "an unchanged hidden word does not fabricate a second edge"
        );
        assert!(scene
            .server_object_motions_pending(REMOTE)
            .is_some_and(|n| n > 0));
    }

    send_player_state_for(&mut app, REMOTE, TELEPORT_UNHIDE_STATE, 3);
    assert!(
        app.frame(),
        "the encoded remote unhide reaches apply_object_state"
    );
    assert_remote_cycle_only(&app, hidden_links);
}

/// Long enough for every particle either script emits to outlive its `lifespan` and be reaped:
/// the hide's two emitter infos carry `lifespan` 0.5 and 0.75 s, and a frame is 1/30 s.
const REAP_FRAMES: u32 = 60;

/// Behaviour: object.set-state.a-body-the-shard-hides-stops-being-a-target-and-comes-back-in-place
///
/// **The teleport hide removes the player from attack detection until the unhide.** `0xF74B`
/// reaches the player's physical body, not only his draw-side `PartArray`.
///
/// Hiding clears `REPORT_COLLISIONS_PS` and sets `IGNORE_COLLISIONS_PS`; unhiding does the inverse
/// and reports collision start. The recorded teleport words already name those final flags, so
/// the assertion also goes through the attack-target search, which a word copied into a
/// scene-side cache cannot satisfy.
#[test]
fn the_teleport_hide_removes_the_player_from_attack_detection_until_unhide() {
    let mut app = app();
    log_in(&mut app);
    assert!(
        attack_detects_player(&mut app),
        "the overlapping control must find the visible body"
    );

    teleport_hide(&mut app);
    assert!(app.frame(), "the inbound hide reaches the scene");
    let hidden = player_physics_state(&app);
    assert!(
        hidden.is_hidden(),
        "the 0xF74B HIDDEN_PS bit reached the player's physics object"
    );
    assert!(
        hidden.ignores_collisions(),
        "hiding makes the player an ignored target"
    );
    assert!(
        !hidden.reports_collisions(),
        "hiding disables collision reporting"
    );
    assert!(
        !attack_detects_player(&mut app),
        "the attack search still detected the player while the hidden-state consumer says to ignore him"
    );

    teleport_unhide(&mut app);
    assert!(app.frame(), "the inbound unhide reaches the scene");
    let visible = player_physics_state(&app);
    assert!(
        !visible.is_hidden(),
        "the 0xF74B clear reached the player's physics object"
    );
    assert!(
        !visible.ignores_collisions(),
        "unhiding restores the player as a target"
    );
    assert!(
        visible.reports_collisions(),
        "unhiding restores collision reporting"
    );
    assert!(
        attack_detects_player(&mut app),
        "the unhidden player is detected again"
    );
}

// ---------------------------------------------------------------------------------------------
// The shimmer lifetime, and its controls.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.hidden.the-hide-shimmer-lasts-until-the-shard-unhides
///
/// **Present before, gone after, in one run.**
///
/// The hide puts fourteen infinite emitters on the body; the player's own `0xF74B` clearing
/// `HIDDEN_PS` plays the unhide script, whose `StopParticleHook`s end them.
#[test]
fn the_teleport_shimmer_ends_when_the_server_clears_hidden() {
    let mut app = app();
    log_in(&mut app);
    assert_eq!(body_emitters(&app), 0, "the body starts with no emitters");

    teleport_hide(&mut app);
    for _ in 0..10 {
        app.frame();
    }
    let during = body_emitters(&app);
    assert_eq!(during, 14, "the hide put its fourteen emitters on the body");

    teleport_unhide(&mut app);
    for _ in 0..REAP_FRAMES {
        app.frame();
    }
    assert_eq!(
        body_emitters(&app),
        0,
        "the unhide's fourteen StopParticleHooks must have ended the shimmer -- it was {during} \
         before the unhide and {REAP_FRAMES} frames have passed since"
    );
}

/// Behaviour: objects.hidden.the-hide-shimmer-lasts-until-the-shard-unhides
///
/// **The control.** The identical run with the unhide withheld: the shimmer is still there.
///
/// The client neither reaps every emitter after a while nor reaps an emitter whose owner has not
/// moved.
#[test]
fn the_shimmer_is_still_there_when_the_unhide_never_arrives() {
    let mut app = app();
    log_in(&mut app);
    teleport_hide(&mut app);
    for _ in 0..10 {
        app.frame();
    }
    assert_eq!(
        body_emitters(&app),
        14,
        "the hide put its fourteen emitters on the body"
    );

    for _ in 0..REAP_FRAMES {
        app.frame();
    }
    assert_eq!(
        body_emitters(&app),
        14,
        "with no unhide the fourteen infinite emitters are still running -- `total_particles == 0 \
         && total_seconds == 0.0` means the automatic stop condition never latches"
    );
}

/// **The second control.** The materialise effect (type 88) is finite, ends itself, and needs
/// no message at all.
#[test]
fn the_materialise_effect_still_ends_itself_with_no_message_at_all() {
    let mut app = app();
    log_in(&mut app);
    send(
        &mut app,
        &EffectsPlayScriptType {
            id: PLAYER,
            script_type: i32::try_from(PS_CREATE).expect("88"),
            intensity: 1.0,
        },
    );
    for _ in 0..5 {
        app.frame();
    }
    assert_eq!(
        body_emitters(&app),
        2,
        "the materialise script's two emitters are on the body"
    );
    // `total_particles = 25` at `birthrate = 1/30 s` plus `lifespan = 1.0 +/- 0.1`.
    for _ in 0..90 {
        app.frame();
    }
    assert_eq!(
        body_emitters(&app),
        0,
        "a finite emitter is reaped by particle updating with nothing on the wire"
    );
}

// ---------------------------------------------------------------------------------------------
// At login: hidden, then visible again, and its controls.
// ---------------------------------------------------------------------------------------------

/// **The recorded login**: `PlayerCreated`, then his own `0xF745` carrying `0x00404410`.
fn log_in_hidden(app: &mut App) {
    create_player(app, TELEPORT_HIDE_STATE);
}

/// The hide half of the recorded cycle, as the two messages the corpus carries: the hide and then
/// the state word.
fn hide(app: &mut App, event: u16) {
    send(
        app,
        &EffectsPlayScriptType {
            id: PLAYER,
            script_type: i32::try_from(PS_HIDE).expect("116"),
            intensity: 1.0,
        },
    );
    send(
        app,
        &ItemSetState {
            id: PLAYER,
            state: TELEPORT_HIDE_STATE,
            timestamps: PhysicsEventStamp { instance: 0, event },
        },
    );
}

fn unhide(app: &mut App, event: u16) {
    send(
        app,
        &ItemSetState {
            id: PLAYER,
            state: TELEPORT_UNHIDE_STATE,
            timestamps: PhysicsEventStamp { instance: 0, event },
        },
    );
}

/// The part-drawing no-draw mask `(draw_state & 1)` over the body's part array: the same
/// predicate `WorldScene`'s submission loop applies, counted as "parts that would be drawn".
fn body_parts_drawn(app: &App) -> (usize, usize) {
    let s = app.world_scene().expect("a scene");
    let c = s.character.as_ref().expect("a body");
    let d = c.driver();
    let total = d.part_array.parts.len();
    (
        d.part_array.parts.iter().filter(|p| !p.no_draw()).count(),
        total,
    )
}

/// The body's current per-part material translucency. `None` once the part uses the gfxobj's
/// own material again, which is what restoring that material after a finished fade to `0.0` does.
fn body_translucency(app: &App) -> Option<f32> {
    let s = app.world_scene().expect("a scene");
    let c = s.character.as_ref().expect("a body");
    let d = c.driver();
    d.part_array
        .parts
        .first()
        .and_then(|p| p.material.map(|m| m.translucency))
}

/// One second of frames at the harness's 1/30 s step: comfortably past the 0.75 s ramp.
const FADE_FRAMES: u32 = 30;

/// Behaviour: objects.hidden.the-body-is-drawn-again-after-the-unhide-ramp
///
/// **Gone, then back, in one run.** A hide cycle on the recorded wire words makes every body
/// part NoDraw, and the server clearing `HIDDEN_PS` brings every part back.
///
/// The hidden script's immediate `Transparent 1.0` sets NoDraw on every part; only the unhide's
/// 0.75 s ramp back to `0.0` can draw him again.
#[test]
fn the_body_is_drawn_again_after_the_server_clears_hidden() {
    let mut app = app();
    create_player(&mut app, TELEPORT_UNHIDE_STATE);
    let (drawn_before, total) = body_parts_drawn(&app);
    assert!(total > 0, "the body has parts at all");
    assert_eq!(drawn_before, total, "he starts visible");

    // The two messages arrive together, so the hide's 0.75 s fade-**out** is in flight when
    // the hidden script's immediate `1.0` lands and keeps writing lower values over it until it
    // finishes: the hook walk does not cancel an existing timed interpolation either. A second
    // later the fade has reached `1.0` and part-translucency assignment has set NoDraw on every
    // part.
    hide(&mut app, 1);
    for _ in 0..FADE_FRAMES {
        app.frame();
    }
    let (drawn_hidden, _) = body_parts_drawn(&app);
    assert_eq!(
        drawn_hidden, 0,
        "the hidden-state edge -> the hidden script -> `Transparent 1.0` -> NoDraw on every part"
    );

    unhide(&mut app, 2);
    for _ in 0..FADE_FRAMES {
        app.frame();
    }
    let (drawn_after, _) = body_parts_drawn(&app);
    assert_eq!(
        drawn_after, total,
        "the unhide's `Transparent 1.0 -> 0.0` over 0.75 s must have brought every one of the \
         {total} body parts back -- {drawn_hidden} were drawn while hidden and {FADE_FRAMES} \
         frames (1.0 s) have passed since the server cleared HIDDEN_PS"
    );
    assert_eq!(
        body_translucency(&app),
        None,
        "and the fade finished at exactly 0.0, which material restoration drops"
    );
}

/// Behaviour: objects.hidden.the-body-is-drawn-again-after-the-unhide-ramp
///
/// **After the recorded login sequence the player can be seen.**
///
/// The create carries `0x00404410`, and the later bare state message carries `0x00400408`, as in
/// the recorded logins checked above. After the unhide's fade every part is drawn at full opacity.
/// The client builds the part array from the description's setup before applying its state, and
/// keeps object-level translucency to reapply across appearance changes; this final-visible check
/// alone does not show that the hidden state survives an appearance rebuild.
#[test]
fn the_recorded_login_sequence_leaves_the_player_visible() {
    let mut app = app();
    log_in_hidden(&mut app);
    unhide(&mut app, 1);
    for _ in 0..FADE_FRAMES {
        app.frame();
    }
    let (drawn, total) = body_parts_drawn(&app);
    assert!(total > 0, "the body has parts at all");
    assert_eq!(
        drawn, total,
        "the player can be seen once the login unhide has been applied"
    );
    assert_eq!(
        body_translucency(&app),
        None,
        "at full opacity, not part-way through a fade"
    );
}

/// Behaviour: objects.hidden.the-body-is-drawn-again-after-the-unhide-ramp
///
/// **The fade is a ramp over time, not a switch.** Partway through the unhide's 0.75 s the body
/// is drawn *and* still partly transparent; a client that simply applied the end value would be at
/// `0.0` already, and one that applied nothing would still be at NoDraw.
#[test]
fn the_unhide_fade_passes_through_the_middle_of_the_ramp() {
    let mut app = app();
    create_player(&mut app, TELEPORT_UNHIDE_STATE);
    hide(&mut app, 1);
    for _ in 0..FADE_FRAMES {
        app.frame();
    }
    unhide(&mut app, 2);
    // 12 frames at 1/30 s is 0.40 s, which is inside 0.75 s and past its middle.
    for _ in 0..12 {
        app.frame();
    }
    let (drawn, total) = body_parts_drawn(&app);
    assert_eq!(drawn, total, "0.40 s into the ramp the body is drawn again");
    let t = body_translucency(&app).expect("...and carries a translucency below 1.0");
    assert!(
        t > 0.0 && t < 1.0,
        "0.40 s into a 0.75 s ramp from 1.0 to 0.0 the body is partly transparent, not either \
         end of it -- the timed hook evaluates `start + (end - start) * t/duration`; got {t}"
    );
}

/// Behaviour: objects.hidden.the-body-is-drawn-again-after-the-unhide-ramp
///
/// **The control.** The identical run with the unhide withheld: the body stays hidden. Nothing in
/// the client clears NoDraw on the body after a while on its own.
#[test]
fn the_body_stays_hidden_while_the_server_still_says_hidden() {
    let mut app = app();
    create_player(&mut app, TELEPORT_UNHIDE_STATE);
    hide(&mut app, 1);
    for _ in 0..FADE_FRAMES * 3 {
        app.frame();
    }
    let (drawn, total) = body_parts_drawn(&app);
    assert!(total > 0, "the body has parts at all");
    assert_eq!(
        drawn,
        0,
        "with no unhide on the wire the body is still not drawn after {} frames -- HIDDEN_PS is \
         the server's to clear and nothing in the client clears it on its own",
        FADE_FRAMES * 3
    );
}

/// **The second control.** A login create *without* `HIDDEN_PS` leaves the body drawn from the
/// first frame: the state word is applied either way, and only the bit decides.
#[test]
fn a_login_create_without_hidden_draws_the_body_at_once() {
    let mut app = app();
    create_player(&mut app, TELEPORT_UNHIDE_STATE);
    let (drawn, total) = body_parts_drawn(&app);
    assert!(total > 0, "the body has parts at all");
    assert_eq!(
        drawn, total,
        "no HIDDEN_PS, no hide script, every part drawn"
    );
}
