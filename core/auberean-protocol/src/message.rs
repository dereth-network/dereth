//! Every message, one enum per direction, dispatched on the type word that opens it.
//!
//! [`ServerMessage::decode`] and [`ClientMessage::decode`] read a whole message and require
//! every byte to be used; `encode` writes it back. A message decoded here encodes to the bytes
//! it was read from (retail padding is always zero; the encoder writes zeros).

use alloc::vec::Vec;

use crate::login::{
    CharacterCreated, CharacterDeleted, CharacterList, CharacterNames, CharacterRef,
    CharacterRefused, ChatPacket, CreateCharacter, DataHeld, DataQuery, DataRecord, DataRefused,
    DataRequest, DeleteCharacter, DownloadBegins, Empty, Notice, PlayCharacter, PlayerObject,
    PlayerQualities, RestoreCharacter, Text, Value,
};
use crate::physics::{
    Attach, ClientPosition, CreateObject, Fx, HeadDirection, Impulse, Leave, Missile, MoveOrder,
    NoAction, ObjectEvent, ObjectOnly, ObjectPosition, PrivateFx, Release, Scale, Slide, Target,
    TargetedFx, TargetedImpulse, TerrainModifier, Word,
};
use crate::quality::{PrivateQuality, VisualQuality};
use crate::script::{ClientCall, ObjectCall, ServerCall};
use crate::text::{NarrowString, StringInfo};
use crate::types::{BehaviorParams, ObjectStamp, PhysicsStory, Placement, VisualDesc};
use crate::wire::{DecodeError, Field, Reader, Result, Writer};

macro_rules! messages {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident($ty:ty) = $code:literal, )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq)]
        pub enum $name {
            $( $(#[$vmeta])* $variant($ty), )*
        }

        impl $name {
            /// The type word this message opens with.
            #[must_use]
            pub const fn message_type(&self) -> u32 {
                match self {
                    $( Self::$variant(_) => $code, )*
                }
            }

            /// Whether a message of this type travels in this direction.
            #[must_use]
            pub const fn is_known_type(message_type: u32) -> bool {
                matches!(message_type, $( $code )|*)
            }

            /// Read one whole message, every byte of it.
            ///
            /// # Errors
            /// The message is not of a known type, or does not read exactly.
            pub fn decode(bytes: &[u8]) -> Result<Self> {
                let mut r = Reader::new(bytes);
                let t = r.u32()?;
                let m = match t {
                    $( $code => Self::$variant(<$ty as Field>::read(&mut r)?), )*
                    _ => return Err(DecodeError::UnknownType(t)),
                };
                r.finish()?;
                Ok(m)
            }

            /// The message's bytes.
            #[must_use]
            pub fn encode(&self) -> Vec<u8> {
                let mut w = Writer::message(self.message_type());
                match self {
                    $( Self::$variant(m) => m.write(&mut w), )*
                }
                w.finish()
            }
        }
    };
}

messages! {
    /// What a server sends.
    ServerMessage {
        // The login connection.
        /// The character has left the world.
        CharExitGame(Empty) = 0x10027,
        CharacterDeleted(CharacterDeleted) = 0x10029,
        /// The account's characters, with their looks.
        CharacterList(CharacterList) = 0x10055,
        CharacterRefused(CharacterRefused) = 0x10056,
        CharacterCreated(CharacterCreated) = 0x10057,
        /// The account's characters in play, by name.
        CharacterNames(CharacterNames) = 0x100B8,
        WorldName(Text) = 0x100CE,
        Notice(Notice) = 0x100C6,
        AccountBooted(Text) = 0x100B4,
        Chat(ChatPacket) = 0x100D3,
        // The data download.
        DataRecord(DataRecord) = 0x10035,
        DataRefused(DataRefused) = 0x10036,
        DataQuery(DataQuery) = 0x10037,
        DataPatchTimePending(Value) = 0x10039,
        DownloadBegins(DownloadBegins) = 0x1003A,
        DownloadEnds(Empty) = 0x1003B,
        // Entering the world.
        PlayerObject(PlayerObject) = 0x10060,
        PlayerQualities(PlayerQualities) = 0x10068,
        // Script calls.
        PlayerCall(ClientCall) = 0x10066,
        CellObjectCall(ObjectCall) = 0x10067,
        VisualObjectCall(ObjectCall) = 0x100C4,
        PlayerCallAlternate(ClientCall) = 0x100CA,
        // Qualities.
        UpdateInt(PrivateQuality<i32>) = 0x10069,
        UpdateIntVisual(VisualQuality<i32>) = 0x1006A,
        UpdateBool(PrivateQuality<i32>) = 0x1006B,
        UpdateBoolVisual(VisualQuality<i32>) = 0x1006C,
        UpdateFloat(PrivateQuality<f32>) = 0x1006D,
        UpdateFloatVisual(VisualQuality<f32>) = 0x1006E,
        UpdateString(PrivateQuality<NarrowString>) = 0x1006F,
        UpdateStringVisual(VisualQuality<NarrowString>) = 0x10070,
        UpdateDataId(PrivateQuality<u32>) = 0x10071,
        UpdateDataIdVisual(VisualQuality<u32>) = 0x10072,
        UpdateInstanceId(PrivateQuality<u64>) = 0x10073,
        UpdateInstanceIdVisual(VisualQuality<u64>) = 0x10074,
        UpdatePlacement(PrivateQuality<Placement>) = 0x10075,
        UpdatePositionVisual(VisualQuality<Placement>) = 0x10076,
        UpdateTimestamp(PrivateQuality<f64>) = 0x10077,
        UpdateTimestampVisual(VisualQuality<f64>) = 0x10078,
        UpdateStringInfo(PrivateQuality<StringInfo>) = 0x100BC,
        UpdateStringInfoVisual(VisualQuality<StringInfo>) = 0x100BD,
        UpdateLongInt(PrivateQuality<u64>) = 0x100D4,
        UpdateLongIntVisual(VisualQuality<u64>) = 0x100D5,
        // Objects and movement.
        CreateObject(CreateObject) = 0x10058,
        PlayBehavior(ObjectEvent<BehaviorParams>) = 0x1007F,
        StopBehavior(ObjectEvent<Word>) = 0x10080,
        MoveSlider(ObjectEvent<Slide>) = 0x10081,
        DoMode(ObjectEvent<Word>) = 0x10082,
        MoveTo(ObjectEvent<MoveOrder>) = 0x10083,
        Position(ObjectPosition) = 0x10086,
        Parent(ObjectEvent<Attach>) = 0x10087,
        HoverHeight(ObjectEvent<Scale>) = 0x10091,
        VelocityScale(ObjectEvent<Scale>) = 0x10092,
        AccelerationScale(ObjectEvent<Scale>) = 0x10093,
        MotionInterpChange(ObjectEvent<Word>) = 0x10094,
        EndLayer(ObjectEvent<Word>) = 0x10095,
        LookAt(ObjectEvent<Target>) = 0x10096,
        Push(ObjectEvent<Impulse>) = 0x10098,
        PushToward(ObjectEvent<TargetedImpulse>) = 0x10099,
        LetGoBehavior(ObjectEvent<Release>) = 0x1009B,
        UpdateVisualDesc(ObjectEvent<VisualDesc>) = 0x1009F,
        DoFx(ObjectEvent<Fx>) = 0x100A0,
        StopFx(ObjectEvent<Word>) = 0x100A1,
        DoTargetedFx(ObjectEvent<TargetedFx>) = 0x100A2,
        TerrainChange(ObjectEvent<TerrainModifier>) = 0x100A5,
        ClearTerrainChanges(ObjectEvent<NoAction>) = 0x100A6,
        GlobalModifier(ObjectEvent<Word>) = 0x100A7,
        DoStory(ObjectEvent<PhysicsStory>) = 0x100A8,
        SetMode(ObjectEvent<Word>) = 0x100A9,
        InitializeMissile(ObjectEvent<Missile>) = 0x100B5,
        JumpScale(ObjectEvent<Scale>) = 0x100BA,
        DoFxScript(ObjectEvent<Word>) = 0x100BE,
        ObjectLeaves(ObjectEvent<Leave>) = 0x100BF,
        PositionCell(ObjectPosition) = 0x100C1,
        ForgetObject(ObjectOnly) = 0x100C2,
        DeParent(ObjectEvent<ObjectStamp>) = 0x100C3,
        StopAllBehaviors(ObjectEvent<NoAction>) = 0x100C5,
        TurnHead(ObjectEvent<HeadDirection>) = 0x100C9,
        Contain(ObjectOnly) = 0x100CB,
        StopMoveCancelsBehaviors(ObjectEvent<NoAction>) = 0x100CF,
        PrivateFx(PrivateFx) = 0x100D2,
    }
}

messages! {
    /// What a client sends (on the plain queues; see [`ClientBlob`] for the secure ones).
    ClientMessage {
        LogOff(CharacterRef) = 0x10027,
        DeleteCharacter(DeleteCharacter) = 0x10029,
        CreateCharacter(CreateCharacter) = 0x1002A,
        PlayCharacter(PlayCharacter) = 0x1002B,
        DataRequest(DataRequest) = 0x10034,
        DataHeld(DataHeld) = 0x10038,
        DownloadEnds(Empty) = 0x1003B,
        RestoreCharacter(RestoreCharacter) = 0x10051,
        ServerCall(ServerCall) = 0x10053,
        Position(ClientPosition) = 0x1008E,
        ForceCreate(Target) = 0x1008F,
        LookAt(Target) = 0x100C7,
        TurnHead(HeadDirection) = 0x100C8,
        SceneRenderingComplete(Empty) = 0x100D1,
        Chat(ChatPacket) = 0x100D3,
    }
}

/// The queues whose client payloads are encrypted to a key no server holds.
pub const SECURE_QUEUES: core::ops::RangeInclusive<u16> = 6..=8;

/// A client message as it arrives on a queue: a plain message, or an encrypted payload that only
/// the holder of the client's key could read.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientBlob {
    Message(ClientMessage),
    Secure { queue: u16, bytes: Vec<u8> },
}

impl ClientBlob {
    /// Read a client message that arrived on `queue`.
    ///
    /// # Errors
    /// A plain message that does not decode.
    pub fn decode(queue: u16, bytes: &[u8]) -> Result<Self> {
        if SECURE_QUEUES.contains(&queue) {
            return Ok(Self::Secure {
                queue,
                bytes: bytes.to_vec(),
            });
        }
        ClientMessage::decode(bytes).map(Self::Message)
    }

    /// The bytes again.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Message(m) => m.encode(),
            Self::Secure { bytes, .. } => bytes.clone(),
        }
    }
}

/// The type word of a message, if it has one.
#[must_use]
pub fn message_type(bytes: &[u8]) -> Option<u32> {
    bytes
        .get(..4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
