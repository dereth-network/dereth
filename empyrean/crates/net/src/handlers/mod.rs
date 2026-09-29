//! `Source/ACE.Server/Network/Handlers`: the transport-level handlers. The other handler files
//! (`CharacterHandler`, `ControlHandler`, `DDDHandler`, `TurbineChatHandler`, …) are world-level
//! (empyrean-world); `ControlHandler.ControlResponse` is empyrean-world's `control_handler`.

pub mod authentication_handler;
