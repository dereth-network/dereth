//! The release versions: Empyrean's, the one `version` every `empyrean-*` crate carries;
//! Dereth's, the one the client, the launcher, the web client, the headless client and the plugin
//! author's tool share; and
//! Lyceum's, the one its editor, its command and the Foundry share; Asheron's Call 2's three:
//! Auberean's (the client and the headless client), Yalain's (every `yalain-*` crate) and
//! Athenaeum's (its editor, its command and its Foundry); the release tags that name them, and the
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

/// The crates that carry Dereth's version: the client, the web client, the same client in a browser, released under its own tag from the same version, the
/// headless client, which names itself by it, and the plugin author's tool, whose rules are those
/// of the client released with it.
pub const DERETH_MANIFESTS: &[&str] = &[
    "dereth/client/Cargo.toml",
    "dereth/web/Cargo.toml",
    "dereth/web/front/Cargo.toml",
    "dereth/headless/Cargo.toml",
    "tools/plugin/Cargo.toml",
];

/// The lock files a Dereth version change touches, as (the folder cargo runs in, the lock file),
/// relative to the workspace root.
pub const DERETH_LOCKS: &[(&str, &str)] = &[("", "Cargo.lock")];

/// The manifests of the crates that carry Dereth's version.
pub fn dereth_manifests(ws: &Path) -> Vec<PathBuf> {
    DERETH_MANIFESTS.iter().map(|m| ws.join(m)).collect()
}

/// Dereth's version, read from its crates' manifests and required to agree.
pub fn dereth_version(ws: &Path) -> Result<String, String> {
    manifests_version(
        ws,
        &dereth_manifests(ws),
        "Dereth crate (the client, the web client, the headless client and the plugin author's tool)",
    )
}

/// The prefix of a Lyceum release tag: `lyceum-v<version>`.
pub const LYCEUM_TAG_PREFIX: &str = "lyceum-v";

/// The crates that carry Lyceum's version: the editor and the `lyceum` command, released together,
/// and the Foundry, the library both are built on.
pub const LYCEUM_MANIFESTS: &[&str] = &[
    "lyceum/editor/Cargo.toml",
    "lyceum/cli/Cargo.toml",
    "lyceum/crates/foundry/Cargo.toml",
];

/// The manifests of the crates that carry Lyceum's version.
pub fn lyceum_manifests(ws: &Path) -> Vec<PathBuf> {
    LYCEUM_MANIFESTS.iter().map(|m| ws.join(m)).collect()
}

/// Lyceum's version, read from its crates' manifests and required to agree.
pub fn lyceum_version(ws: &Path) -> Result<String, String> {
    manifests_version(
        ws,
        &lyceum_manifests(ws),
        "Lyceum crate (the editor, the lyceum command and the Foundry)",
    )
}

/// The prefix of an Auberean release tag: `auberean-v<version>`.
pub const AUBEREAN_TAG_PREFIX: &str = "auberean-v";

/// The crates that carry Auberean's version: the client, the same client in a browser and the
/// headless client, released together, as Dereth's client and headless client are. The core crates
/// under `core/auberean-*` keep the workspace's.
pub const AUBEREAN_MANIFESTS: &[&str] = &[
    "auberean/client/Cargo.toml",
    "auberean/web/Cargo.toml",
    "auberean/headless/Cargo.toml",
];

/// The prefix of a Yalain release tag: `yalain-v<version>`.
pub const YALAIN_TAG_PREFIX: &str = "yalain-v";

/// The prefix of an Athenaeum release tag: `athenaeum-v<version>`.
pub const ATHENAEUM_TAG_PREFIX: &str = "athenaeum-v";

/// The crates that carry Athenaeum's version: the editor and the `athenaeum` command, released
/// together, and the Foundry, the library both are built on.
pub const ATHENAEUM_MANIFESTS: &[&str] = &[
    "athenaeum/editor/Cargo.toml",
    "athenaeum/cli/Cargo.toml",
    "athenaeum/crates/foundry/Cargo.toml",
];

/// The manifests of the workspace members under `yalain/`, in the root manifest's order: every
/// `yalain-*` crate and the `yalain` program.
pub fn yalain_manifests(ws: &Path) -> Result<Vec<PathBuf>, String> {
    let manifests = members_under(ws, "yalain/")?;
    if manifests.is_empty() {
        return Err("the workspace has no yalain/ members".to_owned());
    }
    Ok(manifests)
}

/// The manifests of the workspace members whose folder starts with `prefix`.
fn members_under(ws: &Path, prefix: &str) -> Result<Vec<PathBuf>, String> {
    let root = std::fs::read_to_string(ws.join("Cargo.toml"))
        .map_err(|e| format!("reading Cargo.toml: {e}"))?;
    let table: toml::Table =
        toml::from_str(&root).map_err(|e| format!("parsing Cargo.toml: {e}"))?;
    let members = table
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or("Cargo.toml has no [workspace] members")?;
    Ok(members
        .iter()
        .filter_map(|m| m.as_str())
        .filter(|m| m.starts_with(prefix))
        .map(|m| ws.join(m).join("Cargo.toml"))
        .collect())
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
    Lyceum,
    /// Asheron's Call 2's client.
    Auberean,
    /// Asheron's Call 2's server.
    Yalain,
    /// Asheron's Call 2's world builder.
    Athenaeum,
    /// The launcher, a product of its own: both games' launcher, released from `launcher-v*` tags.
    /// [`Self::ALL`] names the products the release commands cut together, which it isn't one of
    /// until the games' packages take a named launcher.
    Launcher,
}

/// The prefix of a launcher release tag: `launcher-v<version>`.
pub const LAUNCHER_TAG_PREFIX: &str = "launcher-v";

/// The manifest that carries the launcher's version.
pub const LAUNCHER_MANIFEST: &str = "launcher/Cargo.toml";

impl Product {
    /// Every product, in the order the commands list them.
    pub const ALL: [Self; 6] = [
        Self::Empyrean,
        Self::Dereth,
        Self::Lyceum,
        Self::Auberean,
        Self::Yalain,
        Self::Athenaeum,
    ];

    /// The product named on a command line: `empyrean`, `dereth`, `lyceum`, `auberean`, `yalain`
    /// or `athenaeum`.
    pub fn from_command_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .chain([Self::Launcher])
            .find(|p| p.command_name() == name)
    }

    /// The name the commands take.
    pub fn command_name(self) -> &'static str {
        match self {
            Self::Empyrean => "empyrean",
            Self::Dereth => "dereth",
            Self::Lyceum => "lyceum",
            Self::Auberean => "auberean",
            Self::Yalain => "yalain",
            Self::Athenaeum => "athenaeum",
            Self::Launcher => "launcher",
        }
    }

    /// The name a person reads.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Empyrean => "Empyrean",
            Self::Dereth => "Dereth",
            Self::Lyceum => "Lyceum",
            Self::Auberean => "Auberean",
            Self::Yalain => "Yalain",
            Self::Athenaeum => "Athenaeum",
            Self::Launcher => "Launcher",
        }
    }

    /// Whether the product is one of Asheron's Call 2's: Auberean, Yalain or Athenaeum.
    pub fn is_auberean_game(self) -> bool {
        matches!(self, Self::Auberean | Self::Yalain | Self::Athenaeum)
    }

    /// Whether its package holds the launcher, so that its release needs the launcher's first.
    pub fn holds_launcher(self) -> bool {
        matches!(self, Self::Dereth | Self::Auberean)
    }

    /// The prefix of the product's release tags.
    pub fn tag_prefix(self) -> &'static str {
        match self {
            Self::Empyrean => TAG_PREFIX,
            Self::Dereth => DERETH_TAG_PREFIX,
            Self::Lyceum => LYCEUM_TAG_PREFIX,
            Self::Auberean => AUBEREAN_TAG_PREFIX,
            Self::Yalain => YALAIN_TAG_PREFIX,
            Self::Athenaeum => ATHENAEUM_TAG_PREFIX,
            Self::Launcher => LAUNCHER_TAG_PREFIX,
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
                "the client, the launcher, the web client, the headless client and the plugin                  author's tool carry"
            }
            Self::Lyceum => "the editor, the lyceum command and the Foundry carry",
            Self::Auberean => "the client and the headless client carry",
            Self::Yalain => "the yalain-* crates carry",
            Self::Athenaeum => "the editor, the athenaeum command and the Foundry carry",
            Self::Launcher => "the launcher carries",
        }
    }

    /// The manifests that carry the version.
    pub fn manifests(self, ws: &Path) -> Result<Vec<PathBuf>, String> {
        let listed = |list: &[&str]| list.iter().map(|m| ws.join(m)).collect();
        match self {
            Self::Empyrean => empyrean_manifests(ws),
            Self::Dereth => Ok(dereth_manifests(ws)),
            Self::Lyceum => Ok(lyceum_manifests(ws)),
            Self::Auberean => Ok(listed(AUBEREAN_MANIFESTS)),
            Self::Yalain => yalain_manifests(ws),
            Self::Athenaeum => Ok(listed(ATHENAEUM_MANIFESTS)),
            Self::Launcher => Ok(listed(&[LAUNCHER_MANIFEST])),
        }
    }

    /// The product's version in the tree at `ws`.
    pub fn version(self, ws: &Path) -> Result<String, String> {
        match self {
            Self::Empyrean => empyrean_version(ws),
            Self::Dereth => dereth_version(ws),
            Self::Lyceum => lyceum_version(ws),
            Self::Auberean => manifests_version(
                ws,
                &self.manifests(ws)?,
                "Auberean crate (the client and the headless client)",
            ),
            Self::Yalain => manifests_version(ws, &self.manifests(ws)?, "yalain-* crate"),
            Self::Launcher => manifests_version(ws, &self.manifests(ws)?, "launcher crate"),
            Self::Athenaeum => manifests_version(
                ws,
                &self.manifests(ws)?,
                "Athenaeum crate (the editor, the athenaeum command and the Foundry)",
            ),
        }
    }
}

/// A pushed tag must name exactly the version the tree carries for `product`.
pub fn check_product_tag(product: Product, tag: &str, version: &str) -> Result<(), String> {
    let tag = tag.strip_prefix("refs/tags/").unwrap_or(tag);
    let prefix = product.tag_prefix();
    let Some(tagged) = tag.strip_prefix(prefix) else {
        let article = if matches!(product, Product::Empyrean | Product::Athenaeum) {
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
