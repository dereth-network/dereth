// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Enum/SessionTerminationPhase.cs

/// ACE `SessionTerminationPhase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionTerminationPhase {
    #[default]
    Initialized,
    SessionWorkCompleted,
    WorldManagerWorkCompleted,
}
