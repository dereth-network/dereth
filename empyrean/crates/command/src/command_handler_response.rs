// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/CommandHandlerResponse.cs
//! Port of `Source/ACE.Server/Command/CommandHandlerResponse.cs`.

// ACE: CommandHandlerResponse
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandHandlerResponse {
    Ok,
    SudoOk,
    InvalidCommand,
    NoConsoleInvoke,
    NotAuthorized,
    InvalidParameterCount,
    NotInWorld,
}
