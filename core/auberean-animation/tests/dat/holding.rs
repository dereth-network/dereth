//! Where the player's human male holds things and where its bones are, in the pose it takes.
//! Fixture: the human male (visual description `0x1F000023`) of the September 2013 install, one
//! tenth of a second into its first mode.

use auberean_animation::body::Body;
use auberean_animation::CachedRecords;
use auberean_primitives::DataId;

use crate::common::need_records;

const HUMAN_MALE: DataId = DataId(0x1F00_0023);

/// Every holding location the human male's part groups' setups name is found in the first group
/// that names its kind, on the transform its bone has in the body's pose; a kind no setup names is
/// found nowhere. Every bone kind is found in the first group that has it, at the first bone of
/// the kind, with that bone's transform in the pose.
///
/// Behaviour: parts.holding.the-human-male-holds-things-where-its-setups-say
#[test]
fn the_human_male_holds_things_where_its_setups_say() {
    let w = need_records!();
    let recs = CachedRecords::new(w);
    let mut body = Body::assemble(&recs, HUMAN_MALE).expect("the human male");
    body.advance(&recs, 0.1);
    let pose = body.pose(&recs);
    let mut places = 0;
    for part in &body.parts {
        for l in &part.setup.holding_locations {
            let first = body
                .parts
                .iter()
                .position(|p| p.setup.holding_locations.iter().any(|x| x.kind == l.kind))
                .expect("named by some group");
            assert!(body.has_holding_location(l.kind));
            let (group, found, bone) = body
                .holding_location(&recs, l.kind)
                .expect("a named place is found");
            assert_eq!(group, first, "kind {:#x}", l.kind);
            assert_eq!(found.kind, l.kind);
            assert_eq!(bone, pose[group].bones[found.bone as usize]);
            places += 1;
        }
    }
    assert!(places > 0, "the human male holds things");
    assert!(!body.has_holding_location(0xDEAD_BEEF));
    assert!(body.holding_location(&recs, 0xDEAD_BEEF).is_none());

    for part in &body.parts {
        for b in &part.setup.bones {
            let group = body
                .parts
                .iter()
                .position(|p| p.setup.bones.iter().any(|x| x.id == b.id))
                .expect("some group has it");
            let bone = body.parts[group]
                .setup
                .bones
                .iter()
                .position(|x| x.id == b.id)
                .expect("the group has it");
            assert_eq!(
                body.find_bone_type(b.id),
                Some((group, u32::try_from(bone).expect("a bone index")))
            );
            assert_eq!(
                body.bone_type_matrix(&recs, b.id),
                Some(pose[group].bones[bone])
            );
        }
    }
    assert_eq!(body.find_bone_type(0xDEAD_BEEF), None);
}

/// A particle system carried at a holding location follows the first group's bone of the
/// location's number, even when another group names the location, and no bone when the first group
/// has none of that number. With no such location it follows that group's first bone.
///
/// Behaviour: parts.holding.a-carried-system-follows-the-first-groups-bone
#[test]
fn a_carried_system_follows_the_first_groups_bone_whichever_group_names_the_place() {
    let w = need_records!();
    let recs = CachedRecords::new(w);
    let mut body = Body::assemble(&recs, HUMAN_MALE).expect("the human male");
    body.advance(&recs, 0.1);
    let pose = body.pose(&recs);
    let mut on_another_group = 0;
    for (g, part) in body.parts.iter().enumerate().skip(1) {
        for l in &part.setup.holding_locations {
            let (found, _, own) = body.holding_location(&recs, l.kind).expect("named");
            if found != g {
                continue;
            }
            let want = pose[0].bones.get(l.bone as usize).copied();
            assert_eq!(body.carried_bone(&recs, l.kind), want, "kind {:#x}", l.kind);
            assert_ne!(want, Some(own), "kind {:#x} tells the groups apart", l.kind);
            on_another_group += 1;
        }
    }
    assert!(on_another_group > 0, "another group names some place");
    assert_eq!(
        body.carried_bone(&recs, 0xDEAD_BEEF),
        pose[0].bones.first().copied()
    );
}
