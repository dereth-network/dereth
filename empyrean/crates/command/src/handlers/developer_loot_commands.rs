// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/DeveloperLootCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/DeveloperLootCommands.cs`.

use empyrean_entity::enums::AccessLevel;
use empyrean_net::SessionId;
use empyrean_world::factories::loot_generation_factory_test;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_manager::console_write_line;
use crate::command_parameter_helpers::dotnet_parse;
use crate::handler;

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let rows: [(CommandHandlerAttribute, NamedHandler); 2] = [
        (
            CommandHandlerAttribute::with_count(
                "testlootgen",
                AccessLevel::Admin,
                CommandHandlerFlag::ConsoleInvoke,
                1,
                "Generates Loot for testing LootFactories.  Do testlootgen -info for examples.",
                "<number of items> <loot tier> <melee, missile, caster, armor, pet, aetheria (optional)>",
            ),
            handler!(test_loot_generator),
        ),
        (
            CommandHandlerAttribute::with_count(
                "testlootgencorpse",
                AccessLevel::Admin,
                CommandHandlerFlag::ConsoleInvoke,
                1,
                "Generates Corpses for testing LootFactories",
                "<DID> <number corpses> <display table - melee, missile, caster, armor, pet, aetheria>",
            ),
            handler!(test_loot_generator_corpse),
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

/// The table options both commands take: `Ok(Some(table))`, `Ok(None)` for `-log`, `Err(())`
/// for anything else.
fn display_table_option(param: &str) -> Result<Option<String>, ()> {
    match param.to_lowercase().as_str() {
        "melee" | "missile" | "caster" | "jewelry" | "armor"
        // it appears TestLootGen -> CreateRandomLootObjects profiles do not contain any PetDevices, not sure if this is a bug?
        | "pet"
        // it appears TestLootGen -> CreateRandomLootObjects profiles do not contain any Aetheria, not sure if this is a bug?
        | "aetheria"
        | "all" | "cloak" => Ok(Some(param.to_lowercase())),
        "-log" => Ok(None),
        _ => Err(()),
    }
}

/// The optional table and `-log` parameters: `(logStats, displayTable)`, or `None` after the
/// error line was written.
fn table_and_log(parameters: &[String]) -> Option<(bool, String)> {
    let mut log_stats = false;
    let mut display_table = String::new();

    if parameters.len() > 2 {
        match display_table_option(&parameters[2]) {
            Ok(Some(table)) => display_table = table,
            Ok(None) => log_stats = true,
            Err(()) => {
                console_write_line("Invalid Table Option.  Available Tables to show are melee, missile, caster, jewelry, armor, cloak, pet, aetheria or all.");
                return None;
            }
        }
    }

    if parameters.len() > 3 {
        let log_param = parameters[3].to_lowercase();

        if log_param == "-log" {
            log_stats = true;
        } else {
            console_write_line("Invalid Option.  To log a file, use option -log");
            return None;
        }
    }

    Some((log_stats, display_table))
}

// ACE: DeveloperLootCommands.TestLootGenerator
/// `testlootgen <number of items> <loot tier> (table) (-log)`: generates loot for testing the
/// loot factories (console only).
pub fn test_loot_generator(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    if parameters[0] == "-info" {
        console_write_line(concat!(
            "Usage: \n",
            "<number of items> <loot tier> <(optional)display table - melee, missile, caster, jewelry, armor, cloak, pet, aetheria> \n",
            " Example: The following command will generate 1000 items in Tier 7 that shows the melee table\n",
            "testlootgen 1000 7 melee \n",
            " Example: The following command will generate 1000 items in Tier 6 that just shows a summary \n",
            "testlootgen 1000 6 \n",
        ));
        return;
    }

    let Some(num_items) = dotnet_parse::int_try_parse(&parameters[0]) else {
        console_write_line("Number of items is not an integer");
        return;
    };

    // (with one parameter, ACE's `parameters[1]` throws IndexOutOfRangeException)
    let Some(tier) = dotnet_parse::int_try_parse(
        parameters
            .get(1)
            .expect("IndexOutOfRangeException: parameters[1]"),
    ) else {
        console_write_line("Tier is not an integer");
        return;
    };

    if !(1..=8).contains(&tier) {
        console_write_line(&format!(
            "Tier must be 1-8.  You entered tier {tier}, which does not exist!"
        ));
        return;
    }

    let Some((log_stats, display_table)) = table_and_log(parameters) else {
        return;
    };

    let results =
        loot_generation_factory_test::test_loot_gen(w, num_items, tier, log_stats, &display_table);

    console_write_line(&results);
}

// ACE: DeveloperLootCommands.TestLootGeneratorCorpse
/// `testlootgencorpse <DID> <number corpses> (table) (-log)`: generates corpses for testing the
/// loot factories (console only).
pub fn test_loot_generator_corpse(
    w: &mut World,
    _session: Option<SessionId>,
    parameters: &[String],
) {
    if parameters[0] == "-info" {
        console_write_line(concat!(
            "Usage: \n",
            "<DID> <number corpses> <(optional)display table - melee, missile, caster, jewelry, armor, pet, aetheria> \n",
            " Example: The following command will generate 50 corpses generated from DeathTreasure DID 998 that shows the caster table\n",
            "testlootgencorpse 998 50 caster \n",
            " Example: The following command will generate 75 corpses generated from DeathTreasure DID 452 that just shows a summary \n",
            "testlootgencorpse 452 75 \n",
        ));
        return;
    }

    if parameters.len() < 2 {
        console_write_line(
            " LootFactory Simulator \n ---------------------\n Need to specify number of coprses\n",
        );
        return;
    }

    let Some(monster_did) = dotnet_parse::uint_try_parse(&parameters[0]) else {
        console_write_line(
            " LootFactory Simulator \n ---------------------\n DID specified is not an integer \n",
        );
        return;
    };

    let Some(num_items) = dotnet_parse::int_try_parse(&parameters[1]) else {
        console_write_line(" LootFactory Simulator \n ---------------------\n Invalid Parameter - Must be a number \n");
        return;
    };

    let Some((log_stats, display_table)) = table_and_log(parameters) else {
        return;
    };
    let results = loot_generation_factory_test::test_loot_gen_monster(
        w,
        monster_did,
        num_items,
        log_stats,
        &display_table,
    );

    console_write_line(&results);
}
