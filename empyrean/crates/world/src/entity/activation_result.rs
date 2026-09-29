// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/ActivationResult.cs
//! Port of `Source/ACE.Server/Entity/ActivationResult.cs`.

use crate::network::game_messages::game_message::GameMessage;

// ACE: ActivationResult
/// Whether an activation may go on, or the message that refuses it.
#[derive(Debug, Clone)]
pub struct ActivationResult {
    // ACE: ActivationResult.Success
    pub success: bool,
    // ACE: ActivationResult.Message
    pub message: Option<GameMessage>,
}

impl ActivationResult {
    // ACE: ActivationResult.ActivationResult
    /// `new ActivationResult(bool success)`.
    #[must_use]
    pub fn new(success: bool) -> Self {
        ActivationResult {
            success,
            message: None,
        }
    }

    /// `new ActivationResult(GameEventMessage message)`: not a success.
    #[must_use]
    pub fn with_message(message: GameMessage) -> Self {
        ActivationResult {
            success: false,
            message: Some(message),
        }
    }
}
