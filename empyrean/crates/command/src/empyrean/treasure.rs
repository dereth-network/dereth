//! `import-treasure` and `roll-treasure`: a world builder's loot tables brought into a running
//! world, and one drawn without anything made in it. Not ACE (V460).
//!
//! - `import-treasure <death|wielded> <id|all>` imports the content folder's files of a loot
//!   table (`sql/treasure_death/` or `sql/treasure_wielded/`, a file named by the table's id in
//!   five figures at least, `00037.sql`, or starting with it and a space), as `import-sql` imports
//!   a weenie's, and forgets what the world had read of that kind of table, so the next creature
//!   to die or to be made draws from the new rows. It says `Imported <file>` for each file.
//!   ACE's `import-sql` takes no loot tables, and ACE keeps a death treasure it has read until it
//!   stops.
//! - `roll-treasure <id> [times]` (the console's alone) draws loot table `id` as a generator's
//!   treasure entry draws it, the death treasure of that id, else the wielded treasure, `times`
//!   times (1 when not given, 1000 at most), and says what each draw made, a line each: each
//!   thing's class, how many when more than one, and its name. Nothing it makes enters the world,
//!   and their ids are given back. A death treasure is drawn as the end of retail draws it for a
//!   creature.

use std::panic::{catch_unwind, AssertUnwindSafe};

use empyrean_entity::enums::{AccessLevel, ChatMessageType, PropertyString};
use empyrean_net::SessionId;
use empyrean_world::factories::loot_generation_factory;
use empyrean_world::managers::{guid_manager, property_manager};
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::world_object_equipment;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::CommandHandlerInfo;
use crate::handlers::command_handler_helper::{write_output_error, write_output_info};
use crate::handlers::developer_content_commands::import_sql;

/// The most draws one `roll-treasure` makes.
pub const MOST_ROLLS: u32 = 1000;

/// The `import-treasure` and `roll-treasure` rows (new names, added after ACE's rows).
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let (import, import_name) = crate::handler!(handle_import_treasure);
    let (roll, roll_name) = crate::handler!(handle_roll_treasure);
    vec![
        CommandHandlerInfo {
            handler: import,
            handler_name: import_name,
            attribute: CommandHandlerAttribute::with_count(
                "import-treasure",
                AccessLevel::Developer,
                CommandHandlerFlag::None,
                2,
                "Imports a loot table's sql from the Content folder.",
                "<death|wielded> <id>\n<id> - the table's id, or 'all' for every file of that kind",
            ),
        },
        CommandHandlerInfo {
            handler: roll,
            handler_name: roll_name,
            attribute: CommandHandlerAttribute::with_count(
                "roll-treasure",
                AccessLevel::Developer,
                CommandHandlerFlag::ConsoleInvoke,
                1,
                "Draws a loot table and says what it made, without making anything in the world.",
                "<id> [times]\n<id> - a death treasure's id, else a wielded treasure's\n[times] - how many draws, 1 to 1000",
            ),
        },
    ]
}

/// The kind of table a word names: its folder in the content folder's `sql`.
fn folder_of(kind: &str) -> Option<&'static str> {
    match kind.to_ascii_lowercase().as_str() {
        "death" => Some("treasure_death"),
        "wielded" => Some("treasure_wielded"),
        _ => None,
    }
}

/// Whether `file` (a file's name) is one of table `id`'s, `None` taking every file: `00037.sql`,
/// or `00037 ...sql`.
#[must_use]
pub fn is_table_file(file: &str, id: Option<u32>) -> bool {
    let Some(stem) = file
        .strip_suffix(".sql")
        .or_else(|| file.strip_suffix(".SQL"))
    else {
        return false;
    };
    let digits = stem.split(' ').next().unwrap_or_default();
    match id {
        None => true,
        Some(id) => digits == format!("{id:05}"),
    }
}

/// The content folder's files of `folder` (`treasure_death`, ...) for table `id` (every one for
/// `None`), in order by name: the folder, and each file's name.
fn table_files(w: &mut World, folder: &str, id: Option<u32>) -> (std::path::PathBuf, Vec<String>) {
    let mut content = property_manager::get_string(w, "content_folder", "", true).item;
    if content.starts_with('.') {
        if let Ok(cwd) = std::env::current_dir() {
            content = cwd.join(content).to_string_lossy().into_owned();
        }
    }
    let dir = std::path::Path::new(&content).join("sql").join(folder);
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|f| is_table_file(f, id))
                .collect()
        })
        .unwrap_or_default();
    files.sort_by_key(|f| f.to_ascii_lowercase());
    (dir, files)
}

/// `import-treasure <death|wielded> <id|all>`: each of the table's files imported, and what the
/// world had read of that kind of table forgotten.
pub fn handle_import_treasure(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let say = |w: &mut World, s: &str| write_output_info(w, session, s, ChatMessageType::Broadcast);
    let Some(folder) = parameters.first().and_then(|k| folder_of(k)) else {
        say(w, "Unknown loot table kind: give death or wielded.");
        return;
    };
    let which = parameters.get(1).map(String::as_str).unwrap_or_default();
    let id = if which.eq_ignore_ascii_case("all") {
        None
    } else {
        match which.parse::<u32>() {
            Ok(id) => Some(id),
            Err(_) => {
                say(w, &format!("{which} is not a loot table's id."));
                return;
            }
        }
    };
    let (dir, files) = table_files(w, folder, id);
    if files.is_empty() {
        say(
            w,
            &format!(
                "Couldn't find {}",
                dir.join(id.map_or_else(|| "*.sql".to_owned(), |id| format!("{id:05}*.sql")))
                    .display()
            ),
        );
        return;
    }
    for file in files {
        let path = dir.join(&file);
        let result = catch_unwind(AssertUnwindSafe(|| {
            import_sql(w, &path.to_string_lossy());
        }));
        match result {
            Ok(()) => say(w, &format!("Imported {file}")),
            Err(e) => {
                let why = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_default();
                write_output_error(
                    w,
                    session,
                    &format!("Failed to import {file}: {why}"),
                    ChatMessageType::Broadcast,
                );
                return;
            }
        }
    }
    if folder == "treasure_death" {
        match id {
            Some(id) => w.content.clear_cached_death_treasure(id),
            None => w.content.clear_death_treasure_cache(),
        }
    } else {
        w.content.clear_wielded_treasure_cache();
    }
}

/// What a draw made, as `roll-treasure` says it: each thing's class, how many when more than one,
/// and its name; `nothing` for none.
#[must_use]
pub fn made_words(made: &[(u32, i32, String)]) -> String {
    if made.is_empty() {
        return "nothing".into();
    }
    made.iter()
        .map(|(class, n, name)| {
            if *n > 1 {
                format!("{class} x{n} {name}")
            } else {
                format!("{class} {name}")
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Each thing of `made` as `roll-treasure` says it, its id given back.
fn given_back(w: &mut World, made: Vec<WorldObject>) -> Vec<(u32, i32, String)> {
    made.into_iter()
        .map(|o| {
            let said = (
                o.weenie_class_id(),
                o.stack_size().unwrap_or(1),
                o.get_property(PropertyString::Name).unwrap_or_default(),
            );
            guid_manager::recycle_dynamic_guid(w, o.guid);
            said
        })
        .collect()
}

/// `roll-treasure <id> [times]`: the table drawn `times` times, a line for each draw.
pub fn handle_roll_treasure(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let say = |w: &mut World, s: &str| write_output_info(w, session, s, ChatMessageType::Broadcast);
    let Some(id) = parameters.first().and_then(|p| p.parse::<u32>().ok()) else {
        say(w, "Give a loot table's id: roll-treasure <id> [times].");
        return;
    };
    let times = match parameters.get(1).map(|p| p.parse::<u32>()) {
        None => 1,
        Some(Ok(n)) if (1..=MOST_ROLLS).contains(&n) => n,
        Some(_) => {
            say(w, &format!("Draw a table 1 to {MOST_ROLLS} times."));
            return;
        }
    };
    if let Some(death) = w.content.get_cached_death_treasure(id) {
        say(
            w,
            &format!(
                "Rolling treasure {id}: death treasure, tier {}, {times} time(s).",
                death.tier
            ),
        );
        for k in 1..=times {
            let made = loot_generation_factory::create_random_loot_objects(w, &death);
            let made = given_back(w, made);
            say(w, &format!("Treasure roll {k}: {}", made_words(&made)));
        }
    } else {
        let items = w.content.get_cached_wielded_treasure(id);
        if items.is_empty() {
            say(
                w,
                &format!("Rolled treasure {id}: there is no loot table {id}."),
            );
            return;
        }
        say(
            w,
            &format!("Rolling treasure {id}: wielded treasure, {times} time(s)."),
        );
        for k in 1..=times {
            let made = world_object_equipment::generate_wielded_treasure_sets(w, &items)
                .unwrap_or_default();
            let made = given_back(w, made);
            say(w, &format!("Treasure roll {k}: {}", made_words(&made)));
        }
    }
    say(w, &format!("Rolled treasure {id}: {times} time(s)."));
}
