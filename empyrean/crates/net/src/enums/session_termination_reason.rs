// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Enum/SessionTerminationReason.cs

/// ACE `SessionTerminationReason`, in ACE's declaration order (the order indexes the descriptions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionTerminationReason {
    #[default]
    None,
    PacketHeaderDisconnect,
    AccountInformationInvalid,
    AccountSelectCallbackException,
    NetworkTimeout,
    /// A trusted packet had `PacketHeaderFlags.NetErrorDisconnect`.
    ClientSentNetworkErrorDisconnect,
    AccountBooted,
    BadHandshake,
    PongSentClosingConnection,
    NotAuthorizedNoPasswordOrGlsTicketIncludedInLoginReq,
    NotAuthorizedAccountNotFound,
    AccountInUse,
    NotAuthorizedPasswordMismatch,
    NotAuthorizedGlsTicketNotImplementedToProcLoginReq,
    /// The client connection is no longer able to send us packets with encrypted CRC.
    ClientConnectionFailure,
    SendToSocketException,
    WorldClosed,
    AbnormalSequenceReceived,
    AccountLoggedIn,
    ServerShuttingDown,
    AccountBanned,
    ClientVersionIncorrect,
    ForcedLogOffRequested,
    AutoForcedLogOff,
    CharacterSaveFailed,
    BiotaSaveFailed,
    DATsPatchingDisabled,
    DATsNewerThanServer,
}

/// ACE `SessionTerminationReasonHelper.SessionTerminationReasonDescriptions`.
pub const SESSION_TERMINATION_REASON_DESCRIPTIONS: [&str; 28] = [
    "",
    "PacketHeader Disconnect",
    "Account Information Invalid",
    "AccountSelectCallback threw an exception",
    "Network Timeout",
    "client sent network error disconnect",
    "Account Booted",
    "Bad handshake",
    "Pong sent, closing connection.",
    "Not Authorized: No password or GlsTicket included in login request",
    "Not Authorized: Account Not Found",
    "Account In Use: Found another session already logged in for this account",
    "Not Authorized: Password does not match",
    "Not Authorized: GlsTicket is not implemented to process login request",
    "Client connection failure",
    "MainSocket.SendTo exception occured",
    "World is closed",
    "Client supplied an abnormal sequence",
    "Account was logged in, booting currently connected account in favor of new connection",
    "Server is shutting down",
    "Account is banned",
    "Client is not up to date",
    "Forced log off requested by Admin",
    "Forced log off by PlayerManager",
    "Character Save Failed",
    "Biota Save Failed",
    "Client has older DATs than server and patching is disabled",
    "Client has newer DATs than server and cannot be downgraded",
];

impl SessionTerminationReason {
    // ACE: SessionTerminationReasonHelper.GetDescription
    #[must_use]
    pub fn get_description(self) -> &'static str {
        SESSION_TERMINATION_REASON_DESCRIPTIONS
            .get(self as usize)
            .copied()
            .unwrap_or("<reason>")
    }
}
