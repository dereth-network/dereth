//! The `dat` tier's census: the registry's `dat`-tier scenario rows against what this binary's
//! scenarios declare. Set arithmetic over static text; see `tests/cpu/census.rs` for why it runs
//! nothing.

use dereth_testkit::behaviours::Tier;
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
    inventory::shop::SCENARIOS,
    inventory::trade_window::SCENARIOS,
    inventory::pickup::SCENARIOS,
    inventory::give::SCENARIOS,
    inventory::use_refusal::SCENARIOS,
    inventory::delivery::SCENARIOS,
    inventory::confirm::SCENARIOS,
    inventory::equip::SCENARIOS,
    inventory::split::SCENARIOS,
    inventory::burden::SCENARIOS,
    inventory::slots::SCENARIOS,
    inventory::icons::SCENARIOS,
    inventory::overlays::SCENARIOS,
    inventory::clicks::SCENARIOS,
    inventory::death::SCENARIOS,
    login::SCENARIOS,
    magic::SCENARIOS,
    net::SCENARIOS,
    objects::SCENARIOS,
    panels::SCENARIOS,
    shell::SCENARIOS,
    shell::chargen::SCENARIOS,
    social::SCENARIOS,
    ui::SCENARIOS,
    world::SCENARIOS,
    world::motion::SCENARIOS,
];

#[test]
fn every_documented_dat_behaviour_is_asserted_by_a_scenario() {
    tier_census::assert_complete(Tier::Dat, FILES);
}

/// **Every scenario in the table has a test of its own, and that test goes through the check.**
#[test]
fn every_scenario_has_a_test_that_checks_its_declaration() {
    tier_census::assert_every_scenario_is_run(FILES);
}
