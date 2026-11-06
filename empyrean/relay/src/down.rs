//! What the ac1999 server sends, as what Dereth reads.
//!
//! Each 1999 message is decoded by the November 1999 client's rules, never by the later table: the
//! two number some things alike that mean different things (`0x0029` is a vital here and a title
//! table there). The state the later wire needs and the 1999 wire never carried lives in [`Down`]:
//! a game-event stamp sequence per object (the client holds back a stamp that leaves a gap for that
//! object), a sequence byte per property, and the objects already created.
//!
//! Motion commands pass as they are. A world on files from before 2015 numbers them as those files
//! do, and Dereth reads a world's commands in that world's numbering. Scripts and sounds pass too: the
//! server sends its files' numbers, and Dereth plays what it receives as sent.

use std::collections::{BTreeMap, HashMap, HashSet};

use dereth_primitives::{NetQueue, ObjectId};
use dereth_protocol::{
    archive::PackedHash,
    combat::{
        AttackerNotification, CombatHandlePlayerDeathEvent, DefenderNotification,
        EvasionAttackerNotification, EvasionDefenderNotification, VictimNotificationOther,
        VictimNotificationSelf,
    },
    comms::{
        CommunicationChannelBroadcastRecv, CommunicationHearDirectSpeech, CommunicationHearEmote,
        CommunicationHearRangedSpeech, CommunicationHearSoulEmote, CommunicationHearSpeech,
        CommunicationSetSquelchDb, CommunicationTextboxString, SquelchDb, SquelchInfo, VLong,
    },
    events::is_game_event,
    items::ItemUpdateStackSize,
    login::{
        CharGenVerificationResponse, LoginCharacterScreenMessage, LoginCharacterSet, LoginWorldInfo,
    },
    movement::{
        position_flags, MovementPositionAndMovementEvent, MovementPositionEvent, PositionPack,
    },
    objects::{ItemCreateObject, ItemDeleteObject, ItemUpdateObject, LoginCreatePlayer},
    qualities as later,
    qualities::{AlignedString, PrivateUpdate, PublicUpdate},
    read_body, read_body_padded,
    trade::{
        MiscPortalStorm, MiscPortalStormBrewing, MiscPortalStormImminent, MiscPortalStormSubsided,
    },
    types::qualities::{Enchantment, SecondaryAttribute, Skill},
    write_blob, Opcode, OrderedEventHeader, Reader, Writer,
};
use empyrean_net::{GameMessageGroup, OutboundMessage};

use crate::codec::{
    self,
    objects::{CreateObject, ObjDescEvent},
    player::{self, PlayerDescription},
    CreateResponse, Envelope, News, Roster,
};

/// The level a portal storm's warning carries on the later wire, the value Empyrean's own storms send.
/// The 1999 warnings carry none, and the client only asks whether it is above zero.
const STORM_WARNING_LEVEL: f32 = 1.0;
/// A character list holds at least this many: the 1999 wire does not say.
const MIN_CHARACTER_SLOTS: i32 = 5;
/// Chat type of what is said aloud, and of a tell (the same on ac1999 and the later client).
const LINE_SPEECH: u32 = 2;
const LINE_TELL: u32 = 3;
/// The property a stack's size is on both wires: its sequence is shared with any update of that Int.
const STACK_SIZE_INT: u32 = 12;
/// `ITEM_TYPE` of a spell component.
const SPELL_COMPONENT_TYPE: u32 = 0x1000;
/// Chat type of a system line.
pub const SYSTEM_TEXT: u32 = 0;

/// How one upstream opcode fared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Count {
    pub messages: u64,
    pub bytes: u64,
}

impl Count {
    fn add(&mut self, bytes: usize) {
        self.messages += 1;
        self.bytes += bytes as u64;
    }
}

/// What became of the messages one way: by opcode, translated, dropped by policy, or unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    pub taken: BTreeMap<u32, Count>,
    pub dropped: BTreeMap<u32, Count>,
    pub unknown: BTreeMap<u32, Count>,
    pub malformed: BTreeMap<u32, Count>,
}

/// One translated message: what the login sequencing needs held apart, or a message for Dereth.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// For Dereth now.
    Later(OutboundMessage),
    /// The character list, which waits for the data download to end.
    Roster(Roster),
    /// The character screen's news, which waits with it.
    News(News),
    /// A dat download message, for the exchange that collects them.
    Begin(codec::dat::DatBegin),
    Record(Box<codec::dat::DatRecord>),
    CellRecord(Box<codec::dat::CellRecord>),
    CellPair(Box<codec::dat::CellPair>),
    Touch(codec::dat::DatTouch),
    /// The server's answer to the request to enter: the character may be named.
    EntryReady,
    /// The character's option word.
    Options(u32),
    /// The server asked a question.
    Question,
}

/// The session's translation state.
#[derive(Debug, Default)]
pub struct Down {
    pub player: Option<ObjectId>,
    stamps: HashMap<ObjectId, u32>,
    sequences: HashMap<(u32, u32), u8>,
    created: HashSet<ObjectId>,
    /// The last event stamp taken from the server, per object.
    upstream_stamps: BTreeMap<ObjectId, u32>,
    /// Who is squelched, by id, as the server has told it one at a time.
    squelched: BTreeMap<u32, String>,
    pub tally: Tally,
}

/// The queue a later message travels on, from the protocol's own table.
#[must_use]
pub fn group_of(opcode: u32) -> GameMessageGroup {
    // The overlay manifest is this project's own and is not in the retail table; the client reads
    // it with the rest of the download.
    if opcode == dereth_protocol::Opcode::DDD_OVERLAY_MANIFEST.0 {
        return GameMessageGroup::DatabaseQueue;
    }
    match Opcode(opcode).info().and_then(|i| i.recv_queue) {
        Some(NetQueue::Control) => GameMessageGroup::ControlQueue,
        Some(NetQueue::Weenie) => GameMessageGroup::WeenieQueue,
        Some(NetQueue::Logon) => GameMessageGroup::LoginQueue,
        Some(NetQueue::ClientCache) => GameMessageGroup::DatabaseQueue,
        Some(NetQueue::WorldObjects) => GameMessageGroup::SmartboxQueue,
        _ => GameMessageGroup::UIQueue,
    }
}

/// A message for Dereth from its encoded form.
#[must_use]
pub fn plain(data: Vec<u8>) -> OutboundMessage {
    let opcode = data
        .get(..4)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u32::from_le_bytes);
    OutboundMessage {
        group: group_of(opcode),
        data,
    }
}

/// A system line in Dereth's chat.
///
/// # Panics
/// Never for text a Rust string can hold: the later string has no limit the relay reaches.
#[must_use]
pub fn system_line(text: &str) -> OutboundMessage {
    plain(
        write_blob(&CommunicationTextboxString {
            text: text.to_owned(),
            text_type: SYSTEM_TEXT,
        })
        .expect("a system line encodes"),
    )
}

/// A 1999 position pack with its orientation stated: a missing `w` reads zero on that wire and one on
/// the later wire, so a pack that omits it must not omit it any more.
fn stated(mut p: PositionPack, raw_flags: u32) -> PositionPack {
    if raw_flags & position_flags::ORIENTATION_HAS_NO_W != 0 {
        p.orientation.w = 0.0;
        p.flags &= !position_flags::ORIENTATION_HAS_NO_W;
    }
    p
}

/// The flag word of a body whose position pack follows the object id.
fn position_flags_of(body: &[u8]) -> u32 {
    body.get(4..8)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u32::from_le_bytes)
}

/// A 1999 string.
fn text(r: &mut Reader<'_>) -> Result<String, dereth_protocol::MessageError> {
    codec::read_string(r, codec::MAX_NEWS)
}

fn word(body: &[u8], at: usize) -> Option<u32> {
    body.get(at..at + 4)?
        .try_into()
        .ok()
        .map(u32::from_le_bytes)
}

/// Opcodes that pass byte for byte: the same layout on both wires, with no string in it.
const PASS: &[u32] = &[
    // Objects and movement.
    0xF749, 0xF74A, 0xF74B, 0xF74C, 0xF74E, 0xF750, 0xF751, 0xF754, 0xF755, 0xF7DB,
    // Inventory.
    0x0022, 0x0023, 0x0024, 0x0052, 0x0196, 0x019A, 0x01C7, 0x00A0, 0x00B6, 0x00B7, // Combat.
    0x01A7, 0x01B8, 0x01C0, // The session.
    0x01C3, 0x01C9, 0x01CA, 0x01EA, 0xF653, 0xF655, 0xF659, 0xF7C1,
    // The later forms the server still writes.
    0x01D1, 0x01D2, 0x01D3, 0x01D4, 0x01D5, 0x01D6, 0x01D7, 0x01D8, 0x01D9, 0x01DA, 0x01DB, 0x01DC,
    0x01DD, 0x01DE, 0x02B8, 0x02B9, 0x02CF, 0x02D0, 0x02DB, 0x02DC, 0x02E1, 0x028B, 0x02EB, 0xEA60,
];

/// Later-only messages the server still sends for systems a Launch world does not have.
const DROP: &[u32] = &[
    // The answer to a page's change, which the later wire does not have, and a line both clients ignore.
    0x00B5, 0xF630, 0x002B, 0x0075, 0x01C8, 0x01FD, 0x01FE, 0x01FF, 0x0200, 0x0201, 0x0202, 0x0203,
    0x0204, 0x0205, 0x0206, 0x0207, 0x021D, 0x0225, 0x0226, 0x0264, 0x0271, 0x027A, 0x027C, 0x02B4,
    0xF7DE, 0x0051,
];

impl Down {
    /// The next event stamp of the events ordered against `object`.
    fn next_stamp(&mut self, object: ObjectId) -> u32 {
        let stamp = self.stamps.entry(object).or_insert(0);
        *stamp = stamp.wrapping_add(1);
        *stamp
    }

    /// The whole squelch list, as the later wire sends it: every kind of line from each character.
    fn squelch_db(&self) -> CommunicationSetSquelchDb {
        CommunicationSetSquelchDb(SquelchDb {
            character_hash: PackedHash {
                table_size: 16,
                entries: self
                    .squelched
                    .iter()
                    .map(|(id, name)| {
                        (
                            *id,
                            SquelchInfo {
                                squelch_msgs: VLong(vec![u32::MAX]),
                                name: name.clone(),
                                is_zone_squelch: 0,
                            },
                        )
                    })
                    .collect(),
            },
            ..SquelchDb::default()
        })
    }

    /// Who is squelched, by id. A 1999 client keeps the list and shows the server it at each login.
    #[must_use]
    pub fn squelched_ids(&self) -> Vec<u32> {
        self.squelched.keys().copied().collect()
    }

    /// The squelch list as a message for Dereth, when it has anyone on it.
    pub fn squelch_list(&mut self) -> Option<OutboundMessage> {
        if self.squelched.is_empty() {
            return None;
        }
        let data = write_blob(&self.squelch_db()).ok()?;
        Some(self.later(data))
    }

    /// The next sequence byte of one property of one object.
    fn sequence(&mut self, object: u32, key: u32) -> u8 {
        let s = self.sequences.entry((object, key)).or_insert(0);
        *s = s.wrapping_add(1);
        *s
    }

    /// A stack's size, with the next sequence byte of that property.
    fn stack_size(&mut self, item: u32, amount: u32, new_value: u32) -> ItemUpdateStackSize {
        ItemUpdateStackSize {
            sequence: self.sequence(
                item,
                later::stat_type::key(later::stat_type::INT, STACK_SIZE_INT),
            ),
            item: ObjectId(item),
            amount,
            new_value,
        }
    }

    /// A later message on its queue, a game event wrapped with the relay's own stamp.
    pub fn later(&mut self, data: Vec<u8>) -> OutboundMessage {
        let mut out = plain(data);
        if is_game_event(Opcode(out.opcode())) {
            let iid = self.player.unwrap_or(ObjectId(0));
            let header = OrderedEventHeader {
                iid,
                stamp: self.next_stamp(iid),
            };
            let mut w = Writer::new();
            header.write(&mut w);
            let mut wrapped = w.into_inner();
            wrapped.extend_from_slice(&out.data);
            out = OutboundMessage {
                group: GameMessageGroup::UIQueue,
                data: wrapped,
            };
        }
        out
    }

    fn is_player(&self, object: u32) -> bool {
        self.player.is_some_and(|p| p.0 == object)
    }

    /// One whole message from the server.
    pub fn message(&mut self, bytes: &[u8]) -> Vec<Out> {
        let Some(opcode) = word(bytes, 0) else {
            self.tally.malformed.entry(0).or_default().add(bytes.len());
            return Vec::new();
        };
        let body = &bytes[4..];
        match self.translate(opcode, body) {
            Translated::Done(out) => {
                self.tally.taken.entry(opcode).or_default().add(bytes.len());
                out
            }
            Translated::Dropped => {
                let count = self.tally.dropped.entry(opcode).or_default();
                count.add(bytes.len());
                Vec::new()
            }
            Translated::Unknown => {
                let count = self.tally.unknown.entry(opcode).or_default();
                if count.messages == 0 {
                    log::info!(
                        "upstream {opcode:#06X} ({} bytes): not translated",
                        body.len()
                    );
                }
                count.add(bytes.len());
                Vec::new()
            }
            Translated::Malformed(why) => {
                log::warn!(
                    "upstream {opcode:#06X} ({} bytes) did not read: {why}",
                    body.len()
                );
                self.tally
                    .malformed
                    .entry(opcode)
                    .or_default()
                    .add(bytes.len());
                Vec::new()
            }
        }
    }

    fn translate(&mut self, opcode: u32, body: &[u8]) -> Translated {
        match self.try_translate(opcode, body) {
            Ok(t) => t,
            Err(e) => Translated::Malformed(e.to_string()),
        }
    }

    fn try_translate(
        &mut self,
        opcode: u32,
        body: &[u8],
    ) -> Result<Translated, dereth_protocol::MessageError> {
        let done = |out: Out| Ok(Translated::Done(vec![out]));
        if DROP.contains(&opcode) {
            return Ok(Translated::Dropped);
        }
        if PASS.contains(&opcode) {
            let mut data = opcode.to_le_bytes().to_vec();
            data.extend_from_slice(body);
            return done(Out::Later(self.later(data)));
        }
        match opcode {
            0xF7B0 => {
                let envelope = read_body::<Envelope>(body)?;
                let last = self.upstream_stamps.get(&envelope.object).copied();
                if last.is_some_and(|l| envelope.stamp.wrapping_sub(l).cast_signed() <= 0) {
                    return Ok(Translated::Dropped);
                }
                self.upstream_stamps.insert(envelope.object, envelope.stamp);
                let inner = self.message(&envelope.message);
                Ok(Translated::Done(inner))
            }
            0xF658 => done(Out::Roster(read_body(body)?)),
            0xF65A => done(Out::News(read_body(body)?)),
            0xF7C7 => {
                read_body::<codec::EntryReady>(body)?;
                done(Out::EntryReady)
            }
            0xF643 => {
                let r = read_body::<CreateResponse>(body)?;
                let later = CharGenVerificationResponse {
                    response_type: r.code,
                    identity: r.character.unwrap_or_default(),
                };
                done(Out::Later(plain(write_blob(&later)?)))
            }
            0xF7B8 => done(Out::Begin(read_body(body)?)),
            0xF7B7 => done(Out::Record(Box::new(read_body(body)?))),
            0xF7AC => done(Out::CellRecord(Box::new(read_body(body)?))),
            0xF7AB => done(Out::CellPair(Box::new(read_body(body)?))),
            0xF7BB => done(Out::Touch(read_body(body)?)),
            0xF746 => {
                let p = read_body::<LoginCreatePlayer>(body)?;
                self.player = Some(p.player_id);
                done(Out::Later(plain(write_blob(&p)?)))
            }
            0xF745 => {
                let c = read_body::<CreateObject>(body)?;
                // Dereth counts a spell component when its stack changes, and a create is not a
                // change. One that comes straight into a pack in play says its stack as well.
                let stack = (self.player.is_some()
                    && c.object.wdesc.obj_type & SPELL_COMPONENT_TYPE != 0
                    && c.object.wdesc.container_id.is_some())
                .then(|| {
                    (
                        c.object.id.0,
                        c.object.wdesc.stack_size.map_or(1, u32::from),
                        c.object.wdesc.value.unwrap_or(0),
                    )
                });
                // A second create of a known object is the 1999 update.
                let data = if self.created.insert(c.object.id) {
                    write_blob(&ItemCreateObject(c.object))?
                } else {
                    write_blob(&ItemUpdateObject(c.object))?
                };
                let mut out = vec![Out::Later(plain(data))];
                if let Some((item, amount, new_value)) = stack {
                    let data = write_blob(&self.stack_size(item, amount, new_value))?;
                    out.push(Out::Later(self.later(data)));
                }
                Ok(Translated::Done(out))
            }
            0xF747 => {
                let d = read_body_padded::<ItemDeleteObject>(body)?;
                self.created.remove(&d.id);
                done(Out::Later(plain(write_blob(&d)?)))
            }
            0xF625 => {
                let e = read_body::<ObjDescEvent>(body)?.0;
                done(Out::Later(plain(write_blob(&e)?)))
            }
            0xF748 => {
                let mut e = read_body::<MovementPositionEvent>(body)?;
                e.position = stated(e.position, position_flags_of(body));
                done(Out::Later(plain(write_blob(&e)?)))
            }
            0xF619 => {
                let mut e = read_body::<MovementPositionAndMovementEvent>(body)?;
                e.position = stated(e.position, position_flags_of(body));
                done(Out::Later(plain(write_blob(&e)?)))
            }
            0x0013 => {
                let d = read_body::<PlayerDescription>(body)?;
                let options = d.0.player_module.options;
                let data = write_blob(&d.0)?;
                Ok(Translated::Done(vec![
                    Out::Later(self.later(data)),
                    Out::Options(options),
                ]))
            }
            // The later form the server still writes, for a question Dereth can answer.
            0x0274 => {
                let mut data = opcode.to_le_bytes().to_vec();
                data.extend_from_slice(body);
                Ok(Translated::Done(vec![
                    Out::Later(self.later(data)),
                    Out::Question,
                ]))
            }
            // A known spell's update is how the client learns a cast happened, on both wires.
            0x004C => {
                let layered_spell_id =
                    word(body, 0).ok_or_else(|| codec::invalid("ac1999 spell update"))?;
                let data = write_blob(&later::MagicUpdateSpell { layered_spell_id })?;
                done(Out::Later(self.later(data)))
            }
            0x004D => {
                let layered_spell_id =
                    word(body, 0).ok_or_else(|| codec::invalid("ac1999 spell removal"))?;
                let data = write_blob(&later::MagicRemoveSpell { layered_spell_id })?;
                done(Out::Later(self.later(data)))
            }
            // ac1999 sends this: the 1999 enchantment family names an enchantment by its id.
            0x004E | 0x01A5 | 0x004F | 0x01A4 | 0x01A6 | 0x01AE => {
                let mut r = Reader::body(body);
                let data = match opcode {
                    0x004E => {
                        let mut e = Enchantment::read(&mut r)?;
                        e.id = player::layered(e.id);
                        write_blob(&later::MagicUpdateEnchantment(e))?
                    }
                    0x01A5 => {
                        let list = r.packed_list(|r| {
                            let mut e = Enchantment::read(r)?;
                            e.id = player::layered(e.id);
                            Ok(e)
                        })?;
                        write_blob(&later::MagicUpdateMultipleEnchantments(list))?
                    }
                    0x004F => write_blob(&later::MagicRemoveEnchantment {
                        layered_spell_id: player::layered(r.u32()?),
                    })?,
                    0x01A4 => write_blob(&later::MagicDispelEnchantment {
                        layered_spell_id: player::layered(r.u32()?),
                    })?,
                    _ => {
                        let ids = r.packed_list(|r| Ok(player::layered(r.u32()?)))?;
                        if opcode == 0x01A6 {
                            write_blob(&later::MagicRemoveMultipleEnchantments(ids))?
                        } else {
                            write_blob(&later::MagicDispelMultipleEnchantments(ids))?
                        }
                    }
                };
                done(Out::Later(self.later(data)))
            }
            0x009C => {
                let data = write_blob(&MiscPortalStormSubsided)?;
                done(Out::Later(self.later(data)))
            }
            0x009D => {
                let data = write_blob(&MiscPortalStormBrewing {
                    extent: STORM_WARNING_LEVEL,
                })?;
                done(Out::Later(self.later(data)))
            }
            0x009E => {
                let data = write_blob(&MiscPortalStormImminent {
                    extent: STORM_WARNING_LEVEL,
                })?;
                done(Out::Later(self.later(data)))
            }
            // The server's text goes with it: the later client writes that same line itself.
            0x009F => {
                let data = write_blob(&MiscPortalStorm)?;
                done(Out::Later(self.later(data)))
            }
            0x000E => self.update_int(body),
            0x000F => self.update_string(body),
            0x0010 | 0x0011 | 0x0043 | 0x0057 | 0x002C | 0x002F => self.update_word(opcode, body),
            0x002E => self.update_float(body),
            0x0026 => self.update_struct(body, later::stat_type::ATTRIBUTE, 12, 0x02E3, 0x02E4),
            0x0028 => self.update_skill(body),
            0x0029 => self.update_vital(body),
            // The ac1999 server's layout: text, name, id.
            0x0037 => {
                let mut r = Reader::body(body);
                let m = CommunicationHearSpeech {
                    message: text(&mut r)?,
                    sender_name: text(&mut r)?,
                    sender_id: ObjectId(r.u32()?),
                    text_type: LINE_SPEECH,
                };
                done(Out::Later(plain(write_blob(&m)?)))
            }
            // The ac1999 server's layout: text, name, sender, target. The 1999 message has no secret word.
            0x0038 => {
                let mut r = Reader::body(body);
                let m = CommunicationHearDirectSpeech {
                    message: text(&mut r)?,
                    sender_name: text(&mut r)?,
                    sender_id: ObjectId(r.u32()?),
                    target_id: ObjectId(r.u32()?),
                    text_type: LINE_TELL,
                    secret_flags: 0,
                };
                done(Out::Later(plain(write_blob(&m)?)))
            }
            // The ac1999 server's layout: name first, then text, id and range.
            0x01B5 => {
                let mut r = Reader::body(body);
                let sender_name = text(&mut r)?;
                let m = CommunicationHearRangedSpeech {
                    message: text(&mut r)?,
                    sender_name,
                    sender_id: ObjectId(r.u32()?),
                    range: r.f32()?,
                    text_type: LINE_SPEECH,
                };
                done(Out::Later(plain(write_blob(&m)?)))
            }
            0x01E0 | 0x01E2 => {
                let mut r = Reader::body(body);
                let sender = ObjectId(r.u32()?);
                let sender_name = text(&mut r)?;
                let line = text(&mut r)?;
                let data = if opcode == 0x01E0 {
                    write_blob(&CommunicationHearEmote {
                        sender,
                        sender_name,
                        text: line,
                    })?
                } else {
                    write_blob(&CommunicationHearSoulEmote {
                        sender,
                        sender_name,
                        text: line,
                    })?
                };
                done(Out::Later(plain(data)))
            }
            // The ac1999 server's layout. A line with no name carries a closing word nothing reads.
            0x014A => {
                let mut r = Reader::body(body);
                let m = CommunicationChannelBroadcastRecv {
                    channel: r.u32()?,
                    sender_name: text(&mut r)?,
                    message: text(&mut r)?,
                };
                let data = write_blob(&m)?;
                done(Out::Later(self.later(data)))
            }
            // ac1999 sends the later string form.
            0x0016 => {
                let mut data = 0x02EBu32.to_le_bytes().to_vec();
                data.extend_from_slice(body);
                done(Out::Later(self.later(data)))
            }
            // ac1999 sends the code alone, flat. It is the same number on the later wire.
            0x001C => {
                let mut data = 0x028Au32.to_le_bytes().to_vec();
                data.extend_from_slice(body);
                done(Out::Later(self.later(data)))
            }
            // ac1999 sends one name at a time; the later wire sends the whole list every time.
            0x005A | 0x005B => {
                let mut r = Reader::body(body);
                let name = text(&mut r)?;
                let id = r.u32()?;
                if opcode == 0x005A {
                    self.squelched.insert(id, name);
                } else {
                    self.squelched.remove(&id);
                }
                let data = write_blob(&self.squelch_db())?;
                done(Out::Later(self.later(data)))
            }
            // The ac1999 server's layout: the line, the one killed, the killer.
            0x019E => {
                let mut r = Reader::body(body);
                let m = CombatHandlePlayerDeathEvent {
                    message: text(&mut r)?,
                    killed: ObjectId(r.u32()?),
                    killer: ObjectId(r.u32()?),
                };
                let data = write_blob(&m)?;
                done(Out::Later(self.later(data)))
            }
            // ac1999 sends this, in its Release mode: the whole line.
            0x01AC | 0x01AD => {
                let mut r = Reader::body(body);
                let message = text(&mut r)?;
                let data = if opcode == 0x01AC {
                    write_blob(&VictimNotificationSelf { message })?
                } else {
                    write_blob(&VictimNotificationOther { message })?
                };
                done(Out::Later(self.later(data)))
            }
            // The ac1999 server's layout: the f64 is the percent of health, the word after it the damage.
            0x01B1 => {
                let mut r = Reader::body(body);
                let m = AttackerNotification {
                    defender_name: text(&mut r)?,
                    damage_type: r.u32()?,
                    percent: r.f64()?,
                    damage: r.u32()?,
                    critical: r.u32()?,
                    attack_conditions: r.u32()?,
                    attack_conditions_high: r.u32()?,
                };
                let data = write_blob(&m)?;
                done(Out::Later(self.later(data)))
            }
            0x01B2 => {
                let mut r = Reader::body(body);
                let m = DefenderNotification {
                    attacker_name: text(&mut r)?,
                    damage_type: r.u32()?,
                    percent: r.f64()?,
                    damage: r.u32()?,
                    damage_location: r.u32()?,
                    critical: r.u32()?,
                    attack_conditions: r.u32()?,
                    attack_conditions_high: r.u32()?,
                };
                let data = write_blob(&m)?;
                done(Out::Later(self.later(data)))
            }
            0x01B3 => {
                let mut r = Reader::body(body);
                let data = write_blob(&EvasionAttackerNotification {
                    defender_name: text(&mut r)?,
                })?;
                done(Out::Later(self.later(data)))
            }
            0x01B4 => {
                let mut r = Reader::body(body);
                let data = write_blob(&EvasionDefenderNotification {
                    attacker_name: text(&mut r)?,
                })?;
                done(Out::Later(self.later(data)))
            }
            // The ac1999 server's layout: object, count, value. The later form adds a sequence byte.
            0x0197 => {
                let (Some(item), Some(amount), Some(new_value)) =
                    (word(body, 0), word(body, 4), word(body, 8))
                else {
                    return Err(codec::invalid("ac1999 stack size"));
                };
                let data = write_blob(&self.stack_size(item, amount, new_value))?;
                done(Out::Later(self.later(data)))
            }
            0x00C9 => {
                let data = write_blob(&read_body::<codec::appraise::AppraiseInfo>(body)?.0)?;
                done(Out::Later(self.later(data)))
            }
            0x0062 => {
                let data = write_blob(&read_body::<codec::social::Vendor>(body)?.0)?;
                done(Out::Later(self.later(data)))
            }
            0x0020 => {
                let data = write_blob(&read_body::<codec::social::Update>(body)?.0)?;
                done(Out::Later(self.later(data)))
            }
            0x00AF => {
                let data = write_blob(&read_body::<codec::social::FullUpdate>(body)?.0)?;
                done(Out::Later(self.later(data)))
            }
            0x00B0 => {
                let data = write_blob(&read_body::<codec::social::UpdateFellow>(body)?.0)?;
                done(Out::Later(self.later(data)))
            }
            // The ac1999 server's layout: who left, who was dismissed, and the end of it.
            0x00A7 | 0x00B1 | 0x00B3 => {
                let later_opcode: u32 = match opcode {
                    0x00A7 => 0x00A3,
                    0x00B1 => 0x00A4,
                    _ => 0x02BF,
                };
                let mut data = later_opcode.to_le_bytes().to_vec();
                data.extend_from_slice(body);
                done(Out::Later(self.later(data)))
            }
            0x00B4 => {
                let data = write_blob(&read_body::<codec::book::BookOpen>(body)?.0)?;
                done(Out::Later(self.later(data)))
            }
            0x00B8 => {
                let data = write_blob(&read_body::<codec::book::PageResponse>(body)?.0)?;
                done(Out::Later(self.later(data)))
            }
            0xF62C => {
                let mut r = dereth_protocol::Reader::body(body);
                let text = codec::read_string(&mut r, codec::MAX_NEWS)?;
                let line = CommunicationTextboxString {
                    text,
                    text_type: r.u32()?,
                };
                let data = write_blob(&line)?;
                done(Out::Later(self.later(data)))
            }
            _ => Ok(Translated::Unknown),
        }
    }

    /// `0x000E`: an Int. Experience becomes its Int64, the combat mode its bits.
    fn update_int(&mut self, body: &[u8]) -> Result<Translated, dereth_protocol::MessageError> {
        let (Some(object), Some(key), Some(value)) = (word(body, 0), word(body, 4), word(body, 8))
        else {
            return Err(codec::invalid("ac1999 int update"));
        };
        let value = value.cast_signed();
        if key == player::EXPERIENCE_TOTAL_INT || key == player::EXPERIENCE_AVAILABLE_INT {
            let int64 = if key == player::EXPERIENCE_TOTAL_INT {
                player::EXPERIENCE_TOTAL_INT64
            } else {
                player::EXPERIENCE_AVAILABLE_INT64
            };
            let sequence = self.sequence(
                object,
                later::stat_type::key(later::stat_type::INT64, int64),
            );
            let mut data = 0x02CFu32.to_le_bytes().to_vec();
            data.push(sequence);
            data.extend(int64.to_le_bytes());
            data.extend(i64::from(value).to_le_bytes());
            return Ok(Translated::Done(vec![Out::Later(self.later(data))]));
        }
        let value = if key == player::COMBAT_MODE_INT {
            player::combat_mode_to_later(value)
        } else {
            value
        };
        Ok(self.scalar(
            object,
            key,
            later::stat_type::INT,
            0x02CD,
            0x02CE,
            &value.to_le_bytes(),
        ))
    }

    /// One update of a fixed-size value: the private form for the player, the public one otherwise.
    fn scalar(
        &mut self,
        object: u32,
        key: u32,
        stat: u32,
        private: u32,
        public: u32,
        value: &[u8],
    ) -> Translated {
        let sequence = self.sequence(object, later::stat_type::key(stat, key));
        let mut data;
        if self.is_player(object) {
            data = private.to_le_bytes().to_vec();
            data.push(sequence);
        } else {
            data = public.to_le_bytes().to_vec();
            data.push(sequence);
            data.extend(object.to_le_bytes());
        }
        data.extend(key.to_le_bytes());
        data.extend_from_slice(value);
        Translated::Done(vec![Out::Later(self.later(data))])
    }

    fn update_word(
        &mut self,
        opcode: u32,
        body: &[u8],
    ) -> Result<Translated, dereth_protocol::MessageError> {
        use later::stat_type as s;
        let (stat, private, public) = match opcode {
            0x0010 => (s::SKILL, 0x02DF, 0x02E0),
            0x0011 => (s::DID, 0x02D7, 0x02D8),
            0x0043 => (s::BOOL, 0x02D1, 0x02D2),
            0x0057 => (s::IID, 0x02D9, 0x02DA),
            0x002C => (s::ATTRIBUTE, 0x02E5, 0x02E6),
            _ => (s::SKILL, 0x02E1, 0x02E2),
        };
        let (Some(object), Some(key), Some(value)) =
            (word(body, 0), word(body, 4), body.get(8..12))
        else {
            return Err(codec::invalid("ac1999 word update"));
        };
        Ok(self.scalar(object, key, stat, private, public, value))
    }

    fn update_float(&mut self, body: &[u8]) -> Result<Translated, dereth_protocol::MessageError> {
        let (Some(object), Some(key), Some(value)) =
            (word(body, 0), word(body, 4), body.get(8..16))
        else {
            return Err(codec::invalid("ac1999 float update"));
        };
        Ok(self.scalar(object, key, later::stat_type::FLOAT, 0x02D3, 0x02D4, value))
    }

    fn update_struct(
        &mut self,
        body: &[u8],
        stat: u32,
        size: usize,
        private: u32,
        public: u32,
    ) -> Result<Translated, dereth_protocol::MessageError> {
        let (Some(object), Some(key), Some(value)) =
            (word(body, 0), word(body, 4), body.get(8..8 + size))
        else {
            return Err(codec::invalid("ac1999 attribute update"));
        };
        Ok(self.scalar(object, key, stat, private, public, value))
    }

    /// `0x0028`: the skill's later form marks its format in the top half of the level word.
    fn update_skill(&mut self, body: &[u8]) -> Result<Translated, dereth_protocol::MessageError> {
        let (Some(object), Some(key)) = (word(body, 0), word(body, 4)) else {
            return Err(codec::invalid("ac1999 skill update"));
        };
        let mut r = dereth_protocol::Reader::body(&body[8..]);
        let mut skill = Skill::read(&mut r)?;
        skill.format_version = 1;
        let mut w = Writer::new();
        skill.write(&mut w);
        Ok(self.scalar(
            object,
            key,
            later::stat_type::SKILL,
            0x02DD,
            0x02DE,
            &w.into_inner(),
        ))
    }

    /// `0x0029`: a vital. The 1999 key names a maximum (odd) or a current value (even); the later wire
    /// sends the whole attribute for the first and only the level for the second.
    fn update_vital(&mut self, body: &[u8]) -> Result<Translated, dereth_protocol::MessageError> {
        let (Some(object), Some(key)) = (word(body, 0), word(body, 4)) else {
            return Err(codec::invalid("ac1999 vital update"));
        };
        let mut r = dereth_protocol::Reader::body(&body[8..]);
        let vital = SecondaryAttribute::read(&mut r)?;
        let stat = later::stat_type::ATTRIBUTE_2ND;
        if key % 2 == 0 {
            return Ok(self.scalar(
                object,
                key,
                stat,
                0x02E9,
                0x02EA,
                &vital.current_level.to_le_bytes(),
            ));
        }
        let mut w = Writer::new();
        vital.write(&mut w);
        Ok(self.scalar(object, key, stat, 0x02E7, 0x02E8, &w.into_inner()))
    }

    /// `0x000F`: a string, on the later wire's own string form.
    fn update_string(&mut self, body: &[u8]) -> Result<Translated, dereth_protocol::MessageError> {
        let mut r = dereth_protocol::Reader::body(body);
        let object = r.u32()?;
        let key = r.u32()?;
        let value = AlignedString(codec::read_string(&mut r, codec::MAX_NEWS)?);
        let sequence = self.sequence(object, later::stat_type::key(later::stat_type::STRING, key));
        let data = if self.is_player(object) {
            write_blob(&later::QualitiesPrivateUpdateString(PrivateUpdate {
                sequence,
                property_id: key,
                value,
            }))?
        } else {
            write_blob(&later::QualitiesUpdateString(PublicUpdate {
                sequence,
                object: ObjectId(object),
                property_id: key,
                value,
            }))?
        };
        Ok(Translated::Done(vec![Out::Later(self.later(data))]))
    }
}

/// The character list as the later message. The 1999 wire names no capacity and no deleted list.
#[must_use]
pub fn character_set(roster: &Roster) -> LoginCharacterSet {
    let held = i32::try_from(roster.characters.len()).unwrap_or(i32::MAX);
    LoginCharacterSet {
        characters: roster.characters.clone(),
        account: roster.account.clone(),
        num_allowed_characters: held.max(MIN_CHARACTER_SLOTS),
        ..LoginCharacterSet::default()
    }
}

/// The world's name, as the later message says it.
#[must_use]
pub fn world_info(name: &str) -> LoginWorldInfo {
    LoginWorldInfo {
        connections: 0,
        max_connections: 0,
        world_name: name.to_owned(),
    }
}

/// The news, as the later message.
#[must_use]
pub fn screen_message(news: &News) -> LoginCharacterScreenMessage {
    LoginCharacterScreenMessage {
        text: news.text.clone(),
        more: String::new(),
    }
}

enum Translated {
    Done(Vec<Out>),
    Dropped,
    Unknown,
    Malformed(String),
}

#[cfg(test)]
mod tests;
