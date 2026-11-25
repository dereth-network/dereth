//! Behaviour: none (experimental)
use super::*;
use crate::assets::{Subset, SurfaceRef, Vertex};
use crate::scene::Motion;

fn subset() -> Vec<Subset> {
    vec![Subset {
        first_index: 0,
        index_count: 3,
        surface: SurfaceRef::Surface(0x0800_0001),
        two_sided: false,
        tiled: true,
    }]
}

/// Two parts a metre apart along the third axis, one vertex on each, and one between them on
/// both evenly; part 1 is the anchor (part 0 is not named).
fn two_parts() -> SkinModel {
    let v = |z: f32| Vertex {
        position: [0.0, 0.0, z],
        normal: [1.0, 0.0, 0.0],
        uv: [0.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![v(0.0), v(1.0), v(0.5)],
        indices: vec![0, 1, 2],
        subsets: vec![Subset {
            surface: SurfaceRef::Surface(skin_surface(3, 0)),
            ..subset()[0]
        }],
        ..Mesh::default()
    };
    SkinModel::new(
        3,
        mesh,
        vec![[1, 0, 0, 0], [2, 0, 0, 0], [1, 2, 0, 0]],
        vec![
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [0.5, 0.5, 0.0, 0.0],
        ],
        &[
            (1, Mat4::IDENTITY),
            (2, Mat4::from_translation(Vec3::new(0.0, 0.0, -1.0))),
        ],
    )
    .expect("a body")
}

#[test]
fn a_bodys_ids_name_the_body_and_the_object_or_the_primitive() {
    assert_eq!(of_skin_mesh(skin_mesh(5, 900)), Some((5, 900)));
    assert_eq!(of_skin_surface(skin_surface(5, 2)), Some((5, 2)));
    assert_eq!(of_skin_mesh(0x0100_0001), None);
    assert_eq!(of_skin_surface(skin_mesh(5, 900)), None);
}

#[test]
fn a_body_is_drawn_through_the_first_part_it_names_and_hides_the_parts_it_names() {
    let b = two_parts();
    assert_eq!(b.anchor(), 1);
    assert!(!b.follows(0) && b.follows(1) && b.follows(2) && !b.follows(3));
    let (centre, radius) = b.rest.sphere.expect("a sphere");
    assert!(Vec3::from(centre).distance(Vec3::new(0.0, 0.0, 0.5)) < 1e-6);
    assert!(radius > 0.5, "wide enough to hold the body as it moves");
}

/// No part moves the body: wherever the parts stand, a body with no motion stands at rest where
/// its anchor's bind puts it, and one with a motion where the object stands.
#[test]
fn a_body_stands_at_rest_wherever_its_parts_move() {
    let b = two_parts();
    let anywhere = Mat4::from_rotation_z(0.7) * Mat4::from_translation(Vec3::new(3.0, 4.0, 5.0));
    for lifted in [1.0, 2.0, -3.0] {
        let parts = [
            Mat4::IDENTITY,
            anywhere,
            anywhere * Mat4::from_translation(Vec3::Z * lifted),
        ];
        let posed = b.posed(&b.joints(&parts).expect("joints"));
        for (p, v) in posed.iter().zip(&b.mesh.vertices) {
            assert!(Vec3::from(p.0).distance(Vec3::from(v.position)) < 1e-4);
            assert!(Vec3::from(p.1).distance(Vec3::X) < 1e-4);
        }
    }
    // With a motion: in the anchor's space, where the object stands.
    let parts = [Mat4::IDENTITY, anywhere, Mat4::IDENTITY];
    let at = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
    let mut state = PoseState::default();
    let live = Live {
        clock: 0.0,
        state: &mut state,
        motion: Some(Motion {
            frame: anywhere * at,
            ..motion(Some(Slot::Idle))
        }),
    };
    let posed = b.posed(&b.joints_live(&parts, Some(live)).expect("joints"));
    for (p, v) in posed.iter().zip(&b.mesh.vertices) {
        let want = at.transform_point3(Vec3::from(v.position));
        assert!(Vec3::from(p.0).distance(want) < 1e-4, "{:?}", p.0);
    }
    // A frame that lacks the anchor poses nothing.
    assert!(b.joints(&parts[..1]).is_none());
}

#[test]
fn influences_that_do_not_match_the_vertices_make_no_body() {
    let b = two_parts();
    assert!(SkinModel::new(0, b.mesh.clone(), vec![[0; 4]], b.weights.clone(), &[]).is_none());
    assert!(
        SkinModel::new(0, b.mesh.clone(), b.parts.clone(), b.weights.clone(), &[]).is_none(),
        "a body that names no part"
    );
}

#[test]
fn a_body_fits_only_parts_that_draw_the_gfxobjs_it_names() {
    let body = two_parts().with_rig(Rig {
        gfxobjs: vec![(1, 0x0100_1787), (2, 0x0100_1793)],
        ..Rig::default()
    });
    let bare: [&[u32]; 3] = [&[], &[0x0100_1787, 0x0100_004E], &[0x0100_1793]];
    assert!(body.fits(&bare));
    let other: [&[u32]; 3] = [&[], &[0x0100_1787], &[0x0100_9999]];
    assert!(!body.fits(&other));
    assert!(!body.fits(&bare[..2]), "a part missing");
}

/// A body of `bones` (each bound where `binds` says), its three vertices at `at`, each on
/// `joints` by `weights`, drawn in place of part 0, with `rig`'s clips and blending.
fn body(binds: &[Mat4], at: Vec3, joints: [u16; 4], weights: [f32; 4], rig: Rig) -> SkinModel {
    let v = Vertex {
        position: at.to_array(),
        normal: [0.0, 1.0, 0.0],
        uv: [0.0, 0.0],
    };
    let mesh = Mesh {
        vertices: vec![v, v, v],
        indices: vec![0, 1, 2],
        subsets: subset(),
        ..Mesh::default()
    };
    SkinModel::new(
        0,
        mesh,
        vec![joints; 3],
        vec![weights; 3],
        &[(0, Mat4::IDENTITY)],
    )
    .expect("a body")
    .with_rig(Rig {
        bones: binds.iter().map(|b| Bone::new(*b)).collect(),
        ..rig
    })
}

/// A clip whose steps are `steps`, lasting `seconds`.
fn clip(steps: Vec<Vec<(Quat, Vec3)>>, seconds: f32) -> Clip {
    Clip {
        steps: std::sync::Arc::new(steps),
        scales: std::sync::Arc::new(Vec::new()),
        seconds,
        stride: None,
        hit: None,
    }
}

/// A motion filling `slot`, the parts settled in their cycle, nothing more under way.
fn motion(slot: Option<Slot>) -> Motion {
    Motion {
        slot,
        idle: Slot::Idle,
        reversed: false,
        settled: true,
        phase: 0.0,
        until_cycle: 0.0,
        until_hit: None,
        queued: 0,
        keyframed: false,
        frame: Mat4::IDENTITY,
        velocity: Vec3::ZERO,
    }
}

/// Where `b`'s first vertex is drawn playing `m` at `clock`.
fn drawn(b: &SkinModel, state: &mut PoseState, clock: f64, m: Motion) -> Vec3 {
    let live = Live {
        clock,
        state,
        motion: Some(m),
    };
    Vec3::from(
        b.posed(
            &b.joints_live(&[Mat4::IDENTITY], Some(live))
                .expect("joints"),
        )[0]
        .0,
    )
}

#[test]
fn a_vertex_shared_across_a_bent_joint_keeps_its_distance_with_dual_quaternions() {
    // Two bones at the origin, the second bent 90 degrees by the clip.
    let bent = clip(
        vec![vec![
            (Quat::IDENTITY, Vec3::ZERO),
            (Quat::from_rotation_x(90f32.to_radians()), Vec3::ZERO),
        ]],
        1.0,
    );
    let limb = |dqs: f32| {
        body(
            &[Mat4::IDENTITY, Mat4::IDENTITY],
            Vec3::new(0.0, 0.1, 0.0),
            [BONE, BONE + 1, 0, 0],
            [0.5, 0.5, 0.0, 0.0],
            Rig {
                dqs: vec![dqs; 3],
                clips: vec![(Slot::Idle, bent.clone())],
                ..Rig::default()
            },
        )
    };
    let m = motion(Some(Slot::Idle));
    // Blended straight between the two bones, it falls to cos 45 degrees of its distance.
    let p = drawn(&limb(0.0), &mut PoseState::default(), 0.0, m);
    let cos45 = dereth_primitives::num::math::cosf(45f32.to_radians());
    assert!((p.length() - 0.1 * cos45).abs() < 1e-4, "{p}");
    // Blended as dual quaternions, it keeps it.
    let p = drawn(&limb(1.0), &mut PoseState::default(), 0.0, m);
    assert!((p.length() - 0.1).abs() < 1e-4, "{p}");
}

#[test]
fn a_vertex_on_a_scaled_bone_blended_as_dual_quaternions_scales_with_it_in_place() {
    // An eye three metres up, wholly on a bone bound there, which its clip shrinks to half (a
    // blink): it stays where it is, however it is blended, and does not shoot up.
    let eye = Vec3::new(0.0, 0.0, 3.0);
    let blink = Clip {
        scales: std::sync::Arc::new(vec![vec![0.5]]),
        ..clip(vec![vec![(Quat::IDENTITY, eye)]], 1.0)
    };
    for share in [0.0, 1.0] {
        let b = body(
            &[Mat4::from_translation(-eye)],
            eye,
            [BONE, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            Rig {
                dqs: vec![share; 3],
                clips: vec![(Slot::Idle, blink.clone())],
                ..Rig::default()
            },
        );
        let at = drawn(&b, &mut PoseState::default(), 0.0, motion(Some(Slot::Idle)));
        assert!(at.distance(eye) < 1e-4, "share {share}: {at}");
    }
}

/// A leg: one bone at the hip (the origin), its vertex 0.8 below it.
fn leg(clips: Vec<(Slot, Clip)>) -> SkinModel {
    body(
        &[Mat4::IDENTITY],
        Vec3::new(0.0, 0.0, -0.8),
        [BONE, 0, 0, 0],
        [1.0, 0.0, 0.0, 0.0],
        Rig {
            clips,
            ..Rig::default()
        },
    )
}

/// The leg turned `deg` degrees forward about the hip.
fn turned(deg: f32) -> Vec<(Quat, Vec3)> {
    vec![(Quat::from_rotation_x(deg.to_radians()), Vec3::ZERO)]
}

/// A clip turning the leg from `from` to `to` degrees over `seconds`.
fn swing(from: f32, to: f32, seconds: f32) -> Clip {
    clip(vec![turned(from), turned(to)], seconds)
}

/// A clip holding the leg at `deg` degrees.
fn held(deg: f32) -> Clip {
    swing(deg, deg, 1.0)
}

/// How far forward the leg is turned, in degrees, its foot drawn at `foot`.
fn angle(foot: Vec3) -> f32 {
    dereth_primitives::num::math::atan2f(foot.y, -foot.z).to_degrees()
}

/// The leg's angle playing `m` at `clock`.
fn at(b: &SkinModel, state: &mut PoseState, clock: f64, m: Motion) -> f32 {
    angle(drawn(b, state, clock, m))
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

#[test]
fn every_slot_has_the_name_a_pack_gives_it_and_plays_as_its_kind_plays() {
    for s in Slot::ALL {
        assert_eq!(Slot::named(s.name()), Some(s));
    }
    assert_eq!(Slot::named("dance"), None);
    assert_eq!(Slot::Idle.play(), Play::Loop);
    assert_eq!(Slot::Turn.play(), Play::Loop);
    assert_eq!(Slot::Fall.play(), Play::Loop);
    assert_eq!(Slot::Run.play(), Play::Paced);
    assert_eq!(Slot::SideStep.play(), Play::Paced);
    assert_eq!(Slot::MeleeHigh.play(), Play::Once);
    assert_eq!(Slot::Hit.play(), Play::Once);
    assert_eq!(Slot::Die.play(), Play::Hold);
}

/// An empty slot plays the first of its fallbacks the body has.
#[test]
fn an_empty_slot_plays_its_fallback() {
    let b = leg(vec![
        (Slot::Idle, held(10.0)),
        (Slot::Walk, held(20.0)),
        (Slot::MeleeMed, held(30.0)),
        (Slot::Cast, held(40.0)),
        (Slot::Dead, held(50.0)),
    ]);
    let r = |s: Slot| b.resolve(s, Slot::Idle);
    assert_eq!(r(Slot::SideStep), Some(Slot::Walk));
    assert_eq!(r(Slot::WalkBack), Some(Slot::Walk));
    assert_eq!(r(Slot::Run), Some(Slot::Walk));
    assert_eq!(r(Slot::MeleeHigh), Some(Slot::MeleeMed));
    assert_eq!(r(Slot::MeleeLow), Some(Slot::MeleeMed));
    assert_eq!(r(Slot::Special), Some(Slot::MeleeMed));
    assert_eq!(r(Slot::Missile), Some(Slot::MeleeMed));
    assert_eq!(r(Slot::Telegraph), Some(Slot::Cast));
    assert_eq!(r(Slot::IdleMelee), Some(Slot::Idle));
    assert_eq!(r(Slot::Emote), Some(Slot::Idle));
    assert_eq!(r(Slot::Cheer), Some(Slot::Idle));
    assert_eq!(r(Slot::Turn), Some(Slot::Idle));
    assert_eq!(r(Slot::Fall), Some(Slot::Idle));
    assert_eq!(r(Slot::Jump), Some(Slot::Idle));
    assert_eq!(r(Slot::Die), Some(Slot::Dead));
    assert_eq!(r(Slot::Hit), None);
    // The stance's idle stands in for the peace idle first.
    let melee = leg(vec![
        (Slot::Idle, held(10.0)),
        (Slot::IdleMelee, held(15.0)),
    ]);
    assert_eq!(
        melee.resolve(Slot::Turn, Slot::IdleMelee),
        Some(Slot::IdleMelee)
    );
    // Each plays its fallback's clip: a turn in a fight stands at the fight's idle.
    let m = Motion {
        idle: Slot::IdleMelee,
        ..motion(Some(Slot::Turn))
    };
    assert!(near(at(&melee, &mut PoseState::default(), 0.0, m), 15.0));
    assert!(near(
        at(
            &b,
            &mut PoseState::default(),
            0.0,
            motion(Some(Slot::Special))
        ),
        30.0
    ));
    assert!(near(
        at(
            &b,
            &mut PoseState::default(),
            0.0,
            motion(Some(Slot::Telegraph))
        ),
        40.0
    ));
    // With nothing to stand in, an idle stands at rest.
    let bare = leg(vec![(Slot::Walk, held(20.0))]);
    assert!(near(
        at(
            &bare,
            &mut PoseState::default(),
            0.0,
            motion(Some(Slot::Idle))
        ),
        0.0
    ));
}

/// A turn or a side step to the left plays the right's clip backwards, and a walk backwards with
/// no clip of its own plays the walk backwards.
#[test]
fn left_plays_right_backwards_and_a_walk_backwards_plays_the_walk_backwards() {
    let b = leg(vec![(Slot::Turn, swing(0.0, 90.0, 1.0))]);
    let right = at(
        &b,
        &mut PoseState::default(),
        0.25,
        motion(Some(Slot::Turn)),
    );
    assert!(
        near(right, 0.0),
        "the first frame plays from its start: {right}"
    );
    let mut state = PoseState::default();
    at(&b, &mut state, 0.0, motion(Some(Slot::Turn)));
    assert!(near(
        at(&b, &mut state, 0.25, motion(Some(Slot::Turn))),
        22.5
    ));
    let left = Motion {
        reversed: true,
        ..motion(Some(Slot::Turn))
    };
    let mut state = PoseState::default();
    at(&b, &mut state, 0.0, left);
    assert!(near(at(&b, &mut state, 0.25, left), 67.5));
    // The walk with no stride, at its own pace: a quarter second in, backwards.
    let w = leg(vec![(Slot::Walk, swing(0.0, 90.0, 1.0))]);
    let back = Motion {
        reversed: true,
        ..motion(Some(Slot::WalkBack))
    };
    let mut state = PoseState::default();
    at(&w, &mut state, 0.0, back);
    assert!(near(at(&w, &mut state, 0.25, back), 67.5));
}

/// A change of stance with no clip of its own fades from the old stance's idle to the new one's
/// over the parts' way between them.
#[test]
fn a_change_of_stance_fades_from_one_idle_to_the_other() {
    let b = leg(vec![
        (Slot::Idle, held(10.0)),
        (Slot::IdleMelee, held(40.0)),
    ]);
    let mut state = PoseState::default();
    assert!(near(
        at(&b, &mut state, 0.0, motion(Some(Slot::Idle))),
        10.0
    ));
    let changing = Motion {
        idle: Slot::IdleMelee,
        settled: false,
        until_cycle: 0.8,
        ..motion(Some(Slot::Stance))
    };
    assert!(near(at(&b, &mut state, 1.0, changing), 10.0));
    let half = at(
        &b,
        &mut state,
        1.4,
        Motion {
            until_cycle: 0.4,
            ..changing
        },
    );
    assert!(near(half, 25.0), "{half}");
    let settled = Motion {
        idle: Slot::IdleMelee,
        ..motion(Some(Slot::IdleMelee))
    };
    assert!(near(at(&b, &mut state, 2.0, settled), 40.0));
}

/// A hit the body has no clip for leaves what it plays running as it was: an idle goes on, not
/// started again, and so does a swing, rather than the body standing.
#[test]
fn an_empty_hit_keeps_the_clip_that_plays_running() {
    let b = leg(vec![
        (Slot::Idle, swing(0.0, 90.0, 1.0)),
        (Slot::MeleeMed, swing(90.0, 0.0, 1.0)),
    ]);
    let hit = Motion {
        settled: false,
        queued: 1,
        ..motion(Some(Slot::Hit))
    };
    let mut state = PoseState::default();
    at(&b, &mut state, 0.0, motion(Some(Slot::Idle)));
    let idle = at(&b, &mut state, 0.25, motion(Some(Slot::Idle)));
    assert!(near(idle, 22.5), "{idle}");
    assert!(near(at(&b, &mut state, 0.5, hit), 45.0));
    let drawn = state.playing.as_ref().map(|p| p.drawn().0);
    assert_eq!(drawn, Some(Some(Slot::Idle)));
    // So does an action no slot is for.
    assert!(near(at(&b, &mut state, 0.6, motion(None)), 54.0));
    // A swing under way goes on through the hit.
    let swinging = Motion {
        settled: false,
        queued: 1,
        ..motion(Some(Slot::MeleeMed))
    };
    let mut state = PoseState::default();
    at(&b, &mut state, 0.0, swinging);
    assert!(near(at(&b, &mut state, 0.25, swinging), 67.5));
    assert!(near(at(&b, &mut state, 0.5, hit), 45.0));
    // A hit it has a clip for plays it.
    let hurt = leg(vec![(Slot::Idle, held(0.0)), (Slot::Hit, held(70.0))]);
    assert!(near(at(&hurt, &mut PoseState::default(), 0.0, hit), 70.0));
}

/// A death plays once and holds its end, on into lying dead; a corpse first seen lies at the
/// death's end, or in its dead clip.
#[test]
fn a_death_plays_once_and_holds_its_end_and_a_corpse_lies_at_it() {
    let b = leg(vec![
        (Slot::Idle, held(0.0)),
        (Slot::Die, swing(0.0, 90.0, 1.0)),
    ]);
    let dying = Motion {
        settled: false,
        until_cycle: 0.5,
        ..motion(Some(Slot::Die))
    };
    let mut state = PoseState::default();
    at(&b, &mut state, 0.0, dying);
    assert!(near(at(&b, &mut state, 0.5, dying), 45.0));
    let dead = motion(Some(Slot::Dead));
    assert!(
        near(at(&b, &mut state, 0.75, dead), 67.5),
        "the death plays on"
    );
    assert!(
        near(at(&b, &mut state, 3.0, dead), 90.0),
        "and holds its end"
    );
    assert!(near(at(&b, &mut PoseState::default(), 0.0, dead), 90.0));
    let lying = leg(vec![
        (Slot::Die, swing(0.0, 90.0, 1.0)),
        (Slot::Dead, held(85.0)),
    ]);
    assert!(near(at(&lying, &mut PoseState::default(), 0.0, dead), 85.0));
}

/// A body with no sleep of its own dies into its sleep, lies at the death's end, and wakes by
/// dying backwards over the parts' way up.
#[test]
fn a_sleep_with_no_clip_dies_into_it_and_wakes_by_dying_backwards() {
    let b = leg(vec![
        (Slot::Idle, held(0.0)),
        (Slot::Die, swing(0.0, 90.0, 1.0)),
    ]);
    let mut state = PoseState::default();
    at(&b, &mut state, 0.0, motion(Some(Slot::Idle)));
    let falling = Motion {
        settled: false,
        until_cycle: 1.0,
        ..motion(Some(Slot::Sleep))
    };
    at(&b, &mut state, 1.0, falling);
    assert!(near(at(&b, &mut state, 1.5, falling), 45.0));
    let asleep = motion(Some(Slot::Sleep));
    assert!(near(at(&b, &mut state, 3.0, asleep), 90.0));
    let waking = Motion {
        settled: false,
        until_cycle: 2.0,
        ..motion(Some(Slot::Idle))
    };
    assert!(near(at(&b, &mut state, 4.0, waking), 90.0));
    let half = at(
        &b,
        &mut state,
        5.0,
        Motion {
            until_cycle: 1.0,
            ..waking
        },
    );
    assert!(near(half, 45.0), "{half}");
    at(&b, &mut state, 7.0, motion(Some(Slot::Idle)));
    assert!(near(at(&b, &mut state, 7.5, motion(Some(Slot::Idle))), 0.0));
    // A body first seen asleep lies in the dead pose.
    assert!(near(at(&b, &mut PoseState::default(), 0.0, asleep), 90.0));
}

/// An attack's clip is timed so its blow lands when the parts' does: until then it plays at the
/// speed that brings it there, after it at its own.
#[test]
fn an_attacks_blow_lands_when_the_parts_blow_does() {
    let swing_ = Clip {
        hit: Some(0.6),
        ..swing(0.0, 90.0, 1.0)
    };
    let b = leg(vec![(Slot::MeleeMed, swing_)]);
    let attack = |until: Option<f32>| Motion {
        settled: false,
        until_hit: until,
        queued: 1,
        ..motion(Some(Slot::MeleeMed))
    };
    let mut state = PoseState::default();
    // The blow 0.3 s off, 0.6 s into the clip: it plays twice as fast until then.
    at(&b, &mut state, 0.0, attack(Some(0.3)));
    let mid = at(&b, &mut state, 0.15, attack(Some(0.15)));
    assert!(near(mid, 27.0), "{mid}");
    let blow = at(&b, &mut state, 0.3, attack(None));
    assert!(near(blow, 54.0), "{blow}");
    let after = at(&b, &mut state, 0.5, attack(None));
    assert!(near(after, 72.0), "{after}");
    // With no blow to land, at its own pace.
    let mut state = PoseState::default();
    at(&b, &mut state, 0.0, attack(None));
    assert!(near(at(&b, &mut state, 0.3, attack(None)), 27.0));
}

/// An action that starts again (the next of two queued) plays its clip from its start again,
/// fading from where it was.
#[test]
fn an_action_that_starts_again_plays_its_clip_again() {
    let b = leg(vec![(Slot::MeleeMed, swing(0.0, 90.0, 1.0))]);
    let attack = |queued: u8| Motion {
        settled: false,
        queued,
        ..motion(Some(Slot::MeleeMed))
    };
    let mut state = PoseState::default();
    at(&b, &mut state, 0.0, attack(2));
    assert!(near(at(&b, &mut state, 0.5, attack(2)), 45.0));
    at(&b, &mut state, 0.6, attack(1));
    let again = at(&b, &mut state, 0.9, attack(1));
    assert!(near(again, 27.0), "{again}");
}

/// The body stands as high as its clip for a slot reaches (the clip standing in for an empty
/// one), else at rest.
#[test]
fn a_body_stands_as_high_as_its_clip_for_a_slot_reaches() {
    let b = leg(vec![(Slot::Idle, swing(60.0, 120.0, 1.0))]);
    let twice = Mat4::from_scale(Vec3::splat(2.0));
    let top = |s: Option<Slot>| b.top_playing(&[Mat4::IDENTITY], twice, s).expect("a top");
    assert!(
        near(top(Some(Slot::Idle)), 0.8),
        "{}",
        top(Some(Slot::Idle))
    );
    assert!(near(top(Some(Slot::IdleMelee)), 0.8));
    assert!(near(top(None), -1.6));
    assert!(
        near(top(Some(Slot::Walk)), -1.6),
        "no clip stands in for a walk"
    );
    assert_eq!(b.top_playing(&[], twice, Some(Slot::Idle)), None);
}

/// A body plays its clip where the object stands, however its parts are placed.
#[test]
fn a_clip_plays_where_the_object_stands() {
    let b = leg(vec![(Slot::Idle, held(90.0))]);
    let turned_object = Motion {
        frame: Mat4::from_rotation_z(90f32.to_radians()),
        ..motion(Some(Slot::Idle))
    };
    let live = Live {
        clock: 0.0,
        state: &mut PoseState::default(),
        motion: Some(turned_object),
    };
    let parts = [Mat4::from_rotation_y(1.0)];
    let joints = b.joints_live(&parts, Some(live)).expect("joints");
    let foot = parts[0].transform_point3(Vec3::from(b.posed(&joints)[0].0));
    assert!(foot.distance(Vec3::new(-0.8, 0.0, 0.0)) < 1e-4, "{foot}");
}

/// How far the leg's walk travels in one round, in the body's own meters.
const STRIDE: f32 = 1.35;

/// The leg with a walk of a second that swings it forward and back, going round once for each
/// [`STRIDE`], and an idle standing straight.
fn strided() -> SkinModel {
    let walk = Clip {
        stride: Some(STRIDE),
        ..clip(vec![turned(0.0), turned(90.0), turned(0.0)], 1.0)
    };
    leg(vec![(Slot::Walk, walk), (Slot::Idle, held(5.0))])
}

/// The walk's place in its round after `steps` (the clock and the parts' place in their cycle),
/// the motion asking `speed` meters a second of the body's own space, filling `slot`, played
/// backwards or not, at keyframes or not; and the leg's angle then.
fn walked(
    body: &SkinModel,
    state: &mut PoseState,
    steps: impl IntoIterator<Item = (f64, f32)>,
    (slot, speed): (Slot, f32),
    (reversed, keyframed): (bool, bool),
) -> (Option<f32>, f32) {
    let mut a = 0.0;
    for (clock, phase) in steps {
        let m = Motion {
            reversed,
            keyframed,
            phase,
            velocity: Vec3::Y * speed,
            frame: Mat4::from_scale(Vec3::splat(2.0)),
            ..motion(Some(slot))
        };
        a = angle(drawn(body, state, clock, m));
    }
    let share = state
        .playing
        .as_ref()
        .and_then(|p| p.paced)
        .map(|p| p.share);
    (share, a)
}

/// Eleven frames a tenth of a second apart, the parts' place held at `phase`.
fn second(phase: f32) -> impl Iterator<Item = (f64, f32)> {
    (0..=10u8).map(move |k| (f64::from(k) * 0.1, phase))
}

/// A walk with a stride goes round once for each stride its motion asks, whether or not the
/// object gets anywhere: from the parts' place, a second at a stride and a half a second is half
/// way round, the leg turned 90 degrees forward. A walk backwards with no clip goes round the
/// walk the other way. Drawn at keyframes it stands at its nearest step.
#[test]
fn a_walk_goes_round_once_for_each_stride_its_motion_asks() {
    let b = strided();
    let walk = |speed: f32| (Slot::Walk, speed * STRIDE);
    let (share, a) = walked(
        &b,
        &mut PoseState::default(),
        second(0.0),
        walk(1.5),
        (false, false),
    );
    assert!(share.is_some_and(|s| (s - 0.5).abs() < 1e-3), "{share:?}");
    assert!(near(a, 90.0), "{a}");
    // A run with no clip of its own plays the walk, paced alike.
    let run = (Slot::Run, 1.5 * STRIDE);
    let (share, _) = walked(
        &b,
        &mut PoseState::default(),
        second(0.0),
        run,
        (false, false),
    );
    assert!(share.is_some_and(|s| (s - 0.5).abs() < 1e-3), "{share:?}");
    // Blocked: asked half a stride a second, from a tenth of the way round.
    let (share, _) = walked(
        &b,
        &mut PoseState::default(),
        second(0.1),
        walk(0.5),
        (false, false),
    );
    assert!(share.is_some_and(|s| (s - 0.6).abs() < 1e-3), "{share:?}");
    // Backwards, a stride and a quarter from a tenth of the way round: 0.85 of the way.
    let back = (Slot::WalkBack, 1.25 * STRIDE);
    let (share, _) = walked(
        &b,
        &mut PoseState::default(),
        second(0.1),
        back,
        (true, false),
    );
    assert!(share.is_some_and(|s| (s - 0.85).abs() < 1e-3), "{share:?}");
    // At keyframes: two fifths of the way round, the nearest step is the middle one.
    let (share, a) = walked(
        &b,
        &mut PoseState::default(),
        second(0.0),
        walk(1.4),
        (false, true),
    );
    assert!(share.is_some_and(|s| (s - 0.4).abs() < 1e-3), "{share:?}");
    assert!(near(a, 90.0), "{a}");
    // With no speed asked, at its own pace: half a second into its one.
    let (share, a) = walked(
        &b,
        &mut PoseState::default(),
        second(0.0).take(6),
        walk(0.0),
        (false, false),
    );
    assert_eq!(share, None);
    assert!(near(a, 90.0), "{a}");
}

/// A walk asked to carry the body faster than its strides keep up with at a sensible pace goes
/// round at most [`MOST_PACE`] times its own pace, and its feet slip the rest: asked five strides
/// a second, the one-second walk goes round two and a half times a second, so a fifth of a second
/// carries it half way round.
#[test]
fn a_walk_is_sped_up_at_most_so_many_times_its_own_pace() {
    let b = strided();
    let (share, a) = walked(
        &b,
        &mut PoseState::default(),
        second(0.0).take(3),
        (Slot::Walk, 5.0 * STRIDE),
        (false, false),
    );
    assert!(
        share.is_some_and(|s| (s - 0.1 * 2.0 * MOST_PACE).abs() < 1e-3),
        "{share:?}"
    );
    assert!(near(a, 90.0), "{a}");
    // Within it, its feet hold: two strides a second.
    let (share, _) = walked(
        &b,
        &mut PoseState::default(),
        second(0.0).take(3),
        (Slot::Walk, 2.0 * STRIDE),
        (false, false),
    );
    assert!(share.is_some_and(|s| (s - 0.4).abs() < 1e-3), "{share:?}");
}

/// The parts slowing from a walk to a stand keep the walk's clip going, slower each frame,
/// rather than standing already; once it has all but stopped, the body stands.
#[test]
fn a_walk_slows_to_a_stop_before_the_body_stands() {
    let b = strided();
    let mut state = PoseState::default();
    let mut played = Vec::new();
    for k in 0..=14u8 {
        let walking = k <= 5;
        let m = Motion {
            settled: walking,
            until_cycle: if walking { 0.0 } else { 0.5 },
            velocity: if walking {
                Vec3::Y * 2.0 * STRIDE
            } else {
                Vec3::ZERO
            },
            ..motion(Some(if walking { Slot::Walk } else { Slot::Idle }))
        };
        drawn(&b, &mut state, f64::from(k) * 0.1, m);
        played.push(state.playing.as_ref().expect("played").drawn());
    }
    for (k, p) in played.iter().enumerate().take(10) {
        assert_eq!(p.0, Some(Slot::Walk), "{k}: {played:?}");
    }
    let on = |k: usize| (played[k].1 - played[k - 1].1).rem_euclid(1.0);
    assert!((on(5) - 0.2).abs() < 1e-3, "{played:?}");
    for k in 6..=9 {
        assert!(on(k) > 0.0 && on(k) < on(k - 1), "{k}: {played:?}");
    }
    assert_eq!(played[14].0, Some(Slot::Idle), "{played:?}");
}

/// The parts' stopping stride still carrying the object keeps the walk's clip going at its pace,
/// so its feet keep up with the object; once it stops carrying it, the clip slows to a stand.
#[test]
fn a_stopping_stride_keeps_the_walk_at_its_pace_until_it_stops_carrying_the_object() {
    let b = strided();
    let mut state = PoseState::default();
    let mut played = Vec::new();
    for k in 0..=16u8 {
        let walking = k <= 5;
        let carried = (6..=10).contains(&k);
        let m = Motion {
            settled: walking,
            until_cycle: if walking { 0.0 } else { 0.5 },
            velocity: if walking {
                Vec3::Y * 2.0 * STRIDE
            } else if carried {
                Vec3::Y * STRIDE
            } else {
                Vec3::ZERO
            },
            ..motion(Some(if walking { Slot::Walk } else { Slot::Idle }))
        };
        drawn(&b, &mut state, f64::from(k) * 0.1, m);
        played.push(state.playing.as_ref().expect("played").drawn());
    }
    let on = |k: usize| (played[k].1 - played[k - 1].1).rem_euclid(1.0);
    for k in 6..=10 {
        assert_eq!(played[k].0, Some(Slot::Walk), "{k}: {played:?}");
        assert!((on(k) - 0.1).abs() < 1e-3, "{k}: {played:?}");
    }
    for k in 11..=13 {
        assert!(on(k) > 0.0 && on(k) < on(k - 1), "{k}: {played:?}");
    }
    assert_eq!(played[16].0, Some(Slot::Idle), "{played:?}");
}

/// A body bent by its parts follows each part where the frame places it: the vertex on part 2
/// goes where part 2 now stands, the one shared by both goes halfway, and the place a body
/// playing its own clips would stand at is not read.
#[test]
fn a_body_bent_by_its_parts_follows_each_part_where_the_frame_places_it() {
    let b = two_parts().with_rig(Rig {
        by_parts: true,
        ..Rig::default()
    });
    // Part 1 (the anchor) where the bind holds it; part 2 lifted two meters and turned a
    // quarter about the third axis.
    let turned = Mat4::from_rotation_translation(
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        Vec3::new(0.0, 0.0, 3.0),
    );
    let parts = [Mat4::IDENTITY, Mat4::IDENTITY, turned];
    let mut state = PoseState::default();
    let live = Live {
        clock: 0.0,
        state: &mut state,
        motion: Some(Motion {
            frame: Mat4::from_translation(Vec3::new(50.0, 0.0, 0.0)),
            ..motion(Some(Slot::Walk))
        }),
    };
    let joints = b.joints_live(&parts, Some(live)).expect("joints");
    let posed = b.posed(&joints);
    let at = |k: usize| Vec3::from(posed[k].0);
    assert!(at(0).abs_diff_eq(Vec3::ZERO, 1e-5), "{:?}", at(0));
    assert!(
        at(1).abs_diff_eq(Vec3::new(0.0, 0.0, 3.0), 1e-5),
        "{:?}",
        at(1)
    );
    assert!(
        at(2).abs_diff_eq(Vec3::new(0.0, 0.0, 1.5), 1e-5),
        "{:?}",
        at(2)
    );
}
