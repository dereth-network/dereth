//! The selection blink: a clicked world object runs bright, dim, bright, dim and is then restored;
//! a worn item clicked on the paper doll blinks its own doll region bright then dim; selecting
//! yourself blinks the whole doll. Each lighting state is checked against back-buffer pixels.
//! Fixture: a headless App on `DEFAULT_LANDBLOCK` terrain with the shipped gameplay layout, the
//! player from `early-inventory-and-casting`, and two synthetic monsters; no datagram leaves the
//! process.
//!
//! # Retail behaviour
//!
//! The retail 3-D viewport keeps a flip count, the next flip time, and the object being flashed.
//! Its object-found handler first restores a different object when a flash is active and clears the
//! count. A found object with a selection-or-later reason then receives bright lighting, starts the
//! count at one, and schedules the next flip 0.2 seconds later. Drop and targeted-use finds start
//! the flash without changing the selected object; the other qualifying reasons also select it.
//! The handler stores the found object id at its tail regardless of which reason branch ran.
//!
//! Each subsequent global update after the deadline increments the count. Counts below five re-arm
//! another 0.2-second deadline and alternate odd bright states with even dim states. Count five
//! restores the normal lighting and clears the count. The retail data values are luminosity 0.0
//! and diffuse 0.35 for dim, luminosity 0.99 and diffuse 1.0 for bright, and a 0.2-second interval.
//! The part lighting path maps those two values to emissive and diffuse material terms.
//!
//! A **clicked world object** is therefore bright immediately, dim at +0.2 s, bright at +0.4,
//! dim at +0.6, and restored at +0.8: two bright flashes. The paper doll uses the same bright,
//! dim, and restore values for one 0.4-second flash. It lights only the selected worn item's mask,
//! or every doll part when the player is selected. Doll lighting follows every selection route;
//! world lighting starts only from the viewport's object-find path, so keyboard selection changes
//! the selected object without starting a world flash.
//!
//! # What this file measures
//!
//! An injected pointer press passes through the current input and picking paths and must select the
//! object; calling the selection notice directly would not cover that route. Each frame's back
//! buffer is read over the inner 40% of the object's projected rectangle, clear of the target
//! indicator brackets, against a twelve-frame pre-click baseline and an unclicked control monster.
//! The world target's lighting is sampled beside every post-click pixel mean. Doll sampling records
//! the selected part's lighting, while separate assertions check which parts were initially lit.
//! These observers distinguish a missing state transition from a renderer that failed to bind it.
//!
//! The headless clock advances by 1/30 second per frame, so 0.2 seconds is six frames.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_animation::parts::{
    DEFAULT_DIFFUSE, DEFAULT_LUMINOSITY, SELECTION_HIGH_LIGHTING, SELECTION_LOW_LIGHTING,
};
use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::gpu::PreviewId;
use dereth_client::pick::PickScene;
use dereth_client::world::{SceneConfig, DEFAULT_LANDBLOCK};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::pick_geometry::selection_ray;
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, ObjectId, Position, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ItemSetState};
use dereth_protocol::types::PhysicsEventStamp;
use dereth_protocol::{Message, Opcode};
use dereth_ui_screens::hud::target::Projection;

const SCREEN: (u32, u32) = (800, 600);
/// The state word that closes the recorded login's hidden create.
const TELEPORT_UNHIDE_STATE: u32 = 0x0040_0408;
/// The Aluvian male body every bench in the tree stands up.
const MONSTER_SETUP: u32 = 0x0200_0001;
const TARGET: ObjectId = ObjectId(0x8000_2001);
const CONTROL: ObjectId = ObjectId(0x8000_2002);
const PRE_FRAMES: usize = 12;
/// 1.2 s: the world's 0.8 s blink and a restored tail long enough to prove it stays restored.
const POST_FRAMES: usize = 36;
/// The retail 0.2-second flip interval in headless frames: `0.2 / (1/30)`.
const FRAMES_PER_FLIP: usize = 6;

const BRIGHT: (f32, f32) = SELECTION_HIGH_LIGHTING;
const DIM: (f32, f32) = SELECTION_LOW_LIGHTING;
const RESTORED: (f32, f32) = (DEFAULT_LUMINOSITY, DEFAULT_DIFFUSE);

/// **How the two lighting states read in pixels.** The fixed-function bright is
/// `saturate(Emissive 0.99 + Diffuse 1.0 * lights)`, which is visibly brighter than normal only
/// because the *normal* lit colour is below 1: an object drawn at its texture's full brightness
/// could not be lifted further, and only its bound `(0.99, 1.0)` state would show the blink.
///
/// With objects lit, a measured run gives the world target a baseline of 65.11, BRIGHT runs of
/// 108.80 / 108.70, and DIM runs of 60.95; the doll's legs 13.17 / 15.32 / 11.52. The bright half
/// reads *above* the baseline as the retail client does. The dim half is subtler: diffuse 0.35
/// scales only the direct-light term while ambient remains 1.0, and outdoors at noon ambient
/// carries most of a side-lit body's colour. Therefore DIM is asserted below the first BRIGHT
/// mean by more than tolerance and below baseline plus tolerance; it may still sit slightly
/// above the baseline.
#[allow(dead_code)]
const BRIGHT_READS: &str = "above the baseline";

fn frames(app: &mut App, count: usize) {
    for _ in 0..count {
        assert!(app.frame());
    }
}

fn position(app: &App) -> Position {
    app.world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position()
}

/// Start the UI shell and real `DEFAULT_LANDBLOCK` terrain. An application that cannot be
/// initialized fails the GPU station. The recorded player create is relocated onto this terrain
/// with its position flag set; a synthetic state event using the current state stamp plus one then
/// unhides the body through the present object seam.
fn setup() -> (App, ObjectId) {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: SCREEN.0,
        height: SCREEN.1,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir()
            .join("dere-p1-selection-blink-not-created")
            .join("preferences.ini"),
        ..Config::default()
    })
    .unwrap_or_else(|e| panic!("the gpu tier needs a headless App on a software device: {e}"));
    app.start_shell().expect("UI shell");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("real terrain/physics scene");
    frames(&mut app, 60);

    let corpus = Corpus::shared("early-inventory-and-casting");
    let player_row = corpus
        .blobs
        .iter()
        .find(|r| r.dir == Direction::ServerToClient && r.opcode == Opcode::LOGIN_CREATE_PLAYER.0)
        .expect("recorded player identity");
    let id = ObjectId(u32::from_le_bytes(
        player_row.payload[4..8].try_into().unwrap(),
    ));
    let row = corpus
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && u32::from_le_bytes(r.payload[4..8].try_into().unwrap()) == id.0
        })
        .expect("recorded player assets");
    let mut create = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .expect("recorded F745");
    let here = position(&app);
    create.0.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
        objcell_id: here.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: here.frame.origin.x,
                y: here.frame.origin.y,
                z: here.frame.origin.z,
            },
            orientation: dereth_protocol::types::Quat {
                w: here.frame.rotation.w,
                x: here.frame.rotation.x,
                y: here.frame.rotation.y,
                z: here.frame.rotation.z,
            },
        },
    });
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
    app.objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("constructed terrain placement"),
        },
        LocalTime(1.0),
    );
    frames(&mut app, 90);
    {
        let state_ts = app
            .objects()
            .presence(id)
            .expect("the player's presence")
            .state_ts;
        app.objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: ItemSetState::OPCODE,
                body: dereth_protocol::write_body(&ItemSetState {
                    id,
                    state: TELEPORT_UNHIDE_STATE,
                    timestamps: PhysicsEventStamp {
                        instance: 0,
                        event: state_ts.wrapping_add(1),
                    },
                })
                .expect("encodes"),
            },
            LocalTime(4.0),
        );
    }
    frames(&mut app, 60);
    {
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert!(c.on_ground(), "real terrain must support the local body");
        assert_eq!(c.object_id(), id, "the body adopted the server's id");
    }
    (app, id)
}

/// Create a monster at a **player-space** offset through the current object-create event and
/// require that its parts are drawn.
fn place(app: &mut App, id: ObjectId, offset: (f32, f32, f32), now: f64) {
    use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
    let here = position(app);
    let origin =
        dereth_physics::math::localtoglobal(&here.frame, Vec3::new(offset.0, offset.1, offset.2));
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id,
        objdesc: ObjDesc::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP,
            setup_id: Some(MONSTER_SETUP),
            state: 0,
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: here.cell.0,
                frame: dereth_protocol::types::Frame {
                    origin: origin.into(),
                    orientation: here.frame.rotation.into(),
                },
            }),
            ..PhysicsDesc::default()
        },
        wdesc: PublicWeenieDesc::default(),
    };
    let body = dereth_protocol::write_body(&ItemCreateObject(payload)).expect("encode");
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(now),
    );
    frames(app, 30);
    let scene = app.world_scene().unwrap();
    assert!(
        scene.server_object_frame(id).is_some(),
        "{id:?} has a drawn SceneObject: the frame answers for it"
    );
    let drawn = scene
        .drawn_part_order()
        .iter()
        .filter(|d| d.object == Some(id))
        .count();
    assert!(drawn > 0, "{id:?}'s parts were submitted last frame");
}

/// The object's rectangle, on screen.
fn rect(app: &App, id: ObjectId) -> (i32, i32, i32, i32) {
    match app.renderer().target_projection(id, app.world_state()) {
        Some(Projection::OnScreen(r)) => r,
        other => panic!("{id:?} is on screen: {other:?}"),
    }
}

/// The inner 40 % of a rectangle, clear of the target indicator's corner brackets.
fn inner(r: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    let (l, t, rr, b) = r;
    let w = rr - l;
    let h = b - t;
    (
        l + w * 3 / 10,
        t + h * 3 / 10,
        rr - w * 3 / 10,
        b - h * 3 / 10,
    )
}

/// Mean of (B + G + R) / 3 over a region of the last presented frame.
fn brightness(app: &mut App, r: (i32, i32, i32, i32)) -> f64 {
    let (w, h, bgra) = app
        .renderer_mut()
        .capture_bgra()
        .expect("an offscreen capture");
    let (l, t, rr, b) = r;
    let mut sum = 0.0_f64;
    let mut n = 0_u32;
    for y in t.max(0)..b.min(h as i32) {
        for x in l.max(0)..rr.min(w as i32) {
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            sum += f64::from(u32::from(bgra[i]) + u32::from(bgra[i + 1]) + u32::from(bgra[i + 2]))
                / 3.0;
            n += 1;
        }
    }
    assert!(
        n > 0,
        "the region {r:?} has pixels inside the {w}x{h} back buffer"
    );
    sum / f64::from(n)
}

/// Build normalized pointer messages with `Pump` and deliver them directly to the headless input
/// manager at a **screen** point. Two frames cover the press frame and the frame that answers it.
fn press_at(app: &mut App, x: i32, y: i32, at: u32) {
    let mut pump = dereth_client::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let messages = [
        pump.mouse_move_message(f64::from(x), f64::from(y), at),
        pump.mouse_button_message(winit::event::MouseButton::Left, true, at + 10)
            .unwrap(),
        pump.mouse_button_message(winit::event::MouseButton::Left, false, at + 20)
            .unwrap(),
    ];
    for message in messages {
        pump.dispatch(message);
        app.input_manager_mut()
            .expect("real input maps")
            .on_message(message);
    }
    for _ in 0..2 {
        assert!(app.frame());
    }
}

/// Move the pointer without a press, taking it off the doll so the drag-mask hover tooltip never
/// lands on the measured region. As with `press_at`, allow two frames.
fn move_to(app: &mut App, x: i32, y: i32, at: u32) {
    let mut pump = dereth_client::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let message = pump.mouse_move_message(f64::from(x), f64::from(y), at);
    pump.dispatch(message);
    app.input_manager_mut()
        .expect("real input maps")
        .on_message(message);
    for _ in 0..2 {
        assert!(app.frame());
    }
}

/// The `(luminosity, diffuse)` every part of a world object binds this frame, collapsed to one
/// value when all parts agree.
fn world_lighting(app: &App, id: ObjectId) -> (f32, f32) {
    let parts = app
        .world_state()
        .unwrap()
        .object_part_lighting(id)
        .unwrap_or_else(|| panic!("{id:?} is in the scene"));
    assert!(!parts.is_empty(), "{id:?} has parts");
    let first = parts[0];
    assert!(
        parts.iter().all(|p| *p == first),
        "world lighting must be identical across every part: {parts:?}"
    );
    first
}

/// The doll's per-part `(luminosity, diffuse)` this frame.
fn doll_lighting(app: &App) -> Vec<(f32, f32)> {
    app.renderer()
        .preview(PreviewId::PaperDoll)
        .expect("the paper-doll space")
        .object(0)
        .expect("the paper-doll preview object")
        .part_array
        .part_lighting()
}

/// One sampled frame.
#[derive(Debug, Clone)]
struct Sample {
    t: f64,
    target: f64,
    control: f64,
    lighting: (f32, f32),
}

/// The blink read out of the samples: the run lengths of each lighting state in order, with the
/// pixel mean of each run. This is the observable both halves are judged by.
fn runs(samples: &[Sample]) -> Vec<((f32, f32), usize, f64)> {
    let mut out: Vec<((f32, f32), usize, f64)> = Vec::new();
    for s in samples {
        match out.last_mut() {
            Some((state, n, sum)) if *state == s.lighting => {
                *n += 1;
                *sum += s.target;
            }
            _ => out.push((s.lighting, 1, s.target)),
        }
    }
    out.into_iter()
        .map(|(s, n, sum)| (s, n, sum / n as f64))
        .collect()
}

/// The expected run of lighting states followed by `RESTORED`. The first run may contain three to
/// seven sampled frames because the press helper advances two frames before sampling; later runs
/// may contain five to seven around the six-frame interval.
fn assert_runs(runs: &[((f32, f32), usize, f64)], expected: &[(f32, f32)], what: &str) {
    assert!(
        runs.len() == expected.len() + 1,
        "{what}: expected the states {expected:?} then RESTORED, got {runs:?}"
    );
    for (i, want) in expected.iter().enumerate() {
        let (state, n, _) = runs[i];
        assert_eq!(state, *want, "{what}: run {i} is {want:?}: {runs:?}");
        // The first run began inside `press_at`'s two frames, before sampling started, so it
        // can be up to two frames short; every later run is measured whole.
        let lo = if i == 0 {
            FRAMES_PER_FLIP - 3
        } else {
            FRAMES_PER_FLIP - 1
        };
        assert!(
            (lo..=FRAMES_PER_FLIP + 1).contains(&n),
            "{what}: run {i} ({want:?}) must span the accepted 0.2-second sample window around {FRAMES_PER_FLIP} frames, not {n}: {runs:?}"
        );
    }
    let (state, _, _) = runs[expected.len()];
    assert_eq!(
        state, RESTORED,
        "{what}: normal lighting must end the blink: {runs:?}"
    );
}

// =================================================================================================
// (1) A clicked world object.
// =================================================================================================

/// Behaviour: selection.blink.a-picked-object-blinks-bright-dim-bright-dim-and-is-restored
/// Behaviour: selection.blink.a-picked-object-and-worn-item-blink-then-restore
///
/// **The acceptance, world half.** A press through the input path selects the left monster. Over
/// the next 1.2 seconds its lighting runs bright, dim, bright, dim, and restored. The right
/// monster's pixel mean stays within `3 * max_pre_click_variation + 2` for all 36 samples, and its
/// lighting is checked as restored at the end. The target's bound lighting is sampled each frame.
#[test]
fn a_clicked_world_object_blinks_bright_dim_bright_dim_and_is_restored() {
    let (mut app, player) = setup();
    place(&mut app, TARGET, (-1.3, 4.0, 0.0), 10.0);
    place(&mut app, CONTROL, (1.3, 4.0, 0.0), 11.0);
    frames(&mut app, 20);

    let t_rect = rect(&app, TARGET);
    let c_rect = rect(&app, CONTROL);
    eprintln!("selection_blink: target rect {t_rect:?} control rect {c_rect:?}");
    let t_in = inner(t_rect);
    let c_in = inner(c_rect);
    assert!(
        t_in.2 > t_in.0 + 4 && t_in.3 > t_in.1 + 4,
        "the target's inner region is measurable: {t_in:?}"
    );
    assert!(
        c_in.2 > c_in.0 + 4 && c_in.3 > c_in.1 + 4,
        "the control's inner region is measurable: {c_in:?}"
    );
    assert!(
        t_rect.2 <= c_rect.0 || c_rect.2 <= t_rect.0,
        "the two rectangles do not overlap"
    );

    // Baseline, and the per-frame variation of an object nobody clicks.
    let mut pre = Vec::with_capacity(PRE_FRAMES);
    for i in 0..PRE_FRAMES {
        frames(&mut app, 1);
        let s = Sample {
            t: app.clock().cur_time,
            target: brightness(&mut app, t_in),
            control: brightness(&mut app, c_in),
            lighting: world_lighting(&app, TARGET),
        };
        eprintln!(
            "selection_blink: pre {i:2} t {:8.3} target {:7.2} control {:7.2} lighting {:?}",
            s.t, s.target, s.control, s.lighting
        );
        assert_eq!(s.lighting, RESTORED, "nothing lit before the click");
        pre.push(s);
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let spread = |v: &[f64], m: f64| v.iter().map(|x| (x - m).abs()).fold(0.0_f64, f64::max);
    let pre_t: Vec<f64> = pre.iter().map(|s| s.target).collect();
    let pre_c: Vec<f64> = pre.iter().map(|s| s.control).collect();
    let base_t = mean(&pre_t);
    let base_c = mean(&pre_c);
    let variation = spread(&pre_c, base_c).max(spread(&pre_t, base_t));
    let tolerance = 3.0 * variation + 2.0;
    eprintln!("selection_blink: baseline target {base_t:.2} control {base_c:.2} variation {variation:.2} tolerance {tolerance:.2}");
    assert!(
        base_t > 2.0,
        "the target is actually lit and drawn: {base_t}"
    );

    // **The gesture.** A press through the input path on the target's own rectangle must select
    // it. The notice lights it bright *on the press*, so the first sample — the frame that
    // answered the press — is already inside the blink.
    let (cx, cy) = ((t_rect.0 + t_rect.2) / 2, (t_rect.1 + t_rect.3) / 2);
    assert_eq!(
        app.objects().world.selected,
        None,
        "nothing selected to begin with"
    );
    assert_eq!(
        app.interaction().selection_flip_count(),
        0,
        "the world-selection blink is idle"
    );
    press_at(&mut app, cx, cy, 5_000);
    assert_eq!(
        app.objects().world.selected,
        Some(TARGET),
        "the pointer press at ({cx}, {cy}) reached world-object lookup and selected the target"
    );
    assert_ne!(app.objects().world.selected, Some(player));
    let t_click = app.clock().cur_time;

    let mut post = Vec::with_capacity(POST_FRAMES);
    for i in 0..POST_FRAMES {
        if i > 0 {
            frames(&mut app, 1);
        }
        let s = Sample {
            t: app.clock().cur_time,
            target: brightness(&mut app, t_in),
            control: brightness(&mut app, c_in),
            lighting: world_lighting(&app, TARGET),
        };
        eprintln!(
            "selection_blink: post {i:2} +{:5.3}s target {:7.2} ({:+7.2}) control {:7.2} ({:+6.2}) lighting {:?} flip {}",
            s.t - t_click, s.target, s.target - base_t, s.control, s.control - base_c, s.lighting,
            app.interaction().selection_flip_count()
        );
        post.push(s);
    }
    assert_eq!(
        app.objects().world.selected,
        Some(TARGET),
        "still selected at the end"
    );
    assert_eq!(
        app.interaction().selection_flip_count(),
        0,
        "the world-selection blink count returns to zero"
    );

    // The control never moved: the blink is the target's, not the frame's.
    let worst_c = post
        .iter()
        .map(|s| (s.control - base_c).abs())
        .fold(0.0_f64, f64::max);
    assert!(
        worst_c <= tolerance,
        "the unclicked control stayed put: {worst_c:.2} > {tolerance:.2}"
    );
    // The control's lighting never moved either.
    assert_eq!(world_lighting(&app, CONTROL), RESTORED);

    // **The sequence.** bright, dim, bright, dim — 0.2 s each — then restored for the rest.
    let r = runs(&post);
    eprintln!("selection_blink: runs {r:?}");
    assert_runs(&r, &[BRIGHT, DIM, BRIGHT, DIM], "world");
    let last_restored = r.last().unwrap().1;
    assert!(
        last_restored >= 8,
        "at least 8 frames of the restored tail were sampled: {r:?}"
    );

    // **The pixels agree with the lighting, run by run.** Objects are lit, so the baseline is
    // `saturate(Ga + sun)` and not the texture's full white: BRIGHT (`Emissive 0.99`) reads
    // **above** it, and DIM (`Diffuse 0.35`, the ambient term remains 1.0) reads below BRIGHT by
    // far and at or below the baseline by a margin the sun's share of the lit colour decides.
    // The assertions require each BRIGHT run mean above baseline plus tolerance, each DIM run
    // mean below baseline plus tolerance and below the first BRIGHT mean by more than tolerance,
    // and both the restored run mean and every restored frame within tolerance. At least eight
    // restored samples remain.
    let bright_mean = r.iter().find(|x| x.0 == BRIGHT).map_or(base_t, |x| x.2);
    for (state, _, mean_px) in &r {
        let d = mean_px - base_t;
        if *state == BRIGHT {
            assert!(d > tolerance, "a BRIGHT run reads brighter than the baseline: {d:+.2} vs tolerance {tolerance:.2}; runs {r:?}");
        } else if *state == DIM {
            assert!(d < tolerance, "a DIM run stays below the baseline plus tolerance: {d:+.2} vs tolerance {tolerance:.2}; runs {r:?}");
            assert!(mean_px + tolerance < bright_mean, "a DIM run reads darker than the first BRIGHT run: {mean_px:.2} vs {bright_mean:.2}; runs {r:?}");
        } else {
            assert!(d.abs() <= tolerance, "the RESTORED tail is back at the baseline: {d:+.2} vs tolerance {tolerance:.2}; runs {r:?}");
        }
    }
    // And every restored frame individually — a single stray frame at the end would be a
    // one-frame re-flash, which retail does not have.
    for s in post.iter().filter(|s| s.lighting == RESTORED) {
        assert!(
            (s.target - base_t).abs() <= tolerance,
            "restored frame at +{:.3}s is back at the baseline: {:+.2}",
            s.t - t_click,
            s.target - base_t
        );
    }
}

// =================================================================================================
// (2) and (3): the paper doll.
// =================================================================================================

/// A breastplate in chest **armour** inventory location `0x0200`.
const ARMOUR: ObjectId = ObjectId(0x8000_1001);
/// A shirt: `INVENTORY_LOC` chest **clothing**, `0x0002`. Same click-map colour as the armour.
const SHIRT: ObjectId = ObjectId(0x8000_1002);
/// Click-map mask bit 2 selects part 9, the chest of the human setup.
const CHEST_PART: usize = 9;

fn gameplay_screen(
    app: &mut App,
) -> (
    &mut dereth_ui::UiSystem,
    &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
) {
    let shell = app.ui_mut().expect("the UI shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("gameplay screen");
    (ui, screen)
}

/// Open the inventory page through the current panel controller.
fn open_the_backpack(app: &mut App) {
    let (ui, screen) = gameplay_screen(app);
    let page = screen
        .panels
        .pages
        .iter()
        .find(|p| p.element == dereth_ui_screens::screens::gameplay::window::INVENTORY_PAGE)
        .copied()
        .expect("the inventory page is in the shipped panel stack");
    screen.recv_set_panel_visibility(ui, page.panel_id, true);
}

/// The first pixel of the click map painted in `mask`'s colour, in map coordinates.
fn click_map_pixel(map: &dereth_ui_screens::panels::inventory::ClickMap, mask: u32) -> (i32, i32) {
    for y in 0..map.height {
        for x in 0..map.width {
            if map.mask_at(x, y) == mask {
                return (x, y);
            }
        }
    }
    panic!("the shipped click map paints no pixel for mask {mask:#06X}");
}

/// Give the recorded player chest armour and clothing, then open the inventory page.
fn dress_and_open(app: &mut App, player: ObjectId) {
    use dereth_client_model::objects::{InventoryPlacement, ObjectInventory};
    {
        let world = &mut app.objects_mut().world;
        assert_eq!(
            world.player,
            Some(player),
            "the recorded login named the player"
        );
        let mut inv = ObjectInventory::new(player);
        inv.placements = vec![
            InventoryPlacement {
                iid: ARMOUR,
                loc: 0x0000_0200,
                priority: 0,
            },
            InventoryPlacement {
                iid: SHIRT,
                loc: 0x0000_0002,
                priority: 0,
            },
        ];
        world.tables.inventories.insert(player, inv);
    }
    open_the_backpack(app);
    frames(app, 10);
}

/// Return the doll viewport's screen box and the screen point of the click map's chest pixel. The
/// point must hit the shipped paper-doll drag-mask element `0x100001D6`.
fn doll_geometry(app: &mut App) -> ((i32, i32, i32, i32), i32, i32) {
    use dereth_ui::framework::Screen;
    use dereth_ui_screens::panels::inventory::PAPER_DOLL_DRAG_MASK;
    let (ui, screen) = gameplay_screen(app);
    assert!(
        !screen.inventory.slots_view,
        "doll mode, not the Slots workaround"
    );
    let h = screen
        .inventory
        .paper_doll_viewport
        .expect("the paper-doll viewport element 0x100001D5");
    let b = ui.screen_clip_box(h);
    assert!(b.is_valid(), "the doll viewport is on screen: {b:?}");
    let map = screen
        .inventory
        .click_map
        .clone()
        .expect("the shipped paper-doll click-map surface must load");
    let (mx, my) = click_map_pixel(&map, 0x0202);
    let root = *screen.roots().first().expect("root");
    let mask_h = ui
        .get_child_recursive(root, PAPER_DOLL_DRAG_MASK)
        .expect("the paper-doll drag-mask element 0x100001D6 is in the shipped layout");
    let (x0, y0) = ui.screen_origin(mask_h);
    let (x, y) = (x0 + mx, y0 + my);
    let hit = ui
        .hit_test_screen(x, y)
        .expect("the doll's body is under the cursor");
    assert!(
        hit == mask_h || ui.is_ancestor_of(mask_h, hit),
        "the press must reach paper-doll drag-mask element 0x100001D6, not a neighbour"
    );
    ((b.x0, b.y0, b.x1, b.y1), x, y)
}

/// A baseline of the doll region: twelve frames, the mean and the tolerance.
fn doll_baseline(app: &mut App, region: (i32, i32, i32, i32), what: &str) -> (f64, f64) {
    let mut pre = Vec::with_capacity(PRE_FRAMES);
    for i in 0..PRE_FRAMES {
        frames(app, 1);
        let v = brightness(app, region);
        let l = doll_lighting(app);
        eprintln!(
            "selection_blink: {what} pre {i:2} {v:7.2} lit parts {}",
            l.iter().filter(|p| **p != RESTORED).count()
        );
        assert!(
            l.iter().all(|p| *p == RESTORED),
            "nothing lit on the doll before the click"
        );
        pre.push(v);
    }
    let base = pre.iter().sum::<f64>() / pre.len() as f64;
    let var = pre.iter().map(|x| (x - base).abs()).fold(0.0_f64, f64::max);
    let tolerance = 3.0 * var + 2.0;
    eprintln!("selection_blink: {what} baseline {base:.2} (±{var:.2}) tolerance {tolerance:.2}");
    assert!(
        base > 2.0,
        "the doll is actually drawn in its viewport: {base}"
    );
    (base, tolerance)
}

/// Sample the doll region for `POST_FRAMES` frames with the lighting of `part` beside each pixel
/// mean. Judge the `BRIGHT`, `DIM`, and `RESTORED` run means; this helper does not assert each
/// restored frame or a minimum restored-tail length.
fn sample_doll_blink(
    app: &mut App,
    region: (i32, i32, i32, i32),
    part: usize,
    base: f64,
    tolerance: f64,
    t0: f64,
    what: &str,
) {
    let mut post = Vec::with_capacity(POST_FRAMES);
    for i in 0..POST_FRAMES {
        if i > 0 {
            frames(app, 1);
        }
        let l = doll_lighting(app);
        let s = Sample {
            t: app.clock().cur_time,
            target: brightness(app, region),
            control: 0.0,
            lighting: l[part],
        };
        eprintln!(
            "selection_blink: {what} post {i:2} +{:5.3}s {:7.2} ({:+7.2}) part {part} {:?} lit parts {}",
            s.t - t0, s.target, s.target - base, s.lighting,
            l.iter().filter(|p| **p != RESTORED).count()
        );
        if let Ok(dir) = std::env::var("DERETH_TEST_SELECTION_BLINK_SHOTS") {
            let _ = std::fs::create_dir_all(&dir);
            let _ = app
                .renderer_mut()
                .capture_png(&std::path::Path::new(&dir).join(format!("{what}_post_{i:02}.png")));
        }
        post.push(s);
    }
    let r = runs(&post);
    eprintln!("selection_blink: {what} runs {r:?}");
    assert_runs(&r, &[BRIGHT, DIM], what);
    // The same reading as the world half (see `BRIGHT_READS`), with one difference made by the
    // doll's own distant light: its intensity 2.0 gives a Diffuse of (2, 2, 2), so every face
    // turned even a third of the way toward it is already `saturate`d to the texture's full
    // colour, and `Emissive 0.99` cannot lift those pixels further (measured: chest baseline
    // 27.77, BRIGHT 28.82, DIM 25.54). The doll's BRIGHT is therefore "not darker", the DIM
    // below BRIGHT; the legs of a self-selection (13.17 -> 15.32) happen to clear the
    // tolerance, the breastplate does not.
    let bright_mean = r.iter().find(|x| x.0 == BRIGHT).map_or(base, |x| x.2);
    for (state, _, mean_px) in &r {
        let d = mean_px - base;
        if *state == BRIGHT {
            assert!(d >= -tolerance, "{what}: the BRIGHT run reads no darker than the baseline: {d:+.2} vs tolerance {tolerance:.2}; runs {r:?}");
        } else if *state == DIM {
            assert!(d < tolerance, "{what}: the DIM run stays below the baseline plus tolerance: {d:+.2} vs tolerance {tolerance:.2}; runs {r:?}");
            assert!(mean_px + tolerance < bright_mean, "{what}: the DIM run reads darker than the BRIGHT run: {mean_px:.2} vs {bright_mean:.2}; runs {r:?}");
        } else {
            assert!(d.abs() <= tolerance, "{what}: the RESTORED tail is back at the baseline: {d:+.2} vs tolerance {tolerance:.2}; runs {r:?}");
        }
    }
}

/// **The acceptance, doll half.** A press through the input path on the doll's chest resolves the
/// chest click-map entry and selects the breastplate. Immediately afterward only bit 2, part 9, is
/// lit. After the pointer helper advances two more frames away from the doll, sampling follows part
/// 9 through its bright, dim, and restored runs.
#[test]
fn a_worn_item_clicked_on_the_doll_blinks_its_region_bright_then_dim() {
    let (mut app, player) = setup();
    dress_and_open(&mut app, player);
    let (doll_box, press_x, press_y) = doll_geometry(&mut app);
    // The chest: from the click map's chest pixel down a fifth of the viewport, the inner 40 %
    // of the columns.
    let region = {
        let (x0, _y0, x1, y1) = doll_box;
        let (w, h) = (x1 - x0, y1 - doll_box.1);
        (
            x0 + w * 3 / 10,
            press_y + 2,
            x1 - w * 3 / 10,
            (press_y + 2 + h / 5).min(y1),
        )
    };
    eprintln!("selection_blink: doll viewport {doll_box:?} chest region {region:?} press ({press_x}, {press_y})");
    let (base, tolerance) = doll_baseline(&mut app, region, "doll");

    assert_eq!(
        app.objects().world.selected,
        None,
        "nothing selected to begin with"
    );
    press_at(&mut app, press_x, press_y, 5_000);
    assert_eq!(
        app.objects().world.selected,
        Some(ARMOUR),
        "the doll click-map route selects the chest armour"
    );
    let t0 = app.clock().cur_time;
    // Only the chest is lit, and nothing else on the doll.
    let l = doll_lighting(&app);
    let lit: Vec<usize> = (0..l.len()).filter(|i| l[*i] != RESTORED).collect();
    assert_eq!(
        lit,
        vec![CHEST_PART],
        "selection mask bit 2 lights chest part 9: {l:?}"
    );
    // Take the cursor off the drag mask so its hover tooltip never reaches the region. The doll's
    // blink belongs to the doll and is not cancelled by hover; only the world blink is.
    move_to(&mut app, 4, 4, 5_100);
    sample_doll_blink(&mut app, region, CHEST_PART, base, tolerance, t0, "doll");
    assert_eq!(
        app.objects().world.selected,
        Some(ARMOUR),
        "still selected at the end"
    );
    // The world picker saw no find: a doll click selects through the doll element itself.
    assert_eq!(app.interaction().selection_flip_count(), 0);
}

/// Project a point already in viewer space to its viewport pixel.
fn pixel_of(local: Vec3, viewport: (u32, u32), fov_y_rad: f32) -> (f32, f32) {
    let half_w = (viewport.0 as f32 - 1.0) * 0.5;
    let half_h = (viewport.1 as f32 - 1.0) * 0.5;
    let vdst = half_h / math::tanf(fov_y_rad * 0.5);
    let k = vdst / local.y;
    (half_w + local.x * k, half_h - local.z * k)
}

/// Project a world point to its viewport pixel and check it against the pick's `selection_ray`.
fn aim(app: &App, world: Vec3) -> (i32, i32) {
    let scene = app.world_scene().unwrap();
    let viewer = PickScene::viewer(&scene);
    let fov = PickScene::fov_y_rad(&scene, SCREEN);
    let local = dereth_physics::math::globaltolocal(&viewer, world);
    assert!(
        local.y > 0.5,
        "the point is in front of the chase camera: {local:?}"
    );
    let (px, py) = pixel_of(local, SCREEN, fov);
    let ray = selection_ray(&viewer, px, py, SCREEN, fov);
    let to = Vec3::new(
        world.x - viewer.origin.x,
        world.y - viewer.origin.y,
        world.z - viewer.origin.z,
    );
    let len = (to.x * to.x + to.y * to.y + to.z * to.z).sqrt();
    let cos = (ray.x * to.x + ray.y * to.y + ray.z * to.z) / len;
    assert!(
        cos > 0.9999,
        "the pick ray through ({px}, {py}) points at the point: cos = {cos}"
    );
    (px.round() as i32, py.round() as i32)
}

/// **The acceptance, self half.** A press through the input path on the local body selects the
/// player. Its selection mask is `0x7FFFFFFF`; the immediate post-press check requires every
/// available part to be bright and confirms that the setup includes chest part 9. Sampling then
/// follows part 0 through bright, dim, and restored runs over the doll's legs, which no worn-item
/// mask reaches.
/// The world picker lights the local world body too, which is asserted through the lighting bound
/// by the draw.
#[test]
fn selecting_yourself_blinks_the_whole_doll() {
    let (mut app, player) = setup();
    dress_and_open(&mut app, player);
    let (doll_box, _press_x, _press_y) = doll_geometry(&mut app);
    // The doll's legs: rows 55 %..85 % of the viewport, the inner 40 % of columns.
    let region = {
        let (x0, y0, x1, y1) = doll_box;
        let (w, h) = (x1 - x0, y1 - y0);
        (
            x0 + w * 3 / 10,
            y0 + h * 55 / 100,
            x1 - w * 3 / 10,
            y0 + h * 85 / 100,
        )
    };
    eprintln!("selection_blink: doll viewport {doll_box:?} legs region {region:?}");
    let (base, tolerance) = doll_baseline(&mut app, region, "self");

    // Where the body's torso is on screen, one metre above the feet.
    let chest_px = {
        let scene = app.world_scene().unwrap();
        let (root, _parts) = scene.character_frames().expect("the local body's frames");
        aim(
            &app,
            Vec3::new(root.origin.x, root.origin.y, root.origin.z + 1.0),
        )
    };
    eprintln!("selection_blink: self chest px {chest_px:?}");
    assert_eq!(
        app.objects().world.selected,
        None,
        "nothing selected to begin with"
    );
    press_at(&mut app, chest_px.0, chest_px.1, 5_000);
    assert_eq!(
        app.objects().world.selected,
        Some(player),
        "the press at {chest_px:?} reached world-object lookup and selected the player"
    );
    let t0 = app.clock().cur_time;
    // The whole doll, every part.
    let l = doll_lighting(&app);
    assert!(
        l.len() > CHEST_PART,
        "the human setup includes chest part 9: {}",
        l.len()
    );
    assert!(
        l.iter().all(|p| *p == BRIGHT),
        "the all-parts selection mask lights every part: {l:?}"
    );
    // The world body is lit through the world-pick path as well.
    assert_eq!(
        world_lighting(&app, player),
        BRIGHT,
        "the world body is lit by the world-pick notice"
    );
    assert_eq!(
        app.interaction().selection_flip_count(),
        1,
        "the world-selection blink count starts at one"
    );

    sample_doll_blink(&mut app, region, 0, base, tolerance, t0, "self");
    assert_eq!(
        app.objects().world.selected,
        Some(player),
        "still selected at the end"
    );
    // The world body must be restored by the end of the 1.2-second sampling window; this assertion
    // does not measure the exact frame on which restoration occurred.
    assert_eq!(
        world_lighting(&app, player),
        RESTORED,
        "the world body is restored at the end of the 36-sample window"
    );
    assert_eq!(app.interaction().selection_flip_count(), 0);
}
