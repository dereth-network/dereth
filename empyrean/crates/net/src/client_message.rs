// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/ClientMessage.cs

/// ACE `ClientMessage`: one complete client-to-server message, opcode first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientMessage {
    /// ACE `Data`: the whole message, opcode included.
    pub data: Vec<u8>,
    /// ACE `Opcode`: the first four bytes.
    pub opcode: u32,
}

impl ClientMessage {
    // ACE: ClientMessage.ClientMessage
    /// `None` where ACE's constructor would throw `EndOfStreamException` (fewer than 4 bytes); both
    /// of ACE's callers check the length first, so that never happens on a live path.
    #[must_use]
    pub fn new(data: Vec<u8>) -> Option<Self> {
        let opcode = u32::from_le_bytes(data.get(..4)?.try_into().ok()?);
        Some(Self { data, opcode })
    }

    /// ACE `Payload` positioned after the opcode.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.data[4..]
    }
}
