//! The mounts as the server reads them from the September 2013 data: the saddles' and eggs'
//! effects, the mount table's looks and the bodies' own appearance.

use auberean_protocol::types::{PartGroupDesc, VisualDesc};
use yalain_testkit::authored::Portal;
use yalain_world::effect_defs::Kind;
use yalain_world::mounts::MountEffect;

const LEY_SADDLE: u32 = 0x4700_089E;
const MARA_SADDLE: u32 = 0x4700_08A6;
const EGG: u32 = 0x4700_14CC;
const PATTERN: u32 = 0x4700_089D;

/// The ephemeral Ley Ataur saddle puts on its fifteen-minute effect (type 1, its ending
/// appearance), the egg its own held until taken off (type 8, none), a saddle pattern nothing;
/// the effects multiply speed by 1.4 and 1.9; the table holds a look for every type, race and sex,
/// each with its motion and part groups, and a body's own appearance reads.
/// Behaviour: server.mounts.read-from-their-definitions
#[test]
fn the_mounts_read_as_the_data_defines_them() {
    let p = Portal::open();
    let m = p.mounts(&[LEY_SADDLE, MARA_SADDLE, EGG, PATTERN]);
    assert_eq!(m.items.get(&LEY_SADDLE), Some(&0x6F00_19F2));
    assert_eq!(m.items.get(&MARA_SADDLE), Some(&0x6F00_19F8));
    assert_eq!(m.items.get(&EGG), Some(&0x6F00_1C4C));
    assert!(!m.items.contains_key(&PATTERN));
    assert_eq!(
        m.effects.get(&0x6F00_19F2),
        Some(&MountEffect {
            mount_type: 0x4000_0001,
            ending_fx: 0x4100_0537
        })
    );
    assert_eq!(
        m.effects.get(&0x6F00_1C4C),
        Some(&MountEffect {
            mount_type: 0x4000_0008,
            ending_fx: 0
        })
    );
    let t = p.effects(&[0x6F00_19F2, 0x6F00_1C4C]);
    let ley = t.get(0x6F00_19F2).expect("the saddle's effect");
    assert_eq!(ley.duration.at(1.0), Some(900.0));
    assert_eq!(ley.fx, 0x4100_0536);
    assert!(ley.removed_on_death());
    assert!(ley.changes(1.0).contains(&(0x3_0127, true, 1.4)));
    let hero = t.get(0x6F00_1C4C).expect("the egg's effect");
    assert!(hero.duration.is_empty());
    assert!(matches!(hero.kind, Kind::Numeric { .. }));
    assert!(hero.changes(1.0).contains(&(0x3_0127, true, 1.9)));
    // Nine types, five races, two sexes.
    assert_eq!(m.looks.len(), 90);
    let look = &m.looks[&(0x4000_0005, 2, 0x1000)];
    assert_eq!(look.did, 0x1F00_0526);
    assert_eq!(
        (look.motion_interp, look.behavior_table, look.modes),
        (0x3200_000E, 0x4400_0001, 0x5400_0006)
    );
    assert_eq!(look.global.fx_table, 0x1E00_0039);
    assert_eq!(look.global.bits & PartGroupDesc::APPEARANCE, 0);
    assert_eq!(look.part_groups.len(), 7);
    assert!(m
        .looks
        .values()
        .all(|l| l.bits & VisualDesc::PART_GROUPS != 0));
    let face = m
        .bodies
        .get(&0x1F00_007A)
        .and_then(|b| b.get(0x2000_000A))
        .expect("a body's own appearance");
    assert_eq!(face.get(12), Some(&0.6));
}

/// The mount items a start holds are the install's saddles, the fifteen templates of the saddle
/// class whose use mounts their user (not the class's bare template or its test), and the three
/// eggs; the trophy that mounts its user and is no egg is not among them.
/// Behaviour: server.mounts.items-from-the-install
#[test]
fn the_mount_items_are_the_fifteen_saddles_and_the_three_eggs() {
    let p = Portal::open();
    let items = p.mount_items();
    assert_eq!(
        items,
        [
            0x4700_089E,
            0x4700_089F,
            0x4700_08A0,
            0x4700_08A2,
            0x4700_08A3,
            0x4700_08A4,
            0x4700_08A6,
            0x4700_08A7,
            0x4700_08A8,
            0x4700_08AA,
            0x4700_08AB,
            0x4700_08AC,
            0x4700_08AE,
            0x4700_08AF,
            0x4700_08B0,
            0x4700_14CA,
            0x4700_14CB,
            0x4700_14CC,
        ]
    );
    assert!(!items.contains(&0x4700_1BFD));
    // The Golden Saddle's use mounts too, by the effect enumeration, as the eggs' do.
    assert!(p.mounts(&[0x4700_134A]).items.contains_key(&0x4700_134A));
    assert!(!items.contains(&0x4700_134A));
}
