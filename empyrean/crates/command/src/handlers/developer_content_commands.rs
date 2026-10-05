// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/DeveloperContentCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/DeveloperContentCommands.cs`.
//!
//! - **The world database.** ACE writes content to MySQL: `ImportSQL` runs a file of SQL with
//!   `ExecuteSqlRaw`, and `SyncInstances`/`SyncEncounters` run a `DELETE` for an emptied landblock.
//!   Here every such write goes to the content overlay in front of `world.pack`
//!   (`DatabaseManager.World.overlay()`; see `empyrean_content::overlay`): the same SQL
//!   runs with MySQL's semantics, and the overlay journals it so `empyrean-import --overlay` publishes it.
//!   DIVERGE (arch): without a configured overlay (`Server.WorldOverlayPath`) the write throws, as
//!   ACE's would with no database; a file that fails part-way changes nothing (the overlay is
//!   atomic per file, MySQL keeps the statements before the failing one).
//! - **Files.** The SQL and JSON files land where ACE writes them: under the `content_folder`
//!   server property (`sql/weenies/`, `json/landblocks/`, …).
//!   DIVERGE (arch): `DirectoryInfo.GetFiles` returns files in the file system's order; here they
//!   are sorted by path, ordinal ignoring case (NTFS's order), so every platform agrees.
//!   DIVERGE (arch): SQL written from JSON (`json2sql_*`) has no `/* label */` comments and uses
//!   `\n` line ends (empyrean-content's writer), and its `LastModified` stamps take the overlay's
//!   clock rather than `DateTime.UtcNow`.
//! - **The static writers.** ACE keeps one `WeenieSQLWriter`, `LandblockInstanceWriter`, … per
//!   process, whose name dictionaries are read from the database once, on first use (so labels
//!   miss content added later). They are per world thread here (`thread_local!`), which is the
//!   same thing for the one world thread and keeps test worlds apart.

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use empyrean_common::dotnet::numerics::{Quaternion, Vector2, Vector3};
use empyrean_common::dotnet::{format, to_string, CsCast, DotNetDateTime, DotNetDict};
use empyrean_common::extensions::string_extensions;
use empyrean_content::export::json as export_json;
use empyrean_content::export::sql::{
    CookBookSQLWriter, EncounterSQLWriter, EventSQLWriter, LandblockInstanceWriter, QuestSQLWriter,
    RecipeSQLWriter, SQLWriter, SpellSQLWriter, SqlOut, WeenieSQLWriter,
};
use empyrean_content::import::json::{json_to_sql_file, JsonKind};
use empyrean_content::models::world::{
    CookBook, Encounter, Event, LandblockInstance, LandblockInstanceLink, Quest, Weenie,
};
use empyrean_entity::enums::{
    AccessLevel, ChatMessageType, CreatureType, GameEventState, ItemType, PropertyString,
    WeenieError, WeenieType,
};
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_world::dispatch;
use empyrean_world::entity::{landblock, position_extensions};
use empyrean_world::managers::{event_manager, landblock_manager, property_manager};
use empyrean_world::network::game_messages::game_message::enqueue_send;
use empyrean_world::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::player_inventory::{self, SearchLocations};
use empyrean_world::world_objects::world_object::{self, CtorEnv};
use empyrean_world::world_objects::{
    player_networking, world_object_decay, world_object_magic, world_object_networking,
    world_object_tick,
};
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_parameter_helpers::dotnet_parse;
use crate::handler;
use crate::handler_common::{location_of, name_of, obj, obj_mut, session_player};
use crate::handlers::command_handler_helper;

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let dev = AccessLevel::Developer;
    let none = CommandHandlerFlag::None;
    let world = CommandHandlerFlag::RequiresWorld;
    let rows: Vec<(CommandHandlerAttribute, NamedHandler)> = vec![
        (
            CommandHandlerAttribute::with_count("import-json", dev, none, 1, "Imports json data from the Content folder", "<type> <wcid>\n<type> - landblock, quest, recipe, spell, weenie (default if not specified)\n<wcid> - filename prefix to search for. can be 'all' to import all files for this content type"),
            handler!(handle_import_json),
        ),
        (
            CommandHandlerAttribute::with_count("import-sql-folders", dev, none, 1, "Imports all weenie sql data from the Content folder and all sub-folders", "<wcid>\n<wcid> - wcid prefix to search for. can be 'all' to import everything"),
            handler!(handle_import_sql_folders),
        ),
        (
            CommandHandlerAttribute::with_count("import-sql", dev, none, 1, "Imports sql data from the Content folder", "<type> <wcid>\n<type> - landblock, encounter, quest, recipe, spell, weenie (default if not specified)\n<wcid> - filename prefix to search for. can be 'all' to import all files for this content type"),
            handler!(handle_import_sql),
        ),
        (
            CommandHandlerAttribute::with_count("createinst", dev, world, 1, "Spawns a new wcid or classname as a landblock instance", "<wcid or classname>\n\nTo create a parent/child relationship: /createinst -p <parent guid> -c <wcid or classname>\nTo automatically get the parent guid from the last appraised object: /createinst -p -c <wcid or classname>\n\nTo manually specify a start guid: /createinst <wcid or classname> <start guid>\nStart guids can be in the range 0x000-0xFFF, or they can be prefixed with 0x7<landblock id>"),
            handler!(handle_create_inst),
        ),
        (CommandHandlerAttribute::with_description("removeinst", dev, world, "Removes the last appraised object from the current landblock instances", ""), handler!(handle_remove_inst)),
        (
            CommandHandlerAttribute::with_count("addenc", dev, world, 1, "Spawns a new wcid or classname in the current outdoor cell as an encounter", "<wcid or classname>"),
            handler!(handle_add_encounter),
        ),
        (CommandHandlerAttribute::with_description("removeenc", dev, world, "Removes the last appraised object from the encounters table", ""), handler!(handle_remove_enc)),
        (
            CommandHandlerAttribute::with_count("export-json-folders", dev, none, 1, "Exports content from database to JSON file in a WeenieType/ItemType folder structure", "<wcid>"),
            handler!(handle_export_json_folder),
        ),
        (
            CommandHandlerAttribute::with_count("export-json", dev, none, 1, "Exports content from database to JSON file", "<optional type> <id>\n<optional type> - landblock, quest, recipe, spell, weenie (default if not specified)\n<id> - wcid or content id to export"),
            handler!(handle_export_json),
        ),
        (
            CommandHandlerAttribute::with_count("export-sql-folders", dev, none, 1, "Exports weenie content from database to an SQL file in a WeenieType/ItemType folder structure", "<wcid>"),
            handler!(handle_export_sql_folder),
        ),
        (
            CommandHandlerAttribute::with_count("export-sql", dev, none, 1, "Exports content from database to SQL file", "<optional type> <id>\n<optional type> - landblock, encounter, event, quest, recipe, spell, weenie (default if not specified)\n<id> - wcid or content id to export"),
            handler!(handle_export_sql),
        ),
        (
            CommandHandlerAttribute::with_description("clearcache", dev, none, "Clears the various database caches. This enables live editing of the database information", ""),
            handler!(handle_clear_cache),
        ),
        (
            CommandHandlerAttribute::with_count("nudge", dev, none, 1, "Adjusts the spawn position of a landblock instance", "<dir> <amount>\nDirections: x, y, z, north, south, west, east, northwest, northeast, southwest, southeast, n, s, w, e, nw, ne, sw, se, up, down, here"),
            handler!(handle_nudge),
        ),
        (
            CommandHandlerAttribute::with_count("rotate", dev, none, 1, "Adjusts the rotation of a landblock instance", "<dir>\nDirections: north, south, west, east, northwest, northeast, southwest, southeast, n, s, w, e, nw, ne, sw, se, -or-\n0-360, with 0 being north, and 90 being west"),
            handler!(handle_rotate),
        ),
        (CommandHandlerAttribute::with_count("rotate-x", dev, none, 1, "Adjusts the rotation of a landblock instance along the x-axis", "<degrees>"), handler!(handle_rotate_x)),
        (CommandHandlerAttribute::with_count("rotate-y", dev, none, 1, "Adjusts the rotation of a landblock instance along the y-axis", "<degrees>"), handler!(handle_rotate_y)),
        (CommandHandlerAttribute::with_count("rotate-z", dev, none, 1, "Adjusts the rotation of a landblock instance along the z-axis", "<degrees>"), handler!(handle_rotate_z)),
        (
            CommandHandlerAttribute::with_description("generate-classnames", dev, none, "Generates WeenieClassName.cs from current world database", ""),
            handler!(handle_generate_class_names),
        ),
        (
            CommandHandlerAttribute::with_count("vloc2loc", dev, none, 1, "Output a set of LOCs for a given landblock found in the VLOCS dataset", "<LandblockID>\nExample: @vloc2loc 0x0007\n         @vloc2loc 0xCE95"),
            handler!(handle_vloc_to_loc),
        ),
    ];
    rows.into_iter()
        .map(|(attribute, (handler, handler_name))| CommandHandlerInfo {
            handler,
            handler_name,
            attribute,
        })
        .collect()
}

// ================================================================================ helpers

/// `CommandHandlerHelper.WriteOutputInfo(session, output)`.
fn info(w: &mut World, session: Option<SessionId>, output: &str) {
    command_handler_helper::write_output_info(w, session, output, ChatMessageType::Broadcast);
}

/// `session.Network.EnqueueSend(new GameMessageSystemChat(message, ChatMessageType.Broadcast))`.
fn chat(w: &mut World, session: SessionId, message: &str) {
    enqueue_send(
        w,
        session,
        game_message_system_chat(message, ChatMessageType::Broadcast),
    );
}

fn require(session: Option<SessionId>) -> SessionId {
    session.expect("NullReferenceException: session")
}

/// `session.Player.CurrentLandblock`, which ACE dereferences.
fn current_landblock(w: &World, player: ObjectGuid) -> LandblockId {
    obj(w, player)
        .current_landblock
        .expect("NullReferenceException: CurrentLandblock")
}

/// `Path.DirectorySeparatorChar`.
const SEP: char = std::path::MAIN_SEPARATOR;

/// `Vector3.UnitX`.
const UNIT_X: Vector3 = Vector3::new(1.0, 0.0, 0.0);

/// `Vector3.UnitZ`.
const UNIT_Z: Vector3 = Vector3::new(0.0, 0.0, 1.0);

/// `Regex.Match(s, @"\d+").Value`: the first run of digits.
fn first_digits(s: &str) -> &str {
    let Some(start) = s.find(|c: char| c.is_ascii_digit()) else {
        return "";
    };
    let rest = &s[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    &rest[..end]
}

/// `Regex.Match(s, @"^(\d+)").Groups[1].Value`.
fn leading_digits(s: &str) -> &str {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    &s[..end]
}

/// `ushort.TryParse(Regex.Match(s, @"[0-9A-F]{4}", RegexOptions.IgnoreCase).Value,
/// NumberStyles.HexNumber, CultureInfo.InvariantCulture, out var landblockId)`.
fn landblock_from_name(s: &str) -> Option<u16> {
    let b = s.as_bytes();
    let at =
        (0..b.len().saturating_sub(3)).find(|&i| b[i..i + 4].iter().all(u8::is_ascii_hexdigit))?;
    u16::from_str_radix(&s[at..at + 4], 16).ok()
}

/// `string.Replace(old, new)` (ordinal, every occurrence).
fn replace(s: &str, old: &str, new: &str) -> String {
    s.replace(old, new)
}

/// `folder` with its last path segment named `from` renamed `to` (the content folder's `json`,
/// `sql` or `spawnmaps` level); a folder with no such segment has every `from` replaced.
///
/// Not ACE's (a fix): ACE replaced every occurrence of the word in the
/// whole path, so a content folder whose path held it elsewhere (e.g. under `C:\mysql\`) sent the
/// output to another, possibly missing, folder.
#[must_use]
pub fn swap_segment(folder: &str, from: &str, to: &str) -> String {
    let is_sep = |c: Option<char>| c.is_none_or(|c| c == '/' || c == '\\');
    for (i, _) in folder.rmatch_indices(from) {
        if is_sep(folder[..i].chars().next_back())
            && is_sep(folder[i + from.len()..].chars().next())
        {
            return format!("{}{to}{}", &folder[..i], &folder[i + from.len()..]);
        }
    }
    replace(folder, from, to)
}

/// `System.Numerics.Quaternion.ToString()`.
fn quaternion_to_string(q: Quaternion) -> String {
    format!(
        "{{X:{} Y:{} Z:{} W:{}}}",
        to_string(q.x),
        to_string(q.y),
        to_string(q.z),
        to_string(q.w)
    )
}

/// `float.ToRadians()` (ACE.Server's physics extension): `(float)(Math.PI / 180.0f * angle)`.
fn to_radians(angle: f32) -> f32 {
    CsCast::<f32>::cs_cast(std::f64::consts::PI / f64::from(180.0f32) * f64::from(angle))
}

/// A directory as `DirectoryInfo` sees it.
#[derive(Debug, Clone)]
struct DirectoryInfo {
    /// `FullName`.
    full_name: String,
}

impl DirectoryInfo {
    /// `new DirectoryInfo(path)`: the path made absolute.
    fn new(path: &str) -> Self {
        let p = PathBuf::from(path);
        let full = std::path::absolute(&p).unwrap_or(p);
        Self {
            full_name: full.to_string_lossy().into_owned(),
        }
    }

    /// `Exists`.
    fn exists(&self) -> bool {
        Path::new(&self.full_name).is_dir()
    }

    /// `Create()`.
    fn create(&self) {
        std::fs::create_dir_all(&self.full_name)
            .unwrap_or_else(|e| panic!("IOException: {}: {e}", self.full_name));
    }

    /// `new DirectoryInfo(folder)`, created when missing.
    fn ensure(folder: &str) {
        let di = Self {
            full_name: folder.to_owned(),
        };
        if !di.exists() {
            di.create();
        }
    }

    /// `GetFiles("{prefix}*.{ext}", options)`: `(directory, file name)`, matched ignoring case.
    /// DIVERGE (arch): sorted by path (see the module docs).
    fn get_files(&self, prefix: &str, ext: &str, recurse: bool) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        let mut stack = vec![PathBuf::from(&self.full_name)];
        let prefix = prefix.to_uppercase();
        let suffix = format!(".{}", ext.to_uppercase());
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    if recurse {
                        stack.push(e.path());
                    }
                    continue;
                }
                let name = e.file_name().to_string_lossy().into_owned();
                let upper = name.to_uppercase();
                if upper.starts_with(&prefix)
                    && upper.ends_with(&suffix)
                    && upper.len() >= prefix.len() + suffix.len()
                {
                    // (FileInfo.DirectoryName: no trailing separator)
                    out.push((
                        dir.to_string_lossy()
                            .trim_end_matches(['/', '\\'])
                            .to_owned(),
                        name,
                    ));
                }
            }
        }
        out.sort_by_key(|(d, n)| (d.to_uppercase(), n.to_uppercase()));
        out
    }
}

/// `File.ReadAllText(path)`.
fn read_all_text(path: &str) -> String {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("FileNotFoundException: {path}: {e}"));
    let text = String::from_utf8_lossy(&bytes).into_owned();
    text.strip_prefix('\u{feff}')
        .map(str::to_owned)
        .unwrap_or(text)
}

/// `File.WriteAllText(path, text)`.
fn write_all_text(path: &str, text: &str) {
    std::fs::write(path, text).unwrap_or_else(|e| panic!("IOException: {path}: {e}"));
}

/// `Environment.NewLine`.
fn new_line() -> &'static str {
    if cfg!(windows) {
        "\r\n"
    } else {
        "\n"
    }
}

/// `DateTime.Now`. DIVERGE: the world clock's UTC (the world has no time zone), as elsewhere.
fn date_time_now(w: &World) -> DotNetDateTime {
    w.now.utc
}

/// The clock that stands in for `DateTime.UtcNow` in `json2sql_*`: the overlay's.
fn json_now(w: &World) -> DotNetDateTime {
    w.content
        .overlay()
        .map_or_else(empyrean_content::overlay::default_now, |o| o.now())
}

/// A panic's message (a C# exception's `Message`).
fn panic_message(e: &(dyn std::any::Any + Send)) -> String {
    e.downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default()
}

// ================================================================================ FileType

/// ACE's `DeveloperContentCommands.FileType`.
// ACE: DeveloperContentCommands.FileType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Undefined,
    Encounter,
    LandblockInstance,
    Quest,
    Recipe,
    Spell,
    Weenie,
    Event,
}

/// The type named by one parameter; `param` becomes the other one (ACE sets it on every pass,
/// so a failure leaves the last pass's value).
// ACE: DeveloperContentCommands.GetContentType
#[must_use]
pub fn get_content_type(parameters: &[String], param: &mut String) -> FileType {
    for i in 0..parameters.len() {
        let other_idx = usize::from(i == 0);

        param.clone_from(&parameters[other_idx]);

        let file_type = parameters[i].to_lowercase();

        if file_type.starts_with("landblock") {
            return FileType::LandblockInstance;
        } else if file_type.starts_with("encounter") {
            return FileType::Encounter;
        } else if file_type.starts_with("event") {
            return FileType::Event;
        } else if file_type.starts_with("quest") {
            return FileType::Quest;
        } else if file_type.starts_with("recipe") {
            return FileType::Recipe;
        } else if file_type.starts_with("weenie") {
            return FileType::Weenie;
        } else if file_type.starts_with("spell") {
            return FileType::Spell;
        }
    }
    FileType::Undefined
}

/// The parameter and content type of `import-*`/`export-*`, or `None` (reported) for an unknown
/// type.
fn parse_content_type(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) -> Option<(String, FileType)> {
    let mut param = parameters[0].clone();
    let mut content_type = FileType::Weenie;

    if parameters.len() > 1 {
        content_type = get_content_type(parameters, &mut param);

        if content_type == FileType::Undefined {
            info(
                w,
                session,
                &format!("Unknown content type '{}'", parameters[1]),
            );
            return None;
        }
    }
    Some((param, content_type))
}

// ================================================================================ import-json

// ACE: DeveloperContentCommands.HandleImportJson
pub fn handle_import_json(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some((param, content_type)) = parse_content_type(w, session, parameters) else {
        return;
    };
    match content_type {
        FileType::LandblockInstance => import_json_landblock(w, session, &param),
        FileType::Quest => import_json_quest(w, session, &param),
        FileType::Recipe => import_json_recipe(w, session, &param),
        FileType::Weenie => import_json_weenie(w, session, &param),
        _ => {}
    }
}

/// The content folder's `sub` folder and its files matching `{prefix}*.{ext}`, or `None`
/// (reported) when there are none.
fn find_content_files(
    w: &mut World,
    session: Option<SessionId>,
    sub: &[&str],
    prefix: &str,
    ext: &str,
    recurse: bool,
) -> Option<(String, Vec<(String, String)>)> {
    let di = verify_content_folder(w, session, true);
    if !di.exists() {
        return None;
    }

    let mut folder = format!("{}{SEP}", di.full_name);
    for s in sub {
        folder.push_str(s);
        folder.push(SEP);
    }

    let di = DirectoryInfo {
        full_name: folder.clone(),
    };

    let files = if di.exists() {
        di.get_files(prefix, ext, recurse)
    } else {
        Vec::new()
    };

    if files.is_empty() {
        info(
            w,
            session,
            &format!("Couldn't find {folder}{prefix}*.{ext}"),
        );
        return None;
    }
    Some((folder, files))
}

/// `prefix`, or nothing for `all`.
fn prefix_or_all(id: &str, prefix: String) -> String {
    if id.eq_ignore_ascii_case("all") {
        String::new()
    } else {
        prefix
    }
}

// ACE: DeveloperContentCommands.ImportJsonWeenie
pub fn import_json_weenie(w: &mut World, session: Option<SessionId>, wcid: &str) {
    let prefix = prefix_or_all(wcid, format!("{wcid} - "));
    let Some((json_folder, files)) =
        find_content_files(w, session, &["json", "weenies"], &prefix, "json", false)
    else {
        return;
    };
    for (_, file) in files {
        import_json_weenie_file(w, session, &json_folder, &file);
    }
}

// ACE: DeveloperContentCommands.ImportJsonRecipe
pub fn import_json_recipe(w: &mut World, session: Option<SessionId>, recipe_id: &str) {
    let prefix = prefix_or_all(recipe_id, format!("{recipe_id:0>5} - "));
    let Some((json_folder, files)) =
        find_content_files(w, session, &["json", "recipes"], &prefix, "json", false)
    else {
        return;
    };
    for (_, file) in files {
        import_json_recipe_file(w, session, &json_folder, &file);
    }
}

// ACE: DeveloperContentCommands.ImportJsonLandblock
pub fn import_json_landblock(w: &mut World, session: Option<SessionId>, landblock_id: &str) {
    let prefix = prefix_or_all(landblock_id, landblock_id.to_owned());
    let Some((json_folder, files)) =
        find_content_files(w, session, &["json", "landblocks"], &prefix, "json", false)
    else {
        return;
    };
    for (_, file) in files {
        import_json_landblock_file(w, session, &json_folder, &file);
    }
}

// ACE: DeveloperContentCommands.ImportJsonQuest
pub fn import_json_quest(w: &mut World, session: Option<SessionId>, quest_name: &str) {
    let prefix = prefix_or_all(quest_name, quest_name.to_owned());
    let Some((json_folder, files)) =
        find_content_files(w, session, &["json", "quests"], &prefix, "json", false)
    else {
        return;
    };
    for (_, file) in files {
        import_json_quest_file(w, session, &json_folder, &file);
    }
}

// ================================================================================ import-sql

// ACE: DeveloperContentCommands.HandleImportSQLFolders
pub fn handle_import_sql_folders(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let param = parameters[0].clone();
    import_sql_weenie(w, session, &param, true);
}

// ACE: DeveloperContentCommands.HandleImportSQL
pub fn handle_import_sql(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some((param, content_type)) = parse_content_type(w, session, parameters) else {
        return;
    };
    let result = catch_unwind(AssertUnwindSafe(|| match content_type {
        FileType::LandblockInstance => import_sql_landblock(w, session, &param),
        FileType::Encounter => import_sql_encounter(w, session, &param),
        FileType::Event => import_sql_event(w, session, &param),
        FileType::Quest => import_sql_quest(w, session, &param),
        FileType::Recipe => import_sql_recipe(w, session, &param),
        FileType::Spell => import_sql_spell(w, session, &param),
        FileType::Weenie => import_sql_weenie(w, session, &param, false),
        FileType::Undefined => {}
    }));
    if let Err(e) = result {
        let message = panic_message(e.as_ref());
        command_handler_helper::write_output_error(
            w,
            session,
            &format!("There was an error importing the SQL:\n\n{message}"),
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: DeveloperContentCommands.ImportSQLWeenie
pub fn import_sql_weenie(
    w: &mut World,
    session: Option<SessionId>,
    wcid: &str,
    with_folders: bool,
) {
    let prefix = prefix_or_all(wcid, format!("{wcid:0>5} "));
    let Some((_, files)) = find_content_files(
        w,
        session,
        &["sql", "weenies"],
        &prefix,
        "sql",
        with_folders,
    ) else {
        return;
    };
    for (dir, file) in files {
        import_sql_weenie_file(w, session, &format!("{dir}{SEP}"), &file);
    }
}

// ACE: DeveloperContentCommands.ImportSQLRecipe
pub fn import_sql_recipe(w: &mut World, session: Option<SessionId>, recipe_id: &str) {
    let prefix = prefix_or_all(recipe_id, format!("{recipe_id:0>5} "));
    let Some((sql_folder, files)) =
        find_content_files(w, session, &["sql", "recipes"], &prefix, "sql", false)
    else {
        return;
    };
    for (_, file) in files {
        import_sql_recipe_file(w, session, &sql_folder, &file);
    }
}

// ACE: DeveloperContentCommands.ImportSQLLandblock
pub fn import_sql_landblock(w: &mut World, session: Option<SessionId>, landblock_id: &str) {
    let prefix = prefix_or_all(landblock_id, landblock_id.to_owned());
    let Some((sql_folder, files)) =
        find_content_files(w, session, &["sql", "landblocks"], &prefix, "sql", false)
    else {
        return;
    };
    for (_, file) in files {
        import_sql_landblock_file(w, session, &sql_folder, &file);
    }
}

// ACE: DeveloperContentCommands.ImportSQLEncounter
pub fn import_sql_encounter(w: &mut World, session: Option<SessionId>, landblock_id: &str) {
    let prefix = prefix_or_all(landblock_id, landblock_id.to_owned());
    let Some((sql_folder, files)) =
        find_content_files(w, session, &["sql", "encounters"], &prefix, "sql", false)
    else {
        return;
    };
    for (_, file) in files {
        import_sql_encounter_file(w, session, &sql_folder, &file);
    }
}

// ACE: DeveloperContentCommands.ImportSQLEvent
pub fn import_sql_event(w: &mut World, session: Option<SessionId>, event_name: &str) {
    let prefix = prefix_or_all(event_name, event_name.to_owned());
    let Some((sql_folder, files)) =
        find_content_files(w, session, &["sql", "events"], &prefix, "sql", false)
    else {
        return;
    };
    for (_, file) in files {
        import_sql_event_file(w, session, &sql_folder, &file);
    }
}

// ACE: DeveloperContentCommands.ImportSQLQuest
pub fn import_sql_quest(w: &mut World, session: Option<SessionId>, quest_name: &str) {
    let prefix = prefix_or_all(quest_name, quest_name.to_owned());
    let Some((sql_folder, files)) =
        find_content_files(w, session, &["sql", "quests"], &prefix, "sql", false)
    else {
        return;
    };
    for (_, file) in files {
        import_sql_quest_file(w, session, &sql_folder, &file);
    }
}

// ACE: DeveloperContentCommands.ImportSQLSpell
pub fn import_sql_spell(w: &mut World, session: Option<SessionId>, spell_id: &str) {
    let prefix = prefix_or_all(spell_id, format!("{spell_id:0>5} "));
    let Some((sql_folder, files)) =
        find_content_files(w, session, &["sql", "spells"], &prefix, "sql", false)
    else {
        return;
    };
    for (_, file) in files {
        import_sql_spell_file(w, session, &sql_folder, &file);
    }
}

/// Returns the absolute content folder path, and verifies it exists
// ACE: DeveloperContentCommands.VerifyContentFolder
fn verify_content_folder(
    w: &mut World,
    session: Option<SessionId>,
    show_error: bool,
) -> DirectoryInfo {
    let mut content_folder = property_manager::get_string(w, "content_folder", "", true).item;

    // handle relative path
    if content_folder.starts_with('.') {
        let cwd = format!(
            "{}{SEP}",
            std::env::current_dir()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default()
        );
        content_folder = cwd + &content_folder;
    }

    let di = DirectoryInfo::new(&content_folder);

    if !di.exists() && show_error {
        info(
            w,
            session,
            &format!("Couldn't find content folder: {}", di.full_name),
        );
        info(
            w,
            session,
            "To set your content folder, /modifystring content_folder <path>",
        );
    }
    di
}

/// Converts JSON to SQL, imports to database, and clears the weenie cache
// ACE: DeveloperContentCommands.ImportJsonWeenie
fn import_json_weenie_file(
    w: &mut World,
    session: Option<SessionId>,
    json_folder: &str,
    json_file: &str,
) {
    let Some(wcid) = dotnet_parse::uint_try_parse(first_digits(json_file)) else {
        info(w, session, &format!("Couldn't find wcid from {json_file}"));
        return;
    };

    // convert json -> sql
    let Some(sql_file) = json2sql_weenie(w, session, json_folder, json_file) else {
        return;
    };

    // import sql to db
    let sql_folder = swap_segment(json_folder, "json", "sql");
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear this weenie out of the cache
    w.content.clear_cached_weenie(wcid);
}

// ACE: DeveloperContentCommands.ImportJsonRecipe
fn import_json_recipe_file(
    w: &mut World,
    session: Option<SessionId>,
    json_folder: &str,
    json_file: &str,
) {
    if dotnet_parse::uint_try_parse(first_digits(json_file)).is_none() {
        info(
            w,
            session,
            &format!("Couldn't find recipe id from {json_file}"),
        );
        return;
    }

    // convert json -> sql
    let Some(sql_file) = json2sql_recipe(w, session, json_folder, json_file) else {
        return;
    };

    // import sql to db
    let sql_folder = swap_segment(json_folder, "json", "sql");
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear recipe cache
    w.content.clear_cookbook_cache();
}

// ACE: DeveloperContentCommands.ImportJsonLandblock
fn import_json_landblock_file(
    w: &mut World,
    session: Option<SessionId>,
    json_folder: &str,
    json_file: &str,
) {
    let Some(landblock_id) = landblock_from_name(json_file) else {
        info(
            w,
            session,
            &format!("Couldn't find landblock id from {json_file}"),
        );
        return;
    };

    // convert json -> sql
    let Some(sql_file) = json2sql_landblock(w, session, json_folder, json_file) else {
        return;
    };

    // import sql to db
    let sql_folder = swap_segment(json_folder, "json", "sql");
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear any cached instances for this landblock
    w.content.clear_cached_instances_by_landblock(landblock_id);
}

// ACE: DeveloperContentCommands.ImportJsonQuest
fn import_json_quest_file(
    w: &mut World,
    session: Option<SessionId>,
    json_folder: &str,
    json_file: &str,
) {
    let quest_name = string_extensions::trim_end(json_file, ".json");

    // convert json -> sql
    let Some(sql_file) = json2sql_quest(w, session, json_folder, json_file) else {
        return;
    };

    // import sql to db
    let sql_folder = swap_segment(json_folder, "json", "sql");
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear cached quest
    w.content.clear_cached_quest(&quest_name);
}

/// One `json2sql_*`: load and convert the JSON file (empyrean-content's loaders, converters and
/// writers), then write the SQL into `sql_folder`. `None` (reported) on a failure.
fn json2sql(
    w: &mut World,
    session: Option<SessionId>,
    kind: JsonKind,
    folder: &str,
    json_filename: &str,
    sql_folder: &str,
) -> Option<String> {
    let json_file = format!("{folder}{json_filename}");

    // (RecipeSQLWriter.WeenieNames names a recipe's file)
    let names = (kind == JsonKind::Recipe).then(|| w.content.get_all_weenie_names());
    let converted = match std::fs::read(&json_file) {
        Ok(bytes) => json_to_sql_file(
            kind,
            &String::from_utf8_lossy(&bytes),
            json_now(w),
            names.as_ref(),
        ),
        Err(e) => Err(format!("failed to load: {e}")),
    };
    let converted = match converted {
        Ok(c) => c,
        Err(e) => {
            if e.starts_with("failed to load") {
                info(w, session, &format!("Failed to load {json_file}"));
            } else {
                // (Console.WriteLine(e))
                log::info!("{e}");
                info(w, session, &format!("Failed to convert {json_file}"));
            }
            return None;
        }
    };
    for m in &converted.messages {
        info(w, session, m);
    }

    // output to sql
    DirectoryInfo::ensure(sql_folder);

    let sql_filename = converted
        .file_name
        .unwrap_or_else(|| replace(json_filename, ".json", ".sql"));
    if let Err(e) = std::fs::write(
        format!("{sql_folder}{sql_filename}"),
        converted.sql.as_bytes(),
    ) {
        // (Console.WriteLine(e))
        log::info!("{e}");
        info(w, session, &format!("Failed to convert {json_file}"));
        return None;
    }

    info(
        w,
        session,
        &format!("Converted {json_filename} to {sql_filename}"),
    );

    Some(sql_filename)
}

/// Converts a json file to sql file
// ACE: DeveloperContentCommands.json2sql_weenie
pub fn json2sql_weenie(
    w: &mut World,
    session: Option<SessionId>,
    folder: &str,
    json_filename: &str,
) -> Option<String> {
    json2sql(
        w,
        session,
        JsonKind::Weenie,
        folder,
        json_filename,
        &swap_segment(folder, "json", "sql"),
    )
}

// ACE: DeveloperContentCommands.json2sql_recipe
pub fn json2sql_recipe(
    w: &mut World,
    session: Option<SessionId>,
    folder: &str,
    json_filename: &str,
) -> Option<String> {
    json2sql(
        w,
        session,
        JsonKind::Recipe,
        folder,
        json_filename,
        &swap_segment(folder, "json", "sql"),
    )
}

// ACE: DeveloperContentCommands.json2sql_landblock
pub fn json2sql_landblock(
    w: &mut World,
    session: Option<SessionId>,
    folder: &str,
    json_filename: &str,
) -> Option<String> {
    let sql_folder = swap_segment(
        &swap_segment(folder, "spawnmaps", "landblock_instances"),
        "json",
        "sql",
    );
    json2sql(
        w,
        session,
        JsonKind::Landblock,
        folder,
        json_filename,
        &sql_folder,
    )
}

// ACE: DeveloperContentCommands.json2sql_quest
pub fn json2sql_quest(
    w: &mut World,
    session: Option<SessionId>,
    folder: &str,
    json_filename: &str,
) -> Option<String> {
    json2sql(
        w,
        session,
        JsonKind::Quest,
        folder,
        json_filename,
        &swap_segment(folder, "json", "sql"),
    )
}

/// Converts SQL to JSON, imports to database, clears the weenie cache
// ACE: DeveloperContentCommands.ImportSQLWeenie
fn import_sql_weenie_file(
    w: &mut World,
    session: Option<SessionId>,
    sql_folder: &str,
    sql_file: &str,
) {
    let Some(wcid) = dotnet_parse::uint_try_parse(first_digits(sql_file)) else {
        info(w, session, &format!("Couldn't find wcid from {sql_file}"));
        return;
    };

    // import sql to db
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear this weenie out of the cache
    w.content.clear_cached_weenie(wcid);

    // load weenie from database
    let Some(weenie) = w.content.get_weenie(wcid) else {
        info(w, session, &format!("Couldn't load weenie {wcid} from db"));
        return;
    };

    sql2json_weenie(w, session, &weenie, sql_folder, sql_file);
}

// ACE: DeveloperContentCommands.ImportSQLRecipe
fn import_sql_recipe_file(
    w: &mut World,
    session: Option<SessionId>,
    sql_folder: &str,
    sql_file: &str,
) {
    let Some(recipe_id) = dotnet_parse::uint_try_parse(first_digits(sql_file)) else {
        info(
            w,
            session,
            &format!("Couldn't find recipe id from {sql_file}"),
        );
        return;
    };

    // import sql to db
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear this recipe out of the cache
    w.content.clear_cookbook_cache();

    // load cookbooks + recipe from database
    let cookbooks = w.content.get_cookbooks_by_recipe_id(recipe_id);

    if cookbooks.is_empty() {
        info(
            w,
            session,
            &format!("Couldn't load recipe {recipe_id} from db"),
        );
        return;
    }

    sql2json_recipe(w, session, &cookbooks, sql_folder, sql_file);
}

// ACE: DeveloperContentCommands.ImportSQLLandblock
fn import_sql_landblock_file(
    w: &mut World,
    session: Option<SessionId>,
    sql_folder: &str,
    sql_file: &str,
) {
    let Some(landblock_id) = landblock_from_name(sql_file) else {
        info(
            w,
            session,
            &format!("Couldn't find landblock id from {sql_file}"),
        );
        return;
    };

    // import sql to db
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear any cached instances for this landblock
    w.content.clear_cached_instances_by_landblock(landblock_id);

    // load landblock instances from database
    let instances = w.content.get_cached_instances_by_landblock(landblock_id);

    // convert to json file
    sql2json_landblock(w, session, &instances, sql_folder, sql_file);
}

// ACE: DeveloperContentCommands.ImportSQLEncounter
fn import_sql_encounter_file(
    w: &mut World,
    session: Option<SessionId>,
    sql_folder: &str,
    sql_file: &str,
) {
    let Some(landblock_id) = landblock_from_name(sql_file) else {
        info(
            w,
            session,
            &format!("Couldn't find landblock id from {sql_file}"),
        );
        return;
    };

    // import sql to db
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear any cached encounters for this landblock
    w.content.clear_cached_encounters_by_landblock(landblock_id);
}

// ACE: DeveloperContentCommands.ImportSQLEvent
fn import_sql_event_file(
    w: &mut World,
    session: Option<SessionId>,
    sql_folder: &str,
    sql_file: &str,
) {
    // import sql to db
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear cached event
    let event_name = string_extensions::trim_end(sql_file, ".sql");
    w.content.clear_cached_event(&event_name);

    if event_manager::is_event_available(w, &event_name) {
        if event_manager::is_event_started(w, &event_name, None, None) {
            event_manager::stop_event(w, &event_name, None, None);
            info(
                w,
                session,
                &format!("-- Event {event_name} has been stopped."),
            );
        }
        if w.event_manager.remove(&event_name) {
            info(
                w,
                session,
                &format!("-- Event {event_name} has been removed from EventManager."),
            );
        }
    }

    // load event from db
    let evt = w
        .content
        .get_cached_event(&event_name)
        .expect("NullReferenceException: evt");

    if w.event_manager.try_add(Event::clone(&evt)) {
        info(
            w,
            session,
            &format!("-- Event {event_name} has been added to EventManager."),
        );
    }

    // Start the event if it needs to be
    if evt.state == GameEventState::On.0 && event_manager::start_event(w, &evt.name, None, None) {
        info(
            w,
            session,
            &format!("-- Event {event_name} has been started."),
        );
    }
}

// ACE: DeveloperContentCommands.ImportSQLQuest
fn import_sql_quest_file(
    w: &mut World,
    session: Option<SessionId>,
    sql_folder: &str,
    sql_file: &str,
) {
    // import sql to db
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear cached quest
    let quest_name = string_extensions::trim_end(sql_file, ".sql");
    w.content.clear_cached_quest(&quest_name);

    // load quest from db
    let quest = w.content.get_cached_quest(&quest_name);

    // convert to json file
    sql2json_quest(w, session, quest.as_deref(), sql_folder, sql_file);
}

// ACE: DeveloperContentCommands.ImportSQLSpell
fn import_sql_spell_file(
    w: &mut World,
    session: Option<SessionId>,
    sql_folder: &str,
    sql_file: &str,
) {
    let Some(spell_id) = dotnet_parse::uint_try_parse(first_digits(sql_file)) else {
        info(
            w,
            session,
            &format!("Couldn't find spell id from {sql_file}"),
        );
        return;
    };

    // import sql to db
    import_sql(w, &format!("{sql_folder}{sql_file}"));
    info(w, session, &format!("Imported {sql_file}"));

    // clear this spell out of the cache (and everything else)
    w.content.clear_spell_cache();
    world_object_magic::clear_spell_cache();

    // load spell from db
    let _spell = w.content.get_cached_spell(spell_id);
}

/// `json_filename` with the `wcid -` form ACE's JSON files use.
fn dash_after_wcid(json_filename: String) -> String {
    let digits = leading_digits(&json_filename).to_owned();
    if !digits.is_empty() && !json_filename.starts_with(&format!("{digits} -")) {
        return format!("{digits} -{}", &json_filename[digits.len()..]);
    }
    json_filename
}

/// Converts a sql file to json file
// ACE: DeveloperContentCommands.sql2json_weenie
// Not ACE's (a fix): the JSON folder is the SQL folder with its `sql`
// level renamed (`swap_segment`, as the `import-json`/`json2sql_*` folders are); ACE replaced every
// "sql" in the whole path.
pub fn sql2json_weenie(
    w: &mut World,
    session: Option<SessionId>,
    weenie: &Weenie,
    sql_folder: &str,
    sql_filename: &str,
) -> bool {
    let Some((mut json, mut json_weenie)) =
        export_json::try_convert_ace_weenie_to_lsd_json(weenie, w.now.utc)
    else {
        info(
            w,
            session,
            &format!("Failed to convert {sql_filename} to json"),
        );
        return false;
    };

    let json_folder = swap_segment(sql_folder, "sql", "json");
    let json_filename = dash_after_wcid(replace(sql_filename, ".sql", ".json"));

    DirectoryInfo::ensure(&json_folder);

    let path = format!("{json_folder}{json_filename}");
    if Path::new(&path).is_file() && lifestoned_loader_append_metadata(w, &path, &mut json_weenie) {
        json = serialize(&json_weenie);
    }

    write_all_text(&path, &json);

    info(
        w,
        session,
        &format!("Converted {sql_filename} to {json_filename}"),
    );

    true
}

// ACE: DeveloperContentCommands.sql2json_recipe
pub fn sql2json_recipe(
    w: &mut World,
    session: Option<SessionId>,
    cookbooks: &[Option<Arc<CookBook>>],
    sql_folder: &str,
    sql_filename: &str,
) -> bool {
    let Some(result) = export_json::try_convert_cookbooks(cookbooks) else {
        info(
            w,
            session,
            &format!("Failed to convert {sql_filename} to json"),
        );
        return false;
    };

    let json_folder = swap_segment(sql_folder, "sql", "json");
    let json_filename = dash_after_wcid(replace(sql_filename, ".sql", ".json"));

    DirectoryInfo::ensure(&json_folder);

    let json = serialize(&result);

    write_all_text(&format!("{json_folder}{json_filename}"), &json);

    info(
        w,
        session,
        &format!("Converted {sql_filename} to {json_filename}"),
    );

    true
}

/// `GDLEConverter.WeenieNames` and `GDLEConverter.WeenieClassNames`.
type NameDictionaries = (DotNetDict<u32, String>, DotNetDict<u32, String>);

thread_local! {
    /// `GDLEConverter.WeenieNames` and `GDLEConverter.WeenieClassNames`: set once, on first use.
    static GDLE_NAMES: RefCell<Option<NameDictionaries>> = const { RefCell::new(None) };
}

/// `GDLEConverter.TryConvert(instances, out result)`, after setting its name dictionaries when
/// they are null.
fn gdle_try_convert_landblock(
    w: &World,
    instances: &[LandblockInstance],
) -> Option<export_json::Landblock> {
    GDLE_NAMES.with(|n| {
        let mut n = n.borrow_mut();
        let (names, class_names) = n.get_or_insert_with(|| {
            (
                w.content.get_all_weenie_names(),
                w.content.get_all_weenie_class_names(),
            )
        });
        export_json::try_convert_landblock(instances, Some(names), Some(class_names))
    })
}

// ACE: DeveloperContentCommands.sql2json_landblock
pub fn sql2json_landblock(
    w: &mut World,
    session: Option<SessionId>,
    instances: &[LandblockInstance],
    sql_folder: &str,
    sql_filename: &str,
) -> bool {
    let Some(result) = gdle_try_convert_landblock(w, instances) else {
        info(
            w,
            session,
            &format!("Failed to convert {sql_filename} to json"),
        );
        return false;
    };

    let json_folder = swap_segment(sql_folder, "sql", "json");
    let json_filename = replace(sql_filename, ".sql", ".json");

    DirectoryInfo::ensure(&json_folder);

    let json = serialize(&result);

    write_all_text(&format!("{json_folder}{json_filename}"), &json);

    info(
        w,
        session,
        &format!("Converted {sql_filename} to {json_filename}"),
    );

    true
}

// ACE: DeveloperContentCommands.sql2json_quest
pub fn sql2json_quest(
    w: &mut World,
    session: Option<SessionId>,
    quest: Option<&Quest>,
    sql_folder: &str,
    sql_filename: &str,
) -> bool {
    let Some(result) =
        export_json::try_convert_quest(quest.expect("NullReferenceException: quest"))
    else {
        info(
            w,
            session,
            &format!("Failed to convert {sql_filename} to json"),
        );
        return false;
    };

    let json_folder = swap_segment(sql_folder, "sql", "json");
    let json_filename = replace(sql_filename, ".sql", ".json");

    DirectoryInfo::ensure(&json_folder);

    let json = serialize(&result);

    write_all_text(&format!("{json_folder}{json_filename}"), &json);

    info(
        w,
        session,
        &format!("Converted {sql_filename} to {json_filename}"),
    );

    true
}

/// `JsonSerializer.Serialize(value, LifestonedConverter.SerializerSettings)`; a NaN or infinity
/// throws, as it does in ACE.
fn serialize<T: export_json::ToJson>(value: &T) -> String {
    export_json::serialize(value).unwrap_or_else(|e| panic!("ArgumentException: {e}"))
}

/// `LifestonedLoader.AppendMetadata(file, lsdWeenie)`; a file holding `null` throws.
fn lifestoned_loader_append_metadata(
    w: &World,
    path: &str,
    lsd: &mut export_json::LsdWeenie,
) -> bool {
    let existing = read_all_text(path);
    export_json::append_metadata(&existing, lsd, w.now.utc).unwrap_or_else(|e| panic!("{e}"))
}

/// Imports an SQL file into the database: the content overlay. A failure throws (a panic, which
/// the command path reports as ACE's logs the exception).
// ACE: DeveloperContentCommands.ImportSQL
pub fn import_sql(w: &mut World, sql_file: &str) {
    let sql_commands = std::fs::read(sql_file)
        .unwrap_or_else(|e| panic!("FileNotFoundException: {sql_file}: {e}"));

    // (the CRLF conversion and the cut at "/* Lifestoned Changelog:" happen in the overlay's
    // applier, exactly as here)
    execute_sql_raw(w, &sql_commands, sql_file);
}

/// `ctx.Database.ExecuteSqlRaw(sql)` against the world database: the content overlay.
fn execute_sql_raw(w: &mut World, sql: &[u8], shown: &str) {
    let Some(overlay) = w.content.overlay() else {
        panic!("No content overlay is configured (Server.WorldOverlayPath), so the world database cannot be written");
    };
    if let Err(e) = overlay.import_sql(sql, shown) {
        panic!("{e}");
    }
}

// ================================================================================ createinst

thread_local! {
    /// `DeveloperContentCommands.LandblockInstanceWriter`.
    static LANDBLOCK_INSTANCE_WRITER: RefCell<Option<LandblockInstanceWriter>> = const { RefCell::new(None) };
    /// `DeveloperContentCommands.LandblockEncounterWriter`.
    static LANDBLOCK_ENCOUNTER_WRITER: RefCell<Option<EncounterSQLWriter>> = const { RefCell::new(None) };
    /// `DeveloperContentCommands.WeenieSQLWriter`.
    static WEENIE_SQL_WRITER: RefCell<Option<WeenieSQLWriter>> = const { RefCell::new(None) };
    /// `DeveloperContentCommands.RecipeSQLWriter`.
    static RECIPE_SQL_WRITER: RefCell<Option<RecipeSQLWriter>> = const { RefCell::new(None) };
    /// `DeveloperContentCommands.CookBookSQLWriter`.
    static COOK_BOOK_SQL_WRITER: RefCell<Option<CookBookSQLWriter>> = const { RefCell::new(None) };
    /// `DeveloperContentCommands.QuestSQLWriter`.
    static QUEST_SQL_WRITER: RefCell<Option<QuestSQLWriter>> = const { RefCell::new(None) };
    /// `DeveloperContentCommands.SpellSQLWriter`.
    static SPELL_SQL_WRITER: RefCell<Option<SpellSQLWriter>> = const { RefCell::new(None) };
}

/// The static `LandblockInstanceWriter`, created (with `WeenieNames`) when null.
fn with_landblock_instance_writer<R>(
    w: &World,
    f: impl FnOnce(&LandblockInstanceWriter) -> R,
) -> R {
    LANDBLOCK_INSTANCE_WRITER.with(|c| {
        let mut c = c.borrow_mut();
        let writer = c.get_or_insert_with(|| LandblockInstanceWriter {
            base: SQLWriter {
                weenie_names: Some(w.content.get_all_weenie_names()),
                ..SQLWriter::default()
            },
        });
        f(writer)
    })
}

/// The static `EncounterSQLWriter`, created (with `WeenieNames`) when null.
fn with_encounter_writer<R>(w: &World, f: impl FnOnce(&EncounterSQLWriter) -> R) -> R {
    LANDBLOCK_ENCOUNTER_WRITER.with(|c| {
        let mut c = c.borrow_mut();
        let writer = c.get_or_insert_with(|| EncounterSQLWriter {
            base: SQLWriter {
                weenie_names: Some(w.content.get_all_weenie_names()),
                ..SQLWriter::default()
            },
        });
        f(writer)
    })
}

// ACE: DeveloperContentCommands.HandleCreateInst
#[allow(clippy::too_many_lines)]
pub fn handle_create_inst(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let player = session_player(w, s);
    let loc = Position::from_position(&location_of(w, player));

    let mut param = parameters[0].clone();

    let mut parent_guid: Option<u32> = None;

    let landblock = current_landblock(w, player).landblock();

    let first_static_guid = 0x7000_0000 | (u32::from(landblock) << 12);

    if parameters.len() > 1 {
        let all_params = parameters.join(" ");

        if let Some((parent_guid_text, child)) = match_parent_child(&all_params) {
            let mut parent_guid_str = parent_guid_text.clone();
            param = child;

            if parent_guid_str.len() >= 2 && parent_guid_str[..2].eq_ignore_ascii_case("0x") {
                parent_guid_str = parent_guid_str[2..].to_owned();
            }

            let Some(pg) = dotnet_parse::uint_try_parse_hex(&parent_guid_str) else {
                chat(
                    w,
                    s,
                    &format!("Couldn't parse parent guid {parent_guid_text}"),
                );
                return;
            };

            parent_guid = Some(if pg <= 0xFFF {
                first_static_guid | pg
            } else {
                pg
            });
        } else if parameters[1].len() >= 2 && parameters[1][..2].eq_ignore_ascii_case("-c") {
            // get parent from last appraised object
            let Some(parent) = command_handler_helper::get_last_appraised_object(w, s) else {
                chat(w, s, "Couldn't find parent object");
                return;
            };

            parent_guid = Some(parent.full());
        }
    }

    let weenie = match dotnet_parse::uint_try_parse(&param) {
        Some(wcid) => w.content.get_weenie(wcid),           // wcid
        None => w.content.get_weenie_by_class_name(&param), // classname
    };

    let Some(weenie) = weenie else {
        chat(w, s, &format!("Couldn't find weenie {param}"));
        return;
    };

    // clear any cached instances for this landblock
    w.content.clear_cached_instances_by_landblock(landblock);

    let mut instances: Vec<LandblockInstance> = w
        .content
        .get_cached_instances_by_landblock(landblock)
        .to_vec();

    // for link mode, ensure parent guid instance exists
    let mut parent_obj: Option<ObjectGuid> = None;
    let mut parent_instance: Option<usize> = None;

    if let Some(pg) = parent_guid {
        parent_instance = instances.iter().position(|i| i.guid == pg);

        if parent_instance.is_none() {
            chat(
                w,
                s,
                &format!(
                    "Couldn't find landblock instance for parent guid 0x{}",
                    format(pg, "X8")
                ),
            );
            return;
        }

        parent_obj =
            landblock::get_object(w, current_landblock(w, player), ObjectGuid::new(pg), true);

        if parent_obj.is_none() {
            chat(
                w,
                s,
                &format!("Couldn't find parent object 0x{}", format(pg, "X8")),
            );
            return;
        }
    }

    let mut next_static_guid = get_next_static_guid(landblock, &instances);

    let max_static_guid = first_static_guid | 0xFFF;

    // manually specify a start guid?
    if parameters.len() == 2 {
        if let Some(mut start_guid) =
            dotnet_parse::uint_try_parse_hex(&parameters[1].replace("0x", ""))
        {
            if start_guid <= 0xFFF {
                start_guid |= first_static_guid;
            }

            if start_guid < first_static_guid || start_guid > max_static_guid {
                chat(
                    w,
                    s,
                    &format!(
                        "Landblock instance guid {} must be between {} and {}",
                        format(start_guid, "X8"),
                        format(first_static_guid, "X8"),
                        format(max_static_guid, "X8")
                    ),
                );
                return;
            }

            if instances.iter().any(|i| i.guid == start_guid) {
                chat(
                    w,
                    s,
                    &format!(
                        "Landblock instance guid {} already exists",
                        format(start_guid, "X8")
                    ),
                );
                return;
            }
            next_static_guid = start_guid;
        }
    }

    if next_static_guid > max_static_guid {
        chat(
            w,
            s,
            &format!(
                "Landblock {} has reached the maximum # of static guids",
                format(landblock, "X4")
            ),
        );
        return;
    }

    // create and spawn object
    let entity_weenie = Arc::new(empyrean_content::adapter::convert_to_entity_weenie(
        &weenie, false,
    ));

    let created = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(
            env,
            Some(entity_weenie),
            ObjectGuid::new(next_static_guid),
        )
    });
    let Some(wo) = created else {
        chat(
            w,
            s,
            &format!(
                "Failed to create new object for {} - {}",
                weenie.class_id, weenie.class_name
            ),
        );
        return;
    };
    let wo_guid = wo.guid;
    assert!(
        w.objects.insert(Box::new(wo)).is_ok(),
        "createinst: static guid 0x{next_static_guid:08X} is already live"
    );
    empyrean_world::world_objects::creature::post_insert(w, wo_guid);

    let is_link_child = parent_instance.is_some();

    if !obj(w, wo_guid).stuck() && !is_link_child {
        chat(w, s, &format!("{} - {} is missing PropertyBool.Stuck, cannot spawn as landblock instance unless it is a child object", weenie.class_id, weenie.class_name));
        // (ACE leaves the unspawned object to the garbage collector)
        world_object::drop_unreferenced(w, wo_guid);
        return;
    }

    // spawn as ethereal temporarily, to spawn directly on player position
    obj_mut(w, wo_guid).set_ethereal(Some(true));
    let mut wo_location = Position::from_position(&loc);

    // even on flat ground, objects can sometimes fail to spawn at the player's current Z
    // Position.Z has some weird thresholds when moving around, but i guess the same logic doesn't apply when trying to spawn in...
    wo_location.position_z += 0.05;
    obj_mut(w, wo_guid).set_location(Some(wo_location));

    let text = format!(
        "Creating new landblock instance {}@ {}\n{} - {} ({})",
        if is_link_child { "child object " } else { "" },
        loc.to_loc_string(),
        obj(w, wo_guid).weenie_class_id(),
        name_of(w, wo_guid),
        format(next_static_guid, "X8")
    );
    chat(w, s, &text);

    if !dispatch::enter_world::enter_world(w, wo_guid) {
        chat(w, s, "Failed to spawn new object at this location");
        world_object::drop_unreferenced(w, wo_guid);
        return;
    }

    // create new landblock instance
    let instance = create_landblock_instance(w, wo_guid, is_link_child);

    instances.push(instance.clone());

    if let (Some(pi), Some(parent_obj), Some(pg)) = (parent_instance, parent_obj, parent_guid) {
        let link = LandblockInstanceLink {
            parent_guid: pg,
            child_guid: wo_guid.full(),
            last_modified: date_time_now(w),
            ..Default::default()
        };

        instances[pi].landblock_instance_link.push(link);

        obj_mut(w, parent_obj)
            .wo
            .world_object_links
            .linked_instances
            .push(instance);

        // ActivateLinks?
        dispatch::set_link_properties::set_link_properties(w, parent_obj, wo_guid);
        obj_mut(w, parent_obj)
            .wo
            .world_object_links
            .child_links
            .push(wo_guid);
        obj_mut(w, wo_guid).wo.world_object_links.parent_link = Some(parent_obj);
    }

    sync_instances(w, session, landblock, &instances);
}

/// `Regex.Match(allParams, @"-p ([\S]+) -c ([\S]+)", RegexOptions.IgnoreCase)`: the two groups.
fn match_parent_child(all_params: &str) -> Option<(String, String)> {
    let lower = all_params.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("-p ") {
        let at = from + i;
        let rest = &all_params[at + 3..];
        let p_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if p_end > 0 {
            let after = &rest[p_end..];
            if after.len() > 4 && after[..4].eq_ignore_ascii_case(" -c ") {
                let c_rest = &after[4..];
                let c_end = c_rest.find(char::is_whitespace).unwrap_or(c_rest.len());
                if c_end > 0 {
                    return Some((rest[..p_end].to_owned(), c_rest[..c_end].to_owned()));
                }
            }
        }
        from = at + 1;
    }
    None
}

/// Serializes landblock instances to XXYY.sql file, import into database, and clears the cached
/// landblock instances
// ACE: DeveloperContentCommands.SyncInstances
pub fn sync_instances(
    w: &mut World,
    session: Option<SessionId>,
    landblock: u16,
    instances: &[LandblockInstance],
) {
    // serialize to .sql file
    let content_folder = verify_content_folder(w, session, false);

    let folder = DirectoryInfo::new(&format!(
        "{}{SEP}sql{SEP}landblocks{SEP}",
        content_folder.full_name
    ));

    if !folder.exists() {
        folder.create();
    }

    let sql_filename = format!("{}{SEP}{}.sql", folder.full_name, format(landblock, "X4"));

    if instances.is_empty() {
        // handle special case: deleting the last instance from landblock
        let _ = std::fs::remove_file(&sql_filename);

        execute_sql_raw(
            w,
            format!("DELETE FROM landblock_instance WHERE landblock={landblock};").as_bytes(),
            &sql_filename,
        );
    } else {
        let mut file_writer = SqlOut::new(new_line());

        with_landblock_instance_writer(w, |writer| {
            writer
                .create_sql_delete_statement(instances, &mut file_writer)
                .unwrap_or_else(|e| panic!("{e}"));

            file_writer.write_empty_line();

            writer
                .create_sql_insert_statement(instances, &mut file_writer)
                .unwrap_or_else(|e| panic!("{e}"));
        });

        write_all_text(&sql_filename, &file_writer.text);

        // import into db
        import_sql(w, &sql_filename);
    }

    // clear landblock instances for this landblock (again)
    w.content.clear_cached_instances_by_landblock(landblock);
}

// ACE: DeveloperContentCommands.CreateLandblockInstance
#[must_use]
pub fn create_landblock_instance(
    w: &World,
    wo: ObjectGuid,
    is_link_child: bool,
) -> LandblockInstance {
    let location = location_of(w, wo);
    LandblockInstance {
        guid: wo.full(),
        #[allow(clippy::cast_possible_wrap)]
        landblock: Some(location.landblock() as i32),
        weenie_class_id: obj(w, wo).weenie_class_id(),
        obj_cell_id: location.cell(),
        origin_x: location.position_x,
        origin_y: location.position_y,
        origin_z: location.position_z,
        angles_w: location.rotation_w,
        angles_x: location.rotation_x,
        angles_y: location.rotation_y,
        angles_z: location.rotation_z,
        is_link_child,
        last_modified: date_time_now(w),
        landblock_instance_link: Vec::new(),
    }
}

// ACE: DeveloperContentCommands.GetNextStaticGuid
#[must_use]
pub fn get_next_static_guid(landblock: u16, instances: &[LandblockInstance]) -> u32 {
    let first_guid = 0x7000_0000 | (u32::from(landblock) << 12);
    let last_guid = first_guid | 0xFFF;

    let highest_landblock_inst = instances
        .iter()
        .filter(|i| i.landblock == Some(i32::from(landblock)))
        .max_by_key(|i| i.guid);

    let Some(highest_landblock_inst) = highest_landblock_inst else {
        return first_guid;
    };

    let next_guid = highest_landblock_inst.guid.wrapping_add(1);

    if next_guid <= last_guid {
        return next_guid;
    }

    // try more exhaustive search
    get_next_static_guid_gap_finder(landblock, instances).unwrap_or(next_guid)
}

// ACE: DeveloperContentCommands.GetNextStaticGuid_GapFinder
#[must_use]
pub fn get_next_static_guid_gap_finder(
    landblock: u16,
    instances: &[LandblockInstance],
) -> Option<u32> {
    let landblock_guids: std::collections::HashSet<u32> = instances
        .iter()
        .filter(|i| i.landblock == Some(i32::from(landblock)))
        .map(|i| i.guid)
        .collect();

    let first_guid = 0x7000_0000 | (u32::from(landblock) << 12);
    let last_guid = first_guid | 0xFFF;

    (first_guid..=last_guid).find(|guid| !landblock_guids.contains(guid))
}

// ================================================================================ removeinst

// ACE: DeveloperContentCommands.HandleRemoveInst
pub fn handle_remove_inst(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    remove_instance(w, session, false);
}

// ACE: DeveloperContentCommands.RemoveInstance
#[allow(clippy::too_many_lines)]
pub fn remove_instance(w: &mut World, session: Option<SessionId>, confirmed: bool) {
    let s = require(session);
    let Some(wo) = command_handler_helper::get_last_appraised_object(w, s) else {
        return;
    };

    let Some(wo_location) = obj(w, wo).location() else {
        return;
    };

    #[allow(clippy::cast_possible_truncation)]
    let landblock = wo_location.landblock() as u16;

    // if generator child, try getting the "real" guid
    let mut guid = wo.full();
    if let Some(generator) = obj(w, wo).wo.world_object_generators.generator {
        if let Some(static_guid) = w
            .objects
            .get(generator)
            .and_then(|g| g.get_static_guid(guid))
        {
            guid = static_guid;
        }
    }

    let mut instances: Vec<LandblockInstance> = w
        .content
        .get_cached_instances_by_landblock(landblock)
        .to_vec();

    let Some(instance_idx) = instances.iter().position(|i| i.guid == guid) else {
        chat(
            w,
            s,
            &format!(
                "Couldn't find landblock_instance for {} - {} (0x{})",
                obj(w, wo).weenie_class_id(),
                name_of(w, wo),
                format(guid, "X8")
            ),
        );
        return;
    };

    let mut num_childs = instances[instance_idx].landblock_instance_link.len();

    if num_childs > 0 && !confirmed {
        // get total numChilds iteratively
        num_childs = 0;
        for link in instances[instance_idx].landblock_instance_link.clone() {
            num_childs += get_num_childs(w, s, &link, &instances);
        }

        // require confirmation for parent objects
        let msg = format!(
            "Are you sure you want to delete this parent object, and {num_childs} child object{}?",
            if num_childs == 1 { "" } else { "s" }
        );
        let player = session_player(w, s);
        let action: empyrean_world::entity::confirmation::CustomAction =
            Box::new(move |w: &mut World| remove_instance(w, Some(s), true));
        let confirmation =
            empyrean_world::entity::confirmation::Confirmation::custom(player, action);
        if !empyrean_world::world_objects::managers::confirmation_manager::enqueue_send(
            w,
            player,
            confirmation,
            &msg,
        ) {
            player_networking::send_weenie_error(w, player, WeenieError::ConfirmationInProgress);
        }
        return;
    }

    let instance = instances[instance_idx].clone();

    if instance.is_link_child {
        let mut link: Option<LandblockInstanceLink> = None;

        for parent in instances
            .iter_mut()
            .filter(|i| !i.landblock_instance_link.is_empty())
        {
            if let Some(li) = parent
                .landblock_instance_link
                .iter()
                .position(|l| l.child_guid == instance.guid)
            {
                link = Some(parent.landblock_instance_link.remove(li));
                break;
            }
        }
        if link.is_none() {
            chat(
                w,
                s,
                &format!(
                    "Couldn't find parent link for child {} - {} (0x{})",
                    obj(w, wo).weenie_class_id(),
                    name_of(w, wo),
                    format(guid, "X8")
                ),
            );
            return;
        }
    }

    let wcid = obj(w, wo).weenie_class_id();
    let name = name_of(w, wo);

    world_object_decay::delete_object(w, wo, None);

    for link in &instance.landblock_instance_link {
        remove_child(w, s, link, &mut instances);
    }

    instances.retain(|i| i.guid != instance.guid);

    sync_instances(w, session, landblock, &instances);

    chat(
        w,
        s,
        &format!(
            "Removed {}{wcid} - {name} (0x{}) from landblock instances",
            if instance.is_link_child { "child " } else { "" },
            format(guid, "X8")
        ),
    );
}

// ACE: DeveloperContentCommands.GetNumChilds
pub fn get_num_childs(
    w: &mut World,
    session: SessionId,
    link: &LandblockInstanceLink,
    instances: &[LandblockInstance],
) -> usize {
    let Some(child) = instances.iter().find(|i| i.guid == link.child_guid) else {
        chat(
            w,
            session,
            &format!(
                "Couldn't find child instance 0x{}",
                format(link.child_guid, "X8")
            ),
        );
        return 0;
    };

    let mut num_childs = 1;

    for sub_link in &child.landblock_instance_link {
        num_childs += get_num_childs(w, session, sub_link, instances);
    }

    num_childs
}

// ACE: DeveloperContentCommands.RemoveChild
pub fn remove_child(
    w: &mut World,
    session: SessionId,
    link: &LandblockInstanceLink,
    instances: &mut Vec<LandblockInstance>,
) {
    let Some(ci) = instances.iter().position(|i| i.guid == link.child_guid) else {
        chat(
            w,
            session,
            &format!(
                "Couldn't find child instance 0x{}",
                format(link.child_guid, "X8")
            ),
        );
        return;
    };

    let child = instances.remove(ci);

    let player = session_player(w, session);
    let wo = landblock::get_object(
        w,
        current_landblock(w, player),
        ObjectGuid::new(child.guid),
        true,
    );

    if let Some(wo) = wo {
        let (wcid, name) = (obj(w, wo).weenie_class_id(), name_of(w, wo));
        world_object_decay::delete_object(w, wo, None);

        chat(
            w,
            session,
            &format!("Removed child {wcid} - {name} (0x{wo}) from landblock instances"),
        );
    } else {
        chat(
            w,
            session,
            &format!(
                "Couldn't find child object for 0x{}",
                format(link.child_guid, "X8")
            ),
        );
    }

    for sub_link in &child.landblock_instance_link {
        remove_child(w, session, sub_link, instances);
    }
}

// ================================================================================ encounters

// ACE: DeveloperContentCommands.HandleAddEncounter
pub fn handle_add_encounter(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let param = &parameters[0];

    let weenie = match dotnet_parse::uint_try_parse(param) {
        Some(wcid) => w.content.get_weenie(wcid),          // wcid
        None => w.content.get_weenie_by_class_name(param), // classname
    };

    let Some(weenie) = weenie else {
        chat(w, s, &format!("Couldn't find weenie {param}"));
        return;
    };

    let player = session_player(w, s);
    let pos = location_of(w, player);

    if (pos.cell() & 0xFFFF) >= 0x100 {
        chat(w, s, "You must be outdoors to create an encounter!");
        return;
    }

    let cell_x = CsCast::<i32>::cs_cast(pos.position_x) / 24;
    let cell_y = CsCast::<i32>::cs_cast(pos.position_y) / 24;

    #[allow(clippy::cast_possible_truncation)]
    let landblock = pos.landblock() as u16;

    // clear any cached encounters for this landblock
    w.content.clear_cached_encounters_by_landblock(landblock);

    // get existing encounters for this landblock
    let mut encounters: Vec<Encounter> = w
        .content
        .get_cached_encounters_by_landblock(landblock)
        .to_vec();

    // check for existing encounter
    if encounters
        .iter()
        .any(|i| i.cell_x == cell_x && i.cell_y == cell_y)
    {
        chat(w, s, "This cell already contains an encounter!");
        return;
    }

    // spawn encounter
    let Some(wo) = spawn_encounter(w, &weenie, cell_x, cell_y, &pos, s) else {
        return;
    };

    let text = format!(
        "Creating new encounter @ landblock {}, cellX={cell_x}, cellY={cell_y}\n{} - {}",
        format(pos.landblock(), "X4"),
        obj(w, wo).weenie_class_id(),
        name_of(w, wo)
    );
    chat(w, s, &text);

    // add a new encounter (verifications?)
    let encounter = Encounter {
        #[allow(clippy::cast_possible_wrap)]
        landblock: pos.landblock() as i32,
        cell_x,
        cell_y,
        weenie_class_id: weenie.class_id,
        last_modified: date_time_now(w),
        ..Default::default()
    };

    encounters.push(encounter);

    // write encounters to sql file / load into db
    sync_encounters(w, session, landblock, &encounters);
}

// ACE: DeveloperContentCommands.SpawnEncounter
pub fn spawn_encounter(
    w: &mut World,
    weenie: &Weenie,
    cell_x: i32,
    cell_y: i32,
    pos: &Position,
    session: SessionId,
) -> Option<ObjectGuid> {
    let Some(wo) = landblock::world_object_factory_create_new_world_object(w, weenie.class_id)
    else {
        chat(w, session, "Failed to create encounter weenie");
        return None;
    };
    let guid = wo.guid;
    assert!(w.objects.insert(wo).is_ok(), "fresh dynamic guid");
    empyrean_world::world_objects::creature::post_insert(w, guid);

    if !obj(w, guid).is_generator() {
        chat(w, session, "Encounter must be a Generator");
        world_object::drop_unreferenced(w, guid);
        return None;
    }

    #[allow(clippy::cast_precision_loss)]
    let x_pos = (cell_x as f32 * 24.0).clamp(0.5, 191.5);
    #[allow(clippy::cast_precision_loss)]
    let y_pos = (cell_y as f32 * 24.0).clamp(0.5, 191.5);

    let mut origin = Vector3::new(x_pos, y_pos, 0.0);
    let obj_cell_id = landblock::physics_position_adjust_to_outside(pos.cell(), &mut origin);

    let player = session_player(w, session);
    origin.z = landblock::physics_landblock_get_z(w, current_landblock(w, player), origin);

    obj_mut(w, guid).set_location(Some(Position::from_vectors(
        obj_cell_id,
        origin,
        Quaternion::IDENTITY,
    )));

    if landblock::lscape_get_landcell_has_building(w, LandblockId::new(obj_cell_id), obj_cell_id) {
        chat(w, session, "Failed to create encounter near building cell");
        world_object::drop_unreferenced(w, guid);
        return None;
    }

    if property_manager::get_bool(w, "override_encounter_spawn_rates", false, true).item {
        let interval = property_manager::get_double(w, "encounter_regen_interval", 0.0, true).item;
        obj_mut(w, guid).set_regeneration_interval(interval);

        world_object_tick::reinitialize_heartbeats(w, guid);

        // (ACE clones the weenie's generator list into the biota when the two share it; here the
        // biota always holds its own copy, so there is nothing to split.)
    }

    let success = dispatch::enter_world::enter_world(w, guid);

    if !success {
        chat(w, session, "Failed to spawn encounter");
        world_object::drop_unreferenced(w, guid);
        return None;
    }
    Some(guid)
}

/// Serializes encounters to XXYY.sql file, import into database, and clears the cached
/// encounters
// ACE: DeveloperContentCommands.SyncEncounters
pub fn sync_encounters(
    w: &mut World,
    session: Option<SessionId>,
    landblock: u16,
    encounters: &[Encounter],
) {
    // serialize to .sql file
    let content_folder = verify_content_folder(w, session, false);

    let folder = DirectoryInfo::new(&format!(
        "{}{SEP}sql{SEP}encounters{SEP}",
        content_folder.full_name
    ));

    if !folder.exists() {
        folder.create();
    }

    let sql_filename = format!("{}{SEP}{}.sql", folder.full_name, format(landblock, "X4"));

    if encounters.is_empty() {
        // handle special case: deleting the last encounter from landblock
        let _ = std::fs::remove_file(&sql_filename);

        execute_sql_raw(
            w,
            format!("DELETE FROM encounter WHERE landblock={landblock};").as_bytes(),
            &sql_filename,
        );
    } else {
        let mut file_writer = SqlOut::new(new_line());

        with_encounter_writer(w, |writer| {
            writer
                .create_sql_delete_statement(encounters, &mut file_writer)
                .unwrap_or_else(|e| panic!("{e}"));

            file_writer.write_empty_line();

            writer
                .create_sql_insert_statement(encounters, &mut file_writer)
                .unwrap_or_else(|e| panic!("{e}"));
        });

        write_all_text(&sql_filename, &file_writer.text);

        // import into db
        import_sql(w, &sql_filename);
    }

    // clear the encounters for this landblock (again)
    w.content.clear_cached_encounters_by_landblock(landblock);
}

// ACE: DeveloperContentCommands.HandleRemoveEnc
pub fn handle_remove_enc(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let Some(mut o) = command_handler_helper::get_last_appraised_object(w, s) else {
        return;
    };

    // find root generator
    while let Some(generator) = obj(w, o).wo.world_object_generators.generator {
        o = generator;
    }

    let location = location_of(w, o);
    let cell_x = CsCast::<i32>::cs_cast(location.position_x) / 24;
    let cell_y = CsCast::<i32>::cs_cast(location.position_y) / 24;

    #[allow(clippy::cast_possible_truncation)]
    let landblock = location.landblock() as u16;

    // clear any cached encounters for this landblock
    w.content.clear_cached_encounters_by_landblock(landblock);

    // get existing encounters for this landblock
    let mut encounters: Vec<Encounter> = w
        .content
        .get_cached_encounters_by_landblock(landblock)
        .to_vec();

    let wcid = obj(w, o).weenie_class_id();

    // check for existing encounter
    let Some(ei) = encounters
        .iter()
        .position(|i| i.cell_x == cell_x && i.cell_y == cell_y && i.weenie_class_id == wcid)
    else {
        chat(
            w,
            s,
            &format!("Couldn't find encounter for {wcid} - {}", name_of(w, o)),
        );
        return;
    };

    chat(
        w,
        s,
        &format!(
            "Removing encounter @ landblock {}, cellX={cell_x}, cellY={cell_y}\n{wcid} - {}",
            format(location.landblock(), "X4"),
            name_of(w, o)
        ),
    );

    encounters.remove(ei);

    sync_encounters(w, session, landblock, &encounters);

    // this is needed for any generators that don't have GeneratorDestructionType
    destroy_all(w, o);
}

/// Destroys a parent generator, and all of its child objects
/// DIVERGE (arch): the children are read before `Destroy`, which removes the object from the
/// store (ACE reads the destroyed object's profiles afterwards); a child its generator's own
/// destruction already destroyed is skipped.
// ACE: DeveloperContentCommands.DestroyAll
fn destroy_all(w: &mut World, wo: ObjectGuid) {
    let children: Vec<ObjectGuid> = w
        .objects
        .get(wo)
        .map(|o| {
            o.wo.world_object_generators
                .generator_profiles
                .iter()
                .flat_map(|profile| profile.spawned.values().map(|v| v.try_get_world_object(w)))
                .flatten()
                .collect()
        })
        .unwrap_or_default();

    world_object::destroy(w, wo, true, false);

    for child in children {
        if w.objects.contains(child) {
            destroy_all(w, child);
        }
    }
}

// ================================================================================ export-json

// ACE: DeveloperContentCommands.HandleExportJsonFolder
pub fn handle_export_json_folder(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let param = parameters[0].clone();
    export_json_weenie(w, session, &param, true);
}

// ACE: DeveloperContentCommands.HandleExportJson
pub fn handle_export_json(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some((param, content_type)) = parse_content_type(w, session, parameters) else {
        return;
    };
    match content_type {
        FileType::LandblockInstance => export_json_landblock(w, session, &param),
        FileType::Quest => export_json_quest(w, session, &param),
        FileType::Recipe => export_json_recipe(w, session, &param),
        FileType::Weenie => export_json_weenie(w, session, &param, false),
        _ => {}
    }
}

/// `GetWeenie(wcid)` for a number, else `GetWeenie(classname)`.
fn weenie_by_param(w: &World, param: &str) -> Option<Weenie> {
    match dotnet_parse::uint_try_parse(param) {
        Some(wcid) => w.content.get_weenie(wcid),
        None => w.content.get_weenie_by_class_name(param),
    }
}

/// The `WeenieType\CreatureType\` or `WeenieType\ItemType\` folder of `export-*-folders`.
fn type_folder(root: &str, kind: &str, weenie: &Weenie) -> String {
    #[allow(clippy::cast_sign_loss)]
    let weenie_type = WeenieType(weenie.r#type as u32);
    let base = format!("{root}{SEP}{kind}{SEP}weenies{SEP}{weenie_type}{SEP}");
    if weenie_type == WeenieType::Creature {
        // Export to the "CreatureType" folder
        match weenie.weenie_properties_int.iter().find(|x| x.r#type == 2) {
            None => base,
            #[allow(clippy::cast_sign_loss)]
            Some(c_type) => format!("{base}{}{SEP}", CreatureType(c_type.value as u32)),
        }
    } else {
        // Otherwise goes to "ItemType" folder
        match weenie.weenie_properties_int.iter().find(|x| x.r#type == 1) {
            None => base,
            #[allow(clippy::cast_sign_loss)]
            Some(i_type) => format!("{base}{}{SEP}", ItemType(i_type.value as u32)),
        }
    }
}

// ACE: DeveloperContentCommands.ExportJsonWeenie
pub fn export_json_weenie(
    w: &mut World,
    session: Option<SessionId>,
    param: &str,
    with_folders: bool,
) {
    let di = verify_content_folder(w, session, false);

    let Some(weenie) = weenie_by_param(w, param) else {
        info(w, session, &format!("Couldn't find weenie {param}"));
        return;
    };

    let Some((mut json, mut json_weenie)) =
        export_json::try_convert_ace_weenie_to_lsd_json(&weenie, w.now.utc)
    else {
        info(
            w,
            session,
            &format!(
                "Failed to convert {} - {} to json",
                weenie.class_id, weenie.class_name
            ),
        );
        return;
    };

    let json_folder = if with_folders {
        type_folder(&di.full_name, "json", &weenie)
    } else {
        format!("{}{SEP}json{SEP}weenies{SEP}", di.full_name)
    };

    DirectoryInfo::ensure(&json_folder);

    let name = weenie
        .weenie_properties_string
        .iter()
        .find(|i| i.r#type == PropertyString::Name.0)
        .map(|i| i.value.clone())
        .unwrap_or_default();
    let json_filename = format!("{} - {name}.json", weenie.class_id);

    let path = format!("{json_folder}{json_filename}");
    if Path::new(&path).is_file() && lifestoned_loader_append_metadata(w, &path, &mut json_weenie) {
        json = serialize(&json_weenie);
    }

    write_all_text(&path, &json);

    info(
        w,
        session,
        &format!("Exported {json_folder}{json_filename}"),
    );
}

/// The static `RecipeSQLWriter`, created (with `WeenieNames`) when null.
fn with_recipe_writer<R>(w: &World, f: impl FnOnce(&RecipeSQLWriter) -> R) -> R {
    RECIPE_SQL_WRITER.with(|c| {
        let mut c = c.borrow_mut();
        let writer = c.get_or_insert_with(|| RecipeSQLWriter {
            base: SQLWriter {
                weenie_names: Some(w.content.get_all_weenie_names()),
                ..SQLWriter::default()
            },
        });
        f(writer)
    })
}

/// The cook books `GetCookbooksByRecipeId` returned, dereferenced.
fn cookbook_rows(cookbooks: &[Option<Arc<CookBook>>]) -> Vec<CookBook> {
    cookbooks
        .iter()
        .map(|c| CookBook::clone(c.as_ref().expect("NullReferenceException: cookbook")))
        .collect()
}

// ACE: DeveloperContentCommands.ExportJsonRecipe
pub fn export_json_recipe(w: &mut World, session: Option<SessionId>, param: &str) {
    let di = verify_content_folder(w, session, false);

    let Some(recipe_id) = dotnet_parse::uint_try_parse(param) else {
        info(w, session, &format!("{param} not a valid recipe id"));
        return;
    };

    let cookbooks = w.content.get_cookbooks_by_recipe_id(recipe_id);
    if cookbooks.is_empty() {
        info(w, session, &format!("Couldn't find recipe id {recipe_id}"));
        return;
    }

    let Some(result) = export_json::try_convert_cookbooks(&cookbooks) else {
        info(
            w,
            session,
            &format!("Failed to convert recipe id {recipe_id} to json"),
        );
        return;
    };

    let json_folder = format!("{}{SEP}json{SEP}recipes{SEP}", di.full_name);

    DirectoryInfo::ensure(&json_folder);

    let rows = cookbook_rows(&cookbooks);
    let recipe = rows[0]
        .recipe
        .clone()
        .expect("NullReferenceException: cookbooks[0].Recipe");
    let desc = with_recipe_writer(w, |writer| {
        writer.get_default_file_name(&recipe, Some(&rows), true)
    });

    let json_filename = format!(
        "{} - {}.json",
        format(recipe_id, "00000"),
        desc.unwrap_or_default()
    );

    let json = serialize(&result);

    write_all_text(&format!("{json_folder}{json_filename}"), &json);

    info(
        w,
        session,
        &format!("Exported {json_folder}{json_filename}"),
    );
}

// ACE: DeveloperContentCommands.ExportJsonLandblock
pub fn export_json_landblock(w: &mut World, session: Option<SessionId>, param: &str) {
    let di = verify_content_folder(w, session, false);

    let Some(landblock_id) = landblock_from_name(param) else {
        info(w, session, &format!("{param} not a valid landblock"));
        return;
    };

    let instances = w.content.get_cached_instances_by_landblock(landblock_id);

    let Some(result) = gdle_try_convert_landblock(w, &instances) else {
        info(
            w,
            session,
            &format!(
                "Failed to convert landblock {} to json",
                format(landblock_id, "X4")
            ),
        );
        return;
    };

    let json_folder = format!("{}{SEP}json{SEP}landblocks{SEP}", di.full_name);

    DirectoryInfo::ensure(&json_folder);

    let json_filename = format!("{}.json", format(landblock_id, "X4"));

    let json = serialize(&result);

    write_all_text(&format!("{json_folder}{json_filename}"), &json);

    info(
        w,
        session,
        &format!("Exported {json_folder}{json_filename}"),
    );
}

// ACE: DeveloperContentCommands.ExportJsonQuest
pub fn export_json_quest(w: &mut World, session: Option<SessionId>, quest_name: &str) {
    let di = verify_content_folder(w, session, false);

    let Some(quest) = w.content.get_cached_quest(quest_name) else {
        info(w, session, &format!("Couldn't find quest {quest_name}"));
        return;
    };

    let Some(result) = export_json::try_convert_quest(&quest) else {
        info(
            w,
            session,
            &format!("Failed to convert quest {quest_name} to json"),
        );
        return;
    };

    let json_folder = format!("{}{SEP}json{SEP}quests{SEP}", di.full_name);

    DirectoryInfo::ensure(&json_folder);

    let json_filename = format!("{quest_name}.json");

    let json = serialize(&result);

    write_all_text(&format!("{json_folder}{json_filename}"), &json);

    info(
        w,
        session,
        &format!("Exported {json_folder}{json_filename}"),
    );
}

// ================================================================================ export-sql

// ACE: DeveloperContentCommands.HandleExportSqlFolder
pub fn handle_export_sql_folder(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let param = parameters[0].clone();
    export_sql_weenie(w, session, &param, true);
}

// ACE: DeveloperContentCommands.HandleExportSql
pub fn handle_export_sql(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some((param, content_type)) = parse_content_type(w, session, parameters) else {
        return;
    };
    match content_type {
        FileType::LandblockInstance => export_sql_landblock(w, session, &param),
        FileType::Encounter => export_sql_encounter(w, session, &param),
        FileType::Event => export_sql_event(w, session, &param),
        FileType::Quest => export_sql_quest(w, session, &param),
        FileType::Recipe => export_sql_recipe(w, session, &param),
        FileType::Spell => export_sql_spell(w, session, &param),
        FileType::Weenie => export_sql_weenie(w, session, &param, false),
        FileType::Undefined => {}
    }
}

/// Write the writer's text to `folder + file_name`; the writer's exception, or a failed write,
/// is ACE's `catch`: "Failed to export". (What was written before the exception stays in the file.)
fn write_export(
    w: &mut World,
    session: Option<SessionId>,
    folder: &str,
    file_name: &str,
    out: &SqlOut,
    written: Result<(), String>,
) {
    let path = format!("{folder}{file_name}");
    let saved = std::fs::write(&path, &out.text).map_err(|e| e.to_string());
    if let Err(e) = written.and(saved) {
        // (Console.WriteLine(e))
        log::info!("{e}");
        info(w, session, &format!("Failed to export {folder}{file_name}"));
        return;
    }

    info(w, session, &format!("Exported {folder}{file_name}"));
}

// ACE: DeveloperContentCommands.ExportSQLWeenie
pub fn export_sql_weenie(
    w: &mut World,
    session: Option<SessionId>,
    param: &str,
    with_folders: bool,
) {
    let di = verify_content_folder(w, session, false);

    let Some(weenie) = weenie_by_param(w, param) else {
        info(w, session, &format!("Couldn't find weenie {param}"));
        return;
    };

    let sql_folder = if with_folders {
        type_folder(&di.full_name, "sql", &weenie)
    } else {
        format!("{}{SEP}sql{SEP}weenies{SEP}", di.full_name)
    };

    DirectoryInfo::ensure(&sql_folder);

    let mut writer_out = SqlOut::new(new_line());
    let (sql_filename, written) = WEENIE_SQL_WRITER.with(|c| {
        let mut c = c.borrow_mut();
        let writer = c.get_or_insert_with(|| WeenieSQLWriter {
            base: SQLWriter {
                weenie_names: Some(w.content.get_all_weenie_names()),
                spell_names: Some(w.content.get_all_spell_names()),
                treasure_death: Some(w.content.get_all_treasure_death()),
                treasure_wielded: Some(w.content.get_all_treasure_wielded()),
                packet_op_codes: Some(empyrean_entity::packet_op_code_names::values()),
                ..SQLWriter::default()
            },
        });

        let sql_filename = writer.get_default_file_name(&weenie);

        writer.create_sql_delete_statement(&weenie, &mut writer_out);
        writer_out.write_empty_line();
        let written = writer
            .create_sql_insert_statement(&weenie, &mut writer_out)
            .map_err(|e| e.to_string());
        (sql_filename, written)
    });

    write_export(w, session, &sql_folder, &sql_filename, &writer_out, written);
}

// ACE: DeveloperContentCommands.ExportSQLRecipe
pub fn export_sql_recipe(w: &mut World, session: Option<SessionId>, param: &str) {
    let di = verify_content_folder(w, session, false);

    let Some(recipe_id) = dotnet_parse::uint_try_parse(param) else {
        info(w, session, &format!("{param} not a valid recipe id"));
        return;
    };

    let cookbooks = w.content.get_cookbooks_by_recipe_id(recipe_id);
    if cookbooks.is_empty() {
        info(w, session, &format!("Couldn't find recipe id {recipe_id}"));
        return;
    }

    let sql_folder = format!("{}{SEP}sql{SEP}recipes{SEP}", di.full_name);

    DirectoryInfo::ensure(&sql_folder);

    let cookbooks = cookbook_rows(&cookbooks);

    // same recipe for all cookbooks
    let recipe = cookbooks[0]
        .recipe
        .clone()
        .expect("NullReferenceException: cookbooks[0].Recipe");

    let sql_filename = with_recipe_writer(w, |writer| {
        writer.get_default_file_name(&recipe, Some(&cookbooks), false)
    })
    .unwrap_or_default();

    COOK_BOOK_SQL_WRITER.with(|c| {
        c.borrow_mut().get_or_insert_with(|| CookBookSQLWriter {
            base: SQLWriter {
                weenie_names: Some(w.content.get_all_weenie_names()),
                ..SQLWriter::default()
            },
        });
    });

    let mut sql_file = SqlOut::new(new_line());
    let written = with_recipe_writer(w, |recipe_writer| -> Result<(), String> {
        recipe_writer.create_sql_delete_statement(&recipe, &mut sql_file);
        sql_file.write_empty_line();

        recipe_writer
            .create_sql_insert_statement(&recipe, &mut sql_file)
            .map_err(|e| e.to_string())?;
        sql_file.write_empty_line();

        COOK_BOOK_SQL_WRITER.with(|c| {
            let c = c.borrow();
            let cook_book_writer = c.as_ref().expect("created above");
            cook_book_writer
                .create_sql_delete_statement(&cookbooks, &mut sql_file)
                .map_err(|e| e.to_string())?;
            sql_file.write_empty_line();

            cook_book_writer
                .create_sql_insert_statement(&cookbooks, &mut sql_file)
                .map_err(|e| e.to_string())
        })
    });

    write_export(w, session, &sql_folder, &sql_filename, &sql_file, written);
}

// ACE: DeveloperContentCommands.ExportSQLLandblock
pub fn export_sql_landblock(w: &mut World, session: Option<SessionId>, param: &str) {
    let di = verify_content_folder(w, session, false);

    let Some(landblock_id) = landblock_from_name(param) else {
        info(w, session, &format!("{param} not a valid landblock"));
        return;
    };

    let instances = w.content.get_cached_instances_by_landblock(landblock_id);

    let sql_folder = format!("{}{SEP}sql{SEP}landblocks{SEP}", di.full_name);

    DirectoryInfo::ensure(&sql_folder);

    let sql_filename = format!("{}.sql", format(landblock_id, "X4"));

    let empty = instances.is_empty();
    if empty {
        info(
            w,
            session,
            &format!("Landblock {} is empty.", format(landblock_id, "X4")),
        );
    }

    let mut sql_file = SqlOut::new(new_line());
    let written = with_landblock_instance_writer(w, |writer| -> Result<(), String> {
        // Check if the Landblock is empty
        if empty {
            // We'll just create a dummy list with a fake instance in our landblock so we don't anger CreateSQLDeleteStatement()
            let dummy_instance = LandblockInstance {
                obj_cell_id: u32::from(landblock_id) << 16,
                ..Default::default()
            };
            writer
                .create_sql_delete_statement(&[dummy_instance], &mut sql_file)
                .map_err(|e| e.to_string())?;
        } else {
            writer
                .create_sql_delete_statement(&instances, &mut sql_file)
                .map_err(|e| e.to_string())?;
        }
        sql_file.write_empty_line();

        writer
            .create_sql_insert_statement(&instances, &mut sql_file)
            .map_err(|e| e.to_string())
    });

    write_export(w, session, &sql_folder, &sql_filename, &sql_file, written);
}

// ACE: DeveloperContentCommands.ExportSQLEncounter
pub fn export_sql_encounter(w: &mut World, session: Option<SessionId>, param: &str) {
    let di = verify_content_folder(w, session, false);

    let Some(landblock_id) = landblock_from_name(param) else {
        info(w, session, &format!("{param} not a valid landblock"));
        return;
    };

    let encounters = w.content.get_cached_encounters_by_landblock(landblock_id);

    if encounters.is_empty() {
        info(
            w,
            session,
            &format!(
                "Couldn't find encounters for landblock {}",
                format(landblock_id, "X4")
            ),
        );
        return;
    }

    let sql_folder = format!("{}{SEP}sql{SEP}encounters{SEP}", di.full_name);

    DirectoryInfo::ensure(&sql_folder);

    let sql_filename = format!("{}.sql", format(landblock_id, "X4"));

    let mut sql_file = SqlOut::new(new_line());
    let written = with_encounter_writer(w, |writer| -> Result<(), String> {
        writer
            .create_sql_delete_statement(&encounters, &mut sql_file)
            .map_err(|e| e.to_string())?;

        sql_file.write_empty_line();

        writer
            .create_sql_insert_statement(&encounters, &mut sql_file)
            .map_err(|e| e.to_string())
    });

    write_export(w, session, &sql_folder, &sql_filename, &sql_file, written);
}

// ACE: DeveloperContentCommands.ExportSQLEvent
pub fn export_sql_event(w: &mut World, session: Option<SessionId>, event_name: &str) {
    let di = verify_content_folder(w, session, false);

    let Some(evt) = w.content.get_cached_event(event_name) else {
        info(w, session, &format!("Couldn't find event `{event_name}`"));
        return;
    };

    let sql_folder = format!("{}{SEP}sql{SEP}events{SEP}", di.full_name);

    DirectoryInfo::ensure(&sql_folder);

    let sql_filename = format!("{event_name}.sql");

    let event_sql_writer = EventSQLWriter::default();
    let mut sql_file = SqlOut::new(new_line());
    event_sql_writer.create_sql_delete_statement(&evt, &mut sql_file);

    sql_file.write_empty_line();

    event_sql_writer.create_sql_insert_statement(&evt, &mut sql_file);

    write_export(w, session, &sql_folder, &sql_filename, &sql_file, Ok(()));
}

// ACE: DeveloperContentCommands.ExportSQLQuest
pub fn export_sql_quest(w: &mut World, session: Option<SessionId>, quest_name: &str) {
    let di = verify_content_folder(w, session, false);

    let Some(quest) = w.content.get_cached_quest(quest_name) else {
        info(w, session, &format!("Couldn't find quest {quest_name}"));
        return;
    };

    let sql_folder = format!("{}{SEP}sql{SEP}quests{SEP}", di.full_name);

    DirectoryInfo::ensure(&sql_folder);

    let sql_filename = format!("{quest_name}.sql");

    let mut sql_file = SqlOut::new(new_line());
    QUEST_SQL_WRITER.with(|c| {
        let mut c = c.borrow_mut();
        let writer = c.get_or_insert_with(QuestSQLWriter::default);

        writer.create_sql_delete_statement(&quest, &mut sql_file);
        sql_file.write_empty_line();

        writer.create_sql_insert_statement(&quest, &mut sql_file);
    });

    write_export(w, session, &sql_folder, &sql_filename, &sql_file, Ok(()));
}

// ACE: DeveloperContentCommands.ExportSQLSpell
pub fn export_sql_spell(w: &mut World, session: Option<SessionId>, param: &str) {
    let di = verify_content_folder(w, session, false);

    let Some(spell_id) = dotnet_parse::uint_try_parse(param) else {
        info(w, session, &format!("{param} not a valid spell id"));
        return;
    };

    let Some(spell) = w.content.get_cached_spell(spell_id) else {
        info(w, session, &format!("Couldn't find spell id {spell_id}"));
        return;
    };

    let sql_folder = format!("{}{SEP}sql{SEP}spells{SEP}", di.full_name);

    DirectoryInfo::ensure(&sql_folder);

    let mut sql_file = SqlOut::new(new_line());
    let sql_filename = SPELL_SQL_WRITER.with(|c| {
        let mut c = c.borrow_mut();
        let writer = c.get_or_insert_with(SpellSQLWriter::default);

        let sql_filename = writer.get_default_file_name(&spell);

        writer.create_sql_delete_statement(&spell, &mut sql_file);
        sql_file.write_empty_line();

        writer.create_sql_insert_statement(&spell, &mut sql_file);
        sql_filename
    });

    write_export(w, session, &sql_folder, &sql_filename, &sql_file, Ok(()));
}

// ================================================================================ clearcache

/// ACE's `DeveloperContentCommands.CacheType` (`[Flags]`).
// ACE: DeveloperContentCommands.CacheType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheType(pub u32);

#[allow(non_upper_case_globals)]
impl CacheType {
    pub const None: Self = Self(0x0);
    pub const Landblock: Self = Self(0x1);
    pub const Recipe: Self = Self(0x2);
    pub const Spell: Self = Self(0x4);
    pub const Weenie: Self = Self(0x8);
    pub const WieldedTreasure: Self = Self(0x10);
    pub const All: Self = Self(0xFFFF);

    /// `Enum.HasFlag`.
    #[must_use]
    pub fn has_flag(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
}

// ACE: DeveloperContentCommands.HandleClearCache
pub fn handle_clear_cache(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let mut mode = CacheType::All;
    if !parameters.is_empty() {
        let p = parameters[0].to_lowercase();
        if p.contains("landblock") {
            mode = CacheType::Landblock;
        }
        if p.contains("recipe") {
            mode = CacheType::Recipe;
        }
        if p.contains("spell") {
            mode = CacheType::Spell;
        }
        if p.contains("weenie") {
            mode = CacheType::Weenie;
        }
        if p.contains("wield") {
            mode = CacheType::WieldedTreasure;
        }
    }

    if mode.has_flag(CacheType::Landblock) {
        info(w, session, "Clearing landblock instance cache");
        w.content.clear_cached_landblock_instances();
    }

    if mode.has_flag(CacheType::Recipe) {
        info(w, session, "Clearing recipe cache");
        w.content.clear_cookbook_cache();
    }

    if mode.has_flag(CacheType::Spell) {
        info(w, session, "Clearing spell cache");
        w.content.clear_spell_cache();
        world_object_magic::clear_spell_cache();
    }

    if mode.has_flag(CacheType::Weenie) {
        info(w, session, "Clearing weenie cache");
        w.content.clear_weenie_cache();
    }

    if mode.has_flag(CacheType::WieldedTreasure) {
        info(w, session, "Clearing wielded treasure cache");
        w.content.clear_wielded_treasure_cache();
    }
}

// ACE: DeveloperContentCommands.GetFileType
#[must_use]
pub fn get_file_type(filename: &str) -> FileType {
    let lower = filename.to_lowercase();
    if lower.ends_with(".json") {
        return get_json_file_type(filename);
    } else if lower.ends_with(".sql") {
        return get_sql_file_type(filename);
    }
    FileType::Undefined
}

// ACE: DeveloperContentCommands.GetJsonFileType
#[must_use]
pub fn get_json_file_type(filename: &str) -> FileType {
    if !Path::new(filename).is_file() {
        return FileType::Undefined;
    }

    // can possibly be indented format
    let json = read_all_text(filename);

    if json.contains("\"wcid\":") {
        FileType::Weenie
    } else if json.contains("\"recipe\":") {
        FileType::Recipe
    } else {
        FileType::Undefined
    }
}

/// The first line that is not blank decides.
///
/// Not ACE's (a fix): blank lines before it are skipped; ACE's loop
/// `continue`d on a blank first line without reading another, so it never ended and the command
/// hung.
// ACE: DeveloperContentCommands.GetSQLFileType
#[must_use]
pub fn get_sql_file_type(filename: &str) -> FileType {
    if !Path::new(filename).is_file() {
        return FileType::Undefined;
    }

    let text = read_all_text(filename);
    let Some(line) = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .find(|l| !l.trim().is_empty())
    else {
        return FileType::Undefined;
    };

    if line.contains("`encounter`") {
        FileType::Encounter
    } else if line.contains("`landblock_instance`") {
        FileType::LandblockInstance
    } else if line.contains("`quest`") {
        FileType::Quest
    } else if line.contains("`recipe`") {
        FileType::Recipe
    } else if line.contains("`spell`") {
        FileType::Spell
    } else if line.contains("`weenie`") {
        FileType::Weenie
    } else {
        FileType::Undefined
    }
}

// ================================================================================ nudge / rotate

/// `session.Player.FindObject(guid, Player.SearchLocations.Landblock)` for a guid parameter
/// (`TrimStart("0x")`, then hex), else the last appraised object.
fn target_object(
    w: &mut World,
    s: SessionId,
    parameters: &[String],
    with_guid: bool,
    cur_param: &mut usize,
) -> Option<ObjectGuid> {
    if !with_guid {
        return command_handler_helper::get_last_appraised_object(w, s);
    }
    let text = string_extensions::trim_start(&parameters[*cur_param], "0x");
    *cur_param += 1;
    let Some(guid) = dotnet_parse::uint_try_parse_hex(&text) else {
        chat(w, s, &format!("Invalid guid: {}", parameters[0]));
        return None;
    };

    let player = session_player(w, s);
    let o =
        player_inventory::find_object(w, player, ObjectGuid::new(guid), SearchLocations::Landblock)
            .result;

    if o.is_none() {
        chat(w, s, &format!("Couldn't find {}", parameters[0]));
    }
    o
}

/// The landblock-instance and physics checks of the nudge and rotate commands.
fn check_instance_object(w: &mut World, s: SessionId, o: ObjectGuid) -> bool {
    // ensure landblock instance
    if !o.is_static() {
        chat(
            w,
            s,
            &format!("{} ({o}) is not landblock instance", name_of(w, o)),
        );
        return false;
    }

    if obj(w, o).phys.is_none() {
        chat(
            w,
            s,
            &format!("{} ({o}) is not a physics object", name_of(w, o)),
        );
        return false;
    }
    true
}

// ACE: DeveloperContentCommands.HandleNudge
#[allow(clippy::too_many_lines)]
pub fn handle_nudge(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let mut cur_param = 0;

    let Some(o) = target_object(w, s, parameters, parameters.len() == 3, &mut cur_param) else {
        return;
    };

    if !check_instance_object(w, s, o) {
        return;
    }
    let h = obj(w, o).phys.expect("checked above");

    // get direction
    let dirname = parameters[cur_param].to_lowercase();
    cur_param += 1;
    let mut dir = get_nudge_dir(&dirname);

    let mut cur_pos = false;

    if dir.is_none() {
        if dirname == "here" || dirname == "to me" {
            dir = Some(Vector3::ZERO);
            cur_pos = true;
        } else {
            chat(w, s, &format!("Invalid direction: {dirname}"));
            return;
        }
    }

    // get distance / amount
    let mut amount = 1.0f32;
    if cur_param < parameters.len() {
        let Some(a) = dotnet_parse::float_try_parse(&parameters[cur_param]) else {
            // (float.TryParse's out value is 0 when it fails)
            chat(w, s, &format!("Invalid amount: {}", to_string(0.0f32)));
            return;
        };
        amount = a;
    }

    let nudge = dir.expect("set above") * amount;

    // get landblock for static guid
    #[allow(clippy::cast_possible_truncation)]
    let landblock_id = (o.full() >> 12) as u16;

    // get instances for landblock
    let mut instances: Vec<LandblockInstance> = w
        .content
        .get_cached_instances_by_landblock(landblock_id)
        .to_vec();

    // find instance
    let Some(ii) = instances.iter().position(|i| i.guid == o.full()) else {
        chat(
            w,
            s,
            &format!("Couldn't find instance for {} ({o})", name_of(w, o)),
        );
        return;
    };

    if cur_pos {
        // ensure same landblock
        let player = session_player(w, s);
        if (instances[ii].obj_cell_id >> 16) != (location_of(w, player).cell() >> 16) {
            chat(
                w,
                s,
                &format!(
                    "Failed to move {} ({o}) to current location -- different landblock",
                    name_of(w, o)
                ),
            );
            return;
        }

        obj_mut(w, o).set_ethereal(Some(true));
        world_object::enqueue_broadcast_physics_state(w, o);

        let new_loc = Position::from_position(&location_of(w, player));

        // slide?
        let set_pos = position_extensions::phys_position(&new_loc);
        if !phys_ext::set_position(w, h, &set_pos) {
            // DIVERGE: the physics layer answers success or failure, not a SetPositionError name.
            chat(
                w,
                s,
                &format!(
                    "Failed to move {} ({o}) to current location: {}",
                    name_of(w, o),
                    "GeneralFailure"
                ),
            );
            return;
        }

        let location = location_of(w, o);
        let inst = &mut instances[ii];
        inst.angles_x = location.rotation_x;
        inst.angles_y = location.rotation_y;
        inst.angles_z = location.rotation_z;
        inst.angles_w = location.rotation_w;
    } else {
        // compare current position with home position
        // the nudge should be performed as an offset from home position
        let location = location_of(w, o);
        let inst = instances[ii].clone();
        #[allow(clippy::float_cmp)]
        let moved = inst.origin_x != location.position_x
            || inst.origin_y != location.position_y
            || inst.origin_z != location.position_z;
        if moved {
            let home_pos = Position::from_components(
                inst.obj_cell_id,
                inst.origin_x,
                inst.origin_y,
                inst.origin_z,
                inst.angles_x,
                inst.angles_y,
                inst.angles_z,
                inst.angles_w,
                false,
            );

            // slide?
            let set_pos = position_extensions::phys_position(&home_pos);
            if !phys_ext::set_position(w, h, &set_pos) {
                chat(
                    w,
                    s,
                    &format!(
                        "Failed to move {} ({o}) to home position {}",
                        name_of(w, o),
                        home_pos.to_loc_string()
                    ),
                );
                return;
            }
        }

        // perform physics transition
        let current = phys_ext::position(w, h).expect("a physics body has a position");
        let mut new_pos = current;
        // (Physics.Common.Position.add_offset: Frame.Origin += offset)
        new_pos.frame.origin.x += nudge.x;
        new_pos.frame.origin.y += nudge.y;
        new_pos.frame.origin.z += nudge.z;

        let transit = phys_ext::transition(w, h, &current, &new_pos, true);

        let error_msg = format!(
            "{} ({o}) failed to move from {} to {}",
            name_of(w, o),
            position_extensions::ace_position(&current),
            position_extensions::ace_position(&new_pos)
        );

        let Some(transit) = transit else {
            chat(w, s, &error_msg);
            return;
        };

        // ensure same landblock
        if (transit.sphere_path.curr_pos.cell.0 >> 16) != (current.cell.0 >> 16) {
            chat(w, s, &format!("{error_msg} - cannot change landblock"));
            return;
        }

        phys_ext::set_position_internal(w, h, &transit);
    }

    // update ace location
    let prev_loc = Position::from_position(&location_of(w, o));
    let new_location = position_extensions::ace_position(
        &phys_ext::position(w, h).expect("a physics body has a position"),
    );
    obj_mut(w, o).set_location(Some(new_location));

    if prev_loc.landblock() != new_location.landblock() {
        landblock_manager::relocate_object_for_physics(w, o, true);
    }

    // broadcast new position
    world_object_networking::send_update_position(w, o, true);

    chat(
        w,
        s,
        &format!(
            "{} ({o}) - moved from {prev_loc} to {new_location}",
            name_of(w, o)
        ),
    );

    // update sql
    let inst = &mut instances[ii];
    inst.obj_cell_id = new_location.cell();
    inst.origin_x = new_location.position_x;
    inst.origin_y = new_location.position_y;
    inst.origin_z = new_location.position_z;

    sync_instances(w, session, landblock_id, &instances);
}

// ACE: DeveloperContentCommands.GetNudgeDir
#[must_use]
pub fn get_nudge_dir(dir: &str) -> Option<Vector3> {
    Some(match dir {
        "north" | "n" | "y" => Vector3::UNIT_Y,
        "south" | "s" => -Vector3::UNIT_Y,
        "west" | "w" => -UNIT_X,
        "east" | "e" | "x" => UNIT_X,
        "northwest" | "nw" => Vector3::normalize(Vector3::new(-1.0, 1.0, 0.0)),
        "northeast" | "ne" => Vector3::normalize(Vector3::new(1.0, 1.0, 0.0)),
        "southwest" | "sw" => Vector3::normalize(Vector3::new(-1.0, -1.0, 0.0)),
        "southeast" | "se" => Vector3::normalize(Vector3::new(1.0, -1.0, 0.0)),
        "up" | "z" => UNIT_Z,
        "down" => -UNIT_Z,
        _ => return None,
    })
}

/// The tail of `rotate` and `rotate-x/y/z`: find the instance, set the object's rotation, save
/// it, broadcast it.
fn apply_rotation(
    w: &mut World,
    session: Option<SessionId>,
    s: SessionId,
    o: ObjectGuid,
    new_rotation: Quaternion,
) {
    // get landblock for static guid
    #[allow(clippy::cast_possible_truncation)]
    let landblock_id = (o.full() >> 12) as u16;

    // get instances for landblock
    let mut instances: Vec<LandblockInstance> = w
        .content
        .get_cached_instances_by_landblock(landblock_id)
        .to_vec();

    // find instance
    let Some(ii) = instances.iter().position(|i| i.guid == o.full()) else {
        chat(
            w,
            s,
            &format!("Couldn't find instance for {} ({o})", name_of(w, o)),
        );
        return;
    };

    chat(
        w,
        s,
        &format!(
            "{} ({o}) new rotation: {}",
            name_of(w, o),
            quaternion_to_string(new_rotation)
        ),
    );

    // update physics / ace rotation
    if let Some(h) = obj(w, o).phys {
        if let Some(body) = w.physics.get_mut(h) {
            body.position.frame.rotation = dereth_primitives::Quat::new(
                new_rotation.w,
                new_rotation.x,
                new_rotation.y,
                new_rotation.z,
            );
        }
    }
    let mut location = location_of(w, o);
    location.set_rotation(new_rotation);
    obj_mut(w, o).set_location(Some(location));

    // update instance
    let inst = &mut instances[ii];
    inst.angles_w = new_rotation.w;
    inst.angles_x = new_rotation.x;
    inst.angles_y = new_rotation.y;
    inst.angles_z = new_rotation.z;

    sync_instances(w, session, landblock_id, &instances);

    // broadcast new rotation
    world_object_networking::send_update_position(w, o, true);
}

// ACE: DeveloperContentCommands.HandleRotate
pub fn handle_rotate(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let mut cur_param = 0;

    let Some(o) = target_object(w, s, parameters, parameters.len() == 2, &mut cur_param) else {
        return;
    };

    if !check_instance_object(w, s, o) {
        return;
    }

    // get direction
    let dirname = parameters[cur_param].to_lowercase();
    let mut dir = get_nudge_dir(&dirname);

    let mut cur_rotate = false;

    if dir.is_none() {
        if let Some(degrees) = dotnet_parse::float_try_parse(&dirname) {
            let rads = to_radians(degrees);
            let q = Quaternion::create_from_axis_angle(UNIT_Z, rads);
            dir = Some(Vector3::transform(Vector3::UNIT_Y, q));
        } else if dirname == "here" || dirname == "me" {
            dir = Some(Vector3::ZERO);
            cur_rotate = true;
        } else {
            chat(w, s, &format!("Invalid direction: {dirname}"));
            return;
        }
    }

    // get quaternion
    let new_rotation = if cur_rotate {
        let player = session_player(w, s);
        location_of(w, player).rotation()
    } else {
        let d = dir.expect("set above");
        let angle = empyrean_common::math::atan2(f64::from(-d.x), f64::from(d.y));
        Quaternion::create_from_axis_angle(UNIT_Z, CsCast::<f32>::cs_cast(angle))
    };

    let new_rotation = Quaternion::normalize(new_rotation);

    apply_rotation(w, session, s, o, new_rotation);
}

// ACE: DeveloperContentCommands.HandleRotateX
pub fn handle_rotate_x(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    handle_rotate_axis(w, session, UNIT_X, parameters);
}

// ACE: DeveloperContentCommands.HandleRotateY
pub fn handle_rotate_y(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    handle_rotate_axis(w, session, Vector3::UNIT_Y, parameters);
}

// ACE: DeveloperContentCommands.HandleRotateZ
pub fn handle_rotate_z(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    handle_rotate_axis(w, session, UNIT_Z, parameters);
}

// ACE: DeveloperContentCommands.HandleRotateAxis
pub fn handle_rotate_axis(
    w: &mut World,
    session: Option<SessionId>,
    axis: Vector3,
    parameters: &[String],
) {
    let s = require(session);
    let mut cur_param = 0;

    let Some(o) = target_object(w, s, parameters, parameters.len() == 2, &mut cur_param) else {
        return;
    };

    if !check_instance_object(w, s, o) {
        return;
    }

    let degrees_str = &parameters[cur_param];

    let Some(degrees) = dotnet_parse::float_try_parse(degrees_str) else {
        chat(w, s, &format!("Invalid angle: {degrees_str}"));
        return;
    };

    let rads = to_radians(degrees);
    let q = Quaternion::create_from_axis_angle(axis, rads);

    // get quaternion
    let h = obj(w, o).phys.expect("checked above");
    let r = phys_ext::position(w, h)
        .expect("a physics body has a position")
        .frame
        .rotation;
    let orientation = Quaternion::new(r.x, r.y, r.z, r.w);
    let new_rotation = Quaternion::normalize(orientation * q);

    apply_rotation(w, session, s, o, new_rotation);
}

// ================================================================================ generate-classnames

// ACE: DeveloperContentCommands.HandleGenerateClassNames
pub fn handle_generate_class_names(
    w: &mut World,
    session: Option<SessionId>,
    _parameters: &[String],
) {
    let mut lines: Vec<String> = Vec::new();

    let replace_chars = [(" ", "_"), ("-", "_"), ("!", ""), ("#", ""), ("?", "")];

    // ctx.Weenie.OrderBy(i => i.ClassId)
    let weenies = w.content.get_all_weenie_class_names();

    lines.push("namespace ACE.Server.Factories.Enum".to_owned());
    lines.push("{".to_owned());
    lines.push("    public enum WeenieClassName".to_owned());
    lines.push("    {".to_owned());
    lines.push("        undef = 0,".to_owned());

    for (class_id, class_name) in weenies.iter() {
        let mut class_name = class_name.clone();

        for (k, v) in replace_chars {
            class_name = class_name.replace(k, v);
        }

        // (className[0] throws on an empty class name)
        let first = class_name
            .chars()
            .next()
            .expect("IndexOutOfRangeException: className[0]");
        if first.is_ascii_digit() {
            class_name = format!("_{class_name}");
        }

        lines.push(format!("        {class_name} = {class_id},"));
    }

    lines.push("    }".to_owned());
    lines.push("}".to_owned());

    let filename = "WeenieClassName.cs";
    let mut path = format!("..{SEP}..{SEP}..{SEP}..{SEP}Factories{SEP}Enum{SEP}{filename}");
    if !Path::new(&path).is_file() {
        path = filename.to_owned();
    }
    let text: String = lines.iter().map(|l| format!("{l}{}", new_line())).collect();
    write_all_text(&path, &text);

    info(w, session, &format!("Wrote {path}"));
}

// ================================================================================ vloc2loc

// ACE: DeveloperContentCommands.HandleVLOCtoLOC
pub fn handle_vloc_to_loc(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let mut hex = parameters[0].as_str();

    if hex.len() >= 2
        && (hex[..2].eq_ignore_ascii_case("0x") || hex[..2].eq_ignore_ascii_case("&H"))
    {
        hex = &hex[2..];
    }

    let Some(lbid) = dotnet_parse::uint_try_parse_hex(hex) else {
        info(w, session, &format!("Invalid Landblock ID: {}\nLandblock ID should be in the hex format such as this: @vloc2loc 0xAB94", parameters[0]));
        return;
    };

    let di = verify_content_folder(w, session, true);
    if !di.exists() {
        return;
    }

    let vloc_folder = format!("{}{SEP}vlocs{SEP}", di.full_name);

    let di = DirectoryInfo {
        full_name: vloc_folder.clone(),
    };

    let vloc_db = format!("{vloc_folder}vlocDB.txt");

    let vlocs: Option<Vec<String>> = (di.exists() && Path::new(&vloc_db).is_file())
        .then(|| read_all_text(&vloc_db).lines().map(str::to_owned).collect());

    let Some(vlocs) = vlocs else {
        info(
            w,
            session,
            &format!("Unable to read VLOC database file located here: {vloc_db}"),
        );
        return;
    };

    // Name,ObjectClass,LandCell,X,Y
    // Master MacTavish,37,-114359889,97.14075000286103,-63.93749958674113

    if vlocs.is_empty() || vlocs[0] != "Name,ObjectClass,LandCell,X,Y" {
        info(
            w,
            session,
            &format!("{vloc_db} does not appear to be a valid VLOC database file."),
        );
        return;
    }

    let vloc_file = format!("{vloc_folder}{}.txt", format(lbid, "X4"));

    let _ = std::fs::remove_file(&vloc_file);

    for (i, line) in vlocs.iter().enumerate().skip(1) {
        let split: Vec<&str> = line.split(',').collect();

        // (a line with fewer than five fields throws IndexOutOfRangeException)
        let field = |n: usize| {
            split
                .get(n)
                .map(|f| f.trim())
                .unwrap_or_else(|| panic!("IndexOutOfRangeException: vlocs[{i}]"))
        };
        let name = field(0);
        let _object_class = field(1);
        let str_land_cell = field(2);
        let str_x = field(3);
        let str_y = field(4);

        let Some(land_cell) = dotnet_parse::int_try_parse(str_land_cell) else {
            info(w, session, &format!("Unable to parse LandCell ({str_land_cell}) value from line {i} in vlocDB: {line}"));
            continue;
        };
        #[allow(clippy::cast_sign_loss)]
        let obj_cell_id = land_cell as u32;
        let Some(x) = dotnet_parse::float_try_parse(str_x) else {
            info(
                w,
                session,
                &format!("Unable to parse X ({str_x}) value from line {i} in vlocDB: {line}"),
            );
            continue;
        };
        let Some(y) = dotnet_parse::float_try_parse(str_y) else {
            info(
                w,
                session,
                &format!("Unable to parse Y ({str_y}) value from line {i} in vlocDB: {line}"),
            );
            continue;
        };

        if (obj_cell_id >> 16) != lbid {
            continue;
        }

        let mut pos = Position::from_map_coordinates(Vector2::new(x, y));
        let parsed = position_extensions::adjust_map_coords(w, &mut pos).map(|()| {
            position_extensions::translate(&mut pos, obj_cell_id);
            position_extensions::find_z(w, &mut pos);
        });
        // (File.AppendText, one line per match)
        let line = match parsed {
            Ok(()) => format!("{name} - @teleloc {}{}", pos.to_loc_string(), new_line()),
            Err(_) => format!(
                "Unable to parse {name} - 0x{} {str_x}, {str_y}{}",
                format(obj_cell_id, "X8"),
                new_line()
            ),
        };
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&vloc_file)
            .unwrap_or_else(|e| panic!("IOException: {vloc_file}: {e}"));
        std::io::Write::write_all(&mut f, line.as_bytes())
            .unwrap_or_else(|e| panic!("IOException: {vloc_file}: {e}"));
    }

    if Path::new(&vloc_file).is_file() {
        info(
            w,
            session,
            &format!(
                "Successfully wrote VLOCs for 0x{} to {vloc_file}",
                format(lbid, "X4")
            ),
        );
    } else {
        info(
            w,
            session,
            &format!("No VLOCs able to be written for 0x{}", format(lbid, "X4")),
        );
    }
}
