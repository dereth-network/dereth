//! The `dat` tier's census: the registry's `dat`-tier scenario rows against what this binary's
//! scenarios declare. Set arithmetic over static text; see `tests/cpu/census.rs` for why it runs
//! nothing.

use dereth_testkit::behaviours::Tier;
use dereth_testkit::tier_census::{self, ScenarioFile};

use super::{
    chat, combat, frame, inventory, login, magic, net, objects, panels, shell, social, ui, world,
};

/// Every scenario file of this tier: the module's stem, which is also the file's and the registry
/// subject it holds, and the scenarios it lists.
///
/// Assembled from the per-subject files rather than written out here, so that adding a scenario is
/// one edit in the file it belongs to. A file that forgot to list its own scenario shows up as a
/// shortfall in the census and not as a silent gap.
static FILES: &[ScenarioFile] = &[
    ("chat", chat::ALL),
    ("combat", combat::ALL),
    ("frame", frame::ALL),
    ("inventory", inventory::ALL),
    ("login", login::ALL),
    ("magic", magic::ALL),
    ("net", net::ALL),
    ("objects", objects::ALL),
    ("panels", panels::ALL),
    ("shell", shell::ALL),
    ("social", social::ALL),
    ("ui", ui::ALL),
    ("world", world::ALL),
];

#[test]
fn every_documented_dat_behaviour_is_asserted_by_a_scenario() {
    tier_census::assert_complete(Tier::Dat, FILES);
}

/// **Every scenario in the table has a test of its own, and that test goes through the check.**
#[test]
fn every_scenario_has_a_test_that_checks_its_declaration() {
    tier_census::assert_every_scenario_is_run(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/dat/"),
        FILES,
    );
}
