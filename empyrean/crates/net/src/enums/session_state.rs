// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Enum/SessionState.cs

/// ACE `SessionState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum SessionState {
    #[default]
    AuthLoginRequest,
    AuthConnectResponse,
    AuthConnected,
    WorldConnected,
    TerminationStarted,
}
