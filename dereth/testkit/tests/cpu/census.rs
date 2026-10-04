//! The census: what the registry documents for this tier, against what its scenarios declare.
//!
//! It compares two sets of **static text**: the ids the registry documents for this tier, and the
//! ids the scenarios *declare* in their `ALL` entries, and checks that each row's station is the
//! test of the scenario that declares it. Nothing here runs a scenario -- libtest has no after-all
//! hook, so a census that ran them would have to run the whole tier a second time inside itself.
//! What keeps a declaration honest is the scenario's own test, which runs it under a recorder and
//! fails unless declared == asserted -- see [`dereth_testkit::behaviours::run_scenario`], which
//! every `fn scenario_*` goes through. The checks themselves are [`dereth_testkit::tier_census`].

use std::collections::BTreeSet;

use dereth_testkit::behaviours::{self, Tier};
use dereth_testkit::tier_census::{self, ScenarioFile};

use super::{
    chat, combat, frame, inventory, login, magic, net, objects, panels, shell, social, ui, world,
};

/// Every declaration scope in this tier, including existing nested modules.
/// The compiled slices carry their exact module and wrapper identities. Omitting
/// an owning scope leaves a named registry gap; registering it twice is rejected.
static FILES: &[ScenarioFile] = &[
    chat::SCENARIOS,
    combat::SCENARIOS,
    frame::SCENARIOS,
    inventory::SCENARIOS,
    login::SCENARIOS,
    magic::SCENARIOS,
    net::SCENARIOS,
    objects::SCENARIOS,
    panels::SCENARIOS,
    shell::SCENARIOS,
    social::SCENARIOS,
    ui::SCENARIOS,
    world::SCENARIOS,
];

#[test]
fn every_documented_cpu_behaviour_is_asserted_by_a_scenario() {
    tier_census::assert_complete(Tier::Cpu, FILES);
}

/// **Every scenario in the table has a test of its own, and that test goes through the check.**
#[test]
fn every_scenario_has_a_test_that_checks_its_declaration() {
    tier_census::assert_every_scenario_is_run(FILES);
}

/// The whole registry, printed with each row's tier and station. The dat tier's scenario rows
/// are counted here as documented and undeclared, because this binary does not hold them -- which
/// is what the tier column is for, and why the gate above asks only about its own tier. Rows a
/// crate's own test asserts are printed and left out of every census.
#[test]
fn the_whole_registry_is_reported_with_its_tiers() {
    let nothing = BTreeSet::new();
    let all = behaviours::census(None, &nothing);
    let crate_rows = behaviours::all().filter(|b| !b.is_scenario()).count();
    println!(
        "registry: {} behaviours in {} subjects, {} asserted by scenarios and {crate_rows} by \
         crate tests",
        behaviours::count(),
        behaviours::SUBJECTS.len(),
        all.documented,
    );
    for s in behaviours::SUBJECTS {
        for b in s.rows {
            println!(
                "  [{}] {} {} -- {} -- {}",
                b.tier.binary(),
                s.name,
                b.id,
                b.evidence.id(),
                b.station
            );
        }
    }
    // **No pinned total.** The table is split by subject so that changes adding rows do not
    // collide on one line, and a hard-coded count here would put the collision back.
    // What is asserted instead is that the subject slices and the census agree: a subject file
    // that is written and never listed in `SUBJECTS` would make these two disagree.
    assert_eq!(
        all.documented + crate_rows,
        behaviours::count(),
        "the census counted {} scenario rows and {crate_rows} crate rows, and the subject files \
         hold {}",
        all.documented,
        behaviours::count()
    );
    let cpu = behaviours::census(Some(Tier::Cpu), &nothing);
    let dat = behaviours::census(Some(Tier::Dat), &nothing);
    assert_eq!(
        cpu.documented + dat.documented,
        all.documented,
        "a scenario row is in neither tier, or is counted in both"
    );
    assert!(
        dat.documented > 0,
        "no behaviour is marked for the dat tier; the tier column has stopped meaning anything"
    );
}
