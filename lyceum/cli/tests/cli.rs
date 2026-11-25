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
    assert!(
        said.contains("--era is launch, infiltration or eor"),
        "{said}"
    );
    let launch = lyceum(&[&missing[..], &["--for", "ace", "--era", "launch"]].concat());
    assert_eq!(launch.status.code(), Some(2));
    let said = String::from_utf8_lossy(&launch.stderr);
    assert!(
        said.contains("Launch world export is not supported yet"),
        "{said}"
    );
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

/// `pull-dungeon --cells-only` takes no value and reads no server database: its command line is
/// whole, and it goes on to its schema (missing here); an option it doesn't know is refused.
#[test]
fn pull_dungeon_takes_cells_only_as_a_switch_and_reads_no_database_for_it() {
    let out = lyceum(&[
        "pull-dungeon",
        "a/world.toml",
        "018F",
        "--into",
        "b/world.toml",
        "--dats",
        "absent",
        "--schema",
        "missing-schema.json",
        "--cells-only",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("missing-schema.json"), "{error}");
    assert!(!error.contains("takes a value"), "{error}");
    assert!(!error.contains("server database"), "{error}");
    let other = lyceum(&[
        "pull-dungeon",
        "a/world.toml",
        "018F",
        "--into",
        "b/world.toml",
        "--dats",
        "absent",
        "--schema",
        "missing-schema.json",
        "--cells-alone",
    ]);
    assert_eq!(other.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&other.stderr).contains("--cells-alone takes a value"));
}

/// `name` draws people's names from a world's name table of a heritage: the same seed gives the
/// same names, `--count` draws more after the first, and a heritage without a table is refused
/// with the ones there are.
#[test]
fn name_draws_the_same_names_for_a_seed_from_the_worlds_table() {
    let dir = std::env::temp_dir().join(format!("lyceum-cli-name-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("people/name_table")).unwrap();
    std::fs::write(
        dir.join("world.toml"),
        "key = \"names\"\npacks = [\"people\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("people/pack.toml"),
        "name = \"people\"\nversion = \"1\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("people/name_table/Sho.toml"),
        "[[name_table]]\nheritage = \"Sho\"\n[[name_table.slot]]\nparts = { Ko = 2, Ma = 1 }\n[[name_table.slot]]\nparts = { ri = 1, sen = 1 }\n",
    )
    .unwrap();
    let world = dir.join("world.toml");
    let world = world.to_str().unwrap();
    let args = [
        "name",
        world,
        "--heritage",
        "sho",
        "--seed",
        "2012392",
        "--count",
        "3",
    ];
    let once = lyceum(&args);
    assert!(
        once.status.success(),
        "{}",
        String::from_utf8_lossy(&once.stderr)
    );
    let names = String::from_utf8_lossy(&once.stdout).into_owned();
    assert_eq!(names.lines().count(), 3, "{names}");
    for n in names.lines() {
        assert!(
            ["Kori", "Kosen", "Mari", "Masen"].contains(&n),
            "{n} is drawn from the table"
        );
    }
    assert_eq!(String::from_utf8_lossy(&lyceum(&args).stdout), names);
    let none = lyceum(&["name", world, "--heritage", "Lugian", "--seed", "x"]);
    assert!(!none.status.success());
    let why = String::from_utf8_lossy(&none.stderr);
    assert!(
        why.contains("no name table of Lugian") && why.contains("Sho"),
        "{why}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Editing a world repository takes the server's schema of rows, its spawns being the layer's
/// placement rows: without one, and with no import tool where `--empyrean` says, it is refused
/// before the world's files are opened.
#[test]
fn edit_of_a_world_repository_without_the_servers_schema_is_refused() {
    let dir = std::env::temp_dir().join(format!("lyceum-cli-edit-repo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("world.toml"),
        "format_version = 1\ngame = \"ac1\"\nkey = \"r\"\npacks = []\nlineage = []\n",
    )
    .unwrap();
    std::fs::write(dir.join("edits.toml"), "").unwrap();
    let world = dir.join("world.toml");
    let edits = dir.join("edits.toml");
    let nowhere = dir.join("no-server");
    let out = lyceum(&[
        "edit",
        world.to_str().unwrap(),
        "--dats",
        "absent",
        "--edits",
        edits.to_str().unwrap(),
        "--empyrean",
        nowhere.to_str().unwrap(),
    ]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out.status.code(), Some(2));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("the server's schema of rows"), "{error}");
    assert!(!error.contains("absent"), "{error}");
}
