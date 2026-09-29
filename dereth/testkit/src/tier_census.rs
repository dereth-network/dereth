//! The census each scenario binary runs over its own tier: the registry's scenario rows against
//! what the tier's scenario files declare, and the link from each declaration to the test that
//! checks it.
//!
//! Both are static: nothing here runs a scenario. A binary lists its scenario files once and
//! calls [`assert_complete`] and [`assert_every_scenario_is_run`] from two short tests.

use std::collections::BTreeSet;

use crate::behaviours::{self, Scenario, Tier, SCENARIO_PACKAGE};

/// One scenario file of a tier binary: its module name, which is the registry subject it holds,
/// and its `ALL` table.
pub type ScenarioFile = (&'static str, &'static [Scenario]);

/// Every scenario the files list, in file order.
#[must_use]
pub fn scenarios(files: &[ScenarioFile]) -> Vec<Scenario> {
    files
        .iter()
        .flat_map(|(_, list)| list.iter().copied())
        .collect()
}

/// Whether `station` names the test of scenario `scenario` in the file `module` of `tier`: that
/// file's test, at its top level or inside one of its inner modules.
#[must_use]
pub fn is_station_of(station: &str, tier: Tier, module: &str, scenario: &str) -> bool {
    let file = format!("{SCENARIO_PACKAGE}::{}::{module}::", tier.binary());
    station.starts_with(&file) && station.ends_with(&format!("::scenario_{scenario}"))
}

/// Assert that the tier's scenarios and the registry's rows for the tier are the same set: one
/// claim per scenario, no claim declared twice, every row declared and every declaration a row,
/// and every row's `station` the test of the scenario that declares it.
///
/// # Panics
/// Panics, naming each gap, when any of those does not hold.
pub fn assert_complete(tier: Tier, files: &[ScenarioFile]) {
    let all = scenarios(files);

    // **One scenario carries one claim.** It is asserted here rather than inferred from two
    // totals, so that a scenario declaring two ids or none is named instead of showing up as
    // arithmetic.
    let not_one: Vec<&str> = all
        .iter()
        .filter(|(_, ids, _)| ids.len() != 1)
        .map(|(name, _, _)| *name)
        .collect();
    assert!(
        not_one.is_empty(),
        "one scenario carries one behaviour, and these declare some other number: {not_one:?}"
    );

    let mut declared: BTreeSet<&'static str> = BTreeSet::new();
    let twice: Vec<&str> = all
        .iter()
        .flat_map(|(_, ids, _)| ids.iter().copied())
        .filter(|id| !declared.insert(*id))
        .collect();
    assert!(
        twice.is_empty(),
        "two scenarios declare the same behaviour: {twice:?}"
    );

    let c = behaviours::census(Some(tier), &declared);
    println!("{c}");
    assert!(
        c.complete(),
        "these documented behaviours have no scenario in the {} tier: {:?}; and these \
         declarations name nothing the tier documents: {:?}",
        tier.binary(),
        c.undeclared,
        c.undocumented
    );
    assert_eq!(
        c.documented,
        all.len(),
        "the tier documents {} behaviours and this binary lists {} scenarios",
        c.documented,
        all.len()
    );

    // **The row names the test that asserts it**, so a reader can go from a claim to its test and
    // a moved or renamed scenario is a red here rather than a stale pointer.
    let mut misplaced = Vec::new();
    for (module, list) in files {
        for (name, ids, _) in *list {
            for id in *ids {
                let row = behaviours::lookup(id).expect("the census above found every id");
                if !is_station_of(row.station, tier, module, name) {
                    misplaced.push(format!(
                        "{id}: {} (want the test of {module}'s scenario {name})",
                        row.station
                    ));
                }
            }
        }
    }
    assert!(
        misplaced.is_empty(),
        "these rows name another station than the scenario that asserts them: {misplaced:#?}"
    );
}

/// Every name that follows `marker` in `src`, up to the next `close`.
///
/// One pass rather than a `contains` per scenario: the `dat` tier's subject files are half a
/// megabyte each and a scan per scenario turned the census back into seconds.
fn names_after<'a>(src: &'a str, marker: &str, close: char) -> BTreeSet<&'a str> {
    let mut out = BTreeSet::new();
    let mut rest = src;
    while let Some(i) = rest.find(marker) {
        rest = &rest[i + marker.len()..];
        if let Some(j) = rest.find(close) {
            out.insert(&rest[..j]);
        }
    }
    out
}

/// `src` with the whitespace after every `scenario(` removed, so a call the formatter broke across
/// lines reads as the call it is.
fn join_calls(src: &str) -> String {
    const CALL: &str = "scenario(";
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(i) = rest.find(CALL) {
        let end = i + CALL.len();
        out.push_str(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    out.push_str(rest);
    out
}

/// Assert that **every scenario in the table has a test of its own, and that test goes through
/// the check**, reading each file's source under `dir`.
///
/// A static census counts a declaration and never runs it, so a scenario whose `#[test]` had been
/// deleted would still be counted. The link between the declaration and the run is therefore
/// asserted against the source -- the same thing the registry's
/// `every_subject_module_is_listed_in_subjects` test does for its own files.
///
/// # Panics
/// Panics when a file cannot be read, or names a scenario with no `fn scenario_<name>()` that
/// calls `scenario("<name>")`.
pub fn assert_every_scenario_is_run(dir: &str, files: &[ScenarioFile]) {
    for (stem, list) in files {
        let path = format!("{dir}{stem}.rs");
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let tests = names_after(&src, "fn scenario_", '(');
        let joined = join_calls(&src);
        let checked = names_after(&joined, "scenario(\"", '"');
        for (name, _, _) in *list {
            let name: &str = name;
            assert!(
                tests.contains(name),
                "{stem}.rs lists the scenario {name} and has no `fn scenario_{name}()` to run it"
            );
            assert!(
                checked.contains(name),
                "{stem}.rs's test for {name} does not go through the declared == asserted check"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_the_formatter_broke_across_lines_still_reads_as_the_call() {
        let src = "fn scenario_a() {\n    scenario(\n        \"a\",\n    );\n}\n";
        let joined = join_calls(src);
        assert!(names_after(&joined, "scenario(\"", '"').contains("a"));
        assert!(names_after(src, "fn scenario_", '(').contains("a"));
    }

    #[test]
    fn a_scenario_station_is_the_test_path_of_its_scenario() {
        let top = "dereth-testkit::dat::chat::scenario_a_line_is_drawn";
        let inner = "dereth-testkit::dat::chat::window::scenario_a_line_is_drawn";
        assert!(is_station_of(top, Tier::Dat, "chat", "a_line_is_drawn"));
        assert!(is_station_of(inner, Tier::Dat, "chat", "a_line_is_drawn"));
        assert!(!is_station_of(top, Tier::Cpu, "chat", "a_line_is_drawn"));
        assert!(!is_station_of(top, Tier::Dat, "social", "a_line_is_drawn"));
        assert!(!is_station_of(top, Tier::Dat, "chat", "a_line"));
    }
}
