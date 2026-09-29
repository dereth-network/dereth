// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Events/ChatMessageArgs.cs
//! Port of `Source/ACE.Server/Entity/Events/ChatMessageArgs.cs`.

use empyrean_entity::enums::ChatMessageType;

// ACE: ChatMessageArgs
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessageArgs {
    // ACE: ChatMessageArgs.Message
    pub message: String,
    // ACE: ChatMessageArgs.MessageType
    pub message_type: ChatMessageType,
}

impl ChatMessageArgs {
    // ACE: ChatMessageArgs.ChatMessageArgs
    #[must_use]
    pub fn new(message: &str, r#type: ChatMessageType) -> Self {
        Self {
            message: message.to_owned(),
            message_type: r#type,
        }
    }
}
