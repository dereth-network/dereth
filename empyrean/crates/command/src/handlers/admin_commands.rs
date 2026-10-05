// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/AdminCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/AdminCommands.cs`.
//!
//! - **Selection.** Many commands act on "the selected object": `HealthQueryTarget`, else
//!   `ManaQueryTarget`, else `CurrentAppraisalTarget` ([`selected_object_id`]).
//! - **Callbacks.** The shard calls (`SaveBiota`, `GetCharacter`, `IsCharacterNameAvailable`,
//!   `AddCharacterInParallel`) take callbacks that run later on the world thread, as ACE's do.
//! - **Exceptions.** A C# exception inside a handler is a panic here; the command manager catches
//!   and logs it where ACE does. A `try`/`catch` inside a handler catches the panic of the code
//!   it wraps.
//! - **Local time.** DIVERGE: ACE's `ToLocalTime()` and `DateTime.Now` are the host's local time;
//!   the world has only its UTC clock, so those read as UTC (as in `server_manager`).
//!
//! Callees that are not ported yet are private pointer functions at the bottom of this file, each
//! a `not_ported!` named after its ACE member.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

use empyrean_common::dotnet::{format, format_aligned, to_string, CsCast, DotNetDateTime};
use empyrean_common::extensions::date_time_extensions::to_common_string;
use empyrean_entity::enums::ext::skill_helper;
use empyrean_entity::enums::{
    AccessLevel, AceEnum, Channel, ChatMessageType, DestinationType, EnvironChangeType,
    MaterialType, PKLevel, PlayerKillerStatus, PositionType, PropertyAttribute,
    PropertyAttribute2nd, PropertyInt, Skill, SkillAdvancementClass, SpellId, WeenieType,
};
use empyrean_entity::enums::{
    HouseStatus, HouseType, PhysicsState, PlayScript, PropertyBool, PropertyDataId,
    PropertyInstanceId, PropertyInt64, PropertyString, Sound, WeenieClassName,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_store::adapter::BiotaConverter;
use empyrean_store::entity::possessed_biotas::PossessedBiotas;
use empyrean_store::models::shard::{self, Character};
use empyrean_world::dispatch;
use empyrean_world::entity::actions::action_chain::ActionChain;
use empyrean_world::entity::actions::i_actor::Actor;
use empyrean_world::entity::i_player;
use empyrean_world::entity::landblock;
use empyrean_world::entity::spell::Spell;
use empyrean_world::entity::timers;
use empyrean_world::managers::{guid_manager, player_manager, property_manager, quest_manager};
use empyrean_world::network::game_event::events::game_event_item_server_says_move_item::game_event_item_server_says_move_item;
use empyrean_world::network::game_event::game_event_message::session_data;
use empyrean_world::network::game_messages::game_message::{enqueue_send, GameMessage};
use empyrean_world::network::game_messages::messages::game_message_delete_object::game_message_delete_object;
use empyrean_world::network::game_messages::messages::game_message_private_update_attribute::game_message_private_update_attribute;
use empyrean_world::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use empyrean_world::network::game_messages::messages::game_message_private_update_property_int64::game_message_private_update_property_int64;
use empyrean_world::network::game_messages::messages::game_message_private_update_skill::game_message_private_update_skill;
use empyrean_world::network::game_messages::messages::game_message_private_update_vital::game_message_private_update_vital;
use empyrean_world::network::game_messages::messages::game_message_public_update_instance_id::game_message_public_update_instance_id;
use empyrean_world::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use empyrean_world::network::game_messages::messages::game_message_sound::game_message_sound;
use empyrean_world::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use empyrean_world::network::game_messages::messages::game_message_update_position::game_message_update_position;
use empyrean_world::physics::{object_maint, phys_ext};
use empyrean_world::world_objects::entity::creature_attribute::StatCtx;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::managers::enchantment_manager;
use empyrean_world::world_objects::player_inventory::{self, SearchLocations};
use empyrean_world::world_objects::world_object::{self, CtorEnv, WorldObject};
use empyrean_world::world_objects::{
    container, creature_death, creature_equipment, player_location, player_skills,
    world_object_networking, world_object_tick,
};
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_manager::console_write_line;
use crate::command_parameter_helpers::dotnet_parse;
use crate::handler;
use crate::handler_common::{bool_string, name_of, obj, obj_mut, session_player, system_chat};
use crate::handlers::command_handler_helper::{self, send_server_message};

/// This file's `[CommandHandler]` decorations, in declaration order (`knownobjs` is replaced in
/// its slot by DeveloperCommands' own, registered later).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let none = CommandHandlerFlag::None;
    let console = CommandHandlerFlag::ConsoleInvoke;
    let world = CommandHandlerFlag::RequiresWorld;
    let rows: [(CommandHandlerAttribute, NamedHandler); 104] = [
        (CommandHandlerAttribute::new("adminvision", AccessLevel::Sentinel, world, false, 1, "Allows the admin to see admin-only visible items.", "{ on | off | toggle | check }\nControls whether or not the admin can see admin-only visible items. Note that if you turn this feature off, you will need to log out and back in before the visible items become invisible."), handler!(handle_adminvision)),
        (CommandHandlerAttribute::new("adminui", AccessLevel::Developer, world, false, 0, "", ""), handler!(handle_adminui)),
        (CommandHandlerAttribute::new("delete", AccessLevel::Envoy, world, false, 0, "Deletes the selected object.", "Players may not be deleted this way."), handler!(handle_delete_selected)),
        (CommandHandlerAttribute::new("draw", AccessLevel::Developer, world, false, 0, "", ""), handler!(handle_draw)),
        (CommandHandlerAttribute::new("finger", AccessLevel::Sentinel, none, false, 1, "Show the given character's account name or vice-versa.", "[ [-a] character] [-m account]\nGiven a character name, this command displays the name of the owning account.\nIf the -m option is specified, the argument is considered an account name and the characters owned by that account are displayed.\nIf the -a option is specified, then the character name is fingered but their account is implicitly fingered as well."), handler!(handle_finger)),
        (CommandHandlerAttribute::new("freeze", AccessLevel::Sentinel, world, false, 0, "", ""), handler!(handle_freeze)),
        (CommandHandlerAttribute::new("unfreeze", AccessLevel::Sentinel, world, false, 0, "", ""), handler!(handle_un_freeze)),
        (CommandHandlerAttribute::new("gag", AccessLevel::Sentinel, world, false, 1, "Prevents a character from talking.", "< char name >\nThe character will not be able to @tell or use chat normally."), handler!(handle_gag)),
        (CommandHandlerAttribute::new("ungag", AccessLevel::Sentinel, world, false, 1, "Allows a gagged character to talk again.", "< char name >\nThe character will again be able to @tell and use chat normally."), handler!(handle_un_gag)),
        (CommandHandlerAttribute::new("home", AccessLevel::Sentinel, world, false, 0, "Teleports you to your sanctuary position.", "< recall number > - Recalls to a saved position, valid values are 1 - 9.\nNOTE: Calling @home without a number recalls your sanctuary position; calling it with a number will teleport you to the corresponding saved position."), handler!(handle_home)),
        (CommandHandlerAttribute::new("mrt", AccessLevel::Sentinel, world, false, 0, "Toggles the ability to bypass housing boundaries", ""), handler!(handle_mrt)),
        (CommandHandlerAttribute::new("limbo", AccessLevel::Sentinel, world, false, 0, "", ""), handler!(handle_limbo)),
        (CommandHandlerAttribute::new("myiid", AccessLevel::Envoy, world, false, 0, "Displays your Instance ID (IID)", ""), handler!(handle_my_iid)),
        (CommandHandlerAttribute::new("myserver", AccessLevel::Envoy, world, false, 0, "", ""), handler!(handle_my_server)),
        (CommandHandlerAttribute::new("pk", AccessLevel::Developer, world, false, 0, "sets your own PK state.", "< npk / pk / pkl / free >\nThis command sets your current player killer state\nThis command also expects to be used '@cloak player' to properly reflect to other players\n< npk > You can only attack monsters.\n< pk > You can attack player killers and monsters.\n< pkl > You can attack player killer lites and monsters\n< free > You can attack players, player killers, player killer lites, monsters, and npcs"), handler!(handle_pk)),
        (CommandHandlerAttribute::new("querypluginlist", AccessLevel::Envoy, world, false, 0, "", ""), handler!(handle_querypluginlist)),
        (CommandHandlerAttribute::new("queryplugin", AccessLevel::Envoy, world, false, 1, "", ""), handler!(handle_queryplugin)),
        (CommandHandlerAttribute::new("repeat", AccessLevel::Sentinel, world, false, 2, "", ""), handler!(handle_repeat)),
        (CommandHandlerAttribute::new("regen", AccessLevel::Envoy, world, false, 0, "Sends the selected generator a regeneration message.", ""), handler!(handle_regen)),
        (CommandHandlerAttribute::new("save", AccessLevel::Sentinel, world, false, 0, "Sets your sanctuary position or a named recall point.", "< recall number > - Saves your position into the numbered recall, valid values are 1 - 9.\nNOTE: Calling @save without a number saves your sanctuary (Lifestone Recall) position."), handler!(handle_save)),
        (CommandHandlerAttribute::new("serverlist", AccessLevel::Envoy, world, false, 0, "", ""), handler!(handle_serverlist)),
        (CommandHandlerAttribute::new("snoop", AccessLevel::Envoy, world, false, 2, "", ""), handler!(handle_snoop)),
        (CommandHandlerAttribute::new("smite", AccessLevel::Envoy, world, false, 0, "Kills the selected target or all monsters in radar range if \"all\" is specified.", "[all, Player's Name]"), handler!(handle_smite)),
        (CommandHandlerAttribute::new("teleto", AccessLevel::Sentinel, world, false, 1, "Teleport yourself to a player", "[Player's Name]\n"), handler!(handle_teleto)),
        (CommandHandlerAttribute::new("teletome", AccessLevel::Sentinel, world, false, 1, "Teleports a player to your current location.", "PlayerName"), handler!(handle_tele_to_me)),
        (CommandHandlerAttribute::new("telereturn", AccessLevel::Sentinel, world, false, 1, "Return a player to their previous location.", "PlayerName"), handler!(handle_tele_return)),
        (CommandHandlerAttribute::new("teleallto", AccessLevel::Developer, world, false, 0, "Teleports all players to a player. If no target is specified, all players will be teleported to you.", "[Player's Name]\n"), handler!(handle_tele_all_to)),
        (CommandHandlerAttribute::new("telepoi", AccessLevel::Developer, world, false, 1, "Teleport yourself to a named Point of Interest", "[POI|list]\n@telepoi Arwic\nGet the list of POIs\n@telepoi list"), handler!(handle_teleport_poi)),
        (CommandHandlerAttribute::new("teleloc", AccessLevel::Developer, world, false, 4, "Teleport yourself to the specified location.", "cell [x y z] (qw qx qy qz)\n@teleloc follows the same number order as displayed from @loc output\nExample: @teleloc 0x7F0401AD [12.319900 -28.482000 0.005000] -0.338946 0.000000 0.000000 -0.940806\nExample: @teleloc 0x7F0401AD 12.319900 -28.482000 0.005000 -0.338946 0.000000 0.000000 -0.940806\nExample: @teleloc 7F0401AD 12.319900 -28.482000 0.005000"), handler!(handle_teleport_loc)),
        (CommandHandlerAttribute::new("time", AccessLevel::Envoy, none, false, 0, "Displays the server's current game time.", ""), handler!(handle_time)),
        (CommandHandlerAttribute::new("trophies", AccessLevel::Envoy, world, false, 0, "Shows a list of the trophies dropped by the target creature, and the percentage chance of dropping.", ""), handler!(handle_trophies)),
        (CommandHandlerAttribute::new("unlock", AccessLevel::Envoy, world, false, 1, "", ""), handler!(handle_unlock)),
        (CommandHandlerAttribute::new("gamecast", AccessLevel::Envoy, none, false, 1, "Sends a world-wide broadcast.", "<message>\nThis command sends a world-wide broadcast to everyone in the game. Text is prefixed with 'Broadcast from (admin-name)> '.\nSee Also: @gamecast, @gamecastemote, @gamecastlocal, @gamecastlocalemote."), handler!(handle_gamecast)),
        (CommandHandlerAttribute::new("addspell", AccessLevel::Developer, world, false, 1, "Adds the specified spell to your own spellbook.", "<spellid>"), handler!(handle_add_spell)),
        (CommandHandlerAttribute::new("removespell", AccessLevel::Developer, world, false, 1, "Removes the specified spell to your own spellbook.", "<spellid>"), handler!(handle_remove_spell)),
        (CommandHandlerAttribute::new("adminhouse", AccessLevel::Admin, world, false, 0, "House management tools for admins.", ""), handler!(handle_adminhouse)),
        (CommandHandlerAttribute::new("bornagain", AccessLevel::Admin, world, false, 1, "Restores a deleted character to an account.", "deletedCharID(, newCharName)(, accountName)\nGiven the ID of a deleted character, this command restores that character to its owner.  (You can find the ID of a deleted character using the @finger command.)\nIf the deleted character's name has since been taken by a new character, you can specify a new name for the restored character as the second parameter.  (You can find if the name has been taken by also using the @finger command.)  Use a comma to separate the arguments.\nIf needed, you can specify an account name as a third parameter if the character should be restored to an account other than its original owner.  Again, use a comma between the arguments."), handler!(handle_born_again)),
        (CommandHandlerAttribute::new("copychar", AccessLevel::Developer, world, false, 2, "Copies an existing character into your character list.", "< Existing Character Name >, < New Character Name >\nGiven the name of an existing character \"character name\", this command makes a copy of that character with the name \"copy name\" and places it into your character list."), handler!(handle_copychar)),
        (CommandHandlerAttribute::new("create", AccessLevel::Developer, world, false, 1, "Creates an object or objects in the world.", "<wcid or classname> (amount) (palette) (shade)\nThis will attempt to spawn the weenie you specify. If you include an amount to spawn, it will attempt to create that many of the object.\nStackable items will spawn in stacks of their max stack size. All spawns will be limited by the physics engine placement, which may prevent the number you specify from actually spawning.Be careful with large numbers, especially with ethereal weenies."), handler!(handle_create)),
        (CommandHandlerAttribute::new("createliveops", AccessLevel::Developer, world, false, 1, "Creates an object or objects with lifespans in the world for live events.", "<wcid or classname> (amount) (lifespan) (palette) (shade)\nThis will attempt to spawn the weenie you specify. If you include an amount to spawn, it will attempt to spawn that many of the object.\nStackable items will spawn in stacks of their max stack size. All spawns will be limited by the physics engine placement, which may prevent the number you specify from actually spawning.\nBe careful with large numbers, especially with ethereal weenies.\nIf you include a lifespan, this value is in seconds, and defaults to 3600 (1 hour) if not specified."), handler!(handle_create_live_ops)),
        (CommandHandlerAttribute::new("createnamed", AccessLevel::Developer, world, false, 3, "Creates a named object in the world.", "<wcid or classname> <count> <name>"), handler!(handle_create_named)),
        (CommandHandlerAttribute::new("ci", AccessLevel::Developer, world, false, 1, "Creates an object in your inventory.", "wclassid (string or number), Amount to Spawn (optional [default:1]), Palette (optional), Shade (optional)\n"), handler!(handle_ci)),
        (CommandHandlerAttribute::new("crack", AccessLevel::Envoy, world, false, 0, "Cracks the most recently appraised locked target.", "[. open it too]"), handler!(handle_crack)),
        (CommandHandlerAttribute::new("deathxp", AccessLevel::Developer, world, false, 0, "Displays how much experience the last appraised creature is worth when killed.", ""), handler!(handle_deathxp)),
        (CommandHandlerAttribute::new("de_n", AccessLevel::Developer, world, false, 2, "Sends text to named player, formatted exactly as entered.", "<name>, <text>"), handler!(handlede_n)),
        (CommandHandlerAttribute::new("direct_emote_name", AccessLevel::Developer, world, false, 2, "Sends text to named player, formatted exactly as entered.", "<name>, <text>"), handler!(handledirect_emote_name)),
        (CommandHandlerAttribute::new("de_s", AccessLevel::Developer, world, false, 1, "Sends text to selected player, formatted exactly as entered, with no prefix of any kind.", "<text>"), handler!(handlede_s)),
        (CommandHandlerAttribute::new("direct_emote_select", AccessLevel::Developer, world, false, 1, "Sends text to selected player, formatted exactly as entered, with no prefix of any kind.", "<text>"), handler!(handledirect_emote_select)),
        (CommandHandlerAttribute::new("dispel", AccessLevel::Developer, world, false, 0, "Removes all enchantments from the player", "/dispel"), handler!(handle_dispel)),
        (CommandHandlerAttribute::new("event", AccessLevel::Developer, none, false, 2, "Maniuplates the state of an event", "[ start | stop | disable | enable | clear | status ] (name)\n@event clear < name > - clears event with name <name> or all events if you put in 'all' (All clears registered generators, <name> does not)\n@event status <eventSubstring> - get the status of all registered events or get all of the registered events that have <eventSubstring> in the name."), handler!(handle_event)),
        (CommandHandlerAttribute::new("fumble", AccessLevel::Developer, world, false, 0, "", ""), handler!(handle_fumble)),
        (CommandHandlerAttribute::new("god", AccessLevel::Sentinel, world, false, 0, "Turns current character into a god!", "Sets attributes and skills to higher than max levels.\nTo return to a mortal state, use the /ungod command.\nUse the /god command with caution. While unlikely, there is a possibility that the character that runs the command will not be able to return to normal or will end up in a state that is unrecoverable."), handler!(handle_god)),
        (CommandHandlerAttribute::new("ungod", AccessLevel::Sentinel, world, false, 0, "Returns character to a mortal state.", "Sets attributes and skills back to the values they were when you became a god.\nIf the command fails to revert your state it will default to godmode.\nIn the event of failure, contact a server administrator to sort it out."), handler!(handle_ungod)),
        (CommandHandlerAttribute::new("magic god", AccessLevel::Developer, world, false, 0, "", ""), handler!(handle_magic_god)),
        (CommandHandlerAttribute::new("modifyvital", AccessLevel::Admin, none, false, 2, "Adjusts the maximum vital attribute for the last appraised mob/player and restores full vitals", "<Health|Stamina|Mana> <delta>"), handler!(handle_modify_vital)),
        (CommandHandlerAttribute::new("modifyskill", AccessLevel::Admin, none, false, 2, "Adjusts the skill for the last appraised mob/player", "<skillName> <delta>"), handler!(handle_modify_skill)),
        (CommandHandlerAttribute::new("modifyattr", AccessLevel::Admin, none, false, 2, "Adjusts an attribute for the last appraised mob/NPC/player", "<attribute> <delta>"), handler!(handle_modify_attribute)),
        (CommandHandlerAttribute::new("heal", AccessLevel::Envoy, world, false, 0, "Heals yourself (or the selected creature)", "\nThis command fully restores your(or the selected creature's) health, mana, and stamina"), handler!(handle_heal)),
        (CommandHandlerAttribute::new("housekeep", AccessLevel::Developer, world, false, 0, "", ""), handler!(handle_housekeep)),
        (CommandHandlerAttribute::new("idlist", AccessLevel::Developer, world, false, 0, "Shows the next ID that will be allocated from GuidManager.", ""), handler!(handle_i_dlist)),
        (CommandHandlerAttribute::new("gamecastlocalemote", AccessLevel::Developer, none, false, 1, "Sends text to all players within chat range, formatted exactly as entered.", "<message>\nSends text to all players within chat range, formatted exactly as entered, with no prefix of any kind.\nSee Also: @gamecast, @gamecastemote, @gamecastlocal, @gamecastlocalemote."), handler!(handle_game_cast_local_emote)),
        (CommandHandlerAttribute::new("location", AccessLevel::Developer, world, false, 0, "", ""), handler!(handle_location)),
        (CommandHandlerAttribute::new("morph", AccessLevel::Developer, world, false, 1, "Morphs your bodily form into that of the specified creature. Be careful with this one!", "<wcid or weenie class name> [character name]"), handler!(handle_morph)),
        (CommandHandlerAttribute::new("qst", AccessLevel::Developer, world, false, 1, "Query, stamp, and erase quests on the targeted player", "(fellow) [list | bestow | erase]\nqst list - List the quest flags for the targeted player\nqst bestow - Stamps the specific quest flag on the targeted player. If this fails, it's probably because you spelled the quest flag wrong.\nqst stamp - Stamps the specific quest flag on the targeted player the specified number of times. If this fails, it's probably because you spelled the quest flag wrong.\nqst erase - Erase the specific quest flag from the targeted player. If no quest flag is given, it erases the entire quest table for the targeted player.\n"), handler!(handleqst)),
        (CommandHandlerAttribute::new("raise", AccessLevel::Developer, world, false, 2, "", ""), handler!(handle_raise)),
        (CommandHandlerAttribute::new("rename", AccessLevel::Envoy, none, false, 2, "Rename a character. (Do NOT include +'s for admin names)", "< Current Name >, < New Name >"), handler!(handle_rename)),
        (CommandHandlerAttribute::new("setadvclass", AccessLevel::Developer, world, false, 2, "", ""), handler!(handlesetadvclass)),
        (CommandHandlerAttribute::new("spendxp", AccessLevel::Developer, world, false, 2, "", ""), handler!(handle_spendxp)),
        (CommandHandlerAttribute::new("trainskill", AccessLevel::Developer, world, false, 1, "", ""), handler!(handletrainskill)),
        (CommandHandlerAttribute::new("reloadsysmsg", AccessLevel::Developer, world, false, 0, "", ""), handler!(handlereloadsysmsg)),
        (CommandHandlerAttribute::new("gamecastlocal", AccessLevel::Developer, none, false, 1, "Sends a server-wide broadcast.", "<message>\nThis command sends the specified text to every player on the current server.\nSee Also: @gamecast, @gamecastemote, @gamecastlocal, @gamecastlocalemote."), handler!(handle_game_cast_local)),
        (CommandHandlerAttribute::new("sticky", AccessLevel::Developer, world, false, -1, "Sets whether you lose items should you die.", "<off/on>"), handler!(handle_sticky)),
        (CommandHandlerAttribute::new("userlimit", AccessLevel::Admin, world, false, 1, "", ""), handler!(handleuserlimit)),
        (CommandHandlerAttribute::new("watchmen", AccessLevel::Admin, none, false, 0, "Displays a list of accounts with the specified level of admin access.", "(accesslevel)"), handler!(handlewatchmen)),
        (CommandHandlerAttribute::new("gamecastemote", AccessLevel::Developer, none, false, 1, "Sends text to all players, formatted exactly as entered.", "<message>\nSee Also: @gamecast, @gamecastemote, @gamecastlocal, @gamecastlocalemote."), handler!(handle_game_cast_emote)),
        (CommandHandlerAttribute::new("we", AccessLevel::Developer, none, false, 1, "Sends text to all players, formatted exactly as entered.", "<message>\nSee Also: @gamecast, @gamecastemote, @gamecastlocal, @gamecastlocalemote."), handler!(handle_we)),
        (CommandHandlerAttribute::new("dumpattackers", AccessLevel::Developer, world, false, 0, "", ""), handler!(handledumpattackers)),
        (CommandHandlerAttribute::new("knownobjs", AccessLevel::Developer, world, false, 0, "", ""), handler!(handleknownobjs)),
        (CommandHandlerAttribute::new("lbinterval", AccessLevel::Admin, world, false, 1, "", ""), handler!(handlelbinterval)),
        (CommandHandlerAttribute::new("lbthresh", AccessLevel::Admin, world, false, 1, "", ""), handler!(handlelbthresh)),
        (CommandHandlerAttribute::new("radar", AccessLevel::Developer, world, false, 0, "", ""), handler!(handleradar)),
        (CommandHandlerAttribute::new("rares dump", AccessLevel::Developer, world, false, 0, "", ""), handler!(handle_rares_dump)),
        (CommandHandlerAttribute::new("stormnumstormed", AccessLevel::Admin, world, false, 1, "", ""), handler!(handlestormnumstormed)),
        (CommandHandlerAttribute::new("stormthresh", AccessLevel::Admin, world, false, 1, "", ""), handler!(handlestormthresh)),
        (CommandHandlerAttribute::new("showprops", AccessLevel::Admin, none, false, 0, "Displays the name of all properties configurable via the modify commands", ""), handler!(handle_display_props)),
        (CommandHandlerAttribute::new("modifybool", AccessLevel::Admin, none, false, 2, "Modifies a server property that is a bool", "modifybool (string) (bool)"), handler!(handle_modify_server_bool_property)),
        (CommandHandlerAttribute::new("fetchbool", AccessLevel::Admin, none, false, 1, "Fetches a server property that is a bool", "fetchbool (string)"), handler!(handle_fetch_server_bool_property)),
        (CommandHandlerAttribute::new("modifylong", AccessLevel::Admin, none, false, 2, "Modifies a server property that is a long", "modifylong (string) (long)"), handler!(handle_modify_server_long_property)),
        (CommandHandlerAttribute::new("fetchlong", AccessLevel::Admin, none, false, 1, "Fetches a server property that is a long", "fetchlong (string)"), handler!(handle_fetch_server_long_property)),
        (CommandHandlerAttribute::new("modifydouble", AccessLevel::Admin, none, false, 2, "Modifies a server property that is a double", "modifyfloat (string) (double)"), handler!(handle_modify_server_float_property)),
        (CommandHandlerAttribute::new("fetchdouble", AccessLevel::Admin, none, false, 1, "Fetches a server property that is a double", "fetchdouble (string)"), handler!(handle_fetch_server_float_property)),
        (CommandHandlerAttribute::new("modifystring", AccessLevel::Admin, none, false, 2, "Modifies a server property that is a string", "modifystring (string) (string)"), handler!(handle_modify_server_string_property)),
        (CommandHandlerAttribute::new("fetchstring", AccessLevel::Admin, none, false, 1, "Fetches a server property that is a string", "fetchstring (string)"), handler!(handle_fetch_server_string_property)),
        (CommandHandlerAttribute::new("modifypropertydesc", AccessLevel::Admin, none, false, 3, "Modifies a server property's description", "modifypropertydesc <STRING|BOOL|DOUBLE|LONG> (string) (string)"), handler!(handle_modify_property_description)),
        (CommandHandlerAttribute::new("resyncproperties", AccessLevel::Admin, none, false, -1, "Resync the properties database", ""), handler!(handle_resync_server_properties)),
        (CommandHandlerAttribute::new("fix-allegiances", AccessLevel::Admin, console, false, -1, "Fixes the monarch data for allegiances", ""), handler!(handle_fix_allegiances)),
        (CommandHandlerAttribute::new("show-allegiances", AccessLevel::Admin, none, false, -1, "Shows all of the allegiance chains on the server.", ""), handler!(handle_show_allegiances)),
        (CommandHandlerAttribute::new("getenchantments", AccessLevel::Admin, world, false, -1, "Shows the enchantments for the last appraised item", ""), handler!(handle_get_enchantments)),
        (CommandHandlerAttribute::new("cm", AccessLevel::Developer, world, false, 1, "Create a salvage bag in your inventory", "<material_type>, optional: <structure> <workmanship> <num_items>"), handler!(handle_cm)),
        (CommandHandlerAttribute::new("cisalvage", AccessLevel::Developer, world, false, 1, "Create a salvage bag in your inventory", "<material_type>, optional: <structure> <workmanship> <num_items>"), handler!(handle_ci_salvage)),
        (CommandHandlerAttribute::new("setlbenviron", AccessLevel::Developer, world, false, 0, "Sets or clears your current landblock's environment option", "(name or id of EnvironChangeType)\nleave blank to reset to default.\nlist to get a complete list of options."), handler!(handle_set_lb_environ)),
        (CommandHandlerAttribute::new("setglobalenviron", AccessLevel::Developer, world, false, 0, "Sets or clears server's global environment option", "(name or id of EnvironChangeType)\nleave blank to reset to default.\nlist to get a complete list of options."), handler!(handle_set_global_environ)),
        (CommandHandlerAttribute::new("movetome", AccessLevel::Admin, world, false, -1, "Moves the last appraised object to the current player location.", ""), handler!(handle_move_to_me)),
        (CommandHandlerAttribute::new("reload-loot-tables", AccessLevel::Admin, none, false, -1, "reloads the latest data from the loot tables", "optional profile folder"), handler!(handle_reload_loot_tables)),
    ];
    rows.into_iter()
        .map(|(attribute, (handler, handler_name))| CommandHandlerInfo {
            handler,
            handler_name,
            attribute,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// `session` for a handler ACE only reaches with a session (RequiresWorld, or a dereference).
fn require(session: Option<SessionId>) -> SessionId {
    session.expect("NullReferenceException: session")
}

/// `session?.Player`.
fn session_player_opt(w: &World, session: Option<SessionId>) -> Option<ObjectGuid> {
    session.map(|s| session_player(w, s))
}

/// `player.Session.Network.EnqueueSend(msg)`; nothing without a session.
fn send_to_player(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    if let Some(s) = player_manager::player_session(w, player) {
        enqueue_send(w, s, msg);
    }
}

/// `player.SendMessage(msg, type)`.
fn player_send_message(
    w: &mut World,
    player: ObjectGuid,
    msg: &str,
    chat_message_type: ChatMessageType,
) {
    empyrean_world::world_objects::player::send_message(w, player, msg, chat_message_type);
}

/// `CommandHandlerHelper.WriteOutputInfo(session, output, type)`.
fn write_output_info(
    w: &mut World,
    session: Option<SessionId>,
    output: &str,
    chat_message_type: ChatMessageType,
) {
    command_handler_helper::write_output_info(w, session, output, chat_message_type);
}

/// `HealthQueryTarget`, else `ManaQueryTarget`, else `CurrentAppraisalTarget`, else
/// `ObjectGuid.Invalid`.
fn selected_object_id(w: &World, player: ObjectGuid) -> ObjectGuid {
    let p = obj(w, player);
    if let Some(t) = p.health_query_target() {
        ObjectGuid::new(t)
    } else if let Some(t) = p.mana_query_target() {
        ObjectGuid::new(t)
    } else if let Some(t) = p.current_appraisal_target() {
        ObjectGuid::new(t)
    } else {
        ObjectGuid::INVALID
    }
}

/// `HealthQueryTarget.HasValue || ManaQueryTarget.HasValue || CurrentAppraisalTarget.HasValue`.
fn has_selection(w: &World, player: ObjectGuid) -> bool {
    let p = obj(w, player);
    p.health_query_target().is_some()
        || p.mana_query_target().is_some()
        || p.current_appraisal_target().is_some()
}

/// `session.Player.CurrentLandblock?.GetObject(objectId)`.
fn current_landblock_get_object(
    w: &World,
    player: ObjectGuid,
    object_id: ObjectGuid,
) -> Option<ObjectGuid> {
    let lb = obj(w, player).current_landblock?;
    landblock::get_object(w, lb, object_id, true)
}

/// `wo is Creature`.
fn is_creature(w: &World, wo: ObjectGuid) -> bool {
    w.objects.get(wo).is_some_and(|o| o.creature.is_some())
}

/// `wo is Player`.
fn is_player(w: &World, wo: ObjectGuid) -> bool {
    w.objects.get(wo).is_some_and(|o| o.player.is_some())
}

/// `wo.GetType().Name`.
fn type_name(w: &World, wo: ObjectGuid) -> &'static str {
    dispatch::class_of(w, wo).name()
}

/// `string.Join(" ", parameters)`.
fn join(parameters: &[String]) -> String {
    parameters.join(" ")
}

/// `s.Substring(start)` over UTF-16 units.
fn substring_from(s: &str, start: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().skip(start).collect();
    String::from_utf16_lossy(&units)
}

/// `s.Substring(0, n)` over UTF-16 units.
fn substring_to(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}

/// `s.Length` (UTF-16 units).
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `s.TrimStart(' ').TrimEnd(' ')`.
fn trim_spaces(s: &str) -> String {
    s.trim_matches(' ').to_owned()
}

/// `name.First().ToString().ToUpper() + name.Substring(1)`: `First()` throws on an empty string.
fn upper_first(name: &str) -> String {
    let units: Vec<u16> = name.encode_utf16().collect();
    let first = *units
        .first()
        .expect("InvalidOperationException: Sequence contains no elements");
    let first = String::from_utf16_lossy(&[first]).to_uppercase();
    first + &String::from_utf16_lossy(&units[1..])
}

/// `PropertyManager.GetBool(key).Item`.
fn property_manager_get_bool(w: &World, key: &str) -> bool {
    property_manager::get_bool(w, key, false, true).item
}

/// `PropertyManager.GetLong(key).Item`.
fn property_manager_get_long(w: &World, key: &str) -> i64 {
    property_manager::get_long(w, key, 0, true).item
}

// ---------------------------------------------------------------------------------------------
// Enum.TryParse
// ---------------------------------------------------------------------------------------------

/// .NET `Enum.TryParse<TEnum>(value, ignoreCase, out result)`, returning the raw storage bits
/// (masked to the underlying type's width; the caller casts to the enum's type).
///
/// `min..=max` is the underlying type's range. Leading white space is skipped; a value starting
/// with a digit, `-` or `+` is parsed as an integer (sign, digits, trailing ASCII white space)
/// and a value out of range fails; one that is not a number is looked up by name. Names are
/// comma-separated, each trimmed, matched in value order (ordinal, or ordinal ignoring case),
/// and OR-ed together.
#[must_use]
pub fn enum_try_parse<E: AceEnum>(
    value: &str,
    ignore_case: bool,
    min: i128,
    max: i128,
) -> Option<u64> {
    let value = value.trim_start_matches(char::is_whitespace);
    let first = value.chars().next()?;

    let width_mask: u64 = if max > i128::from(u32::MAX) {
        u64::MAX
    } else if max > i128::from(u16::MAX) {
        u64::from(u32::MAX)
    } else if max > i128::from(u8::MAX) {
        u64::from(u16::MAX)
    } else {
        u64::from(u8::MAX)
    };

    if first.is_ascii_digit() || first == '-' || first == '+' {
        match parse_integer_style(value) {
            IntegerParse::Ok(v) => {
                if v < min || v > max {
                    return None;
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                return Some((v as u64) & width_mask);
            }
            IntegerParse::Overflow => return None,
            IntegerParse::Failed => {}
        }
    }

    try_parse_by_name::<E>(value, ignore_case).map(|v| v & width_mask)
}

enum IntegerParse {
    Ok(i128),
    Overflow,
    Failed,
}

/// `Number.TryParseBinaryIntegerStyle(value, AllowLeadingSign | AllowTrailingWhite, invariant)`
/// before the range check: an optional sign, ASCII digits, then only white space (or NULs).
fn parse_integer_style(value: &str) -> IntegerParse {
    let chars: Vec<char> = value.chars().collect();
    let mut i = 0;
    let mut negative = false;
    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
        negative = chars[i] == '-';
        i += 1;
    }
    let start = i;
    let mut v: i128 = 0;
    let mut overflow = false;
    let cap = i128::from(u64::MAX) * 4;
    while i < chars.len() && chars[i].is_ascii_digit() {
        v = v * 10 + i128::from(u32::from(chars[i]) - u32::from('0'));
        if v > cap {
            overflow = true;
            v = cap;
        }
        i += 1;
    }
    if i == start {
        return IntegerParse::Failed;
    }
    while i < chars.len() && (chars[i] == ' ' || ('\u{9}'..='\u{d}').contains(&chars[i])) {
        i += 1;
    }
    if chars[i..].iter().any(|&c| c != '\0') {
        return IntegerParse::Failed;
    }
    if overflow {
        return IntegerParse::Overflow;
    }
    IntegerParse::Ok(if negative { -v } else { v })
}

/// `Enum.TryParseByName`.
fn try_parse_by_name<E: AceEnum>(mut value: &str, ignore_case: bool) -> Option<u64> {
    let mut local_result: u64 = 0;
    while !value.is_empty() {
        let subvalue;
        match value.find(',') {
            None => {
                subvalue = value.trim_matches(char::is_whitespace);
                value = "";
            }
            Some(end_index) if end_index != value.len() - 1 => {
                subvalue = value[..end_index].trim_matches(char::is_whitespace);
                value = &value[end_index + 1..];
            }
            Some(_) => return None,
        }

        let mut success = false;
        for (i, member) in E::MEMBERS.iter().enumerate() {
            let name = E::MEMBER_NAMES[i];
            let matched = if ignore_case {
                equals_ordinal_ignore_case(subvalue, name)
            } else {
                subvalue == name
            };
            if matched {
                local_result |= member.key();
                success = true;
                break;
            }
        }
        if !success {
            return None;
        }
    }
    Some(local_result)
}

/// `a.Equals(b, StringComparison.OrdinalIgnoreCase)`: both upper-cased by the simple (one char)
/// invariant mapping, compared unit for unit. .NET's ordinal casing never maps a non-ASCII
/// character to an ASCII one (`ı` is not `I`).
fn equals_ordinal_ignore_case(a: &str, b: &str) -> bool {
    let up = |c: char| {
        let mut u = c.to_uppercase();
        match (u.next(), u.next()) {
            (Some(x), None)
                if x.len_utf16() == c.len_utf16() && (c.is_ascii() || !x.is_ascii()) =>
            {
                x
            }
            _ => c,
        }
    };
    utf16_len(a) == utf16_len(b) && a.chars().map(up).eq(b.chars().map(up))
}

/// `Enum.TryParse(s, ignoreCase, out SpellId)` (underlying `uint`).
#[must_use]
pub fn try_parse_spell_id(s: &str, ignore_case: bool) -> Option<SpellId> {
    enum_try_parse::<SpellId>(s, ignore_case, 0, i128::from(u32::MAX)).map(|v| SpellId(v.cs_cast()))
}

/// `Enum.TryParse(s, ignoreCase, out AccessLevel)` (underlying `int`).
#[must_use]
pub fn try_parse_access_level(s: &str, ignore_case: bool) -> Option<AccessLevel> {
    enum_try_parse::<AccessLevel>(s, ignore_case, i128::from(i32::MIN), i128::from(i32::MAX))
        .map(|v| AccessLevel(u32_bits_to_i32(v)))
}

/// `Enum.TryParse(s, ignoreCase, out EnvironChangeType)` (underlying `int`).
#[must_use]
pub fn try_parse_environ_change_type(s: &str, ignore_case: bool) -> Option<EnvironChangeType> {
    enum_try_parse::<EnvironChangeType>(s, ignore_case, i128::from(i32::MIN), i128::from(i32::MAX))
        .map(|v| EnvironChangeType(u32_bits_to_i32(v)))
}

/// `Enum.TryParse(s, ignoreCase, out MaterialType)` (underlying `uint`).
#[must_use]
pub fn try_parse_material_type(s: &str, ignore_case: bool) -> Option<MaterialType> {
    enum_try_parse::<MaterialType>(s, ignore_case, 0, i128::from(u32::MAX))
        .map(|v| MaterialType(v.cs_cast()))
}

/// `Enum.TryParse(s, ignoreCase, out PropertyAttribute2nd)` (underlying `ushort`).
#[must_use]
pub fn try_parse_property_attribute_2nd(
    s: &str,
    ignore_case: bool,
) -> Option<PropertyAttribute2nd> {
    enum_try_parse::<PropertyAttribute2nd>(s, ignore_case, 0, i128::from(u16::MAX))
        .map(|v| PropertyAttribute2nd(v.cs_cast()))
}

/// `Enum.TryParse(s, ignoreCase, out PropertyAttribute)` (underlying `ushort`).
#[must_use]
pub fn try_parse_property_attribute(s: &str, ignore_case: bool) -> Option<PropertyAttribute> {
    enum_try_parse::<PropertyAttribute>(s, ignore_case, 0, i128::from(u16::MAX))
        .map(|v| PropertyAttribute(v.cs_cast()))
}

/// `Enum.TryParse(s, ignoreCase, out Skill)` (underlying `int`).
#[must_use]
pub fn try_parse_skill(s: &str, ignore_case: bool) -> Option<Skill> {
    enum_try_parse::<Skill>(s, ignore_case, i128::from(i32::MIN), i128::from(i32::MAX))
        .map(|v| Skill(u32_bits_to_i32(v)))
}

/// `Enum.TryParse(s, ignoreCase, out SkillAdvancementClass)` (underlying `uint`).
#[must_use]
pub fn try_parse_skill_advancement_class(
    s: &str,
    ignore_case: bool,
) -> Option<SkillAdvancementClass> {
    enum_try_parse::<SkillAdvancementClass>(s, ignore_case, 0, i128::from(u32::MAX))
        .map(|v| SkillAdvancementClass(v.cs_cast()))
}

/// The low 32 bits of `v` as an `int`.
fn u32_bits_to_i32(v: u64) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    let low = v as u32;
    low.cast_signed()
}

/// `new IPAddress(bytes).ToString()`: four bytes are IPv4, sixteen IPv6.
///
/// # Panics
/// Any other length (.NET's `ArgumentException`).
fn ip_address_to_string(bytes: &[u8]) -> String {
    if let Ok(v4) = <[u8; 4]>::try_from(bytes) {
        return std::net::Ipv4Addr::from(v4).to_string();
    }
    if let Ok(v6) = <[u8; 16]>::try_from(bytes) {
        return std::net::Ipv6Addr::from(v6).to_string();
    }
    panic!("ArgumentException: An invalid IP address was specified.");
}

// ---------------------------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------------------------

// ACE: AdminCommands.HandleAdminvision
/// `adminvision { on | off | toggle | check }`: allows the admin to see admin-only visible items.
pub fn handle_adminvision(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // @adminvision { on | off | toggle | check}
    // Controls whether or not the admin can see admin-only visible items. Note that if you turn this feature off, you will need to log out and back in before the visible items become invisible.
    // @adminvision - Allows the admin to see admin - only visible items.

    let player = session_player(w, require(session));
    match parameters[0].to_lowercase().as_str() {
        "1" | "on" => player_handle_adminvision_toggle(w, player, 1),
        "0" | "off" => player_handle_adminvision_toggle(w, player, 0),
        "toggle" => player_handle_adminvision_toggle(w, player, 2),
        _ => player_handle_adminvision_toggle(w, player, -1),
    }
}

// ACE: AdminCommands.HandleAdminui
/// `adminui`: ACE's body is a placeholder comment.
pub fn handle_adminui(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // usage: @adminui
    // This command toggles whether the Admin UI is visible.

    // just a placeholder, probably not needed or should be handled by a decal plugin to replicate the admin ui
}

// ACE: AdminCommands.HandleDeleteSelected
/// `delete`: deletes the selected object. Players may not be deleted this way.
pub fn handle_delete_selected(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // @delete - Deletes the selected object. Players may not be deleted this way.

    let s = require(session);
    let me = session_player(w, s);
    let object_id = selected_object_id(w, me);

    // Not ACE's (a fix): with nothing selected the "identify the
    // object" message is the only reply; ACE went on to look up guid 0 and added "Object not found".
    if object_id == ObjectGuid::INVALID {
        send_server_message(
            w,
            session,
            "Delete failed. Please identify the object you wish to delete first.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    if object_id.is_player() {
        send_server_message(
            w,
            session,
            "Delete failed. Players cannot be deleted.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    let found = player_inventory::find_object(w, me, object_id, SearchLocations::Everywhere);
    let root_owner = found.root_owner;

    let Some(wo) = found.result else {
        send_server_message(
            w,
            session,
            "Delete failed. Object not found.",
            ChatMessageType::Broadcast,
        );
        return;
    };

    if parameters.len() == 1 {
        let object_type = parameters[0].to_lowercase();
        let weenie_type = obj(w, wo).biota.weenie_type.to_dotnet_string();

        if object_type != type_name(w, wo).to_lowercase()
            && object_type != weenie_type.to_lowercase()
        {
            let text = format!(
                "Delete failed. Object type specified ({}) does not match object type ({}) or weenie type ({weenie_type}) for 0x{wo}:{}.",
                parameters[0],
                type_name(w, wo),
                name_of(w, wo)
            );
            send_server_message(w, session, &text, ChatMessageType::Broadcast);
            return;
        }
    }

    // DIVERGE: `wo.DeleteObject(rootOwner)` destroys the object, which leaves `World.objects`, while ACE goes on reading the C# reference; what the two lines after it read is taken first (`GameMessageDeleteObject` reads the current instance sequence, which Destroy leaves alone, so the bytes are the same).
    let msg = game_message_delete_object(obj_mut(w, wo));
    let deleted_name = name_of(w, wo);
    world_object_delete_object(w, wo, root_owner);
    enqueue_send(w, s, msg);

    let text = format!("{} has deleted 0x{wo}:{deleted_name}", name_of(w, me));
    player_manager::broadcast_to_audit_channel(w, Some(me), &text);
}

// ACE: AdminCommands.HandleDraw
/// `draw`: ACE's body is empty ("TODO: output").
pub fn handle_draw(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @draw - Draws undrawable things.

    // TODO: output
}

// ACE: AdminCommands.HandleFinger
/// `finger [ [-a] character] [-m account]`: show the given character's account name or
/// vice-versa.
pub fn handle_finger(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @finger[ [-a] character] [-m account]
    // Given a character name, this command displays the name of the owning account.If the -m option is specified, the argument is considered an account name and the characters owned by that account are displayed.If the -a option is specified, then the character name is fingered but their account is implicitly fingered as well.
    // @finger - Show the given character's account name or vice-versa.

    let lookup_char_and_account = parameters.iter().any(|p| p == "-a");
    let lookup_by_account = parameters.iter().any(|p| p == "-m");

    let char_name = if lookup_by_account || lookup_char_and_account {
        join(parameters.get(1..).unwrap_or_default())
    } else {
        join(parameters)
    };

    let mut message;
    if !lookup_by_account && !lookup_char_and_account {
        let (character, _) = player_manager::find_by_name(w, &char_name);

        if let Some(character) = character {
            let name = i_player::name(w, character).unwrap_or_default();
            if let Some(account) = i_player::account(w, character) {
                message = format!(
                    "Login name: {}      Character: {name}\n",
                    account.account_name
                );
            } else {
                message = format!("Login name: account not found, character is orphaned.      Character: {name}\n");
            }
        } else {
            message = format!(
                "There was no active character named \"{char_name}\" found in the database.\n"
            );
        }
    } else {
        let account = if lookup_char_and_account {
            let (character, _) = player_manager::find_by_name(w, &char_name);

            let Some(character) = character else {
                message = format!(
                    "There was no active character named \"{char_name}\" found in the database.\n"
                );
                write_output_info(w, session, &message, ChatMessageType::WorldBroadcast);
                return;
            };
            let Some(character_account) = i_player::account(w, character) else {
                let name = i_player::name(w, character).unwrap_or_default();
                message = format!("Login name: account not found, character is orphaned.      Character: {name}\n");
                write_output_info(w, session, &message, ChatMessageType::WorldBroadcast);
                return;
            };
            // get updated data from db.
            w.auth
                .lock()
                .get_account_by_id(character_account.account_id)
        } else {
            w.auth.lock().get_account_by_name(&char_name)
        };

        if let Some(account) = account {
            if account.banned_time.is_some() {
                let bannedby_account = if account.banned_by_account_id.is_some_and(|id| id > 0) {
                    let by = w
                        .auth
                        .lock()
                        .get_account_by_id(account.banned_by_account_id.unwrap_or_default())
                        .expect("NullReferenceException: GetAccountById(BannedByAccountId)");
                    format!("account {}", by.account_name)
                } else {
                    "CONSOLE".to_owned()
                };

                let expires = account
                    .ban_expire_time
                    .expect("InvalidOperationException: Nullable object must have a value.");
                message = format!(
                    "Account '{}' was banned by {bannedby_account} until server time {}.\n",
                    account.account_name,
                    expires.format("MMM dd yyyy  h:mmtt")
                );
            } else {
                message = format!("Account '{}' is not banned.\n", account.account_name);
            }
            if account.access_level > u32::try_from(AccessLevel::Player.0).unwrap_or_default() {
                message += &format!(
                    "Account '{}' has been granted AccessLevel.{} rights.\n",
                    account.account_name,
                    AccessLevel(account.access_level.cast_signed()).to_dotnet_string()
                );
            }
            message += &format!(
                "Account created on {} by IP: {} \n",
                to_common_string(account.create_time),
                account
                    .create_ip
                    .as_deref()
                    .map_or_else(|| "N/A".to_owned(), ip_address_to_string)
            );
            let account_age = w.now.utc - account.create_time;
            if account_age.total_days() < 15.0 {
                message += "Account was created less than 15 days ago.\n";
            }
            message += &format!(
                "Account last logged on at {} by IP: {}\n",
                account
                    .last_login_time
                    .map_or_else(|| "N/A".to_owned(), to_common_string),
                account
                    .last_login_ip
                    .as_deref()
                    .map_or_else(|| "N/A".to_owned(), ip_address_to_string)
            );
            message += &format!(
                "Account total times logged on {}\n",
                account.total_times_logged_in
            );
            let characters = w
                .shard
                .base_database()
                .get_characters(account.account_id, true);
            message += &format!(
                "{} Character(s) owned by: {}\n",
                characters.len(),
                account.account_name
            );
            message += "-------------------\n";
            let plus =
                |c: &empyrean_store::models::shard::Character| if c.is_plussed { "+" } else { "" };
            for character in characters
                .iter()
                .filter(|x| !x.is_deleted && x.delete_time == 0)
            {
                message += &format!(
                    "\"{}{}\", ID 0x{}\n",
                    plus(character),
                    character.name,
                    format(character.id, "X8")
                );
            }
            let pending_deleted_characters: Vec<_> = characters
                .iter()
                .filter(|x| !x.is_deleted && x.delete_time > 0)
                .collect();
            if !pending_deleted_characters.is_empty() {
                message += "-------------------\n";
                for character in pending_deleted_characters {
                    message += &format!(
                        "\"{}{}\", ID 0x{} -- Will be deleted at server time {}\n",
                        plus(character),
                        character.name,
                        format(character.id, "X8"),
                        unix_seconds_to_date_time(character.delete_time)
                            .format("MMM d yyyy h:mm tt")
                    );
                }
            }
            message += "-------------------\n";
            let deleted_characters: Vec<_> = characters.iter().filter(|x| x.is_deleted).collect();
            if deleted_characters.is_empty() {
                message += "No deleted characters.\n";
            } else {
                for character in deleted_characters {
                    message += &format!(
                        "\"{}{}\", ID 0x{} -- Deleted at server time {}\n",
                        plus(character),
                        character.name,
                        format(character.id, "X8"),
                        unix_seconds_to_date_time(character.delete_time)
                            .format("MMM d yyyy h:mm tt")
                    );
                }
            }
        } else {
            message =
                format!("There was no account named \"{char_name}\" found in the database.\n");
        }
    }

    write_output_info(w, session, &message, ChatMessageType::WorldBroadcast);
}

/// `new DateTime(1970, 1, 1, 0, 0, 0, 0, DateTimeKind.Utc).AddSeconds(seconds)`.
#[allow(clippy::cast_precision_loss)]
fn unix_seconds_to_date_time(seconds: u64) -> DotNetDateTime {
    DotNetDateTime::UNIX_EPOCH.add_seconds(seconds as f64)
}

// ACE: AdminCommands.HandleFreeze
/// `freeze`: ACE's body is empty ("TODO: output").
pub fn handle_freeze(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @freeze - Freezes the selected target for 10 minutes or until unfrozen.

    // TODO: output
}

// ACE: AdminCommands.HandleUnFreeze
/// `unfreeze`: ACE's body is empty ("TODO: output").
pub fn handle_un_freeze(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @unfreeze - Unfreezes the selected target.

    // TODO: output
}

// ACE: AdminCommands.HandleGag
/// `gag < char name >`: prevents a character from talking.
pub fn handle_gag(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @gag < char name >
    // This command gags the specified character for five minutes.  The character will not be able to @tell or use chat normally.
    // @gag - Prevents a character from talking.
    // @ungag -Allows a gagged character to talk again.

    if !parameters.is_empty() {
        let player_name = join(parameters);

        let me = session_player(w, require(session));
        let msg = if player_manager::gag_player(w, me, &player_name) {
            format!("{player_name} has been gagged for five minutes.")
        } else {
            format!("Unable to gag a character named {player_name}, check the name and re-try the command.")
        };

        write_output_info(w, session, &msg, ChatMessageType::WorldBroadcast);
    }
}

// ACE: AdminCommands.HandleUnGag
/// `ungag < char name >`: allows a gagged character to talk again.
pub fn handle_un_gag(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @ungag < char name >
    // @ungag -Allows a gagged character to talk again.

    if !parameters.is_empty() {
        let player_name = join(parameters);

        let me = session_player(w, require(session));
        let msg = if player_manager::un_gag_player(w, me, &player_name) {
            format!("{player_name} has been ungagged.")
        } else {
            format!("Unable to ungag a character named {player_name}, check the name and re-try the command.")
        };

        write_output_info(w, session, &msg, ChatMessageType::WorldBroadcast);
    }
}

/// The `PositionType` of recall number 0 (Sanctuary) to 9 (Save9); `default` otherwise.
fn recall_position_type(parsed: u32, default: PositionType) -> PositionType {
    match parsed {
        0 => PositionType::Sanctuary,
        1 => PositionType::Save1,
        2 => PositionType::Save2,
        3 => PositionType::Save3,
        4 => PositionType::Save4,
        5 => PositionType::Save5,
        6 => PositionType::Save6,
        7 => PositionType::Save7,
        8 => PositionType::Save8,
        9 => PositionType::Save9,
        _ => default,
    }
}

/// `parameters[0]` cut to one character, or "0" without parameters (`HandleHome`, `HandleSave`).
fn recall_number_text(parameters: &[String]) -> String {
    // Limit the incoming parameter to 1 character
    if parameters.is_empty() {
        "0".to_owned()
    } else if utf16_len(&parameters[0]) > 1 {
        substring_to(&parameters[0], 1)
    } else {
        parameters[0].clone()
    }
}

// ACE: AdminCommands.HandleHome
/// Teleports an admin to their sanctuary position. If a single uint value from 1 to 9 is provided
/// as a parameter then the admin is teleported to the cooresponding named recall point.
pub fn handle_home(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // @home has the alias @recall
    let s = require(session);
    let parse_position_string = recall_number_text(parameters);

    // Attempt to parse the integer
    if let Some(parsed_position_int) = dotnet_parse::uint_try_parse(&parse_position_string) {
        // parsedPositionInt value should be limited too a value from, 0-9
        // Create a new position from the current player location
        // Transform too the correct PositionType, based on the "Saved Positions" subset:
        let position_type = recall_position_type(parsed_position_int, PositionType::Undef);

        // If we have the position, teleport the player
        let player = session_player(w, s);
        if obj(w, player).get_position(position_type).is_some() {
            player_location::tele_to_position(w, player, position_type);
            system_chat(
                w,
                s,
                &format!("Recalling to {}", position_type.to_dotnet_string()),
                ChatMessageType::Broadcast,
            );
            return;
        }
    }
    // Invalid character was receieved in the input (it was not 0-9)
    system_chat(
        w,
        s,
        "Could not find a valid recall position.",
        ChatMessageType::Broadcast,
    );
}

// ACE: AdminCommands.HandleMRT
/// `mrt`: toggles the ability to bypass housing boundaries.
pub fn handle_mrt(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // @mrt - Toggles the ability to bypass housing boundaries.
    let player = session_player(w, require(session));
    player_handle_mrt(w, player);
}

// ACE: AdminCommands.HandleLimbo
/// `limbo [on / off]`: ACE's body is empty ("TODO: output").
pub fn handle_limbo(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @limbo[on / off] - Puts the targeted player in 'limbo' which means that the player cannot damage anything or be damaged by anything.The player will not receive direct tells, or channel messages, such as fellowship messages and allegiance chat.  The player will be unable to salvage.This status times out after 15 minutes, use '@limbo on' again on the player to reset the timer. You and the player will be notifed when limbo wears off.If neither on or off are specified, on is assumed.
    // @limbo - Puts the selected target in limbo.

    // TODO: output
}

// ACE: AdminCommands.HandleMyIID
/// `myiid`: displays your Instance ID (IID).
pub fn handle_my_iid(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // @myiid - Displays your Instance ID(IID).

    let s = require(session);
    let guid = session_player(w, s);
    let text = format!(
        "GUID: {}  - Low: {} - High: {} - (0x{})",
        guid.full(),
        guid.low(),
        guid.high(),
        format(guid.full(), "X")
    );
    system_chat(w, s, &text, ChatMessageType::Broadcast);
}

// ACE: AdminCommands.HandleMyServer
/// `myserver`: ACE's body is empty ("TODO: output").
pub fn handle_my_server(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @myserver - Displays the number of the game server on which you are currently located.

    // TODO: output
}

// ACE: AdminCommands.HandlePk
/// `pk < npk / pk / pkl / free >`: sets your own PK state.
pub fn handle_pk(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // @pk - Toggles or sets your own PK state.

    let player = session_player(w, require(session));
    if parameters.is_empty() {
        //player.EnqueueBroadcast(new GameMessagePublicUpdatePropertyInt(player, PropertyInt.PlayerKillerStatus, (int)player.PlayerKillerStatus));
        let message = format!(
            "Your current PK state is: {}\nYou can change it to the following:\nNPK      = Non-Player Killer\nPK       = Player Killer\nPKL      = Player Killer Lite\nFree     = Can kill anything\n",
            obj(w, player).player_killer_status().to_dotnet_string()
        );
        write_output_info(w, session, &message, ChatMessageType::Broadcast);
    } else {
        let set = |w: &mut World, status: PlayerKillerStatus, level: PKLevel| {
            let o = obj_mut(w, player);
            o.set_player_killer_status_prop(status);
            o.set_pk_level_modifier(level.0.cast_signed());
        };
        match parameters[0].to_lowercase().as_str() {
            "npk" => set(w, PlayerKillerStatus::NPK, PKLevel::NPK),
            "pk" => set(w, PlayerKillerStatus::PK, PKLevel::PK),
            "pkl" => set(w, PlayerKillerStatus::PKLite, PKLevel::NPK),
            "free" => set(w, PlayerKillerStatus::Free, PKLevel::Free),
            _ => {}
        }
        let status = obj(w, player).player_killer_status();
        let msg = game_message_public_update_property_int(
            obj_mut(w, player),
            PropertyInt::PlayerKillerStatus,
            status.0.cast_signed(),
        );
        world_object_networking::enqueue_broadcast(w, player, true, &[msg]);
        write_output_info(
            w,
            session,
            &format!(
                "Your current PK state is now set to: {}",
                status.to_dotnet_string()
            ),
            ChatMessageType::Broadcast,
        );

        let text = format!(
            "{} changed their PK state to {}.",
            name_of(w, player),
            status.to_dotnet_string()
        );
        player_manager::broadcast_to_audit_channel(w, Some(player), &text);
    }
}

// ACE: AdminCommands.HandleQuerypluginlist
/// `querypluginlist`: ACE's body is empty ("TODO: output").
pub fn handle_querypluginlist(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @querypluginlist (-fresh) - View list of plug-ins the selected character is running. If you do not use the -fresh paramater, you will get results that where cached at login. That way, you will not be sending an alert to the player that you are querying thier plugin list.  If you use -fresh, then you will get fresh data from the player's client and they will recieve notification that you have asked for thier plugin list.NOTE: Results are dependent upon 3rd party authors providing correct information.
    // @querypluginlist - View list of plug - ins the selected character is running.
    // @querypluginlist<pluginname> - View information about a specific plugin.NOTE: Results are dependent upon 3rd party authors providing correct information.
    // @queryplugin < pluginname > -View information about a specific plugin.

    // TODO: output
}

// ACE: AdminCommands.HandleQueryplugin
/// `queryplugin`: ACE's body is empty ("TODO: output").
pub fn handle_queryplugin(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @queryplugin < pluginname > -View information about a specific plugin.

    // TODO: output
}

// ACE: AdminCommands.HandleRepeat
/// `repeat < Num > < Command >`: ACE's body is empty ("TODO: output").
pub fn handle_repeat(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @repeat < Num > < Command > -Repeat a command a number of times.
    // EX: "@repeat 5 @say Hi" - say Hi 5 times.
    // @repeat < Num > < Command > -Repeat a command a number of times.

    // TODO: output
}

// ACE: AdminCommands.HandleRegen
/// `regen`: sends the selected generator a regeneration message.
pub fn handle_regen(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // @regen - Sends the selected generator a regeneration message.

    let player = session_player(w, require(session));
    if has_selection(w, player) {
        let object_id = selected_object_id(w, player);

        let wo = current_landblock_get_object(w, player, object_id);

        if object_id.is_player() {
            return;
        }

        // Not ACE's (a fix): a selection that is not in the landblock
        // does nothing; ACE's non-short-circuit test threw on it (logged by the command manager).
        let Some(wo) = wo else { return };
        if obj(w, wo).is_generator() {
            dispatch::reset_generator::reset_generator(w, wo);
            obj_mut(w, wo).set_generator_entered_world(false);
            let now = w.now.unix_time;
            world_object_tick::generator_regeneration(w, wo, now);
        }
    }
}

// ACE: AdminCommands.HandleSave
/// Command for saving the Admin's current location as the sanctuary position. If a uint between
/// 1-9 is provided as a parameter, the corresponding named recall is saved.
pub fn handle_save(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // Set the default of 0 to save the sanctuary portal if no parameter is passed.
    let s = require(session);
    let parse_position_string = recall_number_text(parameters);

    // Attempt to parse the integer
    if let Some(parsed_position_int) = dotnet_parse::uint_try_parse(&parse_position_string) {
        // parsedPositionInt value should be limited too a value from, 0-9
        // Create a new position from the current player location
        let player = session_player(w, s);
        let player_position = obj(w, player)
            .location()
            .expect("NullReferenceException: session.Player.Location");
        // Set the correct PositionType, based on the "Saved Positions" position type subset:
        let position_type = recall_position_type(parsed_position_int, PositionType::Sanctuary);

        // Save the position
        obj_mut(w, player).set_position(
            position_type,
            Some(Position::from_position(&player_position)),
        );
        // Report changes to client
        system_chat(
            w,
            s,
            &format!(
                "Set: {} to Loc: {player_position}",
                position_type.to_dotnet_string()
            ),
            ChatMessageType::Broadcast,
        );
        return;
    }
    // Error parsing the text input, from parameter[0]
    system_chat(
        w,
        s,
        "Could not determine the correct PositionType. Please use an integer value from 1 to 9; or omit the parmeter entirely.",
        ChatMessageType::Broadcast,
    );
}

// ACE: AdminCommands.HandleServerlist
/// `serverlist`: ACE's body is empty ("TODO: output").
pub fn handle_serverlist(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // The @serverlist command shows a list of the servers in the current server farm. The format is as follows:
    // -The server ID
    // -The server's speed relative to the master server
    // - Number of users reported by ObjLoc and LoadBalancing
    // - Current total load reported by LoadBalancing
    // - Total load from blocks with no players in them
    // - Blocks with players / blocks loaded / blocks owned
    // - The owned block range from low to high(in hex)
    // - The external IP address to talk to clients on
    // -If the server is your current server or the master
    // @serverlist - Shows a list of the logical servers that control this world.

    // TODO: output
}

// ACE: AdminCommands.HandleSnoop
/// `snoop [start / stop] [Character Name]`: ACE's body is empty ("TODO: output").
pub fn handle_snoop(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @snoop[start / stop][Character Name]
    // - If no character name is supplied, the currently selected character will be used.If neither start nor stop is specified, start will be assumed.
    // @snoop - Listen in on a player's private communication.

    // TODO: output
}

// ACE: AdminCommands.HandleSmite
/// `smite [all, Player's Name]`: kills the selected target or all monsters in radar range if
/// "all" is specified.
pub fn handle_smite(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // @smite [all] - Kills the selected target or all monsters in radar range if "all" is specified.

    let me = session_player(w, require(session));
    if !parameters.is_empty() {
        if parameters[0] == "all" {
            let phys = obj(w, me)
                .phys
                .expect("NullReferenceException: session.Player.PhysicsObj");
            for visible in object_maint::get_visible_objects_values(w, phys) {
                let wo = empyrean_world::physics::phys_ext::weenie_obj(w, visible).world_object(w);

                if wo.is_some_and(|wo| is_player(w, wo)) {
                    // I don't recall if @smite all would kill players in range, assuming it didn't
                    continue;
                }

                let use_take_damage = property_manager_get_bool(w, "smite_uses_takedamage");

                if let Some(creature) = wo.filter(|&wo| is_creature(w, wo)) {
                    if obj(w, creature).attackable() {
                        creature_death::smite(w, creature, me, use_take_damage);
                    }
                }
            }

            let text = format!("{} used smite all.", name_of(w, me));
            player_manager::broadcast_to_audit_channel(w, Some(me), &text);
        } else {
            // if parameters are greater then 1, we may have a space in a character name
            let character_name = if parameters.len() > 1 {
                // adds a space back inbetween each parameter
                let mut character_name = String::new();
                for name in parameters {
                    if character_name.is_empty() {
                        character_name = name.clone();
                    } else {
                        character_name = format!("{character_name} {name}");
                    }
                }
                character_name
            } else {
                // if there are no spaces, just set the characterName to the first paramter
                parameters[0].clone()
            };

            // look up session
            let player = player_manager::get_online_player_by_name(w, &character_name);

            // playerSession will be null when the character is not found
            if let Some(player) = player {
                let use_take_damage = property_manager_get_bool(w, "smite_uses_takedamage");
                creature_death::smite(w, player, me, use_take_damage);

                let text = format!("{} used smite on {}", name_of(w, me), name_of(w, player));
                player_manager::broadcast_to_audit_channel(w, Some(me), &text);
                return;
            }

            send_server_message(
                w,
                session,
                "Select a target and use @smite, or use @smite all to kill all creatures in radar range or @smite [player's name].",
                ChatMessageType::Broadcast,
            );
        }
    } else if let Some(target) = obj(w, me).health_query_target() {
        // Only Creatures will trigger this.. Excludes vendors automatically as a result (Can change design to mimic @delete command)
        let object_id = ObjectGuid::new(target);

        let wo = current_landblock_get_object(w, me, object_id).filter(|&wo| is_creature(w, wo));

        if object_id == me {
            // don't kill yourself
            return;
        }

        if let Some(wo) = wo {
            let use_take_damage = property_manager_get_bool(w, "smite_uses_takedamage");
            creature_death::smite(w, wo, me, use_take_damage);

            let text = format!(
                "{} used smite on {} (0x{wo})",
                name_of(w, me),
                name_of(w, wo)
            );
            player_manager::broadcast_to_audit_channel(w, Some(me), &text);
        }
    } else {
        send_server_message(
            w,
            session,
            "Select a target and use @smite, or use @smite all to kill all creatures in radar range or @smite [players' name].",
            ChatMessageType::Broadcast,
        );
    }
}

/// `player.Location`, which ACE dereferences.
fn location_of(w: &World, player: ObjectGuid) -> Position {
    obj(w, player)
        .location()
        .expect("NullReferenceException: player.Location")
}

// ACE: AdminCommands.HandleTeleto
/// `teleto [char]`: teleports you to the specified character.
pub fn handle_teleto(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // @teleto - Teleports you to the specified character.
    let s = require(session);
    let player_name = join(parameters);
    // Lookup the player in the world
    let player = player_manager::get_online_player_by_name(w, &player_name);
    // If the player is found, teleport the admin to the Player's location
    if let Some(player) = player {
        let me = session_player(w, s);
        let location = location_of(w, player);
        player_location::teleport(w, me, &location, false);
    } else {
        system_chat(
            w,
            s,
            &format!("Player {player_name} was not found."),
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: AdminCommands.HandleTeleToMe
/// Teleports a player to your current location
pub fn handle_tele_to_me(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let player_name = join(parameters);
    let Some(player) = player_manager::get_online_player_by_name(w, &player_name) else {
        system_chat(
            w,
            s,
            &format!("Player {player_name} was not found."),
            ChatMessageType::Broadcast,
        );
        return;
    };
    let me = session_player(w, s);
    let current_pos = Position::from_position(&location_of(w, player));
    let my_location = location_of(w, me);
    player_location::teleport(w, player, &my_location, false);
    obj_mut(w, player).set_position(PositionType::TeleportedCharacter, Some(current_pos));
    send_to_player(
        w,
        player,
        game_message_system_chat(
            &format!("{} has teleported you.", name_of(w, me)),
            ChatMessageType::Magic,
        ),
    );

    let text = format!(
        "{} has teleported {} to them.",
        name_of(w, me),
        name_of(w, player)
    );
    player_manager::broadcast_to_audit_channel(w, Some(me), &text);
}

// ACE: AdminCommands.HandleTeleReturn
/// Teleports a player to their previous position
pub fn handle_tele_return(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let player_name = join(parameters);
    let Some(player) = player_manager::get_online_player_by_name(w, &player_name) else {
        system_chat(
            w,
            s,
            &format!("Player {player_name} was not found."),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let Some(teleported_character) = obj(w, player).teleported_character() else {
        system_chat(
            w,
            s,
            &format!("Player {player_name} does not have a return position saved."),
            ChatMessageType::Broadcast,
        );
        return;
    };

    player_location::teleport(
        w,
        player,
        &Position::from_position(&teleported_character),
        false,
    );
    obj_mut(w, player).set_position(PositionType::TeleportedCharacter, None);
    let me = session_player(w, s);
    send_to_player(
        w,
        player,
        game_message_system_chat(
            &format!(
                "{} has returned you to your previous location.",
                name_of(w, me)
            ),
            ChatMessageType::Magic,
        ),
    );

    let text = format!(
        "{} has returned {} to their previous location.",
        name_of(w, me),
        name_of(w, player)
    );
    player_manager::broadcast_to_audit_channel(w, Some(me), &text);
}

// ACE: AdminCommands.HandleTeleAllTo
/// `teleallto [char]`: teleports all players to a player. If no target is specified, all players
/// will be teleported to you.
pub fn handle_tele_all_to(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let me = session_player(w, require(session));
    let mut destination_player = None;

    if !parameters.is_empty() {
        destination_player = player_manager::get_online_player_by_name(w, &parameters[0]);
    }

    let destination_player = destination_player.unwrap_or(me);

    for player in player_manager::get_all_online(w) {
        if player == destination_player {
            continue;
        }

        let here = Position::from_position(&location_of(w, player));
        obj_mut(w, player).set_position(PositionType::TeleportedCharacter, Some(here));

        let destination = Position::from_position(&location_of(w, destination_player));
        player_location::teleport(w, player, &destination, false);
    }

    let text = format!(
        "{} has teleported all online players to their location.",
        name_of(w, me)
    );
    player_manager::broadcast_to_audit_channel(w, Some(me), &text);
}

/// `Comparer<string>.Default.Compare(a, b)` under en-US (ICU root collation): the primary
/// weights (letters without case, punctuation and digits before letters), then lower case
/// before upper case, then ordinal.
///
/// DIVERGE (forced): an approximation of ICU's collation (the primary weights are empyrean-store's
/// DUCET table; accents and the other secondary differences are not weighed). It matches .NET on
/// the vectored names (`admin_culture_order`), which are what point-of-interest names look like.
#[must_use]
pub fn culture_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let primary = empyrean_store::collation::weights(a).cmp(&empyrean_store::collation::weights(b));
    if primary != std::cmp::Ordering::Equal {
        return primary;
    }
    // tertiary: at the first case difference, lower case sorts first
    for (x, y) in a.chars().zip(b.chars()) {
        if x != y && x.to_lowercase().eq(y.to_lowercase()) {
            return if x.is_lowercase() {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
    }
    a.cmp(b)
}

// ACE: AdminCommands.HandleTeleportPoi
/// `telepoi [POI|list]`: teleport yourself to a named Point of Interest.
pub fn handle_teleport_poi(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let poi = join(parameters);

    if poi.to_lowercase() == "list" {
        w.content.cache_all_points_of_interest();
        let pois = w.content.get_points_of_interest_cache();
        // Not ACE's (a fix): the cache also holds an empty entry for
        // every name a lookup failed to find; only the real points of interest are listed. ACE
        // listed every key, so earlier misspellings (lowercased) appeared among them.
        let mut keys: Vec<String> = pois
            .into_iter()
            .filter(|(_, v)| v.is_some())
            .map(|(k, _)| k)
            .collect();
        // `OrderBy(k => k)` is a stable sort under the culture comparer
        keys.sort_by(|a, b| culture_compare(a, b));
        // `.DefaultIfEmpty().Aggregate((a, b) => a + ", " + b)`: no keys give null
        let list = keys.join(", ");
        system_chat(
            w,
            s,
            &format!("All POIs: {list}"),
            ChatMessageType::Broadcast,
        );
    } else {
        let Some(teleport_poi) = w.content.get_cached_point_of_interest(&poi) else {
            system_chat(
                w,
                s,
                &format!(
                    "Location: \"{poi}\" not found. Use \"list\" to display all valid locations."
                ),
                ChatMessageType::Broadcast,
            );
            return;
        };
        let weenie = w
            .content
            .get_cached_weenie(teleport_poi.weenie_class_id)
            .expect("NullReferenceException: GetCachedWeenie(teleportPOI.WeenieClassId)");
        let destination = weenie
            .get_position(PositionType::Destination)
            .expect("NullReferenceException: weenie.GetPosition(PositionType.Destination)");
        let mut portal_dest = Position::from_position(&destination);
        world_object::adjust_dungeon(w, &mut portal_dest);
        let me = session_player(w, s);
        player_location::teleport(w, me, &portal_dest, false);
    }
}

/// The usage lines `HandleTeleportLOC` sends when it catches an exception.
const TELELOC_HELP: [&str; 6] = [
    "Invalid arguments for @teleloc",
    "Hint: @teleloc follows the same number order as displayed from @loc output",
    "Usage: @teleloc cell [x y z] (qw qx qy qz)",
    "Example: @teleloc 0x7F0401AD [12.319900 -28.482000 0.005000] -0.338946 0.000000 0.000000 -0.940806",
    "Example: @teleloc 0x7F0401AD 12.319900 -28.482000 0.005000 -0.338946 0.000000 0.000000 -0.940806",
    "Example: @teleloc 7F0401AD 12.319900 -28.482000 0.005000",
];

/// What `HandleTeleportLOC` makes of its parameters: the position to teleport to, `Ok(None)`
/// when a coordinate does not parse (the handler returns silently), or `Err` where ACE throws
/// (the cell is not hex, or too few parameters).
///
/// # Errors
/// `int.Parse(cell, HexNumber)` failing, or an index past the parameters.
pub fn teleloc_position(parameters: &[String]) -> Result<Option<Position>, String> {
    let cell_text = if parameters
        .first()
        .ok_or("IndexOutOfRangeException")?
        .starts_with("0x")
    {
        substring_from(&parameters[0], 2)
    } else {
        parameters[0].clone()
    };
    // `(uint)int.Parse(text, NumberStyles.HexNumber)`: the same bits as a hex uint parse
    let cell = dotnet_parse::uint_try_parse_hex(&cell_text).ok_or("FormatException")?;

    let mut position_data = [0f32; 7];
    for i in 0..7usize {
        if i > 2 && parameters.len() < 8 {
            position_data[3] = 1.0;
            position_data[4] = 0.0;
            position_data[5] = 0.0;
            position_data[6] = 0.0;
            break;
        }

        let text = parameters.get(i + 1).ok_or("IndexOutOfRangeException")?;
        let Some(position) = dotnet_parse::float_try_parse(text.trim_matches([' ', '[', ']']))
        else {
            return Ok(None);
        };

        position_data[i] = position;
    }

    Ok(Some(Position::from_components(
        cell,
        position_data[0],
        position_data[1],
        position_data[2],
        position_data[4],
        position_data[5],
        position_data[6],
        position_data[3],
        false,
    )))
}

// ACE: AdminCommands.HandleTeleportLOC
/// `teleloc cell [x y z] (qw qx qy qz)`: teleport yourself to the specified location.
pub fn handle_teleport_loc(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let ok = match teleloc_position(parameters) {
        Ok(None) => true,
        Ok(Some(position)) => {
            let me = session_player(w, require(session));
            catch_unwind(AssertUnwindSafe(|| {
                player_location::teleport(w, me, &position, false)
            }))
            .is_ok()
        }
        Err(_) => false,
    };
    if !ok {
        for line in TELELOC_HELP {
            send_server_message(w, session, line, ChatMessageType::Broadcast);
        }
    }
}

// ACE: AdminCommands.HandleTime
/// `time`: displays the server's current game time.
pub fn handle_time(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // @time - Displays the server's current game time.

    let message_utc = format!(
        "The current server time in UtcNow is: {}",
        to_common_string(w.now.utc)
    );
    //var messagePY = "The current server time translated to DerethDateTime is:\n" + Timers.CurrentLoreTime;
    let in_game = timers::current_in_game_time(w);
    let message_igpy = format!("The current server time shown in game client is:\n{in_game}");
    let message_tod = format!(
        "It is currently {:?} in game right now.",
        in_game.time_of_day()
    );

    write_output_info(w, session, &message_utc, ChatMessageType::WorldBroadcast);
    //CommandHandlerHelper.WriteOutputInfo(session, messagePY, ChatMessageType.WorldBroadcast);
    write_output_info(w, session, &message_igpy, ChatMessageType::WorldBroadcast);
    write_output_info(w, session, &message_tod, ChatMessageType::WorldBroadcast);
}

// ACE: AdminCommands.HandleTrophies
/// `trophies`: shows a list of the trophies dropped by the target creature, and the percentage
/// chance of dropping.
pub fn handle_trophies(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // @trophies - Shows a list of the trophies dropped by the target creature, and the percentage chance of dropping.

    let s = require(session);
    let player = session_player(w, s);
    if !has_selection(w, player) {
        return;
    }
    let object_id = selected_object_id(w, player);

    let wo = current_landblock_get_object(w, player, object_id);

    if object_id.is_player() {
        return;
    }

    let create_list = wo
        .and_then(|g| obj(w, g).biota.properties_create_list.clone())
        .filter(|l| !l.is_empty());
    let msg = match (wo.filter(|&g| is_creature(w, g)), create_list) {
        (Some(creature), Some(create_list)) => {
            let create_list: Vec<_> = create_list
                .iter()
                .filter(|i| {
                    (i.destination_type & DestinationType::Contain).0 != 0
                        || (i.destination_type & DestinationType::Treasure).0 != 0
                            && (i.destination_type & DestinationType::Wield).0 == 0
                })
                .cloned()
                .collect();

            let c = obj(w, creature);
            let wielded_treasure: Vec<ObjectGuid> = container::inventory(c)
                .keys()
                .copied()
                .chain(creature_equipment::equipped_objects_values(w, creature))
                .filter(|&i| {
                    obj(w, i)
                        .wo
                        .world_object
                        .destination_type
                        .contains(DestinationType::Treasure)
                })
                .collect();

            let wcid = c.biota.weenie_class_id;
            let mut msg = format!("Trophy Dump for {} (0x{creature})\n", name_of(w, creature));
            msg += &format!("WCID: {wcid}\n");
            msg += &format!(
                "WeenieClassName: {}\n",
                world_object_networking::shims::weenie_class_name(w, wcid)
            );

            if create_list.is_empty() {
                msg += "Creature has no trophies to drop.\n";
            } else {
                for item in &create_list {
                    let dest = item.destination_type.to_dotnet_string();
                    let shade = format_aligned(item.shade, 7, "P2");
                    let wcid = format_aligned(item.weenie_class_id, 5, "");
                    if item.weenie_class_id == 0 {
                        msg += &format!("{dest}: {shade} - {wcid} - Nothing\n");
                        continue;
                    }

                    let weenie = w.content.get_cached_weenie(item.weenie_class_id);
                    let class_name = weenie.as_ref().map_or_else(
                        || "Item not found in DB".to_owned(),
                        |x| x.class_name.clone().unwrap_or_default(),
                    );
                    let name = weenie.as_ref().map_or_else(
                        || "Item not found in DB".to_owned(),
                        |x| x.get_property(PropertyString::Name).unwrap_or_default(),
                    );
                    msg += &format!("{dest}: {shade} - {wcid} - {class_name} - {name}\n");
                }
            }

            if wielded_treasure.is_empty() {
                msg += "Creature has no wielded items to drop.\n";
            } else {
                for item in wielded_treasure {
                    let o = obj(w, item);
                    let item_wcid = o.biota.weenie_class_id;
                    msg += &format!(
                        "{}: 100.00% - {} - {} - {}\n",
                        o.wo.world_object.destination_type.to_dotnet_string(),
                        format_aligned(item_wcid, 5, ""),
                        world_object_networking::shims::weenie_class_name(w, item_wcid),
                        name_of(w, item)
                    );
                }
            }
            msg
        }
        _ => {
            let wo = wo.expect("NullReferenceException: wo.Name");
            format!("{} (0x{wo}) has no trophies.", name_of(w, wo))
        }
    };

    system_chat(w, s, &msg, ChatMessageType::System);
}

// ACE: AdminCommands.HandleUnlock
/// `unlock {-all | IID}`: ACE's body is empty ("TODO: output").
pub fn handle_unlock(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // usage: @unlock {-all | IID}
    // Cleans the SQL lock on either everyone or the given player.
    // @unlock - Cleans the SQL lock on either everyone or the given player.

    // TODO: output
}

// ACE: AdminCommands.HandleGamecast
/// `gamecast <message>`: sends a world-wide broadcast.
pub fn handle_gamecast(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // > Broadcast from usage: @gamecast<message>
    // This command sends a world-wide broadcast to everyone in the game. Text is prefixed with 'Broadcast from (admin-name)> '.
    // See Also: @gamecast, @gamecastemote, @gamecastlocal, @gamecastlocalemote.
    // @gamecast - Sends a world-wide broadcast.

    //session.Player.HandleActionWorldBroadcast($"Broadcast from {session.Player.Name}> {string.Join(" ", parameters)}", ChatMessageType.WorldBroadcast);

    let sender = session_player_opt(w, session);
    let from = sender.map_or_else(|| "System".to_owned(), |p| name_of(w, p));
    let msg = format!("Broadcast from {from}> {}", join(parameters));
    let sys_message = game_message_system_chat(&msg, ChatMessageType::WorldBroadcast);
    player_manager::broadcast_to_all(w, &sys_message);
    player_manager::log_broadcast_chat(w, Channel::AllBroadcast, sender, &msg);
}

// ACE: AdminCommands.HandleAddSpell
/// `addspell <spellid>`: adds the specified spell to your own spellbook.
pub fn handle_add_spell(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if let Some(spell_id) = try_parse_spell_id(&parameters[0], true) {
        if spell_id.is_defined() {
            let player = session_player(w, require(session));
            player_learn_spell_with_networking(w, player, spell_id.0);
        }
    }
}

// ACE: AdminCommands.HandleRemoveSpell
/// `removespell <spellid>`: removes the specified spell to your own spellbook.
pub fn handle_remove_spell(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Some(spell_id) = try_parse_spell_id(&parameters[0], true) else {
        system_chat(
            w,
            s,
            &format!("Unknown spell {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };
    let player = session_player(w, s);
    if player_remove_known_spell(w, player, spell_id.0) {
        let spell = Spell::new(w, spell_id.0, false);
        system_chat(
            w,
            s,
            &format!("{} removed from spellbook.", spell.name()),
            ChatMessageType::Broadcast,
        );
    } else {
        system_chat(
            w,
            s,
            "You don't know that spell!",
            ChatMessageType::Broadcast,
        );
    }
}

/// The house usage text of `HandleAdminhouse`.
const ADMINHOUSE_HELP: &str = concat!(
    "@adminhouse dump: dumps info about currently selected house or house owned by currently selected player.\n",
    "@adminhouse dump name <name>: dumps info about house owned by the account of the named player.\n",
    "@adminhouse dump account <account_name>: dumps info about house owned by named account.\n",
    "@adminhouse dump hid <houseID>: dumps info about specified house.\n",
    "@adminhouse dump_all: dumps one line about each house in the world.\n",
    "@adminhouse dump_all summary: dumps info about total houses owned for each house type.\n",
    "@adminhouse dump_all dangerous: dumps full info about all houses. Use with caution.\n",
    "@adminhouse rent pay: fully pay the rent of the selected house.\n",
    "@adminhouse rent payall: fully pay the rent for all houses.\n",
    "@adminhouse payrent off / on: sets the targeted house to not require / require normal maintenance payments.\n",
);

/// `session.Player.SendMessage(msg)` (the default `ChatMessageType.Broadcast`).
fn me_send(w: &mut World, me: ObjectGuid, msg: &str) {
    player_send_message(w, me, msg, ChatMessageType::Broadcast);
}

/// `Time.GetDateTimeFromTimestamp(ts).ToLocalTime().ToCommonString()`.
fn timestamp_common_string(ts: f64) -> String {
    to_common_string(empyrean_common::time::Time::get_date_time_from_timestamp(
        ts,
    ))
}

// ACE: AdminCommands.HandleAdminhouse
/// `adminhouse`: house management tools for admins.
#[allow(clippy::too_many_lines)]
pub fn handle_adminhouse(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // @adminhouse dump: dumps info about currently selected house or house owned by currently selected player.
    // @adminhouse dump name<name>: dumps info about house owned by the account of the named player.
    // @adminhouse dump account<account_name>: dumps info about house owned by named account.
    // @adminhouse dump hid<houseID>: dumps info about specified house.
    // @adminhouse dump_all: dumps one line about each house in the world.
    // @adminhouse dump_all summary: dumps info about total houses owned for each house type.
    // @adminhouse dump_all dangerous: dumps full info about all houses.Use with caution.
    // @adminhouse rent pay: fully pay the rent of the selected house.
    // @adminhouse rent warn: rent timestamp is pushed far enough back in time to cause the rent to be almost due for the selected house.
    // @adminhouse rent due: rent timestamp is pushed back to cause the rent to be due for the selected house.
    // @adminhouse rent overdue: sets the rent timestamp far enough back to cause the selected house's rent to be overdue.
    // @adminhouse rent payall: fully pay the rent for all houses.
    // @adminhouse payrent on / off: sets the targeted house to not require / require normal maintenance payments.
    // @adminhouse - House management tools for admins.

    let s = require(session);
    let me = session_player(w, s);
    let p = |i: usize| parameters.get(i).map(String::as_str);

    if p(0) == Some("dump") {
        if parameters.len() == 1 {
            if has_selection(w, me) {
                let Some((house, wo)) = get_selected_house(w, s) else {
                    return;
                };

                dump_house(w, s, house, wo);
            } else {
                me_send(w, me, "No object is selected.");
            }
        } else if p(1) == Some("name") {
            let mut player_name = String::new();
            for param in &parameters[2..] {
                player_name += &format!("{param} ");
            }
            let player_name = player_name.trim().to_owned();

            if player_name.is_empty() {
                me_send(w, me, "You must specify a player's name.");
                return;
            }

            let (player, _) = player_manager::find_by_name(w, &player_name);

            let Some(player) = player else {
                me_send(
                    w,
                    me,
                    &format!("Could not find {player_name} in PlayerManager!"),
                );
                return;
            };

            //var houses = HouseManager.GetCharacterHouses(player.Guid.Full);
            let account_id = i_player::account(w, player)
                .expect("NullReferenceException: player.Account")
                .account_id;
            let houses = house_manager_get_account_houses(w, account_id);

            if houses.is_empty() {
                me_send(
                    w,
                    me,
                    &format!("Player {player_name} does not own a house."),
                );
                return;
            }

            for house in houses {
                dump_house(w, s, house, house);
            }
        } else if p(1) == Some("account") {
            let mut account_name = String::new();
            for param in &parameters[2..] {
                account_name += &format!("{param} ");
            }
            let account_name = account_name.trim().to_owned();

            if account_name.is_empty() {
                me_send(w, me, "You must specify an account name.");
                return;
            }

            let player = player_manager::get_all_players(w).into_iter().find(|&p| {
                let account = i_player::account(w, p).expect("NullReferenceException: p.Account");
                player_manager::equals_ordinal_ignore_case(&account.account_name, &account_name)
            });

            let Some(player) = player else {
                me_send(
                    w,
                    me,
                    &format!("Could not find {account_name} in PlayerManager!"),
                );
                return;
            };

            let account_id = i_player::account(w, player)
                .expect("NullReferenceException: player.Account")
                .account_id;
            let houses = house_manager_get_account_houses(w, account_id);

            if houses.is_empty() {
                me_send(
                    w,
                    me,
                    &format!("Account {account_name} does not own a house."),
                );
                return;
            }

            for house in houses {
                dump_house(w, s, house, house);
            }
        } else if p(1) == Some("hid") {
            // (ACE's `parameters.Length < 2` check cannot hold here; a missing id throws below.)
            let hid = parameters
                .get(2)
                .expect("IndexOutOfRangeException: parameters[2]");
            let Some(house_id) = dotnet_parse::uint_try_parse(hid) else {
                me_send(w, me, &format!("{hid} is not a valid house id."));
                return;
            };

            let houses = house_manager_get_house_by_id(w, house_id);

            if houses.is_empty() {
                me_send(
                    w,
                    me,
                    &format!("HouseId {house_id} is not currently owned."),
                );
                return;
            }

            for house in houses {
                dump_house(w, s, house, house);
            }
        } else {
            me_send(
                w,
                me,
                "You must specify either \"name\", \"account\" or \"hid\".",
            );
        }
    } else if p(0) == Some("dump_all") {
        if parameters.len() == 1 {
            for i in 1u32..6251 {
                let mut msg = format!("{i}: ");

                if let Some(house) = house_manager_get_house_by_id(w, i).first().copied() {
                    let h = obj(w, house);
                    let owner = h.house_owner();
                    let (house_type, owner_name, house_status) = (
                        h.house_type(),
                        h.house_owner_name().unwrap_or_default(),
                        h.house_status(),
                    );
                    let owner_player = player_manager::find_by_guid(w, owner.unwrap_or(0)).0;
                    let (buy_time, rent_time) = house_get_house_data(w, house, owner_player);
                    let rent_due = house_get_rent_due(w, house, rent_time);
                    let rent_paid = slum_lord_is_rent_paid(w, house);
                    msg += &format!(
                        "{} | Owner: {owner_name} (0x{}) | BuyTime: {} ({buy_time}) | RentTime: {} ({rent_time}) | RentDue: {} ({rent_due}) | Rent is {}paid{}",
                        house_type.to_dotnet_string(),
                        owner.map(|o| format(o, "X8")).unwrap_or_default(),
                        timestamp_common_string(f64::from(buy_time)),
                        timestamp_common_string(f64::from(rent_time)),
                        timestamp_common_string(f64::from(rent_due)),
                        if rent_paid { "" } else { "NOT " },
                        if house_status == HouseStatus::Active { String::new() } else { format!("  ({})", house_status.to_dotnet_string()) }
                    );
                } else {
                    msg += "House is NOT currently owned";
                }

                me_send(w, me, &msg);
            }
        } else if p(1) == Some("summary") {
            let apartments_total = 3000f64;
            let cottages_total = 2600f64;
            let villas_total = 570f64;
            let mansions_total = 80f64;

            let mut cottages = 0i32;
            let mut villas = 0i32;
            let mut mansions = 0i32;
            let mut apartments = 0i32;

            for i in 1u32..6251 {
                let Some(house) = house_manager_get_house_by_id(w, i).first().copied() else {
                    continue;
                };

                //var houseData = house.GetHouseData(PlayerManager.FindByGuid(new ObjectGuid(house.HouseOwner ?? 0)));
                match obj(w, house).house_type() {
                    HouseType::Apartment => apartments += 1,
                    HouseType::Cottage => cottages += 1,
                    HouseType::Mansion => mansions += 1,
                    HouseType::Villa => villas += 1,
                    _ => {}
                }
            }

            let apartments_avail = (apartments_total - f64::from(apartments)) / apartments_total;
            let cottages_avail = (cottages_total - f64::from(cottages)) / cottages_total;
            let villas_avail = (villas_total - f64::from(villas)) / villas_total;
            let mansions_avail = (mansions_total - f64::from(mansions)) / mansions_total;

            let mut msg = "HUD Report:\n".to_owned();
            msg += "=========================================================\n";

            msg += &hud_line(
                "Apartments:",
                apartments,
                apartments_total,
                apartments_avail,
            );
            msg += &hud_line("Cottages:", cottages, cottages_total, cottages_avail);
            msg += &hud_line("Villas:", villas, villas_total, villas_avail);
            msg += &hud_line("Mansions:", mansions, mansions_total, mansions_avail);

            let houses_total = apartments_total + cottages_total + villas_total + mansions_total;
            let houses_sold = apartments + cottages + villas + mansions;
            let houses_avail = (houses_total - f64::from(houses_sold)) / houses_total;

            msg += &hud_line("Total:", houses_sold, houses_total, houses_avail);

            msg += "=========================================================\n";

            me_send(w, me, &msg);
        } else if p(1) == Some("dangerous") {
            for i in 1u32..6251 {
                let houses = house_manager_get_house_by_id(w, i);

                if houses.is_empty() {
                    me_send(w, me, &format!("HouseId {i} is not currently owned."));
                    continue;
                }

                for house in houses {
                    dump_house(w, s, house, house);
                }
            }
        } else {
            me_send(
                w,
                me,
                "You must specify either nothing, \"summary\" or \"dangerous\".",
            );
        }
    } else if p(0) == Some("rent") {
        if p(1) == Some("pay") {
            if has_selection(w, me) {
                let Some((house, _wo)) = get_selected_house(w, s) else {
                    return;
                };

                if house_manager_pay_rent(w, house) {
                    let h = obj(w, house);
                    let text = format!(
                        "{} paid rent for HouseId {} (0x{house}:{})",
                        name_of(w, me),
                        h.house_id().map(|v| v.to_string()).unwrap_or_default(),
                        h.biota.weenie_class_id
                    );
                    player_manager::broadcast_to_audit_channel(w, Some(me), &text);
                }
            } else {
                me_send(w, me, "No object is selected.");
            }
        } else if p(1) == Some("payall") {
            house_manager_pay_all_rent(w);

            let text = format!("{} paid all rent for player housing.", name_of(w, me));
            player_manager::broadcast_to_audit_channel(w, Some(me), &text);
        } else {
            me_send(w, me, "You must specify either \"pay\" or \"payall\".");
        }
    } else if p(0) == Some("payrent") {
        let (target, done, already) = match p(1) {
            Some("off") => (
                HouseStatus::InActive,
                "is now maintenance free.",
                "is already maintenance free.",
            ),
            Some("on") => (
                HouseStatus::Active,
                "now requires maintenance.",
                "already requires maintenance.",
            ),
            _ => {
                me_send(w, me, "You must specify either \"on\" or \"off\".");
                return;
            }
        };
        if !has_selection(w, me) {
            me_send(w, me, "No object is selected.");
            return;
        }
        let Some((house, _)) = get_selected_house(w, s) else {
            return;
        };

        if obj(w, house).house_status() == target {
            me_send(
                w,
                me,
                &format!("{} (0x{house}) {already}", name_of(w, house)),
            );
            return;
        }
        obj_mut(w, house).set_house_status(target);
        dispatch::save_biota_to_database::save_biota_to_database(w, house, true);

        me_send(w, me, &format!("{} (0x{house}) {done}", name_of(w, house)));

        let house_owner = obj(w, house).house_owner();
        if house_owner.is_some_and(|o| o > 0) {
            if let Some(online_player) =
                player_manager::get_online_player(w, house_owner.unwrap_or(0))
            {
                let mut update_house_chain = ActionChain::new();
                update_house_chain.add_delay_seconds(w, f64::from(5.0f32));
                update_house_chain.add_action(Actor::Object(online_player), move |w| {
                    empyrean_world::world_objects::player_house::handle_action_query_house(
                        w,
                        online_player,
                    );
                });
                update_house_chain.enqueue_chain(w);
            }
        }

        let h = obj(w, house);
        let status = h.house_status();
        let text = format!(
            "{} set HouseStatus to {} for HouseId {} (0x{house}:{}) which equates to MaintenanceFree = {}",
            name_of(w, me),
            status.to_dotnet_string(),
            h.house_id().map(|v| v.to_string()).unwrap_or_default(),
            h.biota.weenie_class_id,
            bool_string(status == HouseStatus::InActive)
        );
        player_manager::broadcast_to_audit_channel(w, Some(me), &text);
    } else {
        me_send(w, me, ADMINHOUSE_HELP);
    }
}

/// One line of the `dump_all summary` report:
/// `string.Format("{0, -12} {1, 4:0} / {2, 4:0} ({3, 7:P2} available for purchase)\n", ...)`.
#[must_use]
pub fn hud_line(label: &str, sold: i32, total: f64, avail: f64) -> String {
    format!(
        "{} {} / {} ({} available for purchase)\n",
        empyrean_common::dotnet::align(label, -12),
        format_aligned(sold, 4, "0"),
        format_aligned(total, 4, "0"),
        format_aligned(avail, 7, "P2")
    )
}

// ACE: AdminCommands.DumpHouse
/// The house dump, written once the house's slumlord inventory is loaded
/// (`HouseManager.GetHouse(targetHouse.Guid.Full, house => { ... })`).
fn dump_house(w: &mut World, session: SessionId, target_house: ObjectGuid, wo: ObjectGuid) {
    let me = session_player(w, session);
    let wo_name = name_of(w, wo);
    empyrean_world::managers::house_manager::get_house(
        w,
        target_house.full(),
        Box::new(move |w: &mut World, house: ObjectGuid| {
            dump_house_callback(w, me, house, wo, &wo_name)
        }),
    );
}

/// `WorldObject.WeenieClassName`.
fn weenie_class_name_of(w: &World, g: ObjectGuid) -> String {
    let wcid = obj(w, g).biota.weenie_class_id;
    w.content
        .get_cached_weenie(wcid)
        .and_then(|x| x.class_name.clone())
        .unwrap_or_else(|| "WeenieClassName_NOT_FOUND".to_owned())
}

/// `Location.ToLOCString()`.
fn loc_string_of(w: &World, g: ObjectGuid) -> String {
    obj(w, g)
        .location()
        .expect("NullReferenceException: Location")
        .to_loc_string()
}

/// `{player.Name}` of `PlayerManager.FindByGuid(guid)`, or `none`.
fn found_name(w: &World, guid: u32, none: &str) -> String {
    player_manager::find_by_guid(w, guid).0.map_or_else(
        || none.to_owned(),
        |p| i_player::name(w, p).unwrap_or_default(),
    )
}

/// A nullable value interpolated (null is empty).
fn opt_string<T: ToString>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

/// The body of `DumpHouse`'s callback.
#[allow(clippy::too_many_lines)]
fn dump_house_callback(
    w: &mut World,
    me: ObjectGuid,
    house: ObjectGuid,
    wo: ObjectGuid,
    wo_name: &str,
) {
    use empyrean_world::world_objects::{house as h, slum_lord};

    let sys = ChatMessageType::System;
    let o = obj(w, house);
    let mut msg = format!("House Dump for {wo_name} (0x{wo})\n");
    msg += "===House=======================================\n";
    msg += &format!(
        "Name: {} | {} | WCID: {} | GUID: 0x{house}\n",
        name_of(w, house),
        weenie_class_name_of(w, house),
        o.biota.weenie_class_id
    );
    msg += &format!("Location: {}\n", loc_string_of(w, house));
    msg += &format!("HouseID: {}\n", opt_string(o.house_id()));
    msg += &format!(
        "HouseType: {} ({})\n",
        o.house_type().to_dotnet_string(),
        o.house_type().0
    );
    msg += &format!(
        "HouseStatus: {} ({})\n",
        o.house_status().to_dotnet_string(),
        o.house_status().0
    );
    // `(PlayScript)house.GetProperty(PropertyDataId.RestrictionEffect)`: a null did throws
    let effect = o
        .get_property(PropertyDataId::RestrictionEffect)
        .expect("InvalidOperationException: Nullable object must have a value.");
    msg += &format!(
        "RestrictionEffect: {} ({})\n",
        PlayScript(effect).to_dotnet_string(),
        effect
    );
    msg += &format!("HouseMaxHooksUsable: {}\n", o.house_max_hooks_usable());
    msg += &format!(
        "HouseCurrentHooksUsable: {}\n",
        o.house_current_hooks_usable()
    );
    msg += &format!(
        "HouseHooksVisible: {}\n",
        bool_string(o.house_hooks_visible().unwrap_or(false))
    );
    msg += &format!("OpenToEveryone: {}\n", bool_string(o.open_to_everyone()));
    player_send_message(w, me, &msg, sys);

    let linked = h::fields(w, house).linked_houses.clone();
    if !linked.is_empty() {
        let mut msg = "===LinkedHouses================================\n".to_owned();
        for link in linked {
            msg += &format!(
                "Name: {} | {} | WCID: {} | GUID: 0x{link}\n",
                name_of(w, link),
                weenie_class_name_of(w, link),
                obj(w, link).biota.weenie_class_id
            );
            msg += &format!("Location: {}\n", loc_string_of(w, link));
        }
        player_send_message(w, me, &msg, sys);
    }

    let slum_lord_g = h::slum_lord(w, house).expect("NullReferenceException: house.SlumLord");
    let sl = obj(w, slum_lord_g);
    let mut msg = "===SlumLord====================================\n".to_owned();
    msg += &format!(
        "Name: {} | {} | WCID: {} | GUID: 0x{slum_lord_g}\n",
        name_of(w, slum_lord_g),
        weenie_class_name_of(w, slum_lord_g),
        sl.biota.weenie_class_id
    );
    msg += &format!("Location: {}\n", loc_string_of(w, slum_lord_g));
    msg += &format!("MinLevel: {}\n", opt_string(sl.min_level()));
    msg += &format!(
        "AllegianceMinLevel: {}\n",
        sl.allegiance_min_level().unwrap_or(0)
    );
    msg += &format!(
        "HouseRequiresMonarch: {}\n",
        bool_string(sl.house_requires_monarch())
    );
    let rent_paid = slum_lord::is_rent_paid(w, slum_lord_g);
    msg += &format!("IsRentPaid: {}\n", bool_string(rent_paid));
    player_send_message(w, me, &msg, sys);

    let mut msg = "===HouseProfile================================\n".to_owned();
    let house_profile = slum_lord::get_house_profile(w, slum_lord_g);

    msg += &format!(
        "Type: {} | Bitmask: {}\n",
        house_profile.r#type.to_dotnet_string(),
        house_profile.bitmask.to_dotnet_string()
    );

    msg += &format!(
        "MinLevel: {} | MaxLevel: {}\n",
        house_profile.min_level, house_profile.max_level
    );
    msg += &format!(
        "MinAllegRank: {} | MaxAllegRank: {}\n",
        house_profile.min_alleg_rank, house_profile.max_alleg_rank
    );

    let owner_name = house_profile
        .owner_name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "N/A".to_owned());
    msg += &format!(
        "OwnerID: 0x{} | OwnerName: {owner_name}\n",
        house_profile.owner_id
    );
    msg += &format!(
        "MaintenanceFree: {}\n",
        bool_string(house_profile.maintenance_free)
    );
    let cost_name = |c: &empyrean_world::network::structure::house_payment::HousePayment| {
        if c.num > 1 {
            c.plural_name.clone()
        } else {
            c.name.clone()
        }
        .unwrap_or_default()
    };
    msg += "--== Buy Cost==--\n";
    for cost in &house_profile.buy {
        msg += &format!(
            "{} {} (WCID: {})\n",
            format(cost.num, "N0"),
            cost_name(cost),
            cost.weenie_id
        );
    }
    msg += "--==Rent Cost==--\n";
    for cost in &house_profile.rent {
        msg += &format!(
            "{} {} (WCID: {}) | Paid: {}\n",
            format(cost.num, "N0"),
            cost_name(cost),
            cost.weenie_id,
            format(cost.paid, "N0")
        );
    }
    player_send_message(w, me, &msg, sys);

    let owner = player_manager::find_by_guid(w, house_profile.owner_id.full()).0;
    let house_data = h::get_house_data(w, house, owner);
    // `if (houseData != null)`: GetHouseData never answers null
    let mut msg = "===HouseData===================================\n".to_owned();
    msg += &format!(
        "Location: {}\n",
        house_data
            .position
            .expect("NullReferenceException: houseData.Position")
            .to_loc_string()
    );
    msg += &format!("Type: {}\n", house_data.r#type.to_dotnet_string());
    let when = |t: u32| {
        if t > 0 {
            timestamp_common_string(f64::from(t))
        } else {
            "N/A".to_owned()
        }
    };
    msg += &format!(
        "BuyTime: {} ({})\n",
        when(house_data.buy_time),
        house_data.buy_time
    );
    msg += &format!(
        "RentTime: {} ({})\n",
        when(house_data.rent_time),
        house_data.rent_time
    );
    let rent_due = if house_data.rent_time > 0 {
        let due = h::get_rent_due(w, house, house_data.rent_time);
        format!("{} ({due})", timestamp_common_string(f64::from(due)))
    } else {
        " N/A (0)".to_owned()
    };
    msg += &format!("RentDue: {rent_due}\n");
    msg += &format!(
        "MaintenanceFree: {}\n",
        bool_string(house_data.maintenance_free)
    );
    player_send_message(w, me, &msg, sys);

    let links = append_house_link_dump(w, house);
    player_send_message(w, me, &links, sys);

    let house_type = obj(w, house).house_type();
    if house_type == HouseType::Villa || house_type == HouseType::Mansion {
        if let Some(basement) = h::get_dungeon_house(w, house) {
            let b = obj(w, basement);
            let mut msg = "===Basement====================================\n".to_owned();
            msg += &format!(
                "Name: {} | {} | WCID: {} | GUID: 0x{basement}\n",
                name_of(w, basement),
                weenie_class_name_of(w, basement),
                b.biota.weenie_class_id
            );
            msg += &format!("Location: {}\n", loc_string_of(w, basement));
            msg += &format!("HouseMaxHooksUsable: {}\n", b.house_max_hooks_usable());
            msg += &format!(
                "HouseCurrentHooksUsable: {}\n",
                b.house_current_hooks_usable()
            );
            player_send_message(w, me, &msg, sys);
            let links = append_house_link_dump(w, basement);
            player_send_message(w, me, &links, sys);
        }
    }

    let guest_list: Vec<(ObjectGuid, bool)> = h::fields(w, house)
        .guests
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    if !guest_list.is_empty() {
        let mut msg = "===GuestList===================================\n".to_owned();
        for (guest, storage) in guest_list {
            msg += &format!(
                "{} (0x{guest}){}\n",
                found_name(w, guest.full(), "[N/A]"),
                if storage { " *" } else { "" }
            );
        }
        msg += "* denotes granted access to the home's storage\n";
        player_send_message(w, me, &msg, sys);
    }

    let restriction_db =
        empyrean_world::network::structure::restriction_db::restriction_db_new(w, Some(house));
    let mut msg = "===RestrictionDB===============================\n".to_owned();
    msg += &format!(
        "HouseOwner: {} (0x{:08X})\n",
        found_name(w, restriction_db.house_owner, "N/A"),
        restriction_db.house_owner
    );
    msg += &format!("OpenStatus: {}\n", bool_string(restriction_db.open_status));
    msg += &format!(
        "MonarchID: {} (0x{})\n",
        found_name(w, restriction_db.monarch_id.full(), "N/A"),
        restriction_db.monarch_id
    );
    if !restriction_db.table.is_empty() {
        msg += "--==Guests==--\n";
        for (guest, value) in restriction_db.table.iter() {
            msg += &format!(
                "{} (0x{guest}){}\n",
                found_name(w, guest.full(), "[N/A]"),
                if *value == 1 { " *" } else { "" }
            );
        }
        msg += "* denotes granted access to the home's storage\n";
    }
    player_send_message(w, me, &msg, sys);

    let har = empyrean_world::network::structure::house_access::house_access_new(w, Some(house));
    let mut msg = "===HouseAccess=================================\n".to_owned();
    msg += &format!("Bitmask: {}\n", har.bitmask.to_dotnet_string());
    msg += &format!(
        "MonarchID: {} (0x{})\n",
        found_name(w, har.monarch_id.full(), "N/A"),
        har.monarch_id
    );
    if !har.guest_list.is_empty() {
        msg += "--==Guests==--\n";
        for (guest, info) in har.guest_list.iter() {
            msg += &format!(
                "{} (0x{guest}){}\n",
                info.guest_name
                    .clone()
                    .unwrap_or_else(|| "[N/A]".to_owned()),
                if info.item_storage_permission {
                    " *"
                } else {
                    ""
                }
            );
        }
        msg += "* denotes granted access to the home's storage\n";
    }
    if !har.roommates.is_empty() {
        msg += "--==Roommates==--\n";
        for guest in &har.roommates {
            msg += &format!("{} (0x{guest})\n", found_name(w, guest.full(), "[N/A]"));
        }
    }
    player_send_message(w, me, &msg, sys);
}

// ACE: AdminCommands.GetSelectedHouse
/// The selected object's house: a player's own, or the root house of a house, hook, storage chest,
/// slum lord or house portal. `None` (with ACE's message) otherwise; the pair is `(house, target)`.
fn get_selected_house(w: &mut World, session: SessionId) -> Option<(ObjectGuid, ObjectGuid)> {
    let me = session_player(w, session);
    let p = obj(w, me);
    let object_id = if let Some(t) = p.health_query_target() {
        ObjectGuid::new(t)
    } else if let Some(t) = p.mana_query_target() {
        ObjectGuid::new(t)
    } else {
        ObjectGuid::new(
            p.current_appraisal_target()
                .expect("InvalidOperationException: Nullable object must have a value."),
        )
    };

    let Some(target) = current_landblock_get_object(w, me, object_id) else {
        me_send(w, me, "No object is selected or unable to locate in world.");
        return None;
    };

    let house = if is_player(w, target) {
        let Some(house) = player_house(w, target) else {
            me_send(
                w,
                me,
                &format!("Player {} does not own a house.", name_of(w, target)),
            );
            return None;
        };

        //house = HouseManager.GetCharacterHouses(player.Guid.Full).FirstOrDefault();
        Some(house)
    } else {
        match dispatch::class_of(w, target) {
            dispatch::Class::House => house_root_house(w, target),
            dispatch::Class::Hook
            | dispatch::Class::Storage
            | dispatch::Class::SlumLord
            | dispatch::Class::HousePortal => {
                let linked =
                    world_object_house(w, target).expect("NullReferenceException: target.House");
                house_root_house(w, linked)
            }
            _ => {
                me_send(w, me, "Selected object is not a player or housing object.");
                return None;
            }
        }
    };

    let Some(house) = house else {
        me_send(w, me, "Selected house object is null");
        return None;
    };

    Some((house, target))
}

// ACE: AdminCommands.AppendHouseLinkDump
/// The storage, hooks, boot spot and house portal part of the house dump.
fn append_house_link_dump(w: &World, house: ObjectGuid) -> String {
    use empyrean_entity::enums::{HookGroupType, HookType};
    use empyrean_world::world_objects::{hook, house as h};

    let mut msg = String::new();

    let storage = h::storage(w, house);
    if !storage.is_empty() {
        msg += &format!("===Storage for House 0x{house}================\n");
        msg += &format!("Storage.Count: {}\n", storage.len());
        for chest in storage {
            msg += &format!(
                "Name: {} | {} | WCID: {} | GUID: 0x{chest}\n",
                name_of(w, chest),
                weenie_class_name_of(w, chest),
                obj(w, chest).biota.weenie_class_id
            );
            msg += &format!("Location: {}\n", loc_string_of(w, chest));
        }
    }

    let hooks = h::hooks(w, house);
    if !hooks.is_empty() {
        msg += &format!("===Hooks for House 0x{house}==================\n");
        let in_use = hooks.iter().filter(|&&k| hook::has_item(w, k)).count();
        msg += &format!(
            "Hooks.Count: {in_use} in use / {} max allowed usable / {} total\n",
            obj(w, house).house_max_hooks_usable(),
            hooks.len()
        );
        msg += "--==HooksGroups==--\n";
        for hook_group in HookGroupType::ALL {
            msg += &format!(
                "{}.Count: {} in use / {} max allowed per group\n",
                hook_group.to_dotnet_string(),
                h::get_hook_group_current_count(w, house, *hook_group),
                h::get_hook_group_max_count(w, house, *hook_group)
            );
        }
        msg += "--==Hooks==--\n";
        for k in hooks {
            let o = obj(w, k);
            msg += &format!(
                "Name: {} | {} | WCID: {} | GUID: 0x{k}\n",
                name_of(w, k),
                weenie_class_name_of(w, k),
                o.biota.weenie_class_id
            );
            // msg += $"Location: {hook.Location.ToLOCString()}\n";
            // `(HookType)hook.HookType`: a null HookType throws InvalidOperationException
            let hook_type = o
                .hook_type()
                .expect("InvalidOperationException: Nullable object must have a value.");
            let item = hook::item(w, k)
                .filter(|_| hook::has_item(w, k))
                .map_or_else(String::new, |i| {
                    let io = obj(w, i);
                    let group = io.hook_group();
                    format!(
                        " | Item on Hook: {} (0x{i}:{}:{}) | HookGroup: {} ({})",
                        name_of(w, i),
                        io.biota.weenie_class_id,
                        io.biota.weenie_type.to_dotnet_string(),
                        group.unwrap_or(HookGroupType::Undef).to_dotnet_string(),
                        group.map_or(0, |g| g.0)
                    )
                });
            msg += &format!(
                "HookType: {} ({hook_type}){item}\n",
                HookType(i32::from(hook_type)).to_dotnet_string()
            );
        }
    }

    if let Some(boot_spot) = h::boot_spot(w, house) {
        msg += &format!("===BootSpot for House 0x{house}===============\n");
        msg += &format!(
            "Name: {} | {} | WCID: {} | GUID: 0x{boot_spot}\n",
            name_of(w, boot_spot),
            weenie_class_name_of(w, boot_spot),
            obj(w, boot_spot).biota.weenie_class_id
        );
        msg += &format!("Location: {}\n", loc_string_of(w, boot_spot));
    }

    if let Some(portal) = h::house_portal(w, house) {
        msg += &format!("===HousePortal for House 0x{house}============\n");
        msg += &format!(
            "Name: {} | {} | WCID: {} | GUID: 0x{portal}\n",
            name_of(w, portal),
            weenie_class_name_of(w, portal),
            obj(w, portal).biota.weenie_class_id
        );
        msg += &format!("Location: {}\n", loc_string_of(w, portal));
        msg += &format!(
            "Destination: {}\n",
            obj(w, portal)
                .destination()
                .expect("NullReferenceException: Destination")
                .to_loc_string()
        );
    }

    msg
}

// ACE: AdminCommands.HandleBornAgain
/// `bornagain deletedCharID(, newCharName)(, accountName)`: restores a deleted character to an
/// account.
pub fn handle_born_again(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @bornagain deletedCharID[, newCharName[, accountName]]
    // Given the ID of a deleted character, this command restores that character to its owner.  (You can find the ID of a deleted character using the @finger command.)
    // If the deleted character's name has since been taken by a new character, you can specify a new name for the restored character as the second parameter.  (You can find if the name has been taken by also using the @finger command.)  Use a comma to separate the arguments.
    // If needed, you can specify an account name as a third parameter if the character should be restored to an account other than its original owner.  Again, use a comma between the arguments.
    // @bornagain - Restores a deleted character to an account.

    let mut hex_number = parameters[0].clone();

    if hex_number.starts_with("0x") {
        hex_number = substring_from(&hex_number, 2);
    }

    if hex_number.ends_with(',') {
        hex_number.pop();
    }

    if let Some(existing_char_iid) = dotnet_parse::uint_try_parse_hex(&hex_number) {
        let args = join(parameters);
        let existing_name = format!("0x{}", format(existing_char_iid, "X8"));

        if args.contains(',') {
            let two_commas = args.chars().filter(|&c| c == ',').count() == 2;

            let names: Vec<&str> = args.split(',').collect();

            let mut new_char_name = trim_spaces(names[1]);

            if new_char_name.starts_with('+') {
                new_char_name = substring_from(&new_char_name, 1);
            }
            new_char_name = upper_first(&new_char_name);

            if two_commas {
                let new_account_name = trim_spaces(names[2]).to_lowercase();

                let account = w.auth.lock().get_account_by_name(&new_account_name);

                let Some(account) = account else {
                    write_output_info(w, session, &format!("Error, cannot restore. Account \"{new_account_name}\" is not in database."), ChatMessageType::Broadcast);
                    return;
                };

                if player_manager::is_account_at_max_character_slots(w, &account.account_name) {
                    write_output_info(w, session, &format!("Error, cannot restore. Account \"{new_account_name}\" has no free character slots."), ChatMessageType::Broadcast);
                    return;
                }

                do_copy_char(
                    w,
                    require(session),
                    &existing_name,
                    existing_char_iid,
                    true,
                    Some(new_char_name),
                    account.account_id,
                );
            } else {
                let account_name = session_player_account_name(w, session);
                if player_manager::is_account_at_max_character_slots(w, &account_name) {
                    write_output_info(w, session, &format!("Error, cannot restore. Account \"{account_name}\" has no free character slots."), ChatMessageType::Broadcast);
                    return;
                }

                do_copy_char(
                    w,
                    require(session),
                    &existing_name,
                    existing_char_iid,
                    true,
                    Some(new_char_name),
                    0,
                );
            }
        } else {
            let account_name = session_player_account_name(w, session);
            if player_manager::is_account_at_max_character_slots(w, &account_name) {
                write_output_info(w, session, &format!("Error, cannot restore. Account \"{account_name}\" has no free character slots."), ChatMessageType::Broadcast);
                return;
            }

            do_copy_char(
                w,
                require(session),
                &existing_name,
                existing_char_iid,
                true,
                None,
                0,
            );
        }
    } else {
        write_output_info(
            w,
            session,
            "Error, cannot restore. You must include an existing character id in hex form.\nExample: @copychar 0x500000AC\n         @copychar 0x500000AC, Newly Restored\n         @copychar 0x500000AC, Newly Restored, differentaccount\n",
            ChatMessageType::Broadcast,
        );
    }
}

/// `player.Account`, which ACE dereferences.
fn player_account(w: &World, player: ObjectGuid) -> empyrean_store::models::auth::Account {
    obj(w, player)
        .player
        .as_ref()
        .and_then(|p| p.player.account.clone())
        .expect("NullReferenceException: Player.Account")
}

/// `session.Player.Account.AccountName`.
fn session_player_account_name(w: &World, session: Option<SessionId>) -> String {
    player_account(w, session_player(w, require(session))).account_name
}

/// `session.Account` (the session's account name; null interpolates as "").
fn session_account(w: &World, session: SessionId) -> String {
    w.sessions
        .get(session)
        .and_then(|s| s.account.clone())
        .unwrap_or_default()
}

// ACE: AdminCommands.HandleCopychar
/// `copychar < character name >, < copy name >`: copies an existing character into your
/// character list.
pub fn handle_copychar(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @copychar < character name >, < copy name >
    // Given the name of an existing character "character name", this command makes a copy of that character with the name "copy name" and places it into your character list.
    // @copychar - Copies an existing character into your character list.

    let s = require(session);
    if !join(parameters).contains(',') {
        write_output_info(
            w,
            session,
            "Error, cannot copy. You must include the existing character name followed by a comma and then the new name.\n Example: @copychar Old Name, New Name",
            ChatMessageType::Broadcast,
        );
        return;
    }

    let joined = join(parameters);
    let names: Vec<&str> = joined.split(',').collect();

    let mut existing_char_name = trim_spaces(names[0]);
    let mut new_char_name = trim_spaces(names[1]);

    if existing_char_name.starts_with('+') {
        existing_char_name = substring_from(&existing_char_name, 1);
    }
    if new_char_name.starts_with('+') {
        new_char_name = substring_from(&new_char_name, 1);
    }

    new_char_name = upper_first(&new_char_name);

    let (existing_player, _) = player_manager::find_by_name(w, &existing_char_name);

    let characters = w.sessions.get(s).map_or(0, |d| d.characters.len());
    let max_chars = property_manager_get_long(w, "max_chars_per_account");
    let Some(existing_player) =
        existing_player.filter(|_| i64::try_from(characters).unwrap_or(i64::MAX) < max_chars)
    else {
        //CommandHandlerHelper.WriteOutputInfo(session, $"Failed to copy the character \"{existingCharName}\" to a new character \"{newCharName}\" for the account \"{session.Account}\"! Does the character exist _AND_ is not currently logged in? Is the new character name already taken, or is the account out of free character slots?", ChatMessageType.Broadcast);
        let text = format!(
            "Failed to copy the character \"{existing_char_name}\" to a new character \"{new_char_name}\" for the account \"{}\"! Does the character exist? Is the new character name already taken, or is the account out of free character slots?",
            session_account(w, s)
        );
        write_output_info(w, session, &text, ChatMessageType::Broadcast);
        return;
    };

    do_copy_char(
        w,
        s,
        &existing_char_name,
        existing_player.guid().full(),
        false,
        Some(new_char_name),
        0,
    );
}

/// `idSwaps[id]` if present.
fn swap(id_swaps: &std::collections::HashMap<u32, u32>, id: u32) -> Option<u32> {
    id_swaps.get(&id).copied()
}

/// The item fix-ups of `DoCopyChar`: enchantment casters, then the owner and container (or
/// wielder) ids, then the bonded-to ids, where a swapped item takes `craftsman_name`.
fn copy_char_fix_item(
    item: &mut empyrean_entity::Biota,
    id_swaps: &std::collections::HashMap<u32, u32>,
    link: PropertyInstanceId,
    craftsman_name: &str,
) {
    if let Some(registry) = item.properties_enchantment_registry.as_mut() {
        for entry in registry {
            if let Some(new_id) = swap(id_swaps, entry.caster_object_id) {
                entry.caster_object_id = new_id;
            }
        }
    }

    if item.properties_iid.is_none() {
        return;
    }
    for key in [PropertyInstanceId::Owner, link] {
        let iids = item.properties_iid.as_mut().expect("checked");
        if let Some(new_id) = iids.get(&key).copied().and_then(|id| swap(id_swaps, id)) {
            iids.remove(&key);
            iids.add(key, new_id);
        }
    }
    for key in [
        PropertyInstanceId::AllowedActivator,
        PropertyInstanceId::AllowedWielder,
    ] {
        let iids = item.properties_iid.as_mut().expect("checked");
        if let Some(new_id) = iids.get(&key).copied().and_then(|id| swap(id_swaps, id)) {
            iids.remove(&key);
            iids.add(key, new_id);

            let strings = item
                .properties_string
                .as_mut()
                .expect("NullReferenceException: item.PropertiesString");
            strings.remove(&PropertyString::CraftsmanName);
            strings.add(PropertyString::CraftsmanName, craftsman_name.to_owned());
        }
    }
}

// ACE: AdminCommands.DoCopyChar
/// Copies (or, for a deleted character, restores) a character: its character row, its biota and
/// every possession under new guids, into `newAccountId` (or the admin's own account), then adds
/// it to `PlayerManager` and the account's character list.
fn do_copy_char(
    w: &mut World,
    session: SessionId,
    existing_char_name: &str,
    existing_char_id: u32,
    is_deleted_char: bool,
    new_character_name: Option<String>,
    new_account_id: u32,
) {
    let existing_char_name = existing_char_name.to_owned();
    w.shard.get_character(
        existing_char_id,
        Some(Box::new(move |w: &mut World, existing_character: Option<Character>| {
            let Some(existing_character) = existing_character else {
                let text = format!(
                    "Failed to {} the character \"{existing_char_name}\" to a new character \"{}\" for the account \"{}\"! Does the character exist? Is the new character name already taken, or is the account out of free character slots?",
                    if is_deleted_char { "restore" } else { "copy" },
                    new_character_name.clone().unwrap_or_default(),
                    session_account(w, session)
                );
                write_output_info(w, Some(session), &text, ChatMessageType::Broadcast);
                return;
            };
            let new_char_name = new_character_name.unwrap_or_else(|| existing_character.name.clone());

            let existing_player_biota = w.shard.base_database().get_biota(existing_char_id, false);

            w.shard.get_possessed_biotas_in_parallel(
                existing_char_id,
                Some(Box::new(move |w: &mut World, existing_possessions: PossessedBiotas| {
                    let name = new_char_name.clone();
                    w.shard.is_character_name_available(
                        name,
                        Some(Box::new(move |w: &mut World, is_available: bool| {
                            let copy = CopyChar { session, is_deleted_char, new_account_id, existing_character, existing_player_biota, existing_possessions, new_char_name };
                            do_copy_char_available(w, copy, is_available);
                        })),
                    );
                })),
            );
        })),
    );
}

/// What `DoCopyChar`'s callbacks have gathered by the time the name check answers.
struct CopyChar {
    session: SessionId,
    is_deleted_char: bool,
    new_account_id: u32,
    existing_character: Character,
    existing_player_biota: Option<empyrean_store::models::shard::Biota>,
    existing_possessions: PossessedBiotas,
    new_char_name: String,
}

/// `DoCopyChar`'s `IsCharacterNameAvailable` callback.
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
fn do_copy_char_available(w: &mut World, c: CopyChar, is_available: bool) {
    let CopyChar {
        session,
        is_deleted_char,
        new_account_id,
        existing_character,
        existing_player_biota,
        existing_possessions,
        new_char_name,
    } = c;
    let new_char_name = new_char_name.as_str();

    if !is_available {
        let text =
            format!(
            "{new_char_name} is not available to use for the {} character name, try another name.",
            if is_deleted_char { "restored" } else { "copied" }
        );
        write_output_info(w, Some(session), &text, ChatMessageType::Broadcast);
        return;
    }

    let new_player_guid = guid_manager::new_player_guid(w);

    let account_id = if new_account_id > 0 {
        new_account_id
    } else {
        player_account(w, session_player(w, session)).account_id
    };
    let id = new_player_guid.full();
    let mut new_character = Character {
        id,
        account_id,
        name: new_char_name.to_owned(),
        character_options_1: existing_character.character_options_1,
        character_options_2: existing_character.character_options_2,
        default_hair_texture: existing_character.default_hair_texture,
        gameplay_options: existing_character.gameplay_options.clone(),
        hair_texture: existing_character.hair_texture,
        is_plussed: existing_character.is_plussed,
        spellbook_filters: existing_character.spellbook_filters,
        total_logins: 1, // existingCharacter.TotalLogins
        ..Character::default()
    };

    for entry in &existing_character.character_properties_contract_registry {
        new_character.character_properties_contract_registry.push(
            shard::CharacterPropertiesContractRegistry {
                character_id: id,
                ..entry.clone()
            },
        );
    }
    for entry in &existing_character.character_properties_fill_comp_book {
        new_character.character_properties_fill_comp_book.push(
            shard::CharacterPropertiesFillCompBook {
                character_id: id,
                ..entry.clone()
            },
        );
    }
    for entry in &existing_character.character_properties_friend_list {
        new_character
            .character_properties_friend_list
            .push(shard::CharacterPropertiesFriendList {
                character_id: id,
                ..entry.clone()
            });
    }
    for entry in &existing_character.character_properties_quest_registry {
        new_character.character_properties_quest_registry.push(
            shard::CharacterPropertiesQuestRegistry {
                character_id: id,
                ..entry.clone()
            },
        );
    }
    for entry in &existing_character.character_properties_shortcut_bar {
        new_character.character_properties_shortcut_bar.push(
            shard::CharacterPropertiesShortcutBar {
                character_id: id,
                ..entry.clone()
            },
        );
    }
    for entry in &existing_character.character_properties_spell_bar {
        new_character
            .character_properties_spell_bar
            .push(shard::CharacterPropertiesSpellBar {
                character_id: id,
                ..entry.clone()
            });
    }
    for entry in &existing_character.character_properties_squelch {
        new_character
            .character_properties_squelch
            .push(shard::CharacterPropertiesSquelch {
                character_id: id,
                ..entry.clone()
            });
    }
    for entry in &existing_character.character_properties_title_book {
        new_character
            .character_properties_title_book
            .push(shard::CharacterPropertiesTitleBook {
                character_id: id,
                ..entry.clone()
            });
    }

    let mut id_swaps: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();

    let existing_player_biota =
        existing_player_biota.expect("NullReferenceException: existingPlayerBiota");
    let mut new_player_biota =
        BiotaConverter::convert_to_entity_biota(&existing_player_biota, false);

    id_swaps.insert(new_player_biota.id, id);

    new_player_biota.id = id;
    if let Some(a) = new_player_biota.properties_allegiance.as_mut() {
        a.clear();
    }
    if let Some(h) = new_player_biota.house_permissions.as_mut() {
        h.clear();
    }

    let mut new_temp_wielded_items = Vec::new();
    for item in &existing_possessions.wielded_items {
        let mut new_item_biota = BiotaConverter::convert_to_entity_biota(item, false);
        let new_guid = guid_manager::new_dynamic_guid(w);
        id_swaps.insert(new_item_biota.id, new_guid.full());
        new_item_biota.id = new_guid.full();
        new_temp_wielded_items.push(new_item_biota);
    }

    let mut new_temp_inventory_items = Vec::new();
    for item in &existing_possessions.inventory {
        if item.weenie_class_id == u32::from(WeenieClassName::W_DEED_CLASS.0) {
            continue;
        }

        let mut new_item_biota = BiotaConverter::convert_to_entity_biota(item, false);
        let new_guid = guid_manager::new_dynamic_guid(w);
        id_swaps.insert(new_item_biota.id, new_guid.full());
        new_item_biota.id = new_guid.full();
        new_temp_inventory_items.push(new_item_biota);
    }

    let mut new_wielded_items = Vec::new();
    for mut item in new_temp_wielded_items {
        copy_char_fix_item(
            &mut item,
            &id_swaps,
            PropertyInstanceId::Wielder,
            new_char_name,
        );
        new_wielded_items.push(BiotaConverter::convert_from_entity_biota(&item, false));
    }

    let plussed_name = format!(
        "{}{new_char_name}",
        if existing_character.is_plussed {
            "+"
        } else {
            ""
        }
    );
    let mut new_inventory_items = Vec::new();
    for mut item in new_temp_inventory_items {
        copy_char_fix_item(
            &mut item,
            &id_swaps,
            PropertyInstanceId::Container,
            &plussed_name,
        );
        new_inventory_items.push(BiotaConverter::convert_from_entity_biota(&item, false));
    }

    let class = if new_player_biota.weenie_type == WeenieType::Admin {
        dispatch::Class::Admin
    } else if new_player_biota.weenie_type == WeenieType::Sentinel {
        dispatch::Class::Sentinel
    } else {
        dispatch::Class::Player
    };
    let mut new_player = CtorEnv::with_world(w, |env| {
        empyrean_world::world_objects::player::player_from_biota_with_character(
            env,
            class,
            new_player_biota,
            new_inventory_items,
            new_wielded_items,
            new_character,
            Some(session),
        )
    });

    new_player.set_property(PropertyString::Name, new_char_name.to_owned());
    new_player.wo.world_object_database.changes_detected = true;
    if let Some(p) = new_player.player.as_mut() {
        p.player_database.character_changes_detected = true;
    }

    // newPlayer.Allegiance = null: a new Player's Allegiance is already null (Player.Allegiance
    // is set by AllegianceManager at login).
    new_player.remove_property(PropertyInt::AllegianceOfficerRank);
    new_player.remove_property(PropertyInstanceId::Monarch);
    new_player.remove_property(PropertyInstanceId::Patron);
    new_player.remove_property(PropertyDataId::HouseId);
    new_player.remove_property(PropertyInstanceId::House);

    if let Some(character) = new_player
        .player
        .as_mut()
        .and_then(|p| p.player.character.as_mut())
    {
        for entry in &mut character.character_properties_shortcut_bar {
            if let Some(new_id) = swap(&id_swaps, entry.shortcut_object_id) {
                entry.shortcut_object_id = new_id;
            }
        }
    }

    if let Some(registry) = new_player.biota.properties_enchantment_registry.as_mut() {
        for entry in registry {
            if let Some(new_id) = swap(&id_swaps, entry.caster_object_id) {
                entry.caster_object_id = new_id;
            }
        }
    }

    // `newPlayer.GetAllPossessions()`: the objects the constructor built from the possessions.
    let possessed_biotas: Vec<empyrean_entity::Biota> =
        new_player.player.as_ref().and_then(|p| p.player.login_possessions.as_ref()).map_or_else(Vec::new, |lp| {
            lp.inventory
                .iter()
                .chain(lp.wielded_items.iter())
                .map(|b| empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(b, false))
                .collect()
        });

    let character = new_player
        .player
        .as_ref()
        .and_then(|p| p.player.character.clone())
        .expect("NullReferenceException: newPlayer.Character");
    let new_player_name = new_player
        .get_property(PropertyString::Name)
        .unwrap_or_default();
    let new_player_account_name = new_player
        .player
        .as_ref()
        .and_then(|p| p.player.account.as_ref())
        .map(|a| a.account_name.clone());
    let new_player_biota = new_player.biota.clone();
    let existing_display = format!(
        "{}{}",
        if existing_character.is_plussed {
            "+"
        } else {
            ""
        },
        existing_character.name
    );

    // We must await here --
    w.shard.add_character_in_parallel(
        new_player_biota.clone(),
        possessed_biotas,
        character.clone(),
        Some(Box::new(move |w: &mut World, save_success: bool| {
            let account_name = new_player_account_name.expect("NullReferenceException: newPlayer.Account");
            if !save_success {
                //CommandHandlerHelper.WriteOutputInfo(session, $"Failed to copy the character \"{(existingCharacter.IsPlussed ? "+" : "")}{existingCharacter.Name}\" to a new character \"{newPlayer.Name}\" for the account \"{newPlayer.Account.AccountName}\"! Does the character exist _AND_ is not currently logged in? Is the new character name already taken, or is the account out of free character slots?", ChatMessageType.Broadcast);
                let text = format!(
                    "Failed to {} the character \"{existing_display}\" to a new character \"{new_player_name}\" for the account \"{account_name}\"! Does the character exist? Is the new character name already taken, or is the account out of free character slots?",
                    if is_deleted_char { "restore" } else { "copy" }
                );
                write_output_info(w, Some(session), &text, ChatMessageType::Broadcast);
                return;
            }

            player_manager::add_offline_player_biota(w, new_player_biota);

            if new_account_id == 0 {
                if let Some(s) = w.sessions.get_mut(session) {
                    s.characters.push(character);
                }
            } else if let Some(found_active_session) = w.net.find_by_account_id(new_account_id) {
                if let Some(s) = w.sessions.get_mut(found_active_session) {
                    s.characters.push(character);
                }
            }

            let msg = format!(
                "Successfully {} the character \"{existing_display}\" to a new character \"{new_player_name}\" for the account \"{account_name}\".",
                if is_deleted_char { "restored" } else { "copied" }
            );
            write_output_info(w, Some(session), &msg, ChatMessageType::Broadcast);
            let me = w.sessions.player(session);
            player_manager::broadcast_to_audit_channel(w, me, &msg);
        })),
    );
}

// ACE: AdminCommands.HandleCreate
/// Creates an object or objects in the world
pub fn handle_create(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    if let Some(p) = parse_create_parameters(w, s, parameters, false) {
        try_create_object(w, s, &p.weenie, p.num_to_spawn, p.palette, p.shade, None);
    }
}

// ACE: AdminCommands.HandleCreateLiveOps
/// Creates an object or objects in the world -- with lifespans for live events
pub fn handle_create_live_ops(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    if let Some(p) = parse_create_parameters(w, s, parameters, true) {
        try_create_object(
            w,
            s,
            &p.weenie,
            p.num_to_spawn,
            p.palette,
            p.shade,
            p.lifespan,
        );
    }
}

/// The `out` values of `ParseCreateParameters`.
#[derive(Debug, Clone)]
pub struct CreateParameters {
    pub weenie: std::sync::Arc<empyrean_entity::Weenie>,
    pub num_to_spawn: i32,
    pub palette: Option<i32>,
    pub shade: Option<f32>,
    pub lifespan: Option<i32>,
}

// ACE: AdminCommands.ParseCreateParameters
/// Parses the command-line parameters for /create or /createliveops. The only difference with
/// /createliveops is that it includes a lifespan parameter in the middle. `None` is ACE's
/// `false` (a message has been sent).
fn parse_create_parameters(
    w: &mut World,
    session: SessionId,
    parameters: &[String],
    has_lifespan: bool,
) -> Option<CreateParameters> {
    let weenie = get_weenie_for_create(w, session, &parameters[0], false);
    let mut num_to_spawn = 1;
    let mut palette = None;
    let mut shade = None;
    let mut lifespan = None;

    let weenie = weenie?;

    if parameters.len() > 1 {
        let Some(n) = dotnet_parse::int_try_parse(&parameters[1]) else {
            system_chat(
                w,
                session,
                &format!(
                    "Amount to spawn must be a number between {} - {}.",
                    i32::MIN,
                    i32::MAX
                ),
                ChatMessageType::Broadcast,
            );
            return None;
        };
        num_to_spawn = n;
    }

    let mut idx = 2;

    if has_lifespan {
        if parameters.len() > 2 {
            let Some(l) = dotnet_parse::int_try_parse(&parameters[2]) else {
                system_chat(
                    w,
                    session,
                    &format!(
                        "Lifespan must be a number between {} - {}.",
                        i32::MIN,
                        i32::MAX
                    ),
                    ChatMessageType::Broadcast,
                );
                return None;
            };
            lifespan = Some(l);
        } else {
            lifespan = Some(3600);
        }

        idx += 1;
    }

    if parameters.len() > idx {
        let Some(p) = dotnet_parse::int_try_parse(&parameters[idx]) else {
            system_chat(
                w,
                session,
                &format!(
                    "Palette must be number between {} - {}.",
                    i32::MIN,
                    i32::MAX
                ),
                ChatMessageType::Broadcast,
            );
            return None;
        };
        palette = Some(p);

        idx += 1;
    }

    if parameters.len() > idx {
        let Some(sh) = dotnet_parse::float_try_parse(&parameters[idx]) else {
            system_chat(
                w,
                session,
                &format!(
                    "Shade must be number between {} - {}.",
                    to_string(f32::MIN),
                    to_string(f32::MAX)
                ),
                ChatMessageType::Broadcast,
            );
            return None;
        };
        shade = Some(sh);
    }

    Some(CreateParameters {
        weenie,
        num_to_spawn,
        palette,
        shade,
        lifespan,
    })
}

// ACE: AdminCommands.GetWeenieForCreate
/// Returns a weenie for a wcid or classname for /create, /createliveops, and /ci. Performs some
/// basic verifications for weenie types that are safe to spawn with these commands
fn get_weenie_for_create(
    w: &mut World,
    session: SessionId,
    weenie_desc: &str,
    for_inventory: bool,
) -> Option<std::sync::Arc<empyrean_entity::Weenie>> {
    let weenie = match dotnet_parse::uint_try_parse(weenie_desc) {
        Some(wcid) => w.content.get_cached_weenie(wcid),
        None => w.content.get_cached_weenie_by_class_name(weenie_desc),
    };

    let Some(weenie) = weenie else {
        system_chat(
            w,
            session,
            &format!("{weenie_desc} is not a valid weenie."),
            ChatMessageType::Broadcast,
        );
        return None;
    };

    let class_name = weenie.class_name.clone().unwrap_or_default();
    if !verify_create_weenie_type(weenie.weenie_type) {
        system_chat(
            w,
            session,
            &format!(
                "You cannot spawn {class_name} because it is a {}",
                weenie.weenie_type.to_dotnet_string()
            ),
            ChatMessageType::Broadcast,
        );
        return None;
    }

    if for_inventory && weenie.is_stuck() {
        system_chat(
            w,
            session,
            &format!(
                "You cannot spawn {class_name} in your inventory because it cannot be picked up"
            ),
            ChatMessageType::Broadcast,
        );
        return None;
    }

    Some(weenie)
}

// ACE: AdminCommands.VerifyCreateWeenieType
/// False for the weenie types the create commands refuse to spawn.
#[must_use]
pub fn verify_create_weenie_type(weenie_type: WeenieType) -> bool {
    !matches!(
        weenie_type,
        WeenieType::Admin
            | WeenieType::AI
            | WeenieType::Allegiance
            | WeenieType::BootSpot
            | WeenieType::Channel
            | WeenieType::CombatPet
            | WeenieType::Deed
            | WeenieType::Entity
            | WeenieType::EventCoordinator
            | WeenieType::Game
            | WeenieType::GamePiece
            | WeenieType::GScoreGatherer
            | WeenieType::GScoreKeeper
            | WeenieType::GSpellEconomy
            | WeenieType::Hook
            | WeenieType::House
            | WeenieType::HousePortal
            | WeenieType::HUD
            | WeenieType::InGameStatKeeper
            | WeenieType::LScoreKeeper
            | WeenieType::LSpellEconomy
            | WeenieType::Machine
            | WeenieType::Pet
            | WeenieType::ProjectileSpell
            | WeenieType::Sentinel
            | WeenieType::SlumLord
            | WeenieType::SocialManager
            | WeenieType::Storage
            | WeenieType::Undef
            | WeenieType::UNKNOWN__GUESSEDNAME32
    )
}

// ACE: AdminCommands.TryCreateObject
/// Attempts to spawn some # of weenies in the world for /create or /createliveops
fn try_create_object(
    w: &mut World,
    session: SessionId,
    weenie: &std::sync::Arc<empyrean_entity::Weenie>,
    num_to_spawn: i32,
    palette: Option<i32>,
    shade: Option<f32>,
    lifespan: Option<i32>,
) {
    let obj_guid = create_object_for_command(w, session, weenie);

    let Some(o) = obj_guid.filter(|_| num_to_spawn >= 1) else {
        system_chat(
            w,
            session,
            "No object was created.",
            ChatMessageType::Broadcast,
        );
        // (ACE leaves the unspawned object to the garbage collector.)
        if let Some(o) = obj_guid {
            w.objects.remove(o);
        }
        return;
    };

    let mut objs = Vec::new();

    if num_to_spawn == 1 {
        objs.push(o);
    } else {
        let max_stack_size = obj(w, o).max_stack_size();
        if weenie.is_stackable() && max_stack_size.is_some() {
            let max_stack_size = i32::from(max_stack_size.unwrap_or_default());
            let full_stacks = num_to_spawn
                .checked_div(max_stack_size)
                .expect("DivideByZeroException");
            let last_stack_amount = num_to_spawn % max_stack_size;

            for _ in 0..full_stacks {
                let stack = create_object_for_command(w, session, weenie)
                    .expect("NullReferenceException: stack");
                obj_mut(w, stack).set_stack_size(Some(max_stack_size));
                objs.push(stack);
            }
            if last_stack_amount > 0 {
                obj_mut(w, o).set_stack_size(Some(last_stack_amount));
                objs.push(o);
            }
        } else {
            // The number of weenies to spawn will be limited by the physics engine.
            for _ in 0..num_to_spawn {
                objs.push(
                    create_object_for_command(w, session, weenie)
                        .expect("NullReferenceException: obj"),
                );
            }
        }
    }

    for &wo in &objs {
        let x = obj_mut(w, wo);
        if palette.is_some() {
            x.set_palette_template(palette);
        }

        if let Some(shade) = shade {
            x.set_shade(Some(f64::from(shade)));
        }

        if lifespan.is_some() {
            x.set_lifespan(lifespan);
        }

        dispatch::enter_world::enter_world(w, wo);
    }

    let me = session_player(w, session);
    let location = obj(w, o)
        .location()
        .map(|l| l.to_loc_string())
        .unwrap_or_default();
    let text = if num_to_spawn > 1 {
        format!(
            "{} has created {num_to_spawn} {} (0x{o}) near {location}.",
            name_of(w, me),
            name_of(w, o)
        )
    } else {
        format!(
            "{} has created {} (0x{o}) at {location}.",
            name_of(w, me),
            name_of(w, o)
        )
    };
    player_manager::broadcast_to_audit_channel(w, Some(me), &text);

    // DIVERGE: ACE ignores `EnterWorld`'s result, and an object that did not enter is left to the
    // garbage collector once the command returns; here it leaves `World.objects`.
    // (the first object stays out of `objs` when the stacks divide evenly)
    for wo in objs.into_iter().chain([o]) {
        empyrean_world::world_objects::world_object::drop_unreferenced(w, wo);
    }
}

// ACE: AdminCommands.LastSpawnPos
/// The location of the last object a create command made.
pub static LAST_SPAWN_POS: Mutex<Option<Position>> = Mutex::new(None);

// ACE: AdminCommands.CreateObjectForCommand
/// Creates WorldObjects from Weenies for /create, /createliveops, and /ci: the object joins
/// `World.objects` in front of the admin (not yet in the world).
fn create_object_for_command(
    w: &mut World,
    session: SessionId,
    weenie: &std::sync::Arc<empyrean_entity::Weenie>,
) -> Option<ObjectGuid> {
    // `WorldObjectFactory.CreateNewWorldObject(weenie)`
    let guid = guid_manager::new_dynamic_guid(w);
    let Some(mut o) = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(
            env,
            Some(weenie.clone()),
            guid,
        )
    }) else {
        guid_manager::recycle_dynamic_guid(w, guid);
        // (ACE reads `obj.WeenieType` on the null result)
        panic!(
            "NullReferenceException: CreateNewWorldObject({})",
            weenie.weenie_class_id
        );
    };

    //if (obj.TimeToRot == null)
    //obj.TimeToRot = double.MaxValue;

    let me = session_player(w, session);
    let location = location_of(w, me);
    let mut new_location = if o.biota.weenie_type == WeenieType::Creature {
        location.in_front_of(f64::from(5f32), true)
    } else {
        let dist = empyrean_common::dotnet::math::max_f32(2f32, o.use_radius().unwrap_or(2f32));

        location.in_front_of(f64::from(dist), false)
    };

    let cell = empyrean_world::entity::position_extensions::get_cell(w, &new_location);
    new_location.set_landblock_id(empyrean_entity::LandblockId::new(cell));
    o.set_location(Some(new_location));

    *LAST_SPAWN_POS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(new_location);

    let guid = o.guid;
    assert!(w.objects.insert(o).is_ok(), "fresh dynamic guid");
    empyrean_world::world_objects::creature::post_insert(w, guid);
    Some(guid)
}

// ACE: AdminCommands.HandleCreateNamed
/// Creates a named object in the world
pub fn handle_create_named(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Some(weenie) = get_weenie_for_create(w, s, &parameters[0], false) else {
        return;
    };

    let Some(count) = dotnet_parse::int_try_parse(&parameters[1]) else {
        system_chat(
            w,
            s,
            "count must be an integer value",
            ChatMessageType::Broadcast,
        );
        return;
    };

    if count < 1 || count > i32::from(u16::MAX) {
        system_chat(
            w,
            s,
            &format!("count must be a between 1 and {}", u16::MAX),
            ChatMessageType::Broadcast,
        );
        return;
    }

    let named = join(&parameters[2..]);

    let mut first = None;
    let mut created = Vec::new();

    for _ in 0..count {
        let Some(o) = create_object_for_command(w, s, &weenie) else {
            return;
        };

        if first.is_none() {
            first = Some(o);
        }

        obj_mut(w, o).set_property(PropertyString::Name, named.clone());

        dispatch::enter_world::enter_world(w, o);
        created.push(o);
    }

    let first = first.expect("count >= 1");
    let me = session_player(w, s);
    let location = obj(w, first)
        .location()
        .map(|l| l.to_loc_string())
        .unwrap_or_default();
    let text = if count == 1 {
        format!(
            "{} has created {} (0x{first}) at {location}.",
            name_of(w, me),
            name_of(w, first)
        )
    } else {
        format!(
            "{} has created {count}x {} at {location}.",
            name_of(w, me),
            name_of(w, first)
        )
    };
    player_manager::broadcast_to_audit_channel(w, Some(me), &text);

    // DIVERGE: as in `try_create_object`, an object that did not enter leaves `World.objects`.
    for wo in created {
        empyrean_world::world_objects::world_object::drop_unreferenced(w, wo);
    }
}

// ACE: AdminCommands.HandleCI
/// Creates an object in your inventory
pub fn handle_ci(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Some(weenie) = get_weenie_for_create(w, s, &parameters[0], true) else {
        return;
    };

    let mut stack_size: u16 = 0;
    let mut palette = None;
    let mut shade = None;

    if parameters.len() > 1 {
        // `ushort.TryParse(parameters[1], out stackSize) || stackSize == 0`
        match dotnet_parse::uint_try_parse(&parameters[1]).and_then(|v| u16::try_from(v).ok()) {
            Some(v) if v != 0 => stack_size = v,
            _ => {
                system_chat(
                    w,
                    s,
                    &format!("stacksize must be number between 1 - {}", u16::MAX),
                    ChatMessageType::Broadcast,
                );
                return;
            }
        }
    }

    if parameters.len() > 2 {
        let Some(p) = dotnet_parse::int_try_parse(&parameters[2]) else {
            system_chat(
                w,
                s,
                &format!("palette must be number between {} - {}", i32::MIN, i32::MAX),
                ChatMessageType::Broadcast,
            );
            return;
        };
        palette = Some(p);
    }

    if parameters.len() > 3 {
        let Some(sh) = dotnet_parse::float_try_parse(&parameters[3]) else {
            system_chat(
                w,
                s,
                &format!(
                    "shade must be number between {} - {}",
                    to_string(f32::MIN),
                    to_string(f32::MAX)
                ),
                ChatMessageType::Broadcast,
            );
            return;
        };
        shade = Some(sh);
    }

    let Some(o) = create_object_for_command(w, s, &weenie) else {
        // already sent an error message
        return;
    };

    if let Some(max_stack_size) = obj(w, o).max_stack_size().filter(|_| stack_size != 0) {
        stack_size = stack_size.min(max_stack_size);

        obj_mut(w, o).set_stack_size(Some(i32::from(stack_size)));
    }

    if palette.is_some() {
        obj_mut(w, o).set_palette_template(palette);
    }

    if let Some(shade) = shade {
        obj_mut(w, o).set_shade(Some(f64::from(shade)));
    }

    let me = session_player(w, s);
    player_inventory::try_create_in_inventory_with_networking(w, me, o);

    let text = format!(
        "{} has created {} (0x{o}) in their inventory.",
        name_of(w, me),
        name_of(w, o)
    );
    player_manager::broadcast_to_audit_channel(w, Some(me), &text);
}

// ACE: AdminCommands.HandleCrack
/// `crack [.]`: cracks the most recently appraised locked target (and with ".", opens it too).
pub fn handle_crack(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let open_it = parameters.first().is_some_and(|p| p == ".");
    let mut not_ok = false;
    let me = session_player(w, require(session));
    if let Some(target) = obj(w, me).current_appraisal_target() {
        let object_id = ObjectGuid::new(target);
        let wo = current_landblock_get_object(w, me, object_id);
        if let Some(lock) = wo.filter(|&g| {
            matches!(
                dispatch::class_of(w, g),
                dispatch::Class::Door | dispatch::Class::Chest
            )
        }) {
            let weenie_type = obj(w, lock).biota.weenie_type.to_dotnet_string();
            let opening = if open_it {
                format!(" Opening {weenie_type}.")
            } else {
                String::new()
            };
            let lock_code = lock_helper_get_lock_code(w, lock);
            let resist_lockpick = lock_helper_get_resist_lockpick(w, lock);

            if lock_code.as_deref().is_some_and(|c| !c.trim().is_empty()) {
                let lock_code = lock_code.unwrap_or_default();
                let res = lock_unlock_with_key(w, lock, me.full(), &lock_code);
                send_server_message(
                    w,
                    session,
                    &format!("Crack {weenie_type} via {lock_code} result: {res}.{opening}"),
                    ChatMessageType::Broadcast,
                );
            } else if let Some(resist) = resist_lockpick.filter(|&r| r > 0) {
                let res = lock_unlock_with_skill(
                    w,
                    lock,
                    me.full(),
                    resist.wrapping_mul(2).cast_unsigned(),
                );
                send_server_message(
                    w,
                    session,
                    &format!("Crack {weenie_type} with skill {resist}*2 result: {res}.{opening}"),
                    ChatMessageType::Broadcast,
                );
            } else {
                send_server_message(
                    w,
                    session,
                    &format!("The {weenie_type} has no key code or lockpick difficulty.  Unable to crack it.{opening}"),
                    ChatMessageType::Broadcast,
                );
            }

            if open_it {
                match dispatch::class_of(w, lock) {
                    dispatch::Class::Door => door_open(w, lock, me),
                    dispatch::Class::Chest => send_server_message(
                        w,
                        session,
                        &format!(
                            "The {weenie_type} cannot be opened because it is not implemented yet!"
                        ),
                        ChatMessageType::Broadcast,
                    ),
                    _ => {}
                }
            }
        } else {
            not_ok = true;
        }
    } else {
        not_ok = true;
    }
    if not_ok {
        send_server_message(
            w,
            session,
            "Appraise a locked target before using @crack",
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: AdminCommands.HandleDeathxp
/// Displays how much experience the last appraised creature is worth when killed.
pub fn handle_deathxp(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let Some(creature) = command_handler_helper::get_last_appraised_object(w, require(session))
    else {
        return;
    };

    let xp = obj(w, creature)
        .xp_override()
        .map(|x| x.to_string())
        .unwrap_or_default();
    write_output_info(
        w,
        session,
        &format!("{} XP: {xp}", name_of(w, creature)),
        ChatMessageType::Broadcast,
    );
}

// ACE: AdminCommands.Handlede_n
/// `de_n name, text`: sends text to named player, formatted exactly as entered.
pub fn handlede_n(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @de_n name, text
    // usage: @direct_emote_name name, text
    // Sends text to named player, formatted exactly as entered, with no prefix of any kind.
    // @direct_emote_name - Sends text to named player, formatted exactly as entered.

    handledirect_emote_name(w, session, parameters);
}

// ACE: AdminCommands.Handledirect_emote_name
/// `direct_emote_name name, text`: sends text to named player, formatted exactly as entered.
pub fn handledirect_emote_name(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @de_n name, text
    // usage: @direct_emote_name name, text
    // Sends text to named player, formatted exactly as entered, with no prefix of any kind.
    // @direct_emote_name - Sends text to named player, formatted exactly as entered.

    let s = require(session);
    let args = join(parameters);
    if args.contains(',') {
        let split: Vec<&str> = args.split(',').collect();
        let player_name = split[0];
        // Not ACE's (a fix): the text is what follows the comma, less
        // one space after it if there is one; ACE removed two characters whatever they were, so
        // "name," with nothing after it threw (logged by the command manager) and "name,text"
        // lost the text's first character.
        let after = &args[player_name.len() + 1..];
        let msg = after.strip_prefix(' ').unwrap_or(after).to_owned();

        if let Some(player) = player_manager::get_online_player_by_name(w, player_name) {
            player_send_message(w, player, &msg, ChatMessageType::Broadcast);
        } else {
            system_chat(
                w,
                s,
                &format!("Player {player_name} is not online."),
                ChatMessageType::Broadcast,
            );
        }
    } else {
        system_chat(
            w,
            s,
            "There was no player name specified.",
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: AdminCommands.Handlede_s
/// `de_s text`: sends text to selected player, formatted exactly as entered, with no prefix of
/// any kind.
pub fn handlede_s(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @de_s text
    // usage: @direct_emote_select text
    // Sends text to selected player, formatted exactly as entered, with no prefix of any kind.
    // @direct_emote_select - Sends text to selected player, formatted exactly as entered.

    handledirect_emote_select(w, session, parameters);
}

// ACE: AdminCommands.Handledirect_emote_select
/// `direct_emote_select text`: sends text to selected player, formatted exactly as entered, with
/// no prefix of any kind.
pub fn handledirect_emote_select(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @de_s text
    // usage: @direct_emote_select text
    // Sends text to selected player, formatted exactly as entered, with no prefix of any kind.
    // @direct_emote_select - Sends text to selected player, formatted exactly as entered.

    let s = require(session);
    let me = session_player(w, s);
    let object_id = selected_object_id(w, me);

    if object_id == ObjectGuid::INVALID {
        system_chat(
            w,
            s,
            "You must select a player to send them a message.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    match current_landblock_get_object(w, me, object_id) {
        None => system_chat(
            w,
            s,
            "Unable to locate what you have selected.",
            ChatMessageType::Broadcast,
        ),
        Some(player) if is_player(w, player) => {
            let msg = join(parameters);

            player_send_message(w, player, &msg, ChatMessageType::Broadcast);
        }
        Some(wo) => {
            let text = format!(
                "You cannot send text to {} because it is not a player.",
                name_of(w, wo)
            );
            system_chat(w, s, &text, ChatMessageType::Broadcast);
        }
    }
}

// ACE: AdminCommands.HandleDispel
/// `dispel`: removes all enchantments from the player (and, for now, from the items they wear).
pub fn handle_dispel(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let me = session_player(w, require(session));
    enchantment_manager::dispel_all_enchantments(w, me);

    // remove all enchantments from equipped items for now
    for item in creature_equipment::equipped_objects_values(w, me) {
        enchantment_manager::dispel_all_enchantments(w, item);
    }
}

// ACE: AdminCommands.HandleEvent
/// `event [ start | stop | disable | enable | clear | status ] (name)`: manipulates the state of
/// an event.
pub fn handle_event(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @event start| stop | disable | enable name
    // @event clear < name > -clears event with name <name> or all events if you put in 'all' (All clears registered generators, <name> does not).
    // @event status<eventSubstring> - get the status of all registered events or get all of the registered events that have <eventSubstring> in the name.
    // @event - Maniuplates the state of an event.

    // TODO: output

    let event_cmd = parameters[0].to_lowercase();

    let event_name = parameters[1].clone();

    let issuer = session_player_opt(w, session);
    let who = issuer.map_or_else(|| "CONSOLE".to_owned(), |p| name_of(w, p));
    match event_cmd.as_str() {
        "start" => {
            if event_manager_start_event(w, &event_name, issuer) {
                write_output_info(
                    w,
                    session,
                    &format!("Event {event_name} started successfully."),
                    ChatMessageType::Broadcast,
                );
                player_manager::broadcast_to_audit_channel(
                    w,
                    issuer,
                    &format!("{who} has started event {event_name}."),
                );
            } else {
                // Not ACE's (a fix): the failure is written to whoever
                // issued the command, the console included; ACE wrote it to the session only and
                // threw from the console (logged by the command manager).
                write_output_info(
                    w,
                    session,
                    &format!("Unable to start event named {event_name} ."),
                    ChatMessageType::Broadcast,
                );
            }
        }
        "stop" => {
            if event_manager_stop_event(w, &event_name, issuer) {
                write_output_info(
                    w,
                    session,
                    &format!("Event {event_name} stopped successfully."),
                    ChatMessageType::Broadcast,
                );
                player_manager::broadcast_to_audit_channel(
                    w,
                    issuer,
                    &format!("{who} has stopped event {event_name}."),
                );
            } else {
                write_output_info(
                    w,
                    session,
                    &format!("Unable to stop event named {event_name} ."),
                    ChatMessageType::Broadcast,
                );
            }
        }
        "disable" | "enable" | "clear" => {}
        "status" => {
            if event_name != "all" && !event_name.is_empty() {
                let status = event_manager_get_event_status(w, &event_name);
                write_output_info(
                    w,
                    session,
                    &format!("Event {event_name} - GameEventState.{status}"),
                    ChatMessageType::Broadcast,
                );
            }
        }
        _ => write_output_info(
            w,
            session,
            "That is not a valid event command",
            ChatMessageType::Broadcast,
        ),
    }
}

// ACE: AdminCommands.HandleFumble
/// `fumble`: forces the selected target to drop everything they contain to the ground.
#[allow(clippy::too_many_lines)]
pub fn handle_fumble(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // @fumble - Forces the selected target to drop everything they contain to the ground.

    let s = require(session);
    let me = session_player(w, s);
    let object_id = selected_object_id(w, me);

    if object_id == ObjectGuid::INVALID {
        system_chat(
            w,
            s,
            "You must select a player to force them to drop everything.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    let Some(wo) = current_landblock_get_object(w, me, object_id) else {
        system_chat(
            w,
            s,
            "Unable to locate what you have selected.",
            ChatMessageType::Broadcast,
        );
        return;
    };

    if !is_player(w, wo) {
        let text = format!(
            "You cannot force {} to drop everything because it is not a player.",
            name_of(w, wo)
        );
        system_chat(w, s, &text, ChatMessageType::Broadcast);
        return;
    }
    let player = wo;

    let mut items = Vec::new();
    let player_loc = Position::from_position(&location_of(w, player));

    // (Removing from the dictionaries while ACE enumerates them is allowed on .NET Core; the
    // enumeration visits every entry once, in order.)
    let inventory: Vec<ObjectGuid> = container::inventory(obj(w, player))
        .keys()
        .copied()
        .collect();
    for key in inventory {
        if let Some(world_object) = player_inventory::try_remove_from_inventory_with_networking(
            w,
            player,
            key,
            player_inventory::RemoveFromInventoryAction::DropItem,
        ) {
            items.push(world_object);
        }
    }

    for key in creature_equipment::equipped_objects_values(w, player) {
        if let Some(world_object) = player_inventory::try_dequip_object_with_networking(
            w,
            player,
            key,
            player_inventory::DequipObjectAction::DropItem,
        ) {
            items.push(world_object);
        }
    }

    empyrean_world::world_objects::player_database::save_player_to_database(w, player);

    for item in items {
        {
            let o = obj_mut(w, item);
            let mut location = Position::from_position(&player_loc);
            location.position_z += 0.5f32;
            o.set_location(Some(location));
            o.set_placement(Some(empyrean_entity::enums::Placement::Resting)); // This is needed to make items lay flat on the ground.
        }

        // increased precision for non-ethereal objects
        let ethereal = obj(w, item).ethereal();
        phys_ext::set_physics_property_state(
            w,
            item,
            PropertyBool::Ethereal,
            PhysicsState::Ethereal,
            Some(true),
        );

        // Not ACE's (a fix): the items go into the fumbling player's
        // landblock, where they are placed; ACE put them into the admin's, which may be an
        // adjacent one.
        let added = obj(w, player)
            .current_landblock
            .is_some_and(|lb| landblock::add_world_object(w, lb, item));
        if added {
            let mut location = obj(w, item).location().expect("set above");
            let cell = empyrean_world::entity::position_extensions::get_cell(w, &location);
            location.set_landblock_id(empyrean_entity::LandblockId::new(cell));
            obj_mut(w, item).set_location(Some(location));

            // try slide to new position
            if let Some(h) = obj(w, item).phys {
                if let Some(from) = w.physics.get(h).map(|p| p.position) {
                    let to = phys_ext::to_physics_position(&location);
                    let transit = w.physics.transition(h, &from, &to, false);

                    if let Some(transit) = transit.filter(|t| t.sphere_path.curr_cell.is_some()) {
                        phys_ext::set_position_internal(w, h, &transit);

                        world_object::sync_location(w, item);

                        world_object_networking::send_update_position(w, item, true);
                    }
                }
            }
            phys_ext::set_physics_property_state(
                w,
                item,
                PropertyBool::Ethereal,
                PhysicsState::Ethereal,
                ethereal,
            );

            if obj(w, item).ethereal().is_none() {
                let default_physics_state = PhysicsState(
                    obj(w, item)
                        .get_property(PropertyInt::PhysicsState)
                        .unwrap_or(0),
                );

                let value = default_physics_state.contains(PhysicsState::Ethereal);
                phys_ext::set_physics_property_state(
                    w,
                    item,
                    PropertyBool::Ethereal,
                    PhysicsState::Ethereal,
                    Some(value),
                );
            }

            world_object::enqueue_broadcast_physics_state(w, item);

            // drop success
            let m1 = game_message_public_update_instance_id(
                obj_mut(w, item),
                PropertyInstanceId::Container,
                ObjectGuid::INVALID,
            );
            let m2 = game_message_public_update_instance_id(
                obj_mut(w, item),
                PropertyInstanceId::Wielder,
                ObjectGuid::INVALID,
            );
            let m3 = player_manager::player_session(w, player)
                .map(|ps| game_event_item_server_says_move_item(session_data(w, ps), item));
            let m4 = game_message_update_position(w, item, false);
            for m in [Some(m1), Some(m2), m3, Some(m4)].into_iter().flatten() {
                send_to_player(w, player, m);
            }

            let sound = game_message_sound(player, Sound::DropItem, 1.0);
            world_object_networking::enqueue_broadcast(w, player, true, &[sound]);

            empyrean_world::world_objects::managers::emote_manager::on_drop(w, item, player);
            dispatch::save_biota_to_database::save_biota_to_database(w, item, true);
        } else {
            log::warn!(
                "0x{item}:{} for player {} lost from fumble failure.",
                name_of(w, item),
                name_of(w, player)
            );
        }
    }
}

// ACE: AdminCommands.HandleGod
/// `god`: turns current character into a god! Saves the player first; `DoGodMode` runs when the
/// save answers.
pub fn handle_god(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // @god - Sets your own stats to a godly level.
    // need to save stats so that we can return with /ungod
    let s = require(session);
    let player = session_player(w, s);
    let biota = obj(w, player).biota.clone();
    w.shard.save_biota(
        biota,
        Some(Box::new(move |w: &mut World, result: bool| {
            do_god_mode(w, result, s, false)
        })),
    );
}

/// The number of `=`-separated entries a well-formed `GodState` string has.
const GOD_STATE_ENTRIES: usize = 240;

// ACE: AdminCommands.DoGodMode
/// Saves the return state in `GodState` (unless re-entering after a failed `ungod`), then raises
/// level, experience, every skill, attribute and vital far above the maximum.
#[allow(clippy::too_many_lines)]
fn do_god_mode(w: &mut World, player_saved: bool, session: SessionId, exception_return: bool) {
    let Some(current_player) = w.sessions.player(session) else {
        panic!("NullReferenceException: session.Player");
    };
    if !player_saved {
        send_server_message(
            w,
            Some(session),
            "Error saving player. Godmode not available.",
            ChatMessageType::Broadcast,
        );
        console_write_line(&format!("Player {} tried to enter god mode but there was an error saving player. Godmode not available.", name_of(w, current_player)));
        return;
    }

    let god_string = obj(w, current_player).god_state();

    if !exception_return {
        // if godstate starts with 1, you are in godmode

        if god_string.as_deref().is_some_and(|g| g.starts_with('1')) {
            send_server_message(
                w,
                Some(session),
                "You are already a god.",
                ChatMessageType::Broadcast,
            );
            return;
        }

        let return_state = god_return_state(w, current_player);

        // Check string is correctly formatted before altering stats
        // correctly formatted return string should have 240 entries
        // if the construction of the string changes - this will need to be updated to match
        let entries = return_state.split('=').count();
        if entries != GOD_STATE_ENTRIES {
            send_server_message(
                w,
                Some(session),
                "Godmode is not available at this time.",
                ChatMessageType::Broadcast,
            );
            console_write_line(&format!(
                "Player {} tried to enter god mode but there was an error with the godString length. (length = {entries}) Godmode not available.",
                name_of(w, current_player)
            ));
            return;
        }

        // save return state to db in property string
        obj_mut(w, current_player).set_property(PropertyString::GodState, return_state);
        dispatch::save_biota_to_database::save_biota_to_database(w, current_player, true);
    }

    // Begin Godly Stats Increase

    {
        let o = obj_mut(w, current_player);
        o.set_level(Some(999));
        o.set_property(PropertyInt64::AvailableExperience, 0);
        o.set_property(PropertyInt::AvailableSkillCredits, 0);
        o.set_property(PropertyInt64::TotalExperience, 191_226_310_247);
    }

    let o = obj_mut(w, current_player);
    let level = o.get_property(PropertyInt::Level).unwrap_or(0);
    let level_msg = game_message_private_update_property_int(o, PropertyInt::Level, level);
    let exp_msg =
        game_message_private_update_property_int64(o, PropertyInt64::AvailableExperience, 0);
    let sk_msg = game_message_private_update_property_int(o, PropertyInt::AvailableSkillCredits, 0);
    let total_exp_msg = game_message_private_update_property_int64(
        o,
        PropertyInt64::TotalExperience,
        191_226_310_247,
    );

    for m in [level_msg, exp_msg, sk_msg, total_exp_msg] {
        enqueue_send(w, session, m);
    }

    let skills: Vec<Skill> = obj(w, current_player).skills().keys().copied().collect();
    for s in skills {
        player_skills::train_skill_with(w, current_player, s, 0, false);
        player_skills::specialize_skill_with(w, current_player, s, 0, true);
        let o = obj_mut(w, current_player);
        let player_skill = *o.skills().get(&s).expect("KeyNotFoundException: Skills[s]");
        player_skill.set_ranks(o, 226);
        player_skill.set_experience_spent(o, 4_100_490_438);
        player_skill.set_init_level(o, 5000);
        let m = game_message_private_update_skill(o, player_skill);
        enqueue_send(w, session, m);
    }

    let attributes: Vec<PropertyAttribute> = obj(w, current_player)
        .attributes()
        .keys()
        .copied()
        .collect();
    for a in attributes {
        let o = obj_mut(w, current_player);
        let player_attr = *o
            .attributes()
            .get(&a)
            .expect("KeyNotFoundException: Attributes[a]");
        player_attr.set_starting_value(o, 9809);
        player_attr.set_ranks(o, 190);
        player_attr.set_experience_spent(o, 4_019_438_644);
        let m = game_message_private_update_attribute(o, player_attr);
        enqueue_send(w, session, m);
    }

    dispatch::set_max_vitals::set_max_vitals(w, current_player);

    let vitals: Vec<PropertyAttribute2nd> =
        obj(w, current_player).vitals().keys().copied().collect();
    for v in vitals {
        let o = obj_mut(w, current_player);
        let player_vital = *o.vitals().get(&v).expect("KeyNotFoundException: Vitals[v]");
        player_vital.set_ranks(o, 196);
        player_vital.set_experience_spent(o, 4_285_430_197);
        // my OCD will not let health/stam not be equal due to the endurance calc
        player_vital.set_starting_value(
            o,
            if v == PropertyAttribute2nd::MaxHealth {
                94803
            } else {
                89804
            },
        );
        let m = game_message_private_update_vital(o, player_vital);
        enqueue_send(w, session, m);
    }

    world_object::play_particle_effect(w, current_player, PlayScript::LevelUp, current_player, 1.0);
    world_object::play_particle_effect(
        w,
        current_player,
        PlayScript::BaelZharonSmite,
        current_player,
        1.0,
    );

    dispatch::set_max_vitals::set_max_vitals(w, current_player);

    send_server_message(
        w,
        Some(session),
        "You are now a god!!!",
        ChatMessageType::Broadcast,
    );
}

/// `DoGodMode`'s `returnState`: the date, credits, level, experience, then every attribute,
/// max vital and valid skill record of the biota, in its dictionary order.
fn god_return_state(w: &World, player: ObjectGuid) -> String {
    let o = obj(w, player);
    let biota = &o.biota;
    let opt = |v: Option<i64>| v.map(|v| v.to_string()).unwrap_or_default();

    let mut return_state = "1=".to_owned();
    return_state += &format!("{}=", to_common_string(w.now.utc));

    // need level 25, available skill credits 24
    return_state += &format!(
        "24={}=25={}=",
        opt(o
            .get_property(PropertyInt::AvailableSkillCredits)
            .map(i64::from)),
        opt(o.get_property(PropertyInt::Level).map(i64::from))
    );

    // need total xp 1, unassigned xp 2
    return_state += &format!(
        "1={}=2={}=",
        opt(o.get_property(PropertyInt64::TotalExperience)),
        opt(o.get_property(PropertyInt64::AvailableExperience))
    );

    // need all attributes
    // 1 through 6 str, end, coord, quick, focus, self
    for (key, att) in biota
        .properties_attribute
        .as_ref()
        .expect("NullReferenceException: biota.PropertiesAttribute")
        .iter()
    {
        if key.0 > 0 && key.0 <= 6 {
            return_state += &format!("{}=", key.0);
            return_state += &format!("{}=", att.init_level);
            return_state += &format!("{}=", att.level_from_cp);
            return_state += &format!("{}=", att.cp_spent);
        }
    }

    // need all vitals
    // 1, 3, 5 H,S,M (2,4,6 are current values and are not stored since they will be maxed entering/exiting godmode)
    for (key, att_sec) in biota
        .properties_attribute_2nd
        .as_ref()
        .expect("NullReferenceException: biota.PropertiesAttribute2nd")
        .iter()
    {
        if key.0 == 1 || key.0 == 3 || key.0 == 5 {
            return_state += &format!("{}=", key.0);
            return_state += &format!("{}=", att_sec.init_level);
            return_state += &format!("{}=", att_sec.level_from_cp);
            return_state += &format!("{}=", att_sec.cp_spent);
            return_state += &format!("{}=", att_sec.current_level);
        }
    }

    // need all skills
    for (key, sk) in biota
        .properties_skill
        .as_ref()
        .expect("NullReferenceException: biota.PropertiesSkill")
        .iter()
    {
        if skill_helper::VALID_SKILLS.contains(key) {
            return_state += &format!("{}=", key.0);
            return_state += &format!("{}=", sk.level_from_pp);
            return_state += &format!("{}=", sk.sac.0);
            return_state += &format!("{}=", sk.pp);
            return_state += &format!("{}=", sk.init_level);
        }
    }
    return_state
}

// ACE: AdminCommands.HandleUngod
/// `ungod`: returns character to a mortal state.
pub fn handle_ungod(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // @ungod - Returns skills and attributues to pre-god levels.
    let s = require(session);
    let current_player = session_player(w, s);
    let Some(return_string) = obj(w, current_player).god_state() else {
        send_server_message(
            w,
            session,
            "Can't get any more ungodly than you already are...",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let return_string_arr: Vec<&str> = return_string.split('=').collect();

    // correctly formatted return string should have 240 entries
    // if the construction of the string changes - this will need to be updated to match
    if return_string_arr.len() != GOD_STATE_ENTRIES {
        console_write_line(&format!(
            "The returnString was not set to the correct length while {} was attempting to return to normal from godmode.",
            name_of(w, current_player)
        ));
        send_server_message(
            w,
            session,
            "Error returning to mortal state, defaulting to godmode.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    if let Err(e) = ungod_restore(w, s, current_player, &return_string_arr) {
        console_write_line(&format!(
            "Exception ( {} - {} ) caught while {} was attempting to return to normal from godmode.",
            e.source,
            e.message,
            name_of(w, current_player)
        ));
        send_server_message(
            w,
            session,
            "Error returning to mortal state, defaulting to godmode.",
            ChatMessageType::Broadcast,
        );
        do_god_mode(w, true, s, true);
        return;
    }

    dispatch::set_max_vitals::set_max_vitals(w, current_player);

    obj_mut(w, current_player).remove_property(PropertyString::GodState);

    dispatch::save_biota_to_database::save_biota_to_database(w, current_player, true);

    world_object::play_particle_effect(
        w,
        current_player,
        PlayScript::DispelAll,
        current_player,
        1.0,
    );

    send_server_message(
        w,
        session,
        "You have returned from your godly state.",
        ChatMessageType::Broadcast,
    );
}

/// An exception `HandleUngod`'s `try` catches: its `Source` and `Message`.
#[derive(Debug, Clone)]
pub struct CaughtException {
    pub source: &'static str,
    pub message: String,
}

impl CaughtException {
    fn format() -> Self {
        Self {
            source: "System.Private.CoreLib",
            message: "The input string was not in a correct format.".to_owned(),
        }
    }

    fn overflow(type_name: &str) -> Self {
        Self {
            source: "System.Private.CoreLib",
            message: format!("Value was either too large or too small for {type_name}."),
        }
    }

    fn key_not_found(key: &str) -> Self {
        Self {
            source: "System.Private.CoreLib",
            message: format!("The given key '{key}' was not present in the dictionary."),
        }
    }
}

/// `uint.Parse(s)` / `int.Parse(s)` / `long.Parse(s)` / `ushort.Parse(s)`: the value, or the
/// exception .NET throws (`FormatException`, or `OverflowException` for a number out of range).
fn dotnet_parse_integer(
    s: &str,
    min: i128,
    max: i128,
    type_name: &str,
) -> Result<i128, CaughtException> {
    match dotnet_parse::long_try_parse(s)
        .map(i128::from)
        .or_else(|| dotnet_parse::ulong_try_parse(s).map(i128::from))
    {
        Some(v) if v >= min && v <= max => Ok(v),
        Some(_) => Err(CaughtException::overflow(type_name)),
        None => {
            // a well-formed number too large even for a long is an overflow
            let t = s.trim();
            let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                Err(CaughtException::overflow(type_name))
            } else {
                Err(CaughtException::format())
            }
        }
    }
}

fn parse_uint(s: &str) -> Result<u32, CaughtException> {
    dotnet_parse_integer(s, 0, i128::from(u32::MAX), "a UInt32")
        .map(|v| u32::try_from(v).unwrap_or_default())
}

fn parse_int(s: &str) -> Result<i32, CaughtException> {
    dotnet_parse_integer(s, i128::from(i32::MIN), i128::from(i32::MAX), "an Int32")
        .map(|v| i32::try_from(v).unwrap_or_default())
}

fn parse_long(s: &str) -> Result<i64, CaughtException> {
    dotnet_parse_integer(s, i128::from(i64::MIN), i128::from(i64::MAX), "an Int64")
        .map(|v| i64::try_from(v).unwrap_or_default())
}

fn parse_ushort(s: &str) -> Result<u16, CaughtException> {
    dotnet_parse_integer(s, 0, i128::from(u16::MAX), "a UInt16")
        .map(|v| u16::try_from(v).unwrap_or_default())
}

/// The `try` body of `HandleUngod`: walks the return string and restores each value, sending
/// the updates as it goes (what ran before an exception stays applied, as in ACE).
fn ungod_restore(
    w: &mut World,
    session: SessionId,
    current_player: ObjectGuid,
    return_string_arr: &[&str],
) -> Result<(), CaughtException> {
    // DIVERGE: the exception's source names Empyrean where ACE's names `ACE.Server` (brand).
    let arr = |i: usize| -> Result<&str, CaughtException> {
        return_string_arr.get(i).copied().ok_or(CaughtException {
            source: empyrean_common::brand::PRODUCT,
            message: "Index was outside the bounds of the array.".to_owned(),
        })
    };
    let mut i = 2usize;
    while i < return_string_arr.len() {
        match i {
            n if n <= 5 => {
                let property = PropertyInt(parse_uint(arr(i)?)?.cs_cast());
                let value = parse_int(arr(i + 1)?)?;
                obj_mut(w, current_player).set_property(property, value);
                i += 2;
            }
            n if n <= 9 => {
                let property = PropertyInt64(parse_uint(arr(i)?)?.cs_cast());
                let value = parse_long(arr(i + 1)?)?;
                obj_mut(w, current_player).set_property(property, value);
                i += 2;
            }
            n if n <= 33 => {
                let key = PropertyAttribute(parse_uint(arr(i)?)?.cs_cast());
                let o = obj_mut(w, current_player);
                let player_attr = *o
                    .attributes()
                    .get(&key)
                    .ok_or_else(|| CaughtException::key_not_found(&key.to_dotnet_string()))?;
                player_attr.set_starting_value(o, parse_uint(arr(i + 1)?)?);
                player_attr.set_ranks(o, parse_uint(arr(i + 2)?)?);
                player_attr.set_experience_spent(o, parse_uint(arr(i + 3)?)?);
                let m = game_message_private_update_attribute(o, player_attr);
                enqueue_send(w, session, m);
                i += 4;
            }
            n if n <= 48 => {
                let key = PropertyAttribute2nd(parse_int(arr(i)?)?.cs_cast());
                let o = obj_mut(w, current_player);
                let player_vital = *o
                    .vitals()
                    .get(&key)
                    .ok_or_else(|| CaughtException::key_not_found(&key.to_dotnet_string()))?;
                player_vital.set_starting_value(o, parse_uint(arr(i + 1)?)?);
                player_vital.set_ranks(o, parse_uint(arr(i + 2)?)?);
                player_vital.set_experience_spent(o, parse_uint(arr(i + 3)?)?);
                player_vital.set_current(o, parse_uint(arr(i + 4)?)?);
                let m = game_message_private_update_vital(o, player_vital);
                enqueue_send(w, session, m);
                i += 5;
            }
            n if n <= 238 => {
                let key = Skill(parse_int(arr(i)?)?);
                let ranks = parse_ushort(arr(i + 1)?)?;

                // Handle god users stuck in god mode due to bad godstate with Enum string
                let advancement = match try_parse_skill_advancement_class(arr(i + 2)?, false) {
                    Some(advancement) => advancement,
                    None => SkillAdvancementClass(parse_uint(arr(i + 2)?)?),
                };

                let experience_spent = parse_uint(arr(i + 3)?)?;
                let init_level = parse_uint(arr(i + 4)?)?;
                let o = obj_mut(w, current_player);
                let player_skill = *o
                    .skills()
                    .get(&key)
                    .ok_or_else(|| CaughtException::key_not_found(&key.to_dotnet_string()))?;
                player_skill.set_ranks(o, ranks);
                player_skill.set_advancement_class(o, advancement);
                player_skill.set_experience_spent(o, experience_spent);
                player_skill.set_init_level(o, init_level);
                let m = game_message_private_update_skill(o, player_skill);
                enqueue_send(w, session, m);
                i += 5;
            }
            239 => {
                // end of returnString, this will need to be updated if the length of the string changes
                let o = obj_mut(w, current_player);
                let not_null = || CaughtException {
                    source: "System.Private.CoreLib",
                    message: "Nullable object must have a value.".to_owned(),
                };
                let level = o.get_property(PropertyInt::Level).ok_or_else(not_null)?;
                let credits = o
                    .get_property(PropertyInt::AvailableSkillCredits)
                    .ok_or_else(not_null)?;
                let total = o
                    .get_property(PropertyInt64::TotalExperience)
                    .ok_or_else(not_null)?;
                let available = o
                    .get_property(PropertyInt64::AvailableExperience)
                    .ok_or_else(not_null)?;
                let level_msg =
                    game_message_private_update_property_int(o, PropertyInt::Level, level);
                let sk_msg = game_message_private_update_property_int(
                    o,
                    PropertyInt::AvailableSkillCredits,
                    credits,
                );
                let total_exp_msg = game_message_private_update_property_int64(
                    o,
                    PropertyInt64::TotalExperience,
                    total,
                );
                let unassigned_exp_msg = game_message_private_update_property_int64(
                    o,
                    PropertyInt64::AvailableExperience,
                    available,
                );
                for m in [level_msg, sk_msg, total_exp_msg, unassigned_exp_msg] {
                    enqueue_send(w, session, m);
                }
                i += 1;
            }
            _ => {
                // A warning that will alert on the console if the returnString length changes. This should suffice until a smoother way can be found.
                console_write_line(&format!("Hit default case in /ungod command with i = {i}, did you change the length of the PropertyString.GodState array?"));
                i += 1;
            }
        }
    }
    Ok(())
}

// ACE: AdminCommands.HandleMagicGod
/// `magic god`: ACE only answers "You are now a magic god!!!" ("TODO: output").
pub fn handle_magic_god(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // @magic god - Sets your magic stats to the specfied level.

    // TODO: output

    // output: You are now a magic god!!!

    send_server_message(
        w,
        session,
        "You are now a magic god!!!",
        ChatMessageType::Broadcast,
    );
}

/// `CommandHandlerHelper.GetLastAppraisedObject(session)` as a Creature, or ACE's message.
fn last_appraised_creature(w: &mut World, session: Option<SessionId>) -> Option<ObjectGuid> {
    let last_appraised = command_handler_helper::get_last_appraised_object(w, require(session));
    let creature = last_appraised.filter(|&g| is_creature(w, g));
    if creature.is_none() {
        send_server_message(
            w,
            session,
            "The last appraised object was not a mob/NPC/player.",
            ChatMessageType::Broadcast,
        );
    }
    creature
}

/// `creature is Player || creature.IsDynamicThatShouldPersistToShard()` then
/// `creature.SaveBiotaToDatabase()`.
fn save_if_persistent(w: &mut World, creature: ObjectGuid) {
    if is_player(w, creature) || empyrean_world::world_objects::world_object_database::is_dynamic_that_should_persist_to_shard(w, creature) {
        dispatch::save_biota_to_database::save_biota_to_database(w, creature, true);
    }
}

// ACE: AdminCommands.HandleModifyVital
/// `modifyvital <Health|Stamina|Mana> <delta>`: adjusts the maximum vital attribute for the last
/// appraised mob/player and restores full vitals.
pub fn handle_modify_vital(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(creature) = last_appraised_creature(w, session) else {
        return;
    };

    if parameters.len() < 2 {
        send_server_message(
            w,
            session,
            "Usage: modifyvital <Invalid vital type, valid values are: Health,Stamina,Mana",
            ChatMessageType::Broadcast,
        );
        return;
    }

    // determine the vital type
    let Some(vital_attr) = try_parse_property_attribute_2nd(&parameters[0], false) else {
        send_server_message(
            w,
            session,
            "Invalid vital type, valid values are: Health,Stamina,Mana",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let Some(delta) = dotnet_parse::int_try_parse(&parameters[1]) else {
        send_server_message(
            w,
            session,
            "Invalid vital value, values must be valid integers",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let max_attr = match vital_attr {
        PropertyAttribute2nd::Health => PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::Stamina => PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::Mana => PropertyAttribute2nd::MaxMana,
        _ => {
            send_server_message(
                w,
                session,
                "Unexpected vital type, valid values are: Health,Stamina,Mana",
                ChatMessageType::Broadcast,
            );
            return;
        }
    };

    // `(uint)Math.Clamp(ranks + delta, 1, uint.MaxValue)` (long arithmetic)
    let clamp_ranks = |ranks: u32| -> u32 {
        (i64::from(ranks) + i64::from(delta))
            .clamp(1, i64::from(u32::MAX))
            .cs_cast()
    };

    let max_vital = CreatureVital::new(obj_mut(w, creature), max_attr);
    let ranks = max_vital.ranks(obj(w, creature));
    max_vital.set_ranks(obj_mut(w, creature), clamp_ranks(ranks));
    let max_value = max_vital.max_value(&mut StatCtx::in_world(w, creature));
    dispatch::update_vital::update_vital_uint(w, creature, max_vital, max_value);

    // Not ACE's (a fix): the vital's own record is the one just raised
    // and filled; ACE also built a vital on the bare Health/Stamina/Mana id, which added a stray
    // record to the biota (raised, filled and saved with the creature, and read by nothing).

    if is_player(w, creature) {
        let m = game_message_private_update_vital(obj_mut(w, creature), max_vital);
        send_to_player(w, creature, m);
    }

    dispatch::set_max_vitals::set_max_vitals(w, creature);

    // save changes
    save_if_persistent(w, creature);
}

// ACE: AdminCommands.HandleModifySkill
/// `modifyskill <skillName> <delta>`: adjusts the skill for the last appraised mob/player.
pub fn handle_modify_skill(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(creature) = last_appraised_creature(w, session) else {
        return;
    };

    if parameters.len() < 2 {
        send_server_message(
            w,
            session,
            "Usage: modifyskill <skillName> <delta>: missing skillId and/or delta",
            ChatMessageType::Broadcast,
        );
        return;
    }
    let Some(skill) = try_parse_skill(&parameters[0], false) else {
        let names = Skill::NAMES.join(", ");
        send_server_message(
            w,
            session,
            &format!("Invalid skillName, must be a valid skill name (without spaces, with capitalization), valid values are: {names}"),
            ChatMessageType::Broadcast,
        );
        return;
    };
    let Some(delta) = dotnet_parse::int_try_parse(&parameters[1]) else {
        send_server_message(
            w,
            session,
            "Invalid delta, must be a valid integer",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let o = obj_mut(w, creature);
    let creature_skill = if o.player.is_some() {
        *o.skills()
            .get(&skill)
            .unwrap_or_else(|| panic!("KeyNotFoundException: Skills[{}]", skill.to_dotnet_string()))
    } else {
        o.get_creature_skill(skill, true)
            .expect("GetCreatureSkill(skill) adds the skill")
    };
    let init_level = creature_skill.init_level(o);
    // `(ushort)Math.Clamp(InitLevel + delta, 0, (int)ushort.MaxValue)` (long arithmetic)
    let clamped: u16 = (i64::from(init_level) + i64::from(delta))
        .clamp(0, i64::from(u16::MAX))
        .cs_cast();
    creature_skill.set_init_level(o, u32::from(clamped));

    // save changes
    save_if_persistent(w, creature);
    if is_player(w, creature) {
        let m = game_message_private_update_skill(obj_mut(w, creature), creature_skill);
        send_to_player(w, creature, m);
    }
}

// ACE: AdminCommands.HandleModifyAttribute
/// `modifyattr <attribute> <delta>`: adjusts an attribute for the last appraised
/// mob/NPC/player.
pub fn handle_modify_attribute(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(creature) = last_appraised_creature(w, session) else {
        return;
    };

    if parameters.len() < 2 {
        send_server_message(
            w,
            session,
            "Usage: modifyattr <attribute> <delta>: missing attribute name and/or delta",
            ChatMessageType::Broadcast,
        );
        return;
    }
    let Some(attr_type) = try_parse_property_attribute(&parameters[0], false) else {
        send_server_message(
            w,
            session,
            "Invalid skillName, must be a valid skill name (without spaces, with capitalization), valid values are: Strength,Endurance,Coordination,Quickness,Focus,Self",
            ChatMessageType::Broadcast,
        );
        return;
    };
    let Some(delta) = dotnet_parse::int_try_parse(&parameters[1]) else {
        send_server_message(
            w,
            session,
            "Invalid delta, must be a valid integer",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let o = obj_mut(w, creature);
    let attr = *o.attributes().get(&attr_type).unwrap_or_else(|| {
        panic!(
            "KeyNotFoundException: Attributes[{}]",
            attr_type.to_dotnet_string()
        )
    });
    let starting_value = attr.starting_value(o);
    // `(uint)Math.Clamp(StartingValue + delta, 1, 9999)` (long arithmetic)
    attr.set_starting_value(
        o,
        (i64::from(starting_value) + i64::from(delta))
            .clamp(1, 9999)
            .cs_cast(),
    );

    save_if_persistent(w, creature);
    if is_player(w, creature) {
        let m = game_message_private_update_attribute(obj_mut(w, creature), attr);
        send_to_player(w, creature, m);
    }
}

// ACE: AdminCommands.HandleHeal
/// `heal`: heals yourself (or the selected creature). This command fully restores your (or the
/// selected creature's) health, mana, and stamina.
pub fn handle_heal(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // usage: @heal
    // This command fully restores your(or the selected creature's) health, mana, and stamina.
    // @heal - Heals yourself(or the selected creature).

    let s = require(session);
    let me = session_player(w, s);
    let mut object_id = selected_object_id(w, me);

    if object_id == ObjectGuid::INVALID {
        object_id = me;
    }

    match current_landblock_get_object(w, me, object_id) {
        None => system_chat(
            w,
            s,
            "Unable to locate what you have selected.",
            ChatMessageType::Broadcast,
        ),
        Some(player) if is_player(w, player) => dispatch::set_max_vitals::set_max_vitals(w, player),
        Some(wo) => {
            let text = format!(
                "You cannot heal {} because it is not a player.",
                name_of(w, wo)
            );
            system_chat(w, s, &text, ChatMessageType::Broadcast);
        }
    }
}

// ACE: AdminCommands.HandleHousekeep
/// `housekeep`: ACE's body is empty ("TODO: output").
pub fn handle_housekeep(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @housekeep[never { off | on}] -With no parameters, this command displays the housekeeping info for the selected item.With the 'never' flag, it sets the item to never housekeep, or turns that state off.
    // @housekeep - Queries or sets the housekeeping status for the selected item.

    // TODO: output
}

// ACE: AdminCommands.HandleIDlist
/// `idlist`: shows the next ID that will be allocated from GuidManager.
pub fn handle_i_dlist(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // @idlist - Shows the next ID that will be allocated from SQL.

    let s = require(session);
    let text = guid_manager::get_id_list_command_output(w);
    system_chat(w, s, &text, ChatMessageType::WorldBroadcast);
}

// ACE: AdminCommands.HandleGameCastLocalEmote
/// `gamecastlocalemote <message>`: sends text to all players within chat range, formatted exactly
/// as entered.
pub fn handle_game_cast_local_emote(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    // usage: @gamecastlocalemote<message>
    // Sends text to all players within chat range, formatted exactly as entered, with no prefix of any kind.
    // See Also: @gamecast, @gamecastemote, @gamecastlocal, @gamecastlocalemote.
    // @gamecastlocalemote - Sends text to all players within chat range, formatted exactly as entered.

    // Since we only have one server, this command will just call the other one
    handle_game_cast_emote(w, session, parameters);
}

// ACE: AdminCommands.HandleLocation
/// `location`: ACE's body is empty ("TODO: output").
pub fn handle_location(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @location - Causes your current location to be continuously displayed on the screen.

    // TODO: output
}

// ACE: AdminCommands.HandleMorph
/// `morph <wcid or weenie class name> [character name]`: morphs your bodily form into that of the
/// specified creature (a new character on your account; you are logged out).
pub fn handle_morph(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // @morph - Morphs your bodily form into that of the specified creature. Be careful with this one!

    let s = require(session);
    let weenie_desc = parameters[0].clone();

    let weenie = match dotnet_parse::uint_try_parse(&weenie_desc) {
        Some(wcid) => w.content.get_cached_weenie(wcid),
        None => w.content.get_cached_weenie_by_class_name(&weenie_desc),
    };

    let Some(weenie) = weenie else {
        system_chat(
            w,
            s,
            &format!("Weenie {weenie_desc} not found in database, unable to morph."),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let weenie_name = weenie
        .get_property(PropertyString::Name)
        .unwrap_or_default();
    let wt = weenie.weenie_type;
    if wt != WeenieType::Creature
        && wt != WeenieType::Cow
        && wt != WeenieType::Admin
        && wt != WeenieType::Sentinel
        && wt != WeenieType::Vendor
        && wt != WeenieType::Pet
        && wt != WeenieType::CombatPet
    {
        let text = format!(
            "Weenie {weenie_name} ({weenie_desc}) is of WeenieType.{} ({}), unable to morph because that is not allowed.",
            wt.name().unwrap_or_default(),
            wt.to_dotnet_string()
        );
        system_chat(w, s, &text, ChatMessageType::Broadcast);
        return;
    }

    system_chat(
        w,
        s,
        &format!("Morphing you into {weenie_name} ({weenie_desc})... You will be logged out."),
        ChatMessageType::Broadcast,
    );

    let guid = guid_manager::new_player_guid(w);

    let account_id = w.sessions.get(s).map_or(0, |d| d.account_id);
    let mut player = CtorEnv::with_world(w, |env| {
        empyrean_world::world_objects::player::player_from_weenie(
            env,
            dispatch::Class::Player,
            weenie.clone(),
            guid,
            account_id,
        )
    });

    let me = session_player(w, s);
    player.biota.weenie_type = obj(w, me).biota.weenie_type;

    let mut name = parameters[1..].join(" ");
    if parameters.len() > 1 {
        name = name
            .trim_start_matches('+')
            .trim_start_matches(' ')
            .trim_end_matches(' ')
            .to_owned();
        player.set_property(PropertyString::Name, name.clone());
        morph_character_mut(&mut player).name.clone_from(&name);
    } else {
        name = weenie_name;
    }

    let n = name.clone();
    w.shard.is_character_name_available(
        n,
        Some(Box::new(move |w: &mut World, is_available: bool| {
            morph_name_checked(w, s, player, &weenie, &name, is_available)
        })),
    );
}

/// `player.Character` of a player under construction.
fn morph_character_mut(player: &mut WorldObject) -> &mut shard::Character {
    player
        .player
        .as_mut()
        .and_then(|p| p.player.character.as_mut())
        .expect("NullReferenceException: player.Character")
}

/// `HandleMorph`'s `IsCharacterNameAvailable` callback.
///
/// DIVERGE (arch): ACE builds the character's gear on the new `Player` object without putting it
/// in the world; here `TryEquipObject` and `TryAddToInventory` act on `World.objects`, so the new
/// player and its items are in the store (and in no landblock) while they are built, and leave it
/// once their biotas are snapshotted for the save.
#[allow(clippy::needless_pass_by_value)]
fn morph_name_checked(
    w: &mut World,
    session: SessionId,
    mut player: WorldObject,
    weenie: &std::sync::Arc<empyrean_entity::Weenie>,
    name: &str,
    is_available: bool,
) {
    if !is_available {
        write_output_info(
            w,
            Some(session),
            &format!("{name} is not available to use for the morphed character, try another name."),
            ChatMessageType::Broadcast,
        );
        return;
    }

    let me = session_player(w, session);
    player.set_location(obj(w, me).location());

    let (options_1, options_2) = obj(w, me)
        .player
        .as_ref()
        .and_then(|p| p.player.character.as_ref())
        .map(|c| (c.character_options_1, c.character_options_2))
        .expect("NullReferenceException: session.Player.Character");
    {
        let character = morph_character_mut(&mut player);
        character.character_options_1 = options_1;
        character.character_options_2 = options_2;
    }

    let player_guid = player.guid;
    assert!(w.objects.insert(player).is_ok(), "fresh player guid");

    if let Some(create_list) = weenie.properties_create_list.clone() {
        let wearables: Vec<_> = create_list
            .iter()
            .filter(|x| {
                x.destination_type == DestinationType::Wield
                    || x.destination_type == DestinationType::WieldTreasure
            })
            .cloned()
            .collect();
        for wearable in wearables {
            let Some(world_object) = morph_create_item(w, &wearable) else {
                continue;
            };
            let valid_locations = obj(w, world_object).valid_locations().unwrap_or_default();
            creature_equipment::try_equip_object(w, player_guid, world_object, valid_locations);
        }

        let containables: Vec<_> = create_list
            .iter()
            .filter(|x| {
                x.destination_type == DestinationType::Contain
                    || x.destination_type == DestinationType::Shop
                    || x.destination_type == DestinationType::Treasure
                    || x.destination_type == DestinationType::ContainTreasure
                    || x.destination_type == DestinationType::ShopTreasure
            })
            .cloned()
            .collect();
        for containable in containables {
            let Some(world_object) = morph_create_item(w, &containable) else {
                continue;
            };
            container::try_add_to_inventory(w, player_guid, world_object, 0, false, true);
        }
    }

    player_generate_new_face(w, player_guid);

    let possessions = player_inventory::get_all_possessions(w, player_guid);
    let possessed_biotas: Vec<empyrean_entity::Biota> = possessions
        .iter()
        .map(|&p| obj(w, p).biota.clone())
        .collect();
    for p in possessions {
        w.objects.remove(p);
    }
    let player = w.objects.remove(player_guid).expect("inserted above");

    let character = player
        .player
        .as_ref()
        .and_then(|p| p.player.character.clone())
        .expect("NullReferenceException: player.Character");
    let player_name = player
        .get_property(PropertyString::Name)
        .unwrap_or_default();
    let account_name = player
        .player
        .as_ref()
        .and_then(|p| p.player.account.as_ref())
        .map(|a| a.account_name.clone());
    let biota = player.biota.clone();
    let class_name = weenie.class_name.clone().unwrap_or_default();

    // We must await here --
    w.shard.add_character_in_parallel(
        biota.clone(),
        possessed_biotas,
        character.clone(),
        Some(Box::new(move |w: &mut World, save_success: bool| {
            let account_name = account_name.expect("NullReferenceException: player.Account");
            if !save_success {
                let text = format!("Failed to create a morph based on {class_name} to a new character \"{player_name}\" for the account \"{account_name}\"!");
                write_output_info(w, Some(session), &text, ChatMessageType::Broadcast);
                return;
            }

            player_manager::add_offline_player_biota(w, biota);

            if let Some(s) = w.sessions.get_mut(session) {
                s.characters.push(character);
            }

            let msg = format!("Successfully created a morph based on {class_name} to a new character \"{player_name}\" for the account \"{account_name}\".");
            write_output_info(w, Some(session), &msg, ChatMessageType::Broadcast);
            let me = w.sessions.player(session);
            player_manager::broadcast_to_audit_channel(w, me, &msg);

            empyrean_world::sessions::log_off_player(w, session, false);
        })),
    );
}

/// One create-list item of `HandleMorph`: its weenie's object with the entry's palette and shade,
/// in `World.objects`; `None` where ACE skips it.
fn morph_create_item(
    w: &mut World,
    entry: &empyrean_entity::models::properties_create_list::PropertiesCreateList,
) -> Option<ObjectGuid> {
    let weenie_of_wearable = w.content.get_cached_weenie(entry.weenie_class_id)?;

    let guid = guid_manager::new_dynamic_guid(w);
    let Some(mut world_object) = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(
            env,
            Some(weenie_of_wearable),
            guid,
        )
    }) else {
        guid_manager::recycle_dynamic_guid(w, guid);
        return None;
    };

    if entry.palette > 0 {
        world_object.set_palette_template(Some(i32::from(entry.palette)));
    }
    if entry.shade > 0.0 {
        world_object.set_shade(Some(f64::from(entry.shade)));
    }

    let g = world_object.guid;
    assert!(w.objects.insert(world_object).is_ok(), "fresh dynamic guid");
    empyrean_world::world_objects::creature::post_insert(w, g);
    dispatch::calculate_obj_desc::calculate_obj_desc(w, g);
    Some(g)
}

/// `Regex.IsMatch(text, filter.WildCardToRegular(), RegexOptions.IgnoreCase)`: the filter's
/// literal text with `*` matching any run of characters other than a newline, anchored at the
/// start and at the end (or before a final newline), letters compared without case.
#[must_use]
pub fn wildcard_is_match(text: &str, filter: &str) -> bool {
    let parts: Vec<Vec<char>> = filter.split('*').map(|p| p.chars().collect()).collect();
    let text: Vec<char> = text.chars().collect();
    let eq = |a: char, b: char| a == b || a.to_lowercase().eq(b.to_lowercase());

    fn at(
        text: &[char],
        pos: usize,
        parts: &[Vec<char>],
        first: bool,
        eq: &dyn Fn(char, char) -> bool,
    ) -> bool {
        let Some((part, rest)) = parts.split_first() else {
            // `$`: the end, or a final "\n"
            return pos == text.len() || (pos + 1 == text.len() && text[pos] == '\n');
        };
        if first {
            if text.len() < pos + part.len()
                || !text[pos..pos + part.len()]
                    .iter()
                    .zip(part)
                    .all(|(&a, &b)| eq(a, b))
            {
                return false;
            }
            return at(text, pos + part.len(), rest, false, eq);
        }
        // `.*` then the part: try every start, but `.` never crosses a newline
        let mut start = pos;
        loop {
            if text.len() >= start + part.len()
                && text[start..start + part.len()]
                    .iter()
                    .zip(part)
                    .all(|(&a, &b)| eq(a, b))
                && at(text, start + part.len(), rest, false, eq)
            {
                return true;
            }
            if start >= text.len() || text[start] == '\n' {
                return false;
            }
            start += 1;
        }
    }

    at(&text, 0, &parts, true, &eq)
}

/// `Time.GetDateTimeFromTimestamp(ts).ToLocalTime().ToCommonString()` of a quest's last
/// completion.
fn quest_time(ts: u32) -> String {
    timestamp_common_string(f64::from(ts))
}

/// The "Can Solve" line of a quest listing.
fn can_solve_line(w: &World, next_solve: empyrean_common::dotnet::TimeSpan) -> String {
    use empyrean_common::dotnet::TimeSpan;
    if next_solve == TimeSpan::MIN_VALUE {
        "Can Solve: Immediately\n".to_owned()
    } else if next_solve == TimeSpan::MAX_VALUE {
        "Can Solve: Never again\n".to_owned()
    } else {
        format!(
            "Can Solve: In {} days, {} hours, {} minutes and, {} seconds. ({})\n",
            next_solve.format("%d"),
            next_solve.format("%h"),
            next_solve.format("%m"),
            next_solve.format("%s"),
            to_common_string(w.now.utc + next_solve)
        )
    }
}

/// `Convert.ToString(value, 2)`: two's complement bits, no leading zeros.
fn to_binary(value: i32) -> String {
    format!("{:b}", value.cast_unsigned())
}

// ACE: AdminCommands.Handleqst
/// `qst (fellow) [list | bestow | stamp | erase | bits]`: query, stamp, and erase quests on the
/// targeted player.
#[allow(clippy::too_many_lines)]
pub fn handleqst(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // fellow bestow  stamp erase
    // @qst list[filter]-List the quest flags for the targeted player, if a filter is provided, you will only get quest flags back that have the filter as a substring of the quest name. (Filter IS case sensitive!)
    // @qst erase < quest flag > -Erase the specific quest flag from the targeted player.If no quest flag is given, it erases the entire quest table for the targeted player.
    // @qst erase fellow < quest flag > -Erase a fellowship quest flag.
    // @qst bestow < quest flag > -Stamps the specific quest flag on the targeted player.If this fails, it's probably because you spelled the quest flag wrong.
    // @qst - Query, stamp, and erase quests on the targeted player.
    if parameters.is_empty() {
        // todo: display help screen
        return;
    }

    let s = require(session);
    let me = session_player(w, s);
    let object_id = selected_object_id(w, me);

    let wo = current_landblock_get_object(w, me, object_id);

    let Some(creature) = wo.filter(|&g| is_creature(w, g)) else {
        match wo {
            None => me_send(
                w,
                me,
                &format!("Selected object (0x{object_id}) not found."),
            ),
            Some(wo) => me_send(
                w,
                me,
                &format!(
                    "Selected object {} (0x{object_id}) is not a creature.",
                    name_of(w, wo)
                ),
            ),
        }
        return;
    };
    let creature_name = name_of(w, creature);
    let p = |i: usize| parameters.get(i).map(String::as_str);
    let arg = |i: usize| {
        parameters
            .get(i)
            .cloned()
            .unwrap_or_else(|| panic!("IndexOutOfRangeException: parameters[{i}]"))
    };

    if parameters[0] == "list" {
        let mut quests_hdr = format!("Quest Registry for {creature_name} (0x{creature}):\n");
        quests_hdr += "================================================\n";
        me_send(w, me, &quests_hdr);

        let owner = quest_manager::QuestOwner::Creature(creature);
        let mut quests = quest_manager::get_quests(w, &owner);

        let mut filter = String::new();
        if parameters.len() >= 2 {
            filter = parameters[1].clone();

            if !filter.trim().is_empty() {
                quests.retain(|q| wildcard_is_match(&q.quest_name, &filter));
            }
        }

        if quests.is_empty() {
            let with = if filter.trim().is_empty() {
                String::new()
            } else {
                format!(" with filter {filter}")
            };
            me_send(w, me, &format!("No quests found{with}."));
            return;
        }

        for quest in quests {
            let mut quest_entry = String::new();
            quest_entry += &format!(
                "Quest Name: {}\nCompletions: {} | Last Completion: {} ({})\n",
                quest.quest_name,
                quest.num_times_completed,
                quest.last_time_completed,
                quest_time(quest.last_time_completed)
            );
            let next_solve = quest_manager::get_next_solve_time(w, &owner, &quest.quest_name);
            quest_entry += &can_solve_line(w, next_solve);

            quest_entry += "--====--\n";
            me_send(w, me, &quest_entry);
        }
        return;
    }

    if parameters[0] == "bestow" {
        if parameters.len() < 2 {
            // delete all quests?
            // seems unsafe, maybe a confirmation?
            return;
        }
        let quest_name = parameters[1].clone();
        let mut owner = quest_manager::QuestOwner::Creature(creature);
        if quest_manager::has_quest(w, &owner, &quest_name) {
            me_send(w, me, &format!("{creature_name} already has {quest_name}"));
            return;
        }

        if quest_manager::can_solve(w, &owner, &quest_name) {
            quest_manager::update(w, &mut owner, &quest_name);
            me_send(w, me, &format!("{quest_name} bestowed on {creature_name}"));
        } else {
            me_send(
                w,
                me,
                &format!("Couldn't bestow {quest_name} on {creature_name}"),
            );
        }
        return;
    }

    if parameters[0] == "erase" {
        if parameters.len() < 2 {
            // delete all quests?
            // seems unsafe, maybe a confirmation?
            me_send(w, me, "You must specify a quest to erase, if you want to erase all quests use the following command: /qst erase *");
            return;
        }
        let quest_name = parameters[1].clone();
        let mut owner = quest_manager::QuestOwner::Creature(creature);

        if quest_name == "*" {
            quest_manager::erase_all(w, &mut owner);
            me_send(w, me, "All quests erased.");
            return;
        }

        if !quest_manager::has_quest(w, &owner, &quest_name) {
            me_send(w, me, &format!("{quest_name} not found."));
            return;
        }
        quest_manager::erase(w, &mut owner, &quest_name);
        me_send(w, me, &format!("{quest_name} erased."));
        return;
    }

    if parameters[0] == "stamp" {
        let mut num_completions = i32::MIN;

        if parameters.len() > 2 {
            let Some(n) = dotnet_parse::int_try_parse(&parameters[2]) else {
                me_send(w, me, &format!("{} is not a valid int", parameters[2]));
                return;
            };
            num_completions = n;
        }
        let quest_name = arg(1);
        let mut owner = quest_manager::QuestOwner::Creature(creature);

        if num_completions == i32::MIN {
            quest_manager::update(w, &mut owner, &quest_name);
        } else {
            quest_manager::set_quest_completions(w, &mut owner, &quest_name, num_completions);
        }

        if let Some(quest) = quest_manager::get_quest(w, &owner, &quest_name) {
            me_send(
                w,
                me,
                &format!(
                    "{quest_name} stamped with {} completions.",
                    quest.num_times_completed
                ),
            );
        } else {
            me_send(
                w,
                me,
                &format!("Couldn't stamp {quest_name} on {creature_name}"),
            );
        }
        return;
    }

    if parameters[0] == "bits" {
        if parameters.len() < 2 {
            let mut msg =
                "@qst - Query, stamp, and erase quests on the targeted player\n".to_owned();
            msg += "Usage: @qst bits [on | off | show] <questname> <bits>\n";
            msg += "qst bits on  - Stamps the specific quest flag on the targeted player with specified bits ON. If this fails, it's probably because you spelled the quest flag wrong.\n";
            msg += "qst bits off - Stamps the specific quest flag on the targeted player with specified bits OFF. If this fails, it's probably because you spelled the quest flag wrong.\n";
            msg += "qst bits show - List the specific quest flag bits for the targeted player.\n";
            me_send(w, me, &msg);
            return;
        }

        if p(1) == Some("on") || p(1) == Some("off") {
            let on = p(1) == Some("on");
            if parameters.len() < 3 {
                me_send(w, me, "You must specify bits to turn on or off.");
                return;
            }
            // (ACE's `parameters.Length < 2` check cannot hold here.)

            let quest_name = arg(2);

            let quest_bits = arg(3);

            let hex = if quest_bits.to_lowercase().starts_with("0x") {
                substring_from(&quest_bits, 2)
            } else {
                quest_bits.clone()
            };
            let Some(bits) = dotnet_parse::uint_try_parse_hex(&hex) else {
                let kind = if on { "hex number" } else { "uint" };
                me_send(w, me, &format!("{} is not a valid {kind}", parameters[3]));
                return;
            };
            let bits_i = bits.cast_signed();
            let mut owner = quest_manager::QuestOwner::Creature(creature);
            let (state, already) = if on {
                (
                    "ON",
                    quest_manager::has_quest_bits(w, &owner, &quest_name, bits_i),
                )
            } else {
                (
                    "OFF",
                    quest_manager::has_no_quest_bits(w, &owner, &quest_name, bits_i),
                )
            };

            if already {
                me_send(
                    w,
                    me,
                    &format!(
                        "{creature_name} already has set 0x{} bits to {state} for {quest_name}",
                        format(bits, "X")
                    ),
                );
                return;
            }

            quest_manager::set_quest_bits(w, &mut owner, &quest_name, bits_i, on);
            me_send(
                w,
                me,
                &format!(
                    "{creature_name} has set 0x{} bits to {state} for {quest_name}",
                    format(bits, "X")
                ),
            );
            return;
        }

        if p(1) == Some("show") {
            // (ACE's `parameters.Length < 2` check cannot hold here.)

            let quest_name = arg(2);

            let mut quests_hdr =
                format!("Quest Bits Registry for {creature_name} (0x{creature}):\n");
            quests_hdr += "================================================\n";

            let owner = quest_manager::QuestOwner::Creature(creature);
            let Some(quest) = quest_manager::get_quest(w, &owner, &quest_name) else {
                me_send(w, me, &format!("{quest_name} not found."));
                return;
            };

            let max_solves = quest_manager::get_max_solves(w, &quest_name);
            let max_solves_binary = to_binary(max_solves);

            let mut quest_entry = String::new();
            quest_entry += &format!("Quest Name: {}\n", quest.quest_name);
            quest_entry += &format!(
                "Current Set Bits: 0x{}\n",
                format(quest.num_times_completed, "X")
            );
            quest_entry += &format!("Allowed Max Bits: 0x{}\n", format(max_solves, "X"));
            quest_entry += &format!(
                "Last Set On: {} ({})\n",
                quest.last_time_completed,
                quest_time(quest.last_time_completed)
            );

            //var nextSolve = creature.QuestManager.GetNextSolveTime(quest.QuestName);

            //if (nextSolve == TimeSpan.MinValue)
            //    questEntry += "Can Solve: Immediately\n";
            //else if (nextSolve == TimeSpan.MaxValue)
            //    questEntry += "Can Solve: Never again\n";
            //else
            //    questEntry += $"Can Solve: In {nextSolve:%d} days, {nextSolve:%h} hours, {nextSolve:%m} minutes and, {nextSolve:%s} seconds. ({(DateTime.UtcNow + nextSolve).ToLocalTime()})\n";

            let current = to_binary(quest.num_times_completed);
            let width = max_solves_binary.chars().count();
            quest_entry += &format!("-= Binary String Representation =-\n  C: {current:0>width$}\n  A: {max_solves_binary}\n");

            quest_entry += "--====--\n";
            me_send(w, me, &format!("{quests_hdr}{quest_entry}"));
        }
    }

    if parameters[0] == "fellow" {
        if !is_player(w, creature) {
            me_send(w, me, &format!("Selected object {creature_name} (0x{object_id}) is not a player and cannot have a fellowship."));
            return;
        }
        let player = creature;
        let Some(fellowship) = player_fellowship(w, player) else {
            me_send(
                w,
                me,
                &format!("Selected player {creature_name} (0x{object_id}) is not in a fellowship."),
            );
            return;
        };

        if parameters.len() < 2 {
            let mut msg =
                "@qst - Query, stamp, and erase quests on the targeted player\n".to_owned();
            msg += "Usage: @qst fellow [list | bestow | erase]\n";
            msg += "qst fellow list - List the quest flags for the Fellowship of targeted player\n";
            msg += "qst fellow bestow - Stamps the specific quest flag on the Fellowship of targeted player. If this fails, it's probably because you spelled the quest flag wrong.\n";
            msg += "qst fellow stamp - Stamps the specific quest flag on the Fellowship of targeted player the specified number of times. If this fails, it's probably because you spelled the quest flag wrong.\n";
            msg += "qst fellow erase - Erase the specific quest flag from the Fellowship of targeted player. If no quest flag is given, it erases the entire quest table for the Fellowship of targeted player.\n";
            me_send(w, me, &msg);
            return;
        }

        let parameters = parameters.to_vec();
        fellowship_with_quest_manager(
            w,
            fellowship,
            &mut |w: &mut World, owner: &mut quest_manager::QuestOwner<'_>| {
                qst_fellow(w, me, &creature_name, creature, owner, &parameters);
            },
        );
    }
}

/// The `qst fellow list | bestow | erase | stamp` branches, over the fellowship's quest manager.
fn qst_fellow(
    w: &mut World,
    me: ObjectGuid,
    creature_name: &str,
    creature: ObjectGuid,
    owner: &mut quest_manager::QuestOwner<'_>,
    parameters: &[String],
) {
    let arg = |i: usize| {
        parameters
            .get(i)
            .cloned()
            .unwrap_or_else(|| panic!("IndexOutOfRangeException: parameters[{i}]"))
    };
    if parameters[1] == "list" {
        let mut quests_hdr =
            format!("Quest Registry for Fellowship of {creature_name} (0x{creature}):\n");
        quests_hdr += "================================================\n";
        me_send(w, me, &quests_hdr);

        let mut quests = quest_manager::get_quests(w, owner);

        // Not ACE's (a fix): the filter is the word after "list", as
        // it is for a player's own list; ACE read "list" itself as the filter, so only quests
        // named "list" (any case) were listed.
        let mut filter = String::new();
        if parameters.len() >= 3 {
            filter = parameters[2].clone();

            if !filter.trim().is_empty() {
                quests.retain(|q| wildcard_is_match(&q.quest_name, &filter));
            }
        }

        if quests.is_empty() {
            let with = if filter.trim().is_empty() {
                String::new()
            } else {
                format!(" with filter {filter}")
            };
            me_send(w, me, &format!("No quests found{with}."));
            return;
        }

        for quest in quests {
            let mut quest_entry = String::new();
            quest_entry += &format!(
                "Quest Name: {}\nCompletions: {} | Last Completion: {} ({})\n",
                quest.quest_name,
                quest.num_times_completed,
                quest.last_time_completed,
                quest_time(quest.last_time_completed)
            );
            let next_solve = quest_manager::get_next_solve_time(w, owner, &quest.quest_name);
            quest_entry += &can_solve_line(w, next_solve);

            quest_entry += "--====--\n";
            me_send(w, me, &quest_entry);
        }
        return;
    }

    if parameters[1] == "bestow" {
        if parameters.len() < 3 {
            // delete all quests?
            // seems unsafe, maybe a confirmation?
            return;
        }
        let quest_name = parameters[2].clone();
        if quest_manager::has_quest(w, owner, &quest_name) {
            me_send(
                w,
                me,
                &format!("Fellowship of {creature_name} already has {quest_name}"),
            );
            return;
        }

        if quest_manager::can_solve(w, owner, &quest_name) {
            quest_manager::update(w, owner, &quest_name);
            me_send(
                w,
                me,
                &format!("{quest_name} bestowed on Fellowship of {creature_name}"),
            );
        } else {
            me_send(
                w,
                me,
                &format!("Couldn't bestow {quest_name} on Fellowship of {creature_name}"),
            );
        }
        return;
    }

    if parameters[1] == "erase" {
        if parameters.len() < 3 {
            // delete all quests?
            // seems unsafe, maybe a confirmation?
            me_send(w, me, "You must specify a quest to erase, if you want to erase all quests use the following command: /qst fellow erase *");
            return;
        }
        let quest_name = parameters[2].clone();

        if quest_name == "*" {
            quest_manager::erase_all(w, owner);
            me_send(w, me, "All quests erased.");
            return;
        }

        if !quest_manager::has_quest(w, owner, &quest_name) {
            me_send(w, me, &format!("{quest_name} not found."));
            return;
        }
        quest_manager::erase(w, owner, &quest_name);
        me_send(w, me, &format!("{quest_name} erased."));
        return;
    }

    if parameters[1] == "stamp" {
        let mut num_completions = i32::MIN;

        if parameters.len() > 3 {
            let Some(n) = dotnet_parse::int_try_parse(&parameters[3]) else {
                me_send(w, me, &format!("{} is not a valid int", parameters[3]));
                return;
            };
            num_completions = n;
        }
        let quest_name = arg(2);

        if num_completions == i32::MIN {
            quest_manager::update(w, owner, &quest_name);
        } else {
            quest_manager::set_quest_completions(w, owner, &quest_name, num_completions);
        }

        if let Some(quest) = quest_manager::get_quest(w, owner, &quest_name) {
            me_send(
                w,
                me,
                &format!(
                    "{quest_name} stamped with {} completions.",
                    quest.num_times_completed
                ),
            );
        } else {
            me_send(
                w,
                me,
                &format!("Couldn't stamp {quest_name} on {creature_name}"),
            );
        }
    }
}

// ACE: AdminCommands.HandleRaise
/// `raise`: ACE's body is empty ("TODO: output").
pub fn handle_raise(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @raise - Raises your experience (or the experience in a skill) by the given amount.

    // TODO: output
}

// ACE: AdminCommands.HandleRename
/// `rename < Current Name >, < New Name >`: rename a character. (Do NOT include +'s for admin
/// names)
pub fn handle_rename(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // @rename <Current Name>, <New Name> - Rename a character. (Do NOT include +'s for admin names)

    if !join(parameters).contains(',') {
        write_output_info(
            w,
            session,
            "Error, cannot rename. You must include the old name followed by a comma and then the new name.\n Example: @rename Old Name, New Name",
            ChatMessageType::Broadcast,
        );
        return;
    }

    let joined = join(parameters);
    let names: Vec<&str> = joined.split(',').collect();

    let mut old_name = trim_spaces(names[0]);
    let mut new_name = trim_spaces(names[1]);

    if old_name.starts_with('+') {
        old_name = substring_from(&old_name, 1);
    }
    if new_name.starts_with('+') {
        new_name = substring_from(&new_name, 1);
    }

    new_name = upper_first(&new_name);

    let online_player = player_manager::get_online_player_by_name(w, &old_name);
    let offline_player = player_manager::get_offline_player_by_name(w, &old_name).map(|o| o.guid);
    if let Some(online_player) = online_player {
        let checked = new_name.clone();
        w.shard.is_character_name_available(
            checked,
            Some(Box::new(move |w: &mut World, is_available: bool| {
                if !is_available {
                    write_output_info(
                        w,
                        session,
                        &format!("Error, a player named \"{new_name}\" already exists."),
                        ChatMessageType::Broadcast,
                    );
                    return;
                }

                let o = obj_mut(w, online_player);
                if let Some(character) = o.player.as_mut().and_then(|p| p.player.character.as_mut())
                {
                    character.name.clone_from(&new_name);
                } else {
                    panic!("NullReferenceException: onlinePlayer.Character");
                }
                if let Some(p) = o.player.as_mut() {
                    p.player_database.character_changes_detected = true;
                }
                o.set_property(PropertyString::Name, new_name.clone());
                empyrean_world::world_objects::player_database::save_player_to_database(
                    w,
                    online_player,
                );

                write_output_info(
                    w,
                    session,
                    &format!("Player named \"{old_name}\" renamed to \"{new_name}\" successfully!"),
                    ChatMessageType::Broadcast,
                );

                let player_session = player_manager::player_session(w, online_player)
                    .expect("NullReferenceException: onlinePlayer.Session");
                empyrean_world::sessions::log_off_player(w, player_session, false);
            })),
        );
    } else if let Some(offline_player) = offline_player {
        let checked = new_name.clone();
        w.shard.is_character_name_available(
            checked,
            Some(Box::new(move |w: &mut World, is_available: bool| {
                if !is_available {
                    write_output_info(
                        w,
                        session,
                        &format!("Error, a player named \"{new_name}\" already exists."),
                        ChatMessageType::Broadcast,
                    );
                    return;
                }

                let character = w
                    .shard
                    .base_database()
                    .get_character_stub_by_name(&old_name)
                    .expect("NullReferenceException: character");

                let character_id = character.id;
                let (old, new) = (old_name.clone(), new_name.clone());
                w.shard.get_characters(
                    character.account_id,
                    false,
                    Some(Box::new(move |w: &mut World, result: Vec<Character>| {
                        let found_character_match =
                            result.into_iter().find(|c| c.id == character_id);

                        // Not ACE's (a fix): when the character is not
                        // found the error is the only outcome; ACE went on to rename a null
                        // character (NullReferenceException).
                        let Some(found) = found_character_match else {
                            write_output_info(
                                w,
                                session,
                                &format!("Error, a player named \"{old}\" cannot be found."),
                                ChatMessageType::Broadcast,
                            );
                            return;
                        };
                        w.shard.rename_character(found, new, None);
                    })),
                );

                let now = w.now.utc;
                if let Some(o) = w
                    .player_manager
                    .offline_players
                    .get_mut(&offline_player.full())
                {
                    o.set_property(PropertyString::Name, new_name.clone());
                }
                i_player::save_biota_to_database(
                    w,
                    i_player::IPlayer::Offline(offline_player),
                    true,
                );
                let _ = now;

                write_output_info(
                    w,
                    session,
                    &format!("Player named \"{old_name}\" renamed to \"{new_name}\" successfully!"),
                    ChatMessageType::Broadcast,
                );
            })),
        );
    } else {
        write_output_info(
            w,
            session,
            &format!("Error, a player named \"{old_name}\" cannot be found."),
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: AdminCommands.Handlesetadvclass
/// `setadvclass`: ACE's body is empty ("TODO: output").
pub fn handlesetadvclass(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @setadvclass - Sets the advancement class of one of your own skills.

    // TODO: output
}

// ACE: AdminCommands.HandleSpendxp
/// `spendxp`: ACE's body is empty ("TODO: output").
pub fn handle_spendxp(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @spendxp - Allows you to more quickly spend your available xp into the specified skill.

    // TODO: output
}

// ACE: AdminCommands.Handletrainskill
/// `trainskill`: ACE's body is empty ("TODO: output").
pub fn handletrainskill(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @trainskill - Attempts to train the specified skill by spending skill credits on it.

    // TODO: output
}

// ACE: AdminCommands.Handlereloadsysmsg
/// `reloadsysmsg`: ACE's body is empty ("TODO: output").
pub fn handlereloadsysmsg(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @reloadsysmsg - Causes all servers to reload system_messages.txt.

    // TODO: output
}

// ACE: AdminCommands.HandleGameCastLocal
/// `gamecastlocal <message>`: sends a server-wide broadcast.
pub fn handle_game_cast_local(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // Local Server Broadcast from
    // usage: @gamecastlocal<message>
    // This command sends the specified text to every player on the current server.
    // See Also: @gamecast, @gamecastemote, @gamecastlocal, @gamecastlocalemote.
    // @gamecastlocal Sends a server-wide broadcast.

    // Since we only have one server, this command will just call the other one
    handle_gamecast(w, session, parameters);
}

// ACE: AdminCommands.HandleSticky
/// Sets whether you lose items should you die.
pub fn handle_sticky(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let sticky = !(parameters.first().is_some_and(|p| p == "off"));

    if sticky {
        write_output_info(
            w,
            session,
            "You will no longer drop any items on death.",
            ChatMessageType::Broadcast,
        );
    } else {
        write_output_info(
            w,
            session,
            "You will now drop items on death normally.",
            ChatMessageType::Broadcast,
        );
    }

    let me = session_player(w, require(session));
    obj_mut(w, me).set_no_corpse(sticky);
}

// ACE: AdminCommands.Handleuserlimit
/// `userlimit { num }`: ACE's body is empty ("TODO: output").
pub fn handleuserlimit(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @userlimit - Sets how many clients are allowed to connect to this world.

    // TODO: output
}

// ACE: AdminCommands.Handlewatchmen
/// `watchmen (accesslevel)`: displays a list of accounts with the specified level of admin
/// access.
pub fn handlewatchmen(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // @watchmen - Displays a list of accounts with the specified level of admin access.

    let default_access_level = AccessLevel::Advocate;

    let mut access_level = default_access_level;

    if !parameters.is_empty() {
        // `Enum.TryParse` sets the out value to default(AccessLevel) when it fails
        match try_parse_access_level(&parameters[0], true) {
            Some(parsed) => {
                access_level = if parsed.is_defined() {
                    parsed
                } else {
                    default_access_level
                }
            }
            None => access_level = AccessLevel(0),
        }
    }

    let list = w
        .auth
        .lock()
        .get_listof_accounts_by_access_level(access_level);

    let message = if list.is_empty() {
        format!(
            "There are no accounts with {} rights.",
            access_level.to_dotnet_string()
        )
    } else {
        let mut message = format!(
            "The following accounts have been granted {} rights:\n",
            access_level.to_dotnet_string()
        );
        for item in list {
            message += &item;
            message += "\n";
        }
        message
    };

    write_output_info(w, session, &message, ChatMessageType::WorldBroadcast);
}

// ACE: AdminCommands.HandleGameCastEmote
/// `gamecastemote <message>`: sends text to all players, formatted exactly as entered.
pub fn handle_game_cast_emote(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: "@gamecastemote <message>" or "@we <message"
    // Sends text to all players, formatted exactly as entered, with no prefix of any kind.
    // See Also: @gamecast, @gamecastemote, @gamecastlocal, @gamecastlocalemote.
    // @gamecastemote - Sends text to all players, formatted exactly as entered.

    let msg = join(parameters).replace("\\n", "\n");
    //session.Player.HandleActionWorldBroadcast($"{msg}", ChatMessageType.WorldBroadcast);

    let sys_message = game_message_system_chat(&msg, ChatMessageType::WorldBroadcast);
    player_manager::broadcast_to_all(w, &sys_message);
    let sender = session_player_opt(w, session);
    player_manager::log_broadcast_chat(w, Channel::AllBroadcast, sender, &msg);
}

// ACE: AdminCommands.HandleWe
/// `we <message>`: sends text to all players, formatted exactly as entered.
pub fn handle_we(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: "@gamecastemote <message>" or "@we <message"
    // Sends text to all players, formatted exactly as entered, with no prefix of any kind.
    // See Also: @gamecast, @gamecastemote, @gamecastlocal, @gamecastlocalemote.
    // @gamecastemote - Sends text to all players, formatted exactly as entered.

    handle_game_cast_emote(w, session, parameters);
}

// ACE: AdminCommands.Handledumpattackers
/// `dumpattackers`: ACE's body is empty ("TODO: output").
pub fn handledumpattackers(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @dumpattackers - Displays the detection and enemy information for the selected creature.

    // TODO: output
}

// ACE: AdminCommands.Handleknownobjs
/// `knownobjs`: ACE's body is empty ("TODO: output"); DeveloperCommands registers the same name
/// later, which replaces it in the table.
pub fn handleknownobjs(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @knownobjs - Display a list of objects that the client is aware of.

    // TODO: output
}

// ACE: AdminCommands.Handlelbinterval
/// `lbinterval`: ACE's body is empty ("TODO: output").
pub fn handlelbinterval(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @lbinterval - Sets how often in seconds the server farm will rebalance the server farm load.

    // TODO: output
}

// ACE: AdminCommands.Handlelbthresh
/// `lbthresh`: ACE's body is empty ("TODO: output").
pub fn handlelbthresh(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // the @lbthresh command sets the maximum amount of load servers can trade at each balance. Large load transfers at once can cause poor server performance.  (Large would be about 400, small is about 20.)
    // @lbthresh - Set how much load can be transferred between two servers during a single load balance.

    // TODO: output
}

// ACE: AdminCommands.Handleradar
/// `radar`: ACE's body is empty ("TODO: output").
pub fn handleradar(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @radar - Toggles your radar on and off.

    // TODO: output
}

// ACE: AdminCommands.HandleRaresDump
/// `rares dump`: ACE's body is empty ("TODO: output").
pub fn handle_rares_dump(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @rares dump - Lists all tiers of rare items.

    // TODO: output
}

// ACE: AdminCommands.Handlestormnumstormed
/// `stormnumstormed`: ACE's body is empty ("TODO: output").
pub fn handlestormnumstormed(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @stormnumstormed - Sets how many characters are teleported away during a portal storm.

    // TODO: output
}

// ACE: AdminCommands.Handlestormthresh
/// `stormthresh`: ACE's body is empty ("TODO: output").
pub fn handlestormthresh(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @stormthresh - Sets how many character can be in a landblock before we do a portal storm.

    // TODO: output
}

// ACE: AdminCommands.HandleDisplayProps
/// `showprops`: displays the name of all properties configurable via the modify commands.
pub fn handle_display_props(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let text = property_manager::list_properties(w);
    write_output_info(w, session, &text, ChatMessageType::Broadcast);
}

/// `bool.Parse(value)`: "True" or "False" in any case, optionally padded with white space and
/// trailing NULs.
///
/// # Errors
/// Anything else (.NET's `FormatException`).
pub fn dotnet_bool_parse(value: &str) -> Result<bool, CaughtException> {
    let check = |v: &str| {
        if v.eq_ignore_ascii_case("True") {
            Some(true)
        } else if v.eq_ignore_ascii_case("False") {
            Some(false)
        } else {
            None
        }
    };
    if let Some(b) = check(value) {
        return Ok(b);
    }
    let trimmed = value
        .trim_start_matches(|c: char| c.is_whitespace() || c == '\0')
        .trim_end_matches(|c: char| c.is_whitespace() || c == '\0');
    check(trimmed).ok_or(CaughtException {
        source: "System.Private.CoreLib",
        message: format!("String '{value}' was not recognized as a valid Boolean."),
    })
}

// ACE: AdminCommands.HandleModifyServerBoolProperty
/// `modifybool (string) (bool)`: modifies a server property that is a bool.
pub fn handle_modify_server_bool_property(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let Ok(bool_val) = dotnet_bool_parse(&parameters[1]) else {
        write_output_info(
            w,
            session,
            "Please input a valid bool",
            ChatMessageType::Help,
        );
        return;
    };

    let prev_state = property_manager::get_bool(w, &parameters[0], false, true);

    if prev_state.item == bool_val
        && prev_state
            .description
            .as_deref()
            .is_some_and(|d| !d.trim().is_empty())
    {
        write_output_info(
            w,
            session,
            &format!(
                "Bool property is already {} for {}!",
                bool_string(bool_val),
                parameters[0]
            ),
            ChatMessageType::Broadcast,
        );
        return;
    }

    if property_manager::modify_bool(w, &parameters[0], bool_val) {
        write_output_info(
            w,
            session,
            "Bool property successfully updated!",
            ChatMessageType::Broadcast,
        );
        let issuer = session_player_opt(w, session);
        player_manager::broadcast_to_audit_channel(
            w,
            issuer,
            &format!(
                "Successfully changed server bool property {} to {}",
                parameters[0],
                bool_string(bool_val)
            ),
        );

        if parameters[0] == "pk_server" || parameters[0] == "pkl_server" {
            player_manager::update_pk_status_for_all_players(w, &parameters[0], bool_val);
        }
    } else {
        write_output_info(
            w,
            session,
            "Unknown bool property was not updated. Type showprops for a list of properties.",
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: AdminCommands.HandleFetchServerBoolProperty
/// `fetchbool (string)`: fetches a server property that is a bool.
pub fn handle_fetch_server_bool_property(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let bool_val = property_manager::get_bool(w, &parameters[0], false, false);
    let text = format!(
        "{} - {}: {}",
        parameters[0],
        bool_val.description.as_deref().unwrap_or("No Description"),
        bool_string(bool_val.item)
    );
    write_output_info(w, session, &text, ChatMessageType::Broadcast);
}

// ACE: AdminCommands.HandleModifyServerLongProperty
/// `modifylong (string) (long)`: modifies a server property that is a long.
pub fn handle_modify_server_long_property(
    w: &mut World,
    session: Option<SessionId>,
    paramters: &[String],
) {
    let Some(long_val) = dotnet_parse::long_try_parse(&paramters[1]) else {
        write_output_info(
            w,
            session,
            "Please input a valid long",
            ChatMessageType::Help,
        );
        return;
    };
    if property_manager::modify_long(w, &paramters[0], long_val) {
        write_output_info(
            w,
            session,
            "Long property successfully updated!",
            ChatMessageType::Broadcast,
        );
        let issuer = session_player_opt(w, session);
        player_manager::broadcast_to_audit_channel(
            w,
            issuer,
            &format!(
                "Successfully changed server long property {} to {long_val}",
                paramters[0]
            ),
        );
    } else {
        write_output_info(
            w,
            session,
            "Unknown long property was not updated. Type showprops for a list of properties.",
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: AdminCommands.HandleFetchServerLongProperty
/// `fetchlong (string)`: fetches a server property that is a long.
pub fn handle_fetch_server_long_property(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let int_val = property_manager::get_long(w, &parameters[0], 0, false);
    let text = format!(
        "{} - {}: {}",
        parameters[0],
        int_val.description.as_deref().unwrap_or("No Description"),
        int_val.item
    );
    write_output_info(w, session, &text, ChatMessageType::Broadcast);
}

// ACE: AdminCommands.HandleModifyServerFloatProperty
/// `modifydouble (string) (double)`: modifies a server property that is a double.
pub fn handle_modify_server_float_property(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let Some(double_val) = dotnet_parse::double_try_parse(&parameters[1]) else {
        write_output_info(
            w,
            session,
            "Please input a valid double",
            ChatMessageType::Help,
        );
        return;
    };
    if property_manager::modify_double(w, &parameters[0], double_val, false) {
        write_output_info(
            w,
            session,
            "Double property successfully updated!",
            ChatMessageType::Broadcast,
        );
        let issuer = session_player_opt(w, session);
        player_manager::broadcast_to_audit_channel(
            w,
            issuer,
            &format!(
                "Successfully changed server double property {} to {}",
                parameters[0],
                to_string(double_val)
            ),
        );
    } else {
        write_output_info(
            w,
            session,
            "Unknown double property was not updated. Type showprops for a list of properties.",
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: AdminCommands.HandleFetchServerFloatProperty
/// `fetchdouble (string)`: fetches a server property that is a double.
pub fn handle_fetch_server_float_property(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let float_val = property_manager::get_double(w, &parameters[0], 0.0, false);
    let text = format!(
        "{} - {}: {}",
        parameters[0],
        float_val.description.as_deref().unwrap_or("No Description"),
        to_string(float_val.item)
    );
    write_output_info(w, session, &text, ChatMessageType::Broadcast);
}

// ACE: AdminCommands.HandleModifyServerStringProperty
/// `modifystring (string) (string)`: modifies a server property that is a string.
pub fn handle_modify_server_string_property(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    if property_manager::modify_string(w, &parameters[0], &parameters[1]) {
        write_output_info(
            w,
            session,
            "String property successfully updated!",
            ChatMessageType::Broadcast,
        );
        let issuer = session_player_opt(w, session);
        player_manager::broadcast_to_audit_channel(
            w,
            issuer,
            &format!(
                "Successfully changed server string property {} to {}",
                parameters[0], parameters[1]
            ),
        );
    } else {
        write_output_info(
            w,
            session,
            "Unknown string property was not updated. Type showprops for a list of properties.",
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: AdminCommands.HandleFetchServerStringProperty
/// `fetchstring (string)`: fetches a server property that is a string.
pub fn handle_fetch_server_string_property(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let string_val = property_manager::get_string(w, &parameters[0], "", false);
    let text = format!(
        "{} - {}: {}",
        parameters[0],
        string_val
            .description
            .as_deref()
            .unwrap_or("No Description"),
        string_val.item
    );
    write_output_info(w, session, &text, ChatMessageType::Broadcast);
}

// ACE: AdminCommands.HandleModifyPropertyDescription
/// `modifypropertydesc <STRING|BOOL|DOUBLE|LONG> (string) (string)`: modifies a server property's
/// description.
pub fn handle_modify_property_description(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let _is_session = session.is_some();
    match parameters[0].as_str() {
        "STRING" => {
            property_manager::modify_string_description(w, &parameters[1], Some(&parameters[2]))
        }
        "BOOL" => {
            property_manager::modify_bool_description(w, &parameters[1], Some(&parameters[2]))
        }
        "DOUBLE" => {
            property_manager::modify_double_description(w, &parameters[1], Some(&parameters[2]))
        }
        "LONG" => {
            property_manager::modify_long_description(w, &parameters[1], Some(&parameters[2]))
        }
        _ => {
            write_output_info(
                w,
                session,
                "Please pick from STRING, BOOL, DOUBLE, or LONG",
                ChatMessageType::Help,
            );
            return;
        }
    }

    write_output_info(
        w,
        session,
        "Successfully updated property description!",
        ChatMessageType::Help,
    );
}

// ACE: AdminCommands.HandleResyncServerProperties
/// `resyncproperties`: resync the properties database.
pub fn handle_resync_server_properties(
    w: &mut World,
    _session: Option<SessionId>,
    _parameters: &[String],
) {
    property_manager::resync_variables(w);
}

// ACE: AdminCommands.HandleFixAllegiances
/// `fix-allegiances` (console): fixes the monarch data for allegiances.
pub fn handle_fix_allegiances(w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    let players = player_manager::get_all_players(w);

    // build allegiances
    for &player in &players {
        allegiance_manager_get_allegiance(w, Some(player));
    }

    // (ACE's lazy `Where` sees each player as the loop reaches it; the loop only changes the
    // current player, so filtering first is the same.)
    let with_monarch: Vec<i_player::IPlayer> = players
        .iter()
        .copied()
        .filter(|&p| i_player::monarch_id(w, p).is_some())
        .collect();
    for player in with_monarch {
        let monarch_id = i_player::monarch_id(w, player).expect("filtered");

        // find multi allegiances
        for allegiance in allegiance_manager_allegiances(w) {
            if allegiance_monarch_id(w, allegiance) == Some(monarch_id) {
                continue;
            }

            if allegiance_members_contains(w, allegiance, monarch_id) {
                let desynced = player_manager::find_by_guid(w, monarch_id)
                    .0
                    .expect("NullReferenceException: desynced");
                console_write_line(&format!(
                    "{} has references to {} as monarch, but should be {} -- fixing",
                    i_player::name(w, player).unwrap_or_default(),
                    i_player::name(w, desynced).unwrap_or_default(),
                    allegiance_monarch_player_name(w, allegiance)
                ));

                let new_monarch = allegiance_monarch_id(w, allegiance);
                i_player::set_monarch_id(w, player, new_monarch);
                i_player::save_biota_to_database(w, player, true);
            }
        }

        // find missing players
        let monarch_guid = i_player::monarch_id(w, player)
            .expect("InvalidOperationException: Nullable object must have a value.");
        let monarch = player_manager::find_by_guid(w, monarch_guid).0;
        let allegiance = allegiance_manager_get_allegiance(w, monarch);

        if let Some(allegiance) =
            allegiance.filter(|&a| !allegiance_members_contains(w, a, player.guid().full()))
        {
            // walk patrons to get the updated monarch
            let patron_id = i_player::patron_id(w, player)
                .expect("InvalidOperationException: Nullable object must have a value.");
            let Some(mut patron) = player_manager::find_by_guid(w, patron_id).0 else {
                console_write_line(&format!(
                    "{} has references to deleted patron {}, checking for vassals",
                    i_player::name(w, player).unwrap_or_default(),
                    format(patron_id, "X8")
                ));
                i_player::set_patron_id(w, player, None);

                let guid = player.guid().full();
                let vassals = players
                    .iter()
                    .filter(|&&i| i_player::patron_id(w, i) == Some(guid))
                    .count();
                if vassals > 0 {
                    console_write_line(&format!(
                        "Vassals found, {} is the monarch",
                        i_player::name(w, player).unwrap_or_default()
                    ));
                    i_player::set_monarch_id(w, player, Some(guid));
                } else {
                    console_write_line(
                        "No vassals found, removing patron reference to deleted character",
                    );
                    i_player::set_monarch_id(w, player, None);
                }
                i_player::save_biota_to_database(w, player, true);
                continue;
            };

            while let Some(next) = i_player::patron_id(w, patron) {
                patron = player_manager::find_by_guid(w, next)
                    .0
                    .expect("NullReferenceException: patron");
            }

            if i_player::monarch_id(w, player) != Some(patron.guid().full()) {
                console_write_line(&format!(
                    "{} has references to {} as monarch, but should be {} -- fixing missing player",
                    i_player::name(w, player).unwrap_or_default(),
                    monarch
                        .and_then(|m| i_player::name(w, m))
                        .expect("NullReferenceException: monarch.Name"),
                    i_player::name(w, patron).unwrap_or_default()
                ));

                i_player::set_monarch_id(w, player, Some(patron.guid().full()));
                i_player::save_biota_to_database(w, player, true);
            }
            let _ = allegiance;
        }
    }

    for allegiance in allegiance_manager_allegiances(w) {
        allegiance_manager_rebuild(w, allegiance);
    }
}

// ACE: AdminCommands.HandleShowAllegiances
/// `show-allegiances`: shows all of the allegiance chains on the server.
pub fn handle_show_allegiances(w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    let players = player_manager::get_all_players(w);

    // build allegiances
    for player in players {
        allegiance_manager_get_allegiance(w, Some(player));
    }

    for allegiance in allegiance_manager_allegiances(w) {
        allegiance_show_info(w, allegiance);
        console_write_line("---------------");
    }
}

// ACE: AdminCommands.HandleGetEnchantments
/// `getenchantments`: shows the enchantments for the last appraised item.
pub fn handle_get_enchantments(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let Some(item) = command_handler_helper::get_last_appraised_object(w, s) else {
        return;
    };

    let entries: Vec<_> = empyrean_entity::models::properties_enchantment_registry_extensions::get_enchantments_top_layer(
        obj(w, item).biota.properties_enchantment_registry.as_ref(),
    )
    .expect("NullReferenceException: item.Biota.PropertiesEnchantmentRegistry")
    .into_iter()
    .cloned()
    .collect();

    for enchantment in entries {
        let e = empyrean_world::network::structure::enchantment::enchantment_from_registry(
            w,
            item,
            &enchantment,
        );
        let info = e.get_info(w);
        system_chat(w, s, &info, ChatMessageType::Broadcast);
    }
}

// ACE: AdminCommands.HandleCM
/// `cm <material type> <quantity> <ave. workmanship>`: create a salvage bag in your inventory.
pub fn handle_cm(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // Format is: @cm <material type> <quantity> <ave. workmanship>
    handle_ci_salvage(w, session, parameters);
}

// ACE: AdminCommands.HandleCISalvage
/// `cisalvage <material_type>, optional: <structure> <workmanship> <num_items>`: create a salvage
/// bag in your inventory.
pub fn handle_ci_salvage(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Some(material_type) = try_parse_material_type(&parameters[0], true) else {
        system_chat(
            w,
            s,
            &format!("Couldn't find material type {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let wcid = player_material_salvage(material_type)
        .unwrap_or_else(|| panic!("KeyNotFoundException: MaterialSalvage[{}]", material_type.0));
    let salvage_bag = world_object_factory_create_new_world_object(w, wcid)
        .expect("NullReferenceException: salvageBag");

    // A failed `TryParse` leaves its out value at 0.
    let structure: u16 = if parameters.len() > 1 {
        dotnet_parse::uint_try_parse(&parameters[1])
            .and_then(|v| u16::try_from(v).ok())
            .unwrap_or(0)
    } else {
        100
    };

    let workmanship: f32 = if parameters.len() > 2 {
        dotnet_parse::float_try_parse(&parameters[2]).unwrap_or(0.0)
    } else {
        10.0
    };

    let mut num_items_in_material: i32 =
        empyrean_common::dotnet::math::round(f64::from(workmanship)).cs_cast();
    if parameters.len() > 3 {
        num_items_in_material = dotnet_parse::int_try_parse(&parameters[3]).unwrap_or(0);
    }

    #[allow(clippy::cast_precision_loss)]
    let product = workmanship * num_items_in_material as f32;
    let item_workmanship: i32 = empyrean_common::dotnet::math::round(f64::from(product)).cs_cast();

    let bag = obj_mut(w, salvage_bag);
    bag.set_property(PropertyString::Name, format!("Salvage ({structure})"));
    bag.set_structure(Some(structure));
    bag.set_item_workmanship(Some(item_workmanship));
    bag.set_num_items_in_material(Some(num_items_in_material));

    let me = session_player(w, s);
    player_inventory::try_create_in_inventory_with_networking(w, me, salvage_bag);
}

/// `Enum.TryParse(parameters[0], true, out environChange)` and `Enum.IsDefined`: a failed parse
/// leaves `Clear` (0), an undefined value becomes `Clear`.
fn parse_environ_change(parameter: &str) -> EnvironChangeType {
    match try_parse_environ_change_type(parameter, true) {
        Some(e) if e.is_defined() => e,
        _ => EnvironChangeType::Clear,
    }
}

// ACE: AdminCommands.HandleSetLBEnviron
/// `setlbenviron (name or id of EnvironChangeType)`: sets or clears your current landblock's
/// environment option.
pub fn handle_set_lb_environ(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let mut environ_change = EnvironChangeType::Clear;

    if !parameters.is_empty() {
        if parameters[0] == "list" {
            system_chat(w, s, &environ_list_msg(), ChatMessageType::Broadcast);
            return;
        }

        environ_change = parse_environ_change(&parameters[0]);
    }

    let me = session_player(w, s);
    let lb = obj(w, me)
        .current_landblock
        .expect("NullReferenceException: session.Player.CurrentLandblock");
    let lb_text = format(lb.landblock(), "X4");
    let name = environ_change.to_dotnet_string();
    if environ_change.is_fog() {
        system_chat(w, s, &format!("Setting Landblock (0x{lb_text}), including direct adjacent landblocks, to EnvironChangeType.{name}."), ChatMessageType::Broadcast);
        let text = format!("{} set Landblock (0x{lb_text}), including direct adjacent landblocks, to EnvironChangeType.{name}.", name_of(w, me));
        player_manager::broadcast_to_audit_channel(w, Some(me), &text);
    } else {
        system_chat(w, s, &format!("Sending EnvironChangeType.{name} to all players on Landblock (0x{lb_text}), including direct adjacent landblocks."), ChatMessageType::Broadcast);
        let text = format!("{} sent EnvironChangeType.{name} to all players on Landblock (0x{lb_text}), including direct adjacent landblocks.", name_of(w, me));
        player_manager::broadcast_to_audit_channel(w, Some(me), &text);
    }

    if let Some(lb) = obj(w, me).current_landblock {
        landblock::do_environ_change(w, lb, environ_change);
    }
}

// ACE: AdminCommands.HandleSetGlobalEnviron
/// `setglobalenviron (name or id of EnvironChangeType)`: sets or clears server's global
/// environment option.
pub fn handle_set_global_environ(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let mut environ_change = EnvironChangeType::Clear;

    if !parameters.is_empty() {
        if parameters[0] == "list" {
            system_chat(w, s, &environ_list_msg(), ChatMessageType::Broadcast);
            return;
        }

        environ_change = parse_environ_change(&parameters[0]);
    }

    let me = session_player(w, s);
    let name = environ_change.to_dotnet_string();
    if environ_change.is_fog() {
        system_chat(
            w,
            s,
            &format!("Setting all landblocks to EnvironChangeType.{name} ."),
            ChatMessageType::Broadcast,
        );
        let text = format!(
            "{} set all landblocks to EnvironChangeType.{name} .",
            name_of(w, me)
        );
        player_manager::broadcast_to_audit_channel(w, Some(me), &text);
    } else {
        system_chat(
            w,
            s,
            &format!("Sending EnvironChangeType.{name} to all players on all Landblocks."),
            ChatMessageType::Broadcast,
        );
        let text = format!(
            "{} sent EnvironChangeType.{name} to all players on all Landblocks.",
            name_of(w, me)
        );
        player_manager::broadcast_to_audit_channel(w, Some(me), &text);
    }

    empyrean_world::managers::landblock_manager::do_environ_change(w, environ_change);
}

// ACE: AdminCommands.EnvironListMsg
/// The `list` answer of the environment commands: every `EnvironChangeType` name, then notes.
#[must_use]
pub fn environ_list_msg() -> String {
    let mut msg = "Complete list of EnvironChangeType:\n".to_owned();
    for name in EnvironChangeType::NAMES {
        msg += name;
        msg += "\n";
    }

    msg += "Notes about above list:\n";
    msg += "Clear resets to default.\nAll options ending with Fog are continuous.\nAll options ending with Fog2 are continuous and blank radar.\nAll options ending with Sound play once and do not repeat.";

    msg
}

// ACE: AdminCommands.HandleMoveToMe
/// `movetome`: moves the last appraised object to the current player location.
pub fn handle_move_to_me(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let Some(o) = command_handler_helper::get_last_appraised_object(w, s) else {
        return;
    };

    if obj(w, o).current_landblock.is_none() {
        system_chat(
            w,
            s,
            &format!("{} ({o}) is not a landblock object", name_of(w, o)),
            ChatMessageType::Broadcast,
        );
        return;
    }

    if is_player(w, o) {
        let name = name_of(w, o);
        handle_tele_to_me(w, session, &[name]);
        return;
    }

    let prev_loc = obj(w, o)
        .location()
        .expect("NullReferenceException: obj.Location");
    let me = session_player(w, s);
    let mut new_loc = Position::from_position(&location_of(w, me));
    new_loc.set_rotation(prev_loc.rotation()); // keep previous rotation

    // `obj.PhysicsObj.SetPosition(new SetPosition(newLoc.PhysPosition(), Teleport | Slide))`
    // DIVERGE (forced): the shared physics crate's SetPosition answers only whether the position
    // was committed; a failure is reported as `GeneralFailure`.
    let h = obj(w, o)
        .phys
        .expect("NullReferenceException: obj.PhysicsObj");
    let ok = phys_ext::set_position(w, h, &phys_ext::to_physics_position(&new_loc));

    if !ok {
        system_chat(
            w,
            s,
            &format!(
                "Failed to move {} ({o}) to current location: GeneralFailure",
                name_of(w, o)
            ),
            ChatMessageType::Broadcast,
        );
        return;
    }
    system_chat(
        w,
        s,
        &format!("Moving {} ({o}) to current location", name_of(w, o)),
        ChatMessageType::Broadcast,
    );

    // `obj.PhysicsObj.Position.ACEPosition()`: `new Position(ObjCellID, Frame.Origin, Frame.Orientation)`
    let (cell, origin, rotation) = w
        .physics
        .get(h)
        .map(|p| {
            (
                p.position.cell.0,
                p.position.frame.origin,
                p.position.frame.rotation,
            )
        })
        .expect("the body just placed");
    let location = Position::from_vectors(
        cell,
        empyrean_common::dotnet::Vector3::new(origin.x, origin.y, origin.z),
        empyrean_common::dotnet::Quaternion::new(rotation.x, rotation.y, rotation.z, rotation.w),
    );
    obj_mut(w, o).set_location(Some(location));

    if prev_loc.landblock() != location.landblock() {
        empyrean_world::managers::landblock_manager::relocate_object_for_physics(w, o, true);
    }

    world_object_networking::send_update_position(w, o, true);
}

// ACE: AdminCommands.HandleReloadLootTables
/// `reload-loot-tables (optional profile folder)`: reloads the latest data from the loot tables.
pub fn handle_reload_loot_tables(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let sep = std::path::MAIN_SEPARATOR;

    let mut folder = format!("..{sep}..{sep}..{sep}..{sep}Factories{sep}Tables{sep}");
    if !parameters.is_empty() {
        folder.clone_from(&parameters[0]);
    }

    if !std::path::Path::new(&folder).is_dir() {
        write_output_info(
            w,
            session,
            &format!("{folder} not found"),
            ChatMessageType::Broadcast,
        );
        return;
    }
    empyrean_world::factories::entity::loot_swap::update_tables(&folder);
}

// @@HANDLERS@@

// ---------------------------------------------------------------------------------------------
// Pointers to members that are not ported yet (their owners swap these for the real calls).
// ---------------------------------------------------------------------------------------------

/// `session.Player.HandleAdminvisionToggle(choice)`.
fn player_handle_adminvision_toggle(w: &mut World, player: ObjectGuid, choice: i32) {
    empyrean_world::world_objects::player::handle_adminvision_toggle(w, player, choice);
}

/// `wo.DeleteObject(rootOwner)`.
fn world_object_delete_object(w: &mut World, wo: ObjectGuid, root_owner: Option<ObjectGuid>) {
    empyrean_world::world_objects::world_object_decay::delete_object(w, wo, root_owner);
}

/// `session.Player.HandleMRT()`.
fn player_handle_mrt(w: &mut World, player: ObjectGuid) {
    empyrean_world::world_objects::player::handle_mrt(w, player);
}

/// `session.Player.LearnSpellWithNetworking(spellId)` (`uiOutput` defaults to true).
fn player_learn_spell_with_networking(w: &mut World, player: ObjectGuid, spell_id: u32) {
    empyrean_world::world_objects::player_spells::learn_spell_with_networking(
        w, player, spell_id, true,
    );
}

/// `session.Player.RemoveKnownSpell(spellId)`.
fn player_remove_known_spell(w: &mut World, player: ObjectGuid, spell_id: u32) -> bool {
    empyrean_world::world_objects::player_spells::remove_known_spell(w, player, spell_id)
}

/// `HouseManager.GetAccountHouses(accountId)`.
fn house_manager_get_account_houses(w: &mut World, account_id: u32) -> Vec<ObjectGuid> {
    empyrean_world::managers::house_manager::get_account_houses(w, account_id)
}

/// `HouseManager.GetHouseById(houseId)`.
fn house_manager_get_house_by_id(w: &mut World, house_id: u32) -> Vec<ObjectGuid> {
    empyrean_world::managers::house_manager::get_house_by_id(w, house_id)
}

/// `HouseManager.PayRent(house)`.
fn house_manager_pay_rent(w: &mut World, house: ObjectGuid) -> bool {
    empyrean_world::managers::house_manager::pay_rent(w, house)
}

/// `HouseManager.PayAllRent()`.
fn house_manager_pay_all_rent(w: &mut World) {
    empyrean_world::managers::house_manager::pay_all_rent(w);
}

/// `house.GetHouseData(owner)`: its `(BuyTime, RentTime)`.
fn house_get_house_data(
    w: &mut World,
    house: ObjectGuid,
    owner: Option<i_player::IPlayer>,
) -> (u32, u32) {
    let data = empyrean_world::world_objects::house::get_house_data(w, house, owner);
    (data.buy_time, data.rent_time)
}

/// `house.GetRentDue(rentTime)`.
fn house_get_rent_due(w: &World, house: ObjectGuid, rent_time: u32) -> u32 {
    empyrean_world::world_objects::house::get_rent_due(w, house, rent_time)
}

/// `house.SlumLord.IsRentPaid()`.
fn slum_lord_is_rent_paid(w: &mut World, house: ObjectGuid) -> bool {
    let slum_lord = empyrean_world::world_objects::house::slum_lord(w, house)
        .expect("NullReferenceException: house.SlumLord");
    empyrean_world::world_objects::slum_lord::is_rent_paid(w, slum_lord)
}

/// `player.House`.
fn player_house(w: &World, player: ObjectGuid) -> Option<ObjectGuid> {
    empyrean_world::world_objects::player_house::house(w, player)
}

/// `house.RootHouse`.
fn house_root_house(w: &mut World, house: ObjectGuid) -> Option<ObjectGuid> {
    empyrean_world::world_objects::house::root_house(w, house)
}

/// `hook.House`, `storage.House`, `slumLord.House`, `housePortal.House` (`WorldObject.House`).
fn world_object_house(w: &World, wo: ObjectGuid) -> Option<ObjectGuid> {
    let parent = w.objects.get(wo)?.wo.world_object_links.parent_link?;
    w.objects
        .get(parent)
        .is_some_and(|o| o.is_house())
        .then_some(parent)
}

/// `LockHelper.GetLockCode(wo)`.
fn lock_helper_get_lock_code(w: &World, wo: ObjectGuid) -> Option<String> {
    empyrean_world::world_objects::lock::get_lock_code(obj(w, wo))
}

/// `LockHelper.GetResistLockpick(wo)`.
fn lock_helper_get_resist_lockpick(w: &mut World, wo: ObjectGuid) -> Option<i32> {
    empyrean_world::world_objects::lock::get_resist_lockpick(w, wo)
}

/// `UnlockResults.ToString()`.
fn unlock_results_name(r: empyrean_world::world_objects::lock::UnlockResults) -> &'static str {
    use empyrean_world::world_objects::lock::UnlockResults;
    match r {
        UnlockResults::UnlockSuccess => "UnlockSuccess",
        UnlockResults::PickLockFailed => "PickLockFailed",
        UnlockResults::IncorrectKey => "IncorrectKey",
        UnlockResults::AlreadyUnlocked => "AlreadyUnlocked",
        UnlockResults::CannotBePicked => "CannotBePicked",
        UnlockResults::Open => "Open",
    }
}

/// `@lock.Unlock(unlockerGuid, null, lockCode)`: the `UnlockResults` name.
fn lock_unlock_with_key(
    w: &mut World,
    lock: ObjectGuid,
    unlocker_guid: u32,
    key_code: &str,
) -> &'static str {
    unlock_results_name(empyrean_world::world_objects::lock::lock_unlock_key(
        w,
        lock,
        unlocker_guid,
        None,
        Some(key_code),
    ))
}

/// `@lock.Unlock(unlockerGuid, playerLockpickSkillLvl, ref difficulty)`: the `UnlockResults` name.
fn lock_unlock_with_skill(
    w: &mut World,
    lock: ObjectGuid,
    unlocker_guid: u32,
    skill: u32,
) -> &'static str {
    let mut difficulty = 0;
    unlock_results_name(empyrean_world::world_objects::lock::lock_unlock_lockpick(
        w,
        lock,
        unlocker_guid,
        skill,
        &mut difficulty,
    ))
}

/// `door.Open(opener)`.
fn door_open(w: &mut World, door: ObjectGuid, opener: ObjectGuid) {
    empyrean_world::world_objects::door::open(w, door, opener);
}

/// `EventManager.StartEvent(eventName, source, target: null)`.
fn event_manager_start_event(w: &mut World, event_name: &str, source: Option<ObjectGuid>) -> bool {
    empyrean_world::managers::event_manager::start_event(w, event_name, source, None)
}

/// `EventManager.StopEvent(eventName, source, target: null)`.
fn event_manager_stop_event(w: &mut World, event_name: &str, source: Option<ObjectGuid>) -> bool {
    empyrean_world::managers::event_manager::stop_event(w, event_name, source, None)
}

/// `EventManager.GetEventStatus(eventName)`: the `GameEventState` name (its `ToString()`).
fn event_manager_get_event_status(w: &World, event_name: &str) -> String {
    let status = empyrean_world::managers::event_manager::get_event_status(w, event_name);
    status
        .name()
        .map_or_else(|| status.0.to_string(), str::to_owned)
}

/// `player.GenerateNewFace()` (Creature.cs).
fn player_generate_new_face(w: &mut World, player: ObjectGuid) {
    // (the object leaves the store while the construction environment borrows the world; the
    // store has no order, so putting it back changes nothing else)
    let Some(mut o) = w.objects.remove(player) else {
        return;
    };
    CtorEnv::with_world(w, |env| {
        empyrean_world::world_objects::creature::generate_new_face(&mut o, env)
    });
    let _ = w.objects.insert(o);
}

/// `WorldObjectFactory.CreateNewWorldObject(wcid)`: the cached weenie's object under a new dynamic
/// guid (recycled when construction fails), in `World.objects`; `None` without the weenie.
fn world_object_factory_create_new_world_object(w: &mut World, wcid: u32) -> Option<ObjectGuid> {
    let weenie = w.content.get_cached_weenie(wcid)?;
    let guid = guid_manager::new_dynamic_guid(w);
    let Some(wo) = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(
            env,
            Some(weenie),
            guid,
        )
    }) else {
        guid_manager::recycle_dynamic_guid(w, guid);
        return None;
    };
    let guid = wo.guid;
    assert!(w.objects.insert(wo).is_ok(), "fresh dynamic guid");
    empyrean_world::world_objects::creature::post_insert(w, guid);
    Some(guid)
}

/// `Player.MaterialSalvage[(int)materialType]`: the salvage bag wcid of a material.
fn player_material_salvage(material_type: MaterialType) -> Option<u32> {
    let key = i32::try_from(material_type.0).ok()?;
    empyrean_world::world_objects::player_crafting::MATERIAL_SALVAGE
        .get(&key)
        .map(|&wcid| wcid.cast_unsigned())
}

/// A fellowship, by reference (`Player.Fellowship`).
type FellowshipRef = empyrean_world::entity::fellowship::FellowshipRef;

/// `player.Fellowship`.
fn player_fellowship(w: &World, player: ObjectGuid) -> Option<FellowshipRef> {
    empyrean_world::world_objects::player_fellowship::fellowship(w, player)
}

/// `fellowship.QuestManager`, lent to `f` (`FellowshipRef::with_quest_manager`).
fn fellowship_with_quest_manager(
    w: &mut World,
    fellowship: FellowshipRef,
    f: &mut dyn FnMut(&mut World, &mut quest_manager::QuestOwner<'_>),
) {
    fellowship.with_quest_manager(w, |w, qm| {
        f(w, &mut quest_manager::QuestOwner::Fellowship(qm))
    });
}

/// An allegiance, by the guid of its `Allegiance` object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllegianceRef(ObjectGuid);

/// `AllegianceManager.GetAllegiance(player)`.
fn allegiance_manager_get_allegiance(
    w: &mut World,
    player: Option<i_player::IPlayer>,
) -> Option<AllegianceRef> {
    empyrean_world::managers::allegiance_manager::get_allegiance(w, player).map(AllegianceRef)
}

/// `AllegianceManager.Allegiances.Values` (a snapshot, as `ToList()`).
fn allegiance_manager_allegiances(w: &World) -> Vec<AllegianceRef> {
    w.allegiance_manager
        .allegiances
        .keys()
        .map(|&g| AllegianceRef(g))
        .collect()
}

/// `allegiance.MonarchId`.
fn allegiance_monarch_id(w: &World, allegiance: AllegianceRef) -> Option<u32> {
    w.objects
        .get(allegiance.0)
        .and_then(empyrean_world::world_objects::world_object::WorldObject::monarch_id)
}

/// `allegiance.Members.ContainsKey(guid)`.
fn allegiance_members_contains(w: &World, allegiance: AllegianceRef, guid: u32) -> bool {
    empyrean_world::world_objects::allegiance::members(w, allegiance.0)
        .iter()
        .any(|(g, _)| g.full() == guid)
}

/// `allegiance.Monarch.Player.Name`.
fn allegiance_monarch_player_name(w: &World, allegiance: AllegianceRef) -> String {
    let monarch = empyrean_world::world_objects::allegiance::monarch_player_guid(w, allegiance.0);
    let player = player_manager::find_by_guid(w, monarch.full())
        .0
        .expect("ACE: Monarch.Player is null (NullReferenceException)");
    i_player::name(w, player).unwrap_or_default()
}

/// `AllegianceManager.Rebuild(allegiance)`.
fn allegiance_manager_rebuild(w: &mut World, allegiance: AllegianceRef) {
    empyrean_world::managers::allegiance_manager::rebuild(w, Some(allegiance.0));
}

/// `allegiance.ShowInfo()`.
fn allegiance_show_info(w: &World, allegiance: AllegianceRef) {
    empyrean_world::world_objects::allegiance::show_info(w, allegiance.0);
}
