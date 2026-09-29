// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/GameActionAttribute.cs
//! Port of `Source/ACE.Server/Network/GameAction/GameActionAttribute.cs`.

// ACE: GameActionAttribute
/// `[GameAction(opcode)]`: the `GameActionType` a handler is registered for (the raw value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameActionAttribute {
    /// `Opcode`.
    pub opcode: u32,
}

impl GameActionAttribute {
    // ACE: GameActionAttribute.GameActionAttribute
    pub const fn new(opcode: u32) -> Self {
        Self { opcode }
    }
}
