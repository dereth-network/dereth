//! The plugins that come with Dereth: sample plugins of `dereth/plugins/`, carried by the release
//! files beside the client and listed in its plugin panel, each off until the player turns it on.
//!
//! Plugins are experimental, and releases carry none of them for now ([`SHIPPED`]). The rest of
//! this module stays as it is, so carrying them again is setting that switch.
//!
//! A release carries [`BUNDLED`]'s folders, each file as git tracks it (a file left in a sample's
//! folder that git does not track, a route made for a test, say, is never carried), published as
//! `plugins/<folder>/<path>`. Only the kinds of file a plugin is made of are carried ([`KINDS`]):
//! anything else in a carried folder fails the package, as does a folder with no manifest.

use std::path::{Path, PathBuf};

/// Whether a release carries the sample plugins: not while plugins are experimental.
pub const SHIPPED: bool = false;

/// Where the sample plugins are in the tree.
pub const SOURCE: &str = "dereth/plugins";

/// The folder they are published in, beside the client.
pub const PUBLISHED: &str = "plugins";

/// The sample folders a release carries: the trackers, the macro and the navigator in the manner
/// of the add-ons players used, and the smaller samples.
pub const BUNDLED: &[&str] = &[
    "autologin",
    "buffbot",
    "chat-filter",
    "follow",
    "id-notices",
    "looter",
    "navigator",
    "tank",
    "trackers",
    "xp-meter",
];

/// The kinds of file a carried plugin may hold, by extension: its manifest, its scripts, its data
/// (routes, loot and settings profiles, metas, tables, pictures) and its notes.
pub const KINDS: &[&str] = &[
    "toml", "lua", "txt", "md", "nav", "utl", "usd", "met", "json", "png",
];

/// Whether a carried file is of a kind a plugin is made of.
#[must_use]
pub fn allowed_kind(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| KINDS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// The published paths of a listing of tracked files (`/`-separated, below the workspace), with
/// the files they come from: every file of every [`BUNDLED`] folder, sorted.
///
/// # Errors
/// A folder that is missing or holds no manifest, or a file of a kind no plugin is made of.
pub fn from_listing(ws: &Path, tracked: &[String]) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for folder in BUNDLED {
        let prefix = format!("{SOURCE}/{folder}/");
        let mine: Vec<&String> = tracked.iter().filter(|p| p.starts_with(&prefix)).collect();
        if !mine
            .iter()
            .any(|p| p.as_str() == format!("{prefix}plugin.toml"))
        {
            problems.push(format!(
                "{SOURCE}/{folder}: a carried plugin with no plugin.toml tracked"
            ));
        }
        for p in mine {
            let rest = &p[prefix.len()..];
            if !allowed_kind(rest) {
                problems.push(format!(
                    "{p}: not a kind of file a carried plugin may hold ({})",
                    KINDS.join(", ")
                ));
                continue;
            }
            out.push((format!("{PUBLISHED}/{folder}/{rest}"), ws.join(p)));
        }
    }
    if !problems.is_empty() {
        return Err(format!("the carried plugins:\n  {}", problems.join("\n  ")));
    }
    out.sort();
    Ok(out)
}

/// Every file of the carried plugins, as git tracks them: published path and file. None while
/// releases carry no plugins ([`SHIPPED`]).
///
/// # Errors
/// As [`carried`].
pub fn files(ws: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    if !SHIPPED {
        return Ok(Vec::new());
    }
    carried(ws)
}

/// Every file of [`BUNDLED`]'s folders as git tracks them, as a release that carries the plugins
/// would carry them: published path and file.
///
/// # Errors
/// As [`from_listing`], or git could not list the files.
pub fn carried(ws: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let listing = super::git(ws, &["ls-files", "--", SOURCE])?;
    let tracked: Vec<String> = listing
        .lines()
        .map(|l| l.trim().replace('\\', "/"))
        .filter(|l| !l.is_empty())
        .collect();
    from_listing(ws, &tracked)
}

/// The published paths alone.
///
/// # Errors
/// As [`files`].
pub fn paths(ws: &Path) -> Result<Vec<String>, String> {
    Ok(files(ws)?.into_iter().map(|(p, _)| p).collect())
}
