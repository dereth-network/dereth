//! The release versions: Empyrean's, the one `version` every `empyrean-*` crate carries, and
//! Dereth's, the one the client, the launcher, the web client and the headless client share; the release tags that name them, and the
//! edit that moves them.

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
#[cfg(test)]
pub fn shared_version(versions: &[(String, Option<String>)]) -> Result<String, String> {
    shared_version_of("empyrean-* crate", versions)
}

/// The one version every manifest in `versions` carries, with `what` naming the manifests in the
/// error.
pub fn shared_version_of(
    what: &str,
    versions: &[(String, Option<String>)],
) -> Result<String, String> {
    let first = versions
        .first()
        .ok_or_else(|| format!("no {what} manifests were read"))?;
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
            "every {what} must carry the same version ({want} in {}); these differ: {}",
            first.0,
            differing.join(", ")
        ))
    }
}

/// Empyrean's version, read from every `empyrean-*` manifest and required to agree.
pub fn empyrean_version(ws: &Path) -> Result<String, String> {
    manifests_version(ws, &empyrean_manifests(ws)?, "empyrean-* crate")
}

/// The version every manifest in `manifests` carries, required to agree.
fn manifests_version(ws: &Path, manifests: &[PathBuf], what: &str) -> Result<String, String> {
    let mut versions = Vec::new();
    for manifest in manifests {
        let text = std::fs::read_to_string(manifest)
            .map_err(|e| format!("reading {}: {e}", manifest.display()))?;
        let name = manifest
            .strip_prefix(ws)
            .unwrap_or(manifest)
            .display()
            .to_string()
            .replace('\\', "/");
        versions.push((name, package_version(&text)));
    }
    shared_version_of(what, &versions)
}

/// The prefix of a Dereth release tag: `dereth-v<version>`.
pub const DERETH_TAG_PREFIX: &str = "dereth-v";

/// The crates that carry Dereth's version: the client and the launcher, released together, the
/// web client, the same client in a browser, released under its own tag from the same version, and
/// the headless client, which names itself by it.
pub const DERETH_MANIFESTS: &[&str] = &[
    "dereth/client/Cargo.toml",
    "dereth/launcher/Cargo.toml",
    "dereth/web/Cargo.toml",
    "dereth/headless/Cargo.toml",
];

/// The lock files a Dereth version change touches: the workspace's and the launcher app's own,
/// as (the folder cargo runs in, the lock file), relative to the workspace root.
pub const DERETH_LOCKS: &[(&str, &str)] = &[
    ("", "Cargo.lock"),
    ("dereth/launcher", "dereth/launcher/Cargo.lock"),
];

/// The manifests of the crates that carry Dereth's version.
pub fn dereth_manifests(ws: &Path) -> Vec<PathBuf> {
    DERETH_MANIFESTS.iter().map(|m| ws.join(m)).collect()
}

/// Dereth's version, read from the client's and the launcher's manifests and required to agree.
pub fn dereth_version(ws: &Path) -> Result<String, String> {
    manifests_version(
        ws,
        &dereth_manifests(ws),
        "Dereth crate (the client, the launcher, the web client and the headless client)",
    )
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
#[cfg(test)]
pub fn check_tag(tag: &str, version: &str) -> Result<(), String> {
    check_product_tag(Product::Empyrean, tag, version)
}

/// A product with releases of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    Empyrean,
    Dereth,
}

impl Product {
    /// The product named on a command line: `empyrean` or `dereth`.
    pub fn from_command_name(name: &str) -> Option<Self> {
        match name {
            "empyrean" => Some(Self::Empyrean),
            "dereth" => Some(Self::Dereth),
            _ => None,
        }
    }

    /// The name the commands take.
    pub fn command_name(self) -> &'static str {
        match self {
            Self::Empyrean => "empyrean",
            Self::Dereth => "dereth",
        }
    }

    /// The name a person reads.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Empyrean => "Empyrean",
            Self::Dereth => "Dereth",
        }
    }

    /// The prefix of the product's release tags.
    pub fn tag_prefix(self) -> &'static str {
        match self {
            Self::Empyrean => TAG_PREFIX,
            Self::Dereth => DERETH_TAG_PREFIX,
        }
    }

    /// The product's release tag for `version`.
    pub fn tag_for(self, version: &str) -> String {
        format!("{}{version}", self.tag_prefix())
    }

    /// The crates that carry the version, in words.
    fn carriers(self) -> &'static str {
        match self {
            Self::Empyrean => "the empyrean-* crates carry",
            Self::Dereth => {
                "the client, the launcher, the web client and the headless client carry"
            }
        }
    }

    /// The manifests that carry the version.
    pub fn manifests(self, ws: &Path) -> Result<Vec<PathBuf>, String> {
        match self {
            Self::Empyrean => empyrean_manifests(ws),
            Self::Dereth => Ok(dereth_manifests(ws)),
        }
    }

    /// The product's version in the tree at `ws`.
    pub fn version(self, ws: &Path) -> Result<String, String> {
        match self {
            Self::Empyrean => empyrean_version(ws),
            Self::Dereth => dereth_version(ws),
        }
    }
}

/// A pushed tag must name exactly the version the tree carries for `product`.
pub fn check_product_tag(product: Product, tag: &str, version: &str) -> Result<(), String> {
    let tag = tag.strip_prefix("refs/tags/").unwrap_or(tag);
    let prefix = product.tag_prefix();
    let Some(tagged) = tag.strip_prefix(prefix) else {
        let article = if product == Product::Empyrean {
            "an"
        } else {
            "a"
        };
        return Err(format!(
            "tag `{tag}` is not {article} {} release tag ({prefix}<version>)",
            product.display_name()
        ));
    };
    parse_release_version(tagged)?;
    if tagged == version {
        Ok(())
    } else {
        Err(format!(
            "tag `{tag}` names {tagged}, but {} {version}: tag the commit that carries the \
             version (`cargo xtask release {} {tagged}` makes both)",
            product.carriers(),
            product.command_name()
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
