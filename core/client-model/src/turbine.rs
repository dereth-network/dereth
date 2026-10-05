//! Client-owned Turbine startup, routing, send guards and incoming callbacks. No sockets or DLL loading.
//! The serializer is the port of the supplied-callback chatclient.dll path, not a second service.
use crate::{
    chat::{is_message_safe, ChatState, TalkFocus},
    Request, RequestSink, World,
};
use dereth_protocol::turbine::{IncomingPayload, SendToRoomById};
use std::collections::BTreeMap;

/// The narrow display format the client's room-chat formatter uses.
pub const ROOM_DISPLAY_FORMAT: &str = "[%ws] <Tell:IIDString:0:%ws>%ws<\\Tell> says, \"%ws\"";
/// The wide format the send-to-room result handler prints. ANY nonzero result prints, not just
/// FAILED(hr).
pub const ROOM_FAILURE_FORMAT: &str = "Failed to send text: [%ws] to room %X.\n";

#[derive(Debug, Clone, Default)]
pub(crate) struct TurbineClient {
    ready: bool,
    context: u32,
    pending: BTreeMap<u32, SendToRoomById>,
    extent_discrepancies: u64,
}

impl ChatState {
    #[must_use]
    pub fn pending_turbine_contexts(&self) -> Vec<u32> {
        self.turbine_client.pending.keys().copied().collect()
    }

    #[must_use]
    pub const fn turbine_extent_discrepancies(&self) -> u64 {
        self.turbine_client.extent_discrepancies
    }

    /// Only the process-owned Turbine members cross ObjectStream's character teardown.
    /// Ending the character session unregisters the quality handler; the chat UI's construction
    /// owns the room tracker, and the spam guard owns the two spam words. The player system's
    /// character log-on clears squelch; focus/name/selection state is NOT moved.
    pub fn preserve_turbine_provider_from(&mut self, old: &mut Self) {
        self.using_turbine_chat = old.using_turbine_chat;
        self.text_conversion = old.text_conversion.clone();
        self.turbine_client = std::mem::take(&mut old.turbine_client);
        self.turbine_spam = std::mem::take(&mut old.turbine_spam);
        self.chat_rooms = std::mem::take(&mut old.chat_rooms);
    }

    /// The character-set notice starts the Turbine chat provider. Local callback/serializer
    /// registration; no connection handshake. Repeated true character-set notices do not reset
    /// the provider counter. A false one does not call this, and does NOT tear down an existing
    /// provider.
    pub fn startup_turbine_chat(&mut self) {
        self.using_turbine_chat = true;
        self.turbine_client.ready = true;
    }

    /// The chat-room tracker message. Ten words are replaced, including zeros; never merge rooms
    /// from a previous character. The allegiance row is based on room presence, not use flag.
    pub fn recv_chat_room_tracker(&mut self, m: dereth_protocol::comms::ChatRoomMembership) {
        self.chat_rooms = [
            (1, m.allegiance_room),
            (2, m.general_room),
            (3, m.trade_room),
            (4, m.lfg_room),
            (5, m.roleplay_room),
            (6, m.society_room),
            (7, m.society_celhan_room),
            (8, m.society_eldweb_room),
            (9, m.society_radblo_room),
            (10, m.olthoi_room),
        ]
        .into();
        self.set_talk_focus_enabled(TalkFocus::Allegiance, m.allegiance_room != 0);
        if self.wants_allegiance_chat && m.allegiance_room != 0 {
            self.set_talk_focus(TalkFocus::Allegiance);
        }
    }
}

/// The global channel name for a room kind: the client's fixed literal names.
pub fn channel_name(kind: u32) -> &'static str {
    match kind {
        1 => "Allegiance",
        2 => "General",
        3 => "Trade",
        4 => "LFG",
        5 => "Roleplay",
        6 => "Society",
        7 => "Celestial Hand",
        8 => "Eldrytch Web",
        9 => "Radiant Blood",
        10 => "Olthoi",
        _ => "",
    }
}

impl World {
    /// Incoming Turbine chat data, as the chat library delivers it to the client's callbacks.
    /// Provider completion has no dependency on a gameplay UI subscriber. Its display fan-out
    /// is delivered (or discarded for an absent/replaced subscriber) by the host separately.
    /// False means unsupported; the original bytes remain lossless at the SessionEvent/codec
    /// boundary. Do not create an unbounded process-owned archive of unrecognized server input.
    pub fn recv_turbine_chat(&mut self, raw: &[u8]) -> bool {
        if !self.chat.turbine_client.ready {
            return true;
        }
        let Ok(packet) = dereth_protocol::turbine::decode_incoming(raw) else {
            return false;
        };
        if packet.ace_overstated_extent {
            self.chat.turbine_client.extent_discrepancies += 1;
        }
        match packet.payload {
            IncomingPayload::RoomResponse(response) => {
                // The pending request is removed BEFORE the response is handled.
                // Duplicates/missing contexts have no callback and cannot print a second error.
                if let Some(request) = self.chat.turbine_client.pending.remove(&response.context) {
                    if response.result != 0 {
                        self.scroll.add_text_to_scroll(
                            &format!(
                                "Failed to send text: [{}] to room {:X}.\n",
                                request.text, request.room
                            ),
                            0,
                            true,
                            0,
                        );
                    }
                }
            }
            IncomingPayload::RoomEvent(event) => {
                let Some(first) = event.extra.get(..4) else {
                    // Native callback unconditionally dereferences this word. Preserve invalid
                    // input, never emulate that memory-unsafe read or manufacture a sender.
                    return false;
                };
                let sender =
                    dereth_primitives::ObjectId(u32::from_le_bytes(first.try_into().unwrap()));
                // Main callback constructs nul-terminated strings. Keep full Unicode in the
                // wire decoder. Only the callback's conversion copies are transformed.
                let name = event.name.split('\0').next().unwrap_or_default();
                let text = event.text.split('\0').next().unwrap_or_default();
                let Some(narrow) = self
                    .chat
                    .text_conversion
                    .narrow(&text.encode_utf16().collect::<Vec<_>>())
                else {
                    return false;
                };
                // Predicate is ASCII-only; this byte-preserving spelling does not decode ACP.
                if !is_message_safe(&dereth_primitives::text::cp1252::decode(&narrow))
                    || self.chat.is_squelched(sender, "", 1)
                {
                    return true;
                }
                // The room-chat formatter compares this exact order. Zero/duplicate IDs still use
                // its FIRST matching field; neither extra chat_type nor listening options wins.
                let channel = [1, 2, 3, 4, 5, 10, 6, 7, 8, 9].into_iter().find(|kind| {
                    self.chat.chat_rooms.get(kind).copied().unwrap_or(0) == event.room
                });
                if let Some(kind) = channel {
                    let ty = match kind {
                        1 | 10 => 0x12,
                        2..=5 => kind + 0x19,
                        _ => 0x20,
                    };
                    // The formatter's narrow sprintf(%ws) truncates EACH wide field at its
                    // first unconvertible unit; the widening then decodes the bytes as ACP.
                    // The host's `%ws` field conversion, not `dereth_primitives::text`'s directly.
                    let encoding = self.chat.text_conversion.clone();
                    let field =
                        |s: &str| encoding.english_ws_field(&s.encode_utf16().collect::<Vec<_>>());
                    let (Some(channel), Some(name), Some(text)) =
                        (field(channel_name(kind)), field(name), field(text))
                    else {
                        return false;
                    };
                    let mut body = b"[".to_vec();
                    body.extend(channel);
                    body.extend(b"] <Tell:IIDString:0:");
                    body.extend(&name);
                    body.extend(b">");
                    body.extend(name);
                    body.extend(b"<\\Tell> says, \"");
                    body.extend(text);
                    body.push(b'"');
                    let Some(body) = self
                        .chat
                        .text_conversion
                        .widen(&body)
                        .and_then(|u| String::from_utf16(&u).ok())
                    else {
                        return false;
                    };
                    self.scroll.add_text_to_scroll(&body, ty, true, 0);
                }
            }
            IncomingPayload::Unknown => return false,
        }
        true
    }

    /// Send a Turbine chat line, from the ordinary chat-command path or an explicit channel
    /// command.
    /// `now_seconds` is wall-clock Unix time from `time(NULL)`, not simulation or server time.
    /// The return is the retail handler boolean, not a promise that the server received a packet.
    #[allow(clippy::too_many_arguments)]
    pub fn send_turbine_chat(
        &mut self,
        sink: &mut impl RequestSink,
        focus: TalkFocus,
        explicit_command: bool,
        text: &str,
        window: u32,
        now_seconds: i32,
    ) -> bool {
        let room_kind = match focus {
            TalkFocus::Allegiance => 1,
            TalkFocus::General => 2,
            TalkFocus::Trade => 3,
            TalkFocus::Lfg => 4,
            TalkFocus::Roleplay => 5,
            TalkFocus::Society => 6,
            TalkFocus::Olthoi => 10,
            _ => return false,
        };
        let room = self.chat.chat_rooms.get(&room_kind).copied().unwrap_or(0);
        // Retail oddity: all ordinary focuses 7..12 share the Allegiance tail, even though
        // they fetched different room IDs. @general et al. have their own type/option.
        let kind = if explicit_command || room_kind == 10 {
            room_kind
        } else {
            1
        };
        let listening = match kind {
            1 => self.player_system.options.get(27),
            2..=5 => self.player_system.options.get(kind as usize + 33),
            6 => self.player_system.options.get(46),
            10 => self.chat.is_olthoi,
            _ => false,
        };
        if !self.chat.using_turbine_chat || room == 0 {
            self.scroll
                .add_text_to_scroll("Turbine chat is not available.\n", 0, true, window);
            return false;
        }
        if !listening {
            // The failure event's own destination is 0, not the current command window.
            self.scroll.add_text_to_scroll(
                &format!(
                    "You are not listening to the {} channel!\n",
                    channel_name(kind)
                ),
                0,
                true,
                0,
            );
            return true;
        }
        if text.is_empty() || !is_message_safe(text) {
            return false;
        }
        if self.chat.turbine_spam.is_message_spam(now_seconds) {
            // Retail passes a literal 0 for %d (not the bucket or a wait estimate).
            self.scroll.add_text_to_scroll(
                "You must wait 0s before communicating again!",
                0,
                true,
                window,
            );
            return false;
        }
        if !self.chat.turbine_client.ready {
            self.scroll.add_text_to_scroll(
                &format!("Failed to send text to channel: {}\n", channel_name(kind)),
                0,
                true,
                window,
            );
            return false;
        }
        // The send's wide-string conversion consumes the actual narrow command bytes.
        // The existing message String is their bijective byte spelling, NOT decoded ACP.
        let Some(bytes) = dereth_primitives::text::cp1252::encode(text) else {
            return false;
        };
        let Some(text) = self
            .chat
            .text_conversion
            .widen(&bytes)
            .and_then(|u| String::from_utf16(&u).ok())
        else {
            return false;
        };
        // The async dispatcher's add-pending-method precedes serialization/send.
        // Unlike the global game-action stamp, this context is NOT rolled back on failure.
        self.chat.turbine_client.context = self.chat.turbine_client.context.wrapping_add(1);
        let request = SendToRoomById {
            context: self.chat.turbine_client.context,
            room,
            text,
            sender: self.player.unwrap_or_default().0,
            chat_type: kind,
        };
        self.chat
            .turbine_client
            .pending
            .insert(request.context, request.clone());
        sink.send(Request::TurbineChat(request));
        true
    }
}
