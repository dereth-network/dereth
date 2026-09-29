// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Handlers/AuthenticationHandler.cs
//
// The transport half. ACE's `DoLogin` (account lookup and auto-creation), the account checks in
// `AccountSelectCallback` (not found, in use, password, GLS ticket, ban, last-login update) and
// `HandleConnectResponse`/`SendConnectResponse` (world status, character list) need the account
// database and the world; the world does them between the calls this file provides:
//
//   Event::LoginRequest  ->  world: DoLogin  ->  ServerNet::account_select_callback
//     -> AccountSelect::Continue  ->  world: account checks  ->  accept_login / reject_login

use crate::client_packet::ClientPacket;
use crate::enums::{CharacterError, NetAuthType, SessionState, SessionTerminationReason};
use crate::managers::network_manager::send_login_request_reject;
use crate::network_session::NetworkSession;
use crate::packets::packet_inbound_login_request::PacketInboundLoginRequest;
use crate::packets::packet_outbound_connect_request::packet_outbound_connect_request;
use crate::session::{NetIo, Session, SessionCore};
use crate::Event;

/// ACE `AuthenticationHandler.DefaultAuthTimeout`: seconds until an authentication request
/// expires.
pub const DEFAULT_AUTH_TIMEOUT: i32 = 15;

/// The only client version ACE accepts: the end of retail.
pub const CLIENT_VERSION: &str = "1802";

/// The login name ACE treats as a server-list ping from ThwargLauncher.
pub const SERVER_TRACKER_ACCOUNT: &str = "acservertracker:jj9h26hcsggc";

/// What [`account_select_callback`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountSelect {
    /// The `ConnectRequest` is queued; the world now checks the account and calls
    /// `accept_login` or `reject_login`.
    Continue,
    /// The session is terminating (bad handshake, wrong client version, no password, or the
    /// server-tracker ping). The world does nothing more for it.
    Terminated,
}

// ACE: AuthenticationHandler.HandleLoginRequest
/// Parses the login request and hands it to the world, or rejects an over-long account name.
pub fn handle_login_request(
    packet: &ClientPacket,
    network: &mut NetworkSession,
    core: &mut SessionCore,
    io: &mut NetIo<'_>,
) {
    let Some(login_request) = packet
        .header_optional
        .login_request
        .as_deref()
        .and_then(PacketInboundLoginRequest::new)
    else {
        log::error!(
            "Received LoginRequest from {} that threw an exception.",
            core.end_point_c2s
        );
        return;
    };

    // `loginRequest.Account.Length > 50`: C# string length, in UTF-16 units.
    if login_request.account.encode_utf16().count() > 50 {
        if send_login_request_reject(network, core, CharacterError::AccountInvalid, io) {
            core.terminate(
                SessionTerminationReason::AccountInformationInvalid,
                String::new(),
                io.now.utc,
            );
        }
        return;
    }

    // `new Task(() => DoLogin(session, loginRequest)).Start()`: the world's.
    core.login_request = Some(login_request.clone());
    io.events.push_back(Event::LoginRequest {
        session: core.id,
        from: core.end_point_c2s,
        request: login_request,
    });
}

/// Not ACE's (a fix): whether `packet` repeats the login `in_progress`
/// that its endpoint's session already holds. The same login is the same account, credentials,
/// authentication type and client version; the authenticator's `Timestamp` is not compared, so a
/// re-sent request counts as a copy whether or not the client renumbered it. A request that does
/// not parse is not a copy (it keeps ACE's handling).
#[must_use]
pub fn is_repeated_login_request(
    packet: &ClientPacket,
    in_progress: Option<&PacketInboundLoginRequest>,
) -> bool {
    let (Some(held), Some(section)) =
        (in_progress, packet.header_optional.login_request.as_deref())
    else {
        return false;
    };
    PacketInboundLoginRequest::new(section).is_some_and(|copy| {
        copy.net_auth_type == held.net_auth_type
            && copy.account == held.account
            && copy.password == held.password
            && copy.gls_ticket == held.gls_ticket
            && copy.client_version == held.client_version
    })
}

/// Not ACE's (retail's): a repeated LoginRequest for the session's login
/// is ignored. It starts no second login, draws no reject or answer, and leaves the state and the
/// timeout alone. A lost `ConnectRequest` is recovered by the server's own one-second re-send
/// instead (see `NetworkSession::update`), as retail's was.
pub fn repeated_login_request(session: &Session) {
    log::debug!(
        "Repeated LoginRequest from {} for the login in progress ({:?}); ignored.",
        session.core.end_point_c2s,
        session.core.state,
    );
}

// ACE: AuthenticationHandler.AccountSelectCallback
/// The transport checks at the head of ACE's `AccountSelectCallback`, and the `ConnectRequest`.
pub fn account_select_callback(session: &mut Session, io: &mut NetIo<'_>) -> AccountSelect {
    let utc = io.now.utc;
    let cd = &session.network.connection_data;
    let (Some(server_seed), Some(client_seed)) = (cd.server_seed, cd.client_seed) else {
        // "these are null if ConnectionData.DiscardSeeds() is called because of some other error
        // condition."
        // Not ACE's (a fix): in ACE they were also null when a repeated
        // LoginRequest started a second `DoLogin` after the first had sent its ConnectRequest,
        // aborting the login; a copy of the login in progress now starts no second login
        // (`is_repeated_login_request`). Only a different login from the same endpoint gets here.
        session.terminate(
            SessionTerminationReason::BadHandshake,
            Some((io.messages.character_error)(CharacterError::ServerCrash1)),
            None,
            String::new(),
            utc,
        );
        return AccountSelect::Terminated;
    };
    let Some(login_request) = session.core.login_request.clone() else {
        // Unreachable from the world's side (it answers a LoginRequest event), kept total.
        return AccountSelect::Terminated;
    };

    if login_request.client_version != CLIENT_VERSION {
        // DIVERGE: ACE's boot reason names ACE's client site; ours names https://dereth.network (brand).
        session.terminate(
            SessionTerminationReason::ClientVersionIncorrect,
            Some((io.messages.boot_account)(Some(
                " because your client is not the correct version for this server. Please visit https://dereth.network to update to latest client",
            ))),
            None,
            String::new(),
            utc,
        );
        return AccountSelect::Terminated;
    }

    let connect_request = packet_outbound_connect_request(
        io.now.portal_year_ticks,
        session.network.connection_data.connection_cookie,
        u32::from(session.network.client_id),
        server_seed,
        client_seed,
    );
    session.network.connection_data.discard_seeds();
    session.network.enqueue_send_packet(connect_request);

    if login_request.net_auth_type.lt(NetAuthType::AccountPassword) {
        if login_request.account == SERVER_TRACKER_ACCOUNT {
            session.terminate(
                SessionTerminationReason::PongSentClosingConnection,
                Some((io.messages.character_error)(CharacterError::ServerCrash1)),
                None,
                String::new(),
                utc,
            );
            return AccountSelect::Terminated;
        }
        log::debug!(
            "client {} connected with no Password or GlsTicket included so booting",
            login_request.account
        );
        session.terminate(
            SessionTerminationReason::NotAuthorizedNoPasswordOrGlsTicketIncludedInLoginReq,
            Some((io.messages.character_error)(
                CharacterError::AccountInvalid,
            )),
            None,
            String::new(),
            utc,
        );
        return AccountSelect::Terminated;
    }

    AccountSelect::Continue
}

/// The tail of ACE's `AccountSelectCallback`, once the world has accepted the account:
/// `session.SetAccount(...)` and `State = AuthConnectResponse`.
pub fn accept_login(session: &mut Session, account_id: u32, account: String, access_level: u32) {
    session.core.set_account(account_id, account, access_level);
    session.core.state = SessionState::AuthConnectResponse;
}
