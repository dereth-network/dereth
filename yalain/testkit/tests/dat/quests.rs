//! Quests' definitions as the server's world compile reads them from the September 2013 data: the
//! phases done by other quests.

use yalain_content::quests::Subquests;
use yalain_testkit::authored::Portal;
use yalain_world_db::compile::Install as _;
use yalain_world_db::install::DatInstall;

/// The three doctrines' quest is done in its first phase by the three kingdom stones' subquests
/// and in its second by the three doctrines handed in, all of each needed; its third has none.
/// Behaviour: server.quests.subquests-read
#[test]
fn a_quests_phases_read_the_subquests_they_are_done_by() {
    let p = Portal::open();
    let states = p
        .dat
        .entries_in(0x6F00_0000, 0x6FFF_FFFF)
        .map(|e| e.id)
        .collect();
    let install = DatInstall::new(Box::new(|id| p.dat.read(id).ok()), p.ctx.clone(), states)
        .expect("install");
    let def = install
        .quest_definition(None, 0x4000_005E)
        .expect("the doctrines' quest");
    assert_eq!(
        def.subquests,
        vec![
            Subquests {
                phase: 1,
                quests: vec![0x4000_012C, 0x4000_012D, 0x4000_012E],
                any: false,
            },
            Subquests {
                phase: 2,
                quests: vec![0x4000_005F, 0x4000_0060, 0x4000_0061],
                any: false,
            },
        ]
    );
    assert_eq!(def.journals.len(), 3);
    let stone = install
        .quest_definition(None, 0x4000_012D)
        .expect("a stone's subquest");
    assert_eq!(stone.journals, vec![(0x2E5C_1E52, 0x2500_17DC)]);
    assert!(stone.subquests.is_empty());
}
