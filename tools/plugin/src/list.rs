//! `add`: a packed release listed in a plugin source's `plugins.json`, in place of the plugin's
//! entry when it has one: a source lists each plugin at its latest release only. `prune`: the
//! plugins a repository no longer holds dropped from its source's list, with their zips.
//!
//! The zip may be anywhere: beside the list (a path relative to it, as the template's workflow
//! publishes it on the repository's Pages) or at any `https://` address (an index whose entries
//! point at each plugin's own releases).

use std::path::Path;

use dereth_plugin_package::catalog::{self, Catalog, Entry};

/// List the release `zip` in the list at `list_file` (made when there is none), downloaded from
/// `url`, with `notes`, released on `date`. `name` names the source.
pub fn add(
    list_file: &Path,
    zip: &Path,
    url: &str,
    notes: &str,
    name: Option<&str>,
    date: &str,
) -> Result<String, String> {
    if catalog::resolve("https://example.org/plugins.json", url).is_none() {
        return Err(format!(
            "{url} is not an https:// address or a path relative to the list"
        ));
    }
    let bytes = std::fs::read(zip).map_err(|e| format!("{}: {e}", zip.display()))?;
    let entry =
        Entry::for_zip(&bytes, url, notes, date).map_err(|e| format!("{}: {e}", zip.display()))?;
    let mut list = match std::fs::read(list_file) {
        Ok(text) => {
            let read =
                Catalog::parse(&text).map_err(|e| format!("{}: {e}", list_file.display()))?;
            if !read.skipped.is_empty() {
                return Err(format!(
                    "{} has entries that do not read, which writing it would drop; fix them first:\n  {}",
                    list_file.display(),
                    read.skipped.join("\n  ")
                ));
            }
            read.catalog
        }
        Err(_) => Catalog::default(),
    };
    if let Some(name) = name {
        list.name = name.trim().to_owned();
    }
    let said = match list.get(entry.id.as_str()) {
        Some(old) if old.version == entry.version && old.sha256 == entry.sha256 => {
            format!("{} {} is listed already", entry.id, entry.version)
        }
        Some(old) if !entry.version.is_newer_than(&old.version) => {
            return Err(format!(
                "{} lists {} {}; a new release must be newer",
                list_file.display(),
                entry.id,
                old.version
            ));
        }
        Some(old) => format!(
            "listed {} {} in place of {}",
            entry.id, entry.version, old.version
        ),
        None => format!("listed {} {}", entry.id, entry.version),
    };
    list.put(entry);
    std::fs::write(list_file, list.to_json())
        .map_err(|e| format!("{}: {e}", list_file.display()))?;
    Ok(said)
}

/// `prune`: drop from the list at `list_file` every plugin the repository at `repo` no longer
/// holds, its plugins found as every command finds a repository's ([`crate::release::packages_in`]),
/// and the zip of each one dropped (with its `.sha256`, when `pack` left one) when the list names
/// it by a path beside itself. A zip at an `https://` address, or named any other way, is left
/// where it is. Nothing is written when every plugin listed is held, or when there is no list.
///
/// A repository that holds no plugin at all, or one whose `plugin.toml` does not read, drops
/// nothing: the list is never emptied by a wrong folder or a broken manifest.
pub fn prune(list_file: &Path, repo: &Path) -> Result<String, String> {
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
    let bytes = match std::fs::read(list_file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(format!(
                "{} is not there: nothing to unlist",
                list_file.display()
            ));
        }
        Err(e) => return Err(format!("{}: {e}", list_file.display())),
    };
    let read = Catalog::parse(&bytes).map_err(|e| format!("{}: {e}", list_file.display()))?;
    if !read.skipped.is_empty() {
        return Err(format!(
            "{} has entries that do not read, which writing it would drop; fix them first:\n  {}",
            list_file.display(),
            read.skipped.join("\n  ")
        ));
    }
    let mut list = read.catalog;
    let (kept, dropped): (Vec<Entry>, Vec<Entry>) = list
        .plugins
        .into_iter()
        .partition(|e| held.iter().any(|id| id == e.id.as_str()));
    list.plugins = kept;
    if dropped.is_empty() {
        return Ok(format!(
            "every plugin {} lists is in {}",
            list_file.display(),
            repo.display()
        ));
    }
    // The list first: a zip left behind is harmless, a list naming a zip that is gone is not.
    std::fs::write(list_file, list.to_json())
        .map_err(|e| format!("{}: {e}", list_file.display()))?;
    let beside = list_file.parent().unwrap_or(Path::new(""));
    let mut said = Vec::new();
    for e in &dropped {
        let named_still = list.plugins.iter().any(|k| k.zip.trim() == e.zip.trim());
        if let Some(parts) = path_beside(&e.zip).filter(|_| !named_still) {
            let zip = parts.iter().fold(beside.to_path_buf(), |p, s| p.join(s));
            let mut sum = zip.clone().into_os_string();
            sum.push(".sha256");
            for file in [zip.clone(), sum.into()] {
                match std::fs::remove_file(&file) {
                    Ok(()) => {}
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => return Err(format!("{}: {err}", file.display())),
                }
            }
            // The folders it was in, once they are empty, up to the list's own.
            let mut folder = zip.parent();
            while let Some(f) = folder.filter(|f| *f != beside) {
                if std::fs::remove_dir(f).is_err() {
                    break;
                }
                folder = f.parent();
            }
        }
        said.push(format!("unlisted {} {}", e.id, e.version));
    }
    Ok(said.join("\n"))
}

/// The parts of `zip`, an entry's zip as its list names it, when it is a plain path beside the
/// list: no address, no query, no `..`, nothing escaped. `None` for anything else.
fn path_beside(zip: &str) -> Option<Vec<&str>> {
    let zip = zip.trim();
    if zip.is_empty() || zip.starts_with('/') || zip.contains([':', '?', '#', '%', '\\']) {
        return None;
    }
    let parts: Vec<&str> = zip.split('/').collect();
    parts
        .iter()
        .all(|p| !p.is_empty() && *p != "." && *p != "..")
        .then_some(parts)
}
