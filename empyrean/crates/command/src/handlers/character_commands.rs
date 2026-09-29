// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/CharacterCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/CharacterCommands.cs`.
//!
//! The shard calls take callbacks that run later on the world thread (`w.shard`), as ACE's run on
//! its database thread; ACE's `ReaderWriterLockSlim` argument has no counterpart (snapshots).

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{AccessLevel, ChatMessageType, PropertyBool};
use empyrean_net::SessionId;
use empyrean_world::entity::actions::action_chain::ActionChain;
use empyrean_world::entity::actions::i_actor::Actor;
use empyrean_world::entity::i_player::{self, IPlayer};
use empyrean_world::managers::player_manager;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::handler;
use crate::handlers::admin_commands::try_parse_access_level;
use crate::handlers::command_handler_helper;

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let rows: [(CommandHandlerAttribute, NamedHandler); 2] = [
        (
            CommandHandlerAttribute::with_count(
                "set-characteraccess",
                AccessLevel::Admin,
                CommandHandlerFlag::None,
                1,
                "Sets the access level for the character",
                concat!(
                    "charactername (accesslevel)\n",
                    "accesslevel can be a number or enum name\n",
                    "0 = Player | 1 = Advocate | 2 = Sentinel | 3 = Envoy | 4 = Developer | 5 = Admin",
                ),
            ),
            handler!(handle_character_tokenization),
        ),
        (
            CommandHandlerAttribute::with_count(
                "deletecharacter",
                AccessLevel::Admin,
                CommandHandlerFlag::None,
                1,
                "Deletes a character and removes it from players restore list",
                concat!(
                    "[character name]\n",
                    "Given the name of a character, this command deletes that character, booting it if in game, and removes it from character's restore list.  (You can find the name for a character using the @finger command.)\n",
                ),
            ),
            handler!(handle_character_forced_delete),
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

// ACE: CharacterCommands.HandleCharacterTokenization
/// `set-characteraccess charactername (accesslevel)`: sets the access level for the character
/// (a name with spaces is quoted).
pub fn handle_character_tokenization(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let character_name = parameters[0].clone();

    let mut access_level = AccessLevel::Player;
    if parameters.len() > 1 {
        match try_parse_access_level(&parameters[1], true) {
            Some(parsed) => {
                access_level = if parsed.is_defined() {
                    parsed
                } else {
                    AccessLevel::Player
                }
            }
            None => access_level = AccessLevel::Player,
        }
    }

    // Not ACE's (a fix): the character's access is its permission flags,
    // the ones the command checks read and a login sets from the account's level (Admin: IsAdmin;
    // Developer: IsArch; Envoy: IsEnvoy and IsSentinel; Sentinel: IsSentinel; Advocate:
    // IsAdvocate; Player: none), so the command replaces those flags and saves the character.
    // ACE handed the change to a shard call it never implemented, which threw: the command only
    // ever logged an exception.
    let (found, _) = player_manager::find_by_name(w, &character_name);
    let Some(player) = found else {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!("There is no character by the name of {character_name} found in the database. Has it been deleted?"),
            ChatMessageType::Broadcast,
        );
        return;
    };
    let flags: &[PropertyBool] = match access_level {
        AccessLevel::Admin => &[PropertyBool::IsAdmin],
        AccessLevel::Developer => &[PropertyBool::IsArch],
        AccessLevel::Envoy => &[PropertyBool::IsEnvoy, PropertyBool::IsSentinel],
        AccessLevel::Sentinel => &[PropertyBool::IsSentinel],
        AccessLevel::Advocate => &[PropertyBool::IsAdvocate],
        _ => &[],
    };
    for flag in [
        PropertyBool::IsAdmin,
        PropertyBool::IsArch,
        PropertyBool::IsEnvoy,
        PropertyBool::IsSentinel,
        PropertyBool::IsAdvocate,
    ] {
        if flags.contains(&flag) {
            i_player::set_property(w, player, flag, true);
        } else {
            i_player::remove_property(w, player, flag);
        }
    }
    i_player::save_biota_to_database(w, player, true);

    let article_a_or_an = if access_level == AccessLevel::Advocate
        || access_level == AccessLevel::Admin
        || access_level == AccessLevel::Envoy
    {
        "an"
    } else {
        "a"
    };
    command_handler_helper::write_output_info(
        w,
        session,
        &format!(
            "Character {character_name} has been made {article_a_or_an} {}.",
            access_level.name().unwrap_or("")
        ),
        ChatMessageType::Broadcast,
    );
}

/// `$"... {(isOnline ? "booted and " : "")}deleted character {foundPlayer.Name} (0x{foundPlayer.Guid})."`.
fn delete_result(
    is_online: bool,
    name: &str,
    guid: empyrean_entity::ObjectGuid,
    success: bool,
) -> String {
    if success {
        format!(
            "Successfully {}deleted character {name} (0x{guid}).",
            if is_online { "booted and " } else { "" }
        )
    } else {
        format!(
            "Unable to {}delete character {name} (0x{guid}) due to PlayerManager failure.",
            if is_online { "boot and " } else { "" }
        )
    }
}

// ACE: CharacterCommands.HandleCharacterForcedDelete
/// `deletecharacter [character name]`: deletes a character and removes it from the player's
/// restore list, booting it if it is in game.
pub fn handle_character_forced_delete(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let character_name = parameters.join(" ");

    let (found_player, is_online) = player_manager::find_by_name(w, &character_name);

    let Some(found_player) = found_player else {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!("There is no character named {character_name} in the database."),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let found_name = i_player::name(w, found_player).unwrap_or_default();
    let found_guid = found_player.guid();

    if let (true, IPlayer::Online(player)) = (is_online, found_player) {
        let delete_time: u64 = w.now.unix_time.cs_cast();
        let character_id = {
            let p = w
                .objects
                .get_mut(player)
                .and_then(|o| o.player.as_mut())
                .expect("NullReferenceException: player");
            let character = p
                .player
                .character
                .as_mut()
                .expect("NullReferenceException: player.Character");
            character.delete_time = delete_time;
            character.is_deleted = true;
            p.player_database.character_changes_detected = true;
            character.id
        };
        let player_session = player_manager::player_session(w, player)
            .expect("NullReferenceException: player.Session");
        empyrean_world::sessions::log_off_player(w, player_session, true);
        player_manager::handle_player_delete(w, character_id);

        // Not ACE's (a fix): the logout only starts here (the player
        // goes offline after the logout motion), and its end completes the deletion (the
        // session's check for deleted characters processes it), so the command reports success.
        // ACE processed the deletion at once, found no offline player yet, and reported a
        // "PlayerManager failure" for a deletion that then completed.
        command_handler_helper::write_output_info(
            w,
            session,
            &delete_result(is_online, &found_name, found_guid, true),
            ChatMessageType::Broadcast,
        );
    } else {
        let existing_char_id = found_guid.full(); //DatabaseManager.Shard.BaseDatabase.GetCharacterStubByName(foundPlayer.Name).Id;

        w.shard.get_character(
            existing_char_id,
            Some(Box::new(move |w: &mut World, character| {
                if let Some(mut character) = character {
                    character.delete_time = w.now.unix_time.cs_cast();
                    character.is_deleted = true;
                    let character_id = character.id;
                    w.shard.save_character(
                        character,
                        Some(Box::new(move |w: &mut World, result: bool| {
                            if result {
                                let mut delete_offline_chain = ActionChain::new();
                                delete_offline_chain.add_action(Actor::World, move |w| player_manager::handle_player_delete(w, character_id));
                                delete_offline_chain.add_delay_for_one_tick(w);
                                delete_offline_chain.add_action(Actor::World, move |w| {
                                    let success = player_manager::process_deleted_player(w, character_id);
                                    command_handler_helper::write_output_info(
                                        w,
                                        session,
                                        &delete_result(is_online, &found_name, found_guid, success),
                                        ChatMessageType::Broadcast,
                                    );
                                });
                                delete_offline_chain.enqueue_chain(w);
                            } else {
                                command_handler_helper::write_output_info(
                                    w,
                                    session,
                                    &format!(
                                        "Unable to {}delete character {found_name} (0x{found_guid}) due to shard database SaveCharacter failure.",
                                        if is_online { "boot and " } else { "" }
                                    ),
                                    ChatMessageType::Broadcast,
                                );
                            }
                        })),
                    );
                } else {
                    command_handler_helper::write_output_info(
                        w,
                        session,
                        &format!(
                            "Unable to {}delete character {found_name} (0x{found_guid}) due to shard database GetCharacter failure.",
                            if is_online { "boot and " } else { "" }
                        ),
                        ChatMessageType::Broadcast,
                    );
                }
            })),
        );
    }
}
