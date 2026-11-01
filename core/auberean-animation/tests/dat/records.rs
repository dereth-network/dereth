//! Every animation, behavior table and the character template of the end-of-retail set.

use std::collections::BTreeMap;
use std::sync::Arc;

use auberean_animation::clip::Pose;
use auberean_animation::graph::{animate, kind_of, period, Eval, GraphState};
use auberean_animation::lookup::LookupTable;
use auberean_animation::queries::{next_step, TEMPLATE_ENUM};
use auberean_animation::records::DEFAULT_TEMPLATE;
use auberean_animation::sliders::Sliders;
use auberean_animation::turns::Ground;
use auberean_animation::{Animations, CachedRecords};
use auberean_assets::animation::{AnimationKind, BehaviorTable, ModeChange};
use auberean_primitives::DataId;
use auberean_world::chargen::Random;

use crate::common::{ids, need_records, portal};

/// Every animation record decodes; every keyframe track starts at time 0 (so the client builds its
/// lookup table); every animation's period is a number, and played at the start, middle and end
/// of its cycle it adds only numbers into a pose.
///
/// Behaviour: animation.records.every-animation-decodes-and-plays
#[test]
fn every_animation_decodes_and_plays() {
    let w = need_records!();
    let recs = CachedRecords::new(w);
    let list = ids(0x0500_0000, 0x05FF_FFFF);
    let mut kinds: BTreeMap<u32, usize> = BTreeMap::new();
    let (mut undecoded, mut tracks, mut tableless) = (Vec::new(), 0usize, 0usize);
    let (mut events, mut late_events) = (0usize, 0usize);
    let sliders = Sliders::new();
    let mut dice = Random::default();
    for &id in &list {
        let Some(a) = recs.animation(DataId(id)) else {
            undecoded.push(id);
            continue;
        };
        *kinds.entry(kind_of(&a)).or_default() += 1;
        let mut bones = 64;
        if let AnimationKind::Keyframes(k) = &a.animator {
            bones = k.bones.len().max(1);
            for b in &k.bones {
                let keyed: [Vec<f32>; 4] = [
                    b.scale.iter().map(|k| k.time).collect(),
                    b.offset.iter().map(|k| k.time).collect(),
                    b.rotation.iter().map(|k| k.time).collect(),
                    b.weight.iter().map(|k| k.time).collect(),
                ];
                for times in keyed.iter().filter(|t| !t.is_empty()) {
                    tracks += 1;
                    if LookupTable::new(times).is_none() {
                        tableless += 1;
                    }
                }
            }
            if let Some(e) = &k.events {
                for t in e
                    .effects
                    .iter()
                    .map(|x| x.time)
                    .chain(e.hooks.iter().map(|x| x.time))
                    .chain(e.impulses.iter().map(|x| x.time))
                {
                    events += 1;
                    if !(0.0..=1.0).contains(&t) {
                        late_events += 1;
                    }
                }
            }
        }
        let state = GraphState {
            sliders: &sliders,
            mode: 0,
            element: 0,
            terrain: 0,
        };
        let p = period(&recs, DataId(id), &state);
        assert!(p.is_finite() && p >= 0.0, "{id:08X}: period {p}");
        let mut s = sliders.clone();
        let parents = vec![None; bones];
        for phase in [0.0f32, 0.5, 0.999] {
            let mut pose = Pose::new(bones);
            let mut eval = Eval {
                sliders: &mut s,
                dice: &mut dice,
                mode: 0,
                element: 0,
                terrain: 0,
                bone_types: &[],
                ground: Ground::default(),
            };
            animate(
                &recs,
                DataId(id),
                &mut eval,
                1.0,
                phase,
                &parents,
                &mut pose,
            );
            for (b, (sum, w)) in pose.bones.iter().zip(&pose.weights).enumerate() {
                let all = [
                    sum.offset.x,
                    sum.offset.y,
                    sum.offset.z,
                    sum.scale.x,
                    sum.rotation.w,
                    sum.rotation.x,
                    *w,
                ];
                assert!(
                    all.iter().all(|v| v.is_finite()),
                    "{id:08X} bone {b} at {phase}: {sum:?} {w}"
                );
            }
        }
    }
    eprintln!(
        "{} animations: kinds {kinds:x?}; {tracks} keyed tracks, {tableless} not starting at 0; \
         {events} events, {late_events} past the cycle",
        list.len()
    );
    assert!(undecoded.is_empty(), "undecoded: {undecoded:08X?}");
    assert!(list.len() > 1000, "{} animations", list.len());
    assert_eq!(tableless, 0, "every keyed track starts at 0");
}

/// For every behavior table and every pair of its modes, the way from one to the other is either
/// none or starts with a mode the first leads to directly.
///
/// Behaviour: animation.modes.the-mode-path-steps-to-a-neighbor
#[test]
fn every_mode_path_steps_to_a_neighbor() {
    let w = need_records!();
    let recs = CachedRecords::new(w);
    let list = ids(0x4400_0000, 0x44FF_FFFF);
    let (mut tables, mut pairs, mut ways) = (0usize, 0usize, 0usize);
    for id in list {
        let Some(t) = w.get::<BehaviorTable>(id) else {
            continue;
        };
        let t = Arc::new(t);
        tables += 1;
        for (from, _) in &t.modes {
            let next: &[_] = t
                .next_modes
                .iter()
                .find(|(k, _)| k == from)
                .map_or(&[][..], |(_, v)| v.as_slice());
            for (to, _) in &t.modes {
                if from == to {
                    continue;
                }
                pairs += 1;
                if let Some(step) = next_step(
                    &t,
                    &ModeChange {
                        from: *from,
                        to: *to,
                    },
                ) {
                    ways += 1;
                    assert!(
                        next.contains(&step),
                        "{id:08X}: {from:?} -> {to:?} via {step:?}"
                    );
                }
            }
        }
    }
    let _ = recs;
    eprintln!("{tables} behavior tables, {pairs} mode pairs, {ways} with a way");
    assert!(tables >= 10 && ways > 0);
}

/// The character template the client finds by enumeration is the one record of its kind.
///
/// Behaviour: animation.records.the-character-template-is-found-by-enumeration
#[test]
fn the_character_template_is_found_by_enumeration() {
    let w = need_records!();
    let file = portal().clone();
    let db = auberean_assets::database::Database::new(vec![Arc::new(file)]);
    let id = db.data_id_by_enum(TEMPLATE_ENUM.0, TEMPLATE_ENUM.1);
    assert_eq!(id, DEFAULT_TEMPLATE, "found {id:08X}");
    let recs = CachedRecords::new(w);
    use auberean_animation::BodyRecords;
    let t = recs.character_template().expect("decodes");
    assert!(!t.behaviors.is_empty() && !t.sliders.is_empty());
    assert_eq!(ids(0x2F00_0000, 0x2FFF_FFFF), vec![DEFAULT_TEMPLATE]);
}
