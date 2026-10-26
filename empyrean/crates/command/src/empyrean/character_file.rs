//! Characters carried from one world to another, in a character file: `playtest-character export`
//! writes one, and `import-character` brings one into an account here. Not ACE.
//! DIVERGE: V459. ACE has no way to move a character between worlds.
//!
//! **Export** writes the offline character, its character record and everything it owns to a new
//! file, as a backup keeps them, with the file's manifest: the world it left (its data overlay's
//! key) and that world's era, the recipe it was made from when a world builder says so, what the
//! checks of a legitimate character found, and every weenie class it and its objects are made of,
//! each with its class name here and, for one of the world's own weenies, the symbol the world
//! builder gave it.
//!
//! **Import** is the console's alone, and off unless the operator turns on
//! `allow_character_import`: a playtest character was made, not earned. It reads a copy of the
//! file, so the operator's file is never changed, and refuses:
//! - a file of another era, or one a newer Empyrean wrote;
//! - a class this world lacks, or has under another name (the object would become another thing);
//! - a name that is taken, unless a new one is given;
//! - an account that does not exist, is logged in or has no character slot free;
//! - a character the checks find wrong: its experience, credits, ranks, innate split and
//!   augmentations are checked as `verify-xp` and `verify-skill-credits` check a player (nothing
//!   skipped), its level must be one this world allows, and everything it wields must be within its
//!   base values' reach. The objects' own properties are taken as they are.
//!
//! The character and each object are given new object ids from this world's ranges (an object's
//! container, wielder and bond follow them; what named another world's objects and characters, an
//! allegiance, a house, friends, is taken out), and a house deed is left behind, as a copied
//! character leaves it. A place this world's data files lack (another world's dungeon) is
//! forgotten, so the character starts where a character without one starts. An administrator's
//! character imported into a player's account is made a player's, as logging in makes one when
//! permissions follow the account. Everything is written in one transaction.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use dereth_progression::order::{ExportNote, EXPORT_VERSION};
use dereth_progression::Build;
use empyrean_common::config_manager::ConfigManager;
use empyrean_entity::enums::{
    AccessLevel, ChatMessageType, CloakStatus, PropertyBool, PropertyInt, PropertyString,
    WeenieClassName, WeenieType,
};
use empyrean_entity::Biota;
use empyrean_net::SessionId;
use empyrean_store::adapter::BiotaConverter;
use empyrean_store::character_file::{self as file, Class, Manifest, FORMAT};
use empyrean_store::models::shard::Character;
use empyrean_store::{ShardDatabase, SqliteShard};
use empyrean_world::managers::{guid_manager, player_manager};
use empyrean_world::World;

use super::playtest_character::{self as playtest, gear};
use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::CommandHandlerInfo;
use crate::handlers::command_handler_helper::{write_output_error, write_output_info};

/// The `import-character` row (a new name, added after ACE's rows).
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let (handler, handler_name) = crate::handler!(handle_import);
    vec![CommandHandlerInfo {
        handler,
        handler_name,
        attribute: CommandHandlerAttribute::with_description(
            "import-character",
            AccessLevel::Admin,
            CommandHandlerFlag::ConsoleInvoke,
            "Brings a character exported from another Empyrean world into an account, when allow_character_import is on.",
            "<file.empyrean-character> <account> [<new name>]",
        ),
    }]
}

/// Whether the operator lets characters be imported.
fn allowed() -> bool {
    ConfigManager::try_config().is_some_and(|c| c.server.accounts.allow_character_import)
}

/// A character's name as it is kept: without the `+` an administrator's is shown with.
fn bare(name: &str) -> &str {
    name.trim_start_matches('+')
}

/// Write the offline character `name` to the new file `out`, with what the world builder's
/// `note` adds to its manifest. Answers what the checks found, when they found anything.
///
/// # Errors
/// A character that is online or has saves pending, a file already there, or one that would not
/// write (nothing is left behind then).
pub fn export(
    w: &World,
    name: &str,
    out: &Path,
    note: Option<&Path>,
) -> Result<Option<String>, String> {
    let note = match note {
        Some(p) => {
            let n: ExportNote =
                serde_json::from_slice(&std::fs::read(p).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if n.version != EXPORT_VERSION {
                return Err(format!("unsupported export note version {}", n.version));
            }
            n
        }
        None => ExportNote::default(),
    };
    let (id, player) = playtest::offline(w, name)?;
    let found = playtest::verify(w, &player).err();
    {
        let mut db = w.shard.base_database();
        playtest::backup(&mut **db, id, out)?;
    }
    let written = (|| -> Result<(), String> {
        let mut copy = SqliteShard::open(out).map_err(|e| e.to_string())?;
        let classes: BTreeSet<u32> = playtest::rows(&mut copy, id)?
            .iter()
            .map(|b| b.weenie_class_id)
            .collect();
        let manifest = Manifest {
            format: FORMAT,
            world: w.dats.portal_dat().overlay_world_key().unwrap_or_default(),
            era: w.era.id.name().to_owned(),
            name: bare(
                &player
                    .get_property(PropertyString::Name)
                    .unwrap_or_default(),
            )
            .to_owned(),
            id,
            recipe: note.recipe,
            hash: note.hash,
            checks: found.clone().unwrap_or_default(),
            made_by: format!("Empyrean {}", env!("CARGO_PKG_VERSION")),
            classes: classes
                .into_iter()
                .map(|class| Class {
                    class,
                    name: class_name(w, class).unwrap_or_default(),
                    symbol: note.symbols.get(&class).cloned(),
                })
                .collect(),
        };
        file::write_manifest(&copy, &manifest).map_err(|e| e.to_string())?;
        file::as_one_file(&copy).map_err(|e| e.to_string())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(out);
        return Err(e);
    }
    Ok(found)
}

/// The class name this world's weenie `class` has.
fn class_name(w: &World, class: u32) -> Option<String> {
    w.content.get_cached_weenie(class)?.class_name.clone()
}

/// What a character file holds, as read.
struct Read {
    manifest: Manifest,
    player: Biota,
    items: Vec<Biota>,
    record: Character,
}

/// A scratch folder of this process's own, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("empyrean-import-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(Self(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The character in the file `path`, read from a copy of it.
fn read(path: &Path) -> Result<Read, String> {
    if !path.is_file() {
        return Err(format!("there is no file {}", path.display()));
    }
    let scratch = Scratch::new()?;
    let copy = scratch.0.join("character.sqlite");
    std::fs::copy(path, &copy).map_err(|e| e.to_string())?;
    let read = (|| -> Result<Read, String> {
        let policy = empyrean_store::upgrade::UpgradePolicy {
            backup_dir: Some(scratch.0.clone()),
            ..Default::default()
        };
        let mut shard = SqliteShard::open_with(&copy, &policy)
            .map_err(|e| format!("the file does not read as a character file: {e}"))?;
        let manifest = file::read_manifest(&shard)
            .map_err(|e| e.to_string())?
            .ok_or("the file is not a character file")?;
        if manifest.format > FORMAT {
            return Err(format!(
                "the file was written by a newer Empyrean ({})",
                manifest.made_by
            ));
        }
        let mut rows = playtest::rows(&mut shard, manifest.id)?
            .iter()
            .map(|b| BiotaConverter::convert_to_entity_biota(b, false))
            .collect::<Vec<_>>();
        let record = shard
            .get_character(manifest.id)
            .ok_or("the file holds no character record")?;
        let player = rows.remove(0);
        Ok(Read {
            manifest,
            player,
            items: rows,
            record,
        })
    })();
    drop(scratch);
    read
}

/// What an import made: the character's name and object id here, and how many objects it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub name: String,
    pub id: u32,
    pub objects: usize,
}

/// Bring the character in the file `path` into account `account` here, named `new_name` or as it
/// was.
///
/// # Errors
/// Why it was not brought in; nothing is changed then.
pub fn import(
    w: &mut World,
    path: &Path,
    account: &str,
    new_name: Option<&str>,
) -> Result<Imported, String> {
    if !allowed() {
        return Err(
            "importing characters is off; set allow_character_import = true under [server.accounts] in empyrean.toml to allow it"
                .into(),
        );
    }
    let Read {
        manifest,
        mut player,
        items,
        mut record,
    } = read(path)?;

    // The world.
    let era = w.era.id.name();
    if manifest.era != era {
        return Err(format!(
            "the character is of the {} era, and this world plays {era}",
            manifest.era
        ));
    }

    // The account.
    let account = w
        .auth
        .lock()
        .get_account_by_name(account)
        .ok_or_else(|| format!("there is no account {account}"))?;
    if w.net.find_by_account(&account.account_name).is_some() {
        return Err("the account is logged in".into());
    }
    let slots = player_manager::property_manager_get_long(w, "max_chars_per_account");
    let held = w
        .shard
        .base_database()
        .get_characters(account.account_id, false)
        .len();
    if i64::try_from(held).unwrap_or(i64::MAX) >= slots {
        return Err(format!(
            "the account has no character slot free ({held} of {slots})"
        ));
    }

    // The name.
    let name = bare(new_name.unwrap_or(&record.name)).trim().to_owned();
    // Creation's own limit: a name of 1 to 32 characters.
    if name.is_empty() || name.chars().count() > 32 {
        return Err("the name is not one a character can have".into());
    }
    if player_manager::find_by_name(w, &name).0.is_some()
        || !w.shard.base_database().is_character_name_available(&name)
    {
        return Err(if new_name.is_some() {
            format!("the name {name} is taken")
        } else {
            format!("the name {name} is taken; give the character a new one")
        });
    }

    // What it is made of.
    let deed = u32::from(WeenieClassName::W_DEED_CLASS.0);
    let items: Vec<Biota> = items
        .into_iter()
        .filter(|b| b.weenie_class_id != deed)
        .collect();
    for b in std::iter::once(&player).chain(&items) {
        let class = b.weenie_class_id;
        let Some(here) = w.content.get_cached_weenie(class) else {
            return Err(format!("this world has no class {class}"));
        };
        let theirs = manifest
            .classes
            .iter()
            .find(|c| c.class == class)
            .filter(|c| !c.name.is_empty());
        if let Some(c) = theirs {
            let ours = here.class_name.clone().unwrap_or_default();
            if !ours.eq_ignore_ascii_case(&c.name) {
                return Err(format!(
                    "class {class} is {ours} in this world but {} in the file",
                    c.name
                ));
            }
        }
    }

    // The checks.
    let max_level = empyrean_world::world_objects::player_xp::get_max_level(w);
    let level = player
        .get_property(PropertyInt::Level)
        .and_then(|l| u32::try_from(l).ok())
        .unwrap_or(0);
    if !(1..=max_level).contains(&level) {
        return Err(format!(
            "it is level {level}, and this world's levels are 1 to {max_level}"
        ));
    }
    if let Some(allowed) = w.era.heritages {
        let heritage = player
            .get_property(PropertyInt::HeritageGroup)
            .and_then(|h| u32::try_from(h).ok())
            .unwrap_or(0);
        if !allowed.contains(&heritage) {
            return Err("its heritage is not one of this world's era".into());
        }
    }
    playtest::verify_with(w, &player, Some(&record))
        .map_err(|e| format!("the checks find it wrong: {e}"))?;
    let sheet = playtest::sheet_with(w, &player, Some(&record));
    playtest::with_tables(w, |t| -> Result<(), String> {
        let build = Build::of_sheet(&t, &sheet)?;
        let who = gear::Wielder {
            build: &build,
            tables: t,
        };
        for item in items.iter().filter(|b| {
            b.get_property(PropertyInt::CurrentWieldedLocation)
                .is_some()
        }) {
            if !who.meets(item) {
                return Err(format!(
                    "it wields {}, which needs {}, more than it has",
                    gear::name(item),
                    gear::wield_text(&t, item)
                ));
            }
        }
        Ok(())
    })?;

    // Into this world.
    forget_unknown_places(w, &mut player);
    let staff =
        AccessLevel(i32::try_from(account.access_level).unwrap_or(0)) > AccessLevel::Advocate;
    if !staff {
        as_players(&mut player, &mut record);
    }
    let player_id = guid_manager::new_player_guid(w).full();
    let mut items = items;
    let ids = file::renumber(
        &mut player,
        &mut items,
        &mut record,
        (player_id, account.account_id, &name),
        || guid_manager::new_dynamic_guid(w).full(),
    );
    {
        let mut db = w.shard.base_database();
        if let Some(taken) = ids.values().find(|id| db.get_biota(**id, false).is_some()) {
            return Err(format!("object id 0x{taken:08X} is already in use here"));
        }
        db.begin_batch().map_err(|e| e.to_string())?;
        let saved = db.save_biota(&mut player, false)
            && items.iter_mut().all(|b| db.save_biota(b, false))
            && db.save_character(&record);
        if !saved {
            db.rollback_batch();
            return Err("the character would not save".into());
        }
        db.commit_batch().map_err(|e| e.to_string())?;
    }
    let objects = items.len();
    player_manager::add_offline_player_biota(w, player);
    Ok(Imported {
        name,
        id: player_id,
        objects,
    })
}

/// Forget each place of `player`'s that this world's data files lack: a cell of another world's
/// own dungeon, say. Without a location a character starts where its creation started it.
fn forget_unknown_places(w: &World, player: &mut Biota) {
    let cells = w.dats.cell_dat();
    let known = |cell: u32| {
        let record = if cell & 0xFFFF >= 0x100 {
            cell
        } else {
            cell | 0xFFFF
        };
        cells.contains_file(record)
    };
    if let Some(places) = player.properties_position.as_mut() {
        let gone: Vec<_> = places
            .iter()
            .filter(|(_, p)| !known(p.obj_cell_id))
            .map(|(k, _)| *k)
            .collect();
        for k in gone {
            places.remove(&k);
        }
    }
}

/// Make an administrator's or sentinel's character a player's, as logging in makes one for an
/// account below them when permissions follow the account (whether it is cloaked is its body's,
/// which login makes afresh).
fn as_players(player: &mut Biota, record: &mut Character) {
    if player.weenie_type != WeenieType::Admin && player.weenie_type != WeenieType::Sentinel {
        return;
    }
    record.is_plussed = false;
    player.weenie_type = WeenieType::Creature;
    player.set_property(PropertyInt::CloakStatus, CloakStatus::Undef.0);
    player.set_property(PropertyBool::Attackable, true);
    player.set_property(PropertyBool::DamagedByCollisions, true);
    player.try_remove_property(PropertyInt::AdvocateLevel);
    player.try_remove_property(PropertyInt::ChannelsActive);
    player.try_remove_property(PropertyInt::ChannelsAllowed);
    player.set_property(PropertyBool::Invincible, false);
    player.set_property(PropertyBool::IgnoreHouseBarriers, false);
    player.set_property(PropertyBool::IgnorePortalRestrictions, false);
    player.set_property(PropertyBool::SafeSpellComponents, false);
    player.set_property(PropertyBool::ReportCollisions, true);
}

/// `import-character <file> <account> [<new name>]`: the console's alone, since only it can name
/// a file on the server's computer.
pub fn handle_import(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if session.is_some() {
        write_output_error(
            w,
            session,
            "Characters can only be imported on the console.",
            ChatMessageType::Broadcast,
        );
        return;
    }
    let result = match parameters {
        [file, account] => import(w, Path::new(file), account, None),
        [file, account, name] => import(w, Path::new(file), account, Some(name)),
        _ => Err("use import-character <file.empyrean-character> <account> [<new name>]".into()),
    };
    match result {
        Ok(made) => write_output_info(
            w,
            None,
            &format!(
                "Imported {} with {} objects, as 0x{:08X}.",
                made.name, made.objects, made.id
            ),
            ChatMessageType::Broadcast,
        ),
        Err(e) => write_output_error(
            w,
            None,
            &format!("Character not imported: {e}"),
            ChatMessageType::Broadcast,
        ),
    }
}
