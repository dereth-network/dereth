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
    /// Lyceum, the world builder: its editor and its command.
    Lyceum,
}

impl Product {
    /// Every product, in the order the preflight builds them and `ship` releases them.
    pub const ALL: [Self; 5] = [
        Self::Hub,
        Self::Ace,
        Self::Empyrean,
        Self::Dereth,
        Self::Lyceum,
    ];

    /// Its name on the command line of `cargo xtask release`.
    pub const fn command_name(self) -> &'static str {
        match self {
            Self::Hub => "network-hub",
            Self::Ace => "network-ace",
            Self::Empyrean => "empyrean",
            Self::Dereth => "dereth",
            Self::Lyceum => "lyceum",
        }
    }

    /// The packages its release ships, each with the lock file of the workspace it is built in
    /// ([`LOCKS`]): what a change must reach for the product to be released. Dereth's are the
    /// client, the web client (it carries Dereth's version) and the launcher. The ACE mod is not
    /// built by cargo; Lyceum is released for its own folder's changes ([`products_of`]), and the
    /// client's drawing it is built on reaches it with its next release.
    pub const fn ships(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Hub => &[("Cargo.lock", "dereth-network-hub")],
            Self::Empyrean => &[
                ("Cargo.lock", "empyrean-server"),
                ("Cargo.lock", "empyrean-import"),
            ],
            Self::Dereth => &[
                ("Cargo.lock", "dereth-client"),
                ("Cargo.lock", "dereth-web"),
                ("dereth/launcher/Cargo.lock", "dereth-launcher"),
            ],
            Self::Ace | Self::Lyceum => &[],
        }
    }
}

/// What the products are built from, as `cargo metadata` resolves each workspace they ship from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Built {
    /// Every path crate, in a workspace or not (a vendored one), as (name, folder from the
    /// repository's root).
    pub packages: Vec<(String, String)>,
    /// Each product's path crates: the ones the packages it ships ([`Product::ships`]) reach
    /// through normal and build dependencies, which is what its release is built from.
    pub depends: BTreeMap<Product, BTreeSet<String>>,
    /// For each package, the crates it depends on only as dev-dependencies, by name: what its
    /// tests are built with, which no release is.
    pub dev_only: BTreeMap<String, BTreeSet<String>>,
    /// Each shipped package (product, package) its workspace does not hold, which would leave the
    /// product short of what it is built from.
    pub missing: Vec<(Product, &'static str)>,
}

impl Built {
    /// From `metas`: each workspace's lock file ([`LOCKS`]) and its `cargo metadata` answer;
    /// `root` is the repository's root.
    pub fn read(metas: &[(&str, serde_json::Value)], root: &std::path::Path) -> Self {
        let mut built = Self::default();
        let mut folders: BTreeMap<String, String> = BTreeMap::new();
        let mut kept: BTreeMap<(String, String), bool> = BTreeMap::new();
        for (lock, meta) in metas {
            let str_of = |v: &serde_json::Value| v.as_str().unwrap_or_default().to_owned();
            let mut names: BTreeMap<String, String> = BTreeMap::new();
            let mut paths: BTreeSet<String> = BTreeSet::new();
            for p in meta["packages"].as_array().into_iter().flatten() {
                let (id, name) = (str_of(&p["id"]), str_of(&p["name"]));
                if p["source"].is_null() {
                    let manifest = std::path::PathBuf::from(str_of(&p["manifest_path"]));
                    if let Some(dir) = manifest.parent().and_then(|d| d.strip_prefix(root).ok()) {
                        folders.insert(name.clone(), dir.display().to_string().replace('\\', "/"));
                    }
                    paths.insert(id.clone());
                }
                names.insert(id, name);
            }
            // Each package's dependencies, and whether one is a dev-dependency alone.
            let mut edges: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
            for n in meta["resolve"]["nodes"].as_array().into_iter().flatten() {
                let from = str_of(&n["id"]);
                for d in n["deps"].as_array().into_iter().flatten() {
                    let to = str_of(&d["pkg"]);
                    let kinds = d["dep_kinds"].as_array().cloned().unwrap_or_default();
                    let dev = !kinds.is_empty() && kinds.iter().all(|k| k["kind"] == "dev");
                    if let (Some(a), Some(b)) = (names.get(&from), names.get(&to)) {
                        *kept.entry((a.clone(), b.clone())).or_default() |= !dev;
                    }
                    edges.entry(from.clone()).or_default().push((to, dev));
                }
            }
            let members: BTreeSet<String> = meta["workspace_members"]
                .as_array()
                .into_iter()
                .flatten()
                .map(str_of)
                .collect();
            for product in Product::ALL {
                let roots: Vec<&str> = product
                    .ships()
                    .iter()
                    .filter(|(l, _)| l == lock)
                    .map(|(_, p)| *p)
                    .collect();
                let mut seen = BTreeSet::new();
                let mut todo: Vec<String> = members
                    .iter()
                    .filter(|id| names.get(*id).is_some_and(|n| roots.contains(&n.as_str())))
                    .cloned()
                    .collect();
                for root in &roots {
                    if !todo
                        .iter()
                        .any(|id| names.get(id).is_some_and(|n| n == root))
                    {
                        built.missing.push((product, root));
                    }
                }
                while let Some(id) = todo.pop() {
                    if seen.insert(id.clone()) {
                        let next = edges.get(&id).into_iter().flatten();
                        todo.extend(next.filter(|(_, dev)| !dev).map(|(to, _)| to.clone()));
                    }
                }
                let crates = seen
                    .iter()
                    .filter(|id| paths.contains(*id))
                    .filter_map(|id| names.get(id).cloned());
                built.depends.entry(product).or_default().extend(crates);
            }
        }
        built.depends.retain(|_, crates| !crates.is_empty());
        built.packages = folders.into_iter().collect();
        for ((from, to), kept) in kept {
            if !kept {
                built.dev_only.entry(from).or_default().insert(to);
            }
        }
        built
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
/// to the path crate whose folder holds it (`packages`, name and folder: a workspace's package or
/// a vendored crate, [`Built::packages`]) and so to every product built from that crate
/// (`depends`, [`Built::depends`]); the
/// ACE mod's folder is the mod's, Lyceum's folder is Lyceum's, a change to the launcher's own app
/// belongs to Dereth, the web client is released as Dereth (it carries Dereth's version), and
/// nothing else (the docs, the research notes) is a product. A lock file ([`LOCKS`]) belongs to no
/// product by itself: a product is in when the crates it is built from moved in one (`moved`, from
/// [`lock_moves`]).
pub fn products_of(
    changed: &[String],
    packages: &[(String, String)],
    depends: &BTreeMap<Product, BTreeSet<String>>,
    moved: &BTreeMap<Product, Vec<String>>,
) -> BTreeSet<Product> {
    let mut out: BTreeSet<Product> = moved
        .iter()
        .filter(|(_, crates)| !crates.is_empty())
        .map(|(p, _)| *p)
        .collect();
    for path in changed {
        let path = path.replace('\\', "/");
        if LOCKS.contains(&path.as_str()) {
            continue;
        }
        if path.starts_with("network/ace-mod/") {
            out.insert(Product::Ace);
            continue;
        }
        if path.starts_with("lyceum/") {
            out.insert(Product::Lyceum);
            continue;
        }
        if (path.starts_with("dereth/launcher/") && !path.starts_with("dereth/launcher/crates/"))
            || path.starts_with(WEB)
        {
            out.insert(Product::Dereth);
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

/// The lock files a preflight weighs crate by crate: the main workspace's, and the launcher's own
/// workspace's, which is Dereth's.
pub const LOCKS: [&str; 2] = ["Cargo.lock", "dereth/launcher/Cargo.lock"];

/// A crate as a lock file pins it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Locked {
    pub name: String,
    pub version: String,
    /// Where it comes from (a registry, a git commit); none for a path crate.
    pub source: Option<String>,
}

/// A `Cargo.lock`: each crate it pins, and the crates each depends on as the lock lists them.
#[derive(Debug, Clone, Default)]
pub struct Lock {
    crates: Vec<(Locked, Vec<String>)>,
    /// Each name's crates, by their place in `crates`.
    by_name: BTreeMap<String, Vec<usize>>,
}

impl Lock {
    /// The lock in `text`. An empty text, for a lock file that is not there, is an empty lock.
    ///
    /// # Errors
    /// When `text` is not a lock file.
    pub fn parse(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct File {
            #[serde(default)]
            package: Vec<Entry>,
        }
        #[derive(Deserialize)]
        struct Entry {
            name: String,
            version: String,
            source: Option<String>,
            #[serde(default)]
            dependencies: Vec<String>,
        }
        let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut lock = Self::default();
        for e in file.package {
            lock.by_name
                .entry(e.name.clone())
                .or_default()
                .push(lock.crates.len());
            let locked = Locked {
                name: e.name,
                version: e.version,
                source: e.source,
            };
            lock.crates.push((locked, e.dependencies));
        }
        Ok(lock)
    }

    /// The crate a dependency list's entry names: `name`, or `name version` when the lock holds
    /// more than one version of it, or `name version (source)` when more than one source.
    fn find(&self, entry: &str) -> Option<usize> {
        let mut parts = entry.splitn(3, ' ');
        let name = parts.next()?;
        let version = parts.next();
        let source = parts
            .next()
            .map(|s| s.trim_start_matches('(').trim_end_matches(')'));
        self.by_name.get(name)?.iter().copied().find(|&i| {
            let c = &self.crates[i].0;
            version.is_none_or(|v| c.version == v)
                && source.is_none_or(|s| c.source.as_deref() == Some(s))
        })
    }

    /// Every crate the packages `roots` are built from: the roots and all they reach through the
    /// dependency lists, leaving out what a package depends on only as a dev-dependency
    /// (`dev_only`, [`Built::dev_only`]), which the lock does not tell from the rest. A root the
    /// lock does not hold is left out.
    pub fn closure(
        &self,
        roots: &[&str],
        dev_only: &BTreeMap<String, BTreeSet<String>>,
    ) -> BTreeSet<Locked> {
        let mut seen = BTreeSet::new();
        let mut todo: Vec<usize> = roots.iter().filter_map(|r| self.find(r)).collect();
        while let Some(i) = todo.pop() {
            if seen.insert(i) {
                let (from, deps) = &self.crates[i];
                let tests_only = dev_only.get(&from.name);
                todo.extend(deps.iter().filter_map(|d| self.find(d)).filter(|&j| {
                    tests_only.is_none_or(|names| !names.contains(&self.crates[j].0.name))
                }));
            }
        }
        seen.into_iter().map(|i| self.crates[i].0.clone()).collect()
    }
}

/// What moved from one set of crates to another, a line per crate name: `serde 1.0.210 ->
/// 1.0.214`, `bytes 1.10.0 (added)`, `atty 0.2.14 (removed)`. A crate from somewhere other than
/// crates.io names its source. Path crates are left out: a change to one is a change to its
/// folder, which [`products_of`] weighs. Empty when nothing moved.
pub fn crate_moves(before: &BTreeSet<Locked>, after: &BTreeSet<Locked>) -> Vec<String> {
    fn label(c: &Locked) -> String {
        match c.source.as_deref() {
            Some(s) if !s.contains("crates.io-index") && !s.contains("index.crates.io") => {
                format!("{} ({s})", c.version)
            }
            _ => c.version.clone(),
        }
    }
    let side = |set: &BTreeSet<Locked>, name: &str| {
        set.iter()
            .filter(|c| c.name == name && c.source.is_some())
            .map(label)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let names: BTreeSet<&str> = before
        .symmetric_difference(after)
        .filter(|c| c.source.is_some())
        .map(|c| c.name.as_str())
        .collect();
    names
        .into_iter()
        .map(|name| {
            let (was, now) = (side(before, name), side(after, name));
            if was.is_empty() {
                format!("{name} {now} (added)")
            } else if now.is_empty() {
                format!("{name} {was} (removed)")
            } else {
                format!("{name} {was} -> {now}")
            }
        })
        .collect()
}

/// For each product shipped from a lock file among `changed` ([`LOCKS`], [`Product::ships`]),
/// what moved among the crates its release is built from ([`Lock::closure`], leaving out the
/// dev-dependencies `dev_only` names) between the lock's two texts that `texts` gives: at the
/// branch's merge base with `main`, and at the branch. A product none of whose crates moved is
/// not here: a lock change alone does not make it a release.
///
/// # Errors
/// When a lock text is not a lock file.
pub fn lock_moves(
    changed: &[String],
    texts: &dyn Fn(&str) -> (String, String),
    dev_only: &BTreeMap<String, BTreeSet<String>>,
) -> Result<BTreeMap<Product, Vec<String>>, String> {
    let mut out: BTreeMap<Product, Vec<String>> = BTreeMap::new();
    for lock in LOCKS {
        if !changed.iter().any(|p| p.replace('\\', "/") == lock) {
            continue;
        }
        let (before, after) = texts(lock);
        let before = Lock::parse(&before).map_err(|e| format!("{lock} at the merge base: {e}"))?;
        let after = Lock::parse(&after).map_err(|e| format!("{lock}: {e}"))?;
        for product in Product::ALL {
            let roots: Vec<&str> = product
                .ships()
                .iter()
                .filter(|(l, _)| *l == lock)
                .map(|(_, p)| *p)
                .collect();
            if roots.is_empty() {
                continue;
            }
            let (was, now) = (
                before.closure(&roots, dev_only),
                after.closure(&roots, dev_only),
            );
            for line in crate_moves(&was, &now) {
                // Dereth's two locks can move the same crate alike.
                let said = out.entry(product).or_default();
                if !said.contains(&line) {
                    said.push(line);
                }
            }
        }
    }
    Ok(out)
}

/// Why a product is released for its locked crates `moves`, for its line in the plan: the first
/// few, and how many more.
pub fn moves_why(moves: &[String]) -> String {
    const SHOWN: usize = 4;
    let mut said = moves[..moves.len().min(SHOWN)].join(", ");
    if moves.len() > SHOWN {
        said.push_str(&format!(" and {} more", moves.len() - SHOWN));
    }
    format!("its dependencies moved: {said}")
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
    moved: &BTreeMap<Product, Vec<String>>,
) -> bool {
    let others: Vec<String> = changed.iter().filter(|p| !is_web(p)).cloned().collect();
    products_of(changed, packages, depends, moved).contains(&Product::Dereth)
        && !products_of(&others, packages, depends, moved).contains(&Product::Dereth)
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

/// Lyceum's release and why: a patch, for its own folder's changes.
pub fn lyceum_bump() -> (Bump, String) {
    (
        Bump::Patch,
        "the editor's, the lyceum command's or the Foundry's changes".to_owned(),
    )
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
