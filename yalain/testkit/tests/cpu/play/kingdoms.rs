//! Kingdoms: joined at a faction shrine, kept, named in the profile, given their own chat room; the
//! conflict status of where a player is, told as it enters, walks and arrives, and the movement
//! ethereality that follows it.

use auberean_primitives::Vec3;
use auberean_protocol::login::{ChatPacket, Empty};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use dereth_protocol::turbine::{decode_incoming, ExtraBlob, IncomingPayload, SendToRoomById};
use yalain_content::{Conflict, ContentObject, ContentPack, Description, Facts, Kind, Place};
use yalain_testkit::play::{report, server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::kingdoms::{self, kingdom, quality};
use yalain_world::requests::function;
use yalain_world::session::function::{SESSION_START, SHOW_TEXT};
use yalain_world::world::IDLE_MOTION;

const SHRINE: u64 = 0x4002_9900_0000_0063;
const PORTAL: u64 = 0x4002_9900_0000_0056;
const DOMINION_SHRINE: u32 = 0x4700_100D;
const SHRINE_PACKAGE: u32 = 0x13B1;
/// Where every test character starts, and the landblock beside it.
const START: u32 = 0x0299_0109;
const NEXT_LANDBLOCK: u16 = 0x029A;

fn object(id: u64, template: u32, package: u32, destination: Option<Place>) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: START,
        origin: [61.0, 240.2, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

/// A world with a Dominion shrine beside the start, the start's landblock of `here`.
fn world_with_a_shrine(here: Conflict) -> Harness {
    let mut h = Harness::new();
    let mut content = ContentPack {
        objects: vec![object(SHRINE, DOMINION_SHRINE, SHRINE_PACKAGE, None)],
        ..ContentPack::default()
    };
    if here != Conflict::Peace {
        content.conflict.insert((START >> 16) as u16, here);
    }
    h.server.content = content;
    h.server.shrine_packages.insert(SHRINE_PACKAGE);
    h.server
        .shrine_kingdoms
        .insert(SHRINE_PACKAGE, kingdom::DOMINION);
    h
}

/// The kingdom a player in the world holds.
fn kingdom_of(h: &Harness, me: u64) -> Option<u32> {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .and_then(|p| kingdoms::of(&p.qualities))
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(auberean_protocol::login::CharacterRef { character: id }),
    );
    assert!(
        h.run_until(yalain_testkit::play::STEPS, |h| h.bots[b].inbox[before..]
            .iter()
            .any(|r| matches!(
                r.message(),
                Some(ServerMessage::CharacterList(_))
            )))
    );
}

/// Put a player in the world in a kingdom (as a join would leave it).
fn set_kingdom(h: &mut Harness, me: u64, k: u32) {
    for p in h.server.players.values_mut() {
        if p.character == me {
            kingdoms::set_kingdom(&mut p.qualities, k);
        }
    }
}

/// The values of every call of `f` a client was sent since `from`.
fn calls_of(h: &Harness, b: usize, from: usize, f: u32) -> Vec<Vec<Value>> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == f => {
                Some(calls::values(&c.args.as_ref()?.0).unwrap_or_default())
            }
            _ => None,
        })
        .collect()
}

/// The conflict statuses a client was told since `from`.
fn statuses(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    calls_of(h, b, from, kingdoms::STATUS)
        .iter()
        .filter_map(|v| match v.as_slice() {
            [Value::Int(s)] => Some(*s),
            _ => None,
        })
        .collect()
}

/// The visual whole-number qualities of `who` a client was told since `from`: (quality, value).
fn visuals(h: &Harness, b: usize, from: usize, who: u64) -> Vec<(u32, i32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateIntVisual(v) if v.object.id == who => Some((v.quality, v.value)),
            _ => None,
        })
        .collect()
}

/// The string references of the texts a client was shown since `from`, with the string id of a
/// nested variable when there is one.
fn texts(h: &Harness, b: usize, from: usize) -> Vec<((u32, u32), Option<u32>)> {
    calls_of(h, b, from, SHOW_TEXT)
        .iter()
        .filter_map(|v| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => {
                    let nested = t.variables.iter().find_map(|var| match &var.value {
                        auberean_script::native_object::VariableValue::Nested(n) => {
                            Some(n.string_id)
                        }
                        _ => None,
                    });
                    Some(((t.string_id, t.table), nested))
                }
                _ => None,
            })
        })
        .collect()
}

/// The faction channel of each channel list a client was sent since `from` (each list one of a
/// kingdom's): its room, its faction and its name.
fn channel_lists(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, String)> {
    let world = h.server.cfg.world_name.clone();
    let mut out = Vec::new();
    for m in h.world_messages(b, from) {
        let ServerMessage::PlayerCall(c) = &m else {
            continue;
        };
        if c.function.0 != 0x8205_001E {
            continue;
        }
        for k in kingdom::ALL {
            let list = yalain_world::chat::channels_of(Some(k));
            let ours = yalain_world::chat::channels(&world, &list);
            if ours.encode() == m.encode() {
                let f = &list[3];
                out.push((f.room, f.faction, f.name.to_string()));
            }
        }
    }
    out
}

/// A player on foot who uses a faction shrine of another kingdom is told its rating is recorded,
/// and 3.83 s later joins the shrine's kingdom: its kingdom and its physics type change (every
/// client that knows it is told), it is told "You join {the} %1." naming the kingdom, and a moment
/// later it is sent its channels with its kingdom's faction channel. Its ethereality in a peace
/// area is the same in any kingdom, so it is not sent.
/// Behaviour: server.kingdoms.shrine-join
#[test]
fn a_player_on_foot_joins_a_shrines_kingdom_a_little_after_its_rating() {
    use yalain_world::usable::shrines::RECORDED;
    let mut h = world_with_a_shrine(Conflict::Peace);
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    assert_eq!(
        kingdom_of(&h, ida),
        None,
        "a new character is in no kingdom"
    );
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(SHRINE, ida)]),
    );
    // The rating: the moment the join is counted from.
    assert!(h.run_until(100, |h| texts(h, a, fa).iter().any(|t| t.0 == RECORDED)));
    let rated = h.now.0;
    h.run_to(yalain_net::LocalTime(rated + 3.83 - 0.2));
    h.run(3);
    assert_eq!(kingdom_of(&h, ida), None, "not yet");
    assert!(visuals(&h, a, fa, ida).is_empty(), "nothing yet");
    h.run_to(yalain_net::LocalTime(rated + 3.83 + 0.1));
    h.run(3);
    assert_eq!(kingdom_of(&h, ida), Some(kingdom::DOMINION));
    let dominion_physics = 0x2080_0020;
    assert_eq!(
        visuals(&h, a, fa, ida),
        [
            (quality::KINGDOM, 4),
            (quality::PHYSICS_TYPE, dominion_physics)
        ],
        "its kingdom, its physics type; no ethereality in peace"
    );
    assert_eq!(
        visuals(&h, b, fb, ida),
        [
            (quality::KINGDOM, 4),
            (quality::PHYSICS_TYPE, dominion_physics)
        ],
        "the other player sees it too"
    );
    let told = texts(&h, a, fa);
    assert_eq!(
        told.last(),
        Some(&(kingdoms::JOINED, Some(0x1270_95B3))),
        "joined the Dominion: {told:?}"
    );
    h.run(5);
    assert_eq!(
        channel_lists(&h, a, fa),
        [(5, 4, "Faction Faction3 ".to_string())],
        "its channels again, with the Dominion's"
    );
}

/// A player of the shrine's kingdom already is told its rating and nothing more; so is a player
/// in no kingdom at the neutral shrine. A player on a mount is told its use is canceled, and joins
/// nothing either.
/// Behaviour: server.kingdoms.shrine-of-its-own
#[test]
fn a_player_of_the_shrines_kingdom_already_joins_nothing() {
    for (shrine, mine, mounted) in [
        (kingdom::DOMINION, Some(kingdom::DOMINION), false),
        (kingdom::NEUTRAL, None, false),
        (kingdom::DOMINION, None, true),
    ] {
        let mut h = world_with_a_shrine(Conflict::Peace);
        h.server.shrine_kingdoms.insert(SHRINE_PACKAGE, shrine);
        let (a, ida) = h.player_in_world("one", "Elara");
        if let Some(k) = mine {
            set_kingdom(&mut h, ida, k);
        }
        if mounted {
            for p in h.server.players.values_mut() {
                p.riding = Some(yalain_world::mounts::Riding {
                    eid: 1,
                    state: 0,
                    look: auberean_protocol::types::VisualDesc::default(),
                    speed: 1.0,
                    checking: false,
                });
            }
        }
        h.run(10);
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::USE, &[use_of(SHRINE, ida)]),
        );
        h.run_to(yalain_net::LocalTime(h.now.0 + 10.0));
        h.run(5);
        assert_eq!(kingdom_of(&h, ida), mine, "{shrine:#x}");
        assert!(visuals(&h, a, from, ida).is_empty(), "{shrine:#x}");
        assert!(channel_lists(&h, a, from).is_empty(), "{shrine:#x}");
        let told = if mounted { 2 } else { 1 };
        assert_eq!(
            texts(&h, a, from).len(),
            told,
            "the rating (and the cancel) alone"
        );
    }
}

/// A player's kingdom is kept: as it enters again its description and its object carry it (its
/// faction and its physics type's kingdom bit), and so does the object another player is made.
/// Behaviour: server.kingdoms.kept
#[test]
fn a_joined_kingdom_is_kept_and_carried_by_the_players_description_and_object() {
    let mut h = world_with_a_shrine(Conflict::Peace);
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(SHRINE, ida)]),
    );
    h.run_to(yalain_net::LocalTime(h.now.0 + 6.0));
    h.run(5);
    assert!(!visuals(&h, a, from, ida).is_empty(), "joined");
    log_off(&mut h, a, ida);
    let from = h.bots[a].inbox.len();
    h.enter_world(a, "one", ida);
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let faction =
        |w: &auberean_protocol::types::WeenieDesc| (w.mask & 0x2000 != 0).then_some(w.numbers[2]);
    let mine = h.world_messages(a, from);
    let desc = mine
        .iter()
        .find_map(|m| match m {
            ServerMessage::PlayerQualities(q) => Some(q.0.weenie.clone()),
            _ => None,
        })
        .expect("a description");
    assert_eq!(faction(&desc), Some(4));
    assert_eq!(desc.numbers[5], 0x2080_0020, "the Dominion's physics type");
    let object = |msgs: &[ServerMessage]| {
        msgs.iter().find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == ida => Some(o.weenie.clone()),
            _ => None,
        })
    };
    let own = object(&mine).expect("its object");
    assert_eq!((faction(&own), own.numbers[5]), (Some(4), 0x2080_0020));
    let seen = object(&h.world_messages(b, 0)).expect("seen by the other");
    assert_eq!((faction(&seen), seen.numbers[5]), (Some(4), 0x2080_0020));
}

/// A player's profile names its kingdom; a player in none is shown in the neutral kingdom.
/// Behaviour: server.kingdoms.profile
#[test]
fn a_players_profile_names_its_kingdom() {
    use yalain_world::profile::{kingdom as name_of, line};
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (_, idb) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let examine = |h: &mut Harness| {
        let from = h.bots[a].inbox.len();
        let mut b = yalain_world::graph::Builder::new();
        b.native(
            154,
            auberean_script::native_object::NativeObject::Packed {
                ty: 154,
                bytes: [0x4000_0002u32, 0]
                    .iter()
                    .flat_map(|w| w.to_le_bytes())
                    .chain(idb.to_le_bytes())
                    .chain(0x203u32.to_le_bytes())
                    .collect(),
            },
        );
        h.ask(
            a,
            function::EXAMINE,
            &[auberean_script::args::Arg::Object(b.graph())],
            10,
        );
        h.world_messages(a, from)
            .iter()
            .rev()
            .find_map(|m| match m {
                ServerMessage::PlayerCall(c)
                    if c.function.0 == yalain_world::profile::EXAMINATION_PROFILE =>
                {
                    Some(ServerMessage::PlayerCall(c.clone()).encode())
                }
                _ => None,
            })
            .expect("a profile")
    };
    let has = |bytes: &[u8], id: u32| bytes.windows(4).any(|w| w == id.to_le_bytes());
    let neutral = examine(&mut h);
    assert!(has(&neutral, line::NEUTRAL), "none: the neutral kingdom");
    set_kingdom(&mut h, idb, kingdom::ORDER);
    let order = examine(&mut h);
    assert!(has(&order, name_of(kingdom::ORDER)), "the Order");
    assert!(!has(&order, line::NEUTRAL), "and not the neutral one");
}

/// A player's faction channel is its kingdom's: the Shadow's room 3, the Order's 4, the
/// Dominion's 5, the neutral kingdom's (and a player in none) 6; a line said into a kingdom's
/// room reaches the players of that kingdom alone.
/// Behaviour: server.kingdoms.room-reaches-its-kingdom
#[test]
fn a_kingdoms_room_line_reaches_its_kingdoms_players_alone() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Babyfare");
    let (c, _) = h.player_in_world("three", "Hirel");
    set_kingdom(&mut h, ida, kingdom::SHADOW);
    set_kingdom(&mut h, idb, kingdom::SHADOW);
    h.run(10);
    let k = [
        (Some(kingdom::SHADOW), 3),
        (Some(kingdom::ORDER), 4),
        (Some(kingdom::DOMINION), 5),
        (Some(kingdom::NEUTRAL), 6),
        (None, 6),
    ];
    for (kingdom, room) in k {
        assert_eq!(yalain_world::chat::channels_of(kingdom)[3].room, room);
        assert_eq!(kingdoms::room(kingdom), room);
    }
    let (fa, fb, fc) = (
        h.bots[a].inbox.len(),
        h.bots[b].inbox.len(),
        h.bots[c].inbox.len(),
    );
    let packet = SendToRoomById {
        context: 7,
        room: 3,
        text: "For the Shadow".into(),
        extra: ExtraBlob::Speaker {
            sender: 1,
            chat_type: 100 << 16 | 1,
        },
    }
    .network_packet()
    .expect("packs");
    h.bots[a].send(false, 4, &ClientMessage::Chat(ChatPacket(packet)));
    h.run(30);
    let heard = |h: &Harness, b: usize, from: usize| -> usize {
        h.bots[b].inbox[from..]
            .iter()
            .filter(|r| !r.world)
            .filter_map(|r| match r.message() {
                Some(ServerMessage::Chat(ChatPacket(p))) => decode_incoming(
                    &[
                        &u32::try_from(p.len()).expect("small").to_le_bytes()[..],
                        &p,
                    ]
                    .concat(),
                )
                .ok(),
                _ => None,
            })
            .filter(|i| matches!(&i.payload, IncomingPayload::RoomEvent(e) if e.room == 3))
            .count()
    };
    assert_eq!(heard(&h, a, fa), 1, "the speaker's echo");
    assert_eq!(heard(&h, b, fb), 1, "a player of the Shadow");
    assert_eq!(heard(&h, c, fc), 0, "a player in no kingdom");
}

/// The session start carries the conflict status of where the character enters, and nothing more
/// is said of it to a character that has been in the world; a new character is told it once more
/// right after its description, and its object's ethereality, its template's, is set right after
/// the arrival effect.
/// Behaviour: server.kingdoms.status-at-entry
#[test]
fn the_session_start_carries_the_status_of_where_the_character_enters() {
    let mut h = Harness::new();
    h.server
        .content
        .conflict
        .insert((START >> 16) as u16, Conflict::Controlled);
    let b = h.at_character_list("one");
    let id = h.create_character(b, "one", "Elara");
    let session_status = |h: &Harness, from: usize| -> Vec<(u32, u32)> {
        calls_of(h, b, from, SESSION_START)
            .iter()
            .filter_map(|v| match v.as_slice() {
                [.., Value::Int(local), Value::Int(server)] => Some((*local, *server)),
                _ => None,
            })
            .collect()
    };
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", id);
    assert_eq!(
        session_status(&h, from),
        [(2, 0)],
        "controlled, and no server status"
    );
    assert_eq!(statuses(&h, b, from), [2], "a new character is told it");
    // A player in no kingdom in a controlled area passes through every kingdom, as in peace.
    assert_eq!(
        visuals(&h, b, from, id),
        [(quality::ETHEREALITY, 0x3E20_0010)]
    );
    log_off(&mut h, b, id);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", id);
    assert_eq!(session_status(&h, from), [(2, 0)]);
    assert!(statuses(&h, b, from).is_empty(), "told nothing more");
    assert!(visuals(&h, b, from, id).is_empty());
}

/// Walking into a landblock of another conflict status tells the player the new status; its
/// ethereality follows, and every client that knows it is told. Walking on into a landblock of
/// the same status says nothing.
/// Behaviour: server.kingdoms.status-on-the-way
#[test]
fn walking_into_a_landblock_of_another_status_tells_it_and_the_ethereality_follows() {
    let mut h = Harness::new();
    h.server
        .content
        .conflict
        .insert(NEXT_LANDBLOCK, Conflict::Controlled);
    h.server
        .content
        .conflict
        .insert(NEXT_LANDBLOCK + 1, Conflict::Controlled);
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    set_kingdom(&mut h, ida, kingdom::SHADOW);
    // Their clients have drawn the world they entered.
    for x in [a, b] {
        h.bots[x].send(
            true,
            queue::WEENIE,
            &ClientMessage::SceneRenderingComplete(Empty),
        );
    }
    h.run(10);
    let walk = |h: &mut Harness, x: usize, landblock: u16| {
        let cell = u32::from(landblock) << 16 | 0x0001;
        h.bots[x].send(
            true,
            queue::WEENIE,
            &report(cell, Vec3::new(10.0, 10.0, 0.0), 0x40, IDLE_MOTION, 0),
        );
        h.run(10);
    };
    // The other player goes ahead, to see the first come.
    walk(&mut h, b, NEXT_LANDBLOCK);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    walk(&mut h, a, NEXT_LANDBLOCK);
    assert_eq!(statuses(&h, a, fa), [2], "a controlled area");
    // The Shadow there passes through the Shadow and the neutral kingdom.
    assert_eq!(
        visuals(&h, a, fa, ida),
        [(quality::ETHEREALITY, 0x0E20_0010)]
    );
    assert_eq!(
        visuals(&h, b, fb, ida),
        [(quality::ETHEREALITY, 0x0E20_0010)]
    );
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    walk(&mut h, a, NEXT_LANDBLOCK + 1);
    assert!(statuses(&h, a, fa).is_empty(), "the same status: nothing");
    assert!(visuals(&h, a, fa, ida).is_empty());
    assert!(visuals(&h, b, fb, ida).is_empty());
    walk(&mut h, a, 0x029C);
    assert_eq!(statuses(&h, a, fa), [1], "back in peace");
    assert_eq!(
        visuals(&h, a, fa, ida),
        [(quality::ETHEREALITY, 0x3E20_0010)]
    );
}

/// A player taken by a portal into a landblock of another conflict status is told the new status
/// once its arrival is through (its client has drawn the world), not before.
/// Behaviour: server.kingdoms.status-on-arrival
#[test]
fn arriving_through_a_portal_tells_the_status_once_the_world_is_drawn() {
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server.content = ContentPack {
        objects: vec![object(PORTAL, 0x4700_1154, 0x1A00, Some(to))],
        ..ContentPack::default()
    };
    h.server
        .content
        .conflict
        .insert(0x7000, Conflict::KingdomVsKingdom);
    let (a, ida) = h.player_in_world("one", "Elara");
    set_kingdom(&mut h, ida, kingdom::ORDER);
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(PORTAL, ida)]),
    );
    h.run(60);
    assert!(statuses(&h, a, from).is_empty(), "not while arriving");
    h.bots[a].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(Empty),
    );
    h.run(10);
    assert_eq!(statuses(&h, a, from), [3], "kingdom against kingdom");
    // The Order there passes through its own kingdom alone.
    assert_eq!(
        visuals(&h, a, from, ida),
        [(quality::ETHEREALITY, 0x1220_0010)]
    );
}

/// A player's movement ethereality names the kingdoms of the players it may not fight where it
/// stands, by the client's own rule (a player in none counts as neutral): in peace every kingdom;
/// in a controlled area its own and the neutral one (every kingdom for a neutral player); kingdom
/// against kingdom its own; free for all none.
/// Behaviour: server.kingdoms.ethereality
#[test]
fn a_players_ethereality_passes_through_the_kingdoms_it_may_not_fight() {
    use kingdoms::ethereality;
    for k in [None, Some(1), Some(2), Some(4), Some(kingdom::NEUTRAL)] {
        assert_eq!(ethereality(1, k), 0x3E20_0010, "peace {k:?}");
        assert_eq!(ethereality(4, k), 0x0220_0010, "free for all {k:?}");
    }
    assert_eq!(ethereality(2, Some(1)), 0x0E20_0010);
    assert_eq!(ethereality(2, Some(2)), 0x1620_0010);
    assert_eq!(ethereality(2, Some(4)), 0x2620_0010);
    assert_eq!(ethereality(2, None), 0x3E20_0010);
    assert_eq!(ethereality(3, Some(1)), 0x0A20_0010);
    assert_eq!(ethereality(3, Some(2)), 0x1220_0010);
    assert_eq!(ethereality(3, Some(4)), 0x2220_0010);
    assert_eq!(ethereality(3, None), 0x0620_0010);
    assert_eq!(ethereality(5, Some(4)), 0x2220_0010);
    assert!(!kingdoms::may_fight(2, 1, kingdom::NEUTRAL));
    assert!(kingdoms::may_fight(2, 1, 2));
    assert!(!kingdoms::may_fight(3, 4, 4));
    assert!(kingdoms::may_fight(4, 4, 4));
    assert_eq!(kingdoms::physics_type(None), 0x0480_0020);
    assert_eq!(kingdoms::physics_type(Some(kingdom::NEUTRAL)), 0x0480_0020);
    assert_eq!(kingdoms::physics_type(Some(1)), 0x0880_0020);
    assert_eq!(kingdoms::physics_type(Some(2)), 0x1080_0020);
}
