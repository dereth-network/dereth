// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/PacketDirection.cs

/// ACE `PacketDirection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PacketDirection {
    #[default]
    None,
    /// Client -> Server.
    Client,
    /// Server -> Client.
    Server,
}
