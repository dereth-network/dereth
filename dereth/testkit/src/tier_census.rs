//! The census each scenario binary runs over its own tier: the registry's scenario rows against
//! what the tier's scenario files declare, and the link from each declaration to the test that
//! checks it.
//!
//! Both are static: nothing here runs a scenario. A binary lists its scenario files once and
//! calls [`assert_complete`] and [`assert_every_scenario_is_run`] from two short tests.

use std::collections::{BTreeMap, BTreeSet};

use crate::behaviours::{self, Scenario, Tier, SCENARIO_PACKAGE};

/// One scenario file of a tier binary: its module name, which is the registry subject it holds,
/// and its `ALL` table.
#[derive(Debug, Clone, Copy)]
pub struct ScenarioFile {
    /// The complete Rust module path, including the integration target.
    pub module: &'static str,
    /// The bodies and claims declared by this scope.
    pub scenarios: &'static [Scenario],
    /// The generated wrapper names in the same order as `scenarios`.
    pub tests: &'static [&'static str],
}

/// Every scenario the files list, in file order.
#[must_use]
pub fn scenarios(files: &[ScenarioFile]) -> Vec<Scenario> {
    files
        .iter()
        .flat_map(|file| file.scenarios.iter().copied())
        .collect()
}

/// Whether a station names this exact compiled wrapper in the requested tier.
#[must_use]
pub fn is_station_of(station: &str, tier: Tier, module: &str, test: &str) -> bool {
    module
        .split_once("::")
        .is_some_and(|(target, _)| target == tier.binary())
        && station == format!("{SCENARIO_PACKAGE}::{module}::{test}")
}

/// Assert that every documented claim has exactly one matching owner station.
///
/// A scenario can own several claims and assert shared claims owned by another
/// scenario or an ordinary test, including another tier. Every declared claim
/// must be known; this census owns only this tier's scenario stations. The runtime
/// recorder still checks each scenario's complete asserted set.
///
/// # Panics
/// Panics on missing, repeated or misplaced declarations and wrapper identities.
pub fn assert_complete(tier: Tier, files: &[ScenarioFile]) {
    assert_every_scenario_is_run(files);
    let rows: Vec<_> = behaviours::all()
        .filter(|row| row.is_scenario() && row.tier == tier)
        .map(|row| (row.id, row.station))
        .collect();
    let known: BTreeSet<_> = behaviours::all().map(|row| row.id).collect();
    let owned: BTreeSet<_> = rows.iter().map(|(id, _)| *id).collect();
    let declared = scenarios(files)
        .iter()
        .flat_map(|(_, ids, _)| ids.iter().copied())
        .filter(|id| owned.contains(id))
        .collect();
    println!("{}", behaviours::census(Some(tier), &declared));
    let errors = claim_errors(tier, files, &known, &rows);
    assert!(errors.is_empty(), "scenario census gaps: {errors:#?}");
}

fn claim_errors(
    tier: Tier,
    files: &[ScenarioFile],
    known: &BTreeSet<&str>,
    rows: &[(&str, &str)],
) -> Vec<String> {
    let documented: BTreeMap<_, _> = rows.iter().copied().collect();
    let mut declared = BTreeSet::new();
    let mut owners = BTreeMap::<&str, usize>::new();
    let mut errors = Vec::new();
    for file in files {
        for ((name, ids, _), test) in file.scenarios.iter().zip(file.tests) {
            let mut within = BTreeSet::new();
            if ids.is_empty() {
                errors.push(format!("{name}: no declared claims"));
            }
            for id in *ids {
                if !within.insert(*id) {
                    errors.push(format!("{}::{test}: duplicate claim {id}", file.module));
                    continue;
                }
                declared.insert(*id);
                if !known.contains(id) {
                    errors.push(format!("{name}: unknown claim: {id}"));
                    continue;
                }
                let Some(station) = documented.get(id) else {
                    continue;
                };
                if is_station_of(station, tier, file.module, test) {
                    *owners.entry(id).or_default() += 1;
                }
            }
        }
    }
    for id in documented.keys() {
        if !declared.contains(id) {
            errors.push(format!("undocumented by scenarios: {id}"));
        }
        let count = owners.get(id).copied().unwrap_or(0);
        if count != 1 {
            errors.push(format!("{id}: expected one owner station, found {count}"));
        }
    }
    if owners.values().sum::<usize>() != rows.len() {
        errors.push("owned claim count differs from documented claim count".into());
    }
    errors
}

/// Check the compiled declaration identities, without rereading source files.
///
/// The macro emits its slices and wrappers together. This check rejects duplicate
/// slice registration and duplicate wrapper identities before the registry census.
///
/// # Panics
/// Panics on inconsistent slices or repeated module/test identities.
pub fn assert_every_scenario_is_run(files: &[ScenarioFile]) {
    let mut modules = BTreeSet::new();
    let mut tests = BTreeSet::new();
    for file in files {
        assert!(
            modules.insert(file.module),
            "scenario scope listed twice: {}",
            file.module
        );
        assert_eq!(
            file.scenarios.len(),
            file.tests.len(),
            "{}: declaration/test mismatch",
            file.module
        );
        for ((name, _, _), test) in file.scenarios.iter().zip(file.tests) {
            assert_eq!(
                *test,
                format!("scenario_{name}"),
                "{}: wrong wrapper",
                file.module
            );
            assert!(
                tests.insert((file.module, test)),
                "duplicate wrapper: {}::{test}",
                file.module
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scenario_station_names_its_exact_module_and_wrapper() {
        let top = "dereth-testkit::dat::chat::scenario_a_line_is_drawn";
        let inner = "dereth-testkit::dat::chat::window::scenario_a_line_is_drawn";
        assert!(is_station_of(
            top,
            Tier::Dat,
            "dat::chat",
            "scenario_a_line_is_drawn"
        ));
        assert!(is_station_of(
            inner,
            Tier::Dat,
            "dat::chat::window",
            "scenario_a_line_is_drawn"
        ));
        assert!(!is_station_of(
            inner,
            Tier::Dat,
            "dat::chat",
            "scenario_a_line_is_drawn"
        ));
        assert!(!is_station_of(
            top,
            Tier::Cpu,
            "dat::chat",
            "scenario_a_line_is_drawn"
        ));
        assert!(!is_station_of(
            top,
            Tier::Dat,
            "dat::social",
            "scenario_a_line_is_drawn"
        ));
    }

    #[test]
    fn duplicate_scopes_cannot_satisfy_the_census() {
        let file = ScenarioFile {
            module: "cpu::chat",
            scenarios: &[],
            tests: &[],
        };
        assert!(std::panic::catch_unwind(|| assert_every_scenario_is_run(&[file, file])).is_err());
    }
    fn body() {}

    fn known() -> BTreeSet<&'static str> {
        [
            "chat.one",
            "chat.two",
            "chat.shared",
            "chat.external",
            "chat.ordinary",
        ]
        .into_iter()
        .collect()
    }

    const OWNER: &str = "dereth-testkit::cpu::chat::scenario_owner";
    const SHARED: &str = "dereth-testkit::cpu::chat::shared::scenario_shared";
    const ROWS: &[(&str, &str)] = &[
        ("chat.one", OWNER),
        ("chat.two", OWNER),
        ("chat.shared", SHARED),
    ];
    const OWNER_FILE: ScenarioFile = ScenarioFile {
        module: "cpu::chat",
        scenarios: &[("owner", &["chat.one", "chat.two", "chat.shared"], body)],
        tests: &["scenario_owner"],
    };
    const SHARED_FILE: ScenarioFile = ScenarioFile {
        module: "cpu::chat::shared",
        scenarios: &[("shared", &["chat.shared"], body)],
        tests: &["scenario_shared"],
    };

    #[test]
    fn multiple_owned_claims_and_shared_assertions_keep_one_owner_per_row() {
        let files = [OWNER_FILE, SHARED_FILE];
        assert_every_scenario_is_run(&files);
        assert!(claim_errors(Tier::Cpu, &files, &known(), ROWS).is_empty());
    }

    #[test]
    fn shared_assertion_cannot_replace_an_omitted_owner_slice() {
        let errors = claim_errors(Tier::Cpu, &[OWNER_FILE], &known(), ROWS);
        assert!(errors
            .iter()
            .any(|e| e == "chat.shared: expected one owner station, found 0"));
        // Every documented id is still declared: only ownership is missing.
        assert!(!errors
            .iter()
            .any(|e| e.starts_with("undocumented by scenarios:")));
    }

    #[test]
    fn unknown_duplicate_and_wrong_tier_claims_do_not_pass_through_shared_assertions() {
        let unknown = ScenarioFile {
            scenarios: &[("owner", &["chat.one", "chat.two", "chat.unknown"], body)],
            ..OWNER_FILE
        };
        assert!(
            claim_errors(Tier::Cpu, &[unknown, SHARED_FILE], &known(), ROWS)
                .iter()
                .any(|e| e.contains("unknown claim: chat.unknown"))
        );
        let repeated = ScenarioFile {
            scenarios: &[("owner", &["chat.one", "chat.two", "chat.one"], body)],
            ..OWNER_FILE
        };
        assert!(
            claim_errors(Tier::Cpu, &[repeated, SHARED_FILE], &known(), ROWS)
                .iter()
                .any(|e| e.ends_with("duplicate claim chat.one"))
        );
        assert!(
            claim_errors(Tier::Dat, &[OWNER_FILE, SHARED_FILE], &known(), ROWS)
                .iter()
                .any(|e| e.ends_with("expected one owner station, found 0"))
        );
    }

    #[test]
    fn known_cross_tier_and_ordinary_test_claims_are_valid_shared_assertions() {
        let extra = ScenarioFile {
            module: "cpu::chat::extra",
            scenarios: &[("extra", &["chat.external", "chat.ordinary"], body)],
            tests: &["scenario_extra"],
        };
        let errors = claim_errors(Tier::Cpu, &[OWNER_FILE, SHARED_FILE, extra], &known(), ROWS);
        assert!(errors.is_empty());
    }
}
