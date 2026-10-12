//! Behaviour: none (tooling: the network releases' preconditions, version edits, checks and
//! agreement with their release workflows).

use super::*;
use crate::release::Releasable;

fn clean(current: &str) -> RepoState {
    RepoState {
        branch: "main".to_owned(),
        uncommitted: Vec::new(),
        current: current.to_owned(),
        local_tags: vec!["network-hub-v0.0.9".to_owned()],
        remote_tags: Some(vec!["network-hub-v0.0.9".to_owned()]),
    }
}

/// A machine with `dotnet` and, when given, a whole ACE server folder.
fn with_dotnet(ace: Option<&str>) -> Machine {
    Machine {
        dotnet: true,
        ace_dir: ace.map(|a| (a.to_owned(), true)),
    }
}

const META: &str = r#"{
  "Name": "Dereth.Network",
  "Author": "Dereth Network",
  "Version": "0.1.0",
  "Enabled": true
}
"#;

const PROJECT: &str = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net10.0</TargetFramework>
    <AssemblyName>Dereth.Network</AssemblyName>
    <Version>0.1.0</Version>
  </PropertyGroup>
</Project>
"#;

const HUB: &str = r#"[package]
name = "dereth-network-hub"
version = "0.1.0" # the hub's own
edition = "2021"

[dependencies]
dereth-network-protocol = { path = "../protocol" }
serde = { version = "1", features = ["derive"] }
"#;

/// The command line names the two network products, beside Empyrean and Dereth.
#[test]
fn the_network_products_are_named_on_the_command_line() {
    assert_eq!(
        Releasable::from_command_name("network-hub"),
        Some(Releasable::Network(Part::Hub))
    );
    assert_eq!(
        Releasable::from_command_name("network-ace"),
        Some(Releasable::Network(Part::Ace))
    );
    assert_eq!(
        Releasable::from_command_name("empyrean"),
        Some(Releasable::Product(
            crate::package::version::Product::Empyrean
        ))
    );
    assert_eq!(Releasable::from_command_name("network"), None);
    let args: Vec<String> = ["network-ace", "0.2.0", "--remote", "forgejo", "--push"]
        .map(str::to_owned)
        .into();
    assert_eq!(
        crate::release::parse(&args),
        Ok((
            Releasable::Network(Part::Ace),
            "0.2.0".to_owned(),
            true,
            "forgejo".to_owned()
        ))
    );
}

/// A new hub version on a clean main is bumped in the hub's manifest and the lock file, and tagged
/// `network-hub-v<version>`; the hub needs neither `dotnet` nor ACE.
#[test]
fn a_new_hub_version_edits_its_manifest_and_lock_and_is_tagged() {
    assert_eq!(
        preconditions(Part::Hub, "0.2.0", &clean("0.1.0"), &Machine::default()),
        Ok(Plan {
            bump: true,
            tag: "network-hub-v0.2.0".to_owned(),
            files: &["network/hub/Cargo.toml", "Cargo.lock"],
            ace: None,
        })
    );
}

/// A new mod version edits both its version files and is tagged `network-ace-v<version>`,
/// checked against the ACE folder when one is given and without ACE otherwise.
#[test]
fn a_new_mod_version_edits_both_its_files_and_is_tagged() {
    let plan = preconditions(
        Part::Ace,
        "0.2.0",
        &clean("0.1.0"),
        &with_dotnet(Some("C:/ACE")),
    );
    assert_eq!(
        plan,
        Ok(Plan {
            bump: true,
            tag: "network-ace-v0.2.0".to_owned(),
            files: &[
                "network/ace-mod/Dereth.Network/Meta.json",
                "network/ace-mod/Dereth.Network/Dereth.Network.csproj"
            ],
            ace: Some("C:/ACE".to_owned()),
        })
    );
    let without = preconditions(Part::Ace, "0.2.0", &clean("0.1.0"), &with_dotnet(None));
    assert_eq!(without.map(|p| p.ace), Ok(None));
}

/// The version the tree carries, never tagged, is released without a bump.
#[test]
fn the_carried_version_never_tagged_is_released_without_a_bump() {
    let plan = preconditions(Part::Hub, "0.1.0", &clean("0.1.0"), &Machine::default());
    assert_eq!(
        plan.map(|p| (p.bump, p.tag)),
        Ok((false, "network-hub-v0.1.0".to_owned()))
    );
}

/// Another branch, uncommitted changes, an older version and a tag that exists here or on the
/// remote are each refused for either product, every reason named; a version that is not a
/// release version is refused.
#[test]
fn every_unmet_precondition_is_refused_and_named() {
    for part in [Part::Hub, Part::Ace] {
        let machine = with_dotnet(None);
        let tag = part.tag_for("0.2.0");

        let mut elsewhere = clean("0.1.0");
        elsewhere.branch = "agent/network".to_owned();
        elsewhere.uncommitted = vec![" M network/hub/src/main.rs".to_owned()];
        let e = preconditions(part, "0.2.0", &elsewhere, &machine).expect_err("two reasons");
        assert_eq!(e.len(), 2, "{e:?}");
        assert!(
            e[0].contains("`main`") && e[0].contains("agent/network"),
            "{e:?}"
        );
        assert!(e[1].contains("network/hub/src/main.rs"), "{e:?}");

        let e = preconditions(part, "0.0.9", &clean("0.1.0"), &machine).expect_err("older");
        assert!(e.iter().any(|p| p.contains("older")), "{e:?}");

        let mut tagged = clean("0.1.0");
        tagged.local_tags.push(tag.clone());
        let e = preconditions(part, "0.2.0", &tagged, &machine).expect_err("tagged here");
        assert!(
            e.iter().any(|p| p.contains(&tag) && p.contains("here")),
            "{e:?}"
        );

        let mut remote = clean("0.1.0");
        remote.remote_tags = Some(vec![tag.clone()]);
        let e = preconditions(part, "0.2.0", &remote, &machine).expect_err("tagged there");
        assert!(
            e.iter().any(|p| p.contains(&tag) && p.contains("remote")),
            "{e:?}"
        );

        for bad in ["0.2", "0.2.0+b", "v0.2.0", "02.0.0"] {
            assert!(
                preconditions(part, bad, &clean("0.1.0"), &machine).is_err(),
                "{bad}"
            );
        }
        assert!(
            preconditions(part, "0.2.0-rc.1", &clean("0.1.0"), &machine).is_ok(),
            "a release candidate is a release"
        );
    }
}

/// One product's tag does not hold back the other's, nor the same version of another product.
#[test]
fn each_product_is_held_back_only_by_its_own_tag() {
    let mut state = clean("0.1.0");
    state.local_tags = vec![
        "network-hub-v0.2.0".to_owned(),
        "empyrean-v0.2.0".to_owned(),
        "dereth-v0.2.0".to_owned(),
    ];
    assert!(preconditions(Part::Ace, "0.2.0", &state, &with_dotnet(None)).is_ok());
    assert!(preconditions(Part::Hub, "0.2.0", &state, &Machine::default()).is_err());
}

/// The mod is refused without `dotnet` on PATH, and with a `DERETH_ACE_DIR` that is not an ACE
/// server folder, each reason named beside the repository's; the hub needs neither.
#[test]
fn the_mod_is_refused_without_dotnet_or_with_a_wrong_ace_folder() {
    let mut elsewhere = clean("0.1.0");
    elsewhere.branch = "agent/network".to_owned();
    let machine = Machine {
        dotnet: false,
        ace_dir: Some(("C:/nowhere".to_owned(), false)),
    };
    let e = preconditions(Part::Ace, "0.2.0", &elsewhere, &machine).expect_err("three reasons");
    assert_eq!(e.len(), 3, "{e:?}");
    assert!(
        e.iter().any(|p| p.contains("`dotnet` is not on PATH")),
        "{e:?}"
    );
    assert!(
        e.iter()
            .any(|p| p.contains("DERETH_ACE_DIR") && p.contains("C:/nowhere")),
        "{e:?}"
    );

    let no_dotnet = Machine {
        dotnet: false,
        ace_dir: None,
    };
    let e = preconditions(Part::Ace, "0.2.0", &clean("0.1.0"), &no_dotnet).expect_err("dotnet");
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(preconditions(Part::Hub, "0.2.0", &clean("0.1.0"), &machine).is_ok());
}

/// The mod's version is read from `Meta.json` and its project as the release workflow reads it,
/// and the two must agree.
#[test]
fn the_mod_version_is_read_from_both_files_which_must_agree() {
    assert_eq!(ace_version(META, PROJECT), Ok("0.1.0".to_owned()));
    let newer = PROJECT.replace("<Version>0.1.0</Version>", "<Version>0.1.1</Version>");
    let e = ace_version(META, &newer).expect_err("they differ");
    assert!(e.contains("0.1.0") && e.contains("0.1.1"), "{e}");
    assert!(meta_version("{\n  \"Name\": \"Dereth.Network\"\n}\n").is_err());
    let twice = PROJECT.replace(
        "</PropertyGroup>",
        "  <Version>0.1.0</Version>\n  </PropertyGroup>",
    );
    assert!(
        project_version(&twice).is_err(),
        "two versions would be misread"
    );
}

/// The edit moves both of the mod's versions together and leaves every other byte of each file
/// alone; a file with no version leaves both unedited.
#[test]
fn the_mod_edit_moves_both_versions_together() {
    let (meta, project) = set_ace_version(META, PROJECT, "0.2.0-rc.1").expect("edited");
    assert_eq!(meta, META.replace("\"0.1.0\"", "\"0.2.0-rc.1\""));
    assert_eq!(
        project,
        PROJECT.replace("<Version>0.1.0<", "<Version>0.2.0-rc.1<")
    );
    assert_eq!(ace_version(&meta, &project), Ok("0.2.0-rc.1".to_owned()));
    serde_json::from_str::<serde_json::Value>(&meta).expect("Meta.json is still JSON");

    let unversioned = PROJECT.replace("    <Version>0.1.0</Version>\n", "");
    assert!(set_ace_version(META, &unversioned, "0.2.0").is_err());
}

/// The hub's edit moves its `[package]` version alone: not a dependency's, and not the comment
/// after it.
#[test]
fn the_hub_edit_moves_its_package_version_alone() {
    let edited = crate::package::version::set_package_version(HUB, "0.2.0").expect("edited");
    assert_eq!(
        edited,
        HUB.replace("version = \"0.1.0\"", "version = \"0.2.0\"")
    );
    assert!(edited.contains("serde = { version = \"1\""));
}

/// The version files are read from and edited at their places in the tree.
#[test]
fn the_version_files_are_read_and_edited_in_the_tree() {
    let ws = std::env::temp_dir().join(format!("xtask-network-release-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ws);
    for (file, text) in [
        (HUB_MANIFEST, HUB),
        (ACE_META, META),
        (ACE_PROJECT, PROJECT),
    ] {
        let path = ws.join(file);
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("scratch dir");
        std::fs::write(path, text).expect("write");
    }
    assert_eq!(Part::Hub.version(&ws), Ok("0.1.0".to_owned()));
    assert_eq!(Part::Ace.version(&ws), Ok("0.1.0".to_owned()));

    let hub = Part::Hub.edited(&ws, "0.3.0").expect("hub");
    assert_eq!(hub.len(), 1);
    assert_eq!(hub[0].0, HUB_MANIFEST);
    assert_eq!(
        version::package_version(&hub[0].1).as_deref(),
        Some("0.3.0")
    );

    let ace = Part::Ace.edited(&ws, "0.3.0").expect("mod");
    let files: Vec<&str> = ace.iter().map(|(f, _)| *f).collect();
    assert_eq!(files, [ACE_META, ACE_PROJECT]);
    assert_eq!(ace_version(&ace[0].1, &ace[1].1), Ok("0.3.0".to_owned()));
    let _ = std::fs::remove_dir_all(&ws);
}

fn lines(steps: &[Step]) -> Vec<String> {
    steps.iter().map(Step::line).collect()
}

/// The hub's checks are the network's crates' tests and the conformance vectors through the
/// suite's binary.
#[test]
fn the_hub_checks_are_its_crates_tests_and_the_vectors() {
    let steps = checks(Part::Hub, None, "T", true);
    assert_eq!(
        lines(&steps),
        [
            "cargo test --locked --profile test-release -p dereth-network-protocol \
             -p dereth-network-hub -p dereth-network-conformance",
            "cargo build --locked --profile test-release -p dereth-network-hub \
             -p dereth-network-conformance --bins",
            "T/dereth-network-conformance.exe vectors",
        ]
    );
    assert!(steps.iter().all(|s| s.dir.is_empty()));
    assert!(left_out(Part::Hub, None, false).is_empty());
}

/// With an ACE folder the mod is built against it and checked whole: the harness with its
/// targets section, and every patch applied with ACE's Harmony.
#[test]
fn with_ace_the_mod_is_built_and_checked_against_it() {
    let steps = checks(Part::Ace, Some("C:/ACE"), "T", true);
    let all = lines(&steps);
    assert_eq!(
        all[0],
        "cargo build --locked --profile test-release -p dereth-network-hub --bins"
    );
    assert!(all.contains(
        &"dotnet build Dereth.Network/Dereth.Network.csproj -c Release -p:AceDirectory=C:/ACE"
            .to_owned()
    ));
    assert!(all.contains(
        &"dotnet build tests/PatchApply/PatchApply.csproj -c Release -p:AceDirectory=C:/ACE"
            .to_owned()
    ));
    let harness = all
        .iter()
        .find(|l| l.contains("Harness.dll"))
        .expect("the harness runs");
    assert!(
        harness.ends_with(
            "--vectors ../conformance/vectors --hub T/dereth-network-hub.exe --ace C:/ACE \
             --mod ../../target/ace-mod/bin/Dereth.Network/release/Dereth.Network.dll"
        ),
        "{harness}"
    );
    assert!(all
        .last()
        .expect("steps")
        .contains("PatchApply.dll --ace C:/ACE"));
    assert!(steps[1..].iter().all(|s| s.dir == "network/ace-mod"));
    assert!(left_out(Part::Ace, Some("C:/ACE"), true).is_empty());
}

/// Without an ACE folder only the checks that need no ACE run, the harness told to leave out its
/// targets section, and the output says what was left out; off Windows the units section is left
/// out as CI leaves it out.
#[test]
fn without_ace_the_checks_that_need_it_are_left_out_and_said_so() {
    let all = lines(&checks(Part::Ace, None, "T", false));
    assert!(
        all.iter()
            .all(|l| !l.contains("AceDirectory") && !l.contains("PatchApply")),
        "{all:?}"
    );
    assert!(all.iter().all(|l| !l.contains("--ace")), "{all:?}");
    let harness = all
        .iter()
        .find(|l| l.contains("Harness.dll"))
        .expect("the harness runs");
    assert!(
        harness.ends_with("--hub T/dereth-network-hub --skip targets --skip units"),
        "{harness}"
    );
    let notes = left_out(Part::Ace, None, false);
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes[0].contains("DERETH_ACE_DIR is not set"), "{notes:?}");
    assert!(notes[1].contains("units"), "{notes:?}");
}

/// The release smoke test expects the hub to answer `--version` as its release workflow checks.
#[test]
fn the_hub_names_its_version_as_the_workflow_checks() {
    assert_eq!(hub_version_line("0.2.0"), "dereth-network-hub 0.2.0");
}

/// What the command checks before tagging is what the workflows check after: each workflow starts
/// on the product's tags, reads the version from the files the command edits, writes its notes
/// from the paths the command previews, and checks what the command expects; CI's network job
/// runs the hub's tests as the command does.
#[test]
fn the_command_agrees_with_the_release_workflows() {
    let ws = workspace_root();
    let read = |f: &str| std::fs::read_to_string(ws.join(f)).expect(f);
    for part in [Part::Hub, Part::Ace] {
        let workflow = read(part.workflow());
        assert!(
            workflow.contains(&format!("tags: ['{}*']", part.tag_prefix())),
            "{}",
            part.workflow()
        );
        for file in part.version_files().iter().filter(|f| **f != "Cargo.lock") {
            assert!(workflow.contains(file), "{} reads {file}", part.workflow());
        }
        let after_log = workflow
            .lines()
            .skip_while(|l| !l.contains("git log --no-merges"))
            .nth(1)
            .expect("the notes' git log");
        assert_eq!(
            after_log.trim(),
            part.notes_paths().join(" "),
            "{}",
            part.workflow()
        );
    }
    assert!(read(Part::Hub.workflow()).contains("grep -qx \"dereth-network-hub $VERSION\""));
    let workflow = read(Part::Ace.workflow());
    for file in MOD_RELEASE_FILES {
        assert!(workflow.contains(file), "the mod's archive holds {file}");
    }
    let ci = read(".github/workflows/ci.yml");
    for step in checks(Part::Hub, None, "target/test-release", false) {
        assert!(ci.contains(&step.line()), "{}", step.line());
    }
}
