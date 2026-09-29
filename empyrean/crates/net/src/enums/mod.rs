//! The transport enums of `Source/ACE.Server/Network/Enum`.
//!
//! The remaining files in that folder (`ArmorMask`, `AttributeMask`, …) describe game-message
//! payloads and belong with the message builders. `DamageLocation` and
//! `CharacterGenerationVerificationResponse` are here because the message builders take them.

pub mod character_error;
pub mod character_generation_verification_response;
pub mod damage_location;
pub mod net_auth_type;
pub mod session_state;
pub mod session_termination_phase;
pub mod session_termination_reason;

pub use character_error::CharacterError;
pub use character_generation_verification_response::CharacterGenerationVerificationResponse;
pub use damage_location::DamageLocation;
pub use net_auth_type::NetAuthType;
pub use session_state::SessionState;
pub use session_termination_phase::SessionTerminationPhase;
pub use session_termination_reason::SessionTerminationReason;
