// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Handlers/AuthenticationHandler.cs
//! Port of `Source/ACE.Server/Network/Handlers/AuthenticationHandler.cs`.
//!
//! The world half. empyrean-net parses the login request (`HandleLoginRequest`) and raises
//! `Event::LoginRequest`; the world then runs [`do_login`] (the account lookup and auto-creation)
//! and [`account_select_callback`], whose transport checks and `ConnectRequest` are
//! `ServerNet::account_select_callback`, and whose account checks end in `accept_login` or a
//! termination. When the handshake completes, `Event::ConnectResponse` runs
//! [`handle_connect_response`], which sends the character list.
//!
//! DIVERGE: ACE runs `DoLogin` as a `Task` on the thread pool; here it runs on the world thread
//! when the world hears of the request, at the top of the next iteration.

use std::net::SocketAddr;
use std::panic::{catch_unwind, AssertUnwindSafe};

use empyrean_entity::enums::AccessLevel;
use empyrean_net::enums::CharacterError;
use empyrean_net::{
    AccountSelect, NetAuthType, PacketInboundLoginRequest, SessionId, SessionState,
    SessionTerminationReason,
};
use empyrean_store::models::auth::Account;

use crate::managers::player_manager::{self, property_manager_get_bool};
use crate::managers::world_manager::WorldStatusState;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_account_banned::game_message_account_banned;
use crate::network::game_messages::messages::game_message_boot_account::game_message_boot_account;
use crate::network::game_messages::messages::game_message_character_error::game_message_character_error;
use crate::network::game_messages::messages::game_message_character_list::game_message_character_list;
use crate::network::game_messages::messages::game_message_ddd_interrogation::game_message_ddd_interrogation;
use crate::network::game_messages::messages::game_message_server_name::game_message_server_name;
use crate::sessions::{self, CharacterSummary};
use crate::World;

// ACE: AuthenticationHandler.DefaultAuthTimeout
/// Seconds until an authentication request will timeout/expire.
pub const DEFAULT_AUTH_TIMEOUT: i32 = 15;

// ACE: AuthenticationHandler.DoLogin
/// Finds the account (creating it when the configuration allows), then runs
/// [`account_select_callback`]; an exception there terminates the session.
#[allow(clippy::collapsible_if)] // ACE's nested ifs, kept as written
pub fn do_login(
    w: &mut World,
    session: SessionId,
    end_point_c2s: SocketAddr,
    login_request: &PacketInboundLoginRequest,
) {
    let mut account = w.auth.lock().get_account_by_name(&login_request.account);

    if account.is_none() {
        if login_request.net_auth_type == NetAuthType::AccountPassword
            && login_request.password.as_deref() != Some("")
        {
            let accounts = w.auth.lock().accounts_config().clone();
            if accounts.allow_auto_account_creation {
                // no account, dynamically create one
                if w.world_manager.world_status == WorldStatusState::Open {
                    log::info!("Auto creating account for: {}", login_request.account);
                } else {
                    log::debug!("Auto creating account for: {}", login_request.account);
                }

                let mut access_level =
                    AccessLevel(i32::try_from(accounts.default_access_level).unwrap_or(-1));

                if !AccessLevel::ALL.contains(&access_level) {
                    access_level = AccessLevel::Player;
                }

                if w.auth.auto_promote_next_account_to_admin() {
                    access_level = AccessLevel::Admin;
                    w.auth.set_auto_promote_next_account_to_admin(false);
                    log::warn!(
                        "Automatically setting account AccessLevel to Admin for account \"{}\" because there are no admin accounts in the current database.",
                        login_request.account
                    );
                }

                // DIVERGE: C#'s `ToLower()` uses the current culture; this is the invariant
                // lower-casing.
                let created = w.auth.lock().create_account(
                    &login_request.account.to_lowercase(),
                    login_request.password.as_deref().unwrap_or_default(),
                    access_level,
                    end_point_c2s.ip(),
                );
                match created {
                    Ok(created) => account = Some(created),
                    Err(e) => {
                        // In ACE the exception escapes the task before the `try`, so the login is
                        // never answered and the session times out.
                        log::error!(
                            "DoLogin: creating account {} threw: {e}",
                            login_request.account
                        );
                        return;
                    }
                }
            }
        }
    }

    log::debug!(
        "new client connected: {}. setting session properties",
        login_request.account
    );
    let result = catch_unwind(AssertUnwindSafe(|| {
        account_select_callback(w, account, session, end_point_c2s, login_request)
    }));
    if let Err(panic) = result {
        let message = panic
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        log::error!("Error in HandleLoginRequest trying to find the account. {message}");
        terminate(
            w,
            session,
            SessionTerminationReason::AccountSelectCallbackException,
            None,
            String::new(),
        );
    }
}

// ACE: AuthenticationHandler.AccountSelectCallback
/// empyrean-net's half runs first (seeds, client version, the `ConnectRequest`, no-password and
/// server-tracker logins); then the account checks, in ACE's order.
pub fn account_select_callback(
    w: &mut World,
    account: Option<Account>,
    session: SessionId,
    end_point_c2s: SocketAddr,
    login_request: &PacketInboundLoginRequest,
) {
    let now = w.now;
    if w.net.account_select_callback(session, now) == AccountSelect::Terminated {
        return;
    }

    let Some(mut account) = account else {
        terminate(
            w,
            session,
            SessionTerminationReason::NotAuthorizedAccountNotFound,
            Some(game_message_character_error(
                CharacterError::AccountDoesntExist,
            )),
            String::new(),
        );
        return;
    };

    if !property_manager_get_bool(w, "account_login_boots_in_use")
        && w.net.find_by_account(&account.account_name).is_some()
    {
        terminate(
            w,
            session,
            SessionTerminationReason::AccountInUse,
            Some(game_message_character_error(CharacterError::Logon)),
            String::new(),
        );
        return;
    }

    let world_open = w.world_manager.world_status == WorldStatusState::Open;

    if login_request.net_auth_type == NetAuthType::AccountPassword {
        let password = login_request.password.as_deref().unwrap_or_default();
        let matches = {
            let mut auth = w.auth.lock();
            account.password_matches(password, &mut **auth)
        };
        if !matches {
            if world_open {
                log::info!(
                    "client {} connected with non matching password so booting",
                    login_request.account
                );
            } else {
                log::debug!(
                    "client {} connected with non matching password so booting",
                    login_request.account
                );
            }

            terminate(
                w,
                session,
                SessionTerminationReason::NotAuthorizedPasswordMismatch,
                Some(game_message_boot_account(Some(
                    " because the password entered for this account was not correct",
                ))),
                String::new(),
            );

            // TO-DO: temporary lockout of account preventing brute force password discovery
            // exponential duration of lockout for targeted account

            return;
        }

        if property_manager_get_bool(w, "account_login_boots_in_use") {
            if let Some(previously_connected_account) = w.net.find_by_account(&account.account_name)
            {
                // Boot the existing account
                terminate(
                    w,
                    previously_connected_account,
                    SessionTerminationReason::AccountLoggedIn,
                    Some(game_message_character_error(CharacterError::Logon)),
                    String::new(),
                );

                // We still can't let the new account in. They'll need to retry after the previous account has been successfully booted.
                terminate(
                    w,
                    session,
                    SessionTerminationReason::AccountInUse,
                    Some(game_message_character_error(CharacterError::Logon)),
                    String::new(),
                );
                return;
            }
        }

        if world_open {
            log::info!(
                "client {} connected with verified password",
                login_request.account
            );
        } else {
            log::debug!(
                "client {} connected with verified password",
                login_request.account
            );
        }
    } else if login_request.net_auth_type == NetAuthType::GlsTicket {
        if world_open {
            log::info!(
                "client {} connected with GlsTicket which is not implemented yet so booting",
                login_request.account
            );
        } else {
            log::debug!(
                "client {} connected with GlsTicket which is not implemented yet so booting",
                login_request.account
            );
        }

        terminate(
            w,
            session,
            SessionTerminationReason::NotAuthorizedGlsTicketNotImplementedToProcLoginReq,
            Some(game_message_character_error(CharacterError::AccountInvalid)),
            String::new(),
        );

        return;
    }

    if let Some(ban_expire_time) = account.ban_expire_time {
        let now = w.now.utc;
        if now < ban_expire_time {
            let reason = account.ban_reason.clone();
            // `$"{(reason != null ? $" - {reason}" : null)}"`: a null interpolates as "".
            let text = reason
                .as_ref()
                .map(|r| format!(" - {r}"))
                .unwrap_or_default();
            let msg = game_message_account_banned(w, ban_expire_time, Some(&text));
            terminate(
                w,
                session,
                SessionTerminationReason::AccountBanned,
                Some(msg),
                reason.unwrap_or_default(),
            );
            return;
        }
        let mut auth = w.auth.lock();
        account.un_ban(&mut **auth);
    }

    {
        let mut auth = w.auth.lock();
        account.update_last_login(end_point_c2s.ip(), &mut **auth);
    }

    let access_level = AccessLevel(i32::try_from(account.access_level).unwrap_or(i32::MAX));
    if let Some(s) = w.sessions.get_mut(session) {
        s.set_account(
            account.account_id,
            account.account_name.clone(),
            access_level,
        );
        s.state = SessionState::AuthConnectResponse;
    }
    w.net.accept_login(
        session,
        account.account_id,
        account.account_name,
        account.access_level,
    );
}

// ACE: AuthenticationHandler.HandleConnectResponse
/// The handshake is complete: when the world is open (or for staff), fetch the account's
/// characters and send them.
pub fn handle_connect_response(w: &mut World, session: SessionId) {
    // Not ACE: the world half of `session.State = SessionState.AuthConnected`, which empyrean-net's
    // `NetworkManager.ProcessPacket` set on its half.
    if let Some(s) = w.sessions.get_mut(session) {
        s.state = SessionState::AuthConnected;
    }

    let Some((account_id, access_level)) = w
        .sessions
        .get(session)
        .map(|s| (s.account_id, s.access_level))
    else {
        return;
    };

    if w.world_manager.world_status == WorldStatusState::Open || access_level > AccessLevel::Player
    {
        w.shard.get_characters(
            account_id,
            false,
            Some(Box::new(
                move |w: &mut World, result: Vec<CharacterSummary>| {
                    // If you want to create default characters for accounts that have none, here is where you would do it.

                    send_connect_response(w, session, result);
                },
            )),
        );
    } else {
        terminate(
            w,
            session,
            SessionTerminationReason::WorldClosed,
            Some(game_message_character_error(
                CharacterError::LogonServerFull,
            )),
            String::new(),
        );
    }
}

// ACE: AuthenticationHandler.SendConnectResponse
/// The character list (last played first), the server name and the DDD interrogation.
pub fn send_connect_response(
    w: &mut World,
    session: SessionId,
    mut characters: Vec<CharacterSummary>,
) {
    // The client highlights the first character in the list. We sort so the first character sent is the one we last logged in
    order_by_descending_last_login(&mut characters);
    sessions::update_characters(w, session, characters);

    let Some(s) = w.sessions.get(session) else {
        return;
    };
    let character_list_message = game_message_character_list(w, &s.characters, s);
    let server_name_message = game_message_server_name(
        &sessions::config_server_world_name(),
        player_manager::get_online_count(w),
        i32::try_from(w.net.config.maximum_allowed_sessions).unwrap_or(i32::MAX),
    );
    let ddd_interrogation = game_message_ddd_interrogation(w);

    enqueue_send(w, session, character_list_message);
    enqueue_send(w, session, server_name_message);
    enqueue_send(w, session, ddd_interrogation);
}

/// `OrderByDescending(o => o.LastLoginTimestamp)`: a stable sort, with .NET's double ordering
/// (`NaN` below every number).
fn order_by_descending_last_login(characters: &mut [CharacterSummary]) {
    characters.sort_by(|a, b| {
        let key = |c: &CharacterSummary| (!c.last_login_timestamp.is_nan(), c.last_login_timestamp);
        let (ka, kb) = (key(a), key(b));
        kb.0.cmp(&ka.0)
            .then_with(|| kb.1.partial_cmp(&ka.1).unwrap_or(std::cmp::Ordering::Equal))
    });
}

/// `session.Terminate(reason, message, null, extraReason)` from the world.
fn terminate(
    w: &mut World,
    session: SessionId,
    reason: SessionTerminationReason,
    message: Option<GameMessage>,
    extra_reason: String,
) {
    let now = w.now;
    w.net.terminate(
        session,
        reason,
        message.map(GameMessage::into_outbound),
        extra_reason,
        now,
    );
}
