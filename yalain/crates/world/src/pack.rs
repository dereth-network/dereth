//! What a player does with its pack: moving items about it and its bags, turning an item into
//! money in the money bag, and taking everything out of a container it has open.
//!
//! **Reorganizing.** The client asks to move an item of its pack to a slot of the pack or a bag
//! (the slot it names counts the places after the item has left its own). The item leaves its
//! slot, that segment closing up, and goes in at the slot named, the items there moving up one;
//! the move is answered with the slot the server held it at and the slot one before the target
//! when it came from an earlier slot (else the target), and its container is cleared and set
//! again. Part of a stack moved splits off as a new stack at the slot, holding all the old one
//! held; an item moved onto a stack of its kind with its numbers and room joins it, and one with
//! other numbers goes in beside it. Asked while another action is under way, it is refused and
//! nothing moves.
//!
//! **The money bag.** An item of the pack dragged to the money bag is sold for its value, for each
//! piece sold: the player is told how much gold was added, its money goes up, and the item is
//! destroyed (part of a stack: the stack goes down). Asked while busy, it is refused with a text.
//!
//! **Out of the money bag.** Coins dragged out of the money bag come into the pack as a stack of
//! that many, and the money goes down by as many.
//!
//! **Taking all.** The player plays its picking-up behavior; when that is through every item of
//! the container it has open comes into its pack in the container's order, each answered as a
//! take, then the request is answered with the items taken and those left. A container the player
//! does not have open, or a request while another action is under way, is refused.

use auberean_protocol::physics::{ObjectEvent, ObjectOnly};
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::types::{BehaviorParams, ObjectStamp};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::library::Elem;
use auberean_script::native_object::Item;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::{Node, Value};
use crate::graph::{list, tag, Builder, Slot};
use crate::inventory::{self as inv, move_done, move_graph, q, Move};
use crate::state::Held;
use crate::{progress, session, Clock, Server, Timer};

/// The pack's functions.
pub mod function {
    /// The client asks to move an item about its pack.
    pub const REORGANIZE: u32 = 0x820E_0049;
    pub const REORGANIZE_DONE: u32 = 0x820E_0055;
    /// The client asks to take everything from a container.
    pub const TAKE_ALL: u32 = 0x820E_0068;
    pub const TAKE_ALL_DONE: u32 = 0x820E_006C;
    /// The client drags an item of its pack to the money bag.
    pub const TO_MONEY_BAG: u32 = 0x820F_0002;
    pub const TO_MONEY_BAG_DONE: u32 = 0x820F_0009;
    /// Coins dragged out of the money bag, and the answer.
    pub const FROM_MONEY_BAG: u32 = 0x820F_0001;
    pub const FROM_MONEY_BAG_DONE: u32 = 0x820F_0006;
}

/// Why a pack operation was refused, as its answer says.
pub mod status {
    /// Another action is under way.
    pub const ACTIONS_LOCKED: u32 = 1029;
    /// The player does not have the container open.
    pub const NOT_VIEWER: u32 = 1052;
    /// The segment or the pack has no room.
    pub const CONTAINER_FULL: u32 = 1085;
}

/// "Adding %1 pieces of Gold.", the gold as its variable, shown as plain text; "You are too busy
/// to do that." and "That item cannot be transformed into money.", shown as errors.
pub mod text {
    pub const ADDING_GOLD: (u32, u32) = (0x7D58_FE6D, 0x2500_05E4);
    pub const GOLD: u32 = 0xBB3E_745B;
    pub const GOLD_KIND: u16 = 2;
    pub const ADDING_SHOWN_AS: u32 = 8;
    pub const TOO_BUSY: (u32, u32) = (0x31AA_76FF, 0x2500_05E4);
    pub const TOO_BUSY_SHOWN_AS: u32 = 1;
    pub const NO_WORTH: (u32, u32) = (0xD4B3_FC38, 0x2500_05E4);
}

/// The script package of a take-all's description and its slots.
pub const TAKE_ALL_DESC: u16 = 0x1B58;
pub const TAKE_ALL_DESC_SLOTS: usize = 14;

/// Taking all: the picking-up behavior and its effect script.
pub const TAKING: u32 = 0x4100_0020;
pub const TAKE_ALL_SCRIPT: u32 = 0x5700_0008;

/// How long after the picking-up behavior the items come into the pack, in seconds (2017: the
/// median of 142 take-alls, from the behavior to the first item's move).
pub const TAKE_ALL_SECONDS: f64 = 1.72;

fn reorganize_done(m: &Move) -> ServerMessage {
    session::call(function::REORGANIZE_DONE, &[move_graph(m)]).expect("a move packs")
}

fn item_slot(slot: u32) -> i32 {
    i32::try_from(slot).unwrap_or(-1)
}

/// The container of an item changed in its holder's sight: cleared, then the player.
fn contained_again<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    player: u64,
) {
    for value in [0, player] {
        let m = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
            object: inv::visual(s, item),
            quality: q::CONTAINER,
            value,
        });
        inv::to_holder(s, net, world, item, &m);
    }
}

fn quantity_shown<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, item: u64) {
    let n = s.things.get(item).map_or(1, |t| t.quantity.max(1));
    let m = ServerMessage::UpdateIntVisual(VisualQuality {
        object: inv::visual(s, item),
        quality: crate::craft::QUANTITY,
        value: i32::try_from(n).unwrap_or(i32::MAX),
    });
    inv::to_holder(s, net, world, item, &m);
}

fn destroyed<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, item: u64) {
    if let Some(p) = s.players.get_mut(&world) {
        p.known.remove(&item);
    }
    let instance = s.things.remove(item).map_or(0, |t| t.instance);
    let gone = ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
        id: item,
        instance_stamp: instance,
        other_stamp: 0,
    }));
    net.send(world, queue::EVENT, BlobKind::UNORDERED, gone.encode());
}

/// The slot an item moved to goes in at: the slot named, no further than just past its segment's
/// items. `None` when the segment has no room.
fn place_for(held: &[Held], target: u32) -> Option<u32> {
    // A client's slot past the bags is just past the last bag's items.
    let segment = inv::segment_of(target.min(inv::PACK_ROOM - 1));
    let n = inv::segment_counts(held)[segment];
    if n >= inv::SEGMENTS[segment] {
        return None;
    }
    Some(target.min(inv::segment_start(segment) + n))
}

/// The stack an item moved to a slot would join: the item there, of the same template and the same
/// numbers, one that stacks, with room for `quantity` more.
fn stack_at<S: Store>(
    s: &Server<S>,
    held: &[Held],
    slot: u32,
    moved: u64,
    template: u32,
    quantity: u32,
) -> Option<u64> {
    let most = s
        .catalog
        .get(template)
        .and_then(|t| t.props.int("Item_MaxStackSize"))
        .unwrap_or(0);
    held.iter()
        .find(|h| h.location == 0 && h.slot == slot)
        .filter(|h| {
            most > 1
                && h.template == template
                && inv::same_numbers(s, h.id, moved)
                && i64::from(h.quantity.max(1)) + i64::from(quantity) <= most
        })
        .map(|h| h.id)
}

/// A player asks to move an item about its pack.
pub fn reorganize<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let Some(req) = crate::drops::requested(v) else {
        s.unhandled.note("a reorganization that does not read");
        return;
    };
    if crate::places::actions_locked(s, world) {
        let m = Move {
            status: status::ACTIONS_LOCKED,
            ..req
        };
        progress::private(net, world, &reorganize_done(&m));
        return;
    }
    let Some((player, mut held)) = s
        .players
        .get(&world)
        .map(|p| (p.character, p.play.items.clone()))
    else {
        return;
    };
    let Some(h) = held
        .iter()
        .find(|h| h.id == req.item && h.location == 0)
        .copied()
    else {
        s.unhandled
            .note("reorganizing an item the pack does not hold");
        return;
    };
    let (look, quantity) = s
        .things
        .get(h.id)
        .map_or((0, 1), |t| (t.description.look, t.quantity.max(1)));
    let moved = req.quantity.clamp(1, quantity);
    let target = u32::try_from(req.target_slot.max(0)).unwrap_or(0);
    let answer = Move {
        merge: false,
        actual_from_slot: item_slot(h.slot),
        target: player,
        actual_target: player,
        from: player,
        actual_from: player,
        merge_container: player,
        template: h.template,
        look_entity: look,
        quantity: moved,
        quantity_left: 0,
        auto_merge: false,
        no_check: true,
        ..req
    };
    if moved < quantity {
        split(s, net, world, &mut held, &h, moved, target, answer);
        return;
    }
    // The item leaves its slot, its segment closing up.
    held.retain(|x| x.id != h.id);
    inv::close_up(&mut held);
    if let Some(stack) = stack_at(s, &held, target, h.id, h.template, moved) {
        let m = Move {
            merge: true,
            merge_slot: -1,
            actual_target_slot: item_slot(target),
            ..answer
        };
        progress::private(net, world, &reorganize_done(&m));
        if let Some(x) = held.iter_mut().find(|x| x.id == stack) {
            x.quantity = x.quantity.max(1) + moved;
            if let Some(t) = s.things.get_mut(stack) {
                t.quantity = x.quantity;
            }
        }
        quantity_shown(s, net, world, stack);
        destroyed(s, net, world, h.id);
        finish(s, world, held);
        return;
    }
    let Some(slot) = place_for(&held, target) else {
        let m = Move {
            status: status::CONTAINER_FULL,
            ..answer
        };
        progress::private(net, world, &reorganize_done(&m));
        return;
    };
    inv::insert_at(&mut held, slot);
    held.push(Held { slot, ..h });
    let m = Move {
        actual_target_slot: item_slot(slot),
        merge_slot: if h.slot < slot {
            item_slot(slot) - 1
        } else {
            item_slot(slot)
        },
        split_item: h.id,
        split_quantity: 0,
        all_taken: true,
        ..answer
    };
    progress::private(net, world, &reorganize_done(&m));
    if let Some(t) = s.things.get_mut(h.id) {
        t.container = Some((player, slot));
    }
    contained_again(s, net, world, h.id, player);
    finish(s, world, held);
}

/// Part of a stack moved splits off as a new stack at the slot.
#[allow(clippy::too_many_arguments)]
fn split<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    held: &mut Vec<Held>,
    h: &Held,
    moved: u32,
    target: u32,
    answer: Move,
) {
    let Some(slot) = place_for(held, target) else {
        let m = Move {
            status: status::CONTAINER_FULL,
            ..answer
        };
        progress::private(net, world, &reorganize_done(&m));
        return;
    };
    let Some(new) = inv::split_off(s, h.id, moved) else {
        return;
    };
    let player = answer.target;
    inv::insert_at(held, slot);
    held.push(Held {
        id: new,
        template: h.template,
        slot,
        location: 0,
        quantity: moved,
    });
    let left = h.quantity.max(1) - moved;
    if let Some(x) = held.iter_mut().find(|x| x.id == h.id) {
        x.quantity = left;
    }
    if let Some(t) = s.things.get_mut(h.id) {
        t.quantity = left;
    }
    if let Some(t) = s.things.get_mut(new) {
        t.container = Some((player, slot));
    }
    let contained = inv::contain_stamp(s, new);
    net.send(
        world,
        queue::EVENT,
        BlobKind::UNORDERED,
        ServerMessage::Contain(ObjectOnly(contained)).encode(),
    );
    let m = Move {
        actual_target_slot: item_slot(slot),
        merge_slot: item_slot(slot),
        split_item: new,
        split_quantity: moved,
        all_taken: false,
        ..answer
    };
    progress::private(net, world, &reorganize_done(&m));
    quantity_shown(s, net, world, h.id);
    if let Some(t) = s.things.get(new) {
        let create = ServerMessage::CreateObject(inv::item_create(t));
        net.send(world, queue::EVENT, BlobKind::UNORDERED, create.encode());
    }
    inv::knows(s, world, new);
    finish(s, world, std::mem::take(held));
}

/// The pack as it now stands is the player's, kept.
fn finish<S: Store>(s: &mut Server<S>, world: ConnId, held: Vec<Held>) {
    if let Some(p) = s.players.get_mut(&world) {
        p.play.items = held;
        inv::close_up(&mut p.play.items);
    }
    crate::world::keep_player(s, world);
}

/// A player drags an item of its pack to the money bag.
pub fn to_money_bag<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    // The values come last parameter first: the container, the slot, how many, the item.
    let longs: Vec<u64> = v
        .iter()
        .filter_map(|x| match x {
            Value::Long(l) => Some(*l),
            _ => None,
        })
        .collect();
    let ints: Vec<u32> = v
        .iter()
        .filter_map(|x| match x {
            Value::Int(i) => Some(*i),
            _ => None,
        })
        .collect();
    let (&[container, item], &[from_slot, quantity]) = (longs.as_slice(), ints.as_slice()) else {
        s.unhandled
            .note("a drag to the money bag that does not read");
        return;
    };
    let done = |item: u64| {
        session::call(
            function::TO_MONEY_BAG_DONE,
            &[
                Arg::LongInt(container),
                Arg::Int(from_slot),
                Arg::LongInt(item),
            ],
        )
        .expect("packs")
    };
    if crate::places::actions_locked(s, world) {
        let busy = crate::chat::show_text(text::TOO_BUSY_SHOWN_AS, progress::text(text::TOO_BUSY));
        progress::private(net, world, &busy);
        progress::private(net, world, &done(item));
        return;
    }
    let held = s
        .players
        .get(&world)
        .and_then(|p| {
            p.play
                .items
                .iter()
                .find(|h| h.id == item && h.location == 0)
        })
        .copied();
    let Some(h) = held else {
        s.unhandled
            .note("a drag to the money bag of an item the pack does not hold");
        return;
    };
    into_money_bag(s, net, world, h, quantity, &done);
}

/// What so many of an item bring into the money bag: each piece its own value when it has one,
/// coins one each (2017: 3 of 3 coins dragged, 15 of 15 used), any other its template's. (No stack
/// with a value of its own was recorded, 0 of 2,228 examined; each piece paying is this server's
/// choice, so that selling a stack whole or piece by piece pays the same.)
fn worth<S: Store>(s: &Server<S>, template: u32, own_value: u32, n: u32) -> u32 {
    if own_value > 0 {
        own_value.saturating_mul(n)
    } else if template == COINS {
        n
    } else {
        inv::item_value(s, template).saturating_mul(n)
    }
}

/// So many of an item of the pack go into the money bag: "Adding %1 pieces of Gold.", the money
/// up by their worth, the stack down by as many or the item gone, then `done` answered (with the
/// item when some is left, else none). An item worth nothing stays, refused.
fn into_money_bag<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    h: Held,
    quantity: u32,
    done: &dyn Fn(u64) -> ServerMessage,
) {
    let item = h.id;
    let Some((own_value, have)) = s.things.get(item).map(|t| (t.value, t.quantity.max(1))) else {
        return;
    };
    let sold = quantity.clamp(1, have);
    let gold = worth(s, h.template, own_value, sold);
    // An item of no worth is refused and kept, as a client refuses it (2017: none of 753 sold for
    // nothing).
    if gold == 0 {
        let refused =
            crate::chat::show_text(text::TOO_BUSY_SHOWN_AS, progress::text(text::NO_WORTH));
        progress::private(net, world, &refused);
        progress::private(net, world, &done(item));
        return;
    }
    let added = crate::chat::show_text(
        text::ADDING_SHOWN_AS,
        progress::text_with(
            text::ADDING_GOLD,
            vec![progress::word(text::GOLD, text::GOLD_KIND, gold)],
        ),
    );
    progress::private(net, world, &added);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let money = crate::store::money(&p.qualities).saturating_add(gold);
    crate::quals::set_int(&mut p.qualities, crate::store::MONEY, money.cast_signed());
    progress::send_int(net, world, crate::store::MONEY, money.cast_signed());
    if sold < have {
        if let Some(x) = p.play.items.iter_mut().find(|x| x.id == item) {
            x.quantity = have - sold;
        }
        if let Some(t) = s.things.get_mut(item) {
            t.quantity = have - sold;
        }
        quantity_shown(s, net, world, item);
        progress::private(net, world, &done(0));
    } else {
        p.play.items.retain(|x| x.id != item);
        inv::close_up(&mut p.play.items);
        let gone =
            session::call(inv::function::DISCRETE_DESTROY, &[Arg::LongInt(item)]).expect("packs");
        progress::private(net, world, &gone);
        progress::private(net, world, &done(0));
        destroyed(s, net, world, item);
    }
    crate::world::keep_player(s, world);
}

/// A player uses coins of its pack: the whole stack goes into its money bag as a drag of it would,
/// answered as a drag from the player at slot 0 and not as a use (2017: 15 of 15). Returns false
/// for any other item.
pub fn coins_used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &crate::places::Use,
) -> bool {
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    let Some(h) = p
        .play
        .items
        .iter()
        .find(|h| h.id == u.item && h.location == 0 && h.template == COINS)
        .copied()
    else {
        return false;
    };
    let me = p.character;
    let done = |item: u64| {
        session::call(
            function::TO_MONEY_BAG_DONE,
            &[Arg::LongInt(me), Arg::Int(0), Arg::LongInt(item)],
        )
        .expect("packs")
    };
    let all = s.things.get(h.id).map_or(1, |t| t.quantity.max(1));
    into_money_bag(s, net, world, h, all, &done);
    true
}

/// The coins' template: what the money bag gives when coins are dragged out of it.
pub const COINS: u32 = 0x4700_0538;

/// A player drags coins out of its money bag: a stack of that many coins comes into its pack, its
/// money goes down by as many, and the drag is answered with the container it named. More than it
/// has is not answered (its client does not ask).
pub fn from_money_bag<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
) {
    // The values come last parameter first: the container, the slot, how many.
    let (Some(Value::Long(container)), Some(Value::Int(_slot)), Some(Value::Int(quantity))) =
        (v.first(), v.get(1), v.get(2))
    else {
        s.unhandled
            .note("a drag from the money bag that does not read");
        return;
    };
    let (container, quantity) = (*container, *quantity);
    let Some(money) = s
        .players
        .get(&world)
        .map(|p| crate::store::money(&p.qualities))
    else {
        return;
    };
    if quantity == 0 || quantity > money {
        return;
    }
    // Into a full pack, or from a player busy or dead, it is not answered either.
    let full = s
        .players
        .get(&world)
        .is_none_or(|p| inv::pack_full(&p.play.items));
    if full || crate::places::actions_locked(s, world) {
        return;
    }
    if inv::give(s, net, world, COINS, quantity).is_none() {
        return;
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let left = money - quantity;
    crate::quals::set_int(&mut p.qualities, crate::store::MONEY, left.cast_signed());
    progress::send_int(net, world, crate::store::MONEY, left.cast_signed());
    let done =
        session::call(function::FROM_MONEY_BAG_DONE, &[Arg::LongInt(container)]).expect("packs");
    progress::private(net, world, &done);
    crate::world::keep_player(s, world);
}

/// A take-all's description as the client sent it: its container and its player.
#[must_use]
pub fn take_all_requested(v: &[Value]) -> Option<(u64, u64)> {
    let slots = v.iter().find_map(|x| match x.root() {
        Some(Node::Script { package, slots })
            if *package == TAKE_ALL_DESC && slots.len() >= TAKE_ALL_DESC_SLOTS =>
        {
            Some(slots.clone())
        }
        _ => None,
    })?;
    let long = |i: usize| u64::from(slots[i + 1].1) << 32 | u64::from(slots[i].1);
    Some((long(6), long(12)))
}

/// A take-all's answer: its status and last error, the container, the items taken and those left,
/// the player.
#[must_use]
pub fn take_all_done(
    status: u32,
    last_error: u32,
    container: u64,
    taken: &[u64],
    left: &[u64],
    player: u64,
) -> ServerMessage {
    let mut b = Builder::new();
    let root = b.reserve(TAKE_ALL_DESC);
    let ids = |v: &[u64]| list(Elem::Long, v.iter().map(|i| Item::Long(*i)).collect());
    let not_taken = b.native(inv::LONG_LIST, ids(left));
    let taken = b.native(inv::LONG_LIST, ids(taken));
    #[allow(clippy::cast_possible_truncation)]
    b.fill(
        root,
        &[
            Slot::Int(last_error),
            Slot::Int(0),
            Slot::Int(1),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(status),
            Slot::Tagged(tag::LONG, container as u32),
            Slot::Tagged(tag::HIGH, (container >> 32) as u32),
            Slot::Ref(Some(not_taken)),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Ref(Some(taken)),
            Slot::Tagged(tag::LONG, player as u32),
            Slot::Tagged(tag::HIGH, (player >> 32) as u32),
        ],
    );
    session::call(function::TAKE_ALL_DONE, &[Arg::Object(b.graph())]).expect("packs")
}

/// A player asks to take everything out of a container: refused unless it has the container open
/// and nothing else under way; else it plays its picking-up behavior, and the rest when that is
/// through.
pub fn take_all<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let Some((container, _)) = take_all_requested(v) else {
        s.unhandled.note("a take-all that does not read");
        return;
    };
    let Some(player) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let busy = crate::places::actions_locked(s, world)
        || s.timers
            .iter()
            .any(|(_, t)| matches!(t, Timer::TakeAll { world: w, .. } if *w == world));
    let why = if busy {
        status::ACTIONS_LOCKED
    } else if inv::open_corpse_of(s, world) != Some(container) {
        status::NOT_VIEWER
    } else {
        0
    };
    if why != 0 {
        let done = take_all_done(why, 0, container, &[], &[], player);
        progress::private(net, world, &done);
        return;
    }
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.visual = p.visual.wrapping_add(1);
    let mut b = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR
            | BehaviorParams::FX_SCRIPT
            | 0x100
            | BehaviorParams::CONTEXT,
        behavior: TAKING,
        fx_script: TAKE_ALL_SCRIPT,
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
    crate::combat::to_all_seeing(s, net, player, &behave);
    s.after(clock, TAKE_ALL_SECONDS, Timer::TakeAll { world, container });
}

/// The picking-up behavior is through: each item of the container comes into the pack in the
/// container's order, at the front of the pack (or of the first bag with room), answered as a take;
/// one of a template the pack holds a stack of joins it; one the pack has no room for stays. Then
/// the request is answered.
pub fn took_all<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    container: u64,
    clock: Clock,
) {
    let Some(player) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let contents = s
        .things
        .get(container)
        .and_then(|t| t.corpse.as_ref())
        .map(|c| c.contents.clone())
        .or_else(|| crate::usable::chests::contents(s, container))
        .unwrap_or_default();
    let (mut taken, mut left) = (Vec::new(), Vec::new());
    let mut last_error = 0;
    for item in contents {
        let Some((quantity, template, look)) = s
            .things
            .get(item)
            .map(|t| (t.quantity.max(1), t.template, t.description.look))
        else {
            continue;
        };
        let m = Move {
            move_type: inv::move_type::TAKE,
            item,
            split_item: item,
            quantity,
            from: container,
            actual_from: container,
            target: player,
            actual_target: player,
            merge_container: player,
            all_taken: true,
            auto_merge: true,
            contain: true,
            grab: true,
            unlock: true,
            check_take: true,
            no_anim: true,
            no_move: true,
            quiet: true,
            template,
            look_entity: look,
            split_quantity: quantity,
            ..Move::default()
        };
        if let Some((stack, _, had)) = inv::stack_for(s, world, item, quantity) {
            let m = Move {
                merge: true,
                merge_slot: -1,
                split_item: 0,
                ..m
            };
            progress::private(net, world, &move_done(&m));
            let total = had + quantity;
            if let Some(h) = s
                .players
                .get_mut(&world)
                .and_then(|p| p.play.items.iter_mut().find(|h| h.id == stack))
            {
                h.quantity = total;
            }
            if let Some(t) = s.things.get_mut(stack) {
                t.quantity = total;
            }
            quantity_shown(s, net, world, stack);
            destroyed(s, net, world, item);
        } else {
            let full = s
                .players
                .get(&world)
                .is_none_or(|p| inv::pack_full(&p.play.items));
            if full {
                last_error = status::CONTAINER_FULL;
                left.push(item);
                continue;
            }
            let Some(p) = s.players.get_mut(&world) else {
                return;
            };
            let slot = inv::room_for_one(&mut p.play.items);
            p.play.items.push(Held {
                id: item,
                template,
                slot,
                location: 0,
                quantity,
            });
            if let Some(t) = s.things.get_mut(item) {
                t.container = Some((player, slot));
                t.at = None;
            }
            let contained = inv::contain_stamp(s, item);
            net.send(
                world,
                queue::EVENT,
                BlobKind::UNORDERED,
                ServerMessage::Contain(ObjectOnly(contained)).encode(),
            );
            let m = Move {
                target_slot: item_slot(slot),
                actual_target_slot: item_slot(slot),
                merge_slot: item_slot(slot),
                ..m
            };
            progress::private(net, world, &move_done(&m));
            contained_again(s, net, world, item, player);
        }
        taken.push(item);
        if let Some(c) = s.things.get_mut(container).and_then(|t| t.corpse.as_mut()) {
            c.contents.retain(|x| *x != item);
        }
        crate::quests::took(s, net, world, template, clock);
    }
    let done = take_all_done(0, last_error, container, &taken, &left, player);
    progress::private(net, world, &done);
    crate::world::keep_player(s, world);
}
