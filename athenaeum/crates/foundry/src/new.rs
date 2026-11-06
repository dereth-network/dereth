//! A new world over the world database, with an empty pack to fill.
//!
//! ```text
//! <world>/world.toml                  the world's key, its game and era, its packs, its base
//! <world>/world.lock                  the ids its build gives, kept across builds
//! <world>/fill-<key>/pack.toml        the pack its edits go into, empty
//! <world>/.gitignore                  what playing and building it leaves beside it
//! ```
//!
//! The world names the database by name only (`[base] db = "yalain-world"`), so it lies over
//! the release the build pins; a `version` added by hand keeps it on one release.

use std::path::Path;

use crate::world::DATABASE;

/// What a new world is made of: its key and its pack's folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWorld {
    pub key: String,
    pub pack: String,
}

/// A key made from a folder's name: lowercase letters, digits and hyphens.
#[must_use]
pub fn key_from(name: &str) -> String {
    let mut key = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            key.push(c.to_ascii_lowercase());
        } else if !key.ends_with('-') && !key.is_empty() {
            key.push('-');
        }
    }
    let key = key.trim_end_matches('-').to_string();
    if key.is_empty() {
        "world".into()
    } else {
        key.chars().take(48).collect()
    }
}

/// The files of a new world `key`, by path in its folder.
#[must_use]
pub fn files(key: &str) -> Vec<(String, String)> {
    let pack = format!("fill-{key}");
    vec![
        (
            "world.toml".into(),
            format!(
                "# An Asheron's Call 2 world: its packs over the world database, which Yalain serves.\n\
                 format_version = 1\n\
                 key = \"{key}\"\n\
                 game = \"ac2\"\n\
                 era = \"eor\"\n\
                 packs = [\"{pack}\"]\n\
                 \n\
                 # The database under every pack: the release this build pins (add version = \"vX.Y.Z\" to\n\
                 # keep one). A pack's row replaces the database's row of the same key, and [[remove]]\n\
                 # takes one away.\n\
                 [base]\n\
                 db = \"{DATABASE}\"\n"
            ),
        ),
        (
            "world.lock".into(),
            "# This world's ids, numbered when it is built.\n".into(),
        ),
        (
            format!("{pack}/pack.toml"),
            format!(
                "format_version = 1\n\
                 name = \"{pack}\"\n\
                 version = \"0.1.0\"\n\
                 game = \"ac2\"\n\
                 authors = []\n\
                 license = \"CC0-1.0\"\n\
                 description = \"What this world adds to the world database and changes in it.\"\n"
            ),
        ),
    ]
}

/// A new world's `.gitignore`: the playtest folder (the editor's server, its characters' stores
/// and logs), the build's output and the overlay are made from the world, never part of it.
pub const GITIGNORE: &str = "# Made from the world as it is played and built; never part of it.\n\
                             .playtest/\n\
                             build/\n\
                             overlay/\n";

/// Make a new world in `dir`, which must not hold one already; its key is `key`, else made from
/// the folder's name.
///
/// # Errors
/// The folder holds a world already, or a file will not write.
pub fn new_world(dir: &Path, key: Option<&str>) -> Result<NewWorld, String> {
    if dir.join("world.toml").exists() {
        return Err(format!("{} holds a world already", dir.display()));
    }
    let key = match key {
        Some(k) => key_from(k),
        None => key_from(
            &dir.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        ),
    };
    for (rel, text) in files(&key)
        .into_iter()
        .chain([(".gitignore".to_string(), GITIGNORE.to_string())])
    {
        let p = dir.join(&rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(NewWorld {
        pack: format!("fill-{key}"),
        key,
    })
}
