// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Handlers/DDDHandler.cs
//! Port of `Source/ACE.Server/Network/Handlers/DDDHandler.cs`: the client's answer to the DDD
//! interrogation, its end-of-patching notice, and its requests for single dat files.

use dereth_protocol::MessageError;
use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_common::dotnet::format;
use empyrean_dat::DatDatabaseType;
use empyrean_entity::enums::ChatMessageType;
use empyrean_net::{SessionId, SessionTerminationReason};

use crate::managers::ddd_manager::{self, dat_file_type, HI_FI_STRING_AS_INT};
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_popup_string::game_event_popup_string;
use crate::network::game_messages::game_message::{enqueue_send, enqueue_send_many, GameMessage};
use crate::network::game_messages::messages::game_message_boot_account::game_message_boot_account;
use crate::network::game_messages::messages::game_message_ddd_begin_ddd::game_message_ddd_begin_ddd;
use crate::network::game_messages::messages::game_message_ddd_end_ddd::game_message_ddd_end_ddd;
use crate::network::game_messages::messages::game_message_ddd_error_message::game_message_ddd_error_message;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::managers::inbound_message_manager::{HandlerResult, Payload};
use crate::network::structure::c_all_iteration_list::read_c_all_iteration_list;
use crate::network::structure::c_mostly_consecutive_int_set::CMostlyConsecutiveIntSet;
use crate::World;

/// `DDDHandler.Debug`.
pub const DEBUG: bool = false;

/// `Environment.NewLine` on the Windows host ACE runs on.
const NEW_LINE: &str = "\r\n";

/// `session.Account` in an interpolated string (`null` prints nothing).
fn account(w: &World, session: SessionId) -> String {
    w.sessions
        .get(session)
        .and_then(|s| s.account.clone())
        .unwrap_or_default()
}

/// `session.Terminate(reason, message)`.
fn terminate(
    w: &mut World,
    session: SessionId,
    reason: SessionTerminationReason,
    message: GameMessage,
) {
    let now = w.now;
    w.net.terminate(
        session,
        reason,
        Some(message.into_outbound()),
        String::new(),
        now,
    );
}

/// A structure reader's exception as the handlers' `MessageError`.
fn read_error(e: &ReadError) -> MessageError {
    match *e {
        ReadError::EndOfStream {
            at,
            needed,
            available,
        } => MessageError::UnexpectedEof {
            at,
            needed,
            available,
        },
        ReadError::OutputBufferTooSmall { at } | ReadError::NegativeCount { at } => {
            MessageError::InvalidValue {
                field: "CAllIterationList",
                value: at as u64,
            }
        }
    }
}

/// Which dat flags a session carries (`session.DatWarn*`).
#[derive(Clone, Copy)]
enum DatWarn {
    Portal,
    Cell,
    Language,
    HighRes,
}

fn set_dat_warn(w: &mut World, session: SessionId, which: DatWarn) {
    if let Some(s) = w.sessions.get_mut(session) {
        match which {
            DatWarn::Portal => s.dat_warn_portal = true,
            DatWarn::Cell => s.dat_warn_cell = true,
            DatWarn::Language => s.dat_warn_language = true,
            DatWarn::HighRes => s.dat_warn_high_res = true,
        }
    }
}

// ACE: DDDHandler.DDD_InterrogationResponse
/// The client's iteration lists: boot it when its dats are newer than the server's; when they
/// are older, start patching (or boot it when patching is off); otherwise end DDD at once.
pub fn ddd_interrogation_response(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    let enable_dat_patching = empyrean_common::config_manager::ConfigManager::config()
        .ddd
        .enable_dat_patching;
    ddd_interrogation_response_with(w, message, session, enable_dat_patching)
}

/// `DDD_InterrogationResponse` with `ConfigManager.Config.DDD.EnableDATPatching` given.
///
/// Not ACE: split out so a test can take either branch without changing the process-wide
/// configuration that parallel tests share. ACE reads the setting after the list; reading it has
/// no effect, so reading it first changes nothing.
pub fn ddd_interrogation_response_with(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
    enable_dat_patching: bool,
) -> HandlerResult {
    let mut client_is_missing_iterations = false;

    let mut client_has_extra_iterations = false;

    let mut client_portal_dat_int_set = CMostlyConsecutiveIntSet::default();
    let mut client_cell_dat_int_set = CMostlyConsecutiveIntSet::default();
    let mut client_language_dat_int_set = CMostlyConsecutiveIntSet::default();
    let mut client_high_res_dat_int_set = CMostlyConsecutiveIntSet::default();

    let show_dat_warning = property_manager::get_bool(w, "show_dat_warning", false, true).item;

    message.read_u32()?; // the client's language

    // `message.Payload.ReadCAllIterationList()`: the structure reader runs over the rest of the
    // payload, which the handler reads no further.
    let rest = message.read_bytes(message.remaining());
    let iters_with_keys =
        read_c_all_iteration_list(&mut BinaryReader::new(&rest)).map_err(|e| read_error(&e))?;
    //var ItersWithoutKeys = message.Payload.ReadCAllIterationList(); // Not seen this populated in any pcap.
    // message.Payload.ReadUInt32(); // the flags - We don't need this

    let dats = std::sync::Arc::clone(&w.dats);
    let portal_iteration = dats.portal_dat().iteration();
    let cell_iteration = dats.cell_dat().iteration();
    let language_iteration = dats.language_dat().iteration();
    let high_res_iteration = dats
        .high_res_dat()
        .map(empyrean_dat::DatDatabase::iteration);

    for entry in iters_with_keys.lists {
        match entry.dat_file_id {
            1 => {
                // PORTAL or HIGHRES
                if entry.dat_file_type == 0 {
                    // PORTAL
                    client_portal_dat_int_set = entry.list.clone();
                    if entry.list.iterations < portal_iteration {
                        if show_dat_warning {
                            set_dat_warn(w, session, DatWarn::Portal);
                        }

                        client_is_missing_iterations = true;
                    } else if entry.list.iterations > portal_iteration {
                        if show_dat_warning {
                            set_dat_warn(w, session, DatWarn::Portal);
                        }

                        client_has_extra_iterations = true;
                    }
                } else if entry.dat_file_type == HI_FI_STRING_AS_INT {
                    // HIGHRES
                    client_high_res_dat_int_set = entry.list.clone();

                    let Some(high_res_iteration) = high_res_iteration else {
                        continue;
                    };

                    if entry.list.iterations < high_res_iteration {
                        if client_high_res_dat_int_set.iterations == 0 {
                            continue;
                        }

                        if show_dat_warning {
                            set_dat_warn(w, session, DatWarn::HighRes);
                        }

                        client_is_missing_iterations = true;
                    } else if entry.list.iterations > high_res_iteration {
                        if show_dat_warning {
                            set_dat_warn(w, session, DatWarn::HighRes);
                        }

                        client_has_extra_iterations = true;
                    }
                }
            }
            2 => {
                // CELL
                client_cell_dat_int_set = entry.list.clone();
                if entry.list.iterations < cell_iteration {
                    if client_cell_dat_int_set.iterations == 0 {
                        continue;
                    }

                    if show_dat_warning {
                        set_dat_warn(w, session, DatWarn::Cell);
                    }

                    client_is_missing_iterations = true;
                } else if entry.list.iterations > cell_iteration {
                    if show_dat_warning {
                        set_dat_warn(w, session, DatWarn::Cell);
                    }

                    client_has_extra_iterations = true;
                }
            }
            3 => {
                // LANGUAGE
                client_language_dat_int_set = entry.list.clone();
                if entry.list.iterations < language_iteration {
                    if show_dat_warning {
                        set_dat_warn(w, session, DatWarn::Language);
                    }

                    client_is_missing_iterations = true;
                } else if entry.list.iterations > language_iteration {
                    if show_dat_warning {
                        set_dat_warn(w, session, DatWarn::Language);
                    }

                    client_has_extra_iterations = true;
                }
            }
            _ => {}
        }
    }

    if DEBUG {
        let a = account(w, session);
        empyrean_common::console_write_line!(debug: "{a} client_portal.dat:{NEW_LINE}{client_portal_dat_int_set}");
        empyrean_common::console_write_line!(debug: "{a} client_cell_1.dat:{NEW_LINE}{client_cell_dat_int_set}");
        empyrean_common::console_write_line!(debug: "{a} client_Local_English.dat:{NEW_LINE}{client_language_dat_int_set}");
        empyrean_common::console_write_line!(debug: "{a} client_highres.dat:{NEW_LINE}{client_high_res_dat_int_set}");
    }

    let mut log_msg = format!(
        "[DDD] client {} responded to Interrogation:\n client_portal.dat: {} | client_cell_1.dat: {} | client_Local_English.dat: {}",
        account(w, session),
        client_portal_dat_int_set.iterations,
        client_cell_dat_int_set.iterations,
        client_language_dat_int_set.iterations
    );
    if property_manager::get_bool(w, "allow_highres_dat", false, true).item {
        log_msg += &format!(
            "\n client_highres.dat: {}{}",
            client_high_res_dat_int_set.iterations,
            if high_res_iteration.is_none() {
                " (server allows but does not have client_highres.dat to validate)"
            } else {
                ""
            }
        );
    }
    if client_has_extra_iterations {
        log_msg += " | client has more iterations than server, cannot update";
    } else if client_is_missing_iterations {
        log_msg += " | update required";
    } else {
        log_msg += " | no update required";
    }
    if client_is_missing_iterations && !enable_dat_patching {
        log_msg += ", but DAT patching is disabled";
    }
    log::info!("{log_msg}");

    if client_has_extra_iterations {
        let msg = property_manager::get_string(w, "dat_newer_warning_msg", "", true).item;
        // `msg[..^1]`: all but the last character (an empty message throws).
        let mut chars = msg.chars();
        chars
            .next_back()
            .expect("ArgumentOutOfRangeException: msg[..^1] of an empty string");
        let boot = game_message_boot_account(Some(&format!(" because {}", chars.as_str())));
        terminate(
            w,
            session,
            SessionTerminationReason::DATsNewerThanServer,
            boot,
        );
    } else if client_is_missing_iterations && enable_dat_patching {
        let r = ddd_manager::get_missing_iterations(
            &w.ddd_manager,
            &client_portal_dat_int_set,
            &client_cell_dat_int_set,
            &client_language_dat_int_set,
            &client_high_res_dat_int_set,
        );
        let total_missing_iterations = r.total_missing_iterations;
        let total_file_size = r.total_file_size;
        let missing_iterations = r.iterations;
        let patch_status_message = game_message_ddd_begin_ddd(
            total_missing_iterations,
            total_file_size,
            &missing_iterations,
        );
        enqueue_send(w, session, patch_status_message);
        let now = w.now.utc;
        if let Some(s) = w.sessions.get_mut(session) {
            s.begin_ddd_sent_time = now;
            s.begin_ddd_sent = true;
        }

        log::info!(
            "[DDD] client {} informed with BeginDDD payload:\n Total Missing Iterations: {} | Expected Data Transfer Size: {} kB",
            account(w, session),
            total_missing_iterations,
            format(total_file_size / 1024, "N0")
        );

        for (dat_database_type, name) in [
            (DatDatabaseType::Portal, "PortalDat"),
            (DatDatabaseType::Language, "LanguageDat"),
            (DatDatabaseType::HighRes, "HighResDat"),
        ] {
            let Some(dat_missing_iterations) = missing_iterations.get(&dat_database_type) else {
                continue;
            };
            let db = match dat_database_type {
                DatDatabaseType::Portal => Some(&**dats.portal_dat()),
                DatDatabaseType::Language => Some(&**dats.language_dat()),
                _ => dats.high_res_dat(),
            }
            .expect("NullReferenceException: DatManager.HighResDat is null");
            for iteration in dat_missing_iterations.values() {
                let mut files = iteration.clone();
                files.sort_unstable();
                for file_id in files {
                    if db.contains_file(file_id) {
                        //session.Network.EnqueueSend(new GameMessageDDDDataMessage(fileId, DatDatabaseType.Portal));
                        ddd_manager::add_to_queue(w, session, file_id, dat_database_type);
                    } else {
                        log::warn!(
                            "[DDD] DDD_InterrogationResponse: DDDManager.AddToQueue failed: DatManager.{name}.AllFiles does not contain 0x{file_id:08X}"
                        );
                    }
                }
            }
        }
    } else if client_is_missing_iterations && !enable_dat_patching {
        let msg = property_manager::get_string(w, "dat_older_warning_msg", "", true).item;
        let boot =
            game_message_boot_account(Some(&format!(" because {}", msg.trim_end_matches('.'))));
        terminate(
            w,
            session,
            SessionTerminationReason::DATsPatchingDisabled,
            boot,
        );
    } else {
        // client dat files are up to date
        enqueue_send(w, session, game_message_ddd_end_ddd());
    }
    Ok(())
}

// ACE: DDDHandler.DDD_EndDDD
/// The client has patched: answer only when `BeginDDD` was sent.
pub fn ddd_end_ddd(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    // We don't need to reply to this message unless GameMessageDDDBeginDDD was sent.

    if w.sessions.get(session).is_some_and(|s| s.begin_ddd_sent) {
        if let Some(s) = w.sessions.get_mut(session) {
            s.begin_ddd_sent = false;
        }

        enqueue_send(w, session, game_message_ddd_end_ddd());

        if let Some(s) = w.sessions.get_mut(session) {
            s.dat_warn_portal = false;
            s.dat_warn_cell = false;
            s.dat_warn_language = false;
            s.dat_warn_high_res = false;
        }

        log::info!(
            "[DDD] client {} reported it successfully received and patched its DAT files with expected BeginDDD payload",
            account(w, session)
        );
    }
    Ok(())
}

// ACE: DDDHandler.DDD_RequestDataMessage
/// The client asks for one cell-dat file (a landblock, its info, or an indoor cell); with
/// patching off, a first-login client is told so over and over and a logged-in one is booted.
pub fn ddd_request_data_message(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    let enable_dat_patching = empyrean_common::config_manager::ConfigManager::config()
        .ddd
        .enable_dat_patching;
    ddd_request_data_message_with(w, message, session, enable_dat_patching)
}

/// `DDD_RequestDataMessage` with `ConfigManager.Config.DDD.EnableDATPatching` given (not ACE:
/// split out for the same reason as [`ddd_interrogation_response_with`]).
pub fn ddd_request_data_message_with(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
    enable_dat_patching: bool,
) -> HandlerResult {
    let show_dat_warning = property_manager::get_bool(w, "show_dat_warning", false, true).item;

    let qdid_type = message.read_u32()?;
    let qdid_id = message.read_u32()?;

    let patching_note = if enable_dat_patching {
        String::new()
    } else {
        format!(
            "; DAT patching is disabled{}",
            if show_dat_warning {
                " and client will be booted"
            } else {
                ""
            }
        )
    };
    log::info!(
        "[DDD] client {} requested data on 0x{qdid_id:08X} | {}{patching_note}",
        account(w, session),
        dat_file_type::name(qdid_type)
    );

    if !enable_dat_patching {
        if show_dat_warning {
            let msg = property_manager::get_string(w, "dat_older_warning_msg", "", true).item;
            let Some(s) = w.sessions.get_mut(session) else {
                return Ok(());
            };
            let popup_msg = game_event_popup_string(s, &msg);
            let chat_msg = game_message_system_chat(&msg, ChatMessageType::WorldBroadcast);
            let Some(s) = w.sessions.get_mut(session) else {
                return Ok(());
            };
            let transient_msg = game_event_communication_transient_string(s, &msg);

            //var resourceType = message.Payload.ReadUInt32();
            //var dataId = message.Payload.ReadUInt32();
            let error_type = 1u32; // unknown enum... this seems to trigger reattempt request by client.

            let ddd_error_msg = game_message_ddd_error_message(qdid_type, qdid_id, error_type);

            let player = w
                .sessions
                .player(session)
                .expect("NullReferenceException: session.Player is null");
            let first_enter_world_done = w
                .objects
                .get(player)
                .expect("NullReferenceException: session.Player is null")
                .first_enter_world_done();
            if first_enter_world_done {
                // Boot client with msg
                //session.Network.EnqueueSend(new GameMessageBootAccount($"\n{msg}"), dddErrorMsg);
                //session.LogOffPlayer(true);
                let boot = game_message_boot_account(Some(&format!(
                    " because {}",
                    msg.trim_end_matches('.')
                )));
                terminate(
                    w,
                    session,
                    SessionTerminationReason::DATsPatchingDisabled,
                    boot,
                );
            } else {
                // cannot cleanly boot player that hasn't completed first login, client crashes so msg wouldn't be seen, instead spam msgs until server auto boots them or they disconnect.
                enqueue_send_many(
                    w,
                    session,
                    [popup_msg, chat_msg, transient_msg, ddd_error_msg],
                );
            }
        }

        return Ok(());
    }

    let cell_dat = std::sync::Arc::clone(&w.dats);
    let cell_dat = cell_dat.cell_dat();

    // Landblock also needs to send the LandBlockInfo (0xFFFE) file with it...
    if qdid_type == dat_file_type::LAND_BLOCK {
        let qdid_id_fffe = qdid_id.wrapping_sub(1);
        if cell_dat.contains_file(qdid_id_fffe) {
            //session.Network.EnqueueSend(new GameMessageDDDDataMessage(qdid_ID_FFFE, DatDatabaseType.Cell));
            ddd_manager::add_to_queue(w, session, qdid_id_fffe, DatDatabaseType::Cell);
        }
        //else
        //    log.Warn($"[DDD] The server does not have the requested data on 0x{qdid_ID_FFFE:X8} | {qdid_type} to send.");
    }

    if qdid_type == dat_file_type::LAND_BLOCK
        || qdid_type == dat_file_type::LAND_BLOCK_INFO
        || qdid_type == dat_file_type::ENV_CELL
    {
        if cell_dat.contains_file(qdid_id) {
            //session.Network.EnqueueSend(new GameMessageDDDDataMessage(qdid_ID, DatDatabaseType.Cell));
            ddd_manager::add_to_queue(w, session, qdid_id, DatDatabaseType::Cell);
        } else if qdid_type != dat_file_type::LAND_BLOCK_INFO {
            log::warn!(
                "[DDD] DDD_RequestDataMessage: The server does not have the requested data on 0x{qdid_id:08X} | {} to send to client {}.",
                dat_file_type::name(qdid_type),
                account(w, session)
            );
        }
    } else {
        log::warn!(
            "[DDD] DDD_RequestDataMessage: client {} requested data on 0x{qdid_id:08X} | {} which has been ignored.",
            account(w, session),
            dat_file_type::name(qdid_type)
        );
    }
    Ok(())
}
