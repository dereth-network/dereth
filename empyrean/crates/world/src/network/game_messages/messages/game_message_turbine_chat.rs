// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageTurbineChat.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageTurbineChat.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::ChatNetworkBlobDispatchType;
use empyrean_entity::enums::ChatNetworkBlobType;
use empyrean_entity::enums::ChatType;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

/// `Encoding.Unicode.GetBytes(s)`: UTF-16LE code units.
fn unicode_bytes(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// The length prefix of a TurbineChat string, in the client's packed form: one byte under 128
/// characters, two (`0x80 | high`, low) under 0x4000, else four.
///
/// **Retail (V239):** each string carries its own length. ACE packed the
/// message's length for the sender name too, and its two-byte form (`0x80 | (length << 8)` as a
/// `ushort`) dropped every bit above the low byte; the two forms agree below 256.
fn write_length(writer: &mut Vec<u8>, length: usize) {
    let mut packed = dereth_protocol::Writer::new();
    packed.compressed_u32(u32::try_from(length).unwrap_or(u32::MAX));
    writer.write_bytes(&packed.into_inner());
}

/// A "bytes to follow" field: the bytes after the field, to `end`.
///
/// **Retail (V239):** exactly the bytes that follow it. ACE counted from the
/// field's own start plus 4 (`Position - sizePos + 4`), 8 more than follow it.
fn size_to_follow(end: usize, size_pos: usize) -> u32 {
    u32::try_from(end - size_pos - 4).unwrap_or(u32::MAX)
}

// ACE: GameMessageTurbineChat.GameMessageTurbineChat
/// The two blob layouts ACE documents in its comment (`NETBLOB_EVENT_BINARY` `SendToRoomChatEvent`
/// and `NETBLOB_RESPONSE_BINARY` `SendToRoomByIDResponse`). Any other blob type writes only the
/// opcode (ACE prints "Unhandled GameMessageTurbineChat ChatNetworkBlobType" to the console).
#[must_use]
pub fn game_message_turbine_chat(
    chat_network_blob_type: ChatNetworkBlobType,
    chat_network_blob_dispatch_type: ChatNetworkBlobDispatchType,
    channel: u32,
    sender_name: &str,
    message: &str,
    sender_id: u32,
    chat_type: ChatType,
) -> GameMessage {
    let mut msg = GameMessage::new(GameMessageOpcode::TurbineChat, GameMessageGroup::LoginQueue);
    let w = &mut msg.data;

    if chat_network_blob_type == ChatNetworkBlobType::NETBLOB_EVENT_BINARY {
        let first_size_pos = w.stream_position();
        w.write_u32(0); // Bytes to follow
        w.write_u32(chat_network_blob_type.0.cs_cast());
        w.write_u32(chat_network_blob_dispatch_type.0.cs_cast());
        w.write_u32(1);
        w.write_i32(0x000B_00B5); // Unique ID? Both ID's always match. These numbers change between 0x000B0000 - 0x000B00FF I think.
        w.write_u32(1);
        w.write_i32(0x000B_00B5); // Unique ID? Both ID's always match. These numbers change between 0x000B0000 - 0x000B00FF I think.
        w.write_u32(0);

        let second_size_pos = w.stream_position();
        w.write_u32(0); // Bytes to follow
        w.write_u32(channel);

        write_length(w, sender_name.encode_utf16().count());
        w.write_bytes(&unicode_bytes(sender_name));
        write_length(w, message.encode_utf16().count());
        w.write_bytes(&unicode_bytes(message));
        w.write_u32(0x0C);
        w.write_u32(sender_id);
        w.write_u32(0);
        w.write_u32(chat_type.0.cs_cast());

        let end = w.stream_position();
        w.write_position(size_to_follow(end, first_size_pos), first_size_pos);
        w.write_position(size_to_follow(end, second_size_pos), second_size_pos);
    } else if chat_network_blob_type == ChatNetworkBlobType::NETBLOB_RESPONSE_BINARY {
        let first_size_pos = w.stream_position();
        w.write_u32(0); // Bytes to follow
        w.write_u32(chat_network_blob_type.0.cs_cast());
        w.write_u32(chat_network_blob_dispatch_type.0.cs_cast());
        w.write_u32(1);
        w.write_i32(0x000B_00B5); // Unique ID? Both ID's always match. These numbers change between 0x000B0000 - 0x000B00FF I think.
        w.write_u32(1);
        w.write_i32(0x000B_00B5); // Unique ID? Both ID's always match. These numbers change between 0x000B0000 - 0x000B00FF I think.
        w.write_u32(0);

        let second_size_pos = w.stream_position();
        w.write_u32(0); // Bytes to follow
        w.write_u32(channel);
        w.write_u32(2);
        w.write_u32(2);
        w.write_u32(0);

        let end = w.stream_position();
        w.write_position(size_to_follow(end, first_size_pos), first_size_pos);
        w.write_position(size_to_follow(end, second_size_pos), second_size_pos);
    } else {
        log::info!(
            "Unhandled GameMessageTurbineChat ChatNetworkBlobType: 0x{:04X}",
            chat_network_blob_type.0
        );
    }
    msg
}
