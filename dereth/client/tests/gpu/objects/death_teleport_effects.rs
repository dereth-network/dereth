//! **A remote player's death teleport stops submitting particles at the old site.**
//!
//! When another player dies, a survivor at the death site sees the corpse appear and the
//! victim's teleport shimmer disappear from the death site; the victim next materializes at the
//! lifestone. These tests isolate the unloaded-destination case.
//!
//! The modeled server order (ACE): the Dead motion is broadcast, the server waits for its
//! animation, creates a separate corpse and teleports the victim. The teleport sends a position
//! update naming the sanctuary, then changes the hidden/ignore/report-collisions state. A
//! non-admin position update bumps the position sequence and keeps the teleport sequence. The
//! state message is broadcast only when its values changed. Relocation as an adjacency move
//! suppresses the delete broadcast; that absence is not a third message, and ACE culls known
//! objects from the client after 25 seconds. The modeled create, position and state body types
//! are 0xF745, 0xF748 and 0xF74B; the absent delete would be 0xF747.
//!
//! The client resolves the destination cell on placement. For an unloaded destination it removes
//! cell shadows, leaves the cell and queues the object and its children for destruction; then it
//! stores the destination position, registers the object for lost-cell maintenance and clears its
//! active flag. Leaving the cell lists removes it from drawn-cell particle collection even if a
//! later state change starts emitters. The 25 seconds delay destruction, not disappearance.
//!
//! Fixture: a local `App` on the retail assets, fed synthetic encoded create/position/state bodies
//! directly through `ObjectStream::apply_event`; no transport or session replay. The victim and a
//! separate corpse both use the player setup and motion table; the corpse omits the
//! physics-script table. Counters observe object part submissions, owned emitter records and
//! scene-wide particle submissions, not individual pixels. An `App` that cannot be built fails
//! the test.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::{EmitterOwner, SceneConfig};
use dereth_client::{app::App, config::Config};
use dereth_client_net::client_session::SessionEvent;
use dereth_physics::V3;
use dereth_primitives::{DataId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::movement::{MovementPositionEvent, PositionPack};
use dereth_protocol::objects::{ItemCreateObject, ItemSetState, ObjectCreatePayload};
use dereth_protocol::types::{PhysicsDesc, PhysicsEventStamp, PhysicsTimestamps, PublicWeenieDesc};
use dereth_protocol::Message;

/// Aluvian male setup and ordinary player motion table. The recorded player's script table
/// carries fourteen hidden-state emitters (see `objects::hidden_state`). This test supplies that
/// table explicitly and checks the resulting emitter count, not the whole capture corpus.
const PLAYER_SETUP: u32 = 0x0200_0001;
const PLAYER_MTABLE: u32 = 0x0900_0001;
const PLAYER_TABLE: u32 = 0x3400_0004;

const PLAYER: ObjectId = ObjectId(0x5000_1106);
const VICTIM: ObjectId = ObjectId(0x5000_2106);
const CORPSE: ObjectId = ObjectId(0x5000_3106);

/// Literal hide/visible state words, as recorded around a retail teleport.
const TELEPORT_HIDE_STATE: u32 = 0x0040_4410;
const VISIBLE_STATE: u32 = 0x0040_0408;

fn app() -> App {
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        preferences_file: std::env::temp_dir()
            .join("dere-death-teleport-effects-not-created/prefs.ini"),
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
        ..SceneConfig::default()
    })
    .expect("the static scene loads");
    assert!(app.frame());
    app
}

fn send<M: Message>(app: &mut App, m: &M) {
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: M::OPCODE,
            body: dereth_protocol::write_body(m).expect("encodes"),
        },
        LocalTime(0.0),
    );
}

/// The outdoor land cell of the viewer's own resident landblock.
fn viewer_cell(app: &App) -> u32 {
    viewer_cell_of(app.world_scene().expect("a scene"))
}

fn viewer_cell_of(s: dereth_client::world::WorldSceneRef<'_>) -> u32 {
    let block = s.viewer_block().expect("a resident block");
    (u32::try_from(block.0).expect("x") << 24) | (u32::try_from(block.1).expect("y") << 16) | 1
}

fn create(
    app: &mut App,
    id: ObjectId,
    cell: u32,
    origin: Vec3,
    script_table: Option<u32>,
    state: u32,
) {
    send(
        app,
        &ItemCreateObject(ObjectCreatePayload {
            id,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                    | dereth_protocol::types::physicsdesc::flags::SETUP
                    | dereth_protocol::types::physicsdesc::flags::MTABLE
                    | if script_table.is_some() {
                        dereth_protocol::types::physicsdesc::flags::PETABLE
                    } else {
                        0
                    },
                state,
                setup_id: Some(PLAYER_SETUP),
                mtable_id: Some(PLAYER_MTABLE),
                phstable_id: script_table,
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: cell,
                    frame: dereth_protocol::types::Frame {
                        origin: origin.into(),
                        orientation: Quat::IDENTITY.into(),
                    },
                }),
                timestamps: PhysicsTimestamps {
                    instance: 0,
                    ..PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc {
                name: "death teleport".into(),
                obj_type: 0x10,
                bitfield: 0x08,
                ..PublicWeenieDesc::default()
            },
        }),
    );
}

/// Synthetic position update following ACE's non-admin sequence convention: advance the
/// position stamp and retain teleport stamp 0, with the grounded-contact flag.
fn position_event(app: &mut App, id: ObjectId, to: Position, stamp: u16) {
    send(
        app,
        &MovementPositionEvent {
            id,
            position: PositionPack {
                // The contact flag is significant: the client's received-position route refuses
                // repositioning without contact. ACE sets the grounded flag from the physics
                // on-walkable transient state, and the modeled victim was standing when killed,
                // so the 0x04 flag and teleport stamp 0 are sent here.
                flags: dereth_protocol::movement::position_flags::IS_GROUNDED,
                origin: dereth_protocol::types::Origin {
                    objcell_id: to.cell.0,
                    origin: to.frame.origin.into(),
                },
                orientation: to.frame.rotation.into(),
                position_timestamp: stamp,
                teleport_timestamp: 0,
                ..PositionPack::default()
            },
        },
    );
}

fn set_state(app: &mut App, id: ObjectId, state: u32, event: u16) {
    send(
        app,
        &ItemSetState {
            id,
            state,
            timestamps: PhysicsEventStamp { instance: 0, event },
        },
    );
}

/// Log a synthetic local player in, the way `objects::hidden_state` does.
fn log_in(app: &mut App) {
    app.objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(PLAYER), LocalTime(0.0));
    let (cell, origin) = {
        let s = app.world_scene().expect("a scene");
        (
            viewer_cell_of(s),
            s.character.as_ref().expect("a body").render_frame().origin,
        )
    };
    create(app, PLAYER, cell, origin, Some(PLAYER_TABLE), VISIBLE_STATE);
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
        "description application puts the encoded physics-script table on the body"
    );
}

/// Candidate 10 m forward in the camera frame. Later positive part-submission predicates check
/// this fixture; the coordinate helper alone does not guarantee pixel visibility.
fn in_front(app: &App) -> Vec3 {
    let s = app.world_scene().expect("a scene");
    dereth_physics::math::localtoglobal(&s.camera.frame(), Vec3::new(0.0, 10.0, 0.0))
}

fn object_emitters(app: &App, id: ObjectId) -> usize {
    app.world_scene()
        .expect("a scene")
        .emitter_degrade_probe()
        .into_iter()
        .filter(|e| e.owner == EmitterOwner::Object(id))
        .count()
}

fn drawn_parts(app: &App, id: ObjectId) -> usize {
    app.world_scene()
        .expect("a scene")
        .drawn_part_order()
        .iter()
        .filter(|p| p.object == Some(id))
        .count()
}

fn drawn_particles(app: &App) -> usize {
    app.world_scene().expect("a scene").drawn_particles().drawn
}

/// Current scene accessor for the server-object draw cell used by object and particle
/// collection. A None result is the tested unloaded-destination state, not an inspection of
/// every physics-cell list or an assertion that the object has already been destroyed.
fn draw_cell(app: &App, id: ObjectId) -> Option<dereth_primitives::CellId> {
    app.world_state()
        .expect("a scene")
        .server_object_draw_cell(id)
}

/// Synthetic death-site/corpse setup, distant destination and observed background particle
/// count envelope. Later comparisons use this sampled window, not a steady-state proof.
struct Staged {
    site: Position,
    sanctuary: Position,
    /// Maximum/minimum scene-wide particle submissions across 120 measured frames after 120
    /// warm-up frames, with no victim-owned effect.
    high: usize,
    low: usize,
}

/// Construct a visible victim and a separate player-shaped corpse fixture before teleporting.
/// This models their independent identities, not death-animation timing or corpse serialization.
fn stage_the_kill(app: &mut App) -> Staged {
    log_in(app);
    let site = Position::new(
        dereth_primitives::CellId(viewer_cell(app)),
        dereth_primitives::Frame::new(in_front(app), Quat::IDENTITY),
    );

    create(
        app,
        VICTIM,
        site.cell.0,
        site.frame.origin,
        Some(PLAYER_TABLE),
        VISIBLE_STATE,
    );
    for _ in 0..10 {
        assert!(app.frame());
    }
    assert!(
        drawn_parts(app, VICTIM) > 0,
        "the victim initially submits parts to the draw pass"
    );
    assert_eq!(
        object_emitters(app, VICTIM),
        0,
        "and with no emitters of his own"
    );

    // Separate synthetic corpse identity at the site, using player geometry but no script table.
    create(
        app,
        CORPSE,
        site.cell.0,
        site.frame.origin,
        None,
        VISIBLE_STATE,
    );
    for _ in 0..10 {
        assert!(app.frame());
    }
    assert!(
        drawn_parts(app, CORPSE) > 0,
        "the synthetic corpse object submits parts at the staged death site"
    );
    assert_eq!(
        object_emitters(app, CORPSE),
        0,
        "the corpse carries no script table and no effect"
    );

    // Warm the scene for 120 frames, then sample another 120. Positive high and spread <= high/4
    // below bound this observed window; they do not prove future stationarity or distinguish
    // every emitter's contribution. The same scalar envelope is used by both differential arms.
    // Infinite scenery scripts fill and then oscillate about max_particles; the warm-up keeps a
    // still-filling background out of the measured envelope.
    for _ in 0..120 {
        assert!(app.frame());
    }
    let mut settled = Vec::new();
    for _ in 0..120 {
        assert!(app.frame());
        settled.push(drawn_particles(app));
    }
    let high = *settled.iter().max().expect("a settled window");
    let low = *settled.iter().min().expect("a settled window");
    assert!(
        high > 0 && high - low <= high / 4,
        "the sampled background window must have positive counts and spread no greater than one quarter of its maximum -- {low}..={high}"
    );

    // The sanctuary: a landblock five blocks away, which `land_radius: 1` has not loaded.
    let block = site.cell.landblock();
    let far = dereth_primitives::LandblockId(
        (u16::from(block.x().wrapping_add(5)) << 8) | u16::from(block.y()),
    );
    let sanctuary = Position::new(far.cell(1), site.frame);
    Staged {
        site,
        sanctuary,
        high,
        low,
    }
}

/// A fixed margin: 60 is half the smallest measured shimmer and four times the observed
/// background envelope. The assertions do not re-derive those ratios or require every
/// post-update frame to exceed it.
const SHIMMER: usize = 60;

// ---------------------------------------------------------------------------------------------
// The rejecting test, and its controls.
// ---------------------------------------------------------------------------------------------

/// Behaviour: objects.death.a-remote-death-teleport-takes-the-shimmer-off-the-old-site
///
/// Send the distant position update followed by hide state, then sample 30 frames. Every sampled
/// scene-wide particle count must stay at/below the prior maximum. At the final sample the
/// victim must still own 14 emitters but have no draw cell; the separate corpse must still submit
/// parts in the death-site landblock. This distinguishes loss of draw-cell placement from simply
/// suppressing emitter creation, within the observed counter/window scope.
#[test]
fn the_death_teleport_takes_the_victims_shimmer_off_the_survivors_screen() {
    let mut app = app();
    let staged = stage_the_kill(&mut app);

    // Outside-cell normalization can change the cell index; compare the landblock, as in the
    // related objects::remote_leave_return test, rather than asserting an exact cell number.
    assert_eq!(
        draw_cell(&app, VICTIM).map(dereth_primitives::CellId::landblock),
        Some(staged.site.cell.landblock()),
        "he is standing in the death site's landblock"
    );

    position_event(&mut app, VICTIM, staged.sanctuary, 1);
    set_state(&mut app, VICTIM, TELEPORT_HIDE_STATE, 1);
    let mut after = 0;
    for _ in 0..30 {
        assert!(app.frame());
        after = after.max(drawn_particles(&app));
    }

    eprintln!(
        "after the death teleport: victim emitters={} draw cell={:?} corpse parts={} \
         drawn particles={after} (block {}..={})",
        object_emitters(&app, VICTIM),
        draw_cell(&app, VICTIM),
        drawn_parts(&app, CORPSE),
        staged.low,
        staged.high,
    );

    assert_eq!(
        object_emitters(&app, VICTIM),
        14,
        "the hidden-state update leaves fourteen emitters owned by the victim at the final sample"
    );
    assert_eq!(
        draw_cell(&app, VICTIM),
        None,
        "the victim has no draw cell after the update naming an unloaded landblock"
    );
    assert!(
        after <= staged.high,
        "the maximum of the next 30 scene-wide particle counts must stay within the background maximum -- {after} against the sampled {}..={}",
        staged.low,
        staged.high,
    );
    assert!(
        drawn_parts(&app, CORPSE) > 0,
        "the corpse is a different object with its own position and must still be there"
    );
    assert_eq!(
        draw_cell(&app, CORPSE).map(dereth_primitives::CellId::landblock),
        Some(staged.site.cell.landblock()),
        "and it is still in the death site's landblock"
    );
}

/// Behaviour: objects.death.a-remote-death-teleport-takes-the-shimmer-off-the-old-site
///
/// Use the same hide word with a nearby destination in the loaded landblock. At the final
/// sample require 14 owned emitters and a draw cell in that landblock. The maximum over 30 frames
/// must be at least background-high+60, so at least one sampled frame supplies the increase.
/// It does not require that increase on every frame or inspect individual effect pixels.
/// This positive control rejects suppressing all hidden-object effects or emitter creation.
#[test]
fn an_ordinary_hide_in_a_loaded_cell_still_shows_the_shimmer() {
    let mut app = app();
    let staged = stage_the_kill(&mut app);

    let nearby = Position::new(
        staged.site.cell,
        dereth_primitives::Frame::new(
            staged.site.frame.origin.add(Vec3::new(1.0, 0.0, 0.0)),
            Quat::IDENTITY,
        ),
    );
    position_event(&mut app, VICTIM, nearby, 1);
    set_state(&mut app, VICTIM, TELEPORT_HIDE_STATE, 1);
    let mut after = 0;
    for _ in 0..30 {
        assert!(app.frame());
        after = after.max(drawn_particles(&app));
    }

    eprintln!(
        "loaded-cell control: victim emitters={} draw cell={:?} drawn particles={after} \
         (block {}..={})",
        object_emitters(&app, VICTIM),
        draw_cell(&app, VICTIM),
        staged.low,
        staged.high,
    );
    assert_eq!(
        object_emitters(&app, VICTIM),
        14,
        "the hidden-state update leaves fourteen emitters owned by the loaded victim"
    );
    assert_eq!(
        draw_cell(&app, VICTIM).map(dereth_primitives::CellId::landblock),
        Some(staged.site.cell.landblock()),
        "the victim draw cell remains in the resident death-site landblock"
    );
    assert!(
        after >= staged.high + SHIMMER,
        "at least one of the next 30 scene-wide particle counts must be at least 60 above the background maximum -- {after} against the sampled {}..={}",
        staged.low,
        staged.high,
    );
}
