//! `cargo xtask test-lint`: tests are named for what they prove and say which claim they prove.
//!
//! It reads sources only (no build) and applies four groups of rules:
//!
//! * **names**: test modules, functions and helpers are named for their purpose, and a tier
//!   binary with more than twelve modules keeps them in area directories;
//! * **anchors**: every `/// Behaviour: <id>` names a row of the behaviour registry, every registry
//!   row's `station` names a live test or scenario (a row marked `private_oracle` names one the
//!   workspace does not carry; see [`check_stations`]), a client crate's integration test module
//!   anchors at least one of its tests to a behaviour (or says `//! Behaviour: none` when it
//!   makes no behaviour claim), and a server test module carries an `ACE:`, `Vectors:` or
//!   `Divergence:` anchor;
//! * **comments**: the rules of `comment-lint`, applied to test code (the part that lint leaves
//!   out), plus one for `cargo test --test <name>` commands whose target no longer exists;
//! * **ignores**: an `#[ignore]` says why.
//!
//! ## Every finding fails
//!
//! There is no allowlist: the one it was seeded with at the start of the test reorganisation
//! (every finding the tree had then) was emptied by the batches and deleted at their close-out.
//! A finding is fixed by renaming the test, rewriting the comment or adding the anchor.
//!
//! `--list [path-prefix]` prints every finding; `--strict` is accepted and changes nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::comment_lint::{self, Scope};
use crate::testsrc::{self, behaviour_id, close_of, Ignore, Inventory, TokKind};
use crate::util::{print_table, workspace_root, Outcome, Report};

/// A test module file named after a piece of work (the test standard's rule 1).
const MODULE_NAME: &str =
    r"^([a-z]{1,2}\d+[a-z]?(_\d+[a-z]?)*|astra|fable|integration_[a-z]\d+|[a-z]\d+_sweep)(_|$)";

/// A `#[test]` fn named after a piece of work, or carrying a `gap_`/`test_` prefix (rule 2).
const FN_NAME: &str = r"^([a-z]{1,2}\d+[a-z]?(_\d+[a-z]?)*|astra|fable|rule3|gap|test)_";

/// Work labels on helpers and constants; ordinary `test_` helpers remain valid.
const ITEM_NAME: &str = r"(?i)^(?:[ophr]\d+[a-z]?(?:_\d+[a-z]?)*_|(?:astra|fable|rule3)(?:_|$))";

/// Where the behaviour registry's rows are, relative to the workspace root.
const REGISTRY_DIR: &str = "dereth/testkit/src/behaviours";

/// The crates whose integration tests prove client behaviour, so that each test module anchors
/// what it proves to a registry row.
const BEHAVIOUR_CRATES: &[&str] = &[
    "dereth-client",
    "dereth-client-model",
    "dereth-client-net",
    "dereth-client-runtime",
    "dereth-input",
    "dereth-ui",
    "dereth-ui-screens",
];

/// The crate whose scenario files are named for the registry's subjects.
const SCENARIO_PACKAGE: &str = "dereth-testkit";

/// A tier binary with more than this many modules groups them in area directories (rule 6).
const AREA_THRESHOLD: usize = 12;

/// Module directories that hold fixtures or instruments rather than a feature's tests.
const NOT_AREAS: &[&str] = &["common", "instruments"];

/// The rules, in report order, with the group each belongs to.
pub const RULES: &[(&str, &str)] = &[
    ("work-unit module name", "names"),
    ("work-unit test name", "names"),
    ("work-unit declaration name", "names"),
    ("module outside an area", "names"),
    ("unknown behaviour id", "anchors"),
    ("station does not resolve", "anchors"),
    ("private-oracle station is in the workspace", "anchors"),
    ("no behaviour anchor", "anchors"),
    ("no ACE anchor", "anchors"),
    ("ignore without a reason", "ignores"),
];

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub path: String,
    pub rule: String,
    pub line: usize,
    pub what: String,
}

fn declaration_findings(inv: &Inventory) -> Vec<Finding> {
    let item_re = Regex::new(ITEM_NAME).expect("item name rule");
    let module_re = Regex::new(MODULE_NAME).expect("module name rule");
    inv.declarations
        .iter()
        .filter_map(|d| {
            let matches = if d.module {
                module_re.is_match(d.name.trim_start_matches("r#"))
            } else {
                item_re.is_match(d.name.trim_start_matches("r#"))
            };
            if !matches {
                return None;
            }
            let is_test = inv
                .tests
                .iter()
                .any(|t| t.file == d.file && t.name == d.name && t.line == d.line);
            (!is_test).then(|| Finding {
                path: d.file.clone(),
                rule: if d.module {
                    "work-unit module name"
                } else {
                    "work-unit declaration name"
                }
                .into(),
                line: d.line,
                what: d.name.clone(),
            })
        })
        .collect()
}

/// A registry row, as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub station: String,
    /// The row says `private_oracle: true`: its station is a test the workspace does not carry.
    pub private_oracle: bool,
    pub file: String,
    pub line: usize,
}

/// Every row of the registry under `dir` (relative paths are reported from `ws`).
pub fn registry(ws: &Path, dir: &Path) -> Vec<Row> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "rs"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        let rel = rel(ws, &f);
        out.extend(registry_rows(&text, &rel));
    }
    out
}

/// The registry's subjects: the stems of the files under `dir`, `mod.rs` aside.
pub fn registry_subjects(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    let stem = p.file_stem()?.to_str()?.to_owned();
                    (p.extension().is_some_and(|x| x == "rs") && stem != "mod").then_some(stem)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The `behaviour! { id: .., station: .., }` rows in one file.
pub fn registry_rows(text: &str, file: &str) -> Vec<Row> {
    let toks = testsrc::lex(text);
    let mut out = Vec::new();
    let mut k = 0;
    while k + 2 < toks.len() {
        if toks[k].kind == TokKind::Ident
            && toks[k].text == "behaviour"
            && toks[k + 1].text == "!"
            && toks[k + 2].text == "{"
        {
            let close = close_of(&toks, k + 2);
            let field = |name: &str| {
                toks[k + 2..close]
                    .windows(3)
                    .find(|w| {
                        w[0].kind == TokKind::Ident
                            && w[0].text == name
                            && w[1].text == ":"
                            && w[2].kind == TokKind::Str
                    })
                    .map(|w| unquote(&w[2].text))
            };
            let private_oracle = toks[k + 2..close].windows(3).any(|w| {
                w[0].kind == TokKind::Ident
                    && w[0].text == "private_oracle"
                    && w[1].text == ":"
                    && w[2].text == "true"
            });
            if let (Some(id), Some(station)) = (field("id"), field("station")) {
                out.push(Row {
                    id,
                    station,
                    private_oracle,
                    file: file.to_owned(),
                    line: toks[k].line,
                });
            }
            k = close;
        }
        k += 1;
    }
    out
}

fn unquote(s: &str) -> String {
    s.strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(s)
        .to_owned()
}

fn rel(ws: &Path, p: &Path) -> String {
    p.strip_prefix(ws)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The scenario names each scenario module declares in its `ALL` table, as durable paths
/// `<package>::<target>::<module>::<name>`, each with the file that declares it.
fn scenarios(inv: &Inventory) -> BTreeMap<String, String> {
    inv.scenarios.iter().cloned().collect()
}

/// The stem a module file is named by: its file stem, or its directory's name for a `mod.rs`.
fn module_stem(file: &str) -> &str {
    let mut parts = file.rsplit('/');
    let last = parts.next().unwrap_or(file);
    let stem = last.strip_suffix(".rs").unwrap_or(last);
    if stem == "mod" {
        parts.next().unwrap_or(stem)
    } else {
        stem
    }
}

fn is_integration(target: &str) -> bool {
    target != "lib" && !target.starts_with("bin.")
}

/// Whether a module file sits in a fixture or instrument directory of its tier.
fn in_fixture_dir(file: &str) -> bool {
    file.split('/').any(|d| NOT_AREAS.contains(&d))
}

/// Check each registry row's station against the live tests (durable path -> file), and its
/// `private_oracle` mark against the workspace.
///
/// An unmarked station is a live test. A marked one is absent: its test compares against a
/// recording the workspace does not carry, so it lives outside it, and a marked row whose test is
/// in the workspace is a mark nobody took off.
pub fn check_stations(rows: &[Row], live: &BTreeMap<String, String>) -> Vec<Finding> {
    let mut out = Vec::new();
    for r in rows {
        match live.get(&r.station) {
            None if r.private_oracle => {}
            None => out.push(Finding {
                path: r.file.clone(),
                rule: "station does not resolve".into(),
                line: r.line,
                what: format!("{} -> {}", r.id, r.station),
            }),
            Some(file) if r.private_oracle => out.push(Finding {
                path: r.file.clone(),
                rule: PRIVATE_STATION.into(),
                line: r.line,
                what: format!(
                    "{}: the row is marked private_oracle, and {file} is in the workspace",
                    r.id
                ),
            }),
            Some(_) => {}
        }
    }
    out
}

/// The rule a marked station that is in the workspace breaks.
const PRIVATE_STATION: &str = "private-oracle station is in the workspace";

/// Every finding in the workspace at `ws`.
#[allow(clippy::too_many_lines)]
pub fn scan(ws: &Path) -> Result<Vec<Finding>, String> {
    let targets = testsrc::targets(ws)?;
    let inv = testsrc::read(ws, &targets);
    let mut out: BTreeSet<Finding> = BTreeSet::new();
    let module_re = Regex::new(MODULE_NAME).map_err(|e| e.to_string())?;
    let fn_re = Regex::new(FN_NAME).map_err(|e| e.to_string())?;
    out.extend(declaration_findings(&inv));
    out.extend(
        inv.scenario_errors
            .iter()
            .map(|(file, line, error)| Finding {
                path: file.clone(),
                line: *line,
                rule: "invalid scenario declaration".into(),
                what: error.clone(),
            }),
    );

    // Names: module files of integration tests.
    for f in &inv.files {
        if f.root || !is_integration(&f.target) || !comment_lint::is_test_path(&f.file) {
            continue;
        }
        let stem = module_stem(&f.file);
        if module_re.is_match(stem) {
            out.insert(Finding {
                path: f.file.clone(),
                rule: "work-unit module name".into(),
                line: 1,
                what: stem.to_owned(),
            });
        }
    }
    // Names: every test fn.
    for t in &inv.tests {
        if fn_re.is_match(t.name.trim_start_matches("r#")) {
            out.insert(Finding {
                path: t.file.clone(),
                rule: "work-unit test name".into(),
                line: t.line,
                what: t.name.clone(),
            });
        }
    }
    // Areas: a tier root with more than the threshold of modules keeps them in directories. The
    // scenario crate's modules named for a registry subject are that subject's area already.
    let subjects = registry_subjects(&ws.join(REGISTRY_DIR));
    let is_subject_module =
        |package: &str, name: &str| package == SCENARIO_PACKAGE && subjects.contains(name);
    let mut modules_per_target: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let with_tests: BTreeSet<&str> = inv.tests.iter().map(|t| t.file.as_str()).collect();
    for f in &inv.files {
        if !f.root
            && is_integration(&f.target)
            && !in_fixture_dir(&f.file)
            && with_tests.contains(f.file.as_str())
            && !f
                .module
                .first()
                .is_some_and(|name| is_subject_module(&f.package, name))
        {
            *modules_per_target
                .entry((&f.package, &f.target))
                .or_default() += 1;
        }
    }
    for f in inv
        .files
        .iter()
        .filter(|f| f.root && is_integration(&f.target))
    {
        let n = modules_per_target
            .get(&(f.package.as_str(), f.target.as_str()))
            .copied()
            .unwrap_or(0);
        if n <= AREA_THRESHOLD {
            continue;
        }
        for (name, line, file) in &f.decls {
            let is_dir = file.as_deref().is_some_and(|p| p.ends_with("/mod.rs"));
            if !is_dir
                && !NOT_AREAS.contains(&name.as_str())
                && !is_subject_module(&f.package, name)
            {
                out.insert(Finding {
                    path: f.file.clone(),
                    rule: "module outside an area".into(),
                    line: *line,
                    what: name.clone(),
                });
            }
        }
    }

    // Anchors: behaviour ids and registry stations.
    let rows = registry(ws, &ws.join(REGISTRY_DIR));
    let ids: BTreeSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    for t in &inv.tests {
        for d in &t.docs {
            if let Some(id) = behaviour_id(d) {
                if id != "none" && !ids.contains(id) {
                    out.insert(Finding {
                        path: t.file.clone(),
                        rule: "unknown behaviour id".into(),
                        line: t.line,
                        what: id.to_owned(),
                    });
                }
            }
        }
    }
    let live: BTreeMap<String, String> = inv
        .tests
        .iter()
        .map(|t| (t.durable(), t.file.clone()))
        .chain(scenarios(&inv))
        .collect();
    out.extend(check_stations(&rows, &live));
    // Anchors: per module file of an integration test.
    let mut by_file: BTreeMap<&str, Vec<&testsrc::TestFn>> = BTreeMap::new();
    for t in &inv.tests {
        by_file.entry(t.file.as_str()).or_default().push(t);
    }
    let mut seen_files = BTreeSet::new();
    for f in &inv.files {
        if !is_integration(&f.target)
            || in_fixture_dir(&f.file)
            || !seen_files.insert(f.file.as_str())
        {
            continue;
        }
        let Some(tests) = by_file.get(f.file.as_str()) else {
            continue;
        };
        let docs = || {
            f.inner_docs
                .iter()
                .chain(tests.iter().flat_map(|t| t.docs.iter()))
        };
        if BEHAVIOUR_CRATES.contains(&f.package.as_str())
            && !docs().any(|d| behaviour_id(d).is_some())
        {
            out.insert(Finding {
                path: f.file.clone(),
                rule: "no behaviour anchor".into(),
                line: 1,
                what: format!("{} test(s), none anchored", tests.len()),
            });
        }
        if f.package.starts_with("empyrean-")
            && !docs().any(|d| {
                let d = d.trim();
                d.starts_with("ACE:") || d.starts_with("Vectors:") || d.starts_with("Divergence:")
            })
        {
            out.insert(Finding {
                path: f.file.clone(),
                rule: "no ACE anchor".into(),
                line: 1,
                what: format!("{} test(s)", tests.len()),
            });
        }
    }

    // Ignores.
    for t in &inv.tests {
        if matches!(&t.ignore, Ignore::Always(None) | Ignore::Conditional(None))
            || matches!(&t.ignore, Ignore::Always(Some(r)) | Ignore::Conditional(Some(r)) if r.trim().is_empty() || r.trim() == "...")
        {
            out.insert(Finding {
                path: t.file.clone(),
                rule: "ignore without a reason".into(),
                line: t.line,
                what: t.name.clone(),
            });
        }
    }

    // Comments and messages in test code.
    let mut names: BTreeSet<String> = targets
        .iter()
        .filter(|t| is_integration(&t.name))
        .map(|t| t.name.clone())
        .collect();
    names.extend(["cpu", "dat", "gpu", "all", "local"].map(str::to_owned));
    let (comments, _) = comment_lint::scan_workspace(ws, Scope::Test, &names);
    for c in comments {
        out.insert(Finding {
            path: c.path,
            rule: c.rule.to_owned(),
            line: c.line,
            what: c.matched,
        });
    }
    Ok(out.into_iter().collect())
}

/// `cargo xtask test-lint [--strict | --list [prefix]]`.
///
/// Every finding fails. `--strict` is accepted and means the same (it was the form the lint took
/// once its seed allowlist was empty; the allowlist is gone). `--list [path-prefix]` prints every
/// finding and does not judge.
pub fn test_lint(args: &[String]) -> i32 {
    let ws = workspace_root();
    let findings = match scan(&ws) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("test-lint: {e}");
            return 2;
        }
    };
    let private_stations = registry(&ws, &ws.join(REGISTRY_DIR))
        .iter()
        .filter(|r| r.private_oracle)
        .count();
    if let Some(k) = args.iter().position(|a| a == "--list") {
        let prefix = args.get(k + 1).map_or("", String::as_str);
        for f in findings.iter().filter(|f| f.path.starts_with(prefix)) {
            println!("{}:{}: {}: {}", f.path, f.line, f.rule, f.what);
        }
        return 0;
    }
    if let Some(bad) = args.iter().find(|a| a.as_str() != "--strict") {
        eprintln!(
            "test-lint: unknown argument `{bad}`; usage: test-lint [--strict | --list [prefix]]"
        );
        return 2;
    }
    for f in &findings {
        println!("{}:{}: {}: {}", f.path, f.line, f.rule, f.what);
    }
    let groups = ["names", "anchors", "ignores", "comments"];
    let group_of = |rule: &str| {
        RULES
            .iter()
            .find(|(r, _)| *r == rule)
            .map_or("comments", |(_, g)| *g)
    };
    let mut reports = Vec::new();
    for g in groups {
        let n = findings.iter().filter(|f| group_of(&f.rule) == g).count();
        reports.push(Report::new(
            match g {
                "names" => "tests are named for what they prove",
                "anchors" => "tests name the claim they prove",
                "ignores" => "an ignored test says why",
                _ => "test comments describe behaviour",
            },
            if n == 0 { Outcome::Pass } else { Outcome::Fail },
            format!("{n} finding(s)"),
        ));
    }
    reports.push(Report::new(
        "private-oracle tests stay out of the workspace",
        Outcome::Pass,
        format!("{private_stations} registry row(s) marked private_oracle"),
    ));
    print_table("xtask test-lint", &reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_names_reject_private_labels_and_preserve_public_numeric_names() {
        let names = [
            concat!("O", "422_COUNT"),
            concat!("p", "1_41_press"),
            concat!("rule", "3_roundtrip"),
            concat!("r#o", "422_setup"),
            "F7",
            "F750",
            "u32_at",
            "D0",
            "T0",
            "P1",
            "p16",
            "test_client",
            "contract7",
        ];
        let inv = Inventory {
            declarations: names
                .iter()
                .enumerate()
                .map(|(line, name)| testsrc::Declaration {
                    file: "sample.rs".into(),
                    line,
                    name: (*name).into(),
                    module: false,
                })
                .collect(),
            ..Inventory::default()
        };
        let findings = declaration_findings(&inv);
        assert_eq!(
            findings.iter().map(|f| f.what.as_str()).collect::<Vec<_>>(),
            &names[..4]
        );
    }

    #[test]
    fn the_standards_name_rules_catch_work_unit_names_and_pass_claims() {
        let m = Regex::new(MODULE_NAME).expect("module rule");
        for bad in [
            concat!("o", "89_contain"),
            concat!("p", "1_70b_things"),
            concat!("p", "1_164_example_module"),
            concat!("r", "2_1_x"),
            concat!("a", "stra_old_samplers"),
            concat!("f", "able_door_collision"),
            "integration_q1",
            "z8_sweep",
            "g25",
        ] {
            assert!(m.is_match(bad), "{bad} passed the module rule");
        }
        for good in [
            "death_animation",
            "keyboard_selection",
            "d3d12_device",
            "corpus",
            "degrade",
        ] {
            assert!(!m.is_match(good), "{good} failed the module rule");
        }
        let f = Regex::new(FN_NAME).expect("fn rule");
        for bad in [
            concat!("p", "1_164_an_example_claim"),
            "gap_something",
            "test_parse",
            concat!("a", "stra_bc2_mips"),
            concat!("o", "12_x"),
        ] {
            assert!(f.is_match(bad), "{bad} passed the fn rule");
        }
        for good in [
            "a_selected_spell_outranks_the_wand",
            "testing_is_fine",
            "gapless_playback",
            "m1",
        ] {
            assert!(!f.is_match(good), "{good} failed the fn rule");
        }
    }

    #[test]
    fn registry_rows_are_read_from_their_macro_calls() {
        let rows = registry_rows(
            "pub static ROWS: &[Behaviour] = &[\n    behaviour! {\n        id: \"a.b.c\",\n        says: \"x\",\n        since: RETAIL,\n        evidence: Evidence::Private(\"AC-EVID-X\"),\n        station: \"dereth-client::gpu::objects::death::falls_once\",\n        tier: Tier::Dat,\n    },\n];\n",
            "behaviours/objects.rs",
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "a.b.c");
        assert!(!rows[0].private_oracle);
        let marked = registry_rows(
            "behaviour! {\n    id: \"a.b.d\",\n    station: \"x::gpu::y::z\",\n    private_oracle: true,\n    tier: Tier::Gpu,\n}\n",
            "behaviours/objects.rs",
        );
        assert!(marked[0].private_oracle);
        assert_eq!(
            rows[0].station,
            "dereth-client::gpu::objects::death::falls_once"
        );
        assert_eq!(rows[0].line, 2);
    }

    fn row(id: &str, station: &str, private_oracle: bool) -> Row {
        Row {
            id: id.into(),
            station: station.into(),
            private_oracle,
            file: "behaviours/x.rs".into(),
            line: 1,
        }
    }

    #[test]
    fn a_private_oracle_mark_is_held_to_the_workspace() {
        let live = BTreeMap::from([(
            "a::gpu::public::t".to_owned(),
            "a/tests/gpu/public.rs".to_owned(),
        )]);
        let rules = |rows: &[Row]| {
            check_stations(rows, &live)
                .into_iter()
                .map(|f| f.rule)
                .collect::<Vec<_>>()
        };
        let gone = "station does not resolve";
        // A marked station is absent, an unmarked one live.
        assert!(rules(&[row("r", "a::gpu::gone::t", true)]).is_empty());
        assert!(rules(&[row("q", "a::gpu::public::t", false)]).is_empty());
        assert_eq!(rules(&[row("r", "a::gpu::gone::t", false)]), [gone]);
        assert_eq!(
            rules(&[row("q", "a::gpu::public::t", true)]),
            [PRIVATE_STATION]
        );
    }

    #[test]
    fn a_scenario_declaration_is_a_station() {
        let dir = std::env::temp_dir().join("xtask-test-lint-scenarios");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let root = dir.join("main.rs");
        std::fs::write(&root, "mod chat;\n").expect("write");
        std::fs::write(
            dir.join("chat.rs"),
            r#"dereth_testkit::scenarios! {
                scenario_a_line_is_drawn => a_line_is_drawn ["chat.x.y"],
                scenario_another => another ["chat.x.z"],
            }"#,
        )
        .expect("write");
        let inv = testsrc::read(
            &dir,
            &[testsrc::Target {
                package: "dereth-testkit".into(),
                name: "cpu".into(),
                root,
            }],
        );
        std::fs::remove_dir_all(&dir).ok();
        let got = scenarios(&inv);
        assert!(got.contains_key("dereth-testkit::cpu::chat::a_line_is_drawn"));
        assert!(got.contains_key("dereth-testkit::cpu::chat::another"));
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn the_registry_subjects_are_its_file_stems_without_mod() {
        let dir = std::env::temp_dir().join("xtask-test-lint-subjects");
        std::fs::create_dir_all(&dir).expect("temp dir");
        for f in ["mod.rs", "chat.rs", "rendering.rs", "notes.txt"] {
            std::fs::write(dir.join(f), "").expect("write");
        }
        let got = registry_subjects(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(
            got,
            BTreeSet::from(["chat".to_owned(), "rendering".to_owned()])
        );
    }

    #[test]
    fn subject_descendants_do_not_turn_other_root_modules_into_areas() {
        let dir = std::env::temp_dir().join("xtask-test-lint-subject-descendants");
        let subjects = dir.join(REGISTRY_DIR);
        let tier = dir.join("tests/cpu");
        std::fs::create_dir_all(&subjects).expect("registry directory");
        std::fs::create_dir_all(tier.join("chat")).expect("subject directory");
        std::fs::write(subjects.join("chat.rs"), "").expect("registered subject");
        let manifest = |package: &str| {
            format!(
                "[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\
                 [[test]]\nname = \"cpu\"\npath = \"tests/cpu/main.rs\"\n"
            )
        };
        std::fs::write(dir.join("Cargo.toml"), manifest(SCENARIO_PACKAGE)).expect("manifest");
        std::fs::write(tier.join("main.rs"), "mod chat;\nmod checks;\n").expect("tier root");
        let test = "//! Behaviour: none (area layout fixture)\n#[test]\nfn a_line_is_drawn() {}\n";
        std::fs::write(tier.join("checks.rs"), test).expect("ordinary root module");
        let mut children = String::new();
        for index in 0..=AREA_THRESHOLD {
            let name = format!("messages_{index}");
            children.push_str(&format!("mod {name};\n"));
            std::fs::write(tier.join("chat").join(format!("{name}.rs")), test)
                .expect("subject child");
        }
        std::fs::write(tier.join("chat.rs"), children).expect("subject parent");
        let area_findings = || {
            scan(&dir)
                .expect("scan fixture")
                .into_iter()
                .filter(|finding| finding.rule == "module outside an area")
                .map(|finding| finding.what)
                .collect::<Vec<_>>()
        };
        assert!(
            area_findings().is_empty(),
            "children remain in their subject"
        );
        std::fs::remove_file(subjects.join("chat.rs")).expect("remove subject registration");
        assert!(area_findings().contains(&"checks".to_owned()));
        std::fs::write(subjects.join("chat.rs"), "").expect("restore subject registration");
        std::fs::write(dir.join("Cargo.toml"), manifest("ordinary-tests")).expect("other package");
        assert!(area_findings().contains(&"checks".to_owned()));
        std::fs::remove_dir_all(&dir).expect("remove fixture");
    }

    #[test]
    fn a_mod_rs_is_named_by_its_directory() {
        assert_eq!(module_stem("x/tests/gpu/rendering/mod.rs"), "rendering");
        assert_eq!(module_stem("x/tests/gpu/rendering.rs"), "rendering");
    }
}
