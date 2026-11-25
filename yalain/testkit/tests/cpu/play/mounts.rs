//! Mounts: a saddle or a mount's egg used from the pack puts its user on its beast (the mounted
//! flag, the record, the appearance, the speed, the look, the answer), unless something is in its
//! hands, it is indoors or moving, or it is already mounted; the rider dismounts when it asks, when
//! the record runs out, when it stays indoors a moment, or at a resurrection; a rider's pet attack
//! orders are refused; others see the rider mounted. Fixture: two players of the test world, an
//! Ataur saddle and a mount's egg as made-up templates of their effects, a made-up mount table.

use auberean_protocol::table::Table;
use auberean_protocol::types::{PartGroupDesc, VisualDesc};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_store::Store;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::effect_defs::{EffectDef, Kind, Scale, REMOVED_ON_DEATH};
use yalain_world::item_use::{refusal, UsageEffect};
use yalain_world::mounts::{self, MountEffect};
use yalain_world::places::usage;

const SADDLE: u32 = 0x4700_089E;
const EGG: u32 = 0x4700_14CC;
const LEY: u32 = 0x6F00_19F2;
const HERO: u32 = 0x6F00_1C4C;
const LEY_TYPE: u32 = 0x4000_0001;
const HERO_TYPE: u32 = 0x4000_0008;
const FOCUS_COST: u32 = 0x6F00_1C4D;
const FOCUS: u32 = 266;
const MOUNT_FX: u32 = 0x4100_0536;
const ENDING_FX: u32 = 0x4100_0537;
const MOUNT_LOOK: u32 = 0x1F00_0526;
const MOTION: u32 = 0x3200_000E;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const SHOW_TEXT: u32 = 0x8205_0011;
const PET_ATTACK: u32 = yalain_world::pets::function::ATTACK;
const SWORD: u32 = 0x4700_051C;

/// A use of an item, as the client asks for it.
fn use_of(item: u64, me: u64) -> Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(me),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

/// A mount effect: its speed multiplier, its duration (none: until taken off), its appearance.
fn mount_def(state: u32, speed: f32, duration: Option<f32>, fx: u32) -> EffectDef {
    EffectDef {
        state,
        kind: Kind::Numeric {
            flags: 0x9,
            stats: vec![0x1_07D2, 0x3_0127],
            magnitude: Scale(vec![(0.0, speed)]),
        },
        duration: duration.map_or_else(Scale::default, |d| Scale(vec![(0.0, d)])),
        priority: Scale(vec![(0.0, 100.0)]),
        fx,
        external_flags: 0x9,
        internal_flags: REMOVED_ON_DEATH,
        ends_at_logout: false,
        ends_at_teleport: false,
        enum_value: 2777,
        eq_class: 19,
        name: None,
    }
}

/// The mount table's look for one type: its own id, motion, part groups and appearance table.
fn table_look() -> VisualDesc {
    VisualDesc {
        bits: VisualDesc::DID
            | VisualDesc::MOTION_INTERP
            | VisualDesc::BEHAVIOR_TABLE
            | VisualDesc::MODES
            | VisualDesc::GLOBAL
            | VisualDesc::PART_GROUPS,
        did: MOUNT_LOOK,
        motion_interp: MOTION,
        behavior_table: 0x4400_0001,
        modes: 0x5400_0006,
        global: PartGroupDesc {
            bits: PartGroupDesc::KEY | PartGroupDesc::FX_TABLE,
            key: 0x4000_0002,
            fx_table: 0x1E00_0039,
            ..PartGroupDesc::default()
        },
        part_groups: Table::from_inserted(
            1,
            [(
                0x8100_0007,
                PartGroupDesc {
                    bits: PartGroupDesc::KEY | PartGroupDesc::SETUP,
                    key: 0x8100_0007,
                    setup: 0x0400_0499,
                    ..PartGroupDesc::default()
                },
            )],
        ),
        ..VisualDesc::default()
    }
}

/// The test world with the saddle and the egg, their effects and the mount table.
fn world() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut saddle = Props::default();
    saddle
        .set("Usage_Effect1", Prop::Text("Eff_BarnesLostWish1".into()))
        .set("Item_UsageAction", Prop::Word(0x30))
        .set("Item_IsDestroyOnUse", Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(SADDLE, 0x282A, saddle));
    let mut egg = Props::default();
    egg.set("Usage_Effect1", Prop::Text("Eff_HeroMountColor3".into()))
        .set(
            "Usage_Effect2",
            Prop::Text("Eff_HeroMount_FocusCost".into()),
        )
        .set("Item_UsageAction", Prop::Word(0x30))
        .set("Item_CanUseWhileMoving", Prop::Bool(false));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(EGG, 0x282A, egg));
    h.server
        .effect_table
        .effects
        .insert(LEY, mount_def(LEY, 1.4, Some(900.0), MOUNT_FX));
    h.server
        .effect_table
        .effects
        .insert(HERO, mount_def(HERO, 1.9, None, 0));
    h.server.effect_table.effects.insert(
        FOCUS_COST,
        EffectDef {
            state: FOCUS_COST,
            kind: Kind::InstantVital {
                flags: 0xC,
                change: Scale(vec![(0.0, -5.0)]),
            },
            duration: Scale::default(),
            ..mount_def(FOCUS_COST, 1.0, None, 0)
        },
    );
    let effect = |state| UsageEffect {
        state,
        spellcraft: 1.0,
    };
    h.server.item_uses.insert(SADDLE, vec![effect(LEY)]);
    h.server
        .item_uses
        .insert(EGG, vec![effect(HERO), effect(FOCUS_COST)]);
    let m = &mut h.server.mounts;
    m.effects.insert(
        LEY,
        MountEffect {
            mount_type: LEY_TYPE,
            ending_fx: ENDING_FX,
        },
    );
    m.effects.insert(
        HERO,
        MountEffect {
            mount_type: HERO_TYPE,
            ending_fx: 0,
        },
    );
    m.items.insert(SADDLE, LEY);
    m.items.insert(EGG, HERO);
    for (race, sex, _) in yalain_testkit::BODY_ENTITIES {
        for t in [LEY_TYPE, HERO_TYPE] {
            m.looks.insert((t, race, sex), table_look());
        }
    }
    h
}

/// A player of the test world in it, its scene drawn; the client, the player, its connection.
fn in_world(h: &mut Harness, account: &str, name: &str) -> (usize, u64, yalain_net::ConnId) {
    let (b, me) = h.player_in_world(account, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    // Outdoors (the test world starts its players in a hall).
    let at = h.server.players[&world].position;
    let stamp = h.server.players[&world].teleport_stamp;
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(
            at.cell & 0xFFFF_0000 | 0x0001,
            auberean_primitives::Vec3::new(at.origin[0], at.origin[1], at.origin[2]),
            0,
            yalain_world::world::IDLE_MOTION,
            stamp,
        ),
    );
    h.run(5);
    (b, me, world)
}

/// Give a player an item of a template; its id.
fn give(h: &mut Harness, world: yalain_net::ConnId, template: u32) -> u64 {
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, template, 1);
    h.run(3);
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == template)
        .expect("given")
        .id
}

/// Send a use; the inbox mark before.
fn send_use(h: &mut Harness, b: usize, me: u64, item: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(item, me)]));
    h.run(4);
    from
}

/// Ask to dismount; the inbox mark before.
fn send_dismount(h: &mut Harness, b: usize) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(mounts::DISMOUNT, &[]));
    h.run(4);
    from
}

/// The use answers a client was sent since `from`: (status, control flags).
fn use_answers(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => {
                        Some((slots[usage::STATUS].1, slots[usage::FLAGS].1))
                    }
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// The texts a client was shown since `from`, by string id.
fn shown(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                v.iter().find_map(|x| match x {
                    Value::Graph(g) => g.iter().find_map(|(_, n)| match n {
                        Node::TextInfo(t) => Some(t.string_id),
                        _ => None,
                    }),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// The mounted flag's values a client was told since `from`.
fn flags(h: &Harness, b: usize, from: usize) -> Vec<i32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateBool(q) if q.quality == mounts::MOUNTED => Some(q.value),
            _ => None,
        })
        .collect()
}

/// What a client was shown of a player since `from`, in order (its visual events): appearances
/// begun and stopped, speeds, looks (whether mounted).
fn told(h: &Harness, b: usize, from: usize, about: u64) -> Vec<String> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoFx(e) if e.sender.id == about => Some(format!("fx {:X}", e.action.fx)),
            ServerMessage::StopFx(e) if e.sender.id == about => {
                Some(format!("stop {:X}", e.action.0))
            }
            ServerMessage::VelocityScale(e) if e.sender.id == about => {
                Some(format!("speed {:.2}", e.action.0))
            }
            ServerMessage::UpdateVisualDesc(e) if e.sender.id == about => Some(
                if e.action.motion_interp == MOTION {
                    "look mounted"
                } else {
                    "look own"
                }
                .to_string(),
            ),
            _ => None,
        })
        .collect()
}

/// The player's mount record: its effect, flags, timeout and the item it came from.
fn record(h: &Harness, world: yalain_net::ConnId) -> Option<(u32, u32, f32, u64)> {
    let p = &h.server.players[&world];
    let eid = p.riding.as_ref()?.eid;
    p.play
        .effects
        .iter()
        .find(|e| e.eid == eid)
        .map(|e| (e.state, e.flags, e.timeout, e.item))
}

/// An Ataur saddle used from the pack: the mount's appearance, the player told it is mounted, the
/// mounted look, its record (fifteen minutes, from the saddle), the speed times the mount's, the
/// saddle spent, the use answered (flags 828 and the spent bit); another player sees the
/// appearance, the look and the speed in that order.
/// Behaviour: server.mounts.saddle-mounts-its-user
#[test]
fn a_saddle_used_from_the_pack_mounts_its_user_and_is_spent() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let (c, _, _) = in_world(&mut h, "two", "Torvald");
    h.run(5);
    let saddle = give(&mut h, world, SADDLE);
    let seen = h.bots[c].inbox.len();
    let from = send_use(&mut h, b, me, saddle);
    assert_eq!(flags(&h, b, from), [1]);
    assert_eq!(
        told(&h, b, from, me),
        ["fx 41000536", "look mounted", "speed 1.40"]
    );
    assert_eq!(
        use_answers(&h, b, from),
        [(0, mounts::ANSWER_FLAGS | mounts::SPENT)]
    );
    assert_eq!(
        record(&h, world),
        Some((LEY, mounts::record::TIMED, 900.0, saddle))
    );
    assert!(
        !h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.template == SADDLE),
        "the saddle is spent"
    );
    assert_eq!(
        told(&h, c, seen, me),
        ["fx 41000536", "look mounted", "speed 1.40"]
    );
    let look = h.world_messages(c, seen).into_iter().find_map(|m| match m {
        ServerMessage::UpdateVisualDesc(e) if e.sender.id == me => Some(e.action),
        _ => None,
    });
    let look = look.expect("the mounted look");
    assert_eq!(look.bits & VisualDesc::DID, 0, "without the table's own id");
    assert_ne!(
        look.global.bits & PartGroupDesc::APPEARANCE,
        0,
        "the rider's appearance"
    );
}

/// A mount's egg mounts its user until it dismounts: no appearance, a record held until taken
/// off, five taken from the focus as the mount goes on, the egg kept, the answer's flags 828.
/// Behaviour: server.mounts.egg-mounts-until-dismounted
#[test]
fn an_egg_mounts_its_user_until_dismounted_and_is_kept() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let egg = give(&mut h, world, EGG);
    let from = send_use(&mut h, b, me, egg);
    assert_eq!(flags(&h, b, from), [1]);
    assert_eq!(told(&h, b, from, me), ["look mounted", "speed 1.90"]);
    let focus: Vec<i32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q) if q.quality == FOCUS => Some(q.value),
            _ => None,
        })
        .collect();
    assert_eq!(focus, [95], "the egg's focus cost");
    assert_eq!(use_answers(&h, b, from), [(0, mounts::ANSWER_FLAGS)]);
    assert_eq!(
        record(&h, world),
        Some((HERO, mounts::record::UNTIL_OFF, f32::MAX, egg))
    );
    assert!(h.server.players[&world]
        .play
        .items
        .iter()
        .any(|i| i.id == egg));
}

/// What a ride's start or end sent its rider, in the order sent: the mount's appearance ("fx") and
/// its ending one ("endfx"), the mounted flag ("bool"), the rider's look ("look"), the record added
/// ("add") or taken off ("remove"), the appearance stopped ("stop"), the focus told ("cost"), the
/// item gone ("gone"), the use answered ("done"). The speed is left out.
fn sent_order(h: &Harness, b: usize, from: usize, me: u64, item: u64) -> Vec<&'static str> {
    use yalain_world::effects::function as effect;
    let mut sent: Vec<(u64, ServerMessage)> = h.bots[b].inbox[from..]
        .iter()
        .filter(|r| r.world)
        .filter_map(|r| Some((r.blob_id & 0xFFFF_FFFF, r.message()?)))
        .collect();
    sent.sort_by_key(|(n, _)| *n);
    sent.iter()
        .filter_map(|(_, m)| match m {
            ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == MOUNT_FX => Some("fx"),
            ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == ENDING_FX => {
                Some("endfx")
            }
            ServerMessage::StopFx(e) if e.sender.id == me && e.action.0 == MOUNT_FX => Some("stop"),
            ServerMessage::UpdateBool(q) if q.quality == mounts::MOUNTED => Some("bool"),
            ServerMessage::UpdateVisualDesc(e) if e.sender.id == me => Some("look"),
            ServerMessage::UpdateInt(q) if q.quality == FOCUS => Some("cost"),
            ServerMessage::ForgetObject(o) if o.0.id == item => Some("gone"),
            ServerMessage::PlayerCall(c) => match c.function.0 {
                effect::ADD => Some("add"),
                effect::REMOVE => Some("remove"),
                USE_DONE => Some("done"),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// A mount, in the order sent: a saddle's appearance, the mounted flag, the mounted look, the
/// record, the saddle gone, the answer; an egg's the flag, the look, the record, its focus cost,
/// the answer (2017: the appearance before the flag 178 of 178 saddle mounts; the look before the
/// record 286 of 287 and before the answer 287 of 287).
/// Behaviour: server.mounts.in-send-order
#[test]
fn a_mount_plays_its_appearance_then_flags_the_rider_then_shows_its_look_before_its_record() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let saddle = give(&mut h, world, SADDLE);
    let from = send_use(&mut h, b, me, saddle);
    assert_eq!(
        sent_order(&h, b, from, me, saddle),
        ["fx", "bool", "look", "add", "gone", "done"]
    );
    send_dismount(&mut h, b);
    let egg = give(&mut h, world, EGG);
    let from = send_use(&mut h, b, me, egg);
    assert_eq!(
        sent_order(&h, b, from, me, egg),
        ["bool", "look", "add", "cost", "done"]
    );
}

/// A ride's end, in the order sent: the rider's own look, the flag cleared, the record taken off,
/// the mount's appearance stopped, its ending one played (2017: the look before the flag 288 of
/// 288 ends).
/// Behaviour: server.mounts.in-send-order
#[test]
fn a_rides_end_shows_the_riders_own_look_before_it_clears_the_flag() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let saddle = give(&mut h, world, SADDLE);
    send_use(&mut h, b, me, saddle);
    let from = send_dismount(&mut h, b);
    assert_eq!(
        sent_order(&h, b, from, me, saddle),
        ["look", "bool", "remove", "stop", "endfx"]
    );
    let egg = give(&mut h, world, EGG);
    send_use(&mut h, b, me, egg);
    let from = send_dismount(&mut h, b);
    assert_eq!(
        sent_order(&h, b, from, me, egg),
        ["look", "bool", "remove"],
        "an egg's mount has no appearance"
    );
}

/// A mount item is refused, answered alone with flags 828: with something in hand (10045),
/// indoors (10046), moving (10055), already mounted (10047).
/// Behaviour: server.mounts.refused
#[test]
fn a_mount_item_is_refused_in_hand_indoors_moving_or_mounted() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let egg = give(&mut h, world, EGG);
    let sword = h.arm(b, me, SWORD);
    let from = send_use(&mut h, b, me, egg);
    assert_eq!(
        use_answers(&h, b, from),
        [(refusal::HANDS_NOT_EMPTY, mounts::ANSWER_FLAGS)]
    );
    assert!(
        told(&h, b, from, me).is_empty() && flags(&h, b, from).is_empty(),
        "nothing else"
    );
    if let Some(p) = h.server.players.get_mut(&world) {
        p.play.items.retain(|i| i.id != sword);
    }
    let outside = h.server.players[&world].position.cell;
    h.server.players.get_mut(&world).expect("in").position.cell = outside & 0xFFFF_0000 | 0x0105;
    let from = send_use(&mut h, b, me, egg);
    assert_eq!(
        use_answers(&h, b, from),
        [(refusal::INDOORS, mounts::ANSWER_FLAGS)]
    );
    h.server.players.get_mut(&world).expect("in").position.cell = outside;
    let idle = h.server.players[&world].heading_motion;
    h.server.players.get_mut(&world).expect("in").heading_motion = idle & 0xFF00_0000 | 0x0001;
    let from = send_use(&mut h, b, me, egg);
    assert_eq!(
        use_answers(&h, b, from),
        [(refusal::MOVING, mounts::ANSWER_FLAGS)]
    );
    h.server.players.get_mut(&world).expect("in").heading_motion = idle;
    let from = send_use(&mut h, b, me, egg);
    assert_eq!(use_answers(&h, b, from), [(0, mounts::ANSWER_FLAGS)]);
    let from = send_use(&mut h, b, me, egg);
    assert_eq!(
        use_answers(&h, b, from),
        [(refusal::MOUNTED, mounts::ANSWER_FLAGS)]
    );
}

/// A rider asking to dismount: a follow under way stops, its own look is shown, the player is told
/// it is not mounted, its record taken off with the speed back, the mount's appearance stopped and
/// its ending one played; a player not mounted asking is not answered.
/// Behaviour: server.mounts.dismount
#[test]
fn a_rider_dismounting_is_shown_its_own_look_and_speed_again() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let (c, other, _) = in_world(&mut h, "two", "Torvald");
    h.run(5);
    let saddle = give(&mut h, world, SADDLE);
    send_use(&mut h, b, me, saddle);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::follow::FOLLOW, &[Arg::LongInt(other)]),
    );
    h.run(4);
    let seen = h.bots[c].inbox.len();
    let from = send_dismount(&mut h, b);
    assert_eq!(shown(&h, b, from), [yalain_world::follow::STOPPED.0]);
    assert_eq!(
        told(&h, b, from, me),
        ["look own", "speed 1.00", "stop 41000536", "fx 41000537"]
    );
    assert_eq!(flags(&h, b, from), [0]);
    assert_eq!(
        told(&h, c, seen, me),
        ["look own", "speed 1.00", "stop 41000536", "fx 41000537"]
    );
    assert!(h.server.players[&world].riding.is_none());
    assert!(h.server.players[&world]
        .play
        .effects
        .iter()
        .all(|e| e.state != LEY));
    // Not mounted: not answered, and a follow goes on.
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::follow::FOLLOW, &[Arg::LongInt(other)]),
    );
    h.run(4);
    let again = send_dismount(&mut h, b);
    assert!(h.world_messages(b, again).is_empty(), "not answered");
    assert!(h.server.players[&world].following.is_some());
}

/// A rider dismounting while it follows: in the order sent, the follow's order stopped, its own
/// look, the flag, the record off, the appearance stopped and the ending one played, then the
/// follow's end told last.
/// Behaviour: server.mounts.dismount-tells-the-follows-end-last
#[test]
fn a_rider_dismounting_while_following_is_told_the_follow_ended_last() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let (_, other, _) = in_world(&mut h, "two", "Torvald");
    h.run(5);
    let saddle = give(&mut h, world, SADDLE);
    send_use(&mut h, b, me, saddle);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::follow::FOLLOW, &[Arg::LongInt(other)]),
    );
    h.run(4);
    let from = send_dismount(&mut h, b);
    let mut order = Vec::new();
    for m in h.sent_messages(b, from) {
        let part = match &m {
            ServerMessage::MoveTo(e) if e.sender.id == me => "stop order",
            ServerMessage::UpdateVisualDesc(e) if e.sender.id == me => "look",
            ServerMessage::UpdateBool(q) if q.quality == mounts::MOUNTED => "flag",
            ServerMessage::PlayerCall(c)
                if c.function.0 == yalain_world::effects::function::REMOVE =>
            {
                "record off"
            }
            ServerMessage::StopFx(e) if e.sender.id == me => "appearance stopped",
            ServerMessage::DoFx(e) if e.sender.id == me => "ending appearance",
            ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => "told",
            _ => continue,
        };
        order.push(part);
    }
    assert_eq!(
        order,
        [
            "stop order",
            "look",
            "flag",
            "record off",
            "appearance stopped",
            "ending appearance",
            "told"
        ]
    );
    assert_eq!(shown(&h, b, from), [yalain_world::follow::STOPPED.0]);
}

/// A saddle's ride ends when its record runs out, as a dismount ends it.
/// Behaviour: server.mounts.ride-runs-out
#[test]
fn a_saddles_ride_ends_when_its_time_is_up() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let saddle = give(&mut h, world, SADDLE);
    send_use(&mut h, b, me, saddle);
    let from = h.bots[b].inbox.len();
    let t = h.now.0 + 890.0;
    while h.now.0 < t {
        h.step_by(1.0);
    }
    assert!(h.server.players[&world].riding.is_some(), "still riding");
    assert!(told(&h, b, from, me).is_empty());
    let t = h.now.0 + 15.0;
    while h.now.0 < t {
        h.step_by(1.0);
    }
    assert!(h.server.players[&world].riding.is_none());
    assert_eq!(
        told(&h, b, from, me),
        ["look own", "speed 1.00", "stop 41000536", "fx 41000537"]
    );
    assert_eq!(flags(&h, b, from), [0]);
}

/// When a rider's next look at where it is is due on its ride, by the harness clock (a look of an
/// earlier ride comes to nothing).
fn next_look(h: &Harness, world: yalain_net::ConnId) -> Option<f64> {
    let ride = h.server.players[&world].riding.as_ref()?.eid;
    h.server
        .timers
        .iter()
        .filter_map(|(at, t)| match t {
            yalain_world::Timer::RiderCheck { world: w, eid } if *w == world && *eid == ride => {
                Some(at.0)
            }
            _ => None,
        })
        .reduce(f64::min)
}

/// Step the harness until its clock reaches `t`.
fn step_to(h: &mut Harness, t: f64) {
    while h.now.0 < t {
        h.step();
    }
}

/// How late, by the harness clock, a thing due is seen done: the step it falls due in runs, and
/// the clock is read after that step has moved it on.
const LATE: f64 = 2.0 * yalain_testkit::STEP;

/// The longest gap between a rider's looks: the least and all the spread.
const MOST_GAP: f64 =
    yalain_world::fellowship::CHECK_EVERY + yalain_world::fellowship::CHECK_SPREAD;

/// A rider is looked at where it is every 3.5 s and up to 4 s more, evenly, the first anywhere in
/// one gap from the mount: outdoors it rides on, and in and out again between two looks it rides
/// on; walked in to stay, it is put off at the first look after, never later than one gap.
/// Behaviour: server.mounts.rider-looked-at-as-a-fellow-is
#[test]
fn a_rider_walked_indoors_is_put_off_at_its_next_look() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let egg = give(&mut h, world, EGG);
    send_use(&mut h, b, me, egg);
    let at = h.server.players[&world].position;
    let stamp = h.server.players[&world].teleport_stamp;
    let report = |cell: u32| {
        yalain_testkit::play::report(
            cell,
            auberean_primitives::Vec3::new(at.origin[0], at.origin[1], at.origin[2]),
            0,
            yalain_world::world::IDLE_MOTION,
            stamp,
        )
    };
    let first = next_look(&h, world).expect("a look due");
    assert!(first - h.now.0 <= MOST_GAP, "the first within one gap");
    // Mounted again and again, the first look falls anywhere in a gap: some sooner than the least.
    let mut firsts = Vec::new();
    for _ in 0..12 {
        send_dismount(&mut h, b);
        send_use(&mut h, b, me, egg);
        firsts.push(next_look(&h, world).expect("a look due") - h.now.0);
    }
    assert!(
        firsts.iter().all(|f| (-LATE..=MOST_GAP).contains(f)),
        "{firsts:?}"
    );
    assert!(
        firsts
            .iter()
            .any(|f| *f < yalain_world::fellowship::CHECK_EVERY - 1.0),
        "{firsts:?}"
    );
    let first = next_look(&h, world).expect("a look due");
    // Outdoors, look after look: the ride goes on, the gaps spread over their whole span.
    // Each gap runs from the step its look ran in, the one whose clock first reaches the look's
    // time, not from that time.
    let mut gaps = Vec::new();
    let mut due = first;
    for _ in 0..40 {
        step_to(&mut h, due);
        let looked = h.now.0;
        h.step();
        let next = next_look(&h, world).expect("looked at again");
        assert!(next > due, "looked at in the step its clock reached");
        gaps.push(next - looked);
        due = next;
    }
    assert!(
        h.server.players[&world].riding.is_some(),
        "outdoors it rides on"
    );
    let least = gaps.iter().copied().fold(f64::MAX, f64::min);
    let most = gaps.iter().copied().fold(0.0, f64::max);
    assert!(
        least >= yalain_world::fellowship::CHECK_EVERY - 1e-9,
        "least gap {least}"
    );
    assert!(most <= MOST_GAP + 1e-9, "longest gap {most}");
    assert!(least < 4.5 && most > 6.5, "gaps from {least} to {most}");
    // In and out again between two looks: the ride goes on.
    h.bots[b].send(true, queue::WEENIE, &report(at.cell & 0xFFFF_0000 | 0x0105));
    h.run(2);
    h.bots[b].send(true, queue::WEENIE, &report(at.cell));
    h.run(2);
    assert!(h.now.0 < due, "out before the look");
    step_to(&mut h, due + yalain_testkit::STEP);
    assert!(
        h.server.players[&world].riding.is_some(),
        "out again in time"
    );
    // In to stay: off at the next look.
    let due = next_look(&h, world).expect("a look due");
    h.bots[b].send(true, queue::WEENIE, &report(at.cell & 0xFFFF_0000 | 0x0105));
    h.run(1);
    let went_in = h.now.0;
    assert!(h.run_until(200, |h| h.server.players[&world].riding.is_none()));
    let off = h.now.0;
    assert!(off >= went_in, "not before it went in");
    assert!(
        off - went_in <= MOST_GAP + LATE,
        "off {} s after going in",
        off - went_in
    );
    assert!(
        (0.0..=LATE).contains(&(off - due)),
        "off at the look due at {due}, not {off}"
    );
}

/// A rider sent inside by portal is put off at its first look after it arrives: never before it
/// arrives, never later than one gap.
/// Behaviour: server.mounts.ride-ends-indoors
#[test]
fn a_rider_arriving_indoors_by_portal_is_put_off_at_its_next_look() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let egg = give(&mut h, world, EGG);
    send_use(&mut h, b, me, egg);
    let mut to = h.server.players[&world].position;
    to.cell = to.cell & 0xFFFF_0000 | 0x0105;
    let clock = h.clock();
    yalain_world::places::sent_away(&mut h.server, &mut h.net, world, to, clock);
    assert!(h.run_until(200, |h| h.server.players[&world].trip.is_none()));
    let arrived = h.now.0;
    assert_eq!(
        h.server.players[&world].position.cell, to.cell,
        "arrived inside"
    );
    assert!(
        h.server.players[&world].riding.is_some(),
        "a portal keeps the ride"
    );
    let due = next_look(&h, world).expect("a look due");
    assert!(h.run_until(400, |h| h.server.players[&world].riding.is_none()));
    let off = h.now.0;
    assert!(off >= arrived, "not before it arrived");
    assert!(
        off - arrived <= MOST_GAP + LATE,
        "off {} s after arriving",
        off - arrived
    );
    assert!(
        (0.0..=LATE).contains(&(off - due)),
        "off at the look due at {due}, not {off}"
    );
}

/// A resurrection ends a ride as a dismount does.
/// Behaviour: server.mounts.ride-ends-at-resurrection
#[test]
fn a_resurrection_ends_a_ride() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let saddle = give(&mut h, world, SADDLE);
    send_use(&mut h, b, me, saddle);
    let from = h.bots[b].inbox.len();
    yalain_world::effects::resurrected(&mut h.server, &mut h.net, world);
    h.run(3);
    assert!(h.server.players[&world].riding.is_none());
    assert_eq!(
        told(&h, b, from, me),
        ["look own", "speed 1.00", "stop 41000536", "fx 41000537"]
    );
    assert_eq!(flags(&h, b, from), [0]);
}

/// A rider's pet attack order is refused with its own text, before anything else is done.
/// Behaviour: server.mounts.pet-orders-refused
#[test]
fn a_riders_pet_attack_order_is_refused() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let egg = give(&mut h, world, EGG);
    send_use(&mut h, b, me, egg);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(PET_ATTACK, &[Arg::LongInt(0), Arg::LongInt(me)]),
    );
    h.run(4);
    assert_eq!(shown(&h, b, from), [mounts::PET_ORDER_REFUSED.0]);
}

/// A rider who logs off leaves without its mount: the record goes with the leaving, and the look
/// shown as it leaves is its own.
/// Behaviour: server.mounts.ride-ends-at-logoff
#[test]
fn a_rider_logging_off_leaves_without_its_mount() {
    use yalain_world::world::log_off;
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let saddle = give(&mut h, world, SADDLE);
    send_use(&mut h, b, me, saddle);
    let from = h.bots[b].inbox.len();
    let asked = h.now.0;
    h.bots[b].send(
        false,
        queue::LOGON,
        &auberean_protocol::ClientMessage::LogOff(auberean_protocol::login::CharacterRef {
            character: me,
        }),
    );
    while h.now.0 < asked + log_off::LEAVES_AFTER - 0.3 {
        h.step();
    }
    assert!(
        h.server.players[&world].riding.is_some(),
        "riding until it leaves"
    );
    while h.now.0 < asked + log_off::LEAVES_AFTER + 0.5 {
        h.step();
    }
    let looks = told(&h, b, from, me);
    assert_eq!(looks.last().map(String::as_str), Some("look own"));
    let kept = h
        .server
        .store
        .world_state(me)
        .ok()
        .flatten()
        .and_then(|st| yalain_world::state::PlayState::from_bytes(&st.play).ok())
        .expect("kept");
    assert!(
        kept.effects.iter().all(|e| e.state != LEY),
        "the record is gone"
    );
}

/// A player who comes into view of a rider sees it mounted: the mounted look and its speed.
/// Behaviour: server.mounts.seen-mounted
#[test]
fn a_player_who_comes_into_view_sees_the_rider_mounted() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h, "one", "Elara");
    let saddle = give(&mut h, world, SADDLE);
    send_use(&mut h, b, me, saddle);
    let (c, _, _) = in_world(&mut h, "two", "Torvald");
    let made = h.world_messages(c, 0).into_iter().find_map(|m| match m {
        ServerMessage::CreateObject(o) if o.id == me => Some(o),
        _ => None,
    });
    let made = made.expect("the rider is made");
    assert_eq!(made.visual.motion_interp, MOTION);
    assert!((made.physics.velocity_scale - 1.4).abs() < 1e-6);
}

/// The mounted look carries the rider's appearance over its body's defaults: an entry the rider
/// sets keeps its value, one only the body sets is added, and the table's own id goes.
/// Behaviour: server.mounts.mounted-look
#[test]
fn the_mounted_look_carries_the_riders_appearance_over_its_bodys_defaults() {
    let mut m = yalain_world::mounts::Mounts::default();
    m.looks.insert((LEY_TYPE, 1, 0x1000), table_look());
    m.bodies.insert(
        0x1F00_007A,
        Table::from_inserted(
            2,
            [(0x2000_000A, Table::from_inserted(2, [(3, 1.0), (12, 0.6)]))],
        ),
    );
    let own = VisualDesc {
        bits: VisualDesc::PARENT | VisualDesc::GLOBAL,
        parent: 0x1F00_007A,
        global: PartGroupDesc {
            bits: PartGroupDesc::KEY | PartGroupDesc::APPEARANCE,
            key: 0x4000_0002,
            appearance: Table::from_inserted(
                2,
                [
                    (
                        0x2000_000A,
                        Table::from_inserted(2, [(3, 0.5), (11, 0.7), (0x4100_0003, 0.05)]),
                    ),
                    (0x2000_0000, Table::from_inserted(2, [(1, 0.36)])),
                ],
            ),
            ..PartGroupDesc::default()
        },
        ..VisualDesc::default()
    };
    let look = m
        .mounted_look(LEY_TYPE, 1, 0x1000, &own)
        .expect("a look for the type, race and sex");
    assert_eq!(look.bits & VisualDesc::DID, 0);
    assert_eq!(look.motion_interp, MOTION);
    assert_eq!(look.global.fx_table, 0x1E00_0039);
    let face = look
        .global
        .appearance
        .get(0x2000_000A)
        .expect("the rider's entry");
    assert_eq!(
        face.entries,
        [(3, 0.5), (11, 0.7), (12, 0.6), (0x4100_0003, 0.05)]
    );
    assert_eq!(
        look.global
            .appearance
            .entries
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        own.global
            .appearance
            .entries
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>(),
        "the rider's entries in its order"
    );
    assert!(m.mounted_look(HERO_TYPE, 1, 0x1000, &own).is_none());
}
