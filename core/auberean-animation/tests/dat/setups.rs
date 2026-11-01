//! Every setup of the end-of-retail set, packed back into its stored form.

use std::collections::BTreeMap;

use auberean_animation::setups::{pack, pack_size, BoneTypes};
use auberean_assets::setup::{Location, Pose, Setup};
use auberean_assets::{Context, Decode};
use auberean_dat::Reader;

use crate::common::{ids, portal};

fn by_kind<T: Clone>(items: &[T], kind: impl Fn(&T) -> u32) -> BTreeMap<u32, Vec<T>> {
    let mut m: BTreeMap<u32, Vec<T>> = BTreeMap::new();
    for x in items {
        m.entry(kind(x)).or_default().push(x.clone());
    }
    m
}

fn places(l: &[Location]) -> BTreeMap<u32, Vec<Location>> {
    by_kind(l, |x| x.kind)
}

fn poses(p: &[Pose]) -> BTreeMap<u32, Vec<Pose>> {
    by_kind(p, |x| x.kind)
}

/// Every setup packs to the size its packed size says and back into the same setup (its poses and
/// places in the order of the tables that hold them, only the two flags the client keeps); a setup
/// whose stored flags are just those two and whose tables hold one entry a bucket packs to its
/// stored bytes exactly. The first bone of a type is the one found for it.
///
/// Behaviour: animation.setups.every-setup-packs-back-to-itself
#[test]
fn every_setup_packs_back_to_itself() {
    let portal = portal();
    let ctx = Context::from_portal(portal).expect("the property list");
    let list = ids(0x0400_0000, 0x04FF_FFFF);
    let (mut setups, mut exact, mut reordered, mut dup_kinds) = (0usize, 0usize, 0usize, 0usize);
    for &id in &list {
        let raw = portal.read(id).expect("the record reads");
        let Ok(s) = Setup::decode(&mut Reader::new(&raw), id, &ctx) else {
            continue;
        };
        setups += 1;
        let bytes = pack(id, &s);
        assert_eq!(bytes.len(), pack_size(&s), "{id:08X}: the packed size");
        assert_eq!(bytes.len(), raw.len(), "{id:08X}: the stored size");
        let back = Setup::decode(&mut Reader::new(&bytes), id, &ctx).expect("packed decodes");
        assert_eq!(back.flags, s.flags & 0xC, "{id:08X}");
        assert_eq!(poses(&back.poses), poses(&s.poses), "{id:08X}");
        assert_eq!(
            places(&back.holding_locations),
            places(&s.holding_locations)
        );
        assert_eq!(
            places(&back.connection_points),
            places(&s.connection_points)
        );
        let rest = |x: &Setup| {
            let mut x = x.clone();
            x.flags = 0;
            x.poses.clear();
            x.holding_locations.clear();
            x.connection_points.clear();
            x
        };
        assert_eq!(rest(&back), rest(&s), "{id:08X}");
        if bytes == raw {
            exact += 1;
        } else {
            reordered += 1;
        }
        let dups = |m: BTreeMap<u32, usize>| m.values().any(|&n| n > 1);
        let count = |k: Vec<u32>| {
            let mut m = BTreeMap::new();
            for x in k {
                *m.entry(x).or_insert(0usize) += 1;
            }
            m
        };
        if dups(count(s.poses.iter().map(|p| p.kind).collect()))
            || dups(count(s.holding_locations.iter().map(|p| p.kind).collect()))
            || dups(count(s.connection_points.iter().map(|p| p.kind).collect()))
        {
            dup_kinds += 1;
        }
        let types = BoneTypes::new(&s);
        for (i, b) in s.bones.iter().enumerate() {
            let first = types.find(b.id).expect("every bone's type is found");
            assert!(first as usize <= i);
            assert_eq!(s.bones[first as usize].id, b.id);
        }
    }
    eprintln!(
        "{setups} setups: {exact} pack to their stored bytes, {reordered} in another order; \
         {dup_kinds} hold a kind twice"
    );
    assert!(setups > 1000, "{setups}");
    assert!(exact > 0);
}
