//! A camp that keeps its creatures for a master camp, as an outpost's guards stand with their
//! beacon. Fixture: the test world's camp of one beetle as the master, and a second camp of three
//! beetles keeping theirs for it.

use yalain_testkit::Harness;
use yalain_world::combat;
use yalain_world::things::Mode;

const MASTER: usize = 0;
const GUARDS: usize = 1;

fn world() -> (Harness, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let mut guards = yalain_testkit::test_camp(3);
    guards.id = "guards".into();
    let master = h.server.spawns.camps[MASTER].id.clone();
    h.server.spawns.camps.push(guards);
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    h.server.spawns.masters = vec![("guards".into(), master)];
    let (_, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    (h, me)
}

/// The creatures a camp keeps standing, alive.
fn kept(h: &Harness, camp: usize) -> Vec<u64> {
    h.server.camps[camp]
        .alive
        .iter()
        .copied()
        .filter(|id| {
            h.server
                .things
                .get(*id)
                .and_then(|t| t.creature.as_ref())
                .is_some_and(|c| c.mode != Mode::Dead)
        })
        .collect()
}

fn kill(h: &mut Harness, by: u64, id: u64) {
    let t = h.server.things.get_mut(id).expect("the creature");
    t.health = 1;
    if let Some(c) = t.creature.as_mut() {
        c.stats.attacks_first = false;
    }
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, id, by, 50, clock);
    h.run(2);
}

fn run_for(h: &mut Harness, seconds: f64) {
    let until = h.now.0 + seconds;
    while h.now.0 < until {
        h.step_by(0.25);
    }
}

/// The guards stand while their master does; as the master's last creature dies they leave the
/// world at once, leaving no corpse, and stay gone until the master makes another, when they all
/// stand again. A guard's own death touches the master not at all.
/// Behaviour: server.spawns.a-camp-stands-with-its-master
#[test]
fn a_camps_creatures_leave_with_its_masters_last_and_stand_again_with_its_next() {
    let (mut h, me) = world();
    assert_eq!(kept(&h, MASTER).len(), 1, "the master's one");
    let guards = kept(&h, GUARDS);
    assert_eq!(guards.len(), 3, "the guards stand with it");
    kill(&mut h, me, guards[0]);
    assert_eq!(
        kept(&h, MASTER).len(),
        1,
        "a guard's death leaves the master be"
    );
    assert_eq!(kept(&h, GUARDS).len(), 2);
    let beacon = kept(&h, MASTER)[0];
    let left = kept(&h, GUARDS);
    kill(&mut h, me, beacon);
    assert!(kept(&h, GUARDS).is_empty(), "the guards go as it dies");
    assert!(
        left.iter().all(|id| h.server.things.get(*id).is_none()),
        "gone from the world, no corpse left"
    );
    let corpses = |h: &Harness| {
        h.server
            .things
            .by_id
            .values()
            .filter(|t| t.corpse.is_some())
            .count()
    };
    run_for(&mut h, 30.0);
    assert!(
        kept(&h, GUARDS).is_empty(),
        "none while the master keeps none"
    );
    assert_eq!(corpses(&h), 2, "the first guard's and the master's alone");
    run_for(&mut h, 40.0);
    assert_eq!(kept(&h, MASTER).len(), 1, "the master's next");
    assert_eq!(
        kept(&h, GUARDS).len(),
        3,
        "and the guards with it, all at once"
    );
}

/// With no master named, a camp keeps its creatures as any other.
/// Behaviour: server.spawns.a-camp-stands-with-its-master
#[test]
fn a_camp_with_no_master_keeps_its_creatures_through_another_camps_death() {
    let (mut h, me) = world();
    h.server.spawns.masters.clear();
    let beacon = kept(&h, MASTER)[0];
    kill(&mut h, me, beacon);
    assert_eq!(kept(&h, GUARDS).len(), 3);
}
