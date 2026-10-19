//! Behaviour: none (tooling: the `lyceum` command's front end)
//! The command answers its usage, refuses what is not a command, and says the client layer the
//! client builds in is current.

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
        "lyceum build",
        "lyceum validate",
        "lyceum edit",
        "lyceum bake",
        "lyceum import",
        "lyceum layer",
    ] {
        assert!(text.contains(command), "{command}: {text}");
    }
    let wrong = lyceum(&["forge"]);
    assert_eq!(wrong.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("forge is not a command"));
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
