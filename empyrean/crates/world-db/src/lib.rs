//! An AC1 world as rows: ACE's world tables written as TOML packs, one file per stable key, and
//! read back into the content crate's world.
//!
//! That world (`empyrean_content::WorldContent`) is what a pack is made from.
//!
//! **Depends on** the content crate (`empyrean-content`) for the world's models and the pack, the
//! shared pack format (`dereth-pack`) for reading rows and folding layers, the
//! entity crate (`empyrean-entity`) for ACE's names of property types, attributes, skills, body
//! parts, positions and emotes, `empyrean-common` for times and eras, `toml` to read the files and
//! `serde`/`serde_json` for the source loot tables, which are serde models. **Used by** the
//! importer (`empyrean-import world`), which checks and builds a world of layers from its rows.
//!
//! **Must never** write a dump's own ids into a row: they are numbered by rule when the rows are
//! read, so a row says only what a world holds and the same rows always make the same pack.
//!
//! A world is files under a folder per kind ([`files`]):
//!
//! | Folder | One file per | Rows |
//! |---|---|---|
//! | `weenie/` | class (`00509.toml`) | `[[weenie]]`: its class, name and type; its properties by ACE's names (`[weenie.int]`, `[weenie.did]`, ...); its lists (`[[weenie.create_list]]`, `[[weenie.emote]]` with `[[weenie.emote.action]]`, ...) |
//! | `landblock/` | landblock (`A9B4.toml`) | `[[placement]]`: guid, class, cell, spot, turn, the guids it links |
//! | `encounter/` | column of landblocks (`A9xx.toml`) | `[[encounter]]`: landblock, cell, class |
//! | `quest/`, `event/`, `poi/` | name | `[[quest]]` ... |
//! | `spell/`, `recipe/`, `recipe_vital/` | id | |
//! | `cook_book/` | source and target | `[[cook_book]]`: recipe, source, target |
//! | `treasure_death/`, `grouped_treasure/`, `treasure_wielded/` | treasure type | a wielded type's entries in their order (`[[treasure_wielded.entry]]`) |
//! | `treasure_*.toml`, `loot_policy.toml`, `version.toml` | the table | |
//!
//! A field at its default is left out, and so is `last_modified` when it's the world's stamp
//! ([`files::META`]). Ids a dump numbers freely aren't written: [`canon::number`] puts a world in
//! its one order and numbers them by rule, so the same rows always make the same pack.
//!
//! Rows are read as the shared pack format reads them ([`layers`]): each sets one item, and a
//! world of patch layers folds field by field over its base, at any layer ([`layers::Folding`]),
//! each layer checked for what its fold finds and the references to nothing it makes ([`refs`]).

pub mod canon;
pub mod checkpoint;
pub mod compare;
pub mod files;
pub mod kinds;
pub mod layers;
pub mod names;
pub mod refs;
pub mod row;
pub mod value;
pub mod world;

pub use canon::{from_pack, number};
pub use files::{read, read_dir, write, write_dir};

/// The rows `files` hold, read and numbered: the world a pack is made from.
pub fn world(
    files: &std::collections::BTreeMap<String, String>,
) -> value::R<empyrean_content::WorldContent> {
    let mut c = read(files)?;
    number(&mut c);
    Ok(c)
}

#[cfg(test)]
mod tests;
