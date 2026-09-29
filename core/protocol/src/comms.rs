//! Family: communication — `docs/networking/messages/08-communication.md`.
//!
//! Chat, tells, emotes, system text, channels, the squelch database and the confirmation dialogs.
//!
//! **`0xF7DE Communication_TurbineChat` remains opaque in this incoming family.** It arrives on the
//! **Logon queue (4)**, whose consumer discards every other opcode, and
//! the logon-event queue hands the whole blob to `chatclient.dll` without
//! reading a field. `crate::turbine` ports the outgoing `SendToRoomById` serializer and the
//! recognized incoming room event/response callbacks; unsupported forms stay lossless here.

use crate::archive::{PackedHash, Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::Message;
use dereth_primitives::ObjectId;

/// A message whose body is one length-prefixed narrow string.
macro_rules! string_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct $name {
            pub $field: String,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { $field: r.pstring()? })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.pstring(&self.$field)
            }
        }
    };
}

/// A message whose body is one dword.
macro_rules! dword_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident, $ty:ty, $rd:ident, $wr:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name {
            pub $field: $ty,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { $field: r.$rd()? })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.$wr(self.$field);
                Ok(())
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------
// Client → server
// ---------------------------------------------------------------------------------------------

string_message!(
    /// `0x0015 Communication_Talk` — plain local speech.
    CommunicationTalk,
    COMMUNICATION_TALK,
    message
);
string_message!(
    /// `0x01DF Communication_Emote` — `/e`.
    CommunicationEmote,
    COMMUNICATION_EMOTE,
    message
);
string_message!(
    /// `0x01E1 Communication_SoulEmote` — the pose table.
    CommunicationSoulEmote,
    COMMUNICATION_SOUL_EMOTE,
    message
);
string_message!(
    /// `0x0010 Communication_SetAFKMessage`.
    CommunicationSetAfkMessage,
    COMMUNICATION_SET_AFKMESSAGE,
    message
);

dword_message!(
    /// `0x000F Communication_SetAFKMode`.
    CommunicationSetAfkMode,
    COMMUNICATION_SET_AFKMODE,
    afk,
    i32,
    i32,
    i32
);
dword_message!(
    /// `0x0145 Communication_AddToChannel`.
    CommunicationAddToChannel,
    COMMUNICATION_ADD_TO_CHANNEL,
    channel,
    u32,
    u32,
    u32
);
dword_message!(
    /// `0x0146 Communication_RemoveFromChannel`.
    CommunicationRemoveFromChannel,
    COMMUNICATION_REMOVE_FROM_CHANNEL,
    channel,
    u32,
    u32,
    u32
);
dword_message!(
    /// `0x0148 Communication_ChannelList`, **client to server**: which channel to list.
    CommunicationChannelListRequest,
    COMMUNICATION_CHANNEL_LIST,
    channel,
    u32,
    u32,
    u32
);

/// `0x0032 Communication_TalkDirect` — a tell by object id. **Message first, target second.**
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationTalkDirect {
    pub message: String,
    pub target: ObjectId,
}

impl Message for CommunicationTalkDirect {
    const OPCODE: Opcode = Opcode::COMMUNICATION_TALK_DIRECT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            message: r.pstring()?,
            target: ObjectId(r.u32()?),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.message)?;
        w.u32(self.target.0);
        Ok(())
    }
}

/// `0x005D Communication_TalkDirectByName` — a tell by name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationTalkDirectByName {
    pub message: String,
    pub target_name: String,
}

impl Message for CommunicationTalkDirectByName {
    const OPCODE: Opcode = Opcode::COMMUNICATION_TALK_DIRECT_BY_NAME;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            message: r.pstring()?,
            target_name: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.message)?;
        w.pstring(&self.target_name)
    }
}

/// `0x0147 Communication_ChannelBroadcast`, **client to server**.
///
/// **Correction to the community catalogue.** Its `0x0147` page lists `chan`, `senderName`, `msg` —
/// that is the *server-to-client* shape. The client's channel-broadcast request
/// writes only the channel and the message; the sender name is added by the server. See
/// [`CommunicationChannelBroadcastRecv`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationChannelBroadcast {
    pub channel: u32,
    pub message: String,
}

impl Message for CommunicationChannelBroadcast {
    const OPCODE: Opcode = Opcode::COMMUNICATION_CHANNEL_BROADCAST;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            channel: r.u32()?,
            message: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.channel);
        w.pstring(&self.message)
    }
}

/// `0x0149 Communication_ChannelIndex`, **client to server** — no payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommunicationChannelIndexRequest;

impl Message for CommunicationChannelIndexRequest {
    const OPCODE: Opcode = Opcode::COMMUNICATION_CHANNEL_INDEX;

    fn read(_: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self)
    }

    fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
        Ok(())
    }
}

/// `0x0058 Communication_ModifyCharacterSquelch` (C2S).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationModifyCharacterSquelch {
    /// 0 = unsquelch, 1 = squelch.
    pub add: i32,
    pub character_id: ObjectId,
    pub character_name: String,
    /// A `LogTextType`.
    pub msg_type: u32,
}

impl Message for CommunicationModifyCharacterSquelch {
    const OPCODE: Opcode = Opcode::COMMUNICATION_MODIFY_CHARACTER_SQUELCH;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            add: r.i32()?,
            character_id: ObjectId(r.u32()?),
            character_name: r.pstring()?,
            msg_type: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.add);
        w.u32(self.character_id.0);
        w.pstring(&self.character_name)?;
        w.u32(self.msg_type);
        Ok(())
    }
}

/// `0x0059 Communication_ModifyAccountSquelch` (C2S).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationModifyAccountSquelch {
    pub add: i32,
    pub character_name: String,
}

impl Message for CommunicationModifyAccountSquelch {
    const OPCODE: Opcode = Opcode::COMMUNICATION_MODIFY_ACCOUNT_SQUELCH;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            add: r.i32()?,
            character_name: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.add);
        w.pstring(&self.character_name)
    }
}

/// `0x005B Communication_ModifyGlobalSquelch` (C2S) — 12 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommunicationModifyGlobalSquelch {
    pub add: i32,
    pub msg_type: u32,
}

impl Message for CommunicationModifyGlobalSquelch {
    const OPCODE: Opcode = Opcode::COMMUNICATION_MODIFY_GLOBAL_SQUELCH;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            add: r.i32()?,
            msg_type: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.add);
        w.u32(self.msg_type);
        Ok(())
    }
}

/// `0x0275 Character_ConfirmationResponse` (C2S) — 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterConfirmationResponse {
    pub confirmation_type: i32,
    pub context_id: u32,
    pub accepted: i32,
}

impl Message for CharacterConfirmationResponse {
    const OPCODE: Opcode = Opcode::CHARACTER_CONFIRMATION_RESPONSE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            confirmation_type: r.i32()?,
            context_id: r.u32()?,
            accepted: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.confirmation_type);
        w.u32(self.context_id);
        w.i32(self.accepted);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Server → client
// ---------------------------------------------------------------------------------------------

/// `0x02BB Communication_HearSpeech`. Note the **string comes first** in the speech messages, and
/// the object id after it — unlike the emote messages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationHearSpeech {
    pub message: String,
    pub sender_name: String,
    pub sender_id: ObjectId,
    /// `LogTextType`.
    pub text_type: u32,
}

impl Message for CommunicationHearSpeech {
    const OPCODE: Opcode = Opcode::COMMUNICATION_HEAR_SPEECH;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            message: r.pstring()?,
            sender_name: r.pstring()?,
            sender_id: ObjectId(r.u32()?),
            text_type: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.message)?;
        w.pstring(&self.sender_name)?;
        w.u32(self.sender_id.0);
        w.u32(self.text_type);
        Ok(())
    }
}

/// `0x02BC Communication_HearRangedSpeech` — the same plus a range float before the type.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommunicationHearRangedSpeech {
    pub message: String,
    pub sender_name: String,
    pub sender_id: ObjectId,
    pub range: f32,
    pub text_type: u32,
}

impl Message for CommunicationHearRangedSpeech {
    const OPCODE: Opcode = Opcode::COMMUNICATION_HEAR_RANGED_SPEECH;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            message: r.pstring()?,
            sender_name: r.pstring()?,
            sender_id: ObjectId(r.u32()?),
            range: r.f32()?,
            text_type: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.message)?;
        w.pstring(&self.sender_name)?;
        w.u32(self.sender_id.0);
        w.f32(self.range);
        w.u32(self.text_type);
        Ok(())
    }
}

/// `0x02BD Communication_HearDirectSpeech` — a tell.
///
/// The client reads all four trailing dwords but uses only the first three; `secret_flags` is
/// discarded. It is still on the wire, so it is still decoded.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationHearDirectSpeech {
    pub message: String,
    pub sender_name: String,
    pub sender_id: ObjectId,
    pub target_id: ObjectId,
    pub text_type: u32,
    pub secret_flags: u32,
}

impl Message for CommunicationHearDirectSpeech {
    const OPCODE: Opcode = Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            message: r.pstring()?,
            sender_name: r.pstring()?,
            sender_id: ObjectId(r.u32()?),
            target_id: ObjectId(r.u32()?),
            text_type: r.u32()?,
            secret_flags: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.message)?;
        w.pstring(&self.sender_name)?;
        w.u32(self.sender_id.0);
        w.u32(self.target_id.0);
        w.u32(self.text_type);
        w.u32(self.secret_flags);
        Ok(())
    }
}

/// The emote messages, where **the object id comes first** — the opposite of the speech messages.
macro_rules! hear_emote {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct $name {
            pub sender: ObjectId,
            pub sender_name: String,
            pub text: String,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self {
                    sender: ObjectId(r.u32()?),
                    sender_name: r.pstring()?,
                    text: r.pstring()?,
                })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.sender.0);
                w.pstring(&self.sender_name)?;
                w.pstring(&self.text)
            }
        }
    };
}

hear_emote!(
    /// `0x01E0 Communication_HearEmote`.
    CommunicationHearEmote,
    COMMUNICATION_HEAR_EMOTE
);
hear_emote!(
    /// `0x01E2 Communication_HearSoulEmote`.
    CommunicationHearSoulEmote,
    COMMUNICATION_HEAR_SOUL_EMOTE
);

/// `0xF7E0 Communication_TextboxString` — system chat.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationTextboxString {
    pub text: String,
    /// `LogTextType`.
    pub text_type: u32,
}

impl Message for CommunicationTextboxString {
    const OPCODE: Opcode = Opcode::COMMUNICATION_TEXTBOX_STRING;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            text: r.pstring()?,
            text_type: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.text)?;
        w.u32(self.text_type);
        Ok(())
    }
}

string_message!(
    /// `0x02EB Communication_TransientString`.
    CommunicationTransientString,
    COMMUNICATION_TRANSIENT_STRING,
    text
);
string_message!(
    /// `0x0004 Communication_PopUpString`.
    CommunicationPopUpString,
    COMMUNICATION_POP_UP_STRING,
    message
);
string_message!(
    /// `0x0317`: one string, shown as a transient line exactly like `0x02EB`.
    CommunicationTransientString0317,
    COMMUNICATION_TRANSIENT_STRING_0317,
    text
);
string_message!(
    /// `0x0318`: one string, shown as a pop-up exactly like `0x0004`.
    CommunicationPopUpString0318,
    COMMUNICATION_POP_UP_STRING_0318,
    message
);

dword_message!(
    /// `0x028A Communication_WeenieError` — the numeric code only.
    ///
    /// The English text lives in the client's own switch; resolve it through the string table
    /// rather than hard-coding a table here.
    CommunicationWeenieError,
    COMMUNICATION_WEENIE_ERROR,
    error_type,
    u32,
    u32,
    u32
);

/// `0x028B Communication_WeenieErrorWithString` — the code plus the `%s` substitution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationWeenieErrorWithString {
    pub error_type: u32,
    pub text: String,
}

impl Message for CommunicationWeenieErrorWithString {
    const OPCODE: Opcode = Opcode::COMMUNICATION_WEENIE_ERROR_WITH_STRING;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            error_type: r.u32()?,
            text: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.error_type);
        w.pstring(&self.text)
    }
}

/// `0x0147 Communication_ChannelBroadcast`, **server to client** — the shape the catalogue
/// mistakenly gives for the C2S half too.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationChannelBroadcastRecv {
    pub channel: u32,
    pub sender_name: String,
    pub message: String,
}

impl Message for CommunicationChannelBroadcastRecv {
    const OPCODE: Opcode = Opcode::COMMUNICATION_CHANNEL_BROADCAST;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            channel: r.u32()?,
            sender_name: r.pstring()?,
            message: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.channel);
        w.pstring(&self.sender_name)?;
        w.pstring(&self.message)
    }
}

/// A `PackableList<string>` reply.
macro_rules! channel_name_list {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct $name {
            pub names: Vec<String>,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { names: r.packed_list(Reader::pstring)? })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.packed_list(&self.names, |w, s| w.pstring(s))
            }
        }
    };
}

channel_name_list!(
    /// `0x0148 Communication_ChannelList`, **server to client**.
    CommunicationChannelListRecv,
    COMMUNICATION_CHANNEL_LIST
);
channel_name_list!(
    /// `0x0149 Communication_ChannelIndex`, **server to client**.
    CommunicationChannelIndexRecv,
    COMMUNICATION_CHANNEL_INDEX
);

/// `0x0295 Communication_ChatRoomTracker`.
///
/// Ten dwords, in exactly this order. This is what binds a Turbine-chat room id to a game channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatRoomMembership {
    pub allegiance_room: u32,
    pub general_room: u32,
    pub trade_room: u32,
    pub lfg_room: u32,
    pub roleplay_room: u32,
    pub olthoi_room: u32,
    pub society_room: u32,
    pub society_celhan_room: u32,
    pub society_eldweb_room: u32,
    pub society_radblo_room: u32,
}

impl Message for ChatRoomMembership {
    const OPCODE: Opcode = Opcode::COMMUNICATION_CHAT_ROOM_TRACKER;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            allegiance_room: r.u32()?,
            general_room: r.u32()?,
            trade_room: r.u32()?,
            lfg_room: r.u32()?,
            roleplay_room: r.u32()?,
            olthoi_room: r.u32()?,
            society_room: r.u32()?,
            society_celhan_room: r.u32()?,
            society_eldweb_room: r.u32()?,
            society_radblo_room: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        for v in [
            self.allegiance_room,
            self.general_room,
            self.trade_room,
            self.lfg_room,
            self.roleplay_room,
            self.olthoi_room,
            self.society_room,
            self.society_celhan_room,
            self.society_eldweb_room,
            self.society_radblo_room,
        ] {
            w.u32(v);
        }
        Ok(())
    }
}

/// A `vlong`: a `u32` limb count then that many little-endian dwords.
///
/// The communication event filter is one of these, used as a 128-bit vector with one bit per message
/// type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VLong(pub Vec<u32>);

impl VLong {
    /// A squelch query for type 1 (`All`) is true only when **every** bit
    /// 0..127 is set; otherwise it is bit `type`.
    #[must_use]
    pub fn is_squelched(&self, msg_type: u32) -> bool {
        if msg_type == 1 {
            (0..128).all(|b| self.bit(b))
        } else {
            self.bit(msg_type)
        }
    }

    #[must_use]
    pub fn bit(&self, index: u32) -> bool {
        let limb = (index / 32) as usize;
        self.0
            .get(limb)
            .is_some_and(|v| v & (1 << (index % 32)) != 0)
    }

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(r.packed_list(Reader::u32)?))
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.packed_list(&self.0, |w, v| {
            w.u32(*v);
            Ok(())
        })
    }
}

/// One squelch entry, pack and unpack.
///
/// The wire order is **`_squelch_msgs`, then `_name`, then `_is_zone_squelch`** — *not* the struct
/// order, which has the zone flag in the middle.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SquelchInfo {
    pub squelch_msgs: VLong,
    pub name: String,
    pub is_zone_squelch: i32,
}

impl SquelchInfo {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            squelch_msgs: VLong::read(r)?,
            name: r.pstring()?,
            is_zone_squelch: r.i32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.squelch_msgs.write(w)?;
        w.pstring(&self.name)?;
        w.i32(self.is_zone_squelch);
        Ok(())
    }
}

/// The squelch database — the three members in this order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SquelchDb {
    /// Squelched account names. Unpacked with duplicate keys discarded, so a
    /// duplicate key from the server does not fail the whole message.
    pub account_hash: PackedHash<String, u32>,
    pub character_hash: PackedHash<u32, SquelchInfo>,
    /// The `/filter` state.
    pub global_squelch_info: SquelchInfo,
}

/// `0x01F4 Communication_SetSquelchDB` — the server's full replacement of the client's state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationSetSquelchDb(pub SquelchDb);

impl SquelchDb {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            account_hash: r.packed_hash(|r| Ok((r.pstring()?, r.u32()?)))?,
            character_hash: r.packed_hash(|r| Ok((r.u32()?, SquelchInfo::read(r)?)))?,
            global_squelch_info: SquelchInfo::read(r)?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.packed_hash(&self.account_hash, |w, k, v| {
            w.pstring(k)?;
            w.u32(*v);
            Ok(())
        })?;
        w.packed_hash(&self.character_hash, |w, k, v| {
            w.u32(*k);
            v.write(w)
        })?;
        self.global_squelch_info.write(w)
    }
}

impl Message for CommunicationSetSquelchDb {
    const OPCODE: Opcode = Opcode::COMMUNICATION_SET_SQUELCH_DB;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(SquelchDb::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w)
    }
}

/// `0x0274 Character_ConfirmationRequest` (S2C).
///
/// `ConfirmationType` 1..=7 map to the seven notices; **any other value is silently ignored** — no
/// dialog appears and the server never gets a response.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CharacterConfirmationRequest {
    pub confirmation_type: i32,
    pub context_id: u32,
    pub text: String,
}

impl CharacterConfirmationRequest {
    /// Whether the client raises a dialog for this type at all.
    #[must_use]
    pub fn is_handled(confirmation_type: i32) -> bool {
        (1..=7).contains(&confirmation_type)
    }
}

impl Message for CharacterConfirmationRequest {
    const OPCODE: Opcode = Opcode::CHARACTER_CONFIRMATION_REQUEST;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            confirmation_type: r.i32()?,
            context_id: r.u32()?,
            text: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.confirmation_type);
        w.u32(self.context_id);
        w.pstring(&self.text)
    }
}

/// `0x0276 Character_ConfirmationDone` (S2C) — the server cancelling a timed-out confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterConfirmationDone {
    pub confirmation_type: i32,
    pub context_id: u32,
}

impl Message for CharacterConfirmationDone {
    const OPCODE: Opcode = Opcode::CHARACTER_CONFIRMATION_DONE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            confirmation_type: r.i32()?,
            context_id: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.confirmation_type);
        w.u32(self.context_id);
        Ok(())
    }
}

/// `0xF7DE Communication_TurbineChat` — **passed through unparsed, on queue 4**.
///
/// The wire format lives in `chatclient.dll`; this is the generic lossless envelope, not the
/// typed request and incoming callback decoder in `crate::turbine`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommunicationTurbineChat {
    /// Everything after the opcode, verbatim.
    pub payload: Vec<u8>,
}

impl Message for CommunicationTurbineChat {
    const OPCODE: Opcode = Opcode::COMMUNICATION_TURBINE_CHAT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            payload: r.rest().to_vec(),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.bytes(&self.payload);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opcodes::Direction;
    use crate::{round_trip, write_body};

    /// `0xF7DE` is passed through unparsed and arrives on **queue 4**.
    /// Oracle: the logon-event queue, which checks that the body is at least four bytes and
    /// starts with `0xF7DE`, and hands the whole blob to the external chat library.
    #[test]
    fn turbine_chat_is_opaque_and_belongs_to_the_logon_queue() {
        let payload = vec![0xDEu8, 0xAD, 0xBE, 0xEF, 0x01];
        let m = CommunicationTurbineChat {
            payload: payload.clone(),
        };
        let bytes = write_body(&m).unwrap();
        assert_eq!(bytes, payload, "not a field is read or rewritten");
        let back: CommunicationTurbineChat = round_trip(&bytes);
        assert_eq!(back.payload, payload);

        let info = Opcode::COMMUNICATION_TURBINE_CHAT.info().unwrap();
        assert_eq!(info.recv_queue, Some(dereth_primitives::NetQueue::Logon));
        assert_eq!(info.direction, Direction::Both);
    }

    /// Oracle: `docs/networking/messages/08-communication.md` §2 — the catalogue's `0x0147` page
    /// lists channel, sender name and message, which is the **S2C** shape. The client's C2S
    /// channel-broadcast request writes only `channel` then `message`.
    #[test]
    fn channel_broadcast_has_a_different_body_in_each_direction() {
        let c2s = CommunicationChannelBroadcast {
            channel: 0x200,
            message: "hi".into(),
        };
        let s2c = CommunicationChannelBroadcastRecv {
            channel: 0x200,
            sender_name: "Bob".into(),
            message: "hi".into(),
        };
        let a = write_body(&c2s).unwrap();
        let b = write_body(&s2c).unwrap();
        assert_ne!(a, b);
        assert!(
            a.len() < b.len(),
            "the S2C form carries the sender name too"
        );
        let _: CommunicationChannelBroadcast = round_trip(&a);
        let _: CommunicationChannelBroadcastRecv = round_trip(&b);
    }

    /// The speech messages put the string first and the id after; the emote messages put the id
    /// first. Getting them the same way round is the easy mistake.
    #[test]
    fn speech_and_emote_order_their_fields_differently() {
        let speech = write_body(&CommunicationHearSpeech {
            message: "hi".into(),
            sender_name: "Bob".into(),
            sender_id: ObjectId(0x5000_0001),
            text_type: 2,
        })
        .unwrap();
        // First two bytes are the message's length prefix, not an id.
        assert_eq!(u16::from_le_bytes([speech[0], speech[1]]), 2);

        let emote = write_body(&CommunicationHearEmote {
            sender: ObjectId(0x5000_0001),
            sender_name: "Bob".into(),
            text: "waves".into(),
        })
        .unwrap();
        assert_eq!(
            u32::from_le_bytes(emote[0..4].try_into().unwrap()),
            0x5000_0001
        );
    }

    /// Oracle: the squelch entry's own unpack — the wire
    /// order is the vlong, the name, then the zone flag, which is not the struct order.
    #[test]
    fn squelch_info_is_not_in_struct_order() {
        let info = SquelchInfo {
            squelch_msgs: VLong(vec![0xFFFF_FFFF, 0, 0, 0]),
            name: "Bob".into(),
            is_zone_squelch: 1,
        };
        let mut w = Writer::body();
        info.write(&mut w).unwrap();
        let bytes = w.into_inner();
        // The first dword is the vlong's limb count, not the zone flag.
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 4);
        let mut r = Reader::body(&bytes);
        assert_eq!(SquelchInfo::read(&mut r).unwrap(), info);
        r.expect_exhausted().unwrap();
    }

    /// Oracle: the squelch query for type 1 (`All`) needs **every** one of the
    /// 128 bits set, not just bit 1.
    #[test]
    fn squelch_all_requires_every_bit() {
        let none = VLong(vec![0, 0, 0, 0]);
        assert!(!none.is_squelched(1));
        let some = VLong(vec![0b10, 0, 0, 0]);
        assert!(some.bit(1));
        assert!(!some.is_squelched(1), "bit 1 alone is not `All`");
        let all = VLong(vec![u32::MAX; 4]);
        assert!(all.is_squelched(1));
        assert!(all.is_squelched(17));
    }

    /// Only confirmation types 1..=7 raise a dialog; anything else is silently ignored and the
    /// server never gets a response.
    #[test]
    fn only_seven_confirmation_types_are_handled() {
        for t in 1..=7 {
            assert!(CharacterConfirmationRequest::is_handled(t));
        }
        assert!(!CharacterConfirmationRequest::is_handled(0));
        assert!(!CharacterConfirmationRequest::is_handled(8));
    }

    #[test]
    fn every_communication_message_round_trips() {
        let _: CommunicationTalk = round_trip(
            &write_body(&CommunicationTalk {
                message: "hello".into(),
            })
            .unwrap(),
        );
        let _: CommunicationTalkDirect = round_trip(
            &write_body(&CommunicationTalkDirect {
                message: "hi".into(),
                target: ObjectId(1),
            })
            .unwrap(),
        );
        let _: CommunicationTalkDirectByName = round_trip(
            &write_body(&CommunicationTalkDirectByName {
                message: "hi".into(),
                target_name: "Bob".into(),
            })
            .unwrap(),
        );
        let _: CommunicationEmote = round_trip(
            &write_body(&CommunicationEmote {
                message: "waves".into(),
            })
            .unwrap(),
        );
        let _: CommunicationSoulEmote = round_trip(
            &write_body(&CommunicationSoulEmote {
                message: "bow".into(),
            })
            .unwrap(),
        );
        let _: CommunicationSetAfkMode =
            round_trip(&write_body(&CommunicationSetAfkMode { afk: 1 }).unwrap());
        let _: CommunicationSetAfkMessage = round_trip(
            &write_body(&CommunicationSetAfkMessage {
                message: "brb".into(),
            })
            .unwrap(),
        );
        let _: CommunicationAddToChannel =
            round_trip(&write_body(&CommunicationAddToChannel { channel: 0x200 }).unwrap());
        let _: CommunicationRemoveFromChannel =
            round_trip(&write_body(&CommunicationRemoveFromChannel { channel: 0x200 }).unwrap());
        let _: CommunicationChannelListRequest =
            round_trip(&write_body(&CommunicationChannelListRequest { channel: 0x200 }).unwrap());
        assert_eq!(
            write_body(&CommunicationChannelIndexRequest).unwrap().len(),
            0
        );
        let _: CommunicationModifyCharacterSquelch = round_trip(
            &write_body(&CommunicationModifyCharacterSquelch {
                add: 1,
                character_id: ObjectId(1),
                character_name: "Bob".into(),
                msg_type: 2,
            })
            .unwrap(),
        );
        let _: CommunicationModifyAccountSquelch = round_trip(
            &write_body(&CommunicationModifyAccountSquelch {
                add: 1,
                character_name: "Bob".into(),
            })
            .unwrap(),
        );
        let _: CommunicationModifyGlobalSquelch = round_trip(
            &write_body(&CommunicationModifyGlobalSquelch {
                add: 1,
                msg_type: 2,
            })
            .unwrap(),
        );
        let _: CommunicationHearRangedSpeech = round_trip(
            &write_body(&CommunicationHearRangedSpeech {
                message: "hi".into(),
                sender_name: "Bob".into(),
                sender_id: ObjectId(1),
                range: 30.0,
                text_type: 2,
            })
            .unwrap(),
        );
        let _: CommunicationHearDirectSpeech = round_trip(
            &write_body(&CommunicationHearDirectSpeech {
                message: "hi".into(),
                sender_name: "Bob".into(),
                sender_id: ObjectId(1),
                target_id: ObjectId(2),
                text_type: 2,
                secret_flags: 0,
            })
            .unwrap(),
        );
        let _: CommunicationHearSoulEmote = round_trip(
            &write_body(&CommunicationHearSoulEmote {
                sender: ObjectId(1),
                sender_name: "Bob".into(),
                text: "bows".into(),
            })
            .unwrap(),
        );
        let _: CommunicationTextboxString = round_trip(
            &write_body(&CommunicationTextboxString {
                text: "sys".into(),
                text_type: 1,
            })
            .unwrap(),
        );
        let _: CommunicationTransientString =
            round_trip(&write_body(&CommunicationTransientString { text: "t".into() }).unwrap());
        let _: CommunicationPopUpString = round_trip(
            &write_body(&CommunicationPopUpString {
                message: "p".into(),
            })
            .unwrap(),
        );
        let _: CommunicationTransientString0317 = round_trip(
            &write_body(&CommunicationTransientString0317 { text: "t".into() }).unwrap(),
        );
        let _: CommunicationPopUpString0318 = round_trip(
            &write_body(&CommunicationPopUpString0318 {
                message: "p".into(),
            })
            .unwrap(),
        );
        // The two later events carry the same body as the two they behave as, byte for byte.
        assert_eq!(
            write_body(&CommunicationTransientString0317 { text: "t".into() }).unwrap(),
            write_body(&CommunicationTransientString { text: "t".into() }).unwrap()
        );
        assert_eq!(CommunicationTransientString0317::OPCODE.0, 0x0317);
        assert_eq!(CommunicationPopUpString0318::OPCODE.0, 0x0318);
        let _: CommunicationWeenieError =
            round_trip(&write_body(&CommunicationWeenieError { error_type: 0x2C2 }).unwrap());
        let _: CommunicationWeenieErrorWithString = round_trip(
            &write_body(&CommunicationWeenieErrorWithString {
                error_type: 0x2C2,
                text: "Bob".into(),
            })
            .unwrap(),
        );
        let _: CommunicationChannelListRecv = round_trip(
            &write_body(&CommunicationChannelListRecv {
                names: vec!["Bob".into(), "Alice".into()],
            })
            .unwrap(),
        );
        let _: CommunicationChannelIndexRecv = round_trip(
            &write_body(&CommunicationChannelIndexRecv {
                names: vec!["General".into()],
            })
            .unwrap(),
        );
        let _: ChatRoomMembership = round_trip(
            &write_body(&ChatRoomMembership {
                allegiance_room: 1,
                general_room: 2,
                ..ChatRoomMembership::default()
            })
            .unwrap(),
        );
        let _: CharacterConfirmationRequest = round_trip(
            &write_body(&CharacterConfirmationRequest {
                confirmation_type: 7,
                context_id: 1,
                text: "Are you sure?".into(),
            })
            .unwrap(),
        );
        let _: CharacterConfirmationResponse = round_trip(
            &write_body(&CharacterConfirmationResponse {
                confirmation_type: 7,
                context_id: 1,
                accepted: 1,
            })
            .unwrap(),
        );
        let _: CharacterConfirmationDone = round_trip(
            &write_body(&CharacterConfirmationDone {
                confirmation_type: 7,
                context_id: 1,
            })
            .unwrap(),
        );
        let _: CommunicationSetSquelchDb = round_trip(
            &write_body(&CommunicationSetSquelchDb(SquelchDb {
                account_hash: PackedHash {
                    table_size: 8,
                    entries: vec![("bob".into(), 1u32)],
                },
                character_hash: PackedHash {
                    table_size: 8,
                    entries: vec![(
                        1u32,
                        SquelchInfo {
                            squelch_msgs: VLong(vec![1, 0, 0, 0]),
                            name: "Bob".into(),
                            is_zone_squelch: 0,
                        },
                    )],
                },
                global_squelch_info: SquelchInfo {
                    squelch_msgs: VLong(vec![0, 0, 0, 0]),
                    name: String::new(),
                    is_zone_squelch: 0,
                },
            }))
            .unwrap(),
        );
    }
}
