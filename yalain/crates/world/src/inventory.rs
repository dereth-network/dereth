//! A player's inventory: the items in its pack, items given to it, taken from corpses, bought, and
//! turned into money.
//!
//! **Opening a corpse** (in reach, 2 m; the player is walked there first): the corpse flashes and
//! opens for every client that sees it; the opener gets the contents (in slot order) and then each
//! item it does not know yet. No use answer is sent. **Closing** it: it closes for all, the closer
//! gets the answer; it closes so too when the opener walks away from it, as from an open chest.
//!
//! **Taking an item** (the client uses the item inside the open corpse): the player plays its
//! taking behavior, the item leaves the corpse, and once the behavior is through (1.8 s) the item
//! is contained by the player and the move is answered: into a stack it can join where there is
//! one, into a place of its own where the pack has room, and otherwise it stays where it is and the
//! taker is told. **Turning a corpse's items into money**:
//! each item is contained and moved, the money goes up by its value, the item is destroyed in the
//! pack; then the answer with the money earned, each item's object destroyed, and the pack's
//! contents again.
//!
//! **Items given** (a quest's reward, a purchase) are contained, made in the pack and the move
//! answered as a gift (or as a purchase) of the server.
//!
//! **The pack and its bags.** A player's pack is four segments of one list: the pack itself (42
//! places, slots 0-41) and three bags of 12 (slots 42-53, 54-65 and 66-77). A slot names a segment
//! and a place in it; each segment keeps its items together from its first slot.
//!
//! **Where an item comes in.** An item given, bought or taken goes to the front of the first
//! segment with room (slot 0 while the pack itself has room), every item of that segment moving up
//! one, and the move names that slot as both where it was sent and where it went. Into a pack with
//! no room in any segment it goes to the pack itself, at its front: the pack holds more than its
//! places then, as it may, while a bag never does, and a slot past the bags names no segment to the
//! client.

use auberean_protocol::physics::{CreateObject, ObjectEvent, ObjectOnly, Word};
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::text::{StringInfo, Variable, VariableValue};
use auberean_protocol::types::{
    BehaviorParams, ObjectStamp, ParentLink, PhysicsDesc, VisualDesc, WeenieDesc,
};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::library::Elem;
use auberean_script::native_object::Item;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::graph::{list, tag, Builder, Slot};
use crate::progress;
use crate::state::Held;
use crate::things::{Kind, Thing};
use crate::{session, Clock, Server, Timer};

/// The inventory functions.
pub mod function {
    pub const OPEN_DONE: u32 = 0x820E_005D;
    pub const CLOSE: u32 = 0x820E_000C;
    pub const CLOSE_DONE: u32 = 0x820E_0060;
    pub const MOVE_DONE: u32 = 0x820E_005E;
    pub const RESET_CONTENTS: u32 = 0x820E_0053;
    pub const TRANSMUTE_ALL: u32 = 0x820E_0065;
    pub const TRANSMUTE_ALL_DONE: u32 = 0x820E_006A;
    pub const DISCRETE_DESTROY: u32 = 0x820E_0058;
}

/// Effects, modes and behaviors of containers and taking.
pub mod look {
    pub const OPENING_FX: u32 = 0x4100_05D0;
    pub const OPEN_MODE: u32 = 0x4100_0002;
    pub const CLOSED_MODE: u32 = 0x4100_0004;
    pub const TAKING: u32 = 0x4100_0020;
    pub const TAKE_ONE_SCRIPT: u32 = 0x5700_0009;
}

/// Qualities of items and containers.
pub mod q {
    /// The object holding an item (an instance id quality).
    pub const CONTAINER: u32 = 258;
    /// A container is open (a truth quality).
    pub const OPEN: u32 = 702;
}

/// Script packages of the move's description and of the money-turning answer.
pub const MOVE_DESC: u16 = 0x1F8;
pub const MOVE_DESC_SLOTS: usize = 51;
pub const TRANSMUTE_DESC: u16 = 0x1B59;

/// The error of an item that a quest needs, or that is worth nothing, kept back from Transmute All
/// (2017: 111 such items, every one worth nothing as it was made; 14 of their 25 templates say they
/// are quest items, none of the 358 templates of items turned into money do).
pub const NOT_FOR_MONEY: u32 = 1105;

/// The template property that marks an item a quest needs.
pub const QUEST_ITEM: &str = "Item_IsQuestItem";

/// Native type of a list of 64-bit ids.
pub const LONG_LIST: u16 = 0x3B;

/// How close a player must be to a corpse to open it or take from it, in meters.
pub const CORPSE_REACH: f32 = 2.0;

/// How long taking an item takes (the taking behavior), in seconds.
pub const TAKING_SECONDS: f64 = 1.8;

/// The slots of a player's pack itself (its first segment).
pub const PACK_SLOTS: u32 = 42;

/// The segments of a player's pack: the pack itself and its three bags, each segment's places.
pub const SEGMENTS: [u32; 4] = [PACK_SLOTS, 12, 12, 12];

/// The places of a player's pack and its bags together. The pack itself numbers its items past its
/// own places from here on: the bags' numbers are fixed, and these are ours, never named to a client.
pub const PACK_ROOM: u32 = PACK_SLOTS + 36;

/// The first slot of each segment.
#[must_use]
pub fn segment_start(segment: usize) -> u32 {
    SEGMENTS[..segment.min(SEGMENTS.len())].iter().sum()
}

/// The segment an item's slot is in (a slot past the last bag is the pack itself).
#[must_use]
pub fn segment_of(slot: u32) -> usize {
    if slot >= PACK_ROOM {
        return 0;
    }
    (1..SEGMENTS.len())
        .rev()
        .find(|s| slot >= segment_start(*s))
        .unwrap_or(0)
}

/// The slot of the item at a place in its segment.
fn slot_at(segment: usize, place: u32) -> u32 {
    if segment == 0 && place >= PACK_SLOTS {
        PACK_ROOM + (place - PACK_SLOTS)
    } else {
        segment_start(segment) + place
    }
}

/// The order of a pack's items: each segment's in turn, in its slots' order.
#[must_use]
pub fn pack_order(slot: u32) -> (usize, u32) {
    (segment_of(slot), slot)
}

/// A slot to name to a client: one past the bags names no segment there, so the pack's front.
#[must_use]
pub fn named_slot(slot: u32) -> u32 {
    if slot >= PACK_ROOM {
        0
    } else {
        slot
    }
}

/// How many items each segment of a pack holds.
#[must_use]
pub fn segment_counts(held: &[Held]) -> [u32; 4] {
    let mut n = [0u32; 4];
    for h in held.iter().filter(|h| h.location == 0) {
        n[segment_of(h.slot)] += 1;
    }
    n
}

/// The segments of a pack as its session start and its contents name them: each segment's places
/// and how many of them hold an item.
#[must_use]
pub fn pack_segments(held: &[Held]) -> Vec<(u32, u32)> {
    let n = segment_counts(held);
    SEGMENTS.iter().zip(n).map(|(room, n)| (*room, n)).collect()
}

/// The slot after the last item of the first segment with room (the pack's front when every
/// segment is full).
#[must_use]
pub fn end_slot(held: &[Held]) -> u32 {
    let n = segment_counts(held);
    (0..SEGMENTS.len())
        .find(|s| n[*s] < SEGMENTS[*s])
        .map_or(0, |segment| segment_start(segment) + n[segment])
}

/// Make room at a slot of a pack: every item of the slot's segment at or after it moves up one.
pub fn insert_at(held: &mut [Held], slot: u32) {
    let segment = segment_of(slot);
    for h in held
        .iter_mut()
        .filter(|h| h.location == 0 && segment_of(h.slot) == segment && h.slot >= slot)
    {
        h.slot = if h.slot + 1 == PACK_SLOTS {
            PACK_ROOM
        } else {
            h.slot + 1
        };
    }
}

/// The book every new character is given as it first enters.
pub const GUIDE_BOOK: u32 = 0x4700_06CD;

/// A new character's starting pack by race, from the character data's starting inventory
/// (human: bow, sword, stave; Lugian: sword, hammer, scepter; Tumerok: drum, spear, sword). The
/// clothes it starts in are part of its look.
#[must_use]
pub fn starting_pack(race: u32) -> &'static [u32] {
    match race {
        1 => &[0x4700_051D, 0x4700_051C, 0x4700_051B],
        2 => &[0x4700_0515, 0x4700_0516, 0x4700_0517],
        4 => &[0x4700_0521, 0x4700_0522, 0x4700_0523],
        _ => &[],
    }
}

/// Kinds of moves.
pub mod move_type {
    /// Made in the pack by the server (a gift).
    pub const GIFT: u32 = 0;
    /// Taken from a container.
    pub const TAKE: u32 = 20;
    /// Bought from a storekeeper.
    pub const PURCHASE: u32 = 0x4000_0004;
}

/// The name a treasure item is given: its template's name, with an empty suffix.
pub mod treasure_name {
    pub const NAME: (u32, u32) = (0x7556_141B, 0x2500_035F);
    pub const PREFIX: u32 = 0xA14A_2800;
    pub const PREFIX_TEXT: (u32, u32) = (0xEF5F_89B9, 0x2500_035F);
    pub const PREFIX_NAME: u32 = 0x9B26_2000;
    pub const SUFFIX: u32 = 0xA13E_9D40;
    pub const ITEM: u32 = 0x9D06_6C50;
    /// The string of an effect's name table that names an item for it ("of Revival").
    pub const EFFECT_SUFFIX: u32 = 0x5A43_338D;
}

/// A move of an item: each field the server sets (the rest 0).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Move {
    pub move_type: u32,
    pub item: u64,
    pub split_item: u64,
    pub quantity: u32,
    pub from: u64,
    pub from_slot: i32,
    pub actual_from: u64,
    pub actual_from_slot: i32,
    pub target: u64,
    pub actual_target: u64,
    pub merge_container: u64,
    pub target_slot: i32,
    pub actual_target_slot: i32,
    pub merge_slot: i32,
    pub merge: bool,
    pub all_taken: bool,
    pub auto_merge: bool,
    pub contain: bool,
    pub grab: bool,
    pub unlock: bool,
    pub generated: bool,
    pub quiet: bool,
    pub hidden: bool,
    pub no_anim: bool,
    pub no_move: bool,
    /// The move may take a place past the pack's room.
    pub overflow: bool,
    /// The move took one: the pack already held all its room.
    pub used_overflow: bool,
    pub check_take: bool,
    pub played_anim: bool,
    pub template: u32,
    pub look_entity: u32,
    /// How many of the item are still to be contained.
    pub quantity_left: u32,
    /// Why the move was refused (0: it was not).
    pub status: u32,
    /// How many were split off a stack.
    pub split_quantity: u32,
    /// A trade's move.
    pub trade: bool,
    /// The server has made its checks (its answer to a reorganization says so).
    pub no_check: bool,
    /// Whether the item is bound to its owner does not matter (a give's answer says so).
    pub ignore_attunement: bool,
    /// The appearance the item shows in a hand handing it over.
    pub item_appearance: u32,
}

/// The move's description as the client's script object (its 51 slots).
#[must_use]
pub fn move_graph(m: &Move) -> Arg {
    let b = |v: bool| Slot::Int(u32::from(v));
    let i = |v: i32| Slot::Int(v.cast_unsigned());
    let mut s: Vec<Slot> = vec![Slot::Int(0); MOVE_DESC_SLOTS];
    let put_long = |s: &mut Vec<Slot>, at: usize, v: u64| {
        #[allow(clippy::cast_possible_truncation)]
        {
            s[at] = Slot::Tagged(tag::LONG, v as u32);
            s[at + 1] = Slot::Tagged(tag::HIGH, (v >> 32) as u32);
        }
    };
    s[0] = b(m.merge);
    s[1] = i(m.actual_from_slot);
    s[5] = b(m.ignore_attunement);
    s[6] = Slot::Int(m.item_appearance);
    s[7] = b(m.no_check);
    put_long(&mut s, 2, m.target);
    put_long(&mut s, 8, m.actual_target);
    s[10] = b(m.unlock);
    s[11] = i(m.target_slot);
    s[12] = Slot::Int(m.move_type);
    s[13] = b(m.check_take);
    s[14] = b(m.hidden);
    s[20] = b(m.trade);
    put_long(&mut s, 16, m.from);
    put_long(&mut s, 18, 0);
    s[22] = Slot::Int(m.template);
    s[23] = b(m.all_taken);
    s[24] = i(m.actual_target_slot);
    s[25] = Slot::Int(m.quantity_left);
    s[26] = Slot::Int(m.quantity);
    s[27] = b(m.generated);
    put_long(&mut s, 28, m.split_item);
    s[30] = Slot::Ref(None);
    s[31] = b(m.no_anim);
    put_long(&mut s, 32, m.merge_container);
    s[34] = b(m.used_overflow);
    s[35] = b(m.grab);
    put_long(&mut s, 36, m.actual_from);
    s[38] = Slot::Int(m.look_entity);
    s[39] = i(m.from_slot);
    s[40] = Slot::Int(m.status);
    s[41] = b(m.played_anim);
    s[42] = b(m.quiet);
    s[43] = b(m.overflow);
    put_long(&mut s, 44, m.item);
    s[46] = b(m.no_move);
    s[47] = b(m.contain);
    s[48] = Slot::Int(m.split_quantity);
    s[49] = b(m.auto_merge);
    s[50] = i(m.merge_slot);
    let mut g = Builder::new();
    g.object(MOVE_DESC, &s);
    Arg::Object(g.graph())
}

/// The answer to a move.
#[must_use]
pub fn move_done(m: &Move) -> ServerMessage {
    session::call(function::MOVE_DONE, &[move_graph(m)]).expect("a move packs")
}

/// A list of ids as a call value.
#[must_use]
pub fn id_list(ids: &[u64]) -> Arg {
    let mut b = Builder::new();
    b.native(
        LONG_LIST,
        list(Elem::Long, ids.iter().map(|i| Item::Long(*i)).collect()),
    );
    Arg::Object(b.graph())
}

/// A thing's object with its latest visual stamp.
pub(crate) fn stamp_of_thing(t: &Thing) -> ObjectStamp {
    stamp_of(t, t.visual)
}

fn stamp_of(t: &Thing, other: u16) -> ObjectStamp {
    ObjectStamp {
        id: t.id,
        instance_stamp: t.instance,
        other_stamp: other,
    }
}

/// The next visual stamp of a thing.
pub fn visual<S: Store>(s: &mut Server<S>, id: u64) -> ObjectStamp {
    match s.things.get_mut(id) {
        Some(t) => {
            t.visual = t.visual.wrapping_add(1);
            stamp_of(t, t.visual)
        }
        None => ObjectStamp {
            id,
            instance_stamp: 0,
            other_stamp: 0,
        },
    }
}

/// The containing stamp of an item: each containing moves its counter on.
pub(crate) fn contain_stamp<S: Store>(s: &mut Server<S>, id: u64) -> ObjectStamp {
    match s.things.get_mut(id) {
        Some(t) => {
            t.position_stamp = t.position_stamp.wrapping_add(1);
            stamp_of(t, t.position_stamp)
        }
        None => ObjectStamp {
            id,
            instance_stamp: 0,
            other_stamp: 0,
        },
    }
}

/// The variables of a string reference in the order the client's table keeps them (by bucket).
#[must_use]
pub fn ordered(mut vars: Vec<Variable>) -> Vec<Variable> {
    vars.sort_by_key(|v| v.id % auberean_script::native_object::VARIABLE_BUCKETS);
    vars
}

fn name_ref(name: (u32, u32)) -> StringInfo {
    StringInfo {
        string_id: name.0,
        table_id: name.1,
        ..StringInfo::default()
    }
}

fn nested(id: u32, s: StringInfo) -> Variable {
    Variable {
        id,
        kind: 9,
        value: VariableValue::Nested(Box::new(s)),
    }
}

/// A name's suffix: an effect's own item name, the template's name inside it; none, empty.
fn suffix_of(template_name: (u32, u32), effect_table: Option<u32>) -> StringInfo {
    use treasure_name as n;
    let Some(table) = effect_table else {
        return StringInfo::literal("");
    };
    let mut suffix = name_ref((n::EFFECT_SUFFIX, table));
    suffix.variables = vec![nested(n::PREFIX_NAME, name_ref(template_name))];
    suffix.variable_count = 1;
    suffix
}

/// A treasure item's name: its template's name, with the suffix of the effect it is named for.
#[must_use]
pub fn treasure_item_name(template_name: (u32, u32), effect_table: Option<u32>) -> StringInfo {
    treasure_word_name(template_name, effect_table, treasure_name::PREFIX_TEXT.0)
}

/// A treasure item's name with the word it drew before it.
#[must_use]
pub fn treasure_word_name(
    template_name: (u32, u32),
    effect_table: Option<u32>,
    word: u32,
) -> StringInfo {
    use treasure_name as n;
    let mut prefix = name_ref((word, n::PREFIX_TEXT.1));
    prefix.variables = vec![nested(n::PREFIX_NAME, name_ref(template_name))];
    prefix.variable_count = 1;
    let vars = ordered(vec![
        nested(n::PREFIX, prefix),
        nested(n::SUFFIX, suffix_of(template_name, effect_table)),
        nested(n::ITEM, name_ref(template_name)),
    ]);
    let mut s = name_ref(n::NAME);
    s.variable_count = 3;
    s.variables = vars;
    s
}

/// A made item's name: its template's name after the word its level gives it.
#[must_use]
pub fn made_item_name(template_name: (u32, u32), level_word: u32) -> StringInfo {
    use treasure_name as n;
    let mut prefix = name_ref((level_word, n::PREFIX_TEXT.1));
    prefix.variables = vec![nested(n::PREFIX_NAME, name_ref(template_name))];
    prefix.variable_count = 1;
    let vars = ordered(vec![
        nested(n::PREFIX, prefix),
        nested(n::SUFFIX, StringInfo::literal("")),
        nested(n::ITEM, name_ref(template_name)),
    ]);
    let mut s = name_ref(n::NAME);
    s.variable_count = 3;
    s.variables = vars;
    s
}

/// A treasure potion's name: its template's name with its effect's, "{the potion} of {the
/// effect}" (the effect's name table holds the suffix, the template's name inside it).
#[must_use]
pub fn potion_name(template_name: (u32, u32), effect_table: u32) -> StringInfo {
    jewelry_name(template_name, Some(effect_table))
}

/// A piece of treasure jewelry's name: no word before it, with the suffix of the effect it is named
/// for, if it has one (2017: 193 of 193, a suffix on the 73 with an effect).
#[must_use]
pub fn jewelry_name(template_name: (u32, u32), effect_table: Option<u32>) -> StringInfo {
    use treasure_name as n;
    let vars = ordered(vec![
        nested(n::PREFIX, StringInfo::literal("")),
        nested(n::SUFFIX, suffix_of(template_name, effect_table)),
        nested(n::ITEM, name_ref(template_name)),
    ]);
    let mut s = name_ref(n::NAME);
    s.variable_count = 3;
    s.variables = vars;
    s
}

/// The whole-object appearance a treasure potion's color is set in, and its key there.
pub const POTION_COLOR: (u32, u32) = (0x2000_011F, 2);

/// The bits of an item's description naming its durability now and at most.
const DURABILITY_NOW: u32 = 0x800_0000;
const DURABILITY_MOST: u32 = 0x1000_0000;

/// An item's object as a client is told of it: in its container with no place, or standing where
/// it was put down; named and valued when it is a treasure item (one with a value); a treasure
/// potion named for its effect and colored by it.
#[must_use]
pub fn item_create(t: &Thing) -> CreateObject {
    let d = &t.description;
    let mut weenie = crate::player::weenie_with_numbers(
        WeenieDesc {
            mask: WeenieDesc::PACKAGE | WeenieDesc::ENTITY | WeenieDesc::NUMBER_BITS[0],
            package_id: d.package,
            entity_did: t.template,
            ..WeenieDesc::default()
        },
        d.numbers,
    );
    weenie.numbers[0] = i32::try_from(t.quantity).unwrap_or(1);
    if let Some((owner, _)) = t.container {
        weenie.mask |= WeenieDesc::OBJECT_BITS[0];
        weenie.objects[0] = owner;
    }
    if let (Some(name), Some(e)) = (t.facts.name, t.own.effect) {
        weenie.mask |= WeenieDesc::NAME;
        weenie.name = potion_name(name, e.table);
    } else if let Some(name) = t.facts.name.filter(|_| t.value > 0) {
        weenie.mask |= WeenieDesc::NAME;
        // Nothing of its own but its value: no word (2017: 120 of 120 plain pieces of jewelry, and
        // the 39 weapons of a fixed template).
        weenie.name = match (t.own.made, t.own.suffix) {
            (None, suffix) => jewelry_name(name, suffix),
            (Some(m), suffix) if m.prefix != 0 => treasure_word_name(name, suffix, m.prefix),
            (Some(_), suffix) => treasure_item_name(name, suffix),
        };
    }
    if let Some(most) = t.own.made.and_then(|m| m.durability) {
        let number = |bit: u32| WeenieDesc::NUMBER_BITS.iter().position(|b| *b == bit);
        if let (Some(now_at), Some(most_at)) = (number(DURABILITY_NOW), number(DURABILITY_MOST)) {
            weenie.mask |= DURABILITY_NOW | DURABILITY_MOST;
            weenie.numbers[now_at] = i32::try_from(t.durability.unwrap_or(most)).unwrap_or(0);
            weenie.numbers[most_at] = i32::try_from(most).unwrap_or(0);
        }
    }
    if t.value > 0 {
        weenie.mask |= WeenieDesc::NUMBER_BITS[1];
        weenie.numbers[1] = i32::try_from(t.value).unwrap_or(0);
    }
    if let Some(sc) = d.scale {
        weenie.mask |= WeenieDesc::SCALE;
        weenie.scale = sc;
    }
    // In a pack it has no place; standing in the world (put down) it is made where it stands.
    let mut physics = crate::player::physics_at(
        t.at.map(|at| crate::player::placement(&at))
            .unwrap_or_default(),
        t.instance,
        t.visual,
        0,
    );
    // A bow is made strung: its own mode is the bow's.
    if t.strung {
        physics.mode = crate::skill_use::mode::BOW;
    }
    if t.at.is_some() {
        physics.stamps = [t.position_stamp, 0, 0, 0];
    } else {
        physics.bits &= !auberean_protocol::types::PhysicsDesc::POSITION;
        physics.stamps = [0; 4];
    }
    let mut visual = VisualDesc {
        bits: VisualDesc::PARENT,
        parent: d.look,
        ..VisualDesc::default()
    };
    if let Some(look) = t.own.look {
        crate::equip::put_on(
            &mut visual,
            look.table,
            auberean_protocol::table::Table::from_inserted(
                auberean_protocol::table::SMALL,
                look.keys(),
            ),
        );
    }
    if let Some((table, dye)) = t.own.dye {
        crate::equip::put_on(
            &mut visual,
            table,
            auberean_protocol::table::Table::from_inserted(
                auberean_protocol::table::SMALL,
                crate::equip::DYE_KEYS.map(|k| (k, dye)),
            ),
        );
    }
    if let Some(e) = t.own.effect {
        let (table, key) = POTION_COLOR;
        crate::equip::put_on(
            &mut visual,
            table,
            auberean_protocol::table::Table::from_inserted(
                auberean_protocol::table::SMALL,
                [(key, e.color)],
            ),
        );
    }
    CreateObject {
        id: t.id,
        visual,
        physics,
        weenie,
    }
}

/// An item a player holds as its client is told of it: in its pack, as [`item_create`] makes
/// it; in its hand, also with the player as its wielder and hung from the hand the weapon is held
/// in (the player's object of instance `player_instance`).
#[must_use]
pub fn held_create(
    catalog: &yalain_content::Catalog,
    t: &Thing,
    player_instance: u16,
) -> CreateObject {
    let mut o = item_create(t);
    if let Some((player, _)) = t.wielded {
        o.weenie.mask |= WeenieDesc::OBJECT_BITS[1];
        o.weenie.objects[1] = player;
    }
    // What is held in the hand hangs from it; what is worn on the body does not.
    if let Some((player, hand @ (crate::equip::PRIMARY_HAND | crate::equip::OFF_HAND))) = t.wielded
    {
        let hold = crate::equip::hold_of(catalog, t.template);
        o.physics.bits |= PhysicsDesc::PARENT;
        o.physics.parent = ParentLink {
            parent: player,
            location: if hand == hold.preferred {
                hold.primary
            } else {
                hold.secondary
            },
            stamp: player_instance,
        };
    }
    o
}

/// The templates whose script makes them attunable, with no saved property to say so.
const ATTUNABLE_BY_SCRIPT: [u32; 118] = [
    0x4700_0605,
    0x4700_0606,
    0x4700_0608,
    0x4700_0609,
    0x4700_060A,
    0x4700_060B,
    0x4700_060C,
    0x4700_0615,
    0x4700_0616,
    0x4700_0617,
    0x4700_0618,
    0x4700_0619,
    0x4700_061E,
    0x4700_0620,
    0x4700_0621,
    0x4700_0675,
    0x4700_080B,
    0x4700_0895,
    0x4700_0896,
    0x4700_0897,
    0x4700_0898,
    0x4700_08CF,
    0x4700_08D0,
    0x4700_08D1,
    0x4700_0AE9,
    0x4700_0AEA,
    0x4700_0AEB,
    0x4700_0AEC,
    0x4700_0AED,
    0x4700_0AEE,
    0x4700_11C7,
    0x4700_11C8,
    0x4700_11C9,
    0x4700_11CA,
    0x4700_11CB,
    0x4700_11CC,
    0x4700_11CD,
    0x4700_11CE,
    0x4700_11D1,
    0x4700_11D4,
    0x4700_11D5,
    0x4700_11D6,
    0x4700_11D7,
    0x4700_11D8,
    0x4700_11D9,
    0x4700_11DA,
    0x4700_11DB,
    0x4700_11DC,
    0x4700_132D,
    0x4700_1330,
    0x4700_1331,
    0x4700_1332,
    0x4700_1333,
    0x4700_1334,
    0x4700_1335,
    0x4700_1345,
    0x4700_1346,
    0x4700_13F5,
    0x4700_1469,
    0x4700_146A,
    0x4700_146B,
    0x4700_146C,
    0x4700_14CE,
    0x4700_14CF,
    0x4700_14D0,
    0x4700_14D1,
    0x4700_14D2,
    0x4700_14D3,
    0x4700_14D4,
    0x4700_14D5,
    0x4700_14D7,
    0x4700_14D8,
    0x4700_14DB,
    0x4700_14DC,
    0x4700_14DD,
    0x4700_14DE,
    0x4700_14DF,
    0x4700_14E0,
    0x4700_14E1,
    0x4700_14E2,
    0x4700_14E3,
    0x4700_16C0,
    0x4700_16C1,
    0x4700_16C2,
    0x4700_1863,
    0x4700_1864,
    0x4700_1865,
    0x4700_1866,
    0x4700_1868,
    0x4700_1869,
    0x4700_186A,
    0x4700_186B,
    0x4700_186C,
    0x4700_186E,
    0x4700_1870,
    0x4700_1871,
    0x4700_1872,
    0x4700_1873,
    0x4700_1874,
    0x4700_1875,
    0x4700_1876,
    0x4700_1877,
    0x4700_1878,
    0x4700_187B,
    0x4700_187C,
    0x4700_187D,
    0x4700_187E,
    0x4700_188C,
    0x4700_1A53,
    0x4700_1A54,
    0x4700_1A55,
    0x4700_1AF1,
    0x4700_1CE4,
    0x4700_1CE5,
    0x4700_1CE6,
    0x4700_2056,
    0x4700_2057,
    0x4700_2058,
];

/// Is an item of this template bound to whoever holds it, so that it may not be put down or traded
/// away (2017: of templates so marked, by their property or their script, 15 of 15 drops and 10 of
/// 10 trades refused; of others none, 587 drops and 590 trades done)?
#[must_use]
pub fn attunable(catalog: &yalain_content::Catalog, template: u32) -> bool {
    ATTUNABLE_BY_SCRIPT.contains(&template)
        || catalog
            .get(template)
            .is_some_and(|d| d.props.flag("Item_IsAttunable"))
}

/// What a player wears and wields, as its session start lists it: each wielded item in the
/// primary hand, the hands it takes, its implement kind when the server knows it; each item worn
/// on the body where it is, the places it takes, and the look it gives a body of this race and
/// sex.
#[must_use]
pub fn worn(
    catalog: &yalain_content::Catalog,
    items: &[Held],
    kept: &[crate::state::ItemState],
    race: u32,
    sex: u32,
) -> Vec<session::Worn> {
    let mut v: Vec<session::Worn> = items
        .iter()
        .filter(|h| h.location != 0)
        .map(|h| {
            let props = catalog.get(h.template).map(|t| &t.props);
            if h.location & crate::equip::BODY_PLACES != 0 {
                let fit = props.and_then(crate::equip::fit_of).unwrap_or_default();
                session::Worn {
                    item: h.id,
                    location: h.location,
                    taken: h.location | fit.precluded,
                    implement: 0,
                    look: props.and_then(|p| {
                        crate::equip::worn_look(p, race, sex, &crate::equip::own_kept(kept, h.id))
                    }),
                }
            } else {
                let off = h.location == crate::equip::OFF_HAND;
                session::Worn {
                    item: h.id,
                    location: if off {
                        crate::equip::OFF_HAND
                    } else {
                        crate::equip::PRIMARY_HAND
                    },
                    taken: if off {
                        crate::equip::OFF_HAND
                    } else {
                        crate::equip::PRIMARY_HAND
                            | crate::equip::hold_of(catalog, h.template).precluded
                    },
                    implement: crate::skill_use::implement_of(catalog, h.template).unwrap_or(0),
                    look: None,
                }
            }
        })
        .collect();
    v.sort_by_key(|w| w.location);
    v
}

/// Make an item of a template (nowhere yet). `None` when the template is not in the catalog.
pub fn make_item<S: Store>(
    s: &mut Server<S>,
    template: u32,
    quantity: u32,
    value: u32,
) -> Option<u64> {
    let t = s.catalog.get(template)?.clone();
    let id = s.things.new_id();
    let mut item = Thing::of(id, &t, Kind::Item);
    item.instance = 0;
    item.visual = 2;
    item.position_stamp = 0;
    item.quantity = quantity.max(1);
    item.value = value;
    s.things.insert(item);
    Some(id)
}

/// The icon an item is shown with: its template's, with what its dye or its potion's color does to
/// it (2017: 8,032 of 8,038 profiles' icons so, from their items' appearance values).
#[must_use]
pub fn icon_of<S: Store>(s: &Server<S>, t: &Thing) -> Vec<[u32; 3]> {
    let mut icon = s
        .catalog
        .icons
        .get(&t.template)
        .cloned()
        .unwrap_or_default();
    let mut apply = |table: u32, key: u32, value: f32| {
        if let Some(tables) = s.catalog.icon_tables.get(&table) {
            yalain_content::apply_icon_value(&mut icon, tables, key, value);
        }
    };
    if let Some(look) = t.own.look {
        for (key, value) in look.keys() {
            apply(look.table, key, value);
        }
    }
    if let Some((table, dye)) = t.own.dye {
        for key in crate::equip::DYE_KEYS {
            apply(table, key, dye);
        }
    }
    if let Some(e) = t.own.effect {
        apply(POTION_COLOR.0, POTION_COLOR.1, e.color);
    }
    icon
}

/// What one piece of an item is worth: its own value, else its template's.
fn piece_worth<S: Store>(s: &Server<S>, t: &Thing) -> u32 {
    if t.value > 0 {
        t.value
    } else {
        item_value(s, t.template)
    }
}

/// May two items share a stack? A stack holds one set of numbers, so they must be of one template,
/// each piece worth the same and carrying the same of its own. The name of who made one is not
/// among them: the kept stack keeps its maker.
pub(crate) fn same_numbers<S: Store>(s: &Server<S>, a: u64, b: u64) -> bool {
    let (Some(x), Some(y)) = (s.things.get(a), s.things.get(b)) else {
        return false;
    };
    x.template == y.template
        && piece_worth(s, x) == piece_worth(s, y)
        && x.own == y.own
        && x.enchantments == y.enchantments
        && x.bonuses == y.bonuses
        && x.inscription == y.inscription
        && x.durability == y.durability
        && x.destination == y.destination
        && x.binds_on_use == y.binds_on_use
}

/// Does an item carry nothing beyond its template, so that new pieces of the template may join it?
fn is_plain<S: Store>(s: &Server<S>, t: &Thing) -> bool {
    piece_worth(s, t) == item_value(s, t.template)
        && t.own.is_empty()
        && t.enchantments.is_empty()
        && t.bonuses.is_empty()
        && t.inscription.is_none()
        && t.durability.is_none()
        && t.destination.is_none()
        && !t.binds_on_use
}

/// So many pieces split from a stack make a new item (nowhere yet), holding all the stack holds of
/// its own. The stack's count is the caller's to lower.
pub(crate) fn split_off<S: Store>(s: &mut Server<S>, stack: u64, quantity: u32) -> Option<u64> {
    let (template, kept) = s
        .things
        .get(stack)
        .map(|t| (t.template, crate::state::ItemState::of(t)))?;
    let id = make_item(s, template, quantity, 0)?;
    if let Some(t) = s.things.get_mut(id) {
        kept.restore(t);
    }
    Some(id)
}

/// Number each segment's items from its first slot in their order, as the client keeps its pack: a
/// list that closes up when an item leaves it.
pub fn close_up(items: &mut [Held]) {
    let mut order: Vec<usize> = (0..items.len())
        .filter(|i| items[*i].location == 0)
        .collect();
    order.sort_by_key(|i| pack_order(items[*i].slot));
    let mut next = [0u32; 4];
    for i in order {
        let segment = segment_of(items[i].slot);
        items[i].slot = slot_at(segment, next[segment]);
        next[segment] += 1;
    }
}

/// Whether a player's pack and its bags hold an item in every place. What a player makes (a craft's
/// product, a cut, what it mines) waits for a place then; what it is given or takes does not.
pub(crate) fn pack_full(held: &[Held]) -> bool {
    !fits_in_pack(held, 1)
}

/// Whether a player's pack and its bags have room for this many more items.
#[must_use]
pub fn fits_in_pack(held: &[Held], incoming: usize) -> bool {
    let in_pack = held.iter().filter(|h| h.location == 0).count();
    in_pack.saturating_add(incoming) <= PACK_ROOM as usize
}

/// Make room for an item coming into a pack: at the front of the first segment with room, its
/// items each moving up one slot, as the client puts it at the slot the move names; at the pack's
/// front when every segment is full, a gift held all the same. Returns the slot.
pub(crate) fn room_for_one(held: &mut [Held]) -> u32 {
    let n = segment_counts(held);
    let segment = (0..SEGMENTS.len())
        .find(|s| n[*s] < SEGMENTS[*s])
        .unwrap_or(0);
    let slot = segment_start(segment);
    insert_at(held, slot);
    slot
}

/// Put an item into a player's pack (kept with the player). Returns its slot.
fn hold<S: Store>(s: &mut Server<S>, world: ConnId, item: u64) -> Option<u32> {
    let p = s.players.get_mut(&world)?;
    let slot = room_for_one(&mut p.play.items);
    let owner = p.character;
    let t = s.things.get_mut(item)?;
    t.container = Some((owner, slot));
    t.at = None;
    p.play.items.push(Held {
        id: item,
        template: t.template,
        slot,
        location: 0,
        quantity: t.quantity,
    });
    Some(slot)
}

pub(crate) fn knows<S: Store>(s: &mut Server<S>, world: ConnId, id: u64) {
    if let Some(p) = s.players.get_mut(&world) {
        p.known.insert(
            id,
            crate::presence::Seen {
                position_stream: 0,
                head_stream: 0,
            },
        );
    }
}

/// Give a player items of a template, made in its pack as a gift of the server. Returns the item
/// made.
pub fn give<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    template: u32,
    quantity: u32,
) -> Option<u64> {
    let Some(id) = make_item(s, template, quantity, 0) else {
        s.unhandled.note(format!(
            "a gift of template {template:08X}, not in the catalog"
        ));
        return None;
    };
    arrive(
        s,
        net,
        world,
        id,
        Move {
            move_type: move_type::GIFT,
            generated: true,
            from_slot: -1,
            actual_from_slot: -1,
            merge: true,
            merge_slot: -1,
            auto_merge: true,
            grab: true,
            unlock: true,
            ..Move::default()
        },
    );
    Some(id)
}

/// The move type of an item made in a player's pack from an object of the world (a dye plant's
/// harvest).
pub const MADE_FROM: u32 = 36;

/// Give a player items of a template made from an object of the world (a harvested dye plant),
/// each a new stack in its pack. Returns the item made.
pub fn give_from<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    template: u32,
    quantity: u32,
    from: u64,
) -> Option<u64> {
    let Some(id) = make_item(s, template, quantity, 0) else {
        s.unhandled.note(format!(
            "an item of template {template:08X} made from an object, not in the catalog"
        ));
        return None;
    };
    arrive(
        s,
        net,
        world,
        id,
        Move {
            move_type: MADE_FROM,
            from,
            generated: true,
            from_slot: -1,
            actual_from_slot: -1,
            merge: true,
            merge_slot: -1,
            auto_merge: true,
            all_taken: true,
            contain: true,
            no_move: true,
            grab: true,
            unlock: true,
            overflow: true,
            ..Move::default()
        },
    );
    Some(id)
}

/// Give a player items it has made: given as the server gives, and its maker's name kept with it
/// (its profile names who made it). Returns the item made.
pub fn give_made<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    template: u32,
    quantity: u32,
) -> Option<u64> {
    let maker = s
        .players
        .get(&world)
        .map(|p| crate::player::world_name(&p.name, p.sex));
    let id = give(s, net, world, template, quantity)?;
    if let Some(t) = s.things.get_mut(id) {
        t.crafter = maker;
    }
    Some(id)
}

/// Add so many units of a template to a player's plain stacks of it, each filled to the limit its
/// template sets (its new quantity shown), and make stacks of the rest in the pack, none past that
/// limit (2017: 0 of 1,419 stacks seen above it, 25 at it). Which stack fills first is our choice.
pub fn give_stacked<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    template: u32,
    n: u32,
) {
    let most = s
        .catalog
        .get(template)
        .and_then(|t| t.props.int("Item_MaxStackSize"))
        .and_then(|m| u32::try_from(m).ok())
        .filter(|m| *m > 0)
        .unwrap_or(u32::MAX);
    let mut left = n;
    while left > 0 {
        let plain = s.players.get(&world).and_then(|p| {
            p.play
                .items
                .iter()
                .find(|h| {
                    h.template == template
                        && h.location == 0
                        && h.quantity.max(1) < most
                        && s.things.get(h.id).is_some_and(|t| is_plain(s, t))
                })
                .map(|h| h.id)
        });
        let stack = plain.and_then(|id| {
            let p = s.players.get_mut(&world)?;
            let h = p.play.items.iter_mut().find(|h| h.id == id)?;
            let had = h.quantity.max(1);
            let taken = left.min(most - had);
            h.quantity = had + taken;
            left -= taken;
            Some((id, h.quantity))
        });
        let Some((id, quantity)) = stack else {
            break;
        };
        if let Some(t) = s.things.get_mut(id) {
            t.quantity = quantity;
        }
        let m = ServerMessage::UpdateIntVisual(VisualQuality {
            object: visual(s, id),
            quality: crate::craft::QUANTITY,
            value: i32::try_from(quantity).unwrap_or(i32::MAX),
        });
        progress::private(net, world, &m);
    }
    while left > 0 {
        let part = left.min(most);
        give(s, net, world, template, part);
        left -= part;
    }
    crate::world::keep_player(s, world);
}

/// How near an item lying in the world the player picks it up, in meters.
pub const PICK_UP_REACH: f32 = 2.0;

/// The picking-up behavior's effect script.
pub const PICK_UP_SCRIPT: u32 = 0x5700_0008;

/// The move type of an item picked up from the world.
pub const PICKED_UP: u32 = 12;

/// How long the picking-up behavior takes, by race and sex, in seconds (2017: the medians of 304
/// pickups, from the behavior to the item contained).
#[must_use]
pub fn picking_up_seconds(race: u32, sex: u32) -> f64 {
    let female = sex == crate::chargen::FEMALE;
    match (race, female) {
        (1, false) => 1.79,
        (1, true) | (16, false) => 1.81,
        (4, false) => 1.90,
        (4, true) => 1.88,
        _ => 1.50,
    }
}

/// The behavior a player plays with an item's look in its hand (picking it up, putting it down),
/// seen by every client that sees the player.
pub(crate) fn hand_behavior<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    (behavior, fx_script): (u32, u32),
    item: u64,
    context: u32,
) {
    let Some(look) = s.things.get(item).map(|t| t.description.look) else {
        return;
    };
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
        behavior,
        fx_script,
        target: item,
        clone_visual: look,
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
    let id = p.character;
    crate::combat::to_all_seeing(s, net, id, &behave);
}

/// The player at an item lying in the world picks it up: it plays its picking-up behavior with the
/// item's look in its hand (in the walk's context); the rest when the behavior is through.
pub fn pick_up<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    context: u32,
    clock: Clock,
) {
    let Some((race, sex)) = s.players.get(&world).map(|p| (p.race, p.sex)) else {
        return;
    };
    hand_behavior(s, net, world, (look::TAKING, PICK_UP_SCRIPT), item, context);
    s.after(
        clock,
        picking_up_seconds(race, sex),
        Timer::PickUp { world, item },
    );
}

/// Why a pickup was refused, as its answer says.
pub mod refused {
    /// The player could not get to the item.
    pub const CANT_GET_THERE: u32 = 57;
    /// The pack has no room for it.
    pub const CONTAINER_FULL: u32 = 1085;
}

/// The stack in a player's pack an item of `quantity` would join, with its slot and its quantity:
/// of its template and its numbers, a template that stacks (one of more than one to a stack), with
/// room for them all.
pub(crate) fn stack_for<S: Store>(
    s: &Server<S>,
    world: ConnId,
    item: u64,
    quantity: u32,
) -> Option<(u64, u32, u32)> {
    let template = s.things.get(item)?.template;
    let most = s
        .catalog
        .get(template)
        .and_then(|t| t.props.int("Item_MaxStackSize"))
        .unwrap_or(0);
    if most <= 1 {
        return None;
    }
    s.players
        .get(&world)?
        .play
        .items
        .iter()
        .find(|h| {
            h.template == template
                && h.location == 0
                && same_numbers(s, h.id, item)
                && i64::from(h.quantity.max(1)) + i64::from(quantity) <= most
        })
        .map(|h| (h.id, h.slot, h.quantity.max(1)))
}

/// A pickup's answer: the move of the item lying in the world into the player's pack.
fn pickup_move(item: u64, player: u64, quantity: u32, template: u32, look: u32) -> Move {
    Move {
        move_type: PICKED_UP,
        item,
        split_item: item,
        quantity,
        target: player,
        actual_target: player,
        merge_container: player,
        all_taken: true,
        auto_merge: true,
        contain: true,
        grab: true,
        unlock: true,
        played_anim: true,
        template,
        look_entity: look,
        split_quantity: quantity,
        ..Move::default()
    }
}

/// A pickup refused: the move answered with why, the player stopped first when it could not get
/// there (no animation played), or the item put back down when the pack has no room.
pub fn pickup_refused<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    why: u32,
) {
    let Some(player) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some((quantity, template, look)) = s
        .things
        .get(item)
        .map(|t| (t.quantity.max(1), t.template, t.description.look))
    else {
        return;
    };
    let slot = if why == refused::CONTAINER_FULL {
        i32::try_from(PACK_ROOM).unwrap_or(0)
    } else {
        0
    };
    let m = Move {
        merge: true,
        merge_slot: slot,
        target_slot: slot,
        actual_target_slot: slot,
        quantity_left: quantity,
        played_anim: why == refused::CONTAINER_FULL,
        status: why,
        ..pickup_move(item, player, quantity, template, look)
    };
    progress::private(net, world, &move_done(&m));
    if why == refused::CONTAINER_FULL {
        s.next_context = s.next_context.wrapping_add(1);
        let context = s.next_context;
        hand_behavior(
            s,
            net,
            world,
            (
                crate::drops::look::PUTTING_DOWN,
                crate::drops::look::PUT_DOWN_SCRIPT,
            ),
            item,
            context,
        );
    }
}

/// The picking-up behavior is through: the item is contained in the pack, the move answered, its
/// holder told everyone who knows it, and it leaves the world. An item of a template the player
/// holds a stack of joins the stack instead (the move answered, the stack's quantity, the item
/// destroyed and out of the world); one the pack has no room for is put back down.
pub fn picked_up<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    clock: Clock,
) {
    let lying = s
        .things
        .get(item)
        .is_some_and(|t| t.container.is_none() && t.at.is_some());
    let Some(player) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    if !lying {
        return;
    }
    let Some((instance, stamp, quantity, template, look)) = s.things.get(item).map(|t| {
        (
            t.instance,
            t.position_stamp,
            t.quantity.max(1),
            t.template,
            t.description.look,
        )
    }) else {
        return;
    };
    let leave = ServerMessage::ObjectLeaves(ObjectEvent {
        sender: ObjectStamp {
            id: item,
            instance_stamp: instance,
            other_stamp: 0,
        },
        action: auberean_protocol::physics::Leave {
            stamp: stamp.wrapping_add(1),
        },
    });
    if let Some((stack, slot, had)) = stack_for(s, world, item, quantity) {
        let slot = i32::try_from(slot).unwrap_or(0);
        let m = Move {
            split_item: 0,
            merge: true,
            merge_slot: -1,
            target_slot: slot,
            actual_target_slot: slot,
            ..pickup_move(item, player, quantity, template, look)
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
        let more = ServerMessage::UpdateIntVisual(VisualQuality {
            object: visual(s, stack),
            quality: crate::craft::QUANTITY,
            value: i32::try_from(total).unwrap_or(i32::MAX),
        });
        progress::private(net, world, &more);
        let gone = ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
            id: item,
            instance_stamp: instance,
            other_stamp: 0,
        }));
        crate::presence::to_thing_knowers(s, net, item, BlobKind::UNORDERED, &gone);
        to_knowers(s, net, item, &leave);
        s.things.remove(item);
        for p in s.players.values_mut() {
            p.known.remove(&item);
        }
    } else {
        let full = s
            .players
            .get(&world)
            .is_some_and(|p| pack_full(&p.play.items));
        if full {
            pickup_refused(s, net, world, item, refused::CONTAINER_FULL);
            return;
        }
        let Some(slot) = hold(s, world, item) else {
            return;
        };
        let contained = contain_stamp(s, item);
        net.send(
            world,
            queue::EVENT,
            BlobKind::UNORDERED,
            ServerMessage::Contain(ObjectOnly(contained)).encode(),
        );
        let m = Move {
            actual_target_slot: i32::try_from(slot).unwrap_or(0),
            ..pickup_move(item, player, quantity, template, look)
        };
        progress::private(net, world, &move_done(&m));
        let held = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
            object: visual(s, item),
            quality: q::CONTAINER,
            value: player,
        });
        to_knowers(s, net, item, &held);
        to_knowers(s, net, item, &leave);
    }
    crate::quests::took(s, net, world, template, clock);
    crate::world::keep_player(s, world);
}

/// A made item comes into a player's pack: contained, made for the client, the move answered.
fn arrive<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, id: u64, mut m: Move) {
    let Some(player) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    // Past the pack's room the move says so (2017: 7 of 7, at 78 items or more).
    let past_room = s
        .players
        .get(&world)
        .is_some_and(|p| pack_full(&p.play.items));
    m.overflow |= past_room;
    m.used_overflow = past_room;
    let Some(slot) = hold(s, world, id) else {
        return;
    };
    let stamp = contain_stamp(s, id);
    net.send(
        world,
        queue::EVENT,
        BlobKind::UNORDERED,
        ServerMessage::Contain(ObjectOnly(stamp)).encode(),
    );
    if let Some(t) = s.things.get(id) {
        m.item = id;
        m.split_item = id;
        m.quantity = t.quantity.max(1);
        m.target = player;
        m.actual_target = player;
        m.target_slot = i32::try_from(slot).unwrap_or(0);
        m.actual_target_slot = i32::try_from(slot).unwrap_or(0);
        m.template = t.template;
        // The answer first: its client has no such item yet, and calls it "the item".
        progress::private(net, world, &move_done(&m));
        let create = ServerMessage::CreateObject(item_create(t));
        net.send(world, queue::EVENT, BlobKind::UNORDERED, create.encode());
    }
    knows(s, world, id);
    crate::world::keep_player(s, world);
}

/// Items a player had a broker keep come back to it: made again in its pack, from the broker.
pub fn collected<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    template: u32,
    quantity: u32,
    from: u64,
    kept: Option<&crate::state::ItemState>,
) {
    let Some(id) = make_item(s, template, quantity, 0) else {
        return;
    };
    if let (Some(kept), Some(t)) = (kept, s.things.get_mut(id)) {
        kept.restore(t);
    }
    arrive(
        s,
        net,
        world,
        id,
        Move {
            move_type: crate::market::MOVE_OUT,
            from,
            actual_from: from,
            from_slot: -1,
            actual_from_slot: -1,
            merge_slot: -1,
            generated: true,
            grab: true,
            contain: true,
            unlock: true,
            no_anim: true,
            ..Move::default()
        },
    );
}

/// A player has bought items of a template: they are made in its pack.
pub fn bought<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    template: u32,
    quantity: u32,
    from: u64,
    _clock: Clock,
) {
    let Some(id) = make_item(s, template, quantity, 0) else {
        s.unhandled.note(format!(
            "a purchase of template {template:08X}, not in the catalog"
        ));
        return;
    };
    arrive(
        s,
        net,
        world,
        id,
        Move {
            move_type: move_type::PURCHASE,
            from,
            from_slot: -1,
            actual_from_slot: -1,
            merge: true,
            merge_slot: -1,
            generated: true,
            grab: true,
            contain: true,
            auto_merge: true,
            quiet: true,
            unlock: true,
            all_taken: true,
            overflow: true,
            ..Move::default()
        },
    );
}

/// Send a visual event about a thing to every client that knows it.
pub(crate) fn to_knowers<S: Store>(s: &Server<S>, net: &mut ServerNet, id: u64, m: &ServerMessage) {
    crate::presence::to_thing_knowers(s, net, id, BlobKind::VISUAL, m);
}

/// The corpse a player has open.
#[must_use]
pub fn open_corpse_of<S: Store>(s: &Server<S>, world: ConnId) -> Option<u64> {
    s.players.get(&world).and_then(|p| p.open_container)
}

/// Has another player than this one a container open?
#[must_use]
pub fn viewed_by_another<S: Store>(s: &Server<S>, world: ConnId, container: u64) -> bool {
    s.players
        .iter()
        .any(|(w, p)| *w != world && p.open_container == Some(container))
}

fn open_quality<S: Store>(s: &mut Server<S>, id: u64, open: bool) -> ServerMessage {
    ServerMessage::UpdateBoolVisual(VisualQuality {
        object: visual(s, id),
        quality: q::OPEN,
        value: i32::from(open),
    })
}

/// A player opens a corpse (in reach).
pub fn open<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, corpse: u64) {
    if let Some(old) = open_corpse_of(s, world).filter(|o| *o != corpse) {
        close(s, net, world, old);
    }
    let fx = ServerMessage::DoFx(ObjectEvent {
        sender: visual(s, corpse),
        action: auberean_protocol::physics::Fx {
            fx: look::OPENING_FX,
            scalar: 1.0,
        },
    });
    to_knowers(s, net, corpse, &fx);
    let mode = ServerMessage::DoMode(ObjectEvent {
        sender: visual(s, corpse),
        action: Word(look::OPEN_MODE),
    });
    to_knowers(s, net, corpse, &mode);
    let contents = s
        .things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .map(|c| c.contents.clone())
        .unwrap_or_default();
    let done = session::call(
        function::OPEN_DONE,
        &[
            Arg::Int(0),
            id_list(&[]),
            id_list(&contents),
            Arg::LongInt(corpse),
        ],
    )
    .expect("an open packs");
    progress::private(net, world, &done);
    let open = open_quality(s, corpse, true);
    to_knowers(s, net, corpse, &open);
    let stop = ServerMessage::StopFx(ObjectEvent {
        sender: visual(s, corpse),
        action: Word(look::CLOSED_MODE),
    });
    to_knowers(s, net, corpse, &stop);
    for id in contents {
        let Some(t) = s.things.get(id) else { continue };
        let known = s
            .players
            .get(&world)
            .is_some_and(|p| p.known.contains_key(&id));
        if !known {
            let m = ServerMessage::CreateObject(item_create(t));
            net.send(world, queue::EVENT, BlobKind::UNORDERED, m.encode());
            knows(s, world, id);
        }
    }
    if let Some(p) = s.players.get_mut(&world) {
        p.open_container = Some(corpse);
    }
}

/// A player closes a corpse.
pub fn close<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, corpse: u64) {
    let mode = ServerMessage::DoMode(ObjectEvent {
        sender: visual(s, corpse),
        action: Word(look::CLOSED_MODE),
    });
    to_knowers(s, net, corpse, &mode);
    let done =
        session::call(function::CLOSE_DONE, &[Arg::Int(0), Arg::LongInt(corpse)]).expect("packs");
    progress::private(net, world, &done);
    let closed = open_quality(s, corpse, false);
    to_knowers(s, net, corpse, &closed);
    if let Some(p) = s.players.get_mut(&world) {
        if p.open_container == Some(corpse) {
            p.open_container = None;
        }
    }
    crate::death::closed(s, corpse);
}

/// A player has moved: the corpse it has open closes once it is farther than an open chest's
/// opener may go from it.
pub fn moved<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(corpse) = open_corpse_of(s, world) else {
        return;
    };
    let Some(here) = s.players.get(&world).map(|p| p.position) else {
        return;
    };
    // A chest open is the chest's own to close.
    let Some(thing) = s.things.get(corpse).filter(|t| t.corpse.is_some()) else {
        return;
    };
    let far = thing
        .at
        .and_then(|at| crate::places::reach(&here, at.cell, at.origin))
        .is_none_or(|d| d > crate::usable::chests::WALK_AWAY);
    if far {
        close(s, net, world, corpse);
    }
}

/// The client asks to close a container.
pub fn close_request<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[crate::calls::Value],
) {
    let id = v.iter().find_map(|x| match x {
        crate::calls::Value::Long(l) => Some(*l),
        _ => None,
    });
    match id {
        Some(id) if open_corpse_of(s, world) == Some(id) => close(s, net, world, id),
        _ => s
            .unhandled
            .note("closing a container the player does not have open"),
    }
}

/// A player takes an item out of the corpse it has open: it plays its taking behavior and the item
/// leaves the corpse; the rest when the behavior is through.
pub fn take<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    clock: Clock,
) {
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
            | BehaviorParams::CONTEXT,
        behavior: look::TAKING,
        fx_script: look::TAKE_ONE_SCRIPT,
        target: item,
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
    let id = p.character;
    crate::combat::to_all_seeing(s, net, id, &behave);
    let left = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
        object: visual(s, item),
        quality: q::CONTAINER,
        value: 0,
    });
    to_knowers(s, net, item, &left);
    s.after(clock, TAKING_SECONDS, Timer::Take { world, item });
}

/// The taking behavior is through: the item is the player's.
pub fn taken<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    clock: Clock,
) {
    let Some(player) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some((corpse, from_slot)) = s.things.get(item).and_then(|t| t.container) else {
        return;
    };
    // Someone else may have got it meanwhile (the corpse closed and opened by another): one item,
    // one holder. It leaves the corpse only for a pack with room.
    let there = s
        .things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .map(|c| c.contents.clone())
        .or_else(|| crate::usable::chests::contents(s, corpse))
        .is_some_and(|c| c.contains(&item));
    let quantity = s.things.get(item).map_or(1, |t| t.quantity.max(1));
    let stack = stack_for(s, world, item, quantity).filter(|_| there);
    let full = stack.is_none()
        && s.players
            .get(&world)
            .is_some_and(|p| pack_full(&p.play.items));
    if !there || full {
        let m = Move {
            move_type: move_type::TAKE,
            item,
            split_item: item,
            quantity: 1,
            from: corpse,
            actual_from: corpse,
            target: player,
            actual_target: player,
            status: if there {
                crate::pack::status::CONTAINER_FULL
            } else {
                refused::CANT_GET_THERE
            },
            ..Move::default()
        };
        progress::private(net, world, &move_done(&m));
        return;
    }
    if let Some(c) = s.things.get_mut(corpse).and_then(|t| t.corpse.as_mut()) {
        c.contents.retain(|x| *x != item);
    }
    if let Some((stack, _, had)) = stack {
        joined_stack(s, net, world, item, corpse, from_slot, (stack, had), clock);
        return;
    }
    let Some(slot) = hold(s, world, item) else {
        return;
    };
    let stamp = contain_stamp(s, item);
    net.send(
        world,
        queue::EVENT,
        BlobKind::UNORDERED,
        ServerMessage::Contain(ObjectOnly(stamp)).encode(),
    );
    let Some(t) = s.things.get(item) else { return };
    let template = t.template;
    let m = Move {
        move_type: move_type::TAKE,
        item,
        split_item: item,
        quantity: 1,
        from: corpse,
        actual_from: corpse,
        actual_from_slot: i32::try_from(from_slot).unwrap_or(0),
        target: player,
        actual_target: player,
        merge_container: player,
        target_slot: i32::try_from(slot).unwrap_or(0),
        actual_target_slot: i32::try_from(slot).unwrap_or(0),
        all_taken: true,
        auto_merge: true,
        contain: true,
        grab: true,
        unlock: true,
        played_anim: true,
        template,
        ..Move::default()
    };
    progress::private(net, world, &move_done(&m));
    let held = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
        object: visual(s, item),
        quality: q::CONTAINER,
        value: player,
    });
    to_knowers(s, net, item, &held);
    crate::quests::took(s, net, world, template, clock);
    crate::world::keep_player(s, world);
}

/// A taken item joins a stack in the pack, which needs no place of its own: the move answered as a
/// merge, the stack's new quantity shown and the item gone from all who knew it (2017: 162 of 773
/// takes).
#[allow(clippy::too_many_arguments)]
fn joined_stack<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    from: u64,
    from_slot: u32,
    (stack, had): (u64, u32),
    clock: Clock,
) {
    let Some(player) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let Some((instance, quantity, template, look)) = s.things.get(item).map(|t| {
        (
            t.instance,
            t.quantity.max(1),
            t.template,
            t.description.look,
        )
    }) else {
        return;
    };
    let m = Move {
        move_type: move_type::TAKE,
        item,
        quantity,
        from,
        actual_from: from,
        actual_from_slot: i32::try_from(from_slot).unwrap_or(0),
        target: player,
        actual_target: player,
        merge_container: player,
        merge: true,
        merge_slot: -1,
        split_quantity: quantity,
        all_taken: true,
        auto_merge: true,
        contain: true,
        grab: true,
        unlock: true,
        played_anim: true,
        template,
        look_entity: look,
        ..Move::default()
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
    let more = ServerMessage::UpdateIntVisual(VisualQuality {
        object: visual(s, stack),
        quality: crate::craft::QUANTITY,
        value: i32::try_from(total).unwrap_or(i32::MAX),
    });
    progress::private(net, world, &more);
    let gone = ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
        id: item,
        instance_stamp: instance,
        other_stamp: 0,
    }));
    crate::presence::to_thing_knowers(s, net, item, BlobKind::UNORDERED, &gone);
    s.things.remove(item);
    for p in s.players.values_mut() {
        p.known.remove(&item);
    }
    crate::quests::took(s, net, world, template, clock);
    crate::world::keep_player(s, world);
}

/// A player turns everything in a corpse into money.
pub fn transmute_all<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[crate::calls::Value],
) {
    // The request's description: its container is its slots 8 and 9.
    let corpse = v.iter().find_map(|x| match x.root() {
        Some(crate::calls::Node::Script { slots, .. }) if slots.len() >= 10 => {
            Some(u64::from(slots[9].1) << 32 | u64::from(slots[8].1))
        }
        _ => None,
    });
    let Some(corpse) = corpse.filter(|c| s.things.get(*c).is_some_and(|t| t.corpse.is_some()))
    else {
        s.unhandled
            .note("turning into money a container the world does not have");
        return;
    };
    let Some(player) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    // Its own viewer's alone, as taking everything is.
    if open_corpse_of(s, world) != Some(corpse) {
        s.unhandled
            .note("turning into money a corpse the player does not have open");
        return;
    }
    let contents = s
        .things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .map(|c| c.contents.clone())
        .unwrap_or_default();
    let mut earned = 0u32;
    let mut done = Vec::new();
    let mut refused = Vec::new();
    // Each item passes through the pack, which holds it no longer than this answer: it needs no
    // room, and into a full pack its move says it ran past (2017: 6 of 6).
    let past_room = s
        .players
        .get(&world)
        .is_some_and(|p| pack_full(&p.play.items));
    for (slot, item) in contents.iter().enumerate() {
        // A quest item, or one worth nothing, is not turned into money: it stays in the corpse.
        let kept = s.things.get(*item).is_none_or(|t| {
            let worth = if t.value > 0 {
                t.value
            } else {
                item_value(s, t.template)
            };
            worth == 0 || is_quest_item(s, t.template)
        });
        if kept {
            refused.push(*item);
            continue;
        }
        done.push(*item);
        let stamp = contain_stamp(s, *item);
        net.send(
            world,
            queue::EVENT,
            BlobKind::UNORDERED,
            ServerMessage::Contain(ObjectOnly(stamp)).encode(),
        );
        let Some(t) = s.things.get(*item) else {
            continue;
        };
        let count = t.quantity.max(1);
        let worth = if t.value > 0 {
            t.value
        } else {
            item_value(s, t.template)
        }
        .saturating_mul(count);
        let m = Move {
            move_type: move_type::TAKE,
            item: *item,
            split_item: *item,
            quantity: count,
            from: corpse,
            actual_from: corpse,
            actual_from_slot: i32::try_from(slot).unwrap_or(0),
            target: player,
            actual_target: player,
            merge_container: player,
            all_taken: true,
            auto_merge: true,
            contain: true,
            grab: true,
            unlock: true,
            check_take: true,
            hidden: true,
            no_anim: true,
            no_move: true,
            quiet: true,
            overflow: true,
            used_overflow: past_room,
            template: t.template,
            ..Move::default()
        };
        progress::private(net, world, &move_done(&m));
        earned = earned.saturating_add(worth);
        let Some(p) = s.players.get_mut(&world) else {
            return;
        };
        let money = crate::store::money(&p.qualities).saturating_add(worth);
        crate::quals::set_int(&mut p.qualities, crate::store::MONEY, money.cast_signed());
        progress::send_int(net, world, crate::store::MONEY, money.cast_signed());
        let gone =
            session::call(function::DISCRETE_DESTROY, &[Arg::LongInt(*item)]).expect("packs");
        progress::private(net, world, &gone);
    }
    if let Some(c) = s.things.get_mut(corpse).and_then(|t| t.corpse.as_mut()) {
        c.contents.retain(|x| refused.contains(x));
    }
    // An item kept back is told by the error, and by the answer's status too when nothing was
    // turned into money.
    let error = if refused.is_empty() { 0 } else { NOT_FOR_MONEY };
    let status = if done.is_empty() { error } else { 0 };
    let mut b = Builder::new();
    let root = b.reserve(TRANSMUTE_DESC);
    let done_list = b.native(
        LONG_LIST,
        list(Elem::Long, done.iter().map(|i| Item::Long(*i)).collect()),
    );
    let refused_list = b.native(
        LONG_LIST,
        list(Elem::Long, refused.iter().map(|i| Item::Long(*i)).collect()),
    );
    #[allow(clippy::cast_possible_truncation)]
    b.fill(
        root,
        &[
            Slot::Int(error),
            Slot::Int(0),
            Slot::Ref(Some(done_list)),
            Slot::Int(1),
            Slot::Int(earned),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(status),
            Slot::Tagged(tag::LONG, corpse as u32),
            Slot::Tagged(tag::HIGH, (corpse >> 32) as u32),
            Slot::Ref(Some(refused_list)),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Tagged(tag::LONG, player as u32),
            Slot::Tagged(tag::HIGH, (player >> 32) as u32),
        ],
    );
    let answer =
        session::call(function::TRANSMUTE_ALL_DONE, &[Arg::Object(b.graph())]).expect("packs");
    progress::private(net, world, &answer);
    // Each item turned into money is let go of (its holder none) before it is gone (2017: 1,718
    // of the transmutes' destroyed items).
    for item in &done {
        if s.things.get(*item).is_some() {
            let left = ServerMessage::UpdateInstanceIdVisual(VisualQuality {
                object: visual(s, *item),
                quality: q::CONTAINER,
                value: 0,
            });
            net.send(world, queue::EVENT, BlobKind::VISUAL, left.encode());
        }
        if let Some(t) = s.things.remove(*item) {
            let gone = ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
                id: t.id,
                instance_stamp: t.instance,
                other_stamp: 0,
            }));
            net.send(world, queue::EVENT, BlobKind::UNORDERED, gone.encode());
        }
        if let Some(p) = s.players.get_mut(&world) {
            p.known.remove(item);
        }
    }
    let pack = reset_contents(s, world);
    progress::private(net, world, &pack);
    crate::world::keep_player(s, world);
}

/// Is an item of a template one a quest needs (its template says so)?
pub(crate) fn is_quest_item<S: Store>(s: &Server<S>, template: u32) -> bool {
    s.catalog
        .get(template)
        .is_some_and(|t| t.props.flag(QUEST_ITEM))
}

/// What an item is worth when it carries no value of its own: its template's value.
pub(crate) fn item_value<S: Store>(s: &Server<S>, template: u32) -> u32 {
    s.catalog
        .get(template)
        .and_then(|t| t.props.int("Item_Value"))
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0)
}

/// The pack's contents again: the items in slot order, the containers (none), the segments with
/// how many items each holds.
#[must_use]
pub fn reset_contents<S: Store>(s: &Server<S>, world: ConnId) -> ServerMessage {
    let held = s
        .players
        .get(&world)
        .map(|p| p.play.items.clone())
        .unwrap_or_default();
    let mut pack: Vec<&Held> = held.iter().filter(|h| h.location == 0).collect();
    pack.sort_by_key(|h| pack_order(h.slot));
    let ids: Vec<u64> = pack.iter().map(|h| h.id).collect();
    session::call(
        function::RESET_CONTENTS,
        &[
            session::segments_arg(&pack_segments(&held)),
            id_list(&[]),
            id_list(&ids),
        ],
    )
    .expect("contents pack")
}

/// Items a character comes back with whose ids the world already has (saves made when an id could be
/// given twice) are given fresh ones, so that each holder keeps its own. Returns whether any was.
pub(crate) fn fresh_ids_for_clashes(
    things: &mut crate::things::Things,
    play: &mut crate::state::PlayState,
    name: &str,
) -> bool {
    let mut any = false;
    let mut seen = std::collections::HashSet::new();
    for h in &mut play.items {
        if seen.insert(h.id) && things.get(h.id).is_none() {
            continue;
        }
        let new = things.new_id();
        tracing::warn!(
            "{name} came back with an item the world already has: {:#x} is now {new:#x}",
            h.id
        );
        if let Some(k) = play.item_states.iter().find(|k| k.id == h.id).cloned() {
            play.item_states
                .push(crate::state::ItemState { id: new, ..k });
        }
        h.id = new;
        seen.insert(new);
        any = true;
    }
    any
}

/// The items a player holds become things again as it enters, in its pack, each with an instance
/// stamp one more than when it was last kept. Returns them in slot order.
pub fn enter<S: Store>(s: &mut Server<S>, world: ConnId) -> Vec<u64> {
    let Some(p) = s.players.get(&world) else {
        return Vec::new();
    };
    let (owner, mut held) = (p.character, p.play.items.clone());
    let kept = p.play.item_states.clone();
    held.sort_by_key(|h| pack_order(h.slot));
    let mut ids = Vec::new();
    for h in held {
        let Some(t) = s.catalog.get(h.template).cloned() else {
            continue;
        };
        let mut item = Thing::of(h.id, &t, Kind::Item);
        let k = kept.iter().find(|k| k.id == h.id);
        // One more each time its holder enters (an item made last session had 0).
        item.instance = k.map_or(0, |k| k.instance).wrapping_add(1);
        item.visual = 2;
        item.position_stamp = 0;
        item.quantity = h.quantity;
        item.container = Some((owner, h.slot));
        if let Some(k) = k {
            k.restore(&mut item);
        }
        if h.location & crate::equip::BODY_PLACES != 0 {
            item.wielded = Some((owner, h.location));
        } else if h.location == crate::equip::OFF_HAND {
            item.wielded = Some((owner, crate::equip::OFF_HAND));
        } else if h.location != 0 {
            item.wielded = Some((owner, crate::equip::PRIMARY_HAND));
        }
        s.things.insert(item);
        ids.push(h.id);
    }
    ids
}

/// A player's items leave the world with it (they are kept with the player).
pub fn leave<S: Store>(s: &mut Server<S>, held: &[Held]) {
    for h in held {
        s.things.remove(h.id);
    }
}

/// Take an item of a template out of a player's pack and out of the world (a quest takes it, or
/// it is used up): the client is told it is gone from the pack and forgets it. Returns whether
/// the player held one.
pub fn take_from_pack<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    template: u32,
) -> bool {
    let Some(p) = s.players.get_mut(&world) else {
        return false;
    };
    let Some(i) = p.play.items.iter().position(|h| h.template == template) else {
        return false;
    };
    let item = p.play.items.remove(i).id;
    close_up(&mut p.play.items);
    p.known.remove(&item);
    let instance = s.things.remove(item).map_or(0, |t| t.instance);
    let gone = session::call(function::DISCRETE_DESTROY, &[Arg::LongInt(item)]).expect("packs");
    progress::private(net, world, &gone);
    let forget = ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
        id: item,
        instance_stamp: instance,
        other_stamp: 0,
    }));
    net.send(world, queue::EVENT, BlobKind::UNORDERED, forget.encode());
    crate::world::keep_player(s, world);
    true
}

/// Take a held item out of a player's pack and out of the world (used up whole): the client is
/// told it is gone from the pack and forgets it.
pub fn take_item<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    _template: u32,
) -> bool {
    // What it put on its wearer goes with it.
    let stopped = crate::effects::take_off_worn(s, net, world, item);
    crate::effects::stop_fx(s, net, world, &stopped);
    let Some(p) = s.players.get_mut(&world) else {
        return false;
    };
    let Some(i) = p.play.items.iter().position(|h| h.id == item) else {
        return false;
    };
    p.play.items.remove(i);
    close_up(&mut p.play.items);
    p.known.remove(&item);
    let instance = s.things.remove(item).map_or(0, |t| t.instance);
    let gone = session::call(function::DISCRETE_DESTROY, &[Arg::LongInt(item)]).expect("packs");
    progress::private(net, world, &gone);
    let forget = ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
        id: item,
        instance_stamp: instance,
        other_stamp: 0,
    }));
    net.send(world, queue::EVENT, BlobKind::UNORDERED, forget.encode());
    crate::world::keep_player(s, world);
    true
}

/// Does a player hold an item of a template?
#[must_use]
pub fn holds<S: Store>(s: &Server<S>, world: ConnId, template: u32) -> bool {
    s.players
        .get(&world)
        .is_some_and(|p| p.play.items.iter().any(|h| h.template == template))
}
