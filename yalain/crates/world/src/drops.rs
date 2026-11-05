//! A player puts an item of its pack down on the ground.
//!
//! The client asks to move the item to no object (move type 0x21). At once the player plays its
//! putting-down behavior with the item's look in its hand, seen by everyone who sees the player,
//! and the item's container is cleared. When the behavior is through (1.75 s for a human man,
//! about 1.97 s for Lugians and Tumeroks, 1.56 s for an Empyrean woman, 3.57 s for a Drudge) the item stands in the world straight ahead of the player
//! where the player then stands, its body's reach times its height away and half a meter up,
//! facing as the player faces: the move is answered and the item's place sent, and the clients that
//! see the place and did not hold the item are made it there; 0.3 s later it has come to rest just
//! above the ground where the player stands. A drop asked while another is under
//! way is refused at once (actions locked); so is one of an item the pack does not hold, or of part
//! of a stack.
//!
//! An item put down lies [`LIES_SECONDS`] from its landing and is then gone, for everyone.

use std::collections::{BTreeMap, VecDeque};

use auberean_protocol::physics::{ObjectEvent, ObjectPosition};
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::types::{BehaviorParams, ObjectStamp};
use auberean_protocol::ServerMessage;
use dereth_primitives::num::math;
use yalain_net::{ConnId, LocalTime, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::calls::{Node, Value};
use crate::inventory::{move_done, q, Move, MOVE_DESC, MOVE_DESC_SLOTS};
use crate::{progress, Clock, Server, Timer};

/// The client's request to move an item.
pub mod function {
    pub const MOVE_ITEM: u32 = 0x820E_0036;
}

/// A move to no object: putting the item down.
pub const DROP: u32 = 0x21;

/// The putting-down behavior and its effect script.
pub mod look {
    pub const PUTTING_DOWN: u32 = 0x4100_0021;
    pub const PUT_DOWN_SCRIPT: u32 = 0x5700_0005;
}

/// Why a move was refused, as its answer says.
pub mod status {
    /// Another action is under way.
    pub const ACTIONS_LOCKED: u32 = 1029;
    /// The item cannot be put there.
    pub const CANNOT_PLACE_IN_WORLD: u32 = 1033;
    /// The item stays with its owner.
    pub const ATTUNED: u32 = 1044;
}

/// How far above the player's feet the item is put, in meters.
pub const LIFT: f32 = 0.5;

/// How far above the player's feet the item comes to rest, in meters, and how long after it is
/// put it is there, in seconds (the 2017 servers' items fell to 0.03-0.1 m within 0.3 s).
pub const REST: f32 = 0.05;
pub const SETTLE_SECONDS: f64 = 0.3;

/// How long an item put down lies, from its landing, in seconds (2017: the 10 of 786 placed items
/// nobody took while a client could see them were told gone 599.95 to 600.09 s after they landed; of
/// the rest, none was told gone later).
pub const LIES_SECONDS: f64 = 600.0;

/// The flag of a position touching the ground.
const ON_GROUND: u32 = 4;

/// The items put down and when each goes, soonest first: each lies as long, so the first to land is
/// the first to go, and a pass of the world looks at no more than what is due.
#[derive(Debug, Default)]
pub struct Lying {
    /// When it goes, the item, and its place's stamp when it came to rest: one taken up and put down
    /// again since has a later place, and a later turn.
    due: VecDeque<(LocalTime, u64, u16)>,
    /// How many of each template have gone by their time, what's left of them nowhere.
    pub gone: BTreeMap<u32, u64>,
}

/// How long the putting-down behavior takes, by race and sex, in seconds (2017: the medians of
/// 584 drops; Drudges take about twice as long as the rest).
#[must_use]
pub fn putting_down_seconds(race: u32, sex: u32) -> f64 {
    let female = sex == crate::chargen::FEMALE;
    match (race, female) {
        (1, false) => 1.75,
        (1, true) | (16, false) => 1.77,
        (2, _) | (4, true) => 1.97,
        (4, false) => 1.96,
        (16, true) => 1.56,
        (8, _) => 3.57,
        _ => 1.93,
    }
}

/// How far ahead the item lands, per unit of the player's height, by race and sex.
#[must_use]
pub fn reach(race: u32, sex: u32) -> f32 {
    let female = sex == crate::chargen::FEMALE;
    match (race, female) {
        (1, false) | (4, _) => 0.785,
        (1, true) => 0.7224,
        (2, _) => 0.667,
        _ => std::f32::consts::FRAC_1_SQRT_2,
    }
}

/// The move a client asks for, from its description's slots.
#[must_use]
pub fn requested(v: &[Value]) -> Option<Move> {
    let slots = v.iter().find_map(|x| match x.root() {
        Some(Node::Script { package, slots })
            if *package == MOVE_DESC && slots.len() >= MOVE_DESC_SLOTS =>
        {
            Some(slots.clone())
        }
        _ => None,
    })?;
    let int = |i: usize| slots[i].1;
    let signed = |i: usize| slots[i].1.cast_signed();
    let on = |i: usize| slots[i].1 != 0;
    let long = |i: usize| u64::from(slots[i + 1].1) << 32 | u64::from(slots[i].1);
    Some(Move {
        merge: on(0),
        actual_from_slot: signed(1),
        target: long(2),
        actual_target: long(8),
        unlock: on(10),
        target_slot: signed(11),
        move_type: int(12),
        check_take: on(13),
        hidden: on(14),
        from: long(16),
        template: int(22),
        all_taken: on(23),
        actual_target_slot: signed(24),
        quantity_left: int(25),
        quantity: int(26),
        generated: on(27),
        split_item: long(28),
        no_anim: on(31),
        merge_container: long(32),
        used_overflow: on(34),
        grab: on(35),
        actual_from: long(36),
        look_entity: int(38),
        from_slot: signed(39),
        status: int(40),
        played_anim: on(41),
        quiet: on(42),
        overflow: on(43),
        item: long(44),
        no_move: on(46),
        contain: on(47),
        split_quantity: int(48),
        auto_merge: on(49),
        merge_slot: signed(50),
        trade: on(20),
        no_check: on(7),
        ignore_attunement: on(5),
        item_appearance: int(6),
    })
}

/// A client asks to move an item: putting it down, or giving it to a person or an object.
pub fn move_item<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let Some(m) = requested(v) else {
        s.unhandled.note("a move of an item that does not read");
        return;
    };
    if m.move_type == crate::gives::GIVE && m.target != 0 {
        crate::gives::asked(s, net, world, m, clock);
        return;
    }
    if m.move_type != DROP || m.target != 0 {
        s.unhandled.note(format!(
            "a move of an item of type {:#x} into {:#x}",
            m.move_type, m.target
        ));
        return;
    }
    put_down(s, net, world, m, clock);
}

/// The move refused at once: its own description answered with why.
fn refuse(net: &mut ServerNet, world: ConnId, m: &Move, why: u32) {
    let answer = Move {
        status: why,
        ..m.clone()
    };
    progress::private(net, world, &move_done(&answer));
}

/// The player starts putting the item down.
fn put_down<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    m: Move,
    clock: Clock,
) {
    let busy = s
        .timers
        .iter()
        .any(|(_, t)| matches!(t, Timer::PutDown { world: w, .. } if *w == world))
        || crate::places::pack_spoken_for(s, world);
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let (me, race, sex) = (p.character, p.race, p.sex);
    let held = p
        .play
        .items
        .iter()
        .find(|h| h.id == m.item && h.location == 0)
        .copied();
    if busy {
        refuse(net, world, &m, status::ACTIONS_LOCKED);
        return;
    }
    let (Some(held), Some(t)) = (held, s.things.get(m.item)) else {
        s.unhandled
            .note("putting down an item the pack does not hold");
        refuse(net, world, &m, status::CANNOT_PLACE_IN_WORLD);
        return;
    };
    if crate::inventory::attuned_now(s, t) {
        refuse(net, world, &m, status::ATTUNED);
        return;
    }
    if m.quantity < t.quantity {
        s.unhandled.note("putting down part of a stack");
        refuse(net, world, &m, status::CANNOT_PLACE_IN_WORLD);
        return;
    }
    let item_look = t.description.look;
    tracing::debug!(
        ?world,
        item = format_args!("{:#x}", m.item),
        asked_slot = m.from_slot,
        slot = held.slot,
        pack = ?p.play.items.iter().map(|h| (h.slot, h.id)).collect::<Vec<_>>(),
        "putting an item down"
    );
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.visual = p.visual.wrapping_add(1);
    let mut b = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR
            | BehaviorParams::FX_SCRIPT
            | BehaviorParams::TARGET
            | 0x100
            | BehaviorParams::CLONE
            | BehaviorParams::CONTEXT,
        behavior: look::PUTTING_DOWN,
        fx_script: look::PUT_DOWN_SCRIPT,
        target: m.item,
        clone_visual: item_look,
        context,
        ..BehaviorParams::default()
    };
    // The word of flag 0x100 (the seventh of the flags followed by a word).
    b.words[6] = 1;
    let behave = ServerMessage::PlayBehavior(ObjectEvent {
        sender: ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        },
        action: b,
    });
    p.play.items.retain(|h| h.id != m.item);
    crate::combat::to_all_seeing(s, net, me, &behave);
    let left = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
        object: crate::inventory::visual(s, m.item),
        quality: q::CONTAINER,
        value: 0,
    });
    crate::inventory::to_holder(s, net, world, m.item, &left);
    if let Some(t) = s.things.get_mut(m.item) {
        t.container = None;
    }
    s.after(
        clock,
        putting_down_seconds(race, sex),
        Timer::PutDown {
            world,
            slot: held.slot,
            asked: Box::new(m),
        },
    );
}

/// A player leaves the world with an item still being put down: it is back in its pack, kept with
/// it, and its timer forgotten.
pub(crate) fn left<S: Store>(s: &mut Server<S>, world: ConnId) {
    let mut pending = Vec::new();
    s.timers.retain(|(_, t)| match t {
        Timer::PutDown {
            world: w,
            slot,
            asked,
        } if *w == world => {
            pending.push((*slot, asked.item));
            false
        }
        _ => true,
    });
    for (slot, item) in pending {
        let Some(me) = s.players.get(&world).map(|p| p.character) else {
            return;
        };
        let Some(t) = s.things.get_mut(item) else {
            continue;
        };
        t.container = Some((me, slot));
        let (template, quantity) = (t.template, t.quantity);
        if let Some(p) = s.players.get_mut(&world) {
            crate::inventory::insert_at(&mut p.play.items, slot);
            p.play.items.push(crate::state::Held {
                id: item,
                template,
                slot,
                location: 0,
                quantity,
            });
        }
    }
}

/// Where an item put down by a player standing at `at`, reaching `reach` meters, lands: straight
/// ahead of it, [`LIFT`] up, facing as it faces; outdoors in the cell that holds that point.
#[must_use]
pub fn landing(at: &StoredPosition, reach: f32) -> StoredPosition {
    let heading = crate::world::heading_of_rotation(at.rotation);
    let h = (f32::from(heading) * 360.0 / 255.0).to_radians();
    let (x, y) = (
        at.origin[0] + reach * math::sinf(h),
        at.origin[1] + reach * math::cosf(h),
    );
    let (cell, x, y) = if crate::presence::indoors(at.cell) {
        (at.cell, x, y)
    } else {
        crate::spawn::outdoor(at.cell >> 16, x, y)
    };
    StoredPosition {
        cell,
        origin: [x, y, at.origin[2] + LIFT],
        rotation: crate::world::rotation_of_heading(heading),
    }
}

/// The putting-down behavior is through: the item stands in the world ahead of the player and the
/// move is answered.
pub fn put<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    slot: u32,
    asked: &Move,
    clock: Clock,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let height = if p.look.bits & auberean_protocol::types::VisualDesc::SCALE != 0 {
        p.look.scale.z
    } else {
        1.0
    };
    let place = landing(&p.position, reach(p.race, p.sex) * height);
    let item = asked.item;
    let Some(t) = s.things.get_mut(item) else {
        return;
    };
    t.container = None;
    let done = Move {
        actual_from_slot: slot.cast_signed(),
        played_anim: true,
        template: t.template,
        look_entity: t.description.look,
        split_quantity: if t.quantity > 1 { asked.quantity } else { 0 },
        ..asked.clone()
    };
    progress::private(net, world, &move_done(&done));
    // The dropper's client holds the item: its places come on a stream of their own from now on.
    if let Some(p) = s.players.get_mut(&world) {
        let seen = crate::presence::Seen {
            position_stream: net.new_stream(world).unwrap_or(0),
            head_stream: net.new_stream(world).unwrap_or(0),
        };
        p.known.insert(item, seen);
    }
    place_item(s, net, item, place, clock);
    s.after(clock, SETTLE_SECONDS, Timer::Settle { item });
    // It is out of the pack for good: a crash after another has picked it up finds it held once.
    crate::world::keep_player(s, world);
}

/// The item put down comes to rest: [`REST`] above the feet of the player that put it down.
pub fn settle<S: Store>(s: &mut Server<S>, net: &mut ServerNet, item: u64, clock: Clock) {
    let Some(at) = s.things.get(item).and_then(|t| t.at) else {
        return;
    };
    let rested = StoredPosition {
        origin: [at.origin[0], at.origin[1], at.origin[2] - (LIFT - REST)],
        ..at
    };
    place_item(s, net, item, rested, clock);
    if let Some(stamp) = s.things.get(item).map(|t| t.position_stamp) {
        let goes = LocalTime(clock.local.0 + LIES_SECONDS - SETTLE_SECONDS);
        s.lying.due.push_back((goes, item, stamp));
    }
}

/// Those whose time is up go, unless a player has them or has put them down again since.
pub fn look<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    while let Some(&(goes, item, stamp)) = s.lying.due.front() {
        if goes.0 > clock.local.0 {
            return;
        }
        s.lying.due.pop_front();
        let still = s
            .things
            .get(item)
            .filter(|t| t.at.is_some() && t.container.is_none() && t.position_stamp == stamp)
            .map(|t| (t.template, t.quantity.max(1)));
        if let Some((template, quantity)) = still {
            *s.lying.gone.entry(template).or_default() += u64::from(quantity);
            crate::inventory::gone_from_where_it_lay(s, net, item);
        }
    }
}

/// The item stands at `place`: its place sent to those who know it, and it made for those who
/// see it there and do not.
fn place_item<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    item: u64,
    place: StoredPosition,
    clock: Clock,
) {
    let Some(t) = s.things.get_mut(item) else {
        return;
    };
    let heading = crate::world::heading_of_rotation(place.rotation);
    t.at = Some(place);
    t.position_stamp = t.position_stamp.wrapping_add(1);
    t.heading_motion = u32::from(heading) << 24 | crate::world::IDLE_MOTION;
    let position = ObjectPosition {
        sender: ObjectStamp {
            id: item,
            instance_stamp: t.instance,
            other_stamp: 0,
        },
        time: net.game_time(clock.local),
        cell: place.cell,
        origin: auberean_primitives::Vec3::new(place.origin[0], place.origin[1], place.origin[2]),
        heading_motion: t.heading_motion,
        flags: ON_GROUND,
        position_stamp: t.position_stamp,
        ..ObjectPosition::default()
    };
    crate::presence::thing_moves(s, net, item, &position, false);
}
