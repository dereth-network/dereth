//! Empyrean's version: the one `version` every `empyrean-*` crate carries, the release tag that
//! names it, and the edit that moves it.

use std::path::{Path, PathBuf};

/// The prefix of an Empyrean release tag: `empyrean-v<version>`.
pub const TAG_PREFIX: &str = "empyrean-v";

/// The manifests of the workspace members under `empyrean/`, in the root manifest's order.
pub fn empyrean_manifests(ws: &Path) -> Result<Vec<PathBuf>, String> {
    let root = std::fs::read_to_string(ws.join("Cargo.toml"))
        .map_err(|e| format!("reading Cargo.toml: {e}"))?;
    let table: toml::Table =
        toml::from_str(&root).map_err(|e| format!("parsing Cargo.toml: {e}"))?;
    let members = table
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or("Cargo.toml has no [workspace] members")?;
    let manifests: Vec<PathBuf> = members
        .iter()
        .filter_map(|m| m.as_str())
        .filter(|m| m.starts_with("empyrean/"))
        .map(|m| ws.join(m).join("Cargo.toml"))
        .collect();
    if manifests.is_empty() {
        return Err("the workspace has no empyrean/ members".to_owned());
    }
    Ok(manifests)
}

/// The `[package] version` of one manifest, when it is a literal (not `version.workspace`).
pub fn package_version(manifest: &str) -> Option<String> {
    let table: toml::Table = toml::from_str(manifest).ok()?;
    table
        .get("package")?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

/// The version every manifest shares. Each `(name, version)` pair is a manifest's path and its
/// literal version; a missing or differing one is an error naming it.
pub fn shared_version(versions: &[(String, Option<String>)]) -> Result<String, String> {
    let first = versions.first().ok_or("no empyrean manifests were read")?;
    let Some(want) = &first.1 else {
        return Err(format!("{} has no literal [package] version", first.0));
    };
    let differing: Vec<String> = versions
        .iter()
        .filter(|(_, v)| v.as_ref() != Some(want))
        .map(|(name, v)| format!("{name}: {}", v.as_deref().unwrap_or("(none)")))
        .collect();
    if differing.is_empty() {
        Ok(want.clone())
    } else {
        Err(format!(
            "every empyrean-* crate must carry the same version ({want} in {}); these differ: {}",
            first.0,
            differing.join(", ")
        ))
    }
}

/// Empyrean's version, read from every `empyrean-*` manifest and required to agree.
pub fn empyrean_version(ws: &Path) -> Result<String, String> {
    let mut versions = Vec::new();
    for manifest in empyrean_manifests(ws)? {
        let text = std::fs::read_to_string(&manifest)
            .map_err(|e| format!("reading {}: {e}", manifest.display()))?;
        let name = manifest
            .strip_prefix(ws)
            .unwrap_or(&manifest)
            .display()
            .to_string()
            .replace('\\', "/");
        versions.push((name, package_version(&text)));
    }
    shared_version(&versions)
}

/// A release version: `MAJOR.MINOR.PATCH`, optionally followed by a pre-release suffix
/// (`-rc.1`: dot-separated identifiers of letters, digits and hyphens). No build suffix (`+...`).
/// Returns the numeric core; the server reports only that core as its version numbers.
pub fn parse_release_version(v: &str) -> Result<[u64; 3], String> {
    let bad = || {
        format!(
            "`{v}` is not a release version: MAJOR.MINOR.PATCH, optionally with a pre-release suffix \
             such as `-rc.1` (no build suffix)"
        )
    };
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    if let Some(pre) = pre {
        let ok = !pre.is_empty()
            && pre.split('.').all(|id| {
                !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            });
        if !ok {
            return Err(bad());
        }
    }
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(bad());
    }
    let mut out = [0u64; 3];
    for (slot, part) in out.iter_mut().zip(&parts) {
        if part.is_empty()
            || !part.bytes().all(|b| b.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return Err(bad());
        }
        *slot = part.parse().map_err(|_| bad())?;
    }
    Ok(out)
}

/// Whether a release version is a pre-release (it carries a `-...` suffix).
pub fn is_prerelease(v: &str) -> bool {
    v.contains('-')
}

/// The tag of a release: `empyrean-v<version>`.
pub fn tag_for(version: &str) -> String {
    format!("{TAG_PREFIX}{version}")
}

/// A pushed tag must name exactly the version the tree carries.
pub fn check_tag(tag: &str, version: &str) -> Result<(), String> {
    let tag = tag.strip_prefix("refs/tags/").unwrap_or(tag);
    let Some(tagged) = tag.strip_prefix(TAG_PREFIX) else {
        return Err(format!(
            "tag `{tag}` is not an Empyrean release tag ({TAG_PREFIX}<version>)"
        ));
    };
    parse_release_version(tagged)?;
    if tagged == version {
        Ok(())
    } else {
        Err(format!(
            "tag `{tag}` names {tagged}, but the empyrean-* crates carry {version}: tag the commit \
             that carries the version (`cargo xtask release empyrean {tagged}` makes both)"
        ))
    }
}

/// `manifest` with its `[package]` version line set to `new`, the rest of the line (a trailing
/// comment) kept. An error when the section has no literal version line.
pub fn set_package_version(manifest: &str, new: &str) -> Result<String, String> {
    let mut in_package = false;
    let mut done = false;
    let mut out = String::with_capacity(manifest.len() + 8);
    for line in manifest.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_package = trimmed.starts_with("[package]");
        }
        if in_package && !done && trimmed.starts_with("version") {
            let rest = trimmed["version".len()..].trim_start();
            if let Some(value) = rest.strip_prefix('=').map(str::trim_start) {
                if let Some(body) = value.strip_prefix('"') {
                    if let Some(end) = body.find('"') {
                        let indent = &line[..line.len() - trimmed.len()];
                        out.push_str(&format!("{indent}version = \"{new}\"{}", &body[end + 1..]));
                        done = true;
                        continue;
                    }
                }
            }
        }
        out.push_str(line);
    }
    if done {
        Ok(out)
    } else {
        Err("no literal `version = \"...\"` line in [package]".to_owned())
    }
}
