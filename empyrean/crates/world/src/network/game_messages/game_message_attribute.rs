// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/GameMessageAttribute.cs
//! Port of `Source/ACE.Server/Network/GameMessages/GameMessageAttribute.cs`.

use empyrean_net::SessionState;

// ACE: GameMessageAttribute
/// `[GameMessage(opcode, state)]`: the opcode a handler is registered for and the session state
/// the message is accepted in. The opcode is the raw `GameMessageOpcode` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameMessageAttribute {
    /// `Opcode`.
    pub opcode: u32,
    /// `State`.
    pub state: SessionState,
}

impl GameMessageAttribute {
    // ACE: GameMessageAttribute.GameMessageAttribute
    pub const fn new(opcode: u32, state: SessionState) -> Self {
        Self { opcode, state }
    }
}
