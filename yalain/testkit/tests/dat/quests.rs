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
    let lib = p.library();
    let install = DatInstall::new(
        Box::new(|id| p.dat.read(id).ok()),
        p.ctx.clone(),
        &lib,
        states,
    )
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

/// Whether a quest is done once is its setup's: the training hall quest's says one solution (done
/// once), the White Rabbit infiltration's two and Orlen Jarlns' any number (the globals' -1),
/// and a server errand's package (Stubby's) has no setup of its own that says.
/// Behaviour: server.quests.done-once-from-setup
#[test]
fn a_quests_setup_says_whether_it_is_done_once() {
    let p = Portal::open();
    let states = p
        .dat
        .entries_in(0x6F00_0000, 0x6FFF_FFFF)
        .map(|e| e.id)
        .collect();
    let lib = p.library();
    let install = DatInstall::new(
        Box::new(|id| p.dat.read(id).ok()),
        p.ctx.clone(),
        &lib,
        states,
    )
    .expect("install");
    let once = |q: u32| install.quest_definition(None, q).map(|d| d.once);
    assert_eq!(once(0x4000_0088), Some(true), "the training hall quest");
    assert_eq!(once(0x4000_0296), Some(false), "two solutions");
    assert_eq!(once(0x4000_018F), Some(false), "any number");
    assert_eq!(once(0x4000_037A), Some(false), "a server errand");
}

/// How a kill's move on a quest reaches its killer's fellows is the quest's setup's, and a last phase
/// that waits is its definition's: one setup shares with both limits, one with none, one not at all;
/// a quest with no setup of its own shares nothing unless a row says so, and its last phase, reading
/// "return to" someone, spans two.
/// Behaviour: server.quests.a-kills-reach-from-setup
#[test]
fn a_quests_setup_says_how_a_kill_reaches_fellows_and_its_definition_whether_its_last_phase_waits()
{
    let p = Portal::open();
    let states = p
        .dat
        .entries_in(0x6F00_0000, 0x6FFF_FFFF)
        .map(|e| e.id)
        .collect();
    let lib = p.library();
    let install = DatInstall::new(
        Box::new(|id| p.dat.read(id).ok()),
        p.ctx.clone(),
        &lib,
        states,
    )
    .expect("install");
    let read = |q: u32| {
        install.quest_definition(None, q).map(|d| {
            (
                d.fellows.shared,
                d.fellows.near_level,
                d.fellows.near_by,
                d.last_waits,
            )
        })
    };
    assert_eq!(
        read(0x4000_0112),
        Some((true, true, true, false)),
        "both limits"
    );
    assert_eq!(
        read(0x4000_0009),
        Some((true, false, false, false)),
        "no limit"
    );
    assert_eq!(
        read(0x4000_0010),
        Some((false, false, false, false)),
        "not shared"
    );
    assert_eq!(
        read(0x8000_00B7),
        Some((false, false, false, true)),
        "no setup"
    );
}

/// Every effect a creature template names that reads as a quest effect names a quest of the install.
/// Behaviour: server.quests.every-death-effect-names-a-quest
#[test]
fn every_quest_effect_a_creature_names_names_a_quest_of_the_install() {
    let p = Portal::open();
    let names: auberean_assets::strings::EnumMap = auberean_assets::decode(
        0x2300_0090,
        &p.dat.read(0x2300_0090).expect("the quests' names"),
        &p.ctx,
    )
    .expect("the quests' names read");
    let ids: std::collections::HashMap<String, u32> =
        names.names.into_iter().map(|(v, n)| (n, v)).collect();
    let entries: Vec<u32> = p
        .dat
        .entries_in(0x4700_0000, 0x47FF_FFFF)
        .map(|e| e.id)
        .collect();
    let prop_name = |id: u32| {
        p.ctx
            .properties()
            .and_then(|l| l.names.name(id))
            .map(str::to_string)
    };
    let entity = |id: u32| -> Option<auberean_assets::entity::Entity> {
        auberean_assets::decode(id, &p.dat.read(id).ok()?, &p.ctx).ok()
    };
    let (mut named, mut resolved) = (0usize, 0usize);
    let mut lost = Vec::new();
    for t in entries {
        let props = yalain_content::Props::of(t, &entity, &prop_name);
        let Some(stats) = yalain_content::CreatureStats::from_props(&props) else {
            continue;
        };
        for n in &stats.death_effects {
            let lower = n.to_ascii_lowercase();
            if ![
                "eff_increment",
                "eff_bestow",
                "eff_complete",
                "eff_advance",
                "eff_fail",
            ]
            .iter()
            .any(|v| lower.starts_with(v))
                || !lower.contains("quest")
            {
                continue;
            }
            named += 1;
            if yalain_world::quests::effect_named(n, |q| ids.get(q).copied()).is_some() {
                resolved += 1;
            } else {
                lost.push(n.clone());
            }
        }
    }
    assert!(named > 1000, "{named} quest effects named");
    assert_eq!(resolved, named, "not resolved: {lost:?}");
}
