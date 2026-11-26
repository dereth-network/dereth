// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Handlers/DDDHandler.cs
//! Port of `Source/ACE.Server/Network/Handlers/DDDHandler.cs`: the client's answer to the DDD
//! interrogation, its end-of-patching notice, and its requests for single dat files.

use dereth_protocol::MessageError;
use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_common::dotnet::format;
use empyrean_dat::DatDatabaseType;
use empyrean_entity::enums::ChatMessageType;
use empyrean_net::{GameMessageGroup, SessionId, SessionTerminationReason};

use crate::managers::bulk_patch;
use crate::managers::dat_overlay;
use crate::managers::ddd_manager::{self, dat_file_type, HI_FI_STRING_AS_INT};
use crate::managers::launch_base;
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_popup_string::game_event_popup_string;
use crate::network::game_messages::game_message::{enqueue_send, enqueue_send_many, GameMessage};
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::network::game_messages::messages::game_message_boot_account::game_message_boot_account;
use crate::network::game_messages::messages::game_message_ddd_begin_ddd::game_message_ddd_begin_ddd;
use crate::network::game_messages::messages::game_message_ddd_end_ddd::game_message_ddd_end_ddd;
use crate::network::game_messages::messages::game_message_ddd_error_message::game_message_ddd_error_message;
use crate::network::game_messages::messages::game_message_ddd_interrogation::game_message_ddd_interrogation;
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
/// Not ACE: why a client whose data files are not those of a world on the February 2005 set is
/// booted (the account-booted message puts it after "because").
pub const PRE_TOD_OTHER_CLIENT_REASON: &str =
    " because this world plays the February 2005 data files, and your client's are not those files";

/// Not ACE: why a client whose data files are not those of a world on 1999 files is booted, the
/// world's `files` named (the account-booted message puts it after "because").
#[must_use]
pub fn other_files_reason(files: &str) -> String {
    format!(" because this world plays {files}, and your client's are not those files")
}

/// Not ACE: why a client holding other base data files than a world's data overlay was made
/// against is booted (the account-booted message puts it after "because").
pub const OVERLAY_OTHER_BASE_REASON: &str = "this world's data overlay was made against other data files than your client's. Install the data files the world names";

/// Not ACE: why a client that keeps no data overlay of its own, missing iterations of a world with
/// a data overlay, is booted when its own files are not patched (the account-booted message puts it
/// after "because").
pub const OVERLAY_NOT_KEPT_REASON: &str = "this world changes the game's data files, and your client does not take a world's changes. Play it with Dereth, which the world patches";

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

    let client_language = message.read_u32()?; // the client's language

    // `message.Payload.ReadCAllIterationList()`: the structure reader runs over the rest of the
    // payload, which the handler reads no further.
    let rest = message.read_bytes(message.remaining());
    let iters_with_keys =
        read_c_all_iteration_list(&mut BinaryReader::new(&rest)).map_err(|e| read_error(&e))?;
    //var ItersWithoutKeys = message.Payload.ReadCAllIterationList(); // Not seen this populated in any pcap.
    // message.Payload.ReadUInt32(); // the flags - We don't need this

    let dats = std::sync::Arc::clone(&w.dats);
    // DIVERGE (V437): the overlay extension. A client that keeps overlays sets a flag after the
    // lists and names the bases it holds; ACE reads neither. Over a world with a data overlay, such
    // a client is patched to the world (when `[dat_overlay] patching` is on) and checked first to
    // hold the bases the overlay was made against.
    let extension = {
        let mut body = client_language.to_le_bytes().to_vec();
        body.extend_from_slice(&rest);
        dereth_protocol::read_body::<dereth_protocol::admin::DddInterrogationResponse>(&body)
            .ok()
            .filter(|r| {
                r.flags & dereth_protocol::admin::DddInterrogationResponse::FLAG_OVERLAY != 0
            })
    };
    // DIVERGE (V566): a client that asks what its items make is told the server answers it.
    let recipes_asked = {
        let mut body = client_language.to_le_bytes().to_vec();
        body.extend_from_slice(&rest);
        dereth_protocol::read_body::<dereth_protocol::admin::DddInterrogationResponse>(&body)
            .is_ok_and(|r| {
                r.flags & dereth_protocol::admin::DddInterrogationResponse::FLAG_RECIPES != 0
            })
    };
    if recipes_asked {
        enqueue_send(
            w,
            session,
            GameMessage::from_proto(
                GameMessageOpcode::ExtensionOffer,
                GameMessageGroup::DatabaseQueue,
                &dereth_protocol::admin::ExtensionOffer {
                    flags: dereth_protocol::admin::ExtensionOffer::RECIPES,
                },
            ),
        );
    }
    let overlay_world = !dat_overlay::overlaid(&dats).is_empty();
    let overlay_patching = overlay_world
        && extension.is_some()
        && empyrean_common::config_manager::ConfigManager::config()
            .dat_overlay
            .patching;
    let mut client_bases = extension
        .as_ref()
        .map(dat_overlay::ClientBases::of)
        .unwrap_or_default();
    // DIVERGE (V520): a client that keeps overlays and holds the earlier portal file of a step up
    // to this world's is patched up to the world's file, beneath the world's overlay when it has
    // one and patches it.
    let step = extension
        .as_ref()
        .and_then(|r| launch_base::for_client(&dats, r))
        .filter(|_| !overlay_world || overlay_patching);
    client_bases.step = step;
    // What the step's patch needs is built when the first client on its earlier file asks, on a
    // world that patches no other client.
    if let Some(step) = step {
        launch_base::prepare(step, &dats, &mut w.ddd_manager);
    }
    if overlay_patching {
        if let Some(why) = dat_overlay::base_mismatch(&dats, &client_bases) {
            log::info!(
                "[DDD] client {} holds other data files than this world's overlay was made against: {why}",
                account(w, session)
            );
            // The manifest first, so the client can say which files it needs.
            enqueue_send(
                w,
                session,
                dat_overlay::game_message_overlay_manifest(&dat_overlay::manifest(
                    &dats,
                    &client_bases,
                )),
            );
            let boot =
                game_message_boot_account(Some(&format!(" because {OVERLAY_OTHER_BASE_REASON}")));
            terminate(
                w,
                session,
                SessionTerminationReason::DATsPatchingDisabled,
                boot,
            );
            return Ok(());
        }
    }
    // DIVERGE (V437): a client that keeps a folder for each world a server serves answers with
    // the lists of the world it played at this address last, and names it. Named another world
    // than this one, it is told this world's name (the manifest; one naming no world from a world
    // without an overlay) and asked again, once a session, so the lists it then answers with are
    // this world's own.
    if let Some(theirs) = extension
        .as_ref()
        .and_then(dereth_protocol::admin::DddInterrogationResponse::world)
    {
        let ours = empyrean_dat::data_overlay::world_hash(
            &dats.portal_dat().overlay_world_key().unwrap_or_default(),
        );
        let asked = w
            .sessions
            .get(session)
            .is_some_and(|s| s.overlay_asked_again);
        if theirs != ours && !asked {
            log::info!(
                "[DDD] client {} answered for another world than this one; it is told this                  world's name and asked again",
                account(w, session)
            );
            if let Some(s) = w.sessions.get_mut(session) {
                s.overlay_asked_again = true;
            }
            enqueue_send(
                w,
                session,
                dat_overlay::game_message_overlay_manifest(&dat_overlay::manifest(
                    &dats,
                    &client_bases,
                )),
            );
            let again = game_message_ddd_interrogation(w);
            enqueue_send(w, session, again);
            return Ok(());
        }
    }
    // DIVERGE: on the dat set from before Throne of Destiny there is no language file (the
    // portal file holds the strings) and no patching (those files have no iteration lists to
    // patch from): the portal and cell iterations are compared, the language list is not, and a
    // client missing iterations is booted as when patching is off.
    let pre_tod = dats.portal_dat().container_era() == dereth_primitives::ContainerEra::Classic;
    // DIVERGE (V437): a client that keeps overlays is patched to a world with a data overlay,
    // before Throne of Destiny too (the overlay carries its own iteration list).
    let enable_dat_patching =
        (enable_dat_patching && !pre_tod) || overlay_patching || step.is_some();
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
                if pre_tod {
                    continue;
                }
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

    if pre_tod
        && !overlay_patching
        && step.is_none()
        && (client_has_extra_iterations || client_is_missing_iterations)
    {
        // DIVERGE: a client whose files are not the February 2005 set a world plays is told
        // which files the world plays, rather than that its own are newer than expected or
        // incomplete. On a world on 1999 files, they are the world's own, and a client keeping no
        // overlays that holds the files the world's overlay was made against is told which client
        // the world patches (V437).
        let reason = match launch_base::world_files(&dats) {
            None => PRE_TOD_OTHER_CLIENT_REASON.to_owned(),
            Some(_)
                if extension.is_none()
                    && launch_base::holds_overlay_base(
                        &dats,
                        client_portal_dat_int_set.iterations,
                        client_cell_dat_int_set.iterations,
                    ) =>
            {
                format!(" because {OVERLAY_NOT_KEPT_REASON}")
            }
            Some(files) => other_files_reason(&files),
        };
        let boot = game_message_boot_account(Some(&reason));
        let reason = if client_has_extra_iterations {
            SessionTerminationReason::DATsNewerThanServer
        } else {
            SessionTerminationReason::DATsPatchingDisabled
        };
        terminate(w, session, reason, boot);
    } else if client_has_extra_iterations {
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
        // DIVERGE (V437): before Throne of Destiny the language reads are the portal file's, and
        // the client's language list is its own later file's: nothing of it is patched.
        let language_set = if pre_tod {
            let n = language_iteration;
            CMostlyConsecutiveIntSet {
                iterations: n,
                ints: vec![-n, 1],
            }
        } else {
            client_language_dat_int_set.clone()
        };
        let r = ddd_manager::get_missing_iterations(
            &w.ddd_manager,
            &client_portal_dat_int_set,
            &client_cell_dat_int_set,
            &language_set,
            &client_high_res_dat_int_set,
        );
        let total_missing_iterations = r.total_missing_iterations;
        let mut total_file_size = r.total_file_size;
        let mut missing_iterations = r.iterations;
        // DIVERGE (V520): the step's records, in its last revision.
        if let Some(step) = step {
            total_file_size = total_file_size.wrapping_add(launch_base::add_to_patch(
                step,
                &dats,
                &w.ddd_manager,
                &mut missing_iterations,
            ));
        }
        let patch_status_message = if overlay_world {
            // DIVERGE (V437, V438): the manifest ahead of the patch, the world's cell records in
            // it, and the overlay's deletions purged.
            if overlay_patching {
                enqueue_send(
                    w,
                    session,
                    dat_overlay::game_message_overlay_manifest(&dat_overlay::manifest(
                        &dats,
                        &client_bases,
                    )),
                );
                if let Some(cells) = missing_iterations.get(&DatDatabaseType::Cell) {
                    let sizes = w.ddd_manager.dat_file_sizes.get(&DatDatabaseType::Cell);
                    for files in cells.values() {
                        for f in files {
                            let size = sizes.and_then(|s| s.get(f)).map_or(0, |z| {
                                if z.compressed_file_size > 0 {
                                    z.compressed_file_size
                                } else {
                                    z.uncompressed_file_size
                                }
                            });
                            total_file_size = total_file_size.wrapping_add(size.cast_unsigned());
                        }
                    }
                }
            }
            dat_overlay::game_message_ddd_begin_ddd(
                total_file_size,
                &missing_iterations,
                &dat_overlay::deletions(&dats),
                overlay_patching,
            )
        } else {
            game_message_ddd_begin_ddd(
                total_missing_iterations,
                total_file_size,
                &missing_iterations,
            )
        };
        enqueue_send(w, session, patch_status_message);
        let now = w.now.utc;
        // DIVERGE (V437): a client that keeps overlays is sent its records at the overlay's rate.
        let overlay = &empyrean_common::config_manager::ConfigManager::config().dat_overlay;
        let (rate, bytes) = if overlay_patching {
            (overlay.records_per_minute, overlay.bytes_per_second)
        } else {
            (0, 0)
        };
        if let Some(s) = w.sessions.get_mut(session) {
            s.begin_ddd_sent_time = now;
            s.begin_ddd_sent = true;
            s.overlay_records_per_minute = rate;
            s.overlay_bytes_per_second = bytes;
        }

        log::info!(
            "[DDD] client {} informed with BeginDDD payload:\n Total Missing Iterations: {} | Expected Data Transfer Size: {} kB",
            account(w, session),
            total_missing_iterations,
            format(total_file_size / 1024, "N0")
        );

        // DIVERGE (V519): a client that can take the patch as a TCP stream is offered one in place
        // of its records' `0xF7E2`s.
        let bulk = overlay_patching
            && overlay.bulk
            && bulk_patch::port().is_some()
            && extension.as_ref().is_some_and(|r| {
                r.flags & dereth_protocol::admin::DddInterrogationResponse::FLAG_BULK != 0
            });
        let mut bulk_items = Vec::new();

        // DIVERGE (V437): a client that keeps overlays takes the world's cell records in the
        // patch.
        let cell = overlay_patching.then_some((DatDatabaseType::Cell, "CellDat"));
        for (dat_database_type, name) in [
            (DatDatabaseType::Portal, "PortalDat"),
            (DatDatabaseType::Language, "LanguageDat"),
            (DatDatabaseType::HighRes, "HighResDat"),
        ]
        .into_iter()
        .chain(cell)
        {
            let Some(dat_missing_iterations) = missing_iterations.get(&dat_database_type) else {
                continue;
            };
            let db = match dat_database_type {
                DatDatabaseType::Portal => Some(&**dats.portal_dat()),
                DatDatabaseType::Language => Some(&**dats.language_dat()),
                DatDatabaseType::Cell => Some(&**dats.cell_dat()),
                DatDatabaseType::HighRes => dats.high_res_dat(),
            }
            .expect("NullReferenceException: DatManager.HighResDat is null");
            for iteration in dat_missing_iterations.values() {
                let mut files = iteration.clone();
                files.sort_unstable();
                for file_id in files {
                    if db.contains_file(file_id) && bulk {
                        bulk_items.push((file_id, dat_database_type));
                    } else if db.contains_file(file_id) {
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
        if !bulk_items.is_empty() {
            offer_bulk(w, session, bulk_items, &dats);
        }
    } else if client_is_missing_iterations && !enable_dat_patching {
        // DIVERGE (V437): a client that keeps no overlay of its own, missing a world's overlay, is
        // told which client the world patches.
        let msg = if overlay_world && extension.is_none() {
            OVERLAY_NOT_KEPT_REASON.to_owned()
        } else {
            property_manager::get_string(w, "dat_older_warning_msg", "", true).item
        };
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

/// Not ACE (V519): `items` offered to `session` as a TCP stream (`0xF7ED`), with a ticket that
/// goes back to the `0xF7E2` queue unless its client collects it.
fn offer_bulk(
    w: &mut World,
    session: SessionId,
    items: Vec<(u32, DatDatabaseType)>,
    dats: &std::sync::Arc<empyrean_dat::DatManager>,
) {
    let sizes = &w.ddd_manager.dat_file_sizes;
    let mut bytes = 0u64;
    let tickets: Vec<bulk_patch::BulkItem> = items
        .iter()
        .map(|&(id, db)| {
            let z = sizes.get(&db).and_then(|s| s.get(&id)).copied();
            let compress = z.is_some_and(|z| z.compressed_file_size > 0);
            bytes += z.map_or(0, |z| {
                let n = if compress {
                    z.compressed_file_size
                } else {
                    z.uncompressed_file_size
                };
                u64::from(n.cast_unsigned())
            });
            bulk_patch::BulkItem { id, db, compress }
        })
        .collect();
    let who = account(w, session);
    // The ticket opens only from the address the session plays from.
    let address = w.net.session(session).map(|s| s.core.end_point_c2s.ip());
    // Each compressed record goes as the copy the world keeps for every client.
    let records = std::sync::Arc::clone(&w.ddd_manager.overlay_records);
    let offered = bulk_patch::service()
        .zip(address)
        .and_then(|(service, address)| {
            let token =
                service.offer(tickets, std::sync::Arc::clone(dats), records, &who, address)?;
            Some((u32::from(service.port()), token))
        });
    let Some((port, token)) = offered else {
        for (id, db) in items {
            ddd_manager::add_to_queue(w, session, id, db);
        }
        return;
    };
    let records = u32::try_from(items.len()).unwrap_or(u32::MAX);
    let deadline = w.now.utc.add_seconds(bulk_patch::TICKET_SECONDS);
    if let Some(s) = w.sessions.get_mut(session) {
        s.bulk = Some(bulk_patch::BulkPending {
            token,
            deadline,
            claimed: false,
            items,
        });
    }
    enqueue_send(
        w,
        session,
        GameMessage::from_proto(
            GameMessageOpcode::DDD_BulkOffer,
            GameMessageGroup::DatabaseQueue,
            &dereth_protocol::admin::DddBulkOffer {
                port,
                token,
                records,
                bytes,
            },
        ),
    );
    log::info!(
        "[DDD] client {who} offered the patch as a stream on port {port}: {records} records, {} kB",
        bytes / 1024
    );
}

/// Not ACE (V519): `0xF7EE`, a client that did not take or could not finish its stream: what it
/// names as still pending, or the whole offer when it names nothing, goes as `0xF7E2`s. The offer
/// stays until `0xF7EA`, so a long list may come as several declines, and a record goes once.
pub fn ddd_bulk_decline(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    let rest = message.read_bytes(message.remaining());
    let Ok(decline) = dereth_protocol::read_body::<dereth_protocol::admin::DddBulkDecline>(&rest)
    else {
        log::warn!(
            "[DDD] client {} sent a decline that does not read",
            account(w, session)
        );
        return Ok(());
    };
    let Some(pending) = w.sessions.get_mut(session).and_then(|s| s.bulk.as_mut()) else {
        return Ok(());
    };
    // Not claimed now, it never opens.
    bulk_patch::withdraw(&pending.token);
    pending.claimed = true;
    let left = pending.items.len();
    let again = bulk_patch::take_declined(pending, &decline.pending);
    log::info!(
        "[DDD] client {} declined its patch stream: {} of the {left} records left go as 0xF7E2s",
        account(w, session),
        again.len(),
    );
    for (id, db) in again {
        ddd_manager::add_to_queue(w, session, id, db);
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
            // Not ACE (V519): a stream's ticket is done with.
            if let Some(p) = s.bulk.take() {
                bulk_patch::withdraw(&p.token);
            }
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
                .expect("ACE: session.Player is null (NullReferenceException)");
            let first_enter_world_done = w
                .objects
                .get(player)
                .expect("ACE: session.Player is null (NullReferenceException)")
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

/// Not ACE (V566): the least time between one session's recipe queries; one sooner is answered
/// "too soon", with nothing looked up.
pub const RECIPE_QUERY_SECONDS: f64 = 1.0;

/// Not ACE (V566): `0xF7F0`, what the character's items make used on one another. Only the
/// character's own items (in its packs or worn), at most `RecipeQuery::MOST`, and one query a
/// second; the answer names each ordered pair with a recipe, its product, the character's chance
/// now and what a failure destroys. Nothing is changed and nothing else is sent.
pub fn recipe_query(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    use crate::world_objects::player_inventory::{find_object, SearchLocations};
    use dereth_protocol::admin::{RecipeAnswer, RecipeAnswerStatus, RecipeMatch, RecipeQuery};
    use empyrean_entity::ObjectGuid;

    let rest = message.read_bytes(message.remaining());
    let Ok(query) = dereth_protocol::read_body::<RecipeQuery>(&rest) else {
        log::warn!(
            "[DDD] client {} sent a recipe query that does not read",
            account(w, session)
        );
        return Ok(());
    };
    let Some(player) = w.sessions.player(session) else {
        return Ok(());
    };
    let answer = |status, matches| RecipeAnswer {
        request: query.request,
        status,
        matches,
    };
    let now = w.now.utc;
    let too_soon = w
        .sessions
        .get(session)
        .and_then(|s| s.last_recipe_query)
        .is_some_and(|t| now < t.add_seconds(RECIPE_QUERY_SECONDS));
    let reply = if too_soon {
        answer(RecipeAnswerStatus::TooSoon, Vec::new())
    } else {
        if let Some(s) = w.sessions.get_mut(session) {
            s.last_recipe_query = Some(now);
        }
        let items: Vec<ObjectGuid> = query
            .items
            .iter()
            .take(RecipeQuery::MOST)
            .map(|i| ObjectGuid::new(*i))
            .collect();
        let held = |w: &World, i: ObjectGuid| {
            i != player
                && find_object(
                    w,
                    player,
                    i,
                    SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
                )
                .result
                    == Some(i)
        };
        if query.items.len() > RecipeQuery::MOST || items.iter().any(|i| !held(w, *i)) {
            answer(RecipeAnswerStatus::NotHeld, Vec::new())
        } else {
            let mut matches = Vec::new();
            for &source in &items {
                for &target in &items {
                    if source == target {
                        continue;
                    }
                    let Some(recipe) =
                        crate::managers::recipe_manager::get_recipe(w, player, source, target)
                    else {
                        continue;
                    };
                    let chance = recipe_chance(w, player, source, target, &recipe);
                    let makes_name = w
                        .content
                        .get_cached_weenie(recipe.success_wcid)
                        .and_then(|x| x.get_name())
                        .unwrap_or_default();
                    let mut fails = 0;
                    if recipe.fail_destroy_source_chance > 0.0 {
                        fails |= RecipeMatch::FAIL_SOURCE;
                    }
                    if recipe.fail_destroy_target_chance > 0.0 {
                        fails |= RecipeMatch::FAIL_TARGET;
                    }
                    matches.push(RecipeMatch {
                        source: source.full(),
                        target: target.full(),
                        recipe: recipe.id,
                        skill: recipe.skill,
                        difficulty: recipe.difficulty,
                        #[allow(clippy::cast_possible_truncation)]
                        chance: chance as f32,
                        makes_wcid: recipe.success_wcid,
                        makes_name,
                        makes_count: recipe.success_amount,
                        fails,
                    });
                }
            }
            answer(RecipeAnswerStatus::Answered, matches)
        }
    };
    enqueue_send(
        w,
        session,
        GameMessage::from_proto(
            GameMessageOpcode::RecipeAnswer,
            GameMessageGroup::DatabaseQueue,
            &reply,
        ),
    );
    Ok(())
}

/// Not ACE (V566): the character's chance at `recipe` now, as `RecipeManager.GetRecipeChance` and
/// `GetTinkerChance` reckon it, read without their side effects: an untrained skill is a chance of
/// 0, and nothing is told to the player.
fn recipe_chance(
    w: &mut World,
    player: empyrean_entity::ObjectGuid,
    source: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    recipe: &empyrean_content::models::world::Recipe,
) -> f64 {
    use crate::managers::recipe_manager as rm;
    use empyrean_entity::enums::{Skill, SkillAdvancementClass};
    if rm::is_tinkering(recipe) {
        return rm::get_tinker_chance(w, player, source, target, recipe).unwrap_or(0.0);
    }
    if !rm::has_difficulty(recipe) {
        return 1.0;
    }
    let Ok(skill) = i32::try_from(recipe.skill) else {
        return 0.0;
    };
    let skill = crate::world_objects::world_object::convert_to_mo_a_skill(w, player, Skill(skill));
    let Some(s) = w
        .objects
        .get_mut(player)
        .and_then(|o| o.get_creature_skill(skill, false))
    else {
        return 0.0;
    };
    let Some(me) = w.objects.get(player) else {
        return 0.0;
    };
    if s.advancement_class(me) < SkillAdvancementClass::Trained {
        return 0.0;
    }
    let lum = u32::try_from(me.lum_aug_skilled_craft()).unwrap_or(0);
    crate::world_objects::skill_check::get_skill_chance_uint(
        s.current(w, player).wrapping_add(lum),
        recipe.difficulty,
        crate::world_objects::skill_check::DEFAULT_FACTOR,
    )
}
