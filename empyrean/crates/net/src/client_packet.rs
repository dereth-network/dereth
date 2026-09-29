// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/ClientPacket.cs
//
// Also carries `ClientPacketFragment.cs`, `PacketHeaderOptional.cs`, `Packet.cs`, `PacketHeader.cs`
// and `PacketFragmentHeader.cs` on the receive side: their byte layouts are the shared
// `dereth_transport::wire` types, and what is ported here is how ACE reads and verifies them.

use dereth_transport::crc::recover_isaac_key;
use dereth_transport::session::SequenceWindow;
use dereth_transport::wire::optional::seq_ids;
use dereth_transport::wire::{PacketFlags, ParsedPacket, ProtoHeader, HEADER_SIZE};

use crate::network_statistics::NetworkStatistics;

/// ACE `ClientPacket.MaxPacketSize`: the receive buffer. A longer datagram fails the receive
/// (`SocketError.MessageSize` on Windows) and never reaches ACE.
pub const MAX_PACKET_SIZE: usize = 1024;

/// ACE `PacketHeaderOptional`: the values ACE keeps from the sections it reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PacketHeaderOptional {
    pub ack_sequence: u32,
    pub time_synch: f64,
    pub echo_request_client_time: f32,
    pub retransmit_data: Option<Vec<u32>>,
    pub flow_bytes: u32,
    pub flow_interval: u16,
    /// The `ConnectResponse` cookie, which ACE re-reads through `DataReader` in
    /// `PacketInboundConnectResponse`.
    pub connect_response: Option<u64>,
    /// The raw `LoginRequest` section, which ACE re-reads in `PacketInboundLoginRequest`.
    pub login_request: Option<Vec<u8>>,
    /// The `WorldLoginRequest` key, re-read in `PacketInboundWorldLoginRequest`.
    pub world_login_request: Option<u64>,
    /// ACE `Size`: the bytes of the sections read (every section the packet carries).
    pub size: u32,
    /// ACE `Header.Flags` as `HasFlag` reads them in `ToString`.
    pub flags: u32,
}

impl std::fmt::Display for PacketHeaderOptional {
    // ACE: PacketHeaderOptional.ToString
    /// `IsValid` is always true here: an invalid optional header fails the unpacking.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.size == 0 {
            return Ok(());
        }

        let mut nice = String::new();
        if self.flags & PacketFlags::FLOW != 0 {
            nice = format!(" {} Interval: {}", self.flow_bytes, self.flow_interval);
        }
        if let Some(retransmit_data) = &self.retransmit_data {
            // `DefaultIfEmpty()`: an empty list prints as one 0.
            let list = if retransmit_data.is_empty() {
                "0".to_owned()
            } else {
                retransmit_data
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            };
            nice += &format!(" requesting {list}");
        }
        if self.flags & PacketFlags::ACK_SEQUENCE != 0 {
            nice += &format!(" AckSeq: {}", self.ack_sequence);
        }
        f.write_str(nice.trim())
    }
}

// ACE: PacketHeader.ToString
/// A packet header for the packet log: sequence, id, iteration, the checksum (marked `X` when
/// encrypted) and the flag names.
#[must_use]
pub fn packet_header_to_string(header: &ProtoHeader) -> String {
    let c = if header.header.0 & PacketFlags::ENCRYPTED_CHECKSUM != 0 {
        "X"
    } else {
        ""
    };

    format!(
        "Seq: {} Id: {} Iter: {} {c}CRC: {} {}",
        header.seq_id,
        header.rec_id,
        header.iteration,
        header.checksum,
        crate::packet_header_flags_util::unfold_flags(header.header.0)
    )
}

/// ACE `ClientPacketFragment` (`PacketFragmentHeader` plus `Data`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientPacketFragment {
    pub sequence: u32,
    pub id: u32,
    pub count: u16,
    pub size: u16,
    pub index: u16,
    pub queue: u16,
    pub data: Vec<u8>,
}

/// ACE `ClientPacket`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientPacket {
    pub header: ProtoHeader,
    pub header_optional: PacketHeaderOptional,
    pub fragments: Vec<ClientPacketFragment>,
    // ACE: ClientPacket.headerChecksum
    /// ACE `headerChecksum` (`PacketHeader.CalculateHash32`), computed once, at unpacking (ACE
    /// computes it at first use and caches it).
    header_checksum: u32,
    // ACE: ClientPacket.payloadChecksum, ClientPacket.headerOptionalChecksum, ClientPacket.fragmentChecksum
    /// ACE `payloadChecksum`: `headerOptionalChecksum` (the hash of every optional section) plus
    /// `fragmentChecksum` (every fragment's hash), computed once, at unpacking.
    payload_checksum: u32,
}

impl ClientPacket {
    // ACE: ClientPacket.Unpack, ClientPacket.ReadFragments, ClientPacketFragment.Unpack, PacketHeaderOptional.Unpack, PacketHeader.Unpack, PacketFragmentHeader.Unpack
    /// Takes a datagram apart. `None` wherever ACE's `Unpack` returns `false`.
    ///
    /// ACE reads `Header.Size` bytes after the header and ignores anything beyond them; so does
    /// this. The sections and fragments are read by the shared parser.
    ///
    /// DIVERGE: the shared parser also enforces the retail client's structural rules (a packet is
    /// encrypted iff it carries fragments or a non-disposable section; exclusive sections travel
    /// alone; at most 114 ids in a retransmit list; no undefined flag bits), which ACE does not
    /// check. A retail client never sends a packet that breaks them. The client's header-id limit
    /// (below 256) is not applied: on a client's packet it is the session's client id, which may be
    /// 256 or more (V283).
    #[must_use]
    pub fn unpack(buffer: &[u8]) -> Option<Self> {
        if buffer.len() < HEADER_SIZE {
            return None;
        }
        let header = ProtoHeader::from_bytes(buffer).ok()?;
        if usize::from(header.datalen) > buffer.len() - HEADER_SIZE {
            return None;
        }
        let parsed =
            ParsedPacket::parse_from_client(&buffer[..HEADER_SIZE + usize::from(header.datalen)])
                .ok()?;
        Some(Self::from_parsed(&parsed))
    }

    // ACE: PacketHeaderOptional.CalculateHash32, ClientPacketFragment.CalculateHash32
    fn from_parsed(parsed: &ParsedPacket) -> Self {
        let mut ho = PacketHeaderOptional {
            flags: parsed.header.header.0,
            ..PacketHeaderOptional::default()
        };
        // Not ACE's (a fix): every section the packet carries is consumed
        // and hashed, as the sender wrote it, by the shared parser, and the payload checksum is the
        // parser's. ACE read and hashed only the sections it expects, so a packet with any other
        // non-empty section (`NetError`, `NetErrorDisconnect`, `EchoResponse`, ...) failed its
        // checksum and was dropped whole. The values ACE keeps are kept as ACE keeps them; the
        // other sections are ignored.
        for (&mask, body) in &parsed.optional {
            ho.size = ho
                .size
                .wrapping_add(u32::try_from(body.len()).unwrap_or(u32::MAX));
            match mask {
                PacketFlags::REQUEST_RETRANSMIT => ho.retransmit_data = Some(seq_ids(body)),
                PacketFlags::ACK_SEQUENCE => ho.ack_sequence = rd_u32(body, 0),
                PacketFlags::LOGIN_REQUEST => ho.login_request = Some(body.clone()),
                PacketFlags::WORLD_LOGIN_REQUEST => ho.world_login_request = Some(rd_u64(body)),
                PacketFlags::CONNECT_RESPONSE => ho.connect_response = Some(rd_u64(body)),
                PacketFlags::TIME_SYNC => ho.time_synch = f64::from_bits(rd_u64(body)),
                PacketFlags::ECHO_REQUEST => {
                    ho.echo_request_client_time = f32::from_bits(rd_u32(body, 0))
                }
                PacketFlags::FLOW => {
                    ho.flow_bytes = rd_u32(body, 0);
                    ho.flow_interval = u16::from_le_bytes([body[4], body[5]]);
                }
                _ => {}
            }
        }
        let fragments = parsed
            .fragments
            .iter()
            .map(|f| ClientPacketFragment {
                sequence: f.header.blob_id_low,
                id: f.header.blob_id_high,
                count: f.header.num_frags,
                size: f.header.blob_frag_size,
                index: f.header.blob_num,
                queue: f.header.queue_id,
                data: f.payload.clone(),
            })
            .collect();
        Self {
            header: parsed.header,
            header_optional: ho,
            fragments,
            header_checksum: parsed.header_hash,
            payload_checksum: parsed.payload_hash,
        }
    }

    // ACE: PacketHeader.HasFlag
    /// True when **any** of the bits is set.
    #[must_use]
    pub const fn has_flag(&self, flags: u32) -> bool {
        self.header.header.0 & flags != 0
    }

    /// ACE `Header.Flags` compared whole, as `packet.Header.Flags == PacketHeaderFlags.AckSequence`.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.header.header.0
    }

    // ACE: ClientPacket.VerifyCRC
    /// Not ACE's: the key an encrypted checksum must carry is the one
    /// the shared retail receive window assigns to the packet's sequence (the next key in order,
    /// or the key parked when the sequence was skipped), not any key ACE's `CryptoSystem.Search`
    /// finds up to 256 draws ahead. A duplicate, a sequence never skipped, or a key that does not
    /// match is refused and counted as a checksum error; a mismatch on a sequenced encrypted
    /// packet parks its key again so the resend can still be accepted. The plaintext check and
    /// the checksums themselves are ACE's, over every section.
    pub fn verify_crc(&self, fq: &mut SequenceWindow, stats: &mut NetworkStatistics) -> bool {
        let encrypted = self.has_flag(PacketFlags::ENCRYPTED_CHECKSUM);
        let verdict = fq.accept(&self.header, |key| match key {
            Some(key) => {
                recover_isaac_key(
                    self.header.checksum,
                    self.header_checksum,
                    self.payload_checksum,
                ) == key
            }
            None => {
                self.header_checksum.wrapping_add(self.payload_checksum) == self.header.checksum
            }
        });
        if verdict.is_ok() {
            if !encrypted {
                log::debug!(target: "Packets", "{self}");
            }
            return true;
        }
        if !encrypted {
            log::debug!(target: "Packets", "{self}, Checksum Failed");
        }
        stats.c2s_crc_errors_aggregate_increment();
        false
    }
}

fn rd_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn rd_u64(b: &[u8]) -> u64 {
    u64::from(rd_u32(b, 0)) | (u64::from(rd_u32(b, 4)) << 32)
}

impl std::fmt::Display for ClientPacket {
    // ACE: ClientPacket.ToString
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = format!(
            "<<< {} {}",
            packet_header_to_string(&self.header),
            self.header_optional
        );
        f.write_str(s.trim_end())
    }
}
