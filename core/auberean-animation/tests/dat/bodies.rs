//! The player's human male (visual description `0x1F000023`) put together and animated.

use auberean_animation::body::{slider, Body, MOVE};
use auberean_animation::graph::period;
use auberean_animation::skin::{influences, skin};
use auberean_animation::CachedRecords;
use auberean_assets::mesh::Mesh;
use auberean_primitives::{DataId, Vec3};

use crate::common::need_records;

const HUMAN_MALE: DataId = DataId(0x1F00_0023);
/// A behavior table other than the human male's.
const OTHER_TABLE: DataId = DataId(0x4400_0015);

/// The human male puts together from its visual description: the body group with its 41-bone
/// skeleton and a head hung from the body's head connection; its behavior table plays moving in
/// its first mode, and its moving animation's cycle shortens as it speeds up from a walk to a run.
///
/// Behaviour: animation.body.the-human-male-puts-together-and-moves
#[test]
fn the_human_male_puts_together_and_moves() {
    let w = need_records!();
    let recs = CachedRecords::new(w);
    let mut body = Body::assemble(&recs, HUMAN_MALE).expect("the human male");
    let body_part = &body.parts[0];
    assert_eq!(body_part.setup.bones.len(), 41, "the body skeleton");
    assert!(body_part.skeleton.is_some(), "a rest pose");
    assert!(
        body.parts.iter().any(|p| p.parent.is_some()),
        "a group hung from the body"
    );
    let (key, layer) = body
        .combo(MOVE)
        .expect("moving has an animation in the first mode");
    eprintln!(
        "parts {:?}, element {:#x} mode {:#x}, move key {key:#x} layer {layer}, tracks {:?}",
        body.parts
            .iter()
            .map(|p| (p.key, p.setup.bones.len(), p.anim_map.len()))
            .collect::<Vec<_>>(),
        body.element,
        body.mode,
        body.modes
            .from
            .as_ref()
            .map(|t| t.tracks.keys().collect::<Vec<_>>())
    );
    let id = body.parts[0].anim_map[&key];
    let walk = {
        body.sliders.set(slider::FORWARD, 2.2);
        period(&recs, id, &body_state(&body))
    };
    let run = {
        body.sliders.set(slider::FORWARD, 6.5);
        period(&recs, id, &body_state(&body))
    };
    eprintln!("moving {id:?}: period walking {walk} s, running {run} s");
    assert!(walk > run && run > 0.0, "{walk} then {run}");
    body.set_motion(Vec3::new(0.0, 6.5, 0.0), 0.0, Vec3::new(1.0, 1.0, 1.0));
    for _ in 0..30 {
        body.advance(&recs, 1.0 / 30.0);
    }
    assert!(
        (body.sliders.get(slider::FORWARD) - 6.5).abs() < 1e-3,
        "the forward slider reaches the speed"
    );
}

/// The human male moving takes a full wound (value 2 of the wounded effect's four appearance
/// tables laid over its own description) and keeps what it plays: brought up to date with the
/// total, every part keeps its setup and animation map, moving plays on from where it was, and only
/// what the appearance does to its parts changes, to what the total says.
///
/// Behaviour: animation.body.a-changed-look-keeps-what-the-body-plays
#[test]
fn a_wounded_human_male_moves_on_in_its_new_look() {
    use auberean_assets::fx::AppearanceKey;
    use auberean_world::look::{Desc, LookRecords, ObjectLook, PartLook};
    let w = need_records!();
    let recs = CachedRecords::new(w);
    let mut body = Body::assemble(&recs, HUMAN_MALE).expect("the human male");
    assert!(body.do_autonomous_behavior(&recs, MOVE), "moving plays");
    body.set_motion(Vec3::new(0.0, 2.2, 0.0), 0.0, Vec3::new(1.0, 1.0, 1.0));
    for _ in 0..20 {
        body.advance(&recs, 1.0 / 30.0);
    }
    let time = |b: &Body| {
        b.modes
            .from
            .as_ref()
            .and_then(|t| t.tracks.get(MOVE))
            .map(|t| t.time)
    };
    let before = time(&body).expect("moving");
    let setups: Vec<_> = body.parts.iter().map(|p| p.setup.clone()).collect();
    let looks: Vec<PartLook> = body.parts.iter().map(|p| p.look.clone()).collect();
    let v = recs.visual(HUMAN_MALE).expect("its description");
    let mut dress = ObjectLook::new(&Desc::from_stored(&v));
    for table in [0x2000_01CD, 0x2000_003A, 0x2000_01CE, 0x2000_0056] {
        let key = AppearanceKey {
            key: 2,
            scalar: 1.0,
        };
        assert!(
            dress.do_appearance_fx(&recs, table, key),
            "{table:#x} laid on"
        );
    }
    let total = dress.total(&recs).expect("the total");
    assert!(body.compare_and_modify(&recs, &total), "brought up to date");
    assert_eq!(body.parts.len(), setups.len());
    assert!(
        body.parts
            .iter()
            .zip(&setups)
            .all(|(p, s)| std::sync::Arc::ptr_eq(&p.setup, s)),
        "every part kept"
    );
    assert!(body.is_doing_behavior(MOVE), "still moving");
    assert_eq!(time(&body), Some(before), "from where it was");
    let now: Vec<PartLook> = body.parts.iter().map(|p| p.look.clone()).collect();
    assert_ne!(now, looks, "its parts look otherwise");
    let said: Vec<PartLook> = body
        .parts
        .iter()
        .map(|p| {
            total
                .atoms(&recs, p.key)
                .map(PartLook::from_atoms)
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(now, said, "as the total says");
    body.advance(&recs, 1.0 / 30.0);
    assert!(time(&body) > Some(before), "and moves on");
}

fn body_state(b: &Body) -> auberean_animation::GraphState<'_> {
    auberean_animation::GraphState {
        sliders: &b.sliders,
        mode: b.mode,
        element: b.element,
        terrain: b.terrain,
    }
}

/// Posed at rest, every skinned vertex of the human male's body stays where it was modeled; posed
/// running, its feet move.
///
/// Behaviour: animation.skin.a-body-at-rest-skins-to-its-model
#[test]
fn a_body_at_rest_skins_to_its_model() {
    let w = need_records!();
    let recs = CachedRecords::new(w);
    let body = Body::assemble(&recs, HUMAN_MALE).expect("the human male");
    let part = &body.parts[0];
    let skel = part.skeleton.as_ref().expect("a rest pose");
    let rest = skel.palette(&skel.rest_world);
    let mut checked = 0;
    for mesh_id in &part.setup.meshes {
        let Some(mesh) = w.get::<Mesh>(mesh_id.0) else {
            continue;
        };
        let Some(inf) = influences(&mesh.vertices) else {
            continue;
        };
        let pos: Vec<Vec3> = positions(&mesh);
        let normals = vec![Vec3::ZERO; pos.len()];
        let (moved, _) = skin(&pos, &normals, &inf, &rest);
        for (a, b) in pos.iter().zip(&moved) {
            let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
            assert!(d < 1e-3, "{mesh_id:?}: {a:?} moved to {b:?}");
            checked += 1;
        }
    }
    assert!(checked > 1000, "{checked} vertices");
}

fn positions(m: &Mesh) -> Vec<Vec3> {
    let stride = auberean_assets::mesh::vertex_size(m.vertices.format);
    m.vertices
        .bytes
        .chunks_exact(stride)
        .map(|b| {
            let f = |k: usize| f32::from_le_bytes([b[k], b[k + 1], b[k + 2], b[k + 3]]);
            Vec3::new(f(0), f(4), f(8))
        })
        .collect()
}

/// The human male starts in its table's initial mode playing the table's initial behaviors
/// (its idles among them); moving plays its locomotion blend on top; asked for another mode it
/// reaches, it walks the table's mode graph a step at a time, blending from one mode's behaviors
/// to the next, and whoever asked hears when it is there.
///
/// Behaviour: animation.modes.a-body-changes-mode-along-its-table
#[test]
fn the_human_male_changes_mode_along_its_table() {
    let w = need_records!();
    let recs = CachedRecords::new(w);
    let mut body = Body::assemble(&recs, HUMAN_MALE).expect("the human male");
    assert!(body.template.is_some(), "the character template");
    assert!(body.modes.can_animate, "a behavior table");
    let table = body.table.clone().expect("a table");
    assert_eq!(body.mode, table.initial_mode, "the table's initial mode");
    for &b in &table.initial_behaviors {
        assert!(
            body.is_doing_behavior(b) || body.combo(b).is_none(),
            "initial behavior {b:#x} plays"
        );
    }
    assert!(body.do_autonomous_behavior(&recs, MOVE), "moving plays");
    body.set_motion(Vec3::new(0.0, 6.5, 0.0), 0.0, Vec3::new(1.0, 1.0, 1.0));
    for _ in 0..30 {
        body.advance(&recs, 1.0 / 30.0);
    }
    assert!(body.is_doing_behavior(MOVE));
    assert!(
        (body.sliders.current(slider::FORWARD) - 6.5).abs() < 1e-3,
        "the forward slider reaches the speed"
    );
    let poses = body.pose(&recs);
    assert!(poses.iter().all(|p| p.bones.iter().all(|m| m
        .0
        .iter()
        .flatten()
        .all(|v| v.is_finite()))));
    // Another mode of the same element the table has a way to.
    let here = body.modes.mode_state();
    let target = table
        .modes
        .iter()
        .map(|(k, _)| *k)
        .find(|k| {
            k.element == here.element
                && k.mode != here.mode
                && auberean_animation::queries::next_step(
                    &table,
                    &auberean_assets::animation::ModeChange { from: here, to: *k },
                )
                .is_some()
        })
        .expect("another mode with a way there");
    body.take_events();
    assert!(body.do_mode(&recs, 7, target.mode), "a way there");
    let mut steps = 0;
    let mut arrived = false;
    for _ in 0..600 {
        if body.modes.to.is_some() {
            steps += 1;
        }
        body.advance(&recs, 1.0 / 30.0);
        if body
            .take_completions()
            .contains(&auberean_animation::host::Completion {
                context: 7,
                error: 0,
                waited: auberean_animation::host::Waited::Mode,
            })
        {
            arrived = true;
            break;
        }
    }
    eprintln!(
        "from {:#x} to {:#x}: {steps} frames changing, now {:#x}",
        here.mode, target.mode, body.mode
    );
    assert!(arrived, "whoever asked hears the body is there");
    assert_eq!(body.mode, target.mode);
    assert!(steps > 0, "the change blends over frames");
}

/// A behavior table the look changes to while a behavior locks the player's actions drops that
/// behavior and asks nothing of the lock: only a locking behavior's own end lets go, so the next
/// one to end releases it.
///
/// Behaviour: animation.body.a-new-behavior-table-leaves-an-action-lock-set
#[test]
fn a_new_behavior_table_leaves_an_action_lock_set() {
    use auberean_animation::host::Event;
    use auberean_animation::track::Params;
    use auberean_world::look::{Desc, LookRecords, ObjectLook};
    let locks = |b: &mut Body| {
        let events = b.take_events();
        (
            events.iter().any(|e| matches!(e, Event::LockInput)),
            events.iter().any(|e| matches!(e, Event::UnlockInput)),
        )
    };
    let locking = Params {
        behavior: MOVE,
        lock_actions: true,
        ..Params::default()
    };
    let w = need_records!();
    let recs = CachedRecords::new(w);
    let mut body = Body::assemble(&recs, HUMAN_MALE).expect("the human male");
    body.is_player = true;
    locks(&mut body);
    assert!(body.do_behavior(&recs, &locking), "it plays");
    assert_eq!(locks(&mut body), (true, false), "and locks");
    let v = recs.visual(HUMAN_MALE).expect("its description");
    let mut look = ObjectLook::new(&Desc::from_stored(&v))
        .total(&recs)
        .expect("the total");
    assert_ne!(look.behavior_table, OTHER_TABLE);
    look.behavior_table = OTHER_TABLE;
    assert!(body.compare_and_modify(&recs, &look), "brought up to date");
    let still = body
        .modes
        .from
        .as_ref()
        .and_then(|t| t.track(MOVE))
        .is_some_and(|t| t.params.lock_actions);
    assert!(!still, "the locking behavior went with the old table");
    assert_eq!(locks(&mut body), (false, false), "and the lock stays");
    assert!(body.do_behavior(&recs, &locking), "the next one plays");
    assert_eq!(locks(&mut body), (true, false), "and locks");
    body.stop_behavior(&recs, MOVE);
    assert_eq!(locks(&mut body), (false, true), "its end lets go");
}
