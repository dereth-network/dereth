// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/ServerPacket.cs
//
// Also carries `ServerPacketFragment.cs` and the send side of `PacketHeader.cs` and
// `PacketFragmentHeader.cs`, whose byte layouts are the shared `dereth_transport::wire` types.

use dereth_transport::crc::{hash32, wire_checksum};
use dereth_transport::wire::{
    FragmentHeader, PacketFlags, ProtoHeader, FRAG_HEADER_SIZE, HEADER_SIZE,
};
use empyrean_common::dotnet::CsCast;

/// ACE `ServerPacket.MaxPacketSize`: the room after the 20-byte header.
pub const MAX_PACKET_SIZE: i32 = 464;

/// ACE `ServerPacketFragment`: a fragment header and its slice of the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPacketFragment {
    /// `Sequence` is `blob_id_low`, `Id` is `blob_id_high`, `Count` is `num_frags`, `Size` is
    /// `blob_frag_size`, `Index` is `blob_num` and `Queue` is `queue_id`.
    pub header: FragmentHeader,
    pub data: Vec<u8>,
}

impl ServerPacketFragment {
    // ACE: ServerPacketFragment.ServerPacketFragment
    #[must_use]
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            header: FragmentHeader::default(),
            data,
        }
    }

    // ACE: PacketFragment.Length
    /// ACE `PacketFragment.Length`.
    #[must_use]
    pub fn length(&self) -> i32 {
        i32::try_from(FRAG_HEADER_SIZE + self.data.len()).unwrap_or(i32::MAX)
    }

    // ACE: ServerPacketFragment.PackAndReturnHash32, PacketFragmentHeader.PackAndReturnHash32
    /// Appends the fragment to `buffer` and returns its contribution to the payload hash.
    pub fn pack_and_return_hash32(&mut self, buffer: &mut Vec<u8>) -> u32 {
        self.header.blob_frag_size =
            u16::try_from(FRAG_HEADER_SIZE + self.data.len()).unwrap_or(u16::MAX);
        // ACE: PacketFragmentHeader.Pack
        let header_bytes = self.header.to_bytes();
        let header_hash32 = hash32(&header_bytes);
        buffer.extend_from_slice(&header_bytes);
        buffer.extend_from_slice(&self.data);
        header_hash32.wrapping_add(hash32(&self.data))
    }
}

/// ACE `ServerPacket`.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerPacket {
    /// `Sequence`, `Flags`, `Checksum`, `Id`, `Time` (the retail `interval_` field), `Size` and
    /// `Iteration`.
    pub header: ProtoHeader,
    /// ACE `Data`: the optional-header bytes, written through `DataWriter`.
    pub data: Option<Vec<u8>>,
    pub fragments: Vec<ServerPacketFragment>,
    final_checksum: u32,
    issac_xor: u32,
    issac_xor_set: bool,
}

impl Default for ServerPacket {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerPacket {
    #[must_use]
    pub fn new() -> Self {
        Self {
            header: ProtoHeader::default(),
            data: None,
            fragments: Vec::new(),
            final_checksum: 0,
            issac_xor: 0,
            issac_xor_set: false,
        }
    }

    /// ACE `Header.Flags`.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.header.header.0
    }

    /// ACE `Header.Flags |= flags`.
    pub fn add_flags(&mut self, flags: u32) {
        self.header.header = PacketFlags(self.header.header.0 | flags);
    }

    // ACE: PacketHeader.HasFlag
    /// Any of the bits.
    #[must_use]
    pub const fn has_flag(&self, flags: u32) -> bool {
        self.header.header.0 & flags != 0
    }

    // ACE: ServerPacket.IssacXor
    /// ACE `IssacXor`'s getter.
    #[must_use]
    pub const fn issac_xor(&self) -> u32 {
        self.issac_xor
    }

    // ACE: ServerPacket.IssacXor
    /// ACE `IssacXor`'s setter, which throws when called twice. Here the second call is refused
    /// and reported, since the caller (`SendPacket`) never makes it.
    pub fn set_issac_xor(&mut self, value: u32) -> bool {
        if self.issac_xor_set {
            return false;
        }
        self.issac_xor_set = true;
        self.issac_xor = value;
        true
    }

    // ACE: ServerPacket.InitializeDataWriter
    pub fn initialize_data_writer(&mut self) -> &mut Vec<u8> {
        self.data.get_or_insert_with(|| Vec::with_capacity(32))
    }

    // ACE: ServerPacket.CreateReadyToSendPacket
    /// The datagram's bytes. Sets `Size` and `Checksum` on the header as ACE does.
    ///
    /// ACE hashes the optional-header bytes as one block, where the client sums one hash per
    /// section. The results are equal because every section the server writes is a whole number
    /// of dwords (`server_packets_match_the_shared_core_byte_for_byte` in the tests).
    pub fn create_ready_to_send_packet(&mut self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(
            HEADER_SIZE + self.data.as_ref().map_or(0, Vec::len) + self.fragments.len() * 464,
        );
        buffer.resize(HEADER_SIZE, 0);
        let mut payload_checksum = 0u32;

        if let Some(body) = self.data.as_ref().filter(|d| !d.is_empty()) {
            buffer.extend_from_slice(body);
            payload_checksum = payload_checksum.wrapping_add(hash32(body));
        }

        for fragment in &mut self.fragments {
            payload_checksum =
                payload_checksum.wrapping_add(fragment.pack_and_return_hash32(&mut buffer));
        }

        // `Header.Size = (ushort)(size - HeaderSize)`: a C# narrowing that wraps.
        let size: u16 = i32::try_from(buffer.len() - HEADER_SIZE)
            .unwrap_or(i32::MAX)
            .cs_cast();
        self.header.datalen = size;

        // ACE: PacketHeader.CalculateHash32
        let header_checksum = self.header.header_hash();
        self.final_checksum = header_checksum.wrapping_add(payload_checksum);
        // An unencrypted packet's `IssacXor` is 0, so this is the plain sum there.
        self.header.checksum =
            wire_checksum(header_checksum, payload_checksum, Some(self.issac_xor));
        // ACE: PacketHeader.Pack
        buffer[..HEADER_SIZE].copy_from_slice(&self.header.to_bytes());
        buffer
    }
}

impl std::fmt::Display for ServerPacket {
    // ACE: ServerPacket.ToString
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = if self.has_flag(PacketFlags::ENCRYPTED_CHECKSUM) {
            format!(" CRC: {} XOR: {}", self.final_checksum, self.issac_xor)
        } else {
            String::new()
        };
        let s = format!(
            ">>> {}{c}",
            crate::client_packet::packet_header_to_string(&self.header)
        );
        f.write_str(s.trim_end())
    }
}
