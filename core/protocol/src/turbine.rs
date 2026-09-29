//! chatclient.dll by-ID requests and its two recognized incoming binary callback forms.
//! Primary: the retail chat DLL's request serialiser, header writer and send path, compared
//! byte-for-byte with the pristine DLL's output; this is NOT a recorded capture.
use crate::{Message, MessageError, Opcode, Reader, Writer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomEvent {
    pub room: u32,
    pub name: String,
    pub text: String,
    /// The DLL preserves the entire callback blob. The main client reads its first word.
    pub extra: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomResponse {
    pub context: u32,
    pub response_type: u32,
    pub method: u32,
    pub result: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncomingPayload {
    RoomEvent(RoomEvent),
    RoomResponse(RoomResponse),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingPacket {
    pub header: [u32; 7],
    pub payload: IncomingPayload,
    /// ACE GameMessageTurbineChat overstates BOTH extents by eight. Only recognized,
    /// complete semantic prefixes may use this compatibility path; no bytes are fabricated.
    pub ace_overstated_extent: bool,
    /// Original body (including outer length and any tails), also retained for unknown forms.
    pub raw: Vec<u8>,
}

fn read_wide(r: &mut Reader<'_>) -> Result<String, MessageError> {
    let count = r.compressed_u32()? as usize;
    let size = count.checked_mul(2).ok_or(MessageError::Unencodable {
        field: "Turbine text",
        reason: "byte count overflow",
    })?;
    let raw = r.bytes(size)?;
    let units: Vec<_> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect();
    String::from_utf16(&units).map_err(|_| MessageError::Unencodable {
        field: "Turbine text",
        reason: "invalid UTF16",
    })
}

/// The chat DLL's receive, unpack, event-dispatch and response paths.
/// Source parsers do not require exhaustion: declared payload tails and packet tails are
/// retained, not mistaken for another message. Never dereference a declared extent beyond
/// the supplied bytes, as the original client's logon handler does.
pub fn decode_incoming(raw: &[u8]) -> Result<IncomingPacket, MessageError> {
    let mut outer = Reader::new(raw);
    let declared_packet = outer.u32()? as usize;
    let available_packet = outer.remaining();
    let mut packet = Reader::new(outer.bytes(declared_packet.min(available_packet))?);
    let mut header = [0; 7];
    for word in &mut header {
        *word = packet.u32()?;
    }
    let declared_payload = packet.u32()? as usize;
    let available_payload = packet.remaining();
    let recognized = matches!((header[0], header[1]), (1, 1) | (5, 1 | 2));
    // Retail-exact and physically padded inputs produce identical callback fields. This
    // narrowly handles ACE's verified +8/+8 arithmetic, not arbitrary corrupt extents.
    let ace_overstated_extent = recognized
        && declared_packet.checked_sub(available_packet) == Some(8)
        && declared_payload.checked_sub(available_payload) == Some(8);
    if !ace_overstated_extent && declared_packet > available_packet {
        return Err(MessageError::UnexpectedEof {
            at: 4,
            needed: declared_packet,
            available: available_packet,
        });
    }
    if !ace_overstated_extent && declared_payload > available_payload {
        return Err(MessageError::UnexpectedEof {
            at: 36,
            needed: declared_payload,
            available: available_payload,
        });
    }
    let mut body = Reader::new(packet.bytes(declared_payload.min(available_payload))?);
    let payload = match (header[0], header[1]) {
        (1, 1) => {
            let room = body.u32()?;
            let name = read_wide(&mut body)?;
            let text = read_wide(&mut body)?;
            let extra_size = body.u32()? as usize;
            let extra = body.bytes(extra_size)?.to_vec();
            IncomingPayload::RoomEvent(RoomEvent {
                room,
                name,
                text,
                extra,
            })
        }
        // Dispatch determines the allocation only. Both serializers have the same four
        // words; retained pending request chooses the callback, not these type/method words.
        (5, 1 | 2) => IncomingPayload::RoomResponse(RoomResponse {
            context: body.u32()?,
            response_type: body.u32()?,
            method: body.u32()?,
            result: body.u32()?,
        }),
        _ => IncomingPayload::Unknown,
    };
    Ok(IncomingPacket {
        header,
        payload,
        ace_overstated_extent,
        raw: raw.to_vec(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendToRoomById {
    pub context: u32,
    pub room: u32,
    pub text: String,
    pub sender: u32,
    pub chat_type: u32,
}

fn expect(r: &mut Reader<'_>, value: u32) -> Result<(), MessageError> {
    let found = r.u32()?;
    if found == value {
        Ok(())
    } else {
        Err(MessageError::BadMagic {
            expected: value,
            found,
        })
    }
}

impl SendToRoomById {
    /// The bytes passed to the chat network's send call; no main-client opcode/length yet.
    pub fn network_packet(&self) -> Result<Vec<u8>, MessageError> {
        let utf16: Vec<_> = self.text.encode_utf16().collect();
        let count = u32::try_from(utf16.len())
            .ok()
            .filter(|n| *n <= 0x3fff_ffff)
            .ok_or(MessageError::Unencodable {
                field: "Turbine text",
                reason: "compressed count overflow",
            })?;
        let mut body = Writer::new();
        body.u32(self.context);
        body.u32(2); // response type
        body.u32(2); // send-to-room-by-id method
        body.u32(self.room);
        body.compressed_u32(count);
        for unit in utf16 {
            body.u16(unit);
        }
        body.u32(12);
        body.u32(self.sender);
        body.u32(0); // the chat blob's constructor-zero field
        body.u32(self.chat_type);
        let body = body.into_inner();
        let mut packet = Writer::new();
        for word in [3, 2, 1, 0, 0, 0, 0] {
            packet.u32(word);
        }
        packet.u32(
            u32::try_from(body.len()).map_err(|_| MessageError::Unencodable {
                field: "Turbine payload",
                reason: "byte count overflow",
            })?,
        );
        packet.bytes(&body);
        Ok(packet.into_inner())
    }
}

impl Message for SendToRoomById {
    const OPCODE: Opcode = Opcode::COMMUNICATION_TURBINE_CHAT;
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        let packet = self.network_packet()?;
        w.u32(
            u32::try_from(packet.len()).map_err(|_| MessageError::Unencodable {
                field: "Turbine packet",
                reason: "byte count overflow",
            })?,
        );
        w.bytes(&packet);
        Ok(())
    }
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let len = r.u32()? as usize;
        let mut packet = Reader::new(r.bytes(len)?);
        for word in [3, 2, 1, 0, 0, 0, 0] {
            expect(&mut packet, word)?;
        }
        let len = packet.u32()? as usize;
        let mut body = Reader::new(packet.bytes(len)?);
        packet.expect_exhausted()?;
        let context = body.u32()?;
        expect(&mut body, 2)?;
        expect(&mut body, 2)?;
        let room = body.u32()?;
        let count = body.compressed_u32()? as usize;
        let raw = body.bytes(count.checked_mul(2).ok_or(MessageError::Unencodable {
            field: "Turbine text",
            reason: "byte count overflow",
        })?)?;
        let units: Vec<_> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect();
        let text = String::from_utf16(&units).map_err(|_| MessageError::Unencodable {
            field: "Turbine text",
            reason: "invalid UTF16",
        })?;
        expect(&mut body, 12)?;
        let sender = body.u32()?;
        expect(&mut body, 0)?;
        let chat_type = body.u32()?;
        body.expect_exhausted()?;
        Ok(Self {
            context,
            room,
            text,
            sender,
            chat_type,
        })
    }
}
