//! Behaviour: none (tooling: the `lyceum` command's front end)
//! The command answers its usage, refuses what is not a command, says the client layer the
//! client builds in is current, and counts what a world changes.

use std::path::Path;
use std::process::Command;

fn lyceum(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lyceum"))
        .args(args)
        .output()
        .expect("the lyceum command runs")
}

#[test]
fn the_command_answers_its_usage_and_refuses_what_is_not_a_command() {
    let help = lyceum(&["--help"]);
    assert!(help.status.success());
    let text = String::from_utf8_lossy(&help.stdout);
    for command in [
        "lyceum new",
        "lyceum build",
        "lyceum validate",
        "lyceum edit",
        "lyceum bake",
        "lyceum export",
        "lyceum import",
        "lyceum facts",
        "lyceum layer",
        "lyceum fit",
    ] {
        assert!(text.contains(command), "{command}: {text}");
    }
    let wrong = lyceum(&["forge"]);
    assert_eq!(wrong.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("forge is not a command"));
}

#[test]
fn export_refuses_an_unknown_server_or_era_before_opening_files() {
    let missing = [
        "export",
        "missing/world.toml",
        "--dats",
        "missing",
        "--out",
        "missing-out",
    ];
    let server = lyceum(&[&missing[..], &["--for", "mysql"]].concat());
    assert_eq!(server.status.code(), Some(2));
    let said = String::from_utf8_lossy(&server.stderr);
    assert!(said.contains("--for is empyrean, ace or gdle"), "{said}");
    let era = lyceum(&[&missing[..], &["--for", "ace", "--era", "tod"]].concat());
    assert_eq!(era.status.code(), Some(2));
    let said = String::from_utf8_lossy(&era.stderr);
    assert!(said.contains("--era is eor or infiltration"), "{said}");
    let quests = lyceum(&[&missing[..], &["--for", "ace", "--gdle-quests", "q.json"]].concat());
    assert_eq!(quests.status.code(), Some(2));
    let said = String::from_utf8_lossy(&quests.stderr);
    assert!(said.contains("--gdle-quests is for --for gdle"), "{said}");
    assert!(!Path::new("missing-out").exists());
}

#[test]
fn build_refuses_a_revision_with_check_before_opening_files() {
    let out = lyceum(&[
        "build",
        "missing/world.toml",
        "--dats",
        "missing",
        "--after",
        "missing/overlay",
        "--check",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("--after writes a revision"), "{said}");
    let help = String::from_utf8_lossy(&lyceum(&["--help"]).stdout).into_owned();
    assert!(help.contains("[--after <overlay>]"), "{help}");
}

#[test]
fn the_client_layer_the_client_builds_in_is_what_its_manifest_makes() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../core/client-runtime/assets/classic-portal/pack.tsv");
    let check = lyceum(&["layer", manifest.to_str().unwrap(), "--check"]);
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(String::from_utf8_lossy(&check.stdout).contains("the container is current"));
}

#[test]
fn the_command_names_its_version_first() {
    let out = lyceum(&["--version"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        text.lines().next(),
        Some(format!("lyceum {}", env!("CARGO_PKG_VERSION")).as_str())
    );
    assert!(text.lines().any(|l| l.starts_with("target: ")), "{text}");
}

#[test]
fn fit_refuses_invalid_model_ids_and_sampling_before_opening_files() {
    for args in [
        vec!["fit", "--dats", "absent", "--model", "not-an-id"],
        vec![
            "fit",
            "--dats",
            "absent",
            "--model",
            "02000001",
            "--against",
            "03000001",
        ],
        vec![
            "fit", "--dats", "absent", "--model", "02000001", "--grid", "0",
        ],
        vec![
            "fit",
            "--dats",
            "absent",
            "--model",
            "02000001",
            "--directions",
            "129",
        ],
    ] {
        let out = lyceum(&args);
        assert_eq!(out.status.code(), Some(2));
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(error.contains("lyceum: fit"), "{error}");
        assert!(!error.contains("No such file"), "{error}");
    }
}

#[test]
fn edit_refuses_a_drop_it_cannot_read_before_opening_files() {
    for (args, says) in [
        (
            vec![
                "edit",
                "w.toml",
                "--dats",
                "absent",
                "add-building",
                "01002228",
                "A9B4",
                "1",
            ],
            "add-building takes <model> <landblock> <x> <y> <heading>",
        ),
        (
            vec![
                "edit",
                "w.toml",
                "--dats",
                "absent",
                "--bare",
                "--furnished",
                "3DC7/1",
                "add-building",
                "01002228",
                "A9B4",
                "1",
                "2",
                "3",
            ],
            "not both",
        ),
        (
            vec!["edit", "w.toml", "--dats", "absent", "carve", "1"],
            "carve is not an edit",
        ),
        (
            vec!["edit", "w.toml", "--dats", "absent"],
            "--edits, or one edit",
        ),
    ] {
        let out = lyceum(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(error.contains(says), "{args:?}: {error}");
        assert!(!error.contains("absent"), "{error}");
    }
}

#[test]
fn facts_prints_what_the_sample_world_changes_as_one_line_of_json() {
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sample/world.toml");
    let out = lyceum(&["facts", sample.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "{\"key\":\"sample-world\",\"era\":\"eor\",\"packs\":1,\"terrain\":1,\"statics\":6,\"buildings\":0,\"dungeons\":0,\"rooms\":0,\"spawns\":1,\"weenies\":1,\"textures\":1,\"models\":0}\n"
    );
}

#[test]
fn facts_refuses_a_missing_world_and_a_wrong_command_line() {
    let missing = lyceum(&["facts", "missing/world.toml"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).starts_with("lyceum: "));
    assert!(missing.stdout.is_empty());
    for args in [
        vec!["facts"],
        vec!["facts", "a/world.toml", "b/world.toml"],
        vec!["facts", "a/world.toml", "--dats", "absent"],
    ] {
        let out = lyceum(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            error.contains("facts takes a world file"),
            "{args:?}: {error}"
        );
    }
}
