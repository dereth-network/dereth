// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessageGroup.cs

/// ACE `GameMessageGroup`: the fragment queue a server message travels on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum GameMessageGroup {
    InvalidQueue = 0x00,
    EventQueue = 0x01,
    ControlQueue = 0x02,
    WeenieQueue = 0x03,
    LoginQueue = 0x04,
    DatabaseQueue = 0x05,
    SecureControlQueue = 0x06,
    /// Autonomous Position.
    SecureWeenieQueue = 0x07,
    SecureLoginQueue = 0x08,
    UIQueue = 0x09,
    SmartboxQueue = 0x0A,
    ObserverQueue = 0x0B,
}

impl GameMessageGroup {
    /// ACE `GameMessageGroup.QueueMax`: the number of per-group bundles a session keeps.
    pub const QUEUE_MAX: usize = 0x0C;

    /// Every group, in value order: the order `NetworkSession.Update` walks the bundles.
    pub const ALL: [Self; Self::QUEUE_MAX] = [
        Self::InvalidQueue,
        Self::EventQueue,
        Self::ControlQueue,
        Self::WeenieQueue,
        Self::LoginQueue,
        Self::DatabaseQueue,
        Self::SecureControlQueue,
        Self::SecureWeenieQueue,
        Self::SecureLoginQueue,
        Self::UIQueue,
        Self::SmartboxQueue,
        Self::ObserverQueue,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}
