//! ACE: Source/ACE.Server/Network/GameAction/GameActionPacket.cs::GameActionPacket
//! Game-action payloads decode consistently for complete, truncated and extended inputs.
//! Protocol-encoded actions round-trip; Jump reads exactly the 56-byte pack; SetAfkMessage
//! tolerates missing padding.
//! Fixture: explicit values and local in-memory state.

// V232.

use std::fmt::Debug;

use dereth_primitives::ObjectId;
use dereth_protocol::{self as proto, MessageError};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionState;
use empyrean_world::network::managers::inbound_message_manager::{to_boolean, Payload};

use crate::dispatch::{action, decoded, dispatch, with_header, world_with, PLAYER};

// ---- the sweep -----------------------------------------------------------------------------------

/// A small deterministic generator (the sweep must be reproducible).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }

    fn below(&mut self, n: usize) -> usize {
        self.next() as usize % n.max(1)
    }

    /// A byte that tends to be a boundary value: small lengths, the sign bits, the escape.
    fn byte(&mut self) -> u8 {
        const EDGES: [u8; 10] = [0, 1, 2, 3, 4, 5, 0x7F, 0x80, 0xFE, 0xFF];
        if self.next().is_multiple_of(2) {
            EDGES[self.below(EDGES.len())]
        } else {
            (self.next() & 0xFF) as u8
        }
    }
}

/// The texts the samples carry: empty, every length modulo 4, Windows-1252 text, a trailing NUL
/// (a packed terminator), and a long one.
fn texts() -> Vec<String> {
    let mut v: Vec<String> = [
        "",
        "a",
        "ab",
        "abc",
        "abcd",
        "caf\u{e9} \u{2019}q\u{2019} \u{20ac}",
        "a\0",
        "\0",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    v.push("x".repeat(301));
    v
}

/// Every input the sweep feeds a family: each encoded message; every truncation of it after the
/// action header; it with 1..=5 trailing bytes; and seeded mutations (one or two bytes changed,
/// then cut or extended).
fn inputs(messages: &[Vec<u8>], seed: u64) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut rng = Lcg(seed);
    for m in messages {
        for len in 12..=m.len() {
            out.push(m[..len].to_vec());
        }
        for extra in 1..=5 {
            let mut v = m.clone();
            v.extend(std::iter::repeat_n(0xAB, extra));
            out.push(v);
        }
        for _ in 0..300 {
            let mut v = m.clone();
            for _ in 0..=rng.below(2) {
                if v.len() > 12 {
                    let i = 12 + rng.below(v.len() - 12);
                    v[i] = rng.byte();
                }
            }
            match rng.below(3) {
                0 => v.truncate(12 + rng.below(v.len() - 11)),
                1 => v.extend((0..rng.below(8)).map(|_| rng.byte())),
                _ => {}
            }
            out.push(v);
        }
    }
    out
}

type Read<T> = fn(&mut Payload<'_>) -> Result<T, MessageError>;

/// Runs the hand reads and the decode over every input, after the two header reads the game
/// action packet makes: both fail, or both succeed with the same values and the same position.
/// Returns how many inputs read successfully and how many of those ended inside the message's last
/// padding (the reader's skip past the end).
fn agree<T: PartialEq + Debug>(
    name: &str,
    inputs: &[Vec<u8>],
    hand: Read<T>,
    decode: Read<T>,
) -> (usize, usize) {
    let (mut ok, mut past_end) = (0, 0);
    for data in inputs {
        let run = |f: Read<T>| {
            let mut p = Payload::new(data);
            p.read_u32().expect("stamp");
            p.read_u32().expect("action");
            f(&mut p).map(|v| (v, p.position()))
        };
        let (h, d) = (run(hand), run(decode));
        match (&h, &d) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a, b, "{name}: {data:02X?}");
                ok += 1;
                past_end += usize::from(a.1 > data.len());
            }
            (Err(_), Err(_)) => {}
            _ => panic!("{name}: the hand reads gave {h:?}, the decode {d:?}, for {data:02X?}"),
        }
    }
    (ok, past_end)
}

/// A family's sweep: every input agrees, some read and some of those end inside the padding the
/// reader skips past the end of the message.
macro_rules! sweep {
    ($name:literal, $messages:expr, $hand:expr, $decode:expr, padded: $padded:literal) => {{
        let messages: Vec<Vec<u8>> = $messages;
        let inputs = inputs(&messages, 0x5EED ^ $name.len() as u64);
        let (ok, past_end) = agree($name, &inputs, $hand, $decode);
        assert!(ok >= messages.len(), "{}: {} inputs read", $name, ok);
        if $padded {
            assert!(
                past_end > 0,
                "{}: no input ended inside the last padding",
                $name
            );
        } else {
            assert_eq!(past_end, 0, "{}", $name);
        }
    }};
}

fn each_text<M: proto::Message>(make: impl Fn(String) -> M) -> Vec<Vec<u8>> {
    texts().into_iter().map(|t| action(&make(t))).collect()
}

/// A long string (the `0xFFFF` escape to a `u32` length) reads the same: whole, and cut inside its
/// last padding.
#[test]
fn the_long_string_form_reads_as_the_hand_read_did() {
    let long = "y".repeat(0x1_0003);
    let m = action(&proto::comms::CommunicationTalk {
        message: long.clone(),
    });
    let inputs: Vec<Vec<u8>> = (m.len() - 6..=m.len()).map(|n| m[..n].to_vec()).collect();
    let (ok, past_end) = agree(
        "Talk (long)",
        &inputs,
        |p| p.read_string16l(),
        |p| {
            p.decode_padded::<proto::comms::CommunicationTalk>()
                .map(|m| m.message)
        },
    );
    assert_eq!((ok, past_end), (4, 3));
}

/// Communication: Talk, Emote, SoulEmote, SetAFKMessage, TalkDirect, Tell (TalkDirectByName),
/// ChatChannel, ModifyAccountSquelch, ModifyCharacterSquelch.
#[test]
fn communication_actions_decode_as_they_were_read() {
    use proto::comms::*;
    sweep!("Talk", each_text(|message| CommunicationTalk { message }), |p| p.read_string16l(), |p| {
        p.decode_padded::<CommunicationTalk>().map(|m| m.message)
    }, padded: true);
    sweep!("Emote", each_text(|message| CommunicationEmote { message }), |p| p.read_string16l(), |p| {
        p.decode_padded::<CommunicationEmote>().map(|m| m.message)
    }, padded: true);
    sweep!("SoulEmote", each_text(|message| CommunicationSoulEmote { message }), |p| p.read_string16l(), |p| {
        p.decode_padded::<CommunicationSoulEmote>().map(|m| m.message)
    }, padded: true);
    sweep!("SetAfkMessage", each_text(|message| CommunicationSetAfkMessage { message }), |p| p.read_string16l(), |p| {
        p.decode_padded::<CommunicationSetAfkMessage>().map(|m| m.message)
    }, padded: true);
    sweep!(
        "TalkDirect",
        each_text(|message| CommunicationTalkDirect { message, target: ObjectId(0x5000_0002) }),
        |p| Ok((p.read_string16l()?, p.read_u32()?)),
        |p| p.decode::<CommunicationTalkDirect>().map(|m| (m.message, m.target.0)),
        padded: false
    );
    sweep!(
        "Tell",
        each_text(|message| CommunicationTalkDirectByName { target_name: format!("{message} "), message }),
        |p| Ok((p.read_string16l()?, p.read_string16l()?)),
        |p| p.decode_padded::<CommunicationTalkDirectByName>().map(|m| (m.message, m.target_name)),
        padded: true
    );
    sweep!(
        "ChatChannel",
        each_text(|message| CommunicationChannelBroadcast { channel: 0x800, message }),
        |p| Ok((p.read_u32()?, p.read_string16l()?)),
        |p| p.decode_padded::<CommunicationChannelBroadcast>().map(|m| (m.channel, m.message)),
        padded: true
    );
    sweep!(
        "ModifyAccountSquelch",
        each_text(|character_name| CommunicationModifyAccountSquelch { add: 1, character_name }),
        |p| Ok((to_boolean(p.read_u32()?), p.read_string16l()?)),
        |p| p.decode_padded::<CommunicationModifyAccountSquelch>().map(|m| (to_boolean(m.add), m.character_name)),
        padded: true
    );
    sweep!(
        "ModifyCharacterSquelch",
        each_text(|character_name| CommunicationModifyCharacterSquelch {
            add: -1,
            character_id: ObjectId(0x5000_0003),
            character_name,
            msg_type: 3,
        }),
        |p| Ok((to_boolean(p.read_u32()?), p.read_u32()?, p.read_string16l()?, p.read_u32()?)),
        |p| {
            p.decode::<CommunicationModifyCharacterSquelch>()
                .map(|m| (to_boolean(m.add), m.character_id.0, m.character_name, m.msg_type))
        },
        padded: false
    );
}

/// Social: the allegiance actions, AddFriend and Fellowship_Create.
#[test]
fn social_actions_decode_as_they_were_read() {
    use proto::social::*;
    macro_rules! name_only {
        ($label:literal, $ty:ident, $field:ident) => {
            sweep!($label, each_text(|$field| $ty { $field }), |p| p.read_string16l(), |p| {
                p.decode_padded::<$ty>().map(|m| m.$field)
            }, padded: true)
        };
    }
    name_only!("AddAllegianceBan", AllegianceAddAllegianceBan, name);
    name_only!("RemoveAllegianceBan", AllegianceRemoveAllegianceBan, name);
    name_only!("AllegianceInfoRequest", AllegianceInfoRequest, name);
    name_only!(
        "RemoveAllegianceOfficer",
        AllegianceRemoveAllegianceOfficer,
        name
    );
    name_only!(
        "SetAllegianceApprovedVassal",
        AllegianceSetAllegianceApprovedVassal,
        name
    );
    name_only!("SetAllegianceName", AllegianceSetAllegianceName, name);
    name_only!("SetMotd", AllegianceSetMotd, motd);
    sweep!("AddFriend", each_text(|name| SocialAddFriend { name: format!(" {name} ") }), |p| {
        Ok(p.read_string16l()?.trim().to_owned())
    }, |p| p.decode_padded::<SocialAddFriend>().map(|m| m.name.trim().to_owned()), padded: true);
    sweep!(
        "AllegianceChatBoot",
        each_text(|name| AllegianceChatBoot { reason: format!("{name}!"), name }),
        |p| Ok((p.read_string16l()?, p.read_string16l()?)),
        |p| p.decode_padded::<AllegianceChatBoot>().map(|m| (m.name, m.reason)),
        padded: true
    );
    sweep!(
        "AllegianceChatGag",
        each_text(|name| AllegianceChatGag { name, gagged: 2 }),
        |p| Ok((p.read_string16l()?, to_boolean(p.read_u32()?))),
        |p| p.decode::<AllegianceChatGag>().map(|m| (m.name, to_boolean(m.gagged))),
        padded: false
    );
    sweep!(
        "BreakAllegianceBoot",
        each_text(|name| AllegianceBreakAllegianceBoot { name, account_boot: 1 }),
        |p| Ok((p.read_string16l()?, to_boolean(p.read_u32()?))),
        |p| p.decode::<AllegianceBreakAllegianceBoot>().map(|m| (m.name, to_boolean(m.account_boot))),
        padded: false
    );
    sweep!(
        "SetAllegianceOfficer",
        each_text(|name| AllegianceSetAllegianceOfficer { name, level: 3 }),
        |p| Ok((p.read_string16l()?, p.read_u32()?)),
        |p| p.decode::<AllegianceSetAllegianceOfficer>().map(|m| (m.name, m.level)),
        padded: false
    );
    sweep!(
        "SetAllegianceOfficerTitle",
        each_text(|title| AllegianceSetAllegianceOfficerTitle { level: 2, title }),
        |p| Ok((p.read_u32()?, p.read_string16l()?)),
        |p| p.decode_padded::<AllegianceSetAllegianceOfficerTitle>().map(|m| (m.level, m.title)),
        padded: true
    );
    sweep!(
        "FellowshipCreate",
        each_text(|name| FellowshipCreate { name, share_xp: -1 }),
        |p| Ok((p.read_string16l()?, p.read_u32()? > 0)),
        |p| p.decode::<FellowshipCreate>().map(|m| (m.name, u32::from_ne_bytes(m.share_xp.to_ne_bytes()) > 0)),
        padded: false
    );
}

/// Character permissions and consent: RemoveFromPlayerConsentList, Add/RemovePlayerPermission.
#[test]
fn character_permission_actions_decode_as_they_were_read() {
    use proto::admin::*;
    sweep!("RemoveFromPlayerConsentList", each_text(|name| CharacterRemoveFromPlayerConsentList { name }), |p| {
        p.read_string16l()
    }, |p| p.decode_padded::<CharacterRemoveFromPlayerConsentList>().map(|m| m.name), padded: true);
    sweep!("AddPlayerPermission", each_text(|name| CharacterAddPlayerPermission { name }), |p| p.read_string16l(), |p| {
        p.decode_padded::<CharacterAddPlayerPermission>().map(|m| m.name)
    }, padded: true);
    sweep!("RemovePlayerPermission", each_text(|name| CharacterRemovePlayerPermission { name }), |p| {
        p.read_string16l()
    }, |p| p.decode_padded::<CharacterRemovePlayerPermission>().map(|m| m.name), padded: true);
}

/// Housing, writing and the advocate teleport.
#[test]
fn house_writing_and_advocate_actions_decode_as_they_were_read() {
    use proto::trade::*;
    sweep!("HouseAddPermanentGuest", each_text(|name| HouseAddPermanentGuest { name }), |p| p.read_string16l(), |p| {
        p.decode_padded::<HouseAddPermanentGuest>().map(|m| m.name)
    }, padded: true);
    sweep!("HouseRemovePermanentGuest", each_text(|name| HouseRemovePermanentGuest { name }), |p| {
        p.read_string16l()
    }, |p| p.decode_padded::<HouseRemovePermanentGuest>().map(|m| m.name), padded: true);
    sweep!("HouseBootSpecificGuest", each_text(|name| HouseBootSpecificHouseGuest { name }), |p| {
        p.read_string16l()
    }, |p| p.decode_padded::<HouseBootSpecificHouseGuest>().map(|m| m.name), padded: true);
    sweep!(
        "HouseChangeStoragePermission",
        each_text(|name| HouseChangeStoragePermission { name, has_permission: 1 }),
        |p| Ok((p.read_string16l()?, to_boolean(p.read_u32()?))),
        |p| p.decode::<HouseChangeStoragePermission>().map(|m| (m.name, to_boolean(m.has_permission))),
        padded: false
    );
    sweep!(
        "BookModifyPage",
        each_text(|text| WritingBookModifyPage { book_id: ObjectId(0x8000_0010), page: 0xFFFF_FFFE, text }),
        |p| Ok((p.read_u32()?, p.read_i32()?, p.read_string16l()?)),
        |p| {
            p.decode_padded::<WritingBookModifyPage>()
                .map(|m| (m.book_id.0, i32::from_ne_bytes(m.page.to_ne_bytes()), m.text))
        },
        padded: true
    );
    sweep!(
        "SetInscription",
        each_text(|text| WritingSetInscription { object_id: ObjectId(0x8000_0011), text }),
        |p| Ok((p.read_u32()?, p.read_string16l()?)),
        |p| p.decode_padded::<WritingSetInscription>().map(|m| (m.object_id.0, m.text)),
        padded: true
    );
    let destination = proto::types::PositionWire {
        objcell_id: 0xA9B4_0017,
        frame: proto::types::Frame {
            origin: proto::types::Vec3 {
                x: 84.0,
                y: 7.5,
                z: 94.005,
            },
            orientation: proto::types::Quat {
                w: 1.0,
                x: 0.0,
                y: 0.0,
                z: -0.0,
            },
        },
    };
    sweep!(
        "AdvocateTeleport",
        each_text(|target_name| AdvocateTeleport { target_name, destination }),
        |p| Ok((p.read_string16l()?, format!("{:?}", p.decode_with(proto::types::PositionWire::read)?))),
        |p| p.decode::<AdvocateTeleport>().map(|m| (m.target_name, format!("{:?}", m.destination))),
        padded: false
    );
}

// ---- through the dispatch ----------------------------------------------------------------------

fn through<M: proto::Message + Debug>(w: &mut empyrean_world::World, m: M) {
    let (_, reads) = dispatch(w, action(&m));
    assert_eq!(reads, with_header(M::OPCODE.0, vec![decoded(&m)]), "{m:?}");
}

/// Each converted action, encoded by `dereth-protocol`, reaches its handler's decode through the
/// server's dispatch: the handler read the record that was sent (the player is not in the world
/// here, so each stops at its first use of the player, caught).
#[test]
fn every_converted_action_decodes_what_dereth_protocol_encoded() {
    use proto::{admin::*, comms::*, social::*, trade::*};
    let w = &mut world_with(SessionState::WorldConnected, true);
    let t = || "caf\u{e9} \u{2019}n\u{2019}".to_owned();
    let id = ObjectId(0x5000_0002);

    through(w, CommunicationTalk { message: t() });
    through(w, CommunicationEmote { message: t() });
    through(w, CommunicationSoulEmote { message: t() });
    through(w, CommunicationSetAfkMessage { message: t() });
    through(
        w,
        CommunicationTalkDirect {
            message: t(),
            target: id,
        },
    );
    through(
        w,
        CommunicationTalkDirectByName {
            message: t(),
            target_name: t(),
        },
    );
    through(
        w,
        CommunicationChannelBroadcast {
            channel: 0x800,
            message: t(),
        },
    );
    through(
        w,
        CommunicationModifyAccountSquelch {
            add: 1,
            character_name: t(),
        },
    );
    through(
        w,
        CommunicationModifyCharacterSquelch {
            add: 1,
            character_id: id,
            character_name: t(),
            msg_type: 3,
        },
    );

    through(w, AllegianceAddAllegianceBan { name: t() });
    through(w, AllegianceRemoveAllegianceBan { name: t() });
    through(w, AllegianceInfoRequest { name: t() });
    through(w, AllegianceRemoveAllegianceOfficer { name: t() });
    through(w, AllegianceSetAllegianceApprovedVassal { name: t() });
    through(w, AllegianceSetAllegianceName { name: t() });
    through(w, AllegianceSetMotd { motd: t() });
    through(w, SocialAddFriend { name: t() });
    through(
        w,
        AllegianceChatBoot {
            name: t(),
            reason: t(),
        },
    );
    through(
        w,
        AllegianceChatGag {
            name: t(),
            gagged: 1,
        },
    );
    through(
        w,
        AllegianceBreakAllegianceBoot {
            name: t(),
            account_boot: 1,
        },
    );
    through(
        w,
        AllegianceSetAllegianceOfficer {
            name: t(),
            level: 2,
        },
    );
    through(
        w,
        AllegianceSetAllegianceOfficerTitle {
            level: 2,
            title: t(),
        },
    );
    through(
        w,
        FellowshipCreate {
            name: t(),
            share_xp: 1,
        },
    );

    through(w, CharacterRemoveFromPlayerConsentList { name: t() });
    through(w, CharacterAddPlayerPermission { name: t() });
    through(w, CharacterRemovePlayerPermission { name: t() });

    through(w, HouseAddPermanentGuest { name: t() });
    through(w, HouseRemovePermanentGuest { name: t() });
    through(w, HouseBootSpecificHouseGuest { name: t() });
    through(
        w,
        HouseChangeStoragePermission {
            name: t(),
            has_permission: 1,
        },
    );
    through(
        w,
        WritingBookModifyPage {
            book_id: id,
            page: 1,
            text: t(),
        },
    );
    through(
        w,
        WritingSetInscription {
            object_id: id,
            text: t(),
        },
    );
    through(
        w,
        AdvocateTeleport {
            target_name: t(),
            destination: proto::types::PositionWire::default(),
        },
    );
}

/// Not ACE's (V300, owner 2026-09-24, retail): the jump pack is read as the client lays it out,
/// 56 bytes (extent, velocity, the 32-byte position, the four sequences), and nothing after it:
/// the handler's one read is the whole pack, and a reader over the body ends exactly at its end.
#[test]
fn a_jump_reads_the_clients_56_byte_pack_and_nothing_after_it() {
    use proto::movement::{JumpPack, MoveTimestamps, MovementJump};
    use proto::types::space::{Frame, PositionWire, Quat, Vec3};
    let m = MovementJump(JumpPack {
        extent: 0.75,
        velocity: Vec3 {
            x: 1.0,
            y: -2.0,
            z: 9.5,
        },
        position: PositionWire {
            objcell_id: 0xA9B4_0021,
            frame: Frame {
                origin: Vec3 {
                    x: 12.5,
                    y: 140.25,
                    z: 42.0,
                },
                orientation: Quat {
                    w: 0.5,
                    x: -0.5,
                    y: 0.25,
                    z: 0.75,
                },
            },
        },
        timestamps: MoveTimestamps {
            instance: 0x0102,
            server_control: 0x0304,
            teleport: 0x0506,
            force_position: 0x0708,
        },
    });
    let data = action(&m);
    assert_eq!(
        data.len() - 12,
        56,
        "the header (opcode, stamp, action) then 56 bytes"
    );

    let mut p = Payload::new(&data);
    p.read_u32().expect("stamp");
    p.read_u32().expect("action");
    let got = p.decode::<MovementJump>().expect("decodes");
    assert_eq!(got, m);
    assert_eq!(p.position(), data.len(), "exactly 56 bytes consumed");
    assert_eq!(p.remaining(), 0);
    // the fields sit where the client puts them
    let body = &data[12..];
    assert_eq!(f32::from_le_bytes(body[0..4].try_into().unwrap()), 0.75);
    assert_eq!(
        u32::from_le_bytes(body[16..20].try_into().unwrap()),
        0xA9B4_0021
    );
    assert_eq!(
        f32::from_le_bytes(body[32..36].try_into().unwrap()),
        0.5,
        "the rotation, w first"
    );
    assert_eq!(&body[48..56], &[2, 1, 4, 3, 6, 5, 8, 7]);

    // through the dispatch: one read, the pack, and no reads after it
    let w = &mut world_with(SessionState::WorldConnected, true);
    through(w, m);
}

/// The effect is the hand read's: the AFK message a player sets is the text sent, Windows-1252
/// included, and a message that stops inside the string's padding sets it all the same.
#[test]
fn set_afk_message_sets_what_was_sent_even_without_the_last_padding() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let mut p = empyrean_world::world_objects::world_object::WorldObject::allocate(
        empyrean_world::dispatch::Class::Player,
    );
    p.guid = ObjectGuid::new(PLAYER);
    w.objects.insert(p).expect("fresh");
    let afk = |w: &empyrean_world::World| {
        w.objects
            .get(ObjectGuid::new(PLAYER))
            .and_then(|o| o.afk_message())
    };

    let m = proto::comms::CommunicationSetAfkMessage {
        message: "caf\u{e9}".to_owned(),
    };
    let (_, reads) = dispatch(&mut w, action(&m));
    assert_eq!(reads, with_header(0x0010, vec![decoded(&m)]));
    assert_eq!(afk(&w).as_deref(), Some("caf\u{e9}"));

    // 12 + 2 + 4 bytes: two bytes of padding follow; without them the reader skips past the end.
    let m = proto::comms::CommunicationSetAfkMessage {
        message: "back".to_owned(),
    };
    let mut data = action(&m);
    assert_eq!(data.len(), 20);
    data.truncate(18);
    let (_, reads) = dispatch(&mut w, data);
    assert_eq!(reads, with_header(0x0010, vec![decoded(&m)]));
    assert_eq!(afk(&w).as_deref(), Some("back"));

    // Cut inside the text, it fails as the reader's read did, and the message stays.
    let mut data = action(&proto::comms::CommunicationSetAfkMessage {
        message: "gone".to_owned(),
    });
    data.truncate(17);
    let before = w.sessions.inbound.handler_exceptions;
    dispatch(&mut w, data);
    assert_eq!(w.sessions.inbound.handler_exceptions, before + 1);
    assert_eq!(afk(&w).as_deref(), Some("back"));
}
