//! Behaviour: none (experimental)
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use super::super::Skins;
use super::{CORPSE_REACH, CORPSE_WITHIN, GHOST_MOST};
use crate::assets::skin::{Bone, Clip, Rig, SkinModel, Slot, BONE};
use crate::assets::{Mesh, Subset, SurfaceRef, Vertex};
use crate::scene::{
    FrameDelta, Motion, Object, ObjectKey, ObjectPart, ObjectPose, PartPose, SceneDelta,
};

const SETUP: u32 = 0x0200_0001;
const CELL: u32 = 0xA9B3_0001;

/// A body of one bone drawn in place of part 0, standing in its idle and dying over
/// `die` seconds.
fn body(skin: u32, die: f32) -> Arc<SkinModel> {
    let v = Vertex {
        position: [0.0, 0.0, -0.8],
        normal: [0.0, 1.0, 0.0],
        uv: [0.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![v, v, v],
        indices: vec![0, 1, 2],
        subsets: vec![Subset {
            first_index: 0,
            index_count: 3,
            surface: SurfaceRef::Surface(0x0800_0001),
            two_sided: false,
            tiled: true,
        }],
        ..Mesh::default()
    };
    let turned = |deg: f32| vec![(Quat::from_rotation_x(deg.to_radians()), Vec3::ZERO)];
    let clip = |from: f32, to: f32, seconds: f32| Clip {
        steps: Arc::new(vec![turned(from), turned(to)]),
        scales: Arc::new(Vec::new()),
        seconds,
        stride: None,
        hit: None,
    };
    Arc::new(
        SkinModel::new(
            skin,
            mesh,
            vec![[BONE, 0, 0, 0]; 3],
            vec![[1.0, 0.0, 0.0, 0.0]; 3],
            &[(0, Mat4::IDENTITY)],
        )
        .expect("a body")
        .with_rig(Rig {
            bones: vec![Bone::new(Mat4::IDENTITY)],
            clips: vec![
                (Slot::Idle, clip(0.0, 0.0, 1.0)),
                (Slot::Die, clip(0.0, 90.0, die)),
            ],
            ..Rig::default()
        }),
    )
}

fn object(id: u32, setup: u32) -> SceneDelta {
    SceneDelta::ObjectBuilt(Box::new(Object {
        key: ObjectKey::Server(id),
        setup,
        parts: vec![ObjectPart {
            levels: vec![0x0100_0010],
            look: None,
        }],
        palettes: Vec::new(),
        who: crate::scene::Who::default(),
    }))
}

/// The object `id` standing at `(x, y)`, its motion filling `slot` (its parts on their way to
/// their cycle while it dies).
fn pose(id: u32, (x, y): (f32, f32), slot: Slot) -> ObjectPose {
    let at = Mat4::from_translation(Vec3::new(x, y, 0.0));
    ObjectPose {
        key: ObjectKey::Server(id),
        cell: CELL,
        parts: vec![PartPose {
            transform: at,
            previous: at,
            level: 0,
            hidden: false,
            material: None,
        }],
        held_by: None,
        motion: Some(Motion {
            slot: Some(slot),
            idle: Slot::Idle,
            reversed: false,
            settled: slot != Slot::Die,
            phase: 0.0,
            until_cycle: 0.0,
            until_hit: None,
            queued: 0,
            keyframed: false,
            frame: at,
            velocity: Vec3::ZERO,
        }),
    }
}

fn frame(clock: f64, objects: Vec<ObjectPose>) -> FrameDelta {
    let drawn_parts = objects
        .iter()
        .map(|o| crate::scene::DrawnPart {
            key: o.key,
            part: 0,
            outdoors: true,
            lights: [0; 8],
            count: 0,
        })
        .collect();
    FrameDelta {
        clock,
        origin: [0xA9, 0xB3],
        objects,
        drawn_parts,
        ..FrameDelta::default()
    }
}

/// One frame as the renderer takes it: the changes it lets through, and the frame it draws.
fn step(
    skins: &mut Skins,
    deltas: Vec<SceneDelta>,
    mut frame: FrameDelta,
    bodies: &dyn Fn(u32) -> Option<Arc<SkinModel>>,
) -> (Vec<SceneDelta>, FrameDelta) {
    let deltas = skins.haunt(deltas, &frame, true);
    let deltas = skins.swap(deltas, &mut |setup, _, _| bodies(setup));
    skins.with_ghosts(&mut frame);
    skins.pose(&frame);
    skins.remember(&frame);
    (deltas, frame)
}

fn removed(deltas: &[SceneDelta], id: u32) -> bool {
    deltas.contains(&SceneDelta::ObjectRemoved(ObjectKey::Server(id)))
}

fn shown(frame: &FrameDelta, id: u32) -> Option<bool> {
    frame
        .objects
        .iter()
        .find(|o| o.key == ObjectKey::Server(id))
        .map(|o| o.parts.iter().all(|p| !p.hidden))
}

/// How far through its clip the body of `id` was drawn, and in which slot.
fn playing(skins: &Skins, id: u32) -> Option<(Option<Slot>, f32)> {
    skins
        .playing()
        .into_iter()
        .find(|p| p.0 == ObjectKey::Server(id))
        .map(|p| (p.1, p.2))
}

/// A creature at `(x, y)` built at 0, standing until `dies`, dying from then on, its object
/// left at `leaves`: the changes and frames from its build up to the frame it leaves in.
struct Run {
    skins: Skins,
    bodies: Box<dyn Fn(u32) -> Option<Arc<SkinModel>>>,
}

impl Run {
    fn new(die: f32) -> Self {
        let b = body(0, die);
        Self {
            skins: Skins::default(),
            bodies: Box::new(move |setup| (setup == SETUP).then(|| Arc::clone(&b))),
        }
    }

    fn step(
        &mut self,
        deltas: Vec<SceneDelta>,
        frame: FrameDelta,
    ) -> (Vec<SceneDelta>, FrameDelta) {
        step(&mut self.skins, deltas, frame, &*self.bodies)
    }

    /// Creature 7 built at 0 at the origin, standing, then dying from 0.1 to `until`, in
    /// tenths of a second.
    fn dying(&mut self, until: f64) {
        self.step(
            vec![object(7, SETUP)],
            frame(0.0, vec![pose(7, (0.0, 0.0), Slot::Idle)]),
        );
        let mut t = 0.1;
        while t <= until + 1e-9 {
            self.step(Vec::new(), frame(t, vec![pose(7, (0.0, 0.0), Slot::Die)]));
            t += 0.1;
        }
    }
}

#[test]
fn a_body_still_dying_when_its_object_leaves_plays_its_death_to_the_end_then_goes() {
    let mut run = Run::new(2.0);
    run.dying(0.5);
    let (out, f) = run.step(
        vec![SceneDelta::ObjectRemoved(ObjectKey::Server(7))],
        frame(0.6, Vec::new()),
    );
    assert!(!removed(&out, 7), "its removal is held back");
    assert_eq!(shown(&f, 7), Some(true), "it stands in the frame");
    assert!(
        f.drawn_parts.iter().any(|d| d.key == ObjectKey::Server(7)),
        "and is drawn"
    );
    assert_eq!(run.skins.ghosts().0, [ObjectKey::Server(7)]);
    let (slot, share) = playing(&run.skins, 7).expect("posed");
    assert_eq!(slot, Some(Slot::Die));
    assert!((share - 0.25).abs() < 1e-3, "{share}");
    // Its death plays on.
    run.step(Vec::new(), frame(1.6, Vec::new()));
    let (_, share) = playing(&run.skins, 7).expect("posed");
    assert!((share - 0.75).abs() < 1e-3, "{share}");
    let (out, _) = run.step(Vec::new(), frame(2.0, Vec::new()));
    assert!(!removed(&out, 7), "not yet at its end");
    // Its death started at 0.1 and lasts two seconds: it goes once it has played.
    let (out, f) = run.step(Vec::new(), frame(2.15, Vec::new()));
    assert!(removed(&out, 7), "gone at its death's end");
    assert_eq!(shown(&f, 7), None);
    assert!(run.skins.ghosts().0.is_empty());
    assert_eq!(run.skins.objects(), 0);
}

#[test]
fn an_object_that_leaves_standing_or_with_its_death_played_goes_at_once() {
    let mut run = Run::new(2.0);
    run.step(
        vec![object(7, SETUP)],
        frame(0.0, vec![pose(7, (0.0, 0.0), Slot::Idle)]),
    );
    let (out, f) = run.step(
        vec![SceneDelta::ObjectRemoved(ObjectKey::Server(7))],
        frame(0.1, Vec::new()),
    );
    assert!(removed(&out, 7), "standing");
    assert_eq!(shown(&f, 7), None);
    let mut run = Run::new(0.3);
    run.dying(0.6);
    let (out, _) = run.step(
        vec![SceneDelta::ObjectRemoved(ObjectKey::Server(7))],
        frame(0.7, Vec::new()),
    );
    assert!(removed(&out, 7), "its death already played");
}

#[test]
fn a_ghost_lives_no_longer_than_its_cap_however_long_its_death() {
    let mut run = Run::new(30.0);
    run.dying(0.5);
    run.step(
        vec![SceneDelta::ObjectRemoved(ObjectKey::Server(7))],
        frame(0.6, Vec::new()),
    );
    let cap = 0.6 + f64::from(GHOST_MOST);
    let (out, _) = run.step(Vec::new(), frame(cap - 0.05, Vec::new()));
    assert!(!removed(&out, 7));
    let (out, _) = run.step(Vec::new(), frame(cap, Vec::new()));
    assert!(removed(&out, 7), "ended at the cap");
}

#[test]
fn a_ghost_ends_at_once_on_a_teleport_its_block_leaving_or_its_key_built_again() {
    let leave = |run: &mut Run| {
        run.dying(0.5);
        let (out, _) = run.step(
            vec![SceneDelta::ObjectRemoved(ObjectKey::Server(7))],
            frame(0.6, Vec::new()),
        );
        assert!(!removed(&out, 7), "a ghost first");
    };
    let mut run = Run::new(2.0);
    leave(&mut run);
    let (out, f) = run.step(
        Vec::new(),
        FrameDelta {
            teleports: 1,
            ..frame(0.7, Vec::new())
        },
    );
    assert!(removed(&out, 7), "a teleport");
    assert_eq!(shown(&f, 7), None);

    let mut run = Run::new(2.0);
    leave(&mut run);
    let (out, _) = run.step(vec![SceneDelta::BlockOut(0xA9B3)], frame(0.7, Vec::new()));
    assert!(removed(&out, 7), "its block left");

    let mut run = Run::new(2.0);
    leave(&mut run);
    let (out, f) = run.step(
        vec![object(7, SETUP)],
        frame(0.7, vec![pose(7, (0.0, 0.0), Slot::Idle)]),
    );
    assert_eq!(
        out.first(),
        Some(&SceneDelta::ObjectRemoved(ObjectKey::Server(7))),
        "the ghost goes before the object built again"
    );
    assert!(matches!(out.get(1), Some(SceneDelta::ObjectBuilt(_))));
    assert_eq!(f.objects.len(), 1, "the object, no ghost beside it");
    assert!(run.skins.ghosts().0.is_empty());

    // A teleport between its last dying frame and its leaving makes no ghost at all.
    let mut run = Run::new(2.0);
    run.dying(0.5);
    let (out, _) = run.step(
        vec![SceneDelta::ObjectRemoved(ObjectKey::Server(7))],
        FrameDelta {
            teleports: 1,
            ..frame(0.6, Vec::new())
        },
    );
    assert!(removed(&out, 7));
}

#[test]
fn a_ghost_stays_where_it_stood_when_the_frames_origin_moves() {
    let mut run = Run::new(2.0);
    run.step(
        vec![object(7, SETUP)],
        frame(0.0, vec![pose(7, (10.0, 20.0), Slot::Idle)]),
    );
    run.step(
        Vec::new(),
        frame(0.1, vec![pose(7, (10.0, 20.0), Slot::Die)]),
    );
    run.step(
        vec![SceneDelta::ObjectRemoved(ObjectKey::Server(7))],
        frame(0.2, Vec::new()),
    );
    let (_, f) = run.step(
        Vec::new(),
        FrameDelta {
            origin: [0xAA, 0xB3],
            ..frame(0.3, Vec::new())
        },
    );
    let g = f
        .objects
        .iter()
        .find(|o| o.key == ObjectKey::Server(7))
        .expect("the ghost");
    let at = g.parts[0].transform.w_axis.truncate();
    assert!(
        at.distance(Vec3::new(10.0 - 192.0, 20.0, 0.0)) < 1e-4,
        "{at}"
    );
    let m = g.motion.expect("its motion").frame.w_axis.truncate();
    assert!(m.distance(at) < 1e-4);
}

/// The corpse laid as the creature leaves lies hidden under its ghost and shows when the ghost
/// ends; so does one laid a moment before. A corpse of another kind, or too far off, or laid too
/// long after, is never hidden, and a ghost hides only one.
#[test]
fn a_corpse_under_a_ghost_is_hidden_until_it_ends_and_no_other_is() {
    let mut run = Run::new(2.0);
    run.dying(0.5);
    let (_, f) = run.step(
        vec![
            SceneDelta::ObjectRemoved(ObjectKey::Server(7)),
            object(8, SETUP),
            object(9, SETUP),
        ],
        frame(
            0.6,
            vec![
                pose(8, (0.5, 0.0), Slot::Dead),
                pose(9, (1.0, 0.0), Slot::Dead),
            ],
        ),
    );
    assert_eq!(shown(&f, 8), Some(false), "the nearer corpse lies hidden");
    assert_eq!(shown(&f, 9), Some(true), "one corpse to a ghost");
    assert_eq!(shown(&f, 7), Some(true));
    assert_eq!(run.skins.ghosts().1, [ObjectKey::Server(8)]);
    let poses = || {
        vec![
            pose(8, (0.5, 0.0), Slot::Dead),
            pose(9, (1.0, 0.0), Slot::Dead),
        ]
    };
    let (_, f) = run.step(Vec::new(), frame(2.0, poses()));
    assert_eq!(shown(&f, 8), Some(false), "while the ghost lives");
    let (out, f) = run.step(Vec::new(), frame(2.15, poses()));
    assert!(removed(&out, 7));
    assert_eq!(shown(&f, 8), Some(true), "shown the moment the ghost ends");
    let (slot, _) = playing(&run.skins, 8).expect("posed");
    assert_eq!(slot, Some(Slot::Die), "lying at its death's end");

    // Laid a moment before the creature leaves, while its body dies.
    let mut run = Run::new(2.0);
    run.dying(0.5);
    let (_, f) = run.step(
        vec![object(8, SETUP)],
        frame(
            0.6,
            vec![
                pose(7, (0.0, 0.0), Slot::Die),
                pose(8, (0.5, 0.0), Slot::Dead),
            ],
        ),
    );
    assert_eq!(shown(&f, 8), Some(false), "hidden under the dying body");
    let (_, f) = run.step(
        vec![SceneDelta::ObjectRemoved(ObjectKey::Server(7))],
        frame(0.7, vec![pose(8, (0.5, 0.0), Slot::Dead)]),
    );
    assert_eq!(shown(&f, 8), Some(false), "and under its ghost");

    // Another kind, too far off, or too late: never hidden.
    let other = body(1, 2.0);
    let same = body(0, 5.0);
    let bodies = move |setup: u32| match setup {
        SETUP => Some(Arc::clone(&same)),
        0x0200_0002 => Some(Arc::clone(&other)),
        _ => None,
    };
    let mut run = Run {
        skins: Skins::default(),
        bodies: Box::new(bodies),
    };
    run.dying(0.5);
    let (_, f) = run.step(
        vec![
            SceneDelta::ObjectRemoved(ObjectKey::Server(7)),
            object(8, 0x0200_0002),
            object(9, SETUP),
        ],
        frame(
            0.6,
            vec![
                pose(8, (0.5, 0.0), Slot::Dead),
                pose(9, (CORPSE_REACH + 0.5, 0.0), Slot::Dead),
            ],
        ),
    );
    assert_eq!(shown(&f, 8), Some(true), "another setup and body");
    assert_eq!(shown(&f, 9), Some(true), "too far off");
    let late = 0.6 + CORPSE_WITHIN + 0.1;
    let (_, f) = run.step(
        vec![object(10, SETUP)],
        frame(late, vec![pose(10, (0.5, 0.0), Slot::Dead)]),
    );
    assert_eq!(
        run.skins.ghosts().0,
        [ObjectKey::Server(7)],
        "the ghost lives still"
    );
    assert_eq!(shown(&f, 10), Some(true), "laid too long after");
    assert!(run.skins.ghosts().1.is_empty());
}

/// A corpse laid by a body that is dying, but whose object never leaves, shows once the time a
/// corpse may come in has passed.
#[test]
fn a_corpse_by_a_body_that_dies_but_stays_shows_once_it_is_too_late_to_be_its() {
    let mut run = Run::new(5.0);
    run.dying(0.5);
    let both = |t: f64| {
        frame(
            t,
            vec![
                pose(7, (0.0, 0.0), Slot::Die),
                pose(8, (0.5, 0.0), Slot::Dead),
            ],
        )
    };
    let (_, f) = run.step(vec![object(8, SETUP)], both(0.6));
    assert_eq!(shown(&f, 8), Some(false));
    let (_, f) = run.step(Vec::new(), both(0.6 + CORPSE_WITHIN - 0.05));
    assert_eq!(shown(&f, 8), Some(false));
    let (_, f) = run.step(Vec::new(), both(0.6 + CORPSE_WITHIN + 0.05));
    assert_eq!(shown(&f, 8), Some(true));
}
