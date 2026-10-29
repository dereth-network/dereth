//! `latest` and `site`: a repository's plugin source made afresh from its GitHub releases, as the
//! template's Pages workflow makes it after each release and for each push that adds, changes or
//! removes a plugin.
//!
//! Each plugin the repository holds ([`crate::release::packages_in`]) is listed at its latest
//! release: of the releases that are neither drafts nor pre-releases and hold its zip, named
//! `<id>-<version>.zip` as the release workflow names it, the one of the highest version. A plugin
//! with no such release is not listed, and a plugin the repository no longer holds is not listed
//! whatever its releases, so a plugin leaves the source with its folder. The releases are GitHub's
//! own JSON: the API's array, or one release to a line
//! (`gh api --paginate repos/<owner>/<repo>/releases --jq '.[]'`). The tool reaches no network:
//! `latest` names the zips to download, and `site` makes the source of them.

use std::path::Path;

use dereth_plugin_api::manifest::PluginVersion;
use dereth_plugin_package::catalog::{Catalog, Entry};
use serde_json::Value;

/// A plugin's latest release.
#[derive(Debug)]
pub struct Pick {
    pub id: String,
    pub version: PluginVersion,
    /// The release's tag.
    pub tag: String,
    /// The zip's file name among the release's assets.
    pub zip: String,
    /// The release's description: its notes.
    pub notes: String,
    /// The day it was published, `YYYY-MM-DD`.
    pub released: String,
}

/// The releases in `text`: a JSON array of them, or one to a line.
fn releases(text: &str) -> Result<Vec<Value>, String> {
    if text.trim_start().starts_with('[') {
        return serde_json::from_str(text).map_err(|e| format!("the releases do not read: {e}"));
    }
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            serde_json::from_str(l).map_err(|e| format!("the release on line {}: {e}", i + 1))
        })
        .collect()
}

/// The day of a GitHub time, `2026-10-28T08:56:20Z`: `2026-10-28`.
fn day(at: &str) -> Option<String> {
    let d = at.get(..10)?;
    let shaped = d.bytes().enumerate().all(|(i, b)| {
        if i == 4 || i == 7 {
            b == b'-'
        } else {
            b.is_ascii_digit()
        }
    });
    shaped.then(|| d.to_owned())
}

/// Each plugin of the repository at `repo` at its latest release among `text`'s, in id order.
pub fn picks(repo: &Path, text: &str) -> Result<Vec<Pick>, String> {
    let mut held = Vec::new();
    for folder in crate::release::packages_in(repo)? {
        held.push(crate::release::manifest_of(&folder)?.id.to_string());
    }
    if held.is_empty() {
        return Err(format!(
            "{} holds no plugin: give the repository's own folder",
            repo.display()
        ));
    }
    let mut picks: Vec<Pick> = Vec::new();
    for r in releases(text)? {
        if r["draft"].as_bool() == Some(true) || r["prerelease"].as_bool() == Some(true) {
            continue;
        }
        let tag = r["tag_name"].as_str().ok_or("a release has no tag_name")?;
        let released = r["published_at"]
            .as_str()
            .and_then(day)
            .ok_or_else(|| format!("the release {tag} has no published_at"))?;
        let notes = r["body"].as_str().unwrap_or_default().replace("\r\n", "\n");
        for asset in r["assets"].as_array().into_iter().flatten() {
            let Some(zip) = asset["name"].as_str() else {
                continue;
            };
            let Some((id, version)) = held.iter().find_map(|id| {
                let v = zip
                    .strip_suffix(".zip")?
                    .strip_prefix(id.as_str())?
                    .strip_prefix('-')?;
                Some((id, PluginVersion::parse(v)?))
            }) else {
                continue;
            };
            let pick = Pick {
                id: id.clone(),
                version,
                tag: tag.to_owned(),
                zip: zip.to_owned(),
                notes: notes.clone(),
                released: released.clone(),
            };
            match picks.iter_mut().find(|p| &p.id == id) {
                Some(p) if pick.version.is_newer_than(&p.version) => *p = pick,
                Some(_) => {}
                None => picks.push(pick),
            }
        }
    }
    picks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(picks)
}

fn read(file: &Path) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))
}

/// `latest`: the zip of each plugin's latest release, for the workflow to download before `site`,
/// one to a line: `<tag>\t<zip>`.
pub fn latest(repo: &Path, releases_file: &Path) -> Result<String, String> {
    let picks = picks(repo, &read(releases_file)?)?;
    Ok(picks
        .iter()
        .map(|p| format!("{}\t{}", p.tag, p.zip))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// `site`: the source, written into `out`, which must be missing or empty: each plugin's latest
/// release, its zip copied from `zips` (where the zips `latest` names were downloaded) to
/// `<id>/<zip>`, listed in `plugins.json` with the release's notes and day, the source named
/// `name`. A zip that does not hold the plugin and version its name says is refused.
pub fn site(
    repo: &Path,
    releases_file: &Path,
    zips: &Path,
    out: &Path,
    name: Option<&str>,
) -> Result<String, String> {
    let picks = picks(repo, &read(releases_file)?)?;
    if std::fs::read_dir(out).is_ok_and(|mut d| d.next().is_some()) {
        return Err(format!("{} is not empty", out.display()));
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut list = Catalog {
        name: name.unwrap_or_default().trim().to_owned(),
        ..Catalog::default()
    };
    let mut said = Vec::new();
    for p in &picks {
        let from = zips.join(&p.zip);
        let bytes = std::fs::read(&from).map_err(|e| {
            format!(
                "{}: {e} (download the zips `dereth-plugin latest` names first)",
                from.display()
            )
        })?;
        let entry = Entry::for_zip(
            &bytes,
            &format!("{}/{}", p.id, p.zip),
            &p.notes,
            &p.released,
        )
        .map_err(|e| format!("{}: {e}", from.display()))?;
        if entry.id.as_str() != p.id || entry.version != p.version {
            return Err(format!(
                "{} holds {} {}, not what its name says",
                from.display(),
                entry.id,
                entry.version
            ));
        }
        let folder = out.join(&p.id);
        std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        let to = folder.join(&p.zip);
        std::fs::write(&to, &bytes).map_err(|e| format!("{}: {e}", to.display()))?;
        said.push(format!("listed {} {} from {}", p.id, p.version, p.tag));
        list.put(entry);
    }
    let list_file = out.join("plugins.json");
    std::fs::write(&list_file, list.to_json())
        .map_err(|e| format!("{}: {e}", list_file.display()))?;
    if said.is_empty() {
        said.push("no plugin has a release yet: the list is empty".to_owned());
    }
    Ok(said.join("\n"))
}
