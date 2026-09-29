// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Handlers/CharacterHandler.cs
//! Port of `Source/ACE.Server/Network/Handlers/CharacterHandler.cs`.
//!
//! Each handler reads the payload as ACE does, then makes ACE's account, character and world
//! checks.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{HandlerResult, Payload};
use crate::sessions;
use crate::World;

// ---- character creation: CharacterCreate, CharacterCreateEx, SendCharacterCreateResponse ----

/// `session.SendCharacterError(error)`: `Network.EnqueueSend(new GameMessageCharacterError(error))`.
fn send_character_error(
    w: &mut World,
    session: SessionId,
    error: empyrean_net::enums::CharacterError,
) {
    let msg = crate::network::game_messages::messages::game_message_character_error::game_message_character_error(error);
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
}

// ACE: CharacterHandler.CharacterCreate
pub fn character_create(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    let client_string = message.read_string16l()?;

    let Some(s) = w.sessions.get(session) else {
        return Ok(());
    };
    if s.account.as_deref() != Some(client_string.as_str()) {
        return Ok(());
    }
    let access_level = s.access_level;

    if w.server_manager.shutdown_in_progress {
        send_character_error(
            w,
            session,
            empyrean_net::enums::CharacterError::LogonServerFull,
        );
        return Ok(());
    }

    if w.world_manager.world_status == crate::managers::world_manager::WorldStatusState::Open
        || access_level > empyrean_entity::enums::AccessLevel::Player
    {
        character_create_ex(w, message, session)
    } else {
        send_character_error(
            w,
            session,
            empyrean_net::enums::CharacterError::LogonServerFull,
        );
        Ok(())
    }
}

/// `ConfigManager.Config.Server.Accounts.OverrideCharacterPermissions`: the accounts
/// configuration the authentication store was opened with (as the character restore reads it).
fn override_character_permissions(w: &World) -> bool {
    w.auth
        .lock()
        .accounts_config()
        .override_character_permissions
}

/// `string.ToLowerInvariant()`: the invariant culture's one-to-one case mapping, character by
/// character (a character whose Unicode lowercase is not a single character is kept).
fn to_lower_invariant(s: &str) -> String {
    s.chars()
        .map(|c| {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) => l,
                _ => c,
            }
        })
        .collect()
}

// ACE: CharacterHandler.CharacterCreateEx
#[allow(clippy::too_many_lines)]
fn character_create_ex(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    use empyrean_entity::enums::{AccessLevel, HeritageGroup, WeenieType};

    use crate::factories::player_factory::{self, CreateResult};
    use empyrean_net::enums::CharacterGenerationVerificationResponse as Response;

    let mut character_create_info = empyrean_entity::CharacterCreateInfo::default();
    // `Unpack(message.Payload)`: the payload reader's remaining bytes, read by ACE.Entity's unpacker.
    let rest = message.read_bytes(message.remaining());
    let mut reader = empyrean_entity::BinaryReader::new(&rest);
    if character_create_info.unpack(&mut reader).is_none() {
        // EndOfStreamException: ACE's handler catch logs it; nothing is sent.
        return Err(dereth_protocol::MessageError::UnexpectedEof {
            at: message.position(),
            needed: 1,
            available: 0,
        });
    }
    let name = character_create_info.name.clone().unwrap_or_default();

    // Not ACE's: retail's name limit (V248). The wizard stores at most 32
    // characters and never an empty name, so a name outside that did not come from a real client
    // and is answered `Corrupt`, as an out-of-range template is (V243).
    if !dereth_rules::chargen::name_length_ok(&name) {
        send_character_create_response(
            w,
            session,
            Response::Corrupt,
            empyrean_entity::ObjectGuid::default(),
            "",
        );
        return Ok(());
    }

    if player_factory::property_manager_get_bool(w, "taboo_table") {
        // Not ACE's (retail, V227): the shared matcher, retail's plain mode. On the
        // retail table (every pattern `a`-`z` and `*`) it bans the same words as ACE's regex.
        let table = w.dats.portal_dat().taboo_table();
        if dereth_rules::taboo::contains_taboo_word(
            table,
            &dereth_protocol::cp1252::Cp1252,
            &to_lower_invariant(&name),
        ) {
            send_character_create_response(
                w,
                session,
                Response::NameBanned,
                empyrean_entity::ObjectGuid::default(),
                "",
            );
            return Ok(());
        }
    }

    if player_factory::property_manager_get_bool(w, "creature_name_check")
        && w.content.is_creature_name_in_world_database(&name)
    {
        send_character_create_response(
            w,
            session,
            Response::NameBanned,
            empyrean_entity::ObjectGuid::default(),
            "",
        );
        return Ok(());
    }

    // Not ACE's (a fix, V242): ACE checked the name here too, but that check's callback
    // could only answer NameInUse, not stop the method, so a taken name was answered twice. The one
    // check is the one at save time below, which answers once and does not save.

    if (character_create_info.heritage == HeritageGroup::Olthoi
        || character_create_info.heritage == HeritageGroup::OlthoiAcid)
        && player_factory::property_manager_get_bool(w, "olthoi_play_disabled")
    {
        send_character_create_response(
            w,
            session,
            Response::Pending,
            empyrean_entity::ObjectGuid::default(),
            "",
        );
        return Ok(());
    }

    let access_level = w
        .sessions
        .get(session)
        .map_or(AccessLevel::Player, |s| s.access_level);
    let get_cached_weenie = |w: &World, name: &str| w.content.get_cached_weenie_by_class_name(name);
    // `weenie.WeenieType` on a null weenie: NullReferenceException.
    let weenie_type_of = |weenie: &Option<std::sync::Arc<empyrean_entity::Weenie>>| {
        weenie
            .as_ref()
            .expect("NullReferenceException: weenie")
            .weenie_type
    };

    let mut weenie;
    if override_character_permissions(w) {
        if access_level >= AccessLevel::Developer && access_level <= AccessLevel::Admin {
            weenie = get_cached_weenie(w, "admin");
        } else if access_level >= AccessLevel::Sentinel && access_level <= AccessLevel::Envoy {
            weenie = get_cached_weenie(w, "sentinel");
        } else {
            weenie = get_cached_weenie(w, "human");
        }

        if character_create_info.heritage == HeritageGroup::Olthoi
            && weenie_type_of(&weenie) == WeenieType::Admin
        {
            weenie = get_cached_weenie(w, "olthoiadmin");
        }

        if character_create_info.heritage == HeritageGroup::OlthoiAcid
            && weenie_type_of(&weenie) == WeenieType::Admin
        {
            weenie = get_cached_weenie(w, "olthoiacidadmin");
        }
    } else {
        weenie = get_cached_weenie(w, "human");
    }

    if character_create_info.heritage == HeritageGroup::Olthoi
        && weenie_type_of(&weenie) == WeenieType::Creature
    {
        weenie = get_cached_weenie(w, "olthoiplayer");
    }

    if character_create_info.heritage == HeritageGroup::OlthoiAcid
        && weenie_type_of(&weenie) == WeenieType::Creature
    {
        weenie = get_cached_weenie(w, "olthoiacidplayer");
    }

    if character_create_info.is_sentinel && access_level >= AccessLevel::Sentinel {
        weenie = get_cached_weenie(w, "sentinel");
    }

    if character_create_info.is_admin && access_level >= AccessLevel::Developer {
        weenie = get_cached_weenie(w, "admin");
    }

    if weenie.is_none() {
        weenie = get_cached_weenie(w, "human"); // Default catch-all
    }

    let Some(weenie) = weenie else {
        // If it is STILL null after the above catchall, the database is missing critical data and cannot continue with character creation.
        send_character_create_response(
            w,
            session,
            Response::DatabaseDown,
            empyrean_entity::ObjectGuid::default(),
            "",
        );
        log::error!("Database does not contain the weenie for human (1). Characters cannot be created until the missing weenie is restored.");
        return Ok(());
    };

    let guid = crate::managers::guid_manager::new_player_guid(w);

    let mut weenie_type = weenie.weenie_type;

    // If Database didn't have Sentinel/Admin weenies, alter the weenietype coming in.
    if override_character_permissions(w) {
        if access_level >= AccessLevel::Developer
            && access_level <= AccessLevel::Admin
            && weenie_type != WeenieType::Admin
        {
            weenie_type = WeenieType::Admin;
        } else if access_level >= AccessLevel::Sentinel
            && access_level <= AccessLevel::Envoy
            && weenie_type != WeenieType::Sentinel
        {
            weenie_type = WeenieType::Sentinel;
        }
    }

    let account_id = w.sessions.get(session).map_or(0, |s| s.account_id);
    let (result, mut player) = player_factory::create(
        w,
        &character_create_info,
        weenie,
        guid,
        account_id,
        weenie_type,
    );

    if result != CreateResult::Success {
        if result == CreateResult::ClientServerSkillsMismatch {
            // DIVERGE: ACE's boot reason names ACE's client site; ours names https://dereth.network (brand).
            let boot = crate::network::game_messages::messages::game_message_boot_account::game_message_boot_account(Some(
                " because your client is not the correct version for this server. Please visit https://dereth.network to update to latest client",
            ));
            let now = w.now;
            w.net.terminate(
                session,
                empyrean_net::SessionTerminationReason::ClientVersionIncorrect,
                Some(boot.into_outbound()),
                String::new(),
                now,
            );
            return Ok(());
        }

        send_character_create_response(
            w,
            session,
            Response::Corrupt,
            empyrean_entity::ObjectGuid::default(),
            "",
        );
        return Ok(());
    }

    let char_name = name.clone();
    w.shard.is_character_name_available(
        name,
        Some(Box::new(move |w: &mut World, is_available: bool| {
            if !is_available {
                send_character_create_response(
                    w,
                    session,
                    Response::NameInUse,
                    empyrean_entity::ObjectGuid::default(),
                    "",
                );
                return;
            }

            let possessed_biotas: Vec<empyrean_entity::Biota> = player
                .get_all_possessions(w)
                .into_iter()
                .map(|possession| possession.biota.clone())
                .collect();

            // We must await here --
            let biota = player.player.biota.clone();
            let character = player.character().clone();
            w.shard.add_character_in_parallel(
                biota,
                possessed_biotas,
                character,
                Some(Box::new(move |w: &mut World, save_success: bool| {
                    if !save_success {
                        send_character_create_response(
                            w,
                            session,
                            Response::DatabaseDown,
                            empyrean_entity::ObjectGuid::default(),
                            "",
                        );
                        return;
                    }

                    // `PlayerManager.AddOfflinePlayer(player)`.
                    crate::managers::player_manager::add_offline_player_biota(
                        w,
                        player.player.biota.clone(),
                    );
                    if let Some(s) = w.sessions.get_mut(session) {
                        // `session.Characters.Add(player.Character)`.
                        s.characters.push(player.character().clone());
                    }

                    send_character_create_response(
                        w,
                        session,
                        Response::Ok,
                        player.player.guid,
                        &char_name,
                    );
                })),
            );
        })),
    );
    Ok(())
}

// The character-select handlers below; the character-creation members (`CharacterCreate`,
// `CharacterCreateEx`, `SendCharacterCreateResponse`) are above.
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{AccessLevel, HeritageGroup};
use empyrean_entity::ObjectGuid;
use empyrean_net::enums::CharacterError;
use empyrean_net::SessionState;

use crate::managers::player_manager::{self, property_manager_get_bool};
use crate::managers::world_manager::{self, WorldStatusState};
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_character_delete::game_message_character_delete;
use crate::network::game_messages::messages::game_message_character_enter_world_server_ready::game_message_character_enter_world_server_ready;
use crate::network::game_messages::messages::game_message_character_list::game_message_character_list;
use crate::network::game_messages::messages::game_message_character_restore::game_message_character_restore;
use crate::sessions::CharacterSummary;

/// `session.Characters.SingleOrDefault(c => c.Id == guid)`: `None` when absent.
///
/// # Panics
/// When two characters share the id (`InvalidOperationException`).
fn single_or_default(characters: &[CharacterSummary], guid: u32) -> Option<usize> {
    let mut found = characters
        .iter()
        .enumerate()
        .filter(|(_, c)| c.id == guid)
        .map(|(i, _)| i);
    let first = found.next();
    assert!(
        found.next().is_none(),
        "Sequence contains more than one matching element"
    );
    first
}

// ACE: CharacterHandler.SendCharacterCreateResponse
pub fn send_character_create_response(
    w: &mut World,
    session: SessionId,
    response: empyrean_net::enums::CharacterGenerationVerificationResponse,
    guid: empyrean_entity::ObjectGuid,
    char_name: &str,
) {
    let msg = crate::network::game_messages::messages::game_message_character_create_response::game_message_character_create_response(
        response, guid, char_name,
    );
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);
}

// ---- end of the character-creation members ----

// ACE: CharacterHandler.CharacterEnterWorldRequest
pub fn character_enter_world_request(
    w: &mut World,
    _message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    if w.server_manager.shutdown_in_progress {
        sessions::send_character_error(w, session, CharacterError::LogonServerFull);
        return Ok(());
    }

    let access_level = w
        .sessions
        .get(session)
        .map_or(AccessLevel::Player, |s| s.access_level);
    if w.world_manager.world_status == WorldStatusState::Open || access_level > AccessLevel::Player
    {
        enqueue_send(
            w,
            session,
            game_message_character_enter_world_server_ready(),
        );
    } else {
        sessions::send_character_error(w, session, CharacterError::LogonServerFull);
    }
    Ok(())
}

// ACE: CharacterHandler.CharacterEnterWorld
/// Checks the request and hands the character to `WorldManager.PlayerEnterWorld`.
pub fn character_enter_world(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    let guid = message.read_u32()?;

    let client_string = message.read_string16l()?;

    if w.server_manager.shutdown_in_progress {
        sessions::send_character_error(w, session, CharacterError::LogonServerFull);
        return Ok(());
    }

    let Some(s) = w.sessions.get(session) else {
        return Ok(());
    };

    if s.account.as_deref() != Some(client_string.as_str()) {
        sessions::send_character_error(w, session, CharacterError::EnterGameCharacterNotOwned);
        return Ok(());
    }

    let Some(index) = single_or_default(&s.characters, guid) else {
        sessions::send_character_error(w, session, CharacterError::EnterGameCharacterNotOwned);
        return Ok(());
    };
    let character = s.characters[index].clone();

    if character.is_deleted || character.delete_time > 0 {
        sessions::send_character_error(w, session, CharacterError::EnterGameCharacterNotOwned);
        return Ok(());
    }

    if player_manager::get_online_player(w, guid).is_some() {
        // If this happens, it could be that the previous session for this Player terminated in a way that didn't transfer the player to offline via PlayerManager properly.
        sessions::send_character_error(w, session, CharacterError::EnterGameCharacterInWorld);
        return Ok(());
    }

    let Some(offline_player) = player_manager::get_offline_player(w, guid) else {
        // This would likely only happen if the account tried to log in a character that didn't exist.
        sessions::send_character_error(w, session, CharacterError::EnterGameGeneric);
        return Ok(());
    };

    if offline_player.is_deleted(w) || offline_player.is_pending_deletion(w) {
        sessions::send_character_error(w, session, CharacterError::EnterGameCharacterNotOwned);
        return Ok(());
    }

    let heritage = offline_player.heritage();
    if (heritage == Some(HeritageGroup::Olthoi.0) || heritage == Some(HeritageGroup::OlthoiAcid.0))
        && property_manager_get_bool(w, "olthoi_play_disabled")
    {
        sessions::send_character_error(w, session, CharacterError::EnterGameCouldntPlaceCharacter);
        return Ok(());
    }

    if let Some(s) = w.sessions.get_mut(session) {
        s.init_session_for_world_login();
    }

    sessions::set_state(w, session, SessionState::WorldConnected);

    world_manager::player_enter_world(w, session, character);
    Ok(())
}

// ACE: CharacterHandler.CharacterLogOff
pub fn character_log_off(
    w: &mut World,
    _message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    sessions::log_off_player(w, session, false);
    Ok(())
}

// ACE: CharacterHandler.CharacterDelete
/// Marks the character in `characterSlot` for deletion in `char_delete_time` seconds.
///
/// Not ACE's (a fix, V304): a slot outside the list (the client sends -1 when
/// it cannot find the character) is refused with `CharacterError::Delete`, as the other refused
/// deletes are. ACE's list indexer threw, and the client's delete was never answered.
pub fn character_delete(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    let client_string = message.read_string16l()?;
    let character_slot = message.read_u32()?;

    if w.server_manager.shutdown_in_progress {
        sessions::send_character_error(w, session, CharacterError::Delete);
        return Ok(());
    }

    let Some(s) = w.sessions.get(session) else {
        return Ok(());
    };

    if w.world_manager.world_status == WorldStatusState::Closed
        && s.access_level < AccessLevel::Advocate
    {
        sessions::send_character_error(w, session, CharacterError::LogonServerFull);
        return Ok(());
    }

    if s.account.as_deref() != Some(client_string.as_str()) {
        sessions::send_character_error(w, session, CharacterError::Delete);
        return Ok(());
    }

    // `(int)characterSlot`, then the list indexer.
    let slot: i32 = character_slot.cs_cast();
    let Some(index) = usize::try_from(slot)
        .ok()
        .filter(|&i| i < s.characters.len())
    else {
        sessions::send_character_error(w, session, CharacterError::Delete);
        return Ok(());
    };
    // `if (character == null)`: list entries are never null.
    let character_id = s.characters[index].id;

    let offline_player_ok =
        player_manager::get_offline_player(w, character_id).is_some_and(|offline_player| {
            !offline_player.is_deleted(w) && !offline_player.is_pending_deletion(w)
        });
    if !offline_player_ok {
        sessions::send_character_error(w, session, CharacterError::Delete);
        return Ok(());
    }

    enqueue_send(w, session, game_message_character_delete());

    let char_restore_time =
        crate::managers::property_manager::get_long(w, "char_delete_time", 3600, true).item;
    let unix_time = w.now.unix_time;
    let Some(s) = w.sessions.get_mut(session) else {
        return Ok(());
    };
    let character = &mut s.characters[index];
    #[allow(clippy::cast_precision_loss)] // C#'s implicit long -> double
    let delete_time: u64 = (unix_time + char_restore_time as f64).cs_cast();
    character.delete_time = delete_time;
    character.is_deleted = false;

    let snapshot = character.clone();
    w.shard.save_character(
        snapshot,
        Some(Box::new(move |w: &mut World, result: bool| {
            if result {
                if let Some(s) = w.sessions.get(session) {
                    let msg = game_message_character_list(w, &s.characters, s);
                    enqueue_send(w, session, msg);
                }

                player_manager::handle_player_delete(w, character_id);
            } else {
                sessions::send_character_error(w, session, CharacterError::Delete);
            }
        })),
    );
    Ok(())
}

// ACE: CharacterHandler.CharacterRestore
/// Takes a character out of its pending deletion, if its name is still free.
#[allow(clippy::if_same_then_else)] // ACE's two `+` branches, kept as written
pub fn character_restore(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    let guid = message.read_u32()?;

    if w.server_manager.shutdown_in_progress {
        sessions::send_character_error(w, session, CharacterError::EnterGameCouldntPlaceCharacter);
        return Ok(());
    }

    let Some(s) = w.sessions.get(session) else {
        return Ok(());
    };

    if w.world_manager.world_status == WorldStatusState::Closed
        && s.access_level < AccessLevel::Advocate
    {
        sessions::send_character_error(w, session, CharacterError::LogonServerFull);
        return Ok(());
    }

    let Some(index) = single_or_default(&s.characters, guid) else {
        return Ok(());
    };
    let character = &s.characters[index];

    // `Time.GetUnixTime() > character.DeleteTime`: the ulong converts to double.
    let delete_time: f64 = character.delete_time.cs_cast();
    if w.now.unix_time > delete_time || character.is_deleted {
        sessions::send_character_error(w, session, CharacterError::EnterGameCharacterNotOwned);
        return Ok(());
    }

    let name = character.name.clone();
    w.shard.is_character_name_available(
        name,
        Some(Box::new(move |w: &mut World, is_available: bool| {
            if is_available {
                // The session's own `Character` object, as ACE's closure holds it.
                let Some(snapshot) = w.sessions.get_mut(session).and_then(|s| {
                    let character = s.characters.iter_mut().find(|c| c.id == guid)?;
                    character.delete_time = 0;
                    character.is_deleted = false;
                    Some(character.clone())
                }) else {
                    return;
                };

                w.shard.save_character(
                    snapshot.clone(),
                    Some(Box::new(move |w: &mut World, result: bool| {
                        let mut name = snapshot.name.clone();

                        let override_character_permissions = w.auth.lock().accounts_config().override_character_permissions;
                        let access_level = w.sessions.get(session).map_or(AccessLevel::Player, |s| s.access_level);
                        if override_character_permissions && access_level > AccessLevel::Advocate {
                            name = format!("+{name}");
                        } else if !override_character_permissions && snapshot.is_plussed {
                            name = format!("+{name}");
                        }

                        if result {
                            enqueue_send(w, session, game_message_character_restore(guid, &name, 0));
                        } else {
                            send_character_create_response(w, session, empyrean_net::enums::CharacterGenerationVerificationResponse::Corrupt, ObjectGuid::default(), "");
                        }
                    })),
                );
            } else {
                send_character_create_response(w, session, empyrean_net::enums::CharacterGenerationVerificationResponse::NameInUse, ObjectGuid::default(), "");
            }
        })),
    );
    Ok(())
}
