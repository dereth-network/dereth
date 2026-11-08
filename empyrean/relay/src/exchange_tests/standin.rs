//! A stand-in for the ac1999 server, with no socket: the November 1999 client's wire from the relay's
//! own codecs. It logs a client in, pushes made-up dat records and answers the character messages.

use std::collections::VecDeque;

use crate::codec::{
    dat::{DatBegin, DatRecord, Packed},
    objects::CreateObject,
    player::{PlayerDescription, EXPERIENCE_TOTAL_INT},
    write_string, CreateCharacter, CreateResponse, EntryReady, Envelope, News, Roster,
    SelectCharacter,
};
use crate::wire::{encode_generic, Header, KeyStream, Packet};
use dereth_primitives::ObjectId;
use dereth_protocol::{
    archive::PackedHash,
    login::{CharacterIdentity, PlayerModule},
    movement::{MovementPositionEvent, PositionPack},
    objects::{LoginCreatePlayer, ObjectCreatePayload},
    read_checked,
    types::{
        physicsdesc::flags,
        qualities::{
            base_flags, quality_flags, AcBaseQualities, AcQualities, PropertyTables, SpellBookPage,
        },
        PhysicsDesc, PositionWire, PublicWeenieDesc,
    },
    write_blob, Writer,
};
use dereth_transport::{Fragment, FragmentHeader};

/// The world's clock in the stand-in's grant.
pub const WORLD_CLOCK: f64 = 7_000_000.0;
pub const CHARACTER: u32 = 0x5000_0001;
const SERVER_ID: u32 = 11;
pub const PORTAL_RECORD: u32 = 0x0100_0001;
/// The spell the character knows, and the charge the description seeds it with.
pub const KNOWN_SPELL: u32 = 1234;
pub const SEEDED_CHARGE: f32 = 0.6;
/// The portal storm's script in the 1999 files' numbering.
pub const STORM_SCRIPT: u32 = 0x72;
pub const FLAVOR_LINE: &str = "The magic blazes through you!";
pub const HELD_BACK_RECORD: u32 = 0x0E00_0000;

/// A zlib stream holding `data` in one stored block.
fn stored(data: &[u8]) -> Packed {
    let len = u16::try_from(data.len()).unwrap();
    let mut out = vec![0x78, 0x01, 0x01];
    out.extend(len.to_le_bytes());
    out.extend((!len).to_le_bytes());
    out.extend(data);
    out.extend(dereth_dat::inflate::adler32(data).to_be_bytes());
    Packed {
        inflated_len: u32::try_from(data.len()).unwrap(),
        bytes: out,
    }
}

fn opcode_of(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}

/// The stand-in.
pub struct Standin {
    /// Whether it has sent its grant.
    pub granted: bool,
    keys: KeyStream,
    sequence: u32,
    blob: u32,
    out: VecDeque<Vec<u8>>,
    /// The file iterations the login reported: portal and cell.
    pub reported: Option<(u32, u32)>,
    /// The login's text field, for the test to check the credential's form.
    pub login_body: Vec<u8>,
    /// Each whole application message received, in order.
    pub received: Vec<Vec<u8>>,
    /// Whether the client said goodbye.
    pub goodbye: bool,
    switched: bool,
    characters: Vec<CharacterIdentity>,
    assembling: Vec<u8>,
}

impl Standin {
    pub fn new() -> Self {
        Self {
            granted: false,
            keys: KeyStream::new(0),
            sequence: 1,
            blob: 0,
            out: VecDeque::new(),
            reported: None,
            login_body: Vec::new(),
            received: Vec::new(),
            goodbye: false,
            switched: false,
            characters: Vec::new(),
            assembling: Vec::new(),
        }
    }

    /// The next datagram to send.
    pub fn take(&mut self) -> Option<Vec<u8>> {
        self.out.pop_front()
    }

    fn control(&mut self, flags: u32, body: &[u8]) {
        let packet = encode_generic(
            Header {
                flags,
                recipient: SERVER_ID,
                time: 1,
                ..Header::default()
            },
            body,
        )
        .unwrap();
        self.out.push_back(packet);
    }

    /// A message in fragments of 444 bytes, one to a packet.
    fn message(&mut self, queue: u16, bytes: &[u8]) {
        self.blob += 1;
        let parts: Vec<&[u8]> = bytes.chunks(444).collect();
        for (index, part) in parts.iter().enumerate() {
            let f = Fragment::new(
                FragmentHeader {
                    blob_id_low: self.blob,
                    blob_id_high: 0x8000_0000,
                    num_frags: u16::try_from(parts.len()).unwrap(),
                    blob_num: u16::try_from(index).unwrap(),
                    queue_id: queue,
                    ..Default::default()
                },
                part.to_vec(),
            );
            self.sequence += 1;
            let mut h = Header {
                sequence: self.sequence,
                flags: 0x802,
                recipient: SERVER_ID,
                time: 1,
                payload_len: f.header.blob_frag_size,
                ..Header::default()
            };
            h.checksum = h.checksum_base().wrapping_add(f.hash() ^ self.keys.next());
            let mut packet = h.encode().to_vec();
            packet.extend(f.to_bytes());
            self.out.push_back(packet);
        }
    }

    fn roster(&self) -> Vec<u8> {
        write_blob(&Roster {
            characters: self.characters.clone(),
            account: "probe".into(),
        })
        .unwrap()
    }

    fn push_and_list(&mut self) {
        let begin = write_blob(&DatBegin {
            portal_iteration: 105,
            portal_records: 2,
            cell_iteration: 67,
            cell_records: 0,
            portal_bytes: 10,
            cell_bytes: 0,
        })
        .unwrap();
        for _ in 0..3 {
            self.message(7, &begin);
        }
        for id in [PORTAL_RECORD, HELD_BACK_RECORD] {
            let record = write_blob(&DatRecord {
                id,
                cache: 0x27,
                data: stored(b"made up data"),
            })
            .unwrap();
            self.message(7, &record);
        }
        let roster = self.roster();
        self.message(4, &roster);
        let news = write_blob(&News {
            leading: "Stand-in".into(),
            text: "News of the stand-in".into(),
        })
        .unwrap();
        self.message(4, &news);
    }

    fn enter_world(&mut self) {
        let identity = write_blob(&LoginCreatePlayer {
            player_id: ObjectId(CHARACTER),
        })
        .unwrap();
        self.message(3, &identity);
        let player = write_blob(&CreateObject {
            object: ObjectCreatePayload {
                id: ObjectId(CHARACTER),
                physicsdesc: PhysicsDesc {
                    bitfield: flags::SETUP | flags::POSITION,
                    setup_id: Some(0x0200_0001),
                    position: Some(PositionWire {
                        objcell_id: 0x0101_0001,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                wdesc: PublicWeenieDesc {
                    name: "Tester".into(),
                    wcid: 1,
                    icon_id: 0x0600_1036,
                    ..Default::default()
                },
                ..Default::default()
            },
            vendor_class: None,
        })
        .unwrap();
        self.message(3, &player);
        // The second create of a known object is the 1999 update.
        self.message(3, &player);
        let description = write_blob(&PlayerDescription(
            dereth_protocol::login::LoginPlayerDescription {
                qualities: AcQualities {
                    base: AcBaseQualities {
                        flags: base_flags::INT,
                        weenie_type: 0x0A,
                        tables: PropertyTables {
                            ints: Some(PackedHash {
                                table_size: 16,
                                entries: vec![(EXPERIENCE_TOTAL_INT, 500)],
                            }),
                            ..PropertyTables::default()
                        },
                    },
                    flags: quality_flags::SPELL_BOOK,
                    spell_book: Some(PackedHash {
                        table_size: 16,
                        entries: vec![(
                            KNOWN_SPELL,
                            SpellBookPage {
                                casting_likelihood: 0.25,
                                legacy: Some((SEEDED_CHARGE, 0)),
                            },
                        )],
                    }),
                    ..AcQualities::default()
                },
                player_module: PlayerModule::default(),
                ..Default::default()
            },
        ))
        .unwrap();
        self.message(4, &description);
        // A later-only message the server still writes, and one nothing knows.
        self.message(4, &0x002Bu32.to_le_bytes());
        self.message(4, &0x7777u32.to_le_bytes());
    }

    /// What a successful cast sends, as the ac1999 server does.
    fn cast_answered(&mut self) {
        let mut update = 0x004Cu32.to_le_bytes().to_vec();
        update.extend(KNOWN_SPELL.to_le_bytes());
        let update = write_blob(&Envelope {
            object: ObjectId(CHARACTER),
            stamp: 500,
            message: update,
        })
        .unwrap();
        self.message(4, &update);
        let mut line = Writer::new();
        line.u32(0xF62C);
        write_string(&mut line, FLAVOR_LINE, 1024).unwrap();
        line.u32(7);
        self.message(4, &line.into_inner());
        let mut script = 0xF755u32.to_le_bytes().to_vec();
        script.extend(CHARACTER.to_le_bytes());
        script.extend(STORM_SCRIPT.to_le_bytes());
        script.extend(1.0f32.to_le_bytes());
        self.message(3, &script);
    }

    fn application(&mut self, message: Vec<u8>) {
        let opcode = opcode_of(&message);
        match opcode {
            0xF656 => {
                let request = read_checked::<CreateCharacter>(&message).unwrap();
                let tester = CharacterIdentity {
                    gid: ObjectId(CHARACTER),
                    name: request.character.name.clone(),
                    seconds_greyed_out: 0,
                };
                let answer = write_blob(&CreateResponse {
                    code: 1,
                    character: Some(tester.clone()),
                })
                .unwrap();
                self.characters.push(tester);
                self.message(4, &answer);
                let roster = self.roster();
                self.message(4, &roster);
            }
            0xF7C8 => {
                let ready = write_blob(&EntryReady).unwrap();
                self.message(4, &ready);
            }
            0xF657 => {
                read_checked::<SelectCharacter>(&message).unwrap();
                self.enter_world();
            }
            0xF7B1 if opcode_of(&message[8..]) == 0x004A => self.cast_answered(),
            0xF7B1 if opcode_of(&message[8..]) == 0xF61C => {
                // The server's own view of where the player now is, with no `w` in it.
                let position = write_blob(&MovementPositionEvent {
                    id: ObjectId(CHARACTER),
                    position: PositionPack {
                        flags: dereth_protocol::movement::position_flags::ORIENTATION_HAS_NO_W,
                        ..PositionPack::default()
                    },
                })
                .unwrap();
                self.message(3, &position);
            }
            _ => {}
        }
        self.received.push(message);
    }

    /// One datagram from the client.
    pub fn datagram(&mut self, bytes: &[u8]) {
        let header = Header::decode(bytes).unwrap();
        match header.flags & !1 {
            2 if header.sequence == 1 => {
                let body = Packet::decode(bytes).unwrap().body.to_vec();
                assert_eq!(
                    body.len(),
                    96,
                    "the 1999 login is 96 bytes without a ticket"
                );
                let word = |at: usize| u32::from_le_bytes(body[at..at + 4].try_into().unwrap());
                self.reported = Some((word(80), word(84)));
                self.login_body = body;
                let mut grant = 0.0f64.to_le_bytes().to_vec();
                grant.extend(WORLD_CLOCK.to_le_bytes());
                self.granted = true;
                grant.extend(1u32.to_le_bytes());
                grant.extend(7u32.to_le_bytes());
                grant.extend(b"4.35752");
                grant.extend([0; 8]);
                self.control(0x202, &grant);
            }
            0x202 => {
                if !self.switched {
                    self.switched = true;
                    self.control(0x102, &0.0f64.to_le_bytes());
                }
            }
            0x102 => {
                if self.blob == 0 {
                    self.push_and_list();
                }
            }
            0x42 | 0x8002 => self.goodbye = true,
            0x802 => {
                // Past the 20-byte header, the flow word and the 16-byte fragment header.
                let fragment = &bytes[40..];
                let index = u16::from_le_bytes(bytes[36..38].try_into().unwrap());
                let count = u16::from_le_bytes(bytes[32..34].try_into().unwrap());
                if index == 0 {
                    self.assembling.clear();
                }
                self.assembling.extend_from_slice(fragment);
                if index + 1 == count {
                    let message = std::mem::take(&mut self.assembling);
                    self.application(message);
                }
            }
            _ => {}
        }
    }
}
