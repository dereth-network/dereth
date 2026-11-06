//! Divergence: V462
//! Revision-pinned world-base conversion preserves provenance and refuses unknown semantics.
//! Fixture: a local Git repository containing only synthetic SQL and source loot inputs.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use empyrean_common::era::EraId;
use empyrean_content::{PackContent, WorldDatabase};
use serde_json::Value;

const DATABASE: &str = "Server/Database/AC1999-World";

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}

fn fixture(name: &str) -> (PathBuf, PathBuf, String) {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("ac1999-{name}-{}", std::process::id()));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let repo = root.join("repo");
    let base_dir = repo.join(DATABASE).join("1-Base");
    let core_dir = repo.join(DATABASE).join("3-Core");
    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::create_dir_all(&core_dir).unwrap();
    let mut dump = std::fs::read_to_string(crate::patch_fixtures().join("base.sql")).unwrap();
    let cut = dump.find("INSERT INTO `weenie`").unwrap();
    let mut rows = dump.split_off(cut);
    dump = include_str!("../../../schema/ac1999_world.sql").to_owned();
    rows = rows.replace(
        "INSERT INTO `recipe_mod` VALUES (3,5,",
        "INSERT INTO `recipe_mod` VALUES (3,5,3,1,",
    );
    rows = rows.replace("'TestQuest',3600", "'TestQuest',-1");
    rows = rows
        .lines()
        .filter(|l| {
            !l.starts_with("INSERT INTO `version`")
                && !l.starts_with("INSERT INTO `points_of_interest`")
        })
        .collect::<Vec<_>>()
        .join("\n");
    rows.push_str("\nINSERT INTO `treasure_death` VALUES (1,100,1,0.5,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,'2000-01-01 00:00:00');\nINSERT INTO `weenie_properties_spell_book` (`object_Id`,`spell`) VALUES (100,999);\n");
    std::fs::write(base_dir.join("WorldBase.sql"), dump).unwrap();
    std::fs::write(core_dir.join("content.sql"), rows).unwrap();
    let treasure = repo.join("Server/Bin/Data/Treasure");
    std::fs::create_dir_all(&treasure).unwrap();
    write_loot_fixture(&treasure);
    std::fs::write(repo.join("Server/LICENSE"), "Synthetic test fixture\n").unwrap();
    git(&repo, &["init"]);
    let identity = Path::new(env!("CARGO_MANIFEST_DIR"));
    git(
        &repo,
        &[
            "config",
            "user.name",
            &git(
                identity,
                &["config", "--default", "World fixture", "--get", "user.name"],
            ),
        ],
    );
    git(
        &repo,
        &[
            "config",
            "user.email",
            &git(
                identity,
                &[
                    "config",
                    "--default",
                    "fixture@example.invalid",
                    "--get",
                    "user.email",
                ],
            ),
        ],
    );
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    git(&repo, &["add", "Server"]);
    git(
        &repo,
        &[
            "-c",
            "core.hooksPath=",
            "commit",
            "-m",
            "test: synthetic world",
        ],
    );
    let sha = git(&repo, &["rev-parse", "HEAD"]);
    (root, repo, sha)
}

/// Serialize original synthetic policy data in the upstream file layout.
fn write_loot_fixture(root: &Path) {
    let mut p: Value = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/source-loot/synthetic.json"
    ))
    .unwrap();
    let write = |name: &str, value: &Value| {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    };
    write("profile.json", &p["profile"]);
    write("material-colors.json", &p["material_colors"]);
    write("material-values.json", &p["material_values"]);
    for (id, tier) in p["tiers"].as_object().unwrap() {
        write(&format!("Tiers/tier-{id}.json"), tier);
    }
    for (name, cats) in p["pools"].as_object_mut().unwrap() {
        for c in cats.as_array_mut().unwrap() {
            c.as_object_mut().unwrap().remove("mutationBands");
            for e in c["entries"].as_array_mut().unwrap() {
                e["wcid"] = 100.into();
                e["elementalVariants"] = serde_json::json!([]);
            }
        }
        let value = if name == "gems" { &cats[0] } else { cats };
        write(&format!("Pools/{name}.json"), value);
    }
    write(
        "Mutations/armor.json",
        &serde_json::json!({"armor":p["mutations"]["armor"]}),
    );
    write(
        "Mutations/melee-weapons.json",
        &serde_json::json!({"swords":p["mutations"]["swords"]}),
    );
    write(
        "Mutations/missile-weapons.json",
        &serde_json::json!({"bows":p["mutations"]["bows"],"thrown":p["mutations"]["thrown"]}),
    );
    for (name, families) in p["spells"].as_object_mut().unwrap() {
        for f in families.as_array_mut().unwrap() {
            f["spells"] = serde_json::json!([0, 0, 0, 0, 0, 0]);
        }
        write(&format!("Spells/{name}.json"), families);
    }
}

/// A synthetic source holds none of the rows the 1999 world reviews, so these runs keep every row
/// as the source wrote it.
fn run(repo: &Path, sha: &str, out: &Path, accept: bool) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_empyrean-import"));
    cmd.args([
        "ac1999",
        "--target",
        "source",
        "--revision",
        sha,
        "--source",
    ])
    .arg(repo)
    .arg("--out")
    .arg(out);
    if accept {
        cmd.arg("--allow-incomplete");
    }
    cmd.output().unwrap()
}

fn success(out: Output) {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Divergence: V466
#[test]
fn unknown_loot_fields_and_missing_pool_templates_refuse_publication() {
    for unknown in [true, false] {
        let (root, repo, _) = fixture(if unknown {
            "loot-field"
        } else {
            "loot-reference"
        });
        let path = repo.join("Server/Bin/Data/Treasure/Pools/gems.json");
        let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        if unknown {
            value["entries"][0]["unreviewedField"] = true.into();
        } else {
            value["entries"][0]["wcid"] = 987654.into();
        }
        std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        git(&repo, &["add", "Server"]);
        git(
            &repo,
            &[
                "-c",
                "core.hooksPath=",
                "commit",
                "-m",
                "test: invalid loot",
            ],
        );
        let sha = git(&repo, &["rev-parse", "HEAD"]);
        let output = root.join("out");
        let result = run(&repo, &sha, &output, true);
        assert!(!result.status.success());
        assert!(!output.exists());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            error.contains(if unknown {
                "unknown field"
            } else {
                "missing template 987654"
            }),
            "{error}"
        );
    }
}

#[test]
fn committed_content_builds_identically_despite_checkout_edits_and_output_paths() {
    let (root, repo, sha) = fixture("reproducible");
    let a = root.join("a");
    let b = root.join("b");
    success(run(&repo, &sha, &a, true));
    std::fs::write(
        repo.join(DATABASE).join("3-Core/content.sql"),
        "invalid uncommitted SQL",
    )
    .unwrap();
    std::fs::write(repo.join(".git/info/attributes"), "* export-ignore\n").unwrap();
    git(&repo, &["add", "Server"]);
    git(
        &repo,
        &["-c", "core.hooksPath=", "commit", "-m", "test: replacement"],
    );
    let replacement = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["replace", &sha, &replacement]);
    success(run(&repo, &sha, &b, true));
    for name in [
        "world.sql",
        "empyrean.sql",
        "empyrean-changes.json",
        "world.pack",
        "manifest.json",
        "changes.json",
        "pack-report.json",
    ] {
        assert_eq!(
            std::fs::read(a.join(name)).unwrap(),
            std::fs::read(b.join(name)).unwrap(),
            "{name}"
        );
    }
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(a.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["source"]["revision"], sha);
    assert_eq!(manifest["empyrean_restored_treasure"], 1);
    assert_eq!(manifest["ready_for_gameplay"], false);
    let pack = PackContent::open(&a.join("world.pack")).unwrap();
    assert_eq!(pack.era(), EraId::parse("launch").unwrap());
    let changes: Value =
        serde_json::from_slice(&std::fs::read(a.join("changes.json")).unwrap()).unwrap();
    assert!(changes
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["table"] == "quest" && r["action"] == "normalize"));
    for table in [
        "treasure_death",
        "recipe",
        "recipe_mod",
        "recipe_mods_int",
        "cook_book",
        "weenie_properties_spell_book",
    ] {
        assert!(
            changes
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["table"] == table && r["action"] == "quarantine"),
            "{table}"
        );
    }
    let sql = std::fs::read_to_string(a.join("world.sql")).unwrap();
    assert!(!sql.contains("DROP DATABASE"));
    assert!(!sql.contains("USE `ace_world`"));
    assert!(sql.contains("SET NAMES utf8mb4"));
    assert!(
        !sql.as_bytes().contains(&0),
        "SQL strings use portable escapes"
    );
    let pack_report: Value =
        serde_json::from_slice(&std::fs::read(a.join("pack-report.json")).unwrap()).unwrap();
    assert_eq!(pack_report["dump_rows"]["recipe"], 0);
    assert_eq!(pack_report["dump_rows"]["quest"], 1);
    let rebuilt = root.join("rebuilt.pack");
    success(
        Command::new(env!("CARGO_BIN_EXE_empyrean-import"))
            .arg("--sql")
            .arg(a.join("world.sql"))
            .arg("--patches")
            .arg(a.join("empyrean.sql"))
            .args(["--era", "launch", "--out"])
            .arg(&rebuilt)
            .output()
            .unwrap(),
    );
    assert_eq!(
        std::fs::read(a.join("world.pack")).unwrap(),
        std::fs::read(rebuilt).unwrap()
    );
}

#[test]
/// Divergence: V463
fn supported_vital_costs_are_restored_only_in_the_empyrean_extension() {
    let (root, repo, _) = fixture("vital-costs");
    let content = repo.join(DATABASE).join("3-Core/content.sql");
    let mut rows = std::fs::read_to_string(&content)
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with("INSERT INTO `recipe_mod"))
        .collect::<Vec<_>>()
        .join("\n");
    rows.push_str("\nINSERT INTO recipe_mod (id,recipe_Id,`index`,requires,executes_On_Success,health,stamina,mana,unknown_7,data_Id,unknown_9,instance_Id) VALUES (3,5,3,5,1,-10,0,-20,0,0,0,0);\n");
    std::fs::write(content, rows).unwrap();
    git(&repo, &["add", "Server"]);
    git(
        &repo,
        &["-c", "core.hooksPath=", "commit", "-m", "test: vital costs"],
    );
    let sha = git(&repo, &["rev-parse", "HEAD"]);
    let out = root.join("out");
    success(run(&repo, &sha, &out, true));
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["empyrean_restored_recipes"],
        serde_json::json!([5])
    );
    let db = PackContent::open(&out.join("world.pack")).unwrap();
    let requirement = db.get_recipe_vital_requirement(5).unwrap();
    assert_eq!(
        (requirement.health, requirement.stamina, requirement.mana),
        (10, 0, 20)
    );
    let recipe = db.get_recipe(5).unwrap();
    assert_eq!(recipe.recipe_mod.len(), 1);
    assert_eq!(recipe.recipe_mod[0].health, -10);
    assert_eq!(recipe.recipe_mod[0].mana, -20);
    let canonical =
        empyrean_content::import::import(std::fs::File::open(out.join("world.sql")).unwrap())
            .unwrap()
            .0;
    let canonical = PackContent::new(empyrean_content::pack::Pack::from_bytes(canonical).unwrap());
    assert!(canonical.get_recipe(5).is_none());
    assert!(canonical.get_recipe_vital_requirement(5).is_none());
}

#[test]
fn incomplete_conversion_requires_acceptance_and_never_overwrites_an_output() {
    let (root, repo, sha) = fixture("refusals");
    let out = root.join("out");
    assert!(!run(&repo, &sha, &out, false).status.success());
    assert!(!out.exists());
    assert!(!run(&repo, "main", &out, true).status.success());
    assert!(!out.exists());
    std::fs::create_dir(&out).unwrap();
    std::fs::write(out.join("keep"), "untouched").unwrap();
    assert!(!run(&repo, &sha, &out, true).status.success());
    assert_eq!(
        std::fs::read_to_string(out.join("keep")).unwrap(),
        "untouched"
    );
}

#[test]
fn an_unknown_upstream_column_refuses_publication() {
    let (root, repo, _) = fixture("unknown-column");
    let path = repo.join(DATABASE).join("1-Base/WorldBase.sql");
    let schema = std::fs::read_to_string(&path).unwrap().replace(
        "CREATE TABLE `quest` (",
        "CREATE TABLE `quest` (`new_behavior` int DEFAULT 0,",
    );
    std::fs::write(&path, schema).unwrap();
    let row_path = repo.join(DATABASE).join("3-Core/content.sql");
    let rows=std::fs::read_to_string(&row_path).unwrap().replace("INSERT INTO `quest` VALUES", "INSERT INTO `quest` (`id`,`name`,`min_Delta`,`max_Solves`,`message`,`last_Modified`) VALUES");
    std::fs::write(&row_path, rows).unwrap();
    git(&repo, &["add", "Server"]);
    git(
        &repo,
        &[
            "-c",
            "core.hooksPath=",
            "commit",
            "-m",
            "test: unknown column",
        ],
    );
    let sha = git(&repo, &["rev-parse", "HEAD"]);
    let out = root.join("out");
    let result = run(&repo, &sha, &out, true);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("source schema differs from the reviewed definition"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!out.exists());
}

#[test]
fn case_colliding_committed_paths_refuse_publication() {
    let (root, repo, _) = fixture("case-collision");
    let oid = git(
        &repo,
        &["rev-parse", &format!("HEAD:{DATABASE}/3-Core/content.sql")],
    );
    git(
        &repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "100644",
            &oid,
            &format!("{DATABASE}/3-Core/CONTENT.sql"),
        ],
    );
    git(
        &repo,
        &[
            "-c",
            "core.hooksPath=",
            "commit",
            "-m",
            "test: colliding paths",
        ],
    );
    let sha = git(&repo, &["rev-parse", "HEAD"]);
    let out = root.join("out");
    let result = run(&repo, &sha, &out, true);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("case-colliding source paths"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!out.exists());
}

#[test]
fn changed_upstream_column_types_refuse_publication() {
    let (root, repo, _) = fixture("changed-type");
    let path = repo.join(DATABASE).join("1-Base/WorldBase.sql");
    let schema = std::fs::read_to_string(&path).unwrap();
    let changed = schema.replace("`min_Delta` int", "`min_Delta` bigint");
    assert_ne!(schema, changed);
    std::fs::write(path, changed).unwrap();
    git(&repo, &["add", "Server"]);
    git(
        &repo,
        &[
            "-c",
            "core.hooksPath=",
            "commit",
            "-m",
            "test: changed schema",
        ],
    );
    let sha = git(&repo, &["rev-parse", "HEAD"]);
    let out = root.join("out");
    let result = run(&repo, &sha, &out, true);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("source schema differs from the reviewed definition"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!out.exists());
}

#[test]
fn content_schema_mutations_refuse_publication() {
    let (root, repo, _) = fixture("content-ddl");
    std::fs::write(
        repo.join(DATABASE).join("3-Core/alter.sql"),
        "ALTER TABLE `quest` DROP INDEX `name_UNIQUE`;\n",
    )
    .unwrap();
    git(&repo, &["add", "Server"]);
    git(
        &repo,
        &[
            "-c",
            "core.hooksPath=",
            "commit",
            "-m",
            "test: altered constraints",
        ],
    );
    let sha = git(&repo, &["rev-parse", "HEAD"]);
    let out = root.join("out");
    let result = run(&repo, &sha, &out, true);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("unreviewed content DDL"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!out.exists());
}

#[test]
fn target_numeric_constraints_refuse_out_of_range_projection() {
    let (root, repo, _) = fixture("target-constraints");
    let path = repo.join(DATABASE).join("3-Core/content.sql");
    let mut rows = std::fs::read_to_string(&path).unwrap();
    rows.push_str("\nINSERT INTO `weenie_properties_emote` (`id`,`object_Id`,`category`) VALUES (999,100,1);\nINSERT INTO `weenie_properties_emote_action` (`emote_Id`,`order`,`type`,`motion`) VALUES (999,1,1,4294967295);\n");
    std::fs::write(path, rows).unwrap();
    git(&repo, &["add", "Server"]);
    git(
        &repo,
        &[
            "-c",
            "core.hooksPath=",
            "commit",
            "-m",
            "test: target range",
        ],
    );
    let sha = git(&repo, &["rev-parse", "HEAD"]);
    let out = root.join("out");
    let result = run(&repo, &sha, &out, true);
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("world.sql") && error.contains("range"),
        "{error}"
    );
    assert!(!out.exists());
}

/// Divergence: V462
/// A converted bundle, and the candidate's source ZIP, rebuild the same world without the
/// repository; a source file that no longer matches its listed hash is refused.
#[test]
fn a_source_bundle_rebuilds_the_same_world_and_a_changed_file_is_refused() {
    let (root, repo, sha) = fixture("bundle");
    let first = root.join("first");
    success(run(&repo, &sha, &first, true));
    let again = root.join("again");
    success(run(&first, &sha, &again, true));
    let candidate = root.join("candidate");
    success(
        Command::new(env!("CARGO_BIN_EXE_empyrean-import"))
            .args(["ac1999", "package", "--bundle"])
            .arg(&first)
            .arg("--out")
            .arg(&candidate)
            .output()
            .unwrap(),
    );
    let source_zip = std::fs::read_dir(&candidate)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.to_string_lossy().ends_with("-source.zip"))
        .expect("the source ZIP");
    let from_zip = root.join("from-zip");
    success(run(&source_zip, &sha, &from_zip, true));
    for name in [
        "world.sql",
        "empyrean.sql",
        "world.pack",
        "changes.json",
        "manifest.json",
    ] {
        let original = std::fs::read(first.join(name)).unwrap();
        assert_eq!(original, std::fs::read(again.join(name)).unwrap(), "{name}");
        assert_eq!(
            original,
            std::fs::read(from_zip.join(name)).unwrap(),
            "{name}"
        );
    }
    let sums = std::fs::read_to_string(candidate.join("SHA256SUMS")).unwrap();
    assert_eq!(sums.lines().count(), 3, "{sums}");
    let release: Value =
        serde_json::from_slice(&std::fs::read(candidate.join("release.json")).unwrap()).unwrap();
    assert_eq!(release["published"], false);

    std::fs::write(first.join("source/Server/LICENSE"), "changed\n").unwrap();
    let refused = run(&first, &sha, &root.join("refused"), true);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("do not match its hashes"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    std::fs::remove_dir_all(&root).ok();
}
