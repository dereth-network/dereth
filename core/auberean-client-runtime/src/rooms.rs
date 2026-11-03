//! Rooms: the chat channels' lines, through the world's chat service on the login connection, as
//! the client keeps them and hands them to its scripts.
//!
//! **Which way rooms go.** A room line goes to the world's chat service when the login's character
//! list says the world has one, or the player was started with the option that asks for it
//! ([`RoomChat::use_chat_service`]); otherwise it would go to the friends service, which this
//! client does not carry rooms over. The character list saying so also starts the chat service's
//! client, once for the whole program: it is never started again, and its count of requests runs
//! on across characters and sessions.
//!
//! **Sending.** The player's scripts send a line to a room by its id, with their chat flags (an
//! empty line, or room 0, is refused at once and nothing is sent). The line goes out with what the
//! speaker's own name says about it (its meta letters) and a mark of its own, counted from 1; the
//! client keeps the room and the text until the service answers that mark. The scripts are told
//! whether it went out: not when the chat service was asked for but its client never started.
//!
//! **Answers.** The service answers each line with its mark and a code: the client turns the code
//! into one of its own results (0 for done) and hands it to the player's scripts with the room and
//! the text it kept, which show a line when it is not 0. An answer to a mark nobody waits for is
//! dropped.
//!
//! **Lines heard.** A room's line comes with the room, the speaker's name, the text and what the
//! speaker's client said about the speaker: the meta letters are written after the name in
//! brackets, and the chat flags go with the line (0 when that note does not read). The player's
//! scripts decide where it shows. Nothing reaches them while no player stands; packets of the
//! service before its client started are dropped.

use std::collections::BTreeMap;

use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{native_type, NativeObject, TextInfo};
use auberean_script::value::{tag, NativeId};
use dereth_protocol::turbine::{decode_incoming, ExtraBlob, IncomingPayload, SendToRoomById};

use crate::friends::{no_receiver, player_name, string_at, weenie_result, ChatExtra};
use crate::host::{host, InterfaceArg};
use crate::Client;

/// The interface's functions rooms call on the player's scripts.
pub mod function {
    /// A line's answer: the result, the room, the text.
    pub const RESULT_SEND_TO_ROOM: u32 = 0x00B4_003E;
    /// A room's line heard: the chat flags, the room, the text, the speaker.
    pub const HEAR_FROM_ROOM: u32 = 0x00B4_003F;
}

/// A line waiting for the service's answer: what the scripts are handed back with it.
// D2-00393
#[derive(Clone, Debug, PartialEq, Eq)]
struct Waiting {
    room: u32,
    text: Vec<u16>,
}

/// The chat service's client: the count its marks come from, the lines waiting for their
/// answers, and the packets to send. It lives as long as the client does.
// D2-03931, D2-01665, D2-01655
#[derive(Clone, Debug, Default)]
struct Service {
    context: u32,
    waiting: BTreeMap<u32, Waiting>,
    outbox: Vec<Vec<u8>>,
}

/// What the service brought, for the player's scripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Heard {
    /// A line's answer: the client's own result for the service's code, and the room and text the
    /// line was sent with.
    Result {
        result: u32,
        room: u32,
        text: Vec<u16>,
    },
    /// A room's line: the speaker as the scripts write it, the text, the room, the chat flags.
    Line {
        speaker: Vec<u16>,
        text: Vec<u16>,
        room: u32,
        flags: u32,
    },
}

/// Rooms as the client keeps them.
#[derive(Clone, Debug, Default)]
pub struct RoomChat {
    use_chat_service: bool,
    service: Option<Service>,
}

impl RoomChat {
    /// Rooms go to the chat service from now on (the option the player may be started with).
    pub fn use_chat_service(&mut self) {
        self.use_chat_service = true;
    }

    /// Whether rooms go to the chat service.
    #[must_use]
    pub const fn uses_chat_service(&self) -> bool {
        self.use_chat_service
    }

    /// Whether the chat service's client started.
    #[must_use]
    pub const fn started(&self) -> bool {
        self.service.is_some()
    }

    /// The marks of the lines waiting for their answers.
    #[must_use]
    pub fn waiting(&self) -> Vec<u32> {
        self.service
            .as_ref()
            .map(|s| s.waiting.keys().copied().collect())
            .unwrap_or_default()
    }

    /// The login's character list: when it says the world has a chat service, rooms go to it, and
    /// its client starts (once).
    // D2-01659
    pub fn character_set(&mut self, uses_chat_service: u32) {
        if uses_chat_service != 0 {
            self.use_chat_service = true;
            self.start();
        }
    }

    /// Start the chat service's client, unless it started before: its sink for the service's
    /// answers and lines, and the client itself, ready to send.
    // D2-00104, D2-01698, D2-01675, D2-03929, D2-03932, D2-03930
    fn start(&mut self) {
        if self.service.is_none() {
            self.service = Some(Service::default());
        }
    }

    /// A line to a room, with the scripts' chat flags and what the speaker's own name (its
    /// literal, meta letters kept) says about the speaker. What the scripts are told: 1 when the
    /// line went out, 0 when it is empty, the room is 0 or it could not go; when rooms do not go
    /// to the chat service, 0 with a friends service (it carries no rooms here) and the flags
    /// without one.
    // D2-00665, D2-08024, D2-00661
    pub fn send_to_room(
        &mut self,
        room: u32,
        text: &[u16],
        flags: u32,
        speaker: &[u16],
        friends: bool,
    ) -> u32 {
        if text.is_empty() || room == 0 {
            return 0;
        }
        let extra = ChatExtra::new(speaker, flags).pack();
        let waiting = Waiting {
            room,
            text: text.to_vec(),
        };
        if !self.use_chat_service {
            return if friends { 0 } else { flags };
        }
        let Some(service) = self.service.as_mut() else {
            return 0;
        };
        // The mark is counted before the line is packed, and never given back.
        service.context = service.context.wrapping_add(1);
        let request = SendToRoomById {
            context: service.context,
            room,
            text: String::from_utf16_lossy(text),
            extra: ExtraBlob::Other(extra),
        };
        let Ok(packet) = request.network_packet() else {
            return 0;
        };
        service.waiting.insert(service.context, waiting);
        service.outbox.push(packet);
        1
    }

    /// The packets to send to the chat service, in order.
    // D2-00055
    pub fn take_packets(&mut self) -> Vec<Vec<u8>> {
        self.service
            .as_mut()
            .map(|s| core::mem::take(&mut s.outbox))
            .unwrap_or_default()
    }

    /// A packet from the chat service (the bytes after its count): an answer, handed back with
    /// its line, or a room's line. Nothing before the service's client started, for an answer
    /// nobody waits for, or for a packet that does not read.
    // D2-00054, D2-01666, D2-00392, D2-00394, D2-00395, D2-01697, D2-00348, D2-00697, D2-00701
    pub fn receive(&mut self, packet: &[u8]) -> Option<Heard> {
        let service = self.service.as_mut()?;
        let mut framed = u32::try_from(packet.len()).ok()?.to_le_bytes().to_vec();
        framed.extend_from_slice(packet);
        match decode_incoming(&framed).ok()?.payload {
            IncomingPayload::RoomResponse(r) => {
                let w = service.waiting.remove(&r.context)?;
                Some(Heard::Result {
                    result: weenie_result(r.result),
                    room: w.room,
                    text: w.text,
                })
            }
            IncomingPayload::RoomEvent(e) => {
                let note = ChatExtra::unpack(&e.extra);
                let mut speaker: Vec<u16> = e.name.encode_utf16().collect();
                let flags = note.as_ref().map_or(0, |n| n.flags);
                if let Some(n) = note.filter(|n| !n.meta_letters.is_empty()) {
                    speaker.push(u16::from(b'['));
                    speaker.extend_from_slice(&n.meta_letters);
                    speaker.push(u16::from(b']'));
                }
                Some(Heard::Line {
                    speaker,
                    text: e.text.encode_utf16().collect(),
                    room: e.room,
                    flags,
                })
            }
            IncomingPayload::Unknown => None,
        }
    }
}

impl Client {
    /// Rooms as the client keeps them.
    #[must_use]
    pub fn rooms(&self) -> Option<&RoomChat> {
        self.rooms.as_ref()
    }

    pub fn rooms_mut(&mut self) -> Option<&mut RoomChat> {
        self.rooms.as_mut()
    }

    /// A chat service's packet from the login connection, handed on to the player's scripts.
    pub(crate) fn hear_rooms(&mut self, packet: &[u8]) {
        let Some(heard) = self.rooms.as_mut().and_then(|r| r.receive(packet)) else {
            return;
        };
        match heard {
            // D2-00512, D2-01237
            Heard::Result { result, room, text } => {
                self.ask(
                    function::RESULT_SEND_TO_ROOM,
                    &[
                        InterfaceArg::Word(result),
                        InterfaceArg::Word(room),
                        InterfaceArg::Text(text),
                    ],
                );
            }
            // D2-01104
            Heard::Line {
                speaker,
                text,
                room,
                flags,
            } => {
                let speaker = InterfaceArg::Object {
                    ty: native_type::STRING_INFO,
                    object: NativeObject::TextInfo(TextInfo {
                        has_literal: 1,
                        literal: Some(speaker),
                        ..TextInfo::default()
                    }),
                };
                self.ask(
                    function::HEAR_FROM_ROOM,
                    &[
                        InterfaceArg::Word(flags),
                        InterfaceArg::Word(room),
                        InterfaceArg::Text(text),
                        speaker,
                    ],
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The scripts' request
// ---------------------------------------------------------------------------------------------

/// The interface's package.
const GM_INTERFACE: u32 = 0xB4;

/// A line to a room (`fp[-2]`), its text (`fp[-3]`), its chat flags (`fp[-4]`); what the scripts
/// are told below them.
// D2-00996
fn send_to_room(c: &mut Call<'_>) -> Result<Status, Stop> {
    if no_receiver(c)? {
        return Ok(Status::Crash);
    }
    let room = c.cx.at(-2)?;
    let text = string_at(c, -3)?;
    let flags = c.cx.at(-4)?;
    let h = host(c)?;
    let speaker = player_name(h);
    let friends = h.friends.is_some();
    let told = h
        .rooms
        .as_mut()
        .map_or(0, |r| r.send_to_room(room, &text, flags, &speaker, friends));
    c.write(-5, told, tag::MIXED)?;
    Ok(Status::Success)
}

const NATIVES: &[(u32, u32, NativeFn)] = &[(GM_INTERFACE, 0x33, send_to_room)];

/// The rooms' native: the scripts' line to a room.
pub(crate) fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    NATIVES.iter().map(|(p, n, f)| (NativeId::new(*p, *n), *f))
}
