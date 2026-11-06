//! `cargo xtask tier-targets`: a test target named for a tier that needs something a plain checkout
//! does not have (`dat`, `corpus`, `gpu`, `live`) declares the features that switch it on.
//!
//! Without `required-features`, `cargo test --workspace` builds and runs the target on every
//! machine. On one with the data its tests pass, so nothing says so until a machine without the
//! data panics in them: public CI, on a release's way out. Reading the manifests finds that in a
//! second, on any machine.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::util::workspace_root;

/// The tiers that need something a plain checkout does not have.
pub const GUARDED_TIERS: &[&str] = &["dat", "corpus", "gpu", "live"];

/// One line for each test target in `metadata` (the workspace's `cargo metadata --no-deps`) that is
/// named for a guarded tier and declares no required features.
pub fn findings(metadata: &Value) -> Vec<String> {
    let mut found = Vec::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        let name = package["name"].as_str().unwrap_or("<unnamed>");
        for target in package["targets"].as_array().into_iter().flatten() {
            let is_test = target["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|k| k == "test"));
            let tier = target["name"].as_str().unwrap_or_default();
            if !is_test || !GUARDED_TIERS.contains(&tier) {
                continue;
            }
            let guarded = target["required-features"]
                .as_array()
                .is_some_and(|features| !features.is_empty());
            if !guarded {
                found.push(format!(
                    "{name}: the `{tier}` test target has no required-features, so a workspace \
                     test run builds and runs it on machines that lack what it needs"
                ));
            }
        }
    }
    found
}

/// The workspace's manifests as cargo reads them.
fn metadata(ws: &Path) -> Result<Value, String> {
    let out = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))
}

/// `cargo xtask tier-targets`: prints each finding; 0 when there is none.
pub fn tier_targets() -> i32 {
    let metadata = match metadata(&workspace_root()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("tier-targets: {e}");
            return 1;
        }
    };
    let found = findings(&metadata);
    for line in &found {
        println!("  {line}");
    }
    println!(
        "tier-targets: {} target(s) without required features",
        found.len()
    );
    i32::from(!found.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn package(name: &str, targets: Value) -> Value {
        json!({ "packages": [{ "name": name, "targets": targets }] })
    }

    /// **A tier target with no required features is found**, by crate and tier, whichever tier.
    #[test]
    fn a_tier_target_without_required_features_is_found() {
        for tier in GUARDED_TIERS {
            let found = findings(&package(
                "some-crate",
                json!([{ "name": tier, "kind": ["test"], "required-features": [] }]),
            ));
            assert_eq!(found.len(), 1, "{tier}: {found:?}");
            assert!(found[0].starts_with(&format!("some-crate: the `{tier}` test target")));
            let absent = findings(&package(
                "some-crate",
                json!([{ "name": tier, "kind": ["test"] }]),
            ));
            assert_eq!(absent.len(), 1, "a target with no such key at all: {tier}");
        }
    }

    /// **What is guarded, and what needs no guard, is not found**: a target with its features, a
    /// `cpu` target, and a library or binary that shares a tier's name.
    #[test]
    fn guarded_targets_and_other_targets_are_not_found() {
        let found = findings(&package(
            "some-crate",
            json!([
                { "name": "dat", "kind": ["test"], "required-features": ["retail-dats"] },
                { "name": "cpu", "kind": ["test"], "required-features": [] },
                { "name": "gpu", "kind": ["lib"], "required-features": [] },
                { "name": "live", "kind": ["bin"], "required-features": [] },
            ]),
        ));
        assert_eq!(found, Vec::<String>::new());
    }

    /// **This workspace holds to the rule**: every tier target declares its features.
    #[test]
    fn every_tier_target_of_this_workspace_declares_its_features() {
        let metadata = metadata(&workspace_root()).expect("cargo metadata");
        assert_eq!(findings(&metadata), Vec::<String>::new());
    }
}
