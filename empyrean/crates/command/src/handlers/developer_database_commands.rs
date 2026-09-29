// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/DeveloperDatabaseCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/DeveloperDatabaseCommands.cs`.
//!
//! - **The shard.** ACE's fix commands open a `ShardDbContext` and run LINQ or raw SQL over its
//!   tables. Here they read and write through empyrean-store's API on `DatabaseManager.Shard.BaseDatabase`
//!   (`w.shard.base_database()`): every character or biota by id, then the same filters in code
//!   (`empyrean_store::shard_database_offline_tools`).
//! - DIVERGE (arch): the SQL text ACE builds and executes (`fix-shortcut-bars`,
//!   `fix-gear-plating`) is still built as ACE builds it, and its effect is applied through the
//!   store (the character's shortcut rows replaced; the biota's string property set) instead of
//!   being executed.
//! - DIVERGE (arch): `fix-spell-bars` does not wait for Enter on a dry run (`Console.ReadLine()`):
//!   the command runs on the world thread, and the console's input belongs to the console thread.

use std::collections::HashSet;

use empyrean_common::dotnet::{format, DotNetDict, TimeSpan};
use empyrean_entity::enums::{
    AccessLevel, ChatMessageType, EquipMask, PropertyInstanceId, PropertyInt, PropertyString,
};
use empyrean_net::SessionId;
use empyrean_store::models::shard::CharacterPropertiesShortcutBar;
use empyrean_store::shard_database_offline_tools::{all_biotas, all_characters, save_changes};
use empyrean_world::entity::core_plating;
use empyrean_world::entity::i_player;
use empyrean_world::managers::player_manager;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_manager::{console_log_error, console_log_info, console_write_line};
use crate::command_parameter_helpers::dotnet_parse;
use crate::handler;
use crate::handlers::command_handler_helper;
use crate::handlers::processors::database_perf_test::{
    DatabasePerfTest, DEFAULT_BIOTAS_TEST_COUNT,
};

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let dev = AccessLevel::Developer;
    let admin = AccessLevel::Admin;
    let none = CommandHandlerFlag::None;
    let console = CommandHandlerFlag::ConsoleInvoke;
    let rows: [(CommandHandlerAttribute, NamedHandler); 7] = [
        (
            CommandHandlerAttribute::with_count(
                "databasequeueinfo",
                dev,
                none,
                0,
                "Show database queue information.",
                "",
            ),
            handler!(handle_database_queue_info),
        ),
        (
            CommandHandlerAttribute::with_count(
                "databaseperftest",
                dev,
                none,
                0,
                "Test server/database performance.",
                "biotasPerTest\noptional parameter biotasPerTest if omitted 1000",
            ),
            handler!(handle_database_perf_test),
        ),
        (
            CommandHandlerAttribute::with_description(
                "fix-shortcut-bars",
                admin,
                console,
                "Fixes the players with duplicate items on their shortcut bars.",
                "<execute>",
            ),
            handler!(handle_fix_shortcut_bars),
        ),
        (
            CommandHandlerAttribute::with_count(
                "database-shard-cache-pbrt",
                dev,
                none,
                0,
                "Shard Database, Player Biota Cache - Retention Time (in minutes)",
                "",
            ),
            handler!(handle_database_shard_cache_pbrt),
        ),
        (
            CommandHandlerAttribute::with_count(
                "database-shard-cache-npbrt",
                dev,
                none,
                0,
                "Shard Database, Non-Player Biota Cache - Retention Time (in minutes)",
                "",
            ),
            handler!(handle_database_shard_cache_npbrt),
        ),
        (
            CommandHandlerAttribute::with_description(
                "fix-spell-bars",
                admin,
                console,
                "Fixes the players spell bars.",
                "<execute>",
            ),
            handler!(handle_fix_spell_bars),
        ),
        (
            CommandHandlerAttribute::with_description(
                "fix-gear-plating",
                admin,
                console,
                "Corrects the name on Gear Plating.",
                "<execute>",
            ),
            handler!(handle_fix_gear_plating),
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

fn info(w: &mut World, session: Option<SessionId>, output: &str) {
    command_handler_helper::write_output_info(w, session, output, ChatMessageType::Broadcast);
}

// ACE: DeveloperDatabaseCommands.HandleDatabaseQueueInfo
/// `databasequeueinfo`: shows the database queue count and wait time.
pub fn handle_database_queue_info(
    w: &mut World,
    session: Option<SessionId>,
    _parameters: &[String],
) {
    let queue_count = w.shard.queue_count();
    info(
        w,
        session,
        &format!("Current database queue count: {queue_count}"),
    );

    w.shard
        .get_current_queue_wait_time(Some(Box::new(move |w: &mut World, result: TimeSpan| {
            info(
                w,
                session,
                &format!(
                    "Current database queue wait time: {} ms",
                    format(result.total_milliseconds(), "N0")
                ),
            );
        })));
}

// ACE: DeveloperDatabaseCommands.HandleDatabasePerfTest
/// `databaseperftest (biotasPerTest)`: tests server/database performance.
pub fn handle_database_perf_test(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let mut biotas_per_test = DEFAULT_BIOTAS_TEST_COUNT;

    if !parameters.is_empty() {
        // int.TryParse's out value is 0 when it fails
        biotas_per_test = dotnet_parse::int_try_parse(&parameters[0]).unwrap_or(0);
    }

    let processor = DatabasePerfTest::new();
    processor.run_async(w, session, biotas_per_test);
}

// ACE: DeveloperDatabaseCommands.HandleFixShortcutBars
/// `fix-shortcut-bars (execute)`: fixes players with duplicate items on their shortcut bars.
pub fn handle_fix_shortcut_bars(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    console_write_line("");

    console_write_line("This command will attempt to fix duplicate shortcuts found in player shortcut bars. Unless explictly indicated, command will dry run only");
    console_write_line("If the command outputs nothing or errors, you are ready to proceed with updating your shard db with 2019-04-17-00-Character_Shortcut_Changes.sql script");

    console_write_line("");

    let mut execute = false;

    if parameters.is_empty() {
        console_write_line("This will be a dry run and show which characters that would be affected. To perform fix, please use command: fix-shortcut-bars execute");
    } else if parameters[0].to_lowercase() == "execute" {
        execute = true;
    } else {
        console_write_line("Please use command fix-shortcut-bars execute");
    }

    // SELECT * FROM character_properties_shortcut_bar ORDER BY character_Id, shortcut_Bar_Index, id
    let characters = all_characters(&mut **w.shard.base_database());
    let mut results: Vec<CharacterPropertiesShortcutBar> = characters
        .iter()
        .flat_map(|c| c.character_properties_shortcut_bar.iter().cloned())
        .collect();
    results.sort_by_key(|r| (r.character_id, r.shortcut_bar_index));

    let mut sql_commands: Vec<String> = Vec::new();
    let mut fixes: Vec<(u32, DotNetDict<u32, u32>)> = Vec::new();

    let mut character_id = 0u32;
    let mut player_name: Option<String> = None;
    let mut idx_to_obj: DotNetDict<u32, u32> = DotNetDict::new();
    let mut obj_to_idx: DotNetDict<u32, u32> = DotNetDict::new();
    let mut bugged_char = false;
    let mut bugged_player_count = 0;

    for result in &results {
        if character_id != result.character_id {
            if bugged_char {
                bugged_player_count += 1;
                console_write_line(&format!(
                    "Player {} ({character_id}) was found to have errors in their shortcuts.",
                    player_name.as_deref().unwrap_or("")
                ));
                sql_commands.extend(output_shortcut_sql_command(
                    player_name.as_deref(),
                    character_id,
                    &idx_to_obj,
                ));
                fixes.push((character_id, idx_to_obj.clone()));
                bugged_char = false;
            }

            // begin parsing new character
            character_id = result.character_id;
            let (player, _) = player_manager::find_by_guid(w, character_id);
            player_name = Some(match player {
                Some(p) => i_player::name(w, p).unwrap_or_default(),
                None => format!("{character_id:08X}"),
            });
            idx_to_obj = DotNetDict::new();
            obj_to_idx = DotNetDict::new();
        }

        let dupe_idx = idx_to_obj.contains_key(&result.shortcut_bar_index);
        let dupe_obj = obj_to_idx.contains_key(&result.shortcut_object_id);

        if dupe_idx || dupe_obj {
            //Console.WriteLine($"Player: {playerName}, Idx: {result.ShortcutBarIndex}, Obj: {result.ShortcutObjectId:X8} ({result.Id})");
            bugged_char = true;
        }

        obj_to_idx.insert(result.shortcut_object_id, result.shortcut_bar_index);

        if !dupe_obj {
            idx_to_obj.insert(result.shortcut_bar_index, result.shortcut_object_id);
        }
    }

    if bugged_char {
        console_write_line(&format!(
            "Player {} ({character_id}) was found to have errors in their shortcuts.",
            player_name.as_deref().unwrap_or("")
        ));
        bugged_player_count += 1;
        sql_commands.extend(output_shortcut_sql_command(
            player_name.as_deref(),
            character_id,
            &idx_to_obj,
        ));
        fixes.push((character_id, idx_to_obj.clone()));
    }

    console_write_line(&format!(
        "Total players found with bugged shortcuts: {bugged_player_count}"
    ));

    if execute {
        console_write_line("Executing changes...");

        // foreach (var cmd in sqlCommands) ctx.Database.ExecuteSqlRaw(cmd): each character's
        // DELETE, then its INSERTs (see the module docs)
        let _ = sql_commands;
        let mut db = w.shard.base_database();
        for (id, idx_to_obj) in fixes {
            let Some(mut character) = characters.iter().find(|c| c.id == id).cloned() else {
                continue;
            };
            character.character_properties_shortcut_bar = idx_to_obj
                .iter()
                .map(|(&index, &object)| CharacterPropertiesShortcutBar {
                    character_id: id,
                    shortcut_bar_index: index,
                    shortcut_object_id: object,
                })
                .collect();
            character
                .character_properties_shortcut_bar
                .sort_by_key(|r| r.shortcut_bar_index);
            db.write_character(&character)
                .unwrap_or_else(|e| panic!("{e}"));
        }
    } else {
        console_write_line(
            "dry run completed. Use fix-shortcut-bars execute to actually run command",
        );
    }
}

// ACE: DeveloperDatabaseCommands.OutputShortcutSQLCommand
/// The SQL that replaces a character's shortcut bar with `idx_to_obj`.
#[must_use]
pub fn output_shortcut_sql_command(
    _player_name: Option<&str>,
    character_id: u32,
    idx_to_obj: &DotNetDict<u32, u32>,
) -> Vec<String> {
    let mut strings = vec![format!(
        "DELETE FROM `character_properties_shortcut_bar` WHERE `character_Id`={character_id};"
    )];

    for (key, value) in idx_to_obj.iter() {
        strings.push(format!(
            "INSERT INTO `character_properties_shortcut_bar` SET `character_Id`={character_id}, `shortcut_Bar_Index`={key}, `shortcut_Object_Id`={value};"
        ));
    }

    strings
}

/// `DatabaseManager.Shard.BaseDatabase is ShardDatabaseWithCaching`'s retention time for the
/// player (`true`) or non-player biota cache: its minutes (`get`), after setting it to `set`.
fn retention_minutes(w: &World, player: bool, set: Option<i32>) -> Option<f64> {
    let mut db = w.shard.base_database();
    let times = db.cache_retention_times()?;
    let time = if player {
        times.player_biota_retention_time
    } else {
        times.non_player_biota_retention_time
    };
    if let Some(value) = set {
        *time = TimeSpan::from_minutes(f64::from(value));
    }
    Some(time.total_minutes())
}

/// The body of both `database-shard-cache-*` commands.
fn handle_database_shard_cache_retention(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
    player: bool,
) {
    let label = if player { "Player" } else { "Non-Player" };

    let Some(current) = retention_minutes(w, player, None) else {
        info(
            w,
            session,
            "DatabaseManager is not using ShardDatabaseWithCaching",
        );

        return;
    };

    if parameters.is_empty() {
        info(
            w,
            session,
            &format!(
                "Shard Database, {label} Biota Cache - Retention Time {} m",
                format(current, "N0")
            ),
        );

        return;
    }

    let Some(value) = dotnet_parse::int_try_parse(&parameters[0]).filter(|v| *v >= 0) else {
        info(
            w,
            session,
            "Unable to parse argument. Specify retention time in integer minutes.",
        );

        return;
    };

    let updated = retention_minutes(w, player, Some(value)).unwrap_or(current);

    info(
        w,
        session,
        &format!(
            "Shard Database, {label} Biota Cache - Retention Time {} m",
            format(updated, "N0")
        ),
    );
}

// ACE: DeveloperDatabaseCommands.HandleDatabaseShardCachePBRT
/// `database-shard-cache-pbrt (minutes)`: the shard's player biota cache retention time.
pub fn handle_database_shard_cache_pbrt(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    handle_database_shard_cache_retention(w, session, parameters, true);
}

// ACE: DeveloperDatabaseCommands.HandleDatabaseShardCacheNPBRT
/// `database-shard-cache-npbrt (minutes)`: the shard's non-player biota cache retention time.
pub fn handle_database_shard_cache_npbrt(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    handle_database_shard_cache_retention(w, session, parameters, false);
}

// ACE: DeveloperDatabaseCommands.HandleFixSpellBars
/// `fix-spell-bars (execute)`: renumbers each spell bar's indexes from 1.
pub fn handle_fix_spell_bars(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    console_write_line("");

    console_write_line("This command will attempt to fix player spell bars. Unless explictly indicated, command will dry run only");
    console_write_line("You must have executed 2020-04-11-00-Update-Character-SpellBars.sql script first before running this command");

    console_write_line("");

    let mut execute = false;

    if parameters.is_empty() {
        console_write_line("This will be a dry run and show which characters that would be affected. To perform fix, please use command: fix-spell-bars execute");
    } else if parameters[0].to_lowercase() == "execute" {
        execute = true;
    } else {
        console_write_line("Please use command fix-spell-bars execute");
    }

    if !execute {
        console_write_line("");
        console_write_line("Press enter to start.");
        // Console.ReadLine(): see the module docs
    }

    let mut number_of_records_fixed = 0;

    console_log_info("Starting FixSpellBarsPR2918 process. This could take a while...");

    let mut characters = all_characters(&mut **w.shard.base_database());

    let character_spell_bars_not_fixed = characters
        .iter()
        .flat_map(|c| c.character_properties_spell_bar.iter())
        .filter(|c| c.spell_bar_number == 0)
        .count();

    if character_spell_bars_not_fixed > 0 {
        log::warn!("2020-04-11-00-Update-Character-SpellBars.sql patch not yet applied. Please apply this patch ASAP! Skipping FixSpellBarsPR2918 for now...");
        console_log_error("2020-04-11-00-Update-Character-SpellBars.sql patch not yet applied. You must apply this patch before proceeding further...");
        return;
    }

    // OrderBy(CharacterId).ThenBy(SpellBarNumber).ThenBy(SpellBarIndex): characters come by id
    let mut character_id = 0u32;
    let mut spell_bar_number = 0u32;
    let mut spell_bar_index = 0u32;
    let mut changed: HashSet<u32> = HashSet::new();

    for character in &mut characters {
        character
            .character_properties_spell_bar
            .sort_by_key(|e| (e.spell_bar_number, e.spell_bar_index));
        for entry in &mut character.character_properties_spell_bar {
            if entry.character_id != character_id {
                character_id = entry.character_id;
                spell_bar_index = 0;
            }

            if entry.spell_bar_number != spell_bar_number {
                spell_bar_number = entry.spell_bar_number;
                spell_bar_index = 0;
            }

            spell_bar_index += 1;

            if entry.spell_bar_index != spell_bar_index {
                console_write_line(&format!(
                    "FixSpellBarsPR2918: Character 0x{:08X}, SpellBarNumber = {} | SpellBarIndex = {:03}; Fixed - {spell_bar_index:03}",
                    entry.character_id, entry.spell_bar_number, entry.spell_bar_index
                ));
                entry.spell_bar_index = spell_bar_index;
                number_of_records_fixed += 1;
                changed.insert(entry.character_id);
            } else {
                console_write_line(&format!(
                    "FixSpellBarsPR2918: Character 0x{:08X}, SpellBarNumber = {} | SpellBarIndex = {:03}; OK",
                    entry.character_id, entry.spell_bar_number, entry.spell_bar_index
                ));
            }
        }
    }

    // Save
    if execute {
        console_write_line("Saving changes...");
        let mut db = w.shard.base_database();
        save_changes(&mut **db, |db| {
            for c in characters.iter().filter(|c| changed.contains(&c.id)) {
                db.write_character(c)?;
            }
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));
        drop(db);
        console_log_info(&format!(
            "Fixed {} CharacterPropertiesSpellBar records.",
            format(number_of_records_fixed, "N0")
        ));
    } else {
        console_write_line(&format!(
            "{} CharacterPropertiesSpellBar records need to be fixed!",
            format(number_of_records_fixed, "N0")
        ));
        console_write_line("dry run completed. Use fix-spell-bars execute to actually run command");
    }
}

// ACE: DeveloperDatabaseCommands.HandleFixGearPlating
/// Will display and optionally rename Gear Plated items that have an incorrect
/// `PropertyString.GearPlatingName` value. The logic to deduce the GearPlatingName was updated in
/// early 2025. This should only be needed to be run once, if at all.
pub fn handle_fix_gear_plating(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    console_write_line("");

    console_write_line("This command will attempt to correct the names on Gear Plated items. Unless explictly indicated, command will dry run only.");

    console_write_line("");

    let mut execute = false;

    if parameters.is_empty() {
        console_write_line("This will be a dry run and show which characters that would be affected. To perform fix, please use command: \"fix-gear-plating execute\"");
    } else if parameters[0].to_lowercase() == "execute" {
        execute = true;
    } else {
        console_write_line("Please use command \"fix-gear-plating execute\"");
    }

    console_write_line("");
    let mut sql_commands: Vec<String> = Vec::new();
    let mut updates: Vec<(u32, String)> = Vec::new();

    // SELECT s.object_Id, c.name, s.value as itemName, s2.value as gearPlatingName, i.value as locations
    // from `character` as c, biota_properties_i_i_d as iid, biota_properties_int as i, biota_properties_string as s, biota_properties_string as s2
    // where c.id = iid.value and iid.`type` = 1 and s.type = 1 and s2.type = 52 and i.`type` = 9 and ...same object_Id
    let (characters, biotas) = {
        let mut db = w.shard.base_database();
        let characters = all_characters(&mut **db);
        (characters, all_biotas(&mut **db))
    };

    for biota in &biotas {
        let Some(owner) = biota
            .biota_properties_iid
            .iter()
            .find(|r| r.r#type == PropertyInstanceId::Owner.0)
            .map(|r| r.value)
        else {
            continue;
        };
        let Some(character) = characters.iter().find(|c| c.id == owner) else {
            continue;
        };
        let string = |t: PropertyString| {
            biota
                .biota_properties_string
                .iter()
                .find(|r| r.r#type == t.0)
                .map(|r| r.value.clone())
        };
        let (Some(item_name), Some(gear_plating_name)) = (
            string(PropertyString::Name),
            string(PropertyString::GearPlatingName),
        ) else {
            continue;
        };
        let Some(locations) = biota
            .biota_properties_int
            .iter()
            .find(|r| r.r#type == PropertyInt::ValidLocations.0)
            .map(|r| r.value)
        else {
            continue;
        };

        let object_id = biota.id;
        let character_name = &character.name;

        // Check if the name matches what it should. Note this wi
        let new_gear_plating_name =
            core_plating::get_gear_plating_name(EquipMask(locations.cast_unsigned()));
        if new_gear_plating_name != gear_plating_name {
            let update_sql = format!(
                "UPDATE `biota_properties_string` SET `value` = '{new_gear_plating_name}' WHERE `biota_properties_string`.`object_Id` = {object_id} AND `biota_properties_string`.`type` = 52;"
            );
            sql_commands.push(update_sql);
            updates.push((object_id, new_gear_plating_name.clone()));
            console_write_line(&format!("Char: {character_name} - Change `{item_name}` from \"{gear_plating_name}\" to \"{new_gear_plating_name}\""));
        }
    }
    console_write_line("");
    console_write_line(&format!(
        " -- There are {} items that have incorrect Gear Plating Name values. --",
        sql_commands.len()
    ));
    console_write_line("");

    if execute {
        console_write_line("Executing changes...");

        // foreach (var cmd in sqlCommands) ctx.Database.ExecuteSqlRaw(cmd): see the module docs
        let mut db = w.shard.base_database();
        for (object_id, value) in updates {
            let Some(mut biota) = biotas.iter().find(|b| b.id == object_id).cloned() else {
                continue;
            };
            for row in biota
                .biota_properties_string
                .iter_mut()
                .filter(|r| r.r#type == PropertyString::GearPlatingName.0)
            {
                row.value.clone_from(&value);
            }
            db.write_biota(&mut biota).unwrap_or_else(|e| panic!("{e}"));
        }
        drop(db);

        console_write_line("Finished.");
    } else {
        console_write_line(
            "Dry run completed. Use \"fix-gear-plating execute\" to actually run command",
        );
    }
}
