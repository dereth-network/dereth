//! What a preflight builds: the products a branch changed since `main`, the release each one
//! takes next and why, and the release-candidate stamp each build carries.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// A product the preflight builds and `ship` releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Product {
    /// The Dereth Network's hub.
    Hub,
    /// Dereth.Network, the ACE mod.
    Ace,
    /// Empyrean, the server.
    Empyrean,
    /// Dereth: the client and the launcher.
    Dereth,
}

impl Product {
    /// Every product, in the order the preflight builds them and `ship` releases them.
    pub const ALL: [Self; 4] = [Self::Hub, Self::Ace, Self::Empyrean, Self::Dereth];

    /// Its name on the command line of `cargo xtask release`.
    pub const fn command_name(self) -> &'static str {
        match self {
            Self::Hub => "network-hub",
            Self::Ace => "network-ace",
            Self::Empyrean => "empyrean",
            Self::Dereth => "dereth",
        }
    }

    /// The workspace package whose dependencies make the product, if it is built by cargo.
    pub const fn package(self) -> Option<&'static str> {
        match self {
            Self::Hub => Some("dereth-network-hub"),
            Self::Ace => None,
            Self::Empyrean => Some("empyrean-server"),
            Self::Dereth => Some("dereth-client"),
        }
    }
}

/// How far a release moves the version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bump {
    Patch,
    Minor,
}

/// The web client's folder: its crate, its page and its scripts.
const WEB: &str = "dereth/web/";

/// Whether the changed file `path` is the web client's.
pub fn is_web(path: &str) -> bool {
    path.replace('\\', "/").starts_with(WEB)
}

/// The products the files `changed` (paths from the repository's root) belong to. A file belongs
/// to the workspace package whose folder holds it (`packages`, name and folder) and so to every
/// product built from that package (`depends`: each product's packages, its own included); the
/// ACE mod's folder is the mod's, a change to the launcher's own app or to a lock file belongs to
/// what builds with it, the web client is released as Dereth (it carries Dereth's version), and
/// nothing else (the docs, the research notes) is a product.
pub fn products_of(
    changed: &[String],
    packages: &[(String, String)],
    depends: &BTreeMap<Product, BTreeSet<String>>,
) -> BTreeSet<Product> {
    let mut out = BTreeSet::new();
    for path in changed {
        let path = path.replace('\\', "/");
        if path.starts_with("network/ace-mod/") {
            out.insert(Product::Ace);
            continue;
        }
        if (path.starts_with("dereth/launcher/") && !path.starts_with("dereth/launcher/crates/"))
            || path.starts_with(WEB)
        {
            out.insert(Product::Dereth);
            continue;
        }
        if path == "Cargo.lock" {
            out.extend(Product::ALL.iter().filter(|p| p.package().is_some()));
            continue;
        }
        let owner = packages
            .iter()
            .filter(|(_, dir)| {
                let dir = dir.trim_end_matches('/');
                path.starts_with(&format!("{dir}/"))
            })
            .max_by_key(|(_, dir)| dir.len());
        if let Some((name, _)) = owner {
            for (product, packages) in depends {
                if packages.contains(name) {
                    out.insert(*product);
                }
            }
        }
    }
    out
}

/// The version after `current`, moved by `bump`: `0.3.1` is `0.3.2` or `0.4.0`.
pub fn next_version(current: &str, bump: Bump) -> Result<String, String> {
    let core = current.split('-').next().unwrap_or(current);
    let [major, minor, patch] = crate::package::version::parse_release_version(core)?;
    Ok(match bump {
        // A tree that carries a pre-release has not had the release itself yet.
        Bump::Patch if current.contains('-') => core.to_owned(),
        Bump::Patch => format!("{major}.{minor}.{}", patch + 1),
        Bump::Minor if current.contains('-') && patch == 0 => core.to_owned(),
        Bump::Minor => format!("{major}.{}.0", minor + 1),
    })
}

/// The schema version the hub's registry source `text` declares (`SCHEMA_VERSION`).
pub fn hub_schema(text: &str) -> Option<i64> {
    text.lines().find_map(|l| {
        let l = l.trim();
        let rest = l.strip_prefix("pub const SCHEMA_VERSION: i64 = ")?;
        rest.trim_end_matches(';').trim().parse().ok()
    })
}

/// The hub's release and why: a minor one when its registry's schema moves (`before` at `main`,
/// `after` at the branch), since a patch never changes it.
pub fn hub_bump(before: Option<i64>, after: Option<i64>) -> (Bump, String) {
    match (before, after) {
        (Some(b), Some(a)) if a != b => (
            Bump::Minor,
            format!(
                "the registry's schema moves from {b} to {a}, which a patch release never does"
            ),
        ),
        _ => (Bump::Patch, "the registry's schema is unchanged".to_owned()),
    }
}

/// Empyrean's release and why: a patch unless its upgrade declaration refuses one (`refusal`, as
/// the release command would give it), when it is a minor release.
pub fn empyrean_bump(refusal: Option<&str>) -> (Bump, String) {
    match refusal {
        Some(why) => (Bump::Minor, format!("a patch release is refused: {why}")),
        None => (
            Bump::Patch,
            "nothing a patch release may not carry (no migration, pack change or renamed key)"
                .to_owned(),
        ),
    }
}

/// Whether the files `changed` make a Dereth release by the web client's changes alone: without
/// them, they would make none ([`products_of`]).
pub fn web_alone(
    changed: &[String],
    packages: &[(String, String)],
    depends: &BTreeMap<Product, BTreeSet<String>>,
) -> bool {
    let others: Vec<String> = changed.iter().filter(|p| !is_web(p)).cloned().collect();
    products_of(changed, packages, depends).contains(&Product::Dereth)
        && !products_of(&others, packages, depends).contains(&Product::Dereth)
}

/// Dereth's release and why: a patch, for the client's, the launcher's or the web client's changes.
/// When the web client's alone (`web_alone`) make it, the reason says so: the web client carries
/// Dereth's version, so it is released as a Dereth release, its own tag beside Dereth's.
pub fn dereth_bump(web_alone: bool) -> (Bump, String) {
    let why = if web_alone {
        "the web client's changes alone: it carries Dereth's version, so it is released with a \
         Dereth release, its own tag on Dereth's"
    } else {
        "the client's, the launcher's or the web client's changes"
    };
    (Bump::Patch, why.to_owned())
}

/// The release-candidate stamp of `product`'s next build of `version`: one past the last that
/// `stamped` records, which it then records.
pub fn stamp(product: Product, version: &str, stamped: &mut BTreeMap<String, u32>) -> String {
    let n = stamped
        .entry(format!("{} {version}", product.command_name()))
        .or_insert(0);
    *n += 1;
    format!("{version}-rc.{n}")
}
