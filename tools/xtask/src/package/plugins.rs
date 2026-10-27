//! Dereth's own plugin source: the sample plugins of `dereth/plugins/`, each packed into a zip and
//! listed in a `plugins.json`, published on a host of their own ([`URL`]), from which the launcher
//! and the web client both read it.
//!
//! ```text
//! cargo xtask package plugins [--out <dir>]
//! ```
//!
//! writes the source into `<target dir>/package/plugins/` (or `--out`): `plugins.json`, and each
//! plugin's zip at `<id>/<id>-<version>-<hash>.zip`, which the list names relative to itself. The
//! hash is the start of the zip's SHA-256, so a zip whose bytes change under the same version (a
//! deploy from a branch, a change that kept its version) has a name of its own: its host keeps
//! each zip a year as immutable, and a browser keeps what it fetched under the old name. Every folder
//! of [`TREE`] holding a `plugin.toml` is listed, each with the files git tracks in it (a file left
//! in a sample's folder that git does not track, a route made for a test, say, is never packed),
//! packed as the author's tool packs, so the same tree gives the same bytes. Each entry is dated by
//! the last commit that changed its folder, and carries its version's section of the folder's
//! `CHANGES.md` when it has one.
//!
//! `cargo xtask deploy plugins` puts it on its Cloudflare Pages project (`crate::deploy::plugins`);
//! the owner runs that when plugins are ready. No release carries plugins of its own: a player
//! installs Dereth's plugins from its source, as anyone's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dereth_plugin_package::catalog::{Catalog, Entry};
use dereth_plugin_package::Package;

use super::guard;

/// Where the sample plugins are in the tree.
pub const TREE: &str = "dereth/plugins";

/// The source's list's address, the zips beside it. The launcher starts with this source.
pub const URL: &str = dereth_plugin_package::catalog::DERETH_SOURCE;

/// The source's name, as launchers show it.
pub const NAME: &str = "Dereth";

/// The list's file name.
pub const LIST: &str = dereth_plugin_package::catalog::FILE_NAME;

/// How many hexadecimal digits of the zip's SHA-256 its name carries.
const HASH_LEN: usize = 12;

/// The source's files, each at its path on the source's host (`plugins.json`, and
/// `<id>/<id>-<version>-<hash>.zip` for each plugin), from a listing of tracked files (`/`-separated,
/// below the workspace) and `read`, which gives a tracked file's bytes. `dated` gives the day each
/// plugin folder last changed, `YYYY-MM-DD`.
///
/// # Errors
/// A folder whose files do not make a package, or that holds a file no release may carry (a game
/// data file, or one named like it).
pub fn from_listing(
    tracked: &[String],
    read: &dyn Fn(&str) -> Result<Vec<u8>, String>,
    dated: &dyn Fn(&str) -> Result<String, String>,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut folders: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for path in tracked {
        if let Some((folder, rest)) = path
            .strip_prefix(TREE)
            .and_then(|p| p.strip_prefix('/'))
            .and_then(|p| p.split_once('/'))
        {
            folders.entry(folder).or_default().push(rest);
        }
    }
    let mut problems = Vec::new();
    let mut list = Catalog {
        name: NAME.to_owned(),
        plugins: Vec::new(),
    };
    let mut out = Vec::new();
    for (folder, files) in folders {
        if !files.contains(&dereth_plugin_package::MANIFEST_FILE) {
            continue;
        }
        let mut package = Package {
            origin: folder.to_owned(),
            ..Package::default()
        };
        for rel in files {
            let path = format!("{TREE}/{folder}/{rel}");
            let bytes = read(&path)?;
            if let Some(why) = guard::name_finding(&path) {
                problems.push(format!("{path}: {why}"));
            }
            if guard::has_dat_magic(&bytes[..bytes.len().min(guard::HEAD_LEN)]) {
                problems.push(format!("{path}: carries the game's data-file header"));
            }
            package.files.insert(rel.to_owned(), bytes);
        }
        let manifest = match package.manifest() {
            Ok(m) => m,
            Err(e) => {
                problems.push(format!("{TREE}/{folder}: {e}"));
                continue;
            }
        };
        let zip = package.to_zip();
        let hash = dereth_plugin_package::catalog::sha256_hex(&zip);
        let at = format!(
            "{0}/{0}-{1}-{2}.zip",
            manifest.id,
            manifest.version,
            &hash[..HASH_LEN]
        );
        let notes = package
            .text("CHANGES.md")
            .and_then(|t| section(&t, &manifest.version.to_string()))
            .unwrap_or_default();
        let released = dated(&format!("{TREE}/{folder}"))?;
        let entry = Entry::for_zip(&zip, &at, &notes, &released)
            .map_err(|e| format!("{TREE}/{folder}: {e}"))?;
        if list.put(entry).is_some() {
            problems.push(format!(
                "{TREE}/{folder}: another sample has the id {}",
                manifest.id
            ));
        }
        out.push((at, zip));
    }
    if !problems.is_empty() {
        return Err(format!(
            "Dereth's plugin source:\n  {}",
            problems.join("\n  ")
        ));
    }
    out.push((LIST.to_owned(), list.to_json().into_bytes()));
    out.sort();
    Ok(out)
}

/// The lines under a change log's `## <version>` heading, trimmed.
fn section(text: &str, version: &str) -> Option<String> {
    let mut lines = text.lines().skip_while(|l| {
        !l.strip_prefix("## ")
            .and_then(|h| h.strip_prefix(version))
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '(']))
    });
    lines.next()?;
    let body: Vec<&str> = lines.take_while(|l| !l.starts_with("## ")).collect();
    Some(body.join("\n").trim().to_owned())
}

/// The source, from the tree as git tracks it: each file's path on the source's host and its
/// bytes.
///
/// # Errors
/// As [`from_listing`], or git could not list the files.
pub fn source(ws: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let listing = super::git(ws, &["ls-files", "--", TREE])?;
    let tracked: Vec<String> = listing
        .lines()
        .map(|l| l.trim().replace('\\', "/"))
        .filter(|l| !l.is_empty())
        .collect();
    from_listing(&tracked, &|p| super::read(&ws.join(p)), &|folder| {
        super::git(ws, &["log", "-1", "--format=%cs", "--", folder])
    })
}

pub const USAGE: &str = "usage: cargo xtask package plugins [--out <dir>]";

/// `cargo xtask package plugins ...`.
pub fn package(args: &[String]) -> i32 {
    let out = match args {
        [] => crate::util::target_dir().join("package").join("plugins"),
        [flag, dir] if flag == "--out" => PathBuf::from(dir),
        _ => {
            eprintln!("{USAGE}");
            return 2;
        }
    };
    match write_source(&crate::util::workspace_root(), &out) {
        Ok(said) => {
            println!("{said}");
            0
        }
        Err(e) => {
            eprintln!("package: FAILED\n{e}");
            1
        }
    }
}

/// Write the source into `out`, replacing what a run before left there.
fn write_source(ws: &Path, out: &Path) -> Result<String, String> {
    let files = source(ws)?;
    if out.exists() {
        std::fs::remove_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    for (path, bytes) in &files {
        let to = out.join(path);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        super::write(&to, bytes)?;
    }
    Ok(format!(
        "Dereth's plugin source: {} plugins in {}; `cargo xtask deploy plugins` publishes it at {URL}.",
        files.len() - 1,
        out.display()
    ))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: Dereth's plugin source, packed and listed from the tree).
    use super::*;
    use crate::util::workspace_root;
    use dereth_plugin_package::catalog;

    /// The source from the tree: every sample folder listed once, each zip the release its entry
    /// lists, named relative to the list, at the address the launcher starts with.
    #[test]
    fn dereths_source_lists_every_sample_with_its_zip_beside_the_list() {
        let ws = workspace_root();
        let files = source(&ws).expect("the source");
        let (_, list) = files.iter().find(|(p, _)| p == LIST).expect("plugins.json");
        let read = Catalog::parse(list).expect("reads");
        assert!(read.skipped.is_empty(), "{:?}", read.skipped);
        assert_eq!(read.catalog.name, NAME);
        let samples = std::fs::read_dir(ws.join(TREE))
            .expect("the samples")
            .filter_map(Result::ok)
            .filter(|e| e.path().join("plugin.toml").is_file())
            .count();
        assert_eq!(read.catalog.plugins.len(), samples);
        assert_eq!(files.len(), samples + 1, "the list and one zip each");
        for e in &read.catalog.plugins {
            assert_eq!(
                e.zip,
                format!(
                    "{0}/{0}-{1}-{2}.zip",
                    e.id,
                    e.version,
                    &e.sha256[..HASH_LEN]
                )
            );
            let (_, zip) = files.iter().find(|(p, _)| *p == e.zip).expect("its zip");
            e.check(zip).unwrap_or_else(|m| panic!("{}: {m}", e.id));
            assert!(e.id.as_str().starts_with("network.dereth."), "{}", e.id);
            assert_eq!(e.released.len(), 10, "{}: dated", e.id);
            assert_eq!(
                catalog::resolve(URL, &e.zip),
                Some(format!("https://plugins.dereth.network/{}", e.zip))
            );
        }
        let tank = read.catalog.get("network.dereth.tank").expect("the Tank");
        let (_, zip) = files.iter().find(|(p, _)| *p == tank.zip).expect("its zip");
        let package = Package::from_zip("tank", zip).expect("a package");
        assert!(
            package.files.contains_key("THIRD-PARTY.md"),
            "its licenses travel with it"
        );
    }

    #[test]
    fn the_sources_address_is_its_list_on_its_own_host() {
        assert_eq!(URL, dereth_launch::plugins::FIRST_PARTY);
        assert_eq!(URL, format!("https://plugins.dereth.network/{LIST}"));
    }

    fn files(list: &[(&str, &str)]) -> (Vec<String>, BTreeMap<String, Vec<u8>>) {
        let tracked = list.iter().map(|(p, _)| (*p).to_owned()).collect();
        let bytes = list
            .iter()
            .map(|(p, b)| ((*p).to_owned(), b.as_bytes().to_vec()))
            .collect();
        (tracked, bytes)
    }

    fn manifest(id: &str, version: &str) -> String {
        format!("id = \"{id}\"\nname = \"N\"\nversion = \"{version}\"\napi = \"0.7\"\n")
    }

    /// The same tree gives the same bytes; a folder with no manifest is not a plugin; a release's
    /// notes come from the folder's change log; a game data file, a folder that is not a package
    /// or two samples of one id refuse the source.
    #[test]
    fn the_source_is_the_same_bytes_from_the_same_tree_and_refuses_what_is_no_plugin() {
        let a = manifest("network.dereth.a", "1.2.0");
        let changes = "# Changes\n\n## 1.2.0 (2026-10-26)\n\n- Faster.\n\n## 1.1.0\n\n- Older.\n";
        let (tracked, bytes) = files(&[
            ("dereth/plugins/a/plugin.toml", &a),
            ("dereth/plugins/a/main.lua", "print(1)"),
            ("dereth/plugins/a/CHANGES.md", changes),
            ("dereth/plugins/notes/README.md", "not a plugin"),
        ]);
        let read = |p: &str| bytes.get(p).cloned().ok_or_else(|| p.to_owned());
        let dated = |_: &str| Ok("2026-10-26".to_owned());
        let once = from_listing(&tracked, &read, &dated).expect("the source");
        assert_eq!(once, from_listing(&tracked, &read, &dated).expect("again"));
        let names: Vec<&str> = once.iter().map(|(p, _)| p.as_str()).collect();
        let hash = &catalog::sha256_hex(&once[0].1)[..HASH_LEN];
        assert_eq!(
            names,
            [
                format!("network.dereth.a/network.dereth.a-1.2.0-{hash}.zip").as_str(),
                "plugins.json"
            ]
        );
        let list = Catalog::parse(&once[1].1).expect("reads").catalog;
        assert_eq!(list.plugins[0].notes, "- Faster.");

        // A change under the same version is a zip of another name.
        let mut changed = bytes.clone();
        changed.insert("dereth/plugins/a/main.lua".into(), b"print(2)".to_vec());
        let read_changed = |p: &str| changed.get(p).cloned().ok_or_else(|| p.to_owned());
        let other = from_listing(&tracked, &read_changed, &dated).expect("the source");
        assert_ne!(other[0].0, once[0].0);
        assert!(other[0]
            .0
            .starts_with("network.dereth.a/network.dereth.a-1.2.0-"));

        let mut odd = tracked.clone();
        odd.push("dereth/plugins/a/cell.dat".into());
        let mut odd_bytes = bytes.clone();
        odd_bytes.insert("dereth/plugins/a/cell.dat".into(), vec![0; 4]);
        let read_odd = |p: &str| odd_bytes.get(p).cloned().ok_or_else(|| p.to_owned());
        let refused = from_listing(&odd, &read_odd, &dated).expect_err("refused");
        assert!(refused.contains("cell.dat"), "{refused}");

        let b = manifest("network.dereth.a", "2.0.0");
        let (twice, twice_bytes) = files(&[
            ("dereth/plugins/a/plugin.toml", &a),
            ("dereth/plugins/a/main.lua", "print(1)"),
            ("dereth/plugins/b/plugin.toml", &b),
            ("dereth/plugins/b/main.lua", "print(1)"),
        ]);
        let read_twice = |p: &str| twice_bytes.get(p).cloned().ok_or_else(|| p.to_owned());
        let refused = from_listing(&twice, &read_twice, &dated).expect_err("refused");
        assert!(refused.contains("another sample has the id"), "{refused}");

        let (broken, broken_bytes) = files(&[("dereth/plugins/c/plugin.toml", "id = 1")]);
        let read_broken = |p: &str| broken_bytes.get(p).cloned().ok_or_else(|| p.to_owned());
        assert!(from_listing(&broken, &read_broken, &dated).is_err());
    }
}
