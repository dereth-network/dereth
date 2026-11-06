//! The lookup tables a bone track keeps from the data.
//!
//! Behaviour: none (a table kept in place of one made at each look, which no behavior of the game names)

use auberean_animation::clip::bone_at;
use auberean_animation::{Animations, CachedRecords};
use auberean_assets::animation::{AnimationKind, BoneTrack};
use auberean_primitives::DataId;

use crate::common::{ids, need_records};

/// Every track of every eighth animation finds the same keys through the tables it kept as through tables
/// made afresh from its keys, at the keys' own times and between them.
#[test]
fn a_tracks_kept_tables_find_what_new_ones_find() {
    let w = need_records!();
    let recs = CachedRecords::new(w);
    let (mut tracks, mut looks) = (0_usize, 0_usize);
    for id in ids(0x0500_0000, 0x05FF_FFFF).into_iter().step_by(8) {
        let Some(a) = recs.animation(DataId(id)) else {
            continue;
        };
        let AnimationKind::Keyframes(k) = &a.animator else {
            continue;
        };
        for track in &k.bones {
            tracks += 1;
            let afresh = BoneTrack {
                tables: Default::default(),
                ..track.clone()
            };
            let mut times: Vec<f32> = track
                .offset
                .iter()
                .map(|k| k.time)
                .chain(track.scale.iter().map(|k| k.time))
                .chain(track.rotation.iter().map(|k| k.time))
                .chain(track.weight.iter().map(|k| k.time))
                .collect();
            times.push(k.period);
            for t in times.iter().flat_map(|t| [*t, *t * 0.5, *t + 0.013]) {
                looks += 1;
                let (a, b) = (bone_at(track, t), bone_at(&afresh, t));
                assert_eq!(
                    format!("{a:?}"),
                    format!("{b:?}"),
                    "animation {id:#010x} at {t}"
                );
            }
        }
    }
    println!("{tracks} tracks, {looks} looks");
    assert!(tracks > 1_000, "{tracks}");
}
