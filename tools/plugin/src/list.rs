//! `add`: a packed release listed in a plugin source's `plugins.json`, in place of the plugin's
//! entry when it has one: a source lists each plugin at its latest release only.
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
